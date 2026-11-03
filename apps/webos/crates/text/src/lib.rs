//! Text for Pley for LG TV: fonts (the bundled Roboto and the system's own faces
//! for other scripts), shaping, bidirectional order, line breaking and fitting,
//! and glyph rasterisation at device pixels for the renderer's atlas.
//!
//! [`FontSet`] finds the faces; [`TextEngine`] lays strings out for the
//! interface (it implements [`pley_base::TextShaper`], so layouts are in dp and
//! cached) and rasterises the glyphs those layouts name.
//!
//! How a string is laid out follows the Android TV app's text (`PleyType.kt`):
//! every line box is `line_height` tall with its glyphs centred in it
//! (`LineHeightStyle(Center, Trim.None)`), titles can be balanced
//! (`LineBreak.Heading`), lines wrap at UAX #14 opportunities and never get a
//! hyphen added, letter spacing is spread half before and half after each
//! character, and a clamped last line keeps as much of the rest of its
//! paragraph as fits before its "…", as Android's `StaticLayout` does.
//!
//! Every face of a string shares the first face's baseline and line box, so
//! labels of one size line up whatever their script; glyph ink may reach past
//! the line box (Arabic marks, tall capitals), and the renderer doesn't clip
//! text to it.

mod cache;
mod engine;
mod families;
mod fonts;
mod layout;
mod raster;
mod shape;

pub use engine::{LineMetrics, TextEngine};
pub use fonts::{FaceInfo, FontSet, SYNTHETIC_ITALIC, system_font_dirs};
pub use raster::{GlyphBitmap, subpixel_position};

#[cfg(test)]
mod tests;
