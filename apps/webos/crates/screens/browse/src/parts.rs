//! What this crate's pages share: the page title, Android's library message (`TvMessage` in
//! `TvLibraryParts.kt`), skeletons, the details fetched where focus rests, a side panel's list of
//! rows and headings, and typing a title with the on-screen keyboard.

use pley_base::{Point, Rect, TextOpts};
use pley_core::model::ItemSummary;
use pley_core::{ApiError, Promise};
use pley_kit::{Button, Heading, Keyboard, KeyboardState, List, ListRow, ListState, TextEdit, TextField};
use pley_shell::AppCx;
use pley_ui::theme;
use pley_ui::{Edge, Group, Id, Response, Ui};

use crate::tiles::is_music;

/// Where a page's title sits (`Tv.PageTitle` at x 96, y 27).
pub const TITLE_AT: Point = Point::new(theme::GUTTER, theme::SAFE_Y);

/// A page's title, one line cut with "…" at `width`.
pub fn page_title(ui: &mut Ui, text: &str, width: f32) {
    ui.text(TITLE_AT, text, &theme::PAGE_TITLE, &TextOpts::line(width), theme::FG);
}

/// A failed load's title: `title`, or "Can't reach your server" when the server answered nothing
/// (`TvOffline.kt:97-108`).
pub fn failed_title(error: &ApiError, title: &'static str) -> &'static str {
    if error.unreachable() { pley_i18n::t!("library.messages.offline") } else { title }
}

/// A page area's empty or failed state as the Android app's library pages draw it (`TvMessage` in
/// `TvLibraryParts.kt`: a column spaced 8 apart): from (96, `top`), the title in `Tv.Section`, 8,
/// the text in `Tv.Body` Muted (at most 520 wide), then 8 + 16 + 8 and a pill (no icon) that takes
/// focus. Returns the pill's answer.
pub fn column_message(
    ui: &mut Ui,
    id: Id,
    top: f32,
    title: &str,
    body: &str,
    action: Option<&str>,
) -> Option<Response> {
    let width = theme::CANVAS_W - 2.0 * theme::GUTTER;
    let x = theme::GUTTER;
    let mut y = top;
    y += ui.text(Point::new(x, y), title, &theme::SECTION, &TextOpts::wrap(width, 2), theme::FG).h;
    if !body.is_empty() {
        y += 8.0;
        y += ui.text(Point::new(x, y), body, &theme::BODY, &TextOpts::wrap(520.0, 0), theme::MUTED).h;
    }
    let label = action?;
    y += 8.0 + 16.0 + 8.0;
    Some(ui.group(id, Group::row().edge(pley_ui::Dir::Left, Edge::Out), |ui| {
        let bid = ui.id("action");
        ui.initial_focus(bid);
        Button::pill(label).show(ui, bid, Point::new(x, y))
    }))
}

/// A request's answer once it came, or a failure once its task stopped without one (so nothing
/// waits on it for good).
pub fn answer<T>(p: &mut Promise<Result<T, ApiError>>) -> Option<Result<T, ApiError>> {
    match p.take() {
        Some(a) => Some(a),
        None if p.failed() => Some(Err(ApiError::Unreadable("the request stopped".into()))),
        None => None,
    }
}

/// Still skeleton blocks at x 96: `(top, width, height, corner)` each.
pub fn skeleton_blocks(ui: &mut Ui, blocks: &[(f32, f32, f32, f32)]) {
    for &(top, w, h, r) in blocks {
        pley_kit::skeleton_block(ui, Rect::new(theme::GUTTER, top, w, h), r);
    }
}

/// What a page does where focus rests on a title (400 ms, no key held; `GridRest` in the TV app's
/// `library.tsx`): the title's details are fetched for its page, and the server reads ahead what
/// Play reads first, at most once a minute for the same title (`prefetchPlayback`).
#[derive(Default)]
pub struct Rest {
    seen: Option<i64>,
    details: Option<Promise<Result<pley_core::model::ItemDetail, ApiError>>>,
    prefetch: Option<Promise<Result<(), ApiError>>>,
    prefetched: Option<(i64, f64)>,
}

impl Rest {
    /// Focus is on `item` (or on nothing that is a title); `rested` says whether it has rested.
    pub fn update(&mut self, cx: &mut AppCx, item: Option<&ItemSummary>, rested: bool) {
        if let Some(p) = &mut self.details
            && let Some(answer) = p.take()
        {
            self.details = None;
            if let Ok(detail) = answer {
                cx.state.keep_item(detail);
            }
        }
        if self.prefetch.as_mut().is_some_and(|p| p.take().is_some() || p.failed()) {
            self.prefetch = None;
        }
        let Some(item) = item.filter(|_| rested) else { return };
        if self.seen == Some(item.id) {
            return;
        }
        self.seen = Some(item.id);
        let video = item.kind.playable_video();
        let recent = self.prefetched.is_some_and(|(id, at)| id == item.id && cx.now - at < 60.0);
        if video && self.prefetch.is_none() && !recent {
            let id = item.id;
            self.prefetched = Some((id, cx.now));
            self.prefetch = Some(cx.spawn(move |api| api.prefetch(id)));
        }
        // Music has pages of its own, which ask for their own.
        if !is_music(item) && cx.state.cached_item(item.id).is_none() {
            let id = item.id;
            self.details = Some(cx.spawn(move |api| api.item(id)));
        }
    }
}

/// A line of a side panel.
#[derive(Clone, Copy, Debug)]
pub enum Line<'a> {
    /// The panel's title, with what it is for under it (`TvPanelTitle`: 16 in, 8 above, 12 below).
    Title(&'a str, &'a str),
    /// A heading between rows.
    Heading(Heading, &'a str),
    /// A row that can be chosen.
    Row(ListRow<'a>),
}

impl Line<'_> {
    fn height(&self, ui: &Ui, width: f32) -> f32 {
        match self {
            Line::Title(t, s) => {
                let mut h = 8.0 + ui.measure(t, &theme::TITLE, &TextOpts::wrap(width - 16.0, 2)).h + 12.0;
                if !s.is_empty() {
                    h += ui.measure(s, &theme::CAPTION, &TextOpts::wrap(width - 16.0, 2)).h;
                }
                h
            }
            Line::Heading(kind, text) => kind.height(ui, text, width),
            Line::Row(row) => row.height(ui, width),
        }
    }

    fn draw(&self, ui: &mut Ui, id: Id, rect: Rect) -> Option<Response> {
        match self {
            Line::Title(t, s) => {
                let w = rect.w - 16.0;
                let mut y = rect.y + 8.0;
                y += ui.text(Point::new(rect.x + 16.0, y), t, &theme::TITLE, &TextOpts::wrap(w, 2), theme::FG).h;
                if !s.is_empty() {
                    ui.text(Point::new(rect.x + 16.0, y), s, &theme::CAPTION, &TextOpts::wrap(w, 2), theme::MUTED);
                }
                None
            }
            Line::Heading(kind, text) => {
                kind.show(ui, text, rect.origin(), rect.w);
                None
            }
            Line::Row(row) => Some(row.show(ui, id, rect.origin(), rect.w)),
        }
    }
}

/// A side panel's lines in its content `area`, scrolling with focus; the row at `initial` takes
/// focus when the panel opens. Returns the index of the line chosen.
pub fn panel_lines(
    ui: &mut Ui,
    id: Id,
    area: Rect,
    state: &mut ListState,
    lines: &[Line],
    initial: Option<usize>,
) -> Option<usize> {
    let mut chosen = None;
    let width = area.w;
    List::new(area).show(
        ui,
        id,
        state,
        lines.len(),
        |ui, i| lines[i].height(ui, width),
        |ui, slot| {
            if initial == Some(slot.index) {
                ui.initial_focus(slot.id);
            }
            if lines[slot.index].draw(ui, slot.id, slot.rect).is_some_and(|r| r.activated) {
                chosen = Some(slot.index);
            }
        },
    );
    chosen
}

/// A side panel with `title` fixed at its top (`PanelList` in `TvLibraryParts.kt`: `Tv.Title`, 16
/// in, 8 above, 12 below) and `lines` scrolling under it, as [`panel_lines`].
pub fn titled_panel_lines(
    ui: &mut Ui,
    id: Id,
    area: Rect,
    state: &mut ListState,
    title: &str,
    lines: &[Line],
    initial: Option<usize>,
) -> Option<usize> {
    let opts = TextOpts::wrap(area.w - 16.0, 2);
    let h = 8.0 + ui.text(Point::new(area.x + 16.0, area.y + 8.0), title, &theme::TITLE, &opts, theme::FG).h + 12.0;
    let list = Rect::new(area.x, area.y + h, area.w, (area.h - h).max(0.0));
    panel_lines(ui, id, list, state, lines, initial)
}

/// A title being typed: the field in a panel and, while typing, the on-screen keyboard.
#[derive(Clone, Debug, Default)]
pub struct Typing {
    pub edit: TextEdit,
    pub typing: bool,
    keyboard: KeyboardState,
}

/// A playlist's title is 1 to 200 characters (`Playlists.check` in the Android app).
pub const TITLE_MAX: usize = 200;

impl Typing {
    /// Typing starting from `text`, at most [`TITLE_MAX`] characters.
    pub fn new(text: &str) -> Typing {
        Typing { edit: TextEdit::new(text).limit(TITLE_MAX), typing: false, keyboard: KeyboardState::default() }
    }

    /// The title typed, trimmed, when it will do (1 to 200 characters).
    pub fn title(&self) -> Option<String> {
        let t = self.edit.text.trim();
        (!t.is_empty() && t.chars().count() <= TITLE_MAX).then(|| t.to_string())
    }

    /// The field, `width` wide at `at`, saying `placeholder` while empty. OK on it starts typing.
    pub fn field(&mut self, ui: &mut Ui, id: Id, at: Point, width: f32, placeholder: &str) -> Response {
        let r = TextField::new(&self.edit.text, width).placeholder(placeholder).typing(self.typing).show(ui, id, at);
        if r.activated {
            self.typing = true;
            self.keyboard = KeyboardState::default();
        }
        r
    }

    /// The keyboard while typing, over the bottom of the screen (draw it after the panel, so it
    /// keeps focus); whether Done was chosen. Back and Up past its top row stop typing.
    pub fn keyboard(&mut self, ui: &mut Ui, id: Id, action: &str) -> bool {
        if !self.typing {
            return false;
        }
        let out = Keyboard::new().action_label(action).show(ui, id, &mut self.keyboard, &mut self.edit);
        if out.done || out.closed {
            self.typing = false;
        }
        out.done
    }

    /// Back while typing stops typing; whether it did.
    pub fn back(&mut self) -> bool {
        std::mem::take(&mut self.typing)
    }
}
