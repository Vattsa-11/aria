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
    Warm { track: Track },
    Region,
    Trending { language: String },
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
    Region {
        result: Result<crate::region::Geo, String>,
    },
    Trending {
        language: String,
        tracks: Vec<Track>,
        error: Option<String>,
    },
    Warmed {
        track_id: u64,
        warmed: Option<(ResolvedStream, crate::http::HttpStream)>,
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
    DownloadProgress {
        track_id: u64,
        done: u64,
        total: Option<u64>,
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
    pub thumbs: crate::cache::ThumbQueue,
    pub event_tx: Sender<Event>,
    pub rx: Receiver<Event>,
}

fn run_download(
    track: &Track,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    events: &Sender<Event>,
) -> Result<std::path::PathBuf, String> {
    let dir = crate::downloads::ensure_dir()?;
    let stem = crate::httpdl::file_stem(track);
    match crate::httpdl::path_for(track) {
        crate::httpdl::DownloadPath::YtDlp => {
            let url = track.watch_url().ok_or("This track has no download address")?;
            crate::ytdlp::download_audio(&url, &dir, &stem, cancel)
        }
        crate::httpdl::DownloadPath::Http => {
            let stream = crate::provider::resolve_chain(track)?;
            let track_id = track.id;
            let last = std::cell::Cell::new(0u64);
            crate::httpdl::fetch(&stream.url, &stream.headers, &dir, &stem, cancel, &|done, total| {
                if done == total.unwrap_or(0) || done >= last.get() + 128 * 1024 {
                    last.set(done);
                    let _ = events.send(Event::DownloadProgress { track_id, done, total });
                }
            })
        }
        crate::httpdl::DownloadPath::Unsupported => Err("Only online tracks can be downloaded".to_string()),
    }
}

pub fn spawn() -> Jobs {
    let (tx, job_rx) = channel::<Job>();
    let thumbs = crate::cache::ThumbQueue::default();
    let (event_tx, rx) = channel::<Event>();

    let worker_events = event_tx.clone();
    let _ = thread::Builder::new()
        .name("aria-jobs".into())
        .spawn(move || {
            while let Ok(job) = job_rx.recv() {
                handle(job, &worker_events);
            }
        });

    for worker in 0..4 {
        let queue = thumbs.clone();
        let events = event_tx.clone();
        let _ = thread::Builder::new()
            .name(format!("aria-thumbs-{worker}"))
            .spawn(move || loop {
                let job = queue.pop_blocking();
                let key = job.track_id.to_string();
                let cached = crate::cache::load_disk(&key);
                let from_disk = cached.is_some();
                let outcome = match cached {
                    Some(bytes) => Ok(bytes),
                    None => crate::cache::fetch_bytes(&job.source),
                }
                .and_then(|bytes| {
                    let image = crate::cache::decode(&bytes, job.letterboxed)?;
                    Ok((bytes, image))
                });
                let image = match outcome {
                    Ok((bytes, image)) => {
                        if !from_disk {
                            crate::cache::store_disk(&key, &bytes);
                        }
                        Some(image)
                    }
                    Err(reason) => {
                        crate::logs::error(&format!("thumbnail {} ({}): {reason}", job.track_id, job.source));
                        None
                    }
                };
                let _ = events.send(Event::Thumb { track_id: job.track_id, image });
            });
    }

    Jobs {
        tx,
        thumbs,
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
        Job::Region => {
            let events = event_tx.clone();
            let _ = thread::Builder::new().name("aria-region".into()).spawn(move || {
                let _ = events.send(Event::Region { result: crate::region::fetch_geo() });
            });
        }
        Job::Trending { language } => {
            let events = event_tx.clone();
            let _ = thread::Builder::new().name("aria-trending".into()).spawn(move || {
                let (tracks, error) = match crate::provider_jiosaavn::trending(&language, 24) {
                    Ok(results) => (results.into_iter().map(|result| result.into_track()).collect(), None),
                    Err(err) => (Vec::new(), Some(err)),
                };
                let _ = events.send(Event::Trending { language, tracks, error });
            });
        }
        Job::Warm { track } => {
            let events = event_tx.clone();
            let _ = thread::Builder::new().name("aria-warm".into()).spawn(move || {
                let warmed = crate::provider::resolve_chain(&track).ok().and_then(|stream| {
                    let http = crate::http::HttpStream::open(
                        crate::http::stream_agent(),
                        stream.url.clone(),
                        stream.headers.clone(),
                    )
                    .ok()?;
                    Some((stream, http))
                });
                let _ = events.send(Event::Warmed { track_id: track.id, warmed });
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
            let events = event_tx.clone();
            let _ = thread::Builder::new()
                .name("aria-download".into())
                .spawn(move || {
                    let result = run_download(&track, cancel, &events);
                    let _ = events.send(Event::Download { track, result });
                });
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
            let radio = if crate::ytdlp::is_youtube_mix(&url) {
                crate::radio::seed_from_url(&url).and_then(|seed| {
                    crate::radio::fetch(&seed, crate::ytdlp::MIX_LIMIT)
                        .map_err(|err| crate::logs::error(&format!("radio import failed, using yt-dlp: {err}")))
                        .ok()
                })
            } else {
                None
            };
            let (tracks, error) = match radio.map(Ok).unwrap_or_else(|| crate::ytdlp::playlist_tracks(&url)) {
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
