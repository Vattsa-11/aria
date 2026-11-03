use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Overrides {
    pub phone_max: Option<f32>,
    pub tablet_max: Option<f32>,
    pub desktop_max: Option<f32>,
    pub page_margin: Option<i8>,
    pub card_min: Option<f32>,
    pub sheet_width: Option<f32>,
}

static CURRENT: Mutex<Overrides> = Mutex::new(Overrides {
    phone_max: None,
    tablet_max: None,
    desktop_max: None,
    page_margin: None,
    card_min: None,
    sheet_width: None,
});

pub fn get() -> Overrides {
    *CURRENT.lock().unwrap_or_else(|err| err.into_inner())
}

pub fn set(overrides: Overrides) {
    *CURRENT.lock().unwrap_or_else(|err| err.into_inner()) = overrides;
}

pub fn parse(text: &str) -> Result<Overrides, String> {
    let overrides: Overrides = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let sane = |value: Option<f32>| value.is_none_or(|v| (100.0..=6000.0).contains(&v));
    if !sane(overrides.phone_max) || !sane(overrides.tablet_max) || !sane(overrides.desktop_max) || !sane(overrides.card_min) || !sane(overrides.sheet_width) {
        return Err("widths must be between 100 and 6000".to_string());
    }
    if overrides.page_margin.is_some_and(|margin| !(0..=80).contains(&margin)) {
        return Err("page_margin must be between 0 and 80".to_string());
    }
    Ok(overrides)
}

pub fn path() -> PathBuf {
    crate::settings::data_dir().join("ui-live.json")
}

pub const TEMPLATE: &str = "{\n  \"phone_max\": 640,\n  \"tablet_max\": 900,\n  \"desktop_max\": 1280,\n  \"page_margin\": null,\n  \"card_min\": null,\n  \"sheet_width\": null\n}\n";

#[derive(Default)]
pub struct Watcher {
    modified: Option<SystemTime>,
}

impl Watcher {
    pub fn poll(&mut self) -> bool {
        let file = path();
        let Ok(meta) = std::fs::metadata(&file) else {
            if self.modified.take().is_some() {
                set(Overrides::default());
                return true;
            }
            return false;
        };
        let modified = meta.modified().ok();
        if modified == self.modified {
            return false;
        }
        self.modified = modified;
        match std::fs::read_to_string(&file).map_err(|err| err.to_string()).and_then(|text| parse(&text)) {
            Ok(overrides) => {
                set(overrides);
                true
            }
            Err(reason) => {
                crate::logs::error(&format!("ui-live.json ignored: {reason}"));
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_files_override_only_what_they_name() {
        let parsed = parse(r#"{"tablet_max": 1000, "page_margin": 10}"#).unwrap();
        assert_eq!(parsed.tablet_max, Some(1000.0));
        assert_eq!(parsed.page_margin, Some(10));
        assert_eq!(parsed.phone_max, None);
        assert_eq!(parse("{}").unwrap(), Overrides::default());
    }

    #[test]
    fn nonsense_is_rejected_so_a_typo_cannot_break_the_layout() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"phone_max": 5}"#).is_err());
        assert!(parse(r#"{"card_min": 99999}"#).is_err());
        assert!(parse(r#"{"page_margin": 500}"#).is_err());
        assert!(parse(TEMPLATE).is_ok());
    }

    #[test]
    fn the_template_matches_the_built_in_defaults() {
        let parsed = parse(TEMPLATE).unwrap();
        assert_eq!((parsed.phone_max, parsed.tablet_max, parsed.desktop_max), (Some(640.0), Some(900.0), Some(1280.0)));
    }

    #[test]
    fn watcher_reloads_when_the_file_changes_and_resets_when_it_is_removed() {
        crate::settings::test_data_dir();
        let _ = std::fs::remove_file(path());
        let mut watcher = Watcher::default();
        assert!(!watcher.poll());
        std::fs::write(path(), r#"{"page_margin": 33}"#).unwrap();
        assert!(watcher.poll());
        assert_eq!(get().page_margin, Some(33));
        assert!(!watcher.poll());
        std::fs::remove_file(path()).unwrap();
        assert!(watcher.poll());
        assert_eq!(get(), Overrides::default());
    }
}
