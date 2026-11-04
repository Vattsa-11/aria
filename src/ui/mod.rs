pub mod gestures;
pub mod icons;
pub mod pages;
pub mod phone;
pub mod responsive;
pub mod theme;
pub mod widgets;

use std::path::PathBuf;
use std::time::Duration;

use egui::{
    Align, Align2, Color32, CornerRadius, Frame, Layout, Margin, Rect, RichText, ScrollArea, Sense,
    Stroke, TextStyle, Ui, Vec2,
};

use crate::app::{App, Page};
use crate::model::fmt_optional_time;
use crate::settings::RepeatMode;
use crate::ui::icons::Icon;
use crate::ui::pages::rows::track_row;
use crate::ui::theme::Palette;
use crate::ui::widgets::{
    icon_button, primary_icon_button, seek_bar, toggle_icon_button,
};

fn adaptive_accent(app: &App, ctx: &egui::Context) -> Option<Color32> {
    if !app.settings.adaptive_color {
        return None;
    }
    let base = theme::fixed_accent(&app.settings);
    let goal = app.current.as_ref().and_then(|track| app.thumbs.accent(track.id)).unwrap_or(base);
    let step = |channel: u8, which: u8| ctx.animate_value_with_time(egui::Id::NULL.with(("aria.adaptive-accent", which)), channel as f32, 0.6).round().clamp(0.0, 255.0) as u8;
    Some(Color32::from_rgb(step(goal.r(), 0), step(goal.g(), 1), step(goal.b(), 2)))
}

pub fn root(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    theme::set_adaptive(adaptive_accent(app, &ctx));
    let accent = theme::accent(&app.settings);
    let pal = theme::palette(app.settings.theme);
    let window_width = ui.max_rect().width();
    let lay = responsive::plan(window_width);
    let short_desktop = !lay.phone_bar && ui.max_rect().height() < 420.0;

    app.tick();
    if let Some(track) = gestures::take_queue_request(&ctx) {
        app.add_to_queue(track);
    }
    shortcuts(app, &ctx);
    drop_files(app, &ctx);

    if (lay.phone_bar || short_desktop) && app.now_playing_open {
        widgets::set_loading(app.audio_state() == crate::player::AudioState::Loading);
        phone::now_playing(app, ui, accent, &pal);
        return;
    }
    if lay.phone_bar || short_desktop {
        if lay.phone_bar {
            phone::tab_bar(app, ui, accent, &pal);
        }
        phone::mini_player(app, ui, accent, &pal);
    } else {
        player_bar(app, ui, lay.compact_bar, accent, &pal);
    }

    let sheet_width = responsive::sheet_width(lay.sheet, window_width);
    let lyrics_offset = 0.0;
    lyrics_panel(app, ui, accent, &pal, sheet_width, lyrics_offset);
    let queue_offset = if app.lyrics_open && !matches!(lay.sheet, responsive::Sheet::Fullscreen) { sheet_width } else { 0.0 };
    queue_panel(app, ui, accent, &pal, sheet_width, queue_offset);
    let open_sheets = app.lyrics_open as usize + app.queue_open as usize;
    let reserve = responsive::docked_reserve(lay.sheet, open_sheets);

    match lay.nav {
        responsive::Nav::Side => side_nav(app, ui, accent, &pal),
        responsive::Nav::Rail => rail_nav(app, ui, accent, &pal),
        responsive::Nav::BottomTabs => {}
    }

    widgets::set_loading(app.audio_state() == crate::player::AudioState::Loading);
    central(app, ui, &lay, reserve);
    app.window_width = window_width;
    app.layout_overflow = ctx.globally_used_rect().max.x > window_width + 2.0;

    if app.toast.is_some() {
        toast(&ctx, app, accent, &pal);
    }
    #[cfg(target_arch = "wasm32")]
    update_banner(app, &ctx, accent);

    fix_match_popup(app, &ctx, accent, &pal);
    help_dialog(app, &ctx, accent, &pal);

    if app.is_playing() {
        let interval = if app.settings.battery_saver { 800 } else { 250 };
        ctx.request_repaint_after(Duration::from_millis(interval));
    }
}

fn help_dialog(app: &mut App, ctx: &egui::Context, accent: Color32, pal: &Palette) {
    if !app.help_open {
        return;
    }
    let rows = [
        ("Space", "Play or pause"),
        ("Left / Right", "Seek back or forward 5 seconds"),
        ("Up / Down", "Volume up or down"),
        ("Ctrl+L", "Like or unlike the current track"),
        ("Ctrl+F", "Jump to search"),
        ("Ctrl+M", "Toggle the miniplayer"),
        ("F1 or Shift+/", "Show or hide this help"),
        ("Esc", "Close dialogs"),
    ];
    let mut open = true;
    egui::Window::new("Keyboard shortcuts")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .frame(
            Frame::new()
                .fill(pal.surface)
                .corner_radius(egui::CornerRadius::same(14))
                .inner_margin(Margin::same(18)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            for (keys, what) in rows {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [130.0, 24.0],
                        egui::Label::new(RichText::new(keys).size(13.0).color(accent).monospace()),
                    );
                    ui.label(RichText::new(what).size(13.5).color(pal.text));
                });
                ui.add_space(6.0);
            }
        });
    if !open {
        app.help_open = false;
    }
}

fn drop_files(app: &mut App, ctx: &egui::Context) {
    let dropped: Vec<PathBuf> = ctx.input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect()
    });
    if !dropped.is_empty() {
        app.add_paths(dropped);
    }
}

fn shortcuts(app: &mut App, ctx: &egui::Context) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let (space, left, right, up, down, ctrl_f, ctrl_m, ctrl_l, help) =
        ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Space),
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::F) && input.modifiers.ctrl,
                input.key_pressed(egui::Key::M) && input.modifiers.ctrl,
                input.key_pressed(egui::Key::L) && input.modifiers.ctrl,
                input.key_pressed(egui::Key::F1)
                    || (input.key_pressed(egui::Key::Slash) && input.modifiers.shift),
            )
        });
    if space {
        app.toggle_play();
    }
    if left {
        let target = (app.position() - 5.0).max(0.0);
        app.seek(target);
    }
    if right {
        let target = app.position() + 5.0;
        app.seek(target);
    }
    if up {
        let volume = (app.settings.volume + 0.05).min(1.0);
        app.set_volume(volume);
    }
    if down {
        let volume = (app.settings.volume - 0.05).max(0.0);
        app.set_volume(volume);
    }
    if ctrl_f {
        app.page = Page::Search;
        app.focus_search = true;
    }
    if ctrl_m {
        app.toggle_mini();
    }
    if ctrl_l {
        if let Some(track) = app.current.clone() {
            app.track_menu_for(&track, crate::queue::TrackMenuAction::Like);
        }
    }
    if help {
        app.help_open = !app.help_open;
    }
}

const PLAYER_BAR_WIDE_H: f32 = 92.0;
const PLAYER_BAR_COMPACT_H: f32 = 134.0;
const PLAYER_BAR_PAD: f32 = 6.0;
const PLAYER_BAR_H: f32 = 12.0;

fn player_bar(app: &mut App, ui: &mut Ui, compact: bool, accent: Color32, pal: &Palette) {
    let content_h = if compact {
        PLAYER_BAR_COMPACT_H
    } else {
        PLAYER_BAR_WIDE_H
    };

    egui::Panel::bottom("player_bar")
        .exact_size(content_h + PLAYER_BAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal.surface).inner_margin(Margin {
            left: 18,
            right: 18,
            top: 0,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(PLAYER_BAR_PAD);
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.max_rect().min.y,
                Stroke::new(1.0, pal.line),
            );
            if compact {
                player_bar_compact(app, ui, accent, pal);
            } else {
                player_bar_wide(app, ui, accent, pal);
            }
            ui.add_space(PLAYER_BAR_PAD);
        });
}

fn now_playing_labels(app: &App, ui: &mut Ui, width: f32, height: f32, pal: &Palette) {
    let title = app
        .current
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| "Nothing playing".to_string());
    let loading =
        app.busy() || app.audio_state() == crate::player::AudioState::Loading;
    let artist = if loading {
        "Loading...".to_string()
    } else {
        app.current
            .as_ref()
            .map(|track| track.display_artist().to_string())
            .unwrap_or_else(|| "Add a local file to start".to_string())
    };
    let quality = app.current_quality_badge();

    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    let title_font = egui::FontId::proportional(15.0);
    let galley = painter.layout_no_wrap(title, title_font, pal.text);
    let title_width = galley.size().x;
    let title_y = rect.center().y - 9.0 - galley.size().y * 0.5;
    painter.galley(egui::pos2(rect.min.x, title_y), galley, pal.text);
    if let Some(quality) = quality {
        painter.text(
            egui::pos2(rect.min.x + title_width + 8.0, title_y + 8.0),
            Align2::LEFT_CENTER,
            quality,
            egui::FontId::proportional(10.0),
            pal.muted,
        );
    }
    painter.text(
        egui::pos2(rect.min.x, rect.center().y + 11.0),
        Align2::LEFT_CENTER,
        artist,
        egui::FontId::proportional(12.0),
        pal.muted,
    );
}

fn bar_icon(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    icon: Icon,
    tint: Color32,
    active: bool,
) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let radius = CornerRadius::same(10);
    if active {
        ui.painter()
            .rect_filled(rect, radius, tint.gamma_multiply(0.2));
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, radius, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let size = rect.width().min(rect.height()) * 0.46 * widgets::heart_scale(ui.ctx(), id, icon);
    icons::draw(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(size)),
        icon,
        tint,
    );
    response
}

fn bar_play_button(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    playing: bool,
    accent: Color32,
) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    let hot = response.hovered() || response.is_pointer_button_down_on();
    let _ = accent;
    ui.painter()
        .circle_filled(rect.center(), rect.width() * 0.5 * if hot { 1.06 } else { 1.0 }, Color32::WHITE);
    widgets::paint_play_glyph(ui, rect, if playing { Icon::Pause } else { Icon::Play }, Color32::from_rgb(12, 12, 15));
    response
}

fn bar_seek_bar(
    ui: &mut Ui,
    rect: Rect,
    position: f64,
    duration: f64,
    accent: Color32,
    pal: &Palette,
) -> Option<f64> {
    let response = ui.interact(rect, egui::Id::unique("bar_seek"), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), pal.raised);

    let fraction = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * fraction, track.height()),
    );
    let active = response.hovered() || response.dragged();
    if fraction > 0.0 {
        ui.painter()
            .rect_filled(filled, CornerRadius::same(2), if active { accent } else { Color32::WHITE });
    }
    if duration > 0.0 && active {
        ui.painter().circle_filled(
            egui::pos2(filled.max.x, track.center().y),
            6.5,
            Color32::WHITE,
        );
    }

    if let Some(pointer) = response.interact_pointer_pos() {
        if response.clicked() || response.dragged() {
            let relative = (pointer.x - track.min.x) / track.width().max(1.0);
            return Some((relative.clamp(0.0, 1.0) as f64) * duration);
        }
    }
    None
}

fn bar_volume(
    ui: &mut Ui,
    rect: Rect,
    value: f32,
    accent: Color32,
    pal: &Palette,
) -> Option<f32> {
    let response = ui.interact(rect, egui::Id::unique("bar_volume"), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), pal.raised);
    let value = value.clamp(0.0, 1.0);
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * value, track.height()),
    );
    let active = response.hovered() || response.dragged();
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), if active { accent } else { Color32::WHITE });
    if active {
        ui.painter().circle_filled(
            egui::pos2(filled.max.x, track.center().y),
            6.0,
            Color32::WHITE,
        );
    }

    if let Some(pointer) = response.interact_pointer_pos() {
        if response.clicked() || response.dragged() {
            let relative = (pointer.x - track.min.x) / track.width().max(1.0);
            return Some(relative.clamp(0.0, 1.0));
        }
    }
    None
}

fn player_bar_wide(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    let rect = ui.available_rect_before_wrap();
    let painter = ui.painter().clone();
    let cy = rect.center().y;

    let left_w = (rect.width() * 0.24).clamp(240.0, 360.0);
    let center_left = rect.min.x + left_w;

    let block_w = (rect.width() * 0.40).clamp(300.0, 560.0);
    let block_left = rect.center().x - block_w * 0.5;
    let block_right = rect.center().x + block_w * 0.5;

    let art = 56.0;
    let art_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 2.0, cy - art * 0.5),
        Vec2::splat(art),
    );
    let current_art = app.current_art();
    crate::ui::widgets::paint_cover(&painter, art_rect, current_art.as_ref(), 4, accent, pal);

    let title = app
        .current
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| "Nothing playing".to_string());
    let artist = app
        .current
        .as_ref()
        .map(|track| track.display_artist().to_string())
        .unwrap_or_else(|| "Add a local file to start".to_string());
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(art_rect.max.x + 14.0, rect.min.y),
        egui::pos2(center_left - 62.0, rect.max.y),
    );
    let clip = painter.with_clip_rect(text_rect);
    let title_font = egui::FontId::proportional(14.5);
    let title = crate::miniplayer::fit_text(&title, text_rect.width(), |text| {
        clip.layout_no_wrap(text.to_owned(), title_font.clone(), pal.text).size().x
    });
    let galley = clip.layout_no_wrap(title, title_font, pal.text);
    let title_width = galley.size().x;
    let title_y = cy - 10.0 - galley.size().y * 0.5;
    clip.galley(egui::pos2(text_rect.min.x, title_y), galley, pal.text);
    if let Some(quality) = app.current_quality_badge() {
        clip.text(
            egui::pos2(text_rect.min.x + title_width + 8.0, title_y + 8.0),
            Align2::LEFT_CENTER,
            quality,
            egui::FontId::proportional(10.0),
            pal.muted,
        );
    }
    let artist_font = egui::FontId::proportional(12.0);
    let artist = crate::miniplayer::fit_text(&artist, text_rect.width(), |text| {
        clip.layout_no_wrap(text.to_owned(), artist_font.clone(), pal.muted).size().x
    });
    clip.text(
        egui::pos2(text_rect.min.x, cy + 10.0),
        Align2::LEFT_CENTER,
        artist,
        artist_font,
        pal.muted,
    );

    let controls_w = 44.0 * 4.0 + 50.0 + 10.0 * 4.0;
    let block_top = cy - 39.0;
    let mut x = rect.center().x - controls_w * 0.5;

    let shuffle_on = app.settings.shuffle;
    let shuffle_rect = egui::Rect::from_min_size(egui::pos2(x, block_top), Vec2::splat(44.0));
    if bar_icon(
        ui,
        egui::Id::unique("bar_shuffle"),
        shuffle_rect,
        Icon::Shuffle,
        if shuffle_on { accent } else { pal.muted },
        shuffle_on,
    )
    .clicked()
    {
        app.toggle_shuffle();
    }
    x += 44.0 + 10.0;

    let prev_rect = egui::Rect::from_min_size(egui::pos2(x, block_top), Vec2::splat(44.0));
    if bar_icon(ui, egui::Id::unique("bar_prev"), prev_rect, Icon::Prev, pal.text, false)
        .clicked()
    {
        app.previous();
    }
    x += 44.0 + 10.0;

    let play_rect = egui::Rect::from_center_size(
        egui::pos2(x + 25.0, block_top + 22.0),
        Vec2::splat(50.0),
    );
    if bar_play_button(
        ui,
        egui::Id::unique("bar_play"),
        play_rect,
        app.is_playing(),
        accent,
    )
    .clicked()
    {
        app.toggle_play();
    }
    x += 50.0 + 10.0;

    let next_rect = egui::Rect::from_min_size(egui::pos2(x, block_top), Vec2::splat(44.0));
    if bar_icon(ui, egui::Id::unique("bar_next"), next_rect, Icon::Next, pal.text, false)
        .clicked()
    {
        app.play_advance(false);
    }
    x += 44.0 + 10.0;

    let (repeat_icon, repeat_on) = match app.settings.repeat {
        RepeatMode::Off => (Icon::Repeat, false),
        RepeatMode::All => (Icon::Repeat, true),
        RepeatMode::One => (Icon::RepeatOne, true),
    };
    let repeat_rect = egui::Rect::from_min_size(egui::pos2(x, block_top), Vec2::splat(44.0));
    if bar_icon(
        ui,
        egui::Id::unique("bar_repeat"),
        repeat_rect,
        repeat_icon,
        if repeat_on { accent } else { pal.muted },
        repeat_on,
    )
    .clicked()
    {
        app.cycle_repeat();
    }

    let seek_row_rect = egui::Rect::from_min_max(
        egui::pos2(block_left, block_top + 44.0 + 2.0),
        egui::pos2(block_right, block_top + 44.0 + 2.0 + 32.0),
    );
    let duration = app.duration().unwrap_or(0.0);
    let position = app.position();
    let time_font = egui::FontId::proportional(11.5);
    painter.text(
        egui::pos2(seek_row_rect.min.x, seek_row_rect.center().y),
        Align2::LEFT_CENTER,
        crate::model::fmt_time(position),
        time_font.clone(),
        pal.muted,
    );
    painter.text(
        egui::pos2(seek_row_rect.max.x, seek_row_rect.center().y),
        Align2::RIGHT_CENTER,
        fmt_optional_time(if duration > 0.0 { Some(duration) } else { None }),
        time_font,
        pal.muted,
    );
    let seek_rect = egui::Rect::from_min_max(
        egui::pos2(seek_row_rect.min.x + 46.0, seek_row_rect.min.y),
        egui::pos2(seek_row_rect.max.x - 46.0, seek_row_rect.max.y),
    );
    if let Some(target) = bar_seek_bar(ui, seek_rect, position, duration, accent, pal) {
        app.seek(target);
    }

    let mut right = rect.max.x - 2.0;
    let queue_rect = egui::Rect::from_center_size(egui::pos2(right - 22.0, cy), Vec2::splat(44.0));
    right -= 44.0 + 6.0;
    let lyrics_rect = egui::Rect::from_center_size(egui::pos2(right - 22.0, cy), Vec2::splat(44.0));
    right -= 44.0 + 6.0;
    right -= 8.0;
    let volume_rect = egui::Rect::from_min_max(
        egui::pos2(right - 110.0, cy - 16.0),
        egui::pos2(right, cy + 16.0),
    );
    right -= 110.0 + 6.0;
    let volume_icon_center = egui::pos2(right - 18.0, cy);

    if bar_icon(
        ui,
        egui::Id::unique("bar_lyrics"),
        lyrics_rect,
        Icon::Lyrics,
        if app.lyrics_open { accent } else { pal.muted },
        app.lyrics_open,
    )
    .clicked()
    {
        app.toggle_lyrics();
    }

    if bar_icon(
        ui,
        egui::Id::unique("bar_queue"),
        queue_rect,
        Icon::Queue,
        if app.queue_open { accent } else { pal.muted },
        app.queue_open,
    )
    .clicked()
    {
        app.queue_open = !app.queue_open;
    }

    let liked = app.liked_current();
    let heart_rect = egui::Rect::from_center_size(egui::pos2(center_left - 34.0, cy), Vec2::splat(44.0));
    if bar_icon(
        ui,
        ui.scope_id().with("bar-like"),
        heart_rect,
        if liked { Icon::HeartFilled } else { Icon::Heart },
        if liked { accent } else { pal.muted },
        false,
    )
    .clicked()
    {
        app.toggle_like_current();
    }

    if let Some(value) = bar_volume(ui, volume_rect, app.settings.volume, accent, pal) {
        app.set_volume(value);
    }
    icons::draw(
        &painter,
        Rect::from_center_size(volume_icon_center, Vec2::splat(22.0)),
        Icon::Volume,
        pal.muted,
    );
    let mini_rect = egui::Rect::from_center_size(
        egui::pos2(volume_icon_center.x - 34.0, cy),
        Vec2::splat(40.0),
    );
    if bar_icon(
        ui,
        egui::Id::unique("bar_mini"),
        mini_rect,
        Icon::Mini,
        if app.mini.open { accent } else { pal.muted },
        app.mini.open,
    )
    .clicked()
    {
        app.toggle_mini();
    }
}

fn player_bar_compact(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(width, 40.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            let current_art = app.current_art();
            crate::ui::widgets::cover(ui, 36.0, current_art.as_ref(), accent, pal);
            ui.add_space(10.0);
            now_playing_labels(app, ui, (ui.available_width() - 130.0).max(90.0), 36.0, pal);
            let right = ui.available_width();
            ui.allocate_ui_with_layout(
                Vec2::new(right, 36.0),
                Layout::right_to_left(Align::Center),
                |ui| queue_and_like_buttons(app, ui, accent, pal),
            );
        },
    );

    ui.add_space(2.0);
    seek_row(app, ui, accent, pal);
    ui.add_space(2.0);

    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(width, 50.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            let controls_width = 44.0 * 4.0 + 50.0 + 8.0 * 4.0;
            ui.add_space(((ui.available_width() - controls_width) * 0.5).max(0.0));
            transport_controls(app, ui, accent, pal);
        },
    );
}

fn transport_controls(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    if toggle_icon_button(ui, Icon::Shuffle, 44.0, app.settings.shuffle, accent, pal).clicked() {
        app.toggle_shuffle();
    }
    if icon_button(ui, Icon::Prev, 44.0, pal.text, false).clicked() {
        app.previous();
    }
    let icon = if app.is_playing() {
        Icon::Pause
    } else {
        Icon::Play
    };
    if primary_icon_button(ui, icon, 50.0, accent).clicked() {
        app.toggle_play();
    }
    if icon_button(ui, Icon::Next, 44.0, pal.text, false).clicked() {
        app.play_advance(false);
    }

    let (repeat_icon, repeat_on) = match app.settings.repeat {
        RepeatMode::Off => (Icon::Repeat, false),
        RepeatMode::All => (Icon::Repeat, true),
        RepeatMode::One => (Icon::RepeatOne, true),
    };
    if toggle_icon_button(ui, repeat_icon, 44.0, repeat_on, accent, pal).clicked() {
        app.cycle_repeat();
    }
}

fn queue_and_like_buttons(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    if toggle_icon_button(ui, Icon::Lyrics, 44.0, app.lyrics_open, accent, pal).clicked() {
        app.toggle_lyrics();
    }
    if toggle_icon_button(ui, Icon::Queue, 44.0, app.queue_open, accent, pal).clicked() {
        app.queue_open = !app.queue_open;
    }
    if toggle_icon_button(ui, Icon::Mini, 44.0, app.mini.open, accent, pal).clicked() {
        app.toggle_mini();
    }
    let liked = app.liked_current();
    if icon_button(
        ui,
        if liked {
            Icon::HeartFilled
        } else {
            Icon::Heart
        },
        44.0,
        if liked { accent } else { pal.muted },
        liked,
    )
    .clicked()
    {
        app.toggle_like_current();
    }
}

fn seek_row(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    let duration = app.duration().unwrap_or(0.0);
    let position = app.position();
    let width = ui.available_width();
    ui.allocate_ui_with_layout(
        Vec2::new(width, 40.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.label(
                RichText::new(crate::model::fmt_time(position))
                    .size(12.5)
                    .color(pal.muted),
            );
            let bar_width = (ui.available_width() - 96.0).max(80.0);
            if let Some(target) = seek_bar(ui, bar_width, position, duration, accent, pal) {
                app.seek(target);
            }
            ui.label(
                RichText::new(fmt_optional_time(if duration > 0.0 {
                    Some(duration)
                } else {
                    None
                }))
                .size(12.5)
                .color(pal.muted),
            );
        },
    );
}

fn nav_item(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    accent: Color32,
    pal: &Palette,
    height: f32,
) -> bool {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let radius = CornerRadius::same(12);
    if selected {
        ui.painter()
            .rect_filled(rect, radius, accent.gamma_multiply(0.16));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, radius, pal.raised);
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.min.x + 26.0, rect.center().y),
        Vec2::splat(22.0),
    );
    icons::draw(
        ui.painter(),
        icon_rect,
        icon,
        if selected { accent } else { pal.muted },
    );

    ui.painter().text(
        egui::pos2(rect.min.x + 48.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        TextStyle::Body.resolve(ui.style()),
        if selected { pal.text } else { pal.muted },
    );

    response.clicked()
}

fn side_nav(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    egui::Panel::left("nav_side")
        .exact_size(232.0)
        .resizable(false)
        .frame(Frame::new().fill(pal.bg).inner_margin(Margin {
            left: 14,
            right: 14,
            top: 16,
            bottom: 8,
        }))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("Aria").size(21.0).color(pal.text).strong());
            });
            ui.add_space(16.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() }).show(ui, |ui| {
            if nav_item(
                ui,
                Icon::Home,
                "Home",
                app.page == Page::Home,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Home;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::Library,
                "Library",
                app.page == Page::Library,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Library;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::Search,
                "Search",
                app.page == Page::Search,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Search;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::Queue,
                "Playlists",
                matches!(app.page, Page::Playlists | Page::PlaylistDetail(_)),
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Playlists;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::HeartFilled,
                "Liked Songs",
                app.page == Page::Liked,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Liked;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::Download,
                "Downloads",
                app.page == Page::Downloads,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Downloads;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::Import,
                "Import",
                app.page == Page::Import,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Import;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::Chevron,
                "History",
                app.page == Page::History,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::History;
            }
            ui.add_space(2.0);
            if nav_item(
                ui,
                Icon::Settings,
                "Settings",
                app.page == Page::Settings,
                accent,
                pal,
                48.0,
            ) {
                app.page = Page::Settings;
            }
            });
        });
}

fn sheet(ui: &mut Ui, name: &str, width: f32, offset: f32, pal: &Palette, add: impl FnOnce(&mut Ui)) {
    let avail = ui.available_rect_before_wrap();
    let rect = Rect::from_min_max(
        egui::pos2((avail.max.x - offset - width).max(avail.min.x), avail.min.y),
        egui::pos2(avail.max.x - offset, avail.max.y),
    );
    let area_id = ui.scope_id().with(name);
    egui::Area::new(area_id)
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .constrain(false)
        .show(ui.ctx(), |ui| {
            ui.set_clip_rect(rect);
            ui.painter().rect_filled(rect, CornerRadius::ZERO, pal.surface);
            ui.painter().vline(rect.min.x, rect.y_range(), Stroke::new(1.0, pal.line));
            ui.allocate_rect(rect, Sense::click_and_drag());
            let inner = Rect::from_min_max(
                egui::pos2(rect.min.x + 16.0, rect.min.y + 16.0),
                egui::pos2(rect.max.x - 16.0, rect.max.y - 12.0),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
            child.set_clip_rect(inner);
            add(&mut child);
        });
}

fn rail_nav(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    egui::Panel::left("nav_rail")
        .exact_size(responsive::RAIL_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal.bg).inner_margin(Margin { left: 8, right: 8, top: 12, bottom: 8 }))
        .show(ui, |ui| {
            let items: [(Icon, &str, Page); 10] = [
                (Icon::Home, "Home", Page::Home),
                (Icon::Library, "Library", Page::Library),
                (Icon::Search, "Search", Page::Search),
                (Icon::Queue, "Playlists", Page::Playlists),
                (Icon::HeartFilled, "Liked", Page::Liked),
                (Icon::Download, "Downloads", Page::Downloads),
                (Icon::Import, "Import", Page::Import),
                (Icon::Chevron, "History", Page::History),
                (Icon::Note, "Stats", Page::Stats),
                (Icon::Settings, "Settings", Page::Settings),
            ];
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (icon, label, page) in items {
                    let selected = match (page, app.page) {
                        (Page::Playlists, Page::PlaylistDetail(_)) => true,
                        (a, b) => a == b,
                    };
                    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::click());
                    if selected {
                        ui.painter().rect_filled(rect, CornerRadius::same(12), accent.gamma_multiply(0.16));
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(12), pal.raised);
                    }
                    icons::draw(
                        ui.painter(),
                        Rect::from_center_size(egui::pos2(rect.center().x, rect.min.y + 20.0), Vec2::splat(22.0)),
                        icon,
                        if selected { accent } else { pal.muted },
                    );
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.min.y + 40.0),
                        Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(10.0),
                        if selected { accent } else { pal.muted },
                    );
                    if response.on_hover_text(label).clicked() {
                        app.page = page;
                    }
                    ui.add_space(2.0);
                }
            });
        });
}

fn central(app: &mut App, ui: &mut Ui, lay: &responsive::Plan, reserve: f32) {
    let pal = theme::palette(app.settings.theme);
    let side_margin = lay.page_margin;
    let top_margin = if lay.phone_bar { 8 + phone::top_inset(ui) as i8 } else { 8 };
    egui::CentralPanel::default()
        .frame(Frame::new().fill(pal.bg).inner_margin(Margin {
            left: side_margin,
            right: side_margin,
            top: top_margin,
            bottom: 8,
        }))
        .show(ui, |ui| {
            let mut limited;
            let ui: &mut Ui = if reserve > 0.0 {
                let mut rect = ui.available_rect_before_wrap();
                rect.max.x = (rect.max.x - reserve).max(rect.min.x + 240.0);
                limited = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                limited.set_clip_rect(rect);
                &mut limited
            } else {
                ui
            };
            if app.is_offline() {
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        offline_chip(app, ui, &pal);
                    });
                });
            }
            match app.page {
                Page::Home => pages::home(app, ui),
                Page::Library => pages::library(app, ui),
                Page::Search => pages::search(app, ui),
                Page::Playlists => pages::playlists(app, ui),
                Page::PlaylistDetail(index) => pages::playlists::playlist_detail(app, ui, index),
                Page::Liked => pages::liked(app, ui),
                Page::History => pages::history(app, ui),
                Page::Stats => pages::stats(app, ui),
                Page::Downloads => pages::downloads(app, ui),
                Page::Import => pages::import(app, ui),
                Page::Settings => pages::settings(app, ui),
                Page::More => {
                    let accent = theme::accent(&app.settings);
                    phone::more_page(app, ui, accent, &pal)
                }
            }
        });
}

fn offline_chip(app: &App, ui: &mut Ui, pal: &Palette) {
    let label = if app.settings.offline_mode {
        "Offline mode"
    } else {
        "Offline"
    };
    Frame::new()
        .fill(pal.raised)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin {
            left: 10,
            right: 10,
            top: 3,
            bottom: 3,
        })
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(11.5).color(pal.muted));
        });
}


fn lyrics_panel(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette, width: f32, offset: f32) {
    if !app.lyrics_open {
        return;
    }
    let mut open = true;
    let position = app.position();
    let active = crate::lyrics::active_line(&app.lyrics_lines, position);
    let mut seek_to: Option<f64> = None;

    sheet(ui, "lyrics_sheet", width, offset, pal, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Lyrics").size(19.0).color(pal.text).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, 44.0, pal.muted, false).clicked() {
                        open = false;
                    }
                });
            });
            ui.add_space(6.0);
            let title = app
                .current
                .as_ref()
                .map(|track| track.title.clone())
                .unwrap_or_else(|| "Nothing playing".to_string());
            let lyrics_art = app.current_art();
            ui.horizontal(|ui| {
                widgets::cover(ui, 56.0, lyrics_art.as_ref(), accent, pal);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).size(14.0).color(pal.text).strong());
                    let artist = app.current.as_ref().map(|t| t.display_artist().to_string()).unwrap_or_default();
                    ui.label(RichText::new(artist).size(12.0).color(pal.muted));
                });
            });
            ui.add_space(10.0);

            if app.lyrics_loading {
                ui.label(RichText::new("Looking up lyrics...").size(13.5).color(pal.muted));
                return;
            }
            if let Some(error) = app.lyrics_error.clone() {
                ui.label(RichText::new(error).size(13.5).color(pal.muted));
                ui.add_space(8.0);
                if widgets::accent_button(ui, "Retry", egui::vec2(110.0, 44.0), accent) {
                    app.lyrics_track = None;
                    app.refresh_lyrics();
                }
                return;
            }
            if app.lyrics_lines.is_empty() {
                ui.label(RichText::new("No lyrics yet").size(13.5).color(pal.muted));
                return;
            }

            let scroll = egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() });
            scroll.show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                for (index, line) in app.lyrics_lines.iter().enumerate() {
                    let is_active = active == Some(index);
                    let color = if is_active { accent } else { pal.muted };
                    let size = if is_active { 16.5 } else { 14.5 };
                    let response = ui.add(
                        egui::Label::new(RichText::new(&line.text).size(size).color(color))
                            .sense(egui::Sense::click()),
                    );
                    if is_active && app.lyrics_active != Some(index) {
                        response.scroll_to_me(Some(Align::Center));
                    }
                    if response.clicked() {
                        seek_to = Some(line.at);
                    }
                }
            });
        });

    app.lyrics_active = active;
    if let Some(target) = seek_to {
        app.seek(target);
    }
    if !open {
        app.lyrics_open = false;
    }
}

fn queue_panel(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette, width: f32, offset: f32) {
    if !app.queue_open {
        return;
    }
    let mut open = true;
    let mut adding = false;
    let mut clear = false;
    let user_count = app.queue.user_len();
    let context_name = if app.queue.context_name.is_empty() {
        "context".to_string()
    } else {
        app.queue.context_name.clone()
    };

    sheet(ui, "queue_sheet", width, offset, pal, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Queue").size(19.0).color(pal.text).strong());
                if user_count > 0 {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{user_count} in queue"))
                                .size(12.5)
                                .color(accent),
                        );
                    });
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, 44.0, pal.muted, false).clicked() {
                        open = false;
                    }
                });
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if widgets::pill_button(ui, "Add songs", false, accent, pal) {
                    adding = true;
                }
                if user_count > 0 && widgets::pill_button(ui, "Clear", false, accent, pal) {
                    clear = true;
                }
            });
            ui.add_space(6.0);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
                .show(ui, |ui| {
            widgets::section_label(ui, "Now playing", pal);
            ui.add_space(6.0);
            match &app.current {
                Some(track) => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&track.title).size(14.5).color(accent)).truncate());
                        if let Some(badge) = track.quality_badge() {
                            ui.label(RichText::new(badge).size(10.0).color(pal.muted));
                        }
                    });
                    ui.label(
                        RichText::new(track.display_artist())
                            .size(12.5)
                            .color(pal.muted),
                    );
                }
                None => {
                    ui.label(RichText::new("Nothing playing").size(13.5).color(pal.muted));
                }
            }
            ui.add_space(14.0);

            widgets::section_label(ui, "Next in queue", pal);
            ui.add_space(6.0);

            if user_count == 0 {
                ui.label(RichText::new("Queue is empty").size(13.0).color(pal.muted));
            } else {
                let queued: Vec<crate::model::Track> =
                    app.queue.user.iter().cloned().collect();
                let liked_flags: Vec<bool> =
                    queued.iter().map(|track| app.is_liked(track.id)).collect();
                let (dim_flags, badge_flags): (Vec<bool>, Vec<Option<&'static str>>) = queued
                    .iter()
                    .map(|track| app.row_state(track))
                    .unzip();
                app.queue_drag.total = queued.len();
                let mut jump: Option<usize> = None;
                let mut remove: Option<usize> = None;
                let mut moved: Option<(usize, usize)> = None;
                let mut menu_requests: Vec<(egui::Response, crate::model::Track)> = Vec::new();
                ui.scope(|ui| {
                        for (index, track) in queued.iter().enumerate() {
                            let art = app.thumb(track);
                            let resp = track_row(
                                ui,
                                egui::Id::unique("queue_user"),
                                index,
                                track,
                                art.as_ref(),
                                liked_flags[index],
                                accent,
                                pal,
                                true,
                                true,
                                Some(&mut app.queue_drag),
                                dim_flags[index],
                                badge_flags[index],
                            );
                            if resp.clicked {
                                jump = Some(index);
                            } else if resp.menu.clicked() {
                                menu_requests.push((resp.menu.clone(), track.clone()));
                            } else if let Some(to) = resp.move_to {
                                moved = Some((index, to));
                            } else if resp.up.as_ref().is_some_and(|r| r.clicked()) {
                                moved = index.checked_sub(1).map(|to| (index, to));
                            } else if resp.down.as_ref().is_some_and(|r| r.clicked()) {
                                if index + 1 < queued.len() {
                                    moved = Some((index, index + 1));
                                }
                            } else if let Some(r) = resp.remove {
                                if r.clicked() {
                                    remove = Some(index);
                                }
                            }
                        }
                    });
                if let Some(index) = jump {
                    app.jump_queue_user(index);
                }
                if let Some((from, to)) = moved {
                    app.queue.move_user(from, to);
                    app.persist(true);
                }
                if let Some(index) = remove {
                    app.queue.remove_user(index);
                    app.persist(true);
                }
                crate::ui::pages::rows::row_menus(app, menu_requests, crate::ui::pages::rows::MenuKind::Standard, pal);
            }

            ui.add_space(14.0);
            widgets::section_label(ui, &format!("Next from {context_name}"), pal);
            ui.add_space(6.0);

            let preview = app.queue.context_preview();
            if preview.is_empty() {
                ui.label(RichText::new("Nothing queued").size(13.0).color(pal.muted));
            } else {
                let mut jump = None;
                ui.scope(|ui| {
                        for (index, track) in preview {
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 52.0),
                                Sense::click(),
                            );
                            if response.hovered() {
                                ui.painter()
                                    .rect_filled(rect, CornerRadius::same(10), pal.raised);
                            }
                            let inner = egui::Rect::from_min_max(
                                egui::pos2(rect.min.x + 8.0, rect.min.y + 4.0),
                                egui::pos2(rect.max.x - 8.0, rect.max.y - 4.0),
                            );
                            let title_font = TextStyle::Body.resolve(ui.style());
                            let artist_font = TextStyle::Small.resolve(ui.style());
                            let title = widgets::fit_text_to(ui.painter(), &track.title, &title_font, pal.text, inner.width() - 10.0);
                            let artist = widgets::fit_text_to(ui.painter(), track.display_artist(), &artist_font, pal.muted, inner.width() - 10.0);
                            ui.painter().text(
                                egui::pos2(inner.min.x, inner.center().y - 8.0),
                                Align2::LEFT_CENTER,
                                title,
                                title_font,
                                pal.text,
                            );
                            ui.painter().text(
                                egui::pos2(inner.min.x, inner.center().y + 9.0),
                                Align2::LEFT_CENTER,
                                artist,
                                artist_font,
                                pal.muted,
                            );
                            if response.clicked() {
                                jump = Some(index);
                            }
                        }
                    });
                if let Some(index) = jump {
                    app.jump_queue_context(index);
                }
            }
            });
        });
    if clear {
        app.clear_user_queue();
    }
    if adding {
        app.begin_adding_songs();
    }
    if !open {
        app.queue_open = false;
    }
}

fn fix_match_popup(app: &mut App, ctx: &egui::Context, accent: Color32, pal: &Palette) {
    if app.fix_match.is_none() {
        return;
    }
    let mut open = true;
    let mut picked: Option<usize> = None;
    let mut cancel = false;
    let id = egui::Id::unique("fix_match_popup");
    egui::Window::new("Fix match")
        .id(id)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            let Some(state) = app.fix_match.as_ref() else {
                return;
            };
            ui.label(
                RichText::new(format!(
                    "Current: {} - {}",
                    state.track.artist, state.track.title
                ))
                .size(13.5)
                .color(pal.text),
            );
            ui.add_space(10.0);
            if state.running {
                ui.label(RichText::new("Searching providers...").size(13.0).color(pal.muted));
            } else if let Some(error) = &state.error {
                ui.label(RichText::new(error).size(13.0).color(egui::Color32::from_rgb(244, 114, 114)));
            } else {
                ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (index, alt) in state.alternatives.iter().enumerate() {
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 52.0),
                                Sense::click(),
                            );
                            if response.hovered() {
                                ui.painter()
                                    .rect_filled(rect, CornerRadius::same(10), pal.raised);
                            }
                            let badge = alt.quality_badge().unwrap_or_default();
                            ui.painter().with_clip_rect(rect).text(
                                egui::pos2(rect.min.x + 8.0, rect.center().y - 8.0),
                                Align2::LEFT_CENTER,
                                &alt.title,
                                TextStyle::Body.resolve(ui.style()),
                                pal.text,
                            );
                            ui.painter().with_clip_rect(rect).text(
                                egui::pos2(rect.min.x + 8.0, rect.center().y + 9.0),
                                Align2::LEFT_CENTER,
                                format!("{}  {}", alt.display_artist(), badge),
                                TextStyle::Small.resolve(ui.style()),
                                pal.muted,
                            );
                            if response.clicked() {
                                picked = Some(index);
                            }
                        }
                    });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [120.0, 44.0],
                        egui::Button::new(RichText::new("Cancel").size(14.0).color(pal.text)),
                    )
                    .clicked()
                {
                    cancel = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [140.0, 44.0],
                            egui::Button::new(
                                RichText::new("Use this match").size(14.0).color(accent),
                            ),
                        )
                        .clicked()
                    {
                        picked = Some(usize::MAX);
                    }
                });
            });
        });
    if let Some(index) = picked {
        if index == usize::MAX {
            if let Some(state) = app.fix_match.as_ref() {
                if !state.alternatives.is_empty() {
                    app.fix_match = None;
                    return;
                }
            }
        } else {
            app.apply_fix_match(index);
        }
    }
    if cancel || !open {
        app.fix_match = None;
    }
}

#[cfg(target_arch = "wasm32")]
fn update_banner(app: &App, ctx: &egui::Context, accent: Color32) {
    let dismissed = egui::Id::NULL.with("aria.update-dismissed");
    if ctx.data(|data| data.get_temp::<bool>(dismissed)).unwrap_or(false) || !crate::install::update_ready() {
        return;
    }
    let playing = app.is_playing();
    egui::Area::new(egui::Id::NULL.with("aria.update-banner"))
        .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 10.0 + phone::top_inset_of(ctx)))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 24.0).clamp(200.0, 420.0));
            Frame::new()
                .fill(widgets::mix(accent, Color32::BLACK, 0.25))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("A new version of Aria is ready").size(14.0).color(Color32::WHITE).strong());
                            if playing {
                                ui.label(RichText::new("Music pauses for a moment while it updates").size(12.0).color(Color32::from_white_alpha(200)));
                            }
                        });
                        if ui.add(egui::Button::new(RichText::new("Update").strong().color(accent)).fill(Color32::WHITE).corner_radius(CornerRadius::same(12)).min_size(Vec2::new(78.0, 36.0))).clicked() {
                            crate::install::apply_update();
                        }
                        if ui.add(egui::Button::new(RichText::new("Later").color(Color32::WHITE)).fill(Color32::TRANSPARENT).min_size(Vec2::new(52.0, 36.0))).clicked() {
                            ctx.data_mut(|data| data.insert_temp(dismissed, true));
                        }
                    });
                });
        });
}

fn toast(ctx: &egui::Context, app: &mut App, accent: Color32, pal: &Palette) {
    let Some((message, _)) = app.toast.clone() else {
        return;
    };
    let retryable = app.toast_action.is_some();
    let mut retry = false;
    let open_url = if message.contains("not allowing") {
        app.current.as_ref().and_then(|track| track.yt_id()).map(|id| format!("https://music.youtube.com/watch?v={id}"))
    } else {
        None
    };
    egui::Area::new(app.toast_id)
        .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -(ctx.content_rect().height() * 0.25).clamp(60.0, 150.0)))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let outer = (ctx.content_rect().width() - 24.0).clamp(160.0, 420.0);
            Frame::new()
                .fill(pal.raised)
                .corner_radius(CornerRadius::same(12))
                .stroke(Stroke::new(1.0, pal.line))
                .inner_margin(Margin::symmetric(16, 11))
                .show(ui, |ui| {
                    ui.set_width(outer - 32.0);
                    ui.add(egui::Label::new(RichText::new(message).size(13.5).color(pal.text)).wrap());
                    if retryable || open_url.is_some() {
                        ui.add_space(8.0);
                        ui.horizontal_wrapped(|ui| {
                            if retryable && crate::ui::widgets::pill_button(ui, "Retry", false, accent, pal) {
                                retry = true;
                            }
                            if let Some(url) = &open_url {
                                #[cfg(target_arch = "wasm32")]
                                if let Some(id) = url.rsplit('=').next() {
                                    if crate::ui::widgets::pill_button(ui, "Play in app", true, accent, pal) {
                                        crate::install::call_with("ariaEmbed", id);
                                    }
                                }
                                if crate::ui::widgets::pill_button(ui, "Open in YouTube", false, accent, pal) {
                                    crate::platform::open_url(url);
                                }
                            }
                        });
                    }
                });
        });
    if retry {
        app.run_toast_action();
    }
}
