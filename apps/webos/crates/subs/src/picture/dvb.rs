//! Broadcast subtitles (DVB, ETSI EN 300 743, `S_DVBSUB`): each sample is a
//! display set of segments. A page composition places regions on the
//! display; each region has a colour table (CLUT) and objects drawn into it
//! as 2, 4 or 8-bit run-length coded pixels, a field at a time. A page in
//! "normal case" only updates what an acquisition point or mode change
//! defined, so a sample is decoded from the last of those.

use std::collections::HashMap;
use std::sync::Arc;

use super::{FrameCache, PictureDecoder, PictureFrame, PictureSample, fits, placed, ycbcr_to_rgb};

const PAGE: u8 = 0x10;
const REGION: u8 = 0x11;
const CLUT: u8 = 0x12;
const OBJECT: u8 = 0x13;
const DISPLAY: u8 = 0x14;

/// A segment: its type and where its body is in the sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub kind: u8,
    pub start: usize,
    pub end: usize,
}

/// The segments of a display set.
pub fn segments(data: &[u8]) -> Vec<Segment> {
    let mut out = Vec::new();
    // data_identifier 0x20 and subtitle_stream_id 0x00 open a PES data field.
    let mut at = if data.starts_with(&[0x20, 0x00]) { 2 } else { 0 };
    while at + 6 <= data.len() && data[at] == 0x0F {
        let length = (usize::from(data[at + 4]) << 8) | usize::from(data[at + 5]);
        let start = at + 6;
        out.push(Segment { kind: data[at + 1], start, end: (start + length).min(data.len()) });
        at = start + length;
    }
    out
}

struct Page {
    timeout_s: u8,
    /// 0 normal case, 1 acquisition point, 2 mode change.
    state: u8,
    /// Region id, x, y.
    regions: Vec<(u8, u32, u32)>,
}

fn u16_at(b: &[u8], at: usize) -> u32 {
    (u32::from(b[at]) << 8) | u32::from(b[at + 1])
}

fn read_page(b: &[u8]) -> Page {
    let regions =
        b.get(2..).unwrap_or_default().as_chunks::<6>().0.iter().map(|r| (r[0], u16_at(r, 2), u16_at(r, 4))).collect();
    Page { timeout_s: b.first().copied().unwrap_or(0), state: (b.get(1).copied().unwrap_or(0) >> 2) & 3, regions }
}

struct Region {
    width: usize,
    height: usize,
    /// 2, 4 or 8 bits a pixel (0 for a region that says none).
    depth: u8,
    clut: u8,
    /// Object id, x, y.
    objects: Vec<(u16, usize, usize)>,
    pixels: Vec<u8>,
}

/// A CLUT's three tables (2, 4 and 8-bit), RGBA each.
type Tables = [Vec<[u8; 4]>; 3];

/// The colour tables of ETSI EN 300 743 §10 for pixels a CLUT doesn't define.
fn default_clut(depth: u8) -> Vec<[u8; 4]> {
    // Halves round to even, as JavaScript's Uint8ClampedArray stores them.
    let c = |v: f64| v.round_ties_even().clamp(0.0, 255.0) as u8;
    match depth {
        2 => vec![[0, 0, 0, 0], [255, 255, 255, 255], [0, 0, 0, 255], [127, 127, 127, 255]],
        4 => (0..16u8)
            .map(|i| {
                let v = if i < 8 { 255 } else { 127 };
                let on = |bit: u8| if i & bit != 0 { v } else { 0 };
                [on(1), on(2), on(4), if i == 0 { 0 } else { 255 }]
            })
            .collect(),
        _ => (0..=255u8)
            .map(|i| {
                let base = |bit: u8| if i & bit != 0 { 85.0 } else { 0.0 };
                let (r, g, b) = (base(0x01), base(0x02), base(0x04));
                let add = |bit: u8, v: f64| if i & bit != 0 { v } else { 0.0 };
                if i < 8 {
                    [c(r * 3.0), c(g * 3.0), c(b * 3.0), if i == 0 { 0 } else { 64 }]
                } else {
                    match i & 0x88 {
                        0x00 => [c(r + add(0x10, 170.0)), c(g + add(0x20, 170.0)), c(b + add(0x40, 170.0)), 255],
                        0x08 => [c(r + add(0x10, 170.0)), c(g + add(0x20, 170.0)), c(b + add(0x40, 170.0)), 127],
                        0x80 => [
                            c(127.0 + r / 2.0 + add(0x10, 43.0)),
                            c(127.0 + g / 2.0 + add(0x20, 43.0)),
                            c(127.0 + b / 2.0 + add(0x40, 43.0)),
                            255,
                        ],
                        _ => [
                            c(r / 2.0 + add(0x10, 43.0)),
                            c(g / 2.0 + add(0x20, 43.0)),
                            c(b / 2.0 + add(0x40, 43.0)),
                            255,
                        ],
                    }
                }
            })
            .collect(),
    }
}

fn default_tables() -> Tables {
    [default_clut(2), default_clut(4), default_clut(8)]
}

/// Bits of a pixel string, most significant first; zeros past its end.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    bit: u8,
}

impl Bits<'_> {
    fn done(&self) -> bool {
        self.at >= self.data.len()
    }

    fn read(&mut self, n: u32) -> usize {
        let mut v = 0;
        for _ in 0..n {
            let byte = self.data.get(self.at).copied().unwrap_or(0);
            v = (v << 1) | usize::from((byte >> (7 - self.bit)) & 1);
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.at += 1;
            }
        }
        v
    }

    /// The byte after the string: strings end on a byte.
    fn align(&mut self) -> usize {
        if self.bit != 0 {
            self.bit = 0;
            self.at += 1;
        }
        self.at
    }
}

/// The runs of one pixel string, `(count, pixel)`, until its end code, as
/// EN 300 743 codes them for each depth.
fn pixel_string(bits: &mut Bits, depth: u8, mut emit: impl FnMut(usize, usize)) {
    while !bits.done() {
        match depth {
            2 => {
                let v = bits.read(2);
                if v != 0 {
                    emit(1, v);
                } else if bits.read(1) == 1 {
                    let run = bits.read(3) + 3;
                    emit(run, bits.read(2));
                } else if bits.read(1) == 1 {
                    emit(1, 0);
                } else {
                    match bits.read(2) {
                        0 => return,
                        1 => emit(2, 0),
                        2 => {
                            let run = bits.read(4) + 12;
                            emit(run, bits.read(2));
                        }
                        _ => {
                            let run = bits.read(8) + 29;
                            emit(run, bits.read(2));
                        }
                    }
                }
            }
            4 => {
                let v = bits.read(4);
                if v != 0 {
                    emit(1, v);
                } else if bits.read(1) == 0 {
                    let run = bits.read(3);
                    if run == 0 {
                        return;
                    }
                    emit(run + 2, 0);
                } else if bits.read(1) == 0 {
                    let run = bits.read(2) + 4;
                    emit(run, bits.read(4));
                } else {
                    match bits.read(2) {
                        0 => emit(1, 0),
                        1 => emit(2, 0),
                        2 => {
                            let run = bits.read(4) + 9;
                            emit(run, bits.read(4));
                        }
                        _ => {
                            let run = bits.read(8) + 25;
                            emit(run, bits.read(4));
                        }
                    }
                }
            }
            _ => {
                let v = bits.read(8);
                if v != 0 {
                    emit(1, v);
                } else if bits.read(1) == 0 {
                    let run = bits.read(7);
                    if run == 0 {
                        return;
                    }
                    emit(run, 0);
                } else {
                    let run = bits.read(7);
                    emit(run, bits.read(8));
                }
            }
        }
    }
}

/// Draws one field of an object's pixel data into `region` at (`ox`, `oy`),
/// every other line from `first`.
fn draw_field(data: &[u8], region: &mut Region, ox: usize, oy: usize, first: usize) {
    let byte = |i: usize| data.get(i).copied().unwrap_or(0);
    let (mut at, mut x, mut y) = (0, ox, oy + first);
    // Map tables, for codes shallower than the region.
    let mut map24 = [0x0, 0x7, 0x8, 0xF];
    let mut map28 = [0x00, 0x77, 0x88, 0xFF];
    let mut map48: [u8; 16] = std::array::from_fn(|i| i as u8 * 0x11);
    while at < data.len() {
        let kind = data[at];
        at += 1;
        let depth = match kind {
            0xF0 => {
                x = ox;
                y += 2;
                continue;
            }
            0x20 => {
                map24 = [byte(at) >> 4, byte(at) & 0xF, byte(at + 1) >> 4, byte(at + 1) & 0xF];
                at += 2;
                continue;
            }
            0x21 => {
                map28 = [byte(at), byte(at + 1), byte(at + 2), byte(at + 3)];
                at += 4;
                continue;
            }
            0x22 => {
                map48 =
                    std::array::from_fn(|i| if i % 2 == 0 { byte(at + i / 2) >> 4 } else { byte(at + i / 2) & 0xF });
                at += 8;
                continue;
            }
            0x10 => 2,
            0x11 => 4,
            0x12 => 8,
            _ => break,
        };
        let region_depth = region.depth;
        let map = |p: usize| -> u8 {
            if depth == region_depth {
                p as u8
            } else if depth == 2 {
                if region_depth == 4 { map24[p & 3] } else { map28[p & 3] }
            } else if depth == 4 && region_depth == 8 {
                map48[p & 0xF]
            } else {
                // Deeper codes than the region: keep the low bits.
                (p & ((1usize << region_depth) - 1)) as u8
            }
        };
        let mut bits = Bits { data, at, bit: 0 };
        pixel_string(&mut bits, depth, |count, pixel| {
            if y < region.height {
                let stop = region.width.min(x + count);
                let row = y * region.width;
                if x < stop {
                    region.pixels[row + x..row + stop].fill(map(pixel));
                }
            }
            x += count;
        });
        at = bits.align();
    }
}

/// Decodes DVB display sets, from the last acquisition point or mode change on.
#[derive(Default)]
pub struct DvbDecoder {
    parsed: Vec<Option<Vec<Segment>>>,
    cache: FrameCache,
}

impl DvbDecoder {
    /// A decoder with nothing decoded yet.
    pub fn new() -> DvbDecoder {
        DvbDecoder::default()
    }

    fn segs(&mut self, samples: &[PictureSample], index: usize) -> &[Segment] {
        if self.parsed.len() <= index {
            self.parsed.resize_with(index + 1, || None);
        }
        self.parsed[index].get_or_insert_with(|| segments(samples[index].data()))
    }

    fn page(&mut self, samples: &[PictureSample], index: usize) -> Option<Page> {
        let seg = *self.segs(samples, index).iter().find(|s| s.kind == PAGE)?;
        Some(read_page(&samples[index].data()[seg.start..seg.end]))
    }

    fn render(&mut self, samples: &[PictureSample], index: usize) -> Option<PictureFrame> {
        let mut from = index;
        while from > 0 && self.page(samples, from).is_none_or(|p| p.state == 0) {
            from -= 1;
        }
        let (mut width, mut height) = (720u32, 576u32);
        let mut page: Option<Page> = None;
        let mut regions: HashMap<u8, Region> = HashMap::new();
        let mut cluts: HashMap<u8, Tables> = HashMap::new();
        for i in from..=index {
            let data = samples[i].data();
            for seg in self.segs(samples, i).to_vec() {
                let b = &data[seg.start..seg.end];
                match seg.kind {
                    DISPLAY if b.len() >= 5 => {
                        width = u16_at(b, 1) + 1;
                        height = u16_at(b, 3) + 1;
                    }
                    PAGE => {
                        let p = read_page(b);
                        if p.state != 0 {
                            regions.clear();
                        }
                        page = Some(p);
                    }
                    REGION if b.len() >= 10 => read_region(b, &mut regions),
                    CLUT if b.len() >= 2 => read_clut(b, height > 576, &mut cluts),
                    OBJECT if b.len() >= 7 => draw_object(b, &mut regions),
                    _ => {}
                }
            }
        }
        let page = page?;
        let mut bitmaps = Vec::new();
        for &(id, x, y) in &page.regions {
            let Some(region) = regions.get(&id) else { continue };
            if region.width == 0 || region.height == 0 {
                continue;
            }
            let table = match region.depth {
                2 => 0,
                4 => 1,
                _ => 2,
            };
            let tables = cluts.get(&region.clut);
            let defaults;
            let table = match tables {
                Some(t) => &t[table],
                None => {
                    defaults = default_tables();
                    &defaults[table]
                }
            };
            let mut visible = false;
            let mut rgba = Vec::with_capacity(region.pixels.len() * 4);
            for p in &region.pixels {
                let c = table.get(usize::from(*p)).copied().unwrap_or([0; 4]);
                visible |= c[3] != 0;
                rgba.extend_from_slice(&c);
            }
            if visible {
                bitmaps.push(placed(x, y, region.width as u32, region.height as u32, rgba));
            }
        }
        Some(PictureFrame { plane_width: width, plane_height: height, bitmaps })
    }
}

fn read_region(b: &[u8], regions: &mut HashMap<u8, Region>) {
    let id = b[0];
    let fill = b[1] & 0x08 != 0;
    let (w, h) = (u16_at(b, 2) as usize, u16_at(b, 4) as usize);
    let depth = match (b[6] >> 2) & 7 {
        0 => 0,
        1 => 2,
        3 => 8,
        _ => 4,
    };
    let clut = b[7];
    let background = match depth {
        8 => b[8],
        4 => b[9] >> 4,
        _ => (b[9] >> 2) & 3,
    };
    let mut objects = Vec::new();
    let mut o = 10;
    while o + 6 <= b.len() {
        let kind = b[o + 2] >> 6;
        let id = u16_at(b, o) as u16;
        let x = (usize::from(b[o + 2] & 0x0F) << 8) | usize::from(b[o + 3]);
        let y = (usize::from(b[o + 4] & 0x0F) << 8) | usize::from(b[o + 5]);
        objects.push((id, x, y));
        o += if kind == 1 || kind == 2 { 8 } else { 6 };
    }
    if !fits(w, h) {
        return;
    }
    // The same size and depth keep their pixels.
    if !regions.get(&id).is_some_and(|r| r.width == w && r.height == h && r.depth == depth) {
        regions.insert(id, Region { width: w, height: h, depth, clut, objects: Vec::new(), pixels: vec![0; w * h] });
    }
    let Some(region) = regions.get_mut(&id) else { return };
    region.clut = clut;
    region.objects = objects;
    if fill {
        region.pixels.fill(background);
    }
}

fn read_clut(b: &[u8], hd: bool, cluts: &mut HashMap<u8, Tables>) {
    let tables = cluts.entry(b[0]).or_insert_with(default_tables);
    let mut at = 2;
    while at + 2 <= b.len() {
        let entry = usize::from(b[at]);
        let flags = b[at + 1];
        let (y, cr, cb, t) = if flags & 0x01 != 0 {
            if at + 6 > b.len() {
                break;
            }
            let v = (b[at + 2], b[at + 3], b[at + 4], b[at + 5]);
            at += 6;
            v
        } else {
            if at + 4 > b.len() {
                break;
            }
            let (b2, b3) = (b[at + 2], b[at + 3]);
            at += 4;
            (b2 & 0xFC, ((b2 & 0x03) << 6) | ((b3 >> 2) & 0x30), (b3 << 2) & 0xF0, (b3 & 0x03) * 0x55)
        };
        let [r, g, bl] = ycbcr_to_rgb(y, cb, cr, hd);
        // Y 0 is transparent whatever T says.
        let colour = [r, g, bl, if y == 0 { 0 } else { 255 - t }];
        for (table, bit) in tables.iter_mut().zip([0x80, 0x40, 0x20]) {
            if flags & bit != 0
                && let Some(slot) = table.get_mut(entry)
            {
                *slot = colour;
            }
        }
    }
}

fn draw_object(b: &[u8], regions: &mut HashMap<u8, Region>) {
    let id = u16_at(b, 0) as u16;
    // Coding method 0, pixels, only.
    if (b[2] >> 2) & 3 != 0 {
        return;
    }
    let top_len = u16_at(b, 3) as usize;
    let bottom_len = u16_at(b, 5) as usize;
    let field = |from: usize, len: usize| &b[from.min(b.len())..(from + len).min(b.len())];
    let top = field(7, top_len);
    // An empty bottom field repeats the top one.
    let bottom = if bottom_len == 0 { top } else { field(7 + top_len, bottom_len) };
    for region in regions.values_mut() {
        let placed: Vec<(usize, usize)> = region.objects.iter().filter(|o| o.0 == id).map(|o| (o.1, o.2)).collect();
        for (x, y) in placed {
            draw_field(top, region, x, y, 0);
            draw_field(bottom, region, x, y, 1);
        }
    }
}

impl PictureDecoder for DvbDecoder {
    fn decode(&mut self, samples: &[PictureSample], index: usize) -> Option<Arc<PictureFrame>> {
        let mut cache = std::mem::take(&mut self.cache);
        let frame = cache.frame(index, || self.render(samples, index));
        self.cache = cache;
        frame
    }

    fn own_duration_ms(&mut self, samples: &[PictureSample], index: usize) -> Option<u64> {
        self.page(samples, index).filter(|p| p.timeout_s > 0).map(|p| u64::from(p.timeout_s) * 1000)
    }

    fn forget_from(&mut self, index: usize) {
        self.parsed.truncate(index);
        self.cache.forget_from(index);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::u16b;
    use super::*;

    fn segment(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![0x0F, kind];
        out.extend(u16b(1));
        out.extend(u16b(body.len() as u32));
        out.extend(body);
        out
    }

    /// A mode-change page (time-out 5 s) with one 4-bit region of 4×2 at
    /// (60, 500), CLUT entry 1 red, the object four pixels of entry 1.
    fn display_set() -> Vec<u8> {
        let mut page = vec![5, 2 << 2, 0, 0];
        page.extend(u16b(60));
        page.extend(u16b(500));
        let mut region = vec![0, 0x00];
        region.extend(u16b(4));
        region.extend(u16b(2));
        region.extend([(2 << 5) | (2 << 2), 0, 0, 0]);
        region.extend(u16b(0));
        region.extend([0, 0, 0, 0]);
        // CLUT 0: entry 1 of the 4-bit table, full range: Y 81 Cr 240 Cb 90 (red), opaque.
        let clut = [0, 0, 1, 0x40 | 0x01, 81, 240, 90, 0];
        // A 4-bit string: 4 × pixel 1, its end (0000 0 000), then the end of the line.
        let line = [0x11, 0x11, 0x11, 0x00, 0x00, 0xF0];
        let mut object = u16b(0).to_vec();
        object.push(0x00);
        object.extend(u16b(line.len() as u32));
        object.extend(u16b(0));
        object.extend(line);
        let mut out = vec![0x20, 0x00];
        out.extend(segment(PAGE, &page));
        out.extend(segment(REGION, &region));
        out.extend(segment(CLUT, &clut));
        out.extend(segment(OBJECT, &object));
        out.extend(segment(0x80, &[]));
        out.push(0xFF);
        out
    }

    #[test]
    fn splits_a_display_set_into_segments() {
        let kinds: Vec<u8> = segments(&display_set()).iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [0x10, 0x11, 0x12, 0x13, 0x80]);
    }

    #[test]
    fn draws_objects_into_regions_placed_by_the_page() {
        let samples = [PictureSample::new(0, None, display_set())];
        let mut decoder = DvbDecoder::new();
        let frame = decoder.decode(&samples, 0).unwrap();
        assert_eq!((frame.plane_width, frame.plane_height), (720, 576));
        let b = &frame.bitmaps[0];
        assert_eq!((b.x, b.y, b.image.width, b.image.height), (60, 500, 4, 2));
        let [r, g, bl] = ycbcr_to_rgb(81, 90, 240, false);
        assert_eq!(b.image.pixels[..4], [r, g, bl, 255]);
        // The bottom field is the top one: both lines drawn.
        assert_eq!(b.image.pixels[4 * 4 + 3], 255);
        assert_eq!(decoder.own_duration_ms(&samples, 0), Some(5000));
    }

    #[test]
    fn decodes_a_normal_case_page_from_the_last_mode_change() {
        // A normal-case page that only moves the region: the pixels come from the mode change.
        let mut page = vec![0, 0, 0, 0];
        page.extend(u16b(100));
        page.extend(u16b(400));
        let mut update = vec![0x20, 0x00];
        update.extend(segment(PAGE, &page));
        let mut display = vec![0];
        display.extend(u16b(1919));
        display.extend(u16b(1079));
        update.extend(segment(DISPLAY, &display));
        let samples = [PictureSample::new(0, None, display_set()), PictureSample::new(1000, None, update)];
        let frame = DvbDecoder::new().decode(&samples, 1).unwrap();
        assert_eq!((frame.plane_width, frame.plane_height), (1920, 1080));
        assert_eq!((frame.bitmaps[0].x, frame.bitmaps[0].y), (100, 400));
        // A page without regions shows nothing.
        let mut empty = vec![0x20, 0x00];
        empty.extend(segment(PAGE, &[0, 2 << 2]));
        let frame = DvbDecoder::new().decode(&[PictureSample::new(0, None, empty)], 0).unwrap();
        assert!(frame.bitmaps.is_empty());
    }

    #[test]
    fn refuses_regions_larger_than_a_4k_picture() {
        let mut page = vec![5, 2 << 2, 0, 0];
        page.extend(u16b(0));
        page.extend(u16b(0));
        let mut region = vec![0, 0x08];
        region.extend(u16b(0xFFFF));
        region.extend(u16b(0xFFFF));
        region.extend([(2 << 5) | (2 << 2), 0, 0, 0x10]);
        let mut set = vec![0x20, 0x00];
        set.extend(segment(PAGE, &page));
        set.extend(segment(REGION, &region));
        let frame = DvbDecoder::new().decode(&[PictureSample::new(0, None, set)], 0).unwrap();
        assert!(frame.bitmaps.is_empty());
    }

    #[test]
    fn reads_every_run_length_code() {
        // 2-bit: a pixel, 3+3 of 1, a 0, 2 × 0, 12+1 of 2, 29+0 of 3, the end.
        let two = [0x48, 0x44, 0x10, 0x86, 0x0C, 0x03, 0x00];
        let mut runs = Vec::new();
        pixel_string(&mut Bits { data: &two, at: 0, bit: 0 }, 2, |n, p| runs.push((n, p)));
        assert_eq!(runs, [(1, 1), (3, 1), (1, 0), (2, 0), (13, 2), (29, 3)]);
        // 8-bit: a pixel, 5 × 0, 3 × 7, the end.
        let eight = [9, 0, 5, 0, 0x83, 7, 0, 0];
        let mut runs = Vec::new();
        pixel_string(&mut Bits { data: &eight, at: 0, bit: 0 }, 8, |n, p| runs.push((n, p)));
        assert_eq!(runs, [(1, 9), (5, 0), (3, 7)]);
    }

    #[test]
    fn default_tables_follow_the_standard() {
        let eight = default_clut(8);
        assert_eq!(eight[0], [0, 0, 0, 0]);
        assert_eq!(eight[1], [255, 0, 0, 64]);
        assert_eq!(eight[0x10], [170, 0, 0, 255]);
        // Halves round to even: 127 + 42.5 is 170, 42.5 is 42, 42.5 + 43 is 86.
        assert_eq!(eight[0x81], [170, 127, 127, 255]);
        assert_eq!(eight[0x89], [42, 0, 0, 255]);
        assert_eq!(eight[0x99], [86, 0, 0, 255]);
        assert_eq!(default_clut(4)[9], [127, 0, 0, 255]);
    }
}
