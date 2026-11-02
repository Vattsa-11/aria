use crate::app::App;
use crate::ui::pages::rows::track_row;
use crate::ui::theme::{accent, palette};
use crate::ui::widgets::pill_button;

pub fn downloads(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Downloads")
                .size(28.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{} tracks", app.downloads.len()))
                .size(14.0)
                .color(pal.muted),
        );

    });
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!("Available offline · saved to {}", crate::downloads::dir().display()))
            .size(12.5)
            .color(pal.muted),
    );
    ui.add_space(10.0);

    if !app.downloading.is_empty() {
        ui.label(
            egui::RichText::new(app.downloading_summary())
                .size(13.5)
                .color(pal.text),
        );
        ui.add_space(4.0);
        let mut cancel: Option<u64> = None;
        let active = app.downloading.clone();
        for track in &active {
            ui.horizontal(|ui| {
                let progress = app.download_progress.get(&track.id).copied();
                let label = match progress {
                    Some((done, Some(total))) if total > 0 => {
                        format!("{} · {}%", track.title, done * 100 / total)
                    }
                    _ => track.title.clone(),
                };
                ui.label(egui::RichText::new(label).size(13.0).color(pal.muted));
                if pill_button(ui, "Cancel", false, accent, &pal) {
                    cancel = Some(track.id);
                }
            });
        }
        if let Some(id) = cancel {
            app.cancel_download(id);
        }
        ui.add_space(10.0);
    }

    if app.downloads.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("Nothing downloaded yet")
                    .size(20.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(
                    "Use the ⋮ menu on any song and pick Download to keep it for offline playback",
                )
                .size(14.0)
                .color(pal.muted),
            );
        });
        return;
    }

    let mut rows: Vec<(egui::Response, crate::model::Track)> = Vec::new();
    let salt = egui::Id::unique("download_rows");
    let mut play: Option<usize> = None;
    let mut like_index: Option<usize> = None;

    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show_rows(ui, crate::ui::pages::rows::ROW_H, app.downloads.len(), |ui, range| {
        for index in range {
            let track = app.downloads[index].clone();
            let liked = app.is_liked(track.id);
            let resp = track_row(
                ui,
                salt,
                index,
                &track,
                liked,
                accent,
                &pal,
                false,
                false,
                None,
                false,
                Some("Offline"),
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
        app.play_downloads(index);
    }
    if let Some(index) = like_index {
        if let Some(track) = app.downloads.get(index).cloned() {
            app.toggle_like_track(&track);
        }
    }
    crate::ui::pages::rows::row_menus(app, rows, crate::ui::pages::rows::MenuKind::Downloads, &pal);
    ui.add_space(20.0);
}
