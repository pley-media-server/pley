//! A grid of tiles (`TvPosterGrid.kt`, `TvTiles.kt`; the TV app's `PosterGrid` in `kit/grid.tsx`):
//! columns from x 96, 14 apart, as many as fit before x 858 (the
//! A–Z jump control's room); 20 dp from a caption to the next row's wells. The focused row's wells
//! stay 14 dp under the grid's top line (y 136 in a library) while the rows slide under it in
//! 220 ms, and the content never scrolls past its end. Under a held Up or Down the rows glide
//! evenly from move to move under one still ring, the tiles keeping their resting look, and the
//! pictures of the rows going by wait until the key is let go (the Samsung app's `GridView`).
//!
//! Cells are one per position, loaded or not, so a skeleton keeps focus when its page of items
//! arrives. One grid focus group returns to its last cell; Right from the last column goes where
//! [`PosterGrid::right_edge`] says (the jump control), Down from the last row stays, and a held key
//! stops at the sides [`PosterGrid::held_stop`] names. Only the rows on screen and one either side
//! are drawn; the pictures of the next 1.5 screens are asked for ahead.
//!
//! [`JumpControl`] is the library's A–Z jump (`JumpControl.kt`).

use pley_base::{Color, Key, Point, Priority, Rect};
use pley_ui::theme::{self, focus, radius};
use pley_ui::{Dir, Dirs, Edge, FocusOpts, Group, Id, Response, Ui, nudge};

use crate::look::{SCROLL, register, shade, text_centred};
use crate::row::{RowResponse, find_key, note};
use crate::tile::{SlotMode, TileKind, TileLook, TileSlot, ring};

/// Where a grid's columns must end: 960 − 48 − 40 − 14, leaving the jump control its room
/// (`TvTiles.kt`).
pub const GRID_END: f32 = 858.0;
/// The jump control's left edge.
pub const JUMP_X: f32 = 872.0;
/// The room a grid leaves under its last row.
pub const GRID_BOTTOM_PAD: f32 = 41.0;

/// What a grid keeps between frames (owned by the screen).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridState {
    /// The cell focus is on, or was on last. Set it before the grid is first drawn to open it there.
    pub index: usize,
    /// How far the content is moved up, in dp: where it is heading.
    pub scroll: f32,
    focus_to: Option<usize>,
    drawn: (usize, usize),
    shown: f32,
    jumps: u32,
    started: bool,
    /// A held key has moved focus to another row: the rows glide under one still ring.
    quiet: bool,
}

impl GridState {
    /// A grid opening at cell `index`.
    pub fn at(index: usize) -> GridState {
        GridState { index, ..GridState::default() }
    }

    /// Scrolls to cell `index` at once and focuses it (a letter chosen in the jump control).
    pub fn focus_to(&mut self, index: usize) {
        self.focus_to = Some(index);
        self.index = index;
    }

    /// Where the grid shows now (its scroll, animated).
    pub fn shown_scroll(&self) -> f32 {
        self.shown
    }
}

/// What a grid's cells answered this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GridResponse {
    /// The cell with focus, when focus is in the grid.
    pub focused: Option<usize>,
    pub gained: bool,
    pub activated: Option<usize>,
    /// The cell whose actions were asked for.
    pub actions: Option<usize>,
    /// The cells on screen (first and past the last): a long library loads pages around them.
    pub visible: (usize, usize),
    pub columns: usize,
}

/// A grid of one kind of tile.
#[derive(Clone, Copy, Debug)]
pub struct PosterGrid {
    kind: TileKind,
    top: f32,
    columns: Option<usize>,
    left: Edge,
    right: Edge,
    bottom_pad: f32,
    long_press: bool,
    held_stops: Dirs,
}

impl PosterGrid {
    /// A grid of `kind` whose box starts at `top` (122 in a library: wells at 136).
    pub fn new(kind: TileKind, top: f32) -> PosterGrid {
        PosterGrid {
            kind,
            top,
            columns: None,
            left: Edge::Out,
            right: Edge::Stop,
            bottom_pad: GRID_BOTTOM_PAD,
            long_press: true,
            held_stops: Dirs::NONE,
        }
    }

    /// Sides a held key stops at, where a press leaves the grid: a library's Up to its chips and
    /// Right to the jump control.
    pub fn held_stop(self, dirs: Dirs) -> PosterGrid {
        PosterGrid { held_stops: dirs, ..self }
    }

    /// The columns that fit between the gutter and [`GRID_END`]: 7 for library posters, 6 for
    /// posters, 5 for squares.
    pub fn columns_for(kind: TileKind) -> usize {
        let w = kind.size().w;
        (((GRID_END - theme::GUTTER + theme::GRID_COLUMN_GAP) / (w + theme::GRID_COLUMN_GAP)).floor() as usize).max(1)
    }

    /// A fixed number of columns.
    pub fn columns(self, columns: usize) -> PosterGrid {
        PosterGrid { columns: Some(columns.max(1)), ..self }
    }

    /// What Right past the last column does (stops by default; the jump control's id when it shows).
    pub fn right_edge(self, edge: Edge) -> PosterGrid {
        PosterGrid { right: edge, ..self }
    }

    /// What Left past the first column does (leaves the page for the rail by default).
    pub fn left_edge(self, edge: Edge) -> PosterGrid {
        PosterGrid { left: edge, ..self }
    }

    /// The room under the last row.
    pub fn bottom_pad(self, pad: f32) -> PosterGrid {
        PosterGrid { bottom_pad: pad, ..self }
    }

    /// Whether OK held 500 ms is a long press on its cells (on by default).
    pub fn long_press(self, on: bool) -> PosterGrid {
        PosterGrid { long_press: on, ..self }
    }

    /// The columns this grid has.
    pub fn column_count(&self) -> usize {
        self.columns.unwrap_or_else(|| PosterGrid::columns_for(self.kind))
    }

    /// From one row's wells to the next: well + 14 + 34 + 20 (212 for library posters).
    pub fn row_pitch(&self) -> f32 {
        self.kind.size().h + theme::TILES_TO_CAPTION + theme::CAPTION_HEIGHT + theme::GRID_ROW_GAP
    }

    /// The farthest the content scrolls for `len` cells: until its last row's caption ends the pad
    /// above the screen's bottom.
    pub fn max_scroll(&self, len: usize) -> f32 {
        let rows = len.div_ceil(self.column_count());
        if rows == 0 {
            return 0.0;
        }
        let content = theme::GRID_CONTENT_TOP + rows as f32 * self.row_pitch() - theme::GRID_ROW_GAP + self.bottom_pad;
        (content - (theme::CANVAS_H - self.top)).max(0.0)
    }

    /// The scroll that puts cell `index`'s row on the focused line.
    pub fn scroll_for(&self, index: usize, len: usize) -> f32 {
        ((index / self.column_count()) as f32 * self.row_pitch()).min(self.max_scroll(len))
    }

    /// Draws the grid of `len` cells; `draw` draws a cell (a tile, or a skeleton for a position
    /// whose page hasn't come) or asks for its picture.
    pub fn show(
        &self,
        ui: &mut Ui,
        id: Id,
        state: &mut GridState,
        len: usize,
        mut draw: impl FnMut(&mut Ui, &TileSlot),
    ) -> GridResponse {
        let cols = self.column_count();
        let mut out = GridResponse { columns: cols, ..GridResponse::default() };
        if len == 0 {
            state.drawn = (0, 0);
            return out;
        }
        let size = self.kind.size();
        let pitch = self.row_pitch();
        let col_pitch = size.w + theme::GRID_COLUMN_GAP;
        let max = self.max_scroll(len);

        let mut focused_now = false;
        let row_before = state.index / cols;
        if let Some(f) = ui.focused().filter(|_| ui.focused_in(id).is_some())
            && let Some(i) = find_key(state.drawn, len, |i| id.with(i as u64) == f)
        {
            state.index = i;
            focused_now = true;
        }
        // A held Up or Down: the rows slide under a still ring until the key is let go (a tile
        // growing and shrinking at each step, its ring with it, made the page stutter).
        if !ui.holding() {
            state.quiet = false;
        } else if focused_now && state.index / cols != row_before {
            state.quiet = true;
        }
        let jump = state.focus_to.take();
        if let Some(i) = jump {
            state.index = i;
        }
        state.index = state.index.min(len - 1);
        let wells_top = self.top + theme::GRID_CONTENT_TOP;
        if jump.is_some() || !state.started {
            state.scroll = self.scroll_for(state.index, len);
            state.jumps = state.jumps.wrapping_add(jump.is_some() as u32);
            state.started = true;
        } else if focused_now && ui.pointer_mode() {
            let lo = wells_top + (state.index / cols) as f32 * pitch;
            let hi = lo + size.h + theme::TILES_TO_CAPTION + theme::CAPTION_HEIGHT;
            state.scroll = nudge(state.scroll, lo, hi, self.top, theme::CANVAS_H, 0.0, max);
        } else if focused_now {
            state.scroll = self.scroll_for(state.index, len);
        }
        state.scroll = state.scroll.clamp(0.0, max);
        let shown = ui.scroll(id.with(("kit.scroll", state.jumps)), state.scroll, SCROLL);
        state.shown = shown;

        let rows = len.div_ceil(cols);
        let cell_h = size.h + theme::TILES_TO_CAPTION + theme::CAPTION_HEIGHT;
        let row_y = |r: usize| wells_top + r as f32 * pitch - shown;
        let first_row = (((shown - theme::GRID_CONTENT_TOP - cell_h) / pitch).floor() + 1.0).max(0.0) as usize;
        let past_row = ((((theme::CANVAS_H - wells_top) + shown) / pitch).ceil().max(0.0) as usize).min(rows);
        let first_row = first_row.min(past_row);
        // The focused row and its neighbours are drawn even while the grid is still sliding.
        let focused_row = state.index / cols;
        let r0 = first_row.saturating_sub(1).min(focused_row.saturating_sub(1));
        let r1 = (past_row + 1).max(focused_row + 2).min(rows);
        let per_screen = ((theme::CANVAS_H - self.top) / pitch).ceil() as usize;
        let ahead = (r1 + (per_screen as f32 * 1.5).ceil() as usize).min(rows);
        let (d0, d1) = (r0 * cols, (r1 * cols).min(len));
        state.drawn = (d0, d1);
        out.visible = ((first_row * cols).min(len), (past_row * cols).min(len));

        let opts = if self.long_press { FocusOpts::default().long_press() } else { FocusOpts::default() };
        // Down from the last row stays.
        let group = Group::grid(cols as u16)
            .edge(Dir::Left, self.left)
            .edge(Dir::Right, self.right)
            .edge(Dir::Down, Edge::Stop)
            .held_stop(self.held_stops);
        let cell = |i: usize| {
            let x = theme::GUTTER + (i % cols) as f32 * col_pitch;
            Rect::new(x, row_y(i / cols), size.w, size.h)
        };
        let (fr, fv) = (out.visible.0, out.visible.1);
        let quiet = state.quiet;
        // Gliding, the rows going by ask for their pictures behind everything else: where focus
        // lands loads once the key is let go.
        let on_screen = if quiet { Priority::Prefetch } else { Priority::Visible };
        let near = if quiet { Priority::Prefetch } else { Priority::Near };
        let clip = Rect::new(0.0, self.top, theme::CANVAS_W, theme::CANVAS_H - self.top);
        let mut slots: Vec<TileSlot> = Vec::with_capacity(d1 - d0);
        let kind = self.kind;
        ui.clip(clip, |ui| {
            ui.group(id, group, |ui| {
                for i in d0..d1 {
                    let cell_id = id.with(i as u64);
                    let response = register(ui, cell_id, cell(i), opts);
                    let priority = if i >= fr && i < fv { on_screen } else { near };
                    slots.push(TileSlot {
                        index: i,
                        id: cell_id,
                        kind,
                        rect: cell(i),
                        response,
                        mode: SlotMode::Draw,
                        priority,
                    });
                }
            });
            let mut focused_slot = None;
            for slot in &slots {
                if slot.response.focused && !quiet {
                    focused_slot = Some(*slot);
                } else if slot.response.focused {
                    // At rest under the still ring.
                    let response = Response { focused: false, gained: false, ..slot.response };
                    draw(ui, &TileSlot { response, ..*slot });
                } else {
                    draw(ui, slot);
                }
            }
            if let Some(slot) = focused_slot {
                draw(ui, &slot);
            }
            if quiet {
                // The ring where the focused cell is heading: on the focused line, or where the
                // content's end leaves it.
                let x = theme::GUTTER + (state.index % cols) as f32 * col_pitch;
                let y = wells_top + (state.index / cols) as f32 * pitch - state.scroll;
                let rest = TileLook { progress: 1.0, scale: 1.0 };
                ring(ui, Rect::new(x, y, size.w, size.h), kind, &rest);
            }
        });
        let mut rr = RowResponse::default();
        for slot in &slots {
            note(&mut rr, slot.index, &slot.response);
        }
        (out.focused, out.gained, out.activated, out.actions) = (rr.focused, rr.gained, rr.activated, rr.actions);
        // Nothing is fetched ahead while gliding.
        let ahead = if quiet { r1 } else { ahead };
        for i in d1..(ahead * cols).min(len) {
            let cell_id = id.with(i as u64);
            let slot = TileSlot {
                index: i,
                id: cell_id,
                kind,
                rect: cell(i),
                response: crate::look::unregistered(cell_id),
                mode: SlotMode::Prefetch,
                priority: Priority::Prefetch,
            };
            draw(ui, &slot);
        }
        if jump.is_some() {
            ui.request_focus(id.with(state.index as u64));
        }
        out
    }
}

/// The A–Z jump control (`JumpControl.kt`, `AlphaIndex.kt`): a 40 × 40 Surface3
/// box (radius 6) at x 872, level with the grid's first row, with 20 dp of room above and below.
/// At rest it shows the letter of the focused tile's section in `Tv.Title`; focused (Accent, 1.08)
/// Up and Down step through `# A … Z` with the neighbours shown above and below in `Tv.Caption`
/// (Muted, or Disabled at 60 % when no title starts with them); Up at "#" leaves upward; OK jumps;
/// Left goes back to the grid.
#[derive(Clone, Copy, Debug)]
pub struct JumpControl<'a> {
    /// Which of the 27 sections (`#`, then A to Z) have titles.
    pub available: &'a [bool; 27],
    /// The section of the focused tile, shown at rest.
    pub current: usize,
}

/// What the jump control did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpResponse {
    pub response: Response,
    /// The section chosen with OK (0 is "#", 1 to 26 are A to Z).
    pub chosen: Option<usize>,
    /// The section shown while focused.
    pub letter: usize,
}

/// The jump control's sections, in order.
pub const JUMP_LETTERS: [char; 27] = [
    '#', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V',
    'W', 'X', 'Y', 'Z',
];

impl JumpControl<'_> {
    /// The box's size.
    pub const SIZE: f32 = 40.0;

    /// The section a title belongs to: its first letter with accents stripped, "#" for anything
    /// else (`AlphaIndex.kt`).
    pub fn section_of(sort_title: &str) -> usize {
        let Some(c) = sort_title.chars().next() else { return 0 };
        let base = match c {
            'À'..='Å' | 'à'..='å' => 'A',
            'Ç' | 'ç' => 'C',
            'È'..='Ë' | 'è'..='ë' => 'E',
            'Ì'..='Ï' | 'ì'..='ï' => 'I',
            'Ñ' | 'ñ' => 'N',
            'Ò'..='Ö' | 'Ø' | 'ò'..='ö' | 'ø' => 'O',
            'Ù'..='Ü' | 'ù'..='ü' => 'U',
            'Ý' | 'ý' | 'ÿ' => 'Y',
            c => c.to_ascii_uppercase(),
        };
        if base.is_ascii_uppercase() { (base as u8 - b'A') as usize + 1 } else { 0 }
    }

    /// Draws the control with its box's top-left corner at `at` (x 872, the first row's wells'
    /// top). `letter` is the section stepped to while it has focus (kept by the screen).
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point, letter: &mut usize) -> JumpResponse {
        let rect = Rect::new(at.x, at.y, Self::SIZE, Self::SIZE);
        let takes = if *letter == 0 { Dirs::DOWN } else { Dirs::VERTICAL };
        let r = register(ui, id, rect, FocusOpts::default().takes(takes));
        if r.gained {
            *letter = self.current;
        }
        if !r.focused {
            *letter = self.current.min(26);
        }
        let mut chosen = None;
        if r.focused {
            for k in ui.keys() {
                if !k.pressed {
                    continue;
                }
                match k.key {
                    Key::Up => *letter = letter.saturating_sub(1),
                    Key::Down => *letter = (*letter + 1).min(26),
                    _ => {}
                }
            }
            if r.activated {
                chosen = Some(*letter);
            }
        }
        let s = shade(ui, &focus::JUMP, &r);
        ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
            ui.fill(rect, s.fill, radius::CONTROL);
            text_centred(ui, rect, &JUMP_LETTERS[*letter].to_string(), &theme::TITLE, s.content);
        });
        if s.progress > 0.0 {
            // The neighbours, in the 20 dp rooms above and below.
            let neighbour = |ui: &mut Ui, i: usize, y: f32| {
                let c = if self.available[i] { theme::MUTED } else { theme::DISABLED.alpha(0.6) };
                let r = Rect::new(rect.x, y, Self::SIZE, 20.0);
                let alpha = c.a as f32 / 255.0 * s.progress;
                text_centred(ui, r, &JUMP_LETTERS[i].to_string(), &theme::CAPTION, Color { a: 255, ..c }.alpha(alpha));
            };
            if *letter > 0 {
                neighbour(ui, *letter - 1, rect.y - 20.0);
            }
            if *letter < 26 {
                neighbour(ui, *letter + 1, rect.bottom());
            }
        }
        JumpResponse { response: r, chosen, letter: *letter }
    }
}
