//! `ratingKey`s. Plex clients address everything as
//! `/library/metadata/{ratingKey}`, collections and playlists too
//! (python-plexapi uploads a collection's poster there, and fetches an
//! object by its key there), so those get keys of their own, apart from the
//! items' ids: collection `n` is `1000000000 + n`, playlist `n` is
//! `2000000000 + n`. Items' ids stay far below (they count every item ever
//! added), and the keys fit in 32 bits for clients that read them so.

/// The first key of collections.
const COLLECTIONS: i64 = 1_000_000_000;
/// The first key of playlists.
const PLAYLISTS: i64 = 2_000_000_000;

/// What a `ratingKey` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Item(i64),
    Collection(i64),
    Playlist(i64),
}

impl Key {
    pub fn of(key: i64) -> Self {
        match key {
            PLAYLISTS.. => Key::Playlist(key - PLAYLISTS),
            COLLECTIONS.. => Key::Collection(key - COLLECTIONS),
            _ => Key::Item(key),
        }
    }
}

/// The `ratingKey` of collection `id`.
pub fn collection(id: i64) -> i64 {
    COLLECTIONS + id
}

/// The `ratingKey` of playlist `id`.
pub fn playlist(id: i64) -> i64 {
    PLAYLISTS + id
}

/// The collection a path that names collections means
/// (`/library/collections/{key}`): its key, or its id as this API gave it
/// before collections had keys of their own.
pub fn collection_id(key: i64) -> Option<i64> {
    match Key::of(key) {
        Key::Collection(id) | Key::Item(id) => Some(id),
        Key::Playlist(_) => None,
    }
}

/// The playlist a path that names playlists means (`/playlists/{key}`), as
/// [`collection_id`].
pub fn playlist_id(key: i64) -> Option<i64> {
    match Key::of(key) {
        Key::Playlist(id) | Key::Item(id) => Some(id),
        Key::Collection(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_of_their_own() {
        assert_eq!(Key::of(12), Key::Item(12));
        assert_eq!(Key::of(collection(12)), Key::Collection(12));
        assert_eq!(Key::of(playlist(12)), Key::Playlist(12));
        assert_eq!(collection_id(collection(3)), Some(3));
        assert_eq!(collection_id(3), Some(3), "the id as given before");
        assert_eq!(collection_id(playlist(3)), None);
        assert_eq!(playlist_id(playlist(3)), Some(3));
        assert_eq!(playlist_id(collection(3)), None);
        assert!(playlist(147_000_000) < i64::from(i32::MAX));
    }
}
