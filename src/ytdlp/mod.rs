use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::model::{thumb_url, Track};
use crate::provider::ProviderName;
pub use crate::provider::ResolvedStream;
use crate::settings::data_dir;

pub fn local_bin() -> PathBuf {
    data_dir().join("bin").join(binary_name())
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn release_asset() -> &'static str {
    if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp_linux"
    }
}

const NAMES: [&str; 4] = ["yt-dlp.exe", "yt-dlp.cmd", "yt-dlp.bat", "yt-dlp"];

pub fn candidates(
    path_var: Option<std::ffi::OsString>,
    local: &std::path::Path,
    exe_dir: Option<&std::path::Path>,
) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    if let Some(paths) = path_var {
        'dirs: for dir in std::env::split_paths(&paths) {
            for name in NAMES {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    found.push(candidate);
                    break 'dirs;
                }
            }
        }
    }
    if local.is_file() {
        found.push(local.to_path_buf());
    }
    if let Some(dir) = exe_dir {
        for name in ["yt-dlp.exe", "yt-dlp"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                found.push(candidate);
                break;
            }
        }
    }
    let mut unique: Vec<PathBuf> = Vec::new();
    for path in found {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}

fn version_key(version: &str) -> Vec<u64> {
    version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

pub fn pick_newest(found: Vec<(PathBuf, Option<String>)>) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, Option<Vec<u64>>)> = None;
    for (path, version) in found {
        let key = version.as_deref().map(version_key);
        let better = match &best {
            None => true,
            Some((_, current)) => key > *current,
        };
        if better {
            best = Some((path, key));
        }
    }
    best.map(|(path, _)| path)
}

type DiscoverKey = Vec<(PathBuf, Option<std::time::SystemTime>)>;

static DISCOVERED: std::sync::Mutex<Option<(DiscoverKey, Option<PathBuf>)>> =
    std::sync::Mutex::new(None);

fn version_of(binary: &std::path::Path) -> Option<String> {
    let output = crate::proc::command(binary).arg("--version").output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
}

pub fn discover() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from));
    let found = candidates(std::env::var_os("PATH"), &local_bin(), exe_dir.as_deref());
    if found.len() <= 1 {
        return found.into_iter().next();
    }
    let key: DiscoverKey = found
        .iter()
        .map(|path| (path.clone(), std::fs::metadata(path).and_then(|meta| meta.modified()).ok()))
        .collect();
    if let Ok(cache) = DISCOVERED.lock() {
        if let Some((cached_key, result)) = cache.as_ref() {
            if *cached_key == key {
                return result.clone();
            }
        }
    }
    let versions = found
        .into_iter()
        .map(|path| {
            let version = version_of(&path);
            (path, version)
        })
        .collect();
    let result = pick_newest(versions);
    if let Ok(mut cache) = DISCOVERED.lock() {
        *cache = Some((key, result.clone()));
    }
    result
}

pub fn version() -> Option<String> {
    version_of(&discover()?)
}

pub fn run(args: &[&str]) -> Result<String, String> {
    let binary = discover().ok_or_else(|| "yt-dlp not found".to_string())?;
    let output = crate::proc::command(&binary)
        .args(args)
        .output()
        .map_err(|err| format!("Failed to run yt-dlp: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let summary = stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");
        let message = if summary.is_empty() {
            "yt-dlp failed".to_string()
        } else {
            summary
        };
        return Err(message);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn safe_duration(value: Option<f64>) -> Option<Duration> {
    value
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
        .map(Duration::from_secs_f64)
}

pub fn search(query: &str, limit: usize) -> Result<Vec<Track>, String> {
    let needle = format!("ytsearch{limit}:{query}");
    let raw = run(&[
        "--flat-playlist",
        "--no-warnings",
        "--playlist-end",
        &limit.to_string(),
        "--dump-single-json",
        &needle,
    ])?;
    let json: Value = serde_json::from_str(&raw).map_err(|_| "yt-dlp returned invalid JSON".to_string())?;
    let entries = json.get("entries").and_then(Value::as_array).ok_or_else(|| "No results found".to_string())?;

    let mut tracks = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let title = entry.get("title").and_then(Value::as_str).unwrap_or("Unknown title");
        let artist = entry
            .get("uploader")
            .or_else(|| entry.get("channel"))
            .or_else(|| entry.get("artist"))
            .or_else(|| entry.get("creator"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let duration = safe_duration(entry.get("duration").and_then(Value::as_f64));
        let mut track = Track::yt(id, title, artist, duration);
        track.thumb = entry
            .get("thumbnails")
            .and_then(Value::as_array)
            .and_then(|items| items.last())
            .and_then(|item| item.get("url"))
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| Some(thumb_url(id)));
        tracks.push(track);
    }
    Ok(tracks)
}

pub fn playlist_tracks(url: &str) -> Result<Vec<Track>, String> {
    let raw = run(&[
        "--flat-playlist",
        "--no-warnings",
        "--dump-single-json",
        url,
    ])?;
    let json: Value =
        serde_json::from_str(&raw).map_err(|_| "yt-dlp returned invalid JSON".to_string())?;
    let entries = json
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| "That URL is not a playlist".to_string())?;

    let mut tracks = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Unknown title");
        let artist = entry
            .get("uploader")
            .or_else(|| entry.get("channel"))
            .or_else(|| entry.get("artist"))
            .or_else(|| entry.get("creator"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let duration = safe_duration(entry.get("duration").and_then(Value::as_f64));
        tracks.push(Track::yt(id, title, artist, duration));
    }
    if tracks.is_empty() {
        return Err("That playlist has no tracks".to_string());
    }
    Ok(tracks)
}

fn string_list(value: Option<&Value>) -> Vec<(String, String)> {
    let Some(Value::Object(map)) = value else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
        .collect()
}

pub fn resolve_ytm(url: &str) -> Result<ResolvedStream, String> {
    let raw = run(&[
        "-J",
        "--no-warnings",
        "--no-playlist",
        "-f",
        "bestaudio[ext=m4a]/bestaudio/best",
        "--audio-quality",
        "0",
        url,
    ])?;
    let json: Value = serde_json::from_str(&raw).map_err(|_| "yt-dlp returned invalid JSON".to_string())?;
    let url = json
        .get("url")
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| "yt-dlp returned no stream URL".to_string())?;
    let headers = string_list(json.get("http_headers"));
    let duration = safe_duration(json.get("duration").and_then(Value::as_f64));
    let bitrate = json
        .get("format")
        .and_then(|format| format.get("audio_bitrate"))
        .and_then(Value::as_f64)
        .map(|b| (b / 1000.0).round() as u16);
    Ok(ResolvedStream {
        url,
        headers,
        duration,
        provider: ProviderName::YoutubeMusic,
        bitrate_kbps: bitrate,
    })
}

pub fn resolve_best_audio(url: &str) -> Result<Option<ResolvedStream>, String> {
    let raw = run(&[
        "-J",
        "--no-warnings",
        "--no-playlist",
        "-f",
        "bestaudio/best",
        url,
    ])?;
    let mut stream_url: Option<String> = None;
    let mut headers = Vec::new();
    let mut duration = None;
    let mut bitrate: Option<u16> = None;

    if let Ok(json) = serde_json::from_str::<Value>(&raw) {
        duration = safe_duration(json.get("duration").and_then(Value::as_f64));
        let candidates = [
            json.get("url").map(|value| (&json, value)),
            json
                .get("requested_downloads")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .map(|first| (&json, first)),
            json
                .get("requested_formats")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .map(|first| (&json, first)),
        ];
        for candidate in candidates.into_iter().flatten() {
            let (root, format) = candidate;
            if let Some(found) = format.get("url").and_then(Value::as_str) {
                stream_url = Some(found.to_string());
                headers = string_list(format.get("http_headers").or(root.get("http_headers")));
                bitrate = format
                    .get("audio_bitrate")
                    .and_then(Value::as_f64)
                    .or_else(|| format.get("tbr").and_then(Value::as_f64))
                    .map(|b| (b / 1000.0).round() as u16);
                break;
            }
        }
    }

    let stream_url = match stream_url {
        Some(url) => url,
        None => {
            let raw = run(&[
                "--no-warnings",
                "--no-playlist",
                "-g",
                "-f",
                "bestaudio/best",
                url,
            ])?;
            raw.lines()
                .map(str::trim)
                .find(|line| line.starts_with("http"))
                .ok_or_else(|| "yt-dlp returned no stream URL".to_string())?
                .to_string()
        }
    };

    Ok(Some(ResolvedStream {
        url: stream_url,
        headers,
        duration,
        provider: ProviderName::Youtube,
        bitrate_kbps: bitrate,
    }))
}

fn download_args(dir: &std::path::Path, stem: &str, url: &str) -> Vec<String> {
    let template = dir.join(format!("{stem}.%(ext)s"));
    vec![
        "--no-warnings".to_string(),
        "--no-playlist".to_string(),
        "--no-progress".to_string(),
        "--newline".to_string(),
        "-f".to_string(),
        "bestaudio[ext=m4a]/bestaudio/best".to_string(),
        "--audio-quality".to_string(),
        "0".to_string(),
        "-o".to_string(),
        template.to_string_lossy().to_string(),
        url.to_string(),
    ]
}

fn find_download(dir: &std::path::Path, stem: &str) -> Option<PathBuf> {
    let prefix = format!("{stem}.");
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        if !name.starts_with(&prefix) || name.ends_with(".part") || name.ends_with(".ytdl") {
            continue;
        }
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

pub fn download_audio(
    url: &str,
    dir: &std::path::Path,
    stem: &str,
    cancel: Arc<AtomicBool>,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let binary = discover().ok_or_else(|| "yt-dlp not found".to_string())?;
    let args = download_args(dir, stem, url);
    let mut child = crate::proc::command(&binary)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| format!("Failed to run yt-dlp: {err}"))?;

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            crate::downloads::remove_partials_for(stem);
            return Err("Cancelled".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => {
                return Err("yt-dlp could not download that track".to_string());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(120)),
            Err(err) => return Err(err.to_string()),
        }
    }

    find_download(dir, stem).ok_or_else(|| "Download finished but no audio file was written".to_string())
}

pub fn download_latest() -> Result<PathBuf, String> {
    let destination = local_bin();
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let agent = crate::http::fetch_agent();
    let mut response = agent
        .get(format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{}", release_asset()))
        .call()
        .map_err(|err| err.to_string())?;
    use std::io::Read as _;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(64 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.len() < 1_000_000 {
        return Err("Downloaded yt-dlp looks truncated".to_string());
    }
    let temp = destination.with_extension("exe.tmp");
    std::fs::write(&temp, &bytes).map_err(|err| err.to_string())?;
    std::fs::rename(&temp, &destination).map_err(|err| err.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o755));
    }
    Ok(destination)
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aria-ytdlp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_path_then_data_folder_then_exe_folder() {
        let path_dir = scratch("path");
        let data_dir = scratch("data");
        let exe_dir = scratch("exe");
        std::fs::write(path_dir.join("yt-dlp"), b"x").unwrap();
        let local = data_dir.join("yt-dlp");
        std::fs::write(&local, b"x").unwrap();
        std::fs::write(exe_dir.join("yt-dlp"), b"x").unwrap();
        let path_var = std::env::join_paths([&path_dir]).ok();
        let order = candidates(path_var, &local, Some(&exe_dir));
        assert_eq!(
            order,
            vec![path_dir.join("yt-dlp"), local.clone(), exe_dir.join("yt-dlp")]
        );
        let without_path = candidates(None, &local, Some(&exe_dir));
        assert_eq!(without_path, vec![local.clone(), exe_dir.join("yt-dlp")]);
        let only_exe = candidates(None, &data_dir.join("missing"), Some(&exe_dir));
        assert_eq!(only_exe, vec![exe_dir.join("yt-dlp")]);
        assert!(candidates(None, &data_dir.join("missing"), None).is_empty());
        for dir in [path_dir, data_dir, exe_dir] {
            std::fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn newest_version_wins_and_ties_keep_the_earlier_one() {
        let old = (PathBuf::from("/usr/bin/yt-dlp"), Some("2024.04.09".to_string()));
        let new = (PathBuf::from("/data/bin/yt-dlp"), Some("2025.10.22".to_string()));
        assert_eq!(pick_newest(vec![old.clone(), new.clone()]), Some(new.0.clone()));
        assert_eq!(pick_newest(vec![new.clone(), old.clone()]), Some(new.0.clone()));
        let same = (PathBuf::from("/exe/yt-dlp"), Some("2025.10.22".to_string()));
        assert_eq!(pick_newest(vec![new.clone(), same]), Some(new.0.clone()));
        let unknown = (PathBuf::from("/odd/yt-dlp"), None);
        assert_eq!(pick_newest(vec![unknown.clone(), old.clone()]), Some(old.0.clone()));
        assert_eq!(pick_newest(vec![unknown.clone()]), Some(unknown.0));
        assert_eq!(pick_newest(vec![]), None);
        let nightly = (PathBuf::from("/n/yt-dlp"), Some("2025.10.22.232958".to_string()));
        assert_eq!(pick_newest(vec![new.clone(), nightly.clone()]), Some(nightly.0));
    }
}
