//! Jellyfin's ids: 32 hexadecimal characters, which clients read as GUIDs.
//! Pley's ids are numbers anyone could count through, so each Jellyfin id
//! carries a keyed check of its own: ids can't be guessed, and the pictures
//! clients load without a token (as Jellyfin lets them) show nothing to
//! someone who wasn't given the id.

use ring::hmac;

/// What an id names: it is part of the id, so one kind's id never passes
/// for another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// A movie, show, season, episode, artist, album, track or extra.
    Item = 1,
    /// A library, which Jellyfin calls a view.
    Library = 2,
    User = 3,
    Person = 4,
    /// A media file: a media source.
    File = 5,
    /// A genre, by its name ([`genre_number`]).
    Genre = 6,
    Collection = 7,
    Playlist = 8,
    /// A studio, by its name, as genres are.
    Studio = 9,
    /// A year, by its number.
    Year = 10,
    /// An entry of a playlist, which may hold an item more than once.
    PlaylistEntry = 11,
}

impl Kind {
    fn from_byte(byte: u8) -> Option<Kind> {
        Some(match byte {
            1 => Kind::Item,
            2 => Kind::Library,
            3 => Kind::User,
            4 => Kind::Person,
            5 => Kind::File,
            6 => Kind::Genre,
            7 => Kind::Collection,
            8 => Kind::Playlist,
            9 => Kind::Studio,
            10 => Kind::Year,
            11 => Kind::PlaylistEntry,
            _ => return None,
        })
    }
}

/// Numbers up to 2^56 fit an id.
const MAX: i64 = (1 << 56) - 1;

/// Makes and reads ids with the server's key.
pub struct Ids {
    key: hmac::Key,
}

impl Ids {
    /// Ids under a key derived from the server's `secret.key`.
    pub fn new(secret: &[u8]) -> Self {
        Self { key: crate::two_step::derived_key(secret, b"pley jellyfin ids") }
    }

    /// The id of `number` of `kind`, lowercase without dashes, as Jellyfin
    /// writes them.
    pub fn of(&self, kind: Kind, number: i64) -> String {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&(number.clamp(0, MAX) as u64).to_be_bytes());
        bytes[0] = kind as u8;
        let tag = hmac::sign(&self.key, &bytes[..8]);
        bytes[8..].copy_from_slice(&tag.as_ref()[..8]);
        hex::encode(bytes)
    }

    /// What an id names, when this server made it: with or without dashes,
    /// in either case.
    pub fn read(&self, text: &str) -> Option<(Kind, i64)> {
        let hex: String = text.trim().chars().filter(|c| *c != '-').collect();
        if hex.len() != 32 {
            return None;
        }
        let bytes = hex::decode(hex.to_ascii_lowercase()).ok()?;
        let tag = hmac::sign(&self.key, &bytes[..8]);
        // Compared in constant time, as the tag's bytes are a secret.
        let differs = tag.as_ref()[..8].iter().zip(&bytes[8..]).fold(0u8, |acc, (a, b)| acc | (a ^ b));
        if differs != 0 {
            return None;
        }
        let kind = Kind::from_byte(bytes[0])?;
        let mut number = [0u8; 8];
        number[1..].copy_from_slice(&bytes[1..8]);
        Some((kind, u64::from_be_bytes(number) as i64))
    }

    /// The number of an id of `kind`.
    pub fn number(&self, kind: Kind, text: &str) -> Option<i64> {
        self.read(text).filter(|(k, _)| *k == kind).map(|(_, n)| n)
    }

    /// A media source's id: the file and the token for other apps it was
    /// listed to, 64 hexadecimal characters. Players open a source's stream
    /// and subtitles with it and no token, so it stands for the token: it
    /// works while that token does, for what its user may see.
    pub fn source(&self, file_id: i64, token_id: i64) -> String {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&(file_id.clamp(0, MAX) as u64).to_be_bytes());
        bytes[0] = SOURCE;
        bytes[8..16].copy_from_slice(&(token_id.max(0) as u64).to_be_bytes());
        let tag = hmac::sign(&self.key, &bytes[..16]);
        bytes[16..].copy_from_slice(&tag.as_ref()[..16]);
        hex::encode(bytes)
    }

    /// The file and the token of a media source's id this server made.
    pub fn read_source(&self, text: &str) -> Option<(i64, i64)> {
        let hex: String = text.trim().chars().filter(|c| *c != '-').collect();
        if hex.len() != 64 {
            return None;
        }
        let bytes = hex::decode(hex.to_ascii_lowercase()).ok()?;
        if bytes[0] != SOURCE {
            return None;
        }
        let tag = hmac::sign(&self.key, &bytes[..16]);
        let differs = tag.as_ref()[..16].iter().zip(&bytes[16..]).fold(0u8, |acc, (a, b)| acc | (a ^ b));
        if differs != 0 {
            return None;
        }
        let mut file = [0u8; 8];
        file[1..].copy_from_slice(&bytes[1..8]);
        let token = u64::from_be_bytes(bytes[8..16].try_into().ok()?);
        Some((u64::from_be_bytes(file) as i64, token as i64))
    }
}

/// The first byte of a media source's id, which no [`Kind`] has.
const SOURCE: u8 = 0x80;

/// A genre's or a studio's number: its lowercase name hashed (FNV-1a) to 56 bits, so the
/// same name in any library has the same id. Pley keeps genres by name.
pub fn genre_number(name: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in name.trim().to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash & MAX as u64) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_read_back_and_checked() {
        let ids = Ids::new(&[7u8; 32]);
        let id = ids.of(Kind::Item, 42);
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(ids.read(&id), Some((Kind::Item, 42)));
        assert_eq!(ids.number(Kind::Item, &id.to_ascii_uppercase()), Some(42), "either case");
        let dashed = format!("{}-{}-{}-{}-{}", &id[..8], &id[8..12], &id[12..16], &id[16..20], &id[20..]);
        assert_eq!(ids.number(Kind::Item, &dashed), Some(42), "a GUID's dashes");

        assert_eq!(ids.number(Kind::Library, &id), None, "another kind's");
        assert_ne!(ids.of(Kind::Item, 43)[16..], id[16..], "each id its own check");
        let mut forged = id.clone().into_bytes();
        forged[15] = if forged[15] == b'0' { b'1' } else { b'0' };
        assert_eq!(ids.read(std::str::from_utf8(&forged).unwrap()), None, "a number changed");
        assert_eq!(Ids::new(&[8u8; 32]).read(&id), None, "another server's");
        assert_eq!(ids.read("not an id"), None);
        assert_eq!(ids.read(&"0".repeat(32)), None);
    }

    #[test]
    fn sources_name_a_file_and_a_token() {
        let ids = Ids::new(&[7u8; 32]);
        let source = ids.source(12, 34);
        assert_eq!(source.len(), 64);
        assert_eq!(ids.read_source(&source), Some((12, 34)));
        assert_eq!(ids.read_source(&source.to_ascii_uppercase()), Some((12, 34)));
        assert_eq!(ids.read(&source), None, "not an item's id");
        assert_eq!(ids.read_source(&ids.of(Kind::File, 12)), None, "not a source's");
        let mut other_token = hex::decode(&source).unwrap();
        other_token[15] ^= 1;
        assert_eq!(ids.read_source(&hex::encode(other_token)), None, "the token is checked");
        assert_eq!(Ids::new(&[8u8; 32]).read_source(&source), None);
    }

    #[test]
    fn genres_are_numbered_by_name() {
        assert_eq!(genre_number("Science Fiction"), genre_number(" science fiction "));
        assert_ne!(genre_number("Drama"), genre_number("Comedy"));
        assert!(genre_number("Drama") <= MAX);
    }
}
