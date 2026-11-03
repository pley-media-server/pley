//! Text: the styles the design system uses, a string laid out into lines of
//! placed glyphs, and the shaper that lays it out (`pley-gfx`'s).
//!
//! Layout happens in dp, so the interface can measure and place text without
//! knowing the screen's scale; the renderer rasterises the glyphs at device
//! pixels.

use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use crate::geom::Size;

/// The only weights the TV design uses (Fire OS lacks the others, and the LG
/// app follows the Android TV app).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weight {
    Regular,
    Medium,
    Bold,
}

impl Weight {
    /// The CSS / OpenType weight number.
    pub fn value(self) -> u16 {
        match self {
            Weight::Regular => 400,
            Weight::Medium => 500,
            Weight::Bold => 700,
        }
    }
}

/// Which face draws a string.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// Roboto, shipped with the app: every interface string.
    Sans,
    /// A monospace face: the sign-in code.
    Mono,
    /// A face asked for by name (a subtitle font chosen in Settings); the
    /// shaper falls back to `Sans` when the TV has no such face.
    Named(Arc<str>),
}

/// How a string looks: a type style of the design system, or a subtitle style.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: Family,
    /// Font size in dp.
    pub size: f32,
    /// Height of each line in dp; glyphs are centred in it vertically, as
    /// Android's `LineHeightStyle(Center, Trim.None)` does.
    pub line_height: f32,
    pub weight: Weight,
    pub italic: bool,
    /// Letter spacing in dp, added after every glyph (the design's tracking).
    pub tracking: f32,
}

impl TextStyle {
    pub fn new(size: f32, line_height: f32, weight: Weight) -> TextStyle {
        TextStyle { family: Family::Sans, size, line_height, weight, italic: false, tracking: 0.0 }
    }

    pub fn with_weight(&self, weight: Weight) -> TextStyle {
        TextStyle { weight, ..self.clone() }
    }

    pub fn with_tracking(&self, tracking: f32) -> TextStyle {
        TextStyle { tracking, ..self.clone() }
    }
}

// Styles key the shaper's cache; floats compare by their bits, which is what a
// cache wants (the same constant always gives the same key).
impl Eq for TextStyle {}

impl Hash for TextStyle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.family.hash(state);
        self.size.to_bits().hash(state);
        self.line_height.to_bits().hash(state);
        self.weight.hash(state);
        self.italic.hash(state);
        self.tracking.to_bits().hash(state);
    }
}

/// Horizontal alignment of each line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// How a string is fitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextOpts {
    /// Wrap at this width; with `max_lines` 1, cut the line here instead.
    pub max_width: Option<f32>,
    /// At most this many lines; 0 for as many as it takes.
    pub max_lines: u16,
    /// Lines are aligned within `max_width` (or the widest line without one).
    pub align: Align,
    /// End a cut last line with "…".
    pub ellipsis: bool,
    /// Even out the lengths of wrapped lines, as titles do (Android's
    /// `LineBreak.Heading`).
    pub balance: bool,
}

impl Default for TextOpts {
    fn default() -> TextOpts {
        TextOpts { max_width: None, max_lines: 1, align: Align::Start, ellipsis: true, balance: false }
    }
}

impl TextOpts {
    /// One line, cut with "…" at `width`.
    pub fn line(width: f32) -> TextOpts {
        TextOpts { max_width: Some(width), ..TextOpts::default() }
    }

    /// Up to `lines` lines wrapped at `width`, the last cut with "…".
    pub fn wrap(width: f32, lines: u16) -> TextOpts {
        TextOpts { max_width: Some(width), max_lines: lines, ..TextOpts::default() }
    }

    pub fn aligned(self, align: Align) -> TextOpts {
        TextOpts { align, ..self }
    }

    pub fn balanced(self) -> TextOpts {
        TextOpts { balance: true, ..self }
    }
}

impl Eq for TextOpts {}

impl Hash for TextOpts {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.max_width.map(f32::to_bits).hash(state);
        self.max_lines.hash(state);
        self.align.hash(state);
        self.ellipsis.hash(state);
        self.balance.hash(state);
    }
}

/// One glyph, placed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// Which of the shaper's fonts (an index the renderer understands).
    pub font: u16,
    /// The glyph's id in that font.
    pub id: u16,
    /// Its origin on the baseline, in dp from the layout's top-left corner.
    pub x: f32,
    pub y: f32,
    /// Its font size in dp.
    pub size: f32,
}

/// One line of a layout.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// The line's glyphs in [`TextLayout::glyphs`].
    pub glyphs: Range<u32>,
    /// Where the line starts after alignment, in dp from the layout's left.
    pub x: f32,
    /// The line's width in dp.
    pub width: f32,
    /// The line box's top, in dp from the layout's top.
    pub top: f32,
    /// The baseline, in dp from the layout's top.
    pub baseline: f32,
}

/// A string shaped and broken into lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    /// The string laid out, as given (tests and the screen reader read it).
    pub text: Arc<str>,
    /// Width: `max_width` when lines are centred or end-aligned within it,
    /// otherwise the widest line. Height: the lines' boxes.
    pub size: Size,
    pub lines: Vec<TextLine>,
    pub glyphs: Vec<Glyph>,
    /// Whether some of the text didn't fit (lines dropped or a line cut).
    pub truncated: bool,
}

impl TextLayout {
    /// The first line's baseline, for aligning text with other text.
    pub fn first_baseline(&self) -> f32 {
        self.lines.first().map_or(0.0, |l| l.baseline)
    }
}

/// Lays text out. Implemented by `pley-gfx` over the app's fonts and the TV's;
/// a stub in headless tests.
pub trait TextShaper {
    /// `text` laid out in `style` within `opts`. Implementations cache: the
    /// same arguments give the same layout, so callers may ask every frame.
    fn layout(&self, text: &str, style: &TextStyle, opts: &TextOpts) -> Arc<TextLayout>;
}
