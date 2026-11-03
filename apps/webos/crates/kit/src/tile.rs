//! Tiles (`TvTile.kt`, `designsystem/tile/`, `designsystem/image/Placeholder.kt`): a well of
//! the kind's size holding the picture (or its placeholder until the picture covers it), the
//! overlays on the art, and a caption 14 dp under it. Focused, the well grows to 1.08 about its
//! centre with a 3 dp AccentBright ring 4.5 dp outside the grown edge, one progress value driving
//! both (150 ms in, 120 ms out; under a held key evenly from move to move, as the Samsung app's
//! tiles do; Low-tier TVs animate only the focused tile); while OK is held it shrinks to 1.04. The caption stays outside the growth, its title bold when
//! focused. No glow.

use pley_base::{Color, Icon, ImageRequest, ImageState, Point, Priority, Rect, Size, TextOpts, TextStyle, Weight};
use pley_ui::theme::{self, focus, radius};
use pley_ui::{FocusOpts, Id, ImageOpts, Motion, Response, Shown, Ui};

use crate::badge::{BADGE_HEIGHT, Badge, BadgeStyle};
use crate::look::{focus_progress, free_line, register, spoken, text_mid, tile_focus_progress};

/// The kinds of tile (`TileKind.kt`, `TileStyle.Tv`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TileKind {
    /// 108 × 162 (2:3): movies, shows, seasons.
    Poster,
    /// 204 × 115 (16:9): Continue Watching, extras, episodes in search.
    Landscape,
    /// 184 × 104 (16:9): a season's episodes, Live TV's channels.
    Episode,
    /// 96 × 144: a library's grid, collections.
    LibraryGrid,
    /// 132 × 132: music, playlists.
    Square,
    /// A 96 circle: cast and crew, people in search.
    Person,
    /// A 100 circle: Who's watching?
    Avatar,
}

impl TileKind {
    /// The well's size.
    pub const fn size(self) -> Size {
        match self {
            TileKind::Poster => Size::new(108.0, 162.0),
            TileKind::Landscape => Size::new(204.0, 115.0),
            TileKind::Episode => Size::new(184.0, 104.0),
            TileKind::LibraryGrid => Size::new(96.0, 144.0),
            TileKind::Square => Size::new(132.0, 132.0),
            TileKind::Person => Size::new(96.0, 96.0),
            TileKind::Avatar => Size::new(100.0, 100.0),
        }
    }

    /// The gap between neighbours in a row: 18 between wide tiles, 14 between the others.
    pub const fn gap(self) -> f32 {
        match self {
            TileKind::Landscape | TileKind::Episode => theme::WIDE_TILE_GAP,
            _ => theme::TILE_GAP,
        }
    }

    /// From one tile's left edge to the next's.
    pub const fn pitch(self) -> f32 {
        self.size().w + self.gap()
    }

    /// Whether the well is a circle.
    pub const fn round(self) -> bool {
        matches!(self, TileKind::Person | TileKind::Avatar)
    }

    /// The well's corner radius.
    pub fn radius(self) -> f32 {
        if self.round() { radius::circle(self.size().w) } else { radius::TILE }
    }

    /// The well's scale when focused: 1.08, a profile's 1.1 (`TvProfilesScreen.kt`).
    pub fn focus_scale(self) -> f32 {
        if self == TileKind::Avatar { 1.1 } else { focus::TILE_SCALE }
    }

    /// A row of these, from its header's top to its caption's bottom: 24 + 14 + well + 14 + 34
    /// (Poster 248, Landscape 201, Episode 190, Square 218, Person 182); without a header, the
    /// well and its caption.
    pub fn row_height(self, header: bool) -> f32 {
        let head = if header { theme::ROW_HEADER_HEIGHT + theme::HEADER_TO_TILES } else { 0.0 };
        head + self.size().h + theme::TILES_TO_CAPTION + theme::CAPTION_HEIGHT
    }
}

/// What a tile's well holds.
#[derive(Clone, Copy)]
pub enum Well<'a> {
    /// The tile's picture, its placeholder until the picture covers it, and the overlays.
    Art,
    /// A row's last tile when the row has more: a 32 dp arrow over the label (`TvSeeAllTile`).
    SeeAll(&'a str),
    /// A still Surface2 shape of what is coming (a grid position whose page hasn't come).
    Skeleton,
    /// Drawn by the screen in the well's rectangle (a channel's logo); focus is drawn around it.
    Custom(&'a dyn Fn(&mut Ui, Rect)),
}

impl std::fmt::Debug for Well<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Well::Art => f.write_str("Art"),
            Well::SeeAll(l) => write!(f, "SeeAll({l:?})"),
            Well::Skeleton => f.write_str("Skeleton"),
            Well::Custom(_) => f.write_str("Custom"),
        }
    }
}

/// Whether a slot is drawn or only has its picture fetched ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlotMode {
    Draw,
    /// Within 1.5 screens past what shows: ask for the picture, draw nothing.
    Prefetch,
}

/// Where a container wants a tile drawn, and what focus did to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileSlot {
    /// The tile's position in the container's list.
    pub index: usize,
    pub id: Id,
    pub kind: TileKind,
    /// The well, unscaled, in screen coordinates.
    pub rect: Rect,
    pub response: Response,
    /// Drawn, or only asked to fetch its picture.
    pub mode: SlotMode,
    /// How soon its picture is needed.
    pub priority: Priority,
}

/// A tile: what its well holds, its caption and its overlays.
#[derive(Clone, Copy, Debug)]
pub struct Tile<'a> {
    pub kind: TileKind,
    pub title: &'a str,
    pub subtitle: &'a str,
    /// Picture addresses, tried in order: the first that loads wins.
    pub art: &'a [ImageRequest],
    /// What the placeholder's colour is made from (the title by default).
    pub seed: &'a str,
    /// How far it was played, 0 to 1, when in progress.
    pub progress: Option<f32>,
    /// Unwatched episodes of a show or season: a count instead of the tick.
    pub unwatched: u32,
    pub watched: bool,
    /// Quality badges at the bottom left (a library sorted by bitrate).
    pub badges: &'a [String],
    /// A count at the top right (a collection's items).
    pub count: Option<&'a str>,
    pub well: Well<'a>,
    /// Whether the caption is drawn (a See all tile has none).
    pub caption: bool,
}

impl<'a> Tile<'a> {
    /// A tile of `kind` titled `title`, with no picture yet.
    pub fn new(kind: TileKind, title: &'a str) -> Tile<'a> {
        Tile {
            kind,
            title,
            subtitle: "",
            art: &[],
            seed: title,
            progress: None,
            unwatched: 0,
            watched: false,
            badges: &[],
            count: None,
            well: Well::Art,
            caption: true,
        }
    }

    /// A See all tile of `kind`.
    pub fn see_all(kind: TileKind, label: &'a str) -> Tile<'a> {
        Tile { well: Well::SeeAll(label), caption: false, ..Tile::new(kind, "") }
    }

    /// A skeleton of `kind`: the well and two caption bars.
    pub fn skeleton(kind: TileKind) -> Tile<'a> {
        Tile { well: Well::Skeleton, ..Tile::new(kind, "") }
    }

    /// The caption's second line.
    pub fn subtitle(self, subtitle: &'a str) -> Tile<'a> {
        Tile { subtitle, ..self }
    }

    /// The picture's addresses, first choice first.
    pub fn art(self, art: &'a [ImageRequest]) -> Tile<'a> {
        Tile { art, ..self }
    }

    /// The placeholder's seed.
    pub fn seed(self, seed: &'a str) -> Tile<'a> {
        Tile { seed, ..self }
    }

    /// In progress, `fraction` played.
    pub fn progress(self, fraction: Option<f32>) -> Tile<'a> {
        Tile { progress: fraction, ..self }
    }

    /// Unwatched episodes.
    pub fn unwatched(self, count: u32) -> Tile<'a> {
        Tile { unwatched: count, ..self }
    }

    /// Watched: the tick, when nothing is unwatched.
    pub fn watched(self, watched: bool) -> Tile<'a> {
        Tile { watched, ..self }
    }

    /// Quality badges.
    pub fn badges(self, badges: &'a [String]) -> Tile<'a> {
        Tile { badges, ..self }
    }

    /// A count badge.
    pub fn count(self, count: &'a str) -> Tile<'a> {
        Tile { count: Some(count), ..self }
    }

    /// A well the screen draws.
    pub fn custom(self, draw: &'a dyn Fn(&mut Ui, Rect)) -> Tile<'a> {
        Tile { well: Well::Custom(draw), ..self }
    }

    /// Asks for the picture only (a slot past the screen).
    pub fn prefetch(&self, ui: &mut Ui, priority: Priority) {
        if matches!(self.well, Well::Art) {
            first_art(ui, self.art, priority);
        }
    }

    /// Draws the tile in a container's slot, or asks for its picture when the slot is ahead of
    /// the screen.
    pub fn paint(&self, ui: &mut Ui, slot: &TileSlot) {
        if slot.mode == SlotMode::Prefetch {
            self.prefetch(ui, slot.priority);
            return;
        }
        ui.name(slot.id, || spoken(&[self.title, self.subtitle]));
        let look = tile_look(ui, slot.id, self.kind, &slot.response);
        let rect = slot.rect;
        ui.transform(Point::ZERO, look.scale, rect.center(), |ui| self.draw_well(ui, rect, slot.priority));
        ring(ui, rect, self.kind, &look);
        if self.caption {
            self.draw_caption(ui, rect, slot.response.focused);
        }
    }

    /// A tile on its own with its well's top-left corner at `at`: registered as `id` (OK held
    /// 500 ms is a long press, for its actions) and drawn.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point) -> Response {
        let rect = Rect::from_origin_size(at, self.kind.size());
        let response = register(ui, id, rect, FocusOpts::default().long_press());
        let slot = TileSlot {
            index: 0,
            id,
            kind: self.kind,
            rect,
            response,
            mode: SlotMode::Draw,
            priority: Priority::Visible,
        };
        self.paint(ui, &slot);
        response
    }

    fn draw_well(&self, ui: &mut Ui, rect: Rect, priority: Priority) {
        let r = self.kind.radius();
        match self.well {
            Well::Art => {
                let shown = first_art(ui, self.art, priority);
                // A person's circle is Surface2 (Android's `TvPersonScreen.kt`, the Samsung app's
                // `TileView.cs`): the gradient is for a title, and a person shows none.
                if shown.is_some_and(|s| s.opaque()) || self.kind.round() {
                    ui.fill(rect, theme::SURFACE2, r);
                } else {
                    placeholder(ui, rect, self.seed, self.title, r);
                }
                if let Some(s) = shown {
                    ui.draw_image(rect, &s, &ImageOpts::cover().rounded(r));
                }
                self.overlays(ui, rect);
            }
            Well::SeeAll(label) => {
                ui.fill(rect, theme::SURFACE2, r);
                let layout = ui.layout(label, &theme::LABEL, &TextOpts::line(rect.w - 16.0));
                let block = 32.0 + 8.0 + layout.size.h;
                let top = rect.y + (rect.h - block) / 2.0;
                ui.icon(Rect::new(rect.center().x - 16.0, top, 32.0, 32.0), Icon::ArrowRight, theme::FG);
                let x = rect.x + (rect.w - layout.size.w) / 2.0;
                ui.draw_text(Point::new(x, top + 40.0), layout, theme::FG);
            }
            Well::Skeleton => ui.fill(rect, theme::SURFACE2, r),
            Well::Custom(draw) => draw(ui, rect),
        }
    }

    /// The overlays on the art, in `TileOverlays.kt`'s order: the hairline, the progress bar, the
    /// unwatched count or the watched tick, the count badge, quality badges.
    fn overlays(&self, ui: &mut Ui, rect: Rect) {
        const INSET: f32 = 6.0;
        if !self.kind.round() {
            ui.stroke(rect.expand(-0.5), theme::TILE_HAIRLINE, radius::TILE - 0.5, 1.0);
        }
        let mut bar = 0.0;
        if let Some(p) = self.progress.filter(|p| *p > 0.0) {
            bar = PROGRESS_HEIGHT;
            let track = Rect::new(rect.x, rect.bottom() - bar, rect.w, bar);
            ui.fill(track, theme::PROGRESS_TRACK_ON_ART, 0.0);
            ui.fill(Rect::new(track.x, track.y, rect.w * p.min(1.0), bar), theme::ACCENT, 0.0);
        }
        let right = rect.right() - INSET;
        if self.unwatched > 0 {
            let text = self.unwatched.to_string();
            let badge = Badge::new(&text, BadgeStyle::Count);
            let w = badge.size(ui).w;
            badge.show(ui, Point::new(right - w, rect.y + INSET));
        } else if self.watched {
            let tick = Rect::new(right - TICK_BOX, rect.y + INSET, TICK_BOX, TICK_BOX);
            ui.fill(tick, theme::ON_ART, radius::BADGE);
            ui.icon(tick.place(Size::new(TICK_GLYPH, TICK_GLYPH), 0.5, 0.5), Icon::Check, theme::ACCENT);
        } else if let Some(count) = self.count {
            let badge = Badge::new(count, BadgeStyle::OnArt);
            let w = badge.size(ui).w;
            badge.show(ui, Point::new(right - w, rect.y + INSET));
        }
        let mut x = rect.x + INSET;
        let top = rect.bottom() - bar - INSET - BADGE_HEIGHT;
        for b in self.badges {
            let badge = Badge::new(b, BadgeStyle::OnArt);
            let w = badge.size(ui).w;
            // Badges that don't fit are dropped.
            if x + w > right {
                break;
            }
            badge.show(ui, Point::new(x, top));
            x += w + 6.0;
        }
    }

    fn draw_caption(&self, ui: &mut Ui, rect: Rect, focused: bool) {
        let mut y = rect.bottom() + theme::TILES_TO_CAPTION;
        match self.well {
            Well::Skeleton => {
                // `SkeletonTile`: two bars, 60 % and 35 % of the width, 0.6 of a line tall.
                let h = theme::CAPTION.line_height * 0.6;
                let pad = (theme::CAPTION.line_height - h) / 2.0;
                ui.fill(Rect::new(rect.x, y + pad, rect.w * 0.6, h), theme::SURFACE2, radius::BADGE);
                y += theme::CAPTION.line_height;
                ui.fill(Rect::new(rect.x, y + pad, rect.w * 0.35, h), theme::SURFACE2, radius::BADGE);
            }
            _ => caption(ui, Point::new(rect.x, y), rect.w, self.title, self.subtitle, focused),
        }
    }
}

/// A progress bar's height on art.
pub const PROGRESS_HEIGHT: f32 = 4.0;
/// The watched tick's box and its check.
const TICK_BOX: f32 = 20.0;
const TICK_GLYPH: f32 = 12.0;

/// A tile's caption from `at`, `width` wide: the title in `Tv.Caption` Fg (bold when focused; the
/// weight switches, never animates), the subtitle in Muted, one line each with "…".
pub fn caption(ui: &mut Ui, at: Point, width: f32, title: &str, subtitle: &str, focused: bool) {
    let style: TextStyle = if focused { theme::CAPTION.with_weight(Weight::Bold) } else { theme::CAPTION };
    if !title.is_empty() {
        ui.text(at, title, &style, &TextOpts::line(width), theme::FG);
    }
    if !subtitle.is_empty() {
        let y = at.y + theme::CAPTION.line_height;
        ui.text(Point::new(at.x, y), subtitle, &theme::CAPTION, &TextOpts::line(width), theme::MUTED);
    }
}

/// A tile's focus this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TileLook {
    /// 0 at rest, 1 focused: the ring's alpha and the growth.
    pub progress: f32,
    /// The well's scale about its centre.
    pub scale: f32,
}

/// The look of the tile `id` of `kind` that answered `r`: the focus progress, and the scale it
/// gives with the press (`TileFocusLook` in `TvTile.kt`).
pub fn tile_look(ui: &mut Ui, id: Id, kind: TileKind, r: &Response) -> TileLook {
    let p = tile_focus_progress(ui, id, r.focused);
    let pressed = focus::TILE_PRESSED_SCALE / focus::TILE_SCALE;
    let press = ui.animate(id.with("kit.press"), if r.pressed { pressed } else { 1.0 }, Motion::PRESS);
    TileLook { progress: p, scale: (1.0 + (kind.focus_scale() - 1.0) * p) * press }
}

/// The focus ring around a well at `rect` grown to `look.scale`: 3 dp of AccentBright centred
/// 4.5 dp outside the grown edge, its corners the tile's grown radius plus 4.5 (a circle around
/// round tiles), its alpha the focus progress.
pub fn ring(ui: &mut Ui, rect: Rect, kind: TileKind, look: &TileLook) {
    if look.progress <= 0.0 {
        return;
    }
    let grown = rect.scale_about(rect.center(), look.scale);
    let r = grown.expand(focus::RING_OFFSET);
    let corner = if kind.round() { r.w / 2.0 } else { radius::TILE * look.scale + focus::RING_OFFSET };
    ui.stroke(r, focus::RING_COLOR.alpha(look.progress), corner, focus::RING_STROKE);
}

/// The first of `art` that hasn't failed, as it can show now (asking for it at `priority`); `None`
/// when there is none or every one failed. A later address is asked for only once the ones before
/// it have failed (`ArtworkImage`'s candidates).
pub fn first_art(ui: &mut Ui, art: &[ImageRequest], priority: Priority) -> Option<Shown> {
    for req in art {
        let shown = ui.image_state(req, priority);
        if !matches!(shown.state, ImageState::Failed) {
            return Some(shown);
        }
    }
    None
}

/// The placeholder art (`Placeholder.kt`): a 135° gradient from hsl(h, 30 %, 22 %) at the top-left
/// to hsl(h + 40, 35 %, 12 %) at the bottom-right, `h` from the seed, with `title` centred on it
/// in `Tv.Title` at Fg 70 % (12 dp in, up to 4 lines), smaller (down to 0.7×) when its longest
/// word wouldn't fit whole. An empty title draws the gradient alone (backdrops, a channel's or an
/// album's well); a person's round well is Surface2 instead.
pub fn placeholder(ui: &mut Ui, rect: Rect, seed: &str, title: &str, corner: f32) {
    let (a, b) = theme::placeholder_colors(seed);
    // CSS's 135° gradient puts the end colours on the top-left and bottom-right corners; the other
    // two corners sit where their projection on the gradient line falls.
    let k = if rect.w + rect.h > 0.0 { (rect.w - rect.h) / (2.0 * (rect.w + rect.h)) } else { 0.0 };
    let colors = [a, a.lerp(b, 0.5 + k), a.lerp(b, 0.5 - k), b];
    ui.cmd(pley_base::Cmd::Quad { rect, colors, radius: corner });
    if title.is_empty() {
        return;
    }
    let avail = rect.w - 24.0;
    if avail <= 0.0 {
        return;
    }
    let style = placeholder_style(title, avail);
    let opts = TextOpts::wrap(avail, 4).aligned(pley_base::Align::Center);
    let layout = ui.layout(title, &style, &opts);
    let y = rect.y + (rect.h - layout.size.h) / 2.0;
    ui.draw_text(Point::new(rect.x + 12.0, y), layout, theme::PLACEHOLDER_TITLE);
}

/// The placeholder title's style: `Tv.Title`, shrunk in half steps so the longest word fits
/// `avail` whole, at least 0.7× (the TV app's `placeholderTitleSize`: a character is taken as
/// 0.56 of the size). Scripts without spaces between words keep the full size.
fn placeholder_style(title: &str, avail: f32) -> TextStyle {
    let base = theme::TITLE;
    let unspaced = title.chars().any(|c| matches!(c as u32, 0x0e00..=0x0e7f | 0x3000..=0x9fff | 0xac00..=0xd7af));
    let longest = title.split_whitespace().map(|w| w.chars().count()).max().unwrap_or(0);
    if unspaced || longest == 0 {
        return base;
    }
    let fit = avail / (longest as f32 * 0.56);
    if fit >= base.size {
        return base;
    }
    let size = ((fit * 2.0).floor() / 2.0).max(base.size * 0.7);
    TextStyle { size, line_height: base.line_height * size / base.size, ..base }
}

/// A profile on Who's watching? (`TvProfilesScreen.kt`): a circle of `size` (100) holding the
/// profile's picture, or its initial on its colour; its name 12 below in `Tv.Label`, centred, bold
/// when focused. Focused it grows to 1.1 with the tile ring; PIN profiles carry a lock badge.
#[derive(Clone, Copy, Debug)]
pub struct ProfileTile<'a> {
    pub name: &'a str,
    /// Its colour, or the one its name gives ([`theme::profile_color`]).
    pub color: Option<Color>,
    pub picture: &'a [ImageRequest],
    pub locked: bool,
    pub size: f32,
    /// Drawn at 60 % while the app switches to it.
    pub dimmed: bool,
}

impl<'a> ProfileTile<'a> {
    /// A profile named `name`, 100 dp.
    pub fn new(name: &'a str) -> ProfileTile<'a> {
        ProfileTile { name, color: None, picture: &[], locked: false, size: 100.0, dimmed: false }
    }

    /// The profile's own colour.
    pub fn color(self, color: Option<Color>) -> ProfileTile<'a> {
        ProfileTile { color, ..self }
    }

    /// Its picture's addresses.
    pub fn picture(self, picture: &'a [ImageRequest]) -> ProfileTile<'a> {
        ProfileTile { picture, ..self }
    }

    /// It asks for a PIN.
    pub fn locked(self, locked: bool) -> ProfileTile<'a> {
        ProfileTile { locked, ..self }
    }

    /// A circle of `size` dp (the PIN pad's avatar is 56, the rail's smaller).
    pub fn size(self, size: f32) -> ProfileTile<'a> {
        ProfileTile { size, ..self }
    }

    /// Dimmed while switching to it.
    pub fn dimmed(self, dimmed: bool) -> ProfileTile<'a> {
        ProfileTile { dimmed, ..self }
    }

    /// Draws the circle alone (no focus, no name) in `rect`.
    pub fn avatar(&self, ui: &mut Ui, rect: Rect) {
        let fill = self.color.unwrap_or_else(|| theme::profile_color(self.name));
        let r = radius::circle(rect.w);
        let shown = first_art(ui, self.picture, Priority::Visible);
        if !shown.is_some_and(|s| s.opaque()) {
            ui.fill(rect, fill, r);
            let initial: String = self.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            let style = TextStyle { size: rect.w * 0.44, line_height: rect.w * 0.5, ..theme::PAGE_TITLE };
            let layout = ui.layout(&initial, &style, &free_line());
            let x = rect.x + (rect.w - layout.size.w) / 2.0;
            text_mid(ui, x, rect.y, rect.h, layout, theme::content_on(fill));
        }
        if let Some(s) = shown {
            ui.draw_image(rect, &s, &ImageOpts::cover().rounded(r));
        }
    }

    /// Draws the profile with its circle's top-left corner at `at`, registered as `id`.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point) -> Response {
        let rect = Rect::new(at.x, at.y, self.size, self.size);
        ui.name(id, || self.name.to_string());
        let response = register(ui, id, rect, FocusOpts::default());
        let p = focus_progress(ui, id, response.focused);
        let look = TileLook { progress: p, scale: 1.0 + (TileKind::Avatar.focus_scale() - 1.0) * p };
        let alpha = if self.dimmed { 0.6 } else { 1.0 };
        ui.opacity(alpha, |ui| {
            ui.transform(Point::ZERO, look.scale, rect.center(), |ui| {
                self.avatar(ui, rect);
                if self.locked {
                    // A 28 circle centred at 0.86 of the avatar on both axes: a 2 dp Bg ring round
                    // a Surface3 disc with a 14 dp lock.
                    let c = Point::new(rect.x + rect.w * 0.86, rect.y + rect.h * 0.86);
                    let badge = Rect::new(c.x - 14.0, c.y - 14.0, 28.0, 28.0);
                    ui.fill(badge, theme::BG, 14.0);
                    ui.fill(badge.expand(-2.0), theme::SURFACE3, 12.0);
                    ui.icon(badge.place(Size::new(14.0, 14.0), 0.5, 0.5), Icon::Lock, theme::FG);
                }
            });
            ring(ui, rect, TileKind::Avatar, &look);
            let style = if response.focused { theme::LABEL.with_weight(Weight::Bold) } else { theme::LABEL };
            let width = self.size + 40.0;
            let opts = TextOpts::line(width).aligned(pley_base::Align::Center);
            ui.text(
                Point::new(rect.center().x - width / 2.0, rect.bottom() + 12.0),
                self.name,
                &style,
                &opts,
                theme::FG,
            );
        });
        response
    }
}
