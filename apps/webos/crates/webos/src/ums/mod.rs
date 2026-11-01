//! The player engine on LG's own media pipeline: uMediaServer
//! (`luna://com.webos.media`) in URI mode, as the web app's `<video>` used it.
//! The TV fetches, demuxes and decodes the keyed address itself, with its
//! hardware decoders, HDR, Dolby Vision and sound output; the app sends
//! commands and listens.
//!
//! [`machine::Machine`] holds every rule and is tested on scripted messages;
//! [`UmsEngine`] runs its calls on the luna thread, makes and places the
//! exported window (on the controller's thread, which is the window's), and
//! hands the controller its events. The luna client that loaded a pipeline
//! owns it: uMediaServer unloads it when that client goes, so the app's one
//! registration plays everything.

pub mod acb;
pub mod machine;
pub mod payload;
pub mod plane;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pley_base::{Rect, Waker};
use pley_core::media::{EngineEvent, EngineStats, LoadRequest, PlayerEngine};
use pley_platform::VideoWindow;
use serde_json::{Value, json};

use crate::data::write_atomic;
use crate::facts::CodecTable;
use crate::luna::{Luna, LunaError, Subscription};
use crate::sdl::ExportedWindow;
use acb::Acb;
use machine::{Action, Config, Failure, Input, Machine, Method, PlaneKind};
use plane::{Plane, PlaneMemory};

/// How long a command's answer may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the subscription's first message may take.
const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the 4.x binding's check may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// What the engine is set up with.
#[derive(Clone, Debug)]
pub struct UmsConfig {
    /// The id SAM launched the app with (`option.appId`, ACB's).
    pub app_id: String,
    /// The firmware's build, which what is learned about 4.x bindings is kept by.
    pub firmware: String,
    /// The URI player's codec table, which clamps HLS ceilings.
    pub table: Option<CodecTable>,
    /// Where `video-plane.json` is kept.
    pub data_dir: PathBuf,
    /// Device pixels per dp on the interface plane (2 on every LG TV).
    pub scale: f32,
}

/// The engine.
pub struct UmsEngine {
    machine: Machine,
    luna: Option<Luna>,
    waker: Waker,
    inbox: Arc<Mutex<Vec<Input>>>,
    windows: HashMap<u64, ExportedWindow>,
    subscriptions: HashMap<u64, Subscription>,
    acb: Option<&'static Mutex<Acb>>,
    memory_file: PathBuf,
    events: Vec<EngineEvent>,
}

impl UmsEngine {
    /// An engine on `luna` (none off the TV: every load then fails), waking
    /// the main loop with `waker` when the pipeline has news. Make it on the
    /// thread that runs the controller, which is the window's.
    pub fn new(luna: Option<Luna>, waker: Waker, cfg: UmsConfig) -> UmsEngine {
        let exported = ExportedWindow::available();
        let acb = if exported { None } else { Acb::get(&cfg.app_id) };
        let memory_file = cfg.data_dir.join(plane::FILE);
        let text = std::fs::read_to_string(&memory_file).ok();
        let memory = PlaneMemory::for_firmware(text.as_deref(), &cfg.firmware);
        let machine = Machine::new(
            Config {
                app_id: cfg.app_id,
                plane: if exported { PlaneKind::Exported } else { PlaneKind::Legacy },
                acb_available: acb.is_some(),
                table: cfg.table,
                scale: cfg.scale,
            },
            memory,
        );
        UmsEngine {
            machine,
            luna,
            waker,
            inbox: Arc::new(Mutex::new(Vec::new())),
            windows: HashMap::new(),
            subscriptions: HashMap::new(),
            acb,
            memory_file,
            events: Vec::new(),
        }
    }

    /// How the next playback's picture is bound, for Diagnostics.
    pub fn plane(&self) -> Plane {
        self.machine.plane()
    }

    /// Runs the machine's calls until it has none left.
    fn run(&mut self) {
        loop {
            let actions = self.machine.take_actions();
            if actions.is_empty() {
                break;
            }
            for action in actions {
                self.execute(action);
            }
        }
        self.events.extend(self.machine.take_events());
    }

    fn execute(&mut self, action: Action) {
        let now = Instant::now();
        match action {
            Action::Call { serial, method, payload } => self.call(serial, method, payload),
            Action::Subscribe { serial, media_id } => {
                let Some(luna) = &self.luna else { return };
                let (inbox, waker) = (self.inbox.clone(), self.waker.clone());
                let subscription = luna.subscribe(
                    &payload::uri("subscribe"),
                    &payload::media(&media_id),
                    SUBSCRIBE_TIMEOUT,
                    move |answer| {
                        let input = match answer {
                            Ok(message) => Input::Message { serial, message },
                            Err(e) => Input::Lost { serial, error: failure(e) },
                        };
                        push(&inbox, &waker, input);
                    },
                );
                self.subscriptions.insert(serial, subscription);
            }
            Action::Unsubscribe { serial } => {
                self.subscriptions.remove(&serial);
            }
            Action::PlaceWindow { serial, source, dest } => {
                if let Some(w) = self.windows.get_mut(&serial) {
                    w.place(Some(source), Some(dest));
                }
            }
            Action::DestroyWindow { serial } => {
                self.windows.remove(&serial);
            }
            Action::AcbBind { serial, media_id, dest, playing } => {
                let full = dest == video_full();
                let ok = self.acb.is_some_and(|acb| lock(acb).bind(&media_id, dest, full, playing));
                self.machine.input(Input::AcbBound { serial, ok }, now);
            }
            Action::AcbPlace { dest, full } => {
                if let Some(acb) = self.acb {
                    lock(acb).place(dest, full);
                }
            }
            Action::AcbState(state) => {
                if let Some(acb) = self.acb {
                    lock(acb).set_state(state);
                }
            }
            Action::CheckPlane { serial, media_id } => {
                let Some(luna) = &self.luna else { return };
                let (inbox, waker) = (self.inbox.clone(), self.waker.clone());
                let uri = payload::uri(Method::GetForegroundAppInfo.name());
                luna.call_with(&uri, &json!({}), CHECK_TIMEOUT, move |answer| {
                    let shown = answer.ok().and_then(|v| plane::shown_in(&v, &media_id));
                    push(&inbox, &waker, Input::PlaneShown { serial, shown });
                });
            }
            Action::Remember(memory) => {
                if let Err(e) = write_atomic(&self.memory_file, memory.to_json().as_bytes()) {
                    crate::log::line(&format!("ums: the video plane's memory wasn't saved: {e}"));
                }
            }
            Action::WakeAfter(delay) => {
                if let Some(luna) = &self.luna {
                    let waker = self.waker.clone();
                    luna.after(delay, move || waker.wake());
                }
            }
        }
    }

    fn call(&mut self, serial: u64, method: Method, payload: Value) {
        let Some(luna) = &self.luna else {
            // Off the TV: every call fails at once, as a TV without the service would.
            let error = failure(LunaError::Unavailable("LG's media pipeline isn't here".into()));
            let input = match method {
                Method::Load => Input::Loaded { serial, answer: Err(error) },
                _ => Input::Answered { serial, method, error: Some(error) },
            };
            self.machine.input(input, Instant::now());
            return;
        };
        let (inbox, waker) = (self.inbox.clone(), self.waker.clone());
        let timeout = if method == Method::Load { machine::LOAD_BUDGET } else { CALL_TIMEOUT };
        luna.call_with(&payload::uri(method.name()), &payload, timeout, move |answer| {
            let input = match method {
                Method::Load => Input::Loaded {
                    serial,
                    answer: answer.map_err(failure).and_then(|v| {
                        v.get("mediaId")
                            .and_then(Value::as_str)
                            .filter(|id| !id.is_empty())
                            .map(str::to_string)
                            .ok_or(Failure { code: machine::ENGINE_ERROR, message: "the load gave no media id".into() })
                    }),
                },
                _ => Input::Answered { serial, method, error: answer.err().map(failure) },
            };
            push(&inbox, &waker, input);
        });
    }

    /// Feeds the pipeline's news to the machine.
    fn drain(&mut self) {
        let inputs = std::mem::take(&mut *self.inbox.lock().unwrap_or_else(|e| e.into_inner()));
        let now = Instant::now();
        for input in inputs {
            self.machine.input(input, now);
        }
        self.machine.tick(now);
        self.run();
    }
}

fn push(inbox: &Mutex<Vec<Input>>, waker: &Waker, input: Input) {
    inbox.lock().unwrap_or_else(|e| e.into_inner()).push(input);
    waker.wake();
}

fn lock(acb: &Mutex<Acb>) -> std::sync::MutexGuard<'_, Acb> {
    acb.lock().unwrap_or_else(|e| e.into_inner())
}

fn video_full() -> Rect {
    crate::sdl::video::FULL
}

/// A bus error as the machine sees it.
fn failure(e: LunaError) -> Failure {
    match e {
        LunaError::Refused { code, text } => {
            Failure { code: code.and_then(|c| i32::try_from(c).ok()).unwrap_or(machine::ENGINE_ERROR), message: text }
        }
        other => Failure { code: machine::ENGINE_ERROR, message: other.to_string() },
    }
}

impl PlayerEngine for UmsEngine {
    fn load(&mut self, req: LoadRequest) {
        let mut window = None;
        if self.machine.wants_window() {
            let mut w = ExportedWindow::default();
            if w.create().is_some() {
                window = Some(w);
            } else {
                crate::log::line("ums: no exported window: the picture may not show");
            }
        }
        let id = window.as_ref().and_then(|w| w.id().map(str::to_string));
        let serial = self.machine.load(req, id.as_deref(), Instant::now());
        if let Some(w) = window {
            self.windows.insert(serial, w);
        }
        self.run();
    }

    fn play(&mut self) {
        if let Some(req) = self.machine.take_parked(None, true) {
            return self.load(req);
        }
        self.machine.play();
        self.run();
    }

    fn pause(&mut self) {
        self.machine.pause(Instant::now());
        self.run();
    }

    fn seek(&mut self, position_ms: u64) {
        if let Some(req) = self.machine.take_parked(Some(position_ms), false) {
            return self.load(req);
        }
        self.machine.seek(position_ms, Instant::now());
        self.run();
    }

    fn set_rate(&mut self, rate: f32) -> bool {
        let ok = self.machine.set_rate(rate);
        self.run();
        ok
    }

    fn select_audio(&mut self, ordinal: usize) -> bool {
        let ok = self.machine.select_audio(ordinal);
        self.run();
        ok
    }

    fn set_video_rect(&mut self, rect: Option<Rect>) {
        self.machine.set_video_rect(rect);
        self.run();
    }

    fn unload(&mut self) {
        self.machine.unload();
        self.run();
    }

    fn events(&mut self) -> Vec<EngineEvent> {
        self.drain();
        std::mem::take(&mut self.events)
    }

    fn position_ms(&self) -> u64 {
        self.machine.position_ms(Instant::now())
    }

    fn stats(&self) -> EngineStats {
        self.machine.stats()
    }
}

impl Drop for UmsEngine {
    fn drop(&mut self) {
        // The pipeline goes with its engine.
        self.machine.unload();
        self.run();
    }
}

#[cfg(test)]
mod tests;
