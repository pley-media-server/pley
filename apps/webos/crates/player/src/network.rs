//! What the server knows of the network the TV plays on, for Automatic's
//! start (docs/QUALITY.md, "Automatic"), and the speed test it asks for on a
//! remote network it doesn't know yet (packages/tv/src/player/network.ts).
//! The test itself is pley-core's `Api::speed_test`: 2 s read from the first
//! byte of a 32 MiB answer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use pley_core::ApiError;
use pley_core::model::{NetworkAnswer, NetworkReport, SpeedSample};

use crate::autoquality::{Remembered, wall_ms};
use crate::server::PlayerServer;

/// How long the server is given to say what it knows; past that, the TV's own memory.
pub const NETWORK_ASK_MS: u64 = 1_500;
/// What the speed test asks for: 32 MiB.
pub const SPEED_TEST_BYTES: u64 = 32 * 1024 * 1024;
/// A test that couldn't read all of it keeps 30 % in hand, as the server does.
pub const PROBE_SHARE: f64 = 0.7;
/// The longest a speed test's report is waited for before starting.
pub const REPORT_WAIT_MS: u64 = 500;
/// A speed test isn't run again this soon, whatever came of the last.
pub const PROBE_AGAIN_MS: u64 = 10 * 60_000;
/// Ceiling changes are told in the background, with the usual time.
const TELL_MS: u64 = 20_000;

/// A speed test's result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpeedResult {
    pub kbps: f64,
    pub complete: bool,
}

/// What [`start_ceiling`] asks: the server's memory, the report of a test, the test, and the wall clock.
pub trait NetworkIo {
    fn ask(&self) -> Result<NetworkAnswer, ApiError>;
    fn tell(&self, report: &NetworkReport) -> Result<NetworkAnswer, ApiError>;
    fn speed_test(&self) -> Option<SpeedResult>;
    /// Milliseconds since 1970: what remembered ceilings are kept in.
    fn wall(&self) -> i64;
}

/// Where Automatic may start, from the server: the ceiling it knows for
/// this network (`Some(None)`: none), after a speed test when it asks for
/// one; none when it can't say (no answer, an older server, a test that
/// measured nothing), and the TV's own memory decides as before.
pub fn start_ceiling(io: &dyn NetworkIo) -> Option<Option<Remembered>> {
    let mut answer = io.ask().ok()?;
    if answer.probe && !answer.local {
        let speed = io.speed_test()?;
        match io.tell(&NetworkReport::Probe { kbps: speed.kbps, complete: speed.complete }) {
            Ok(told) => answer = told,
            // Not heard in time: what the server would make of it.
            Err(_) => {
                return Some(
                    (!speed.complete).then(|| Remembered { kbps: (speed.kbps * PROBE_SHARE).round(), at: io.wall() }),
                );
            }
        }
    }
    Some(
        answer
            .ceiling_kbps
            .filter(|k| *k > 0)
            .map(|k| Remembered { kbps: k as f64, at: answer.since.unwrap_or_else(|| io.wall()) }),
    )
}

/// What the player asks of the server's memory of networks. Both block: run them on a task.
pub trait NetworkMemory: Send + Sync {
    /// Where Automatic may start ([`start_ceiling`]).
    fn start(&self) -> Option<Option<Remembered>>;
    /// Automatic's remembered ceiling changed (none: forgotten).
    fn report(&self, kbps: Option<f64>);
}

/// What was heard of a server in this run of the app.
#[derive(Clone, Copy, Debug, Default)]
struct Heard {
    /// It answered 404: an older server, never asked again.
    older: bool,
    last_probe: Option<Instant>,
}

fn heard() -> &'static Mutex<HashMap<String, Heard>> {
    static HEARD: OnceLock<Mutex<HashMap<String, Heard>>> = OnceLock::new();
    HEARD.get_or_init(Default::default)
}

fn with_heard<R>(server: &str, f: impl FnOnce(&mut Heard) -> R) -> R {
    let mut map = heard().lock().unwrap_or_else(|e| e.into_inner());
    f(map.entry(server.to_string()).or_default())
}

/// Forgets what was heard of servers (tests).
pub fn forget_servers() {
    heard().lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// The server's memory of networks, for this TV (`client_id`, the one it plays as).
pub struct ServerNetwork {
    server: Arc<dyn PlayerServer>,
    client_id: String,
}

impl ServerNetwork {
    /// The memory of `server`'s networks for this TV.
    pub fn new(server: Arc<dyn PlayerServer>, client_id: impl Into<String>) -> ServerNetwork {
        ServerNetwork { server, client_id: client_id.into() }
    }

    fn older(&self) -> bool {
        with_heard(self.server.server(), |h| h.older)
    }

    fn mark_older(&self, e: &ApiError) {
        if e.is(404) {
            with_heard(self.server.server(), |h| h.older = true);
        }
    }
}

impl NetworkIo for ServerNetwork {
    fn ask(&self) -> Result<NetworkAnswer, ApiError> {
        let answer = self.server.network(&self.client_id, Duration::from_millis(NETWORK_ASK_MS));
        if let Err(e) = &answer {
            self.mark_older(e);
        }
        answer
    }

    fn tell(&self, report: &NetworkReport) -> Result<NetworkAnswer, ApiError> {
        self.server.tell_network(&self.client_id, report, Duration::from_millis(REPORT_WAIT_MS))
    }

    fn speed_test(&self) -> Option<SpeedResult> {
        let now = Instant::now();
        let again = with_heard(self.server.server(), |h| {
            let soon = h.last_probe.is_some_and(|t| now.duration_since(t) < Duration::from_millis(PROBE_AGAIN_MS));
            if !soon {
                h.last_probe = Some(now);
            }
            !soon
        });
        if !again {
            return None;
        }
        let sample: SpeedSample = self.server.speed_test(SPEED_TEST_BYTES).ok()?;
        sample.kbps().map(|kbps| SpeedResult { kbps, complete: sample.complete })
    }

    fn wall(&self) -> i64 {
        wall_ms()
    }
}

impl NetworkMemory for ServerNetwork {
    fn start(&self) -> Option<Option<Remembered>> {
        if self.older() {
            return None;
        }
        start_ceiling(self)
    }

    fn report(&self, kbps: Option<f64>) {
        if self.older() {
            return;
        }
        let report = NetworkReport::Ceiling(kbps.map(f64::round));
        if let Err(e) = self.server.tell_network(&self.client_id, &report, Duration::from_millis(TELL_MS)) {
            self.mark_older(&e);
        }
    }
}
