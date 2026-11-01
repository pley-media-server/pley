//! The TV's window, GL context, remote and lifecycle, over the firmware's own
//! SDL2 (`libSDL2-2.0.so.0`: 2.0.5 on webOS 4, 2.0.10 on 5–6, 2.0.14 on 7 and
//! later), opened at run time (DESIGN.md §5.1, RES/plx-ui-gfx.md §1).
//!
//! The window is 1920×1080 at 0,0 with a GLES 2 context, RGBA8888 with no
//! depth or stencil (a tiler would write an unused depth buffer back every
//! frame), and swap interval 1. LG's video plane shows where the interface's
//! pixels are transparent, which takes the 32-bit config with alpha, the
//! surface's opaque region cleared, and a blend that accumulates alpha (the
//! renderer's). On a 4K panel the plane stays 1920×1080 and the compositor
//! scales it: the panel's own resolution is only a fact for the capability
//! report.

pub mod events;
pub mod keys;
pub mod video;

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use pley_base::Waker;
use pley_platform::glow::{self, HasContext};
use pley_platform::{Platform, PlatformEvent, VideoWindow};

use crate::dl::{Lib, fn_table};
use events::{EVENT_BYTES, Effect, SdlEvent, Translator};
pub use video::ExportedWindow;

/// `SDL_Rect`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SdlRect {
    pub x: c_int,
    pub y: c_int,
    pub w: c_int,
    pub h: c_int,
}

fn_table! {
    /// The SDL2 functions every firmware's SDL has.
    pub(crate) struct SdlApi {
        SDL_SetMainReady: unsafe extern "C" fn(),
        SDL_SetHintWithPriority: unsafe extern "C" fn(*const c_char, *const c_char, c_int) -> c_int,
        SDL_Init: unsafe extern "C" fn(u32) -> c_int,
        SDL_Quit: unsafe extern "C" fn(),
        SDL_GetError: unsafe extern "C" fn() -> *const c_char,
        SDL_GetVersion: unsafe extern "C" fn(*mut u8),
        SDL_GetCurrentVideoDriver: unsafe extern "C" fn() -> *const c_char,
        SDL_GL_SetAttribute: unsafe extern "C" fn(c_int, c_int) -> c_int,
        SDL_CreateWindow: unsafe extern "C" fn(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut c_void,
        SDL_DestroyWindow: unsafe extern "C" fn(*mut c_void),
        SDL_MinimizeWindow: unsafe extern "C" fn(*mut c_void),
        SDL_GetWindowWMInfo: unsafe extern "C" fn(*mut c_void, *mut u8) -> c_int,
        SDL_GL_CreateContext: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
        SDL_GL_DeleteContext: unsafe extern "C" fn(*mut c_void),
        SDL_GL_MakeCurrent: unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int,
        SDL_GL_SetSwapInterval: unsafe extern "C" fn(c_int) -> c_int,
        SDL_GL_GetDrawableSize: unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int),
        SDL_GL_GetProcAddress: unsafe extern "C" fn(*const c_char) -> *mut c_void,
        SDL_GL_SwapWindow: unsafe extern "C" fn(*mut c_void),
        SDL_WaitEventTimeout: unsafe extern "C" fn(*mut u8, c_int) -> c_int,
        SDL_PollEvent: unsafe extern "C" fn(*mut u8) -> c_int,
        SDL_PushEvent: unsafe extern "C" fn(*mut u8) -> c_int,
        SDL_RegisterEvents: unsafe extern "C" fn(c_int) -> u32,
        SDL_ShowCursor: unsafe extern "C" fn(c_int) -> c_int,
    }
}

/// LG's additions to SDL, each looked up on its own: the exported-window family
/// exists from webOS 5, the panel's resolution from 4.4.2, the cursor call not
/// on 3.x. Naming one in code would stop the binary on a firmware without it.
#[derive(Clone, Copy, Default)]
pub(crate) struct WebosExt {
    pub cursor_visibility: Option<unsafe extern "C" fn(c_int) -> c_int>,
    pub panel_resolution: Option<unsafe extern "C" fn(*mut c_int, *mut c_int) -> c_int>,
    pub create_exported: Option<unsafe extern "C" fn(c_int) -> *const c_char>,
    pub set_exported: Option<unsafe extern "C" fn(*const c_char, *mut SdlRect, *mut SdlRect) -> c_int>,
    pub destroy_exported: Option<unsafe extern "C" fn(*const c_char)>,
}

pub(crate) struct Sdl {
    pub api: SdlApi,
    pub ext: WebosExt,
}

/// The firmware's SDL, opened once; `None` off the TV.
pub(crate) fn sdl() -> Option<&'static Sdl> {
    static SDL: OnceLock<Option<Sdl>> = OnceLock::new();
    SDL.get_or_init(|| {
        let lib = Lib::open(&["libSDL2-2.0.so.0"])?;
        let api = match SdlApi::load(&lib) {
            Ok(api) => api,
            Err(missing) => {
                crate::log::line(&format!("sdl: {} lacks {}", lib.name, missing.join(", ")));
                return None;
            }
        };
        // SAFETY: each type is the function's signature in LG's `SDL_webOS.h`.
        let ext = unsafe {
            WebosExt {
                cursor_visibility: lib.sym("SDL_webOSCursorVisibility"),
                panel_resolution: lib.sym("SDL_webOSGetPanelResolution"),
                create_exported: lib.sym("SDL_webOSCreateExportedWindow"),
                set_exported: lib.sym("SDL_webOSSetExportedWindow"),
                destroy_exported: lib.sym("SDL_webOSDestroyExportedWindow"),
            }
        };
        Some(Sdl { api, ext })
    })
    .as_ref()
}

/// The panel's resolution as LG's SDL reported it when the window was made
/// (3840×2160 on a 4K set): a fact for the capability report, never the
/// interface's size.
pub fn panel_resolution() -> Option<(u32, u32)> {
    *PANEL.lock().unwrap_or_else(|e| e.into_inner())
}

static PANEL: Mutex<Option<(u32, u32)>> = Mutex::new(None);
/// The type of the app's own events, once registered.
static USER_TYPE: AtomicU32 = AtomicU32::new(0);

/// Asks the platform's thread to hide the app (`SDL_MinimizeWindow`), from any
/// thread: the fallback when the TV's Home can't be shown.
pub fn request_minimize() -> bool {
    push_user(events::user::MINIMIZE)
}

fn push_user(code: i32) -> bool {
    let ty = USER_TYPE.load(Ordering::Acquire);
    let Some(sdl) = sdl().filter(|_| ty != 0) else { return false };
    let mut buf = EventBuf::new();
    buf.0[0..4].copy_from_slice(&ty.to_ne_bytes());
    buf.0[12..16].copy_from_slice(&code.to_ne_bytes());
    // SAFETY: a user event in a buffer larger than SDL_Event; SDL_PushEvent is
    // thread-safe and copies it.
    unsafe { (sdl.api.SDL_PushEvent)(buf.0.as_mut_ptr()) > 0 }
}

/// Why the window couldn't be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SdlError(pub String);

impl fmt::Display for SdlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SdlError {}

/// One `SDL_Event`, with room to spare and SDL's alignment.
#[repr(C, align(8))]
struct EventBuf([u8; EVENT_BYTES]);

impl EventBuf {
    fn new() -> EventBuf {
        EventBuf([0; EVENT_BYTES])
    }
}

// SDL_Init's flag, SDL_GLattr's values and window flags (SDL2's headers).
const INIT_VIDEO: u32 = 0x20;
const GL_RED_SIZE: c_int = 0;
const GL_GREEN_SIZE: c_int = 1;
const GL_BLUE_SIZE: c_int = 2;
const GL_ALPHA_SIZE: c_int = 3;
const GL_BUFFER_SIZE: c_int = 4;
const GL_DOUBLEBUFFER: c_int = 5;
const GL_DEPTH_SIZE: c_int = 6;
const GL_STENCIL_SIZE: c_int = 7;
const GL_CONTEXT_MAJOR_VERSION: c_int = 17;
const GL_CONTEXT_MINOR_VERSION: c_int = 18;
const GL_CONTEXT_PROFILE_MASK: c_int = 21;
const GL_CONTEXT_PROFILE_ES: c_int = 4;
const WINDOW_FULLSCREEN: u32 = 0x1;
const WINDOW_OPENGL: u32 = 0x2;
const HINT_OVERRIDE: c_int = 2;
/// `SDL_SYSWM_WAYLAND`.
const SYSWM_WAYLAND: u32 = 6;
/// `wl_surface.set_opaque_region`'s request number.
const WL_SURFACE_SET_OPAQUE_REGION: u32 = 4;

/// The interface plane every LG TV has.
pub const PLANE: (u32, u32) = (1920, 1080);

/// The hints LG's SDL reads before `SDL_Init`. Back reaches the app only with
/// the first (otherwise the TV acts on it itself).
pub const HINTS: [(&str, &str); 1] = [("SDL_WEBOS_ACCESS_POLICY_KEYS_BACK", "true")];

/// The platform on an LG TV.
pub struct WebosPlatform {
    sdl: &'static Sdl,
    window: *mut c_void,
    context: *mut c_void,
    gl: Arc<glow::Context>,
    size: (u32, u32),
    waker: Waker,
    wake_pending: Arc<AtomicBool>,
    user_type: u32,
    translator: Translator,
    event: Box<EventBuf>,
    quit: bool,
    video: ExportedWindow,
}

impl WebosPlatform {
    /// Opens the firmware's SDL, makes the window and its GL context, and reads
    /// the panel's resolution.
    ///
    /// # Safety
    ///
    /// Call it first in `main`, before any other thread starts: it sets
    /// environment variables LG's SDL reads, which is only sound while no other
    /// thread reads the environment.
    pub unsafe fn new() -> Result<WebosPlatform, SdlError> {
        let sdl = sdl().ok_or_else(|| SdlError("the TV's SDL (libSDL2-2.0.so.0) isn't there".into()))?;
        let api = &sdl.api;
        for (name, value) in HINTS {
            // SAFETY: the caller promises no other thread runs yet.
            unsafe { std::env::set_var(name, value) };
            let (n, v) = (CString::new(name).expect("a hint name"), CString::new(value).expect("a hint value"));
            // SAFETY: NUL-terminated strings SDL copies.
            unsafe { (api.SDL_SetHintWithPriority)(n.as_ptr(), v.as_ptr(), HINT_OVERRIDE) };
        }
        // SAFETY: SDL's functions called in the order SDL documents, on this thread,
        // with the pointers SDL returns.
        unsafe {
            (api.SDL_SetMainReady)();
            if (api.SDL_Init)(INIT_VIDEO) != 0 {
                return Err(error(sdl, "SDL_Init"));
            }
            for (attr, value) in [
                (GL_CONTEXT_PROFILE_MASK, GL_CONTEXT_PROFILE_ES),
                (GL_CONTEXT_MAJOR_VERSION, 2),
                (GL_CONTEXT_MINOR_VERSION, 0),
                (GL_RED_SIZE, 8),
                (GL_GREEN_SIZE, 8),
                (GL_BLUE_SIZE, 8),
                (GL_ALPHA_SIZE, 8),
                (GL_BUFFER_SIZE, 32),
                (GL_DEPTH_SIZE, 0),
                (GL_STENCIL_SIZE, 0),
                (GL_DOUBLEBUFFER, 1),
            ] {
                (api.SDL_GL_SetAttribute)(attr, value);
            }
            let title = c"Pley";
            let window = (api.SDL_CreateWindow)(
                title.as_ptr(),
                0,
                0,
                PLANE.0 as c_int,
                PLANE.1 as c_int,
                WINDOW_OPENGL | WINDOW_FULLSCREEN,
            );
            if window.is_null() {
                return Err(error(sdl, "SDL_CreateWindow"));
            }
            let context = (api.SDL_GL_CreateContext)(window);
            if context.is_null() {
                let e = error(sdl, "SDL_GL_CreateContext");
                (api.SDL_DestroyWindow)(window);
                return Err(e);
            }
            (api.SDL_GL_MakeCurrent)(window, context);
            (api.SDL_GL_SetSwapInterval)(1);
            let gl = glow::Context::from_loader_function_cstr(|name| (api.SDL_GL_GetProcAddress)(name.as_ptr()));
            let size = drawable_size(sdl, window);

            let mut panel = None;
            if let Some(get) = sdl.ext.panel_resolution {
                let (mut w, mut h) = (0, 0);
                if get(&mut w, &mut h) != 0 && w > 0 && h > 0 {
                    panel = Some((w as u32, h as u32));
                }
            }
            *PANEL.lock().unwrap_or_else(|e| e.into_inner()) = panel;

            let registered = (api.SDL_RegisterEvents)(1);
            let user_type = if registered == u32::MAX { events::kind::USER } else { registered };
            USER_TYPE.store(user_type, Ordering::Release);
            let wake_pending = Arc::new(AtomicBool::new(false));
            let pending = wake_pending.clone();
            let waker = Waker::new(move || {
                if !pending.swap(true, Ordering::AcqRel) && !push_user(events::user::WAKE) {
                    pending.store(false, Ordering::Release);
                }
            });

            let driver = text((api.SDL_GetCurrentVideoDriver)());
            crate::log::line(&format!(
                "sdl: driver {driver}, {}×{}, GL {} on {}, alpha bits {}, panel {}",
                size.0,
                size.1,
                gl.get_parameter_string(glow::VERSION),
                gl.get_parameter_string(glow::RENDERER),
                gl.get_parameter_i32(glow::ALPHA_BITS),
                panel.map_or("unknown".into(), |(w, h)| format!("{w}×{h}")),
            ));
            if let Err(why) = clear_opaque_region(sdl, window) {
                crate::log::line(&format!("sdl: the surface's opaque region stays: {why}"));
            }
            Ok(WebosPlatform {
                sdl,
                window,
                context,
                gl: Arc::new(gl),
                size,
                waker,
                wake_pending,
                user_type,
                translator: Translator::default(),
                event: Box::new(EventBuf::new()),
                quit: false,
                video: ExportedWindow::default(),
            })
        }
    }

    fn handle(&mut self, out: &mut Vec<PlatformEvent>) {
        let event = events::parse(&self.event.0, self.user_type);
        if event == SdlEvent::Quit || event == SdlEvent::Closed {
            self.quit = true;
        }
        match self.translator.translate(event, out) {
            Some(Effect::Woken) => self.wake_pending.store(false, Ordering::Release),
            Some(Effect::Resized) => {
                let size = drawable_size(self.sdl, self.window);
                if size != self.size && size.0 > 0 && size.1 > 0 {
                    self.size = size;
                    out.push(PlatformEvent::Resized(size.0, size.1));
                }
            }
            Some(Effect::Foregrounded) => {
                // SDL may have made a new surface while the app was away.
                if let Err(why) = clear_opaque_region(self.sdl, self.window) {
                    crate::log::line(&format!("sdl: the surface's opaque region stays: {why}"));
                }
            }
            // SAFETY: the window this platform made, on its thread.
            Some(Effect::Minimize) => unsafe { (self.sdl.api.SDL_MinimizeWindow)(self.window) },
            None => {}
        }
    }
}

/// Most events read in one `wait_events`, so a flood can't hold a frame back.
const MAX_EVENTS_PER_WAIT: usize = 256;
/// The longest SDL is waited on before GLib's default context is pumped again.
const WAIT_SLICE: Duration = Duration::from_millis(20);
/// Most sources of GLib's default context dispatched per slice (plx-native's bound).
const GLIB_DISPATCHES: usize = 8;

impl Platform for WebosPlatform {
    fn surface_size(&self) -> (u32, u32) {
        self.size
    }

    fn gl(&self) -> Arc<glow::Context> {
        self.gl.clone()
    }

    fn wait_events(&mut self, timeout: Option<Duration>, out: &mut Vec<PlatformEvent>) {
        if self.quit {
            if !out.contains(&PlatformEvent::Quit) {
                out.push(PlatformEvent::Quit);
            }
            return;
        }
        // The firmware's SDL (2.0.14 and older) waits by polling every 10 ms
        // anyway, so waiting in slices costs nothing more, and between slices
        // GLib's default context gets its turn.
        let deadline = timeout.and_then(|t| Instant::now().checked_add(t));
        let api = &self.sdl.api;
        loop {
            crate::luna::pump_default_context(GLIB_DISPATCHES);
            let left = deadline.map_or(WAIT_SLICE, |d| d.saturating_duration_since(Instant::now()).min(WAIT_SLICE));
            let ms = if left.is_zero() { 0 } else { left.as_millis().max(1) as c_int };
            // SAFETY: SDL writes one event into a buffer larger than SDL_Event.
            if unsafe { (api.SDL_WaitEventTimeout)(self.event.0.as_mut_ptr(), ms) } != 0 {
                break;
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                return;
            }
        }
        self.handle(out);
        for _ in 1..MAX_EVENTS_PER_WAIT {
            // SAFETY: as above.
            if unsafe { (api.SDL_PollEvent)(self.event.0.as_mut_ptr()) } == 0 {
                break;
            }
            self.handle(out);
        }
    }

    fn present(&mut self) {
        // SAFETY: the window this platform made, with its context current.
        unsafe { (self.sdl.api.SDL_GL_SwapWindow)(self.window) };
    }

    fn waker(&self) -> Waker {
        self.waker.clone()
    }

    fn set_pointer_visible(&mut self, visible: bool) {
        // SAFETY: LG's cursor call where it exists, SDL's otherwise.
        unsafe {
            match self.sdl.ext.cursor_visibility {
                Some(set) => {
                    set(visible as c_int);
                }
                None => {
                    (self.sdl.api.SDL_ShowCursor)(visible as c_int);
                }
            }
        }
    }

    fn video_window(&mut self) -> Option<&mut dyn VideoWindow> {
        if ExportedWindow::available() { Some(&mut self.video) } else { None }
    }

    fn quit(&mut self) {
        self.quit = true;
    }
}

impl Drop for WebosPlatform {
    fn drop(&mut self) {
        self.video.destroy();
        // SAFETY: the context and window this platform made, released once.
        unsafe {
            (self.sdl.api.SDL_GL_DeleteContext)(self.context);
            (self.sdl.api.SDL_DestroyWindow)(self.window);
            (self.sdl.api.SDL_Quit)();
        }
    }
}

fn drawable_size(sdl: &Sdl, window: *mut c_void) -> (u32, u32) {
    let (mut w, mut h) = (0, 0);
    // SAFETY: a live window and two valid outputs.
    unsafe { (sdl.api.SDL_GL_GetDrawableSize)(window, &mut w, &mut h) };
    if w > 0 && h > 0 { (w as u32, h as u32) } else { PLANE }
}

fn text(raw: *const c_char) -> String {
    if raw.is_null() {
        String::new()
    } else {
        // SAFETY: SDL returns NUL-terminated strings it keeps.
        unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned()
    }
}

fn error(sdl: &Sdl, what: &str) -> SdlError {
    // SAFETY: SDL_GetError has no preconditions.
    SdlError(format!("{what} failed: {}", text(unsafe { (sdl.api.SDL_GetError)() })))
}

/// `SDL_SysWMinfo`, larger than the headers say: LG's fork writes more.
#[repr(C, align(8))]
struct WmInfo([u8; 512]);

type Marshal = unsafe extern "C" fn(*mut c_void, u32, ...);

/// Clears the Wayland surface's opaque region so the compositor shows the
/// video plane under transparent pixels (RES/plx-media.md §4.4). The commit is
/// left to the next swap: a commit of the app's own presents a null buffer.
fn clear_opaque_region(sdl: &Sdl, window: *mut c_void) -> Result<(), &'static str> {
    static MARSHAL: OnceLock<Option<Marshal>> = OnceLock::new();
    let marshal = MARSHAL
        .get_or_init(|| {
            let lib = Lib::open_loaded(&["libwayland-client.so.0"])?;
            // SAFETY: libwayland-client's variadic `wl_proxy_marshal`.
            unsafe { lib.sym::<Marshal>("wl_proxy_marshal") }
        })
        .ok_or("libwayland-client isn't there")?;
    let mut info = WmInfo([0; 512]);
    // SAFETY: SDL_GetVersion fills the three version bytes at the start, so the
    // running SDL sees its own version (from 2.0.6 an older one gets nothing);
    // the buffer is larger than any SDL_SysWMinfo.
    let surface = unsafe {
        (sdl.api.SDL_GetVersion)(info.0.as_mut_ptr());
        if (sdl.api.SDL_GetWindowWMInfo)(window, info.0.as_mut_ptr()) == 0 {
            return Err("SDL_GetWindowWMInfo failed");
        }
        if u32::from_ne_bytes(info.0[4..8].try_into().expect("four bytes")) != SYSWM_WAYLAND {
            return Err("the window isn't a Wayland surface");
        }
        // The union starts at byte 8: the display, then the surface.
        let at = 8 + std::mem::size_of::<usize>();
        usize::from_ne_bytes(info.0[at..at + std::mem::size_of::<usize>()].try_into().expect("a pointer"))
            as *mut c_void
    };
    if surface.is_null() {
        return Err("no Wayland surface");
    }
    // SAFETY: the window's live wl_surface; set_opaque_region takes one object
    // argument, and null clears the region.
    unsafe { marshal(surface, WL_SURFACE_SET_OPAQUE_REGION, std::ptr::null_mut::<c_void>()) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_reaches_the_app() {
        assert!(HINTS.contains(&("SDL_WEBOS_ACCESS_POLICY_KEYS_BACK", "true")));
    }

    #[test]
    fn off_the_tv_there_is_no_window() {
        if sdl().is_none() {
            // SAFETY: nothing is set when SDL can't be opened.
            assert!(unsafe { WebosPlatform::new() }.is_err());
            assert!(!request_minimize());
            assert_eq!(panel_resolution(), None);
        }
    }
}
