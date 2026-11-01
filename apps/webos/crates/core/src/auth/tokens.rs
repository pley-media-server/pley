//! The tokens of one server (packages/tv/src/auth/tokens.ts): the access
//! token while it works, a new pair from the refresh token when it doesn't.
//! Refreshes happen strictly one at a time (the server ends the sign-in when
//! a refresh token is used twice), and the new pair is saved before the new
//! access token is used.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

use serde::{Deserialize, Serialize};

use super::{AuthError, CLIENT_ID, SWITCH_PROFILE_GRANT, SignOutReason, post_form};
use crate::http::{ApiError, Http, NetError, TokenSource, sentence};
use crate::model::TokenAnswer;
use crate::store::{Store, keys};
use crate::task::{Clock, SystemClock};

/// How long before its end an access token is refreshed.
pub const EARLY_MS: u64 = 60_000;
/// How long a refreshed token counts as just issued: refused within this, the sign-in is over.
pub const JUST_ISSUED_MS: u64 = 60_000;
/// RFC 6749 §5.1: without `expires_in`, an hour, Pley's own.
const DEFAULT_EXPIRES_IN: u64 = 3600;

/// One server's sign-in as kept (`pley.tokens.<issuer>`, the web TV app's shape).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTokens {
    pub issuer: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// When the access token ends, in ms since 1970.
    pub expires_at: u64,
    #[serde(default)]
    pub scope: String,
    pub token_endpoint: String,
    pub revocation_endpoint: String,
}

impl fmt::Debug for StoredTokens {
    // Tokens never reach logs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredTokens")
            .field("issuer", &self.issuer)
            .field("expires_at", &self.expires_at)
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

impl StoredTokens {
    /// The tokens of `answer`, issued at `now_ms` by `issuer`.
    pub fn new(
        answer: &TokenAnswerParsed,
        issuer: &str,
        token_endpoint: &str,
        revocation_endpoint: &str,
        now_ms: u64,
    ) -> StoredTokens {
        StoredTokens {
            issuer: issuer.into(),
            access_token: answer.access_token.clone(),
            refresh_token: answer.refresh_token.clone(),
            expires_at: now_ms + answer.expires_in * 1000,
            scope: answer.scope.clone(),
            token_endpoint: token_endpoint.into(),
            revocation_endpoint: revocation_endpoint.into(),
        }
    }

    /// The scope as names.
    pub fn scopes(&self) -> Vec<&str> {
        self.scope.split(' ').filter(|s| !s.is_empty()).collect()
    }
}

/// A token answer, checked.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenAnswerParsed {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Seconds.
    pub expires_in: u64,
    pub scope: String,
}

impl fmt::Debug for TokenAnswerParsed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenAnswerParsed").field("expires_in", &self.expires_in).field("scope", &self.scope).finish()
    }
}

/// A token endpoint's answer `body`, for a request that asked `asked_scope`.
pub fn parse_token_answer(body: &[u8], asked_scope: &str) -> Result<TokenAnswerParsed, AuthError> {
    let unreadable = || AuthError::Failed("the server's answer can't be read: try again".into());
    let a: TokenAnswer = serde_json::from_slice(body).map_err(|_| unreadable())?;
    if a.access_token.is_empty() {
        return Err(unreadable());
    }
    if a.token_type.as_deref().is_some_and(|t| !t.eq_ignore_ascii_case("bearer")) {
        return Err(unreadable());
    }
    Ok(TokenAnswerParsed {
        access_token: a.access_token,
        refresh_token: a.refresh_token.filter(|r| !r.is_empty()),
        expires_in: a.expires_in.filter(|e| *e > 0.0).map_or(DEFAULT_EXPIRES_IN, |e| e as u64),
        // RFC 6749 §5.1: a scope left out is the one asked for.
        scope: a.scope.unwrap_or_else(|| asked_scope.to_string()),
    })
}

/// The scope names in `answer` that `asked` doesn't hold.
pub fn extra_scopes(answer: &str, asked: &str) -> Vec<String> {
    let have: Vec<&str> = asked.split(' ').filter(|s| !s.is_empty()).collect();
    answer.split(' ').filter(|s| !s.is_empty() && !have.contains(s)).map(str::to_string).collect()
}

/// Ends the grant of `tokens` on its server (RFC 7009); trouble is ignored
/// (the person can sign the TV out in Account → Apps). The refresh token ends
/// the whole grant; the access token too, when there is no other.
pub fn revoke(http: &Http, client_id: &str, tokens: &StoredTokens) {
    let token = tokens.refresh_token.as_deref().unwrap_or(&tokens.access_token);
    let _ = post_form(http, &tokens.revocation_endpoint, &[("token", token), ("client_id", client_id)]);
}

/// Why a profile switch was refused, and what to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchRefusal {
    WrongPin,
    /// Five wrong PINs in 15 minutes, in the server's words when it gave some.
    Throttled(String),
    /// A hundred wrong in a row: only the owner or an administrator can reset it.
    PinLocked,
    /// The profile sees more and has no PIN: it is approved on a phone with a new code.
    Browser,
    NotAllowed,
    /// The sign-in lacks the `profiles` scope.
    CantSwitch,
    /// An older server that can't check a PIN: the person goes through.
    CantCheckPin,
    Unreachable(String),
    SignedOut,
    /// Anything else, as a sentence.
    Other(String),
}

impl SwitchRefusal {
    /// The refusal `e` stands for.
    pub fn of(e: &AuthError) -> SwitchRefusal {
        match e {
            AuthError::Unreachable(net) => SwitchRefusal::Unreachable(net.reason().into()),
            AuthError::SignedOut(_) => SwitchRefusal::SignedOut,
            AuthError::Refused { status, oauth } => {
                let description = oauth.as_ref().and_then(|o| o.error_description.clone()).filter(|d| !d.is_empty());
                match (e.code(), e.reason()) {
                    (_, Some("wrong_pin")) => SwitchRefusal::WrongPin,
                    (_, Some("throttled")) => SwitchRefusal::Throttled(
                        description.unwrap_or_else(|| "too many wrong PINs: try again in a few minutes".into()),
                    ),
                    (_, Some("pin_locked")) => SwitchRefusal::PinLocked,
                    (_, Some("browser")) => SwitchRefusal::Browser,
                    (_, Some("not_allowed")) => SwitchRefusal::NotAllowed,
                    (Some("unauthorized_client"), _) => SwitchRefusal::CantSwitch,
                    (Some("invalid_request"), None) => SwitchRefusal::CantCheckPin,
                    _ if *status >= 500 => SwitchRefusal::Other("The server refused: try again.".into()),
                    _ => SwitchRefusal::Other(sentence(&e.to_string())),
                }
            }
            AuthError::Failed(m) => SwitchRefusal::Other(sentence(m)),
        }
    }

    /// The words the PIN pad shows.
    pub fn message(&self) -> String {
        match self {
            SwitchRefusal::WrongPin => "Wrong PIN.".into(),
            SwitchRefusal::Throttled(m) => sentence(m),
            SwitchRefusal::PinLocked => {
                "This profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it."
                    .into()
            }
            SwitchRefusal::Browser => "This profile needs its password: sign in again to use it.".into(),
            SwitchRefusal::NotAllowed => "You can't switch to this profile.".into(),
            SwitchRefusal::CantSwitch => "This sign-in can't switch profiles: sign in again to allow it.".into(),
            SwitchRefusal::CantCheckPin => String::new(),
            SwitchRefusal::Unreachable(reason) => format!("Can't reach your server: {reason}."),
            SwitchRefusal::SignedOut => "You were signed out: sign in again.".into(),
            SwitchRefusal::Other(m) => m.clone(),
        }
    }
}

type Listener = Arc<dyn Fn(SignOutReason) + Send + Sync>;

/// The tokens of one server. One per server in the whole app ([`Tokens::shared`]):
/// two would refresh with the same refresh token and end the sign-in.
pub struct Tokens {
    issuer: String,
    client_id: String,
    store: Store,
    http: Http,
    clock: Arc<dyn Clock>,
    /// Held through every refresh, switch, install and sign-out: one at a time.
    serial: Mutex<()>,
    /// What is kept, read once from the store.
    cache: Mutex<Option<Option<StoredTokens>>>,
    /// The access token the last refresh or switch gave, and when.
    last_issued: Mutex<Option<(String, u64)>>,
    listeners: Mutex<Vec<Listener>>,
}

impl fmt::Debug for Tokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tokens").field("issuer", &self.issuer).finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Tokens {
    /// The one `Tokens` of `issuer` kept in `store`, shared by every caller in
    /// the process.
    pub fn shared(issuer: &str, store: &Store, http: &Http) -> Arc<Tokens> {
        type Registry = Mutex<HashMap<(PathBuf, String), Weak<Tokens>>>;
        static ALL: OnceLock<Registry> = OnceLock::new();
        let mut all = lock(ALL.get_or_init(Default::default));
        all.retain(|_, t| t.strong_count() > 0);
        let key = (store.dir().to_path_buf(), issuer.to_string());
        if let Some(t) = all.get(&key).and_then(Weak::upgrade) {
            return t;
        }
        let t = Arc::new(Tokens::with_clock(issuer, store.clone(), http.clone(), Arc::new(SystemClock)));
        all.insert(key, Arc::downgrade(&t));
        t
    }

    /// Tokens of `issuer` on their own, with `clock` (for tests).
    pub fn with_clock(issuer: &str, store: Store, http: Http, clock: Arc<dyn Clock>) -> Tokens {
        Tokens {
            issuer: issuer.into(),
            client_id: CLIENT_ID.into(),
            store,
            http,
            clock,
            serial: Mutex::new(()),
            cache: Mutex::new(None),
            last_issued: Mutex::new(None),
            listeners: Mutex::new(Vec::new()),
        }
    }

    /// The server's origin these tokens are for.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Calls `f` whenever the server ends the sign-in or the scope guard does.
    pub fn listen(&self, f: impl Fn(SignOutReason) + Send + Sync + 'static) {
        lock(&self.listeners).push(Arc::new(f));
    }

    /// What is kept for this server; none when signed out. A record of another
    /// issuer counts as none.
    pub fn load(&self) -> Option<StoredTokens> {
        let mut cache = lock(&self.cache);
        if cache.is_none() {
            let kept = self.store.get::<StoredTokens>(&keys::tokens(&self.issuer)).filter(|t| t.issuer == self.issuer);
            *cache = Some(kept);
        }
        cache.clone().flatten()
    }

    fn save(&self, tokens: &StoredTokens) {
        *lock(&self.cache) = Some(Some(tokens.clone()));
        // A failed write keeps the pair for this run; the next launch signs in again.
        let _ = self.store.set(&keys::tokens(&self.issuer), tokens);
    }

    fn wipe(&self) {
        *lock(&self.cache) = Some(None);
        let _ = self.store.remove(&keys::tokens(&self.issuer));
    }

    /// Whether this TV holds a sign-in for the server.
    pub fn signed_in(&self) -> bool {
        self.load().is_some()
    }

    /// What the sign-in was allowed.
    pub fn scopes(&self) -> Vec<String> {
        self.load().map(|t| t.scopes().into_iter().map(str::to_string).collect()).unwrap_or_default()
    }

    /// Whether the sign-in was allowed `scope` (`profiles` lets the TV switch).
    pub fn allows(&self, scope: &str) -> bool {
        self.scopes().iter().any(|s| s == scope)
    }

    /// The access token to send, refreshed first when it ends within a minute.
    pub fn access_token(&self) -> Result<String, AuthError> {
        let tokens = self.load().ok_or(AuthError::SignedOut(SignOutReason::Ended))?;
        if tokens.expires_at.saturating_sub(self.clock.now_ms()) > EARLY_MS {
            return Ok(tokens.access_token);
        }
        self.refresh(Some(&tokens.access_token))
    }

    /// A new access token in place of `stale` (refused, or about to end).
    /// When another caller refreshed meanwhile, its token is used and nothing is sent.
    pub fn refresh(&self, stale: Option<&str>) -> Result<String, AuthError> {
        let _one = lock(&self.serial);
        let tokens = self.load().ok_or(AuthError::SignedOut(SignOutReason::Ended))?;
        let fresh = tokens.expires_at.saturating_sub(self.clock.now_ms()) > EARLY_MS;
        if Some(tokens.access_token.as_str()) != stale && fresh {
            return Ok(tokens.access_token);
        }
        let Some(refresh) = tokens.refresh_token.clone() else { return Err(self.end(SignOutReason::Ended)) };
        let answer = post_form(
            &self.http,
            &tokens.token_endpoint,
            &[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", &self.client_id)],
        );
        let answer = match answer {
            Err(e) if e.code() == Some("invalid_grant") => return Err(self.end(SignOutReason::Ended)),
            Err(e) => return Err(e),
            Ok(a) => a,
        };
        let parsed = parse_token_answer(&answer.body, &tokens.scope)?;
        let now = self.clock.now_ms();
        let fresh = StoredTokens::new(&parsed, &self.issuer, &tokens.token_endpoint, &tokens.revocation_endpoint, now);
        if !extra_scopes(&parsed.scope, &tokens.scope).is_empty() {
            return Err(self.widened(&fresh));
        }
        // A refresh token is good once: the new one is kept before anything else.
        self.save(&fresh);
        *lock(&self.last_issued) = Some((fresh.access_token.clone(), now));
        Ok(fresh.access_token)
    }

    /// The server answered `invalid_token` to `sent`. When `sent` is the token
    /// a refresh gave moments ago, another refresh won't do better: the
    /// sign-in is over, and this answers true. False when `sent` is older.
    pub fn refused_after_refresh(&self, sent: &str) -> bool {
        let _one = lock(&self.serial);
        let just = lock(&self.last_issued)
            .as_ref()
            .is_some_and(|(token, at)| token == sent && self.clock.now_ms().saturating_sub(*at) <= JUST_ISSUED_MS);
        if !just || self.load().is_none_or(|t| t.access_token != sent) {
            return false;
        }
        self.end(SignOutReason::Ended);
        true
    }

    /// Moves the sign-in to another profile of the household (Pley's
    /// switch-profile grant), or checks the PIN of the one it acts as. A
    /// refusal changes nothing, unless the refresh token itself is spent.
    pub fn switch_profile(&self, profile: i64, pin: Option<&str>) -> Result<(), AuthError> {
        let _one = lock(&self.serial);
        let tokens = self.load().ok_or(AuthError::SignedOut(SignOutReason::Ended))?;
        let Some(refresh) = tokens.refresh_token.clone() else { return Err(self.end(SignOutReason::Ended)) };
        let profile = profile.to_string();
        let mut fields = vec![
            ("grant_type", SWITCH_PROFILE_GRANT),
            ("client_id", self.client_id.as_str()),
            ("refresh_token", refresh.as_str()),
            ("profile", profile.as_str()),
        ];
        if let Some(pin) = pin {
            fields.push(("pin", pin));
        }
        let answer = match post_form(&self.http, &tokens.token_endpoint, &fields) {
            // Without a reason the refresh token itself is spent or unknown.
            Err(e) if e.code() == Some("invalid_grant") && e.reason().is_none() => {
                return Err(self.end(SignOutReason::Ended));
            }
            Err(e) => return Err(e),
            Ok(a) => a,
        };
        let parsed = parse_token_answer(&answer.body, &tokens.scope)?;
        let now = self.clock.now_ms();
        let fresh = StoredTokens::new(&parsed, &self.issuer, &tokens.token_endpoint, &tokens.revocation_endpoint, now);
        if !extra_scopes(&parsed.scope, &tokens.scope).is_empty() {
            return Err(self.widened(&fresh));
        }
        self.save(&fresh);
        *lock(&self.last_issued) = Some((fresh.access_token.clone(), now));
        Ok(())
    }

    /// Keeps the tokens of a sign-in that just finished, after the scope
    /// guard; the sign-in they replace on this server is ended.
    pub fn install(&self, fresh: StoredTokens, asked_scope: &str) -> Result<(), AuthError> {
        let _one = lock(&self.serial);
        if fresh.issuer != self.issuer {
            return Err(AuthError::Failed("these tokens are another server's".into()));
        }
        if !extra_scopes(&fresh.scope, asked_scope).is_empty() {
            revoke(&self.http, &self.client_id, &fresh);
            return Err(AuthError::SignedOut(SignOutReason::WiderScope));
        }
        let previous = self.load();
        self.save(&fresh);
        *lock(&self.last_issued) = None;
        if let Some(previous) = previous
            && previous.refresh_token != fresh.refresh_token
        {
            revoke(&self.http, &self.client_id, &previous);
        }
        Ok(())
    }

    /// Ends this app's sign-in on the server (RFC 7009), then forgets the
    /// tokens whatever it answered (offline included).
    pub fn sign_out(&self) {
        let _one = lock(&self.serial);
        if let Some(tokens) = self.load() {
            revoke(&self.http, &self.client_id, &tokens);
        }
        self.wipe();
        *lock(&self.last_issued) = None;
    }

    fn end(&self, reason: SignOutReason) -> AuthError {
        self.wipe();
        *lock(&self.last_issued) = None;
        let listeners = lock(&self.listeners).clone();
        for l in listeners {
            l(reason);
        }
        AuthError::SignedOut(reason)
    }

    /// A refresh or switch gave more than this sign-in holds: its grant is
    /// ended and the app signed out.
    fn widened(&self, fresh: &StoredTokens) -> AuthError {
        revoke(&self.http, &self.client_id, fresh);
        self.end(SignOutReason::WiderScope)
    }
}

impl TokenSource for Tokens {
    fn access_token(&self) -> Result<String, ApiError> {
        Tokens::access_token(self).map_err(ApiError::from)
    }

    fn refresh(&self, stale: &str) -> Result<String, ApiError> {
        Tokens::refresh(self, Some(stale)).map_err(ApiError::from)
    }

    fn refused_after_refresh(&self, sent: &str) -> bool {
        Tokens::refused_after_refresh(self, sent)
    }
}

impl From<NetError> for AuthError {
    fn from(e: NetError) -> AuthError {
        AuthError::Unreachable(e)
    }
}
