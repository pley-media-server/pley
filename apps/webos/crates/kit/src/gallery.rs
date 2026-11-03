//! The gallery: every control of the kit on one page of sections, working, for the app's debug
//! route and for checking the kit on a TV and in screenshots. Nothing here talks to a server: the
//! pictures are addresses that never load, so their placeholders show.

use pley_base::{Icon, ImageRequest, Point, Rect};
use pley_ui::theme;
use pley_ui::{Dir, Edge, Group, Id, Ui};

use crate::backdrop::{Backdrop, BackdropArt, BackdropScrim, BackdropState};
use crate::badge::{Badge, BadgeStyle, MetaLine};
use crate::button::{BarItem, Button, RoundButton, button_bar};
use crate::column::{ColumnState, RowColumn, Section};
use crate::feedback::{
    LoadingRing, Message, MessageStyle, ProgressBar, ToastPlacement, ToastState, skeleton_block, toast,
};
use crate::grid::{GridState, JumpControl, PosterGrid};
use crate::keyboard::{Keyboard, KeyboardState, TextEdit, TextField};
use crate::list::{Heading, List, ListRow, ListState, Stepper, Trailing};
use crate::overlay::{Action, ActionSheet, Dialog, DialogButton, SidePanel};
use crate::pad::{Keypad, PadKey, PadState, PinDots};
use crate::qr::Qr;
use crate::row::{Row, RowState};
use crate::tile::{ProfileTile, Tile, TileKind};

/// What the gallery keeps between frames (owned by its screen).
#[derive(Debug, Default)]
pub struct GalleryState {
    page: ColumnState,
    rows: [RowState; 5],
    grid: GridState,
    panel: ListState,
    backdrop: BackdropState,
    toast: ToastState,
    panel_open: bool,
    dialog_open: bool,
    sheet: bool,
    typing: bool,
    keyboard: KeyboardState,
    edit: TextEdit,
    pin: PadState,
    address: PadState,
    pin_typed: usize,
    shake: Option<f64>,
    switches: [bool; 2],
    sync: i32,
    chips: [bool; 1],
    tab: usize,
    season: usize,
    letter: usize,
    qr: Option<Qr>,
}

const TITLES: [&str; 10] = [
    "Northern Lights",
    "The Long Night",
    "Glasshouse",
    "Paper Moons",
    "Harbour",
    "Saltwater",
    "Red Orchard",
    "Quiet Hours",
    "Ninefold",
    "Lantern",
];

/// The rows of tiles the gallery shows, one per kind.
const ROW_KINDS: [(TileKind, &str); 5] = [
    (TileKind::Poster, "Posters"),
    (TileKind::Landscape, "Landscape"),
    (TileKind::Episode, "Episodes"),
    (TileKind::Square, "Squares"),
    (TileKind::Person, "People"),
];

/// Sections of the page, top to bottom.
const BUTTONS: u64 = 0;
const CHIPS: u64 = 1;
const ROWS: u64 = 2;
const PADS: u64 = 10;
const LISTS: u64 = 11;
const FEEDBACK: u64 = 12;
const TYPING: u64 = 13;
const GRID: u64 = 14;

fn header(ui: &mut Ui, y: f32, text: &str) {
    ui.text(Point::new(theme::GUTTER, y), text, &theme::SECTION, &pley_base::TextOpts::line(816.0), theme::FG);
}

/// Draws the gallery: a page of sections holding every control, with a side panel, a dialog, an
/// action sheet, toasts and the on-screen keyboard opened from its buttons.
pub fn gallery(ui: &mut Ui, id: Id, state: &mut GalleryState) {
    let art = BackdropArt::new(1, vec![ImageRequest::new("gallery://backdrop", 1280)], "Gallery");
    let scrim = if state.page.shown_shift() > 0.0 { BackdropScrim::Rows } else { BackdropScrim::Hero };
    Backdrop::new(scrim).show(ui, id.with("backdrop"), &mut state.backdrop, Some(&art));
    if state.sheet {
        sheet(ui, id, state);
        toast(ui, &mut state.toast, ToastPlacement::TopRight);
        return;
    }
    let mut sections = vec![Section::hero(BUTTONS, 152.0), Section::row(CHIPS, 186.0)];
    for (i, (kind, _)) in ROW_KINDS.iter().enumerate() {
        sections.push(Section::row(ROWS + i as u64, kind.row_height(true)));
    }
    sections.extend([
        Section::row(PADS, 336.0),
        Section::row(LISTS, 330.0),
        Section::row(FEEDBACK, 330.0),
        Section::row(TYPING, 180.0),
        Section::row(GRID, 497.0),
    ]);
    let page = RowColumn::new(theme::SAFE_Y);
    let mut st = std::mem::take(&mut state.page);
    page.show(ui, id.with("page"), &mut st, &sections, |ui, i, y| match sections[i].key {
        BUTTONS => buttons(ui, y, state),
        CHIPS => chips(ui, y, state),
        k if (ROWS..ROWS + 5).contains(&k) => row(ui, y, (k - ROWS) as usize, state),
        PADS => pads(ui, y, state),
        LISTS => lists(ui, y, state),
        FEEDBACK => feedback(ui, y, state),
        TYPING => typing(ui, y, state),
        _ => grid(ui, y, state),
    });
    state.page = st;
    if state.typing {
        let r =
            Keyboard::new().action_label("Search").show(ui, id.with("keyboard"), &mut state.keyboard, &mut state.edit);
        if r.done || r.closed {
            state.typing = false;
        }
    }
    let panel = SidePanel::new().show(ui, id.with("panel"), state.panel_open, |ui, area| {
        let labels = ["Title", "Year", "Date added", "Rating", "Release date", "Duration", "Last played", "Bitrate"];
        let list = List::new(area);
        let checked = 2;
        let panel_state = &mut state.panel;
        list.show(
            ui,
            Id::new("gallery.sort"),
            panel_state,
            labels.len() + 1,
            |_, i| if i == 0 { 38.0 } else { 44.0 },
            |ui, slot| {
                if slot.index == 0 {
                    Heading::PanelTitle.show(ui, "Sort by", slot.rect.origin(), slot.rect.w);
                    return;
                }
                if slot.index - 1 == checked {
                    ui.initial_focus(slot.id);
                }
                ListRow::new(labels[slot.index - 1]).checked(slot.index - 1 == checked).show(
                    ui,
                    slot.id,
                    slot.rect.origin(),
                    slot.rect.w,
                );
            },
        );
    });
    if panel.closed {
        state.panel_open = false;
    }
    let dialog_buttons = [DialogButton::new("Cancel"), DialogButton::danger("Delete")];
    let d = Dialog::new("Delete this playlist?")
        .body("It goes from every device. The titles in it stay in your libraries.")
        .buttons(&dialog_buttons)
        .show(ui, id.with("dialog"), state.dialog_open);
    if d.dismissed || d.chosen.is_some() {
        state.dialog_open = false;
    }
    toast(ui, &mut state.toast, ToastPlacement::TopRight);
}

fn buttons(ui: &mut Ui, y: f32, state: &mut GalleryState) {
    header(ui, y, "Buttons");
    let items = [
        ("play", BarItem::Button(Button::pill("Resume").icon(Icon::Play).long_press())),
        ("start", BarItem::Round(RoundButton::new(Icon::RotateCcw, "From start"))),
        ("watched", BarItem::Round(RoundButton::new(Icon::CircleCheck, "Mark as watched"))),
        ("more", BarItem::Round(RoundButton::new(Icon::Ellipsis, "More").on_art(false))),
        ("text", BarItem::Button(Button::text("Change server"))),
        ("link", BarItem::Button(Button::link("An artist").color(theme::ACCENT_BRIGHT))),
        ("off", BarItem::Button(Button::pill("Unavailable").enabled(false))),
    ];
    let first = ui.id("first");
    button_bar(ui, first, Point::new(theme::GUTTER, y + 38.0), &items);
    let items = [
        ("panel", BarItem::Button(Button::pill("Side panel").icon(Icon::SlidersHorizontal))),
        ("dialog", BarItem::Button(Button::pill("Dialog"))),
        ("sheet", BarItem::Button(Button::pill("Actions").icon(Icon::Ellipsis))),
        ("toast", BarItem::Button(Button::text("Toast"))),
        ("error", BarItem::Button(Button::text("Error toast").color(theme::DANGER_TEXT))),
    ];
    let second = ui.id("second");
    let r = button_bar(ui, second, Point::new(theme::GUTTER, y + 112.0), &items);
    let now = ui.time();
    if r[0].activated {
        state.panel_open = true;
        state.panel = ListState::at(3);
    }
    state.dialog_open |= r[1].activated;
    state.sheet |= r[2].activated;
    if r[3].activated {
        state.toast.show("Added to My List", false, now);
    }
    if r[4].activated {
        state.toast.show("The favourite wasn't saved: no answer in time.", true, now);
    }
}

fn chips(ui: &mut Ui, y: f32, state: &mut GalleryState) {
    header(ui, y, "Chips, tabs, badges");
    let chips_id = ui.id("chips");
    ui.group(chips_id, Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop), |ui| {
        let mut x = theme::GUTTER;
        let list = [
            Button::chip("Title").icon(Icon::ArrowUpNarrowWide),
            Button::chip("Filter").icon(Icon::SlidersHorizontal),
            Button::chip("Unwatched").selected(state.chips[0]),
            Button::chip("Science Fiction & Fantasy").max_label_width(120.0),
        ];
        for (i, chip) in list.iter().enumerate() {
            let cid = ui.id(i);
            if chip.show(ui, cid, Point::new(x, y + 38.0)).activated && i == 2 {
                state.chips[0] = !state.chips[0];
            }
            x += chip.size(ui).w + theme::BUTTON_GAP;
        }
        ui.text(Point::new(x, y + 49.0), "342 movies", &theme::CAPTION, &pley_base::TextOpts::default(), theme::MUTED);
    });
    let tabs_id = ui.id("tabs");
    ui.group(tabs_id, Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop), |ui| {
        let mut x = theme::GUTTER;
        for (i, label) in ["Recommended", "Library", "Collections"].iter().enumerate() {
            let b = Button::tab(label, state.tab == i);
            let tid = ui.id(i);
            if b.show(ui, tid, Point::new(x, y + 94.0)).activated {
                state.tab = i;
            }
            x += b.size(ui).w + 8.0;
        }
        x += 32.0;
        for (i, label) in ["Season 1", "Season 2", "Specials"].iter().enumerate() {
            let b = Button::season_tab(label, state.season == i);
            let sid = ui.id(("season", i));
            if b.show(ui, sid, Point::new(x, y + 94.0)).activated {
                state.season = i;
            }
            x += b.size(ui).w + 4.0;
        }
    });
    let mut x = theme::GUTTER;
    for b in [Badge::live(), Badge::on_now(), Badge::new_programme(), Badge::new("12", BadgeStyle::Number)] {
        x += b.show(ui, Point::new(x, y + 152.0)).w + 8.0;
    }
    let quality = ["4K".to_owned(), "DV P8.1".to_owned(), "Atmos".to_owned()];
    MetaLine::new()
        .lead("S2 · E5 · The Long Night")
        .text("2019")
        .content_rating("TV-MA")
        .text("58 min")
        .stars("7.4")
        .quality(&quality)
        .show(ui, Point::new(x + 16.0, y + 152.0), 912.0 - x - 16.0);
}

fn row(ui: &mut Ui, y: f32, which: usize, state: &mut GalleryState) {
    let (kind, title) = ROW_KINDS[which];
    let art: Vec<[ImageRequest; 1]> =
        (0..TITLES.len()).map(|i| [ImageRequest::new(format!("gallery://{which}/{i}"), 240)]).collect();
    let badges = ["4K".to_owned(), "HEVC".to_owned()];
    let len = TITLES.len() + 1;
    let rid = ui.id("row");
    Row::new(kind, y).header(title).show(
        ui,
        rid,
        &mut state.rows[which],
        len,
        |i| i as u64,
        |ui, slot| {
            if slot.index == TITLES.len() {
                Tile::see_all(kind, "See all").paint(ui, slot);
                return;
            }
            let i = slot.index;
            let tile =
                Tile::new(kind, TITLES[i]).subtitle(if i % 2 == 0 { "2019" } else { "26 min left" }).art(&art[i]);
            let tile = match i % 5 {
                1 => tile.progress(Some(0.4)),
                2 => tile.unwatched(6),
                3 => tile.watched(true),
                4 => tile.badges(&badges),
                _ => tile.count("12"),
            };
            tile.paint(ui, slot);
        },
    );
}

fn pads(ui: &mut Ui, y: f32, state: &mut GalleryState) {
    header(ui, y, "Profiles and pads");
    let top = y + 38.0;
    let profiles_id = ui.id("profiles");
    ui.group(profiles_id, Group::row(), |ui| {
        for (i, name) in ["Anna", "Sam"].iter().enumerate() {
            let pid = ui.id(i);
            ProfileTile::new(name).locked(i == 1).show(ui, pid, Point::new(theme::GUTTER + i as f32 * 140.0, top));
        }
    });
    PinDots { typed: state.pin_typed }.show(ui, 216.0, top + 160.0, state.shake);
    let pad_id = ui.id("pin");
    match Keypad::pin().show(ui, pad_id, Point::new(400.0, top), &mut state.pin) {
        Some(PadKey::Digit(_)) => state.pin_typed = (state.pin_typed + 1).min(8),
        Some(PadKey::Delete) => state.pin_typed = state.pin_typed.saturating_sub(1),
        Some(PadKey::Ok) => {
            state.pin_typed = 0;
            state.shake = Some(ui.time());
        }
        _ => {}
    }
    let address_id = ui.id("address");
    Keypad::address().show(ui, address_id, Point::new(660.0, top), &mut state.address);
}

fn lists(ui: &mut Ui, y: f32, state: &mut GalleryState) {
    header(ui, y, "Lists");
    let top = y + 38.0;
    let w = 360.0;
    let rows = [
        ListRow::new("Play").icon(Icon::Play),
        ListRow::new("English (SRT, Forced, External)").supporting("Made for this file · SDH").checked(true),
        ListRow::new("Genre").trailing(Trailing::Value("Science Fiction")),
        ListRow::new("Go to show").icon(Icon::Tv).trailing(Trailing::Chevron),
        ListRow::new("Delete playlist").icon(Icon::Trash2).danger(true),
        ListRow::new("Add to playlist").icon(Icon::ListPlus).enabled(false),
    ];
    let left_id = ui.id("left");
    ui.group(left_id, Group::column(), |ui| {
        let mut ry = top;
        for (i, r) in rows.iter().enumerate() {
            let rid = ui.id(i);
            let h = r.height(ui, w);
            r.show(ui, rid, Point::new(theme::GUTTER, ry), w);
            ry += h + 2.0;
        }
    });
    let x = theme::GUTTER + w + 40.0;
    let right_id = ui.id("right");
    ui.group(right_id, Group::column(), |ui| {
        let mut ry = top;
        let switches = [("Reduce motion", "Scales, slides and fades snap"), ("Play next episode automatically", "")];
        for (i, (label, value)) in switches.iter().enumerate() {
            let r = ListRow::setting(label)
                .icon(Icon::Monitor)
                .supporting(value)
                .trailing(Trailing::Switch(state.switches[i]));
            let sid = ui.id(("switch", i));
            let h = r.height(ui, w);
            if r.show(ui, sid, Point::new(x, ry), w).activated {
                state.switches[i] = !state.switches[i];
            }
            ry += h + 2.0;
        }
        let value = format!("{:+.1} s", state.sync as f32 / 10.0);
        let sid = ui.id("sync");
        let s = Stepper::new("Sync", &value).show(ui, sid, Point::new(x, ry), w);
        state.sync = if s.reset { 0 } else { (state.sync + s.step).clamp(-100, 100) };
        ry += 54.0;
        Heading::Settings.show(ui, "Advanced", Point::new(x, ry), w);
    });
}

fn feedback(ui: &mut Ui, y: f32, _state: &mut GalleryState) {
    header(ui, y, "Loading, progress, skeletons, messages");
    let top = y + 38.0;
    // Waiting since long ago, so both turn now.
    LoadingRing::buffering().show(ui, Point::new(theme::GUTTER + 20.0, top + 20.0), -10.0);
    LoadingRing::tuning().show(ui, Point::new(theme::GUTTER + 80.0, top + 20.0), -10.0);
    ProgressBar::page().show(ui, Rect::new(260.0, top + 10.0, 200.0, 4.0), 0.35);
    ProgressBar::on_art().show(ui, Rect::new(260.0, top + 26.0, 200.0, 4.0), 0.7);
    skeleton_block(ui, Rect::new(500.0, top, 260.0, 38.0), 2.0);
    skeleton_block(ui, Rect::new(500.0, top + 48.0, 210.0, 18.0), 2.0);
    let sk = TileKind::Square.size();
    skeleton_block(ui, Rect::new(780.0, top, sk.w * 0.6, sk.h * 0.6), TileKind::Square.radius());
    let mid = ui.id("message");
    Message::new(MessageStyle::Column { top: top + 100.0 }, "Couldn't load recommendations")
        .body("No answer in time. Check that the server is on and that this TV is on its network.")
        .action("Retry", Icon::RefreshCw)
        .show(ui, mid);
}

fn typing(ui: &mut Ui, y: f32, state: &mut GalleryState) {
    header(ui, y, "Typing and QR codes");
    let top = y + 38.0;
    let fid = ui.id("field");
    let field = TextField::new(&state.edit.text, 520.0)
        .placeholder("Search movies, shows, people…")
        .icon(Icon::Search)
        .typing(state.typing)
        .show(ui, fid, Point::new(theme::GUTTER, top));
    if field.activated {
        state.typing = true;
    }
    let qr = state.qr.get_or_insert_with(|| Qr::new("http://192.168.1.20:7539/link").expect("a short address fits"));
    let tile = Rect::new(700.0, top, 120.0, 120.0);
    ui.fill(tile, theme::QR_LIGHT, theme::radius::DIALOG);
    qr.show(ui, tile, theme::radius::DIALOG, 1.0);
}

fn grid(ui: &mut Ui, y: f32, state: &mut GalleryState) {
    header(ui, y, "A grid");
    let jump = ui.id("jump");
    let grid = PosterGrid::new(TileKind::LibraryGrid, y + 24.0).right_edge(Edge::To(jump));
    let gid = ui.id("grid");
    let art: Vec<[ImageRequest; 1]> =
        (0..TITLES.len()).map(|i| [ImageRequest::new(format!("gallery://grid/{i}"), 192)]).collect();
    grid.show(ui, gid, &mut state.grid, 14, |ui, slot| {
        // The last cells' page hasn't come: skeletons that still take focus.
        if slot.index >= TITLES.len() {
            Tile::skeleton(TileKind::LibraryGrid).paint(ui, slot);
        } else {
            Tile::new(TileKind::LibraryGrid, TITLES[slot.index]).art(&art[slot.index]).paint(ui, slot);
        }
    });
    let available = std::array::from_fn(|i| i % 3 != 2);
    let mut letter = state.letter;
    JumpControl { available: &available, current: 3 }.show(ui, jump, Point::new(872.0, y + 38.0), &mut letter);
    state.letter = letter;
}

fn sheet(ui: &mut Ui, id: Id, state: &mut GalleryState) {
    let actions = [
        Action::new("Resume").icon(Icon::Play),
        Action::new("Play from start"),
        Action::new("Mark as watched"),
        Action::new("Remove from Continue Watching"),
        Action::new("Go to show"),
    ];
    let chosen =
        ActionSheet::new("Glasshouse", &actions).subtitle("S2 · E5 · The Long Night").show(ui, id.with("sheet"));
    if let Some(i) = chosen {
        state.sheet = false;
        state.toast.show(actions[i].label, false, ui.time());
    }
    if ui.take_key(pley_base::Key::Back) {
        state.sheet = false;
    }
}
