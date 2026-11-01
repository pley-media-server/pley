//! A row of tiles (`TvRow.kt`, `TvScrolling.kt`; the TV app's `kit/row.tsx`): a header
//! in `Tv.Section`, 14 dp, then tiles from the gutter (x 96) that slide so the focused tile's left
//! edge stays at x 96, except that a row whose tiles all end before x 912 never moves and the last
//! tile never comes further left than its end padding allows (48 + a tile's width), so focus walks
//! right near the end. The slide takes 220 ms and jumps while a key is held; the pointer moves the
//! row only as far as brings a cut-off tile in whole.
//!
//! The row is one focus group that comes back to its last tile, entered from above or below at the
//! nearest tile; Left at its start leaves the page (the shell opens the rail), Right at its end
//! stops. Only the tiles on screen and one either side are drawn; the pictures of the next 1.5
//! screens are asked for ahead.

use pley_base::{Point, Priority, Rect};
use pley_ui::theme;
use pley_ui::{Dir, Edge, FocusOpts, Group, Id, Response, Ui, nudge};

use crate::look::{SCROLL, register};
use crate::tile::{SlotMode, TileKind, TileSlot};

/// What a row keeps between frames: owned by the screen, so a page drawn again opens where it was.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowState {
    /// The tile focus is on, or was on last (the one the row keeps in place). Set it before the row
    /// is first drawn to open it at a tile (a season's next episode).
    pub index: usize,
    /// How far the tiles are moved left, in dp: where the row is heading.
    pub offset: f32,
    drawn: (usize, usize),
    shown: f32,
    started: bool,
}

impl RowState {
    /// A row opening at tile `index`.
    pub fn at(index: usize) -> RowState {
        RowState { index, ..RowState::default() }
    }

    /// Where the row shows now (its offset, animated).
    pub fn shown_offset(&self) -> f32 {
        self.shown
    }
}

/// What a row's tiles answered this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowResponse {
    /// The tile with focus, when focus is in the row.
    pub focused: Option<usize>,
    /// Focus arrived on [`RowResponse::focused`] this frame.
    pub gained: bool,
    /// The tile OK (or a click) selected.
    pub activated: Option<usize>,
    /// The tile whose actions were asked for (OK held or the menu key).
    pub actions: Option<usize>,
    /// The tiles drawn on screen (first and past the last).
    pub visible: (usize, usize),
    /// The row's height, header to caption.
    pub height: f32,
}

/// A row of tiles of one kind.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    kind: TileKind,
    header: Option<&'a str>,
    y: f32,
    x: f32,
    left: Edge,
    right: Edge,
    long_press: bool,
}

/// The view a row's pointer nudges keep tiles whole in: right of the closed rail to the screen's
/// edge (`nudge` in the TV app's `kit/row.tsx`).
const POINTER_VIEW: (f32, f32) = (theme::RAIL_COLLAPSED, theme::CANVAS_W);

impl<'a> Row<'a> {
    /// A row of `kind` tiles with its top at `y`.
    pub fn new(kind: TileKind, y: f32) -> Row<'a> {
        Row { kind, header: None, y, x: theme::GUTTER, left: Edge::Out, right: Edge::Stop, long_press: true }
    }

    /// With a header line.
    pub fn header(self, header: &'a str) -> Row<'a> {
        Row { header: Some(header), ..self }
    }

    /// Tiles start at `x` instead of the gutter.
    pub fn start(self, x: f32) -> Row<'a> {
        Row { x, ..self }
    }

    /// What Left past the first tile does (it leaves the page by default).
    pub fn left_edge(self, edge: Edge) -> Row<'a> {
        Row { left: edge, ..self }
    }

    /// What Right past the last tile does (it stops by default).
    pub fn right_edge(self, edge: Edge) -> Row<'a> {
        Row { right: edge, ..self }
    }

    /// Whether OK held 500 ms is a long press on its tiles (on by default, for their actions).
    pub fn long_press(self, on: bool) -> Row<'a> {
        Row { long_press: on, ..self }
    }

    /// The row's height.
    pub fn height(&self) -> f32 {
        self.kind.row_height(self.header.is_some())
    }

    /// Where the wells' tops are.
    pub fn tiles_y(&self) -> f32 {
        self.y + if self.header.is_some() { theme::ROW_HEADER_HEIGHT + theme::HEADER_TO_TILES } else { 0.0 }
    }

    /// The farthest the tiles move left for `len` of them: none when they all end before x 912,
    /// else as far as leaves the last tile its end padding (48 + a tile's width).
    pub fn max_offset(&self, len: usize) -> f32 {
        if len == 0 {
            return 0.0;
        }
        let w = self.kind.size().w;
        let span = len as f32 * self.kind.pitch() - self.kind.gap();
        if self.x + span <= theme::ROW_FITS_BEFORE {
            return 0.0;
        }
        (self.x + span + theme::ROW_END_PADDING + w - theme::CANVAS_W).max(0.0)
    }

    /// The offset that keeps tile `index` at the row's start.
    pub fn offset_for(&self, index: usize, len: usize) -> f32 {
        (index as f32 * self.kind.pitch()).min(self.max_offset(len))
    }

    /// Draws the row of `len` tiles. `key` names each tile (an item's id, so focus follows it);
    /// `draw` draws a tile in its slot (with [`crate::Tile::paint`]) or, for slots past the
    /// screen, asks for its picture. The focused tile is drawn last, over its neighbours.
    pub fn show(
        &self,
        ui: &mut Ui,
        id: Id,
        state: &mut RowState,
        len: usize,
        key: impl Fn(usize) -> u64,
        mut draw: impl FnMut(&mut Ui, &TileSlot),
    ) -> RowResponse {
        let mut out = RowResponse { height: self.height(), ..RowResponse::default() };
        if let Some(h) = self.header {
            let width = theme::CANVAS_W - self.x - theme::SAFE_X;
            ui.text(Point::new(self.x, self.y), h, &theme::SECTION, &pley_base::TextOpts::line(width), theme::FG);
        }
        if len == 0 {
            state.drawn = (0, 0);
            return out;
        }
        let pitch = self.kind.pitch();
        let size = self.kind.size();
        let max = self.max_offset(len);

        // Which tile has focus, among those drawn last frame (and a few around them, in case the
        // list changed under it).
        let mut focused_now = false;
        if let Some(f) = ui.focused().filter(|_| ui.focused_in(id).is_some())
            && let Some(i) = find_key(state.drawn, len, |i| id.with(key(i)) == f)
        {
            state.index = i;
            focused_now = true;
        }
        state.index = state.index.min(len - 1);
        let lo = self.x + state.index as f32 * pitch;
        if !state.started {
            state.offset = self.offset_for(state.index, len);
            state.started = true;
        } else if focused_now && ui.pointer_mode() {
            state.offset = nudge(state.offset, lo, lo + size.w, POINTER_VIEW.0, POINTER_VIEW.1, 0.0, max);
        } else if focused_now {
            state.offset = self.offset_for(state.index, len);
        }
        state.offset = state.offset.clamp(0.0, max);
        let shown = ui.animate(id.with("kit.scroll"), state.offset, SCROLL);
        state.shown = shown;

        // On screen: the tiles crossing 0..960; one more either side so the next move has a target.
        let first = (((shown - self.x - size.w) / pitch).floor() + 1.0).max(0.0) as usize;
        let past = (((shown + theme::CANVAS_W - self.x) / pitch).ceil().max(0.0) as usize).min(len);
        let first = first.min(past);
        // The focused tile and its neighbours are drawn even while the row is still sliding
        // toward them, so the next press always has somewhere to go.
        let d0 = first.saturating_sub(1).min(state.index.saturating_sub(1));
        let d1 = (past + 1).max(state.index + 2).min(len);
        let per_screen = (theme::CANVAS_W / pitch).ceil() as usize;
        let ahead = (d1 + (per_screen as f32 * 1.5).ceil() as usize).min(len);
        state.drawn = (d0, d1);
        out.visible = (first, past);

        let y = self.tiles_y();
        let opts = if self.long_press { FocusOpts::default().long_press() } else { FocusOpts::default() };
        let group = Group::row().edge(Dir::Left, self.left).edge(Dir::Right, self.right);
        let mut slots: Vec<TileSlot> = Vec::with_capacity(d1 - d0);
        ui.group(id, group, |ui| {
            for i in d0..d1 {
                let tile_id = id.with(key(i));
                let rect = Rect::new(self.x + i as f32 * pitch - shown, y, size.w, size.h);
                let response = register(ui, tile_id, rect, opts);
                let priority = if i >= first && i < past { Priority::Visible } else { Priority::Near };
                slots.push(TileSlot {
                    index: i,
                    id: tile_id,
                    kind: self.kind,
                    rect,
                    response,
                    mode: SlotMode::Draw,
                    priority,
                });
            }
        });
        let mut focused_slot = None;
        for slot in &slots {
            note(&mut out, slot.index, &slot.response);
            if slot.response.focused {
                focused_slot = Some(*slot);
                continue;
            }
            draw(ui, slot);
        }
        if let Some(slot) = focused_slot {
            draw(ui, &slot);
        }
        for i in d1..ahead {
            let tile_id = id.with(key(i));
            let rect = Rect::new(self.x + i as f32 * pitch - shown, y, size.w, size.h);
            let slot = TileSlot {
                index: i,
                id: tile_id,
                kind: self.kind,
                rect,
                response: crate::look::unregistered(tile_id),
                mode: SlotMode::Prefetch,
                priority: Priority::Prefetch,
            };
            draw(ui, &slot);
        }
        out
    }
}

/// The index `is` picks: first among those drawn last frame (and two either side, should the list
/// have changed under them), then anywhere.
pub(crate) fn find_key(drawn: (usize, usize), len: usize, is: impl Fn(usize) -> bool) -> Option<usize> {
    let lo = drawn.0.saturating_sub(2).min(len);
    let hi = (drawn.1 + 2).min(len);
    (lo..hi).find(|&i| is(i)).or_else(|| (0..len).find(|&i| is(i)))
}

/// Folds one tile's answer into the row's.
pub(crate) fn note(out: &mut RowResponse, index: usize, r: &Response) {
    if r.focused {
        out.focused = Some(index);
        out.gained |= r.gained;
    }
    if r.activated {
        out.activated = Some(index);
    }
    if r.long_pressed || r.menu {
        out.actions = Some(index);
    }
}
