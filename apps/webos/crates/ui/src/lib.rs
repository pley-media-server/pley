//! The interface framework of Pley for LG TV: immediate-mode drawing over persistent state, built
//! for a TV remote.
//!
//! Each frame the current screen draws itself into a [`Ui`], which records the display list
//! (`pley_base::DisplayList`), registers focusable rectangles and focus groups, and answers each
//! element with a [`Response`]. What lasts between frames (focus and its memories, keys and the
//! pointer in progress, animated values) lives in the [`UiState`]:
//!
//! - **Focus** follows the TV app's and the Android TV app's rules: [`Group`]s move in order
//!   ([`GroupKind::Row`], [`GroupKind::Column`], [`GroupKind::Grid`]) or by geometry
//!   ([`GroupKind::Free`]) inside, by geometry between groups, with [`Edge`] rules at their
//!   sides; groups remember their last element, traps keep focus until they close, pages keep
//!   their focus on the back stack ([`PageMemory`]), and something is always focused. Moves are
//!   resolved against the previous frame, so the frame after a key press draws the new focus.
//! - **Input**: the repeat gate ([`KeyGate`]: a key held 180 ms moves focus every 45 ms on the
//!   app's own clock, [`Ui::holding`], [`Ui::rested`]), OK and its 500 ms long press, the menu key, and the Magic
//!   Remote's pointer (hover focuses without re-centring, a click is OK, the wheel steps focus or
//!   scrolls a group that takes it).
//! - **Animation**: [`Ui::animate`] and its colour and rectangle forms, with the design's
//!   [`Motion`]s; held keys, Reduce motion and Low-tier TVs snap them, except that under a held
//!   key scrolls and a tile's focus glide evenly from move to move ([`Motion::GLIDE`], on every
//!   tier); only a running animation asks for another frame.
//! - **Theme**: every colour, type style, spacing, radius, scrim and focus treatment of the
//!   design, the canvas's fixed lines and the device tiers ([`theme`]).
//!
//! [`testing`] runs frames headless with scripted input, for this crate, the kit and the screens.

mod focus;
mod id;
mod input;
mod motion;
pub mod testing;
pub mod theme;
mod ui;

pub use focus::{Dir, Dirs, Edge, FocusOpts, Group, GroupKind, PageMemory};
pub use id::Id;
pub use input::{FrameInput, InputEvent, KeyGate, LONG_PRESS, OK_HELD_MAX, POINTER_SHOWN, TWIN, WHEEL_RUN, nudge};
pub use motion::{Easing, Motion};
pub use theme::{Tier, TierExt};
pub use ui::{Fit, FrameOutput, ImageOpts, Response, Shown, Ui, UiState, fit_rects};

#[cfg(test)]
mod tests;
