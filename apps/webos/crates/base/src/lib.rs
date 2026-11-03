//! The vocabulary the crates of Pley for LG TV share: geometry on the 960×540
//! canvas, colours, remote keys and the pointer, the display list a frame is
//! drawn from, and the contracts through which the interface measures text,
//! asks for pictures and wakes the main loop.
//!
//! Nothing here draws, fetches or decodes; `pley-gfx` and `pley-core` do, behind
//! the traits defined here, so the interface toolkit can be tested headless.

pub mod color;
pub mod display;
pub mod geom;
pub mod icon;
pub mod image;
pub mod input;
pub mod text;
pub mod tier;
pub mod time;
pub mod wake;

pub use color::Color;
pub use display::{Cmd, DisplayList, GradientStop};
pub use geom::{Insets, Point, Rect, Size};
pub use icon::Icon;
pub use image::{Fetch, FetchError, ImageId, ImageProvider, ImageRequest, ImageState, Priority, RgbaImage};
pub use input::{Key, KeyEvent, PointerEvent};
pub use text::{Align, Family, Glyph, TextLayout, TextLine, TextOpts, TextShaper, TextStyle, Weight};
pub use tier::Tier;
pub use wake::Waker;

/// The canvas every screen is laid out on, in dp: the Android TV app's, which
/// the renderer scales to the TV's 1920×1080 interface plane (a scale of 2).
pub const CANVAS: Size = Size { w: 960.0, h: 540.0 };
