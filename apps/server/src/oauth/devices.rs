//! TVs and other devices without a usable browser sign in with a code they
//! show (OAuth 2.0 Device Authorization Grant, RFC 8628): the person opens
//! `/link` on a phone or computer where they are signed in to pley's web
//! app, types the code and allows the app, while the device asks the token
//! endpoint for the answer. In memory only, like the rest of a sign-in in
//! progress.
//!
//! * Only pley's own apps start one: `POST /api/oauth/device` is anonymous,
//!   and pley reads no app's description for someone who isn't signed in.
//! * A request lasts 10 minutes. Its device code carries 256 random bits,
//!   and only its SHA-256 is kept; its user code is 8 letters
//!   ([`new_user_code`]), which the person compares with their screen.
//! * These are the only things the authorization server keeps for someone
//!   who isn't signed in: at most 20 at a time from one address and 1,000
//!   in all. The address is the connection's, or that of the client a proxy
//!   on this computer or its network forwards for
//!   ([`crate::auth::forwarded_for`]), never one a caller on the internet
//!   writes; an IPv6 /64 network counts as one address, since one host may
//!   be given all of it. Nothing in progress is dropped to make room, so a
//!   flood can hold up sign-ins with a code but never cancel one.
//! * Allowed, a request waits for the device's next question. When the
//!   account's sign-ins end before the device has its tokens (a new
//!   password, signing out everywhere, disabling, deletion), the answer
//!   becomes a refusal ([`Devices::forget_user`]), and the tokens are made
//!   only while the browser session that allowed it is signed in.
//! * Only signed-in people can try codes, and wrong ones count: past 10 by
//!   one person or 300 by everyone in 10 minutes, no code is looked up.
//! * A device asking sooner than its interval is told to slow down, and
//!   waits 5 seconds longer from then on (RFC 8628 §3.5). Once the person
//!   answered, the device gets the answer once and the request is over.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv6Addr};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::AppSnapshot;
use crate::auth::scopes::Scopes;
use crate::auth::{hash_token, new_token};

/// The grant type of a device asking for its answer (RFC 8628 §3.4).
pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// How long a request lasts.
pub const LIFETIME: Duration = Duration::from_secs(600);
/// How long a device waits between two questions, to begin with.
pub const INTERVAL: Duration = Duration::from_secs(5);
/// What asking too soon adds to the wait (RFC 8628 §3.5).
const SLOWER: Duration = Duration::from_secs(5);
/// Requests in progress from one address.
pub const MAX_PER_ADDRESS: usize = 20;
/// Requests in progress from one IPv6 site (a /56, what a home is usually
/// given): its 256 networks of /64 couldn't fill [`MAX_REQUESTS`] then.
pub const MAX_PER_SITE: usize = 50;
/// Requests in progress in all.
pub const MAX_REQUESTS: usize = 1_000;
/// How long a wrong code counts.
pub(super) const WRONG_CODE_WINDOW: Duration = Duration::from_secs(600);
/// Wrong codes one person may type within the window.
pub const MAX_WRONG_PER_PERSON: usize = 10;
/// Wrong codes everyone may type within the window.
pub const MAX_WRONG: usize = 300;
/// The letters of user codes: RFC 8628 §6.1's 20 consonants. Without
/// vowels they spell no words, and without digits nothing looks alike.
const ALPHABET: &[u8; 20] = b"BCDFGHJKLMNPQRSTVWXZ";
/// Letters in a user code: 20^8, about 2.56 × 10^10 codes.
const CODE_LETTERS: usize = 8;
/// Random bytes below this stand for a letter, 12 for each; the others are
/// drawn again, so that every letter is as likely (no modulo bias).
const FAIR_BYTES: usize = 256 - 256 % ALPHABET.len();

/// A new user code: 8 letters of RFC 8628 §6.1's consonants, from the
/// operating system's random generator.
pub fn new_user_code() -> String {
    let mut code = String::with_capacity(CODE_LETTERS);
    let mut bytes = [0u8; 16];
    while code.len() < CODE_LETTERS {
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        code.extend(bytes.iter().filter_map(|&byte| letter(byte)).take(CODE_LETTERS - code.len()));
    }
    code
}

/// The letter a random byte stands for, when it stands for one.
fn letter(byte: u8) -> Option<char> {
    let byte = usize::from(byte);
    (byte < FAIR_BYTES).then(|| char::from(ALPHABET[byte % ALPHABET.len()]))
}

/// A code as the person typed it, in capitals and without spaces or dashes;
/// `None` when that isn't 8 letters of the alphabet.
pub fn normalize(typed: &str) -> Option<String> {
    let code: String =
        typed.chars().filter(|c| !c.is_whitespace() && *c != '-').map(|c| c.to_ascii_uppercase()).collect();
    (code.len() == CODE_LETTERS && code.bytes().all(|b| ALPHABET.contains(&b))).then_some(code)
}

/// How a code is written for people: BCDF-GHJK.
pub fn shown(code: &str) -> String {
    match code.split_at_checked(CODE_LETTERS / 2) {
        Some((first, second)) => format!("{first}-{second}"),
        None => code.to_owned(),
    }
}

/// What requests from `address` count against: the address itself, or for
/// IPv6 its /64 network, all of which one host may use.
pub(super) fn network(address: IpAddr) -> IpAddr {
    match address.to_canonical() {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & (u128::MAX << 64))),
        v4 => v4,
    }
}

/// The IPv6 site (/56) of a network, which counts against [`MAX_PER_SITE`]
/// too; an IPv4 address has none.
pub(super) fn site(network: Option<IpAddr>) -> Option<Ipv6Addr> {
    match network? {
        IpAddr::V6(v6) => Some(Ipv6Addr::from(u128::from(v6) & (u128::MAX << 72))),
        IpAddr::V4(_) => None,
    }
}

/// Where a request is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    /// Nobody typed its code yet.
    Pending,
    /// Shown on the consent page of a pending request (see
    /// [`super::pending`]), by its id: that page answers.
    Deciding(String),
    /// Allowed: the app acts as `user_id`, allowed by `approved_by`, the
    /// same person (a TV gets the profile of whoever allowed it), from the
    /// browser session whose token has the SHA-256 `session_hash`.
    Allowed {
        user_id: i64,
        approved_by: i64,
        session_hash: String,
        scopes: Scopes,
    },
    /// Allowed, and the device's tokens are being made: no other question
    /// gets anything. `signed_out` once the account's sign-ins ended
    /// meanwhile.
    Issuing {
        user_id: i64,
        approved_by: i64,
        signed_out: bool,
    },
    Denied,
}

struct Entry {
    /// The app, as it was when the request was made.
    app: AppSnapshot,
    scopes: Scopes,
    /// 8 letters, without the dash.
    user_code: String,
    /// Who asked: shown on the consent page, so the person can tell which
    /// device is signing in.
    asker: Asker,
    /// The [`network`] it came from, when known.
    network: Option<IpAddr>,
    created: Instant,
    /// How long the device must wait between two questions.
    interval: Duration,
    last_poll: Option<Instant>,
    status: Status,
}

impl Entry {
    fn alive(&self, now: Instant) -> bool {
        now.duration_since(self.created) < LIFETIME
    }
}

#[derive(Default)]
struct State {
    /// By the SHA-256 of their device code.
    requests: HashMap<String, Entry>,
    /// Their keys, by user code.
    by_code: HashMap<String, String>,
    /// When each person typed wrong codes, oldest first.
    wrong: HashMap<i64, VecDeque<Instant>>,
    /// When anyone did.
    wrong_in_all: VecDeque<Instant>,
}

impl State {
    /// Forgets request `key`, and its user code.
    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.requests.remove(key) {
            self.by_code.remove(&entry.user_code);
        }
    }

    /// Forgets the requests that are over.
    fn drop_over(&mut self, now: Instant) {
        self.requests.retain(|_, entry| entry.alive(now));
        let requests = &self.requests;
        self.by_code.retain(|_, key| requests.contains_key(key));
    }
}

/// Drops the times a wrong code no longer counts for.
pub(super) fn forget_old(times: &mut VecDeque<Instant>, now: Instant) {
    while times.front().is_some_and(|&at| now.duration_since(at) >= WRONG_CODE_WINDOW) {
        times.pop_front();
    }
}

/// The device asking for a code.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asker {
    /// The address it asked from, as [`crate::auth::SourceIp`] reads it.
    pub address: Option<IpAddr>,
    /// What it says it is, from its User-Agent ("Kodi 21.3; Windows"):
    /// unchecked, since anyone may start a request.
    pub device: Option<String>,
}

impl From<Option<IpAddr>> for Asker {
    fn from(address: Option<IpAddr>) -> Self {
        Asker { address, device: None }
    }
}

/// A request just made: what the device shows and keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    /// What the device asks for its answer with.
    pub device_code: String,
    /// What the person types: 8 letters, without the dash ([`shown`] writes
    /// them for people).
    pub user_code: String,
}

/// A request whose code the person typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The request, for [`Devices::link`] and [`Devices::decide`].
    pub key: String,
    pub app: AppSnapshot,
    pub scopes: Scopes,
    /// The device that asked.
    pub asker: Asker,
    /// How long it has left.
    pub left: Duration,
    /// The pending request whose consent page shows it already, if any.
    pub deciding: Option<String>,
}

/// Why a code typed on pley's page leads to no request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Too many wrong codes lately, by this person or by everyone.
    Throttled,
    /// No request in progress has it (expired ones included): a wrong code.
    Unknown,
    /// Its request was answered already.
    Used,
}

/// What a device asking for its answer gets (RFC 8628 §3.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    /// An unknown device code, or another app's: nothing changed.
    Unknown,
    /// Nobody answered yet.
    Pending,
    /// Asked sooner than its interval, which is now 5 seconds longer.
    SlowDown,
    /// Refused. The request is over.
    Denied,
    /// Ten minutes went by. The request is over.
    Expired,
    /// Allowed: this answer comes once. Once its tokens are made (or not),
    /// [`Devices::issued`] ends the request.
    Allowed(DeviceGrant),
}

/// What a person allowed a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceGrant {
    pub app: AppSnapshot,
    /// The profile the app acts as.
    pub user_id: i64,
    /// Who allowed it.
    pub approved_by: i64,
    /// SHA-256 of the token of the browser session that allowed it.
    pub session_hash: String,
    pub scopes: Scopes,
}

/// Too many requests are in progress, from one address or in all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

/// The request is over, or another page shows it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Over;

/// The requests in progress, and the wrong codes of the last 10 minutes.
#[derive(Default)]
pub struct Devices {
    state: Mutex<State>,
    /// How far tests moved this store's clock on ([`Devices::wait`]).
    later: Mutex<Duration>,
}

impl Devices {
    /// Starts a request of `app` for `scopes`, made by `asker`: what the
    /// device shows and keeps. Refused while its address (its /64 for IPv6)
    /// has 20 in progress, its IPv6 /56 50, or everyone 1,000: only requests
    /// that are over make room. Requests from unknown addresses count
    /// together.
    pub fn create(&self, app: AppSnapshot, scopes: Scopes, asker: impl Into<Asker>) -> Result<Started, Full> {
        self.create_drawing(app, scopes, asker.into(), new_user_code)
    }

    /// [`Devices::create`], with user codes from `draw`.
    fn create_drawing(
        &self,
        app: AppSnapshot,
        scopes: Scopes,
        asker: Asker,
        mut draw: impl FnMut() -> String,
    ) -> Result<Started, Full> {
        let now = self.now();
        let network = asker.address.map(network);
        let mut state = self.state.lock();
        if state.requests.len() >= MAX_REQUESTS {
            state.drop_over(now);
        }
        let from_there = state.requests.values().filter(|entry| entry.alive(now) && entry.network == network).count();
        let site = site(network);
        let from_site = site.map_or(0, |site| {
            state.requests.values().filter(|entry| entry.alive(now) && self::site(entry.network) == Some(site)).count()
        });
        if state.requests.len() >= MAX_REQUESTS || from_there >= MAX_PER_ADDRESS || from_site >= MAX_PER_SITE {
            return Err(Full);
        }
        // Unique among the codes kept: a clash, which practically never
        // happens, is drawn again.
        let user_code = loop {
            let code = draw();
            if !state.by_code.contains_key(&code) {
                break code;
            }
        };
        let device_code = new_token();
        let key = hash_token(&device_code);
        state.by_code.insert(user_code.clone(), key.clone());
        let entry = Entry {
            app,
            scopes,
            user_code: user_code.clone(),
            asker,
            network,
            created: now,
            interval: INTERVAL,
            last_poll: None,
            status: Status::Pending,
        };
        state.requests.insert(key, entry);
        Ok(Started { device_code, user_code })
    }

    /// The request whose user code `user_code` (as [`normalize`] gives it)
    /// `user_id` typed on pley's page. A code that leads to no request in
    /// progress is a wrong code, and counts: past 10 by the person or 300 by
    /// everyone in 10 minutes, no code is looked up.
    pub fn find(&self, user_code: &str, user_id: i64) -> Result<Found, Refused> {
        let now = self.now();
        let mut guard = self.state.lock();
        let state = &mut *guard;
        let mine = state.wrong.get_mut(&user_id).map_or(0, |times| {
            forget_old(times, now);
            times.len()
        });
        forget_old(&mut state.wrong_in_all, now);
        if mine >= MAX_WRONG_PER_PERSON || state.wrong_in_all.len() >= MAX_WRONG {
            return Err(Refused::Throttled);
        }
        let found = state.by_code.get(user_code).and_then(|key| Some((key, state.requests.get(key)?)));
        match found {
            Some((key, entry)) if entry.alive(now) => {
                let deciding = match &entry.status {
                    Status::Pending => None,
                    Status::Deciding(id) => Some(id.clone()),
                    Status::Allowed { .. } | Status::Issuing { .. } | Status::Denied => return Err(Refused::Used),
                };
                Ok(Found {
                    key: key.clone(),
                    app: entry.app.clone(),
                    scopes: entry.scopes,
                    asker: entry.asker.clone(),
                    left: LIFETIME.saturating_sub(now.duration_since(entry.created)),
                    deciding,
                })
            }
            _ => {
                state.wrong.entry(user_id).or_default().push_back(now);
                state.wrong_in_all.push_back(now);
                Err(Refused::Unknown)
            }
        }
    }

    /// Shows request `key` on the consent page of pending request
    /// `pending_id`, when it is still as it was found: waiting for its code
    /// (`seen` is `None`), or on the page of pending request `seen`. False
    /// when someone was quicker, or when it is over.
    pub fn link(&self, key: &str, seen: Option<&str>, pending_id: &str) -> bool {
        let now = self.now();
        let mut state = self.state.lock();
        let Some(entry) = state.requests.get_mut(key).filter(|entry| entry.alive(now)) else { return false };
        let as_found = match (&entry.status, seen) {
            (Status::Pending, None) => true,
            (Status::Deciding(id), Some(seen)) => id == seen,
            _ => false,
        };
        if as_found {
            entry.status = Status::Deciding(pending_id.to_owned());
        }
        as_found
    }

    /// The answer of the consent page of pending request `pending_id` to
    /// request `key`: allowed by `user_id` from the browser session whose
    /// token has the SHA-256 `session_hash`, for the profile they use, with
    /// `scopes`, or refused. The device gets it when it next asks.
    pub fn decide(
        &self,
        key: &str,
        pending_id: &str,
        allow: bool,
        user_id: i64,
        session_hash: &str,
        scopes: Scopes,
    ) -> Result<(), Over> {
        let now = self.now();
        let mut state = self.state.lock();
        let entry = state.requests.get_mut(key).filter(|entry| entry.alive(now)).ok_or(Over)?;
        if !matches!(&entry.status, Status::Deciding(id) if id == pending_id) {
            return Err(Over);
        }
        entry.status = if allow {
            Status::Allowed { user_id, approved_by: user_id, session_hash: session_hash.to_owned(), scopes }
        } else {
            Status::Denied
        };
        Ok(())
    }

    /// The sign-ins of `user_id` ended (a new password, signing out
    /// everywhere, disabling, deletion): the devices they allowed that
    /// haven't asked for their answer yet are refused instead, as the apps
    /// already signed in are signed out, and one whose tokens are being made
    /// right now gets nothing ([`Devices::issued`]). Such a device isn't
    /// among the account's apps yet, so nothing else could stop it.
    pub fn forget_user(&self, user_id: i64) {
        let mut state = self.state.lock();
        for entry in state.requests.values_mut() {
            match &mut entry.status {
                Status::Allowed { user_id: acting, approved_by, .. }
                    if *acting == user_id || *approved_by == user_id =>
                {
                    entry.status = Status::Denied;
                }
                Status::Issuing { user_id: acting, approved_by, signed_out }
                    if *acting == user_id || *approved_by == user_id =>
                {
                    *signed_out = true;
                }
                _ => {}
            }
        }
    }

    /// The device of `device_code` got its tokens, or didn't: its request is
    /// over. True when the account's sign-ins ended while they were being
    /// made: end what it got.
    pub fn issued(&self, device_code: &str) -> bool {
        let key = hash_token(device_code);
        let mut state = self.state.lock();
        let signed_out = matches!(
            state.requests.get(&key).map(|entry| &entry.status),
            Some(Status::Issuing { signed_out: true, .. })
        );
        state.remove(&key);
        signed_out
    }

    /// The answer for a device asking with `device_code` as `client_id`
    /// (RFC 8628 §3.4-3.5). Decided under the lock, so of two questions at
    /// once only one gets an allowed request; the others are told it is
    /// unknown, as the request is over for them.
    pub fn poll(&self, device_code: &str, client_id: &str) -> Poll {
        let now = self.now();
        let key = hash_token(device_code);
        let mut state = self.state.lock();
        let Some(entry) = state
            .requests
            .get_mut(&key)
            .filter(|entry| entry.app.id == client_id && !matches!(entry.status, Status::Issuing { .. }))
        else {
            return Poll::Unknown;
        };
        let answer = if entry.alive(now) {
            let too_soon = entry.last_poll.is_some_and(|last| now.duration_since(last) < entry.interval);
            entry.last_poll = Some(now);
            match &entry.status {
                // slow_down is a kind of "not answered yet" (RFC 8628 §3.5):
                // an answer comes at once, however soon.
                Status::Pending | Status::Deciding(_) if too_soon => {
                    entry.interval += SLOWER;
                    Poll::SlowDown
                }
                Status::Pending | Status::Deciding(_) => Poll::Pending,
                Status::Denied => Poll::Denied,
                Status::Allowed { user_id, approved_by, session_hash, scopes } => {
                    let grant = DeviceGrant {
                        app: entry.app.clone(),
                        user_id: *user_id,
                        approved_by: *approved_by,
                        session_hash: session_hash.clone(),
                        scopes: *scopes,
                    };
                    entry.status =
                        Status::Issuing { user_id: grant.user_id, approved_by: grant.approved_by, signed_out: false };
                    Poll::Allowed(grant)
                }
                // Left out above.
                Status::Issuing { .. } => Poll::Unknown,
            }
        } else {
            Poll::Expired
        };
        if matches!(answer, Poll::Denied | Poll::Expired) {
            state.remove(&key);
        }
        answer
    }

    /// Forgets requests that are over, and wrong codes that no longer count.
    pub fn sweep(&self) {
        let now = self.now();
        let mut guard = self.state.lock();
        let state = &mut *guard;
        state.drop_over(now);
        state.wrong.retain(|_, times| {
            forget_old(times, now);
            !times.is_empty()
        });
        forget_old(&mut state.wrong_in_all, now);
    }

    fn now(&self) -> Instant {
        Instant::now() + *self.later.lock()
    }

    /// Moves this store's clock on by `by`, as if that much time went by
    /// (tests: here and the integration tests, which can't wait ten
    /// minutes).
    #[doc(hidden)]
    pub fn wait(&self, by: Duration) {
        *self.later.lock() += by;
    }

    /// The interval of the request of `device_code` (tests).
    #[cfg(test)]
    fn interval(&self, device_code: &str) -> Option<Duration> {
        self.state.lock().requests.get(&hash_token(device_code)).map(|entry| entry.interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TV: &str = "https://pley-media-server.github.io/pley/oauth/android.json";
    const DESKTOP: &str = "https://pley-media-server.github.io/pley/oauth/desktop.json";

    fn app() -> AppSnapshot {
        AppSnapshot {
            id: TV.into(),
            host: "pley-media-server.github.io".into(),
            name: Some("Pley for Android".into()),
            first_party: true,
            refresh: true,
        }
    }

    fn library() -> Scopes {
        Scopes::parse("library").unwrap()
    }

    fn ip(address: &str) -> Option<IpAddr> {
        Some(address.parse().unwrap())
    }

    fn start(devices: &Devices) -> Started {
        devices.create(app(), library(), ip("203.0.113.2")).unwrap()
    }

    /// A request whose user code is `code`.
    fn start_with(devices: &Devices, code: &str) -> Started {
        devices.create_drawing(app(), library(), ip("203.0.113.2").into(), || code.to_owned()).unwrap()
    }

    /// The request of `started`, typed in by `user_id` and shown on the page
    /// of pending request `page`: its key.
    fn on_page(devices: &Devices, started: &Started, user_id: i64, page: &str) -> String {
        let found = devices.find(&started.user_code, user_id).unwrap();
        assert!(devices.link(&found.key, None, page));
        found.key
    }

    #[test]
    fn user_codes_use_the_consonant_alphabet() {
        let mut counts: HashMap<char, usize> = HashMap::new();
        for _ in 0..10_000 {
            let code = new_user_code();
            assert_eq!(code.len(), CODE_LETTERS, "{code}");
            for c in code.chars() {
                *counts.entry(c).or_default() += 1;
            }
        }
        // Only the 20 letters, each drawn about 4,000 times in 80,000.
        assert_eq!(counts.len(), ALPHABET.len(), "{counts:?}");
        for (c, count) in &counts {
            assert!(ALPHABET.contains(&(*c as u8)), "{c}");
            assert!((3_600..=4_400).contains(count), "{c} came {count} times");
        }
        // Exactly: each letter stands for 12 of the 256 bytes, and the 16
        // left over are drawn again.
        let mut bytes_per_letter: HashMap<char, usize> = HashMap::new();
        for c in (0..=255).filter_map(letter) {
            *bytes_per_letter.entry(c).or_default() += 1;
        }
        assert!(bytes_per_letter.values().all(|bytes| *bytes == 12), "{bytes_per_letter:?}");
        assert_eq!(bytes_per_letter.len(), 20);
        assert_eq!((letter(239), letter(240), letter(255)), (Some('Z'), None, None));

        // Unique among the codes kept: a clash is drawn again...
        let devices = Devices::default();
        let mut draws = ["BBBBBBBB", "BBBBBBBB", "CCCCCCCC"].into_iter().map(str::to_owned);
        let first = devices.create_drawing(app(), library(), Asker::default(), || draws.next().unwrap()).unwrap();
        let second = devices.create_drawing(app(), library(), Asker::default(), || draws.next().unwrap()).unwrap();
        assert_eq!((first.user_code.as_str(), second.user_code.as_str()), ("BBBBBBBB", "CCCCCCCC"));
        assert_eq!(devices.find("CCCCCCCC", 1).unwrap().key, hash_token(&second.device_code));
        // The letters only: the dash is for people.
        let letters = start(&devices).user_code;
        assert!(letters.len() == CODE_LETTERS && normalize(&letters).as_deref() == Some(letters.as_str()), "{letters}");
        // Device codes are 256 bits, kept as SHA-256 only.
        assert_eq!(first.device_code.len(), 43);
        {
            let state = devices.state.lock();
            assert!(state.requests.contains_key(&hash_token(&first.device_code)));
            assert!(!state.requests.contains_key(&first.device_code));
        }
        // A code is drawn again only while the request that had it is kept.
        devices.wait(LIFETIME);
        devices.sweep();
        assert_eq!(start_with(&devices, "BBBBBBBB").user_code, "BBBBBBBB");
    }

    #[test]
    fn codes_are_shown_in_two_halves_and_read_as_people_type_them() {
        assert_eq!(shown("BCDFGHJK"), "BCDF-GHJK");
        for typed in ["BCDF-GHJK", "bcdf-ghjk", " bcdf ghjk ", "BCDFGHJK", "b-c-d-f-g-h-j-k", "BcDf\tGhJk"] {
            assert_eq!(normalize(typed).as_deref(), Some("BCDFGHJK"), "{typed:?}");
        }
        for typed in [
            "",
            "BCDF-GHJ",
            "BCDF-GHJKL",
            "BCDF-GHJA",
            "BCDF-GHJ1",
            "BCDF_GHJK",
            "BCDF\u{2013}GHJK",
            "\u{412}CDF-GHJK",
            "BCDF-GHJ\u{1E30}",
        ] {
            assert_eq!(normalize(typed), None, "{typed:?}");
        }
    }

    #[test]
    fn codes_expire_after_ten_minutes() {
        let devices = Devices::default();
        let started = start(&devices);
        devices.wait(LIFETIME - Duration::from_secs(1));
        let found = devices.find(&started.user_code, 1).unwrap();
        assert!(found.left <= Duration::from_secs(1) && found.left > Duration::ZERO, "{:?}", found.left);
        assert_eq!(devices.poll(&started.device_code, TV), Poll::Pending);

        devices.wait(Duration::from_secs(1));
        // The page finds no request for it (a wrong code, as any other)...
        assert_eq!(devices.find(&started.user_code, 1), Err(Refused::Unknown));
        // ...and the device is told once; then the request is gone.
        assert_eq!(devices.poll(&started.device_code, TV), Poll::Expired);
        assert_eq!(devices.poll(&started.device_code, TV), Poll::Unknown);

        // An answer comes too late too, and nobody can answer one that is over.
        let late = start(&devices);
        let key = on_page(&devices, &late, 1, "page");
        devices.wait(LIFETIME);
        assert_eq!(devices.decide(&key, "page", true, 1, "session", library()), Err(Over));
        assert!(!devices.link(&key, Some("page"), "another page"));
        assert_eq!(devices.poll(&late.device_code, TV), Poll::Expired);
    }

    #[test]
    fn asking_too_soon_slows_a_device_down_by_five_seconds_each_time() {
        let devices = Devices::default();
        let started = start(&devices);
        let poll = || devices.poll(&started.device_code, TV);
        let interval = || devices.interval(&started.device_code).unwrap().as_secs();
        assert_eq!(poll(), Poll::Pending, "the first question is never too soon");
        assert_eq!(interval(), 5);
        assert_eq!(poll(), Poll::SlowDown);
        assert_eq!(interval(), 10);
        devices.wait(Duration::from_secs(9));
        assert_eq!(poll(), Poll::SlowDown, "9 seconds are too soon for 10");
        assert_eq!(interval(), 15);
        devices.wait(Duration::from_secs(15));
        assert_eq!(poll(), Poll::Pending);
        assert_eq!(interval(), 15, "it stays slower");

        // Another app's question changes nothing, not even the time of the
        // last one.
        devices.wait(Duration::from_secs(15));
        assert_eq!(devices.poll(&started.device_code, DESKTOP), Poll::Unknown);
        assert_eq!(poll(), Poll::Pending);
        assert_eq!(interval(), 15);

        // On a consent page, it is still waiting.
        let key = on_page(&devices, &started, 1, "page");
        devices.wait(Duration::from_secs(15));
        assert_eq!(poll(), Poll::Pending);
        assert_eq!(poll(), Poll::SlowDown);
        assert_eq!(interval(), 20);

        // Answered: the answer comes at once, however soon.
        devices.decide(&key, "page", true, 1, "session", library()).unwrap();
        assert!(matches!(poll(), Poll::Allowed(_)));
    }

    #[test]
    fn a_request_is_answered_once() {
        let devices = Devices::default();
        let started = start(&devices);
        let found = devices.find(&started.user_code, 1).unwrap();
        assert_eq!((&found.app, found.scopes, &found.deciding), (&app(), library(), &None));

        // Shown on one page at a time.
        assert!(devices.link(&found.key, None, "page-1"));
        assert!(!devices.link(&found.key, None, "page-2"), "someone was quicker");
        assert_eq!(devices.find(&started.user_code, 2).unwrap().deciding.as_deref(), Some("page-1"));
        assert!(!devices.link(&found.key, Some("page-3"), "page-2"), "only from the page it is on");
        assert!(devices.link(&found.key, Some("page-1"), "page-2"));

        // Only that page's answer counts, and only once.
        assert_eq!(devices.decide(&found.key, "page-1", true, 1, "session", library()), Err(Over));
        assert_eq!(devices.decide("made-up", "page-2", true, 1, "session", library()), Err(Over));
        assert_eq!(devices.decide(&found.key, "page-2", true, 7, "session", library()), Ok(()));
        assert_eq!(devices.decide(&found.key, "page-2", false, 7, "session", library()), Err(Over));
        assert_eq!(devices.find(&started.user_code, 1), Err(Refused::Used));
        assert!(!devices.link(&found.key, Some("page-2"), "page-3"));

        // The device gets it once, acting as whoever allowed it, from their
        // session.
        let grant =
            DeviceGrant { app: app(), user_id: 7, approved_by: 7, session_hash: "session".into(), scopes: library() };
        assert_eq!(devices.poll(&started.device_code, TV), Poll::Allowed(grant));
        assert_eq!(devices.poll(&started.device_code, TV), Poll::Unknown, "while its tokens are made");
        assert_eq!(devices.find(&started.user_code, 1), Err(Refused::Used));
        assert!(!devices.issued(&started.device_code), "nothing ended meanwhile");
        assert_eq!(devices.poll(&started.device_code, TV), Poll::Unknown);
        assert_eq!(devices.find(&started.user_code, 1), Err(Refused::Unknown), "gone with its answer");

        // A refusal, too.
        let refused = start(&devices);
        let key = on_page(&devices, &refused, 1, "page");
        assert_eq!(devices.poll(&refused.device_code, DESKTOP), Poll::Unknown, "another app's");
        devices.decide(&key, "page", false, 1, "session", library()).unwrap();
        assert_eq!(devices.find(&refused.user_code, 1), Err(Refused::Used));
        assert_eq!(devices.poll(&refused.device_code, TV), Poll::Denied);
        assert_eq!(devices.poll(&refused.device_code, TV), Poll::Unknown);
        assert_eq!(devices.poll("made-up", TV), Poll::Unknown);
        assert!(devices.state.lock().by_code.is_empty());
    }

    #[test]
    fn a_devices_answer_ends_with_the_sign_ins_of_whoever_allowed_it() {
        let devices = Devices::default();
        let allowed_by = |user_id: i64| {
            let started = start(&devices);
            let key = on_page(&devices, &started, user_id, "page");
            devices.decide(&key, "page", true, user_id, "session", library()).unwrap();
            started
        };
        let bobs = allowed_by(2);
        let bobs_issuing = allowed_by(2);
        assert!(matches!(devices.poll(&bobs_issuing.device_code, TV), Poll::Allowed(_)));
        let carols = allowed_by(3);
        let waiting = start(&devices);
        let deciding = start(&devices);
        on_page(&devices, &deciding, 2, "bob's page");

        devices.forget_user(2);
        // Bob's TV is told no, once, and its code can't be used again...
        assert_eq!(devices.find(&bobs.user_code, 1), Err(Refused::Used));
        assert_eq!(devices.poll(&bobs.device_code, TV), Poll::Denied);
        assert_eq!(devices.poll(&bobs.device_code, TV), Poll::Unknown);
        // ...and what the one being signed in got ends at once...
        assert!(devices.issued(&bobs_issuing.device_code));
        // ...while others' answers, and requests nobody answered, stay.
        assert!(matches!(devices.poll(&carols.device_code, TV), Poll::Allowed(grant) if grant.user_id == 3));
        assert!(!devices.issued(&carols.device_code));
        assert_eq!(devices.poll(&waiting.device_code, TV), Poll::Pending);
        assert_eq!(devices.poll(&deciding.device_code, TV), Poll::Pending);
    }

    #[test]
    fn requests_are_capped_and_none_in_progress_is_dropped() {
        let devices = Devices::default();
        let create = |address: Option<IpAddr>| devices.create(app(), library(), address);
        let first = create(ip("203.0.113.1")).unwrap();
        for _ in 1..MAX_PER_ADDRESS {
            create(ip("203.0.113.1")).unwrap();
        }
        assert_eq!(create(ip("203.0.113.1")), Err(Full));
        // The same address on an IPv6 socket.
        assert_eq!(create(ip("::ffff:203.0.113.1")), Err(Full));
        // Every address has its own 20; unknown ones share theirs.
        create(ip("203.0.113.2")).unwrap();
        for _ in 0..MAX_PER_ADDRESS {
            create(None).unwrap();
        }
        assert_eq!(create(None), Err(Full));
        // An IPv6 /64 is one address: one host may use all of it.
        for i in 0..MAX_PER_ADDRESS {
            create(ip(&format!("2001:db8:1:2:{i:x}::{i:x}"))).unwrap();
        }
        assert_eq!(create(ip("2001:db8:1:2:ffff:ffff:ffff:ffff")), Err(Full));
        create(ip("2001:db8:1:3::1")).unwrap();
        // A home's /56 holds 256 of those: 50 at most from all of them.
        for i in 0..MAX_PER_SITE - MAX_PER_ADDRESS - 1 {
            create(ip(&format!("2001:db8:1:{:x}::1", 0x10 + i))).unwrap();
        }
        assert_eq!(create(ip("2001:db8:1:ff::1")), Err(Full), "the /56 is full");
        create(ip("2001:db8:1:100::1")).unwrap();

        // 1,000 in all.
        let made = 3 * MAX_PER_ADDRESS + 2 + (MAX_PER_SITE - MAX_PER_ADDRESS - 1) + 1;
        for i in 0..MAX_REQUESTS - made {
            create(ip(&format!("198.51.100.{}", i / MAX_PER_ADDRESS))).unwrap();
        }
        assert_eq!(create(ip("192.0.2.9")), Err(Full));
        // Nothing in progress was dropped to make room.
        assert_eq!(devices.poll(&first.device_code, TV), Poll::Pending);
        assert_eq!(devices.state.lock().requests.len(), MAX_REQUESTS);

        // Requests that are over do make room.
        devices.wait(LIFETIME);
        create(ip("203.0.113.1")).unwrap();
        let state = devices.state.lock();
        assert_eq!((state.requests.len(), state.by_code.len()), (1, 1), "the expired ones went");
    }

    #[test]
    fn wrong_codes_are_throttled_per_person_and_for_the_server() {
        let devices = Devices::default();
        let started = start_with(&devices, "CCCCCCCC");
        for _ in 0..MAX_WRONG_PER_PERSON {
            assert_eq!(devices.find("BBBBBBBB", 1), Err(Refused::Unknown));
        }
        // Then even the right code isn't looked up...
        assert_eq!(devices.find(&started.user_code, 1), Err(Refused::Throttled));
        // ...but someone else's attempts are their own.
        assert!(devices.find(&started.user_code, 2).is_ok());
        // Right codes and used ones don't count.
        let key = on_page(&devices, &started, 2, "page");
        devices.decide(&key, "page", false, 2, "session", library()).unwrap();
        for _ in 0..MAX_WRONG_PER_PERSON {
            assert_eq!(devices.find(&started.user_code, 3), Err(Refused::Used));
        }
        assert_eq!(devices.find(&started.user_code, 3), Err(Refused::Used));
        // Ten minutes after the first, they no longer count.
        devices.wait(Duration::from_secs(599));
        assert_eq!(devices.find("BBBBBBBB", 1), Err(Refused::Throttled));
        devices.wait(Duration::from_secs(1));
        assert_eq!(devices.find("BBBBBBBB", 1), Err(Refused::Unknown), "counting again");

        // 300 for everyone together.
        let devices = Devices::default();
        let started = start_with(&devices, "CCCCCCCC");
        for user_id in 0..(MAX_WRONG / MAX_WRONG_PER_PERSON) as i64 {
            for _ in 0..MAX_WRONG_PER_PERSON {
                assert_eq!(devices.find("BBBBBBBB", user_id), Err(Refused::Unknown));
            }
        }
        assert_eq!(devices.find(&started.user_code, 1_000), Err(Refused::Throttled));
        // The sweep forgets them once they no longer count.
        devices.wait(WRONG_CODE_WINDOW);
        devices.sweep();
        let state = devices.state.lock();
        assert!(state.wrong.is_empty() && state.wrong_in_all.is_empty());
    }

    #[test]
    fn the_sweep_forgets_requests_that_are_over() {
        let devices = Devices::default();
        let old = [start(&devices), start(&devices)];
        devices.wait(Duration::from_secs(300));
        let new = start(&devices);
        devices.wait(Duration::from_secs(300));
        devices.sweep();
        {
            let state = devices.state.lock();
            assert_eq!((state.requests.len(), state.by_code.len()), (1, 1));
        }
        assert_eq!(devices.poll(&old[0].device_code, TV), Poll::Unknown, "gone, not just expired");
        assert_eq!(devices.poll(&new.device_code, TV), Poll::Pending);
    }
}
