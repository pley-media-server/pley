//! Quick Connect, the Jellyfin apps' way of signing in with a code
//! (docs/COMPATIBILITY.md): the app asks the Jellyfin-compatible API for a
//! code and shows it, the person types it on Pley's `/link` page (or in a
//! Jellyfin app signed in already) and allows the app, and the app, which
//! asks every few seconds, then trades its secret for a token for other
//! apps. Two-step sign-in happens where the person is signed in, so
//! accounts that use it sign in to Jellyfin's apps too. In memory only,
//! like the rest of a sign-in in progress.
//!
//! * Anyone may start one, as anyone may start a TV's sign-in
//!   ([`super::devices`]): at most 10 at a time from one address (an IPv6
//!   /64, as there), 25 from one IPv6 /56, and 250 in all. Nothing in
//!   progress is dropped to make room, but a device that asks again, from
//!   the same address with the same device id, replaces its request that
//!   no one has typed the code of yet: some apps ask each time their
//!   sign-in screen opens.
//! * A request lasts 10 minutes, as Jellyfin's do. Once its code is typed on
//!   the page it lasts 5 minutes more at least, for the person to decide,
//!   and once allowed 2 minutes more at least, for the app, which may ask
//!   only every few seconds, to get its token; never past 30 minutes from
//!   when it was made. Its secret carries 256 random bits, and only its
//!   SHA-256 is kept; its code is 6 digits, as Jellyfin's are, which apps
//!   that approve codes take and nothing else. The person compares it with
//!   their screen, and the consent page says which app, device and address
//!   asked.
//! * Only signed-in people try codes, and wrong ones count: past 10 by one
//!   person or 300 by everyone in 10 minutes, no code is looked up.
//! * Allowed, the app gets its token once. When the account's sign-ins end
//!   before it has it (a new password, signing out everywhere, disabling,
//!   deletion), the request is over instead ([`QuickConnect::forget_user`]).

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::devices::{forget_old, network, site};
use crate::auth::compatibility::Origins;
use crate::auth::hash_token;

/// How long a request lasts.
pub const LIFETIME: Duration = Duration::from_secs(600);
/// How long a request lasts at least once its code was typed on the page.
pub const TO_DECIDE: Duration = Duration::from_secs(300);
/// How long an allowed request lasts at least, for its app to ask.
pub const TO_COLLECT: Duration = Duration::from_secs(120);
/// The longest a request lasts, from when it was made.
pub const MAX_LIFETIME: Duration = Duration::from_secs(1800);
/// Requests in progress from one address.
pub const MAX_PER_ADDRESS: usize = 10;
/// Requests in progress from one IPv6 site (a /56).
pub const MAX_PER_SITE: usize = 25;
/// Requests in progress in all: a code typed at random finds one in 4,000
/// at most.
pub const MAX_REQUESTS: usize = 250;
/// Wrong codes one person may type within 10 minutes, as for TVs' codes
/// (which count on their own).
pub const MAX_WRONG_PER_PERSON: usize = 10;
/// Wrong codes everyone may type within the window.
pub const MAX_WRONG: usize = 300;
/// Digits in a code: 10^6 codes, as Jellyfin's.
pub const CODE_DIGITS: usize = 6;
/// Random numbers below this stand for a code; the others are drawn again,
/// so that every code is as likely (no modulo bias).
const FAIR: u32 = u32::MAX - u32::MAX % 1_000_000;

/// A new code: 6 digits, from the operating system's random generator.
pub fn new_code() -> String {
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        let number = u32::from_le_bytes(bytes);
        if number < FAIR {
            return format!("{:06}", number % 1_000_000);
        }
    }
}

/// A new secret: 256 random bits as 64 hexadecimal characters, as
/// Jellyfin writes them.
fn new_secret() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    hex::encode(bytes)
}

/// A code as the person typed it, without spaces or dashes; `None` when
/// that isn't 6 digits.
pub fn normalize(typed: &str) -> Option<String> {
    let code: String = typed.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    (code.len() == CODE_DIGITS && code.bytes().all(|b| b.is_ascii_digit())).then_some(code)
}

/// The app asking, as it says it is in its `Authorization` header:
/// unchecked, since anyone may start a request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asker {
    /// Its name ("Findroid").
    pub app: Option<String>,
    pub version: Option<String>,
    /// The device's name ("Living room").
    pub device: Option<String>,
    pub device_id: Option<String>,
    /// The address it asked from, as [`crate::auth::SourceIp`] reads it.
    pub address: Option<IpAddr>,
}

impl Asker {
    /// How the logs name it: "Findroid on Phone".
    pub fn label(&self) -> String {
        let app = self.app.as_deref().unwrap_or("A Jellyfin app");
        match &self.device {
            Some(device) => format!("{app} on {device}"),
            None => app.to_owned(),
        }
    }
}

/// Who allowed an app, which has to still be signed in when the app gets
/// its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approver {
    /// On Pley's page, in the browser session whose token has this SHA-256.
    Session(String),
    /// In a Jellyfin app signed in already, with this token for other apps.
    Token(i64),
}

/// What a person allowed an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowed {
    /// The account the app signs in to: the person's.
    pub user_id: i64,
    pub approver: Approver,
    /// Where the token may be used from: anywhere when allowed on Pley's
    /// page, the approving token's origins when allowed with it.
    pub origins: Origins,
}

/// Where a request is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    /// Nobody typed its code yet.
    Pending,
    /// Shown on Pley's consent page, as request `id`, to `user_id` in the
    /// browser session whose token has the SHA-256 `session_hash`: that
    /// page answers.
    Deciding {
        id: String,
        user_id: i64,
        session_hash: String,
    },
    Allowed(Allowed),
    /// Allowed, and the app's token is being made: no other question gets
    /// it. `signed_out` once the account's sign-ins ended meanwhile.
    Issuing {
        user_id: i64,
        signed_out: bool,
    },
}

struct Entry {
    code: String,
    asker: Asker,
    /// The [`network`] it came from, when known.
    network: Option<IpAddr>,
    created: Instant,
    /// When it is over: [`LIFETIME`] after it was made, later once typed or
    /// allowed ([`Entry::last`]).
    expires: Instant,
    /// When it was made, in seconds since 1970, for the app's `DateAdded`.
    added_at: i64,
    status: Status,
}

impl Entry {
    fn alive(&self, now: Instant) -> bool {
        now < self.expires
    }

    /// Lasts `at_least` from `now`, never past [`MAX_LIFETIME`] from when it
    /// was made.
    fn last(&mut self, now: Instant, at_least: Duration) {
        let until = (now + at_least).min(self.created + MAX_LIFETIME);
        self.expires = self.expires.max(until);
    }
}

#[derive(Default)]
struct State {
    /// By the SHA-256 of their secret.
    requests: HashMap<String, Entry>,
    /// Their keys, by code.
    by_code: HashMap<String, String>,
    /// When each person typed wrong codes, oldest first.
    wrong: HashMap<i64, VecDeque<Instant>>,
    /// When anyone did.
    wrong_in_all: VecDeque<Instant>,
}

impl State {
    fn remove(&mut self, key: &str) {
        if let Some(entry) = self.requests.remove(key) {
            self.by_code.remove(&entry.code);
        }
    }

    fn drop_over(&mut self, now: Instant) {
        self.requests.retain(|_, entry| entry.alive(now));
        let requests = &self.requests;
        self.by_code.retain(|_, key| requests.contains_key(key));
    }

    /// Counts a wrong code of `user_id`; whether they, or everyone, typed
    /// too many to look one up.
    fn throttled(&mut self, user_id: i64, now: Instant) -> bool {
        let mine = self.wrong.get_mut(&user_id).map_or(0, |times| {
            forget_old(times, now);
            times.len()
        });
        forget_old(&mut self.wrong_in_all, now);
        mine >= MAX_WRONG_PER_PERSON || self.wrong_in_all.len() >= MAX_WRONG
    }

    fn count_wrong(&mut self, user_id: i64, now: Instant) {
        self.wrong.entry(user_id).or_default().push_back(now);
        self.wrong_in_all.push_back(now);
    }
}

/// A request just made: what the app shows and keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    pub secret: String,
    pub code: String,
    pub added_at: i64,
}

/// A request, as the app that keeps its secret is told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub code: String,
    pub asker: Asker,
    pub added_at: i64,
    /// Allowed: the app may now trade its secret for a token.
    pub authenticated: bool,
}

/// A request whose code the person typed on Pley's page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The request, for [`QuickConnect::show`].
    pub key: String,
    pub asker: Asker,
    /// The page that shows it already, if any: its request id, person and
    /// session (the SHA-256 of its token).
    pub deciding: Option<(String, i64, String)>,
}

/// Why a typed code leads to no request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Too many wrong codes lately, by this person or by everyone.
    Throttled,
    /// No request in progress has it: a wrong code.
    Unknown,
    /// Its request was answered already, or another person's page shows it.
    Used,
}

/// What an app trading its secret gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Take {
    /// No request in progress has the secret.
    Unknown,
    /// Nobody allowed it yet.
    Pending,
    /// Allowed: this answer comes once. Once its token is made (or not),
    /// [`QuickConnect::issued`] ends the request.
    Allowed(Allowed),
}

/// Too many requests are in progress, from one address or in all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

/// The request is over, or another page shows it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Over;

/// The requests in progress, and the wrong codes of the last 10 minutes.
#[derive(Default)]
pub struct QuickConnect {
    state: Mutex<State>,
    /// How far tests moved this store's clock on ([`QuickConnect::wait`]).
    later: Mutex<Duration>,
}

impl QuickConnect {
    /// Starts a request of the app `asker` describes: what it shows and
    /// keeps. Refused while its address (its /64 for IPv6) has 10 in
    /// progress, its IPv6 /56 25, or everyone 250, once the app's own
    /// request no one typed the code of yet has made room.
    pub fn start(&self, asker: Asker, added_at: i64) -> Result<Started, Full> {
        self.start_drawing(asker, added_at, new_code)
    }

    fn start_drawing(&self, asker: Asker, added_at: i64, mut draw: impl FnMut() -> String) -> Result<Started, Full> {
        let now = self.now();
        let network = asker.address.map(network);
        let mut state = self.state.lock();
        state.drop_over(now);
        // The same device asking again, from the same place: its earlier
        // code goes, unless someone is deciding on it already.
        if let Some(device) = asker.device_id.as_deref() {
            let again: Vec<String> = state
                .requests
                .iter()
                .filter(|(_, entry)| {
                    entry.status == Status::Pending
                        && entry.network == network
                        && entry.asker.device_id.as_deref() == Some(device)
                })
                .map(|(key, _)| key.clone())
                .collect();
            for key in again {
                state.remove(&key);
            }
        }
        let from_there = state.requests.values().filter(|entry| entry.network == network).count();
        let site = site(network);
        let from_site = site
            .map_or(0, |site| state.requests.values().filter(|entry| self::site(entry.network) == Some(site)).count());
        if state.requests.len() >= MAX_REQUESTS || from_there >= MAX_PER_ADDRESS || from_site >= MAX_PER_SITE {
            return Err(Full);
        }
        let code = loop {
            let code = draw();
            if !state.by_code.contains_key(&code) {
                break code;
            }
        };
        let secret = new_secret();
        let key = hash_token(&secret);
        state.by_code.insert(code.clone(), key.clone());
        let entry = Entry {
            code: code.clone(),
            asker,
            network,
            created: now,
            expires: now + LIFETIME,
            added_at,
            status: Status::Pending,
        };
        state.requests.insert(key, entry);
        Ok(Started { secret, code, added_at })
    }

    /// The request of `secret`, while it is in progress.
    pub fn seen(&self, secret: &str) -> Option<Seen> {
        let now = self.now();
        let state = self.state.lock();
        let entry = state.requests.get(&hash_token(secret)).filter(|entry| entry.alive(now))?;
        Some(Seen {
            code: entry.code.clone(),
            asker: entry.asker.clone(),
            added_at: entry.added_at,
            authenticated: matches!(entry.status, Status::Allowed(_) | Status::Issuing { .. }),
        })
    }

    /// The request whose code `code` (as [`normalize`] gives it) `user_id`
    /// typed on Pley's page. A code that leads to no request in progress is
    /// a wrong one, and counts: past 10 by the person or 300 by everyone in
    /// 10 minutes, no code is looked up.
    pub fn find(&self, code: &str, user_id: i64) -> Result<Found, Refused> {
        let now = self.now();
        let mut state = self.state.lock();
        if state.throttled(user_id, now) {
            return Err(Refused::Throttled);
        }
        let found = state.by_code.get(code).and_then(|key| Some((key, state.requests.get(key)?)));
        match found {
            Some((key, entry)) if entry.alive(now) => {
                let deciding = match &entry.status {
                    Status::Pending => None,
                    Status::Deciding { id, user_id, session_hash } => {
                        Some((id.clone(), *user_id, session_hash.clone()))
                    }
                    Status::Allowed(_) | Status::Issuing { .. } => return Err(Refused::Used),
                };
                Ok(Found { key: key.clone(), asker: entry.asker.clone(), deciding })
            }
            _ => {
                state.count_wrong(user_id, now);
                Err(Refused::Unknown)
            }
        }
    }

    /// Shows request `key` on the consent page of `user_id`, in the browser
    /// session whose token has the SHA-256 `session_hash`, as request `id`:
    /// when it is still as it was found, waiting for its code (`seen` is
    /// `None`) or on the page of request `seen`. Shown, it lasts
    /// [`TO_DECIDE`] more at least, and the answer is how long it has left;
    /// `None` when someone was quicker, or when it is over. Only the page
    /// that shows it keeps it going: looking up a code shown elsewhere
    /// doesn't.
    pub fn show(&self, key: &str, seen: Option<&str>, id: &str, user_id: i64, session_hash: &str) -> Option<Duration> {
        let now = self.now();
        let mut state = self.state.lock();
        let entry = state.requests.get_mut(key).filter(|entry| entry.alive(now))?;
        let as_found = match (&entry.status, seen) {
            (Status::Pending, None) => true,
            (Status::Deciding { id: shown, .. }, Some(seen)) => shown == seen,
            _ => false,
        };
        if !as_found {
            return None;
        }
        entry.status = Status::Deciding { id: id.to_owned(), user_id, session_hash: session_hash.to_owned() };
        entry.last(now, TO_DECIDE);
        Some(entry.expires - now)
    }

    /// The answer of the consent page showing request `id`, which only its
    /// person and session may give: allowed, the app signs in to their
    /// account, from anywhere; refused, the request is over, and the app is
    /// told it is unknown.
    pub fn decide(&self, id: &str, user_id: i64, session_hash: &str, allow: bool) -> Result<Asker, Over> {
        let now = self.now();
        let mut state = self.state.lock();
        let key = state
            .requests
            .iter()
            .find(|(_, entry)| {
                entry.alive(now)
                    && matches!(&entry.status, Status::Deciding { id: shown, user_id: by, session_hash: hash }
                        if shown == id && *by == user_id && hash == session_hash)
            })
            .map(|(key, _)| key.clone())
            .ok_or(Over)?;
        let asker = state.requests[&key].asker.clone();
        if allow {
            if let Some(entry) = state.requests.get_mut(&key) {
                entry.status = Status::Allowed(Allowed {
                    user_id,
                    approver: Approver::Session(session_hash.to_owned()),
                    origins: Origins::default(),
                });
                entry.last(now, TO_COLLECT);
            }
        } else {
            state.remove(&key);
        }
        Ok(asker)
    }

    /// `user_id` allows the request of code `code` from a Jellyfin app
    /// signed in with token `token_id`, which may be used from `origins`.
    /// Wrong codes count as on Pley's page. A code another person's page
    /// shows, or allowed by someone else, is used.
    pub fn authorize(&self, code: &str, user_id: i64, token_id: i64, origins: Origins) -> Result<Asker, Refused> {
        let now = self.now();
        let mut state = self.state.lock();
        if state.throttled(user_id, now) {
            return Err(Refused::Throttled);
        }
        let Some(key) = state.by_code.get(code).filter(|key| state.requests.get(*key).is_some_and(|e| e.alive(now)))
        else {
            state.count_wrong(user_id, now);
            return Err(Refused::Unknown);
        };
        let key = key.clone();
        let entry = state.requests.get_mut(&key).ok_or(Refused::Unknown)?;
        match &entry.status {
            Status::Pending => {}
            Status::Deciding { user_id: by, .. } if *by == user_id => {}
            // Allowed by this person already: the same answer again.
            Status::Allowed(allowed) if allowed.user_id == user_id => return Ok(entry.asker.clone()),
            _ => return Err(Refused::Used),
        }
        entry.status = Status::Allowed(Allowed { user_id, approver: Approver::Token(token_id), origins });
        entry.last(now, TO_COLLECT);
        Ok(entry.asker.clone())
    }

    /// The answer for an app trading `secret`. Decided under the lock, so
    /// of two at once only one gets an allowed request; the others are told
    /// it is unknown, as it is over for them.
    pub fn take(&self, secret: &str) -> Take {
        let now = self.now();
        let mut state = self.state.lock();
        let Some(entry) = state.requests.get_mut(&hash_token(secret)).filter(|entry| entry.alive(now)) else {
            return Take::Unknown;
        };
        match &entry.status {
            Status::Pending | Status::Deciding { .. } => Take::Pending,
            Status::Issuing { .. } => Take::Unknown,
            Status::Allowed(allowed) => {
                let allowed = allowed.clone();
                entry.status = Status::Issuing { user_id: allowed.user_id, signed_out: false };
                Take::Allowed(allowed)
            }
        }
    }

    /// The app of `secret` got its token, or didn't: its request is over.
    /// True when the account's sign-ins ended while it was being made: end
    /// what it got.
    pub fn issued(&self, secret: &str) -> bool {
        let key = hash_token(secret);
        let mut state = self.state.lock();
        let signed_out = matches!(
            state.requests.get(&key).map(|entry| &entry.status),
            Some(Status::Issuing { signed_out: true, .. })
        );
        state.remove(&key);
        signed_out
    }

    /// The sign-ins of `user_id` ended: the apps they allowed that don't
    /// have their token yet get none, and one whose token is being made
    /// right now loses it ([`QuickConnect::issued`]).
    pub fn forget_user(&self, user_id: i64) {
        let mut state = self.state.lock();
        let allowed: Vec<String> = state
            .requests
            .iter()
            .filter(|(_, entry)| matches!(&entry.status, Status::Allowed(allowed) if allowed.user_id == user_id))
            .map(|(key, _)| key.clone())
            .collect();
        for key in allowed {
            state.remove(&key);
        }
        for entry in state.requests.values_mut() {
            if let Status::Issuing { user_id: by, signed_out } = &mut entry.status
                && *by == user_id
            {
                *signed_out = true;
            }
        }
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
    /// (tests, which can't wait ten minutes).
    #[doc(hidden)]
    pub fn wait(&self, by: Duration) {
        *self.later.lock() += by;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asker(address: &str, device_id: &str) -> Asker {
        Asker {
            app: Some("Findroid".into()),
            version: Some("0.16".into()),
            device: Some("Phone".into()),
            device_id: Some(device_id.into()),
            address: Some(address.parse().unwrap()),
        }
    }

    #[test]
    fn codes_are_six_digits_and_secrets_256_bits() {
        for _ in 0..200 {
            let code = new_code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| b.is_ascii_digit()), "{code}");
        }
        let secret = new_secret();
        assert_eq!(secret.len(), 64);
        assert_eq!(normalize(" 123 456 ").as_deref(), Some("123456"));
        assert_eq!(normalize("123-456").as_deref(), Some("123456"));
        for wrong in ["12345", "1234567", "12a456", "BCDFGHJK", ""] {
            assert_eq!(normalize(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn allowed_on_the_page_the_app_gets_its_token_once() {
        let store = QuickConnect::default();
        let started = store.start(asker("192.168.1.20", "a"), 100).unwrap();
        assert_eq!(store.take(&started.secret), Take::Pending);
        let found = store.find(&started.code, 1).unwrap();
        assert_eq!(found.asker.app.as_deref(), Some("Findroid"));
        assert!(store.show(&found.key, None, "page", 1, "session").is_some());
        assert!(!store.seen(&started.secret).unwrap().authenticated);
        assert_eq!(store.decide("page", 2, "session", true), Err(Over), "someone else's page");
        assert_eq!(store.decide("page", 1, "other", true), Err(Over), "another session's");
        store.decide("page", 1, "session", true).unwrap();
        assert!(store.seen(&started.secret).unwrap().authenticated);
        assert_eq!(store.find(&started.code, 1), Err(Refused::Used));

        let Take::Allowed(allowed) = store.take(&started.secret) else { panic!("allowed") };
        assert_eq!(allowed.user_id, 1);
        assert_eq!(allowed.approver, Approver::Session("session".into()));
        assert!(allowed.origins.anywhere());
        assert_eq!(store.take(&started.secret), Take::Unknown, "once");
        assert!(!store.issued(&started.secret));
        assert!(store.seen(&started.secret).is_none(), "over");
    }

    #[test]
    fn refused_the_request_is_over() {
        let store = QuickConnect::default();
        let started = store.start(asker("192.168.1.20", "a"), 0).unwrap();
        let found = store.find(&started.code, 1).unwrap();
        assert!(store.show(&found.key, None, "page", 1, "session").is_some());
        store.decide("page", 1, "session", false).unwrap();
        assert_eq!(store.take(&started.secret), Take::Unknown);
        assert!(store.seen(&started.secret).is_none());
    }

    #[test]
    fn typed_and_allowed_requests_last_longer_but_never_past_half_an_hour() {
        let store = QuickConnect::default();
        let started = store.start(asker("192.168.1.20", "a"), 0).unwrap();
        store.wait(LIFETIME - Duration::from_secs(10));
        let found = store.find(&started.code, 1).unwrap();
        assert_eq!(store.show(&found.key, None, "page", 1, "s"), Some(TO_DECIDE), "typed at the last moment");
        store.wait(TO_DECIDE - Duration::from_secs(1));
        store.decide("page", 1, "s", true).unwrap();
        store.wait(TO_COLLECT - Duration::from_secs(1));
        assert!(matches!(store.take(&started.secret), Take::Allowed(_)), "allowed at the last moment: time to ask");

        let started = store.start(asker("192.168.1.20", "b"), 0).unwrap();
        store.authorize(&started.code, 1, 7, Origins::default()).unwrap();
        store.wait(LIFETIME - Duration::from_secs(1));
        assert!(matches!(store.take(&started.secret), Take::Allowed(_)), "allowed in an app");

        // Looked up but shown on another page: no longer for that.
        let started = store.start(asker("192.168.1.20", "c"), 0).unwrap();
        store.wait(LIFETIME - Duration::from_secs(10));
        let found = store.find(&started.code, 1).unwrap();
        store.show(&found.key, None, "first", 1, "s").unwrap();
        store.wait(TO_DECIDE - Duration::from_secs(1));
        assert!(store.find(&started.code, 2).unwrap().deciding.is_some());
        store.wait(Duration::from_secs(1));
        assert_eq!(store.find(&started.code, 2).map(|_| ()), Err(Refused::Unknown), "over");

        // Shown again and again, it still ends half an hour after it was made.
        let started = store.start(asker("192.168.1.20", "d"), 0).unwrap();
        let key = store.find(&started.code, 1).unwrap().key;
        let step = Duration::from_secs(240);
        let mut seen = None;
        for n in 0..7 {
            let id = format!("page {n}");
            store.show(&key, seen.as_deref(), &id, 1, "s").unwrap();
            seen = Some(id);
            store.wait(step);
        }
        // The real clock runs on too, between making it and now.
        let left = store.show(&key, seen.as_deref(), "last", 1, "s").unwrap();
        assert!(left <= MAX_LIFETIME - step * 7 && left > MAX_LIFETIME - step * 7 - Duration::from_secs(1), "{left:?}");
        store.wait(MAX_LIFETIME - step * 7);
        assert_eq!(store.find(&started.code, 1).map(|_| ()), Err(Refused::Unknown), "over");
        assert!(store.seen(&started.secret).is_none());
    }

    #[test]
    fn a_jellyfin_app_signed_in_already_allows_with_its_origins() {
        let store = QuickConnect::default();
        let started = store.start(asker("192.168.1.20", "a"), 0).unwrap();
        let local = Origins::parse(&["local"]).unwrap();
        store.authorize(&started.code, 1, 7, local.clone()).unwrap();
        assert!(store.authorize(&started.code, 1, 7, local.clone()).is_ok(), "the same answer again");
        assert_eq!(store.authorize(&started.code, 2, 8, Origins::default()), Err(Refused::Used));
        let Take::Allowed(allowed) = store.take(&started.secret) else { panic!("allowed") };
        assert_eq!((allowed.approver, allowed.origins), (Approver::Token(7), local));
    }

    #[test]
    fn wrong_codes_count() {
        let store = QuickConnect::default();
        let started = store.start(asker("192.168.1.20", "a"), 0).unwrap();
        let wrong = if started.code == "000000" { "000001" } else { "000000" };
        for _ in 0..MAX_WRONG_PER_PERSON {
            assert_eq!(store.find(wrong, 1).map(|_| ()), Err(Refused::Unknown));
        }
        assert_eq!(store.find(&started.code, 1).map(|_| ()), Err(Refused::Throttled), "the right one waits too");
        assert_eq!(store.authorize(&started.code, 1, 1, Origins::default()).map(|_| ()), Err(Refused::Throttled));
        assert!(store.find(&started.code, 2).is_ok(), "someone else's are their own");
        store.wait(super::super::devices::WRONG_CODE_WINDOW);
        store.sweep();
        assert_eq!(store.find(&started.code, 1).map(|_| ()), Err(Refused::Unknown), "expired meanwhile");
    }

    #[test]
    fn requests_are_bounded_and_a_device_asking_again_replaces_its_own() {
        let store = QuickConnect::default();
        let first = store.start(asker("192.168.1.20", "tv"), 0).unwrap();
        let again = store.start(asker("192.168.1.20", "tv"), 0).unwrap();
        assert!(store.seen(&first.secret).is_none(), "replaced");
        assert!(store.seen(&again.secret).is_some());
        // Someone is deciding on it: it stays.
        let found = store.find(&again.code, 1).unwrap();
        assert!(store.show(&found.key, None, "page", 1, "s").is_some());
        store.start(asker("192.168.1.20", "tv"), 0).unwrap();
        assert!(store.seen(&again.secret).is_some());

        let store = QuickConnect::default();
        for n in 0..MAX_PER_ADDRESS {
            store.start(asker("192.168.1.20", &n.to_string()), 0).unwrap();
        }
        assert_eq!(store.start(asker("192.168.1.20", "more"), 0), Err(Full));
        assert!(store.start(asker("192.168.1.21", "more"), 0).is_ok(), "another address");
        for n in 0..MAX_PER_ADDRESS {
            store.start(asker(&format!("2001:db8:0:1::{n}"), &n.to_string()), 0).unwrap();
        }
        assert_eq!(store.start(asker("2001:db8:0:1::ff", "more"), 0), Err(Full), "one /64");
        for n in 0..(MAX_PER_SITE - MAX_PER_ADDRESS) {
            store.start(asker(&format!("2001:db8:0:{:x}::1", n + 2), "x"), 0).unwrap();
        }
        assert_eq!(store.start(asker("2001:db8:0:ff::1", "more"), 0), Err(Full), "one /56");
        store.wait(LIFETIME);
        assert!(store.start(asker("192.168.1.20", "later"), 0).is_ok(), "over ones make room");
    }

    #[test]
    fn codes_are_unique_among_those_kept() {
        let store = QuickConnect::default();
        let mut draws = ["111111", "111111", "222222"].into_iter().map(str::to_owned);
        let first = store.start_drawing(asker("10.0.0.1", "a"), 0, || draws.next().unwrap()).unwrap();
        let second = store.start_drawing(asker("10.0.0.2", "b"), 0, || draws.next().unwrap()).unwrap();
        assert_eq!((first.code.as_str(), second.code.as_str()), ("111111", "222222"));
    }

    #[test]
    fn sign_ins_ending_end_what_was_allowed() {
        let store = QuickConnect::default();
        let waiting = store.start(asker("10.0.0.1", "a"), 0).unwrap();
        store.authorize(&waiting.code, 1, 1, Origins::default()).unwrap();
        let issuing = store.start(asker("10.0.0.2", "b"), 0).unwrap();
        store.authorize(&issuing.code, 1, 1, Origins::default()).unwrap();
        assert!(matches!(store.take(&issuing.secret), Take::Allowed(_)));
        let other = store.start(asker("10.0.0.3", "c"), 0).unwrap();
        store.authorize(&other.code, 2, 2, Origins::default()).unwrap();

        store.forget_user(1);
        assert_eq!(store.take(&waiting.secret), Take::Unknown);
        assert!(store.issued(&issuing.secret), "signed out while it was made");
        assert!(matches!(store.take(&other.secret), Take::Allowed(_)), "another account's");
    }
}
