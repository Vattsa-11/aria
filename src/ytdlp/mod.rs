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
    data_dir().join("bin").join("yt-dlp.exe")
}

pub fn discover() -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in ["yt-dlp.exe", "yt-dlp.cmd", "yt-dlp.bat", "yt-dlp"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    let local = local_bin();
    if local.is_file() {
        return Some(local);
    }
    None
}

pub fn version() -> Option<String> {
    let binary = discover()?;
    let output = crate::proc::command(&binary).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().next().map(|line| line.trim().to_string())
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
        .get("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe")
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
    Ok(destination)
}
