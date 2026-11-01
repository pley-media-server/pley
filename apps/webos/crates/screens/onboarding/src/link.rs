//! Signing in with a code (`TvLinkScreen.kt`, `LinkViewModel.kt`; the TV app's
//! `onboarding/link.tsx`): the steps and the code on the left, a QR code of the approval page on
//! the right, the status line at the bottom. The same layout approves a profile that needs its
//! password, or "Sign in as another profile", from "Who's watching?".
//!
//! The code comes from pley-core's [`DeviceSignIn`] on a thread of its own, which polls the
//! server at its interval, backs off while it can't be reached and renews an expired code
//! silently up to three times. Polling stops while the app is hidden and starts again when it
//! shows, with the same code while it works. Once approved, the tokens are kept, the name is
//! awaited for at most 3 s, "Signed in as Anna" shows for half a second, then Home.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use pley_base::{Icon, Point, Rect, Size, TextOpts};
use pley_core::auth::{AuthError, DeviceCode, DeviceSignIn, LinkState};
use pley_core::model::Me;
use pley_core::{ApiError, Cancel, Device, Promise};
use pley_kit::{Button, Qr};
use pley_shell::{AppCx, AppEvent, Chrome, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Group, Ui};

use crate::kit::{Logo, brand_glow, free_line, space, status_line};

/// How long "Signed in as …" shows before Home (`SIGNED_IN_MS`).
pub const SIGNED_IN_FOR: f64 = 0.5;
/// How long the name may take to come (`NAME_WAIT_MS`).
pub const NAME_WAIT: f64 = 3.0;
/// Between two asks for the name.
const NAME_ASK_EVERY: f64 = 0.4;

/// The screen's words (`strings_tv_onboarding.xml`).
pub const TITLE: &str = "Sign in to Pley";
pub const SCAN_STEP: &str = "Scan the code with your phone";
pub const GO_TO: &str = "or go to";
pub const ENTER_STEP: &str = "and enter this code";
pub const SAME_WIFI: &str = "Your phone must be on the same Wi-Fi as this TV.";
pub const OTHER_DEVICE: &str = "Or open Pley on a signed-in device → Link a TV";
pub const SCAN: &str = "Scan with your phone camera";
pub const CHANGE_SERVER: &str = "Change server";
pub const WAITING: &str = "Waiting for approval…";
pub const GETTING_CODE: &str = "Getting a code…";
pub const DECLINED: &str = "Request declined";
pub const EXPIRED: &str = "Code expired: press OK for a new one";
pub const NEW_CODE: &str = "Get a new code";
pub const USE_ANOTHER: &str = "Use another profile";
pub const APPROVE_TITLE: &str = "Approve on your phone";
pub const ANOTHER_TITLE: &str = "Sign in as another profile";
pub const ANOTHER_BODY: &str = "Approve this code on a phone signed in to Pley, and pick the profile there.";

/// "Anna's profile needs its password: …", for a profile approved on a phone.
pub fn approve_body(name: &str) -> String {
    format!(
        "{name}'s profile needs its password: approve this code on a phone signed in to Pley, and pick {name} \
         there."
    )
}

/// "Can't reach the server. Trying again in 4 s".
pub fn retrying(seconds: u64) -> String {
    format!("Can't reach the server. Trying again in {seconds} s")
}

/// "Signed in as Anna", or "Signed in" without a name.
pub fn signed_in_text(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("Signed in as {n}"),
        None => "Signed in".into(),
    }
}

/// The code's halves: `BCDF-GHJK` as two, anything else whole (`codeHalves`).
pub fn halves(code: &str) -> Vec<String> {
    match code.split_once('-') {
        Some((a, b)) if !a.is_empty() && !b.is_empty() && !b.contains('-') => vec![a.into(), b.into()],
        _ => vec![code.into()],
    }
}

/// A sentence: capitalised, ending with a full stop (`sentence`).
pub fn sentence(text: &str) -> String {
    pley_core::http::sentence(text)
}

/// Where signing in with a code stands (`LinkStatus`).
#[derive(Clone, Debug, PartialEq)]
pub enum LinkStatus {
    /// "Getting a code…"
    Starting,
    /// "Waiting for approval…"
    Waiting,
    /// "Can't reach the server. Trying again in N s", N counting down to `retry_at`.
    Unreachable { retry_at: f64 },
    /// "Request declined": a new code is offered.
    Declined,
    /// "Code expired: press OK for a new one".
    Expired,
    /// The error's sentence: only the way back is offered.
    Failed(String),
    /// Approved: "Signed in as …" for half a second.
    SignedIn { name: Option<String> },
}

impl LinkStatus {
    /// Coming back to the screen doesn't ask again after these.
    pub fn is_final(&self) -> bool {
        matches!(self, LinkStatus::Declined | LinkStatus::Expired | LinkStatus::Failed(_) | LinkStatus::SignedIn { .. })
    }

    /// "Get a new code" shows instead of the notes, and takes focus.
    pub fn needs_new_code(&self) -> bool {
        matches!(self, LinkStatus::Declined | LinkStatus::Expired)
    }
}

/// Whom the code signs in as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Signing this TV in (the code screen).
    SignIn,
    /// A profile that needs its password, approved on a phone; `previous` is who the app was.
    Profile { id: i64, name: String, previous: Option<i64> },
    /// "Sign in as another profile" (the sign-in can't switch): whoever the approver picks.
    Another { previous: Option<i64> },
}

/// The name being awaited after an approval.
struct NameWait {
    until: f64,
    next: f64,
    ask: Option<Promise<Result<Me, ApiError>>>,
}

/// A sign-in with a code, run for one screen.
pub struct LinkFlow {
    server: String,
    purpose: Purpose,
    status: LinkStatus,
    code: Option<DeviceCode>,
    run: Option<(Receiver<LinkState>, Cancel)>,
    install: Option<Promise<Result<(), AuthError>>>,
    name: Option<NameWait>,
    done_at: Option<f64>,
    visible: bool,
}

impl Drop for LinkFlow {
    fn drop(&mut self) {
        self.stop();
    }
}

impl LinkFlow {
    /// A sign-in to `server` for `purpose`; it starts on the first [`LinkFlow::update`].
    pub fn new(server: &str, purpose: Purpose) -> LinkFlow {
        LinkFlow {
            server: server.to_string(),
            purpose,
            status: LinkStatus::Starting,
            code: None,
            run: None,
            install: None,
            name: None,
            done_at: None,
            visible: true,
        }
    }

    /// Where it stands.
    pub fn status(&self) -> &LinkStatus {
        &self.status
    }

    /// The code shown, once there is one.
    pub fn code(&self) -> Option<&DeviceCode> {
        self.code.as_ref()
    }

    /// The server it signs in to.
    pub fn server(&self) -> &str {
        &self.server
    }

    /// Whom it signs in as.
    pub fn purpose(&self) -> &Purpose {
        &self.purpose
    }

    /// Starts asking: the code kept from before while it works, else a new one.
    fn start(&mut self, cx: &mut AppCx) {
        if self.run.is_some() || self.status.is_final() || self.install.is_some() || self.name.is_some() {
            return;
        }
        let resume = self.code.clone();
        self.run_from(cx, resume);
    }

    fn run_from(&mut self, cx: &mut AppCx, resume: Option<DeviceCode>) {
        let (tx, rx) = channel();
        let waker = cx.tasks().waker().clone();
        let (cancel, _thread) =
            DeviceSignIn::new(cx.state.http().clone()).run_on_thread(self.server.clone(), resume, tx, waker);
        self.run = Some((rx, cancel));
    }

    fn stop(&mut self) {
        if let Some((_, cancel)) = self.run.take() {
            cancel.cancel();
        }
    }

    /// "Get a new code": both counts start over; the old code stays, dimmed, until the new one
    /// comes.
    pub fn new_code(&mut self, cx: &mut AppCx) {
        self.stop();
        self.status = LinkStatus::Starting;
        self.run_from(cx, None);
    }

    /// The app went to the background or came back: polling stops, and starts again with the same
    /// code (or a new one if it expired meanwhile).
    pub fn visible(&mut self, visible: bool, cx: &mut AppCx) {
        self.visible = visible;
        if visible {
            self.start(cx);
        } else {
            self.stop();
        }
    }

    /// Runs the flow at `now`: news from the sign-in thread, the tokens kept, the name awaited,
    /// and Home after "Signed in". Returns when it wants to run again, if it does by itself.
    pub fn update(&mut self, cx: &mut AppCx, now: f64) -> Option<f64> {
        if self.visible
            && self.run.is_none()
            && !self.status.is_final()
            && self.install.is_none()
            && self.name.is_none()
        {
            self.start(cx);
        }
        let mut news = Vec::new();
        if let Some((rx, _)) = &self.run {
            while let Ok(state) = rx.try_recv() {
                news.push(state);
            }
        }
        for state in news {
            self.hear(state, cx, now);
        }

        if let Some(promise) = self.install.as_mut()
            && let Some(answer) = promise.take()
        {
            self.install = None;
            match answer {
                Ok(()) => self.installed(cx, now),
                Err(e) => self.status = LinkStatus::Failed(e.to_string()),
            }
        }

        if self.name.is_some() {
            self.await_name(cx, now);
        }

        if let Some(at) = self.done_at
            && now >= at + SIGNED_IN_FOR
        {
            self.done_at = None;
            cx.reset(Route::Home);
            return None;
        }
        match &self.status {
            LinkStatus::Unreachable { retry_at } if *retry_at > now => {
                // The countdown ticks each second.
                Some(now + ((retry_at - now).fract().max(0.05)))
            }
            LinkStatus::SignedIn { .. } => self.done_at.map(|t| t + SIGNED_IN_FOR),
            // While an ask is on its way only the deadline needs a frame: the task pool wakes the
            // loop when the answer comes.
            _ => self.name.as_ref().map(|n| if n.ask.is_some() { n.until } else { n.next.min(n.until) }),
        }
    }

    /// Approved: the new sign-in is being kept, or "Signed in" shows. Leaving now would drop
    /// tokens the server already gave in place of the old ones, so the way back waits.
    pub fn settling(&self) -> bool {
        self.install.is_some() || self.name.is_some() || matches!(self.status, LinkStatus::SignedIn { .. })
    }

    fn hear(&mut self, state: LinkState, cx: &mut AppCx, now: f64) {
        match state {
            LinkState::Starting => self.status = LinkStatus::Starting,
            LinkState::Code(code) => self.code = Some(code),
            LinkState::Waiting => self.status = LinkStatus::Waiting,
            LinkState::Unreachable { retry_in } => {
                self.status = LinkStatus::Unreachable { retry_at: now + retry_in.as_secs_f64() };
            }
            LinkState::Declined => {
                self.run = None;
                self.status = LinkStatus::Declined;
            }
            LinkState::Expired => {
                self.run = None;
                self.status = LinkStatus::Expired;
            }
            LinkState::Failed(message) => {
                self.run = None;
                self.status = LinkStatus::Failed(message);
            }
            LinkState::SignedIn(tokens) => {
                self.run = None;
                let scope = self.code.as_ref().map(|c| c.scope.clone()).unwrap_or_default();
                self.install = Some(cx.state.install_sign_in(tokens, scope));
            }
        }
    }

    /// The tokens are kept: the app is signed in, and the name is awaited. An approval's new
    /// tokens may be for another profile than the app was: who it acts as follows once the server
    /// says.
    fn installed(&mut self, cx: &mut AppCx, now: f64) {
        match &self.purpose {
            Purpose::SignIn => cx.state.signed_in_now(),
            Purpose::Profile { .. } | Purpose::Another { .. } => cx.state.pass_profile_gate(),
        }
        self.name = Some(NameWait { until: now + NAME_WAIT, next: now, ask: None });
        self.await_name(cx, now);
    }

    /// Asks who the app is every 0.4 s for at most 3 s, until it acts as the profile wanted.
    fn await_name(&mut self, cx: &mut AppCx, now: f64) {
        let Some(wait) = self.name.as_mut() else { return };
        let mut found: Option<Me> = None;
        if let Some(ask) = wait.ask.as_mut()
            && let Some(answer) = ask.take()
        {
            wait.ask = None;
            wait.next = now + NAME_ASK_EVERY;
            if let Ok(me) = answer
                && wants(&self.purpose, &me)
            {
                found = Some(me);
            }
        } else if wait.ask.as_ref().is_some_and(|a| a.failed()) {
            wait.ask = None;
            wait.next = now + NAME_ASK_EVERY;
        }
        let out_of_time = found.is_none() && now >= wait.until;
        if found.is_none() && !out_of_time && wait.ask.is_none() && now >= wait.next {
            wait.ask = Some(cx.spawn(|api| api.me()));
        }
        if found.is_none() && !out_of_time {
            return;
        }
        self.name = None;
        match (&self.purpose, &found) {
            (Purpose::SignIn, _) => {}
            (_, Some(me)) => cx.state.switched(me.id),
            // Who the app acts as comes with the account's own ask.
            (_, None) => cx.state.signed_in_now(),
        }
        // The server's `name` is the display name, or the username without one.
        let name = found.map(|me| me.name);
        self.status = LinkStatus::SignedIn { name: name.filter(|n| !n.trim().is_empty()) };
        self.done_at = Some(now);
    }
}

/// Whether the server acts as who the approval made the app (`LinkViewModel.signedIn`): the
/// profile asked for, or whoever else the approver picked.
fn wants(purpose: &Purpose, me: &Me) -> bool {
    match purpose {
        Purpose::SignIn => true,
        Purpose::Profile { id, previous, .. } => me.id == *id || Some(me.id) != *previous,
        Purpose::Another { previous } => Some(me.id) != *previous,
    }
}

/// What the link layout draws besides the flow: its title, its introduction (an approval's), and
/// its way back.
pub struct LinkLabels<'a> {
    /// "Sign in to Pley", or an approval's title.
    pub title: &'a str,
    /// An approval's two lines under the title.
    pub intro: Option<&'a str>,
    /// The way back under the QR code: "Change server" or "Use another profile".
    pub back: &'a str,
}

/// What was chosen on the link layout this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkChoice {
    /// "Get a new code".
    pub new_code: bool,
    /// The way back.
    pub back: bool,
}

/// The link layout's pictures, kept while it shows.
#[derive(Default)]
pub struct LinkView {
    logo: Logo,
    qr: Option<(String, Qr)>,
    /// Which button focus was last sent to: "Get a new code" or the way back.
    placed_new_code: Option<bool>,
}

impl LinkView {
    /// Draws the layout of `flow` (`TvLinkContent`).
    pub fn show(&mut self, ui: &mut Ui, flow: &LinkFlow, labels: &LinkLabels) -> LinkChoice {
        brand_glow(ui);
        let status = flow.status();
        if let LinkStatus::SignedIn { name } = status {
            signed_in(ui, name.as_deref());
            return LinkChoice::default();
        }
        let mut choice = LinkChoice::default();
        let needs_new = status.needs_new_code();
        let x = theme::GUTTER;
        let width = 440.0;
        let mut y = 56.0;
        self.logo.show(ui, Point::new(x, y), 40.0);
        y += 40.0 + 28.0;
        ui.text(Point::new(x, y), labels.title, &theme::PAGE_TITLE, &TextOpts::line(width), theme::FG);
        y += theme::PAGE_TITLE.line_height;
        if let Some(intro) = labels.intro {
            y += 6.0;
            y += ui.text(Point::new(x, y), intro, &theme::BODY, &TextOpts::wrap(width, 2), theme::MUTED).h;
            y += 10.0;
        } else {
            y += 18.0;
        }
        let code = flow.code();
        step(ui, Point::new(x, y), 1, |ui, at| {
            ui.text(at, SCAN_STEP, &theme::BODY_LARGE, &TextOpts::line(width - 42.0), theme::FG);
        });
        y += 28.0 + 12.0;
        let address = code.map_or_else(|| "…".to_string(), DeviceCode::address);
        step(ui, Point::new(x, y), 2, |ui, at| {
            let lead = ui.layout(GO_TO, &theme::BODY_LARGE, &free_line());
            let w = lead.size.w + space(ui, &theme::BODY_LARGE);
            // The address in `Tv.Section`'s size, bold, on the same baseline as "or go to".
            let style = pley_base::TextStyle { line_height: theme::BODY_LARGE.line_height, ..theme::SECTION };
            let rest = (width - 42.0 - w).max(1.0);
            let shown = ui.layout(&address, &style, &TextOpts::line(rest));
            let dy = lead.first_baseline() - shown.first_baseline();
            ui.draw_text(at, lead, theme::FG);
            ui.draw_text(Point::new(at.x + w, at.y + dy), shown, theme::FG);
        });
        y += 28.0 + 12.0;
        step(ui, Point::new(x, y), 3, |ui, at| {
            ui.text(at, ENTER_STEP, &theme::BODY_LARGE, &TextOpts::line(width - 42.0), theme::FG);
        });
        y += 28.0 + 12.0;
        let dim = code.is_none() || matches!(status, LinkStatus::Expired | LinkStatus::Starting);
        code_fields(ui, Point::new(x, y), code.map(DeviceCode::shown_code).as_deref(), dim);
        y += 76.0 + 16.0;

        let new_id = ui.id("new-code");
        let back_id = ui.id("back");
        if needs_new {
            ui.group(ui.id("new-code-row"), Group::row(), |ui| {
                let r = Button::pill(NEW_CODE).icon(Icon::RefreshCw).show(ui, new_id, Point::new(x, y));
                choice.new_code = r.activated;
            });
        } else {
            let note = TextOpts::line(width);
            ui.text(Point::new(x, y), SAME_WIFI, &theme::BODY, &note, theme::MUTED);
            let y2 = y + theme::BODY.line_height + 6.0;
            ui.text(Point::new(x, y2), OTHER_DEVICE, &theme::BODY, &note, theme::MUTED);
        }

        // The status line at y 478.
        let at = Point::new(x, 478.0);
        let now = ui.time();
        match status {
            LinkStatus::Starting => {
                status_line(ui, at, width, GETTING_CODE, &theme::LABEL, theme::MUTED, true);
            }
            LinkStatus::Waiting => {
                status_line(ui, at, width, WAITING, &theme::LABEL, theme::MUTED, true);
            }
            LinkStatus::Unreachable { retry_at } => {
                let left = (retry_at - now).max(0.0).ceil() as u64;
                status_line(ui, at, width, &retrying(left), &theme::LABEL, theme::DANGER_TEXT, false);
                if *retry_at > now {
                    ui.repaint_after((retry_at - now).fract().max(0.05));
                }
            }
            LinkStatus::Declined => {
                status_line(ui, at, width, DECLINED, &theme::LABEL, theme::DANGER_TEXT, false);
            }
            LinkStatus::Expired => {
                status_line(ui, at, width, EXPIRED, &theme::LABEL, theme::MUTED, false);
            }
            LinkStatus::Failed(message) => {
                status_line(ui, at, width, &sentence(message), &theme::LABEL, theme::DANGER_TEXT, false);
            }
            LinkStatus::SignedIn { .. } => {}
        }

        // The right column, x 600 from y 120: the QR code, its caption and the way back.
        let qx = 600.0;
        let tile = Rect::new(qx, 120.0, 268.0, 268.0);
        let qr_text = code.map(|c| c.qr_address().to_string());
        if self.qr.as_ref().map(|(t, _)| t) != qr_text.as_ref() {
            self.qr = qr_text.and_then(|t| Qr::new(&t).ok().map(|q| (t, q)));
        }
        let qr_alpha = if *status == LinkStatus::Expired { 0.4 } else { 1.0 };
        match self.qr.as_mut() {
            Some((_, qr)) => qr.show(ui, tile, radius::DIALOG, qr_alpha),
            None => ui.fill(tile, theme::SURFACE2, radius::DIALOG),
        }
        let caption = TextOpts::line(268.0).aligned(pley_base::Align::Center);
        ui.text(Point::new(qx, tile.bottom() + 12.0), SCAN, &theme::CAPTION, &caption, theme::MUTED);
        let by = tile.bottom() + 12.0 + theme::CAPTION.line_height + 18.0;
        let back = Button::text(labels.back);
        let bw = back.size(ui).w;
        ui.group(ui.id("back-row"), Group::row(), |ui| {
            choice.back = back.show(ui, back_id, Point::new(qx + (268.0 - bw) / 2.0, by)).activated;
        });

        // Focus: the way back, or "Get a new code" whenever a new code is needed.
        let target = if needs_new { new_id } else { back_id };
        ui.initial_focus(target);
        if self.placed_new_code != Some(needs_new) {
            self.placed_new_code = Some(needs_new);
            ui.request_focus(target);
        }
        if choice.new_code || choice.back {
            ui.request_repaint();
        }
        choice
    }
}

/// A numbered step (`Step`): a 28 dp Surface3 box with the number, 14 dp, then its words.
fn step(ui: &mut Ui, at: Point, n: u32, words: impl FnOnce(&mut Ui, Point)) {
    let tile = Rect::new(at.x, at.y, 28.0, 28.0);
    ui.fill(tile, theme::SURFACE3, radius::FIELD);
    let label = n.to_string();
    let w = ui.measure(&label, &theme::LABEL, &free_line()).w;
    let ty = at.y + (28.0 - theme::LABEL.line_height) / 2.0;
    ui.text(Point::new(at.x + (28.0 - w) / 2.0, ty), &label, &theme::LABEL, &free_line(), theme::FG);
    let wy = at.y + (28.0 - theme::BODY_LARGE.line_height) / 2.0;
    words(ui, Point::new(at.x + 28.0 + 14.0, wy));
}

/// The code in two 76 dp Surface2 fields (`CodeFields`), a 20 × 4 Subtle dash between them in a
/// 52 dp gap; "····" in both before there is a code; at 40 % while dimmed.
fn code_fields(ui: &mut Ui, at: Point, code: Option<&str>, dim: bool) {
    let parts = code.map_or_else(|| vec!["····".to_string(), "····".to_string()], halves);
    let alpha = if dim { 0.4 } else { 1.0 };
    ui.opacity(alpha, |ui| {
        let mut x = at.x;
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                ui.fill(Rect::new(x + 16.0, at.y + 36.0, 20.0, 4.0), theme::SUBTLE, radius::CONTROL.min(2.0));
                x += 52.0;
            }
            let w = ui.measure(part, &theme::CODE, &free_line()).w;
            let field = Rect::new(x, at.y, w + 40.0, 76.0);
            ui.fill(field, theme::SURFACE2, radius::CARD);
            let ty = at.y + (76.0 - theme::CODE.line_height) / 2.0;
            ui.text(Point::new(x + 20.0, ty), part, &theme::CODE, &free_line(), theme::FG);
            x = field.right();
        }
    });
}

/// "Signed in as Anna" (`SignedIn`): at x 96, y 200, a 64 dp Success square with a check, then the
/// words in `Tv.PageTitle` on at most two lines, 20 apart.
fn signed_in(ui: &mut Ui, name: Option<&str>) {
    let tile = Rect::new(theme::GUTTER, 200.0, 64.0, 64.0);
    ui.fill(tile, theme::SUCCESS, radius::DIALOG);
    ui.icon(tile.place(Size::new(32.0, 32.0), 0.5, 0.5), Icon::Check, theme::BG);
    let words = signed_in_text(name);
    ui.text(Point::new(tile.x, tile.bottom() + 20.0), &words, &theme::PAGE_TITLE, &TextOpts::wrap(440.0, 2), theme::FG);
}

/// The code screen of `Route::Link`. It keeps the screen on while it shows.
pub struct LinkScreen {
    server: String,
    flow: Option<LinkFlow>,
    view: LinkView,
    device: Arc<dyn Device>,
}

impl LinkScreen {
    /// The code screen for `server`; at its first frame the server becomes the one in use (an
    /// address that can't be read, as the simulator's `link:here`, keeps the server in use).
    pub fn new(cx: &mut AppCx, server: &str) -> LinkScreen {
        LinkScreen { server: server.to_string(), flow: None, view: LinkView::default(), device: cx.device().clone() }
    }

    /// The sign-in, once the screen has shown.
    pub fn flow(&self) -> Option<&LinkFlow> {
        self.flow.as_ref()
    }
}

impl Drop for LinkScreen {
    fn drop(&mut self) {
        self.device.keep_awake(false);
    }
}

impl Screen for LinkScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let now = ui.time();
        let flow = self.flow.get_or_insert_with(|| {
            cx.state.use_server(&self.server);
            let server = cx.state.server().unwrap_or(&self.server).to_string();
            self.device.keep_awake(true);
            LinkFlow::new(&server, Purpose::SignIn)
        });
        if let Some(at) = flow.update(cx, now) {
            ui.repaint_after((at - now).max(0.0));
        }
        let labels = LinkLabels { title: TITLE, intro: None, back: CHANGE_SERVER };
        let choice = self.view.show(ui, flow, &labels);
        if choice.new_code {
            flow.new_code(cx);
        }
        if choice.back && !flow.settling() {
            cx.reset(Route::Servers);
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        if let (AppEvent::Visible(v), Some(flow)) = (event, self.flow.as_mut()) {
            flow.visible(*v, cx);
            self.device.keep_awake(*v);
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Bare
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_split_in_two_halves() {
        assert_eq!(halves("BCDF-GHJK"), ["BCDF", "GHJK"]);
        assert_eq!(halves("BCDFGHJK"), ["BCDFGHJK"]);
        assert_eq!(halves("AB-CD-EF"), ["AB-CD-EF"]);
        assert_eq!(halves("-ABC"), ["-ABC"]);
    }

    #[test]
    fn the_words_follow_android() {
        assert_eq!(retrying(4), "Can't reach the server. Trying again in 4 s");
        assert_eq!(signed_in_text(Some("Anna")), "Signed in as Anna");
        assert_eq!(signed_in_text(None), "Signed in");
        assert_eq!(
            approve_body("Robin"),
            "Robin's profile needs its password: approve this code on a phone signed in to Pley, and pick Robin there."
        );
        assert_eq!(
            sentence("this code doesn't work anymore: get a new one"),
            "This code doesn't work anymore: get a new one."
        );
    }

    #[test]
    fn final_states_stop_asking() {
        assert!(LinkStatus::Declined.is_final() && LinkStatus::Declined.needs_new_code());
        assert!(LinkStatus::Expired.needs_new_code());
        assert!(LinkStatus::Failed("x".into()).is_final() && !LinkStatus::Failed("x".into()).needs_new_code());
        assert!(!LinkStatus::Waiting.is_final() && !LinkStatus::Unreachable { retry_at: 0.0 }.is_final());
    }

    #[test]
    fn approvals_wait_for_the_profile_asked() {
        let me = |id| Me { id, name: "Robin".into(), ..Me::default() };
        assert!(wants(&Purpose::SignIn, &me(1)));
        let robin = Purpose::Profile { id: 3, name: "Robin".into(), previous: Some(1) };
        assert!(wants(&robin, &me(3)));
        // Still who the app was: the new tokens aren't in use yet.
        assert!(!wants(&robin, &me(1)));
        // The approver picked someone else: the app acts as them.
        assert!(wants(&robin, &me(2)));
        assert!(wants(&Purpose::Another { previous: Some(1) }, &me(2)));
        assert!(!wants(&Purpose::Another { previous: Some(1) }, &me(1)));
    }

    use crate::testing::{Rig, fixtures};
    use pley_base::Key;
    use pley_fixtures::DeviceApproval;
    use pley_ui::Id;

    fn code(user_code: &str) -> DeviceCode {
        DeviceCode {
            server: "http://192.168.1.20:7539".into(),
            issuer: "http://192.168.1.20:7539".into(),
            device_code: "secret".into(),
            user_code: user_code.into(),
            verification_uri: "http://192.168.1.20:7539/link".into(),
            verification_uri_complete: Some(format!("http://192.168.1.20:7539/link?code={user_code}")),
            expires_at: u64::MAX,
            interval: std::time::Duration::from_secs(5),
            token_endpoint: "http://192.168.1.20:7539/api/oauth/token".into(),
            revocation_endpoint: "http://192.168.1.20:7539/api/oauth/revoke".into(),
            scope: "library play playlists profiles".into(),
        }
    }

    /// The link layout alone, for a flow in a state set by hand.
    struct Layout {
        flow: LinkFlow,
        view: LinkView,
        chosen: LinkChoice,
    }

    impl Screen for Layout {
        fn ui(&mut self, ui: &mut Ui, _cx: &mut AppCx) {
            let labels = LinkLabels { title: TITLE, intro: None, back: CHANGE_SERVER };
            let choice = self.view.show(ui, &self.flow, &labels);
            self.chosen.new_code |= choice.new_code;
            self.chosen.back |= choice.back;
        }

        fn chrome(&self) -> Chrome {
            Chrome::Bare
        }
    }

    fn layout(status: LinkStatus, code: Option<DeviceCode>) -> Layout {
        let mut flow = LinkFlow::new("http://192.168.1.20:7539", Purpose::SignIn);
        flow.status = status;
        flow.code = code;
        // Drawn only: nothing asks the server.
        flow.visible = false;
        Layout { flow, view: LinkView::default(), chosen: LinkChoice::default() }
    }

    #[test]
    fn the_layout_shows_the_code_its_page_and_where_focus_goes() {
        let mut rig = Rig::new();
        let mut s = layout(LinkStatus::Waiting, Some(code("BCDFGHJK")));
        rig.frame(&mut s, Vec::new());
        for words in [TITLE, SCAN_STEP, GO_TO, "192.168.1.20:7539/link", ENTER_STEP, "BCDF", "GHJK", SAME_WIFI] {
            assert!(rig.says(words), "{words} isn't shown");
        }
        assert!(rig.says(OTHER_DEVICE) && rig.says(SCAN) && rig.says(WAITING));
        assert_eq!(rig.focused(), Some(Id::ROOT.with("back")));
        rig.press(&mut s, Key::Ok);
        assert!(s.chosen.back);

        // Expired: the code dimmed, "Get a new code" instead of the notes, and focus on it.
        s.flow.status = LinkStatus::Expired;
        rig.advance(&mut s, 0.1);
        assert!(rig.says(EXPIRED) && rig.says(NEW_CODE) && !rig.says(SAME_WIFI));
        assert_eq!(rig.focused(), Some(Id::ROOT.with("new-code")));
        rig.press(&mut s, Key::Ok);
        assert!(s.chosen.new_code);

        // Before a code: dots in both fields and "…" for the page.
        let mut s = layout(LinkStatus::Starting, None);
        rig.frame(&mut s, Vec::new());
        assert!(rig.says(GETTING_CODE) && rig.says("····") && rig.says("…"));

        let mut s = layout(LinkStatus::Failed("this server is too old for this app".into()), None);
        rig.frame(&mut s, Vec::new());
        assert!(rig.says("This server is too old for this app.") && !rig.says(NEW_CODE));

        let mut s = layout(LinkStatus::SignedIn { name: Some("Anna".into()) }, Some(code("BCDFGHJK")));
        rig.frame(&mut s, Vec::new());
        assert!(rig.says("Signed in as Anna") && !rig.says("BCDF"));
    }

    #[test]
    fn a_code_is_approved_and_the_tv_goes_home_signed_in() {
        let server = fixtures();
        server.set_device_approval(DeviceApproval::Never);
        let mut rig = Rig::new();
        let mut s = rig.cx(|cx| LinkScreen::new(cx, server.url()));
        rig.frame(&mut s, Vec::new());
        assert!(rig.device.awake(), "the screen stays on while the code shows");
        assert!(rig.until(&mut s, |rig| rig.says(WAITING)), "no code came");
        let shown = s.flow().and_then(LinkFlow::code).map(DeviceCode::shown_code).unwrap();
        assert!(halves(&shown).iter().all(|half| rig.says(half)));

        server.set_device_approval(DeviceApproval::Allow);
        assert!(rig.until(&mut s, |rig| rig.says("Signed in as Alex")), "never signed in");
        assert!(!rig.reset_to(&Route::Home), "\"Signed in\" shows first");
        assert!(rig.until(&mut s, |rig| rig.reset_to(&Route::Home)), "Home never opened");
        assert!(rig.state.has_tokens(server.url()));
        assert!(rig.state.profile_gate_passed(), "who approved picked the profile");
        drop(s);
        assert!(!rig.device.awake());
    }

    #[test]
    fn a_declined_code_offers_a_new_one() {
        let server = fixtures();
        server.set_device_approval(DeviceApproval::Deny);
        let mut rig = Rig::new();
        let mut s = rig.cx(|cx| LinkScreen::new(cx, server.url()));
        assert!(rig.until(&mut s, |rig| rig.says(DECLINED)), "never declined");
        let first = s.flow().and_then(LinkFlow::code).map(DeviceCode::shown_code);
        assert!(rig.says(NEW_CODE));
        assert_eq!(rig.focused(), Some(Id::ROOT.with("new-code")));

        server.set_device_approval(DeviceApproval::Never);
        rig.press(&mut s, Key::Ok);
        let fresh = |rig: &Rig, s: &LinkScreen| {
            rig.says(WAITING) && s.flow().and_then(LinkFlow::code).map(DeviceCode::shown_code) != first
        };
        let start = std::time::Instant::now();
        while !fresh(&rig, &s) && start.elapsed() < std::time::Duration::from_secs(10) {
            rig.until(&mut s, |rig| rig.says(WAITING));
        }
        assert!(fresh(&rig, &s), "no new code came");
        assert_eq!(rig.focused(), Some(Id::ROOT.with("back")), "focus goes back to Change server");
    }

    #[test]
    fn a_server_out_of_reach_is_asked_again_and_hiding_stops_asking() {
        let mut rig = Rig::new();
        let mut s = rig.cx(|cx| LinkScreen::new(cx, "http://127.0.0.1:9"));
        assert!(rig.until(&mut s, |rig| rig.says("Can't reach the server. Trying again in 2 s")));
        assert!(matches!(s.flow().unwrap().status(), LinkStatus::Unreachable { .. }));
        rig.cx(|cx| s.event(&AppEvent::Visible(false), cx));
        assert!(s.flow.as_ref().unwrap().run.is_none() && !rig.device.awake());
        rig.cx(|cx| s.event(&AppEvent::Visible(true), cx));
        assert!(s.flow.as_ref().unwrap().run.is_some() && rig.device.awake());
        // "Change server" starts over at the list.
        rig.press(&mut s, Key::Ok);
        assert!(rig.reset_to(&Route::Servers));
    }
}
