//! The window, GL context and input events of the platform Pley for LG TV runs on,
//! behind one trait, with the desktop backend the simulator uses.
//!
//! The app's main loop talks only to [`Platform`]: it waits for events, draws
//! with the GL context it is given, and presents. The TV's backend (SDL2 from
//! the firmware) lives in `pley-webos`; the desktop one (winit and glutin) is
//! [`desktop::DesktopPlatform`], behind the `desktop` feature.

use std::sync::Arc;
use std::time::Duration;

use pley_base::{KeyEvent, PointerEvent, Rect, RgbaImage, Waker};

#[cfg(feature = "desktop")]
pub mod desktop;

/// The GL bindings every crate draws with, re-exported so they agree on one version.
pub use glow;

/// Something the platform reports to the main loop.
#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    Key(KeyEvent),
    /// The pointer, with positions in device pixels of the surface; the app
    /// converts them to dp (divide by [`Platform::scale`]).
    Pointer(PointerEvent),
    /// Text typed with a key press, for fields that take text. It comes after
    /// the [`PlatformEvent::Key`] of the same press; a field that takes the
    /// text ignores that key's meaning.
    Text(String),
    /// The app went to the background (`false`) or came back (`true`).
    Visible(bool),
    /// The surface's new size in device pixels.
    Resized(u32, u32),
    /// The window was closed or the system asked the app to end.
    Quit,
}

/// The window the app draws in, its GL context and its input.
pub trait Platform {
    /// The drawable's size in device pixels.
    fn surface_size(&self) -> (u32, u32);

    /// The GL context, current on this thread, with GLES 2.0 semantics.
    fn gl(&self) -> Arc<glow::Context>;

    /// Waits until an event arrives, the waker is called or `timeout` passes
    /// (`None`: no limit, `Some(ZERO)`: just collect what is there), and
    /// appends what arrived to `out`.
    fn wait_events(&mut self, timeout: Option<Duration>, out: &mut Vec<PlatformEvent>);

    /// Shows the frame just drawn; paced by vsync.
    fn present(&mut self);

    /// Wakes [`Platform::wait_events`] from any thread.
    fn waker(&self) -> Waker;

    /// Shows or hides the pointer (the Magic Remote's, the mouse's).
    fn set_pointer_visible(&mut self, visible: bool);

    /// The window the media pipeline draws video into, where the platform has
    /// one (webOS 5 and later); `None` elsewhere.
    fn video_window(&mut self) -> Option<&mut dyn VideoWindow>;

    /// Ends the app the way the platform expects; `wait_events` then reports
    /// [`PlatformEvent::Quit`].
    fn quit(&mut self);

    /// Device pixels per dp: the 960×540 canvas fitted to the surface.
    fn scale(&self) -> f32 {
        let (w, h) = self.surface_size();
        canvas_scale(w, h)
    }

    /// The frame just drawn, read back from the GL surface: call it after
    /// rendering and before [`Platform::present`] (the simulator's
    /// `--screenshot`, captures on the TV).
    fn read_frame(&mut self) -> RgbaImage {
        let (w, h) = self.surface_size();
        read_pixels(&self.gl(), w, h)
    }
}

/// The exported window LG's media pipeline draws video into (webOS 5 and
/// later), placed under the interface's surface.
pub trait VideoWindow {
    /// Creates the window if there is none and returns the id the load
    /// payload names it by (`_Window_Id_<n>`); `None` when it can't be made.
    fn create(&mut self) -> Option<String>;

    /// Places the picture: `source` is the part of the video frame shown (in
    /// its pixels; `None` for all of it), `dest` where it shows (in device
    /// pixels; `None` for the whole screen).
    fn place(&mut self, source: Option<Rect>, dest: Option<Rect>);

    /// Destroys the window; the next [`VideoWindow::create`] makes a new one.
    fn destroy(&mut self);
}

/// Device pixels per dp for a surface `width` × `height`: the canvas fits the
/// tighter side and grows along the other, as the Android TV app's density
/// does (`TvTheme.kt`, `min(w / 960, h / 540)`).
pub fn canvas_scale(width: u32, height: u32) -> f32 {
    (width as f32 / pley_base::CANVAS.w).min(height as f32 / pley_base::CANVAS.h).max(f32::MIN_POSITIVE)
}

/// Reads the bound framebuffer's `width` × `height` pixels as a picture, top
/// row first and with straight alpha (the framebuffer holds premultiplied
/// colour).
pub fn read_pixels(gl: &glow::Context, width: u32, height: u32) -> RgbaImage {
    use glow::HasContext;

    let mut image = RgbaImage::new(width, height);
    if width == 0 || height == 0 {
        return image;
    }
    let mut raw = vec![0u8; image.pixels.len()];
    // SAFETY: the context is current on this thread (the Platform contract) and
    // `raw` holds exactly width × height RGBA pixels with a pack alignment of 1.
    unsafe {
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            width as i32,
            height as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut raw)),
        );
    }
    flip_and_unpremultiply(&raw, &mut image);
    image
}

/// GL's rows run bottom to top; pictures' top to bottom.
fn flip_and_unpremultiply(raw: &[u8], image: &mut RgbaImage) {
    let row = image.width as usize * 4;
    let height = image.height as usize;
    for y in 0..height {
        let src = &raw[(height - 1 - y) * row..(height - y) * row];
        let dst = &mut image.pixels[y * row..(y + 1) * row];
        for (d, s) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
            let a = s[3];
            if a == 0 || a == 255 {
                d.copy_from_slice(s);
            } else {
                let un = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                d[0] = un(s[0]);
                d[1] = un(s[1]);
                d[2] = un(s[2]);
                d[3] = a;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_fits_the_tighter_side() {
        assert_eq!(canvas_scale(1920, 1080), 2.0);
        assert_eq!(canvas_scale(1280, 720), 1280.0 / 960.0);
        assert_eq!(canvas_scale(1920, 1200), 2.0);
        assert_eq!(canvas_scale(1440, 1080), 1.5);
    }

    #[test]
    fn read_back_flips_rows_and_unpremultiplies() {
        // Two rows as GL returns them: bottom row first.
        let raw = [
            10, 20, 30, 255, 0, 0, 0, 0, // bottom
            64, 32, 0, 128, 255, 255, 255, 255, // top
        ];
        let mut image = RgbaImage::new(2, 2);
        flip_and_unpremultiply(&raw, &mut image);
        assert_eq!(&image.pixels[0..4], &[128, 64, 0, 128]);
        assert_eq!(&image.pixels[4..8], &[255, 255, 255, 255]);
        assert_eq!(&image.pixels[8..12], &[10, 20, 30, 255]);
        assert_eq!(&image.pixels[12..16], &[0, 0, 0, 0]);
    }
}
