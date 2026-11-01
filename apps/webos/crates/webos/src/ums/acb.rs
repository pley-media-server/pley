//! LG's app-switching control block (`libAcbAPI.so.1`, webOS 4.x only), one
//! of webOS 4's candidates for binding an address playback's picture
//! (RES/plx-media.md §4.2, §18.2).
//!
//! The handle is made and initialised once per process with the app id SAM
//! launched (a wrong id binds the plane for another app: sound, a black
//! picture, no error). Per playback, once the pipeline has loaded: the main
//! sink, the pipeline's `mediaId`, `LOADED`, the display window, then the
//! play state, which follows the pipeline's from then on. Every call's
//! trailing task-id pointer is passed: the shorter form corrupts memory.

use std::ffi::{CString, c_char, c_int, c_long};
use std::sync::{Mutex, OnceLock};

use pley_base::Rect;

use super::machine::AcbState;
use crate::dl::{Lib, fn_table};

type Callback = unsafe extern "C" fn(c_long, c_long, c_long, c_long, c_long, *const c_char);

fn_table! {
    /// The ACB functions the binding uses.
    struct AcbApi {
        AcbAPI_create: unsafe extern "C" fn() -> c_long,
        AcbAPI_initialize: unsafe extern "C" fn(c_long, c_int, *const c_char, Callback) -> c_int,
        AcbAPI_setSinkType: unsafe extern "C" fn(c_long, c_int) -> c_int,
        AcbAPI_setMediaId: unsafe extern "C" fn(c_long, *const c_char) -> c_int,
        AcbAPI_setState: unsafe extern "C" fn(c_long, c_int, c_int, *mut c_long) -> c_int,
        AcbAPI_setDisplayWindow: unsafe extern "C" fn(c_long, c_long, c_long, c_long, c_long, c_int, *mut c_long) -> c_int,
    }
}

// The values LG's media clients pass.
const PLAYER_TYPE_MSE: c_int = 10;
const SINK_MAIN: c_int = 0;
const APP_FOREGROUND: c_int = 1;
const PLAY_UNLOADED: c_int = 0;
const PLAY_LOADED: c_int = 1;
const PLAY_PLAYING: c_int = 2;
const PLAY_PAUSED: c_int = 3;

/// The process's ACB handle.
pub struct Acb {
    api: AcbApi,
    handle: c_long,
}

// SAFETY: the handle is a plain number; the library's calls are made under the
// mutex `get` hands out.
unsafe impl Send for Acb {}

/// Answers ACB's notices; the binding reads nothing from them.
unsafe extern "C" fn on_event(
    _acb: c_long,
    _task: c_long,
    _event: c_long,
    _app: c_long,
    _play: c_long,
    _reply: *const c_char,
) {
}

impl Acb {
    /// The process's handle, made on first use for `app_id`; `None` where the
    /// library isn't (webOS 5 and later, or off the TV) or won't initialise.
    pub fn get(app_id: &str) -> Option<&'static Mutex<Acb>> {
        static ACB: OnceLock<Option<Mutex<Acb>>> = OnceLock::new();
        ACB.get_or_init(|| {
            let lib = Lib::open(&["libAcbAPI.so.1"])?;
            let api = AcbApi::load(&lib).ok()?;
            let id = CString::new(app_id).ok()?;
            // SAFETY: ACB's functions with their C signatures; the callback lives
            // for the process.
            let handle = unsafe {
                let handle = (api.AcbAPI_create)();
                if handle == 0 || !ok((api.AcbAPI_initialize)(handle, PLAYER_TYPE_MSE, id.as_ptr(), on_event)) {
                    crate::log::line("acb: the control block wouldn't initialise");
                    return None;
                }
                handle
            };
            Some(Mutex::new(Acb { api, handle }))
        })
        .as_ref()
    }

    /// Binds the pipeline `media_id`'s picture to `dest` (device pixels) and
    /// sets its play state; false when a step is refused.
    pub fn bind(&mut self, media_id: &str, dest: Rect, full: bool, playing: bool) -> bool {
        let Ok(id) = CString::new(media_id) else { return false };
        let mut task: c_long = 0;
        // SAFETY: the process's handle and valid arguments, the task pointer included.
        let steps = unsafe {
            ok((self.api.AcbAPI_setSinkType)(self.handle, SINK_MAIN))
                && ok((self.api.AcbAPI_setMediaId)(self.handle, id.as_ptr()))
                && ok((self.api.AcbAPI_setState)(self.handle, APP_FOREGROUND, PLAY_LOADED, &mut task))
        };
        steps && self.place(dest, full) && self.set_state(if playing { AcbState::Playing } else { AcbState::Paused })
    }

    /// Moves the picture.
    pub fn place(&mut self, dest: Rect, full: bool) -> bool {
        let mut task: c_long = 0;
        let r = |v: f32| v.round() as c_long;
        // SAFETY: as in `bind`.
        ok(unsafe {
            (self.api.AcbAPI_setDisplayWindow)(
                self.handle,
                r(dest.x),
                r(dest.y),
                r(dest.w),
                r(dest.h),
                full as c_int,
                &mut task,
            )
        })
    }

    /// Follows the pipeline's play state.
    pub fn set_state(&mut self, state: AcbState) -> bool {
        let play = match state {
            AcbState::Playing => PLAY_PLAYING,
            AcbState::Paused => PLAY_PAUSED,
            AcbState::Unloaded => PLAY_UNLOADED,
        };
        let mut task: c_long = 0;
        // SAFETY: as in `bind`.
        ok(unsafe { (self.api.AcbAPI_setState)(self.handle, APP_FOREGROUND, play, &mut task) })
    }
}

/// ACB's calls answer true (non-zero) when they went through; -1 is a refusal.
fn ok(result: c_int) -> bool {
    result > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_webos_4_there_is_no_control_block() {
        if cfg!(not(target_os = "linux")) {
            assert!(Acb::get("app.pley.tv").is_none());
        }
        assert!(ok(1) && !ok(0) && !ok(-1));
    }
}
