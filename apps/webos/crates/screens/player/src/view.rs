//! The player screen's own state and its frame (`TvPlayerScreen.kt`, `TvPlayerUi.kt`,
//! `TvPlayerKeys.kt`; the TV app's `ui/player/player.tsx`): which overlays show, where focus
//! goes, what each key does, and the layers drawn over the picture, bottom to top: the controls'
//! scrim, the subtitles, the focus holder of the hidden controls, the controls, the compact
//! Playback details, Skip, the sleep notice and Up Next, the seek badge, the buffering ring,
//! the panels, the error and the toast.

use pley_base::{Color, Icon, Key, KeyEvent, Point, Rect};
use pley_core::Settings;
use pley_core::format::FactBlock;
use pley_core::model::SubtitleResult;
use pley_core::store::{SubtitleBackground, SubtitleFont, SubtitlePosition, SubtitleSize};
use pley_kit::feedback::{BUFFERING_DELAY, HANDOFF_DELAY};
use pley_kit::{ListState, LoadingRing, Message, MessageStyle, SidePanel, inert};
use pley_player::controller::{SeekPreview, SubtitleSearch, UpNextCard, next_chapter_start, previous_chapter_start};
use pley_player::seeker::{MEDIA_STEP_MS, SETTLE_MS, STEP_MS, Seeker, WAIT_MS};
use pley_player::{Phase, PlayerState};
use pley_ui::{Dirs, Easing, Edge, FocusOpts, Group, Id, Motion, Ui, theme};

use crate::chapters::{self, Strip};
use crate::controls::{self, Btn, RowOptions, Scrub, TopBar};
use crate::overlays::{self, Card, KEY_TOAST_SECS, LONG_TOAST_SECS, TOAST_SECS, Toast};
use crate::panels::{self, Panel, PanelInput, Pick, Row};
use crate::playback::Playback;
use crate::previews::{Previews, Tile, Want};
use crate::{clock, subtitles, text};

/// The controls hide this long after the last key while a video plays (`HIDE_AFTER_MS`).
pub const HIDE_AFTER_MS: i64 = 5_000;
/// OK this soon after a seek, or while one is coming, plays on from there (`TvSeeker.CONFIRM_MS`).
pub const CONFIRM_MS: i64 = 2_000;
/// The title banner shows this long as each item begins (`BannerMs`).
pub const BANNER_MS: i64 = 4_000;
/// The server stopped the playback: the player leaves this long after saying so (`TvPlayerExtras.kt`).
pub const STOPPED_LEAVE_MS: i64 = 20_000;
/// Skip, the notice and Up Next fade in and out over 150 ms.
const CORNER_FADE: Motion = Motion::new(0.150, Easing::FAST_OUT_SLOW_IN);
/// The buffering ring's centre.
const RING_CENTRE: Point = Point::new(theme::CANVAS_W / 2.0, theme::CANVAS_H / 2.0);
/// "Ends at" counts at most this far ahead: a duration past it is the server's mistake.
const ENDS_AT_MAX_MS: i64 = 7 * 24 * 3_600_000;
/// A sheet's previews counted at most, across and down, to find the sheet ahead of a scrub.
const SHEET_SIDE_MAX: i64 = 100;

/// What the frame needs from the app besides the playback.
pub struct FrameEnv<'a> {
    /// The subtitles' look and whether Playback details show over the video.
    pub settings: &'a Settings,
    /// The TV writes times on 24 hours.
    pub hours24: bool,
    /// Seconds since 1970, now, for the clock.
    pub unix_now: f64,
}

/// What a frame asks of the app.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Out {
    /// The player leaves (the end, Stop, Back on the error, a stop by the server).
    pub leave: bool,
    /// A choice of the subtitle look to keep in the settings.
    pub style: Option<Style>,
}

/// A choice of the subtitle look made in the Subtitles panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Size(SubtitleSize),
    Background(SubtitleBackground),
    Font(SubtitleFont),
    Position(SubtitlePosition),
}

impl Style {
    /// Puts the choice in `settings`.
    pub fn apply(self, settings: &mut Settings) {
        match self {
            Style::Size(v) => settings.subtitle_size = v,
            Style::Background(v) => settings.subtitle_background = v,
            Style::Font(v) => settings.subtitle_font = v,
            Style::Position(v) => settings.subtitle_position = v,
        }
    }
}

/// Where focus is asked to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The holder of the keys while the controls are hidden.
    Video,
    Skip,
    /// The sleep notice's Keep watching.
    Keep,
    Scrubber,
    /// The chapters as cards over the scrubber.
    Chapters,
    Button(Btn),
    PlayNow,
    /// Up Next's Cancel.
    Cancel,
    /// A row of the panel open, or Playback details.
    Panel,
    /// A button of the error.
    Error,
}

/// The ids of what the player draws, made once a frame in the page's scope.
#[derive(Clone, Copy, Debug)]
pub struct Ids {
    pub video: Id,
    pub skip_group: Id,
    pub skip: Id,
    pub keep_group: Id,
    pub keep: Id,
    pub scrub_group: Id,
    pub scrubber: Id,
    pub chapters: Id,
    pub row: Id,
    pub card: Id,
    pub play_now: Id,
    pub cancel: Id,
    pub error: Id,
    pub panel: Id,
    pub wide: Id,
    pub info: Id,
    fade: Id,
}

impl Ids {
    fn new(ui: &Ui) -> Ids {
        let id = |k: &str| ui.id(("player", k));
        Ids {
            video: id("video"),
            skip_group: id("skip-group"),
            skip: id("skip"),
            keep_group: id("keep-group"),
            keep: id("keep"),
            scrub_group: id("scrub-group"),
            scrubber: id("scrubber"),
            chapters: id("chapters"),
            row: id("row"),
            card: id("card"),
            play_now: id("play-now"),
            cancel: id("cancel"),
            error: id("error"),
            panel: id("panel"),
            wide: id("panel-wide"),
            info: id("info"),
            fade: id("fade"),
        }
    }

    /// A button of the control row.
    pub fn button(&self, b: Btn) -> Id {
        self.row.with(b)
    }

    fn of(&self, t: Target) -> Id {
        match t {
            Target::Video => self.video,
            Target::Skip => self.skip,
            Target::Keep => self.keep,
            Target::Scrubber => self.scrubber,
            Target::Chapters => self.chapters,
            Target::Button(b) => self.button(b),
            Target::PlayNow => self.play_now,
            Target::Cancel => self.cancel,
            Target::Panel => self.panel,
            Target::Error => self.error,
        }
    }
}

/// What the screen saw of the playback last frame, to act on what changes.
#[derive(Clone, Debug, Default)]
struct Seen {
    notice: u64,
    up_next: Option<i64>,
    error: bool,
    skip: bool,
    keep: bool,
    original: bool,
    lower: bool,
    adding: bool,
}

/// Where a panel's focus starts when it is drawn next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fresh {
    /// On the choice in force (or the first row).
    Initial,
    /// On Find subtitles…, coming back from the search.
    Find,
}

/// What was done this frame, carried out once drawing lets go of the playback.
#[derive(Clone, Debug, PartialEq)]
enum Act {
    VideoOk,
    VideoMenu,
    Button(Btn),
    ScrubOk,
    ScrubClick(f32),
    /// OK on a chapter's card: its index.
    Chapter(usize),
    Skip,
    Keep,
    PlayNow,
    CancelCard,
    Retry,
    Leave,
    Pick(Pick),
    Sync(i32),
    SyncReset,
    ClosePanel,
    Details(bool),
}

/// The player screen's state, apart from the playback.
#[derive(Debug)]
pub struct PlayerView {
    controls: bool,
    /// The chapters as cards, while they are open.
    strip: Option<Strip>,
    /// The item the title banner was last shown or due for.
    banner_for: Option<i64>,
    /// The banner waits for the item to play.
    banner_due: bool,
    /// The banner shows until then (ms of the app's clock).
    banner_until: i64,
    last_input: i64,
    panel: Option<Panel>,
    /// The narrow panel drawn last and its rows, so it slides out as it was.
    last_narrow: Option<Panel>,
    rows: Vec<Row>,
    /// Info's sections as they showed last, for the same reason.
    sections: Vec<FactBlock>,
    list: ListState,
    fresh: Option<Fresh>,
    info_scroll: f32,
    seeker: Seeker,
    confirm_until: i64,
    toast: Option<Toast>,
    remembered_subtitle: Option<i64>,
    ask: Option<Target>,
    seen: Seen,
    waiting_since: Option<f64>,
    stopped_at: Option<i64>,
    fired_at: Option<i64>,
    details_open: bool,
    can_find: bool,
    found: Vec<SubtitleResult>,
    add_failed: Option<String>,
    fonts: Option<Vec<SubtitleFont>>,
    last_pointer: Option<Point>,
    last_focus: Option<Id>,
    last_skip: String,
    last_seconds: i64,
    last_card: Option<UpNextCard>,
    previews: Previews,
}

impl Default for PlayerView {
    fn default() -> PlayerView {
        PlayerView::new()
    }
}

fn ms(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
}

fn secs(ms: i64) -> f64 {
    ms.max(0) as f64 / 1000.0
}

impl PlayerView {
    /// The screen as a player opens: controls hidden, nothing open.
    pub fn new() -> PlayerView {
        PlayerView {
            controls: false,
            last_input: 0,
            panel: None,
            last_narrow: None,
            rows: Vec::new(),
            sections: Vec::new(),
            list: ListState::default(),
            fresh: None,
            info_scroll: 0.0,
            seeker: Seeker::default(),
            confirm_until: 0,
            toast: None,
            remembered_subtitle: None,
            ask: None,
            seen: Seen::default(),
            waiting_since: None,
            stopped_at: None,
            strip: None,
            banner_for: None,
            banner_due: false,
            banner_until: 0,
            fired_at: None,
            details_open: false,
            can_find: true,
            found: Vec::new(),
            add_failed: None,
            fonts: None,
            last_pointer: None,
            last_focus: None,
            last_skip: String::new(),
            last_seconds: 0,
            last_card: None,
            previews: Previews::none(),
        }
    }

    /// Where the seek previews' sheets come from (none until this is called: no pictures).
    pub fn set_previews(&mut self, previews: Previews) {
        self.previews = previews;
    }

    /// Whether the controls show.
    pub fn controls(&self) -> bool {
        self.controls
    }

    /// The panel open, if one is.
    pub fn panel(&self) -> Option<Panel> {
        self.panel
    }

    /// The toast showing, if one is.
    pub fn toast_text(&self) -> Option<&str> {
        self.toast.as_ref().map(|t| t.text.as_str())
    }

    /// What has focus, in the player's terms (read before the frame draws: focus as the last
    /// frames settled it).
    pub fn focus(&self, ui: &Ui) -> Option<Target> {
        let ids = Ids::new(ui);
        let f = ui.focused()?;
        let simple = [
            Target::Video,
            Target::Skip,
            Target::Keep,
            Target::Scrubber,
            Target::Chapters,
            Target::PlayNow,
            Target::Cancel,
        ];
        if let Some(t) = simple.into_iter().find(|t| ids.of(*t) == f) {
            return Some(t);
        }
        let buttons = [
            Btn::Chapters,
            Btn::Sleep,
            Btn::Speed,
            Btn::Previous,
            Btn::Back10,
            Btn::PlayPause,
            Btn::Forward10,
            Btn::Next,
            Btn::Subtitles,
            Btn::Audio,
            Btn::Quality,
            Btn::Info,
        ];
        if let Some(b) = buttons.into_iter().find(|b| ids.button(*b) == f) {
            return Some(Target::Button(b));
        }
        if ui.focused_in(ids.panel).is_some() || ui.focused_in(ids.wide).is_some() {
            return Some(Target::Panel);
        }
        ui.focused_in(ids.error).map(|_| Target::Error)
    }

    /// The playback was made again, paused (the TV showed the app after hiding it): what was seen
    /// of the last one goes, nothing is open, and the controls show on the scrubber, as a pause
    /// shows them. The title banner, shown for this item already, doesn't come again.
    pub fn reopened(&mut self) {
        let can_find = self.can_find;
        let previews = std::mem::take(&mut self.previews);
        let banner_for = self.banner_for;
        *self = PlayerView {
            can_find,
            previews,
            banner_for,
            controls: true,
            ask: Some(Target::Scrubber),
            ..PlayerView::new()
        };
    }

    /// A panel is open: it keeps focus.
    pub fn modal(&self) -> bool {
        self.panel.is_some()
    }

    /// Back, in the Android app's order: the search back to Subtitles, a panel closes, Up Next
    /// is cancelled, the controls hide; otherwise the player leaves (false).
    pub fn back(&mut self, p: &mut dyn Playback) -> bool {
        match self.panel {
            Some(Panel::Find) => {
                self.panel = Some(Panel::Subtitles);
                self.fresh = Some(Fresh::Find);
                return true;
            }
            Some(panel) => {
                self.panel = None;
                self.ask = Some(Target::Button(panel.button()));
                return true;
            }
            None => {}
        }
        let s = p.state();
        if matches!(s.phase, Phase::Error { .. }) {
            return false;
        }
        if s.up_next.is_some() {
            p.cancel_up_next();
            return true;
        }
        if self.strip.is_some() {
            self.close_chapters();
            return true;
        }
        // A seek waiting for OK: Back lets it go, and it plays on from where it was.
        if self.controls && self.seeker.pending() {
            self.seeker.cancel();
            return true;
        }
        if self.controls {
            let skip = self.skip_shown(s);
            self.controls = false;
            self.ask = Some(hidden_focus(skip));
            return true;
        }
        false
    }

    /// One frame: the playback runs, then the keys, then the layers; what to do comes back.
    pub fn frame(&mut self, ui: &mut Ui, p: &mut dyn Playback, env: &FrameEnv) -> Out {
        let now = ms(ui.time());
        let ids = Ids::new(ui);
        let mut out = Out::default();
        ui.set_clear(Color::TRANSPARENT);
        p.tick(now);
        if let Some(t) = self.seeker.tick(now) {
            p.seek_to(t);
            self.confirm_until = now + CONFIRM_MS;
        }
        self.follow(ui, p, now, &ids, &mut out);
        self.input(ui, p, now, &ids, &mut out);
        if let Some(t) = self.ask.take() {
            ui.request_focus(ids.of(t));
        }
        let acts = self.draw(ui, p, env, &ids);
        for a in acts {
            self.act(ui, p, now, a, &mut out);
        }
        self.timers(ui, p, now, env);
        out
    }

    // ---------------------------------------------------------------- what the playback says

    fn skip_shown(&self, s: &PlayerState) -> bool {
        s.skip_label.is_some()
            && s.up_next.is_none()
            && !matches!(s.phase, Phase::Error { .. })
            && !s.switching
            && !(self.controls && self.seeker.pending())
    }

    fn keep_shown(&self, s: &PlayerState) -> bool {
        s.sleep.seconds_left.is_some() && !matches!(s.phase, Phase::Error { .. })
    }

    /// Acts on what changed in the playback: its notices become toasts; Up Next and errors take
    /// the screen; Skip and the sleep notice take focus as they come; the end, a stop by the
    /// server and the sleep timer's long pause make the player leave.
    fn follow(&mut self, ui: &mut Ui, p: &mut dyn Playback, now: i64, ids: &Ids, out: &mut Out) {
        let s = p.state();
        let notice = if s.notice_id != self.seen.notice { s.notice.clone() } else { None };
        self.seen.notice = s.notice_id;
        let note = s.note.clone().unwrap_or_default();
        let original = s.can_play_original && !self.seen.original && !note.is_empty();
        let lower = s.suggest_lower_quality && !self.seen.lower && !note.is_empty();
        self.seen.original = s.can_play_original;
        self.seen.lower = s.suggest_lower_quality;
        let card = s.up_next.as_ref().map(|c| c.item_id);
        if s.up_next.is_some() && s.up_next != self.last_card {
            self.last_card = s.up_next.clone();
        }
        let error = matches!(s.phase, Phase::Error { .. });
        let skip = self.skip_shown(s);
        let keep = self.keep_shown(s);
        if let Some(l) = &s.skip_label {
            l.clone_into(&mut self.last_skip);
        }
        if let Some(n) = s.sleep.seconds_left {
            self.last_seconds = n;
        }
        // The title banner, once as each item begins playing (the first, or the next episode
        // by itself), unless the controls show, which have the title already.
        if self.banner_for != Some(s.item_id) {
            self.banner_for = Some(s.item_id);
            self.banner_due = true;
            self.banner_until = 0;
        }
        if self.banner_due && s.phase == Phase::Playing {
            self.banner_due = false;
            if !self.controls {
                self.banner_until = now + BANNER_MS;
            }
        }
        let waiting = matches!(s.phase, Phase::Preparing | Phase::Buffering) || s.switching;
        let fired = s.sleep.fired && s.phase == Phase::Paused;
        let ended = s.phase == Phase::Ended;
        let stopped = s.stopped_by_server;
        let adding = matches!(s.search, SubtitleSearch::Adding(_));
        let added = self.seen.adding && matches!(s.search, SubtitleSearch::Idle);
        let failed = match &s.search {
            SubtitleSearch::Failed(e) => Some(e.clone()),
            _ => None,
        };
        if let SubtitleSearch::Found(r) = &s.search
            && *r != self.found
        {
            self.found = r.clone();
        }

        if let Some(n) = notice {
            self.say(ui, n, TOAST_SECS, false);
            p.dismiss_notice();
        }
        if original {
            self.say(ui, format!("{note} Play original is in Quality."), LONG_TOAST_SECS, false);
        }
        if lower {
            self.say(ui, format!("{note} Choose one in Quality."), LONG_TOAST_SECS, true);
        }
        if card.is_some() && card != self.seen.up_next {
            // Up Next takes the screen: the seek goes, the panel and controls make way.
            self.seeker.cancel();
            self.strip = None;
            self.panel = None;
            self.controls = false;
            self.ask = Some(Target::PlayNow);
        } else if card.is_none() && self.seen.up_next.is_some() && !error {
            self.ask = Some(hidden_focus(skip));
        }
        self.seen.up_next = card;
        if error && !self.seen.error {
            self.seeker.cancel();
            self.strip = None;
            self.panel = None;
            self.controls = false;
        } else if !error && self.seen.error {
            self.ask = Some(hidden_focus(skip));
        }
        self.seen.error = error;
        let focused = ui.focused();
        if skip && !self.seen.skip && !self.controls && self.panel.is_none() {
            self.ask = Some(Target::Skip);
        } else if !skip && self.seen.skip && focused == Some(ids.skip) {
            self.ask = Some(if self.controls { Target::Scrubber } else { Target::Video });
        }
        if keep && !self.seen.keep && !skip && card.is_none() && !self.controls && self.panel.is_none() {
            self.ask = Some(Target::Keep);
        } else if !keep && self.seen.keep && focused == Some(ids.keep) {
            self.ask = Some(if self.controls { Target::Scrubber } else { hidden_focus(skip) });
        }
        self.seen.skip = skip;
        self.seen.keep = keep;
        self.waiting_since = if waiting { Some(self.waiting_since.unwrap_or(ui.time())) } else { None };

        // The search: an added subtitle closes it; a failed add says why; an older server's
        // search isn't offered again.
        if added && self.panel == Some(Panel::Find) {
            self.panel = None;
            self.ask = Some(Target::Button(Btn::Subtitles));
        }
        if let Some(e) = failed {
            if self.seen.adding {
                self.add_failed = Some(text::add_problem(&e));
            }
            if text::search_unsupported(&e) {
                self.can_find = false;
            }
        }
        self.seen.adding = adding;

        if ended {
            out.leave = true;
        }
        let stopped_at = if stopped { Some(*self.stopped_at.get_or_insert(now)) } else { None };
        self.stopped_at = stopped_at;
        if stopped_at.is_some_and(|t| now >= t + STOPPED_LEAVE_MS) {
            out.leave = true;
        }
        let fired_at = if fired { Some(*self.fired_at.get_or_insert(now)) } else { None };
        self.fired_at = fired_at;
        if fired_at.is_some_and(|t| now >= t + pley_player::sleep::LEAVE_AFTER_MS) {
            out.leave = true;
        }
    }

    // ---------------------------------------------------------------- keys

    /// The keys, in `TvPlayerScreen.kt`'s order: the media keys whatever shows; then nothing more
    /// while an error, Up Next or a panel has the keys; then Left and Right seek, and Up, Down,
    /// Info and Guide bring the controls, from the hidden controls' holder (or Skip, or the
    /// notice) and Left and Right from the scrubber. A move of the pointer brings the controls.
    fn input(&mut self, ui: &mut Ui, p: &mut dyn Playback, now: i64, ids: &Ids, out: &mut Out) {
        let s = p.state();
        let error = matches!(s.phase, Phase::Error { .. });
        let card = s.up_next.is_some();
        let busy = s.phase == Phase::Preparing || s.switching;
        let skip = self.skip_shown(s);
        let chapters = s.chapters.len();
        let clock = p.clock();

        let pointer = ui.pointer();
        let moved = pointer.is_some() && pointer != self.last_pointer;
        self.last_pointer = pointer;
        if moved {
            self.touch(now);
            if !self.controls && !card && !error && self.panel.is_none() {
                self.show(Target::Scrubber);
            }
        }
        if ui.focused() != self.last_focus {
            self.last_focus = ui.focused();
            self.touch(now);
        }
        let keys: Vec<KeyEvent> = ui.keys().to_vec();
        if !keys.is_empty() {
            self.touch(now);
        }
        for e in keys.iter().filter(|e| e.pressed && !e.repeat) {
            match e.key {
                Key::PlayPause => self.play_pause(p, now),
                Key::Play => p.play(),
                Key::Pause => {
                    p.pause();
                    if !self.controls && !card && !error {
                        self.show(Target::Scrubber);
                    }
                }
                Key::FastForward | Key::Rewind => {
                    let dir = if e.key == Key::Rewind { -1 } else { 1 };
                    let c = p.clock();
                    self.seeker.press(now, dir, MEDIA_STEP_MS, SETTLE_MS, c.position_ms, c.duration_ms);
                }
                Key::Next => self.next(p),
                Key::Previous => self.previous(p),
                Key::Subtitles => self.toggle_subtitles(ui, p),
                Key::Audio => self.open_audio(p),
                Key::Stop => out.leave = true,
                _ => {}
            }
        }
        if error || card || self.panel.is_some() {
            return;
        }
        let focused = ui.focused();
        let holder = !self.controls && [ids.video, ids.skip, ids.keep].iter().any(|i| focused == Some(*i));
        let on_bar = self.controls && focused == Some(ids.scrubber);
        let on_cards = self.strip.is_some() && focused == Some(ids.chapters);
        for e in &keys {
            let first = e.pressed && !e.repeat;
            if on_cards {
                match e.key {
                    Key::Left | Key::Right if e.pressed => {
                        let by = if e.key == Key::Left { -1 } else { 1 };
                        if let Some(strip) = self.strip.as_mut() {
                            strip.step(by, chapters);
                        }
                    }
                    Key::Down if first => self.close_chapters(),
                    _ => {}
                }
                continue;
            }
            match e.key {
                Key::Up if on_bar && first && self.seeker.pending() && chapters > 1 => {
                    self.open_chapters(p.clock().position_ms, &p.state().chapters, false);
                }
                Key::Left | Key::Right => {
                    let dir = if e.key == Key::Left { -1 } else { 1 };
                    if !e.pressed {
                        // Let go, a scrub waits for OK where it got to, as presses do.
                        self.seeker.release(now);
                    } else if (holder || on_bar) && !busy {
                        // With the controls hidden, they show on the scrubber, the seek's preview
                        // with them.
                        if !self.controls {
                            self.show(Target::Scrubber);
                        }
                        if e.repeat {
                            self.seeker.repeat(now, dir, clock.position_ms, clock.duration_ms);
                        } else {
                            self.seeker.press(now, dir, STEP_MS, WAIT_MS, clock.position_ms, clock.duration_ms);
                        }
                    }
                }
                Key::Up if holder && first => self.show(Target::Scrubber),
                Key::Down if holder && first => self.show(Target::Button(Btn::PlayPause)),
                Key::Info | Key::Guide if !self.controls && first => {
                    self.show(if skip { Target::Skip } else { Target::Scrubber });
                }
                _ => {}
            }
        }
    }

    fn touch(&mut self, now: i64) {
        self.last_input = now;
    }

    /// The chapters as cards, on the chapter a waiting seek was going to (or the one playing): the
    /// seek gives way to them, and the video plays on while they show.
    fn open_chapters(&mut self, position_ms: i64, marks: &[pley_player::controller::ChapterMark], from_button: bool) {
        if marks.len() < 2 {
            return;
        }
        let from = self.seeker.target.unwrap_or(position_ms);
        self.seeker.cancel();
        self.strip = Some(Strip::open(marks, from, from_button));
        self.controls = true;
        self.ask = Some(Target::Chapters);
    }

    /// The cards go, their pictures with them; focus goes back where they were opened from.
    fn close_chapters(&mut self) {
        if let Some(strip) = self.strip.take() {
            self.ask = Some(if strip.from_button { Target::Button(Btn::Chapters) } else { Target::Scrubber });
        }
    }

    fn show(&mut self, target: Target) {
        self.controls = true;
        self.ask = Some(target);
        // The controls have the title: the banner makes way, for good.
        self.banner_until = 0;
    }

    fn say(&mut self, ui: &mut Ui, text: String, secs: f64, error: bool) {
        self.toast = Some(Toast { text, error, at: ui.time(), secs });
        ui.request_repaint();
    }

    /// Play/Pause (`playPause`): OK after a seek plays on from there; otherwise it pauses (showing
    /// the controls on the scrubber) or plays.
    fn play_pause(&mut self, p: &mut dyn Playback, now: i64) {
        if self.confirm(p, now) {
            if !p.wants_to_play() {
                p.play();
            }
            return;
        }
        if p.wants_to_play() {
            p.pause();
            let s = p.state();
            if !self.controls && s.up_next.is_none() && !matches!(s.phase, Phase::Error { .. }) {
                self.show(Target::Scrubber);
            }
        } else {
            p.play();
        }
    }

    /// OK confirming a seek (`TvSeeker.confirm`): one coming is made now; within 2 s of one made,
    /// OK only plays on. Either way the next OK pauses.
    fn confirm(&mut self, p: &mut dyn Playback, now: i64) -> bool {
        if let Some(t) = self.seeker.target {
            self.seeker.cancel();
            p.seek_to(t);
            self.confirm_until = 0;
            return true;
        }
        if now < self.confirm_until {
            self.confirm_until = 0;
            return true;
        }
        false
    }

    /// Next: an episode plays the next one; otherwise the next chapter more than a second ahead.
    fn next(&mut self, p: &mut dyn Playback) {
        let s = p.state();
        let at = self.seeker.target.unwrap_or(p.clock().position_ms);
        if s.subtitle.is_some() {
            self.seeker.cancel();
            p.play_next();
        } else if let Some(t) = next_chapter_start(&s.chapters, at) {
            self.seeker.cancel();
            p.seek_to(t);
        }
    }

    /// Previous: the chapter's start when more than 3 s into it (or in the first), else the one
    /// before; the start without chapters. Never the previous episode.
    fn previous(&mut self, p: &mut dyn Playback) {
        let at = self.seeker.target.unwrap_or(p.clock().position_ms);
        let to = previous_chapter_start(&p.state().chapters, at);
        self.seeker.cancel();
        p.seek_to(to);
    }

    /// The subtitles key (`toggleCaptions`): off, remembering the track; or back on with it,
    /// else the first not burned in, else any; a 2 s toast says which.
    fn toggle_subtitles(&mut self, ui: &mut Ui, p: &mut dyn Playback) {
        let subs = &p.state().subtitles;
        if let Some(on) = subs.iter().find(|t| t.selected && t.index.is_some()) {
            self.remembered_subtitle = on.index;
            p.select_subtitle(None);
            self.say(ui, "Subtitles off".into(), KEY_TOAST_SECS, false);
            return;
        }
        let tracks: Vec<_> = subs.iter().filter(|t| t.index.is_some()).collect();
        let pick = tracks
            .iter()
            .find(|t| t.index == self.remembered_subtitle)
            .or_else(|| tracks.iter().find(|t| !t.burn_in))
            .or_else(|| tracks.first())
            .map(|t| (t.index, t.label.clone()));
        if let Some((index, label)) = pick {
            p.select_subtitle(index);
            self.say(ui, format!("Subtitles: {label}"), KEY_TOAST_SECS, false);
        }
    }

    /// The audio key opens the Audio panel over the controls, with two tracks or more; it changes
    /// nothing by itself (`openAudio`).
    fn open_audio(&mut self, p: &mut dyn Playback) {
        let s = p.state();
        if s.audio.len() < 2 || matches!(s.phase, Phase::Error { .. }) || s.up_next.is_some() {
            return;
        }
        if self.panel == Some(Panel::Audio) {
            return;
        }
        self.controls = true;
        self.open(Panel::Audio);
    }

    fn open(&mut self, panel: Panel) {
        self.panel = Some(panel);
        self.fresh = Some(Fresh::Initial);
        if panel == Panel::Info {
            self.info_scroll = 0.0;
        }
        if panel == Panel::Find {
            self.add_failed = None;
            self.found.clear();
        }
    }

    fn close_panel(&mut self) {
        if let Some(panel) = self.panel.take() {
            self.ask = Some(Target::Button(panel.button()));
        }
    }

    // ---------------------------------------------------------------- drawing

    fn draw(&mut self, ui: &mut Ui, p: &dyn Playback, env: &FrameEnv, ids: &Ids) -> Vec<Act> {
        let mut acts = Vec::new();
        let s = p.state();
        let error = matches!(s.phase, Phase::Error { .. });
        let card = s.up_next.is_some();
        let skip = self.skip_shown(s);
        let keep = self.keep_shown(s);
        let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
        let fade = ui.animate(ids.fade, if self.controls { 1.0 } else { 0.0 }, Motion::OVERLAY);

        controls::scrim(ui, fade);
        let banner = !self.controls && !error && !card && ms(ui.time()) < self.banner_until;
        let banner_fade = ui.animate(ids.fade.with("banner"), if banner { 1.0 } else { 0.0 }, Motion::OVERLAY);
        if banner_fade > 0.0 {
            let line = text::episode_line(s.subtitle.as_deref());
            controls::banner(ui, &s.title, line.as_deref(), banner_fade);
        }
        subtitles::draw(ui, p.subtitles(), &subtitles::look_of(env.settings), fade);

        if !self.controls && !error && !card && self.panel.is_none() {
            ui.initial_focus(ids.video);
            let r = controls::register(ui, ids.video, full, FocusOpts::default().takes(Dirs::ALL));
            if r.activated {
                acts.push(Act::VideoOk);
            }
            if r.menu {
                acts.push(Act::VideoMenu);
            }
        }

        let preview = self.seek_preview(ui, p);
        // Drawn from the frame they are asked for, though still transparent, so focus can land
        // on them at once.
        if fade > 0.0 || self.controls {
            let live = self.controls && !error && !card;
            let preview = preview.as_ref();
            ui.opacity(fade, |ui| {
                if live {
                    self.draw_controls(ui, p, env, ids, skip, preview, &mut acts);
                } else {
                    inert(ui, |ui| self.draw_controls(ui, p, env, ids, skip, preview, &mut acts));
                }
            });
        }

        let readout = env.settings.playback_details && !self.controls && self.panel.is_none() && !card && !error;
        if readout {
            overlays::readout(ui, &panels::readout_lines(s, &p.engine_stats()));
            ui.repaint_after(1.0);
        }
        let details = readout || self.panel == Some(Panel::Info);
        if details != self.details_open {
            self.details_open = details;
            acts.push(Act::Details(details));
        }

        let bottom = overlays::bottom(fade);
        let skip_fade = ui.animate(ids.skip.with("fade"), if skip { 1.0 } else { 0.0 }, CORNER_FADE);
        if skip_fade > 0.0 || skip {
            let label = self.last_skip.clone();
            let controls = self.controls;
            let draw =
                |ui: &mut Ui| overlays::skip(ui, ids.skip_group, ids.skip, &label, bottom, controls, ids.scrubber);
            let r = ui.opacity(skip_fade, |ui| if skip { draw(ui) } else { inert(ui, draw) });
            if r.activated {
                acts.push(Act::Skip);
            }
            if r.menu && !self.controls {
                acts.push(Act::VideoMenu);
            }
        }
        let keep_fade = ui.animate(ids.keep.with("fade"), if keep { 1.0 } else { 0.0 }, CORNER_FADE);
        if keep_fade > 0.0 || keep {
            let (n, controls) = (self.last_seconds, self.controls);
            let draw =
                |ui: &mut Ui| overlays::sleep_notice(ui, ids.keep_group, ids.keep, n, bottom, controls, ids.scrubber);
            let r = ui.opacity(keep_fade, |ui| if keep { draw(ui) } else { inert(ui, draw) });
            if r.activated {
                acts.push(Act::Keep);
            }
            if r.menu && !self.controls {
                acts.push(Act::VideoMenu);
            }
        }
        let card_fade = ui.animate(ids.card.with("fade"), if card && !error { 1.0 } else { 0.0 }, CORNER_FADE);
        if (card_fade > 0.0 || card && !error)
            && let Some(c) = &self.last_card
        {
            let shown = Card {
                heading: &c.heading,
                title: &c.title,
                image: c.image_url.as_deref().map(|u| pley_base::ImageRequest::new(u, 480)),
                seconds_left: c.seconds_left,
            };
            let reduced = ui.reduced_motion();
            let live = card && !error;
            let a = ui.opacity(card_fade, |ui| {
                let draw = |ui: &mut Ui| overlays::up_next(ui, ids.card, ids.play_now, ids.cancel, &shown, reduced);
                if live { draw(ui) } else { inert(ui, draw) }
            });
            if a.play_now {
                acts.push(Act::PlayNow);
            }
            if a.cancel {
                acts.push(Act::CancelCard);
            }
        }

        if !self.controls && self.seeker.badge_ms != 0 {
            overlays::seek_badge(ui, self.seeker.badge_ms);
        }
        // "Reconnecting…" shows under the ring only, once its delay has passed (`TvBufferingRing`).
        if let Some(since) = self.waiting_since
            && !error
        {
            let delay = if s.switching { HANDOFF_DELAY } else { BUFFERING_DELAY };
            let ring = LoadingRing::buffering().delay(delay).show(ui, RING_CENTRE, since);
            if ring && s.reconnecting {
                overlays::reconnecting(ui, RING_CENTRE.y + 20.0);
            }
        }

        self.draw_panels(ui, p, env, ids, &mut acts);

        if let Phase::Error { message, can_retry, title } = &s.phase {
            let title = if s.stopped_by_server {
                "Playback stopped"
            } else {
                title.as_deref().unwrap_or("Can't play this item")
            };
            ui.group(ids.error, Group::row().trap(), |ui| {
                let m = Message::new(MessageStyle::Player, title).body(message);
                let m = if *can_retry {
                    m.action("Try again", Icon::RefreshCw).secondary("Back")
                } else {
                    m.action("Back", Icon::ArrowLeft)
                };
                let r = m.show(ui, ids.error.with("message"));
                if r.action.is_some_and(|r| r.activated) {
                    acts.push(if *can_retry { Act::Retry } else { Act::Leave });
                }
                if r.secondary.is_some_and(|r| r.activated) {
                    acts.push(Act::Leave);
                }
            });
        }

        if let Some(t) = &self.toast
            && !overlays::toast(ui, t, self.controls)
        {
            self.toast = None;
        }
        acts
    }

    /// The top bar, the seek preview, the scrubber and the row.
    #[allow(clippy::too_many_arguments)]
    fn draw_controls(
        &self,
        ui: &mut Ui,
        p: &dyn Playback,
        env: &FrameEnv,
        ids: &Ids,
        skip: bool,
        preview: Option<&(SeekPreview, Option<Tile>)>,
        acts: &mut Vec<Act>,
    ) {
        let s = p.state();
        let clock = p.clock();
        let now = env.unix_now as i64;
        let line = text::episode_line(s.subtitle.as_deref());
        let time = clock::time_of_day(now, env.hours24);
        let ends = (s.phase == Phase::Paused).then(|| {
            let left = clock.duration_ms.saturating_sub(clock.position_ms).clamp(0, ENDS_AT_MAX_MS) / 1000;
            format!("Ends at {}", clock::time_of_day(now.saturating_add(left), env.hours24))
        });
        controls::top_bar(
            ui,
            &TopBar {
                title: &s.title,
                line: line.as_deref(),
                method: s.method.as_deref(),
                clock: &time,
                ends: ends.as_deref(),
            },
        );
        let chapter_at = self.strip.as_ref().and_then(|st| s.chapters.get(st.index)).map(|c| c.start_ms);
        if let Some(strip) = &self.strip {
            let r = chapters::show(ui, ids.chapters, strip, &s.chapters);
            if r.activated {
                acts.push(Act::Chapter(strip.index));
            }
        } else if let Some(t) = self.seeker.target {
            let (picture, tile) = preview.map_or((None, None), |(p, t)| (Some(p), t.as_ref()));
            controls::preview(ui, t, clock.duration_ms, picture, tile, &s.chapters);
        }
        let up = if skip { Edge::To(ids.skip) } else { Edge::Stop };
        let scrub = Scrub {
            position_ms: clock.position_ms,
            duration_ms: clock.duration_ms,
            buffered_ms: clock.buffered_ms,
            target_ms: chapter_at.or(self.seeker.target),
            chapters: &s.chapters,
            markers: &s.markers,
            up,
            up_opens_chapters: self.seeker.pending() && s.chapters.len() > 1,
            down: ids.button(Btn::PlayPause),
        };
        let r = controls::scrubber(ui, ids.scrub_group, ids.scrubber, &scrub);
        if r.activated {
            match r.pointer.filter(|_| r.clicked) {
                Some(at) => acts.push(Act::ScrubClick(((at.x - controls::BAR_X) / controls::BAR_W).clamp(0.0, 1.0))),
                None => acts.push(Act::ScrubOk),
            }
        }
        let options = RowOptions {
            previous_next: text::shows_previous_next(s.subtitle.as_deref(), s.chapters.len()),
            subtitles: !s.subtitles.is_empty() || (self.can_find && s.kind.is_some()),
            audio: !s.audio.is_empty(),
            quality: !s.qualities.is_empty(),
            chapters: s.chapters.len() > 1,
        };
        let button = |b: Btn| ids.button(b);
        if let Some(b) = controls::row(ui, ids.row, &button, &options, p.wants_to_play(), ids.scrubber) {
            acts.push(Act::Button(b));
        }
    }

    /// The seek preview for the seek on its way: the server's (its tile cut out of its sheet off
    /// the main thread, the sheet ahead of the scrub fetched early) or the chapter's picture.
    /// With no seek on its way the previews rest.
    fn seek_preview(&mut self, ui: &Ui, p: &dyn Playback) -> Option<(SeekPreview, Option<Tile>)> {
        let now = ui.time();
        let Some(t) = self.seeker.target else {
            if self.strip.is_some() {
                self.chapter_tiles(ui, p);
            } else {
                self.previews.idle(now);
            }
            return None;
        };
        let preview = p.seek_preview(t)?;
        let tile = match (&preview, &p.state().trickplay) {
            (SeekPreview::Sheet { url, x, y, width, height }, Some(tp)) => {
                let info = &tp.info;
                let forward = t >= p.clock().position_ms;
                let span = info.columns.clamp(1, SHEET_SIDE_MAX)
                    * info.rows.clamp(1, SHEET_SIDE_MAX)
                    * info.interval_ms.clamp(1, 3_600_000);
                let at = if forward { t.saturating_add(span) } else { t.saturating_sub(span).max(0) };
                let ahead = p.seek_preview(at);
                let ahead = match &ahead {
                    Some(SeekPreview::Sheet { url: next, .. }) if next != url => Some(next.as_str()),
                    _ => None,
                };
                let want = Want { sheet: url, x: *x, y: *y, width: *width, height: *height, columns: info.columns };
                self.previews.tile(&want, ahead, ui.tier(), now)
            }
            _ => None,
        };
        Some((preview, tile))
    }

    /// The previews of the chapters' cards near focus without a picture of their own, cut out of the
    /// focused card's sheet: cards on another sheet wait for focus, so the sheets kept stay few.
    fn chapter_tiles(&mut self, ui: &Ui, p: &dyn Playback) {
        let s = p.state();
        let (Some(strip), Some(tp)) = (self.strip.as_mut(), &s.trickplay) else { return };
        let now = ui.time();
        let step = tp.info.interval_ms.clamp(1, 3_600_000);
        let mut sheet: Option<String> = None;
        for i in strip.asking(s.chapters.len()) {
            let c = &s.chapters[i];
            if c.image_url.is_some() || strip.has_tile(i) {
                continue;
            }
            // Just after its start, as the server's preview at the start can be the chapter before.
            let Some(SeekPreview::Sheet { url, x, y, width, height }) = p.seek_preview(c.start_ms + step) else {
                continue;
            };
            match &sheet {
                None => sheet = Some(url.clone()),
                Some(first) if *first != url => continue,
                Some(_) => {}
            }
            let want = Want { sheet: &url, x, y, width, height, columns: tp.info.columns };
            if let Some(t) = self.previews.tile(&want, None, ui.tier(), now) {
                strip.keep_tile(i, t);
            }
        }
    }

    /// The narrow panels (one at a time, sliding out as they were) and Info's wide one.
    fn draw_panels(&mut self, ui: &mut Ui, p: &dyn Playback, env: &FrameEnv, ids: &Ids, acts: &mut Vec<Act>) {
        let s = p.state();
        let narrow = self.panel.filter(|k| *k != Panel::Info);
        if narrow == Some(Panel::Subtitles) && self.fonts.is_none() {
            self.fonts = Some(subtitles::fonts_offered(ui));
        }
        // The open panel's rows are made each frame (what they say can change); a panel sliding
        // out keeps the rows it had.
        if let Some(k) = narrow {
            let input = PanelInput {
                state: s,
                settings: env.settings,
                fonts: self.fonts.as_deref().unwrap_or_default(),
                can_find: self.can_find,
                found: &self.found,
                add_failed: self.add_failed.as_deref(),
            };
            self.rows = panels::rows(k, &input);
            self.last_narrow = narrow;
        }
        let kind = self.last_narrow;
        let rows = &self.rows;
        let fresh = self.fresh.take().filter(|_| narrow.is_some());
        let list = &mut self.list;
        let mut answer = None;
        let r = SidePanel::new().back_closes(false).show(ui, ids.panel, narrow.is_some(), |ui, area| {
            let Some(k) = kind else { return };
            let list_id = ids.panel.with(k);
            if let Some(f) = fresh {
                let at = match f {
                    Fresh::Find => rows.iter().position(|r| matches!(r, Row::Choice { pick: Pick::Find, .. })),
                    Fresh::Initial => None,
                }
                .unwrap_or_else(|| panels::initial(rows));
                *list = ListState::at(at);
                ui.request_focus(list_id.with(at as u64));
            }
            answer = Some(panels::draw(ui, list_id, area, rows, list));
        });
        if narrow.is_none() && r.progress <= 0.0 {
            self.last_narrow = None;
            self.rows = Vec::new();
        }
        if r.closed {
            acts.push(Act::ClosePanel);
        }
        if let Some(a) = answer {
            if let Some(pick) = a.pick {
                acts.push(Act::Pick(pick));
            }
            if let Some(step) = a.sync {
                acts.push(Act::Sync(step));
            }
            if a.sync_reset {
                acts.push(Act::SyncReset);
            }
        }

        let info = self.panel == Some(Panel::Info);
        if info {
            self.sections = panels::info_sections(s, &p.engine_stats());
        }
        let (sections, scroll) = (&self.sections, &mut self.info_scroll);
        let r = SidePanel::wide().back_closes(false).show(ui, ids.wide, info, |ui, area| {
            panels::info(ui, ids.info, area, sections, scroll);
        });
        if info {
            ui.repaint_after(1.0);
        } else if r.progress <= 0.0 {
            self.sections = Vec::new();
        }
        if r.closed {
            acts.push(Act::ClosePanel);
        }
    }

    // ---------------------------------------------------------------- what was done

    fn act(&mut self, ui: &mut Ui, p: &mut dyn Playback, now: i64, a: Act, out: &mut Out) {
        let busy = {
            let s = p.state();
            s.phase == Phase::Preparing || s.switching
        };
        ui.request_repaint();
        match a {
            // OK with the controls hidden confirms a seek coming, else shows the controls on the
            // scrubber as Up does; it never pauses there.
            Act::VideoOk if self.seeker.pending() => self.play_pause(p, now),
            Act::VideoOk => self.show(Target::Scrubber),
            Act::Chapter(i) => {
                // A chapter chosen: it goes to its start, and the cards go.
                if let Some(c) = p.state().chapters.get(i) {
                    let start = c.start_ms;
                    self.seeker.cancel();
                    p.seek_to(start);
                    self.confirm_until = now + CONFIRM_MS;
                }
                self.close_chapters();
            }
            Act::ScrubOk if !busy => self.play_pause(p, now),
            Act::ScrubOk => {}
            Act::VideoMenu => {
                let skip = self.skip_shown(p.state());
                self.show(if skip { Target::Skip } else { Target::Scrubber });
            }
            // A seek still waiting for OK goes, or it would land after these and undo them.
            Act::ScrubClick(f) => {
                self.seeker.cancel();
                p.seek_to((f as f64 * p.clock().duration_ms as f64) as i64);
            }
            Act::Button(b) => self.button(p, now, b),
            Act::Skip => {
                self.seeker.cancel();
                p.skip();
            }
            Act::Keep => p.keep_watching(),
            Act::PlayNow => p.play_next(),
            Act::CancelCard => p.cancel_up_next(),
            Act::Retry => p.retry(),
            Act::Leave => out.leave = true,
            Act::Pick(pick) => self.pick(p, pick, out),
            Act::Sync(step) => {
                let offset = i64::from(p.state().subtitle_offset_ms);
                p.set_subtitle_offset(offset + i64::from(step) * i64::from(pley_subs::SYNC_STEP_MS));
            }
            Act::SyncReset => p.set_subtitle_offset(0),
            Act::ClosePanel => self.close_panel(),
            Act::Details(open) => p.set_details_open(open),
        }
    }

    fn button(&mut self, p: &mut dyn Playback, now: i64, b: Btn) {
        let c = p.clock();
        match b {
            Btn::PlayPause => self.play_pause(p, now),
            Btn::Back10 => self.seeker.press(now, -1, STEP_MS, SETTLE_MS, c.position_ms, c.duration_ms),
            Btn::Forward10 => self.seeker.press(now, 1, STEP_MS, SETTLE_MS, c.position_ms, c.duration_ms),
            Btn::Previous => self.previous(p),
            Btn::Next => self.next(p),
            Btn::Chapters => self.open_chapters(c.position_ms, &p.state().chapters, true),
            other => {
                if let Some(panel) = Panel::of(other) {
                    self.open(panel);
                }
            }
        }
    }

    fn pick(&mut self, p: &mut dyn Playback, pick: Pick, out: &mut Out) {
        match &pick {
            Pick::Subtitle(i) => p.select_subtitle(*i),
            Pick::Find => {
                self.open(Panel::Find);
                p.find_subtitles();
            }
            Pick::Size(v) => out.style = Some(Style::Size(*v)),
            Pick::Background(v) => out.style = Some(Style::Background(*v)),
            Pick::Font(v) => out.style = Some(Style::Font(*v)),
            Pick::Position(v) => out.style = Some(Style::Position(*v)),
            Pick::Result(n) => {
                if let Some(r) = self.found.get(*n).cloned() {
                    self.add_failed = None;
                    p.add_found_subtitle(&r);
                    // Known now rather than when the next frame sees it: a server close by can
                    // answer before then, and the search would close without having been seen.
                    self.seen.adding = matches!(p.state().search, SubtitleSearch::Adding(_));
                }
            }
            Pick::Audio(i) => p.select_audio(*i),
            Pick::PlayOriginal => p.play_original(),
            Pick::Quality(id) => p.select_quality(id),
            Pick::Sleep(c) => p.set_sleep(*c),
            Pick::Speed(r) => p.set_speed(*r),
        }
        if !pick.keeps_open() {
            self.close_panel();
        }
    }

    // ---------------------------------------------------------------- time

    /// The controls hide 5 s after the last key unless paused, in a panel or seeking; the next
    /// frame is asked for when anything is next due.
    fn timers(&mut self, ui: &mut Ui, p: &dyn Playback, now: i64, env: &FrameEnv) {
        let s = p.state();
        if self.controls && self.panel.is_none() && self.strip.is_none() && p.wants_to_play() && !self.seeker.pending()
        {
            let at = self.last_input + HIDE_AFTER_MS;
            if now >= at {
                let skip = self.skip_shown(s);
                self.controls = false;
                self.ask = Some(hidden_focus(skip));
                ui.request_repaint();
            } else {
                ui.repaint_after(secs(at - now));
            }
        }
        let due = [
            p.next_wake(),
            self.seeker.next_due(),
            self.stopped_at.map(|t| t + STOPPED_LEAVE_MS),
            self.fired_at.map(|t| t + pley_player::sleep::LEAVE_AFTER_MS),
            (self.banner_until > now).then_some(self.banner_until),
        ];
        if let Some(t) = due.into_iter().flatten().min() {
            ui.repaint_after(secs(t - now));
        }
        // The decoded preview sheet is let go a while after the scrub.
        if let Some(t) = self.previews.next_due() {
            ui.repaint_after((t - ui.time()).max(0.0));
        }
        if self.controls {
            ui.repaint_after(clock::to_next_minute(env.unix_now) + 0.05);
        }
    }
}

/// Where focus goes as the controls hide (`hideControls(if (skipShown) Skip else Video)`): Skip,
/// else the holder of the keys. The sleep notice takes focus only as it comes.
fn hidden_focus(skip: bool) -> Target {
    if skip { Target::Skip } else { Target::Video }
}

/// Whether the TV's screen saver is kept away (`TvPlayerActivity.kt`: the screen is kept on while
/// the phase wants to play): while the video plays or is on its way to, never while it is paused,
/// at its end or stopped by an error, so no still picture is held on the panel.
pub fn keeps_awake(phase: &Phase, wants_to_play: bool) -> bool {
    wants_to_play && matches!(phase, Phase::Preparing | Phase::Buffering | Phase::Playing)
}
