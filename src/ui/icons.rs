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

fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let w = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        w.0 * p0.0 + w.1 * p1.0 + w.2 * p2.0 + w.3 * p3.0,
        w.0 * p0.1 + w.1 * p1.1 + w.2 * p2.1 + w.3 * p3.1,
    )
}

const HEART_SEGMENTS: usize = 14;

fn heart_left_path() -> Vec<(f32, f32)> {
    let curves = [
        ((12.0, 5.09), (10.91, 3.81), (9.24, 3.0), (7.5, 3.0)),
        ((7.5, 3.0), (4.42, 3.0), (2.0, 5.42), (2.0, 8.5)),
        ((2.0, 8.5), (2.0, 12.28), (5.4, 15.36), (10.55, 20.03)),
    ];
    let mut points = Vec::new();
    for (p0, p1, p2, p3) in curves {
        for step in 0..HEART_SEGMENTS {
            points.push(cubic(p0, p1, p2, p3, step as f32 / HEART_SEGMENTS as f32));
        }
    }
    points.push((10.55, 20.03));
    points.push((12.0, 21.35));
    points
}

fn heart_point(center: Pos2, size: f32, point: (f32, f32), mirror: bool) -> Pos2 {
    let unit = size / 22.0;
    let x = if mirror { 24.0 - point.0 } else { point.0 };
    Pos2::new(center.x + (x - 12.0) * unit, center.y + (point.1 - 12.2) * unit)
}

pub fn heart_outline(center: Pos2, size: f32) -> Vec<Pos2> {
    let path = heart_left_path();
    let mut points: Vec<Pos2> = path.iter().map(|point| heart_point(center, size, *point, false)).collect();
    points.extend(path.iter().rev().map(|point| heart_point(center, size, *point, true)));
    points
}

pub fn heart_halves(center: Pos2, size: f32) -> (Vec<Pos2>, Vec<Pos2>) {
    let path = heart_left_path();
    let overlap = size / 22.0 * 0.12;
    let mut left: Vec<Pos2> = path.iter().map(|point| heart_point(center, size, *point, false)).collect();
    let mut right: Vec<Pos2> = path.iter().map(|point| heart_point(center, size, *point, true)).collect();
    for point in left.iter_mut() {
        point.x = point.x.min(center.x + overlap);
    }
    for point in right.iter_mut() {
        point.x = point.x.max(center.x - overlap);
    }
    (left, right)
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
            let size = scale * 0.98;
            if icon == Icon::HeartFilled {
                let (left, right) = heart_halves(center, size);
                painter.add(Shape::convex_polygon(left, color, Stroke::NONE));
                painter.add(Shape::convex_polygon(right, color, Stroke::NONE));
            } else {
                let outline = heart_outline(center, size);
                painter.add(Shape::closed_line(outline, Stroke::new((scale * 0.075).clamp(1.3, 2.2), color)));
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

#[cfg(test)]
mod heart_tests {
    use super::*;

    fn is_convex(points: &[Pos2]) -> bool {
        let mut sign = 0i8;
        for index in 0..points.len() {
            let a = points[index];
            let b = points[(index + 1) % points.len()];
            let c = points[(index + 2) % points.len()];
            let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            if cross.abs() < 0.05 {
                continue;
            }
            let current = if cross > 0.0 { 1 } else { -1 };
            if sign == 0 {
                sign = current;
            } else if sign != current {
                return false;
            }
        }
        true
    }

    #[test]
    fn each_heart_half_is_convex_so_the_fill_has_no_artifacts() {
        let (left, right) = heart_halves(Pos2::new(50.0, 50.0), 40.0);
        assert!(is_convex(&left), "left half is not convex");
        assert!(is_convex(&right), "right half is not convex");
    }

    #[test]
    fn heart_is_symmetric_and_stays_inside_its_box() {
        let center = Pos2::new(100.0, 100.0);
        let outline = heart_outline(center, 44.0);
        let (min_x, max_x) = outline.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
        assert!(((center.x - min_x) - (max_x - center.x)).abs() < 0.01);
        assert!(max_x - min_x <= 44.0 + 0.01);
        let (min_y, max_y) = outline.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        assert!(max_y - min_y <= 44.0);
        assert!(outline.len() > 60);
    }
}
