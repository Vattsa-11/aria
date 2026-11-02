use crate::model::Track;
use crate::provider::{MetadataResult, Provider, ProviderName, ResolvedStream};
use crate::ytdlp;

pub struct YoutubeProvider;

impl Default for YoutubeProvider {
    fn default() -> Self {
        Self
    }
}

impl Provider for YoutubeProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Youtube
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
                provider: ProviderName::Youtube,
            });
        }
        Ok(results)
    }

    fn resolve(&self, track: &Track) -> Result<Option<ResolvedStream>, String> {
        let url = track
            .watch_url()
            .ok_or_else(|| "Track cannot be streamed".to_string())?;
        ytdlp::resolve_best_audio(&url)
    }

}
