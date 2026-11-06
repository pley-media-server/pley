//! What shows over the picture besides the controls (`TvPlayerOverlays.kt`,
//! `TvPlayerExtras.kt`): Skip Intro and Skip Credits, the sleep timer's notice, the Up Next card
//! and its countdown, the seek badge, "Reconnecting…", the compact Playback details and the
//! player's toasts.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use pley_base::{Color, Icon, ImageRequest, Point, Priority, Rect, RgbaImage, Size, TextOpts};
use pley_i18n::t;
use pley_kit::look::shade;
use pley_kit::{Button, placeholder};
use pley_ui::theme::{self, focus::ControlLook, radius};
use pley_ui::{Dir, Dirs, Edge, FocusOpts, Group, Id, ImageOpts, Response, Ui};

use crate::controls::register;

/// Skip and the sleep notice's bottom edge with the controls hidden and shown
/// (`SKIP_BOTTOM_HIDDEN/SHOWN`: 51 and 185 dp up).
pub const BOTTOM_HIDDEN: f32 = theme::CANVAS_H - 51.0;
/// See [`BOTTOM_HIDDEN`].
pub const BOTTOM_SHOWN: f32 = theme::CANVAS_H - 185.0;
/// The right edge of Skip and Up Next.
const RIGHT: f32 = theme::CANVAS_W - theme::SAFE_X;

/// Where Skip and the notice sit as the controls come (`lift` 0 hidden, 1 shown).
pub fn bottom(lift: f32) -> f32 {
    BOTTOM_HIDDEN + (BOTTOM_SHOWN - BOTTOM_HIDDEN) * lift.clamp(0.0, 1.0)
}

/// What a move from Skip or the notice does: nothing, but Down to the scrubber while the controls
/// show; with them hidden every direction belongs to the player (it seeks or shows them).
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
        (Color::BLACK.alpha(0.6), theme::FG, theme::FG.alpha(0.9), 2.0)
    } else {
        (Color::BLACK.alpha(0.45), theme::FG.alpha(0.85), theme::FG.alpha(0.25), 1.0)
    };
    ui.fill(rect, fill, radius::CONTROL);
    ui.stroke(rect.expand(-width / 2.0), edge, radius::CONTROL - width / 2.0, width);
    let lh = l.size.h;
    ui.draw_text(Point::new(rect.x + 18.0, rect.y + (40.0 - lh) / 2.0), l, content);
    ui.icon(Rect::new(rect.right() - 18.0 - 18.0, rect.y + 11.0, 18.0, 18.0), Icon::ChevronsRight, content);
    r
}

/// The sleep timer's notice: bottom left at the safe area, a Surface3 row (radius 6, padded
/// 20 · 8 · 8, items 16 apart) of the moon, "Pausing in 28 s" and "Keep watching".
pub fn sleep_notice(
    ui: &mut Ui,
    group: Id,
    id: Id,
    seconds: i64,
    bottom: f32,
    controls: bool,
    scrubber: Id,
) -> Response {
    let said = t!("player.sleep.pausingIn", seconds = seconds);
    let l = ui.layout(&said, &theme::LABEL, &TextOpts::default());
    let keep = ui.layout(t!("player.sleep.keepWatching"), &theme::LABEL, &TextOpts::default());
    let button = Size::new(keep.size.w + 40.0, keep.size.h + 8.0);
    let h = button.h + 16.0;
    let w = 20.0 + theme::ICON_SIZE + 16.0 + l.size.w + 16.0 + button.w + 8.0;
    let rect = Rect::new(theme::SAFE_X, bottom - h, w, h);
    ui.fill(rect, theme::SURFACE3, radius::CONTROL);
    let mut x = rect.x + 20.0;
    ui.icon(Rect::new(x, rect.y + (h - 20.0) / 2.0, 20.0, 20.0), Icon::Moon, theme::MUTED);
    x += 20.0 + 16.0;
    let lw = l.size.w;
    let lh = l.size.h;
    ui.draw_text(Point::new(x, rect.y + (h - lh) / 2.0), l, theme::FG);
    x += lw + 16.0;
    let b = Rect::new(x, rect.y + 8.0, button.w, button.h);
    let r = ui.group(group, corner_group(controls, scrubber), |ui| register(ui, id, b, corner_opts(controls)));
    let look = ControlLook { fill: theme::ON_ART, edge: None, focus_scale: 1.05, ..theme::focus::PILL };
    let s = shade(ui, &look, &r);
    let kh = keep.size.h;
    ui.transform(Point::ZERO, s.scale, b.center(), |ui| {
        ui.fill(b, s.fill, radius::CONTROL);
        ui.draw_text(Point::new(b.x + 20.0, b.y + (b.h - kh) / 2.0), keep, s.content);
    });
    r
}

/// What the Up Next card shows.
pub struct Card<'a> {
    pub heading: &'a str,
    pub title: &'a str,
    pub image: Option<ImageRequest>,
    /// Seconds before the next plays; none when it won't by itself.
    pub seconds_left: Option<i64>,
}

/// What was done on the card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardAnswer {
    pub play_now: bool,
    pub cancel: bool,
}

/// The Up Next card's width (`UP_NEXT_WIDTH`) and its still's height.
pub const CARD_W: f32 = 300.0;
const STILL_H: f32 = 169.0;
/// The countdown (`COUNTDOWN_SECONDS`) the ring fills over.
const COUNTDOWN: f32 = 10.0;

/// The Up Next card (`TvPlayerOverlays.kt`): 300 wide, right edge 912, bottom 489, Surface2 at
/// 96 % with 8 dp corners; the still, then "Up next · S2 · E6", the title, and Play now (with its
/// countdown ring) and Cancel. Focus stays on the card: Play now first, Left and Up and Down go
/// nowhere.
pub fn up_next(ui: &mut Ui, group: Id, play_id: Id, cancel_id: Id, card: &Card, reduced: bool) -> CardAnswer {
    let pad = 16.0;
    let inner = CARD_W - 2.0 * pad;
    let heading = ui.layout(card.heading, &theme::CAPTION, &TextOpts::line(inner));
    let title = ui.layout(card.title, &theme::TITLE, &TextOpts::wrap(inner, 2));
    let h = STILL_H + pad + heading.size.h + title.size.h + pad + 48.0 + pad;
    let rect = Rect::new(RIGHT - CARD_W, BOTTOM_HIDDEN - h, CARD_W, h);
    ui.fill(rect, theme::SURFACE2.alpha(0.96), radius::DIALOG);
    // The card's shape cuts the still: rounded at the top, square where the text starts. It is
    // drawn taller by the corner and clipped to its own height. Until the still is in, and when
    // there is none, the title's placeholder gradient stands in (`Artwork`, without its title).
    let still = Rect::new(rect.x, rect.y, CARD_W, STILL_H);
    let drawn = Rect::new(rect.x, rect.y, CARD_W, STILL_H + radius::DIALOG);
    ui.clip(still, |ui| {
        let shown = card.image.as_ref().map(|req| ui.image_state(req, Priority::Visible));
        if !shown.is_some_and(|s| s.opaque()) {
            placeholder(ui, drawn, card.title, "", radius::DIALOG);
        }
        if let Some(s) = shown {
            ui.draw_image(drawn, &s, &ImageOpts::cover().rounded(radius::DIALOG));
        }
    });
    let mut y = rect.y + STILL_H + pad;
    let hh = heading.size.h;
    ui.draw_text(Point::new(rect.x + pad, y), heading, theme::MUTED);
    y += hh;
    let th = title.size.h;
    ui.draw_text(Point::new(rect.x + pad, y), title, theme::FG);
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

/// Play now: at least 48 tall, padded 20 · 16 · 4; the play icon, "Play now" and, 12 dp after, the
/// countdown ring (22 dp, a 3 dp stroke filling as the seconds go), or "Next in 7" in Caption with
/// Reduce motion. At rest Fg 12 % with a 1 dp Control edge; focused the Accent, 1.05 larger. Its
/// width.
fn play_now(ui: &mut Ui, id: Id, at: Point, left: Option<i64>, reduced: bool, answer: &mut CardAnswer) -> f32 {
    let l = ui.layout(t!("player.upNext.playNow"), &theme::LABEL, &TextOpts::default());
    let next_in = left
        .filter(|_| reduced)
        .map(|s| ui.layout(&t!("player.upNext.nextIn", seconds = s), &theme::CAPTION, &TextOpts::default()));
    let ring = left.is_some() && !reduced;
    let after = match &next_in {
        Some(n) => 12.0 + n.size.w,
        None if ring => 12.0 + 22.0,
        None => 0.0,
    };
    let w = 20.0 + 22.0 + 10.0 + l.size.w + after + 16.0;
    let rect = Rect::new(at.x, at.y, w, 48.0);
    let r = register(ui, id, rect, FocusOpts::default());
    answer.play_now = r.activated;
    let s = shade(ui, &theme::focus::PILL, &r);
    let content = s.content;
    let lh = l.size.h;
    ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
        ui.fill(rect, s.fill, radius::CONTROL);
        if let Some(edge) = s.edge {
            ui.stroke(rect.expand(-0.5), edge, radius::CONTROL - 0.5, 1.0);
        }
        let mut x = rect.x + 20.0;
        ui.icon(Rect::new(x, rect.y + 13.0, 22.0, 22.0), Icon::Play, content);
        x += 22.0 + 10.0;
        let lw = l.size.w;
        ui.draw_text(Point::new(x, rect.y + (48.0 - lh) / 2.0), l, content);
        x += lw + 12.0;
        if let Some(n) = next_in {
            let nh = n.size.h;
            ui.draw_text(Point::new(x, rect.y + (48.0 - nh) / 2.0), n, content);
        } else if let (true, Some(seconds)) = (ring, left) {
            let done = ((COUNTDOWN - seconds as f32) / COUNTDOWN).clamp(0.0, 1.0);
            countdown(ui, Rect::new(x, rect.y + 13.0, 22.0, 22.0), done, content);
        }
    });
    w
}

/// The countdown ring: a 3 dp stroke, its track at 25 % and the arc from the top, clockwise, as
/// far as `done`; a picture made at the screen's pixels (the display list has no arcs).
fn countdown(ui: &mut Ui, rect: Rect, done: f32, color: Color) {
    let px = ui.image_px(rect.w);
    let stroke = 3.0 * ui.scale();
    let steps = (done * 100.0).round() as u32;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ("player.countdown", px, steps, color.r, color.g, color.b, color.a).hash(&mut h);
    let image = ring_image(px, stroke, steps as f32 / 100.0, color);
    ui.raw(rect, Arc::new(image), h.finish());
}

/// A ring `px` pixels across with a `stroke` wide band: the track at 25 % of `color` and the arc
/// in `color` from 12 o'clock clockwise over `done` of the turn, edges smoothed over a pixel.
pub fn ring_image(px: u32, stroke: f32, done: f32, color: Color) -> RgbaImage {
    let mut img = RgbaImage::new(px, px);
    let c = px as f32 / 2.0;
    let outer = c;
    let inner = (c - stroke).max(0.0);
    for y in 0..px {
        for x in 0..px {
            let dx = x as f32 + 0.5 - c;
            let dy = y as f32 + 0.5 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let cover = (outer - d).clamp(0.0, 1.0).min((d - inner).clamp(0.0, 1.0));
            if cover <= 0.0 {
                continue;
            }
            // Angle from 12 o'clock, clockwise, 0 to 1.
            let turn = (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0);
            let a = if turn <= done { 1.0 } else { 0.25 };
            let i = ((y * px + x) * 4) as usize;
            img.pixels[i] = color.r;
            img.pixels[i + 1] = color.g;
            img.pixels[i + 2] = color.b;
            img.pixels[i + 3] = (color.a as f32 * a * cover).round() as u8;
        }
    }
    img
}

/// The seek badge while the controls are hidden: a 64 dp square, radius 6, Bg 60 %, saying what
/// the presses asked ("−10 s") at the middle of the left or right third.
pub fn seek_badge(ui: &mut Ui, ms: i64) {
    let side = 64.0;
    let bias = if ms < 0 { -2.0 / 3.0 } else { 2.0 / 3.0 };
    let x = (theme::CANVAS_W - side) * (1.0 + bias) / 2.0;
    let rect = Rect::new(x, (theme::CANVAS_H - side) / 2.0, side, side);
    ui.fill(rect, theme::BG.alpha(0.6), radius::CONTROL);
    let l = ui.layout(&pley_player::seeker::badge_text(ms), &theme::LABEL, &TextOpts::default());
    let (w, h) = (l.size.w, l.size.h);
    ui.draw_text(Point::new(rect.center().x - w / 2.0, rect.center().y - h / 2.0), l, theme::FG);
}

/// "Reconnecting…" 16 dp under the buffering ring, on Bg 60 % (padded 12 × 6, radius 6).
pub fn reconnecting(ui: &mut Ui, ring_bottom: f32) {
    let l = ui.layout(pley_player::details::reconnecting(), &theme::LABEL, &TextOpts::default());
    let (w, h) = (l.size.w + 24.0, l.size.h + 12.0);
    let rect = Rect::new((theme::CANVAS_W - w) / 2.0, ring_bottom + 16.0, w, h);
    ui.fill(rect, theme::BG.alpha(0.6), radius::CONTROL);
    ui.draw_text(Point::new(rect.x + 12.0, rect.y + 6.0), l, theme::FG);
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
    ui.fill(rect, theme::BG.alpha(0.72), radius::CONTROL);
    let mut y = rect.y + 8.0;
    for ((label, _), value) in lines.iter().zip(values) {
        ui.text(Point::new(rect.x + 12.0, y), label, &theme::CAPTION, &TextOpts::line(label_w - 8.0), theme::MUTED);
        ui.draw_text(Point::new(rect.x + 12.0 + label_w, y), value, theme::FG);
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
    let color = if toast.error { theme::DANGER_TEXT } else { theme::FG };
    ui.opacity(alpha.clamp(0.0, 1.0), |ui| {
        ui.fill(Rect::from_origin_size(origin, size), theme::SURFACE3, radius::CONTROL);
        ui.draw_text(Point::new(origin.x + 20.0, origin.y + 10.0), l, color);
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_the_countdown_ring_clockwise_from_the_top() {
        let img = ring_image(44, 6.0, 0.25, Color::WHITE);
        let alpha = |x: u32, y: u32| img.pixels[((y * 44 + x) * 4 + 3) as usize];
        // Just above the right of the ring is under a quarter turn: filled; the left is the
        // track at 25 %.
        assert_eq!(alpha(42, 21), 255);
        assert_eq!(alpha(42, 23), 64);
        assert!(alpha(1, 22) > 50 && alpha(1, 22) < 70, "{}", alpha(1, 22));
        // Inside the ring: nothing.
        assert_eq!(alpha(22, 22), 0);
    }

    #[test]
    fn places_skip_and_the_notice_as_the_controls_come() {
        assert_eq!(bottom(0.0), 489.0);
        assert_eq!(bottom(1.0), 355.0);
    }
}
