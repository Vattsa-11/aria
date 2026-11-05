use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::doctor::{Finding, Severity};

pub struct Inputs {
    pub downloads_dir: PathBuf,
    pub play_ok: u32,
    pub play_err: u32,
}

fn pass(title: impl Into<String>, detail: impl Into<String>) -> Finding {
    Finding { severity: Severity::Info, title: title.into(), detail: detail.into(), suggestion: String::new() }
}

fn problem(severity: Severity, title: impl Into<String>, detail: impl Into<String>, suggestion: impl Into<String>) -> Finding {
    Finding { severity, title: title.into(), detail: detail.into(), suggestion: suggestion.into() }
}

fn database() -> Finding {
    match crate::db::integrity() {
        Ok(text) if text.trim() == "ok" => pass("Database integrity: ok", "SQLite integrity_check passed"),
        Ok(text) => problem(Severity::Error, "Database integrity check failed", text.chars().take(300).collect::<String>(), "Back up the Aria data folder, then delete library.db to rebuild it"),
        Err(err) => problem(Severity::Error, "Database could not be opened", err, "Check that the Aria data folder is writable"),
    }
}

fn audio_device() -> Finding {
    match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(device) => {
            drop(device);
            pass("Audio device: available", "A fresh probe of the default output device succeeded")
        }
        Err(err) => problem(Severity::Error, "No audio output device", err.to_string(), "Plug in or enable an output device, then restart Aria"),
    }
}

pub fn probe(url: &str) -> Result<Duration, String> {
    let started = Instant::now();
    match crate::http::fetch_agent().get(url).header("User-Agent", concat!("Aria/", env!("CARGO_PKG_VERSION"))).call() {
        Ok(_) | Err(ureq::Error::StatusCode(_)) => Ok(started.elapsed()),
        Err(err) => Err(err.to_string()),
    }
}

pub fn latency_finding(name: &str, outcome: Result<Duration, String>) -> Finding {
    match outcome {
        Ok(took) if took > Duration::from_millis(2500) => problem(Severity::Warn, format!("{name}: slow ({} ms)", took.as_millis()), "The service answered, but slowly, so songs may take a while to start", "Try another network, or turn on Battery saver"),
        Ok(took) => pass(format!("{name}: reachable"), format!("{} ms", took.as_millis())),
        Err(err) => problem(Severity::Error, format!("{name}: unreachable"), err, "Check your internet connection or firewall"),
    }
}

fn providers() -> Vec<Finding> {
    [
        ("YouTube Music", "https://music.youtube.com/"),
        ("JioSaavn", "https://www.jiosaavn.com/"),
        ("GitHub (updates)", "https://api.github.com/"),
    ]
    .into_iter()
    .map(|(name, url)| latency_finding(name, probe(url)))
    .collect()
}

pub fn writable(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let probe = dir.join(".aria-write-test");
    std::fs::write(&probe, b"ok").map_err(|err| err.to_string())?;
    std::fs::remove_file(&probe).map_err(|err| err.to_string())
}

fn downloads(dir: &std::path::Path) -> Finding {
    match writable(dir) {
        Ok(()) => pass("Downloads folder: writable", dir.display().to_string()),
        Err(err) => problem(Severity::Error, "Downloads folder is not writable", format!("{}: {err}", dir.display()), "Choose a folder you can write to, or fix its permissions"),
    }
}

fn update_integrity() -> Finding {
    match crate::update::check() {
        Err(err) => pass("Update check skipped", err),
        Ok(release) => match release.asset {
            None => pass("Update integrity: no installer for this platform", format!("latest release {}", release.version)),
            Some(asset) if asset.sum_url.is_some() => pass("Update integrity: checksum published", format!("{} has a SHA-256 file, and Aria verifies it before installing", asset.name)),
            Some(asset) => problem(Severity::Warn, "Latest release has no checksum", format!("{} has no .sha256 file, so Aria will refuse to install it", asset.name), "Publish the .sha256 files with the release"),
        },
    }
}

pub fn playback_finding(ok: u32, err: u32) -> Finding {
    let total = ok + err;
    if total < 3 {
        pass("Playback: not enough plays yet", format!("{ok} played, {err} failed this session"))
    } else if err * 2 >= total {
        problem(Severity::Warn, "Most plays are failing", format!("{err} of {total} plays failed this session"), "Run the provider checks above; update yt-dlp in Settings if YouTube is failing")
    } else {
        pass("Playback: working", format!("{ok} played, {err} failed this session"))
    }
}

pub fn run(inputs: Inputs) -> Vec<Finding> {
    let mut findings = vec![database(), audio_device()];
    findings.extend(providers());
    findings.push(downloads(&inputs.downloads_dir));
    findings.push(update_integrity());
    findings.push(playback_finding(inputs.play_ok, inputs.play_err));
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_and_missing_services_are_flagged() {
        assert_eq!(latency_finding("X", Ok(Duration::from_millis(300))).severity.label(), "info");
        assert_eq!(latency_finding("X", Ok(Duration::from_millis(4000))).severity.label(), "warning");
        assert_eq!(latency_finding("X", Err("dns".into())).severity.label(), "error");
    }

    #[test]
    fn playback_failures_only_matter_after_a_few_plays() {
        assert_eq!(playback_finding(0, 2).severity.label(), "info");
        assert_eq!(playback_finding(1, 3).severity.label(), "warning");
        assert_eq!(playback_finding(9, 1).severity.label(), "info");
    }

    #[test]
    fn a_writable_folder_passes_and_a_file_in_its_place_fails() {
        let base = std::env::temp_dir().join(format!("aria-health-{}", std::process::id()));
        assert!(writable(&base).is_ok());
        let file = base.join("not-a-folder");
        std::fs::write(&file, b"x").unwrap();
        assert!(writable(&file).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
