use std::ffi::c_void;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub enum MediaAction {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    SeekTo(Duration),
    SeekRelative(i64),
    SetVolume(f64),
}

pub struct MediaControls;

pub fn init(_hwnd: Option<*mut c_void>, _tx: Sender<MediaAction>) -> Option<MediaControls> {
    None
}

pub fn set_track(_controls: Option<&mut MediaControls>, _title: &str, _artist: &str, _thumb: Option<&str>, _duration: Option<Duration>) {}

pub fn set_state(_controls: Option<&mut MediaControls>, _playing: bool, _position: Duration) {}

pub fn set_stopped(_controls: Option<&mut MediaControls>) {}
