//! Pley for LG TV's Home and its list of libraries.
//!
//! - [`HomeScreen`] ([`Route::Home`]): the hero for the first title of Continue Watching, the
//!   server's rows under it, the info block and backdrop of the tile focus rests on, and Home's
//!   loading, empty, failed and offline states (Android's `TvHome.kt`).
//! - [`LibrariesScreen`] ([`Route::Libraries`]): every library as a card, three to a row, and the
//!   profile's own order of them (Android's `TvLibraries.kt`).
//!
//! Both draw at once from what was saved and follow the server's answers ("saved first, fresh
//! after": the app's state holds Home's rows and the libraries). Other pages are opened only
//! through routes: a title's page and actions, a library, the player.
//!
//! The simulator shows them against the fake server (`cargo run -p pley-home --example home_sim --
//! --fixtures --route home`), and Home's states with the server slow, away or empty
//! (`cargo run -p pley-home --example home_states -- loading|failed|offline|empty|scanning|no-libraries`).

pub mod ahead;
pub mod home;
pub mod libraries;
pub mod model;

pub use home::HomeScreen;
pub use libraries::LibrariesScreen;
pub use model::HomeLook;

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages: Home and the libraries list.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    let _ = cx;
    match route {
        Route::Home => Some(Box::new(HomeScreen::new())),
        Route::Libraries { sort } => Some(Box::new(LibrariesScreen::new(*sort))),
        _ => None,
    }
}
