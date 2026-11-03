//! What the three pages share: picture addresses, the page's message for a title or person that
//! can't be shown, still blocks while loading, and changes made on the server.

use pley_base::{Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::model::{ItemKind, ItemSummary};
use pley_core::{Api, ApiError, Promise};
use pley_kit::Button;
use pley_shell::{AppCx, PlayRequest, Route};
use pley_ui::theme::{self, radius};
use pley_ui::{Id, Response, Ui};

/// Which of an item's pictures a place shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArtKind {
    Poster,
    /// 16:9 stills: episodes, extras, Continue Watching.
    Thumb,
    Backdrop,
}

/// The server's addresses for an item's picture, first choice first, falling back on its season's
/// and show's (the TV app's `artCandidates`, `kit/art.tsx`).
pub fn art_paths(item: &ItemSummary, kind: ArtKind) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |p: Option<String>| {
        if let Some(p) = p.filter(|p| !p.is_empty())
            && !out.contains(&p)
        {
            out.push(p);
        }
    };
    let of = |id: Option<i64>, which: &str| id.map(|id| format!("/api/items/{id}/images/{which}"));
    match kind {
        ArtKind::Poster => {
            add(item.poster_url.clone());
            add(of(item.parent_id, "poster"));
            add(of(item.grandparent_id, "poster"));
        }
        ArtKind::Thumb => {
            if item.kind == ItemKind::Movie {
                add(item.backdrop_url.clone());
            }
            add(item.thumb_url.clone());
            if item.kind == ItemKind::Episode {
                add(of(item.grandparent_id, "backdrop"));
            } else {
                add(item.backdrop_url.clone());
            }
        }
        ArtKind::Backdrop => {
            add(item.backdrop_url.clone());
            add(of(item.grandparent_id, "backdrop"));
            add(of(item.parent_id, "backdrop"));
        }
    }
    out
}

/// The requests for `paths` at `width_px` device pixels.
pub fn requests(cx: &AppCx, paths: &[String], width_px: u32) -> Vec<ImageRequest> {
    paths.iter().filter_map(|p| cx.image(p, width_px)).collect()
}

/// The page an item opens: its own page, an artist's or album's, a track's album (`pageOf`).
pub fn page_of(item: &ItemSummary) -> Route {
    match item.kind {
        ItemKind::Artist => Route::Artist { id: item.id },
        ItemKind::Album => Route::Album { id: item.id, track_id: None },
        ItemKind::Track => match item.parent_id {
            Some(album) => Route::Album { id: album, track_id: Some(item.id) },
            None => Route::Item { id: item.id },
        },
        _ => Route::Item { id: item.id },
    }
}

/// What a title's page shows instead of the title.
#[derive(Clone, Debug, PartialEq)]
pub enum Elsewhere {
    /// Another page takes its place.
    Page(Route),
    /// "Item not found": nothing can show it.
    NotFound,
}

/// What a title's page gives way to (`ItemViewModel.pageStates`): music has its own pages, an
/// extra shows its movie's or show's; a track or extra without a parent, or a kind the app doesn't
/// know, isn't found.
pub fn elsewhere(item: &ItemSummary) -> Option<Elsewhere> {
    let route = match item.kind {
        ItemKind::Artist | ItemKind::Album => Some(page_of(item)),
        ItemKind::Track => item.parent_id.map(|album| Route::Album { id: album, track_id: Some(item.id) }),
        ItemKind::Extra => item.parent_id.map(|id| Route::Item { id }),
        ItemKind::Unknown => None,
        _ => return None,
    };
    Some(route.map_or(Elsewhere::NotFound, Elsewhere::Page))
}

/// A tile's words, pictures and marks, made once when the answer comes (not every frame).
#[derive(Clone, Debug, PartialEq)]
pub struct TileData {
    pub id: i64,
    pub title: String,
    pub subtitle: String,
    pub art: Vec<ImageRequest>,
    pub progress: Option<f32>,
    pub watched: bool,
    pub unwatched: u32,
    /// The title it shows, for its actions and for playing it.
    pub item: Option<ItemSummary>,
}

impl TileData {
    /// A tile of `item` for a well `width` dp wide, with `paths` as its pictures.
    pub fn of(cx: &AppCx, ui: &Ui, item: &ItemSummary, kind: ArtKind, width: f32, subtitle: String) -> TileData {
        let progress = crate::page::in_progress(item)
            .then(|| pley_core::format::progress_fraction(item.user_state.position_ms, item.duration_ms));
        TileData {
            id: item.id,
            title: item.title.clone(),
            subtitle,
            art: requests(cx, &art_paths(item, kind), ui.image_px(width)),
            progress,
            watched: crate::page::watched(item),
            unwatched: pley_core::format::unwatched_count(item).clamp(0, u32::MAX as i64) as u32,
            item: Some(item.clone()),
        }
    }

    /// The kit's tile for it.
    pub fn tile(&self, kind: pley_kit::TileKind) -> pley_kit::Tile<'_> {
        pley_kit::Tile::new(kind, &self.title)
            .subtitle(&self.subtitle)
            .art(&self.art)
            .progress(self.progress)
            .watched(self.watched)
            .unwatched(self.unwatched)
    }
}

/// The toast when a title has nothing to play.
pub const NOTHING_TO_PLAY: &str = "Nothing to play: this item has no media files.";

/// Plays a video from its start, or where it was left.
pub fn play_request(id: i64, from_start: bool) -> PlayRequest {
    PlayRequest { from_start, ..PlayRequest::item(id) }
}

/// A title shown at "Can't reach your server" when the server is out of reach, else `failed`.
pub fn failed_title(error: &ApiError, failed: &'static str) -> &'static str {
    if error.unreachable() { "Can't reach your server" } else { failed }
}

/// A page's message (`TvMessage` in `TvItemParts.kt`): vertically centred at x 96, a 40 dp icon in
/// Muted, 16, the title in `Tv.PageTitle`, 8, the text in `Tv.Body` Muted (at most 464 wide), 24,
/// a pill (without an icon) that takes focus. Returns the pill's answer.
pub fn message(ui: &mut Ui, id: Id, icon: Icon, title: &str, body: &str, action: &str) -> Response {
    let width = 464.0;
    let title_h = ui.measure(title, &theme::PAGE_TITLE, &TextOpts::wrap(width, 2).balanced()).h;
    let body_h = if body.is_empty() { 0.0 } else { 8.0 + ui.measure(body, &theme::BODY, &TextOpts::wrap(width, 4)).h };
    let total = 40.0 + 16.0 + title_h + body_h + 24.0 + theme::BUTTON_HEIGHT;
    let x = theme::GUTTER;
    let mut y = ((theme::CANVAS_H - total) / 2.0).max(theme::SAFE_Y);
    ui.icon(Rect::new(x, y, 40.0, 40.0), icon, theme::MUTED);
    y += 56.0;
    ui.text(Point::new(x, y), title, &theme::PAGE_TITLE, &TextOpts::wrap(width, 2).balanced(), theme::FG);
    y += title_h;
    if !body.is_empty() {
        ui.text(Point::new(x, y + 8.0), body, &theme::BODY, &TextOpts::wrap(width, 4), theme::MUTED);
        y += body_h;
    }
    y += 24.0;
    ui.group(id, pley_kit::bar_group(), |ui| {
        let pill = ui.id("action");
        ui.initial_focus(pill);
        Button::pill(action).show(ui, pill, Point::new(x, y))
    })
}

/// A still block of what is coming (`Block` in `TvItemScreen.kt`): Surface2, 2 dp corners.
pub fn block(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32) {
    ui.fill(Rect::new(x, y, w, h), theme::SURFACE2, radius::BADGE);
}

/// A change asked of the server for a title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    /// Watched (a show or season: every episode) or not.
    Watched { id: i64, watched: bool },
    /// In My List or not.
    MyList { id: i64, on: bool },
    /// Out of Continue Watching (an episode takes its show with it).
    HideFromContinueWatching { id: i64 },
}

impl Change {
    /// Asks the server.
    pub fn run(self, api: &Api) -> Result<(), ApiError> {
        match self {
            Change::Watched { id, watched } => api.set_watched(id, watched),
            Change::MyList { id, on } => api.set_in_my_list(id, on),
            Change::HideFromContinueWatching { id } => api.hide_from_continue_watching(id),
        }
    }

    /// Starts it on the task pool; the page that asked keeps the answer.
    pub fn start(self, cx: &AppCx) -> Promise<Result<(), ApiError>> {
        cx.spawn(move |api| self.run(api))
    }

    /// Starts it so that it carries on after the page that asked has gone (the actions page closes
    /// before its change is made, as on Android). Home follows through the server's events.
    pub fn detach(self, cx: &AppCx) {
        let Some(api) = cx.api().cloned() else { return };
        cx.tasks().detach(move || {
            if let Err(e) = self.run(&api) {
                log::warn!("{self:?} refused: {e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_fall_back_on_the_show() {
        let episode = ItemSummary {
            id: 5,
            kind: ItemKind::Episode,
            parent_id: Some(4),
            grandparent_id: Some(3),
            thumb_url: Some("/api/items/5/images/thumb?v=1".into()),
            ..ItemSummary::default()
        };
        assert_eq!(
            art_paths(&episode, ArtKind::Thumb),
            ["/api/items/5/images/thumb?v=1", "/api/items/3/images/backdrop"]
        );
        assert_eq!(
            art_paths(&episode, ArtKind::Backdrop),
            ["/api/items/3/images/backdrop", "/api/items/4/images/backdrop"]
        );
        assert_eq!(art_paths(&episode, ArtKind::Poster), ["/api/items/4/images/poster", "/api/items/3/images/poster"]);
    }

    #[test]
    fn music_and_extras_have_pages_of_their_own() {
        let item = |kind, parent| ItemSummary { id: 9, kind, parent_id: parent, ..ItemSummary::default() };
        let page = |route| Some(Elsewhere::Page(route));
        assert_eq!(elsewhere(&item(ItemKind::Artist, None)), page(Route::Artist { id: 9 }));
        assert_eq!(elsewhere(&item(ItemKind::Track, Some(8))), page(Route::Album { id: 8, track_id: Some(9) }));
        assert_eq!(elsewhere(&item(ItemKind::Extra, Some(2))), page(Route::Item { id: 2 }));
        assert_eq!(elsewhere(&item(ItemKind::Movie, None)), None);
        assert_eq!(page_of(&item(ItemKind::Album, None)), Route::Album { id: 9, track_id: None });
    }

    #[test]
    fn a_track_or_extra_without_a_parent_isnt_found() {
        // Its page would otherwise open itself again and again.
        let item = |kind| ItemSummary { id: 9, kind, ..ItemSummary::default() };
        for kind in [ItemKind::Track, ItemKind::Extra, ItemKind::Unknown] {
            assert_eq!(elsewhere(&item(kind)), Some(Elsewhere::NotFound), "{kind:?}");
        }
    }
}
