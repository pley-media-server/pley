//! The fixture server's pictures, drawn on request without fonts: a
//! gradient in a colour from the picture's seed with a simple shape or two,
//! so tiles look apart. Sized like the server's: widths rounded up to its
//! buckets (apps/server/src/images.rs), the height from the picture's shape;
//! JPEG at quality 82 as the server resizes, PNG where there is transparency
//! (channel logos). Recent ones are kept.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use crate::http::{Body, Response, lock};
use crate::{jpeg, png};

/// The server's widths.
pub(crate) const BUCKETS: [u32; 8] = [160, 240, 320, 480, 640, 960, 1280, 1920];
const QUALITY: u8 = 82;
/// How many pictures are kept.
const KEPT: usize = 160;

/// A width rounded up to the server's buckets; the largest above them.
pub(crate) fn bucket(width: u32) -> u32 {
    BUCKETS.iter().copied().find(|b| *b >= width).unwrap_or(BUCKETS[BUCKETS.len() - 1])
}

/// A picture's shape: width over height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Poster,
    Wide,
    Square,
}

impl Shape {
    fn height(self, width: u32) -> u32 {
        match self {
            Shape::Poster => width * 3 / 2,
            Shape::Wide => width * 9 / 16,
            Shape::Square => width,
        }
        .max(1)
    }

    /// The width without `w`: the stored original's.
    fn original(self) -> u32 {
        match self {
            Shape::Poster | Shape::Square => 640,
            Shape::Wide => 1280,
        }
    }
}

/// A picture as an answer.
pub(crate) fn picture(bytes: Arc<[u8]>, content_type: &str, cache: &str) -> Response {
    let len = bytes.len();
    Response::new(200)
        .with_header("Content-Type", content_type)
        .with_header("Cache-Control", cache)
        .with_body(Body::Shared(bytes, 0..len))
}

/// The pictures drawn lately.
#[derive(Default)]
pub(crate) struct Pictures {
    kept: Mutex<Kept>,
}

/// Pictures by key, and the keys oldest first.
#[derive(Default)]
struct Kept {
    by_key: HashMap<String, Arc<[u8]>>,
    order: VecDeque<String>,
}

impl Pictures {
    fn cached(&self, key: String, make: impl FnOnce() -> Vec<u8>) -> Arc<[u8]> {
        if let Some(bytes) = lock(&self.kept).by_key.get(&key) {
            return bytes.clone();
        }
        let bytes: Arc<[u8]> = make().into();
        let mut kept = lock(&self.kept);
        if kept.by_key.insert(key.clone(), bytes.clone()).is_none() {
            kept.order.push_back(key);
        }
        while kept.order.len() > KEPT {
            if let Some(old) = kept.order.pop_front() {
                kept.by_key.remove(&old);
            }
        }
        bytes
    }

    /// Artwork for `seed` of `shape`, `width` pixels wide (bucketed), or
    /// the original's width without one.
    pub fn artwork(&self, seed: i64, shape: Shape, width: Option<u32>) -> Arc<[u8]> {
        let w = width.map(bucket).unwrap_or(shape.original());
        let h = shape.height(w);
        // A handful of colours and layouts: enough to tell tiles apart, few
        // enough to keep.
        let hue = (seed * 37).rem_euclid(24) as u32;
        let layout = (seed * 7).rem_euclid(3) as u32;
        self.cached(format!("art:{hue}:{layout}:{w}:{h}"), || {
            let mut canvas = Canvas::gradient(w, h, hsv(hue * 15, 0.55, 0.85), hsv((hue * 15 + 40) % 360, 0.7, 0.25));
            let (fw, fh) = (w as f32, h as f32);
            let small = fw.min(fh);
            match layout {
                0 => {
                    canvas.circle(fw * 0.72, fh * 0.34, small * 0.36, [255, 255, 255], 0.16);
                    canvas.circle(fw * 0.22, fh * 0.78, small * 0.18, [0, 0, 0], 0.18);
                }
                1 => canvas.band(fw * 0.2, fw * 0.12, [255, 255, 255], 0.15),
                _ => {
                    canvas.rect(0.0, fh * 0.68, fw, fh, [0, 0, 0], 0.22);
                    canvas.circle(fw * 0.5, fh * 0.68, small * 0.22, hsv((hue * 15 + 180) % 360, 0.5, 0.95), 0.5);
                }
            }
            jpeg::encode(w, h, &canvas.px, 4, QUALITY)
        })
    }

    /// A profile's picture: a head and shoulders on its colour, 256 pixels square.
    pub fn avatar(&self, rgb: u32) -> Arc<[u8]> {
        self.cached(format!("avatar:{rgb:06x}"), || {
            let color = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
            let mut canvas = Canvas::gradient(256, 256, color, shade(color, 0.7));
            canvas.circle(128.0, 104.0, 50.0, [255, 255, 255], 0.85);
            canvas.circle(128.0, 290.0, 112.0, [255, 255, 255], 0.85);
            jpeg::encode(256, 256, &canvas.px, 4, QUALITY)
        })
    }

    /// A person's photo: a head and shoulders on a gradient.
    pub fn photo(&self, seed: i64, width: Option<u32>) -> Arc<[u8]> {
        let w = width.map(bucket).unwrap_or(Shape::Poster.original());
        let h = Shape::Poster.height(w);
        let hue = (seed * 11).rem_euclid(24) as u32;
        self.cached(format!("photo:{hue}:{w}"), || {
            let mut canvas = Canvas::gradient(w, h, hsv(hue * 15, 0.3, 0.7), hsv(hue * 15, 0.4, 0.3));
            let fw = w as f32;
            canvas.circle(fw * 0.5, h as f32 * 0.36, fw * 0.2, [235, 220, 205], 1.0);
            canvas.circle(fw * 0.5, h as f32 * 0.98, fw * 0.42, hsv((hue * 15 + 200) % 360, 0.5, 0.5), 1.0);
            jpeg::encode(w, h, &canvas.px, 4, QUALITY)
        })
    }

    /// A channel's logo: a rounded badge with a disc, on transparency (PNG),
    /// 2:1, at most 400 pixels wide as the server keeps logos.
    pub fn logo(&self, seed: i64, width: Option<u32>) -> Arc<[u8]> {
        let w = width.map(bucket).unwrap_or(400).min(400);
        let h = w / 2;
        let hue = (seed * 5).rem_euclid(24) as u32;
        self.cached(format!("logo:{hue}:{w}"), || {
            let (fw, fh) = (w as f32, h as f32);
            let mut canvas = Canvas::transparent(w, h);
            canvas.rounded(fw * 0.04, fh * 0.12, fw * 0.96, fh * 0.88, fh * 0.2, hsv(hue * 15, 0.65, 0.8));
            canvas.circle(fw * 0.22, fh * 0.5, fh * 0.24, [255, 255, 255], 0.9);
            png::encode(w, h, 4, &canvas.rgba())
        })
    }

    /// A sheet of seek previews: `tiles` tiles from `first` (of `count`),
    /// `columns` across, each a shade of its own with a bar showing where it
    /// is, outlined.
    pub fn sheet(&self, first: u32, tiles: u32, count: u32, tile: (u32, u32), columns: u32) -> Arc<[u8]> {
        let (tw, th) = tile;
        let rows = tiles.div_ceil(columns).max(1);
        let (w, h) = (tw * columns, th * rows);
        self.cached(format!("sheet:{first}:{tiles}:{count}:{tw}:{th}:{columns}"), || {
            let mut canvas = Canvas::gradient(w, h, [0, 0, 0], [0, 0, 0]);
            for i in 0..tiles {
                let k = first + i;
                let x = (i % columns * tw) as f32;
                let y = (i / columns * th) as f32;
                let (fw, fh) = (tw as f32, th as f32);
                canvas.rect(x, y, x + fw, y + fh, [255, 255, 255], 1.0);
                canvas.rect(x + 2.0, y + 2.0, x + fw - 2.0, y + fh - 2.0, hsv(k * 15 % 360, 0.5, 0.45), 1.0);
                let done = (k + 1) as f32 / count.max(1) as f32;
                canvas.rect(x + 2.0, y + fh - 14.0, x + 2.0 + (fw - 4.0) * done, y + fh - 2.0, [240, 240, 240], 1.0);
            }
            jpeg::encode(w, h, &canvas.px, 4, QUALITY)
        })
    }
}

/// Pixels to draw on: RGBA, 8 bits a channel, straight alpha.
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn gradient(w: u32, h: u32, top: [u8; 3], bottom: [u8; 3]) -> Canvas {
        let (w, h) = (w as usize, h as usize);
        let mut px = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            let t = y as f32 / h.max(2) as f32;
            let c = [0, 1, 2].map(|i| (top[i] as f32 + (bottom[i] as f32 - top[i] as f32) * t).round() as u8);
            px.extend_from_slice(&[c[0], c[1], c[2], 255].repeat(w));
        }
        Canvas { w, h, px }
    }

    fn transparent(w: u32, h: u32) -> Canvas {
        let (w, h) = (w as usize, h as usize);
        Canvas { w, h, px: vec![0; w * h * 4] }
    }

    /// Blends `color` at `alpha` over the pixels of row `y` from `x0` to `x1`.
    fn span(&mut self, y: i64, x0: f32, x1: f32, color: [u8; 3], alpha: f32) {
        if y < 0 || y as usize >= self.h {
            return;
        }
        let end = (x1.round().max(0.0) as usize).min(self.w);
        let start = (x0.round().max(0.0) as usize).min(end);
        let row = y as usize * self.w * 4;
        let pixels = &mut self.px[row + start * 4..row + end * 4];
        if alpha >= 1.0 {
            pixels.copy_from_slice(&[color[0], color[1], color[2], 255].repeat(end - start));
            return;
        }
        for p in pixels.as_chunks_mut::<4>().0 {
            let a = p[3] as f32 / 255.0;
            let out = alpha + a * (1.0 - alpha);
            if out <= 0.0 {
                continue;
            }
            for i in 0..3 {
                p[i] = ((color[i] as f32 * alpha + p[i] as f32 * a * (1.0 - alpha)) / out).round() as u8;
            }
            p[3] = (out * 255.0).round() as u8;
        }
    }

    fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: [u8; 3], alpha: f32) {
        for y in y0.round() as i64..y1.round() as i64 {
            self.span(y, x0, x1, color, alpha);
        }
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 3], alpha: f32) {
        for y in (cy - r).floor() as i64..=(cy + r).ceil() as i64 {
            let dy = y as f32 + 0.5 - cy;
            if dy.abs() > r {
                continue;
            }
            let dx = (r * r - dy * dy).sqrt();
            self.span(y, cx - dx, cx + dx, color, alpha);
        }
    }

    /// A band across, leaning right as it goes down.
    fn band(&mut self, start: f32, width: f32, color: [u8; 3], alpha: f32) {
        for y in 0..self.h as i64 {
            let x = start + y as f32 * 0.5;
            self.span(y, x, x + width, color, alpha);
        }
    }

    fn rounded(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, color: [u8; 3]) {
        for y in y0.round() as i64..y1.round() as i64 {
            let fy = y as f32 + 0.5;
            let inset = if fy < y0 + r {
                r - (r * r - (y0 + r - fy).powi(2)).max(0.0).sqrt()
            } else if fy > y1 - r {
                r - (r * r - (fy - (y1 - r)).powi(2)).max(0.0).sqrt()
            } else {
                0.0
            };
            self.span(y, x0 + inset, x1 - inset, color, 1.0);
        }
    }

    fn rgba(self) -> Vec<u8> {
        self.px
    }
}

fn shade(color: [u8; 3], f: f32) -> [u8; 3] {
    color.map(|c| (c as f32 * f) as u8)
}

/// A colour from its hue (degrees), saturation and value (0 to 1).
fn hsv(hue: u32, saturation: f32, value: f32) -> [u8; 3] {
    let c = value * saturation;
    let x = c * (1.0 - ((hue as f32 / 60.0) % 2.0 - 1.0).abs());
    let m = value - c;
    let (r, g, b) = match hue / 60 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_round_up_to_buckets() {
        assert_eq!(bucket(1), 160);
        assert_eq!(bucket(160), 160);
        assert_eq!(bucket(161), 240);
        assert_eq!(bucket(1_000), 1_280);
        assert_eq!(bucket(4_000), 1_920);
    }

    #[test]
    fn colours_from_hues() {
        assert_eq!(hsv(0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv(120, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv(240, 1.0, 0.5), [0, 0, 128]);
    }

    #[test]
    fn jpegs_decode_to_their_size_and_colours() {
        let pictures = Pictures::default();
        for (shape, width, size) in [(Shape::Poster, Some(240), (240, 360)), (Shape::Wide, None, (1_280, 720))] {
            let bytes = pictures.artwork(42, shape, width);
            let mut decoder = jpeg_decoder::Decoder::new(&bytes[..]);
            let pixels = decoder.decode().expect("a JPEG any decoder reads");
            let info = decoder.info().unwrap();
            assert_eq!((info.width as u32, info.height as u32), size);
            assert_eq!(pixels.len(), size.0 as usize * size.1 as usize * 3);
        }
        // A flat picture comes back flat, to within JPEG's loss.
        let flat: Vec<u8> = [200u8, 40, 90].repeat(48 * 40);
        let decoded = jpeg_decoder::Decoder::new(&jpeg::encode(48, 40, &flat, 3, 82)[..]).decode().unwrap();
        for p in decoded.chunks(3) {
            for (got, want) in p.iter().zip([200u8, 40, 90]) {
                assert!((*got as i32 - want as i32).abs() <= 6, "{p:?}");
            }
        }
    }

    #[test]
    fn logos_are_transparent_pngs() {
        let bytes = Pictures::default().logo(3, Some(1_000));
        assert_eq!(&bytes[1..4], b"PNG");
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 400);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 200);
        assert_eq!(bytes[25], 6);
    }
}
