//! Thin wrapper over Tauri's JS `invoke` bridge.
//!
//! Tauri injects `window.__TAURI__.core.invoke` into the webview. We bind to it
//! directly rather than pulling in a wrapper crate, so the UI builds as plain
//! CSR wasm with no Tauri Rust dependency.

use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn invoke_js(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    /// Listen for events emitted from the Tauri backend.
    #[wasm_bindgen(
        js_namespace = ["window", "__TAURI__", "event"],
        js_name = listen,
        catch
    )]
    async fn listen_js(
        event: &str,
        handler: &Closure<dyn FnMut(JsValue)>,
    ) -> Result<JsValue, JsValue>;
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

/// Event payload emitted by the backend progress reporter.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProgressEvent {
    pub operation: String,
    pub percent: u8,
}

/// Listen to Tauri backend events. The returned cleanup function must be called
/// to unlisten when the listener is no longer needed.
pub fn listen_progress<F>(on_event: F) -> Result<impl FnOnce(), String>
where
    F: FnMut(ProgressEvent) + 'static,
{
    listen("progress", on_event)
}

/// Listen to an arbitrary Tauri event.
pub fn listen<F>(event: &str, mut on_event: F) -> Result<impl FnOnce(), String>
where
    F: FnMut(ProgressEvent) + 'static,
{
    let closure = Closure::new(move |value: JsValue| {
        if let Ok(ev) = serde_wasm_bindgen::from_value::<ProgressEvent>(value) {
            on_event(ev);
        }
    });

    let event = event.to_string();
    spawn_local(async move {
        let _ = listen_js(&event, &closure).await;
        // Keep the closure alive for as long as the listener is active.
        closure.forget();
    });

    // Tauri's listen_js returns an unlisten function, but we cannot await it
    // synchronously here. For now we return a no-op cleanup; the listener lives
    // for the lifetime of the page.
    Ok(|| {})
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
