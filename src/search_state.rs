use crate::model::Track;
use crate::provider::ProviderName;

pub const PAGE: usize = 15;

#[derive(Default)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<Track>,
    pub running: bool,
    pub error: Option<String>,
    pub started: bool,
    pub limit: usize,
    pub more: bool,
    pub exhausted: bool,
    pub provider: Option<ProviderName>,
}

pub enum Outcome {
    Updated,
    Failed(String),
}

impl SearchState {
    pub fn showing_results(&self) -> bool {
        self.started || !self.results.is_empty()
    }

    pub fn reset(&mut self) {
        self.query.clear();
        self.results.clear();
        self.started = false;
        self.running = false;
        self.error = None;
    }

    pub fn fail_offline(&mut self, message: &str) {
        self.running = false;
        self.started = true;
        self.results.clear();
        self.error = Some(message.to_string());
    }

    pub fn begin(&mut self) -> usize {
        self.running = true;
        self.started = true;
        self.error = None;
        self.limit = PAGE;
        self.more = false;
        self.exhausted = false;
        PAGE
    }

    pub fn next_page(&mut self) -> Option<usize> {
        if self.query.trim().is_empty() || self.running || self.exhausted || self.results.is_empty() {
            return None;
        }
        self.running = true;
        self.more = true;
        self.limit = self.results.len().max(self.limit) + PAGE;
        Some(self.limit)
    }

    pub fn change_source(&mut self, provider: Option<ProviderName>) -> bool {
        if self.provider == provider {
            return false;
        }
        self.provider = provider;
        self.results.clear();
        self.error = None;
        self.started = false;
        true
    }

    pub fn matches(&self, query: &str, provider: Option<ProviderName>) -> bool {
        query == self.query.trim() && provider == self.provider
    }

    pub fn receive(&mut self, results: Vec<Track>, error: Option<String>) -> Outcome {
        self.running = false;
        self.started = true;
        let more = std::mem::take(&mut self.more);
        match error {
            Some(message) if more => {
                self.exhausted = true;
                Outcome::Failed(message)
            }
            Some(message) => {
                self.error = Some(message.clone());
                self.results.clear();
                Outcome::Failed(message)
            }
            None if more => {
                let known: std::collections::HashSet<u64> = self.results.iter().map(|track| track.id).collect();
                let fresh: Vec<Track> = results.into_iter().filter(|track| !known.contains(&track.id)).collect();
                if fresh.is_empty() {
                    self.exhausted = true;
                } else {
                    self.results.extend(fresh);
                }
                Outcome::Updated
            }
            None => {
                self.error = None;
                self.exhausted = results.len() < PAGE;
                self.results = results;
                Outcome::Updated
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackSource;

    fn track(id: u64) -> Track {
        Track { id, title: format!("t{id}"), artist: String::new(), album: String::new(), duration: None, source: TrackSource::Yt { id: id.to_string() }, thumb: None, provider_meta: None }
    }

    fn page(from: u64, count: u64) -> Vec<Track> {
        (from..from + count).map(track).collect()
    }

    #[test]
    fn more_results_are_appended_without_reordering_or_repeating() {
        let mut state = SearchState { query: "q".into(), ..SearchState::default() };
        state.begin();
        state.receive(page(1, 15), None);
        assert!(!state.exhausted);
        assert_eq!(state.next_page(), Some(30));
        state.receive(page(10, 15), None);
        let ids: Vec<u64> = state.results.iter().map(|t| t.id).collect();
        assert_eq!(ids, (1..=24).collect::<Vec<_>>());
    }

    #[test]
    fn a_short_first_page_or_an_empty_next_page_ends_the_list() {
        let mut state = SearchState { query: "q".into(), ..SearchState::default() };
        state.begin();
        state.receive(page(1, 4), None);
        assert!(state.exhausted);
        assert_eq!(state.next_page(), None);
        let mut state = SearchState { query: "q".into(), ..SearchState::default() };
        state.begin();
        state.receive(page(1, 15), None);
        state.next_page();
        state.receive(page(1, 15), None);
        assert!(state.exhausted);
    }

    #[test]
    fn paging_waits_for_a_running_search_and_an_error_while_paging_keeps_the_results() {
        let mut state = SearchState { query: "q".into(), ..SearchState::default() };
        assert_eq!(state.next_page(), None);
        state.begin();
        state.receive(page(1, 15), None);
        assert!(state.next_page().is_some());
        assert_eq!(state.next_page(), None);
        assert!(matches!(state.receive(Vec::new(), Some("boom".into())), Outcome::Failed(_)));
        assert_eq!(state.results.len(), 15);
        assert!(state.exhausted && state.error.is_none());
    }

    #[test]
    fn a_first_page_error_clears_results_and_a_source_change_resets() {
        let mut state = SearchState { query: "q".into(), ..SearchState::default() };
        state.begin();
        state.receive(page(1, 15), None);
        state.begin();
        state.receive(Vec::new(), Some("down".into()));
        assert!(state.results.is_empty() && state.error.as_deref() == Some("down"));
        assert!(!state.change_source(None));
        assert!(state.change_source(Some(ProviderName::Jiosaavn)));
        assert!(!state.started && state.error.is_none());
        assert!(state.matches("q", Some(ProviderName::Jiosaavn)));
    }
}
