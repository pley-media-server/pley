//! The channels as cards (`ChannelTiles`, `ChannelTileCard` in `TvLiveTvScreen.kt`): three across
//! beside the categories, a page of channels loaded at a time as they come near. A card is the
//! channel's logo in its well with its number at the top left, a star for a favourite and how far
//! what's on is along its foot, captioned with its name and what's on. OK watches; holding OK,
//! Menu or Info opens the details; Left from the first column goes to the category shown, on a
//! press of its own rather than a held key.

use pley_base::{Icon, Key, Point, Rect};
use pley_core::model::LiveChannel;
use pley_kit::{Badge, BadgeStyle, GridState, PosterGrid, Tile, TileKind, TileSlot};
use pley_player::live::data::channel_name;
use pley_player::live::{Category, ChannelPages};
use pley_shell::AppCx;
use pley_ui::theme;
use pley_ui::{Dir, Dirs, Edge, Id, Ui};

use crate::draw::{Well, channel_tile, progress};
use crate::lineup::Lineup;

/// Rows of channels loaded past those on screen (`MARGIN`), as cells.
const MARGIN: usize = 20 * COLUMNS;
/// Cards across.
pub const COLUMNS: usize = 3;
/// The wells' top under the content's top.
const WELLS_DOWN: f32 = 8.0;
/// Cards are 18 apart across (`ChannelTiles`); the kit's grid sets them 14 apart, so each column is
/// drawn this much further right per column before it.
const WIDER: f32 = 18.0 - theme::GRID_COLUMN_GAP;

/// What the cards ask of the page.
#[derive(Clone, Debug, PartialEq)]
pub enum CardAction {
    Watch(LiveChannel),
    Details(LiveChannel),
    /// Left from the first column: to the categories.
    Leave,
}

/// What the cards did this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardsOut {
    pub action: Option<CardAction>,
    /// The cells to load: those on screen and a margin either side.
    pub wanted: (usize, usize),
    pub focused: bool,
}

/// The cards' grid as it was left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cards {
    pub grid: GridState,
    /// Whether a card had focus last frame.
    pub focused: bool,
}

impl Cards {
    /// The id the card at `index` is registered as.
    pub fn cell_id(id: Id, index: usize) -> Id {
        id.with(index as u64)
    }

    /// Opens on card `index` (the channel watched last).
    pub fn land(&mut self, index: usize) {
        self.grid.focus_to(index);
    }

    /// Draws the cards with their left edge at `x` under `top`.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        cx: &AppCx,
        id: Id,
        x: f32,
        top: f32,
        pages: Lineup,
        category: &Category,
        now: i64,
    ) -> CardsOut {
        let mut out = CardsOut::default();
        let len = pages.total().unwrap_or(0).max(0) as usize;
        // The kit's grid lays its columns out from the gutter: moved across to the cards' place.
        let grid = PosterGrid::new(TileKind::Episode, top + WELLS_DOWN - theme::GRID_CONTENT_TOP)
            .columns(COLUMNS)
            .left_edge(Edge::Stop)
            // A held Up stops at the top row; the tabs take a press of its own.
            .held_stop(Dirs::UP);
        let r = ui.translate(Point::new(x - theme::GUTTER, 0.0), |ui| {
            grid.show(ui, id, &mut self.grid, len, |ui, slot| {
                let column = (slot.index % COLUMNS) as f32;
                let slot = &TileSlot { rect: slot.rect.translate(column * WIDER, 0.0), ..*slot };
                match pages.at(slot.index as i64) {
                    Some(c) => card(ui, cx, slot, c, category, now),
                    None => Tile::skeleton(TileKind::Episode).paint(ui, slot),
                }
            })
        });
        out.focused = r.focused.is_some();
        self.focused = out.focused;
        out.wanted = (r.visible.0.saturating_sub(MARGIN), (r.visible.1 + MARGIN).min(len));
        // Info, as it is let go, opens what's on as holding OK and Menu do.
        let info = r.focused.filter(|_| ui.keys().iter().any(|k| k.key == Key::Info && !k.pressed));
        if let Some(c) = r.activated.and_then(|i| pages.at(i as i64)) {
            out.action = Some(CardAction::Watch(c.clone()));
        } else if let Some(c) = r.actions.or(info).and_then(|i| pages.at(i as i64)) {
            out.action = Some(CardAction::Details(c.clone()));
        } else if ui.edge_hit(id) == Some(Dir::Left) && !ui.holding() {
            // Out to the categories on a press of its own, not under a held key.
            out.action = Some(CardAction::Leave);
        }
        out
    }
}

/// What a card's caption says under the name: what's on, or why nothing is.
pub fn whats_on(c: &LiveChannel, now: i64) -> String {
    match c.on_at(now) {
        Some(p) => p.title.clone(),
        None if c.has_guide => "Nothing in the guide now".into(),
        None => "No guide for this channel".into(),
    }
}

fn card(ui: &mut Ui, cx: &AppCx, slot: &TileSlot, c: &LiveChannel, category: &Category, now: i64) {
    let name = channel_name(c, Some(category));
    let on = whats_on(c, now);
    let well = |ui: &mut Ui, rect: Rect| card_well(ui, cx, rect, c, now);
    Tile::new(TileKind::Episode, &name).subtitle(&on).custom(&well).paint(ui, slot);
}

/// A card's well: the logo (or the name on a gradient), the number, the favourite's star and how
/// far what's on is.
fn card_well(ui: &mut Ui, cx: &AppCx, rect: Rect, c: &LiveChannel, now: i64) {
    channel_tile(ui, cx, rect, c, Well::PAGE.named().styled(theme::TITLE));
    if let Some(n) = c.number.as_deref().filter(|n| !n.is_empty()) {
        Badge::new(n, BadgeStyle::Number).show(ui, Point::new(rect.x + 6.0, rect.y + 6.0));
    }
    if c.favorite {
        ui.icon(Rect::new(rect.right() - 6.0 - 16.0, rect.y + 6.0, 16.0, 16.0), Icon::Star, theme::ACCENT);
    }
    if let Some(p) = c.on_at(now) {
        let bar = Rect::new(rect.x, rect.bottom() - 3.0, rect.w, 3.0);
        progress(ui, bar, p, now, theme::PROGRESS_TRACK_ON_ART, theme::ACCENT);
    }
}

/// Loads the channels the cards want.
pub fn want(pages: &mut ChannelPages, wanted: (usize, usize), now_ms: i64) {
    pages.want(wanted.0 as i64, wanted.1.max(wanted.0 + 1) as i64 - 1, now_ms);
}
