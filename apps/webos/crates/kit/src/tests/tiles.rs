//! Tiles: focus growth, the ring, the press, overlays, placeholders and pictures.

use pley_base::{Cmd, Icon, ImageId, ImageRequest, Key, Point, Rect};
use pley_ui::testing::{Harness, Step};
use pley_ui::theme;
use pley_ui::{Id, Ui};

use super::{close, close_rect, fills_of, scales, settle, strokes, text_at};
use crate::tile::{ProfileTile, Tile, TileKind};

fn poster(ui: &mut Ui, tile: Tile) {
    let id = ui.id("arrival");
    tile.show(ui, id, Point::new(96.0, 210.0));
}

#[test]
fn a_focused_tile_grows_to_1_08_inside_its_ring() {
    let mut screen = |ui: &mut Ui| poster(ui, Tile::new(TileKind::Poster, "Arrival").subtitle("2016"));
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    let well = Rect::new(96.0, 210.0, 108.0, 162.0);
    assert_eq!(scales(&f.list), vec![(1.08, well.center())]);
    let ring = well.scale_about(well.center(), 1.08).expand(4.5);
    let s = strokes(&f.list);
    let (rect, color, radius, width) = s.iter().copied().find(|s| s.1 == theme::ACCENT_BRIGHT).unwrap();
    assert!(close_rect(rect, ring));
    assert_eq!((color, width), (theme::ACCENT_BRIGHT, 3.0));
    assert!(close(radius, 4.0 * 1.08 + 4.5));
    // The ring reaches 6 dp past the grown edge; the caption stays put, 14 under the well (the
    // first "Arrival" is the placeholder's, in Fg at 70 %).
    assert!(close(rect.x - 1.5, 96.0 - 108.0 * 0.04 - 6.0));
    let caption = f.list.cmds.iter().find_map(|c| match c {
        Cmd::Text { origin, layout, color } if &*layout.text == "Arrival" && *color == theme::FG => Some(*origin),
        _ => None,
    });
    assert_eq!(caption, Some(Point::new(96.0, 386.0)));
    assert_eq!(text_at(&f.list, "2016"), Some(Point::new(96.0, 403.0)));
}

#[test]
fn ok_held_on_a_tile_shrinks_it_to_1_04() {
    let mut screen = |ui: &mut Ui| poster(ui, Tile::new(TileKind::Poster, "Arrival"));
    let mut h = Harness::new();
    h.idle(&mut screen);
    settle(&mut h, &mut screen);
    h.step(Step::Down(Key::Ok), &mut screen);
    h.wait(0.2, &mut screen);
    let f = h.idle(&mut screen);
    assert!(close(scales(&f.list)[0].0, 1.04));
    h.step(Step::Up(Key::Ok), &mut screen);
    let f = settle(&mut h, &mut screen);
    assert!(close(scales(&f.list)[0].0, 1.08));
}

#[test]
fn the_placeholder_shows_until_the_picture_covers_it() {
    let art = [ImageRequest::new("http://pley/poster/1", 240)];
    let mut screen = |ui: &mut Ui| poster(ui, Tile::new(TileKind::Poster, "Arrival").art(&art));
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    let quad = |l: &pley_base::DisplayList| l.cmds.iter().any(|c| matches!(c, Cmd::Quad { .. }));
    assert!(quad(&f.list));
    // The placeholder's title, at Fg 70 %.
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Text { layout, color, .. }
        if &*layout.text == "Arrival" && *color == theme::PLACEHOLDER_TITLE)));
    // Ready long ago: it shows at once on the well, with no placeholder under it.
    let id = h.images.ready("http://pley/poster/1", 240, 360, -10.0);
    let f = h.idle(&mut screen);
    assert!(!quad(&f.list));
    assert!(
        f.list.cmds.iter().any(|c| matches!(c, Cmd::Image { image, opacity, .. } if *image == id && *opacity == 1.0))
    );
    assert_eq!(fills_of(&f.list, theme::SURFACE2).len(), 1);
}

#[test]
fn a_picture_that_fails_gives_way_to_the_next_address() {
    let art = [ImageRequest::new("http://pley/own", 240), ImageRequest::new("http://pley/parent", 240)];
    let mut screen = |ui: &mut Ui| poster(ui, Tile::new(TileKind::Poster, "Arrival").art(&art));
    let mut h = Harness::new();
    h.idle(&mut screen);
    // Only the first is asked for while it may still come.
    assert!(h.images.requests().iter().all(|(r, _)| &*r.url == "http://pley/own"));
    h.images.fail("http://pley/own");
    let id = h.images.ready("http://pley/parent", 240, 360, -10.0);
    let f = h.idle(&mut screen);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Image { image, .. } if *image == id)));
    assert_ne!(id, ImageId(0));
}

#[test]
fn overlays_draw_progress_the_unwatched_count_or_the_tick() {
    let mut screen = |ui: &mut Ui| {
        let a = ui.id("a");
        Tile::new(TileKind::Poster, "In progress").progress(Some(0.5)).show(ui, a, Point::new(96.0, 210.0));
        let b = ui.id("b");
        Tile::new(TileKind::Poster, "Show").unwatched(3).watched(true).show(ui, b, Point::new(218.0, 210.0));
        let c = ui.id("c");
        Tile::new(TileKind::Poster, "Watched").watched(true).show(ui, c, Point::new(340.0, 210.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    // Focus on the last, so the first two draw at rest.
    h.step(Step::Press(Key::Right), &mut screen);
    h.step(Step::Press(Key::Right), &mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("c")));
    // A 4 dp bar along the foot, half of it Accent.
    assert!(fills_of(&f.list, theme::PROGRESS_TRACK_ON_ART).contains(&Rect::new(96.0, 368.0, 108.0, 4.0)));
    assert!(fills_of(&f.list, theme::ACCENT).contains(&Rect::new(96.0, 368.0, 54.0, 4.0)));
    // The count instead of the tick: 18 tall, at least 20 wide, 6 in from the top right.
    assert!(fills_of(&f.list, theme::ACCENT).contains(&Rect::new(218.0 + 108.0 - 6.0 - 20.0, 216.0, 20.0, 18.0)));
    assert_eq!(text_at(&f.list, "3").map(|p| p.y), Some(216.0 + 2.5));
    // The tick: a 20 dp box on art with a 12 dp check.
    let tick = Rect::new(340.0 + 108.0 - 6.0 - 20.0, 216.0, 20.0, 20.0);
    assert!(fills_of(&f.list, theme::ON_ART).contains(&tick));
    let checks = f.list.cmds.iter().filter(|c| matches!(c, Cmd::Icon { icon: Icon::Check, .. })).count();
    assert_eq!(checks, 1);
}

#[test]
fn round_tiles_have_round_rings_and_no_hairline() {
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("person");
        Tile::new(TileKind::Person, "Amy Adams").subtitle("Louise Banks").show(ui, id, Point::new(96.0, 210.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    let s = strokes(&f.list);
    assert_eq!(s.len(), 1);
    let grown = 96.0 * 1.08 + 9.0;
    assert!(close(s[0].0.w, grown) && close(s[0].2, grown / 2.0));
    // No title on a person's placeholder.
    assert_eq!(f.list.texts().filter(|t| *t == "Amy Adams").count(), 1);
}

#[test]
fn a_see_all_tile_says_so_and_has_no_caption() {
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("all");
        Tile::see_all(TileKind::Poster, "See all").show(ui, id, Point::new(96.0, 210.0));
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert_eq!(f.list.texts().collect::<Vec<_>>(), vec!["See all"]);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Icon { icon: Icon::ArrowRight, rect, .. } if rect.w == 32.0)));
}

#[test]
fn a_profile_grows_to_1_1_and_says_its_name_under_it() {
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("anna");
        ProfileTile::new("anna").locked(true).show(ui, id, Point::new(430.0, 200.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert!(close(scales(&f.list)[0].0, 1.1));
    // The initial on the profile's colour, the name 12 under the circle, centred.
    assert!(f.list.says("A"));
    let at = text_at(&f.list, "anna").unwrap();
    assert_eq!(at.y, 312.0);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Icon { icon: Icon::Lock, .. })));
    assert!(fills_of(&f.list, theme::profile_color("anna")).iter().any(|r| r.w == 100.0));
}
