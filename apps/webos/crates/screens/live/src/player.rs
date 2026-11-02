//! The live player (`TvLivePlayerScreen.kt`, `LivePlayerViewModel.kt`): a
//! channel full screen, tuned through pley-player's [`LiveSession`]. With nothing over the picture, Up
//! and Down go to the next and previous channel of the category it was opened from (Up the next), as
//! channel up and down always do; digits tune a number; OK, Menu or Guide opens the channel list beside
//! the picture, which moves aside; OK held shows what's on with the controls (Samsung's rule, where
//! Android's OK shows the controls); Info shows what's on for 5 s.
//! Live TV is watched as it airs: play, pause, rewind and fast forward, Left and Right do nothing. A
//! channel that doesn't start says why, with Try again, Other channels and Back to the guide; every
//! connection in use lists what holds each, with Watch here instead where it may be taken over.

use std::sync::Arc;

use pley_base::{Align, Color, Icon, Key, Point, Rect, TextOpts};
use pley_core::http::sentence;
use pley_core::model::{BusyConnection, LiveCategory, LiveChannel, LiveChannelPage};
use pley_core::{ApiError, Device, Promise};
use pley_kit::button::ROUND_SIZE;
use pley_kit::{Button, LoadingRing, RoundButton, bar_group, inert};
use pley_player::live::data::{CHANNEL_PAGE, channel_by_number, channel_name, time_left};
use pley_player::live::{Category, ChannelPages, LivePhase, LiveSession, Trouble, holders, trouble_of};
use pley_player::{PlayerServer, now_ms};
use pley_shell::{AppCx, AppEvent, Chrome, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dirs, FocusOpts, Id, Motion, Ui};

use crate::channel_list::{self, ChannelList, ListAction};
use crate::clock::Clock;
use crate::draw::{Well, channel_tile, fit, line_mid, measure, programme_badge, progress};
use crate::kept::{self, page_category};

/// Digits of a channel number, at most (`MAX_DIGITS`).
pub const MAX_DIGITS: usize = 5;
/// A typed number is tuned this long after its last digit (`DIGITS_WAIT_MS`).
pub const DIGITS_WAIT: f64 = 1.4;
/// "No channel 123" shows this long (`NOT_FOUND_MS`).
pub const NOT_FOUND: f64 = 2.0;
/// What's on shows this long when asked for with Info (`INFO_MS`).
pub const INFO: f64 = 5.0;
/// The controls hide after this long untouched while the channel plays (`CONTROLS_MS`).
pub const CONTROLS: f64 = 6.0;
/// A stall shows the ring only once it lasts this long (`STALL_SHOWN_MS`).
pub const STALL_SHOWN: f64 = 0.7;
/// What's on is drawn anew this often (`LivePlayerViewModel`'s clock).
const CLOCK_S: i64 = 5;
/// The trouble card's widest (`widthIn(max = 520.dp)`) and its padding.
const CARD_MAX: f32 = 520.0;
const CARD_PAD: f32 = 32.0;
/// A connection's row in the card, and the space under it.
const BUSY_ROW_H: f32 = 60.0;
const BUSY_ROW_GAP: f32 = 8.0;

/// What shows over the picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlay {
    /// Nothing: the keys change channel.
    #[default]
    None,
    /// What's on, with the channel list and favourite buttons (OK).
    Controls,
    /// The channel list beside the picture (Menu, Guide, or Channels in the controls).
    Channels,
}

/// The picture's place while the channel list shows: 16:9 within what the list leaves at the left.
pub fn picture_aside() -> Rect {
    fit(Rect::new(0.0, 0.0, theme::CANVAS_W - channel_list::WIDTH, theme::CANVAS_H), 16.0 / 9.0)
}

/// A channel number typed on the remote (`ChannelDigits`): up to five digits, looked up once none came
/// for 1.4 s; a number that matches nothing says so for 2 s.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Digits {
    /// The digits typed, shown until the lookup answers.
    pub typed: String,
    /// When the last digit came (the UI's seconds).
    at: f64,
    /// The number being looked up.
    looking: bool,
    /// The number that matched nothing, and since when it says so.
    not_found: Option<(String, f64)>,
}

impl Digits {
    /// A digit pressed at `now`: added while there are fewer than five; the wait starts again.
    pub fn push(&mut self, digit: u8, now: f64) {
        if self.typed.len() < MAX_DIGITS {
            self.typed.push(char::from(b'0' + digit % 10));
        }
        self.at = now;
        self.looking = false;
        self.not_found = None;
    }

    /// The number to look up, once, when no digit came for 1.4 s.
    pub fn due(&mut self, now: f64) -> Option<String> {
        if self.typed.is_empty() || self.looking || now - self.at < DIGITS_WAIT - 1e-6 {
            return None;
        }
        self.looking = true;
        Some(self.typed.clone())
    }

    /// The lookup found a channel: the box goes.
    pub fn found(&mut self) {
        *self = Digits::default();
    }

    /// The lookup found nothing: "No channel 123" from `now`.
    pub fn missed(&mut self, now: f64) {
        let number = std::mem::take(&mut self.typed);
        *self = Digits { not_found: Some((number, now)), ..Digits::default() };
    }

    /// What the box shows at `now`: the digits typed (true: in the code style), or "No channel 123".
    pub fn text(&self, now: f64) -> Option<(String, bool)> {
        if !self.typed.is_empty() {
            return Some((self.typed.clone(), true));
        }
        let (number, at) = self.not_found.as_ref()?;
        (now - at < NOT_FOUND).then(|| (format!("No channel {number}"), false))
    }

    /// When it next has something to do: the lookup, or the end of "No channel 123".
    pub fn wake(&self) -> Option<f64> {
        if !self.typed.is_empty() && !self.looking {
            return Some(self.at + DIGITS_WAIT);
        }
        self.not_found.as_ref().map(|(_, at)| at + NOT_FOUND)
    }
}

/// What the bar at the foot asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarAction {
    Channels,
    Favourite,
    /// The menu key on a button.
    Menu,
}

/// What the trouble card asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardAction {
    Retry,
    TakeOver(i64),
    Channels,
    Back,
}

/// A favourite being marked on the server.
struct Marking {
    id: i64,
    on: bool,
    job: Promise<Result<(), ApiError>>,
}

/// The live player.
pub struct LivePlayerScreen {
    /// The channel asked for first, then the one tuned last.
    channel_id: i64,
    /// The category channel up and down go through.
    category: LiveCategory,
    /// When the channel was chosen (the UI's seconds), for the log's time to the first picture.
    pressed_at: f64,
    /// The channel asked of the server when no page showed it.
    asking: Option<Promise<Result<LiveChannel, ApiError>>>,
    /// The channel couldn't be had: why.
    unknown: Option<Trouble>,
    session: Option<LiveSession>,
    device: Option<Arc<dyn Device>>,
    /// The category's channels, for channel up and down.
    zap: Option<ChannelPages>,
    digits: Digits,
    lookup: Option<Promise<Result<LiveChannelPage, ApiError>>>,
    overlay: Overlay,
    /// When a key last touched the controls.
    poked_at: f64,
    /// Info asked for what's on; it shows once the channel plays.
    peek: bool,
    info_until: Option<f64>,
    list: ChannelList,
    marking: Vec<Marking>,
    /// What's on, asked again once the programme playing ended.
    refresh: Option<Promise<Result<LiveChannel, ApiError>>>,
    refreshed_for: Option<(i64, i64)>,
    /// The channel playing as asked last (what's on, the favourite), over the session's.
    fresh: Option<LiveChannel>,
    /// Whether the picture was last put aside.
    aside: Option<bool>,
    /// The session's state version last seen, and when it began buffering.
    seen: u64,
    buffering_since: Option<f64>,
    /// The trouble whose buttons took focus.
    trouble_focused: Option<u64>,
    focus_picture: bool,
    focus_controls: bool,
    logged: bool,
    /// Whether the screen saver was last held off (none: ask again, the TV let it go while hidden).
    awake: Option<bool>,
    /// The TV hid the app.
    hidden: bool,
    /// The TV writes the time in 24 hours: read as the player opens and shows again.
    hours24: Option<bool>,
}

impl LivePlayerScreen {
    /// The player of `channel_id`, channel up and down going through `category`; `pressed_at` is when
    /// it was chosen. Nothing plays before it first draws.
    pub fn new(channel_id: i64, category: LiveCategory, pressed_at: f64) -> LivePlayerScreen {
        LivePlayerScreen {
            channel_id,
            list: ChannelList::new(&category),
            category,
            pressed_at,
            asking: None,
            unknown: None,
            session: None,
            device: None,
            zap: None,
            digits: Digits::default(),
            lookup: None,
            overlay: Overlay::None,
            poked_at: 0.0,
            peek: false,
            info_until: None,
            marking: Vec::new(),
            refresh: None,
            refreshed_for: None,
            fresh: None,
            aside: None,
            seen: u64::MAX,
            buffering_since: None,
            trouble_focused: None,
            focus_picture: true,
            focus_controls: false,
            logged: false,
            awake: None,
            hidden: false,
            hours24: None,
        }
    }

    /// What shows over the picture.
    pub fn overlay(&self) -> Overlay {
        self.overlay
    }

    /// The category channel up and down go through.
    pub fn category(&self) -> &LiveCategory {
        &self.category
    }

    /// The channel playing (or being tuned), once known.
    pub fn channel(&self) -> Option<&LiveChannel> {
        let session = self.session.as_ref()?.state();
        Some(self.fresh.as_ref().filter(|f| f.id == session.channel.id).unwrap_or(&session.channel))
    }

    /// Where watching is, once a channel is known.
    pub fn phase(&self) -> Option<LivePhase> {
        self.session.as_ref().map(|s| s.state().phase)
    }

    /// The digits typed, or "No channel 123".
    pub fn digits(&self) -> &Digits {
        &self.digits
    }

    fn server(cx: &AppCx) -> Option<Arc<dyn PlayerServer>> {
        cx.api().map(|a| Arc::new(a.clone()) as Arc<dyn PlayerServer>)
    }

    /// The first channel from the page that showed it, else from the server; then the session tuning it.
    fn start(&mut self, cx: &mut AppCx) {
        if self.session.is_some() || self.unknown.is_some() {
            return;
        }
        let known = cx.state.known_live_channel(self.channel_id).cloned();
        let channel = match known {
            Some(c) => c,
            None => {
                let Some(p) = self.asking.as_mut() else {
                    let id = self.channel_id;
                    self.asking = Some(cx.spawn(move |api| api.live_channel(id)));
                    return;
                };
                if !p.ready() && !p.failed() {
                    return;
                }
                let answer = p.take().unwrap_or(Err(ApiError::Net(pley_core::NetError::Cancelled)));
                self.asking = None;
                match answer {
                    Ok(c) => c,
                    Err(e) => {
                        self.unknown = Some(trouble_of(&e, "This channel"));
                        return;
                    }
                }
            }
        };
        self.begin(cx, channel);
    }

    /// The session, tuning `channel`: as the player opens, or once another channel is chosen after the
    /// first couldn't be had.
    fn begin(&mut self, cx: &mut AppCx, channel: LiveChannel) {
        let Some(env) = cx.player_env() else { return };
        // The music gives way: one thing plays at a time.
        if let Some(music) = cx.state.music.as_mut() {
            music.pause();
        }
        let device = cx.device().clone();
        self.device = Some(device.clone());
        // Recently watched goes in the order it had as the player opened: asked before the watch changes it.
        self.zap_pages(cx);
        let mut session = LiveSession::new(channel.clone(), &env, device.engine(), now_ms());
        self.remember(cx, &channel);
        self.channel_id = channel.id;
        self.unknown = None;
        self.asking = None;
        session.tune(channel);
        self.session = Some(session);
    }

    /// The channel playing, or being tuned, without trouble: choosing it again changes nothing.
    fn playing_id(&self) -> Option<i64> {
        let s = self.session.as_ref()?.state();
        (s.phase != LivePhase::Error).then(|| self.channel().map(|c| c.id)).flatten()
    }

    /// The channel tuned is kept: Live TV opens back at it, and the player starts with it at once.
    fn remember(&self, cx: &mut AppCx, channel: &LiveChannel) {
        cx.state.keep_live_channel(channel.clone());
        kept::keep_last_watched(cx, channel.id, &self.category);
    }

    /// The pages channel up and down go through, made for the category when it changes.
    fn zap_pages(&mut self, cx: &AppCx) -> Option<&mut ChannelPages> {
        let wanted = page_category(&self.category);
        if self.zap.as_ref().is_none_or(|z| *z.category() != wanted) {
            let server = LivePlayerScreen::server(cx)?;
            let frozen = wanted == Category::Recent;
            let mut pages = ChannelPages::new(server, cx.tasks().clone(), wanted, frozen);
            pages.want(0, 0, now_ms());
            self.zap = Some(pages);
        }
        self.zap.as_mut()
    }

    /// Watches `channel`; channel up and down go on through the category. The channel already playing
    /// isn't tuned again, one in trouble is (`LivePlayerViewModel.tune`).
    fn tune(&mut self, cx: &mut AppCx, channel: LiveChannel) {
        if self.playing_id() == Some(channel.id) {
            return;
        }
        self.fresh = None;
        self.info_until = None;
        self.peek = false;
        self.trouble_focused = None;
        if self.overlay == Overlay::Controls {
            self.overlay = Overlay::None;
            self.focus_picture = true;
        }
        if self.session.is_none() {
            // The first channel couldn't be had: this one starts the session.
            self.begin(cx, channel);
            return;
        }
        self.channel_id = channel.id;
        self.remember(cx, &channel);
        if let Some(s) = self.session.as_mut() {
            s.tune(channel);
        }
    }

    /// Channel up (`by` 1) or down (−1) in the category, round its ends; from the channel asked for
    /// when it couldn't be had (the category's first then, when it isn't in it).
    fn step(&mut self, cx: &AppCx, by: i64) {
        let current = self.channel().map_or(self.channel_id, |c| c.id);
        if let Some(zap) = self.zap_pages(cx) {
            zap.neighbor(current, by, now_ms());
        }
    }

    fn open_list(&mut self) {
        let current = self.channel().map_or(self.channel_id, |c| c.id);
        self.list.open(&self.category, current);
        self.overlay = Overlay::Channels;
    }

    /// Menu or Guide: the channel list, or back to the picture.
    fn toggle_list(&mut self) {
        if self.overlay == Overlay::Channels {
            self.close_list();
        } else {
            self.open_list();
        }
    }

    fn close_list(&mut self) {
        self.list.close();
        self.overlay = Overlay::None;
        // Back to the picture, or to the trouble card's first button.
        self.focus_picture = true;
        self.trouble_focused = None;
    }

    /// Takes what came back: the channel up or down, a number looked up, favourites, what's on.
    fn collect(&mut self, cx: &mut AppCx, now: f64, seconds: i64) {
        let ms = now_ms();
        let mut tune = None;
        if let Some(zap) = self.zap.as_mut() {
            zap.poll(ms);
            if let Some(found) = zap.take_neighbor() {
                tune = found;
            }
        }
        if let Some(number) = self.digits.due(now) {
            let browse = Category::Search(number).browse(0, CHANNEL_PAGE, None);
            self.lookup = Some(cx.spawn(move |api| api.live_browse(&browse)));
        }
        if let Some(p) = self.lookup.as_mut()
            && (p.ready() || p.failed())
        {
            let page = p.take().and_then(Result::ok);
            self.lookup = None;
            let typed = self.digits.typed.clone();
            match page.as_ref().and_then(|page| channel_by_number(&page.channels, &typed)) {
                Some(c) => {
                    self.digits.found();
                    tune = Some(c.clone());
                }
                None => self.digits.missed(now),
            }
        }
        if let Some(c) = tune {
            self.tune(cx, c);
        }
        let mut i = 0;
        while i < self.marking.len() {
            if !self.marking[i].job.ready() && !self.marking[i].job.failed() {
                i += 1;
                continue;
            }
            let mut m = self.marking.swap_remove(i);
            let why = match m.job.take() {
                Some(Ok(())) => {
                    // The list's Favourites counts it.
                    self.list.groups_changed();
                    continue;
                }
                Some(Err(e)) => sentence(&e.to_string()),
                None => "The request stopped.".to_string(),
            };
            // The server didn't keep it: undone.
            self.mark(m.id, !m.on);
            cx.toast_error(format!("The favourite wasn't saved: {why}"));
        }
        // What's on, asked again once the programme playing ended.
        if let Some(p) = self.refresh.as_mut()
            && (p.ready() || p.failed())
        {
            if let Some(Ok(c)) = p.take()
                && Some(c.id) == self.channel().map(|c| c.id)
            {
                self.fresh = Some(c);
            }
            self.refresh = None;
        }
        let ended = self.channel().filter(|c| c.on_at(seconds).is_none()).and_then(|c| {
            let listed = [c.now.as_ref(), c.next.as_ref()];
            listed.into_iter().flatten().filter(|p| p.stop <= seconds).map(|p| (c.id, p.stop)).max_by_key(|e| e.1)
        });
        if let Some((id, stop)) = ended
            && self.refresh.is_none()
            && self.refreshed_for != Some((id, stop))
        {
            self.refreshed_for = Some((id, stop));
            self.refresh = Some(cx.spawn(move |api| api.live_channel(id)));
        }
    }

    /// Marks the channel playing a favourite or not, at once; undone if the server refuses.
    fn toggle_favourite(&mut self, cx: &AppCx) {
        let Some(c) = self.channel() else { return };
        let (id, on) = (c.id, !c.favorite);
        self.mark(id, on);
        self.marking.push(Marking { id, on, job: cx.spawn(move |api| api.set_live_favorite(id, on)) });
    }

    fn mark(&mut self, id: i64, on: bool) {
        if let Some(c) = self.channel().filter(|c| c.id == id) {
            let mut c = c.clone();
            c.favorite = on;
            self.fresh = Some(c);
        }
        if let Some(zap) = self.zap.as_mut() {
            zap.patch(id, |c| c.favorite = on);
        }
        self.list.patch(id, on);
    }

    /// The keys that act whatever shows: channel up and down, digits, Menu or Guide, Info.
    fn keys(&mut self, ui: &mut Ui, cx: &AppCx, now: f64) {
        let keys = ui.keys().to_vec();
        if self.overlay == Overlay::Controls && !keys.is_empty() {
            self.poked_at = now;
        }
        for k in keys {
            match k.key {
                Key::ChannelUp if k.pressed && !k.repeat => self.step(cx, 1),
                Key::ChannelDown if k.pressed && !k.repeat => self.step(cx, -1),
                Key::Digit(d) if k.pressed && !k.repeat && !self.list.typing() => {
                    self.lookup = None;
                    self.digits.push(d, now);
                }
                // Menu reaches the focused element instead (pley-ui acts on it once, on key-up): each
                // part of the screen passes it on as the list's toggle.
                Key::Guide if !k.pressed => self.toggle_list(),
                Key::Info if !k.pressed => self.peek = true,
                _ => continue,
            }
            ui.request_repaint();
        }
    }
}

impl Drop for LivePlayerScreen {
    fn drop(&mut self) {
        // The session ends on the server as it drops; the screen may sleep again.
        self.session = None;
        if let Some(d) = self.device.as_ref().filter(|_| self.awake == Some(true)) {
            d.keep_awake(false);
        }
    }
}

impl Screen for LivePlayerScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        ui.set_clear(Color::TRANSPARENT);
        let now = ui.time();
        let hours24 = *self.hours24.get_or_insert_with(|| cx.device().clock_24h());
        let clock = Clock::read(hours24);
        self.start(cx);
        if let Some(s) = self.session.as_mut() {
            s.tick(now_ms());
            if let Some(wake) = s.next_wake() {
                ui.repaint_after((wake - now_ms()).max(0) as f64 / 1000.0);
            }
        }
        self.collect(cx, now, clock.now);
        self.keys(ui, cx, now);
        if let Some(t) = self.digits.wake() {
            ui.repaint_after(t - now);
        }
        ui.repaint_after((CLOCK_S - clock.now.rem_euclid(CLOCK_S)) as f64);

        let state = self.session.as_ref().map(|s| (s.version(), s.state().clone()));
        let channel = self.channel().cloned();
        let phase = state.as_ref().map(|s| s.1.phase);
        let playing = matches!(phase, Some(LivePhase::Playing | LivePhase::Buffering));
        // The screen saver is held off while a channel tunes or plays, not while the trouble card waits;
        // the TV lets it go whenever the app is hidden, so it is asked again as it shows.
        let awake = !self.hidden && (playing || phase == Some(LivePhase::Tuning));
        if self.awake != Some(awake) {
            if awake || self.awake == Some(true) {
                cx.device().keep_awake(awake);
            }
            self.awake = Some(awake);
        }
        if let Some((version, s)) = &state {
            if *version != self.seen {
                self.seen = *version;
                if s.phase == LivePhase::Buffering {
                    self.buffering_since.get_or_insert(now);
                } else {
                    self.buffering_since = None;
                }
                if s.phase != LivePhase::Error {
                    self.trouble_focused = None;
                }
            }
            if s.shown && !std::mem::replace(&mut self.logged, true) && self.pressed_at > 0.0 {
                log::info!("live: first picture {:.0} ms after the press", (now - self.pressed_at) * 1000.0);
            }
        }
        if self.peek && playing {
            self.peek = false;
            self.info_until = Some(now + INFO);
        }
        if self.overlay == Overlay::Controls && !playing {
            self.overlay = Overlay::None;
            self.focus_picture = true;
        }
        if self.overlay == Overlay::Controls && phase == Some(LivePhase::Playing) {
            let hide = self.poked_at + CONTROLS;
            if now >= hide {
                self.overlay = Overlay::None;
                self.focus_picture = true;
            } else {
                ui.repaint_after(hide - now);
            }
        }

        // The picture: aside while the channel list shows.
        let aside = self.overlay == Overlay::Channels;
        if self.aside != Some(aside)
            && let Some(s) = self.session.as_mut()
        {
            self.aside = Some(aside);
            s.set_video_rect(aside.then(picture_aside));
        }
        let stage = if aside {
            Rect::new(0.0, 0.0, theme::CANVAS_W - channel_list::WIDTH, theme::CANVAS_H)
        } else {
            Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H)
        };
        let audio_only = state.as_ref().is_some_and(|s| s.1.audio_only);
        if aside || !playing || audio_only {
            ui.fill(Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H), Color::BLACK, 0.0);
            if aside && playing && !audio_only {
                ui.punch(picture_aside());
            }
        }

        // The picture takes the keys while nothing shows over it.
        let failed = phase == Some(LivePhase::Error) || self.unknown.is_some();
        let picture_id = ui.id("picture");
        if self.overlay == Overlay::None && !failed {
            let r = ui.focusable_with(picture_id, stage, FocusOpts::default().long_press().takes(Dirs::ALL));
            ui.initial_focus(picture_id);
            if std::mem::take(&mut self.focus_picture) {
                ui.request_focus(picture_id);
            }
            if r.focused {
                for k in ui.keys().to_vec() {
                    if k.pressed && !k.repeat {
                        match k.key {
                            Key::Up => self.step(cx, 1),
                            Key::Down => self.step(cx, -1),
                            _ => {}
                        }
                    }
                }
            }
            // OK opens the channel list as it is let go, tuning or playing, as Menu does; held, it
            // shows the controls (the list, the favourite) over what's on, as a long press opens
            // actions elsewhere. Android's OK shows the controls; Samsung's opens the list.
            if r.menu || r.activated {
                self.open_list();
            }
            if r.long_pressed && playing {
                self.overlay = Overlay::Controls;
                self.poked_at = now;
                self.focus_controls = true;
                ui.request_repaint();
            }
        }

        // What shows in place of a picture.
        match (&state, &channel) {
            (Some((_, s)), Some(c)) if s.phase == LivePhase::Tuning => {
                tuning(ui, cx, stage, c, &self.category, clock.now, now);
            }
            (Some((_, s)), Some(c)) if s.audio_only && playing => sound_only(ui, cx, stage, c, clock.now),
            (None, _) if self.unknown.is_none() => {
                LoadingRing::tuning().show(ui, stage.center(), now);
            }
            _ => {}
        }
        if let Some(since) = self.buffering_since.filter(|_| phase == Some(LivePhase::Buffering)) {
            LoadingRing::tuning().delay(STALL_SHOWN).show(ui, stage.center(), since);
        }

        // Why it doesn't play.
        let trouble = state.as_ref().and_then(|s| s.1.trouble.clone()).or_else(|| self.unknown.clone());
        if let Some(t) = trouble.filter(|_| failed) {
            let card_id = ui.id("trouble");
            let focus = self.trouble_focused != Some(self.seen) && self.overlay != Overlay::Channels;
            if focus {
                self.trouble_focused = Some(self.seen);
            }
            let live = self.overlay != Overlay::Channels;
            let shown = channel.as_ref().filter(|_| self.unknown.is_none());
            let action = if live {
                trouble_card(ui, cx, card_id, &t, shown, focus)
            } else {
                inert(ui, |ui| trouble_card(ui, cx, card_id, &t, shown, false))
            };
            match action {
                Some(CardAction::Retry) => {
                    if self.unknown.take().is_none()
                        && let Some(s) = self.session.as_mut()
                    {
                        s.retry();
                    }
                }
                Some(CardAction::TakeOver(id)) => {
                    if let Some(s) = self.session.as_mut() {
                        s.take_over(id);
                    }
                }
                Some(CardAction::Channels) => self.open_list(),
                Some(CardAction::Back) => cx.pop(),
                None => {}
            }
        }

        // What's on at the foot, with the controls after OK.
        let info = self.info_until.is_some_and(|t| now < t);
        if let Some(t) = self.info_until {
            if now < t {
                ui.repaint_after(t - now);
            } else {
                self.info_until = None;
            }
        }
        let bar_id = ui.id("bar");
        let visible = playing && self.overlay != Overlay::Channels && (self.overlay == Overlay::Controls || info);
        let p = ui.animate(bar_id.with("shown"), if visible { 1.0 } else { 0.0 }, Motion::OVERLAY);
        if p > 0.0
            && let (Some(c), Some((_, s))) = (&channel, &state)
        {
            let controls = self.overlay == Overlay::Controls;
            let focus = std::mem::take(&mut self.focus_controls);
            let live = s.phase == LivePhase::Playing || s.phase == LivePhase::Buffering;
            let bar = |ui: &mut Ui| info_bar(ui, cx, bar_id, c, &self.category, &clock, live, controls, p, focus);
            let action = if visible && controls { bar(ui) } else { inert(ui, bar) };
            match action {
                Some(BarAction::Channels | BarAction::Menu) => self.open_list(),
                Some(BarAction::Favourite) => self.toggle_favourite(cx),
                None => {}
            }
        }

        // A number being typed, or one that matched nothing.
        if let Some((text, code)) = self.digits.text(now) {
            digits_box(ui, &text, code);
        }

        // The channel list beside the picture.
        let current = self.channel().map_or(self.channel_id, |c| c.id);
        let list_id = ui.id("list");
        match self.list.show(ui, cx, list_id, current) {
            Some(ListAction::Tune(c, from)) => {
                self.category = from;
                self.close_list();
                self.tune(cx, *c);
            }
            Some(ListAction::Close) => self.close_list(),
            None => {}
        }
        if self.overlay == Overlay::Channels && !self.list.open {
            self.overlay = Overlay::None;
            self.focus_picture = true;
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        match self.overlay {
            Overlay::Channels => {
                if !self.list.back() {
                    self.close_list();
                }
                true
            }
            Overlay::Controls => {
                self.overlay = Overlay::None;
                self.focus_picture = true;
                true
            }
            Overlay::None => false,
        }
    }

    fn event(&mut self, event: &AppEvent, _cx: &mut AppCx) {
        match event {
            // The TV hid the app: the connection and the decoders are let go, to tune again when it shows.
            AppEvent::Visible(false) => {
                if let Some(s) = self.session.as_mut() {
                    s.suspend();
                }
                self.aside = None;
                self.hidden = true;
            }
            AppEvent::Visible(true) => {
                if let Some(s) = self.session.as_mut() {
                    s.resume();
                }
                // The TV let the screen saver go while the app was hidden: it is held off again.
                self.hidden = false;
                self.awake = None;
                self.hours24 = None;
            }
            AppEvent::Shown => self.hours24 = None,
            AppEvent::LiveChanged => {
                if let Some(z) = self.zap.as_mut() {
                    z.stale();
                }
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Video
    }

    fn modal(&self) -> bool {
        self.overlay != Overlay::None
    }
}

/// "101  Name": the channel's number and its name as the category lists it.
fn numbered_name(c: &LiveChannel, category: &LiveCategory) -> String {
    let name = channel_name(c, Some(&page_category(category)));
    match c.number.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => format!("{n}  {name}"),
        None => name,
    }
}

/// While a channel tunes, on black: its logo, the ring, its number and name, and what's on or
/// "Tuning…" (`Tuning`).
fn tuning(ui: &mut Ui, cx: &AppCx, stage: Rect, c: &LiveChannel, category: &LiveCategory, seconds: i64, now: f64) {
    let height = 112.0 + 22.0 + 36.0 + 18.0 + theme::SECTION.line_height + 6.0 + theme::BODY.line_height;
    let mut y = stage.y + (stage.h - height) / 2.0;
    let cx_ = stage.center().x;
    let well = Well::PAGE.named().styled(theme::TITLE).filled(Color::WHITE.alpha(0.06), radius::DIALOG);
    channel_tile(ui, cx, Rect::new(cx_ - 100.0, y, 200.0, 112.0), c, well);
    y += 112.0 + 22.0;
    LoadingRing::tuning().show(ui, Point::new(cx_, y + 18.0), now);
    y += 36.0 + 18.0;
    centred(ui, &numbered_name(c, category), &theme::SECTION, cx_, y, stage.w - 96.0, theme::FG);
    y += theme::SECTION.line_height + 6.0;
    let on = c.on_at(seconds).map_or("Tuning…", |p| p.title.as_str());
    centred(ui, on, &theme::BODY, cx_, y, stage.w - 96.0, theme::MUTED);
}

/// A channel with sound and no picture: its logo where the picture would be, with what's on (`SoundOnly`).
fn sound_only(ui: &mut Ui, cx: &AppCx, stage: Rect, c: &LiveChannel, seconds: i64) {
    let on = c.on_at(seconds);
    let height = 135.0 + 18.0 + 20.0 + on.map_or(0.0, |_| 8.0 + theme::SECTION.line_height);
    let mut y = stage.y + (stage.h - height) / 2.0;
    let x = stage.center().x;
    let well = Well::PAGE.named().styled(theme::TITLE).filled(Color::WHITE.alpha(0.06), radius::DIALOG);
    channel_tile(ui, cx, Rect::new(x - 120.0, y, 240.0, 135.0), c, well);
    y += 135.0 + 18.0;
    let label = "Sound only";
    let w = 20.0 + 8.0 + measure(ui, label, &theme::LABEL);
    ui.icon(Rect::new(x - w / 2.0, y, 20.0, 20.0), Icon::Radio, theme::MUTED);
    line_mid(ui, label, &theme::LABEL, x - w / 2.0 + 28.0, y, 20.0, w, theme::MUTED);
    y += 20.0;
    if let Some(p) = on {
        centred(ui, &p.title, &theme::SECTION, x, y + 8.0, stage.w - 96.0, theme::FG);
    }
}

/// One line of `text` centred on `x` from `y`, cut at `width`.
fn centred(ui: &mut Ui, text: &str, style: &pley_base::TextStyle, x: f32, y: f32, width: f32, color: Color) {
    let layout = ui.layout(text, style, &TextOpts::line(width.max(1.0)));
    let w = layout.size.w;
    ui.draw_text(Point::new(x - w / 2.0, y), layout, color);
}

/// The bar at the foot (`InfoBar`): the channel's logo, number and name, what's on with its times, how
/// far it is and what's next, the Live badge at the right; with `controls`, the channel list and
/// favourite buttons, else the hint. It slides up a third of its height and fades as `p` goes to 1.
#[allow(clippy::too_many_arguments)]
fn info_bar(
    ui: &mut Ui,
    cx: &AppCx,
    id: Id,
    c: &LiveChannel,
    category: &LiveCategory,
    clock: &Clock,
    live: bool,
    controls: bool,
    p: f32,
    focus: bool,
) -> Option<BarAction> {
    let seconds = clock.now;
    let on = c.on_at(seconds).cloned();
    let next = c.next_at(seconds).cloned();
    let column_h = theme::LABEL.line_height
        + 2.0
        + theme::SECTION.line_height
        + on.as_ref().map_or(0.0, |_| 6.0 + theme::CAPTION.line_height)
        + next.as_ref().map_or(0.0, |_| 4.0 + theme::CAPTION.line_height);
    let row_h = column_h.max(63.0);
    let after = if controls { 16.0 + ROUND_SIZE + theme::BUTTON_LABEL_ROOM } else { 12.0 + theme::CAPTION.line_height };
    let h = 64.0 + row_h + after + theme::SAFE_Y;
    let top = theme::CANVAS_H - h;
    let mut action = None;
    ui.translate(Point::new(0.0, (1.0 - p) * h / 3.0), |ui| {
        ui.opacity(p, |ui| {
            let stops = vec![
                pley_base::GradientStop::new(0.0, Color::TRANSPARENT),
                pley_base::GradientStop::new(0.5, Color::BLACK.alpha(0.75)),
                pley_base::GradientStop::new(1.0, Color::BLACK.alpha(0.92)),
            ];
            ui.gradient(Rect::new(0.0, top, theme::CANVAS_W, h), true, stops);
            let x0 = theme::SAFE_X;
            let right = theme::CANVAS_W - theme::SAFE_X;
            let row_top = top + 64.0;
            let well = Well::PAGE.named().filled(Color::WHITE.alpha(0.08), radius::TILE);
            channel_tile(ui, cx, Rect::new(x0, row_top + (row_h - 63.0) / 2.0, 112.0, 63.0), c, well);

            // Live at the right, beside the column.
            let mut end = right;
            if live {
                let w = live_pill(ui, right, row_top + row_h / 2.0);
                end = right - w - 18.0;
            }
            let x = x0 + 112.0 + 18.0;
            let width = (end - x).max(1.0);
            let mut y = row_top + (row_h - column_h) / 2.0;

            // The number, the name, the star.
            let mut lx = x;
            if let Some(n) = c.number.as_deref().filter(|n| !n.is_empty()) {
                lx += ui.text(Point::new(lx, y), n, &theme::LABEL, &TextOpts::line(width), theme::MUTED).w + 8.0;
            }
            let name = channel_name(c, Some(&page_category(category)));
            let room = (x + width - lx - if c.favorite { 20.0 } else { 0.0 }).max(1.0);
            lx += ui.text(Point::new(lx, y), &name, &theme::LABEL, &TextOpts::line(room), theme::MUTED).w;
            if c.favorite {
                let star = Rect::new(lx + 6.0, y + (theme::LABEL.line_height - 14.0) / 2.0, 14.0, 14.0);
                ui.icon(star, Icon::Star, theme::ACCENT);
            }
            y += theme::LABEL.line_height + 2.0;

            // What's on, with its badge.
            let mut tx = x;
            if let Some(p) = &on {
                let bw = programme_badge(ui, Point::new(x, y + (theme::SECTION.line_height - 17.0) / 2.0), p);
                if bw > 0.0 {
                    tx += bw + 8.0;
                }
            }
            let title = match &on {
                Some(p) => p.title.as_str(),
                None if c.has_guide => "Nothing in the guide now",
                None => "No guide for this channel",
            };
            ui.text(Point::new(tx, y), title, &theme::SECTION, &TextOpts::line((x + width - tx).max(1.0)), theme::FG);
            y += theme::SECTION.line_height;

            // Its times, what's left and how far it is.
            if let Some(p) = &on {
                y += 6.0;
                let span = format!("{} · {}", clock.span(p), time_left(p.stop, seconds));
                let sw = ui.text(Point::new(x, y), &span, &theme::CAPTION, &TextOpts::line(width), theme::MUTED).w;
                let bar = Rect::new(x + sw + 12.0, y + (theme::CAPTION.line_height - 3.0) / 2.0, 180.0, 3.0);
                if bar.right() <= x + width {
                    progress(ui, bar, p, seconds, Color::WHITE.alpha(0.18), theme::ACCENT);
                }
                y += theme::CAPTION.line_height;
            }
            if let Some(n) = &next {
                y += 4.0;
                let text = format!("Next {} · {}", clock.time(n.start), n.title);
                ui.text(Point::new(x, y), &text, &theme::CAPTION, &TextOpts::line(width), theme::SUBTLE);
            }

            let below = row_top + row_h;
            if controls {
                let at = below + 16.0;
                let (channels_id, favourite_id) = (id.with("channels"), id.with("favourite"));
                ui.initial_focus(channels_id);
                if focus {
                    ui.request_focus(channels_id);
                }
                let favourite = if c.favorite {
                    RoundButton::new(Icon::Star, "Remove from favourites")
                } else {
                    RoundButton::new(Icon::StarOutline, "Add to favourites")
                };
                ui.group(id.with("controls"), bar_group(), |ui| {
                    let r = RoundButton::new(Icon::List, "Channels").show(ui, channels_id, Point::new(x0, at));
                    if r.activated {
                        action = Some(BarAction::Channels);
                    }
                    let fx = x0 + ROUND_SIZE + theme::BUTTON_GAP;
                    let f = favourite.show(ui, favourite_id, Point::new(fx, at));
                    if f.activated {
                        action = Some(BarAction::Favourite);
                    }
                    if r.menu || f.menu {
                        action = Some(BarAction::Menu);
                    }
                });
            } else {
                let hint = "▲▼ change channel · OK for channels · hold OK for more";
                ui.text(
                    Point::new(x0, below + 12.0),
                    hint,
                    &theme::CAPTION,
                    &TextOpts::line(right - x0),
                    theme::SUBTLE,
                );
            }
        });
    });
    action
}

/// Live: a red pill with a white dot, its right edge at `right`, centred on `mid`; its width.
fn live_pill(ui: &mut Ui, right: f32, mid: f32) -> f32 {
    let text = ui.layout("LIVE", &theme::BADGE, &TextOpts::default());
    let (tw, th) = (text.size.w, text.size.h);
    let (w, h) = (12.0 + 7.0 + 7.0 + tw + 12.0, 6.0 + th + 6.0);
    let rect = Rect::new(right - w, mid - h / 2.0, w, h);
    ui.fill(rect, theme::DANGER, h / 2.0);
    ui.fill(Rect::new(rect.x + 12.0, mid - 3.5, 7.0, 7.0), Color::WHITE, 3.5);
    ui.draw_text(Point::new(rect.x + 26.0, rect.y + 6.0), text, Color::WHITE);
    w
}

/// The number typed, or "No channel 123", at the top right inside the safe area (`Digits`).
fn digits_box(ui: &mut Ui, text: &str, code: bool) {
    let style = if code { theme::CODE } else { theme::SECTION };
    let layout = ui.layout(text, &style, &TextOpts::default());
    let (w, h) = (layout.size.w + 48.0, layout.size.h + 24.0);
    let rect = Rect::new(theme::CANVAS_W - theme::SAFE_X - w, theme::SAFE_Y, w, h);
    ui.fill(rect, Color::BLACK.alpha(0.85), radius::DIALOG);
    ui.draw_text(Point::new(rect.x + 24.0, rect.y + 12.0), layout, theme::FG);
}

/// "Its 2 connections are watching:".
pub fn connections_heading(n: usize) -> String {
    if n == 1 { "Its one connection is watching:".into() } else { format!("Its {n} connections are watching:") }
}

/// How many of `n` connections the busy card lists with `room` dp for their rows and what follows
/// them: all of them when they fit with `note` (the footnote under every one), else as many as fit
/// above "and N more" (`more` dp tall), one at least.
fn connections_shown(n: usize, room: f32, note: f32, more: f32) -> usize {
    let pitch = BUSY_ROW_H + BUSY_ROW_GAP;
    if n as f32 * pitch + note <= room + 0.5 {
        return n;
    }
    let fit = ((room - more) / pitch + 0.01).floor().max(1.0) as usize;
    fit.min(n.saturating_sub(1)).max(1).min(n)
}

/// Why the channel doesn't play, centred over black (`Trouble`): the channel's logo (or a warning), the
/// title, the server's words or the connections in use, then Try again, Other channels and Back to the
/// guide. `focus` sends focus to its first button: Watch here instead, else Try again, else Other channels.
fn trouble_card(
    ui: &mut Ui,
    cx: &AppCx,
    id: Id,
    t: &Trouble,
    channel: Option<&LiveChannel>,
    focus: bool,
) -> Option<CardAction> {
    let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
    ui.fill(full, Color::BLACK.alpha(0.88), 0.0);
    let inner_max = CARD_MAX - 2.0 * CARD_PAD;
    let connections: &[BusyConnection] = &t.connections;

    // The buttons, to measure the card by.
    let retry = t.can_retry.then(|| Button::pill("Try again").icon(Icon::RefreshCw));
    let others = Button::pill("Other channels").icon(Icon::List);
    let back = Button::pill("Back to the guide");
    let gap = theme::BUTTON_GAP;
    let mut buttons_w = others.size(ui).w + gap + back.size(ui).w;
    if let Some(r) = &retry {
        buttons_w += r.size(ui).w + gap;
    }
    let centered = |w: f32| TextOpts::wrap(w, 3).aligned(Align::Center);
    let title_w = measure(ui, &t.title, &theme::SECTION);
    let message = t.message.trim();
    let message_w = if message.is_empty() { 0.0 } else { measure(ui, message, &theme::BODY) };
    let inner = if connections.is_empty() {
        buttons_w.max(title_w).max(message_w).max(120.0).min(inner_max)
    } else {
        inner_max
    }
    .max(buttons_w);
    let title = ui.layout(&t.title, &theme::SECTION, &centered(inner));
    let heading = (!connections.is_empty())
        .then(|| ui.layout(&connections_heading(connections.len()), &theme::BODY, &centered(inner)));
    let top_h = if channel.is_some() { 68.0 } else { 40.0 };
    let mut h = top_h + 18.0 + title.size.h;
    if let Some(hd) = &heading {
        h += 10.0 + hd.size.h + 14.0;
    }
    h += 24.0 + theme::BUTTON_HEIGHT;

    // As many connections as the screen has room for, the rest counted (Android lists them all; on a
    // 540 dp screen four or five would push the buttons off it).
    let none_to_take = !connections.iter().any(|c| c.can_take_over);
    let note = "A channel already being watched can be joined, or try again once one is stopped.";
    let note = (!connections.is_empty() && none_to_take).then(|| ui.layout(note, &theme::CAPTION, &centered(inner)));
    let room = theme::CANVAS_H - 2.0 * theme::SAFE_Y - 2.0 * CARD_PAD - h;
    let note_h = note.as_ref().map_or(0.0, |n| 6.0 + n.size.h);
    let shown = connections_shown(connections.len(), room, note_h, 6.0 + theme::CAPTION.line_height);
    let more = connections.len() - shown;
    let first_to_take = connections.iter().take(shown).find(|c| c.can_take_over).map(|c| c.channel_id);
    let footnote =
        if more > 0 { Some(ui.layout(&format!("and {more} more"), &theme::CAPTION, &centered(inner))) } else { note };
    let words =
        (connections.is_empty() && !message.is_empty()).then(|| ui.layout(message, &theme::BODY, &centered(inner)));

    h += shown as f32 * (BUSY_ROW_H + BUSY_ROW_GAP);
    if let Some(f) = &footnote {
        h += 6.0 + f.size.h;
    }
    if let Some(wd) = &words {
        h += 10.0 + wd.size.h;
    }
    let card = Rect::new(
        (theme::CANVAS_W - inner - 2.0 * CARD_PAD) / 2.0,
        ((theme::CANVAS_H - h - 2.0 * CARD_PAD) / 2.0).max(theme::SAFE_Y),
        inner + 2.0 * CARD_PAD,
        h + 2.0 * CARD_PAD,
    );
    ui.fill(card, theme::SURFACE2, radius::DIALOG);
    ui.stroke(card.expand(-0.5), theme::SURFACE4, radius::DIALOG - 0.5, 1.0);
    let (x, mid) = (card.x + CARD_PAD, card.center().x);
    let mut y = card.y + CARD_PAD;
    match channel {
        Some(c) => {
            let well = Well::PAGE.named().filled(Color::WHITE.alpha(0.06), radius::TILE);
            channel_tile(ui, cx, Rect::new(mid - 60.0, y, 120.0, 68.0), c, well);
        }
        None => ui.icon(Rect::new(mid - 20.0, y, 40.0, 40.0), Icon::CircleAlert, theme::ACCENT),
    }
    y += top_h + 18.0;
    let th = title.size.h;
    ui.draw_text(Point::new(x, y), title, theme::FG);
    y += th;

    let mut action = None;
    let mut first_focus: Option<Id> = None;
    if let Some(hd) = heading {
        y += 10.0;
        let hh = hd.size.h;
        ui.draw_text(Point::new(x, y), hd, theme::MUTED);
        y += hh + 14.0;
        for (i, c) in connections.iter().take(shown).enumerate() {
            let row = Rect::new(x, y, inner, BUSY_ROW_H);
            ui.fill(row, Color::WHITE.alpha(0.04), radius::CONTROL);
            let take = c.can_take_over.then(|| Button::pill("Watch here instead"));
            let take_w = take.as_ref().map_or(0.0, |b| b.size(ui).w + 12.0);
            let text_w = (inner - 32.0 - take_w).max(1.0);
            let block = theme::LABEL.line_height + theme::CAPTION.line_height;
            let ty = y + (BUSY_ROW_H - block) / 2.0;
            ui.text(Point::new(x + 16.0, ty), &c.channel_name, &theme::LABEL, &TextOpts::line(text_w), theme::FG);
            let by = Point::new(x + 16.0, ty + theme::LABEL.line_height);
            ui.text(by, &holders(c), &theme::CAPTION, &TextOpts::line(text_w), theme::MUTED);
            if let Some(b) = take {
                let bid = id.with(("take", i as u64));
                let size = b.size(ui);
                let at = Point::new(row.right() - 16.0 - size.w, y + (BUSY_ROW_H - size.h) / 2.0);
                let r = b.show(ui, bid, at);
                if r.activated {
                    action = Some(CardAction::TakeOver(c.channel_id));
                } else if r.menu {
                    action = Some(CardAction::Channels);
                }
                if Some(c.channel_id) == first_to_take && first_focus.is_none() {
                    first_focus = Some(bid);
                }
            }
            y += BUSY_ROW_H + BUSY_ROW_GAP;
        }
        if let Some(f) = footnote {
            y += 6.0;
            let fh = f.size.h;
            ui.draw_text(Point::new(x, y), f, theme::SUBTLE);
            y += fh;
        }
    } else if let Some(wd) = words {
        y += 10.0;
        let wh = wd.size.h;
        ui.draw_text(Point::new(x, y), wd, theme::MUTED);
        y += wh;
    }

    y += 24.0;
    let mut bx = mid - buttons_w / 2.0;
    let (retry_id, others_id, back_id) = (id.with("retry"), id.with("others"), id.with("back"));
    ui.group(id.with("buttons"), bar_group(), |ui| {
        let mut menu = false;
        if let Some(b) = &retry {
            let r = b.show(ui, retry_id, Point::new(bx, y));
            if r.activated {
                action = Some(CardAction::Retry);
            }
            menu |= r.menu;
            bx += b.size(ui).w + gap;
        }
        let r = others.show(ui, others_id, Point::new(bx, y));
        if r.activated {
            action = Some(CardAction::Channels);
        }
        menu |= r.menu;
        bx += others.size(ui).w + gap;
        let r = back.show(ui, back_id, Point::new(bx, y));
        if r.activated {
            action = Some(CardAction::Back);
        }
        // The menu key opens the channel list here too.
        if menu || r.menu {
            action = Some(CardAction::Channels);
        }
    });
    let first = first_focus.unwrap_or(if retry.is_some() { retry_id } else { others_id });
    ui.initial_focus(first);
    if focus {
        ui.request_focus(first);
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_wait_then_look_up_once() {
        let mut d = Digits::default();
        d.push(1, 10.0);
        d.push(2, 10.5);
        assert_eq!(d.text(10.6), Some(("12".into(), true)));
        assert_eq!(d.due(11.8), None, "1.4 s after the last digit, not the first");
        assert_eq!(d.wake(), Some(10.5 + DIGITS_WAIT));
        assert_eq!(d.due(11.9).as_deref(), Some("12"));
        assert_eq!(d.due(12.5), None, "asked once");
        assert_eq!(d.text(12.5), Some(("12".into(), true)), "the digits stay while the lookup runs");
        d.found();
        assert_eq!(d.text(12.6), None);
    }

    #[test]
    fn five_digits_at_most_and_a_miss_says_so_for_two_seconds() {
        let mut d = Digits::default();
        for (i, k) in [1, 2, 3, 4, 5, 6, 7].into_iter().enumerate() {
            d.push(k, i as f64 * 0.1);
        }
        assert_eq!(d.typed, "12345");
        let number = d.due(5.0).unwrap();
        assert_eq!(number, "12345");
        d.missed(5.2);
        assert_eq!(d.text(5.3), Some(("No channel 12345".into(), false)));
        assert_eq!(d.wake(), Some(5.2 + NOT_FOUND));
        assert_eq!(d.text(7.3), None);
        // A digit typed meanwhile starts a new number.
        d.missed(8.0);
        d.push(9, 8.5);
        assert_eq!(d.text(8.6), Some(("9".into(), true)));
    }

    #[test]
    fn a_digit_during_the_lookup_starts_the_wait_again() {
        let mut d = Digits::default();
        d.push(4, 0.0);
        assert_eq!(d.due(1.5).as_deref(), Some("4"));
        d.push(2, 1.6);
        assert_eq!(d.due(2.0), None);
        assert_eq!(d.due(3.0).as_deref(), Some("42"));
    }

    #[test]
    fn the_picture_moves_aside_into_what_the_list_leaves() {
        let r = picture_aside();
        assert_eq!((r.x, r.w), (0.0, 600.0));
        assert!((r.h - 337.5).abs() < 0.01 && (r.y - 101.25).abs() < 0.01);
        assert!(r.right() <= theme::CANVAS_W - channel_list::WIDTH);
    }

    #[test]
    fn the_busy_heading_counts_the_connections() {
        assert_eq!(connections_heading(1), "Its one connection is watching:");
        assert_eq!(connections_heading(3), "Its 3 connections are watching:");
    }

    #[test]
    fn the_busy_card_lists_what_fits_and_counts_the_rest() {
        let pitch = BUSY_ROW_H + BUSY_ROW_GAP;
        // Room for three rows exactly.
        assert_eq!(connections_shown(3, 3.0 * pitch, 0.0, 23.0), 3);
        assert_eq!(connections_shown(2, 3.0 * pitch, 0.0, 23.0), 2);
        // Four don't fit: two, and "and 2 more" under them.
        assert_eq!(connections_shown(4, 3.0 * pitch, 0.0, 23.0), 2);
        // The footnote under every one counts too.
        assert_eq!(connections_shown(3, 3.0 * pitch, 40.0, 23.0), 2);
        // One at least, however little room.
        assert_eq!(connections_shown(5, 10.0, 0.0, 23.0), 1);
        assert_eq!(connections_shown(1, 10.0, 0.0, 23.0), 1);
        assert_eq!(connections_shown(0, 10.0, 0.0, 23.0), 0);
    }
}
