//! The focus look every control shares: one progress value from rest (0) to focused (1), 150 ms in
//! and 120 ms out (`Motion::FOCUS`, `Motion::UNFOCUS`), the press, and the colours, edge and scale
//! that follow them (`theme::focus`; `TvTile.kt`, `TvButtons.kt`, `PleyMotion.kt`).

use std::cell::Cell;

use pley_base::{Color, Point, Rect, TextLayout, TextOpts, TextStyle};
use pley_ui::theme::focus::ControlLook;
use pley_ui::{Easing, FocusOpts, Id, Motion, Response, Ui};

/// Rows and grids sliding to keep focus in place: 220 ms `cubic-bezier(0.2, 0, 0, 1)`, as the TV
/// app's rows and grids slide (`packages/tv/src/ui/kit/row.tsx`, `grid.tsx`). Snaps while a key is held and
/// with Reduce motion.
pub const SCROLL: Motion = Motion::new(0.220, Easing::cubic(0.2, 0.0, 0.0, 1.0));

/// Pages of rows bringing the focused row to its line: 240 ms with the same curve (the TV app's
/// `SectionPage`).
pub const PAGE_SCROLL: Motion = Motion::new(0.240, Easing::cubic(0.2, 0.0, 0.0, 1.0));

/// A press flashing a key of the pads and the keyboard: 100 ms (`TvKeypad.kt`).
pub(crate) const FLASH: Motion = Motion::new(0.100, Easing::LINEAR);

/// How a control shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shade {
    /// How far focus has come, 0 to 1.
    pub progress: f32,
    pub fill: Color,
    pub content: Color,
    /// The 1 dp edge at rest, fading as focus comes.
    pub edge: Option<Color>,
    /// The control's scale about its centre.
    pub scale: f32,
}

impl Shade {
    /// A control at rest, as a disabled one is drawn.
    pub fn rest(look: &ControlLook) -> Shade {
        Shade { progress: 0.0, fill: look.fill, content: look.content, edge: look.edge, scale: 1.0 }
    }
}

/// How far focus has come on `id`, heading for 1 while it is focused.
pub fn focus_progress(ui: &mut Ui, id: Id, focused: bool) -> f32 {
    let (target, motion) = if focused { (1.0, Motion::FOCUS) } else { (0.0, Motion::UNFOCUS) };
    ui.animate(id.with("kit.focus"), target, motion)
}

/// How far OK (or the pointer's button) has pressed `id`, 100 ms each way.
pub fn press_progress(ui: &mut Ui, id: Id, pressed: bool) -> f32 {
    ui.animate(id.with("kit.press"), if pressed { 1.0 } else { 0.0 }, Motion::PRESS)
}

/// The look of a control with `look` that answered `r` this frame: the fill and content colours
/// blend to Accent and OnAccent as focus comes, to AccentBright while pressed, the edge fades, and
/// the scale grows to the look's focus scale (back to 1 while pressed, as tv-material's
/// `pressedScale = 1f` in `TvButtons.kt`).
pub fn shade(ui: &mut Ui, look: &ControlLook, r: &Response) -> Shade {
    let p = focus_progress(ui, r.id, r.focused);
    let q = press_progress(ui, r.id, r.pressed);
    let fill = mix(mix(look.fill, look.focus_fill, p), look.pressed_fill, q);
    let content = mix(look.content, look.focus_content, p.max(q));
    let edge = look.edge.map(|e| e.alpha(1.0 - p.max(q)));
    let focus_scale = 1.0 + (look.focus_scale - 1.0) * p;
    Shade { progress: p, fill, content, edge, scale: focus_scale + (1.0 - focus_scale) * q }
}

/// `a` blended `t` of the way to `b`; a transparent end takes the other's colour at no alpha, so a
/// fill fading in from nothing never passes through grey.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    match (a.is_transparent(), b.is_transparent()) {
        (true, true) => Color::TRANSPARENT,
        (true, false) => b.alpha(t),
        (false, true) => a.alpha(1.0 - t),
        (false, false) => a.lerp(b, t),
    }
}

/// What a control's response says beyond focus and OK.
pub trait ResponseExt {
    /// OK held 500 ms or the menu key: open the element's actions (`menuKey` in `TvTile.kt`).
    fn actions(&self) -> bool;
}

impl ResponseExt for Response {
    fn actions(&self) -> bool {
        self.long_pressed || self.menu
    }
}

thread_local! {
    /// How many [`inert`] scopes are open on this thread (the interface's).
    static INERT: Cell<u32> = const { Cell::new(0) };
}

/// Closes an [`inert`] scope, even if what it draws panics.
struct InertScope;

impl Drop for InertScope {
    fn drop(&mut self) {
        INERT.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Draws `f` with every kit control in it drawn as it looks but not registered: nothing in it can
/// take focus, be clicked or claim the first focus. Panels and dialogs draw their content this
/// way while they slide or fade out, so focus goes back to the page at once.
pub fn inert<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
    INERT.with(|d| d.set(d.get() + 1));
    let _scope = InertScope;
    f(ui)
}

/// Whether kit controls are being drawn [`inert`].
pub fn is_inert() -> bool {
    INERT.with(|d| d.get() > 0)
}

/// Registers a kit control at `rect` (unless drawn [`inert`]). When it is selected, held or sent
/// the menu key, it asks for one more frame: the screen changes what it shows in answer, after
/// this frame was drawn.
pub(crate) fn register(ui: &mut Ui, id: Id, rect: Rect, opts: FocusOpts) -> Response {
    if is_inert() {
        return unregistered(id);
    }
    let r = ui.focusable_with(id, rect, opts);
    if r.activated || r.long_pressed || r.menu {
        ui.request_repaint();
    }
    r
}

/// The response of something drawn but not registered: a disabled control, or one drawn inert.
pub(crate) fn unregistered(id: Id) -> Response {
    Response {
        id,
        focused: false,
        gained: false,
        activated: false,
        clicked: false,
        long_pressed: false,
        menu: false,
        pressed: false,
        hovered: false,
        pointer: None,
    }
}

/// One line, never cut: a label measured at its own width.
pub(crate) fn free_line() -> TextOpts {
    TextOpts { max_width: None, max_lines: 1, ellipsis: false, ..TextOpts::default() }
}

/// One line cut with "…" at `width` when `width` is given.
pub(crate) fn line_within(width: Option<f32>) -> TextOpts {
    match width {
        Some(w) => TextOpts::line(w.max(0.0)),
        None => free_line(),
    }
}

/// Draws `layout` centred vertically in a box from `top` that is `height` tall, from `x`.
pub(crate) fn text_mid(ui: &mut Ui, x: f32, top: f32, height: f32, layout: std::sync::Arc<TextLayout>, color: Color) {
    let y = top + (height - layout.size.h) / 2.0;
    ui.draw_text(Point::new(x, y), layout, color);
}

/// Draws `text` in `style` centred in `rect` on one line (cut at its width).
pub(crate) fn text_centred(ui: &mut Ui, rect: Rect, text: &str, style: &TextStyle, color: Color) {
    let layout = ui.layout(text, style, &TextOpts::line(rect.w.max(1.0)));
    let x = rect.x + (rect.w - layout.size.w) / 2.0;
    text_mid(ui, x, rect.y, rect.h, layout, color);
}

/// A rectangle's outline drawn inside its edge: `width` thick, following `radius`.
pub(crate) fn inner_stroke(ui: &mut Ui, rect: Rect, color: Color, radius: f32, width: f32) {
    let half = width / 2.0;
    ui.stroke(rect.expand(-half), color, (radius - half).max(0.0), width);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_ui::theme;

    #[test]
    fn mixing_from_transparent_keeps_the_colour() {
        let m = mix(Color::TRANSPARENT, theme::ACCENT, 0.5);
        assert_eq!((m.r, m.g, m.b), (theme::ACCENT.r, theme::ACCENT.g, theme::ACCENT.b));
        assert_eq!(m.a, 128);
        assert_eq!(mix(theme::SURFACE3, theme::ACCENT, 1.0), theme::ACCENT);
        assert_eq!(mix(theme::SURFACE3, theme::ACCENT, 0.0), theme::SURFACE3);
    }

    #[test]
    fn rest_shade_is_the_look_at_rest() {
        let s = Shade::rest(&theme::focus::PILL);
        assert_eq!((s.fill, s.content, s.edge, s.scale), (theme::PILL_REST, theme::FG, Some(theme::CONTROL), 1.0));
    }
}
