use crate::app::App;
use crate::ui::theme::{accent, palette};
use crate::ui::pages::rows::track_row;

pub fn history(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("History")
                .size(28.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{} recently played", app.history.len()))
                .size(14.0)
                .color(pal.muted),
        );
    });
    ui.add_space(10.0);

    if app.history.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("Nothing played yet")
                    .size(20.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Tracks you play will show up here")
                    .size(14.0)
                    .color(pal.muted),
            );
        });
        return;
    }
    let mut rows: Vec<(egui::Response, crate::model::Track)> = Vec::new();
    let salt = egui::Id::unique("history_rows");
    let mut play: Option<usize> = None;
    let mut like_index: Option<usize> = None;

    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show_rows(ui, crate::ui::pages::rows::ROW_H, app.history.len(), |ui, range| {
        for index in range {
            let track = app.history[index].clone();
            let liked = app.is_liked(track.id);
            let (dim, badge) = app.row_state(&track);
            let resp = track_row(
                ui, salt, index, &track, liked, accent, &pal, false, false, None, dim, badge,
            );
            let clicked = resp.clicked;
            let like_clicked = resp.like.clicked();
            rows.push((resp.menu, track));
            if clicked {
                play = Some(index);
            } else if like_clicked {
                like_index = Some(index);
            }
        }
    });

    if let Some(index) = play {
        app.play_history(index);
    }
    if let Some(index) = like_index {
        if let Some(track) = app.history.get(index).cloned() {
            app.toggle_like_track(&track);
        }
    }
    crate::ui::pages::rows::row_menus(app, rows, crate::ui::pages::rows::MenuKind::Standard, &pal);
    ui.add_space(20.0);
}
