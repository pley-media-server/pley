//! Pley for LG TV's page for a title (film, show, season, episode), its actions and
//! a person's page.

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    let _ = (route, cx);
    None
}
