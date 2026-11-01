//! Pley for LG TV's Live TV: categories, the guide, channel cards, search,
//! programme details and the live player.

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    let _ = (route, cx);
    None
}
