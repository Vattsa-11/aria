use std::time::Duration;

use crate::model::Track;

#[derive(Clone, Debug, PartialEq)]
pub enum MatchState {
    Pending,
    Searching,
    Matched,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedEntry {
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
}

impl ParsedEntry {
    fn new(title: String, artist: String, duration: Option<Duration>) -> Self {
        Self {
            title,
            artist,
            duration,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImportEntry {
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
    pub matched: Option<Track>,
    pub alternatives: Vec<Track>,
    pub skipped: bool,
    pub state: MatchState,
}

impl ImportEntry {
    pub fn with_duration(title: String, artist: String, duration: Option<Duration>) -> Self {
        Self {
            title,
            artist,
            duration,
            matched: None,
            alternatives: Vec::new(),
            skipped: false,
            state: MatchState::Pending,
        }
    }

    pub fn label(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} — {}", self.artist, self.title)
        }
    }

    pub fn query(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} {}", self.artist, self.title)
        }
    }
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn column(fields: &[String], names: &[&str]) -> Option<usize> {
    fields.iter().position(|field| {
        let field = field.trim().to_lowercase();
        names.iter().any(|name| field == *name)
    })
}

fn parse_clock(text: &str) -> Option<Duration> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let number = |value: &str| value.trim().parse::<f64>().ok();
    match parts.as_slice() {
        [minutes, seconds] => {
            let minutes = number(minutes)?;
            let seconds = number(seconds)?;
            if minutes < 0.0 || seconds < 0.0 || seconds >= 60.0 {
                return None;
            }
            Some(Duration::from_secs_f64(minutes * 60.0 + seconds))
        }
        [hours, minutes, seconds] => {
            let hours = number(hours)?;
            let minutes = number(minutes)?;
            let seconds = number(seconds)?;
            if hours < 0.0 || minutes < 0.0 || seconds < 0.0 || seconds >= 60.0 {
                return None;
            }
            Some(Duration::from_secs_f64(hours * 3600.0 + minutes * 60.0 + seconds))
        }
        _ => None,
    }
}

pub fn parse_duration(text: &str, millis: bool) -> Option<Duration> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(clock) = parse_clock(text) {
        return Some(clock);
    }
    let value: f64 = text.parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    if millis || value >= 86_400.0 {
        Some(Duration::from_millis(value.round() as u64))
    } else {
        Some(Duration::from_secs_f64(value))
    }
}

fn take_trailing_duration(title: &str) -> (String, Option<Duration>) {
    let trimmed = title.trim_end();
    for (open, close) in [('(', ')'), ('[', ']')] {
        let Some(inner) = trimmed.strip_suffix(close) else {
            continue;
        };
        let Some(start) = inner.rfind(open) else {
            continue;
        };
        let Some(duration) = parse_clock(&inner[start + open.len_utf8()..]) else {
            continue;
        };
        let rest = inner[..start].trim_end();
        if rest.is_empty() {
            continue;
        }
        return (rest.to_string(), Some(duration));
    }
    (trimmed.to_string(), None)
}

pub fn parse_csv(text: &str) -> Vec<ParsedEntry> {
    let rows: Vec<Vec<String>> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(split_csv_line)
        .collect();

    let header_index = rows
        .iter()
        .position(|fields| column(fields, &["track name", "title", "name"]).is_some());
    let Some(header_index) = header_index else {
        return Vec::new();
    };
    let header = &rows[header_index];
    let Some(title_col) = column(header, &["track name", "title", "name"]) else {
        return Vec::new();
    };
    let artist_col = column(
        header,
        &["artist name(s)", "artist name", "artists", "artist"],
    );
    let millis_col = column(
        header,
        &[
            "duration (ms)",
            "duration ms",
            "duration_ms",
            "duration(milliseconds)",
        ],
    );
    let duration_col = millis_col.or_else(|| {
        column(
            header,
            &["duration", "track duration", "duration (s)", "length"],
        )
    });

    let mut entries = Vec::new();
    for fields in &rows[header_index + 1..] {
        let Some(title) = fields.get(title_col) else {
            continue;
        };
        if title.is_empty() {
            continue;
        }
        let artist = artist_col
            .and_then(|index| fields.get(index))
            .cloned()
            .unwrap_or_default();
        let is_millis = millis_col.is_some() && duration_col == millis_col;
        let duration = duration_col
            .and_then(|index| fields.get(index))
            .and_then(|value| parse_duration(value, is_millis));
        entries.push(ParsedEntry::new(title.clone(), artist, duration));
    }
    dedupe(entries)
}

fn strip_numbering(line: &str) -> &str {
    let mut chars = line.char_indices().peekable();
    let mut digits = 0;
    while let Some((index, ch)) = chars.peek().copied() {
        if ch.is_ascii_digit() {
            digits += 1;
            chars.next();
        } else if digits > 0 && (ch == '.' || ch == ')') {
            let rest = &line[index + ch.len_utf8()..];
            return rest.trim_start_matches([' ', '\t']).trim();
        } else {
            break;
        }
    }
    line
}

pub fn parse_text_list(text: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let line = strip_numbering(line);
        if line.is_empty() {
            continue;
        }
        let parsed = if let Some((left, right)) = line.split_once('\t') {
            (right.trim().to_string(), left.trim().to_string())
        } else if let Some((left, right)) = line.split_once(" - ") {
            (right.trim().to_string(), left.trim().to_string())
        } else if let Some((left, right)) = line.split_once(" – ") {
            (right.trim().to_string(), left.trim().to_string())
        } else {
            let lower = line.to_lowercase();
            match lower.find(" by ") {
                Some(position) => (
                    line[..position].trim().to_string(),
                    line[position + 4..].trim().to_string(),
                ),
                None => (line.to_string(), String::new()),
            }
        };
        let (title, duration) = take_trailing_duration(&parsed.0);
        if title.is_empty() {
            continue;
        }
        entries.push(ParsedEntry::new(title, parsed.1, duration));
    }
    dedupe(entries)
}

pub fn dedupe(entries: Vec<ParsedEntry>) -> Vec<ParsedEntry> {
    let mut seen: Vec<String> = Vec::new();
    let mut unique = Vec::new();
    for entry in entries {
        let key = format!(
            "{}|{}",
            entry.title.trim().to_lowercase(),
            entry.artist.trim().to_lowercase()
        );
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        unique.push(entry);
    }
    unique
}

pub fn entries_from(titles: Vec<ParsedEntry>) -> Vec<ImportEntry> {
    titles
        .into_iter()
        .map(|entry| ImportEntry::with_duration(entry.title, entry.artist, entry.duration))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(entries: Vec<ParsedEntry>) -> Vec<(String, String)> {
        entries
            .into_iter()
            .map(|entry| (entry.title, entry.artist))
            .collect()
    }

    #[test]
    fn csv_handles_quotes_commas_and_unicode() {
        let csv = "\u{feff}Track URI,Track Name,Artist URI(s),Artist Name(s),Album Name\n\
            spotify:track:1,\"Hello, World\",spotify:artist:1,\"Adele, Someone\",Album\n\
            spotify:track:2,Chaiyya Chaiyya,spotify:artist:2,Sukhwinder Singh,Album\n\
            spotify:track:3,\"Song \"\"Quoted\"\"\",spotify:artist:3,Artist 3,Album\n";
        let entries = pairs(parse_csv(csv));
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, "Hello, World");
        assert_eq!(entries[0].1, "Adele, Someone");
        assert_eq!(entries[1].0, "Chaiyya Chaiyya");
        assert_eq!(entries[1].1, "Sukhwinder Singh");
        assert_eq!(entries[2].0, "Song \"Quoted\"");
    }

    #[test]
    fn csv_without_header_or_title_column_is_empty() {
        assert!(parse_csv("just,some,rows\n1,2,3").is_empty());
        assert!(parse_csv("").is_empty());
    }

    #[test]
    fn csv_reads_the_exportify_duration_ms_column() {
        let csv = "Track Name,Artist Name(s),Duration (ms)\n\
            Song A,Artist A,225000\n\
            Song B,Artist B,180500\n\
            Song C,Artist C,\n";
        let entries = parse_csv(csv);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].duration, Some(Duration::from_secs(225)));
        assert_eq!(
            entries[1].duration,
            Some(Duration::from_millis(180_500))
        );
        assert_eq!(entries[2].duration, None);
    }

    #[test]
    fn csv_duration_column_is_case_insensitive() {
        let csv = "track name,artist name(s),DURATION (MS)\nSong,Artist,90000\n";
        let entries = parse_csv(csv);
        assert_eq!(entries[0].duration, Some(Duration::from_secs(90)));
    }

    #[test]
    fn csv_accepts_seconds_and_clock_values() {
        let seconds = "Track Name,Artist Name(s),Duration\nSong A,Artist A,225\n";
        let clock = "Track Name,Artist Name(s),Duration\nSong B,Artist B,3:45\n";
        let long = "Track Name,Artist Name(s),Duration\nSong C,Artist C,1:03:30\n";
        assert_eq!(
            parse_csv(seconds)[0].duration,
            Some(Duration::from_secs(225))
        );
        assert_eq!(parse_csv(clock)[0].duration, Some(Duration::from_secs(225)));
        assert_eq!(
            parse_csv(long)[0].duration,
            Some(Duration::from_secs(3810))
        );
    }

    #[test]
    fn csv_without_a_duration_column_still_imports() {
        let csv = "Track Name,Artist Name(s),Album Name\nSong,Artist,Album\n";
        let entries = parse_csv(csv);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Song");
        assert_eq!(entries[0].duration, None);
    }

    #[test]
    fn text_list_reads_a_trailing_duration() {
        let list = "Artist A - Song A (3:45)\nArtist B - Song B [1:02:03]\nArtist C - Song C\n";
        let entries = parse_text_list(list);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].title, "Song A");
        assert_eq!(entries[0].artist, "Artist A");
        assert_eq!(entries[0].duration, Some(Duration::from_secs(225)));
        assert_eq!(entries[1].title, "Song B");
        assert_eq!(entries[1].duration, Some(Duration::from_secs(3723)));
        assert_eq!(entries[2].title, "Song C");
        assert_eq!(entries[2].duration, None);
    }

    #[test]
    fn parentheses_that_are_not_times_stay_in_the_title() {
        let list = "Artist - Song (Live)\nArtist - Song (3:45\n";
        let entries = parse_text_list(list);
        assert_eq!(entries[0].title, "Song (Live)");
        assert_eq!(entries[0].duration, None);
        assert_eq!(entries[1].title, "Song (3:45");
        assert_eq!(entries[1].duration, None);
    }

    #[test]
    fn text_list_supports_common_shapes() {
        let list = "1. Artist A - Song A\nArtist B\tSong B\nSong C by Artist C\nJust A Title\n\
            # a comment\n\nArtist A - Song A\n";
        let entries = pairs(parse_text_list(list));
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0], ("Song A".to_string(), "Artist A".to_string()));
        assert_eq!(entries[1], ("Song B".to_string(), "Artist B".to_string()));
        assert_eq!(entries[2], ("Song C".to_string(), "Artist C".to_string()));
        assert_eq!(entries[3], ("Just A Title".to_string(), String::new()));
    }

    #[test]
    fn duplicates_are_dropped_case_insensitively() {
        let entries = dedupe(vec![
            ParsedEntry::new("Song".to_string(), "Artist".to_string(), None),
            ParsedEntry::new("song".to_string(), "ARTIST".to_string(), None),
            ParsedEntry::new("Other".to_string(), "Artist".to_string(), None),
        ]);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn entries_build_a_useful_query() {
        let entry = ImportEntry::with_duration("Song".to_string(), "Artist".to_string(), None);
        assert_eq!(entry.query(), "Artist Song");
        assert_eq!(entry.label(), "Artist — Song");
        let bare = ImportEntry::with_duration("Song".to_string(), String::new(), None);
        assert_eq!(bare.query(), "Song");
    }

    #[test]
    fn entries_carry_the_parsed_duration() {
        let entries = entries_from(vec![ParsedEntry::new(
            "Song".to_string(),
            "Artist".to_string(),
            Some(Duration::from_secs(200)),
        )]);
        assert_eq!(entries[0].duration, Some(Duration::from_secs(200)));
    }
}
