use serde_json::Value;

pub const LANGUAGES: [(&str, &str); 14] = [
    ("hindi", "Hindi"),
    ("tamil", "Tamil"),
    ("telugu", "Telugu"),
    ("malayalam", "Malayalam"),
    ("kannada", "Kannada"),
    ("bengali", "Bengali"),
    ("marathi", "Marathi"),
    ("punjabi", "Punjabi"),
    ("gujarati", "Gujarati"),
    ("bhojpuri", "Bhojpuri"),
    ("odia", "Odia"),
    ("assamese", "Assamese"),
    ("urdu", "Urdu"),
    ("english", "English"),
];

const STATE_LANGUAGES: [(&str, &str); 22] = [
    ("tamil nadu", "tamil"),
    ("puducherry", "tamil"),
    ("kerala", "malayalam"),
    ("lakshadweep", "malayalam"),
    ("karnataka", "kannada"),
    ("andhra pradesh", "telugu"),
    ("telangana", "telugu"),
    ("maharashtra", "marathi"),
    ("west bengal", "bengali"),
    ("tripura", "bengali"),
    ("punjab", "punjabi"),
    ("chandigarh", "punjabi"),
    ("gujarat", "gujarati"),
    ("odisha", "odia"),
    ("orissa", "odia"),
    ("assam", "assamese"),
    ("bihar", "bhojpuri"),
    ("jharkhand", "bhojpuri"),
    ("jammu and kashmir", "urdu"),
    ("goa", "hindi"),
    ("uttar pradesh", "hindi"),
    ("delhi", "hindi"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Geo {
    pub country_code: String,
    pub region: String,
}

pub fn is_known_language(id: &str) -> bool {
    LANGUAGES.iter().any(|(known, _)| *known == id)
}

pub fn language_label(id: &str) -> &'static str {
    LANGUAGES
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, label)| *label)
        .unwrap_or("")
}

pub fn language_for(geo: &Geo) -> &'static str {
    if !geo.country_code.eq_ignore_ascii_case("IN") {
        return "english";
    }
    let region = geo.region.trim().to_ascii_lowercase();
    STATE_LANGUAGES
        .iter()
        .find(|(state, _)| *state == region)
        .map(|(_, language)| *language)
        .unwrap_or("hindi")
}

pub fn parse_geo(json: &Value) -> Result<Geo, String> {
    if json.get("success").and_then(Value::as_bool) == Some(false) {
        let reason = json.get("message").and_then(Value::as_str).unwrap_or("lookup refused");
        return Err(format!("Location lookup failed: {reason}"));
    }
    let country_code = json
        .get("country_code")
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())
        .ok_or("Location lookup returned no country")?
        .to_string();
    let region = json.get("region").and_then(Value::as_str).unwrap_or("").to_string();
    Ok(Geo { country_code, region })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_geo() -> Result<Geo, String> {
    let mut response = crate::http::fetch_agent()
        .get("https://ipwho.is/?fields=success,message,country_code,region")
        .call()
        .map_err(|err| format!("Location lookup failed: {err}"))?;
    let text = response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_to_string()
        .map_err(|err| format!("Location lookup failed: {err}"))?;
    let json: Value = serde_json::from_str(&text).map_err(|_| "Location lookup returned unreadable data".to_string())?;
    parse_geo(&json)
}

pub fn trending_title(label: Option<&str>, language: &str) -> String {
    match label.filter(|label| !label.trim().is_empty()) {
        Some(place) => format!("Trending in {place}"),
        None => format!("Trending in {}", language_label(language)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn geo(country: &str, region: &str) -> Geo {
        Geo { country_code: country.into(), region: region.into() }
    }

    #[test]
    fn indian_states_map_to_their_language() {
        assert_eq!(language_for(&geo("IN", "Tamil Nadu")), "tamil");
        assert_eq!(language_for(&geo("IN", "KERALA")), "malayalam");
        assert_eq!(language_for(&geo("IN", "Karnataka")), "kannada");
        assert_eq!(language_for(&geo("IN", "Telangana")), "telugu");
        assert_eq!(language_for(&geo("IN", "Andhra Pradesh")), "telugu");
        assert_eq!(language_for(&geo("IN", "West Bengal")), "bengali");
        assert_eq!(language_for(&geo("IN", "Punjab")), "punjabi");
        assert_eq!(language_for(&geo("IN", "Maharashtra")), "marathi");
    }

    #[test]
    fn unknown_indian_regions_default_to_hindi_and_other_countries_to_english() {
        assert_eq!(language_for(&geo("IN", "Madhya Pradesh")), "hindi");
        assert_eq!(language_for(&geo("IN", "")), "hindi");
        assert_eq!(language_for(&geo("US", "California")), "english");
        assert_eq!(language_for(&geo("in", "tamil nadu")), "tamil");
    }

    #[test]
    fn every_mapped_language_is_a_selectable_one() {
        for (_, language) in STATE_LANGUAGES {
            assert!(is_known_language(language), "{language}");
        }
        assert!(!is_known_language("klingon"));
        assert!(!is_known_language("tamil&x=1"));
    }

    #[test]
    fn parses_the_lookup_and_reports_refusals() {
        let ok = json!({"success": true, "country_code": "IN", "region": "Tamil Nadu"});
        assert_eq!(parse_geo(&ok).unwrap(), geo("IN", "Tamil Nadu"));
        let refused = json!({"success": false, "message": "Too many requests"});
        assert!(parse_geo(&refused).unwrap_err().contains("Too many requests"));
        assert!(parse_geo(&json!({"success": true})).is_err());
        assert_eq!(parse_geo(&json!({"country_code": "FR"})).unwrap().region, "");
    }

    #[test]
    #[ignore = "needs network access"]
    fn live_lookup_and_trending() {
        let geo = fetch_geo().expect("geo");
        let language = language_for(&geo);
        println!("detected {geo:?} -> {language}");
        let songs = crate::provider_jiosaavn::trending(language, 24).expect("trending");
        println!("{} songs, first: {} / {}", songs.len(), songs[0].title, songs[0].artist);
        assert!(songs.len() >= 10);
        assert!(songs.iter().all(|song| !song.title.contains("&quot;") && !song.title.is_empty()));
        assert!(songs[0].thumb.as_deref().unwrap_or("").contains("500x500"));
    }

    #[test]
    fn section_title_prefers_the_place_name() {
        assert_eq!(trending_title(Some("Tamil Nadu"), "tamil"), "Trending in Tamil Nadu");
        assert_eq!(trending_title(None, "tamil"), "Trending in Tamil");
        assert_eq!(trending_title(Some("  "), "hindi"), "Trending in Hindi");
    }
}
