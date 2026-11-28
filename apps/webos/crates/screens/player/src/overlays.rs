//! What shows over the picture besides the controls (`TvPlayerOverlays.kt`,
//! `TvPlayerExtras.kt`): Skip Intro and Skip Credits, the Up Next card and its countdown, the
//! seek badge, "Reconnecting…", the compact Playback details and the player's toasts.

use pley_base::{Color, Icon, Point, Rect, Size, TextOpts};
use pley_i18n::t;
use pley_kit::Button;
use pley_kit::look::{mix, shade};
use pley_player::controller::COUNTDOWN_SECONDS;
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Dirs, Easing, Edge, FocusOpts, Group, Id, Motion, Response, Ui};

use crate::controls::register;

/// Skip's bottom edge with the controls hidden and shown (`SKIP_BOTTOM_HIDDEN/SHOWN`: 51 and
/// 185 dp up).
pub const BOTTOM_HIDDEN: f32 = theme::CANVAS_H - 51.0;
/// See [`BOTTOM_HIDDEN`].
pub const BOTTOM_SHOWN: f32 = theme::CANVAS_H - 185.0;
/// The right edge of Skip and Up Next.
const RIGHT: f32 = theme::CANVAS_W - theme::SAFE_X;

/// Where Skip sits as the controls come (`lift` 0 hidden, 1 shown).
pub fn bottom(lift: f32) -> f32 {
    BOTTOM_HIDDEN + (BOTTOM_SHOWN - BOTTOM_HIDDEN) * lift.clamp(0.0, 1.0)
}

/// What a move from Skip does: nothing, but Down to the scrubber while the controls show; with
/// them hidden every direction belongs to the player (it seeks or shows them).
fn corner_group(controls: bool, scrubber: Id) -> Group {
    let down = if controls { Edge::To(scrubber) } else { Edge::Stop };
    let g = Group::row().edge(Dir::Up, Edge::Stop).edge(Dir::Left, Edge::Stop).edge(Dir::Right, Edge::Stop);
    g.edge(Dir::Down, down)
}

fn corner_opts(controls: bool) -> FocusOpts {
    if controls { FocusOpts::default() } else { FocusOpts::default().takes(Dirs::ALL) }
}

/// Skip Intro or Skip Credits: right edge at 912, bottom at `bottom`; at least 40 tall, padded
/// 18 × 2; the label then a chevrons-right icon; black 45 % with a 1 dp Fg 25 % edge, focused
/// black 60 % with a 2 dp Fg 90 % edge and Fg content. No scale, no accent, no countdown.
pub fn skip(ui: &mut Ui, group: Id, id: Id, label: &str, bottom: f32, controls: bool, scrubber: Id) -> Response {
    let l = ui.layout(label, &theme::LABEL, &TextOpts::default());
    let w = 18.0 + l.size.w + 8.0 + 18.0 + 18.0;
    let rect = Rect::new(RIGHT - w, bottom - 40.0, w, 40.0);
    let r = ui.group(group, corner_group(controls, scrubber), |ui| register(ui, id, rect, corner_opts(controls)));
    let (fill, content, edge, width) = if r.focused {
        (Color::BLACK.alpha(0.6), theme::fg(), theme::fg().alpha(0.9), 2.0)
    } else {
        (Color::BLACK.alpha(0.45), theme::fg().alpha(0.85), theme::fg().alpha(0.25), 1.0)
    };
    ui.fill(rect, fill, radius::CONTROL);
    ui.stroke(rect.expand(-width / 2.0), edge, radius::CONTROL - width / 2.0, width);
    let lh = l.size.h;
    ui.draw_text(Point::new(rect.x + 18.0, rect.y + (40.0 - lh) / 2.0), l, content);
    ui.icon(Rect::new(rect.right() - 18.0 - 18.0, rect.y + 11.0, 18.0, 18.0), Icon::ChevronsRight, content);
    r
}

/// What the Up Next card shows.
pub struct Card<'a> {
    pub heading: &'a str,
    pub title: &'a str,
    /// Seconds before the next plays; none when it won't by itself.
    pub seconds_left: Option<i64>,
}

/// What was done on the card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardAnswer {
    pub play_now: bool,
    pub cancel: bool,
}

/// The Up Next card's width (`UP_NEXT_WIDTH`).
pub const CARD_W: f32 = 300.0;
/// Through each second of the countdown the sweep glides, evenly, to where it stands at the
/// second's end (`countdownFill`): it moves without steps and is full as the countdown reaches 0.
/// It runs on every tier, as on Android, and while a direction key is held: the sweep follows the
/// countdown, not the focus.
const SWEEP_GLIDE: Motion = Motion { held: true, ..Motion::new(1.0, Easing::LINEAR) };
/// The sweep over a focused Play now, darker on the Accent; at rest it is the theme's Fg at 14 %
/// on its faint fill (`PlayNowButton`).
const SWEEP_FOCUSED: Color = theme::alpha(Color::BLACK, 0.18);

/// The Up Next card (`TvPlayerOverlays.kt`): 300 wide, right edge 912, bottom 489, Surface2 at
/// 96 % with 8 dp corners; "Up next · S2 · E6", the title, and Play now (with the countdown in
/// it) and Cancel. No still: the card stays small over the credits. Focus stays on the card: Play
/// now first, Left and Up and Down go nowhere.
pub fn up_next(ui: &mut Ui, group: Id, play_id: Id, cancel_id: Id, card: &Card, reduced: bool) -> CardAnswer {
    let pad = 16.0;
    let inner = CARD_W - 2.0 * pad;
    let heading = ui.layout(card.heading, &theme::CAPTION, &TextOpts::line(inner));
    let title = ui.layout(card.title, &theme::TITLE, &TextOpts::wrap(inner, 2));
    let h = pad + heading.size.h + title.size.h + pad + 48.0 + pad;
    let rect = Rect::new(RIGHT - CARD_W, BOTTOM_HIDDEN - h, CARD_W, h);
    ui.fill(rect, theme::surface2().alpha(0.96), radius::DIALOG);
    let mut y = rect.y + pad;
    let hh = heading.size.h;
    ui.draw_text(Point::new(rect.x + pad, y), heading, theme::muted());
    y += hh;
    let th = title.size.h;
    ui.draw_text(Point::new(rect.x + pad, y), title, theme::fg());
    y += th + pad;

    let mut answer = CardAnswer::default();
    let g = Group::row().trap().edge(Dir::Up, Edge::Stop).edge(Dir::Down, Edge::Stop).edge(Dir::Left, Edge::Stop);
    let g = g.edge(Dir::Right, Edge::Stop);
    ui.group(group, g, |ui| {
        ui.initial_focus(play_id);
        let w = play_now(ui, play_id, Point::new(rect.x + pad, y), card.seconds_left, reduced, &mut answer);
        let cancel = Button::text(t!("player.upNext.cancel")).tall();
        let at = Point::new(rect.x + pad + w + 12.0, y);
        answer.cancel = cancel.show(ui, cancel_id, at).activated;
    });
    answer
}

/// Play now (`PlayNowButton`): at least 48 tall, padded 20 · 16 · 4; the play icon and "Play now",
/// with the countdown in it while one runs: "Play now (7)" and a sweep across the button, under
/// the label and inside the button's shape, moving smoothly ([`countdown_fill`]); "Next in 7" and
/// a step each second with Reduce motion. The button keeps the width of the countdown's first
/// second as the count shortens. At rest Fg 12 % with a 1 dp Control edge; focused the Accent,
/// 1.05 larger. Its width.
fn play_now(ui: &mut Ui, id: Id, at: Point, left: Option<i64>, reduced: bool, answer: &mut CardAnswer) -> f32 {
    let l = ui.layout(&play_now_label(left, reduced), &theme::LABEL, &TextOpts::default());
    let widest = left.map(|_| play_now_label(Some(COUNTDOWN_SECONDS), reduced));
    let widest = widest.map(|label| ui.measure(&label, &theme::LABEL, &TextOpts::default()).w);
    let label_w = widest.map_or(l.size.w, |w| w.max(l.size.w));
    let done = left.map(|s| countdown_fill(ui, id.with("sweep"), s, reduced));
    let w = 20.0 + 22.0 + 10.0 + label_w + 16.0;
    let rect = Rect::new(at.x, at.y, w, 48.0);
    let r = register(ui, id, rect, FocusOpts::default());
    answer.play_now = r.activated;
    let s = shade(ui, &theme::focus::pill(), &r);
    let content = s.content;
    let lh = l.size.h;
    ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
        ui.fill(rect, s.fill, radius::CONTROL);
        if let Some(done) = done.filter(|d| *d > 0.0) {
            // The button's own shape in the sweep's color, cut where the countdown stands.
            let swept = Rect::new(rect.x, rect.y, rect.w * done, rect.h);
            let color = mix(theme::fg().alpha(0.14), SWEEP_FOCUSED, s.progress);
            ui.clip(swept, |ui| ui.fill(rect, color, radius::CONTROL));
        }
        if let Some(edge) = s.edge {
            ui.stroke(rect.expand(-0.5), edge, radius::CONTROL - 0.5, 1.0);
        }
        let x = rect.x + 20.0;
        ui.icon(Rect::new(x, rect.y + 13.0, 22.0, 22.0), Icon::Play, content);
        ui.draw_text(Point::new(x + 22.0 + 10.0, rect.y + (48.0 - lh) / 2.0), l, content);
    });
    w
}

/// Play now's words (`playNowLabel`): with the seconds left while a countdown runs.
fn play_now_label(left: Option<i64>, reduced: bool) -> String {
    match left {
        None => t!("player.upNext.playNow").to_string(),
        Some(seconds) if reduced => t!("player.upNext.nextIn", seconds = seconds),
        Some(seconds) => t!("player.upNext.playNowIn", seconds = seconds),
    }
}

/// How much of the countdown has gone with `left` seconds to go, 0 to 1 (`countdownFill`): through
/// each second it glides ([`SWEEP_GLIDE`]) to where it stands at that second's end, carrying on
/// from where it is when a second comes a little early. Ahead of this second's end (a countdown
/// that started again) or more than half a step behind its start (seconds that were missed), it
/// starts from this second's place at once rather than sliding there. With Reduce motion it
/// steps once a second.
fn countdown_fill(ui: &mut Ui, id: Id, left: i64, reduced: bool) -> f32 {
    let total = COUNTDOWN_SECONDS as f32;
    let step = 1.0 / total;
    let gone = |seconds: i64| ((COUNTDOWN_SECONDS - seconds) as f32 / total).clamp(0.0, 1.0);
    let (start, end) = (gone(left), gone(left - 1));
    if reduced {
        return start;
    }
    // Asked for every frame the countdown shows, so its glide carries on from second to second.
    let fill = ui.animate_from(id, start, end, SWEEP_GLIDE);
    if fill > end + step / 1000.0 || fill < start - step / 2.0 {
        return ui.animate_again(id, start, end, SWEEP_GLIDE);
    }
    fill
}

/// The seek badge while the controls are hidden: a 64 dp square, radius 6, Bg 60 %, saying what
/// the presses asked ("−10 s") at the middle of the left or right third.
pub fn seek_badge(ui: &mut Ui, ms: i64) {
    let side = 64.0;
    let bias = if ms < 0 { -2.0 / 3.0 } else { 2.0 / 3.0 };
    let x = (theme::CANVAS_W - side) * (1.0 + bias) / 2.0;
    let rect = Rect::new(x, (theme::CANVAS_H - side) / 2.0, side, side);
    ui.fill(rect, theme::bg().alpha(0.6), radius::CONTROL);
    let l = ui.layout(&pley_player::seeker::badge_text(ms), &theme::LABEL, &TextOpts::default());
    let (w, h) = (l.size.w, l.size.h);
    ui.draw_text(Point::new(rect.center().x - w / 2.0, rect.center().y - h / 2.0), l, theme::fg());
}

/// "Reconnecting…" 16 dp under the buffering ring, on Bg 60 % (padded 12 × 6, radius 6).
pub fn reconnecting(ui: &mut Ui, ring_bottom: f32) {
    let l = ui.layout(pley_player::details::reconnecting(), &theme::LABEL, &TextOpts::default());
    let (w, h) = (l.size.w + 24.0, l.size.h + 12.0);
    let rect = Rect::new((theme::CANVAS_W - w) / 2.0, ring_bottom + 16.0, w, h);
    ui.fill(rect, theme::bg().alpha(0.6), radius::CONTROL);
    ui.draw_text(Point::new(rect.x + 12.0, rect.y + 6.0), l, theme::fg());
}

/// The compact Playback details over the video (Settings → Playback details): top left in the
/// safe area, at most 480 wide, Bg 72 % with 6 dp corners, padded 12 × 8, labels in a 96 dp
/// column (`TvReadout.kt`).
pub fn readout(ui: &mut Ui, lines: &[(String, String)]) {
    if lines.is_empty() {
        return;
    }
    let label_w = 96.0;
    let max = 480.0 - 24.0 - label_w;
    let values: Vec<_> = lines.iter().map(|(_, v)| ui.layout(v, &theme::CAPTION, &TextOpts::line(max))).collect();
    let w = values.iter().map(|v| v.size.w).fold(0.0, f32::max) + label_w + 24.0;
    let line_h = theme::CAPTION.line_height;
    let rect = Rect::new(theme::SAFE_X, theme::SAFE_Y, w.min(480.0), lines.len() as f32 * line_h + 16.0);
    ui.fill(rect, theme::bg().alpha(0.72), radius::CONTROL);
    let mut y = rect.y + 8.0;
    for ((label, _), value) in lines.iter().zip(values) {
        ui.text(Point::new(rect.x + 12.0, y), label, &theme::CAPTION, &TextOpts::line(label_w - 8.0), theme::muted());
        ui.draw_text(Point::new(rect.x + 12.0 + label_w, y), value, theme::fg());
        y += line_h;
    }
}

/// A toast of the player's own: one at a time, at the top right, lower while the controls show.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub error: bool,
    /// When it showed and how long it stays, in seconds.
    pub at: f64,
    pub secs: f64,
}

/// How long toasts stay (`TvPlayerUi.kt`): 3.5 s, errors and quality notes 6 s, the subtitles
/// key's and what the user's remote did 2 s.
pub const TOAST_SECS: f64 = 3.5;
/// See [`TOAST_SECS`].
pub const LONG_TOAST_SECS: f64 = 6.0;
/// See [`TOAST_SECS`].
pub const KEY_TOAST_SECS: f64 = 2.0;
const TOAST_FADE: f64 = 0.150;

/// Draws `toast` while it lasts: Surface3, radius 6, at most 420 wide, padded 20 × 10, up to
/// three lines of `Tv.Label` in Fg or DangerText; its top at 27, or 79 with the controls shown.
/// False once it is over.
pub fn toast(ui: &mut Ui, toast: &Toast, controls: bool) -> bool {
    let age = ui.time() - toast.at;
    if age >= toast.secs {
        return false;
    }
    let alpha = if age < TOAST_FADE {
        (age / TOAST_FADE) as f32
    } else if age > toast.secs - TOAST_FADE {
        ((toast.secs - age) / TOAST_FADE) as f32
    } else {
        1.0
    };
    if age < TOAST_FADE || age > toast.secs - TOAST_FADE {
        ui.request_repaint();
    } else {
        ui.repaint_after(toast.secs - TOAST_FADE - age);
    }
    let l = ui.layout(&toast.text, &theme::LABEL, &TextOpts::wrap(420.0 - 40.0, 3));
    let size = Size::new(l.size.w + 40.0, l.size.h + 20.0);
    let top = if controls { 79.0 } else { theme::SAFE_Y };
    let origin = Point::new(RIGHT - size.w, top);
    let color = if toast.error { theme::danger() } else { theme::fg() };
    ui.opacity(alpha.clamp(0.0, 1.0), |ui| {
        ui.fill(Rect::from_origin_size(origin, size), theme::surface3(), radius::CONTROL);
        ui.draw_text(Point::new(origin.x + 20.0, origin.y + 10.0), l, color);
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_skip_and_the_notice_as_the_controls_come() {
        assert_eq!(bottom(0.0), 489.0);
        assert_eq!(bottom(1.0), 355.0);
    }
}
