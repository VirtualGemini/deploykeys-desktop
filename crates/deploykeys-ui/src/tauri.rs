//! Thin wrapper over Tauri's JS `invoke` bridge.
//!
//! Tauri injects `window.__TAURI__.core.invoke` into the webview. We bind to it
//! directly rather than pulling in a wrapper crate, so the UI builds as plain
//! CSR wasm with no Tauri Rust dependency.

use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn invoke_js(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Invoke a Tauri command with no arguments and deserialize its result.
pub async fn invoke_no_args<R: DeserializeOwned>(cmd: &str) -> Result<R, String> {
    invoke(cmd, &EmptyArgs).await
}

/// Invoke a Tauri command, serializing `args` to JS and deserializing the result.
pub async fn invoke<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, String> {
    let args = serde_wasm_bindgen::to_value(args).map_err(|e| e.to_string())?;
    match invoke_js(cmd, args).await {
        Ok(value) => serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string()),
        Err(err) => Err(error_to_string(err)),
    }
}

/// Tauri rejects with the command's error string (we return `Result<_, String>`
/// from every command), which arrives as a JS string.
fn error_to_string(err: JsValue) -> String {
    err.as_string()
        .or_else(|| {
            js_sys::JSON::stringify(&err)
                .ok()
                .and_then(|s| s.as_string())
        })
        .unwrap_or_else(|| "Unknown error from backend".to_string())
}

#[derive(Serialize)]
struct EmptyArgs;
