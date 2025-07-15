use crate::parser::consts::DependencyKind;
use regex::Regex;
use serde::{self, Deserialize, Deserializer, Serializer};
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

pub fn deserialize_regex<'de, D>(deserializer: D) -> Result<Regex, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Regex::new(&s).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
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

#[derive(Debug, serde::Serialize, Deserialize, Clone)]
pub struct ParseOptions {
    pub context: String,
    pub extensions: Vec<String>,
    pub js: Vec<String>,
    #[serde(
        serialize_with = "serialize_regex",
        deserialize_with = "deserialize_regex"
    )]
    pub include: Regex,
    #[serde(
        serialize_with = "serialize_regex",
        deserialize_with = "deserialize_regex"
    )]
    pub exclude: Regex,
    pub tsconfig: Option<String>,
    #[serde(skip)]
    pub progress: Option<Progress>,
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
pub type DependencyTree = HashMap<String, Arc<Option<Vec<Dependency>>>>;

#[derive(Debug, Clone)]
pub struct Alias {
    pub root: PathBuf,
    pub paths: HashMap<String, Vec<String>>,
}
