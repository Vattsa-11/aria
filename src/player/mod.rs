use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use std::time::Duration;

use rodio::source::Source;
use rodio::{Decoder, DeviceSinkBuilder, Player as RodioPlayer};

use crate::equalizer::{EqParams, EqSource};
use crate::normalize::NormalizeSource;
use crate::jobs::Event;
use crate::model::TrackSource;
use crate::ytdlp::ResolvedStream;
use crate::model::Track;

trait AudioSource: Read + Seek + Send + Sync {}
impl AudioSource for BufReader<File> {}
impl AudioSource for crate::http::HttpStream {}

pub struct PlayRequest {
    pub track: Track,
    pub resolved: Option<ResolvedStream>,
    pub start: Duration,
    pub duration: Option<Duration>,
    pub fade: Duration,
    pub warm: Option<crate::http::HttpStream>,
}

pub enum Command {
    Play(Box<PlayRequest>),
    Pause,
    Resume,
    Seek(Duration),
    SetVolume(f32),
    Stop,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AudioState {
    Idle,
    Loading,
    Playing,
    Paused,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub state: AudioState,
    pub track_id: Option<u64>,
    pub position: Duration,
    pub duration: Option<Duration>,
}

pub struct PlayerHandle {
    tx: Sender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl PlayerHandle {
    pub fn spawn(
        volume: f32,
        events: Sender<Event>,
        eq: Arc<EqParams>,
        normalize: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let snapshot = Arc::new(Mutex::new(Snapshot {
            state: AudioState::Idle,
            track_id: None,
            position: Duration::ZERO,
            duration: None,
        }));
        let shared = snapshot.clone();
        let audio_eq = eq.clone();
        let _ = thread::Builder::new()
            .name("aria-audio".into())
            .spawn(move || run(rx, shared, volume, events, audio_eq, normalize));
        Self { tx, snapshot }
    }

    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }
}

fn lock(snapshot: &Arc<Mutex<Snapshot>>) -> std::sync::MutexGuard<'_, Snapshot> {
    snapshot.lock().unwrap_or_else(|err| err.into_inner())
}

fn set_state(snapshot: &Arc<Mutex<Snapshot>>, state: AudioState) {
    lock(snapshot).state = state;
}

fn run(
    rx: Receiver<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    volume: f32,
    events: Sender<Event>,
    eq: Arc<EqParams>,
    normalize: Arc<std::sync::atomic::AtomicBool>,
) {
    let device = match DeviceSinkBuilder::open_default_sink() {
        Ok(device) => device,
        Err(err) => {
            set_state(&snapshot, AudioState::Idle);
            let _ = events.send(Event::AudioError {
                message: format!("Audio device unavailable: {err}"),
            });
            return;
        }
    };
    let sinks = [
        RodioPlayer::connect_new(device.mixer()),
        RodioPlayer::connect_new(device.mixer()),
    ];
    sinks[0].set_volume(volume);

    let mut loaded = false;
    let mut volume = volume;
    let mut active = 0usize;
    let mut fade_in: Option<(Instant, Duration)> = None;
    let mut fade_out: Option<(usize, Instant, Duration)> = None;
    let mut paused = false;

    let mut backlog: std::collections::VecDeque<Command> = std::collections::VecDeque::new();

    loop {
        while let Some(command) = backlog.pop_front().or_else(|| rx.try_recv().ok()) {
            match command {
                Command::Play(request) => {
                    let mut queued = Vec::new();
                    while let Ok(next) = rx.try_recv() {
                        queued.push(next);
                    }
                    let (mut request, others) = coalesce_play(request, queued);
                    for other in others.into_iter().rev() {
                        backlog.push_front(other);
                    }
                    let fade = request.fade.min(Duration::from_secs(12));
                    let crossfading = fade > Duration::from_millis(80)
                        && loaded
                        && !sinks[active].empty();
                    {
                        let mut state = lock(&snapshot);
                        state.state = AudioState::Loading;
                        state.track_id = Some(request.track.id);
                        state.position = Duration::ZERO;
                        state.duration = request.duration;
                    }
                    let target = if crossfading { 1 - active } else { active };
                    match load_source(&mut request) {
                        Ok((source, duration_hint, byte_len)) => match build_decoder(source, byte_len) {
                            Ok(decoder) => {
                                let decoder_duration = decoder.total_duration();
                                if crossfading {
                                    sinks[target].stop();
                                    fade_out = Some((active, Instant::now(), fade));
                                    fade_in = Some((Instant::now(), fade));
                                    active = target;
                                } else {
                                    sinks[0].stop();
                                    sinks[1].stop();
                                    active = 0;
                                    fade_out = None;
                                    fade_in = None;
                                }
                                paused = false;
                                sinks[active].set_volume(if crossfading { 0.0 } else { volume });
                                sinks[active].append(NormalizeSource::with_flag(
                                    EqSource::new(decoder, eq.clone()),
                                    normalize.clone(),
                                ));
                                if request.start > Duration::ZERO {
                                    let _ = sinks[active].try_seek(request.start);
                                }
                                sinks[active].play();
                                loaded = true;
                                let mut state = lock(&snapshot);
                                state.state = AudioState::Playing;
                                state.position = request.start;
                                state.duration = duration_hint
                                    .or(decoder_duration)
                                    .or(request.duration);
                            }
                            Err(err) => {
                                loaded = false;
                                set_state(&snapshot, AudioState::Idle);
                                let _ = events.send(Event::AudioError {
                                    message: format!("Unsupported or corrupt audio: {err}"),
                                });
                            }
                        },
                        Err(err) => {
                            loaded = false;
                            set_state(&snapshot, AudioState::Idle);
                            let _ = events.send(Event::AudioError { message: err });
                        }
                    }
                }
                Command::Pause => {
                    if loaded {
                        for sink in &sinks {
                            sink.pause();
                        }
                        paused = true;
                        set_state(&snapshot, AudioState::Paused);
                    }
                }
                Command::Resume => {
                    if loaded {
                        for sink in &sinks {
                            sink.play();
                        }
                        paused = false;
                        set_state(&snapshot, AudioState::Playing);
                    }
                }
                Command::Seek(position) => {
                    let _ = sinks[active].try_seek(position);
                    lock(&snapshot).position = position;
                }
                Command::SetVolume(value) => {
                    volume = value.clamp(0.0, 1.0);
                }
                Command::Stop => {
                    for sink in &sinks {
                        sink.stop();
                    }
                    fade_in = None;
                    fade_out = None;
                    paused = false;
                    loaded = false;
                    let mut state = lock(&snapshot);
                    state.state = AudioState::Idle;
                    state.position = Duration::ZERO;
                }
            }
        }

        if let Some((index, started, len)) = fade_out {
            let progress = (started.elapsed().as_secs_f32() / len.as_secs_f32().max(0.001))
                .clamp(0.0, 1.0);
            sinks[index].set_volume(volume * (1.0 - progress));
            if progress >= 1.0 {
                sinks[index].stop();
                fade_out = None;
            }
        }

        if let Some((started, len)) = fade_in {
            let progress = (started.elapsed().as_secs_f32() / len.as_secs_f32().max(0.001))
                .clamp(0.0, 1.0);
            sinks[active].set_volume(volume * progress);
            if progress >= 1.0 {
                fade_in = None;
            }
        } else if !paused {
            sinks[active].set_volume(volume);
        }

        if loaded {
            if sinks[active].empty() {
                loaded = false;
                fade_in = None;
                set_state(&snapshot, AudioState::Idle);
                let _ = events.send(Event::AudioEnded);
            } else {
                lock(&snapshot).position = sinks[active].get_pos();
            }
        }

        thread::sleep(Duration::from_millis(40));
    }
}

fn build_decoder(
    source: Box<dyn AudioSource>,
    byte_len: Option<u64>,
) -> Result<Decoder<Box<dyn AudioSource>>, rodio::decoder::DecoderError> {
    let mut builder = Decoder::builder().with_data(source).with_seekable(true);
    if let Some(len) = byte_len {
        builder = builder.with_byte_len(len);
    }
    builder.build()
}

pub fn coalesce_play(first: Box<PlayRequest>, queued: Vec<Command>) -> (Box<PlayRequest>, Vec<Command>) {
    let mut request = first;
    let mut others = Vec::new();
    for command in queued {
        match command {
            Command::Play(newer) => {
                request = newer;
                others.retain(|kept| matches!(kept, Command::SetVolume(_)));
            }
            other => others.push(other),
        }
    }
    (request, others)
}

type Loaded = (Box<dyn AudioSource>, Option<Duration>, Option<u64>);

fn load_source(
    request: &mut PlayRequest,
) -> Result<Loaded, String> {
    match &request.track.source {
        TrackSource::Local(path) => {
            let file = File::open(path)
                .map_err(|err| format!("Cannot open {}: {err}", path.display()))?;
            let len = file.metadata().ok().map(|meta| meta.len());
            Ok((
                Box::new(BufReader::new(file)),
                request.track.duration,
                len,
            ))
        }
        TrackSource::Yt { .. } | TrackSource::Jiosaavn { .. } => {
            let resolved = request
                .resolved
                .as_ref()
                .ok_or_else(|| "Stream was not resolved".to_string())?;
            let stream = match request.warm.take() {
                Some(mut warmed) => {
                    use std::io::Seek as _;
                    let _ = warmed.seek(std::io::SeekFrom::Start(0));
                    warmed
                }
                None => crate::http::HttpStream::open(
                    crate::http::stream_agent(),
                    resolved.url.clone(),
                    resolved.headers.clone(),
                )?,
            };
            let duration = resolved.duration.or(request.duration);
            let len = stream.len();
            Ok((Box::new(stream), duration, len))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_downloaded_aac_in_mp4() {
        let path = std::env::temp_dir().join(format!("aria-tone-{}.m4a", std::process::id()));
        std::fs::write(&path, include_bytes!("fixtures/tone.m4a")).expect("write fixture");
        let file = File::open(&path).expect("open");
        let len = file.metadata().ok().map(|meta| meta.len());
        let decoder = build_decoder(Box::new(BufReader::new(file)), len).expect("decode m4a");
        assert!(decoder.count() > 5_000);
        std::fs::remove_file(&path).ok();
    }

    fn request(id: u64) -> Box<PlayRequest> {
        let mut track = Track::local(std::path::Path::new("/music/a.mp3"));
        track.id = id;
        Box::new(PlayRequest {
            track,
            resolved: None,
            start: Duration::ZERO,
            duration: None,
            fade: Duration::ZERO,
            warm: None,
        })
    }

    #[test]
    fn only_the_latest_queued_play_is_loaded() {
        let queued = vec![
            Command::SetVolume(0.4),
            Command::Play(request(2)),
            Command::Pause,
            Command::Play(request(3)),
            Command::Seek(Duration::from_secs(5)),
        ];
        let (chosen, others) = coalesce_play(request(1), queued);
        assert_eq!(chosen.track.id, 3);
        assert_eq!(others.len(), 2);
        assert!(matches!(others[0], Command::SetVolume(_)) || matches!(others[0], Command::Seek(_)));
        assert!(others.iter().any(|c| matches!(c, Command::Seek(_))));
        let (only, none) = coalesce_play(request(9), Vec::new());
        assert_eq!(only.track.id, 9);
        assert!(none.is_empty());
    }
}
