//! DVD subtitles (VobSub, `S_VOBSUB`): each sample is one subpicture unit
//! (SPU): two interlaced fields of 2-bit run-length pixels, then control
//! sequences that say when it shows and stops, which 4 of the 16 palette
//! colours its pixels take, their alpha, and where it goes. The palette and
//! the plane's size come from the track's private data, the `.idx` header
//! (`size: 720x480`, `palette: 000000, ffffff, …`).

use std::sync::Arc;

use super::{FrameCache, PictureDecoder, PictureFrame, PictureSample, fits, placed};

/// What the `.idx` header says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VobSubHeader {
    pub width: u32,
    pub height: u32,
    /// At least 16 RGB colours.
    pub palette: Vec<[u8; 3]>,
}

/// Reads the `.idx` header (Latin-1 text); a DVD's 720×576 and alternating
/// black and white for what it doesn't say.
pub fn parse_idx(codec_private: Option<&[u8]>) -> VobSubHeader {
    let text: String = codec_private.unwrap_or_default().iter().map(|b| *b as char).collect();
    let lower = text.to_ascii_lowercase();
    let (mut width, mut height) = (720, 576);
    for (at, _) in lower.match_indices("size:") {
        if let Some(size) = size_after(&lower[at + 5..]) {
            (width, height) = size;
            break;
        }
    }
    let mut palette = Vec::new();
    if let Some(at) = lower.find("palette:") {
        let list = lower[at + 8..].trim_start();
        let end = list.find(|c: char| !(c.is_ascii_hexdigit() || c == ',' || c.is_whitespace())).unwrap_or(list.len());
        for hex in list[..end].split(',') {
            // As JavaScript's parseInt: the hex digits it starts with, if any.
            let digits: String = hex.trim().chars().take_while(char::is_ascii_hexdigit).collect();
            if digits.is_empty() {
                continue;
            }
            let v = digits.chars().fold(0u32, |v, c| v.wrapping_shl(4) | c.to_digit(16).unwrap_or(0));
            palette.push([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
        }
    }
    while palette.len() < 16 {
        palette.push(if palette.len() % 2 == 1 { [255, 255, 255] } else { [0, 0, 0] });
    }
    VobSubHeader { width, height, palette }
}

/// `\s*(\d+)\s*x\s*(\d+)` at the start of `s`.
fn size_after(s: &str) -> Option<(u32, u32)> {
    let number = |s: &str| -> Option<(u32, usize)> {
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        Some((s[..digits].parse().ok()?, digits))
    };
    let s = s.trim_start();
    let (w, n) = number(s)?;
    let s = s[n..].trim_start().strip_prefix('x')?.trim_start();
    let (h, _) = number(s)?;
    Some((w, h))
}

/// A subpicture unit's control sequences.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spu {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    /// Palette entries for pixel values 0 to 3.
    pub colors: [u8; 4],
    /// Alpha 0 to 15 for pixel values 0 to 3.
    pub alpha: [u8; 4],
    /// Where the fields' pixels start.
    pub top: usize,
    pub bottom: usize,
    /// From the stop command; `None` without one.
    pub stop_ms: Option<u64>,
    pub start_ms: u64,
}

fn u16_at(b: &[u8], at: usize) -> usize {
    (usize::from(b[at]) << 8) | usize::from(b[at + 1])
}

/// Reads the control sequences of an SPU; `None` when it is cut short.
pub fn parse_spu(data: &[u8]) -> Option<Spu> {
    if data.len() < 4 {
        return None;
    }
    let size = u16_at(data, 0);
    let ctrl = u16_at(data, 2);
    let end = if size == 0 { data.len() } else { size.min(data.len()) };
    let mut spu = Spu {
        x1: 0,
        y1: 0,
        x2: -1,
        y2: -1,
        colors: [0, 1, 2, 3],
        alpha: [0, 15, 15, 15],
        top: 0,
        bottom: 0,
        stop_ms: None,
        start_ms: 0,
    };
    let mut seq = ctrl;
    let mut seen = Vec::new();
    while seq + 4 <= end && !seen.contains(&seq) {
        seen.push(seq);
        // Delays count in 1024 ticks of a 90 kHz clock.
        let delay_ms = (u16_at(data, seq) as u64 * 2048 + 90) / 180;
        let next = u16_at(data, seq + 2);
        let mut at = seq + 4;
        while at < end {
            let cmd = data[at];
            at += 1;
            match cmd {
                // Forced start, start.
                0x00 | 0x01 => spu.start_ms = delay_ms,
                0x02 => spu.stop_ms = Some(delay_ms),
                0x03 | 0x04 => {
                    if at + 2 > end {
                        return None;
                    }
                    let v = [data[at + 1] & 0x0F, data[at + 1] >> 4, data[at] & 0x0F, data[at] >> 4];
                    if cmd == 0x03 {
                        spu.colors = v;
                    } else {
                        spu.alpha = v;
                    }
                    at += 2;
                }
                0x05 => {
                    if at + 6 > end {
                        return None;
                    }
                    let b = |i: usize| i32::from(data[at + i]);
                    spu.x1 = (b(0) << 4) | (b(1) >> 4);
                    spu.x2 = ((b(1) & 0x0F) << 8) | b(2);
                    spu.y1 = (b(3) << 4) | (b(4) >> 4);
                    spu.y2 = ((b(4) & 0x0F) << 8) | b(5);
                    at += 6;
                }
                0x06 => {
                    if at + 4 > end {
                        return None;
                    }
                    spu.top = u16_at(data, at);
                    spu.bottom = u16_at(data, at + 2);
                    at += 4;
                }
                // 0xFF ends the sequence; an unknown command's length isn't known, so it does too.
                _ => break,
            }
        }
        if next == seq {
            break;
        }
        seq = next;
    }
    Some(spu)
}

/// The pixel data of a field, a nibble at a time; nothing but zeros past its end.
struct Nibbles<'a> {
    data: &'a [u8],
    at: usize,
    limit: usize,
}

impl Nibbles<'_> {
    fn read(&mut self) -> usize {
        if self.done() {
            return 0;
        }
        let byte = self.data.get(self.at >> 1).copied().unwrap_or(0);
        self.at += 1;
        usize::from(if self.at & 1 == 0 { byte & 0x0F } else { byte >> 4 })
    }

    fn done(&self) -> bool {
        self.at >= self.limit
    }
}

/// One field's lines (every other line of the picture) into `out`, from line `first`.
fn decode_field(data: &[u8], offset: usize, end: usize, (width, height): (usize, usize), first: usize, out: &mut [u8]) {
    let mut n = Nibbles { data, at: offset * 2, limit: end * 2 };
    let mut y = first;
    while y < height {
        let mut x = 0;
        while x < width {
            let mut v = n.read();
            if v < 0x4 {
                v = (v << 4) | n.read();
                if v < 0x10 {
                    v = (v << 4) | n.read();
                    if v < 0x40 {
                        v = (v << 4) | n.read();
                    }
                }
            }
            let color = (v & 3) as u8;
            let count = match v >> 2 {
                // To the end of the line.
                0 => width - x,
                count => count,
            };
            let stop = width.min(x + count);
            out[y * width + x..y * width + stop].fill(color);
            x = stop;
            if n.done() {
                break;
            }
        }
        // Lines start on a byte.
        n.at += n.at & 1;
        y += 2;
    }
}

/// Decodes VobSub subpictures in the header's palette.
pub struct VobSubDecoder {
    header: VobSubHeader,
    spus: Vec<Option<Option<Spu>>>,
    cache: FrameCache,
}

impl VobSubDecoder {
    /// A decoder for a track whose private data (the `.idx` header) is `codec_private`.
    pub fn new(codec_private: Option<&[u8]>) -> VobSubDecoder {
        VobSubDecoder { header: parse_idx(codec_private), spus: Vec::new(), cache: FrameCache::default() }
    }

    fn spu(&mut self, samples: &[PictureSample], index: usize) -> Option<Spu> {
        if self.spus.len() <= index {
            self.spus.resize_with(index + 1, || None);
        }
        self.spus[index].get_or_insert_with(|| parse_spu(samples[index].data())).clone()
    }

    fn render(&mut self, samples: &[PictureSample], index: usize) -> Option<PictureFrame> {
        let spu = self.spu(samples, index)?;
        let (plane_width, plane_height) = (self.header.width, self.header.height);
        let width = spu.x2 - spu.x1 + 1;
        let height = spu.y2 - spu.y1 + 1;
        if width <= 0 || height <= 0 || !fits(width as usize, height as usize) {
            return Some(PictureFrame { plane_width, plane_height, bitmaps: Vec::new() });
        }
        let (width, height) = (width as usize, height as usize);
        let data = samples[index].data();
        let size = u16_at(data, 0);
        let ctrl = u16_at(data, 2);
        let end = [ctrl, size, data.len()].into_iter().find(|v| *v != 0).unwrap_or(0).min(data.len());
        let mut pixels = vec![0u8; width * height];
        decode_field(data, spu.top, end, (width, height), 0, &mut pixels);
        decode_field(data, spu.bottom, end, (width, height), 1, &mut pixels);
        let lut: Vec<[u8; 4]> = (0..4)
            .map(|v| {
                let [r, g, b] = self.header.palette[usize::from(spu.colors[v])];
                [r, g, b, spu.alpha[v] * 17]
            })
            .collect();
        let rgba = pixels.iter().flat_map(|p| lut[usize::from(*p)]).collect();
        let bitmap = placed(spu.x1.max(0) as u32, spu.y1.max(0) as u32, width as u32, height as u32, rgba);
        Some(PictureFrame { plane_width, plane_height, bitmaps: vec![bitmap] })
    }
}

impl PictureDecoder for VobSubDecoder {
    fn decode(&mut self, samples: &[PictureSample], index: usize) -> Option<Arc<PictureFrame>> {
        let mut cache = std::mem::take(&mut self.cache);
        let frame = cache.frame(index, || self.render(samples, index));
        self.cache = cache;
        frame
    }

    fn own_duration_ms(&mut self, samples: &[PictureSample], index: usize) -> Option<u64> {
        let spu = self.spu(samples, index)?;
        spu.stop_ms.filter(|stop| *stop > spu.start_ms).map(|stop| stop - spu.start_ms)
    }

    fn forget_from(&mut self, index: usize) {
        self.spus.truncate(index);
        self.cache.forget_from(index);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::u16b;
    use super::*;

    const IDX: &str = "size: 720x480\npalette: 000000, ffffff, 808080, ff0000, 00ff00, 0000ff, 000000, 000000, \
                       000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000\n";

    /// An SPU of a 4×2 picture at (10, 400): the top line colour 1, the
    /// bottom colour 2, shown for 175 ticks of 1024/90 ms.
    fn spu() -> Vec<u8> {
        // 8-bit codes 0001 00cc: a count of 4 in colour cc, then a byte to align.
        let pixels = [0x11, 0x00, 0x12, 0x00];
        let ctrl_at = 4 + pixels.len() as u32;
        let stop_seq_at = ctrl_at + 24;
        let mut ctrl = Vec::new();
        ctrl.extend(u16b(0));
        ctrl.extend(u16b(stop_seq_at));
        ctrl.push(0x01);
        ctrl.extend([0x03, 0x32, 0x10]);
        ctrl.extend([0x04, 0xFF, 0xF0]);
        ctrl.extend([0x05, 0x00, 0xA0, 0x0D, 0x19, 0x01, 0x91]);
        ctrl.push(0x06);
        ctrl.extend(u16b(4));
        ctrl.extend(u16b(6));
        ctrl.push(0xFF);
        let mut stop = Vec::new();
        stop.extend(u16b(175));
        stop.extend(u16b(stop_seq_at));
        stop.extend([0x02, 0xFF]);
        let mut all = pixels.to_vec();
        all.extend(ctrl);
        all.extend(stop);
        let mut out = Vec::new();
        out.extend(u16b(4 + all.len() as u32));
        out.extend(u16b(ctrl_at));
        out.extend(all);
        out
    }

    #[test]
    fn reads_the_idx_header() {
        let h = parse_idx(Some(IDX.as_bytes()));
        assert_eq!((h.width, h.height), (720, 480));
        assert_eq!(h.palette[1], [255, 255, 255]);
        assert_eq!(h.palette[3], [255, 0, 0]);
        let none = parse_idx(None);
        assert_eq!((none.width, none.height, none.palette.len()), (720, 576, 16));
        assert_eq!((none.palette[0], none.palette[1]), ([0, 0, 0], [255, 255, 255]));
        // The first size that reads as one; colours as JavaScript's parseInt reads them; the rest alternating.
        let odd = parse_idx(Some(
            b"# VobSub\nsize: big\nSIZE: 1920 X 1080\npalette: 0000ff, 00ff00 , ff0000\ncustom colors: OFF",
        ));
        assert_eq!((odd.width, odd.height), (1920, 1080));
        assert_eq!(odd.palette[..4], [[0, 0, 255], [0, 255, 0], [255, 0, 0], [255, 255, 255]]);
    }

    #[test]
    fn reads_control_sequences_place_colours_and_when_it_stops() {
        let s = parse_spu(&spu()).unwrap();
        assert_eq!((s.x1, s.x2, s.y1, s.y2), (10, 13, 400, 401));
        assert_eq!(s.colors, [0, 1, 2, 3]);
        assert_eq!(s.alpha, [0, 15, 15, 15]);
        assert_eq!(s.stop_ms, Some((175.0f64 * 1024.0 / 90.0).round() as u64));
        assert_eq!(parse_spu(&spu()[..30]), None, "cut inside a command");
        assert_eq!(parse_spu(&[0, 1]), None);
    }

    #[test]
    fn decodes_interlaced_fields_in_the_palettes_colours() {
        let samples = [PictureSample::new(0, None, spu())];
        let mut decoder = VobSubDecoder::new(Some(IDX.as_bytes()));
        let frame = decoder.decode(&samples, 0).unwrap();
        assert_eq!((frame.plane_width, frame.plane_height), (720, 480));
        let b = &frame.bitmaps[0];
        assert_eq!((b.x, b.y, b.image.width, b.image.height), (10, 400, 4, 2));
        assert_eq!(b.image.pixels[..4], [255, 255, 255, 255]);
        assert_eq!(b.image.pixels[16..20], [128, 128, 128, 255]);
        assert_eq!(decoder.own_duration_ms(&samples, 0), Some((175.0f64 * 1024.0 / 90.0).round() as u64));
    }
}
