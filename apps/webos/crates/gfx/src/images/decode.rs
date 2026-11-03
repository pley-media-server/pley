//! Decoding pictures (JPEG, PNG, WebP, a GIF's first frame) with pure-Rust
//! decoders into premultiplied RGBA, and shrinking them to the width they are
//! drawn at.
//!
//! Every decode is bounded before it allocates (an allocation failure
//! aborts the process, so a hostile or broken file must be
//! refused from its header), and a decoder that panics yields a broken
//! picture, not a crashed worker.

use std::fmt;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// The largest side a picture may have (a poster cover-scaled into a 1080p
/// box is 1920×2880).
pub const MAX_SIDE: u32 = 4096;

/// The most memory one decode may take for its pixels: the largest picture the
/// app asks for is about 21 MiB.
pub const MAX_BYTES: usize = 32 << 20;

/// A decoded picture: premultiplied RGBA, 8 bits a channel, rows top to
/// bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Picture {
    /// Bytes of texels it takes on the GPU.
    pub fn bytes(&self) -> usize {
        self.pixels.len()
    }
}

impl fmt::Debug for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Picture({}×{})", self.width, self.height)
    }
}

/// Why bytes didn't make a picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not a format the app reads.
    Format,
    /// Bigger than [`MAX_SIDE`] or [`MAX_BYTES`].
    TooBig,
    /// The decoder refused the data (truncated, corrupt).
    Corrupt(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Format => f.write_str("not a JPEG, PNG, WebP or GIF picture"),
            DecodeError::TooBig => f.write_str("picture too big"),
            DecodeError::Corrupt(why) => write!(f, "broken picture: {why}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Jpeg,
    Png,
    WebP,
    Gif,
}

fn sniff(bytes: &[u8]) -> Option<Format> {
    match bytes {
        [0xff, 0xd8, 0xff, ..] => Some(Format::Jpeg),
        [0x89, b'P', b'N', b'G', ..] => Some(Format::Png),
        [b'G', b'I', b'F', b'8', ..] => Some(Format::Gif),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some(Format::WebP),
        _ => None,
    }
}

/// Decodes `bytes` and shrinks the picture to `width_px` when it is wider (0
/// keeps its size).
pub fn decode(bytes: &[u8], width_px: u32) -> Result<Picture, DecodeError> {
    let format = sniff(bytes).ok_or(DecodeError::Format)?;
    let decoded = catch_unwind(AssertUnwindSafe(|| match format {
        Format::Jpeg => jpeg(bytes),
        Format::Png => png(bytes),
        Format::WebP => webp(bytes),
        Format::Gif => gif(bytes),
    }))
    .map_err(|_| DecodeError::Corrupt("decoder panicked".into()))??;
    let mut picture = decoded;
    premultiply(&mut picture.pixels);
    if width_px > 0 && picture.width > width_px {
        let height =
            ((picture.height as u64 * width_px as u64 + picture.width as u64 / 2) / picture.width as u64).max(1) as u32;
        picture = downscale(&picture, width_px, height);
    }
    Ok(picture)
}

/// Refuses a size before anything is allocated for it.
fn check_size(width: u32, height: u32) -> Result<(), DecodeError> {
    if width == 0 || height == 0 {
        return Err(DecodeError::Corrupt("empty picture".into()));
    }
    if width > MAX_SIDE || height > MAX_SIDE || width as usize * height as usize * 4 > MAX_BYTES {
        return Err(DecodeError::TooBig);
    }
    Ok(())
}

fn corrupt(e: impl fmt::Display) -> DecodeError {
    DecodeError::Corrupt(e.to_string())
}

fn jpeg(bytes: &[u8]) -> Result<Picture, DecodeError> {
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;

    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(MAX_SIDE as usize)
        .set_max_height(MAX_SIDE as usize);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(Cursor::new(bytes), options);
    decoder.decode_headers().map_err(|e| match e {
        zune_jpeg::errors::DecodeErrors::LargeDimensions(_) => DecodeError::TooBig,
        e => corrupt(e),
    })?;
    let (w, h) = decoder.dimensions().ok_or_else(|| corrupt("no size"))?;
    check_size(w as u32, h as u32)?;
    let pixels = decoder.decode().map_err(corrupt)?;
    if pixels.len() != w * h * 4 {
        return Err(corrupt("unexpected pixel layout"));
    }
    Ok(Picture { width: w as u32, height: h as u32, pixels })
}

fn png(bytes: &[u8]) -> Result<Picture, DecodeError> {
    let mut decoder = png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: MAX_BYTES });
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(corrupt)?;
    let (w, h) = (reader.info().width, reader.info().height);
    check_size(w, h)?;
    let size = reader.output_buffer_size().ok_or(DecodeError::TooBig)?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(corrupt)?;
    buf.truncate(info.buffer_size());
    let pixels = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => expand(&buf, 3, |p| [p[0], p[1], p[2], 255]),
        png::ColorType::GrayscaleAlpha => expand(&buf, 2, |p| [p[0], p[0], p[0], p[1]]),
        png::ColorType::Grayscale => expand(&buf, 1, |p| [p[0], p[0], p[0], 255]),
        png::ColorType::Indexed => return Err(corrupt("palette left unexpanded")),
    };
    if pixels.len() != w as usize * h as usize * 4 {
        return Err(corrupt("unexpected pixel layout"));
    }
    Ok(Picture { width: w, height: h, pixels })
}

fn webp(bytes: &[u8]) -> Result<Picture, DecodeError> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(corrupt)?;
    let (w, h) = decoder.dimensions();
    check_size(w, h)?;
    decoder.set_memory_limit(MAX_BYTES);
    let size = decoder.output_buffer_size().ok_or(DecodeError::TooBig)?;
    let mut buf = vec![0; size];
    decoder.read_image(&mut buf).map_err(corrupt)?;
    let pixels = if decoder.has_alpha() { buf } else { expand(&buf, 3, |p| [p[0], p[1], p[2], 255]) };
    if pixels.len() != w as usize * h as usize * 4 {
        return Err(corrupt("unexpected pixel layout"));
    }
    Ok(Picture { width: w, height: h, pixels })
}

/// The first frame of a GIF on its full canvas (a frame may cover only part).
fn gif(bytes: &[u8]) -> Result<Picture, DecodeError> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.set_memory_limit(gif::MemoryLimit::Bytes(std::num::NonZeroU64::new(MAX_BYTES as u64).expect("non-zero")));
    let mut decoder = options.read_info(Cursor::new(bytes)).map_err(corrupt)?;
    let (w, h) = (decoder.width() as u32, decoder.height() as u32);
    check_size(w, h)?;
    let mut pixels = vec![0u8; w as usize * h as usize * 4];
    let frame = decoder.read_next_frame().map_err(corrupt)?.ok_or_else(|| corrupt("no frame"))?;
    let (fx, fy, fw, fh) = (frame.left as usize, frame.top as usize, frame.width as usize, frame.height as usize);
    for row in 0..fh {
        let y = fy + row;
        if y >= h as usize {
            break;
        }
        let x_end = (fx + fw).min(w as usize);
        if fx >= x_end {
            break;
        }
        let n = (x_end - fx) * 4;
        let src = &frame.buffer[row * fw * 4..][..n];
        pixels[(y * w as usize + fx) * 4..][..n].copy_from_slice(src);
    }
    Ok(Picture { width: w, height: h, pixels })
}

fn expand(buf: &[u8], channels: usize, f: impl Fn(&[u8]) -> [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len() / channels * 4);
    for p in buf.chunks_exact(channels) {
        out.extend_from_slice(&f(p));
    }
    out
}

/// Straight RGBA to premultiplied, in place.
pub fn premultiply(pixels: &mut [u8]) {
    for p in pixels.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        if a < 255 {
            for c in &mut p[..3] {
                // Exact rounding of c × a / 255.
                let v = *c as u32 * a + 128;
                *c = ((v + (v >> 8)) >> 8) as u8;
            }
        }
    }
}

/// Shrinks a premultiplied picture to `width` × `height` by averaging the
/// source pixels each destination pixel covers, partial pixels weighted by
/// how much of them it covers: sharp, without the aliasing of skipping
/// pixels, and fast because each axis is done once.
pub fn downscale(src: &Picture, width: u32, height: u32) -> Picture {
    let (sw, sh) = (src.width as usize, src.height as usize);
    let (dw, dh) = (width.clamp(1, src.width) as usize, height.clamp(1, src.height) as usize);
    let xs = spans(sw, dw);
    let ys = spans(sh, dh);
    // One source row shrunk horizontally, kept as weighted sums (weights in
    // 1/256 of a source pixel) so rounding happens once; the vertical pass
    // sums those rows, so memory stays at a few destination rows.
    let mut row = vec![0u32; dw * 4];
    let mut row_of = usize::MAX;
    let mut acc = vec![0u64; dw * 4];
    let mut pixels = vec![0u8; dw * dh * 4];
    for (y, span) in ys.iter().enumerate() {
        acc.fill(0);
        for (i, w) in span.weights.iter().enumerate() {
            let sy = span.start + i;
            // A source row shared by two destination rows is shrunk once.
            if sy != row_of {
                shrink_row(&src.pixels[sy * sw * 4..(sy + 1) * sw * 4], &xs, &mut row);
                row_of = sy;
            }
            for (a, r) in acc.iter_mut().zip(&row) {
                *a += *r as u64 * *w as u64;
            }
        }
        let out = &mut pixels[y * dw * 4..(y + 1) * dw * 4];
        for (x, column) in xs.iter().enumerate() {
            let total = (span.total as u64 * column.total as u64).max(1);
            for c in 0..4 {
                out[x * 4 + c] = ((acc[x * 4 + c] + total / 2) / total).min(255) as u8;
            }
        }
    }
    Picture { width: dw as u32, height: dh as u32, pixels }
}

fn shrink_row(line: &[u8], xs: &[Span], out: &mut [u32]) {
    for (x, span) in xs.iter().enumerate() {
        let mut acc = [0u32; 4];
        for (i, w) in span.weights.iter().enumerate() {
            let p = &line[(span.start + i) * 4..][..4];
            for c in 0..4 {
                acc[c] += p[c] as u32 * *w;
            }
        }
        out[x * 4..x * 4 + 4].copy_from_slice(&acc);
    }
}

/// The source pixels one destination pixel covers along an axis, and how
/// much of each (in 1/256 of a pixel).
struct Span {
    start: usize,
    weights: Vec<u32>,
    total: u32,
}

const ONE: u64 = 256;

fn spans(src: usize, dst: usize) -> Vec<Span> {
    // Edges in 1/256 source pixels: destination pixel i covers
    // [i × src / dst, (i + 1) × src / dst).
    let edge = |i: usize| (i as u64 * src as u64 * ONE) / dst as u64;
    (0..dst)
        .map(|i| {
            let (a, b) = (edge(i), edge(i + 1));
            let start = (a / ONE) as usize;
            let end = b.div_ceil(ONE).max(a / ONE + 1) as usize;
            let weights: Vec<u32> = (start..end.min(src))
                .map(|p| {
                    let (p0, p1) = (p as u64 * ONE, (p as u64 + 1) * ONE);
                    (p1.min(b) - p0.max(a)) as u32
                })
                .collect();
            let total = weights.iter().sum();
            Span { start, weights, total }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Picture {
        Picture { width: w, height: h, pixels: rgba.iter().copied().cycle().take((w * h * 4) as usize).collect() }
    }

    fn png_bytes(w: u32, h: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(data).unwrap();
        out
    }

    #[test]
    fn premultiplies_exactly() {
        let mut p = [255, 128, 0, 128, 10, 20, 30, 255, 200, 200, 200, 0];
        premultiply(&mut p);
        assert_eq!(p, [128, 64, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn halving_averages_blocks() {
        let mut src = solid(4, 2, [0, 0, 0, 255]);
        // Left half white, right half black.
        for y in 0..2 {
            for x in 0..2 {
                let i = (y * 4 + x) * 4;
                src.pixels[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let out = downscale(&src, 2, 1);
        assert_eq!((out.width, out.height), (2, 1));
        assert_eq!(out.pixels, vec![255, 255, 255, 255, 0, 0, 0, 255]);
    }

    #[test]
    fn fractional_ratios_weigh_partial_pixels() {
        // Three pixels into two: the middle one is split between both.
        let src = Picture { width: 3, height: 1, pixels: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255] };
        let out = downscale(&src, 2, 1);
        // Left: (1 × red + 0.5 × green) / 1.5.
        assert_eq!(&out.pixels[0..4], &[170, 85, 0, 255]);
        assert_eq!(&out.pixels[4..8], &[0, 85, 170, 255]);
    }

    #[test]
    fn solid_stays_solid_at_any_ratio() {
        let src = solid(1920, 1080, [12, 34, 56, 255]);
        for w in [1280u32, 960, 481, 1] {
            let h = (1080 * w).div_ceil(1920);
            let out = downscale(&src, w, h);
            assert!(out.pixels.as_chunks::<4>().0.iter().all(|p| *p == [12, 34, 56, 255]), "{w}");
        }
    }

    #[test]
    fn decodes_png_of_every_colour_type_to_premultiplied_rgba() {
        let rgba = png_bytes(2, 1, png::ColorType::Rgba, &[255, 0, 0, 255, 255, 255, 255, 128]);
        let p = decode(&rgba, 0).unwrap();
        assert_eq!(p.pixels, vec![255, 0, 0, 255, 128, 128, 128, 128]);
        let rgb = png_bytes(1, 1, png::ColorType::Rgb, &[1, 2, 3]);
        assert_eq!(decode(&rgb, 0).unwrap().pixels, vec![1, 2, 3, 255]);
        let gray = png_bytes(1, 1, png::ColorType::Grayscale, &[9]);
        assert_eq!(decode(&gray, 0).unwrap().pixels, vec![9, 9, 9, 255]);
    }

    #[test]
    fn shrinks_to_the_width_asked_keeping_the_aspect() {
        let data = vec![200u8; 300 * 450 * 3];
        let bytes = png_bytes(300, 450, png::ColorType::Rgb, &data);
        let p = decode(&bytes, 216).unwrap();
        assert_eq!((p.width, p.height), (216, 324));
        // Never enlarged.
        let p = decode(&bytes, 480).unwrap();
        assert_eq!((p.width, p.height), (300, 450));
    }

    #[test]
    fn refuses_what_it_cant_read() {
        assert_eq!(decode(b"<svg xmlns=...>", 100), Err(DecodeError::Format));
        assert!(matches!(decode(&[0x89, b'P', b'N', b'G', 0, 0], 100), Err(DecodeError::Corrupt(_))));
        let huge = png_bytes(4097, 1, png::ColorType::Grayscale, &[0; 4097]);
        assert_eq!(decode(&huge, 0), Err(DecodeError::TooBig));
    }

    #[test]
    fn gif_first_frame_lands_on_its_canvas() {
        let mut out = Vec::new();
        {
            let mut enc = gif::Encoder::new(&mut out, 4, 4, &[0, 0, 0, 255, 255, 255]).unwrap();
            let mut frame = gif::Frame { left: 2, top: 1, width: 2, height: 2, ..gif::Frame::default() };
            frame.buffer = std::borrow::Cow::Owned(vec![1, 1, 1, 1]);
            enc.write_frame(&frame).unwrap();
        }
        let p = decode(&out, 0).unwrap();
        assert_eq!((p.width, p.height), (4, 4));
        let at = |x: usize, y: usize| &p.pixels[(y * 4 + x) * 4..][..4];
        assert_eq!(at(2, 1), &[255, 255, 255, 255]);
        assert_eq!(at(0, 0), &[0, 0, 0, 0]);
    }
}
