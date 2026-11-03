//! Who this server is, so an app can tell it from another device at an
//! address it saved: its id (the one discovery answers), and proofs that
//! it holds the key an app was given.
//!
//! Each sign-in of an app (a grant) gets its own proof key in the token
//! answers, derived from the server's `secret.key` and the grant's id, so
//! nothing more is stored and no app can pass for the server to another.
//! Before sending its token to an address, an app sends a challenge of its
//! own, and the server answers with an HMAC of it under that key. Every app
//! can check an HMAC-SHA256 (Roku's BrightScript included), which a
//! signature wouldn't allow.
//!
//! A proof covers the address the server was asked at, as it names itself
//! there (its issuer), so a device that passes the challenge on to the
//! server's other address gets a proof for that address, which the app
//! refuses. The server makes no proof for a plain-http address to a client
//! on the internet: only something passing challenges on asks for one there.
//! An https address is a proxy's, and counts only from a proxy at home.
//!
//! Pley's own sign-in has no grant: the desktop app asks for a key with the
//! window's session ([`Identity::session_key`]). Its id is random, and the
//! key derives from it, so nothing is stored for it either.

use std::net::{IpAddr, Ipv6Addr, UdpSocket};
use std::path::Path;

use base64::Engine;
use ring::hmac;

/// The challenge an app sends: 16 to 64 random bytes, base64url without
/// padding.
const CHALLENGE_BYTES: std::ops::RangeInclusive<usize> = 16..=64;
/// Before the server's id and the challenge in what a proof covers, so the
/// key signs nothing else.
const PROOF_CONTEXT: &str = "pley-server-proof";

/// The file in the data folder that holds the server's id.
pub const ID_FILE: &str = "server-id";
/// Where older versions kept it, renamed to [`ID_FILE`] on start so apps
/// keep recognising the server.
const OLD_ID_FILE: &str = "plex-machine-id";

/// The server's id, 40 hexadecimal characters: read from the data folder,
/// made and saved there on first start.
pub fn load_id(data_dir: &Path) -> String {
    let file = data_dir.join(ID_FILE);
    let old = data_dir.join(OLD_ID_FILE);
    if !file.exists()
        && old.exists()
        && let Err(err) = std::fs::rename(&old, &file)
    {
        tracing::warn!("cannot rename {} to {}: {err}", old.display(), file.display());
    }
    let valid = |id: &String| id.len() == 40 && id.bytes().all(|b| b.is_ascii_hexdigit());
    let saved = [&file, &old]
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok().map(|s| s.trim().to_owned()).filter(valid));
    saved.unwrap_or_else(|| {
        let id = format!("{}{}", uuid::Uuid::new_v4().simple(), &uuid::Uuid::new_v4().simple().to_string()[..8]);
        if let Err(err) = std::fs::write(&file, &id) {
            tracing::warn!("cannot save {}: {err}", file.display());
        }
        id
    })
}

pub struct Identity {
    /// The 40 hexadecimal characters discovery and apps know the server by.
    pub id: String,
    /// Derives the proof keys; `None` while `secret.key` can't be used.
    root: Option<hmac::Key>,
}

/// How many random bytes name a session's key.
const SESSION_KEY_ID_BYTES: usize = 16;

/// What an app keeps of a sign-in to tell its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofKey {
    /// The grant's id, or `s` and a session key's random id: what the app
    /// sends with its challenge.
    pub id: String,
    /// The key, base64url without padding.
    pub key: String,
}

impl Identity {
    pub fn new(data_dir: &Path) -> Self {
        let id = load_id(data_dir);
        // TwoStep::new says in the log why the key can't be used.
        let secret = crate::two_step::load_key(&data_dir.join(crate::two_step::KEY_FILE)).ok();
        Self::with_secret(id, secret.as_ref().map(|s| s.as_slice()))
    }

    fn with_secret(id: String, secret: Option<&[u8]>) -> Self {
        let root = secret.map(|secret| crate::two_step::derived_key(secret, b"pley server proofs"));
        Self { id, root }
    }

    /// The proof key of the sign-in `grant_id`, the same at every refresh.
    pub fn proof_key(&self, grant_id: i64) -> Option<ProofKey> {
        let key = self.grant_key(grant_id)?;
        Some(ProofKey { id: grant_id.to_string(), key: b64(&key) })
    }

    /// A new proof key for a session of pley's own sign-in, under a random id.
    pub fn session_key(&self) -> Option<ProofKey> {
        let mut bytes = [0u8; SESSION_KEY_ID_BYTES];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).ok()?;
        let id = b64(&bytes);
        let key = self.derive(&format!("session {id}"))?;
        Some(ProofKey { id: format!("s{id}"), key: b64(&key) })
    }

    /// The proof of `challenge` for the sign-in `proof_id`, asked at
    /// `address` (the issuer): `Ok(None)` when the server can't make one (its
    /// key can't be used), `Err` when the request is wrong.
    pub fn prove(&self, proof_id: &str, address: &str, challenge: &str) -> Result<Option<String>, &'static str> {
        let derived_from = match proof_id.strip_prefix('s') {
            Some(id) => {
                let canonical = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(id)
                    .is_ok_and(|b| b.len() == SESSION_KEY_ID_BYTES && b64(&b) == id);
                if !canonical {
                    return Err("proof_id isn't a sign-in's id");
                }
                format!("session {id}")
            }
            None => {
                let grant_id = proof_id
                    .parse::<i64>()
                    .ok()
                    .filter(|&id| id > 0 && id.to_string() == proof_id)
                    .ok_or("proof_id isn't a sign-in's id")?;
                format!("grant {grant_id}")
            }
        };
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(challenge)
            .ok()
            .filter(|b| CHALLENGE_BYTES.contains(&b.len()))
            .ok_or("challenge must be 16 to 64 bytes in base64url without padding")?;
        // A challenge decodes from one spelling only, so the proof covers
        // what the app sent.
        if b64(&bytes) != challenge {
            return Err("challenge must be 16 to 64 bytes in base64url without padding");
        }
        let Some(key) = self.derive(&derived_from) else { return Ok(None) };
        Ok(Some(proof(&key, &self.id, address, challenge)))
    }

    fn grant_key(&self, grant_id: i64) -> Option<[u8; 32]> {
        self.derive(&format!("grant {grant_id}"))
    }

    fn derive(&self, what: &str) -> Option<[u8; 32]> {
        let root = self.root.as_ref()?;
        hmac::sign(root, what.as_bytes()).as_ref().try_into().ok()
    }
}

/// Whether a client at `ip` is on this server's own networks, for a proof
/// of a plain-http address: private, loopback and link-local addresses; a
/// global IPv6 address in the /64 the server itself reaches it from (the
/// same link); and a Tailscale address (100.64.0.0/10) only when the server
/// reaches it from one of its own, as an internet provider's shared
/// addresses (the same range) are someone else's.
pub fn at_home(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v4) if crate::setup::is_shared_address(v4) => {
            matches!(own_address_towards(ip), Some(IpAddr::V4(own)) if crate::setup::is_shared_address(own))
        }
        IpAddr::V6(v6) if !crate::setup::is_local_address(ip) => {
            matches!(own_address_towards(ip), Some(IpAddr::V6(own)) if same_64(own, v6))
        }
        _ => crate::setup::is_local_address(ip),
    }
}

/// The address of this computer that a packet to `ip` would leave from:
/// a UDP socket "connected" there, which sends nothing.
fn own_address_towards(ip: IpAddr) -> Option<IpAddr> {
    let any: IpAddr = if ip.is_ipv4() { std::net::Ipv4Addr::UNSPECIFIED.into() } else { Ipv6Addr::UNSPECIFIED.into() };
    let socket = UdpSocket::bind((any, 0)).ok()?;
    socket.connect((ip, 9)).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

fn same_64(a: Ipv6Addr, b: Ipv6Addr) -> bool {
    a.segments()[..4] == b.segments()[..4]
}

/// HMAC-SHA256 under `key` of `pley-server-proof`, the server's id, the
/// address it was asked at and the challenge, each on a line of its own, in
/// base64url without padding.
pub fn proof(key: &[u8], server_id: &str, address: &str, challenge: &str) -> String {
    let tag = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, key),
        format!("{PROOF_CONTEXT}\n{server_id}\n{address}\n{challenge}").as_bytes(),
    );
    b64(tag.as_ref())
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef01234567";
    const CHALLENGE: &str = "AAECAwQFBgcICQoLDA0ODw";
    const AT: &str = "http://192.168.1.20:7539";

    #[test]
    fn the_id_is_made_once_and_kept_from_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let made = load_id(dir.path());
        assert_eq!(made.len(), 40);
        assert_eq!(load_id(dir.path()), made, "kept");

        let older = tempfile::tempdir().unwrap();
        std::fs::write(older.path().join(OLD_ID_FILE), format!("{ID}\n")).unwrap();
        assert_eq!(load_id(older.path()), ID, "an older version's id stays the server's");
        assert!(!older.path().join(OLD_ID_FILE).exists(), "renamed");
        assert_eq!(std::fs::read_to_string(older.path().join(ID_FILE)).unwrap().trim(), ID);
    }

    fn identity() -> Identity {
        Identity::with_secret(ID.into(), Some(&[7; 32]))
    }

    #[test]
    fn a_proof_is_the_hmac_an_app_computes_with_its_key() {
        let identity = identity();
        let given = identity.proof_key(17).unwrap();
        assert_eq!(given.id, "17");
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&given.key).unwrap();
        assert_eq!(key.len(), 32);
        let answered = identity.prove("17", AT, CHALLENGE).unwrap().unwrap();
        assert_eq!(answered, proof(&key, ID, AT, CHALLENGE));
        assert_eq!(answered.len(), 43, "32 bytes in base64url");
    }

    #[test]
    fn each_sign_in_has_its_own_key_which_stays_with_the_secret() {
        let identity = identity();
        assert_eq!(identity.proof_key(17), identity.proof_key(17), "the same at every refresh");
        assert_ne!(identity.proof_key(17).unwrap().key, identity.proof_key(18).unwrap().key);
        assert_ne!(identity.prove("17", AT, CHALLENGE), identity.prove("18", AT, CHALLENGE));
        let another = Identity::with_secret(ID.into(), Some(&[8; 32]));
        assert_ne!(identity.proof_key(17), another.proof_key(17), "another server's secret, another key");
    }

    #[test]
    fn a_proof_covers_the_server_and_the_challenge() {
        let key = [1u8; 32];
        let challenge2 = "AAECAwQFBgcICQoLDA0OEA";
        assert_ne!(proof(&key, ID, AT, CHALLENGE), proof(&key, ID, AT, challenge2));
        assert_ne!(proof(&key, ID, AT, CHALLENGE), proof(&key, "another", AT, CHALLENGE));
        assert_ne!(proof(&key, ID, AT, CHALLENGE), proof(&key, ID, "https://pley.example.com", CHALLENGE));
        // A known answer, for the apps' tests.
        assert_eq!(proof(&key, ID, AT, CHALLENGE), "YwkvaCQfaIQCZkoOP5yOE3yR4xRKU5T5wYWz4b85SCM");
    }

    #[test]
    fn a_session_key_has_a_random_id_its_proofs_go_by() {
        let identity = identity();
        let given = identity.session_key().unwrap();
        assert!(given.id.starts_with('s') && given.id.len() == 23, "{}", given.id);
        assert_ne!(identity.session_key().unwrap(), given, "a new one each time");
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&given.key).unwrap();
        assert_eq!(identity.prove(&given.id, AT, CHALLENGE).unwrap().unwrap(), proof(&key, ID, AT, CHALLENGE));
        assert_ne!(identity.prove(&given.id, AT, CHALLENGE), identity.prove("17", AT, CHALLENGE));
        assert_eq!(Identity::with_secret(ID.into(), None).session_key(), None);
    }

    #[test]
    fn the_home_network_is_where_the_server_is() {
        for home in ["192.168.1.30", "10.0.0.4", "127.0.0.1", "::1", "fd00::5", "fe80::1", "::ffff:192.168.1.30"] {
            assert!(at_home(home.parse().unwrap()), "{home}");
        }
        // Somewhere on the internet, and the internet's documentation ranges.
        for away in ["203.0.113.9", "8.8.8.8", "2001:db8::5", "::ffff:203.0.113.9"] {
            assert!(!at_home(away.parse().unwrap()), "{away}");
        }
        assert!(same_64("2001:db8:1:2::5".parse().unwrap(), "2001:db8:1:2:aaaa::9".parse().unwrap()));
        assert!(!same_64("2001:db8:1:2::5".parse().unwrap(), "2001:db8:1:3::5".parse().unwrap()));
    }

    #[test]
    fn requests_are_checked() {
        let identity = identity();
        for (proof_id, challenge) in [
            ("0", CHALLENGE),
            ("-3", CHALLENGE),
            ("017", CHALLENGE),
            ("x", CHALLENGE),
            ("s", CHALLENGE),
            ("sAAECAwQFBgcICQoLDA0ODw=", CHALLENGE),
            ("sAAECAwQFBgcICQoLDA0O", CHALLENGE),
            ("sAAECAwQFBgcICQoLDA0ODx", CHALLENGE),
            ("17", "short"),
            ("17", "AAECAwQFBgcICQoLDA0ODw=="),
            ("17", "AAECAwQFBgcICQoLDA0ODw+"),
            ("17", &"A".repeat(87)),
        ] {
            assert!(identity.prove(proof_id, AT, challenge).is_err(), "{proof_id} {challenge}");
        }
        assert!(identity.prove("17", AT, &"A".repeat(86)).unwrap().is_some(), "64 bytes");
        assert!(identity.prove("sAAECAwQFBgcICQoLDA0ODw", AT, CHALLENGE).unwrap().is_some());
    }

    #[test]
    fn without_its_key_the_server_proves_nothing() {
        let identity = Identity::with_secret(ID.into(), None);
        assert_eq!(identity.proof_key(17), None);
        assert_eq!(identity.prove("17", AT, CHALLENGE), Ok(None));
        assert!(identity.prove("x", AT, CHALLENGE).is_err(), "a wrong request is still wrong");
    }
}
