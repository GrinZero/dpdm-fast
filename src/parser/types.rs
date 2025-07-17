use crate::parser::consts::DependencyKind;
use regex::Regex;
use serde::{self, Serializer};
use spinoff::Spinner;
use std::{
    collections::HashMap,
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

fn serialize_regex<S>(regex: &Regex, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&regex.to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum IsModule {
    Bool(bool),
    Unknown,
}

#[derive(Clone)]
pub struct Progress {
    pub total: Arc<Mutex<i32>>,
    pub current: Arc<Mutex<String>>,
    pub ended: Arc<Mutex<i32>>,
    pub spinner: Arc<Mutex<Spinner>>,
}

impl fmt::Debug for Progress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Progress")
            .field("total", &self.total)
            .field("current", &self.current)
            .field("ended", &self.ended)
            .field("spinner", &"Spinner") // 手动调用 DebugSpinner 的 fmt
            .finish()
    }
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct ParseOptions {
    pub context: String,
    pub extensions: Vec<String>,
    pub js: Vec<String>,
    #[serde(serialize_with = "serialize_regex")]
    pub include: Regex,
    #[serde(serialize_with = "serialize_regex")]
    pub exclude: Regex,
    pub tsconfig: Option<String>,
    #[serde(skip)]
    pub progress: Option<Progress>,

    pub symbol: bool,
    pub transform: bool,
    pub skip_dynamic_imports: bool,
    pub is_module: IsModule, // 是否是 ESM 模块
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct Dependency {
    pub issuer: String,
    pub request: String,
    pub kind: DependencyKind,
    pub id: Option<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct ImportSymbol {
    pub local: String,    // 本地变量名
    pub imported: String, // 从外部导入的符号名（对于 default/namespace 特别标记）
    pub source: String,   // 来源模块
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct ExportSymbol {
    pub local: String,                   // 本地符号名
    pub exported: String,                // 导出的符号名
    pub reexport_source: Option<String>, // 如果是 re-export，标记源模块
}

pub type DependencyTree = HashMap<String, Arc<Option<Vec<Dependency>>>>;

#[derive(Debug, serde::Serialize, Clone)]
pub struct SymbolNode {
    pub exports: Vec<ExportSymbol>,
    pub imports: Vec<ImportSymbol>,
}
pub type SymbolTree = HashMap<String, Arc<Option<SymbolNode>>>;

#[derive(Debug, Clone)]
pub struct Alias {
    pub root: PathBuf,
    pub paths: HashMap<String, Vec<String>>,
}
