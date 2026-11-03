//! Pley for LG TV's Settings, watch history, Diagnostics and About, as the Android TV app has them
//! (`TvSettingsScreen.kt`, `TvHistoryScreen.kt`, `TvDiagnosticsScreen.kt`), with what LG adds:
//! "Delete all local data" (a native app's data outlives it), the
//! network and the log on Diagnostics, and the licence texts of NOTICES.txt.
//!
//! - [`SettingsScreen`]: the categories and their rows, the side panels of choices, the questions
//!   before signing out or deleting, and "Manage your account on the web".
//! - [`HistoryScreen`]: the profile's plays, 15 at a time, each removable.
//! - [`DiagnosticsScreen`]: the TV's facts, what it plays, the network, the log's last lines.
//! - [`LicencesScreen`]: the open-source parts and their licence texts.
//!
//! What each pane holds is data ([`pane`]), so it is tested without drawing.

pub mod diagnostics;
pub mod history;
pub mod licences;
pub mod notices;
pub mod pane;
pub mod settings;
pub mod web;

pub use diagnostics::DiagnosticsScreen;
pub use history::HistoryScreen;
pub use licences::LicencesScreen;
pub use settings::SettingsScreen;

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages.
pub fn screen(route: &Route, _cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    Some(match route {
        Route::Settings { section } => Box::new(SettingsScreen::new(*section)),
        Route::History => Box::new(HistoryScreen::new()),
        Route::Diagnostics => Box::new(DiagnosticsScreen::new()),
        Route::Licences => Box::new(LicencesScreen::new()),
        _ => return None,
    })
}
