use std::time::Duration;

pub struct WebOs;

impl rsqlite_vfs::OsCallback for WebOs {
    fn sleep(duration: Duration) {
        let until = js_sys::Date::now() + duration.as_secs_f64() * 1000.0;
        while js_sys::Date::now() < until {}
    }

    fn random(buffer: &mut [u8]) {
        for byte in buffer.iter_mut() {
            *byte = (js_sys::Math::random() * 256.0) as u8;
        }
    }

    fn epoch_timestamp_in_ms() -> i64 {
        js_sys::Date::now() as i64
    }
}
