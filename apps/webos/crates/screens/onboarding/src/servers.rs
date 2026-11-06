//! "Choose your server" (`TvServersScreen.kt`, `ServersViewModel.kt`; the TV app's
//! `onboarding/servers.tsx`) and its address entry (`TvAddressEntry.kt`).
//!
//! The list holds the servers this TV used, most recent first, then those that answer the UDP
//! broadcast sent as the screen opens; saved servers that didn't answer are asked directly and
//! marked "Can't reach it". "Search this network" asks every address of the TV's /24.
//! Choosing a server checks it, keeps it and opens its sign-in code, "Who's watching?" or Home.
//! Holding OK (or the menu key) on a saved server offers to forget it. Enter an address replaces
//! the list in place, as on Android, and Back brings the list back.

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use pley_base::{Icon, Key, Point, Rect, TextOpts, Waker};
use pley_core::addresses::{candidates, host_port, origin, unencrypted};
use pley_core::discovery::{self, ConnectProblem, FoundServer, SWEEP_TOTAL, SearchNews};
use pley_core::store::SavedServer;
use pley_core::{Cancel, Http, Promise};
use pley_kit::{Button, Dialog, DialogButton, Keyboard, KeyboardState, ResponseExt, TextEdit, TextField};
use pley_shell::{AppCx, AppEvent, Chrome, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Edge, FocusOpts, Group, Id, Ui, nudge};

use crate::globe::{Globe, Place};
use crate::kit::{Logo, brand_glow, status_line, sweep_bar};

/// A server row's height and the gap between rows.
const ROW_H: f32 = 64.0;
const ROW_GAP: f32 = 4.0;
/// A row's width.
const ROW_W: f32 = 520.0;
/// The logo's height, as on the sign-in code.
const LOGO_H: f32 = 40.0;
/// The title row's top and height.
const TITLE_Y: f32 = 27.0;
const TITLE_H: f32 = 38.0;
/// The list's greatest height.
const LIST_MAX: f32 = 300.0;
/// The list's top: 27 + the 38 dp title row + 25.
const LIST_Y: f32 = 90.0;
/// A focused row's top sits this far below the list's top once the list scrolls.
const ROW_LEAD: f32 = 90.0;
/// The address field's width.
const FIELD_W: f32 = 480.0;
/// The longest server name kept: names come from anything on the network that answers, and the
/// Forget dialog's title wraps without a limit.
const NAME_MAX: usize = 64;

/// "Hold OK on a saved server to forget it." and the other words of the screen
/// (`strings_tv_onboarding.xml`).
pub const TITLE: &str = "Choose your server";
pub const ON_NETWORK: &str = "On this network";
pub const UNREACHABLE: &str = "Can't reach it";
pub const LOOKALIKE: &str = "Not your saved server";
pub const UNENCRYPTED_TAG: &str = "Not encrypted";
pub const SEARCHING: &str = "Looking for Pley on your network…";
pub const NONE_FOUND: &str = "No servers found. Make sure this TV and the server are on the same network.";
pub const ENTER_ADDRESS: &str = "Enter an address";
pub const SEARCH_NETWORK: &str = "Search this network";
pub const SWEEPING_NETWORK: &str = "Checking this network…";
pub const FORGET_HINT: &str = "Hold OK on a saved server to forget it.";
pub const FORGET_BODY: &str = "Pley signs this TV out of it and deletes what it saved from it.";
pub const ADDRESS_TITLE: &str = "Enter your server's address";
pub const ADDRESS_PLACEHOLDER: &str = "192.168.1.20 or pley.example.com";
pub const ADDRESS_HINT: &str = "Port 7539 is used by default.";
pub const CONNECT: &str = "Connect";
pub const CHECKING: &str = "Checking…";

/// "Checking 192.168.1.1-254…" while the sweep asks the TV's /24.
pub fn sweeping_text(subnet: Option<&str>) -> String {
    match subnet {
        Some(s) => format!("Checking {s}1-254…"),
        None => SWEEPING_NETWORK.into(),
    }
}

/// A server's name as rows show it and the TV keeps it: one line of at most 64 characters, else
/// its address.
pub fn server_name(name: &str, url: &str) -> String {
    let line: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let short: String = line.trim().chars().take(NAME_MAX).collect();
    match short.trim_end() {
        "" => host_port(url),
        name => name.to_string(),
    }
}

/// What went wrong with a server, as the list and the address entry say it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Not an address, not Pley, or not answering.
    Connect(ConnectProblem),
    /// Plain http beyond the home network answered the address entry: said once, and Connect again goes
    /// ahead.
    Unencrypted { address: String },
}

impl Problem {
    /// The words shown in DangerText.
    pub fn text(&self) -> String {
        match self {
            Problem::Connect(p) => p.text(),
            Problem::Unencrypted { address } => format!(
                "{address} isn't encrypted: what you watch could be read on the way. Use its https address if it \
                 has one, or press Connect again to go ahead anyway."
            ),
        }
    }
}

/// A row of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerRow {
    /// Its origin.
    pub url: String,
    /// The name the server gave, else its address.
    pub name: String,
    /// The discovery id, when a broadcast gave one.
    pub id: Option<String>,
    /// On this TV's list.
    pub saved: bool,
    /// It answered the broadcast or the sweep (a server checked directly doesn't count).
    pub found: bool,
    /// Saved, not found, and it didn't answer when asked.
    pub unreachable: bool,
    /// Found, not saved, and named like a saved server it isn't.
    pub lookalike: bool,
}

impl ServerRow {
    /// Its second line and whether it is a warning: "Connecting to nas:7539…" while it is checked,
    /// "Can't reach it", else its address.
    pub fn line(&self, connecting: bool) -> (String, bool) {
        let address = host_port(&self.url);
        if connecting {
            (format!("Connecting to {address}…"), false)
        } else if self.unreachable {
            (UNREACHABLE.into(), true)
        } else {
            (address, false)
        }
    }

    /// The tag at its right and whether it is a warning (DangerText) rather than "On this network"
    /// with its green dot.
    pub fn tag(&self) -> Option<(&'static str, bool)> {
        if self.lookalike {
            Some((LOOKALIKE, true))
        } else if unencrypted(&self.url) {
            Some((UNENCRYPTED_TAG, true))
        } else if self.found {
            Some((ON_NETWORK, false))
        } else {
            None
        }
    }
}

/// The list (`Servers.merge`): the saved servers in the order kept (most recently used first),
/// then the servers found that aren't saved. A found server is a saved one when their discovery
/// ids or their addresses match; one that isn't but has a saved server's name is a lookalike.
pub fn merge(saved: &[SavedServer], found: &[FoundServer], unreachable: &HashSet<String>) -> Vec<ServerRow> {
    let same = |a_url: &str, a_id: Option<&str>, b_url: &str, b_id: Option<&str>| {
        (a_id.is_some() && a_id == b_id) || a_url == b_url
    };
    let mut rows: Vec<ServerRow> = saved
        .iter()
        .map(|s| {
            let found = found.iter().any(|f| same(&s.url, s.id.as_deref(), &f.url, f.id.as_deref()));
            ServerRow {
                url: s.url.clone(),
                name: server_name(&s.name, &s.url),
                id: s.id.clone(),
                saved: true,
                found,
                unreachable: !found && unreachable.contains(&s.url),
                lookalike: false,
            }
        })
        .collect();
    let saved_names: Vec<String> = rows.iter().map(|r| r.name.to_lowercase()).collect();
    for f in found {
        if rows.iter().any(|r| same(&r.url, r.id.as_deref(), &f.url, f.id.as_deref())) {
            continue;
        }
        let name = server_name(&f.name, &f.url);
        let lookalike = saved_names.contains(&name.to_lowercase());
        rows.push(ServerRow {
            url: f.url.clone(),
            name,
            id: f.id.clone(),
            saved: false,
            found: true,
            unreachable: false,
            lookalike,
        });
    }
    rows
}

/// The list's offset for the focused row at `index` (the TV app's rule): its top 90 below the
/// list's top, within the list's ends.
pub fn list_shift(index: usize, rows: usize) -> f32 {
    let content = content_height(rows);
    let max = (content - LIST_MAX).max(0.0);
    (index as f32 * (ROW_H + ROW_GAP) - ROW_LEAD).clamp(0.0, max)
}

fn content_height(rows: usize) -> f32 {
    if rows == 0 { 0.0 } else { rows as f32 * ROW_H + (rows - 1) as f32 * ROW_GAP }
}

/// A server being asked whether it is Pley.
type Check = Promise<Result<FoundServer, ConnectProblem>>;

/// A search running on its own thread: what it found comes in on `rx`.
struct Search {
    rx: Receiver<SearchNews>,
    cancel: Cancel,
}

impl Search {
    /// Adds what came in since the last frame to `news`; true once the search is over: it said
    /// so, or its thread is gone (it couldn't start, or it panicked).
    fn drain(&self, news: &mut Vec<FoundServer>) -> bool {
        loop {
            match self.rx.try_recv() {
                Ok(SearchNews::Found(server)) => news.push(server),
                Ok(SearchNews::Done) | Err(TryRecvError::Disconnected) => return true,
                Err(TryRecvError::Empty) => return false,
            }
        }
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// "Search this network" on its own thread (a sweep holds 48 connections for up to 6 s), each
/// server to `tx` as it answers, then [`SearchNews::Done`].
fn sweep_on_thread(http: Http, subnet: String, tx: Sender<SearchNews>, waker: Waker) -> Cancel {
    let cancel = Cancel::new();
    let c = cancel.clone();
    let spawned = std::thread::Builder::new().name("pley-sweep".into()).spawn(move || {
        let found = |server: FoundServer| {
            if !c.is_cancelled() && tx.send(SearchNews::Found(server)).is_ok() {
                waker.wake();
            }
        };
        discovery::sweep(&http, &subnet, &found, &c);
        if !c.is_cancelled() && tx.send(SearchNews::Done).is_ok() {
            waker.wake();
        }
    });
    if let Err(e) = spawned {
        log::warn!("the sweep didn't start: {e}");
    }
    cancel
}

/// "Enter your server's address" while it shows: the field, empty at first, and the TV's
/// keyboard, open from the start so a name or a whole https address types as easily as an IP.
struct Pad {
    edit: TextEdit,
    /// The on-screen keyboard is open under the field.
    typing: bool,
    keyboard: KeyboardState,
    checking: Option<Check>,
    problem: Option<Problem>,
}

impl Pad {
    fn new() -> Pad {
        Pad { edit: TextEdit::new(""), typing: true, keyboard: KeyboardState::default(), checking: None, problem: None }
    }

    fn edited(&mut self) {
        self.problem = None;
    }
}

/// "Choose your server", with its address entry.
pub struct ServersScreen {
    saved: Vec<SavedServer>,
    found: Vec<FoundServer>,
    unreachable: HashSet<String>,
    local: Option<Ipv4Addr>,
    subnet: Option<String>,
    search: Option<Search>,
    searching: bool,
    searched: bool,
    sweep: Option<Search>,
    sweep_since: Option<f64>,
    /// Saved servers the broadcast didn't find, asked directly.
    checks: Vec<(String, Check)>,
    /// The server being checked before it opens, and its discovery id.
    choosing: Option<(String, Option<String>, Check)>,
    problem: Option<Problem>,
    /// The unencrypted address the address entry already warned about: Connect again goes ahead.
    warned: Option<String>,
    /// The rows as they show, made again only when the servers saved, found or out of reach
    /// change.
    rows: Vec<ServerRow>,
    rows_stale: bool,
    /// The person pressed a key: focus is theirs from then on.
    touched: bool,
    /// What the screen last sent focus to, to tell when the person moved it.
    placed: Option<Id>,
    shift: f32,
    forgetting: Option<ServerRow>,
    dialog_open: bool,
    /// The row a closed dialog hands focus back to.
    refocus: Option<Id>,
    pad: Option<Pad>,
    logo: Logo,
    globe: Globe,
    opened: bool,
}

impl ServersScreen {
    /// The screen; `pad` opens it on the address entry.
    pub fn new(cx: &mut AppCx, pad: bool) -> ServersScreen {
        let local = cx.device().local_ipv4();
        let subnet = local.and_then(|ip| discovery::subnet_of(&ip.to_string()));
        let pad = pad.then(Pad::new);
        ServersScreen {
            saved: cx.state.saved_servers(),
            found: Vec::new(),
            unreachable: HashSet::new(),
            local,
            subnet,
            search: None,
            searching: false,
            searched: false,
            sweep: None,
            sweep_since: None,
            checks: Vec::new(),
            choosing: None,
            problem: None,
            warned: None,
            rows: Vec::new(),
            rows_stale: true,
            touched: false,
            placed: None,
            shift: 0.0,
            forgetting: None,
            dialog_open: false,
            refocus: None,
            pad,
            logo: Logo::default(),
            globe: Globe::default(),
            opened: false,
        }
    }

    /// The rows as last drawn.
    pub fn rows(&self) -> &[ServerRow] {
        &self.rows
    }

    /// Makes the rows again when what they come from changed.
    fn sync_rows(&mut self) {
        if std::mem::take(&mut self.rows_stale) {
            self.rows = merge(&self.saved, &self.found, &self.unreachable);
        }
    }

    fn mark_unreachable(&mut self, url: String) {
        self.rows_stale |= self.unreachable.insert(url);
    }

    /// Whether the address entry shows.
    pub fn pad_open(&self) -> bool {
        self.pad.is_some()
    }

    /// The broadcast, sent as the screen opens.
    fn open(&mut self, cx: &mut AppCx) {
        self.opened = true;
        let (tx, rx) = channel();
        let waker = cx.tasks().waker().clone();
        let cancel = discovery::search(cx.state.http().clone(), self.local, false, tx, waker);
        self.search = Some(Search { rx, cancel });
        self.searching = true;
    }

    fn add_found(&mut self, server: FoundServer) {
        let Some(url) = origin(&server.url) else { return };
        self.rows_stale |= self.unreachable.remove(&url);
        if !self.found.iter().any(|f| f.url == url) {
            let name = server_name(&server.name, &url);
            self.found.push(FoundServer { url, name, ..server });
            self.rows_stale = true;
        }
    }

    /// What came from the network and the server since the last frame.
    fn collect(&mut self, cx: &mut AppCx) {
        let mut news = Vec::new();
        let search_done = self.search.as_ref().is_some_and(|s| s.drain(&mut news));
        let sweep_done = self.sweep.as_ref().is_some_and(|s| s.drain(&mut news));
        for server in news {
            self.add_found(server);
        }
        if search_done {
            self.search = None;
            self.searching = false;
            self.searched = true;
            self.check_saved(cx);
        }
        if sweep_done {
            self.sweep = None;
            self.sweep_since = None;
            self.searched = true;
        }

        let mut i = 0;
        while i < self.checks.len() {
            match self.checks[i].1.take() {
                Some(answer) => {
                    let (url, _) = self.checks.swap_remove(i);
                    if answer.is_err() {
                        self.mark_unreachable(url);
                    }
                }
                None if self.checks[i].1.failed() => {
                    self.checks.swap_remove(i);
                }
                None => i += 1,
            }
        }

        if let Some((url, id, promise)) = self.choosing.as_mut()
            && let Some(answer) = promise.take()
        {
            let (url, id) = (url.clone(), id.clone());
            self.choosing = None;
            match answer {
                Ok(info) => go(cx, &url, &info.name, id.as_deref()),
                Err(problem) => {
                    if matches!(problem, ConnectProblem::Unreachable { .. }) {
                        self.mark_unreachable(url);
                    }
                    self.problem = Some(Problem::Connect(problem));
                }
            }
        }

        if let Some(pad) = self.pad.as_mut()
            && let Some(promise) = pad.checking.as_mut()
            && let Some(answer) = promise.take()
        {
            pad.checking = None;
            match answer {
                // Plain http beyond the home network is said once, about the address that
                // answered (a bare name falls back to http after https); Connect again goes ahead
                // (`ServersViewModel.connect`).
                Ok(info) if unencrypted(&info.url) && self.warned.as_deref() != Some(info.url.as_str()) => {
                    pad.problem = Some(Problem::Unencrypted { address: host_port(&info.url) });
                    self.warned = Some(info.url);
                }
                Ok(info) => go(cx, &info.url, &info.name, None),
                Err(problem) => pad.problem = Some(Problem::Connect(problem)),
            }
        }
    }

    /// The saved servers the broadcast didn't find are asked whether they answer.
    fn check_saved(&mut self, cx: &mut AppCx) {
        for s in &self.saved {
            if self.found.iter().any(|f| f.url == s.url || (s.id.is_some() && f.id == s.id)) {
                continue;
            }
            let (http, url) = (cx.state.http().clone(), s.url.clone());
            let promise = cx.tasks().spawn(move || discovery::check(&http, &url));
            self.checks.push((s.url.clone(), promise));
        }
    }

    /// OK on a row: the server is checked and, when it answers as Pley, opened. A row's "Not
    /// encrypted" tag is its only warning (`ServersViewModel.choose`).
    fn choose(&mut self, cx: &mut AppCx, row: &ServerRow) {
        if self.choosing.is_some() {
            return;
        }
        self.problem = None;
        let (http, url) = (cx.state.http().clone(), row.url.clone());
        let promise = cx.tasks().spawn(move || discovery::check(&http, &url));
        self.choosing = Some((row.url.clone(), row.id.clone(), promise));
    }

    /// "Search this network": every address of the TV's /24.
    fn start_sweep(&mut self, cx: &mut AppCx) {
        let Some(subnet) = self.subnet.clone() else { return };
        if self.sweep.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let cancel = sweep_on_thread(cx.state.http().clone(), subnet, tx, cx.tasks().waker().clone());
        self.sweep = Some(Search { rx, cancel });
        self.sweep_since = None;
    }

    /// Connect on the address entry: what was typed becomes the addresses to try (https first for a name on
    /// the internet); plain http beyond home is warned about once it answers.
    fn connect(&mut self, cx: &mut AppCx) {
        let Some(pad) = self.pad.as_mut() else { return };
        if pad.checking.is_some() {
            return;
        }
        let text = pad.edit.text.trim().to_string();
        if text.is_empty() {
            return;
        }
        pad.problem = None;
        if candidates(&text).is_empty() {
            pad.problem = Some(Problem::Connect(ConnectProblem::BadAddress));
            return;
        }
        let http = cx.state.http().clone();
        pad.checking = Some(cx.tasks().spawn(move || discovery::connect(&http, &text)));
    }

    fn forget(&mut self, cx: &mut AppCx, row: &ServerRow) {
        cx.state.forget_server(&row.url);
        self.saved.retain(|s| s.url != row.url);
        self.unreachable.remove(&row.url);
        self.rows_stale = true;
    }

    fn list_ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        brand_glow(ui);
        self.sync_rows();
        let rows = std::mem::take(&mut self.rows);
        let x = theme::GUTTER;
        let right = theme::CANVAS_W - theme::SAFE_X;
        let title_opts = TextOpts::line(right - x - 200.0);
        ui.text(Point::new(x, TITLE_Y + (TITLE_H - 34.0) / 2.0), TITLE, &theme::PAGE_TITLE, &title_opts, theme::FG);
        let logo_w = Logo::width(ui, LOGO_H);
        self.logo.show(ui, Point::new(right - logo_w, TITLE_Y + (TITLE_H - LOGO_H) / 2.0), LOGO_H);

        let list_h = content_height(rows.len()).min(LIST_MAX);
        let list_rect = Rect::new(x, LIST_Y, ROW_W + 16.0, list_h);
        let list_id = ui.id("list");
        let mut row_ids = Vec::with_capacity(rows.len());
        let mut chosen = None;
        let mut held = None;
        let mut focused_at = None;
        let shift = ui.animate(ui.id("shift"), self.shift, pley_kit::SCROLL);
        let connecting = self.choosing.as_ref().map(|(url, _, _)| url.clone());
        ui.clip(list_rect, |ui| {
            // Right stops in the list and the buttons: only Down or Up reach the globe.
            ui.group(list_id, Group::column().edge(Dir::Up, Edge::Stop).edge(Dir::Right, Edge::Stop), |ui| {
                for (i, row) in rows.iter().enumerate() {
                    let id = ui.id(("row", row.url.as_str()));
                    row_ids.push(id);
                    let top = LIST_Y + i as f32 * (ROW_H + ROW_GAP) - shift;
                    let rect = Rect::new(x, top, ROW_W, ROW_H);
                    let busy = connecting.as_deref() == Some(row.url.as_str());
                    let r = server_row(ui, id, rect, row, busy);
                    if r.focused {
                        focused_at = Some((i, r.gained));
                    }
                    if r.activated {
                        chosen = Some(row.clone());
                    }
                    if row.saved && r.actions() {
                        held = Some(row.clone());
                    }
                }
            });
        });
        if let Some((i, gained)) = focused_at
            && gained
        {
            self.shift = if ui.pointer_mode() {
                let top = i as f32 * (ROW_H + ROW_GAP);
                let max = (content_height(rows.len()) - LIST_MAX).max(0.0);
                nudge(self.shift, top, top + ROW_H, 0.0, list_h, 0.0, max)
            } else {
                list_shift(i, rows.len())
            };
        }

        let mut y = LIST_Y + list_h + if rows.is_empty() { 0.0 } else { 16.0 };
        let buttons_id = ui.id("buttons");
        let (enter_id, enter, search) = ui.group(buttons_id, Group::row().edge(Dir::Right, Edge::Stop), |ui| {
            let enter_id = ui.id("enter");
            let enter = Button::text(ENTER_ADDRESS).show(ui, enter_id, Point::new(x, y));
            let w = Button::text(ENTER_ADDRESS).size(ui).w;
            let sweeping = self.sweep.is_some();
            let search = Button::text(SEARCH_NETWORK).enabled(!sweeping && self.subnet.is_some()).show(
                ui,
                ui.id("search"),
                Point::new(x + w + 12.0, y),
            );
            (enter_id, enter, search)
        });
        y += theme::BUTTON_HEIGHT + 16.0;

        // The status under the buttons: the sweep, the search, none found, and what went wrong.
        let width = ROW_W;
        if self.sweep.is_some() {
            let since = *self.sweep_since.get_or_insert(ui.time());
            let text = sweeping_text(self.subnet.as_deref());
            y += status_line(ui, Point::new(x, y), width, &text, &theme::CAPTION, theme::MUTED, false) + 8.0;
            let progress = ((ui.time() - since) / SWEEP_TOTAL.as_secs_f64()) as f32;
            sweep_bar(ui, Point::new(x, y), progress);
            if progress < 1.0 {
                ui.request_repaint();
            }
            y += 4.0 + 8.0;
        } else if self.searching {
            y += status_line(ui, Point::new(x, y), width, SEARCHING, &theme::LABEL, theme::MUTED, true) + 8.0;
        } else if self.searched && rows.is_empty() && self.problem.is_none() {
            y += ui.text(Point::new(x, y), NONE_FOUND, &theme::BODY, &TextOpts::wrap(width, 0), theme::MUTED).h + 8.0;
        }
        if let Some(p) = &self.problem {
            ui.text(Point::new(x, y), &p.text(), &theme::BODY, &TextOpts::wrap(width, 0), theme::DANGER_TEXT);
        }
        if rows.iter().any(|r| r.saved) {
            let at = Point::new(x, theme::CANVAS_H - theme::SAFE_Y - theme::CAPTION.line_height);
            ui.text(at, FORGET_HINT, &theme::CAPTION, &TextOpts::line(width), theme::SUBTLE);
        }

        // Focus: the first row once there is one, else "Enter an address", until a key is pressed.
        let target = row_ids.first().copied().unwrap_or(enter_id);
        if !ui.keys().is_empty() || (self.placed.is_some() && ui.focused() != self.placed && ui.focused().is_some()) {
            self.touched = true;
        }
        ui.initial_focus(target);
        if !self.touched && self.placed != Some(target) && !self.dialog_open {
            ui.request_focus(target);
            self.placed = Some(target);
        }
        if let Some(id) = self.refocus.take()
            && row_ids.contains(&id)
        {
            ui.request_focus(id);
        }

        if let Some(row) = held {
            self.touched = true;
            self.forgetting = Some(row);
            self.dialog_open = true;
        }
        if let Some(row) = chosen {
            self.touched = true;
            self.choose(cx, &row);
        }
        if enter.activated {
            self.touched = true;
            self.problem = None;
            self.pad = Some(Pad::new());
            ui.request_repaint();
        }
        if search.activated {
            self.touched = true;
            self.start_sweep(cx);
            ui.request_repaint();
        }

        self.dialog_ui(ui, cx, &row_ids, &rows);
        self.rows = rows;
        self.globe.show(ui, cx, Place::BottomRight);
    }

    fn dialog_ui(&mut self, ui: &mut Ui, cx: &mut AppCx, row_ids: &[Id], rows: &[ServerRow]) {
        let Some(row) = self.forgetting.clone() else { return };
        let title = format!("Forget {}?", row.name);
        let buttons = [DialogButton::new("Cancel"), DialogButton::danger("Forget")];
        let dialog = Dialog::new(&title).body(FORGET_BODY).buttons(&buttons).initial(0);
        let r = dialog.show(ui, ui.id("forget"), self.dialog_open);
        if self.dialog_open && (r.dismissed || r.chosen.is_some()) {
            self.dialog_open = false;
            if r.chosen == Some(1) {
                self.forget(cx, &row);
            } else if let Some(at) = rows.iter().position(|x| x.url == row.url) {
                self.refocus = row_ids.get(at).copied();
            }
            ui.request_repaint();
        }
        if !self.dialog_open && r.progress <= 0.0 {
            self.forgetting = None;
        }
    }

    /// "Enter your server's address" (`TvAddressEntry.kt`; the TV app's `AddressEntry`): the
    /// title, the field and Connect under it, how addresses work and what went wrong, and the
    /// TV's keyboard below. With the keyboard closed, the remote's digits type, and OK on the
    /// field opens it again.
    fn pad_ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        brand_glow(ui);
        let Some(pad) = self.pad.as_mut() else { return };
        let centre = theme::CANVAS_W / 2.0;
        let opts = TextOpts::line(theme::CANVAS_W).aligned(pley_base::Align::Center);
        ui.text(Point::new(0.0, 27.0), ADDRESS_TITLE, &theme::TITLE, &opts, theme::FG);
        let field_y = 27.0 + theme::TITLE.line_height + 8.0;
        let connect_y = field_y + pley_kit::keyboard::FIELD_HEIGHT + 14.0;
        let info_y = connect_y + theme::BUTTON_HEIGHT + 16.0;

        let mut connect = false;
        let mut start_typing = false;
        let typing = pad.typing;
        let column = ui.id("pad");
        let mut draw = |ui: &mut Ui, pad: &mut Pad| {
            ui.group(column, Group::column().edge(Dir::Right, Edge::Stop), |ui| {
                let field = TextField::new(&pad.edit.text, FIELD_W).placeholder(ADDRESS_PLACEHOLDER).typing(typing);
                let f = field.show(ui, ui.id("field"), Point::new(centre - FIELD_W / 2.0, field_y));
                start_typing |= f.activated;
                // With the keyboard closed: what a keyboard types, and the remote's digits.
                if !typing {
                    let mut typed: String = ui.typed().chars().filter(|c| !c.is_control()).collect();
                    for k in ui.keys() {
                        if let Key::Digit(d) = k.key
                            && k.pressed
                        {
                            typed.push(char::from(b'0' + d));
                        }
                    }
                    if !typed.is_empty() && pad.edit.insert(&typed) {
                        pad.edited();
                    }
                }
                let label = if pad.checking.is_some() { CHECKING } else { CONNECT };
                let pill = Button::pill(label);
                let at = Point::new(centre - pill.size(ui).w / 2.0, connect_y);
                connect |= pill.show(ui, ui.id("connect"), at).activated;
            });
        };
        if typing {
            pley_kit::inert(ui, |ui| draw(ui, pad));
        } else {
            draw(ui, pad);
        }
        let wrap = TextOpts::wrap(FIELD_W, 0).aligned(pley_base::Align::Center);
        let info = Point::new(centre - FIELD_W / 2.0, info_y);
        let h = ui.text(info, ADDRESS_HINT, &theme::CAPTION, &wrap, theme::MUTED).h;
        if let Some(p) = &pad.problem {
            ui.text(Point::new(info.x, info.y + h + 8.0), &p.text(), &theme::BODY, &wrap, theme::DANGER_TEXT);
        }
        if typing {
            let kb =
                Keyboard::new().action_label(CONNECT).show(ui, ui.id("keyboard"), &mut pad.keyboard, &mut pad.edit);
            if kb.changed {
                pad.edited();
            }
            if kb.done {
                pad.typing = false;
                connect = true;
            } else if kb.closed {
                pad.typing = false;
            }
        }
        if start_typing {
            pad.typing = true;
            pad.keyboard = KeyboardState::default();
            ui.request_repaint();
        }
        if connect {
            self.connect(cx);
            ui.request_repaint();
        }
        if !typing {
            self.globe.show(ui, cx, Place::BottomRight);
        }
    }
}

/// Opens a server that answered as Pley: kept first on the list, used from now on, then its code
/// screen when the TV holds no sign-in for it, "Who's watching?" when its household had more
/// than one profile, else Home (`TvServersScreen.kt`).
fn go(cx: &mut AppCx, url: &str, name: &str, id: Option<&str>) {
    let Some(home) = origin(url) else { return };
    cx.state.remember_server(&home, &server_name(name, &home), id);
    cx.state.use_server(&home);
    if !cx.state.has_tokens(&home) {
        cx.reset(Route::Link { server: home });
        return;
    }
    // A server still signed in needs no code. As at a start, who is watching, the libraries and
    // Home are asked for and its events followed: the shell asks only once, at its first frame,
    // and Home shows what is saved until then (`SessionManager.kt`).
    cx.state.revalidate();
    let visible = cx.state.visible();
    cx.state.listen(visible);
    cx.reset(if cx.state.household_size() > 1 { Route::Profiles } else { Route::Home });
}

/// One server (`TvServerRow`): 520 × 64, padding 12; a 40 dp icon box with the monitor, the name
/// over its second line, and the tag at the right. Focused, an Accent fill with OnAccent content
/// and no scale (rows in lists never scale).
fn server_row(ui: &mut Ui, id: Id, rect: Rect, row: &ServerRow, connecting: bool) -> pley_ui::Response {
    let opts = if row.saved { FocusOpts::default().long_press() } else { FocusOpts::default() };
    let r = ui.focusable_with(id, rect, opts);
    if r.activated || r.long_pressed || r.menu {
        ui.request_repaint();
    }
    let focused = r.focused;
    if focused {
        ui.fill(rect, theme::ACCENT, radius::CARD);
    }
    let ink = if focused { theme::ON_ACCENT } else { theme::FG };
    let soft = if focused { theme::ON_ACCENT } else { theme::MUTED };
    let icon_box = Rect::new(rect.x + 12.0, rect.y + 12.0, 40.0, 40.0);
    let well = if focused { theme::ON_ACCENT.alpha(theme::ICON_WELL_ALPHA) } else { theme::SURFACE3 };
    ui.fill(icon_box, well, radius::FIELD);
    ui.icon(icon_box.place(pley_base::Size::new(22.0, 22.0), 0.5, 0.5), Icon::Monitor, ink);

    let mut text_right = rect.right() - 12.0;
    if let Some((tag, warning)) = row.tag() {
        let w = ui.measure(tag, &theme::CAPTION, &crate::kit::free_line()).w;
        let tag_x = text_right - w;
        let y = rect.y + (rect.h - theme::CAPTION.line_height) / 2.0;
        let color = if warning && !focused { theme::DANGER_TEXT } else { soft };
        ui.text(Point::new(tag_x, y), tag, &theme::CAPTION, &crate::kit::free_line(), color);
        text_right = tag_x - 8.0;
        if !warning {
            let dot = Rect::new(tag_x - 16.0, rect.y + (rect.h - 8.0) / 2.0, 8.0, 8.0);
            ui.fill(dot, theme::SUCCESS, 4.0);
            text_right = dot.x - 8.0;
        }
    }
    let text_x = icon_box.right() + 16.0;
    let width = (text_right - text_x).max(1.0);
    let top = rect.y + (rect.h - theme::TITLE.line_height - theme::CAPTION.line_height) / 2.0;
    ui.text(Point::new(text_x, top), &row.name, &theme::TITLE, &TextOpts::line(width), ink);
    let (line, danger) = row.line(connecting);
    let color = if danger && !focused { theme::DANGER_TEXT } else { soft };
    ui.text(Point::new(text_x, top + theme::TITLE.line_height), &line, &theme::CAPTION, &TextOpts::line(width), color);
    r
}

impl Screen for ServersScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        if !self.opened {
            self.open(cx);
        }
        self.collect(cx);
        // An answer on its way needs no frames: the task pool wakes the loop when it comes.
        if self.pad.is_some() {
            self.pad_ui(ui, cx);
        } else {
            self.list_ui(ui, cx);
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if self.globe.open() {
            self.globe.close();
            return true;
        }
        if let Some(pad) = self.pad.as_mut() {
            if pad.typing {
                pad.typing = false;
            } else {
                self.pad = None;
                self.touched = false;
                self.placed = None;
            }
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        if let AppEvent::Device(pley_core::DeviceEvent::Network { ipv4: Some(ip), .. }) = event {
            self.local = Some(*ip);
            self.subnet = discovery::subnet_of(&ip.to_string());
        }
        let _ = cx;
    }

    fn chrome(&self) -> Chrome {
        Chrome::Bare
    }

    fn modal(&self) -> bool {
        self.dialog_open || self.globe.open() || self.pad.as_ref().is_some_and(|p| p.typing)
    }

    /// Every word is drawn as it goes: the servers found and an address being typed stay.
    fn relabel(&mut self, _cx: &mut AppCx) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(url: &str, name: &str, id: Option<&str>) -> SavedServer {
        SavedServer { url: url.into(), name: name.into(), id: id.map(Into::into), last_used: 0 }
    }

    fn found(url: &str, name: &str, id: Option<&str>) -> FoundServer {
        FoundServer { url: url.into(), name: name.into(), version: "1".into(), id: id.map(Into::into) }
    }

    #[test]
    fn saved_servers_come_first_then_the_ones_found() {
        let s = [saved("http://nas:7539", "Den", Some("a")), saved("https://pley.example.com", "Cloud", None)];
        let f = [
            found("http://192.168.1.20:7539", "Den", Some("a")),
            found("http://192.168.1.30:7539", "Attic", Some("b")),
            found("http://192.168.1.40:7539", "cloud ", None),
        ];
        let rows = merge(&s, &f, &HashSet::from(["https://pley.example.com".to_string()]));
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Den", "Cloud", "Attic", "cloud"]);
        // The saved Den answered by its discovery id at a new address.
        assert!(rows[0].saved && rows[0].found && !rows[0].unreachable);
        assert!(rows[1].saved && !rows[1].found && rows[1].unreachable);
        assert!(!rows[2].saved && rows[2].found && !rows[2].lookalike);
        // Named like a saved server it isn't.
        assert!(rows[3].lookalike);
    }

    #[test]
    fn rows_say_where_they_are_and_what_to_mind() {
        let row = ServerRow {
            url: "http://nas:7539".into(),
            name: "Den".into(),
            id: None,
            saved: true,
            found: true,
            unreachable: false,
            lookalike: false,
        };
        assert_eq!(row.line(false), ("nas:7539".into(), false));
        assert_eq!(row.line(true), ("Connecting to nas:7539…".into(), false));
        assert_eq!(row.tag(), Some((ON_NETWORK, false)));
        let away = ServerRow { found: false, unreachable: true, ..row.clone() };
        assert_eq!(away.line(false), (UNREACHABLE.into(), true));
        assert_eq!(away.tag(), None);
        let open = ServerRow { url: "http://pley.example.com".into(), ..row.clone() };
        assert_eq!(open.tag(), Some((UNENCRYPTED_TAG, true)));
        let lookalike = ServerRow { lookalike: true, ..row };
        assert_eq!(lookalike.tag(), Some((LOOKALIKE, true)));
    }

    #[test]
    fn names_from_the_network_are_one_short_line() {
        assert_eq!(server_name("  Den\n", "http://nas:7539"), "Den");
        assert_eq!(server_name("Living\r\nroom", "http://nas:7539"), "Living  room");
        assert_eq!(server_name(" \t", "http://192.168.1.20:7539"), "192.168.1.20:7539");
        let long = "x".repeat(2048);
        assert_eq!(server_name(&long, "http://nas:7539").chars().count(), 64);
    }

    #[test]
    fn the_list_scrolls_the_focused_row_to_90_below_its_top() {
        assert_eq!(list_shift(0, 8), 0.0);
        assert_eq!(list_shift(1, 8), 0.0);
        assert_eq!(list_shift(3, 8), 3.0 * 68.0 - 90.0);
        // Never past the end: 8 rows are 540 tall, the list 300.
        assert_eq!(list_shift(7, 8), 540.0 - 300.0);
        assert_eq!(list_shift(2, 3), 0.0);
    }

    #[test]
    fn problems_are_said_as_android_says_them() {
        assert_eq!(
            Problem::Unencrypted { address: "pley.example.com".into() }.text(),
            "pley.example.com isn't encrypted: what you watch could be read on the way. Use its https address if \
             it has one, or press Connect again to go ahead anyway."
        );
        assert_eq!(
            Problem::Connect(ConnectProblem::BadAddress).text(),
            "Enter an address such as nas, 192.168.1.20 or pley.example.com."
        );
        assert_eq!(sweeping_text(Some("192.168.1.")), "Checking 192.168.1.1-254…");
        assert_eq!(sweeping_text(None), "Checking this network…");
    }

    use crate::testing::{Rig, fixtures, now_ms};
    use pley_base::Key;

    fn row_id(url: &str) -> Id {
        Id::ROOT.with("list").with(("row", url))
    }

    fn enter_id() -> Id {
        Id::ROOT.with("buttons").with("enter")
    }

    /// The screen on a TV that saved `saved`, without the broadcast (tests say what was found).
    fn screen(rig: &mut Rig, saved: &[(&str, &str)]) -> ServersScreen {
        // Remembering a server puts it first: the list's first is remembered last.
        let store = rig.cx(|cx| cx.store().clone());
        for (url, name) in saved.iter().rev() {
            store.remember_server(url, name, None, now_ms()).unwrap();
        }
        let mut s = rig.cx(|cx| ServersScreen::new(cx, false));
        s.opened = true;
        s
    }

    #[test]
    fn the_first_row_takes_focus_until_a_key_is_pressed() {
        let mut rig = Rig::new();
        let mut s = screen(&mut rig, &[]);
        rig.frame(&mut s, Vec::new());
        // Nothing listed: "Enter an address".
        assert_eq!(rig.focused(), Some(enter_id()));
        s.add_found(found("http://192.168.1.20:7539", "Den", Some("a")));
        rig.frame(&mut s, Vec::new());
        assert_eq!(rig.focused(), Some(row_id("http://192.168.1.20:7539")));
        // Once the person moved focus, servers coming in leave it where it is.
        rig.press(&mut s, Key::Down);
        assert_eq!(rig.focused(), Some(enter_id()));
        s.add_found(found("http://192.168.1.30:7539", "Attic", Some("b")));
        rig.advance(&mut s, 0.1);
        assert_eq!(rig.focused(), Some(enter_id()));
        assert!(rig.says("Attic") && rig.says(ON_NETWORK));
    }

    #[test]
    fn down_reaches_the_globe_and_a_language_picked_shows_at_once() {
        use crate::globe::Globe;
        use pley_i18n::Language;
        let mut rig = Rig::new();
        let mut s = screen(&mut rig, &[]);
        rig.frame(&mut s, Vec::new());
        assert!(rig.says("EN"));
        // Right never reaches it; Down does, and Left and Right don't leave it.
        rig.press(&mut s, Key::Right);
        rig.press(&mut s, Key::Right);
        assert_eq!(rig.focused(), Some(Id::ROOT.with("buttons").with("search")));
        rig.press(&mut s, Key::Down);
        assert_eq!(rig.focused(), Some(Globe::id()));
        rig.press(&mut s, Key::Left);
        rig.press(&mut s, Key::Right);
        assert_eq!(rig.focused(), Some(Globe::id()));
        // OK lists Automatic, with what that is here, then each language in itself.
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.5);
        assert!(s.modal());
        for words in ["Choose a language", "Automatic (English)", "English", "Français", "Español"] {
            assert!(rig.says(words), "{words}");
        }
        // Français shows at once and is kept; the globe keeps focus.
        rig.press(&mut s, Key::Down);
        rig.press(&mut s, Key::Down);
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.5);
        assert_eq!(rig.state.language(), Language::Fr);
        assert_eq!(rig.state.language_choice(), Some(Language::Fr));
        assert!(!s.modal() && rig.says("FR"));
        assert_eq!(rig.focused(), Some(Globe::id()));
        // Its Automatic is the TV's English; picking it forgets the choice.
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.5);
        assert!(rig.says("Automatique (English)"));
        rig.press(&mut s, Key::Up);
        rig.press(&mut s, Key::Up);
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.5);
        assert_eq!(rig.state.language(), Language::En);
        assert_eq!(rig.state.language_choice(), None);
        // Back closes the panel before anything else.
        rig.press(&mut s, Key::Ok);
        assert!(s.modal());
        assert!(rig.cx(|cx| s.back(cx)));
        rig.advance(&mut s, 0.5);
        assert!(!s.modal());
    }

    #[test]
    fn the_status_says_what_the_search_is_doing() {
        let mut rig = Rig::new();
        let mut s = screen(&mut rig, &[]);
        s.searching = true;
        rig.frame(&mut s, Vec::new());
        assert!(rig.says(SEARCHING));
        s.searching = false;
        s.searched = true;
        rig.frame(&mut s, Vec::new());
        assert!(rig.says(NONE_FOUND) && !rig.says(SEARCHING));
        assert!(!rig.says(FORGET_HINT));

        // The sweep: its range, its bar, and "Search this network" off until it ends.
        let (tx, rx) = channel();
        s.sweep = Some(Search { rx, cancel: Cancel::new() });
        rig.frame(&mut s, Vec::new());
        assert!(rig.says("Checking 192.168.1.1-254…") && !rig.says(NONE_FOUND));
        rig.press(&mut s, Key::Right);
        assert_eq!(rig.focused(), Some(enter_id()), "the search can't be chosen while it runs");
        tx.send(SearchNews::Found(found("http://192.168.1.40:7539", "Loft", None))).unwrap();
        tx.send(SearchNews::Done).unwrap();
        rig.frame(&mut s, Vec::new());
        assert!(s.sweep.is_none() && rig.says("Loft") && !rig.says("Checking"));
        rig.press(&mut s, Key::Right);
        assert_eq!(rig.focused(), Some(Id::ROOT.with("buttons").with("search")));

        // A search whose thread is gone ends as if it had said so.
        let (tx, rx) = channel::<SearchNews>();
        drop(tx);
        s.search = Some(Search { rx, cancel: Cancel::new() });
        s.searching = true;
        rig.frame(&mut s, Vec::new());
        assert!(s.search.is_none() && !s.searching && !rig.says(SEARCHING));
    }

    #[test]
    fn menu_on_a_saved_server_offers_to_forget_it() {
        let mut rig = Rig::new();
        let mut s = screen(&mut rig, &[("http://192.168.1.20:7539", "Den"), ("http://192.168.1.21:7539", "Attic")]);
        rig.frame(&mut s, Vec::new());
        assert!(rig.says(FORGET_HINT));
        rig.press(&mut s, Key::Menu);
        assert!(s.modal() && rig.says("Forget Den?") && rig.says(FORGET_BODY));
        // Cancel has focus and gives it back to the row.
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.5);
        assert!(!s.modal());
        assert_eq!(rig.focused(), Some(row_id("http://192.168.1.20:7539")));
        assert_eq!(s.rows().len(), 2);

        // Held OK opens it too; Forget takes the server off the list and off the TV.
        rig.hold_ok(&mut s, 0.8);
        assert!(s.modal());
        rig.press(&mut s, Key::Right);
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.5);
        let names: Vec<&str> = s.rows().iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Attic"]);
        assert!(rig.cx(|cx| cx.state.saved_servers()).iter().all(|x| x.name != "Den"));
    }

    #[test]
    fn entering_an_address_opens_the_keyboard_and_back_closes_it() {
        let mut rig = Rig::new();
        let mut s = screen(&mut rig, &[]);
        rig.frame(&mut s, Vec::new());
        rig.press(&mut s, Key::Ok);
        assert!(s.pad_open() && rig.says(ADDRESS_TITLE) && rig.says(ADDRESS_HINT));
        // Empty, with the TV's keyboard open at once.
        assert_eq!(s.pad.as_ref().unwrap().edit.text, "");
        assert!(s.pad.as_ref().unwrap().typing && s.modal());
        // The keyboard takes the remote's digits; Back closes it, and the digits still type.
        rig.press(&mut s, Key::Digit(1));
        assert_eq!(s.pad.as_ref().unwrap().edit.text, "1");
        assert!(rig.cx(|cx| s.back(cx)));
        rig.frame(&mut s, Vec::new());
        assert!(!s.pad.as_ref().unwrap().typing);
        rig.press(&mut s, Key::Digit(0));
        assert_eq!(s.pad.as_ref().unwrap().edit.text, "10");
        // OK on the field opens the keyboard again.
        assert_eq!(rig.focused(), Some(Id::ROOT.with("pad").with("field")));
        rig.press(&mut s, Key::Ok);
        assert!(s.pad.as_ref().unwrap().typing);
        // Back closes the keyboard, then the screen.
        assert!(rig.cx(|cx| s.back(cx)));
        assert!(rig.cx(|cx| s.back(cx)));
        rig.frame(&mut s, Vec::new());
        assert!(!s.pad_open() && rig.says(TITLE));
    }

    #[test]
    fn an_address_that_isnt_one_is_said_so() {
        let mut rig = Rig::new();
        let mut s = rig.cx(|cx| ServersScreen::new(cx, true));
        s.opened = true;
        s.pad.as_mut().unwrap().edit = TextEdit::new("nas:port");
        rig.frame(&mut s, Vec::new());
        rig.cx(|cx| s.connect(cx));
        rig.frame(&mut s, Vec::new());
        assert!(rig.says("Enter an address such as nas, 192.168.1.20 or pley.example.com."));
    }

    #[test]
    fn connecting_to_a_pley_server_opens_its_sign_in() {
        let server = fixtures();
        let mut rig = Rig::new();
        let mut s = rig.cx(|cx| ServersScreen::new(cx, true));
        s.opened = true;
        let typed = server.url().trim_start_matches("http://").to_string();
        s.pad.as_mut().unwrap().edit = TextEdit::new(&typed);
        rig.frame(&mut s, Vec::new());
        rig.cx(|cx| s.connect(cx));
        rig.frame(&mut s, Vec::new());
        assert!(rig.says(CHECKING));
        let link = Route::Link { server: server.url().to_string() };
        assert!(rig.until(&mut s, |rig| rig.reset_to(&link)), "the sign-in never opened");
        // Kept first on the list, and the server in use.
        let kept = rig.cx(|cx| cx.state.saved_servers());
        assert_eq!(kept.first().map(|x| x.url.as_str()), Some(server.url()));
        assert_eq!(rig.cx(|cx| cx.state.server().map(str::to_string)).as_deref(), Some(server.url()));
    }

    #[test]
    fn choosing_a_signed_in_server_loads_it_and_a_dead_one_says_so() {
        let server = fixtures();
        let mut rig = Rig::signed_in(&server, pley_fixtures::ids::ALEX);
        // The TV moved to another server since ("Switch server" in Settings).
        rig.cx(|cx| cx.state.use_server("http://127.0.0.1:9"));
        let mut s = screen(&mut rig, &[("http://127.0.0.1:9", "Gone")]);
        rig.frame(&mut s, Vec::new());
        // The dead one is first (used last); its check fails and marks it. (Linux refuses the
        // connection before the next frame, so "Connecting to…" may never show: `row.line` has it.)
        rig.press(&mut s, Key::Ok);
        assert!(rig.until(&mut s, |rig| rig.says(UNREACHABLE)), "the dead server was never marked");
        assert!(rig.says("Couldn't reach 127.0.0.1:9"));
        rig.press(&mut s, Key::Down);
        rig.press(&mut s, Key::Ok);
        assert!(rig.until(&mut s, |rig| rig.reset_to(&Route::Home)), "Home never opened");
        // Still signed in there: its events are followed, and who is watching and Home are asked.
        assert_eq!(rig.state.server(), Some(server.url()));
        assert!(rig.state.listening(), "the events never started");
        assert!(rig.until(&mut s, |rig| rig.state.home_fresh() && rig.state.me().is_some()), "nothing was asked");
    }

    #[test]
    fn plain_http_beyond_home_is_warned_about_once_it_answers_the_pad() {
        let mut rig = Rig::new();
        // A row says so with its tag alone, as on Android: OK checks it at once.
        let mut s = screen(&mut rig, &[("http://203.0.113.5:7539", "Away")]);
        rig.frame(&mut s, Vec::new());
        assert!(rig.says(UNENCRYPTED_TAG));
        rig.press(&mut s, Key::Ok);
        assert!(s.choosing.is_some() && !rig.says("isn't encrypted"));

        // On the address entry, the address that answered is warned about once; Connect again goes ahead.
        let mut s = rig.cx(|cx| ServersScreen::new(cx, true));
        s.opened = true;
        let away = found("http://203.0.113.5:7539", "Away", None);
        let link = Route::Link { server: away.url.clone() };
        s.pad.as_mut().unwrap().checking = Some(Promise::resolved(Ok(away.clone())));
        rig.frame(&mut s, Vec::new());
        assert!(rig.says("203.0.113.5:7539 isn't encrypted") && !rig.reset_to(&link));
        s.pad.as_mut().unwrap().checking = Some(Promise::resolved(Ok(away)));
        rig.frame(&mut s, Vec::new());
        assert!(rig.reset_to(&link));
    }
}
