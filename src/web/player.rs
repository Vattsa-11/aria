use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::{AudioState, Command, Snapshot};
use crate::equalizer::EqParams;
use crate::jobs::Event;

pub struct PlayerHandle {
    snapshot: Arc<Mutex<Snapshot>>,
}

impl PlayerHandle {
    pub fn spawn(_volume: f32, _events: Sender<Event>, _eq: Arc<EqParams>, _normalize: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Snapshot { state: AudioState::Idle, track_id: None, position: Duration::ZERO, duration: None })),
        }
    }

    pub fn send(&self, _command: Command) {}

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap_or_else(|err| err.into_inner()).clone()
    }
}
