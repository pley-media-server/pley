//! Pictures the kit makes itself on the CPU, at the screen's pixels: the loading ring's arc, which
//! the display list has no command for.

use std::f32::consts::{FRAC_PI_2, TAU};

use pley_base::{Color, RgbaImage};

/// A ring `size` pixels square: an optional full `track` circle with an arc of `sweep` degrees in
/// `color` over it, starting `start` degrees clockwise from the top, both `stroke` pixels wide with
/// round ends, anti-aliased by each pixel's distance from the stroke's middle.
pub(crate) fn ring(size: u32, stroke: f32, sweep: f32, start: f32, track: Option<Color>, color: Color) -> RgbaImage {
    let mut img = RgbaImage::new(size, size);
    let c = size as f32 / 2.0;
    let half = stroke / 2.0;
    let r = c - half;
    let a0 = start.to_radians() - FRAC_PI_2;
    let sweep = sweep.to_radians().clamp(0.0, TAU);
    let a1 = a0 + sweep;
    let end0 = (r * a0.cos(), r * a0.sin());
    let end1 = (r * a1.cos(), r * a1.sin());
    // Only the band the stroke can touch needs the angle worked out: it is drawn every frame
    // while the ring turns, on a TV's CPU.
    let (inner, outer) = ((r - half - 1.0).max(0.0), r + half + 1.0);
    let (inner2, outer2) = (inner * inner, outer * outer);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d2 = px * px + py * py;
            if d2 < inner2 || d2 > outer2 {
                continue;
            }
            let d = d2.sqrt();
            let off = (d - r).abs();
            let track_cov = (half + 0.5 - off).clamp(0.0, 1.0);
            let rel = (py.atan2(px) - a0).rem_euclid(TAU);
            let arc_off =
                if rel <= sweep { off } else { (px - end0.0).hypot(py - end0.1).min((px - end1.0).hypot(py - end1.1)) };
            let arc_cov = (half + 0.5 - arc_off).clamp(0.0, 1.0);
            let under = track.map_or((Color::TRANSPARENT, 0.0), |t| (t, track_cov * t.a as f32 / 255.0));
            let over = (color, arc_cov * color.a as f32 / 255.0);
            let a = over.1 + under.1 * (1.0 - over.1);
            if a <= 0.0 {
                continue;
            }
            let ch = |o: u8, u: u8| {
                ((o as f32 * over.1 + u as f32 * under.1 * (1.0 - over.1)) / a).round().clamp(0.0, 255.0) as u8
            };
            let i = (y as usize * size as usize + x as usize) * 4;
            img.pixels[i] = ch(over.0.r, under.0.r);
            img.pixels[i + 1] = ch(over.0.g, under.0.g);
            img.pixels[i + 2] = ch(over.0.b, under.0.b);
            img.pixels[i + 3] = (a * 255.0).round() as u8;
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
    }

    #[test]
    fn the_arc_starts_at_the_top_and_turns_clockwise() {
        let red = Color::rgb(0xff0000);
        let img = ring(80, 4.0, 90.0, 0.0, None, red);
        // On the stroke at the top and at the right (90° clockwise): inside the arc.
        assert_eq!(px(&img, 40, 2)[3], 255);
        assert!(px(&img, 70, 15)[3] > 200);
        // At the bottom and on the left: outside it.
        assert_eq!(px(&img, 40, 77)[3], 0);
        assert_eq!(px(&img, 2, 40)[3], 0);
        // The centre is clear.
        assert_eq!(px(&img, 40, 40)[3], 0);
    }

    #[test]
    fn the_track_shows_where_the_arc_is_not() {
        let img = ring(80, 4.0, 90.0, 0.0, Some(Color::WHITE), Color::rgb(0x0000ff));
        assert_eq!(px(&img, 40, 77), [255, 255, 255, 255]);
        assert_eq!(px(&img, 40, 2), [0, 0, 255, 255]);
    }
}
