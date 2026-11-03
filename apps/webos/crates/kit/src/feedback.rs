//! What tells people where things stand: toasts, the loading ring, progress bars, skeletons, the
//! messages pages show when they are empty or failed, and the info block's fade.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use pley_base::{Align, Color, Icon, Point, Rect, Size, TextOpts};
use pley_ui::theme::{self, radius};
use pley_ui::{Id, Motion, Response, Ui};

use crate::button::{Button, bar_group};
use crate::raster;
use crate::tile::TileKind;

/// Where a toast shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToastPlacement {
    /// Top right inside the safe area (right edge 912, top 27), padding 20 × 12: item pages,
    /// libraries, playlists, settings' "Saved" (`TvToastHost`).
    TopRight,
    /// Bottom left at the gutter (x 96, bottom edge 513), padding 20 × 10: Home (`TvHomeToast`).
    BottomLeft,
    /// The player's: top right, padding 20 × 10, below the controls' top bar while it shows (79).
    Player { controls: bool },
}

/// The toast showing, if any (owned by whoever shows toasts: the shell, the player).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToastState {
    text: String,
    error: bool,
    at: f64,
    live: bool,
}

/// How long a toast stays (`TvToastHost`): 3.5 s, an error 6 s.
pub const TOAST_SECONDS: f64 = 3.5;
/// An error toast's time.
pub const ERROR_TOAST_SECONDS: f64 = 6.0;
const TOAST_MAX_WIDTH: f32 = 420.0;

impl ToastState {
    /// Shows `text` from time `at` (the frame's time), replacing any toast showing; an `error`
    /// is in DangerText and stays longer.
    pub fn show(&mut self, text: impl Into<String>, error: bool, at: f64) {
        *self = ToastState { text: text.into(), error, at, live: true };
    }

    /// Takes the toast away.
    pub fn clear(&mut self) {
        self.live = false;
    }

    /// The text showing, if a toast shows.
    pub fn text(&self) -> Option<&str> {
        self.live.then_some(self.text.as_str())
    }

    fn duration(&self) -> f64 {
        if self.error { ERROR_TOAST_SECONDS } else { TOAST_SECONDS }
    }
}

/// Draws the toast in `state` at `placement`: Surface3 with 6 dp corners, at most 420 wide, up to
/// three lines of `Tv.Label` in Fg (DangerText for errors), fading in and out over 150 ms.
pub fn toast(ui: &mut Ui, state: &mut ToastState, placement: ToastPlacement) {
    if !state.live {
        return;
    }
    let age = ui.time() - state.at;
    let dur = state.duration();
    if age >= dur {
        state.live = false;
        return;
    }
    let fade = Motion::REVEAL.duration as f64;
    let alpha = if age < fade {
        Motion::REVEAL.progress(age)
    } else if age > dur - fade {
        ((dur - age) / fade) as f32
    } else {
        1.0
    };
    if age < fade || age > dur - fade {
        ui.request_repaint();
    } else {
        ui.repaint_after(dur - fade - age);
    }
    let pad_y = if placement == ToastPlacement::TopRight { 12.0 } else { 10.0 };
    let layout = ui.layout(&state.text, &theme::LABEL, &TextOpts::wrap(TOAST_MAX_WIDTH - 40.0, 3));
    let size = Size::new(layout.size.w + 40.0, layout.size.h + 2.0 * pad_y);
    let right = theme::CANVAS_W - theme::SAFE_X;
    let origin = match placement {
        ToastPlacement::TopRight => Point::new(right - size.w, theme::SAFE_Y),
        ToastPlacement::Player { controls } => Point::new(right - size.w, if controls { 79.0 } else { theme::SAFE_Y }),
        ToastPlacement::BottomLeft => Point::new(theme::GUTTER, theme::CANVAS_H - theme::SAFE_Y - size.h),
    };
    let color = if state.error { theme::DANGER_TEXT } else { theme::FG };
    ui.opacity(alpha.clamp(0.0, 1.0), |ui| {
        ui.fill(Rect::from_origin_size(origin, size), theme::SURFACE3, radius::CONTROL);
        ui.draw_text(Point::new(origin.x + 20.0, origin.y + pad_y), layout, color);
    });
}

/// A turning ring that says something is on its way. The player's buffering ring
/// (`TvPlayerOverlays.kt`) is 40 dp: a 2 dp Fg ring with a 90° Accent
/// arc going round once a second, showing only once waiting has lasted 500 ms (2.5 s while the
/// stream changes hands). Live TV's tuning ring is 36 dp, a 3 dp Accent arc of 270°, round in
/// 900 ms. Both are drawn at the screen's pixels as a picture of the arc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoadingRing {
    pub size: f32,
    pub stroke: f32,
    /// The arc's length, in degrees.
    pub sweep: f32,
    /// Seconds a turn.
    pub period: f32,
    /// The full ring under the arc, if any.
    pub track: Option<Color>,
    pub color: Color,
    /// Seconds of waiting before it shows.
    pub delay: f64,
}

/// The buffering ring's delay (`BUFFERING_DELAY_MS`).
pub const BUFFERING_DELAY: f64 = 0.5;
/// Its delay while the stream changes hands (`HANDOFF_SPINNER_DELAY_MS`).
pub const HANDOFF_DELAY: f64 = 2.5;

impl LoadingRing {
    /// The player's buffering ring.
    pub fn buffering() -> LoadingRing {
        LoadingRing {
            size: 40.0,
            stroke: 2.0,
            sweep: 90.0,
            period: 1.0,
            track: Some(theme::FG),
            color: theme::ACCENT,
            delay: BUFFERING_DELAY,
        }
    }

    /// Live TV's tuning ring.
    pub fn tuning() -> LoadingRing {
        LoadingRing {
            size: 36.0,
            stroke: 3.0,
            sweep: 270.0,
            period: 0.9,
            track: None,
            color: theme::ACCENT,
            delay: 0.0,
        }
    }

    /// Shows only after `seconds` of waiting.
    pub fn delay(self, seconds: f64) -> LoadingRing {
        LoadingRing { delay: seconds, ..self }
    }

    /// Draws the ring centred on `center` if waiting began at `since` long enough ago; asks for a
    /// frame when it is due, and every frame while it turns. Whether it shows.
    pub fn show(&self, ui: &mut Ui, center: Point, since: f64) -> bool {
        let waited = ui.time() - since;
        if waited < self.delay - 1e-6 {
            ui.repaint_after(self.delay - waited);
            return false;
        }
        let turned = ((waited - self.delay) / self.period as f64).fract() as f32;
        // Whole steps of 3°, so a frame that lands on the same angle reuses the picture.
        let step = (turned * 120.0).floor() as u32 % 120;
        let px = ui.image_px(self.size);
        let image = raster::ring(px, self.stroke * ui.scale(), self.sweep, step as f32 * 3.0, self.track, self.color);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("kit.ring", px, self.stroke.to_bits(), self.sweep.to_bits(), step, self.track, self.color).hash(&mut h);
        let rect = Rect::new(center.x - self.size / 2.0, center.y - self.size / 2.0, self.size, self.size);
        ui.raw(rect, Arc::new(image), h.finish());
        ui.request_repaint();
        true
    }
}

/// A progress bar: a track and how far along it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProgressBar {
    pub track: Color,
    pub fill: Color,
    pub radius: f32,
}

impl ProgressBar {
    /// On art: black at 60 % under Accent (tiles, channel cards).
    pub fn on_art() -> ProgressBar {
        ProgressBar { track: theme::PROGRESS_TRACK_ON_ART, fill: theme::ACCENT, radius: 0.0 }
    }

    /// On the page: Surface4 under Accent, with 2 dp corners.
    pub fn page() -> ProgressBar {
        ProgressBar { track: theme::SURFACE4, fill: theme::ACCENT, radius: radius::BADGE }
    }

    /// Draws the bar in `rect`, `fraction` (0 to 1) filled.
    pub fn show(&self, ui: &mut Ui, rect: Rect, fraction: f32) {
        ui.fill(rect, self.track, self.radius);
        let f = fraction.clamp(0.0, 1.0);
        if f > 0.0 {
            ui.fill(Rect::new(rect.x, rect.y, rect.w * f, rect.h), self.fill, self.radius);
        }
    }
}

/// A still skeleton block (`Skeleton.kt`): Surface2, static on TV.
pub fn skeleton_block(ui: &mut Ui, rect: Rect, corner: f32) {
    ui.fill(rect, theme::SURFACE2, corner);
}

/// A still skeleton row: a header bar of `header` at (96, `y`), `gap` under it, then `count` wells
/// of `kind` with their caption bars (Recommended: 180 × 18, 22, seven posters; Search: 140 × 22).
pub fn skeleton_row(ui: &mut Ui, y: f32, header: Size, gap: f32, kind: TileKind, count: usize) {
    skeleton_block(ui, Rect::new(theme::GUTTER, y, header.w, header.h), radius::BADGE);
    let size = kind.size();
    let top = y + header.h + gap;
    for i in 0..count {
        let x = theme::GUTTER + i as f32 * kind.pitch();
        if x >= theme::CANVAS_W {
            break;
        }
        skeleton_block(ui, Rect::new(x, top, size.w, size.h), kind.radius());
        let h = theme::CAPTION.line_height * 0.6;
        let pad = (theme::CAPTION.line_height - h) / 2.0;
        let cy = top + size.h + theme::TILES_TO_CAPTION;
        skeleton_block(ui, Rect::new(x, cy + pad, size.w * 0.6, h), radius::BADGE);
        skeleton_block(ui, Rect::new(x, cy + theme::CAPTION.line_height + pad, size.w * 0.35, h), radius::BADGE);
    }
}

/// How a [`Message`] is laid out: each page area has its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MessageStyle {
    /// Home and Libraries (`TvHomeNotice`): centred in the page (x 96–912, vertically), the title
    /// in `Tv.PageTitle`, 12, the text in `Tv.Body` Muted (at most 464 wide), 24, the button.
    Page,
    /// A library's areas (`TvMessage`): at x 96 from `top` (140 by default), the title in
    /// `Tv.Section`, 8, the text (at most 520 wide), 16, the button.
    Column { top: f32 },
    /// A title's page (`TvItemParts.kt` `TvMessage`): vertically centred at x 96, a 40 dp icon in
    /// Muted, 16, `Tv.PageTitle`, 8, the text (at most 464), 24, the button.
    Item,
    /// Music and History: x 96, y 60, `Tv.Title`, 10, the text, 12, the button.
    Compact,
    /// The player's failure (`TvPlayerOverlays.kt`): black, centred within 160 dp either side,
    /// `Tv.PageTitle`, 12, the text in four lines, 32, then "Try again" and "Back".
    Player,
}

/// A page's empty or failed state: a title, a sentence, and the one thing to do about it.
#[derive(Clone, Copy, Debug)]
pub struct Message<'a> {
    style: MessageStyle,
    icon: Option<Icon>,
    title: &'a str,
    body: &'a str,
    action: Option<(&'a str, Icon)>,
    secondary: Option<&'a str>,
}

/// What a message's buttons answered.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MessageResponse {
    /// The main button (Retry, Clear filters, Back).
    pub action: Option<Response>,
    /// The player's "Back" beside "Try again".
    pub secondary: Option<Response>,
}

impl<'a> Message<'a> {
    /// A message titled `title`, laid out as `style`.
    pub fn new(style: MessageStyle, title: &'a str) -> Message<'a> {
        let icon = (style == MessageStyle::Item).then_some(Icon::CircleAlert);
        Message { style, icon, title, body: "", action: None, secondary: None }
    }

    /// Its sentence.
    pub fn body(self, body: &'a str) -> Message<'a> {
        Message { body, ..self }
    }

    /// The icon over the title (the item page's).
    pub fn icon(self, icon: Option<Icon>) -> Message<'a> {
        Message { icon, ..self }
    }

    /// Its button: a pill with an icon (Retry with refresh-cw); it takes focus. Without one, the
    /// screen sends focus elsewhere (the rail, the selected tab).
    pub fn action(self, label: &'a str, icon: Icon) -> Message<'a> {
        Message { action: Some((label, icon)), ..self }
    }

    /// A text button after the pill (the player's "Back").
    pub fn secondary(self, label: &'a str) -> Message<'a> {
        Message { secondary: Some(label), ..self }
    }

    fn spec(&self) -> (pley_base::TextStyle, f32, f32, f32) {
        // Title style, gap under the title, the text's widest, gap before the button.
        match self.style {
            MessageStyle::Page => (theme::PAGE_TITLE, 12.0, 464.0, 24.0),
            MessageStyle::Column { .. } => (theme::SECTION, 8.0, 520.0, 16.0),
            MessageStyle::Item => (theme::PAGE_TITLE, 8.0, 464.0, 24.0),
            MessageStyle::Compact => (theme::TITLE, 10.0, 520.0, 12.0),
            MessageStyle::Player => (theme::PAGE_TITLE, 12.0, theme::CANVAS_W - 320.0, 32.0),
        }
    }

    /// Draws the message; its button is registered inside a group `id`.
    pub fn show(&self, ui: &mut Ui, id: Id) -> MessageResponse {
        let (title_style, title_gap, body_max, button_gap) = self.spec();
        let centred = matches!(self.style, MessageStyle::Page | MessageStyle::Player);
        let width = if centred { body_max } else { theme::CANVAS_W - theme::GUTTER - theme::SAFE_X };
        let align = if centred { Align::Center } else { Align::Start };
        let title_opts = TextOpts::wrap(width, 2).aligned(align);
        let body_lines = if self.style == MessageStyle::Player { 4 } else { 0 };
        let body_opts = TextOpts::wrap(body_max, body_lines).aligned(align);
        let title = ui.layout(self.title, &title_style, &title_opts);
        let body = (!self.body.is_empty()).then(|| ui.layout(self.body, &theme::BODY, &body_opts));
        let icon_h = if self.icon.is_some() { 40.0 + 16.0 } else { 0.0 };
        let body_h = body.as_ref().map_or(0.0, |b| title_gap + b.size.h);
        let button_h =
            if self.action.is_some() || self.secondary.is_some() { button_gap + theme::BUTTON_HEIGHT } else { 0.0 };
        let total = icon_h + title.size.h + body_h + button_h;
        let (left, mut y) = match self.style {
            MessageStyle::Page => {
                ((theme::GUTTER + theme::ROW_FITS_BEFORE - width) / 2.0, (theme::CANVAS_H - total) / 2.0)
            }
            MessageStyle::Player => ((theme::CANVAS_W - width) / 2.0, (theme::CANVAS_H - total) / 2.0),
            MessageStyle::Column { top } => (theme::GUTTER, top),
            MessageStyle::Item => (theme::GUTTER, (theme::CANVAS_H - total) / 2.0),
            MessageStyle::Compact => (theme::GUTTER, 60.0),
        };
        if self.style == MessageStyle::Player {
            ui.fill(Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H), Color::BLACK, 0.0);
        }
        if let Some(icon) = self.icon {
            let x = if centred { left + (width - 40.0) / 2.0 } else { left };
            ui.icon(Rect::new(x, y, 40.0, 40.0), icon, theme::MUTED);
            y += icon_h;
        }
        let th = title.size.h;
        ui.draw_text(Point::new(left, y), title, theme::FG);
        y += th;
        if let Some(b) = body {
            y += title_gap;
            let bh = b.size.h;
            let bx = if centred { left + (width - b.size.w) / 2.0 } else { left };
            ui.draw_text(Point::new(bx, y), b, theme::MUTED);
            y += bh;
        }
        let mut out = MessageResponse::default();
        if button_h == 0.0 {
            return out;
        }
        y += button_gap;
        let pill = self.action.map(|(label, icon)| Button::pill(label).icon(icon));
        let back = self.secondary.map(Button::text);
        let widths = (pill.map_or(0.0, |b| b.size(ui).w), back.map_or(0.0, |b| b.size(ui).w));
        let total_w = widths.0 + widths.1 + if pill.is_some() && back.is_some() { 12.0 } else { 0.0 };
        let mut x = if centred { left + (width - total_w) / 2.0 } else { left };
        ui.group(id, bar_group(), |ui| {
            if let Some(p) = pill {
                let pid = ui.id("action");
                ui.initial_focus(pid);
                out.action = Some(p.show(ui, pid, Point::new(x, y)));
                x += widths.0 + 12.0;
            }
            if let Some(b) = back {
                let bid = ui.id("secondary");
                ui.initial_focus(bid);
                out.secondary = Some(b.show(ui, bid, Point::new(x, y)));
            }
        });
        out
    }
}

/// The info block over a page of rows (Home, a title's page): faded in over 150 ms while `visible`
/// (kept with Reduce motion), out as fast. `draw` draws its content.
pub struct InfoBlock;

impl InfoBlock {
    /// The info block's place and width (x 96, y 28, 464 wide).
    pub const RECT: Rect = Rect::new(96.0, 28.0, 464.0, 132.0);

    /// Draws `draw` at the block's fade; returns the fade (0 when nothing was drawn).
    pub fn show(ui: &mut Ui, id: Id, visible: bool, draw: impl FnOnce(&mut Ui)) -> f32 {
        let a = ui.animate(id.with("kit.info"), if visible { 1.0 } else { 0.0 }, Motion::REVEAL);
        if a > 0.0 {
            ui.opacity(a, draw);
        }
        a
    }
}
