use std::sync::Mutex;
use std::time::Duration;

use des::cipher::{Block, BlockDecrypt, KeyInit};
use des::Des;
use serde_json::Value;

use crate::model::{Track, TrackSource};
use crate::provider::{MetadataResult, Provider, ProviderName, ResolvedStream};

const DIRECT_BASE: &str = "https://www.jiosaavn.com/api.php";
const PROXY_BASE: &str = "https://saavn.dev/api";
const MEDIA_KEY: &[u8; 8] = b"38346591";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Aria/0.1";
const API_PARAMS: &str = "_format=json&_marker=0&api_version=4&ctx=web6dot0";

static BASE_OVERRIDE: Mutex<String> = Mutex::new(String::new());

pub fn set_base(base: &str) {
    if let Ok(mut slot) = BASE_OVERRIDE.lock() {
        *slot = base.trim().to_string();
    }
}

fn configured_base() -> String {
    BASE_OVERRIDE
        .lock()
        .map(|slot| slot.clone())
        .unwrap_or_default()
}

pub fn ordered_bases(env: Option<&str>, configured: &str) -> Vec<String> {
    let mut bases: Vec<String> = Vec::new();
    let mut push = |base: &str| {
        let base = base.trim().trim_end_matches('/').to_string();
        if !base.is_empty() && !bases.iter().any(|existing| existing == &base) {
            bases.push(base);
        }
    };
    if let Some(env) = env {
        push(env);
    }
    push(configured);
    push(DIRECT_BASE);
    push(PROXY_BASE);
    bases
}

fn bases() -> Vec<String> {
    let env = std::env::var("ARIA_JIOSAAVN_BASE").ok();
    ordered_bases(env.as_deref(), &configured_base())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Direct,
    Proxy,
}

fn mode_of(base: &str) -> Mode {
    if base.contains("api.php") {
        Mode::Direct
    } else {
        Mode::Proxy
    }
}

fn encode_query(query: &str) -> String {
    let mut out = String::new();
    for ch in query.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else if ch == ' ' {
            out.push('+');
        } else {
            let mut buffer = [0u8; 4];
            for byte in ch.encode_utf8(&mut buffer).as_bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

fn get_json(url: &str) -> Result<Value, String> {
    let agent = crate::http::fetch_agent();
    let response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|err| err.to_string())?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|err| err.to_string())?;
    serde_json::from_str(&text).map_err(|_| "JioSaavn returned an unexpected response".to_string())
}

fn string_of(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::String(text)) => text.eq_ignore_ascii_case("true"),
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0) > 0.0,
        _ => false,
    }
}

fn duration_of(value: Option<&Value>) -> Option<Duration> {
    let secs = match value? {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if secs.is_finite() && secs > 0.0 {
        Some(Duration::from_secs_f64(secs))
    } else {
        None
    }
}

fn image_of(value: Option<&Value>) -> Option<String> {
    image_raw(value).map(|url| crate::model::upgrade_jiosaavn_image(&url))
}

fn image_raw(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Array(items) => items
            .iter()
            .rev()
            .find_map(|item| string_of(item.get("link")).or_else(|| item.as_str().map(String::from))),
        Value::Object(map) => string_of(map.get("link")),
        _ => None,
    }
}

fn artist_of(entry: &Value) -> String {
    if let Some(artist) = string_of(entry.get("primaryArtists"))
        .or_else(|| string_of(entry.get("primary_artists")))
        .or_else(|| string_of(entry.get("artist")))
    {
        return artist;
    }
    if let Some(subtitle) = string_of(entry.get("subtitle")) {
        let artists = subtitle.split(" - ").next().unwrap_or(&subtitle);
        return artists.trim().to_string();
    }
    if let Some(map) = entry
        .get("more_info")
        .and_then(|info| info.get("artistMap"))
        .and_then(|map| map.get("primary_artists"))
    {
        if let Some(items) = map.as_array() {
            let names: Vec<String> = items
                .iter()
                .filter_map(|item| string_of(item.get("name")))
                .collect();
            if !names.is_empty() {
                return names.join(", ");
            }
        }
    }
    String::new()
}

fn entries_of(json: &Value) -> Vec<Value> {
    if let Some(items) = json.get("results").and_then(Value::as_array) {
        return items.clone();
    }
    if let Some(data) = json.get("data") {
        if let Some(items) = data.get("results").and_then(Value::as_array) {
            return items.clone();
        }
        if let Some(items) = data.as_array() {
            return items.clone();
        }
    }
    if let Some(items) = json.get("songs").and_then(Value::as_array) {
        return items.clone();
    }
    Vec::new()
}

pub fn parse_search(json: &Value) -> Vec<MetadataResult> {
    let mut results = Vec::new();
    for entry in entries_of(json) {
        let Some(track_id) = string_of(entry.get("id")) else {
            continue;
        };
        let title = string_of(entry.get("name"))
            .or_else(|| string_of(entry.get("title")))
            .or_else(|| string_of(entry.get("song")))
            .unwrap_or_else(|| "Unknown title".to_string());
        let duration = duration_of(
            entry
                .get("duration")
                .or_else(|| entry.get("more_info").and_then(|info| info.get("duration"))),
        );
        let thumb = image_of(entry.get("image"));
        results.push(MetadataResult {
            track_id,
            title,
            artist: artist_of(&entry),
            duration,
            thumb,
            provider: ProviderName::Jiosaavn,
        });
    }
    results
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut table = [255u8; 256];
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for (index, byte) in alphabet.iter().enumerate() {
        table[*byte as usize] = index as u8;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for ch in input.bytes() {
        if ch == b'=' || ch.is_ascii_whitespace() {
            continue;
        }
        let value = table[ch as usize];
        if value == 255 {
            return Err("JioSaavn token was not valid base64".to_string());
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Ok(out)
}

pub fn decrypt_media(encrypted: &str) -> Result<String, String> {
    let raw = decode_base64(encrypted)?;
    if raw.is_empty() || raw.len() % 8 != 0 {
        return Err("JioSaavn token had an unexpected length".to_string());
    }
    let cipher = Des::new_from_slice(MEDIA_KEY).map_err(|err| err.to_string())?;
    let mut plain = Vec::with_capacity(raw.len());
    for chunk in raw.chunks(8) {
        let mut block = Block::<Des>::default();
        block.copy_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        plain.extend_from_slice(&block);
    }
    if let Some(&last) = plain.last() {
        let pad = last as usize;
        if (1..=8).contains(&pad) && plain.len() >= pad {
            plain.truncate(plain.len() - pad);
        }
    }
    String::from_utf8(plain).map_err(|_| "JioSaavn token did not decode to text".to_string())
}

pub fn upgrade_quality(url: &str, high: bool) -> (String, u16) {
    if high {
        if url.contains("_96.") {
            return (url.replace("_96.", "_320."), 320);
        }
        if url.contains("_320.") {
            return (url.to_string(), 320);
        }
    }
    let bitrate = if url.contains("_320.") { 320 } else { 96 };
    (url.to_string(), bitrate)
}

pub fn parse_stream(json: &Value) -> Result<(String, Option<Duration>, u16), String> {
    let entries = entries_of(json);
    let entry = entries
        .first()
        .ok_or_else(|| "JioSaavn returned no song details".to_string())?;

    if let Some(downloads) = entry.get("downloadUrl").and_then(Value::as_array) {
        let mut best: Option<(u16, String)> = None;
        for item in downloads {
            let Some(url) = string_of(item.get("link")) else {
                continue;
            };
            let quality = item
                .get("quality")
                .and_then(|quality| match quality {
                    Value::String(text) => text.trim_end_matches("kbps").parse::<u16>().ok(),
                    Value::Number(number) => number.as_u64().map(|value| value as u16),
                    _ => None,
                })
                .unwrap_or(0);
            if best.as_ref().map(|(kbps, _)| quality > *kbps).unwrap_or(true) {
                best = Some((quality, url));
            }
        }
        if let Some((kbps, url)) = best {
            let duration = duration_of(
                entry
                    .get("duration")
                    .or_else(|| entry.get("more_info").and_then(|info| info.get("duration"))),
            );
            return Ok((url, duration, kbps));
        }
    }

    if let Some(url) = string_of(entry.get("media_url"))
        .or_else(|| string_of(entry.get("mediaUrl")))
        .or_else(|| string_of(entry.get("url")))
    {
        let duration = duration_of(
            entry
                .get("duration")
                .or_else(|| entry.get("more_info").and_then(|info| info.get("duration"))),
        );
        let (url, kbps) = upgrade_quality(&url, true);
        return Ok((url, duration, kbps));
    }

    let info = entry
        .get("more_info")
        .ok_or_else(|| "JioSaavn returned no media URL".to_string())?;
    let encrypted = string_of(info.get("encrypted_media_url"))
        .or_else(|| string_of(info.get("encrypted_drm_media_url")))
        .ok_or_else(|| "JioSaavn returned no media URL".to_string())?;
    let high = truthy(info.get("320kbps"));
    let decrypted = decrypt_media(&encrypted)?;
    let duration = duration_of(info.get("duration"));
    let (url, kbps) = upgrade_quality(&decrypted, high);
    Ok((url, duration, kbps))
}

fn search_at(base: &str, query: &str, limit: usize) -> Result<Vec<MetadataResult>, String> {
    let encoded = encode_query(query);
    let url = match mode_of(base) {
        Mode::Direct => format!(
            "{base}?__call=search.getResults&q={encoded}&n={limit}&{API_PARAMS}"
        ),
        Mode::Proxy => format!(
            "{base}/search/songs?query={encoded}&limit={limit}"
        ),
    };
    let json = get_json(&url)?;
    Ok(parse_search(&json))
}

fn resolve_at(base: &str, track_id: &str) -> Result<(String, Option<Duration>, u16), String> {
    let url = match mode_of(base) {
        Mode::Direct => format!("{base}?__call=song.getDetails&pids={track_id}&{API_PARAMS}"),
        Mode::Proxy => format!("{base}/songs/{track_id}"),
    };
    let json = get_json(&url)?;
    parse_stream(&json)
}

fn join_paths(errors: Vec<String>) -> String {
    if errors.is_empty() {
        "JioSaavn is unreachable".to_string()
    } else {
        errors.join("; ")
    }
}

pub struct JiosaavnProvider;

impl Default for JiosaavnProvider {
    fn default() -> Self {
        Self
    }
}

impl Provider for JiosaavnProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Jiosaavn
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<MetadataResult>, String> {
        let mut errors = Vec::new();
        for base in bases() {
            match search_at(&base, query, limit) {
                Ok(results) if !results.is_empty() => return Ok(results),
                Ok(_) => errors.push(format!("{}: no results", mode_name(&base))),
                Err(err) => errors.push(format!("{}: {err}", mode_name(&base))),
            }
        }
        Err(join_paths(errors))
    }

    fn resolve(&self, track: &Track) -> Result<Option<ResolvedStream>, String> {
        let track_id = match &track.source {
            TrackSource::Jiosaavn { id } => id.clone(),
            _ => return Ok(None),
        };
        let mut errors = Vec::new();
        for base in bases() {
            match resolve_at(&base, &track_id) {
                Ok((url, duration, bitrate)) => {
                    return Ok(Some(ResolvedStream {
                        url,
                        headers: vec![("User-Agent".to_string(), USER_AGENT.to_string())],
                        duration: duration.or(track.duration),
                        provider: ProviderName::Jiosaavn,
                        bitrate_kbps: Some(bitrate),
                    }))
                }
                Err(err) => errors.push(format!("{}: {err}", mode_name(&base))),
            }
        }
        Err(join_paths(errors))
    }
}

fn mode_name(base: &str) -> &'static str {
    match mode_of(base) {
        Mode::Direct => "direct",
        Mode::Proxy => "proxy",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECT_SEARCH: &str = r#"{
        "total": 2,
        "results": [
            {
                "id": "1gHtmQ3x",
                "title": "Gehra Hua",
                "subtitle": "Irshad Kamil, Arijit Singh - Dhurandhar",
                "image": "https://c.saavncdn.com/475/cover-150x150.jpg",
                "more_info": { "duration": "362", "320kbps": "true" }
            },
            {
                "id": 553426,
                "title": "Tum Hi Ho",
                "subtitle": "Mithoon, Arijit Singh - Aashiqui 2",
                "image": "",
                "more_info": { "duration": 262 }
            }
        ]
    }"#;

    const PROXY_SEARCH: &str = r#"{
        "data": {
            "total": 1,
            "results": [
                {
                    "id": "abc123",
                    "name": "Kesariya",
                    "primaryArtists": "Arijit Singh, Pritam",
                    "duration": 254,
                    "image": [
                        { "quality": "50x50", "link": "https://c.saavncdn.com/small.jpg" },
                        { "quality": "500x500", "link": "https://c.saavncdn.com/large.jpg" }
                    ]
                }
            ]
        }
    }"#;

    const DETAILS: &str = r#"{
        "songs": [
            {
                "id": "1gHtmQ3x",
                "title": "Gehra Hua",
                "more_info": {
                    "duration": "362",
                    "320kbps": true,
                    "encrypted_media_url": "ID2ieOjCrwfgWvL5sXl4B1ImC5QfbsDy/1hrAoFVeiTYsLR09FjcMgF48Zl3V1EPt4oAnp6wVpuEZ9KOfCiKyhw7tS9a8Gtq"
                }
            }
        ]
    }"#;

    const PROXY_DETAILS: &str = r#"{
        "data": [
            {
                "id": "abc123",
                "name": "Kesariya",
                "duration": 254,
                "downloadUrl": [
                    { "quality": "96kbps", "link": "https://aac.saavncdn.com/1/kesariya_96.mp4" },
                    { "quality": "320kbps", "link": "https://aac.saavncdn.com/1/kesariya_320.mp4" },
                    { "quality": "12kbps", "link": "https://aac.saavncdn.com/1/kesariya_12.mp4" }
                ]
            }
        ]
    }"#;

    #[test]
    fn parses_direct_search_results() {
        let json: Value = serde_json::from_str(DIRECT_SEARCH).expect("fixture");
        let results = parse_search(&json);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].track_id, "1gHtmQ3x");
        assert_eq!(results[0].title, "Gehra Hua");
        assert_eq!(results[0].artist, "Irshad Kamil, Arijit Singh");
        assert_eq!(results[0].duration, Some(Duration::from_secs(362)));
        assert_eq!(
            results[0].thumb.as_deref(),
            Some("https://c.saavncdn.com/475/cover-500x500.jpg")
        );
        assert_eq!(results[1].track_id, "553426");
        assert_eq!(results[1].duration, Some(Duration::from_secs(262)));
        assert!(results[1].thumb.is_none());
        assert_eq!(results[0].provider, ProviderName::Jiosaavn);
    }

    #[test]
    fn parses_proxy_search_results() {
        let json: Value = serde_json::from_str(PROXY_SEARCH).expect("fixture");
        let results = parse_search(&json);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Kesariya");
        assert_eq!(results[0].artist, "Arijit Singh, Pritam");
        assert_eq!(
            results[0].thumb.as_deref(),
            Some("https://c.saavncdn.com/large.jpg")
        );
    }

    #[test]
    fn decrypts_the_known_media_token() {
        let json: Value = serde_json::from_str(DETAILS).expect("fixture");
        let (url, duration, bitrate) = parse_stream(&json).expect("stream");
        assert!(url.starts_with("https://aac.saavncdn.com/475/"), "{url}");
        assert!(url.ends_with(".mp4"), "{url}");
        assert!(url.contains("_320.mp4"), "{url}");
        assert_eq!(duration, Some(Duration::from_secs(362)));
        assert_eq!(bitrate, 320);
    }

    #[test]
    fn picks_the_highest_proxy_quality() {
        let json: Value = serde_json::from_str(PROXY_DETAILS).expect("fixture");
        let (url, duration, bitrate) = parse_stream(&json).expect("stream");
        assert!(url.ends_with("kesariya_320.mp4"), "{url}");
        assert_eq!(duration, Some(Duration::from_secs(254)));
        assert_eq!(bitrate, 320);
    }

    #[test]
    fn quality_upgrade_only_touches_the_bitrate_marker() {
        let (url, kbps) = upgrade_quality("https://aac.saavncdn.com/1/song_96.mp4", true);
        assert_eq!(url, "https://aac.saavncdn.com/1/song_320.mp4");
        assert_eq!(kbps, 320);

        let (url, kbps) = upgrade_quality("https://aac.saavncdn.com/1/song_96.mp4", false);
        assert_eq!(url, "https://aac.saavncdn.com/1/song_96.mp4");
        assert_eq!(kbps, 96);
    }

    #[test]
    fn bad_tokens_report_an_error_instead_of_panicking() {
        assert!(decrypt_media("not base64!!").is_err());
        assert!(decrypt_media("").is_err());
        assert!(decrypt_media("QUJD").is_err());
    }

    #[test]
    fn base_order_prefers_override_then_direct_then_proxy() {
        let bases = ordered_bases(None, "");
        assert_eq!(bases[0], DIRECT_BASE);
        assert_eq!(bases[1], PROXY_BASE);

        let bases = ordered_bases(Some("https://example.test/api/"), "https://custom.test/api");
        assert_eq!(bases[0], "https://example.test/api");
        assert_eq!(bases[1], "https://custom.test/api");
        assert!(bases.contains(&DIRECT_BASE.to_string()));
        assert!(bases.contains(&PROXY_BASE.to_string()));

        let bases = ordered_bases(Some(" https://custom.test/api "), "https://custom.test/api");
        assert_eq!(
            bases.iter().filter(|base| *base == "https://custom.test/api").count(),
            1
        );
    }

    #[test]
    fn base_url_selects_the_request_shape() {
        assert_eq!(mode_of(DIRECT_BASE), Mode::Direct);
        assert_eq!(mode_of("https://saavn.dev/api"), Mode::Proxy);
        assert_eq!(encode_query("arijit singh"), "arijit+singh");
        assert_eq!(encode_query("A R Rahman & Co"), "A+R+Rahman+%26+Co");
    }
}
