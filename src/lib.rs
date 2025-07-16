#[cfg(feature = "wasm")]
mod node_resolve;
mod parser;
mod utils;
use js_sys::Promise;
use parser::parser::parse_dependency_tree;
use serde_wasm_bindgen::{from_value,to_value};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn parse_tree(entries: JsValue, base_options: JsValue) -> Promise {
    console_error_panic_hook::set_once();
    future_to_promise(async move {
        use crate::parser::types::{ParseOptions, ParseOptionsInput};
        let entries_vec: Vec<String> = from_value(entries)
            .map_err(|e| JsValue::from_str(&format!("entries parse error: {:?}", e)))?;

        let raw_options: ParseOptionsInput = from_value(base_options)
            .map_err(|e| JsValue::from_str(&format!("options parse error: {:?}", e)))?;

        let options = ParseOptions {
            context: raw_options.context,
            extensions: raw_options.extensions,
            js: raw_options.js,
            tsconfig: raw_options.tsconfig,
            transform: raw_options.transform,
            skip_dynamic_imports: raw_options.skip_dynamic_imports,
            include: raw_options.include,
            exclude: raw_options.exclude,
            is_module: raw_options.is_module,
            progress: None, // 这里浏览器无 terminal spinner
        };

        // 调用原异步函数
        let tree = parse_dependency_tree(&entries_vec, &options).await;

        // 序列化输出
        to_value(&tree).map_err(|e| JsValue::from_str(&format!("result serialize error: {:?}", e)))
    })
}
