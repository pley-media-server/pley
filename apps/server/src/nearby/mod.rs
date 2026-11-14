//! Devices on the server's network that Pley plays on itself, as no Pley
//! app runs there: Google Cast and AirPlay devices, by the protocol each
//! speaks. The server finds them, lists them for the household's remotes,
//! and plays a cast on one as a playback session of its own ([`casts`]),
//! the device fetching the session's keyed stream. What is said to a
//! device is the protocol's own ([`crate::cast`], [`crate::airplay`]).

pub mod casts;
pub mod discovery;
pub mod player;

use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

/// How long a look for devices waits for answers, how long one is good
/// for, and how long a device that stops answering stays listed.
const LOOK_FOR: Duration = Duration::from_millis(1500);
const LOOK_AGAIN: Duration = Duration::from_secs(30);
const KEEP_FOR: Duration = Duration::from_secs(5 * 60);

/// Models remembered as refusing their stream, at most.
const MAX_KNOWN: usize = 256;

/// What a device speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Cast,
    AirPlay,
}

impl Protocol {
    pub const ALL: [Protocol; 2] = [Protocol::Cast, Protocol::AirPlay];

    /// The prefix of its devices' ids among the remotes' devices.
    pub fn prefix(self) -> &'static str {
        match self {
            Protocol::Cast => "cast:",
            Protocol::AirPlay => "airplay:",
        }
    }

    /// Its name in a remote device's `protocol`.
    pub fn word(self) -> &'static str {
        match self {
            Protocol::Cast => "cast",
            Protocol::AirPlay => "airplay",
        }
    }

    /// The protocol of a device id, prefix and all.
    pub fn of(id: &str) -> Option<Protocol> {
        Protocol::ALL.into_iter().find(|p| id.starts_with(p.prefix()))
    }

    /// The commands its devices take, for the remotes.
    pub fn controls(self) -> &'static [&'static str] {
        match self {
            Protocol::Cast => &["play", "pause", "seek", "skip", "volume"],
            // An AirPlay device keeps its volume to itself.
            Protocol::AirPlay => &["play", "pause", "seek", "skip"],
        }
    }

    /// Its players show text subtitles from beside the stream.
    pub fn shows_subtitles(self) -> bool {
        match self {
            Protocol::Cast => true,
            Protocol::AirPlay => false,
        }
    }
}

/// A device that answered a look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Its own id, the same at every look (see [`Found::key`]).
    pub id: String,
    /// The name its owner gave it ("Living room TV").
    pub name: String,
    /// "Chromecast", "Chromecast Ultra", "Google TV Streamer", a TV's.
    pub model: Option<String>,
    pub address: SocketAddr,
    /// It shows video: a speaker or a group of them doesn't.
    pub video: bool,
    pub protocol: Protocol,
}

impl Found {
    /// Its id among the remotes' devices, its protocol's prefix first:
    /// two protocols' devices never share one.
    pub fn key(&self) -> String {
        format!("{}{}", self.protocol.prefix(), self.id)
    }
}

/// The devices found, and the casts playing on them.
#[derive(Default)]
pub struct NearbyDevices {
    /// By [`Found::key`].
    found: Mutex<HashMap<String, (Found, Instant)>>,
    looked: Mutex<Option<Instant>>,
    looking: tokio::sync::Mutex<()>,
    /// The cast playing on each device, by its key: a new one ends it.
    playing: Mutex<HashMap<String, (u64, CancellationToken)>>,
    /// Held, by device, while a cast starts its session, which ends the
    /// device's last one: of two casts at once, the newer one's session is
    /// the one that stays.
    starting: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    casts: AtomicU64,
    /// The models that refused their stream: they get the basic one,
    /// until the server restarts.
    refused: Mutex<HashSet<String>>,
    /// What Pley learned of Cast devices.
    pub(crate) cast: crate::cast::device::Known,
    /// What Pley learned of AirPlay devices.
    pub(crate) airplay: crate::airplay::device::Known,
}

impl NearbyDevices {
    /// The devices that show video, looking again when the last look is
    /// old. Pley looks on the local network only while `discovery` is on
    /// (Settings → Network); turned off, the devices found are forgotten.
    /// `fixed` is the device the server was told of, which is always listed.
    pub async fn listed(&self, fixed: Option<Found>, discovery: bool) -> Vec<Found> {
        if !discovery {
            let fixed = fixed.as_ref().map(Found::key);
            self.found.lock().retain(|key, _| Some(key) == fixed.as_ref());
            *self.looked.lock() = None;
        }
        if (discovery || fixed.is_some()) && self.looked.lock().is_none_or(|at| at.elapsed() >= LOOK_AGAIN) {
            let _one = self.looking.lock().await;
            // Another request looked while this one waited.
            if self.looked.lock().is_none_or(|at| at.elapsed() >= LOOK_AGAIN) {
                self.look(fixed).await;
            }
        }
        let mut found = self.found.lock();
        found.retain(|_, (_, at)| at.elapsed() < KEEP_FOR);
        found.values().map(|(device, _)| device.clone()).filter(|d| d.video).collect()
    }

    async fn look(&self, fixed: Option<Found>) {
        let from: Vec<Ipv4Addr> =
            crate::network::adapters().into_iter().filter(|a| a.up).flat_map(|a| a.addresses).collect();
        let browse = async {
            if from.is_empty() || fixed.is_some() {
                return Ok(Vec::new());
            }
            let services = [crate::cast::discovery::SERVICE, crate::airplay::discovery::SERVICE];
            discovery::browse(&services, &from, LOOK_FOR).await
        };
        let mut seen = browse.await.unwrap_or_else(|err| {
            tracing::warn!("couldn't look for devices on the network: {err}");
            Vec::new()
        });
        seen.extend(fixed);
        one_per_address(&mut seen);
        let now = Instant::now();
        let mut found = self.found.lock();
        for device in seen {
            found.insert(device.key(), (device, now));
        }
        *self.looked.lock() = Some(now);
    }

    /// The device of `key` ([`Found::key`]) seen lately, while `discovery`
    /// is on, as [`Self::listed`] lists it.
    pub fn device(&self, key: &str, fixed: Option<Found>, discovery: bool) -> Option<Found> {
        if !discovery && fixed.is_none_or(|f| f.key() != key) {
            return None;
        }
        let found = self.found.lock();
        found.get(key).filter(|(d, at)| d.video && at.elapsed() < KEEP_FOR).map(|(d, _)| d.clone())
    }

    /// Whether `model` refused its stream before.
    fn refused(&self, model: &str) -> bool {
        self.refused.lock().contains(model)
    }

    /// Remembers that `model` refused its stream and played the basic one.
    fn remember_refused(&self, model: String) {
        let mut refused = self.refused.lock();
        if refused.len() < MAX_KNOWN {
            refused.insert(model);
        }
    }
}

/// Keeps one device of those at one address: a TV with both Google Cast
/// and AirPlay built in is listed once, as a Cast device, which plays
/// subtitles and takes its volume.
fn one_per_address(seen: &mut Vec<Found>) {
    let cast: HashSet<std::net::IpAddr> =
        seen.iter().filter(|d| d.protocol == Protocol::Cast && d.video).map(|d| d.address.ip()).collect();
    seen.retain(|d| d.protocol == Protocol::Cast || !cast.contains(&d.address.ip()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn turning_discovery_off_forgets_the_devices_found() {
        let devices = NearbyDevices::default();
        let tv = Found {
            id: "tv".into(),
            name: "Living room TV".into(),
            model: None,
            address: "192.168.1.20:8009".parse().unwrap(),
            video: true,
            protocol: Protocol::Cast,
        };
        devices.found.lock().insert(tv.key(), (tv.clone(), Instant::now()));
        *devices.looked.lock() = Some(Instant::now());
        assert_eq!(devices.device("cast:tv", None, true), Some(tv.clone()));
        assert_eq!(devices.device("cast:tv", None, false), None, "nor cast to");
        assert!(devices.listed(None, false).await.is_empty());
        assert_eq!(devices.device("cast:tv", None, true), None, "forgotten");
    }

    #[test]
    fn a_device_id_names_its_protocol() {
        assert_eq!(Protocol::of("cast:9f8e"), Some(Protocol::Cast));
        assert_eq!(Protocol::of("airplay:a1b2c3d4e5f6"), Some(Protocol::AirPlay));
        assert_eq!(Protocol::of("app-3"), None);
    }

    #[test]
    fn a_tv_with_cast_and_airplay_is_listed_once_as_a_cast_device() {
        let device = |protocol, ip: &str, port| Found {
            id: ip.into(),
            name: "Living room".into(),
            model: None,
            address: format!("{ip}:{port}").parse().unwrap(),
            video: true,
            protocol,
        };
        let mut seen = vec![
            device(Protocol::AirPlay, "192.168.1.20", 7000),
            device(Protocol::Cast, "192.168.1.20", 8009),
            device(Protocol::AirPlay, "192.168.1.21", 7000),
        ];
        one_per_address(&mut seen);
        let left: Vec<_> = seen.iter().map(|d| (d.protocol, d.address.port())).collect();
        assert_eq!(left, vec![(Protocol::Cast, 8009), (Protocol::AirPlay, 7000)]);
    }
}
