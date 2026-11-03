//! Pley for LG TV's requests, as the Samsung app has them (the TV app's `discover.tsx`,
//! `title.tsx`, `requests-page.tsx` and `request-kit.tsx`): Discover, TMDB's lists and titles
//! for a profile that may ask for them; a title's page with its one main action and the season
//! picker; and Your requests (Requests for managers), with what may be done to each.

mod actions;
mod common;
mod discover;
mod pages;
mod requests_page;
mod title;
mod titles;
mod widgets;

use pley_shell::{AppCx, Route, Screen};

pub use common::{TvClock, download_bar, stage_pill, title_badge, tone_color};
pub use discover::DiscoverScreen;
pub use requests_page::RequestsScreen;
pub use title::TitleScreen;
pub use titles::TitlesScreen;

/// The screen of `route` when it is one of this crate's pages. Making one asks nothing of the
/// server: that waits until it first draws.
pub fn screen(route: &Route, _cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        Route::Discover => Some(Box::new(DiscoverScreen::new())),
        Route::DiscoverList { list } => Some(Box::new(TitlesScreen::list(*list))),
        Route::DiscoverSearch { query } => Some(Box::new(TitlesScreen::search(query))),
        Route::Title { kind, tmdb_id, pick } => Some(Box::new(TitleScreen::new(*kind, *tmdb_id, *pick))),
        Route::Requests => Some(Box::new(RequestsScreen::new())),
        _ => None,
    }
}
