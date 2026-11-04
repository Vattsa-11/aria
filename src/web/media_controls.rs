use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::mpsc::Sender;
use std::time::Duration;

use wasm_bindgen::JsValue;

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

thread_local! {
    static SENDER: RefCell<Option<Sender<MediaAction>>> = const { RefCell::new(None) };
}

pub fn init(_hwnd: Option<*mut c_void>, tx: Sender<MediaAction>) -> Option<MediaControls> {
    SENDER.with(|slot| *slot.borrow_mut() = Some(tx));
    Some(MediaControls)
}

pub fn parse(item: &str) -> Option<MediaAction> {
    match item {
        "play" => Some(MediaAction::Play),
        "pause" => Some(MediaAction::Pause),
        "next" => Some(MediaAction::Next),
        "previous" => Some(MediaAction::Previous),
        "stop" => Some(MediaAction::Stop),
        other => {
            let seconds: f64 = other.strip_prefix("seek:")?.parse().ok()?;
            (seconds.is_finite() && seconds >= 0.0).then(|| MediaAction::SeekTo(Duration::from_secs_f64(seconds)))
        }
    }
}

pub fn pump() {
    let raw = crate::install::take_media();
    if raw.is_empty() {
        return;
    }
    SENDER.with(|slot| {
        if let Some(tx) = slot.borrow().as_ref() {
            for item in raw.split(',') {
                if let Some(action) = parse(item) {
                    let _ = tx.send(action);
                }
            }
        }
    });
}

pub fn set_track(controls: Option<&mut MediaControls>, title: &str, artist: &str, thumb: Option<&str>, duration: Option<Duration>) {
    if controls.is_none() {
        return;
    }
    crate::install::call_args(
        "ariaMediaTrack",
        &[title.into(), artist.into(), thumb.unwrap_or("").into(), JsValue::from_f64(duration.map(|value| value.as_secs_f64()).unwrap_or(0.0))],
    );
}

pub fn set_state(controls: Option<&mut MediaControls>, playing: bool, position: Duration) {
    if controls.is_none() {
        return;
    }
    crate::install::call_args("ariaMediaState", &[JsValue::from_bool(playing), JsValue::from_f64(position.as_secs_f64())]);
}

pub fn set_stopped(controls: Option<&mut MediaControls>) {
    if controls.is_some() {
        crate::install::call_named("ariaMediaStop");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_screen_buttons_map_to_player_actions() {
        assert!(matches!(parse("play"), Some(MediaAction::Play)));
        assert!(matches!(parse("next"), Some(MediaAction::Next)));
        assert!(matches!(parse("previous"), Some(MediaAction::Previous)));
        assert!(matches!(parse("seek:42.5"), Some(MediaAction::SeekTo(at)) if at == Duration::from_secs_f64(42.5)));
        assert!(parse("seek:-3").is_none());
        assert!(parse("seek:NaN").is_none());
        assert!(parse("explode").is_none());
    }
}
