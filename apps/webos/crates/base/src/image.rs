//! Pictures: asking for one by address and width, and how its bytes are
//! fetched.
//!
//! The interface asks every frame for the pictures it shows; the cache in
//! `pley-gfx` answers at once with what it has, and fetches and decodes the
//! rest off the main thread, waking the loop when they arrive.

use std::fmt;
use std::sync::Arc;

/// A decoded picture held by the renderer (a texture).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u32);

/// How soon a picture is needed. Requests are served in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Priority {
    /// On screen now.
    Visible,
    /// Just off screen: the next tiles of a row, the next rows of a grid.
    Near,
    /// Wanted soon (the next screen's art); dropped first under pressure.
    Prefetch,
}

/// A picture to show: where it comes from and how wide it is drawn.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageRequest {
    /// The full address, including the server's `w` bucket when it has one
    /// (`pley-core` builds these).
    pub url: Arc<str>,
    /// The width it is drawn at, in device pixels; the cache never keeps a
    /// larger decode than this.
    pub width_px: u32,
}

impl ImageRequest {
    pub fn new(url: impl Into<Arc<str>>, width_px: u32) -> ImageRequest {
        ImageRequest { url: url.into(), width_px }
    }
}

/// What the cache can show for a request right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageState {
    /// Decoded and on the GPU. `since` is the [`crate::time::now`] at which it
    /// became ready, so a picture that wasn't in memory can fade in.
    Ready {
        id: ImageId,
        width: u32,
        height: u32,
        since: f64,
    },
    Loading,
    /// It can't be had (missing, unreadable); draw the placeholder for good.
    Failed,
}

/// The picture cache, as the interface sees it.
pub trait ImageProvider {
    /// What can be shown for `req` now; starts or re-prioritises its loading.
    fn request(&mut self, req: &ImageRequest, priority: Priority) -> ImageState;
}

/// A picture the app makes itself (a subtitle, a QR code): straight RGBA,
/// 8 bits a channel, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// A fully transparent picture.
    pub fn new(width: u32, height: u32) -> RgbaImage {
        RgbaImage { width, height, pixels: vec![0; width as usize * height as usize * 4] }
    }
}

impl fmt::Debug for RgbaImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RgbaImage({}×{})", self.width, self.height)
    }
}

/// Why a fetch gave nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The server answered that there is no such picture (404).
    Missing,
    /// The server couldn't be reached; worth asking again later.
    Unreachable,
    /// Anything else, with what was said.
    Failed(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Missing => f.write_str("not found"),
            FetchError::Unreachable => f.write_str("server out of reach"),
            FetchError::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for FetchError {}

/// Fetches a picture's bytes. `pley-core`'s HTTP client implements it, adding
/// the bearer token for the server's own origin only.
pub trait Fetch: Send + Sync {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}
