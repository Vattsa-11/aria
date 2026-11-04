use egui::{Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::{App, Page};
use crate::player::AudioState;
use crate::settings::RepeatMode;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::Palette;
use crate::ui::widgets::{self, icon_button, primary_icon_button, toggle_icon_button};

const TAB_H: f32 = 60.0;
const MINI_H: f32 = 72.0;

pub fn tab_height(window_height: f32) -> f32 {
    if window_height >= 480.0 {
        TAB_H
    } else if window_height >= 340.0 {
        48.0
    } else {
        40.0
    }
}

pub fn mini_height(window_height: f32) -> f32 {
    if window_height >= 480.0 {
        MINI_H
    } else if window_height >= 340.0 {
        58.0
    } else {
        48.0
    }
}

#[cfg(test)]
mod height_tests {
    use super::*;

    #[test]
    fn bars_shrink_as_the_window_gets_shorter_and_never_eat_it_whole() {
        for height in [200.0, 260.0, 339.0, 340.0, 479.0, 480.0, 900.0] {
            assert!(tab_height(height) + mini_height(height) <= height * 0.5, "{height}");
        }
        assert!(tab_height(300.0) < tab_height(500.0) && mini_height(300.0) < mini_height(500.0));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Search,
    Library,
    Liked,
    More,
}

pub const TABS: [(Icon, &str, Tab); 5] = [
    (Icon::Home, "Home", Tab::Home),
    (Icon::Search, "Search", Tab::Search),
    (Icon::Library, "Library", Tab::Library),
    (Icon::HeartFilled, "Liked", Tab::Liked),
    (Icon::More, "More", Tab::More),
];

pub fn tab_for(page: Page) -> Tab {
    match page {
        Page::Home => Tab::Home,
        Page::Search => Tab::Search,
        Page::Library => Tab::Library,
        Page::Liked => Tab::Liked,
        _ => Tab::More,
    }
}

pub fn page_for(tab: Tab) -> Page {
    match tab {
        Tab::Home => Page::Home,
        Tab::Search => Page::Search,
        Tab::Library => Page::Library,
        Tab::Liked => Page::Liked,
        Tab::More => Page::More,
    }
}

pub const MORE_ITEMS: [(Icon, &str, Page); 6] = [
    (Icon::Queue, "Playlists", Page::Playlists),
    (Icon::Download, "Downloads", Page::Downloads),
    (Icon::Import, "Import", Page::Import),
    (Icon::Chevron, "History", Page::History),
    (Icon::Note, "Stats", Page::Stats),
    (Icon::Settings, "Settings", Page::Settings),
];

fn bottom_inset(ui: &Ui) -> f32 {
    ui.ctx().input(|input| input.safe_area_insets()).0.bottom.max(0.0)
}

pub fn top_inset(ui: &Ui) -> f32 {
    ui.ctx().input(|input| input.safe_area_insets()).0.top.max(0.0)
}

pub fn tab_bar(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    let inset = bottom_inset(ui);
    let tab_h = tab_height(ui.max_rect().height());
    let icons_only = tab_h < 56.0;
    egui::Panel::bottom("phone_tabs")
        .exact_size(tab_h + inset)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal.surface))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(rect.x_range(), rect.min.y, Stroke::new(1.0, pal.line));
            let cell_w = rect.width() / TABS.len() as f32;
            let current = tab_for(app.page);
            for (index, (icon, label, tab)) in TABS.iter().enumerate() {
                let cell = Rect::from_min_size(
                    egui::pos2(rect.min.x + index as f32 * cell_w, rect.min.y),
                    Vec2::new(cell_w, tab_h),
                );
                let response = ui.interact(cell, ui.scope_id().with(("phone-tab", index)), Sense::click());
                let selected = current == *tab;
                let color = if selected { accent } else { pal.muted };
                let icon_y = if icons_only { cell.center().y } else { cell.min.y + 21.0 };
                icons::draw(
                    ui.painter(),
                    Rect::from_center_size(egui::pos2(cell.center().x, icon_y), Vec2::splat(if icons_only { 22.0 } else { 24.0 })),
                    *icon,
                    color,
                );
                if !icons_only {
                    ui.painter().text(
                        egui::pos2(cell.center().x, cell.min.y + 44.0),
                        egui::Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(11.5),
                        color,
                    );
                }
                if response.clicked() {
                    app.page = page_for(*tab);
                }
            }
        });
}

pub fn mini_player(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    let Some(track) = app.current.clone() else {
        return;
    };
    let mini_h = mini_height(ui.max_rect().height());
    let dense = mini_h < 56.0;
    egui::Panel::bottom("phone_mini")
        .exact_size(mini_h)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(pal.bg).inner_margin(Margin { left: 10, right: 10, top: 6, bottom: 6 }))
        .show(ui, |ui| {
            let card = ui.max_rect();
            let tint = app.thumbs.tint(track.id);
            let fill = match tint {
                Some(tint) if pal.bg.r() < 80 => widgets::mix(pal.raised, tint, 0.9),
                _ => pal.raised,
            };
            ui.painter().rect_filled(card, CornerRadius::same(14), fill);
            let open = ui.interact(card, ui.scope_id().with("phone-mini-open"), Sense::click_and_drag());
            let (pull, swiped) = crate::ui::gestures::swipe(ui, &open);
            match swiped {
                Some(crate::ui::gestures::Swipe::Left) => app.play_advance(false),
                Some(crate::ui::gestures::Swipe::Right) => app.previous(),
                Some(crate::ui::gestures::Swipe::Up) => app.now_playing_open = true,
                _ => {}
            }
            let card = card.translate(Vec2::new((pull.x * 0.5).clamp(-60.0, 60.0), 0.0));
            let art_side = (card.height() - 12.0).clamp(28.0, 48.0);
            let art_rect = Rect::from_min_size(
                egui::pos2(card.min.x + 8.0, card.center().y - art_side / 2.0),
                Vec2::splat(art_side),
            );
            let art = app.current_art();
            widgets::paint_cover(ui.painter(), art_rect, art.as_ref(), 10, accent, pal);

            let play_rect = Rect::from_center_size(egui::pos2(card.max.x - 30.0, card.center().y), Vec2::splat(44.0));
            let heart_rect = Rect::from_center_size(egui::pos2(play_rect.min.x - 26.0, card.center().y), Vec2::splat(48.0));
            let text_rect = Rect::from_min_max(
                egui::pos2(art_rect.max.x + 12.0, card.min.y + if dense { 0.0 } else { 8.0 }),
                egui::pos2(heart_rect.min.x - 4.0, card.max.y - if dense { 0.0 } else { 12.0 }),
            );
            let painter = ui.painter().with_clip_rect(text_rect);
            let title_font = FontId::proportional(14.5);
            let fitted = crate::miniplayer::fit_text(&track.title, text_rect.width(), |text| {
                painter.layout_no_wrap(text.to_owned(), title_font.clone(), pal.text).size().x
            });
            painter.text(egui::pos2(text_rect.min.x, if dense { text_rect.center().y } else { text_rect.min.y + 8.0 }), egui::Align2::LEFT_CENTER, fitted, title_font, pal.text);
            let artist_font = FontId::proportional(12.0);
            let artist = crate::miniplayer::fit_text(track.display_artist(), text_rect.width(), |text| {
                painter.layout_no_wrap(text.to_owned(), artist_font.clone(), pal.muted).size().x
            });
            if !dense {
                painter.text(egui::pos2(text_rect.min.x, text_rect.min.y + 27.0), egui::Align2::LEFT_CENTER, artist, artist_font, pal.muted);
            }

            let liked = app.liked_current();
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(heart_rect));
            if icon_button(&mut child, if liked { Icon::HeartFilled } else { Icon::Heart }, 44.0, if liked { accent } else { pal.muted }, liked).clicked() {
                app.toggle_like_current();
            }
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(play_rect));
            let icon = if app.is_playing() { Icon::Pause } else { Icon::Play };
            if primary_icon_button(&mut child, icon, 40.0, accent).clicked() {
                app.toggle_play();
            }

            let duration = app.duration().unwrap_or(0.0);
            if duration > 0.0 {
                let fraction = (app.position() / duration).clamp(0.0, 1.0) as f32;
                let line = Rect::from_min_size(egui::pos2(card.min.x + 12.0, card.max.y - 3.0), Vec2::new((card.width() - 24.0) * fraction, 2.0));
                ui.painter().rect_filled(line, CornerRadius::same(1), accent);
            }
            if open.clicked() {
                app.now_playing_open = true;
            }
        });
}

pub fn now_playing(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    let top = top_inset(ui);
    let bottom = bottom_inset(ui);
    egui::CentralPanel::default()
        .frame(Frame::new().fill(pal.bg).inner_margin(Margin {
            left: 20,
            right: 20,
            top: (8.0 + top) as i8,
            bottom: (12.0 + bottom) as i8,
        }))
        .show(ui, |ui| {
            if let Some(tint) = app.current.as_ref().and_then(|track| app.thumbs.tint(track.id)).filter(|_| pal.bg.r() < 80) {
                let screen = ui.ctx().content_rect();
                widgets::paint_gradient(ui.painter(), screen, widgets::mix(pal.bg, tint, 0.95), pal.bg);
            }
            let backdrop = ui.interact(ui.max_rect(), ui.scope_id().with("now-playing-swipe"), Sense::drag());
            match crate::ui::gestures::swipe(ui, &backdrop).1 {
                Some(crate::ui::gestures::Swipe::Down) => app.now_playing_open = false,
                Some(crate::ui::gestures::Swipe::Left) => app.play_advance(false),
                Some(crate::ui::gestures::Swipe::Right) => app.previous(),
                _ => {}
            }
            let width = ui.available_width();
            ui.horizontal(|ui| {
                if icon_button(ui, Icon::ChevronDown, 48.0, pal.text, false).clicked() {
                    app.now_playing_open = false;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::Queue, 48.0, pal.muted, false).clicked() {
                        app.now_playing_open = false;
                        app.queue_open = true;
                    }
                    ui.with_layout(Layout::top_down(Align::Center), |ui| {
                        ui.label(RichText::new("Now playing").size(14.0).color(pal.muted));
                    });
                });
            });
            ui.add_space(8.0);

            let side = width.min(ui.available_height() * 0.46).clamp(120.0, 380.0);
            let art = app.current_art();
            ui.vertical_centered(|ui| widgets::cover(ui, side, art.as_ref(), accent, pal));
            ui.add_space(18.0);

            let (title, artist) = match app.current.as_ref() {
                Some(track) => (track.title.clone(), track.display_artist().to_string()),
                None => ("Nothing playing".to_string(), String::new()),
            };
            let liked = app.liked_current();
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new((width - 56.0).max(80.0), 54.0), Layout::top_down(Align::Min), |ui| {
                    ui.add(egui::Label::new(RichText::new(title).size(22.0).strong().color(pal.text)).truncate());
                    ui.add(egui::Label::new(RichText::new(artist).size(15.0).color(pal.muted)).truncate());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, if liked { Icon::HeartFilled } else { Icon::Heart }, 48.0, if liked { accent } else { pal.muted }, liked).clicked() {
                        app.toggle_like_current();
                    }
                });
            });
            ui.add_space(6.0);

            let position = app.position();
            let duration = app.duration().unwrap_or(0.0);
            if let Some(target) = widgets::seek_bar(ui, width, position, duration, accent, pal) {
                app.seek(target);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(crate::model::fmt_time(position)).size(12.0).color(pal.muted));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(crate::model::fmt_optional_time(if duration > 0.0 { Some(duration) } else { None }))
                            .size(12.0)
                            .color(pal.muted),
                    );
                });
            });
            ui.add_space(10.0);

            let total = 48.0 + 56.0 + 76.0 + 56.0 + 48.0 + 4.0 * ui.spacing().item_spacing.x;
            ui.horizontal(|ui| {
                ui.add_space(((width - total) * 0.5).max(0.0));
                if toggle_icon_button(ui, Icon::Shuffle, 48.0, app.settings.shuffle, accent, pal).clicked() {
                    app.toggle_shuffle();
                }
                if icon_button(ui, Icon::Prev, 56.0, pal.text, false).clicked() {
                    app.previous();
                }
                let icon = if app.audio_state() == AudioState::Playing { Icon::Pause } else { Icon::Play };
                if primary_icon_button(ui, icon, 76.0, accent).clicked() {
                    app.toggle_play();
                }
                if icon_button(ui, Icon::Next, 56.0, pal.text, false).clicked() {
                    app.play_advance(false);
                }
                let (repeat_icon, repeat_on) = match app.settings.repeat {
                    RepeatMode::Off => (Icon::Repeat, false),
                    RepeatMode::All => (Icon::Repeat, true),
                    RepeatMode::One => (Icon::RepeatOne, true),
                };
                if toggle_icon_button(ui, repeat_icon, 48.0, repeat_on, accent, pal).clicked() {
                    app.cycle_repeat();
                }
            });
            ui.add_space(14.0);

            ui.horizontal(|ui| {
                let total = 48.0 * 2.0 + ui.spacing().item_spacing.x;
                ui.add_space(((width - total) * 0.5).max(0.0));
                if icon_button(ui, Icon::Lyrics, 48.0, pal.muted, false).clicked() {
                    app.now_playing_open = false;
                    if !app.lyrics_open {
                        app.toggle_lyrics();
                    }
                }
                if icon_button(ui, Icon::Queue, 48.0, pal.muted, false).clicked() {
                    app.now_playing_open = false;
                    app.queue_open = true;
                }
            });
        });
}

pub fn more_page(app: &mut App, ui: &mut Ui, accent: Color32, pal: &Palette) {
    ui.add_space(10.0);
    ui.label(RichText::new("More").size(28.0).color(pal.text).strong());
    ui.add_space(10.0);
    for (icon, label, page) in MORE_ITEMS {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::click());
        if response.hovered() || response.is_pointer_button_down_on() {
            ui.painter().rect_filled(rect, CornerRadius::same(12), pal.raised);
        }
        icons::draw(ui.painter(), Rect::from_center_size(egui::pos2(rect.min.x + 28.0, rect.center().y), Vec2::splat(24.0)), icon, accent);
        ui.painter().text(egui::pos2(rect.min.x + 60.0, rect.center().y), egui::Align2::LEFT_CENTER, label, FontId::proportional(16.0), pal.text);
        icons::draw(ui.painter(), Rect::from_center_size(egui::pos2(rect.max.x - 24.0, rect.center().y), Vec2::splat(18.0)), Icon::Chevron, pal.muted);
        if response.clicked() {
            app.page = page;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_belongs_to_a_tab_and_tabs_navigate_back() {
        assert_eq!(tab_for(Page::Home), Tab::Home);
        assert_eq!(tab_for(Page::Liked), Tab::Liked);
        assert_eq!(tab_for(Page::PlaylistDetail(3)), Tab::More);
        assert_eq!(tab_for(Page::Settings), Tab::More);
        for (_, _, tab) in TABS {
            assert_eq!(tab_for(page_for(tab)), tab);
        }
    }

    #[test]
    fn the_more_page_lists_every_page_that_has_no_tab() {
        let listed: Vec<Page> = MORE_ITEMS.iter().map(|(_, _, page)| *page).collect();
        for page in [Page::Playlists, Page::Downloads, Page::Import, Page::History, Page::Stats, Page::Settings] {
            assert!(listed.contains(&page), "{page:?} is unreachable on a phone");
        }
        for (_, _, page) in MORE_ITEMS {
            assert_eq!(tab_for(page), Tab::More);
        }
    }
}
