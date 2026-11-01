//! The exported window LG's media pipeline draws video into, webOS 5 and
//! later.
//!
//! The app exports a region of its own surface; the compositor answers with an
//! id shaped `_Window_Id_<n>`, which the load payload names as
//! `option.windowId`, and that is the whole binding. One window per playback:
//! made before the load, destroyed after the pipeline is released (a window
//! kept across playbacks leaves the second one without a picture). Placing it
//! scales the picture: the part of the frame shown (`source`, in the video's
//! pixels) into a rectangle of the screen (`dest`, in device pixels). LG's
//! SDL must be called on the thread that made the window.

use std::ffi::{CStr, CString, c_int};

use pley_base::Rect;
use pley_platform::VideoWindow;

use super::{SdlRect, sdl};

/// The video type of exported window (`SDL_WEBOS_EXPORED_WINDOW_TYPE_VIDEO`,
/// spelt so in LG's header).
const VIDEO: c_int = 0;

/// The full 1920×1080 interface plane, the size placed when none is known.
pub const FULL: Rect = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };

/// An exported window, made on demand.
#[derive(Debug, Default)]
pub struct ExportedWindow {
    id: Option<CString>,
}

impl ExportedWindow {
    /// Whether this firmware's SDL exports windows (webOS 5 and later).
    pub fn available() -> bool {
        sdl().is_some_and(|s| s.ext.create_exported.is_some() && s.ext.set_exported.is_some())
    }

    /// The id of the window made, if one is.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref().and_then(|id| id.to_str().ok())
    }
}

impl VideoWindow for ExportedWindow {
    fn create(&mut self) -> Option<String> {
        if self.id.is_none() {
            let create = sdl()?.ext.create_exported?;
            // SAFETY: LG's function with its C signature; the string it returns is
            // copied at once, since its lifetime isn't documented.
            let id = unsafe {
                let raw = create(VIDEO);
                if raw.is_null() {
                    return None;
                }
                CStr::from_ptr(raw).to_owned()
            };
            if id.as_bytes().is_empty() {
                return None;
            }
            self.id = Some(id);
        }
        self.id().map(str::to_string)
    }

    fn place(&mut self, source: Option<Rect>, dest: Option<Rect>) {
        let (Some(id), Some(set)) = (&self.id, sdl().and_then(|s| s.ext.set_exported)) else {
            return;
        };
        // Never a null rectangle: on the compositor's side a null region aborts.
        let mut src = SdlRect::from(source.unwrap_or(FULL));
        let mut dst = SdlRect::from(dest.unwrap_or(FULL));
        // SAFETY: a window id from SDL and two valid rectangles for the call.
        let placed = unsafe { set(id.as_ptr(), &mut src, &mut dst) };
        if placed == 0 {
            crate::log::line("sdl: the exported window couldn't be placed");
        }
    }

    fn destroy(&mut self) {
        let Some(id) = self.id.take() else { return };
        if let Some(destroy) = sdl().and_then(|s| s.ext.destroy_exported) {
            // SAFETY: an id SDL gave and nothing uses any more.
            unsafe { destroy(id.as_ptr()) };
        }
    }
}

impl Drop for ExportedWindow {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl From<Rect> for SdlRect {
    fn from(r: Rect) -> SdlRect {
        SdlRect { x: r.x.round() as c_int, y: r.y.round() as c_int, w: r.w.round() as c_int, h: r.h.round() as c_int }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_round_to_whole_pixels() {
        let r = SdlRect::from(Rect::new(0.4, 10.5, 1039.6, 1080.0));
        assert_eq!((r.x, r.y, r.w, r.h), (0, 11, 1040, 1080));
    }

    #[test]
    fn off_the_tv_no_window_is_made() {
        let mut w = ExportedWindow::default();
        if !ExportedWindow::available() {
            assert_eq!(w.create(), None);
        }
        w.place(None, None);
        w.destroy();
        assert_eq!(w.id(), None);
    }
}
