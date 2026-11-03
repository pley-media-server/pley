//! Pley for LG TV's Search (the Android TV app's
//! `TvSearchScreen.kt` and `SearchViewModel.kt`, the TV app's `search.tsx`): the field with the
//! kit's on-screen keyboard, the profile's recent searches, and what `GET /api/search` finds,
//! asked 250 ms after typing pauses and shown in rows by kind (movies, shows, seasons, episodes,
//! artists, albums, tracks, then Live TV channels, then people), with the loading, empty and
//! failed states.
//! For whoever may ask for titles, a Discover row follows: what TMDB has for the words that the
//! libraries lack, badged, with See all.
//!
//! - [`query`]: from what is typed to what shows (the debounce, the latest answer, Retry).
//! - [`recent`]: the profile's recent searches, kept off the UI thread.
//! - [`results`]: the answer sorted into rows, what each tile says, where a result opens; TMDB's
//!   titles as tiles.
//! - [`tmdb`]: the Discover row's words, asked and answered.
//! - [`screen`]: the page itself.

pub mod query;
pub mod recent;
pub mod results;
pub mod screen;
pub mod tmdb;

pub use screen::SearchScreen;

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages: Search, with the query it was
/// opened with.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        Route::Search { query } => Some(Box::new(SearchScreen::new(query, cx.now, cx.store().clone()))),
        _ => None,
    }
}
