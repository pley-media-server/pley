//! One clock for the whole app: seconds since it started, monotonic.
//!
//! Animations, rest timers, picture fade-ins and the player's clock all read
//! the same time base, so values computed on different threads compare.

use std::sync::OnceLock;
use std::time::Instant;

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// Seconds since the app started.
pub fn now() -> f64 {
    start().elapsed().as_secs_f64()
}

/// The [`now`] value of an [`Instant`].
pub fn at(instant: Instant) -> f64 {
    instant.saturating_duration_since(start()).as_secs_f64()
}
