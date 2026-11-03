//! Pley for LG TV's music pages: an artist, an album and Now Playing (the Android TV app's
//! `TvArtistScreen`, `TvAlbumScreen` and `TvNowPlayingScreen`), and the music player that plays on
//! while people browse ([`MusicPlayer`], held by the shell as its background session, so the rail
//! shows Now Playing while music has a queue).
//!
//! Pages open each other only through routes: an album's tiles open [`Route::Album`], its artist
//! link [`Route::Artist`], Play and Shuffle [`Route::NowPlaying`]; a tile's actions (long OK or
//! the menu key) open over the page ([`pley_shell::AppCx::tile_actions`]).

mod album;
mod artist;
mod now_playing;
mod pages;
mod player;
mod widgets;

pub use album::AlbumScreen;
pub use artist::ArtistScreen;
pub use now_playing::NowPlayingScreen;
pub use player::{Gathered, MusicPlayer, current_track, gather, gathered, music, play};

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages: an artist, an album, Now Playing.
pub fn screen(route: &Route, _cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        Route::Artist { id } => Some(Box::new(ArtistScreen::new(*id))),
        Route::Album { id, track_id } => Some(Box::new(AlbumScreen::new(*id, *track_id))),
        Route::NowPlaying => Some(Box::new(NowPlayingScreen::new())),
        _ => None,
    }
}
