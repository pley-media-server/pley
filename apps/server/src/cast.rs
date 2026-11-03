//! Cast: playing a title on another of the profile's devices, and steering
//! it from there. A device that can be cast to says so as it opens its event
//! stream (`/api/events?cast_device=…`), and is listed for as long as that
//! stream stays open: there is nothing to sign up for or forget, and a
//! device that goes away leaves the list with its connection. What other
//! devices send it (play this, pause, seek, stop) reaches it as events on
//! that stream, and what it plays it reports back for the others to show.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::catalog::ItemSummary;
use crate::error::{ApiResult, AppError};
use crate::events::{EventBus, ServerEvent};

/// Devices one profile may have listed at once: a household's worth of
/// screens, not a list without end.
pub const MAX_DEVICES_EACH: usize = 32;

/// How long a device's name may be, in characters.
pub const NAME_MAX: usize = 64;

/// What a cast device is, for the icon senders show beside its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Web,
    Desktop,
    Android,
    AndroidTv,
    Kodi,
    Roku,
    Samsung,
    Lg,
}

impl Platform {
    pub fn from_name(name: &str) -> Option<Platform> {
        serde_json::from_value(serde_json::Value::String(name.to_owned())).ok()
    }
}

/// Whether the player is moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayState {
    Playing,
    Paused,
    /// Loading, or waiting for more of the stream.
    Buffering,
}

/// What a device last said it plays.
#[derive(Debug, Clone, Serialize)]
pub struct CastState {
    pub item: ItemSummary,
    pub position_ms: i64,
    pub duration_ms: Option<i64>,
    pub state: PlayState,
    /// How long ago it said so, for senders to move a playing position on.
    pub age_ms: u64,
}

/// One device in the profile's list.
#[derive(Debug, Clone, Serialize)]
pub struct CastDevice {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    /// `None`: it plays nothing.
    pub state: Option<CastState>,
}

/// A command for a device's player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastAction {
    Pause,
    Resume,
    Seek,
    Stop,
}

struct Reported {
    item: ItemSummary,
    position_ms: i64,
    duration_ms: Option<i64>,
    state: PlayState,
    at: Instant,
}

struct Device {
    name: String,
    platform: Platform,
    /// Its event streams open: one, but for the moments an app connects
    /// again before its old connection is noticed gone.
    streams: usize,
    reported: Option<Reported>,
}

/// The devices that may be cast to, by profile and device id.
pub struct Cast {
    devices: Mutex<HashMap<(i64, String), Device>>,
    events: EventBus,
}

impl Cast {
    pub fn new(events: EventBus) -> Arc<Cast> {
        Arc::new(Cast { devices: Mutex::new(HashMap::new()), events })
    }

    /// Lists device `id` of `user_id` while the returned guard lives (its
    /// event stream). The same id again (the app connecting again) is the
    /// same device, under the name it gives now.
    pub fn connect(self: &Arc<Self>, user_id: i64, id: &str, name: &str, platform: Platform) -> ApiResult<Receiver> {
        let id = valid_id(id)?;
        let name = valid_name(name)?;
        let changed = {
            let mut devices = self.devices.lock();
            let key = (user_id, id.clone());
            match devices.get_mut(&key) {
                Some(device) => {
                    device.streams += 1;
                    let changed = device.name != name || device.platform != platform;
                    device.name = name;
                    device.platform = platform;
                    changed
                }
                None => {
                    if devices.keys().filter(|(user, _)| *user == user_id).count() >= MAX_DEVICES_EACH {
                        return Err(AppError::TooManyRequests(format!(
                            "a profile may have {MAX_DEVICES_EACH} devices to cast to at once"
                        )));
                    }
                    devices.insert(key, Device { name, platform, streams: 1, reported: None });
                    true
                }
            }
        };
        if changed {
            self.events.send(ServerEvent::CastDevicesChanged { user_id });
        }
        Ok(Receiver { cast: self.clone(), user_id, id })
    }

    /// The devices of `user_id`, by name.
    pub fn list(&self, user_id: i64) -> Vec<CastDevice> {
        let devices = self.devices.lock();
        let mut out: Vec<CastDevice> = devices
            .iter()
            .filter(|((user, _), _)| *user == user_id)
            .map(|((_, id), device)| CastDevice {
                id: id.clone(),
                name: device.name.clone(),
                platform: device.platform,
                state: device.reported.as_ref().map(Reported::to_state),
            })
            .collect();
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
        out
    }

    /// Whether device `id` of `user_id` is listed.
    pub fn connected(&self, user_id: i64, id: &str) -> bool {
        self.devices.lock().contains_key(&(user_id, id.to_owned()))
    }

    /// Device `id` says what it plays (`None`: nothing): its profile's
    /// streams hear it. `false` when it isn't listed.
    pub fn report(&self, user_id: i64, id: &str, playing: Option<(ItemSummary, i64, Option<i64>, PlayState)>) -> bool {
        let state = {
            let mut devices = self.devices.lock();
            let Some(device) = devices.get_mut(&(user_id, id.to_owned())) else { return false };
            device.reported = playing.map(|(item, position_ms, duration_ms, state)| Reported {
                item,
                position_ms,
                duration_ms,
                state,
                at: Instant::now(),
            });
            device.reported.as_ref().map(|r| Box::new(r.to_state()))
        };
        self.events.send(ServerEvent::CastState { user_id, device_id: id.to_owned(), state });
        true
    }

    fn disconnect(&self, user_id: i64, id: &str) {
        let gone = {
            let mut devices = self.devices.lock();
            let key = (user_id, id.to_owned());
            match devices.get_mut(&key) {
                Some(device) if device.streams > 1 => {
                    device.streams -= 1;
                    false
                }
                Some(_) => devices.remove(&key).is_some(),
                None => false,
            }
        };
        if gone {
            self.events.send(ServerEvent::CastDevicesChanged { user_id });
        }
    }
}

impl Reported {
    fn to_state(&self) -> CastState {
        CastState {
            item: self.item.clone(),
            position_ms: self.position_ms,
            duration_ms: self.duration_ms,
            state: self.state,
            age_ms: u64::try_from(self.at.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }
}

/// A device listed for as long as it lives: its event stream holds it.
pub struct Receiver {
    cast: Arc<Cast>,
    user_id: i64,
    id: String,
}

impl Receiver {
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.cast.disconnect(self.user_id, &self.id);
    }
}

/// A device's id: the app makes it once and keeps it, 8 to 64 letters,
/// digits, `-` and `_`.
pub fn valid_id(id: &str) -> ApiResult<String> {
    let ok = (8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok { Ok(id.to_owned()) } else { Err(AppError::bad_request("a cast device's id is 8 to 64 letters and digits")) }
}

/// A device's name as people see it: trimmed, 1 to [`NAME_MAX`] characters,
/// none of them control characters.
fn valid_name(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX || name.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "a cast device's name is 1 to {NAME_MAX} characters, with no control characters"
        )));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(events: &mut tokio::sync::broadcast::Receiver<ServerEvent>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(event) = events.try_recv() {
            out.push(serde_json::to_value(&event).unwrap()["type"].as_str().unwrap().to_owned());
        }
        out
    }

    #[test]
    fn a_device_is_listed_while_its_stream_is_open() {
        let bus = EventBus::new();
        let mut events = bus.subscribe();
        let cast = Cast::new(bus);
        let tv = cast.connect(4, "device-tv-1", "  Living room  ", Platform::AndroidTv).unwrap();
        assert_eq!(tv.id(), "device-tv-1");
        let listed = cast.list(4);
        assert_eq!((listed.len(), listed[0].name.as_str()), (1, "Living room"));
        assert!(cast.list(5).is_empty(), "another profile's devices");
        assert_eq!(kinds(&mut events), ["cast_devices_changed"]);

        // Connecting again before the old stream is gone: one device, which
        // stays until both have ended.
        let again = cast.connect(4, "device-tv-1", "Living room", Platform::AndroidTv).unwrap();
        assert!(kinds(&mut events).is_empty(), "nothing changed");
        drop(tv);
        assert!(cast.connected(4, "device-tv-1"));
        drop(again);
        assert!(!cast.connected(4, "device-tv-1"));
        assert_eq!(kinds(&mut events), ["cast_devices_changed"]);
    }

    #[test]
    fn ids_names_and_numbers_are_bounded() {
        let cast = Cast::new(EventBus::new());
        assert!(cast.connect(4, "short", "TV", Platform::Roku).is_err());
        assert!(cast.connect(4, "has spaces in it", "TV", Platform::Roku).is_err());
        assert!(cast.connect(4, &"a".repeat(65), "TV", Platform::Roku).is_err());
        assert!(cast.connect(4, "device-tv-1", "   ", Platform::Roku).is_err());
        assert!(cast.connect(4, "device-tv-1", "TV\u{7}", Platform::Roku).is_err());
        assert!(cast.connect(4, "device-tv-1", &"é".repeat(NAME_MAX), Platform::Roku).is_ok());
        assert!(cast.connect(4, "device-tv-1", &"é".repeat(NAME_MAX + 1), Platform::Roku).is_err());
        let held: Vec<_> = (0..MAX_DEVICES_EACH)
            .map(|i| cast.connect(5, &format!("device-{i:04}"), "TV", Platform::Lg).unwrap())
            .collect();
        assert!(matches!(cast.connect(5, "device-more", "TV", Platform::Lg), Err(AppError::TooManyRequests(_))));
        assert!(cast.connect(5, "device-0000", "TV", Platform::Lg).is_ok(), "one listed already");
        assert!(cast.connect(6, "device-more", "TV", Platform::Lg).is_ok(), "per profile");
        drop(held);
        assert_eq!(Platform::from_name("android_tv"), Some(Platform::AndroidTv));
        assert_eq!(Platform::from_name("toaster"), None);
    }
}
