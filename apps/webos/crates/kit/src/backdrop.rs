//! The page's backdrop (`TvBackdrop` in `TvSurfaces.kt`, the TV app's `kit/backdrop.tsx`):
//! one picture under everything, full screen, cropped toward its subject
//! (`BiasAlignment(0.4, −0.6)`), never blurred, under two scrims that keep the text column on at
//! least 85 % Bg: one from the left (Bg solid at 0, 92 % at 300, 85 % at 560, clear at 800) and one
//! up from the bottom whose band depends on the page ([`BackdropScrim`]).
//!
//! It follows what the screen wants only once focus has rested (400 ms), and only once the new
//! picture is in (or none of its addresses loads, or a second has passed): then it cross-fades in
//! 300 ms, at once on Low-tier TVs and with Reduce motion. Until a picture comes, the title's
//! placeholder gradient shows. The album page's faint art is [`faint_art`].

use pley_base::{ImageRequest, ImageState, Priority, Rect};
use pley_ui::theme::{self, Ramp, TierExt};
use pley_ui::{Id, ImageOpts, Motion, Ui};

use crate::tile::{first_art, placeholder};

/// The bottom scrim's band (`BackdropScrim`): Bg clear at its start to solid at its end, solid
/// below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BackdropScrim {
    /// Home at rest, under the hero: 220 → 410.
    Hero,
    /// Home in the rows, under the info block: 120 → 300.
    Rows,
    /// A title's page: 260 → 500.
    Item,
}

impl BackdropScrim {
    /// The scrim's ramp along y.
    pub fn ramp(self) -> &'static Ramp {
        match self {
            BackdropScrim::Hero => &theme::BOTTOM_SCRIM_HERO,
            BackdropScrim::Rows => &theme::BOTTOM_SCRIM_ROWS,
            BackdropScrim::Item => &theme::BOTTOM_SCRIM_ITEM,
        }
    }
}

/// A backdrop to show: its picture's addresses (first choice first) and what its placeholder's
/// colour comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct BackdropArt {
    /// Names it (an item's id): the backdrop changes when the key does.
    pub key: u64,
    pub art: Vec<ImageRequest>,
    pub seed: String,
}

impl BackdropArt {
    /// A backdrop named `key`.
    pub fn new(key: u64, art: Vec<ImageRequest>, seed: impl Into<String>) -> BackdropArt {
        BackdropArt { key, art, seed: seed.into() }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Slot {
    art: Option<BackdropArt>,
    /// When it came on screen.
    at: f64,
}

/// What a backdrop keeps between frames (owned by the screen).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackdropState {
    front: Option<Slot>,
    back: Option<Slot>,
    pending: Option<(Option<BackdropArt>, f64)>,
    swaps: u32,
}

impl BackdropState {
    /// The key of the backdrop showing (or coming in).
    pub fn showing(&self) -> Option<u64> {
        self.front.as_ref().and_then(|s| s.art.as_ref()).map(|a| a.key)
    }

    /// Whether a new backdrop waits for focus to rest or its picture to come.
    pub fn waiting(&self) -> bool {
        self.pending.is_some()
    }
}

/// How long a backdrop waits for its picture before it swaps in anyway (`PictureWaitMs`).
pub const PICTURE_WAIT: f64 = 1.0;

/// The page's backdrop, with its bottom scrim.
#[derive(Clone, Copy, Debug)]
pub struct Backdrop {
    scrim: BackdropScrim,
}

impl Backdrop {
    /// A backdrop under `scrim`.
    pub fn new(scrim: BackdropScrim) -> Backdrop {
        Backdrop { scrim }
    }

    /// Draws the backdrop full screen, heading for `wanted` (`None`: no picture, the page's Bg).
    pub fn show(&self, ui: &mut Ui, id: Id, state: &mut BackdropState, wanted: Option<&BackdropArt>) {
        let now = ui.time();
        let goal = wanted.map(|w| w.key);
        let heading = match &state.pending {
            Some((art, _)) => art.as_ref().map(|a| a.key),
            None => state.front.as_ref().and_then(|s| s.art.as_ref()).map(|a| a.key),
        };
        if state.front.is_none() && state.pending.is_none() {
            // The first backdrop shows at once.
            state.front = Some(Slot { art: wanted.cloned(), at: now });
        } else if goal != heading && ui.rested() {
            state.pending = Some((wanted.cloned(), now));
        }
        if let Some((art, since)) = &state.pending {
            let ready = match art {
                Some(a) => match first_art(ui, &a.art, Priority::Visible) {
                    Some(s) => matches!(s.state, ImageState::Ready { .. }),
                    None => true,
                },
                None => true,
            };
            if ready || now - since >= PICTURE_WAIT - 1e-6 {
                let incoming = Slot { art: art.clone(), at: now };
                state.back = state.front.take();
                state.front = Some(incoming);
                state.pending = None;
                state.swaps = state.swaps.wrapping_add(1);
            } else {
                ui.repaint_after(since + PICTURE_WAIT - now);
            }
        }
        let fade = ui.animate_from(id.with(("kit.fade", state.swaps)), 0.0, 1.0, Motion::BACKDROP);
        if fade >= 1.0 {
            state.back = None;
        }
        if let Some(back) = &state.back {
            draw_slot(ui, back);
        }
        if let Some(front) = &state.front {
            let alpha = if state.back.is_some() { fade } else { 1.0 };
            ui.opacity(alpha, |ui| draw_slot(ui, front));
        }
        let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
        ui.ramp(full, false, &theme::BACKDROP_LEFT_SCRIM);
        ui.ramp(full, true, self.scrim.ramp());
    }
}

fn draw_slot(ui: &mut Ui, slot: &Slot) {
    let Some(art) = &slot.art else { return };
    let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
    let shown = first_art(ui, &art.art, Priority::Visible);
    // A picture already in when the slot came swaps in with it; one that comes later fades in.
    let shown = shown.map(|mut s| {
        if let ImageState::Ready { since, .. } = s.state
            && since <= slot.at
        {
            s.opacity = 1.0;
        }
        s
    });
    if !shown.is_some_and(|s| s.opaque()) {
        placeholder(ui, full, &art.seed, "", 0.0);
    }
    if let Some(s) = shown {
        let (h, v) = theme::BACKDROP_BIAS;
        ui.draw_image(full, &s, &ImageOpts::cover().biased(h, v));
    }
}

/// The width in pixels to ask a backdrop for: the screen's, at most 1280 (960 on Low-tier TVs).
pub fn backdrop_px(ui: &Ui) -> u32 {
    ui.image_px(theme::CANVAS_W).min(ui.tier().rules().backdrop_max_px)
}

/// Asks ahead for backdrops focus may rest on next (the focused tile's neighbours), so they swap
/// in as soon as it does.
pub fn prefetch_backdrops(ui: &mut Ui, arts: &[&[ImageRequest]]) {
    for art in arts {
        first_art(ui, art, Priority::Prefetch);
    }
}

/// An album page's faint art (`FaintArt` in `TvMusicKit.kt`): the cover full screen at 14 %,
/// fading into Bg from 40 % of the height down.
pub fn faint_art(ui: &mut Ui, art: &[ImageRequest]) {
    let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
    if let Some(s) = first_art(ui, art, Priority::Visible) {
        ui.opacity(theme::FAINT_ART_ALPHA, |ui| ui.draw_image(full, &s, &ImageOpts::cover()));
    }
    ui.ramp(full, true, &theme::FAINT_ART_FADE);
}
