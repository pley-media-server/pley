//! Accounts and sessions: Argon2id password hashes, random session tokens stored
//! hashed (SHA-256) in the database, a short-lived in-memory cache so that
//! segment/image requests don't hit SQLite, and axum extractors.
//!
//! Apps signed in through pley's own page hold app tokens instead
//! ([`grants`]): they act without administrator rights, and only on the
//! routes their [`scopes`] open. Other apps and tools hold
//! [`compatibility`] tokens, which work on the compatibility API only.

pub mod compatibility;
pub mod grants;
pub mod scopes;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use axum::extract::{FromRequestParts, MatchedPath};
use axum::http::request::Parts;
use axum::http::{Method, header};
use base64::Engine;
use dashmap::DashMap;
use sha2::{Digest, Sha256};

use self::grants::{ACCESS_PREFIX, APP_TOKEN_PREFIX, IssueError, Issued, NewGrant, RefreshError, RefreshInput};
use self::scopes::{Need, Scope, Scopes};
use crate::db::Db;
use crate::error::AppError;
use crate::state::AppState;
use crate::util::now;

pub const COOKIE_NAME: &str = "pley_token";
pub const SESSION_TTL_SECS: i64 = 90 * 24 * 3600;
const CACHE_TTL: Duration = Duration::from_secs(60);
const TOUCH_INTERVAL_SECS: i64 = 600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    /// Administrator rights for this request: the account's role for pley's
    /// own sign-in, never for an app's token, and for a compatibility token
    /// only to see everyone's (with `activity`).
    pub is_admin: bool,
    /// The account is an administrator, whatever the token: what requests
    /// go by (an administrator's own are approved as made, and pley's apps
    /// allowed `approvals` manage them, see [`AuthUser::manages_requests`]).
    pub account_admin: bool,
    /// Every library, or only the ones in `user_libraries` (see [`crate::access`]).
    pub all_libraries: bool,
    /// Age limit for movies and shows; `None` = unrestricted.
    pub max_age: Option<i64>,
    /// The app acting for the user (an app token); `None`: pley's own sign-in.
    pub app: Option<Arc<AppAccess>>,
    /// The compatibility token of the request (the compatibility API's
    /// callers); `None`: a session or an app.
    pub compatibility: Option<CompatibilityAccess>,
}

/// What a compatibility token allows (see [`compatibility`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityAccess {
    pub token_id: i64,
    pub scopes: compatibility::Scopes,
}

/// What an app's token was allowed (see [`grants`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppAccess {
    pub grant_id: i64,
    /// The address of the app's client metadata document.
    pub client_id: Arc<str>,
    pub scopes: Scopes,
    /// Who allowed it (the profile it acts as, unless it switched since).
    pub approved_by: i64,
}

impl AuthUser {
    /// A user acting with all rights the account has, for server-side work;
    /// `None` for a disabled account.
    pub async fn load(db: &Db, id: i64) -> sqlx::Result<Option<AuthUser>> {
        let row: Option<(i64, String, bool, bool, Option<i64>)> = sqlx::query_as(
            "SELECT id, username, is_admin, all_libraries, max_age FROM users WHERE id = ? AND NOT disabled",
        )
        .bind(id)
        .fetch_optional(&db.read)
        .await?;
        Ok(row.map(|(id, username, is_admin, all_libraries, max_age)| AuthUser {
            id,
            username,
            is_admin,
            account_admin: is_admin,
            all_libraries,
            max_age: if is_admin { None } else { max_age },
            app: None,
            compatibility: None,
        }))
    }

    /// Whether this is pley's own sign-in (the web, the desktop app's
    /// window) or one of pley's own apps, not an app by another publisher.
    pub fn first_party(&self) -> bool {
        self.app.as_ref().is_none_or(|app| crate::oauth::clients::builtin(&app.client_id).is_some())
    }

    /// Whether this request manages requests (approves and declines them,
    /// sees and deletes everyone's): an administrator's own sign-in, or one
    /// of pley's own apps on an administrator's account allowed `approvals`
    /// (no other app can be).
    pub fn manages_requests(&self) -> bool {
        match &self.app {
            None => self.is_admin,
            Some(app) => self.account_admin && app.scopes.contains(Scope::Approvals) && self.first_party(),
        }
    }

    /// Whether this request may use what `scope` allows: pley's own sign-in
    /// always, an app when its token has it.
    pub fn allowed(&self, scope: Scope) -> bool {
        self.app.as_ref().is_none_or(|app| app.scopes.contains(scope))
    }

    /// Whether this request's compatibility token may use what `scope`
    /// allows: the token has it, and for a scope only administrators' tokens
    /// may have, the account is one now (a role taken away takes it at
    /// once). Never for a session or an app.
    pub fn compatibility_allows(&self, scope: compatibility::Scope) -> bool {
        self.compatibility
            .is_some_and(|token| token.scopes.contains(scope) && (!scope.administrators_only() || self.account_admin))
    }
}

/// Where a token was used from, for the account page's list.
#[derive(Debug, Clone, Default)]
pub struct UsedFrom {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

/// Extractor that additionally requires administrator rights.
#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

/// Password hashes made or checked at once. Each takes 19 MiB and a core
/// for a moment (Argon2id), so a flood of sign-ins queues instead of
/// exhausting memory.
const HASHING_AT_ONCE: usize = 4;
/// How long a password waits its turn before pley says it is busy.
const HASHING_WAIT: Duration = Duration::from_secs(10);

/// Runs Argon2 `work` when its turn comes.
async fn hashing<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, AppError> {
    static TURNS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(HASHING_AT_ONCE);
    let _turn = tokio::time::timeout(HASHING_WAIT, TURNS.acquire())
        .await
        .map_err(|_| AppError::Unavailable("Pley is busy checking passwords: try again in a moment".into()))?
        .map_err(|e| AppError::Internal(e.into()))?;
    tokio::task::spawn_blocking(work).await.map_err(|e| AppError::Internal(e.into()))
}

pub async fn hash_password(password: String) -> Result<String, AppError> {
    hashing(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|h: PasswordHash| h.to_string())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("hashing password: {e}")))
    })
    .await?
}

/// A real Argon2 hash of a random password, made once: checking a login for
/// an unknown user against it costs the same as checking a real one.
pub async fn dummy_password_hash() -> String {
    static HASH: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    HASH.get_or_init(|| async {
        let mut secret = [0u8; 16];
        getrandom::fill(&mut secret).expect("the system's random numbers");
        hash_password(hex::encode(secret)).await.unwrap_or_default()
    })
    .await
    .clone()
}

/// Whether `password` matches `hash` (never for the markers of accounts
/// without a password); an error when pley is too busy to check.
pub async fn verify_password(password: String, hash: String) -> Result<bool, AppError> {
    hashing(move || {
        PasswordHash::new(&hash)
            .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
    })
    .await
}

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A session token from `draw`, never one that looks like an app's token
/// (drawn again, which practically never happens).
fn session_token(mut draw: impl FnMut() -> String) -> String {
    loop {
        let token = draw();
        if !token.starts_with(APP_TOKEN_PREFIX) {
            return token;
        }
    }
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// The session cookie. `secure` (the site is reached over HTTPS, e.g. through
/// a TLS proxy) keeps browsers from ever sending it over plain HTTP.
pub fn session_cookie(token: &str, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECS}{secure}")
}

/// Whether the proxy in front of pley received the request over HTTPS.
pub fn forwarded_https(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .is_some_and(|proto| proto.trim().eq_ignore_ascii_case("https"))
}

pub fn clear_cookie() -> String {
    format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

pub fn validate_username(name: &str) -> Result<(), AppError> {
    let ok = (1..=64).contains(&name.chars().count())
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '@'));
    if ok { Ok(()) } else { Err(AppError::bad_request("username must be 1-64 letters, digits or _ - . @")) }
}

/// Shortest password accepted (NIST SP 800-63B's minimum).
pub const MIN_PASSWORD_CHARS: usize = 8;
const MAX_PASSWORD_CHARS: usize = 256;

/// Whether `password` may become the password of `username`: long enough,
/// not the username, not one character over and over, and not one of the
/// passwords tried first by anyone guessing.
pub fn validate_password(password: &str, username: &str) -> Result<(), AppError> {
    let chars = password.chars().count();
    if chars < MIN_PASSWORD_CHARS {
        return Err(AppError::bad_request(format!("use at least {MIN_PASSWORD_CHARS} characters for the password")));
    }
    if chars > MAX_PASSWORD_CHARS {
        return Err(AppError::bad_request(format!("use at most {MAX_PASSWORD_CHARS} characters for the password")));
    }
    let lower = password.to_lowercase();
    if lower == username.trim().to_lowercase() {
        return Err(AppError::bad_request("the password can't be the username"));
    }
    let mut letters = lower.chars();
    let first = letters.next();
    if letters.all(|c| Some(c) == first) || COMMON_PASSWORDS.contains(&lower.as_str()) {
        return Err(AppError::bad_request(
            "that password is too easy to guess: try a few words that don't go together",
        ));
    }
    Ok(())
}

/// Very common passwords of 8 characters or more (shorter ones are refused
/// anyway), lowercase.
const COMMON_PASSWORDS: &[&str] = &[
    "password",
    "password1",
    "password12",
    "password123",
    "password1234",
    "password!",
    "passw0rd",
    "p@ssw0rd",
    "p@ssword",
    "mypassword",
    "12345678",
    "123456789",
    "1234567890",
    "12345678910",
    "87654321",
    "987654321",
    "0987654321",
    "12341234",
    "11223344",
    "12121212",
    "123123123",
    "123321123",
    "1234qwer",
    "qwer1234",
    "1q2w3e4r",
    "1q2w3e4r5t",
    "1q2w3e4r5t6y",
    "q1w2e3r4",
    "q1w2e3r4t5",
    "1qaz2wsx",
    "1qazxsw2",
    "zaq12wsx",
    "qazwsxedc",
    "123qweasd",
    "qwertyui",
    "qwertyuiop",
    "qwerty12",
    "qwerty123",
    "qwerty1234",
    "asdfghjk",
    "asdfghjkl",
    "asdf1234",
    "asdfasdf",
    "zxcvbnm1",
    "zxcvbnm123",
    "abcd1234",
    "1234abcd",
    "abc12345",
    "abcdefgh",
    "abcdefg1",
    "aa123456",
    "a1234567",
    "a12345678",
    "123456789a",
    "12345678a",
    "a123456789",
    "iloveyou",
    "iloveyou1",
    "iloveyou2",
    "loveyou1",
    "sunshine",
    "sunshine1",
    "princess",
    "princess1",
    "football",
    "football1",
    "baseball",
    "basketball",
    "superman",
    "batman123",
    "trustno1",
    "welcome1",
    "welcome123",
    "letmein1",
    "letmein123",
    "starwars",
    "whatever",
    "dragon123",
    "master123",
    "computer",
    "michelle",
    "jennifer",
    "charlie1",
    "jordan23",
    "shadow12",
    "monkey123",
    "mustang1",
    "liverpool",
    "chelsea1",
    "arsenal1",
    "manchester",
    "internet",
    "samsung1",
    "google123",
    "freedom1",
    "pokemon1",
    "minecraft",
    "fortnite",
    "butterfly",
    "chocolate",
    "babygirl1",
    "admin123",
    "admin1234",
    "administrator",
    "changeme",
    "changeme1",
    "default1",
    "secret123",
    "pley1234",
    "pleypley",
    "plexpass",
    "plex1234",
    "jellyfin",
    "mediaserver",
];

struct CachedSession {
    user: AuthUser,
    expires_at: i64,
    cached_at: Instant,
    touched_at: i64,
}

struct CachedApp {
    user: AuthUser,
    /// When the token, or its grant, stops working.
    expires_at: i64,
    cached_at: Instant,
}

struct CachedCompatibility {
    user: AuthUser,
    expires_at: i64,
    cached_at: Instant,
}

pub struct AuthService {
    db: Db,
    cache: DashMap<String, CachedSession>,
    /// App access tokens, like `cache` but never extended.
    apps: DashMap<String, CachedApp>,
    /// Compatibility tokens, like `cache`.
    compatibility: DashMap<String, CachedCompatibility>,
    /// Bumped by every forget before it forgets: a validation that read the
    /// database before a forget doesn't cache what it read.
    epoch: AtomicU64,
    /// Every account with a password must use two-step sign-in (see
    /// [`AuthService::require_two_step`]).
    two_step_required: AtomicBool,
    /// Failed password and PIN attempts.
    pub throttle: crate::accounts::Throttle,
    /// The code that sets up a server without accounts from elsewhere.
    pub setup: crate::setup::SetupCode,
    /// Where tests hold the service, to land a forget at the worst moment.
    #[cfg(test)]
    pauses: Pauses,
}

/// Where tests hold [`AuthService`] (each pause is taken once).
#[cfg(test)]
#[derive(Default)]
struct Pauses {
    /// Held after [`AuthService::validate_app`]'s,
    /// [`AuthService::validate_compatibility`]'s or
    /// [`AuthService::validate`]'s query, before it caches what it read.
    after_query: parking_lot::Mutex<Option<Arc<Gate>>>,
    /// Run by [`AuthService::remember_app`] between its first look at the
    /// epoch and the insert.
    before_insert: parking_lot::Mutex<Option<Hook>>,
}

/// Something a test does from inside the service.
#[cfg(test)]
type Hook = Box<dyn FnOnce(&AuthService) + Send>;

/// A pause: the service says it got there, and waits to be let go.
#[cfg(test)]
#[derive(Default)]
struct Gate {
    reached: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

impl AuthService {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            cache: DashMap::new(),
            apps: DashMap::new(),
            compatibility: DashMap::new(),
            epoch: AtomicU64::new(0),
            two_step_required: AtomicBool::new(false),
            throttle: Default::default(),
            setup: Default::default(),
            #[cfg(test)]
            pauses: Pauses::default(),
        }
    }

    /// Whether every account with a password must use two-step sign-in: the
    /// `require_two_step` setting. It holds while the server can't set
    /// authenticator apps up too: an account without it gets in nowhere
    /// then, rather than in with its password alone.
    pub fn two_step_required(&self) -> bool {
        self.two_step_required.load(Ordering::SeqCst)
    }

    /// Requires two-step sign-in of every account with a password, or not.
    /// While it is required, the sessions, app tokens and compatibility
    /// tokens of an account without it don't work, nor of an app allowed by
    /// one, and their refresh tokens get nothing: whatever was signed in
    /// with a password alone. Cached ones are read again.
    pub fn require_two_step(&self, on: bool) {
        self.two_step_required.store(on, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.cache.clear();
        self.apps.clear();
        self.compatibility.clear();
    }

    /// Whether `user_id` is kept out because the server requires two-step
    /// sign-in and they don't use it, for what doesn't come through
    /// [`AuthService::validate`]: the keyed addresses and the compatibility
    /// API's session addresses of a playback started before it was required.
    pub async fn lacks_two_step(&self, user_id: i64) -> sqlx::Result<bool> {
        if !self.two_step_required() {
            return Ok(false);
        }
        let row: Option<bool> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM users u WHERE u.id = ?",
            without_two_step("u")
        )))
        .bind(user_id)
        .fetch_optional(&self.db.read)
        .await?;
        Ok(row.unwrap_or(false))
    }

    pub async fn create_session(
        &self,
        user_id: i64,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Result<String, AppError> {
        self.create_session_drawing(new_token, user_id, user_agent, ip).await
    }

    /// [`AuthService::create_session`], with tokens from `draw`.
    async fn create_session_drawing(
        &self,
        draw: impl FnMut() -> String,
        user_id: i64,
        user_agent: Option<&str>,
        ip: Option<&str>,
    ) -> Result<String, AppError> {
        let token = session_token(draw);
        let now = now();
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, last_seen_at, expires_at, user_agent, last_ip)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(now)
        .bind(now)
        .bind(now + SESSION_TTL_SECS)
        .bind(user_agent.map(|ua| ua.chars().take(200).collect::<String>()))
        .bind(ip)
        .execute(&self.db.write)
        .await?;
        Ok(token)
    }

    /// The user of a session token. Never of an app's token: those go
    /// through [`AuthService::validate_app`].
    pub async fn validate(&self, token: &str) -> Option<AuthUser> {
        if token.starts_with(APP_TOKEN_PREFIX) {
            return None;
        }
        let key = hash_token(token);
        let now = now();
        if let Some(entry) = self.cache.get(&key)
            && entry.cached_at.elapsed() < CACHE_TTL
            && entry.expires_at > now
        {
            return Some(entry.user.clone());
        }
        // Read before the database: a sign-out or a forget that lands while
        // the query runs keeps what it read out of the cache.
        let epoch = self.epoch.load(Ordering::SeqCst);
        #[allow(clippy::type_complexity)]
        let row: Option<(i64, String, bool, bool, Option<i64>, bool, bool, i64, i64)> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "SELECT u.id, u.username, u.is_admin, u.all_libraries, u.max_age, u.disabled, {}, s.expires_at,
                        s.last_seen_at
                 FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_hash = ?",
                without_two_step("u")
            )))
            .bind(&key)
            .fetch_optional(&self.db.read)
            .await
            .ok()?;
        #[cfg(test)]
        {
            let gate = self.pauses.after_query.lock().take();
            if let Some(gate) = gate {
                gate.reached.notify_one();
                gate.release.notified().await;
            }
        }
        let Some((id, username, is_admin, all_libraries, max_age, disabled, no_two_step, expires_at, last_seen_at)) =
            row
        else {
            self.cache.remove(&key);
            return None;
        };
        if expires_at <= now || disabled || (no_two_step && self.two_step_required()) {
            self.cache.remove(&key);
            return None;
        }
        let user = AuthUser {
            id,
            username,
            is_admin,
            account_admin: is_admin,
            all_libraries,
            max_age: if is_admin { None } else { max_age },
            app: None,
            compatibility: None,
        };
        let mut touched_at = last_seen_at;
        if now - last_seen_at > TOUCH_INTERVAL_SECS {
            // Sliding expiration, written at most every few minutes.
            let _ = sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?")
                .bind(now)
                .bind(now + SESSION_TTL_SECS)
                .bind(&key)
                .execute(&self.db.write)
                .await;
            touched_at = now;
        }
        self.remember_session(
            key,
            CachedSession { user: user.clone(), expires_at, cached_at: Instant::now(), touched_at },
            epoch,
        );
        Some(user)
    }

    /// Caches a session read while the epoch was `epoch`, unless something
    /// was forgotten or signed out since: what was read may be gone (as
    /// [`AuthService::remember_app`]).
    fn remember_session(&self, key: String, entry: CachedSession, epoch: u64) {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        self.cache.insert(key.clone(), entry);
        if self.epoch.load(Ordering::SeqCst) != epoch {
            self.cache.remove(&key);
        }
    }

    /// Signs a session out. The row goes first, then the epoch moves and
    /// the cache forgets it: a validation that read the row before it went
    /// doesn't cache it, whenever it looked at the epoch.
    pub async fn revoke(&self, token: &str) {
        let key = hash_token(token);
        let _ = sqlx::query("DELETE FROM sessions WHERE token_hash = ?").bind(&key).execute(&self.db.write).await;
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.cache.remove(&key);
    }

    /// Drop cached sessions of a user (password change, deletion, role
    /// change), the cached tokens of apps acting as them or allowed by
    /// them, and their cached compatibility tokens.
    pub fn forget_user(&self, user_id: i64) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.cache.retain(|_, s| s.user.id != user_id);
        self.apps
            .retain(|_, a| a.user.id != user_id && a.user.app.as_ref().is_none_or(|app| app.approved_by != user_id));
        self.compatibility.retain(|_, c| c.user.id != user_id);
    }

    /// Drop the cached tokens of these grants (ended, or changed).
    pub fn forget_grants(&self, grant_ids: &[i64]) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.apps.retain(|_, a| a.user.app.as_ref().is_none_or(|app| !grant_ids.contains(&app.grant_id)));
    }

    /// Drop every cached app token.
    pub fn forget_apps(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.apps.clear();
    }

    pub async fn purge_expired(&self) {
        let _ = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?").bind(now()).execute(&self.db.write).await;
        if let Err(err) = grants::purge(&self.db).await {
            tracing::warn!("could not forget apps' expired tokens: {err}");
        }
        if let Err(err) = compatibility::purge(&self.db).await {
            tracing::warn!("could not forget expired tokens for other apps: {err}");
        }
        let now = now();
        self.cache.retain(|_, s| s.expires_at > now && s.touched_at > 0);
        self.apps.retain(|_, a| a.expires_at > now);
        self.compatibility.retain(|_, c| c.expires_at > now);
        self.throttle.purge();
    }

    /// The user an app's access token acts as, with the app's rights: never
    /// an administrator's; every library and no age limit for an
    /// administrator's role, the profile's limits otherwise. Not for an
    /// expired token, an ended grant, or a disabled profile or approver.
    /// Never extended: apps refresh their tokens.
    pub async fn validate_app(&self, token: &str) -> Option<AuthUser> {
        if !token.starts_with(ACCESS_PREFIX) {
            return None;
        }
        let key = hash_token(token);
        let now = now();
        if let Some(entry) = self.apps.get(&key)
            && entry.cached_at.elapsed() < CACHE_TTL
            && entry.expires_at > now
        {
            return Some(entry.user.clone());
        }
        // Read before the database: a forget that lands while the query
        // runs keeps what it read out of the cache.
        let epoch = self.epoch.load(Ordering::SeqCst);
        let row = self.app_row(&key).await.ok()?;
        let Some(row) = row else {
            self.apps.remove(&key);
            return None;
        };
        let expires_at = row.token_expires.min(row.grant_expires).min(row.ends_at.unwrap_or(i64::MAX));
        let no_two_step = row.user_without_two_step || row.approver_without_two_step;
        if expires_at <= now || row.disabled || row.approver_disabled || (no_two_step && self.two_step_required()) {
            self.apps.remove(&key);
            return None;
        }
        let user = AuthUser {
            id: row.id,
            username: row.username,
            is_admin: false,
            account_admin: row.role_admin,
            all_libraries: row.all_libraries || row.role_admin,
            max_age: if row.role_admin { None } else { row.max_age },
            app: Some(Arc::new(AppAccess {
                grant_id: row.grant_id,
                client_id: row.client_id.into(),
                scopes: Scopes::from_stored(&row.scope),
                approved_by: row.approved_by,
            })),
            compatibility: None,
        };
        self.remember_app(key, CachedApp { user: user.clone(), expires_at, cached_at: Instant::now() }, epoch);
        Some(user)
    }

    /// What the database says about the access token whose SHA-256 is `key`.
    async fn app_row(&self, key: &str) -> sqlx::Result<Option<AppRow>> {
        let row = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT u.id, u.username, u.is_admin AS role_admin, u.all_libraries, u.max_age, u.disabled,
                    a.disabled AS approver_disabled, {} AS user_without_two_step,
                    {} AS approver_without_two_step, t.expires_at AS token_expires, g.id AS grant_id, g.client_id,
                    g.scope, g.approved_by, g.expires_at AS grant_expires, g.ends_at
             FROM app_tokens t
             JOIN app_grants g ON g.id = t.grant_id
             JOIN users u ON u.id = g.user_id
             JOIN users a ON a.id = g.approved_by
             WHERE t.token_hash = ? AND t.kind = 'access'",
            without_two_step("u"),
            without_two_step("a")
        )))
        .bind(key)
        .fetch_optional(&self.db.read)
        .await;
        #[cfg(test)]
        {
            let gate = self.pauses.after_query.lock().take();
            if let Some(gate) = gate {
                gate.reached.notify_one();
                gate.release.notified().await;
            }
        }
        row
    }

    /// Caches an app token read while the epoch was `epoch`, unless
    /// something was forgotten since: what was read may be gone.
    fn remember_app(&self, key: String, entry: CachedApp, epoch: u64) {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        #[cfg(test)]
        {
            let pause = self.pauses.before_insert.lock().take();
            if let Some(pause) = pause {
                pause(self);
            }
        }
        self.apps.insert(key.clone(), entry);
        // A forget between the check and the insert has bumped the epoch
        // already, and its sweep may have missed this entry.
        if self.epoch.load(Ordering::SeqCst) != epoch {
            self.apps.remove(&key);
        }
    }

    /// The user of a compatibility token, with what its scopes allow:
    /// administrator rights for viewing (everyone's sessions and history,
    /// file paths) only with `activity`, on an administrator's account;
    /// changes go by [`AuthUser::compatibility_allows`]. Every library and no
    /// age limit for an administrator's role, the account's limits
    /// otherwise. Not for an expired token, a disabled account, or one with
    /// a password alone while the server requires two-step sign-in. A token
    /// in use lasts 90 days from then, written at most every few minutes
    /// with where it was `used` from, when that is known.
    pub async fn validate_compatibility(&self, token: &str, used: Option<&UsedFrom>) -> Option<AuthUser> {
        if !token.starts_with(compatibility::PREFIX) {
            return None;
        }
        let key = hash_token(token);
        let now = now();
        if let Some(entry) = self.compatibility.get(&key)
            && entry.cached_at.elapsed() < CACHE_TTL
            && entry.expires_at > now
        {
            return Some(entry.user.clone());
        }
        // Read before the database: a revocation or a forget that lands
        // while the query runs keeps what it read out of the cache.
        let epoch = self.epoch.load(Ordering::SeqCst);
        let row = self.compatibility_row(&key).await.ok()?;
        let Some(row) = row else {
            self.compatibility.remove(&key);
            return None;
        };
        if row.expires_at <= now || row.disabled || (row.without_two_step && self.two_step_required()) {
            self.compatibility.remove(&key);
            return None;
        }
        let scopes = compatibility::Scopes::from_stored(&row.scope);
        let user = AuthUser {
            id: row.user_id,
            username: row.username,
            is_admin: row.role_admin && scopes.contains(compatibility::Scope::Activity),
            account_admin: row.role_admin,
            all_libraries: row.all_libraries || row.role_admin,
            max_age: if row.role_admin { None } else { row.max_age },
            app: None,
            compatibility: Some(CompatibilityAccess { token_id: row.id, scopes }),
        };
        if row.last_used_at.is_none_or(|at| now - at > TOUCH_INTERVAL_SECS) {
            // Sliding expiration, written at most every few minutes.
            let used = used.cloned().unwrap_or_default();
            let _ = sqlx::query(
                "UPDATE compatibility_tokens SET last_used_at = ?, expires_at = ?, last_ip = COALESCE(?, last_ip),
                        user_agent = COALESCE(?, user_agent)
                 WHERE id = ?",
            )
            .bind(now)
            .bind(now + compatibility::IDLE_SECS)
            .bind(used.ip)
            .bind(used.user_agent.map(|ua| ua.chars().take(200).collect::<String>()))
            .bind(row.id)
            .execute(&self.db.write)
            .await;
        }
        let entry = CachedCompatibility { user: user.clone(), expires_at: row.expires_at, cached_at: Instant::now() };
        self.remember_compatibility(key, entry, epoch);
        Some(user)
    }

    /// What the database says about the compatibility token whose SHA-256
    /// is `key`.
    async fn compatibility_row(&self, key: &str) -> sqlx::Result<Option<CompatibilityRow>> {
        let row = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT t.id, t.scope, t.expires_at, t.last_used_at, u.id AS user_id, u.username, u.is_admin AS role_admin,
                    u.all_libraries, u.max_age, u.disabled, {} AS without_two_step
             FROM compatibility_tokens t JOIN users u ON u.id = t.user_id
             WHERE t.token_hash = ?",
            without_two_step("u")
        )))
        .bind(key)
        .fetch_optional(&self.db.read)
        .await;
        #[cfg(test)]
        {
            let gate = self.pauses.after_query.lock().take();
            if let Some(gate) = gate {
                gate.reached.notify_one();
                gate.release.notified().await;
            }
        }
        row
    }

    /// Caches a compatibility token read while the epoch was `epoch`, unless
    /// something was forgotten or revoked since (as
    /// [`AuthService::remember_app`]).
    fn remember_compatibility(&self, key: String, entry: CachedCompatibility, epoch: u64) {
        if self.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        self.compatibility.insert(key.clone(), entry);
        if self.epoch.load(Ordering::SeqCst) != epoch {
            self.compatibility.remove(&key);
        }
    }

    /// Makes a compatibility token for `user_id` (see [`compatibility::create`]):
    /// the token, shown once, and its row.
    pub async fn create_compatibility_token(
        &self,
        user_id: i64,
        asked: &compatibility::Asked,
    ) -> Result<(String, compatibility::TokenRow), AppError> {
        let (token, row, dropped) = compatibility::create(&self.db, user_id, asked).await?;
        if !dropped.is_empty() {
            self.forget_compatibility_tokens(&dropped);
        }
        Ok((token, row))
    }

    /// Revokes compatibility token `id` of `user_id`; whether there was one.
    /// The row goes first, then the cache forgets it, as [`AuthService::revoke`].
    pub async fn end_compatibility_token(&self, user_id: i64, id: i64) -> Result<bool, AppError> {
        let ended = compatibility::delete(&self.db, user_id, id).await?;
        self.forget_compatibility_tokens(&[id]);
        Ok(ended)
    }

    /// Revokes the compatibility token `token`; whether there was one.
    pub async fn end_compatibility_token_of(&self, token: &str) -> Result<bool, AppError> {
        let ended = compatibility::delete_token(&self.db, token).await?;
        if let Some(id) = ended {
            self.forget_compatibility_tokens(&[id]);
        }
        Ok(ended.is_some())
    }

    /// Revokes every compatibility token of `user_id`.
    pub async fn end_compatibility_tokens(&self, user_id: i64) -> Result<(), AppError> {
        let ended = compatibility::delete_for(&self.db, user_id).await?;
        self.forget_compatibility_tokens(&ended);
        Ok(())
    }

    /// Drop the cached compatibility tokens with these ids (ended).
    fn forget_compatibility_tokens(&self, ids: &[i64]) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.compatibility.retain(|_, c| c.user.compatibility.is_none_or(|token| !ids.contains(&token.token_id)));
    }

    /// The user of a session token or of an app's access token.
    pub async fn validate_any(&self, token: &str) -> Option<AuthUser> {
        if token.starts_with(ACCESS_PREFIX) { self.validate_app(token).await } else { self.validate(token).await }
    }

    /// Records what a person allowed an app; its first tokens (see
    /// [`grants::issue`]).
    pub async fn issue(&self, grant: NewGrant) -> Result<Issued, IssueError> {
        let (issued, dropped) = grants::issue(&self.db, grant).await?;
        if !dropped.is_empty() {
            self.forget_grants(&dropped);
        }
        Ok(issued)
    }

    /// Exchanges an app's refresh token (see [`grants::refresh`]).
    pub async fn refresh(&self, input: RefreshInput) -> Result<Issued, RefreshError> {
        let client = input.client_id.clone();
        let result = grants::refresh(&self.db, input, self.two_step_required()).await;
        match &result {
            // Its oldest access tokens may be gone.
            Ok(issued) => self.forget_grants(&[issued.grant_id]),
            Err(RefreshError::Reused { grant_id }) => {
                tracing::warn!("{client} showed a spent refresh token (someone has a copy): its access is ended");
                self.forget_grants(&[*grant_id]);
            }
            Err(RefreshError::Ended { grant_id }) => {
                tracing::warn!("{client} refreshed its tokens too often: its access is ended");
                self.forget_grants(&[*grant_id]);
            }
            Err(_) => {}
        }
        result
    }

    /// Ends the grant of an app token (access or refresh, spent or not).
    /// Whether one ended.
    pub async fn revoke_app_token(&self, token: &str) -> bool {
        token.starts_with(APP_TOKEN_PREFIX) && self.end_grant_of_token(&hash_token(token)).await.unwrap_or(false)
    }

    /// Ends one grant; whether there was one.
    pub async fn end_grant(&self, grant_id: i64) -> Result<bool, AppError> {
        let ended = grants::delete(&self.db, grant_id).await?;
        self.forget_grants(&[grant_id]);
        Ok(ended)
    }

    /// Ends the grant of the token whose SHA-256 is `token_hash`, if it is
    /// still there (see [`grants::delete_of_token`]); whether there was one.
    pub async fn end_grant_of_token(&self, token_hash: &str) -> Result<bool, AppError> {
        let ended = grants::delete_of_token(&self.db, token_hash).await?;
        if let Some(grant_id) = ended {
            self.forget_grants(&[grant_id]);
        }
        Ok(ended.is_some())
    }

    /// Ends the apps acting as `user_id` or allowed by them; their grants.
    pub async fn end_apps(&self, user_id: i64) -> Result<Vec<i64>, AppError> {
        let ended = grants::delete_for(&self.db, user_id).await?;
        self.forget_grants(&ended);
        Ok(ended)
    }

    /// Ends the grants of every app but `clients`; their ids.
    pub async fn end_grants_except_clients(&self, clients: &[&str]) -> Result<Vec<i64>, AppError> {
        let ended = grants::delete_except_clients(&self.db, clients).await?;
        self.forget_grants(&ended);
        Ok(ended)
    }

    /// Signs `user_id` out everywhere: their sessions but the one whose hash
    /// is `keep_session_hash`, then every app acting as them or allowed by
    /// them, and every compatibility token of theirs.
    pub async fn sign_out_everywhere(&self, user_id: i64, keep_session_hash: Option<&str>) -> Result<(), AppError> {
        sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash != ?")
            .bind(user_id)
            .bind(keep_session_hash.unwrap_or_default())
            .execute(&self.db.write)
            .await?;
        self.end_apps(user_id).await?;
        self.end_compatibility_tokens(user_id).await?;
        self.forget_user(user_id);
        Ok(())
    }
}

/// SQL that is true when the account of the `users` row `alias` signs in
/// with a password and doesn't use two-step sign-in: what a server that
/// requires it doesn't let in ([`AuthService::require_two_step`]).
pub(crate) fn without_two_step(alias: &str) -> String {
    format!(
        "({alias}.password_hash NOT LIKE '!%' AND NOT EXISTS (SELECT 1 FROM two_step w WHERE w.user_id = {alias}.id))"
    )
}

/// What [`AuthService::validate_app`] reads about a token.
#[derive(sqlx::FromRow)]
struct AppRow {
    id: i64,
    username: String,
    role_admin: bool,
    all_libraries: bool,
    max_age: Option<i64>,
    disabled: bool,
    approver_disabled: bool,
    /// The profile it acts as, or whoever allowed it, signs in with a
    /// password alone ([`without_two_step`]).
    user_without_two_step: bool,
    approver_without_two_step: bool,
    token_expires: i64,
    grant_id: i64,
    client_id: String,
    scope: String,
    approved_by: i64,
    grant_expires: i64,
    ends_at: Option<i64>,
}

/// What [`AuthService::validate_compatibility`] reads about a token.
#[derive(sqlx::FromRow)]
struct CompatibilityRow {
    id: i64,
    scope: String,
    expires_at: i64,
    last_used_at: Option<i64>,
    user_id: i64,
    username: String,
    role_admin: bool,
    all_libraries: bool,
    max_age: Option<i64>,
    disabled: bool,
    /// The account signs in with a password alone ([`without_two_step`]).
    without_two_step: bool,
}

/// Token from `Authorization: Bearer`, `X-Pley-Token`, the session cookie, or `?token=`.
pub fn token_from_parts(parts: &Parts) -> Option<String> {
    header_token(parts).or_else(|| cookie_token(parts)).or_else(|| query_token(parts))
}

/// A token the client sent on purpose (header or `?token=`), never the
/// browser's cookie: what listeners without CSRF protection accept.
pub fn token_without_cookie(parts: &Parts) -> Option<String> {
    header_token(parts).or_else(|| query_token(parts))
}

/// The token of `Authorization: Bearer`, the scheme in any letter case.
fn bearer_token(parts: &Parts) -> Option<String> {
    let value = parts.headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim().to_owned())
}

fn header_token(parts: &Parts) -> Option<String> {
    bearer_token(parts)
        .or_else(|| parts.headers.get("x-pley-token").and_then(|v| v.to_str().ok()).map(|v| v.trim().to_owned()))
}

pub(crate) fn cookie_token(parts: &Parts) -> Option<String> {
    for cookie_header in parts.headers.get_all(header::COOKIE) {
        let Ok(cookies) = cookie_header.to_str() else { continue };
        for pair in cookies.split(';') {
            if let Some((name, value)) = pair.trim().split_once('=')
                && name == COOKIE_NAME
                && !value.is_empty()
            {
                return Some(value.to_owned());
            }
        }
    }
    None
}

fn query_token(parts: &Parts) -> Option<String> {
    parts.uri.query().and_then(|q| {
        q.split('&').find_map(|kv| kv.strip_prefix("token=").map(|t| t.to_owned())).filter(|t| !t.is_empty())
    })
}

/// The session token of the request, if any (to mark "this device").
pub struct CurrentToken(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for CurrentToken {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(CurrentToken(token_from_parts(parts)))
    }
}

/// The client's address, for the devices and apps lists: [`SourceIp`]'s, so
/// that someone signing in with a stolen password can't show its owner's
/// own address there.
pub struct ClientIp(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let SourceIp(source) = SourceIp::from_request_parts(parts, state).await?;
        Ok(ClientIp(source.map(|ip| ip.to_string())))
    }
}

/// Where a request came from, for limits on what anyone may start (TV
/// sign-ins): the other end of its connection, or the client a proxy on
/// this computer or its network forwards for ([`forwarded_for`]). Unlike
/// [`ClientIp`], nobody on the internet can choose it. Unknown only without
/// a connection (tests).
pub struct SourceIp(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for SourceIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let peer = parts.extensions.get::<axum::extract::ConnectInfo<SocketAddr>>().map(|info| info.0.ip());
        Ok(SourceIp(peer.map(|peer| forwarded_for(peer, &parts.headers))))
    }
}

/// How many proxies in a row a request is followed back through.
const MAX_PROXIES: usize = 10;

/// The client that sent a request pley got from `peer`: `peer` itself,
/// unless it has a loopback or private address
/// ([`crate::setup::is_local_address`]). Then `peer` is a proxy, such as
/// nginx or Caddy on this computer, and the client is the address it added
/// at the end of X-Forwarded-For; when that one is local too, it is another
/// proxy's, and the address before it counts, and so on. What a client
/// writes in the header itself comes before its own address, so an address
/// on the internet ends the search before reaching it. Someone on the local
/// network can name any address here, as they could connect from another
/// one. Anything but an address (from a proxy on a Unix socket, say) leaves
/// the proxy's own.
pub fn forwarded_for(peer: IpAddr, headers: &axum::http::HeaderMap) -> IpAddr {
    let mut client = peer.to_canonical();
    let hops = headers
        .get_all("x-forwarded-for")
        .iter()
        .rev()
        .flat_map(|value| value.to_str().unwrap_or_default().rsplit(','))
        .take(MAX_PROXIES);
    for hop in hops {
        if !crate::setup::is_local_address(client) {
            break;
        }
        let hop = hop.trim();
        // Some proxies add the client's port.
        let Some(address) = hop.parse::<IpAddr>().ok().or_else(|| hop.parse::<SocketAddr>().ok().map(|a| a.ip()))
        else {
            break;
        };
        client = address.to_canonical();
    }
    client
}

/// The user a request was authenticated as, kept for its other extractors.
/// Private, so nothing else can put a user there.
#[derive(Clone)]
struct Authenticated(AuthUser);

/// Every pley API handler that needs a user goes through here: sessions as
/// always, and app tokens only from `Authorization: Bearer`, and only on the
/// routes their scopes open ([`scopes::APP_ROUTES`]: any other route is
/// closed to apps). Compatibility tokens never: they are for the
/// compatibility API.
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = match parts.extensions.get::<Authenticated>() {
            Some(Authenticated(user)) => user.clone(),
            None => {
                let user = authenticate(parts, state).await?;
                parts.extensions.insert(Authenticated(user.clone()));
                user
            }
        };
        if let Some(app) = &user.app {
            app_may_use(parts, app)?;
        }
        Ok(user)
    }
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<AuthUser, AppError> {
    if let Some(token) = bearer_token(parts) {
        if token.starts_with(ACCESS_PREFIX) {
            return state.auth.validate_app(&token).await.ok_or(AppError::InvalidToken);
        }
        if token.starts_with(compatibility::PREFIX) {
            return Err(AppError::CompatibilityToken);
        }
        if token.starts_with(APP_TOKEN_PREFIX) {
            // A refresh token is no bearer token.
            return Err(AppError::InvalidToken);
        }
    }
    let token = token_from_parts(parts).ok_or(AppError::Unauthorized)?;
    if token.starts_with(compatibility::PREFIX) {
        // Other apps' tokens open the compatibility API only, wherever
        // they are sent here: refused, with no other token tried.
        return Err(AppError::CompatibilityToken);
    }
    if token.starts_with(APP_TOKEN_PREFIX) {
        // In X-Pley-Token, a cookie or the URL (where logs and Referer
        // headers keep it): refused, with no other token tried.
        return Err(AppError::TokenPlacement);
    }
    // The session's cookie goes along with requests other sites' pages
    // make too (a page on another port of this host is the same site to
    // the browser), and SameSite=Lax lets their POSTs through when there is
    // no body to ask about first, and their links: what changes anything
    // with it comes from pley's own page, and no other site's page has it
    // fetch anything (an administrator's folder browser opening a share of
    // the attacker's, say). Typed addresses and bookmarks are no site's. The
    // cookie is what signs the request in, `?token=` or not; a header is no
    // cookie.
    let by_cookie = header_token(parts).is_none() && cookie_token(parts).is_some();
    if by_cookie {
        let elsewhere = match parts.method {
            Method::GET | Method::HEAD | Method::OPTIONS => from_another_sites_page(parts),
            _ => from_another_site(parts),
        };
        if elsewhere {
            return Err(AppError::forbidden("this request didn't come from Pley's page"));
        }
    }
    state.auth.validate(&token).await.ok_or(AppError::Unauthorized)
}

/// Whether the browser says another site's page asked for this (a link, an
/// image, a fetch), not the user's own address bar or bookmarks.
fn from_another_sites_page(parts: &Parts) -> bool {
    parts.headers.get("sec-fetch-site").is_some_and(|site| matches!(site.as_bytes(), b"cross-site" | b"same-site"))
}

/// Whether a browser says another site's page made this request: by
/// `Sec-Fetch-Site`, or, without it (older browsers), by an `Origin` other
/// than the server it went to. A request saying neither isn't a page's.
fn from_another_site(parts: &Parts) -> bool {
    if let Some(site) = parts.headers.get("sec-fetch-site") {
        return site.as_bytes() != b"same-origin";
    }
    parts.headers.contains_key(header::ORIGIN) && !origin_is_host(parts)
}

/// Whether the request's `Origin` is the server it was sent to (its
/// `Host`): scheme-less hosts and ports compared, nothing else in `Host`.
pub(crate) fn origin_is_host(parts: &Parts) -> bool {
    let Some(origin) = parts.headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else { return false };
    let Ok(origin) = url::Url::parse(origin) else { return false };
    if !matches!(origin.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = parts.headers.get(header::HOST).and_then(|v| v.to_str().ok()) else { return false };
    if host.contains(['/', '\\', '?', '#', '@']) {
        return false;
    }
    let Ok(host) = url::Url::parse(&format!("{}://{}", origin.scheme(), host.trim())) else { return false };
    origin.host().is_some()
        && origin.host() == host.host()
        && origin.port_or_known_default() == host.port_or_known_default()
}

/// Whether the app's token opens the route of this request.
fn app_may_use(parts: &Parts, app: &AppAccess) -> Result<(), AppError> {
    let path = parts.extensions.get::<MatchedPath>().map(MatchedPath::as_str);
    match path.and_then(|path| scopes::required(&parts.method, path)) {
        None => Err(AppError::InsufficientScope(None)),
        Some(Need::AnyApp) => Ok(()),
        Some(Need::Scope(scope)) if app.scopes.contains(scope) => Ok(()),
        Some(Need::Scope(scope)) => Err(AppError::InsufficientScope(Some(scope.name()))),
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(AppError::forbidden("administrator rights required"));
        }
        Ok(AdminUser(user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_round_trip() {
        let hash = hash_password("hunter22".into()).await.unwrap();
        assert!(verify_password("hunter22".into(), hash.clone()).await.unwrap());
        assert!(!verify_password("wrong".into(), hash).await.unwrap());
        assert!(!verify_password(String::new(), "!managed".into()).await.unwrap());
    }

    #[tokio::test]
    async fn passwords_saved_by_earlier_versions_still_sign_in() {
        // Made by argon2 0.5, which pley used until October 2026.
        let hash = "$argon2id$v=19$m=19456,t=2,p=1$WWW5Ql9KdGRryRtZQHNBlQ$q02uDlN7nHCcWcmEqUPnSMB/ooHW5gEGESZbsu2y33w";
        assert!(verify_password("correct horse".into(), hash.into()).await.unwrap());
        assert!(!verify_password("correct horses".into(), hash.into()).await.unwrap());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_flood_of_passwords_queues() {
        // Many at once (an attacker trying usernames): at most a few hashes
        // in memory at any moment, and every check still answers.
        let hash = hash_password("hunter22".into()).await.unwrap();
        let checks = (0..24).map(|_| verify_password("hunter22".into(), hash.clone()));
        let results = futures::future::join_all(checks).await;
        assert!(results.into_iter().all(|r| r.unwrap()));
    }

    #[test]
    fn tokens_are_random_and_hashable() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(hash_token(&a).len(), 64);
    }

    #[test]
    fn extracts_tokens() {
        let req = axum::http::Request::builder()
            .uri("/api/items?x=1&token=abc")
            .header("cookie", "foo=bar; pley_token=cookie-token")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        assert_eq!(token_from_parts(&parts).as_deref(), Some("cookie-token"));

        let req = axum::http::Request::builder().uri("/api/items?token=abc").body(()).unwrap();
        let (parts, _) = req.into_parts();
        assert_eq!(token_from_parts(&parts).as_deref(), Some("abc"));

        let req = axum::http::Request::builder().uri("/").header("authorization", "Bearer xyz").body(()).unwrap();
        let (parts, _) = req.into_parts();
        assert_eq!(token_from_parts(&parts).as_deref(), Some("xyz"));
    }

    #[test]
    fn username_rules() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("bob.smith@home").is_ok());
        assert!(validate_username("").is_err());
        assert!(validate_username("bad name").is_err());
        assert!(validate_username("Zoë").is_ok(), "letters of any script");
        assert!(validate_username(&"a".repeat(64)).is_ok());
        assert!(validate_username(&"a".repeat(65)).is_err());
        assert!(validate_username("a/b").is_err());
        assert!(validate_username("<script>").is_err());
    }

    #[test]
    fn passwords_are_long_and_hard_to_guess() {
        assert!(validate_password("correct horse", "bob").is_ok());
        assert!(validate_password("seven77", "bob").is_err(), "too short");
        assert!(validate_password("日本語の言葉です", "bob").is_ok(), "characters, not bytes");
        assert!(validate_password("日本語の言葉", "bob").is_err());
        assert!(validate_password(&"x".repeat(257), "bob").is_err());
        assert!(validate_password("Password1", "bob").is_err(), "a common one, in any case");
        assert!(validate_password("qwertyuiop", "bob").is_err());
        assert!(validate_password("zzzzzzzzzz", "bob").is_err(), "one character over and over");
        assert!(validate_password("Bob.Smith", "bob.smith").is_err(), "the username");
        for common in COMMON_PASSWORDS {
            assert!(common.chars().count() >= MIN_PASSWORD_CHARS, "{common} is refused for its length anyway");
            assert_eq!(*common, common.to_lowercase());
        }
    }

    fn token_of(builder: axum::http::request::Builder) -> Option<String> {
        let (parts, _) = builder.body(()).unwrap().into_parts();
        token_from_parts(&parts)
    }

    #[test]
    fn token_sources_in_order_of_precedence() {
        let all = axum::http::Request::builder()
            .uri("/api/x?token=query")
            .header("cookie", "pley_token=cookie")
            .header("x-pley-token", "header")
            .header("authorization", "Bearer bearer");
        assert_eq!(token_of(all).as_deref(), Some("bearer"));
        let no_bearer = axum::http::Request::builder()
            .uri("/api/x?token=query")
            .header("cookie", "pley_token=cookie")
            .header("x-pley-token", " header ")
            .header("authorization", "Basic dXNlcjpwYXNz");
        assert_eq!(token_of(no_bearer).as_deref(), Some("header"), "other schemes are not tokens");
        let cookies = axum::http::Request::builder()
            .uri("/api/x?token=query")
            .header("cookie", "theme=dark")
            .header("cookie", "a=b;   pley_token=cookie; c=d");
        assert_eq!(token_of(cookies).as_deref(), Some("cookie"), "any cookie header, any position");
        let empty_cookie =
            axum::http::Request::builder().uri("/api/x?a=1&token=query").header("cookie", "pley_token=; x=y");
        assert_eq!(token_of(empty_cookie).as_deref(), Some("query"), "a cleared cookie is no token");
        assert_eq!(token_of(axum::http::Request::builder().uri("/api/x?token=")), None);
        assert_eq!(token_of(axum::http::Request::builder().uri("/api/x?mytoken=abc")), None);
        assert_eq!(token_of(axum::http::Request::builder().uri("/api/x")), None);
    }

    #[test]
    fn cookies_are_http_only_and_last_as_long_as_sessions() {
        let cookie = session_cookie("tok", false);
        assert!(cookie.starts_with("pley_token=tok;"));
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax") && cookie.contains("Path=/"));
        assert!(cookie.contains(&format!("Max-Age={SESSION_TTL_SECS}")));
        assert!(!cookie.contains("Secure") && session_cookie("tok", true).ends_with("; Secure"));
        let mut headers = axum::http::HeaderMap::new();
        assert!(!forwarded_https(&headers));
        headers.insert("x-forwarded-proto", "https, http".parse().unwrap());
        assert!(forwarded_https(&headers));
        assert!(clear_cookie().contains("pley_token=;") && clear_cookie().contains("Max-Age=0"));
    }

    #[test]
    fn a_request_comes_from_its_connection_or_the_client_a_local_proxy_names() {
        let from = |peer: &str, forwarded: &[&[u8]]| {
            let mut headers = axum::http::HeaderMap::new();
            for value in forwarded {
                headers.append("x-forwarded-for", axum::http::HeaderValue::from_bytes(value).unwrap());
            }
            forwarded_for(peer.parse().unwrap(), &headers).to_string()
        };
        // From the internet: the connection's address, whatever the header says.
        assert_eq!(from("203.0.113.9", &[]), "203.0.113.9");
        assert_eq!(from("203.0.113.9", &[b"10.0.0.1"]), "203.0.113.9");
        assert_eq!(from("203.0.113.9", &[b"198.51.100.4, 127.0.0.1"]), "203.0.113.9");
        assert_eq!(from("2001:db8::1", &[b"198.51.100.4"]), "2001:db8::1");
        // From a proxy on this computer or its network: the address it added
        // last, never what the client sent before it.
        assert_eq!(from("127.0.0.1", &[b"203.0.113.9"]), "203.0.113.9");
        assert_eq!(from("::1", &[b"2001:db8::7"]), "2001:db8::7");
        assert_eq!(from("172.18.0.2", &[b"10.9.9.9, 198.51.100.4, 203.0.113.9"]), "203.0.113.9");
        assert_eq!(from("127.0.0.1", &[b"198.51.100.4", b"203.0.113.9"]), "203.0.113.9", "the last header too");
        // Proxies in a row on the network are followed back to the client.
        assert_eq!(from("127.0.0.1", &[b"198.51.100.4, 203.0.113.9, 192.168.1.2"]), "203.0.113.9");
        assert_eq!(from("127.0.0.1", &[b"198.51.100.4, 203.0.113.9", b"172.18.0.3"]), "203.0.113.9");
        // A client on the local network, through the proxy or not.
        assert_eq!(from("127.0.0.1", &[b"192.168.1.20"]), "192.168.1.20");
        assert_eq!(from("192.168.1.20", &[]), "192.168.1.20");
        // Ports some proxies add.
        assert_eq!(from("127.0.0.1", &[b"203.0.113.9:4711"]), "203.0.113.9");
        assert_eq!(from("127.0.0.1", &[b"[2001:db8::7]:4711"]), "2001:db8::7");
        // Anything else is the proxy's own address.
        for odd in [&b"spoof-1"[..], b"", b"unknown", b"203.0.113.9, unix:", b"\xff203.0.113.9"] {
            assert_eq!(from("127.0.0.1", &[odd]), "127.0.0.1", "{}", String::from_utf8_lossy(odd));
        }
        // IPv4 clients of an IPv6 socket count as IPv4.
        assert_eq!(from("::ffff:203.0.113.9", &[b"10.0.0.1"]), "203.0.113.9");
        assert_eq!(from("::ffff:127.0.0.1", &[b"::ffff:203.0.113.9"]), "203.0.113.9");
        // At most 10 proxies in a row.
        let ten = ["10.0.0.1"; 10].join(", ");
        assert_eq!(from("127.0.0.1", &[format!("203.0.113.9, {ten}").as_bytes()]), "10.0.0.1");
        let nine = ["10.0.0.1"; 9].join(", ");
        assert_eq!(from("127.0.0.1", &[format!("203.0.113.9, {nine}").as_bytes()]), "203.0.113.9");
    }

    async fn service_with_user(dir: &std::path::Path) -> (Db, AuthService) {
        let db = Db::open(&dir.join("pley.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, is_admin, all_libraries, max_age, created_at)
             VALUES (1, 'kid', 'x', 0, 0, 12, 0), (2, 'admin', 'x', 1, 0, 12, 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        let service = AuthService::new(db.clone());
        (db, service)
    }

    #[tokio::test]
    async fn sessions_carry_the_users_restrictions() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, auth) = service_with_user(dir.path()).await;
        let token = auth.create_session(1, Some("agent"), Some("10.0.0.2")).await.unwrap();
        let user = auth.validate(&token).await.unwrap();
        assert_eq!(
            user,
            AuthUser {
                id: 1,
                username: "kid".into(),
                is_admin: false,
                account_admin: false,
                all_libraries: false,
                max_age: Some(12),
                app: None,
                compatibility: None
            }
        );
        let admin = auth.create_session(2, None, None).await.unwrap();
        let admin = auth.validate(&admin).await.unwrap();
        assert_eq!(admin.max_age, None, "administrators have no age limit");
        assert!(admin.is_admin && admin.account_admin && admin.manages_requests() && admin.first_party());
        assert!(auth.validate("not-a-token").await.is_none());
    }

    #[tokio::test]
    async fn expired_and_disabled_sessions_stop_working() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        let expired = auth.create_session(1, None, None).await.unwrap();
        sqlx::query("UPDATE sessions SET expires_at = ? WHERE token_hash = ?")
            .bind(now() - 1)
            .bind(hash_token(&expired))
            .execute(&db.write)
            .await
            .unwrap();
        assert!(auth.validate(&expired).await.is_none());
        auth.purge_expired().await;
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&db.read).await.unwrap();
        assert_eq!(left, 0, "expired sessions are purged");

        // A cached session survives a change in the database until the
        // user is forgotten (what the users API does on every change).
        let token = auth.create_session(1, None, None).await.unwrap();
        assert!(auth.validate(&token).await.is_some());
        sqlx::query("UPDATE users SET disabled = 1 WHERE id = 1").execute(&db.write).await.unwrap();
        assert!(auth.validate(&token).await.is_some(), "served from the cache");
        auth.forget_user(1);
        assert!(auth.validate(&token).await.is_none(), "disabled users are signed out");

        // Revoking works whether or not the session is cached.
        sqlx::query("UPDATE users SET disabled = 0 WHERE id = 1").execute(&db.write).await.unwrap();
        let token = auth.create_session(1, None, None).await.unwrap();
        assert!(auth.validate(&token).await.is_some());
        auth.revoke(&token).await;
        assert!(auth.validate(&token).await.is_none());
    }

    #[tokio::test]
    async fn sessions_in_use_are_extended() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        let token = auth.create_session(1, None, None).await.unwrap();
        let stale = now() - TOUCH_INTERVAL_SECS - 60;
        sqlx::query("UPDATE sessions SET last_seen_at = ?, expires_at = ? WHERE token_hash = ?")
            .bind(stale)
            .bind(now() + 60)
            .bind(hash_token(&token))
            .execute(&db.write)
            .await
            .unwrap();
        assert!(auth.validate(&token).await.is_some());
        let (last_seen, expires): (i64, i64) =
            sqlx::query_as("SELECT last_seen_at, expires_at FROM sessions WHERE token_hash = ?")
                .bind(hash_token(&token))
                .fetch_one(&db.read)
                .await
                .unwrap();
        assert!(last_seen > stale);
        assert!(expires >= now() + SESSION_TTL_SECS - 5, "a full lifetime again");
    }

    /// Tokens of an app acting as `user_id`, allowed by `approved_by`.
    async fn app_of(auth: &AuthService, user_id: i64, approved_by: i64) -> Issued {
        auth.issue(NewGrant {
            user_id,
            approved_by,
            client_id: "https://pley-media-server.github.io/pley/oauth/android.json".into(),
            client_name: None,
            scopes: Scopes::parse("library play").unwrap(),
            refresh: true,
            first_party: true,
            session_hash: None,
            user_agent: None,
            ip: None,
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn app_tokens_act_without_administrator_rights() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, auth) = service_with_user(dir.path()).await;
        let admin = app_of(&auth, 2, 2).await;
        let user = auth.validate_app(&admin.access_token).await.unwrap();
        assert!(!user.is_admin, "never an administrator through an app");
        assert!(user.all_libraries, "an administrator's role sees every library");
        assert_eq!(user.max_age, None, "and has no age limit");
        let app = user.app.as_deref().unwrap();
        assert_eq!((app.grant_id, app.approved_by), (admin.grant_id, 2));
        assert_eq!(app.scopes.to_string(), "library play");
        assert_eq!(&*app.client_id, "https://pley-media-server.github.io/pley/oauth/android.json");

        let kid = app_of(&auth, 1, 1).await;
        let kid = auth.validate_app(&kid.access_token).await.unwrap();
        assert_eq!((kid.id, kid.is_admin, kid.all_libraries, kid.max_age), (1, false, false, Some(12)));
        assert!(auth.validate_any(&admin.access_token).await.is_some_and(|u| u.app.is_some()));
        assert!(auth.validate_app(admin.refresh_token.as_ref().unwrap()).await.is_none(), "no refresh token");
        assert!(auth.validate_app("pley_at_unknown").await.is_none());
    }

    #[tokio::test]
    async fn disabled_users_and_approvers_stop_their_apps() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        // The kid's profile, allowed by the administrator.
        let issued = app_of(&auth, 1, 2).await;
        for id in [1, 2] {
            sqlx::query("UPDATE users SET disabled = 1 WHERE id = ?").bind(id).execute(&db.write).await.unwrap();
            auth.forget_user(id);
            assert!(auth.validate_app(&issued.access_token).await.is_none(), "user {id} disabled");
            sqlx::query("UPDATE users SET disabled = 0 WHERE id = ?").bind(id).execute(&db.write).await.unwrap();
            auth.forget_user(id);
            assert!(auth.validate_app(&issued.access_token).await.is_some(), "user {id} enabled again");
        }
        sqlx::query("UPDATE app_tokens SET expires_at = ? WHERE kind = 'access'")
            .bind(now() - 1)
            .execute(&db.write)
            .await
            .unwrap();
        auth.forget_apps();
        assert!(auth.validate_app(&issued.access_token).await.is_none(), "expired");
    }

    #[tokio::test]
    async fn forgetting_reaches_cached_app_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        let disable = |id: i64, off: bool| {
            let db = db.clone();
            async move {
                sqlx::query("UPDATE users SET disabled = ? WHERE id = ?")
                    .bind(off)
                    .bind(id)
                    .execute(&db.write)
                    .await
                    .unwrap();
            }
        };
        let issued = app_of(&auth, 1, 2).await;
        let token = issued.access_token.as_str();
        assert!(auth.validate_app(token).await.is_some());

        // The profile it acts as.
        disable(1, true).await;
        assert!(auth.validate_app(token).await.is_some(), "served from the cache");
        auth.forget_user(1);
        assert!(auth.validate_app(token).await.is_none());
        disable(1, false).await;
        assert!(auth.validate_app(token).await.is_some());

        // Who allowed it.
        disable(2, true).await;
        assert!(auth.validate_app(token).await.is_some(), "served from the cache");
        auth.forget_user(2);
        assert!(auth.validate_app(token).await.is_none());
        disable(2, false).await;
        assert!(auth.validate_app(token).await.is_some());

        // Its grant, and every app.
        let other = app_of(&auth, 2, 2).await;
        assert!(auth.validate_app(&other.access_token).await.is_some());
        sqlx::query("DELETE FROM app_grants").execute(&db.write).await.unwrap();
        assert!(auth.validate_app(token).await.is_some(), "served from the cache");
        auth.forget_grants(&[issued.grant_id]);
        assert!(auth.validate_app(token).await.is_none());
        assert!(auth.validate_app(&other.access_token).await.is_some(), "another grant stays cached");
        auth.forget_apps();
        assert!(auth.validate_app(&other.access_token).await.is_none());

        // A validation that read the database before a forget doesn't cache.
        let again = app_of(&auth, 1, 1).await;
        let key = hash_token(&again.access_token);
        let user = auth.validate_app(&again.access_token).await.unwrap();
        auth.apps.clear();
        let epoch = auth.epoch.load(Ordering::SeqCst);
        auth.forget_grants(&[again.grant_id]);
        let entry = CachedApp { user, expires_at: now() + 60, cached_at: Instant::now() };
        auth.remember_app(key.clone(), entry, epoch);
        assert!(!auth.apps.contains_key(&key));
    }

    #[tokio::test]
    async fn a_forget_during_an_app_tokens_lookup_keeps_it_out_of_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        let issued = app_of(&auth, 1, 1).await;
        let gate = Arc::new(Gate::default());
        *auth.pauses.after_query.lock() = Some(gate.clone());
        // The grant ends while the lookup holds what it read before.
        let ending = async {
            gate.reached.notified().await;
            sqlx::query("DELETE FROM app_grants WHERE id = ?").bind(issued.grant_id).execute(&db.write).await.unwrap();
            auth.forget_grants(&[issued.grant_id]);
            gate.release.notify_one();
        };
        let (read, ()) = tokio::join!(auth.validate_app(&issued.access_token), ending);
        assert!(read.is_some(), "it read the grant before it ended");
        assert!(!auth.apps.contains_key(&hash_token(&issued.access_token)), "nothing stale is cached");
        assert!(auth.validate_app(&issued.access_token).await.is_none());
    }

    #[tokio::test]
    async fn a_sign_out_during_a_sessions_lookup_keeps_it_out_of_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, auth) = service_with_user(dir.path()).await;
        let session = auth.create_session(1, None, None).await.unwrap();
        let gate = Arc::new(Gate::default());
        *auth.pauses.after_query.lock() = Some(gate.clone());
        // Signed out while the lookup holds what it read before.
        let signing_out = async {
            gate.reached.notified().await;
            auth.revoke(&session).await;
            gate.release.notify_one();
        };
        let (read, ()) = tokio::join!(auth.validate(&session), signing_out);
        assert!(read.is_some(), "it read the session before it ended");
        assert!(!auth.cache.contains_key(&hash_token(&session)), "nothing stale is cached");
        assert!(auth.validate(&session).await.is_none());

        // A user forgotten (disabled, a password changed) likewise.
        let other = auth.create_session(1, None, None).await.unwrap();
        let gate = Arc::new(Gate::default());
        *auth.pauses.after_query.lock() = Some(gate.clone());
        let forgetting = async {
            gate.reached.notified().await;
            auth.forget_user(1);
            gate.release.notify_one();
        };
        let (read, ()) = tokio::join!(auth.validate(&other), forgetting);
        assert!(read.is_some());
        assert!(!auth.cache.contains_key(&hash_token(&other)), "nothing stale is cached");
    }

    #[tokio::test]
    async fn a_forget_between_the_check_and_the_insert_still_wins() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, auth) = service_with_user(dir.path()).await;
        let issued = app_of(&auth, 1, 1).await;
        let grant_id = issued.grant_id;
        *auth.pauses.before_insert.lock() = Some(Box::new(move |auth: &AuthService| auth.forget_grants(&[grant_id])));
        assert!(auth.validate_app(&issued.access_token).await.is_some());
        assert!(!auth.apps.contains_key(&hash_token(&issued.access_token)), "the forget's sweep came too early");
    }

    #[tokio::test]
    async fn session_validation_never_accepts_app_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        let issued = app_of(&auth, 1, 1).await;
        // Even a session stored under the same hash.
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, last_seen_at, expires_at) VALUES (?, 1, 0, 0, ?)",
        )
        .bind(hash_token(&issued.access_token))
        .bind(now() + 3600)
        .execute(&db.write)
        .await
        .unwrap();
        assert!(auth.validate(&issued.access_token).await.is_none());
        assert!(auth.validate(issued.refresh_token.as_ref().unwrap()).await.is_none());
        let session = auth.create_session(1, None, None).await.unwrap();
        assert!(auth.validate_app(&session).await.is_none(), "and the other way round");
        assert!(auth.validate_any(&session).await.is_some_and(|u| u.app.is_none()));
    }

    #[test]
    fn session_tokens_never_start_like_app_tokens() {
        let mut draws = ["pley_at_x", "pley_abc", "pleyfine"].into_iter().map(str::to_owned);
        assert_eq!(session_token(|| draws.next().unwrap()), "pleyfine");
        assert!(!session_token(new_token).starts_with(APP_TOKEN_PREFIX));
    }

    #[tokio::test]
    async fn a_new_session_never_gets_a_token_that_looks_like_an_apps() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, auth) = service_with_user(dir.path()).await;
        let mut draws = ["pley_at_x", "pley_rt_y", "pleyfine"].into_iter().map(str::to_owned);
        let token = auth.create_session_drawing(|| draws.next().unwrap(), 1, None, None).await.unwrap();
        assert_eq!(token, "pleyfine");
        assert!(auth.validate(&token).await.is_some());
    }

    /// A compatibility token of `user_id` with `scope`.
    async fn compatibility_token(auth: &AuthService, user_id: i64, scope: &str) -> String {
        let asked = compatibility::asked("Tautulli", Some(scope)).unwrap();
        auth.create_compatibility_token(user_id, &asked).await.unwrap().0
    }

    #[tokio::test]
    async fn compatibility_tokens_act_as_administrators_only_to_see_with_activity() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, auth) = service_with_user(dir.path()).await;
        use compatibility::Scope::{Activity, Manage, Play};

        let play = auth.validate_compatibility(&compatibility_token(&auth, 2, "play").await, None).await.unwrap();
        assert_eq!((play.id, play.is_admin, play.account_admin), (2, false, true));
        assert!(play.all_libraries && play.max_age.is_none(), "an administrator's role sees everything");
        assert!(play.app.is_none());
        assert!(play.compatibility_allows(Play));
        assert!(!play.compatibility_allows(Activity) && !play.compatibility_allows(Manage));

        let activity = auth.validate_compatibility(&compatibility_token(&auth, 2, "activity").await, None).await;
        let activity = activity.unwrap();
        assert!(activity.is_admin && activity.compatibility_allows(Activity));
        assert!(!activity.compatibility_allows(Manage), "seeing isn't changing");

        let manage = auth.validate_compatibility(&compatibility_token(&auth, 2, "manage").await, None).await.unwrap();
        assert!(!manage.is_admin, "changing isn't seeing everyone's");
        assert!(manage.compatibility_allows(Manage));

        // Someone else's limits stay theirs.
        let kid = auth.validate_compatibility(&compatibility_token(&auth, 1, "play").await, None).await.unwrap();
        assert_eq!((kid.id, kid.is_admin, kid.all_libraries, kid.max_age), (1, false, false, Some(12)));
        // Rights stored with a token are checked against the role at use.
        let stored = compatibility_token(&auth, 1, "activity manage").await;
        let kid = auth.validate_compatibility(&stored, None).await.unwrap();
        assert!(!kid.is_admin && !kid.compatibility_allows(Activity) && !kid.compatibility_allows(Manage));

        assert!(auth.validate_compatibility("pley_ct_unknown", None).await.is_none());
        let session = auth.create_session(2, None, None).await.unwrap();
        assert!(auth.validate_compatibility(&session, None).await.is_none(), "never a session");
        let app = app_of(&auth, 2, 2).await;
        assert!(auth.validate_compatibility(&app.access_token, None).await.is_none(), "nor an app's token");
        let token = compatibility_token(&auth, 2, "play").await;
        assert!(auth.validate(&token).await.is_none(), "and the other way round");
        assert!(auth.validate_app(&token).await.is_none());
        assert!(auth.validate_any(&token).await.is_none());
    }

    #[tokio::test]
    async fn compatibility_tokens_end_and_stop_working() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        let token = compatibility_token(&auth, 1, "play").await;
        let other = compatibility_token(&auth, 2, "play").await;
        assert!(auth.validate_compatibility(&token, None).await.is_some());

        // A disabled account's: served from the cache until it is forgotten,
        // as sessions are, and working again once it is enabled.
        sqlx::query("UPDATE users SET disabled = 1 WHERE id = 1").execute(&db.write).await.unwrap();
        assert!(auth.validate_compatibility(&token, None).await.is_some(), "served from the cache");
        auth.forget_user(1);
        assert!(auth.validate_compatibility(&token, None).await.is_none());
        sqlx::query("UPDATE users SET disabled = 0 WHERE id = 1").execute(&db.write).await.unwrap();
        assert!(auth.validate_compatibility(&token, None).await.is_some());

        // Revoked, cached or not; others stay.
        let id: i64 = sqlx::query_scalar("SELECT id FROM compatibility_tokens WHERE token_hash = ?")
            .bind(hash_token(&token))
            .fetch_one(&db.read)
            .await
            .unwrap();
        assert!(!auth.end_compatibility_token(2, id).await.unwrap(), "only by its own account");
        assert!(auth.validate_compatibility(&token, None).await.is_some());
        assert!(auth.end_compatibility_token(1, id).await.unwrap());
        assert!(auth.validate_compatibility(&token, None).await.is_none());
        assert!(auth.validate_compatibility(&other, None).await.is_some());

        // Signed out everywhere, by its value, and expired.
        let token = compatibility_token(&auth, 1, "play").await;
        assert!(auth.validate_compatibility(&token, None).await.is_some());
        auth.sign_out_everywhere(1, None).await.unwrap();
        assert!(auth.validate_compatibility(&token, None).await.is_none());
        assert!(auth.end_compatibility_token_of(&other).await.unwrap());
        assert!(auth.validate_compatibility(&other, None).await.is_none());
        assert!(!auth.end_compatibility_token_of(&other).await.unwrap());
        let token = compatibility_token(&auth, 1, "play").await;
        sqlx::query("UPDATE compatibility_tokens SET expires_at = ?").bind(now() - 1).execute(&db.write).await.unwrap();
        auth.forget_user(1);
        assert!(auth.validate_compatibility(&token, None).await.is_none(), "90 days unused");
        auth.purge_expired().await;
        let left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM compatibility_tokens").fetch_one(&db.read).await.unwrap();
        assert_eq!(left, 0);

        // Required two-step sign-in, of an account with a password alone.
        let token = compatibility_token(&auth, 1, "play").await;
        assert!(auth.validate_compatibility(&token, None).await.is_some());
        auth.require_two_step(true);
        assert!(auth.validate_compatibility(&token, None).await.is_none());
        auth.require_two_step(false);
        assert!(auth.validate_compatibility(&token, None).await.is_some());
    }

    #[tokio::test]
    async fn compatibility_tokens_in_use_are_extended_and_say_where_from() {
        let dir = tempfile::tempdir().unwrap();
        let (db, auth) = service_with_user(dir.path()).await;
        let token = compatibility_token(&auth, 1, "play").await;
        let row = || {
            let db = db.clone();
            let key = hash_token(&token);
            async move {
                sqlx::query_as::<_, (Option<i64>, i64, Option<String>, Option<String>)>(
                    "SELECT last_used_at, expires_at, last_ip, user_agent FROM compatibility_tokens WHERE token_hash = ?",
                )
                .bind(key)
                .fetch_one(&db.read)
                .await
                .unwrap()
            }
        };
        assert_eq!(row().await.0, None, "not used yet");
        let used = UsedFrom { ip: Some("10.0.0.7".into()), user_agent: Some("Tautulli/2.15".into()) };
        assert!(auth.validate_compatibility(&token, Some(&used)).await.is_some());
        let (last_used, expires, ip, agent) = row().await;
        assert!(last_used.is_some_and(|at| at >= now() - 5));
        assert!(expires >= now() + compatibility::IDLE_SECS - 5);
        assert_eq!((ip.as_deref(), agent.as_deref()), (Some("10.0.0.7"), Some("Tautulli/2.15")));

        // Written again only every few minutes; a check without a request
        // (the notification socket's) keeps where it was last used from.
        let stale = now() - TOUCH_INTERVAL_SECS - 60;
        sqlx::query("UPDATE compatibility_tokens SET last_used_at = ?, expires_at = ?")
            .bind(stale)
            .bind(now() + 60)
            .execute(&db.write)
            .await
            .unwrap();
        auth.compatibility.clear();
        assert!(auth.validate_compatibility(&token, None).await.is_some());
        let (last_used, expires, ip, _) = row().await;
        assert!(last_used.is_some_and(|at| at > stale) && expires >= now() + compatibility::IDLE_SECS - 5);
        assert_eq!(ip.as_deref(), Some("10.0.0.7"));
        sqlx::query("UPDATE compatibility_tokens SET last_used_at = ?").bind(now()).execute(&db.write).await.unwrap();
        auth.compatibility.clear();
        let other = UsedFrom { ip: Some("10.0.0.8".into()), user_agent: None };
        assert!(auth.validate_compatibility(&token, Some(&other)).await.is_some());
        assert_eq!(row().await.2.as_deref(), Some("10.0.0.7"), "not written within a few minutes");
    }

    #[tokio::test]
    async fn a_revocation_during_a_compatibility_tokens_lookup_keeps_it_out_of_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let (_db, auth) = service_with_user(dir.path()).await;
        let token = compatibility_token(&auth, 1, "play").await;
        let gate = Arc::new(Gate::default());
        *auth.pauses.after_query.lock() = Some(gate.clone());
        let revoking = async {
            gate.reached.notified().await;
            assert!(auth.end_compatibility_token_of(&token).await.unwrap());
            gate.release.notify_one();
        };
        let (read, ()) = tokio::join!(auth.validate_compatibility(&token, None), revoking);
        assert!(read.is_some(), "it read the token before it was revoked");
        assert!(!auth.compatibility.contains_key(&hash_token(&token)), "nothing stale is cached");
        assert!(auth.validate_compatibility(&token, None).await.is_none());
    }

    #[test]
    fn bearer_is_case_insensitive() {
        for header in ["Bearer abc", "bearer abc", "BEARER abc", "bEaReR   abc "] {
            let parts = axum::http::Request::builder().header("authorization", header).body(()).unwrap().into_parts().0;
            assert_eq!(bearer_token(&parts).as_deref(), Some("abc"), "{header}");
            assert_eq!(token_from_parts(&parts).as_deref(), Some("abc"), "{header}");
        }
        for header in ["Basic abc", "Bearerabc", "Bearer"] {
            let parts = axum::http::Request::builder().header("authorization", header).body(()).unwrap().into_parts().0;
            assert_eq!(bearer_token(&parts), None, "{header}");
        }
    }
}
