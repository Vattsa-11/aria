use crate::model::{fmt_time, Track};
use crate::queue::TrackMenuAction;
use crate::ui::icons::Icon;
use crate::ui::theme::Palette;

pub const ROW_H: f32 = 64.0;

pub fn window(tracks: &[Track], visible: &std::ops::Range<usize>) -> Vec<Track> {
    let start = visible.start.saturating_sub(6).min(tracks.len());
    let end = (visible.end + 14).min(tracks.len());
    tracks[start..end.max(start)].to_vec()
}
pub const MIN_HIT: f32 = 44.0;

#[derive(Default)]
pub struct DragState {
    pub dragged_index: Option<usize>,
    pub drag_start_y: f32,
    pub pending_move: Option<(usize, usize)>,

    pub total: usize,
}

fn menu_button(ui: &mut egui::Ui, label: &str, pal: &Palette) -> bool {
    let width = ui.available_width();
    ui.add_sized(
        [width, 44.0],
        egui::Button::new(
            egui::RichText::new(label).size(14.5).color(pal.text),
        ),
    )
    .clicked()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuKind {
    Standard,
    Downloads,
}

#[derive(Clone, Copy, Debug)]
pub struct MenuView {
    pub liked: bool,
    pub is_online: bool,
    pub downloaded: bool,
    pub downloading: bool,
    pub heart_visible: bool,
    pub show_play: bool,
    pub has_artist: bool,
    pub has_link: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuItem {
    Play,
    GoToArtist,
    CopyInfo,
    CopyLink,
    PlayNext,
    AddToQueue,
    Like(bool),
    FixMatch,
    Download,
    CancelDownload,
    DeleteDownload,
}

impl MenuItem {
    fn label(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::GoToArtist => "Go to artist",
            Self::CopyInfo => "Copy song info",
            Self::CopyLink => "Copy link",
            Self::PlayNext => "Play next",
            Self::AddToQueue => "Add to queue",
            Self::Like(true) => "Remove from Liked Songs",
            Self::Like(false) => "Save to Liked Songs",
            Self::FixMatch => "Fix match",
            Self::Download => "Download",
            Self::CancelDownload => "Cancel download",
            Self::DeleteDownload => "Delete download",
        }
    }

    fn action(self) -> TrackMenuAction {
        match self {
            Self::Play => TrackMenuAction::Play,
            Self::GoToArtist => TrackMenuAction::GoToArtist,
            Self::CopyInfo => TrackMenuAction::CopyInfo,
            Self::CopyLink => TrackMenuAction::CopyLink,
            Self::PlayNext => TrackMenuAction::PlayNext,
            Self::AddToQueue => TrackMenuAction::AddToQueue,
            Self::Like(_) => TrackMenuAction::Like,
            Self::FixMatch => TrackMenuAction::FixMatch,
            Self::Download => TrackMenuAction::Download,
            Self::CancelDownload => TrackMenuAction::CancelDownload,
            Self::DeleteDownload => TrackMenuAction::DeleteDownload,
        }
    }
}

pub fn menu_items(view: &MenuView) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if view.show_play {
        items.push(MenuItem::Play);
    }
    items.push(MenuItem::PlayNext);
    items.push(MenuItem::AddToQueue);
    if !view.heart_visible {
        items.push(MenuItem::Like(view.liked));
    }
    if view.is_online {
        items.push(MenuItem::FixMatch);
        if view.downloading {
            items.push(MenuItem::CancelDownload);
        } else if view.downloaded {
            items.push(MenuItem::DeleteDownload);
        } else {
            items.push(MenuItem::Download);
        }
    } else if view.downloaded {
        items.push(MenuItem::DeleteDownload);
    }
    if view.has_artist {
        items.push(MenuItem::GoToArtist);
    }
    items.push(MenuItem::CopyInfo);
    if view.has_link {
        items.push(MenuItem::CopyLink);
    }
    items
}

fn menu_body(
    ui: &mut egui::Ui,
    confirm_id: egui::Id,
    view: &MenuView,
    playlists: &[(usize, String)],
    pal: &Palette,
    actions: &mut Vec<TrackMenuAction>,
) {
    ui.set_min_width(220.0);
    for item in menu_items(view) {
        if item == MenuItem::DeleteDownload {
            let pending = ui.data(|data| data.get_temp::<bool>(confirm_id)).unwrap_or(false);
            let label = if pending { "Tap again to delete" } else { item.label() };
            if menu_button(ui, label, pal) {
                if pending {
                    ui.data_mut(|data| data.insert_temp(confirm_id, false));
                    actions.push(item.action());
                } else {
                    ui.data_mut(|data| data.insert_temp(confirm_id, true));
                }
            }
        } else if menu_button(ui, item.label(), pal) {
            actions.push(item.action());
        }
    }
    ui.separator();
    ui.label(egui::RichText::new("Add to playlist").size(12.0).color(pal.muted));
    if menu_button(ui, "New playlist with this song", pal) {
        actions.push(TrackMenuAction::NewPlaylist);
    }
    for (idx, name) in playlists {
        if menu_button(ui, name, pal) {
            actions.push(TrackMenuAction::AddToPlaylist(*idx));
        }
    }
}

pub fn show_track_menu(
    resp: &egui::Response,
    view: &MenuView,
    playlists: &[(usize, String)],
    pal: &Palette,
) -> Vec<TrackMenuAction> {
    let mut actions = Vec::new();
    let confirm_id = resp.id.with("delete-confirm");
    if resp.clicked() {
        resp.ctx.data_mut(|data| data.insert_temp(confirm_id, false));
    }
    let at_pointer = resp.ctx.data(|data| data.get_temp::<bool>(resp.id.with("at-pointer"))).unwrap_or(false);
    let mut popup = egui::Popup::from_toggle_button_response(resp);
    if at_pointer {
        popup = popup.anchor(egui::PopupAnchor::PointerFixed);
    }
    popup.show(|ui| menu_body(ui, confirm_id, view, playlists, pal, &mut actions));
    actions
}

fn menu_view_for(app: &crate::app::App, track: &Track) -> MenuView {
    MenuView {
        liked: app.is_liked(track.id),
        is_online: track.is_online(),
        downloaded: app.is_downloaded(track.id),
        downloading: app.is_downloading(track.id),
        heart_visible: true,
        show_play: true,
        has_artist: !track.artist.trim().is_empty(),
        has_link: track.watch_url().is_some(),
    }
}

fn playlist_choices(app: &crate::app::App) -> Vec<(usize, String)> {
    app.playlists
        .iter()
        .enumerate()
        .map(|(index, playlist)| (index, playlist.name.clone()))
        .collect()
}

pub fn track_context_menu(
    app: &mut crate::app::App,
    response: &egui::Response,
    track: &Track,
    pal: &Palette,
) {
    let view = menu_view_for(app, track);
    let playlists = playlist_choices(app);
    let confirm_id = response.id.with("delete-confirm");
    let mut actions = Vec::new();
    egui::Popup::context_menu(response)
        .show(|ui| menu_body(ui, confirm_id, &view, &playlists, pal, &mut actions));
    for action in actions {
        app.track_menu_for(track, action);
    }
}

pub fn open_menu_on_right_click(ctx: &egui::Context, row: &egui::Response, menu: &egui::Response) {
    let flag = menu.id.with("at-pointer");
    if row.secondary_clicked() {
        ctx.data_mut(|data| data.insert_temp(flag, true));
        egui::Popup::open_id(ctx, egui::Popup::default_response_id(menu));
    } else if menu.clicked() {
        ctx.data_mut(|data| data.insert_temp(flag, false));
    }
}

pub fn row_menus(
    app: &mut crate::app::App,
    rows: Vec<(egui::Response, Track)>,
    _kind: MenuKind,
    pal: &Palette,
) {
    if rows.is_empty() {
        return;
    }
    let playlists = playlist_choices(app);
    for (response, track) in rows {
        let view = menu_view_for(app, &track);
        for action in show_track_menu(&response, &view, &playlists, pal) {
            app.track_menu_for(&track, action);
        }
    }
}

pub struct RowResponse {
    pub menu: egui::Response,
    pub like: egui::Response,
    pub remove: Option<egui::Response>,
    pub up: Option<egui::Response>,
    pub down: Option<egui::Response>,
    pub clicked: bool,
    pub move_to: Option<usize>,
}

#[allow(clippy::too_many_arguments)]
pub fn track_row(
    ui: &mut egui::Ui,
    id_salt: egui::Id,
    index: usize,
    track: &Track,
    art: Option<&egui::TextureHandle>,
    liked: bool,
    accent: egui::Color32,
    pal: &Palette,
    show_remove: bool,
    show_reorder: bool,
    mut drag_state: Option<&mut DragState>,
    dim: bool,
    badge: Option<&str>,
) -> RowResponse {
    let width = ui.available_width();
    let show_reorder = show_reorder && width >= 460.0;
    let sense = if drag_state.is_some() && show_reorder {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::new(width, ROW_H), sense);

    if response.hovered() && !response.dragged() {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(10), pal.raised);
    }

    if let Some(ds) = drag_state.as_mut() {
        if response.drag_started() && show_reorder {
            ds.dragged_index = Some(index);
            ds.drag_start_y = rect.min.y;
        }
        if response.dragged() && ds.dragged_index == Some(index) {
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(10), accent.gamma_multiply(0.15));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if response.drag_stopped() {
            if let Some(from) = ds.dragged_index.take() {
                if let Some(pointer_pos) = response.interact_pointer_pos() {
                    let last = ds.total.saturating_sub(1);
                    let moved = ((pointer_pos.y - ds.drag_start_y) / ROW_H).round() as isize;
                    let target_index = (from as isize + moved).clamp(0, last as isize) as usize;
                    if from != target_index {
                        ds.pending_move = Some((from, target_index));
                    }
                }
            }
        }
    }

    let art_w = MIN_HIT - 8.0;
    let art_rect = egui::Rect::from_min_size(
        egui::pos2(rect.min.x + 10.0, rect.center().y - art_w * 0.5),
        egui::Vec2::splat(art_w),
    );
    crate::ui::widgets::paint_cover(ui.painter(), art_rect, art, 6, accent, pal);

    let mut right = rect.max.x - 10.0;

    let menu_rect = egui::Rect::from_center_size(
        egui::pos2(right - 22.0, rect.center().y),
        egui::Vec2::splat(MIN_HIT),
    );
    right = menu_rect.min.x - 4.0;
    let menu_response = ui.interact(
        menu_rect,
        id_salt.with(("row-menu", index)),
        egui::Sense::click(),
    );
    crate::ui::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(menu_rect.center(), egui::Vec2::splat(22.0)),
        Icon::More,
        if menu_response.hovered() { pal.text } else { pal.muted },
    );

    open_menu_on_right_click(ui.ctx(), &response, &menu_response);

    let like_rect = egui::Rect::from_center_size(
        egui::pos2(right - 22.0, rect.center().y),
        egui::Vec2::splat(MIN_HIT),
    );
    right = like_rect.min.x - 4.0;
    let like_response = ui.interact(
        like_rect,
        id_salt.with(("row-like", index)),
        egui::Sense::click(),
    );
    let heart_icon = if liked { Icon::HeartFilled } else { Icon::Heart };
    let heart_pop = crate::ui::widgets::heart_scale(ui.ctx(), like_response.id, heart_icon);
    crate::ui::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(like_rect.center(), egui::Vec2::splat(22.0 * heart_pop)),
        heart_icon,
        if liked { accent } else { pal.muted },
    );

    let mut remove = None;
    if show_remove {
        let remove_rect = egui::Rect::from_center_size(
            egui::pos2(right - 22.0, rect.center().y),
            egui::Vec2::splat(MIN_HIT),
        );
        right = remove_rect.min.x - 4.0;
        let resp = ui.interact(
            remove_rect,
            id_salt.with(("row-remove", index)),
            egui::Sense::click(),
        );
        crate::ui::icons::draw(
            ui.painter(),
            egui::Rect::from_center_size(remove_rect.center(), egui::Vec2::splat(20.0)),
            Icon::Close,
            if resp.hovered() { pal.text } else { pal.muted },
        );
        remove = Some(resp);
    }

    let mut up = None;
    let mut down = None;
    if show_reorder {
        let up_rect = egui::Rect::from_center_size(
            egui::pos2(right - 22.0, rect.center().y),
            egui::Vec2::splat(MIN_HIT),
        );
        right = up_rect.min.x - 4.0;
        let resp = ui.interact(up_rect, id_salt.with(("row-up", index)), egui::Sense::click());
        crate::ui::icons::draw(
            ui.painter(),
            egui::Rect::from_center_size(up_rect.center(), egui::Vec2::splat(18.0)),
            Icon::ChevronUp,
            if resp.hovered() { pal.text } else { pal.muted },
        );
        up = Some(resp);

        let down_rect = egui::Rect::from_center_size(
            egui::pos2(right - 22.0, rect.center().y),
            egui::Vec2::splat(MIN_HIT),
        );
        right = down_rect.min.x - 4.0;
        let resp = ui.interact(
            down_rect,
            id_salt.with(("row-down", index)),
            egui::Sense::click(),
        );
        crate::ui::icons::draw(
            ui.painter(),
            egui::Rect::from_center_size(down_rect.center(), egui::Vec2::splat(18.0)),
            Icon::ChevronDown,
            if resp.hovered() { pal.text } else { pal.muted },
        );
        down = Some(resp);
    }

    let text_left = art_rect.max.x + 12.0;
    let text_right = right;
    let body_font = egui::TextStyle::Body.resolve(ui.style());
    let small_font = egui::TextStyle::Small.resolve(ui.style());

    let title_color = if dim { pal.text.gamma_multiply(0.45) } else { pal.text };
    let sub_color = if dim { pal.muted.gamma_multiply(0.55) } else { pal.muted };
    let badge_w = badge.map(|_| 24.0).unwrap_or(0.0);
    let badge_gap = if badge.is_some() { badge_w + 8.0 } else { 0.0 };
    let duration_right = text_right - badge_gap;
    let duration_text = track.duration.map(|d| fmt_time(d.as_secs_f64()));
    let duration_width = duration_text
        .as_ref()
        .map(|text| ui.painter().layout_no_wrap(text.clone(), small_font.clone(), sub_color).size().x + 12.0)
        .unwrap_or(0.0);
    let title_max = (text_right - text_left).max(0.0);
    let artist_max = (duration_right - duration_width - text_left).max(0.0);
    let title = crate::ui::widgets::fit_text_to(ui.painter(), &track.title, &body_font, title_color, title_max);
    ui.painter().text(
        egui::pos2(text_left, rect.min.y + 14.0),
        egui::Align2::LEFT_TOP,
        title,
        body_font,
        title_color,
    );
    let artist = crate::ui::widgets::fit_text_to(ui.painter(), track.display_artist(), &small_font, sub_color, artist_max);
    ui.painter().text(
        egui::pos2(text_left, rect.min.y + 36.0),
        egui::Align2::LEFT_TOP,
        artist,
        small_font.clone(),
        sub_color,
    );
    if let Some(text) = duration_text {
        ui.painter().text(
            egui::pos2(duration_right, rect.min.y + 36.0),
            egui::Align2::RIGHT_TOP,
            text,
            small_font,
            sub_color,
        );
    }
    if badge.is_some() {
        let badge_rect = egui::Rect::from_min_size(
            egui::pos2(text_right - badge_w, rect.min.y + 33.0),
            egui::Vec2::new(badge_w, 18.0),
        );
        ui.painter().rect_filled(
            badge_rect,
            egui::CornerRadius::same(9),
            accent.gamma_multiply(0.18),
        );
        crate::ui::icons::draw(
            ui.painter(),
            egui::Rect::from_center_size(badge_rect.center(), egui::Vec2::splat(12.0)),
            Icon::Download,
            accent,
        );
    }

    let clicked = response.clicked()
        && !menu_response.clicked()
        && !like_response.clicked()
        && remove.as_ref().map(|r| !r.clicked()).unwrap_or(true)
        && up.as_ref().map(|r| !r.clicked()).unwrap_or(true)
        && down.as_ref().map(|r| !r.clicked()).unwrap_or(true);

    let move_to = drag_state
        .and_then(|ds| ds.pending_move.take())
        .map(|(_, to)| to);

    RowResponse {
        menu: menu_response,
        like: like_response,
        remove,
        up,
        down,
        clicked,
        move_to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetch_window_extends_past_the_visible_rows() {
        let tracks: Vec<Track> = (0..100)
            .map(|n| Track::local(std::path::Path::new(&format!("/m/{n}.mp3"))))
            .collect();
        assert_eq!(window(&tracks, &(10..20)).len(), 6 + 10 + 14);
        assert_eq!(window(&tracks, &(0..8)).len(), 8 + 14);
        assert_eq!(window(&tracks, &(90..100)).len(), 10 + 6);
        assert!(window(&[], &(0..0)).is_empty());
    }

    fn view() -> MenuView {
        MenuView {
            liked: false,
            is_online: true,
            downloaded: false,
            downloading: false,
            heart_visible: true,
            show_play: false,
            has_artist: true,
            has_link: true,
        }
    }

    #[test]
    fn like_is_hidden_when_row_shows_a_heart() {
        assert!(!menu_items(&view()).iter().any(|item| matches!(item, MenuItem::Like(_))));
        let narrow = MenuView { heart_visible: false, liked: true, ..view() };
        assert!(menu_items(&narrow).contains(&MenuItem::Like(true)));
    }

    #[test]
    fn download_is_offered_for_every_online_track() {
        assert!(menu_items(&view()).contains(&MenuItem::Download));
        let local = MenuView { is_online: false, ..view() };
        assert!(!menu_items(&local).contains(&MenuItem::Download));
    }

    #[test]
    fn downloaded_tracks_offer_delete_instead_of_download() {
        let done = MenuView { downloaded: true, ..view() };
        let items = menu_items(&done);
        assert!(items.contains(&MenuItem::DeleteDownload));
        assert!(!items.contains(&MenuItem::Download));
        let busy = MenuView { downloading: true, ..view() };
        let items = menu_items(&busy);
        assert!(items.contains(&MenuItem::CancelDownload));
        assert!(!items.contains(&MenuItem::Download));
    }

    #[test]
    fn every_menu_offers_artist_and_copy_actions_when_they_apply() {
        let items = menu_items(&view());
        assert!(items.contains(&MenuItem::GoToArtist));
        assert!(items.contains(&MenuItem::CopyInfo));
        assert!(items.contains(&MenuItem::CopyLink));
        let bare = MenuView { has_artist: false, has_link: false, ..view() };
        let items = menu_items(&bare);
        assert!(!items.contains(&MenuItem::GoToArtist));
        assert!(!items.contains(&MenuItem::CopyLink));
        assert!(items.contains(&MenuItem::CopyInfo));
    }

    #[test]
    fn downloads_menu_has_play_first_and_no_duplicates() {
        let items = menu_items(&MenuView { downloaded: true, show_play: true, ..view() });
        assert_eq!(items[0], MenuItem::Play);
        let mut seen = std::collections::HashSet::new();
        for item in &items {
            assert!(seen.insert(item.action()), "duplicate {item:?}");
        }
    }
}
