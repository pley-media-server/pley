//! The open-source licences (About's rows open it): the parts Pley for LG TV is built from on the
//! left, as Settings lays out its categories, and the focused part's licence texts on the right.
//! Right or OK goes into the text, where Up and Down scroll it (and the Magic Remote's wheel);
//! Left goes back to the list. The texts are NOTICES.txt's, which the package carries.
//!
//! A licence text runs to 20 KB: it is laid out a frame's worth at a time from its top, never
//! all in the frame its part gets focus, so moving down the list stays smooth.

use std::sync::Arc;

use pley_base::{Key, Point, Rect, TextOpts};
use pley_i18n::t;
use pley_kit::look::{focus_progress, mix};
use pley_kit::{List, ListRow, ListState, PAGE_SCROLL};
use pley_shell::{AppCx, Chrome, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dirs, Edge, FocusOpts, Group, Id, Ui};

use crate::notices::{self, Notice, Notices};

/// The list's column, as Settings' categories.
const LIST_X: f32 = theme::GUTTER;
const LIST_W: f32 = 256.0;
const TOP: f32 = 81.0;
/// The text's pane, as Settings' rows: x 376 to 912.
const PANE_X: f32 = 376.0;
const PANE_W: f32 = theme::CANVAS_W - theme::SAFE_X - PANE_X;
/// The text's inset in its pane.
const INSET: f32 = 16.0;
/// The text's width.
const TEXT_W: f32 = PANE_W - 2.0 * INSET;
/// How far a press of Up or Down scrolls the text: three lines.
const STEP: f32 = 3.0 * 20.0;
/// The text laid out in a frame, in characters: enough to fill the pane in the first.
const MEASURE_CHARS: usize = 1600;

/// What the page says when the app carries no NOTICES.txt.
pub fn no_notices() -> &'static str {
    t!("settings.about.noNotices")
}

/// A part's row in the list.
fn part_row(part: &Notice) -> ListRow<'_> {
    ListRow::setting(&part.name).supporting(&part.licence).supporting_lines(1)
}

/// About how tall `line` is at the text's width, until it is measured: for the scroll bar.
fn estimate(line: &str) -> f32 {
    let per_line = (TEXT_W / (theme::BODY.size * 0.5)).max(1.0);
    (line.chars().count() as f32 / per_line).ceil().max(1.0) * theme::BODY.line_height
}

/// The licences page.
#[derive(Default)]
pub struct LicencesScreen {
    opened: bool,
    list: ListState,
    shown: usize,
    /// Each part's row height in the list, measured once.
    row_heights: Vec<f32>,
    /// The shown part's text, line by line.
    lines: Vec<String>,
    /// The heights of the lines measured so far, from the top.
    heights: Vec<f32>,
    /// The part `lines` are of.
    laid_out: Option<usize>,
    scroll: f32,
}

impl LicencesScreen {
    /// The page, on the part About's row named.
    pub fn new() -> LicencesScreen {
        LicencesScreen::default()
    }

    /// The part shown.
    pub fn shown(&self) -> usize {
        self.shown
    }

    /// How far the text is scrolled.
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// The id of the list's row `index`, under the page's `root`.
    pub fn row_id(root: Id, index: usize) -> Id {
        root.with("parts").with(index as u64)
    }

    /// The id of the text, under the page's `root`.
    pub fn text_id(root: Id) -> Id {
        root.with("text")
    }

    /// Takes the shown part's text, to be measured from its top.
    fn lay_out(&mut self, notices: &Notices) {
        if self.laid_out == Some(self.shown) {
            return;
        }
        let Some(part) = notices.parts.get(self.shown) else { return };
        let mut head = vec![part.licence.clone()];
        head.extend(part.link.clone());
        self.lines = head.into_iter().chain([String::new()]).chain(part.text.lines().map(str::to_string)).collect();
        self.heights.clear();
        self.laid_out = Some(self.shown);
        self.scroll = 0.0;
    }

    /// Measures the next lines of the text, a frame's worth; whether some are left.
    fn measure_more(&mut self, ui: &Ui) -> bool {
        let opts = TextOpts::wrap(TEXT_W, 0);
        let mut budget = MEASURE_CHARS;
        while budget > 0
            && let Some(line) = self.lines.get(self.heights.len())
        {
            let h = if line.is_empty() { theme::BODY.line_height } else { ui.measure(line, &theme::BODY, &opts).h };
            self.heights.push(h);
            budget = budget.saturating_sub(line.len().max(1));
        }
        self.heights.len() < self.lines.len()
    }

    fn text(&mut self, ui: &mut Ui, root: Id, part: &Notice) {
        if self.measure_more(ui) {
            ui.request_repaint();
        }
        let rect = Rect::new(PANE_X, TOP, PANE_W, theme::CANVAS_H - TOP - theme::SAFE_Y);
        let id = LicencesScreen::text_id(root);
        let r = ui.group(root.with("reader"), Group::free().wheel(), |ui| {
            ui.focusable_with(id, rect, FocusOpts::default().takes(Dirs::VERTICAL))
        });
        let title_h = theme::TITLE.line_height + 8.0;
        let view =
            Rect::new(rect.x + INSET, rect.y + INSET + title_h, rect.w - 2.0 * INSET, rect.h - 2.0 * INSET - title_h);
        // It scrolls as far as it is measured, which runs ahead of any reader; the bar counts the
        // rest as estimated.
        let measured: f32 = self.heights.iter().sum();
        let rest: f32 = self.lines[self.heights.len()..].iter().map(|l| estimate(l)).sum();
        let content = measured + rest;
        let max = (measured - view.h).max(0.0);
        if r.focused {
            for k in ui.keys() {
                match k.key {
                    Key::Up if k.pressed => self.scroll -= STEP,
                    Key::Down if k.pressed => self.scroll += STEP,
                    Key::ChannelUp if k.pressed => self.scroll -= view.h - STEP,
                    Key::ChannelDown if k.pressed => self.scroll += view.h - STEP,
                    _ => {}
                }
            }
        }
        self.scroll -= ui.wheel(root.with("reader")) as f32 * STEP;
        self.scroll = self.scroll.clamp(0.0, max);
        let shown = ui.animate(id.with("scroll"), self.scroll, PAGE_SCROLL);
        let p = focus_progress(ui, id, r.focused);
        ui.fill(rect, mix(theme::SURFACE, theme::SURFACE2, p), radius::CARD);
        ui.text(
            Point::new(rect.x + INSET, rect.y + INSET),
            &part.name,
            &theme::TITLE,
            &TextOpts::line(rect.w - 2.0 * INSET),
            theme::FG,
        );
        let opts = TextOpts::wrap(view.w, 0);
        ui.clip(view, |ui| {
            let mut y = view.y - shown;
            for (line, h) in self.lines.iter().zip(&self.heights) {
                if y + h >= view.y && !line.is_empty() {
                    ui.text(Point::new(view.x, y), line, &theme::BODY, &opts, theme::SUMMARY);
                }
                y += h;
                if y > view.bottom() {
                    break;
                }
            }
        });
        if content > view.h {
            // Where the text is: a 3 dp bar along the pane's right edge.
            let track = Rect::new(rect.right() - 6.0, view.y, 3.0, view.h);
            ui.fill(track, theme::SURFACE4, 1.5);
            let thumb_h = (view.h * view.h / content).max(24.0);
            let at = shown / (content - view.h);
            let thumb_y = track.y + (track.h - thumb_h) * at.clamp(0.0, 1.0);
            ui.fill(Rect::new(track.x, thumb_y, 3.0, thumb_h), theme::MUTED, 1.5);
        }
    }
}

impl Screen for LicencesScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let root = ui.scope_id();
        if !self.opened {
            self.opened = true;
            self.shown = notices::take_open_at();
            self.list = ListState::at(self.shown);
        }
        ui.text(
            Point::new(theme::GUTTER, theme::SAFE_Y),
            t!("settings.about.licences"),
            &theme::PAGE_TITLE,
            &TextOpts::line(LIST_W + PANE_W),
            theme::FG,
        );
        let Some(read) = notices::shared(cx.tasks()) else { return };
        let Some(notices) = read.filter(|n| !n.parts.is_empty()) else {
            // About lists no parts then, so only a direct way here meets this: say so, with Back.
            let (title, back) = (t!("settings.about.noLicences"), t!("settings.actions.back"));
            if crate::history::message(ui, root.with("message"), title, no_notices(), back) {
                cx.back();
            }
            return;
        };
        let notices: Arc<Notices> = notices;
        let parts = &notices.parts;
        self.shown = self.shown.min(parts.len() - 1);
        if self.row_heights.len() != parts.len() {
            self.row_heights = parts.iter().map(|p| part_row(p).height(ui, LIST_W)).collect();
        }
        ui.initial_focus(LicencesScreen::row_id(root, self.shown));
        let view = Rect::new(LIST_X, TOP, LIST_W, theme::CANVAS_H - TOP);
        let heights = &self.row_heights;
        let mut into = false;
        let mut focused = None;
        List::new(view).gap(2.0).edges(Edge::Stop, Edge::Stop).show(
            ui,
            root.with("parts"),
            &mut self.list,
            parts.len(),
            |_, i| heights[i],
            |ui, s| {
                let r = part_row(&parts[s.index]).show(ui, s.id, s.rect.origin(), s.rect.w);
                if r.focused {
                    focused = Some(s.index);
                }
                into |= r.activated;
            },
        );
        if let Some(i) = focused {
            self.shown = i;
        }
        self.lay_out(&notices);
        if let Some(part) = parts.get(self.shown) {
            self.text(ui, root, part);
        }
        if into {
            ui.request_focus(LicencesScreen::text_id(root));
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }
}
