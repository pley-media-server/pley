//! An album (`TvAlbumScreen.kt`; the TV app's `AlbumScreen`): its art faint behind the page; at
//! the left the cover, the title, the artist as a link to their page, "2001 · 10 tracks · 42m",
//! the profile's rating, then Play, Shuffle and, while one of its tracks plays, Now playing; at
//! the right its tracks, with disc headers on albums of several discs. OK on a track plays the
//! album from it, or pauses and resumes the one playing. Focus starts on Play, or on the track the
//! album was opened from. The remote's media keys work here while music has a queue.

use pley_base::{Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::model::{ItemDetail, ItemList, ItemSummary};
use pley_core::store::snapshot_keys;
use pley_kit::{Button, RoundButton, SCROLL, TrackLine, backdrop_px, skeleton_block};
use pley_shell::{AppCx, AppEvent, Chrome, Load, Resource, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Edge, Group, Ui, nudge};

use crate::pages::{self, AlbumRow, NOT_FOUND_BODY, NOTHING_TO_PLAY, UNREACHABLE_TITLE};
use crate::player;
use crate::widgets::{FAINT_ART_PX, ListState, art_well, faint_art, list_failure, list_state, message, stars};

/// The page's top, the left column's width and its cover (`TvAlbumScreen.kt`).
const TOP: f32 = 40.0;
const COLUMN: f32 = 280.0;
const ART: f32 = 220.0;
/// The faint art's alpha on an album.
const FAINT: f32 = 0.14;
/// The tracks: from x 400 to the right safe edge, the page's top to its bottom.
const LIST: Rect = Rect::new(400.0, TOP, theme::CANVAS_W - theme::SAFE_X - 400.0, theme::CANVAS_H - TOP);
/// Between lines.
const LINE_GAP: f32 = 2.0;
/// Room after the last line (the list's `contentPadding`), so the last tracks can come up from
/// the screen's bottom edge.
const LIST_END: f32 = 120.0;
/// A disc header: 14 above its `Tv.Label` line (18), 4 below; 12 in.
const DISC_HEIGHT: f32 = 14.0 + 18.0 + 4.0;
/// Lines beyond the list's edges still drawn, so focus always has its next line.
const OVERSCAN: f32 = 150.0;
/// Nothing shows for this long while the page loads, then its skeleton.
const SKELETON_AFTER: f64 = 0.3;

/// The album's cover, asked for at the screen's pixels: faint over the page, and in its well.
struct AlbumArt {
    px: (u32, u32),
    faint: Vec<ImageRequest>,
    cover: Vec<ImageRequest>,
}

/// What a track's line says, worked out once per answer.
struct LineText {
    number: String,
    artist: Option<String>,
    length: String,
}

/// An album's page.
pub struct AlbumScreen {
    /// The track it was opened from: highlighted, and focused first.
    track_id: Option<i64>,
    detail: Resource<ItemDetail>,
    children: Resource<ItemList>,
    tracks: Vec<ItemSummary>,
    rows: Vec<AlbumRow>,
    /// Each row's top in the list, and the list's height.
    tops: Vec<f32>,
    content: f32,
    texts: Vec<LineText>,
    meta: String,
    art: Option<AlbumArt>,
    scroll: f32,
    scroll_started: bool,
    focus_given: bool,
    problem: Option<String>,
    since: Option<f64>,
}

impl AlbumScreen {
    /// The page of album `id`, opened from its track `track_id` when given.
    pub fn new(id: i64, track_id: Option<i64>) -> AlbumScreen {
        AlbumScreen {
            track_id,
            detail: Resource::new(Some(snapshot_keys::item(id)), move |api| api.item(id)),
            children: Resource::new(Some(snapshot_keys::children(id)), move |api| api.children(id)),
            tracks: Vec::new(),
            rows: Vec::new(),
            tops: Vec::new(),
            content: 0.0,
            texts: Vec::new(),
            meta: String::new(),
            art: None,
            scroll: 0.0,
            scroll_started: false,
            focus_given: false,
            problem: None,
            since: None,
        }
    }

    fn update(&mut self, cx: &mut AppCx) {
        let detail = self.detail.update(cx);
        let tracks = self.children.update(cx);
        if tracks {
            self.tracks = pages::tracks_of(self.children.value().map_or(&[], |l| &l.items));
            self.rows = pages::album_rows(&self.tracks);
        }
        if detail || tracks {
            self.art = None;
            let album = self.detail.value();
            let artist = album.and_then(|a| a.parent_title.as_deref());
            self.texts = self
                .tracks
                .iter()
                .map(|t| LineText {
                    number: t.index.map(|i| i.to_string()).unwrap_or_default(),
                    artist: pages::track_artist(t, artist).map(str::to_string),
                    length: pages::track_length(t),
                })
                .collect();
            self.meta = pages::album_meta(album.and_then(|a| a.year), &self.tracks);
            self.lay_out();
        }
    }

    /// Each row's top in the list.
    fn lay_out(&mut self) {
        self.tops.clear();
        let mut y = 0.0;
        for row in &self.rows {
            self.tops.push(y);
            y += self.row_height(*row) + LINE_GAP;
        }
        self.content = (y - LINE_GAP).max(0.0);
    }

    fn row_height(&self, row: AlbumRow) -> f32 {
        match row {
            AlbumRow::Disc(_) => DISC_HEIGHT,
            AlbumRow::Track(i) => self.line(i, None, false).height(),
        }
    }

    fn line(&self, i: usize, current: Option<(i64, bool)>, highlight: bool) -> TrackLine<'_> {
        let (t, text) = (&self.tracks[i], &self.texts[i]);
        let is_current = current.is_some_and(|c| c.0 == t.id);
        TrackLine {
            number: &text.number,
            title: &t.title,
            subtitle: text.artist.as_deref(),
            duration: &text.length,
            current: is_current,
            playing: is_current && current.is_some_and(|c| c.1),
            highlighted: highlight && self.track_id == Some(t.id),
            ..TrackLine::default()
        }
    }

    /// Plays the album from its track at `at`, or shuffled from a random one, and opens Now
    /// Playing.
    fn play(&mut self, cx: &mut AppCx, at: usize, shuffle: bool) {
        if self.tracks.is_empty() {
            self.problem = Some(NOTHING_TO_PLAY.into());
            return;
        }
        self.problem = None;
        let at = if shuffle { player::random_below(self.tracks.len()) } else { at.min(self.tracks.len() - 1) };
        if player::play(cx, self.tracks.clone(), at, shuffle) {
            cx.push(Route::NowPlaying);
        }
    }

    fn pictures(&mut self, ui: &Ui, cx: &AppCx) {
        let px = (backdrop_px(ui).min(FAINT_ART_PX), ui.image_px(ART));
        if self.art.as_ref().is_some_and(|a| a.px == px) {
            return;
        }
        let poster = self.detail.value().and_then(|d| d.poster_url.clone());
        let one = |w: u32| poster.as_deref().and_then(|p| cx.image(p, w)).into_iter().collect();
        self.art = Some(AlbumArt { px, faint: one(px.0), cover: one(px.1) });
    }

    fn skeleton(&self, ui: &mut Ui, now: f64) {
        let since = self.since.unwrap_or(now);
        if now - since < SKELETON_AFTER {
            ui.repaint_after(since + SKELETON_AFTER - now);
            return;
        }
        // `AlbumSkeleton`: the cover and two bars under it; eight lines 40 to its right.
        skeleton_block(ui, Rect::new(theme::GUTTER, TOP, ART, ART), radius::TILE);
        skeleton_block(ui, Rect::new(theme::GUTTER, TOP + ART + 14.0, 180.0, 22.0), radius::BADGE);
        skeleton_block(ui, Rect::new(theme::GUTTER, TOP + ART + 44.0, 120.0, 16.0), radius::BADGE);
        for i in 0..8 {
            let rect = Rect::new(theme::GUTTER + ART + 40.0, TOP + i as f32 * 44.0, 480.0, 34.0);
            skeleton_block(ui, rect, radius::FIELD);
        }
    }

    fn content(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.pictures(ui, cx);
        let Some(art) = self.art.take() else { return };
        let current = player::current_track(cx).filter(|c| self.tracks.iter().any(|t| t.id == c.0));
        let title = self.detail.value().map_or("", |d| d.title.as_str());
        faint_art(ui, &art.faint, title, FAINT);
        let (go, play_at) = self.left(ui, &art.cover, current);
        let picked = self.list(ui, current);
        self.art = Some(art);

        match go {
            Some(Go::Artist(id)) => cx.push(Route::Artist { id }),
            Some(Go::NowPlaying) => cx.push(Route::NowPlaying),
            None => {}
        }
        if let Some(shuffle) = play_at {
            self.play(cx, 0, shuffle);
        }
        if let Some(i) = picked {
            match (current, self.tracks.get(i)) {
                (Some((id, _)), Some(t)) if t.id == id => {
                    if let Some(m) = player::music(cx) {
                        m.toggle();
                    }
                }
                _ => self.play(cx, i, false),
            }
        }
    }

    /// The left column; what it asks to open, and Play (`Some(false)`) or Shuffle (`Some(true)`).
    fn left(&self, ui: &mut Ui, cover: &[ImageRequest], current: Option<(i64, bool)>) -> (Option<Go>, Option<bool>) {
        let Some(album) = self.detail.value() else { return (None, None) };
        let x = theme::GUTTER;
        art_well(ui, Rect::new(x, TOP, ART, ART), cover, &album.title, radius::TILE);
        let mut y = TOP + ART + 12.0;
        y += ui.text(Point::new(x, y), &album.title, &theme::TITLE, &TextOpts::wrap(COLUMN, 2).balanced(), theme::FG).h;

        let mut go = None;
        let mut start = None;
        let tracks_loading = matches!(list_state(&self.children), ListState::Loading);
        // Opened from a track, the first focus waits for it (Android's `initial = highlighted ==
        // null`): until the tracks come, the column is drawn but takes no focus.
        let awaiting = self.track_id.is_some() && tracks_loading && !self.focus_given;
        let column = ui.id("left");
        let mut draw = |ui: &mut Ui| {
            ui.group(column, Group::column(), |ui| {
                match (&album.parent_title, album.parent_id) {
                    (Some(artist), Some(artist_id)) => {
                        // The link's padding sits outside the column, so its text lines up with the title's.
                        let link = Button::link(artist).color(theme::ACCENT_BRIGHT).max_label_width(COLUMN);
                        if link.show(ui, ui.id("artist"), Point::new(x - 16.0, y + 2.0)).activated {
                            go = Some(Go::Artist(artist_id));
                        }
                        y += 2.0 + theme::BUTTON_HEIGHT;
                    }
                    (Some(artist), None) => {
                        y += ui.text(Point::new(x, y), artist, &theme::LABEL, &TextOpts::line(COLUMN), theme::FG).h;
                    }
                    _ => {}
                }
                if !self.meta.is_empty() {
                    y += 4.0;
                    y +=
                        ui.text(Point::new(x, y), &self.meta, &theme::CAPTION, &TextOpts::line(COLUMN), theme::MUTED).h;
                }
                if let Some(rating) = pages::user_rating(album) {
                    stars(ui, Point::new(x, y + 6.0), rating);
                    y += 6.0 + 16.0;
                }
                y += 16.0;

                let row = ui.id("buttons");
                ui.group(row, Group::row().edge(Dir::Left, Edge::Out), |ui| {
                    let mut bx = x;
                    let play = Button::pill("Play").icon(Icon::Play).enabled(tracks_loading || !self.tracks.is_empty());
                    let play_id = ui.id("play");
                    if !awaiting {
                        ui.initial_focus(play_id);
                    }
                    if play.show(ui, play_id, Point::new(bx, y)).activated {
                        start = Some(false);
                    }
                    bx += play.size(ui).w + theme::BUTTON_GAP;
                    let shuffle =
                        RoundButton::new(Icon::Shuffle, "Shuffle").on_art(false).enabled(!self.tracks.is_empty());
                    if shuffle.show(ui, ui.id("shuffle"), Point::new(bx, y)).activated {
                        start = Some(true);
                    }
                    bx += shuffle.size().w + theme::BUTTON_GAP;
                    if current.is_some() {
                        let now = RoundButton::new(Icon::AudioLines, "Now playing").on_art(false);
                        if now.show(ui, ui.id("now"), Point::new(bx, y)).activated {
                            go = Some(Go::NowPlaying);
                        }
                    }
                });
            });
        };
        if awaiting {
            pley_kit::inert(ui, draw);
        } else {
            draw(ui);
        }
        if let Some(problem) = &self.problem {
            // Under the buttons and the labels that show under round ones.
            let at = Point::new(x, y + theme::BUTTON_HEIGHT + 6.0 + theme::CAPTION.line_height + 8.0);
            ui.text(at, problem, &theme::CAPTION, &TextOpts::wrap(COLUMN, 2), theme::DANGER_TEXT);
        }
        (go, start)
    }

    /// The tracks; the one OK picked.
    fn list(&mut self, ui: &mut Ui, current: Option<(i64, bool)>) -> Option<usize> {
        let failed = match list_state(&self.children) {
            ListState::Loading => {
                for i in 0..8 {
                    let rect = Rect::new(LIST.x, LIST.y + 4.0 + i as f32 * 46.0, 480.0, 36.0);
                    skeleton_block(ui, rect, radius::FIELD);
                }
                return None;
            }
            ListState::Failed(e) => Some(e.problem()),
            ListState::Ready => None,
        };
        if let Some(problem) = failed {
            if list_failure(ui, LIST.origin(), LIST.w, &problem).activated {
                self.children.invalidate();
            }
            return None;
        }
        if self.tracks.is_empty() {
            ui.text(LIST.origin(), "No tracks found", &theme::BODY, &TextOpts::line(LIST.w), theme::MUTED);
            return None;
        }
        let id = ui.id("tracks");
        let line_id = |t: &ItemSummary| id.with(t.id);
        let row_of =
            |track: i64| self.rows.iter().position(|r| matches!(r, AlbumRow::Track(i) if self.tracks[*i].id == track));
        let highlighted = self.track_id.and_then(row_of);
        let max = (self.content + LIST_END - LIST.h).max(0.0);
        let anchored = |row: usize| (self.tops[row] - LIST.h * theme::SCROLL_ANCHOR).clamp(0.0, max);

        // The row focus is on: the list follows it, its top at 30 % of the list's height.
        let focused = ui.focused().filter(|_| ui.focused_in(id).is_some()).and_then(|f| {
            self.rows.iter().position(|r| matches!(r, AlbumRow::Track(i) if line_id(&self.tracks[*i]) == f))
        });
        if !self.scroll_started {
            self.scroll = highlighted.map_or(0.0, anchored);
            self.scroll_started = true;
        } else if let Some(row) = focused {
            let (lo, hi) = (self.tops[row], self.tops[row] + self.row_height(self.rows[row]));
            self.scroll =
                if ui.pointer_mode() { nudge(self.scroll, lo, hi, 0.0, LIST.h, 0.0, max) } else { anchored(row) };
        }
        self.scroll = self.scroll.clamp(0.0, max);
        let shown = ui.animate(id.with("scroll"), self.scroll, SCROLL);

        // Coming in from the left (or first): the track it was opened from, the one playing, else
        // the first; after that, the one focused last.
        let entry = highlighted
            .map(|r| self.rows[r])
            .or_else(|| current.and_then(|c| row_of(c.0)).map(|r| self.rows[r]))
            .and_then(|r| match r {
                AlbumRow::Track(i) => Some(line_id(&self.tracks[i])),
                AlbumRow::Disc(_) => None,
            })
            .unwrap_or_else(|| line_id(&self.tracks[0]));
        let mut group = Group::column();
        if ui.remembered(id).is_none() {
            group = group.enter_at(entry);
        }
        if let Some(row) = highlighted
            && !self.focus_given
        {
            // Opened from a track: it takes the first focus once it is drawn.
            let AlbumRow::Track(i) = self.rows[row] else { unreachable!() };
            ui.request_focus(line_id(&self.tracks[i]));
            self.focus_given = true;
        }

        let mut picked = None;
        let near = focused.unwrap_or(usize::MAX);
        ui.clip(LIST, |ui| {
            ui.group(id, group, |ui| {
                for (r, row) in self.rows.iter().enumerate() {
                    let top = LIST.y + self.tops[r] - shown;
                    let h = self.row_height(*row);
                    let close = near != usize::MAX && r.abs_diff(near) <= 2;
                    if !close && (top > LIST.bottom() + OVERSCAN || top + h < LIST.y - OVERSCAN) {
                        continue;
                    }
                    let rect = Rect::new(LIST.x, top, LIST.w, h);
                    match *row {
                        AlbumRow::Disc(d) => {
                            let at = Point::new(rect.x + 12.0, rect.y + 14.0);
                            ui.text(at, &format!("Disc {d}"), &theme::LABEL, &TextOpts::line(LIST.w), theme::SUBTLE);
                        }
                        AlbumRow::Track(i) => {
                            let line = self.line(i, current, true);
                            if line.show(ui, line_id(&self.tracks[i]), rect).activated {
                                picked = Some(i);
                            }
                        }
                    }
                }
            });
        });
        picked
    }
}

/// A page the album opens.
enum Go {
    Artist(i64),
    NowPlaying,
}

impl Screen for AlbumScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let now = cx.now;
        self.since.get_or_insert(now);
        self.update(cx);
        let asking = self.detail.asking();
        match self.detail.state() {
            Load::Failed { error, stale: None } if !asking => {
                let (title, body, action) = if error.is(404) {
                    ("Album not found".to_string(), NOT_FOUND_BODY.to_string(), "Back")
                } else if error.unreachable() {
                    (UNREACHABLE_TITLE.to_string(), error.problem(), "Retry")
                } else {
                    ("Couldn't load this album".to_string(), error.problem(), "Retry")
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
