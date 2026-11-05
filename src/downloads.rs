
use std::path::{Path, PathBuf};

use crate::settings::data_dir;

pub fn dir() -> PathBuf {
    data_dir().join("downloads")
}

pub fn ensure_dir() -> Result<PathBuf, String> {
    let dir = dir();
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn existing(track_id: u64) -> Option<PathBuf> {
    existing_in(&dir(), track_id)
}

#[cfg(target_arch = "wasm32")]
pub fn existing(track_id: u64) -> Option<PathBuf> {
    Some(PathBuf::from(format!("offline/{track_id}")))
}

#[cfg(target_arch = "wasm32")]
pub fn offline_id(path: &Path) -> Option<u64> {
    path.strip_prefix("offline").ok()?.to_str()?.parse().ok()
}

pub fn playback_track(track: &crate::model::Track, local: Option<PathBuf>) -> crate::model::Track {
    let mut playable = track.clone();
    if let Some(path) = local {
        playable.source = crate::model::TrackSource::Local(path);
    }
    playable
}

fn existing_in(folder: &Path, track_id: u64) -> Option<PathBuf> {
    let dotted = format!("{track_id}.");
    let named = format!("{track_id} - ");
    let mut found: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
            (name.starts_with(&dotted) || name.starts_with(&named)) && !name.ends_with(".part") && !name.ends_with(".ytdl") && path.is_file()
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn stats() -> (usize, u64) {
    stats_in(&dir())
}

#[cfg(target_arch = "wasm32")]
pub fn stats() -> (usize, u64) {
    (0, 0)
}

fn stats_in(dir: &Path) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    entries
        .flatten()
        .filter_map(|entry| entry.metadata().ok())
        .filter(|meta| meta.is_file())
        .fold((0, 0), |(count, bytes), meta| (count + 1, bytes + meta.len()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_all() -> usize {
    clear_in(&dir())
}

#[cfg(target_arch = "wasm32")]
pub fn clear_all() -> usize {
    crate::install::call_named("ariaOfflineClear");
    0
}

fn clear_in(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn remove_partials_for(stem: &str) {
    remove_partials_in(&dir(), stem);
}

fn remove_partials_in(folder: &Path, stem: &str) {
    let prefix = format!("{stem}.");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().map(|name| name.to_string_lossy().to_string()) else {
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        if !name.ends_with(".part") && !name.ends_with(".ytdl") {
            continue;
        }
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove(track_id: u64) -> bool {
    match existing(track_id) {
        Some(path) => std::fs::remove_file(path).is_ok(),
        None => false,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn remove(track_id: u64) -> bool {
    crate::install::call_with("ariaOfflineDelete", &track_id.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Track;
    use std::time::Duration;

    #[test]
    fn finds_and_removes_downloaded_files() {
        crate::settings::test_data_dir();
        let track = Track::yt("aria-test-downloads", "Song", "Artist", Some(Duration::from_secs(3)));
        let dir = ensure_dir().expect("downloads dir");
        let _ = remove(track.id);
        assert!(existing(track.id).is_none());

        let partial = dir.join(format!("{}.m4a.part", track.id));
        std::fs::write(&partial, b"partial").expect("write partial");
        assert!(existing(track.id).is_none());
        let _ = std::fs::remove_file(&partial);

        let path = dir.join(format!("{}.m4a", track.id));
        std::fs::write(&path, b"audio").expect("write audio");
        assert_eq!(existing(track.id), Some(path));

        assert!(remove(track.id));
        assert!(existing(track.id).is_none());
    }

    #[test]
    fn cancel_cleanup_leaves_finished_files_alone() {
        crate::settings::test_data_dir();
        let dir = ensure_dir().expect("downloads dir").join("cancel-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("77.m4a"), b"audio").expect("audio");
        std::fs::write(dir.join("77.m4a.part"), b"partial").expect("partial");
        std::fs::write(dir.join("77.m4a.ytdl"), b"state").expect("ytdl");

        remove_partials_in(&dir, "77");

        assert!(dir.join("77.m4a").is_file());
        assert!(!dir.join("77.m4a.part").exists());
        assert!(!dir.join("77.m4a.ytdl").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_both_id_and_named_files_but_not_partials() {
        crate::settings::test_data_dir();
        let dir = ensure_dir().expect("downloads dir").join("existing-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("9 - Artist - Song.m4a.part"), b"x").unwrap();
        std::fs::write(dir.join("9 - Artist - Song.part"), b"x").unwrap();
        assert!(existing_in(&dir, 9).is_none());
        std::fs::write(dir.join("9 - Artist - Song.m4a"), b"x").unwrap();
        assert!(existing_in(&dir, 9).unwrap().ends_with("9 - Artist - Song.m4a"));
        std::fs::write(dir.join("8.webm"), b"x").unwrap();
        assert!(existing_in(&dir, 8).unwrap().ends_with("8.webm"));
        assert!(existing_in(&dir, 90).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn local_file_replaces_the_online_source_for_playback() {
        let online = Track::yt("abc", "Song", "Artist", None);
        let local = playback_track(&online, Some(PathBuf::from("/d/1.m4a")));
        assert_eq!(local.local_path(), Some(Path::new("/d/1.m4a")));
        assert_eq!(local.id, online.id);
        let streamed = playback_track(&online, None);
        assert!(streamed.is_online());
    }

    #[test]
    fn stats_count_files_and_bytes() {
        crate::settings::test_data_dir();
        let dir = ensure_dir().expect("downloads dir").join("stats-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("1.m4a"), b"12345").expect("write");
        std::fs::write(dir.join("2.m4a"), b"123").expect("write");
        assert_eq!(stats_in(&dir), (2, 8));
        assert_eq!(stats_in(&dir.join("missing")), (0, 0));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clears_every_file_in_a_folder() {
        crate::settings::test_data_dir();
        let dir = ensure_dir().expect("downloads dir").join("clear-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("1.m4a"), b"audio").expect("write audio");
        std::fs::write(dir.join("2.webm.part"), b"partial").expect("write partial");

        assert_eq!(clear_in(&dir), 2);
        assert_eq!(std::fs::read_dir(&dir).expect("read dir").count(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
