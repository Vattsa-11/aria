use std::collections::HashMap;

use crate::model::Track;

pub const RECENT_CAP: usize = 8;

pub fn push_recent(list: &mut Vec<String>, query: &str) {
    let query = query.trim();
    if query.is_empty() {
        return;
    }
    list.retain(|existing| !existing.eq_ignore_ascii_case(query));
    list.insert(0, query.to_string());
    list.truncate(RECENT_CAP);
}

pub fn top_artists(sources: &[&[Track]], count: usize) -> Vec<String> {
    let mut tally: HashMap<String, (usize, String)> = HashMap::new();
    for list in sources {
        for track in list.iter() {
            let artist = track.artist.split([',', '&']).next().unwrap_or("").trim();
            if artist.is_empty() || artist.eq_ignore_ascii_case("unknown artist") {
                continue;
            }
            let entry = tally.entry(artist.to_lowercase()).or_insert((0, artist.to_string()));
            entry.0 += 1;
        }
    }
    let mut ranked: Vec<(usize, String)> = tally.into_values().collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked.into_iter().take(count).map(|(_, name)| name).collect()
}

pub fn quick_starts(language_label: Option<&str>) -> Vec<String> {
    let mut starts = Vec::new();
    if let Some(label) = language_label.filter(|label| !label.is_empty()) {
        starts.push(format!("{label} hits"));
        starts.push(format!("{label} melody"));
    }
    for fixed in ["Top hits", "Lo-fi chill", "Workout", "Romantic", "90s hits", "Acoustic"] {
        starts.push(fixed.to_string());
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn song(artist: &str) -> Track {
        let mut track = Track::local(Path::new("/m/a.mp3"));
        track.artist = artist.to_string();
        track
    }

    #[test]
    fn recent_searches_are_deduplicated_newest_first_and_capped() {
        let mut list = vec!["alpha".to_string(), "beta".to_string()];
        push_recent(&mut list, "  Beta ");
        assert_eq!(list, vec!["Beta", "alpha"]);
        push_recent(&mut list, "");
        push_recent(&mut list, "   ");
        assert_eq!(list.len(), 2);
        for n in 0..20 {
            push_recent(&mut list, &format!("song {n}"));
        }
        assert_eq!(list.len(), RECENT_CAP);
        assert_eq!(list[0], "song 19");
    }

    #[test]
    fn top_artists_rank_by_plays_across_lists_and_skip_unknowns() {
        let history = vec![song("Anirudh"), song("Sid Sriram, Other"), song("Anirudh"), song("")];
        let liked = vec![song("sid sriram"), song("Sid Sriram & Band"), song("Unknown artist"), song("Yuvan")];
        let top = top_artists(&[&history, &liked], 2);
        assert_eq!(top, vec!["Sid Sriram", "Anirudh"]);
        assert!(top_artists(&[&[]], 3).is_empty());
        assert_eq!(top_artists(&[&liked], 1).len(), 1);
    }

    #[test]
    fn quick_starts_lead_with_the_regional_language() {
        assert_eq!(quick_starts(Some("Tamil"))[0], "Tamil hits");
        assert_eq!(quick_starts(None)[0], "Top hits");
        assert_eq!(quick_starts(Some(""))[0], "Top hits");
    }
}
