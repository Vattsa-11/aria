use std::cell::{Cell, RefCell};
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};

use super::{Event, Job, Jobs};
use crate::model::Track;
use crate::provider::{MetadataResult, ProviderName, ResolvedStream};

static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

pub fn set_context(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

pub fn wake() {
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

fn emit(events: &Sender<Event>, event: Event) {
    let _ = events.send(event);
    wake();
}

pub struct JobTx {
    events: Sender<Event>,
}

impl JobTx {
    pub fn send(&self, job: Job) -> Result<(), ()> {
        let events = self.events.clone();
        spawn_local(async move { run(job, events).await });
        Ok(())
    }
}

thread_local! {
    static THUMBS: RefCell<Option<(crate::cache::ThumbQueue, Sender<Event>)>> = const { RefCell::new(None) };
    static IN_FLIGHT: Cell<usize> = const { Cell::new(0) };
}

const THUMB_PARALLEL: usize = 4;

pub fn spawn() -> Jobs {
    let (event_tx, rx) = channel::<Event>();
    let thumbs = crate::cache::ThumbQueue::default();
    THUMBS.with(|slot| *slot.borrow_mut() = Some((thumbs.clone(), event_tx.clone())));
    Jobs { tx: JobTx { events: event_tx.clone() }, thumbs, event_tx, rx }
}

async fn fetch_bytes(path: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("no window")?;
    let response = JsFuture::from(window.fetch_with_str(path)).await.map_err(|_| "offline".to_string())?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| "unexpected response".to_string())?;
    if !response.ok() {
        return Err(format!("status {}", response.status()));
    }
    let buffer = JsFuture::from(response.array_buffer().map_err(|_| "unreadable".to_string())?).await.map_err(|_| "unreadable".to_string())?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

pub fn pump_thumbs() {
    loop {
        if IN_FLIGHT.with(Cell::get) >= THUMB_PARALLEL {
            return;
        }
        let next = THUMBS.with(|slot| slot.borrow().as_ref().and_then(|(queue, events)| queue.pop().map(|job| (job, events.clone()))));
        let Some((job, events)) = next else { return };
        IN_FLIGHT.with(|count| count.set(count.get() + 1));
        spawn_local(async move {
            let image = if job.source.starts_with("http") {
                match fetch_bytes(&format!("/api/image?u={}", encode(&job.source))).await.and_then(|bytes| crate::cache::decode(&bytes, job.letterboxed)) {
                    Ok(image) => Some(image),
                    Err(reason) => {
                        web_sys::console::warn_1(&format!("thumbnail {}: {reason}", job.track_id).into());
                        None
                    }
                }
            } else {
                None
            };
            emit(&events, Event::Thumb { track_id: job.track_id, image });
            IN_FLIGHT.with(|count| count.set(count.get().saturating_sub(1)));
            pump_thumbs();
        });
    }
}

async fn get_json(path: &str) -> Result<Value, String> {
    let window = web_sys::window().ok_or("no window")?;
    let response = JsFuture::from(window.fetch_with_str(path)).await.map_err(|_| "You appear to be offline".to_string())?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| "unexpected response".to_string())?;
    let text = JsFuture::from(response.text().map_err(|_| "unreadable response".to_string())?)
        .await
        .map_err(|_| "unreadable response".to_string())?
        .as_string()
        .unwrap_or_default();
    let json: Value = serde_json::from_str(&text).map_err(|_| format!("Server answered {}", response.status()))?;
    if !response.ok() {
        return Err(json["error"].as_str().unwrap_or("Request failed").to_string());
    }
    Ok(json)
}

fn encode(value: &str) -> String {
    js_sys::encode_uri_component(value).as_string().unwrap_or_default()
}

fn source_of(provider: Option<ProviderName>) -> &'static str {
    match provider {
        Some(ProviderName::Jiosaavn) => "jiosaavn",
        _ => "youtube",
    }
}

pub fn provider_of(source: &str) -> ProviderName {
    if source == "youtube" {
        ProviderName::YoutubeMusic
    } else {
        ProviderName::Jiosaavn
    }
}

pub fn tracks_from(json: &Value) -> Vec<Track> {
    json.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let secs = item["duration"].as_f64().filter(|secs| *secs > 0.0)?;
                    Some(MetadataResult {
                        track_id: item["id"].as_str()?.to_string(),
                        title: item["title"].as_str()?.to_string(),
                        artist: item["artist"].as_str().unwrap_or_default().to_string(),
                        duration: Some(std::time::Duration::from_secs_f64(secs)),
                        thumb: item["thumb"].as_str().map(String::from),
                        provider: provider_of(item["source"].as_str().unwrap_or("jiosaavn")),
                    })
                })
                .map(MetadataResult::into_track)
                .collect()
        })
        .unwrap_or_default()
}

thread_local! {
    static RESOLVED: RefCell<std::collections::HashMap<u64, (String, ProviderName)>> = RefCell::new(std::collections::HashMap::new());
    static LOOKING: RefCell<std::collections::HashSet<u64>> = RefCell::new(std::collections::HashSet::new());
    static FAILED: RefCell<std::collections::HashMap<u64, web_time::Instant>> = RefCell::new(std::collections::HashMap::new());
}

const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

fn resolve_path(track: &Track) -> Option<String> {
    let provider = track.provider_meta.as_ref().map(|meta| meta.provider)?;
    let id = match &track.source {
        crate::model::TrackSource::Yt { id } => id.clone(),
        crate::model::TrackSource::Jiosaavn { id } => id.clone(),
        crate::model::TrackSource::Local(_) => return None,
    };
    let seconds = track.duration.map(|duration| duration.as_secs()).unwrap_or(0);
    Some(format!("/api/resolve?source={}&id={}&t={}&a={}&d={}", source_of(Some(provider)), encode(&id), encode(&track.title), encode(track.display_artist()), seconds))
}

async fn resolve_url(track: &Track) -> Result<(String, ProviderName), String> {
    if let Some(hit) = RESOLVED.with(|map| map.borrow().get(&track.id).cloned()) {
        return Ok(hit);
    }
    let path = resolve_path(track).ok_or("This track cannot be streamed in the browser")?;
    let json = get_json(&path).await?;
    let url = json["url"].as_str().ok_or("No playable source found for this song")?.to_string();
    let provider = if json["source"].as_str() == Some("jiosaavn") { ProviderName::Jiosaavn } else { ProviderName::YoutubeMusic };
    RESOLVED.with(|map| {
        let mut map = map.borrow_mut();
        if map.len() > 200 {
            map.clear();
        }
        map.insert(track.id, (url.clone(), provider));
    });
    Ok((url, provider))
}

pub fn peek_url(track: &Track) -> Option<String> {
    if let Some((url, _)) = RESOLVED.with(|map| map.borrow().get(&track.id).cloned()) {
        return Some(url);
    }
    if FAILED.with(|map| map.borrow().get(&track.id).is_some_and(|at| at.elapsed() < RETRY_AFTER)) {
        return None;
    }
    if !LOOKING.with(|set| set.borrow_mut().insert(track.id)) {
        return None;
    }
    let track = track.clone();
    spawn_local(async move {
        if resolve_url(&track).await.is_err() {
            FAILED.with(|map| map.borrow_mut().insert(track.id, web_time::Instant::now()));
        }
        LOOKING.with(|set| set.borrow_mut().remove(&track.id));
    });
    None
}

fn js_error(value: wasm_bindgen::JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }
    js_sys::Reflect::get(&value, &"message".into()).ok().and_then(|message| message.as_string()).unwrap_or_else(|| "Download failed".to_string())
}

async fn download_track(track: &Track, events: &Sender<Event>) -> Result<std::path::PathBuf, String> {
    use wasm_bindgen::closure::Closure;
    let (url, _) = resolve_url(track).await?;
    if url.starts_with("yt:") {
        return Err("This song can only be played in the YouTube player, so it cannot be saved offline".to_string());
    }
    let id = track.id;
    let progress_events = events.clone();
    let progress = Closure::<dyn FnMut(f64, f64)>::new(move |done: f64, total: f64| {
        let _ = progress_events.send(Event::DownloadProgress { track_id: id, done: done as u64, total: if total > 0.0 { Some(total as u64) } else { None } });
        wake();
    });
    let window = web_sys::window().ok_or("no window")?;
    let function = js_sys::Reflect::get(&window, &"ariaDownload".into())
        .map_err(js_error)?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "Offline storage is not available in this browser".to_string())?;
    let promise = function.call3(&window, &id.to_string().into(), &url.into(), progress.as_ref()).map_err(js_error)?;
    JsFuture::from(js_sys::Promise::from(promise)).await.map_err(js_error)?;
    Ok(std::path::PathBuf::from(format!("offline/{id}")))
}

async fn run(job: Job, events: Sender<Event>) {
    match job {
        Job::Search { query, provider, .. } => {
            let path = format!("/api/search?source={}&q={}", source_of(provider), encode(&query));
            let (mut results, error) = match get_json(&path).await {
                Ok(json) => (tracks_from(&json), None),
                Err(err) => (Vec::new(), Some(err)),
            };
            crate::matchscore::rank(&mut results, &query, "", None);
            emit(&events, Event::Search { query, provider, results, error });
        }
        Job::Trending { language } => {
            let (tracks, error) = match get_json(&format!("/api/trending?lang={}", encode(&language))).await {
                Ok(json) => (tracks_from(&json), None),
                Err(err) => (Vec::new(), Some(err)),
            };
            emit(&events, Event::Trending { language, tracks, error });
        }
        Job::Resolve { gen, track } => {
            let result = resolve_url(&track).await.map(|(url, provider)| ResolvedStream { url, headers: Vec::new(), duration: track.duration, provider, bitrate_kbps: None });
            emit(&events, Event::Resolved { gen, track, result });
        }
        Job::Warm { track } => emit(&events, Event::Warmed { track_id: track.id, warmed: None }),
        Job::Probe => {
            let online = web_sys::window().map(|window| window.navigator().on_line()).unwrap_or(true);
            emit(&events, Event::Connectivity { online });
        }
        Job::Region => {
            let result = match get_json("/api/region").await {
                Ok(json) => crate::region::parse_geo(&json),
                Err(err) => Err(err),
            };
            emit(&events, Event::Region { result });
        }
        Job::Lyrics { track } => {
            let seconds = track.duration.map(|duration| duration.as_secs()).unwrap_or(0);
            let path = format!("/api/lyrics?t={}&a={}&d={}", encode(&track.title), encode(track.display_artist()), seconds);
            let result = match get_json(&path).await {
                Ok(json) => json["lyrics"].as_str().map(String::from).ok_or_else(|| "No lyrics found".to_string()),
                Err(err) => Err(err),
            };
            emit(&events, Event::Lyrics { track_id: track.id, result });
        }
        Job::Radio { seed, .. } => emit(&events, Event::Radio { seed, tracks: Vec::new(), error: Some("Radio is not available in the browser".to_string()) }),
        Job::Download { track, .. } => {
            let result = download_track(&track, &events).await;
            emit(&events, Event::Download { track, result });
        }
        Job::FixMatch { track } => emit(&events, Event::FixMatch { track_id: track.id, alternatives: Vec::new(), error: Some("Not available in the browser".to_string()) }),
        Job::ImportMatch { token, index, .. } => emit(&events, Event::ImportMatch { token, index, tracks: Vec::new(), error: Some("Importing is not available in the browser yet".to_string()) }),
        Job::ImportPlaylist { token, .. } => emit(&events, Event::ImportPlaylist { token, tracks: Vec::new(), error: Some("Importing is not available in the browser yet".to_string()) }),
        Job::EnsureYtDlp | Job::DownloadYtDlp | Job::CheckUpdate | Job::DownloadUpdate { .. } => {}
    }
}
