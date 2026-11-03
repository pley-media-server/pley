//! Screenshots: a frame read back from the surface, as a PNG file.

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::Path;

use pley_base::RgbaImage;

/// `image` encoded as PNG (straight RGBA, 8 bits a channel).
pub fn encode_png(image: &RgbaImage) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    write_png(&mut out, image)?;
    Ok(out)
}

/// Writes `image` to `path` as a PNG file.
pub fn save_png(path: &Path, image: &RgbaImage) -> io::Result<()> {
    write_png(BufWriter::new(File::create(path)?), image)
}

fn write_png(w: impl io::Write, image: &RgbaImage) -> io::Result<()> {
    let mut encoder = png::Encoder::new(w, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(io::Error::other)?;
    writer.write_image_data(&image.pixels).map_err(io::Error::other)?;
    writer.finish().map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trips() {
        let mut image = RgbaImage::new(3, 2);
        image.pixels[0..4].copy_from_slice(&[1, 2, 3, 4]);
        image.pixels[20..24].copy_from_slice(&[250, 251, 252, 253]);
        let bytes = encode_png(&image).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height, info.color_type), (3, 2, png::ColorType::Rgba));
        assert_eq!(buf, image.pixels);
    }
}
