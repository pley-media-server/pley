//! A baseline JPEG encoder: JFIF, YCbCr with 4:2:0 chroma, the example
//! quantisation tables scaled by quality as libjpeg does, and the standard
//! Huffman tables (ITU-T T.81, Annex K). What the server answers for resized
//! pictures (JPEG, quality 82), drawn here without an image library.

use std::f32::consts::PI;

/// Natural (row-major) index of each zigzag position.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54,
    47, 55, 62, 63,
];

/// Annex K.1's tables, in natural order.
const LUMA_QUANT: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14, 17, 22, 29, 51,
    87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92, 49, 64, 78, 87, 103, 121, 120, 101,
    72, 92, 95, 98, 112, 100, 103, 99,
];
const CHROMA_QUANT: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99, 47, 66, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99,
];

/// Annex K.3's tables: code counts by length, then the symbols.
const DC_LUMA_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const DC_CHROMA_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const DC_VALUES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const AC_LUMA_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
const AC_LUMA_VALUES: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14,
    0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09,
    0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a,
    0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65,
    0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88,
    0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9,
    0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca,
    0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea,
    0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
];
const AC_CHROMA_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
const AC_CHROMA_VALUES: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32,
    0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16,
    0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39,
    0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64,
    0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86,
    0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
    0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8,
    0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9,
    0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
];

/// A Huffman table's code and length by symbol.
struct Table {
    codes: [(u16, u8); 256],
}

impl Table {
    fn new(bits: &[u8; 16], values: &[u8]) -> Table {
        let mut codes = [(0u16, 0u8); 256];
        let mut code = 0u16;
        let mut k = 0;
        for (length, count) in bits.iter().enumerate() {
            for _ in 0..*count {
                codes[values[k] as usize] = (code, length as u8 + 1);
                code += 1;
                k += 1;
            }
            code <<= 1;
        }
        Table { codes }
    }
}

/// Entropy-coded bits, with the 0xFF bytes stuffed.
struct Bits {
    out: Vec<u8>,
    acc: u32,
    count: u32,
}

impl Bits {
    fn put(&mut self, bits: u32, count: u32) {
        self.acc = (self.acc << count) | (bits & ((1 << count) - 1));
        self.count += count;
        while self.count >= 8 {
            let byte = (self.acc >> (self.count - 8)) as u8;
            self.out.push(byte);
            if byte == 0xff {
                self.out.push(0);
            }
            self.count -= 8;
        }
        self.acc &= (1 << self.count) - 1;
    }

    fn symbol(&mut self, table: &Table, symbol: u8) {
        let (code, length) = table.codes[symbol as usize];
        debug_assert!(length > 0, "symbol {symbol:#x} isn't in the table");
        self.put(code as u32, length as u32);
    }

    /// Pads the last byte with ones.
    fn finish(mut self) -> Vec<u8> {
        if self.count > 0 {
            let pad = 8 - self.count;
            self.put((1 << pad) - 1, pad);
        }
        self.out
    }
}

/// A coefficient's size category and its bits (negatives one less, as T.81 has it).
fn category(value: i32) -> (u32, u32) {
    let size = 32 - value.unsigned_abs().leading_zeros();
    let bits = if value < 0 { (value - 1) as u32 & ((1 << size) - 1) } else { value as u32 };
    (size, bits)
}

/// The quantisation table for `quality` (1 to 100), as libjpeg scales it.
fn scaled(base: &[u16; 64], quality: u8) -> [u16; 64] {
    let quality = quality.clamp(1, 100) as u32;
    let scale = if quality < 50 { 5_000 / quality } else { 200 - 2 * quality };
    base.map(|q| ((q as u32 * scale + 50) / 100).clamp(1, 255) as u16)
}

/// The 8-point DCT-II's basis, scaled so the 2-D transform is T.81's.
fn basis() -> [[f32; 8]; 8] {
    let mut c = [[0.0; 8]; 8];
    for (u, row) in c.iter_mut().enumerate() {
        let scale = if u == 0 { (1.0f32 / 8.0).sqrt() } else { 0.5 };
        for (x, v) in row.iter_mut().enumerate() {
            *v = scale * ((2 * x + 1) as f32 * u as f32 * PI / 16.0).cos();
        }
    }
    c
}

/// Transforms, quantises and codes one block of samples (level-shifted).
struct BlockCoder {
    basis: [[f32; 8]; 8],
}

impl BlockCoder {
    fn transform(&self, samples: &[f32; 64]) -> [f32; 64] {
        let c = &self.basis;
        let mut rows = [0.0f32; 64];
        for y in 0..8 {
            let row = &samples[y * 8..y * 8 + 8];
            // Flat rows (gradients, solid shapes) have their DC term only.
            if row.iter().all(|v| *v == row[0]) {
                rows[y * 8] = row[0] * 8.0 * c[0][0];
                continue;
            }
            for u in 0..8 {
                rows[y * 8 + u] = (0..8).map(|x| c[u][x] * row[x]).sum();
            }
        }
        let mut out = [0.0f32; 64];
        for u in 0..8 {
            if (0..8).all(|y| rows[y * 8 + u] == 0.0) {
                continue;
            }
            for v in 0..8 {
                out[v * 8 + u] = (0..8).map(|y| c[v][y] * rows[y * 8 + u]).sum();
            }
        }
        out
    }

    fn code(
        &self,
        bits: &mut Bits,
        samples: &[f32; 64],
        quant: &[u16; 64],
        previous_dc: &mut i32,
        dc: &Table,
        ac: &Table,
    ) {
        let coefficients = self.transform(samples);
        let mut zz = [0i32; 64];
        for (k, n) in ZIGZAG.iter().enumerate() {
            zz[k] = (coefficients[*n] / quant[*n] as f32).round() as i32;
        }
        zz[0] = zz[0].clamp(-2_047, 2_047);
        let diff = zz[0] - *previous_dc;
        *previous_dc = zz[0];
        let (size, value) = category(diff);
        bits.symbol(dc, size as u8);
        if size > 0 {
            bits.put(value, size);
        }
        let mut run = 0u32;
        for &coefficient in &zz[1..] {
            if coefficient == 0 {
                run += 1;
                continue;
            }
            while run > 15 {
                bits.symbol(ac, 0xf0);
                run -= 16;
            }
            let (size, value) = category(coefficient.clamp(-1_023, 1_023));
            bits.symbol(ac, ((run << 4) | size) as u8);
            bits.put(value, size);
            run = 0;
        }
        if run > 0 {
            bits.symbol(ac, 0x00);
        }
    }
}

fn segment(out: &mut Vec<u8>, marker: u8, payload: &[u8]) {
    out.extend_from_slice(&[0xff, marker]);
    out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(payload);
}

fn huffman_payload(class_and_id: u8, bits: &[u8; 16], values: &[u8]) -> Vec<u8> {
    let mut payload = vec![class_and_id];
    payload.extend_from_slice(bits);
    payload.extend_from_slice(values);
    payload
}

/// A pixel's Y, Cb and Cr (JFIF), level-shifted around 0.
fn ycbcr([r, g, b]: [u8; 3]) -> [f32; 3] {
    let (r, g, b) = (r as f32, g as f32, b as f32);
    [
        0.299 * r + 0.587 * g + 0.114 * b - 128.0,
        -0.168_736 * r - 0.331_264 * g + 0.5 * b,
        0.5 * r - 0.418_688 * g - 0.081_312 * b,
    ]
}

/// Encodes `pixels` (RGB or RGBA, `channels` bytes a pixel, rows top to
/// bottom; alpha is ignored) at `quality`.
pub(crate) fn encode(width: u32, height: u32, pixels: &[u8], channels: usize, quality: u8) -> Vec<u8> {
    assert!(width > 0 && height > 0 && width <= 65_535 && height <= 65_535 && channels >= 3);
    debug_assert_eq!(pixels.len(), width as usize * height as usize * channels);
    let luma_quant = scaled(&LUMA_QUANT, quality);
    let chroma_quant = scaled(&CHROMA_QUANT, quality);

    let mut out = Vec::with_capacity(pixels.len() / 12 + 1_024);
    out.extend_from_slice(&[0xff, 0xd8]);
    segment(&mut out, 0xe0, &[b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0]);
    let mut tables = vec![0u8];
    tables.extend(ZIGZAG.iter().map(|n| luma_quant[*n] as u8));
    tables.push(1);
    tables.extend(ZIGZAG.iter().map(|n| chroma_quant[*n] as u8));
    segment(&mut out, 0xdb, &tables);
    let (w, h) = (width as u16, height as u16);
    let mut frame = vec![8];
    frame.extend_from_slice(&h.to_be_bytes());
    frame.extend_from_slice(&w.to_be_bytes());
    frame.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    segment(&mut out, 0xc0, &frame);
    let mut huffman = huffman_payload(0x00, &DC_LUMA_BITS, &DC_VALUES);
    huffman.extend(huffman_payload(0x10, &AC_LUMA_BITS, &AC_LUMA_VALUES));
    huffman.extend(huffman_payload(0x01, &DC_CHROMA_BITS, &DC_VALUES));
    huffman.extend(huffman_payload(0x11, &AC_CHROMA_BITS, &AC_CHROMA_VALUES));
    segment(&mut out, 0xc4, &huffman);
    segment(&mut out, 0xda, &[3, 1, 0x00, 2, 0x11, 3, 0x11, 0, 63, 0]);

    let dc_luma = Table::new(&DC_LUMA_BITS, &DC_VALUES);
    let ac_luma = Table::new(&AC_LUMA_BITS, &AC_LUMA_VALUES);
    let dc_chroma = Table::new(&DC_CHROMA_BITS, &DC_VALUES);
    let ac_chroma = Table::new(&AC_CHROMA_BITS, &AC_CHROMA_VALUES);
    let coder = BlockCoder { basis: basis() };
    let mut bits = Bits { out: Vec::with_capacity(pixels.len() / 16), acc: 0, count: 0 };
    let mut dc = [0i32; 3];
    let (width, height) = (width as usize, height as usize);
    let mut y_plane = [0.0f32; 256];
    let mut cb = [0.0f32; 64];
    let mut cr = [0.0f32; 64];
    let mut reference = [0u8; 64];
    for my in (0..height).step_by(16) {
        for mx in (0..width).step_by(16) {
            // A flat area (most of a solid tile or shape) is converted once.
            let start = (my * width + mx) * channels;
            let first = [pixels[start], pixels[start + 1], pixels[start + 2]];
            let flat = mx + 16 <= width && my + 16 <= height && {
                for slot in reference[..16 * channels].chunks_exact_mut(channels) {
                    slot.copy_from_slice(&pixels[start..start + channels]);
                }
                (0..16).all(|dy| {
                    let row = ((my + dy) * width + mx) * channels;
                    pixels[row..row + 16 * channels] == reference[..16 * channels]
                })
            };
            if flat {
                let [y, b, r] = ycbcr(first);
                y_plane.fill(y);
                cb.fill(b);
                cr.fill(r);
            } else {
                cb.fill(0.0);
                cr.fill(0.0);
                for dy in 0..16 {
                    // Past the edges, the last row and column repeat.
                    let y = (my + dy).min(height - 1);
                    for dx in 0..16 {
                        let x = (mx + dx).min(width - 1);
                        let p = (y * width + x) * channels;
                        let [luma, blue, red] = ycbcr([pixels[p], pixels[p + 1], pixels[p + 2]]);
                        y_plane[dy * 16 + dx] = luma;
                        let c = (dy / 2) * 8 + dx / 2;
                        cb[c] += blue / 4.0;
                        cr[c] += red / 4.0;
                    }
                }
            }
            for (bx, by) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
                let mut block = [0.0f32; 64];
                for v in 0..8 {
                    for u in 0..8 {
                        block[v * 8 + u] = y_plane[(by + v) * 16 + bx + u];
                    }
                }
                coder.code(&mut bits, &block, &luma_quant, &mut dc[0], &dc_luma, &ac_luma);
            }
            coder.code(&mut bits, &cb, &chroma_quant, &mut dc[1], &dc_chroma, &ac_chroma);
            coder.code(&mut bits, &cr, &chroma_quant, &mut dc[2], &dc_chroma, &ac_chroma);
        }
    }
    out.extend(bits.finish());
    out.extend_from_slice(&[0xff, 0xd9]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ac_symbol_has_a_code() {
        for (bits, values) in [(&AC_LUMA_BITS, &AC_LUMA_VALUES), (&AC_CHROMA_BITS, &AC_CHROMA_VALUES)] {
            assert_eq!(bits.iter().map(|b| *b as usize).sum::<usize>(), values.len());
            let table = Table::new(bits, values);
            for run in 0..16u8 {
                for size in 1..=10u8 {
                    assert!(table.codes[((run << 4) | size) as usize].1 > 0);
                }
            }
            assert!(table.codes[0x00].1 > 0 && table.codes[0xf0].1 > 0);
        }
    }

    #[test]
    fn categories_follow_t81() {
        assert_eq!(category(0), (0, 0));
        assert_eq!(category(1), (1, 1));
        assert_eq!(category(-1), (1, 0));
        assert_eq!(category(-3), (2, 0));
        assert_eq!(category(5), (3, 5));
        assert_eq!(category(-1_023), (10, 0));
    }

    #[test]
    fn flat_blocks_keep_their_level() {
        let coder = BlockCoder { basis: basis() };
        let flat = [-28.0f32; 64];
        let out = coder.transform(&flat);
        assert!((out[0] - (-28.0 * 8.0)).abs() < 1e-3);
        assert!(out[1..].iter().all(|v| v.abs() < 1e-3));
    }
}
