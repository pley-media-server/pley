//! The cryptography of AirPlay's pairing (HomeKit's): SRP-6a with the
//! 3072-bit group of RFC 5054 and SHA-512 to pair with a PIN, Ed25519
//! keys that the server and the device keep and sign with, X25519 to agree
//! on a session's secret, HKDF-SHA512 for its keys, and
//! ChaCha20-Poly1305 for what is sent with them. All but SRP's numbers are
//! `ring`'s.

use num_bigint::BigUint;
use ring::{aead, hkdf};
use sha2::{Digest, Sha512};

/// RFC 5054's 3072-bit group; its generator is 5.
const N_HEX: &str = "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9A637ED6B0BFF5CB6F406B7EDEE386BFB5A899FA5AE9F24117C4B1FE649286651ECE45B3DC2007CB8A163BF0598DA48361C55D39A69163FA8FD24CF5F83655D23DCA3AD961C62F356208552BB9ED529077096966D670C354E4ABC9804F1746C08CA18217C32905E462E36CE3BE39E772C180E86039B2783A2EC07A28FB5C55DF06F4C52C9DE2BCBF6955817183995497CEA956AE515D2261898FA051015728E5A8AAAC42DAD33170D04507A33A85521ABDF1CBA64ECFB850458DBEF0A8AEA71575D060C7DB3970F85A6E1E4C7ABF5AE8CDB0933D71E8C94E04A25619DCEE3D2261AD2EE6BF12FFA06D98A0864D87602733EC86A64521F2B18177B200CBBE117577A615D6C770988C0BAD946E208E24FA074E5AB3143DB5BFCE0FD108E4B82D120A93AD2CAFFFFFFFFFFFFFFFF";
const GENERATOR: u8 = 5;

pub fn sha512(parts: &[&[u8]]) -> Vec<u8> {
    let mut hash = Sha512::new();
    for part in parts {
        hash.update(part);
    }
    hash.finalize().to_vec()
}

struct Len(usize);

impl hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// A 32-byte key from `secret`, for `salt` and `info` (HKDF-SHA512).
pub fn derive(secret: &[u8], salt: &str, info: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    hkdf::Salt::new(hkdf::HKDF_SHA512, salt.as_bytes())
        .extract(secret)
        .expand(&[info.as_bytes()], Len(32))
        .and_then(|okm| okm.fill(&mut out))
        .expect("32 bytes is a length HKDF-SHA512 gives");
    out
}

/// A nonce of a pairing message's name ("PS-Msg05"), padded with zeros
/// in front to 12 bytes.
pub fn named_nonce(name: &str) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[12 - name.len()..].copy_from_slice(name.as_bytes());
    nonce
}

/// The nonce of the `count`th frame of a channel: little-endian, after
/// four zeros.
pub fn counted_nonce(count: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&count.to_le_bytes());
    nonce
}

fn key(key: &[u8; 32]) -> aead::LessSafeKey {
    aead::LessSafeKey::new(aead::UnboundKey::new(&aead::CHACHA20_POLY1305, key).expect("a 32-byte key"))
}

/// `plain`, sealed with `k` (ChaCha20-Poly1305), its tag after it.
pub fn seal(k: &[u8; 32], nonce: [u8; 12], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let mut sealed = plain.to_vec();
    key(k)
        .seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(aad), &mut sealed)
        .expect("a message ChaCha20-Poly1305 can seal");
    sealed
}

/// What `sealed` holds, if `k` sealed it.
pub fn open(k: &[u8; 32], nonce: [u8; 12], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    let mut buffer = sealed.to_vec();
    let plain =
        key(k).open_in_place(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::from(aad), &mut buffer).ok()?;
    Some(plain.to_vec())
}

fn pad(value: &[u8], len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len.saturating_sub(value.len())];
    out.extend_from_slice(value);
    out
}

/// The pairing's SRP-6a, on the side of the one who types the PIN.
pub struct Srp {
    n: BigUint,
    g: BigUint,
    secret: BigUint,
    /// `A`, sent to the device.
    pub public: Vec<u8>,
}

/// What the PIN proved, if the device agrees: the proof sent, the one the
/// device must answer with, and the session key.
pub struct Proof {
    pub ours: Vec<u8>,
    pub theirs: Vec<u8>,
    pub key: Vec<u8>,
}

impl Srp {
    /// A new exchange, from 32 random bytes.
    pub fn new(random: [u8; 32]) -> Srp {
        let n = BigUint::parse_bytes(N_HEX.as_bytes(), 16).expect("the group's prime");
        let g = BigUint::from(GENERATOR);
        let secret = BigUint::from_bytes_be(&random);
        let public = g.modpow(&secret, &n).to_bytes_be();
        Srp { n, g, secret, public }
    }

    /// The proof of `pin` for the device's `salt` and public number;
    /// `None` when that number is no use (a multiple of N).
    pub fn prove(&self, user: &str, pin: &str, salt: &[u8], their_public: &[u8]) -> Option<Proof> {
        let n_bytes = self.n.to_bytes_be();
        let len = n_bytes.len();
        let b = BigUint::from_bytes_be(their_public);
        if (&b % &self.n) == BigUint::ZERO {
            return None;
        }
        let x = BigUint::from_bytes_be(&sha512(&[salt, &sha512(&[format!("{user}:{pin}").as_bytes()])]));
        let k = BigUint::from_bytes_be(&sha512(&[&n_bytes, &pad(&self.g.to_bytes_be(), len)]));
        let u = BigUint::from_bytes_be(&sha512(&[&pad(&self.public, len), &pad(their_public, len)]));
        let kgx = (k * self.g.modpow(&x, &self.n)) % &self.n;
        let base = (b + &self.n - kgx) % &self.n;
        let shared = base.modpow(&(&self.secret + u * x), &self.n);
        let key = sha512(&[&shared.to_bytes_be()]);
        let hn = sha512(&[&n_bytes]);
        let hg = sha512(&[&self.g.to_bytes_be()]);
        let xor: Vec<u8> = hn.iter().zip(&hg).map(|(a, b)| a ^ b).collect();
        let ours = sha512(&[&xor, &sha512(&[user.as_bytes()]), salt, &self.public, their_public, &key]);
        let theirs = sha512(&[&self.public, &ours, &key]);
        Some(Proof { ours, theirs, key })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The device's side of SRP-6a, as a fake device in tests plays it:
    /// its public number for `pin`, and its session key once it has `A`.
    pub(crate) struct Device {
        n: BigUint,
        v: BigUint,
        b: BigUint,
        pub public: Vec<u8>,
    }

    impl Device {
        pub(crate) fn new(pin: &str, salt: &[u8]) -> Device {
            let n = BigUint::parse_bytes(N_HEX.as_bytes(), 16).unwrap();
            let g = BigUint::from(GENERATOR);
            let len = n.to_bytes_be().len();
            let x = BigUint::from_bytes_be(&sha512(&[salt, &sha512(&[format!("Pair-Setup:{pin}").as_bytes()])]));
            let v = g.modpow(&x, &n);
            let k = BigUint::from_bytes_be(&sha512(&[&n.to_bytes_be(), &pad(&g.to_bytes_be(), len)]));
            let b = BigUint::from_bytes_be(&[0x42; 32]);
            let public = ((k * &v + g.modpow(&b, &n)) % &n).to_bytes_be();
            Device { n, v, b, public }
        }

        pub(crate) fn key(&self, a_public: &[u8]) -> Vec<u8> {
            let len = self.n.to_bytes_be().len();
            let a = BigUint::from_bytes_be(a_public);
            let u = BigUint::from_bytes_be(&sha512(&[&pad(a_public, len), &pad(&self.public, len)]));
            let shared = (a * self.v.modpow(&u, &self.n)).modpow(&self.b, &self.n);
            sha512(&[&shared.to_bytes_be()])
        }

        /// The proof a sender with the right PIN sends for `a_public`.
        pub(crate) fn expected(&self, salt: &[u8], a_public: &[u8]) -> Vec<u8> {
            let hn = sha512(&[&self.n.to_bytes_be()]);
            let hg = sha512(&[&[GENERATOR]]);
            let xor: Vec<u8> = hn.iter().zip(&hg).map(|(a, b)| a ^ b).collect();
            let key = self.key(a_public);
            sha512(&[&xor, &sha512(&[b"Pair-Setup"]), salt, a_public, &self.public, &key])
        }
    }

    #[test]
    fn the_right_pin_agrees_on_a_key_and_a_wrong_one_doesnt() {
        let salt = [9u8; 16];
        let device = Device::new("1234", &salt);
        let ours = Srp::new([7u8; 32]);
        let right = ours.prove("Pair-Setup", "1234", &salt, &device.public).unwrap();
        assert_eq!(right.key, device.key(&ours.public));
        let wrong = ours.prove("Pair-Setup", "1235", &salt, &device.public).unwrap();
        assert_ne!(wrong.key, device.key(&ours.public));
        assert!(ours.prove("Pair-Setup", "1234", &salt, &[0]).is_none());
    }

    #[test]
    fn what_is_sealed_opens_with_its_key_and_nonce_alone() {
        let k = derive(b"secret", "Control-Salt", "Control-Write-Encryption-Key");
        let sealed = seal(&k, counted_nonce(3), &[5, 0], b"hello");
        assert_eq!(open(&k, counted_nonce(3), &[5, 0], &sealed).as_deref(), Some(&b"hello"[..]));
        assert_eq!(open(&k, counted_nonce(4), &[5, 0], &sealed), None);
        assert_eq!(named_nonce("PV-Msg02"), *b"\0\0\0\0PV-Msg02");
    }
}
