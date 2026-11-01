//! The smallest PNG writer: 8 bits a channel, RGB or RGBA, no interlacing,
//! every row filtered "Up" (the pictures here are mostly vertical gradients,
//! which that turns into runs of zeros).

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 13, 10, 26, 10];

/// A picture of `channels` (3 or 4) bytes a pixel, rows top to bottom.
pub(crate) fn encode(width: u32, height: u32, channels: usize, pixels: &[u8]) -> Vec<u8> {
    let stride = width as usize * channels;
    debug_assert_eq!(pixels.len(), stride * height as usize);
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    let zeros = vec![0u8; stride];
    for y in 0..height as usize {
        let row = &pixels[y * stride..(y + 1) * stride];
        let above = if y == 0 { &zeros[..] } else { &pixels[(y - 1) * stride..y * stride] };
        raw.push(2);
        raw.extend(row.iter().zip(above).map(|(p, a)| p.wrapping_sub(*a)));
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw).expect("writing to memory");
    let compressed = encoder.finish().expect("writing to memory");

    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // 8 bits a channel, colour type 2 (RGB) or 6 (RGBA), deflate, no filter method, no interlace.
    header.extend_from_slice(&[8, if channels == 4 { 6 } else { 2 }, 0, 0, 0]);

    let mut out = Vec::with_capacity(compressed.len() + 64);
    out.extend_from_slice(&SIGNATURE);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &compressed);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc::default();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// CRC-32 (ISO 3309), as PNG's chunks carry it.
struct Crc(u32);

impl Default for Crc {
    fn default() -> Crc {
        Crc(0xffff_ffff)
    }
}

impl Crc {
    fn update(&mut self, data: &[u8]) {
        for byte in data {
            let mut c = (self.0 ^ *byte as u32) & 0xff;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            self.0 = c ^ (self.0 >> 8);
        }
    }

    fn finish(&self) -> u32 {
        self.0 ^ 0xffff_ffff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn crc_matches_the_reference() {
        let mut crc = Crc::default();
        crc.update(b"123456789");
        assert_eq!(crc.finish(), 0xcbf4_3926);
    }

    #[test]
    fn rows_come_back_after_unfiltering() {
        let (w, h) = (3u32, 2u32);
        let pixels: Vec<u8> = (0..(w * h * 4) as u8).map(|v| v.wrapping_mul(37)).collect();
        let png = encode(w, h, 4, &pixels);
        assert_eq!(&png[..8], &SIGNATURE);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(png[25], 6);
        let length = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
        assert_eq!(&png[37..41], b"IDAT");
        let mut raw = Vec::new();
        flate2::read::ZlibDecoder::new(&png[41..41 + length]).read_to_end(&mut raw).unwrap();
        let stride = (w * 4) as usize;
        let mut previous = vec![0u8; stride];
        let mut back = Vec::new();
        for row in raw.chunks(stride + 1) {
            assert_eq!(row[0], 2);
            let line: Vec<u8> = row[1..].iter().zip(&previous).map(|(v, a)| v.wrapping_add(*a)).collect();
            back.extend_from_slice(&line);
            previous = line;
        }
        assert_eq!(back, pixels);
    }
}
