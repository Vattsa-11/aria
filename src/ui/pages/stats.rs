use crate::app::App;
use crate::ui::theme::{accent, palette, Palette};

pub fn stats(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Listening stats")
                .size(28.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(12.0);
        if crate::ui::widgets::pill_button(ui, "Refresh", false, accent, &pal) {
            app.stats_dirty = true;
        }
    });
    ui.add_space(12.0);

    if app.stats_dirty {
        app.stats = crate::db::listening_stats().unwrap_or_default();
        app.stats_dirty = false;
    }

    let stats = app.stats.clone();
    if stats.total_plays == 0 {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("No listening history yet")
                    .size(20.0)
                    .color(pal.text)
                    .strong(),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Play a few tracks and come back")
                    .size(14.0)
                    .color(pal.muted),
            );
        });
        return;
    }

    let tile = ((ui.available_width() - 28.0) / 3.0 - 32.0).clamp(64.0, 150.0);
    ui.horizontal(|ui| {
        summary(ui, &pal, accent, tile, "Plays", &stats.total_plays.to_string());
        summary(
            ui,
            &pal,
            accent,
            tile,
            "Hours",
            &format!("{:.1}", stats.total_seconds as f64 / 3600.0),
        );
        summary(ui, &pal, accent, tile, "Artists", &stats.top_artists.len().to_string());
    });
    ui.add_space(14.0);

    egui::ScrollArea::vertical().scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show(ui, |ui| {
            cards(ui, &pal, &stats);
        });
}

fn summary(ui: &mut egui::Ui, pal: &Palette, accent: egui::Color32, min_width: f32, label: &str, value: &str) {
    egui::Frame::new()
        .fill(pal.surface)
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin {
            left: 16,
            right: 16,
            top: 12,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.set_min_width(min_width);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(value).size(24.0).color(accent).strong());
                ui.label(egui::RichText::new(label).size(12.5).color(pal.muted));
            });
        });
    ui.add_space(10.0);
}

fn cards(ui: &mut egui::Ui, pal: &Palette, stats: &crate::db::ListeningStats) {
    card(ui, pal, "Top tracks", |ui| {
        for (index, (title, artist, plays, seconds)) in stats.top_tracks.iter().enumerate() {
            row(
                ui,
                pal,
                &format!("{}. {} - {}", index + 1, title, artist),
                &format!("{} plays - {}", plays, hours(*seconds)),
            );
        }
    });
    card(ui, pal, "Top artists", |ui| {
        for (index, (artist, plays, seconds)) in stats.top_artists.iter().enumerate() {
            row(
                ui,
                pal,
                &format!("{}. {}", index + 1, artist),
                &format!("{} plays - {}", plays, hours(*seconds)),
            );
        }
    });
    card(ui, pal, "Hours per month", |ui| {
        if stats.months.is_empty() {
            ui.label(
                egui::RichText::new("Nothing yet")
                    .size(13.0)
                    .color(pal.muted),
            );
            return;
        }
        let peak = stats.months.iter().map(|(_, s)| *s).max().unwrap_or(1).max(1);
        for (month, seconds) in &stats.months {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(month)
                        .size(13.0)
                        .color(pal.muted)
                        .monospace(),
                );
                ui.add_space(8.0);
                let fraction = *seconds as f32 / peak as f32;
                let width = (ui.available_width() - 90.0).max(40.0);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(width, 12.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(4), pal.raised);
                let filled = egui::Rect::from_min_size(
                    rect.min,
                    egui::vec2(width * fraction, rect.height()),
                );
                ui.painter()
                    .rect_filled(filled, egui::CornerRadius::same(4), pal.text);
                ui.label(egui::RichText::new(hours(*seconds)).size(12.0).color(pal.muted));
            });
            ui.add_space(6.0);
        }
    });
}

fn card(ui: &mut egui::Ui, pal: &Palette, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(pal.surface)
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(egui::Margin {
            left: 16,
            right: 16,
            top: 14,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).size(16.0).color(pal.text).strong());
            ui.add_space(8.0);
            body(ui);
        });
    ui.add_space(12.0);
}

fn row(ui: &mut egui::Ui, pal: &Palette, left: &str, right: &str) {
    ui.horizontal(|ui| {
        let right_label = egui::RichText::new(right).size(12.5).color(pal.muted);
        let right_width = ui.painter().layout_no_wrap(right.to_owned(), egui::FontId::proportional(12.5), pal.muted).size().x;
        let left_width = (ui.available_width() - right_width - 34.0).max(40.0);
        ui.add_sized(
            [left_width, 18.0],
            egui::Label::new(egui::RichText::new(left).size(13.5).color(pal.text)).truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(right_label);
        });
    });
    ui.add_space(4.0);
}

fn hours(seconds: u64) -> String {
    let minutes = seconds / 60;
    if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{:.1} h", seconds as f64 / 3600.0)
    }
}
