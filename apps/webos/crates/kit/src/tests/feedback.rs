//! Toasts, the loading ring, messages, badges and the metadata line, the backdrop, the gallery.

use std::cell::{Cell, RefCell};

use pley_base::{Cmd, ImageRequest, Key, Point, Rect};
use pley_ui::testing::{Harness, Step};
use pley_ui::theme;
use pley_ui::{Id, Ui};

use super::{fills_of, settle, text_at, text_color};
use crate::backdrop::{Backdrop, BackdropArt, BackdropScrim, BackdropState};
use crate::badge::{Badge, MetaLine};
use crate::feedback::{InfoBlock, LoadingRing, Message, MessageStyle, ToastPlacement, ToastState, toast};
use crate::gallery::{GalleryState, gallery};

fn raw_rects(list: &pley_base::DisplayList) -> Vec<Rect> {
    list.cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Raw { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn the_buffering_ring_waits_half_a_second_then_turns() {
    let mut screen = |ui: &mut Ui| {
        LoadingRing::buffering().show(ui, Point::new(480.0, 270.0), 0.0);
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert!(raw_rects(&f.list).is_empty());
    // It asks for the frame it shows on.
    assert_eq!(f.output.wake_at, Some(0.5));
    let frames = h.wait(0.6, &mut screen);
    let last = frames.last().unwrap();
    assert_eq!(raw_rects(&last.list), vec![Rect::new(460.0, 250.0, 40.0, 40.0)]);
    let keys: Vec<u64> = frames
        .iter()
        .filter_map(|f| f.list.cmds.iter().find_map(|c| if let Cmd::Raw { key, .. } = c { Some(*key) } else { None }))
        .collect();
    // It turns: each frame's picture differs.
    assert!(keys.len() >= 4 && keys.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn a_toast_fades_in_stays_and_goes() {
    let state = RefCell::new(ToastState::default());
    state.borrow_mut().show("Added to My List", false, 0.0);
    let mut screen = |ui: &mut Ui| toast(ui, &mut state.borrow_mut(), ToastPlacement::TopRight);
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    // 16 characters of 7.1 with 20 either side, its right edge at 912, 27 from the top.
    let w = 16.0 * 7.1 + 40.0;
    assert!(fills_of(&f.list, theme::SURFACE3).iter().any(|r| (r.x - (912.0 - w)).abs() < 0.01 && r.y == 27.0));
    assert!(f.list.cmds.contains(&Cmd::PushOpacity(0.0)));
    h.wait(1.0, &mut screen);
    let f = h.idle(&mut screen);
    assert!(!f.list.cmds.iter().any(|c| matches!(c, Cmd::PushOpacity(_))));
    h.wait(3.0, &mut screen);
    let f = h.idle(&mut screen);
    assert!(fills_of(&f.list, theme::SURFACE3).is_empty());
    assert_eq!(state.borrow().text(), None);
    // An error stays 6 s, in DangerText, at the bottom left on Home.
    state.borrow_mut().show("Couldn't add it", true, h.time());
    h.wait(4.0, &mut screen);
    let f = h.idle(&mut screen);
    assert_eq!(text_color(&f.list, "Couldn't add it"), Some(theme::DANGER_TEXT));
    let mut screen = |ui: &mut Ui| toast(ui, &mut state.borrow_mut(), ToastPlacement::BottomLeft);
    let f = h.idle(&mut screen);
    let r = fills_of(&f.list, theme::SURFACE3)[0];
    assert_eq!((r.x, r.bottom()), (96.0, 513.0));
}

#[test]
fn a_page_message_centres_itself_and_focuses_its_button() {
    let mut screen = |ui: &mut Ui| {
        Message::new(MessageStyle::Page, "Couldn't load your home screen")
            .body("Check that the server is on.")
            .action("Retry", pley_base::Icon::RefreshCw)
            .show(ui, Id::new("msg"));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("msg").with("action")));
    // 34 + 12 + 20 + 24 + 40 tall, in the middle of the page; centred in x 96–912.
    let top = (540.0 - 130.0) / 2.0;
    assert_eq!(text_at(&f.list, "Couldn't load your home screen"), Some(Point::new(272.0, top)));
    let pill = fills_of(&f.list, theme::ACCENT)[0];
    assert_eq!((pill.y, pill.w), (top + 90.0, 128.0));
    assert!((pill.center().x - 504.0).abs() < 0.01);
}

#[test]
fn an_item_message_leads_with_its_icon_at_the_gutter() {
    let mut screen = |ui: &mut Ui| {
        Message::new(MessageStyle::Item, "Item not found")
            .body("It may have been removed.")
            .action("Back", pley_base::Icon::ArrowLeft)
            .show(ui, Id::new("msg"));
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    let icon = f.list.cmds.iter().find_map(|c| match c {
        Cmd::Icon { rect, icon: pley_base::Icon::CircleAlert, color } => Some((*rect, *color)),
        _ => None,
    });
    let top = (540.0 - (56.0 + 34.0 + 8.0 + 20.0 + 24.0 + 40.0)) / 2.0;
    assert_eq!(icon, Some((Rect::new(96.0, top, 40.0, 40.0), theme::MUTED)));
    assert_eq!(text_at(&f.list, "Item not found"), Some(Point::new(96.0, top + 56.0)));
}

#[test]
fn badges_and_the_metadata_line() {
    let quality = vec!["4K".to_owned(), "DV".to_owned()];
    let width = Cell::new(0.0);
    let mut screen = |ui: &mut Ui| {
        Badge::live().show(ui, Point::new(96.0, 20.0));
        let line = MetaLine::new()
            .lead("S2 · E5 · The Long Night")
            .text("2019")
            .content_rating("TV-MA")
            .stars("7.4")
            .quality(&quality);
        width.set(line.show(ui, Point::new(96.0, 100.0), 464.0));
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    // LIVE: Danger with Bg text, 5 × 2 of padding: 13 + 4 tall.
    assert_eq!(text_color(&f.list, "LIVE"), Some(theme::BG));
    let live = fills_of(&f.list, theme::DANGER);
    assert_eq!(live.len(), 1);
    assert!(super::close_rect(live[0], Rect::new(96.0, 20.0, 4.0 * 5.9 + 10.0, 17.0)));
    // The lead in Fg first; the others Muted; "·" between them in Subtle.
    assert_eq!(text_color(&f.list, "S2 · E5 · The Long Night"), Some(theme::FG));
    assert_eq!(text_color(&f.list, "2019"), Some(theme::MUTED));
    assert_eq!(text_color(&f.list, "·"), Some(theme::SUBTLE));
    assert!(width.get() <= 464.0 + 0.01);
    // Too long: the lead gives way, cut with "…", and the rest keeps its place.
    let line_w = Cell::new(0.0);
    let mut narrow = |ui: &mut Ui| {
        let line = MetaLine::new().lead("A very long episode title that cannot fit").text("2019");
        line_w.set(line.show(ui, Point::new(96.0, 100.0), 200.0));
    };
    let f = h.idle(&mut narrow);
    assert!(f.list.texts().any(|t| t == "2019"));
    assert!(line_w.get() <= 200.0 + 0.01, "{}", line_w.get());
}

#[test]
fn the_backdrop_waits_for_rest_and_its_picture_then_crossfades() {
    let state = RefCell::new(BackdropState::default());
    let wanted = RefCell::new(BackdropArt::new(1, vec![ImageRequest::new("http://pley/b/1", 1280)], "Arrival"));
    let mut screen = |ui: &mut Ui| {
        let w = wanted.borrow().clone();
        Backdrop::new(BackdropScrim::Rows).show(ui, Id::new("backdrop"), &mut state.borrow_mut(), Some(&w));
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    // The first one shows at once: its placeholder while the picture comes, and both scrims.
    assert_eq!(state.borrow().showing(), Some(1));
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Quad { .. })));
    let gradients = f.list.cmds.iter().filter(|c| matches!(c, Cmd::LinearGradient { .. })).count();
    assert_eq!(gradients, 2);
    // A key moves focus: the next backdrop waits for rest...
    *wanted.borrow_mut() = BackdropArt::new(2, vec![ImageRequest::new("http://pley/b/2", 1280)], "Dune");
    h.step(Step::Press(Key::Right), &mut screen);
    h.wait(0.2, &mut screen);
    assert_eq!((state.borrow().showing(), state.borrow().waiting()), (Some(1), false));
    // ...then for its picture, at most a second.
    h.wait(0.3, &mut screen);
    assert!(state.borrow().waiting());
    h.images.ready("http://pley/b/2", 1280, 720, h.time());
    h.idle(&mut screen);
    assert_eq!(state.borrow().showing(), Some(2));
    // Halfway through the cross-fade, the new picture is at about half.
    h.wait(0.15, &mut screen);
    let f = h.idle(&mut screen);
    let o = f.list.cmds.iter().find_map(|c| if let Cmd::PushOpacity(o) = c { Some(*o) } else { None }).unwrap();
    assert!((o - 0.5).abs() < 0.1, "{o}");
    // A picture that never comes swaps in after a second anyway.
    *wanted.borrow_mut() = BackdropArt::new(3, vec![ImageRequest::new("http://pley/b/3", 1280)], "Heat");
    h.idle(&mut screen);
    assert!(state.borrow().waiting());
    h.wait(0.9, &mut screen);
    assert_eq!(state.borrow().showing(), Some(2));
    h.wait(0.2, &mut screen);
    assert_eq!(state.borrow().showing(), Some(3));
}

#[test]
fn the_info_block_fades_with_focus() {
    let visible = Cell::new(false);
    let mut screen = |ui: &mut Ui| {
        InfoBlock::show(ui, Id::new("info"), visible.get(), |ui| {
            ui.text(Point::new(96.0, 28.0), "Arrival", &theme::PAGE_TITLE, &pley_base::TextOpts::default(), theme::FG);
        });
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert!(!f.list.says("Arrival"));
    visible.set(true);
    h.idle(&mut screen);
    h.wait(0.075, &mut screen);
    let f = h.idle(&mut screen);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::PushOpacity(o) if (*o - 0.5).abs() < 0.1)));
    let f = settle(&mut h, &mut screen);
    assert!(f.list.says("Arrival") && !f.list.cmds.iter().any(|c| matches!(c, Cmd::PushOpacity(_))));
}

#[test]
fn the_gallery_draws_every_control() {
    let state = RefCell::new(GalleryState::default());
    let mut screen = |ui: &mut Ui| gallery(ui, Id::new("gallery"), &mut state.borrow_mut());
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert!(f.focused.is_some());
    assert!(f.list.cmds.len() > 50);
    // Walk down every section and back without losing focus.
    for _ in 0..30 {
        let f = h.step(Step::Press(Key::Down), &mut screen);
        assert!(f.focused.is_some());
    }
    for _ in 0..30 {
        h.step(Step::Press(Key::Up), &mut screen);
    }
    let f = settle(&mut h, &mut screen);
    assert!(f.focused.is_some());
}
