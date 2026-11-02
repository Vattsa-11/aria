use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::provider::{ProviderMeta, ProviderName};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackSource {
    Local(PathBuf),
    Yt { id: String },
    Jiosaavn { id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<Duration>,
    pub source: TrackSource,
    pub thumb: Option<String>,
    #[serde(default)]
    pub provider_meta: Option<ProviderMeta>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: u64,
    pub name: String,
    pub tracks: Vec<Track>,
}

impl Playlist {
    pub fn new(name: &str) -> Self {
        Self {
            id: hash_id(&format!("playlist:{name}:{}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0))),
            name: name.to_string(),
            tracks: Vec::new(),
        }
    }
}

pub fn hash_id(key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.to_lowercase().hash(&mut hasher);
    hasher.finish()
}

impl Track {
    pub fn local(path: &Path) -> Self {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let (artist, title) = match stem.split_once(" - ") {
            Some((artist, title)) if !artist.trim().is_empty() => {
                (artist.trim().to_string(), title.trim().to_string())
            }
            _ => (String::new(), stem.trim().to_string()),
        };

        Self {
            id: hash_id(&path.to_string_lossy()),
            title,
            artist,
            album: String::new(),
            duration: None,
            source: TrackSource::Local(path.to_path_buf()),
            thumb: None,
            provider_meta: None,
        }
    }

    pub fn from_path(path: &Path) -> Self {
        Self::local(path)
    }

    pub fn yt(video_id: &str, title: &str, artist: &str, duration: Option<Duration>) -> Self {
        Self {
            id: hash_id(&format!("yt:{video_id}")),
            title: title.to_string(),
            artist: artist.to_string(),
            album: String::new(),
            duration,
            source: TrackSource::Yt {
                id: video_id.to_string(),
            },
            thumb: Some(thumb_url(video_id)),
            provider_meta: Some(ProviderMeta {
                provider: ProviderName::Youtube,
                bitrate_kbps: None,
            }),
        }
    }

    pub fn display_artist(&self) -> &str {
        if self.artist.is_empty() {
            "Unknown artist"
        } else {
            &self.artist
        }
    }

    pub fn filename(&self) -> Option<&str> {
        None
    }

    pub fn local_path(&self) -> Option<&Path> {
        match &self.source {
            TrackSource::Local(path) => Some(path),
            TrackSource::Yt { .. } | TrackSource::Jiosaavn { .. } => None,
        }
    }

    pub fn yt_id(&self) -> Option<String> {
        match &self.source {
            TrackSource::Yt { id } => Some(id.clone()),
            TrackSource::Local(_) | TrackSource::Jiosaavn { .. } => None,
        }
    }

    pub fn watch_url(&self) -> Option<String> {
        match &self.source {
            TrackSource::Yt { id } => Some(format!("https://www.youtube.com/watch?v={id}")),
            TrackSource::Local(_) | TrackSource::Jiosaavn { .. } => None,
        }
    }

    pub fn art_source(&self) -> Option<(String, bool)> {
        if let Some(url) = self.thumb.as_ref().filter(|url| !url.trim().is_empty()) {
            return Some((url.clone(), url.contains("ytimg.com")));
        }
        let folder = self.local_path()?.parent()?;
        find_local_art(folder).map(|path| (path.to_string_lossy().to_string(), false))
    }

    pub fn is_online(&self) -> bool {
        !matches!(self.source, TrackSource::Local(_))
    }

    pub fn quality_badge(&self) -> Option<String> {
        self.provider_meta.as_ref().map(|m| m.quality_badge())
    }
}

const LOCAL_ART_NAMES: [&str; 5] = ["cover", "folder", "front", "album", "albumart"];
const LOCAL_ART_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

pub fn find_local_art(folder: &Path) -> Option<PathBuf> {
    let mut best: Option<(usize, PathBuf)> = None;
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        let stem = path.file_stem()?.to_string_lossy().to_lowercase();
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        if !LOCAL_ART_EXTENSIONS.contains(&extension.as_str()) {
            continue;
        }
        if let Some(rank) = LOCAL_ART_NAMES.iter().position(|name| *name == stem) {
            if best.as_ref().is_none_or(|(current, _)| rank < *current) {
                best = Some((rank, path));
            }
        }
    }
    best.map(|(_, path)| path)
}

pub fn upgrade_jiosaavn_image(url: &str) -> String {
    for small in ["50x50", "150x150", "250x250"] {
        if url.contains(small) {
            return url.replace(small, "500x500");
        }
    }
    url.to_string()
}

pub fn thumb_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
}

pub fn fmt_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "0:00".to_string();
    }
    let total = secs.round() as u64;
    let minutes = total / 60;
    let seconds = total % 60;
    if minutes >= 60 {
        format!("{}:{:02}:{:02}", minutes / 60, minutes % 60, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

pub fn fmt_optional_time(secs: Option<f64>) -> String {
    match secs {
        Some(value) => fmt_time(value),
        None => "--:--".to_string(),
    }
}

#[cfg(test)]
mod art_tests {
    use super::*;

    #[test]
    fn jiosaavn_art_is_upgraded_to_500() {
        assert_eq!(
            upgrade_jiosaavn_image("https://c.saavncdn.com/475/cover-150x150.jpg"),
            "https://c.saavncdn.com/475/cover-500x500.jpg"
        );
        assert_eq!(upgrade_jiosaavn_image("https://c.saavncdn.com/a-50x50.jpg"), "https://c.saavncdn.com/a-500x500.jpg");
        assert_eq!(upgrade_jiosaavn_image("https://c.saavncdn.com/large.jpg"), "https://c.saavncdn.com/large.jpg");
    }

    #[test]
    fn each_provider_supplies_an_art_source() {
        let yt = Track::yt("abc123", "Song", "Artist", None);
        assert_eq!(
            yt.art_source(),
            Some(("https://i.ytimg.com/vi/abc123/hqdefault.jpg".to_string(), true))
        );
        let mut jio = yt.clone();
        jio.source = TrackSource::Jiosaavn { id: "x".into() };
        jio.thumb = Some("https://c.saavncdn.com/a-500x500.jpg".into());
        assert_eq!(jio.art_source(), Some(("https://c.saavncdn.com/a-500x500.jpg".to_string(), false)));
        jio.thumb = Some("  ".into());
        assert_eq!(jio.art_source(), None);
        let local = Track::local(Path::new("/nonexistent/a.mp3"));
        assert_eq!(local.art_source(), None);
    }

    #[test]
    fn local_files_use_folder_art_with_cover_first() {
        let dir = std::env::temp_dir().join(format!("aria-art-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(find_local_art(&dir), None);
        std::fs::write(dir.join("Folder.JPG"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        assert!(find_local_art(&dir).unwrap().ends_with("Folder.JPG"));
        std::fs::write(dir.join("cover.png"), b"x").unwrap();
        assert!(find_local_art(&dir).unwrap().ends_with("cover.png"));
        let track = Track::local(&dir.join("song.mp3"));
        let (source, letterboxed) = track.art_source().unwrap();
        assert!(source.ends_with("cover.png") && !letterboxed);
        std::fs::remove_dir_all(&dir).ok();
    }
}
