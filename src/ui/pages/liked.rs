use crate::app::App;
use crate::ui::theme::{accent, palette};
use crate::ui::pages::rows::track_row;

pub fn liked(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Liked Songs")
                .size(28.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{} tracks", app.liked.len()))
                .size(14.0)
                .color(pal.muted),
        );
    });
    ui.add_space(10.0);

    if app.liked.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("No liked songs yet")
                    .size(20.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Tap the heart on any track to add it here")
                    .size(14.0)
                    .color(pal.muted),
            );
        });
        return;
    }
    let salt = egui::Id::unique("liked_rows");
    let mut play: Option<usize> = None;
    let mut like_index: Option<usize> = None;

    let mut rows: Vec<(egui::Response, crate::model::Track)> = Vec::new();

    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show_rows(ui, crate::ui::pages::rows::ROW_H, app.liked.len(), |ui, range| {
        for index in range {
            let track = app.liked[index].clone();
            let (dim, badge) = app.row_state(&track);
            let resp = track_row(
                ui, salt, index, &track, true, accent, &pal, false, false, None, dim, badge,
            );
            let clicked = resp.clicked;
            let like_clicked = resp.like.clicked();
            rows.push((resp.menu, track.clone()));
            if clicked {
                play = Some(index);
            } else if like_clicked {
                like_index = Some(index);
            }
        }
    });

    if let Some(index) = play {
        app.play_liked(index);
    }
    if let Some(index) = like_index {
        if let Some(track) = app.liked.get(index).cloned() {
            app.toggle_like_track(&track);
        }
    }
    crate::ui::pages::rows::row_menus(app, rows, crate::ui::pages::rows::MenuKind::Standard, &pal);
    ui.add_space(20.0);
}
