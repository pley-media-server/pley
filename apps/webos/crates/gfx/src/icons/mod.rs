//! Icons: Lucide's paths, rasterised at device pixels into alpha masks that the
//! renderer tints, as both TV apps draw them (round caps and joins, the stroke
//! of [`Icon::stroke_for`], a few glyphs filled as well).

mod paths;

use pley_base::Icon;
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

/// Every icon, for warming caches and for tests.
pub const ALL: &[Icon] = &[
    Icon::ArrowDownWideNarrow,
    Icon::ArrowLeft,
    Icon::ArrowRight,
    Icon::ArrowUpDown,
    Icon::ArrowUpNarrowWide,
    Icon::AudioLines,
    Icon::BookmarkCheck,
    Icon::BookmarkMinus,
    Icon::BookmarkPlus,
    Icon::Calendar,
    Icon::Captions,
    Icon::Check,
    Icon::ChevronDown,
    Icon::ChevronLeft,
    Icon::ChevronRight,
    Icon::ChevronUp,
    Icon::ChevronsRight,
    Icon::CircleAlert,
    Icon::CircleCheck,
    Icon::CircleX,
    Icon::Clapperboard,
    Icon::Clock,
    Icon::Delete,
    Icon::Disc3,
    Icon::Ellipsis,
    Icon::EyeOff,
    Icon::FastForward,
    Icon::Film,
    Icon::Funnel,
    Icon::Gauge,
    Icon::GripVertical,
    Icon::History,
    Icon::House,
    Icon::Info,
    Icon::Languages,
    Icon::Layers,
    Icon::List,
    Icon::ListMusic,
    Icon::ListOrdered,
    Icon::ListPlus,
    Icon::ListVideo,
    Icon::Lock,
    Icon::LogOut,
    Icon::Monitor,
    Icon::Moon,
    Icon::Music,
    Icon::Pause,
    Icon::Pencil,
    Icon::Play,
    Icon::Plus,
    Icon::Radio,
    Icon::RefreshCw,
    Icon::Repeat,
    Icon::Repeat1,
    Icon::Rewind,
    Icon::RotateCcw,
    Icon::RotateCw,
    Icon::Save,
    Icon::Search,
    Icon::Server,
    Icon::ServerOff,
    Icon::Settings,
    Icon::Shuffle,
    Icon::SkipBack,
    Icon::SkipForward,
    Icon::SlidersHorizontal,
    Icon::Star,
    Icon::StarOutline,
    Icon::Tag,
    Icon::Timer,
    Icon::Trash2,
    Icon::Tv,
    Icon::TvMinimal,
    Icon::UserRound,
    Icon::Users,
    Icon::WifiOff,
    Icon::X,
];

/// `icon` drawn `size_px` device pixels square with a stroke of `stroke` grid
/// units (of 24): one coverage byte per pixel, rows top to bottom. `None` only
/// for a zero size.
pub fn rasterize(icon: Icon, size_px: u32, stroke: f32) -> Option<Vec<u8>> {
    let mut pixmap = Pixmap::new(size_px, size_px)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    let line = Stroke { width: stroke, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    let transform = Transform::from_scale(size_px as f32 / 24.0, size_px as f32 / 24.0);
    for data in paths::paths(icon) {
        let Some(path) = parse(data) else { continue };
        if icon.filled() {
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
        pixmap.stroke_path(&path, &paint, &line, transform, None);
    }
    // White premultiplied by coverage: the alpha byte is the coverage.
    Some(pixmap.data().as_chunks::<4>().0.iter().map(|p| p[3]).collect())
}

/// Parses SVG path data (every command of SVG 1.1, arcs as cubic curves).
pub fn parse(data: &str) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut s = Scanner { bytes: data.as_bytes(), at: 0 };
    let mut cmd = 0u8;
    let (mut cx, mut cy) = (0f32, 0f32);
    let (mut sx, mut sy) = (0f32, 0f32);
    // The last control point, for the smooth curve commands.
    let mut last_ctrl: Option<(u8, f32, f32)> = None;
    loop {
        s.skip_separators();
        let Some(&b) = s.bytes.get(s.at) else { break };
        if b.is_ascii_alphabetic() {
            cmd = b;
            s.at += 1;
            if matches!(cmd, b'Z' | b'z') {
                pb.close();
                (cx, cy) = (sx, sy);
                last_ctrl = None;
                continue;
            }
        } else if cmd == 0 {
            return None;
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        let mut ctrl = None;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (s.number()? + ox, s.number()? + oy);
                pb.move_to(x, y);
                (cx, cy, sx, sy) = (x, y, x, y);
                // Further pairs after a move are lines.
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                (cx, cy) = (s.number()? + ox, s.number()? + oy);
                pb.line_to(cx, cy);
            }
            b'H' => {
                cx = s.number()? + ox;
                pb.line_to(cx, cy);
            }
            b'V' => {
                cy = s.number()? + oy;
                pb.line_to(cx, cy);
            }
            b'C' => {
                let (x1, y1) = (s.number()? + ox, s.number()? + oy);
                let (x2, y2) = (s.number()? + ox, s.number()? + oy);
                (cx, cy) = (s.number()? + ox, s.number()? + oy);
                pb.cubic_to(x1, y1, x2, y2, cx, cy);
                ctrl = Some((b'C', x2, y2));
            }
            b'S' => {
                let (x1, y1) = reflect(last_ctrl, b'C', cx, cy);
                let (x2, y2) = (s.number()? + ox, s.number()? + oy);
                (cx, cy) = (s.number()? + ox, s.number()? + oy);
                pb.cubic_to(x1, y1, x2, y2, cx, cy);
                ctrl = Some((b'C', x2, y2));
            }
            b'Q' => {
                let (x1, y1) = (s.number()? + ox, s.number()? + oy);
                (cx, cy) = (s.number()? + ox, s.number()? + oy);
                pb.quad_to(x1, y1, cx, cy);
                ctrl = Some((b'Q', x1, y1));
            }
            b'T' => {
                let (x1, y1) = reflect(last_ctrl, b'Q', cx, cy);
                (cx, cy) = (s.number()? + ox, s.number()? + oy);
                pb.quad_to(x1, y1, cx, cy);
                ctrl = Some((b'Q', x1, y1));
            }
            b'A' => {
                let (rx, ry, rotation) = (s.number()?, s.number()?, s.number()?);
                let (large, sweep) = (s.flag()?, s.flag()?);
                let (x, y) = (s.number()? + ox, s.number()? + oy);
                arc_to(&mut pb, (cx, cy), (rx, ry), rotation, large, sweep, (x, y));
                (cx, cy) = (x, y);
            }
            _ => return None,
        }
        last_ctrl = ctrl;
    }
    pb.finish()
}

/// The first control point of a smooth curve: the last one mirrored about the
/// current point, or the current point when the previous segment wasn't of
/// the same kind.
fn reflect(last: Option<(u8, f32, f32)>, kind: u8, cx: f32, cy: f32) -> (f32, f32) {
    match last {
        Some((k, x, y)) if k == kind => (2.0 * cx - x, 2.0 * cy - y),
        _ => (cx, cy),
    }
}

/// An elliptical arc as cubic curves of at most a quarter turn each, from the
/// endpoint form of SVG 1.1 (appendix F.6.5).
fn arc_to(
    pb: &mut PathBuilder,
    from: (f32, f32),
    radii: (f32, f32),
    rotation_deg: f32,
    large: bool,
    sweep: bool,
    to: (f32, f32),
) {
    let (x1, y1) = (from.0 as f64, from.1 as f64);
    let (x2, y2) = (to.0 as f64, to.1 as f64);
    let (mut rx, mut ry) = (radii.0.abs() as f64, radii.1.abs() as f64);
    if (x1 - x2).abs() < 1e-9 && (y1 - y2).abs() < 1e-9 {
        return;
    }
    if rx < 1e-9 || ry < 1e-9 {
        pb.line_to(to.0, to.1);
        return;
    }
    let phi = (rotation_deg as f64).to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    // Radii too small to reach the end point grow until they just do.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        if a.is_nan() { 0.0 } else { a }
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let segments = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / segments as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        let (ex, ey) = (rx * c, ry * s);
        (cos_phi * ex - sin_phi * ey + cx, sin_phi * ex + cos_phi * ey + cy)
    };
    let tangent = |t: f64| {
        let (s, c) = t.sin_cos();
        let (ex, ey) = (-rx * s, ry * c);
        (cos_phi * ex - sin_phi * ey, sin_phi * ex + cos_phi * ey)
    };
    let mut t = theta1;
    for i in 0..segments {
        let t2 = t + step;
        let (p0x, p0y) = point(t);
        let (d0x, d0y) = tangent(t);
        let (p3x, p3y) = if i + 1 == segments { (x2, y2) } else { point(t2) };
        let (d3x, d3y) = tangent(t2);
        pb.cubic_to(
            (p0x + k * d0x) as f32,
            (p0y + k * d0y) as f32,
            (p3x - k * d3x) as f32,
            (p3y - k * d3y) as f32,
            p3x as f32,
            p3y as f32,
        );
        t = t2;
    }
}

/// Reads numbers and flags out of path data, which may run them together
/// ("-1-1", ".5.5").
struct Scanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Scanner<'_> {
    fn skip_separators(&mut self) {
        while let Some(&b) = self.bytes.get(self.at) {
            if b.is_ascii_whitespace() || b == b',' {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn number(&mut self) -> Option<f32> {
        self.skip_separators();
        let start = self.at;
        let b = self.bytes;
        let mut i = self.at;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let mut seen_dot = false;
        let mut digits = 0;
        while let Some(&c) = b.get(i) {
            if c.is_ascii_digit() {
                digits += 1;
            } else if c == b'.' && !seen_dot {
                seen_dot = true;
            } else {
                break;
            }
            i += 1;
        }
        if digits == 0 {
            return None;
        }
        if matches!(b.get(i), Some(b'e' | b'E'))
            && b.get(i + 1).is_some_and(|c| c.is_ascii_digit() || *c == b'-' || *c == b'+')
        {
            i += 2;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        self.at = i;
        std::str::from_utf8(&b[start..i]).ok()?.parse().ok()
    }

    /// An arc flag: one digit, which may touch what follows ("a1 1 0 00 1 1").
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let flag = match self.bytes.get(self.at)? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.at += 1;
        Some(flag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_listed_once() {
        let mut seen = std::collections::HashSet::new();
        for icon in ALL {
            assert!(seen.insert(*icon), "{icon:?} twice");
        }
        assert_eq!(ALL.len(), 77);
    }

    #[test]
    fn every_path_parses() {
        for icon in ALL {
            for data in paths::paths(*icon) {
                assert!(parse(data).is_some(), "{icon:?}: {data}");
            }
        }
    }

    #[test]
    fn every_icon_rasterises_to_a_mask_with_ink() {
        for icon in ALL {
            for size in [18u32, 40, 96] {
                let mask = rasterize(*icon, size, Icon::stroke_for(size as f32 / 2.0)).unwrap();
                assert_eq!(mask.len(), (size * size) as usize);
                let ink = mask.iter().filter(|&&a| a > 0).count();
                assert!(ink > (size * size) as usize / 40, "{icon:?} at {size} has {ink} pixels of ink");
                assert!(ink < (size * size) as usize, "{icon:?} at {size} is solid");
            }
        }
    }

    #[test]
    fn filled_icons_cover_their_middle() {
        let play = rasterize(Icon::Play, 48, 2.0).unwrap();
        assert_eq!(play[24 * 48 + 20], 255);
        // The outline of the same star leaves its middle empty; the filled one doesn't.
        let star = rasterize(Icon::Star, 48, 2.0).unwrap();
        let outline = rasterize(Icon::StarOutline, 48, 2.0).unwrap();
        assert_eq!(star[26 * 48 + 24], 255);
        assert_eq!(outline[26 * 48 + 24], 0);
    }

    #[test]
    fn numbers_run_together() {
        let mut s = Scanner { bytes: b"-1-1.5.5e1,2", at: 0 };
        assert_eq!(s.number(), Some(-1.0));
        assert_eq!(s.number(), Some(-1.5));
        assert_eq!(s.number(), Some(5.0));
        assert_eq!(s.number(), Some(2.0));
        assert_eq!(s.number(), None);
    }

    #[test]
    fn a_circle_of_arcs_stays_on_its_circle() {
        let path = parse("M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0z").unwrap();
        let b = path.bounds();
        assert!((b.left() - 2.0).abs() < 0.01 && (b.right() - 22.0).abs() < 0.01);
        assert!((b.top() - 2.0).abs() < 0.05 && (b.bottom() - 22.0).abs() < 0.05);
    }
}
