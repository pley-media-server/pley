//! Waking the main loop from another thread.

use std::fmt;
use std::sync::Arc;

/// Makes the main loop run a frame's worth of work: called by worker threads
/// when a request finishes, a picture decodes or the player has news. Cheap to
/// clone and safe to call from any thread, any number of times.
#[derive(Clone)]
pub struct Waker(Arc<dyn Fn() + Send + Sync>);

impl Waker {
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Waker {
        Waker(Arc::new(f))
    }

    /// A waker that does nothing, for tests and headless use.
    pub fn noop() -> Waker {
        Waker::new(|| {})
    }

    pub fn wake(&self) {
        (self.0)()
    }
}

impl fmt::Debug for Waker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Waker")
    }
}
