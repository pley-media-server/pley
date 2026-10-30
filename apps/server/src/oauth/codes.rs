//! Authorization codes: what an app gets in its return address once a
//! person allowed it, and exchanges once for its tokens at the token
//! endpoint (OAuth 2.1 -16 §4.1). In memory only.
//!
//! A code carries 256 random bits, lives 60 seconds, and is bound to the
//! app, its return address, its PKCE challenge (RFC 7636, S256 only), the
//! person and the scopes. Only pley's SHA-256 of it is kept.
//!
//! An exchange with anything wrong (another app, another return address, a
//! wrong verifier) changes nothing: the code still works once afterwards. An
//! exchange with everything right that comes after the first one means
//! someone else has the code, so the access the first one got ends
//! (OAuth 2.1 -16 §4.1.3, §7.5.3), even when it is still being issued.
//! Codes are kept for their 60 seconds after they are used, to notice that.
//!
//! A code works only while the browser session that allowed the app is
//! signed in (checked as the grant is recorded), and when the account's
//! sign-ins end (a new password, signing out everywhere, disabling,
//! deletion), its codes end with them ([`Codes::forget_user`]): nothing
//! allowed before can sign an app in after.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use base64::Engine;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use super::AppSnapshot;
use crate::auth::scopes::Scopes;
use crate::auth::{hash_token, new_token};
use crate::util::constant_time_eq;

/// How long a code works.
pub const CODE_LIFETIME: Duration = Duration::from_secs(60);
/// Codes kept at most (each for a minute).
pub const MAX_CODES: usize = 1_000;
/// Codes one profile may have waiting to be exchanged: a new one drops its
/// oldest, so nobody fills the server's room for everyone else's.
const MAX_WAITING_PER_USER: usize = 10;
/// Codes one profile may have within their lifetime, exchanged or not.
const MAX_PER_USER: usize = 50;

/// What a person allowed, carried from the consent page to the token
/// endpoint by a code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGrant {
    pub app: AppSnapshot,
    /// The return address as the app sent it.
    pub redirect_uri: String,
    /// BASE64URL(SHA-256(verifier)) (RFC 7636 §4.2).
    pub code_challenge: String,
    /// The profile the app acts as, who allowed it.
    pub user_id: i64,
    /// SHA-256 of the token of the browser session that allowed it.
    pub session_hash: String,
    pub scopes: Scopes,
}

/// Too many codes are waiting to be exchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

/// What becomes of the grant the first exchange of a code made (see
/// [`Codes::issued`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issued {
    /// Nothing happened meanwhile: the app keeps it.
    Kept,
    /// The code was exchanged again meanwhile: someone else has it. End it.
    Replayed,
    /// The account's sign-ins ended meanwhile. End it.
    SignedOut,
}

/// What an exchange of a code gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Redeem {
    /// The first exchange with everything right: issue the tokens, then
    /// call [`Codes::issued`] (or [`Codes::failed`]).
    First(CodeGrant),
    /// The code was exchanged before: someone else has it. The grant its
    /// first exchange made, which must end, by the SHA-256 of one of its
    /// tokens; `None` while it is still being issued ([`Codes::issued`]
    /// then says so).
    Replay(Option<String>),
    /// Unknown, expired, or not this app's, return address or verifier.
    /// The code is as it was.
    Invalid(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Redeemed {
    No,
    /// Its tokens are being issued; meanwhile it was exchanged again, or
    /// the account's sign-ins ended.
    InProgress {
        replayed: bool,
        signed_out: bool,
    },
    /// Exchanged: the SHA-256 of a token of the grant it made, which stays
    /// on record as long as that grant does (unlike its number, which is
    /// given again once it ends).
    Grant(String),
}

struct Entry {
    grant: CodeGrant,
    created: Instant,
    redeemed: Redeemed,
}

impl Entry {
    fn alive(&self, now: Instant) -> bool {
        now.duration_since(self.created) < CODE_LIFETIME
    }
}

/// The codes handed out in the last minute, by the SHA-256 of each.
#[derive(Default)]
pub struct Codes {
    codes: Mutex<HashMap<String, Entry>>,
}

/// Whether `verifier` is a PKCE code verifier: 43 to 128 of
/// `[A-Za-z0-9._~-]` (RFC 7636 §4.1).
pub fn is_verifier(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len())
        && verifier.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-'))
}

/// Whether `challenge` could be an S256 challenge: 43 base64url characters
/// (the SHA-256 of a verifier).
pub fn is_challenge(challenge: &str) -> bool {
    challenge.len() == 43 && challenge.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

/// BASE64URL(SHA-256(verifier)) (RFC 7636 §4.2).
pub fn s256(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

impl Codes {
    /// A new code for what a person allowed.
    pub fn new_code(&self, grant: CodeGrant) -> Result<String, Full> {
        let now = Instant::now();
        let mut codes = self.codes.lock();
        let theirs: Vec<(&String, &Entry)> =
            codes.iter().filter(|(_, entry)| entry.grant.user_id == grant.user_id && entry.alive(now)).collect();
        if theirs.len() >= MAX_PER_USER {
            return Err(Full);
        }
        let waiting = theirs.iter().filter(|(_, entry)| matches!(entry.redeemed, Redeemed::No));
        if waiting.clone().count() >= MAX_WAITING_PER_USER {
            let oldest = waiting.min_by_key(|(_, entry)| entry.created).map(|(key, _)| (*key).clone());
            if let Some(oldest) = oldest {
                codes.remove(&oldest);
            }
        }
        if codes.len() >= MAX_CODES {
            codes.retain(|_, entry| entry.alive(now));
            if codes.len() >= MAX_CODES {
                return Err(Full);
            }
        }
        let code = new_token();
        codes.insert(hash_token(&code), Entry { grant, created: now, redeemed: Redeemed::No });
        Ok(code)
    }

    /// Exchanges `code` for what it was given for, when the app, the return
    /// address (when sent: OAuth 2.1 -16 §10.2) and the verifier are right.
    /// Decided under the lock, so only one exchange is ever the first.
    pub fn redeem(&self, code: &str, client_id: &str, redirect_uri: Option<&str>, verifier: &str) -> Redeem {
        let now = Instant::now();
        let mut codes = self.codes.lock();
        let Some(entry) = codes.get_mut(&hash_token(code)) else { return Redeem::Invalid("unknown code") };
        if !entry.alive(now) {
            return Redeem::Invalid("the code has expired");
        }
        if entry.grant.app.id != client_id {
            return Redeem::Invalid("the code was given to another app");
        }
        if redirect_uri.is_some_and(|uri| uri != entry.grant.redirect_uri) {
            return Redeem::Invalid("the code was sent to another return address");
        }
        if !is_verifier(verifier) || !constant_time_eq(s256(verifier).as_bytes(), entry.grant.code_challenge.as_bytes())
        {
            return Redeem::Invalid("the code verifier doesn't match");
        }
        match entry.redeemed {
            Redeemed::No => {
                entry.redeemed = Redeemed::InProgress { replayed: false, signed_out: false };
                Redeem::First(entry.grant.clone())
            }
            Redeemed::InProgress { signed_out, .. } => {
                entry.redeemed = Redeemed::InProgress { replayed: true, signed_out };
                Redeem::Replay(None)
            }
            Redeemed::Grant(ref token_hash) => Redeem::Replay(Some(token_hash.clone())),
        }
    }

    /// The first exchange of `code` made a grant, one of whose tokens has
    /// the SHA-256 `token_hash` (one it keeps on record while it lasts):
    /// whether the app keeps it, or it must end now because the code was
    /// exchanged again meanwhile, or the account's sign-ins ended.
    pub fn issued(&self, code: &str, token_hash: String) -> Issued {
        let mut codes = self.codes.lock();
        let Some(entry) = codes.get_mut(&hash_token(code)) else { return Issued::Kept };
        let issued = match entry.redeemed {
            Redeemed::InProgress { replayed: true, .. } => Issued::Replayed,
            Redeemed::InProgress { signed_out: true, .. } => Issued::SignedOut,
            _ => Issued::Kept,
        };
        entry.redeemed = Redeemed::Grant(token_hash);
        issued
    }

    /// The sign-ins of `user_id` ended (a new password, signing out
    /// everywhere, disabling, deletion): the codes they were given and
    /// nobody exchanged are gone, and one whose tokens are being issued
    /// right now gets nothing ([`Codes::issued`]). Used ones stay, to
    /// notice replays: the grants they made end with the account's apps.
    pub fn forget_user(&self, user_id: i64) {
        self.codes.lock().retain(|_, entry| {
            if entry.grant.user_id != user_id {
                return true;
            }
            match &mut entry.redeemed {
                Redeemed::No => false,
                Redeemed::InProgress { signed_out, .. } => {
                    *signed_out = true;
                    true
                }
                Redeemed::Grant(_) => true,
            }
        });
    }

    /// The first exchange of `code` got no tokens (the account was disabled
    /// meanwhile, apps by other publishers were turned off, or the database
    /// failed). The code is gone: the app starts again.
    pub fn failed(&self, code: &str) {
        self.codes.lock().remove(&hash_token(code));
    }

    /// Whether `code` still works and was never exchanged: the return
    /// address that carries it may be shown again.
    pub fn unredeemed(&self, code: &str) -> bool {
        let now = Instant::now();
        self.codes.lock().get(&hash_token(code)).is_some_and(|entry| entry.alive(now) && entry.redeemed == Redeemed::No)
    }

    /// Forgets codes older than a minute.
    pub fn sweep(&self) {
        let now = Instant::now();
        self.codes.lock().retain(|_, entry| entry.alive(now));
    }

    /// Makes `code` look `age` old (tests).
    #[cfg(test)]
    fn age(&self, code: &str, age: Duration) {
        if let Some(entry) = self.codes.lock().get_mut(&hash_token(code)) {
            entry.created = Instant::now().checked_sub(age).expect("a time in the past");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
    const APP: &str = "https://pley-media-server.github.io/pley/oauth/android.json";
    const BACK: &str = "app.pley.android:/oauth/callback";

    fn grant() -> CodeGrant {
        CodeGrant {
            app: AppSnapshot {
                id: APP.into(),
                host: "pley-media-server.github.io".into(),
                name: Some("Pley for Android".into()),
                first_party: true,
                refresh: true,
            },
            redirect_uri: BACK.into(),
            code_challenge: CHALLENGE.into(),
            user_id: 7,
            session_hash: "session-of-7".into(),
            scopes: Scopes::parse("library play").unwrap(),
        }
    }

    #[test]
    fn s256_is_rfc_7636s() {
        // RFC 7636 appendix B.
        assert_eq!(s256(VERIFIER), CHALLENGE);
        assert!(is_verifier(VERIFIER) && is_challenge(CHALLENGE));
        assert!(!is_verifier(&"a".repeat(42)) && is_verifier(&"a".repeat(43)));
        assert!(is_verifier(&"a".repeat(128)) && !is_verifier(&"a".repeat(129)));
        assert!(!is_verifier(&format!("{}+", "a".repeat(43))), "base64, not base64url");
        assert!(is_verifier(&format!("{}.~_-", "a".repeat(43))));
        assert!(!is_challenge(&"a".repeat(42)) && !is_challenge(&"a".repeat(44)));
        assert!(!is_challenge(&format!("{}=", "a".repeat(42))));
    }

    #[test]
    fn a_code_works_once_with_everything_right() {
        let codes = Codes::default();
        let code = codes.new_code(grant()).unwrap();
        assert_eq!(code.len(), 43);
        let wrong = [
            codes.redeem("nope", APP, None, VERIFIER),
            codes.redeem(&code, "https://apps.example.com/app.json", None, VERIFIER),
            codes.redeem(&code, APP, Some("app.pley.android:/other"), VERIFIER),
            codes.redeem(&code, APP, Some(BACK), &"a".repeat(43)),
            codes.redeem(&code, APP, Some(BACK), ""),
            codes.redeem(&code, APP, Some(BACK), CHALLENGE),
        ];
        assert!(wrong.iter().all(|r| matches!(r, Redeem::Invalid(_))), "{wrong:?}");
        assert!(codes.unredeemed(&code), "wrong exchanges change nothing");
        assert_eq!(codes.redeem(&code, APP, Some(BACK), VERIFIER), Redeem::First(grant()));
        assert!(!codes.unredeemed(&code));
        // The return address may be left out.
        assert_eq!(codes.redeem(&code, APP, None, VERIFIER), Redeem::Replay(None));
    }

    #[test]
    fn a_replay_ends_the_grant_even_while_it_is_issued() {
        let codes = Codes::default();
        let code = codes.new_code(grant()).unwrap();
        assert!(matches!(codes.redeem(&code, APP, None, VERIFIER), Redeem::First(_)));
        // Nobody came again: the grant stays.
        assert_eq!(codes.issued(&code, "grant-3".into()), Issued::Kept);
        assert_eq!(codes.redeem(&code, APP, None, VERIFIER), Redeem::Replay(Some("grant-3".into())));
        // Wrong ones are no replays.
        assert!(matches!(codes.redeem(&code, APP, None, &"b".repeat(43)), Redeem::Invalid(_)));

        let code = codes.new_code(grant()).unwrap();
        assert!(matches!(codes.redeem(&code, APP, None, VERIFIER), Redeem::First(_)));
        assert_eq!(codes.redeem(&code, APP, None, VERIFIER), Redeem::Replay(None));
        assert_eq!(codes.issued(&code, "grant-4".into()), Issued::Replayed, "the replay came during issuance");
        assert_eq!(codes.redeem(&code, APP, None, VERIFIER), Redeem::Replay(Some("grant-4".into())));
    }

    #[test]
    fn codes_end_with_the_sign_ins_of_whoever_allowed_them() {
        let codes = Codes::default();
        let waiting = codes.new_code(grant()).unwrap();
        let issuing = codes.new_code(grant()).unwrap();
        let used = codes.new_code(grant()).unwrap();
        let someone_elses = codes.new_code(CodeGrant { user_id: 8, ..grant() }).unwrap();
        assert!(matches!(codes.redeem(&issuing, APP, None, VERIFIER), Redeem::First(_)));
        assert!(matches!(codes.redeem(&used, APP, None, VERIFIER), Redeem::First(_)));
        assert_eq!(codes.issued(&used, "grant-5".into()), Issued::Kept);

        codes.forget_user(7);
        // Nobody exchanged it: it is gone.
        assert_eq!(codes.redeem(&waiting, APP, None, VERIFIER), Redeem::Invalid("unknown code"));
        // Being issued: what it gets ends at once.
        assert_eq!(codes.issued(&issuing, "grant-6".into()), Issued::SignedOut);
        // Used: its grant ends with the account's apps; a replay is still
        // noticed.
        assert_eq!(codes.redeem(&used, APP, None, VERIFIER), Redeem::Replay(Some("grant-5".into())));
        // Others' codes stay.
        assert!(codes.unredeemed(&someone_elses));
    }

    #[test]
    fn a_failed_issuance_uses_the_code_up() {
        let codes = Codes::default();
        let code = codes.new_code(grant()).unwrap();
        assert!(matches!(codes.redeem(&code, APP, None, VERIFIER), Redeem::First(_)));
        codes.failed(&code);
        assert!(matches!(codes.redeem(&code, APP, None, VERIFIER), Redeem::Invalid(_)));
    }

    #[test]
    fn codes_last_a_minute() {
        let codes = Codes::default();
        let code = codes.new_code(grant()).unwrap();
        codes.age(&code, Duration::from_secs(59));
        assert!(codes.unredeemed(&code));
        codes.age(&code, Duration::from_secs(61));
        assert!(!codes.unredeemed(&code));
        assert_eq!(codes.redeem(&code, APP, None, VERIFIER), Redeem::Invalid("the code has expired"));
        // A used one is kept as long, to notice replays, then forgotten.
        let used = codes.new_code(grant()).unwrap();
        assert!(matches!(codes.redeem(&used, APP, None, VERIFIER), Redeem::First(_)));
        assert_eq!(codes.issued(&used, "grant-1".into()), Issued::Kept);
        codes.sweep();
        assert_eq!(codes.codes.lock().len(), 1, "the expired one goes, the used one stays");
        codes.age(&used, Duration::from_secs(61));
        codes.sweep();
        assert!(codes.codes.lock().is_empty());
    }

    fn grant_for(user_id: i64) -> CodeGrant {
        CodeGrant { user_id, ..grant() }
    }

    #[test]
    fn codes_are_capped() {
        let codes = Codes::default();
        let first = codes.new_code(grant_for(0)).unwrap();
        for n in 1..MAX_CODES {
            codes.new_code(grant_for(n as i64 % 100)).unwrap();
        }
        assert_eq!(codes.new_code(grant_for(100)), Err(Full));
        // Room comes back as they expire.
        codes.age(&first, Duration::from_secs(61));
        assert!(codes.new_code(grant_for(100)).is_ok());
        assert_eq!(codes.new_code(grant_for(101)), Err(Full));
    }

    #[test]
    fn one_profile_does_not_fill_the_room_for_everyone() {
        let codes = Codes::default();
        let oldest = codes.new_code(grant()).unwrap();
        for _ in 1..MAX_CODES {
            codes.new_code(grant()).unwrap();
        }
        // Only its last few wait: older ones were dropped for them.
        assert_eq!(codes.codes.lock().len(), MAX_WAITING_PER_USER);
        assert!(matches!(codes.redeem(&oldest, APP, Some(BACK), VERIFIER), Redeem::Invalid(_)));
        assert!(codes.new_code(grant_for(8)).is_ok());
        // Exchanged codes count too, up to a limit of their own.
        let codes = Codes::default();
        for _ in 0..MAX_PER_USER {
            let code = codes.new_code(grant()).unwrap();
            assert!(matches!(codes.redeem(&code, APP, Some(BACK), VERIFIER), Redeem::First(_)));
        }
        assert_eq!(codes.new_code(grant()), Err(Full));
        assert!(codes.new_code(grant_for(8)).is_ok());
    }
}
