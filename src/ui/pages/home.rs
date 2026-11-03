use egui::{Color32, RichText, ScrollArea, Sense, Vec2};

use crate::app::{App, Page};
use crate::model::Track;
use crate::ui::icons::Icon;
use crate::ui::theme::{accent, palette, Palette};

static CARD_BITS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0x431c_0000);

fn card_w() -> f32 {
    f32::from_bits(CARD_BITS.load(std::sync::atomic::Ordering::Relaxed))
}

fn adapt_cards(ui: &egui::Ui) {
    let available = ui.available_width();
    let minimum = if available < 460.0 {
        140.0
    } else if available < 800.0 {
        150.0
    } else if available < 1300.0 {
        156.0
    } else {
        172.0
    };
    let width = crate::ui::responsive::card_width(available, minimum, ui.spacing().item_spacing.x);
    CARD_BITS.store(width.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

#[derive(Clone, Copy)]
enum Action {
    OpenLiked,
    OpenDownloads,
    OpenSearch,
    OpenImports,
    OpenPlaylist(usize),
    PlayPlaylist(usize, usize),
    PlayRecent(usize),
    PlayPick(PickSource),
    PlayLibrary(usize),
    PlayTrending(usize),
}

pub fn home(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);
    let mut actions: Vec<Action> = Vec::new();

    ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show(ui, |ui| {
            adapt_cards(ui);
            ui.add_space(10.0);
            ui.label(
                RichText::new(greeting())
                    .size(28.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new(format!(
                    "{} tracks · {} playlists · {} downloaded",
                    app.tracks.len(),
                    app.playlists.len(),
                    app.downloads.len()
                ))
                .size(13.0)
                .color(pal.muted),
            );
            ui.add_space(14.0);

            shortcut_grid(app, ui, &pal, accent, &mut actions);
            trending_section(app, ui, &pal, accent, &mut actions);
            mood_section(app, ui, &pal, accent);

            if !app.history.is_empty() {
                section(ui, "Recently played", &pal);
                let recent: Vec<Track> = app.history.iter().take(12).cloned().collect();
                track_strip(app, ui, &recent, &pal, accent, &mut actions, Action::PlayRecent);
                ui.add_space(10.0);
            }

            if !app.playlists.is_empty() {
                section(ui, "Your playlists", &pal);
                let playlists: Vec<(usize, String, usize, Option<Track>)> = app
                    .playlists
                    .iter()
                    .enumerate()
                    .take(12)
                    .map(|(index, playlist)| {
                        (
                            index,
                            playlist.name.clone(),
                            playlist.tracks.len(),
                            playlist.tracks.first().cloned(),
                        )
                    })
                    .collect();
                ui.horizontal_wrapped(|ui| {
                    for (index, name, count, cover) in playlists {
                        let art = cover.and_then(|track| app.thumb(&track));
                        if card(ui, &name, &format!("{count} tracks"), art.as_ref(), &pal, accent)
                        {
                            actions.push(Action::PlayPlaylist(index, 0));
                        }
                        if ui
                            .add_sized(
                                [card_w(), 24.0],
                                egui::Button::new(
                                    RichText::new("Open").size(11.5).color(accent),
                                ),
                            )
                            .clicked()
                        {
                            actions.push(Action::OpenPlaylist(index));
                        }
                    }
                });
                ui.add_space(10.0);
            }

            let picks = quick_picks(app);
            if !picks.is_empty() {
                section(ui, "Quick picks", &pal);
                ui.horizontal_wrapped(|ui| {
                    for (track, source) in picks {
                        let art = app.thumb(&track);
                        let (clicked, card_response) = pick_card(ui, &track, art.as_ref(), &pal, accent);
                        crate::ui::pages::rows::track_context_menu(app, &card_response, &track, &pal);
                        if clicked {
                            match source {
                                PickSource::Library(index) => actions.push(Action::PlayLibrary(index)),
                                PickSource::Liked(index) => actions.push(Action::PlayPick(PickSource::Liked(index))),
                            }
                        }
                    }
                });
                ui.add_space(10.0);
            }

            if app.tracks.is_empty() && app.history.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Nothing here yet")
                            .size(20.0)
                            .color(pal.text)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Add files or a folder to your library, import a playlist, or search for a song.",
                        )
                        .size(14.0)
                        .color(pal.muted),
                    );
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 320.0).max(0.0) * 0.5);
                        if crate::ui::widgets::accent_button(
                            ui,
                            "Search",
                            Vec2::new(150.0, 44.0),
                            accent,
                        ) {
                            actions.push(Action::OpenSearch);
                        }
                        if crate::ui::widgets::pill_button(
                            ui,
                            "Import a playlist",
                            false,
                            accent,
                            &pal,
                        ) {
                            actions.push(Action::OpenImports);
                        }
                    });
                });
            }

            ui.add_space(24.0);
        });

    for action in actions {
        match action {
            Action::OpenLiked => app.page = Page::Liked,
            Action::OpenDownloads => app.page = Page::Downloads,
            Action::OpenSearch => app.page = Page::Search,
            Action::OpenImports => app.page = Page::Import,
            Action::OpenPlaylist(index) => app.page = Page::PlaylistDetail(index),
            Action::PlayPlaylist(index, track) => app.play_playlist(index, track),
            Action::PlayRecent(index) => app.play_history(index),
            Action::PlayLibrary(index) => app.play_library(index),
            Action::PlayTrending(index) => app.play_trending(index),
            Action::PlayPick(PickSource::Liked(index)) => app.play_liked(index),
            Action::PlayPick(PickSource::Library(index)) => app.play_library(index),
        }
    }
}

#[derive(Clone, Copy)]
enum PickSource {
    Liked(usize),
    Library(usize),
}

fn quick_picks(app: &App) -> Vec<(Track, PickSource)> {
    let mut picks: Vec<(Track, PickSource)> = Vec::new();
    for (index, track) in app.liked.iter().enumerate() {
        if picks.len() >= 8 {
            break;
        }
        picks.push((track.clone(), PickSource::Liked(index)));
    }
    for (index, track) in app.tracks.iter().enumerate() {
        if picks.len() >= 8 {
            break;
        }
        if picks.iter().any(|(picked, _)| picked.id == track.id) {
            continue;
        }
        picks.push((track.clone(), PickSource::Library(index)));
    }
    picks
}

fn greeting() -> &'static str {
    "Good to see you"
}

fn mood_section(app: &mut App, ui: &mut egui::Ui, pal: &Palette, accent: Color32) {
    section(ui, "How are you feeling?", pal);
    ui.horizontal_wrapped(|ui| {
        for mood in crate::pace::Mood::ALL {
            let active = app.mood == Some(mood);
            if crate::ui::widgets::pill_button(ui, mood.label(), active, accent, pal) {
                if active {
                    app.set_mood(None);
                } else {
                    app.start_mood_mix(mood);
                }
            }
        }
        if app.mood.is_some() && crate::ui::widgets::pill_button(ui, "Clear mood", false, accent, pal) {
            app.set_mood(None);
        }
    });
    ui.add_space(4.0);
    let line = match (app.mood, app.pace.is_empty()) {
        (Some(mood), true) => format!("Aiming for {} songs. Aria adapts as you listen.", mood.label().to_lowercase()),
        (Some(mood), false) => format!(
            "Aiming for {} songs, blended with your session pace (about {} BPM).",
            mood.label().to_lowercase(),
            app.pace.bpm.round() as i32
        ),
        (None, false) => format!(
            "Keeping your session pace: about {} BPM, energy {}%.",
            app.pace.bpm.round() as i32,
            (app.pace.energy * 100.0).round() as i32
        ),
        (None, true) => "Pick a mood for a matching mix, or just listen: Aria learns your pace.".to_string(),
    };
    ui.label(RichText::new(line).size(12.5).color(pal.muted));
    ui.add_space(10.0);
}

fn trending_section(
    app: &mut App,
    ui: &mut egui::Ui,
    pal: &Palette,
    accent: Color32,
    actions: &mut Vec<Action>,
) {
    use crate::settings::RegionMode;
    match app.settings.region_mode {
        RegionMode::Off => {}
        RegionMode::Ask => {
            egui::Frame::new()
                .fill(pal.raised)
                .corner_radius(egui::CornerRadius::same(14))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.label(RichText::new("Songs trending near you?").size(18.0).color(pal.text).strong());
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Aria can show what is trending for your state, such as Tamil songs in Tamil Nadu. \"Use my location\" looks up your approximate state from your IP address once, using ipwho.is, and stores only the state and language. Or choose a language yourself with no lookup.",
                        )
                        .size(12.5)
                        .color(pal.muted),
                    );
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if crate::ui::widgets::accent_button(ui, "Use my location", egui::vec2(170.0, 40.0), accent) {
                            app.set_region_mode(RegionMode::Auto);
                        }
                        for (id, label) in [("hindi", "Hindi"), ("tamil", "Tamil"), ("telugu", "Telugu"), ("malayalam", "Malayalam"), ("english", "English")] {
                            if crate::ui::widgets::pill_button(ui, label, false, accent, pal) {
                                app.choose_region_language(id);
                            }
                        }
                        if crate::ui::widgets::pill_button(ui, "Not now", false, accent, pal) {
                            app.set_region_mode(RegionMode::Off);
                        }
                    });
                });
            ui.add_space(10.0);
        }
        RegionMode::Auto | RegionMode::Manual => {
            let language = app.settings.region_language.clone().unwrap_or_default();
            if app.geo_loading {
                section(ui, "Trending near you", pal);
                ui.label(RichText::new("Finding your region...").size(13.0).color(pal.muted));
                ui.add_space(10.0);
                return;
            }
            if language.is_empty() {
                if let Some(error) = app.trending_error.clone() {
                    section(ui, "Trending near you", pal);
                    ui.label(RichText::new(error).size(13.0).color(pal.muted));
                    ui.horizontal_wrapped(|ui| {
                        for (id, label) in [("hindi", "Hindi"), ("tamil", "Tamil"), ("telugu", "Telugu"), ("malayalam", "Malayalam"), ("english", "English")] {
                            if crate::ui::widgets::pill_button(ui, label, false, accent, pal) {
                                app.choose_region_language(id);
                            }
                        }
                    });
                    ui.add_space(10.0);
                }
                return;
            }
            let title = crate::region::trending_title(app.settings.region_label.as_deref(), &language);
            section(ui, &title, pal);
            if app.trending.is_empty() {
                if app.trending_loading {
                    ui.label(RichText::new("Loading trending songs...").size(13.0).color(pal.muted));
                } else if let Some(error) = app.trending_error.clone() {
                    ui.label(RichText::new(format!("Could not load trending songs: {error}")).size(13.0).color(pal.muted));
                    if crate::ui::widgets::pill_button(ui, "Retry", false, accent, pal) {
                        app.refresh_trending();
                    }
                }
            } else {
                let tracks = app.trending.clone();
                track_strip(app, ui, &tracks, pal, accent, actions, Action::PlayTrending);
            }
            ui.add_space(10.0);
        }
    }
}

fn section(ui: &mut egui::Ui, title: &str, pal: &Palette) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).size(18.0).color(pal.text).strong());
    ui.add_space(8.0);
}

fn shortcut_grid(
    app: &mut App,
    ui: &mut egui::Ui,
    pal: &Palette,
    accent: Color32,
    actions: &mut Vec<Action>,
) {
    let entries: [(&str, Icon, Action, String); 4] = [
        (
            "Liked Songs",
            Icon::HeartFilled,
            Action::OpenLiked,
            format!("{} tracks", app.liked.len()),
        ),
        (
            "Downloads",
            Icon::Download,
            Action::OpenDownloads,
            format!("{} offline", app.downloads.len()),
        ),
        (
            "Search",
            Icon::Search,
            Action::OpenSearch,
            "Find something new".to_string(),
        ),
        (
            "Import",
            Icon::Import,
            Action::OpenImports,
            "CSV, list or URL".to_string(),
        ),
    ];

    let width = ui.available_width();
    let tile_width = if width < 460.0 { width } else { ((width - 12.0) * 0.5).clamp(180.0, 340.0) };
    ui.horizontal_wrapped(|ui| {
        for (label, icon, action, subtitle) in entries {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(tile_width, 64.0), Sense::click());
            let fill = if response.hovered() {
                pal.raised
            } else {
                pal.surface
            };
            ui.painter().rect_filled(rect, egui::CornerRadius::same(12), fill);
            let icon_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + 12.0, rect.center().y - 22.0),
                Vec2::splat(44.0),
            );
            ui.painter()
                .rect_filled(icon_rect, egui::CornerRadius::same(8), accent.gamma_multiply(0.18));
            crate::ui::icons::draw(
                ui.painter(),
                egui::Rect::from_center_size(icon_rect.center(), Vec2::splat(22.0)),
                icon,
                accent,
            );
            ui.painter().text(
                egui::pos2(icon_rect.max.x + 12.0, rect.center().y - 12.0),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(14.5),
                pal.text,
            );
            ui.painter().text(
                egui::pos2(icon_rect.max.x + 12.0, rect.center().y + 8.0),
                egui::Align2::LEFT_CENTER,
                subtitle,
                egui::FontId::proportional(11.5),
                pal.muted,
            );
            if response.clicked() {
                actions.push(action);
            }
        }
    });
}

fn track_strip(
    app: &mut App,
    ui: &mut egui::Ui,
    tracks: &[Track],
    pal: &Palette,
    accent: Color32,
    actions: &mut Vec<Action>,
    play: fn(usize) -> Action,
) {
    ui.horizontal_wrapped(|ui| {
        for (index, track) in tracks.iter().enumerate() {
            let texture = app.thumb(track);
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(card_w(), card_w() + 44.0), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect.expand(4.0), egui::CornerRadius::same(12), pal.raised);
            }
            let art_rect =
                egui::Rect::from_min_size(rect.min + Vec2::new(4.0, 4.0), Vec2::splat(card_w() - 8.0));
            paint_art(ui, art_rect, texture, &track.title, pal, accent);
            let clip = ui.painter().with_clip_rect(egui::Rect::from_min_max(
                egui::pos2(rect.min.x + 4.0, art_rect.max.y + 6.0),
                egui::pos2(rect.max.x - 4.0, rect.max.y),
            ));
            clip.text(
                egui::pos2(rect.min.x + 6.0, art_rect.max.y + 8.0),
                egui::Align2::LEFT_TOP,
                &track.title,
                egui::FontId::proportional(13.0),
                pal.text,
            );
            clip.text(
                egui::pos2(rect.min.x + 6.0, art_rect.max.y + 26.0),
                egui::Align2::LEFT_TOP,
                track.display_artist(),
                egui::FontId::proportional(11.0),
                pal.muted,
            );
            crate::ui::pages::rows::track_context_menu(app, &response, track, pal);
            if response.clicked() {
                actions.push(play(index));
            }
        }
    });
}

fn card(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    art: Option<&egui::TextureHandle>,
    pal: &Palette,
    accent: Color32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(card_w(), card_w() + 40.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.expand(4.0), egui::CornerRadius::same(12), pal.raised);
    }
    let art_rect =
        egui::Rect::from_min_size(rect.min + Vec2::new(4.0, 4.0), Vec2::splat(card_w() - 8.0));
    crate::ui::widgets::paint_cover(ui.painter(), art_rect, art, 10, accent, pal);
    let clip = ui.painter().with_clip_rect(egui::Rect::from_min_max(
        egui::pos2(rect.min.x + 4.0, art_rect.max.y + 6.0),
        egui::pos2(rect.max.x - 4.0, rect.max.y),
    ));
    clip.text(
        egui::pos2(rect.min.x + 6.0, art_rect.max.y + 8.0),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(13.0),
        pal.text,
    );
    clip.text(
        egui::pos2(rect.min.x + 6.0, art_rect.max.y + 26.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        egui::FontId::proportional(11.0),
        pal.muted,
    );
    response.clicked()
}

fn pick_card(ui: &mut egui::Ui, track: &Track, art: Option<&egui::TextureHandle>, pal: &Palette, accent: Color32) -> (bool, egui::Response) {
    let width = ((ui.available_width() - 12.0) * 0.5).clamp(220.0, 460.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 60.0), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(10), pal.raised);
    }
    let art_tex = art;
    let art = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 6.0, rect.center().y - 22.0),
        Vec2::splat(44.0),
    );
    crate::ui::widgets::paint_cover(ui.painter(), art, art_tex, 8, accent, pal);
    let clip = ui.painter().with_clip_rect(egui::Rect::from_min_max(
        egui::pos2(art.max.x + 10.0, rect.min.y + 8.0),
        egui::pos2(rect.max.x - 10.0, rect.max.y - 8.0),
    ));
    clip.text(
        egui::pos2(art.max.x + 10.0, rect.center().y - 10.0),
        egui::Align2::LEFT_CENTER,
        &track.title,
        egui::FontId::proportional(13.5),
        pal.text,
    );
    clip.text(
        egui::pos2(art.max.x + 10.0, rect.center().y + 10.0),
        egui::Align2::LEFT_CENTER,
        track.display_artist(),
        egui::FontId::proportional(11.0),
        pal.muted,
    );
    (response.clicked(), response)
}

fn paint_art(
    ui: &egui::Ui,
    rect: egui::Rect,
    texture: Option<egui::TextureHandle>,
    title: &str,
    pal: &Palette,
    accent: Color32,
) {
    crate::ui::widgets::paint_cover(ui.painter(), rect, texture.as_ref(), 10, accent, pal);
    let _ = title;
}
