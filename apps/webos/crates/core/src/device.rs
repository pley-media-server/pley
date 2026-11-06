//! The device the app runs on, as the rest of the app sees it: its facts,
//! folders, capability report, network address, clock format, screen reader,
//! the USB drive plugged into it, and its player engine. `pley-webos` implements it for LG TVs; [`FakeDevice`]
//! stands in for tests and the simulator.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use pley_i18n::t;

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
    /// Diagnostics' lines: `TV: …`, `webOS 23`, `Panel: 4K OLED`, `HDR: …`…, in the language shown
    /// when the report was made.
    pub lines: Vec<String>,
    /// Containers LG's pipeline shows Dolby Vision from; a Dolby Vision file in
    /// another is asked for as a remux (all but MKV before webOS 25).
    pub dv_containers: Vec<String>,
}

/// The playback settings that change the capability report: which surround
/// formats are sent as they are.
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

/// The USB drive plugged into the TV: the first one mounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drive {
    /// Where it is mounted (`/tmp/usb/sda/sda1` on an LG TV).
    pub root: PathBuf,
    /// Plugged in and mounted by the TV, but the app's jail doesn't show its files.
    pub hidden: bool,
}

impl Drive {
    /// Whether the app can read its files.
    pub fn readable(&self) -> bool {
        !self.hidden
    }
}

/// How full a drive is, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DriveSpace {
    pub free: u64,
    pub total: u64,
}

/// News from the device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    /// LG's Audio Guidance was turned on or off.
    ScreenReader(bool),
    /// The network came or went, with the TV's address on it.
    Network { connected: bool, ipv4: Option<Ipv4Addr> },
    /// A USB drive was plugged in, pulled out or swapped: the one there now.
    Drive(Option<Drive>),
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
    /// The TV's IPv4 address, for "Search this network" and the address entry.
    fn local_ipv4(&self) -> Option<Ipv4Addr>;
    /// Times of day as `21:05` rather than `9:05 PM`.
    fn clock_24h(&self) -> bool;
    /// The TV's languages, its menus' first, as tags (`fr-CA`): the app shows the first Pley has
    /// when nothing else says which (docs/LG.md, "Language"). None until the TV has said.
    fn languages(&self) -> Vec<String> {
        Vec::new()
    }
    /// Whether LG's Audio Guidance is on.
    fn screen_reader(&self) -> bool;
    /// Reads `text` aloud in the TV's voice, cutting off what it was reading. Called only while
    /// [`Device::screen_reader`] is on; never blocks.
    fn speak(&self, _text: &str) {}
    /// Keeps the screen saver away while playing.
    fn keep_awake(&self, on: bool);
    /// Leaves the app the way the TV expects.
    fn exit(&self);
    /// A new player engine.
    fn engine(&self) -> Box<dyn PlayerEngine>;
    /// Sends the device's news (screen reader, network, drives) to `tx`.
    fn subscribe(&self, tx: Sender<DeviceEvent>);
    /// The USB drive plugged in (the first mounted); none while none is. Quick: the device follows
    /// drives as they come and go.
    fn drive(&self) -> Option<Drive> {
        None
    }
    /// How full the drive mounted at `root` is; none when the device won't say. It asks the disk,
    /// so it runs off the main thread.
    fn drive_space(&self, _root: &Path) -> Option<DriveSpace> {
        None
    }
    /// The make and model of the drive mounted at `root`, as it reports them ("Kingston
    /// DataTraveler 3.0"); none when it says nothing or the device won't let the app read it. It
    /// reads the system's files, so it runs off the main thread.
    fn drive_name(&self, _root: &Path) -> Option<String> {
        None
    }
}

/// Where an LG TV mounts USB drives: `/tmp/usb/<disk>/<partition>`.
pub const USB_ROOT: &str = "/tmp/usb";

/// The extensions a drive's file is known by in a log line: the videos the app lists and the
/// subtitles beside them.
const DRIVE_FILE_EXTENSIONS: [&str; 17] = [
    "mkv", "mp4", "m4v", "mov", "ts", "m2ts", "mts", "avi", "mpg", "mpeg", "webm", "srt", "vtt", "mks", "ass", "ssa",
    "sub",
];

/// `text` with the folders and file names of a drive taken out: everything after `/tmp/usb/`
/// and the disk and partition (`sda/sda1`, which name devices, not anyone's files) up to the
/// file's extension, so `/tmp/usb/sda/sda1/Films/Mad%20Max.mkv` logs as
/// `/tmp/usb/sda/sda1/….mkv`. Without an extension, the rest of the path goes, up to a quote or
/// the end of the line.
pub fn hide_drive_paths(text: &str) -> String {
    let marker = format!("{USB_ROOT}/");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(&marker) {
        let (before, after) = rest.split_at(at + marker.len());
        out.push_str(before);
        let mut after = after;
        // Below the partition: a folder or a file of someone's.
        let mut below = true;
        for _ in 0..2 {
            let end = after.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(after.len());
            if !is_device_name(&after[..end]) {
                break;
            }
            out.push_str(&after[..end]);
            after = &after[end..];
            match after.strip_prefix('/') {
                Some(under) => {
                    out.push('/');
                    after = under;
                }
                None => {
                    below = false;
                    break;
                }
            }
        }
        if below {
            let span = after.find(['"', '\'', '<', '>', '\n']).unwrap_or(after.len());
            let end = extension_at(&after[..span]).unwrap_or(span);
            if end > 0 {
                out.push('…');
            }
            after = &after[end..];
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// A disk or partition as the kernel names it: `sda`, `sdb1`.
fn is_device_name(s: &str) -> bool {
    let Some(tail) = s.strip_prefix("sd") else { return false };
    let letters = tail.bytes().take_while(u8::is_ascii_lowercase).count();
    letters > 0 && tail.bytes().skip(letters).all(|b| b.is_ascii_digit())
}

/// Where the first `.mkv` (or another drive file's extension, ended by anything but a letter or
/// digit) starts in `path`.
fn extension_at(path: &str) -> Option<usize> {
    path.match_indices('.').map(|(at, _)| at).find(|&at| {
        let ext = &path[at + 1..];
        let len = ext.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(ext.len());
        DRIVE_FILE_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(&ext[..len]))
    })
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
    languages: Mutex<Vec<String>>,
    awake: AtomicBool,
    exited: AtomicBool,
    os_major: AtomicU32,
    ipv4: Mutex<Option<Ipv4Addr>>,
    subscribers: Mutex<Vec<Sender<DeviceEvent>>>,
    spoken: Mutex<Vec<String>>,
    drive: Mutex<Option<Drive>>,
    drive_space: Mutex<Option<DriveSpace>>,
    drive_name: Mutex<Option<String>>,
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
            languages: Mutex::new(vec!["en-US".into()]),
            awake: AtomicBool::new(false),
            exited: AtomicBool::new(false),
            os_major: AtomicU32::new(22),
            ipv4: Mutex::new(Some(Ipv4Addr::new(192, 168, 1, 50))),
            subscribers: Mutex::new(Vec::new()),
            spoken: Mutex::new(Vec::new()),
            drive: Mutex::new(None),
            drive_space: Mutex::new(None),
            drive_name: Mutex::new(None),
        }
    }

    /// What Audio Guidance read since the last call, oldest first.
    pub fn take_spoken(&self) -> Vec<String> {
        std::mem::take(&mut *self.spoken.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Shared, as the app holds its device.
    pub fn shared(self) -> Arc<dyn Device> {
        Arc::new(self)
    }

    /// Runs as webOS `major` (22 at first).
    pub fn set_os_major(&self, major: u32) {
        self.os_major.store(major, Ordering::SeqCst);
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

    /// The TV's languages (`en-US` at first).
    pub fn set_languages(&self, languages: &[&str]) {
        *self.languages.lock().unwrap_or_else(|e| e.into_inner()) = languages.iter().map(|l| l.to_string()).collect();
    }

    /// Changes the TV's address, telling subscribers.
    pub fn set_ipv4(&self, ip: Option<Ipv4Addr>) {
        *self.ipv4.lock().unwrap_or_else(|e| e.into_inner()) = ip;
        self.send(DeviceEvent::Network { connected: ip.is_some(), ipv4: ip });
    }

    /// Plugs a drive in (or pulls it out, with `None`), telling subscribers when that changes.
    pub fn set_drive(&self, drive: Option<Drive>) {
        let mut now = self.drive.lock().unwrap_or_else(|e| e.into_inner());
        if *now != drive {
            *now = drive.clone();
            drop(now);
            self.send(DeviceEvent::Drive(drive));
        }
    }

    /// How full the drive says it is (none: it won't say).
    pub fn set_drive_space(&self, space: Option<DriveSpace>) {
        *self.drive_space.lock().unwrap_or_else(|e| e.into_inner()) = space;
    }

    /// The make and model the drive gives (none: it says nothing).
    pub fn set_drive_name(&self, name: Option<&str>) {
        *self.drive_name.lock().unwrap_or_else(|e| e.into_inner()) = name.map(str::to_string);
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
        let os_major = self.os_major.load(Ordering::SeqCst);
        DeviceInfo {
            maker: "LG".into(),
            model: model.into(),
            os: format!("webOS {os_major}"),
            os_major,
            firmware: Some("03.20.70".into()),
            app_version: env!("CARGO_PKG_VERSION").into(),
            user_agent: user_agent(env!("CARGO_PKG_VERSION"), os_major, model),
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

    fn languages(&self) -> Vec<String> {
        self.languages.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn screen_reader(&self) -> bool {
        self.screen_reader.load(Ordering::SeqCst)
    }

    fn speak(&self, text: &str) {
        self.spoken.lock().unwrap_or_else(|e| e.into_inner()).push(text.to_string());
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

    fn drive(&self) -> Option<Drive> {
        self.drive.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn drive_space(&self, root: &Path) -> Option<DriveSpace> {
        let space = *self.drive_space.lock().unwrap_or_else(|e| e.into_inner());
        space.filter(|_| self.drive().is_some_and(|d| d.root == root))
    }

    fn drive_name(&self, root: &Path) -> Option<String> {
        let name = self.drive_name.lock().unwrap_or_else(|e| e.into_inner()).clone();
        name.filter(|_| self.drive().is_some_and(|d| d.root == root))
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
    let sent =
        if passthrough.is_empty() { t!("diagnostics.lines.nothing").to_string() } else { passthrough.join(", ") };
    let lines = vec![
        t!("diagnostics.lines.tvFirmware", model = "OLED65C2PUA", firmware = "03.20.70"),
        "webOS 22".to_string(),
        t!("diagnostics.lines.panel", panel = "4K OLED"),
        t!("diagnostics.lines.hdr", formats = "HDR10, HLG, Dolby Vision"),
        t!("diagnostics.lines.dolbyVision", profiles = "5, 8", containers = dv_containers.join(", ").to_uppercase()),
        t!("diagnostics.lines.video", decoders = video_line),
        t!("diagnostics.lines.audioDecoded", formats = decode.join(", ")),
        t!("diagnostics.lines.sent", formats = sent),
        t!(
            "diagnostics.lines.sound",
            output = t!("diagnostics.lines.digital", output = "tv_speaker", digital = "auto")
        ),
        t!("diagnostics.lines.streams", segments = "ts, fmp4"),
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

    #[test]
    fn the_fake_device_follows_a_drive_plugged_in_and_pulled_out() {
        let device = FakeDevice::new(|| Box::new(NullEngine::default()));
        let (tx, rx) = std::sync::mpsc::channel();
        device.subscribe(tx);
        assert_eq!(device.drive(), None);
        let drive = Drive { root: PathBuf::from("/tmp/usb/sda/sda1"), hidden: false };
        device.set_drive(Some(drive.clone()));
        device.set_drive(Some(drive.clone()));
        assert_eq!(rx.try_recv().unwrap(), DeviceEvent::Drive(Some(drive.clone())));
        assert!(rx.try_recv().is_err(), "the same drive again is no news");
        assert_eq!(device.drive(), Some(drive.clone()));
        device.set_drive_space(Some(DriveSpace { free: 1, total: 2 }));
        assert_eq!(device.drive_space(&drive.root), Some(DriveSpace { free: 1, total: 2 }));
        assert_eq!(device.drive_space(Path::new("/tmp/usb/sdb/sdb1")), None);
        device.set_drive_name(Some("Kingston DataTraveler 3.0"));
        assert_eq!(device.drive_name(&drive.root).as_deref(), Some("Kingston DataTraveler 3.0"));
        device.set_drive(None);
        assert_eq!(rx.try_recv().unwrap(), DeviceEvent::Drive(None));
        assert_eq!(device.drive_space(&drive.root), None);
        assert_eq!(device.drive_name(&drive.root), None);
    }

    #[test]
    fn a_drives_folders_and_file_names_never_reach_the_log() {
        let hide = hide_drive_paths;
        assert_eq!(
            hide("load file:///tmp/usb/sda/sda1/Films/Mad%20Max.mkv (direct)"),
            "load file:///tmp/usb/sda/sda1/….mkv (direct)"
        );
        assert_eq!(
            hide(r#"{"uri":"file:///tmp/usb/sdb/sdb1/My Films/Our trip.MP4","x":1}"#),
            r#"{"uri":"file:///tmp/usb/sdb/sdb1/….MP4","x":1}"#
        );
        assert_eq!(hide("subtitles /tmp/usb/sda/sda1/a/Film.en.srt read"), "subtitles /tmp/usb/sda/sda1/….srt read");
        // The drive itself is no one's secret; a folder without a file is hidden to the end.
        assert_eq!(hide("drives: /tmp/usb/sda/sda1 mounted"), "drives: /tmp/usb/sda/sda1 mounted");
        assert_eq!(hide("can't read /tmp/usb/sdc/sdc2/Holiday 2019"), "can't read /tmp/usb/sdc/sdc2/…");
        // A name that isn't a device's is hidden from the start.
        assert_eq!(
            hide("/tmp/usb/Films/x.mkv and /tmp/usb/sda/sda1/y.ts"),
            "/tmp/usb/….mkv and /tmp/usb/sda/sda1/….ts"
        );
        assert_eq!(hide("nothing here"), "nothing here");
    }
}
