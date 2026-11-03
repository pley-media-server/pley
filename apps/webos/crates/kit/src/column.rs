//! A page of sections (`TvRowColumn` in `TvRow.kt`, `TvScrolling.kt`; the TV app's `SectionPage` in
//! `kit/row.tsx`): sections stacked 28 dp apart, the page moved so the focused
//! section's header sits on its anchor line (172 on Home and the item page; 80 on library tabs; 96
//! on Search; 240 on a person's page), or at the page's top for a hero. Once moved, nothing is
//! drawn above the clip line (the anchor less 12: 160 on Home), which leaves the area above for the
//! info block. The page slides in 240 ms (gliding evenly from move to move under a held key), jumps
//! when the move is more than a screen (Back to Home's hero), and with the pointer moves only as far as brings what it points
//! at in whole.
//!
//! The sections form one column focus group: Up and Down go to the section above or below, which
//! takes focus back where it last had it (a row its tile), and coming back from the rail restores
//! the section focus was in. Only the sections on screen and one either side are drawn.

use pley_base::Rect;
use pley_ui::theme;
use pley_ui::{Group, Id, Ui, nudge};

use crate::look::PAGE_SCROLL;

/// One section of a page: a row, a hero, a block of details.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    /// Names the section (stable while the page lives).
    pub key: u64,
    /// Its height, from its top (a row's header) to its bottom (a row's caption).
    pub height: f32,
    /// The y its top sits at while it has focus; `None` puts the page at its top (a hero).
    pub anchor: Option<f32>,
}

impl Section {
    /// A row whose header goes to the focused-row line (y 172).
    pub fn row(key: u64, height: f32) -> Section {
        Section { key, height, anchor: Some(theme::FOCUSED_ROW_HEADER_Y) }
    }

    /// A hero: focus in it shows the page from its top.
    pub fn hero(key: u64, height: f32) -> Section {
        Section { key, height, anchor: None }
    }

    /// A section whose top goes to `anchor` while focused.
    pub fn new(key: u64, height: f32, anchor: Option<f32>) -> Section {
        Section { key, height, anchor }
    }
}

/// What a page of sections keeps between frames (owned by the screen).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColumnState {
    /// The section focus is in, or was in last.
    pub section: usize,
    /// How far the page is moved up, in dp: where it is heading.
    pub shift: f32,
    /// Added to the shift while focus is in the last section: the item page reading its details
    /// on past what can take focus (`TvReadOn.kt`).
    pub extra: f32,
    drawn: (usize, usize),
    shown: f32,
    jumps: u32,
    started: bool,
}

impl ColumnState {
    /// Where the page shows now (its shift, animated).
    pub fn shown_shift(&self) -> f32 {
        self.shown
    }
}

/// What a page of sections did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColumnResponse {
    /// The section focus is in, if it is in the page.
    pub focused: Option<usize>,
    /// How far the page shows moved up.
    pub shift: f32,
    /// The sections drawn (first and past the last).
    pub drawn: (usize, usize),
}

/// A page of sections from `top`.
#[derive(Clone, Copy, Debug)]
pub struct RowColumn {
    top: f32,
    clip_above: Option<f32>,
    gap: f32,
}

impl RowColumn {
    /// Sections from `top`, [`theme::ROW_GAP`] apart, clipped above y 160 once the page moves.
    pub fn new(top: f32) -> RowColumn {
        RowColumn { top, clip_above: Some(theme::FOCUSED_ROW_HEADER_Y - theme::ROWS_CLIP_ABOVE), gap: theme::ROW_GAP }
    }

    /// The clip line (`None`: never clipped).
    pub fn clip_above(self, y: Option<f32>) -> RowColumn {
        RowColumn { clip_above: y, ..self }
    }

    /// The space between sections.
    pub fn gap(self, gap: f32) -> RowColumn {
        RowColumn { gap, ..self }
    }

    /// The clip line: once the page has moved, the area above it is the info block's.
    pub fn clip_line(&self) -> Option<f32> {
        self.clip_above
    }

    /// Each section's top on the unmoved page.
    pub fn tops(&self, sections: &[Section]) -> Vec<f32> {
        let mut y = self.top;
        sections
            .iter()
            .map(|s| {
                let t = y;
                y += s.height + self.gap;
                t
            })
            .collect()
    }

    /// The shift that puts section `i` on its anchor line.
    pub fn shift_for(&self, sections: &[Section], i: usize) -> f32 {
        let Some(s) = sections.get(i) else { return 0.0 };
        match s.anchor {
            Some(anchor) => (self.tops(&sections[..=i])[i] - anchor).max(0.0),
            None => 0.0,
        }
    }

    /// Draws the page: `draw(ui, index, top)` draws section `index` with its top at `top` on screen.
    /// Each section is drawn inside a group of its own, so what it registers moves as one.
    pub fn show(
        &self,
        ui: &mut Ui,
        id: Id,
        state: &mut ColumnState,
        sections: &[Section],
        mut draw: impl FnMut(&mut Ui, usize, f32),
    ) -> ColumnResponse {
        let mut out = ColumnResponse::default();
        if sections.is_empty() {
            state.drawn = (0, 0);
            return out;
        }
        let tops = self.tops(sections);
        let len = sections.len();
        let mut focused = None;
        if ui.focused_in(id).is_some() {
            let lo = state.drawn.0.saturating_sub(1).min(len);
            let hi = (state.drawn.1 + 1).min(len);
            focused = (lo..hi).chain(0..len).find(|&i| ui.focused_in(id.with(sections[i].key)).is_some());
        }
        if let Some(i) = focused {
            state.section = i;
        }
        state.section = state.section.min(len - 1);
        let target = match focused {
            Some(i) if ui.pointer_mode() => match ui.focused_rect(id.with(sections[i].key)) {
                Some(r) => {
                    let (lo, hi) = (r.y + state.shown, r.bottom() + state.shown);
                    let view_lo = if state.shift > 0.0 { self.clip_above.unwrap_or(0.0) } else { 0.0 };
                    nudge(state.shift, lo, hi, view_lo, theme::CANVAS_H, 0.0, f32::INFINITY)
                }
                None => state.shift,
            },
            Some(i) => self.shift_for(sections, i) + if i + 1 == len { state.extra } else { 0.0 },
            None if !state.started => self.shift_for(sections, state.section),
            None => state.shift,
        };
        state.started = true;
        if (target - state.shown).abs() > theme::CANVAS_H {
            // More than a screen away: go there at once.
            state.jumps = state.jumps.wrapping_add(1);
        }
        state.shift = target;
        let shown = ui.scroll(id.with(("kit.shift", state.jumps)), target, PAGE_SCROLL);
        state.shown = shown;

        let clip = self.clip_above.filter(|_| shown > 0.5);
        let view_top = clip.unwrap_or(0.0);
        let visible = |i: usize| {
            let y = tops[i] - shown;
            y + sections[i].height > view_top && y < theme::CANVAS_H
        };
        let first = (0..len).find(|&i| visible(i)).unwrap_or(state.section);
        let past = (first..len).take_while(|&i| visible(i)).last().map_or(first + 1, |i| i + 1);
        // The focused section and its neighbours are drawn even while the page is still moving.
        let d0 = first.saturating_sub(1).min(state.section.saturating_sub(1));
        let d1 = (past + 1).max(state.section + 2).min(len);
        state.drawn = (d0, d1);
        out.drawn = (d0, d1);
        out.focused = focused;
        out.shift = shown;

        let mut body = |ui: &mut Ui| {
            ui.group(id, Group::column(), |ui| {
                for i in d0..d1 {
                    let sid = id.with(sections[i].key);
                    ui.group(sid, Group::free(), |ui| draw(ui, i, tops[i] - shown));
                }
            })
        };
        match clip {
            Some(c) => ui.clip(Rect::new(0.0, c, theme::CANVAS_W, theme::CANVAS_H - c), body),
            None => body(ui),
        }
        out
    }
}
