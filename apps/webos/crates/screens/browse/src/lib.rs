//! Pley for LG TV's library pages, collections and playlists: a library with its Recommended
//! rows, its whole listing as a grid (sort, filters, the A–Z jump) and its collections; the
//! Collections page and a collection's; Playlists, a playlist's page with its sheets (rename,
//! remove, move, delete, shuffle), New playlist (and Save these filters) and Add to playlist.
//! Each follows the Android TV app's screen of the same name on the 960 × 540 canvas.

mod alpha;
mod collections;
mod library;
mod listing;
mod parts;
mod playlist;
mod playlists;
mod sheets;
mod tiles;
mod view;

pub use alpha::{AlphaIndex, JUMP_FROM};
pub use collections::{CollectionScreen, CollectionsScreen};
pub use library::LibraryScreen;
pub use listing::{HEAD, KEPT_PAGES, PAGE, Pager, WINDOW_ABOVE, WINDOW_ABOVE_LOW};
pub use parts::TITLE_MAX;
pub use playlist::{Lift, PlaylistScreen, entry_titles};
pub use playlists::{PlaylistsScreen, playlist_meta};
pub use sheets::{AddToPlaylistScreen, NewPlaylistScreen, added_message, kind_of};
pub use tiles::{ArtKind, Cell, art_paths, page_of};
pub use view::{count_label, describe_smart_filter, smart_filter_of};

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    let _ = cx;
    Some(match route {
        Route::Library { id, tab, sort } => Box::new(LibraryScreen::new(*id, *tab, *sort)),
        Route::Collections { library_id } => Box::new(CollectionsScreen::new(*library_id)),
        Route::Collection { id } => Box::new(CollectionScreen::new(*id)),
        Route::Playlists => Box::new(PlaylistsScreen::new()),
        Route::Playlist { id } => Box::new(PlaylistScreen::new(*id)),
        Route::AddToPlaylist { item_ids, title } => Box::new(AddToPlaylistScreen::new(item_ids.clone(), title.clone())),
        Route::NewPlaylist { item_ids, smart } => Box::new(NewPlaylistScreen::new(item_ids.clone(), smart.clone())),
        _ => return None,
    })
}
