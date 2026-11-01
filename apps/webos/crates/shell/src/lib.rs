//! Where Pley for LG TV's screens meet its data: the pages ([`Route`]), what a page is to the
//! shell ([`Screen`], [`Chrome`], [`AppEvent`]), what it works with ([`AppCx`]), the app's state
//! ([`AppState`], the port of `packages/tv/src/state.ts`), "saved first, fresh after" for a
//! screen's own answers ([`Resource`]), and the [`Shell`]: the back stack, Back, the rail, the
//! curtain, toasts, the offline notice and the idle dimmer.
//!
//! A screen crate exports a factory `fn(&Route, &mut AppCx) -> Option<Box<dyn Screen>>`; the app
//! adds each to the [`Registry`], and every route nobody draws yet gets a [`PlaceholderScreen`].
//!
//! Each frame the app calls [`Shell::filter_input`] on the input, [`Shell::update`] (the state's
//! answers, timers and events), then builds the [`pley_ui::Ui`] and calls [`Shell::frame`], and
//! after [`pley_ui::Ui::finish`] calls [`Shell::finish`], which carries out the navigation the
//! frame asked for.

mod cx;
mod load;
mod overlay;
mod players;
mod rail;
mod route;
mod screen;
mod shell;
mod state;

pub use cx::{AppCx, BackgroundSession, NavCommand, Requests, ToastRequest};
pub use load::{Load, Resource};
pub use overlay::{DIM_AFTER, DIM_ALPHA, ERROR_TOAST_SECONDS, TOAST_SECONDS};
pub use rail::{
    MAX_ITEMS, MAX_LIBRARIES, Rail, RailEntries, RailEntry, avatar_color, current_entry, initial, rail_entries,
};
pub use route::{LibraryTab, PlayQueue, PlayRequest, Route, SettingsSection, SmartPlaylist};
pub use screen::{AppEvent, Chrome, PLACEHOLDER_TEXT, PlaceholderScreen, Registry, Screen, ScreenFactory};
pub use shell::{Entry, Shell};
pub use state::{
    AppState, CurrentFetch, ITEM_TTL, LIBRARY_GATHER, OFFLINE_ASK, OFFLINE_NOTICE, OFFLINE_TEXT, REVALIDATE,
    STATE_GATHER, Scan,
};
