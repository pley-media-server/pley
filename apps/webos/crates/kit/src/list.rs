//! Lists: the rows of side panels, settings and pages of choices, their headings, the step rows of
//! the player's panels, and a scrolling column that keeps the focused row at 30 % of its height.
//!
//! A list row (`PanelRow` in `TvItemParts.kt`, `TvLibraryControls.kt`, `TvPlaylistParts.kt`;
//! `TvSettingRow` in `TvSettingsKit.kt`) is at least 44 dp tall with 6 dp corners; focused it takes
//! an Accent fill with OnAccent content and never scales (the panel would clip it).

use pley_base::{Color, Icon, ImageRequest, Key, Point, Priority, Rect, Size, TextOpts, TextStyle};
use pley_ui::theme::{self, radius};
use pley_ui::{Dirs, Edge, FocusOpts, Group, Id, ImageOpts, Motion, Response, Ui, nudge};

use crate::look::{SCROLL, focus_progress, free_line, mix, press_progress, register, text_mid, unregistered};
use crate::row::find_key;
use crate::tile::{first_art, placeholder};

/// What a list row ends with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trailing<'a> {
    None,
    /// A 20 dp Accent check on the choice in force (nothing when `false`).
    Check(bool),
    /// A value in `Tv.Caption` Muted (a filter's current genre), then a chevron: a row that opens
    /// another list.
    Value(&'a str),
    /// A 20 dp chevron in Subtle: a row that opens something.
    Chevron,
    /// A switch, on or off (Settings).
    Switch(bool),
}

/// A row of a panel, a settings pane or a page of choices.
#[derive(Clone, Copy, Debug)]
pub struct ListRow<'a> {
    label: &'a str,
    icon: Option<Icon>,
    supporting: &'a str,
    supporting_lines: u16,
    trailing: Trailing<'a>,
    danger: bool,
    enabled: bool,
    padding: f32,
    art: Option<(&'a [ImageRequest], &'a str)>,
}

/// A list row's least height.
pub const ROW_MIN_HEIGHT: f32 = theme::LIST_ROW_HEIGHT;
/// Padding above and below a row's text.
const PAD_Y: f32 = 8.0;
/// Between the icon (or the art) and the text.
const LEAD_GAP: f32 = 12.0;
/// A leading picture's side (a playlist's cover).
const ART_SIZE: f32 = 40.0;
/// The switch's track.
const SWITCH: Size = Size::new(40.0, 22.0);

impl<'a> ListRow<'a> {
    /// A row saying `label` (`Tv.Body`, up to two lines), 16 dp in from its sides.
    pub fn new(label: &'a str) -> ListRow<'a> {
        ListRow {
            label,
            icon: None,
            supporting: "",
            supporting_lines: 2,
            trailing: Trailing::None,
            danger: false,
            enabled: true,
            padding: 16.0,
            art: None,
        }
    }

    /// A settings row: 20 dp in from its sides (`TvSettingRow`).
    pub fn setting(label: &'a str) -> ListRow<'a> {
        ListRow { padding: 20.0, ..ListRow::new(label) }
    }

    /// With a 20 dp icon before the label.
    pub fn icon(self, icon: Icon) -> ListRow<'a> {
        ListRow { icon: Some(icon), ..self }
    }

    /// A second line under the label (`Tv.Caption`, Muted): a hint, a setting's value.
    pub fn supporting(self, text: &'a str) -> ListRow<'a> {
        ListRow { supporting: text, ..self }
    }

    /// At most this many lines of the second line (2; a choice's hint 3).
    pub fn supporting_lines(self, lines: u16) -> ListRow<'a> {
        ListRow { supporting_lines: lines.max(1), ..self }
    }

    /// What it ends with.
    pub fn trailing(self, trailing: Trailing<'a>) -> ListRow<'a> {
        ListRow { trailing, ..self }
    }

    /// The choice in force: a check at its end.
    pub fn checked(self, checked: bool) -> ListRow<'a> {
        ListRow { trailing: Trailing::Check(checked), ..self }
    }

    /// Destructive (Delete, Sign out): its label and icon in DangerText.
    pub fn danger(self, danger: bool) -> ListRow<'a> {
        ListRow { danger, ..self }
    }

    /// Drawn at 40 % and out of focus's way when `false`.
    pub fn enabled(self, enabled: bool) -> ListRow<'a> {
        ListRow { enabled, ..self }
    }

    /// A 40 dp picture before the label (a playlist's cover), its placeholder from `seed`.
    pub fn art(self, art: &'a [ImageRequest], seed: &'a str) -> ListRow<'a> {
        ListRow { art: Some((art, seed)), ..self }
    }

    fn lead_width(&self) -> f32 {
        if self.art.is_some() {
            ART_SIZE + LEAD_GAP
        } else if self.icon.is_some() {
            theme::ICON_SIZE + LEAD_GAP
        } else {
            0.0
        }
    }

    fn trailing_width(&self, ui: &Ui) -> f32 {
        match self.trailing {
            Trailing::None | Trailing::Check(false) => 0.0,
            Trailing::Check(true) | Trailing::Chevron => LEAD_GAP + theme::ICON_SIZE,
            Trailing::Value(v) => {
                LEAD_GAP + ui.measure(v, &theme::CAPTION, &free_line()).w.min(140.0) + 4.0 + theme::ICON_SIZE
            }
            Trailing::Switch(_) => LEAD_GAP + SWITCH.w,
        }
    }

    fn text_width(&self, ui: &Ui, width: f32) -> f32 {
        (width - 2.0 * self.padding - self.lead_width() - self.trailing_width(ui)).max(1.0)
    }

    /// The row's height at `width`: its text and padding, at least 44.
    pub fn height(&self, ui: &Ui, width: f32) -> f32 {
        let w = self.text_width(ui, width);
        let mut h = ui.measure(self.label, &theme::BODY, &TextOpts::wrap(w, 2)).h;
        if !self.supporting.is_empty() {
            h += ui.measure(self.supporting, &theme::CAPTION, &TextOpts::wrap(w, self.supporting_lines)).h;
        }
        (h + 2.0 * PAD_Y).max(ROW_MIN_HEIGHT)
    }

    /// Draws the row with its top-left corner at `at`, `width` wide, registered as `id`.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point, width: f32) -> Response {
        let rect = Rect::new(at.x, at.y, width, self.height(ui, width));
        if !self.enabled {
            ui.opacity(theme::DISABLED_ALPHA, |ui| self.paint(ui, id, rect, 0.0, 0.0));
            return unregistered(id);
        }
        ui.name(id, || crate::look::spoken(&[self.label, self.supporting]));
        let r = register(ui, id, rect, FocusOpts::default());
        let p = focus_progress(ui, id, r.focused);
        let q = press_progress(ui, id, r.pressed);
        self.paint(ui, id, rect, p, q);
        r
    }

    fn paint(&self, ui: &mut Ui, id: Id, rect: Rect, p: f32, q: f32) {
        let fill = mix(mix(Color::TRANSPARENT, theme::ACCENT, p), theme::ACCENT_BRIGHT, q);
        ui.fill(rect, fill, radius::CONTROL);
        let rest = if self.danger { theme::DANGER_TEXT } else { theme::FG };
        let fg = mix(rest, theme::ON_ACCENT, p.max(q));
        let second = mix(theme::MUTED, theme::ON_ACCENT_SECONDARY, p.max(q));
        let mut x = rect.x + self.padding;
        if let Some((art, seed)) = self.art {
            let r = Rect::new(x, rect.y + (rect.h - ART_SIZE) / 2.0, ART_SIZE, ART_SIZE);
            let shown = first_art(ui, art, Priority::Visible);
            if !shown.is_some_and(|s| s.opaque()) {
                placeholder(ui, r, seed, "", radius::TILE);
            }
            if let Some(s) = shown {
                ui.draw_image(r, &s, &ImageOpts::cover().rounded(radius::TILE));
            }
            x += ART_SIZE + LEAD_GAP;
        } else if let Some(icon) = self.icon {
            // Settings rows' icons rest in Muted; panel rows' take the label's colour.
            let tint =
                if self.padding > 16.0 && !self.danger { mix(theme::MUTED, theme::ON_ACCENT, p.max(q)) } else { fg };
            let r = Rect::new(x, rect.y + (rect.h - theme::ICON_SIZE) / 2.0, theme::ICON_SIZE, theme::ICON_SIZE);
            ui.icon(r, icon, tint);
            x += theme::ICON_SIZE + LEAD_GAP;
        }
        let w = self.text_width(ui, rect.w);
        let label = ui.layout(self.label, &theme::BODY, &TextOpts::wrap(w, 2));
        let sub = (!self.supporting.is_empty())
            .then(|| ui.layout(self.supporting, &theme::CAPTION, &TextOpts::wrap(w, self.supporting_lines)));
        let block = label.size.h + sub.as_ref().map_or(0.0, |s| s.size.h);
        let mut y = rect.y + (rect.h - block) / 2.0;
        let label_h = label.size.h;
        ui.draw_text(Point::new(x, y), label, fg);
        y += label_h;
        if let Some(sub) = sub {
            ui.draw_text(Point::new(x, y), sub, second);
        }
        let right = rect.right() - self.padding;
        let icon_rect =
            |x: f32| Rect::new(x, rect.y + (rect.h - theme::ICON_SIZE) / 2.0, theme::ICON_SIZE, theme::ICON_SIZE);
        match self.trailing {
            Trailing::None | Trailing::Check(false) => {}
            Trailing::Check(true) => {
                ui.icon(icon_rect(right - theme::ICON_SIZE), Icon::Check, mix(theme::ACCENT, theme::ON_ACCENT, p))
            }
            Trailing::Chevron => ui.icon(
                icon_rect(right - theme::ICON_SIZE),
                Icon::ChevronRight,
                mix(theme::SUBTLE, theme::ON_ACCENT, p),
            ),
            Trailing::Value(v) => {
                ui.icon(
                    icon_rect(right - theme::ICON_SIZE),
                    Icon::ChevronRight,
                    mix(theme::SUBTLE, theme::ON_ACCENT, p),
                );
                let layout = ui.layout(v, &theme::CAPTION, &TextOpts::line(140.0));
                let vx = right - theme::ICON_SIZE - 4.0 - layout.size.w;
                text_mid(ui, vx, rect.y, rect.h, layout, second);
            }
            Trailing::Switch(on) => {
                let track = Rect::new(right - SWITCH.w, rect.y + (rect.h - SWITCH.h) / 2.0, SWITCH.w, SWITCH.h);
                switch(ui, id, track, on, p);
            }
        }
    }
}

/// A switch in `track` (40 × 22, radius 11, a thumb of radius 8 inset 3; `TvSettingsKit.kt`): off
/// a Surface4 track with a Control thumb, on an Accent track with an OnAccent thumb; on a focused
/// row the track turns OnAccent (30 % when off) and the thumb Accent when on, OnAccent when off.
/// The thumb slides in 150 ms.
pub fn switch(ui: &mut Ui, id: Id, track: Rect, on: bool, focus: f32) {
    let t = ui.animate(id.with("kit.switch"), if on { 1.0 } else { 0.0 }, Motion::FOCUS);
    let rest_track = mix(theme::SURFACE4, theme::ACCENT, t);
    let focus_track = mix(theme::ON_ACCENT.alpha(0.3), theme::ON_ACCENT, t);
    let rest_thumb = mix(theme::CONTROL, theme::ON_ACCENT, t);
    let focus_thumb = mix(theme::ON_ACCENT, theme::ACCENT, t);
    ui.fill(track, mix(rest_track, focus_track, focus), track.h / 2.0);
    let d = 16.0;
    let x = track.x + 3.0 + (track.w - 6.0 - d) * t;
    ui.fill(Rect::new(x, track.y + (track.h - d) / 2.0, d, d), mix(rest_thumb, focus_thumb, focus), d / 2.0);
}

/// A step row (`TvPlayerExtras.kt`): Sync and Audio delay. A title in
/// `Tv.Label` with a hint under it ("OK resets", or why it can't move), then a chevron, the value
/// ("+0.5 s") and a chevron. Left and Right step on every press that gets through the repeat gate,
/// so a held key keeps stepping; OK resets. Focused, the whole row is Accent.
#[derive(Clone, Copy, Debug)]
pub struct Stepper<'a> {
    title: &'a str,
    value: &'a str,
    hint: &'a str,
    can_down: bool,
    can_up: bool,
}

/// What a step row did this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepperResponse {
    pub response: Response,
    /// The steps taken: −1 a press of Left, +1 of Right.
    pub step: i32,
    /// OK: back to zero.
    pub reset: bool,
}

impl<'a> Stepper<'a> {
    /// A step row titled `title` showing `value`.
    pub fn new(title: &'a str, value: &'a str) -> Stepper<'a> {
        Stepper { title, value, hint: "OK resets", can_down: true, can_up: true }
    }

    /// The line under the title.
    pub fn hint(self, hint: &'a str) -> Stepper<'a> {
        Stepper { hint, ..self }
    }

    /// Whether it can step down and up (a chevron that can't is dimmed).
    pub fn limits(self, can_down: bool, can_up: bool) -> Stepper<'a> {
        Stepper { can_down, can_up, ..self }
    }

    /// Draws the row from `at`, `width` wide.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point, width: f32) -> StepperResponse {
        let pad = 16.0;
        let value_w = ui.measure(self.value, &theme::LABEL, &free_line()).w + 12.0;
        let controls = 2.0 * theme::ICON_SIZE + value_w;
        let text_w = (width - 2.0 * pad - controls - LEAD_GAP).max(1.0);
        let title = ui.layout(self.title, &theme::LABEL, &TextOpts::line(text_w));
        let hint = (!self.hint.is_empty()).then(|| ui.layout(self.hint, &theme::CAPTION, &TextOpts::wrap(text_w, 2)));
        let block = title.size.h + hint.as_ref().map_or(0.0, |h| h.size.h);
        let rect = Rect::new(at.x, at.y, width, (block + 2.0 * PAD_Y).max(ROW_MIN_HEIGHT));
        ui.name(id, || crate::look::spoken(&[self.title, self.value]));
        let r = register(ui, id, rect, FocusOpts::default().takes(Dirs::HORIZONTAL));
        let mut step = 0;
        if r.focused {
            for k in ui.keys() {
                match k.key {
                    Key::Left if k.pressed && self.can_down => step -= 1,
                    Key::Right if k.pressed && self.can_up => step += 1,
                    _ => {}
                }
            }
        }
        let p = focus_progress(ui, id, r.focused);
        ui.fill(rect, mix(Color::TRANSPARENT, theme::ACCENT, p), radius::CONTROL);
        let fg = mix(theme::FG, theme::ON_ACCENT, p);
        let second = mix(theme::MUTED, theme::ON_ACCENT_SECONDARY, p);
        let mut y = rect.y + (rect.h - block) / 2.0;
        let title_h = title.size.h;
        ui.draw_text(Point::new(rect.x + pad, y), title, fg);
        y += title_h;
        if let Some(h) = hint {
            ui.draw_text(Point::new(rect.x + pad, y), h, second);
        }
        let mut x = rect.right() - pad - controls;
        let icon_y = rect.y + (rect.h - theme::ICON_SIZE) / 2.0;
        let dim = |on: bool, c: Color| if on { c } else { c.alpha(theme::DISABLED_ALPHA) };
        ui.icon(Rect::new(x, icon_y, theme::ICON_SIZE, theme::ICON_SIZE), Icon::ChevronLeft, dim(self.can_down, fg));
        x += theme::ICON_SIZE;
        let value = ui.layout(self.value, &theme::LABEL, &free_line());
        text_mid(ui, x + 6.0, rect.y, rect.h, value, fg);
        x += value_w;
        ui.icon(Rect::new(x, icon_y, theme::ICON_SIZE, theme::ICON_SIZE), Icon::ChevronRight, dim(self.can_up, fg));
        if step != 0 {
            ui.request_repaint();
        }
        StepperResponse { response: r, step, reset: r.activated }
    }
}

/// The headings and notes between list rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Heading {
    /// A panel's title: `Tv.Title` Fg, two lines, padding 8 above and below, 16 in.
    PanelTitle,
    /// A heading in an item's panels: `Tv.Title` Fg, 16 above, 8 below, 16 in.
    Section,
    /// A small heading in a library's panels: `Tv.Caption` Muted, 16 above, 4 below, 16 in.
    Small,
    /// A heading in a playlist's panels: `Tv.Label` Muted, 16 above, 6 below, 16 in.
    Label,
    /// A heading inside a settings pane: `Tv.Label` Subtle, 16 above, 6 below, 20 in.
    Settings,
    /// A note: `Tv.Body` Muted, 8 above and below, 16 in.
    Note,
}

impl Heading {
    fn spec(self) -> (TextStyle, Color, f32, f32, f32, u16) {
        match self {
            Heading::PanelTitle => (theme::TITLE, theme::FG, 8.0, 8.0, 16.0, 2),
            Heading::Section => (theme::TITLE, theme::FG, 16.0, 8.0, 16.0, 2),
            Heading::Small => (theme::CAPTION, theme::MUTED, 16.0, 4.0, 16.0, 1),
            Heading::Label => (theme::LABEL, theme::MUTED, 16.0, 6.0, 16.0, 1),
            Heading::Settings => (theme::LABEL, theme::SUBTLE, 16.0, 6.0, 20.0, 1),
            Heading::Note => (theme::BODY, theme::MUTED, 8.0, 8.0, 16.0, 0),
        }
    }

    /// Its height saying `text` in `width`.
    pub fn height(self, ui: &Ui, text: &str, width: f32) -> f32 {
        let (style, _, top, bottom, inset, lines) = self.spec();
        top + ui.measure(text, &style, &TextOpts::wrap((width - 2.0 * inset).max(1.0), lines)).h + bottom
    }

    /// Draws it from `at`, `width` wide; returns its height.
    pub fn show(self, ui: &mut Ui, text: &str, at: Point, width: f32) -> f32 {
        let (style, color, top, bottom, inset, lines) = self.spec();
        let size = ui.text(
            Point::new(at.x + inset, at.y + top),
            text,
            &style,
            &TextOpts::wrap((width - 2.0 * inset).max(1.0), lines),
            color,
        );
        top + size.h + bottom
    }
}

/// What a scrolling list keeps between frames (owned by the screen).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ListState {
    /// The row focus is on, or was on last. Set it before the list is first drawn to open it there
    /// (a panel opens on the choice in force).
    pub index: usize,
    /// How far the content is moved up: where it is heading.
    pub scroll: f32,
    drawn: (usize, usize),
    shown: f32,
    started: bool,
}

impl ListState {
    /// A list opening at row `index`.
    pub fn at(index: usize) -> ListState {
        ListState { index, ..ListState::default() }
    }
}

/// Where a list wants a row drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListSlot {
    pub index: usize,
    /// The id to register the row's focusable as (headings register nothing).
    pub id: Id,
    /// The row's rectangle on screen.
    pub rect: Rect,
}

/// What a list did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ListResponse {
    /// The row with focus, when focus is in the list.
    pub focused: Option<usize>,
    /// How far the content shows moved up.
    pub scroll: f32,
    /// The content's height.
    pub content: f32,
}

/// A column of rows in a viewport that keeps the focused row's top at 30 % of its height, unless
/// that would run past an end (`TvScrolling.kt`; the TV app's `useScrollToFocus`), sliding in
/// 220 ms; with the pointer it moves only as far as brings a cut-off row in whole. The rows are one
/// column focus group; only those in view and three either side are drawn, so focus can always
/// reach the next row past a heading.
#[derive(Clone, Copy, Debug)]
pub struct List {
    view: Rect,
    gap: f32,
    up: Edge,
    down: Edge,
}

impl List {
    /// A list in `view`, rows 2 dp apart.
    pub fn new(view: Rect) -> List {
        List { view, gap: 2.0, up: Edge::Pass, down: Edge::Pass }
    }

    /// The space between rows.
    pub fn gap(self, gap: f32) -> List {
        List { gap, ..self }
    }

    /// What Up past the first row and Down past the last do.
    pub fn edges(self, up: Edge, down: Edge) -> List {
        List { up, down, ..self }
    }

    /// Draws `len` rows: `height(ui, i)` says how tall row `i` is, `draw` draws it in its slot.
    pub fn show(
        &self,
        ui: &mut Ui,
        id: Id,
        state: &mut ListState,
        len: usize,
        height: impl Fn(&Ui, usize) -> f32,
        mut draw: impl FnMut(&mut Ui, &ListSlot),
    ) -> ListResponse {
        let mut out = ListResponse::default();
        if len == 0 {
            state.drawn = (0, 0);
            return out;
        }
        let mut tops = Vec::with_capacity(len + 1);
        let mut y = 0.0;
        for i in 0..len {
            tops.push(y);
            y += height(ui, i) + self.gap;
        }
        let content = y - self.gap;
        tops.push(content);
        out.content = content;
        let max = (content - self.view.h).max(0.0);
        let mut focused_now = false;
        if let Some(f) = ui.focused().filter(|_| ui.focused_in(id).is_some())
            && let Some(i) = find_key(state.drawn, len, |i| id.with(i as u64) == f)
        {
            state.index = i;
            focused_now = true;
        }
        state.index = state.index.min(len - 1);
        let (lo, hi) = (tops[state.index], tops[state.index + 1] - self.gap);
        let anchored = (lo - self.view.h * theme::SCROLL_ANCHOR).clamp(0.0, max);
        if !state.started {
            state.scroll = anchored;
            state.started = true;
        } else if focused_now && ui.pointer_mode() {
            state.scroll = nudge(state.scroll, lo, hi, 0.0, self.view.h, 0.0, max);
        } else if focused_now {
            state.scroll = anchored;
        }
        state.scroll = state.scroll.clamp(0.0, max);
        let shown = ui.animate(id.with("kit.scroll"), state.scroll, SCROLL);
        state.shown = shown;
        out.scroll = shown;

        let first = (0..len).find(|&i| tops[i + 1] - self.gap > shown).unwrap_or(len - 1);
        let past = (first..len).find(|&i| tops[i] >= shown + self.view.h).unwrap_or(len);
        let d0 = first.saturating_sub(3).min(state.index.saturating_sub(3));
        let d1 = (past + 3).max(state.index + 4).min(len);
        state.drawn = (d0, d1);
        let group = Group::column().edge(pley_ui::Dir::Up, self.up).edge(pley_ui::Dir::Down, self.down);
        let view = self.view;
        let gap = self.gap;
        ui.clip(view, |ui| {
            ui.group(id, group, |ui| {
                for i in d0..d1 {
                    let rect = Rect::new(view.x, view.y + tops[i] - shown, view.w, tops[i + 1] - tops[i] - gap);
                    draw(ui, &ListSlot { index: i, id: id.with(i as u64), rect });
                }
            });
        });
        if focused_now {
            out.focused = Some(state.index);
        }
        out
    }
}
