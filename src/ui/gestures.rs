use egui::{Color32, Context, CornerRadius, Id, Pos2, Rect, Response, Ui, Vec2};

use crate::model::Track;
use crate::ui::icons::{self, Icon};

pub const SWIPE_MIN: f32 = 64.0;
pub const ROW_SWIPE_MIN: f32 = 72.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Swipe {
    Left,
    Right,
    Up,
    Down,
}

pub fn classify(delta: Vec2, min: f32) -> Option<Swipe> {
    let (across, along) = (delta.x.abs(), delta.y.abs());
    if across >= min && across > along * 1.5 {
        Some(if delta.x > 0.0 { Swipe::Right } else { Swipe::Left })
    } else if along >= min && along > across * 1.5 {
        Some(if delta.y > 0.0 { Swipe::Down } else { Swipe::Up })
    } else {
        None
    }
}

pub fn swipe(ui: &Ui, response: &Response) -> (Vec2, Option<Swipe>) {
    let key = response.id.with("swipe-origin");
    let ctx = ui.ctx();
    if response.drag_started() {
        let origin = response.interact_pointer_pos();
        ctx.data_mut(|data| data.insert_temp(key, origin));
    }
    let origin: Option<Pos2> = ctx.data(|data| data.get_temp::<Option<Pos2>>(key)).flatten();
    let now = ctx.input(|input| input.pointer.latest_pos());
    let offset = match (origin, now) {
        (Some(origin), Some(now)) if response.dragged() => now - origin,
        _ => Vec2::ZERO,
    };
    let mut done = None;
    if response.drag_stopped() {
        if let (Some(origin), Some(now)) = (origin, now) {
            done = classify(now - origin, SWIPE_MIN);
        }
        ctx.data_mut(|data| data.remove_temp::<Option<Pos2>>(key));
    }
    (offset, done)
}

pub fn row_swipe(ctx: &Context, rect: Rect, salt: Id) -> (f32, bool) {
    let key = salt.with("row-swipe");
    let (pressed, released, down, now) = ctx.input(|input| (input.pointer.primary_pressed(), input.pointer.primary_released(), input.pointer.primary_down(), input.pointer.latest_pos()));
    if pressed {
        if let Some(position) = now.filter(|position| rect.contains(*position)) {
            ctx.data_mut(|data| data.insert_temp(key, position));
        }
    }
    let Some(origin) = ctx.data(|data| data.get_temp::<Pos2>(key)) else {
        return (0.0, false);
    };
    let Some(now) = now else {
        return (0.0, false);
    };
    let delta = now - origin;
    if released {
        ctx.data_mut(|data| data.remove_temp::<Pos2>(key));
        return (0.0, classify(delta, ROW_SWIPE_MIN) == Some(Swipe::Right));
    }
    if !down {
        ctx.data_mut(|data| data.remove_temp::<Pos2>(key));
        return (0.0, false);
    }
    let horizontal = delta.x > 0.0 && delta.x > delta.y.abs() * 1.5;
    (if horizontal { delta.x } else { 0.0 }, false)
}

fn queue_key() -> Id {
    Id::NULL.with("aria.swipe-queue")
}

pub fn request_queue(ctx: &Context, track: Track) {
    ctx.data_mut(|data| data.insert_temp(queue_key(), Some(track)));
}

pub fn take_queue_request(ctx: &Context) -> Option<Track> {
    ctx.data_mut(|data| {
        let found = data.get_temp::<Option<Track>>(queue_key()).flatten();
        if found.is_some() {
            data.insert_temp(queue_key(), None::<Track>);
        }
        found
    })
}

pub fn paint_reveal(ui: &Ui, rect: Rect, reveal: f32, accent: Color32) {
    if reveal < 4.0 {
        return;
    }
    let width = reveal.min(rect.width() * 0.6);
    let strip = Rect::from_min_size(rect.min, Vec2::new(width, rect.height()));
    let ready = reveal >= ROW_SWIPE_MIN;
    ui.painter().rect_filled(strip, CornerRadius::same(10), accent.gamma_multiply(if ready { 0.45 } else { 0.22 }));
    if width > 34.0 {
        icons::draw(ui.painter(), Rect::from_center_size(egui::pos2(strip.min.x + 24.0, strip.center().y), Vec2::splat(22.0)), Icon::Queue, Color32::WHITE);
    }
}

pub fn horizontal_drag(ctx: &Context, rect: Rect, key: Id) -> f32 {
    let (pressed, down, now, delta, origin_now) = ctx.input(|input| (input.pointer.primary_pressed(), input.pointer.primary_down(), input.pointer.latest_pos(), input.pointer.delta(), input.pointer.press_origin()));
    if pressed {
        if let Some(position) = origin_now.or(now).filter(|position| rect.contains(*position)) {
            ctx.data_mut(|data| data.insert_temp(key, (position, 0u8)));
        }
    }
    let Some((origin, axis)) = ctx.data(|data| data.get_temp::<(Pos2, u8)>(key)) else {
        return 0.0;
    };
    if !down {
        ctx.data_mut(|data| data.remove_temp::<(Pos2, u8)>(key));
        return 0.0;
    }
    let Some(now) = now else {
        return 0.0;
    };
    if axis == 0 {
        let moved = now - origin;
        if moved.x.abs() > 8.0 || moved.y.abs() > 8.0 {
            let axis = if moved.x.abs() > moved.y.abs() * 1.2 { 1 } else { 2 };
            ctx.data_mut(|data| data.insert_temp(key, (origin, axis)));
            return if axis == 1 { moved.x } else { 0.0 };
        }
        return 0.0;
    }
    if axis == 1 {
        delta.x
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swipes_need_distance_and_a_clear_direction() {
        assert_eq!(classify(Vec2::new(90.0, 10.0), SWIPE_MIN), Some(Swipe::Right));
        assert_eq!(classify(Vec2::new(-90.0, -20.0), SWIPE_MIN), Some(Swipe::Left));
        assert_eq!(classify(Vec2::new(5.0, -100.0), SWIPE_MIN), Some(Swipe::Up));
        assert_eq!(classify(Vec2::new(-8.0, 120.0), SWIPE_MIN), Some(Swipe::Down));
        assert_eq!(classify(Vec2::new(40.0, 5.0), SWIPE_MIN), None);
        assert_eq!(classify(Vec2::new(80.0, 70.0), SWIPE_MIN), None);
        assert_eq!(classify(Vec2::ZERO, SWIPE_MIN), None);
    }
}
