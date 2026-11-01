//! An artist (`TvArtistScreen.kt`; the TV app's `ArtistScreen`): their backdrop, the round
//! picture, "Artist", the name, "4 albums · 40 tracks  ·  Rock · Pop" and the summary; Play,
//! Shuffle and, while music plays, Now playing; then their albums, newest first, as a row of
//! square tiles. Play and Shuffle ask for the albums and all their tracks afresh (the albums'
//! tracks together, as Android's `CachedPlayItem` does) and start the music as soon as they come,
//! whatever page shows by then; Now Playing opens once this page shows. Focus starts on Play;
//! Down goes to the albums. The remote's media keys work here while music has a queue.

use std::sync::Arc;

use pley_base::{Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::model::{ItemDetail, ItemList, ItemSummary};
use pley_core::store::snapshot_keys;
use pley_kit::{
    BackdropArt, BackdropScrim, BackdropState, Button, RoundButton, Row, RowState, Tile, TileKind, backdrop_px,
    bar_group, skeleton_block,
};
use pley_shell::{AppCx, AppEvent, Chrome, Load, Resource, Route, Screen};
use pley_ui::Ui;
use pley_ui::theme::{self, radius};

use crate::pages::{self, NOT_FOUND_BODY, NOTHING_TO_PLAY, UNREACHABLE_TITLE};
use crate::player::{self, Gathered};
use crate::widgets::{ListState, art_well, list_failure, list_state, message};

/// The header's top, the picture's side and the gap after it (`TvArtistScreen.kt`).
const TOP: f32 = 40.0;
const PICTURE: f32 = 120.0;
const PICTURE_GAP: f32 = 28.0;
/// The widest the name and its lines get.
const COLUMN_MAX: f32 = 520.0;
/// From the header to the buttons, and from the buttons' row (with its labels) to the albums.
const HEADER_TO_BUTTONS: f32 = 18.0;
const BUTTONS_TO_ALBUMS: f32 = 4.0;
/// Nothing shows for this long while the page loads, then its skeleton.
const SKELETON_AFTER: f64 = 0.3;

/// The pictures the page draws, asked for at the screen's pixels.
#[derive(Default)]
struct Art {
    px: (u32, u32, u32),
    backdrop: Option<BackdropArt>,
    picture: Vec<ImageRequest>,
    albums: Vec<Vec<ImageRequest>>,
}

/// An artist's page.
pub struct ArtistScreen {
    id: i64,
    detail: Resource<ItemDetail>,
    children: Resource<ItemList>,
    albums: Vec<ItemSummary>,
    years: Vec<String>,
    /// The line under the name, made when an answer comes.
    meta: String,
    art: Option<Art>,
    backdrop: BackdropState,
    row: RowState,
    /// Play or Shuffle on its way: the music player's ticket for it.
    starting: Option<u64>,
    /// Held while the page is on the stack: a Play still on its way goes with the page.
    alive: Arc<()>,
    problem: Option<String>,
    since: Option<f64>,
}

impl ArtistScreen {
    /// The page of artist `id`.
    pub fn new(id: i64) -> ArtistScreen {
        ArtistScreen {
            id,
            detail: Resource::new(Some(snapshot_keys::item(id)), move |api| api.item(id)),
            children: Resource::new(Some(snapshot_keys::children(id)), move |api| api.children(id)),
            albums: Vec::new(),
            years: Vec::new(),
            meta: String::new(),
            art: None,
            backdrop: BackdropState::default(),
            row: RowState::default(),
            starting: None,
            alive: Arc::new(()),
            problem: None,
            since: None,
        }
    }

    fn update(&mut self, cx: &mut AppCx) {
        let mut changed = self.detail.update(cx);
        if self.children.update(cx) {
            self.albums = pages::sorted_albums(self.children.value().map_or(&[], |l| &l.items));
            self.years = self.albums.iter().map(|a| a.year.map(|y| y.to_string()).unwrap_or_default()).collect();
            changed = true;
        }
        if changed {
            self.art = None;
            let albums = self.children.value().map(|_| self.albums.len());
            self.meta = self.detail.value().map(|d| pages::artist_meta(d, albums)).unwrap_or_default();
        }
        // The music player started (or not) on what came, whatever page showed then; Now Playing
        // opens once this page shows, as Android's event waits for its screen.
        if let Some(outcome) = self.starting.and_then(|ticket| player::gathered(cx, ticket)) {
            self.starting = None;
            match outcome {
                Gathered::Played => cx.push(Route::NowPlaying),
                Gathered::Nothing => self.problem = Some(NOTHING_TO_PLAY.into()),
                Gathered::Failed(why) => self.problem = Some(why),
                Gathered::Dropped => {}
            }
        }
    }

    /// Play or Shuffle: the albums and their tracks, asked for afresh by the music player.
    fn start(&mut self, cx: &mut AppCx, shuffle: bool) {
        if self.starting.is_some() {
            return;
        }
        self.problem = None;
        self.starting = player::gather(cx, self.id, shuffle, &self.alive);
    }

    fn pictures(&mut self, ui: &Ui, cx: &AppCx) {
        let px = (backdrop_px(ui), ui.image_px(PICTURE), ui.image_px(TileKind::Square.size().w));
        if self.art.as_ref().is_some_and(|a| a.px == px) {
            return;
        }
        let Some(detail) = self.detail.value() else { return };
        let one = |path: &Option<String>, w: u32| path.as_deref().and_then(|p| cx.image(p, w)).into_iter().collect();
        let backdrop: Vec<ImageRequest> = one(&detail.backdrop_url, px.0);
        self.art = Some(Art {
            px,
            backdrop: (!backdrop.is_empty()).then(|| BackdropArt::new(self.id as u64, backdrop, detail.title.clone())),
            picture: one(&detail.poster_url, px.1),
            albums: self.albums.iter().map(|a| one(&a.poster_url, px.2)).collect(),
        });
    }

    fn skeleton(&self, ui: &mut Ui, now: f64) {
        let since = self.since.unwrap_or(now);
        if now - since < SKELETON_AFTER {
            ui.repaint_after(since + SKELETON_AFTER - now);
            return;
        }
        // `ArtistSkeleton`: the circle, and two bars centred beside it, 10 apart.
        skeleton_block(ui, Rect::new(theme::GUTTER, TOP, PICTURE, PICTURE), PICTURE / 2.0);
        let x = theme::GUTTER + PICTURE + PICTURE_GAP;
        let y = TOP + (PICTURE - 64.0) / 2.0;
        skeleton_block(ui, Rect::new(x, y, 280.0, 36.0), radius::BADGE);
        skeleton_block(ui, Rect::new(x, y + 46.0, 200.0, 18.0), radius::BADGE);
    }

    fn content(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.pictures(ui, cx);
        let Some(detail) = self.detail.value() else { return };
        let art = self.art.take().unwrap_or_default();
        pley_kit::Backdrop::new(BackdropScrim::Item).show(
            ui,
            ui.id("backdrop"),
            &mut self.backdrop,
            art.backdrop.as_ref(),
        );

        // The header: the picture and a column beside it, centred on each other.
        let x = theme::GUTTER + PICTURE + PICTURE_GAP;
        let width = COLUMN_MAX.min(theme::CANVAS_W - theme::SAFE_X - x);
        let name = ui.layout(&detail.title, &theme::PAGE_TITLE, &TextOpts::wrap(width, 2).balanced());
        let meta = &self.meta;
        let summary = detail
            .summary
            .summary
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(|s| ui.layout(s, &theme::BODY, &TextOpts::wrap(width, 2)));
        let column = theme::CAPTION.line_height
            + name.size.h
            + if meta.is_empty() { 0.0 } else { 6.0 + theme::LABEL.line_height }
            + summary.as_ref().map_or(0.0, |s| 8.0 + s.size.h);
        let header = column.max(PICTURE);
        let picture = Rect::new(theme::GUTTER, TOP + (header - PICTURE) / 2.0, PICTURE, PICTURE);
        art_well(ui, picture, &art.picture, &detail.title, PICTURE / 2.0);
        let mut y = TOP + (header - column) / 2.0;
        y += ui.text(Point::new(x, y), "Artist", &theme::CAPTION, &TextOpts::line(width), theme::MUTED).h;
        let h = name.size.h;
        ui.draw_text(Point::new(x, y), name, theme::FG);
        y += h;
        if !meta.is_empty() {
            y += 6.0;
            y += ui.text(Point::new(x, y), meta, &theme::LABEL, &TextOpts::line(width), theme::MUTED).h;
        }
        if let Some(s) = summary {
            ui.draw_text(Point::new(x, y + 8.0), s, theme::SUMMARY);
        }

        // Play, Shuffle, Now playing, and what went wrong beside them.
        let buttons_y = TOP + header + HEADER_TO_BUTTONS;
        let busy = self.starting.is_some();
        let playing = player::current_track(cx).is_some();
        let (mut play, mut shuffle, mut now) = (false, false, false);
        let bar = ui.id("buttons");
        ui.initial_focus(bar.with("play"));
        let end = ui.group(bar, bar_group(), |ui| {
            let mut x = theme::GUTTER;
            // While the tracks come, Play and Shuffle are dimmed as Android's disabled buttons
            // are, but keep focus where it is, so Back from Now Playing finds it on Play; presses
            // wait meanwhile. Now playing still works, at full strength.
            ui.opacity(if busy { theme::DISABLED_ALPHA } else { 1.0 }, |ui| {
                let b = Button::pill("Play").icon(Icon::Play);
                play = b.show(ui, ui.id("play"), Point::new(x, buttons_y)).activated;
                x += b.size(ui).w + theme::BUTTON_GAP;
                let r = RoundButton::new(Icon::Shuffle, "Shuffle");
                shuffle = r.show(ui, ui.id("shuffle"), Point::new(x, buttons_y)).activated;
                x += r.size().w + theme::BUTTON_GAP;
            });
            if playing {
                let r = RoundButton::new(Icon::AudioLines, "Now playing");
                now = r.show(ui, ui.id("now"), Point::new(x, buttons_y)).activated;
                x += r.size().w + theme::BUTTON_GAP;
            }
            x
        });
        if let Some(problem) = &self.problem {
            let at = Point::new(end + 8.0, buttons_y + 14.0);
            let width = (theme::CANVAS_W - theme::SAFE_X - at.x).max(1.0);
            ui.text(at, problem, &theme::CAPTION, &TextOpts::wrap(width, 2), theme::DANGER_TEXT);
        }

        // The albums.
        let row_y = buttons_y + theme::BUTTON_HEIGHT + theme::BUTTON_LABEL_ROOM + BUTTONS_TO_ALBUMS;
        let mut open = None;
        let mut retry = false;
        let state = list_state(&self.children);
        if let ListState::Failed(e) = state {
            retry = list_failure(ui, Point::new(theme::GUTTER, row_y + 12.0), width, &e.problem()).activated;
        } else if self.albums.is_empty() {
            let text = if matches!(state, ListState::Loading) { "Loading…" } else { "No albums found." };
            let at = Point::new(theme::GUTTER, row_y + 12.0);
            ui.text(at, text, &theme::BODY, &TextOpts::line(width), theme::MUTED);
        } else {
            let (albums, years) = (&self.albums, &self.years);
            let art_of = |i: usize| art.albums.get(i).map_or(&[][..], Vec::as_slice);
            let r = Row::new(TileKind::Square, row_y).header("Albums").show(
                ui,
                ui.id("albums"),
                &mut self.row,
                albums.len(),
                |i| albums[i].id as u64,
                |ui, slot| {
                    let a = &albums[slot.index];
                    Tile::new(TileKind::Square, &a.title)
                        .subtitle(&years[slot.index])
                        .art(art_of(slot.index))
                        .paint(ui, slot);
                },
            );
            if let Some(i) = r.activated {
                open = Some(Route::Album { id: albums[i].id, track_id: None });
            } else if let Some(i) = r.actions {
                open = Some(Route::ItemActions { id: albums[i].id, continue_watching: false });
            }
        }
        self.art = Some(art);

        if retry {
            self.children.invalidate();
        }
        if (play || shuffle) && !busy {
            self.start(cx, shuffle);
        }
        if now {
            cx.push(Route::NowPlaying);
        }
        if let Some(route) = open {
            cx.push(route);
        }
    }
}

impl Screen for ArtistScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        player::media_keys(ui, cx);
        let now = cx.now;
        self.since.get_or_insert(now);
        self.update(cx);
        let asking = self.detail.asking();
        match self.detail.state() {
            Load::Failed { error, stale: None } if !asking => {
                let (title, body, action) = if error.is(404) {
                    ("Artist not found".to_string(), NOT_FOUND_BODY.to_string(), "Back")
                } else if error.unreachable() {
                    (UNREACHABLE_TITLE.to_string(), error.problem(), "Retry")
                } else {
                    ("Couldn't load this artist".to_string(), error.problem(), "Retry")
                };
                if message(ui, &title, &body, action).activated {
                    if action == "Back" {
                        cx.back();
                    } else {
                        self.detail.invalidate();
                        self.children.invalidate();
                    }
                }
            }
            // Asked again after Retry: the skeleton while it comes, as Android's page starts over.
            Load::Loading | Load::Failed { stale: None, .. } => self.skeleton(ui, now),
            _ => self.content(ui, cx),
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => {
                self.detail.invalidate();
                self.children.invalidate();
            }
            AppEvent::Shown => {
                self.detail.revalidate(cx.now);
                self.children.revalidate(cx.now);
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }
}
