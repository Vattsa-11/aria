use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{AudioContext, BiquadFilterNode, BiquadFilterType, GainNode, HtmlAudioElement};

use super::{equal_power, AudioState, Command, PlayRequest, Snapshot};
use crate::equalizer::{EqParams, BAND_COUNT, BAND_FREQS, BAND_Q};
use crate::jobs::Event;

const CURVE_POINTS: usize = 64;

struct Deck {
    element: HtmlAudioElement,
    gain: GainNode,
    track_id: Option<u64>,
    duration: Option<Duration>,
    loading: bool,
    preloaded: Option<String>,
}

struct Graph {
    context: AudioContext,
    decks: [Deck; 2],
    filters: Vec<BiquadFilterNode>,
    preamp: GainNode,
    master: GainNode,
    active: usize,
    eq: Arc<EqParams>,
    eq_version: u64,
    volume: f32,
    events: Sender<Event>,
    embed: Option<Embed>,
}

struct Embed {
    track_id: u64,
    duration: Option<Duration>,
    ended_sent: bool,
    error_sent: bool,
}

pub fn embed_id(url: &str) -> Option<&str> {
    url.strip_prefix("yt:").filter(|id| !id.is_empty())
}

#[derive(Clone)]
pub struct PlayerHandle {
    graph: Rc<RefCell<Option<Graph>>>,
    pending_volume: Rc<RefCell<f32>>,
    events: Sender<Event>,
    eq: Arc<EqParams>,
}

async fn offline_url(id: u64) -> Option<String> {
    let window = web_sys::window()?;
    let function = js_sys::Reflect::get(&window, &"ariaOfflineUrl".into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    let promise = function.call1(&window, &id.to_string().into()).ok()?;
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise)).await.ok()?.as_string()
}

fn wake_up() {
    super::super::jobs::wake();
}

fn build(volume: f32, events: Sender<Event>, eq: Arc<EqParams>) -> Result<Graph, String> {
    let context = AudioContext::new().map_err(|_| "Audio is not available in this browser".to_string())?;
    let master = context.create_gain().map_err(|_| "audio graph")?;
    master.gain().set_value(volume);
    let preamp = context.create_gain().map_err(|_| "audio graph")?;
    let mut filters = Vec::with_capacity(BAND_COUNT);
    for frequency in BAND_FREQS {
        let filter = context.create_biquad_filter().map_err(|_| "audio graph")?;
        filter.set_type(BiquadFilterType::Peaking);
        filter.frequency().set_value(frequency);
        filter.q().set_value(BAND_Q);
        filters.push(filter);
    }
    let mut decks = Vec::new();
    for _ in 0..2 {
        let element = HtmlAudioElement::new().map_err(|_| "audio element")?;
        element.set_preload("auto");
        element.set_cross_origin(Some("anonymous"));
        let source = context.create_media_element_source(&element).map_err(|_| "audio source")?;
        let gain = context.create_gain().map_err(|_| "audio graph")?;
        source.connect_with_audio_node(&gain).map_err(|_| "audio graph")?;
        gain.connect_with_audio_node(&preamp).map_err(|_| "audio graph")?;
        decks.push(Deck { element, gain, track_id: None, duration: None, loading: false, preloaded: None });
    }
    let mut previous: web_sys::AudioNode = preamp.clone().into();
    for filter in &filters {
        previous.connect_with_audio_node(filter).map_err(|_| "audio graph")?;
        previous = filter.clone().into();
    }
    previous.connect_with_audio_node(&master).map_err(|_| "audio graph")?;
    master.connect_with_audio_node(&context.destination()).map_err(|_| "audio graph")?;
    let [first, second]: [Deck; 2] = decks.try_into().map_err(|_| "audio decks")?;
    let mut graph = Graph { context, decks: [first, second], filters, preamp, master, active: 0, eq, eq_version: 0, volume, events, embed: None };
    graph.sync_eq();
    for index in 0..2 {
        graph.listen(index);
    }
    Ok(graph)
}

impl Graph {
    fn sync_eq(&mut self) {
        let version = self.eq.version();
        if version == self.eq_version {
            return;
        }
        self.eq_version = version;
        let (enabled, preamp_db, gains) = self.eq.snapshot();
        self.preamp.gain().set_value(if enabled { 10f32.powf(preamp_db / 20.0) } else { 1.0 });
        for (filter, gain) in self.filters.iter().zip(gains) {
            filter.gain().set_value(if enabled { gain } else { 0.0 });
        }
    }

    fn listen(&self, index: usize) {
        let element = self.decks[index].element.clone();
        let events = self.events.clone();
        let ended_element = element.clone();
        let ended = Closure::<dyn FnMut()>::new(move || {
            if !ended_element.ended() {
                return;
            }
            let _ = events.send(Event::AudioEnded);
            wake_up();
        });
        let _ = element.add_event_listener_with_callback("ended", ended.as_ref().unchecked_ref());
        ended.forget();
        let events = self.events.clone();
        let failed_element = element.clone();
        let failed = Closure::<dyn FnMut()>::new(move || {
            if failed_element.src().is_empty() {
                return;
            }
            let message = failed_element.error().map(|error| error.message()).filter(|text| !text.is_empty()).unwrap_or_else(|| "The browser could not play this track".to_string());
            let _ = events.send(Event::AudioError { message });
            wake_up();
        });
        let _ = element.add_event_listener_with_callback("error", failed.as_ref().unchecked_ref());
        failed.forget();
    }

    fn ramp(gain: &GainNode, from_to: impl Fn(f32) -> f32, seconds: f64, now: f64) {
        let mut curve: Vec<f32> = (0..CURVE_POINTS).map(|step| from_to(step as f32 / (CURVE_POINTS - 1) as f32)).collect();
        let param = gain.gain();
        let _ = param.cancel_scheduled_values(now);
        let _ = param.set_value_curve_at_time(&mut curve, now, seconds.max(0.05));
    }

    fn stop_embed(&mut self) {
        if self.embed.take().is_some() {
            crate::install::call_named("ariaYTStop");
        }
    }

    fn play_embed(&mut self, request: &PlayRequest, id: &str) {
        for deck in &mut self.decks {
            let _ = deck.element.pause();
            deck.track_id = None;
            deck.loading = false;
        }
        self.embed = Some(Embed { track_id: request.track.id, duration: request.duration, ended_sent: false, error_sent: false });
        crate::install::call_with("ariaYTVolume", &format!("{}", (self.volume * 100.0).round()));
        crate::install::call_with("ariaYTLoad", id);
    }

    fn embed_snapshot(&mut self) -> Option<Snapshot> {
        let embed = self.embed.as_mut()?;
        let info = crate::install::yt_state()?;
        let name = info["state"].as_str().unwrap_or("buffering");
        let state = match name {
            "playing" => AudioState::Playing,
            "paused" => AudioState::Paused,
            "ended" | "idle" => AudioState::Idle,
            _ => AudioState::Loading,
        };
        if name == "ended" && !embed.ended_sent {
            embed.ended_sent = true;
            let _ = self.events.send(Event::AudioEnded);
            wake_up();
        }
        if name == "error" && !embed.error_sent {
            embed.error_sent = true;
            let code = info["error"].as_i64().unwrap_or(0);
            let message = if code == 101 || code == 150 {
                "The owner does not allow this song to be played outside YouTube".to_string()
            } else {
                "The YouTube player could not play this song".to_string()
            };
            let _ = self.events.send(Event::AudioError { message });
            wake_up();
        }
        let reported = info["duration"].as_f64().filter(|secs| *secs > 0.0);
        Some(Snapshot {
            state,
            track_id: Some(embed.track_id),
            position: Duration::from_secs_f64(info["position"].as_f64().unwrap_or(0.0).max(0.0)),
            duration: reported.map(Duration::from_secs_f64).or(embed.duration),
        })
    }

    fn play(&mut self, request: Box<PlayRequest>) {
        let Some(stream) = request.resolved.as_ref() else {
            let _ = self.events.send(Event::AudioError { message: "This track cannot be played in the browser".to_string() });
            return;
        };
        if let Some(id) = embed_id(&stream.url) {
            let id = id.to_string();
            self.play_embed(&request, &id);
            return;
        }
        self.stop_embed();
        let _ = self.context.resume();
        let now = self.context.current_time();
        let crossfade = request.fade > Duration::from_millis(50) && self.is_playing(self.active);
        let idle_ready = self.decks[1 - self.active].preloaded.as_deref() == Some(stream.url.as_str());
        let next = if crossfade || idle_ready { 1 - self.active } else { self.active };
        let old = self.active;
        let seconds = request.fade.as_secs_f64();
        {
            let deck = &mut self.decks[next];
            let reuse = deck.preloaded.as_deref() == Some(stream.url.as_str());
            if !reuse {
                let _ = deck.element.pause();
                deck.element.set_src(&stream.url);
            }
            deck.preloaded = None;
            deck.element.set_current_time(request.start.as_secs_f64());
            deck.track_id = Some(request.track.id);
            deck.duration = request.duration.or(stream.duration);
            deck.loading = true;
            if let Ok(promise) = deck.element.play() {
                let events = self.events.clone();
                let rejected = Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(move |_| {
                    let _ = events.send(Event::AudioError { message: "Press play to start audio".to_string() });
                    wake_up();
                });
                let _ = promise.catch(&rejected);
                rejected.forget();
            }
        }
        if crossfade {
            Self::ramp(&self.decks[next].gain, |t| equal_power(t).1, seconds, now);
            Self::ramp(&self.decks[old].gain, |t| equal_power(t).0, seconds, now);
            let element = self.decks[old].element.clone();
            let stop = Closure::<dyn FnMut()>::new(move || {
                let _ = element.pause();
            });
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(stop.as_ref().unchecked_ref(), (seconds * 1000.0) as i32 + 100);
            }
            stop.forget();
        } else {
            let _ = self.decks[next].gain.gain().cancel_scheduled_values(now);
            self.decks[next].gain.gain().set_value(1.0);
            if old != next {
                let _ = self.decks[old].element.pause();
            }
        }
        self.active = next;
    }

    fn preload(&mut self, url: String) {
        if embed_id(&url).is_some() {
            return;
        }
        let idle = 1 - self.active;
        let deck = &mut self.decks[idle];
        if deck.preloaded.as_deref() == Some(url.as_str()) || (deck.track_id.is_some() && !deck.element.paused()) {
            return;
        }
        deck.element.set_src(&url);
        deck.element.load();
        deck.preloaded = Some(url);
    }

    fn is_playing(&self, index: usize) -> bool {
        let deck = &self.decks[index];
        deck.track_id.is_some() && !deck.element.paused() && !deck.element.ended()
    }

    fn snapshot(&mut self) -> Snapshot {
        self.sync_eq();
        if let Some(snapshot) = self.embed_snapshot() {
            return snapshot;
        }
        let deck = &mut self.decks[self.active];
        let element = &deck.element;
        if deck.track_id.is_none() {
            return Snapshot { state: AudioState::Idle, track_id: None, position: Duration::ZERO, duration: None };
        }
        if deck.loading && !element.paused() && element.ready_state() >= 3 {
            deck.loading = false;
        }
        let state = if element.ended() {
            AudioState::Idle
        } else if deck.loading {
            AudioState::Loading
        } else if element.paused() {
            AudioState::Paused
        } else {
            AudioState::Playing
        };
        let reported = element.duration();
        let duration = if reported.is_finite() && reported > 0.0 { Some(Duration::from_secs_f64(reported)) } else { deck.duration };
        Snapshot { state, track_id: deck.track_id, position: Duration::from_secs_f64(element.current_time().max(0.0)), duration }
    }

    fn command(&mut self, command: Command) {
        if self.embed.is_some() {
            match &command {
                Command::Pause => {
                    crate::install::call_named("ariaYTPause");
                    return;
                }
                Command::Resume => {
                    crate::install::call_named("ariaYTPlay");
                    return;
                }
                Command::Seek(to) => {
                    crate::install::call_with("ariaYTSeek", &to.as_secs_f64().to_string());
                    return;
                }
                Command::SetVolume(volume) => {
                    self.volume = *volume;
                    self.master.gain().set_value(*volume);
                    crate::install::call_with("ariaYTVolume", &format!("{}", (volume * 100.0).round()));
                    return;
                }
                Command::Stop => {
                    self.stop_embed();
                    return;
                }
                _ => {}
            }
        }
        match command {
            Command::Play(request) => self.play(request),
            Command::Pause => {
                let _ = self.decks[self.active].element.pause();
            }
            Command::Resume => {
                let _ = self.context.resume();
                let _ = self.decks[self.active].element.play();
            }
            Command::Preload(url) => self.preload(url),
            Command::Seek(to) => self.decks[self.active].element.set_current_time(to.as_secs_f64()),
            Command::SetVolume(volume) => {
                self.volume = volume;
                self.master.gain().set_value(volume);
            }
            Command::Stop => {
                for deck in &mut self.decks {
                    let _ = deck.element.pause();
                    deck.element.set_src("");
                    deck.track_id = None;
                    deck.loading = false;
                }
            }
        }
    }
}

impl PlayerHandle {
    pub fn spawn(volume: f32, events: Sender<Event>, eq: Arc<EqParams>, _normalize: Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { graph: Rc::new(RefCell::new(None)), pending_volume: Rc::new(RefCell::new(volume)), events, eq }
    }

    pub fn send(&self, command: Command) {
        if let Command::Play(request) = &command {
            if request.resolved.is_none() {
                if let Some(id) = request.track.local_path().and_then(crate::downloads::offline_id) {
                    let Command::Play(mut request) = command else { return };
                    let handle = self.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        match offline_url(id).await {
                            Some(url) => {
                                let provider = request.track.provider_meta.as_ref().map(|meta| meta.provider).unwrap_or(crate::provider::ProviderName::Jiosaavn);
                                request.resolved = Some(crate::provider::ResolvedStream { url, headers: Vec::new(), duration: request.duration, provider, bitrate_kbps: None });
                                handle.dispatch(Command::Play(request));
                            }
                            None => {
                                let _ = handle.events.send(Event::AudioError { message: "The offline copy of this song is missing. Download it again.".to_string() });
                                wake_up();
                            }
                        }
                    });
                    return;
                }
            }
        }
        self.dispatch(command);
    }

    fn dispatch(&self, command: Command) {
        if let Command::SetVolume(volume) = &command {
            *self.pending_volume.borrow_mut() = *volume;
        }
        let mut slot = self.graph.borrow_mut();
        if slot.is_none() {
            if !matches!(command, Command::Play(_) | Command::Resume) {
                return;
            }
            match build(*self.pending_volume.borrow(), self.events.clone(), self.eq.clone()) {
                Ok(graph) => *slot = Some(graph),
                Err(message) => {
                    let _ = self.events.send(Event::AudioError { message });
                    return;
                }
            }
        }
        if let Some(graph) = slot.as_mut() {
            graph.command(command);
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        match self.graph.borrow_mut().as_mut() {
            Some(graph) => graph.snapshot(),
            None => Snapshot { state: AudioState::Idle, track_id: None, position: Duration::ZERO, duration: None },
        }
    }
}
