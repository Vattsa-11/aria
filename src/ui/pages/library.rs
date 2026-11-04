use crate::app::App;
use crate::model::{fmt_time, Track};
use crate::ui::icons::Icon;
use crate::ui::theme::{accent, palette, Palette};

const ROW_H: f32 = 64.0;
const MIN_HIT: f32 = 44.0;

fn songs_view(app: &mut App, ui: &mut egui::Ui, accent: egui::Color32, pal: &Palette) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        ui.horizontal_wrapped(|ui| {
            if crate::ui::widgets::accent_button(ui, "Add files", egui::vec2(140.0, 44.0), accent) {
                app.open_file_picker();
            }
            ui.add_space(6.0);
            if crate::ui::widgets::pill_button(ui, "Add folder", false, accent, pal) {
                app.scan_folder();
            }
            ui.add_space(6.0);
            if crate::ui::widgets::pill_button(ui, "Rescan folders", false, accent, pal) {
                app.rescan_folders();
            }
        });
        ui.add_space(10.0);
    }

    if app.tracks.is_empty() {
        empty_state(app, ui, pal, accent);
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
                    pal,
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
    crate::ui::pages::rows::row_menus(app, rows, crate::ui::pages::rows::MenuKind::Standard, pal);
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

use crate::app::Page;
use crate::model::Playlist;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    All,
    Playlists,
    Liked,
    Downloaded,
    Recent,
    Songs,
}

impl Filter {
    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Playlists => "Playlists",
            Filter::Liked => "Liked",
            Filter::Downloaded => "Downloaded",
            Filter::Recent => "Recently played",
            Filter::Songs => "Local files",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Page(Page),
    Songs,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Filter,
    pub title: String,
    pub subtitle: String,
    pub count: usize,
    pub target: Target,
    pub cover: Option<Track>,
}

fn songs(count: usize) -> String {
    if count == 1 { "1 song".to_string() } else { format!("{count} songs") }
}

pub fn entries(liked: &[Track], downloads: &[Track], history: &[Track], playlists: &[Playlist], local: usize) -> Vec<Entry> {
    let mut list = vec![
        Entry { kind: Filter::Liked, title: "Liked Songs".into(), subtitle: format!("Playlist · {}", songs(liked.len())), count: liked.len(), target: Target::Page(Page::Liked), cover: None },
        Entry { kind: Filter::Downloaded, title: "Downloads".into(), subtitle: format!("{} · available offline", songs(downloads.len())), count: downloads.len(), target: Target::Page(Page::Downloads), cover: downloads.first().cloned() },
        Entry { kind: Filter::Recent, title: "Recently played".into(), subtitle: songs(history.len()), count: history.len(), target: Target::Page(Page::History), cover: history.first().cloned() },
    ];
    for (index, playlist) in playlists.iter().enumerate() {
        list.push(Entry {
            kind: Filter::Playlists,
            title: playlist.name.clone(),
            subtitle: format!("Playlist · {}", songs(playlist.tracks.len())),
            count: playlist.tracks.len(),
            target: Target::Page(Page::PlaylistDetail(index)),
            cover: playlist.tracks.first().cloned(),
        });
    }
    if local > 0 {
        list.push(Entry { kind: Filter::Songs, title: "Local files".into(), subtitle: songs(local), count: local, target: Target::Songs, cover: None });
    }
    list
}

pub fn filters(list: &[Entry]) -> Vec<Filter> {
    let has = |kind: Filter| list.iter().any(|entry| entry.kind == kind && (entry.kind == Filter::Playlists || entry.count > 0));
    [Filter::Playlists, Filter::Liked, Filter::Downloaded, Filter::Recent, Filter::Songs].into_iter().filter(|kind| has(*kind)).collect()
}

pub fn visible(list: &[Entry], filter: Filter, alphabetical: bool) -> Vec<&Entry> {
    let mut shown: Vec<&Entry> = list.iter().filter(|entry| filter == Filter::All || entry.kind == filter).collect();
    if alphabetical {
        shown.sort_by_key(|entry| entry.title.to_lowercase());
    }
    shown
}

fn state_id() -> egui::Id {
    egui::Id::NULL.with("aria.library-state")
}

fn paint_tile(ui: &egui::Ui, rect: egui::Rect, entry: &Entry, art: Option<&egui::TextureHandle>, accent: egui::Color32, pal: &Palette) {
    let radius = (rect.width() * 0.12).clamp(6.0, 14.0) as u8;
    let tile = |color: egui::Color32, icon: Icon| {
        ui.painter().rect_filled(rect, egui::CornerRadius::same(radius), color);
        crate::ui::icons::draw(ui.painter(), egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(rect.width() * 0.42)), icon, egui::Color32::WHITE);
    };
    match entry.kind {
        Filter::Liked => tile(crate::ui::widgets::mix(accent, egui::Color32::from_rgb(120, 70, 220), 0.6), Icon::HeartFilled),
        Filter::Downloaded if entry.cover.is_none() => tile(crate::ui::widgets::mix(accent, egui::Color32::from_rgb(30, 160, 110), 0.7), Icon::Download),
        Filter::Songs => tile(pal.raised, Icon::Note),
        _ => crate::ui::widgets::paint_cover(ui.painter(), rect, art, radius, accent, pal),
    }
}

fn hub(app: &mut App, ui: &mut egui::Ui, accent: egui::Color32, pal: &Palette) {
    let ctx = ui.ctx().clone();
    let (mut filter, mut alphabetical) = ctx.data(|data| data.get_temp::<(Filter, bool)>(state_id())).unwrap_or((Filter::All, false));
    let list = entries(&app.liked, &app.downloads, &app.history, &app.playlists, app.tracks.len());
    if filter != Filter::All && !filters(&list).contains(&filter) {
        filter = Filter::All;
    }

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if filter != Filter::All {
            if crate::ui::widgets::pill_button(ui, "✕", false, accent, pal) {
                filter = Filter::All;
            }
            crate::ui::widgets::pill_button(ui, filter.label(), true, accent, pal);
        } else {
            for kind in filters(&list) {
                if crate::ui::widgets::pill_button(ui, kind.label(), false, accent, pal) {
                    filter = kind;
                }
            }
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if crate::ui::widgets::pill_button(ui, if alphabetical { "A–Z" } else { "Recents" }, false, accent, pal) {
            alphabetical = !alphabetical;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::widgets::pill_button(ui, if app.settings.library_grid { "List" } else { "Grid" }, false, accent, pal) {
                app.settings.library_grid = !app.settings.library_grid;
                app.touch_settings();
            }
        });
    });
    ui.add_space(8.0);

    ctx.data_mut(|data| data.insert_temp(state_id(), (filter, alphabetical)));
    if filter == Filter::Songs {
        songs_view(app, ui, accent, pal);
        return;
    }

    let shown: Vec<Entry> = visible(&list, filter, alphabetical).into_iter().cloned().collect();
    let grid = app.settings.library_grid;
    let mut open: Option<Target> = None;
    egui::ScrollArea::vertical()
        .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Always, ..Default::default() })
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if grid {
                let gap = 12.0;
                let width = crate::ui::responsive::card_width(ui.available_width(), 150.0, gap);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
                    for entry in &shown {
                        let art = entry.cover.as_ref().and_then(|track| app.thumb(track));
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, width + 46.0), egui::Sense::click());
                        let tile = egui::Rect::from_min_size(rect.min, egui::Vec2::splat(width));
                        paint_tile(ui, tile, entry, art.as_ref(), accent, pal);
                        if response.is_pointer_button_down_on() {
                            ui.painter().rect_filled(tile, egui::CornerRadius::same(12), egui::Color32::from_black_alpha(60));
                        }
                        let body = egui::TextStyle::Body.resolve(ui.style());
                        let small = egui::TextStyle::Small.resolve(ui.style());
                        let title = crate::ui::widgets::fit_text_to(ui.painter(), &entry.title, &body, pal.text, width);
                        ui.painter().text(egui::pos2(rect.min.x, tile.max.y + 6.0), egui::Align2::LEFT_TOP, title, body, pal.text);
                        let sub = crate::ui::widgets::fit_text_to(ui.painter(), &entry.subtitle, &small, pal.muted, width);
                        ui.painter().text(egui::pos2(rect.min.x, tile.max.y + 26.0), egui::Align2::LEFT_TOP, sub, small, pal.muted);
                        if response.clicked() {
                            open = Some(entry.target);
                        }
                    }
                });
            } else {
                for entry in &shown {
                    let art = entry.cover.as_ref().and_then(|track| app.thumb(track));
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 68.0), egui::Sense::click());
                    if response.is_pointer_button_down_on() {
                        ui.painter().rect_filled(rect, egui::CornerRadius::same(10), pal.raised);
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, egui::CornerRadius::same(10), pal.raised.gamma_multiply(0.6));
                    }
                    let tile = egui::Rect::from_min_size(egui::pos2(rect.min.x + 6.0, rect.center().y - 28.0), egui::Vec2::splat(56.0));
                    paint_tile(ui, tile, entry, art.as_ref(), accent, pal);
                    let body = egui::TextStyle::Body.resolve(ui.style());
                    let small = egui::TextStyle::Small.resolve(ui.style());
                    let room = rect.max.x - tile.max.x - 24.0;
                    let title = crate::ui::widgets::fit_text_to(ui.painter(), &entry.title, &body, pal.text, room);
                    ui.painter().text(egui::pos2(tile.max.x + 14.0, rect.center().y - 20.0), egui::Align2::LEFT_TOP, title, body, pal.text);
                    let sub = crate::ui::widgets::fit_text_to(ui.painter(), &entry.subtitle, &small, pal.muted, room);
                    ui.painter().text(egui::pos2(tile.max.x + 14.0, rect.center().y + 3.0), egui::Align2::LEFT_TOP, sub, small, pal.muted);
                    if response.clicked() {
                        open = Some(entry.target);
                    }
                }
            }
            ui.add_space(16.0);
        });
    match open {
        Some(Target::Page(page)) => app.page = page,
        Some(Target::Songs) => ctx.data_mut(|data| {
            data.insert_temp(state_id(), (Filter::Songs, alphabetical));
        }),
        None => {}
    }
}

pub fn library(app: &mut App, ui: &mut egui::Ui) {
    let accent = accent(&app.settings);
    let pal = palette(app.settings.theme);
    ui.add_space(10.0);
    ui.label(egui::RichText::new("Your Library").size(28.0).color(pal.text).strong());
    ui.add_space(10.0);
    hub(app, ui, accent, &pal);
}

#[cfg(test)]
mod hub_tests {
    use super::*;

    fn track(id: u64, title: &str) -> Track {
        Track { id, title: title.into(), artist: "A".into(), album: String::new(), duration: None, source: crate::model::TrackSource::Yt { id: id.to_string() }, thumb: None, provider_meta: None }
    }

    fn playlist(name: &str, count: usize) -> Playlist {
        let mut list = Playlist::new(name);
        list.tracks = (0..count).map(|n| track(n as u64, "x")).collect();
        list
    }

    #[test]
    fn filter_chips_only_appear_for_content_you_actually_have() {
        let none = entries(&[], &[], &[], &[], 0);
        assert!(filters(&none).is_empty());
        let some = entries(&[track(1, "a")], &[], &[track(2, "b")], &[playlist("Gym", 0)], 3);
        assert_eq!(filters(&some), vec![Filter::Playlists, Filter::Liked, Filter::Recent, Filter::Songs]);
    }

    #[test]
    fn collections_are_described_like_spotify_does() {
        let list = entries(&[track(1, "a")], &[track(2, "b"), track(3, "c")], &[], &[playlist("Gym", 12)], 0);
        assert_eq!(list[0].subtitle, "Playlist · 1 song");
        assert_eq!(list[1].subtitle, "2 songs · available offline");
        assert_eq!(list.iter().find(|entry| entry.title == "Gym").unwrap().subtitle, "Playlist · 12 songs");
        assert!(list.iter().all(|entry| entry.kind != Filter::Songs));
    }

    #[test]
    fn filtering_and_sorting_work_together() {
        let list = entries(&[track(1, "a")], &[], &[], &[playlist("zeta", 1), playlist("Alpha", 1)], 0);
        let only: Vec<&str> = visible(&list, Filter::Playlists, false).iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(only, vec!["zeta", "Alpha"]);
        let sorted: Vec<&str> = visible(&list, Filter::Playlists, true).iter().map(|entry| entry.title.as_str()).collect();
        assert_eq!(sorted, vec!["Alpha", "zeta"]);
        assert_eq!(visible(&list, Filter::All, false).len(), list.len());
    }
}
