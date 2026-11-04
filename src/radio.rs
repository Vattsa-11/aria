use std::time::Duration;

use serde_json::{json, Value};

use crate::model::Track;

const ENDPOINT: &str = "https://music.youtube.com/youtubei/v1/next?prettyPrint=false";
const CLIENT_VERSION: &str = "1.20251020.03.00";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0";
const MAX_PAGES: usize = 8;

pub fn seed_from_url(url: &str) -> Option<String> {
    let valid = |id: &str| id.len() == 11 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if let Some((_, query)) = url.split_once('?') {
        for pair in query.split(['&', '#']) {
            if let Some(id) = pair.strip_prefix("v=") {
                if valid(id) {
                    return Some(id.to_string());
                }
            }
        }
    }
    let rest = url.split("youtu.be/").nth(1)?;
    let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    valid(&id).then_some(id)
}

fn parse_length(text: &str) -> Option<Duration> {
    let mut seconds = 0u64;
    for part in text.split(':') {
        seconds = seconds * 60 + part.trim().parse::<u64>().ok()?;
    }
    Some(Duration::from_secs(seconds))
}

fn runs_text(value: Option<&Value>) -> String {
    value
        .and_then(|node| node.get("runs"))
        .and_then(Value::as_array)
        .map(|runs| runs.iter().filter_map(|run| run.get("text").and_then(Value::as_str)).collect::<String>())
        .unwrap_or_default()
}

fn collect(node: &Value, tracks: &mut Vec<Track>, token: &mut Option<String>) {
    match node {
        Value::Object(map) => {
            if let Some(renderer) = map.get("playlistPanelVideoRenderer") {
                if let Some(track) = track_from(renderer) {
                    tracks.push(track);
                }
                return;
            }
            if let Some(next) = map.get("nextRadioContinuationData") {
                if let Some(value) = next.get("continuation").and_then(Value::as_str) {
                    *token = Some(value.to_string());
                }
            }
            for value in map.values() {
                collect(value, tracks, token);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect(item, tracks, token);
            }
        }
        _ => {}
    }
}

fn track_from(renderer: &Value) -> Option<Track> {
    let id = renderer.get("videoId")?.as_str()?;
    if id.is_empty() {
        return None;
    }
    let title = runs_text(renderer.get("title"));
    if title.trim().is_empty() {
        return None;
    }
    let artist = runs_text(renderer.get("shortBylineText"));
    let duration = parse_length(&runs_text(renderer.get("lengthText")));
    let mut track = Track::yt(id, title.trim(), artist.trim(), duration);
    let art = renderer
        .get("thumbnail")
        .and_then(|node| node.get("thumbnails"))
        .and_then(Value::as_array)
        .and_then(|list| list.last())
        .and_then(|item| item.get("url"))
        .and_then(Value::as_str);
    if let Some(url) = art.filter(|url| !url.contains("ytimg.com")) {
        track.thumb = Some(url.to_string());
    }
    Some(track)
}

pub fn parse(json: &Value) -> (Vec<Track>, Option<String>) {
    let mut tracks = Vec::new();
    let mut token = None;
    collect(json, &mut tracks, &mut token);
    (tracks, token)
}

fn post(body: Value) -> Result<Value, String> {
    let agent = crate::http::fetch_agent();
    let mut response = agent
        .post(ENDPOINT)
        .header("Content-Type", "application/json")
        .header("Origin", "https://music.youtube.com")
        .header("User-Agent", USER_AGENT)
        .send(body.to_string())
        .map_err(|err| format!("YouTube Music radio request failed: {err}"))?;
    let text = response
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_string()
        .map_err(|err| format!("YouTube Music radio answer was cut off: {err}"))?;
    serde_json::from_str(&text).map_err(|err| format!("YouTube Music radio answered with unreadable data: {err}"))
}

fn context() -> Value {
    json!({ "client": { "clientName": "WEB_REMIX", "clientVersion": CLIENT_VERSION, "hl": "en", "gl": "US" } })
}

pub fn fetch(seed: &str, limit: usize) -> Result<Vec<Track>, String> {
    let first = post(json!({
        "context": context(),
        "videoId": seed,
        "playlistId": format!("RDAMVM{seed}"),
        "params": "wAEB",
        "isAudioOnly": true,
    }))?;
    let (tracks, mut token) = parse(&first);
    let mut all = crate::ytdlp::dedupe_tracks(tracks);
    let mut pages = 1;
    while all.len() < limit && pages < MAX_PAGES {
        let Some(current) = token.take() else {
            break;
        };
        let next = post(json!({ "context": context(), "continuation": current }))?;
        let (more, next_token) = parse(&next);
        if more.is_empty() {
            break;
        }
        all.extend(more);
        all = crate::ytdlp::dedupe_tracks(all);
        token = next_token;
        pages += 1;
    }
    all.truncate(limit);
    if all.is_empty() {
        Err("YouTube Music returned no radio tracks for that song".to_string())
    } else {
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("fixtures/ytm_next_sample.json");

    #[test]
    fn parses_tracks_artist_duration_and_next_page_token() {
        let json: Value = serde_json::from_str(SAMPLE).expect("fixture");
        let (tracks, token) = parse(&json);
        assert_eq!(tracks.len(), 3, "broken entries are skipped");
        assert!(tracks.iter().all(|t| !t.title.is_empty() && t.yt_id().is_some()));
        assert!(tracks.iter().any(|t| t.title.contains("Legends Never Die")));
        let legends = tracks.iter().find(|t| t.title.contains("Legends")).unwrap();
        assert_eq!(legends.artist, "League of Legends");
        assert_eq!(legends.duration, Some(Duration::from_secs(179)));
        assert_eq!(token.as_deref(), Some("TOKEN123"));
    }

    #[test]
    fn finds_the_seed_video_in_common_link_shapes() {
        assert_eq!(
            seed_from_url("https://www.youtube.com/watch?v=KvMY1uzSC1E&list=RDKvMY1uzSC1E&start_radio=1").as_deref(),
            Some("KvMY1uzSC1E")
        );
        assert_eq!(seed_from_url("https://music.youtube.com/watch?list=RDAMVMabc&v=KvMY1uzSC1E").as_deref(), Some("KvMY1uzSC1E"));
        assert_eq!(seed_from_url("https://youtu.be/KvMY1uzSC1E?si=x").as_deref(), Some("KvMY1uzSC1E"));
        assert_eq!(seed_from_url("https://www.youtube.com/playlist?list=PLabc"), None);
        assert_eq!(seed_from_url("https://www.youtube.com/watch?v=short"), None);
    }

    #[test]
    #[ignore = "needs network access to YouTube Music"]
    fn live_radio_returns_a_hundred_unique_tracks() {
        let started = web_time::Instant::now();
        let tracks = fetch("KvMY1uzSC1E", 100).expect("live radio");
        println!("{} tracks in {:?}", tracks.len(), started.elapsed());
        assert!(tracks.len() >= 90, "got {}", tracks.len());
        let mut ids: Vec<u64> = tracks.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), tracks.len());
        assert!(tracks.iter().all(|t| !t.artist.is_empty() || !t.title.is_empty()));
    }

    #[test]
    fn parses_lengths_with_hours() {
        assert_eq!(parse_length("3:40"), Some(Duration::from_secs(220)));
        assert_eq!(parse_length("1:02:03"), Some(Duration::from_secs(3723)));
        assert_eq!(parse_length("live"), None);
    }
}
