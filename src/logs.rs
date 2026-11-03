use std::io::Write;
use std::path::Path;

const MAX_BYTES: u64 = 1024 * 1024;

pub fn log_path() -> std::path::PathBuf {
    crate::settings::data_dir().join("aria.log")
}

pub fn log(level: &str, message: &str) {
    log_to(&crate::settings::data_dir(), level, message);
}

pub fn log_to(dir: &Path, level: &str, message: &str) {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("aria.log");
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_BYTES {
            let _ = std::fs::remove_file(&path);
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{stamp} [{level}] {message}");
    }
}

pub fn error(message: &str) {
    log("error", message);
}

pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log("panic", &info.to_string());
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("aria-log-test-{name}"))
    }

    #[test]
    fn writes_lines_and_keeps_appending() {
        let dir = temp_dir("append");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");

        log_to(&dir, "error", "first failure");
        log_to(&dir, "panic", "second failure");

        let raw = std::fs::read_to_string(dir.join("aria.log")).expect("log file");
        assert!(raw.contains("[error] first failure"), "{raw}");
        assert!(raw.contains("[panic] second failure"), "{raw}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn oversized_log_is_rotated() {
        let dir = temp_dir("rotate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("aria.log");
        std::fs::write(&path, vec![b'x'; (MAX_BYTES + 16) as usize]).expect("big log");

        log_to(&dir, "error", "after rotation");

        let raw = std::fs::read_to_string(&path).expect("log file");
        assert!(raw.len() < 1024, "log was not rotated");
        assert!(raw.contains("after rotation"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
