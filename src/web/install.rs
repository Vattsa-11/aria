use wasm_bindgen::JsCast;

fn call(name: &str) -> Option<wasm_bindgen::JsValue> {
    let window = web_sys::window()?;
    let function = js_sys::Reflect::get(&window, &name.into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    function.call0(&window).ok()
}

pub fn state() -> String {
    call("ariaInstallState").and_then(|value| value.as_string()).unwrap_or_else(|| "browser".to_string())
}

pub fn call_named(name: &str) {
    let _ = call(name);
}

pub fn call_with(name: &str, argument: &str) {
    let Some(window) = web_sys::window() else { return };
    if let Ok(function) = js_sys::Reflect::get(&window, &name.into()).and_then(|value| value.dyn_into::<js_sys::Function>().map_err(|_| wasm_bindgen::JsValue::NULL)) {
        let _ = function.call1(&window, &argument.into());
    }
}

pub fn haptic() {
    call_with("ariaHaptic", "14");
}

pub fn install() {
    let _ = call("ariaInstall");
}

pub struct StorageInfo {
    pub usage: f64,
    pub quota: f64,
    pub persisted: bool,
}

pub fn storage() -> Option<StorageInfo> {
    let window = web_sys::window()?;
    let raw = js_sys::Reflect::get(&window, &"__ariaStorage".into()).ok()?.as_string()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(StorageInfo { usage: json["usage"].as_f64()?, quota: json["quota"].as_f64()?, persisted: json["persisted"].as_bool().unwrap_or(false) })
}

pub fn request_persist() {
    let _ = call("ariaRequestPersist");
}

pub fn megabytes(bytes: f64) -> String {
    if bytes >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB", bytes / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    }
}
