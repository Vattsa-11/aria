use std::ffi::c_void;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub use souvlaki::MediaControls;
use souvlaki::{
    MediaControlEvent, MediaMetadata, MediaPlayback, MediaPosition,
    PlatformConfig,
};

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

pub fn init(
    hwnd: Option<*mut c_void>,
    tx: Sender<MediaAction>,
) -> Option<MediaControls> {
    let config = PlatformConfig {
        display_name: "Aria",
        dbus_name: "aria.player",
        hwnd,
    };
    let mut controls = MediaControls::new(config).ok()?;
    controls
        .attach(move |event| {
            let action = match event {
                MediaControlEvent::Play => MediaAction::Play,
                MediaControlEvent::Pause => MediaAction::Pause,
                MediaControlEvent::Toggle => MediaAction::Toggle,
                MediaControlEvent::Next => MediaAction::Next,
                MediaControlEvent::Previous => MediaAction::Previous,
                MediaControlEvent::Stop => MediaAction::Stop,
                MediaControlEvent::Seek(direction) => {
                    MediaAction::SeekRelative(direction_sign(direction) * 10)
                }
                MediaControlEvent::SeekBy(direction, amount) => {
                    MediaAction::SeekRelative(direction_sign(direction) * amount.as_secs() as i64)
                }
                MediaControlEvent::SetPosition(position) => MediaAction::SeekTo(position.0),
                MediaControlEvent::SetVolume(volume) => MediaAction::SetVolume(volume),
                _ => return,
            };
            let _ = tx.send(action);
        })
        .ok()?;
    Some(controls)
}

fn direction_sign(direction: souvlaki::SeekDirection) -> i64 {
    match direction {
        souvlaki::SeekDirection::Forward => 1,
        souvlaki::SeekDirection::Backward => -1,
    }
}

pub fn set_track(
    controls: Option<&mut MediaControls>,
    title: &str,
    artist: &str,
    thumb: Option<&str>,
    duration: Option<Duration>,
) {
    let Some(controls) = controls else {
        return;
    };
    let _ = controls.set_metadata(MediaMetadata {
        title: Some(title),
        artist: Some(artist),
        album: None,
        cover_url: thumb,
        duration,
    });
}

pub fn set_state(
    controls: Option<&mut MediaControls>,
    playing: bool,
    position: Duration,
) {
    let Some(controls) = controls else {
        return;
    };
    let progress = Some(MediaPosition(position));
    let playback = if playing {
        MediaPlayback::Playing { progress }
    } else {
        MediaPlayback::Paused { progress }
    };
    let _ = controls.set_playback(playback);
}

pub fn set_stopped(controls: Option<&mut MediaControls>) {
    if let Some(controls) = controls {
        let _ = controls.set_playback(MediaPlayback::Stopped);
    }
}
