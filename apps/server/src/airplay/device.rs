//! What the shared layer for devices on the network ([`crate::nearby`])
//! asks of AirPlay: which stream a device gets, whether Pley may play on
//! it yet, and how its player is reached: paired, or transiently where
//! the device is open to everyone on the network.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::net::TcpStream;

use super::connection::{Channel, Keyed};
use super::pairing::{self, Failed, Identity};
use super::pairings;
use super::player::Session;
use crate::nearby::Found;
use crate::nearby::casts::Choice;
use crate::nearby::player::{Connected, Failure};
use crate::state::AppState;

/// Devices remembered as open to everyone on the network, at most.
const MAX_KNOWN: usize = 256;
/// How often a device may be asked to show a PIN: each ask shows a new
/// one on the TV.
const ASK_AGAIN: Duration = Duration::from_secs(3);

/// What Pley learned of AirPlay devices.
#[derive(Default)]
pub struct Known {
    /// The devices that took a transient pairing, by id, until the server
    /// restarts: they need no PIN.
    open: Mutex<HashSet<String>>,
    /// When each device was last asked to show a PIN.
    asked: Mutex<HashMap<String, Instant>>,
}

/// What it is: an Apple TV 4K, an Apple TV HD, or a TV or other receiver
/// with AirPlay built in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    AppleTv4k,
    AppleTvHd,
    Other,
}

fn kind(model: Option<&str>) -> Kind {
    let Some(rest) = model.and_then(|m| m.strip_prefix("AppleTV")) else { return Kind::Other };
    // `AppleTV5,3` is the HD; 6 and later the 4Ks.
    match rest.split(',').next().and_then(|major| major.parse::<u32>().ok()) {
        Some(major) if major >= 6 => Kind::AppleTv4k,
        _ => Kind::AppleTvHd,
    }
}

/// The caps of what a device of `kind` plays.
fn caps(kind: Kind) -> Value {
    let (width, height, hevc, hdr) = match kind {
        Kind::AppleTv4k => (3840, 2160, true, true),
        Kind::AppleTvHd | Kind::Other => (1920, 1080, false, false),
    };
    let mut video = vec![json!({
        "codec": "h264",
        "hardware": true,
        "profiles": ["Constrained Baseline", "Baseline", "Main", "High"],
        "max_width": 1920,
        "max_height": 1080,
        "max_fps": 60.0,
        "max_bit_depth": 8,
        "max_level": 42,
    })];
    let mut codecs = vec!["h264"];
    if hevc {
        codecs.push("hevc");
        video.push(json!({
            "codec": "hevc",
            "hardware": true,
            "profiles": ["Main", "Main 10"],
            "max_width": width,
            "max_height": height,
            "max_fps": 60.0,
            "max_bit_depth": 10,
            "max_level": 153,
        }));
    }
    let audio = match kind {
        Kind::AppleTv4k | Kind::AppleTvHd => vec!["aac", "ac3", "eac3"],
        Kind::Other => vec!["aac"],
    };
    json!({
        "caps_version": 2,
        "containers": ["mp4"],
        "video_codecs": codecs,
        "video": video,
        "hdr": hdr,
        "hdr_types": if hdr { vec!["hdr10", "hlg"] } else { Vec::new() },
        "audio_codecs": audio,
        "audio_decode": audio,
        "audio_passthrough": [],
        "audio_channels": if kind == Kind::Other { 2 } else { 6 },
        // Subtitles beside the stream don't reach an AirPlay player:
        // pictures are burned in, text ones aren't shown.
        "subtitle_codecs": [],
        "picture_subtitles_sideload": false,
        "hls": {"segments": if hevc { vec!["fmp4", "ts"] } else { vec!["ts"] }},
        "display": {"width": width, "height": height, "refresh_rates": [60.0]},
    })
}

/// How many digits the PIN `device` shows has, where that is known: an
/// Apple TV's are 4. A TV with AirPlay built in may ask for a password set
/// in its settings instead.
pub fn pin_digits(device: &Found) -> Option<u8> {
    match kind(device.model.as_deref()) {
        Kind::AppleTv4k | Kind::AppleTvHd => Some(4),
        Kind::Other => None,
    }
}

/// The stream `device` gets: its kind's, then the basic one.
pub fn choose(device: &Found) -> Choice {
    let kind = kind(device.model.as_deref());
    Choice { caps: caps(kind), basic: (kind != Kind::Other).then(|| caps(Kind::Other)), model: device.model.clone() }
}

/// Whether Pley may play on `device`: it was paired, or it takes a
/// transient pairing (open to everyone on the network). `false`: it wants a
/// PIN.
pub async fn ready(state: &AppState, device: &Found) -> Result<bool, Failure> {
    if state.nearby.airplay.open.lock().contains(&device.id) {
        return Ok(true);
    }
    if pairings::get(&state.db, &device.id).await.ok().flatten().is_some() {
        return Ok(true);
    }
    let mut channel = channel(device).await?;
    match pairing::transient(&mut channel).await {
        Ok(_) => {
            remember_open(state, device);
            Ok(true)
        }
        Err(Failed::WantsPin) | Err(Failed::WrongPin) => Ok(false),
        Err(err) => {
            tracing::warn!("couldn't pair with the AirPlay device {}: {err}", device.address);
            Err(Failure::Lost)
        }
    }
}

fn remember_open(state: &AppState, device: &Found) {
    let mut open = state.nearby.airplay.open.lock();
    if open.len() < MAX_KNOWN {
        open.insert(device.id.clone());
    }
}

/// Asks `device` to show a PIN, for [`pair`] to prove.
pub async fn ask_for_pin(state: &AppState, device: &Found) -> Result<(), Failed> {
    {
        let mut asked = state.nearby.airplay.asked.lock();
        asked.retain(|_, at| at.elapsed() < ASK_AGAIN);
        if asked.contains_key(&device.id) {
            return Err(Failed::Wait(ASK_AGAIN));
        }
        if asked.len() < MAX_KNOWN {
            asked.insert(device.id.clone(), Instant::now());
        }
    }
    let mut channel = Channel::connect(device.address).await?;
    pairing::ask_for_pin(&mut channel).await
}

/// Pairs the server with `device`, with the `pin` it shows, for good:
/// `user_id` typed it. A pairing before it is replaced.
pub async fn pair(state: &AppState, user_id: i64, device: &Found, pin: &str) -> Result<(), Failed> {
    let identity = Identity::load(&state.config.data_dir).ok_or(Failed::Refused("no key to pair with"))?;
    let mut channel = Channel::connect(device.address).await?;
    let paired = pairing::pair(&mut channel, &identity, pin).await?;
    pairings::save(&state.db, &device.id, &paired, &device.name, user_id).await.map_err(|err| {
        tracing::warn!("couldn't keep a pairing with an AirPlay device: {err}");
        Failed::Refused("not kept")
    })?;
    state.nearby.airplay.open.lock().remove(&device.id);
    tracing::info!("paired with the AirPlay device {}", device.address);
    Ok(())
}

async fn channel(device: &Found) -> Result<Channel<TcpStream>, Failure> {
    Channel::connect(device.address).await.map_err(|err| {
        tracing::warn!("couldn't connect to the AirPlay device {}: {err}", device.address);
        Failure::Lost
    })
}

/// Connects to `device`, paired or transiently, and sets up a session to
/// play in.
pub async fn connect(state: &AppState, device: &Found) -> Result<Connected, Failure> {
    let identity = Identity::load(&state.config.data_dir).ok_or_else(|| {
        tracing::warn!("AirPlay devices need the server's secret.key, which can't be read");
        Failure::NotStarted("no key to pair with".into())
    })?;
    let paired = pairings::get(&state.db, &device.id).await.ok().flatten();
    let mut channel = channel(device).await?;
    let secret = match &paired {
        Some(paired) => pairing::verify(&mut channel, &identity, &paired.public_key).await,
        None => pairing::transient(&mut channel).await,
    };
    match &secret {
        // Unpaired on the device, which proved who it is first.
        Err(Failed::Forgotten) => {
            let _ = pairings::remove(&state.db, &device.id).await;
        }
        // Open to everyone before, it wants a PIN now: [`ready`] asks for
        // pairing from here on.
        Err(Failed::WantsPin | Failed::WrongPin) if paired.is_none() => {
            state.nearby.airplay.open.lock().remove(&device.id);
        }
        _ => {}
    }
    let secret = secret.map_err(|err| {
        tracing::warn!("couldn't pair with the AirPlay device {}: {err}", device.address);
        failure(err, paired.is_some())
    })?;
    channel.encrypt(&secret, Keyed::Control);
    match Session::open(channel, &secret, device.address.ip(), &identity.id).await {
        Ok(session) => Ok(Connected::AirPlay(Box::new(session))),
        Err(err) => {
            tracing::warn!("couldn't set up a session with the AirPlay device {}: {err}", device.address);
            Err(Failure::NotStarted(err.to_string()))
        }
    }
}

/// Why a cast to a device can't play, when connecting to it failed with
/// `err`, `paired` or transiently.
fn failure(err: Failed, paired: bool) -> Failure {
    match err {
        Failed::Io(_) => Failure::Lost,
        // A device that signs with another key keeps its pairing, since
        // nothing proved it the same device: a new pairing replaces it.
        Failed::Forgotten | Failed::OtherKey => Failure::NeedsPairing,
        // A device that took a transient pairing before wants a PIN now.
        Failed::WantsPin | Failed::WrongPin if !paired => Failure::NeedsPairing,
        other => Failure::NotStarted(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_that_wants_a_pin_now_asks_for_pairing() {
        assert_eq!(failure(Failed::WantsPin, false), Failure::NeedsPairing);
        assert_eq!(failure(Failed::WrongPin, false), Failure::NeedsPairing);
        assert_eq!(failure(Failed::Forgotten, true), Failure::NeedsPairing);
        assert_eq!(failure(Failed::OtherKey, true), Failure::NeedsPairing);
        assert_eq!(failure(Failed::Io(std::io::ErrorKind::TimedOut.into()), false), Failure::Lost);
        assert!(matches!(failure(Failed::Refused("an unexpected status"), false), Failure::NotStarted(_)));
    }

    #[tokio::test]
    async fn a_tv_open_before_that_wants_a_pin_now_is_paired_first() {
        // A TV that answers every pairing as Apple TVs do when they want a
        // PIN.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut channel = Channel::over(stream);
                    while channel.read().await.is_ok() {
                        if channel.reply("HTTP/1.1 470 Connection Authorization Required", &[], &[]).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        let dir = crate::testing::tempdir();
        let mut config = crate::config::Config::for_data_dir(dir.path());
        config.transcoder_path = Some(dir.path().join("no-transcoder-in-tests"));
        let state = AppState::init(config).await.unwrap();
        let tv = Found {
            id: "a1b2c3d4e5f6".into(),
            name: "TV".into(),
            model: None,
            address,
            video: true,
            protocol: crate::nearby::Protocol::AirPlay,
        };
        // It took a transient pairing once, so it is taken as open.
        remember_open(&state, &tv);
        assert_eq!(ready(&state, &tv).await, Ok(true));
        // Now it wants a PIN: the cast asks for pairing, and so do the next.
        assert!(matches!(connect(&state, &tv).await, Err(Failure::NeedsPairing)));
        assert!(!state.nearby.airplay.open.lock().contains(&tv.id));
        assert_eq!(ready(&state, &tv).await, Ok(false));
    }

    #[test]
    fn an_apple_tv_4k_gets_hevc_and_4k_and_others_the_basic_stream() {
        assert_eq!(kind(Some("AppleTV14,1")), Kind::AppleTv4k);
        assert_eq!(kind(Some("AppleTV5,3")), Kind::AppleTvHd);
        assert_eq!(kind(Some("UN55TU8000")), Kind::Other);
        assert_eq!(kind(None), Kind::Other);
        let tv = caps(Kind::AppleTv4k);
        assert_eq!(tv["video_codecs"], json!(["h264", "hevc"]));
        assert_eq!(tv["display"]["width"], 3840);
        let other = caps(Kind::Other);
        assert_eq!((other["video_codecs"].clone(), other["audio_codecs"].clone()), (json!(["h264"]), json!(["aac"])));
        assert_eq!(other["hls"]["segments"], json!(["ts"]));
    }

    #[test]
    fn an_apple_tv_s_pin_has_4_digits_and_another_tv_s_isnt_known() {
        let device = |model: Option<&str>| Found {
            id: "a1b2c3d4e5f6".into(),
            name: "TV".into(),
            model: model.map(str::to_owned),
            address: "192.168.1.20:7000".parse().unwrap(),
            video: true,
            protocol: crate::nearby::Protocol::AirPlay,
        };
        assert_eq!(pin_digits(&device(Some("AppleTV5,3"))), Some(4));
        assert_eq!(pin_digits(&device(Some("AppleTV14,1"))), Some(4));
        assert_eq!(pin_digits(&device(Some("UN55TU8000"))), None);
        assert_eq!(pin_digits(&device(None)), None);
    }
}
