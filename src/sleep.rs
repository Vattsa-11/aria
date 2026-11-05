use std::time::Duration;
use web_time::Instant;

pub const FADE_SECS: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    SetVolume(f32),
    Stop { restore: f32 },
}

#[derive(Clone, Debug, Default)]
pub struct SleepTimer {
    pub until: Option<Instant>,
    pub end_of_track: bool,
    fade_started: Option<Instant>,
    saved_volume: f32,
}

impl SleepTimer {
    pub fn arm_minutes(&mut self, minutes: u32) {
        self.until = Some(Instant::now() + Duration::from_secs(minutes as u64 * 60));
        self.end_of_track = false;
        self.fade_started = None;
    }

    pub fn arm_end_of_track(&mut self) {
        self.until = None;
        self.end_of_track = true;
        self.fade_started = None;
    }

    pub fn clear(&mut self) {
        self.until = None;
        self.end_of_track = false;
        self.fade_started = None;
    }

    pub fn active(&self) -> bool {
        self.until.is_some() || self.end_of_track
    }

    pub fn label(&self) -> Option<String> {
        if let Some(until) = self.until {
            let left = until.saturating_duration_since(Instant::now());
            let minutes = left.as_secs().div_ceil(60);
            return Some(format!("Sleeping in {minutes} min"));
        }
        if self.end_of_track {
            return Some("Sleeping after this track".to_string());
        }
        None
    }

    pub fn deadline_reached(&self) -> bool {
        self.until
            .map(|until| Instant::now() >= until)
            .unwrap_or(false)
    }

    pub fn track_finished(&mut self) -> bool {
        if self.end_of_track {
            self.clear();
            return true;
        }
        false
    }

    pub fn begin_fade(&mut self, volume: f32) -> bool {
        if self.fade_started.is_some() {
            return false;
        }
        self.saved_volume = volume;
        self.fade_started = Some(Instant::now());
        true
    }

    pub fn fading(&self) -> bool {
        self.fade_started.is_some()
    }

    pub fn fade_volume(&self, base: f32) -> f32 {
        let Some(started) = self.fade_started else {
            return base;
        };
        let progress = (started.elapsed().as_secs_f32() / FADE_SECS).clamp(0.0, 1.0);
        base * (1.0 - progress)
    }

    pub fn fade_finished(&self) -> bool {
        self.fade_started
            .map(|started| started.elapsed().as_secs_f32() >= FADE_SECS)
            .unwrap_or(false)
    }

    pub fn step(&mut self, volume: f32) -> Vec<Step> {
        let mut steps = Vec::new();
        if !self.active() {
            return steps;
        }
        if !self.fading() && self.deadline_reached() {
            self.begin_fade(volume);
        }
        if self.fading() {
            let target = self.fade_volume(volume);
            if target > 0.001 {
                steps.push(Step::SetVolume(target));
            }
            if self.fade_finished() {
                steps.push(Step::Stop { restore: self.take_saved_volume() });
            }
        }
        steps
    }

    pub fn take_saved_volume(&mut self) -> f32 {
        let saved = self.saved_volume;
        self.clear();
        saved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unarmed_timer_is_inactive() {
        let timer = SleepTimer::default();
        assert!(!timer.active());
        assert!(timer.label().is_none());
        assert!(!timer.deadline_reached());
    }

    #[test]
    fn arming_minutes_sets_a_label() {
        let mut timer = SleepTimer::default();
        timer.arm_minutes(15);
        assert!(timer.active());
        assert!(timer.label().unwrap().contains("15"));
    }

    #[test]
    fn end_of_track_fires_once() {
        let mut timer = SleepTimer::default();
        timer.arm_end_of_track();
        assert!(timer.track_finished());
        assert!(!timer.active());
        assert!(!timer.track_finished());
    }

    #[test]
    fn the_fade_ramps_down_to_silence() {
        let mut timer = SleepTimer::default();
        assert!(!timer.fading());
        timer.begin_fade(0.8);
        assert!(timer.fading());
        assert!(!timer.begin_fade(0.8));
        let start = timer.fade_volume(0.8);
        assert!((start - 0.8).abs() < 0.05);
        timer.fade_started = Some(Instant::now() - Duration::from_secs(3));
        let middle = timer.fade_volume(0.8);
        assert!(middle < 0.8 && middle > 0.0);
        timer.fade_started = Some(Instant::now() - Duration::from_secs(9));
        assert!(timer.fade_finished());
        assert_eq!(timer.fade_volume(0.8), 0.0);
        assert!((timer.take_saved_volume() - 0.8).abs() < 1e-6);
        assert!(!timer.active());
    }

    #[test]
    fn stepping_does_nothing_until_the_deadline_then_fades_and_stops() {
        let mut timer = SleepTimer::default();
        assert!(timer.step(0.8).is_empty());
        timer.arm_minutes(10);
        assert!(timer.step(0.8).is_empty());
        timer.until = Some(Instant::now() - Duration::from_secs(1));
        let first = timer.step(0.8);
        assert!(matches!(first.as_slice(), [Step::SetVolume(v)] if *v > 0.7));
        timer.fade_started = Some(Instant::now() - Duration::from_secs(9));
        let last = timer.step(0.8);
        assert_eq!(last, vec![Step::Stop { restore: 0.8 }]);
        assert!(!timer.active());
        assert!(timer.step(0.8).is_empty());
    }
}
