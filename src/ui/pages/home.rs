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
                RichText::new(counts_line(app.liked.len(), app.tracks.len(), app.playlists.len(), app.downloads.len()))
                .size(13.0)
                .color(pal.muted),
            );
            ui.add_space(14.0);

            shortcut_grid(app, ui, &pal, accent, &mut actions);
            trending_section(app, ui, &pal, accent, &mut actions);
            mood_section(app, ui, &pal, accent);

            if !app.history.is_empty() {
                let recent: Vec<Track> = app.history.iter().take(12).cloned().collect();
                track_strip(app, ui, &recent, &pal, accent, &mut actions, Action::PlayRecent, "Recently played");
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

fn counts_line(liked: usize, local: usize, playlists: usize, downloads: usize) -> String {
    let mut parts = vec![format!("{liked} liked")];
    if local > 0 {
        parts.push(format!("{local} local"));
    }
    parts.push(if playlists == 1 { "1 playlist".to_string() } else { format!("{playlists} playlists") });
    parts.push(format!("{downloads} downloaded"));
    parts.join(" · ")
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
            if app.trending.is_empty() {
                section(ui, &title, pal);
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
                track_strip(app, ui, &tracks, pal, accent, actions, Action::PlayTrending, &title);
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

fn shelf_width(available: f32) -> f32 {
    if available < 460.0 {
        138.0
    } else if available < 800.0 {
        150.0
    } else {
        166.0
    }
}

#[allow(clippy::too_many_arguments)]
fn track_card(
    app: &mut App,
    ui: &mut egui::Ui,
    index: usize,
    track: &Track,
    width: f32,
    pal: &Palette,
    accent: Color32,
    actions: &mut Vec<Action>,
    play: fn(usize) -> Action,
) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, width + 46.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let texture = app.thumb(track);
    if response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect.expand(4.0), egui::CornerRadius::same(12), pal.raised);
    } else if response.hovered() {
        ui.painter().rect_filled(rect.expand(4.0), egui::CornerRadius::same(12), pal.raised.gamma_multiply(0.7));
    }
    let art_rect = egui::Rect::from_min_size(rect.min + Vec2::new(4.0, 4.0), Vec2::splat(width - 8.0));
    paint_art(ui, art_rect, texture, &track.title, pal, accent);
    let title_font = egui::FontId::proportional(13.0);
    let artist_font = egui::FontId::proportional(11.5);
    let room = width - 10.0;
    let title = crate::ui::widgets::fit_text_to(ui.painter(), &track.title, &title_font, pal.text, room);
    ui.painter().text(egui::pos2(rect.min.x + 6.0, art_rect.max.y + 8.0), egui::Align2::LEFT_TOP, title, title_font, pal.text);
    let artist = crate::ui::widgets::fit_text_to(ui.painter(), track.display_artist(), &artist_font, pal.muted, room);
    ui.painter().text(egui::pos2(rect.min.x + 6.0, art_rect.max.y + 27.0), egui::Align2::LEFT_TOP, artist, artist_font, pal.muted);
    crate::ui::pages::rows::track_context_menu(app, &response, track, pal);
    if response.clicked() {
        actions.push(play(index));
    }
}

#[allow(clippy::too_many_arguments)]
fn track_strip(
    app: &mut App,
    ui: &mut egui::Ui,
    tracks: &[Track],
    pal: &Palette,
    accent: Color32,
    actions: &mut Vec<Action>,
    play: fn(usize) -> Action,
    title: &str,
) {
    let key = egui::Id::NULL.with(("aria.shelf", title));
    let mut expanded = ui.ctx().data(|data| data.get_temp::<bool>(key)).unwrap_or(false);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(18.0).color(pal.text).strong());
        if tracks.len() > 3 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if expanded { "Show less" } else { "Show all" };
                if ui.add(egui::Button::new(RichText::new(label).size(13.0).color(pal.muted)).fill(Color32::TRANSPARENT)).clicked() {
                    expanded = !expanded;
                }
            });
        }
    });
    ui.add_space(8.0);
    for track in tracks.iter().rev() {
        let _ = app.thumb(track);
    }
    if expanded {
        ui.horizontal_wrapped(|ui| {
            for (index, track) in tracks.iter().enumerate() {
                track_card(app, ui, index, track, card_w(), pal, accent, actions, play);
            }
        });
    } else {
        let width = shelf_width(ui.available_width());
        let ctx = ui.ctx().clone();
        let offset_key = key.with("x");
        let rect_key = key.with("rect");
        let mut offset = ctx.data(|data| data.get_temp::<f32>(offset_key)).unwrap_or(0.0);
        if let Some(rect) = ctx.data(|data| data.get_temp::<egui::Rect>(rect_key)) {
            offset -= crate::ui::gestures::horizontal_drag(&ctx, rect, key.with("drag"));
        }
        let output = egui::ScrollArea::horizontal()
            .id_salt(key)
            .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Never, ..Default::default() })
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .auto_shrink([false, true])
            .horizontal_scroll_offset(offset)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    for (index, track) in tracks.iter().enumerate() {
                        track_card(app, ui, index, track, width, pal, accent, actions, play);
                    }
                });
            });
        let most = (output.content_size.x - output.inner_rect.width()).max(0.0);
        ctx.data_mut(|data| {
            data.insert_temp(offset_key, output.state.clamped_offset().x.clamp(0.0, most));
            data.insert_temp(rect_key, output.inner_rect);
        });
    }
    ui.ctx().data_mut(|data| data.insert_temp(key, expanded));
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

#[cfg(test)]
mod counts_tests {
    use super::counts_line;

    #[test]
    fn the_summary_counts_what_people_actually_collect() {
        assert_eq!(counts_line(0, 0, 0, 0), "0 liked · 0 playlists · 0 downloaded");
        assert_eq!(counts_line(3, 0, 1, 2), "3 liked · 1 playlist · 2 downloaded");
        assert_eq!(counts_line(1, 40, 2, 0), "1 liked · 40 local · 2 playlists · 0 downloaded");
    }
}
