//! Playlists (`TvPlaylistsScreen.kt`, `TvPlaylistParts.kt`; the TV app's `PlaylistsScreen` in
//! `playlists.tsx`): "Playlists" at (96, 27) with New playlist at the right safe edge, then every
//! playlist as a square cover five across, its wells from y 95 (the header's 40 and 28 under it),
//! captioned with its title and "12 items · 3h 12m" (and "Smart"). Focus starts on the first
//! cover, or New playlist when there are none, and comes back to the cover left.

use pley_base::{Cmd, Icon, ImageRequest, Point, Rect, Size, TextOpts};
use pley_core::format::{format_duration, pluralize};
use pley_core::model::{PlaylistKind, PlaylistList, PlaylistSummary};
use pley_core::store::snapshot_keys;
use pley_kit::{Button, GridState, PosterGrid, Tile, TileKind, skeleton_row};
use pley_shell::{AppCx, AppEvent, Chrome, Load, Resource, Route, Screen};
use pley_ui::theme;
use pley_ui::{Dir, Edge, Group, Id, ImageOpts, Ui};

use crate::parts::{column_message, failed_title};

/// The header row: the title and New playlist, 40 tall from the safe line.
const HEADER_Y: f32 = theme::SAFE_Y;
const HEADER_H: f32 = theme::BUTTON_HEIGHT;
/// The grid's box: its wells 28 under the header (y 95).
const GRID_TOP: f32 = HEADER_Y + HEADER_H + 28.0 - theme::GRID_CONTENT_TOP;
/// Where the page's states start: 40 under the header.
const STATE_TOP: f32 = HEADER_Y + HEADER_H + 40.0;
/// The empty cover's glyph.
const GLYPH: f32 = 48.0;

/// "12 items · 3h 12m", "1 item", "0 items" (`Playlists.meta`).
pub fn playlist_meta(playlist: &PlaylistSummary) -> String {
    let count = pluralize(playlist.item_count, "item", None);
    let duration = format_duration(Some(playlist.duration_ms));
    [count, duration].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// A playlist as its cover and the sheets show it: its title, its line, its pictures.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: i64,
    pub title: String,
    /// "12 items · 3h 12m".
    pub meta: String,
    /// The meta with " · Smart" for a smart playlist: the cover's second line.
    pub caption: String,
    pub video: bool,
    pub smart: bool,
    /// The first entries' pictures at the cover's width, then at a quarter's.
    pub art: Vec<ImageRequest>,
    pub quarters: Vec<ImageRequest>,
}

impl Card {
    /// `playlist` with its pictures asked at `px` (the whole cover) and `quarter_px` (a mosaic's
    /// quarter) device pixels.
    pub fn new(cx: &AppCx, playlist: &PlaylistSummary, px: u32, quarter_px: u32) -> Card {
        let meta = playlist_meta(playlist);
        let smart = playlist.smart();
        let caption = if smart {
            [meta.as_str(), "Smart"].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
        } else {
            meta.clone()
        };
        let ask = |w: u32| -> Vec<ImageRequest> {
            match cx.api() {
                Some(api) => playlist.artwork.iter().take(4).map(|p| api.image(p, w)).collect(),
                None => Vec::new(),
            }
        };
        Card {
            id: playlist.id,
            title: playlist.title.clone(),
            meta,
            caption,
            video: playlist.kind != PlaylistKind::Audio,
            smart,
            art: ask(px),
            quarters: ask(quarter_px),
        }
    }
}

/// A playlist's square cover (`TvPlaylistArt`): a 2 × 2 mosaic of its first four entries'
/// pictures, else its first picture (the title placeholder under it), else its kind's glyph
/// (`glyph` wide, Subtle) on a Surface3 to Surface2 gradient; with the tile's hairline.
pub fn cover(ui: &mut Ui, rect: Rect, card: &Card, corner: f32, glyph: f32) {
    if card.quarters.len() >= 4 {
        ui.fill(rect, theme::SURFACE2, corner);
        let (w, h) = (rect.w / 2.0, rect.h / 2.0);
        for (i, req) in card.quarters.iter().take(4).enumerate() {
            let (c, r) = ((i % 2) as f32, (i / 2) as f32);
            // Each quarter reaches past the middle by the corner's radius, so its rounded inner
            // corners fall under its neighbours and only the cover's own corners show round.
            let x0 = rect.x + c * w - c * corner;
            let y0 = rect.y + r * h - r * corner;
            let quarter = Rect::new(x0, y0, w + corner, h + corner);
            ui.image(quarter, req, &ImageOpts::cover().rounded(corner));
        }
    } else if !card.art.is_empty() {
        let shown = pley_kit::first_art(ui, &card.art, pley_base::Priority::Visible);
        if shown.is_some_and(|s| s.opaque()) {
            ui.fill(rect, theme::SURFACE2, corner);
        } else {
            pley_kit::placeholder(ui, rect, &card.title, &card.title, corner);
        }
        if let Some(s) = shown {
            ui.draw_image(rect, &s, &ImageOpts::cover().rounded(corner));
        }
    } else {
        let (a, b) = (theme::SURFACE3, theme::SURFACE2);
        let mid = a.lerp(b, 0.5);
        ui.cmd(Cmd::Quad { rect, colors: [a, mid, mid, b], radius: corner });
        let icon = if card.video { Icon::ListVideo } else { Icon::ListMusic };
        let at = Rect::new(rect.center().x - glyph / 2.0, rect.center().y - glyph / 2.0, glyph, glyph);
        ui.icon(at, icon, theme::SUBTLE);
    }
    ui.stroke(rect.expand(-0.5), theme::TILE_HAIRLINE, corner - 0.5, 1.0);
}

/// The rail's Playlists page.
pub struct PlaylistsScreen {
    list: Resource<PlaylistList>,
    cards: Vec<Card>,
    grid: GridState,
}

impl Default for PlaylistsScreen {
    fn default() -> Self {
        PlaylistsScreen::new()
    }
}

impl PlaylistsScreen {
    /// The page.
    pub fn new() -> PlaylistsScreen {
        PlaylistsScreen {
            list: Resource::new(Some(snapshot_keys::PLAYLISTS.to_string()), |api| api.playlists()),
            cards: Vec::new(),
            grid: GridState::default(),
        }
    }

    /// Draws "Playlists" and, unless nothing could be had, New playlist (which can't take focus
    /// until the list is known, so the first cover gets it); whether it was chosen.
    fn header(&self, ui: &mut Ui, with_button: bool, live: bool, initial: bool) -> bool {
        let title_y = HEADER_Y + (HEADER_H - theme::PAGE_TITLE.line_height) / 2.0;
        let pill = Button::pill("New playlist").icon(Icon::Plus);
        let pill_w = pill.size(ui).w;
        let right = theme::CANVAS_W - theme::SAFE_X;
        let title_w = right - pill_w - 16.0 - theme::GUTTER;
        ui.text(
            Point::new(theme::GUTTER, title_y),
            "Playlists",
            &theme::PAGE_TITLE,
            &TextOpts::line(title_w),
            theme::FG,
        );
        if !with_button {
            return false;
        }
        let at = Point::new(right - pill_w, HEADER_Y);
        if !live {
            pley_kit::inert(ui, |ui| pill.show(ui, Id::new("new"), at));
            return false;
        }
        let group = Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop);
        ui.group(ui.id("new-group"), group, |ui| {
            let id = ui.id("new");
            if initial {
                ui.initial_focus(id);
            }
            pill.show(ui, id, at).activated
        })
    }
}

impl Screen for PlaylistsScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        if self.list.update(cx) {
            let px = ui.image_px(TileKind::Square.size().w);
            let quarter = ui.image_px(TileKind::Square.size().w / 2.0);
            let playlists = self.list.value().map(|l| l.playlists.clone()).unwrap_or_default();
            self.cards = playlists.iter().map(|p| Card::new(cx, p, px, quarter)).collect();
        }
        let failed = matches!(self.list.state(), Load::Failed { stale: None, .. });
        let known = self.list.value().is_some();
        let none = known && self.cards.is_empty();
        if self.header(ui, !failed, known, none) {
            cx.push(Route::NewPlaylist { item_ids: Vec::new(), smart: None });
        }
        match self.list.state() {
            Load::Loading => {
                skeleton_row(ui, GRID_TOP + theme::GRID_CONTENT_TOP, Size::ZERO, 0.0, TileKind::Square, 5);
                return;
            }
            Load::Failed { error, stale: None } => {
                let title = failed_title(error, "Couldn't load your playlists");
                let problem = error.problem();
                let r = column_message(ui, ui.id("state"), STATE_TOP, title, &problem, Some("Retry"));
                if r.is_some_and(|r| r.activated) {
                    self.list.invalidate();
                }
                return;
            }
            _ => {}
        }
        if none {
            let body = "Make one here, or use “Add to playlist” on a movie, show, album or episode (also when \
                        holding OK on a poster).";
            column_message(ui, ui.id("state"), STATE_TOP, "No playlists yet", body, None);
            return;
        }
        let grid_id = ui.id("grid");
        ui.initial_focus(grid_id.with(self.grid.index.min(self.cards.len() - 1) as u64));
        let cards = &self.cards;
        let r = PosterGrid::new(TileKind::Square, GRID_TOP).long_press(false).show(
            ui,
            grid_id,
            &mut self.grid,
            cards.len(),
            |ui, slot| {
                let card = &cards[slot.index];
                let draw = |ui: &mut Ui, rect: Rect| cover(ui, rect, card, TileKind::Square.radius(), GLYPH);
                Tile::new(TileKind::Square, &card.title).subtitle(&card.caption).custom(&draw).paint(ui, slot);
            },
        );
        if let Some(i) = r.activated {
            cx.push(Route::Playlist { id: cards[i].id });
        }
    }

    fn event(&mut self, event: &AppEvent, _cx: &mut AppCx) {
        // Playlists change from other pages (made, renamed, emptied, deleted) without an event from
        // the server: the page asks again whenever it shows again.
        if matches!(event, AppEvent::Changed | AppEvent::Shown) {
            self.list.invalidate();
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.list.value().is_some() || self.list.error().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playlist_reads_as_on_android() {
        let p = PlaylistSummary { item_count: 12, duration_ms: 11_520_000, ..PlaylistSummary::default() };
        assert_eq!(playlist_meta(&p), "12 items · 3h 12m");
        let one = PlaylistSummary { item_count: 1, duration_ms: 0, ..PlaylistSummary::default() };
        assert_eq!(playlist_meta(&one), "1 item");
        assert_eq!(playlist_meta(&PlaylistSummary::default()), "0 items");
    }
}
