use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use egui::Context;
use raw_window_handle::RawWindowHandle;
use raw_window_handle::HasWindowHandle;
use crate::media_controls::MediaControls;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    More,
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
    pub pasted: String,

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

pub use crate::search_state::SearchState;

pub struct App {
    pub settings: Settings,
    pub player: PlayerHandle,
    pub tracks: Vec<Track>,
    pub queue: Queue,
    pub play_stack: crate::queue::PlayStack,
    going_back: bool,
    radio: crate::prefetch::Radio,
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
    warmup: crate::prefetch::Warmup,
    manual_fade: bool,
    pub trending: Vec<Track>,
    pub agent: Option<crate::agent::AgentHandle>,
    pub agent_error: Option<String>,
    pub agent_findings: Vec<crate::doctor::Finding>,
    pub health_running: bool,
    pub lyrics_paste: Option<String>,
    play_ok: u32,
    play_err: u32,
    agent_audit: std::collections::VecDeque<(u64, String, bool)>,
    agent_events: std::collections::VecDeque<(u64, u64, String, serde_json::Value)>,
    agent_seq: u64,
    recent_errors: std::collections::VecDeque<String>,
    pub window_width: f32,
    live_watcher: crate::live::Watcher,
    pub preview_width: Option<f32>,
    pub preview_height: Option<f32>,
    pub layout_overflow: bool,
    features: std::collections::HashMap<u64, crate::pace::Features>,
    features_checked: std::collections::HashSet<u64>,
    pub pace: crate::pace::Profile,
    pub mood: Option<crate::pace::Mood>,
    pub trending_loading: bool,
    pub trending_error: Option<String>,
    pub geo_loading: bool,
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
    pub now_playing_open: bool,
    pub ytdlp_status: Option<(String, bool)>,
    pub update: crate::update::State,
    pub fix_match: Option<FixMatchState>,
    pub new_playlist_dialog: bool,
    pub new_playlist_name: String,
    pub rename_dialog: Option<(usize, String)>,
    pub rename_buffer: String,
    pub playlist_drag: crate::ui::pages::rows::DragState,
    pub queue_drag: crate::ui::pages::rows::DragState,

    pub mini: crate::miniplayer::MiniState,
    play_gen: u64,
    pub adding_songs: bool,
    #[cfg(target_arch = "wasm32")]
    awake_on: bool,
    listen: crate::listen::ListenMeter,
    #[cfg(target_arch = "wasm32")]
    last_layers: u32,
    skip_on_fail: bool,
    skip_streak: u8,

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
        let mini_was_open = settings.mini.open;
        theme::apply(&ctx, &settings);

        #[cfg(not(target_arch = "wasm32"))]
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
            radio: crate::prefetch::Radio::default(),
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
            warmup: crate::prefetch::Warmup::default(),
            manual_fade: false,
            trending: Vec::new(),
            agent: None,
            agent_error: None,
            agent_findings: Vec::new(),
            health_running: false,
            lyrics_paste: None,
            play_ok: 0,
            play_err: 0,
            agent_audit: std::collections::VecDeque::new(),
            agent_events: std::collections::VecDeque::new(),
            agent_seq: 0,
            recent_errors: std::collections::VecDeque::new(),
            window_width: 0.0,
            live_watcher: crate::live::Watcher::default(),
            preview_height: std::env::var("ARIA_PREVIEW_HEIGHT").ok().and_then(|value| value.parse::<f32>().ok()).filter(|height| (120.0..=4000.0).contains(height)),
            preview_width: std::env::var("ARIA_PREVIEW_WIDTH").ok().and_then(|value| value.parse::<f32>().ok()).filter(|width| (200.0..=4000.0).contains(width)),
            layout_overflow: false,
            features: std::collections::HashMap::new(),
            features_checked: std::collections::HashSet::new(),
            pace: crate::pace::Profile::empty(),
            mood: None,
            trending_loading: false,
            trending_error: None,
            geo_loading: false,
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
            now_playing_open: false,
            ytdlp_status: None,
            update: crate::update::State::Idle,
            fix_match: None,
            new_playlist_dialog: false,
            new_playlist_name: String::new(),
            rename_dialog: None,
            rename_buffer: String::new(),
            playlist_drag: crate::ui::pages::rows::DragState::default(),
            queue_drag: crate::ui::pages::rows::DragState::default(),
            mini: crate::miniplayer::MiniState {
                open: cfg!(not(target_arch = "wasm32")) && mini_was_open,
                ..Default::default()
            },
            play_gen: 0,
            adding_songs: false,
            #[cfg(target_arch = "wasm32")]
            awake_on: false,
            listen: crate::listen::ListenMeter::default(),
            #[cfg(target_arch = "wasm32")]
            last_layers: 0,
            skip_on_fail: false,
            skip_streak: 0,
            resolving: false,
            pending_start_ms,
            ctx: Some(ctx),
            last_persist: Instant::now(),
            settings_dirty: false,
            settings_saved_at: Instant::now(),
            last_media_state: None,
            last_media_position: u64::MAX,
        };
        crate::embedded::extract_if_missing();
        let _ = app.jobs.tx.send(Job::EnsureYtDlp);
        if !app.settings.offline_mode {
            app.check_for_updates();
        }
        app.refresh_trending();
        app.warm_restored_track();
        app.sync_agent();
        app
    }

    pub fn ensure_media(&mut self, frame: &eframe::Frame) {
        if self.media_ready {
            return;
        }
        self.media_ready = true;
        #[cfg(not(target_arch = "wasm32"))]
        let hwnd = frame
            .winit_window()
            .and_then(|w| w.window_handle().ok())
            .and_then(|handle| match handle.as_raw() {
                RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as *mut c_void),
                _ => None,
            });
        #[cfg(target_arch = "wasm32")]
        let hwnd: Option<*mut c_void> = {
            let _ = frame;
            None
        };
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
        self.resume_position_ms() as f64 / 1000.0
    }

    fn resume_position_ms(&self) -> u64 {
        let snapshot = self.player.snapshot();
        resume_position_ms(
            snapshot.state == AudioState::Idle && self.current.is_some(),
            self.pending_start_ms,
            snapshot.position.as_millis() as u64,
        )
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
        self.recent_errors.push_back(message.clone());
        while self.recent_errors.len() > 30 {
            self.recent_errors.pop_front();
        }
        self.toast = Some((message, Instant::now()));
        self.toast_action = action;
    }

    pub fn check_for_updates(&mut self) {
        if matches!(self.update, crate::update::State::Checking | crate::update::State::Downloading(_)) {
            return;
        }
        self.update = crate::update::State::Checking;
        let _ = self.jobs.tx.send(Job::CheckUpdate);
    }

    pub fn install_update(&mut self) {
        if let crate::update::State::Available(release) = self.update.clone() {
            match release.asset.clone() {
                Some(asset) => {
                    self.update = crate::update::State::Downloading(release);
                    let _ = self.jobs.tx.send(Job::DownloadUpdate { asset });
                }
                None => {
                    crate::proc::open_folder(std::path::Path::new(&release.page));
                }
            }
        }
    }

    pub fn launch_update(&mut self, ctx: &egui::Context) {
        if let crate::update::State::Ready(path) = self.update.clone() {
            match crate::update::launch(&path) {
                Ok(quit) => {
                    if quit {
                        self.persist(true);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Err(reason) => self.update = crate::update::State::Failed(reason),
            }
        }
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
        self.skip_on_fail = false;
        self.pending_start_ms = None;
        let manual = std::mem::take(&mut self.manual_fade);
        let fade = if std::mem::take(&mut self.crossfade_arm) {
            let base = transition_secs(self.settings.crossfade, manual);
            let from = self.current.as_ref().and_then(|now| self.features.get(&now.id)).copied();
            let to = self.features.get(&track.id).copied();
            Duration::from_secs_f32(if manual { base } else { smart_fade(base, from, to) })
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
        self.learn_from_previous(track.id);
        self.remember_previous(track.id);
        self.current = Some(track.clone());
        self.current_origin = Some(origin);
        self.listen.start(track.id);
        self.play_gen += 1;
        self.record_history(&track);
        let duration = track.duration;
        let offline = self.download_path(track.id);
        let analyze = self.features_for(track.id).is_none();
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
                    analyze,
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
                    analyze,
                })));
            }
            (true, None) => {
                if let Some(warmed) = self.warmup.take(track.id) {
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
                        analyze,
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
        self.play_advance_inner(automatic);
        self.skip_on_fail = true;
    }

    #[cfg(target_arch = "wasm32")]
    fn layer_count(&self) -> u32 {
        let page = match self.page {
            Page::Home => 0,
            Page::PlaylistDetail(_) => 2,
            _ => 1,
        };
        let search = (self.page == Page::Search && self.search_is_showing_results()) as u32;
        page + search + self.adding_songs as u32 + self.lyrics_open as u32 + self.queue_open as u32 + self.now_playing_open as u32
    }

    #[cfg(target_arch = "wasm32")]
    fn go_back(&mut self) -> bool {
        if self.adding_songs {
            self.adding_songs = false;
        } else if self.lyrics_open {
            self.lyrics_open = false;
        } else if self.queue_open {
            self.queue_open = false;
        } else if self.now_playing_open {
            self.now_playing_open = false;
        } else if self.page == Page::Search && self.search_is_showing_results() {
            self.reset_search();
        } else if matches!(self.page, Page::PlaylistDetail(_)) {
            self.page = Page::Playlists;
        } else if self.page != Page::Home {
            self.page = Page::Home;
        } else {
            return false;
        }
        true
    }

    #[cfg(target_arch = "wasm32")]
    fn sync_history(&mut self) {
        let back = crate::install::take_back();
        for _ in 0..back {
            if !self.go_back() {
                self.notify("Swipe back again to exit");
                crate::install::arm_exit();
            }
        }
        let layers = self.layer_count();
        if back > 0 || layers != self.last_layers {
            crate::install::sync_layers(layers);
            self.last_layers = layers;
        }
    }

    pub fn search_is_showing_results(&self) -> bool {
        self.search.showing_results()
    }

    pub fn reset_search(&mut self) {
        self.search.reset();
    }

    pub fn begin_adding_songs(&mut self) {
        self.adding_songs = true;
        self.page = Page::Search;
        self.queue_open = false;
        self.now_playing_open = false;
    }

    pub fn clear_user_queue(&mut self) {
        let mut count = self.queue.user_len();
        while self.queue.user_len() > 0 {
            self.queue.remove_user(0);
        }
        count += self.queue.clear_upcoming_context();
        if count > 0 {
            self.notify("Queue cleared");
            self.persist(true);
        }
    }

    fn play_advance_inner(&mut self, automatic: bool) {
        if !automatic && self.settings.crossfade > 0.05 && self.audio_state() == AudioState::Playing {
            self.crossfade_arm = true;
            self.manual_fade = true;
        }
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
                        let video = self.current.as_ref().and_then(|track| track.yt_id());
                        let _ = self.jobs.tx.send(Job::Radio { seed, video });
                        self.radio.pending = true;
                        self.radio.append = false;
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
        if self.adding_songs {
            self.add_to_queue(track);
            return;
        }
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
        self.haptic();
        self.queue.push_user(track);
        self.notify("Added to queue");
        self.persist(true);
    }

    pub fn copy_to_clipboard(&mut self, text: String, message: &str) {
        if let Some(ctx) = &self.ctx {
            ctx.copy_text(text);
            self.notify(message);
        }
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
        #[cfg(target_arch = "wasm32")]
        {
            crate::install::pick_file();
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.import_file_native();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn import_file_native(&mut self) {
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
        let source = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        self.import_text(&text, is_csv, source);
    }

    pub fn import_pasted(&mut self) {
        let text = std::mem::take(&mut self.import.pasted);
        let is_csv = text.lines().next().is_some_and(|line| line.to_lowercase().contains("track name") || line.to_lowercase().contains("artist name"));
        self.import_text(&text, is_csv, "Pasted list".to_string());
    }

    pub fn import_text(&mut self, text: &str, is_csv: bool, source: String) {
        let text = text.to_string();
        let mut entries = if is_csv {
            crate::import::parse_csv(&text)
        } else {
            Vec::new()
        };
        if entries.is_empty() {
            entries = crate::import::parse_text_list(&text);
        }
        if entries.is_empty() {
            self.fail("No songs found. Use one song per line, like: Title - Artist", None);
            return;
        }
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
        self.persist_history();
    }

    pub fn track_menu_for(&mut self, track: &Track, action: TrackMenuAction) {
        match action {
            TrackMenuAction::Play => self.play_single(track.clone()),
            TrackMenuAction::GoToArtist => {
                let artist = track.artist.split([',', '&']).next().unwrap_or("").trim().to_string();
                if !artist.is_empty() {
                    self.page = Page::Search;
                    self.search.query = artist;
                    self.run_search();
                }
            }
            TrackMenuAction::CopyInfo => {
                let text = if track.artist.trim().is_empty() {
                    track.title.clone()
                } else {
                    format!("{} - {}", track.artist, track.title)
                };
                self.copy_to_clipboard(text, "Copied song info");
            }
            TrackMenuAction::CopyLink => {
                if let Some(link) = track.watch_url() {
                    self.copy_to_clipboard(link, "Copied link");
                }
            }
            TrackMenuAction::NewPlaylist => {
                let names: Vec<String> = self.playlists.iter().map(|p| p.name.clone()).collect();
                let name = crate::suggest::unique_name(&names, "New playlist");
                let mut playlist = Playlist::new(&name);
                playlist.tracks.push(track.clone());
                self.playlists.push(playlist);
                self.persist_playlists();
                self.notify(format!("Created {name} with this song"));
            }
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

    fn haptic(&self) {
        #[cfg(target_arch = "wasm32")]
        if self.settings.haptics {
            crate::install::haptic();
        }
    }

    pub fn toggle_like_track(&mut self, track: &Track) {
        self.haptic();
        if self.is_liked(track.id) {
            self.liked.retain(|t| t.id != track.id);
            self.notify("Removed from Liked Songs");
        } else {
            self.liked.insert(0, track.clone());
            if let Some(features) = self.features_for(track.id) {
                self.pace.learn(features, crate::pace::Reaction::Liked);
            }
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
            AUDIO_EXTENSIONS,
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
            self.search.fail_offline("You're offline. Downloaded and local tracks are still playable from Downloads.");
            return;
        }
        crate::suggest::push_recent(&mut self.settings.recent_searches, &query);
        self.touch_settings();
        let limit = self.search.begin();
        let provider = self.search.provider;
        let _ = self.jobs.tx.send(Job::Search { query, limit, provider });
    }

    pub fn search_more(&mut self) {
        if self.is_offline() {
            return;
        }
        let query = self.search.query.trim().to_string();
        if let Some(limit) = self.search.next_page() {
            let provider = self.search.provider;
            let _ = self.jobs.tx.send(Job::Search { query, limit, provider });
        }
    }

    pub fn set_search_source(&mut self, provider: Option<crate::provider::ProviderName>) {
        if self.search.change_source(provider) && !self.search.query.trim().is_empty() {
            self.run_search();
        }
    }

    pub fn busy(&self) -> bool {
        self.resolving || self.search.running || self.radio.pending || !self.downloading.is_empty()
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

    fn upcoming_tracks(&self, count: usize) -> Vec<Track> {
        let mut upcoming: Vec<Track> = self.queue.user.iter().take(count).cloned().collect();
        if upcoming.len() < count {
            let need = count - upcoming.len();
            upcoming.extend(self.queue.context_preview().into_iter().take(need).map(|(_, track)| track.clone()));
        }
        upcoming
    }

    fn next_preview(&self) -> Option<Track> {
        self.queue
            .user
            .front()
            .cloned()
            .or_else(|| self.queue.context_preview().into_iter().next().map(|(_, track)| track.clone()))
    }

    fn next_is_ready(&mut self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            return match self.next_preview() {
                None => true,
                Some(next) => !next.is_online() || self.download_path(next.id).is_some() || crate::jobs::peek_url(&next).is_some(),
            };
        }
        #[cfg(not(target_arch = "wasm32"))]
        match self.next_preview() {
            None => true,
            Some(next) => {
                !next.is_online()
                    || self.download_path(next.id).is_some()
                    || self.warmup.has(next.id)
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn web_warm_next(&mut self) {
        if self.audio_state() != AudioState::Playing || self.is_offline() {
            return;
        }
        for track in self.upcoming_tracks(2) {
            if track.is_online() && self.download_path(track.id).is_none() && self.warmup.request(track.id) {
                let _ = self.jobs.tx.send(Job::Warm { track });
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn web_preload_next(&mut self) {
        if self.audio_state() != AudioState::Playing || self.position() < 3.0 {
            return;
        }
        if let Some(url) = self.next_preview().as_ref().and_then(crate::jobs::peek_url) {
            self.player.send(Command::Preload(url));
        }
    }

    fn maybe_warm_next(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.web_warm_next();
            self.web_preload_next();
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.audio_state() != AudioState::Playing || self.is_offline() || self.position() < crate::prefetch::AFTER_SECS {
            return;
        }
        self.warmup.expire();
        let current = self.current.as_ref().map(|track| track.id);
        let upcoming = self.upcoming_tracks(crate::prefetch::AHEAD);
        let candidates: Vec<(u64, bool)> = upcoming
            .iter()
            .map(|track| (track.id, !track.is_online() || self.download_path(track.id).is_some()))
            .collect();
        for index in self.warmup.targets(&candidates, current) {
            let track = upcoming[index].clone();
            self.warmup.request(track.id);
            let _ = self.jobs.tx.send(Job::Warm { track });
        }
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

    pub fn features_for(&mut self, id: u64) -> Option<crate::pace::Features> {
        if let Some(features) = self.features.get(&id) {
            return Some(*features);
        }
        if self.features_checked.insert(id) {
            if let Some(features) = crate::db::load_features(id) {
                self.features.insert(id, features);
                return Some(features);
            }
        }
        None
    }

    fn learn_from_previous(&mut self, next_id: u64) {
        let Some(previous) = self.current.clone() else {
            return;
        };
        if previous.id == next_id {
            return;
        }
        let snapshot = self.player.snapshot();
        if snapshot.track_id != Some(previous.id) {
            return;
        }
        let duration = snapshot.duration.or(previous.duration).map(|d| d.as_secs_f64());
        let reaction = crate::pace::reaction_for(snapshot.position.as_secs_f64(), duration);
        if let (Some(reaction), Some(features)) = (reaction, self.features_for(previous.id)) {
            self.pace.learn(features, reaction);
        }
    }

    pub fn rank_for_pace(&mut self, tracks: Vec<Track>) -> Vec<Track> {
        for track in &tracks {
            let _ = self.features_for(track.id);
        }
        let target = self.pace.target(self.mood);
        let mut recent: Vec<String> = self
            .history
            .iter()
            .take(5)
            .chain(self.current.iter())
            .map(|track| track.artist.split([',', '&']).next().unwrap_or("").trim().to_lowercase())
            .filter(|artist| !artist.is_empty())
            .collect();
        recent.dedup();
        let known = |id: u64| self.features.get(&id).copied();
        crate::pace::rank(tracks, target, &known, &recent)
    }

    pub fn set_mood(&mut self, mood: Option<crate::pace::Mood>) {
        self.mood = mood;
    }

    pub fn start_mood_mix(&mut self, mood: crate::pace::Mood) {
        if self.is_offline() {
            self.notify("Mood mixes need an internet connection");
            return;
        }
        self.mood = Some(mood);
        let language = self
            .settings
            .region_language
            .as_deref()
            .filter(|_| self.settings.region_mode != RegionMode::Off)
            .map(crate::region::language_label)
            .unwrap_or("");
        let seed = format!("{} {}", mood.query(), language).trim().to_string();
        self.radio.label = Some(format!("{} mix", mood.label()));
        self.radio.pending = true;
        let _ = self.jobs.tx.send(Job::Radio { seed, video: None });
        self.notify(format!("Building a {} mix", mood.label().to_lowercase()));
    }

    fn warm_restored_track(&mut self) {
        let Some(track) = self.current.clone() else {
            return;
        };
        if !track.is_online() || self.is_offline() || self.download_path(track.id).is_some() {
            return;
        }
        self.warmup.request(track.id);
        let _ = self.jobs.tx.send(Job::Warm { track });
    }

    pub fn top_artists(&self, limit: usize) -> Vec<(String, Track)> {
        let mut counts: Vec<(String, usize, Track)> = Vec::new();
        for track in self.liked.iter().chain(self.history.iter()) {
            let artist = track.artist.trim();
            if artist.is_empty() || !track.is_online() {
                continue;
            }
            match counts.iter_mut().find(|(name, _, _)| name.eq_ignore_ascii_case(artist)) {
                Some(entry) => entry.1 += 1,
                None => counts.push((artist.to_string(), 1, track.clone())),
            }
        }
        counts.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        counts.into_iter().take(limit).map(|(name, _, track)| (name, track)).collect()
    }

    pub fn play_artist_mix(&mut self, artist: &str) {
        if self.is_offline() {
            return;
        }
        self.radio.label = Some(format!("{artist} mix"));
        self.radio.pending = true;
        self.radio.append = false;
        let _ = self.jobs.tx.send(Job::Radio { seed: artist.to_string(), video: None });
    }

    pub fn prefetch_home(&mut self) {
        if self.is_offline() {
            return;
        }
        let picks: Vec<Track> = self.trending.iter().take(3).chain(self.history.iter().take(3)).cloned().collect();
        for track in picks {
            if !track.is_online() || self.warmup.is_requested(track.id) || self.download_path(track.id).is_some() {
                continue;
            }
            self.warmup.request(track.id);
            let _ = self.jobs.tx.send(Job::Warm { track });
        }
    }

    pub fn sync_agent(&mut self) {
        let wanted = self.settings.agent_enabled || std::env::var_os("ARIA_AGENT").is_some();
        if !wanted {
            self.agent = None;
            self.agent_error = None;
            return;
        }
        if self.agent.is_some() {
            return;
        }
        let Some(ctx) = self.ctx.clone() else {
            return;
        };
        let port = std::env::var("ARIA_AGENT_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(crate::agent::DEFAULT_PORT);
        match crate::agent::start(ctx, port) {
            Ok(handle) => {
                self.agent_error = None;
                self.agent = Some(handle);
            }
            Err(error) => {
                crate::logs::error(&format!("agent mode could not start: {error}"));
                self.agent_error = Some(error);
            }
        }
    }

    pub fn doctor_facts(&self) -> crate::doctor::Facts {
        let log = std::fs::read_to_string(crate::logs::log_path()).unwrap_or_default();
        let mut lines = crate::agent::tail_lines(&log, 300);
        lines.extend(self.recent_errors.iter().cloned());
        crate::doctor::Facts {
            ytdlp_version: crate::ytdlp::version(),
            online: self.online,
            lines,
            window_width: self.window_width,
            overflow: self.layout_overflow,
            today_days: crate::doctor::today_days(),
        }
    }

    pub fn run_doctor(&mut self) {
        let findings = crate::doctor::analyze(&self.doctor_facts());
        let _ = std::fs::create_dir_all(crate::agent::agent_dir());
        let _ = std::fs::write(
            crate::agent::agent_dir().join("doctor-report.md"),
            crate::doctor::report_markdown(&findings),
        );
        self.agent_findings = findings;
        self.run_health();
    }

    pub fn run_health(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.health_running {
                return;
            }
            self.health_running = true;
            let inputs = crate::health::Inputs { downloads_dir: crate::downloads::dir(), play_ok: self.play_ok, play_err: self.play_err };
            let events = self.jobs.event_tx.clone();
            let spawned = std::thread::Builder::new().name("aria-health".into()).spawn(move || {
                let findings = crate::health::run(inputs);
                let _ = events.send(Event::Health { findings });
            });
            if spawned.is_err() {
                self.health_running = false;
            }
        }
    }

    fn now_ms() -> u64 {
        web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
    }

    fn record_agent_event(&mut self, kind: &str, data: serde_json::Value) {
        if self.agent.is_none() {
            return;
        }
        self.agent_seq += 1;
        self.agent_events.push_back((self.agent_seq, Self::now_ms(), kind.to_string(), data));
        while self.agent_events.len() > 200 {
            self.agent_events.pop_front();
        }
    }

    fn poll_agent(&mut self, ctx: &egui::Context) {
        loop {
            let call = match self.agent.as_ref().and_then(|handle| handle.calls.try_recv().ok()) {
                Some(call) => call,
                None => break,
            };
            self.agent_execute(ctx, call);
        }
    }

    fn agent_state(&self, ctx: &egui::Context) -> serde_json::Value {
        use serde_json::json;
        let snapshot = self.player.snapshot();
        let lay = crate::ui::responsive::plan(self.window_width.max(1.0));
        let track = self.current.as_ref().map(|track| {
            json!({
                "id": track.id,
                "title": track.title,
                "artist": track.artist,
                "online": track.is_online(),
                "provider": track.provider_meta.as_ref().map(|meta| meta.provider.label()),
            })
        });
        json!({
            "track": track,
            "audio": format!("{:?}", self.audio_state()).to_lowercase(),
            "position_secs": self.position(),
            "duration_secs": self.duration(),
            "volume": self.settings.volume,
            "shuffle": self.settings.shuffle,
            "repeat": format!("{:?}", self.settings.repeat).to_lowercase(),
            "queue_user": self.queue.user_len(),
            "page": crate::agent::page_name(self.page),
            "queue_open": self.queue_open,
            "update": format!("{:?}", self.update),
            "lyrics_open": self.lyrics_open,
            "now_playing_open": self.now_playing_open,
            "miniplayer_open": self.mini.open,
            "online": self.online,
            "offline_mode": self.settings.offline_mode,
            "search": {
                "query": self.search.query,
                "running": self.search.running,
                "results": self.search.results.len(),
                "error": self.search.error,
            },
            "mood": self.mood.map(|mood| mood.label()),
            "pace": { "bpm": self.pace.bpm, "energy": self.pace.energy, "samples": self.pace.samples },
            "ytdlp": crate::ytdlp::discover().map(|path| path.display().to_string()),
            "window": { "width": self.window_width, "ppp": ctx.pixels_per_point() },
            "layout": {
                "breakpoint": format!("{:?}", lay.breakpoint).to_lowercase(),
                "nav": format!("{:?}", lay.nav).to_lowercase(),
                "sheet": format!("{:?}", lay.sheet),
                "overflow": self.layout_overflow,
            },
            "player_state_debug": format!("{:?}", snapshot.state),
            "recent_errors": self.recent_errors.iter().rev().take(10).collect::<Vec<_>>(),
        })
    }

    fn agent_execute(&mut self, ctx: &egui::Context, call: crate::agent::Call) {
        use serde_json::json;
        let args = call.args.clone();
        let number = |key: &str| args.get(key).and_then(|value| value.as_f64());
        let text = |key: &str| args.get(key).and_then(|value| value.as_str()).map(str::to_string);
        if crate::agent::scope_of(&call.cmd) == Some(crate::agent::Scope::Destructive) && !self.settings.agent_destructive {
            let message = format!("\"{}\" is a destructive command. Turn on \"Allow destructive commands\" in Settings, Agent mode, to use it", call.cmd);
            return self.agent_reply(call, Err(message));
        }
        let result: Result<serde_json::Value, String> = match call.cmd.as_str() {
            "ping" => Ok(json!({ "pong": true, "version": env!("CARGO_PKG_VERSION") })),
            "state" => Ok(self.agent_state(ctx)),
            "play" => {
                if self.audio_state() != AudioState::Playing {
                    self.toggle_play();
                }
                Ok(json!({ "ok": true }))
            }
            "pause" => {
                if self.audio_state() == AudioState::Playing {
                    self.toggle_play();
                }
                Ok(json!({ "ok": true }))
            }
            "toggle" => {
                self.toggle_play();
                Ok(json!({ "ok": true }))
            }
            "next" => {
                self.play_advance(false);
                Ok(json!({ "ok": true }))
            }
            "prev" => {
                self.previous();
                Ok(json!({ "ok": true }))
            }
            "seek" => match number("secs") {
                Some(secs) if secs >= 0.0 => {
                    self.seek(secs);
                    Ok(json!({ "ok": true }))
                }
                _ => Err("seek needs args.secs (a number of seconds)".to_string()),
            },
            "volume" => match number("value") {
                Some(value) => {
                    self.set_volume(value as f32);
                    Ok(json!({ "volume": self.settings.volume }))
                }
                None => Err("volume needs args.value between 0 and 1".to_string()),
            },
            "like" => {
                self.toggle_like_current();
                Ok(json!({ "liked": self.liked_current() }))
            }
            "navigate" => match text("page").and_then(|name| crate::agent::parse_page(&name)) {
                Some(page) => {
                    self.page = page;
                    Ok(json!({ "page": crate::agent::page_name(page) }))
                }
                None => Err("navigate needs args.page: home, library, search, playlists, liked, history, stats, downloads, import, settings or more".to_string()),
            },
            "panel" => {
                let name = text("name").unwrap_or_default();
                let on = args.get("open").and_then(|value| value.as_bool());
                match name.as_str() {
                    "queue" => self.queue_open = on.unwrap_or(!self.queue_open),
                    "lyrics" => {
                        if on.unwrap_or(!self.lyrics_open) != self.lyrics_open {
                            self.toggle_lyrics();
                        }
                    }
                    "now_playing" => self.now_playing_open = on.unwrap_or(!self.now_playing_open),
                    "miniplayer" => {
                        if on.unwrap_or(!self.mini.open) != self.mini.open {
                            self.toggle_mini();
                        }
                    }
                    _ => return self.agent_reply(call, Err("panel needs args.name: queue, lyrics, now_playing or miniplayer".to_string())),
                }
                Ok(json!({ "ok": true }))
            }
            "search" => match text("query") {
                Some(query) if !query.trim().is_empty() => {
                    self.page = Page::Search;
                    self.search.query = query;
                    if let Some(name) = text("provider") {
                        self.search.provider = match name.to_lowercase().as_str() {
                            "ytmusic" | "ytm" => Some(crate::provider::ProviderName::YoutubeMusic),
                            "jiosaavn" | "saavn" => Some(crate::provider::ProviderName::Jiosaavn),
                            "youtube" | "yt" => Some(crate::provider::ProviderName::Youtube),
                            _ => None,
                        };
                    }
                    self.run_search();
                    Ok(json!({ "started": true }))
                }
                _ => Err("search needs args.query".to_string()),
            },
            "results" => Ok(json!(self
                .search
                .results
                .iter()
                .take(30)
                .enumerate()
                .map(|(index, track)| json!({
                    "index": index,
                    "title": track.title,
                    "artist": track.artist,
                    "provider": track.provider_meta.as_ref().map(|meta| meta.provider.label()),
                }))
                .collect::<Vec<_>>())),
            "play_result" => match number("index") {
                Some(index) if (index as usize) < self.search.results.len() => {
                    self.play_search_result(index as usize);
                    Ok(json!({ "ok": true }))
                }
                _ => Err("play_result needs args.index of an existing search result".to_string()),
            },
            "mood" => match text("name").as_deref() {
                Some("none") | None => {
                    self.set_mood(None);
                    Ok(json!({ "mood": null }))
                }
                Some(name) => match crate::pace::Mood::ALL.iter().find(|mood| mood.label().eq_ignore_ascii_case(name)) {
                    Some(mood) => {
                        self.start_mood_mix(*mood);
                        Ok(json!({ "mood": mood.label() }))
                    }
                    None => Err("mood needs args.name: chill, focus, workout, party, sad or none".to_string()),
                },
            },
            "window" => match (number("width"), number("height")) {
                (Some(width), Some(height)) if (200.0..=4000.0).contains(&width) && (200.0..=4000.0).contains(&height) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width as f32, height as f32)));
                    Ok(json!({ "requested": [width, height] }))
                }
                _ => Err("window needs args.width and args.height between 200 and 4000".to_string()),
            },
            "logs" => {
                let count = number("lines").unwrap_or(40.0).clamp(1.0, 500.0) as usize;
                let log = std::fs::read_to_string(crate::logs::log_path()).unwrap_or_default();
                Ok(json!(crate::agent::tail_lines(&log, count)))
            }
            "diagnose" => {
                self.run_doctor();
                Ok(json!({
                    "findings": self.agent_findings.iter().map(|finding| finding.to_json()).collect::<Vec<_>>(),
                    "report": crate::agent::agent_dir().join("doctor-report.md").display().to_string(),
                }))
            }
            "commands" => Ok(crate::agent::describe()),
            "health" => {
                if args.get("run").and_then(|value| value.as_bool()).unwrap_or(false) {
                    self.run_health();
                }
                Ok(json!({
                    "running": self.health_running,
                    "findings": self.agent_findings.iter().map(|finding| finding.to_json()).collect::<Vec<_>>(),
                }))
            }
            "history" => {
                let limit = number("limit").map(|n| n as usize).unwrap_or(25).min(200);
                Ok(json!(self.history.iter().take(limit).map(|t| json!({ "id": t.id, "title": t.title, "artist": t.artist, "duration": t.duration.map(|d| d.as_secs()) })).collect::<Vec<_>>()))
            }
            "events" => {
                let since = number("since").map(|n| n as u64).unwrap_or(0);
                let items: Vec<_> = self.agent_events.iter().filter(|(seq, ..)| *seq > since).map(|(seq, at, kind, data)| json!({ "seq": seq, "at_ms": at, "kind": kind, "data": data })).collect();
                Ok(json!({ "latest": self.agent_seq, "events": items }))
            }
            "audit" => {
                let limit = number("limit").map(|n| n as usize).unwrap_or(50).min(200);
                Ok(json!(self.agent_audit.iter().rev().take(limit).map(|(at, cmd, ok)| json!({ "at_ms": at, "cmd": cmd, "ok": ok })).collect::<Vec<_>>()))
            }
            "shuffle" => {
                self.toggle_shuffle();
                Ok(json!({ "shuffle": self.settings.shuffle }))
            }
            "repeat" => {
                self.cycle_repeat();
                Ok(json!({ "repeat": format!("{:?}", self.settings.repeat).to_lowercase() }))
            }
            "settings" => serde_json::to_value(&self.settings).map_err(|error| error.to_string()),
            "set_setting" => {
                let key = text("key").unwrap_or_default();
                let value = args.get("value").cloned().unwrap_or(serde_json::Value::Null);
                let flag = value.as_bool();
                let amount = value.as_f64();
                let outcome: Result<(), String> = match (key.as_str(), flag, amount) {
                    ("volume", _, Some(v)) => {
                        self.set_volume(v as f32);
                        Ok(())
                    }
                    ("crossfade", _, Some(v)) => {
                        self.settings.crossfade = (v as f32).clamp(0.0, 12.0);
                        Ok(())
                    }
                    ("autoplay", Some(v), _) => {
                        self.settings.autoplay = v;
                        Ok(())
                    }
                    ("normalize", Some(v), _) => {
                        self.settings.normalize = v;
                        self.normalize.store(v, std::sync::atomic::Ordering::Relaxed);
                        Ok(())
                    }
                    ("battery_saver", Some(v), _) => {
                        self.settings.battery_saver = v;
                        Ok(())
                    }
                    ("offline_mode", Some(v), _) => {
                        self.settings.offline_mode = v;
                        Ok(())
                    }
                    ("mini_opacity", _, Some(v)) => {
                        self.settings.mini_opacity = (v as f32).clamp(crate::miniplayer::MIN_OPACITY, 1.0);
                        Ok(())
                    }
                    ("mini_auto_fade", Some(v), _) => {
                        self.settings.mini_auto_fade = v;
                        Ok(())
                    }
                    ("theme", _, _) => match value.as_str() {
                        Some("dark") => {
                            self.settings.theme = crate::settings::ThemeVariant::Dark;
                            Ok(())
                        }
                        Some("amoled") => {
                            self.settings.theme = crate::settings::ThemeVariant::Amoled;
                            Ok(())
                        }
                        _ => Err("theme must be \"dark\" or \"amoled\"".to_string()),
                    },
                    _ => Err("set_setting keys: volume, crossfade, autoplay, normalize, battery_saver, offline_mode, mini_opacity, mini_auto_fade, theme (with a value of the right type)".to_string()),
                };
                outcome.map(|()| {
                    self.touch_settings();
                    json!({ "ok": true })
                })
            }
            "queue" => Ok(json!({
                "user": self.queue.user.iter().map(|track| json!({ "id": track.id, "title": track.title, "artist": track.artist })).collect::<Vec<_>>(),
                "context_name": self.queue.context_name,
                "context_next": self.queue.context_preview().into_iter().take(30).map(|(_, track)| json!({ "id": track.id, "title": track.title, "artist": track.artist })).collect::<Vec<_>>(),
            })),
            "queue_add" => match number("index").and_then(|index| self.search.results.get(index as usize).cloned()) {
                Some(track) => {
                    self.add_to_queue(track);
                    Ok(json!({ "queue_user": self.queue.user_len() }))
                }
                None => Err("queue_add needs args.index of an existing search result".to_string()),
            },
            "queue_remove" => match number("index") {
                Some(index) if (index as usize) < self.queue.user_len() => {
                    self.queue.remove_user(index as usize);
                    self.persist(true);
                    Ok(json!({ "queue_user": self.queue.user_len() }))
                }
                _ => Err("queue_remove needs args.index inside the user queue".to_string()),
            },
            "queue_clear" => {
                self.queue.user.clear();
                self.persist(true);
                Ok(json!({ "queue_user": 0 }))
            }
            "playlists" => Ok(json!(self
                .playlists
                .iter()
                .enumerate()
                .map(|(index, playlist)| json!({ "index": index, "name": playlist.name, "tracks": playlist.tracks.len() }))
                .collect::<Vec<_>>())),
            "playlist" => match number("index").and_then(|index| self.playlists.get(index as usize)) {
                Some(playlist) => Ok(json!({
                    "name": playlist.name,
                    "tracks": playlist.tracks.iter().take(200).enumerate().map(|(index, track)| json!({ "index": index, "title": track.title, "artist": track.artist })).collect::<Vec<_>>(),
                })),
                None => Err("playlist needs args.index of an existing playlist".to_string()),
            },
            "playlist_create" => match text("name") {
                Some(name) if !name.trim().is_empty() => {
                    self.create_playlist(&name);
                    Ok(json!({ "playlists": self.playlists.len() }))
                }
                _ => Err("playlist_create needs args.name".to_string()),
            },
            "playlist_add" => match (number("playlist"), number("result_index")) {
                (Some(playlist), Some(result)) if (playlist as usize) < self.playlists.len() => match self.search.results.get(result as usize).cloned() {
                    Some(track) => {
                        self.add_to_playlist(playlist as usize, track);
                        Ok(json!({ "ok": true }))
                    }
                    None => Err("result_index is not an existing search result".to_string()),
                },
                _ => Err("playlist_add needs args.playlist (index) and args.result_index".to_string()),
            },
            "playlist_play" => match number("index") {
                Some(index) if (index as usize) < self.playlists.len() => {
                    self.play_playlist(index as usize, number("track").unwrap_or(0.0) as usize);
                    Ok(json!({ "ok": true }))
                }
                _ => Err("playlist_play needs args.index of an existing playlist".to_string()),
            },
            "playlist_delete" => match number("index") {
                Some(index) if (index as usize) < self.playlists.len() => {
                    self.delete_playlist(index as usize);
                    Ok(json!({ "playlists": self.playlists.len() }))
                }
                _ => Err("playlist_delete needs args.index of an existing playlist".to_string()),
            },
            "list" => {
                let tracks: Option<&Vec<crate::model::Track>> = match text("name").as_deref() {
                    Some("liked") => Some(&self.liked),
                    Some("history") => Some(&self.history),
                    Some("downloads") => Some(&self.downloads),
                    Some("library") => Some(&self.tracks),
                    Some("trending") => Some(&self.trending),
                    _ => None,
                };
                match tracks {
                    Some(tracks) => Ok(json!(tracks
                        .iter()
                        .take(100)
                        .enumerate()
                        .map(|(index, track)| json!({ "index": index, "id": track.id, "title": track.title, "artist": track.artist }))
                        .collect::<Vec<_>>())),
                    None => Err("list needs args.name: liked, history, downloads, library or trending".to_string()),
                }
            }
            "play_list" => match (text("name").as_deref(), number("index")) {
                (Some(name), Some(index)) => {
                    let index = index as usize;
                    let size = match name {
                        "liked" => self.liked.len(),
                        "history" => self.history.len(),
                        "downloads" => self.downloads.len(),
                        "library" => self.tracks.len(),
                        "trending" => self.trending.len(),
                        _ => 0,
                    };
                    if index >= size {
                        Err("index is outside that list".to_string())
                    } else {
                        match name {
                            "liked" => self.play_liked(index),
                            "history" => self.play_history(index),
                            "downloads" => self.play_downloads(index),
                            "library" => self.play_library(index),
                            _ => self.play_trending(index),
                        }
                        Ok(json!({ "ok": true }))
                    }
                }
                _ => Err("play_list needs args.name and args.index".to_string()),
            },
            "download" => match number("index").and_then(|index| self.search.results.get(index as usize).cloned()) {
                Some(track) => {
                    self.start_download(track);
                    Ok(json!({ "downloading": self.downloading.len() }))
                }
                None => Err("download needs args.index of an existing search result".to_string()),
            },
            "download_cancel" => match number("id") {
                Some(id) => {
                    self.cancel_download(id as u64);
                    Ok(json!({ "ok": true }))
                }
                None => Err("download_cancel needs args.id (see state or list downloads)".to_string()),
            },
            "download_delete" => match number("id") {
                Some(id) => {
                    self.remove_download(id as u64);
                    Ok(json!({ "ok": true }))
                }
                None => Err("download_delete needs args.id".to_string()),
            },
            "lyrics" => Ok(json!({
                "loading": self.lyrics_loading,
                "error": self.lyrics_error,
                "lines": self.lyrics_lines.iter().take(300).map(|line| json!({ "at": line.at, "text": line.text })).collect::<Vec<_>>(),
            })),
            "lyrics_refresh" => {
                self.lyrics_track = None;
                self.refresh_lyrics();
                Ok(json!({ "started": true }))
            }
            "import_url" => match text("url") {
                Some(url) if !url.trim().is_empty() => {
                    self.import.url = url;
                    self.page = Page::Import;
                    self.import_url();
                    Ok(json!({ "started": true }))
                }
                _ => Err("import_url needs args.url".to_string()),
            },
            "region" => match (text("mode").as_deref(), text("language")) {
                (_, Some(language)) => {
                    self.choose_region_language(&language);
                    Ok(json!({ "language": self.settings.region_language }))
                }
                (Some("off"), _) => {
                    self.set_region_mode(RegionMode::Off);
                    Ok(json!({ "mode": "off" }))
                }
                (Some("auto"), _) => {
                    self.set_region_mode(RegionMode::Auto);
                    Ok(json!({ "mode": "auto" }))
                }
                _ => Err("region needs args.mode (off or auto) or args.language (for example tamil)".to_string()),
            },
            "sleep" => {
                if let Some(minutes) = number("minutes") {
                    self.sleep.arm_minutes(minutes.max(1.0) as u32);
                } else if args.get("end_of_track").and_then(|value| value.as_bool()) == Some(true) {
                    self.sleep.arm_end_of_track();
                } else {
                    self.sleep.clear();
                }
                Ok(json!({ "active": self.sleep.active(), "label": self.sleep.label() }))
            }
            "live" => {
                let file = crate::live::path();
                match args.get("overrides") {
                    Some(overrides) => crate::live::parse(&overrides.to_string()).and_then(|_| {
                        std::fs::write(&file, serde_json::to_string_pretty(overrides).unwrap_or_default()).map_err(|error| error.to_string())
                    }).map(|()| json!({ "written": file.display().to_string() })),
                    None => std::fs::write(&file, crate::live::TEMPLATE).map(|()| json!({ "template": file.display().to_string() })).map_err(|error| error.to_string()),
                }
            }
            "layer_at" => match (number("x"), number("y")) {
                (Some(x), Some(y)) => {
                    let layer = ctx.layer_id_at(egui::pos2(x as f32, y as f32));
                    Ok(json!({ "layer": layer.map(|layer| format!("{:?}", layer.order)) }))
                }
                _ => Err("layer_at needs x and y".to_string()),
            },
            "preview" => {
                self.preview_width = number("width").filter(|width| (200.0..=4000.0).contains(width)).map(|width| width as f32);
                ctx.request_repaint();
                self.preview_height = number("height").filter(|height| (120.0..=4000.0).contains(height)).map(|height| height as f32);
                Ok(json!({ "preview_width": self.preview_width, "preview_height": self.preview_height }))
            }
            "mini_size" => match (number("width"), number("height")) {
                (Some(width), Some(height)) if self.mini.open => {
                    ctx.send_viewport_cmd_to(
                        crate::miniplayer::viewport_id(),
                        egui::ViewportCommand::InnerSize(egui::vec2(width as f32, height as f32)),
                    );
                    Ok(json!({ "requested": [width, height] }))
                }
                (Some(_), Some(_)) => Err("the miniplayer is not open; use panel miniplayer first".to_string()),
                _ => Err("mini_size needs args.width and args.height".to_string()),
            },
            "screenshot" => {
                let target = (text("viewport").as_deref() == Some("mini")).then(crate::miniplayer::viewport_id);
                let reply = call.reply.clone();
                let dir = crate::agent::agent_dir();
                let stamp = web_time::SystemTime::now()
                    .duration_since(web_time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let path = dir.join(format!("shot-{stamp}.png"));
                let shot = egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                    move |image: std::sync::Arc<egui::ColorImage>| {
                        let _ = std::fs::create_dir_all(&dir);
                        let [width, height] = image.size;
                        let saved = image::save_buffer(&path, image.as_raw(), width as u32, height as u32, image::ColorType::Rgba8);
                        let answer = match saved {
                            Ok(()) => json!({ "path": path.display().to_string(), "width": width, "height": height }),
                            Err(error) => json!({ "error": format!("could not save the screenshot: {error}") }),
                        };
                        let _ = reply.send(answer);
                    },
                ));
                match target {
                    Some(id) => ctx.send_viewport_cmd_to(id, shot),
                    None => ctx.send_viewport_cmd(shot),
                }
                ctx.request_repaint();
                return;
            }
            other => Err(format!("unknown command \"{other}\". Try: ping, state, play, pause, toggle, next, prev, seek, volume, like, navigate, panel, search, results, play_result, mood, window, preview, mini_size, logs, diagnose, screenshot")),
        };
        self.agent_reply(call, result);
    }

    fn agent_reply(&mut self, call: crate::agent::Call, result: Result<serde_json::Value, String>) {
        if !matches!(call.cmd.as_str(), "ping" | "audit" | "events") {
            self.agent_audit.push_back((Self::now_ms(), call.cmd.clone(), result.is_ok()));
            while self.agent_audit.len() > 200 {
                self.agent_audit.pop_front();
            }
        }
        let value = match result {
            Ok(value) => value,
            Err(error) => serde_json::json!({ "error": error }),
        };
        let _ = call.reply.send(value);
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
        self.settings.mini.open = self.mini.open;
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
        self.settings.mini.open = false;
        self.touch_settings();
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }

    pub fn persist(&mut self, immediate: bool) {
        if !immediate && self.last_persist.elapsed() < Duration::from_secs(if cfg!(target_arch = "wasm32") { 3 } else { 15 }) {
            return;
        }
        self.last_persist = Instant::now();
        let position_ms = self.resume_position_ms();
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
        if !self.settings_dirty {
            return;
        }
        let wait = Duration::from_millis(600);
        if self.settings_saved_at.elapsed() < wait {
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint_after(wait);
            }
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
        for step in self.sleep.step(self.settings.volume) {
            match step {
                crate::sleep::Step::SetVolume(volume) => self.player.send(Command::SetVolume(volume)),
                crate::sleep::Step::Stop { restore } => {
                    self.player.send(Command::Pause);
                    self.player.send(Command::SetVolume(restore));
                    self.notify("Sleep timer stopped playback");
                }
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

    pub fn set_custom_lyrics(&mut self, body: &str) -> bool {
        let Some(track) = self.current.clone() else {
            return false;
        };
        let lines = crate::lyrics::parse_lyrics(body);
        if lines.is_empty() {
            self.notify("Those lyrics are empty");
            return false;
        }
        let _ = crate::db::save_lyrics(track.id, body);
        self.lyrics_track = Some(track.id);
        self.lyrics_lines = lines;
        self.lyrics_active = None;
        self.lyrics_error = None;
        self.lyrics_loading = false;
        self.lyrics_paste = None;
        self.notify("Lyrics saved for this song");
        true
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
        let secs = handoff_window(self.settings.crossfade, plain_handoff());
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
        if remaining > 1.0 && !self.next_is_ready() {
            return;
        }
        self.crossfade_armed = Some(track.id);
        self.crossfade_arm = true;
        self.play_advance(true);
    }

    #[cfg(target_arch = "wasm32")]
    fn sync_awake(&mut self) {
        let want = settings::wants_awake(self.settings.keep_awake, self.is_playing(), crate::install::youtube_player_playing());
        if want != self.awake_on {
            self.awake_on = want;
            crate::install::keep_awake(want);
        }
    }

    fn count_listening(&mut self) {
        let Some(track) = self.current.clone() else {
            self.listen.tick_now(0, false, None);
            return;
        };
        let playing = self.is_playing();
        if self.listen.tick_now(track.id, playing, self.duration()) {
            crate::db::log_play(&track);
            self.play_ok += 1;
            self.stats_dirty = true;
        }
    }

    fn prefetch_radio(&mut self) {
        if !self.settings.autoplay || self.is_offline() || self.radio.pending || self.audio_state() != AudioState::Playing {
            return;
        }
        if self.queue.has_following(true) || self.settings.repeat != RepeatMode::Off {
            return;
        }
        let Some(id) = self.current.as_ref().map(|track| track.id) else {
            return;
        };
        if self.radio.tried == Some(id) {
            return;
        }
        let Some(seed) = self.radio_seed() else {
            return;
        };
        self.radio.tried = Some(id);
        let video = self.current.as_ref().and_then(|track| track.yt_id());
        let _ = self.jobs.tx.send(Job::Radio { seed, video });
        self.radio.pending = true;
        self.radio.append = true;
    }

    fn remember_position(&mut self) {
        if self.current.is_none() {
            return;
        }
        let snapshot = self.player.snapshot();
        if !matches!(snapshot.state, AudioState::Playing | AudioState::Paused) {
            return;
        }
        self.pending_start_ms = crate::listen::resume_point(snapshot.position.as_millis() as u64, snapshot.duration.map(|d| d.as_millis() as u64));
    }

    pub fn tick(&mut self) {
        #[cfg(target_arch = "wasm32")]
        self.sync_awake();
        self.remember_position();
        self.prefetch_radio();
        self.count_listening();
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
        if self.live_watcher.poll() {
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint();
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
                    if !self.search.matches(&query, provider) {
                        continue;
                    }
                    match self.search.receive(results, error) {
                        crate::search_state::Outcome::Failed(message) if self.search.error.is_some() => {
                            self.fail(message, Some(ToastAction::Search));
                        }
                        crate::search_state::Outcome::Failed(message) => crate::logs::error(&message),
                        crate::search_state::Outcome::Updated => {}
                    }
                }
                Event::Health { findings } => {
                    self.health_running = false;
                    self.agent_findings.retain(|finding| finding.severity != crate::doctor::Severity::Info || finding.detail.is_empty());
                    self.agent_findings.extend(findings);
                }
                Event::Features { track_id, features } => {
                    self.features_checked.insert(track_id);
                    self.features.insert(track_id, features);
                    let _ = crate::db::save_features(track_id, features);
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
                    self.warmup.done(track_id);
                    if let Some((stream, http)) = warmed {
                        self.warmup.store(crate::prefetch::Warmed { track_id, stream, http, at: Instant::now() });
                    }
                }
                Event::Resolved { gen, mut track, result } => {
                    if gen != self.play_gen {
                        continue;
                    }
                    self.resolving = false;
                    if let Err(message) = &result {
                        self.play_err += 1;
                        self.record_agent_event("play_failed", serde_json::json!({ "id": track.id, "title": track.title, "error": message }));
                    }
                    match result {
                        Ok(stream) => {
                            self.skip_streak = 0;
                            self.record_agent_event("track_started", serde_json::json!({ "id": track.id, "title": track.title, "artist": track.artist }));
                            track.provider_meta = Some(ProviderMeta {
                                provider: stream.provider,
                                bitrate_kbps: stream.bitrate_kbps,
                            });
                            let duration = stream.duration.or(track.duration);
                            let analyze = self.features_for(track.id).is_none();
                            self.player.send(Command::Play(Box::new(PlayRequest {
                                track,
                                resolved: Some(stream),
                                start: Duration::ZERO,
                                duration,
                                fade: self.pending_fade,
                                warm: None,
                                analyze,
                            })));
                        }
                        Err(message) if self.skip_on_fail && self.skip_streak < 3 && self.queue.has_following(true) => {
                            self.skip_streak += 1;
                            self.notify(format!("Skipped {}: {message}", track.title));
                            self.skip_on_fail = false;
                            self.play_advance(true);
                        }
                        Err(message) => {
                            self.skip_streak = 0;
                            self.player.send(Command::Stop);
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
                Event::UpdateChecked(result) => {
                    self.update = match result {
                        Ok(release) if crate::update::is_newer(env!("CARGO_PKG_VERSION"), &release.version) => crate::update::State::Available(release),
                        Ok(_) => crate::update::State::UpToDate,
                        Err(reason) => crate::update::State::Failed(reason),
                    };
                    if let Some(ctx) = &self.ctx {
                        ctx.request_repaint();
                    }
                }
                Event::UpdateDownloaded(result) => {
                    self.update = match result {
                        Ok(path) => crate::update::State::Ready(path),
                        Err(reason) => crate::update::State::Failed(reason),
                    };
                    if let Some(ctx) = &self.ctx {
                        ctx.request_repaint();
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
                            self.lyrics_lines = crate::lyrics::parse_lyrics(&body);
                            self.lyrics_error = if self.lyrics_lines.is_empty() {
                                Some("No lyrics found for this track".to_string())
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
                    self.radio.pending = false;
                    if tracks.is_empty() {
                        self.radio.append = false;
                        if let Some(message) = error {
                            crate::logs::error(&format!("Radio for {seed} failed: {message}"));
                        }
                    } else if std::mem::take(&mut self.radio.append) {
                        let current = self.current.clone();
                        let mut seen = std::collections::HashSet::new();
                        let fresh: Vec<Track> = self
                            .rank_for_pace(tracks)
                            .into_iter()
                            .filter(|track| {
                                current.as_ref().is_none_or(|now| now.id != track.id && !now.title.eq_ignore_ascii_case(&track.title))
                                    && seen.insert(track.title.to_lowercase())
                            })
                            .take(15)
                            .collect();
                        for track in fresh {
                            self.queue.push_user(track);
                        }
                        if self.audio_state() == AudioState::Idle && self.queue.has_following(false) {
                            self.play_advance(false);
                        }
                    } else {
                        let tracks = self.rank_for_pace(tracks);
                        let name = self.radio.label.take().unwrap_or_else(|| format!("{seed} radio"));
                        self.queue.set_context(tracks.clone(), name);
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
                Event::AudioError { message } => {
                    self.play_err += 1;
                    self.record_agent_event("audio_error", serde_json::json!({ "message": message }));
                    self.fail(message, None)
                }
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

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "m4a", "m4b", "aac", "wav", "wave", "opus", "mp4", "aif", "aiff", "aifc", "caf", "mka", "mkv", "webm", "mp2", "mp1",
];

fn collect_audio_files(folder: &std::path::Path, depth: usize) -> Vec<PathBuf> {
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
            .map(|ext| AUDIO_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
            .unwrap_or(false);
        if is_audio {
            found.push(path);
        }
    }
    found
}

pub fn resume_position_ms(idle_with_track: bool, pending: Option<u64>, snapshot_ms: u64) -> u64 {
    match (idle_with_track, pending) {
        (true, Some(saved)) => saved,
        _ => snapshot_ms,
    }
}

pub fn transition_secs(setting: f32, manual: bool) -> f32 {
    if setting <= 0.05 {
        0.0
    } else if manual {
        setting.min(1.2)
    } else {
        setting.clamp(0.0, 12.0)
    }
}

pub fn smart_fade(base: f32, from: Option<crate::pace::Features>, to: Option<crate::pace::Features>) -> f32 {
    match (from, to) {
        (Some(a), Some(b)) => {
            let jump = (a.energy - b.energy).abs();
            let tempo = if a.bpm > 0.0 && b.bpm > 0.0 { (a.bpm - b.bpm).abs() / a.bpm.max(b.bpm) } else { 0.0 };
            (base * (1.0 - jump * 1.2 - tempo * 0.5).clamp(0.4, 1.0)).max(1.5_f32.min(base))
        }
        _ => base,
    }
}

fn plain_handoff() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        crate::platform::touch_device() && !crate::platform::volume_fades()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

pub fn handoff_window(crossfade: f32, plain_audio: bool) -> f32 {
    if plain_audio {
        0.3
    } else {
        crossfade
    }
}

pub fn effective_state(audio: AudioState, resolving: bool) -> AudioState {
    if resolving {
        AudioState::Loading
    } else {
        audio
    }
}

impl eframe::App for App {
    fn logic(&mut self, _ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        {
            self.sync_history();
            media_controls::pump();
            if let Some((name, text)) = crate::install::take_picked() {
                let is_csv = name.to_lowercase().ends_with(".csv");
                self.import_text(&text, is_csv, name);
            }
        }
        self.tick();
        self.ensure_media(frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let agent_ctx = ui.ctx().clone();
        self.poll_agent(&agent_ctx);
        #[cfg(target_arch = "wasm32")]
        {
            self.sync_history();
            media_controls::pump();
        }

        if self.busy() || self.audio_state() == AudioState::Loading {
            let interval = if self.settings.battery_saver { 250 } else { 80 };
            ui.ctx().request_repaint_after(Duration::from_millis(interval));
        }
        self.tick();
        self.ensure_media(frame);
        match (self.preview_width, self.preview_height) {
            (None, None) => crate::ui::root(self, ui),
            (width, height) => {
                let mut rect = ui.max_rect();
                if let Some(width) = width {
                    rect.max.x = rect.min.x + width.min(rect.width());
                }
                if let Some(height) = height {
                    rect.max.y = rect.min.y + height.min(rect.height());
                }
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                child.set_clip_rect(rect);
                crate::ui::root(self, &mut child);
            }
        }
        let ctx = ui.ctx().clone();
        crate::miniplayer::show(self, &ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist(true);
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
mod fade_tests {
    use super::*;
    use crate::pace::Features;

    #[test]
    fn similar_songs_keep_the_full_fade_and_big_jumps_shorten_it() {
        let calm = Features { bpm: 90.0, energy: 0.3 };
        let calm2 = Features { bpm: 92.0, energy: 0.32 };
        let loud = Features { bpm: 150.0, energy: 0.9 };
        assert!(smart_fade(6.0, Some(calm), Some(calm2)) > 5.4);
        assert!(smart_fade(6.0, Some(calm), Some(loud)) < 3.0);
        assert_eq!(smart_fade(6.0, None, Some(loud)), 6.0);
        assert!(smart_fade(1.0, Some(calm), Some(loud)) <= 1.0);
    }
}

#[cfg(test)]
mod state_tests {
    #[test]
    fn phones_hand_off_in_the_last_instant_instead_of_cutting_the_song_short() {
        assert_eq!(super::handoff_window(4.0, true), 0.3);
        assert_eq!(super::handoff_window(0.0, true), 0.3);
        assert_eq!(super::handoff_window(4.0, false), 4.0);
        assert_eq!(super::handoff_window(0.0, false), 0.0);
    }

    use super::{effective_state, resume_position_ms, transition_secs};
    use crate::player::AudioState;

    #[test]
    fn a_restored_song_keeps_its_saved_position_until_playback_starts() {
        assert_eq!(resume_position_ms(true, Some(93_000), 0), 93_000);
        assert_eq!(resume_position_ms(false, Some(93_000), 4_000), 4_000);
        assert_eq!(resume_position_ms(true, None, 0), 0);
        assert_eq!(resume_position_ms(true, None, 7_000), 7_000);
    }

    #[test]
    fn manual_skips_get_a_short_fade_and_automatic_ones_the_full_setting() {
        assert_eq!(transition_secs(0.0, false), 0.0);
        assert_eq!(transition_secs(0.0, true), 0.0);
        assert_eq!(transition_secs(4.0, false), 4.0);
        assert_eq!(transition_secs(4.0, true), 1.2);
        assert_eq!(transition_secs(0.8, true), 0.8);
        assert_eq!(transition_secs(30.0, false), 12.0);
    }

    #[test]
    fn resolving_counts_as_loading_so_play_cannot_start_a_second_load() {
        for audio in [AudioState::Idle, AudioState::Playing, AudioState::Paused, AudioState::Loading] {
            assert_eq!(effective_state(audio, true), AudioState::Loading);
            assert_eq!(effective_state(audio, false), audio);
        }
    }
}
