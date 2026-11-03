//! Pley for LG TV's Live TV, as the Android TV app has it:
//! the page of categories beside the guide or the channels as cards, with search, a programme's
//! details and favourites ([`LiveTvScreen`]), and the live player with its channel list beside the
//! picture ([`LivePlayerScreen`]).

mod cards;
mod categories;
mod channel_list;
mod clock;
mod details;
mod draw;
mod guide;
mod kept;
mod lineup;
mod page;
mod player;

use pley_shell::{AppCx, Route, Screen};

pub use clock::{Clock, utc_offset};
pub use guide::{Cursor, Guide, guide_start, left_for, slot};
pub use kept::View;
pub use page::LiveTvScreen;
pub use player::{Digits, LivePlayerScreen, Overlay, picture_aside};

/// The screen of `route` when it is one of this crate's pages. Making one asks nothing of the
/// server: that waits until it first draws.
pub fn screen(route: &Route, _cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        Route::LiveTv => Some(Box::new(LiveTvScreen::new())),
        Route::LivePlayer { channel_id, category, pressed_at } => {
            Some(Box::new(LivePlayerScreen::new(*channel_id, category.clone(), *pressed_at)))
        }
        _ => None,
    }
}
