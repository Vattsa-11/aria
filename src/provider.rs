use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::Track;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderName {
    YoutubeMusic,
    Jiosaavn,
    Youtube,
}

impl ProviderName {
    pub fn label(&self) -> &'static str {
        match self {
            ProviderName::YoutubeMusic => "YT Music",
            ProviderName::Jiosaavn => "JioSaavn",
            ProviderName::Youtube => "YouTube",
        }
    }

    pub fn short(&self) -> &'static str {
        match self {
            ProviderName::YoutubeMusic => "YTM",
            ProviderName::Jiosaavn => "JS",
            ProviderName::Youtube => "YT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMeta {
    pub provider: ProviderName,
    pub bitrate_kbps: Option<u16>,
}

impl ProviderMeta {
    pub fn quality_badge(&self) -> String {
        let p = self.provider.short();
        match self.bitrate_kbps {
            Some(kbps) if kbps > 0 => format!("{p} {kbps}k"),
            _ => p.to_string(),
        }
    }
}

pub struct ResolvedStream {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub duration: Option<Duration>,
    pub provider: ProviderName,
    pub bitrate_kbps: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct MetadataResult {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
    pub thumb: Option<String>,
    pub provider: ProviderName,
}

impl MetadataResult {
    pub fn into_track(self) -> Track {
        let source = match self.provider {
            ProviderName::Jiosaavn => crate::model::TrackSource::Jiosaavn {
                id: self.track_id.clone(),
            },
            _ => crate::model::TrackSource::Yt {
                id: self.track_id.clone(),
            },
        };
        let id = crate::model::hash_id(&format!("{:?}:{}", self.provider, self.track_id));
        let mut track = Track {
            id,
            title: self.title,
            artist: self.artist,
            album: String::new(),
            duration: self.duration,
            source,
            thumb: None,
            provider_meta: Some(ProviderMeta {
                provider: self.provider,
                bitrate_kbps: None,
            }),
        };
        if let Some(thumb) = self.thumb {
            track.thumb = Some(thumb);
        }
        track
    }
}

pub trait Provider: Send + Sync {
    fn name(&self) -> ProviderName;

    fn search(&self, query: &str, limit: usize) -> Result<Vec<MetadataResult>, String>;

    fn resolve(&self, track: &Track) -> Result<Option<ResolvedStream>, String>;
}

#[cfg(not(target_arch = "wasm32"))]
pub fn all_providers() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(crate::provider_youtube_music::YoutubeMusicProvider::new()),
        Box::new(crate::provider_jiosaavn::JiosaavnProvider),
        Box::new(crate::provider_youtube::YoutubeProvider),
    ]
}

#[cfg(not(target_arch = "wasm32"))]
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(12);

#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_chain(track: &Track) -> Result<ResolvedStream, String> {
    let mut last_err = "No providers available".to_string();
    for provider in all_providers() {
        let attempt = track.clone();
        let label = provider_label(&attempt);
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("aria-resolve".into())
            .spawn(move || {
                let _ = tx.send(provider.resolve(&attempt));
            });
        if spawned.is_err() {
            continue;
        }
        match rx.recv_timeout(PROVIDER_TIMEOUT) {
            Ok(Ok(Some(stream))) => return Ok(stream),
            Ok(Ok(None)) => continue,
            Ok(Err(err)) => last_err = err,
            Err(_) => {
                last_err = format!("{label} timed out");
            }
        }
    }
    Err(last_err)
}

#[cfg(not(target_arch = "wasm32"))]
fn provider_label(track: &Track) -> String {
    track
        .provider_meta
        .as_ref()
        .map(|meta| meta.provider.label().to_string())
        .unwrap_or_else(|| "Stream".to_string())
}
