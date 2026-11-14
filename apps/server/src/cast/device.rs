//! What the shared layer for devices on the network ([`crate::nearby`])
//! asks of Google Cast: which stream a device gets, and how its player is
//! reached.

use std::collections::HashMap;
use std::net::SocketAddr;

use parking_lot::Mutex;

use super::connection::Connection;
use super::profile::{self, Hardware, Profile};
use crate::nearby::casts::Choice;
use crate::nearby::player::{Connected, Failure};
use crate::nearby::{Found, Protocol};
use crate::state::AppState;

/// Devices whose setup API's answer is kept, at most.
const MAX_KNOWN: usize = 256;

/// What Pley learned of Cast devices.
#[derive(Default)]
pub struct Known {
    /// What each device's setup API said, by its id, asked once (`None`:
    /// it didn't answer).
    hardware: Mutex<HashMap<String, Option<Hardware>>>,
}

/// The stream `device` gets: the [profile] of its model, then the basic
/// one.
pub async fn choose(state: &AppState, device: &Found) -> Choice {
    let hardware = hardware(state, device).await;
    let picked = profile::pick(device.model.as_deref(), hardware.as_ref());
    Choice {
        caps: picked.caps(),
        basic: (picked != Profile::BASIC).then(|| Profile::BASIC.caps()),
        model: profile::identity(device.model.as_deref(), hardware.as_ref()),
    }
}

/// Connects to `device`, to start Google's player there.
pub async fn connect(device: &Found) -> Result<Connected, Failure> {
    match Connection::connect(device.address).await {
        Ok((connection, incoming)) => Ok(Connected::Cast(connection, incoming)),
        Err(err) => {
            tracing::warn!("couldn't connect to the Cast device {}: {err}", device.address);
            Err(Failure::Lost)
        }
    }
}

/// What `device`'s setup API said, asked the first time it is cast to.
async fn hardware(state: &AppState, device: &Found) -> Option<Hardware> {
    let known = &state.nearby.cast.hardware;
    if let Some(answer) = known.lock().get(&device.id) {
        return answer.clone();
    }
    let fixed = &state.config.cast_fixed;
    let setup = match fixed.device {
        Some(address) if address == device.address => fixed.setup,
        _ => Some(SocketAddr::new(device.address.ip(), profile::SETUP_PORT)),
    };
    let answer = match setup {
        Some(setup) => profile::ask(setup).await,
        None => None,
    };
    let mut known = known.lock();
    if known.len() < MAX_KNOWN {
        known.insert(device.id.clone(), answer.clone());
    }
    answer
}

/// A Cast device no look finds, to develop and test with: Android's Google
/// TV emulator, its port 8009 forwarded (`adb forward tcp:8009 tcp:8009`),
/// or a test's fake device. Debug builds read it from `PLEY_CAST_DEVICE`
/// (its address) and `PLEY_CAST_SERVER` (Pley's address as the device
/// reaches it: `http://10.0.2.2:7539` from the emulator); release builds
/// have none.
#[derive(Debug, Clone, Default)]
pub struct Fixed {
    pub device: Option<SocketAddr>,
    pub server: Option<String>,
    /// Where its setup API answers (`PLEY_CAST_SETUP`, else the device's
    /// own on [`profile::SETUP_PORT`]); none, not asked.
    pub setup: Option<SocketAddr>,
}

impl Fixed {
    pub fn from_env() -> Fixed {
        if !cfg!(debug_assertions) {
            return Fixed::default();
        }
        let device: Option<SocketAddr> = std::env::var("PLEY_CAST_DEVICE").ok().and_then(|a| a.parse().ok());
        let setup = std::env::var("PLEY_CAST_SETUP").ok().and_then(|a| a.parse().ok());
        Fixed {
            device,
            server: std::env::var("PLEY_CAST_SERVER").ok().filter(|s| s.starts_with("http://")),
            setup: setup.or_else(|| device.map(|d| SocketAddr::new(d.ip(), profile::SETUP_PORT))),
        }
    }

    pub(crate) fn found(&self) -> Option<Found> {
        let address = self.device?;
        Some(Found {
            id: "dev".into(),
            name: "Cast emulator".into(),
            model: None,
            address,
            video: true,
            protocol: Protocol::Cast,
        })
    }
}
