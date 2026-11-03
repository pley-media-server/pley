//! What Automatic quality knows of the networks a client plays on
//! (docs/QUALITY.md, "Automatic"): per signed-in user, client and network,
//! the ceiling Automatic's last change left (kept 24 hours) and the last
//! speed test (kept 7 days). A device back on a network it knows starts at
//! what that network carries; one on a network nobody measured from it
//! measures it first, with [`speed_test`]'s data.
//!
//! A network is `"local"` (this server's own: [`crate::setup::is_local_address`])
//! or a keyed hash of the address the request came from ([`SourceIp`]: it
//! follows trusted proxies only, so nobody on the internet can choose it),
//! an IPv4 address as it is and an IPv6 address by its /64, which a home's
//! devices share while their own addresses change. The address itself is
//! never stored, and the key stays with the server, so a copy of the
//! database can't tell where anyone played from.
//!
//! [`SourceIp`]: crate::auth::SourceIp

use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use dashmap::DashMap;
use ring::hmac;
use serde::Serialize;

use crate::db::Db;

/// How long the ceiling of Automatic's last change counts.
pub const CEILING_KEPT_MS: i64 = 24 * 3600 * 1000;
/// How long a speed test counts, and a row is kept at all.
pub const PROBE_KEPT_MS: i64 = 7 * 24 * 3600 * 1000;
/// The share of a speed test's speed Automatic starts at: a short test of
/// one connection reads above what a long playback keeps getting.
const PROBE_PERCENT: i64 = 70;
/// The fastest network anyone reports, in kbps (10 Gbps).
pub const MAX_KBPS: i64 = 10_000_000;
/// The longest client id, as `POST /api/playback` takes it.
const MAX_CLIENT_ID: usize = 128;
/// The network of every address on this server's own network.
pub const LOCAL: &str = "local";
/// Hex characters kept of a network's hash: 128 bits.
const HASH_HEX: usize = 32;
/// The most rows one user keeps (clients times networks in a week): client
/// ids are the clients' to choose, so one making up new ones can't grow the
/// table without end. The least recently written go first.
pub const MAX_ROWS_PER_USER: i64 = 200;

/// A speed test's size when none is asked for: 32 MiB.
pub const SPEED_TEST_BYTES: u64 = 32 * 1024 * 1024;
pub const SPEED_TEST_MIN: u64 = 64 * 1024;
pub const SPEED_TEST_MAX: u64 = 64 * 1024 * 1024;
/// Speed tests one user runs at once, at most.
pub const SPEED_TESTS_AT_ONCE: usize = 2;
/// The size of the pieces a speed test is sent in.
const SPEED_TEST_CHUNK: usize = 64 * 1024;
/// How long a speed test may take before the server ends it: players
/// stop reading after a few seconds, and one that keeps its connection open
/// without reading doesn't hold its turn forever.
const SPEED_TEST_LIMIT: Duration = Duration::from_secs(120);

pub struct Bandwidth {
    db: Db,
    /// Hashes addresses into networks.
    key: hmac::Key,
    /// Speed tests running, per user.
    tests: Arc<DashMap<i64, usize>>,
}

/// What is stored of one client on one network.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct Memory {
    pub probe_kbps: Option<i64>,
    /// The whole test arrived in the client's time: the network is at
    /// least that fast, and nothing limits it.
    pub probe_complete: bool,
    pub probe_at: Option<i64>,
    pub ceiling_kbps: Option<i64>,
    pub ceiling_at: Option<i64>,
}

/// What a client reports (`PUT /api/playback/network`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    /// A speed test's result.
    Probe { kbps: i64, complete: bool },
    /// Automatic's ceiling changed: a step down, or a step up to a quality.
    Ceiling(i64),
    /// Back to Original, or Automatic picked again: no ceiling, and no test
    /// due for 7 days.
    Forget,
}

/// The answer of `GET` and `PUT /api/playback/network`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Answer {
    /// This server's own network: never a speed test.
    pub local: bool,
    /// The most Automatic starts at here; `None`: no limit known.
    pub ceiling_kbps: Option<i64>,
    /// What set it: `"playback"` or `"probe"`.
    pub source: Option<&'static str>,
    /// When (unix ms).
    pub since: Option<i64>,
    /// Run a speed test before starting Automatic.
    pub probe: bool,
}

impl Bandwidth {
    /// Networks are hashed with a key derived from the server's
    /// `secret.key` (made with it if missing); one that can't be read gives
    /// a key for this run only, and what was known is forgotten.
    pub fn new(db: Db, data_dir: &Path) -> Self {
        let secret = match crate::two_step::load_key(&data_dir.join(crate::two_step::KEY_FILE)) {
            Ok(secret) => secret,
            Err(_) => {
                // TwoStep::new has said why in the log.
                let mut secret = [0u8; 32];
                getrandom::fill(&mut secret).expect("the system's random numbers");
                secret
            }
        };
        Self::with_secret(db, &secret)
    }

    fn with_secret(db: Db, secret: &[u8]) -> Self {
        Self { db, key: crate::two_step::derived_key(secret, b"pley playback networks"), tests: Arc::default() }
    }

    /// The network a request from `ip` came from ([`network_id`]).
    pub fn network(&self, ip: Option<IpAddr>) -> String {
        network_id(&self.key, ip)
    }

    /// What is known of `client_id`'s `network` for `user_id`.
    pub async fn get(&self, user_id: i64, client_id: &str, network: &str) -> sqlx::Result<Answer> {
        let memory = self.load(&self.db.read, user_id, client_id, network).await?;
        Ok(answer(network == LOCAL, &memory, crate::util::now_ms()))
    }

    /// Keeps what a client reports, and answers as [`Bandwidth::get`].
    /// Rows nobody wrote for 7 days go at the same time, and a user's beyond
    /// [`MAX_ROWS_PER_USER`].
    pub async fn report(&self, user_id: i64, client_id: &str, network: &str, report: Report) -> sqlx::Result<Answer> {
        let now = crate::util::now_ms();
        let mut tx = self.db.write.begin().await?;
        let mut memory = self.load(&mut *tx, user_id, client_id, network).await?;
        apply(&mut memory, report, now);
        sqlx::query(
            "INSERT OR REPLACE INTO playback_networks
                 (user_id, client_id, network, probe_kbps, probe_complete, probe_at, ceiling_kbps, ceiling_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(client_id)
        .bind(network)
        .bind(memory.probe_kbps)
        .bind(memory.probe_complete)
        .bind(memory.probe_at)
        .bind(memory.ceiling_kbps)
        .bind(memory.ceiling_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM playback_networks WHERE updated_at <= ?")
            .bind(now - PROBE_KEPT_MS)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM playback_networks WHERE user_id = ? AND (client_id, network) NOT IN
                 (SELECT client_id, network FROM playback_networks WHERE user_id = ?
                  ORDER BY updated_at DESC LIMIT ?)",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(MAX_ROWS_PER_USER)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(answer(network == LOCAL, &memory, now))
    }

    async fn load<'e, E: sqlx::SqliteExecutor<'e>>(
        &self,
        db: E,
        user_id: i64,
        client_id: &str,
        network: &str,
    ) -> sqlx::Result<Memory> {
        let row: Option<Memory> = sqlx::query_as(
            "SELECT probe_kbps, probe_complete, probe_at, ceiling_kbps, ceiling_at FROM playback_networks
             WHERE user_id = ? AND client_id = ? AND network = ?",
        )
        .bind(user_id)
        .bind(client_id)
        .bind(network)
        .fetch_optional(db)
        .await?;
        Ok(row.unwrap_or_default())
    }

    /// A turn for one of `user_id`'s speed tests, held until its response
    /// is sent or dropped; `None` while [`SPEED_TESTS_AT_ONCE`] run.
    pub fn speed_test_turn(&self, user_id: i64) -> Option<SpeedTestTurn> {
        take_turn(&self.tests, user_id)
    }
}

fn take_turn(tests: &Arc<DashMap<i64, usize>>, user_id: i64) -> Option<SpeedTestTurn> {
    let mut running = tests.entry(user_id).or_insert(0);
    if *running >= SPEED_TESTS_AT_ONCE {
        return None;
    }
    *running += 1;
    Some(SpeedTestTurn { tests: tests.clone(), user_id })
}

/// One running speed test ([`Bandwidth::speed_test_turn`]).
pub struct SpeedTestTurn {
    tests: Arc<DashMap<i64, usize>>,
    user_id: i64,
}

impl Drop for SpeedTestTurn {
    fn drop(&mut self) {
        if let Some(mut running) = self.tests.get_mut(&self.user_id) {
            *running = running.saturating_sub(1);
        }
        self.tests.remove_if(&self.user_id, |_, running| *running == 0);
    }
}

/// `"local"` for an address on this server's own network (or none: a
/// request without a connection), else the keyed hash of an IPv4 address
/// or of an IPv6 address's first 64 bits, in hex.
pub fn network_id(key: &hmac::Key, ip: Option<IpAddr>) -> String {
    let Some(ip) = ip.map(|ip| ip.to_canonical()) else { return LOCAL.to_owned() };
    if crate::setup::is_local_address(ip) {
        return LOCAL.to_owned();
    }
    let mut message = Vec::with_capacity(9);
    match ip {
        IpAddr::V4(v4) => {
            message.push(4);
            message.extend_from_slice(&v4.octets());
        }
        IpAddr::V6(v6) => {
            message.push(6);
            message.extend_from_slice(&v6.octets()[..8]);
        }
    }
    let mut id = hex::encode(hmac::sign(key, &message).as_ref());
    id.truncate(HASH_HEX);
    id
}

/// What `memory` makes of the network at `now` (unix ms): a ceiling of
/// Automatic's within 24 hours wins; else a speed test within 7 days gives
/// 70 % of its speed, or no limit for one that arrived whole (and for a
/// forget); else nothing is known, and a network other than the server's
/// own is worth a speed test.
pub fn answer(local: bool, memory: &Memory, now: i64) -> Answer {
    let recent = |at: Option<i64>, kept: i64| at.is_some_and(|at| now - at < kept);
    let played = memory.ceiling_kbps.filter(|_| recent(memory.ceiling_at, CEILING_KEPT_MS));
    let probed = recent(memory.probe_at, PROBE_KEPT_MS);
    let (ceiling_kbps, source, since) = if let Some(kbps) = played {
        (Some(kbps), Some("playback"), memory.ceiling_at)
    } else if probed && !memory.probe_complete {
        match memory.probe_kbps {
            Some(kbps) => (Some((kbps * PROBE_PERCENT / 100).max(1)), Some("probe"), memory.probe_at),
            None => (None, None, None),
        }
    } else {
        (None, None, None)
    };
    Answer { local, ceiling_kbps, source, since, probe: !local && played.is_none() && !probed }
}

/// `report` made at `now` (unix ms), in `memory`.
pub fn apply(memory: &mut Memory, report: Report, now: i64) {
    match report {
        Report::Probe { kbps, complete } => {
            memory.probe_kbps = Some(kbps);
            memory.probe_complete = complete;
            memory.probe_at = Some(now);
        }
        Report::Ceiling(kbps) => {
            memory.ceiling_kbps = Some(kbps);
            memory.ceiling_at = Some(now);
        }
        Report::Forget => {
            // As a speed test that found no limit, so none is due either.
            *memory = Memory { probe_complete: true, probe_at: Some(now), ..Memory::default() };
        }
    }
}

/// A client id as `POST /api/playback` takes it: 1 to 128 bytes.
pub fn valid_client_id(client_id: Option<&str>) -> Option<&str> {
    client_id.filter(|id| !id.is_empty() && id.len() <= MAX_CLIENT_ID)
}

/// A reported speed in kbps: a number from 1 to [`MAX_KBPS`], rounded
/// (players divide bytes by seconds, and may send the fraction).
pub fn valid_kbps(kbps: f64) -> Option<i64> {
    let kbps = kbps.round();
    (kbps.is_finite() && (1.0..=MAX_KBPS as f64).contains(&kbps)).then_some(kbps as i64)
}

/// A speed test's size for `asked` bytes: 32 MiB unless asked, within
/// 64 KiB and 64 MiB.
pub fn speed_test_size(asked: Option<u64>) -> u64 {
    asked.unwrap_or(SPEED_TEST_BYTES).clamp(SPEED_TEST_MIN, SPEED_TEST_MAX)
}

/// `size` bytes of data no compression shrinks, made as they are sent
/// (64 KiB at a time, a few microseconds each) rather than held in memory.
/// The turn is given back when the body ends or is dropped.
pub fn speed_test(size: u64, turn: SpeedTestTurn) -> Body {
    let mut seed = [0u8; 8];
    getrandom::fill(&mut seed).expect("the system's random numbers");
    let noise = Noise(u64::from_le_bytes(seed));
    let deadline = Instant::now() + SPEED_TEST_LIMIT;
    let stream = futures::stream::unfold((noise, size, turn), move |(mut noise, left, turn)| async move {
        if left == 0 {
            return None;
        }
        if Instant::now() > deadline {
            let err = std::io::Error::new(std::io::ErrorKind::TimedOut, "the speed test took too long");
            return Some((Err(err), (noise, 0, turn)));
        }
        let mut chunk = vec![0u8; (left as usize).min(SPEED_TEST_CHUNK)];
        noise.fill(&mut chunk);
        let sent = chunk.len() as u64;
        Some((Ok(Bytes::from(chunk)), (noise, left - sent, turn)))
    });
    Body::from_stream(stream)
}

/// wyrand: 8 random-looking bytes for a multiplication, which no
/// compressor finds a pattern in. Not for secrets.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0xa076_1d64_78bd_642f);
        let t = u128::from(self.0).wrapping_mul(u128::from(self.0 ^ 0xe703_7ed1_a0b4_28db));
        ((t >> 64) as u64) ^ (t as u64)
    }

    fn fill(&mut self, buf: &mut [u8]) {
        for piece in buf.chunks_mut(8) {
            let bytes = self.next().to_le_bytes();
            piece.copy_from_slice(&bytes[..piece.len()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    const HOUR: i64 = 3600 * 1000;
    const DAY: i64 = 24 * HOUR;
    const NOW: i64 = 1_761_600_000_000;

    fn key() -> hmac::Key {
        hmac::Key::new(hmac::HMAC_SHA256, b"a server's own key, for the tests")
    }

    fn id(ip: &str) -> String {
        network_id(&key(), Some(ip.parse().unwrap()))
    }

    #[test]
    fn networks_are_local_or_a_hash_of_the_address() {
        for local in ["127.0.0.1", "192.168.1.20", "10.0.0.9", "100.101.102.103", "::1", "fd00::5", "fe80::1"] {
            assert_eq!(id(local), LOCAL, "{local}");
        }
        assert_eq!(id("::ffff:192.168.1.20"), LOCAL, "an IPv4 address in IPv6");
        assert_eq!(network_id(&key(), None), LOCAL, "no connection: tests");

        let home = id("81.2.69.160");
        assert_eq!(home.len(), 32);
        assert!(home.chars().all(|c| c.is_ascii_hexdigit()), "{home}");
        assert_eq!(id("81.2.69.160"), home, "the same every time");
        assert_eq!(id("::ffff:81.2.69.160"), home, "as an IPv4 address");
        assert_ne!(id("81.2.69.161"), home, "every IPv4 address its own");

        // A home's IPv6 devices share their /64, whatever their own part.
        let six = id("2001:db8:1234:5678::1");
        assert_eq!(id("2001:db8:1234:5678:a1b2:c3d4:e5f6:789"), six);
        assert_ne!(id("2001:db8:1234:5679::1"), six, "the next /64");
        assert_ne!(six, home);

        // Another server's key: other hashes.
        let other = hmac::Key::new(hmac::HMAC_SHA256, b"another server");
        assert_ne!(network_id(&other, Some(IpAddr::V4(Ipv4Addr::new(81, 2, 69, 160)))), home);
        let v6: Ipv6Addr = "2001:db8:1234:5678::1".parse().unwrap();
        assert_ne!(network_id(&other, Some(IpAddr::V6(v6))), six);
    }

    fn after(reports: &[(Report, i64)]) -> Memory {
        let mut memory = Memory::default();
        for (report, at) in reports {
            apply(&mut memory, *report, *at);
        }
        memory
    }

    fn answered(local: bool, memory: &Memory, now: i64) -> (Option<i64>, Option<&'static str>, Option<i64>, bool) {
        let a = answer(local, memory, now);
        assert_eq!(a.local, local);
        (a.ceiling_kbps, a.source, a.since, a.probe)
    }

    #[test]
    fn nothing_known_asks_for_a_speed_test_away_from_home_only() {
        assert_eq!(answered(false, &Memory::default(), NOW), (None, None, None, true));
        assert_eq!(answered(true, &Memory::default(), NOW), (None, None, None, false));
    }

    #[test]
    fn a_speed_test_gives_seventy_percent_for_seven_days() {
        let probed = after(&[(Report::Probe { kbps: 52_000, complete: false }, NOW)]);
        assert_eq!(answered(false, &probed, NOW), (Some(36_400), Some("probe"), Some(NOW), false));
        assert_eq!(answered(false, &probed, NOW + 7 * DAY - 1), (Some(36_400), Some("probe"), Some(NOW), false));
        // Then it is forgotten, and another test is due.
        assert_eq!(answered(false, &probed, NOW + 7 * DAY), (None, None, None, true));
        // Never below 1.
        let slow = after(&[(Report::Probe { kbps: 1, complete: false }, NOW)]);
        assert_eq!(answer(false, &slow, NOW).ceiling_kbps, Some(1));
    }

    #[test]
    fn a_whole_speed_test_limits_nothing() {
        let fast = after(&[(Report::Probe { kbps: 140_000, complete: true }, NOW)]);
        assert_eq!(answered(false, &fast, NOW + DAY), (None, None, None, false));
        assert_eq!(answered(false, &fast, NOW + 7 * DAY), (None, None, None, true));
        // A later test that didn't arrive whole limits again.
        let later = after(&[
            (Report::Probe { kbps: 140_000, complete: true }, NOW),
            (Report::Probe { kbps: 10_000, complete: false }, NOW + HOUR),
        ]);
        assert_eq!(answered(false, &later, NOW + HOUR), (Some(7_000), Some("probe"), Some(NOW + HOUR), false));
    }

    #[test]
    fn a_ceiling_from_playback_wins_for_a_day() {
        let both =
            after(&[(Report::Probe { kbps: 52_000, complete: false }, NOW), (Report::Ceiling(8_000), NOW + HOUR)]);
        assert_eq!(answered(false, &both, NOW + HOUR), (Some(8_000), Some("playback"), Some(NOW + HOUR), false));
        assert_eq!(answered(false, &both, NOW + HOUR + DAY - 1).1, Some("playback"));
        // A day later the speed test counts again, while it lasts.
        assert_eq!(answered(false, &both, NOW + HOUR + DAY), (Some(36_400), Some("probe"), Some(NOW), false));
        // Alone, the ceiling ends in a day, and a test is due.
        let ceiling = after(&[(Report::Ceiling(8_000), NOW)]);
        assert_eq!(answered(false, &ceiling, NOW + DAY - 1), (Some(8_000), Some("playback"), Some(NOW), false));
        assert_eq!(answered(false, &ceiling, NOW + DAY), (None, None, None, true));
        // A step up to a quality moves it; at home it is kept too.
        let up = after(&[(Report::Ceiling(8_000), NOW), (Report::Ceiling(20_000), NOW + HOUR)]);
        assert_eq!(answered(true, &up, NOW + HOUR), (Some(20_000), Some("playback"), Some(NOW + HOUR), false));
        // Even over a speed test that limited nothing.
        let over = after(&[(Report::Probe { kbps: 140_000, complete: true }, NOW), (Report::Ceiling(8_000), NOW)]);
        assert_eq!(answered(false, &over, NOW).1, Some("playback"));
    }

    #[test]
    fn forgetting_limits_nothing_and_asks_for_no_test_for_seven_days() {
        let forgot = after(&[
            (Report::Probe { kbps: 52_000, complete: false }, NOW),
            (Report::Ceiling(8_000), NOW),
            (Report::Forget, NOW + HOUR),
        ]);
        assert_eq!(forgot, Memory { probe_complete: true, probe_at: Some(NOW + HOUR), ..Memory::default() });
        assert_eq!(answered(false, &forgot, NOW + HOUR), (None, None, None, false));
        assert_eq!(answered(false, &forgot, NOW + HOUR + 7 * DAY - 1), (None, None, None, false));
        assert_eq!(answered(false, &forgot, NOW + HOUR + 7 * DAY), (None, None, None, true));
        // A step down afterwards counts as ever.
        let mut again = forgot;
        apply(&mut again, Report::Ceiling(4_000), NOW + 2 * HOUR);
        assert_eq!(answered(false, &again, NOW + 2 * HOUR).0, Some(4_000));
    }

    #[test]
    fn requests_are_checked() {
        assert_eq!(valid_client_id(Some("tv-1")), Some("tv-1"));
        assert_eq!(valid_client_id(Some(&"x".repeat(128))).map(str::len), Some(128));
        assert_eq!(valid_client_id(Some(&"x".repeat(129))), None);
        assert_eq!(valid_client_id(Some("")), None);
        assert_eq!(valid_client_id(None), None);

        assert_eq!(valid_kbps(52_000.0), Some(52_000));
        assert_eq!(valid_kbps(1.0), Some(1));
        assert_eq!(valid_kbps(10_000_000.0), Some(10_000_000));
        assert_eq!(valid_kbps(4_321.6), Some(4_322), "rounded");
        for wrong in [0.0, -5.0, 0.4, 10_000_001.0, f64::NAN, f64::INFINITY] {
            assert_eq!(valid_kbps(wrong), None, "{wrong}");
        }

        assert_eq!(speed_test_size(None), 33_554_432);
        assert_eq!(speed_test_size(Some(8_388_608)), 8_388_608);
        assert_eq!(speed_test_size(Some(10)), 65_536);
        assert_eq!(speed_test_size(Some(1 << 40)), 67_108_864);
    }

    #[test]
    fn speed_tests_take_turns() {
        let tests = Arc::new(DashMap::new());
        let turn = |user| take_turn(&tests, user);
        let first = turn(1).unwrap();
        let second = turn(1).unwrap();
        assert!(turn(1).is_none(), "two at a time");
        assert!(turn(2).is_some(), "per user");
        drop(first);
        let third = turn(1).unwrap();
        drop((second, third));
        assert!(!tests.contains_key(&1), "nothing left behind");
    }

    #[test]
    fn noise_does_not_compress() {
        let mut noise = Noise(42);
        let mut data = vec![0u8; 1 << 20];
        noise.fill(&mut data[..1000 * 8 + 3]);
        noise.fill(&mut data[1000 * 8 + 3..]);
        // Every byte value about as often as the others.
        let mut counts = [0usize; 256];
        for b in &data {
            counts[*b as usize] += 1;
        }
        let expected = data.len() / 256;
        assert!(counts.iter().all(|c| c.abs_diff(expected) < expected / 10), "{counts:?}");
        // And no stretch repeats another.
        let mut seen = std::collections::HashSet::new();
        assert!(data.chunks(16).all(|piece| seen.insert(piece)));
    }
}
