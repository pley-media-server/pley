//! Posters Pley draws for collections: the first four films' posters in a
//! 2×2 grid (fewer laid out to fill it), and, for a collection given an icon, the grid fading into the
//! icon's color at the bottom with the icon there; or the icon alone on its
//! color. Drawn pictures are kept in
//! `cache/collections`, named by what they're drawn from, so a new film or
//! poster draws a new one.

use std::path::PathBuf;

use image::imageops::{self, FilterType};
use image::{DynamicImage, GenericImageView, Rgb, RgbImage, Rgba, RgbaImage};

use crate::auth::AuthUser;
use crate::collections::{self, Artwork};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;

/// The icons a collection's poster can carry: Lucide's, by their names there.
/// Their pictures are drawn by `scripts/render-collection-icons.mjs`.
pub const ICONS: &[&str] = &[
    "star",
    "clock",
    "calendar",
    "eye-off",
    "heart",
    "flame",
    "trophy",
    "award",
    "sparkles",
    "ghost",
    "rocket",
    "wand-sparkles",
    "drama",
    "face-grinning",
    "swords",
    "zap",
    "fingerprint",
    "mountain-snow",
    "globe",
    "landmark",
    "video",
    "dog",
    "baby",
    "users",
    "popcorn",
    "book-open",
    "snowflake",
    "sun",
    "leaf",
    "tree-pine",
    "clapperboard",
    "tv",
    "music",
    "book-headphones",
    "layers",
];

/// The color of a poster drawn for a collection with no posters to show
/// and no icon chosen: quiet, as nothing was chosen.
pub const EMPTY_COLOR: &str = "slate";

/// The icon a poster on its color alone starts with: its library's kind's,
/// as the apps show libraries.
pub fn default_icon(kind: crate::library::LibraryKind) -> &'static str {
    use crate::library::LibraryKind;
    match kind {
        LibraryKind::Movies => "clapperboard",
        LibraryKind::Shows => "tv",
        LibraryKind::Music => "music",
        LibraryKind::Audiobooks => "book-headphones",
        // A kind added later starts with Pley's icon for collections.
        #[allow(unreachable_patterns)]
        _ => "layers",
    }
}

/// The colors an icon can be drawn in, by name.
pub const COLORS: &[(&str, [u8; 3])] = &[
    ("red", [214, 69, 69]),
    ("orange", [222, 110, 48]),
    ("amber", [196, 136, 26]),
    ("green", [76, 150, 58]),
    ("teal", [29, 158, 117]),
    ("blue", [55, 138, 221]),
    ("violet", [118, 104, 221]),
    ("pink", [212, 83, 126]),
    ("slate", [125, 138, 158]),
];

/// An icon's color when none is chosen.
pub const DEFAULT_COLOR: &str = "blue";

pub fn is_icon(name: &str) -> bool {
    ICONS.contains(&name)
}

pub fn is_color(name: &str) -> bool {
    COLORS.iter().any(|(n, _)| *n == name)
}

fn color(name: &str) -> [u8; 3] {
    COLORS.iter().find(|(n, _)| *n == name).or_else(|| COLORS.iter().find(|(n, _)| *n == DEFAULT_COLOR)).unwrap().1
}

/// The picture of icon `name`: white on transparent, 256 px square.
fn icon_picture(name: &str) -> Option<&'static [u8]> {
    macro_rules! icons {
        ($($name:literal),* $(,)?) => {
            match name {
                $($name => Some(include_bytes!(concat!("../assets/collection-icons/", $name, ".png")).as_slice()),)*
                _ => None,
            }
        };
    }
    icons!(
        "star",
        "clock",
        "calendar",
        "eye-off",
        "heart",
        "flame",
        "trophy",
        "award",
        "sparkles",
        "ghost",
        "rocket",
        "wand-sparkles",
        "drama",
        "face-grinning",
        "swords",
        "zap",
        "fingerprint",
        "mountain-snow",
        "globe",
        "landmark",
        "video",
        "dog",
        "baby",
        "users",
        "popcorn",
        "book-open",
        "snowflake",
        "sun",
        "leaf",
        "tree-pine",
        "clapperboard",
        "tv",
        "music",
        "book-headphones",
        "layers",
    )
}

const WIDTH: u32 = 600;
const HEIGHT: u32 = 900;
const GAP: u32 = 4;
const BACKGROUND: [u8; 3] = [11, 13, 18];
const ICON_SIZE: u32 = 120;
/// The icon on its color alone, in the middle.
const ICON_SIZE_ALONE: u32 = 200;
/// The icon's space below it, over films' posters.
const ICON_BOTTOM: u32 = 45;

/// `picture` scaled to cover `width` × `height`, its middle kept. Cut to the
/// cell's shape first and scaled after, so a picture of an odd shape (a
/// poster 1 px wide) never grows past the cell.
fn cover(picture: &DynamicImage, width: u32, height: u32) -> RgbImage {
    let (w, h) = picture.dimensions();
    let (wide, tall) = (u64::from(w) * u64::from(height), u64::from(h) * u64::from(width));
    let (cw, ch) = if wide > tall {
        // Wider than the cell: its full height, the middle of its width.
        ((u64::from(h) * u64::from(width)).div_ceil(u64::from(height)) as u32, h)
    } else {
        (w, (u64::from(w) * u64::from(height)).div_ceil(u64::from(width)) as u32)
    };
    let (cw, ch) = (cw.clamp(1, w), ch.clamp(1, h));
    picture.crop_imm((w - cw) / 2, (h - ch) / 2, cw, ch).resize_exact(width, height, FilterType::Triangle).to_rgb8()
}

/// `a` towards `b` by `t` (0 to 1).
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8)
}

/// Where each of `count` posters goes (x, y, width, height), none twice:
/// one fills the picture; two side by side; three, one tall on the left and
/// two on the right; four in a 2×2 grid.
fn cells(count: usize) -> Vec<(u32, u32, u32, u32)> {
    let (w, h) = ((WIDTH - GAP) / 2, (HEIGHT - GAP) / 2);
    let (right, lower) = (w + GAP, h + GAP);
    match count {
        0 => Vec::new(),
        1 => vec![(0, 0, WIDTH, HEIGHT)],
        2 => vec![(0, 0, w, HEIGHT), (right, 0, w, HEIGHT)],
        3 => vec![(0, 0, w, HEIGHT), (right, 0, w, h), (right, lower, w, h)],
        _ => vec![(0, 0, w, h), (right, 0, w, h), (0, lower, w, h), (right, lower, w, h)],
    }
}

/// A collection's poster: `posters` (the first up to four of its films'),
/// laid out by how many there are ([`cells`]); with an icon (its name and its
/// color's), fading into the color at the bottom, the icon there.
pub fn draw(posters: &[DynamicImage], icon: Option<(&str, &str)>) -> RgbImage {
    let mut canvas = RgbImage::from_pixel(WIDTH, HEIGHT, Rgb(BACKGROUND));
    for (poster, (x, y, w, h)) in posters.iter().zip(cells(posters.len())) {
        imageops::replace(&mut canvas, &cover(poster, w, h), i64::from(x), i64::from(y));
    }
    let Some((name, color_name)) = icon else { return canvas };
    let base = color(color_name);
    if posters.is_empty() {
        // On its color alone: a diagonal, a little lighter at the top left
        // and darker at the bottom right (a CSS gradient at 150°).
        let (light, dark) = (mix(base, [255, 255, 255], 0.08), mix(base, [0, 0, 0], 0.55));
        let (dx, dy) = (0.5_f32, 0.866_f32);
        let half = WIDTH as f32 / 2.0 * dx + HEIGHT as f32 / 2.0 * dy;
        for (x, y, pixel) in canvas.enumerate_pixels_mut() {
            let along = (x as f32 - WIDTH as f32 / 2.0) * dx + (y as f32 - HEIGHT as f32 / 2.0) * dy;
            pixel.0 = mix(light, dark, (along / half + 1.0) / 2.0);
        }
    } else {
        // The band: the color, a little darkened, taking over from 58% of
        // the height and all but opaque from 90%.
        let band = mix(base, [0, 0, 0], 0.35);
        let (from, full) = (0.58, 0.90);
        for (_, y, pixel) in canvas.enumerate_pixels_mut() {
            let at = y as f32 / HEIGHT as f32;
            let t = if at <= from {
                0.0
            } else if at >= full {
                0.97
            } else {
                let p = (at - from) / (full - from);
                0.97 * p * p * (3.0 - 2.0 * p)
            };
            pixel.0 = mix(pixel.0, band, t);
        }
    }
    if let Some(picture) = icon_picture(name).and_then(|png| image::load_from_memory(png).ok()) {
        let size = if posters.is_empty() { ICON_SIZE_ALONE } else { ICON_SIZE };
        let glyph = picture.resize_exact(size, size, FilterType::Lanczos3).to_rgba8();
        let white = RgbaImage::from_fn(size, size, |x, y| {
            let Rgba([_, _, _, a]) = *glyph.get_pixel(x, y);
            Rgba([255, 255, 255, a])
        });
        let x = (WIDTH - size) / 2;
        let y = if posters.is_empty() { (HEIGHT - size) / 2 } else { HEIGHT - size - ICON_BOTTOM };
        let mut rgba = DynamicImage::ImageRgb8(canvas).to_rgba8();
        imageops::overlay(&mut rgba, &white, i64::from(x), i64::from(y));
        canvas = DynamicImage::ImageRgba8(rgba).to_rgb8();
    }
    canvas
}

/// What a collection's poster or backdrop is, for a user: a file to serve
/// (`drawn`: one Pley drew, which changes with its films without the
/// collection changing), or an item whose artwork to serve.
pub enum Shown {
    File { path: PathBuf, drawn: bool },
    Item(i64),
}

/// Collection `id`'s artwork of `kind` (`poster` or `backdrop`) for `user`,
/// drawn if it's drawn; `None`: no such collection, or not for them.
pub async fn shown(state: &AppState, user: &AuthUser, id: i64, kind: &str) -> ApiResult<Option<Shown>> {
    Ok(match collections::artwork(&state.db, &state.store, user, id, kind).await? {
        None => None,
        Some(Artwork::File(path)) => Some(Shown::File { path, drawn: false }),
        Some(Artwork::Item(item)) => Some(Shown::Item(item)),
        Some(Artwork::Drawn { items, icon }) => {
            // A poster that can't be found (a file gone) is left out, for the next one.
            let mut posters = Vec::with_capacity(collections::DRAWN_POSTERS);
            for &item in &items {
                if let Ok(path) = crate::api::items::resolve_artwork(state, item, "poster").await {
                    posters.push(path);
                }
                if posters.len() == collections::DRAWN_POSTERS {
                    break;
                }
            }
            match (icon, items.first()) {
                (None, Some(&first)) if posters.len() < collections::DRAWN_POSTERS_ALONE => Some(Shown::Item(first)),
                (icon, _) => Some(Shown::File { path: drawn(state, posters, icon).await?, drawn: true }),
            }
        }
    })
}

/// The picture drawn from `posters` (files) and `icon`, drawn now unless an
/// earlier one is in the cache.
async fn drawn(state: &AppState, posters: Vec<PathBuf>, icon: Option<(String, String)>) -> ApiResult<PathBuf> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    // The drawing's own version: a change to how posters are drawn draws them again.
    hash.update(b"collection poster 1\0");
    if let Some((name, color)) = &icon {
        hash.update(format!("{name}\0{color}\0"));
    }
    for path in &posters {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        hash.update(format!("{}\0{modified:?}\0", path.display()));
    }
    let name: String = hash.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect();
    let dir = state.config.cache_dir().join("collections");
    let path = dir.join(format!("{name}.jpg"));
    if path.is_file() {
        return Ok(path);
    }
    let _drawing = crate::images::UPLOAD_DECODES.acquire().await.map_err(|e| AppError::Internal(e.into()))?;
    let target = path.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let pictures: Vec<DynamicImage> = posters
            .iter()
            .filter_map(|p| std::fs::read(p).ok())
            .filter_map(|bytes| crate::images::decode(&bytes).ok())
            .collect();
        let icon = icon.as_ref().map(|(n, c)| (n.as_str(), c.as_str()));
        let picture = draw(&pictures, icon);
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88).encode_image(&picture)?;
        std::fs::create_dir_all(&dir)?;
        // Written aside and moved in place, so a request at the same time
        // never reads half a picture; aside under a name of its own, as two
        // requests may draw the same poster at once.
        static DRAWS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let draw = DRAWS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let partial = dir.join(format!("{name}.{}.{draw}.part", std::process::id()));
        std::fs::write(&partial, jpeg)?;
        std::fs::rename(&partial, &target)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .map_err(AppError::Internal)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_its_picture_and_the_lists_agree() {
        for name in ICONS {
            let png = icon_picture(name).unwrap_or_else(|| panic!("no picture for {name}"));
            let picture = image::load_from_memory(png).unwrap();
            assert_eq!(picture.dimensions(), (256, 256), "{name}");
        }
        assert!(icon_picture("not-an-icon").is_none());
        assert!(is_color(DEFAULT_COLOR));
        use crate::library::LibraryKind;
        for kind in [LibraryKind::Movies, LibraryKind::Shows, LibraryKind::Music, LibraryKind::Audiobooks] {
            assert!(is_icon(default_icon(kind)), "{kind:?}");
        }
        assert!(is_icon("layers"), "the icon for kinds to come");
    }

    #[test]
    fn a_picture_of_any_shape_covers_its_cell_without_growing_past_it() {
        // A sliver either way would be thousands of cells long scaled whole.
        for (w, h) in [(1, 12000), (12000, 1), (200, 300), (300, 200), (1, 1)] {
            let picture = DynamicImage::ImageRgb8(RgbImage::from_pixel(w, h, Rgb([10, 20, 30])));
            for (cw, ch) in [(298, 448), (WIDTH, HEIGHT)] {
                let covered = cover(&picture, cw, ch);
                assert_eq!(covered.dimensions(), (cw, ch), "{w}x{h} into {cw}x{ch}");
                assert_eq!(covered.get_pixel(cw / 2, ch / 2).0, [10, 20, 30]);
            }
        }
    }

    fn solid(rgb: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(200, 300, Rgb(rgb)))
    }

    #[test]
    fn the_grid_shows_four_posters_and_the_icon_fades_its_bottom_into_its_colour() {
        let posters = [solid([250, 0, 0]), solid([0, 250, 0]), solid([0, 0, 250]), solid([250, 250, 0])];
        let plain = draw(&posters, None);
        assert_eq!(plain.dimensions(), (WIDTH, HEIGHT));
        assert_eq!(plain.get_pixel(100, 100).0, [250, 0, 0]);
        assert_eq!(plain.get_pixel(500, 100).0, [0, 250, 0]);
        assert_eq!(plain.get_pixel(100, 800).0, [0, 0, 250]);
        assert_eq!(plain.get_pixel(500, 800).0, [250, 250, 0]);

        let marked = draw(&posters, Some(("star", "teal")));
        assert_eq!(marked.get_pixel(100, 100).0, [250, 0, 0], "the top half as it is");
        let band = mix(color("teal"), [0, 0, 0], 0.35);
        let bottom = marked.get_pixel(20, 890).0;
        assert!(bottom.iter().zip(band).all(|(a, b)| a.abs_diff(b) < 12), "{bottom:?} near {band:?}");
        // The icon, in white: some pixels in its square are white.
        let y0 = HEIGHT - ICON_SIZE - ICON_BOTTOM;
        let lit = (y0..y0 + ICON_SIZE)
            .flat_map(|y| (240..360).map(move |x| (x, y)))
            .filter(|(x, y)| marked.get_pixel(*x, *y).0.iter().all(|a| *a > 235))
            .count();
        assert!(lit > 300, "{lit} pixels of the icon");
    }

    #[test]
    fn fewer_posters_share_the_picture_and_none_leaves_the_colour() {
        let (red, green, blue) = (solid([250, 0, 0]), solid([0, 250, 0]), solid([0, 0, 250]));
        let one = draw(std::slice::from_ref(&red), None);
        assert_eq!((one.get_pixel(100, 100).0, one.get_pixel(500, 800).0), ([250, 0, 0], [250, 0, 0]));
        let two = draw(&[red.clone(), green.clone()], None);
        assert_eq!((two.get_pixel(100, 800).0, two.get_pixel(500, 100).0), ([250, 0, 0], [0, 250, 0]), "halves");
        let three = draw(&[red, green, blue], None);
        assert_eq!(three.get_pixel(100, 800).0, [250, 0, 0], "the first, tall on the left");
        assert_eq!((three.get_pixel(500, 100).0, three.get_pixel(500, 800).0), ([0, 250, 0], [0, 0, 250]));
        let none = draw(&[], Some(("ghost", "violet")));
        let (light, dark) = (mix(color("violet"), [255, 255, 255], 0.08), mix(color("violet"), [0, 0, 0], 0.55));
        let (top, bottom) = (none.get_pixel(0, 0).0, none.get_pixel(WIDTH - 1, HEIGHT - 1).0);
        assert!(top.iter().zip(light).all(|(a, b)| a.abs_diff(b) < 4), "{top:?} near {light:?}");
        assert!(bottom.iter().zip(dark).all(|(a, b)| a.abs_diff(b) < 4), "{bottom:?} near {dark:?}");
    }
}
