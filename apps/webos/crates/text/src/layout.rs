//! Breaking shaped text into lines, fitting it to a width and a number of
//! lines, ordering each line for display and placing its glyphs.

use std::ops::Range;
use std::sync::Arc;

use pley_base::{Align, Glyph, Size, TextLayout, TextLine, TextOpts, TextStyle};
use unicode_bidi::{BidiClass, BidiInfo, ParagraphInfo, bidi_class};
use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::fonts::{Chain, FontSet};
use crate::shape::{self, Shaped};

/// Widths closer than this (dp) are equal: sums of advances drift.
const EPSILON: f32 = 1e-3;
/// Balancing stops narrowing once the width is known this closely (dp).
const BALANCE_PRECISION: f32 = 0.05;
/// The ellipsis a cut line ends with.
const ELLIPSIS: &str = "\u{2026}";

/// The bidirectional levels of a string (UAX #9), for strings that need them.
pub(crate) struct Bidi<'a> {
    info: Option<BidiInfo<'a>>,
}

impl<'a> Bidi<'a> {
    /// Left to right throughout (the ellipsis, strings without right-to-left text).
    const LTR: Bidi<'static> = Bidi { info: None };

    /// The levels of `text`; none at all when nothing in it runs right to left.
    pub(crate) fn new(text: &'a str) -> Bidi<'a> {
        // Most strings have nothing right-to-left in them and skip the algorithm.
        let rtl = text.chars().any(|c| {
            c >= '\u{590}'
                && matches!(
                    bidi_class(c),
                    BidiClass::R | BidiClass::AL | BidiClass::RLE | BidiClass::RLO | BidiClass::RLI | BidiClass::FSI
                )
        });
        Bidi { info: rtl.then(|| BidiInfo::new(text, None)) }
    }

    /// The embedding level of the character at a byte (odd: right to left).
    pub(crate) fn level(&self, byte: usize) -> u8 {
        self.info.as_ref().map_or(0, |info| info.levels[byte].number())
    }

    fn paragraph(&self, byte: usize) -> Option<&ParagraphInfo> {
        let info = self.info.as_ref()?;
        info.paragraphs.iter().find(|p| byte < p.range.end).or(info.paragraphs.last())
    }

    /// Whether the paragraph a byte is in runs right to left (its first strong
    /// character decides, as Android's `TextDirection.Content` does).
    fn rtl_paragraph(&self, byte: usize) -> bool {
        self.paragraph(byte).is_some_and(|p| p.level.is_rtl())
    }

    /// A line's runs of one level, in display order: whether each is right to
    /// left, and its bytes.
    fn visual_runs(&self, line: Range<usize>) -> Vec<(bool, Range<usize>)> {
        if line.is_empty() {
            return Vec::new();
        }
        match (&self.info, self.paragraph(line.start)) {
            (Some(info), Some(paragraph)) => {
                let (levels, runs) = info.visual_runs(paragraph, line);
                runs.into_iter().map(|run| (levels[run.start].is_rtl(), run)).collect()
            }
            _ => vec![(false, line)],
        }
    }
}

/// A hard line (text between mandatory breaks) and where it may wrap: break
/// positions after its start, ending with its end.
struct Paragraph {
    range: Range<usize>,
    breaks: Vec<usize>,
}

/// Splits `text` at its line breaks, keeping each paragraph's wrap points:
/// UAX #14 opportunities at cluster boundaries, never after a soft hyphen
/// (wrapping there would need a hyphen, and Pley never adds one).
fn paragraphs(text: &str, shaped: &Shaped) -> Vec<Paragraph> {
    let mut paragraphs = Vec::new();
    let mut start = 0;
    let mut breaks = Vec::new();
    for (at, kind) in linebreaks(text) {
        if kind == BreakOpportunity::Mandatory {
            breaks.push(at);
            paragraphs.push(Paragraph { range: start..at, breaks: std::mem::take(&mut breaks) });
            start = at;
        } else if shaped.starts[at] && !text[..at].ends_with('\u{AD}') {
            breaks.push(at);
        }
    }
    // A string ending in a line break has an empty last line, as on Android.
    let ends_broken = text
        .chars()
        .next_back()
        .is_some_and(|c| matches!(c, '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'));
    if paragraphs.is_empty() || ends_broken {
        paragraphs.push(Paragraph { range: start..text.len(), breaks: vec![text.len()] });
    }
    paragraphs
}

/// Measuring and breaking lines over a shaped string.
struct Lines<'a> {
    text: &'a str,
    shaped: &'a Shaped,
}

impl Lines<'_> {
    /// Where a line from `start` to `end` stops drawing: before its trailing
    /// spaces and line break, which hang past its edge.
    fn trim(&self, start: usize, end: usize) -> usize {
        let mut trimmed = end;
        for (i, c) in self.text[start..end].char_indices().rev() {
            if !(c.is_whitespace() || shape::is_skipped(c)) {
                break;
            }
            trimmed = start + i;
        }
        trimmed
    }

    /// A line's width without what hangs past its edge.
    fn width(&self, start: usize, end: usize) -> f32 {
        self.shaped.width(start, self.trim(start, end))
    }

    /// The furthest cluster boundary after `start` (and at most `end`) up to
    /// which the line fits in `max`; `start` itself when nothing does.
    fn fit(&self, start: usize, end: usize, max: f32) -> usize {
        let mut best = start;
        for at in start + 1..=end {
            if !self.shaped.starts[at] {
                continue;
            }
            if self.width(start, at) <= max + EPSILON {
                best = at;
            } else if self.shaped.width(start, at) > max + EPSILON {
                break;
            }
        }
        best
    }

    /// Greedy wrapping of a paragraph at `max`, stopping after `cap` lines. A
    /// word wider than the line is broken between two of its characters.
    fn wrap(&self, paragraph: &Paragraph, max: f32, cap: usize) -> Vec<Range<usize>> {
        let mut lines = Vec::new();
        let mut start = paragraph.range.start;
        let mut fits: Option<usize> = None;
        let mut i = 0;
        while i < paragraph.breaks.len() && lines.len() < cap {
            let at = paragraph.breaks[i];
            if at <= start {
                i += 1;
            } else if self.width(start, at) <= max + EPSILON {
                fits = Some(at);
                i += 1;
            } else if let Some(end) = fits.take() {
                lines.push(start..end);
                start = end;
            } else {
                // Not even the first word fits: break it, keeping at least one character a line.
                let mut end = self.fit(start, at, max);
                if end == start {
                    end = (start + 1..=at).find(|&b| self.shaped.starts[b]).unwrap_or(at);
                }
                lines.push(start..end);
                start = end;
            }
        }
        if lines.len() < cap {
            lines.push(start..paragraph.range.end);
        }
        lines
    }

    /// The paragraph wrapped into `count` lines (what greedy wrapping at `max`
    /// gave) at the narrowest width that still takes no more, so the lines
    /// come out as even as they can: titles' balanced breaking, Android's
    /// `LineBreak.Heading`.
    fn balance(&self, paragraph: &Paragraph, max: f32, count: usize) -> Vec<Range<usize>> {
        let (mut narrow, mut wide) = (0.0f32, max);
        while wide - narrow > BALANCE_PRECISION {
            let mid = (narrow + wide) / 2.0;
            if self.wrap(paragraph, mid, count + 1).len() <= count {
                wide = mid;
            } else {
                narrow = mid;
            }
        }
        self.wrap(paragraph, wide, usize::MAX)
    }
}

/// A line to draw: its text, and how its end is fitted.
struct Spec {
    range: Range<usize>,
    /// The end of the paragraph it is in (a clamped last line runs to it).
    paragraph_end: usize,
    /// End with "…" (cutting the text before it to make room).
    ellipsis: bool,
    /// Cut the text where the width ends (one line without an ellipsis).
    cut: bool,
}

/// A style's line box: the first face's ascent and descent in dp, the line
/// height, and where the baseline sits in it, with the glyphs centred
/// vertically as Android's `LineHeightStyle(Center, Trim.None)` centres them.
pub(crate) struct LineBox {
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) cap_height: f32,
    pub(crate) x_height: f32,
    pub(crate) height: f32,
    pub(crate) baseline: f32,
}

pub(crate) fn line_box(chain: &Chain, style: &TextStyle) -> LineBox {
    let size = style.size;
    let (ascent, descent, cap_height, x_height) = match chain.faces.first() {
        Some(first) => {
            let em = size / first.face.upem;
            let face = &first.face;
            (face.ascent * em, face.descent * em, face.cap_height * em, face.x_height * em)
        }
        // No face at all: Roboto's proportions keep the boxes right.
        None => (size * 0.928, size * 0.244, size * 0.711, size * 0.528),
    };
    let height = if style.line_height > 0.0 { style.line_height } else { ascent + descent };
    LineBox { ascent, descent, cap_height, x_height, height, baseline: (height - ascent - descent) / 2.0 + ascent }
}

/// Lays `text` out as the `TextShaper` contract describes.
pub(crate) fn lay_out(text: &str, style: &TextStyle, opts: &TextOpts, chain: &Chain, fonts: &FontSet) -> TextLayout {
    let bidi = Bidi::new(text);
    let shaped = shape::shape(text, &bidi, chain, fonts, style);
    let lines = Lines { text, shaped: &shaped };
    let paragraphs = paragraphs(text, &shaped);
    let max_width = opts.max_width.filter(|w| w.is_finite()).map(|w| w.max(0.0));
    let max_lines = if opts.max_lines == 0 { usize::MAX } else { usize::from(opts.max_lines) };

    let mut specs: Vec<Spec> = Vec::new();
    let mut truncated = false;
    if max_lines == 1 {
        // One line is never wrapped: it is cut where the width ends.
        let first = &paragraphs[0];
        let overflow = max_width.is_some_and(|w| lines.width(first.range.start, first.range.end) > w + EPSILON);
        truncated = overflow || paragraphs.len() > 1;
        specs.push(Spec {
            range: first.range.clone(),
            paragraph_end: first.range.end,
            ellipsis: truncated && opts.ellipsis,
            cut: overflow,
        });
    } else {
        for paragraph in &paragraphs {
            let room = max_lines - specs.len();
            let mut wrapped = match max_width {
                Some(w) => lines.wrap(paragraph, w, room.saturating_add(1)),
                None => vec![paragraph.range.clone()],
            };
            if opts.balance
                && wrapped.len() > 1
                && wrapped.len() <= room
                && let Some(w) = max_width
            {
                wrapped = lines.balance(paragraph, w, wrapped.len());
            }
            let end = paragraph.range.end;
            specs.extend(wrapped.into_iter().map(|range| Spec {
                range,
                paragraph_end: end,
                ellipsis: false,
                cut: false,
            }));
            if specs.len() > max_lines {
                break;
            }
        }
        if specs.len() > max_lines {
            truncated = true;
            specs.truncate(max_lines);
            if opts.ellipsis
                && let Some(last) = specs.last_mut()
            {
                // Android's clamped last line holds as much of the rest of its
                // paragraph as fits before the "…", not just its own words.
                last.range.end = last.paragraph_end;
                last.ellipsis = true;
            }
        }
    }

    let ellipsis = specs.iter().any(|s| s.ellipsis).then(|| shape::shape(ELLIPSIS, &Bidi::LTR, chain, fonts, style));
    let ellipsis_width = ellipsis.as_ref().map_or(0.0, Shaped::total);
    let line_box = line_box(chain, style);

    let mut glyphs: Vec<Glyph> = Vec::new();
    let mut placed: Vec<(TextLine, bool)> = Vec::with_capacity(specs.len());
    for (i, spec) in specs.iter().enumerate() {
        let start = spec.range.start;
        let mut end = spec.range.end;
        if let Some(max) = max_width {
            if spec.ellipsis {
                end = lines.fit(start, end, max - ellipsis_width);
            } else if spec.cut {
                end = lines.fit(start, end, max);
            }
        }
        let end = lines.trim(start, end);
        let top = i as f32 * line_box.height;
        let baseline = top + line_box.baseline;
        let rtl = bidi.rtl_paragraph(start);
        let first = glyphs.len();
        let mut pen = 0.0;
        // The ellipsis ends the line in its paragraph's direction.
        let ellipsis = ellipsis.as_ref().filter(|_| spec.ellipsis);
        if rtl && let Some(e) = ellipsis {
            place(&mut glyphs, e, &Bidi::LTR, 0..ELLIPSIS.len(), baseline, style.size, &mut pen);
        }
        place(&mut glyphs, &shaped, &bidi, start..end, baseline, style.size, &mut pen);
        if !rtl && let Some(e) = ellipsis {
            place(&mut glyphs, e, &Bidi::LTR, 0..ELLIPSIS.len(), baseline, style.size, &mut pen);
        }
        let line = TextLine { glyphs: first as u32..glyphs.len() as u32, x: 0.0, width: pen, top, baseline };
        placed.push((line, rtl));
    }

    // Lines align within the width asked for, or within the widest line.
    let widest = placed.iter().map(|(line, _)| line.width).fold(0.0, f32::max);
    let flush_left = |rtl: bool| matches!((opts.align, rtl), (Align::Start, false) | (Align::End, true));
    let width = match max_width {
        Some(w) if !placed.iter().all(|&(_, rtl)| flush_left(rtl)) => w,
        _ => widest,
    };
    let lines: Vec<TextLine> = placed
        .into_iter()
        .map(|(mut line, rtl)| {
            let free = (width - line.width).max(0.0);
            line.x = match opts.align {
                Align::Center => free / 2.0,
                _ if flush_left(rtl) => 0.0,
                _ => free,
            };
            for glyph in &mut glyphs[line.glyphs.start as usize..line.glyphs.end as usize] {
                glyph.x += line.x;
            }
            line
        })
        .collect();
    let height = lines.len() as f32 * line_box.height;
    TextLayout { text: Arc::from(text), size: Size::new(width, height), lines, glyphs, truncated }
}

/// Places the glyphs of `line` (bytes of the shaped string) in display order
/// from `pen`, on `baseline`, advancing `pen`.
fn place(
    out: &mut Vec<Glyph>,
    shaped: &Shaped,
    bidi: &Bidi,
    line: Range<usize>,
    baseline: f32,
    size: f32,
    pen: &mut f32,
) {
    for (rtl, range) in bidi.visual_runs(line) {
        let first = shaped.runs.partition_point(|run| run.range.end <= range.start);
        let count = shaped.runs[first..].iter().take_while(|run| run.range.start < range.end).count();
        let runs = &shaped.runs[first..first + count];
        let mut place_run = |run: &shape::Run| {
            for glyph in &shaped.glyphs[run.glyphs.clone()] {
                if range.contains(&glyph.cluster) {
                    out.push(Glyph { font: run.font, id: glyph.id, x: *pen + glyph.dx, y: baseline - glyph.dy, size });
                    *pen += glyph.advance;
                }
            }
        };
        if rtl {
            runs.iter().rev().for_each(&mut place_run);
        } else {
            runs.iter().for_each(&mut place_run);
        }
    }
}
