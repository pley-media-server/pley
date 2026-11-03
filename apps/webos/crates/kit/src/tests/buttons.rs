//! Buttons, chips, tabs and the button bar.

use std::cell::Cell;

use pley_base::{Cmd, Icon, Key, Point, Rect};
use pley_ui::testing::Harness;
use pley_ui::theme;
use pley_ui::{Dir, Id, Ui};

use super::{fills_of, press, scales, settle, text_at, text_color};
use crate::button::{BarItem, Button, RoundButton, button_bar};

#[test]
fn a_pill_is_at_least_128_wide_and_grows_to_1_05_with_an_accent_fill() {
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("play");
        Button::pill("Play").icon(Icon::Play).show(ui, id, Point::new(96.0, 300.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("play")));
    let rect = Rect::new(96.0, 300.0, 128.0, 40.0);
    assert!(fills_of(&f.list, theme::ACCENT).contains(&rect));
    assert_eq!(scales(&f.list), vec![(1.05, rect.center())]);
    assert_eq!(text_color(&f.list, "Play"), Some(theme::ON_ACCENT));
    // A long label makes it wider: 20 + 20 + 8 + label + 20.
    let h2 = Harness::new();
    let mut list = pley_base::DisplayList::default();
    let mut state = pley_ui::UiState::new();
    let mut images = pley_ui::testing::StubImages::new();
    let ui = Ui::new(&mut state, &mut list, &h2.shaper, &mut images, 2.0, &pley_ui::FrameInput::at(0.0));
    let label = "Resume S2 · E5";
    let w = Button::pill(label).icon(Icon::Play).size(&ui).w;
    assert!((w - (68.0 + label.chars().count() as f32 * 7.1)).abs() < 0.01);
    assert_eq!(Button::text("OK").size(&ui).w, 58.0);
}

#[test]
fn disabled_buttons_are_dimmed_and_never_focused() {
    let mut screen = |ui: &mut Ui| {
        let a = ui.id("resume");
        Button::pill("Resume").enabled(false).show(ui, a, Point::new(96.0, 300.0));
        let b = ui.id("info");
        Button::pill("Info").show(ui, b, Point::new(234.0, 300.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("info")));
    assert!(f.list.cmds.contains(&Cmd::PushOpacity(theme::DISABLED_ALPHA)));
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(Id::new("info")));
}

#[test]
fn a_round_buttons_label_shows_under_it_only_while_focused() {
    let mut screen = |ui: &mut Ui| {
        let items = [
            ("watched", BarItem::Round(RoundButton::new(Icon::CircleCheck, "Mark as watched"))),
            ("info", BarItem::Round(RoundButton::new(Icon::Info, "Info"))),
        ];
        button_bar(ui, Id::new("bar"), Point::new(96.0, 300.0), &items);
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("bar").with("watched")));
    // Centred on the button, 46 below its top, as wide as it needs (15 characters of 6.7).
    let at = text_at(&f.list, "Mark as watched").unwrap();
    assert!((at.x - (116.0 - 15.0 * 6.7 / 2.0)).abs() < 0.01 && at.y == 346.0);
    assert_eq!(text_at(&f.list, "Info"), None);
    assert_eq!(scales(&f.list), vec![(1.08, Point::new(116.0, 320.0))]);
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("bar").with("info")));
    let f = h.idle(&mut screen);
    assert_eq!(text_at(&f.list, "Info").map(|p| p.y), Some(346.0));
    assert_eq!(text_at(&f.list, "Mark as watched"), None);
    // The second button sits 10 after the first.
    assert_eq!(scales(&f.list), vec![(1.08, Point::new(166.0, 320.0))]);
}

#[test]
fn left_from_a_bars_first_button_leaves_the_page_and_right_stops_at_its_end() {
    let failed = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        let items = [("play", BarItem::Button(Button::pill("Play"))), ("more", BarItem::Button(Button::text("More")))];
        button_bar(ui, Id::new("bar"), Point::new(96.0, 300.0), &items);
        if let Some(d) = ui.failed_move() {
            failed.set(Some(d));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(Id::new("bar").with("play")));
    assert_eq!(failed.get(), Some(Dir::Left));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("bar").with("more")));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("bar").with("more")));
}

#[test]
fn chips_lead_with_a_check_when_selected_and_tabs_mark_the_current_one() {
    let mut screen = |ui: &mut Ui| {
        let t = ui.id("library");
        Button::tab("Library", true).show(ui, t, Point::new(600.0, 27.0));
        let c = ui.id("unwatched");
        Button::chip("Unwatched").selected(true).show(ui, c, Point::new(96.0, 80.0));
        let o = ui.id("collections");
        Button::tab("Collections", false).show(ui, o, Point::new(720.0, 27.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    // The current tab rests on Surface3; the focused one is Accent; the other tab has no fill.
    assert_eq!(f.focused, Some(Id::new("library")));
    let checks: Vec<_> = f
        .list
        .cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Icon { icon: Icon::Check, color, .. } => Some(*color),
            _ => None,
        })
        .collect();
    assert_eq!(checks, vec![theme::ACCENT]);
    assert_eq!(text_color(&f.list, "Collections"), Some(theme::MUTED));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("collections")));
    let f = h.idle(&mut screen);
    assert_eq!(fills_of(&f.list, theme::SURFACE3).len(), 2);
}
