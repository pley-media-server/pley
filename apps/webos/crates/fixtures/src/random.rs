//! Random numbers for the catalogue: Kotlin's `Random(seed)` (Marsaglia's
//! xorwow, as kotlin.random.XorWowRandom draws it), so a seed gives the same
//! numbers as in the Android app's fixture server and both catalogues match.
//!
//! Also the fake server's tokens and session ids, which only need to differ.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

/// Kotlin's `kotlin.random.Random(seed: Int)`.
pub(crate) struct KotlinRandom {
    x: i32,
    y: i32,
    z: i32,
    w: i32,
    v: i32,
    addend: i32,
}

impl KotlinRandom {
    pub fn new(seed: i32) -> KotlinRandom {
        let (seed1, seed2) = (seed, seed >> 31);
        let mut random = KotlinRandom {
            x: seed1,
            y: seed2,
            z: 0,
            w: 0,
            v: !seed1,
            addend: (seed1 << 10) ^ ((seed2 as u32) >> 4) as i32,
        };
        // Kotlin discards the first 64 numbers: trivial seeds start with zeros.
        for _ in 0..64 {
            random.next_int();
        }
        random
    }

    pub fn next_int(&mut self) -> i32 {
        let mut t = self.x;
        t ^= ((t as u32) >> 2) as i32;
        self.x = self.y;
        self.y = self.z;
        self.z = self.w;
        let v0 = self.v;
        self.w = v0;
        t = (t ^ (t << 1)) ^ v0 ^ (v0 << 4);
        self.v = t;
        self.addend = self.addend.wrapping_add(362_437);
        t.wrapping_add(self.addend)
    }

    fn next_bits(&mut self, count: u32) -> i32 {
        let value = self.next_int();
        if count == 0 { 0 } else { ((value as u32) >> (32 - count)) as i32 }
    }

    fn next_long(&mut self) -> i64 {
        let high = (self.next_int() as i64) << 32;
        high.wrapping_add(self.next_int() as i64)
    }

    /// `nextInt(until)`: 0 to `until - 1`.
    pub fn below(&mut self, until: i32) -> i32 {
        assert!(until > 0);
        if until & until.wrapping_neg() == until {
            return self.next_bits(31 - until.leading_zeros());
        }
        loop {
            let bits = ((self.next_int() as u32) >> 1) as i32;
            let value = bits % until;
            if bits.wrapping_sub(value).wrapping_add(until - 1) >= 0 {
                return value;
            }
        }
    }

    /// `nextLong(until)`: 0 to `until - 1`.
    pub fn below_long(&mut self, until: i64) -> i64 {
        assert!(until > 0);
        if until & until.wrapping_neg() == until {
            let low = until as i32;
            let high = (until as u64 >> 32) as i32;
            return if low != 0 {
                (self.next_bits(31 - low.leading_zeros()) as i64) & 0xffff_ffff
            } else if high == 1 {
                (self.next_int() as i64) & 0xffff_ffff
            } else {
                ((self.next_bits(31 - high.leading_zeros()) as i64) << 32) + ((self.next_int() as i64) & 0xffff_ffff)
            };
        }
        loop {
            let bits = ((self.next_long() as u64) >> 1) as i64;
            let value = bits % until;
            if bits.wrapping_sub(value).wrapping_add(until - 1) >= 0 {
                return value;
            }
        }
    }
}

/// 64 bits that differ every call and every run (not for secrets: the fake
/// server's tokens and keys only need to be unique).
pub(crate) fn unique_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    if let Ok(since) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(since.as_nanos());
    }
    hasher.finish()
}

/// `n` random bytes as base64url without padding (32 bytes: 43 characters, as
/// the server's tokens and keys).
pub(crate) fn token(bytes: usize) -> String {
    let mut raw = Vec::with_capacity(bytes + 8);
    while raw.len() < bytes {
        raw.extend_from_slice(&unique_u64().to_le_bytes());
    }
    raw.truncate(bytes);
    base64url(&raw)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// `bytes` in base64url without padding.
pub(crate) fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 2);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | ((*b as u32) << (16 - 8 * i)));
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// The bytes of base64url without padding, when `text` is written so.
pub(crate) fn from_base64url(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= (ALPHABET.iter().position(|a| a == c)? as u32) << (18 - 6 * i);
        }
        out.extend((0..chunk.len() - 1).map(|i| (n >> (16 - 8 * i)) as u8));
    }
    Some(out)
}

/// 32 hexadecimal digits, as the server's session ids.
pub(crate) fn hex_id() -> String {
    format!("{:016x}{:016x}", unique_u64(), unique_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_have_the_servers_length() {
        assert_eq!(token(32).len(), 43);
        assert!(token(32).bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert_ne!(token(32), token(32));
        assert_eq!(hex_id().len(), 32);
        let bytes: Vec<u8> = (0..=255).collect();
        for n in 0..bytes.len() {
            assert_eq!(from_base64url(&base64url(&bytes[..n])).as_deref(), Some(&bytes[..n]));
        }
        assert_eq!(from_base64url("AA=="), None);
    }

    #[test]
    fn bounded_draws_stay_in_bounds() {
        let mut random = KotlinRandom::new(7539);
        for until in [1, 2, 3, 4, 36, 77, 90, 300] {
            for _ in 0..100 {
                let n = random.below(until);
                assert!((0..until).contains(&n));
            }
        }
        for until in [10_800, 18_000, 1 << 20, 1 << 33] {
            for _ in 0..100 {
                let n = random.below_long(until);
                assert!((0..until).contains(&n));
            }
        }
    }

    #[test]
    fn the_same_seed_draws_the_same() {
        let a: Vec<i32> = {
            let mut r = KotlinRandom::new(7539);
            (0..8).map(|_| r.below(77)).collect()
        };
        let b: Vec<i32> = {
            let mut r = KotlinRandom::new(7539);
            (0..8).map(|_| r.below(77)).collect()
        };
        assert_eq!(a, b);
    }
}
