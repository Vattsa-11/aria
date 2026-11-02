use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use egui::ColorImage;

use crate::model::Track;
use crate::ytdlp::ResolvedStream;

pub enum Job {
    Search {
        query: String,
        limit: usize,
        provider: Option<crate::provider::ProviderName>,
    },
    Resolve { gen: u64, track: Track },
    FixMatch { track: Track },
    Download {
        track: Track,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    },

    Probe,
    EnsureYtDlp,
    DownloadYtDlp,
    Lyrics { track: Track },
    Radio { seed: String },
    ImportMatch {
        token: u64,
        index: usize,
        query: String,
        duration: Option<std::time::Duration>,
    },
    ImportPlaylist {
        token: u64,
        url: String,
    },
}

pub enum ThumbJob {
    Fetch { track_id: u64, url: String },
}

pub enum Event {
    Lyrics {
        track_id: u64,
        result: Result<String, String>,
    },
    Radio {
        seed: String,
        tracks: Vec<Track>,
        error: Option<String>,
    },
    Search {
        query: String,
        provider: Option<crate::provider::ProviderName>,
        results: Vec<Track>,
        error: Option<String>,
    },
    Resolved {
        gen: u64,
        track: Track,
        result: Result<ResolvedStream, String>,
    },
    FixMatch {
        track_id: u64,
        alternatives: Vec<Track>,
        error: Option<String>,
    },
    Download {
        track: Track,
        result: Result<std::path::PathBuf, String>,
    },
    YtDlp {
        message: String,
        ready: bool,
    },
    Thumb {
        track_id: u64,
        image: Option<ColorImage>,
    },
    AudioEnded,
    AudioError {
        message: String,
    },
    Connectivity {
        online: bool,
    },
    ImportMatch {
        token: u64,
        index: usize,
        tracks: Vec<Track>,
        error: Option<String>,
    },
    ImportPlaylist {
        token: u64,
        tracks: Vec<Track>,
        error: Option<String>,
    },
}

pub struct Jobs {
    pub tx: Sender<Job>,
    pub thumb_tx: Sender<ThumbJob>,
    pub event_tx: Sender<Event>,
    pub rx: Receiver<Event>,
}

pub fn spawn() -> Jobs {
    let (tx, job_rx) = channel::<Job>();
    let (thumb_tx, thumb_rx) = channel::<ThumbJob>();
    let (event_tx, rx) = channel::<Event>();

    let worker_events = event_tx.clone();
    let thumb_events = event_tx.clone();
    let _ = thread::Builder::new()
        .name("aria-jobs".into())
        .spawn(move || {
            while let Ok(job) = job_rx.recv() {
                handle(job, &worker_events);
            }
        });

    let _ = thread::Builder::new()
        .name("aria-thumbs".into())
        .spawn(move || {
            while let Ok(job) = thumb_rx.recv() {
                match job {
                    ThumbJob::Fetch { track_id, url } => {
                        let key = track_id.to_string();
                        let bytes = crate::cache::load_disk(&key)
                            .or_else(|| crate::cache::fetch_bytes(&url));
                        let image = bytes
                            .as_deref()
                            .and_then(crate::cache::decode);
                        if let Some(bytes) = &bytes {
                            crate::cache::store_disk(&key, bytes);
                        }
                        let _ = thumb_events.send(Event::Thumb { track_id, image });
                    }
                }
            }
        });

    Jobs {
        tx,
        thumb_tx,
        event_tx,
        rx,
    }
}

fn is_indian_language(query: &str) -> bool {
    query.chars().any(|c| {
        let cp = c as u32;
        (0x0900..=0x097F).contains(&cp)
            || (0x0980..=0x09FF).contains(&cp)
            || (0x0A00..=0x0A7F).contains(&cp)
            || (0x0A80..=0x0AFF).contains(&cp)
            || (0x0B00..=0x0B7F).contains(&cp)
            || (0x0B80..=0x0BFF).contains(&cp)
            || (0x0C00..=0x0C7F).contains(&cp)
            || (0x0C80..=0x0CFF).contains(&cp)
            || (0x0D00..=0x0D7F).contains(&cp)
            || (0x0D80..=0x0DFF).contains(&cp)
    })
}

fn provider_search(
    query: &str,
    limit: usize,
    only: Option<crate::provider::ProviderName>,
) -> Result<Vec<Track>, String> {
    let mut providers = crate::provider::all_providers();
    if let Some(only) = only {
        providers.retain(|p| p.name() == only);
    } else if is_indian_language(query) {
        providers.sort_by_key(|p| {
            if p.name() == crate::provider::ProviderName::Jiosaavn {
                0
            } else {
                1
            }
        });
    }
    let mut last_err = "No providers available".to_string();
    for provider in providers {
        match provider.search(query, limit) {
            Ok(results) if !results.is_empty() => {
                return Ok(results.into_iter().map(|r| r.into_track()).collect());
            }
            Ok(_) => continue,
            Err(err) => {
                last_err = err;
                continue;
            }
        }
    }
    Err(last_err)
}

fn split_query(query: &str) -> (&str, &str) {
    match query.split_once(' ') {
        Some((artist, title)) if !artist.is_empty() => (title, artist),
        _ => (query, ""),
    }
}

fn handle(job: Job, event_tx: &Sender<Event>) {
    match job {
        Job::Search {
            query,
            limit,
            provider,
        } => {
            let (mut results, error) = match provider_search(&query, limit, provider) {
                Ok(results) => (results, None),
                Err(err) => (Vec::new(), Some(err)),
            };
            crate::matchscore::rank(&mut results, &query, "", None);
            let _ = event_tx.send(Event::Search {
                query,
                provider,
                results,
                error,
            });
        }
        Job::Resolve { gen, track } => {
            let result = crate::provider::resolve_chain(&track);
            let _ = event_tx.send(Event::Resolved { gen, track, result });
        }
        Job::FixMatch { track } => {
            let query = format!("{} - {}", track.artist, track.title);
            let query = query.trim_matches(['-', ' ']).to_string();
            let mut alternatives: Vec<Track> = Vec::new();
            let mut last_err: Option<String> = None;
            for provider in crate::provider::all_providers() {
                match provider.search(&query, 5) {
                    Ok(results) => {
                        for result in results {
                            let candidate = result.into_track();
                            if candidate.yt_id() == track.yt_id() {
                                continue;
                            }
                            if !alternatives.iter().any(|t: &Track| t.id == candidate.id) {
                                alternatives.push(candidate);
                            }
                        }
                    }
                    Err(err) => {
                        last_err = Some(err);
                    }
                }
            }
            crate::matchscore::rank(&mut alternatives, &track.title, &track.artist, track.duration);
            let error = if alternatives.is_empty() {
                last_err.or_else(|| Some("No alternative matches found".to_string()))
            } else {
                None
            };
            let _ = event_tx.send(Event::FixMatch {
                track_id: track.id,
                alternatives,
                error,
            });
        }
        Job::Download { track, cancel } => {
            let result = match track.watch_url() {
                Some(url) => match crate::downloads::ensure_dir() {
                    Ok(dir) => crate::ytdlp::download_audio(
                        &url,
                        &dir,
                        &track.id.to_string(),
                        cancel,
                    ),
                    Err(err) => Err(err),
                },
                None => Err("Only online tracks can be downloaded".to_string()),
            };
            let _ = event_tx.send(Event::Download { track, result });
        }
        Job::ImportMatch {
            token,
            index,
            query,
            duration,
        } => {
            let (mut tracks, error) = match provider_search(&query, 5, None) {
                Ok(results) => (results, None),
                Err(err) => (Vec::new(), Some(err)),
            };
            let (title_hint, artist_hint) = split_query(&query);
            crate::matchscore::rank(&mut tracks, title_hint, artist_hint, duration);
            let _ = event_tx.send(Event::ImportMatch {
                token,
                index,
                tracks,
                error,
            });
        }
        Job::ImportPlaylist { token, url } => {
            let (tracks, error) = match crate::ytdlp::playlist_tracks(&url) {
                Ok(tracks) => (tracks, None),
                Err(err) => (Vec::new(), Some(err)),
            };
            let _ = event_tx.send(Event::ImportPlaylist {
                token,
                tracks,
                error,
            });
        }
        Job::Lyrics { track } => {
            let track_id = track.id;
            let result = match crate::db::load_lyrics(track_id) {
                Some(body) => Ok(body),
                None => crate::lyrics::fetch(&track).inspect(|body| {
                    let _ = crate::db::save_lyrics(track_id, body);
                }),
            };
            let _ = event_tx.send(Event::Lyrics { track_id, result });
        }
        Job::Radio { seed } => {
            let query = format!("{seed} radio mix");
            let (tracks, error) = match provider_search(&query, 20, None) {
                Ok(results) => (results, None),
                Err(err) => (Vec::new(), Some(err)),
            };
            let _ = event_tx.send(Event::Radio {
                seed,
                tracks,
                error,
            });
        }
        Job::Probe => {
            let _ = event_tx.send(Event::Connectivity {
                online: crate::net::is_online(),
            });
        }
        Job::EnsureYtDlp => {
            let event = match crate::ytdlp::discover() {
                Some(_) => {
                    let version = crate::ytdlp::version().unwrap_or_default();
                    Event::YtDlp {
                        message: if version.is_empty() {
                            "yt-dlp ready".to_string()
                        } else {
                            format!("yt-dlp {version}")
                        },
                        ready: true,
                    }
                }
                None => Event::YtDlp {
                    message: "yt-dlp is not installed".to_string(),
                    ready: false,
                },
            };
            let _ = event_tx.send(event);
        }
        Job::DownloadYtDlp => {
            let _ = event_tx.send(Event::YtDlp {
                message: "Downloading yt-dlp...".to_string(),
                ready: false,
            });
            let event = match crate::ytdlp::download_latest() {
                Ok(_) => Event::YtDlp {
                    message: "yt-dlp ready".to_string(),
                    ready: true,
                },
                Err(err) => Event::YtDlp {
                    message: format!("yt-dlp download failed: {err}"),
                    ready: false,
                },
            };
            let _ = event_tx.send(event);
        }
    }
}
