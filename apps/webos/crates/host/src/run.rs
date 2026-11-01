//! The frame loop (DESIGN §4.1): wait for events (no longer than the earliest due time, not at all
//! while something animates), apply input, answers and timers, then build the display list,
//! render and present. An idle app waits without a timeout and uses no CPU.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use pley_base::time;
use pley_base::{DisplayList, Point, PointerEvent, TextShaper, Tier};
use pley_core::{Device, Http, Store, Tasks};
use pley_gfx::{DiskCacheConfig, ImageCache, ImageCacheConfig, Renderer};
use pley_platform::{Platform, PlatformEvent};
use pley_shell::{AppEvent, AppState, NavCommand, Route, Shell};
use pley_text::{FontSet, TextEngine};
use pley_ui::{FrameInput, FrameOutput, InputEvent, Ui, UiState};

use crate::App;
use crate::args::Args;
use crate::script;

/// The capability probe runs this long after the first frame (`PROBE_AFTER_MS`), so the first
/// Play has it and the start doesn't wait for it.
const PROBE_AFTER: f64 = 1.0;
/// A screenshot waits for the frames to settle this long after the script, at most this long.
const SETTLE: f64 = 0.8;
const SETTLE_MAX: f64 = 15.0;
/// How long leaving waits for the last playback reports to go out.
const EXIT_FLUSH: Duration = Duration::from_secs(1);

/// The folder the app's own files are in: `assets` beside the binary on the TV (and in a
/// package), the repository's `apps/webos/assets` while developing.
pub fn assets_dir() -> PathBuf {
    let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join("assets")));
    match beside {
        Some(dir) if cfg!(feature = "tv") || dir.join("fonts").is_dir() => dir,
        _ => repository_assets(),
    }
}

/// The repository's `apps/webos/assets`, for the simulator and the tests.
#[cfg(not(feature = "tv"))]
fn repository_assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// The TV's build never names this machine's folders (`cargo xtask check` fails on them): its
/// assets are beside the binary, else in the working folder.
#[cfg(feature = "tv")]
fn repository_assets() -> PathBuf {
    PathBuf::from("assets")
}

/// The GPU budget for pictures of a TV of `tier` (DESIGN §4.2).
fn gpu_budget(tier: Tier) -> usize {
    match tier {
        Tier::Low => ImageCacheConfig::GPU_LOW,
        Tier::Mid => ImageCacheConfig::GPU_MID,
        Tier::High => ImageCacheConfig::GPU_HIGH,
    }
}

/// Frame times for `--stats`.
#[derive(Debug, Default)]
struct Stats {
    started: f64,
    first_frame: Option<f64>,
    build: Vec<f64>,
    render: Vec<f64>,
}

impl Stats {
    fn report(&self) {
        let ms = |v: &[f64]| {
            if v.is_empty() {
                return "none".to_string();
            }
            let mut s = v.to_vec();
            s.sort_by(f64::total_cmp);
            let mean = s.iter().sum::<f64>() / s.len() as f64;
            format!("mean {mean:.2} ms, p95 {:.2} ms, max {:.2} ms", s[s.len() * 95 / 100], s[s.len() - 1])
        };
        match self.first_frame {
            Some(t) => println!("first frame after {:.0} ms", (t - self.started) * 1e3),
            None => println!("no frame drawn"),
        }
        println!("{} frames; display list: {}; render: {}", self.build.len(), ms(&self.build), ms(&self.render));
    }
}

/// Builds one frame: the shell draws the page and everything around it; the navigation it asked
/// for is carried out after. Whether the page on top changed.
#[allow(clippy::too_many_arguments)]
fn build(
    ui_state: &mut UiState,
    list: &mut DisplayList,
    shaper: &dyn TextShaper,
    images: &mut ImageCache,
    scale: f32,
    input: FrameInput,
    shell: &mut Shell,
    state: &mut AppState,
) -> (FrameOutput, bool) {
    let mut ui = Ui::new(ui_state, list, shaper, images, scale, &input);
    shell.frame(&mut ui, state);
    let out = ui.finish();
    let changed = shell.finish(state, ui_state, input.time);
    (out, changed)
}

/// The first page and, for `--route`, the page opened over it.
fn first_pages(state: &AppState, asked: Option<&str>) -> Result<(Route, Option<Route>), String> {
    let start = state.start_route();
    let Some(asked) = asked else { return Ok((start, None)) };
    let route = Route::parse(asked).ok_or_else(|| format!("no such page: {asked}"))?;
    // A page of the rail starts the stack; any other opens over the start (Back then works).
    Ok(if route.is_rail_page() || start != Route::Home { (route, None) } else { (start, Some(route)) })
}

/// Runs `app` on `platform` until it is left or closed.
#[allow(clippy::too_many_arguments)]
pub fn run(
    app: &App,
    mut platform: impl Platform,
    device: Arc<dyn Device>,
    store: Store,
    http: Http,
    args: &Args,
    started: f64,
    left: Arc<AtomicBool>,
) -> Result<(), String> {
    let waker = platform.waker();
    let tier = args.tier.unwrap_or_else(|| device.tier());
    let mut state = AppState::new(device.clone(), store, http, Tasks::new(waker.clone()));
    let mut ui_state = UiState::new();
    ui_state.set_tier(tier);
    log::debug!("state read");

    let fonts = assets_dir().join("fonts");
    let mut fallback = device.font_dirs();
    fallback.extend(pley_text::system_font_dirs());
    let engine = Arc::new(TextEngine::new(FontSet::load(&fonts, &fallback)));
    {
        // The TV's other faces are indexed off the main thread, before a title needs one.
        let engine = engine.clone();
        let _ =
            std::thread::Builder::new().name("pley-fonts".into()).spawn(move || engine.fonts().index_system_fonts());
    }
    let (w, h) = platform.surface_size();
    let mut renderer =
        Renderer::new(platform.gl(), engine.clone(), (w, h), platform.scale()).map_err(|e| format!("{e:?}"))?;
    let config = ImageCacheConfig {
        gpu_budget: gpu_budget(tier),
        disk: Some(DiskCacheConfig {
            dir: device.cache_dir().join("pictures"),
            budget: ImageCacheConfig::DISK_DEFAULT,
        }),
        ..ImageCacheConfig::default()
    };
    let mut images = ImageCache::new(state.picture_fetch(), waker.clone(), config);
    log::debug!("renderer ready");

    let (device_tx, device_rx) = mpsc::channel();
    device.subscribe(device_tx);

    let mut shell = Shell::new((app.screens)());
    let (first, over) = first_pages(&state, args.route.as_deref())?;
    shell.start(&mut state, &mut ui_state, first);
    if let Some(route) = over {
        shell.navigate(NavCommand::Push(route), &mut state, &mut ui_state);
    }

    let mut script = match &args.keys {
        Some(s) => Some(script::Player::new(script::parse(s)?, f64::INFINITY)),
        None => None,
    };
    let mut stats = Stats { started, ..Stats::default() };
    let mut list = DisplayList::default();
    let mut events = Vec::new();
    let mut wake_at: Option<f64> = Some(0.0);
    let mut first_frame: Option<f64> = None;
    let mut probed = false;
    let mut quiet_since: Option<f64> = None;
    let mut script_done_at: Option<f64> = None;

    loop {
        let now = time::now();
        let probe_due = first_frame.filter(|_| !probed).map(|t| t + PROBE_AFTER);
        let shooting = args.screenshot.is_some().then_some(now + 0.05);
        let due =
            [wake_at, state.next_due(), shell.next_due(), script.as_ref().and_then(|s| s.due()), probe_due, shooting]
                .into_iter()
                .flatten()
                .reduce(f64::min);
        let timeout = due.map(|d| Duration::from_secs_f64((d - now).clamp(0.0, 3600.0)));
        platform.wait_events(timeout, &mut events);

        let now = time::now();
        let scale = platform.scale();
        let mut input = Vec::new();
        let mut quit = false;
        for e in events.drain(..) {
            match e {
                PlatformEvent::Key(k) => input.push(InputEvent::Key(k)),
                PlatformEvent::Pointer(p) => input.push(InputEvent::Pointer(to_dp(p, scale))),
                PlatformEvent::Text(t) => input.push(InputEvent::Text(t)),
                PlatformEvent::Visible(v) => {
                    log::info!("the app is {}", if v { "shown" } else { "hidden" });
                    if !v {
                        device.keep_awake(false);
                    }
                    shell.visible(v, &mut state, &mut ui_state, now);
                }
                PlatformEvent::Resized(w, h) => renderer.resize(w, h, platform.scale()),
                PlatformEvent::Quit => quit = true,
            }
        }
        if quit || left.load(Ordering::SeqCst) {
            break;
        }
        while let Ok(e) = device_rx.try_recv() {
            shell.deliver(AppEvent::Device(e), &mut state, &mut ui_state, now);
        }
        if let Some(s) = script.as_mut() {
            input.extend(s.take(now));
            if s.done() && script_done_at.is_none() {
                script_done_at = Some(now);
            }
        }
        shell.filter_input(&mut input, now);
        shell.update(&mut state, &mut ui_state, now);
        if shell.exited() && left.load(Ordering::SeqCst) {
            break;
        }
        ui_state.set_reduced_motion(state.settings().reduce_motion || args.reduced_motion);
        if probe_due.is_some_and(|t| now >= t) {
            probed = true;
            state.probe_caps(false);
        }

        let waiting = renderer.prepare(&mut images);
        let t0 = Instant::now();
        let frame_input = FrameInput { time: now, events: input };
        let (mut out, mut changed) =
            build(&mut ui_state, &mut list, engine.as_ref(), &mut images, scale, frame_input, &mut shell, &mut state);
        // Focus settled after drawing, or a new page came on top: draw again before presenting, so
        // no frame shows without focus or with the page before.
        for _ in 0..2 {
            if !(out.redraw || changed) {
                break;
            }
            (out, changed) = build(
                &mut ui_state,
                &mut list,
                engine.as_ref(),
                &mut images,
                scale,
                FrameInput::at(now),
                &mut shell,
                &mut state,
            );
        }
        let built = t0.elapsed();
        let drawn = renderer.render(&list, &mut images);
        platform.set_pointer_visible(out.pointer_visible);
        if args.stats {
            stats.build.push(built.as_secs_f64() * 1e3);
            stats.render.push((drawn.build + drawn.submit).as_secs_f64() * 1e3);
        }
        wake_at = out.wake_at;
        if drawn.more || waiting || images.has_uploads() {
            wake_at = Some(now);
        }

        if let Some(path) = &args.screenshot {
            let ready = script.as_ref().is_none_or(|s| s.done()) && first_frame.is_some();
            let quiet = ready && wake_at.is_none_or(|t| t > now + 0.1) && !state.busy() && !drawn.more && !waiting;
            quiet_since = if quiet { quiet_since.or(Some(now)) } else { None };
            let since = script_done_at.or(first_frame).unwrap_or(now);
            if ready && (quiet_since.is_some_and(|t| now - t >= SETTLE) || now - since > SETTLE_MAX) {
                let frame = platform.read_frame();
                pley_gfx::screenshot::save_png(path, &frame).map_err(|e| format!("screenshot not written: {e}"))?;
                println!("wrote {}", path.display());
                break;
            }
        }
        platform.present();
        if first_frame.is_none() {
            first_frame = Some(time::now());
            stats.first_frame = first_frame;
            log::info!("first frame after {:.0} ms", (time::now() - started) * 1e3);
            // The script starts once the first page shows.
            if let Some(s) = script.as_mut() {
                s.start_at(time::now() + 0.3);
            }
        }
    }
    if args.stats {
        stats.report();
    }
    // The pages go first, so a player on the stack ends its playback; then the server hears of
    // every playback that ended before the app goes.
    drop(shell);
    state.end_playback();
    state.flush_reports(EXIT_FLUSH);
    Ok(())
}

/// The platform's pointer in device pixels, as the interface's dp.
fn to_dp(p: PointerEvent, scale: f32) -> PointerEvent {
    match p {
        PointerEvent::Moved(at) => PointerEvent::Moved(Point::new(at.x / scale, at.y / scale)),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::media::NullEngine;
    use pley_core::{FakeDevice, PlayerEngine};

    fn state() -> AppState {
        let device = FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>).shared();
        let store = Store::open(device.data_dir().join("store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        AppState::new(device, store, http, Tasks::new(pley_base::Waker::noop()))
    }

    #[test]
    fn a_page_asked_for_opens_over_the_start_unless_the_rail_opens_it() {
        let mut state = state();
        assert_eq!(first_pages(&state, None), Ok((Route::Servers, None)));
        // Without a sign-in, the page asked for is shown alone.
        assert_eq!(first_pages(&state, Some("item:12")), Ok((Route::Item { id: 12 }, None)));
        assert!(first_pages(&state, Some("nowhere")).is_err());
        state.pass_profile_gate();
        assert_eq!(first_pages(&state, Some("settings")), Ok((Route::Settings { section: None }, None)));
    }

    #[test]
    fn pictures_get_the_tier_s_budget() {
        assert_eq!(gpu_budget(Tier::Low), 64 << 20);
        assert_eq!(gpu_budget(Tier::High), 192 << 20);
        assert!(assets_dir().join("fonts").is_dir());
    }
}
