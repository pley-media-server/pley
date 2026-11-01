//! Pley for LG TV's first screens: choosing a server, signing in with a code and choosing who is
//! watching, as the Android TV app has them (`apps/android/ui/tv/.../onboarding`).
//!
//! - [`ServersScreen`]: "Choose your server" (`Route::Servers`) and its address pad
//!   (`Route::AddressPad` opens it on the pad).
//! - [`LinkScreen`]: signing in with a code and a QR code (`Route::Link`).
//! - [`ProfilesScreen`]: "Who's watching?" (`Route::Profiles`), the PIN pad (`Route::Pin` opens it
//!   on a profile's pad, or does what picking a profile without a PIN does) and a profile
//!   approved on a phone.
//!
//! The shell's start gate decides which comes first; these screens move on with `cx.reset`.
//! "Always start as" is Settings' and the start gate's: "Who's watching?" keeps the household
//! they list.
//!
//! In the simulator, `--fixtures --route servers` (or `address`, `profiles`, `pin:2`) shows them
//! signed in to the fake server; the `fixture_server` example runs a fake server whose codes are
//! approved, declined or never answered, for the code screen (`--server <its address>`).

pub mod kit;
pub mod link;
pub mod profiles;
pub mod servers;
#[cfg(test)]
mod testing;

pub use link::{LinkFlow, LinkScreen, LinkStatus, Purpose};
pub use profiles::ProfilesScreen;
pub use servers::ServersScreen;

use pley_shell::{AppCx, Route, Screen};

/// The screen of `route` when it is one of this crate's pages.
pub fn screen(route: &Route, cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    Some(match route {
        Route::Servers => Box::new(ServersScreen::new(cx, false)),
        Route::AddressPad => Box::new(ServersScreen::new(cx, true)),
        Route::Link { server } => Box::new(LinkScreen::new(cx, server)),
        Route::Profiles => Box::new(ProfilesScreen::new(cx, None)),
        Route::Pin { profile } => Box::new(ProfilesScreen::new(cx, Some(*profile))),
        _ => return None,
    })
}
