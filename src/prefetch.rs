use std::collections::{HashMap, HashSet};
use std::time::Duration;

use web_time::Instant;

pub const MAX_AGE: Duration = Duration::from_secs(15 * 60);
pub const AFTER_SECS: f64 = 1.5;
pub const AHEAD: usize = 6;
pub const KEEP: usize = 6;
pub const IN_FLIGHT: usize = 2;

pub struct Warmed {
    pub track_id: u64,
    pub stream: crate::provider::ResolvedStream,
    pub http: crate::http::HttpStream,
    pub at: Instant,
}

#[derive(Default)]
pub struct Warmup {
    ready: HashMap<u64, Warmed>,
    requested: HashSet<u64>,
}

impl Warmup {
    pub fn has(&self, id: u64) -> bool {
        self.ready.contains_key(&id)
    }

    pub fn is_requested(&self, id: u64) -> bool {
        self.requested.contains(&id)
    }

    pub fn request(&mut self, id: u64) -> bool {
        self.requested.insert(id)
    }

    pub fn done(&mut self, id: u64) {
        self.requested.remove(&id);
    }

    pub fn expire(&mut self) {
        self.ready.retain(|_, warmed| warmed.at.elapsed() < MAX_AGE);
    }

    pub fn store(&mut self, warmed: Warmed) {
        self.ready.insert(warmed.track_id, warmed);
        while self.ready.len() > KEEP {
            let Some(oldest) = self.ready.iter().min_by_key(|(_, warmed)| warmed.at).map(|(id, _)| *id) else {
                break;
            };
            self.ready.remove(&oldest);
        }
    }

    pub fn take(&mut self, id: u64) -> Option<Warmed> {
        let warmed = self.ready.remove(&id)?;
        if is_usable(warmed.track_id, id, warmed.at.elapsed()) {
            warmed.http.resume_fill();
            Some(warmed)
        } else {
            None
        }
    }

    pub fn targets(&self, candidates: &[(u64, bool)], current: Option<u64>) -> Vec<usize> {
        let ready: HashSet<u64> = self.ready.keys().copied().collect();
        targets(candidates, &ready, &self.requested, current, IN_FLIGHT)
    }
}

#[derive(Default)]
pub struct Radio {
    pub pending: bool,
    pub append: bool,
    pub tried: Option<u64>,
    pub label: Option<String>,
}

pub fn is_usable(warmed_id: u64, wanted_id: u64, age: Duration) -> bool {
    warmed_id == wanted_id && age < MAX_AGE
}

pub fn targets(
    upcoming: &[(u64, bool)],
    warmed: &HashSet<u64>,
    requested: &HashSet<u64>,
    current: Option<u64>,
    in_flight_limit: usize,
) -> Vec<usize> {
    let mut budget = in_flight_limit.saturating_sub(requested.len());
    let mut picks = Vec::new();
    for (index, (id, local)) in upcoming.iter().enumerate() {
        if budget == 0 {
            break;
        }
        if *local || Some(*id) == current || warmed.contains(id) || requested.contains(id) {
            continue;
        }
        picks.push(index);
        budget -= 1;
    }
    picks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_upcoming_songs_are_warmed_nearest_first_without_flooding() {
        let upcoming = vec![(1, false), (2, true), (3, false), (4, false), (5, false)];
        let none = HashSet::new();
        assert_eq!(targets(&upcoming, &none, &none, None, 2), vec![0, 2]);
        let warmed: HashSet<u64> = [1].into_iter().collect();
        assert_eq!(targets(&upcoming, &warmed, &none, None, 2), vec![2, 3]);
        let busy: HashSet<u64> = [3].into_iter().collect();
        assert_eq!(targets(&upcoming, &warmed, &busy, None, 2), vec![3]);
        let full: HashSet<u64> = [3, 4].into_iter().collect();
        assert!(targets(&upcoming, &none, &full, None, 2).is_empty());
        assert!(targets(&upcoming, &none, &none, Some(1), 1) == vec![2]);
        assert!(targets(&[], &none, &none, None, 2).is_empty());
    }

    #[test]
    fn warmed_streams_expire() {
        assert!(is_usable(5, 5, Duration::from_secs(60)));
        assert!(!is_usable(5, 6, Duration::from_secs(60)));
        assert!(!is_usable(5, 5, MAX_AGE));
    }

    #[test]
    fn requests_are_tracked_until_they_finish() {
        let mut warmup = Warmup::default();
        assert!(warmup.request(7));
        assert!(!warmup.request(7));
        assert!(warmup.is_requested(7));
        warmup.done(7);
        assert!(!warmup.is_requested(7));
        assert!(!warmup.has(7));
    }
}
