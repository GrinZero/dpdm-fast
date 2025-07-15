#[cfg(feature = "wasm")]
mod node_resolve;
mod parser;
mod utils;
use js_sys::Promise;
use parser::parser::parse_dependency_tree;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn parse_tree(entries_json: String, base_options_json: String) -> Promise {
    future_to_promise(async move {
        // 反序列化参数

        use crate::parser::types::ParseOptions;
        let entries: Vec<String> = serde_json::from_str(&entries_json)
            .map_err(|e| JsValue::from_str(&format!("entries json parse error: {:?}", e)))?;

        let base_options: ParseOptions = serde_json::from_str(&base_options_json)
            .map_err(|e| JsValue::from_str(&format!("base_options json parse error: {:?}", e)))?;

        // 调用原异步函数
        let tree = parse_dependency_tree(&entries, &base_options).await;

        // 序列化输出
        let res_json = serde_json::to_string(&tree)
            .map_err(|e| JsValue::from_str(&format!("result serialize error: {:?}", e)))?;

        Ok(JsValue::from_str(&res_json))
    })
}
