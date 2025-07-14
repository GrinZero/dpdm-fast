mod node_resolve;
mod parser;
mod utils;

use clap::Parser;
use parser::parser::parse_dependency_tree;
use regex::Regex;
use serde_json::json;
use spinoff::{spinners, Color, Spinner};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::sync::{Arc, Mutex};

use parser::types::{IsModule, ParseOptions, Progress};

use crate::parser::analyze_file::analyze_file;

#[derive(Parser, Debug)]
#[clap(
    version = "1.0",
    name = "dpdm",
    about = "Analyze the files' dependencies."
)]
struct Args {
    /// The file paths or globs
    #[arg(required = true)]
    files: Vec<String>,

    #[arg(required = true, short)]
    analyze_files: Vec<String>,

    /// The context directory to shorten path, default is current directory
    #[arg(long)]
    context: Option<String>,

    /// Comma separated extensions to resolve
    #[arg(short, long, default_value = "ts,tsx,mjs,js,jsx,json")]
    extensions: String,

    /// Comma separated extensions indicate the file is js like
    #[arg(long, default_value = "ts,tsx,mjs,js,jsx")]
    js: String,

    /// Included filenames regexp in string, default includes all files
    #[arg(long, default_value = ".*")]
    include: String,

    /// Excluded filenames regexp in string, set as empty string to include all files
    #[arg(long, default_value = "node_modules")]
    exclude: String,

    /// Output json to file
    #[arg(short, long)]
    output: Option<String>,

    /// Print tree to stdout
    #[arg(long, default_value = "false")]
    no_tree: bool,

    /// Print circular to stdout
    #[arg(long, default_value = "true")]
    circular: bool,

    /// Print warning to stdout
    #[arg(long, default_value = "false")]
    no_warning: bool,

    /// The tsconfig path, which is used for resolve path alias
    #[arg(long)]
    tsconfig: Option<String>,

    /// Transform typescript modules to javascript before analyze
    #[arg(short = 'T', long, default_value = "false")]
    transform: bool,

    /// Exit with specified code
    #[arg(long)]
    exit_code: Option<String>,

    /// Show progress bar
    #[arg(long, default_value = "true")]
    no_progress: bool,

    /// This file is a glob, used for finding unused files
    #[arg(long)]
    detect_unused_files_from: Option<String>,
    /// Skip parse import(...) statement
    #[arg(long)]
    skip_dynamic_imports: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // 处理参数
    let files = &args.files;

    if files.is_empty() {
        eprintln!("\nMissing entry file");
        std::process::exit(1);
    }

    let exit_cases: HashSet<&str> = ["circular"].iter().cloned().collect();
    let mut exit_codes: Vec<(String, i32)> = Vec::new();

    if let Some(exit_code_str) = &args.exit_code {
        for c in exit_code_str.split(',') {
            let parts: Vec<&str> = c.split(':').collect();
            if parts.len() != 2 {
                eprintln!("Invalid exit code format");
                std::process::exit(1);
            }
            let label = parts[0];
            let code: i32 = parts[1].parse().unwrap_or_else(|_| {
                eprintln!("exit code should be a number");
                std::process::exit(1);
            });

            if !exit_cases.contains(label) {
                eprintln!("unsupported exit case \"{}\"", label);
                std::process::exit(1);
            }
            exit_codes.push((label.to_string(), code));
        }
    }

    let spinner = Arc::new(Mutex::new(Spinner::new(
        spinners::Dots,
        "Start analyzing dependencies...",
        Color::Green,
    )));

    let context: String = args.context.as_ref().map(|s| s.clone()).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    });

    let no_progress = &args.no_progress;

    let progress = Progress {
        total: Arc::new(Mutex::new(0)),
        current: Arc::new(Mutex::new(String::new())),
        ended: Arc::new(Mutex::new(0)),
        spinner,
    };

    let mut extensions: Vec<String> = args
        .extensions
        .split(',')
        .map(|s| format!(".{}", s))
        .collect();
    extensions.insert(0, String::from(""));
    let options = ParseOptions {
        context,
        extensions,
        js: args
            .js
            .split(',')
            .map(String::from)
            .map(|s| format!(".{}", s))
            .collect(),
        include: Regex::new(&args.include).unwrap_or_else(|_| Regex::new(".*").unwrap()),
        exclude: Regex::new(&args.exclude).unwrap_or_else(|_| Regex::new("$").unwrap()),
        tsconfig: args.tsconfig.clone(),
        transform: args.transform,
        skip_dynamic_imports: args.skip_dynamic_imports.as_deref() == Some("tree"),
        is_module: IsModule::Unknown,
        progress: match no_progress {
            true => {
                progress.spinner.lock().unwrap().stop();
                None
            }
            false => Some(progress),
        },
    };

    let dependency_tree = parse_dependency_tree(&files, &options).await;

    let analyze_files = args.analyze_files;
    let mut all_result: HashMap<String, Vec<String>> = HashMap::new();
    for file in analyze_files {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        analyze_file(&file, &dependency_tree, &mut visited, &mut result);
        result.sort();
        result.dedup();

        all_result.insert(file, result);
    }

    // 把 dependency_tree 写入 output
    let output = args.output.clone();
    if output.is_some() {
        let file = File::create(output.unwrap()).expect("Failed to create file");
        let data = json!({
            "result": all_result,
        });
        serde_json::to_writer_pretty(file, &data).expect("Failed to write JSON");
    }

    if utils::tree::is_empty(&dependency_tree) {
        println!("\nNo entry files were matched.");
        std::process::exit(1);
    }

    println!("Analyze done!");
}
