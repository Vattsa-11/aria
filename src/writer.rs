#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::thread::{self, JoinHandle};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
use web_time::Instant;

use crate::model::{Playlist, Track};
use crate::queue::Queue;

#[cfg(not(target_arch = "wasm32"))]
const COALESCE_MS: u64 = 180;
#[cfg(not(target_arch = "wasm32"))]
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

pub enum Write {
    Playlists(Vec<Playlist>),
    Liked(Vec<Track>),
    History(Vec<Track>),
    State(Box<StateWrite>),
    Stop,
}

pub struct StateWrite {
    pub queue: Queue,
    pub current: Option<Track>,
    pub position_ms: u64,
}

impl Write {
    pub fn apply(self) {
        match self {
            Write::Playlists(playlists) => {
                if let Err(err) = crate::db::save_playlists(&playlists) {
                    crate::logs::error(&format!("Failed to save playlists: {err}"));
                }
            }
            Write::Liked(tracks) => {
                if let Err(err) = crate::db::save_liked(&tracks) {
                    crate::logs::error(&format!("Failed to save liked songs: {err}"));
                }
            }
            Write::History(tracks) => {
                if let Err(err) = crate::db::save_history(&tracks) {
                    crate::logs::error(&format!("Failed to save history: {err}"));
                }
            }
            Write::State(state) => {
                if let Err(err) =
                    crate::db::save_state(&state.queue, state.current.as_ref(), state.position_ms)
                {
                    crate::logs::error(&format!("Failed to save queue state: {err}"));
                }
            }
            Write::Stop => {}
        }
    }
}

#[derive(Default)]
struct Batch {
    playlists: Option<Vec<Playlist>>,
    liked: Option<Vec<Track>>,
    history: Option<Vec<Track>>,
    state: Option<Box<StateWrite>>,
}

impl Batch {
    fn absorb(&mut self, write: Write) {
        match write {
            Write::Playlists(playlists) => self.playlists = Some(playlists),
            Write::Liked(tracks) => self.liked = Some(tracks),
            Write::History(tracks) => self.history = Some(tracks),
            Write::State(state) => self.state = Some(state),
            Write::Stop => {}
        }
    }

    fn drain(&mut self) -> Vec<Write> {
        let mut pending = Vec::new();
        if let Some(playlists) = self.playlists.take() {
            pending.push(Write::Playlists(playlists));
        }
        if let Some(tracks) = self.liked.take() {
            pending.push(Write::Liked(tracks));
        }
        if let Some(tracks) = self.history.take() {
            pending.push(Write::History(tracks));
        }
        if let Some(state) = self.state.take() {
            pending.push(Write::State(state));
        }
        pending
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Writer {
    tx: Sender<Write>,
    handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Clone for Writer {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            handle: self.handle.clone(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Writer {
    pub fn send(&self, write: Write) {
        let _ = self.tx.send(write);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Write::Stop);
        let handle = self
            .handle
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        let Some(handle) = handle else {
            return;
        };
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        while !handle.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if handle.is_finished() {
            let _ = handle.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn() -> Writer {
    spawn_with(Write::apply)
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_with<F>(apply: F) -> Writer
where
    F: Fn(Write) + Send + 'static,
{
    let (tx, rx) = channel::<Write>();
    let worker = thread::Builder::new()
        .name("aria-writer".into())
        .spawn(move || {
            'outer: while let Ok(first) = rx.recv() {
                if matches!(first, Write::Stop) {
                    break;
                }
                let mut batch = Batch::default();
                batch.absorb(first);
                loop {
                    match rx.recv_timeout(Duration::from_millis(COALESCE_MS)) {
                        Ok(Write::Stop) => {
                            for write in batch.drain() {
                                apply(write);
                            }
                            break 'outer;
                        }
                        Ok(write) => batch.absorb(write),
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => {
                            for write in batch.drain() {
                                apply(write);
                            }
                            break 'outer;
                        }
                    }
                }
                for write in batch.drain() {
                    apply(write);
                }
                while let Ok(extra) = rx.try_recv() {
                    if matches!(extra, Write::Stop) {
                        break 'outer;
                    }
                    let mut tail = Batch::default();
                    tail.absorb(extra);
                    for write in tail.drain() {
                        apply(write);
                    }
                }
            }
        })
        .ok();

    Writer {
        tx,
        handle: Arc::new(Mutex::new(worker)),
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct Writer;

#[cfg(target_arch = "wasm32")]
impl Writer {
    pub fn send(&self, write: Write) {
        if !matches!(write, Write::Stop) {
            write.apply();
        }
    }

    pub fn shutdown(&self) {}
}

#[cfg(target_arch = "wasm32")]
pub fn spawn() -> Writer {
    Writer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playlists(name: &str) -> Vec<Playlist> {
        vec![Playlist {
            id: 1,
            name: name.to_string(),
            tracks: Vec::new(),
        }]
    }

    #[test]
    fn a_batch_keeps_only_the_latest_value_of_each_kind() {
        let mut batch = Batch::default();
        batch.absorb(Write::Playlists(playlists("first")));
        batch.absorb(Write::Playlists(playlists("second")));
        let drained = batch.drain();
        assert_eq!(drained.len(), 1);
        match &drained[0] {
            Write::Playlists(saved) => assert_eq!(saved[0].name, "second"),
            _ => panic!("wrong kind"),
        }
    }

    #[test]
    fn unrelated_kinds_are_kept_side_by_side() {
        let mut batch = Batch::default();
        batch.absorb(Write::Liked(Vec::new()));
        batch.absorb(Write::History(Vec::new()));
        assert!(batch.liked.is_some());
        assert!(batch.history.is_some());
        assert_eq!(batch.drain().len(), 2);
        assert!(batch.drain().is_empty());
    }

    #[test]
    fn the_newest_state_wins() {
        let mut batch = Batch::default();
        for position in [1_000u64, 2_000, 3_000] {
            batch.absorb(Write::State(Box::new(StateWrite {
                queue: Queue::default(),
                current: None,
                position_ms: position,
            })));
        }
        match batch.drain().pop() {
            Some(Write::State(state)) => assert_eq!(state.position_ms, 3_000),
            _ => panic!("state missing"),
        }
    }

    #[test]
    fn stop_flushes_the_pending_write() {
        let (sink, received) = channel::<String>();
        let writer = spawn_with(move |write| {
            let label = match write {
                Write::Playlists(saved) => saved[0].name.clone(),
                Write::Liked(_) => "liked".to_string(),
                Write::History(_) => "history".to_string(),
                Write::State(_) => "state".to_string(),
                Write::Stop => "stop".to_string(),
            };
            let _ = sink.send(label);
        });

        writer.send(Write::Playlists(playlists("pending")));
        writer.send(Write::Playlists(playlists("newest")));
        writer.shutdown();

        let labels: Vec<String> = received.try_iter().collect();
        assert_eq!(labels, vec!["newest".to_string()]);
    }

    #[test]
    fn stop_as_the_first_message_exits_the_worker() {
        let (sink, received) = channel::<String>();
        let writer = spawn_with(move |write| {
            if matches!(write, Write::Stop) {
                let _ = sink.send("stop".to_string());
            }
        });
        writer.shutdown();
        let labels: Vec<String> = received.try_iter().collect();
        assert!(labels.is_empty(), "worker applied {labels:?}");
        assert!(writer
            .handle
            .lock()
            .map(|slot| slot.is_none())
            .unwrap_or(false));
    }
}
