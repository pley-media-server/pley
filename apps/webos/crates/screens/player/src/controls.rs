//! The controls overlay (`TvPlayerControls.kt`): the scrim, the title and clock at the top, the
//! seek preview, the scrubber with its times, chapter gaps and marker label, and the control
//! row. Everything is on the 960×540 canvas; the screen draws it at the controls' fade.

use pley_base::{Cmd, Color, GradientStop, Icon, ImageRequest, Point, Rect, Size, TextOpts, Weight};
use pley_core::model::MarkerKind;
use pley_i18n::t;
use pley_kit::look::shade;
use pley_player::controller::{ChapterMark, SeekPreview};
use pley_player::markers::active_marker;
use pley_ui::theme::{self, focus::ControlLook, radius};
use pley_ui::{Dir, Dirs, Edge, FocusOpts, Group, Id, ImageOpts, Motion, Response, Ui};

use crate::previews::Tile;
use crate::text;

/// The times row's top (`TIMES_TOP`).
pub const TIMES_TOP: f32 = 399.0;
/// The track's centre line (`TRACK_CENTRE`).
pub const TRACK_CENTRE: f32 = 425.0;
/// The focusable bar's height around the track (`SCRUB_HEIGHT`).
pub const SCRUB_HEIGHT: f32 = 40.0;
/// The scrubber's span: the safe area's sides.
pub const BAR_X: f32 = theme::SAFE_X;
/// See [`BAR_X`].
pub const BAR_W: f32 = theme::CANVAS_W - 2.0 * theme::SAFE_X;
const TRACK_HEIGHT: f32 = 4.0;
const CHAPTER_GAP: f32 = 2.0;
/// The thumb's diameter, and focused (`PleyTv.ScrubThumb`, `ScrubThumbFocused`).
const THUMB: f32 = 10.0;
const THUMB_FOCUSED: f32 = 12.0;
/// The focused thumb's ring: its stroke and how far out it is (`PleyFocus.ThumbRing*`).
pub(crate) const RING_STROKE: f32 = 2.0;
pub(crate) const RING_OFFSET: f32 = 2.0;
/// The control row's top (`ROW_TOP`).
pub const ROW_TOP: f32 = 441.0;
/// The seek preview's box and its top (`PREVIEW_*`).
pub const PREVIEW_W: f32 = 240.0;
/// See [`PREVIEW_W`].
pub const PREVIEW_H: f32 = 135.0;
/// See [`PREVIEW_W`].
pub const PREVIEW_TOP: f32 = 212.0;
/// Buttons: 36 dp with 20 dp glyphs, transport glyphs 24, Play/Pause 44 with 26.
const BUTTON: f32 = 36.0;
const TRANSPORT_ICON: f32 = 24.0;
const PLAY_BUTTON: f32 = 44.0;
const PLAY_ICON: f32 = 26.0;
/// Between the centre group's buttons (`TRANSPORT_GAP`), and the side groups' (`PleyTv.ButtonGap`).
const TRANSPORT_GAP: f32 = 16.0;
const SIDE_GAP: f32 = 10.0;
/// A focused button's label, below it.
const LABEL_GAP: f32 = 8.0;

/// The scrim behind the controls (`controlScrims`): black at 65 % behind the title, 25 % through
/// the middle and 90 % under the row, at the controls' fade.
pub fn scrim(ui: &mut Ui, fade: f32) {
    if fade <= 0.0 {
        return;
    }
    let stops = [(0.0, 0.65), (0.22, 0.25), (0.55, 0.25), (0.78, 0.7), (1.0, 0.9)]
        .iter()
        .map(|&(at, a)| GradientStop::new(at, Color::BLACK.alpha(a * fade)))
        .collect();
    ui.gradient(Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H), true, stops);
}

/// The title banner's height.
pub const BANNER_H: f32 = 120.0;

/// The title banner (`PlayerScreen.Banner`): what plays and its episode line where the top bar has
/// them, over a shade from the scrim's 65 % black to clear, at `fade`.
pub fn banner(ui: &mut Ui, title: &str, line: Option<&str>, fade: f32) {
    if fade <= 0.0 {
        return;
    }
    let stops =
        vec![GradientStop::new(0.0, Color::BLACK.alpha(0.65 * fade)), GradientStop::new(1.0, Color::TRANSPARENT)];
    ui.gradient(Rect::new(0.0, 0.0, theme::CANVAS_W, BANNER_H), true, stops);
    let left = theme::SAFE_X;
    let width = theme::CANVAS_W - theme::SAFE_X - 300.0;
    ui.opacity(fade, |ui| {
        let mut y = theme::SAFE_Y;
        y += ui.text(Point::new(left, y), title, &theme::TITLE, &TextOpts::line(width), theme::FG).h;
        if let Some(line) = line {
            ui.text(Point::new(left, y + 4.0), line, &theme::LABEL, &TextOpts::line(width), theme::MUTED);
        }
    });
}

/// What the top bar shows.
pub struct TopBar<'a> {
    pub title: &'a str,
    pub line: Option<&'a str>,
    pub method: Option<&'a str>,
    pub clock: &'a str,
    /// "Ends at 22:14", while paused.
    pub ends: Option<&'a str>,
}

/// The title and episode line at the top left; how it plays, the clock and "Ends at" at the top
/// right.
pub fn top_bar(ui: &mut Ui, bar: &TopBar) {
    let left = theme::SAFE_X;
    let right = theme::CANVAS_W - theme::SAFE_X;
    let width = right - left - 300.0 + theme::SAFE_X;
    let mut y = theme::SAFE_Y;
    y += ui.text(Point::new(left, y), bar.title, &theme::TITLE, &TextOpts::line(width), theme::FG).h;
    if let Some(line) = bar.line {
        ui.text(Point::new(left, y + 4.0), line, &theme::LABEL, &TextOpts::line(width), theme::MUTED);
    }
    let clock = ui.layout(bar.clock, &theme::LABEL, &TextOpts::default());
    let badge_style = theme::CAPTION.with_weight(Weight::Bold);
    let badge = bar.method.map(|m| ui.layout(m, &badge_style, &TextOpts::default()));
    let badge_h = theme::CAPTION.line_height + 6.0;
    let row_h = badge.as_ref().map_or(clock.size.h, |_| badge_h.max(clock.size.h));
    let top = theme::SAFE_Y;
    let clock_w = clock.size.w;
    let clock_h = clock.size.h;
    ui.draw_text(Point::new(right - clock_w, top + (row_h - clock_h) / 2.0), clock, theme::MUTED);
    if let (Some(method), Some(layout)) = (bar.method, badge) {
        let color = text::method_color(method);
        let w = layout.size.w + 16.0;
        let r = Rect::new(right - clock_w - 12.0 - w, top + (row_h - badge_h) / 2.0, w, badge_h);
        ui.fill(r, theme::ON_ART, radius::BADGE);
        ui.fill(r, color.alpha(0.2), radius::BADGE);
        ui.draw_text(Point::new(r.x + 8.0, r.y + 3.0), layout, color);
    }
    if let Some(ends) = bar.ends {
        let l = ui.layout(ends, &theme::LABEL, &TextOpts::default());
        let w = l.size.w;
        ui.draw_text(Point::new(right - w, top + row_h), l, theme::MUTED);
    }
}

/// Registers a focusable of the player unless it is being drawn on its way out; a kit-like
/// control asks for one more frame when it is selected.
pub fn register(ui: &mut Ui, id: Id, rect: Rect, opts: FocusOpts) -> Response {
    if pley_kit::is_inert() {
        return Response {
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
        };
    }
    let r = ui.focusable_with(id, rect, opts);
    if r.activated || r.menu {
        ui.request_repaint();
    }
    r
}

/// Where the scrubber is.
pub struct Scrub<'a> {
    pub position_ms: i64,
    pub duration_ms: i64,
    pub buffered_ms: i64,
    /// A seek on its way: the thumb shows it.
    pub target_ms: Option<i64>,
    pub chapters: &'a [ChapterMark],
    pub markers: &'a [pley_core::model::Marker],
    /// What Up does from the bar: Skip, when it shows.
    pub up: Edge,
    /// Up opens the chapters as cards (a seek waits, and the film has chapters): the bar takes it.
    pub up_opens_chapters: bool,
    /// Play/Pause, where Down goes.
    pub down: Id,
}

/// The times, the bar (focusable, taking Left and Right, OK and the pointer's click) and the
/// label of the marker playing. `group` holds the bar alone so Up and Down go where Android
/// sends them.
pub fn scrubber(ui: &mut Ui, group: Id, id: Id, s: &Scrub) -> Response {
    let left = BAR_X;
    let right = BAR_X + BAR_W;
    // While a seek is on its way, the times say where it goes, as the thumb and the blue do.
    let shown_ms = s.target_ms.unwrap_or(s.position_ms);
    let elapsed = text::time(shown_ms);
    let rest = text::remaining(shown_ms, s.duration_ms);
    ui.text(Point::new(left, TIMES_TOP), &elapsed, &theme::CAPTION, &TextOpts::default(), theme::FG);
    let rest = ui.layout(&rest, &theme::CAPTION, &TextOpts::default());
    let w = rest.size.w;
    ui.draw_text(Point::new(right - w, TIMES_TOP), rest, theme::FG);

    let bar = Rect::new(left, TRACK_CENTRE - SCRUB_HEIGHT / 2.0, BAR_W, SCRUB_HEIGHT);
    let g = Group::row()
        .edge(Dir::Up, s.up)
        .edge(Dir::Down, Edge::To(s.down))
        .edge(Dir::Left, Edge::Stop)
        .edge(Dir::Right, Edge::Stop);
    let takes = if s.up_opens_chapters { Dirs::HORIZONTAL.union(Dirs::UP) } else { Dirs::HORIZONTAL };
    let r = ui.group(group, g, |ui| register(ui, id, bar, FocusOpts::default().takes(takes).click_at()));
    let focus = ui.animate(id.with("thumb"), if r.focused { 1.0 } else { 0.0 }, Motion::FOCUS);
    track(ui, s, focus);

    if let Some(m) = active_marker(s.markers, s.position_ms) {
        let label =
            if m.kind == MarkerKind::Intro { t!("player.controls.intro") } else { t!("player.controls.credits") };
        let l = ui.layout(label, &theme::CAPTION, &TextOpts::default());
        let d = s.duration_ms.max(1) as f32;
        let mid = left + ((m.start_ms + m.end_ms) as f32 / 2.0 / d).clamp(0.0, 1.0) * BAR_W;
        let x = (mid - l.size.w / 2.0).clamp(left, (right - l.size.w).max(left));
        ui.draw_text(Point::new(x, TRACK_CENTRE + 6.0), l, theme::MUTED);
    }
    r
}

/// The track: Fg 25 % behind, buffered at 45 %, played in the Accent up to the thumb, cut at
/// chapter starts; the thumb at the seek target or the playhead, with its ring as focus comes.
fn track(ui: &mut Ui, s: &Scrub, focus: f32) {
    let w = BAR_W;
    let y = TRACK_CENTRE - TRACK_HEIGHT / 2.0;
    let d = s.duration_ms.max(1) as f32;
    let x = |ms: i64| (ms as f32 / d).clamp(0.0, 1.0) * w;
    let mut edges = vec![0.0];
    edges.extend(s.chapters.iter().map(|c| x(c.start_ms)).filter(|&e| e > CHAPTER_GAP && e < w - CHAPTER_GAP));
    edges.push(w);
    let played = x(s.target_ms.unwrap_or(s.position_ms));
    let buffered = x(s.buffered_ms.max(s.position_ms));
    let r = TRACK_HEIGHT / 2.0;
    let last = edges.len() - 2;
    for i in 0..=last {
        let a = edges[i] + if i == 0 { 0.0 } else { CHAPTER_GAP / 2.0 };
        let b = edges[i + 1] - if i == last { 0.0 } else { CHAPTER_GAP / 2.0 };
        if b <= a {
            continue;
        }
        let seg = |to: f32| Rect::new(BAR_X + a, y, to.min(b) - a, TRACK_HEIGHT);
        ui.fill(seg(b), theme::FG.alpha(0.25), r);
        if buffered > a {
            ui.fill(seg(buffered), theme::FG.alpha(0.45), r);
        }
        if played > a {
            ui.fill(seg(played), theme::ACCENT, r);
        }
    }
    let radius = (THUMB + (THUMB_FOCUSED - THUMB) * focus) / 2.0;
    let cx = BAR_X + x(s.target_ms.unwrap_or(s.position_ms)).clamp(radius, w - radius);
    let c = Point::new(cx, TRACK_CENTRE);
    ui.fill(Rect::new(c.x - radius, c.y - radius, 2.0 * radius, 2.0 * radius), theme::FG, radius);
    if focus > 0.0 {
        let ring = radius + RING_OFFSET;
        let rect = Rect::new(c.x - ring, c.y - ring, 2.0 * ring, 2.0 * ring);
        ui.stroke(rect, theme::ACCENT_BRIGHT.alpha(focus), ring, RING_STROKE);
    }
}

/// The seek preview over the scrubber while a seek is on its way (`TvSeekPreview`): the server's
/// seek preview (`tile`, cut out of its sheet) or the chapter's picture in a 240 × 135 box with a
/// 2 dp Fg 20 % edge, its bottom at 347, and under it the target time and the chapter; centred
/// on the target, inside the safe area.
pub fn preview(
    ui: &mut Ui,
    target_ms: i64,
    duration_ms: i64,
    picture: Option<&SeekPreview>,
    tile: Option<&Tile>,
    chapters: &[ChapterMark],
) {
    let time = ui.layout(&text::time(target_ms), &theme::LABEL, &TextOpts::default());
    let chapter = text::chapter_at(chapters, target_ms).map(|i| {
        let line = text::chapter_line(&chapters[i], i);
        ui.layout(&line, &theme::CAPTION, &TextOpts::line(BAR_W))
    });
    let mut width = time.size.w.max(chapter.as_ref().map_or(0.0, |c| c.size.w));
    if picture.is_some() {
        width = width.max(PREVIEW_W);
    }
    let d = duration_ms.max(1) as f32;
    let centre = BAR_X + (target_ms as f32 / d).clamp(0.0, 1.0) * BAR_W;
    let x = (centre - width / 2.0).clamp(BAR_X, (BAR_X + BAR_W - width).max(BAR_X));
    let mid = x + width / 2.0;
    let mut y = if picture.is_some() { PREVIEW_TOP } else { PREVIEW_TOP + PREVIEW_H + 6.0 };
    if let Some(p) = picture {
        let rect = Rect::new(mid - PREVIEW_W / 2.0, y, PREVIEW_W, PREVIEW_H);
        ui.fill(rect, theme::SURFACE2, radius::TILE);
        draw_preview(ui, rect, p, tile);
        ui.stroke(rect.expand(-1.0), theme::FG.alpha(0.2), radius::TILE - 1.0, 2.0);
        y += PREVIEW_H + 6.0;
    }
    let tw = time.size.w;
    let th = time.size.h;
    ui.draw_text(Point::new(mid - tw / 2.0, y), time, theme::FG);
    if let Some(c) = chapter {
        let cw = c.size.w;
        ui.draw_text(Point::new(mid - cw / 2.0, y + th), c, theme::MUTED);
    }
}

/// The preview's picture: the tile cut out of its seek-preview sheet (none while the sheet comes),
/// or a chapter's picture, letterboxed (`PlayerOverlays.Letterbox`): as large as fits, centred,
/// black where it doesn't reach, never stretched or cropped, as the server's previews keep the
/// film's shape.
fn draw_preview(ui: &mut Ui, rect: Rect, p: &SeekPreview, tile: Option<&Tile>) {
    match p {
        SeekPreview::Chapter { url } => {
            ui.fill(rect, Color::BLACK, radius::TILE);
            let req = ImageRequest::new(url.as_str(), ui.image_px(PREVIEW_W));
            ui.image(rect, &req, &ImageOpts::contain().rounded(radius::TILE));
        }
        SeekPreview::Sheet { .. } => {
            if let Some(t) = tile {
                letterboxed(ui, rect, t);
            }
        }
    }
}

/// A preview cut out of its sheet, letterboxed in `rect` on black.
pub(crate) fn letterboxed(ui: &mut Ui, rect: Rect, t: &Tile) {
    ui.fill(rect, Color::BLACK, radius::TILE);
    let (image, key) = (t.image.clone(), t.key);
    let at = letterbox(rect, image.width as f32, image.height as f32);
    // Rounded only where the picture reaches the box's corners.
    let round = if at.w >= rect.w - 0.5 && at.h >= rect.h - 0.5 { radius::TILE } else { 0.0 };
    ui.cmd(Cmd::Raw { rect: at, image, key, radius: round, opacity: 1.0 });
}

/// A `w` × `h` picture as large as fits in `rect`, centred.
pub fn letterbox(rect: Rect, w: f32, h: f32) -> Rect {
    if w <= 0.0 || h <= 0.0 {
        return rect;
    }
    let k = (rect.w / w).min(rect.h / h);
    let (fw, fh) = (w * k, h * k);
    Rect::new(rect.x + (rect.w - fw) / 2.0, rect.y + (rect.h - fh) / 2.0, fw, fh)
}

/// A button of the control row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Btn {
    Chapters,
    Sleep,
    Speed,
    Previous,
    PlayPause,
    Next,
    Subtitles,
    Audio,
    Quality,
    Info,
}

impl Btn {
    /// What it says under itself when focused (and to screen readers).
    pub fn label(self, playing: bool) -> &'static str {
        match self {
            Btn::Chapters => t!("player.controls.chapters"),
            Btn::Sleep => t!("player.controls.sleep"),
            Btn::Speed => t!("player.controls.speed"),
            Btn::Previous => t!("player.controls.previous"),
            Btn::PlayPause if playing => t!("player.controls.pause"),
            Btn::PlayPause => t!("player.controls.play"),
            Btn::Next => t!("player.controls.next"),
            Btn::Subtitles => t!("player.controls.subtitles"),
            Btn::Audio => t!("player.controls.audio"),
            Btn::Quality => t!("player.controls.quality"),
            Btn::Info => t!("player.controls.info"),
        }
    }
}

/// Which buttons the row has (`RowOptions`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowOptions {
    pub previous_next: bool,
    pub subtitles: bool,
    pub audio: bool,
    pub quality: bool,
    pub chapters: bool,
}

impl RowOptions {
    /// The row's three groups, left to right: [Chapters] Sleep Speed · [Previous] Play/Pause
    /// [Next] · [Subtitles] [Audio] [Quality] Info, as the Android TV app's (Left and Right seek).
    pub fn groups(&self) -> [Vec<Btn>; 3] {
        let mut left = Vec::new();
        if self.chapters {
            left.push(Btn::Chapters);
        }
        left.extend([Btn::Sleep, Btn::Speed]);
        let mut centre = Vec::new();
        if self.previous_next {
            centre.push(Btn::Previous);
        }
        centre.push(Btn::PlayPause);
        if self.previous_next {
            centre.push(Btn::Next);
        }
        let mut right = Vec::new();
        if self.subtitles {
            right.push(Btn::Subtitles);
        }
        if self.audio {
            right.push(Btn::Audio);
        }
        if self.quality {
            right.push(Btn::Quality);
        }
        right.push(Btn::Info);
        [left, centre, right]
    }
}

fn size_of(b: Btn) -> f32 {
    if b == Btn::PlayPause { PLAY_BUTTON } else { BUTTON }
}

/// Where each button of the row goes: the side groups at the safe area's edges, the transport
/// centred; the 36 dp buttons 4 dp down so their centres line up with Play/Pause's.
pub fn layout_row(options: &RowOptions) -> Vec<(Btn, Rect)> {
    let [left, centre, right] = options.groups();
    let mut out = Vec::new();
    let place = |b: Btn, x: f32| {
        let s = size_of(b);
        Rect::new(x, ROW_TOP + (PLAY_BUTTON - s) / 2.0, s, s)
    };
    let mut x = theme::SAFE_X;
    for b in left {
        out.push((b, place(b, x)));
        x += size_of(b) + SIDE_GAP;
    }
    let width: f32 = centre.iter().map(|b| size_of(*b)).sum::<f32>() + TRANSPORT_GAP * (centre.len() - 1) as f32;
    let mut x = (theme::CANVAS_W - width) / 2.0;
    for b in centre {
        out.push((b, place(b, x)));
        x += size_of(b) + TRANSPORT_GAP;
    }
    let width: f32 = right.iter().map(|b| size_of(*b)).sum::<f32>() + SIDE_GAP * (right.len() - 1) as f32;
    let mut x = theme::CANVAS_W - theme::SAFE_X - width;
    for b in right {
        out.push((b, place(b, x)));
        x += size_of(b) + SIDE_GAP;
    }
    out
}

/// The control row in one focus group (Left and Right along it, Up to the scrubber, Down
/// nowhere); the button selected, if one was.
pub fn row(
    ui: &mut Ui,
    group: Id,
    ids: &dyn Fn(Btn) -> Id,
    options: &RowOptions,
    playing: bool,
    scrubber: Id,
) -> Option<Btn> {
    let g = Group::row()
        .edge(Dir::Up, Edge::To(scrubber))
        .edge(Dir::Down, Edge::Stop)
        .edge(Dir::Left, Edge::Stop)
        .edge(Dir::Right, Edge::Stop);
    let mut chosen = None;
    ui.group(group, g, |ui| {
        for (b, rect) in layout_row(options) {
            if button(ui, ids(b), rect, b, playing).activated {
                chosen = Some(b);
            }
        }
    });
    chosen
}

/// The look of the player's buttons: a glyph alone on the scrim until focused, then an Accent
/// square (radius 6) 1.08 larger with the glyph in OnAccent; AccentBright while pressed.
const LOOK: ControlLook = ControlLook {
    fill: Color::TRANSPARENT,
    content: theme::FG,
    edge: None,
    focus_fill: theme::ACCENT,
    focus_content: theme::ON_ACCENT,
    pressed_fill: theme::ACCENT_BRIGHT,
    focus_scale: theme::focus::ROUND.focus_scale,
};

/// One button of the row, its label fading in 8 dp under it while focused: centred under it, but
/// kept inside the safe area, where a long label under a button at either end would run off.
fn button(ui: &mut Ui, id: Id, rect: Rect, b: Btn, playing: bool) -> Response {
    let r = register(ui, id, rect, FocusOpts::default());
    let s = shade(ui, &LOOK, &r);
    ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
        ui.fill(rect, s.fill, radius::CONTROL);
        glyph(ui, rect, b, playing, s.content);
    });
    let shown = ui.animate(id.with("label"), if r.focused { 1.0 } else { 0.0 }, Motion::REVEAL);
    if shown > 0.0 {
        let l = ui.layout(b.label(playing), &theme::CAPTION, &TextOpts::default());
        let x = (rect.center().x - l.size.w / 2.0).min(theme::CANVAS_W - theme::SAFE_X - l.size.w).max(theme::SAFE_X);
        let at = Point::new(x, rect.bottom() + LABEL_GAP);
        ui.opacity(shown, |ui| ui.draw_text(at, l, theme::FG));
    }
    r
}

fn glyph(ui: &mut Ui, rect: Rect, b: Btn, playing: bool, color: Color) {
    let (icon, size) = match b {
        Btn::Chapters => (Icon::ListVideo, theme::ICON_SIZE),
        Btn::Sleep => (Icon::Moon, theme::ICON_SIZE),
        Btn::Speed => (Icon::FastForward, theme::ICON_SIZE),
        Btn::Previous => (Icon::SkipBack, TRANSPORT_ICON),
        Btn::Next => (Icon::SkipForward, TRANSPORT_ICON),
        Btn::PlayPause => (if playing { Icon::Pause } else { Icon::Play }, PLAY_ICON),
        Btn::Subtitles => (Icon::Captions, theme::ICON_SIZE),
        Btn::Audio => (Icon::AudioLines, theme::ICON_SIZE),
        Btn::Quality => (Icon::Gauge, theme::ICON_SIZE),
        Btn::Info => (Icon::Info, theme::ICON_SIZE),
    };
    ui.icon(
        Rect::from_origin_size(Point::ZERO, Size::new(size, size))
            .translate(rect.center().x - size / 2.0, rect.center().y - size / 2.0),
        icon,
        color,
    );
}

/// Every string a frame drew, in order (for tests).
#[cfg(test)]
pub(crate) fn texts(list: &pley_base::DisplayList) -> Vec<String> {
    list.texts().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use pley_ui::testing::Harness;

    use super::*;

    fn all() -> RowOptions {
        RowOptions { previous_next: true, subtitles: true, audio: true, quality: true, chapters: true }
    }

    #[test]
    fn a_preview_is_letterboxed_in_its_box() {
        let boxed = Rect::new(0.0, 0.0, 240.0, 135.0);
        // A 2.39:1 film's preview: the full width, black above and below.
        let wide = letterbox(boxed, 320.0, 134.0);
        assert_eq!((wide.x, wide.w), (0.0, 240.0));
        assert!((wide.h - 100.5).abs() < 0.01 && (wide.y - 17.25).abs() < 0.01, "{wide:?}");
        // A 4:3 one: the full height, black at the sides.
        let square = letterbox(boxed, 160.0, 120.0);
        assert_eq!((square.y, square.h), (0.0, 135.0));
        assert!((square.w - 180.0).abs() < 0.01 && (square.x - 30.0).abs() < 0.01, "{square:?}");
        assert_eq!(letterbox(boxed, 0.0, 10.0), boxed);
    }

    #[test]
    fn lays_the_row_out_as_android_does() {
        let row = layout_row(&all());
        let at = |b: Btn| row.iter().find(|(x, _)| *x == b).unwrap().1;
        assert_eq!(at(Btn::Chapters), Rect::new(48.0, 445.0, 36.0, 36.0));
        assert_eq!(at(Btn::Sleep).x, 94.0);
        assert_eq!(at(Btn::PlayPause), Rect::new(458.0, 441.0, 44.0, 44.0));
        assert_eq!(at(Btn::Info).right(), 912.0);
        assert_eq!(at(Btn::Previous).x, 480.0 - 74.0);
        assert_eq!(at(Btn::Next).right(), 480.0 + 74.0);
        let bare = layout_row(&RowOptions::default());
        let names: Vec<Btn> = bare.iter().map(|(b, _)| *b).collect();
        assert_eq!(names, [Btn::Sleep, Btn::Speed, Btn::PlayPause, Btn::Info]);
    }

    #[test]
    fn draws_the_scrubber_with_its_times_and_marker() {
        let mut h = Harness::new();
        let markers = [pley_core::model::Marker {
            kind: MarkerKind::Intro,
            start_ms: 2_000,
            end_ms: 8_000,
            ..Default::default()
        }];
        let chapters = [
            ChapterMark { title: None, start_ms: 0, end_ms: 10_000, image_url: None },
            ChapterMark { title: None, start_ms: 10_000, end_ms: 20_000, image_url: None },
        ];
        let f = h.idle(&mut |ui| {
            let g = ui.id("g");
            let id = ui.id("bar");
            let down = ui.id("down");
            let s = Scrub {
                position_ms: 3_000,
                duration_ms: 20_000,
                buffered_ms: 9_000,
                target_ms: None,
                chapters: &chapters,
                markers: &markers,
                up: Edge::Stop,
                up_opens_chapters: false,
                down,
            };
            ui.initial_focus(id);
            scrubber(ui, g, id, &s);
        });
        let t = texts(&f.list);
        assert_eq!(t, ["0:03", "−0:17", "Intro"]);
        // Two chapters: the track in two pieces; the first also buffered and played (both end
        // before the second starts); then the thumb.
        let fills = f.list.cmds.iter().filter(|c| matches!(c, Cmd::Fill { .. })).count();
        assert_eq!(fills, 3 + 1 + 1);
    }
}
