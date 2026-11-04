#[derive(Default)]
pub struct ListenMeter {
    track: Option<u64>,
    listened: f64,
    logged: bool,
}

const LONGEST_GAP: f64 = 1.5;
const UNKNOWN_LENGTH_SECS: f64 = 30.0;
const SHORTEST_COUNT: f64 = 5.0;

pub fn needed(duration: Option<f64>) -> f64 {
    match duration.filter(|secs| secs.is_finite() && *secs > 0.0) {
        Some(secs) => (secs / 2.0).max(SHORTEST_COUNT.min(secs)),
        None => UNKNOWN_LENGTH_SECS,
    }
}

impl ListenMeter {
    pub fn start(&mut self, track: u64) {
        self.track = Some(track);
        self.listened = 0.0;
        self.logged = false;
    }

    pub fn tick(&mut self, track: u64, playing: bool, seconds_since_last: f64, duration: Option<f64>) -> bool {
        if self.track != Some(track) || self.logged || !playing {
            return false;
        }
        self.listened += seconds_since_last.clamp(0.0, LONGEST_GAP);
        if self.listened >= needed(duration) {
            self.logged = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(meter: &mut ListenMeter, track: u64, seconds: u32, duration: Option<f64>) -> u32 {
        (0..seconds).filter(|_| meter.tick(track, true, 1.0, duration)).count() as u32
    }

    #[test]
    fn a_play_counts_once_when_half_of_the_song_has_been_heard() {
        let mut meter = ListenMeter::default();
        meter.start(1);
        assert_eq!(listen(&mut meter, 1, 99, Some(200.0)), 0);
        assert_eq!(listen(&mut meter, 1, 200, Some(200.0)), 1);
    }

    #[test]
    fn pausing_seeking_and_other_songs_do_not_count() {
        let mut meter = ListenMeter::default();
        meter.start(1);
        for _ in 0..500 {
            assert!(!meter.tick(1, false, 1.0, Some(200.0)));
        }
        assert!(!meter.tick(2, true, 1.0, Some(200.0)));
        assert!(!meter.tick(1, true, 9999.0, Some(1000.0)), "a long gap is capped, so a sleeping phone cannot count");
        meter.start(2);
        assert_eq!(listen(&mut meter, 2, 150, Some(300.0)), 1);
    }

    #[test]
    fn restarting_a_song_can_count_again_and_unknown_lengths_need_thirty_seconds() {
        let mut meter = ListenMeter::default();
        meter.start(7);
        assert_eq!(listen(&mut meter, 7, 29, None), 0);
        assert_eq!(listen(&mut meter, 7, 5, None), 1);
        meter.start(7);
        assert_eq!(listen(&mut meter, 7, 40, None), 1);
        assert_eq!(needed(Some(4.0)), 4.0);
        assert_eq!(needed(Some(240.0)), 120.0);
    }
}
