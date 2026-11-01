//! The kit of Pley for LG TV: the buttons, tiles, rows, grids, panels, dialogs, keyboards and other
//! controls every screen is built from, drawn as the Android TV app draws them (its TV kit,
//! `apps/android/ui/tv/.../kit`, and the design system under it), on `pley-ui`'s frames.
//!
//! Every control is a plain value built with a few methods and shown with `show(ui, id, …)`,
//! which registers what can take focus, draws, and answers with what happened ([`Response`], or a
//! control's own answer). Containers that scroll ([`Row`], [`RowColumn`], [`PosterGrid`], [`List`])
//! keep a small state the screen owns ([`RowState`], …), so a page drawn again after Back opens
//! where it was left; they draw only what is near the screen and call back for each tile or row.
//!
//! Geometry is in dp on the 960×540 canvas, colours and type come from [`pley_ui::theme`], and
//! motion from [`pley_ui::Motion`] (snapping while a key is held, with Reduce motion and, for some,
//! on Low-tier TVs). Where a number comes from is said beside it: the Android TV app's files
//! (`apps/android/ui/tv/...`, its design system under `apps/android/core/designsystem/...`) and the
//! TV web app's (`packages/tv/src/ui/...`) that LG used until now.
//!
//! [`gallery()`] draws every control on one page, for the app's debug route and for screenshots.

pub mod backdrop;
pub mod badge;
pub mod button;
pub mod column;
pub mod feedback;
pub mod gallery;
pub mod grid;
pub mod keyboard;
pub mod list;
pub mod look;
pub mod overlay;
pub mod pad;
pub mod qr;
mod raster;
pub mod row;
pub mod tile;

pub use backdrop::{Backdrop, BackdropArt, BackdropScrim, BackdropState, backdrop_px, faint_art, prefetch_backdrops};
pub use badge::{Badge, BadgeStyle, MetaLine};
pub use button::{BarItem, Button, ButtonKind, RoundButton, bar_group, button_bar};
pub use column::{ColumnResponse, ColumnState, RowColumn, Section};
pub use feedback::{
    InfoBlock, LoadingRing, Message, MessageResponse, MessageStyle, ProgressBar, ToastPlacement, ToastState,
    skeleton_block, skeleton_row, toast,
};
pub use gallery::{GalleryState, gallery};
pub use grid::{GRID_END, GridResponse, GridState, JUMP_LETTERS, JUMP_X, JumpControl, JumpResponse, PosterGrid};
pub use keyboard::{Keyboard, KeyboardPage, KeyboardResponse, KeyboardState, TextEdit, TextField};
pub use list::{Heading, List, ListResponse, ListRow, ListSlot, ListState, Stepper, StepperResponse, Trailing, switch};
pub use look::{PAGE_SCROLL, ResponseExt, SCROLL, Shade, inert, is_inert};
pub use overlay::{Action, ActionSheet, Dialog, DialogButton, DialogResponse, PanelResponse, SidePanel, scrim_fill};
pub use pad::{Keypad, PadKey, PadLayout, PadState, PinDots};
pub use qr::{Qr, QrError};
pub use row::{Row, RowResponse, RowState};
pub use tile::{
    ProfileTile, SlotMode, Tile, TileKind, TileLook, TileSlot, Well, caption, first_art, placeholder, ring, tile_look,
};

pub use pley_ui::Response;

#[cfg(test)]
mod tests;
