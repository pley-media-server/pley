//! "Who's watching?" (`TvProfilesScreen.kt`, `ProfilesViewModel.kt`; the TV app's
//! `onboarding/profiles.tsx`): the household in one centred row, the PIN pad of a profile that
//! has one, and approval on a phone for a profile that needs its password. The steps replace each
//! other with a 200 ms crossfade, and Back on the PIN pad or the approval returns to the row.
//!
//! The saved household draws at once, then the server's. Picking the current profile continues,
//! except at a cold start when it has a PIN: the PIN is confirmed first. Another profile is
//! switched to with the server's switch-profile grant (with its PIN when it has one); a profile
//! that needs its password is approved with a new code on a phone signed in to Pley. A switch
//! once sent always gets its answer, even after Back: the server may have made it already.

use std::sync::Arc;

use pley_base::{Key, Point, TextOpts};
use pley_core::auth::{AuthError, SwitchRefusal};
use pley_core::http::sentence;
use pley_core::model::{AppProfile, Me, ProfileNeeds};
use pley_core::{ApiError, Device, Promise};
use pley_i18n::t;
use pley_kit::{Button, Keypad, PadKey, PadState, PinDots, ProfileTile};
use pley_shell::{AppCx, AppEvent, Chrome, Route, Screen};
use pley_ui::theme;
use pley_ui::{Group, Motion, Ui};

use crate::kit::profile_fill;
use crate::link::{LinkFlow, LinkLabels, LinkView, Purpose};

/// PINs are 4 to 8 digits (`PinEntry`).
pub const PIN_MIN: usize = 4;
pub const PIN_MAX: usize = 8;
/// An avatar's size and the gap between two.
const AVATAR: f32 = 100.0;
const AVATAR_GAP: f32 = 40.0;
/// More profiles than this scroll, the focused one's left edge at x 288.
const FIT: usize = 6;

/// "Enter Anna's PIN".
pub fn enter_pin(name: &str) -> String {
    t!("profiles.pin.title", name = name)
}

/// The left edge of the row of `n` profiles with the one at `focused` focused: centred while
/// they fit, else the focused one's left edge at x 288, never past 48 from either end.
pub fn row_left(n: usize, focused: usize) -> f32 {
    let span = n as f32 * AVATAR + n.saturating_sub(1) as f32 * AVATAR_GAP;
    if n <= FIT {
        (theme::CANVAS_W - span) / 2.0
    } else {
        let want = 288.0 - focused as f32 * (AVATAR + AVATAR_GAP);
        want.clamp(theme::CANVAS_W - theme::SAFE_X - span, theme::SAFE_X)
    }
}

/// The household as it shows: who is current comes from who the app is (`me`) when it is in the
/// list (a saved list may be from before a switch), and the current profile wears a lock when it
/// has a PIN (the server lists it as needing nothing).
pub fn shown(profiles: &[AppProfile], me: Option<&Me>) -> Vec<AppProfile> {
    let me = me.filter(|m| profiles.iter().any(|p| p.id == m.id));
    profiles
        .iter()
        .map(|p| match me {
            None => p.clone(),
            Some(m) if m.id == p.id => AppProfile {
                current: true,
                needs: if m.has_pin { ProfileNeeds::Pin } else { ProfileNeeds::None },
                ..p.clone()
            },
            Some(_) => AppProfile { current: false, ..p.clone() },
        })
        .collect()
}

/// What picking a profile leads to (`ProfilesViewModel.pick`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Home, as who the app is.
    Done,
    /// Its PIN; `confirm`: the current profile's, at a cold start.
    Pin { confirm: bool },
    /// Switched to at once.
    Switch,
    /// Approved on a phone with a new code.
    Approve,
    /// It can't be used from the app: the PIN pad, locked.
    Locked,
}

/// What picking `profile` does; `gate`: "Who's watching?" asked at a cold start, the only time the
/// current profile's PIN is confirmed.
pub fn pick(profile: &AppProfile, gate: bool) -> Pick {
    if profile.current {
        return if gate && profile.needs == ProfileNeeds::Pin { Pick::Pin { confirm: true } } else { Pick::Done };
    }
    match profile.needs {
        ProfileNeeds::None => Pick::Switch,
        ProfileNeeds::Pin => Pick::Pin { confirm: false },
        ProfileNeeds::Password => Pick::Approve,
        ProfileNeeds::Locked => Pick::Locked,
    }
}

/// What a refusal without a reason says (`Profiles.message`): the server's description, else that
/// the profile can't be switched to.
fn refused_words(e: &AuthError) -> String {
    let description = match e {
        AuthError::Refused { oauth: Some(o), .. } => o.error_description.as_deref().filter(|d| !d.trim().is_empty()),
        _ => None,
    };
    description.map_or_else(|| t!("profiles.errors.notAllowed").into(), sentence)
}

/// The PIN pad's state.
struct PinStep {
    profile: AppProfile,
    confirm: bool,
    locked: bool,
    error: Option<String>,
    /// The digits typed: never shown, never kept.
    digits: String,
    shake_since: Option<f64>,
    keys: PadState,
    placed: bool,
}

impl PinStep {
    /// The pad of `profile`; `confirm`: the current profile's PIN, at a cold start.
    fn new(profile: AppProfile, confirm: bool) -> Box<PinStep> {
        Box::new(PinStep {
            profile,
            confirm,
            locked: false,
            error: None,
            digits: String::new(),
            shake_since: None,
            keys: PadState::default(),
            placed: false,
        })
    }

    /// The pad of a profile that can't be used from the app: off, saying so.
    fn locked_out(profile: AppProfile) -> Box<PinStep> {
        let mut step = PinStep::new(profile, false);
        step.locked = true;
        step.error = Some(t!("profiles.errors.locked").into());
        step
    }
}

/// The approval on a phone.
struct ApproveStep {
    flow: LinkFlow,
    view: LinkView,
    /// The profile approved, `None` for "Sign in as another profile".
    name: Option<String>,
}

enum Step {
    Pick,
    Pin(Box<PinStep>),
    Approve(Box<ApproveStep>),
}

/// A switch on its way. The screen holds it, not the step that sent it: a switch the server made
/// has ended the old tokens, and only its answer carries the new ones, so Back on the PIN pad
/// leaves it running (`ProfilesViewModel.backToPick`).
struct Switch {
    profile: AppProfile,
    /// The current profile's PIN, confirmed at a cold start.
    confirm: bool,
    /// Sent with a PIN typed on the pad.
    pin: bool,
    promise: Promise<Result<(), AuthError>>,
}

/// What a step asked for this frame.
enum Next {
    Stay,
    Pick,
    Pin(Box<PinStep>),
    /// The PIN typed on the pad, to send.
    Send(String),
    Approve(Option<AppProfile>),
    Done,
}

/// "Who's watching?", its PIN pad and its approval.
pub struct ProfilesScreen {
    profiles: Vec<AppProfile>,
    /// The household as last kept on the TV: an answer that matches it isn't written again.
    kept: Vec<AppProfile>,
    /// The first answer is awaited (Retry leaves this off, so Retry stays and keeps focus).
    loading: bool,
    load: Option<Promise<Result<Vec<AppProfile>, ApiError>>>,
    error: Option<String>,
    can_switch: bool,
    busy: Option<Switch>,
    step: Step,
    /// Counts the steps shown, for the crossfade.
    shown_steps: u32,
    /// The step fading out, drawn inert.
    leaving: Option<Step>,
    last_focused: Option<i64>,
    focused_at: usize,
    /// The row takes focus again (back from the PIN pad or the approval).
    refocus: bool,
    /// The PIN pad of this profile opens first (`Route::Pin`).
    open_pin: Option<i64>,
    gate: bool,
    started: bool,
    /// The server in use, which approvals sign in to again.
    server: String,
    device: Arc<dyn Device>,
}

impl ProfilesScreen {
    /// The screen; `pin` opens it on that profile's PIN pad, or does what picking it does when it
    /// has no PIN.
    pub fn new(cx: &mut AppCx, pin: Option<i64>) -> ProfilesScreen {
        let profiles = cx.state.profiles().map(<[AppProfile]>::to_vec).unwrap_or_else(|| cx.state.saved_household());
        ProfilesScreen {
            kept: profiles.clone(),
            profiles,
            loading: true,
            load: None,
            error: None,
            can_switch: true,
            busy: None,
            step: Step::Pick,
            shown_steps: 0,
            leaving: None,
            last_focused: None,
            focused_at: 0,
            refocus: false,
            open_pin: pin,
            gate: !cx.state.profile_gate_passed(),
            started: false,
            server: String::new(),
            device: cx.device().clone(),
        }
    }

    /// Asks the server for the household; Retry asks again, with Retry left in place.
    fn load(&mut self, cx: &mut AppCx) {
        self.can_switch = cx.state.tokens().is_none_or(|t| t.allows("profiles"));
        if !self.can_switch {
            self.loading = false;
            return;
        }
        self.error = None;
        self.load = Some(cx.spawn(|api| api.profiles()));
    }

    fn collect(&mut self, cx: &mut AppCx, now: f64) {
        if let Some(promise) = self.load.as_mut()
            && let Some(answer) = promise.take()
        {
            self.load = None;
            self.loading = false;
            match answer {
                Ok(list) => {
                    // Kept for the next start only when it changed: keeping writes to disk.
                    if list != self.kept {
                        cx.state.keep_household(list.clone());
                        self.kept.clone_from(&list);
                    }
                    self.profiles = list;
                }
                // The shell shows signing in again.
                Err(ApiError::SignedOut) => {}
                // With a list on screen, trouble stays quiet.
                Err(e) if self.profiles.is_empty() => {
                    self.error = Some(if e.status() == Some(403) {
                        t!("profiles.errors.cantSwitch").into()
                    } else {
                        sentence(&e.to_string())
                    });
                }
                Err(e) => log::info!("the household not loaded: {e}"),
            }
        } else if self.load.as_ref().is_some_and(Promise::failed) {
            self.load = None;
            self.loading = false;
        }

        if let Some(switch) = self.busy.as_mut()
            && let Some(answer) = switch.promise.take()
            && let Some(Switch { profile, confirm, pin, .. }) = self.busy.take()
        {
            match answer {
                Ok(()) => self.done(cx, (!confirm).then_some(profile.id)),
                Err(e) => self.refused(cx, profile, confirm, pin, &e, now),
            }
        } else if self.busy.as_ref().is_some_and(|s| s.promise.failed()) {
            self.busy = None;
        }
    }

    /// Home, as `switched` to, or as who the app is already.
    fn done(&mut self, cx: &mut AppCx, switched: Option<i64>) {
        match switched {
            Some(profile) => cx.state.switched(profile),
            None => cx.state.pass_profile_gate(),
        }
        cx.reset(Route::Home);
    }

    /// A switch the server didn't make, by its reason (`ProfilesViewModel.refused` and `confirm`).
    fn refused(&mut self, cx: &mut AppCx, profile: AppProfile, confirm: bool, pin: bool, e: &AuthError, now: f64) {
        match SwitchRefusal::of(e) {
            // The shell shows signing in again.
            SwitchRefusal::SignedOut => {}
            // A server that can't check a PIN for an app answers so for the profile the app is
            // already: the person goes on (`Profiles.confirmPin`). Asked for another profile, the
            // same answer is a refusal.
            SwitchRefusal::CantCheckPin if confirm => self.done(cx, None),
            SwitchRefusal::CantCheckPin => self.failed(profile, confirm, refused_words(e), now),
            SwitchRefusal::Browser if !confirm => self.go(Next::Approve(Some(profile))),
            // Sent without a PIN: the list was older than the profile's new PIN.
            SwitchRefusal::WrongPin if !pin => {
                let profile = AppProfile { needs: ProfileNeeds::Pin, ..profile };
                self.go(Next::Pin(PinStep::new(profile, false)));
            }
            r @ (SwitchRefusal::WrongPin | SwitchRefusal::PinLocked | SwitchRefusal::Throttled(_)) => {
                let locked = r == SwitchRefusal::PinLocked;
                self.wrong(profile, confirm, r.message(), locked, now);
            }
            // A PIN the server couldn't be asked about isn't wrong: no shake.
            r @ SwitchRefusal::Unreachable(_) => match &mut self.step {
                Step::Pin(step) => step.error = Some(r.message()),
                _ => self.error = Some(r.message()),
            },
            r => self.failed(profile, confirm, r.message(), now),
        }
    }

    /// A refusal on the PIN pad, which shakes; the pad opens again if Back left it (`wrong`).
    fn wrong(&mut self, profile: AppProfile, confirm: bool, message: String, locked: bool, now: f64) {
        if !matches!(&self.step, Step::Pin(step) if step.profile.id == profile.id) {
            self.go(Next::Pin(PinStep::new(profile, confirm)));
        }
        if let Step::Pin(step) = &mut self.step {
            step.digits.clear();
            step.error = Some(message);
            step.shake_since = Some(now);
            // Locked, focus goes to "Use another profile".
            step.placed &= !locked;
            step.locked = locked;
        }
    }

    /// Any other refusal: as a wrong PIN on the pad, else under the row (`failed`).
    fn failed(&mut self, profile: AppProfile, confirm: bool, message: String, now: f64) {
        if matches!(self.step, Step::Pin(_)) {
            self.wrong(profile, confirm, message, false, now);
        } else {
            self.error = Some(message).filter(|m| !m.is_empty());
        }
    }

    /// Sends the PIN typed on the pad.
    fn send(&mut self, cx: &mut AppCx, pin: String) {
        let Step::Pin(step) = &mut self.step else { return };
        if self.busy.is_some() {
            return;
        }
        step.error = None;
        let promise = cx.state.switch_profile(step.profile.id, Some(pin));
        self.busy = Some(Switch { profile: step.profile.clone(), confirm: step.confirm, pin: true, promise });
    }

    /// `Route::Pin`: what picking the profile does, shown at once (the shell's curtain covers the
    /// change) as soon as the household lists it, the saved one or the server's: its PIN pad, a
    /// switch on the row, its approval, or Home. Returns false while that isn't known yet.
    fn open_pin(&mut self, cx: &mut AppCx) -> bool {
        let Some(id) = self.open_pin else { return true };
        let Some(p) = shown(&self.profiles, cx.state.me()).into_iter().find(|p| p.id == id) else {
            if self.loading {
                return false;
            }
            self.open_pin = None;
            return true;
        };
        self.open_pin = None;
        match self.pick(cx, &p) {
            Next::Done => self.done(cx, None),
            Next::Stay => {}
            next => {
                self.go(next);
                self.leaving = None;
                self.shown_steps = 0;
            }
        }
        true
    }

    /// Moves to another step with the crossfade.
    fn go(&mut self, next: Next) {
        let step = match next {
            Next::Stay | Next::Done | Next::Send(_) => return,
            Next::Pick => {
                self.refocus = true;
                Step::Pick
            }
            Next::Pin(step) => Step::Pin(step),
            Next::Approve(profile) => {
                let server = self.server.clone();
                let previous = self.profiles.iter().find(|p| p.current).map(|p| p.id);
                let purpose = match &profile {
                    Some(p) => Purpose::Profile { id: p.id, name: p.name.clone(), previous },
                    None => Purpose::Another { previous },
                };
                self.device.keep_awake(true);
                Step::Approve(Box::new(ApproveStep {
                    flow: LinkFlow::new(&server, purpose),
                    view: LinkView::default(),
                    name: profile.map(|p| p.name),
                }))
            }
        };
        if matches!(self.step, Step::Approve(_)) && !matches!(step, Step::Approve(_)) {
            self.device.keep_awake(false);
        }
        let old = std::mem::replace(&mut self.step, step);
        self.leaving = Some(old);
        self.shown_steps += 1;
    }

    fn pick(&mut self, cx: &mut AppCx, profile: &AppProfile) -> Next {
        if self.busy.is_some() {
            return Next::Stay;
        }
        self.error = None;
        self.last_focused = Some(profile.id);
        match pick(profile, self.gate) {
            Pick::Done => Next::Done,
            Pick::Pin { confirm } => Next::Pin(PinStep::new(profile.clone(), confirm)),
            Pick::Switch => {
                let promise = cx.state.switch_profile(profile.id, None);
                self.busy = Some(Switch { profile: profile.clone(), confirm: false, pin: false, promise });
                Next::Stay
            }
            Pick::Approve => Next::Approve(Some(profile.clone())),
            Pick::Locked => Next::Pin(PinStep::locked_out(profile.clone())),
        }
    }

    /// The row, its error and its buttons. Not `live`, it is fading out: drawn as it was, with
    /// the same ids so the focused profile's ring fades from where it was, and nothing claimed.
    fn pick_ui(&mut self, ui: &mut Ui, cx: &mut AppCx, live: bool) -> Next {
        let opts = TextOpts::line(theme::CANVAS_W).aligned(pley_base::Align::Center);
        ui.text(Point::new(0.0, 110.0), t!("profiles.title"), &theme::PAGE_TITLE, &opts, theme::FG);

        let list = shown(&self.profiles, cx.state.me());
        let n = list.len();
        let target = self
            .last_focused
            .and_then(|id| list.iter().position(|p| p.id == id))
            .or_else(|| list.iter().position(|p| p.current))
            .unwrap_or(0);
        let left = ui.animate(ui.id("left"), row_left(n, self.focused_at), pley_kit::SCROLL);
        let px = ui.image_px(AVATAR);
        let busy = self.busy.as_ref().map(|s| s.profile.id);
        // Hovered with the pointer, a profile takes focus but the row stays.
        let pointer = ui.pointer_mode();
        let mut picked = None;
        let mut target_id = None;
        let row_id = ui.id("profiles");
        ui.group(row_id, Group::row(), |ui| {
            for (i, p) in list.iter().enumerate() {
                let id = ui.id(("profile", p.id));
                if i == target {
                    target_id = Some(id);
                }
                let picture: Vec<_> = p.avatar_url.as_deref().and_then(|url| cx.avatar(url, px)).into_iter().collect();
                let at = Point::new(left + i as f32 * (AVATAR + AVATAR_GAP), 204.0);
                let r = ProfileTile::new(&p.name)
                    .color(Some(profile_fill(&p.name, p.avatar_color.as_deref())))
                    .picture(&picture)
                    .locked(p.needs == ProfileNeeds::Pin)
                    .dimmed(busy == Some(p.id))
                    .show(ui, id, at);
                if live && r.focused {
                    if !pointer {
                        self.focused_at = i;
                    }
                    self.last_focused = Some(p.id);
                }
                if r.activated {
                    picked = Some(p.clone());
                }
            }
        });

        // Under the row at y 400, centred and 12 apart: what went wrong, Retry, another profile.
        let mut y = 400.0;
        if let Some(e) = &self.error {
            let opts = TextOpts::wrap(560.0, 0).aligned(pley_base::Align::Center);
            y += ui.text(Point::new((theme::CANVAS_W - 560.0) / 2.0, y), e, &theme::BODY, &opts, theme::DANGER_TEXT).h;
            y += 12.0;
        }
        let mut retry = false;
        let mut another = false;
        ui.group(ui.id("buttons"), Group::column(), |ui| {
            if list.is_empty() && !self.loading {
                let b = Button::pill(t!("common.actions.retry")).icon(pley_base::Icon::RefreshCw);
                let w = b.size(ui).w;
                let id = ui.id("retry");
                if live {
                    ui.initial_focus(id);
                }
                retry = b.show(ui, id, Point::new((theme::CANVAS_W - w) / 2.0, y)).activated;
                y += theme::BUTTON_HEIGHT + 12.0;
            }
            if !self.can_switch {
                let b = Button::text(t!("signin.approve.anyTitle"));
                let w = b.size(ui).w;
                let id = ui.id("another");
                // Retry, when it shows, has focus first (`TvProfilesScreen.kt`).
                if live && list.is_empty() && self.loading {
                    ui.initial_focus(id);
                }
                another = b.show(ui, id, Point::new((theme::CANVAS_W - w) / 2.0, y)).activated;
            }
        });
        if !live {
            return Next::Stay;
        }

        if let Some(id) = target_id {
            ui.initial_focus(id);
            if self.refocus {
                self.refocus = false;
                ui.request_focus(id);
            }
        }
        if retry {
            self.load(cx);
            ui.request_repaint();
        }
        if another {
            return Next::Approve(None);
        }
        match picked {
            Some(p) => {
                ui.request_repaint();
                self.pick(cx, &p)
            }
            None => Next::Stay,
        }
    }

    fn approve_ui(ui: &mut Ui, cx: &mut AppCx, step: &mut ApproveStep, live: bool) -> Next {
        if live {
            let now = ui.time();
            if let Some(at) = step.flow.update(cx, now) {
                ui.repaint_after((at - now).max(0.0));
            }
        }
        let intro = match &step.name {
            Some(name) => crate::link::approve_body(name),
            None => t!("signin.approve.anyIntro").to_string(),
        };
        let title = if step.name.is_some() { t!("signin.approve.title") } else { t!("signin.approve.anyTitle") };
        let labels = LinkLabels { title, intro: Some(&intro), back: t!("profiles.useAnother") };
        let choice = step.view.show(ui, &step.flow, &labels);
        if choice.new_code {
            step.flow.new_code(cx);
        }
        // Once approved, the new sign-in is kept and Home follows: the way back waits.
        if choice.back && !step.flow.settling() { Next::Pick } else { Next::Stay }
    }

    /// The PIN pad; `sending`: a PIN is on its way, and the keys wait for its answer.
    fn pin_ui(ui: &mut Ui, cx: &mut AppCx, step: &mut PinStep, live: bool, sending: bool) -> Next {
        let mut next = Next::Stay;
        let centre = theme::CANVAS_W / 2.0;
        let mut y = 30.0;
        let picture: Vec<_> =
            step.profile.avatar_url.as_deref().and_then(|url| cx.avatar(url, ui.image_px(56.0))).into_iter().collect();
        ProfileTile::new(&step.profile.name)
            .color(Some(profile_fill(&step.profile.name, step.profile.avatar_color.as_deref())))
            .picture(&picture)
            .size(56.0)
            .avatar(ui, pley_base::Rect::new(centre - 28.0, y, 56.0, 56.0));
        y += 56.0 + 14.0;
        let centred = TextOpts::line(theme::CANVAS_W).aligned(pley_base::Align::Center);
        ui.text(Point::new(0.0, y), &enter_pin(&step.profile.name), &theme::TITLE, &centred, theme::FG);
        y += theme::TITLE.line_height + 16.0;
        PinDots { typed: step.digits.len() }.show(ui, centre, y, step.shake_since);
        y += PinDots::DOT;
        if let Some(e) = &step.error {
            let opts = TextOpts::line(560.0).aligned(pley_base::Align::Center);
            let ty = y + (22.0 - theme::BODY.line_height) / 2.0;
            ui.text(Point::new(centre - 280.0, ty), e, &theme::BODY, &opts, theme::DANGER_TEXT);
        }
        y += 22.0;

        let enabled = !step.locked && live;
        let remote_digit =
            enabled && ui.keys().iter().any(|k| matches!(k.key, Key::Digit(_)) && k.pressed && !k.repeat);
        if live && !ui.keys().is_empty() {
            step.placed = true;
        }
        let size = Keypad::size();
        let pad_id = ui.id("pin");
        let key =
            Keypad::pin().enabled(!step.locked).show(ui, pad_id, Point::new(centre - size.w / 2.0, y), &mut step.keys);
        y += size.h + 16.0;
        if !sending && enabled {
            match key {
                Some(PadKey::Digit(d)) if step.digits.len() < PIN_MAX => {
                    step.digits.push(char::from(b'0' + d));
                    // Typed with the number keys, a PIN is sent with OK: OK takes focus once it could
                    // be sent, rather than 5 typing a 5.
                    if remote_digit && step.digits.len() >= PIN_MIN {
                        ui.request_focus(pad_id.with(11usize));
                    }
                }
                Some(PadKey::Delete) => {
                    step.digits.pop();
                }
                Some(PadKey::Ok) if step.digits.len() >= PIN_MIN => {
                    next = Next::Send(std::mem::take(&mut step.digits));
                    ui.request_repaint();
                }
                _ => {}
            }
        }
        if step.locked {
            let b = Button::text(t!("profiles.useAnother"));
            let w = b.size(ui).w;
            let id = ui.id("another");
            ui.group(ui.id("locked"), Group::row(), |ui| {
                if live {
                    ui.initial_focus(id);
                }
                if b.show(ui, id, Point::new(centre - w / 2.0, y)).activated {
                    next = Next::Pick;
                }
            });
            if live && !step.placed {
                step.placed = true;
                ui.request_focus(id);
            }
        } else {
            let opts = TextOpts::line(theme::CANVAS_W).aligned(pley_base::Align::Center);
            ui.text(Point::new(0.0, y), t!("profiles.pin.keysHint"), &theme::CAPTION, &opts, theme::MUTED);
        }
        next
    }

    fn step_ui(&mut self, ui: &mut Ui, cx: &mut AppCx, leaving: bool) -> Next {
        let mut step = std::mem::replace(
            if leaving { self.leaving.as_mut().expect("a step leaving") } else { &mut self.step },
            Step::Pick,
        );
        let sending = self.busy.as_ref().is_some_and(|s| s.pin);
        let next = match &mut step {
            Step::Pick => self.pick_ui(ui, cx, !leaving),
            Step::Pin(pin) => ProfilesScreen::pin_ui(ui, cx, pin, !leaving, sending),
            Step::Approve(approve) => ProfilesScreen::approve_ui(ui, cx, approve, !leaving),
        };
        let slot = if leaving { self.leaving.as_mut().expect("a step leaving") } else { &mut self.step };
        *slot = step;
        if leaving { Next::Stay } else { next }
    }
}

impl Drop for ProfilesScreen {
    fn drop(&mut self) {
        if matches!(self.step, Step::Approve(_)) {
            self.device.keep_awake(false);
        }
    }
}

impl Screen for ProfilesScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        if !self.started {
            self.started = true;
            self.server = cx.state.server().unwrap_or_default().to_string();
            self.load(cx);
        }
        self.collect(cx, ui.time());
        if self.open_pin.is_some() && !self.open_pin(cx) {
            // Nothing shows until the household says whether the profile is in it.
            return;
        }

        // The step coming in is drawn first, so its first focus wins over the one fading out.
        let step_id = ui.id(("step", self.shown_steps));
        let p = if self.shown_steps == 0 { 1.0 } else { ui.animate_from(step_id, 0.0, 1.0, Motion::OVERLAY) };
        let next = ui.opacity(p, |ui| self.step_ui(ui, cx, false));
        if p < 1.0 && self.leaving.is_some() {
            ui.opacity(1.0 - p, |ui| {
                pley_kit::inert(ui, |ui| {
                    if matches!(self.leaving, Some(Step::Pick)) {
                        self.step_ui(ui, cx, true);
                    } else {
                        ui.push_id("leaving", |ui| {
                            self.step_ui(ui, cx, true);
                        });
                    }
                });
            });
        } else {
            self.leaving = None;
        }
        match next {
            Next::Done => self.done(cx, None),
            Next::Send(pin) => self.send(cx, pin),
            other => self.go(other),
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        match &self.step {
            Step::Pick => false,
            // Approved: the new sign-in is being kept and Home follows within seconds.
            Step::Approve(step) if step.flow.settling() => true,
            _ => {
                self.go(Next::Pick);
                true
            }
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match (event, &mut self.step) {
            (AppEvent::Visible(v), Step::Approve(step)) => {
                step.flow.visible(*v, cx);
                self.device.keep_awake(*v);
            }
            // The server is back, or the household changed: the row asks again.
            (AppEvent::Changed, Step::Pick) if self.started && self.load.is_none() => self.load(cx),
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Bare
    }

    fn modal(&self) -> bool {
        !matches!(self.step, Step::Pick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The screen's words in English, which tests show.
    const TITLE: &str = "Who's watching?";
    const PIN_HINT: &str = "You can also type with the number keys on your remote.";
    const RETRY: &str = "Retry";
    const LOCKED: &str = "This profile can't be used from the app.";
    const NOT_ALLOWED: &str = "You can't switch to this profile.";
    const USE_ANOTHER: &str = "Use another profile";
    const APPROVE_TITLE: &str = "Approve on your phone";
    const ANOTHER_TITLE: &str = "Sign in as another profile";
    const ANOTHER_BODY: &str = "Approve this code on a phone signed in to Pley, and pick the profile there.";
    const WAITING: &str = "Waiting for approval…";

    fn profile(id: i64, needs: ProfileNeeds, current: bool) -> AppProfile {
        AppProfile { id, name: format!("P{id}"), avatar_url: None, avatar_color: None, current, needs }
    }

    #[test]
    fn picking_follows_what_the_profile_needs() {
        assert_eq!(pick(&profile(1, ProfileNeeds::None, true), true), Pick::Done);
        assert_eq!(pick(&profile(1, ProfileNeeds::Pin, true), true), Pick::Pin { confirm: true });
        // Only a cold start confirms the current profile's PIN.
        assert_eq!(pick(&profile(1, ProfileNeeds::Pin, true), false), Pick::Done);
        assert_eq!(pick(&profile(2, ProfileNeeds::None, false), true), Pick::Switch);
        assert_eq!(pick(&profile(2, ProfileNeeds::Pin, false), false), Pick::Pin { confirm: false });
        assert_eq!(pick(&profile(3, ProfileNeeds::Password, false), true), Pick::Approve);
        assert_eq!(pick(&profile(4, ProfileNeeds::Locked, false), true), Pick::Locked);
    }

    #[test]
    fn who_is_current_comes_from_who_the_app_is() {
        let saved = [profile(1, ProfileNeeds::None, true), profile(2, ProfileNeeds::Pin, false)];
        let me = Me { id: 2, has_pin: true, ..Me::default() };
        let list = shown(&saved, Some(&me));
        assert!(!list[0].current && list[1].current);
        assert_eq!(list[1].needs, ProfileNeeds::Pin);
        // Someone the list doesn't know leaves it as it is.
        let stranger = Me { id: 9, ..Me::default() };
        assert_eq!(shown(&saved, Some(&stranger)), saved.to_vec());
    }

    #[test]
    fn the_row_centres_or_scrolls_to_the_focused_profile() {
        // Three: 100 + 40 + 100 + 40 + 100 = 380, centred.
        assert_eq!(row_left(3, 0), (960.0 - 380.0) / 2.0);
        // Ten: the focused one's left edge at 288, never past 48 from either end.
        assert_eq!(row_left(10, 0), 48.0);
        assert_eq!(row_left(10, 3), 288.0 - 3.0 * 140.0);
        let span = 10.0 * 100.0 + 9.0 * 40.0;
        assert_eq!(row_left(10, 9), 960.0 - 48.0 - span);
        assert_eq!(enter_pin("Sam"), "Enter Sam's PIN");
        pley_i18n::with_language(pley_i18n::Language::Fr, || {
            assert_eq!(enter_pin("Sam"), "Saisissez le code PIN du profil Sam");
        });
        pley_i18n::with_language(pley_i18n::Language::Es, || assert_eq!(enter_pin("Sam"), "Introduce el PIN de Sam"));
    }

    use crate::link::approve_body;
    use crate::testing::{Rig, fixtures};
    use pley_fixtures::ids::{ALEX, ROBIN, SAM};
    use pley_ui::Id;

    fn tile(id: i64) -> Id {
        Id::ROOT.with("profiles").with(("profile", id))
    }

    fn pin_key(i: usize) -> Id {
        Id::ROOT.with("pin").with(i)
    }

    fn digits(rig: &mut Rig, s: &mut ProfilesScreen, pin: &str) {
        for d in pin.bytes() {
            rig.press(s, Key::Digit(d - b'0'));
        }
    }

    #[test]
    fn the_household_shows_with_the_current_profile_focused() {
        let server = fixtures();
        let mut rig = Rig::signed_in(&server, ALEX);
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        assert!(rig.until(&mut s, |rig| rig.says("Robin")), "the household never came");
        assert!(rig.says(TITLE) && rig.says("Alex") && rig.says("Sam"));
        assert_eq!(rig.focused(), Some(tile(ALEX)));
        assert_eq!(rig.state.saved_household().len(), 3, "kept for the next start");
        // Picking who the app already is (no PIN asked after the start) goes Home.
        rig.cx(|cx| cx.state.pass_profile_gate());
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        rig.until(&mut s, |rig| rig.says("Robin"));
        rig.press(&mut s, Key::Ok);
        assert!(rig.reset_to(&Route::Home));
    }

    #[test]
    fn a_pin_profile_asks_its_pin_and_the_right_one_switches() {
        let server = fixtures();
        let mut rig = Rig::signed_in(&server, ALEX);
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        assert!(rig.until(&mut s, |rig| rig.says("Robin")));
        rig.press(&mut s, Key::Right);
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.3);
        assert!(s.modal() && rig.says("Enter Sam's PIN") && rig.says(PIN_HINT));
        assert_eq!(rig.focused(), Some(pin_key(4)), "focus starts on 5");
        // Typed with the remote, OK takes focus once the PIN could be sent.
        digits(&mut rig, &mut s, "999");
        assert_eq!(rig.focused(), Some(pin_key(4)));
        digits(&mut rig, &mut s, "9");
        assert_eq!(rig.focused(), Some(pin_key(11)));
        rig.press(&mut s, Key::Ok);
        assert!(rig.until(&mut s, |rig| rig.says("Wrong PIN.")), "the wrong PIN wasn't refused");
        let Step::Pin(pin) = &s.step else { panic!("the PIN pad closed") };
        assert!(pin.digits.is_empty() && pin.shake_since.is_some());
        digits(&mut rig, &mut s, "1234");
        rig.press(&mut s, Key::Ok);
        assert!(rig.until(&mut s, |rig| rig.reset_to(&Route::Home)), "Home never opened");
        assert_eq!(rig.state.profile(), Some(SAM));
    }

    #[test]
    fn a_password_profile_is_approved_on_a_phone_and_back_returns_to_it() {
        let server = fixtures();
        server.set_device_approval(pley_fixtures::DeviceApproval::Never);
        let mut rig = Rig::signed_in(&server, ALEX);
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        assert!(rig.until(&mut s, |rig| rig.says("Robin")));
        rig.press(&mut s, Key::Right);
        rig.press(&mut s, Key::Right);
        rig.press(&mut s, Key::Ok);
        assert!(rig.until(&mut s, |rig| rig.says(WAITING)), "no code came");
        assert!(rig.says(APPROVE_TITLE) && rig.says(&approve_body("Robin")) && rig.says(USE_ANOTHER));
        assert!(s.modal() && rig.device.awake());
        assert!(rig.cx(|cx| s.back(cx)));
        rig.advance(&mut s, 0.3);
        assert!(!s.modal() && !rig.device.awake());
        assert_eq!(rig.focused(), Some(tile(ROBIN)));
    }

    #[test]
    fn the_pin_route_opens_the_pad_once_the_household_lists_the_profile() {
        let server = fixtures();
        let mut rig = Rig::signed_in(&server, ALEX);
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, Some(SAM)));
        rig.frame(&mut s, Vec::new());
        assert!(!rig.says(TITLE), "the row doesn't flash first");
        assert!(rig.until(&mut s, |rig| rig.says("Enter Sam's PIN")));
        assert!(!rig.says(TITLE));
        // Back shows the row.
        assert!(rig.cx(|cx| s.back(cx)));
        rig.advance(&mut s, 0.3);
        assert!(rig.says(TITLE) && rig.says("Robin"));
    }

    #[test]
    fn nothing_listed_offers_retry() {
        let server = fixtures();
        let mut rig = Rig::signed_in(&server, ALEX);
        server.set_mode(pley_fixtures::Mode::Down).unwrap();
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        assert!(rig.until(&mut s, |rig| rig.says(RETRY)), "Retry never showed");
        // Asking again leaves Retry where it is, with focus.
        rig.press(&mut s, Key::Ok);
        assert!(rig.says(RETRY));
        assert_eq!(rig.focused(), Some(Id::ROOT.with("buttons").with("retry")));
        server.set_mode(pley_fixtures::Mode::Normal).unwrap();
        rig.press(&mut s, Key::Ok);
        assert!(rig.until(&mut s, |rig| rig.says("Robin")), "Retry didn't ask again");
        assert!(!rig.says(RETRY));
    }

    #[test]
    fn back_while_a_pin_is_checked_keeps_the_switch() {
        let server = fixtures();
        let mut rig = Rig::signed_in(&server, ALEX);
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        assert!(rig.until(&mut s, |rig| rig.says("Robin")));
        rig.press(&mut s, Key::Right);
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.3);
        digits(&mut rig, &mut s, pley_fixtures::ids::PIN);
        // The server answers late; Back comes first and shows the row, Sam dimmed.
        server.set_mode(pley_fixtures::Mode::Slow).unwrap();
        rig.press(&mut s, Key::Ok);
        assert!(s.busy.is_some(), "the PIN wasn't sent");
        assert!(rig.cx(|cx| s.back(cx)));
        rig.advance(&mut s, 0.3);
        assert!(!s.modal() && rig.says(TITLE));
        // The switch the server made still lands.
        assert!(rig.until(&mut s, |rig| rig.reset_to(&Route::Home)), "the switch was dropped");
        assert_eq!(rig.state.profile(), Some(SAM));
    }

    #[test]
    fn once_approved_back_waits_and_the_approved_profile_goes_home() {
        use pley_fixtures::DeviceApproval;
        let server = fixtures();
        server.set_device_approval(DeviceApproval::Never);
        let mut rig = Rig::signed_in(&server, ALEX);
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, Some(ROBIN)));
        assert!(rig.until(&mut s, |rig| rig.says(WAITING)), "no code came");
        server.set_sign_in_as(ROBIN);
        server.set_device_approval(DeviceApproval::Allow);
        let settling = |s: &ProfilesScreen| matches!(&s.step, Step::Approve(a) if a.flow.settling());
        let start = std::time::Instant::now();
        while !settling(&s) && start.elapsed() < std::time::Duration::from_secs(10) {
            rig.until(&mut s, |_| true);
        }
        // The new sign-in is being kept: Back neither leaves nor drops it.
        assert!(settling(&s) && rig.cx(|cx| s.back(cx)) && settling(&s));
        assert!(rig.until(&mut s, |rig| rig.reset_to(&Route::Home)), "Home never opened");
        assert_eq!(rig.state.profile(), Some(ROBIN));
    }

    #[test]
    fn a_refusal_without_a_reason_lets_only_the_current_profile_through() {
        let refusal = || {
            let oauth = pley_core::model::OAuthError { error: "invalid_request".into(), ..Default::default() };
            Promise::resolved(Err(AuthError::Refused { status: 400, oauth: Some(oauth) }))
        };
        let mut rig = Rig::new();
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        s.started = true;
        s.loading = false;
        s.profiles = vec![profile(1, ProfileNeeds::Pin, true), profile(2, ProfileNeeds::Pin, false)];
        rig.frame(&mut s, Vec::new());
        // Another profile's PIN refused so: a refusal, not a way in.
        let other = s.profiles[1].clone();
        s.go(Next::Pin(PinStep::new(other.clone(), false)));
        s.busy = Some(Switch { profile: other, confirm: false, pin: true, promise: refusal() });
        rig.advance(&mut s, 0.3);
        assert!(rig.says(NOT_ALLOWED) && rig.says("Enter P2's PIN"));
        assert!(!rig.reset_to(&Route::Home) && !rig.state.profile_gate_passed());
        // The current profile's PIN at a cold start: the server can't check it, and the person
        // goes on.
        let me = s.profiles[0].clone();
        s.busy = Some(Switch { profile: me, confirm: true, pin: true, promise: refusal() });
        rig.frame(&mut s, Vec::new());
        assert!(rig.reset_to(&Route::Home) && rig.state.profile_gate_passed());
    }

    #[test]
    fn the_pin_route_does_what_picking_does_for_a_profile_without_a_pin() {
        let server = fixtures();
        server.set_device_approval(pley_fixtures::DeviceApproval::Never);
        let mut rig = Rig::signed_in(&server, ALEX);
        // Robin needs a password: the approval, not a PIN pad.
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, Some(ROBIN)));
        assert!(rig.until(&mut s, |rig| rig.says(APPROVE_TITLE)), "the approval never showed");
        assert!(!rig.says("Enter Robin's PIN"));
        drop(s);
        // Who the app is already, without a PIN: Home.
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, Some(ALEX)));
        assert!(rig.until(&mut s, |rig| rig.reset_to(&Route::Home)), "Home never opened");
        assert!(!rig.says("Enter Alex's PIN"));
    }

    #[test]
    fn a_locked_profile_and_a_sign_in_that_cant_switch() {
        let mut rig = Rig::new();
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        s.started = true;
        s.loading = false;
        s.profiles = vec![profile(1, ProfileNeeds::None, true), profile(4, ProfileNeeds::Locked, false)];
        rig.frame(&mut s, Vec::new());
        rig.press(&mut s, Key::Right);
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.3);
        assert!(rig.says(LOCKED) && rig.says(USE_ANOTHER) && !rig.says(PIN_HINT));
        assert_eq!(rig.focused(), Some(Id::ROOT.with("another")), "the keypad is off");
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.3);
        assert!(matches!(s.step, Step::Pick) && rig.says(TITLE));

        // Without the profiles scope: "Sign in as another profile", approved on a phone.
        let mut s = rig.cx(|cx| ProfilesScreen::new(cx, None));
        s.started = true;
        s.loading = false;
        s.can_switch = false;
        rig.frame(&mut s, Vec::new());
        assert!(rig.says(ANOTHER_TITLE) && rig.says(RETRY));
        assert_eq!(rig.focused(), Some(Id::ROOT.with("buttons").with("retry")));
        rig.press(&mut s, Key::Down);
        rig.press(&mut s, Key::Ok);
        rig.advance(&mut s, 0.3);
        assert!(rig.says(ANOTHER_BODY) && rig.says(USE_ANOTHER));
        let Step::Approve(step) = &s.step else { panic!("no approval") };
        assert_eq!(*step.flow.purpose(), Purpose::Another { previous: None });
    }
}
