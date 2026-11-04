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

fn wake() {
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

pub fn spawn() -> Jobs {
    let (event_tx, rx) = channel::<Event>();
    Jobs {
        tx: JobTx { events: event_tx.clone() },
        thumbs: crate::cache::ThumbQueue::default(),
        event_tx,
        rx,
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
        Some(ProviderName::Youtube) | Some(ProviderName::YoutubeMusic) => "youtube",
        _ => "jiosaavn",
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

pub fn stream_url(track: &Track) -> Option<(String, ProviderName)> {
    let provider = track.provider_meta.as_ref().map(|meta| meta.provider)?;
    let id = match &track.source {
        crate::model::TrackSource::Yt { id } => id.clone(),
        crate::model::TrackSource::Jiosaavn { id } => id.clone(),
        crate::model::TrackSource::Local(_) => return None,
    };
    Some((format!("/api/audio?source={}&id={}", source_of(Some(provider)), encode(&id)), provider))
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
            let result = match stream_url(&track) {
                Some((url, provider)) => Ok(ResolvedStream { url, headers: Vec::new(), duration: track.duration, provider, bitrate_kbps: None }),
                None => Err("This track cannot be streamed in the browser".to_string()),
            };
            emit(&events, Event::Resolved { gen, track, result });
        }
        Job::Warm { track } => emit(&events, Event::Warmed { track_id: track.id, warmed: None }),
        Job::Probe => {
            let online = web_sys::window().map(|window| window.navigator().on_line()).unwrap_or(true);
            emit(&events, Event::Connectivity { online });
        }
        Job::Region => emit(&events, Event::Region { result: Err("Location is not used in the browser".to_string()) }),
        Job::Lyrics { track } => emit(&events, Event::Lyrics { track_id: track.id, result: Err("Lyrics are not available yet".to_string()) }),
        Job::Radio { seed, .. } => emit(&events, Event::Radio { seed, tracks: Vec::new(), error: Some("Radio is not available in the browser".to_string()) }),
        Job::Download { track, .. } => emit(&events, Event::Download { track, result: Err("Downloads are not available in the browser".to_string()) }),
        Job::FixMatch { track } => emit(&events, Event::FixMatch { track_id: track.id, alternatives: Vec::new(), error: Some("Not available in the browser".to_string()) }),
        Job::ImportMatch { token, index, .. } => emit(&events, Event::ImportMatch { token, index, tracks: Vec::new(), error: Some("Importing is not available in the browser yet".to_string()) }),
        Job::ImportPlaylist { token, .. } => emit(&events, Event::ImportPlaylist { token, tracks: Vec::new(), error: Some("Importing is not available in the browser yet".to_string()) }),
        Job::EnsureYtDlp | Job::DownloadYtDlp | Job::CheckUpdate | Job::DownloadUpdate { .. } => {}
    }
}
