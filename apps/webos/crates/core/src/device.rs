//! The device the app runs on, as the rest of the app sees it: its facts,
//! folders, capability report, network address, clock format, screen reader,
//! and its player engine. `pley-webos` implements it for LG TVs; [`FakeDevice`]
//! stands in for tests and the simulator.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::media::PlayerEngine;
use crate::model::{ClientCaps, DeviceCaps, DisplayCaps, DolbyVisionDecoder, DynamicRange, HlsCaps, VideoDecoder};
use crate::store::{PassthroughChoice, PassthroughChoices, PassthroughFormat, Settings};

/// The device's facts, for Diagnostics, About and the `User-Agent`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    /// `LG`.
    pub maker: String,
    /// `OLED65C3PUA`.
    pub model: String,
    /// `webOS 23`.
    pub os: String,
    /// The webOS version as LG names it now: 4, 5, 6, then 22, 23… (from nyx,
    /// never a user agent).
    pub os_major: u32,
    pub firmware: Option<String>,
    /// This app's version, `0.2.0`.
    pub app_version: String,
    /// See [`user_agent`].
    pub user_agent: String,
    /// Whether the data folder survives a restart: when it doesn't, the app
    /// says it can't keep the sign-in.
    pub data_durable: bool,
}

pub use pley_base::Tier;

/// What the device plays, as the server is told (the version 2 report) and
/// as Diagnostics lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CapsReport {
    pub caps: ClientCaps,
    /// Diagnostics' lines: `TV: …`, `webOS 23`, `Panel: 4K OLED`, `HDR: …`…
    pub lines: Vec<String>,
    /// Containers LG's pipeline shows Dolby Vision from; a Dolby Vision file in
    /// another is asked for as a remux (all but MKV before webOS 25).
    pub dv_containers: Vec<String>,
}

/// The playback settings that change the capability report
/// (`RES/tv-player-logic.md §3.4`): which surround formats are sent as they are.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CapsOverrides {
    pub passthrough: PassthroughChoices,
}

impl CapsOverrides {
    /// The overrides the device settings make.
    pub fn from_settings(settings: &Settings) -> CapsOverrides {
        CapsOverrides { passthrough: settings.passthrough.clone() }
    }

    /// The choice for one format.
    pub fn choice(&self, f: PassthroughFormat) -> PassthroughChoice {
        self.passthrough.get(f)
    }

    /// "Send TrueHD to the receiver": the web app's `passthroughTrueHd`.
    pub fn truehd(&self) -> bool {
        self.choice(PassthroughFormat::TrueHd) == PassthroughChoice::On
    }

    /// "Send DTS to the receiver": the web app's `passthroughDts`.
    pub fn dts(&self) -> bool {
        self.choice(PassthroughFormat::Dts) == PassthroughChoice::On
    }
}

/// News from the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    /// LG's Audio Guidance was turned on or off.
    ScreenReader(bool),
    /// The network came or went, with the TV's address on it.
    Network { connected: bool, ipv4: Option<Ipv4Addr> },
}

/// The device. Every method is quick; none waits on the network.
pub trait Device: Send + Sync {
    /// Maker, model, `webOS 23`, firmware, app version, user agent.
    fn info(&self) -> DeviceInfo;
    /// Settings, sign-ins, snapshots.
    fn data_dir(&self) -> PathBuf;
    /// Pictures (budgeted).
    fn cache_dir(&self) -> PathBuf;
    /// The TV's own fonts, for subtitles in other scripts.
    fn font_dirs(&self) -> Vec<PathBuf>;
    /// How much the device can take.
    fn tier(&self) -> Tier;
    /// The capability report and Diagnostics lines for these settings.
    fn caps(&self, overrides: &CapsOverrides) -> CapsReport;
    /// The TV's IPv4 address, for "Search this network" and the address pad.
    fn local_ipv4(&self) -> Option<Ipv4Addr>;
    /// Times of day as `21:05` rather than `9:05 PM`.
    fn clock_24h(&self) -> bool;
    /// Whether LG's Audio Guidance is on.
    fn screen_reader(&self) -> bool;
    /// Keeps the screen saver away while playing.
    fn keep_awake(&self, on: bool);
    /// Leaves the app the way the TV expects.
    fn exit(&self);
    /// A new player engine.
    fn engine(&self) -> Box<dyn PlayerEngine>;
    /// Sends the device's news (screen reader, network) to `tx`.
    fn subscribe(&self, tx: Sender<DeviceEvent>);
}

/// The `User-Agent` the server names the TV by (`pleyLG/0.2.0 (webOS 23;
/// OLED65C3PUA)`): the model cleaned to printable ASCII without `();\`, at
/// most 40 characters, as the Android app cleans its device name.
pub fn user_agent(app_version: &str, os_major: u32, model: &str) -> String {
    let clean: String = model
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .filter(|c| !"();\\".contains(*c))
        .take(40)
        .collect();
    let clean = clean.trim();
    let model = if clean.is_empty() { "LG TV" } else { clean };
    format!("pleyLG/{app_version} (webOS {os_major}; {model})")
}

/// A new, empty folder under the system's temporary folder, for tests, the
/// simulator and [`FakeDevice`].
pub fn scratch_dir(prefix: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let dir = std::env::temp_dir().join(format!("pley-{prefix}-{}-{n}-{nanos}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

type EngineFactory = dyn Fn() -> Box<dyn PlayerEngine> + Send + Sync;

/// A device for tests and the simulator: a 2022 LG OLED's capability report,
/// folders under a temporary folder, and the engine it is given.
pub struct FakeDevice {
    root: PathBuf,
    engine: Box<EngineFactory>,
    screen_reader: AtomicBool,
    clock_24h: AtomicBool,
    awake: AtomicBool,
    exited: AtomicBool,
    ipv4: Mutex<Option<Ipv4Addr>>,
    subscribers: Mutex<Vec<Sender<DeviceEvent>>>,
}

impl FakeDevice {
    /// A device whose folders are in a new temporary folder.
    pub fn new(engine: impl Fn() -> Box<dyn PlayerEngine> + Send + Sync + 'static) -> FakeDevice {
        FakeDevice::in_dir(scratch_dir("device"), engine)
    }

    /// A device whose folders are under `root` (the simulator's `--data`).
    pub fn in_dir(root: PathBuf, engine: impl Fn() -> Box<dyn PlayerEngine> + Send + Sync + 'static) -> FakeDevice {
        FakeDevice {
            root,
            engine: Box::new(engine),
            screen_reader: AtomicBool::new(false),
            clock_24h: AtomicBool::new(true),
            awake: AtomicBool::new(false),
            exited: AtomicBool::new(false),
            ipv4: Mutex::new(Some(Ipv4Addr::new(192, 168, 1, 50))),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Shared, as the app holds its device.
    pub fn shared(self) -> Arc<dyn Device> {
        Arc::new(self)
    }

    /// Turns the stand-in for Audio Guidance on or off, telling subscribers.
    pub fn set_screen_reader(&self, on: bool) {
        self.screen_reader.store(on, Ordering::SeqCst);
        self.send(DeviceEvent::ScreenReader(on));
    }

    /// Times of day as `21:05` (true) or `9:05 PM`.
    pub fn set_clock_24h(&self, on: bool) {
        self.clock_24h.store(on, Ordering::SeqCst);
    }

    /// Changes the TV's address, telling subscribers.
    pub fn set_ipv4(&self, ip: Option<Ipv4Addr>) {
        *self.ipv4.lock().unwrap_or_else(|e| e.into_inner()) = ip;
        self.send(DeviceEvent::Network { connected: ip.is_some(), ipv4: ip });
    }

    /// Tells every subscriber `event`.
    pub fn send(&self, event: DeviceEvent) {
        self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).retain(|tx| tx.send(event.clone()).is_ok());
    }

    /// Whether the app asked to leave.
    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Whether the app keeps the screen awake now.
    pub fn awake(&self) -> bool {
        self.awake.load(Ordering::SeqCst)
    }
}

impl Device for FakeDevice {
    fn info(&self) -> DeviceInfo {
        let model = "OLED65C2PUA";
        DeviceInfo {
            maker: "LG".into(),
            model: model.into(),
            os: "webOS 22".into(),
            os_major: 22,
            firmware: Some("03.20.70".into()),
            app_version: env!("CARGO_PKG_VERSION").into(),
            user_agent: user_agent(env!("CARGO_PKG_VERSION"), 22, model),
            data_durable: true,
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }

    fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    fn font_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn tier(&self) -> Tier {
        Tier::High
    }

    fn caps(&self, overrides: &CapsOverrides) -> CapsReport {
        lg_2022_caps(overrides)
    }

    fn local_ipv4(&self) -> Option<Ipv4Addr> {
        *self.ipv4.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn clock_24h(&self) -> bool {
        self.clock_24h.load(Ordering::SeqCst)
    }

    fn screen_reader(&self) -> bool {
        self.screen_reader.load(Ordering::SeqCst)
    }

    fn keep_awake(&self, on: bool) {
        self.awake.store(on, Ordering::SeqCst);
    }

    fn exit(&self) {
        self.exited.store(true, Ordering::SeqCst);
    }

    fn engine(&self) -> Box<dyn PlayerEngine> {
        (self.engine)()
    }

    fn subscribe(&self, tx: Sender<DeviceEvent>) {
        self.subscribers.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn decoder(
    codec: &str,
    profiles: Option<&[&str]>,
    size: (i64, i64),
    fps: f64,
    depth: i64,
    level: Option<i64>,
) -> VideoDecoder {
    VideoDecoder {
        codec: codec.into(),
        hardware: true,
        profiles: profiles.map(strings),
        max_width: Some(size.0),
        max_height: Some(size.1),
        max_fps: Some(fps),
        max_bit_depth: Some(depth),
        max_level: level,
    }
}

/// The capability report of a 2022 LG OLED (webOS 22, 4K, HDR10 and Dolby
/// Vision, sound from the TV's speakers), by the rules of the web app's
/// `caps.ts`: what [`FakeDevice`] reports.
pub fn lg_2022_caps(overrides: &CapsOverrides) -> CapsReport {
    let avc = ["Constrained Baseline", "Baseline", "Main", "High"];
    let video = vec![
        decoder("h264", Some(&avc), (3840, 2160), 30.0, 8, Some(51)),
        decoder("h264", Some(&avc), (1920, 1080), 60.0, 8, Some(42)),
        decoder("hevc", Some(&["Main", "Main 10"]), (4096, 2160), 60.0, 10, Some(153)),
        decoder("vp9", Some(&["Profile 0", "Profile 2"]), (3840, 2160), 60.0, 10, None),
        decoder("av1", Some(&["Main"]), (3840, 2160), 60.0, 10, None),
        decoder("mpeg2video", None, (1920, 1080), 60.0, 8, None),
        decoder("mpeg4", None, (1920, 1080), 30.0, 8, None),
    ];
    let decode = strings(&["aac", "ac3", "eac3", "mp3", "mp2", "flac", "vorbis", "pcm_s16le", "pcm_s24le", "opus"]);
    // The TV's speakers: nothing goes to a receiver unless Settings says so, and
    // only DTS can (the TV doesn't decode it).
    let passthrough = if overrides.dts() { strings(&["dts"]) } else { Vec::new() };
    let mut audio_codecs = decode.clone();
    audio_codecs.extend(passthrough.iter().cloned());
    let dv_containers = strings(&["mp4", "m4v", "mov", "ts", "m2ts", "mpegts"]);
    let caps = ClientCaps {
        containers: strings(&[
            "mkv", "mp4", "m4v", "mov", "ts", "m2ts", "mpegts", "avi", "mpg", "mpeg", "mp3", "m4a", "aac", "flac",
            "ogg", "oga", "wav", "mka",
        ]),
        video_codecs: strings(&["h264", "hevc", "vp9", "av1", "mpeg2video", "mpeg4"]),
        audio_codecs,
        hdr: true,
        dolby_vision: true,
        caps_version: Some(2),
        hdr_types: vec![DynamicRange::Hdr10, DynamicRange::Hlg, DynamicRange::DolbyVision],
        dolby_vision_profiles: vec![
            DolbyVisionDecoder { profile: 5, max_level: None },
            DolbyVisionDecoder { profile: 8, max_level: None },
        ],
        audio_decode: decode.clone(),
        audio_passthrough: passthrough.clone(),
        audio_channels: 2,
        subtitle_codecs: strings(&[
            "subrip",
            "ass",
            "ssa",
            "webvtt",
            "mov_text",
            "hdmv_pgs_subtitle",
            "dvd_subtitle",
            "dvb_subtitle",
        ]),
        audio_track_switching: true,
        picture_subtitles_sideload: true,
        hls: HlsCaps { master_playlist: true, segments: strings(&["ts", "fmp4"]), variants: None },
        display: Some(DisplayCaps { width: 3840, height: 2160, refresh_rates: vec![50.0, 60.0] }),
        device: Some(DeviceCaps {
            form: "tv".into(),
            tier: Some("high".into()),
            fire_tv: false,
            model: Some("OLED65C2PUA".into()),
        }),
        video: video.clone(),
    };
    let video_line = video
        .iter()
        .map(|v| {
            format!(
                "{} {}×{}@{}",
                v.codec,
                v.max_width.unwrap_or(0),
                v.max_height.unwrap_or(0),
                v.max_fps.unwrap_or(0.0)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let lines = vec![
        "TV: OLED65C2PUA (firmware 03.20.70)".to_string(),
        "webOS 22".to_string(),
        "Panel: 4K OLED".to_string(),
        "HDR: HDR10, HLG, Dolby Vision".to_string(),
        format!("Dolby Vision: profiles 5, 8, from {}", dv_containers.join(", ").to_uppercase()),
        format!("Video: {video_line}"),
        format!("Audio decoded: {}", decode.join(", ")),
        format!(
            "Sent to the receiver: {}",
            if passthrough.is_empty() { "nothing".to_string() } else { passthrough.join(", ") }
        ),
        "Sound: tv_speaker (digital auto)".to_string(),
        "Streams: HLS ts, fmp4".to_string(),
    ];
    CapsReport { caps, lines, dv_containers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::NullEngine;

    #[test]
    fn the_user_agent_names_the_app_the_os_and_a_clean_model() {
        assert_eq!(user_agent("0.2.0", 23, "OLED65C3PUA"), "pleyLG/0.2.0 (webOS 23; OLED65C3PUA)");
        assert_eq!(user_agent("0.2.0", 4, "65UK(6300);\\x"), "pleyLG/0.2.0 (webOS 4; 65UK6300x)");
        assert_eq!(user_agent("0.2.0", 5, " \u{1F4FA} "), "pleyLG/0.2.0 (webOS 5; LG TV)");
        assert_eq!(user_agent("1", 6, &"M".repeat(60)), format!("pleyLG/1 (webOS 6; {})", "M".repeat(40)));
    }

    #[test]
    fn the_fake_tv_reports_as_the_web_app_would() {
        let report = lg_2022_caps(&CapsOverrides::default());
        let json = serde_json::to_string(&report.caps).unwrap();
        assert!(json.starts_with(r#"{"containers":["mkv","mp4""#), "{json}");
        assert!(json.contains(r#""max_fps":30,"max_bit_depth":8,"max_level":51}"#), "{json}");
        assert!(json.contains(r#"{"codec":"mpeg2video","hardware":true,"max_width":1920"#), "{json}");
        assert!(json.contains(r#""hls":{"master_playlist":true,"segments":["ts","fmp4"]}"#), "{json}");
        assert!(json.contains(r#""display":{"width":3840,"height":2160,"refresh_rates":[50,60]}"#), "{json}");
        assert!(json.contains(r#""dolby_vision_profiles":[{"profile":5},{"profile":8}]"#), "{json}");
        assert!(report.caps.audio_passthrough.is_empty());
        let mut dts = CapsOverrides::default();
        dts.passthrough.set(PassthroughFormat::Dts, PassthroughChoice::On);
        assert_eq!(lg_2022_caps(&dts).caps.audio_passthrough, ["dts"]);
    }

    #[test]
    fn the_fake_device_hands_out_its_engine_and_news() {
        let device = FakeDevice::new(|| Box::new(NullEngine::default()));
        let (tx, rx) = std::sync::mpsc::channel();
        device.subscribe(tx);
        device.set_screen_reader(true);
        assert_eq!(rx.try_recv().unwrap(), DeviceEvent::ScreenReader(true));
        assert!(device.screen_reader());
        assert!(device.data_dir().starts_with(std::env::temp_dir()));
        let mut engine = device.engine();
        engine.seek(5_000);
        assert_eq!(engine.position_ms(), 5_000);
        device.exit();
        assert!(device.exited());
        assert_eq!(device.info().user_agent, user_agent(env!("CARGO_PKG_VERSION"), 22, "OLED65C2PUA"));
    }
}
