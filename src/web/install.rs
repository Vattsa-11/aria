use wasm_bindgen::JsCast;

fn call(name: &str) -> Option<wasm_bindgen::JsValue> {
    let window = web_sys::window()?;
    let function = js_sys::Reflect::get(&window, &name.into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    function.call0(&window).ok()
}

pub fn state() -> String {
    call("ariaInstallState").and_then(|value| value.as_string()).unwrap_or_else(|| "browser".to_string())
}

pub fn call_args(name: &str, args: &[wasm_bindgen::JsValue]) {
    let Some(window) = web_sys::window() else { return };
    let Ok(function) = js_sys::Reflect::get(&window, &name.into()).and_then(|value| value.dyn_into::<js_sys::Function>().map_err(|_| wasm_bindgen::JsValue::NULL)) else { return };
    let list = js_sys::Array::new();
    for arg in args {
        list.push(arg);
    }
    let _ = function.apply(&window, &list);
}

pub fn yt_state() -> Option<serde_json::Value> {
    serde_json::from_str(&call("ariaYTState")?.as_string()?).ok()
}

pub fn pick_file() {
    call_named("ariaPickFile");
}

pub fn take_picked() -> Option<(String, String)> {
    let raw = call("ariaTakePicked")?.as_string().filter(|text| !text.is_empty())?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some((json["name"].as_str()?.to_string(), json["text"].as_str()?.to_string()))
}

pub fn take_media() -> String {
    call("ariaTakeMedia").and_then(|value| value.as_string()).unwrap_or_default()
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

pub fn take_back() -> u32 {
    call("ariaTakeBack").and_then(|value| value.as_f64()).unwrap_or(0.0) as u32
}

pub fn sync_layers(layers: u32) {
    call_with("ariaSyncLayers", &layers.to_string());
}

pub fn arm_exit() {
    call_named("ariaArmExit");
}

pub fn update_ready() -> bool {
    call("ariaUpdateReady").and_then(|value| value.as_bool()).unwrap_or(false)
}

pub fn apply_update() {
    let _ = call("ariaApplyUpdate");
}

pub fn youtube_player_playing() -> bool {
    yt_state().and_then(|info| info["state"].as_str().map(|state| state == "playing")).unwrap_or(false)
}

pub fn keep_awake(on: bool) {
    call_args("ariaKeepAwake", &[wasm_bindgen::JsValue::from_bool(on)]);
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
