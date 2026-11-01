//! Coverage masks for rounded corners, filled and outlined.
//!
//! A rounded rectangle is drawn as flat pieces plus four corner pieces whose
//! coverage comes from one of these masks in the atlas, so the fragment shader
//! never computes a distance: on the TV's Mali GPUs the frame is bound by
//! fragment arithmetic, and a texture read costs the
//! arithmetic pipe almost nothing.
//!
//! A mask covers one corner, the top-left one: texel (0, 0) is the rectangle's
//! outer corner. The other three corners sample it mirrored.

/// The extent, in device pixels, of a corner piece: the larger of the outer
/// radius and the outline's width (a thick outline's corner is wider than its
/// radius).
pub fn corner_extent(radius: f32, width: Option<f32>) -> f32 {
    match width {
        Some(w) => radius.max(w),
        None => radius,
    }
}

/// The coverage of a corner piece `extent` pixels square (plus one row and
/// column past it, so bilinear sampling at the piece's inner edges reads
/// coverage rather than the atlas's empty gutter).
///
/// `radius` is the outer corner's radius. With `width`, the shape is an
/// outline that wide, its inner corner concentric (radius `radius − width`,
/// square when that isn't positive); without, it is filled.
///
/// Returns the mask's side in texels and its coverage bytes, row by row.
pub fn corner(extent: f32, radius: f32, width: Option<f32>) -> (u32, Vec<u8>) {
    let side = extent.ceil() as u32 + 1;
    let mut out = Vec::with_capacity((side * side) as usize);
    for j in 0..side {
        for i in 0..side {
            let (x, y) = (i as f32 + 0.5, j as f32 + 0.5);
            let outside_outer = rounded_corner_distance(x, y, 0.0, radius);
            let mut cover = (0.5 - outside_outer).clamp(0.0, 1.0);
            if let Some(w) = width {
                let inner_radius = (radius - w).max(0.0);
                let outside_inner = rounded_corner_distance(x, y, w, inner_radius);
                cover *= (0.5 + outside_inner).clamp(0.0, 1.0);
            }
            out.push((cover * 255.0 + 0.5) as u8);
        }
    }
    (side, out)
}

/// Signed distance from (x, y) to a rounded rectangle's top-left corner region
/// whose edges are at `edge` on both axes and whose corner is rounded by
/// `radius`: negative inside, positive outside. The rectangle extends without
/// end to the right and down.
fn rounded_corner_distance(x: f32, y: f32, edge: f32, radius: f32) -> f32 {
    // Distances past the corner circle's centre toward the outside.
    let qx = edge + radius - x;
    let qy = edge + radius - y;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - radius
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(mask: &(u32, Vec<u8>), x: u32, y: u32) -> u8 {
        mask.1[(y * mask.0 + x) as usize]
    }

    #[test]
    fn filled_corner_is_clear_at_the_corner_and_solid_inside() {
        let m = corner(8.0, 8.0, None);
        assert_eq!(m.0, 9);
        assert_eq!(at(&m, 0, 0), 0);
        assert_eq!(at(&m, 8, 8), 255);
        // Along the edges, past the curve, the shape is full.
        assert_eq!(at(&m, 8, 0), 255);
        assert_eq!(at(&m, 0, 8), 255);
        // On the curve's diagonal the coverage ramps.
        let mid = at(&m, 2, 2);
        assert!(mid > 0 && mid < 255, "{mid}");
    }

    #[test]
    fn coverage_is_symmetric_about_the_diagonal() {
        let m = corner(12.5, 12.5, Some(3.0));
        for y in 0..m.0 {
            for x in 0..m.0 {
                assert_eq!(at(&m, x, y), at(&m, y, x));
            }
        }
    }

    #[test]
    fn outline_is_hollow() {
        // A 6 px ring with a 25 px outer radius: the band, then nothing inside.
        let m = corner(25.0, 25.0, Some(6.0));
        assert_eq!(at(&m, 25, 2), 255);
        assert_eq!(at(&m, 25, 8), 0);
        assert_eq!(at(&m, 25, 25), 0);
        assert_eq!(at(&m, 2, 25), 255);
    }

    #[test]
    fn thick_outline_on_a_small_radius_keeps_a_square_inner_corner() {
        let extent = corner_extent(2.0, Some(6.0));
        assert_eq!(extent, 6.0);
        let m = corner(extent, 2.0, Some(6.0));
        // Past the band on both axes is the inside of the outline: empty.
        assert_eq!(at(&m, 6, 6), 0);
        assert_eq!(at(&m, 3, 6), 255);
        // The 2 px outer rounding still cuts the corner pixel.
        assert!(at(&m, 0, 0) < 128);
    }
}
