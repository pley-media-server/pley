//! Pley for LG TV's page of the USB drive plugged into the TV (the Samsung app's
//! `Screens/StorageScreen.cs`): a folder of the drive, its folders, then its videos, a line each.
//! OK on a folder opens it over this page (Back comes back up), on a video plays it in Pley's
//! player from where it stopped, with no server behind it. A video started before says where it
//! resumes, with a bar of how far through it is. Hidden files and what Windows and macOS leave on
//! a drive aren't shown; nor are files the TV doesn't play.
//!
//! The folder is read on the task pool, never on the main thread (a drive that sleeps takes a
//! moment to answer), and again when the drive is plugged in, pulled out or swapped, and when the
//! page is on top again (Back from the player: a video's line says where it resumes now). Each
//! line's words are made once per reading; the lines move with the kit's list, as other lists of
//! the app do.

mod listing;

use std::path::PathBuf;

use pley_base::{Point, Rect, TextOpts};
use pley_core::Promise;
use pley_local::server::ITEM_ID;
use pley_shell::{AppCx, AppEvent, Chrome, PlayRequest, Route, Screen};
use pley_ui::theme;
use pley_ui::{Edge, Id, Ui};

pub use listing::{Line, Listing, Pick, Problem, TITLE, UP, list};

/// Where the caption and the list are (`StorageScreen.cs`): the caption 8 under the title, the
/// list 16 under the caption, 720 wide.
const CAPTION_Y: f32 = theme::SAFE_Y + 34.0 + 8.0;
const LIST_TOP: f32 = CAPTION_Y + 18.0 + 16.0;
const LIST_W: f32 = 720.0;
const LINE_GAP: f32 = 2.0;
/// The words' width: the page's, from the gutter to the right safe edge.
const WIDTH: f32 = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
/// A message's place under the list's top: under the ".." line when there is one.
const MESSAGE_BELOW_UP: f32 = pley_kit::track::ICON_LINE_MIN + 24.0;
const MESSAGE_ALONE: f32 = 40.0;

/// The screen of `route` when it is one of this crate's pages: a folder of the drive.
pub fn screen(route: &Route, _cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        Route::Storage { path } => Some(Box::new(StorageScreen::new(path.clone()))),
        _ => None,
    }
}

/// A folder of the drive.
pub struct StorageScreen {
    /// The folder shown; none: the drive's top.
    path: Option<PathBuf>,
    /// The reading under way; a newer one replaces it, and the older answer is dropped with it.
    reading: Option<Promise<Listing>>,
    listing: Option<Listing>,
    /// Each line's height, worked out once per reading.
    heights: Vec<f32>,
    list: pley_kit::ListState,
    /// Focus has been sent to the rail for a page with no lines.
    rail_sent: bool,
}

impl StorageScreen {
    /// The page of the folder `path` (none: the drive's top); it reads the folder when first drawn.
    pub fn new(path: Option<PathBuf>) -> StorageScreen {
        StorageScreen {
            path,
            reading: None,
            listing: None,
            heights: Vec::new(),
            list: pley_kit::ListState::default(),
            rail_sent: false,
        }
    }

    /// The id of line `index`, under the page's group `page`.
    pub fn line_id(page: Id, index: usize) -> Id {
        page.with("lines").with(index as u64)
    }

    /// What the page shows now; none before the first reading is in.
    pub fn listing(&self) -> Option<&Listing> {
        self.listing.as_ref()
    }

    fn has_lines(&self) -> bool {
        self.listing.as_ref().is_some_and(|l| !l.lines.is_empty())
    }

    /// Reads the folder again on the task pool.
    fn read(&mut self, cx: &mut AppCx) {
        let device = cx.device().clone();
        let store = cx.store().clone();
        let path = self.path.clone();
        self.reading = Some(cx.tasks().spawn(move || list(device.as_ref(), &store, path.as_deref())));
    }

    /// Takes the reading once it is in: the lines change in place, focus staying on the line it
    /// was on (the last when that one went), and a page left with no lines gives focus to the rail.
    fn take_reading(&mut self, ui: &mut Ui, cx: &mut AppCx, list_id: Id) {
        let Some(reading) = self.reading.as_mut() else { return };
        let Some(listing) = reading.take() else {
            if reading.failed() {
                self.reading = None;
            }
            return;
        };
        self.reading = None;
        let had_focus = ui.focused_in(list_id).is_some();
        self.heights = listing.lines.iter().map(|l| l.track().height()).collect();
        if let Some(problem) = listing.problem {
            let (title, body) = problem.words();
            ui.announce(format!("{title}. {body}"));
        }
        let len = listing.lines.len();
        self.listing = Some(listing);
        if len == 0 {
            if had_focus || !self.rail_sent {
                self.rail_sent = true;
                cx.to_rail();
            }
        } else if had_focus && self.list.index >= len {
            self.list.index = len - 1;
            ui.request_focus(list_id.with(self.list.index as u64));
        }
        ui.request_repaint();
    }

    /// The lines; the one OK picked.
    fn lines(&mut self, ui: &mut Ui, list_id: Id) -> Option<Pick> {
        let listing = self.listing.as_ref()?;
        if listing.lines.is_empty() {
            return None;
        }
        ui.initial_focus(list_id.with(self.list.index.min(listing.lines.len() - 1) as u64));
        let view = Rect::new(theme::GUTTER, LIST_TOP, LIST_W, theme::CANVAS_H - LIST_TOP);
        let heights = &self.heights;
        let mut picked = None;
        pley_kit::List::new(view).gap(LINE_GAP).edges(Edge::Stop, Edge::Stop).show(
            ui,
            list_id,
            &mut self.list,
            listing.lines.len(),
            |_, i| heights[i],
            |ui, slot| {
                if listing.lines[slot.index].track().show(ui, slot.id, slot.rect).activated {
                    picked = Some(listing.lines[slot.index].pick.clone());
                }
            },
        );
        picked
    }

    /// Why the folder shows no lines of its own: under the ".." line when there is one.
    fn message(&self, ui: &mut Ui) {
        let Some(listing) = &self.listing else { return };
        let Some(problem) = listing.problem else { return };
        let (title, body) = problem.words();
        let y = LIST_TOP + if listing.has_up() { MESSAGE_BELOW_UP } else { MESSAGE_ALONE };
        let h = ui.text(Point::new(theme::GUTTER, y), title, &theme::TITLE, &TextOpts::wrap(WIDTH, 2), theme::FG).h;
        let at = Point::new(theme::GUTTER, y + h + 10.0);
        ui.text(at, body, &theme::BODY, &TextOpts::wrap(WIDTH, 3), theme::MUTED);
    }
}

impl Screen for StorageScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let list_id = ui.scope_id().with("lines");
        if self.listing.is_none() && self.reading.is_none() {
            self.read(cx);
        }
        self.take_reading(ui, cx, list_id);
        ui.text(Point::new(theme::GUTTER, theme::SAFE_Y), TITLE, &theme::PAGE_TITLE, &TextOpts::line(WIDTH), theme::FG);
        if let Some(caption) = self.listing.as_ref().map(|l| l.caption.as_str()).filter(|c| !c.is_empty()) {
            let at = Point::new(theme::GUTTER, CAPTION_Y);
            ui.text(at, caption, &theme::CAPTION, &TextOpts::line(WIDTH), theme::MUTED);
        }
        let picked = self.lines(ui, list_id);
        self.message(ui);
        match picked {
            Some(Pick::Up) => cx.back(),
            Some(Pick::Folder(path)) => cx.push(Route::Storage { path: Some(path) }),
            Some(Pick::Video { path, start_ms }) => {
                cx.play(PlayRequest { file: Some(path), start_ms: Some(start_ms), ..PlayRequest::item(ITEM_ID) })
            }
            None => {}
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            // Back from the player (where each video stopped), the drive pulled out or swapped,
            // the TV showing the app again: what is there now.
            AppEvent::Shown | AppEvent::Visible(true) | AppEvent::Device(pley_core::DeviceEvent::Drive(_)) => {
                self.read(cx)
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    /// The list keeps focus while it has a line (".." at least, under the drive's top).
    fn takes_focus(&self) -> bool {
        self.has_lines()
    }

    fn title(&self) -> Option<String> {
        Some(TITLE.into())
    }
}
