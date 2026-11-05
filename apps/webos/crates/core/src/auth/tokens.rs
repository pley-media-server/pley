//! The tokens of one server (packages/tv/src/auth/tokens.ts): the access
//! token while it works, a new pair from the refresh token when it doesn't.
//! Refreshes happen strictly one at a time (the server ends the sign-in when
//! a refresh token is used twice), and the new pair is saved before the new
//! access token is used. No token leaves before the server at the address
//! has proved it is the one signed in to ([`super::proof`]).

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};

use serde::{Deserialize, Serialize};

use super::proof::{self, ServerProof};
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
    /// What tells the server from another device later ([`ServerProof`]);
    /// absent from servers before proofs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof_key: Option<String>,
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
        let proof = answer.proof.as_ref();
        StoredTokens {
            issuer: issuer.into(),
            access_token: answer.access_token.clone(),
            refresh_token: answer.refresh_token.clone(),
            expires_at: now_ms + answer.expires_in * 1000,
            scope: answer.scope.clone(),
            token_endpoint: token_endpoint.into(),
            revocation_endpoint: revocation_endpoint.into(),
            server_id: proof.map(|p| p.server_id.clone()),
            proof_id: proof.map(|p| p.proof_id.clone()),
            proof_key: proof.map(|p| p.proof_key.clone()),
        }
    }

    /// The tokens of `answer` as kept after a refresh or a profile switch of
    /// `self`: the same sign-in, whose key never changes. `self`'s proof stays,
    /// whatever the answer says; an answer brings one only to a sign-in without.
    pub fn renewed(&self, answer: &TokenAnswerParsed, now_ms: u64) -> StoredTokens {
        let mut fresh =
            StoredTokens::new(answer, &self.issuer, &self.token_endpoint, &self.revocation_endpoint, now_ms);
        if self.proof().is_some() {
            (fresh.server_id, fresh.proof_id, fresh.proof_key) =
                (self.server_id.clone(), self.proof_id.clone(), self.proof_key.clone());
        }
        fresh
    }

    /// What the server proves itself with, when the sign-in has it and it is
    /// written as it should be.
    pub fn proof(&self) -> Option<ServerProof> {
        ServerProof::of(self.server_id.as_deref(), self.proof_id.as_deref(), self.proof_key.as_deref())
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
    /// The server's id and this sign-in's proof key; none from servers before proofs.
    pub proof: Option<ServerProof>,
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
        proof: ServerProof::of_answer(a.server_id.as_ref(), a.proof_id.as_ref(), a.proof_key.as_ref()),
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
    /// A cast's switch ticket that expired, was spent, or isn't this TV's.
    TicketSpent,
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
                    (_, Some("ticket_spent")) => SwitchRefusal::TicketSpent,
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
            SwitchRefusal::TicketSpent => "This cast can't switch profiles any more: cast it again.".into(),
            SwitchRefusal::Unreachable(reason) => format!("Can't reach your server: {reason}."),
            SwitchRefusal::SignedOut => "You were signed out: sign in again.".into(),
            SwitchRefusal::Other(m) => m.clone(),
        }
    }
}

type Listener = Arc<dyn Fn(SignOutReason) + Send + Sync>;
type Random = Arc<dyn Fn(usize) -> Vec<u8> + Send + Sync>;

/// The server's proofs as one sign-in sees them.
#[derive(Default)]
struct Proofs {
    /// When, and on which network ([`proof::network`]), the server last proved itself.
    proved_at: Option<(u64, u64)>,
    /// A proof is under way: other callers wait for it.
    proving: bool,
    /// Counts the proofs begun, so a caller waiting on one knows when it ended.
    round: u64,
    /// The last proof that failed, by its round, and why.
    failed: Option<(u64, NetError)>,
}

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
    proofs: Mutex<Proofs>,
    /// Told when a proof ends.
    proof_ended: Condvar,
    /// The challenges' random bytes.
    random: Random,
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
            proofs: Mutex::new(Proofs::default()),
            proof_ended: Condvar::new(),
            random: Arc::new(proof::random_bytes),
        }
    }

    /// These tokens, with challenges made of `random`'s bytes (for tests).
    pub fn with_random(mut self, random: impl Fn(usize) -> Vec<u8> + Send + Sync + 'static) -> Tokens {
        self.random = Arc::new(random);
        self
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
        lock(&self.proofs).proved_at = None;
    }

    /// Returns once the server at this address has proved it is the one
    /// signed in to, within the last half hour and on this network, or now
    /// when `now`: no token leaves before. A sign-in from a server without
    /// proofs has nothing to check.
    fn proved(&self, tokens: &StoredTokens, now: bool) -> Result<(), AuthError> {
        let Some(proof) = tokens.proof() else { return Ok(()) };
        let mut proofs = lock(&self.proofs);
        // A refresh token, a PIN or a sign-out waits for a proof of its own.
        if now {
            proofs.proved_at = None;
        }
        // A proof under way counts once it ends, unless the TV changed
        // networks meanwhile: looked at again then, as Android does.
        loop {
            if self.recently_proved(&proofs) {
                return Ok(());
            }
            if proofs.proving {
                let round = proofs.round;
                while proofs.proving && proofs.round == round {
                    proofs = self.proof_ended.wait(proofs).unwrap_or_else(|e| e.into_inner());
                }
                if let Some((failed, why)) = &proofs.failed
                    && *failed == round
                {
                    return Err(AuthError::Unreachable(why.clone()));
                }
                continue;
            }
            proofs.proving = true;
            proofs.round += 1;
            let (round, network) = (proofs.round, proof::network());
            drop(proofs);
            let result = proof::prove(&self.http, &self.issuer, &proof, &*self.random);
            proofs = lock(&self.proofs);
            proofs.proving = false;
            match &result {
                Ok(()) => proofs.proved_at = Some((self.clock.now_ms(), network)),
                Err(why) => proofs.failed = Some((round, why.clone())),
            }
            self.proof_ended.notify_all();
            result?;
        }
    }

    // A clock set back makes an age below zero: proved again.
    fn recently_proved(&self, proofs: &Proofs) -> bool {
        proofs.proved_at.is_some_and(|(at, network)| {
            let now = self.clock.now_ms();
            network == proof::network() && now >= at && now - at < proof::TTL_MS
        })
    }

    /// Whether the server proves it holds `tokens`' key now (none: nothing to check).
    fn proves(&self, tokens: &StoredTokens) -> bool {
        tokens.proof().is_none_or(|p| proof::prove(&self.http, &self.issuer, &p, &*self.random).is_ok())
    }

    /// A request to the server got no answer: the TV may be on another
    /// network, so the next token waits for a new proof.
    pub fn doubt_server(&self) {
        lock(&self.proofs).proved_at = None;
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

    /// The access token to send, refreshed first when it ends within a
    /// minute, once the server has proved itself.
    pub fn access_token(&self) -> Result<String, AuthError> {
        let tokens = self.load().ok_or(AuthError::SignedOut(SignOutReason::Ended))?;
        // A refresh waits for a proof of its own: one before it would be wasted.
        if tokens.expires_at.saturating_sub(self.clock.now_ms()) <= EARLY_MS {
            return self.refresh(Some(&tokens.access_token));
        }
        self.proved(&tokens, false)?;
        Ok(tokens.access_token)
    }

    /// A new access token in place of `stale` (refused, or about to end).
    /// When another caller refreshed meanwhile, its token is used and nothing is sent.
    pub fn refresh(&self, stale: Option<&str>) -> Result<String, AuthError> {
        let _one = lock(&self.serial);
        let tokens = self.load().ok_or(AuthError::SignedOut(SignOutReason::Ended))?;
        let fresh = tokens.expires_at.saturating_sub(self.clock.now_ms()) > EARLY_MS;
        if Some(tokens.access_token.as_str()) != stale && fresh {
            // Refreshed by another caller meanwhile, maybe on another network since.
            self.proved(&tokens, false)?;
            return Ok(tokens.access_token);
        }
        let Some(refresh) = tokens.refresh_token.clone() else { return Err(self.end(SignOutReason::Ended)) };
        // The refresh token goes only to a server that proves itself now.
        self.proved(&tokens, true)?;
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
        let fresh = tokens.renewed(&parsed, now);
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
        self.switch(profile, pin.map(|pin| ("pin", pin)))
    }

    /// Moves the sign-in to the profile of a remote that cast to this TV,
    /// with the cast's switch ticket in place of a PIN (docs/API.md,
    /// "Casting"). A ticket expired, spent or another device's is refused
    /// with `ticket_spent` ([`SwitchRefusal::TicketSpent`]).
    pub fn switch_profile_with_ticket(&self, profile: i64, ticket: &str) -> Result<(), AuthError> {
        self.switch(profile, Some(("ticket", ticket)))
    }

    /// The switch-profile grant to `profile`, with what stands for its PIN.
    fn switch(&self, profile: i64, proof: Option<(&str, &str)>) -> Result<(), AuthError> {
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
        if let Some(field) = proof {
            fields.push(field);
        }
        self.proved(&tokens, true)?;
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
        let fresh = tokens.renewed(&parsed, now);
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
        // A new sign-in, a new key: the next token waits for its proof.
        lock(&self.proofs).proved_at = None;
        // The sign-in before goes only to the server that proves it holds its
        // key: another device that got the TV to sign in again gets nothing.
        if let Some(previous) = previous
            && previous.refresh_token != fresh.refresh_token
            && self.proves(&previous)
        {
            revoke(&self.http, &self.client_id, &previous);
        }
        Ok(())
    }

    /// Ends this app's sign-in on the server (RFC 7009), then forgets the
    /// tokens whatever it answered (offline included). A server that can't
    /// prove itself gets no token: the person can sign the TV out in
    /// Account → Apps.
    pub fn sign_out(&self) {
        let _one = lock(&self.serial);
        if let Some(tokens) = self.load()
            && self.proves(&tokens)
        {
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

    fn doubt_server(&self) {
        Tokens::doubt_server(self)
    }
}

impl From<NetError> for AuthError {
    fn from(e: NetError) -> AuthError {
        AuthError::Unreachable(e)
    }
}
