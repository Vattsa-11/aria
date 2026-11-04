use std::sync::Arc;

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, Style,
    TextStyle, Visuals,
};

use crate::settings::{Settings, ThemeVariant};

pub const ACCENT_PRESETS: [[u8; 3]; 10] = [
    [79, 140, 255],
    [124, 92, 255],
    [236, 72, 153],
    [239, 108, 74],
    [245, 158, 11],
    [34, 197, 94],
    [16, 185, 165],
    [56, 189, 248],
    [244, 63, 94],
    [168, 178, 209],
];

#[derive(Clone)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
}

pub fn palette(variant: ThemeVariant) -> Palette {
    match variant {
        ThemeVariant::Dark => Palette {
            bg: Color32::from_rgb(15, 15, 18),
            surface: Color32::from_rgb(22, 22, 27),
            raised: Color32::from_rgb(31, 31, 38),
            line: Color32::from_rgb(38, 38, 46),
            text: Color32::from_rgb(232, 232, 238),
            muted: Color32::from_rgb(148, 148, 162),
        },
        ThemeVariant::Amoled => Palette {
            bg: Color32::from_rgb(0, 0, 0),
            surface: Color32::from_rgb(9, 9, 11),
            raised: Color32::from_rgb(20, 20, 24),
            line: Color32::from_rgb(28, 28, 33),
            text: Color32::from_rgb(240, 240, 245),
            muted: Color32::from_rgb(138, 138, 150),
        },
    }
}

static ADAPTIVE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn set_adaptive(color: Option<Color32>) {
    let packed = color.map(|color| 0xFF00_0000 | (color.r() as u32) << 16 | (color.g() as u32) << 8 | color.b() as u32).unwrap_or(0);
    ADAPTIVE.store(packed, std::sync::atomic::Ordering::Relaxed);
}

pub fn fixed_accent(settings: &Settings) -> Color32 {
    Color32::from_rgb(settings.accent[0], settings.accent[1], settings.accent[2])
}

pub fn accent(settings: &Settings) -> Color32 {
    let packed = ADAPTIVE.load(std::sync::atomic::Ordering::Relaxed);
    if packed == 0 {
        return fixed_accent(settings);
    }
    Color32::from_rgb((packed >> 16) as u8, (packed >> 8) as u8, packed as u8)
}

pub fn accent_hex(settings: &Settings) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        settings.accent[0], settings.accent[1], settings.accent[2]
    )
}

pub fn parse_hex(input: &str) -> Option<[u8; 3]> {
    let cleaned = input.trim().trim_start_matches('#');
    if cleaned.len() != 6 || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&cleaned[0..2], 16).ok()?;
    let g = u8::from_str_radix(&cleaned[2..4], 16).ok()?;
    let b = u8::from_str_radix(&cleaned[4..6], 16).ok()?;
    Some([r, g, b])
}

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/Inter.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".to_owned());
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &Context, settings: &Settings) {
    let pal = palette(settings.theme);
    let accent = accent(settings);

    let mut style = Style::default();
    let mut visuals = Visuals::dark();

    visuals.panel_fill = pal.bg;
    visuals.window_fill = pal.surface;
    visuals.extreme_bg_color = pal.raised;
    visuals.faint_bg_color = pal.surface;
    visuals.window_stroke = Stroke::new(1.0, pal.line);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(12);
    visuals.override_text_color = Some(pal.text);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = accent.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, accent);
    visuals.slider_trailing_fill = true;
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.image_loading_spinners = false;

    let widgets = Visuals::dark().widgets;
    visuals.widgets = widgets;

    style.visuals = visuals.clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(16.0, 12.0);
    style.spacing.interact_size = egui::vec2(44.0, 44.0);
    style.spacing.slider_width = 180.0;
    style.spacing.scroll.floating = false;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.bar_inner_margin = 4.0;
    style.spacing.scroll.bar_outer_margin = 2.0;
    style.spacing.menu_margin = egui::Margin::same(8);
    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(24.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(12.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
    ]
    .into();

    ctx.set_global_style(style.clone());
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style.clone());
    ctx.set_visuals(visuals);
}
