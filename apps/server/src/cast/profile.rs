//! What a Cast device is sent: a profile picked by its model. A Cast device
//! doesn't say what it decodes (only a receiver app running on it can ask,
//! and the server uses Google's), so the profile comes from what Google
//! publishes of its own devices, and from the device's local setup API,
//! which tells the hardware apart and whether it runs Google TV or Android
//! TV. That API is Google's own and undocumented: it is a hint, asked
//! briefly the first time a device is cast to (and again later when it
//! didn't answer), and a device that doesn't answer is known by its mDNS
//! model alone. A device that refuses what its profile sent is sent the
//! basic profile instead ([`super::devices`]), and its model remembered.

use std::net::SocketAddr;
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::{Value, json};

/// The setup API's port, and how long a cast waits for it.
pub const SETUP_PORT: u16 = 8008;
const ASK_WITHIN: Duration = Duration::from_secs(1);
/// The most of its answer read.
const MAX_ANSWER: usize = 64 * 1024;

/// What a cast's stream may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// HEVC, beside H.264.
    pub hevc: bool,
    /// The most lines a picture has: 1080 or 2160.
    pub lines: u32,
    /// HDR10 as it is.
    pub hdr: bool,
}

impl Profile {
    /// What every Cast device plays: H.264 at 1080p, SDR.
    pub const BASIC: Profile = Profile { hevc: false, lines: 1080, hdr: false };

    /// The caps of a play request for this profile: MP4 for a direct play,
    /// HLS (MPEG-TS, or fMP4 with HEVC, which MPEG-TS doesn't carry to
    /// Google's player) otherwise, and stereo AAC or MP3 always, as a
    /// device on an SDR TV or a soundbar plays them. Text subtitles come
    /// beside, as WebVTT, and picture ones are burned in.
    pub fn caps(&self) -> Value {
        let (width, height) = if self.lines >= 2160 { (3840, 2160) } else { (1920, 1080) };
        let mut video = vec![json!({
            "codec": "h264",
            "hardware": true,
            "profiles": ["Constrained Baseline", "Baseline", "Main", "High"],
            "max_width": width,
            "max_height": height,
            "max_fps": 60.0,
            "max_bit_depth": 8,
            "max_level": if self.lines >= 2160 { 51 } else { 42 },
        })];
        let mut codecs = vec!["h264"];
        if self.hevc {
            codecs.push("hevc");
            video.push(json!({
                "codec": "hevc",
                "hardware": true,
                "profiles": ["Main", "Main 10"],
                "max_width": width,
                "max_height": height,
                "max_fps": 60.0,
                "max_bit_depth": 10,
                "max_level": if self.lines >= 2160 { 153 } else { 123 },
            }));
        }
        json!({
            "caps_version": 2,
            "containers": ["mp4"],
            "video_codecs": codecs,
            "video": video,
            "hdr": self.hdr,
            "hdr_types": if self.hdr { vec!["hdr10"] } else { Vec::new() },
            "audio_codecs": ["aac", "mp3"],
            "audio_decode": ["aac", "mp3"],
            "audio_passthrough": [],
            "audio_channels": 2,
            "subtitle_codecs": ["subrip", "webvtt", "ass", "ssa", "mov_text", "text"],
            "picture_subtitles_sideload": false,
            "hls": {"segments": if self.hevc { vec!["fmp4", "ts"] } else { vec!["ts"] }},
            "display": {"width": width, "height": height, "refresh_rates": [60.0]},
        })
    }
}

/// What the device's setup API says of it, when it answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hardware {
    /// "Chromecast", "Google TV Streamer", a TV's.
    pub model: Option<String>,
    /// The hardware's own name: "sabrina" for Chromecast with Google TV
    /// (4K), "boreal" for its HD one, "kirkwood" for Google TV Streamer.
    pub product: Option<String>,
    /// Its owner turned 4K off.
    pub no_4k: bool,
    /// It runs Google TV or Android TV (it takes Cast Connect, which only
    /// they do).
    pub android_tv: bool,
}

/// The answer of the setup API at `address` (the device's, on
/// [`SETUP_PORT`]), read within bounds, or `None` when it doesn't answer in
/// time or says something else.
pub async fn ask(address: SocketAddr) -> Option<Hardware> {
    static HTTP: LazyLock<Option<reqwest::Client>> = LazyLock::new(|| {
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ASK_WITHIN)
            .build()
            .ok()
    });
    let http = HTTP.as_ref()?;
    let url = format!("http://{address}/setup/eureka_info?params=device_info");
    let mut response = http.get(url).send().await.ok().filter(|r| r.status().is_success())?;
    let mut body = Vec::new();
    while let Some(chunk) = tokio::time::timeout(ASK_WITHIN, response.chunk()).await.ok()?.ok()? {
        if body.len() + chunk.len() > MAX_ANSWER {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    read(&serde_json::from_slice(&body).ok()?)
}

/// The parts of an answer used; the rest (its keys, its addresses) is
/// neither kept nor logged.
fn read(answer: &Value) -> Option<Hardware> {
    let info = answer.get("device_info").filter(|i| i.is_object())?;
    let text = |key: &str| {
        let s = info[key].as_str()?;
        let s: String = s.chars().filter(|c| !c.is_control()).take(100).collect();
        let s = s.trim();
        (!s.is_empty()).then(|| s.to_owned())
    };
    Some(Hardware {
        model: text("model_name"),
        product: text("product_name"),
        no_4k: info["4k_blocked"].as_i64().is_some_and(|b| b != 0),
        android_tv: info["capabilities"]["cast_connect_supported"].as_bool() == Some(true),
    })
}

/// How the device is known for [`pick`] and for remembering a refusal: its
/// hardware's name, else its model.
pub fn identity(model: Option<&str>, hardware: Option<&Hardware>) -> Option<String> {
    let hardware = hardware.and_then(|h| h.product.as_deref().or(h.model.as_deref()));
    hardware.or(model).map(str::to_lowercase)
}

/// The profile for a device of mDNS model `model`, with what its setup API
/// said. The models Google lists as playing HEVC, 4K and HDR10 get them;
/// Google TV and Android TV, nearly all of which decode HEVC, get it up to
/// 4K, in SDR (an HDR stream on a screen without HDR plays, washed out,
/// rather than failing, so it is never guessed); everything else gets the
/// basic profile.
pub fn pick(model: Option<&str>, hardware: Option<&Hardware>) -> Profile {
    let names: Vec<String> =
        [model, hardware.and_then(|h| h.model.as_deref()), hardware.and_then(|h| h.product.as_deref())]
            .into_iter()
            .flatten()
            .map(str::to_lowercase)
            .collect();
    let named = |known: &[&str]| names.iter().any(|n| known.contains(&n.as_str()));
    let profile = if named(&["chromecast ultra", "google tv streamer", "kirkwood", "sabrina"]) {
        Profile { hevc: true, lines: 2160, hdr: true }
    } else if named(&["chromecast hd", "boreal"]) {
        Profile { hevc: true, lines: 1080, hdr: false }
    } else if hardware.is_some_and(|h| h.android_tv) {
        Profile { hevc: true, lines: 2160, hdr: false }
    } else {
        Profile::BASIC
    };
    match hardware {
        Some(h) if h.no_4k => Profile { lines: profile.lines.min(1080), ..profile },
        _ => profile,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware(model: &str, product: &str, android_tv: bool) -> Hardware {
        Hardware { model: Some(model.into()), product: Some(product.into()), no_4k: false, android_tv }
    }

    #[test]
    fn a_profile_is_picked_by_model() {
        let uhd = Profile { hevc: true, lines: 2160, hdr: true };
        assert_eq!(pick(Some("Chromecast Ultra"), None), uhd);
        assert_eq!(pick(Some("Google TV Streamer"), None), uhd);
        // Chromecast with Google TV calls itself "Chromecast" over mDNS:
        // its hardware tells it apart from the first Chromecasts.
        assert_eq!(pick(Some("Chromecast"), Some(&hardware("Chromecast", "sabrina", true))), uhd);
        assert_eq!(
            pick(Some("Chromecast HD"), Some(&hardware("Chromecast HD", "boreal", true))),
            Profile { hevc: true, lines: 1080, hdr: false }
        );
        assert_eq!(pick(Some("Chromecast"), None), Profile::BASIC);
        assert_eq!(pick(Some("Chromecast"), Some(&hardware("Chromecast", "unknown", false))), Profile::BASIC);
        // A TV with Google TV: HEVC, never HDR.
        let tv = hardware("BRAVIA 4K VH2", "BRAVIA_VH2_M_EU", true);
        assert_eq!(pick(Some("BRAVIA 4K VH2"), Some(&tv)), Profile { hevc: true, lines: 2160, hdr: false });
        // An unknown TV, and one whose owner turned 4K off.
        assert_eq!(pick(Some("LG webOS TV"), None), Profile::BASIC);
        let capped = Hardware { no_4k: true, ..hardware("Google TV Streamer", "kirkwood", true) };
        assert_eq!(pick(None, Some(&capped)), Profile { hevc: true, lines: 1080, hdr: true });
    }

    #[test]
    fn what_the_setup_api_says_is_read_within_bounds() {
        let answer = json!({"device_info": {
            "4k_blocked": 0,
            "capabilities": {"cast_connect_supported": true, "display_supported": true},
            "model_name": "Chromecast\u{7}",
            "product_name": "sabrina",
            "mac_address": "00:00:00:00:00:00",
        }});
        assert_eq!(read(&answer), Some(hardware("Chromecast", "sabrina", true)));
        let long = json!({"device_info": {"model_name": "x".repeat(500), "4k_blocked": 1}});
        let read_long = read(&long).unwrap();
        assert_eq!((read_long.model.unwrap().len(), read_long.no_4k, read_long.product), (100, true, None));
        assert_eq!(read(&json!({"device_info": "?"})), None);
        assert_eq!(read(&json!([1, 2])), None);
        assert_eq!(
            identity(Some("Chromecast"), Some(&hardware("Chromecast", "Sabrina", true))),
            Some("sabrina".into())
        );
        assert_eq!(identity(Some("Chromecast"), None), Some("chromecast".into()));
    }

    #[test]
    fn hevc_goes_in_fmp4_segments() {
        let basic = Profile::BASIC.caps();
        assert_eq!((basic["video_codecs"].clone(), basic["hls"]["segments"].clone()), (json!(["h264"]), json!(["ts"])));
        assert_eq!(basic["video"][0]["max_height"], 1080);
        let uhd = Profile { hevc: true, lines: 2160, hdr: true }.caps();
        assert_eq!(uhd["hls"]["segments"], json!(["fmp4", "ts"]));
        assert_eq!((uhd["video"][1]["codec"].clone(), uhd["hdr_types"].clone()), (json!("hevc"), json!(["hdr10"])));
    }
}
