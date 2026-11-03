//! Pley for LG TV's page for a title (film, show, season, episode), its actions and a person's
//! page, as the Android TV app draws them (`TvItemScreen`, `TvItemActionsScreen`,
//! `TvPersonScreen`).

mod actions;
mod common;
mod item;
mod page;
mod person;
mod request;
mod words;

use pley_shell::{AppCx, Overlay, Route, Screen};

pub use actions::{ItemActionsScreen, TileActions};
pub use item::ItemScreen;
pub use person::PersonScreen;

/// The screen of `route` when it is one of this crate's pages: a title's page, a title's actions
/// (long OK or the menu key on a tile elsewhere) and a person's page.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        Route::Item { id } => Some(Box::new(ItemScreen::new(*id, cx))),
        Route::ItemActions { id, continue_watching } => {
            Some(Box::new(ItemActionsScreen::new(*id, *continue_watching, cx)))
        }
        Route::Person { id } => Some(Box::new(PersonScreen::new(*id))),
        _ => None,
    }
}

/// A tile's actions in a side panel over the page (OK held, or the menu key, on a tile anywhere):
/// the shell's [`pley_shell::Registry::tile_actions`].
pub fn tile_actions(id: i64, continue_watching: bool, cx: &mut AppCx) -> Box<dyn Overlay> {
    Box::new(TileActions::new(id, continue_watching, cx))
}
