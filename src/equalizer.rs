
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use rodio::source::SeekError;
#[cfg(not(target_arch = "wasm32"))]
use rodio::Source;

pub const BAND_COUNT: usize = 10;
pub const BAND_FREQS: [f32; BAND_COUNT] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
pub const BAND_LABELS: [&str; BAND_COUNT] = [
    "31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
];
pub const MIN_GAIN: f32 = -12.0;
pub const MAX_GAIN: f32 = 12.0;
pub const MIN_PREAMP: f32 = -12.0;
pub const MAX_PREAMP: f32 = 12.0;

pub const BAND_Q: f32 = 1.3;

pub struct Preset {
    pub name: &'static str,
    pub gains: [f32; BAND_COUNT],
}

pub const PRESETS: [Preset; 8] = [
    Preset {
        name: "Flat",
        gains: [0.0; BAND_COUNT],
    },
    Preset {
        name: "Rock",
        gains: [5.0, 4.0, 3.0, 1.0, -1.0, -1.0, 1.0, 2.0, 3.0, 4.0],
    },
    Preset {
        name: "Pop",
        gains: [-1.0, 0.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -1.0, -1.0],
    },
    Preset {
        name: "Jazz",
        gains: [4.0, 3.0, 1.0, 2.0, -2.0, -2.0, 0.0, 1.0, 3.0, 4.0],
    },
    Preset {
        name: "Classical",
        gains: [4.0, 3.0, 2.0, 1.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0],
    },
    Preset {
        name: "Bass boost",
        gains: [9.0, 8.0, 6.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    },
    Preset {
        name: "Vocal",
        gains: [-3.0, -2.0, -1.0, 2.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0],
    },
    Preset {
        name: "Electronic",
        gains: [5.0, 4.0, 1.0, 0.0, -2.0, -1.0, 1.0, 2.0, 4.0, 5.0],
    },
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqPreset {
    pub name: String,
    pub gains: [f32; BAND_COUNT],
    pub preamp: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqSettings {
    #[serde(default)]
    pub custom: Vec<EqPreset>,
    pub enabled: bool,
    pub preset: String,
    pub preamp: f32,
    pub gains: [f32; BAND_COUNT],
}

impl Default for EqSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: "Flat".to_string(),
            preamp: 0.0,
            gains: [0.0; BAND_COUNT],
            custom: Vec::new(),
        }
    }
}

impl EqSettings {
    pub fn apply_preset(&mut self, name: &str) {
        if let Some(preset) = PRESETS.iter().find(|preset| preset.name == name) {
            self.gains = preset.gains;
            self.preset = preset.name.to_string();
        }
    }

    pub fn matching_preset(&self) -> Option<&'static str> {
        PRESETS
            .iter()
            .find(|preset| preset.gains == self.gains)
            .map(|preset| preset.name)
    }

    pub fn save_custom(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let preset = EqPreset {
            name: name.to_string(),
            gains: self.gains,
            preamp: self.preamp,
        };
        match self
            .custom
            .iter_mut()
            .find(|saved| saved.name.eq_ignore_ascii_case(name))
        {
            Some(saved) => *saved = preset,
            None => self.custom.push(preset),
        }
        self.preset = name.to_string();
        true
    }

    pub fn apply_custom(&mut self, name: &str) -> bool {
        let Some(saved) = self
            .custom
            .iter()
            .find(|saved| saved.name.eq_ignore_ascii_case(name))
            .cloned()
        else {
            return false;
        };
        self.gains = saved.gains;
        self.preamp = saved.preamp;
        self.preset = saved.name;
        true
    }

    pub fn delete_custom(&mut self, name: &str) {
        self.custom
            .retain(|saved| !saved.name.eq_ignore_ascii_case(name));
        if self.preset.eq_ignore_ascii_case(name) {
            self.preset = "Custom".to_string();
        }
    }

    pub fn set_gain(&mut self, band: usize, gain: f32) {
        if let Some(slot) = self.gains.get_mut(band) {
            *slot = gain.clamp(MIN_GAIN, MAX_GAIN);
            self.preset = self
                .matching_preset()
                .map(|name| name.to_string())
                .unwrap_or_else(|| "Custom".to_string());
        }
    }
}

pub struct EqParams {
    enabled: AtomicBool,
    preamp_db: AtomicU32,
    gains: [AtomicU32; BAND_COUNT],
    version: AtomicU64,
}

impl EqParams {
    pub fn new(settings: &EqSettings) -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(settings.enabled),
            preamp_db: AtomicU32::new(settings.preamp.to_bits()),
            gains: std::array::from_fn(|band| {
                AtomicU32::new(settings.gains[band].to_bits())
            }),
            version: AtomicU64::new(1),
        })
    }

    pub fn apply(&self, settings: &EqSettings) {
        self.enabled.store(settings.enabled, Ordering::Relaxed);
        self.preamp_db
            .store(settings.preamp.to_bits(), Ordering::Relaxed);
        for band in 0..BAND_COUNT {
            self.gains[band].store(settings.gains[band].to_bits(), Ordering::Relaxed);
        }
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    pub fn snapshot(&self) -> (bool, f32, [f32; BAND_COUNT]) {
        let mut gains = [0.0f32; BAND_COUNT];
        for (band, slot) in gains.iter_mut().enumerate() {
            *slot = f32::from_bits(self.gains[band].load(Ordering::Relaxed));
        }
        (
            self.enabled.load(Ordering::Relaxed),
            f32::from_bits(self.preamp_db.load(Ordering::Relaxed)),
            gains,
        )
    }
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn peaking(freq: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        let nyquist = sample_rate * 0.5;

        let freq = freq.min(nyquist * 0.92).max(10.0);
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            ..Self::default()
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct EqSource<S> {
    inner: S,
    params: Arc<EqParams>,
    channels: usize,
    sample_rate: u32,
    filters: Vec<Biquad>,
    version: u64,
    enabled: bool,
    preamp: f32,
    channel: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl<S: Source> EqSource<S> {
    pub fn new(inner: S, params: Arc<EqParams>) -> Self {
        let channels = inner.channels().get() as usize;
        let sample_rate = inner.sample_rate().get();
        let mut source = Self {
            inner,
            params,
            channels: channels.max(1),
            sample_rate,
            filters: Vec::new(),
            version: 0,
            enabled: false,
            preamp: 1.0,
            channel: 0,
        };
        source.rebuild();
        source
    }

    fn rebuild(&mut self) {
        let (enabled, preamp_db, gains) = self.params.snapshot();
        self.version = self.params.version();
        self.enabled = enabled;
        self.preamp = 10f32.powf(preamp_db / 20.0);
        self.filters = if enabled {
            let layout = self.channels * BAND_COUNT;
            let mut filters = Vec::with_capacity(layout);
            for _channel in 0..self.channels {
                for band in 0..BAND_COUNT {
                    filters.push(Biquad::peaking(
                        BAND_FREQS[band],
                        BAND_Q,
                        gains[band],
                        self.sample_rate as f32,
                    ));
                }
            }
            filters
        } else {
            Vec::new()
        };
    }

    fn refresh(&mut self) {
        if self.params.version() != self.version {
            self.rebuild();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S: Source> Iterator for EqSource<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let sample = self.inner.next()?;
        self.refresh();
        if !self.enabled {
            return Some(sample);
        }
        let channel = self.channel;
        self.channel = (self.channel + 1) % self.channels;

        let mut value = sample * self.preamp;
        let base = channel * BAND_COUNT;
        for band in 0..BAND_COUNT {
            if let Some(filter) = self.filters.get_mut(base + band) {
                value = filter.process(value);
            }
        }
        Some(value.clamp(-1.0, 1.0))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<S: Source> Source for EqSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(pos)?;
        for filter in &mut self.filters {
            filter.reset();
        }
        self.channel = 0;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;
    use rodio::source::SineWave;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn filtered(gain_db: f32, freq: f32) -> Vec<f32> {
        let mut filter = Biquad::peaking(freq, BAND_Q, gain_db, 48_000.0);
        let input = SineWave::new(freq);
        let mut out = Vec::new();

        for (index, sample) in input.take(9_600).enumerate() {
            let value = filter.process(sample);
            if index >= 4_800 {
                out.push(value);
            }
        }
        out
    }

    #[test]
    fn boost_raises_the_band_it_targets() {
        let flat = filtered(0.0, 1_000.0);
        let boosted = filtered(6.0, 1_000.0);
        let ratio = rms(&boosted) / rms(&flat);

        assert!(ratio > 1.8 && ratio < 2.2, "ratio was {ratio}");
    }

    #[test]
    fn cut_lowers_the_band_it_targets() {
        let flat = filtered(0.0, 1_000.0);
        let cut = filtered(-6.0, 1_000.0);
        let ratio = rms(&cut) / rms(&flat);
        assert!(ratio > 0.44 && ratio < 0.56, "ratio was {ratio}");
    }

    #[test]
    fn flat_curve_passes_audio_through_unchanged() {
        let params = EqParams::new(&EqSettings {
            enabled: true,
            ..EqSettings::default()
        });
        let mut source = EqSource::new(SineWave::new(440.0), params);
        let mut reference = SineWave::new(440.0);
        for _ in 0..2_000 {
            let expected = reference.next().unwrap();
            let actual = source.next().unwrap();

            assert!((actual - expected).abs() < 1e-3, "{actual} != {expected}");
        }
    }

    #[test]
    fn disabling_the_eq_bypasses_everything() {
        let params = EqParams::new(&EqSettings {
            enabled: false,
            preamp: 6.0,
            gains: [12.0; BAND_COUNT],
            ..EqSettings::default()
        });
        let mut source = EqSource::new(SineWave::new(440.0), params);
        let mut reference = SineWave::new(440.0);
        for _ in 0..500 {
            let expected = reference.next().unwrap();
            assert_eq!(source.next().unwrap(), expected);
        }
    }

    #[test]
    fn live_changes_apply_without_reloading() {
        let params = EqParams::new(&EqSettings::default());

        let mut source = EqSource::new(SineWave::new(1_000.0).amplify(0.15), params.clone());

        let quiet: Vec<f32> = source.by_ref().take(4_800).collect();
        params.apply(&EqSettings {
            enabled: true,
            gains: [10.0; BAND_COUNT],
            ..EqSettings::default()
        });
        let loud: Vec<f32> = source.take(4_800).collect();
        assert!(
            rms(&loud) > rms(&quiet) * 1.5,
            "{} vs {}",
            rms(&loud),
            rms(&quiet)
        );
    }

    #[test]
    fn presets_apply_and_are_detected() {
        let mut settings = EqSettings::default();
        assert_eq!(settings.matching_preset(), Some("Flat"));
        settings.apply_preset("Rock");
        assert_eq!(settings.preset, "Rock");
        assert_eq!(settings.matching_preset(), Some("Rock"));
        settings.set_gain(0, 1.0);
        assert_eq!(settings.preset, "Custom");
        assert_eq!(settings.matching_preset(), None);
    }

    #[test]
    fn preset_save_and_load_roundtrips() {
        let mut settings = EqSettings {
            enabled: true,
            preamp: -3.5,
            ..EqSettings::default()
        };
        settings.apply_preset("Vocal");

        let raw = serde_json::to_string(&settings).expect("serialize");
        let loaded: EqSettings = serde_json::from_str(&raw).expect("deserialize");

        assert_eq!(loaded, settings);
        assert_eq!(loaded.preset, "Vocal");
        assert_eq!(loaded.matching_preset(), Some("Vocal"));
        assert_eq!(loaded.preamp, -3.5);

        let mut custom = EqSettings::default();
        custom.set_gain(4, 5.5);
        let loaded: EqSettings =
            serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
        assert_eq!(loaded.gains[4], 5.5);
        assert_eq!(loaded.preset, "Custom");
    }

    #[test]
    fn custom_presets_roundtrip_through_settings() {
        let mut settings = EqSettings {
            enabled: true,
            ..EqSettings::default()
        };
        settings.set_gain(2, 6.5);
        settings.set_gain(7, -3.0);
        settings.preamp = -2.0;
        assert!(settings.save_custom("My curve"));
        assert_eq!(settings.preset, "My curve");

        let saved: EqSettings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(saved.custom.len(), 1);
        assert_eq!(saved.custom[0].gains[2], 6.5);
        assert_eq!(saved.custom[0].preamp, -2.0);

        let mut fresh = saved;
        fresh.gains = [0.0; BAND_COUNT];
        fresh.preamp = 0.0;
        assert!(fresh.apply_custom("my curve"));
        assert_eq!(fresh.gains[2], 6.5);
        assert_eq!(fresh.gains[7], -3.0);
        assert_eq!(fresh.preamp, -2.0);

        fresh.delete_custom("My curve");
        assert!(fresh.custom.is_empty());
        assert_eq!(fresh.preset, "Custom");
        assert!(!fresh.apply_custom("My curve"));
    }

    #[test]
    fn gains_are_clamped_to_the_slider_range() {
        let mut settings = EqSettings::default();
        settings.set_gain(3, 40.0);
        assert_eq!(settings.gains[3], MAX_GAIN);
        settings.set_gain(3, -40.0);
        assert_eq!(settings.gains[3], MIN_GAIN);
    }
}
