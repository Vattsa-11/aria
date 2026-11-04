use serde::Deserialize;

use crate::model::Track;

#[derive(Clone, Debug, PartialEq)]
pub struct LyricLine {
    pub at: f64,
    pub text: String,
}

pub fn parse_lrc(raw: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for line in raw.lines() {
        let mut stamps = Vec::new();
        let mut rest = line.trim_start();
        while let Some(open) = rest.strip_prefix('[') {
            let Some(close) = open.find(']') else {
                break;
            };
            let Some(seconds) = parse_stamp(&open[..close]) else {
                break;
            };
            stamps.push(seconds);
            rest = &open[close + 1..];
        }
        if stamps.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        if text.is_empty() {
            continue;
        }
        for at in stamps {
            lines.push(LyricLine {
                at,
                text: text.clone(),
            });
        }
    }
    lines.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    lines
}

fn parse_stamp(tag: &str) -> Option<f64> {
    let (minutes, rest) = tag.split_once(':')?;
    let minutes: f64 = minutes.trim().parse().ok()?;
    let seconds: f64 = rest.replace(',', ".").parse().ok()?;
    if !seconds.is_finite() || minutes < 0.0 {
        return None;
    }
    Some(minutes * 60.0 + seconds)
}

pub fn active_line(lines: &[LyricLine], position: f64) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }
    let mut found = None;
    for (index, line) in lines.iter().enumerate() {
        if line.at <= position + 0.15 {
            found = Some(index);
        } else {
            break;
        }
    }
    found
}

#[derive(Deserialize)]
struct LrcLibHit {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch(track: &Track) -> Result<String, String> {
    let title = clean_title(&track.title);
    let artist = track.artist.trim().to_string();
    if title.is_empty() {
        return Err("Nothing to look up".to_string());
    }
    let mut url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}",
        encode(&artist),
        encode(&title)
    );
    if let Some(duration) = track.duration {
        url.push_str(&format!("&duration={}", duration.as_secs()));
    }
    if let Some(found) = request(&url) {
        if let Some(lyrics) = pick(&found) {
            return Ok(lyrics);
        }
    }
    let search = format!(
        "https://lrclib.net/api/search?q={}",
        encode(&format!("{artist} {title}"))
    );
    let results = request_list(&search).unwrap_or_default();
    let mut best: Option<&LrcLibHit> = None;
    let mut best_synced = false;
    for hit in &results {
        let synced = hit
            .synced_lyrics
            .as_ref()
            .map(|text| !text.trim().is_empty())
            .unwrap_or(false);
        if synced && !best_synced {
            best = Some(hit);
            best_synced = true;
            continue;
        }
        if best.is_none() {
            best = Some(hit);
        }
    }
    match best.and_then(pick) {
        Some(lyrics) => Ok(lyrics),
        None => Err("No lyrics found".to_string()),
    }
}

fn pick(hit: &LrcLibHit) -> Option<String> {
    if let Some(text) = hit.synced_lyrics.as_ref() {
        if !text.trim().is_empty() {
            return Some(text.clone());
        }
    }
    hit.plain_lyrics
        .as_ref()
        .filter(|text| !text.trim().is_empty())
        .cloned()
}

#[cfg(not(target_arch = "wasm32"))]
fn request(url: &str) -> Option<LrcLibHit> {
    let agent = crate::http::fetch_agent();
    let body = agent.get(url).call().ok()?.into_body();
    serde_json::from_reader(body.into_reader()).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn request_list(url: &str) -> Option<Vec<LrcLibHit>> {
    let agent = crate::http::fetch_agent();
    let body = agent.get(url).call().ok()?.into_body();
    serde_json::from_reader(body.into_reader()).ok()
}

fn clean_title(title: &str) -> String {
    let mut text = title.to_string();
    for marker in [
        " (Official Video)",
        " (Official Audio)",
        " (Lyric Video)",
        " (Lyrics)",
        " [Official Music Video]",
        " (Audio)",
    ] {
        text = text.replace(marker, "");
    }
    text.trim().to_string()
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else if ch == ' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_parsed_in_order() {
        let raw = "[00:12.50]second\n[00:01.00]first\n";
        let lines = parse_lrc(raw);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "first");
        assert!((lines[0].at - 1.0).abs() < 1e-6);
        assert!((lines[1].at - 12.5).abs() < 1e-6);
    }

    #[test]
    fn multiple_timestamps_share_one_line() {
        let lines = parse_lrc("[00:10.00][00:20.00]chorus");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.text == "chorus"));
    }

    #[test]
    fn metadata_and_untimed_lines_are_ignored() {
        let raw = "[ar:Someone]\n[ti:Song]\nplain line\n[00:05.00]real line\n";
        let lines = parse_lrc(raw);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "real line");
    }

    #[test]
    fn the_active_line_follows_the_position() {
        let lines = parse_lrc("[00:00.00]a\n[00:05.00]b\n[00:10.00]c\n");
        assert_eq!(active_line(&lines, -1.0), None);
        assert_eq!(active_line(&lines, 0.2), Some(0));
        assert_eq!(active_line(&lines, 7.0), Some(1));
        assert_eq!(active_line(&lines, 99.0), Some(2));
    }

    #[test]
    fn queries_are_encoded() {
        assert_eq!(encode("A B"), "A+B");
        assert_eq!(encode("a&b"), "a%26b");
    }

    #[test]
    fn title_markers_are_stripped() {
        assert_eq!(clean_title("Song (Official Video)"), "Song");
        assert_eq!(clean_title("  Song (Lyrics) "), "Song");
    }
}
