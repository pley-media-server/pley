//! What runs Pley for LG TV: the frame loop, the start on the TV and in the simulator, and the
//! simulator's command line. The `pley` binary (pley-lg) runs it with every screen; each screen
//! crate's `sim` example runs it with that crate's screens alone.
//!
//! On a computer (the default `desktop` feature) it opens a window and plays the TV's part with a
//! fake device; `--fixtures` starts the fake Pley server and signs in to it. On the TV (the `tv`
//! feature) the window, the device and the media pipeline are `pley-webos`'s.

mod args;
mod logger;
mod run;
mod script;
#[cfg(feature = "desktop")]
mod sim;
#[cfg(feature = "tv")]
mod tv;

use pley_shell::Registry;

pub use run::assets_dir;

/// What a binary runs.
#[derive(Clone, Copy)]
pub struct App {
    /// Its version, as the TV, the logs and the server are told it.
    pub version: &'static str,
    /// The registry of every screen it has.
    pub screens: fn() -> Registry,
}

/// Runs `app` as the command line asks until it is left; on an error, says so and exits.
pub fn main(app: App) {
    // The app's clock starts here: the time to the first frame counts from it.
    let started = pley_base::time::now();
    let args = match args::parse(std::env::args().skip(1)) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{}", args::USAGE);
            return;
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    #[cfg(feature = "tv")]
    let result = tv::main(&app, &args, started);
    #[cfg(all(feature = "desktop", not(feature = "tv")))]
    let result = desktop::main(&app, &args, started);
    #[cfg(not(any(feature = "tv", feature = "desktop")))]
    let result: Result<(), String> = Err(format!("built without a platform ({} {args:?}, {started})", app.version));
    if let Err(e) = result {
        log::error!("{e}");
        eprintln!("{e}");
        std::process::exit(1);
    }
}

#[cfg(all(feature = "desktop", not(feature = "tv")))]
mod desktop {
    use std::path::PathBuf;
    use std::sync::Arc;

    use pley_core::addresses::origin;
    use pley_core::{Device, Http, Store};
    use pley_platform::Platform;
    use pley_platform::desktop::{DesktopOptions, DesktopPlatform};

    use crate::App;
    use crate::args::{Args, DriveArg};
    use crate::sim::{SimDevice, sign_in_to_fixtures};
    use crate::{logger, run};

    /// The simulator: a window, a fake TV keeping its data under `--data`, and the server asked for.
    pub fn main(app: &App, args: &Args, started: f64) -> Result<(), String> {
        // With the fake server, a fresh folder each time: its address changes from run to run.
        let root = match (&args.data, args.fixtures) {
            (Some(dir), _) => dir.clone(),
            (None, true) => pley_core::device::scratch_dir("lg-sim"),
            (None, false) => std::env::temp_dir().join("pley-lg-sim"),
        };
        logger::init(Some(&root.join("logs")), args.verbose);
        log::info!("Pley for LG TV {} (simulator), data in {}", app.version, root.display());
        let drive = match &args.drive {
            Some(DriveArg::Folder(dir)) if dir.is_dir() => Some(dir.clone()),
            Some(DriveArg::Folder(dir)) => return Err(format!("--drive: {} isn't a folder", dir.display())),
            Some(DriveArg::Fixtures) => {
                let dir = root.join("drive");
                pley_fixtures::write_sample_drive(&dir).map_err(|e| format!("the sample drive wasn't made: {e}"))?;
                Some(dir)
            }
            None => None,
        };
        let device = Arc::new(SimDevice::new(root, args.tier, app.version));
        if let Some(locale) = &args.locale {
            device.set_languages(&[locale.as_str()]);
        }
        if let Some(dir) = drive {
            log::info!("the drive is {}", dir.display());
            device.set_drive_folder(dir);
        }
        let info = device.info();
        let store = open_store(device.data_dir().join("store"), info.data_durable)?;
        let http = Http::network(&info.user_agent);
        let _fixtures = if args.fixtures { Some(sign_in_to_fixtures(&store, &http)?) } else { None };
        if let Some(url) = &args.server {
            let server = origin(url).ok_or_else(|| format!("not a server address: {url}"))?;
            store.set_active_server(Some(&server)).map_err(|e| e.to_string())?;
        }
        let (width, height) = args.size.unwrap_or((1280, 720));
        let options = DesktopOptions { width, height, ..DesktopOptions::default() };
        let platform = DesktopPlatform::new(&options).map_err(|e| format!("no window: {e:?}"))?;
        log::debug!("window open");
        device.set_waker(platform.waker());
        let left = device.left();
        let plugged = device.clone();
        run::run(app, platform, device, store, http, args, started, left, &move |on| plugged.plug(on))
    }

    fn open_store(dir: PathBuf, durable: bool) -> Result<Store, String> {
        let store = if durable { Store::open(&dir) } else { Store::open_volatile(&dir) };
        store.map_err(|e| format!("the data folder {} can't be used: {e}", dir.display()))
    }
}
