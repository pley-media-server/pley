//! The renderer of Pley for LG TV: turns a display list into batched GLES2
//! draws at the screen's scale, with glyphs, icons and rounded corners in one
//! alpha atlas, and fetches, decodes and caches pictures.
//!
//! A frame, on the main thread:
//!
//! 1. [`Renderer::prepare`] uploads pictures the workers decoded (within the
//!    frame's budget), so the interface can draw them in this frame;
//! 2. the interface builds its [`pley_base::DisplayList`], asking the
//!    [`ImageCache`] (a [`pley_base::ImageProvider`]) for pictures;
//! 3. [`Renderer::render`] draws the list; the platform presents.

mod atlas;
pub mod frame;
mod glyphs;
pub mod icons;
pub mod images;
mod masks;
mod renderer;
pub mod screenshot;

pub use atlas::{Atlas, AtlasEntry, AtlasFull, AtlasKey};
pub use glyphs::{GlyphMask, GlyphSource, NoGlyphs};
pub use images::{DiskCacheConfig, ImageCache, ImageCacheConfig};
pub use renderer::{FrameStats, GfxError, Renderer};
