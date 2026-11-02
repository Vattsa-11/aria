use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::model::{Track, TrackSource};

pub const CANCELLED: &str = "Cancelled";
const ATTEMPTS: u32 = 3;
const CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadPath {
    YtDlp,
    Http,
    Unsupported,
}

pub fn path_for(track: &Track) -> DownloadPath {
    match track.source {
        TrackSource::Yt { .. } => DownloadPath::YtDlp,
        TrackSource::Jiosaavn { .. } => DownloadPath::Http,
        TrackSource::Local(_) => DownloadPath::Unsupported,
    }
}

pub fn file_stem(track: &Track) -> String {
    let clean = |text: &str| -> String {
        text.chars()
            .map(|c| if c.is_control() || "\\/:*?\"<>|%".contains(c) { ' ' } else { c })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut name = if track.artist.trim().is_empty() {
        clean(&track.title)
    } else {
        format!("{} - {}", clean(&track.artist), clean(&track.title))
    };
    name = name.chars().take(100).collect::<String>().trim().trim_end_matches('.').to_string();
    if name.is_empty() {
        track.id.to_string()
    } else {
        format!("{} - {}", track.id, name)
    }
}

pub fn extension_for(url: &str, content_type: Option<&str>) -> &'static str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp3" => "mp3",
        "m4a" | "mp4" | "aac" => "m4a",
        "flac" => "flac",
        "ogg" | "oga" => "ogg",
        "opus" => "opus",
        "webm" => "webm",
        "wav" => "wav",
        _ => match content_type.unwrap_or("").split(';').next().unwrap_or("").trim() {
            "audio/mpeg" => "mp3",
            "audio/flac" => "flac",
            "audio/ogg" => "ogg",
            "audio/webm" => "webm",
            "audio/wav" | "audio/x-wav" => "wav",
            _ => "m4a",
        },
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_connect(Some(Duration::from_secs(12)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .build()
        .into()
}

fn part_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.part"))
}

fn cancelled(part: &Path) -> String {
    let _ = std::fs::remove_file(part);
    CANCELLED.to_string()
}

fn total_from_range(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.trim().parse().ok()
}

enum Outcome {
    Done(Option<String>),
    Retry(String),
    Fatal(String),
}

fn attempt(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(String, String)],
    part: &Path,
    cancel: &AtomicBool,
    progress: &dyn Fn(u64, Option<u64>),
) -> Outcome {
    let have = std::fs::metadata(part).map(|meta| meta.len()).unwrap_or(0);
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(name.as_str(), value.as_str());
    }
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let mut response = match request.call() {
        Ok(response) => response,
        Err(err) => return Outcome::Retry(err.to_string()),
    };
    let status = response.status().as_u16();
    if status == 416 {
        let _ = std::fs::remove_file(part);
        return Outcome::Retry("range not satisfiable".to_string());
    }
    if !(200..300).contains(&status) {
        let message = format!("server answered HTTP {status}");
        return if status >= 500 || status == 429 {
            Outcome::Retry(message)
        } else {
            Outcome::Fatal(message)
        };
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let content_length = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let (mut written, total) = if status == 206 {
        let total = response
            .headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(total_from_range)
            .or(content_length.map(|length| length + have));
        (have, total)
    } else {
        (0, content_length)
    };
    let file = if written == 0 {
        OpenOptions::new().create(true).write(true).truncate(true).open(part)
    } else {
        OpenOptions::new().append(true).open(part)
    };
    let mut file = match file {
        Ok(file) => file,
        Err(err) => return Outcome::Fatal(err.to_string()),
    };
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Outcome::Fatal(CANCELLED.to_string());
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) => return Outcome::Retry(err.to_string()),
        };
        if let Err(err) = file.write_all(&buffer[..read]) {
            return Outcome::Fatal(err.to_string());
        }
        written += read as u64;
        progress(written, total);
    }
    if let Err(err) = file.flush() {
        return Outcome::Fatal(err.to_string());
    }
    match total {
        Some(total) if written != total => Outcome::Retry(format!("got {written} of {total} bytes")),
        _ => Outcome::Done(content_type),
    }
}

pub fn fetch(
    url: &str,
    headers: &[(String, String)],
    dir: &Path,
    stem: &str,
    cancel: Arc<AtomicBool>,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let part = part_path(dir, stem);
    let agent = agent();
    let mut last_error = String::new();
    for round in 0..ATTEMPTS {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled(&part));
        }
        if round > 0 {
            std::thread::sleep(Duration::from_millis(400 * 2u64.pow(round)));
        }
        match attempt(&agent, url, headers, &part, &cancel, progress) {
            Outcome::Done(content_type) => {
                let extension = extension_for(url, content_type.as_deref());
                let target = dir.join(format!("{stem}.{extension}"));
                std::fs::rename(&part, &target).map_err(|err| err.to_string())?;
                return Ok(target);
            }
            Outcome::Retry(message) => {
                crate::logs::error(&format!("download attempt {} failed: {message}", round + 1));
                last_error = message;
            }
            Outcome::Fatal(message) if message == CANCELLED => return Err(cancelled(&part)),
            Outcome::Fatal(message) => {
                let _ = std::fs::remove_file(&part);
                return Err(message);
            }
        }
    }
    Err(format!("Download failed after {ATTEMPTS} attempts: {last_error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    fn body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn serve(data: Vec<u8>, connections: usize, honor_range: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            for _ in 0..connections {
                let Ok((mut stream, _)) = listener.accept() else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut start = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\\r\\n" || line == "\r\n" {
                        break;
                    }
                    if let Some(rest) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        start = rest.trim().trim_end_matches('-').parse().unwrap_or(0);
                    }
                }
                let head = if honor_range && start > 0 {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{}\r\nContent-Length: {}\r\nContent-Type: audio/mp4\r\nConnection: close\r\n\r\n",
                        data.len() - 1,
                        data.len(),
                        data.len() - start
                    )
                } else {
                    start = 0;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: audio/mp4\r\nConnection: close\r\n\r\n",
                        data.len()
                    )
                };
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&data[start..]);
            }
        });
        format!("http://{address}/song.mp4?token=1")
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aria-httpdl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn downloads_and_renames_with_the_right_extension() {
        let data = body(300_000);
        let url = serve(data.clone(), 1, true);
        let dir = temp_dir("ok");
        let seen = std::sync::atomic::AtomicU64::new(0);
        let path = fetch(&url, &[], &dir, "1 - Artist - Song", Arc::new(AtomicBool::new(false)), &|done, total| {
            assert_eq!(total, Some(300_000));
            seen.store(done, Ordering::Relaxed);
        })
        .expect("download");
        assert_eq!(path.file_name().unwrap().to_string_lossy(), "1 - Artist - Song.m4a");
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(seen.load(Ordering::Relaxed), 300_000);
        assert!(!part_path(&dir, "1 - Artist - Song").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancel_removes_the_part_file() {
        let url = serve(body(600_000), 1, true);
        let dir = temp_dir("cancel");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let result = fetch(&url, &[], &dir, "2", cancel, &|done, _| {
            if done > 100_000 {
                flag.store(true, Ordering::Relaxed);
            }
        });
        assert_eq!(result.unwrap_err(), CANCELLED);
        assert!(!part_path(&dir, "2").exists());
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resumes_from_an_existing_part_file() {
        let data = body(200_000);
        let url = serve(data.clone(), 1, true);
        let dir = temp_dir("resume");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(part_path(&dir, "3"), &data[..70_000]).unwrap();
        let first = std::sync::atomic::AtomicU64::new(u64::MAX);
        let path = fetch(&url, &[], &dir, "3", Arc::new(AtomicBool::new(false)), &|done, _| {
            let _ = first.compare_exchange(u64::MAX, done, Ordering::Relaxed, Ordering::Relaxed);
        })
        .expect("resume");
        assert_eq!(std::fs::read(path).unwrap(), data);
        assert!(first.load(Ordering::Relaxed) > 70_000, "should continue past the existing bytes");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn restarts_when_the_server_ignores_range() {
        let data = body(120_000);
        let url = serve(data.clone(), 1, false);
        let dir = temp_dir("norange");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(part_path(&dir, "4"), b"stale bytes").unwrap();
        let path = fetch(&url, &[], &dir, "4", Arc::new(AtomicBool::new(false)), &|_, _| {}).expect("restart");
        assert_eq!(std::fs::read(path).unwrap(), data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn retries_a_dropped_connection_then_resumes() {
        let data = body(150_000);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let served = data.clone();
        std::thread::spawn(move || {
            for round in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut start = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(rest) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        start = rest.trim().trim_end_matches('-').parse().unwrap_or(0);
                    }
                }
                if round == 0 {
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", served.len());
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&served[..50_000]);
                } else {
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        served.len() - 1,
                        served.len(),
                        served.len() - start
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&served[start..]);
                }
            }
        });
        let dir = temp_dir("retry");
        let url = format!("http://{address}/a.mp3");
        let path = fetch(&url, &[], &dir, "5", Arc::new(AtomicBool::new(false)), &|_, _| {}).expect("retry");
        assert_eq!(path.extension().unwrap(), "mp3");
        assert_eq!(std::fs::read(path).unwrap(), data);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn client_errors_fail_without_retrying() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 1024];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        });
        let dir = temp_dir("404");
        let error = fetch(&format!("http://{address}/x.m4a"), &[], &dir, "6", Arc::new(AtomicBool::new(false)), &|_, _| {}).unwrap_err();
        assert!(error.contains("404"), "{error}");
        assert!(!part_path(&dir, "6").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn provider_decides_the_download_path() {
        let yt = Track::yt("abc", "Song", "Artist", None);
        assert_eq!(path_for(&yt), DownloadPath::YtDlp);
        let mut jio = yt.clone();
        jio.source = TrackSource::Jiosaavn { id: "xyz".into() };
        assert_eq!(path_for(&jio), DownloadPath::Http);
        let local = Track::local(Path::new("/music/a.mp3"));
        assert_eq!(path_for(&local), DownloadPath::Unsupported);
    }

    #[test]
    fn names_files_from_title_and_artist() {
        let mut track = Track::yt("abc", "What?/Is*This: \"Song\"", "The  Artist", None);
        track.id = 42;
        assert_eq!(file_stem(&track), "42 - The Artist - What Is This Song");
        track.artist.clear();
        assert_eq!(file_stem(&track), "42 - What Is This Song");
        track.title = "???".into();
        assert_eq!(file_stem(&track), "42");
        assert_eq!(extension_for("https://x/y.mp4?a=b", None), "m4a");
        assert_eq!(extension_for("https://x/y", Some("audio/mpeg; charset=x")), "mp3");
        assert_eq!(extension_for("https://x/y", None), "m4a");
    }
}
