#[cfg(any(target_arch = "wasm32", test))]
pub fn is_touch(platform: &str) -> bool {
    matches!(platform, "android" | "ios")
}

#[cfg(target_arch = "wasm32")]
pub fn touch_device() -> bool {
    web_sys::window()
        .and_then(|window| js_sys::Reflect::get(&window, &"__ariaPlatform".into()).ok())
        .and_then(|value| value.as_string())
        .is_some_and(|platform| is_touch(&platform))
}

#[cfg(target_os = "android")]
pub fn touch_device() -> bool {
    true
}

#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
pub fn touch_device() -> bool {
    false
}

pub fn native_engine() -> bool {
    cfg!(not(target_arch = "wasm32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_phones_and_tablets_count_as_touch_devices() {
        assert!(is_touch("android"));
        assert!(is_touch("ios"));
        assert!(!is_touch("pc"));
        assert!(!is_touch(""));
    }
}
