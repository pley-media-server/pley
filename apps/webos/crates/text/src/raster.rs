//! Glyph rasterisation: 8-bit coverage at device pixels, for the renderer's
//! glyph atlas.
//!
//! Outlines come from ttf-parser (the parser the shaper already uses, so a
//! face is parsed once) and are filled by `ab_glyph_rasterizer`: exact-area
//! anti-aliasing in plain `f32` arithmetic, with no tables, SIMD or hinting
//! interpreter, which keeps it fast and identical on the TV's Cortex-A9/A53
//! and on the desktop. TrueType quadratics and CFF cubics are both drawn
//! directly, without flattening beforehand.

use ab_glyph_rasterizer::{Point, Rasterizer, point};
use rustybuzz::ttf_parser::{GlyphId, OutlineBuilder};

use crate::fonts::Face;

/// The slant of a synthetic italic: Android's fake italic, the
/// `Paint.setTextSkewX(-0.25f)` its documentation gives for oblique text.
const ITALIC_SKEW: f32 = 0.25;
/// Glyphs larger than this are not drawn (no screen of the app asks for one;
/// the bound keeps a bad size from allocating without limit).
const MAX_SIZE_PX: f32 = 512.0;

/// A glyph's coverage, as the atlas stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphBitmap {
    /// The bitmap's size in device pixels: the ink's bounds, rounded out.
    pub width: u32,
    pub height: u32,
    /// From the glyph's pen position on the baseline to the bitmap's top-left
    /// corner, in device pixels, y down (`top` is negative for ink above the
    /// baseline).
    pub left: i32,
    pub top: i32,
    /// 8-bit coverage, row-major, top row first, `width × height` bytes.
    pub coverage: Vec<u8>,
}

/// Splits a pen position (device pixels) into the whole pixel a glyph is drawn
/// at and the quarter-pixel offset it is rasterised with (0, 0.25, 0.5 or
/// 0.75): pass that offset to `TextEngine::rasterize` and key the atlas by it.
pub fn subpixel_position(x: f32) -> (i32, f32) {
    let quarters = (x * 4.0).round();
    let whole = (quarters / 4.0).floor();
    (whole as i32, quarters / 4.0 - whole)
}

/// Rasterises a glyph of `face` at `size_px` pixels per em, its pen moved
/// right by `subpixel_x` (rounded to a quarter pixel), slanted for a
/// synthetic italic. `None` for a glyph without ink (a space) or a size out
/// of bounds.
pub(crate) fn rasterize(face: &Face, glyph: u16, size_px: f32, subpixel_x: f32, italic: bool) -> Option<GlyphBitmap> {
    if !(size_px > 0.0 && size_px <= MAX_SIZE_PX) || !subpixel_x.is_finite() {
        return None;
    }
    let shift = (subpixel_x * 4.0).round() / 4.0;
    let scale = size_px / face.upem;
    let mut outline = Outline::new(scale, if italic { ITALIC_SKEW * scale } else { 0.0 }, shift);
    face.ttf.outline_glyph(GlyphId(glyph), &mut outline)?;
    outline.close();
    if outline.segments.is_empty() {
        return None;
    }

    let left = outline.min.x.floor();
    let top = outline.min.y.floor();
    let width = (outline.max.x.ceil() - left) as usize;
    let height = (outline.max.y.ceil() - top) as usize;
    if width == 0 || height == 0 {
        return None;
    }
    let at = |p: Point| point(p.x - left, p.y - top);
    let mut rasterizer = Rasterizer::new(width, height);
    for segment in &outline.segments {
        match *segment {
            Segment::Line(a, b) => rasterizer.draw_line(at(a), at(b)),
            Segment::Quad(a, b, c) => rasterizer.draw_quad(at(a), at(b), at(c)),
            Segment::Cubic(a, b, c, d) => rasterizer.draw_cubic(at(a), at(b), at(c), at(d)),
        }
    }
    let mut coverage = vec![0u8; width * height];
    // Overlapping contours (static cuts of variable fonts keep them) add up past 1.
    rasterizer.for_each_pixel(|i, alpha| coverage[i] = (alpha.min(1.0) * 255.0 + 0.5) as u8);
    Some(GlyphBitmap { width: width as u32, height: height as u32, left: left as i32, top: top as i32, coverage })
}

#[derive(Clone, Copy)]
enum Segment {
    Line(Point, Point),
    Quad(Point, Point, Point),
    Cubic(Point, Point, Point, Point),
}

/// An outline in device pixels (y down), with its bounds.
struct Outline {
    scale: f32,
    /// x moves by this much per font unit of height (the synthetic italic's slant).
    skew: f32,
    shift: f32,
    segments: Vec<Segment>,
    start: Point,
    last: Point,
    min: Point,
    max: Point,
}

impl Outline {
    fn new(scale: f32, skew: f32, shift: f32) -> Outline {
        let far = f32::MAX;
        Outline {
            scale,
            skew,
            shift,
            segments: Vec::new(),
            start: point(0.0, 0.0),
            last: point(0.0, 0.0),
            min: point(far, far),
            max: point(-far, -far),
        }
    }

    /// A point in font units to device pixels; every control point is inside
    /// the bounds, and the curves stay within their control points.
    fn map(&mut self, x: f32, y: f32) -> Point {
        let p = point(x * self.scale + y * self.skew + self.shift, -y * self.scale);
        self.min = point(self.min.x.min(p.x), self.min.y.min(p.y));
        self.max = point(self.max.x.max(p.x), self.max.y.max(p.y));
        p
    }

    /// Closes the open contour, if any, back to its start.
    fn close(&mut self) {
        if self.last != self.start {
            self.segments.push(Segment::Line(self.last, self.start));
            self.last = self.start;
        }
    }
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = self.map(x, y);
        self.last = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.map(x, y);
        self.segments.push(Segment::Line(self.last, p));
        self.last = p;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c = self.map(x1, y1);
        let p = self.map(x, y);
        self.segments.push(Segment::Quad(self.last, c, p));
        self.last = p;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let c1 = self.map(x1, y1);
        let c2 = self.map(x2, y2);
        let p = self.map(x, y);
        self.segments.push(Segment::Cubic(self.last, c1, c2, p));
        self.last = p;
    }

    fn close(&mut self) {
        Outline::close(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpixel_positions_snap_to_quarters() {
        assert_eq!(subpixel_position(10.0), (10, 0.0));
        assert_eq!(subpixel_position(10.3), (10, 0.25));
        assert_eq!(subpixel_position(10.62), (10, 0.5));
        assert_eq!(subpixel_position(10.9), (11, 0.0));
        assert_eq!(subpixel_position(-0.3), (-1, 0.75));
    }
}
