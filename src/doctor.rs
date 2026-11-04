use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub suggestion: String,
}

impl Finding {
    pub fn to_json(&self) -> Value {
        json!({
            "severity": self.severity.label(),
            "title": self.title,
            "detail": self.detail,
            "suggestion": self.suggestion,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub ytdlp_version: Option<String>,
    pub online: bool,
    pub lines: Vec<String>,
    pub window_width: f32,
    pub overflow: bool,
    pub today_days: Option<i64>,
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn version_age_days(version: &str, today_days: i64) -> Option<i64> {
    let mut parts = version.trim().split('.').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
    if !(2000..=2100).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(today_days - days_from_civil(year, month, day))
}

pub fn today_days() -> Option<i64> {
    let secs = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).ok()?.as_secs();
    Some((secs / 86_400) as i64)
}

fn count(lines: &[String], needle: &str) -> usize {
    let needle = needle.to_lowercase();
    lines.iter().filter(|line| line.to_lowercase().contains(&needle)).count()
}

fn first(lines: &[String], needle: &str) -> String {
    let needle = needle.to_lowercase();
    lines
        .iter()
        .find(|line| line.to_lowercase().contains(&needle))
        .map(|line| line.chars().take(220).collect())
        .unwrap_or_default()
}

pub fn analyze(facts: &Facts) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut add = |severity, title: &str, detail: String, suggestion: &str| {
        findings.push(Finding { severity, title: title.to_string(), detail, suggestion: suggestion.to_string() });
    };

    let old_ytdlp = match (facts.ytdlp_version.as_deref(), facts.today_days) {
        (Some(version), Some(today)) => version_age_days(version, today).is_some_and(|age| age > 90),
        _ => false,
    };
    match facts.ytdlp_version.as_deref() {
        None => add(
            Severity::Error,
            "yt-dlp was not found",
            "YouTube and YouTube Music songs cannot be played or downloaded.".to_string(),
            "Open Settings and press Update yt-dlp, or install the Windows MSI or EXE, which include it.",
        ),
        Some(version) if old_ytdlp => add(
            Severity::Warn,
            "yt-dlp is old",
            format!("Version {version} is more than 90 days old. YouTube changes often and old versions stop working."),
            "Open Settings and press Update yt-dlp.",
        ),
        _ => {}
    }

    let format_errors = count(&facts.lines, "Requested format is not available");
    if format_errors > 0 {
        let suggestion = if old_ytdlp {
            "Update yt-dlp first (Settings, Update yt-dlp). This error is the classic sign of an outdated yt-dlp."
        } else {
            "That video may be restricted or need a sign-in. Try the JioSaavn or YT Music result for the same song, or enable Settings, YouTube sign-in."
        };
        add(
            Severity::Error,
            "A YouTube song could not be played (no usable audio format)",
            format!("{format_errors} time(s). Example: {}", first(&facts.lines, "Requested format is not available")),
            suggestion,
        );
    }
    let sign_in = count(&facts.lines, "Please sign in");
    if sign_in > 0 {
        add(
            Severity::Warn,
            "YouTube asked for a sign-in",
            format!("{sign_in} time(s). YouTube sometimes blocks anonymous requests from one connection."),
            "Use JioSaavn or YT Music results, update yt-dlp, or turn on Settings, YouTube sign-in with a separate account.",
        );
    }
    let locked = count(&facts.lines, "database is locked");
    if locked > 0 {
        add(
            Severity::Warn,
            "The database was locked",
            format!("{locked} time(s). Saving history or the queue failed."),
            "Another copy of Aria is probably running. Close every Aria window and start it once.",
        );
    }
    let forbidden = count(&facts.lines, "http status: 403");
    if forbidden > 0 {
        add(
            Severity::Warn,
            "A server refused a request (HTTP 403)",
            format!("{forbidden} time(s)."),
            "Usually an expired or blocked stream link. Playing the song again fetches a fresh one.",
        );
    }
    if count(&facts.lines, "Audio device unavailable") > 0 {
        add(
            Severity::Error,
            "No audio output device",
            first(&facts.lines, "Audio device unavailable"),
            "Check that a speaker or headphones are connected and selected, then restart Aria.",
        );
    }
    let thumbs = count(&facts.lines, "thumbnail");
    if thumbs >= 5 {
        add(
            Severity::Info,
            "Several cover images failed to load",
            format!("{thumbs} thumbnail problems. Example: {}", first(&facts.lines, "thumbnail")),
            "Usually a connection hiccup. Covers retry after a minute; scrolling reloads them.",
        );
    }
    if !facts.online {
        add(
            Severity::Info,
            "Aria is offline",
            "Streaming and search need a connection.".to_string(),
            "Reconnect, or play downloaded and local songs from Downloads and Library.",
        );
    }
    if facts.overflow {
        add(
            Severity::Warn,
            "Content is wider than the window",
            format!("At {:.0} px wide something draws outside the visible area.", facts.window_width),
            "Check the panel and sheet widths in ui/responsive.rs for this breakpoint.",
        );
    }
    if findings.is_empty() {
        findings.push(Finding {
            severity: Severity::Info,
            title: "No problems found".to_string(),
            detail: "yt-dlp, network and layout checks passed and the log shows no known errors.".to_string(),
            suggestion: String::new(),
        });
    }
    findings
}

pub fn report_markdown(findings: &[Finding]) -> String {
    let mut out = String::from("# Aria doctor report\n\n");
    for finding in findings {
        out.push_str(&format!("## [{}] {}\n\n", finding.severity.label(), finding.title));
        if !finding.detail.is_empty() {
            out.push_str(&format!("{}\n\n", finding.detail));
        }
        if !finding.suggestion.is_empty() {
            out.push_str(&format!("**Fix:** {}\n\n", finding.suggestion));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(lines: &[&str]) -> Facts {
        Facts {
            ytdlp_version: Some("2026.10.26".to_string()),
            online: true,
            lines: lines.iter().map(|line| line.to_string()).collect(),
            window_width: 1200.0,
            overflow: false,
            today_days: Some(days_from_civil(2026, 11, 3)),
        }
    }

    fn titles(findings: &[Finding]) -> Vec<String> {
        findings.iter().map(|f| f.title.clone()).collect()
    }

    #[test]
    fn dates_and_ages_are_computed_without_a_calendar_crate() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 28), 2);
        let today = days_from_civil(2026, 11, 3);
        assert_eq!(version_age_days("2026.10.26", today), Some(8));
        assert_eq!(version_age_days("2024.04.09", today).map(|age| age > 90), Some(true));
        assert_eq!(version_age_days("2026.10.26.232958", today), Some(8));
        assert_eq!(version_age_days("nightly", today), None);
        assert_eq!(version_age_days("1999.01.01", today), None);
    }

    #[test]
    fn a_healthy_app_reports_no_problems() {
        let findings = analyze(&facts(&["1 [error] unrelated"]));
        assert_eq!(titles(&findings), vec!["No problems found"]);
    }

    #[test]
    fn missing_or_old_yt_dlp_is_reported_with_the_update_fix() {
        let mut none = facts(&[]);
        none.ytdlp_version = None;
        let findings = analyze(&none);
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].suggestion.contains("Update yt-dlp"));
        let mut old = facts(&[]);
        old.ytdlp_version = Some("2024.04.09".to_string());
        let findings = analyze(&old);
        assert_eq!(findings[0].title, "yt-dlp is old");
    }

    #[test]
    fn the_format_error_points_at_an_old_yt_dlp_when_that_is_the_cause() {
        let line = "ERROR: [youtube] Z87R6: Requested format is not available. Use --list-formats";
        let mut old = facts(&[line]);
        old.ytdlp_version = Some("2024.04.09".to_string());
        let with_old = analyze(&old);
        let error = with_old.iter().find(|f| f.title.contains("no usable audio")).expect("finding");
        assert!(error.suggestion.contains("outdated"));
        let current = analyze(&facts(&[line]));
        let error = current.iter().find(|f| f.title.contains("no usable audio")).expect("finding");
        assert!(error.suggestion.contains("restricted"));
        assert!(error.detail.contains("1 time"));
    }

    #[test]
    fn known_log_patterns_each_get_their_own_finding() {
        let findings = analyze(&facts(&[
            "x [error] Failed to save history: database is locked",
            "x [error] http status: 403",
            "x [error] Please sign in",
            "x [error] Audio device unavailable: nope",
        ]));
        let all = titles(&findings).join("|");
        assert!(all.contains("database was locked"));
        assert!(all.contains("HTTP 403"));
        assert!(all.contains("sign-in"));
        assert!(all.contains("No audio output"));
    }

    #[test]
    fn offline_overflow_and_thumbnail_noise_are_flagged() {
        let mut state = facts(&["thumbnail 1", "thumbnail 2", "thumbnail 3", "thumbnail 4", "thumbnail 5"]);
        state.online = false;
        state.overflow = true;
        state.window_width = 800.0;
        let all = titles(&analyze(&state)).join("|");
        assert!(all.contains("offline"));
        assert!(all.contains("wider than the window"));
        assert!(all.contains("cover images"));
        let few = analyze(&facts(&["thumbnail 1", "thumbnail 2"]));
        assert_eq!(titles(&few), vec!["No problems found"]);
    }

    #[test]
    fn the_markdown_report_lists_each_finding_with_its_fix() {
        let report = report_markdown(&analyze(&facts(&["Please sign in"])));
        assert!(report.starts_with("# Aria doctor report"));
        assert!(report.contains("**Fix:**"));
    }
}
