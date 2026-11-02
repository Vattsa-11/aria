use std::time::Duration;

use crate::model::Track;

pub const VARIANT_WORDS: [&str; 10] = [
    "live",
    "cover",
    "remix",
    "slowed",
    "sped up",
    "reverb",
    "8d",
    "nightcore",
    "karaoke",
    "instrumental",
];

const PERFECT_WINDOW: f64 = 5.0;
const CLOSE_WINDOW: f64 = 15.0;
const FAR_DRIFT: f64 = 45.0;

fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            gap = false;
        } else if !gap {
            out.push(' ');
            gap = true;
        }
    }
    out.trim().to_string()
}

fn has_phrase(haystack: &str, phrase: &str) -> bool {
    let needle = normalize(phrase);
    if needle.is_empty() {
        return false;
    }
    let hay = format!(" {} ", normalize(haystack));
    hay.contains(&format!(" {needle} "))
}

fn is_topic_channel(artist: &str) -> bool {
    let artist = normalize(artist);
    artist == "topic" || artist.ends_with(" topic")
}

fn artist_score(candidate: &str, wanted: &str) -> i32 {
    let wanted = normalize(wanted);
    if wanted.is_empty() {
        return 0;
    }
    let mut artist = normalize(candidate);
    if let Some(stripped) = artist.strip_suffix(" topic") {
        artist = stripped.to_string();
    }
    if artist == wanted {
        return 25;
    }
    if artist.contains(&wanted) || wanted.contains(&artist) {
        return 18;
    }
    let guarded = format!(" {artist} ");
    let shared = wanted
        .split(' ')
        .filter(|word| word.len() >= 3)
        .any(|word| guarded.contains(&format!(" {word} ")));
    if shared {
        8
    } else {
        0
    }
}

pub fn duration_score(candidate: Option<Duration>, wanted: Option<Duration>) -> i32 {
    let (Some(candidate), Some(wanted)) = (candidate, wanted) else {
        return 0;
    };
    let wanted_secs = wanted.as_secs_f64();
    if wanted_secs <= 0.0 {
        return 0;
    }
    let drift = (candidate.as_secs_f64() - wanted_secs).abs();
    if drift <= PERFECT_WINDOW {
        20
    } else if drift <= CLOSE_WINDOW {
        6
    } else if drift > FAR_DRIFT {
        -15
    } else {
        0
    }
}

pub fn score(
    candidate_title: &str,
    candidate_artist: &str,
    wanted_title: &str,
    wanted_artist: &str,
) -> i32 {
    let title = normalize(candidate_title);
    let artist = normalize(candidate_artist);
    let wanted_title_norm = normalize(wanted_title);
    let wanted_combined = format!("{wanted_title_norm} {}", normalize(wanted_artist));
    let mut total = 0;

    if is_topic_channel(candidate_artist) {
        total += 40;
    }
    if has_phrase(candidate_title, "official audio") {
        total += 30;
    }
    if has_phrase(candidate_title, "official video") || has_phrase(candidate_title, "official music video") {
        total += 12;
    }
    if has_phrase(candidate_title, "lyrics") || has_phrase(candidate_title, "lyric video") {
        total += 6;
    }
    if !wanted_title_norm.is_empty() && title.contains(&wanted_title_norm) {
        total += 30;
    }
    total += artist_score(candidate_artist, wanted_artist);

    let candidate_combined = format!("{title} {artist}");
    for word in VARIANT_WORDS {
        if has_phrase(&candidate_combined, word) && !has_phrase(&wanted_combined, word) {
            total -= 45;
        }
    }
    total
}

pub fn score_track(
    candidate: &Track,
    wanted_title: &str,
    wanted_artist: &str,
    wanted_duration: Option<Duration>,
) -> i32 {
    score(candidate.title.as_str(), &candidate.artist, wanted_title, wanted_artist)
        + duration_score(candidate.duration, wanted_duration)
}

pub fn rank(
    tracks: &mut [Track],
    wanted_title: &str,
    wanted_artist: &str,
    wanted_duration: Option<Duration>,
) {
    tracks.sort_by_key(|track| {
        std::cmp::Reverse(score_track(
            track,
            wanted_title,
            wanted_artist,
            wanted_duration,
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str, title: &str, artist: &str, secs: u64) -> Track {
        Track::yt(id, title, artist, Some(Duration::from_secs(secs)))
    }

    #[test]
    fn topic_channels_and_official_audio_outrank_plain_uploads() {
        let topic = score("Song", "Artist - Topic", "Song", "Artist");
        let official = score("Song (Official Audio)", "Artist", "Song", "Artist");
        let plain = score("Song", "Random Uploader", "Song", "Artist");
        assert!(topic > official, "{topic} vs {official}");
        assert!(official > plain, "{official} vs {plain}");
    }

    #[test]
    fn every_penalised_variant_loses_to_the_original() {
        let original = score("Song", "Artist - Topic", "Song", "Artist");
        for word in VARIANT_WORDS {
            let variant = score(
                &format!("Song ({word})"),
                "Artist - Topic",
                "Song",
                "Artist",
            );
            assert!(variant < original, "{word}: {variant} vs {original}");
        }
    }

    #[test]
    fn a_variant_is_kept_when_the_original_title_asks_for_it() {
        for word in VARIANT_WORDS {
            let wanted = format!("Song ({word})");
            let variant = score(&format!("Song ({word})"), "Artist", &wanted, "Artist");
            let plain = score("Song", "Artist", &wanted, "Artist");
            assert!(variant >= plain, "{word}: {variant} vs {plain}");
        }
    }

    #[test]
    fn variant_words_inside_longer_words_are_ignored() {
        assert!(!has_phrase("discovery channel", "cover"));
        assert!(!has_phrase("discovery mix", "remix"));
        assert!(has_phrase("song live at wembley", "live"));
        let plain = score("Song", "Artist", "Song", "Artist");
        let inside = score("Song (Discovery Mix)", "Artist", "Song", "Artist");
        assert_eq!(plain, inside);
    }

    #[test]
    fn duration_within_five_seconds_beats_a_far_off_cut() {
        let exact = track("a", "Song", "Artist", 202);
        let close = track("b", "Song", "Artist", 214);
        let live = track("c", "Song", "Artist", 260);
        let wanted = Some(Duration::from_secs(200));
        assert_eq!(duration_score(exact.duration, wanted), 20);
        assert_eq!(duration_score(close.duration, wanted), 6);
        assert_eq!(duration_score(live.duration, wanted), -15);
        assert!(score_track(&exact, "Song", "Artist", wanted) > score_track(&live, "Song", "Artist", wanted));
    }

    #[test]
    fn a_long_drift_is_penalised() {
        let standard = track("a", "Song", "Artist", 200);
        let extended = track("b", "Song", "Artist", 600);
        let wanted = Some(Duration::from_secs(200));
        assert_eq!(duration_score(extended.duration, wanted), -15);
        assert!(score_track(&standard, "Song", "Artist", wanted) > score_track(&extended, "Song", "Artist", wanted));
    }

    #[test]
    fn duration_is_ignored_when_either_side_is_unknown() {
        assert_eq!(duration_score(None, Some(Duration::from_secs(200))), 0);
        assert_eq!(duration_score(Some(Duration::from_secs(200)), None), 0);
    }

    #[test]
    fn the_matching_artist_name_scores_higher() {
        let right = score("Song", "The Artist", "Song", "Artist");
        let wrong = score("Song", "Someone Else", "Song", "Artist");
        assert!(right > wrong, "{right} vs {wrong}");
        assert!(artist_score("Artist - Topic", "Artist") > artist_score("Other Band", "Artist"));
    }

    #[test]
    fn ranking_puts_the_best_match_first() {
        let mut tracks = vec![
            track("a", "Song (Cover)", "Random", 210),
            track("b", "Song (Official Audio)", "Artist - Topic", 201),
            track("c", "Song", "Artist", 330),
        ];
        rank(&mut tracks, "Song", "Artist", Some(Duration::from_secs(200)));
        assert_eq!(tracks[0].yt_id().as_deref(), Some("b"));
        assert_eq!(tracks[2].yt_id().as_deref(), Some("a"));
    }

    #[test]
    fn ranking_a_sample_import_list_orders_every_row() {
        let mut tracks = vec![
            track("live", "Song (Live at Wembley)", "Artist", 240),
            track("slowed", "Song (slowed + reverb)", "Nightcore Kid", 190),
            track("studio", "Song", "Artist - Topic", 202),
            track("remix", "Song (Remix)", "DJ Someone", 205),
        ];
        rank(&mut tracks, "Song", "Artist", Some(Duration::from_secs(200)));
        let order: Vec<Option<String>> = tracks.iter().map(|t| t.yt_id()).collect();
        assert_eq!(
            order,
            vec![
                Some("studio".to_string()),
                Some("live".to_string()),
                Some("remix".to_string()),
                Some("slowed".to_string()),
            ]
        );
    }
}
