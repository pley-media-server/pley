//! What the TV says of itself that decides what it plays: the webOS
//! version, the panel, the HDR formats it shows, where its sound goes, and
//! what its media pipeline decodes. The port of the LG web app's, with the
//! native sources in place of the web runtime's: the webOS version comes
//! from nyx's platform version, never a user agent; the screen from LG's SDL.
//!
//! Read once at start, every source bounded: files capped in size, every
//! luna call with a timeout. Anything that doesn't answer is left at what is
//! safe (no Dolby Vision, sound decoded by the TV), as the web app does.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::luna::Luna;

/// nyx's identity of the firmware (`webos_release`, `core_os_release`…).
pub const OS_INFO: &str = "/var/run/nyx/os_info.json";
/// nyx's identity of the set (`modelName`…; spellings vary by release).
pub const DEVICE_INFO: &str = "/var/run/nyx/device_info.json";
/// What uMediaServer's URI player decodes, per codec.
pub const CODEC_TABLE: &str = "/etc/umediaserver/device_codec_capability_config.json";

/// The configuration names the web app asks for (`facts.ts:65-77`).
pub const CONFIGS: [&str; 11] = [
    "tv.model.modelname",
    "tv.nyx.firmwareVersion",
    "tv.nyx.platformVersion",
    "tv.hw.panelResolution",
    "tv.hw.displayType",
    "tv.hw.ddrSize",
    "tv.model.supportHDR",
    "tv.config.supportDolbyHDRContents",
    "tv.config.supportDolbyTVATMOS",
    "tv.model.soundModeType",
    "tv.model.edidType",
];

/// The web app's timeout for each of these calls (`luna.ts`).
const CALL_TIMEOUT: Duration = Duration::from_secs(3);
/// The most of a facts file that is read: nyx's files are a few hundred bytes,
/// the codec table a few kilobytes.
const MAX_FILE_BYTES: u64 = 256 * 1024;

/// The facts.
#[derive(Clone, Debug, PartialEq)]
pub struct WebosFacts {
    /// 4, 5, 6, 22, 23, 24, 25, 26.
    pub webos: u32,
    /// The platform version the webOS version comes from (`4.10.2`, `8.2.0`).
    pub release: Option<String>,
    /// The firmware's build (`core_os_release`, `4.10.2-31`): an update can
    /// change a decoder, so what is learned about the pipeline is kept per build.
    pub build: Option<String>,
    pub model: String,
    pub firmware: Option<String>,
    /// The pipeline's resolution: 3840×2160 on a UHD set (the interface stays
    /// 1920×1080).
    pub screen: Option<(u32, u32)>,
    pub uhd: bool,
    pub uhd8k: bool,
    pub oled: bool,
    /// `tv.model.supportHDR`.
    pub hdr10: bool,
    /// `tv.config.supportDolbyHDRContents`.
    pub dolby_vision: bool,
    /// Where the sound goes: `tv_speaker`, `external_arc`, `external_optical`, `bt_soundbar`…
    pub sound_output: Option<String>,
    /// The digital output setting: `auto`, `passThrough` or `pcm`.
    pub digital_output: Option<String>,
    /// The receiver reports Atmos (eARC/ARC), or the TV decodes it to its own speakers.
    pub atmos: bool,
    /// `tv.model.edidType`: what the HDMI sink takes (`TrueHD`, `TrueHD+dts`).
    pub edid: Option<String>,
    /// `tv.hw.ddrSize` (`1.5G`, `3G`).
    pub memory: Option<String>,
    /// The URI player's codec table, when the file could be read.
    pub codecs: Option<CodecTable>,
    /// Times of day as `21:05` (the TV's clock setting, else its locale's custom).
    pub clock_24h: bool,
}

impl Default for WebosFacts {
    /// What is safe when the TV said nothing: webOS 4, an HD panel, SDR, sound
    /// from the TV.
    fn default() -> WebosFacts {
        WebosFacts::from_sources(&Sources::default())
    }
}

/// What the readers gathered, before the web app's rules turn it into facts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sources {
    /// [`OS_INFO`]'s object.
    pub os_info: Value,
    /// [`DEVICE_INFO`]'s object.
    pub device_info: Value,
    /// `com.webos.service.config/getConfigs`' `configs`.
    pub configs: Value,
    /// `getSystemSettings` (category `sound`)'s `settings`.
    pub sound: Value,
    /// `com.webos.service.arccontroller/getARCState`'s answer.
    pub arc: Value,
    /// `getSystemSettings` (`localeInfo`)'s `settings`.
    pub locale: Value,
    /// `SDL_webOSGetPanelResolution`.
    pub panel: Option<(u32, u32)>,
    pub codecs: Option<CodecTable>,
}

impl WebosFacts {
    /// Reads the facts from the TV: nyx's files and the codec table, then the
    /// config, settings and ARC services in parallel (when `luna` is there),
    /// with `panel` from LG's SDL. Waits for the services, so never on the
    /// interface's thread.
    pub fn read(luna: Option<&Luna>, panel: Option<(u32, u32)>) -> WebosFacts {
        WebosFacts::from_sources(&read_sources(Path::new("/"), luna, panel))
    }

    /// Applies `facts.ts`' rules to what the readers found.
    pub fn from_sources(s: &Sources) -> WebosFacts {
        let config = |name: &str| s.configs.get(name);
        let platform = text(config("tv.nyx.platformVersion")).or_else(|| text(s.os_info.get("webos_release")));
        let webos = webos_of_platform(platform.as_deref()).unwrap_or(4);
        let panel = text(config("tv.hw.panelResolution")).unwrap_or_default().to_uppercase();
        let (width, height) = s.panel.unwrap_or((0, 0));
        let uhd = panel == "UD" || panel == "8K" || width >= 3840;
        let sound_output = text(s.sound.get("soundOutput"));
        let digital = text(s.sound.get("soundOutputDigital"));
        let arc_atmos = s.arc.get("arcProfile").and_then(Value::as_str) == Some("ATMOS")
            || s.arc.get("earcATMOS") == Some(&Value::Bool(true));
        let tv_atmos = truthy(config("tv.config.supportDolbyTVATMOS"))
            || text(config("tv.model.soundModeType")).as_deref() == Some("Dolby Atmos");
        let to_receiver =
            sound_output.as_deref().is_some_and(|o| o.starts_with("external_arc") && o != "external_arc_sound_alive");
        let device_model = ["modelName", "model_name", "device_name"].iter().find_map(|k| text(s.device_info.get(*k)));
        WebosFacts {
            webos,
            release: platform.clone(),
            build: text(s.os_info.get("core_os_release")),
            model: text(config("tv.model.modelname")).or(device_model).unwrap_or_else(|| "LG TV".into()),
            firmware: text(config("tv.nyx.firmwareVersion")).or_else(|| text(s.os_info.get("webos_release"))),
            screen: if width > 0 && height > 0 {
                Some((width, height))
            } else if uhd {
                Some((3840, 2160))
            } else {
                None
            },
            uhd,
            uhd8k: panel == "8K",
            oled: text(config("tv.hw.displayType")).as_deref() == Some("OLED"),
            hdr10: truthy(config("tv.model.supportHDR")),
            dolby_vision: truthy(config("tv.config.supportDolbyHDRContents")),
            atmos: if to_receiver {
                arc_atmos && matches!(digital.as_deref(), Some("auto" | "passThrough"))
            } else {
                tv_atmos
            },
            sound_output,
            digital_output: digital,
            edid: text(config("tv.model.edidType")),
            memory: text(config("tv.hw.ddrSize")),
            codecs: s.codecs.clone(),
            clock_24h: clock_24h(&s.locale),
        }
    }

    /// One line naming the set and its firmware, for the log at start.
    pub fn identity(&self) -> String {
        format!(
            "LG {} · webOS {} (release {}, build {}, firmware {}) · {}{}{} · memory {}",
            self.model,
            self.webos,
            self.release.as_deref().unwrap_or("?"),
            self.build.as_deref().unwrap_or("?"),
            self.firmware.as_deref().unwrap_or("?"),
            if self.uhd8k {
                "8K"
            } else if self.uhd {
                "4K"
            } else {
                "HD"
            },
            if self.oled { " OLED" } else { "" },
            if self.codecs.is_some() { " · codec table read" } else { "" },
            self.memory.as_deref().unwrap_or("?"),
        )
    }
}

/// Reads every source under `root` (`/` on the TV; a folder of samples in tests).
pub fn read_sources(root: &Path, luna: Option<&Luna>, panel: Option<(u32, u32)>) -> Sources {
    let file = |path: &str| read_bounded(&root.join(path.trim_start_matches('/')));
    let json = |path: &str| file(path).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let mut s = Sources {
        os_info: json(OS_INFO),
        device_info: json(DEVICE_INFO),
        panel,
        codecs: file(CODEC_TABLE).and_then(|t| CodecTable::parse(&t)),
        ..Sources::default()
    };
    if let Some(luna) = luna {
        let configs =
            luna.call("luna://com.webos.service.config/getConfigs", &json!({ "configNames": CONFIGS }), CALL_TIMEOUT);
        let sound = luna.call(
            "luna://com.webos.settingsservice/getSystemSettings",
            &json!({ "category": "sound", "keys": ["soundOutput", "soundOutputDigital"] }),
            CALL_TIMEOUT,
        );
        let arc = luna.call("luna://com.webos.service.arccontroller/getARCState", &json!({}), CALL_TIMEOUT);
        let locale = luna.call(
            "luna://com.webos.settingsservice/getSystemSettings",
            &json!({ "keys": ["localeInfo"] }),
            CALL_TIMEOUT,
        );
        let field = |answer: Result<Value, _>, key: &str| answer.ok().and_then(|mut v| v.get_mut(key).map(Value::take));
        s.configs = field(configs.wait(), "configs").unwrap_or_default();
        s.sound = field(sound.wait(), "settings").unwrap_or_default();
        s.arc = arc.wait().unwrap_or_default();
        s.locale = field(locale.wait(), "settings").unwrap_or_default();
    }
    s
}

fn read_bounded(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(MAX_FILE_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

/// The webOS version of a platform version: `4.4.3` is webOS 4, `8.2.0` is
/// webOS 23, `10.2.0` webOS 25 (`facts.ts`' `webosOfPlatform`). Some sets
/// run a newer webOS than their year's (Re:New), so the running version
/// decides, never the year.
pub fn webos_of_platform(version: Option<&str>) -> Option<u32> {
    let digits: String = version?.chars().take_while(char::is_ascii_digit).collect();
    let rest = &version?[digits.len()..];
    if digits.is_empty() || !rest.starts_with('.') {
        return None;
    }
    let major: u32 = digits.parse().ok()?;
    match major {
        7..=11 => Some(major + 15),
        3..=6 => Some(major),
        _ => None,
    }
}

/// `facts.ts`' `str`: a non-empty string, or a number written as JavaScript writes it.
fn text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
            _ => n.to_string(),
        }),
        _ => None,
    }
}

/// `facts.ts`' `bool`: `true`, `"true"`, `"TRUE"` or `1`.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s == "true" || s == "TRUE",
        Some(Value::Number(n)) => n.as_f64() == Some(1.0),
        _ => false,
    }
}

/// Regions whose custom is a 12-hour clock, for a TV whose clock setting
/// follows its locale.
const TWELVE_HOUR_REGIONS: [&str; 15] =
    ["US", "CA", "AU", "NZ", "IN", "PK", "BD", "PH", "EG", "SA", "KR", "TW", "MY", "CO", "MX"];

/// Times of day as `21:05`: the TV's clock setting (`localeInfo.clock`, `12`
/// or `24`), else the custom of its interface's region (`en-US` is 12-hour).
pub fn clock_24h(locale_settings: &Value) -> bool {
    let info = locale_settings.get("localeInfo").unwrap_or(locale_settings);
    match info.get("clock").and_then(Value::as_str) {
        Some("24") => return true,
        Some("12") => return false,
        _ => {}
    }
    let ui = info.pointer("/locales/UI").and_then(Value::as_str).unwrap_or("");
    let region = ui.split(['-', '_']).nth(1).unwrap_or("");
    let lang = ui.split(['-', '_']).next().unwrap_or("");
    !(TWELVE_HOUR_REGIONS.contains(&region) || (lang == "ar" && !ui.is_empty()))
}

/// The URI player's codec table: per video codec the
/// largest picture, frame rate and bitrate; per audio codec the channels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodecTable {
    pub video: Vec<VideoRow>,
    pub audio: Vec<AudioRow>,
}

/// One video decoder of the table.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoRow {
    /// Normalised: `h264`, `hevc`, `mpeg2`, `vp8`, `vp9`, `av1`.
    pub name: String,
    pub max_width: u32,
    pub max_height: u32,
    /// May be fractional (59.94).
    pub max_fps: f64,
    pub max_bitrate: Option<u64>,
}

/// One audio decoder of the table.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioRow {
    /// Normalised: `mpeg`, `aac`, `ac3`, `eac3`, `dts`, `flac`…
    pub name: String,
    pub channels: u32,
}

impl CodecTable {
    /// Reads the table; `None` when it isn't the table's shape, rather than
    /// guessing numbers.
    pub fn parse(text: &str) -> Option<CodecTable> {
        let root: Value = serde_json::from_str(text).ok()?;
        let mut table = CodecTable::default();
        for row in root.get("videoCodecs")?.as_array()? {
            let num = |k: &str| row.get(k).and_then(Value::as_f64).filter(|v| *v > 0.0);
            let read = || {
                Some(VideoRow {
                    name: codec_name(row.get("name")?.as_str()?),
                    max_width: num("maxWidth")? as u32,
                    max_height: num("maxHeight")? as u32,
                    max_fps: num("maxFrameRate")?,
                    max_bitrate: num("maxBitRate").map(|b| b as u64),
                })
            };
            // A row without its numbers is left out, not guessed.
            let Some(next) = read() else { continue };
            // One decoder can have two rows (`H.265` and `HEVC`): the smaller of each wins.
            match table.video.iter_mut().find(|r| r.name == next.name) {
                Some(r) => {
                    r.max_width = r.max_width.min(next.max_width);
                    r.max_height = r.max_height.min(next.max_height);
                    r.max_fps = r.max_fps.min(next.max_fps);
                    r.max_bitrate = match (r.max_bitrate, next.max_bitrate) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (a, b) => a.or(b),
                    };
                }
                None => table.video.push(next),
            }
        }
        if let Some(rows) = root.get("audioCodecs").and_then(Value::as_array) {
            for row in rows {
                let (Some(name), Some(channels)) =
                    (row.get("name").and_then(Value::as_str), row.get("channels").and_then(Value::as_u64))
                else {
                    continue;
                };
                let name = codec_name(name);
                match table.audio.iter_mut().find(|r| r.name == name) {
                    Some(r) => r.channels = r.channels.min(channels as u32),
                    None => table.audio.push(AudioRow { name, channels: channels as u32 }),
                }
            }
        }
        Some(table)
    }

    /// The row for an ffprobe codec name (`h264`, `hevc`, `mpeg2video`, `vp9`, `av1`).
    pub fn video(&self, ffprobe: &str) -> Option<&VideoRow> {
        let name = match ffprobe {
            "mpeg2video" => "mpeg2".to_string(),
            other => codec_name(other),
        };
        self.video.iter().find(|r| r.name == name)
    }

    /// Diagnostics' line: `h264 4096×2304@60, hevc …`.
    pub fn line(&self) -> String {
        let video: Vec<String> =
            self.video.iter().map(|r| format!("{} {}×{}@{}", r.name, r.max_width, r.max_height, r.max_fps)).collect();
        let audio: Vec<String> = self.audio.iter().map(|r| format!("{} {} ch", r.name, r.channels)).collect();
        format!("Pipeline decoders: {}; {}", video.join(", "), audio.join(", "))
    }
}

/// Lower case without `.` and `-`; H.265 is HEVC.
fn codec_name(raw: &str) -> String {
    let name: String = raw.chars().filter(|c| *c != '.' && *c != '-').collect::<String>().to_lowercase();
    if name == "h265" { "hevc".into() } else { name }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_the_webos_version_by_the_platform_version() {
        assert_eq!(webos_of_platform(Some("4.4.3")), Some(4));
        assert_eq!(webos_of_platform(Some("8.2.0")), Some(23));
        assert_eq!(webos_of_platform(Some("10.3.1")), Some(25));
        assert_eq!(webos_of_platform(Some("11.1.0")), Some(26));
        assert_eq!(webos_of_platform(Some("6.5.3")), Some(6));
        assert_eq!(webos_of_platform(Some("12.0.0")), None);
        assert_eq!(webos_of_platform(Some("x4.1")), None);
        assert_eq!(webos_of_platform(Some("4")), None);
        assert_eq!(webos_of_platform(None), None);
    }

    fn sample() -> Sources {
        Sources {
            os_info: json!({ "webos_release": "8.2.0", "core_os_release": "8.2.0-27", "webos_name": "webOS TV" }),
            device_info: json!({ "model_name": "OLED65C3PUA" }),
            configs: json!({
                "tv.model.modelname": "OLED65C3PUA",
                "tv.nyx.firmwareVersion": "03.20.65",
                "tv.nyx.platformVersion": "8.2.0",
                "tv.hw.panelResolution": "ud",
                "tv.hw.displayType": "OLED",
                "tv.hw.ddrSize": "3G",
                "tv.model.supportHDR": true,
                "tv.config.supportDolbyHDRContents": "TRUE",
                "tv.config.supportDolbyTVATMOS": 1,
                "tv.model.edidType": "TrueHD+dts",
            }),
            sound: json!({ "soundOutput": "external_arc", "soundOutputDigital": "passThrough" }),
            arc: json!({ "returnValue": true, "arcProfile": "ATMOS" }),
            locale: json!({ "localeInfo": { "locales": { "UI": "en-US" }, "clock": "locale" } }),
            panel: Some((3840, 2160)),
            codecs: None,
        }
    }

    #[test]
    fn reads_a_2023_oled_as_the_web_app_would() {
        let f = WebosFacts::from_sources(&sample());
        assert_eq!(f.webos, 23);
        assert_eq!(f.model, "OLED65C3PUA");
        assert_eq!(f.firmware.as_deref(), Some("03.20.65"));
        assert_eq!(f.build.as_deref(), Some("8.2.0-27"));
        assert_eq!(f.screen, Some((3840, 2160)));
        assert!(f.uhd && !f.uhd8k && f.oled && f.hdr10 && f.dolby_vision);
        // To the receiver over ARC, which reports Atmos, with the output passing through.
        assert!(f.atmos);
        assert_eq!(f.edid.as_deref(), Some("TrueHD+dts"));
        assert_eq!(f.memory.as_deref(), Some("3G"));
        assert!(!f.clock_24h);
    }

    #[test]
    fn atmos_follows_where_the_sound_goes() {
        let mut s = sample();
        s.sound = json!({ "soundOutput": "external_arc", "soundOutputDigital": "pcm" });
        assert!(!WebosFacts::from_sources(&s).atmos);
        s.sound = json!({ "soundOutput": "tv_speaker" });
        assert!(WebosFacts::from_sources(&s).atmos, "the TV decodes Atmos to its speakers");
        s.configs["tv.config.supportDolbyTVATMOS"] = json!(false);
        assert!(!WebosFacts::from_sources(&s).atmos);
        s.configs["tv.model.soundModeType"] = json!("Dolby Atmos");
        assert!(WebosFacts::from_sources(&s).atmos);
        s.sound = json!({ "soundOutput": "external_arc_sound_alive", "soundOutputDigital": "auto" });
        s.arc = json!({ "earcATMOS": true });
        assert!(WebosFacts::from_sources(&s).atmos, "sound alive isn't a receiver: the TV's own rule applies");
    }

    #[test]
    fn falls_back_to_nyx_and_safe_defaults() {
        let s = Sources {
            os_info: json!({ "webos_release": "4.10.2", "core_os_release": "4.10.2-31" }),
            device_info: json!({ "modelName": "43UM7300PUA" }),
            ..Sources::default()
        };
        let f = WebosFacts::from_sources(&s);
        assert_eq!((f.webos, f.model.as_str(), f.firmware.as_deref()), (4, "43UM7300PUA", Some("4.10.2")));
        assert!(!f.uhd && !f.hdr10 && !f.dolby_vision && !f.atmos);
        assert_eq!(f.screen, None);
        let none = WebosFacts::default();
        assert_eq!((none.webos, none.model.as_str(), none.sound_output.clone()), (4, "LG TV", None));
        assert!(none.clock_24h);
    }

    #[test]
    fn an_8k_panel_and_numbers_as_strings() {
        let s = Sources {
            configs: json!({ "tv.hw.panelResolution": "8K", "tv.nyx.platformVersion": 5.3, "tv.hw.ddrSize": 3 }),
            ..Sources::default()
        };
        let f = WebosFacts::from_sources(&s);
        assert!(f.uhd && f.uhd8k);
        assert_eq!(f.screen, Some((3840, 2160)));
        assert_eq!(f.webos, 5);
        assert_eq!(f.memory.as_deref(), Some("3"));
    }

    #[test]
    fn the_clock_follows_the_setting_then_the_region() {
        assert!(clock_24h(&json!({ "localeInfo": { "clock": "24", "locales": { "UI": "en-US" } } })));
        assert!(!clock_24h(&json!({ "localeInfo": { "clock": "12", "locales": { "UI": "de-DE" } } })));
        assert!(clock_24h(&json!({ "localeInfo": { "locales": { "UI": "en-GB" } } })));
        assert!(!clock_24h(&json!({ "localeInfo": { "locales": { "UI": "ko-KR" } } })));
        assert!(!clock_24h(&json!({ "localeInfo": { "locales": { "UI": "ar-AE" } } })));
        assert!(clock_24h(&json!({ "localeInfo": { "locales": { "UI": "fr_FR" } } })));
        assert!(clock_24h(&Value::Null));
    }

    #[test]
    fn reads_the_codec_table_merging_one_decoders_two_rows() {
        let text = r#"{ "version" : "1.0", "license" : "x",
            "videoCodecs" : [
                { "name" : "H.264", "maxWidth" : 4096, "maxHeight" : 2304, "maxFrameRate" : 60, "maxBitRate" : 50 },
                { "name" : "H.265", "maxWidth" : 4096, "maxHeight" : 2304, "maxFrameRate" : 120, "maxBitRate" : 50 },
                { "name" : "HEVC", "maxWidth" : 4096, "maxHeight" : 2176, "maxFrameRate" : 59.94 },
                { "name" : "MPEG2", "maxWidth" : 1920, "maxHeight" : 1088, "maxFrameRate" : 30, "maxBitRate" : 40 },
                { "name" : "VP9", "maxWidth" : 4096, "maxHeight" : 2304, "maxFrameRate" : 60 },
                { "name" : "broken" }
            ],
            "audioCodecs" : [ { "name" : "AC3", "channels" : 6 }, { "name" : "E-AC3", "channels" : 8 } ] }"#;
        let table = CodecTable::parse(text).unwrap();
        let hevc = table.video("hevc").unwrap();
        assert_eq!((hevc.max_width, hevc.max_height, hevc.max_fps, hevc.max_bitrate), (4096, 2176, 59.94, Some(50)));
        assert_eq!(table.video("mpeg2video").unwrap().max_height, 1088);
        assert_eq!(table.video("h264").unwrap().max_fps, 60.0);
        assert!(table.video("av1").is_none());
        assert_eq!(
            table.audio,
            [AudioRow { name: "ac3".into(), channels: 6 }, AudioRow { name: "eac3".into(), channels: 8 }]
        );
        assert!(table.line().starts_with("Pipeline decoders: h264 4096×2304@60, hevc 4096×2176@59.94"));
        assert_eq!(CodecTable::parse("{}"), None);
        assert_eq!(CodecTable::parse("not json"), None);
    }

    #[test]
    fn reads_the_files_from_a_folder() {
        let root = pley_core::device::scratch_dir("facts");
        std::fs::create_dir_all(root.join("var/run/nyx")).unwrap();
        std::fs::create_dir_all(root.join("etc/umediaserver")).unwrap();
        std::fs::write(
            root.join("var/run/nyx/os_info.json"),
            r#"{"webos_release":"6.5.3","core_os_release":"6.5.3-47"}"#,
        )
        .unwrap();
        std::fs::write(root.join("var/run/nyx/device_info.json"), r#"{"device_name":"OLED55C1"}"#).unwrap();
        std::fs::write(
            root.join("etc/umediaserver/device_codec_capability_config.json"),
            r#"{"videoCodecs":[{"name":"AV1","maxWidth":3840,"maxHeight":2160,"maxFrameRate":60}]}"#,
        )
        .unwrap();
        let f = WebosFacts::from_sources(&read_sources(&root, None, Some((3840, 2160))));
        assert_eq!((f.webos, f.model.as_str(), f.build.as_deref()), (6, "OLED55C1", Some("6.5.3-47")));
        assert!(f.uhd);
        assert_eq!(f.codecs.as_ref().unwrap().video("av1").unwrap().max_width, 3840);
        assert!(f.identity().contains("webOS 6"));
    }
}
