use crate::model::Track;
use crate::provider::{MetadataResult, Provider, ProviderName, ResolvedStream};
use crate::ytdlp;

pub struct YoutubeMusicProvider;

impl YoutubeMusicProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for YoutubeMusicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for YoutubeMusicProvider {
    fn name(&self) -> ProviderName {
        ProviderName::YoutubeMusic
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<MetadataResult>, String> {
        let tracks = ytdlp::search(query, limit)?;
        let mut results = Vec::with_capacity(tracks.len());
        for track in tracks {
            let track_id = track.yt_id().unwrap_or_default();
            results.push(MetadataResult {
                track_id,
                title: track.title,
                artist: track.artist,
                duration: track.duration,
                thumb: track.thumb.clone(),
                provider: ProviderName::YoutubeMusic,
            });
        }
        Ok(results)
    }

    fn resolve(&self, track: &Track) -> Result<Option<ResolvedStream>, String> {
        if let Some(ytid) = track.yt_id() {
            let url = format!("https://music.youtube.com/watch?v={ytid}");
            let resolved = ytdlp::resolve_ytm(&url)?;
            return Ok(Some(resolved));
        }
        if let Some(url) = track.watch_url() {
            return ytdlp::resolve_best_audio(&url);
        }
        Ok(None)
    }

}
