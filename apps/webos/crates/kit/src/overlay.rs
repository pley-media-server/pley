//! What opens over a page: side panels, dialogs and a title's action sheet.
//!
//! A side panel (`TvSidePanel` in `TvSurfaces.kt`; the player's `TvPlayerPanels.kt`) is 360 dp
//! wide (440 for the player's Info) at the right edge, full height: Surface2 with its
//! left corners rounded 8 and a 1 dp Surface4 line along its top, over a scrim of Bg at 60 %. It
//! slides in from the right in 200 ms while the scrim fades (both in place with Reduce motion),
//! keeps focus inside until it closes, and closes on Back or a click on the scrim; focus then goes
//! back to what opened it. Its content sits 24 in from its left, 48 from its right and 27 from its
//! top and bottom.

use pley_base::{Icon, Key, Point, Rect, Size, TextOpts};
use pley_ui::theme::{self, radius};
use pley_ui::{Group, Id, Motion, Ui};

use crate::button::{Button, bar_group};
use crate::list::ListRow;
use crate::look::{inert, is_inert};

/// Bg at 60 % over the whole screen, times `alpha` (a fading scrim).
pub fn scrim_fill(ui: &mut Ui, alpha: f32) {
    let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
    ui.fill(full, theme::BG.alpha(0.6 * alpha.clamp(0.0, 1.0)), 0.0);
}

/// A side panel.
#[derive(Clone, Copy, Debug)]
pub struct SidePanel {
    width: f32,
    back_closes: bool,
}

/// What a panel or dialog did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PanelResponse {
    /// Back or a click on the scrim asked it to close: the screen stops opening it.
    pub closed: bool,
    /// How far it is in, 0 to 1 (it is drawn while above 0).
    pub progress: f32,
}

impl Default for SidePanel {
    fn default() -> SidePanel {
        SidePanel::new()
    }
}

impl SidePanel {
    /// A panel's width.
    pub const WIDTH: f32 = 360.0;
    /// The player's Info panel's width (`INFO_WIDTH`).
    pub const INFO_WIDTH: f32 = 440.0;
    /// Its content's inset from its left edge.
    pub const PAD_START: f32 = 24.0;
    /// From its right edge.
    pub const PAD_END: f32 = 48.0;

    /// A 360 dp panel that closes on Back.
    pub fn new() -> SidePanel {
        SidePanel { width: SidePanel::WIDTH, back_closes: true }
    }

    /// The 440 dp panel of the player's Info.
    pub fn wide() -> SidePanel {
        SidePanel { width: SidePanel::INFO_WIDTH, ..SidePanel::new() }
    }

    /// Whether the panel claims Back to close itself (on by default; a screen that closes it from
    /// its own Back handling turns this off).
    pub fn back_closes(self, on: bool) -> SidePanel {
        SidePanel { back_closes: on, ..self }
    }

    /// Where the content goes, with the panel `progress` of the way in.
    pub fn content_rect(&self, progress: f32) -> Rect {
        let x = theme::CANVAS_W - self.width * progress;
        Rect::new(
            x + SidePanel::PAD_START,
            theme::SAFE_Y,
            self.width - SidePanel::PAD_START - SidePanel::PAD_END,
            theme::CANVAS_H - 2.0 * theme::SAFE_Y,
        )
    }

    /// Draws the panel while it is `open` or sliding out; `content` draws inside it (given the
    /// content's rectangle), within a focus trap while open. Call it every frame, open or not, so
    /// it can slide out. One panel open at a time: the next one replaces it.
    pub fn show(&self, ui: &mut Ui, id: Id, open: bool, content: impl FnOnce(&mut Ui, Rect)) -> PanelResponse {
        let p = ui.animate_from(id.with("kit.open"), 0.0, if open { 1.0 } else { 0.0 }, Motion::OVERLAY);
        let mut out = PanelResponse { closed: false, progress: p };
        if p <= 0.0 && !open {
            return out;
        }
        scrim_fill(ui, p);
        let live = open && !is_inert();
        if live {
            let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
            out.closed |= ui.clickable(id.with("kit.scrim"), full).activated;
        }
        let x = theme::CANVAS_W - self.width * p;
        // Past the right edge by its radius, so only the left corners show rounded.
        ui.fill(Rect::new(x, 0.0, self.width + radius::DIALOG, theme::CANVAS_H), theme::SURFACE2, radius::DIALOG);
        ui.fill(Rect::new(x + radius::DIALOG, 0.0, self.width - radius::DIALOG, 1.0), theme::SURFACE4, 0.0);
        let area = self.content_rect(p);
        if live {
            ui.group(id, Group::column().trap(), |ui| content(ui, area));
            if self.back_closes && ui.take_key(Key::Back) {
                out.closed = true;
            }
        } else {
            // Sliding out: drawn as it was, out of focus's way.
            inert(ui, |ui| content(ui, area));
        }
        if out.closed {
            ui.request_repaint();
        }
        out
    }
}

/// A button of a [`Dialog`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DialogButton<'a> {
    pub label: &'a str,
    /// Destructive: its label in DangerText.
    pub danger: bool,
}

impl<'a> DialogButton<'a> {
    /// A button saying `label`.
    pub fn new(label: &'a str) -> DialogButton<'a> {
        DialogButton { label, danger: false }
    }

    /// A destructive one.
    pub fn danger(label: &'a str) -> DialogButton<'a> {
        DialogButton { label, danger: true }
    }
}

/// What a dialog did this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DialogResponse {
    /// The button chosen.
    pub chosen: Option<usize>,
    /// Back, or a click outside it.
    pub dismissed: bool,
    /// How far it is in, 0 to 1.
    pub progress: f32,
}

/// A dialog in the middle of the screen (`TvDialog` in `OnboardingKit.kt`, `TvTextDialog` in
/// `TvItemParts.kt`; the TV app's `kit/panel.tsx`): Surface2 with 8 dp corners, 320 to 560 wide, over
/// the scrim; its title in `Tv.Title`, its text under it, and text buttons at its right 12 apart.
/// It fades in 200 ms, keeps focus until it closes, and Back or a click outside dismisses it.
#[derive(Clone, Copy, Debug)]
pub struct Dialog<'a> {
    title: &'a str,
    body: &'a str,
    buttons: &'a [DialogButton<'a>],
    initial: usize,
    reading: bool,
}

const CLOSE: [DialogButton<'static>; 1] = [DialogButton { label: "Close", danger: false }];

impl<'a> Dialog<'a> {
    /// The least width.
    pub const MIN_WIDTH: f32 = 320.0;
    /// The greatest width.
    pub const MAX_WIDTH: f32 = 560.0;

    /// A dialog titled `title` (padding 24; its text in `Tv.Body` Muted, 8 below the title).
    pub fn new(title: &'a str) -> Dialog<'a> {
        Dialog { title, body: "", buttons: &[], initial: 0, reading: false }
    }

    /// A text to read (a title's full summary): padding 28, the text in `Tv.BodyLarge` at Fg 85 %
    /// 16 below the title, keeping its line breaks, and a Close button.
    pub fn reading(title: &'a str, text: &'a str) -> Dialog<'a> {
        Dialog { title, body: text, buttons: &CLOSE, initial: 0, reading: true }
    }

    /// Its text.
    pub fn body(self, body: &'a str) -> Dialog<'a> {
        Dialog { body, ..self }
    }

    /// Its buttons, left to right.
    pub fn buttons(self, buttons: &'a [DialogButton<'a>]) -> Dialog<'a> {
        Dialog { buttons, ..self }
    }

    /// The button focused first.
    pub fn initial(self, index: usize) -> Dialog<'a> {
        Dialog { initial: index, ..self }
    }

    fn padding(&self) -> f32 {
        if self.reading { 28.0 } else { 24.0 }
    }

    fn body_style(&self) -> (pley_base::TextStyle, pley_base::Color, f32) {
        if self.reading { (theme::BODY_LARGE, theme::SUMMARY, 16.0) } else { (theme::BODY, theme::MUTED, 8.0) }
    }

    /// Where the dialog sits: centred, as wide as its content within 320 to 560.
    pub fn rect(&self, ui: &Ui) -> Rect {
        let pad = self.padding();
        let (style, _, gap) = self.body_style();
        let inner_max = Dialog::MAX_WIDTH - 2.0 * pad;
        let opts = TextOpts { max_lines: 0, ..TextOpts::wrap(inner_max, 0) };
        let title = ui.measure(self.title, &theme::TITLE, &TextOpts::wrap(inner_max, 0));
        let body = if self.body.is_empty() { Size::ZERO } else { ui.measure(self.body, &style, &opts) };
        let buttons: f32 = self.buttons.iter().map(|b| Button::text(b.label).size(ui).w).sum::<f32>()
            + 12.0 * self.buttons.len().saturating_sub(1) as f32;
        let w = (title.w.max(body.w).max(buttons) + 2.0 * pad).clamp(Dialog::MIN_WIDTH, Dialog::MAX_WIDTH);
        let inner = w - 2.0 * pad;
        let title_h = ui.measure(self.title, &theme::TITLE, &TextOpts::wrap(inner, 0)).h;
        let body_h =
            if self.body.is_empty() { 0.0 } else { gap + ui.measure(self.body, &style, &TextOpts::wrap(inner, 0)).h };
        let buttons_h = if self.buttons.is_empty() { 0.0 } else { 16.0 + theme::BUTTON_HEIGHT };
        let h = pad + title_h + body_h + buttons_h + pad;
        Rect::new((theme::CANVAS_W - w) / 2.0, (theme::CANVAS_H - h) / 2.0, w, h)
    }

    /// Draws the dialog while it is `open` or fading out (call it every frame).
    pub fn show(&self, ui: &mut Ui, id: Id, open: bool) -> DialogResponse {
        let p = ui.animate_from(id.with("kit.open"), 0.0, if open { 1.0 } else { 0.0 }, Motion::OVERLAY);
        let mut out = DialogResponse { progress: p, ..DialogResponse::default() };
        if p <= 0.0 && !open {
            return out;
        }
        scrim_fill(ui, p);
        let rect = self.rect(ui);
        let live = open && !is_inert();
        if live {
            // Audio Guidance reads the dialog as it opens, then its focused button.
            ui.announce_once(id.with("kit.read"), || crate::look::spoken(&[self.title, self.body]));
            let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
            out.dismissed |= ui.clickable(id.with("kit.outside"), full).activated;
        }
        let pad = self.padding();
        let (style, color, gap) = self.body_style();
        let inner = rect.w - 2.0 * pad;
        ui.opacity(p, |ui| {
            ui.fill(rect, theme::SURFACE2, radius::DIALOG);
            // Clicks on the dialog itself aren't clicks outside it.
            if live {
                ui.clickable(id.with("kit.card"), rect);
            }
            let mut y = rect.y + pad;
            y +=
                ui.text(Point::new(rect.x + pad, y), self.title, &theme::TITLE, &TextOpts::wrap(inner, 0), theme::FG).h;
            if !self.body.is_empty() {
                y += gap;
                ui.text(Point::new(rect.x + pad, y), self.body, &style, &TextOpts::wrap(inner, 0), color);
            }
            if self.buttons.is_empty() {
                return;
            }
            let by = rect.bottom() - pad - theme::BUTTON_HEIGHT;
            let buttons = |ui: &mut Ui| {
                // A trap with nothing in it would hold focus nowhere: none while drawn inert.
                let group = if is_inert() { Group::free() } else { bar_group().trap() };
                ui.group(id, group, |ui| {
                    let widths: Vec<f32> = self.buttons.iter().map(|b| Button::text(b.label).size(ui).w).collect();
                    let total: f32 = widths.iter().sum::<f32>() + 12.0 * (widths.len() - 1) as f32;
                    let mut x = rect.right() - pad - total;
                    let mut chosen = None;
                    for (i, b) in self.buttons.iter().enumerate() {
                        let bid = ui.id(i);
                        if i == self.initial {
                            ui.initial_focus(bid);
                        }
                        let mut button = Button::text(b.label);
                        if b.danger {
                            button = button.color(theme::DANGER_TEXT);
                        }
                        if button.show(ui, bid, Point::new(x, by)).activated {
                            chosen = Some(i);
                        }
                        x += widths[i] + 12.0;
                    }
                    chosen
                })
            };
            // Fading out, it no longer keeps focus.
            out.chosen = if live { buttons(ui) } else { inert(ui, buttons) };
        });
        if live && ui.take_key(Key::Back) {
            out.dismissed = true;
        }
        if out.dismissed {
            ui.request_repaint();
        }
        out
    }
}

/// One action of an [`ActionSheet`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Action<'a> {
    pub label: &'a str,
    pub icon: Option<Icon>,
    pub danger: bool,
}

impl<'a> Action<'a> {
    /// An action saying `label`.
    pub fn new(label: &'a str) -> Action<'a> {
        Action { label, icon: None, danger: false }
    }

    /// With an icon (on the full-screen sheet only Play and Resume carry one).
    pub fn icon(self, icon: Icon) -> Action<'a> {
        Action { icon: Some(icon), ..self }
    }

    /// Destructive.
    pub fn danger(self) -> Action<'a> {
        Action { danger: true, ..self }
    }
}

/// A title's actions as a page of their own (`TvItemActionsScreen.kt`, `ItemActions.kt`): over
/// the title's backdrop (drawn by the screen), at x 96, y 58, 464 wide, the title in
/// `Tv.PageTitle` (two lines), its subtitle in `Tv.Body` Muted, 24 dp, then one list row per
/// action, the first focused.
#[derive(Clone, Copy, Debug)]
pub struct ActionSheet<'a> {
    title: &'a str,
    subtitle: &'a str,
    actions: &'a [Action<'a>],
}

impl<'a> ActionSheet<'a> {
    /// The sheet's place.
    pub const ORIGIN: Point = Point::new(96.0, 58.0);
    /// Its width.
    pub const WIDTH: f32 = 464.0;

    /// The actions of the title `title`.
    pub fn new(title: &'a str, actions: &'a [Action<'a>]) -> ActionSheet<'a> {
        ActionSheet { title, subtitle: "", actions }
    }

    /// The line under the title ("S2 · E5 · Office Olympics", "2019").
    pub fn subtitle(self, subtitle: &'a str) -> ActionSheet<'a> {
        ActionSheet { subtitle, ..self }
    }

    /// Draws the sheet; returns the action chosen.
    pub fn show(&self, ui: &mut Ui, id: Id) -> Option<usize> {
        let at = ActionSheet::ORIGIN;
        let w = ActionSheet::WIDTH;
        let mut y = at.y;
        y += ui.text(Point::new(at.x, y), self.title, &theme::PAGE_TITLE, &TextOpts::wrap(w, 2), theme::FG).h;
        if !self.subtitle.is_empty() {
            y += ui.text(Point::new(at.x, y), self.subtitle, &theme::BODY, &TextOpts::line(w), theme::MUTED).h;
        }
        y += 24.0;
        let mut chosen = None;
        ui.group(id, Group::column(), |ui| {
            for (i, a) in self.actions.iter().enumerate() {
                let rid = ui.id(i);
                if i == 0 {
                    ui.initial_focus(rid);
                }
                let mut row = ListRow::new(a.label).danger(a.danger);
                if let Some(icon) = a.icon {
                    row = row.icon(icon);
                }
                let h = row.height(ui, w);
                if row.show(ui, rid, Point::new(at.x, y), w).activated {
                    chosen = Some(i);
                }
                y += h + 2.0;
            }
        });
        chosen
    }
}
