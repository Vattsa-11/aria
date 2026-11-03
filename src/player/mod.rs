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
    pub analyze: bool,
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
    let mut streaming_only = false;
    let mut stream_fork: Option<(crate::http::HttpStream, Option<u64>)> = None;

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
                        state.position = request.start;
                        state.duration = request.duration;
                    }
                    let target = if crossfading { 1 - active } else { active };
                    match load_source(&mut request) {
                        Ok(Loaded { source, duration: duration_hint, byte_len, fork }) => match match &fork {
                            Some(handle) => build_streaming_decoder(source)
                                .map(|decoder| (decoder, true))
                                .or_else(|_| build_decoder(Box::new(handle.fork()), byte_len).map(|decoder| (decoder, false))),
                            None => build_decoder(source, byte_len).map(|decoder| (decoder, false)),
                        } {
                            Ok((decoder, streaming)) => {
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
                                if request.analyze {
                                    spawn_analysis(&request.track, fork.as_ref().map(|handle| handle.fork()), events.clone());
                                }
                                sinks[active].play();
                                loaded = true;
                                streaming_only = streaming;
                                stream_fork = if streaming { fork.map(|handle| (handle, byte_len)) } else { None };
                                if request.start > Duration::ZERO {
                                    let swapped = match &stream_fork {
                                        Some((handle, len)) if streaming_only => {
                                            swap_to_seekable(&sinks[active], handle, *len, &eq, &normalize, request.start, false)
                                        }
                                        _ => false,
                                    };
                                    if swapped {
                                        streaming_only = false;
                                    } else {
                                        let _ = sinks[active].try_seek(request.start);
                                    }
                                }
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
                    let mut swapped = false;
                    if streaming_only {
                        if let Some((handle, len)) = &stream_fork {
                            set_state(&snapshot, AudioState::Loading);
                            swapped = swap_to_seekable(&sinks[active], handle, *len, &eq, &normalize, position, paused);
                            set_state(&snapshot, if paused { AudioState::Paused } else { AudioState::Playing });
                        }
                    }
                    if swapped {
                        streaming_only = false;
                    } else {
                        let _ = sinks[active].try_seek(position);
                    }
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
                    streaming_only = false;
                    stream_fork = None;
                    let mut state = lock(&snapshot);
                    state.state = AudioState::Idle;
                    state.position = Duration::ZERO;
                }
            }
        }

        if let Some((index, started, len)) = fade_out {
            let progress = (started.elapsed().as_secs_f32() / len.as_secs_f32().max(0.001))
                .clamp(0.0, 1.0);
            sinks[index].set_volume(volume * equal_power(progress).0);
            if progress >= 1.0 {
                sinks[index].stop();
                fade_out = None;
            }
        }

        if let Some((started, len)) = fade_in {
            let progress = (started.elapsed().as_secs_f32() / len.as_secs_f32().max(0.001))
                .clamp(0.0, 1.0);
            sinks[active].set_volume(volume * equal_power(progress).1);
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

pub fn equal_power(progress: f32) -> (f32, f32) {
    let angle = progress.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
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

struct Loaded {
    source: Box<dyn AudioSource>,
    duration: Option<Duration>,
    byte_len: Option<u64>,
    fork: Option<crate::http::HttpStream>,
}

fn head_samples(
    mut decoder: Decoder<Box<dyn AudioSource>>,
    seconds: u32,
) -> (Vec<f32>, u32) {
    use rodio::Source;
    let rate = decoder.sample_rate().get();
    let channels = decoder.channels().get() as usize;
    let factor = (rate / 11_025).max(1) as usize;
    let wanted = rate as usize * channels * seconds as usize;
    let mut out = Vec::with_capacity(wanted / (channels * factor) + 1);
    let mut frame_sum = 0.0f32;
    let mut in_frame = 0usize;
    let mut frames = 0usize;
    let mut block_sum = 0.0f32;
    for (taken, sample) in decoder.by_ref().take(wanted).enumerate() {
        let _ = taken;
        frame_sum += sample;
        in_frame += 1;
        if in_frame == channels {
            block_sum += frame_sum / channels as f32;
            frame_sum = 0.0;
            in_frame = 0;
            frames += 1;
            if frames == factor {
                out.push(block_sum / factor as f32);
                block_sum = 0.0;
                frames = 0;
            }
        }
    }
    (out, rate / factor as u32)
}

fn spawn_analysis(track: &Track, fork: Option<crate::http::HttpStream>, events: Sender<Event>) {
    let id = track.id;
    let local = track.local_path().map(|path| path.to_path_buf());
    let _ = thread::Builder::new().name("aria-analyze".into()).spawn(move || {
        let source: Box<dyn AudioSource> = match (fork, local) {
            (Some(handle), _) => Box::new(handle),
            (None, Some(path)) => match File::open(path) {
                Ok(file) => Box::new(BufReader::new(file)),
                Err(_) => return,
            },
            _ => return,
        };
        let Ok(decoder) = build_streaming_decoder(source) else {
            return;
        };
        let (samples, rate) = head_samples(decoder, 45);
        if let Some(features) = crate::pace::analyze(&samples, rate) {
            let _ = events.send(Event::Features { track_id: id, features });
        }
    });
}

fn build_streaming_decoder(
    source: Box<dyn AudioSource>,
) -> Result<Decoder<Box<dyn AudioSource>>, rodio::decoder::DecoderError> {
    Decoder::builder().with_data(source).with_seekable(false).build()
}

fn swap_to_seekable(
    sink: &RodioPlayer,
    fork: &crate::http::HttpStream,
    byte_len: Option<u64>,
    eq: &Arc<EqParams>,
    normalize: &Arc<std::sync::atomic::AtomicBool>,
    position: Duration,
    paused: bool,
) -> bool {
    let Ok(decoder) = build_decoder(Box::new(fork.fork()), byte_len) else {
        return false;
    };
    sink.stop();
    sink.append(NormalizeSource::with_flag(
        EqSource::new(decoder, eq.clone()),
        normalize.clone(),
    ));
    let _ = sink.try_seek(position);
    if paused {
        sink.pause();
    } else {
        sink.play();
    }
    true
}

fn load_source(
    request: &mut PlayRequest,
) -> Result<Loaded, String> {
    match &request.track.source {
        TrackSource::Local(path) => {
            let file = File::open(path)
                .map_err(|err| format!("Cannot open {}: {err}", path.display()))?;
            let len = file.metadata().ok().map(|meta| meta.len());
            Ok(Loaded {
                source: Box::new(BufReader::new(file)),
                duration: request.track.duration,
                byte_len: len,
                fork: None,
            })
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
            let fork = stream.fork();
            Ok(Loaded {
                source: Box::new(stream),
                duration,
                byte_len: len,
                fork: Some(fork),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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

    fn sine_samples() -> Vec<i16> {
        (0..8000).map(|n| ((n as f32 * 0.1).sin() * 12000.0) as i16).collect()
    }

    fn wav_bytes() -> Vec<u8> {
        let data: Vec<u8> = sine_samples().iter().flat_map(|sample| sample.to_le_bytes()).collect();
        let mut out = b"RIFF".to_vec();
        out.extend((36 + data.len() as u32).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend([1, 0, 1, 0]);
        out.extend(8000u32.to_le_bytes());
        out.extend(16000u32.to_le_bytes());
        out.extend([2, 0, 16, 0]);
        out.extend(b"data");
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    fn aiff_bytes() -> Vec<u8> {
        let data: Vec<u8> = sine_samples().iter().flat_map(|sample| sample.to_be_bytes()).collect();
        let mut out = b"FORM".to_vec();
        out.extend((4 + 26 + 16 + data.len() as u32).to_be_bytes());
        out.extend(b"AIFFCOMM");
        out.extend(18u32.to_be_bytes());
        out.extend(1u16.to_be_bytes());
        out.extend(8000u32.to_be_bytes());
        out.extend(16u16.to_be_bytes());
        out.extend([0x40, 0x0B, 0xFA, 0, 0, 0, 0, 0, 0, 0]);
        out.extend(b"SSND");
        out.extend((8 + data.len() as u32).to_be_bytes());
        out.extend([0u8; 8]);
        out.extend(data);
        out
    }

    #[test]
    fn decodes_wav_and_aiff_so_more_local_files_play() {
        for (name, bytes) in [("wav", wav_bytes()), ("aiff", aiff_bytes())] {
            let path = std::env::temp_dir().join(format!("aria-format-{name}-{}.{name}", std::process::id()));
            std::fs::write(&path, &bytes).expect("write");
            let file = File::open(&path).expect("open");
            let len = file.metadata().ok().map(|meta| meta.len());
            let decoder = build_decoder(Box::new(BufReader::new(file)), len).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(decoder.count() >= 7_000, "{name}");
            std::fs::remove_file(&path).ok();
        }
    }

    struct Counting {
        file: File,
        highest: Arc<Mutex<u64>>,
    }

    impl std::io::Read for Counting {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let read = self.file.read(buf)?;
            let position = self.file.stream_position()?;
            let mut highest = self.highest.lock().unwrap();
            *highest = (*highest).max(position);
            Ok(read)
        }
    }

    impl std::io::Seek for Counting {
        fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
            self.file.seek(position)
        }
    }

    impl AudioSource for Counting {}

    fn fragmented_fixture(name: &str) -> (std::path::PathBuf, u64) {
        let path = std::env::temp_dir().join(format!("aria-dash-{name}-{}.m4a", std::process::id()));
        std::fs::write(&path, include_bytes!("fixtures/dash.m4a")).expect("write fixture");
        let len = std::fs::metadata(&path).unwrap().len();
        (path, len)
    }

    #[test]
    fn head_analysis_reads_samples_from_a_real_file() {
        let (path, _) = fragmented_fixture("head");
        let source = Counting { file: File::open(&path).unwrap(), highest: Arc::new(Mutex::new(0)) };
        let decoder = build_streaming_decoder(Box::new(source)).expect("decode");
        let (samples, rate) = head_samples(decoder, 45);
        assert!((10_000..12_000).contains(&rate), "rate {rate}");
        assert!(samples.len() as f32 > rate as f32 * 20.0, "got {} samples", samples.len());
        assert!(crate::pace::analyze(&samples, rate).is_some());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn streaming_decoder_starts_after_reading_only_the_beginning() {
        let (path, len) = fragmented_fixture("start");
        let highest = Arc::new(Mutex::new(0u64));
        let source = Counting { file: File::open(&path).unwrap(), highest: highest.clone() };
        let decoder = build_streaming_decoder(Box::new(source)).expect("streaming decode");
        let read = *highest.lock().unwrap();
        assert!(read * 4 < len, "read {read} of {len} bytes before starting");
        assert!(decoder.take(2000).count() > 1000);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn seekable_decoder_scans_every_fragment_first() {
        let (path, len) = fragmented_fixture("scan");
        let highest = Arc::new(Mutex::new(0u64));
        let source = Counting { file: File::open(&path).unwrap(), highest: highest.clone() };
        build_decoder(Box::new(source), Some(len)).expect("seekable decode");
        assert!(*highest.lock().unwrap() * 10 >= len * 9, "expected a full scan");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn streaming_decoder_still_seeks_to_the_right_place() {
        use rodio::Source;
        let (path, len) = fragmented_fixture("seek");
        let highest = Arc::new(Mutex::new(0u64));
        let source = Counting { file: File::open(&path).unwrap(), highest: highest.clone() };
        let mut decoder = build_streaming_decoder(Box::new(source)).expect("streaming decode");
        let rate = decoder.sample_rate().get() as f64 * decoder.channels().get() as f64;
        decoder.try_seek(Duration::from_secs(10)).expect("seek");
        let remaining = decoder.count() as f64 / rate;
        assert!((13.0..15.0).contains(&remaining), "about 14s should remain after seeking to 10s of 24s, got {remaining}");
        assert!(*highest.lock().unwrap() <= len);
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
            analyze: false,
        })
    }

    #[test]
    fn crossfade_keeps_the_loudness_steady_through_the_middle() {
        let (out, incoming) = equal_power(0.0);
        assert!((out - 1.0).abs() < 1e-6 && incoming.abs() < 1e-6);
        let (out, incoming) = equal_power(1.0);
        assert!(out.abs() < 1e-6 && (incoming - 1.0).abs() < 1e-6);
        for step in 0..=20 {
            let (out, incoming) = equal_power(step as f32 / 20.0);
            let power = out * out + incoming * incoming;
            assert!((power - 1.0).abs() < 1e-4, "power {power} at step {step}");
        }
        let (out, incoming) = equal_power(0.5);
        assert!(out + incoming > 1.3, "a linear fade would dip to 1.0 here");
        assert_eq!(equal_power(-1.0).0, 1.0);
        assert!((equal_power(5.0).1 - 1.0).abs() < 1e-6);
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
