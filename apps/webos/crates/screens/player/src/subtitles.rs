//! Subtitles over the picture (`TvPlayerScreen.kt`'s caption layers, the TV app's
//! `ui/player/subtitles.tsx`): text cues from pley-subs' presenter in the look and place its
//! style gives (white bold text with a black outline, or on a dark box; lifted above the
//! controls while they show), and picture subtitles as the raw pictures their frames decode to,
//! where their frames put them.

use std::sync::Arc;

use pley_base::{Align, Point, Rect, TextLayout, TextOpts, TextStyle};
use pley_core::Settings;
use pley_core::store::{SubtitleBackground, SubtitleFont, SubtitlePosition, SubtitleSize};
use pley_subs::{CueGroup, Shown, Span, SubtitleSettings, TextCue, TextLayer, cue_align, cue_group, picture_rect};
use pley_ui::Ui;
use pley_ui::theme;

/// The rectangle the picture fills: the whole canvas (LG's video plane fills the screen).
pub const VIDEO: Rect = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);

/// The subtitle look the settings ask for, in pley-subs' terms.
pub fn look_of(settings: &Settings) -> SubtitleSettings {
    SubtitleSettings {
        size: match settings.subtitle_size {
            SubtitleSize::Small => pley_subs::SubtitleSize::Small,
            SubtitleSize::Normal => pley_subs::SubtitleSize::Normal,
            SubtitleSize::Large => pley_subs::SubtitleSize::Large,
            SubtitleSize::Huge => pley_subs::SubtitleSize::Largest,
        },
        boxed: settings.subtitle_background == SubtitleBackground::DarkBox,
        font: font_of(settings.subtitle_font),
        position: match settings.subtitle_position {
            SubtitlePosition::Bottom => pley_subs::SubtitlePosition::Bottom,
            SubtitlePosition::Raised => pley_subs::SubtitlePosition::Raised,
            SubtitlePosition::Higher => pley_subs::SubtitlePosition::Higher,
            SubtitlePosition::Highest => pley_subs::SubtitlePosition::Highest,
        },
    }
}

/// pley-subs' name for a font of the settings.
pub fn font_of(font: SubtitleFont) -> pley_subs::SubtitleFont {
    match font {
        SubtitleFont::Sans => pley_subs::SubtitleFont::Default,
        SubtitleFont::Serif => pley_subs::SubtitleFont::Serif,
        SubtitleFont::Monospace => pley_subs::SubtitleFont::Monospace,
        SubtitleFont::Casual => pley_subs::SubtitleFont::Casual,
    }
}

/// The fonts the Subtitles panel offers: Sans serif, Serif and Monospace, and Casual only where
/// the TV draws it differently from the default (`ScreenCaptions.kt`; pley-subs' `fonts_here`).
pub fn fonts_offered(ui: &Ui) -> Vec<SubtitleFont> {
    let measure = |family: &pley_base::Family| {
        let style = TextStyle { family: family.clone(), ..TextStyle::new(48.0, 60.0, pley_base::Weight::Bold) };
        let opts = TextOpts { max_width: None, ..TextOpts::default() };
        Some(ui.measure(pley_subs::style::FONT_SAMPLE, &style, &opts).w)
    };
    let here = pley_subs::fonts_here(measure);
    let mut out = vec![SubtitleFont::Sans, SubtitleFont::Serif, SubtitleFont::Monospace];
    if here.contains(&pley_subs::SubtitleFont::Casual) {
        out.push(SubtitleFont::Casual);
    }
    out
}

/// One line of a cue as drawn: pieces of text side by side (one piece unless its spans differ
/// in style), or one piece the shaper wrapped into several lines.
struct Block {
    pieces: Vec<Piece>,
    /// The lines' boxes, relative to the block's top-left: (x, y, width).
    lines: Vec<(f32, f32, f32)>,
    height: f32,
}

struct Piece {
    layout: Arc<TextLayout>,
    x: f32,
    underline: bool,
}

/// Draws what `shown` holds: pictures where their frames put them, and text cues in `look`,
/// lifted `lift` of the way above the controls (0 hidden, 1 shown).
pub fn draw(ui: &mut Ui, shown: &Shown, look: &SubtitleSettings, lift: f32) {
    for p in &shown.pictures {
        ui.raw(picture_rect(p, VIDEO), p.image.clone(), p.key);
    }
    if shown.text.is_empty() {
        return;
    }
    let layer = look.text_layer(VIDEO, lift);
    let (top, bottom): (Vec<&Arc<TextCue>>, Vec<&Arc<TextCue>>) =
        shown.text.iter().partition(|c| cue_group(c) == CueGroup::Top);
    let mut y = layer.top_y;
    for cue in top {
        y += draw_cue(ui, &layer, cue, y) + layer.cue_gap;
    }
    let laid: Vec<(Vec<Block>, f32)> = bottom.iter().map(|c| lay_out(ui, &layer, c)).collect();
    let total: f32 = laid.iter().map(|(_, h)| h).sum::<f32>() + layer.cue_gap * laid.len().saturating_sub(1) as f32;
    let mut y = layer.area.bottom() - total;
    for (blocks, h) in laid {
        paint(ui, &layer, &blocks, y);
        y += h + layer.cue_gap;
    }
}

fn draw_cue(ui: &mut Ui, layer: &TextLayer, cue: &TextCue, y: f32) -> f32 {
    let (blocks, h) = lay_out(ui, layer, cue);
    paint(ui, layer, &blocks, y);
    h
}

/// A cue's lines laid out in the layer's width, aligned as the cue asks; and their height.
fn lay_out(ui: &Ui, layer: &TextLayer, cue: &TextCue) -> (Vec<Block>, f32) {
    let width = layer.area.w.max(1.0);
    let align = cue_align(cue);
    let line_h = layer.style.line_height;
    let mut blocks = Vec::with_capacity(cue.lines.len());
    let mut height = 0.0;
    for line in &cue.lines {
        let spans = merged(line);
        if spans.iter().all(|s| s.text.trim().is_empty()) {
            continue;
        }
        let block = if spans.len() == 1 {
            wrapped(ui, &layer.span_style(&spans[0]), &spans[0].text, spans[0].underline, width, align)
        } else {
            beside(ui, layer, &spans, width, align, line_h)
        };
        height += block.height;
        blocks.push(block);
    }
    (blocks, height)
}

/// One style for the whole line: the shaper wraps and aligns it.
fn wrapped(ui: &Ui, style: &TextStyle, text: &str, underline: bool, width: f32, align: Align) -> Block {
    let layout = ui.layout(text, style, &TextOpts { ellipsis: false, ..TextOpts::wrap(width, 0).aligned(align) });
    let lines = layout.lines.iter().map(|l| (l.x, l.top, l.width)).collect();
    let height = layout.size.h;
    Block { pieces: vec![Piece { layout, x: 0.0, underline }], lines, height }
}

/// Spans of different styles side by side on one line; a line too long for that is wrapped in
/// the plain style instead (rare: styles that change mid-line in a long line).
fn beside(ui: &Ui, layer: &TextLayer, spans: &[Span], width: f32, align: Align, line_h: f32) -> Block {
    let free = TextOpts { max_width: None, ellipsis: false, ..TextOpts::default() };
    let layouts: Vec<Arc<TextLayout>> = spans.iter().map(|s| ui.layout(&s.text, &layer.span_style(s), &free)).collect();
    let total: f32 = layouts.iter().map(|l| l.size.w).sum();
    if total > width {
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        return wrapped(ui, &layer.style, &text, false, width, align);
    }
    let start = match align {
        Align::Start => 0.0,
        Align::Center => (width - total) / 2.0,
        Align::End => width - total,
    };
    let mut x = start;
    let mut pieces = Vec::with_capacity(spans.len());
    for (s, l) in spans.iter().zip(layouts) {
        let w = l.size.w;
        pieces.push(Piece { layout: l, x, underline: s.underline });
        x += w;
    }
    Block { pieces, lines: vec![(start, 0.0, total)], height: line_h }
}

/// Adjacent spans of the same look joined, so a line in one style is laid out whole.
fn merged(line: &[Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(line.len());
    for s in line {
        match out.last_mut() {
            Some(last) if last.italic == s.italic && last.underline == s.underline => last.text.push_str(&s.text),
            _ => out.push(s.clone()),
        }
    }
    out
}

/// The outline's eight copies: around the glyphs, the diagonals a little closer, as an 8-way
/// text shadow draws it (the TV app's `text-shadow`; Media3 strokes the glyphs' edge).
const OUTLINE_DIRS: [(f32, f32); 8] =
    [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.72, 0.72), (-0.72, 0.72), (0.72, -0.72), (-0.72, -0.72)];

fn paint(ui: &mut Ui, layer: &TextLayer, blocks: &[Block], top: f32) {
    let x0 = layer.area.x;
    let line_h = layer.style.line_height;
    let mut y = top;
    for b in blocks {
        if let Some(cue_box) = layer.cue_box {
            for &(lx, ly, lw) in &b.lines {
                let r = Rect::new(x0 + lx - cue_box.pad_x, y + ly, lw + 2.0 * cue_box.pad_x, line_h);
                ui.fill(r, cue_box.color, cue_box.radius);
            }
        }
        for p in &b.pieces {
            let at = Point::new(x0 + p.x, y);
            if let Some(o) = layer.outline {
                for (dx, dy) in OUTLINE_DIRS {
                    ui.draw_text(Point::new(at.x + dx * o.width, at.y + dy * o.width), p.layout.clone(), o.color);
                }
            }
            ui.draw_text(at, p.layout.clone(), layer.color);
            if p.underline {
                let (below, thick) = layer.underline;
                for l in &p.layout.lines {
                    let r = Rect::new(at.x + l.x, at.y + l.baseline + below, l.width, thick);
                    if let Some(o) = layer.outline {
                        ui.fill(r.expand(o.width), o.color, 0.0);
                    }
                    ui.fill(r, layer.color, 0.0);
                }
            }
        }
        y += b.height;
    }
}

#[cfg(test)]
mod tests {
    use pley_base::{Cmd, Color, RgbaImage, Size};
    use pley_subs::{PictureCue, parse_vtt};
    use pley_ui::testing::Harness;

    use super::*;

    fn shown(vtt: &str) -> Shown {
        let cues = parse_vtt(vtt).into_iter().map(Arc::new).collect();
        Shown { text: cues, pictures: Vec::new(), key: 1 }
    }

    fn texts(list: &pley_base::DisplayList) -> Vec<(String, Point, Color)> {
        list.cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::Text { origin, layout, color } => Some((layout.text.to_string(), *origin, *color)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn draws_text_outlined_above_the_bottom_and_lifted_with_the_controls() {
        let mut h = Harness::new();
        let s = shown("WEBVTT\n\n00:01.000 --> 00:02.000\nHello there\n");
        let look = SubtitleSettings::default();
        let f = h.idle(&mut |ui| draw(ui, &s, &look, 0.0));
        let t = texts(&f.list);
        // Eight outline copies in black, then the text in white.
        assert_eq!(t.len(), 9);
        assert!(t[..8].iter().all(|(_, _, c)| *c == Color::BLACK));
        let (text, at, color) = &t[8];
        assert_eq!((text.as_str(), *color), ("Hello there", Color::WHITE));
        let line_h = 0.048 * 540.0 * 1.25;
        assert!((at.y + line_h - (540.0 - 40.0)).abs() < 0.01, "{at:?}");
        let f = h.idle(&mut |ui| draw(ui, &s, &look, 1.0));
        let lifted = texts(&f.list)[8].1;
        assert!((lifted.y + line_h - (540.0 - 153.0)).abs() < 0.01, "{lifted:?}");
    }

    #[test]
    fn stacks_cues_and_draws_a_box_when_chosen() {
        let mut h = Harness::new();
        let s = shown("WEBVTT\n\n00:01.000 --> 00:03.000\nFirst\n\n00:01.500 --> 00:03.000\nSecond\nline\n");
        let look = SubtitleSettings { boxed: true, ..SubtitleSettings::default() };
        let f = h.idle(&mut |ui| draw(ui, &s, &look, 0.0));
        // The outline stays with the box (Media3's look): three lines of white text over their
        // copies, the first cue above the second.
        let t: Vec<_> = texts(&f.list).into_iter().filter(|(_, _, c)| *c == Color::WHITE).collect();
        let names: Vec<&str> = t.iter().map(|(s, _, _)| s.as_str()).collect();
        assert_eq!(names, ["First", "Second", "line"]);
        assert!(t[0].1.y < t[1].1.y && t[1].1.y < t[2].1.y);
        let boxes = f.list.cmds.iter().filter(|c| matches!(c, Cmd::Fill { color, .. } if color.a == 0xA8)).count();
        assert_eq!(boxes, 3);
    }

    #[test]
    fn places_cues_asked_at_the_top_and_mixes_styles_on_a_line() {
        let mut h = Harness::new();
        let s = shown("WEBVTT\n\n00:01.000 --> 00:03.000 line:0\nUp <i>here</i>\n");
        let f = h.idle(&mut |ui| draw(ui, &s, &SubtitleSettings::default(), 1.0));
        let t: Vec<_> = texts(&f.list).into_iter().filter(|(_, _, c)| *c == Color::WHITE).collect();
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].0.as_str(), t[1].0.as_str()), ("Up ", "here"));
        // At the top of the safe area, not lifted.
        assert_eq!(t[0].1.y, 27.0);
        assert!(t[1].1.x > t[0].1.x);
    }

    #[test]
    fn draws_pictures_where_their_frame_puts_them() {
        let mut h = Harness::new();
        let cue = PictureCue {
            start_ms: 0,
            end_ms: None,
            image: Arc::new(RgbaImage::new(360, 60)),
            rect: Rect::new(180.0, 480.0, 360.0, 60.0),
            plane: Size::new(720.0, 576.0),
            key: 7,
        };
        let s = Shown { text: Vec::new(), pictures: vec![cue], key: 2 };
        let f = h.idle(&mut |ui| draw(ui, &s, &SubtitleSettings::default(), 1.0));
        let raw: Vec<(Rect, u64)> = f
            .list
            .cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::Raw { rect, key, .. } => Some((*rect, *key)),
                _ => None,
            })
            .collect();
        assert_eq!(raw, [(Rect::new(240.0, 450.0, 480.0, 56.25), 7)]);
    }

    #[test]
    fn maps_the_settings_to_the_look() {
        let s = Settings {
            subtitle_size: SubtitleSize::Huge,
            subtitle_background: SubtitleBackground::DarkBox,
            subtitle_font: SubtitleFont::Monospace,
            subtitle_position: SubtitlePosition::Highest,
            ..Settings::default()
        };
        let look = look_of(&s);
        assert_eq!(look.size, pley_subs::SubtitleSize::Largest);
        assert!(look.boxed);
        assert_eq!(look.font, pley_subs::SubtitleFont::Monospace);
        assert_eq!(look.position, pley_subs::SubtitlePosition::Highest);
    }
}
