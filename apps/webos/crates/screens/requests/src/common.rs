//! What Discover, a title's page and the requests share (the TV app's `request-kit.tsx`): the
//! TV's clock for words about time, the colours of a request's tones, the stage pill and a
//! download's bar, a title's badge on its poster, the tiles of titles, seasons and requests,
//! asking and following, the badges titles' details left, the pages that say why requests
//! can't be used, signing in again, and asking again on a timer.

use std::cell::RefCell;
use std::collections::HashMap;

use pley_base::{Color, Icon, ImageRequest, Point, Rect, Size, TextOpts};
use pley_core::addresses::display_address;
use pley_core::model::{DiscoverDetail, DiscoverTitle, MediaRequest, NewRequest, RequestSummary, TitleStatus};
use pley_core::requests::{
    ActionId, RequestsProblem, StageWords, Tone, TvTime, action_done, ask_refusal, asked_text, library_item,
    problem_words, requests_problem, requests_title, title_key,
};
use pley_core::{ApiError, Promise};
use pley_i18n::t;
use pley_kit::{Dialog, DialogButton, Message, MessageStyle, SlotMode, Tile, TileKind, TileSlot, tile_look};
use pley_shell::{AppCx, Route};
use pley_ui::theme::{self, radius};
use pley_ui::{Id, Ui};

/// Waiting for a decision: the TV app's `--warning`.
pub const WARNING: Color = Color::rgb(0xf2b544);

/// A tone's colour, for words and the stage's dot.
pub fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Amber => WARNING,
        Tone::Blue => theme::INFO,
        Tone::Green => theme::SUCCESS,
        Tone::Red => theme::DANGER_TEXT,
        Tone::Gray => theme::MUTED,
    }
}

/// A pill or badge in a tone: the tone at 20 % (grey 16 %) over art's Bg at 80 %, so it reads on
/// posters and pages alike.
fn tone_fill(ui: &mut Ui, rect: Rect, tone: Tone, corner: f32) {
    ui.fill(rect, theme::ON_ART, corner);
    let tint = if tone == Tone::Gray { theme::MUTED.alpha(0.16) } else { tone_color(tone).alpha(0.2) };
    ui.fill(rect, tint, corner);
}

/// The TV's clock for words about time: now, its zone's offset, and its 12- or 24-hour format
/// (asked of the TV once, again when the page shows again).
#[derive(Default)]
pub struct TvClock {
    hours24: Option<bool>,
}

impl TvClock {
    pub fn time(&mut self, cx: &AppCx) -> TvTime {
        let hours24 = *self.hours24.get_or_insert_with(|| cx.device().clock_24h());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        TvTime::new(now, pley_live::utc_offset(now), hours24)
    }

    /// The TV's format is asked for again (the page shows again).
    pub fn forget(&mut self) {
        self.hours24 = None;
    }
}

// ---- the stage pill and a download's bar

/// A stage pill's measures: height, padding before the dot and after the words, the dot and
/// the gap after it.
fn pill_spec(small: bool) -> (f32, f32, f32, f32, f32) {
    if small { (22.0, 8.0, 10.0, 6.0, 6.0) } else { (26.0, 10.0, 12.0, 8.0, 8.0) }
}

/// A stage pill's size, its words cut to fit `max_w`.
pub fn pill_size(ui: &Ui, words: &StageWords, small: bool, max_w: f32) -> Size {
    let (h, before, after, dot, gap) = pill_spec(small);
    let style = if small { theme::CAPTION } else { theme::LABEL };
    let room = (max_w - before - dot - gap - after).max(0.0);
    let w = ui.measure(&words.text, &style, &TextOpts::line(room)).w;
    Size::new(before + dot + gap + w + after, h)
}

/// A request's stage (`StagePill`): a dot and the words in the stage's colour, on its tone.
pub fn stage_pill(ui: &mut Ui, at: Point, words: &StageWords, small: bool, max_w: f32) -> Size {
    let (h, before, after, dot, gap) = pill_spec(small);
    let style = if small { theme::CAPTION } else { theme::LABEL };
    let size = pill_size(ui, words, small, max_w);
    let rect = Rect::from_origin_size(at, size);
    tone_fill(ui, rect, words.tone, h / 2.0);
    let color = tone_color(words.tone);
    ui.fill(Rect::new(at.x + before, at.y + (h - dot) / 2.0, dot, dot), color, dot / 2.0);
    let room = (size.w - before - dot - gap - after).max(1.0);
    let layout = ui.layout(&words.text, &style, &TextOpts::line(room));
    let y = at.y + (h - layout.size.h) / 2.0;
    ui.draw_text(Point::new(at.x + before + dot + gap, y), layout, color);
    size
}

/// How much of a download is in (`DownloadBar`): 4 high, blue on a faint track.
pub fn download_bar(ui: &mut Ui, rect: Rect, progress: f64) {
    ui.fill(rect, theme::FG.alpha(0.16), 2.0);
    let p = progress.clamp(0.0, 1.0) as f32;
    if p > 0.0 {
        ui.fill(Rect::new(rect.x, rect.y, rect.w * p, rect.h), theme::INFO, 2.0);
    }
}

/// A stage with a download's bar 4 under it, as wide as its pill (`StageBlock`), from `at`
/// (`right`: `at` is its right edge). Its size.
pub fn stage_block(ui: &mut Ui, at: Point, words: &StageWords, max_w: f32, right: bool) -> Size {
    let size = pill_size(ui, words, false, max_w);
    let x = if right { at.x - size.w } else { at.x };
    stage_pill(ui, Point::new(x, at.y), words, false, max_w);
    match words.progress {
        Some(p) => {
            download_bar(ui, Rect::new(x, at.y + size.h + 4.0, size.w, 4.0), p);
            Size::new(size.w, size.h + 8.0)
        }
        None => size,
    }
}

/// A title's badge on its poster (`title-badge`): at its upper left, its words in the tone.
pub fn title_badge(ui: &mut Ui, well: Rect, text: &str, tone: Tone) {
    let room = (well.w - 12.0 - 10.0).max(1.0);
    let layout = ui.layout(text, &theme::BADGE, &TextOpts::line(room));
    let rect = Rect::new(well.x + 6.0, well.y + 6.0, layout.size.w + 10.0, 18.0);
    tone_fill(ui, rect, tone, radius::BADGE);
    let y = rect.y + (rect.h - layout.size.h) / 2.0;
    ui.draw_text(Point::new(rect.x + 5.0, y), layout, tone_color(tone));
}

// ---- tiles

/// A title from TMDB as a poster (`TitleTile`), worked out once: its picture, its badge.
#[derive(Clone, Debug)]
pub struct TitleTileModel {
    pub title: DiscoverTitle,
    pub subtitle: String,
    pub art: Vec<ImageRequest>,
    pub badge: Option<&'static str>,
    pub tone: Tone,
}

impl TitleTileModel {
    pub fn new(cx: &AppCx, ui: &Ui, title: DiscoverTitle, mixed: bool) -> TitleTileModel {
        let subtitle = pley_core::requests::title_subtitle(title.kind, title.year, mixed);
        let art = art(cx, ui, title.poster_url.as_deref(), TileKind::Poster.size().w);
        let badge = pley_core::requests::title_badge(title.status, title.item_id);
        let tone = pley_core::requests::title_tone(title.status, title.item_id);
        TitleTileModel { title, subtitle, art, badge, tone }
    }

    pub fn key(&self) -> String {
        title_key(self.title.kind, self.title.tmdb_id)
    }
}

/// Where a title's poster opens: its library title when a library this account can open has it
/// whole ([`library_item`]), else its Discover page.
pub fn title_page(t: &DiscoverTitle) -> Route {
    match library_item(t) {
        Some(id) => Route::Item { id },
        None => Route::Title { kind: t.kind, tmdb_id: t.tmdb_id, pick: false },
    }
}

/// A server's picture path as requests at `width` dp.
pub fn art(cx: &AppCx, ui: &Ui, path: Option<&str>, width: f32) -> Vec<ImageRequest> {
    path.filter(|p| !p.is_empty()).and_then(|p| cx.image(p, ui.image_px(width))).into_iter().collect()
}

/// Draws an overlay on a tile's well as the tile grows with focus.
fn on_well(ui: &mut Ui, slot: &TileSlot, draw: impl FnOnce(&mut Ui, Rect)) {
    let look = tile_look(ui, slot.id, slot.kind, &slot.response);
    let rect = slot.rect;
    ui.transform(Point::ZERO, look.scale, rect.center(), |ui| draw(ui, rect));
}

/// Draws a title's poster in its slot, its badge on it.
pub fn paint_title(ui: &mut Ui, slot: &TileSlot, t: &TitleTileModel) {
    let tile = Tile::new(TileKind::Poster, &t.title.title).subtitle(&t.subtitle).art(&t.art);
    tile.paint(ui, slot);
    if slot.mode == SlotMode::Draw
        && let Some(badge) = t.badge
    {
        on_well(ui, slot, |ui, rect| title_badge(ui, rect, badge, t.tone));
    }
}

/// A tile whose caption's second line is in a tone (a request's stage), on up to `lines` lines
/// (a download's time on the second: "Downloading 70%" / "in 2 hours").
pub struct ToneTile<'a> {
    pub kind: TileKind,
    pub title: &'a str,
    pub line: &'a str,
    pub tone: Option<Tone>,
    pub lines: usize,
    pub art: &'a [ImageRequest],
    pub progress: Option<f64>,
    pub badge: Option<(&'a str, Tone)>,
    /// News since the requests were last looked at: an accent dot.
    pub news: bool,
}

impl ToneTile<'_> {
    pub fn paint(&self, ui: &mut Ui, slot: &TileSlot) {
        let progress = self.progress.map(|p| p.clamp(0.0, 1.0) as f32).filter(|p| *p > 0.0);
        let tile = Tile::new(self.kind, self.title).art(self.art).progress(progress);
        tile.paint(ui, slot);
        if slot.mode != SlotMode::Draw {
            return;
        }
        if self.badge.is_some() || self.news {
            on_well(ui, slot, |ui, rect| {
                if let Some((text, tone)) = self.badge {
                    title_badge(ui, rect, text, tone);
                }
                if self.news {
                    let dot = Rect::new(rect.right() - 18.0, rect.y + 8.0, 10.0, 10.0);
                    ui.fill(dot.expand(2.0), theme::ON_ART, 7.0);
                    ui.fill(dot, theme::ACCENT, 5.0);
                }
            });
        }
        // The line under the title, as the caption's second.
        let rect = slot.rect;
        let y = rect.bottom() + theme::TILES_TO_CAPTION + theme::CAPTION.line_height;
        let color = self.tone.map_or(theme::MUTED, tone_color);
        let text = match (self.lines, self.line.split_once(" · ")) {
            (2, Some((first, rest))) => format!("{first}\n{rest}"),
            _ => self.line.to_string(),
        };
        let opts = TextOpts::wrap(rect.w, self.lines.max(1) as u16);
        ui.text(Point::new(rect.x, y), &text, &theme::CAPTION, &opts, color);
    }
}

// ---- badges the details left

/// A title's status and library title as its details last said, and when.
#[derive(Clone, Copy, Debug)]
struct Mark {
    status: TitleStatus,
    item_id: Option<i64>,
    at: f64,
}

thread_local! {
    /// Badges as titles' details left them, for lists drawn before them, per server and profile.
    static MARKS: RefCell<(u64, HashMap<String, Mark>)> = RefCell::new((0, HashMap::new()));
}

/// A title's details came: lists drawn before show its badge as they say.
pub fn keep_title(cx: &AppCx, d: &DiscoverDetail) {
    let generation = cx.state.generation();
    MARKS.with(|m| {
        let mut m = m.borrow_mut();
        if m.0 != generation {
            *m = (generation, HashMap::new());
        }
        let key = title_key(d.title.kind, d.title.tmdb_id);
        let same = m.1.get(&key).is_some_and(|k| k.status == d.title.status && k.item_id == d.title.item_id);
        if !same {
            m.1.insert(key, Mark { status: d.title.status, item_id: d.title.item_id, at: cx.now });
        }
    });
}

/// `t` with the badge details left since `at` (when its list came).
pub fn marked(cx: &AppCx, t: &DiscoverTitle, at: f64) -> DiscoverTitle {
    let generation = cx.state.generation();
    MARKS.with(|m| {
        let m = m.borrow();
        match m.1.get(&title_key(t.kind, t.tmdb_id)) {
            Some(k) if m.0 == generation && k.at > at => {
                DiscoverTitle { status: k.status, item_id: k.item_id.or(t.item_id), ..t.clone() }
            }
            _ => t.clone(),
        }
    })
}

/// How many marks were left, to know when lists' badges may have changed.
pub fn marks_version(cx: &AppCx) -> (u64, usize, u64) {
    MARKS.with(|m| {
        let m = m.borrow();
        let newest = m.1.values().map(|k| k.at.to_bits()).max().unwrap_or(0);
        (if m.0 == cx.state.generation() { m.0 } else { 0 }, m.1.len(), newest)
    })
}

// ---- asking, following

/// A request's answer once it came (a stopped request as a failure).
pub fn answer_of<T>(p: &mut Promise<Result<T, ApiError>>) -> Option<Result<T, ApiError>> {
    match p.take() {
        Some(a) => Some(a),
        None if p.failed() => Some(Err(ApiError::Unreadable("the request stopped".into()))),
        None => None,
    }
}

/// Asking for a title, or following the requests that cover it (Tell me too).
pub struct Asking {
    body: NewRequest,
    job: Promise<Result<(MediaRequest, bool), ApiError>>,
}

impl Asking {
    pub fn start(cx: &AppCx, body: NewRequest) -> Asking {
        let sent = body.clone();
        Asking { body, job: cx.spawn(move |api| api.ask_for(&sent)) }
    }

    /// Once it answered: a toast says how it went or why not, and what shows requests asks
    /// again. `Some(true)` asked, `Some(false)` refused.
    pub fn poll(&mut self, cx: &mut AppCx, time: TvTime) -> Option<bool> {
        let answer = answer_of(&mut self.job)?;
        let key = title_key(self.body.kind, self.body.tmdb_id);
        match answer {
            Ok((r, created)) => {
                cx.toast(asked_text(&r, created));
                cx.state.requests_changed(Some(key));
                cx.state.load_requests(true);
                Some(true)
            }
            Err(e) if e.cancelled() => Some(false),
            Err(e) => {
                let p = requests_problem(&e);
                let words = ask_refusal(&p, self.body.kind, cx.state.request_summary(), time);
                cx.toast_error(words);
                // What may be asked for, or whether at all, changed meanwhile.
                cx.state.load_requests(true);
                if matches!(p, RequestsProblem::Conflict { .. } | RequestsProblem::Declined { .. }) {
                    cx.state.requests_changed(Some(key));
                }
                Some(false)
            }
        }
    }
}

/// Stopping following someone else's request.
pub struct Unwatching {
    request: MediaRequest,
    job: Promise<Result<(), ApiError>>,
}

impl Unwatching {
    pub fn start(cx: &AppCx, request: MediaRequest) -> Unwatching {
        let id = request.id;
        Unwatching { request, job: cx.spawn(move |api| api.unwatch_request(id)) }
    }

    /// Once it answered: whether it was done.
    pub fn poll(&mut self, cx: &mut AppCx, time: TvTime) -> Option<bool> {
        let answer = answer_of(&mut self.job)?;
        let r = &self.request;
        match answer {
            Ok(()) => {
                cx.toast(action_done(ActionId::Action(pley_core::model::RequestAction::Unwatch), r));
                cx.state.requests_changed(Some(title_key(r.kind, r.tmdb_id)));
                cx.state.load_requests(true);
                Some(true)
            }
            Err(e) => {
                if !e.cancelled() {
                    cx.toast_error(ask_refusal(&requests_problem(&e), r.kind, None, time));
                }
                Some(false)
            }
        }
    }
}

// ---- asking again on a timer

/// Asks again every so often while the app shows (`usePoll`).
#[derive(Default)]
pub struct Poll {
    next: Option<f64>,
    every: Option<f64>,
}

impl Poll {
    /// Whether it is time to ask again, asking every `every` seconds (`None`: not at all).
    pub fn due(&mut self, ui: &mut Ui, cx: &AppCx, every: Option<f64>) -> bool {
        let every = every.filter(|_| cx.state.visible());
        if every != self.every {
            self.every = every;
            self.next = every.map(|e| cx.now + e);
        }
        let Some(next) = self.next else { return false };
        if cx.now >= next {
            self.next = self.every.map(|e| cx.now + e);
            return true;
        }
        ui.repaint_after(next - cx.now);
        false
    }
}

// ---- pages that can't show requests

/// Why requests can't be used, from the summary alone (`None`: they may be, or it isn't known yet).
pub fn summary_problem(s: Option<&RequestSummary>) -> Option<RequestsProblem> {
    let s = s?;
    if s.sign_in_again == Some(pley_core::model::SignInAgain::Requests) {
        return Some(RequestsProblem::SignIn);
    }
    if !s.enabled {
        return Some(RequestsProblem::Off);
    }
    if s.access == pley_core::model::RequestAccess::Off {
        return Some(RequestsProblem::NoAccess);
    }
    None
}

/// A failed load's title: `title`, or "Can't reach your server" when the server answered nothing.
pub fn failed_title<'a>(e: Option<&ApiError>, title: &'a str) -> &'a str {
    if e.is_some_and(ApiError::unreachable) { t!("requests.unreachable") } else { title }
}

/// What a page that can't show requests offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemAction {
    Retry,
    SignIn,
    Requests,
}

fn problem_icon(p: &RequestsProblem) -> Option<Icon> {
    match p {
        RequestsProblem::Off => Some(Icon::CircleX),
        RequestsProblem::NoAccess => Some(Icon::Lock),
        RequestsProblem::TmdbDown => Some(Icon::ServerOff),
        RequestsProblem::TmdbKey => Some(Icon::CircleAlert),
        RequestsProblem::SignIn => Some(Icon::UserRound),
        _ => None,
    }
}

/// A page that can't show requests (`RequestsMessage`), saying why: off, not open to this account
/// (its requests still reachable when it has some), signing in again, TMDB's key or TMDB not
/// answering (Retry); anything else in the app's own words, with Retry. What was pressed.
pub fn problem_page(
    ui: &mut Ui,
    cx: &AppCx,
    id: Id,
    problem: &RequestsProblem,
    title: &str,
) -> (Option<ProblemAction>, bool) {
    let s = cx.state.request_summary();
    let Some(words) = problem_words(problem, s.is_some_and(|s| s.manage)) else {
        let e = match problem {
            RequestsProblem::Other(e) => Some(e),
            _ => None,
        };
        let body = e.map(ApiError::problem).unwrap_or_default();
        let r = Message::new(MessageStyle::Item, failed_title(e, title))
            .body(&body)
            .action(t!("requests.buttons.retry"), Icon::RefreshCw)
            .show(ui, id);
        return (r.action.is_some_and(|a| a.activated).then_some(ProblemAction::Retry), true);
    };
    let label = requests_title(s);
    let action = if words.retry {
        Some((t!("requests.buttons.retry"), Icon::RefreshCw, ProblemAction::Retry))
    } else if *problem == RequestsProblem::SignIn {
        Some((t!("requests.buttons.signIn"), Icon::UserRound, ProblemAction::SignIn))
    } else if *problem == RequestsProblem::NoAccess && s.is_some_and(|s| s.own > 0 || s.manage) {
        Some((label, Icon::Inbox, ProblemAction::Requests))
    } else {
        None
    };
    let mut message = Message::new(MessageStyle::Item, words.title).icon(problem_icon(problem)).body(words.body);
    if let Some((label, icon, _)) = action {
        message = message.action(label, icon);
    }
    let r = message.show(ui, id);
    let pressed = r.action.is_some_and(|a| a.activated);
    (action.filter(|_| pressed).map(|a| a.2), action.is_some())
}

/// Signing in again, so the TV may ask or decide (`SignInAgainDialog`): signed out here, and a new
/// code to link it.
#[derive(Default)]
pub struct SignInAgain {
    pub open: bool,
}

impl SignInAgain {
    pub fn show(&mut self, ui: &mut Ui, cx: &mut AppCx, id: Id) {
        let server = cx.state.server().map(str::to_string);
        let body = match server.as_deref() {
            Some(server) => t!("requests.signInAgain.bodyServer", server = display_address(server)),
            None => t!("requests.signInAgain.body").to_string(),
        };
        let buttons =
            [DialogButton::new(t!("requests.buttons.cancel")), DialogButton::new(t!("requests.buttons.signIn"))];
        let r = Dialog::new(t!("requests.signInAgain.title"))
            .body(&body)
            .buttons(&buttons)
            .initial(1)
            .show(ui, id, self.open);
        if r.dismissed || r.chosen == Some(0) {
            self.open = false;
        }
        if r.chosen == Some(1) && std::mem::take(&mut self.open) {
            let back_to = match server {
                Some(server) => Route::Link { server },
                None => Route::Servers,
            };
            cx.state.sign_out();
            cx.reset(back_to);
        }
    }
}

/// A line's last two words kept together, so a wrapped line never ends with one word alone.
pub fn keep_last_words(text: &str) -> String {
    match text.rfind(' ') {
        Some(at) if at > 0 => format!("{}\u{a0}{}", &text[..at], &text[at + 1..]),
        _ => text.to_string(),
    }
}
