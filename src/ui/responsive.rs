#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Breakpoint {
    Phone,
    Tablet,
    Desktop,
    Wide,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    BottomTabs,
    Rail,
    Side,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sheet {
    Fullscreen,
    Overlay(f32),
    Docked(f32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Plan {
    pub breakpoint: Breakpoint,
    pub nav: Nav,
    pub compact_bar: bool,
    pub phone_bar: bool,
    pub sheet: Sheet,
    pub page_margin: i8,
    pub card_min: f32,
}

pub const RAIL_WIDTH: f32 = 76.0;

pub fn breakpoint(width: f32) -> Breakpoint {
    if width < 640.0 {
        Breakpoint::Phone
    } else if width < 900.0 {
        Breakpoint::Tablet
    } else if width < 1280.0 {
        Breakpoint::Desktop
    } else {
        Breakpoint::Wide
    }
}

pub fn plan(width: f32) -> Plan {
    match breakpoint(width) {
        Breakpoint::Phone => Plan {
            breakpoint: Breakpoint::Phone,
            nav: Nav::BottomTabs,
            compact_bar: false,
            phone_bar: true,
            sheet: Sheet::Fullscreen,
            page_margin: 14,
            card_min: 140.0,
        },
        Breakpoint::Tablet => Plan {
            breakpoint: Breakpoint::Tablet,
            nav: Nav::Rail,
            compact_bar: true,
            phone_bar: false,
            sheet: Sheet::Overlay((width * 0.55).clamp(300.0, 380.0)),
            page_margin: 18,
            card_min: 150.0,
        },
        Breakpoint::Desktop => Plan {
            breakpoint: Breakpoint::Desktop,
            nav: Nav::Side,
            compact_bar: false,
            phone_bar: false,
            sheet: Sheet::Docked(340.0),
            page_margin: 24,
            card_min: 156.0,
        },
        Breakpoint::Wide => Plan {
            breakpoint: Breakpoint::Wide,
            nav: Nav::Side,
            compact_bar: false,
            phone_bar: false,
            sheet: Sheet::Docked(380.0),
            page_margin: 28,
            card_min: 172.0,
        },
    }
}

pub fn sheet_width(sheet: Sheet, window_width: f32) -> f32 {
    match sheet {
        Sheet::Fullscreen => window_width,
        Sheet::Overlay(width) | Sheet::Docked(width) => width.min(window_width),
    }
}

pub fn docked_reserve(sheet: Sheet, open_sheets: usize) -> f32 {
    match sheet {
        Sheet::Docked(width) => width * open_sheets as f32,
        _ => 0.0,
    }
}

pub fn columns(available: f32, min_card: f32, gap: f32) -> usize {
    (((available + gap) / (min_card + gap)).floor() as usize).max(1)
}

pub fn card_width(available: f32, min_card: f32, gap: f32) -> f32 {
    let count = columns(available, min_card, gap);
    let width = (available - gap * (count as f32 - 1.0)) / count as f32;
    width.clamp(min_card.min(available), min_card * 1.4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_follow_common_device_widths() {
        assert_eq!(breakpoint(360.0), Breakpoint::Phone);
        assert_eq!(breakpoint(639.9), Breakpoint::Phone);
        assert_eq!(breakpoint(768.0), Breakpoint::Tablet);
        assert_eq!(breakpoint(1024.0), Breakpoint::Desktop);
        assert_eq!(breakpoint(1280.0), Breakpoint::Wide);
        assert_eq!(breakpoint(2560.0), Breakpoint::Wide);
    }

    #[test]
    fn each_size_gets_the_navigation_and_bar_that_fits() {
        let phone = plan(390.0);
        assert_eq!((phone.nav, phone.phone_bar, phone.compact_bar), (Nav::BottomTabs, true, false));
        let tablet = plan(800.0);
        assert_eq!((tablet.nav, tablet.phone_bar, tablet.compact_bar), (Nav::Rail, false, true));
        let desktop = plan(1180.0);
        assert_eq!((desktop.nav, desktop.phone_bar, desktop.compact_bar), (Nav::Side, false, false));
        assert!(plan(1920.0).page_margin > plan(390.0).page_margin);
    }

    #[test]
    fn sheets_dock_on_big_screens_and_float_or_fill_on_small_ones() {
        assert_eq!(plan(390.0).sheet, Sheet::Fullscreen);
        assert!(matches!(plan(800.0).sheet, Sheet::Overlay(_)));
        assert!(matches!(plan(1180.0).sheet, Sheet::Docked(_)));
        assert_eq!(sheet_width(Sheet::Fullscreen, 390.0), 390.0);
        assert_eq!(sheet_width(Sheet::Docked(340.0), 300.0), 300.0);
        if let Sheet::Overlay(width) = plan(700.0).sheet {
            assert!((300.0..=380.0).contains(&width));
        }
    }

    #[test]
    fn only_docked_sheets_take_space_from_the_page() {
        assert_eq!(docked_reserve(Sheet::Docked(340.0), 1), 340.0);
        assert_eq!(docked_reserve(Sheet::Docked(340.0), 2), 680.0);
        assert_eq!(docked_reserve(Sheet::Docked(340.0), 0), 0.0);
        assert_eq!(docked_reserve(Sheet::Overlay(340.0), 2), 0.0);
        assert_eq!(docked_reserve(Sheet::Fullscreen, 1), 0.0);
    }

    #[test]
    fn cards_fill_the_row_without_gaps_or_tiny_leftovers() {
        assert_eq!(columns(362.0, 140.0, 10.0), 2);
        assert_eq!(columns(1000.0, 156.0, 10.0), 6);
        assert_eq!(columns(100.0, 156.0, 10.0), 1);
        let width = card_width(362.0, 140.0, 10.0);
        assert!((width * 2.0 + 10.0 - 362.0).abs() < 0.01, "two cards should fill the row, got {width}");
        let wide = card_width(1000.0, 156.0, 10.0);
        assert!((156.0..=156.0 * 1.4).contains(&wide));
        assert_eq!(card_width(80.0, 156.0, 10.0), 80.0);
    }
}
