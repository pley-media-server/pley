//! Side panels, dialogs and the action sheet.

use std::cell::Cell;

use pley_base::{Key, Point, Rect};
use pley_ui::testing::{Harness, Step};
use pley_ui::theme;
use pley_ui::{Id, Ui};

use super::{fills_of, press, settle, text_color};
use crate::button::Button;
use crate::list::ListRow;
use crate::overlay::{Action, ActionSheet, Dialog, DialogButton, SidePanel};

#[test]
fn a_side_panel_slides_in_keeps_focus_and_gives_it_back_when_closed() {
    let open = Cell::new(false);
    let mut screen = |ui: &mut Ui| {
        let a = ui.id("sort");
        if Button::chip("Sort").show(ui, a, Point::new(96.0, 80.0)).activated {
            open.set(true);
        }
        let b = ui.id("filter");
        Button::chip("Filter").show(ui, b, Point::new(200.0, 80.0));
        let r = SidePanel::new().show(ui, Id::new("panel"), open.get(), |ui, area| {
            for (i, label) in ["Title", "Year", "Date added"].iter().enumerate() {
                let id = ui.id(i);
                if i == 1 {
                    ui.initial_focus(id);
                }
                let at = Point::new(area.x, area.y + i as f32 * 46.0);
                ListRow::new(label).checked(i == 1).show(ui, id, at, area.w);
            }
        });
        if r.closed {
            open.set(false);
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let row = |i: usize| Some(Id::new("panel").with(i));
    // OK on Sort opens it; focus starts on the choice in force.
    assert_eq!(press(&mut h, Key::Ok, &mut screen), row(1));
    let f = h.idle(&mut screen);
    // 360 wide at the right edge (drawn 8 past it so only its left corners round), content 24 in.
    assert!(fills_of(&f.list, theme::SURFACE2).contains(&Rect::new(600.0, 0.0, 368.0, 540.0)));
    assert!(fills_of(&f.list, theme::BG.alpha(0.6)).contains(&Rect::new(0.0, 0.0, 960.0, 540.0)));
    assert!(fills_of(&f.list, theme::ACCENT).contains(&Rect::new(624.0, 27.0 + 46.0, 288.0, 44.0)));
    // Focus stays inside.
    assert_eq!(press(&mut h, Key::Left, &mut screen), row(1));
    assert_eq!(press(&mut h, Key::Up, &mut screen), row(0));
    assert_eq!(press(&mut h, Key::Up, &mut screen), row(0));
    // Back closes it, and focus goes back to Sort.
    assert_eq!(press(&mut h, Key::Back, &mut screen), Some(Id::new("sort")));
    assert!(!open.get());
    // Halfway out it still shows, sliding right.
    h.step(Step::Press(Key::Ok), &mut screen);
    h.wait(0.3, &mut screen);
    h.step(Step::Press(Key::Back), &mut screen);
    h.wait(0.1, &mut screen);
    let f = h.idle(&mut screen);
    let panel = fills_of(&f.list, theme::SURFACE2);
    assert!(panel.iter().any(|r| r.x > 600.0 && r.x < 960.0), "{panel:?}");
    // Sliding out, its rows are drawn but no longer take focus.
    assert!(f.list.says("Year"));
    assert_eq!(f.focused, Some(Id::new("sort")));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("filter")));
}

#[test]
fn a_dialog_sits_in_the_middle_keeps_focus_and_back_dismisses_it() {
    let open = Cell::new(true);
    let chosen = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        let a = ui.id("remove");
        Button::text("Remove").show(ui, a, Point::new(96.0, 400.0));
        let buttons = [DialogButton::new("Cancel"), DialogButton::danger("Delete")];
        let dialog = Dialog::new("Delete Road trip?").body("This can't be undone.").buttons(&buttons);
        let r = dialog.show(ui, Id::new("dialog"), open.get());
        if r.chosen.is_some() {
            chosen.set(r.chosen);
        }
        if r.dismissed {
            open.set(false);
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("dialog").with(0usize)));
    // At least 320 wide, centred.
    let card = fills_of(&f.list, theme::SURFACE2)[0];
    assert_eq!((card.w, card.x), (320.0, 320.0));
    assert!(((card.y + card.h / 2.0) - 270.0).abs() < 0.01);
    assert_eq!(text_color(&f.list, "Delete"), Some(theme::DANGER_TEXT));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("dialog").with(1usize)));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(Id::new("dialog").with(1usize)));
    h.step(Step::Press(Key::Ok), &mut screen);
    assert_eq!(chosen.get(), Some(1));
    assert_eq!(press(&mut h, Key::Back, &mut screen), Some(Id::new("remove")));
    assert!(!open.get());
}

#[test]
fn an_action_sheet_lists_its_actions_and_focuses_the_first() {
    let chosen = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        let actions = [
            Action::new("Resume").icon(pley_base::Icon::Play),
            Action::new("Play from start"),
            Action::new("Remove from Continue Watching"),
        ];
        let sheet = ActionSheet::new("Severance", &actions).subtitle("S2 · E5 · Trojan's Horse");
        if let Some(i) = sheet.show(ui, Id::new("sheet")) {
            chosen.set(Some(i));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("sheet").with(0usize)));
    // Title at (96, 58), subtitle under it, rows 24 below.
    assert_eq!(super::text_at(&f.list, "Severance"), Some(Point::new(96.0, 58.0)));
    assert!(fills_of(&f.list, theme::ACCENT).contains(&Rect::new(96.0, 58.0 + 34.0 + 20.0 + 24.0, 464.0, 44.0)));
    press(&mut h, Key::Down, &mut screen);
    h.step(Step::Press(Key::Ok), &mut screen);
    assert_eq!(chosen.get(), Some(1));
}
