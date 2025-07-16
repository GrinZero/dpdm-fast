#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;


#[cfg(feature = "bin")]
use std::path::PathBuf;
pub fn canonicalize_path(path: &str) -> String {
    PathBuf::from(path)
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[cfg(feature = "wasm")]
#[wasm_bindgen(module = "/src/nodejs/fs.js")]
extern "C" {
    #[wasm_bindgen(js_name = canonicalizePath)]
    pub fn canonicalize_path(path: &str) -> String;
}
