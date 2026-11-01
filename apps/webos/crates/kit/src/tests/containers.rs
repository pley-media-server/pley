//! Rows, pages of rows, grids and lists: where focus keeps things, what is drawn, memory, edges.

use std::cell::{Cell, RefCell};

use pley_base::{Cmd, Key, Point, Rect};
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
fn a_held_key_moves_the_row_without_animating() {
    let cell = OneRow::new(40);
    let mut screen = one_row(&cell);
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.step(Step::Down(Key::Right), &mut screen);
    // LG repeats after 400 ms, then every 50 ms; the gate lets one move through each 100 ms.
    h.wait(0.4, &mut screen);
    for _ in 0..6 {
        h.step(Step::Repeat(Key::Right), &mut screen);
        h.wait(0.05, &mut screen);
    }
    let slot = focused_slot(&cell);
    assert!(slot.index >= 3);
    // Held: the row is where it is heading, the focused tile at the gutter at once.
    assert_eq!(slot.rect.x, 96.0);
    assert_eq!(cell.borrow().state.shown_offset(), cell.borrow().state.offset);
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
