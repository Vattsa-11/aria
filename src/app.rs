use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::Context;
use raw_window_handle::RawWindowHandle;
use raw_window_handle::HasWindowHandle;
use souvlaki::MediaControls;

use crate::cache::ThumbnailCache;
use crate::jobs::{Event, Job, Jobs};
use crate::media_controls::{self, MediaAction};
use crate::model::{Playlist, Track, TrackSource};
use crate::provider::ProviderMeta;
use crate::player::{AudioState, Command, PlayRequest, PlayerHandle};
use crate::queue::{Advance, Queue, TrackOrigin};
use crate::queue::TrackMenuAction;
use crate::settings::{self, RegionMode, RepeatMode, Settings};
use crate::ui::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Library,
    Search,
    Playlists,
    Liked,
    History,
    Stats,
    Downloads,
    Import,
    PlaylistDetail(usize),
    Settings,
}

pub struct FixMatchState {
    pub track: Track,
    pub alternatives: Vec<Track>,
    pub running: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct ImportState {
    pub entries: Vec<crate::import::ImportEntry>,
    pub token: u64,
    pub inflight: usize,
    pub loading_playlist: bool,
    pub source: String,
    pub error: Option<String>,
    pub url: String,

    pub target: Option<usize>,
    pub new_playlist: String,

    pub swap: Option<usize>,
}

pub enum ToastAction {
    Search,
    YtDlp,
    Download(Track),
    Resolve(Track),
}

#[derive(Default)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<Track>,
    pub running: bool,
    pub error: Option<String>,
    pub started: bool,

    pub provider: Option<crate::provider::ProviderName>,
}

pub struct App {
    pub settings: Settings,
    pub player: PlayerHandle,
    pub tracks: Vec<Track>,
    pub queue: Queue,
    pub play_stack: crate::queue::PlayStack,
    going_back: bool,
    radio_pending: bool,
    pub current: Option<Track>,
    pub current_origin: Option<TrackOrigin>,
    pub page: Page,
    pub liked: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub history: Vec<Track>,

    pub downloads: Vec<Track>,

    pub downloading: Vec<Track>,
    download_cancels: Vec<(u64, Arc<std::sync::atomic::AtomicBool>)>,

    pub delete_all: Option<(u8, usize, u64)>,
    warm: Option<Warmed>,
    pub trending: Vec<Track>,
    pub trending_loading: bool,
    pub trending_error: Option<String>,
    pub geo_loading: bool,
    warm_requested: Option<u64>,
    pub download_progress: std::collections::HashMap<u64, (u64, Option<u64>)>,

    pub online: bool,
    probe_busy: bool,
    last_probe: Instant,

    offline_skips: u8,
    pub queue_open: bool,
    pub toast: Option<(String, Instant)>,
    pub toast_action: Option<ToastAction>,
    pub toast_id: egui::Id,
    pub accent_hex: String,
    pub eq_preset_name: String,
    pub search: SearchState,
    pub import: ImportState,
    pub thumbs: ThumbnailCache,
    pub eq: Arc<crate::equalizer::EqParams>,
    pub normalize: Arc<std::sync::atomic::AtomicBool>,
    pub stats: crate::db::ListeningStats,
    pub stats_dirty: bool,
    pub sleep: crate::sleep::SleepTimer,
    pub sleep_stopping: bool,
    pub help_open: bool,
    pub lyrics_open: bool,
    pub lyrics_lines: Vec<crate::lyrics::LyricLine>,
    pub lyrics_loading: bool,
    pub lyrics_error: Option<String>,
    pub lyrics_track: Option<u64>,
    pub lyrics_active: Option<usize>,
    pub crossfade_armed: Option<u64>,
    pub crossfade_arm: bool,
    pub pending_fade: Duration,
    pub jobs: Jobs,
    pub writer: crate::writer::Writer,
    pub media: Option<MediaControls>,
    pub media_tx: Sender<MediaAction>,
    pub media_rx: Receiver<MediaAction>,
    pub media_ready: bool,
    pub focus_search: bool,
    pub ytdlp_status: Option<(String, bool)>,
    pub fix_match: Option<FixMatchState>,
    pub new_playlist_dialog: bool,
    pub new_playlist_name: String,
    pub rename_dialog: Option<(usize, String)>,
    pub rename_buffer: String,
    pub playlist_drag: crate::ui::pages::rows::DragState,
    pub queue_drag: crate::ui::pages::rows::DragState,

    pub mini: crate::miniplayer::MiniState,
    play_gen: u64,

    resolving: bool,
    pending_start_ms: Option<u64>,
    ctx: Option<Context>,
    last_persist: Instant,
    settings_dirty: bool,
    settings_saved_at: Instant,
    last_media_state: Option<bool>,
    last_media_position: u64,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ctx = cc.egui_ctx.clone();
        theme::install_fonts(&ctx);
        let mut settings = settings::load();
        theme::apply(&ctx, &settings);

        crate::provider_jiosaavn::set_base(&settings.jiosaavn_base);
        crate::ytdlp::set_cookie_browser(settings.ytdlp_cookie_browser.as_deref());
        let jobs = crate::jobs::spawn();
        let writer = crate::writer::spawn();
        let eq = crate::equalizer::EqParams::new(&settings.eq);
        let normalize = Arc::new(std::sync::atomic::AtomicBool::new(settings.normalize));
        let player = PlayerHandle::spawn(
            settings.volume,
            jobs.event_tx.clone(),
            eq.clone(),
            normalize.clone(),
        );
        let (media_tx, media_rx) = channel::<MediaAction>();

        let saved = crate::db::load_state().ok();
        let queue = saved
            .as_ref()
            .and_then(|saved| saved.queue.clone())
            .unwrap_or_else(|| Queue::new(settings.shuffle, settings.repeat));
        settings.shuffle = queue.shuffle;
        settings.repeat = queue.repeat;

        let current = saved.as_ref().and_then(|saved| saved.current.clone());
        let pending_start_ms = saved
            .as_ref()
            .filter(|saved| saved.position_ms > 1_000)
            .map(|saved| saved.position_ms);

        let tracks: Vec<Track> = settings
            .library
            .iter()
            .filter(|path| path.is_file())
            .map(|path| Track::from_path(path))
            .collect();
        let accent_hex = theme::accent_hex(&settings);

        let mut app = Self {
            settings,
            player,
            tracks,
            queue,
            play_stack: crate::queue::PlayStack::default(),
            going_back: false,
            radio_pending: false,
            current,
            current_origin: None,
            page: Page::Home,
            liked: crate::db::load_liked().unwrap_or_default(),
            playlists: crate::db::load_playlists().unwrap_or_default(),
            history: crate::db::load_history().unwrap_or_default(),
            downloads: crate::db::load_downloads().unwrap_or_default(),
            downloading: Vec::new(),
            download_cancels: Vec::new(),
            delete_all: None,
            warm: None,
            trending: Vec::new(),
            trending_loading: false,
            trending_error: None,
            geo_loading: false,
            warm_requested: None,
            download_progress: std::collections::HashMap::new(),
            online: true,
            probe_busy: false,
            last_probe: Instant::now(),
            offline_skips: 0,
            queue_open: false,
            toast: None,
            toast_action: None,
            toast_id: egui::Id::unique("aria.toast"),
            accent_hex,
            eq_preset_name: String::new(),
            search: SearchState::default(),
            import: ImportState::default(),
            thumbs: ThumbnailCache::default(),
            eq,
            normalize,
            stats: crate::db::ListeningStats::default(),
            stats_dirty: true,
            sleep: crate::sleep::SleepTimer::default(),
            sleep_stopping: false,
            help_open: false,
            lyrics_open: false,
            lyrics_lines: Vec::new(),
            lyrics_loading: false,
            lyrics_error: None,
            lyrics_track: None,
            lyrics_active: None,
            crossfade_armed: None,
            crossfade_arm: false,
            pending_fade: Duration::ZERO,
            jobs,
            writer,
            media: None,
            media_tx,
            media_rx,
            media_ready: false,
            focus_search: false,
            ytdlp_status: None,
            fix_match: None,
            new_playlist_dialog: false,
            new_playlist_name: String::new(),
            rename_dialog: None,
            rename_buffer: String::new(),
            playlist_drag: crate::ui::pages::rows::DragState::default(),
            queue_drag: crate::ui::pages::rows::DragState::default(),
            mini: crate::miniplayer::MiniState::default(),
            play_gen: 0,
            resolving: false,
            pending_start_ms,
            ctx: Some(ctx),
            last_persist: Instant::now(),
            settings_dirty: false,
            settings_saved_at: Instant::now(),
            last_media_state: None,
            last_media_position: u64::MAX,
        };
        let _ = app.jobs.tx.send(Job::EnsureYtDlp);
        app.refresh_trending();
        app
    }

    pub fn ensure_media(&mut self, frame: &eframe::Frame) {
        if self.media_ready {
            return;
        }
        self.media_ready = true;
        let hwnd = frame
            .winit_window()
            .and_then(|w| w.window_handle().ok())
            .and_then(|handle| match handle.as_raw() {
                RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as *mut c_void),
                _ => None,
            });
        self.media = media_controls::init(hwnd, self.media_tx.clone());
        self.update_media_track();
    }

    pub fn audio_state(&self) -> AudioState {
        effective_state(self.player.snapshot().state, self.resolving)
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.audio_state(), AudioState::Playing)
    }

    pub fn position(&self) -> f64 {
        self.player.snapshot().position.as_secs_f64()
    }

    pub fn duration(&self) -> Option<f64> {
        self.player
            .snapshot()
            .duration
            .or_else(|| self.current.as_ref().and_then(|track| track.duration))
            .map(|duration| duration.as_secs_f64())
    }

    pub fn notify(&mut self, message: impl Into<String>) {
        self.toast = Some((message.into(), Instant::now()));
        self.toast_action = None;
    }

    pub fn fail(&mut self, message: impl Into<String>, action: Option<ToastAction>) {
        let message = message.into();
        crate::logs::error(&message);
        self.toast = Some((message, Instant::now()));
        self.toast_action = action;
    }

    pub fn run_toast_action(&mut self) {
        let Some(action) = self.toast_action.take() else {
            return;
        };
        self.toast = None;
        match action {
            ToastAction::Search => self.run_search(),
            ToastAction::YtDlp => {
                let _ = self.jobs.tx.send(Job::DownloadYtDlp);
                self.notify("Downloading yt-dlp...");
            }
            ToastAction::Download(track) => self.start_download(track),
            ToastAction::Resolve(track) => {
                let origin = self.current_origin.unwrap_or(TrackOrigin::Context);
                self.start_track(track, origin, Duration::ZERO);
            }
        }
    }

    pub fn current_quality_badge(&self) -> Option<String> {
        self.current.as_ref().and_then(|t| t.quality_badge())
    }

    pub fn start_fix_match(&mut self, track: Track) {
        let query = track.clone();
        self.fix_match = Some(FixMatchState {
            track,
            alternatives: Vec::new(),
            running: true,
            error: None,
        });
        let _ = self.jobs.tx.send(Job::FixMatch { track: query });
    }

    pub fn apply_fix_match(&mut self, index: usize) {
        let Some(state) = self.fix_match.take() else {
            return;
        };
        let Some(new_track) = state.alternatives.get(index).cloned() else {
            return;
        };
        let old_id = state.track.id;
        for track in self.queue.user.iter_mut() {
            if track.id == old_id {
                *track = new_track.clone();
            }
        }
        for track in self.queue.context.iter_mut() {
            if track.id == old_id {
                *track = new_track.clone();
            }
        }
        if self.current.as_ref().map(|t| t.id) == Some(old_id) {
            self.current = Some(new_track.clone());
        }
        self.persist(true);
        self.notify("Match fixed");
    }

    pub fn start_track(&mut self, track: Track, origin: TrackOrigin, start: Duration) {
        let fade = if std::mem::take(&mut self.crossfade_arm) {
            Duration::from_secs_f32(self.settings.crossfade.clamp(0.0, 12.0))
        } else {
            Duration::ZERO
        };
        self.pending_fade = fade;
        self.crossfade_armed = Some(track.id);

        if self.is_offline() && !self.available_offline(&track) {
            self.offline_skips = self.offline_skips.saturating_add(1);
            if self.offline_skips > 50 {
                self.offline_skips = 0;
                self.notify("Nothing in this list is available offline");
                return;
            }
            self.play_advance(true);
            return;
        }
        self.offline_skips = 0;
        self.remember_previous(track.id);
        self.current = Some(track.clone());
        self.current_origin = Some(origin);
        self.play_gen += 1;
        self.record_history(&track);
        let duration = track.duration;
        let offline = self.download_path(track.id);
        match (track.is_online(), offline) {

            (true, Some(path)) => {
                let local = crate::downloads::playback_track(&track, Some(path));
                self.player.send(Command::Play(Box::new(PlayRequest {
                    track: local,
                    resolved: None,
                    start,
                    duration,
                    fade,
                    warm: None,
                })));
            }
            (false, _) => {
                self.player.send(Command::Play(Box::new(PlayRequest {
                    track,
                    resolved: None,
                    start,
                    duration,
                    fade,
                    warm: None,
                })));
            }
            (true, None) => {
                if let Some(warmed) = self.take_warm(track.id) {
                    let mut track = track;
                    track.provider_meta = Some(ProviderMeta {
                        provider: warmed.stream.provider,
                        bitrate_kbps: warmed.stream.bitrate_kbps,
                    });
                    let duration = warmed.stream.duration.or(duration);
                    self.player.send(Command::Play(Box::new(PlayRequest {
                        track,
                        resolved: Some(warmed.stream),
                        start,
                        duration,
                        fade,
                        warm: Some(warmed.http),
                    })));
                } else {
                    let gen = self.play_gen;
                    self.resolving = true;
                    let _ = self.jobs.tx.send(Job::Resolve { gen, track });
                }
            }
        }
        self.update_media_track();
        self.persist(true);
    }

    fn radio_seed(&self) -> Option<String> {
        let track = self.current.clone()?;
        let artist = track.artist.trim().to_string();
        if !artist.is_empty() {
            return Some(artist);
        }
        let title = track.title.trim().to_string();
        if title.is_empty() {
            None
        } else {
            Some(title)
        }
    }

    fn remember_previous(&mut self, next_id: u64) {
        if self.going_back {
            return;
        }
        let Some(previous) = self.current.clone() else {
            return;
        };
        if previous.id == next_id {
            return;
        }
        let origin = self.current_origin.unwrap_or(TrackOrigin::Context);
        self.play_stack.record(previous, origin);
    }

    pub fn play_advance(&mut self, automatic: bool) {
        match self.queue.advance(automatic) {
            Advance::RepeatCurrent => {
                if let Some(track) = self.current.clone() {
                    let origin = self.current_origin.unwrap_or(TrackOrigin::Context);
                    self.start_track(track, origin, Duration::ZERO);
                } else {
                    self.play_advance(false);
                }
            }
            Advance::PlayUser(track) => self.start_track(track, TrackOrigin::User, Duration::ZERO),
            Advance::PlayContext { index, track } => {
                self.queue.mark_context_playing(index);
                self.start_track(track, TrackOrigin::Context, Duration::ZERO);
            }
            Advance::End => {
                self.player.send(Command::Stop);
                if self.settings.autoplay && !self.is_offline() {
                    if let Some(seed) = self.radio_seed() {
                        let _ = self.jobs.tx.send(Job::Radio { seed });
                        self.radio_pending = true;
                        self.notify("Queue finished - starting a radio mix");
                        self.persist(true);
                        return;
                    }
                }
                self.persist(true);
            }
        }
    }

    pub fn toggle_play(&mut self) {
        match self.audio_state() {
            AudioState::Playing => self.player.send(Command::Pause),
            AudioState::Paused => self.player.send(Command::Resume),
            AudioState::Loading => {}
            AudioState::Idle => {
                if let Some(track) = self.current.clone() {
                    let origin = self.current_origin.unwrap_or(TrackOrigin::Context);
                    let start = self
                        .pending_start_ms
                        .take()
                        .map(Duration::from_millis)
                        .unwrap_or(Duration::ZERO);
                    self.start_track(track, origin, start);
                } else {
                    self.play_advance(false);
                }
            }
        }
    }

    pub fn previous(&mut self) {
        if self.position() > 3.0 {
            self.seek(0.0);
            return;
        }
        if let Some((track, origin)) = self.play_stack.back() {
            self.going_back = true;
            self.start_track(track, origin, Duration::ZERO);
            self.going_back = false;
            return;
        }
        if self.current_origin != Some(TrackOrigin::Context) {
            self.seek(0.0);
            return;
        }
        let Some(current) = self.queue.context_pos else {
            self.seek(0.0);
            return;
        };
        let previous = match self
            .queue
            .order
            .iter()
            .position(|index| *index == current)
        {
            Some(0) => {
                if self.queue.repeat == RepeatMode::All {
                    self.queue.order.last().copied()
                } else {
                    None
                }
            }
            Some(position) => Some(self.queue.order[position - 1]),
            None => None,
        };
        match previous.and_then(|index| self.queue.jump_context(index)) {
            Some(track) => self.start_track(track, TrackOrigin::Context, Duration::ZERO),
            None => self.seek(0.0),
        }
    }

    pub fn play_library(&mut self, index: usize) {
        let Some(track) = self.tracks.get(index).cloned() else {
            return;
        };
        let library_is_context =
            self.queue.context_name == "Library" && self.queue.context.len() == self.tracks.len();
        if !library_is_context {
            self.queue.set_context(self.tracks.clone(), "Library");
        }
        self.queue.mark_context_playing(index);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn play_search_result(&mut self, index: usize) {
        let Some(track) = self.search.results.get(index).cloned() else {
            return;
        };
        let name = if self.search.query.trim().is_empty() {
            "Search results".to_string()
        } else {
            format!("Search: {}", self.search.query.trim())
        };
        self.queue.set_context(self.search.results.clone(), name);
        self.queue.mark_context_playing(index);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn add_to_queue(&mut self, track: Track) {
        self.queue.push_user(track);
        self.notify("Added to queue");
        self.persist(true);
    }

    pub fn play_single(&mut self, track: Track) {
        self.queue.set_context(vec![track.clone()], "Now playing");
        self.queue.mark_context_playing(0);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn play_next(&mut self, track: Track) {
        self.queue.push_front_user(track);
        self.notify("Playing next");
        self.persist(true);
    }

    pub fn jump_queue_user(&mut self, index: usize) {
        if let Some(track) = self.queue.take_user(index) {
            self.start_track(track, TrackOrigin::User, Duration::ZERO);
        }
    }

    pub fn jump_queue_context(&mut self, index: usize) {
        if let Some(track) = self.queue.jump_context(index) {
            self.start_track(track, TrackOrigin::Context, Duration::ZERO);
        }
    }

    pub fn toggle_shuffle(&mut self) {
        self.queue.toggle_shuffle();
        self.settings.shuffle = self.queue.shuffle;
        self.persist(true);
        self.notify(if self.queue.shuffle {
            "Shuffle on"
        } else {
            "Shuffle off"
        });
    }

    pub fn cycle_repeat(&mut self) {
        self.queue.cycle_repeat();
        self.settings.repeat = self.queue.repeat;
        self.persist(true);
        self.notify(match self.queue.repeat {
            RepeatMode::Off => "Loop off",
            RepeatMode::All => "Loop all",
            RepeatMode::One => "Loop one",
        });
    }

    pub fn seek(&mut self, seconds: f64) {
        self.player.send(Command::Seek(Duration::from_secs_f64(
            seconds.max(0.0),
        )));
    }

    pub fn save_eq_preset(&mut self) {
        let name = self.eq_preset_name.clone();
        if !self.settings.eq.save_custom(&name) {
            self.notify("Name the preset first");
            return;
        }
        self.eq_preset_name.clear();
        self.apply_eq();
        self.notify(format!("Saved preset {name}"));
    }

    pub fn apply_eq_custom(&mut self, name: &str) {
        if self.settings.eq.apply_custom(name) {
            self.apply_eq();
        }
    }

    pub fn delete_eq_preset(&mut self, name: &str) {
        self.settings.eq.delete_custom(name);
        self.apply_eq();
    }

    pub fn apply_eq(&mut self) {
        self.eq.apply(&self.settings.eq);
        self.touch_settings();
    }

    pub fn set_normalize(&mut self, enabled: bool) {
        self.normalize
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.settings.volume = volume.clamp(0.0, 1.0);
        self.player.send(Command::SetVolume(self.settings.volume));
        self.touch_settings();
    }

    pub fn toggle_like(&mut self, index: usize) {
        let Some(track) = self.tracks.get(index).cloned() else {
            return;
        };
        self.toggle_like_track(&track);
    }

    pub fn is_liked(&self, id: u64) -> bool {
        self.liked.iter().any(|t| t.id == id)
    }

    pub fn play_liked(&mut self, index: usize) {
        let Some(track) = self.liked.get(index).cloned() else {
            return;
        };
        self.queue.set_context(self.liked.clone(), "Liked Songs");
        self.queue.mark_context_playing(index);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn play_downloads(&mut self, index: usize) {
        let Some(track) = self.downloads.get(index).cloned() else {
            return;
        };
        self.queue.set_context(self.downloads.clone(), "Downloads");
        self.queue.mark_context_playing(index);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn play_history(&mut self, index: usize) {
        let Some(track) = self.history.get(index).cloned() else {
            return;
        };
        self.queue.set_context(self.history.clone(), "History");
        self.queue.mark_context_playing(index);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn play_playlist(&mut self, playlist_index: usize, track_index: usize) {
        let Some(playlist) = self.playlists.get(playlist_index) else {
            return;
        };
        let Some(track) = playlist.tracks.get(track_index).cloned() else {
            return;
        };
        let name = playlist.name.clone();
        self.queue.set_context(playlist.tracks.clone(), name);
        self.queue.mark_context_playing(track_index);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn import_file(&mut self) {
        let file = crate::dialogs::pick_file("Track lists", &["csv", "txt", "m3u", "m3u8"]);
        let Some(path) = file else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                self.fail(format!("Cannot read that file: {err}"), None);
                return;
            }
        };
        let is_csv = path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("csv"))
            .unwrap_or(false);
        let mut entries = if is_csv {
            crate::import::parse_csv(&text)
        } else {
            Vec::new()
        };
        if entries.is_empty() {
            entries = crate::import::parse_text_list(&text);
        }
        if entries.is_empty() {
            self.fail("No tracks found in that file", None);
            return;
        }
        let source = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        self.begin_import(entries, source);
    }

    pub fn import_url(&mut self) {
        let url = self.import.url.trim().to_string();
        if url.is_empty() {
            self.notify("Paste a playlist URL first");
            return;
        }
        self.import.token += 1;
        self.import.inflight = 0;
        self.import.loading_playlist = true;
        self.import.error = None;
        let _ = self.jobs.tx.send(Job::ImportPlaylist {
            token: self.import.token,
            url,
        });
    }

    pub fn begin_import(&mut self, titles: Vec<crate::import::ParsedEntry>, source: String) {
        self.import.token += 1;
        self.import.entries = crate::import::entries_from(titles);
        self.import.source = source;
        self.import.error = None;
        self.import.inflight = 0;
        self.import.swap = None;
        self.import.new_playlist = if self.playlists.is_empty() {
            "Imported playlist".to_string()
        } else {
            String::new()
        };
        self.import.target = None;
        self.page = Page::Import;
        self.pump_import();
    }

    pub fn pump_import(&mut self) {
        if self.import.inflight > 0 || self.import.entries.is_empty() {
            return;
        }
        let Some(index) = self
            .import
            .entries
            .iter()
            .position(|entry| entry.state == crate::import::MatchState::Pending)
        else {
            return;
        };
        let query = self.import.entries[index].query();
        let duration = self.import.entries[index].duration;
        self.import.entries[index].state = crate::import::MatchState::Searching;
        self.import.inflight = 1;
        let _ = self.jobs.tx.send(Job::ImportMatch {
            token: self.import.token,
            index,
            query,
            duration,
        });
    }

    pub fn skip_import(&mut self, index: usize) {
        if let Some(entry) = self.import.entries.get_mut(index) {
            entry.skipped = !entry.skipped;
        }
    }

    pub fn retry_import(&mut self, index: usize) {
        if let Some(entry) = self.import.entries.get_mut(index) {
            entry.state = crate::import::MatchState::Pending;
            entry.matched = None;
            entry.alternatives.clear();
        }
        self.pump_import();
    }

    pub fn import_candidates(&self, index: usize) -> Vec<Track> {
        let Some(entry) = self.import.entries.get(index) else {
            return Vec::new();
        };
        let mut tracks = Vec::new();
        if let Some(track) = &entry.matched {
            tracks.push(track.clone());
        }
        for track in &entry.alternatives {
            if !tracks.iter().any(|existing| existing.id == track.id) {
                tracks.push(track.clone());
            }
        }
        tracks
    }

    pub fn choose_import_match(&mut self, index: usize, candidate: usize) {
        let chosen = self.import_candidates(index).into_iter().nth(candidate);
        let Some(track) = chosen else {
            return;
        };
        if let Some(entry) = self.import.entries.get_mut(index) {
            let mut rest: Vec<Track> = Vec::new();
            if let Some(old) = entry.matched.clone() {
                rest.push(old);
            }
            for other in &entry.alternatives {
                if other.id != track.id {
                    rest.push(other.clone());
                }
            }
            entry.matched = Some(track);
            entry.alternatives = rest;
            entry.state = crate::import::MatchState::Matched;
            entry.skipped = false;
        }
        self.import.swap = None;
    }

    pub fn clear_import(&mut self) {
        self.import.entries.clear();
        self.import.source.clear();
        self.import.error = None;
        self.import.swap = None;
        self.import.inflight = 0;
        self.import.token += 1;
    }

    pub fn apply_import(&mut self) {
        let tracks: Vec<Track> = self
            .import
            .entries
            .iter()
            .filter(|entry| !entry.skipped)
            .filter_map(|entry| entry.matched.clone())
            .collect();
        if tracks.is_empty() {
            self.notify("Nothing matched to import yet");
            return;
        }
        let target = match self.import.target {
            Some(index) if index < self.playlists.len() => index,
            _ => {
                let name = self.import.new_playlist.trim().to_string();
                if name.is_empty() {
                    self.notify("Name the new playlist");
                    return;
                }
                if self
                    .playlists
                    .iter()
                    .any(|playlist| playlist.name.eq_ignore_ascii_case(&name))
                {
                    self.notify("That playlist already exists — pick it on the left");
                    return;
                }
                self.playlists.push(Playlist::new(&name));
                self.playlists.len() - 1
            }
        };
        let mut added = 0usize;
        let mut duplicates = 0usize;
        for track in tracks {
            let Some(playlist) = self.playlists.get_mut(target) else {
                break;
            };
            if playlist.tracks.iter().any(|existing| existing.id == track.id) {
                duplicates += 1;
                continue;
            }
            playlist.tracks.push(track);
            added += 1;
        }
        self.persist_playlists();
        self.import.entries.clear();
        self.import.source.clear();
        self.import.swap = None;
        self.page = Page::PlaylistDetail(target);
        self.notify(if duplicates == 0 {
            format!("Imported {added} tracks")
        } else {
            format!("Imported {added} tracks, skipped {duplicates} already in the playlist")
        });
    }

    pub fn save_mix_as_playlist(&mut self, tracks: Vec<Track>) {
        let Some(first) = tracks.first() else {
            return;
        };
        let names: Vec<String> = self.playlists.iter().map(|p| p.name.clone()).collect();
        let name = crate::ytdlp::mix_playlist_name(&names, &first.title);
        let count = tracks.len();
        let mut playlist = Playlist::new(&name);
        playlist.tracks = tracks;
        self.playlists.push(playlist);
        self.persist_playlists();
        self.page = Page::PlaylistDetail(self.playlists.len() - 1);
        self.notify(format!("Saved {count} songs from the Mix as the playlist {name}"));
    }

    pub fn create_playlist(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.notify("Enter a playlist name");
            return;
        }
        if self.playlists.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
            self.notify("A playlist with that name already exists");
            return;
        }
        self.playlists.push(Playlist::new(name));
        self.persist_playlists();
        self.notify(format!("Created playlist {name}"));
    }

    pub fn rename_playlist(&mut self, index: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.notify("Enter a playlist name");
            return;
        }
        if let Some(playlist) = self.playlists.get_mut(index) {
            playlist.name = name.to_string();
            self.persist_playlists();
            self.notify("Playlist renamed");
        }
    }

    pub fn delete_playlist(&mut self, index: usize) {
        if index < self.playlists.len() {
            let name = self.playlists.remove(index).name;
            self.persist_playlists();
            self.notify(format!("Deleted {name}"));
            if self.page == Page::PlaylistDetail(index) {
                self.page = Page::Playlists;
            }
        }
    }

    pub fn add_to_playlist(&mut self, playlist_index: usize, track: Track) {
        let name = match self.playlists.get_mut(playlist_index) {
            Some(playlist) => {
                playlist.tracks.push(track);
                playlist.name.clone()
            }
            None => return,
        };
        self.persist_playlists();
        self.notify(format!("Added to {name}"));
    }

    pub fn remove_from_playlist(&mut self, playlist_index: usize, track_index: usize) {
        let removed = match self.playlists.get_mut(playlist_index) {
            Some(playlist) if track_index < playlist.tracks.len() => {
                playlist.tracks.remove(track_index);
                true
            }
            _ => false,
        };
        if removed {
            self.persist_playlists();
        }
    }

    pub fn move_playlist_track(&mut self, playlist_index: usize, from: usize, to: usize) {
        if from == to {
            return;
        }
        let moved = match self.playlists.get_mut(playlist_index) {
            Some(playlist)
                if from < playlist.tracks.len() && to < playlist.tracks.len() =>
            {
                let track = playlist.tracks.remove(from);
                playlist.tracks.insert(to, track);
                true
            }
            _ => false,
        };
        if moved {
            self.persist_playlists();
        }
    }

    fn persist_playlists(&mut self) {
        self.writer
            .send(crate::writer::Write::Playlists(self.playlists.clone()));
    }

    fn persist_liked(&mut self) {
        self.writer
            .send(crate::writer::Write::Liked(self.liked.clone()));
    }

    fn persist_history(&mut self) {
        self.writer
            .send(crate::writer::Write::History(self.history.clone()));
    }

    fn record_history(&mut self, track: &Track) {
        self.history.retain(|t| t.id != track.id);
        self.history.insert(0, track.clone());
        if self.history.len() > 200 {
            self.history.truncate(200);
        }
        crate::db::log_play(track);
        self.stats_dirty = true;
        self.persist_history();
    }

    pub fn track_menu_for(&mut self, track: &Track, action: TrackMenuAction) {
        match action {
            TrackMenuAction::Play => self.play_single(track.clone()),
            TrackMenuAction::PlayNext => self.play_next(track.clone()),
            TrackMenuAction::AddToQueue => self.add_to_queue(track.clone()),
            TrackMenuAction::Like => self.toggle_like_track(track),
            TrackMenuAction::FixMatch => self.start_fix_match(track.clone()),
            TrackMenuAction::Download => self.start_download(track.clone()),
            TrackMenuAction::CancelDownload => self.cancel_download(track.id),
            TrackMenuAction::DeleteDownload => self.remove_download(track.id),
            TrackMenuAction::AddToPlaylist(idx) => {
                self.add_to_playlist(idx, track.clone())
            }
        }
    }

    pub fn is_downloaded(&self, track_id: u64) -> bool {
        self.downloads.iter().any(|track| track.id == track_id)
    }

    pub fn is_downloading(&self, track_id: u64) -> bool {
        self.downloading.iter().any(|track| track.id == track_id)
    }

    pub fn download_path(&self, track_id: u64) -> Option<PathBuf> {
        if self.is_downloaded(track_id) {
            if let Some(path) = crate::downloads::existing(track_id) {
                return Some(path);
            }
        }
        None
    }

    pub fn start_download(&mut self, track: Track) {
        if !track.is_online() {
            self.notify("Already available offline");
            return;
        }
        if self.is_downloaded(track.id) {
            self.notify("Already downloaded");
            return;
        }
        if self.is_downloading(track.id) {
            return;
        }
        self.queue_download(track.clone());
        self.notify(format!("Downloading {}", track.title));
    }

    fn queue_download(&mut self, track: Track) {
        if self.settings.battery_saver {
            self.notify("Battery saver is on: downloads are paused");
            return;
        }
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.download_cancels.push((track.id, cancel.clone()));
        self.downloading.push(track.clone());
        let _ = self.jobs.tx.send(Job::Download { track, cancel });
    }

    pub fn cancel_download(&mut self, track_id: u64) {
        if let Some((_, flag)) = self
            .download_cancels
            .iter()
            .find(|(id, _)| *id == track_id)
        {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.download_cancels.retain(|(id, _)| *id != track_id);
        self.downloading.retain(|track| track.id != track_id);
        self.download_progress.remove(&track_id);
        self.notify("Download cancelled");
    }

    pub fn download_playlist(&mut self, playlist_index: usize) {
        let Some(playlist) = self.playlists.get(playlist_index) else {
            return;
        };
        let tracks = playlist.tracks.clone();
        let mut started = 0usize;
        for track in tracks {
            if !track.is_online() || self.is_downloaded(track.id) || self.is_downloading(track.id) {
                continue;
            }
            self.queue_download(track);
            started += 1;
        }
        if started == 0 {
            self.notify("Everything in this playlist is already offline");
        } else {
            self.notify(format!(
                "Downloading {started} track{}",
                if started == 1 { "" } else { "s" }
            ));
        }
    }

    pub fn remove_download(&mut self, track_id: u64) {
        crate::downloads::remove(track_id);
        let _ = crate::db::delete_download(track_id);
        self.downloads.retain(|track| track.id != track_id);
        self.notify("Download removed");
    }

    pub fn clear_downloads(&mut self) {
        self.delete_all = None;
        let files = crate::downloads::clear_all();
        let _ = crate::db::clear_downloads();
        self.downloads.clear();
        self.notify(if files == 1 {
            "Removed 1 downloaded file".to_string()
        } else {
            format!("Removed {files} downloaded files")
        });
        self.persist(true);
    }

    pub fn is_offline(&self) -> bool {
        self.settings.offline_mode || !self.online
    }

    pub fn available_offline(&self, track: &Track) -> bool {
        !track.is_online() || self.is_downloaded(track.id)
    }

    pub fn row_state(&self, track: &Track) -> (bool, Option<&'static str>) {
        let dim = self.is_offline() && !self.available_offline(track);
        let badge = if self.is_downloaded(track.id) {
            Some("Offline")
        } else {
            None
        };
        (dim, badge)
    }

    pub fn set_offline_mode(&mut self, on: bool) {
        self.settings.offline_mode = on;
        self.touch_settings();
        self.notify(if on {
            "Offline mode on — only downloaded and local tracks"
        } else if self.online {
            "Offline mode off"
        } else {
            "Offline mode off — still no connection"
        });
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }

    fn probe(&mut self) {
        if self.probe_busy {
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
            return;
        }
        if self.last_probe.elapsed() < Duration::from_secs(10) {
            return;
        }
        self.last_probe = Instant::now();
        self.probe_busy = true;
        let _ = self.jobs.tx.send(Job::Probe);
    }

    pub fn liked_current(&self) -> bool {
        self.current.as_ref().map(|t| self.is_liked(t.id)).unwrap_or(false)
    }

    pub fn toggle_like_track(&mut self, track: &Track) {
        if self.is_liked(track.id) {
            self.liked.retain(|t| t.id != track.id);
            self.notify("Removed from Liked Songs");
        } else {
            self.liked.insert(0, track.clone());
            self.notify("Added to Liked Songs");
        }
        self.persist_liked();
    }

    pub fn toggle_like_current(&mut self) {
        if let Some(track) = self.current.clone() {
            self.toggle_like_track(&track);
        }
    }

    pub fn scan_folder(&mut self) {
        let Some(folder) = crate::dialogs::pick_folder() else {
            return;
        };
        let paths = collect_audio_files(&folder, 8);
        if paths.is_empty() {
            self.notify("No audio files in that folder");
            return;
        }
        let folders = self.settings.library_folders.clone();
        if !folders.iter().any(|saved| saved == &folder) {
            self.settings.library_folders.push(folder.clone());
            self.touch_settings();
        }
        self.add_paths(paths);
    }

    pub fn rescan_folders(&mut self) {
        let folders = self.settings.library_folders.clone();
        if folders.is_empty() {
            self.notify("No folders saved yet");
            return;
        }
        let mut paths = Vec::new();
        for folder in &folders {
            paths.extend(collect_audio_files(folder, 8));
        }
        if paths.is_empty() {
            self.notify("No audio files found in your folders");
            return;
        }
        self.add_paths(paths);
    }

    pub fn open_file_picker(&mut self) {
        self.pick_files();
    }

    pub fn pick_files(&mut self) {
        let files = crate::dialogs::pick_files(
            "Audio",
            &["mp3", "flac", "ogg", "oga", "m4a", "aac", "wav", "opus", "mp4"],
        );
        if let Some(files) = files {
            self.add_paths(files);
        }
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0usize;
        for path in paths {
            if !path.is_file() {
                continue;
            }
            if self.tracks.iter().any(|track| track.source == TrackSource::Local(path.clone())) {
                continue;
            }
            self.tracks.push(Track::from_path(&path));
            added += 1;
        }
        if added == 0 {
            self.notify("No new audio files found");
            return;
        }
        if self.queue.context_name == "Library" {
            self.queue.set_context(self.tracks.clone(), "Library");
        }
        self.persist_library();
        self.notify(format!(
            "Added {} track{}",
            added,
            if added == 1 { "" } else { "s" }
        ));
    }

    fn persist_library(&mut self) {
        self.settings.library = self
            .tracks
            .iter()
            .filter_map(|track| track.local_path().map(PathBuf::from))
            .collect();
        settings::save(&self.settings);
    }

    pub fn run_search(&mut self) {
        let query = self.search.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        if self.is_offline() {
            self.search.running = false;
            self.search.started = true;
            self.search.results.clear();
            self.search.error = Some(
                "You're offline. Downloaded and local tracks are still playable from Downloads."
                    .to_string(),
            );
            return;
        }
        self.search.running = true;
        self.search.started = true;
        self.search.error = None;
        let provider = self.search.provider;
        let _ = self.jobs.tx.send(Job::Search {
            query,
            limit: 20,
            provider,
        });
    }

    pub fn set_search_source(&mut self, provider: Option<crate::provider::ProviderName>) {
        if self.search.provider == provider {
            return;
        }
        self.search.provider = provider;
        self.search.results.clear();
        self.search.error = None;
        self.search.started = false;
        if !self.search.query.trim().is_empty() {
            self.run_search();
        }
    }

    pub fn busy(&self) -> bool {
        self.resolving || self.search.running || self.radio_pending || !self.downloading.is_empty()
    }

    pub fn downloading_summary(&self) -> String {
        format!("Downloading {}", self.downloading.len())
    }

    pub fn thumb(&mut self, track: &Track) -> Option<egui::TextureHandle> {
        if let Some(handle) = self.thumbs.get(track.id) {
            return Some(handle);
        }
        if self.thumbs.is_pending(track.id) || self.thumbs.recently_failed(track.id) {
            return None;
        }
        let (source, letterboxed) = track.art_source()?;
        self.thumbs.mark_pending(track.id);
        self.jobs.thumbs.push(crate::cache::ThumbJob {
            track_id: track.id,
            source,
            letterboxed,
        });
        None
    }

    pub fn current_art(&mut self) -> Option<egui::TextureHandle> {
        let track = self.current.clone()?;
        self.thumb(&track)
    }

    fn take_warm(&mut self, track_id: u64) -> Option<Warmed> {
        let warmed = self.warm.take()?;
        if warm_is_usable(warmed.track_id, track_id, warmed.at.elapsed()) {
            Some(warmed)
        } else {
            None
        }
    }

    fn next_preview(&self) -> Option<Track> {
        self.queue
            .user
            .front()
            .cloned()
            .or_else(|| self.queue.context_preview().into_iter().next().map(|(_, track)| track.clone()))
    }

    fn maybe_warm_next(&mut self) {
        if self.audio_state() != AudioState::Playing || self.is_offline() {
            return;
        }
        let Some(next) = self.next_preview() else {
            return;
        };
        let already = self.warm.as_ref().is_some_and(|warmed| warmed.track_id == next.id)
            || self.warm_requested == Some(next.id);
        let local = !next.is_online() || self.download_path(next.id).is_some();
        let is_current = self.current.as_ref().is_some_and(|track| track.id == next.id);
        if !should_warm(self.position(), local, already, is_current) {
            return;
        }
        self.warm_requested = Some(next.id);
        let _ = self.jobs.tx.send(Job::Warm { track: next });
    }

    pub fn prefetch_playback_art(&mut self) {
        let mut upcoming: Vec<Track> = self.current.iter().cloned().collect();
        upcoming.extend(self.queue.user.iter().take(6).cloned());
        upcoming.extend(self.queue.context_preview().into_iter().take(6).map(|(_, track)| track.clone()));
        self.prefetch_thumbs(&upcoming);
    }

    pub fn prefetch_thumbs(&mut self, tracks: &[Track]) {
        for track in tracks {
            let _ = self.thumb(track);
        }
    }

    pub fn refresh_trending(&mut self) {
        let wants = matches!(self.settings.region_mode, RegionMode::Auto | RegionMode::Manual);
        let language = self.settings.region_language.clone().filter(|language| crate::region::is_known_language(language));
        match (wants, language) {
            (true, Some(language)) => {
                self.trending_loading = true;
                self.trending_error = None;
                let _ = self.jobs.tx.send(Job::Trending { language });
            }
            _ => {
                self.trending.clear();
                self.trending_loading = false;
            }
        }
    }

    pub fn set_region_mode(&mut self, mode: RegionMode) {
        self.settings.region_mode = mode;
        self.trending_error = None;
        match mode {
            RegionMode::Auto => {
                self.geo_loading = true;
                let _ = self.jobs.tx.send(Job::Region);
            }
            RegionMode::Off => {
                self.trending.clear();
                self.trending_loading = false;
                self.geo_loading = false;
            }
            RegionMode::Manual | RegionMode::Ask => self.refresh_trending(),
        }
        self.touch_settings();
    }

    pub fn choose_region_language(&mut self, language: &str) {
        if !crate::region::is_known_language(language) {
            return;
        }
        self.settings.region_mode = RegionMode::Manual;
        self.settings.region_language = Some(language.to_string());
        self.settings.region_label = None;
        self.geo_loading = false;
        self.touch_settings();
        self.refresh_trending();
    }

    pub fn play_trending(&mut self, index: usize) {
        let Some(track) = self.trending.get(index).cloned() else {
            return;
        };
        let name = crate::region::trending_title(
            self.settings.region_label.as_deref(),
            self.settings.region_language.as_deref().unwrap_or(""),
        );
        self.queue.set_context(self.trending.clone(), name);
        self.queue.mark_context_playing(index);
        self.start_track(track, TrackOrigin::Context, Duration::ZERO);
    }

    pub fn toggle_mini(&mut self) {
        self.mini.toggle();
        self.touch_settings();
    }

    pub fn flush_mini_geometry(&mut self, ctx: &egui::Context) {
        self.touch_settings();
        let interval = Duration::from_millis(400);
        let elapsed = self.settings_saved_at.elapsed();
        if elapsed < interval {
            ctx.request_repaint_after(interval - elapsed);
            return;
        }
        self.save_settings_now();
    }

    pub fn close_mini(&mut self, ctx: &egui::Context) {
        self.mini.close();
        self.touch_settings();
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }

    pub fn persist(&mut self, immediate: bool) {
        if !immediate && self.last_persist.elapsed() < Duration::from_secs(15) {
            return;
        }
        self.last_persist = Instant::now();
        let position_ms = self.player.snapshot().position.as_millis() as u64;
        self.writer
            .send(crate::writer::Write::State(Box::new(crate::writer::StateWrite {
                queue: self.queue.clone(),
                current: self.current.clone(),
                position_ms,
            })));
    }

    pub fn touch_settings(&mut self) {
        self.settings_dirty = true;
    }

    fn flush_settings(&mut self) {
        if !self.settings_dirty || self.settings_saved_at.elapsed() < Duration::from_millis(600) {
            return;
        }
        self.settings_dirty = false;
        self.settings_saved_at = Instant::now();
        settings::save(&self.settings);
    }

    pub fn save_settings_now(&mut self) {
        self.settings_dirty = false;
        self.settings_saved_at = Instant::now();
        settings::save(&self.settings);
    }

    pub fn apply_hex_accent(&mut self) {
        match theme::parse_hex(&self.accent_hex) {
            Some(rgb) => {
                self.settings.accent = rgb;
                self.accent_hex = theme::accent_hex(&self.settings);
                self.reload_theme();
                self.touch_settings();
                self.notify("Accent color updated");
            }
            None => self.notify("Use a hex color like #4F8CFF"),
        }
    }

    pub fn reload_theme(&mut self) {
        if let Some(ctx) = &self.ctx {
            theme::apply(ctx, &self.settings);
        }
    }

    pub fn update_sleep_timer(&mut self) {
        if !self.sleep.active() {
            return;
        }
        if !self.sleep.fading() && self.sleep.deadline_reached() {
            self.sleep.begin_fade(self.settings.volume);
        }
        if self.sleep.fading() {
            let target = self.sleep.fade_volume(self.settings.volume);
            if target > 0.001 {
                self.player.send(Command::SetVolume(target));
            }
            if self.sleep.fade_finished() {
                self.player.send(Command::Pause);
                let restored = self.sleep.take_saved_volume();
                self.player.send(Command::SetVolume(restored));
                self.sleep_stopping = false;
                self.notify("Sleep timer stopped playback");
            }
        }
    }

    pub fn arm_sleep_minutes(&mut self, minutes: u32) {
        self.sleep.arm_minutes(minutes);
        self.notify(format!("Sleep timer set for {minutes} minutes"));
    }

    pub fn arm_sleep_end_of_track(&mut self) {
        self.sleep.arm_end_of_track();
        self.notify("Sleep timer set for the end of this track");
    }

    pub fn cancel_sleep(&mut self) {
        self.sleep.clear();
        self.notify("Sleep timer cancelled");
    }

    pub fn toggle_lyrics(&mut self) {
        self.lyrics_open = !self.lyrics_open;
        if self.lyrics_open {
            self.lyrics_track = None;
            self.refresh_lyrics();
        }
    }

    pub fn refresh_lyrics(&mut self) {
        let Some(track) = self.current.clone() else {
            self.lyrics_lines.clear();
            self.lyrics_error = Some("Nothing playing".to_string());
            return;
        };
        if self.lyrics_track == Some(track.id) && !self.lyrics_lines.is_empty() {
            return;
        }
        self.lyrics_track = Some(track.id);
        self.lyrics_lines.clear();
        self.lyrics_active = None;
        self.lyrics_error = None;
        self.lyrics_loading = true;
        let _ = self.jobs.tx.send(Job::Lyrics { track });
    }

    pub fn maybe_crossfade(&mut self) {
        let secs = self.settings.crossfade;
        if secs <= 0.05 {
            return;
        }
        let Some(track) = self.current.clone() else {
            return;
        };
        if self.crossfade_armed == Some(track.id) {
            return;
        }
        if self.audio_state() != AudioState::Playing {
            return;
        }
        let Some(duration) = self.duration() else {
            return;
        };
        let remaining = duration - self.position();
        if remaining > secs as f64 || remaining < -0.5 {
            return;
        }
        if !self.queue.has_following(true) {
            return;
        }
        self.crossfade_armed = Some(track.id);
        self.crossfade_arm = true;
        self.play_advance(true);
    }

    pub fn tick(&mut self) {
        self.update_sleep_timer();
        self.maybe_crossfade();
        self.prefetch_playback_art();
        self.maybe_warm_next();
        if self.lyrics_open && !self.lyrics_loading {
            if let Some(track) = self.current.as_ref() {
                if self.lyrics_track != Some(track.id) {
                    self.refresh_lyrics();
                }
            }
        }
        self.poll();
        self.probe();
        self.persist(false);
        self.flush_settings();
        if let Some((_, started)) = &self.toast {
            if started.elapsed().as_secs_f32() > 2.6 {
                self.toast = None;
                self.toast_action = None;
            }
        }
        self.update_media_tick();
    }

    fn poll(&mut self) {
        while let Ok(event) = self.jobs.rx.try_recv() {
            match event {
                Event::Search {
                    query,
                    provider,
                    results,
                    error,
                } => {
                    if query != self.search.query.trim() || provider != self.search.provider {
                        continue;
                    }
                    self.search.running = false;
                    self.search.started = true;
                    match error {
                        Some(message) => {
                            self.search.error = Some(message.clone());
                            self.search.results.clear();
                            self.fail(message, Some(ToastAction::Search));
                        }
                        None => {
                            self.search.error = None;
                            self.search.results = results;
                        }
                    }
                }
                Event::Region { result } => {
                    self.geo_loading = false;
                    match result {
                        Ok(geo) => {
                            let language = crate::region::language_for(&geo);
                            self.settings.region_language = Some(language.to_string());
                            self.settings.region_label = Some(geo.region.clone()).filter(|region| !region.is_empty());
                            self.touch_settings();
                            self.refresh_trending();
                        }
                        Err(message) => {
                            crate::logs::error(&message);
                            self.trending_error = Some(message);
                        }
                    }
                }
                Event::Trending { language, tracks, error } => {
                    if self.settings.region_language.as_deref() != Some(language.as_str()) {
                        continue;
                    }
                    self.trending_loading = false;
                    match error {
                        Some(message) => {
                            crate::logs::error(&format!("trending {language}: {message}"));
                            self.trending_error = Some(message);
                        }
                        None => {
                            self.trending_error = None;
                            self.trending = tracks;
                        }
                    }
                }
                Event::Warmed { track_id, warmed } => {
                    if self.warm_requested == Some(track_id) {
                        self.warm_requested = None;
                    }
                    if let Some((stream, http)) = warmed {
                        self.warm = Some(Warmed { track_id, stream, http, at: Instant::now() });
                    }
                }
                Event::Resolved { gen, mut track, result } => {
                    if gen != self.play_gen {
                        continue;
                    }
                    self.resolving = false;
                    match result {
                        Ok(stream) => {
                            track.provider_meta = Some(ProviderMeta {
                                provider: stream.provider,
                                bitrate_kbps: stream.bitrate_kbps,
                            });
                            let duration = stream.duration.or(track.duration);
                            self.player.send(Command::Play(Box::new(PlayRequest {
                                track,
                                resolved: Some(stream),
                                start: Duration::ZERO,
                                duration,
                                fade: self.pending_fade,
                                warm: None,
                            })));
                        }
                        Err(message) => {
                            let title = track.title.clone();
                            self.fail(
                                format!("Cannot play {title}: {message}"),
                                Some(ToastAction::Resolve(track)),
                            );
                        }
                    }
                }
                Event::FixMatch {
                    track_id,
                    alternatives,
                    error,
                } => {
                    if let Some(state) = self.fix_match.as_mut() {
                        if state.track.id == track_id {
                            state.running = false;
                            state.alternatives = alternatives;
                            state.error = error;
                        }
                    }
                }
                Event::DownloadProgress { track_id, done, total } => {
                    self.download_progress.insert(track_id, (done, total));
                }
                Event::Download { track, result } => {
                    self.download_progress.remove(&track.id);
                    self.downloading.retain(|item| item.id != track.id);
                    self.download_cancels.retain(|(id, _)| *id != track.id);
                    match result {
                        Ok(path) => {
                            let _ = crate::db::save_download(&track, &path);
                            self.downloads.retain(|t| t.id != track.id);
                            self.downloads.insert(0, track.clone());
                            self.notify(format!("Saved {} offline", track.title));
                        }
                        Err(message) if message == "Cancelled" => {}
                        Err(message) => {
                            let title = track.title.clone();
                            self.fail(
                                format!("Download failed for {title}: {message}"),
                                Some(ToastAction::Download(track)),
                            );
                        }
                    }
                }
                Event::Connectivity { online } => {
                    self.probe_busy = false;
                    if online != self.online {
                        self.online = online;
                        if !self.settings.offline_mode {
                            self.notify(if online { "Back online" } else { "Offline" });
                        }
                        if let Some(ctx) = &self.ctx {
                            ctx.request_repaint();
                        }
                    }
                }
                Event::YtDlp { message, ready } => {
                    let failed = message.contains("failed");
                    let missing = !ready && message.contains("not installed");
                    self.ytdlp_status = Some((message.clone(), ready));
                    if failed {
                        self.fail(message, Some(ToastAction::YtDlp));
                    } else if missing {
                        self.fail(
                            "yt-dlp is missing: install it to stream from YouTube",
                            Some(ToastAction::YtDlp),
                        );
                    }
                }
                Event::Thumb { track_id, image } => {
                    self.thumbs.clear_pending(track_id);
                    match image {
                        Some(image) => {
                            if let Some(ctx) = self.ctx.clone() {
                                self.thumbs.insert(track_id, &ctx, image);
                                ctx.request_repaint();
                            }
                        }
                        None => self.thumbs.mark_failed(track_id),
                    }
                }
                Event::Lyrics { track_id, result } => {
                    if self.lyrics_track != Some(track_id) {
                        continue;
                    }
                    self.lyrics_loading = false;
                    match result {
                        Ok(body) => {
                            self.lyrics_lines = crate::lyrics::parse_lrc(&body);
                            self.lyrics_error = if self.lyrics_lines.is_empty() {
                                Some("No timed lyrics for this track".to_string())
                            } else {
                                None
                            };
                        }
                        Err(message) => {
                            self.lyrics_lines.clear();
                            self.lyrics_error = Some(message);
                        }
                    }
                }
                Event::Radio {
                    seed,
                    tracks,
                    error,
                } => {
                    self.radio_pending = false;
                    if tracks.is_empty() {
                        if let Some(message) = error {
                            crate::logs::error(&format!("Radio for {seed} failed: {message}"));
                        }
                    } else {
                        self.queue.set_context(tracks.clone(), format!("{seed} radio"));
                        self.queue.mark_context_playing(0);
                        if let Some(track) = tracks.first().cloned() {
                            self.start_track(track, TrackOrigin::Context, Duration::ZERO);
                        }
                    }
                }
                Event::ImportMatch {
                    token,
                    index,
                    tracks,
                    error,
                } => {
                    if token != self.import.token {
                        continue;
                    }
                    self.import.inflight = 0;
                    if let Some(entry) = self.import.entries.get_mut(index) {
                        if tracks.is_empty() {
                            let message = error.unwrap_or_else(|| "No match found".to_string());
                            entry.state = crate::import::MatchState::Failed(message);
                        } else {
                            entry.matched = tracks.first().cloned();
                            entry.alternatives = tracks.into_iter().skip(1).collect();
                            entry.state = crate::import::MatchState::Matched;
                            entry.skipped = false;
                        }
                    }
                    self.pump_import();
                }
                Event::ImportPlaylist {
                    token,
                    tracks,
                    error,
                } => {
                    if token != self.import.token {
                        continue;
                    }
                    self.import.loading_playlist = false;
                    match error {
                        Some(message) => {
                            self.import.error = Some(message.clone());
                            self.fail(format!("Import failed: {message}"), None);
                        }
                        None => {
                            if crate::ytdlp::is_youtube_mix(&self.import.url) {
                                self.save_mix_as_playlist(tracks);
                                continue;
                            }
                            let mut entries = Vec::new();
                            for track in tracks {
                                let mut entry = crate::import::ImportEntry::with_duration(
                                    track.title.clone(),
                                    track.artist.clone(),
                                    track.duration,
                                );
                                entry.matched = Some(track);
                                entry.state = crate::import::MatchState::Matched;
                                entries.push(entry);
                            }
                            self.import.entries = entries;
                            self.import.source = self.import.url.clone();
                            self.import.error = None;
                            self.import.swap = None;
                            self.import.inflight = 0;
                            self.import.target = None;
                            if self.playlists.is_empty() {
                                self.import.new_playlist = "Imported playlist".to_string();
                            }
                            self.page = Page::Import;
                        }
                    }
                }
                Event::AudioEnded => {
                    if self.sleep.track_finished() {
                        self.player.send(Command::Stop);
                        self.notify("Sleep timer stopped playback");
                        self.persist(true);
                    } else {
                        self.play_advance(true);
                    }
                }
                Event::AudioError { message } => self.fail(message, None),
            }
        }

        while let Ok(action) = self.media_rx.try_recv() {
            self.handle_media(action);
        }
    }

    fn handle_media(&mut self, action: MediaAction) {
        match action {
            MediaAction::Play => self.toggle_play(),
            MediaAction::Pause => self.player.send(Command::Pause),
            MediaAction::Toggle => self.toggle_play(),
            MediaAction::Next => self.play_advance(false),
            MediaAction::Previous => self.previous(),
            MediaAction::Stop => {
                self.player.send(Command::Stop);
                media_controls::set_stopped(self.media.as_mut());
            }
            MediaAction::SeekTo(position) => self.seek(position.as_secs_f64()),
            MediaAction::SeekRelative(secs) => {
                let target = (self.position() + secs as f64).max(0.0);
                self.seek(target);
            }
            MediaAction::SetVolume(volume) => self.set_volume(volume as f32),
        }
    }

    fn update_media_track(&mut self) {
        let Some(track) = self.current.clone() else {
            media_controls::set_stopped(self.media.as_mut());
            self.last_media_state = Some(false);
            return;
        };
        media_controls::set_track(
            self.media.as_mut(),
            &track.title,
            track.display_artist(),
            track.thumb.as_deref(),
            track.duration,
        );
        self.last_media_state = None;
        self.last_media_position = u64::MAX;
    }

    fn update_media_tick(&mut self) {
        let playing = self.is_playing();
        let position = self.player.snapshot().position;
        let seconds = position.as_secs();
        let state_changed = self.last_media_state != Some(playing);
        if playing {
            if state_changed || self.last_media_position != seconds {
                media_controls::set_state(self.media.as_mut(), true, position);
            }
        } else if state_changed && self.current.is_some() {
            media_controls::set_state(self.media.as_mut(), false, position);
        }
        self.last_media_state = Some(playing);
        self.last_media_position = seconds;
    }
}

fn collect_audio_files(folder: &std::path::Path, depth: usize) -> Vec<PathBuf> {
    const EXTENSIONS: [&str; 9] = [
        "mp3", "flac", "ogg", "oga", "m4a", "aac", "wav", "opus", "mp4",
    ];
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth > 0 {
                found.extend(collect_audio_files(&path, depth - 1));
            }
            continue;
        }
        let is_audio = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
            .unwrap_or(false);
        if is_audio {
            found.push(path);
        }
    }
    found
}

pub struct Warmed {
    track_id: u64,
    stream: crate::provider::ResolvedStream,
    http: crate::http::HttpStream,
    at: Instant,
}

const WARM_MAX_AGE: Duration = Duration::from_secs(15 * 60);
const WARM_AFTER_SECS: f64 = 5.0;

pub fn warm_is_usable(warmed_id: u64, wanted_id: u64, age: Duration) -> bool {
    warmed_id == wanted_id && age < WARM_MAX_AGE
}

pub fn should_warm(position_secs: f64, local: bool, already: bool, is_current: bool) -> bool {
    !local && !already && !is_current && position_secs >= WARM_AFTER_SECS
}

pub fn effective_state(audio: AudioState, resolving: bool) -> AudioState {
    if resolving {
        AudioState::Loading
    } else {
        audio
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {

        if self.busy() || self.audio_state() == AudioState::Loading {
            let interval = if self.settings.battery_saver { 250 } else { 80 };
            ui.ctx().request_repaint_after(Duration::from_millis(interval));
        }
        self.tick();
        self.ensure_media(frame);
        crate::ui::root(self, ui);
        let ctx = ui.ctx().clone();
        crate::miniplayer::show(self, &ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings_now();
        self.writer.shutdown();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::palette(self.settings.theme)
            .bg
            .to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod state_tests {
    use super::{effective_state, should_warm, warm_is_usable};
    use std::time::Duration;
    use crate::player::AudioState;

    #[test]
    fn next_track_is_warmed_only_when_useful() {
        assert!(should_warm(6.0, false, false, false));
        assert!(!should_warm(2.0, false, false, false));
        assert!(!should_warm(30.0, true, false, false));
        assert!(!should_warm(30.0, false, true, false));
        assert!(!should_warm(30.0, false, false, true));
    }

    #[test]
    fn warmed_stream_must_match_the_track_and_be_fresh() {
        assert!(warm_is_usable(5, 5, Duration::from_secs(60)));
        assert!(!warm_is_usable(5, 6, Duration::from_secs(60)));
        assert!(!warm_is_usable(5, 5, Duration::from_secs(16 * 60)));
    }

    #[test]
    fn resolving_counts_as_loading_so_play_cannot_start_a_second_load() {
        for audio in [AudioState::Idle, AudioState::Playing, AudioState::Paused, AudioState::Loading] {
            assert_eq!(effective_state(audio, true), AudioState::Loading);
            assert_eq!(effective_state(audio, false), audio);
        }
    }
}
