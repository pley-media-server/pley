//! Rows, pages of rows, grids and lists: where focus keeps things, what is drawn, memory, edges.

use std::cell::{Cell, RefCell};

use pley_base::{Cmd, Key, Point, Priority, Rect};
use pley_ui::testing::{Harness, Step};
use pley_ui::theme;
use pley_ui::{Dir, Edge, Group, Id, Ui};

use super::{close, press, settle};
use crate::button::Button;
use crate::column::{ColumnState, RowColumn, Section};
use crate::grid::{GridState, JumpControl, PosterGrid};
use crate::list::{List, ListRow, ListState};
use crate::row::{Row, RowState};
use crate::tile::{SlotMode, Tile, TileKind, TileSlot};

/// A screen with one row of `len` posters; records the slots drawn each frame.
struct OneRow {
    state: RowState,
    len: usize,
    drawn: Vec<TileSlot>,
}

impl OneRow {
    fn new(len: usize) -> RefCell<OneRow> {
        RefCell::new(OneRow { state: RowState::default(), len, drawn: Vec::new() })
    }
}

fn one_row(cell: &RefCell<OneRow>) -> impl FnMut(&mut Ui) + '_ {
    move |ui: &mut Ui| {
        let mut s = cell.borrow_mut();
        let s = &mut *s;
        s.drawn.clear();
        let drawn = &mut s.drawn;
        Row::new(TileKind::Poster, 172.0).header("Recently Added").show(
            ui,
            Id::new("row"),
            &mut s.state,
            s.len,
            |i| i as u64,
            |ui, slot| {
                drawn.push(*slot);
                Tile::new(TileKind::Poster, "Title").paint(ui, slot);
            },
        );
    }
}

fn focused_slot(cell: &RefCell<OneRow>) -> TileSlot {
    *cell.borrow().drawn.iter().find(|s| s.response.focused).unwrap()
}

#[test]
fn a_row_keeps_its_focused_tile_at_the_gutter() {
    let cell = OneRow::new(20);
    let mut screen = one_row(&cell);
    let mut h = Harness::new();
    h.idle(&mut screen);
    for i in 1..=5 {
        assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("row").with(i as u64)));
        let slot = focused_slot(&cell);
        assert_eq!((slot.index, slot.rect.x, slot.rect.y), (i, 96.0, 210.0));
    }
    assert_eq!(cell.borrow().state.offset, 5.0 * 122.0);
}

#[test]
fn a_row_that_fits_never_moves_and_the_end_comes_no_further_than_its_padding() {
    let cell = OneRow::new(5);
    let mut screen = one_row(&cell);
    let mut h = Harness::new();
    h.idle(&mut screen);
    for _ in 0..4 {
        press(&mut h, Key::Right, &mut screen);
    }
    assert_eq!((focused_slot(&cell).index, focused_slot(&cell).rect.x), (4, 96.0 + 4.0 * 122.0));

    let cell = OneRow::new(20);
    let mut screen = one_row(&cell);
    let mut h = Harness::new();
    h.idle(&mut screen);
    for _ in 0..19 {
        h.step(Step::Press(Key::Right), &mut screen);
    }
    settle(&mut h, &mut screen);
    let slot = focused_slot(&cell);
    // The last tile ends 48 + a tile's width from the screen's edge: focus walked right.
    assert_eq!(slot.index, 19);
    assert!(close(slot.rect.right(), 960.0 - 48.0 - 108.0));
    // Right at the end stays.
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(Id::new("row").with(19u64)));
}

#[test]
fn a_row_draws_only_what_is_near_the_screen_and_asks_ahead_for_pictures() {
    let cell = OneRow::new(500);
    let mut screen = one_row(&cell);
    let mut h = Harness::new();
    h.idle(&mut screen);
    for _ in 0..100 {
        h.step(Step::Press(Key::Right), &mut screen);
    }
    settle(&mut h, &mut screen);
    let c = cell.borrow();
    let drawn: Vec<usize> = c.drawn.iter().filter(|s| s.mode == SlotMode::Draw).map(|s| s.index).collect();
    let ahead: Vec<usize> = c.drawn.iter().filter(|s| s.mode == SlotMode::Prefetch).map(|s| s.index).collect();
    // On screen from x 0 to 960 with the 100th tile at 96: the 99th to the 107th, and one more
    // either side.
    assert_eq!(drawn.iter().min(), Some(&98));
    assert_eq!(drawn.iter().max(), Some(&108));
    assert_eq!(drawn.len(), 11);
    // 1.5 screens of 8 tiles' pictures past them.
    assert_eq!(ahead, (109..121).collect::<Vec<_>>());
    // The focused tile is drawn last of those on screen, over its neighbours; the rest only ask.
    assert!(c.drawn[10].response.focused);
}

#[test]
fn a_held_key_glides_the_row_from_tile_to_tile_under_one_still_ring() {
    let cell = OneRow::new(60);
    let shown = RefCell::new(Vec::new());
    let slots = RefCell::new(Vec::new());
    let mut inner = one_row(&cell);
    let mut screen = |ui: &mut Ui| {
        inner(ui);
        let c = cell.borrow();
        shown.borrow_mut().push(c.state.shown_offset());
        slots.borrow_mut().push(c.drawn.iter().map(|s| (s.mode, s.priority)).collect::<Vec<_>>());
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    shown.borrow_mut().clear();
    slots.borrow_mut().clear();
    let frames = hold(&mut h, Key::Right, &mut screen);
    // Each move glides evenly into the next: never more than half a tile a frame, and once under
    // way never resting on a tile.
    let v = shown.borrow().clone();
    assert!(v.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] < 61.0), "{v:?}");
    assert!(v[v.len() / 2..].iter().all(|x| x % 122.0 > 0.5), "{v:?}");
    assert!(cell.borrow().state.index >= 18);
    // Meanwhile one ring stays still at the gutter over the tiles at rest, and the pictures wait:
    // nothing is fetched ahead, and the tiles on screen ask behind everything else.
    let held: Vec<_> = frames.iter().filter(|f| f.time >= 0.25).collect();
    assert!(held.len() > 30);
    let ring = rings(&held[0].list);
    assert_eq!(ring.len(), 1);
    assert!(close(ring[0].x, 96.0 - theme::focus::RING_OFFSET), "{ring:?}");
    for f in &held {
        assert_eq!(rings(&f.list), ring, "at {}", f.time);
    }
    assert!(super::scales(&held.last().unwrap().list).is_empty(), "a tile grew under the ring");
    let late = slots.borrow().len() - held.len();
    for frame in &slots.borrow()[late..] {
        assert!(frame.iter().all(|&(mode, priority)| mode == SlotMode::Draw && priority == Priority::Prefetch));
    }
    // Let go, the last glide lands with the focused tile at the gutter, grown into its own ring,
    // and the pictures load.
    h.step(Step::Up(Key::Right), &mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(focused_slot(&cell).rect.x, 96.0);
    let rings = rings(&f.list);
    assert_eq!(rings.len(), 1);
    assert!(rings[0].w > ring[0].w);
    assert!(cell.borrow().drawn.iter().any(|s| s.priority == Priority::Visible));
    assert!(cell.borrow().drawn.iter().any(|s| s.mode == SlotMode::Prefetch));
}

#[test]
fn a_held_key_steps_focus_across_a_rows_last_tiles() {
    let cell = OneRow::new(12);
    let mut screen = one_row(&cell);
    let mut h = Harness::new();
    h.idle(&mut screen);
    let frames = hold(&mut h, Key::Right, &mut screen);
    // The tiles stop coming to the gutter: the ring goes with focus across the last of them.
    let last = frames.last().unwrap();
    let ring = rings(&last.list);
    assert_eq!(ring.len(), 1);
    assert!(ring[0].x > 96.0 + 122.0, "{ring:?}");
    assert_eq!(focused_slot(&cell).index, 11);
    h.step(Step::Up(Key::Right), &mut screen);
    settle(&mut h, &mut screen);
    assert!(close(focused_slot(&cell).rect.right(), 960.0 - 48.0 - 108.0));
}

#[test]
fn left_at_a_rows_start_leaves_the_page_and_the_pointer_does_not_recentre() {
    let cell = OneRow::new(20);
    let failed = Cell::new(None);
    let mut inner = one_row(&cell);
    let mut screen = |ui: &mut Ui| {
        inner(ui);
        if let Some(d) = ui.failed_move() {
            failed.set(Some(d));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    press(&mut h, Key::Left, &mut screen);
    assert_eq!(failed.get(), Some(Dir::Left));
    // Pointing at the fourth tile focuses it where it is.
    h.step(Step::Point(Point::new(96.0 + 3.0 * 122.0 + 50.0, 290.0)), &mut screen);
    settle(&mut h, &mut screen);
    assert_eq!((focused_slot(&cell).index, focused_slot(&cell).rect.x), (3, 96.0 + 3.0 * 122.0));
    // One cut off at the right edge comes in only as far as it must.
    h.step(Step::Point(Point::new(96.0 + 7.0 * 122.0 + 5.0, 290.0)), &mut screen);
    settle(&mut h, &mut screen);
    let slot = focused_slot(&cell);
    assert_eq!(slot.index, 7);
    assert!(close(slot.rect.right(), 960.0));
}

#[test]
fn a_page_of_rows_puts_the_focused_row_on_its_line_and_clips_above_160() {
    let state = RefCell::new(ColumnState::default());
    let rows = [RowState::default(); 4];
    let rows = RefCell::new(rows);
    let tiles_y = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let sections = [
            Section::hero(0, 328.0),
            Section::row(1, TileKind::Poster.row_height(true)),
            Section::row(2, TileKind::Poster.row_height(true)),
            Section::row(3, TileKind::Poster.row_height(true)),
        ];
        let mut rows = rows.borrow_mut();
        let mut ys = tiles_y.borrow_mut();
        ys.clear();
        RowColumn::new(28.0).show(ui, Id::new("page"), &mut state.borrow_mut(), &sections, |ui, i, y| {
            if i == 0 {
                ui.group(Id::new("hero"), Group::row(), |ui| {
                    let id = ui.id("resume");
                    Button::pill("Resume").show(ui, id, Point::new(96.0, y + 280.0))
                });
                return;
            }
            let row = Row::new(TileKind::Poster, y).header("Row");
            row.show(
                ui,
                Id::new("row").with(i),
                &mut rows[i],
                10,
                |k| k as u64,
                |ui, slot| {
                    if slot.response.focused {
                        ys.push((i, slot.rect.y));
                    }
                    Tile::new(TileKind::Poster, "T").paint(ui, slot);
                },
            );
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    // At rest the hero shows from the top: the first row's header at 28 + 328 + 28 = 384.
    assert_eq!(state.borrow().shown_shift(), 0.0);
    assert!(!f.list.cmds.iter().any(|c| matches!(c, Cmd::PushClip { .. })));
    press(&mut h, Key::Down, &mut screen);
    // The first row's header on the line at 172: its tiles at 210.
    assert_eq!(*tiles_y.borrow(), vec![(1, 210.0)]);
    press(&mut h, Key::Down, &mut screen);
    let f = h.idle(&mut screen);
    assert_eq!(*tiles_y.borrow(), vec![(2, 210.0)]);
    assert!(f.list.cmds.contains(&Cmd::PushClip { rect: Rect::new(0.0, 160.0, 960.0, 380.0) }));
    // Up twice: back on the hero's button, the page at its top.
    press(&mut h, Key::Up, &mut screen);
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(Id::new("hero").with("resume")));
    assert_eq!(state.borrow().shown_shift(), 0.0);
}

#[test]
fn rows_come_back_to_their_last_tile() {
    let rows = RefCell::new([RowState::default(); 2]);
    let state = RefCell::new(ColumnState::default());
    let mut screen = |ui: &mut Ui| {
        let sections = [Section::row(0, 248.0), Section::row(1, 248.0)];
        let mut rows = rows.borrow_mut();
        RowColumn::new(172.0).show(ui, Id::new("page"), &mut state.borrow_mut(), &sections, |ui, i, y| {
            Row::new(TileKind::Poster, y).header("Row").show(
                ui,
                Id::new("row").with(i),
                &mut rows[i],
                10,
                |k| k as u64,
                |ui, s| {
                    Tile::new(TileKind::Poster, "T").paint(ui, s);
                },
            );
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    for _ in 0..3 {
        press(&mut h, Key::Right, &mut screen);
    }
    let row = |i: usize, k: u64| Some(Id::new("row").with(i).with(k));
    // Down enters the next row at the nearest tile: both rows keep their tiles at the gutter.
    assert_eq!(press(&mut h, Key::Down, &mut screen), row(1, 0));
    assert_eq!(press(&mut h, Key::Up, &mut screen), row(0, 3));
    // The Magic Remote's wheel steps between rows a notch at a time, as Down and Up do.
    h.step(Step::Point(Point::new(500.0, 100.0)), &mut screen);
    h.step(Step::Wheel(-1.0), &mut screen);
    assert_eq!(settle(&mut h, &mut screen).focused, row(1, 0));
    h.step(Step::Wheel(1.0), &mut screen);
    assert_eq!(settle(&mut h, &mut screen).focused, row(0, 3));
}

#[test]
fn drawn_inert_nothing_takes_focus() {
    let mut screen = |ui: &mut Ui| {
        crate::look::inert(ui, |ui| {
            let a = ui.id("a");
            Button::pill("Play").show(ui, a, Point::new(96.0, 300.0));
        });
        let b = ui.id("b");
        Button::pill("Info").show(ui, b, Point::new(300.0, 300.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("b")));
    assert!(f.list.says("Play"));
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(Id::new("b")));
    assert!(!crate::look::is_inert());
}

#[test]
fn a_step_row_steps_with_left_and_right_and_ok_resets() {
    let value = Cell::new(0i32);
    let mut screen = |ui: &mut Ui| {
        let text = format!("{:+} ms", value.get() * 50);
        let id = ui.id("sync");
        let r = crate::list::Stepper::new("Sync", &text).show(ui, id, Point::new(624.0, 100.0), 288.0);
        value.set(if r.reset { 0 } else { value.get() + r.step });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    press(&mut h, Key::Right, &mut screen);
    press(&mut h, Key::Right, &mut screen);
    assert_eq!(value.get(), 2);
    // It keeps focus: Left steps back rather than leaving.
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(Id::new("sync")));
    assert_eq!(value.get(), 1);
    let f = h.step(Step::Press(Key::Ok), &mut screen);
    assert_eq!(value.get(), 0);
    // It asked for the frame that shows the new value.
    assert_eq!(f.output.wake_at, Some(h.time()));
}

#[test]
fn a_switch_row_turns_its_thumb_and_takes_the_accent_when_focused() {
    let on = Cell::new(false);
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("motion");
        let row = ListRow::setting("Reduce motion")
            .supporting("Scales and slides snap")
            .trailing(crate::list::Trailing::Switch(on.get()));
        if row.show(ui, id, Point::new(376.0, 80.0), 536.0).activated {
            on.set(!on.get());
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    // Focused: an Accent fill, the track OnAccent at 30 % with an OnAccent thumb at its left.
    let track =
        Rect::new(376.0 + 536.0 - 20.0 - 40.0, 80.0 + (f32::max(44.0, 20.0 + 17.0 + 16.0) - 22.0) / 2.0, 40.0, 22.0);
    assert!(super::fills_of(&f.list, theme::ON_ACCENT.alpha(0.3)).contains(&track));
    h.step(Step::Press(Key::Ok), &mut screen);
    let f = settle(&mut h, &mut screen);
    assert!(on.get());
    assert!(super::fills_of(&f.list, theme::ON_ACCENT).contains(&track));
    let thumb = Rect::new(track.right() - 3.0 - 16.0, track.y + 3.0, 16.0, 16.0);
    assert!(super::fills_of(&f.list, theme::ACCENT).contains(&thumb));
}

#[test]
fn a_library_grid_has_seven_columns_and_keeps_the_focused_row_at_136() {
    let state = RefCell::new(GridState::default());
    let focused = Cell::new(None);
    let count = Cell::new(0);
    let jump = Id::new("jump");
    let letter = Cell::new(0);
    let mut screen = |ui: &mut Ui| {
        let grid = PosterGrid::new(TileKind::LibraryGrid, 122.0).right_edge(Edge::To(jump));
        count.set(0);
        let r = grid.show(ui, Id::new("grid"), &mut state.borrow_mut(), 10_000, |ui, slot| {
            if slot.mode == SlotMode::Draw {
                count.set(count.get() + 1);
            }
            if slot.response.focused {
                focused.set(Some((slot.index, slot.rect)));
            }
            Tile::new(TileKind::LibraryGrid, "T").paint(ui, slot);
        });
        assert_eq!(r.columns, 7);
        let mut l = letter.get();
        JumpControl { available: &[true; 27], current: 1 }.show(ui, jump, Point::new(872.0, 136.0), &mut l);
        letter.set(l);
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    press(&mut h, Key::Down, &mut screen);
    press(&mut h, Key::Down, &mut screen);
    let (i, rect) = focused.get().unwrap();
    assert_eq!((i, rect), (14, Rect::new(96.0, 136.0, 96.0, 144.0)));
    // Rows on screen and one either side: never the whole library.
    assert!(count.get() <= 5 * 7, "{}", count.get());
    // Right along the row, then past the last column to the jump control.
    for _ in 0..6 {
        press(&mut h, Key::Right, &mut screen);
    }
    assert_eq!(focused.get().unwrap().0, 20);
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(jump));
    // Its letters step with Up and Down; Left goes back to the grid.
    press(&mut h, Key::Down, &mut screen);
    assert_eq!(letter.get(), 2);
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(Id::new("grid").with(20u64)));
}

#[test]
fn a_grid_never_scrolls_past_its_end_and_jumps_to_a_cell() {
    let state = RefCell::new(GridState::default());
    let focused = Cell::new(None);
    let len = Cell::new(10);
    let mut screen = |ui: &mut Ui| {
        let grid = PosterGrid::new(TileKind::LibraryGrid, 122.0);
        grid.show(ui, Id::new("grid"), &mut state.borrow_mut(), len.get(), |ui, slot| {
            if slot.response.focused {
                focused.set(Some((slot.index, slot.rect.y)));
            }
            Tile::new(TileKind::LibraryGrid, "T").paint(ui, slot);
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    // Two rows: 14 + 2 × 212 − 20 + 41 = 459 of content in 418: 41 to scroll.
    press(&mut h, Key::Down, &mut screen);
    assert_eq!(focused.get(), Some((7, 136.0 + 212.0 - 41.0)));
    len.set(3000);
    state.borrow_mut().focus_to(2000);
    h.idle(&mut screen);
    let f = settle(&mut h, &mut screen);
    assert_eq!(f.focused, Some(Id::new("grid").with(2000u64)));
    assert_eq!(focused.get().unwrap().1, 136.0);
}

/// Holds `key` for a second, the TV repeating every 50 ms from 400 ms, then lets it go; the
/// frames drawn meanwhile.
fn hold(h: &mut Harness, key: Key, screen: &mut dyn FnMut(&mut Ui)) -> Vec<pley_ui::testing::Frame> {
    let mut frames = h.run(&[Step::Down(key), Step::Wait(0.4)], screen);
    for _ in 0..12 {
        frames.extend(h.run(&[Step::Repeat(key), Step::Wait(0.05)], screen));
    }
    frames
}

/// The focus rings a frame draws.
fn rings(list: &pley_base::DisplayList) -> Vec<Rect> {
    let ring = theme::focus::RING_COLOR;
    super::strokes(list)
        .into_iter()
        .filter(|s| s.1.a > 0 && (s.1.r, s.1.g, s.1.b) == (ring.r, ring.g, ring.b))
        .map(|s| s.0)
        .collect()
}

#[test]
fn a_held_down_glides_the_grid_under_one_still_ring() {
    let state = RefCell::new(GridState::default());
    let shown = RefCell::new(Vec::new());
    let slots = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let mut s = state.borrow_mut();
        slots.borrow_mut().clear();
        PosterGrid::new(TileKind::LibraryGrid, 122.0).show(ui, Id::new("grid"), &mut s, 10_000, |ui, slot| {
            slots.borrow_mut().push((slot.mode, slot.priority));
            Tile::new(TileKind::LibraryGrid, "T").paint(ui, slot);
        });
        shown.borrow_mut().push(s.shown_scroll());
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    shown.borrow_mut().clear();
    let frames = hold(&mut h, Key::Down, &mut screen);
    // Once the hold is under way: one ring, still, on the focused line; the rows' pictures wait,
    // and nothing is fetched ahead.
    let held: Vec<_> = frames.iter().filter(|f| f.time >= 0.25).collect();
    assert!(held.len() > 30);
    for f in &held {
        assert_eq!(rings(&f.list).len(), 1, "at {}", f.time);
    }
    let ring = rings(&held[0].list)[0];
    assert!(close(ring.y, 136.0 - theme::focus::RING_OFFSET), "{ring:?}");
    assert!(held.iter().all(|f| rings(&f.list)[0] == ring));
    assert!(slots.borrow().iter().all(|&(mode, priority)| mode == SlotMode::Draw && priority == Priority::Prefetch));
    // Meanwhile the rows glide: never more than half a row a frame, and on their way between rows.
    let v = shown.borrow().clone();
    assert!(v.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] < 106.0), "{v:?}");
    let pitch = 212.0;
    assert!(v[v.len() / 2..].iter().all(|x| x % pitch > 0.5), "{v:?}");
    assert!(*v.last().unwrap() > 15.0 * pitch);
    // Let go, the focused tile grows into its own ring and the pictures load.
    h.step(Step::Up(Key::Down), &mut screen);
    let f = settle(&mut h, &mut screen);
    let rings = rings(&f.list);
    assert_eq!(rings.len(), 1);
    assert!(rings[0].w > ring.w);
    assert!(slots.borrow().iter().any(|&(_, priority)| priority == Priority::Visible));
    let s = state.borrow();
    assert_eq!(s.shown_scroll(), s.scroll);
}

#[test]
fn a_held_key_stops_at_a_grids_held_stops_and_down_stays_on_its_last_row() {
    let jump = Id::new("jump");
    let chips = Id::new("chips");
    let below = Id::new("below");
    let state = RefCell::new(GridState::default());
    let mut screen = |ui: &mut Ui| {
        ui.focusable(chips, Rect::new(96.0, 80.0, 120.0, 40.0));
        let grid = PosterGrid::new(TileKind::LibraryGrid, 122.0)
            .right_edge(Edge::To(jump))
            .held_stop(pley_ui::Dirs::UP.union(pley_ui::Dirs::RIGHT));
        grid.show(ui, Id::new("grid"), &mut state.borrow_mut(), 10, |ui, slot| {
            Tile::new(TileKind::LibraryGrid, "T").paint(ui, slot);
        });
        let mut l = 0;
        JumpControl { available: &[true; 27], current: 1 }.show(ui, jump, Point::new(872.0, 136.0), &mut l);
        ui.focusable(below, Rect::new(96.0, 500.0, 120.0, 30.0));
    };
    let cell = |i: u64| Some(Id::new("grid").with(i));
    let mut h = Harness::new();
    h.idle(&mut screen);
    // Down on the last row stays, though something is below.
    press(&mut h, Key::Down, &mut screen);
    assert_eq!(press(&mut h, Key::Down, &mut screen), cell(7));
    // A held Up goes up the grid and stops at its top; a press goes on to the chips.
    hold(&mut h, Key::Up, &mut screen);
    assert_eq!(settle(&mut h, &mut screen).focused, cell(0));
    h.step(Step::Up(Key::Up), &mut screen);
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(chips));
    // A held Right goes along the row and stops at its end; a press goes on to the jump control.
    press(&mut h, Key::Down, &mut screen);
    hold(&mut h, Key::Right, &mut screen);
    h.step(Step::Up(Key::Right), &mut screen);
    assert_eq!(settle(&mut h, &mut screen).focused, cell(6));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(jump));
}

#[test]
fn reduce_motion_steps_a_held_grid_a_row_at_a_time() {
    let state = RefCell::new(GridState::default());
    let shown = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let mut s = state.borrow_mut();
        PosterGrid::new(TileKind::LibraryGrid, 122.0).show(ui, Id::new("grid"), &mut s, 10_000, |ui, slot| {
            Tile::new(TileKind::LibraryGrid, "T").paint(ui, slot);
        });
        shown.borrow_mut().push(s.shown_scroll());
    };
    let mut h = Harness::new();
    h.state.set_reduced_motion(true);
    h.idle(&mut screen);
    hold(&mut h, Key::Down, &mut screen);
    let v = shown.borrow();
    assert!(v.iter().all(|x| x % 212.0 == 0.0), "{v:?}");
    assert!(*v.last().unwrap() > 15.0 * 212.0);
}

#[test]
fn a_list_keeps_its_focused_row_at_30_percent() {
    let state = RefCell::new(ListState::default());
    let view = Rect::new(624.0, 27.0, 288.0, 486.0);
    let tops = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let mut tops = tops.borrow_mut();
        tops.clear();
        List::new(view).show(
            ui,
            Id::new("list"),
            &mut state.borrow_mut(),
            40,
            |_, _| 44.0,
            |ui, slot| {
                let r =
                    ListRow::new("Choice").checked(slot.index == 0).show(ui, slot.id, slot.rect.origin(), slot.rect.w);
                if r.focused {
                    tops.push((slot.index, slot.rect.y));
                }
            },
        );
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    for _ in 0..10 {
        h.step(Step::Press(Key::Down), &mut screen);
    }
    settle(&mut h, &mut screen);
    // The tenth row's top at 30 % of 486 down the view.
    assert_eq!(tops.borrow()[0].0, 10);
    assert!(close(tops.borrow()[0].1, 27.0 + 486.0 * theme::SCROLL_ANCHOR));
    // At the end it stops where the last row ends the view.
    for _ in 0..40 {
        h.step(Step::Press(Key::Down), &mut screen);
    }
    settle(&mut h, &mut screen);
    let (i, y) = tops.borrow()[0];
    assert_eq!(i, 39);
    assert!(close(y + 44.0, 27.0 + 486.0));
}

#[test]
fn a_drives_line_has_its_icon_tile_its_bar_and_says_its_words() {
    use crate::track::{ICON_LINE_MIN, LINE_MIN, TrackLine};
    let film = TrackLine {
        title: "Mad Max",
        subtitle: Some("MKV · Resumes at 1:02:03"),
        duration: "4.2 GB",
        icon: Some(pley_base::Icon::FileVideo),
        progress: 0.5,
        ..TrackLine::default()
    };
    let folder = TrackLine { title: "Films", icon: Some(pley_base::Icon::Folder), ..TrackLine::default() };
    let track = TrackLine { number: "3", title: "Song", ..TrackLine::default() };
    assert_eq!((track.height(), folder.height()), (LINE_MIN, ICON_LINE_MIN));
    // Its words, the bar and the padding: taller than an icon's least.
    assert_eq!(film.height(), 20.0 + 17.0 + 6.0 + 3.0 + 12.0);
    assert_eq!(film.spoken(), "Mad Max, MKV · Resumes at 1:02:03, 4.2 GB");
    let rect = Rect::new(96.0, 100.0, 720.0, film.height());
    let mut screen = |ui: &mut Ui| {
        film.show(ui, Id::new("film"), rect);
    };
    let mut h = Harness::new();
    let frame = h.idle(&mut screen);
    let tiles = super::fills_of(&frame.list, theme::SURFACE2);
    assert_eq!(tiles, [Rect::new(108.0, 100.0 + (rect.h - 36.0) / 2.0, 36.0, 36.0)]);
    let bars = super::fills_of(&frame.list, theme::PROGRESS_TRACK_ON_ART);
    assert_eq!(bars.len(), 1);
    assert_eq!((bars[0].x, bars[0].w, bars[0].h), (108.0 + 36.0 + 14.0, 160.0, 3.0));
    assert_eq!(super::fills_of(&frame.list, theme::ACCENT)[0].w, 80.0);
}
