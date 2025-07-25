use super::dependenct_collector::DependencyCollector;
use super::types::{Alias, Dependency, IsModule, ParseOptions};
use crate::parser::strip_type_only_imports::StripTypeOnlyImports;
use crate::parser::types::{DependencyTree, ExportSymbol, ImportSymbol, SymbolNode, SymbolTree};
use crate::utils::resolver::simple_resolver;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use swc_core::common::{sync::Lrc, FileName, Mark, SourceMap};
use swc_core::common::{Globals, GLOBALS};
use swc_core::ecma::ast::{EsVersion, Program};
use swc_core::ecma::parser::Lexer;
use swc_core::ecma::parser::{Parser, StringInput, Syntax, TsSyntax};
use swc_core::ecma::transforms::base::resolver;
use swc_core::ecma::transforms::typescript::strip_type;
use swc_core::ecma::utils::swc_common;
use swc_core::ecma::visit::{VisitMutWith, VisitWith};

lazy_static! {
    static ref CACHE: Mutex<HashMap<String, Arc<Option<Vec<Dependency>>>>> =
        Mutex::new(HashMap::new());
}

pub async fn parse_tree_recursive(
    context: PathBuf,
    path: PathBuf,
    output: Arc<Mutex<DependencyTree>>,
    symbol_output: Arc<Mutex<SymbolTree>>,
    cm: Arc<Lrc<SourceMap>>,    // 将 Lrc<SourceMap> 包装在 Arc 中
    options: Arc<ParseOptions>, // 将 ParseOptions 包装在 Arc 中
    alias: Option<Arc<Alias>>,
) -> Option<String> {
    let id: Option<String> = match simple_resolver(
        &context.to_string_lossy().to_string(),
        &path.to_string_lossy().to_string(),
        &options.extensions,
        alias.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            eprintln!("{:?}", e);
            return None;
        }
    };

    let id: String = match id {
        Some(id) => {
            let output_lock = output.lock().unwrap();
            if output_lock.contains_key(&id) {
                return Some(id);
            }
            id
        }
        None => {
            return None;
        }
    };

    // 检查缓存
    {
        let cache = CACHE.lock().unwrap();
        if let Some(cached_result) = cache.get(&id) {
            let mut output_lock = output.lock().unwrap();
            output_lock.insert(id.clone(), Arc::clone(cached_result));
            return Some(id.clone());
        }
    }

    if !options.include.is_match(&id) || options.exclude.is_match(&id) {
        let mut output_lock = output.lock().unwrap();
        output_lock.insert(id.clone(), Arc::new(None));
        return Some(id.clone());
    }

    match Path::new(&id).extension() {
        Some(ext) => {
            let ext: String = if ext.to_string_lossy().to_string() == "" {
                String::new()
            } else {
                format!(".{}", ext.to_string_lossy().to_string())
            };
            if !options.js.contains(&ext) {
                let mut output_lock = output.lock().unwrap();
                output_lock.insert(id.clone(), Arc::new(Some(Vec::new())));
                return Some(id.clone());
            }
        }
        None => {
            let mut output_lock = output.lock().unwrap();
            output_lock.insert(id.clone(), Arc::new(Some(Vec::new())));
            return Some(id.clone());
        }
    }

    if let Some(progress) = &options.progress {
        {
            let mut total = progress.total.lock().unwrap();
            *total += 1;
        }
        {
            let mut current = progress.current.lock().unwrap();
            *current = id.clone();
        }
        {
            let mut spinner = progress.spinner.lock().unwrap();
            let text = format!(
                "[{}/{}] Analyzing {}...",
                *progress.ended.lock().unwrap(),
                *progress.total.lock().unwrap(),
                *progress.current.lock().unwrap()
            );
            spinner.update_text(text);
        }
    }
    let file_content = fs::read_to_string(&id).expect("Unable to read file");

    let id_path: PathBuf = Path::new(&id).to_path_buf();

    // 使用 swc 解析代码
    let fm: Lrc<swc_common::SourceFile> =
        cm.new_source_file(FileName::Real(id_path.clone()).into(), file_content);
    let lexer = Lexer::new(
        Syntax::Typescript(TsSyntax {
            tsx: true,
            decorators: false,
            ..Default::default()
        }),
        EsVersion::EsNext,
        StringInput::from(&*fm),
        None,
    );

    let mut parser: Parser<Lexer<'_>> = Parser::new_from(lexer);
    let program_result = match options.is_module {
        IsModule::Bool(true) => parser.parse_module().map(Program::Module),
        IsModule::Bool(false) => parser.parse_script().map(Program::Script),
        IsModule::Unknown => parser.parse_program(),
    };

    let mut program: Program = match program_result {
        Ok(program) => program,
        Err(_err) => {
            // eprintln!("Failed to parse program: {:?}", err);
            return None;
        }
    };

    program = match options.transform {
        true => match id.ends_with(".tsx") || id.ends_with(".ts") {
            true => {
                let program = GLOBALS.set(&Globals::new(), || {
                    let unresolved_mark = Mark::new();
                    let top_level_mark = Mark::new();

                    program.visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, true));
                    program.visit_mut_with(&mut StripTypeOnlyImports);
                    program.visit_mut_with(&mut strip_type());

                    program
                });
                program
            }
            false => program,
        },
        false => program,
    };

    let new_context: PathBuf = Path::new(&id).parent().unwrap().to_path_buf();

    let dependencies: Vec<Dependency> = Vec::new();
    {
        let mut output_lock = output.lock().unwrap();
        output_lock.insert(id.clone(), Arc::new(Some(Vec::new())));
    }

    let exports: Vec<ExportSymbol> = Vec::new();
    let imports: Vec<ImportSymbol> = Vec::new();
    // 创建一个依赖收集器
    let mut collector: DependencyCollector = DependencyCollector {
        id,
        path: path.clone(),
        dependencies,
        collect_symbol: options.symbol,
        skip_dynamic_imports: options.skip_dynamic_imports,
        exports,
        imports,
        next_import_id: 0,
        local_symbol_map: HashMap::new(),
        // local_dynamic_import_ids: vec![]
        dynamic_import_expr_to_id_map: HashMap::new(),
    };

    // 遍历 AST
    program.visit_with(&mut collector);

    {
        let symbol_node = SymbolNode {
            exports: collector.exports,
            imports: collector.imports,
        };
        let mut symbol_tree_lock = symbol_output.lock().unwrap();
        symbol_tree_lock.insert(collector.id.clone(), Arc::new(Some(symbol_node)));
    }

    let mut deps: Vec<_> = Vec::new();
    for dep in &collector.dependencies {
        let path: PathBuf = PathBuf::from(dep.request.clone());
        let new_context: PathBuf = new_context.clone();
        let output_clone = Arc::clone(&output);
        let symbol_output_clone = Arc::clone(&symbol_output);
        let cm_clone = Arc::clone(&cm);
        let options_clone = Arc::clone(&options);
        let alias_clone = alias.clone();
        let dep_future = async move {
            Box::pin(parse_tree_recursive(
                new_context,
                path,
                output_clone,
                symbol_output_clone,
                cm_clone,
                options_clone,
                alias_clone,
            ))
        };
        deps.push(dep_future);
    }

    let results = futures::future::join_all(deps).await;
    for (i, dep) in results.into_iter().enumerate() {
        collector.dependencies[i].id = dep.await;
    }

    collector.dependencies.retain(|dep| {
        if let Some(ref id) = dep.id {
            !id.contains("node_modules")
        } else {
            true
        }
    });

    // 将收集到的依赖存储到输出和缓存中
    let dependencies = Arc::new(Some(collector.dependencies));
    {
        let mut output_lock = output.lock().unwrap();
        output_lock.insert(collector.id.clone(), dependencies.clone());
    }
    {
        let mut cache = CACHE.lock().unwrap();
        cache.insert(collector.id.clone(), dependencies.clone());
    }

    if let Some(progress) = &options.progress {
        {
            let mut ended = progress.ended.lock().unwrap();
            *ended += 1;
        }
        {
            let mut spinner = progress.spinner.lock().unwrap();
            let text = format!(
                "[{}/{}] Analyzing {}...",
                *progress.ended.lock().unwrap(),
                *progress.total.lock().unwrap(),
                *progress.current.lock().unwrap()
            );
            spinner.update_text(text);
        }
    }
    Some(collector.id.clone())
}
