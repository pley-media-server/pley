//! Pley for LG TV's requests, as the Samsung app has them (the TV app's `discover.tsx`,
//! `title.tsx`, `requests-page.tsx` and `request-kit.tsx`): Discover, TMDB's lists and titles
//! for a profile that may ask for them; a title's page with its one main action and the season
//! picker; and Your requests (Requests for managers), with what may be done to each.

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages. Making one asks nothing of the
/// server: that waits until it first draws.
pub fn screen(route: &Route, _cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        _ => None,
    }
}
