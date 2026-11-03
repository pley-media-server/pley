//! The simulator's TV and the fake server: a [`Device`] over pley-core's fake TV that leaves the
//! window when the app leaves, with a folder for a USB drive (`--drive`), and `--fixtures`'
//! sign-in through the device code, as a TV does.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use pley_base::{Tier, Waker};
use pley_core::auth::{DeviceSignIn, LinkEnd, LinkState, Tokens};
use pley_core::{
    Api, Cancel, CapsOverrides, CapsReport, Device, DeviceEvent, DeviceInfo, Drive, DriveSpace, FakeDevice, Http,
};
use pley_core::{PlayerEngine, Store};
use pley_fixtures::{FixtureServer, Options};
use pley_player::{SimClock, SimEngine};

/// The simulator's TV: a 2022 LG OLED's facts and capabilities, its folders under the data
/// folder, the tier asked for, and leaving that closes the window.
pub struct SimDevice {
    inner: FakeDevice,
    version: &'static str,
    tier: Tier,
    left: Arc<AtomicBool>,
    waker: Mutex<Option<Waker>>,
    /// The folder that stands in for a USB drive (`--drive`).
    drive: Mutex<Option<PathBuf>>,
}

impl SimDevice {
    /// A TV running the app's `version`, keeping its data under `root`. Its player is
    /// pley-player's [`SimEngine`] on the app's clock: no picture, but positions, seeks, the rate
    /// and the end as the TV's.
    pub fn new(root: PathBuf, tier: Option<Tier>, version: &'static str) -> SimDevice {
        let inner =
            FakeDevice::in_dir(root, || Box::new(SimEngine::new(SimClock::system()).0) as Box<dyn PlayerEngine>);
        let tier = tier.unwrap_or_else(|| inner.tier());
        SimDevice {
            inner,
            version,
            tier,
            left: Arc::new(AtomicBool::new(false)),
            waker: Mutex::new(None),
            drive: Mutex::new(None),
        }
    }

    /// Wakes `waker` when the app leaves.
    pub fn set_waker(&self, waker: Waker) {
        *self.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(waker);
    }

    /// Makes `dir` the USB drive, plugged in.
    pub fn set_drive_folder(&self, dir: PathBuf) {
        *self.drive.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir);
        self.plug(true);
    }

    /// Plugs the drive in or pulls it out (the script's `drive:in` and `drive:out`); nothing
    /// without `--drive`.
    pub fn plug(&self, on: bool) {
        let dir = self.drive.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(root) = dir else { return };
        self.inner.set_drive(on.then_some(Drive { root, hidden: false }));
        if let Some(w) = self.waker.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            w.wake();
        }
    }

    /// Set once the app leaves.
    pub fn left(&self) -> Arc<AtomicBool> {
        self.left.clone()
    }
}

impl Device for SimDevice {
    fn info(&self) -> DeviceInfo {
        DeviceInfo { app_version: self.version.into(), ..self.inner.info() }
    }

    fn data_dir(&self) -> PathBuf {
        self.inner.data_dir()
    }

    fn cache_dir(&self) -> PathBuf {
        self.inner.cache_dir()
    }

    fn font_dirs(&self) -> Vec<PathBuf> {
        self.inner.font_dirs()
    }

    fn tier(&self) -> Tier {
        self.tier
    }

    fn caps(&self, overrides: &CapsOverrides) -> CapsReport {
        self.inner.caps(overrides)
    }

    fn local_ipv4(&self) -> Option<Ipv4Addr> {
        self.inner.local_ipv4()
    }

    fn clock_24h(&self) -> bool {
        self.inner.clock_24h()
    }

    fn screen_reader(&self) -> bool {
        self.inner.screen_reader()
    }

    fn keep_awake(&self, on: bool) {
        self.inner.keep_awake(on)
    }

    fn exit(&self) {
        self.left.store(true, Ordering::SeqCst);
        if let Some(w) = self.waker.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            w.wake();
        }
    }

    fn engine(&self) -> Box<dyn PlayerEngine> {
        self.inner.engine()
    }

    fn subscribe(&self, tx: Sender<DeviceEvent>) {
        self.inner.subscribe(tx)
    }

    fn drive(&self) -> Option<Drive> {
        self.inner.drive()
    }

    /// The folder's file system's, as the TV says of a drive (none on Windows: the caption stays
    /// empty).
    fn drive_space(&self, root: &Path) -> Option<DriveSpace> {
        self.drive().filter(|d| d.root == root)?;
        space(root)
    }

    fn drive_name(&self, root: &Path) -> Option<String> {
        self.inner.drive_name(root)
    }
}

#[cfg(unix)]
fn space(dir: &Path) -> Option<DriveSpace> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: a zeroed statvfs is a valid output buffer for statvfs(3).
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: a NUL-terminated path and a valid buffer.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let unit = stat.f_frsize as u64;
    Some(DriveSpace {
        free: (stat.f_bavail as u64).saturating_mul(unit),
        total: (stat.f_blocks as u64).saturating_mul(unit),
    })
}

#[cfg(not(unix))]
fn space(_dir: &Path) -> Option<DriveSpace> {
    None
}

/// Starts the fake server and signs in to it as a TV does: a device code, approved at once by the
/// fake household, polled for the tokens, which are kept in `store`; the server becomes the one in
/// use. The server must live as long as the app.
pub fn sign_in_to_fixtures(store: &Store, http: &Http) -> Result<FixtureServer, String> {
    // Tokens kept from an earlier run sign in again; the device grant asks for polls a second apart.
    let options = Options { adopt_unknown_tokens: true, device_interval_secs: 1, ..Options::default() };
    let server = FixtureServer::start_with(options).map_err(|e| format!("the fake server didn't start: {e}"))?;
    let url = server.url().to_string();
    log::info!("the fake server listens at {url}");

    let mut scope = String::new();
    let end = DeviceSignIn::new(http.clone()).run(
        &url,
        None,
        &mut |state| {
            if let LinkState::Code(code) = &state {
                scope = code.scope.clone();
            }
        },
        &Cancel::new(),
    );
    let tokens = match end {
        LinkEnd::SignedIn(tokens) => tokens,
        other => return Err(format!("the fake server's sign-in ended: {other:?}")),
    };
    let issuer = tokens.issuer.clone();
    Tokens::shared(&issuer, store, http).install(tokens, &scope).map_err(|e| format!("tokens not kept: {e}"))?;
    let name = Api::new(http.clone(), &issuer, Tokens::shared(&issuer, store, http))
        .reachable(std::time::Duration::from_secs(5))
        .map(|info| info.name)
        .unwrap_or_else(|_| "Pley".into());
    store.set_active_server(Some(&issuer)).map_err(|e| e.to_string())?;
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    store.remember_server(&issuer, &name, None, now_ms).map_err(|e| e.to_string())?;
    log::info!("signed in to the fake server");
    Ok(server)
}
