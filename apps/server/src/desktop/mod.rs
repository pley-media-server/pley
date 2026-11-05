//! Desktop integration: a tray icon while pley runs in an interactive desktop
//! session (Windows notification area, macOS menu bar, Linux
//! StatusNotifierItem), and starting pley at login.
//!
//! The tray is a small remote control for the server: it opens the web UI,
//! starts library scans, shows the logs, toggles start-at-login and quits. It
//! never appears for services, containers or SSH sessions, and `--no-tray`
//! turns it off.

pub mod autostart;
#[cfg(all(feature = "tray", target_os = "linux"))]
mod linux;
#[cfg(all(feature = "tray", target_os = "macos"))]
pub mod macos;
#[cfg(all(feature = "tray", windows))]
mod windows;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use tokio::sync::mpsc;

use crate::i18n;
use crate::state::AppState;

/// Starts a tray icon for a running server; `None` when there is none.
pub type TrayLauncher = Box<dyn FnOnce(TrayContext) -> Option<TrayGuard> + Send>;

/// A running tray icon. Dropping it removes the icon.
pub struct TrayGuard(#[allow(dead_code)] Box<dyn Send>);

/// What the tray asks the server to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    ScanLibraries,
    Quit,
}

/// Menu item ids of the `tray-icon` based implementations.
#[cfg(all(feature = "tray", any(windows, target_os = "macos")))]
mod item {
    pub const OPEN: &str = "open";
    pub const SCAN: &str = "scan";
    pub const LOGS: &str = "logs";
    pub const AUTOSTART: &str = "autostart";
    pub const QUIT: &str = "quit";
}

/// Everything a tray implementation needs. Cheap to clone, usable from any thread.
#[derive(Clone)]
pub struct TrayContext {
    /// Address of the web UI, e.g. `http://localhost:7539`.
    pub url: String,
    pub logs_dir: PathBuf,
    commands: mpsc::UnboundedSender<Command>,
    /// One line about what the server is doing, refreshed every few seconds.
    status: Arc<RwLock<String>>,
    /// For the setup code, while the server has no account.
    state: AppState,
}

impl TrayContext {
    /// Wire tray commands to the server listening on `addr` and keep the
    /// status line fresh. Must be called inside the server's runtime.
    pub fn new(state: &AppState, addr: SocketAddr) -> Self {
        let (commands, mut rx) = mpsc::unbounded_channel();
        let worker = state.clone();
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                match command {
                    Command::ScanLibraries => {
                        if let Err(err) = worker.scanner.enqueue_all(false).await {
                            tracing::warn!("could not start a scan: {err}");
                        }
                    }
                    // Stops this server (see `crate::run_with_tray`).
                    Command::Quit => worker.stopping.cancel(),
                }
            }
        });
        let status = Arc::new(RwLock::new(status_line(state)));
        let (refresh, monitored) = (status.clone(), state.clone());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(3));
            loop {
                tick.tick().await;
                let line = status_line(&monitored);
                if *refresh.read() != line {
                    *refresh.write() = line;
                }
            }
        });
        Self {
            url: web_url(addr),
            logs_dir: state.config.log_file().parent().map(PathBuf::from).unwrap_or_default(),
            commands,
            status,
            state: state.clone(),
        }
    }

    pub fn status(&self) -> String {
        self.status.read().clone()
    }

    /// A menu item's words (`tray.<key>` in `locales/`), with the `&`
    /// before the letter Windows underlines; [`TrayContext::plain_label`]
    /// without it.
    #[cfg_attr(not(feature = "tray"), allow(dead_code))]
    pub(crate) fn label(&self, key: &str) -> String {
        i18n::t(language(&self.state), &format!("tray.{key}"))
    }

    #[cfg_attr(not(all(feature = "tray", unix)), allow(dead_code))]
    pub(crate) fn plain_label(&self, key: &str) -> String {
        self.label(key).replace('&', "")
    }

    /// Tooltip / hover text.
    pub fn tooltip(&self) -> String {
        format!("Pley: {}", self.status())
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// Opens the web app in the browser, where it listens now (Settings →
    /// Network may have moved it): its setup page with the setup code filled
    /// in, while the server has no account.
    pub(crate) fn open_web_ui(&self) {
        let url = self.state.network.web_url().unwrap_or_else(|| self.url.clone());
        match self.state.auth.setup.shown() {
            Some(code) => open(format!("{url}/setup?code={code}")),
            None => open(&url),
        }
    }

    /// A new server on a desktop: straight to setting it up, code and all.
    /// Only the real trays call this, so servers started by tests don't
    /// open the browser.
    #[cfg_attr(not(feature = "tray"), allow(dead_code))]
    fn open_setup_if_new(&self) {
        if self.state.auth.setup.shown().is_some() {
            self.open_web_ui();
        }
    }

    #[cfg_attr(not(feature = "tray"), allow(dead_code))]
    fn open_logs(&self) {
        open(&self.logs_dir);
    }

    /// Flip start-at-login; returns the new state, or the old one on failure.
    #[cfg_attr(not(feature = "tray"), allow(dead_code))]
    fn toggle_autostart(&self, enable: bool) -> bool {
        match autostart::set_enabled(enable) {
            Ok(()) => enable,
            Err(err) => {
                tracing::warn!("could not change start at login: {err:#}");
                !enable
            }
        }
    }
}

/// Start the tray for this platform (Windows, Linux). macOS runs its menu bar
/// item on the main thread instead, see [`macos::serve`].
pub fn start_tray(ctx: TrayContext) -> Option<TrayGuard> {
    ctx.open_setup_if_new();
    #[cfg(all(feature = "tray", windows))]
    return windows::start(ctx);
    #[cfg(all(feature = "tray", target_os = "linux"))]
    return linux::start(ctx);
    #[allow(unreachable_code)]
    {
        let _ = ctx;
        None
    }
}

/// Whether this process runs in a desktop session where a tray icon makes
/// sense: not a Windows service, not a container or SSH session.
pub fn interactive_session() -> bool {
    if !cfg!(feature = "tray") {
        return false;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        let mut session = 0u32;
        // Services run in session 0, users in 1 and up.
        // SAFETY: plain query with a valid out pointer.
        let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } != 0;
        ok && session != 0
    }
    #[cfg(target_os = "macos")]
    {
        let var = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        !var("SSH_CONNECTION") && !var("SSH_TTY") && std::env::var("USER").is_ok_and(|u| u != "root")
    }
    #[cfg(target_os = "linux")]
    {
        let var = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
        (var("DISPLAY") || var("WAYLAND_DISPLAY")) && var("DBUS_SESSION_BUS_ADDRESS")
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    false
}

/// The language of the tray: the server's (Settings → General), else the
/// desktop's, else English.
pub(crate) fn language(state: &AppState) -> &'static str {
    if let Some(language) = i18n::supported(&state.settings.get().language) {
        return language;
    }
    #[cfg(feature = "tray")]
    if let Some(language) = sys_locale::get_locales().find_map(|locale| i18n::supported(&locale)) {
        return language;
    }
    "en"
}

/// "2 streams (1 transcoding), scanning Movies" or "idle".
fn status_line(state: &AppState) -> String {
    let language = language(state);
    let t = |key: &str| i18n::t(language, &format!("tray.status.{key}"));
    if state.stopping.is_cancelled() {
        return t("stopping");
    }
    let playing = state.sessions.active().len();
    let transcoding = state.sessions.active_transcodes();
    let mut parts = Vec::new();
    if playing > 0 {
        parts.push(i18n::t_count(language, "tray.status.streams", playing as u64, &[]));
    }
    if transcoding > 0 {
        let streams = parts.pop().unwrap_or_default();
        let count = transcoding.to_string();
        let line = i18n::t_with(language, "tray.status.transcoding", &[("streams", &streams), ("count", &count)]);
        parts.push(line.trim().to_owned());
    }
    if let Some(scan) = state.scanner.statuses().first() {
        parts.push(if scan.library_name.is_empty() {
            t("scanningLibraries")
        } else {
            i18n::t_with(language, "tray.status.scanning", &[("library", &scan.library_name)])
        });
    }
    if parts.is_empty() { t("idle") } else { parts.join(", ") }
}

pub(crate) fn web_url(bind: SocketAddr) -> String {
    let ip = bind.ip();
    let host = if ip.is_unspecified() || ip.is_loopback() {
        "localhost".to_owned()
    } else if ip.is_ipv6() {
        format!("[{ip}]")
    } else {
        ip.to_string()
    };
    format!("http://{host}:{}", bind.port())
}

pub(crate) fn open(target: impl AsRef<std::ffi::OsStr>) {
    #[cfg(feature = "tray")]
    if let Err(err) = open::that_detached(target.as_ref()) {
        tracing::warn!("could not open {}: {err}", target.as_ref().to_string_lossy());
    }
    #[cfg(not(feature = "tray"))]
    let _ = target;
}

/// Waits for `thread` to end, for at most `timeout`: whether it did. One
/// that doesn't is left running (the end of the process stops it) rather
/// than holding up the shutdown.
#[cfg_attr(not(all(feature = "tray", windows)), allow(dead_code))]
pub(crate) fn join_within(thread: JoinHandle<()>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while !thread.is_finished() {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = thread.join();
    true
}

/// Decode an embedded PNG into straight RGBA pixels.
#[cfg_attr(not(feature = "tray"), allow(dead_code))]
fn rgba(png: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some((image.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_for_bind_addresses() {
        assert_eq!(web_url("0.0.0.0:7539".parse().unwrap()), "http://localhost:7539");
        assert_eq!(web_url("127.0.0.1:8000".parse().unwrap()), "http://localhost:8000");
        assert_eq!(web_url("192.168.1.5:7539".parse().unwrap()), "http://192.168.1.5:7539");
        assert_eq!(web_url("[fe80::1]:7539".parse().unwrap()), "http://[fe80::1]:7539");
    }

    #[test]
    fn a_thread_that_does_not_end_does_not_hold_up_the_shutdown() {
        let quick = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(20)));
        assert!(join_within(quick, Duration::from_secs(5)));

        let (release, held) = std::sync::mpsc::channel::<()>();
        let stuck = std::thread::spawn(move || {
            let _ = held.recv();
        });
        let started = Instant::now();
        assert!(!join_within(stuck, Duration::from_millis(100)));
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        drop(release);
    }

    #[test]
    fn embedded_icons_decode() {
        let (pixels, width, height) = rgba(include_bytes!("../../assets/tray-32.png")).unwrap();
        assert_eq!((width, height), (32, 32));
        assert_eq!(pixels.len(), 32 * 32 * 4);
        // Rounded corners are transparent.
        assert_eq!(pixels[3], 0);
    }
}
