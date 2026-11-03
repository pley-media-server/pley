//! Itemising a string into runs that one face draws in one direction and
//! script, and shaping each run with rustybuzz.

use std::ops::Range;

use pley_base::TextStyle;
use rustybuzz::ttf_parser::Tag;
use rustybuzz::{Direction, UnicodeBuffer};
use unicode_script::{Script, UnicodeScript};

use crate::fonts::{Chain, ChainFace, FontSet, NO_LIGATURES};
use crate::layout::Bidi;

/// Glyphs one face shaped in one direction.
pub(crate) struct Run {
    /// Its text, in bytes.
    pub(crate) range: Range<usize>,
    /// The face, with the synthetic-italic flag: the layout's `Glyph::font`.
    pub(crate) font: u16,
    /// Its glyphs in [`Shaped::glyphs`], in visual order (a right-to-left run's
    /// are reversed).
    pub(crate) glyphs: Range<usize>,
}

/// A shaped glyph, in dp.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapedGlyph {
    pub(crate) id: u16,
    /// The byte its cluster starts at.
    pub(crate) cluster: usize,
    pub(crate) advance: f32,
    pub(crate) dx: f32,
    /// How far it is raised (fonts measure y upwards).
    pub(crate) dy: f32,
}

/// A string shaped: its runs in logical order, and per byte what line breaking
/// measures with.
pub(crate) struct Shaped {
    pub(crate) runs: Vec<Run>,
    pub(crate) glyphs: Vec<ShapedGlyph>,
    /// For each byte and one past the end: the advance of all clusters before it.
    pub(crate) prefix: Vec<f32>,
    /// For each byte and one past the end: whether a cluster starts there, the
    /// only places a line may be cut.
    pub(crate) starts: Vec<bool>,
}

impl Shaped {
    /// The advance from one cluster boundary to another.
    pub(crate) fn width(&self, start: usize, end: usize) -> f32 {
        self.prefix[end] - self.prefix[start]
    }

    /// The whole string's advance.
    pub(crate) fn total(&self) -> f32 {
        self.prefix.last().copied().unwrap_or(0.0)
    }
}

/// A run before shaping.
struct Item {
    range: Range<usize>,
    level: u8,
    face: ChainFace,
    /// Its script, once a character of one appears.
    script: Option<Script>,
}

/// Shapes `text` with a style's faces, in the bidirectional levels given.
pub(crate) fn shape(text: &str, bidi: &Bidi, chain: &Chain, fonts: &FontSet, style: &TextStyle) -> Shaped {
    let items = itemize(text, bidi, chain, fonts);
    // Android switches optional ligatures off in letter-spaced text.
    let features = if style.tracking != 0.0 { NO_LIGATURES } else { 0 };
    let mut runs = Vec::with_capacity(items.len());
    let mut glyphs = Vec::with_capacity(text.len());
    for item in &items {
        let first = glyphs.len();
        shape_item(text, item, style, features, &mut glyphs);
        runs.push(Run { range: item.range.clone(), font: item.face.id, glyphs: first..glyphs.len() });
    }

    let mut starts = vec![false; text.len() + 1];
    let mut advance = vec![0.0f32; text.len() + 1];
    for glyph in &glyphs {
        starts[glyph.cluster] = true;
        advance[glyph.cluster] += glyph.advance;
    }
    // Characters outside every run (line ends, controls) are clusters with nothing to draw.
    for (i, c) in text.char_indices() {
        if is_skipped(c) {
            starts[i] = true;
        }
    }
    starts[text.len()] = true;
    let mut sum = 0.0;
    let prefix = advance
        .iter()
        .map(|a| {
            let before = sum;
            sum += a;
            before
        })
        .collect();
    Shaped { runs, glyphs, prefix, starts }
}

/// Characters that end a run and draw nothing: line breaks and other controls
/// (a tab is drawn as a space).
pub(crate) fn is_skipped(c: char) -> bool {
    (c.is_control() && c != '\t') || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Splits `text` into runs of one bidirectional level, one script and one face.
///
/// Each character goes to the first of the style's faces that has it, else to
/// fallback. Spaces, punctuation and digits (script Common) stay in the run
/// before them when its face has them, and combining marks (Inherited) always
/// do, so a word in another script keeps its spaces and accents.
fn itemize(text: &str, bidi: &Bidi, chain: &Chain, fonts: &FontSet) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut open = false;
    for (i, c) in text.char_indices() {
        let end = i + c.len_utf8();
        if is_skipped(c) {
            open = false;
            continue;
        }
        let level = bidi.level(i);
        let script = c.script();
        let weak = matches!(script, Script::Common | Script::Inherited | Script::Unknown);
        if weak
            && open
            && let Some(item) = items.last_mut()
            && item.level == level
            && (script == Script::Inherited || item.face.face.covers(c))
        {
            item.range.end = end;
            continue;
        }
        let Some(face) = pick(c, chain, fonts) else {
            open = false;
            continue;
        };
        if open
            && let Some(item) = items.last_mut()
            && item.level == level
            && item.face.id == face.id
            && item.face.features == face.features
            && (weak || item.script.is_none_or(|s| s == script))
        {
            if !weak {
                item.script = Some(script);
            }
            item.range.end = end;
            continue;
        }
        items.push(Item { range: i..end, level, face, script: (!weak).then_some(script) });
        open = true;
    }
    items
}

/// The face for a character: the first of the style's faces that has it,
/// else fallback's, else the style's first face (whose missing-glyph box then
/// shows). Invisible characters go to the first face without a search: the
/// shaper hides them whatever the face.
fn pick(c: char, chain: &Chain, fonts: &FontSet) -> Option<ChainFace> {
    if let Some(face) = chain.faces.iter().find(|f| f.face.covers(c)) {
        return Some(face.clone());
    }
    if !is_default_ignorable(c)
        && let Some(face) = fonts.fallback(c, chain.weight, chain.italic)
    {
        return Some(face);
    }
    chain.faces.first().cloned()
}

/// Unicode's Default_Ignorable_Code_Point: joiners, variation selectors,
/// direction marks, the soft hyphen and the like.
fn is_default_ignorable(c: char) -> bool {
    matches!(c,
        '\u{AD}' | '\u{34F}' | '\u{61C}' | '\u{115F}'..='\u{1160}' | '\u{17B4}'..='\u{17B5}'
        | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}' | '\u{FFA0}'
        | '\u{FFF0}'..='\u{FFF8}' | '\u{1BCA0}'..='\u{1BCA3}' | '\u{1D173}'..='\u{1D17A}'
        | '\u{E0000}'..='\u{E0FFF}')
}

fn shape_item(text: &str, item: &Item, style: &TextStyle, features: u8, out: &mut Vec<ShapedGlyph>) {
    let face = &item.face.face;
    let mut buffer = UnicodeBuffer::new();
    for (i, c) in text[item.range.clone()].char_indices() {
        // The interface has no tab stops: a tab is a space.
        buffer.add(if c == '\t' { ' ' } else { c }, (item.range.start + i) as u32);
    }
    // The text around the run lets Arabic join across a change of face.
    buffer.set_pre_context(&text[..item.range.start]);
    buffer.set_post_context(&text[item.range.end..]);
    let direction = if item.level % 2 == 1 { Direction::RightToLeft } else { Direction::LeftToRight };
    let script = item.script.map_or(rustybuzz::script::COMMON, |s| {
        rustybuzz::Script::from_iso15924_tag(Tag(s.as_iso15924_tag())).unwrap_or(rustybuzz::script::COMMON)
    });
    buffer.set_direction(direction);
    buffer.set_script(script);
    let plan = face.plan(direction, script, features | item.face.features);
    let shaped = rustybuzz::shape_with_plan(face.shaper(), &plan, buffer);

    let scale = style.size / face.upem;
    let first = out.len();
    out.extend(shaped.glyph_infos().iter().zip(shaped.glyph_positions()).map(|(info, pos)| ShapedGlyph {
        id: info.glyph_id as u16,
        cluster: info.cluster as usize,
        advance: pos.x_advance as f32 * scale,
        dx: pos.x_offset as f32 * scale,
        dy: pos.y_offset as f32 * scale,
    }));
    if style.tracking != 0.0 {
        track(&mut out[first..], style.tracking);
    }
}

/// Letter spacing as Android's text engine (Minikin) applies it: every cluster
/// that advances gets half the spacing before it and half after, so spaced
/// text stays centred on its ink. Marks and invisible characters get none.
fn track(glyphs: &mut [ShapedGlyph], tracking: f32) {
    let mut start = 0;
    while start < glyphs.len() {
        let cluster = glyphs[start].cluster;
        let end = start + glyphs[start..].iter().take_while(|g| g.cluster == cluster).count();
        let advance: f32 = glyphs[start..end].iter().map(|g| g.advance).sum();
        if advance != 0.0 {
            for glyph in &mut glyphs[start..end] {
                glyph.dx += tracking / 2.0;
            }
            glyphs[end - 1].advance += tracking;
        }
        start = end;
    }
}
