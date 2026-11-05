use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::model::Track;
use crate::settings::RepeatMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackOrigin {
    Context,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TrackMenuAction {
    Play,
    GoToArtist,
    CopyInfo,
    CopyLink,
    NewPlaylist,
    PlayNext,
    AddToQueue,
    Like,
    FixMatch,
    Download,
    CancelDownload,
    DeleteDownload,
    AddToPlaylist(usize),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Queue {
    pub user: VecDeque<Track>,
    pub context: Vec<Track>,
    pub context_name: String,
    pub context_pos: Option<usize>,
    pub order: Vec<usize>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub rng: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Advance {
    PlayUser(Track),
    PlayContext { index: usize, track: Track },
    RepeatCurrent,
    End,
}

fn fresh_seed() -> u64 {
    let nanos = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    (nanos ^ 0x9E37_79B9_7F4A_7C15).max(1)
}

impl Default for Queue {
    fn default() -> Self {
        Self::new(false, RepeatMode::Off)
    }
}

impl Queue {
    pub fn new(shuffle: bool, repeat: RepeatMode) -> Self {
        let mut queue = Self {
            user: VecDeque::new(),
            context: Vec::new(),
            context_name: String::new(),
            context_pos: None,
            order: Vec::new(),
            shuffle,
            repeat,
            rng: fresh_seed(),
        };
        queue.rebuild_order();
        queue
    }

    pub fn set_context(&mut self, tracks: Vec<Track>, name: impl Into<String>) {
        self.context = tracks;
        self.context_name = name.into();
        self.context_pos = None;
        self.rebuild_order();
    }

    pub fn rebuild_order(&mut self) {
        let len = self.context.len();
        self.order = (0..len).collect();
        if self.shuffle {
            self.shuffle_order();
        }
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.rebuild_order();
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        };
    }

    fn shuffle_order(&mut self) {
        let len = self.order.len();
        for i in (1..len).rev() {
            let j = (self.next_random() % (i as u64 + 1)) as usize;
            self.order.swap(i, j);
        }
        if let Some(current) = self.context_pos {
            if let Some(position) = self.order.iter().position(|index| *index == current) {
                let value = self.order.remove(position);
                self.order.insert(0, value);
            }
        }
    }

    fn next_random(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    pub fn push_user(&mut self, track: Track) {
        self.user.push_back(track);
    }

    pub fn push_front_user(&mut self, track: Track) {
        self.user.push_front(track);
    }

    pub fn remove_user(&mut self, index: usize) {
        if index < self.user.len() {
            self.user.remove(index);
        }
    }

    pub fn take_user(&mut self, index: usize) -> Option<Track> {
        self.user.remove(index)
    }

    pub fn move_user(&mut self, from: usize, to: usize) {
        if from == to || from >= self.user.len() || to >= self.user.len() {
            return;
        }
        if let Some(track) = self.user.remove(from) {
            self.user.insert(to, track);
        }
    }

    pub fn remove_context(&mut self, index: usize) -> bool {
        if index >= self.context.len() || self.context_pos == Some(index) {
            return false;
        }
        self.context.remove(index);
        self.order.retain(|entry| *entry != index);
        for entry in &mut self.order {
            if *entry > index {
                *entry -= 1;
            }
        }
        if let Some(position) = self.context_pos {
            if position > index {
                self.context_pos = Some(position - 1);
            }
        }
        true
    }

    pub fn clear_upcoming_context(&mut self) -> usize {
        let mut upcoming: Vec<usize> = self.context_preview().into_iter().map(|(index, _)| index).collect();
        upcoming.sort_unstable_by(|a, b| b.cmp(a));
        upcoming.into_iter().filter(|index| self.remove_context(*index)).count()
    }

    pub fn mark_context_playing(&mut self, index: usize) {
        self.context_pos = Some(index);
    }

    pub fn next_context_index(&self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        let current_position = self
            .context_pos
            .and_then(|index| self.order.iter().position(|i| *i == index));
        let start = match current_position {
            Some(position) => position + 1,
            None => 0,
        };
        if start < self.order.len() {
            Some(self.order[start])
        } else if self.repeat == RepeatMode::All {
            Some(self.order[0])
        } else {
            None
        }
    }

    pub fn advance(&mut self, automatic: bool) -> Advance {
        if automatic && self.repeat == RepeatMode::One {
            return Advance::RepeatCurrent;
        }
        if let Some(track) = self.user.pop_front() {
            return Advance::PlayUser(track);
        }
        match self.next_context_index() {
            Some(index) => {
                let track = self.context[index].clone();
                Advance::PlayContext { index, track }
            }
            None => Advance::End,
        }
    }

    pub fn has_following(&self, automatic: bool) -> bool {
        (automatic && self.repeat == RepeatMode::One) || !self.user.is_empty() || self.next_context_index().is_some()
    }

    pub fn context_preview(&self) -> Vec<(usize, &Track)> {
        if self.order.is_empty() {
            return Vec::new();
        }
        let current_position = self
            .context_pos
            .and_then(|index| self.order.iter().position(|i| *i == index));
        let mut start = match current_position {
            Some(position) => position + 1,
            None => 0,
        };
        if start >= self.order.len() {
            if self.repeat == RepeatMode::All {
                start = 0;
            } else {
                return Vec::new();
            }
        }
        self.order[start..]
            .iter()
            .map(|index| (*index, &self.context[*index]))
            .collect()
    }

    pub fn jump_context(&mut self, index: usize) -> Option<Track> {
        if index >= self.context.len() {
            return None;
        }
        let track = self.context[index].clone();
        self.mark_context_playing(index);
        Some(track)
    }

    pub fn user_len(&self) -> usize {
        self.user.len()
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlayStack {
    entries: Vec<(Track, TrackOrigin)>,
}

const MAX_HISTORY: usize = 100;

impl PlayStack {
    pub fn record(&mut self, track: Track, origin: TrackOrigin) {
        if self
            .entries
            .last()
            .map(|(last, _)| last.id == track.id)
            .unwrap_or(false)
        {
            return;
        }
        self.entries.push((track, origin));
        if self.entries.len() > MAX_HISTORY {
            self.entries.remove(0);
        }
    }

    pub fn back(&mut self) -> Option<(Track, TrackOrigin)> {
        self.entries.pop()
    }

}

#[cfg(test)]
mod tests {
    #[test]
    fn removing_an_upcoming_song_keeps_the_rest_in_order_and_the_playing_one_safe() {
        let mut queue = super::Queue::new(false, crate::settings::RepeatMode::Off);
        let track = |id: u64| crate::model::Track { id, title: format!("t{id}"), artist: "a".into(), album: String::new(), duration: None, source: crate::model::TrackSource::Yt { id: id.to_string() }, thumb: None, provider_meta: None };
        queue.set_context((0..5).map(track).collect(), "list");
        queue.rebuild_order();
        queue.mark_context_playing(1);
        assert!(!queue.remove_context(1), "the playing song cannot be removed from under the player");
        assert!(queue.remove_context(3));
        let upcoming: Vec<u64> = queue.context_preview().into_iter().map(|(_, song)| song.id).collect();
        assert_eq!(upcoming, vec![2, 4]);
        assert!(queue.remove_context(0));
        assert_eq!(queue.context_pos, Some(0));
        assert_eq!(queue.context[queue.context_pos.unwrap()].id, 1, "the playing marker follows its song");
        assert!(!queue.remove_context(99));
    }

    #[test]
    fn clearing_upcoming_songs_leaves_what_has_played() {
        let mut queue = super::Queue::new(false, crate::settings::RepeatMode::Off);
        let track = |id: u64| crate::model::Track { id, title: format!("t{id}"), artist: "a".into(), album: String::new(), duration: None, source: crate::model::TrackSource::Yt { id: id.to_string() }, thumb: None, provider_meta: None };
        queue.set_context((0..6).map(track).collect(), "list");
        queue.rebuild_order();
        queue.mark_context_playing(2);
        assert_eq!(queue.clear_upcoming_context(), 3);
        assert!(queue.context_preview().is_empty());
        assert_eq!(queue.context.len(), 3);
    }

    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn track(name: &str) -> Track {
        Track {
            id: crate::model::hash_id(name),
            title: name.to_string(),
            artist: "artist".to_string(),
            album: String::new(),
            duration: Some(Duration::from_secs(180)),
            source: crate::model::TrackSource::Local(PathBuf::from(format!("{name}.mp3"))),
            thumb: None,
            provider_meta: None,
        }
    }

    fn context(count: usize) -> Vec<Track> {
        (0..count)
            .map(|i| track(&format!("context-{i}")))
            .collect()
    }

    fn position_of(queue: &Queue, index: usize) -> usize {
        queue
            .order
            .iter()
            .position(|i| *i == index)
            .expect("index in order")
    }

    #[test]
    fn user_queue_wins_over_shuffle_and_context_position_survives() {
        let mut queue = Queue::new(true, RepeatMode::Off);
        queue.rng = 0x9E37_79B9_7F4A_7C15;
        queue.set_context(context(6), "Album");
        queue.mark_context_playing(2);
        queue.push_user(track("user-a"));
        queue.push_user(track("user-b"));

        let expected_next_context = {
            let position = position_of(&queue, 2);
            queue.order[position + 1]
        };

        let first = queue.advance(true);
        assert!(
            matches!(&first, Advance::PlayUser(t) if t.title == "user-a"),
            "shuffle must not reorder the user queue, got {first:?}"
        );
        assert_eq!(queue.context_pos, Some(2));
        assert_eq!(queue.user_len(), 1);

        let second = queue.advance(true);
        assert!(
            matches!(&second, Advance::PlayUser(t) if t.title == "user-b"),
            "second advance must also come from the user queue, got {second:?}"
        );
        assert_eq!(
            queue.context_pos,
            Some(2),
            "context position must be preserved while the user queue drains"
        );

        let third = queue.advance(true);
        assert_eq!(
            third,
            Advance::PlayContext {
                index: expected_next_context,
                track: queue.context[expected_next_context].clone(),
            },
            "context must resume exactly where it left off"
        );
        assert_ne!(expected_next_context, 2, "must not replay the same context track");
        assert_ne!(expected_next_context, 0, "must not restart the context");
    }

    #[test]
    fn user_queue_priority_without_shuffle() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.set_context(context(4), "Playlist");
        queue.mark_context_playing(0);
        queue.push_user(track("only-user"));

        let next = queue.advance(false);
        assert!(matches!(&next, Advance::PlayUser(t) if t.title == "only-user"));
        assert_eq!(queue.context_pos, Some(0));
        assert_eq!(queue.user_len(), 0);

        let next = queue.advance(false);
        assert_eq!(
            next,
            Advance::PlayContext {
                index: 1,
                track: queue.context[1].clone()
            }
        );
    }

    #[test]
    fn repeat_one_repeats_current_even_with_user_items() {
        let mut queue = Queue::new(false, RepeatMode::One);
        queue.set_context(context(3), "Album");
        queue.mark_context_playing(1);
        queue.push_user(track("user-a"));

        let next = queue.advance(true);
        assert_eq!(next, Advance::RepeatCurrent);
        assert_eq!(
            queue.user_len(),
            1,
            "repeat-one must not consume the user queue"
        );
        assert_eq!(queue.context_pos, Some(1));
    }

    #[test]
    fn repeat_one_does_not_block_manual_next() {
        let mut queue = Queue::new(false, RepeatMode::One);
        queue.set_context(context(3), "Album");
        queue.mark_context_playing(1);
        queue.push_user(track("user-a"));

        let next = queue.advance(false);
        assert!(matches!(next, Advance::PlayUser(_)));
    }

    #[test]
    fn repeat_all_loops_context_never_user_queue() {
        let mut queue = Queue::new(false, RepeatMode::All);
        queue.set_context(context(3), "Album");
        queue.mark_context_playing(2);

        let next = queue.advance(true);
        assert_eq!(
            next,
            Advance::PlayContext {
                index: 0,
                track: queue.context[0].clone()
            }
        );

        queue.push_user(track("user-a"));
        queue.push_user(track("user-b"));
        let first = queue.advance(true);
        let second = queue.advance(true);
        assert!(matches!(first, Advance::PlayUser(_)));
        assert!(matches!(second, Advance::PlayUser(_)));
        assert_eq!(queue.user_len(), 0, "user queue is drained, never looped");

        queue.mark_context_playing(2);
        let next = queue.advance(true);
        assert_eq!(
            next,
            Advance::PlayContext {
                index: 0,
                track: queue.context[0].clone()
            },
            "repeat-all loops only the context"
        );
    }

    #[test]
    fn repeat_off_ends_after_context_and_user_queues_empty() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.set_context(context(2), "Album");
        queue.mark_context_playing(1);

        assert_eq!(queue.advance(true), Advance::End);

        queue.push_user(track("user-a"));
        assert!(matches!(queue.advance(true), Advance::PlayUser(_)));
        assert_eq!(queue.advance(true), Advance::End);
    }

    #[test]
    fn play_next_inserts_front_add_to_queue_appends_back() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.push_user(track("first"));
        queue.push_front_user(track("second"));
        queue.push_user(track("third"));

        let titles: Vec<String> = queue.user.iter().map(|t| t.title.clone()).collect();
        assert_eq!(titles, ["second", "first", "third"]);
    }

    #[test]
    fn has_following_reports_whether_advance_would_move_on() {
        let mut queue = Queue::new(false, crate::settings::RepeatMode::Off);
        assert!(!queue.has_following(true));
        queue.set_context(vec![track("a"), track("b")], "mix");
        assert!(queue.has_following(true));
        queue.repeat = crate::settings::RepeatMode::One;
        assert!(queue.has_following(true));
    }

    #[test]
    fn shuffle_toggle_never_touches_user_queue() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.set_context(context(8), "Album");
        queue.push_user(track("user-a"));
        queue.push_user(track("user-b"));

        queue.toggle_shuffle();
        let titles: Vec<String> = queue.user.iter().map(|t| t.title.clone()).collect();
        assert_eq!(titles, ["user-a", "user-b"]);

        queue.toggle_shuffle();
        let titles: Vec<String> = queue.user.iter().map(|t| t.title.clone()).collect();
        assert_eq!(titles, ["user-a", "user-b"]);
        assert_eq!(queue.order, (0..8).collect::<Vec<usize>>());
    }

    #[test]
    fn shuffle_moves_current_context_track_to_front_of_order() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.set_context(context(8), "Album");
        queue.mark_context_playing(5);
        queue.toggle_shuffle();

        assert_eq!(queue.order[0], 5, "current track stays first in the new order");
        let next = queue.next_context_index();
        assert!(next.is_some() && next != Some(5), "next must be a different track");
    }

    #[test]
    fn user_queue_reorder_remove_and_take() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.push_user(track("a"));
        queue.push_user(track("b"));
        queue.push_user(track("c"));

        queue.move_user(0, 2);
        let titles: Vec<String> = queue.user.iter().map(|t| t.title.clone()).collect();
        assert_eq!(titles, ["b", "c", "a"]);

        queue.remove_user(1);
        let titles: Vec<String> = queue.user.iter().map(|t| t.title.clone()).collect();
        assert_eq!(titles, ["b", "a"]);

        let taken = queue.take_user(0);
        assert_eq!(taken.map(|t| t.title), Some("b".to_string()));
        assert_eq!(queue.user_len(), 1);
    }

    #[test]
    fn duplicate_tracks_can_coexist_in_queues() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.set_context(vec![track("same"), track("same")], "Album");
        queue.push_user(track("same"));
        queue.push_user(track("same"));
        assert_eq!(queue.user_len(), 2);
        assert_eq!(queue.context.len(), 2);

        match queue.advance(true) {
            Advance::PlayUser(t) => assert_eq!(t.title, "same"),
            other => panic!("expected user track, got {other:?}"),
        }
        assert_eq!(queue.user_len(), 1, "played user track is removed once played");
    }

    #[test]
    fn jumping_into_context_resumes_from_there() {
        let mut queue = Queue::new(false, RepeatMode::Off);
        queue.set_context(context(5), "Album");

        let jumped = queue.jump_context(3);
        assert_eq!(jumped.map(|t| t.title), Some("context-3".to_string()));
        assert_eq!(queue.context_pos, Some(3));

        let next = queue.advance(true);
        assert_eq!(
            next,
            Advance::PlayContext {
                index: 4,
                track: queue.context[4].clone()
            }
        );
    }

    #[test]
    fn play_stack_returns_the_previous_track() {
        let mut stack = PlayStack::default();
        stack.record(track("one"), TrackOrigin::Context);
        stack.record(track("two"), TrackOrigin::User);

        let (previous, origin) = stack.back().expect("previous track");
        assert_eq!(previous.title, "two");
        assert_eq!(origin, TrackOrigin::User);
        let (older, origin) = stack.back().expect("older track");
        assert_eq!(older.title, "one");
        assert_eq!(origin, TrackOrigin::Context);
        assert!(stack.back().is_none());
    }

    #[test]
    fn play_stack_ignores_repeats_and_stays_bounded() {
        let mut stack = PlayStack::default();
        stack.record(track("same"), TrackOrigin::Context);
        stack.record(track("same"), TrackOrigin::Context);
        assert_eq!(stack.back().map(|(t, _)| t.title), Some("same".to_string()));
        assert!(stack.back().is_none());

        for index in 0..150 {
            stack.record(track(&format!("t{index}")), TrackOrigin::Context);
        }
        let mut depth = 0;
        while stack.back().is_some() {
            depth += 1;
        }
        assert_eq!(depth, MAX_HISTORY);
    }

    #[test]
    fn empty_context_and_empty_user_queue_ends() {
        let mut queue = Queue::new(false, RepeatMode::All);
        assert_eq!(queue.advance(true), Advance::End);
        queue.push_user(track("only"));
        assert!(matches!(queue.advance(true), Advance::PlayUser(_)));
        assert_eq!(queue.advance(true), Advance::End);
    }
}
