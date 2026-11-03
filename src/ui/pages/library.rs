use crate::app::App;
use crate::model::{fmt_time, Track};
use crate::ui::icons::Icon;
use crate::ui::theme::{accent, palette, Palette};

const ROW_H: f32 = 64.0;
const MIN_HIT: f32 = 44.0;

pub fn library(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);

    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Library")
                .size(28.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(format!("{} tracks", app.tracks.len()))
                .size(14.0)
                .color(pal.muted),
        );
        ui.horizontal_wrapped(|ui| {
            if crate::ui::widgets::accent_button(
                ui,
                "Add files",
                egui::vec2(140.0, 44.0),
                accent,
            ) {
                app.open_file_picker();
            }
            ui.add_space(6.0);
            if crate::ui::widgets::pill_button(ui, "Add folder", false, accent, &pal) {
                app.scan_folder();
            }
            ui.add_space(6.0);
            if crate::ui::widgets::pill_button(ui, "Rescan folders", false, accent, &pal) {
                app.rescan_folders();
            }
        });
    });
    ui.add_space(10.0);

    if app.tracks.is_empty() {
        empty_state(app, ui, &pal, accent);
        return;
    }

    let mut rows: Vec<(egui::Response, Track)> = Vec::new();

    let mut visible = 0..0;
    egui::ScrollArea::vertical()
        .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show_rows(ui, crate::ui::pages::rows::ROW_H, app.tracks.len(), |ui, range| {
        visible = range.clone();
        for index in range {
            let track = app.tracks[index].clone();
            let liked = app.is_liked(track.id);
            let body = egui::TextStyle::Body.resolve(ui.style());
            let small = egui::TextStyle::Small.resolve(ui.style());
            let art = app.thumb(&track);
            let response = render_row(
                ui,
                index,
                track.clone(),
                liked,
                RowStyle {
                    art,
                    accent,
                    pal: &pal,
                    body_font: body,
                    small_font: small,
                },
            );
            let menu_hit = response.menu.clicked();
            let like_hit = response.like.clicked();
            let row_hit = response.clicked;
            rows.push((response.menu, track));
            if like_hit {
                app.toggle_like(index);
            } else if row_hit && !menu_hit {
                app.play_library(index);
            }
        }
    });

    let ahead = crate::ui::pages::rows::window(&app.tracks, &visible);
    app.prefetch_thumbs(&ahead);
    crate::ui::pages::rows::row_menus(app, rows, crate::ui::pages::rows::MenuKind::Standard, &pal);
    ui.add_space(20.0);
}

fn empty_state(app: &mut App, ui: &mut egui::Ui, pal: &Palette, accent: egui::Color32) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.label(
            egui::RichText::new("Your library is empty")
                .size(20.0)
                .color(pal.text)
                .strong(),
        );
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Add audio files or drop them onto this window")
                .size(14.0)
                .color(pal.muted),
        );
        ui.add_space(12.0);
        if crate::ui::widgets::accent_button(
            ui,
            "Add files",
            egui::vec2(140.0, 44.0),
            accent,
        ) {
            app.open_file_picker();
        }
    });
}

struct Response {
    menu: egui::Response,
    like: egui::Response,
    clicked: bool,
}

struct RowStyle<'a> {
    art: Option<egui::TextureHandle>,
    accent: egui::Color32,
    pal: &'a Palette,
    body_font: egui::FontId,
    small_font: egui::FontId,
}

fn render_row(
    ui: &mut egui::Ui,
    index: usize,
    track: Track,
    liked: bool,
    style: RowStyle<'_>,
) -> Response {
    let RowStyle {
        art,
        accent,
        pal,
        body_font,
        small_font,
    } = style;

    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::Vec2::new(width, ROW_H), egui::Sense::click());

    let art_w = MIN_HIT;
    let art_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 10.0, rect.center().y - art_w * 0.5),
        egui::Vec2::splat(art_w),
    );
    crate::ui::widgets::paint_cover(ui.painter(), art_rect, art.as_ref(), 8, accent, pal);

    let menu_x = rect.max.x - 48.0;
    let menu_rect = egui::Rect::from_min_size(
        egui::pos2(menu_x, rect.center().y - MIN_HIT * 0.5),
        egui::Vec2::splat(MIN_HIT),
    );
    let menu_response = ui.interact(
        menu_rect,
        ui.scope_id().with(("menu", index)),
        egui::Sense::click(),
    );
    crate::ui::pages::rows::open_menu_on_right_click(ui.ctx(), &response, &menu_response);
    crate::ui::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(menu_rect.center(), egui::Vec2::splat(22.0)),
        Icon::More,
        if menu_response.hovered() { pal.text } else { pal.muted },
    );

    let like_x = rect.max.x - 92.0;
    let like_rect = egui::Rect::from_min_size(
        egui::pos2(like_x, rect.center().y - MIN_HIT * 0.5),
        egui::Vec2::splat(MIN_HIT),
    );
    let like_response = ui.interact(
        like_rect,
        ui.scope_id().with(("like", index)),
        egui::Sense::click(),
    );
    crate::ui::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(like_rect.center(), egui::Vec2::splat(22.0)),
        if liked { Icon::HeartFilled } else { Icon::Heart },
        if liked { accent } else { pal.muted },
    );

    let text_left = art_rect.max.x + 14.0;
    let text_right = menu_rect.min.x - 4.0;
    ui.painter().text(
        egui::pos2(text_left, rect.min.y + 14.0),
        egui::Align2::LEFT_TOP,
        &track.title,
        body_font,
        pal.text,
    );
    ui.painter().text(
        egui::pos2(text_left, rect.min.y + 36.0),
        egui::Align2::LEFT_TOP,
        track.display_artist(),
        small_font.clone(),
        pal.muted,
    );
    if let Some(d) = track.duration {
        ui.painter().text(
            egui::pos2(text_right, rect.min.y + 36.0),
            egui::Align2::RIGHT_TOP,
            fmt_time(d.as_secs_f64()),
            small_font.clone(),
            pal.muted,
        );
    }
    if let Some(l) = track.filename() {
        ui.painter().text(
            egui::pos2(text_left, rect.min.y + 50.0),
            egui::Align2::LEFT_TOP,
            l,
            small_font.clone(),
            pal.muted,
        );
    }

    let clicked = response.clicked()
        && !menu_response.clicked()
        && !like_response.clicked();
    Response {
        menu: menu_response,
        like: like_response,
        clicked,
    }
}
