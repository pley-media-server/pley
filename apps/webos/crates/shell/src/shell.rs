//! The shell (the TV app's `ui/shell/app.tsx`, Android's `TvApp.kt` and `TvNavigator.kt`): one
//! page at a time from the back stack, the rail beside the pages that have it, the curtain
//! between pages, a tile's actions over the page (Android's `TvTileActionsPanel`), toasts, the
//! offline notice, the idle dimmer, and Back in the Android app's order:
//! what the page has open, then the rail leaves the app, a page above the first pops, a first
//! page opens the rail, else the app is left. Leaving follows LG's Back-button guide
//! (<https://webostv.developer.lge.com/develop/guides/back-button>): from webOS 6 it asks "Exit
//! Pley?" first, as the Samsung app does (`Kit/Shell.cs`); up to webOS 5 the TV's Home shows.
//! What the user's remotes cast to the TV opens its player over whatever page shows ([`cast`]).
//!
//! [`cast`]: crate::cast

use pley_base::{ImageRequest, Key};
use pley_core::ApiError;
use pley_core::model::{Library, LiveCategory, RequestSummary, ServerEvent};
use pley_kit::{Dialog, DialogButton};
use pley_ui::{Dir, Edge, Group, Id, InputEvent, PageMemory, Ui, UiState};

use crate::cast::{
    CAST_NOTICE_SECONDS, Cast, CastNews, CastPlay, Casts, NOTHING_TO_PLAY, notice, opening, switch_refused,
};
use crate::cx::{AppCx, NavCommand, Requests, ToastRequest};
use crate::notice::Notices;
use crate::overlay::{self, DIM_AFTER, TOAST_TOP, TOAST_TOP_PLAYER, Toasts};
use crate::rail::{Rail, RailEntries, RailView, current_entry, rail_entries};
use crate::route::{PlayRequest, Route};
use crate::screen::{AppEvent, Chrome, Overlay, Registry, Screen};
use crate::state::AppState;

/// One page on the back stack: its route, its screen (kept while pages above it show) and the
/// focus it had.
pub struct Entry {
    id: u64,
    route: Route,
    screen: Box<dyn Screen>,
    memory: PageMemory,
    /// It missed a [`AppEvent::Changed`] while under another page: it hears it when on top again.
    missed: bool,
    /// A player: refreshes wait while it is on the stack.
    holds: bool,
}

impl Entry {
    /// Unique per page opened: its focus ids are scoped by it.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The page it shows.
    pub fn route(&self) -> &Route {
        &self.route
    }

    /// Its screen.
    pub fn screen(&self) -> &dyn Screen {
        self.screen.as_ref()
    }

    /// The focus group the page draws in.
    pub fn group_id(&self) -> Id {
        Id::new("pley.page").with(self.id)
    }
}

/// A tile's actions over the page they were asked on.
struct PagePanel {
    /// The page's [`Entry::id`]: the panel goes with it.
    page: u64,
    /// Unique per panel opened: its focus ids are scoped by it.
    serial: u64,
    overlay: Box<dyn Overlay>,
}

/// What the rail's entries are made from.
struct EntriesFrom<L = Vec<Library>, R = Option<RequestSummary>> {
    libraries: L,
    queued: bool,
    live: bool,
    requests: R,
    drive: bool,
}

/// The same, borrowed from the app's state for a frame.
type EntriesNow<'a> = EntriesFrom<&'a [Library], Option<&'a RequestSummary>>;

impl EntriesFrom {
    fn is(&self, now: &EntriesNow) -> bool {
        self.libraries == now.libraries
            && self.queued == now.queued
            && self.live == now.live
            && self.requests.as_ref() == now.requests
            && self.drive == now.drive
    }
}

impl EntriesNow<'_> {
    fn kept(&self) -> EntriesFrom {
        EntriesFrom {
            libraries: self.libraries.to_vec(),
            queued: self.queued,
            live: self.live,
            requests: self.requests.cloned(),
            drive: self.drive,
        }
    }
}

/// The shell: the back stack and everything drawn around the page on top.
pub struct Shell {
    registry: Registry,
    stack: Vec<Entry>,
    next_id: u64,
    rail: Rail,
    /// A tile's actions over the page on top.
    panel: Option<PagePanel>,
    toasts: Toasts,
    /// Ready-to-watch news over the pages.
    notices: Notices,
    /// A notice waits to show (for [`Shell::next_due`]).
    notice_waiting: bool,
    requests: Requests,
    /// The page on top when the last frame drew, for the curtain and [`AppEvent::Shown`].
    shown: Option<u64>,
    curtain: Option<u64>,
    last_input: f64,
    dimmed: bool,
    music_due: Option<f64>,
    exited: bool,
    /// "Exit Pley?" is open.
    exit_asked: bool,
    /// It was answered Exit: the app ends.
    quit: bool,
    entries: RailEntries,
    /// What [`Shell::entries`] were made from: they are made again only when it changes.
    entries_from: Option<EntriesFrom>,
    /// The server was asked for who is watching, the libraries and Home, after the first frame.
    asked: bool,
    /// What a remote cast, while what it plays is found.
    casts: Casts,
}

impl Shell {
    /// A shell drawing pages with `registry`'s screens. [`Shell::start`] opens the first page.
    pub fn new(registry: Registry) -> Shell {
        Shell {
            registry,
            stack: Vec::new(),
            next_id: 1,
            rail: Rail::default(),
            panel: None,
            toasts: Toasts::default(),
            notices: Notices::default(),
            notice_waiting: false,
            requests: Requests::default(),
            shown: None,
            curtain: None,
            last_input: pley_base::time::now(),
            dimmed: false,
            music_due: None,
            exited: false,
            exit_asked: false,
            quit: false,
            entries: rail_entries(&[], false, false, None, false),
            entries_from: None,
            asked: false,
            casts: Casts::default(),
        }
    }

    /// Opens the first page (the start gate's, [`AppState::start_route`], or one asked for).
    pub fn start(&mut self, state: &mut AppState, ui: &mut UiState, route: Route) {
        let now = state.now();
        self.apply(NavCommand::Reset(route), state, ui, now);
    }

    // What the app asks of it.

    /// The page on top.
    pub fn top(&self) -> Option<&Entry> {
        self.stack.last()
    }

    /// The route of the page on top.
    pub fn top_route(&self) -> Option<&Route> {
        self.stack.last().map(|e| &e.route)
    }

    /// The routes of the back stack, first page first.
    pub fn routes(&self) -> Vec<&Route> {
        self.stack.iter().map(|e| &e.route).collect()
    }

    /// How many pages are on the stack.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Whether the rail is open (it has focus).
    pub fn rail_open(&self) -> bool {
        self.rail.is_open()
    }

    /// The rail's entries as last drawn.
    pub fn rail_entries(&self) -> &RailEntries {
        &self.entries
    }

    /// Whether a tile's actions are open over the page (they have focus).
    pub fn panel_open(&self) -> bool {
        self.panel.as_ref().is_some_and(|p| p.overlay.modal())
    }

    /// The toast showing.
    pub fn toast(&self) -> Option<&str> {
        self.toasts.text()
    }

    /// Whether the idle dimmer is down.
    pub fn dimmed(&self) -> bool {
        self.dimmed
    }

    /// Whether the app asked to be left (Back in the rail, or on a first page without it, up to
    /// webOS 5; Exit in "Exit Pley?" after).
    pub fn exited(&self) -> bool {
        self.exited
    }

    /// Whether "Exit Pley?" is open.
    pub fn exit_asked(&self) -> bool {
        self.exit_asked
    }

    /// Whether "Exit Pley?" was answered Exit: the frame loop ends, and with it the app.
    pub fn quit(&self) -> bool {
        self.quit
    }

    /// The chrome of the page on top.
    pub fn chrome(&self) -> Chrome {
        self.stack.last().map_or(Chrome::Bare, |e| e.screen.chrome())
    }

    /// Whether the rail shows: the stack's first page and the page on top both have it.
    pub fn rail_shown(&self, state: &AppState) -> bool {
        let root = self.stack.first().map(|e| e.screen.chrome());
        root == Some(Chrome::Rail) && self.chrome() == Chrome::Rail && state.api().is_some()
    }

    /// When the shell next needs a frame: a toast's end, the dimmer, the music's clock.
    pub fn next_due(&self) -> Option<f64> {
        let dim = (!self.dimmed && self.chrome() != Chrome::Video).then_some(self.last_input + DIM_AFTER);
        [self.toasts.next_due(), dim, self.music_due, self.notices.next_due(self.notice_waiting)]
            .into_iter()
            .flatten()
            .reduce(f64::min)
    }

    // Each frame.

    /// The frame's input, before the interface sees it: any input wakes the dimmed screen, and the
    /// key that wakes it does nothing more.
    pub fn filter_input(&mut self, events: &mut Vec<InputEvent>, now: f64) {
        if events.is_empty() {
            return;
        }
        self.last_input = now;
        if std::mem::take(&mut self.dimmed) {
            events.retain(|e| !matches!(e, InputEvent::Key(_)));
        }
    }

    /// Runs what isn't drawn: the state's answers and timers (telling the pages what they say),
    /// a cast found, the music, the dimmer.
    pub fn update(&mut self, state: &mut AppState, ui: &mut UiState, now: f64) {
        for e in state.update(now) {
            self.deliver(e, state, ui, now);
        }
        match self.casts.take(state.generation()) {
            Some(CastNews::Switched { profile, cast }) => self.switched_for_cast(profile, cast, state, ui, now),
            Some(CastNews::Refused(e)) => {
                log::info!("the server didn't switch the TV to the profile that cast: {e}");
                if let Some(text) = switch_refused(&e).filter(|_| self.takes_casts(state)) {
                    self.requests.toasts.push(ToastRequest { text, error: true, seconds: None });
                    self.carry_out(state, ui, now);
                }
            }
            Some(CastNews::Found { from, found }) => self.play_cast(from.as_deref(), found, state, ui, now),
            None => {}
        }
        if let Some(mut music) = state.music.take() {
            self.music_due = music.tick(now);
            if state.music.is_none() {
                state.music = Some(music);
            } else {
                music.end();
            }
        } else {
            self.music_due = None;
        }
        if !self.dimmed && self.chrome() != Chrome::Video && now >= self.last_input + DIM_AFTER {
            self.dimmed = true;
        }
    }

    /// The app was shown (`true`) or hidden: the state follows, the page hears it, and coming back
    /// never finds the screen dimmed. Hidden, a cast still being found plays nothing.
    pub fn visible(&mut self, visible: bool, state: &mut AppState, ui: &mut UiState, now: f64) {
        state.set_visible(visible);
        if visible {
            self.last_input = now;
            self.dimmed = false;
        } else {
            self.casts.cancel();
        }
        self.deliver(AppEvent::Visible(visible), state, ui, now);
    }

    /// Whether a cast is being found (tests wait for it).
    pub fn casting(&self) -> bool {
        self.casts.busy()
    }

    /// Tells the page on top `event` (the pages under it hear [`AppEvent::Changed`] when on top
    /// again); a sign-in the server ended goes back to its code screen; a cast for this TV starts
    /// being found.
    pub fn deliver(&mut self, event: AppEvent, state: &mut AppState, ui: &mut UiState, now: f64) {
        if let AppEvent::Server(e) = &event {
            self.hear_cast(e, state, now);
        }
        if event == AppEvent::Changed {
            let n = self.stack.len();
            for e in self.stack.iter_mut().take(n.saturating_sub(1)) {
                e.missed = true;
            }
        }
        if let Some(top) = self.stack.last_mut() {
            let mut cx = AppCx::new(state, &mut self.requests, now);
            top.screen.event(&event, &mut cx);
        }
        if event == AppEvent::SignedOut
            && let Some(server) = state.server().map(str::to_string)
            && !matches!(self.top_route(), Some(Route::Link { .. } | Route::Servers))
        {
            self.requests.nav.push(NavCommand::Reset(Route::Link { server }));
        }
        self.carry_out(state, ui, now);
    }

    /// Draws the page on top and everything around it, and handles Back, Left past the page's
    /// edge and the rail.
    pub fn frame(&mut self, ui: &mut Ui, state: &mut AppState) {
        let now = ui.time();
        let Some(top) = self.stack.len().checked_sub(1) else { return };
        let rail_shown = self.rail_shown(state);
        if rail_shown {
            let from = EntriesFrom {
                libraries: state.libraries().unwrap_or(&[]),
                queued: state.music.as_ref().is_some_and(|m| m.queued()),
                live: state.live_available(),
                requests: state.request_summary(),
                // The drive's entry shows only while the app can read it: a drive the jail hides
                // has nothing to open.
                drive: state.device().drive().is_some_and(|d| d.readable()),
            };
            // Made again only when what they show changed, not every frame the rail is drawn.
            if self.entries_from.as_ref().is_none_or(|e| !e.is(&from)) {
                let requests = crate::rail::requests_entry(from.requests);
                self.entries = rail_entries(from.libraries, from.queued, from.live, requests, from.drive);
                self.entries_from = Some(from.kept());
            }
        } else if self.rail.is_open() {
            self.rail.close();
        }
        let entry_id = self.stack[top].id;
        if self.shown != Some(entry_id) {
            // A new page on top: the curtain (none for the first, none with Reduce motion: its
            // motion snaps then).
            if self.shown.is_some() {
                self.curtain = Some(entry_id);
            }
            self.shown = Some(entry_id);
            // Audio Guidance reads the page's name, then what has focus: the page's own title,
            // else its rail entry's.
            let entry = &self.stack[top];
            let title = entry.screen.title().or_else(|| {
                let id = current_entry(Some(&entry.route), &self.entries)?;
                self.entries.top.iter().chain([&self.entries.settings]).find(|e| e.id == id).map(|e| e.label.clone())
            });
            if let Some(title) = title {
                ui.announce_page(title);
            }
        }

        // A tile's actions go with the page they were asked on.
        if self.panel.as_ref().is_some_and(|p| p.page != entry_id) {
            self.panel = None;
        }
        let panel_open = self.panel_open();

        // Back, before the page draws: in the rail it leaves the app; else the page's own. A
        // notice on screen, "Exit Pley?" or a tile's actions take it themselves.
        let asking = self.exit_asked;
        if ui.key_down(Key::Back) && !self.notices.showing() && !asking && !panel_open {
            if self.rail.is_open() {
                ui.take_key(Key::Back);
                self.requests.nav.push(NavCommand::Exit);
            } else {
                let mut cx = AppCx::new(state, &mut self.requests, now);
                if self.stack[top].screen.back(&mut cx) {
                    ui.take_key(Key::Back);
                }
            }
        }
        // A page chosen in the rail takes focus once it has something to focus.
        if self.rail.handing() && self.stack[top].screen.takes_focus() {
            self.rail.close();
        }

        // The remote's media keys the page answers itself (Live TV's guide), then the rest to the
        // music on every page but a player's (`App.cs`'s MediaKeys), before the page draws, so it
        // shows the track they lead to.
        if self.stack[top].screen.media_keys(ui) {
            ui.request_repaint();
        }
        if self.chrome() != Chrome::Video
            && let Some(m) = state.music.as_mut()
            && m.media_keys(ui)
        {
            ui.request_repaint();
        }

        let entry = &mut self.stack[top];
        let group = Group::free().edge(Dir::Left, Edge::Out);
        let modal = {
            let mut cx = AppCx::new(state, &mut self.requests, now);
            ui.group(entry.group_id(), group, |ui| entry.screen.ui(ui, &mut cx));
            entry.screen.modal()
        };
        // A tile's actions over the page; the page under them holds still as under its own panel.
        if let Some(p) = self.panel.as_mut() {
            let mut cx = AppCx::new(state, &mut self.requests, now);
            let id = Id::new("pley.tile-actions").with(p.serial);
            if !ui.group(id, Group::free(), |ui| p.overlay.ui(ui, &mut cx)) {
                self.panel = None;
            }
        }
        let modal = modal || self.panel_open();
        if let Some(c) = self.curtain
            && !overlay::curtain(ui, c)
        {
            self.curtain = None;
        }

        for t in std::mem::take(&mut self.requests.toasts) {
            self.toasts.show(t, now);
        }
        // Back the page didn't take, and what the page asked: the rail, or Back's next steps.
        // Left from a page's first column opens the rail on a press of its own: a held Left stops
        // there, so scrolling back along a row never opens it by itself.
        let mut to_rail =
            ui.failed_move() == Some(Dir::Left) && !ui.failed_move_held() && !modal && !self.rail.is_open();
        if !self.notices.showing() && !asking && ui.take_key(Key::Back) {
            self.requests.nav.push(NavCommand::Back);
        }
        let depth = self.stack.len();
        let mut rest = Vec::new();
        for cmd in std::mem::take(&mut self.requests.nav) {
            match cmd {
                NavCommand::ToRail => to_rail = true,
                NavCommand::Back if depth > 1 => rest.push(NavCommand::Pop),
                NavCommand::Back if rail_shown => to_rail = true,
                NavCommand::Back => rest.push(NavCommand::Exit),
                other => rest.push(other),
            }
        }
        self.requests.nav = rest;

        if rail_shown {
            let current = current_entry(self.stack.first().map(|e| &e.route), &self.entries);
            if to_rail && !self.rail.is_open() {
                self.rail.open(ui, current.as_deref().unwrap_or("home"));
            }
            let avatar = state.me().and_then(|m| m.avatar_url.as_deref()).and_then(|url| {
                let api = state.api()?;
                Some(ImageRequest::new(api.url(url), ui.image_px(32.0)))
            });
            let view = RailView {
                entries: &self.entries,
                current: current.as_deref(),
                me: state.me(),
                avatar,
                offline_mark: state.offline() && !state.offline_notice(),
                page_modal: modal || asking,
            };
            if let Some(chosen) = self.rail.ui(ui, &view).chosen {
                let same = self.stack.len() == 1 && self.stack[0].route == chosen.route;
                if same {
                    self.rail.close();
                } else {
                    self.rail.hand_over_later(&chosen.id);
                    self.requests.nav.push(NavCommand::Reset(chosen.route));
                }
            }
        }

        self.exit_dialog(ui);

        let chrome = self.chrome();
        // Ready-to-watch news: never over a video, a panel or dialog, the open rail or Your
        // requests, which lists it already.
        let on_requests = matches!(self.stack.last().map(|e| &e.route), Some(Route::Requests));
        let free = rail_shown && !modal && !self.rail.is_open() && !on_requests && chrome != Chrome::Video;
        self.notice_waiting = state.request_notice().is_some();
        if let Some(n) = self.notices.ui(ui, state, free) {
            let route = n.item_id.map_or(Route::Requests, |id| Route::Item { id });
            self.requests.nav.push(NavCommand::Push(route));
        }
        let top_y = if chrome == Chrome::Video { TOAST_TOP_PLAYER } else { TOAST_TOP };
        let below = overlay::offline_notice(ui, rail_shown && state.offline_notice(), top_y);
        self.toasts.ui(ui, below);
        overlay::dimmer(ui, self.dimmed);
    }

    /// "Exit Pley?" (`AskExit` in the Samsung app's `Kit/Shell.cs`): a narrow card, its title and
    /// Cancel and Exit, Exit focused; Cancel or Back stays. Drawn every frame so it can fade out.
    fn exit_dialog(&mut self, ui: &mut Ui) {
        let buttons = [DialogButton::new("Cancel"), DialogButton::new("Exit")];
        let dialog = Dialog::new("Exit Pley?").buttons(&buttons).initial(1).width(360.0);
        let r = dialog.show(ui, Id::new("pley.exit"), self.exit_asked);
        if r.dismissed || r.chosen == Some(0) {
            self.exit_asked = false;
        }
        if r.chosen == Some(1) {
            self.exit_asked = false;
            self.requests.nav.push(NavCommand::Quit);
        }
    }

    /// Whether the TV plays what a remote casts now: while the app shows and is signed in, past
    /// the pages of getting in (a server, a sign-in, "Who's watching?" and a PIN are never
    /// skipped).
    fn takes_casts(&self, state: &AppState) -> bool {
        state.visible() && state.signed_in() && self.chrome() != Chrome::Bare
    }

    /// A cast for this TV starts being found, in place of one still being found, while the TV
    /// takes casts; one from another profile's remote switches to that profile first. A cast for
    /// another of the profile's devices is that device's.
    fn hear_cast(&mut self, event: &ServerEvent, state: &AppState, now: f64) {
        let Some(cast) = Cast::for_receiver(event, state.install_id(), state.profile()) else { return };
        if !self.takes_casts(state) {
            log::info!("a remote's cast came while the TV can't play it");
            return;
        }
        log::info!("a remote cast to this TV");
        self.casts.start(cast, state, now);
    }

    /// The server switched the TV to the profile of a remote that cast to it: the app acts as that
    /// profile, as a switch on "Who's watching?" does (what plays ends, the event stream opens
    /// again as that profile's), Home shows under what the cast plays, and the cast is found. The
    /// cast waits here, not on the stream, so the new stream neither loses it nor plays it again.
    /// Hidden, or back on the pages of getting in, the TV switches all the same, as the server
    /// has, but plays nothing.
    fn switched_for_cast(
        &mut self,
        profile: i64,
        cast: Option<Cast>,
        state: &mut AppState,
        ui: &mut UiState,
        now: f64,
    ) {
        log::info!("switched profiles for a remote's cast");
        state.switched(profile);
        if self.chrome() != Chrome::Bare {
            self.requests.nav.push(NavCommand::Reset(Route::Home));
            self.carry_out(state, ui, now);
        }
        match cast {
            Some(cast) if self.takes_casts(state) => self.casts.start(cast, state, now),
            _ => log::info!("a remote's cast was let go as the TV switched profiles for it"),
        }
    }

    /// Plays what a cast found as the app's own Play would, in place of what plays: a video or a
    /// channel in its player, over the page on top or in place of a player there; music in the
    /// music player, with Now Playing. "Playing from …" shows over it for 4 s, and the dimmed
    /// screen wakes, as for a key. A cast that found nothing, or that the server couldn't answer,
    /// says so; one found once the TV went back to the pages of getting in plays nothing.
    fn play_cast(
        &mut self,
        from: Option<&str>,
        found: Result<CastPlay, ApiError>,
        state: &mut AppState,
        ui: &mut UiState,
        now: f64,
    ) {
        if !self.takes_casts(state) {
            log::info!("a remote's cast was found once the TV couldn't play it");
            return;
        }
        let route = match found {
            Ok(CastPlay::Video(request)) => Route::Player(PlayRequest { pressed_at: now, ..request }),
            Ok(CastPlay::Channel(channel_id)) => {
                Route::LivePlayer { channel_id, category: LiveCategory::All, pressed_at: now }
            }
            Ok(CastPlay::Music(tracks)) => {
                let mut cx = AppCx::new(state, &mut self.requests, now);
                if !self.registry.play_cast_music(&mut cx, tracks) {
                    log::warn!("a remote cast music, which the app couldn't play");
                    return;
                }
                Route::NowPlaying
            }
            Ok(CastPlay::Nothing) => {
                self.requests.toasts.push(ToastRequest { text: NOTHING_TO_PLAY.into(), error: false, seconds: None });
                self.carry_out(state, ui, now);
                return;
            }
            Err(e) => {
                log::info!("a remote's cast found nothing to play: {e}");
                state.report_failure(&e);
                self.requests.toasts.push(ToastRequest { text: e.problem(), error: true, seconds: None });
                self.carry_out(state, ui, now);
                return;
            }
        };
        if let Some(open) = opening(self.top_route(), route) {
            self.requests.nav.push(open);
        }
        // Played from elsewhere, the TV leaves what it was asking or had open, and wakes: the
        // dimmer would otherwise stay over the picture until a key.
        self.exit_asked = false;
        self.rail.close();
        self.last_input = now;
        self.dimmed = false;
        let text = notice(from);
        self.requests.toasts.push(ToastRequest { text, error: false, seconds: Some(CAST_NOTICE_SECONDS) });
        self.carry_out(state, ui, now);
    }

    /// Carries out a navigation from outside a frame (the simulator's `--route`).
    pub fn navigate(&mut self, cmd: NavCommand, state: &mut AppState, ui: &mut UiState) {
        let now = state.now();
        self.apply(cmd, state, ui, now);
        self.carry_out(state, ui, now);
    }

    /// After the frame: carries out the navigation it asked for. Whether the page on top changed
    /// (the app then draws again at once).
    pub fn finish(&mut self, state: &mut AppState, ui: &mut UiState, now: f64) -> bool {
        let before = self.stack.last().map(|e| e.id);
        if !std::mem::replace(&mut self.asked, true) {
            // The first frame drew what was saved; now the server is asked, and its events followed.
            state.revalidate();
            state.listen(state.visible());
        }
        self.carry_out(state, ui, now);
        self.stack.last().map(|e| e.id) != before
    }

    fn carry_out(&mut self, state: &mut AppState, ui: &mut UiState, now: f64) {
        // A screen made or told something may ask for more; a few rounds settle it.
        for _ in 0..8 {
            let cmds = std::mem::take(&mut self.requests.nav);
            if cmds.is_empty() {
                break;
            }
            for cmd in cmds {
                self.apply(cmd, state, ui, now);
            }
        }
        for t in std::mem::take(&mut self.requests.toasts) {
            self.toasts.show(t, now);
        }
    }

    fn make(&mut self, route: Route, state: &mut AppState, now: f64) -> Entry {
        let mut cx = AppCx::new(state, &mut self.requests, now);
        let screen = self.registry.make(&route, &mut cx);
        let holds = screen.chrome() == Chrome::Video;
        if holds {
            state.hold_refresh(true);
        }
        let id = self.next_id;
        self.next_id += 1;
        log::info!("page {} opens", route.name());
        Entry { id, route, screen, memory: PageMemory::default(), missed: false, holds }
    }

    fn drop_entry(&mut self, entry: Entry, state: &mut AppState) {
        if entry.holds {
            state.hold_refresh(false);
        }
    }

    fn apply(&mut self, cmd: NavCommand, state: &mut AppState, ui: &mut UiState, now: f64) {
        match cmd {
            NavCommand::Push(route) => {
                if let Some(top) = self.stack.last_mut() {
                    top.memory = ui.export_page();
                }
                let entry = self.make(route, state, now);
                self.stack.push(entry);
                ui.import_page(PageMemory::default());
            }
            NavCommand::Replace(route) => {
                if let Some(old) = self.stack.pop() {
                    self.drop_entry(old, state);
                }
                let entry = self.make(route, state, now);
                self.stack.push(entry);
                ui.import_page(PageMemory::default());
            }
            NavCommand::Pop => {
                if self.stack.len() <= 1 {
                    return;
                }
                if let Some(old) = self.stack.pop() {
                    self.drop_entry(old, state);
                }
                let Some(top) = self.stack.last_mut() else { return };
                ui.import_page(std::mem::take(&mut top.memory));
                let missed = std::mem::take(&mut top.missed);
                let mut cx = AppCx::new(state, &mut self.requests, now);
                top.screen.event(&AppEvent::Shown, &mut cx);
                if missed {
                    top.screen.event(&AppEvent::Changed, &mut cx);
                }
            }
            NavCommand::Reset(route) => {
                // Nothing changes when it is already the only page.
                if self.stack.len() == 1 && self.stack[0].route == route {
                    return;
                }
                for old in std::mem::take(&mut self.stack) {
                    self.drop_entry(old, state);
                }
                let entry = self.make(route, state, now);
                self.stack.push(entry);
                ui.import_page(PageMemory::default());
                // Chosen in the rail: the rail keeps focus, on that entry, until the page can take it.
                if let Some(id) = self.rail.take_chosen().filter(|_| self.rail.handing()) {
                    ui.request_focus(Rail::entry_focus_id(&id));
                }
            }
            NavCommand::ToRail | NavCommand::Back => {
                // Asked outside a frame (an event): Back's next steps, the rail at the next frame.
                if matches!(cmd, NavCommand::Back) && self.stack.len() > 1 {
                    return self.apply(NavCommand::Pop, state, ui, now);
                }
                if self.rail_shown(state) {
                    let current = current_entry(self.stack.first().map(|e| &e.route), &self.entries);
                    self.rail.open_later(ui, current.as_deref().unwrap_or("home"));
                } else if matches!(cmd, NavCommand::Back) {
                    self.apply(NavCommand::Exit, state, ui, now);
                }
            }
            NavCommand::Exit if asks_before_leaving(state.device().info().os_major) => {
                self.exit_asked = true;
            }
            NavCommand::Exit => {
                log::info!("leaving the app");
                state.end_playback();
                state.device().exit();
                self.exited = true;
            }
            NavCommand::TileActions { id, continue_watching } => {
                let Some(page) = self.stack.last().map(|e| e.id) else { return };
                let mut cx = AppCx::new(state, &mut self.requests, now);
                match self.registry.make_tile_actions(id, continue_watching, &mut cx) {
                    Some(overlay) => {
                        let serial = self.next_id;
                        self.next_id += 1;
                        self.panel = Some(PagePanel { page, serial, overlay });
                    }
                    None => self.apply(NavCommand::Push(Route::ItemActions { id, continue_watching }), state, ui, now),
                }
            }
            NavCommand::Quit => {
                log::info!("the app ends");
                state.end_playback();
                self.exited = true;
                self.quit = true;
            }
        }
    }
}

/// Whether leaving asks "Exit Pley?" first: from webOS 6, as LG's Back-button guide says
/// (<https://webostv.developer.lge.com/develop/guides/back-button>: "show a popup asking whether
/// to exit the app"); up to webOS 5 it says to launch Home, which [`Device::exit`] does.
///
/// [`Device::exit`]: pley_core::Device::exit
fn asks_before_leaving(os_major: u32) -> bool {
    os_major >= 6
}
