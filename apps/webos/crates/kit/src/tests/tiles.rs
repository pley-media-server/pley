//! Tiles: focus growth, the ring, the press, overlays, placeholders and pictures.

use pley_base::{Cmd, Color, DisplayList, Icon, ImageId, ImageRequest, Key, MarkGlyph, Point, Rect};
use pley_ui::testing::{Harness, Step};
use pley_ui::theme;
use pley_ui::{Id, Ui};

use super::{close, close_rect, fills_of, scales, settle, strokes, text_at, text_color};
use crate::tile::{ProfileTile, Tile, TileKind, placeholder};

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
    let (rect, color, radius, width) = s.iter().copied().find(|s| s.1 == theme::accent_bright()).unwrap();
    assert!(close_rect(rect, ring));
    assert_eq!((color, width), (theme::accent_bright(), 3.0));
    assert!(close(radius, 4.0 * 1.08 + 4.5));
    // The ring reaches 6 dp past the grown edge; the caption stays put, 14 under the well.
    assert!(close(rect.x - 1.5, 96.0 - 108.0 * 0.04 - 6.0));
    let caption = f.list.cmds.iter().find_map(|c| match c {
        Cmd::Text { origin, layout, color } if &*layout.text == "Arrival" && *color == theme::fg() => Some(*origin),
        _ => None,
    });
    assert_eq!(caption, Some(Point::new(96.0, 386.0)));
    assert_eq!(text_at(&f.list, "2016"), Some(Point::new(96.0, 403.0)));
}

#[test]
fn under_a_held_key_a_tiles_focus_glides_from_tile_to_tile() {
    let mut screen = |ui: &mut Ui| {
        let column = ui.id("column");
        ui.group(column, pley_ui::Group::column(), |ui| {
            for i in 0..30u64 {
                Tile::new(TileKind::Poster, "T").show(ui, column.with(i), Point::new(96.0, 20.0 + i as f32 * 220.0));
            }
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let mut frames = h.run(&[Step::Down(Key::Down), Step::Wait(0.4)], &mut screen);
    for _ in 0..8 {
        frames.extend(h.run(&[Step::Repeat(Key::Down), Step::Wait(0.05)], &mut screen));
    }
    // Once the hold is under way, each tile grows as focus comes and shrinks as it goes, evenly
    // into the next move, rather than popping to 1.08 and back.
    let held: Vec<_> = frames.iter().filter(|f| f.time >= 0.25).collect();
    let between = |s: f32| s > 1.001 && s < 1.079;
    let gliding = held.iter().filter(|f| scales(&f.list).iter().any(|&(s, _)| between(s))).count();
    assert!(gliding * 2 > held.len(), "{gliding} of {}", held.len());
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
    let quad = |l: &DisplayList| l.cmds.iter().any(|c| matches!(c, Cmd::Quad { .. }));
    assert!(quad(&f.list));
    // Ready long ago: it shows at once on the well, with no placeholder under it.
    let id = h.images.ready("http://pley/poster/1", 240, 360, -10.0);
    let f = h.idle(&mut screen);
    assert!(!quad(&f.list));
    assert!(
        f.list.cmds.iter().any(|c| matches!(c, Cmd::Image { image, opacity, .. } if *image == id && *opacity == 1.0))
    );
    assert_eq!(fills_of(&f.list, theme::surface2()).len(), 1);
}

/// Where Pley's mark is drawn, with which glyph, and in what.
fn marks(list: &DisplayList) -> Vec<(Rect, MarkGlyph, Color)> {
    list.cmds
        .iter()
        .filter_map(|c| match c {
            Cmd::Icon { rect, icon: Icon::PleyMark(glyph), color } => Some((*rect, *glyph, *color)),
            _ => None,
        })
        .collect()
}

/// How many times `text` is drawn.
fn times(list: &DisplayList, text: &str) -> usize {
    list.texts().filter(|t| *t == text).count()
}

#[test]
fn a_tile_without_a_picture_shows_its_kinds_mark_and_not_its_title() {
    let art = [ImageRequest::new("http://pley/poster/1", 240)];
    let mut screen = |ui: &mut Ui| {
        let a = ui.id("none");
        Tile::new(TileKind::Poster, "Arrival").mark(Some(MarkGlyph::Movies)).show(ui, a, Point::new(96.0, 210.0));
        let b = ui.id("failed");
        let show = Tile::new(TileKind::Poster, "Severance").art(&art).mark(Some(MarkGlyph::Shows));
        show.show(ui, b, Point::new(218.0, 210.0));
    };
    let mut h = Harness::new();
    h.images.fail("http://pley/poster/1");
    let f = h.idle(&mut screen);
    // Without a picture, and when its picture can't be had: the mark with the tile's glyph in the
    // middle, 40 % of the well's shorter side, in white (its posters carry their own alphas).
    let side = 108.0 * 0.4;
    let mid = |x: f32| Rect::new(x + (108.0 - side) / 2.0, 210.0 + (162.0 - side) / 2.0, side, side);
    let m = marks(&f.list);
    assert_eq!(m.len(), 2);
    assert!(close_rect(m[0].0, mid(96.0)) && close_rect(m[1].0, mid(218.0)), "{m:?}");
    assert_eq!((m[0].1, m[1].1), (MarkGlyph::Movies, MarkGlyph::Shows));
    assert!(m.iter().all(|(_, _, c)| *c == Color::WHITE));
    // The caption names the tile; the placeholder doesn't say it again.
    assert_eq!((times(&f.list, "Arrival"), times(&f.list, "Severance")), (1, 1));
    assert_eq!(text_color(&f.list, "Arrival"), Some(theme::fg()));
}

#[test]
fn a_tile_given_no_kind_shows_the_gradient_alone() {
    let mut screen = |ui: &mut Ui| poster(ui, Tile::new(TileKind::Poster, "Arrival"));
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Quad { .. })));
    assert!(marks(&f.list).is_empty());
}

#[test]
fn a_tile_whose_picture_loads_shows_the_gradient_alone() {
    // A page of tiles scrolling in shows no marks before its pictures.
    let art = [ImageRequest::new("http://pley/poster/1", 240)];
    let mut screen =
        |ui: &mut Ui| poster(ui, Tile::new(TileKind::Poster, "Arrival").art(&art).mark(Some(MarkGlyph::Movies)));
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Quad { .. })));
    assert!(marks(&f.list).is_empty());
    assert_eq!(times(&f.list, "Arrival"), 1);
}

#[test]
fn a_placeholder_given_a_title_puts_the_mark_above_it_where_there_is_room() {
    let cover = Rect::new(96.0, 210.0, 132.0, 132.0);
    let small = Rect::new(300.0, 210.0, 96.0, 40.0);
    let mut screen = |ui: &mut Ui| {
        // A playlist's cover whose picture can't be had, and one whose picture is coming.
        placeholder(ui, cover, "Road trip", "Road trip", Some(MarkGlyph::Music), 4.0);
        placeholder(ui, cover.translate(0.0, 160.0), "Coming", "Coming", None, 4.0);
        // A box the title fills: no mark.
        placeholder(ui, small, "Night drive", "Night drive", Some(MarkGlyph::Music), 4.0);
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    let m = marks(&f.list);
    assert_eq!(m.len(), 1, "{m:?}");
    assert_eq!(m[0].1, MarkGlyph::Music);
    // 26 % of the shorter side, centered across; the title 10 under it, the two centered down.
    let side = 132.0 * 0.26;
    let title = text_at(&f.list, "Road trip").unwrap();
    let title_h = f.list.cmds.iter().find_map(|c| match c {
        Cmd::Text { layout, .. } if &*layout.text == "Road trip" => Some(layout.size.h),
        _ => None,
    });
    let top = cover.y + (cover.h - side - 10.0 - title_h.unwrap()) / 2.0;
    assert!(close_rect(m[0].0, Rect::new(cover.x + (cover.w - side) / 2.0, top, side, side)), "{m:?}");
    assert!(close(title.y, top + side + 10.0));
    assert_eq!(text_color(&f.list, "Road trip"), Some(theme::PLACEHOLDER_TITLE));
    assert!(f.list.says("Coming") && f.list.says("Night drive"));
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
    assert!(fills_of(&f.list, theme::accent()).contains(&Rect::new(96.0, 368.0, 54.0, 4.0)));
    // The count instead of the tick: 18 tall, at least 20 wide, 6 in from the top right.
    assert!(fills_of(&f.list, theme::accent()).contains(&Rect::new(218.0 + 108.0 - 6.0 - 20.0, 216.0, 20.0, 18.0)));
    assert_eq!(text_at(&f.list, "3").map(|p| p.y), Some(216.0 + 2.5));
    // The tick: a 20 dp box on art with a 12 dp check.
    let tick = Rect::new(340.0 + 108.0 - 6.0 - 20.0, 216.0, 20.0, 20.0);
    assert!(fills_of(&f.list, theme::on_art()).contains(&tick));
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
fn a_round_well_is_surface2_until_the_photo_covers_it() {
    let art = [ImageRequest::new("http://pley/person/7", 192)];
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("person");
        Tile::new(TileKind::Person, "Amy Adams").art(&art).show(ui, id, Point::new(96.0, 210.0));
    };
    let mut h = Harness::new();
    let f = h.idle(&mut screen);
    // No name-seeded gradient: the circle is Surface2, round.
    assert!(!f.list.cmds.iter().any(|c| matches!(c, Cmd::Quad { .. })));
    let well = fills_of(&f.list, theme::surface2());
    assert_eq!(well, [Rect::new(96.0, 210.0, 96.0, 96.0)]);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Fill { color, radius, .. }
        if *color == theme::surface2() && close(*radius, 48.0))));
    // The photo comes over the same well.
    let id = h.images.ready("http://pley/person/7", 192, 192, -10.0);
    let f = h.idle(&mut screen);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Image { image, .. } if *image == id)));
    assert_eq!(fills_of(&f.list, theme::surface2()).len(), 1);
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
    // The initial on the profile's color, the name 12 under the circle, centered.
    assert!(f.list.says("A"));
    let at = text_at(&f.list, "anna").unwrap();
    assert_eq!(at.y, 312.0);
    assert!(f.list.cmds.iter().any(|c| matches!(c, Cmd::Icon { icon: Icon::Lock, .. })));
    let fill = pley_ui::avatar::color("anna", None);
    assert!(fills_of(&f.list, fill).iter().any(|r| r.w == 100.0));
    assert_eq!(text_color(&f.list, "A"), Some(pley_ui::avatar::ink(fill)));
}

/// A profile's own color, `#rgb` too, under any theme; a blank name shows "?".
#[test]
fn a_profile_is_drawn_in_its_own_colors_whatever_the_theme() {
    use pley_ui::avatar::{INK_DARK, INK_LIGHT};
    use pley_ui::theme::{Palette, Theme};
    for theme in [Theme::Midnight, Theme::Midday] {
        theme::show(Palette::of(theme, None));
        for (name, own, fill, letter, ink) in [
            ("ben", Some("#7c3aed"), 0x7c3aed, "B", INK_LIGHT),
            ("mia", Some("#abc"), 0xaabbcc, "M", INK_DARK),
            ("  ", None, 0x38bdf8, "?", INK_DARK),
        ] {
            let mut screen =
                |ui: &mut Ui| ProfileTile::new(name).color(own).avatar(ui, Rect::new(0.0, 0.0, 48.0, 48.0));
            let f = Harness::new().idle(&mut screen);
            assert!(fills_of(&f.list, Color::rgb(fill)).iter().any(|r| r.w == 48.0), "{name:?} in {theme:?}");
            assert_eq!(text_color(&f.list, letter), Some(ink), "{name:?} in {theme:?}");
        }
    }
    theme::show(Palette::of(Theme::Midnight, None));
}
