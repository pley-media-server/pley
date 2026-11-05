//! What the shell draws over every page: toasts, the offline notice, the curtain between pages and
//! the dimmer of an idle screen.

use pley_base::{Color, Icon, Point, Rect, Size, TextOpts};
use pley_ui::theme;
use pley_ui::{Easing, Id, Motion, Ui};

use crate::cx::ToastRequest;
use crate::state::OFFLINE_TEXT;

/// How long a toast shows (`kit/toast.tsx`; Android's "Saved" pill).
pub const TOAST_SECONDS: f64 = 3.5;
/// How long an error's toast shows.
pub const ERROR_TOAST_SECONDS: f64 = 6.0;
/// The widest a toast grows.
pub const TOAST_MAX_WIDTH: f32 = 420.0;
/// Where toasts and the notice sit: their right edge 48 dp from the screen's, 27 from the top.
pub const TOAST_TOP: f32 = theme::SAFE_Y;
/// In the players they sit lower, under the top of the controls.
pub const TOAST_TOP_PLAYER: f32 = 79.0;
const TOAST_RIGHT: f32 = theme::CANVAS_W - theme::SAFE_X;
const PAD_X: f32 = 20.0;
const PAD_Y: f32 = 10.0;
const NOTICE_ICON: f32 = 20.0;
const NOTICE_GAP: f32 = 10.0;

/// The screen dims after this long without a key or the pointer, outside playback: webOS never
/// blanks a native app in the foreground, so the app protects the
/// panel itself. The time and depth are Android's Now Playing dimming (120 s, to 30 %).
pub const DIM_AFTER: f64 = 120.0;
/// How much of the picture the dimmer takes away.
pub const DIM_ALPHA: f32 = 0.7;
/// The dimmer fades in slowly, so it isn't mistaken for the app doing something.
const DIM_FADE: Motion = Motion::new(1.0, Easing::LINEAR);

fn toast_id() -> Id {
    Id::new("pley.toast")
}

#[derive(Debug)]
struct Toast {
    id: u64,
    text: String,
    error: bool,
    until: f64,
    /// The first of a series fades in; one replacing another shows at once.
    fades_in: bool,
}

/// The toast showing, if any.
#[derive(Debug, Default)]
pub struct Toasts {
    current: Option<Toast>,
    next: u64,
}

impl Toasts {
    /// Shows `req` from `now`, replacing what shows.
    pub fn show(&mut self, req: ToastRequest, now: f64) {
        let fades_in = self.current.as_ref().is_none_or(|t| now >= t.until);
        self.next += 1;
        let seconds = if req.error { ERROR_TOAST_SECONDS } else { TOAST_SECONDS };
        self.current = Some(Toast { id: self.next, text: req.text, error: req.error, until: now + seconds, fades_in });
    }

    /// What the toast showing says.
    pub fn text(&self) -> Option<&str> {
        self.current.as_ref().map(|t| t.text.as_str())
    }

    /// When the toast showing starts fading out.
    pub fn next_due(&self) -> Option<f64> {
        self.current.as_ref().map(|t| t.until)
    }

    /// Draws the toast with its top at `top`.
    pub fn ui(&mut self, ui: &mut Ui, top: f32) {
        let Some(t) = &self.current else { return };
        let now = ui.time();
        let id = toast_id().with(t.id);
        let target = if now < t.until { 1.0 } else { 0.0 };
        let a = if t.fades_in {
            ui.animate_from(id, 0.0, target, Motion::REVEAL)
        } else {
            ui.animate(id, target, Motion::REVEAL)
        };
        if target == 0.0 && a <= 0.0 {
            self.current = None;
            return;
        }
        if target > 0.0 {
            ui.repaint_after(t.until - now);
            ui.announce_once(id.with("read"), || t.text.clone());
        }
        let color = if t.error { theme::DANGER_TEXT } else { theme::FG };
        let opts = TextOpts::wrap(TOAST_MAX_WIDTH - 2.0 * PAD_X, 3);
        let layout = ui.layout(&t.text, &theme::LABEL, &opts);
        let size = Size::new(layout.size.w + 2.0 * PAD_X, layout.size.h + 2.0 * PAD_Y);
        let rect = Rect::new(TOAST_RIGHT - size.w, top, size.w, size.h);
        ui.opacity(a, |ui| {
            ui.fill(rect, theme::SURFACE3, theme::radius::CONTROL);
            ui.draw_text(Point::new(rect.x + PAD_X, rect.y + PAD_Y), layout, color);
        });
    }
}

/// Draws "Pley is offline" at the top right while `showing`, fading in and out; never
/// focusable. Returns the bottom of what it drew (`top` when nothing shows).
pub fn offline_notice(ui: &mut Ui, showing: bool, top: f32) -> f32 {
    let a = ui.animate(Id::new("pley.offline-notice"), if showing { 1.0 } else { 0.0 }, Motion::REVEAL);
    if a <= 0.0 {
        return top;
    }
    let text = ui.layout(OFFLINE_TEXT, &theme::LABEL, &TextOpts::line(TOAST_MAX_WIDTH));
    let inner = text.size.h.max(NOTICE_ICON);
    let width = 2.0 * PAD_X + NOTICE_ICON + NOTICE_GAP + text.size.w;
    let rect = Rect::new(TOAST_RIGHT - width, top, width, inner + 2.0 * PAD_Y);
    ui.opacity(a, |ui| {
        ui.fill(rect, theme::SURFACE3, theme::radius::CONTROL);
        let icon = Rect::new(rect.x + PAD_X, rect.y + (rect.h - NOTICE_ICON) / 2.0, NOTICE_ICON, NOTICE_ICON);
        ui.icon(icon, Icon::ServerOff, theme::MUTED);
        let at = Point::new(icon.right() + NOTICE_GAP, rect.y + (rect.h - text.size.h) / 2.0);
        ui.draw_text(at, text, theme::FG);
    });
    rect.bottom() + theme::space::S
}

/// The curtain over a page that just came on top: Bg snapped opaque, fading out over 110 ms
/// (`Motion::CURTAIN`); none for the first page or with Reduce motion. Whether it still shows.
pub fn curtain(ui: &mut Ui, entry: u64) -> bool {
    let a = ui.animate_from(Id::new("pley.curtain").with(entry), 1.0, 0.0, Motion::CURTAIN);
    if a <= 0.0 {
        return false;
    }
    ui.fill(Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H), theme::alpha(theme::BG, a), 0.0);
    true
}

/// Draws the idle dimmer at its depth for `dimmed`.
pub fn dimmer(ui: &mut Ui, dimmed: bool) {
    // It fades in slowly and goes at once with the key that wakes it.
    let (target, motion) = if dimmed { (DIM_ALPHA, DIM_FADE) } else { (0.0, Motion::REVEAL) };
    let a = ui.animate(Id::new("pley.dimmer"), target, motion);
    if a > 0.0 {
        ui.fill(Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H), Color::BLACK.alpha(a), 0.0);
    }
}
