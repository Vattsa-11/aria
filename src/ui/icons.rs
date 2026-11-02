use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Prev,
    Shuffle,
    Repeat,
    RepeatOne,
    Heart,
    HeartFilled,
    Queue,
    Volume,
    Search,
    Settings,
    Library,
    Close,
    Note,
    Chevron,
    ChevronUp,
    ChevronDown,
    More,
    Mini,
    Download,
    Import,
    Home,
    Lyrics,
}

fn arc(center: Pos2, radius: f32, start: f32, end: f32, segments: usize) -> Vec<Pos2> {
    (0..=segments)
        .map(|i| {
            let t = start + (end - start) * (i as f32 / segments as f32);
            Pos2::new(center.x + radius * t.cos(), center.y + radius * t.sin())
        })
        .collect()
}

fn poly(points: &[(f32, f32)], center: Pos2, scale: f32) -> Vec<Pos2> {
    points
        .iter()
        .map(|(x, y)| Pos2::new(center.x + x * scale, center.y + y * scale))
        .collect()
}

pub fn draw(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let center = rect.center();
    let scale = rect.width().min(rect.height());
    let stroke = Stroke::new((scale * 0.085).clamp(1.4, 2.6), color);

    match icon {
        Icon::Lyrics => {
            let body = Rect::from_center_size(center, Vec2::new(0.62 * scale, 0.52 * scale));
            painter.rect_stroke(
                body,
                CornerRadius::same((scale * 0.1) as u8),
                stroke,
                StrokeKind::Inside,
            );
            for (index, offset) in [(-0.12f32), 0.04, 0.2].into_iter().enumerate() {
                let width = if index == 2 { 0.3 } else { 0.4 };
                painter.line_segment(
                    [
                        Pos2::new(body.min.x + 0.1 * scale, center.y + offset * scale),
                        Pos2::new(
                            body.min.x + (0.1 + width) * scale,
                            center.y + offset * scale,
                        ),
                    ],
                    Stroke::new(stroke.width * 0.9, color),
                );
            }
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(
                poly(&[(-0.26, -0.34), (0.34, 0.0), (-0.26, 0.34)], center, scale),
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for x in [-0.20, 0.07] {
                let bar = Rect::from_min_size(
                    Pos2::new(center.x + x * scale, center.y - 0.32 * scale),
                    Vec2::new(0.13 * scale, 0.64 * scale),
                );
                painter.rect_filled(bar, CornerRadius::same(2), color);
            }
        }
        Icon::Next => {
            painter.add(Shape::convex_polygon(
                poly(&[(-0.30, -0.30), (0.14, 0.0), (-0.30, 0.30)], center, scale),
                color,
                Stroke::NONE,
            ));
            let bar = Rect::from_min_size(
                Pos2::new(center.x + 0.18 * scale, center.y - 0.30 * scale),
                Vec2::new(0.12 * scale, 0.60 * scale),
            );
            painter.rect_filled(bar, 1.5, color);
        }
        Icon::Prev => {
            painter.add(Shape::convex_polygon(
                poly(&[(0.30, -0.30), (-0.14, 0.0), (0.30, 0.30)], center, scale),
                color,
                Stroke::NONE,
            ));
            let bar = Rect::from_min_size(
                Pos2::new(center.x - 0.30 * scale, center.y - 0.30 * scale),
                Vec2::new(0.12 * scale, 0.60 * scale),
            );
            painter.rect_filled(bar, 1.5, color);
        }
        Icon::Shuffle => {
            let first = poly(
                &[(-0.32, -0.22), (-0.06, -0.22), (0.06, 0.20), (0.26, 0.20)],
                center,
                scale,
            );
            let second = poly(
                &[(-0.32, 0.20), (-0.06, 0.20), (0.06, -0.22), (0.26, -0.22)],
                center,
                scale,
            );
            painter.add(Shape::line(first, stroke));
            painter.add(Shape::line(second, stroke));
            for y in [-0.22, 0.20] {
                painter.add(Shape::convex_polygon(
                    poly(
                        &[(0.20, y - 0.11), (0.34, y), (0.20, y + 0.11)],
                        center,
                        scale,
                    ),
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Icon::Repeat | Icon::RepeatOne => {
            let radius = 0.28 * scale;
            painter.add(Shape::line(arc(center, radius, -2.45, 2.45, 22), stroke));
            painter.add(Shape::convex_polygon(
                poly(
                    &[(-0.36, -0.10), (-0.18, -0.22), (-0.18, 0.02)],
                    center,
                    scale,
                ),
                color,
                Stroke::NONE,
            ));
            if icon == Icon::RepeatOne {
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "1",
                    egui::FontId::proportional(scale * 0.42),
                    color,
                );
            }
        }
        Icon::Heart | Icon::HeartFilled => {

            const SEGMENTS: usize = 56;
            let unit = 0.33 * scale / 16.0;
            let vertical_offset = 6.0 * unit;
            let points: Vec<Pos2> = (0..SEGMENTS)
                .map(|i| {
                    let t = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
                    let x = 16.0 * t.sin().powi(3);
                    let y = -(13.0 * t.cos()
                        - 5.0 * (2.0 * t).cos()
                        - 2.0 * (3.0 * t).cos()
                        - (4.0 * t).cos());
                    Pos2::new(
                        center.x + x * unit,
                        center.y + y * unit - vertical_offset,
                    )
                })
                .collect();
            if icon == Icon::HeartFilled {

                painter.circle_filled(
                    Pos2::new(center.x - 0.16 * scale, center.y - 0.07 * scale),
                    0.17 * scale,
                    color,
                );
                painter.circle_filled(
                    Pos2::new(center.x + 0.16 * scale, center.y - 0.07 * scale),
                    0.17 * scale,
                    color,
                );
                painter.add(Shape::convex_polygon(
                    poly(&[(-0.30, -0.07), (0.30, -0.07), (0.0, 0.30)], center, scale),
                    color,
                    Stroke::NONE,
                ));
            } else {
                painter.add(Shape::closed_line(points, stroke));
            }
        }
        Icon::Queue => {
            for (i, width) in [0.58, 0.44, 0.52].iter().enumerate() {
                let y = -0.22 + i as f32 * 0.22;
                painter.line_segment(
                    [
                        Pos2::new(center.x - 0.30 * scale, center.y + y * scale),
                        Pos2::new(center.x + (-0.30 + width) * scale, center.y + y * scale),
                    ],
                    stroke,
                );
            }
        }
        Icon::Volume => {
            painter.add(Shape::convex_polygon(
                poly(
                    &[
                        (-0.34, -0.11),
                        (-0.16, -0.11),
                        (-0.02, -0.28),
                        (-0.02, 0.28),
                        (-0.16, 0.11),
                        (-0.34, 0.11),
                    ],
                    center,
                    scale,
                ),
                color,
                Stroke::NONE,
            ));
            painter.add(Shape::line(
                arc(center, 0.20 * scale, -0.85, 0.85, 10),
                stroke,
            ));
            painter.add(Shape::line(
                arc(center, 0.34 * scale, -0.85, 0.85, 12),
                stroke,
            ));
        }
        Icon::Search => {
            painter.circle_stroke(
                Pos2::new(center.x - 0.06 * scale, center.y - 0.06 * scale),
                0.24 * scale,
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + 0.11 * scale, center.y + 0.11 * scale),
                    Pos2::new(center.x + 0.30 * scale, center.y + 0.30 * scale),
                ],
                stroke,
            );
        }
        Icon::Settings => {
            for (y, knob) in [(-0.16, -0.10), (0.16, 0.14)] {
                painter.line_segment(
                    [
                        Pos2::new(center.x - 0.32 * scale, center.y + y * scale),
                        Pos2::new(center.x + 0.32 * scale, center.y + y * scale),
                    ],
                    stroke,
                );
                painter.circle_filled(
                    Pos2::new(center.x + knob * scale, center.y + y * scale),
                    0.09 * scale,
                    color,
                );
            }
        }
        Icon::Library => {
            for (i, x) in [-0.26, -0.02, 0.22].iter().enumerate() {
                let height = if i == 2 { 0.44 } else { 0.66 };
                let bar = Rect::from_min_size(
                    Pos2::new(center.x + x * scale, center.y - height * 0.5 * scale),
                    Vec2::new(0.11 * scale, height * scale),
                );
                painter.rect_filled(bar, CornerRadius::same(2), color);
            }
        }
        Icon::Mini => {
            let outer = Rect::from_center_size(center, Vec2::splat(0.72 * scale));
            painter.rect_stroke(
                outer,
                CornerRadius::same(3),
                Stroke::new(stroke.width, color),
                StrokeKind::Inside,
            );
            let inner = Rect::from_min_size(
                Pos2::new(outer.min.x + 0.16 * scale, outer.min.y + 0.30 * scale),
                Vec2::new(0.34 * scale, 0.26 * scale),
            );
            painter.rect_filled(inner, CornerRadius::same(2), color);
        }
        Icon::Close => {
            painter.line_segment(
                [
                    Pos2::new(center.x - 0.22 * scale, center.y - 0.22 * scale),
                    Pos2::new(center.x + 0.22 * scale, center.y + 0.22 * scale),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + 0.22 * scale, center.y - 0.22 * scale),
                    Pos2::new(center.x - 0.22 * scale, center.y + 0.22 * scale),
                ],
                stroke,
            );
        }
        Icon::Note => {
            painter.circle_filled(
                Pos2::new(center.x - 0.10 * scale, center.y + 0.18 * scale),
                0.12 * scale,
                color,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + 0.02 * scale, center.y + 0.18 * scale),
                    Pos2::new(center.x + 0.02 * scale, center.y - 0.28 * scale),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    Pos2::new(center.x + 0.02 * scale, center.y - 0.28 * scale),
                    Pos2::new(center.x + 0.26 * scale, center.y - 0.18 * scale),
                ],
                stroke,
            );
        }
        Icon::Chevron => {
            painter.add(Shape::line(
                poly(&[(-0.14, -0.18), (0.10, 0.0), (-0.14, 0.18)], center, scale),
                stroke,
            ));
        }
        Icon::ChevronUp => {
            painter.add(Shape::line(
                poly(&[(-0.22, 0.12), (0.0, -0.14), (0.22, 0.12)], center, scale),
                stroke,
            ));
        }
        Icon::ChevronDown => {
            painter.add(Shape::line(
                poly(&[(-0.22, -0.12), (0.0, 0.14), (0.22, -0.12)], center, scale),
                stroke,
            ));
        }
        Icon::Download => {

            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - 0.30 * scale),
                    Pos2::new(center.x, center.y + 0.10 * scale),
                ],
                stroke,
            );
            painter.add(Shape::line(
                poly(&[(-0.18, -0.06), (0.0, 0.13), (0.18, -0.06)], center, scale),
                stroke,
            ));
            painter.line_segment(
                [
                    Pos2::new(center.x - 0.28 * scale, center.y + 0.28 * scale),
                    Pos2::new(center.x + 0.28 * scale, center.y + 0.28 * scale),
                ],
                stroke,
            );
        }
        Icon::Home => {
            painter.add(Shape::line(
                poly(&[(-0.34, -0.02), (0.0, -0.32), (0.34, -0.02)], center, scale),
                stroke,
            ));
            painter.add(Shape::line(
                poly(&[(-0.24, -0.06), (-0.24, 0.30), (0.24, 0.30), (0.24, -0.06)], center, scale),
                stroke,
            ));
        }
        Icon::Import => {

            painter.line_segment(
                [
                    Pos2::new(center.x, center.y + 0.30 * scale),
                    Pos2::new(center.x, center.y - 0.10 * scale),
                ],
                stroke,
            );
            painter.add(Shape::line(
                poly(&[(-0.18, 0.06), (0.0, -0.13), (0.18, 0.06)], center, scale),
                stroke,
            ));
            painter.line_segment(
                [
                    Pos2::new(center.x - 0.28 * scale, center.y - 0.28 * scale),
                    Pos2::new(center.x + 0.28 * scale, center.y - 0.28 * scale),
                ],
                stroke,
            );
        }
        Icon::More => {
            for x in [-0.24, 0.0, 0.24] {
                painter.circle_filled(
                    Pos2::new(center.x + x * scale, center.y),
                    0.07 * scale,
                    color,
                );
            }
        }
    }
}
