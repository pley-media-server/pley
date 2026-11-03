//! pley-core's `Device` for an LG TV: what the TV is and plays, its folders,
//! its network address, its clock and Audio Guidance, the USB drive plugged
//! into it, the screen saver, leaving the app, and the player engine.
//!
//! Every method is quick. The facts are read once at start on a thread of
//! their own (nyx's files, the config, settings and ARC services); Audio
//! Guidance and the network are luna subscriptions whose news goes to
//! subscribers, and so is the drive, which a thread of its own follows
//! (`drives.rs`).

use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

use pley_base::{Tier, Waker};
use pley_core::device::{CapsOverrides, CapsReport, Device, DeviceEvent, DeviceInfo, Drive, DriveSpace, user_agent};
use pley_core::media::PlayerEngine;
use serde_json::{Value, json};

use crate::caps::{tier_of, webos_caps};
use crate::data::{self, DataDir};
use crate::facts::{WebosFacts, read_sources};
use crate::luna::{Luna, Subscription};
use crate::ums::{UmsConfig, UmsEngine};
use crate::{APP_ID, sdl};

/// The most a caller waits for the facts the first time (they take tens of
/// milliseconds; a TV whose services don't answer gets the safe defaults).
const FACTS_WAIT: Duration = Duration::from_secs(2);
/// How long a subscription's first answer may take.
const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long showing the TV's Home may take before the app hides itself instead.
const HOME_TIMEOUT: Duration = Duration::from_secs(1);
/// Back pressed again within this long doesn't ask the TV twice.
const EXIT_COOLDOWN: Duration = Duration::from_secs(2);
/// The system fonts, for subtitles in scripts the app's own fonts lack.
const SYSTEM_FONTS: &str = "/usr/share/fonts";

/// The TV.
#[derive(Clone)]
pub struct WebosDevice {
    inner: Arc<Inner>,
}

struct Inner {
    /// Where the TV's files are: `/`, a folder of samples in tests.
    root: PathBuf,
    luna: Option<Luna>,
    waker: Waker,
    app_version: String,
    app_id: String,
    app_dir: PathBuf,
    data: DataDir,
    facts: Mutex<Option<WebosFacts>>,
    facts_ready: Condvar,
    waited: AtomicBool,
    screen_reader: AtomicBool,
    network: Mutex<Option<(bool, Option<Ipv4Addr>)>>,
    drive: Mutex<Option<Drive>>,
    awake: AtomicBool,
    subscribers: Mutex<Vec<Sender<DeviceEvent>>>,
    subscriptions: Mutex<Vec<Subscription>>,
    screen_saver: Mutex<Option<Subscription>>,
    last_exit: Mutex<Option<Instant>>,
}

impl WebosDevice {
    /// The TV this app runs on: its data folder probed, its facts being read,
    /// its news subscribed to. `waker` wakes the main loop when there is news
    /// (and for the engine's).
    pub fn new(luna: Option<Luna>, waker: Waker, app_version: &str) -> WebosDevice {
        WebosDevice::in_root(Path::new("/"), luna, waker, app_version)
    }

    /// The same with the TV's files under `root` (tests use a folder of samples).
    pub fn in_root(root: &Path, luna: Option<Luna>, waker: Waker, app_version: &str) -> WebosDevice {
        let app_dir = data::app_dir().unwrap_or_else(|| root.to_path_buf());
        let app_id = data::app_id_of(&app_dir, APP_ID);
        let data = data::probe(root, &app_id).unwrap_or_else(|| {
            crate::log::line("device: no folder takes the app's data; nothing will be kept");
            DataDir { path: std::env::temp_dir().join(&app_id), durable: false }
        });
        crate::log::line(&format!(
            "device: {} from {}, data {} ({})",
            app_id,
            app_dir.display(),
            data.path.display(),
            if data.durable { "kept" } else { "lost at restart" }
        ));
        let device = WebosDevice {
            inner: Arc::new(Inner {
                root: root.to_path_buf(),
                luna,
                waker,
                app_version: app_version.into(),
                app_id,
                app_dir,
                data,
                facts: Mutex::new(None),
                facts_ready: Condvar::new(),
                waited: AtomicBool::new(false),
                screen_reader: AtomicBool::new(false),
                network: Mutex::new(None),
                drive: Mutex::new(None),
                awake: AtomicBool::new(false),
                subscribers: Mutex::new(Vec::new()),
                subscriptions: Mutex::new(Vec::new()),
                screen_saver: Mutex::new(None),
                last_exit: Mutex::new(None),
            }),
        };
        device.read_facts(root.to_path_buf());
        device.subscribe_news();
        device.watch_drives(root);
        device
    }

    /// Shared, as the app holds its device.
    pub fn shared(self) -> Arc<dyn Device> {
        Arc::new(self)
    }

    /// The facts (waiting a little the first time, while they are read).
    pub fn facts(&self) -> WebosFacts {
        let mut facts = self.inner.facts.lock().unwrap_or_else(|e| e.into_inner());
        // Only the first caller waits: after that the safe defaults stand in
        // until the facts come.
        if facts.is_none() && !self.inner.waited.swap(true, Ordering::SeqCst) {
            facts = self
                .inner
                .facts_ready
                .wait_timeout_while(facts, FACTS_WAIT, |f| f.is_none())
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        facts.clone().unwrap_or_default()
    }

    /// The app's install folder (its assets are there).
    pub fn app_dir(&self) -> &Path {
        &self.inner.app_dir
    }

    /// The id SAM launched the app with.
    pub fn app_id(&self) -> &str {
        &self.inner.app_id
    }

    /// The data folder and whether it survives a restart.
    pub fn data(&self) -> &DataDir {
        &self.inner.data
    }

    /// The picture cache's byte budget (half the free space, at most 128 MiB,
    /// none below 8 MiB).
    pub fn cache_budget(&self) -> u64 {
        data::cache_budget(&self.inner.data.path)
    }

    /// The luna handle, for anything else that talks to the TV's services.
    pub fn luna(&self) -> Option<&Luna> {
        self.inner.luna.as_ref()
    }

    fn read_facts(&self, root: PathBuf) {
        let inner = self.inner.clone();
        let spawned = std::thread::Builder::new().name("pley-facts".into()).spawn(move || {
            let facts = WebosFacts::from_sources(&read_sources(&root, inner.luna.as_ref(), sdl::panel_resolution()));
            crate::log::line(&format!("device: {}", facts.identity()));
            *inner.facts.lock().unwrap_or_else(|e| e.into_inner()) = Some(facts);
            inner.facts_ready.notify_all();
            inner.waker.wake();
        });
        if spawned.is_err() {
            *self.inner.facts.lock().unwrap_or_else(|e| e.into_inner()) = Some(WebosFacts::default());
        }
    }

    fn subscribe_news(&self) {
        let Some(luna) = &self.inner.luna else { return };
        let weak = Arc::downgrade(&self.inner);
        let guidance = luna.subscribe(
            "luna://com.webos.settingsservice/getSystemSettings",
            &json!({ "category": "option", "keys": ["audioGuidance"] }),
            SUBSCRIBE_TIMEOUT,
            move |answer| {
                let on = answer.ok().and_then(|v| audio_guidance(&v));
                if let (Some(on), Some(inner)) = (on, weak.upgrade())
                    && inner.screen_reader.swap(on, Ordering::SeqCst) != on
                {
                    inner.send(DeviceEvent::ScreenReader(on));
                }
            },
        );
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        let network = luna.subscribe(
            "luna://com.webos.service.connectionmanager/getStatus",
            &json!({}),
            SUBSCRIBE_TIMEOUT,
            move |answer| {
                let (Ok(v), Some(inner)) = (answer, weak.upgrade()) else { return };
                let status = network_status(&v);
                let mut last = inner.network.lock().unwrap_or_else(|e| e.into_inner());
                if *last != Some(status) {
                    *last = Some(status);
                    drop(last);
                    inner.send(DeviceEvent::Network { connected: status.0, ipv4: status.1 });
                }
            },
        );
        self.inner.subscriptions.lock().unwrap_or_else(|e| e.into_inner()).extend([guidance, network]);
    }

    /// Follows the drive plugged in, telling subscribers when it changes.
    fn watch_drives(&self, root: &Path) {
        let (alive, weak) = (Arc::downgrade(&self.inner), Arc::downgrade(&self.inner));
        crate::drives::watch(
            root,
            self.inner.luna.as_ref(),
            move || alive.strong_count() > 0,
            move |drive| {
                let Some(inner) = weak.upgrade() else { return };
                let mut now = inner.drive.lock().unwrap_or_else(|e| e.into_inner());
                if *now != drive {
                    *now = drive.clone();
                    drop(now);
                    inner.send(DeviceEvent::Drive(drive));
                }
            },
        );
    }

    /// Answers the TV's screen saver requests while the app keeps the screen
    /// awake (`registerScreenSaverRequest`: an answer with `ack: false`
    /// refuses the screen saver; SDL-webOS answers the same way).
    fn watch_screen_saver(&self, luna: &Luna) {
        let mut slot = self.inner.screen_saver.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return;
        }
        let client = format!("{}.wakelock", self.inner.app_id);
        let weak = Arc::downgrade(&self.inner);
        let answering = luna.clone();
        *slot = Some(luna.subscribe(
            "luna://com.webos.service.tvpower/power/registerScreenSaverRequest",
            &json!({ "clientName": client }),
            SUBSCRIBE_TIMEOUT,
            move |answer| {
                let (Ok(v), Some(inner)) = (answer, weak.upgrade()) else { return };
                if let Some(timestamp) = v.get("timestamp") {
                    let ack = !inner.awake.load(Ordering::SeqCst);
                    let reply = json!({ "clientName": client, "ack": ack, "timestamp": timestamp });
                    answering.call_with(
                        "luna://com.webos.service.tvpower/power/responseScreenSaverRequest",
                        &reply,
                        SUBSCRIBE_TIMEOUT,
                        |_| {},
                    );
                }
            },
        ));
    }
}

impl Inner {
    fn send(&self, event: DeviceEvent) {
        self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).retain(|tx| tx.send(event.clone()).is_ok());
        self.waker.wake();
    }
}

/// Audio Guidance from `getSystemSettings`' answer (`settings.audioGuidance`).
pub fn audio_guidance(answer: &Value) -> Option<bool> {
    answer.pointer("/settings/audioGuidance").and_then(Value::as_str).map(|v| v == "on")
}

/// Whether the TV is on a network, and its address there, from the
/// connection manager's answer: wired first, then Wi-Fi, as the web app reads it.
pub fn network_status(answer: &Value) -> (bool, Option<Ipv4Addr>) {
    let link = |name: &str| {
        let link = answer.get(name)?;
        (link.get("state").and_then(Value::as_str) == Some("connected"))
            .then(|| link.get("ipAddress").and_then(Value::as_str).and_then(|a| a.parse::<Ipv4Addr>().ok()))
    };
    match (link("wired"), link("wifi")) {
        (Some(Some(ip)), _) | (_, Some(Some(ip))) => (true, Some(ip)),
        (Some(None), _) | (_, Some(None)) => (true, None),
        _ => (false, None),
    }
}

/// The address the TV's route to the outside leaves from: connecting a UDP
/// socket sends nothing, it only picks the route (to a documentation-only
/// address that nobody owns).
fn routed_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

impl Device for WebosDevice {
    fn info(&self) -> DeviceInfo {
        let f = self.facts();
        DeviceInfo {
            maker: "LG".into(),
            os: format!("webOS {}", f.webos),
            os_major: f.webos,
            firmware: f.firmware.clone(),
            app_version: self.inner.app_version.clone(),
            user_agent: user_agent(&self.inner.app_version, f.webos, &f.model),
            data_durable: self.inner.data.durable,
            model: f.model,
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.inner.data.path.clone()
    }

    fn cache_dir(&self) -> PathBuf {
        self.inner.data.path.join("cache")
    }

    fn font_dirs(&self) -> Vec<PathBuf> {
        let fonts = PathBuf::from(SYSTEM_FONTS);
        if fonts.is_dir() { vec![fonts] } else { Vec::new() }
    }

    fn tier(&self) -> Tier {
        tier_of(&self.facts())
    }

    fn caps(&self, overrides: &CapsOverrides) -> CapsReport {
        webos_caps(&self.facts(), overrides)
    }

    fn local_ipv4(&self) -> Option<Ipv4Addr> {
        match *self.inner.network.lock().unwrap_or_else(|e| e.into_inner()) {
            Some((_, Some(ip))) => Some(ip),
            Some((false, None)) => None,
            _ => routed_ipv4(),
        }
    }

    fn clock_24h(&self) -> bool {
        self.facts().clock_24h
    }

    fn screen_reader(&self) -> bool {
        self.inner.screen_reader.load(Ordering::SeqCst)
    }

    /// Audio Guidance reads with the TV's own voice (`com.webos.service.tts`), cutting off what
    /// it was reading, as the TV's menus do.
    fn speak(&self, text: &str) {
        let Some(luna) = &self.inner.luna else { return };
        luna.call_with(
            "luna://com.webos.service.tts/speak",
            &json!({ "text": text, "clear": true }),
            SUBSCRIBE_TIMEOUT,
            |answer| {
                if let Err(e) = answer {
                    crate::log::line(&format!("device: Audio Guidance couldn't read ({e})"));
                }
            },
        );
    }

    fn keep_awake(&self, on: bool) {
        self.inner.awake.store(on, Ordering::SeqCst);
        if on && let Some(luna) = &self.inner.luna {
            self.watch_screen_saver(luna);
        }
    }

    /// Back on the first screen shows the TV's Home and keeps the app running
    /// (LG's rule for webOS 23 to 25, and what the web app's `platformBack`
    /// did up to webOS 5): webOS 4 draws Home as a ribbon over the app, later
    /// ones send it to the background. If Home won't show, the app hides itself.
    fn exit(&self) {
        {
            let mut last = self.inner.last_exit.lock().unwrap_or_else(|e| e.into_inner());
            if last.is_some_and(|at| at.elapsed() < EXIT_COOLDOWN) {
                return;
            }
            *last = Some(Instant::now());
        }
        match &self.inner.luna {
            Some(luna) => luna.call_with(
                "luna://com.webos.applicationManager/launch",
                &json!({ "id": "com.webos.app.home" }),
                HOME_TIMEOUT,
                |answer| {
                    if let Err(e) = answer {
                        crate::log::line(&format!("device: the TV's Home didn't show ({e}); hiding the app"));
                        sdl::request_minimize();
                    }
                },
            ),
            None => {
                sdl::request_minimize();
            }
        }
    }

    fn engine(&self) -> Box<dyn PlayerEngine> {
        let f = self.facts();
        let firmware = f.build.clone().or(f.firmware.clone()).unwrap_or_else(|| "unknown".into());
        Box::new(UmsEngine::new(
            self.inner.luna.clone(),
            self.inner.waker.clone(),
            UmsConfig {
                app_id: self.inner.app_id.clone(),
                firmware,
                table: f.codecs,
                data_dir: self.inner.data.path.clone(),
                scale: pley_platform::canvas_scale(sdl::PLANE.0, sdl::PLANE.1),
            },
        ))
    }

    fn subscribe(&self, tx: Sender<DeviceEvent>) {
        self.inner.subscribers.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
    }

    fn drive(&self) -> Option<Drive> {
        self.inner.drive.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn drive_space(&self, root: &Path) -> Option<DriveSpace> {
        data::space(root).map(|(free, total)| DriveSpace { free, total })
    }

    fn drive_name(&self, root: &Path) -> Option<String> {
        crate::drives::name(&self.inner.root, root)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pley_core::media::{EngineEvent, LoadRequest, MediaKind};

    use super::*;

    fn tv() -> (WebosDevice, PathBuf) {
        let root = pley_core::device::scratch_dir("webos-device");
        fs::create_dir_all(root.join("media/internal")).unwrap();
        fs::create_dir_all(root.join("var/run/nyx")).unwrap();
        fs::write(root.join("var/run/nyx/os_info.json"), r#"{"webos_release":"8.2.0","core_os_release":"8.2.0-27"}"#)
            .unwrap();
        fs::write(root.join("var/run/nyx/device_info.json"), r#"{"modelName":"OLED65C3PUA"}"#).unwrap();
        (WebosDevice::in_root(&root, None, Waker::noop(), "0.3.0"), root)
    }

    #[test]
    fn names_itself_from_nyx_with_a_clean_user_agent() {
        let (device, root) = tv();
        let info = device.info();
        assert_eq!((info.maker.as_str(), info.model.as_str(), info.os.as_str()), ("LG", "OLED65C3PUA", "webOS 23"));
        assert_eq!(info.user_agent, "pleyLG/0.3.0 (webOS 23; OLED65C3PUA)");
        assert!(info.data_durable);
        assert_eq!(device.data_dir(), root.join("media/internal/.app.pley.tv-data"));
        assert_eq!(device.cache_dir(), device.data_dir().join("cache"));
        assert_eq!(device.tier(), Tier::High);
        assert_eq!(device.caps(&CapsOverrides::default()).caps.device.unwrap().model.as_deref(), Some("OLED65C3PUA"));
        assert!(device.clock_24h());
        assert!(!device.screen_reader());
    }

    #[test]
    fn its_engine_says_when_there_is_no_pipeline() {
        let (device, _) = tv();
        let mut engine = device.engine();
        engine.load(LoadRequest::new("http://h/api/media/k/file.mp4", MediaKind::File));
        let events = engine.events();
        assert!(matches!(&events[..], [EngineEvent::Error { before_first_picture: true, .. }]), "{events:?}");
        assert_eq!(engine.stats().player, "webOS media pipeline");
        engine.unload();
        device.keep_awake(true);
        device.exit();
    }

    #[test]
    fn reads_audio_guidance_and_the_network_as_the_web_app_does() {
        assert_eq!(audio_guidance(&json!({ "settings": { "audioGuidance": "on" } })), Some(true));
        assert_eq!(audio_guidance(&json!({ "settings": { "audioGuidance": "off" } })), Some(false));
        assert_eq!(audio_guidance(&json!({ "returnValue": true })), None);
        let wifi = json!({
            "wired": { "state": "disconnected" },
            "wifi": { "state": "connected", "ipAddress": "192.168.1.23" }
        });
        assert_eq!(network_status(&wifi), (true, Some(Ipv4Addr::new(192, 168, 1, 23))));
        let both = json!({
            "wired": { "state": "connected", "ipAddress": "10.0.0.5" },
            "wifi": { "state": "connected", "ipAddress": "192.168.1.23" }
        });
        assert_eq!(network_status(&both), (true, Some(Ipv4Addr::new(10, 0, 0, 5))));
        assert_eq!(network_status(&json!({ "wired": { "state": "connected" } })), (true, None));
        assert_eq!(network_status(&json!({ "wifi": { "state": "disconnected" } })), (false, None));
    }

    #[test]
    fn a_tv_without_a_drive_has_none() {
        let (device, _) = tv();
        assert_eq!(device.drive(), None);
    }

    #[test]
    fn news_reaches_subscribers() {
        let (device, _) = tv();
        let (tx, rx) = std::sync::mpsc::channel();
        device.subscribe(tx);
        device.inner.send(DeviceEvent::ScreenReader(true));
        assert_eq!(rx.try_recv().unwrap(), DeviceEvent::ScreenReader(true));
    }
}
