use wasm_bindgen::JsCast;

fn call(name: &str) -> Option<wasm_bindgen::JsValue> {
    let window = web_sys::window()?;
    let function = js_sys::Reflect::get(&window, &name.into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    function.call0(&window).ok()
}

pub fn state() -> String {
    call("ariaInstallState").and_then(|value| value.as_string()).unwrap_or_else(|| "browser".to_string())
}

pub fn install() {
    let _ = call("ariaInstall");
}
