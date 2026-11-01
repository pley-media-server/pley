//! Pley for LG TV's video player: the controls, the timeline and its previews, the
//! panels, Skip and Up Next, and the subtitles over the picture.

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    let _ = (route, cx);
    None
}
