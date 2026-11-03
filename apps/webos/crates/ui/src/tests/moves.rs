//! Focus moves: order inside groups, geometry between them, edges, memory, traps, elements going
//! away, page memory, requests.

use std::cell::{Cell, RefCell};

use pley_base::{Key, Point, Rect};

use super::{press, row, t, tile};
use crate::testing::{Harness, Step};
use crate::{Dir, Dirs, Edge, FocusOpts, Group, Id, PageMemory, Ui};

fn button(ui: &mut Ui, name: &str, rect: Rect) {
    let id = ui.id(name);
    ui.focusable(id, rect);
}

#[test]
fn walks_a_row_and_stops_at_its_ends() {
    let edge = Cell::new(None);
    let failed = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        row(ui, "row", 210.0, 0..4, Group::row().edge(Dir::Right, Edge::Stop).edge(Dir::Left, Edge::Out));
        if let Some(d) = ui.edge_hit(Id::new("row")) {
            edge.set(Some(d));
        }
        if let Some(d) = ui.failed_move() {
            failed.set(Some(d));
        }
    };
    let mut h = Harness::new();
    assert_eq!(h.idle(&mut screen).focused, Some(t("row", 0)));
    for i in 1..4 {
        assert_eq!(press(&mut h, Key::Right, &mut screen), Some(t("row", i)));
    }
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(t("row", 3)));
    assert_eq!((edge.get(), failed.get()), (Some(Dir::Right), None));
    for i in (0..3).rev() {
        assert_eq!(press(&mut h, Key::Left, &mut screen), Some(t("row", i)));
    }
    // Left at the row's start leaves the page: the shell sends it to the rail.
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(t("row", 0)));
    assert_eq!(failed.get(), Some(Dir::Left));
}

#[test]
fn moves_through_a_column_and_not_out_of_its_sides() {
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("col");
        ui.group(id, Group::column(), |ui| {
            for i in 0..3usize {
                let b = ui.id(i);
                ui.focusable(b, Rect::new(96.0, 100.0 + 50.0 * i as f32, 200.0, 40.0));
            }
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let b = |i: usize| t("col", i);
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(b(1)));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(b(2)));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(b(2)));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(b(2)));
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(b(1)));
}

#[test]
fn moves_through_a_grid_by_rows_and_columns() {
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("grid");
        ui.group(id, Group::grid(3), |ui| {
            for i in 0..7usize {
                let c = ui.id(i);
                ui.focusable(c, Rect::new(96.0 + (i % 3) as f32 * 122.0, 80.0 + (i / 3) as f32 * 230.0, 108.0, 162.0));
            }
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let c = |i: usize| t("grid", i);
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(c(1)));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(c(2)));
    // Never wrapping to the next row.
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(c(2)));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(c(5)));
    // From above a shorter last row: its last cell.
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(c(6)));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(c(6)));
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(c(3)));
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(c(3)));
}

#[test]
fn enters_a_new_row_at_the_tile_nearest_along_it() {
    // Android's TvHomeTest: Down from the last tile of a long row lands on a short row's last
    // tile (its "See all").
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 0.0, 0..5, Group::row());
        row(ui, "b", 220.0, 0..3, Group::row());
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    for _ in 0..4 {
        press(&mut h, Key::Right, &mut screen);
    }
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(t("b", 2)));
}

#[test]
fn rows_come_back_to_their_last_tile() {
    let memory = Cell::new(true);
    let mut screen = |ui: &mut Ui| {
        let g = if memory.get() { Group::row() } else { Group::row().no_memory() };
        row(ui, "a", 0.0, 0..5, g);
        row(ui, "b", 220.0, 0..5, Group::row());
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    for _ in 0..3 {
        press(&mut h, Key::Right, &mut screen);
    }
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(t("b", 3)));
    press(&mut h, Key::Left, &mut screen);
    press(&mut h, Key::Left, &mut screen);
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(t("a", 3)));
    // A row without memory is entered by geometry every time.
    memory.set(false);
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(t("b", 1)));
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(t("a", 1)));
}

#[test]
fn an_edge_can_send_focus_elsewhere() {
    let jump = Id::new("jump");
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("grid");
        ui.group(id, Group::grid(2).edge(Dir::Right, Edge::To(jump)), |ui| {
            for i in 0..4usize {
                let c = ui.id(i);
                ui.focusable(c, Rect::new(96.0 + (i % 2) as f32 * 122.0, 80.0 + (i / 2) as f32 * 230.0, 108.0, 162.0));
            }
        });
        ui.focusable(jump, Rect::new(880.0, 80.0, 40.0, 40.0));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(t("grid", 2)));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(t("grid", 3)));
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(jump));
}

#[test]
fn free_groups_move_to_the_nearest_element_in_the_direction() {
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("free");
        ui.group(id, Group::free(), |ui| {
            button(ui, "a", Rect::new(100.0, 100.0, 100.0, 40.0));
            button(ui, "b", Rect::new(300.0, 100.0, 100.0, 40.0));
            button(ui, "c", Rect::new(100.0, 200.0, 100.0, 40.0));
            button(ui, "d", Rect::new(320.0, 210.0, 100.0, 40.0));
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    let b = |n: &str| Id::new("free").with(n);
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(b("b")));
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(b("a")));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(b("c")));
    // In the beam beats nearer out of it.
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(b("d")));
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(b("b")));
}

#[test]
fn a_trap_keeps_focus_until_it_closes_then_gives_it_back() {
    let open = Cell::new(false);
    let failed = Cell::new(None);
    let mut screen = |ui: &mut Ui| {
        row(ui, "row", 210.0, 0..5, Group::row());
        if open.get() {
            let panel = ui.id("panel");
            ui.group(panel, Group::column().trap().enter_at(panel.with(1usize)), |ui| {
                for i in 0..3usize {
                    let r = ui.id(i);
                    ui.focusable(r, Rect::new(640.0, 100.0 + 44.0 * i as f32, 300.0, 44.0));
                }
            });
        }
        if let Some(d) = ui.failed_move() {
            failed.set(Some(d));
        }
    };
    let p = |i: usize| t("panel", i);
    let mut h = Harness::new();
    h.idle(&mut screen);
    press(&mut h, Key::Right, &mut screen);
    assert_eq!(press(&mut h, Key::Right, &mut screen), Some(t("row", 2)));
    // A panel opens on its current choice.
    open.set(true);
    assert_eq!(h.idle(&mut screen).focused, Some(p(1)));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(p(2)));
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(p(2)));
    assert_eq!(failed.get(), Some(Dir::Left));
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(p(1)));
    open.set(false);
    assert_eq!(h.idle(&mut screen).focused, Some(t("row", 2)));
}

#[test]
fn focus_moves_on_when_its_element_goes() {
    let keys = RefCell::new(vec![0usize, 1, 2, 3, 4]);
    let show_a = Cell::new(true);
    let show_b = Cell::new(true);
    let mut screen = |ui: &mut Ui| {
        let play = ui.id("play");
        ui.focusable(play, Rect::new(96.0, 40.0, 120.0, 40.0));
        ui.initial_focus(play);
        let page = ui.id("page");
        ui.group(page, Group::column(), |ui| {
            if show_a.get() {
                row(ui, "a", 210.0, keys.borrow().clone(), Group::row());
            }
            if show_b.get() {
                row(ui, "b", 430.0, 0..5, Group::row());
            }
        });
    };
    let a = |k: usize| Id::new("page").with("a").with(k);
    let b = |k: usize| Id::new("page").with("b").with(k);
    let mut h = Harness::new();
    assert_eq!(h.idle(&mut screen).focused, Some(Id::new("play")));
    press(&mut h, Key::Down, &mut screen);
    for _ in 0..3 {
        press(&mut h, Key::Right, &mut screen);
    }
    assert_eq!(h.state.focused(), Some(a(3)));
    // The tile now in its place.
    keys.borrow_mut().retain(|&k| k != 3);
    assert_eq!(h.idle(&mut screen).focused, Some(a(4)));
    // Nothing in its place: the last one left.
    keys.borrow_mut().truncate(3);
    assert_eq!(h.idle(&mut screen).focused, Some(a(2)));
    // The row goes: the row now nearest, at the tile under where focus was.
    show_a.set(false);
    assert_eq!(h.idle(&mut screen).focused, Some(b(2)));
    // The page's groups go: the page's initial element.
    show_b.set(false);
    assert_eq!(h.idle(&mut screen).focused, Some(Id::new("play")));
}

#[test]
fn a_page_gets_its_focus_back_unless_a_key_came_first() {
    let page = Cell::new('A');
    let loaded = Cell::new(true);
    let mut screen = |ui: &mut Ui| match page.get() {
        'A' => {
            let retry = ui.id("retry");
            ui.focusable(retry, Rect::new(96.0, 40.0, 120.0, 40.0));
            if loaded.get() {
                row(ui, "a", 210.0, 0..6, Group::row());
            }
        }
        _ => {
            row(ui, "b", 210.0, 0..6, Group::row());
            ui.initial_focus(Id::new("b").with(2usize));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    press(&mut h, Key::Down, &mut screen);
    for _ in 0..3 {
        press(&mut h, Key::Right, &mut screen);
    }
    assert_eq!(h.state.focused(), Some(t("a", 3)));
    let mem = h.state.export_page();
    assert_eq!(mem.focused, Some(t("a", 3)));

    // A new page starts at its initial element.
    h.state.import_page(PageMemory::default());
    page.set('B');
    assert_eq!(h.idle(&mut screen).focused, Some(t("b", 2)));

    // Back: the remembered tile again.
    h.state.import_page(mem.clone());
    page.set('A');
    assert_eq!(h.idle(&mut screen).focused, Some(t("a", 3)));
    // Its row remembers it too.
    assert!(h.state.export_page().groups.contains(&(Id::new("a"), t("a", 3))));

    // Back to a page still loading: the tile takes focus once it is drawn.
    h.state.import_page(mem.clone());
    loaded.set(false);
    assert_eq!(h.idle(&mut screen).focused, Some(Id::new("retry")));
    loaded.set(true);
    assert_eq!(h.idle(&mut screen).focused, Some(t("a", 3)));

    // ...unless a key was pressed meanwhile.
    h.state.import_page(mem);
    loaded.set(false);
    h.idle(&mut screen);
    press(&mut h, Key::Up, &mut screen);
    loaded.set(true);
    assert_eq!(h.idle(&mut screen).focused, Some(Id::new("retry")));
}

#[test]
fn focus_can_be_asked_for_by_element_or_group() {
    let show_late = Cell::new(false);
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 0.0, 0..5, Group::row());
        row(ui, "b", 220.0, 0..5, Group::row());
        if show_late.get() {
            let late = ui.id("late");
            ui.focusable(late, Rect::new(96.0, 440.0, 100.0, 40.0));
        }
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.state.request_focus(t("a", 4));
    assert_eq!(h.idle(&mut screen).focused, Some(t("a", 4)));
    // A group is entered by its rules: here its first tile, never visited.
    h.state.request_focus(Id::new("b"));
    assert_eq!(h.idle(&mut screen).focused, Some(t("b", 0)));
    // An element drawn a frame later still gets it.
    h.state.request_focus(Id::new("late"));
    h.idle(&mut screen);
    show_late.set(true);
    assert_eq!(h.idle(&mut screen).focused, Some(Id::new("late")));
    // One never drawn is dropped.
    h.state.request_focus(Id::new("never"));
    h.run(&[Step::Wait(1.0)], &mut screen);
    assert_eq!(h.idle(&mut screen).focused, Some(Id::new("late")));
}

#[test]
fn an_element_can_take_directions_itself() {
    let keys = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        row(ui, "a", 210.0, 0..3, Group::row().no_memory());
        let jump = ui.id("jump");
        ui.focusable_with(jump, Rect::new(880.0, 210.0, 40.0, 40.0), FocusOpts::default().takes(Dirs::VERTICAL));
        keys.borrow_mut().extend(ui.keys().iter().filter(|k| k.pressed).map(|k| k.key));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.state.request_focus(Id::new("jump"));
    h.idle(&mut screen);
    assert_eq!(press(&mut h, Key::Up, &mut screen), Some(Id::new("jump")));
    assert_eq!(press(&mut h, Key::Down, &mut screen), Some(Id::new("jump")));
    assert_eq!(*keys.borrow(), [Key::Up, Key::Down]);
    assert_eq!(press(&mut h, Key::Left, &mut screen), Some(t("a", 2)));
}

#[test]
fn containers_learn_where_focus_is() {
    let seen = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let page = ui.id("page");
        ui.group(page, Group::column(), |ui| {
            // The rows scroll: what they register is in their own coordinates.
            ui.translate(Point::new(-122.0, 0.0), |ui| {
                row(ui, "a", 0.0, 0..5, Group::row());
                row(ui, "b", 220.0, 0..5, Group::row());
            });
        });
        let b = page.with("b");
        seen.borrow_mut().push((ui.focused_index(page), ui.focused_index(b), ui.focused_in(b), ui.focused_rect(b)));
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    press(&mut h, Key::Down, &mut screen);
    press(&mut h, Key::Right, &mut screen);
    let b1 = Id::new("page").with("b").with(1usize);
    assert_eq!(seen.borrow().last().copied(), Some((Some(1), Some(1), Some(b1), Some(tile(1, 220.0)))));
    press(&mut h, Key::Up, &mut screen);
    assert_eq!(seen.borrow().last().copied(), Some((Some(0), None, None, None)));
}

#[test]
fn gained_says_when_focus_arrives() {
    let gained = RefCell::new(Vec::new());
    let mut screen = |ui: &mut Ui| {
        let id = ui.id("row");
        ui.group(id, Group::row(), |ui| {
            for i in 0..3usize {
                let t = ui.id(i);
                if ui.focusable(t, tile(i, 210.0)).gained {
                    gained.borrow_mut().push(i);
                }
            }
        });
    };
    let mut h = Harness::new();
    h.idle(&mut screen);
    h.idle(&mut screen);
    press(&mut h, Key::Right, &mut screen);
    h.idle(&mut screen);
    assert_eq!(*gained.borrow(), [0, 1]);
}
