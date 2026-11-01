//! Playback for Pley for LG TV: the controller that asks the server what to play
//! and drives the TV's engine, with quality, Automatic, recovery, reports, Up Next,
//! markers, the sleep timer, music and Live TV sessions.
//!
//! Everything here runs on the main thread and never blocks: server calls go
//! to the task pool (`pley_core::task::Tasks`) through [`server::PlayerServer`]
//! (answered by `pley_core::Api`), reports through the [`reporter::Reporter`]'s
//! own thread, and the owner calls `tick(now)` each frame with the app's clock
//! in ms ([`now_ms`]), wakes at `next_wake()`, and reads a snapshot.
//!
//! - [`PlaybackController`]: one video, then what plays after it
//!   ([`controller::PlayerState`], [`controller::PlayerClock`], the subtitles to draw).
//! - [`MusicSession`]: the app's music queue, playing behind every page.
//! - [`LiveSession`]: a live channel; [`live`] has what the Live TV screens need beside it.
//! - [`SimEngine`]: the engine of the simulator and the tests.
//! - [`seeker::Seeker`]: the remote's seeking, for the player screen.

pub mod autoquality;
pub mod controller;
pub mod details;
pub mod env;
pub mod live;
pub mod markers;
pub mod music;
pub mod network;
pub mod nextup;
pub mod plan;
pub mod prepare;
pub mod quality;
pub mod recovery;
pub mod reporter;
pub mod seeker;
pub mod server;
pub mod sim;
pub mod sleep;
pub mod speed;

pub use controller::{Phase, PlaybackController, PlayerClock, PlayerState, SavedPlayback};
pub use env::PlayerEnv;
pub use live::{LiveSession, LiveState};
pub use music::{MusicSession, NowPlaying};
pub use plan::{PlayRequest, Queue};
pub use reporter::Reporter;
pub use server::PlayerServer;
pub use sim::{SimClock, SimEngine, SimHandle};

/// The app's clock in ms, as the players' `tick` takes it (pley-base's clock).
pub fn now_ms() -> i64 {
    (pley_base::time::now() * 1000.0) as i64
}
