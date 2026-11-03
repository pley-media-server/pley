//! Blu-ray subtitles (HDMV PGS, `S_HDMV/PGS`): each sample is one display
//! set of segments: a presentation composition (PCS) placing objects on the
//! plane, windows (WDS), palettes (PDS) and run-length coded objects (ODS).
//! An epoch start or acquisition point holds everything it needs; other
//! display sets may reuse palettes and objects defined since.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use super::{FrameCache, PictureDecoder, PictureFrame, PictureSample, fits, placed, ycbcr_to_rgb};

const PDS: u8 = 0x14;
const ODS: u8 = 0x15;
const PCS: u8 = 0x16;
const END: u8 = 0x80;

#[derive(Clone, Copy)]
struct Crop {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

struct PlacedObject {
    id: u16,
    x: u32,
    y: u32,
    crop: Option<Crop>,
}

struct Composition {
    width: u32,
    height: u32,
    /// 0x00 normal, 0x40 acquisition point, 0x80 epoch start.
    state: u8,
    palette_id: u8,
    objects: Vec<PlacedObject>,
}

/// One palette entry: index, Y, Cr, Cb, alpha.
type PaletteEntry = [u8; 5];

struct ObjectPart {
    id: u16,
    first: bool,
    last: bool,
    width: u32,
    height: u32,
    /// In the sample's bytes.
    data: Range<usize>,
}

#[derive(Default)]
struct DisplaySet {
    pcs: Option<Composition>,
    palettes: Vec<(u8, Vec<PaletteEntry>)>,
    objects: Vec<ObjectPart>,
}

fn u16_at(b: &[u8], at: usize) -> u32 {
    (u32::from(b[at]) << 8) | u32::from(b[at + 1])
}

fn parse_display_set(data: &[u8]) -> DisplaySet {
    let mut set = DisplaySet::default();
    let mut at = 0;
    while at + 3 <= data.len() {
        // Some muxers keep the "PG" header of the .sup format: 2 magic bytes, PTS and DTS.
        if data[at] == 0x50 && data[at + 1] == 0x47 {
            at += 10;
            if at + 3 > data.len() {
                break;
            }
        }
        let kind = data[at];
        let size = u16_at(data, at + 1) as usize;
        let start = at + 3;
        let body_range = start..(start + size).min(data.len());
        let body = &data[body_range.clone()];
        at = start + size;
        match kind {
            PCS => set.pcs = read_pcs(body),
            PDS if body.len() >= 2 => {
                let entries = body[2..].as_chunks::<5>().0.to_vec();
                set.palettes.push((body[0], entries));
            }
            ODS if body.len() >= 4 => {
                let id = u16_at(body, 0) as u16;
                let flags = body[3];
                let (first, last) = (flags & 0x80 != 0, flags & 0x40 != 0);
                if first {
                    if body.len() >= 11 {
                        let (width, height) = (u16_at(body, 7), u16_at(body, 9));
                        let data = body_range.start + 11..body_range.end;
                        set.objects.push(ObjectPart { id, first, last, width, height, data });
                    }
                } else {
                    let data = body_range.start + 4..body_range.end;
                    set.objects.push(ObjectPart { id, first, last, width: 0, height: 0, data });
                }
            }
            _ => {}
        }
        if kind == END {
            break;
        }
    }
    set
}

fn read_pcs(b: &[u8]) -> Option<Composition> {
    if b.len() < 11 {
        return None;
    }
    let count = b[10];
    let mut objects = Vec::new();
    let mut at = 11;
    for _ in 0..count {
        if at + 8 > b.len() {
            break;
        }
        let id = u16_at(b, at) as u16;
        let cropped = b[at + 3] & 0x40 != 0;
        let (x, y) = (u16_at(b, at + 4), u16_at(b, at + 6));
        at += 8;
        let mut crop = None;
        if cropped && at + 8 <= b.len() {
            crop = Some(Crop { x: u16_at(b, at), y: u16_at(b, at + 2), w: u16_at(b, at + 4), h: u16_at(b, at + 6) });
            at += 8;
        }
        objects.push(PlacedObject { id, x, y, crop });
    }
    Some(Composition { width: u16_at(b, 0), height: u16_at(b, 2), state: b[7], palette_id: b[9], objects })
}

/// PGS run-length data as palette indexes, `width × height`.
pub fn decode_rle(rle: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0u8; width * height];
    let (mut x, mut y, mut i) = (0, 0, 0);
    let next = |i: &mut usize| {
        let b = rle.get(*i).copied().unwrap_or(0);
        *i += 1;
        b
    };
    while i < rle.len() && y < height {
        let b = next(&mut i);
        let (color, count) = if b != 0 {
            (b, 1)
        } else {
            let flags = next(&mut i);
            if flags == 0 {
                // End of line.
                x = 0;
                y += 1;
                continue;
            }
            let count = if flags & 0x40 != 0 {
                (usize::from(flags & 0x3F) << 8) | usize::from(next(&mut i))
            } else {
                usize::from(flags & 0x3F)
            };
            (if flags & 0x80 != 0 { next(&mut i) } else { 0 }, count)
        };
        let end = width.min(x + count);
        if color != 0 && x < end {
            out[y * width + x..y * width + end].fill(color);
        }
        x = end;
    }
    out
}

/// Decodes PGS display sets, from the epoch's definitions on.
#[derive(Default)]
pub struct PgsDecoder {
    sets: Vec<Option<DisplaySet>>,
    cache: FrameCache,
}

impl PgsDecoder {
    /// A decoder with nothing decoded yet.
    pub fn new() -> PgsDecoder {
        PgsDecoder::default()
    }

    fn parse(&mut self, samples: &[PictureSample], index: usize) {
        if self.sets.len() <= index {
            self.sets.resize_with(index + 1, || None);
        }
        if self.sets[index].is_none() {
            self.sets[index] = Some(parse_display_set(samples[index].data()));
        }
    }

    fn state(&mut self, samples: &[PictureSample], index: usize) -> u8 {
        self.parse(samples, index);
        self.sets[index].as_ref().and_then(|s| s.pcs.as_ref()).map_or(0, |p| p.state)
    }

    fn compose(&mut self, samples: &[PictureSample], index: usize) -> Option<PictureFrame> {
        self.parse(samples, index);
        // Palettes and objects as they stand at `index`: from the last epoch start or acquisition point.
        let mut from = index;
        while from > 0 && self.state(samples, from) & 0xC0 == 0 {
            from -= 1;
        }
        self.parse(samples, from);
        let pcs = self.sets[index].as_ref()?.pcs.as_ref()?;
        if pcs.objects.is_empty() {
            return Some(PictureFrame { plane_width: pcs.width, plane_height: pcs.height, bitmaps: Vec::new() });
        }
        let mut palettes: HashMap<u8, Vec<Option<PaletteEntry>>> = HashMap::new();
        let mut objects: HashMap<u16, (u32, u32, Vec<u8>)> = HashMap::new();
        let mut pending: HashMap<u16, (u32, u32, Vec<u8>)> = HashMap::new();
        for (i, set) in self.sets[from..=index].iter().enumerate() {
            let Some(set) = set else { continue };
            let data = samples[from + i].data();
            for (id, entries) in &set.palettes {
                let palette = palettes.entry(*id).or_insert_with(|| vec![None; 256]);
                for e in entries {
                    palette[usize::from(e[0])] = Some(*e);
                }
            }
            for o in &set.objects {
                if o.first {
                    pending.insert(o.id, (o.width, o.height, Vec::new()));
                }
                let Some(part) = pending.get_mut(&o.id) else { continue };
                part.2.extend_from_slice(&data[o.data.clone()]);
                if o.last
                    && let Some(whole) = pending.remove(&o.id)
                {
                    objects.insert(o.id, whole);
                }
            }
        }
        let palette = palettes.get(&pcs.palette_id)?;
        let hd = pcs.height > 576;
        let mut lut = [[0u8; 4]; 256];
        for e in palette.iter().flatten() {
            let [r, g, b] = ycbcr_to_rgb(e[1], e[3], e[2], hd);
            lut[usize::from(e[0])] = [r, g, b, e[4]];
        }
        let mut bitmaps = Vec::new();
        for p in &pcs.objects {
            let Some((width, height, rle)) = objects.get(&p.id) else { continue };
            let (width, height) = (*width, *height);
            if width == 0 || height == 0 || !fits(width as usize, height as usize) {
                continue;
            }
            let indexes = decode_rle(rle, width as usize, height as usize);
            let crop = p.crop.unwrap_or(Crop { x: 0, y: 0, w: width, h: height });
            let w = crop.w.min(width.saturating_sub(crop.x));
            let h = crop.h.min(height.saturating_sub(crop.y));
            if w == 0 || h == 0 {
                continue;
            }
            let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
            for row in 0..h {
                let src = ((crop.y + row) * width + crop.x) as usize;
                for &c in &indexes[src..src + w as usize] {
                    pixels.extend_from_slice(&lut[usize::from(c)]);
                }
            }
            bitmaps.push(placed(p.x, p.y, w, h, pixels));
        }
        Some(PictureFrame { plane_width: pcs.width, plane_height: pcs.height, bitmaps })
    }
}

impl PictureDecoder for PgsDecoder {
    fn decode(&mut self, samples: &[PictureSample], index: usize) -> Option<Arc<PictureFrame>> {
        let mut cache = std::mem::take(&mut self.cache);
        let frame = cache.frame(index, || self.compose(samples, index));
        self.cache = cache;
        frame
    }

    fn own_duration_ms(&mut self, _: &[PictureSample], _: usize) -> Option<u64> {
        None
    }

    fn forget_from(&mut self, index: usize) {
        self.sets.truncate(index);
        self.cache.forget_from(index);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::pgs_display_set;
    use super::*;

    fn sample(data: Vec<u8>) -> PictureSample {
        PictureSample::new(0, None, data)
    }

    #[test]
    fn decodes_runs_line_ends_and_long_runs() {
        // Line 1: 2 × colour 1, 2 × colour 0. Line 2: 4 × colour 2 (a short coloured run).
        let rle = [1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0];
        assert_eq!(decode_rle(&rle, 4, 2), [1, 1, 0, 0, 2, 2, 2, 2]);
        let long = decode_rle(&[0, 0xC1, 0x00, 7, 0, 0], 300, 1);
        assert!(!long.iter().all(|p| *p == 7));
        assert!(long[..256].iter().all(|p| *p == 7));
    }

    #[test]
    fn places_an_object_on_the_plane_in_its_palettes_colours() {
        let set = pgs_display_set(0x80, true, 100, 900, Some(&[1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0]), 4, 2);
        let frame = PgsDecoder::new().decode(&[sample(set)], 0).unwrap();
        assert_eq!((frame.plane_width, frame.plane_height), (1920, 1080));
        assert_eq!(frame.bitmaps.len(), 1);
        let b = &frame.bitmaps[0];
        assert_eq!((b.x, b.y, b.image.width, b.image.height), (100, 900, 4, 2));
        assert_eq!(b.image.pixels[..4], [255, 255, 255, 255]);
        assert_eq!(b.image.pixels[3 + 4 * 2], 0);
        assert_eq!(b.image.pixels[16..20], [0, 0, 0, 128]);
    }

    #[test]
    fn decodes_a_normal_case_display_set_from_its_epochs_definitions() {
        let start = pgs_display_set(0x80, true, 100, 900, Some(&[1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0]), 4, 2);
        let clear = pgs_display_set(0x00, false, 0, 0, None, 0, 0);
        let again = pgs_display_set(0x00, true, 200, 950, None, 0, 0);
        let samples = [sample(start), sample(clear), sample(again)];
        let mut decoder = PgsDecoder::new();
        assert!(decoder.decode(&samples, 1).unwrap().bitmaps.is_empty());
        let frame = decoder.decode(&samples, 2).unwrap();
        let reused = &frame.bitmaps[0];
        assert_eq!((reused.x, reused.y), (200, 950));
        assert_eq!(reused.image.pixels[3], 255);
    }

    #[test]
    fn crops_objects_joins_fragments_and_skips_the_sup_header() {
        // A 4×1 object of colours 1 1 2 2 in two ODS fragments, shown cropped to its right half.
        let mut pcs = vec![0x07, 0x80, 0x04, 0x38, 0x10, 0, 1, 0x80, 0, 0, 1];
        pcs.extend([0, 0, 0, 0x40, 0, 50, 0, 60, 0, 2, 0, 0, 0, 2, 0, 1]);
        let mut set = vec![0x50, 0x47, 0, 0, 0, 0, 0, 0, 0, 0];
        set.extend(super::super::tests::pgs_segment(PCS, &pcs));
        set.extend(super::super::tests::pgs_segment(PDS, &[0, 0, 1, 235, 128, 128, 255, 2, 81, 240, 90, 255]));
        let first = [0, 0, 0, 0x80, 0, 0, 8, 0, 4, 0, 1, 1, 1];
        set.extend(super::super::tests::pgs_segment(ODS, &first));
        set.extend(super::super::tests::pgs_segment(ODS, &[0, 0, 0, 0x40, 2, 2, 0, 0]));
        set.extend(super::super::tests::pgs_segment(END, &[]));
        let frame = PgsDecoder::new().decode(&[sample(set)], 0).unwrap();
        let b = &frame.bitmaps[0];
        assert_eq!((b.x, b.y, b.image.width, b.image.height), (50, 60, 2, 1));
        // Colour 2 is red; the plane is HD, so through BT.709.
        let [r, g, bl] = ycbcr_to_rgb(81, 90, 240, true);
        assert_eq!(b.image.pixels, [r, g, bl, 255, r, g, bl, 255]);
    }

    #[test]
    fn refuses_objects_larger_than_a_4k_picture() {
        let set = pgs_display_set(0x80, true, 0, 0, Some(&[0, 0]), 4000, 4000);
        assert!(PgsDecoder::new().decode(&[sample(set)], 0).unwrap().bitmaps.is_empty());
    }

    #[test]
    fn forgets_what_changed() {
        let set = pgs_display_set(0x80, true, 100, 900, Some(&[1, 1, 1, 1, 0, 0]), 4, 1);
        let mut decoder = PgsDecoder::new();
        let first = decoder.decode(&[sample(set.clone())], 0).unwrap();
        assert!(Arc::ptr_eq(&first, &decoder.decode(&[sample(set.clone())], 0).unwrap()));
        decoder.forget_from(0);
        assert!(!Arc::ptr_eq(&first, &decoder.decode(&[sample(set)], 0).unwrap()));
    }
}
