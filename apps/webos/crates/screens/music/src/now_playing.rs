//! Now Playing (`TvNowPlayingScreen.kt`; the TV app's `NowPlayingScreen`): the album art faint
//! behind the page; the cover; the title, "Artist · Album" and a status line; the scrubber with
//! its times; Shuffle, Previous, Play/Pause, Next and the sleep timer; and the queue, "Up next".
//! Focus starts on Play/Pause. The scrubber takes Left and Right (10 s) and OK (play or pause);
//! the remote's media keys work anywhere on the page. The music plays on when the page is left.
//! The shell dims the page after two minutes without a key, as Android's does.

use std::hash::{Hash, Hasher};

use pley_base::{Icon, ImageRequest, Key, Point, Rect, Size, TextOpts};
use pley_core::format::format_clock;
use pley_core::model::{ImageKind, ItemSummary};
use pley_i18n::t;
use pley_kit::{RoundButton, SCROLL, TrackLine, backdrop_px, look};
use pley_player::sleep::SleepState;
use pley_shell::{AppCx, Chrome, Screen};
use pley_ui::theme::focus::{self, ControlLook};
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Dirs, Edge, FocusOpts, Group, Id, Ui, nudge};

use crate::pages::{self, QUEUE_LINE, QUEUE_PITCH, SEEK_STEP_MS};
use crate::player::{self, MusicPlayer};
use crate::widgets::{FAINT_ART_PX, art_well, faint_art, message, register};

/// The cover: 300 dp at (96, 58), 8 dp corners; the column 32 dp to its right, to the safe edge.
const ART: Rect = Rect::new(theme::GUTTER, 58.0, 300.0, 300.0);
const COLUMN_X: f32 = theme::GUTTER + 300.0 + 32.0;
const COLUMN_W: f32 = theme::CANVAS_W - theme::SAFE_X - COLUMN_X;
/// The faint art's alpha on Now Playing.
const FAINT: f32 = 0.2;
/// The status line's box and the text's place in it.
const STATUS_HEIGHT: f32 = 22.0;
/// The scrubber: a 24 dp box with a 4 dp track through its middle; the thumb 10 dp, 12 focused,
/// with a 2 dp AccentBright ring 1 dp clear of it.
const SCRUB_HEIGHT: f32 = 24.0;
const SCRUB_TRACK: f32 = 4.0;
const THUMB: f32 = 10.0;
const THUMB_FOCUSED: f32 = 12.0;
const RING_GAP: f32 = 1.0;
const RING: f32 = 2.0;
/// The controls' row: 64 dp and the round buttons' labels; the round buttons 8 dp down, 16 apart.
const CONTROLS_HEIGHT: f32 = 64.0 + theme::BUTTON_LABEL_ROOM;
const CONTROLS_GAP: f32 = 16.0;
const ROUND_DROP: f32 = 8.0;
const PLAY: f32 = 64.0;
const PLAY_GLYPH: f32 = 32.0;
/// Queue lines drawn beyond the box's edges.
const QUEUE_OVERSCAN: usize = 3;
/// How far from the line focused last frame the focused line is looked for first.
const FOCUS_NEAR: usize = 3;

/// Play/Pause: Fg 15 %, Accent and OnAccent focused at 1.08, AccentBright pressed.
const PLAY_LOOK: ControlLook = ControlLook {
    fill: theme::alpha(theme::FG, 0.15),
    content: theme::FG,
    edge: None,
    focus_fill: theme::ACCENT,
    focus_content: theme::ON_ACCENT,
    pressed_fill: theme::ACCENT_BRIGHT,
    focus_scale: focus::ROUND.focus_scale,
};

/// What the page shows of a track that changes only with the track.
struct TrackArt {
    key: (i64, u32, u32),
    faint: Vec<ImageRequest>,
    cover: Vec<ImageRequest>,
    title: String,
    byline: String,
    /// What picks the placeholder's gradient: the album, else the title.
    seed: String,
}

impl TrackArt {
    fn new(cx: &AppCx, track: &ItemSummary, key: (i64, u32, u32)) -> TrackArt {
        let at = |w: u32| -> Vec<ImageRequest> {
            match (&track.poster_url, track.parent_id) {
                (Some(p), _) => cx.image(p, w).into_iter().collect(),
                (None, Some(album)) => cx.item_image(album, ImageKind::Poster, None, w).into_iter().collect(),
                (None, None) => Vec::new(),
            }
        };
        TrackArt {
            key,
            faint: at(key.1),
            cover: at(key.2),
            title: track.title.clone(),
            byline: pages::byline(track),
            seed: track.parent_title.clone().unwrap_or_else(|| track.title.clone()),
        }
    }
}

/// What a queue line says, worked out once per queue.
struct QueueText {
    byline: String,
    length: String,
    number: String,
}

/// The Now Playing page.
#[derive(Default)]
pub struct NowPlayingScreen {
    art: Option<TrackArt>,
    queue: Vec<QueueText>,
    /// The music's version the queue's texts were checked at, and the queue they were made for.
    queue_version: Option<u64>,
    queue_key: u64,
    /// Where the queue is heading: how far its lines are moved up.
    queue_shift: f32,
    /// The track playing and the queue the queue was last brought to: it moves to the track
    /// playing only when that changes (Android's `LaunchedEffect(current)`).
    queue_at: Option<(usize, u64)>,
    /// The queue's line focused last frame.
    queue_focus: Option<usize>,
    /// The times under the scrubber, for the whole seconds they show.
    times: Option<((i64, i64), String, String)>,
    sleep_label: Option<(SleepState, String)>,
}

impl NowPlayingScreen {
    /// The page.
    pub fn new() -> NowPlayingScreen {
        NowPlayingScreen::default()
    }

    fn queue_texts(&mut self, version: u64, list: &[ItemSummary]) {
        if self.queue_version == Some(version) {
            return;
        }
        self.queue_version = Some(version);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for t in list {
            t.id.hash(&mut h);
        }
        let key = h.finish();
        if key == self.queue_key && self.queue.len() == list.len() {
            return;
        }
        self.queue_key = key;
        self.queue = list
            .iter()
            .enumerate()
            .map(|(i, t)| QueueText {
                byline: pages::byline(t),
                length: pages::track_length(t),
                number: (i + 1).to_string(),
            })
            .collect();
    }

    fn times(&mut self, position: i64, duration: i64) -> (&str, &str) {
        let left = (duration - position).max(0);
        let key = (position / 1000, left / 1000);
        if self.times.as_ref().is_none_or(|t| t.0 != key) {
            self.times = Some((key, format_clock(Some(position)), format!("−{}", format_clock(Some(left)))));
        }
        let (_, played, left) = self.times.as_ref().expect("the times were just made");
        (played, left)
    }

    fn sleep_label(&mut self, sleep: SleepState) -> &str {
        if self.sleep_label.as_ref().is_none_or(|s| s.0 != sleep) {
            self.sleep_label = Some((sleep, pages::sleep_label(&sleep)));
        }
        &self.sleep_label.as_ref().expect("the label was just made").1
    }
}

impl Screen for NowPlayingScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let Some(id) = player::music(cx).and_then(|m| m.now_playing()).map(|n| n.track.id) else {
            let (title, body) = (t!("music.nowPlaying.nothingTitle"), t!("music.nowPlaying.nothingBody"));
            if message(ui, title, body, t!("music.actions.back")).activated {
                cx.back();
            }
            return;
        };
        let key = (id, backdrop_px(ui).min(FAINT_ART_PX), ui.image_px(ART.w));
        if self.art.as_ref().is_none_or(|a| a.key != key) {
            let track = player::music(cx).and_then(|m| m.now_playing()).map(|n| n.track.clone());
            self.art = track.map(|t| TrackArt::new(cx, &t, key));
        }
        let Some(m) = player::music(cx) else { return };
        let Some(n) = m.now_playing() else { return };
        let (index, count, shuffle, playing) = (n.index, n.count, n.shuffle, n.playing);
        self.queue_texts(m.version(), m.list());
        let Some(art) = self.art.take() else { return };

        faint_art(ui, &art.faint, &art.title, FAINT);
        art_well(ui, ART, &art.cover, &art.seed, radius::DIALOG);

        // The title, the byline and the status line.
        let x = COLUMN_X;
        let mut y = ART.y;
        let title_opts = TextOpts::wrap(COLUMN_W, 2).balanced();
        y += ui.text(Point::new(x, y), &art.title, &theme::PAGE_TITLE, &title_opts, theme::FG).h;
        y += 4.0;
        ui.text(Point::new(x, y), &art.byline, &theme::BODY_LARGE, &TextOpts::line(COLUMN_W), theme::MUTED);
        y += theme::BODY_LARGE.line_height;
        let (status, failed) = m.status();
        if !status.is_empty() {
            let color = if failed { theme::DANGER_TEXT } else { theme::SUBTLE };
            ui.text(Point::new(x, y + 4.0), &status, &theme::CAPTION, &TextOpts::line(COLUMN_W), color);
        }
        y += STATUS_HEIGHT + 14.0;
        self.art = Some(art);

        // The scrubber and its times.
        let (position, duration, buffered) = (m.position_ms(), m.duration_ms(), m.buffered_ms());
        let scrub = Rect::new(x, y, COLUMN_W, SCRUB_HEIGHT);
        let id = ui.id("scrubber");
        let r = register(ui, id, scrub, FocusOpts::default().takes(Dirs::HORIZONTAL).click_at());
        if r.focused {
            let mut to = None;
            for k in ui.keys().iter().filter(|k| k.pressed) {
                let step = match k.key {
                    Key::Left => -SEEK_STEP_MS,
                    Key::Right => SEEK_STEP_MS,
                    _ => continue,
                };
                to = Some(pages::seek_target(to.unwrap_or(position), step, duration));
            }
            if let Some(to) = to {
                m.seek_to(to);
            }
        }
        if r.activated {
            match r.pointer.filter(|_| r.clicked && duration > 0) {
                // The Magic Remote's click seeks where it points.
                Some(p) => m.seek_to((((p.x - scrub.x) / scrub.w).clamp(0.0, 1.0) as f64 * duration as f64) as i64),
                None => m.toggle(),
            }
        }
        let p = look::focus_progress(ui, id, r.focused);
        scrubber(ui, scrub, position, duration, buffered, p);
        y += SCRUB_HEIGHT + 4.0;
        let (played, left) = self.times(position, duration);
        ui.text(Point::new(x, y), played, &theme::CAPTION, &TextOpts::default(), theme::FG);
        let w = ui.measure(left, &theme::CAPTION, &TextOpts::default()).w;
        ui.text(Point::new(scrub.right() - w, y), left, &theme::CAPTION, &TextOpts::default(), theme::FG);
        y += theme::CAPTION.line_height + 16.0;

        // Shuffle, Previous, Play/Pause, Next, the sleep timer.
        let controls = ui.id("controls");
        let pause = m.shows_pause();
        let sleep_text = self.sleep_label(m.sleep());
        ui.group(controls, Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop), |ui| {
            let mut bx = x;
            let round = |ui: &mut Ui, bx: &mut f32, key: &str, b: RoundButton| {
                let r = b.on_art(true).show(ui, ui.id(key), Point::new(*bx, y + ROUND_DROP));
                *bx += b.size().w + CONTROLS_GAP;
                r.activated
            };
            let shuffle_label = if shuffle { t!("music.actions.shuffleOff") } else { t!("music.actions.shuffle") };
            if round(ui, &mut bx, "shuffle", RoundButton::new(Icon::Shuffle, shuffle_label)) {
                m.set_shuffle(!shuffle);
            }
            if round(ui, &mut bx, "previous", RoundButton::new(Icon::SkipBack, t!("music.actions.previous"))) {
                m.previous();
            }
            let play = ui.id("play");
            ui.initial_focus(play);
            if play_pause(ui, play, Rect::new(bx, y, PLAY, PLAY), pause) {
                m.toggle();
            }
            bx += PLAY + CONTROLS_GAP;
            let next = RoundButton::new(Icon::SkipForward, t!("music.actions.next")).enabled(index + 1 < count);
            if round(ui, &mut bx, "next", next) {
                m.next();
            }
            if round(ui, &mut bx, "sleep", RoundButton::new(Icon::Moon, sleep_text)) {
                m.step_sleep();
            }
        });
        y += CONTROLS_HEIGHT;

        // The queue.
        if m.list().len() > 1 {
            let up_next = t!("music.nowPlaying.upNext");
            ui.text(Point::new(x + 12.0, y), up_next, &theme::LABEL, &TextOpts::line(COLUMN_W), theme::SUBTLE);
            y += theme::LABEL.line_height + 4.0;
            let view = Rect::new(x, y, COLUMN_W, (theme::CANVAS_H - y).max(QUEUE_PITCH));
            if let Some(i) = self.queue(ui, view, m, index, playing) {
                m.jump_to(i);
            }
        }

        if playing {
            ui.repaint_after(pages::next_second(position));
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }
}

impl NowPlayingScreen {
    /// The line of the queue `id` holding focus: looked for around last frame's (or the track
    /// playing) first, so a long queue isn't searched every frame.
    fn focused_line(&self, ui: &Ui, id: Id, n: usize, current: usize) -> Option<usize> {
        let f = ui.focused().filter(|_| ui.focused_in(id).is_some())?;
        let last = self.queue_focus.unwrap_or(current).min(n.saturating_sub(1));
        let near = last.saturating_sub(FOCUS_NEAR)..(last + FOCUS_NEAR + 1).min(n);
        near.clone()
            .find(|&i| id.with(i) == f)
            .or_else(|| (0..n).filter(|i| !near.contains(i)).find(|&i| id.with(i) == f))
    }

    /// The queue in `view`: lines at a fixed pitch, only those near the box drawn, so a queue of
    /// hundreds moves as one of ten does. The line OK picked.
    fn queue(&mut self, ui: &mut Ui, view: Rect, m: &MusicPlayer, index: usize, playing: bool) -> Option<usize> {
        let list = m.list();
        let id = ui.id("queue");
        let n = list.len();
        let max = pages::queue_max(n, view.h);
        let focused = self.focused_line(ui, id, n, index);
        self.queue_focus = focused;
        self.queue_shift = match focused {
            // The pointer leaves the queue where it is, but for a line cut off.
            Some(f) if ui.pointer_mode() => {
                let lo = f as f32 * QUEUE_PITCH;
                nudge(self.queue_shift, lo, lo + QUEUE_LINE, 0.0, view.h, 0.0, max)
            }
            Some(f) => pages::queue_shift(f, n, view.h, true),
            None if self.queue_at != Some((index, self.queue_key)) => pages::queue_shift(index, n, view.h, false),
            None => self.queue_shift.min(max),
        };
        self.queue_at = Some((index, self.queue_key));
        let shown = ui.animate(id.with("scroll"), self.queue_shift, SCROLL);

        let first = ((shown / QUEUE_PITCH).floor() as usize).saturating_sub(QUEUE_OVERSCAN);
        let past = (first + (view.h / QUEUE_PITCH).ceil() as usize + 2 * QUEUE_OVERSCAN + 1).min(n);
        let around = focused.map_or(first..first, |f| f.saturating_sub(1)..(f + 2).min(n));
        // Coming back down lands on the line focused last, as Android's `focusRestorer` does, when
        // it is still drawn; else on the track playing.
        let mut group = Group::column();
        let remembered = ui.remembered(id).is_some_and(|r| (first..past).any(|i| id.with(i) == r));
        if !remembered {
            group = group.enter_at(id.with(index.min(n - 1)));
        }
        let mut picked = None;
        ui.clip(view, |ui| {
            ui.group(id, group, |ui| {
                let lines = (first..past).chain(around.filter(|i| *i < first || *i >= past));
                for i in lines {
                    let (t, text) = (&list[i], &self.queue[i]);
                    let rect = Rect::new(view.x, view.y + i as f32 * QUEUE_PITCH - shown, view.w, QUEUE_LINE);
                    let line = TrackLine {
                        number: &text.number,
                        title: &t.title,
                        subtitle: (!text.byline.is_empty()).then_some(text.byline.as_str()),
                        duration: &text.length,
                        current: i == index,
                        playing: i == index && playing,
                        highlighted: false,
                        ..TrackLine::default()
                    };
                    if line.show(ui, id.with(i), rect).activated {
                        picked = Some(i);
                    }
                }
            });
        });
        picked
    }
}

/// The scrubber in `rect`: the track (Fg 25 %), what is loaded (Fg 45 %) and what played (Accent),
/// and the thumb, growing with `focus` (0 to 1) and ringed while focused.
fn scrubber(ui: &mut Ui, rect: Rect, position: i64, duration: i64, buffered: i64, focus: f32) {
    let fraction = |ms: i64| if duration > 0 { (ms as f32 / duration as f32).clamp(0.0, 1.0) } else { 0.0 };
    let track = Rect::new(rect.x, rect.center().y - SCRUB_TRACK / 2.0, rect.w, SCRUB_TRACK);
    let corner = SCRUB_TRACK / 2.0;
    ui.fill(track, theme::alpha(theme::FG, 0.25), corner);
    let loaded = fraction(buffered);
    if loaded > 0.0 {
        ui.fill(Rect::new(track.x, track.y, track.w * loaded, track.h), theme::alpha(theme::FG, 0.45), corner);
    }
    let played = fraction(position);
    if played > 0.0 {
        ui.fill(Rect::new(track.x, track.y, track.w * played, track.h), theme::ACCENT, corner);
    }
    let side = THUMB + (THUMB_FOCUSED - THUMB) * focus;
    let cx = (track.x + track.w * played).clamp(track.x + side / 2.0, track.right() - side / 2.0);
    let centre = Point::new(cx, track.center().y);
    let thumb = Rect::from_origin_size(Point::new(centre.x - side / 2.0, centre.y - side / 2.0), Size::new(side, side));
    ui.fill(thumb, theme::FG, side / 2.0);
    if focus > 0.0 {
        let reach = THUMB_FOCUSED / 2.0 + RING_GAP + RING / 2.0;
        let ring = Rect::new(centre.x - reach, centre.y - reach, reach * 2.0, reach * 2.0);
        ui.stroke(ring, theme::ACCENT_BRIGHT.alpha(focus), reach, RING);
    }
}

/// Play/Pause: a 64 dp circle with a 32 dp glyph (`PlayPause`); true when selected.
fn play_pause(ui: &mut Ui, id: Id, rect: Rect, pause: bool) -> bool {
    // Audio Guidance names it by what it does, as the round buttons beside it are named by their
    // labels.
    ui.name(id, || if pause { t!("music.actions.pause") } else { t!("music.actions.play") }.to_string());
    let r = register(ui, id, rect, FocusOpts::default());
    let s = look::shade(ui, &PLAY_LOOK, &r);
    ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
        ui.fill(rect, s.fill, rect.w / 2.0);
        let glyph = rect.place(Size::new(PLAY_GLYPH, PLAY_GLYPH), 0.5, 0.5);
        ui.icon(glyph, if pause { Icon::Pause } else { Icon::Play }, s.content);
    });
    r.activated
}
