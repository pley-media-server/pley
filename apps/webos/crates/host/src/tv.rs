//! The app on the TV: `pley-webos`'s window over the firmware's SDL2, its device (facts, data
//! folder, capability report, media pipeline) and its log file.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use pley_core::{Device, Http, Store};
use pley_platform::Platform;
use pley_webos::{WebosDevice, WebosPlatform};

use crate::App;
use crate::args::Args;
use crate::{logger, run};

/// Runs the app on the TV until the TV closes it.
pub fn main(app: &App, args: &Args, started: f64) -> Result<(), String> {
    // SAFETY: the first thing the app does, before any other thread exists: the platform sets
    // environment variables LG's SDL reads.
    let platform = unsafe { WebosPlatform::new() }.map_err(|e| format!("no window: {}", e.0))?;
    if let Err(e) = pley_webos::log::start(Path::new(pley_webos::log::LOG_DIR)) {
        eprintln!("no log file: {e}");
    }
    // Lines go to stderr, which the TV's log captures.
    logger::init(None, args.verbose);
    log::info!("Pley for LG TV {}", app.version);
    let luna = pley_webos::Luna::start();
    let device: Arc<dyn Device> = Arc::new(WebosDevice::new(luna, platform.waker(), app.version));
    let info = device.info();
    let dir = device.data_dir().join("store");
    let store = if info.data_durable { Store::open(&dir) } else { Store::open_volatile(&dir) }
        .map_err(|e| format!("the data folder can't be used: {e}"))?;
    let http = Http::network(&info.user_agent);
    // The TV leaves the app itself (Back on the first page asks it to); the loop ends on SDL's quit.
    run::run(app, platform, device, store, http, args, started, Arc::new(AtomicBool::new(false)))
}
