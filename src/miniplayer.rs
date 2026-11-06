
use std::time::Duration;
use web_time::Instant;

use egui::{
    Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};

use crate::app::App;
use crate::model::{fmt_optional_time, fmt_time};
use crate::settings::RepeatMode;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{accent, palette, Palette};

pub const MIN_WIDTH: f32 = 280.0;
pub const MIN_HEIGHT: f32 = 80.0;
pub const MAX_WIDTH: f32 = 480.0;
pub const MAX_HEIGHT: f32 = 640.0;

pub fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("aria-miniplayer")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MiniLayout {
    Compact,
    Medium,
    Large,
}

impl MiniLayout {
    pub fn for_height(height: f32) -> Self {
        if height < 120.0 {
            Self::Compact
        } else if height <= 300.0 {
            Self::Medium
        } else {
            Self::Large
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MiniPlan {
    pub layout: MiniLayout,
    pub scale: f32,
    pub show_prev: bool,
    pub show_volume: bool,
}

pub fn mini_plan(width: f32, height: f32) -> MiniPlan {
    let layout = MiniLayout::for_height(height);
    MiniPlan {
        layout,
        scale: (width / 340.0).clamp(0.85, 1.2),
        show_prev: layout == MiniLayout::Compact && width >= 380.0,
        show_volume: layout == MiniLayout::Medium && width >= 420.0 && height >= 190.0,
    }
}

pub struct MiniState {
    pub open: bool,
    pub placed: bool,
    pub swipe: Option<Pos2>,
    pub press: Option<(Instant, bool)>,
    pub marquee: Option<(String, Instant)>,
    pub layout: Option<MiniLayout>,
    pub prev: Option<MiniLayout>,
    pub opacity: f32,
    pub active_at: Instant,
    pub layout_height: f32,
    pub muted_from: Option<f32>,
}

impl Default for MiniState {
    fn default() -> Self {
        Self {
            open: false,
            placed: false,
            swipe: None,
            press: None,
            marquee: None,
            layout: None,
            prev: None,
            opacity: 1.0,
            active_at: Instant::now(),
            layout_height: 0.0,
            muted_from: None,
        }
    }
}

impl MiniState {
    fn reset_view(&mut self) {
        self.placed = false;
        self.swipe = None;
        self.press = None;
        self.marquee = None;
        self.layout = None;
        self.prev = None;
        self.layout_height = 0.0;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.opacity = 0.0;
            self.active_at = Instant::now();
        }
        self.reset_view();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.reset_view();
    }
}

pub const MIN_OPACITY: f32 = 0.3;

pub const FADE_FACTOR: f32 = 0.45;

pub const FADE_IDLE: Duration = Duration::from_millis(2500);

pub fn fade_target(base: f32, auto_fade: bool, active: bool) -> f32 {
    let base = base.clamp(MIN_OPACITY, 1.0);
    if auto_fade && !active {
        (base * FADE_FACTOR).max(MIN_OPACITY * FADE_FACTOR)
    } else {
        base
    }
}

pub fn approach_opacity(current: f32, target: f32, dt: f32) -> f32 {
    let step = (dt * 9.0).clamp(0.0, 1.0);
    current + (target - current) * step
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.mini.open {
        return;
    }

    let width = app.settings.mini.width.clamp(MIN_WIDTH, MAX_WIDTH);
    let height = app.settings.mini.height.clamp(MIN_HEIGHT, MAX_HEIGHT);

    let monitor = ctx.input(|input| input.viewport().monitor_size);
    let mut position = match (app.settings.mini.x, app.settings.mini.y) {
        (Some(x), Some(y)) => Some(Pos2::new(x, y)),
        _ => monitor.map(|monitor| {
            Pos2::new(
                ((monitor.x - width) * 0.5).max(0.0),
                (monitor.y - height - 90.0).max(0.0),
            )
        }),
    };
    if let (Some(pos), Some(monitor)) = (position.as_mut(), monitor) {
        pos.x = pos.x.clamp(0.0, (monitor.x - width).max(0.0));
        pos.y = pos.y.clamp(0.0, (monitor.y - height).max(0.0));
    }

    let mut builder = egui::ViewportBuilder::default()
        .with_title("Aria")
        .with_decorations(false)
        .with_always_on_top()
        .with_resizable(true)
        .with_min_inner_size([MIN_WIDTH, MIN_HEIGHT])
        .with_max_inner_size([MAX_WIDTH, MAX_HEIGHT]);

    if !app.mini.placed {
        builder = builder.with_inner_size([width, height]);
        if let Some(pos) = position {
            builder = builder.with_position([pos.x, pos.y]);
        }
    }

    let id = viewport_id();
    let main_ctx = ctx.clone();
    ctx.show_viewport_immediate(
        id,
        builder,
        |ui: &mut Ui, _class: egui::ViewportClass| {
            app.mini.placed = true;
            let ctx = ui.ctx().clone();
            let (close_requested, outer_rect) = ctx.input(|input| {
                let vp = input.viewport();
                (vp.close_requested(), vp.outer_rect)
            });

            if close_requested {
                app.close_mini(&ctx);
                return;
            }

            let rect = ui.max_rect();
            let width = rect.width().clamp(MIN_WIDTH, MAX_WIDTH);
            let height = rect.height().clamp(MIN_HEIGHT, MAX_HEIGHT);
            let position = outer_rect.map(|outer| (outer.min.x, outer.min.y));
            let moved = match (position, app.settings.mini.x, app.settings.mini.y) {
                (Some((x, y)), Some(ox), Some(oy)) => (x - ox).abs() > 0.5 || (y - oy).abs() > 0.5,
                _ => true,
            };
            let resized = (app.settings.mini.width - width).abs() > 0.5
                || (app.settings.mini.height - height).abs() > 0.5;
            app.settings.mini.width = width;
            app.settings.mini.height = height;
            if let Some((x, y)) = position {
                app.settings.mini.x = Some(x);
                app.settings.mini.y = Some(y);
            }
            if moved || resized {
                app.flush_mini_geometry(&main_ctx);
            }

            let accent = accent(&app.settings);
            let pal = palette(app.settings.theme);
            paint_shell(ui, rect, &pal);
            background_drag(ui, rect);

            draw_layout(MiniLayout::for_height(rect.height()), app, ui, rect, accent, &pal);

            resize_borders(ui, rect, &pal);
            move_handle(ui, rect, &pal);
            drive_move(ui);
            close_button(app, ui, rect, &accent, &pal);
            apply_opacity(app, ui, rect, outer_rect);

            if app.is_playing() {
                ctx.request_repaint_after(Duration::from_millis(120));
            }
        },
    );
}

fn draw_layout(
    layout: MiniLayout,
    app: &mut App,
    ui: &mut Ui,
    rect: Rect,
    accent: Color32,
    pal: &Palette,
) {
    match layout {
        MiniLayout::Compact => compact(app, ui, rect, accent, pal),
        MiniLayout::Medium => medium(app, ui, rect, accent, pal),
        MiniLayout::Large => large(app, ui, rect, accent, pal),
    }
}

fn apply_opacity(app: &mut App, ui: &Ui, rect: Rect, outer: Option<Rect>) {
    let hovering = ui.rect_contains_pointer(rect);
    if hovering {
        app.mini.active_at = Instant::now();
    }
    let idle = !hovering && app.mini.active_at.elapsed() > FADE_IDLE;
    let target = fade_target(app.settings.mini_opacity, app.settings.mini_auto_fade, !idle);
    let dt = ui.input(|input| input.stable_dt).min(0.1);
    let next = approach_opacity(app.mini.opacity, target, dt);
    if (next - app.mini.opacity).abs() > 0.002 {
        app.mini.opacity = next;
        ui.ctx().request_repaint_after(Duration::from_millis(24));
    }
    if let Some(outer) = outer {
        let scale = ui.ctx().pixels_per_point();
        layer::set_opacity(outer, scale, app.mini.opacity);
    }
}

#[cfg(windows)]
mod layer {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicU8, Ordering};

    use egui::Rect;

    type Hwnd = *mut c_void;

    #[repr(C)]
    struct WinRect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: extern "system" fn(Hwnd, isize) -> i32, lparam: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut WinRect) -> i32;
        fn IsWindowVisible(hwnd: Hwnd) -> i32;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
        fn SetLayeredWindowAttributes(hwnd: Hwnd, key: u32, alpha: u8, flags: u32) -> i32;
    }

    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_LAYERED: isize = 0x0008_0000;
    const LWA_ALPHA: u32 = 0x0000_0002;

    struct Search {
        pid: u32,
        bounds: (i32, i32, i32, i32),
        found: Hwnd,
    }

    static LAST_ALPHA: AtomicU8 = AtomicU8::new(0);

    extern "system" fn visit(hwnd: Hwnd, lparam: isize) -> i32 {
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut pid);
        }
        if pid != search.pid || unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        let mut rect = WinRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            GetWindowRect(hwnd, &mut rect);
        }
        let (x, y, w, h) = search.bounds;
        if (rect.left - x).abs() <= 2
            && (rect.top - y).abs() <= 2
            && (rect.right - rect.left - w).abs() <= 2
            && (rect.bottom - rect.top - h).abs() <= 2
        {
            search.found = hwnd;
            return 0;
        }
        1
    }

    pub fn set_opacity(outer: Rect, scale: f32, opacity: f32) {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8;
        if LAST_ALPHA.swap(alpha, Ordering::Relaxed) == alpha {
            return;
        }
        let mut search = Search {
            pid: std::process::id(),
            bounds: (
                (outer.min.x * scale).round() as i32,
                (outer.min.y * scale).round() as i32,
                (outer.width() * scale).round() as i32,
                (outer.height() * scale).round() as i32,
            ),
            found: std::ptr::null_mut(),
        };
        unsafe {
            EnumWindows(visit, &mut search as *mut Search as isize);
        }
        if search.found.is_null() {
            return;
        }
        unsafe {
            let style = GetWindowLongPtrW(search.found, GWL_EXSTYLE);
            if style & WS_EX_LAYERED == 0 {
                SetWindowLongPtrW(search.found, GWL_EXSTYLE, style | WS_EX_LAYERED);
            }
            SetLayeredWindowAttributes(search.found, 0, alpha, LWA_ALPHA);
        }
    }
}

#[cfg(not(windows))]
mod layer {
    use egui::Rect;

    pub fn set_opacity(_outer: Rect, _scale: f32, _opacity: f32) {}
}

fn paint_shell(ui: &Ui, rect: Rect, pal: &Palette) {
    ui.painter().rect_filled(rect, CornerRadius::ZERO, pal.bg);
    ui.painter().rect_stroke(
        rect.shrink(0.5),
        CornerRadius::same(2),
        Stroke::new(1.0, pal.line),
        StrokeKind::Inside,
    );
}

pub const EDGE_ZONE: f32 = 16.0;
pub const CORNER_ZONE: f32 = 28.0;
pub const GRIP_SIZE: f32 = 28.0;

pub const PRESETS: [(&str, f32, f32); 3] = [
    ("Compact", 280.0, 80.0),
    ("Medium", 340.0, 200.0),
    ("Large", 340.0, 440.0),
];

fn close_hit(rect: Rect) -> Rect {
    Rect::from_min_size(egui::pos2(rect.max.x - 34.0, rect.min.y), Vec2::splat(34.0))
}

fn move_hit(rect: Rect) -> Rect {
    Rect::from_min_size(rect.min, Vec2::splat(34.0))
}

pub fn zone_at(rect: Rect, pos: Pos2) -> Option<egui::ResizeDirection> {
    use egui::ResizeDirection as D;
    if !rect.contains(pos) || close_hit(rect).contains(pos) || move_hit(rect).contains(pos) {
        return None;
    }
    let left = pos.x - rect.min.x;
    let right = rect.max.x - pos.x;
    let top = pos.y - rect.min.y;
    let bottom = rect.max.y - pos.y;
    let near_x = left < CORNER_ZONE || right < CORNER_ZONE;
    let near_y = top < CORNER_ZONE || bottom < CORNER_ZONE;
    if near_x && near_y {
        return Some(match (left < right, top < bottom) {
            (true, true) => D::NorthWest,
            (false, true) => D::NorthEast,
            (true, false) => D::SouthWest,
            (false, false) => D::SouthEast,
        });
    }
    if left < EDGE_ZONE {
        Some(D::West)
    } else if right < EDGE_ZONE {
        Some(D::East)
    } else if top < EDGE_ZONE {
        Some(D::North)
    } else if bottom < EDGE_ZONE {
        Some(D::South)
    } else {
        None
    }
}

fn is_manual(direction: egui::ResizeDirection) -> bool {
    use egui::ResizeDirection as D;
    matches!(direction, D::East | D::South | D::SouthEast)
}

fn cursor_for(direction: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::{CursorIcon as C, ResizeDirection as D};
    match direction {
        D::North | D::South => C::ResizeVertical,
        D::East | D::West => C::ResizeHorizontal,
        D::NorthWest | D::SouthEast => C::ResizeNwSe,
        D::NorthEast | D::SouthWest => C::ResizeNeSw,
    }
}

fn edges(direction: egui::ResizeDirection) -> (bool, bool, bool, bool) {
    use egui::ResizeDirection as D;
    let west = matches!(direction, D::West | D::NorthWest | D::SouthWest);
    let east = matches!(direction, D::East | D::NorthEast | D::SouthEast);
    let north = matches!(direction, D::North | D::NorthWest | D::NorthEast);
    let south = matches!(direction, D::South | D::SouthWest | D::SouthEast);
    (west, east, north, south)
}

pub fn resize_rect(direction: egui::ResizeDirection, start: Rect, delta: Vec2) -> Rect {
    let (west, east, north, south) = edges(direction);
    let mut min = start.min;
    let mut max = start.max;
    if west {
        min.x = (min.x + delta.x).clamp(max.x - MAX_WIDTH, max.x - MIN_WIDTH);
    }
    if east {
        max.x = (max.x + delta.x).clamp(min.x + MIN_WIDTH, min.x + MAX_WIDTH);
    }
    if north {
        min.y = (min.y + delta.y).clamp(max.y - MAX_HEIGHT, max.y - MIN_HEIGHT);
    }
    if south {
        max.y = (max.y + delta.y).clamp(min.y + MIN_HEIGHT, min.y + MAX_HEIGHT);
    }
    Rect::from_min_max(min, max)
}

#[cfg(windows)]
mod native {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    extern "system" {
        fn GetCursorPos(point: *mut Point) -> i32;
        fn GetAsyncKeyState(key: i32) -> i16;
    }

    pub fn cursor_pixels() -> Option<(f32, f32)> {
        let mut point = Point { x: 0, y: 0 };
        let ok = unsafe { GetCursorPos(&mut point) };
        (ok != 0).then_some((point.x as f32, point.y as f32))
    }

    pub fn left_button_down() -> bool {
        unsafe { GetAsyncKeyState(0x01) < 0 }
    }
}

#[cfg(not(windows))]
mod native {
    pub fn cursor_pixels() -> Option<(f32, f32)> {
        None
    }

    pub fn left_button_down() -> bool {
        false
    }
}

type ResizeStart = (egui::ResizeDirection, Rect, Pos2, bool);
type MoveStart = (Rect, Pos2, bool);

fn screen_cursor(ui: &Ui, outer: Rect, local: Option<Pos2>, touch: bool) -> Option<Pos2> {
    if touch {
        return local.map(|pos| outer.min + pos.to_vec2());
    }
    if let Some((x, y)) = native::cursor_pixels() {
        let scale = ui.ctx().pixels_per_point().max(0.01);
        return Some(egui::pos2(x / scale, y / scale));
    }
    local.map(|pos| outer.min + pos.to_vec2())
}

fn resize_borders(ui: &mut Ui, rect: Rect, pal: &Palette) {
    use egui::ViewportCommand;
    let drag_id = ui.scope_id().with("mini-manual-resize");
    let (press, latest, down, hover, outer, touching) = ui.input(|input| {
        (
            if input.pointer.primary_pressed() { input.pointer.press_origin() } else { None },
            input.pointer.latest_pos(),
            input.pointer.primary_down(),
            input.pointer.hover_pos(),
            input.viewport().outer_rect,
            input.any_touches(),
        )
    });

    if let Some(origin) = press {
        if let Some(direction) = zone_at(rect, origin) {
            let manual_all = cfg!(windows) && outer.is_some();
            if manual_all || is_manual(direction) {
                let start_outer = outer.unwrap_or(rect);
                let start_cursor = screen_cursor(ui, start_outer, Some(origin), touching);
                if let Some(start_cursor) = start_cursor {
                    let start: ResizeStart = (direction, start_outer, start_cursor, touching);
                    ui.data_mut(|data| data.insert_temp(drag_id, start));
                }
            } else {
                ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(direction));
            }
        }
    }

    let active: Option<ResizeStart> = ui.data(|data| data.get_temp(drag_id));
    if let Some((direction, start_outer, start_cursor, touch)) = active {
        if down || (!touch && native::left_button_down()) {
            let current_outer = outer.unwrap_or(start_outer);
            if let Some(cursor) = screen_cursor(ui, current_outer, latest, touch) {
                let target = resize_rect(direction, start_outer, cursor - start_cursor);
                if (target.min - current_outer.min).length() > 0.5 {
                    ui.ctx().send_viewport_cmd(ViewportCommand::OuterPosition(target.min));
                }
                if (target.size() - current_outer.size()).length() > 0.5 {
                    ui.ctx().send_viewport_cmd(ViewportCommand::InnerSize(target.size()));
                }
            }
            ui.ctx().set_cursor_icon(cursor_for(direction));
            ui.ctx().request_repaint();
        } else {
            ui.data_mut(|data| data.remove::<ResizeStart>(drag_id));
        }
    } else if let Some(direction) = hover.and_then(|pos| zone_at(rect, pos)) {
        ui.ctx().set_cursor_icon(cursor_for(direction));
    }

    if rect.height() >= 120.0 {
        let grip = Rect::from_min_max(
            egui::pos2(rect.max.x - GRIP_SIZE * 0.6, rect.max.y - GRIP_SIZE * 0.6),
            egui::pos2(rect.max.x - 5.0, rect.max.y - 5.0),
        );
        let stroke = Stroke::new(1.5, pal.muted);
        for step in 0..3 {
            let offset = step as f32 * 4.5;
            ui.painter().line_segment(
                [
                    egui::pos2(grip.max.x - offset, grip.max.y),
                    egui::pos2(grip.max.x, grip.max.y - offset),
                ],
                stroke,
            );
        }
    }
}

fn move_drag_id(ui: &Ui) -> egui::Id {
    ui.scope_id().with("mini-move-window")
}

// Manual move on Windows, mirroring `resize_borders`: `ViewportCommand::StartDrag` goes
// through Win32's caption modal loop (WM_NCLBUTTONDOWN/HTCAPTION), which is mouse-only and
// never engages for touch/pen input (Surface Pro etc.). Track the drag start and move the
// window with `OuterPosition` each frame instead.
fn start_move(ui: &mut Ui, rect: Rect, origin: Option<Pos2>) {
    let drag_id = move_drag_id(ui);
    let (outer, touching) = ui.input(|input| (input.viewport().outer_rect, input.any_touches()));
    if cfg!(windows) && outer.is_some() {
        let start_outer = outer.unwrap_or(rect);
        if let Some(start_cursor) = screen_cursor(ui, start_outer, origin, touching) {
            let start: MoveStart = (start_outer, start_cursor, touching);
            ui.data_mut(|data| data.insert_temp(drag_id, start));
        }
    } else {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

fn drive_move(ui: &mut Ui) {
    let drag_id = move_drag_id(ui);
    let (latest, down, outer, touching) = ui.input(|input| {
        (
            input.pointer.latest_pos(),
            input.pointer.primary_down(),
            input.viewport().outer_rect,
            input.any_touches(),
        )
    });
    let active: Option<MoveStart> = ui.data(|data| data.get_temp(drag_id));
    if let Some((start_outer, start_cursor, touch)) = active {
        let touch = touch || touching;
        if down || (!touch && native::left_button_down()) {
            let current_outer = outer.unwrap_or(start_outer);
            if let Some(cursor) = screen_cursor(ui, current_outer, latest, touch) {
                let target = start_outer.min + (cursor - start_cursor);
                if (target - current_outer.min).length() > 0.5 {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::OuterPosition(target));
                }
            }
            ui.ctx().request_repaint();
        } else {
            ui.data_mut(|data| data.remove::<MoveStart>(drag_id));
        }
    }
}

fn background_drag(ui: &mut Ui, rect: Rect) {
    let response = ui.interact(rect, ui.scope_id().with("mini-bg"), Sense::drag());
    let origin = ui.input(|input| {
        if input.pointer.primary_pressed() {
            input.pointer.press_origin()
        } else {
            None
        }
    });
    let on_edge = origin.is_some_and(|pos| zone_at(rect, pos).is_some() || close_hit(rect).contains(pos));
    if response.drag_started() && !on_edge {
        start_move(ui, rect, origin);
    }
}

fn move_handle(ui: &mut Ui, rect: Rect, pal: &Palette) {
    let hit = move_hit(rect);
    let button = Rect::from_center_size(hit.center(), Vec2::splat(26.0));
    let response = ui.interact(hit, ui.scope_id().with("mini-move"), Sense::drag());
    let active = response.hovered() || response.is_pointer_button_down_on();
    ui.painter().rect_filled(
        button,
        CornerRadius::same(13),
        if active { pal.raised } else { pal.bg.gamma_multiply(0.85) },
    );
    let dot = if active { pal.text } else { pal.muted };
    for row in -1..=1 {
        for col in [-1.0f32, 1.0] {
            let center = button.center() + Vec2::new(col * 2.8, row as f32 * 3.0);
            ui.painter().circle_filled(center, 1.35, dot);
        }
    }
    if active {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
    }
    if ui.input(|input| input.pointer.primary_pressed()) {
        if let Some(origin) = ui.input(|input| input.pointer.press_origin()) {
            if hit.contains(origin) {
                start_move(ui, rect, Some(origin));
            }
        }
    }
}

fn close_button(app: &mut App, ui: &mut Ui, rect: Rect, accent: &Color32, pal: &Palette) {
    let hit = close_hit(rect);
    let button = Rect::from_center_size(hit.center(), Vec2::splat(26.0));
    let response = ui.interact(hit, ui.scope_id().with("mini-close"), Sense::click());
    let active = response.hovered() || response.is_pointer_button_down_on();
    ui.painter().rect_filled(
        button,
        CornerRadius::same(13),
        if active { pal.raised } else { pal.bg.gamma_multiply(0.85) },
    );
    icons::draw(
        ui.painter(),
        Rect::from_center_size(button.center(), Vec2::splat(12.0)),
        Icon::Close,
        if active { *accent } else { pal.muted },
    );
    let tapped = ui.input(|input| {
        input.pointer.any_released()
            && input.pointer.interact_pos().is_some_and(|pos| hit.contains(pos))
            && input.pointer.press_origin().is_none_or(|pos| hit.contains(pos))
    });
    if response.clicked() || tapped {
        let ctx = ui.ctx().clone();
        app.close_mini(&ctx);
    }
}

fn icon_button(ui: &mut Ui, id: egui::Id, rect: Rect, icon: Icon, tint: Color32, active: bool) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    let radius = CornerRadius::same(10);
    if active {
        ui.painter()
            .rect_filled(rect, radius, tint.gamma_multiply(0.2));
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, radius, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    icons::draw(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(rect.width().min(rect.height()) * 0.46)),
        icon,
        tint,
    );
    response
}

fn play_button(ui: &mut Ui, id: egui::Id, rect: Rect, playing: bool, accent: Color32) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    let _ = accent;
    let radius = rect.width() * 0.5 * if response.hovered() { 1.06 } else { 1.0 };
    ui.painter().circle_filled(rect.center(), radius, Color32::WHITE);
    crate::ui::widgets::paint_play_glyph(ui, rect, if playing { Icon::Pause } else { Icon::Play }, Color32::from_rgb(12, 12, 15));
    response
}

fn seek_bar(
    ui: &mut Ui,
    rect: Rect,
    position: f64,
    duration: f64,
    accent: Color32,
    pal: &Palette,
) -> Option<f64> {
    let response = ui.interact(rect, egui::Id::unique("mini-seek"), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), pal.raised);
    let fraction = if duration > 0.0 {
        (position / duration).clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * fraction, track.height()),
    );
    if fraction > 0.0 {
        ui.painter()
            .rect_filled(filled, CornerRadius::same(2), accent);
    }
    let active = response.hovered() || response.dragged();
    if duration > 0.0 {
        ui.painter().circle_filled(
            egui::pos2(filled.max.x, track.center().y),
            if active { 7.0 } else { 5.0 },
            if active { Color32::WHITE } else { accent },
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

fn volume_slider(
    ui: &mut Ui,
    rect: Rect,
    value: f32,
    accent: Color32,
    pal: &Palette,
) -> Option<f32> {
    let response = ui.interact(rect, egui::Id::unique("mini-volume"), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter()
        .rect_filled(track, CornerRadius::same(2), pal.raised);
    let value = value.clamp(0.0, 1.0);
    let filled = Rect::from_min_size(
        track.min,
        Vec2::new(track.width() * value, track.height()),
    );
    ui.painter()
        .rect_filled(filled, CornerRadius::same(2), accent);
    if response.hovered() || response.dragged() {
        ui.painter()
            .circle_filled(egui::pos2(filled.max.x, track.center().y), 6.0, Color32::WHITE);
    }
    if let Some(pointer) = response.interact_pointer_pos() {
        if response.clicked() || response.dragged() {
            let relative = (pointer.x - track.min.x) / track.width().max(1.0);
            return Some(relative.clamp(0.0, 1.0));
        }
    }
    None
}

fn art(ui: &Ui, rect: Rect, texture: Option<&egui::TextureHandle>, accent: Color32, pal: &Palette) {
    crate::ui::widgets::paint_cover(ui.painter(), rect, texture, 12, accent, pal);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(12),
        Stroke::new(1.0, pal.line),
        StrokeKind::Inside,
    );
}

fn track_title(app: &App) -> String {
    app.current
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| "Nothing playing".to_string())
}

fn track_artist(app: &App) -> String {
    app.current
        .as_ref()
        .map(|track| track.display_artist().to_string())
        .unwrap_or_else(|| "Add a local file to start".to_string())
}

fn scroll_offset(ui: &Ui, app: &mut App, text: &str, width: f32) -> f32 {
    let speed = 28.0;
    let measured = ui
        .painter()
        .layout_no_wrap(
            text.to_owned(),
            FontId::proportional(12.5),
            Color32::WHITE,
        )
        .size()
        .x;
    if measured <= width {
        return 0.0;
    }
    match &app.mini.marquee {
        Some((current, start)) if current == text => {
            let hold = 2.5;
            let cycle = hold + (measured + 48.0) / speed;
            let time = start.elapsed().as_secs_f32() % cycle;
            if time < hold {
                0.0
            } else {
                (time - hold) * speed
            }
        }
        _ => {
            app.mini.marquee = Some((text.to_string(), Instant::now()));
            0.0
        }
    }
}

fn paint_scrolled_text(ui: &Ui, rect: Rect, text: &str, size: f32, color: Color32, offset: f32) {
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(size), color);
    let clip = painter.with_clip_rect(rect);
    clip.galley(
        egui::pos2(rect.min.x - offset, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

fn transport(app: &mut App, ui: &mut Ui, row: Rect, accent: Color32, pal: &Palette, with_small: bool) {
    let play_size = 46.0;
    let small = if with_small { 40.0 } else { 34.0 };
    let gap = 4.0;
    let total = if with_small {
        small * 4.0 + play_size + gap * 4.0
    } else {
        small * 2.0 + play_size + gap * 2.0
    };
    let mut x = row.center().x - total * 0.5;
    let center_y = row.center().y;

    if with_small {
        let shuffle_on = app.settings.shuffle;
        let rect = Rect::from_center_size(egui::pos2(x + small * 0.5, center_y), Vec2::splat(small));
        if icon_button(
            ui,
            egui::Id::unique("mini-shuffle"),
            rect,
            Icon::Shuffle,
            if shuffle_on { accent } else { pal.muted },
            shuffle_on,
        )
        .clicked()
        {
            app.toggle_shuffle();
        }
        x += small + gap;
    }

    let prev = Rect::from_center_size(egui::pos2(x + small * 0.5, center_y), Vec2::splat(small));
    if icon_button(ui, egui::Id::unique("mini-prev"), prev, Icon::Prev, pal.text, false).clicked() {
        app.previous();
    }
    x += small + gap;

    let play = Rect::from_center_size(egui::pos2(x + play_size * 0.5, center_y), Vec2::splat(play_size));
    if play_button(ui, egui::Id::unique("mini-play"), play, app.is_playing(), accent).clicked() {
        app.toggle_play();
    }
    x += play_size + gap;

    let next = Rect::from_center_size(egui::pos2(x + small * 0.5, center_y), Vec2::splat(small));
    if icon_button(ui, egui::Id::unique("mini-next"), next, Icon::Next, pal.text, false).clicked() {
        app.play_advance(false);
    }
    x += small + gap;

    if with_small {
        let (icon, on) = match app.settings.repeat {
            RepeatMode::Off => (Icon::Repeat, false),
            RepeatMode::All => (Icon::Repeat, true),
            RepeatMode::One => (Icon::RepeatOne, true),
        };
        let rect = Rect::from_center_size(egui::pos2(x + small * 0.5, center_y), Vec2::splat(small));
        if icon_button(
            ui,
            egui::Id::unique("mini-repeat"),
            rect,
            icon,
            if on { accent } else { pal.muted },
            on,
        )
        .clicked()
        {
            app.cycle_repeat();
        }
    }
}

fn like_button(app: &mut App, ui: &mut Ui, rect: Rect, accent: Color32, pal: &Palette) {
    let liked = app.liked_current();
    if icon_button(
        ui,
        egui::Id::unique("mini-like"),
        rect,
        if liked { Icon::HeartFilled } else { Icon::Heart },
        if liked { accent } else { pal.muted },
        liked,
    )
    .clicked()
    {
        app.toggle_like_current();
    }
}

pub fn fit_text(text: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    if measure(text) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0usize, chars.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>().trim_end().to_string() + "…";
        if measure(&candidate) <= max_width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    if low == 0 {
        return "…".to_string();
    }
    chars[..low].iter().collect::<String>().trim_end().to_string() + "…"
}

fn text_line(ui: &Ui, rect: Rect, text: &str, size: f32, color: Color32, align: Align2) {
    let font = FontId::proportional(size);
    let painter = ui.painter();
    let fitted = fit_text(text, rect.width(), |candidate| {
        painter
            .layout_no_wrap(candidate.to_owned(), font.clone(), color)
            .size()
            .x
    });
    let anchor = match align {
        Align2::RIGHT_CENTER => egui::pos2(rect.max.x, rect.center().y),
        Align2::CENTER_CENTER => rect.center(),
        _ => egui::pos2(rect.min.x, rect.center().y),
    };
    painter.with_clip_rect(rect).text(anchor, align, fitted, font, color);
}

fn volume_row(app: &mut App, ui: &mut Ui, row: Rect, accent: Color32, pal: &Palette) {
    let mute = Rect::from_center_size(
        egui::pos2(row.min.x + 18.0, row.center().y),
        Vec2::splat(36.0),
    );
    let muted = app.settings.volume <= 0.0;
    if icon_button(
        ui,
        egui::Id::unique("mini-mute"),
        mute,
        Icon::Volume,
        if muted { pal.line } else { pal.muted },
        false,
    )
    .clicked()
    {
        if muted {
            let restore = app.mini.muted_from.take().unwrap_or(0.7);
            app.set_volume(restore);
        } else {
            app.mini.muted_from = Some(app.settings.volume);
            app.set_volume(0.0);
        }
    }
    let slider = Rect::from_min_max(
        egui::pos2(mute.max.x + 6.0, row.center().y - 16.0),
        egui::pos2(row.max.x - 6.0, row.center().y + 16.0),
    );
    if let Some(value) = volume_slider(ui, slider, app.settings.volume, accent, pal) {
        app.set_volume(value);
    }
}

fn swipe_zone(app: &mut App, ui: &mut Ui, rect: Rect, salt: &'static str) {
    let response = ui.interact(rect, ui.scope_id().with(salt), Sense::drag());
    if response.drag_started() {
        app.mini.swipe = response.interact_pointer_pos();
        app.mini.press = None;
    }
    if response.drag_stopped() {
        if let (Some(start), Some(end)) = (app.mini.swipe.take(), response.interact_pointer_pos()) {
            let dx = end.x - start.x;
            if dx < -60.0 {
                app.play_advance(false);
            } else if dx > 60.0 {
                app.previous();
            }
        }
    }
}

fn gestures(app: &mut App, ui: &mut Ui, rect: Rect) {
    let response = ui.interact(rect, ui.scope_id().with("mini-gesture"), Sense::click_and_drag());

    if response.drag_started() {
        app.mini.swipe = response.interact_pointer_pos();
        app.mini.press = None;
    }

    if response.drag_stopped() {
        if let (Some(start), Some(end)) = (app.mini.swipe.take(), response.interact_pointer_pos()) {
            let dx = end.x - start.x;
            if dx < -60.0 {
                app.play_advance(false);
            } else if dx > 60.0 {
                app.previous();
            }
        }
        return;
    }

    if response.is_pointer_button_down_on() && !response.dragged() {
        let press = app.mini.press.get_or_insert((Instant::now(), false));
        if !press.1 && press.0.elapsed() >= Duration::from_millis(600) {
            press.1 = true;
            app.toggle_like_current();
        }
    }

    if response.clicked() {
        let long_pressed = app.mini.press.take().map(|(_, fired)| fired).unwrap_or(false);
        if !long_pressed {
            let ctx = ui.ctx().clone();
            app.close_mini(&ctx);
        }
    } else if !response.is_pointer_button_down_on() {
        app.mini.press = None;
    }
}

fn seek_row(app: &mut App, ui: &mut Ui, row: Rect, accent: Color32, pal: &Palette) {
    let position = app.position();
    let duration = app.duration().unwrap_or(0.0);
    let label = 38.0;
    let left = Rect::from_min_size(row.min, Vec2::new(label, row.height()));
    let right = Rect::from_min_max(egui::pos2(row.max.x - label, row.min.y), row.max);
    let bar = Rect::from_min_max(egui::pos2(left.max.x + 4.0, row.min.y), egui::pos2(right.min.x - 4.0, row.max.y));
    text_line(ui, left, &fmt_time(position), 11.0, pal.muted, Align2::LEFT_CENTER);
    text_line(ui, right, &fmt_optional_time((duration > 0.0).then_some(duration)), 11.0, pal.muted, Align2::RIGHT_CENTER);
    if let Some(target) = seek_bar(ui, bar, position, duration, accent, pal) {
        app.seek(target);
    }
}

fn info_block(app: &mut App, ui: &mut Ui, title: Rect, artist: Rect, scale: f32, pal: &Palette) {
    let name = track_title(app);
    let offset = scroll_offset(ui, app, &name, title.width());
    paint_scrolled_text(ui, title, &name, 15.5 * scale, pal.text, offset);
    text_line(ui, artist, &track_artist(app), 12.5 * scale, pal.muted, Align2::LEFT_CENTER);
    if app.mini.marquee.is_some() {
        ui.ctx().request_repaint_after(Duration::from_millis(40));
    }
}

fn large(app: &mut App, ui: &mut Ui, rect: Rect, accent: Color32, pal: &Palette) {
    let plan = mini_plan(rect.width(), rect.height());
    let pad = 16.0;
    let inner = rect.width() - pad * 2.0;
    let volume = Rect::from_min_max(egui::pos2(rect.min.x + pad, rect.max.y - pad - 34.0), egui::pos2(rect.max.x - pad, rect.max.y - pad));
    let transport_row = Rect::from_center_size(egui::pos2(rect.center().x, volume.min.y - 34.0), Vec2::new(inner, 52.0));
    let seek = Rect::from_center_size(egui::pos2(rect.center().x, transport_row.min.y - 22.0), Vec2::new(inner, 22.0));
    let heart = 40.0;
    let artist_rect = Rect::from_min_size(egui::pos2(rect.min.x + pad, seek.min.y - 26.0), Vec2::new(inner - heart - 6.0, 20.0));
    let title_rect = Rect::from_min_size(egui::pos2(rect.min.x + pad, artist_rect.min.y - 24.0), Vec2::new(inner - heart - 6.0, 24.0));
    let art_size = (title_rect.min.y - 12.0 - (rect.min.y + pad)).min(inner).max(56.0);
    let art_rect = Rect::from_min_size(egui::pos2(rect.center().x - art_size * 0.5, rect.min.y + pad), Vec2::splat(art_size));

    let current_art = app.current_art();
    art(ui, art_rect, current_art.as_ref(), accent, pal);
    gestures(app, ui, art_rect);
    swipe_zone(app, ui, Rect::from_min_max(title_rect.min, artist_rect.max), "mini-swipe-large");
    info_block(app, ui, title_rect, artist_rect, plan.scale.min(1.15), pal);
    like_button(app, ui, Rect::from_center_size(egui::pos2(rect.max.x - pad - heart * 0.5, (title_rect.min.y + artist_rect.max.y) * 0.5), Vec2::splat(heart)), accent, pal);
    seek_row(app, ui, seek, accent, pal);
    transport(app, ui, transport_row, accent, pal, true);
    volume_row(app, ui, volume, accent, pal);
}

fn medium(app: &mut App, ui: &mut Ui, rect: Rect, accent: Color32, pal: &Palette) {
    if rect.height() < 180.0 {
        return compact(app, ui, rect, accent, pal);
    }
    let plan = mini_plan(rect.width(), rect.height());
    let pad = 16.0;
    let inner = rect.width() - pad * 2.0;
    let extra = if plan.show_volume { 36.0 } else { 0.0 };
    let art_size = (rect.height() - pad * 2.0 - 46.0 - 8.0 - 22.0 - 10.0 - extra).clamp(48.0, 104.0);
    let art_rect = Rect::from_min_size(egui::pos2(rect.min.x + pad, rect.min.y + pad), Vec2::splat(art_size));
    let current_art = app.current_art();
    art(ui, art_rect, current_art.as_ref(), accent, pal);
    gestures(app, ui, art_rect);

    let text_x = art_rect.max.x + 12.0;
    let text_w = (rect.max.x - 38.0 - text_x).max(40.0);
    let title_rect = Rect::from_min_size(egui::pos2(text_x, art_rect.min.y + 4.0), Vec2::new(text_w, 24.0));
    let artist_rect = Rect::from_min_size(egui::pos2(text_x, title_rect.max.y), Vec2::new(text_w, 20.0));
    swipe_zone(app, ui, Rect::from_min_max(title_rect.min, artist_rect.max), "mini-swipe-medium");
    info_block(app, ui, title_rect, artist_rect, plan.scale.min(1.15), pal);
    like_button(app, ui, Rect::from_center_size(egui::pos2(rect.max.x - pad - 18.0, art_rect.max.y - 20.0), Vec2::splat(36.0)), accent, pal);

    let seek = Rect::from_min_size(egui::pos2(rect.min.x + pad, art_rect.max.y + 10.0), Vec2::new(inner, 22.0));
    seek_row(app, ui, seek, accent, pal);
    let bottom = if plan.show_volume { rect.max.y - pad - extra } else { rect.max.y - pad };
    let center_y = (seek.max.y + bottom) * 0.5;
    transport(app, ui, Rect::from_center_size(egui::pos2(rect.center().x, center_y), Vec2::new(inner, 48.0)), accent, pal, rect.width() >= 360.0);
    if plan.show_volume {
        volume_row(app, ui, Rect::from_min_max(egui::pos2(rect.min.x + pad, rect.max.y - pad - 32.0), egui::pos2(rect.max.x - pad, rect.max.y - pad)), accent, pal);
    }
}

fn compact(app: &mut App, ui: &mut Ui, rect: Rect, accent: Color32, pal: &Palette) {
    let plan = mini_plan(rect.width(), rect.height());
    let pad = 8.0;
    let art_size = (rect.height() - pad * 2.0 - 4.0).clamp(40.0, 84.0);
    let art_rect = Rect::from_min_size(egui::pos2(rect.min.x + pad, rect.min.y + pad), Vec2::splat(art_size));
    let current_art = app.current_art();
    art(ui, art_rect, current_art.as_ref(), accent, pal);
    gestures(app, ui, art_rect);

    let bar = Rect::from_min_max(egui::pos2(rect.min.x + EDGE_ZONE, rect.max.y - EDGE_ZONE - 12.0), egui::pos2(rect.max.x - EDGE_ZONE, rect.max.y - EDGE_ZONE));
    if let Some(target) = thin_seek(app, ui, bar, accent, pal) {
        app.seek(target);
    }

    let text_x = art_rect.max.x + 10.0;
    let text_w = (rect.max.x - 38.0 - text_x).max(40.0);
    let title_rect = Rect::from_min_size(egui::pos2(text_x, rect.min.y + 10.0), Vec2::new(text_w, 20.0));
    let artist_rect = Rect::from_min_size(egui::pos2(text_x, title_rect.max.y), Vec2::new(text_w, 16.0));
    swipe_zone(app, ui, Rect::from_min_max(title_rect.min, artist_rect.max), "mini-swipe-compact");
    info_block(app, ui, title_rect, artist_rect, 0.95, pal);

    let small = 26.0;
    let play = 32.0;
    let cy = rect.max.y - EDGE_ZONE - 17.0;
    let mut x = rect.max.x - 10.0 - small * 0.5;
    if icon_button(ui, egui::Id::unique("mini-next"), Rect::from_center_size(egui::pos2(x, cy), Vec2::splat(small)), Icon::Next, pal.text, false).clicked() {
        app.play_advance(false);
    }
    x -= small * 0.5 + 2.0 + play * 0.5;
    if play_button(ui, egui::Id::unique("mini-play"), Rect::from_center_size(egui::pos2(x, cy), Vec2::splat(play)), app.is_playing(), accent).clicked() {
        app.toggle_play();
    }
    if plan.show_prev || rect.width() >= 340.0 {
        x -= play * 0.5 + 2.0 + small * 0.5;
        if icon_button(ui, egui::Id::unique("mini-prev"), Rect::from_center_size(egui::pos2(x, cy), Vec2::splat(small)), Icon::Prev, pal.text, false).clicked() {
            app.previous();
        }
    }
}

fn thin_seek(app: &mut App, ui: &mut Ui, rect: Rect, accent: Color32, pal: &Palette) -> Option<f64> {
    let duration = app.duration().unwrap_or(0.0);
    let fraction = if duration > 0.0 { (app.position() / duration).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let line = Rect::from_center_size(egui::pos2(rect.center().x, rect.max.y - 4.0), Vec2::new(rect.width(), 4.0));
    ui.painter().rect_filled(line, CornerRadius::same(2), pal.raised);
    ui.painter().rect_filled(Rect::from_min_size(line.min, Vec2::new(line.width() * fraction, 4.0)), CornerRadius::same(2), accent);
    let response = ui.interact(rect, egui::Id::unique("mini-thin-seek"), Sense::click_and_drag());
    let pointer = response.interact_pointer_pos()?;
    (duration > 0.0 && (response.clicked() || response.dragged())).then(|| ((pointer.x - rect.min.x) / rect.width().max(1.0)).clamp(0.0, 1.0) as f64 * duration)
}

#[cfg(test)]
mod tests {
    use super::{MiniLayout, MiniState};

    #[test]
    fn zones_use_edge_and_corner_sizes() {
        use super::zone_at;
        use egui::{pos2, Rect, ResizeDirection as D};
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(340.0, 440.0));
        assert_eq!(zone_at(rect, pos2(330.0, 430.0)), Some(D::SouthEast));
        assert_eq!(zone_at(rect, pos2(313.0, 413.0)), Some(D::SouthEast));
        assert_eq!(zone_at(rect, pos2(5.0, 5.0)), None, "the move handle owns the top-left corner");
        assert_eq!(zone_at(rect, pos2(5.0, 40.0)), Some(D::West));
        assert_eq!(zone_at(rect, pos2(5.0, 430.0)), Some(D::SouthWest));
        assert_eq!(zone_at(rect, pos2(170.0, 15.0)), Some(D::North));
        assert_eq!(zone_at(rect, pos2(170.0, 17.0)), None);
        assert_eq!(zone_at(rect, pos2(15.0, 200.0)), Some(D::West));
        assert_eq!(zone_at(rect, pos2(17.0, 200.0)), None);
        assert_eq!(zone_at(rect, pos2(330.0, 200.0)), Some(D::East));
        assert_eq!(zone_at(rect, pos2(170.0, 430.0)), Some(D::South));
        assert_eq!(zone_at(rect, pos2(325.0, 10.0)), None);
        assert_eq!(zone_at(rect, pos2(170.0, 220.0)), None);
    }

    #[test]
    fn resizing_any_edge_keeps_the_opposite_edge_fixed_and_respects_limits() {
        use super::resize_rect;
        use egui::{pos2, vec2, Rect, ResizeDirection as D};
        let start = Rect::from_min_max(pos2(500.0, 300.0), pos2(840.0, 740.0));
        let grown = resize_rect(D::West, start, vec2(-60.0, 0.0));
        assert_eq!((grown.min.x, grown.max.x), (440.0, 840.0));
        let taller = resize_rect(D::North, start, vec2(0.0, -80.0));
        assert_eq!((taller.min.y, taller.max.y), (220.0, 740.0));
        let corner = resize_rect(D::NorthWest, start, vec2(-20.0, -30.0));
        assert_eq!(corner.max, start.max);
        assert_eq!(corner.min, pos2(480.0, 270.0));
        let squeezed = resize_rect(D::West, start, vec2(900.0, 0.0));
        assert_eq!(squeezed.width(), 280.0);
        assert_eq!(squeezed.max.x, 840.0);
        let capped = resize_rect(D::NorthWest, start, vec2(-900.0, -900.0));
        assert_eq!((capped.width(), capped.height()), (480.0, 640.0));
        assert_eq!(capped.max, start.max);
        let se = resize_rect(D::SouthEast, start, vec2(30.0, 40.0));
        assert_eq!(se.min, start.min);
        assert_eq!(se.max, pos2(870.0, 780.0));
        let small = resize_rect(D::SouthEast, start, vec2(-900.0, -900.0));
        assert_eq!((small.width(), small.height()), (280.0, 80.0));
    }

    #[test]
    fn presets_sit_inside_limits() {
        use super::{MiniLayout, MAX_HEIGHT, MAX_WIDTH, MIN_HEIGHT, MIN_WIDTH, PRESETS};
        for (_, w, h) in PRESETS {
            assert!((MIN_WIDTH..=MAX_WIDTH).contains(&w));
            assert!((MIN_HEIGHT..=MAX_HEIGHT).contains(&h));
        }
        assert_eq!(MiniLayout::for_height(PRESETS[0].2), MiniLayout::Compact);
        assert_eq!(MiniLayout::for_height(PRESETS[1].2), MiniLayout::Medium);
        assert_eq!(MiniLayout::for_height(PRESETS[2].2), MiniLayout::Large);
    }

    #[test]
    fn fit_text_truncates_with_ellipsis() {
        use super::fit_text;
        let measure = |text: &str| text.chars().count() as f32 * 10.0;
        assert_eq!(fit_text("Short", 100.0, measure), "Short");
        assert_eq!(fit_text("Exactly10!", 100.0, measure), "Exactly10!");
        let cut = fit_text("A very long song title indeed", 100.0, measure);
        assert!(cut.ends_with('…'));
        assert!(measure(&cut) <= 100.0);
        assert_eq!(cut, "A very lo…");
        assert_eq!(fit_text("Anything", 5.0, measure), "…");
        assert_eq!(fit_text("", 0.0, measure), "");
        let trimmed = fit_text("ab cd ef gh ij kl", 60.0, measure);
        assert!(!trimmed.contains(" …"));
    }

    #[test]
    fn the_mini_plan_adapts_to_width_as_well_as_height() {
        use super::mini_plan;
        let narrow = mini_plan(280.0, 80.0);
        assert_eq!(narrow.layout, MiniLayout::Compact);
        assert!(narrow.scale < 0.9 && !narrow.show_prev);
        let wide_strip = mini_plan(480.0, 90.0);
        assert!(wide_strip.show_prev && wide_strip.scale > 1.1);
        assert!(!mini_plan(480.0, 200.0).show_prev, "medium already has previous");
        assert!(mini_plan(480.0, 220.0).show_volume);
        assert!(!mini_plan(340.0, 220.0).show_volume);
        assert!(!mini_plan(480.0, 150.0).show_volume);
        assert!(!mini_plan(480.0, 440.0).show_volume);
        for width in [280.0, 340.0, 480.0] {
            let scale = mini_plan(width, 300.0).scale;
            assert!((0.85..=1.2).contains(&scale));
        }
    }

    #[test]
    fn close_hides_miniplayer_and_resets_view() {
        let mut state = MiniState::default();
        state.toggle();
        assert!(state.open);
        state.placed = true;
        state.layout = Some(MiniLayout::Large);
        state.layout_height = 440.0;
        state.close();
        assert!(!state.open);
        assert!(!state.placed);
        assert!(state.layout.is_none());
        assert_eq!(state.layout_height, 0.0);
        state.close();
        assert!(!state.open);
        state.toggle();
        assert!(state.open);
    }

    #[test]
    fn opacity_fades_only_when_idle() {
        use super::{fade_target, MIN_OPACITY};
        assert_eq!(fade_target(1.0, false, true), 1.0);
        assert_eq!(fade_target(1.0, false, false), 1.0);
        assert_eq!(fade_target(1.0, true, true), 1.0);
        let faded = fade_target(1.0, true, false);
        assert!((MIN_OPACITY * 0.4..0.6).contains(&faded), "faded to {faded}");
        assert_eq!(fade_target(0.1, false, true), MIN_OPACITY);
        assert_eq!(fade_target(2.0, false, true), 1.0);
    }

    #[test]
    fn opacity_approaches_without_overshooting() {
        use super::approach_opacity;
        let mut value = 1.0f32;
        for _ in 0..120 {
            let next = approach_opacity(value, 0.45, 0.016);
            assert!(next <= value + f32::EPSILON);
            value = next;
        }
        assert!((value - 0.45).abs() < 0.02, "settled at {value}");
        assert_eq!(approach_opacity(0.5, 0.5, 0.016), 0.5);
    }

    #[test]
    fn layout_thresholds_match_the_spec() {
        assert_eq!(MiniLayout::for_height(80.0), MiniLayout::Compact);
        assert_eq!(MiniLayout::for_height(119.0), MiniLayout::Compact);
        assert_eq!(MiniLayout::for_height(120.0), MiniLayout::Medium);
        assert_eq!(MiniLayout::for_height(200.0), MiniLayout::Medium);
        assert_eq!(MiniLayout::for_height(300.0), MiniLayout::Medium);
        assert_eq!(MiniLayout::for_height(301.0), MiniLayout::Large);
        assert_eq!(MiniLayout::for_height(440.0), MiniLayout::Large);
    }
}
