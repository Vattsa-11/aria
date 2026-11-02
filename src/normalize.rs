use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rodio::source::SeekError;
use rodio::Source;

const TARGET_RMS: f32 = 0.125;
const MIN_GAIN: f32 = 0.125;
const MAX_GAIN: f32 = 4.0;
const RMS_TAU_SECS: f32 = 2.5;
const GAIN_TAU_SECS: f32 = 0.25;
const PEAK_TAU_SECS: f32 = 0.05;
const PEAK_CEILING: f32 = 0.97;

pub fn target_gain(observed_rms: f32) -> f32 {
    if observed_rms <= 1e-5 {
        1.0
    } else {
        (TARGET_RMS / observed_rms).clamp(MIN_GAIN, MAX_GAIN)
    }
}

pub struct NormalizeSource<S> {
    inner: S,
    channels: u16,
    enabled: Arc<AtomicBool>,
    mean_square: f32,
    peak: f32,
    gain: f32,
    frame_pos: usize,
    rms_alpha: f32,
    gain_alpha: f32,
    peak_alpha: f32,
}

impl<S: Source> NormalizeSource<S> {
    #[cfg(test)]
    fn new(inner: S, enabled: bool) -> Self {
        Self::with_flag(inner, Arc::new(AtomicBool::new(enabled)))
    }

    pub fn with_flag(inner: S, enabled: Arc<AtomicBool>) -> Self {
        let sample_rate = inner.sample_rate().get() as f32;
        let alpha = |tau: f32| 1.0 - (-1.0 / (sample_rate.max(1.0) * tau)).exp();
        Self {
            channels: inner.channels().get(),
            inner,
            enabled,
            mean_square: 0.0,
            peak: 0.0,
            gain: 1.0,
            frame_pos: 0,
            rms_alpha: alpha(RMS_TAU_SECS),
            gain_alpha: alpha(GAIN_TAU_SECS),
            peak_alpha: alpha(PEAK_TAU_SECS),
        }
    }


    fn limited_gain(&self, gain: f32) -> f32 {
        if self.peak <= 1e-5 {
            gain
        } else {
            gain.min(PEAK_CEILING / self.peak).max(MIN_GAIN)
        }
    }
}

impl<S: Source> Iterator for NormalizeSource<S> {
    type Item = S::Item;

    fn next(&mut self) -> Option<f32> {
        let sample = self.inner.next()?;
        let frame_member = self.frame_pos.is_multiple_of(self.channels.max(1) as usize);
        self.frame_pos = self.frame_pos.wrapping_add(1);

        if frame_member {
            self.mean_square += self.rms_alpha * (sample * sample - self.mean_square);
            self.peak += self.peak_alpha * (sample.abs() - self.peak);
        }

        if !self.enabled.load(Ordering::Relaxed) {
            self.gain += self.gain_alpha * (1.0 - self.gain);
            return Some(sample);
        }

        let target = self.limited_gain(target_gain(self.mean_square.sqrt()));
        self.gain += self.gain_alpha * (target - self.gain);
        Some((sample * self.gain).clamp(-1.0, 1.0))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for NormalizeSource<S> {
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
        self.mean_square = 0.0;
        self.peak = 0.0;
        self.gain = 1.0;
        self.frame_pos = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::source::SineWave;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    fn run(amplitude: f32, enabled: bool) -> Vec<f32> {
        let tone = SineWave::new(440.0).amplify(amplitude);
        let source = NormalizeSource::new(tone, enabled);
        source.take(48_000 * 6).collect()
    }

    #[test]
    fn a_quiet_track_is_lifted_towards_the_target() {
        let before = rms(&run(0.03, false)[48_000 * 4..]);
        let after = rms(&run(0.03, true)[48_000 * 4..]);
        assert!(after > before * 1.6, "before {before}, after {after}");
    }

    #[test]
    fn a_loud_track_is_pulled_down() {
        let before = rms(&run(0.9, false)[48_000 * 4..]);
        let after = rms(&run(0.9, true)[48_000 * 4..]);
        assert!(after < before * 0.9, "before {before}, after {after}");
    }

    #[test]
    fn a_flat_track_is_left_close_to_its_original_level() {
        let before = rms(&run(0.125 * std::f32::consts::SQRT_2, false)[48_000 * 4..]);
        let after = rms(&run(0.125 * std::f32::consts::SQRT_2, true)[48_000 * 4..]);
        let ratio = after / before;
        assert!(ratio > 0.85 && ratio < 1.15, "ratio {ratio}");
    }

    #[test]
    fn disabled_normalization_is_a_passthrough() {
        let plain: Vec<f32> = SineWave::new(440.0).amplify(0.2).take(2_000).collect();
        let wrapped: Vec<f32> = NormalizeSource::new(SineWave::new(440.0).amplify(0.2), false)
            .take(2_000)
            .collect();
        assert_eq!(plain, wrapped);
    }

    #[test]
    fn silence_is_not_amplified() {
        let channels = std::num::NonZero::new(1).expect("one channel");
        let rate = std::num::NonZero::new(44_100).expect("sample rate");
        let silence = rodio::source::Zero::new(channels, rate);
        let samples: Vec<f32> = NormalizeSource::new(silence, true).take(4_410).collect();
        assert!(samples.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn gain_limits_are_respected() {
        assert_eq!(target_gain(0.0), 1.0);
        assert!((target_gain(0.0001) - MAX_GAIN).abs() < 1e-6);
        assert!((target_gain(2.0) - MIN_GAIN).abs() < 1e-6);
        assert!((target_gain(0.126).round() - 0.992).abs() < 0.01);
    }
}
