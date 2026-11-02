use crate::app::{App, Page};
use crate::model::Playlist;
use crate::ui::pages::rows::track_row;
use crate::ui::theme::{accent, palette, Palette};

pub fn playlists(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Playlists")
                .size(28.0)
                .color(pal.text)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::widgets::accent_button(
                ui,
                "New playlist",
                egui::vec2(150.0, 44.0),
                accent,
            ) {
                app.new_playlist_dialog = true;
                app.new_playlist_name = String::new();
            }
        });
    });
    ui.add_space(10.0);

    if app.new_playlist_dialog {
        new_playlist_dialog(app, ui, accent, &pal);
    }

    if app.playlists.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("No playlists yet")
                    .size(20.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Create a playlist, then add tracks from any menu")
                    .size(14.0)
                    .color(pal.muted),
            );
        });
        return;
    }

    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() }).show(ui, |ui| {
        let total = app.playlists.len();
        for index in 0..total {
            let playlist = app.playlists[index].clone();
            playlist_row(app, ui, index, &playlist, accent, &pal);
        }
    });
    ui.add_space(20.0);
}

fn new_playlist_dialog(app: &mut App, ui: &mut egui::Ui, accent: egui::Color32, pal: &Palette) {
    egui::Frame::new()
        .fill(pal.surface)
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("New playlist").size(16.0).color(pal.text).strong(),
            );
            ui.add_space(8.0);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut app.new_playlist_name)
                    .desired_width(f32::INFINITY)
                    .hint_text("Playlist name"),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let name = app.new_playlist_name.clone();
                app.create_playlist(&name);
                app.new_playlist_dialog = false;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [100.0, 44.0],
                        egui::Button::new(egui::RichText::new("Cancel").size(14.0).color(pal.text)),
                    )
                    .clicked()
                {
                    app.new_playlist_dialog = false;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui::widgets::accent_button(
                        ui,
                        "Create",
                        egui::vec2(120.0, 44.0),
                        accent,
                    ) {
                        let name = app.new_playlist_name.clone();
                        app.create_playlist(&name);
                        app.new_playlist_dialog = false;
                    }
                });
            });
        });
}

fn playlist_row(
    app: &mut App,
    ui: &mut egui::Ui,
    index: usize,
    playlist: &Playlist,
    accent: egui::Color32,
    pal: &Palette,
) {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::Vec2::new(width, 60.0), egui::Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(10), pal.raised);
    }

    let art_w = 44.0;
    let art_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 8.0, rect.center().y - art_w * 0.5),
        egui::Vec2::splat(art_w),
    );
    ui.painter()
        .rect_filled(art_rect, egui::CornerRadius::same(8), pal.surface);
    crate::ui::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(art_rect.center(), egui::Vec2::splat(22.0)),
        crate::ui::icons::Icon::Queue,
        accent.gamma_multiply(0.85),
    );

    let text_left = art_rect.max.x + 12.0;
    ui.painter().text(
        egui::pos2(text_left, rect.min.y + 14.0),
        egui::Align2::LEFT_TOP,
        &playlist.name,
        egui::TextStyle::Body.resolve(ui.style()),
        pal.text,
    );
    ui.painter().text(
        egui::pos2(text_left, rect.min.y + 36.0),
        egui::Align2::LEFT_TOP,
        format!("{} tracks", playlist.tracks.len()),
        egui::TextStyle::Small.resolve(ui.style()),
        pal.muted,
    );

    if response.clicked() {
        app.page = Page::PlaylistDetail(index);
    }
}

pub fn playlist_detail(app: &mut App, ui: &mut egui::Ui, index: usize) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    let Some(playlist) = app.playlists.get(index).cloned() else {
        ui.label(
            egui::RichText::new("Playlist not found").size(16.0).color(pal.muted),
        );
        return;
    };

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [44.0, 44.0],
                egui::Button::new(egui::RichText::new("<").size(18.0).color(pal.text)),
            )
            .clicked()
        {
            app.page = Page::Playlists;
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(&playlist.name)
                .size(26.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{} tracks", playlist.tracks.len()))
                .size(13.0)
                .color(pal.muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_sized(
                    [90.0, 44.0],
                    egui::Button::new(egui::RichText::new("Delete").size(14.0).color(pal.muted)),
                )
                .clicked()
            {
                app.delete_playlist(index);
            }
            if ui
                .add_sized(
                    [90.0, 44.0],
                    egui::Button::new(egui::RichText::new("Rename").size(14.0).color(pal.text)),
                )
                .clicked()
            {
                app.rename_buffer = playlist.name.clone();
                app.rename_dialog = Some((index, playlist.name.clone()));
            }
            if !playlist.tracks.is_empty()
                && crate::ui::widgets::pill_button(ui, "Download all", false, accent, &pal)
            {
                app.download_playlist(index);
            }
            if !playlist.tracks.is_empty()
                && ui
                    .add_sized(
                        [100.0, 44.0],
                        egui::Button::new(
                            egui::RichText::new("Play all").size(14.0).color(accent),
                        ),
                    )
                    .clicked()
            {
                app.play_playlist(index, 0);
            }
        });
    });
    ui.add_space(10.0);

    if let Some((rename_idx, name)) = app.rename_dialog.clone() {
        if rename_idx == index {
            rename_dialog(app, ui, index, &name, accent, &pal);
        }
    }

    if playlist.tracks.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);
            ui.label(
                egui::RichText::new("This playlist is empty")
                    .size(18.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Use any track's menu to add it here")
                    .size(13.0)
                    .color(pal.muted),
            );
        });
        return;
    }
    let mut rows: Vec<(egui::Response, crate::model::Track)> = Vec::new();
    let salt = egui::Id::unique(("playlist_rows", index));
    app.playlist_drag.total = playlist.tracks.len();
    let mut play: Option<usize> = None;
    let mut like_index: Option<usize> = None;
    let mut remove_index: Option<usize> = None;

    egui::ScrollArea::vertical()
        .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show_rows(ui, crate::ui::pages::rows::ROW_H, playlist.tracks.len(), |ui, range| {
        for track_index in range {
            let track = playlist.tracks[track_index].clone();
            let liked = app.is_liked(track.id);
            let (dim, badge) = app.row_state(&track);
            let resp = track_row(
                ui,
                salt,
                track_index,
                &track,
                liked,
                accent,
                &pal,
                true,
                true,
                Some(&mut app.playlist_drag),
                dim,
                badge,
            );
            let clicked = resp.clicked;
            rows.push((resp.menu.clone(), track));
            if clicked {
                play = Some(track_index);
            } else if resp.like.clicked() {
                like_index = Some(track_index);
            } else if let Some(r) = resp.remove {
                if r.clicked() {
                    remove_index = Some(track_index);
                }
            } else if resp.up.as_ref().is_some_and(|r| r.clicked()) {
                if track_index > 0 {
                    app.move_playlist_track(index, track_index, track_index - 1);
                }
            } else if resp.down.as_ref().is_some_and(|r| r.clicked()) {
                if track_index + 1 < playlist.tracks.len() {
                    app.move_playlist_track(index, track_index, track_index + 1);
                }
            } else if let Some(to) = resp.move_to {
                app.move_playlist_track(index, track_index, to);
            }
        }
    });

    if let Some(track_index) = play {
        app.play_playlist(index, track_index);
    }
    if let Some(track_index) = like_index {
        if let Some(track) = playlist.tracks.get(track_index).cloned() {
            app.toggle_like_track(&track);
        }
    }
    if let Some(track_index) = remove_index {
        app.remove_from_playlist(index, track_index);
    }
    crate::ui::pages::rows::row_menus(app, rows, crate::ui::pages::rows::MenuKind::Standard, &pal);
    ui.add_space(20.0);
}

fn rename_dialog(
    app: &mut App,
    ui: &mut egui::Ui,
    index: usize,
    _current: &str,
    accent: egui::Color32,
    pal: &Palette,
) {
    let mut done = false;
    egui::Frame::new()
        .fill(pal.surface)
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Rename playlist").size(16.0).color(pal.text).strong(),
            );
            ui.add_space(8.0);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut app.rename_buffer)
                    .desired_width(f32::INFINITY),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                done = true;
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [100.0, 44.0],
                        egui::Button::new(egui::RichText::new("Cancel").size(14.0).color(pal.text)),
                    )
                    .clicked()
                {
                    app.rename_dialog = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui::widgets::accent_button(
                        ui,
                        "Save",
                        egui::vec2(120.0, 44.0),
                        accent,
                    ) {
                        done = true;
                    }
                });
            });
        });
    if done {
        let name = app.rename_buffer.clone();
        app.rename_playlist(index, &name);
        app.rename_dialog = None;
    }
}
