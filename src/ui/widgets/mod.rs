use egui::{
    Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, TextStyle,
    Ui, Vec2,
};

use crate::ui::icons::{self, Icon};
use crate::ui::theme::Palette;

pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, tint: Color32, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let radius = CornerRadius::same((size * 0.3) as u8);
    let hover_fill = ui.visuals().widgets.hovered.weak_bg_fill;
    if active {
        ui.painter().rect_filled(rect, radius, tint.gamma_multiply(0.22));
    } else if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, radius, hover_fill);
    }
    let pop = heart_scale(ui.ctx(), response.id, icon);
    icons::draw(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(size * 0.44 * pop)),
        icon,
        tint,
    );
    response
}

pub fn toggle_icon_button(
    ui: &mut Ui,
    icon: Icon,
    size: f32,
    on: bool,
    accent: Color32,
    pal: &Palette,
) -> Response {
    icon_button(ui, icon, size, if on { accent } else { pal.muted }, on)
}

pub fn eq_slider(
    ui: &mut Ui,
    value: f32,
    min: f32,
    max: f32,
    accent: Color32,
    pal: &Palette,
) -> Option<f32> {
    let height = 128.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(40.0, height), Sense::click_and_drag());
    let rail = Rect::from_center_size(rect.center(), Vec2::new(4.0, height - 12.0));
    ui.painter()
        .rect_filled(rail, CornerRadius::same(2), pal.raised);

    let span = (max - min).max(0.001);
    let y_for = |value: f32| {
        let t = ((value - min) / span).clamp(0.0, 1.0);
        rail.max.y - t * rail.height()
    };
    let y = y_for(value);
    let y_zero = y_for(0.0);

    ui.painter().hline(
        rail.x_range().expand(4.0),
        y_zero,
        Stroke::new(1.0, pal.line),
    );

    let (top, bottom) = if y < y_zero { (y, y_zero) } else { (y_zero, y) };
    if bottom - top > 0.5 {
        ui.painter().rect_filled(
            Rect::from_x_y_ranges(rail.x_range(), top..=bottom),
            CornerRadius::same(2),
            accent,
        );
    }

    let active = response.hovered() || response.dragged();
    ui.painter().circle_filled(
        egui::pos2(rect.center().x, y),
        if active { 7.0 } else { 5.5 },
        accent,
    );

    if let Some(pointer) = response.interact_pointer_pos() {
        if response.clicked() || response.dragged() {
            let t = ((rail.max.y - pointer.y) / rail.height()).clamp(0.0, 1.0);
            return Some(min + t * span);
        }
    }
    None
}

pub fn toggle_switch(ui: &mut Ui, on: bool, accent: Color32, pal: &Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(56.0, 44.0), Sense::click());
    let track = Rect::from_center_size(rect.center(), Vec2::new(48.0, 26.0));
    let track_color = if on {
        accent
    } else if response.hovered() {
        pal.line
    } else {
        pal.raised
    };
    ui.painter()
        .rect_filled(track, CornerRadius::same(13), track_color);
    let knob_x = if on {
        track.max.x - 13.0
    } else {
        track.min.x + 13.0
    };
    ui.painter().circle_filled(
        egui::pos2(knob_x, track.center().y),
        10.0,
        if on {
            Color32::from_rgb(12, 12, 15)
        } else {
            pal.muted
        },
    );
    response.clicked()
}

pub fn accent_button(ui: &mut Ui, label: &str, size: Vec2, accent: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if response.hovered() || response.is_pointer_button_down_on() {
        accent.gamma_multiply(1.15)
    } else {
        accent
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same((size.y * 0.26) as u8), fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        TextStyle::Button.resolve(ui.style()),
        Color32::from_rgb(12, 12, 15),
    );
    response.clicked()
}

static LOADING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_loading(loading: bool) {
    LOADING.store(loading, std::sync::atomic::Ordering::Relaxed);
}

pub fn spinner_points(center: egui::Pos2, radius: f32, time: f64) -> Vec<egui::Pos2> {
    let start = (time * 4.5) as f32;
    (0..=18)
        .map(|step| {
            let angle = start + step as f32 * (std::f32::consts::PI * 1.5 / 18.0);
            egui::pos2(center.x + radius * angle.cos(), center.y + radius * angle.sin())
        })
        .collect()
}

pub fn paint_play_glyph(ui: &Ui, rect: Rect, icon: Icon, color: Color32) {
    let is_play = matches!(icon, Icon::Play | Icon::Pause);
    if is_play && LOADING.load(std::sync::atomic::Ordering::Relaxed) {
        let radius = rect.width() * 0.22;
        let time = ui.input(|input| input.time);
        ui.painter().add(egui::Shape::line(
            spinner_points(rect.center(), radius, time),
            Stroke::new((rect.width() * 0.07).max(2.0), color),
        ));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    } else {
        icons::draw(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.42)),
            icon,
            color,
        );
    }
}

pub fn fit_text_to(painter: &egui::Painter, text: &str, font: &FontId, color: Color32, max_width: f32) -> String {
    crate::miniplayer::fit_text(text, max_width.max(0.0), |candidate| {
        painter.layout_no_wrap(candidate.to_owned(), font.clone(), color).size().x
    })
}

pub fn heart_scale(ctx: &egui::Context, id: egui::Id, icon: Icon) -> f32 {
    if !matches!(icon, Icon::Heart | Icon::HeartFilled) {
        return 1.0;
    }
    let progress = ctx.animate_bool_with_time(id.with("heart-pop"), icon == Icon::HeartFilled, 0.15);
    1.0 + 0.28 * (progress * std::f32::consts::PI).sin()
}

pub fn primary_icon_button(ui: &mut Ui, icon: Icon, size: f32, accent: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let hot = response.hovered() || response.is_pointer_button_down_on();
    let radius = size * 0.5 * if hot { 1.06 } else { 1.0 };
    let _ = accent;
    ui.painter().circle_filled(rect.center(), radius, Color32::WHITE);
    paint_play_glyph(ui, rect, icon, Color32::from_rgb(12, 12, 15));
    response
}

pub fn seek_bar(
    ui: &mut Ui,
    width: f32,
    position: f64,
    duration: f64,
    accent: Color32,
    pal: &Palette,
) -> Option<f64> {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 40.0), Sense::click_and_drag());
    let track_height = 4.0;
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), track_height));
    let fraction = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };

    ui.painter()
        .rect_filled(track, CornerRadius::same(2), pal.raised);
    let active = response.hovered() || response.is_pointer_button_down_on();

    let filled_width = track.width() * fraction;
    if filled_width > 0.5 {
        let filled = Rect::from_min_size(track.min, Vec2::new(filled_width, track.height()));
        ui.painter()
            .rect_filled(filled, CornerRadius::same(2), if active { accent } else { Color32::WHITE });
    }
    if active {
        ui.painter().circle_filled(
            egui::pos2(track.min.x + filled_width, track.center().y),
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

pub fn paint_cover(
    painter: &egui::Painter,
    rect: Rect,
    texture: Option<&egui::TextureHandle>,
    radius: u8,
    accent: Color32,
    pal: &Palette,
) {
    let corner = CornerRadius::same(radius);
    painter.rect_filled(rect, corner, pal.raised);
    match texture {
        Some(texture) => {
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            painter.add(egui::epaint::RectShape::filled(rect, corner, Color32::WHITE).with_texture(texture.id(), uv));
        }
        None => icons::draw(
            painter,
            Rect::from_center_size(rect.center(), Vec2::splat(rect.width() * 0.4)),
            Icon::Note,
            accent.gamma_multiply(0.85),
        ),
    }
}

pub fn cover(ui: &mut Ui, size: f32, texture: Option<&egui::TextureHandle>, accent: Color32, pal: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_cover(ui.painter(), rect, texture, (size * 0.18).clamp(6.0, 18.0) as u8, accent, pal);
}

pub fn section_label(ui: &mut Ui, text: &str, pal: &Palette) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .font(FontId::proportional(11.5))
            .color(pal.muted),
    );
}

pub fn pill_button(ui: &mut Ui, label: &str, active: bool, accent: Color32, pal: &Palette) -> bool {    let font = TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), pal.text);
    let size = Vec2::new(galley.size().x + 36.0, 44.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if active {
        accent
    } else if response.hovered() {
        pal.surface
    } else {
        pal.raised
    };
    ui.painter().rect_filled(rect, CornerRadius::same(22), fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        if active {
            Color32::from_rgb(12, 12, 15)
        } else {
            pal.text
        },
    );
    response.clicked()
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(blend(a.r(), b.r()), blend(a.g(), b.g()), blend(a.b(), b.b()))
}

pub fn paint_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

#[cfg(test)]
mod spinner_tests {
    use super::spinner_points;

    #[test]
    fn spinner_arc_rotates_and_stays_on_the_circle() {
        let center = egui::pos2(10.0, 10.0);
        let first = spinner_points(center, 8.0, 0.0);
        let later = spinner_points(center, 8.0, 0.5);
        assert_eq!(first.len(), 19);
        assert_ne!(first[0], later[0]);
        for point in first.iter().chain(later.iter()) {
            let distance = ((point.x - 10.0).powi(2) + (point.y - 10.0).powi(2)).sqrt();
            assert!((distance - 8.0).abs() < 0.01);
        }
    }
}
