//! The player on a video of the USB drive: it plays the file with no server, starts it again from
//! the same file when the TV shows the app again, and says so when the drive is gone.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Waker};
use pley_core::device::{Drive, scratch_dir};
use pley_core::{FakeDevice, Http, PlayerEngine, Store, Tasks};
use pley_player::{SimClock, SimEngine, SimHandle};
use pley_shell::{AppEvent, AppState, PlayRequest, Registry, Route, Shell};
use pley_ui::testing::{StubImages, StubShaper};
use pley_ui::{FrameInput, Ui, UiState};

/// The app with the player alone over a fake TV whose engine the test watches.
struct TestApp {
    ui: UiState,
    list: DisplayList,
    images: StubImages,
    shell: Shell,
    state: AppState,
    time: f64,
    engines: Arc<Mutex<Vec<SimHandle>>>,
    device: Arc<FakeDevice>,
}

impl TestApp {
    fn new(drive: Option<Drive>) -> TestApp {
        let engines: Arc<Mutex<Vec<SimHandle>>> = Arc::default();
        let made = engines.clone();
        let device = Arc::new(FakeDevice::new(move || {
            let (engine, handle) = SimEngine::new(SimClock::system());
            made.lock().unwrap().push(handle);
            Box::new(engine) as Box<dyn PlayerEngine>
        }));
        device.set_drive(drive);
        let tv = device.clone();
        let store = Store::open(scratch_dir("player-drive-store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        let mut registry = Registry::new();
        registry.add(pley_player_ui::screen);
        let state = AppState::new(device, store, http, Tasks::new(Waker::noop()));
        TestApp {
            ui: UiState::new(),
            list: DisplayList::default(),
            images: StubImages::new(),
            shell: Shell::new(registry),
            state,
            time: pley_base::time::now(),
            engines,
            device: tv,
        }
    }

    fn frame(&mut self) {
        self.time = self.time.max(pley_base::time::now());
        let now = self.time;
        self.shell.update(&mut self.state, &mut self.ui, now);
        for _ in 0..4 {
            let frame = FrameInput { time: now, events: Vec::new() };
            let mut ui = Ui::new(&mut self.ui, &mut self.list, &StubShaper, &mut self.images, 2.0, &frame);
            self.shell.frame(&mut ui, &mut self.state);
            let out = ui.finish();
            let changed = self.shell.finish(&mut self.state, &mut self.ui, now);
            if !(out.redraw || changed) {
                break;
            }
        }
    }

    /// The addresses every engine made so far was asked to load.
    fn loads(&self) -> Vec<String> {
        self.engines.lock().unwrap().iter().flat_map(|h| h.loads()).map(|l| l.url).collect()
    }

    /// Draws frames until `n` loads went out (5 s at most).
    fn until_loads(&mut self, n: usize) -> Vec<String> {
        let start = Instant::now();
        while self.loads().len() < n && start.elapsed() < Duration::from_secs(5) {
            self.frame();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.loads()
    }
}

fn film_on_a_drive() -> (Drive, std::path::PathBuf) {
    let root = scratch_dir("player-drive");
    let film = root.join("Films").join("Our trip.mp4");
    std::fs::create_dir_all(film.parent().unwrap()).unwrap();
    std::fs::write(&film, b"not much of a film").unwrap();
    (Drive { root, hidden: false }, film)
}

fn play(file: std::path::PathBuf) -> Route {
    Route::Player(PlayRequest { file: Some(file), start_ms: Some(0), ..PlayRequest::item(-1) })
}

#[test]
fn a_drives_video_plays_from_its_file_with_no_server() {
    let (drive, film) = film_on_a_drive();
    let mut app = TestApp::new(Some(drive));
    app.shell.start(&mut app.state, &mut app.ui, play(film));
    let loads = app.until_loads(1);
    assert_eq!(loads.len(), 1, "{loads:?}");
    assert!(loads[0].starts_with("file:///"), "{}", loads[0]);
    assert!(loads[0].ends_with("/Films/Our%20trip.mp4"), "{}", loads[0]);
    assert!(matches!(app.shell.top_route(), Some(Route::Player(_))));

    // Hidden by the TV's Home, then shown: the same file again.
    app.shell.visible(false, &mut app.state, &mut app.ui, app.time);
    app.shell.deliver(AppEvent::Visible(true), &mut app.state, &mut app.ui, app.time);
    let loads = app.until_loads(2);
    assert_eq!(loads.len(), 2, "{loads:?}");
    assert_eq!(loads[0], loads[1]);
}

#[test]
fn a_drives_video_whose_drive_is_gone_says_so_and_leaves() {
    let (_, film) = film_on_a_drive();
    let mut app = TestApp::new(None);
    app.shell.start(&mut app.state, &mut app.ui, Route::Home);
    app.shell.navigate(pley_shell::NavCommand::Push(play(film)), &mut app.state, &mut app.ui);
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(app.shell.top_route(), Some(&Route::Home));
    assert!(app.list.texts().any(|t| t == pley_player_ui::NO_DRIVE), "{:?}", app.list.texts().collect::<Vec<_>>());
    assert!(app.loads().is_empty());
}

#[test]
fn a_drive_pulled_out_while_the_app_was_hidden_says_so_and_leaves_when_it_shows() {
    let (drive, film) = film_on_a_drive();
    let mut app = TestApp::new(Some(drive));
    app.shell.start(&mut app.state, &mut app.ui, Route::Home);
    app.shell.navigate(pley_shell::NavCommand::Push(play(film)), &mut app.state, &mut app.ui);
    assert_eq!(app.until_loads(1).len(), 1);
    // Home on the remote, the drive pulled out, back to the app.
    app.shell.visible(false, &mut app.state, &mut app.ui, app.time);
    app.device.set_drive(None);
    app.shell.deliver(AppEvent::Visible(true), &mut app.state, &mut app.ui, app.time);
    for _ in 0..3 {
        app.frame();
    }
    // Not an empty page only Back leaves.
    assert_eq!(app.shell.top_route(), Some(&Route::Home));
    assert!(app.list.texts().any(|t| t == pley_player_ui::NO_DRIVE), "{:?}", app.list.texts().collect::<Vec<_>>());
    assert_eq!(app.loads().len(), 1);
}
