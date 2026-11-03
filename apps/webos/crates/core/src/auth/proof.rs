//! What tells the server from another device at its address (docs/API.md,
//! "Telling servers apart"): every token answer carries the server's id and a
//! key of this sign-in's own, and before a token goes to the server's address
//! the server proves it holds that key, for that address (the Android app's
//! `ServerProofs`, packages/tv/src/auth/tokens.ts, the native Samsung app's
//! `ServerProofs`).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ring::hmac;
use serde_json::Value;

use crate::addresses;
use crate::http::{Http, NetError, Request};

/// How long a server's proof counts, unless a request to it gets no answer
/// or the network changes meanwhile.
pub const TTL_MS: u64 = 30 * 60_000;
/// How long the server has to answer a challenge (the web TV app's 10 s).
pub const PROOF_TIMEOUT: Duration = Duration::from_secs(10);
/// The challenge's random bytes: 16, the fewest the server takes.
const CHALLENGE_BYTES: usize = 16;
/// A proof key's bytes.
const KEY_BYTES: usize = 32;
/// The most of a proof's answer read: it comes from a device not proved yet,
/// and the server's own is a few hundred bytes.
const ANSWER_MOST: u64 = 64 << 10;

/// Counts the times the TV may have changed networks.
static NETWORK: AtomicU64 = AtomicU64::new(0);

/// Which network the TV may be on, as a count: a proof counts on the network
/// it was made on.
pub fn network() -> u64 {
    NETWORK.load(Ordering::SeqCst)
}

/// The TV may be on another network (its connection changed, or it came
/// back to the app): every server proves itself again before its next token.
pub fn network_may_have_changed() {
    NETWORK.fetch_add(1, Ordering::SeqCst);
}

/// The server's id and this sign-in's proof key, as a token answer gives them.
#[derive(Clone, PartialEq, Eq)]
pub struct ServerProof {
    pub server_id: String,
    /// The sign-in's id, sent with the challenge.
    pub proof_id: String,
    /// 32 bytes in base64url without padding. Never logged.
    pub proof_key: String,
}

impl fmt::Debug for ServerProof {
    // Never print the key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerProof")
            .field("server_id", &self.server_id)
            .field("proof_id", &self.proof_id)
            .finish_non_exhaustive()
    }
}

impl ServerProof {
    /// The proof fields when there are all three and they are written as they
    /// should be: a sign-in id of 1 to 19 digits without a leading zero, and a
    /// key of 32 bytes in base64url without padding. Anything else is none,
    /// as from a server before proofs.
    pub fn of(server_id: Option<&str>, proof_id: Option<&str>, proof_key: Option<&str>) -> Option<ServerProof> {
        let (server_id, proof_id, proof_key) = (server_id?, proof_id?, proof_key?);
        let id_ok = proof_id.len() <= 19
            && proof_id.bytes().all(|b| b.is_ascii_digit())
            && proof_id.bytes().next().is_some_and(|b| b != b'0');
        if server_id.is_empty() || !id_ok || key_of(proof_key).is_none() {
            return None;
        }
        Some(ServerProof { server_id: server_id.into(), proof_id: proof_id.into(), proof_key: proof_key.into() })
    }

    /// The proof fields of a token answer (`server_id`, `proof_id`, `proof_key`).
    pub fn of_answer(
        server_id: Option<&Value>,
        proof_id: Option<&Value>,
        proof_key: Option<&Value>,
    ) -> Option<ServerProof> {
        ServerProof::of(server_id?.as_str(), proof_id?.as_str(), proof_key?.as_str())
    }

    /// The key's bytes.
    fn key(&self) -> Option<Vec<u8>> {
        key_of(&self.proof_key)
    }
}

/// The 32 bytes of a key in base64url without padding; none for anything else.
fn key_of(text: &str) -> Option<Vec<u8>> {
    from_base64url(text).filter(|k| k.len() == KEY_BYTES)
}

/// The proof of `server_id`, asked at `address`, for `challenge` under `key`:
/// HMAC-SHA256 of `pley-server-proof`, the id, the address and the challenge,
/// each on a line of its own, in base64url without padding.
pub fn proof(key: &[u8], server_id: &str, address: &str, challenge: &str) -> String {
    base64url(hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), &message(server_id, address, challenge)).as_ref())
}

fn message(server_id: &str, address: &str, challenge: &str) -> Vec<u8> {
    format!("pley-server-proof\n{server_id}\n{address}\n{challenge}").into_bytes()
}

/// `n` random bytes from the system.
pub fn random_bytes(n: usize) -> Vec<u8> {
    // Without the system's randomness, a challenge of zeros: the proof is
    // still the server's, only its freshness is weaker.
    secure_random(n).unwrap_or_else(|| vec![0u8; n])
}

/// `n` random bytes from the system's secure source; none when it gives none, for a secret
/// that mustn't fall back to anything guessable.
pub fn secure_random(n: usize) -> Option<Vec<u8>> {
    let mut bytes = vec![0u8; n];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).ok()?;
    Some(bytes)
}

/// Asks the server at `issuer` to prove it holds `proof`'s key. Fails with
/// what kept the answer away when nothing answers, [`NetError::ServerFailed`]
/// when the server (or a proxy before it) fails for now, and
/// [`NetError::AnotherDevice`] when what answers can't prove it.
///
/// Never cancelled with the task that asks: other requests may wait on the
/// same proof. No redirect is followed: a proof counts only from the address
/// itself.
pub fn prove(
    http: &Http,
    issuer: &str,
    proof: &ServerProof,
    random: &dyn Fn(usize) -> Vec<u8>,
) -> Result<(), NetError> {
    let key = proof.key().ok_or(NetError::AnotherDevice)?;
    let challenge = base64url(&random(CHALLENGE_BYTES));
    // The address as the server reads it from the request: the proof covers it.
    let address = addresses::origin(issuer).unwrap_or_else(|| issuer.trim_end_matches('/').to_string());
    let req = Request::get(format!("{address}/api/server?challenge={challenge}&proof_id={}", proof.proof_id))
        .header("Accept", "application/json")
        .timeout(PROOF_TIMEOUT)
        .cancel(None);
    let answer = http.open(req)?;
    if answer.status >= 500 {
        return Err(NetError::ServerFailed);
    }
    if answer.status != 200 {
        return Err(NetError::AnotherDevice);
    }
    let answer = answer.read_all(ANSWER_MOST).map_err(|e| match e {
        NetError::TooBig => NetError::AnotherDevice,
        e => e,
    })?;
    let body: Value = serde_json::from_slice(&answer.body).map_err(|_| NetError::AnotherDevice)?;
    let given = body.get("proof").and_then(Value::as_str).and_then(from_base64url);
    let same_server = body.get("id").and_then(Value::as_str) == Some(proof.server_id.as_str());
    let proved = given.is_some_and(|given| {
        // In a time that doesn't tell where they differ.
        hmac::verify(&hmac::Key::new(hmac::HMAC_SHA256, &key), &message(&proof.server_id, &address, &challenge), &given)
            .is_ok()
    });
    if same_server && proved { Ok(()) } else { Err(NetError::AnotherDevice) }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url without padding (RFC 4648 §5).
pub fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// The bytes of base64url without padding; none for anything else (padding,
/// `+` or `/`, a length no encoding has, or bits left over).
pub fn from_base64url(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            let v = ALPHABET.iter().position(|a| a == c)? as u32;
            n |= v << (18 - 6 * i);
        }
        let bytes = chunk.len() - 1;
        // Bits past the last byte are zero in the one spelling of these bytes.
        if bytes < 3 && n & ((1 << (24 - 8 * bytes)) - 1) != 0 {
            return None;
        }
        out.extend((0..bytes).map(|i| (n >> (16 - 8 * i)) as u8));
    }
    Some(out)
}

#[cfg(test)]
mod tests;
