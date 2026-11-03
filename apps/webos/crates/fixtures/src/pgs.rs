//! A Matroska file holding one PGS subtitle track, made here rather than
//! stored: what GET /api/files/{id}/subtitles/{index}.mks answers for
//! subtitles shown as pictures (a port of the Android fixtures' `PgsMks`).
//! Each cue is a display set drawing its text (a small pixel font) white with
//! a black edge on a dark box, centred low on a 1080p plane, and another
//! clearing it at its end. Times are the file's own, in ms.

/// The plane the pictures are placed on.
pub(crate) const WIDTH: usize = 1920;
pub(crate) const HEIGHT: usize = 1080;

const SCALE: usize = 5;
const PAD: usize = 14;
const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;
const BOTTOM_MARGIN: usize = 90;

/// The file for `cues` (start ms, end ms, text), `duration_ms` long.
pub(crate) fn build(cues: &[(i64, i64, String)], duration_ms: i64, language: Option<&str>) -> Vec<u8> {
    let mut clusters = Vec::new();
    for (n, (start, end, text)) in cues.iter().enumerate() {
        clusters.extend(cluster(*start, &show(n, text)));
        clusters.extend(cluster(*end, &clear(n)));
    }
    let mut info = uint(0x2AD7B1, 1_000_000);
    info.extend(float(0x4489, duration_ms as f64));
    info.extend(string(0x4D80, "pley fixtures"));
    info.extend(string(0x5741, "pley fixtures"));
    let info = element(0x1549A966, &info);
    let mut entry = uint(0xD7, 1);
    entry.extend(uint(0x73C5, 1));
    entry.extend(uint(0x83, 0x11));
    entry.extend(uint(0x9C, 0));
    entry.extend(string(0x86, "S_HDMV/PGS"));
    entry.extend(string(0x22B59C, language.unwrap_or("und")));
    let tracks = element(0x1654AE6B, &element(0xAE, &entry));
    let mut header = uint(0x4286, 1);
    for (id, value) in [(0x42F7, 1), (0x42F2, 4), (0x42F3, 8)] {
        header.extend(uint(id, value));
    }
    header.extend(string(0x4282, "matroska"));
    header.extend(uint(0x4287, 4));
    header.extend(uint(0x4285, 2));
    let mut segment = info;
    segment.extend(tracks);
    segment.extend(clusters);
    let mut out = element(0x1A45DFA3, &header);
    out.extend(element(0x18538067, &segment));
    out
}

fn show(n: usize, text: &str) -> Vec<u8> {
    let bitmap = render(text);
    let (w, h) = (bitmap[0].len(), bitmap.len());
    let x = (WIDTH - w.min(WIDTH)) / 2;
    let y = HEIGHT.saturating_sub(h + BOTTOM_MARGIN);
    let mut pcs = u16b(WIDTH);
    pcs.extend(u16b(HEIGHT));
    pcs.push(0x10);
    pcs.extend(u16b(n * 2));
    pcs.extend([0x80, 0, 0, 1]);
    pcs.extend(u16b(0));
    pcs.extend([0, 0]);
    pcs.extend(u16b(x));
    pcs.extend(u16b(y));
    let mut wds = vec![1, 0];
    for v in [x, y, w, h] {
        wds.extend(u16b(v));
    }
    // Y, Cr, Cb and alpha: 1 white, 2 black, 3 a dark box.
    let mut pds = vec![0, 0];
    for (id, luma, alpha) in [(1u8, 235u8, 255u8), (2, 16, 255), (3, 16, 176)] {
        pds.extend([id, luma, 128, 128, alpha]);
    }
    let rle = rle(&bitmap);
    let mut ods = u16b(0);
    ods.extend([0, 0xC0]);
    ods.extend(u24b(rle.len() + 4));
    ods.extend(u16b(w));
    ods.extend(u16b(h));
    ods.extend(rle);
    let mut out = pgs_segment(0x16, &pcs);
    out.extend(pgs_segment(0x17, &wds));
    out.extend(pgs_segment(0x14, &pds));
    out.extend(pgs_segment(0x15, &ods));
    out.extend(pgs_segment(0x80, &[]));
    out
}

/// A display set with no object: the screen clears.
fn clear(n: usize) -> Vec<u8> {
    let mut pcs = u16b(WIDTH);
    pcs.extend(u16b(HEIGHT));
    pcs.push(0x10);
    pcs.extend(u16b(n * 2 + 1));
    pcs.extend([0, 0, 0, 0]);
    let mut out = pgs_segment(0x16, &pcs);
    out.extend(pgs_segment(0x80, &[]));
    out
}

/// A PGS segment without the "PG" header, as Matroska keeps them.
fn pgs_segment(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![kind];
    out.extend(u16b(payload.len()));
    out.extend_from_slice(payload);
    out
}

/// PGS run lengths, line by line.
fn rle(bitmap: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in bitmap {
        let mut i = 0;
        while i < line.len() {
            let color = line[i];
            let mut run = 1;
            while i + run < line.len() && line[i + run] == color && run < 16_383 {
                run += 1;
            }
            match (color, run) {
                (c, r) if c != 0 && r < 3 => out.extend(std::iter::repeat_n(c, r)),
                (0, r) if r < 64 => out.extend([0, r as u8]),
                (0, r) => out.extend([0, 0x40 | (r >> 8) as u8, r as u8]),
                (c, r) if r < 64 => out.extend([0, 0x80 | r as u8, c]),
                (c, r) => out.extend([0, 0xC0 | (r >> 8) as u8, r as u8, c]),
            }
            i += run;
        }
        out.extend([0, 0]);
    }
    out
}

/// `text` in the pixel font, [`SCALE`] times, white with a black edge on the box.
fn render(text: &str) -> Vec<Vec<u8>> {
    let glyphs: Vec<&[&str; 7]> = text.to_uppercase().chars().map(glyph).collect();
    let cell = (GLYPH_W + 1) * SCALE;
    let w = (glyphs.len() * cell).saturating_sub(SCALE) + 2 * PAD;
    let h = GLYPH_H * SCALE + 2 * PAD;
    let mut bitmap = vec![vec![3u8; w]; h];
    for (g, rows) in glyphs.iter().enumerate() {
        for (row, bits) in rows.iter().enumerate() {
            for (col, bit) in bits.bytes().enumerate() {
                if bit != b'1' {
                    continue;
                }
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        bitmap[PAD + row * SCALE + dy][PAD + g * cell + col * SCALE + dx] = 1;
                    }
                }
            }
        }
    }
    // A black edge around the strokes, as subtitles have.
    let mut edged = bitmap.clone();
    for y in 0..h {
        for x in 0..w {
            if bitmap[y][x] != 3 {
                continue;
            }
            let near = (y.saturating_sub(2)..(y + 3).min(h))
                .any(|ny| (x.saturating_sub(2)..(x + 3).min(w)).any(|nx| bitmap[ny][nx] == 1));
            if near {
                edged[y][x] = 2;
            }
        }
    }
    edged
}

/// A 5 × 7 font: what the cues say ("PGS 12").
fn glyph(c: char) -> &'static [&'static str; 7] {
    match c {
        '0' => &["01110", "10001", "10011", "10101", "11001", "10001", "01110"],
        '1' => &["00100", "01100", "00100", "00100", "00100", "00100", "01110"],
        '2' => &["01110", "10001", "00001", "00010", "00100", "01000", "11111"],
        '3' => &["11111", "00010", "00100", "00010", "00001", "10001", "01110"],
        '4' => &["00010", "00110", "01010", "10010", "11111", "00010", "00010"],
        '5' => &["11111", "10000", "11110", "00001", "00001", "10001", "01110"],
        '6' => &["00110", "01000", "10000", "11110", "10001", "10001", "01110"],
        '7' => &["11111", "00001", "00010", "00100", "01000", "01000", "01000"],
        '8' => &["01110", "10001", "10001", "01110", "10001", "10001", "01110"],
        '9' => &["01110", "10001", "10001", "01111", "00001", "00010", "01100"],
        'P' => &["11110", "10001", "10001", "11110", "10000", "10000", "10000"],
        'G' => &["01110", "10001", "10000", "10111", "10001", "10001", "01111"],
        'S' => &["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
        'E' => &["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
        'N' => &["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
        'F' => &["11111", "10000", "10000", "11110", "10000", "10000", "10000"],
        'R' => &["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
        'U' => &["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
        'D' => &["11110", "10001", "10001", "10001", "10001", "10001", "11110"],
        '-' => &["00000", "00000", "00000", "11111", "00000", "00000", "00000"],
        _ => &["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
    }
}

fn cluster(time_ms: i64, block: &[u8]) -> Vec<u8> {
    // A SimpleBlock of track 1 at the cluster's time, a keyframe.
    let mut simple = vec![0x81, 0, 0, 0x80];
    simple.extend_from_slice(block);
    let mut payload = uint(0xE7, time_ms.max(0) as u64);
    payload.extend(element(0xA3, &simple));
    element(0x1F43B675, &payload)
}

pub(crate) fn element(id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = ebml_id(id);
    out.extend(ebml_size(payload.len() as u64));
    out.extend_from_slice(payload);
    out
}

pub(crate) fn uint(id: u32, value: u64) -> Vec<u8> {
    let mut bytes = 1;
    while bytes < 8 && value >> (8 * bytes) != 0 {
        bytes += 1;
    }
    let data: Vec<u8> = (0..bytes).map(|i| (value >> (8 * (bytes - 1 - i))) as u8).collect();
    element(id, &data)
}

pub(crate) fn float(id: u32, value: f64) -> Vec<u8> {
    element(id, &value.to_bits().to_be_bytes())
}

pub(crate) fn string(id: u32, value: &str) -> Vec<u8> {
    element(id, value.as_bytes())
}

fn ebml_id(id: u32) -> Vec<u8> {
    let mut bytes = 1;
    while bytes < 4 && id >> (8 * bytes) != 0 {
        bytes += 1;
    }
    (0..bytes).map(|i| (id >> (8 * (bytes - 1 - i))) as u8).collect()
}

/// An EBML size in the fewest bytes that hold it.
fn ebml_size(value: u64) -> Vec<u8> {
    let mut bytes = 1;
    while bytes < 8 && value >= (1u64 << (7 * bytes)) - 1 {
        bytes += 1;
    }
    let marked = value | (1u64 << (7 * bytes));
    (0..bytes).map(|i| (marked >> (8 * (bytes - 1 - i))) as u8).collect()
}

fn u16b(v: usize) -> Vec<u8> {
    (v as u16).to_be_bytes().to_vec()
}

fn u24b(v: usize) -> Vec<u8> {
    vec![(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_take_the_fewest_bytes() {
        assert_eq!(ebml_size(5), vec![0x85]);
        assert_eq!(ebml_size(126), vec![0xFE]);
        assert_eq!(ebml_size(127), vec![0x40, 127]);
        assert_eq!(ebml_id(0x1A45DFA3), vec![0x1A, 0x45, 0xDF, 0xA3]);
    }

    #[test]
    fn a_file_with_one_pgs_track() {
        let cues = vec![(500, 4_500, "PGS 1".to_string()), (6_500, 10_500, "PGS 2".to_string())];
        let file = build(&cues, 20_000, Some("fre"));
        assert_eq!(&file[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
        let has = |needle: &[u8]| file.windows(needle.len()).any(|w| w == needle);
        assert!(has(b"S_HDMV/PGS"));
        assert!(has(b"fre"));
        // Two clusters a cue: one showing it, one clearing it.
        assert_eq!(file.windows(4).filter(|w| *w == [0x1F, 0x43, 0xB6, 0x75]).count(), 4);
    }
}
