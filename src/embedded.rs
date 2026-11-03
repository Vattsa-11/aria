use std::path::Path;

#[cfg(feature = "embed-ytdlp")]
static YTDLP: &[u8] = include_bytes!("../bundled/yt-dlp.exe");
#[cfg(not(feature = "embed-ytdlp"))]
static YTDLP: &[u8] = &[];

const MIN_VALID_BYTES: u64 = 1_000_000;

pub fn extract_if_missing() -> bool {
    write_if_missing(YTDLP, &crate::ytdlp::local_bin())
}

pub fn write_if_missing(bytes: &[u8], target: &Path) -> bool {
    if bytes.is_empty() {
        return false;
    }
    if std::fs::metadata(target).is_ok_and(|meta| meta.is_file() && meta.len() > MIN_VALID_BYTES) {
        return false;
    }
    let Some(parent) = target.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let temp = target.with_extension("tmp");
    std::fs::write(&temp, bytes).is_ok() && std::fs::rename(&temp, target).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("aria-embed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn unpacks_when_missing_and_keeps_a_newer_download() {
        let dir = scratch("keep");
        let target = dir.join("bin").join("yt-dlp.exe");
        let bundled = vec![1u8; 1_500_000];
        assert!(write_if_missing(&bundled, &target));
        assert_eq!(std::fs::read(&target).unwrap().len(), 1_500_000);
        std::fs::write(&target, vec![2u8; 2_000_000]).unwrap();
        assert!(!write_if_missing(&bundled, &target));
        assert_eq!(std::fs::read(&target).unwrap()[0], 2);
        assert!(!dir.join("bin").join("yt-dlp.tmp").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replaces_a_truncated_copy_and_ignores_empty_bundles() {
        let dir = scratch("repair");
        let target = dir.join("yt-dlp.exe");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&target, b"broken").unwrap();
        assert!(!write_if_missing(&[], &target));
        assert_eq!(std::fs::read(&target).unwrap(), b"broken");
        assert!(write_if_missing(&vec![3u8; 1_200_000], &target));
        assert_eq!(std::fs::metadata(&target).unwrap().len(), 1_200_000);
        std::fs::remove_dir_all(&dir).ok();
    }
}
