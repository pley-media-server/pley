//! The LG TV under Pley for LG TV: the SDL2 window from the firmware, luna-service2,
//! the facts the TV tells about itself, the capability report, and the player
//! engine on LG's media pipeline. Every firmware library is opened at run time.
//!
//! - [`sdl::WebosPlatform`]: the window, GLES2 context, remote and lifecycle
//!   (DESIGN.md §5.1, §5.2, §5.2b).
//! - [`luna::Luna`]: the luna thread and its handle (§5.5).
//! - [`facts::WebosFacts`] and [`caps::webos_caps`]: what the TV is and plays (§5.4).
//! - [`device::WebosDevice`]: pley-core's `Device` for the TV, with the data
//!   folder it probes (§5.7).
//! - [`ums::UmsEngine`]: pley-core's `PlayerEngine` on uMediaServer (§5.3).
//! - [`log`] and [`launch`]: the TV's log file and the launch payload.

pub mod caps;
pub mod data;
pub mod device;
mod dl;
pub mod facts;
pub mod launch;
pub mod log;
pub mod luna;
pub mod sdl;
pub mod ums;

pub use caps::webos_caps;
pub use device::WebosDevice;
pub use facts::WebosFacts;
pub use launch::Launch;
pub use luna::Luna;
pub use sdl::WebosPlatform;
pub use ums::UmsEngine;

/// The app's id on the TV (`appinfo.json`), the name its data and logs go by.
pub const APP_ID: &str = "app.pley.tv";
