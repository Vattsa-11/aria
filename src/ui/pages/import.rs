use egui::{Align, Color32, Layout, RichText, ScrollArea, TextEdit, Vec2};

use crate::app::App;
use crate::import::MatchState;
use crate::ui::theme::{accent, palette, Palette};
use crate::ui::widgets::pill_button;

const OK: Color32 = Color32::from_rgb(96, 210, 160);
const WARN: Color32 = Color32::from_rgb(224, 168, 96);

pub fn import(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("Import")
                .size(28.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(12.0);
        ui.label(
            RichText::new("Exportify CSV, a text list, or a playlist URL")
                .size(13.0)
                .color(pal.muted),
        );
    });
    ui.add_space(10.0);

    if ui.available_width() < 560.0 {
        ui.add_sized(
            [ui.available_width(), 38.0],
            TextEdit::singleline(&mut app.import.url)
                .hint_text("Paste a YouTube or YouTube Music playlist URL")
                .text_color(pal.text),
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if pill_button(ui, "Choose file", false, accent, &pal) {
                app.import_file();
            }
            if pill_button(ui, "Import URL", false, accent, &pal) {
                app.import_url();
            }
        });
    } else {
        ui.horizontal(|ui| {
            if pill_button(ui, "Choose file", false, accent, &pal) {
                app.import_file();
            }
            ui.add_space(10.0);
            let width = (ui.available_width() - 260.0).max(160.0);
            ui.add_sized(
                [width, 34.0],
                TextEdit::singleline(&mut app.import.url)
                    .hint_text("Paste a YouTube or YouTube Music playlist URL")
                    .text_color(pal.text),
            );
            if pill_button(ui, "Import URL", false, accent, &pal) {
                app.import_url();
            }
        });
    }

    if app.import.loading_playlist {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Reading the playlist…")
                    .size(13.0)
                    .color(pal.muted),
            );
        });
    }
    if let Some(error) = &app.import.error {
        ui.add_space(6.0);
        ui.label(RichText::new(error).size(13.0).color(WARN));
    }

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);

    if app.import.entries.is_empty() {
        empty_state(ui, &pal);
        return;
    }

    let total = app.import.entries.len();
    let matched = app
        .import
        .entries
        .iter()
        .filter(|entry| entry.matched.is_some())
        .count();
    let skipped = app
        .import
        .entries
        .iter()
        .filter(|entry| entry.skipped)
        .count();
    let pending = app
        .import
        .entries
        .iter()
        .filter(|entry| entry.state == MatchState::Pending || entry.state == MatchState::Searching)
        .count();

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{matched} of {total} matched"))
                .size(13.5)
                .color(pal.text),
        );
        ui.add_space(10.0);
        ui.add(
            egui::ProgressBar::new(matched as f32 / total as f32)
                .desired_width(180.0)
                .corner_radius(egui::CornerRadius::same(6))
                .fill(accent),
        );
        if pending > 0 {
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("matching {pending} in the background"))
                    .size(12.5)
                    .color(pal.muted),
            );
        }
        if skipped > 0 {
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("{skipped} skipped"))
                    .size(12.5)
                    .color(pal.muted),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pill_button(ui, "Clear", false, accent, &pal) {
                app.clear_import();
            }
        });
    });
    ui.add_space(2.0);
    if !app.import.source.is_empty() {
        ui.label(
            RichText::new(format!("From {}", app.import.source))
                .size(12.0)
                .color(pal.muted),
        );
    }
    ui.add_space(8.0);

    let mut actions: Vec<(usize, RowHit)> = Vec::new();
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .max_height((ui.available_height() - 132.0).max(140.0))
        .show(ui, |ui| {
            for (index, entry) in app.import.entries.iter().enumerate() {
                if let Some(hit) = row(ui, index, entry, &pal, accent) {
                    actions.push((index, hit));
                }
            }
            ui.add_space(8.0);
        });

    for (index, hit) in actions {
        match hit {
            RowHit::Swap => app.import.swap = Some(index),
            RowHit::Skip => app.skip_import(index),
            RowHit::Retry => app.retry_import(index),
        }
    }

    ui.add_space(6.0);
    footer(app, ui, &pal, accent);
    swap_window(app, ui, &pal, accent);
}

#[derive(Clone, Copy)]
enum RowHit {
    Swap,
    Skip,
    Retry,
}

fn row(
    ui: &mut egui::Ui,
    index: usize,
    entry: &crate::import::ImportEntry,
    pal: &Palette,
    accent: Color32,
) -> Option<RowHit> {
    let mut hit: Option<RowHit> = None;
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 56.0), egui::Sense::hover());
    if index % 2 == 1 {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(6), pal.surface);
    }

    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(10.0, 6.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    inner.label(
        RichText::new(format!("{:>2}", index + 1))
            .monospace()
            .size(12.0)
            .color(pal.muted),
    );
    inner.add_space(8.0);

    let text_width = (rect.width() - 260.0).max(180.0);
    inner.allocate_ui_with_layout(
        Vec2::new(text_width, 44.0),
        Layout::top_down(Align::LEFT),
        |ui| {
            if let Some(track) = &entry.matched {
                let colour = if entry.skipped { pal.muted } else { pal.text };
                ui.label(RichText::new(&track.title).size(13.5).color(colour));
                ui.label(
                    RichText::new(format!("{} · matched", track.display_artist()))
                        .size(11.5)
                        .color(OK),
                );
            } else {
                ui.label(
                    RichText::new(entry.title.clone())
                        .size(13.5)
                        .color(pal.text),
                );
                let (label, colour) = match &entry.state {
                    MatchState::Searching => ("searching…".to_string(), pal.muted),
                    MatchState::Failed(message) => (message.clone(), WARN),
                    _ => ("waiting".to_string(), pal.muted),
                };
                ui.label(RichText::new(label).size(11.5).color(colour));
            }
            ui.label(
                RichText::new(entry.label())
                    .size(10.5)
                    .color(pal.muted),
            );
        },
    );

    inner.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let skip_label = if entry.skipped { "Include" } else { "Skip" };
        if pill_button(ui, skip_label, entry.skipped, accent, pal) {
            hit = Some(RowHit::Skip);
        }
        if matches!(entry.state, MatchState::Failed(_))
            && pill_button(ui, "Retry", false, accent, pal)
        {
            hit = Some(RowHit::Retry);
        }
        if entry.matched.is_some() && pill_button(ui, "Swap", false, accent, pal) {
            hit = Some(RowHit::Swap);
        }
    });
    hit
}

fn empty_state(ui: &mut egui::Ui, pal: &Palette) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.label(
            RichText::new("Bring a playlist in")
                .size(20.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Exportify CSV, a plain text list (Artist - Title), or a playlist URL. \
                 Every match is reviewed before anything is added.",
            )
            .size(14.0)
            .color(pal.muted),
        );
    });
}

fn footer(app: &mut App, ui: &mut egui::Ui, pal: &Palette, accent: Color32) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Add to").size(13.0).color(pal.muted));
        let names: Vec<String> = app
            .playlists
            .iter()
            .map(|playlist| playlist.name.clone())
            .collect();
        let current = match app.import.target {
            Some(index) => names
                .get(index)
                .cloned()
                .unwrap_or_else(|| "New playlist".to_string()),
            None => "New playlist".to_string(),
        };
        egui::ComboBox::from_id_salt("import_target")
            .selected_text(current)
            .width(180.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(app.import.target.is_none(), "New playlist")
                    .clicked()
                {
                    app.import.target = None;
                }
                for (index, name) in names.iter().enumerate() {
                    if ui
                        .selectable_label(app.import.target == Some(index), name)
                        .clicked()
                    {
                        app.import.target = Some(index);
                    }
                }
            });
        if app.import.target.is_none() {
            ui.add_sized(
                [200.0, 32.0],
                TextEdit::singleline(&mut app.import.new_playlist)
                    .hint_text("New playlist name")
                    .text_color(pal.text),
            );
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if pill_button(ui, "Add to playlist", true, accent, pal) {
                app.apply_import();
            }
        });
    });
}

fn swap_window(app: &mut App, ui: &mut egui::Ui, pal: &Palette, accent: Color32) {
    let Some(index) = app.import.swap else {
        return;
    };
    let Some(entry) = app.import.entries.get(index) else {
        app.import.swap = None;
        return;
    };
    let title = entry.title.clone();
    let candidates = app.import_candidates(index);
    let mut chosen: Option<usize> = None;
    let mut close = false;
    egui::Window::new("Choose a match")
        .id(egui::Id::unique("import_swap"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ui.ctx(), |ui| {
            ui.set_min_width(360.0);
            ui.label(
                RichText::new(format!("Alternatives for {title}"))
                    .size(13.0)
                    .color(pal.muted),
            );
            ui.add_space(6.0);
            for (candidate, track) in candidates.iter().enumerate() {
                let label = format!("{} — {}", track.title, track.display_artist());
                if ui
                    .add_sized(
                        [ui.available_width(), 40.0],
                        egui::Button::new(RichText::new(label).size(13.0).color(pal.text)),
                    )
                    .clicked()
                {
                    chosen = Some(candidate);
                }
            }
            if candidates.is_empty() {
                ui.label(
                    RichText::new("No other matches — try Retry.")
                        .size(12.5)
                        .color(pal.muted),
                );
            }
            ui.add_space(6.0);
            if pill_button(ui, "Cancel", false, accent, pal) {
                close = true;
            }
        });
    if let Some(candidate) = chosen {
        app.choose_import_match(index, candidate);
        app.import.swap = None;
    } else if close {
        app.import.swap = None;
    }
}
