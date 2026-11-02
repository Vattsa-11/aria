use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[derive(Default)]
pub enum ThemeVariant {
    #[default]
    Dark,
    Amoled,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[derive(Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub accent: [u8; 3],
    pub theme: ThemeVariant,
    pub volume: f32,
    pub library: Vec<PathBuf>,

    pub library_folders: Vec<PathBuf>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub eq: crate::equalizer::EqSettings,
    pub mini: MiniSettings,

    pub mini_opacity: f32,

    pub mini_auto_fade: bool,

    pub offline_mode: bool,
    pub autoplay: bool,
    pub normalize: bool,
    pub battery_saver: bool,
    pub crossfade: f32,

    pub jiosaavn_base: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct MiniSettings {
    pub width: f32,
    pub height: f32,
    pub x: Option<f32>,
    pub y: Option<f32>,
}

impl Default for MiniSettings {
    fn default() -> Self {
        Self {
            width: 340.0,
            height: 440.0,
            x: None,
            y: None,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            accent: [79, 140, 255],
            theme: ThemeVariant::Dark,
            volume: 0.8,
            library: Vec::new(),
            library_folders: Vec::new(),
            shuffle: false,
            repeat: RepeatMode::Off,
            eq: crate::equalizer::EqSettings::default(),
            mini: MiniSettings::default(),
            mini_opacity: 1.0,
            mini_auto_fade: false,
            offline_mode: false,
            autoplay: true,
            normalize: false,
            battery_saver: false,
            crossfade: 0.0,
            jiosaavn_base: String::new(),
        }
    }
}

static DATA_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(test)]
pub fn test_data_dir() -> PathBuf {
    DATA_DIR_OVERRIDE
        .get_or_init(|| std::env::temp_dir().join("aria-test-data"))
        .clone()
}

pub fn data_dir() -> PathBuf {
    if let Some(dir) = DATA_DIR_OVERRIDE.get() {
        return dir.clone();
    }
    if let Some(dir) = std::env::var_os("ARIA_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Aria")
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn load() -> Settings {
    let path = settings_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let dir = data_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(raw) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(settings_path(), raw);
    }
}
