//! The display list: everything a frame draws, in order, in dp.
//!
//! The interface records it; `pley-gfx` turns it into batched GL draws at the
//! screen's scale. Tests read it to check what a screen draws without a GPU.

use std::sync::Arc;

use crate::color::Color;
use crate::geom::{Point, Rect};
use crate::icon::Icon;
use crate::image::{ImageId, RgbaImage};
use crate::text::TextLayout;

/// A colour at a position along a gradient, 0 at its start and 1 at its end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub at: f32,
    pub color: Color,
}

impl GradientStop {
    pub const fn new(at: f32, color: Color) -> GradientStop {
        GradientStop { at, color }
    }
}

/// One drawing command. Rectangles are in dp, in the coordinates of the
/// enclosing transform.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// A filled rectangle, its corners rounded by `radius` (0 for square).
    Fill {
        rect: Rect,
        color: Color,
        radius: f32,
    },
    /// An outline of `width` centred on the rectangle's edge.
    Stroke {
        rect: Rect,
        color: Color,
        radius: f32,
        width: f32,
    },
    /// A gradient along x (`vertical` false) or y, through its stops, which
    /// are piecewise linear (the renderer draws them as coloured vertices, not
    /// per-pixel work).
    LinearGradient {
        rect: Rect,
        vertical: bool,
        stops: Vec<GradientStop>,
    },
    /// A rectangle whose colour blends between its four corners (top-left,
    /// top-right, bottom-left, bottom-right): diagonal gradients such as the
    /// placeholder art's 135° one, drawn as coloured vertices.
    Quad {
        rect: Rect,
        colors: [Color; 4],
        radius: f32,
    },
    /// A radial glow: `inner` at `center`, fading to `outer` at `radius`.
    RadialGradient {
        rect: Rect,
        center: Point,
        radius: f32,
        inner: Color,
        outer: Color,
    },
    /// A picture from the cache. `src` is the part of it shown, in its pixels
    /// (for cropping to cover); `opacity` fades it in.
    Image {
        rect: Rect,
        image: ImageId,
        src: Rect,
        radius: f32,
        opacity: f32,
    },
    /// A picture the app made. `key` names its content: the renderer keeps
    /// its texture while the key is drawn and uploads it again when it changes.
    Raw {
        rect: Rect,
        image: Arc<RgbaImage>,
        key: u64,
        radius: f32,
        opacity: f32,
    },
    /// A laid-out string with its top-left corner at `origin`.
    Text {
        origin: Point,
        layout: Arc<TextLayout>,
        color: Color,
    },
    Icon {
        rect: Rect,
        icon: Icon,
        color: Color,
    },
    /// Clears the rectangle to transparent so the video plane under the
    /// interface shows through.
    Punch {
        rect: Rect,
    },
    /// Clips what follows to the rectangle (intersected with any outer clip).
    PushClip {
        rect: Rect,
    },
    PopClip,
    /// What follows is scaled by `scale` about `pivot`, then moved by
    /// `offset`: a focused tile growing, a row scrolling.
    PushTransform {
        offset: Point,
        scale: f32,
        pivot: Point,
    },
    PopTransform,
    /// What follows is drawn at `opacity` (multiplied with any outer one).
    PushOpacity(f32),
    PopOpacity,
}

/// A frame: the colour the screen is cleared to, then the commands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    /// Opaque `Bg` for pages; transparent while video plays under the
    /// interface.
    pub clear: Color,
    pub cmds: Vec<Cmd>,
}

impl DisplayList {
    pub fn new(clear: Color) -> DisplayList {
        DisplayList { clear, cmds: Vec::new() }
    }

    pub fn push(&mut self, cmd: Cmd) {
        self.cmds.push(cmd);
    }

    /// Every string drawn, in order: for tests that check what a screen says.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.cmds.iter().filter_map(|c| match c {
            Cmd::Text { layout, .. } => Some(&*layout.text),
            _ => None,
        })
    }

    /// Whether some string drawn contains `needle`.
    pub fn says(&self, needle: &str) -> bool {
        self.texts().any(|t| t.contains(needle))
    }
}
