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
    duration: Option<f64>,
}

pub fn parse_lyrics(raw: &str) -> Vec<LyricLine> {
    let timed = parse_lrc(raw);
    if !timed.is_empty() {
        return timed;
    }
    let mut lines: Vec<LyricLine> = Vec::new();
    for line in raw.lines() {
        let text = line.trim();
        if text.is_empty() {
            if lines.last().is_some_and(|last| !last.text.is_empty()) {
                lines.push(LyricLine { at: f64::MAX, text: String::new() });
            }
            continue;
        }
        if text.starts_with('[') && text.ends_with(']') && text.contains(':') {
            continue;
        }
        lines.push(LyricLine { at: f64::MAX, text: text.to_string() });
    }
    while lines.last().is_some_and(|last| last.text.is_empty()) {
        lines.pop();
    }
    lines
}

pub fn is_timed(line: &LyricLine) -> bool {
    line.at < 1.0e9
}

const NOISE: [&str; 14] = ["official", "lyric", "video", "audio", "visualizer", "full song", "hd", "4k", "remaster", "feat", "ft.", "ft ", "with ", "from "];

fn strip_tags(title: &str) -> String {
    let mut out = String::new();
    let mut group = String::new();
    let mut depth = 0usize;
    for ch in title.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                if depth == 1 {
                    group.clear();
                }
                group.push(ch);
            }
            ')' | ']' if depth > 0 => {
                group.push(ch);
                depth -= 1;
                if depth == 0 {
                    let lower = group.to_lowercase();
                    if !NOISE.iter().any(|noise| lower.contains(noise)) {
                        out.push_str(&group);
                    }
                }
            }
            _ if depth > 0 => group.push(ch),
            _ => out.push(ch),
        }
    }
    if depth > 0 {
        out.push_str(&group);
    }
    let cut = out.find(" | ").map(|at| &out[..at]).unwrap_or(&out);
    cut.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn clean_title(title: &str) -> String {
    strip_tags(title)
}

fn first_artist(artist: &str) -> String {
    let lower = artist.to_lowercase();
    let mut cut = artist.len();
    for separator in [",", " & ", " feat", " ft.", " x ", " × ", " and ", " with "] {
        if let Some(at) = lower.find(separator) {
            cut = cut.min(at);
        }
    }
    artist[..cut].trim().to_string()
}

pub fn variants(title: &str, artist: &str) -> Vec<(String, String)> {
    let clean = clean_title(title);
    let artist = artist.trim().trim_end_matches(" - Topic").trim().to_string();
    let mut found: Vec<(String, String)> = Vec::new();
    let mut add = |title: String, artist: String| {
        if !title.is_empty() && !found.iter().any(|(t, a)| t.eq_ignore_ascii_case(&title) && a.eq_ignore_ascii_case(&artist)) {
            found.push((title, artist));
        }
    };
    if let Some((left, right)) = clean.split_once(" - ") {
        let (left, right) = (left.trim().to_string(), right.trim().to_string());
        if artist.is_empty() || artist.eq_ignore_ascii_case(&left) || left.to_lowercase().contains(&artist.to_lowercase()) {
            add(right.clone(), first_artist(&left));
            add(right.clone(), left.clone());
        }
        add(right, artist.clone());
    }
    add(clean.clone(), artist.clone());
    add(clean.clone(), first_artist(&artist));
    add(clean, String::new());
    found
}

fn choose(hits: &[LrcLibHit], duration: Option<u64>) -> Option<String> {
    let close = |hit: &LrcLibHit| match (duration, hit.duration) {
        (Some(want), Some(have)) => (have - want as f64).abs() <= 4.0,
        _ => true,
    };
    let synced = |hit: &LrcLibHit| hit.synced_lyrics.as_ref().is_some_and(|text| !text.trim().is_empty());
    hits.iter()
        .find(|hit| close(hit) && synced(hit))
        .or_else(|| hits.iter().find(|hit| close(hit) && pick(hit).is_some()))
        .or_else(|| hits.iter().find(|hit| synced(hit)))
        .or_else(|| hits.iter().find(|hit| pick(hit).is_some()))
        .and_then(pick)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn sidecar(track: &Track) -> Option<String> {
    let crate::model::TrackSource::Local(path) = &track.source else {
        return None;
    };
    let text = std::fs::read_to_string(path.with_extension("lrc")).ok()?;
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch(track: &Track) -> Result<String, String> {
    if let Some(text) = sidecar(track) {
        return Ok(text);
    }
    let seconds = track.duration.map(|duration| duration.as_secs());
    let tries = variants(&track.title, &track.artist);
    if tries.is_empty() {
        return Err("Nothing to look up".to_string());
    }
    for (title, artist) in tries.iter().take(4) {
        let mut url = format!("https://lrclib.net/api/get?artist_name={}&track_name={}", encode(artist), encode(title));
        if let Some(seconds) = seconds {
            url.push_str(&format!("&duration={seconds}"));
        }
        if let Some(lyrics) = request(&url).as_ref().and_then(pick) {
            return Ok(lyrics);
        }
        let by_fields = format!("https://lrclib.net/api/search?track_name={}&artist_name={}", encode(title), encode(artist));
        if let Some(lyrics) = choose(&request_list(&by_fields).unwrap_or_default(), seconds) {
            return Ok(lyrics);
        }
    }
    let (title, artist) = &tries[0];
    let loose = format!("https://lrclib.net/api/search?q={}", encode(&format!("{artist} {title}")));
    choose(&request_list(&loose).unwrap_or_default(), seconds).ok_or_else(|| "No lyrics found".to_string())
}

fn pick(hit: &LrcLibHit) -> Option<String> {
    if let Some(text) = hit.synced_lyrics.as_ref() {
        if !text.trim().is_empty() {
            return Some(text.clone());
        }
    }
    hit.plain_lyrics.as_ref().filter(|text| !text.trim().is_empty()).cloned()
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
        assert_eq!(clean_title("Kesariya (From \"Brahmastra\") [Official Audio]"), "Kesariya");
        assert_eq!(clean_title("Song (Taylor's Version)"), "Song (Taylor's Version)");
        assert_eq!(clean_title("Song | T-Series"), "Song");
    }

    #[test]
    fn artist_dash_title_is_split_into_the_likely_pairs() {
        let tries = variants("Arijit Singh - Kesariya (Official Video)", "");
        assert_eq!(tries[0], ("Kesariya".to_string(), "Arijit Singh".to_string()));
        let tries = variants("Song", "A & B");
        assert!(tries.contains(&("Song".to_string(), "A".to_string())));
        assert!(tries.contains(&("Song".to_string(), String::new())));
    }

    #[test]
    fn hits_are_chosen_by_duration_then_synced() {
        let hit = |synced: Option<&str>, plain: Option<&str>, duration: f64| LrcLibHit { synced_lyrics: synced.map(String::from), plain_lyrics: plain.map(String::from), duration: Some(duration) };
        let hits = vec![hit(Some("[00:01.00]far"), None, 300.0), hit(None, Some("near plain"), 200.0), hit(Some("[00:01.00]near"), None, 201.0)];
        assert_eq!(choose(&hits, Some(200)).as_deref(), Some("[00:01.00]near"));
        assert_eq!(choose(&hits, Some(500)).as_deref(), Some("[00:01.00]far"));
        assert!(choose(&[], Some(1)).is_none());
    }

    #[test]
    fn plain_lyrics_become_untimed_lines() {
        let lines = parse_lyrics("[ar:X]\nfirst\n\nsecond\n");
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), vec!["first", "", "second"]);
        assert!(lines.iter().all(|l| !is_timed(l)));
        assert!(parse_lyrics("[00:01.00]a").iter().all(is_timed));
    }
}
