//! The design system of the Android TV app, which every screen of Pley for LG TV follows: its
//! colors, type styles, spacing, radii, scrims and gradients, the focus treatment, the canvas's
//! fixed lines and the device tiers. Values come from the Android app's design system
//! (`apps/android/core/designsystem/.../PleyColors.kt`, `PleyType.kt`, `PleySpace.kt`,
//! `PleyShape.kt`) and its TV kit (`apps/android/ui/tv/...`), which the TV web app matched;
//! every number is in dp on the 960×540 canvas.
//!
//! The colors are the theme's (docs/THEMES.md): each role of the design is a token of the palette
//! shown ([`palette`]), read when it is drawn, so they are functions rather than constants. Only
//! what sits on pictures and video (a tile's hairline, a track over art, a QR code) keeps its own
//! black or white.

use pley_base::{Color, Family, GradientStop, TextStyle, Weight};

/// `c` with its alpha multiplied by `f`, rounded as Compose's `Color.copy(alpha)` does; usable in
/// constants, unlike [`Color::alpha`] (which gives the same value).
pub const fn alpha(c: Color, f: f32) -> Color {
    Color { r: c.r, g: c.g, b: c.b, a: (c.a as f32 * f + 0.5) as u8 }
}

// Colors (`PleyColors.kt`), each a token of the theme shown (docs/THEMES.md, [`palette`]): the
// Android design's roles, read from the palette the shell shows at the start of each frame.

pub use crate::palette::{Accent, Palette, Theme, palette, show};

/// The window, pages, scrims' base, the rail fade and the curtain: `bg`.
pub fn bg() -> Color {
    palette().bg
}
/// The open rail's panel (at 96 %) and its soft edge: `surface`.
pub fn surface() -> Color {
    palette().surface
}
/// Tile wells, skeletons, side panels, dialogs, text fields, library cards: `surface-2`.
pub fn surface2() -> Color {
    palette().surface2
}
/// Chips, the current tab, round buttons off art, toasts, the offline notice, the jump control:
/// `surface-3`.
pub fn surface3() -> Color {
    palette().surface3
}
/// Hairlines, progress tracks off art, empty rating stars: `border`.
pub fn surface4() -> Color {
    palette().border
}
/// The border of badges on the page; decoration only: `border-strong`.
pub fn outline() -> Color {
    palette().border_strong
}
/// The 1 dp edge of pills, round buttons and text fields at rest: `subtle`, the token nearest
/// the Android design's, which keeps a control's edge 3:1 against the page in every theme.
pub fn control() -> Color {
    palette().subtle
}
/// Primary text and icons: `fg`.
pub fn fg() -> Color {
    palette().fg
}
/// Secondary text, the metadata line, rail entries at rest, unselected tabs: `muted`.
pub fn muted() -> Color {
    palette().muted
}
/// Tertiary text, "·" separators, list numbers, fact labels: `subtle`.
pub fn subtle() -> Color {
    palette().subtle
}
/// Disabled text and icons: `subtle`, drawn at [`DISABLED_ALPHA`] where a control is disabled.
pub fn disabled() -> Color {
    palette().subtle
}
/// Focus fills, progress bars, checks, the unwatched count, the rail's current-entry bar:
/// `accent`.
pub fn accent() -> Color {
    palette().accent
}
/// The tile focus ring, a focused field's edge, pressed fills, text links, the cursor:
/// `accent-hover`.
pub fn accent_bright() -> Color {
    palette().accent_hover
}
/// Content on [`accent`] and [`accent_bright`]: `accent-fg`.
pub fn on_accent() -> Color {
    palette().accent_fg
}
/// The "New" program badge's background (Live TV): the accent at 18 %.
pub fn primary_container() -> Color {
    alpha(palette().accent, 0.18)
}
/// The logo's front poster: a gradient from `brand-light` to [`brand`].
pub fn brand_light() -> Color {
    palette().brand_light
}
/// The end of the logo's front poster's gradient: `brand`.
pub fn brand() -> Color {
    palette().brand
}
/// The logo's middle poster: `brand-deep`.
pub fn brand_deep() -> Color {
    palette().brand_deep
}
/// The logo's back poster: `brand-far`.
pub fn brand_far() -> Color {
    palette().brand_far
}
/// The logo's tile: `brand-tile`.
pub fn brand_tile() -> Color {
    palette().brand_tile
}
/// "Signed in" and "On this network" dots, a request's green: `success`.
pub fn success() -> Color {
    palette().success
}
/// Destructive fills and labels, the "Live" badge, error toasts and lines: `danger`.
pub fn danger() -> Color {
    palette().danger
}
/// Words on a [`danger`] fill: `danger-fg`.
pub fn danger_fg() -> Color {
    palette().danger_fg
}
/// What waits on someone, a request's amber: `warning`; words on it are [`bg`].
pub fn warning() -> Color {
    palette().warning
}
/// Informational notices: `info`.
pub fn info() -> Color {
    palette().info
}
/// Shadows and the dim behind panels and dialogs: `shade`, which has its own opacity in a light
/// theme.
pub fn shade() -> Color {
    palette().shade
}
/// QR codes' modules: black, as on paper, whatever the theme, so every phone reads them.
pub const QR_DARK: Color = Color::BLACK;
/// A QR code's background and quiet zone.
pub const QR_LIGHT: Color = Color::WHITE;
/// The watched tick's box, badges on art, round buttons over backdrops: Bg at 80 %.
pub fn on_art() -> Color {
    alpha(bg(), 0.8)
}
/// A progress bar's track over art: black at 60 %.
pub const PROGRESS_TRACK_ON_ART: Color = alpha(Color::BLACK, 0.6);
/// The 1 dp stroke inside every rectangular tile, over its picture: white at 6 %.
pub const TILE_HAIRLINE: Color = alpha(Color::WHITE, 0.06);
/// Behind side panels and dialogs: Shade at 60 %.
pub fn scrim() -> Color {
    shade().alpha(0.6)
}
/// Over the page while the rail is open: Shade at 50 % (`TvApp.kt`).
pub fn rail_scrim() -> Color {
    shade().alpha(0.5)
}
/// The open rail's panel: Surface at 96 % (`TvRail.kt`).
pub fn rail_panel() -> Color {
    alpha(surface(), 0.96)
}
/// A pill button's fill at rest: Fg at 12 % (`TvButtons.kt`).
pub fn pill_rest() -> Color {
    alpha(fg(), 0.12)
}
/// A text button's fill at rest: Fg at 8 % (`TvButtons.kt`).
pub fn text_button_rest() -> Color {
    alpha(fg(), 0.08)
}
/// Summaries: Fg at 85 %.
pub fn summary() -> Color {
    alpha(fg(), 0.85)
}
/// A placeholder's title on a tile, over its picture: white at 70 %, as on the web.
pub const PLACEHOLDER_TITLE: Color = alpha(Color::WHITE, 0.7);
/// The current track's line: Fg at 6 %.
pub fn track_current() -> Color {
    alpha(fg(), 0.06)
}
/// A highlighted track: Accent at 14 %.
pub fn track_highlight() -> Color {
    alpha(accent(), 0.14)
}
/// Secondary text on a focused (Accent) row: OnAccent at 80 %.
pub fn on_accent_secondary() -> Color {
    alpha(on_accent(), 0.8)
}
/// The onboarding screens' brand glow: BrandDeep at 18 %.
pub fn brand_glow() -> Color {
    alpha(brand_deep(), 0.18)
}
/// A library card's second line, as a fraction of its content color's alpha.
pub const CARD_SUBLINE_ALPHA: f32 = 0.75;
/// Icon wells, as a fraction of their content color's alpha.
pub const ICON_WELL_ALPHA: f32 = 0.12;
/// Disabled controls are drawn at this opacity, and are not focusable (`TvButtons.kt`).
pub const DISABLED_ALPHA: f32 = 0.4;

fn utf16_hash(s: &str) -> u32 {
    s.encode_utf16().fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32))
}

/// The placeholder's hue for a title, in degrees, as every app's (packages/placeholder): the hash, mod 360
/// (`designsystem/image/Placeholder.kt`).
pub fn placeholder_hue(seed: &str) -> u32 {
    utf16_hash(seed) % 360
}

/// The placeholder's two colors for a title: its 135° gradient runs from hsl(h, 30 %, 22 %) at
/// the top-left to hsl(h + 40, 35 %, 12 %) at the bottom-right. Drawn whenever a picture hasn't
/// finished loading or doesn't exist; backdrops use it without the title.
pub fn placeholder_colors(seed: &str) -> (Color, Color) {
    let h = placeholder_hue(seed);
    (hsl(h as f32, 0.30, 0.22), hsl(((h + 40) % 360) as f32, 0.35, 0.12))
}

/// CSS's hsl(), hue in degrees, saturation and lightness 0 to 1, as `Placeholder.hsl` computes it.
pub fn hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let hp = (hue % 360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp {
        h if h < 1.0 => (c, x, 0.0),
        h if h < 2.0 => (x, c, 0.0),
        h if h < 3.0 => (0.0, c, x),
        h if h < 4.0 => (0.0, x, c),
        h if h < 5.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    let ch = |v: f32| ((v + m) * 255.0 + 0.5) as u8;
    Color { r: ch(r), g: ch(g), b: ch(b), a: 0xff }
}

// Type (`PleyType.Tv`): Roboto at 400, 500 and 700 only, line height centered, no font padding.

const fn style(size: f32, line_height: f32, weight: Weight, tracking: f32) -> TextStyle {
    TextStyle { family: Family::Sans, size, line_height, weight, italic: false, tracking }
}

/// Hero titles, the item page's title, collection and playlist titles: 38/44, 700, −0.5.
pub const HERO_TITLE: TextStyle = style(38.0, 44.0, Weight::Bold, -0.5);
/// Page titles, Home's info-block title, notice titles, a person's name: 28/34, 700, −0.25.
pub const PAGE_TITLE: TextStyle = style(28.0, 34.0, Weight::Bold, -0.25);
/// Row headers and library-message titles: 18/24, 700.
pub const SECTION: TextStyle = style(18.0, 24.0, Weight::Bold, 0.0);
/// Episode and context lines, panel and dialog titles, placeholder titles: 16/22, 700.
pub const TITLE: TextStyle = style(16.0, 22.0, Weight::Bold, 0.0);
/// Hero summaries: 15/22, 400.
pub const BODY_LARGE: TextStyle = style(15.0, 22.0, Weight::Regular, 0.0);
/// Lists, panel rows, info-block summaries, message bodies: 14/20, 400.
pub const BODY: TextStyle = style(14.0, 20.0, Weight::Regular, 0.0);
/// Buttons, the metadata line, tabs, chips, rail labels, toasts: 14/18, 500, 0.1.
pub const LABEL: TextStyle = style(14.0, 18.0, Weight::Medium, 0.1);
/// Tile captions (700 when focused), labels under round buttons, counts: 13/17, 500, 0.2.
pub const CAPTION: TextStyle = style(13.0, 17.0, Weight::Medium, 0.2);
/// Quality and rating badges, the unwatched count, channel numbers: 11/13, 700, 0.4.
pub const BADGE: TextStyle = style(11.0, 13.0, Weight::Bold, 0.4);
/// The sign-in code: 48/56 monospace, 700, 4.
pub const CODE: TextStyle = TextStyle {
    family: Family::Mono,
    size: 48.0,
    line_height: 56.0,
    weight: Weight::Bold,
    italic: false,
    tracking: 4.0,
};
/// The PIN and address pads' digits: 28/28, 500.
pub const PIN_DIGIT: TextStyle = style(28.0, 28.0, Weight::Medium, 0.0);
/// The rail's profile name: Label with a 17 dp line (`TvRail.kt`).
pub const RAIL_PROFILE: TextStyle = style(14.0, 17.0, Weight::Medium, 0.1);
/// The rail's "Switch profile": Caption with a 15 dp line (`TvRail.kt`).
pub const RAIL_SWITCH: TextStyle = style(13.0, 15.0, Weight::Medium, 0.2);

/// The 4 dp spacing grid (`PleySpace.kt`); TV code mostly uses the canvas lines below.
pub mod space {
    /// 2 dp.
    pub const XXS: f32 = 2.0;
    /// 4 dp.
    pub const XS: f32 = 4.0;
    /// 8 dp.
    pub const S: f32 = 8.0;
    /// 12 dp.
    pub const M: f32 = 12.0;
    /// 16 dp.
    pub const L: f32 = 16.0;
    /// 20 dp.
    pub const XL: f32 = 20.0;
    /// 24 dp.
    pub const XXL: f32 = 24.0;
    /// 32 dp.
    pub const X3: f32 = 32.0;
    /// 40 dp.
    pub const X4: f32 = 40.0;
    /// 48 dp.
    pub const X5: f32 = 48.0;
    /// 64 dp.
    pub const X6: f32 = 64.0;
}

/// Corner radii (`PleyShape.kt`): corners are softened, never rounded off; nothing that holds a
/// label is a pill, and only faces and dots are circles.
pub mod radius {
    /// Badges, progress lines, skeleton text bars, the watched tick's box, the count pill.
    pub const BADGE: f32 = 2.0;
    /// Tiles, album art, playlist covers.
    pub const TILE: f32 = 4.0;
    /// Text fields, track lines, history rows.
    pub const FIELD: f32 = 4.0;
    /// Every button, chip, tab, rail entry, a panel row's focus fill, toasts, the jump control.
    pub const CONTROL: f32 = 6.0;
    /// Library cards, details and media-info cards, the "lifted" ring.
    pub const CARD: f32 = 6.0;
    /// Dialogs, and side panels' left corners.
    pub const DIALOG: f32 = 8.0;
    /// Bottom sheets (the phone's; listed for completeness).
    pub const SHEET: f32 = 12.0;

    /// A circle's radius for a square of `size`: avatars, people, status dots.
    pub fn circle(size: f32) -> f32 {
        size / 2.0
    }
}

// The canvas's fixed lines (`PleySpace.kt` `PleyTv`, `TvSafeArea`).

/// The canvas, which the renderer scales to the screen.
pub const CANVAS_W: f32 = 960.0;
/// The canvas's height.
pub const CANVAS_H: f32 = 540.0;
/// The overscan margins (5 %): text and focusables stay inside; backdrops and scrolling rows
/// bleed past.
pub const SAFE_X: f32 = 48.0;
/// The overscan margin at the top and bottom.
pub const SAFE_Y: f32 = 27.0;
/// The rail closed (8 + 56 entry + 8) and open (8 + 256 entry + 8).
pub const RAIL_COLLAPSED: f32 = 72.0;
/// The rail's width open.
pub const RAIL_EXPANDED: f32 = 272.0;
/// Every page's left edge; rows keep their focused tile here.
pub const GUTTER: f32 = 96.0;
/// The right edge of text over backdrops (the text column 96–560).
pub const TEXT_COLUMN_END: f32 = 560.0;
/// Where a page of rows puts the focused row's header.
pub const FOCUSED_ROW_HEADER_Y: f32 = 172.0;
/// The same line on library tabs and collections, on Search, and on a person's page.
pub const HEADER_Y_LIBRARY: f32 = 80.0;
/// The focused row's header line on Search.
pub const HEADER_Y_SEARCH: f32 = 96.0;
/// The focused row's header line on a person's page.
pub const HEADER_Y_PERSON: f32 = 240.0;
/// Rows above the focused header are clipped this far above it once the page has scrolled.
pub const ROWS_CLIP_ABOVE: f32 = 12.0;
/// One line of [`SECTION`]: a row's header.
pub const ROW_HEADER_HEIGHT: f32 = 24.0;
/// Header to tiles, leaving room for the focused tile's growth and ring.
pub const HEADER_TO_TILES: f32 = 14.0;
/// Tiles to their caption.
pub const TILES_TO_CAPTION: f32 = 14.0;
/// Two caption lines of 17.
pub const CAPTION_HEIGHT: f32 = 34.0;
/// One row's caption to the next row's header.
pub const ROW_GAP: f32 = 28.0;
/// The focused row's tiles' top: 172 + 24 + 14 (`TvItemScreen.kt`).
pub const FOCUSED_ROW_TILES_Y: f32 = FOCUSED_ROW_HEADER_Y + ROW_HEADER_HEIGHT + HEADER_TO_TILES;
/// The room left under a page of rows so its last row can reach the focused line:
/// 540 − 172 − (24 + 14 + 115 + 14 + 34) (`TvRow.kt`).
pub const LAST_ROW_ROOM: f32 = 167.0;
/// Between posters, squares and people.
pub const TILE_GAP: f32 = 14.0;
/// Between landscape and episode tiles.
pub const WIDE_TILE_GAP: f32 = 18.0;
/// A row's end padding after its last tile.
pub const ROW_END_PADDING: f32 = 48.0;
/// A row whose tiles all end before this x never scrolls.
pub const ROW_FITS_BEFORE: f32 = 912.0;
/// Rows draw this far above and below their tiles so the focus look isn't clipped.
pub const ROW_BLEED: f32 = 16.0;
/// A poster grid: between columns, from a caption to the next row's wells, and from the grid's
/// top to the focused row's wells.
pub const GRID_COLUMN_GAP: f32 = 14.0;
/// A grid's caption to the next row's wells.
pub const GRID_ROW_GAP: f32 = 20.0;
/// A grid's top to its focused row's wells.
pub const GRID_CONTENT_TOP: f32 = 14.0;
/// Lists and columns keep the focused element's top at this fraction of their height, unless
/// that would run past an end (`TvScrolling.kt`).
pub const SCROLL_ANCHOR: f32 = 0.3;
/// Between buttons in a row, and the room under round buttons for their label.
pub const BUTTON_GAP: f32 = 10.0;
/// The room under round buttons for their label.
pub const BUTTON_LABEL_ROOM: f32 = 24.0;
/// Pills, round buttons, chips and tabs.
pub const BUTTON_HEIGHT: f32 = 40.0;
/// The least height of a list or panel row.
pub const LIST_ROW_HEIGHT: f32 = 44.0;
/// Icons in buttons, rows and the rail.
pub const ICON_SIZE: f32 = 20.0;
/// The page's fade under the rail: Bg solid to x 56, clear from x 72 ([`RAIL_FADE`]).
pub const RAIL_FADE_SOLID_UNTIL: f32 = 56.0;
/// Where the page's fade under the rail is clear.
pub const RAIL_FADE_CLEAR_FROM: f32 = 72.0;
/// The open rail's soft edge: x 272 to 336 ([`RAIL_PANEL_EDGE`]).
pub const RAIL_EDGE_END: f32 = 336.0;
/// Reading text that can't take focus moves on in steps of this much (`TvReadOn.kt`).
pub const READ_ON_STEP: f32 = 160.0;

/// The focus treatment (`TvTile.kt`, `TvButtons.kt`, `PleyShape.kt`): fills, edges and scales;
/// no glow anywhere.
pub mod focus {
    use super::*;

    /// A focused tile's art well grows to this, about its center; the caption stays outside.
    pub const TILE_SCALE: f32 = 1.08;
    /// A tile while OK is held on it: 1.04 in all (the multiplier is 1.04 / 1.08).
    pub const TILE_PRESSED_SCALE: f32 = 1.04;
    /// The tile ring: 3 dp of AccentBright centered 4.5 dp outside the scaled edge, its radius the
    /// tile's plus 4.5, reaching 6 dp past the edge; its alpha follows the focus progress.
    pub const RING_STROKE: f32 = 3.0;
    /// How far outside the scaled edge the ring's center line runs.
    pub const RING_OFFSET: f32 = 4.5;
    /// How far past the edge the ring reaches.
    pub const RING_REACH: f32 = 6.0;
    /// The ring's color.
    pub fn ring_color() -> Color {
        accent_bright()
    }
    /// A text field's focused edge: 2 dp of AccentBright, no scale; 1 dp of Control at rest.
    pub const FIELD_EDGE: f32 = 2.0;
    /// A focused text field's edge color.
    pub fn field_edge_color() -> Color {
        accent_bright()
    }
    /// The 1 dp edge of pills, round buttons and fields at rest.
    pub const REST_EDGE: f32 = 1.0;

    /// How a control looks at rest, focused and pressed.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct ControlLook {
        pub fill: Color,
        pub content: Color,
        /// A 1 dp [`control`] edge at rest, if it has one.
        pub edge: Option<Color>,
        pub focus_fill: Color,
        pub focus_content: Color,
        /// The fill while OK is held on it.
        pub pressed_fill: Color,
        /// Its scale when focused (1 for none).
        pub focus_scale: f32,
    }

    fn look(fill: Color, content: Color, edge: bool, pressed: Color, scale: f32) -> ControlLook {
        ControlLook {
            fill,
            content,
            edge: edge.then(control),
            focus_fill: accent(),
            focus_content: on_accent(),
            pressed_fill: pressed,
            focus_scale: scale,
        }
    }

    /// Pill buttons.
    pub fn pill() -> ControlLook {
        look(pill_rest(), fg(), true, accent_bright(), 1.05)
    }
    /// "Round" buttons (40 dp squares, radius 6) off art; their label fades in under them.
    pub fn round() -> ControlLook {
        look(surface3(), fg(), true, accent_bright(), 1.08)
    }
    /// "Round" buttons over art and backdrops.
    pub fn round_on_art() -> ControlLook {
        look(on_art(), fg(), true, accent_bright(), 1.08)
    }
    /// Text buttons.
    pub fn text_button() -> ControlLook {
        look(text_button_rest(), fg(), false, accent_bright(), 1.05)
    }
    /// Text buttons drawn as links.
    pub fn link() -> ControlLook {
        look(Color::TRANSPARENT, fg(), false, accent_bright(), 1.05)
    }
    /// Chips (filters, choices).
    pub fn chip() -> ControlLook {
        look(surface3(), fg(), false, accent_bright(), 1.05)
    }
    /// Tabs (season tabs too); the current one is [`tab_current`].
    pub fn tab() -> ControlLook {
        look(Color::TRANSPARENT, muted(), false, accent_bright(), 1.05)
    }
    /// The current tab.
    pub fn tab_current() -> ControlLook {
        look(surface3(), fg(), false, accent_bright(), 1.05)
    }
    /// Rail entries; the current one shows Fg content (and a bar closed, a Surface3 pill open).
    pub fn rail_entry() -> ControlLook {
        look(Color::TRANSPARENT, muted(), false, accent_bright(), 1.05)
    }
    /// The rail's current entry.
    pub fn rail_entry_current() -> ControlLook {
        look(Color::TRANSPARENT, fg(), false, accent_bright(), 1.05)
    }
    /// Panel and list rows: an Accent fill when focused, never scaled (the panel would clip it).
    pub fn list_row() -> ControlLook {
        look(Color::TRANSPARENT, fg(), false, accent(), 1.0)
    }
    /// Library cards (the libraries list).
    pub fn library_card() -> ControlLook {
        look(surface2(), fg(), false, accent_bright(), 1.05)
    }
    /// The library's jump control.
    pub fn jump() -> ControlLook {
        look(surface3(), fg(), false, accent(), 1.08)
    }
}

/// A gradient along one axis as the design gives it: colors at positions in dp along that axis,
/// piecewise linear between them, the end colors held beyond. It holds up to four.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    points: [(f32, Color); 4],
    len: usize,
}

impl Ramp {
    /// A ramp through `points` (the first four), in order along the axis.
    pub fn new(points: &[(f32, Color)]) -> Ramp {
        let mut out = [(0.0, Color::TRANSPARENT); 4];
        let len = points.len().min(4);
        out[..len].copy_from_slice(&points[..len]);
        Ramp { points: out, len }
    }

    fn points(&self) -> &[(f32, Color)] {
        &self.points[..self.len]
    }

    /// The ramp's stops for a rectangle spanning `start` to `start + len` along its axis, as the
    /// display list wants them (0 to 1 across the rectangle), cut at the rectangle's ends.
    pub fn stops(&self, start: f32, len: f32) -> Vec<GradientStop> {
        let pts = self.points();
        let mut out = Vec::with_capacity(pts.len() + 2);
        if pts.is_empty() || len <= 0.0 {
            return out;
        }
        let end = start + len;
        out.push(GradientStop::new(0.0, self.at(start)));
        for &(p, c) in pts {
            if p > start && p < end {
                out.push(GradientStop::new((p - start) / len, c));
            }
        }
        out.push(GradientStop::new(1.0, self.at(end)));
        out
    }

    /// The color at position `p`.
    pub fn at(&self, p: f32) -> Color {
        let pts = self.points();
        let Some(&(first, c0)) = pts.first() else { return Color::TRANSPARENT };
        if p <= first {
            return c0;
        }
        for w in pts.windows(2) {
            let ((a, ca), (b, cb)) = (w[0], w[1]);
            if p <= b {
                return if b > a { ca.lerp(cb, (p - a) / (b - a)) } else { cb };
            }
        }
        pts[pts.len() - 1].1
    }
}

// Scrims and gradients (`TvSurfaces.kt`, `TvRow.kt`, `TvRail.kt`). They are drawn as vertex
// gradients, never per-pixel work.

/// A backdrop's left scrim along x: Bg solid at 0, 92 % at 300, 85 % at 560, clear at 800.
pub fn backdrop_left_scrim() -> Ramp {
    let bg = bg();
    Ramp::new(&[(0.0, bg), (300.0, alpha(bg, 0.92)), (560.0, alpha(bg, 0.85)), (800.0, alpha(bg, 0.0))])
}
/// A backdrop's bottom scrim along y under a hero (clear at 220, Bg from 410 down).
pub fn bottom_scrim_hero() -> Ramp {
    Ramp::new(&[(220.0, alpha(bg(), 0.0)), (410.0, bg())])
}
/// The same under rows (120 → 300).
pub fn bottom_scrim_rows() -> Ramp {
    Ramp::new(&[(120.0, alpha(bg(), 0.0)), (300.0, bg())])
}
/// The same on a title's page (260 → 500).
pub fn bottom_scrim_item() -> Ramp {
    Ramp::new(&[(260.0, alpha(bg(), 0.0)), (500.0, bg())])
}
/// The page's fade under the closed rail, along x, drawn by the page over its content.
pub fn rail_fade() -> Ramp {
    Ramp::new(&[(0.0, bg()), (RAIL_FADE_SOLID_UNTIL, bg()), (RAIL_FADE_CLEAR_FROM, alpha(bg(), 0.0))])
}
/// The open rail's soft edge along x, after its [`rail_panel`] fill.
pub fn rail_panel_edge() -> Ramp {
    Ramp::new(&[(RAIL_EXPANDED, alpha(surface(), 0.85)), (RAIL_EDGE_END, alpha(surface(), 0.0))])
}
/// An album page's faint art (drawn at [`FAINT_ART_ALPHA`]) fades into Bg down the screen: clear
/// at 40 % of the height, Bg at its bottom.
pub fn faint_art_fade() -> Ramp {
    Ramp::new(&[(CANVAS_H * 0.4, alpha(bg(), 0.0)), (CANVAS_H, bg())])
}
/// The album page's faint art is drawn at this opacity.
pub const FAINT_ART_ALPHA: f32 = 0.14;
/// Collection and playlist pages dim their hero with Bg up to this alpha as the content scrolls
/// over it (scrolled distance / hero height).
pub const DIM_MAX_ALPHA: f32 = 0.92;
/// An empty playlist's cover: a linear gradient from Surface3 to Surface2.
pub fn empty_cover() -> (Color, Color) {
    (surface3(), surface2())
}
/// Backdrops are cropped toward their subject: Android's `BiasAlignment(0.4, −0.6)`, right of
/// center and above the middle. Never blurred.
pub const BACKDROP_BIAS: (f32, f32) = (0.4, -0.6);

pub use pley_base::Tier;

/// What a tier changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TierRules {
    /// Pictures not in memory fade in; on Low they appear at once.
    pub image_fade: bool,
    /// Backdrops cross-fade; on Low they swap at once.
    pub backdrop_crossfade: bool,
    /// Only the focused tile animates on Low: losing focus snaps.
    pub unfocus_animates: bool,
    /// The widest backdrop asked for, in pixels.
    pub backdrop_max_px: u32,
    /// The widest picture asked for, in pixels.
    pub image_max_px: u32,
    /// Texels kept on the GPU (the picture cache's budget), in bytes.
    pub texture_budget: u64,
    /// Screens of pictures prefetched ahead of a row or grid, and how many of those are decoded
    /// (the rest only downloaded to disk).
    pub prefetch_screens: f32,
    pub prefetch_decoded_screens: f32,
    /// Library listings are windowed above this many items.
    pub library_window_above: u32,
}

/// What a [`Tier`] changes in the interface.
pub trait TierExt {
    fn rules(self) -> TierRules;
}

impl TierExt for Tier {
    /// What this tier changes.
    fn rules(self) -> TierRules {
        const MIB: u64 = 1024 * 1024;
        match self {
            Tier::Low => TierRules {
                image_fade: false,
                backdrop_crossfade: false,
                unfocus_animates: false,
                backdrop_max_px: 960,
                image_max_px: 1280,
                texture_budget: 64 * MIB,
                prefetch_screens: 1.5,
                prefetch_decoded_screens: 1.0,
                library_window_above: 2000,
            },
            Tier::Mid | Tier::High => TierRules {
                image_fade: true,
                backdrop_crossfade: true,
                unfocus_animates: true,
                backdrop_max_px: 1280,
                image_max_px: 1920,
                texture_budget: if matches!(self, Tier::High) { 192 * MIB } else { 128 * MIB },
                prefetch_screens: 1.5,
                prefetch_decoded_screens: 1.5,
                library_window_above: 5000,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Midnight, shown until a theme is picked, is the Android design's.
    #[test]
    fn colours_match_the_design() {
        assert_eq!(bg(), Color::rgb(0x0b0d10));
        assert_eq!(surface2(), Color::rgb(0x191d24));
        assert_eq!(fg(), Color::rgb(0xe9ebef));
        assert_eq!(accent(), Color::rgb(0x38bdf8));
        assert_eq!(accent_bright(), Color::rgb(0x7dd3fc));
        assert_eq!(on_accent(), Color::rgb(0x03131f));
        assert_eq!(danger(), Color::rgb(0xed6c6d));
        assert_eq!(disabled(), control());
    }

    /// Every color follows the palette shown, from the moment it is shown.
    #[test]
    fn colours_follow_the_palette_shown() {
        show(Palette::of(Theme::Midday, Some(Accent::Amber)));
        let p = palette();
        assert_eq!((bg(), fg(), accent(), on_accent()), (p.bg, p.fg, p.accent, p.accent_fg));
        assert_eq!(accent(), Color::rgb(0xa03d00));
        assert_eq!(focus::pill().focus_fill, accent());
        assert_eq!(backdrop_left_scrim().at(0.0), Color::rgb(0xf3f5f8));
        // A profile's avatar keeps its own colors in a light theme: dark on a bright circle,
        // white on a deep one, as in Midnight.
        assert_eq!(crate::avatar::ink(Color::rgb(0xfacc15)), crate::avatar::INK_DARK);
        assert_eq!(crate::avatar::ink(Color::rgb(0x7c3aed)), crate::avatar::INK_LIGHT);
        show(Palette::of(Theme::Midnight, None));
        assert_eq!(accent(), Color::rgb(0x38bdf8));
    }

    #[test]
    fn alpha_variants_round_as_compose() {
        assert_eq!(on_art(), Color::rgba(0x0b0d10cc));
        assert_eq!(scrim(), Color::BLACK.alpha(0.6));
        assert_eq!(scrim().a, 153);
        assert_eq!(rail_scrim().a, 128);
        assert_eq!(TILE_HAIRLINE, Color::rgba(0xffffff0f));
        assert_eq!(PROGRESS_TRACK_ON_ART, Color::rgba(0x00000099));
        assert_eq!(pill_rest(), fg().alpha(0.12));
        assert_eq!(text_button_rest(), fg().alpha(0.08));
        assert_eq!(rail_panel(), surface().alpha(0.96));
        assert_eq!(track_highlight(), accent().alpha(0.14));
        for f in [0.0, 0.06, 0.14, 0.5, 0.85, 0.92, 1.0] {
            assert_eq!(alpha(fg(), f), fg().alpha(f), "{f}");
        }
    }

    #[test]
    fn type_styles_match_the_design() {
        let table = [
            (&HERO_TITLE, 38.0, 44.0, Weight::Bold, -0.5),
            (&PAGE_TITLE, 28.0, 34.0, Weight::Bold, -0.25),
            (&SECTION, 18.0, 24.0, Weight::Bold, 0.0),
            (&TITLE, 16.0, 22.0, Weight::Bold, 0.0),
            (&BODY_LARGE, 15.0, 22.0, Weight::Regular, 0.0),
            (&BODY, 14.0, 20.0, Weight::Regular, 0.0),
            (&LABEL, 14.0, 18.0, Weight::Medium, 0.1),
            (&CAPTION, 13.0, 17.0, Weight::Medium, 0.2),
            (&BADGE, 11.0, 13.0, Weight::Bold, 0.4),
            (&PIN_DIGIT, 28.0, 28.0, Weight::Medium, 0.0),
        ];
        for (s, size, line, weight, tracking) in table {
            assert_eq!((s.size, s.line_height, s.weight, s.tracking), (size, line, weight, tracking));
            assert_eq!(s.family, Family::Sans);
            assert!(!s.italic);
        }
        assert_eq!((CODE.family.clone(), CODE.size, CODE.tracking), (Family::Mono, 48.0, 4.0));
        assert_eq!((RAIL_PROFILE.line_height, RAIL_SWITCH.line_height), (17.0, 15.0));
    }

    #[test]
    fn lines_and_derived_lines() {
        assert_eq!((GUTTER, SAFE_X, SAFE_Y, TEXT_COLUMN_END), (96.0, 48.0, 27.0, 560.0));
        assert_eq!(FOCUSED_ROW_TILES_Y, 210.0);
        assert_eq!(FOCUSED_ROW_HEADER_Y - ROWS_CLIP_ABOVE, 160.0);
        let room = CANVAS_H
            - FOCUSED_ROW_HEADER_Y
            - (ROW_HEADER_HEIGHT + HEADER_TO_TILES + 115.0 + TILES_TO_CAPTION + CAPTION_HEIGHT);
        assert_eq!(room, LAST_ROW_ROOM);
        assert_eq!((radius::CONTROL, radius::DIALOG, radius::circle(40.0)), (6.0, 8.0, 20.0));
        assert_eq!((space::L, space::X6), (16.0, 64.0));
    }

    #[test]
    fn focus_treatment() {
        assert_eq!(focus::TILE_SCALE, 1.08);
        assert_eq!(focus::RING_OFFSET + focus::RING_STROKE / 2.0, focus::RING_REACH);
        assert_eq!(focus::pill().edge, Some(control()));
        assert_eq!((focus::pill().focus_fill, focus::pill().focus_content), (accent(), on_accent()));
        assert_eq!(focus::round().focus_scale, 1.08);
        assert_eq!(focus::list_row().focus_scale, 1.0);
    }

    #[test]
    fn ramps_cut_to_the_drawn_span() {
        let s = backdrop_left_scrim().stops(0.0, 960.0);
        assert_eq!(s.first(), Some(&GradientStop::new(0.0, bg())));
        assert_eq!(s[1], GradientStop::new(300.0 / 960.0, alpha(bg(), 0.92)));
        assert_eq!(s.last().unwrap().color, alpha(bg(), 0.0));
        // A span starting inside the ramp begins with the color there.
        let s = bottom_scrim_hero().stops(315.0, 225.0);
        assert_eq!(s[0].color.a, 128);
        assert_eq!(s[1], GradientStop::new((410.0 - 315.0) / 225.0, bg()));
        assert_eq!(s.last().unwrap().color, bg());
        assert_eq!(rail_fade().at(40.0), bg());
        assert_eq!(rail_fade().at(64.0).a, 128);
    }

    /// packages/placeholder's examples, which every app's gradient is held to: each title's hue,
    /// and its two colors as CSS makes them, within a step of a byte.
    #[test]
    fn placeholder_colors_are_the_placeholder_files() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/placeholder/placeholder.json");
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        // The tiles draw CSS's 135° (`kit::placeholder`): another angle needs them drawn again.
        assert_eq!(file["gradient"]["angle"].as_u64(), Some(135));
        let examples = file["examples"].as_array().unwrap();
        assert!(!examples.is_empty());
        let hex = |v: &serde_json::Value| Color::rgb(u32::from_str_radix(&v.as_str().unwrap()[1..], 16).unwrap());
        let near =
            |a: Color, b: Color| [a.r.abs_diff(b.r), a.g.abs_diff(b.g), a.b.abs_diff(b.b)].iter().all(|d| *d <= 1);
        for e in examples {
            let seed = e["seed"].as_str().unwrap();
            assert_eq!(Some(placeholder_hue(seed) as u64), e["hue"].as_u64(), "{seed}");
            let (from, to) = placeholder_colors(seed);
            assert!(near(from, hex(&e["from"])) && near(to, hex(&e["to"])), "{seed}: {from:?} {to:?}");
        }
    }

    #[test]
    fn placeholder_colours() {
        // The web's hash: "a" is 97.
        assert_eq!(placeholder_hue("a"), 97);
        assert_eq!(placeholder_hue("ab"), (97 * 31 + 98) % 360);
        assert_eq!(hsl(0.0, 1.0, 0.5), Color::rgb(0xff0000));
        assert_eq!(hsl(120.0, 1.0, 0.25), Color::rgb(0x008000));
        let (a, b) = placeholder_colors("Arrival");
        assert_ne!(a, b);
    }

    #[test]
    fn tiers() {
        let low = Tier::Low.rules();
        assert!(!low.image_fade && !low.unfocus_animates && !low.backdrop_crossfade);
        assert_eq!((low.backdrop_max_px, low.image_max_px, low.library_window_above), (960, 1280, 2000));
        assert_eq!(Tier::Mid.rules().texture_budget, 128 * 1024 * 1024);
        assert_eq!(Tier::High.rules().texture_budget, 192 * 1024 * 1024);
        assert_eq!(Tier::High.rules().library_window_above, 5000);
    }
}
