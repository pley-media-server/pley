//! Pley for LG TV's first screens: choosing a server, signing in with a code and
//! choosing who is watching.

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    let _ = (route, cx);
    None
}
