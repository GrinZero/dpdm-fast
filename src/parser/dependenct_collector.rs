use crate::parser::types::{ExportSymbol, ImportSymbol};

use super::consts::DependencyKind;
use super::types::Dependency;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use swc_core::ecma::ast::{
    Callee, Decl, ExportDecl, Ident, ImportSpecifier, ModuleExportName, Pat,
};
use swc_core::ecma::utils::swc_ecma_ast;
use swc_core::ecma::visit::{Visit, VisitWith};

pub struct DependencyCollector {
    pub path: PathBuf,
    pub dependencies: Vec<Dependency>,
    pub id: String,
    pub skip_dynamic_imports: bool,
    pub collect_symbol: bool,
    pub imports: Vec<ImportSymbol>,
    pub exports: Vec<ExportSymbol>,
    pub local_symbol_map: HashMap<String, Vec<String>>,
    pub next_import_id: usize, // pub local_dynamic_import_ids: Vec<usize>
    pub dynamic_import_expr_to_id_map: HashMap<String, usize>,
}

impl DependencyCollector {
    fn collect_dynamic_import_ids(&mut self, expr: &swc_ecma_ast::Expr) -> Vec<usize> {
        struct ImportIdCollector<'a> {
            found_ids: Vec<usize>,
            collector: &'a mut DependencyCollector,
        }
        impl<'a> Visit for ImportIdCollector<'a> {
            fn visit_call_expr(&mut self, call_expr: &swc_ecma_ast::CallExpr) {
                if let Callee::Import(_) = &call_expr.callee {
                    if let Some(arg) = call_expr.args.get(0) {
                        if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(ref s)) = *arg.expr {
                            let request = s.value.to_string();
                            if let Some(id) =
                                self.collector.dynamic_import_expr_to_id_map.get(&request)
                            {
                                self.found_ids.push(*id);
                            } else {
                                let new_id = self.collector.next_import_id;
                                self.collector.next_import_id += 1;

                                self.collector
                                    .dynamic_import_expr_to_id_map
                                    .insert(request.clone(), new_id);

                                // 同时也要补一条 ImportSymbol，否则它只是个映射，没有真正的 ImportSymbol 记录
                                let symbol = ImportSymbol {
                                    id: new_id.clone(),
                                    local: format!("__dynamic_import_{}", new_id),
                                    imported: "*".to_string(),
                                    source: request.clone(),
                                };
                                self.collector.imports.push(symbol);
                                self.found_ids.push(new_id);
                            }
                        }
                    }
                }
                call_expr.visit_children_with(self);
            }
        }

        let mut collector = ImportIdCollector {
            found_ids: vec![],
            collector: self,
        };
        expr.visit_with(&mut collector);
        collector.found_ids
    }

    fn add_import(&mut self, local: String, imported: String, source: String) {
        let symbol = ImportSymbol {
            id: self.next_import_id,
            local,
            imported,
            source,
        };
        self.next_import_id += 1;
        self.imports.push(symbol);
    }

    /// 匹配某个本地符号名对应的 import id（可多个）
    fn find_import_ids(&self, local: &str) -> Vec<usize> {
        self.imports
            .iter()
            .filter(|imp| imp.local == local)
            .map(|imp| imp.id)
            .collect()
    }

    fn find_unique_import_ids_recursive(&self, roots: &[String]) -> Vec<usize> {
        let mut visited = HashSet::new();
        let mut stack: Vec<String> = roots.to_vec();

        let mut result = HashSet::new();

        while let Some(sym) = stack.pop() {
            if visited.insert(sym.clone()) {
                // 是否本地再绑定
                if let Some(deps) = self.local_symbol_map.get(&sym) {
                    for dep in deps {
                        stack.push(dep.clone());
                    }
                } else {
                    // 如果不是本地再绑定，看是否是 import
                    for id in self.find_import_ids(&sym) {
                        result.insert(id);
                    }
                }
            }
        }

        // for id in &self.local_dynamic_import_ids {
        //     result.insert(*id);
        // }

        result.into_iter().collect()
    }
}

pub struct IdentCollector<'a> {
    pub idents: &'a mut Vec<String>,
}

impl<'a> Visit for IdentCollector<'a> {
    fn visit_ident(&mut self, ident: &Ident) {
        self.idents.push(ident.sym.to_string());
    }
}

impl Visit for DependencyCollector {
    fn visit_import_decl(&mut self, import: &swc_ecma_ast::ImportDecl) {
        // 处理静态导入
        let request = import.src.value.to_string();
        let dependency = Dependency {
            issuer: self.path.to_string_lossy().to_string(),
            request: request.clone(),
            kind: DependencyKind::StaticImport,
            id: Some(self.id.clone()),
        };
        self.dependencies.push(dependency);

        if !self.collect_symbol {
            return;
        }

        for specifier in &import.specifiers {
            match specifier {
                ImportSpecifier::Named(named) => {
                    let local = named.local.sym.to_string();
                    let imported = match &named.imported {
                        Some(ModuleExportName::Ident(ident)) => ident.sym.to_string(),
                        Some(ModuleExportName::Str(s)) => s.value.to_string(),
                        None => local.clone(),
                    };
                    self.add_import(local, imported, request.clone());
                }
                ImportSpecifier::Default(default) => {
                    self.add_import(
                        default.local.sym.to_string(),
                        "default".to_string(),
                        request.clone(),
                    );
                }
                ImportSpecifier::Namespace(ns) => {
                    self.add_import(ns.local.sym.to_string(), "*".to_string(), request.clone());
                }
            }
        }
    }

    fn visit_call_expr(&mut self, expr: &swc_ecma_ast::CallExpr) {
        if let Callee::Import(_) = &expr.callee {
            if let Some(arg) = expr.args.get(0) {
                if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(ref s)) = *arg.expr {
                    let request = s.value.to_string();
                    let dependency = Dependency {
                        issuer: self.path.to_string_lossy().to_string(),
                        request: request.clone(),
                        kind: DependencyKind::DynamicImport,
                        id: Some(self.id.clone()),
                    };
                    if !self.skip_dynamic_imports {
                        self.dependencies.push(dependency);
                    }

                    let id = if let Some(existing_id) =
                        self.dynamic_import_expr_to_id_map.get(&request)
                    {
                        *existing_id
                    } else {
                        let new_id = self.next_import_id;
                        self.next_import_id += 1;
                        self.dynamic_import_expr_to_id_map
                            .insert(request.clone(), new_id);
                        new_id
                    };

                    let exists = self.imports.iter().any(|imp| imp.id == id);

                    if !exists {
                        let symbol = ImportSymbol {
                            id,
                            local: format!("__dynamic_import_{}", id),
                            imported: "*".to_string(),
                            source: request.clone(),
                        };
                        self.imports.push(symbol);
                    }
                }
            }
        }

        if let swc_ecma_ast::Callee::Expr(ref callee_expr) = expr.callee {
            // 处理 CommonJS 导入
            if let swc_ecma_ast::Expr::Ident(ref ident) = &**callee_expr {
                if ident.sym == *"require" {
                    if let Some(arg) = expr.args.get(0) {
                        if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(ref s)) = *arg.expr {
                            let request = s.value.to_string();
                            let dependency = Dependency {
                                issuer: self.path.to_string_lossy().to_string(),
                                request,
                                kind: DependencyKind::CommonJS,
                                id: Some(self.id.clone()),
                            };
                            self.dependencies.push(dependency);
                        }
                    }
                }
            }
        }
        expr.visit_children_with(self);
    }

    fn visit_export_all(&mut self, node: &swc_ecma_ast::ExportAll) {
        let request = node.src.value.to_string();
        let dependency = Dependency {
            issuer: self.path.to_string_lossy().to_string(),
            request: request.clone(),
            kind: DependencyKind::StaticExport,
            id: Some(self.id.clone()),
        };
        self.dependencies.push(dependency);

        if self.collect_symbol {
            self.exports.push(ExportSymbol {
                local: "*".to_string(),
                exported: "*".to_string(),
                reexport_source: Some(request),
                depends_on: vec![],
            });
        }
        node.visit_children_with(self);
    }

    fn visit_var_decl(&mut self, var: &swc_ecma_ast::VarDecl) {
        if !self.collect_symbol {
            return var.visit_children_with(self);
        }

        for decl in &var.decls {
            let local = match &decl.name {
                Pat::Ident(binding) => binding.id.sym.to_string(),
                Pat::Object(_) | Pat::Array(_) => "<destructured>".to_string(),
                _ => "<unknown>".to_string(),
            };

            let mut used_idents = vec![];
            if let Some(init) = &decl.init {
                let mut collector = IdentCollector {
                    idents: &mut used_idents,
                };
                init.visit_with(&mut collector);
            }

            self.local_symbol_map.insert(local.clone(), used_idents);
        }
        var.visit_children_with(self);
    }

    fn visit_named_export(&mut self, export: &swc_ecma_ast::NamedExport) {
        if !self.collect_symbol {
            return export.visit_children_with(self);
        }

        // export { foo as bar } from './mod'
        let request = export.src.as_ref().map(|src| src.value.to_string());

        for specifier in &export.specifiers {
            match specifier {
                swc_ecma_ast::ExportSpecifier::Named(named) => {
                    let local = match &named.orig {
                        swc_ecma_ast::ModuleExportName::Ident(ident) => ident.sym.to_string(),
                        swc_ecma_ast::ModuleExportName::Str(s) => s.value.to_string(),
                    };
                    let exported = match &named.exported {
                        Some(ModuleExportName::Ident(ident)) => ident.sym.to_string(),
                        Some(ModuleExportName::Str(s)) => s.value.to_string(),
                        None => local.clone(),
                    };

                    let depends_on = if request.is_some() {
                        // 有 from，就是 re-export，不关联本地 import
                        vec![]
                    } else {
                        self.find_import_ids(&local)
                    };

                    self.exports.push(ExportSymbol {
                        local: local.clone(),
                        exported,
                        reexport_source: request.clone(),
                        depends_on,
                    });

                    if let Some(req) = &request {
                        let dependency = Dependency {
                            issuer: self.path.to_string_lossy().to_string(),
                            request: req.clone(),
                            kind: DependencyKind::StaticExport,
                            id: Some(self.id.clone()),
                        };
                        self.dependencies.push(dependency);
                    }
                }
                swc_ecma_ast::ExportSpecifier::Default(_) => {
                    // 不太常见，忽略
                }
                swc_ecma_ast::ExportSpecifier::Namespace(ns) => {
                    // export * as ns from './mod'
                    let exported = match &ns.name {
                        swc_ecma_ast::ModuleExportName::Ident(ident) => ident.sym.to_string(),
                        swc_ecma_ast::ModuleExportName::Str(s) => s.value.to_string(),
                    };
                    self.exports.push(ExportSymbol {
                        local: "*".to_string(),
                        exported,
                        reexport_source: request.clone(),
                        depends_on: vec![],
                    });

                    if let Some(req) = &request {
                        let dependency = Dependency {
                            issuer: self.path.to_string_lossy().to_string(),
                            request: req.clone(),
                            kind: DependencyKind::StaticExport,
                            id: Some(self.id.clone()),
                        };
                        self.dependencies.push(dependency);
                    }
                }
            }
        }
        export.visit_children_with(self);
    }

    fn visit_export_decl(&mut self, export: &ExportDecl) {
        if !self.collect_symbol {
            return export.visit_children_with(self);
        }

        match &export.decl {
            Decl::Class(class) => {
                let local = class.ident.sym.to_string();
                let mut used_idents = vec![];
                let mut collector = IdentCollector {
                    idents: &mut used_idents,
                };
                class.visit_with(&mut collector);
                self.local_symbol_map
                    .insert(local.clone(), used_idents.clone());

                let depends_on = self.find_unique_import_ids_recursive(&used_idents);

                self.exports.push(ExportSymbol {
                    local: local.clone(),
                    exported: local,
                    reexport_source: None,
                    depends_on,
                });
            }
            Decl::Fn(func) => {
                let local = func.ident.sym.to_string();
                // 对函数体做 usage 收集
                let mut used_idents = vec![];
                let mut collector = IdentCollector {
                    idents: &mut used_idents,
                };
                func.visit_with(&mut collector);
                self.local_symbol_map
                    .insert(local.clone(), used_idents.clone());

                let depends_on = self.find_unique_import_ids_recursive(&used_idents);

                self.exports.push(ExportSymbol {
                    local: local.clone(),
                    exported: local,
                    reexport_source: None,
                    depends_on,
                });
            }
            Decl::Var(var) => {
                for decl in &var.decls {
                    let local = match &decl.name {
                        Pat::Ident(binding) => binding.id.sym.to_string(),
                        Pat::Object(_) | Pat::Array(_) => "<destructured>".to_string(),
                        _ => "<unknown>".to_string(),
                    };

                    let mut used_idents = vec![];
                    if let Some(init) = &decl.init {
                        let mut collector = IdentCollector {
                            idents: &mut used_idents,
                        };
                        init.visit_with(&mut collector);
                    }

                    self.local_symbol_map
                        .insert(local.clone(), used_idents.clone());
                    let mut depends_on = self.find_unique_import_ids_recursive(&[local.clone()]);

                    // 正确在这里拼 dynamic import
                    if let Some(init) = &decl.init {
                        depends_on.extend(self.collect_dynamic_import_ids(init));
                    }
                    self.exports.push(ExportSymbol {
                        local: local.clone(),
                        exported: local,
                        reexport_source: None,
                        depends_on,
                    });
                }
            }
            _ => {}
        }

        export.visit_children_with(self);
    }

    fn visit_export_default_decl(&mut self, export: &swc_ecma_ast::ExportDefaultDecl) {
        if !self.collect_symbol {
            return export.visit_children_with(self);
        }
        let local = match &export.decl {
            swc_ecma_ast::DefaultDecl::Class(class) => class
                .ident
                .as_ref()
                .map(|id| id.sym.to_string())
                .unwrap_or("default".to_string()),
            swc_ecma_ast::DefaultDecl::Fn(func) => func
                .ident
                .as_ref()
                .map(|id| id.sym.to_string())
                .unwrap_or("default".to_string()),
            swc_ecma_ast::DefaultDecl::TsInterfaceDecl(_) => "default".to_string(),
        };

        let mut used_idents = vec![];
        export.decl.visit_with(&mut IdentCollector {
            idents: &mut used_idents,
        });
        self.local_symbol_map
            .insert(local.clone(), used_idents.clone());

        let depends_on = self.find_unique_import_ids_recursive(&used_idents);

        self.exports.push(ExportSymbol {
            local: local.clone(),
            exported: "default".to_string(),
            reexport_source: None,
            depends_on,
        });
        export.visit_children_with(self);
    }

    fn visit_export_default_expr(&mut self, export: &swc_ecma_ast::ExportDefaultExpr) {
        if !self.collect_symbol {
            return;
        }

        let mut used_idents = vec![];
        export.expr.visit_with(&mut IdentCollector {
            idents: &mut used_idents,
        });
        let depends_on = self.find_unique_import_ids_recursive(&used_idents);

        self.exports.push(ExportSymbol {
            local: "default".to_string(),
            exported: "default".to_string(),
            reexport_source: None,
            depends_on,
        });

        export.visit_children_with(self);
    }
}
