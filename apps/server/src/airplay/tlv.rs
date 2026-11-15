//! TLV8, the format of AirPlay's pairing messages (from HomeKit): a type
//! byte, a length byte and up to 255 bytes of value; a longer value goes in
//! several items of the same type, one after the other.

pub const METHOD: u8 = 0;
pub const IDENTIFIER: u8 = 1;
pub const SALT: u8 = 2;
pub const PUBLIC_KEY: u8 = 3;
pub const PROOF: u8 = 4;
pub const ENCRYPTED: u8 = 5;
pub const STATE: u8 = 6;
pub const ERROR: u8 = 7;
pub const RETRY_DELAY: u8 = 8;
pub const SIGNATURE: u8 = 10;
pub const FLAGS: u8 = 19;

/// The error a device answers a pairing step with when it wants a PIN, or
/// the PIN was wrong (`Authentication`), or it waits before another try.
pub const ERROR_AUTHENTICATION: u8 = 2;
pub const ERROR_BACKOFF: u8 = 3;

/// The longest message read, in bytes: a device's public key, salt, proof
/// and signature take under 1 KiB.
pub const MAX: usize = 16 * 1024;

pub fn write(items: &[(u8, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (kind, value) in items {
        if value.is_empty() {
            out.extend([*kind, 0]);
        }
        for chunk in value.chunks(255) {
            out.push(*kind);
            out.push(chunk.len() as u8);
            out.extend_from_slice(chunk);
        }
    }
    out
}

/// The items of `data`, a value cut into several items joined again. What
/// a truncated item holds is dropped.
pub fn read(data: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut at = 0;
    let mut last: Option<(u8, usize)> = None;
    while at + 2 <= data.len().min(MAX) {
        let (kind, len) = (data[at], usize::from(data[at + 1]));
        let Some(value) = data.get(at + 2..at + 2 + len) else { break };
        match (last, out.last_mut()) {
            (Some((was, 255)), Some((_, joined))) if was == kind => joined.extend_from_slice(value),
            _ => out.push((kind, value.to_vec())),
        }
        last = Some((kind, len));
        at += 2 + len;
    }
    out
}

pub fn get(items: &[(u8, Vec<u8>)], kind: u8) -> Option<&[u8]> {
    items.iter().find(|(k, _)| *k == kind).map(|(_, v)| v.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_value_goes_in_several_items_and_comes_back_whole() {
        let key = vec![7u8; 384];
        let data = write(&[(STATE, &[1]), (PUBLIC_KEY, &key), (PROOF, &[])]);
        assert_eq!(data.len(), 3 + (2 + 255) + (2 + 129) + 2);
        let items = read(&data);
        assert_eq!(get(&items, PUBLIC_KEY), Some(key.as_slice()));
        assert_eq!((get(&items, STATE), get(&items, PROOF)), (Some(&[1u8][..]), Some(&[][..])));
        // Cut short: what is whole stays.
        assert_eq!(read(&data[..10]).len(), 1);
    }
}
