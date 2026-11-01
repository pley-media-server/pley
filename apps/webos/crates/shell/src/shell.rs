//! The shell (the TV app's `ui/shell/app.tsx`, Android's `TvApp.kt` and `TvNavigator.kt`): one
//! page at a time from the back stack, the rail beside the pages that have it, the curtain
//! between pages, toasts, the offline notice, the idle dimmer, and Back in the Android app's order:
//! what the page has open, then the rail leaves the app, a page above the first pops, a first
//! page opens the rail, else the app is left.

use pley_base::{ImageRequest, Key};
use pley_ui::{Dir, Edge, Group, Id, InputEvent, PageMemory, Ui, UiState};

use crate::cx::{AppCx, NavCommand, Requests};
use crate::overlay::{self, DIM_AFTER, TOAST_TOP, TOAST_TOP_PLAYER, Toasts};
use crate::rail::{Rail, RailEntries, RailView, current_entry, rail_entries};
use crate::route::Route;
use crate::screen::{AppEvent, Chrome, Registry, Screen};
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

/// The shell: the back stack and everything drawn around the page on top.
pub struct Shell {
    registry: Registry,
    stack: Vec<Entry>,
    next_id: u64,
    rail: Rail,
    toasts: Toasts,
    requests: Requests,
    /// The page on top when the last frame drew, for the curtain and [`AppEvent::Shown`].
    shown: Option<u64>,
    curtain: Option<u64>,
    last_input: f64,
    dimmed: bool,
    music_due: Option<f64>,
    exited: bool,
    entries: RailEntries,
    /// The server was asked for who is watching, the libraries and Home, after the first frame.
    asked: bool,
}

impl Shell {
    /// A shell drawing pages with `registry`'s screens. [`Shell::start`] opens the first page.
    pub fn new(registry: Registry) -> Shell {
        Shell {
            registry,
            stack: Vec::new(),
            next_id: 1,
            rail: Rail::default(),
            toasts: Toasts::default(),
            requests: Requests::default(),
            shown: None,
            curtain: None,
            last_input: pley_base::time::now(),
            dimmed: false,
            music_due: None,
            exited: false,
            entries: rail_entries(&[], false, false),
            asked: false,
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

    /// The toast showing.
    pub fn toast(&self) -> Option<&str> {
        self.toasts.text()
    }

    /// Whether the idle dimmer is down.
    pub fn dimmed(&self) -> bool {
        self.dimmed
    }

    /// Whether the app asked to be left (Back in the rail, or on a first page without it).
    pub fn exited(&self) -> bool {
        self.exited
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
        [self.toasts.next_due(), dim, self.music_due].into_iter().flatten().reduce(f64::min)
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
    /// the music, the dimmer.
    pub fn update(&mut self, state: &mut AppState, ui: &mut UiState, now: f64) {
        for e in state.update(now) {
            self.deliver(e, state, ui, now);
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
    /// never finds the screen dimmed.
    pub fn visible(&mut self, visible: bool, state: &mut AppState, ui: &mut UiState, now: f64) {
        state.set_visible(visible);
        if visible {
            self.last_input = now;
            self.dimmed = false;
        }
        self.deliver(AppEvent::Visible(visible), state, ui, now);
    }

    /// Tells the page on top `event` (the pages under it hear [`AppEvent::Changed`] when on top
    /// again); a sign-in the server ended goes back to its code screen.
    pub fn deliver(&mut self, event: AppEvent, state: &mut AppState, ui: &mut UiState, now: f64) {
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
            let libraries = state.libraries().unwrap_or(&[]);
            let queued = state.music.as_ref().is_some_and(|m| m.queued());
            self.entries = rail_entries(libraries, queued, state.live_available());
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
        }

        // Back, before the page draws: in the rail it leaves the app; else the page's own.
        if ui.key_down(Key::Back) {
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

        let entry = &mut self.stack[top];
        let group = Group::free().edge(Dir::Left, Edge::Out);
        let modal = {
            let mut cx = AppCx::new(state, &mut self.requests, now);
            ui.group(entry.group_id(), group, |ui| entry.screen.ui(ui, &mut cx));
            entry.screen.modal()
        };
        if let Some(c) = self.curtain
            && !overlay::curtain(ui, c)
        {
            self.curtain = None;
        }

        for t in std::mem::take(&mut self.requests.toasts) {
            self.toasts.show(t, now);
        }
        // Back the page didn't take, and what the page asked: the rail, or Back's next steps.
        let mut to_rail = ui.failed_move() == Some(Dir::Left) && !modal && !self.rail.is_open();
        if ui.take_key(Key::Back) {
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
                page_modal: modal,
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

        let chrome = self.chrome();
        let top_y = if chrome == Chrome::Video { TOAST_TOP_PLAYER } else { TOAST_TOP };
        let below = overlay::offline_notice(ui, rail_shown && state.offline_notice(), top_y);
        self.toasts.ui(ui, below);
        overlay::dimmer(ui, self.dimmed);
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
            NavCommand::Exit => {
                log::info!("leaving the app");
                state.end_playback();
                state.device().exit();
                self.exited = true;
            }
        }
    }
}
