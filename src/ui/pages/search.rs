use std::time::Duration;

use egui::{Align2, Rect, Response, RichText, Sense, TextStyle, Ui, Vec2};

use crate::app::App;
use crate::model::fmt_time;
use crate::ui::icons::Icon;
use crate::ui::theme::{accent, palette, Palette};
use crate::ui::widgets::pill_button;

fn thumb_or_placeholder(ui: &mut Ui, rect: Rect, texture: Option<egui::TextureHandle>, pal: &Palette, accent: egui::Color32) {
    crate::ui::widgets::paint_cover(ui.painter(), rect, texture.as_ref(), 8, accent, pal);
}

struct Row {
    index: usize,
    clicked: bool,
    menu: Response,
}

struct ResultRow<'a> {
    width: f32,
    title: &'a str,
    artist: &'a str,
    duration: Option<Duration>,
    texture: Option<egui::TextureHandle>,
    playing: bool,
    liked: bool,
    quality: Option<String>,
    accent: egui::Color32,
    pal: &'a Palette,
    row_height: f32,
}

fn result_row(ui: &mut Ui, index: usize, data: ResultRow<'_>) -> Row {
    let ResultRow {
        width,
        title,
        artist,
        duration,
        texture,
        playing,
        liked,
        quality,
        accent,
        pal,
        row_height,
    } = data;
    let width = width.min(ui.available_width());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, row_height), Sense::click());

    if playing {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(12), accent.gamma_multiply(0.14));
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(12), pal.surface);
    }

    let art_size = row_height - 20.0;
    let art_rect = Rect::from_min_size(
        egui::pos2(rect.min.x + 10.0, rect.center().y - art_size * 0.5),
        Vec2::splat(art_size),
    );
    thumb_or_placeholder(ui, art_rect, texture, pal, accent);

    let menu_rect = Rect::from_min_size(
        egui::pos2(rect.max.x - 48.0, rect.center().y - 22.0),
        Vec2::splat(44.0),
    );
    let menu_response = ui.interact(
        menu_rect,
        ui.scope_id().with(("row-menu", index)),
        Sense::click(),
    );
    crate::ui::pages::rows::open_menu_on_right_click(ui.ctx(), &response, &menu_response);
    icons_draw_menu(ui.painter(), menu_rect, &menu_response, pal);

    let like_rect = Rect::from_min_size(
        egui::pos2(rect.max.x - 92.0, rect.center().y - 22.0),
        Vec2::splat(44.0),
    );
    let like_response = ui.interact(
        like_rect,
        ui.scope_id().with(("row-like", index)),
        Sense::click(),
    );
    let heart_icon = if liked { Icon::HeartFilled } else { Icon::Heart };
    let heart_pop = crate::ui::widgets::heart_scale(ui.ctx(), like_response.id, heart_icon);
    crate::ui::icons::draw(
        ui.painter(),
        Rect::from_center_size(like_rect.center(), Vec2::splat(22.0 * heart_pop)),
        heart_icon,
        if liked { accent } else { pal.muted },
    );

    let duration_width = if duration.is_some() { 48.0 } else { 0.0 };
    let duration_right = like_rect.min.x - 6.0;
    let badge_width = quality
        .as_ref()
        .map(|badge| badge.chars().count() as f32 * 7.0 + 12.0)
        .unwrap_or(0.0);
    let badge_right = duration_right - duration_width - if duration_width > 0.0 { 8.0 } else { 0.0 };
    let text_left = art_rect.max.x + 14.0;
    let text_right = (badge_right - badge_width - if badge_width > 0.0 { 12.0 } else { 0.0 }).max(text_left + 40.0);
    let title_rect = Rect::from_min_max(
        egui::pos2(text_left, rect.min.y + 6.0),
        egui::pos2(text_right, rect.center().y - 1.0),
    );
    let artist_rect = Rect::from_min_max(
        egui::pos2(text_left, rect.center().y + 1.0),
        egui::pos2(text_right, rect.max.y - 6.0),
    );
    let body_font = TextStyle::Body.resolve(ui.style());
    let small_font = TextStyle::Small.resolve(ui.style());

    ui.painter().with_clip_rect(title_rect).text(
        egui::pos2(title_rect.min.x, title_rect.center().y),
        Align2::LEFT_CENTER,
        title,
        body_font,
        if playing { accent } else { pal.text },
    );
    ui.painter().with_clip_rect(artist_rect).text(
        egui::pos2(artist_rect.min.x, artist_rect.center().y),
        Align2::LEFT_CENTER,
        artist,
        small_font.clone(),
        pal.muted,
    );

    if let Some(duration) = duration {
        ui.painter().text(
            egui::pos2(duration_right, rect.center().y),
            Align2::RIGHT_CENTER,
            fmt_time(duration.as_secs_f64()),
            small_font.clone(),
            pal.muted,
        );
    }

    if let Some(quality) = quality {
        let badge_rect = Rect::from_center_size(
            egui::pos2(badge_right - badge_width * 0.5, rect.center().y),
            Vec2::new(badge_width, 18.0),
        );
        ui.painter().rect_filled(badge_rect, egui::CornerRadius::same(4), pal.raised);
        ui.painter().text(
            badge_rect.center(),
            Align2::CENTER_CENTER,
            &quality,
            small_font.clone(),
            accent,
        );
    }

    Row {
        index,
        clicked: response.clicked() && !menu_response.clicked() && !like_response.clicked(),
        menu: menu_response,
    }
}

fn icons_draw_menu(
    painter: &egui::Painter,
    rect: Rect,
    response: &Response,
    pal: &Palette,
) {
    let color = if response.hovered() { pal.text } else { pal.muted };
    if response.hovered() || response.is_pointer_button_down_on() {
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(12),
            pal.raised,
        );
    }
    crate::ui::icons::draw(
        painter,
        Rect::from_center_size(rect.center(), Vec2::splat(22.0)),
        Icon::More,
        color,
    );
}

pub fn search(app: &mut App, ui: &mut Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    let short = ui.available_height() < 380.0;
    ui.add_space(if short { 4.0 } else { 10.0 });
    if !short {
        ui.label(RichText::new("Search").size(28.0).color(pal.text).strong());
        ui.add_space(10.0);
    }

    let search_id = egui::Id::unique("aria.search-input");
    if app.focus_search {
        app.focus_search = false;
        ui.ctx().memory_mut(|memory| memory.request_focus(search_id));
    }

    ui.horizontal(|ui| {

        let field_width = (ui.available_width() - 104.0).max(120.0);
        let response = ui.add(
            egui::TextEdit::singleline(&mut app.search.query)
                .id(search_id)
                .hint_text("Search songs...")
                .desired_width(field_width)
                .font(TextStyle::Body),
        );
        let submitted =
            response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if submitted {
            app.run_search();
        }
        ui.add_space(4.0);
        let label = if app.search.running {
            "Searching..."
        } else {
            "Search"
        };
        if pill_button(ui, label, app.search.running, accent, &pal) {
            app.run_search();
        }
    });

    ui.add_space(8.0);
    if !short {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let sources: [(&str, Option<crate::provider::ProviderName>); 4] = [
            ("Auto", None),
            ("YT Music", Some(crate::provider::ProviderName::YoutubeMusic)),
            ("JioSaavn", Some(crate::provider::ProviderName::Jiosaavn)),
            ("YouTube", Some(crate::provider::ProviderName::Youtube)),
        ];
        let mut picked: Option<Option<crate::provider::ProviderName>> = None;
        for (label, provider) in sources {
            let active = app.search.provider == provider;
            if pill_button(ui, label, active, accent, &pal) {
                picked = Some(provider);
            }
        }
        if let Some(provider) = picked {
            app.set_search_source(provider);
        }
    });
    }

    ui.add_space(4.0);
    if let Some((message, ready)) = app.ytdlp_status.clone() {
        if !ready {
            ui.label(RichText::new(message).size(12.5).color(pal.muted));
        }
    }
    if let Some(error) = &app.search.error {
        ui.label(RichText::new(error).size(13.0).color(egui::Color32::from_rgb(244, 114, 114)));
    }
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    if app.search.started && app.search.results.is_empty() && !app.search.running {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                RichText::new(if app.search.error.is_some() {
                    "Search failed"
                } else {
                    "No results"
                })
                .size(16.0)
                .color(pal.muted),
            );
        });
        return;
    }
    if app.search.results.is_empty() {
        suggestions(app, ui, accent, &pal);
        return;
    }

    let row_height = 64.0;
    let total = app.search.results.len();

    let row_width = ui.available_width().max(200.0);
    let mut play: Option<usize> = None;
    let mut rows: Vec<Row> = Vec::new();
    let mut visible = 0..0;

    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show_rows(ui, row_height, total, |ui, range| {
            ui.set_width(ui.available_width());
            visible = range.clone();
            for index in range {
                let track = app.search.results[index].clone();
                let texture = app.thumb(&track);
                let playing =
                    app.current.as_ref().map(|t| t.id) == Some(track.id);
                let liked = app.is_liked(track.id);
                let row = result_row(
                    ui,
                    index,
                    ResultRow {
                        width: row_width,
                        title: &track.title,
                        artist: track.display_artist(),
                        duration: track.duration,
                        texture,
                        playing,
                        liked,
                        quality: track.quality_badge(),
                        accent,
                        pal: &pal,
                        row_height,
                    },
                );
                if row.clicked {
                    play = Some(index);
                }
                rows.push(row);
            }
        });

    let window: Vec<crate::model::Track> = app.search.results
        [visible.start.saturating_sub(6)..(visible.end + 14).min(total)]
        .to_vec();
    app.prefetch_thumbs(&window);

    let menu_rows: Vec<(Response, crate::model::Track)> = rows
        .into_iter()
        .filter_map(|row| {
            app.search.results.get(row.index).cloned().map(|track| (row.menu, track))
        })
        .collect();
    crate::ui::pages::rows::row_menus(app, menu_rows, crate::ui::pages::rows::MenuKind::Standard, &pal);

    if let Some(index) = play {
        app.play_search_result(index);
    }
}

fn chip_row(ui: &mut Ui, labels: &[String], accent: egui::Color32, pal: &Palette) -> Option<String> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for label in labels {
            if pill_button(ui, label, false, accent, pal) {
                picked = Some(label.clone());
            }
        }
    });
    picked
}

fn heading(ui: &mut Ui, text: &str, pal: &Palette) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).size(16.0).color(pal.text).strong());
    ui.add_space(6.0);
}

fn suggestions(app: &mut App, ui: &mut Ui, accent: egui::Color32, pal: &Palette) {
    use crate::settings::RegionMode;
    let mut run: Option<String> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !app.settings.recent_searches.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Recent searches").size(16.0).color(pal.text).strong());
                    if pill_button(ui, "Clear", false, accent, pal) {
                        app.settings.recent_searches.clear();
                        app.touch_settings();
                    }
                });
                ui.add_space(6.0);
                let recent = app.settings.recent_searches.clone();
                if let Some(query) = chip_row(ui, &recent, accent, pal) {
                    run = Some(query);
                }
            }

            let artists = crate::suggest::top_artists(&[&app.history, &app.liked], 4);
            if !artists.is_empty() {
                heading(ui, "Based on your listening", pal);
                if let Some(query) = chip_row(ui, &artists, accent, pal) {
                    run = Some(query);
                }
            }

            let language = app.settings.region_language.clone();
            let label = language.as_deref().map(crate::region::language_label);
            heading(ui, "Quick starts", pal);
            if let Some(query) = chip_row(ui, &crate::suggest::quick_starts(label), accent, pal) {
                run = Some(query);
            }

            match app.settings.region_mode {
                RegionMode::Auto | RegionMode::Manual if !app.trending.is_empty() => {
                    let title = crate::region::trending_title(
                        app.settings.region_label.as_deref(),
                        language.as_deref().unwrap_or(""),
                    );
                    heading(ui, &title, pal);
                    let tracks: Vec<crate::model::Track> = app.trending.iter().take(12).cloned().collect();
                    let salt = egui::Id::unique("search_suggestions");
                    let mut play: Option<usize> = None;
                    let mut rows: Vec<(Response, crate::model::Track)> = Vec::new();
                    for (index, track) in tracks.iter().enumerate() {
                        let art = app.thumb(track);
                        let liked = app.is_liked(track.id);
                        let resp = crate::ui::pages::rows::track_row(
                            ui, salt, index, track, art.as_ref(), liked, accent, pal, false, false, None, false, None,
                        );
                        if resp.clicked {
                            play = Some(index);
                        } else if resp.like.clicked() {
                            app.toggle_like_track(track);
                        }
                        rows.push((resp.menu, track.clone()));
                    }
                    crate::ui::pages::rows::row_menus(app, rows, crate::ui::pages::rows::MenuKind::Standard, pal);
                    if let Some(index) = play {
                        app.play_trending(index);
                    }
                }
                RegionMode::Ask | RegionMode::Off => {
                    heading(ui, "Trending near you", pal);
                    ui.label(
                        RichText::new("Turn on regional suggestions to see songs trending in your state.")
                            .size(13.0)
                            .color(pal.muted),
                    );
                    ui.add_space(6.0);
                    if pill_button(ui, "Show trending for my region", false, accent, pal) {
                        app.set_region_mode(RegionMode::Auto);
                    }
                }
                _ => {
                    if app.trending_loading || app.geo_loading {
                        heading(ui, "Trending near you", pal);
                        ui.label(RichText::new("Loading...").size(13.0).color(pal.muted));
                    }
                }
            }
            ui.add_space(20.0);
        });
    if let Some(query) = run {
        app.search.query = query;
        app.run_search();
    }
}
