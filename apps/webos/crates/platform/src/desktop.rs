//! The simulator's window on Windows, macOS and Linux: winit for the window and
//! input, glutin for a GL context that runs the TV's GLSL ES 1.00 shaders
//! (desktop OpenGL 2.1, or GLES 2.0 through EGL where that is what the system
//! offers).
//!
//! Keys follow the simulator's map: arrows, Enter (OK), Escape or
//! Backspace (Back), M or F1 (menu), I (info), F2 to F5 (the colour keys),
//! Space (play/pause), `,` and `.` (rewind, fast forward), Page Up and Page
//! Down (CH+ and CH−), digits; the mouse is the Magic Remote.

use std::error::Error;
use std::ffi::CStr;
use std::fmt;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use pley_base::{Key, KeyEvent, Point, PointerEvent, Waker};
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};
use winit::platform::scancode::PhysicalKeyExtScancode;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::{Platform, PlatformEvent, VideoWindow};

/// How the simulator's window is opened.
#[derive(Clone, Debug)]
pub struct DesktopOptions {
    /// The drawable's size in device pixels (1280×720 unless asked otherwise).
    pub width: u32,
    pub height: u32,
    pub title: String,
    /// Wait for the display's refresh on present (off only to measure).
    pub vsync: bool,
}

impl Default for DesktopOptions {
    fn default() -> DesktopOptions {
        DesktopOptions { width: 1280, height: 720, title: "Pley".into(), vsync: true }
    }
}

/// Why the window or its GL context couldn't be made.
#[derive(Debug)]
pub struct DesktopError(String);

impl fmt::Display for DesktopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DesktopError {}

impl DesktopError {
    fn new(what: &str, err: impl fmt::Display) -> DesktopError {
        DesktopError(format!("{what}: {err}"))
    }
}

/// The user event a [`Waker`] sends to interrupt a wait.
#[derive(Clone, Copy, Debug)]
struct Wake;

/// The desktop backend of [`Platform`].
pub struct DesktopPlatform {
    event_loop: EventLoop<Wake>,
    state: State,
    gl: Arc<glow::Context>,
    waker: Waker,
}

/// What the event handler owns while winit pumps events into it.
struct State {
    window: Window,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    size: (u32, u32),
    events: Vec<PlatformEvent>,
    wake_pending: Arc<AtomicBool>,
    quit: bool,
}

impl DesktopPlatform {
    /// Opens the window and makes its GL context current on this thread.
    pub fn new(options: &DesktopOptions) -> Result<DesktopPlatform, DesktopError> {
        let event_loop =
            EventLoop::<Wake>::with_user_event().build().map_err(|e| DesktopError::new("event loop", e))?;
        let attributes = WindowAttributes::default()
            .with_title(options.title.clone())
            .with_inner_size(PhysicalSize::new(options.width.max(1), options.height.max(1)));
        // An alpha channel like the TV's surface, and no depth or stencil
        // buffer: the renderer uses neither.
        let template = ConfigTemplateBuilder::new().with_alpha_size(8).with_depth_size(0).with_stencil_size(0);
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(&event_loop, template, pick_config)
            .map_err(|e| DesktopError::new("GL display", e))?;
        let window = window.ok_or_else(|| DesktopError("no window".into()))?;
        let handle = window.window_handle().map_err(|e| DesktopError::new("window handle", e))?.as_raw();

        let display = config.display();
        // Desktop OpenGL 2.1 runs GLSL 1.20, which the renderer's shaders compile
        // as with a prelude; GLES 2.0 runs them as they are.
        let attempts = [
            ContextAttributesBuilder::new()
                .with_context_api(ContextApi::OpenGl(Some(Version::new(2, 1))))
                .build(Some(handle)),
            ContextAttributesBuilder::new()
                .with_context_api(ContextApi::Gles(Some(Version::new(2, 0))))
                .build(Some(handle)),
            ContextAttributesBuilder::new().build(Some(handle)),
        ];
        let mut context = None;
        let mut last_error = String::new();
        for attempt in &attempts {
            // SAFETY: the window handle stays valid: the window lives in `State`
            // as long as the context does.
            match unsafe { display.create_context(&config, attempt) } {
                Ok(c) => {
                    context = Some(c);
                    break;
                }
                Err(e) => last_error = e.to_string(),
            }
        }
        let context = context.ok_or_else(|| DesktopError::new("GL context", last_error))?;
        let surface_attributes =
            window.build_surface_attributes(Default::default()).map_err(|e| DesktopError::new("GL surface", e))?;
        // SAFETY: as above, the window outlives the surface.
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .map_err(|e| DesktopError::new("GL surface", e))?;
        let context = context.make_current(&surface).map_err(|e| DesktopError::new("make current", e))?;
        let interval = if options.vsync { SwapInterval::Wait(NonZeroU32::MIN) } else { SwapInterval::DontWait };
        // Some drivers refuse to change the interval; the frame loop still works.
        let _ = surface.set_swap_interval(&context, interval);

        // SAFETY: the context is current on this thread; glutin returns the
        // driver's entry points for it.
        let gl =
            unsafe { glow::Context::from_loader_function_cstr(|name: &CStr| display.get_proc_address(name).cast()) };
        let size = window.inner_size();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let waker = make_waker(event_loop.create_proxy(), wake_pending.clone());
        let state = State {
            window,
            surface,
            context,
            size: (size.width, size.height),
            events: Vec::new(),
            wake_pending,
            quit: false,
        };
        Ok(DesktopPlatform { event_loop, state, gl: Arc::new(gl), waker })
    }

    /// The window, for what the trait doesn't cover (its title, focus).
    pub fn window(&self) -> &Window {
        &self.state.window
    }
}

/// Prefers a configuration with 8 alpha bits and no multisampling.
fn pick_config(configs: Box<dyn Iterator<Item = Config> + '_>) -> Config {
    configs
        .min_by_key(|c| (c.alpha_size() != 8, c.num_samples(), c.depth_size(), c.stencil_size()))
        .expect("the display offers no GL configuration")
}

/// A waker that sends at most one user event until the loop has taken it.
fn make_waker(proxy: EventLoopProxy<Wake>, pending: Arc<AtomicBool>) -> Waker {
    let proxy = Mutex::new(proxy);
    Waker::new(move || {
        if !pending.swap(true, Ordering::AcqRel)
            && let Ok(proxy) = proxy.lock()
        {
            let _ = proxy.send_event(Wake);
        }
    })
}

impl Platform for DesktopPlatform {
    fn surface_size(&self) -> (u32, u32) {
        self.state.size
    }

    fn gl(&self) -> Arc<glow::Context> {
        self.gl.clone()
    }

    fn wait_events(&mut self, timeout: Option<Duration>, out: &mut Vec<PlatformEvent>) {
        let timeout = if self.state.quit || !self.state.events.is_empty() { Some(Duration::ZERO) } else { timeout };
        if let PumpStatus::Exit(_) = self.event_loop.pump_app_events(timeout, &mut self.state) {
            self.state.quit = true;
        }
        out.append(&mut self.state.events);
        if self.state.quit && !out.contains(&PlatformEvent::Quit) {
            out.push(PlatformEvent::Quit);
        }
    }

    fn present(&mut self) {
        if let Err(e) = self.state.surface.swap_buffers(&self.state.context) {
            eprintln!("pley: present failed: {e}");
        }
    }

    fn waker(&self) -> Waker {
        self.waker.clone()
    }

    fn set_pointer_visible(&mut self, visible: bool) {
        self.state.window.set_cursor_visible(visible);
    }

    fn video_window(&mut self) -> Option<&mut dyn VideoWindow> {
        None
    }

    fn quit(&mut self) {
        self.state.quit = true;
        self.state.window.set_visible(false);
    }
}

impl ApplicationHandler<Wake> for State {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: Wake) {
        self.wake_pending.store(false, Ordering::Release);
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.quit = true;
                self.events.push(PlatformEvent::Quit);
            }
            WindowEvent::Resized(size) => {
                // A minimised window reports 0×0; keep drawing at the last size.
                if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                    self.surface.resize(&self.context, w, h);
                    self.size = (size.width, size.height);
                    self.events.push(PlatformEvent::Resized(size.width, size.height));
                }
            }
            WindowEvent::Occluded(occluded) => self.events.push(PlatformEvent::Visible(!occluded)),
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let key = map_key(&event.logical_key);
                let code = event.physical_key.to_scancode().map_or(0, |c| c as i32);
                self.events.push(PlatformEvent::Key(KeyEvent { key, pressed, repeat: event.repeat, code }));
                if pressed && let Some(text) = event.text.as_deref().and_then(printable) {
                    self.events.push(PlatformEvent::Text(text));
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = Point::new(position.x as f32, position.y as f32);
                self.events.push(PlatformEvent::Pointer(PointerEvent::Moved(at)));
            }
            WindowEvent::CursorLeft { .. } => self.events.push(PlatformEvent::Pointer(PointerEvent::Hidden)),
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let pressed = state == ElementState::Pressed;
                self.events.push(PlatformEvent::Pointer(PointerEvent::Button { pressed }));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = wheel_notches(delta);
                if notches != 0.0 {
                    self.events.push(PlatformEvent::Pointer(PointerEvent::Wheel(notches)));
                }
            }
            _ => {}
        }
    }
}

/// The remote key a keyboard key stands for in the simulator.
pub fn map_key(key: &WinitKey) -> Key {
    match key {
        WinitKey::Named(named) => match named {
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::Enter => Key::Ok,
            NamedKey::Escape | NamedKey::Backspace | NamedKey::BrowserBack => Key::Back,
            NamedKey::F1 | NamedKey::ContextMenu => Key::Menu,
            NamedKey::F2 => Key::Red,
            NamedKey::F3 => Key::Green,
            NamedKey::F4 => Key::Yellow,
            NamedKey::F5 => Key::Blue,
            NamedKey::Space | NamedKey::MediaPlayPause => Key::PlayPause,
            NamedKey::MediaPlay => Key::Play,
            NamedKey::MediaPause => Key::Pause,
            NamedKey::MediaStop => Key::Stop,
            NamedKey::MediaFastForward => Key::FastForward,
            NamedKey::MediaRewind => Key::Rewind,
            NamedKey::MediaTrackNext => Key::Next,
            NamedKey::MediaTrackPrevious => Key::Previous,
            NamedKey::PageUp => Key::ChannelUp,
            NamedKey::PageDown => Key::ChannelDown,
            _ => Key::Unknown,
        },
        WinitKey::Character(text) => {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => map_char(c),
                _ => Key::Unknown,
            }
        }
        _ => Key::Unknown,
    }
}

fn map_char(c: char) -> Key {
    match c.to_ascii_lowercase() {
        'm' => Key::Menu,
        'i' => Key::Info,
        ',' => Key::Rewind,
        '.' => Key::FastForward,
        ' ' => Key::PlayPause,
        d @ '0'..='9' => Key::Digit(d as u8 - b'0'),
        _ => Key::Unknown,
    }
}

/// Typed text without control characters (Enter, Backspace and Escape type
/// those, and they are keys, not text).
fn printable(text: &str) -> Option<String> {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    (!text.is_empty()).then_some(text)
}

/// Wheel turns in notches, positive away from the viewer. Touchpads report
/// pixels; about 100 of them make a notch on every system.
fn wheel_notches(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(p) => (p.y / 100.0) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(s: &str) -> WinitKey {
        WinitKey::Character(s.into())
    }

    #[test]
    fn simulator_keys_follow_the_design() {
        assert_eq!(map_key(&WinitKey::Named(NamedKey::ArrowLeft)), Key::Left);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Enter)), Key::Ok);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Escape)), Key::Back);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Backspace)), Key::Back);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::F1)), Key::Menu);
        assert_eq!(map_key(&ch("M")), Key::Menu);
        assert_eq!(map_key(&ch("i")), Key::Info);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::F2)), Key::Red);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::F5)), Key::Blue);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Space)), Key::PlayPause);
        assert_eq!(map_key(&ch(",")), Key::Rewind);
        assert_eq!(map_key(&ch(".")), Key::FastForward);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::PageUp)), Key::ChannelUp);
        assert_eq!(map_key(&WinitKey::Named(NamedKey::PageDown)), Key::ChannelDown);
        assert_eq!(map_key(&ch("7")), Key::Digit(7));
        assert_eq!(map_key(&ch("q")), Key::Unknown);
        assert_eq!(map_key(&ch("ab")), Key::Unknown);
    }

    #[test]
    fn only_printable_text_is_typed() {
        assert_eq!(printable("a"), Some("a".into()));
        assert_eq!(printable("\r"), None);
        assert_eq!(printable("\u{8}"), None);
        assert_eq!(printable("é"), Some("é".into()));
    }

    #[test]
    fn wheel_counts_notches() {
        assert_eq!(wheel_notches(MouseScrollDelta::LineDelta(0.0, 1.0)), 1.0);
        let pixels = MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, -200.0));
        assert_eq!(wheel_notches(pixels), -2.0);
    }
}
