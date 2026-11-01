//! Turning a display list into quads: the CPU half of the renderer, free of GL
//! so it can be tested.
//!
//! Every command becomes axis-aligned quads (four vertices, two triangles) in
//! device pixels, appended to one vertex array; consecutive quads that draw
//! with the same program and texture form one batch, one draw call. Clips are
//! applied to the quads themselves (transforms only scale and move, so every
//! quad stays axis-aligned and clips exactly), so a clip never breaks a batch;
//! batches break only where the picture texture changes or video is punched
//! through.

use pley_base::{Cmd, Color, GradientStop, Icon, ImageId, Point, Rect, RgbaImage};

use crate::atlas::{Atlas, AtlasEntry, AtlasFull, AtlasKey};
use crate::glyphs::GlyphSource;
use crate::{icons, masks};

/// A colour premultiplied by its alpha, channels 0 to 255.
pub type Rgba = [f32; 4];

/// A rectangle in device pixels, by its edges.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DevRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl DevRect {
    /// The rectangle between its left, top, right and bottom edges.
    pub const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> DevRect {
        DevRect { x0, y0, x1, y1 }
    }

    /// Whether it covers no area.
    pub fn is_empty(&self) -> bool {
        !(self.x1 > self.x0 && self.y1 > self.y0)
    }

    /// Its width.
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }

    /// Its height.
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }

    /// The overlap of two rectangles (empty when they don't overlap).
    pub fn intersect(&self, o: &DevRect) -> DevRect {
        DevRect::new(self.x0.max(o.x0), self.y0.max(o.y0), self.x1.min(o.x1), self.y1.min(o.y1))
    }
}

/// A texture a picture draws from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TexRef {
    /// A picture of the cache.
    Image(ImageId),
    /// A picture the app made, by its key.
    Raw(u64),
}

/// How a batch's quads are drawn: which program and texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paint {
    /// Colour times atlas coverage: fills, gradients, outlines, text, icons.
    Atlas,
    /// A picture times its opacity.
    Picture(TexRef),
    /// A picture times its opacity and atlas coverage: a rounded picture's
    /// corners.
    PictureMasked(TexRef),
    /// Transparent black written with blending off, so the video plane under
    /// the interface shows.
    Punch,
}

/// One corner of a quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    /// Device pixels, y down.
    pub pos: [f32; 2],
    /// The picture's texture coordinates.
    pub uv: [f32; 2],
    /// The atlas's texture coordinates.
    pub mask: [f32; 2],
    /// Premultiplied colour.
    pub color: [u8; 4],
}

/// A run of quads drawn with one paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Batch {
    pub paint: Paint,
    /// The first quad (vertex `4 × first`).
    pub first: u32,
    pub quads: u32,
}

/// A frame's quads, four vertices each (top-left, top-right, bottom-left,
/// bottom-right), and its batches.
#[derive(Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub batches: Vec<Batch>,
}

impl Mesh {
    /// Empties it, keeping its memory for the next frame.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.batches.clear();
    }

    /// How many quads it holds.
    pub fn quads(&self) -> usize {
        self.vertices.len() / 4
    }

    fn push(&mut self, paint: Paint, quad: [Vertex; 4]) {
        let index = self.quads() as u32;
        match self.batches.last_mut() {
            Some(b) if b.paint == paint => b.quads += 1,
            _ => self.batches.push(Batch { paint, first: index, quads: 1 }),
        }
        self.vertices.extend_from_slice(&quad);
    }

    /// Drops every quad from `quads` on (a command that couldn't finish).
    fn truncate(&mut self, quads: usize) {
        self.vertices.truncate(quads * 4);
        while let Some(b) = self.batches.last_mut() {
            if b.first as usize >= quads {
                self.batches.pop();
            } else {
                b.quads = b.quads.min(quads as u32 - b.first);
                break;
            }
        }
    }
}

/// Picture textures, as the frame builder needs them.
pub trait Textures {
    /// The size of a cached picture's texture if it is resident (and records
    /// that it was drawn).
    fn image(&mut self, id: ImageId) -> Option<(u32, u32)>;
    /// The size of a made picture's texture, uploading it when its key is new.
    fn raw(&mut self, key: u64, image: &RgbaImage) -> Option<(u32, u32)>;
}

/// What building a frame draws on besides the display list.
pub struct Inputs<'a> {
    pub atlas: &'a mut Atlas,
    pub glyphs: &'a dyn GlyphSource,
    pub textures: &'a mut dyn Textures,
}

/// dp to device pixels: `p × s + (dx, dy)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xform {
    pub s: f32,
    pub dx: f32,
    pub dy: f32,
}

impl Xform {
    /// A point in device pixels.
    pub fn point(&self, p: Point) -> (f32, f32) {
        (p.x * self.s + self.dx, p.y * self.s + self.dy)
    }

    /// A rectangle in device pixels.
    pub fn rect(&self, r: Rect) -> DevRect {
        let (x0, y0) = self.point(r.origin());
        DevRect::new(x0, y0, x0 + r.w * self.s, y0 + r.h * self.s)
    }

    /// The transform inside a `PushTransform`: what follows is scaled by
    /// `scale` about `pivot`, then moved by `offset`, in this transform's
    /// coordinates.
    pub fn then(&self, offset: Point, scale: f32, pivot: Point) -> Xform {
        let ox = pivot.x * (1.0 - scale) + offset.x;
        let oy = pivot.y * (1.0 - scale) + offset.y;
        Xform { s: self.s * scale, dx: self.dx + ox * self.s, dy: self.dy + oy * self.s }
    }
}

/// Walks a display list into a [`Mesh`], keeping the clip, transform and
/// opacity stacks across calls so a frame interrupted by a full atlas resumes
/// where it stopped.
pub struct Builder {
    xforms: Vec<Xform>,
    clips: Vec<DevRect>,
    opacities: Vec<f32>,
    /// After an atlas reset that didn't make room: entries that still don't
    /// fit are left out instead of stopping the frame again.
    pub lenient: bool,
}

impl Builder {
    /// A builder for a `width` × `height` pixel surface at `px_per_dp`.
    pub fn new(width: u32, height: u32, px_per_dp: f32) -> Builder {
        Builder {
            xforms: vec![Xform { s: px_per_dp, dx: 0.0, dy: 0.0 }],
            clips: vec![DevRect::new(0.0, 0.0, width as f32, height as f32)],
            opacities: vec![1.0],
            lenient: false,
        }
    }

    fn xf(&self) -> Xform {
        *self.xforms.last().expect("the base transform stays")
    }

    fn clip(&self) -> DevRect {
        *self.clips.last().expect("the base clip stays")
    }

    fn opacity(&self) -> f32 {
        *self.opacities.last().expect("the base opacity stays")
    }

    /// Builds `cmds[from..]`. On a full atlas, drops the quads of the command
    /// it was in and returns that command's index: draw the mesh, reset the
    /// atlas and call again from there.
    pub fn run(&mut self, cmds: &[Cmd], from: usize, mesh: &mut Mesh, cx: &mut Inputs<'_>) -> Result<(), usize> {
        for (i, cmd) in cmds.iter().enumerate().skip(from) {
            let mark = mesh.quads();
            if self.cmd(cmd, mesh, cx).is_err() {
                mesh.truncate(mark);
                return Err(i);
            }
        }
        Ok(())
    }

    fn cmd(&mut self, cmd: &Cmd, mesh: &mut Mesh, cx: &mut Inputs<'_>) -> Result<(), AtlasFull> {
        match cmd {
            Cmd::PushClip { rect } => {
                let r = self.xf().rect(*rect).intersect(&self.clip());
                self.clips.push(r);
            }
            Cmd::PopClip => {
                if self.clips.len() > 1 {
                    self.clips.pop();
                }
            }
            Cmd::PushTransform { offset, scale, pivot } => {
                let next = self.xf().then(*offset, *scale, *pivot);
                self.xforms.push(next);
            }
            Cmd::PopTransform => {
                if self.xforms.len() > 1 {
                    self.xforms.pop();
                }
            }
            Cmd::PushOpacity(o) => {
                let next = self.opacity() * o.clamp(0.0, 1.0);
                self.opacities.push(next);
            }
            Cmd::PopOpacity => {
                if self.opacities.len() > 1 {
                    self.opacities.pop();
                }
            }
            _ if self.clip().is_empty() || self.xf().s <= 0.0 => {}
            Cmd::Fill { rect, color, radius } => {
                let c = premultiply(*color, self.opacity());
                if c[3] > 0.0 {
                    let xf = self.xf();
                    self.rounded(mesh, cx, xf.rect(*rect), radius * xf.s, Fill::Color(c))?;
                }
            }
            Cmd::Stroke { rect, color, radius, width } => {
                let c = premultiply(*color, self.opacity());
                if c[3] > 0.0 && *width > 0.0 {
                    self.stroke(mesh, cx, *rect, *radius, *width, c)?;
                }
            }
            Cmd::LinearGradient { rect, vertical, stops } => self.linear(mesh, cx.atlas, *rect, *vertical, stops),
            Cmd::Quad { rect, colors, radius } => {
                let o = self.opacity();
                let cs = colors.map(|c| premultiply(c, o));
                if cs.iter().any(|c| c[3] > 0.0) {
                    let xf = self.xf();
                    self.rounded(mesh, cx, xf.rect(*rect), radius * xf.s, Fill::Corners(cs))?;
                }
            }
            Cmd::RadialGradient { rect, center, radius, inner, outer } => {
                self.radial(mesh, cx.atlas, *rect, *center, *radius, *inner, *outer)
            }
            Cmd::Image { rect, image, src, radius, opacity } => {
                let o = opacity.clamp(0.0, 1.0) * self.opacity();
                if o > 0.0
                    && let Some((tw, th)) = cx.textures.image(*image)
                {
                    let uv = src_uv(*src, tw, th);
                    let xf = self.xf();
                    let c = [o * 255.0; 4];
                    self.rounded(mesh, cx, xf.rect(*rect), radius * xf.s, Fill::Picture(TexRef::Image(*image), c, uv))?;
                }
            }
            Cmd::Raw { rect, image, key, radius, opacity } => {
                let o = opacity.clamp(0.0, 1.0) * self.opacity();
                if o > 0.0 && cx.textures.raw(*key, image).is_some() {
                    let xf = self.xf();
                    let c = [o * 255.0; 4];
                    let uv = [0.0, 0.0, 1.0, 1.0];
                    self.rounded(mesh, cx, xf.rect(*rect), radius * xf.s, Fill::Picture(TexRef::Raw(*key), c, uv))?;
                }
            }
            Cmd::Text { origin, layout, color } => {
                let c = premultiply(*color, self.opacity());
                if c[3] > 0.0 {
                    self.text(mesh, cx, *origin, layout, c)?;
                }
            }
            Cmd::Icon { rect, icon, color } => {
                let c = premultiply(*color, self.opacity());
                if c[3] > 0.0 {
                    self.icon(mesh, cx, *rect, *icon, c)?;
                }
            }
            Cmd::Punch { rect } => {
                let r = self.xf().rect(*rect);
                let white = white(cx.atlas);
                emit(mesh, Paint::Punch, &self.clip(), r, [0.0; 4], white, [[0.0; 4]; 4]);
            }
        }
        Ok(())
    }

    /// A rectangle, its corners rounded by `radius` device pixels: flat pieces
    /// plus four corner pieces covered through a mask.
    fn rounded(
        &mut self,
        mesh: &mut Mesh,
        cx: &mut Inputs<'_>,
        r: DevRect,
        radius: f32,
        fill: Fill,
    ) -> Result<(), AtlasFull> {
        if r.is_empty() {
            return Ok(());
        }
        let clip = self.clip();
        let white = white(cx.atlas);
        let (body, corners, whole, uv) = match fill {
            Fill::Color(c) => (Paint::Atlas, Paint::Atlas, [c; 4], [0.0; 4]),
            Fill::Corners(cs) => (Paint::Atlas, Paint::Atlas, cs, [0.0; 4]),
            Fill::Picture(t, c, uv) => (Paint::Picture(t), Paint::PictureMasked(t), [c; 4], uv),
        };
        // Each piece takes the whole shape's corner colours where its own
        // corners fall, so the pieces together blend as one rectangle.
        let colors = |s: DevRect| -> [Rgba; 4] {
            let at = |x: f32, y: f32| {
                let (tx, ty) = ((x - r.x0) / r.w(), (y - r.y0) / r.h());
                let top = lerp_rgba(&whole[0], &whole[1], tx);
                let bottom = lerp_rgba(&whole[2], &whole[3], tx);
                lerp_rgba(&top, &bottom, ty)
            };
            [at(s.x0, s.y0), at(s.x1, s.y0), at(s.x0, s.y1), at(s.x1, s.y1)]
        };
        let radius = radius.min(r.w() / 2.0).min(r.h() / 2.0);
        if radius < 0.5 {
            emit(mesh, body, &clip, r, uv, white, whole);
            return Ok(());
        }
        // A rounded shape is about the size of the visible clip at most; one
        // wholly clipped away needs no mask.
        if r.intersect(&clip).is_empty() {
            return Ok(());
        }
        let Some(mask) = self.corner_mask(cx.atlas, radius, radius, None)? else {
            emit(mesh, body, &clip, r, uv, white, whole);
            return Ok(());
        };
        let sub_uv = |s: DevRect| sub_uv(&r, &uv, &s);
        let (x0, y0, x1, y1) = (r.x0, r.y0, r.x1, r.y1);
        let k = radius;
        for s in [
            DevRect::new(x0 + k, y0, x1 - k, y0 + k),
            DevRect::new(x0, y0 + k, x1, y1 - k),
            DevRect::new(x0 + k, y1 - k, x1 - k, y1),
        ] {
            emit(mesh, body, &clip, s, sub_uv(s), white, colors(s));
        }
        for (s, m) in corner_pieces(&r, k, &mask, cx.atlas) {
            emit(mesh, corners, &clip, s, sub_uv(s), m, colors(s));
        }
        Ok(())
    }

    /// An outline `width` dp wide centred on the rectangle's edge.
    fn stroke(
        &mut self,
        mesh: &mut Mesh,
        cx: &mut Inputs<'_>,
        rect: Rect,
        radius: f32,
        width: f32,
        c: Rgba,
    ) -> Result<(), AtlasFull> {
        let xf = self.xf();
        let outer = xf.rect(rect.expand(width / 2.0));
        if outer.is_empty() {
            return Ok(());
        }
        let w = width * xf.s;
        let ro = if radius > 0.0 { (radius + width / 2.0) * xf.s } else { 0.0 };
        let ro = ro.min(outer.w() / 2.0).min(outer.h() / 2.0);
        if 2.0 * w >= outer.w().min(outer.h()) {
            return self.rounded(mesh, cx, outer, ro, Fill::Color(c));
        }
        let clip = self.clip();
        if outer.intersect(&clip).is_empty() {
            return Ok(());
        }
        let white = white(cx.atlas);
        let colors = [c; 4];
        let (x0, y0, x1, y1) = (outer.x0, outer.y0, outer.x1, outer.y1);
        let extent = masks::corner_extent(ro, Some(w)).min(outer.w() / 2.0).min(outer.h() / 2.0);
        let mask = if ro >= 0.5 { self.corner_mask(cx.atlas, extent, ro, Some(w))? } else { None };
        let Some(mask) = mask else {
            // Square corners: four bands.
            for s in [
                DevRect::new(x0, y0, x1, y0 + w),
                DevRect::new(x0, y1 - w, x1, y1),
                DevRect::new(x0, y0 + w, x0 + w, y1 - w),
                DevRect::new(x1 - w, y0 + w, x1, y1 - w),
            ] {
                emit(mesh, Paint::Atlas, &clip, s, [0.0; 4], white, colors);
            }
            return Ok(());
        };
        let k = extent;
        for s in [
            DevRect::new(x0 + k, y0, x1 - k, y0 + w),
            DevRect::new(x0 + k, y1 - w, x1 - k, y1),
            DevRect::new(x0, y0 + k, x0 + w, y1 - k),
            DevRect::new(x1 - w, y0 + k, x1, y1 - k),
        ] {
            emit(mesh, Paint::Atlas, &clip, s, [0.0; 4], white, colors);
        }
        for (s, m) in corner_pieces(&outer, k, &mask, cx.atlas) {
            emit(mesh, Paint::Atlas, &clip, s, [0.0; 4], m, colors);
        }
        Ok(())
    }

    /// The atlas entry of a corner mask, made on first use; `None` when it
    /// can't be had even on an empty page.
    fn corner_mask(
        &self,
        atlas: &mut Atlas,
        extent: f32,
        radius: f32,
        width: Option<f32>,
    ) -> Result<Option<AtlasEntry>, AtlasFull> {
        let q = |v: f32| (v * 4.0).round().clamp(0.0, u16::MAX as f32) as u16;
        let key = AtlasKey::Corner { extent: q(extent), radius: q(radius), width: width.map_or(0, q) };
        if let Some(entry) = atlas.get(&key) {
            return Ok(entry);
        }
        let unq = |v: u16| v as f32 / 4.0;
        let (side, coverage) = masks::corner(unq(q(extent)), unq(q(radius)), width.map(|w| unq(q(w))));
        self.insert(atlas, key, side, side, 0, 0, &coverage)
    }

    /// Puts coverage on the atlas; a full page stops the command unless the
    /// builder is lenient, when the entry is left out.
    #[allow(clippy::too_many_arguments)]
    fn insert(
        &self,
        atlas: &mut Atlas,
        key: AtlasKey,
        w: u32,
        h: u32,
        left: i32,
        top: i32,
        coverage: &[u8],
    ) -> Result<Option<AtlasEntry>, AtlasFull> {
        match atlas.insert(key, w, h, left, top, coverage) {
            Ok(e) => Ok(Some(e)),
            Err(AtlasFull) if self.lenient || atlas.is_fresh() => {
                atlas.insert_empty(key);
                Ok(None)
            }
            Err(full) => Err(full),
        }
    }

    fn linear(&mut self, mesh: &mut Mesh, atlas: &Atlas, rect: Rect, vertical: bool, stops: &[GradientStop]) {
        let r = self.xf().rect(rect);
        if r.is_empty() || stops.is_empty() {
            return;
        }
        let opacity = self.opacity();
        let mut stops: Vec<(f32, Rgba)> =
            stops.iter().map(|s| (s.at.clamp(0.0, 1.0), premultiply(s.color, opacity))).collect();
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (first, last) = (stops[0], stops[stops.len() - 1]);
        // Solid before the first stop and after the last, then one piece per
        // pair of stops, each colour exactly at its stop.
        let mut pieces = Vec::with_capacity(stops.len() + 1);
        pieces.push((0.0, first.0, first.1, first.1));
        pieces.extend(stops.windows(2).map(|w| (w[0].0, w[1].0, w[0].1, w[1].1)));
        pieces.push((last.0, 1.0, last.1, last.1));
        let clip = self.clip();
        let white = white(atlas);
        for (a, b, ca, cb) in pieces {
            if b <= a || (ca[3] <= 0.0 && cb[3] <= 0.0) {
                continue;
            }
            let (s, colors) = if vertical {
                (DevRect::new(r.x0, r.y0 + a * r.h(), r.x1, r.y0 + b * r.h()), [ca, ca, cb, cb])
            } else {
                (DevRect::new(r.x0 + a * r.w(), r.y0, r.x0 + b * r.w(), r.y1), [ca, cb, ca, cb])
            };
            emit(mesh, Paint::Atlas, &clip, s, [0.0; 4], white, colors);
        }
    }

    /// A radial gradient as a fan of triangles from the centre (colour linear
    /// in the distance), and past the radius a ring in the outer colour, all
    /// clipped to the rectangle.
    #[allow(clippy::too_many_arguments)]
    fn radial(
        &mut self,
        mesh: &mut Mesh,
        atlas: &Atlas,
        rect: Rect,
        center: Point,
        radius: f32,
        inner: Color,
        outer: Color,
    ) {
        let xf = self.xf();
        let clip = xf.rect(rect).intersect(&self.clip());
        let r = radius * xf.s;
        if clip.is_empty() || r <= 0.0 {
            return;
        }
        let opacity = self.opacity();
        let (ci, co) = (premultiply(inner, opacity), premultiply(outer, opacity));
        let (cx0, cy0) = xf.point(center);
        let far = [(clip.x0, clip.y0), (clip.x1, clip.y0), (clip.x0, clip.y1), (clip.x1, clip.y1)]
            .iter()
            .map(|(x, y)| ((x - cx0).powi(2) + (y - cy0).powi(2)).sqrt())
            .fold(0.0f32, f32::max);
        // Enough segments that a chord stays within a fraction of a pixel of
        // the circle.
        let segments = ((r.sqrt() * 3.0) as usize).clamp(24, 128);
        let white = white(atlas);
        let ring = far * 1.5 + 1.0;
        let at = |i: usize, d: f32| {
            let t = i as f32 / segments as f32 * std::f32::consts::TAU;
            (cx0 + d * t.cos(), cy0 + d * t.sin())
        };
        for i in 0..segments {
            let (a, b) = (at(i, r), at(i + 1, r));
            if ci[3] > 0.0 || co[3] > 0.0 {
                emit_triangle(mesh, &clip, [((cx0, cy0), ci), (a, co), (b, co)], white);
            }
            if co[3] > 0.0 && far > r {
                let (a2, b2) = (at(i, ring), at(i + 1, ring));
                emit_triangle(mesh, &clip, [(a, co), (a2, co), (b, co)], white);
                emit_triangle(mesh, &clip, [(b, co), (a2, co), (b2, co)], white);
            }
        }
    }

    fn text(
        &mut self,
        mesh: &mut Mesh,
        cx: &mut Inputs<'_>,
        origin: Point,
        layout: &pley_base::TextLayout,
        c: Rgba,
    ) -> Result<(), AtlasFull> {
        let xf = self.xf();
        let clip = self.clip();
        for g in &layout.glyphs {
            let (px, py) = xf.point(Point::new(origin.x + g.x, origin.y + g.y));
            let size = g.size * xf.s;
            // Off-screen glyphs are never rasterised: the box a glyph can ink
            // is within about its size of the pen.
            if size < 1.0
                || px - size > clip.x1
                || px + 2.0 * size < clip.x0
                || py - 1.5 * size > clip.y1
                || py + size < clip.y0
            {
                continue;
            }
            let size_q = (size * 16.0).round() as u32;
            let mut x = px.floor();
            let mut phase = ((px - x) * 4.0).round() as u32;
            if phase == 4 {
                phase = 0;
                x += 1.0;
            }
            let y = py.round();
            let key = AtlasKey::Glyph { font: g.font, glyph: g.id, size: size_q, phase: phase as u8 };
            let entry = match cx.atlas.get(&key) {
                Some(e) => e,
                None => match cx.glyphs.rasterize(g.font, g.id, size_q as f32 / 16.0, phase as f32 / 4.0) {
                    Some(m) if m.width > 0 && m.height > 0 => {
                        self.insert(cx.atlas, key, m.width, m.height, m.left, m.top, &m.coverage)?
                    }
                    _ => {
                        cx.atlas.insert_empty(key);
                        None
                    }
                },
            };
            if let Some(e) = entry {
                let x0 = x + e.left as f32;
                let y0 = y + e.top as f32;
                let s = DevRect::new(x0, y0, x0 + e.w as f32, y0 + e.h as f32);
                let m = entry_uv(cx.atlas, &e);
                emit(mesh, Paint::Atlas, &clip, s, [0.0; 4], m, [c; 4]);
            }
        }
        Ok(())
    }

    fn icon(&mut self, mesh: &mut Mesh, cx: &mut Inputs<'_>, rect: Rect, icon: Icon, c: Rgba) -> Result<(), AtlasFull> {
        let d = self.xf().rect(rect);
        let clip = self.clip();
        if d.is_empty() || d.intersect(&clip).is_empty() {
            return Ok(());
        }
        let px = d.w().min(d.h()).round().clamp(1.0, 1024.0) as u32;
        let stroke = Icon::stroke_for(rect.w.min(rect.h));
        let key = AtlasKey::Icon { icon, px: px as u16, stroke: (stroke * 100.0).round() as u16 };
        let entry = match cx.atlas.get(&key) {
            Some(e) => e,
            None => match icons::rasterize(icon, px, stroke) {
                Some(coverage) => self.insert(cx.atlas, key, px, px, 0, 0, &coverage)?,
                None => None,
            },
        };
        if let Some(e) = entry {
            // Whole pixels keep the strokes crisp.
            let x0 = ((d.x0 + d.x1 - px as f32) / 2.0).round();
            let y0 = ((d.y0 + d.y1 - px as f32) / 2.0).round();
            let s = DevRect::new(x0, y0, x0 + px as f32, y0 + px as f32);
            let m = entry_uv(cx.atlas, &e);
            emit(mesh, Paint::Atlas, &clip, s, [0.0; 4], m, [c; 4]);
        }
        Ok(())
    }
}

/// What a rounded shape is filled with.
#[derive(Clone, Copy)]
enum Fill {
    Color(Rgba),
    /// Colours at the top-left, top-right, bottom-left and bottom-right corners,
    /// blended between.
    Corners([Rgba; 4]),
    /// A texture, the colour that multiplies it (its opacity, premultiplied)
    /// and its texture coordinates (u0, v0, u1, v1).
    Picture(TexRef, Rgba, [f32; 4]),
}

/// A straight colour premultiplied, times `opacity`.
pub fn premultiply(c: Color, opacity: f32) -> Rgba {
    let a = c.a as f32 / 255.0 * opacity;
    [c.r as f32 * a, c.g as f32 * a, c.b as f32 * a, 255.0 * a]
}

/// The texture coordinates of `src` (in texels; empty for all of it).
fn src_uv(src: Rect, tw: u32, th: u32) -> [f32; 4] {
    if src.w <= 0.0 || src.h <= 0.0 || tw == 0 || th == 0 {
        return [0.0, 0.0, 1.0, 1.0];
    }
    let (tw, th) = (tw as f32, th as f32);
    [src.x / tw, src.y / th, (src.x + src.w) / tw, (src.y + src.h) / th]
}

/// The atlas coordinates every flat piece samples: the solid block.
fn white(atlas: &Atlas) -> [f32; 4] {
    let [u, v] = atlas.white_uv();
    [u, v, u, v]
}

fn entry_uv(atlas: &Atlas, e: &AtlasEntry) -> [f32; 4] {
    let [u0, v0] = atlas.uv(e, 0.0, 0.0);
    let [u1, v1] = atlas.uv(e, e.w as f32, e.h as f32);
    [u0, v0, u1, v1]
}

/// The texture coordinates of the part `s` of `r`, whose coordinates are `uv`.
fn sub_uv(r: &DevRect, uv: &[f32; 4], s: &DevRect) -> [f32; 4] {
    let fx = |x: f32| uv[0] + (uv[2] - uv[0]) * (x - r.x0) / r.w();
    let fy = |y: f32| uv[1] + (uv[3] - uv[1]) * (y - r.y0) / r.h();
    [fx(s.x0), fy(s.y0), fx(s.x1), fy(s.y1)]
}

/// The four corner pieces of `r`, `k` pixels square, and the mask coordinates
/// of each: the mask's texel (0, 0) at the outer corner, mirrored for the
/// other three.
fn corner_pieces(r: &DevRect, k: f32, mask: &AtlasEntry, atlas: &Atlas) -> [(DevRect, [f32; 4]); 4] {
    let [a0, b0] = atlas.uv(mask, 0.0, 0.0);
    let [a1, b1] = atlas.uv(mask, k, k);
    [
        (DevRect::new(r.x0, r.y0, r.x0 + k, r.y0 + k), [a0, b0, a1, b1]),
        (DevRect::new(r.x1 - k, r.y0, r.x1, r.y0 + k), [a1, b0, a0, b1]),
        (DevRect::new(r.x0, r.y1 - k, r.x0 + k, r.y1), [a0, b1, a1, b0]),
        (DevRect::new(r.x1 - k, r.y1 - k, r.x1, r.y1), [a1, b1, a0, b0]),
    ]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_rgba(a: &Rgba, b: &Rgba, t: f32) -> Rgba {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t), lerp(a[3], b[3], t)]
}

fn to_u8(c: &Rgba) -> [u8; 4] {
    let q = |v: f32| (v + 0.5).clamp(0.0, 255.0) as u8;
    let a = q(c[3]);
    // Rounding must not leave a channel above alpha, or premultiplied
    // blending would brighten.
    [q(c[0]).min(a), q(c[1]).min(a), q(c[2]).min(a), a]
}

/// Appends the part of `r` inside `clip`, its texture coordinates and corner
/// colours (top-left, top-right, bottom-left, bottom-right) cut to match.
fn emit(mesh: &mut Mesh, paint: Paint, clip: &DevRect, r: DevRect, uv: [f32; 4], mask: [f32; 4], colors: [Rgba; 4]) {
    if r.is_empty() {
        return;
    }
    let c = r.intersect(clip);
    if c.is_empty() {
        return;
    }
    let tx0 = (c.x0 - r.x0) / r.w();
    let tx1 = (c.x1 - r.x0) / r.w();
    let ty0 = (c.y0 - r.y0) / r.h();
    let ty1 = (c.y1 - r.y0) / r.h();
    let vertex = |x: f32, y: f32, tx: f32, ty: f32| {
        let top = lerp_rgba(&colors[0], &colors[1], tx);
        let bottom = lerp_rgba(&colors[2], &colors[3], tx);
        Vertex {
            pos: [x, y],
            uv: [lerp(uv[0], uv[2], tx), lerp(uv[1], uv[3], ty)],
            mask: [lerp(mask[0], mask[2], tx), lerp(mask[1], mask[3], ty)],
            color: to_u8(&lerp_rgba(&top, &bottom, ty)),
        }
    };
    mesh.push(
        paint,
        [
            vertex(c.x0, c.y0, tx0, ty0),
            vertex(c.x1, c.y0, tx1, ty0),
            vertex(c.x0, c.y1, tx0, ty1),
            vertex(c.x1, c.y1, tx1, ty1),
        ],
    );
}

type Corner = ((f32, f32), Rgba);

/// Appends a triangle clipped to `clip`, as quads whose last two corners are
/// the same point.
fn emit_triangle(mesh: &mut Mesh, clip: &DevRect, tri: [Corner; 3], white: [f32; 4]) {
    let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for ((x, y), _) in &tri {
        lo_x = lo_x.min(*x);
        hi_x = hi_x.max(*x);
        lo_y = lo_y.min(*y);
        hi_y = hi_y.max(*y);
    }
    if hi_x <= clip.x0 || lo_x >= clip.x1 || hi_y <= clip.y0 || lo_y >= clip.y1 {
        return;
    }
    let poly = clip_polygon(&tri, clip);
    if poly.len() < 3 {
        return;
    }
    let vertex =
        |((x, y), c): &Corner| Vertex { pos: [*x, *y], uv: [0.0; 2], mask: [white[0], white[1]], color: to_u8(c) };
    for i in 1..poly.len() - 1 {
        let (a, b, c) = (vertex(&poly[0]), vertex(&poly[i]), vertex(&poly[i + 1]));
        mesh.push(Paint::Atlas, [a, b, c, c]);
    }
}

/// Sutherland–Hodgman: the part of a convex polygon inside `clip`, colours
/// interpolated along the cut edges.
fn clip_polygon(poly: &[Corner], clip: &DevRect) -> Vec<Corner> {
    let mut out: Vec<Corner> = poly.to_vec();
    // Each edge of the clip as (axis, bound, keep values above the bound).
    for (axis, bound, above) in [(0usize, clip.x0, true), (0, clip.x1, false), (1, clip.y0, true), (1, clip.y1, false)]
    {
        if out.is_empty() {
            break;
        }
        let input = std::mem::take(&mut out);
        let coord = |p: &Corner| if axis == 0 { p.0.0 } else { p.0.1 };
        let inside = |p: &Corner| if above { coord(p) >= bound } else { coord(p) <= bound };
        for i in 0..input.len() {
            let cur = input[i];
            let prev = input[(i + input.len() - 1) % input.len()];
            let cut = |a: &Corner, b: &Corner| {
                let t = (bound - coord(a)) / (coord(b) - coord(a));
                let p = (lerp(a.0.0, b.0.0, t), lerp(a.0.1, b.0.1, t));
                (p, lerp_rgba(&a.1, &b.1, t))
            };
            match (inside(&prev), inside(&cur)) {
                (true, true) => out.push(cur),
                (true, false) => out.push(cut(&prev, &cur)),
                (false, true) => {
                    out.push(cut(&prev, &cur));
                    out.push(cur);
                }
                (false, false) => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use pley_base::{Glyph, TextLayout};

    use super::*;
    use crate::glyphs::GlyphMask;

    /// Glyphs as solid boxes: `size / 2` wide and `size` tall, above the
    /// baseline.
    struct Boxes;

    impl GlyphSource for Boxes {
        fn rasterize(&self, _font: u16, glyph: u16, size_px: f32, _sub: f32) -> Option<GlyphMask> {
            if glyph == 0 {
                return None;
            }
            let (w, h) = ((size_px / 2.0) as u32, size_px as u32);
            Some(GlyphMask { width: w, height: h, left: 0, top: -(h as i32), coverage: vec![255; (w * h) as usize] })
        }
    }

    #[derive(Default)]
    struct Tex {
        images: HashMap<ImageId, (u32, u32)>,
        raws: Vec<u64>,
    }

    impl Textures for Tex {
        fn image(&mut self, id: ImageId) -> Option<(u32, u32)> {
            self.images.get(&id).copied()
        }

        fn raw(&mut self, key: u64, image: &RgbaImage) -> Option<(u32, u32)> {
            self.raws.push(key);
            Some((image.width, image.height))
        }
    }

    struct Rig {
        atlas: Atlas,
        tex: Tex,
        builder: Builder,
        mesh: Mesh,
    }

    impl Rig {
        fn new() -> Rig {
            Rig {
                atlas: Atlas::new(512),
                tex: Tex::default(),
                builder: Builder::new(1920, 1080, 2.0),
                mesh: Mesh::default(),
            }
        }

        fn run(&mut self, cmds: &[Cmd]) -> Result<(), usize> {
            let mut cx = Inputs { atlas: &mut self.atlas, glyphs: &Boxes, textures: &mut self.tex };
            self.builder.run(cmds, 0, &mut self.mesh, &mut cx)
        }

        fn quad(&self, i: usize) -> &[Vertex] {
            &self.mesh.vertices[i * 4..i * 4 + 4]
        }
    }

    fn red() -> Color {
        Color::rgb(0xff0000)
    }

    fn bounds(q: &[Vertex]) -> [f32; 4] {
        [q[0].pos[0], q[0].pos[1], q[3].pos[0], q[3].pos[1]]
    }

    #[test]
    fn square_fill_is_one_quad_at_device_pixels() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::Fill { rect: Rect::new(10.0, 20.0, 30.0, 40.0), color: red(), radius: 0.0 }]).unwrap();
        assert_eq!(rig.mesh.quads(), 1);
        assert_eq!(bounds(rig.quad(0)), [20.0, 40.0, 80.0, 120.0]);
        assert_eq!(rig.quad(0)[0].color, [255, 0, 0, 255]);
        assert_eq!(rig.mesh.batches, vec![Batch { paint: Paint::Atlas, first: 0, quads: 1 }]);
        // Flat pieces sample the solid block.
        assert_eq!(rig.quad(0)[0].mask, rig.atlas.white_uv());
    }

    #[test]
    fn colours_are_premultiplied_with_the_opacity_stack() {
        let mut rig = Rig::new();
        rig.run(&[
            Cmd::PushOpacity(0.5),
            Cmd::Fill { rect: Rect::new(0.0, 0.0, 1.0, 1.0), color: Color::rgba(0xffffff80), radius: 0.0 },
            Cmd::PopOpacity,
            Cmd::Fill { rect: Rect::new(0.0, 0.0, 1.0, 1.0), color: Color::rgba(0xffffff80), radius: 0.0 },
        ])
        .unwrap();
        assert_eq!(rig.quad(0)[0].color, [64, 64, 64, 64]);
        assert_eq!(rig.quad(1)[0].color, [128, 128, 128, 128]);
    }

    #[test]
    fn rounded_fill_is_three_flat_pieces_and_four_masked_corners() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::Fill { rect: Rect::new(0.0, 0.0, 100.0, 50.0), color: red(), radius: 4.0 }]).unwrap();
        assert_eq!(rig.mesh.quads(), 7);
        assert_eq!(rig.mesh.batches.len(), 1);
        // The middle band spans the full width between the corner rows.
        assert_eq!(bounds(rig.quad(1)), [0.0, 8.0, 200.0, 92.0]);
        // The top-left corner piece maps the mask's corner texel to the outer corner.
        let tl = rig.quad(3);
        assert_eq!(bounds(tl), [0.0, 0.0, 8.0, 8.0]);
        let key = AtlasKey::Corner { extent: 32, radius: 32, width: 0 };
        let e = rig.atlas.get(&key).unwrap().unwrap();
        assert_eq!(tl[0].mask, rig.atlas.uv(&e, 0.0, 0.0));
        // The top-right one is mirrored.
        let tr = rig.quad(4);
        assert_eq!(tr[1].mask, rig.atlas.uv(&e, 0.0, 0.0));
        // Area covered: the whole rectangle, once.
        let area: f32 = (0..7)
            .map(|i| {
                let b = bounds(rig.quad(i));
                (b[2] - b[0]) * (b[3] - b[1])
            })
            .sum();
        assert_eq!(area, 200.0 * 100.0);
    }

    #[test]
    fn a_pill_has_no_middle_band() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::Fill { rect: Rect::new(0.0, 0.0, 100.0, 40.0), color: red(), radius: 50.0 }]).unwrap();
        // Radius clamps to half the height: the middle band is empty.
        assert_eq!(rig.mesh.quads(), 6);
    }

    #[test]
    fn stroke_is_four_bands_and_four_ring_corners() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::Stroke { rect: Rect::new(10.0, 10.0, 108.0, 162.0), color: red(), radius: 8.5, width: 3.0 }])
            .unwrap();
        assert_eq!(rig.mesh.quads(), 8);
        // Outer edge 1.5 dp outside the rectangle.
        let top = bounds(rig.quad(0));
        assert_eq!(top[1], 17.0);
        assert_eq!(top[3] - top[1], 6.0);
        let key = AtlasKey::Corner { extent: 80, radius: 80, width: 24 };
        assert!(rig.atlas.get(&key).is_some());
    }

    #[test]
    fn square_stroke_has_no_mask() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::Stroke { rect: Rect::new(10.0, 10.0, 50.0, 50.0), color: red(), radius: 0.0, width: 1.0 }])
            .unwrap();
        assert_eq!(rig.mesh.quads(), 4);
        assert!(rig.atlas.is_fresh());
    }

    #[test]
    fn linear_gradient_has_a_piece_per_stop_pair_and_solid_ends() {
        let mut rig = Rig::new();
        let bg = Color::rgb(0x0b0d10);
        rig.run(&[Cmd::LinearGradient {
            rect: Rect::new(0.0, 0.0, 960.0, 540.0),
            vertical: false,
            stops: vec![
                GradientStop::new(0.0, bg),
                GradientStop::new(300.0 / 960.0, bg.alpha(0.92)),
                GradientStop::new(560.0 / 960.0, bg.alpha(0.85)),
                GradientStop::new(800.0 / 960.0, bg.alpha(0.0)),
            ],
        }])
        .unwrap();
        // The solid start is empty (the first stop is at 0); the transparent end is skipped.
        assert_eq!(rig.mesh.quads(), 3);
        let q = rig.quad(1);
        assert_eq!(q[0].pos[0], 600.0);
        assert_eq!(q[1].pos[0], 1120.0);
        assert_eq!(q[0].color[3], 235);
        assert_eq!(q[1].color[3], 217);
        assert_eq!(q[0].color, q[2].color);
    }

    #[test]
    fn vertical_gradient_colours_run_down() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::LinearGradient {
            rect: Rect::new(0.0, 100.0, 960.0, 200.0),
            vertical: true,
            stops: vec![GradientStop::new(0.0, Color::TRANSPARENT), GradientStop::new(1.0, Color::BLACK)],
        }])
        .unwrap();
        // Stops at both ends: no solid pieces.
        assert_eq!(rig.mesh.quads(), 1);
        let q = rig.quad(0);
        assert_eq!((q[0].color[3], q[1].color[3], q[2].color[3]), (0, 0, 255));
        assert_eq!(bounds(q), [0.0, 200.0, 1920.0, 600.0]);
    }

    #[test]
    fn clip_cuts_quads_and_their_coordinates() {
        let mut rig = Rig::new();
        rig.tex.images.insert(ImageId(1), (100, 100));
        rig.run(&[
            Cmd::PushClip { rect: Rect::new(0.0, 0.0, 50.0, 540.0) },
            Cmd::Image {
                rect: Rect::new(0.0, 0.0, 100.0, 100.0),
                image: ImageId(1),
                src: Rect::new(0.0, 0.0, 100.0, 100.0),
                radius: 0.0,
                opacity: 1.0,
            },
            Cmd::LinearGradient {
                rect: Rect::new(0.0, 0.0, 100.0, 10.0),
                vertical: false,
                stops: vec![GradientStop::new(0.0, Color::BLACK), GradientStop::new(1.0, Color::TRANSPARENT)],
            },
            Cmd::PopClip,
        ])
        .unwrap();
        let pic = rig.quad(0);
        assert_eq!(bounds(pic), [0.0, 0.0, 100.0, 200.0]);
        assert_eq!(pic[1].uv, [0.5, 0.0]);
        assert_eq!(pic[0].color, [255; 4]);
        // The gradient's colour at the cut is halfway.
        let g = rig.quad(1);
        assert_eq!(g[1].color[3], 128);
        assert_eq!(rig.mesh.batches.len(), 2);
    }

    #[test]
    fn transform_scales_about_the_pivot_and_nests() {
        let base = Xform { s: 2.0, dx: 0.0, dy: 0.0 };
        let tile = Rect::new(96.0, 210.0, 108.0, 162.0);
        let grown = base.then(Point::ZERO, 1.08, tile.center());
        let r = grown.rect(tile);
        let expected = tile.scale_about(tile.center(), 1.08);
        assert!((r.x0 - expected.x * 2.0).abs() < 1e-3 && (r.w() - expected.w * 2.0).abs() < 1e-3);
        let scrolled = base.then(Point::new(-50.0, 0.0), 1.0, Point::ZERO).then(Point::ZERO, 1.08, tile.center());
        let r2 = scrolled.rect(tile);
        assert!((r2.x0 - (r.x0 - 100.0)).abs() < 1e-3);
    }

    #[test]
    fn clip_follows_the_transform_and_unbalanced_pops_are_ignored() {
        let mut rig = Rig::new();
        rig.run(&[
            Cmd::PopClip,
            Cmd::PopTransform,
            Cmd::PushTransform { offset: Point::new(100.0, 0.0), scale: 1.0, pivot: Point::ZERO },
            Cmd::PushClip { rect: Rect::new(0.0, 0.0, 10.0, 10.0) },
            Cmd::PopTransform,
            Cmd::Fill { rect: Rect::new(0.0, 0.0, 960.0, 540.0), color: red(), radius: 0.0 },
        ])
        .unwrap();
        // The clip was set under the transform: device x 200 to 220.
        assert_eq!(bounds(rig.quad(0)), [200.0, 0.0, 220.0, 20.0]);
    }

    #[test]
    fn everything_outside_the_clip_is_dropped() {
        let mut rig = Rig::new();
        rig.run(&[
            Cmd::PushClip { rect: Rect::new(0.0, 0.0, 10.0, 10.0) },
            Cmd::Fill { rect: Rect::new(20.0, 20.0, 10.0, 10.0), color: red(), radius: 3.0 },
            Cmd::Icon { rect: Rect::new(20.0, 20.0, 24.0, 24.0), icon: Icon::Play, color: red() },
            Cmd::PopClip,
        ])
        .unwrap();
        assert_eq!(rig.mesh.quads(), 0);
        // Nothing was rasterised for them either.
        assert!(rig.atlas.is_fresh());
    }

    #[test]
    fn text_glyphs_are_placed_on_whole_pixels_from_the_pen() {
        let mut rig = Rig::new();
        let layout = TextLayout {
            text: "ab ".into(),
            glyphs: vec![
                Glyph { font: 0, id: 1, x: 0.0, y: 20.0, size: 16.0 },
                Glyph { font: 0, id: 2, x: 9.1, y: 20.0, size: 16.0 },
                Glyph { font: 0, id: 0, x: 18.0, y: 20.0, size: 16.0 },
            ],
            ..TextLayout::default()
        };
        rig.run(&[Cmd::Text { origin: Point::new(10.0, 10.0), layout: Arc::new(layout), color: Color::WHITE }])
            .unwrap();
        assert_eq!(rig.mesh.quads(), 2);
        // 16 dp at 2 px/dp: a 16×32 box whose bottom sits on the baseline (y 60).
        assert_eq!(bounds(rig.quad(0)), [20.0, 28.0, 36.0, 60.0]);
        // x = 38.2 px: pen at 38, phase 1 (¼ px).
        assert_eq!(bounds(rig.quad(1))[0], 38.0);
        assert!(rig.atlas.get(&AtlasKey::Glyph { font: 0, glyph: 2, size: 512, phase: 1 }).is_some());
        // The space is remembered as empty.
        assert_eq!(rig.atlas.get(&AtlasKey::Glyph { font: 0, glyph: 0, size: 512, phase: 0 }), Some(None));
    }

    #[test]
    fn icons_are_rasterised_at_their_device_size() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::Icon { rect: Rect::new(10.0, 10.0, 20.0, 20.0), icon: Icon::Search, color: Color::WHITE }])
            .unwrap();
        assert_eq!(bounds(rig.quad(0)), [20.0, 20.0, 60.0, 60.0]);
        assert!(rig.atlas.get(&AtlasKey::Icon { icon: Icon::Search, px: 40, stroke: 225 }).is_some());
    }

    #[test]
    fn rounded_picture_splits_into_plain_and_masked_batches() {
        let mut rig = Rig::new();
        rig.tex.images.insert(ImageId(7), (216, 324));
        rig.run(&[Cmd::Image {
            rect: Rect::new(0.0, 0.0, 108.0, 162.0),
            image: ImageId(7),
            src: Rect::new(0.0, 0.0, 216.0, 324.0),
            radius: 4.0,
            opacity: 0.5,
        }])
        .unwrap();
        let t = TexRef::Image(ImageId(7));
        assert_eq!(
            rig.mesh.batches,
            vec![
                Batch { paint: Paint::Picture(t), first: 0, quads: 3 },
                Batch { paint: Paint::PictureMasked(t), first: 3, quads: 4 },
            ]
        );
        // The top-left corner piece covers the picture's first 8 × 8 pixels.
        let tl = rig.quad(3);
        assert_eq!(tl[3].uv, [8.0 / 216.0, 8.0 / 324.0]);
        assert_eq!(tl[0].color, [128; 4]);
    }

    #[test]
    fn missing_picture_draws_nothing_and_raw_pictures_upload_by_key() {
        let mut rig = Rig::new();
        let qr = Arc::new(RgbaImage::new(8, 8));
        rig.run(&[
            Cmd::Image {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                image: ImageId(9),
                src: Rect::ZERO,
                radius: 0.0,
                opacity: 1.0,
            },
            Cmd::Raw { rect: Rect::new(0.0, 0.0, 10.0, 10.0), image: qr, key: 42, radius: 0.0, opacity: 1.0 },
        ])
        .unwrap();
        assert_eq!(rig.mesh.quads(), 1);
        assert_eq!(rig.tex.raws, vec![42]);
        assert_eq!(rig.mesh.batches[0].paint, Paint::Picture(TexRef::Raw(42)));
    }

    #[test]
    fn punch_is_transparent_and_its_own_batch() {
        let mut rig = Rig::new();
        rig.run(&[
            Cmd::Fill { rect: Rect::new(0.0, 0.0, 960.0, 540.0), color: red(), radius: 0.0 },
            Cmd::Punch { rect: Rect::new(100.0, 100.0, 200.0, 100.0) },
            Cmd::Fill { rect: Rect::new(0.0, 0.0, 10.0, 10.0), color: red(), radius: 0.0 },
        ])
        .unwrap();
        assert_eq!(
            rig.mesh.batches.iter().map(|b| b.paint).collect::<Vec<_>>(),
            vec![Paint::Atlas, Paint::Punch, Paint::Atlas]
        );
        assert_eq!(rig.quad(1)[0].color, [0; 4]);
    }

    #[test]
    fn radial_glow_from_a_corner_stays_in_its_rectangle() {
        let mut rig = Rig::new();
        rig.run(&[Cmd::RadialGradient {
            rect: Rect::new(0.0, 0.0, 960.0, 540.0),
            center: Point::new(960.0, 540.0),
            radius: 576.0,
            inner: Color::rgb(0x0284c7).alpha(0.18),
            outer: Color::TRANSPARENT,
        }])
        .unwrap();
        assert!(rig.mesh.quads() > 10);
        for v in &rig.mesh.vertices {
            assert!(v.pos[0] >= -1e-3 && v.pos[0] <= 1920.001 && v.pos[1] >= -1e-3 && v.pos[1] <= 1080.001);
        }
        // At the centre the inner colour; it fades to nothing at the radius.
        let max_alpha = rig.mesh.vertices.iter().map(|v| v.color[3]).max().unwrap();
        assert_eq!(max_alpha, 46);
        assert!(rig.mesh.vertices.iter().any(|v| v.color[3] == 0));
        assert_eq!(rig.mesh.batches.len(), 1);
    }

    #[test]
    fn clipped_polygon_keeps_colours_linear() {
        let tri = [((0.0, 0.0), [0.0; 4]), ((10.0, 0.0), [255.0; 4]), ((0.0, 10.0), [0.0; 4])];
        let out = clip_polygon(&tri, &DevRect::new(0.0, 0.0, 5.0, 10.0));
        assert_eq!(out.len(), 4);
        let cut = out.iter().find(|p| p.0 == (5.0, 0.0)).unwrap();
        assert_eq!(cut.1[0], 127.5);
    }

    #[test]
    fn a_full_atlas_stops_at_the_command_and_resumes() {
        let mut rig = Rig::new();
        rig.atlas = Atlas::new(64);
        let icons = [Icon::Play, Icon::Pause, Icon::Search, Icon::House];
        let cmds: Vec<Cmd> = icons
            .iter()
            .map(|&icon| Cmd::Icon { rect: Rect::new(0.0, 0.0, 12.0, 12.0), icon, color: Color::WHITE })
            .collect();
        // Each icon is 24 px: two fit on a 64-texel page beside the solid block's shelf.
        let stopped = rig.run(&cmds).unwrap_err();
        assert!(stopped > 0 && stopped < 4);
        let before = rig.mesh.quads();
        assert_eq!(before, stopped);
        rig.atlas.reset();
        let mut cx = Inputs { atlas: &mut rig.atlas, glyphs: &Boxes, textures: &mut rig.tex };
        rig.mesh.clear();
        let next = rig.builder.run(&cmds, stopped, &mut rig.mesh, &mut cx);
        assert!(next.is_ok() || next.unwrap_err() > stopped);
    }
}
