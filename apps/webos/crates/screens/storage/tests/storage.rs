//! The drive's page over a fake TV with a folder for a drive: what a folder lists and says, where
//! OK goes, where each video resumes, a drive pulled out, and what Audio Guidance reads. No
//! server is needed: a drive's videos play without one.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use pley_base::{DisplayList, Key, KeyEvent, Waker};
use pley_core::device::{Drive, DriveSpace, scratch_dir};
use pley_core::media::NullEngine;
use pley_core::{
    CapsOverrides, CapsReport, Device, DeviceEvent, DeviceInfo, FakeDevice, Http, PlayerEngine, Store, Tasks, Tier,
};
use pley_local::resume;
use pley_local::server::ITEM_ID;
use pley_shell::{AppState, NavCommand, PlayRequest, Registry, Route, Shell};
use pley_storage::StorageScreen;
use pley_ui::testing::{StubImages, StubShaper};
use pley_ui::{FrameInput, Id, InputEvent, Ui, UiState};

/// The app without a window, with the drive's page alone, over a fake TV.
struct TestApp {
    ui: UiState,
    list: DisplayList,
    images: StubImages,
    shell: Shell,
    state: AppState,
    time: f64,
}

impl TestApp {
    fn open_on(device: Arc<dyn Device>, route: Route) -> TestApp {
        let store = Store::open(scratch_dir("storage-store")).unwrap();
        let http = Http::network("pleyLG/0.1.0 (webOS 22; test)");
        let mut registry = Registry::new();
        registry.add(pley_storage::screen);
        let state = AppState::new(device, store, http, Tasks::new(Waker::noop()));
        let mut app = TestApp {
            ui: UiState::new(),
            list: DisplayList::default(),
            images: StubImages::new(),
            shell: Shell::new(registry),
            state,
            time: pley_base::time::now(),
        };
        app.shell.start(&mut app.state, &mut app.ui, route);
        app.read();
        app
    }

    fn frame(&mut self, events: Vec<InputEvent>) {
        let now = self.time;
        let mut input = events;
        self.shell.filter_input(&mut input, now);
        self.shell.update(&mut self.state, &mut self.ui, now);
        for _ in 0..4 {
            let frame = FrameInput { time: now, events: std::mem::take(&mut input) };
            let mut ui = Ui::new(&mut self.ui, &mut self.list, &StubShaper, &mut self.images, 2.0, &frame);
            self.shell.frame(&mut ui, &mut self.state);
            let out = ui.finish();
            let changed = self.shell.finish(&mut self.state, &mut self.ui, now);
            if !(out.redraw || changed) {
                break;
            }
        }
    }

    fn press(&mut self, key: Key) {
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: true, repeat: false, code: 0 })]);
        self.time += 0.05;
        self.frame(vec![InputEvent::Key(KeyEvent { key, pressed: false, repeat: false, code: 0 })]);
        self.advance(0.15);
    }

    fn advance(&mut self, secs: f64) {
        let end = self.time + secs;
        while self.time < end {
            self.time = (self.time + 0.05).min(end);
            self.frame(Vec::new());
        }
    }

    /// Draws frames, the app's clock following the real one so the reading comes, until `done`
    /// holds (5 s at most); then lets the page settle.
    fn until(&mut self, done: impl Fn(&TestApp) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            self.time = self.time.max(pley_base::time::now());
            self.frame(Vec::new());
            if done(self) {
                self.advance(0.6);
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// Waits for the page on top to have read its folder: its lines or why there are none.
    fn read(&mut self) {
        let read = |a: &TestApp| a.list.texts().count() > 1;
        assert!(self.until(read), "the folder was never read");
    }

    /// Delivers the TV's news, as the frame loop does.
    fn news(&mut self, rx: &std::sync::mpsc::Receiver<pley_core::DeviceEvent>) {
        while let Ok(e) = rx.try_recv() {
            self.shell.deliver(pley_shell::AppEvent::Device(e), &mut self.state, &mut self.ui, self.time);
        }
    }

    fn page(&self) -> Id {
        self.shell.top().unwrap().group_id()
    }

    fn line(&self, i: usize) -> Id {
        StorageScreen::line_id(self.page(), i)
    }

    fn shows(&self, text: &str) -> bool {
        self.list.texts().any(|t| t == text)
    }

    /// Where the icon `icon` is drawn.
    fn icon_at(&self, icon: pley_base::Icon) -> Option<pley_base::Rect> {
        self.list.cmds.iter().find_map(|c| match c {
            pley_base::Cmd::Icon { rect, icon: i, .. } if *i == icon => Some(*rect),
            _ => None,
        })
    }
}

/// A drive: two films, a folder of a series, and what a person never wants listed.
fn drive() -> PathBuf {
    let root = scratch_dir("storage-drive");
    for folder in ["Season 1", "$RECYCLE.BIN", "System Volume Information"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
    }
    std::fs::write(root.join("Film 10.mkv"), vec![0u8; 3000]).unwrap();
    std::fs::write(root.join("Film 2.mp4"), vec![0u8; 1536]).unwrap();
    std::fs::write(root.join(".hidden.mkv"), b"x").unwrap();
    std::fs::write(root.join("notes.txt"), b"x").unwrap();
    std::fs::write(root.join("Film 2.en.srt"), b"1\n").unwrap();
    root
}

fn tv(root: &Path) -> Arc<FakeDevice> {
    let device = Arc::new(FakeDevice::new(|| Box::new(NullEngine::default()) as Box<dyn PlayerEngine>));
    device.set_drive(Some(Drive { root: root.to_path_buf(), hidden: false }));
    device.set_drive_space(Some(DriveSpace { free: 1 << 30, total: 2 << 30 }));
    device
}

/// Where `text` is drawn: its top.
fn y_of(app: &TestApp, text: &str) -> Option<f32> {
    app.list.cmds.iter().find_map(|c| match c {
        pley_base::Cmd::Text { origin, layout, .. } if &*layout.text == text => Some(origin.y),
        _ => None,
    })
}

#[test]
fn the_drives_top_lists_its_folders_then_its_videos_and_how_full_it_is() {
    let root = drive();
    let app = TestApp::open_on(tv(&root), Route::Storage { path: None });
    assert!(app.shows("USB drive"));
    assert!(app.shows("/ · 1.0 GB free of 2.0 GB"));
    let (folder, two, ten) = (y_of(&app, "Season 1"), y_of(&app, "Film 2"), y_of(&app, "Film 10"));
    assert!(folder < two && two < ten, "{folder:?} {two:?} {ten:?}");
    assert!(app.shows("MP4") && app.shows("1.5 KB") && app.shows("MKV"));
    for hidden in ["$RECYCLE.BIN", "System Volume Information", "hidden", ".hidden", "notes", "Film 2.en"] {
        assert!(!app.shows(hidden), "{hidden}");
    }
    assert_eq!(app.ui.focused(), Some(app.line(0)));
}

#[test]
fn a_video_started_before_says_where_it_resumes_and_plays_from_there() {
    let root = drive();
    let device = tv(&root);
    let mut app = TestApp::open_on(device, Route::Storage { path: None });
    let id = resume::file_id(Path::new("Film 2.mp4"), 1536);
    resume::keep(app.state.store(), id, 1_830_000, Some(7_200_000)).unwrap();
    // Back from a player: the page reads where each video stopped again.
    app.shell.deliver(pley_shell::AppEvent::Shown, &mut app.state, &mut app.ui, app.time);
    assert!(app.until(|a| a.shows("MP4 · Resumes at 30:30")));
    // Its bar: a quarter of 160 dp, in the accent.
    let bar = (1_830_000.0 / 7_200_000.0) * 160.0;
    let fills = app.list.cmds.iter().filter(|c| {
        matches!(c, pley_base::Cmd::Fill { rect, color, .. } if *color == pley_ui::theme::ACCENT && (rect.w - bar).abs() < 0.01)
    });
    assert_eq!(fills.count(), 1);
    app.press(Key::Down);
    assert_eq!(app.ui.focused(), Some(app.line(1)));
    app.press(Key::Ok);
    let want =
        PlayRequest { file: Some(root.join("Film 2.mp4")), start_ms: Some(1_830_000), ..PlayRequest::item(ITEM_ID) };
    match app.shell.top_route() {
        Some(Route::Player(r)) => assert_eq!(PlayRequest { pressed_at: 0.0, ..r.clone() }, want),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_folder_opens_over_the_page_and_up_comes_back() {
    let root = drive();
    std::fs::write(root.join("Season 1/notes.txt"), b"x").unwrap();
    let mut app = TestApp::open_on(tv(&root), Route::Storage { path: None });
    app.press(Key::Ok);
    app.read();
    assert_eq!(app.shell.top_route(), Some(&Route::Storage { path: Some(root.join("Season 1")) }));
    assert!(app.shows("/Season 1"));
    // Under "..", why nothing else shows.
    assert!(app.shows("..") && app.shows("Up a folder"));
    assert!(app.shows("Nothing to play here"));
    assert!(y_of(&app, "Up a folder") < y_of(&app, "Nothing to play here"));
    let icon = app.icon_at(pley_base::Icon::FolderOpen).expect("the message's picture");
    assert!(icon.bottom() < y_of(&app, "Nothing to play here").unwrap());
    assert_eq!(app.ui.focused(), Some(app.line(0)));
    app.press(Key::Ok);
    assert_eq!(app.shell.top_route(), Some(&Route::Storage { path: None }));
}

#[test]
fn a_drive_pulled_out_leaves_the_page_saying_so() {
    let root = drive();
    let device = tv(&root);
    let (tx, rx) = std::sync::mpsc::channel();
    pley_core::Device::subscribe(device.as_ref(), tx);
    let mut app = TestApp::open_on(device.clone(), Route::Storage { path: None });
    assert!(app.shows("Film 10"));
    device.set_drive(None);
    app.news(&rx);
    assert!(app.until(|a| a.shows("No USB drive")));
    assert!(app.shows("Plug a USB drive into the TV to play its videos here."));
    assert!(!app.shows("Film 10"));
    // Focus stays on the page, on its message, which nothing marks: the rail stays closed.
    assert_eq!(app.ui.focused(), Some(StorageScreen::message_id(app.page())));
    assert!(!app.shell.rail_open());
    let usb = app.icon_at(pley_base::Icon::Usb).expect("the message's picture");
    assert!((usb.center().x - (96.0 + (960.0 - 96.0 - 48.0) / 2.0)).abs() < 0.5, "centred: {usb:?}");
    // A drive the jail hides is no drive to the page either.
    device.set_drive(Some(Drive { root: root.clone(), hidden: true }));
    app.news(&rx);
    app.advance(0.3);
    assert!(app.until(|a| a.shows("No USB drive")));
    // Plugged in again, its videos are back.
    device.set_drive(Some(Drive { root: root.clone(), hidden: false }));
    app.news(&rx);
    assert!(app.until(|a| a.shows("Film 10")));
    assert_eq!(app.ui.focused(), Some(app.line(0)), "the lines take focus from the message");
}

/// A TV whose reading of a drive stops outright: asked the drive's name, it panics.
struct Broken {
    tv: Arc<FakeDevice>,
    asked: AtomicUsize,
}

impl Device for Broken {
    fn info(&self) -> DeviceInfo {
        self.tv.info()
    }
    fn data_dir(&self) -> PathBuf {
        self.tv.data_dir()
    }
    fn cache_dir(&self) -> PathBuf {
        self.tv.cache_dir()
    }
    fn font_dirs(&self) -> Vec<PathBuf> {
        self.tv.font_dirs()
    }
    fn tier(&self) -> Tier {
        self.tv.tier()
    }
    fn caps(&self, overrides: &CapsOverrides) -> CapsReport {
        self.tv.caps(overrides)
    }
    fn local_ipv4(&self) -> Option<Ipv4Addr> {
        self.tv.local_ipv4()
    }
    fn clock_24h(&self) -> bool {
        self.tv.clock_24h()
    }
    fn screen_reader(&self) -> bool {
        self.tv.screen_reader()
    }
    fn keep_awake(&self, on: bool) {
        self.tv.keep_awake(on)
    }
    fn exit(&self) {
        self.tv.exit()
    }
    fn engine(&self) -> Box<dyn PlayerEngine> {
        self.tv.engine()
    }
    fn subscribe(&self, tx: Sender<DeviceEvent>) {
        self.tv.subscribe(tx)
    }
    fn drive(&self) -> Option<Drive> {
        self.tv.drive()
    }
    fn drive_name(&self, _root: &Path) -> Option<String> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        panic!("the drive's name can't be asked");
    }
}

#[test]
fn a_reading_that_stops_says_the_folder_cant_be_read_and_isnt_tried_again() {
    let root = drive();
    let broken = Arc::new(Broken { tv: tv(&root), asked: AtomicUsize::new(0) });
    let mut app = TestApp::open_on(broken.clone(), Route::Storage { path: Some(root.join("Season 1")) });
    assert!(app.shows("This folder can't be read"));
    assert!(app.shows(pley_storage::UP), "no way up");
    app.advance(1.0);
    assert_eq!(broken.asked.load(Ordering::SeqCst), 1, "the folder was read again and again");
}

#[test]
fn an_empty_drive_keeps_focus_on_its_message() {
    let root = scratch_dir("storage-empty");
    let app = TestApp::open_on(tv(&root), Route::Storage { path: None });
    assert!(app.shows("Nothing to play here") && app.shows("/ · 1.0 GB free of 2.0 GB"));
    assert!(app.icon_at(pley_base::Icon::FolderOpen).is_some());
    assert_eq!(app.ui.focused(), Some(StorageScreen::message_id(app.page())));
    assert!(!app.shell.rail_open());
}

#[test]
fn the_title_is_the_drives_make_and_model_when_it_says_them() {
    let root = drive();
    let device = tv(&root);
    device.set_drive_name(Some("Kingston DataTraveler 3.0"));
    let app = TestApp::open_on(device, Route::Storage { path: None });
    assert!(app.shows("Kingston DataTraveler 3.0") && !app.shows("USB drive"));
    assert_eq!(app.shell.top().unwrap().screen().title().as_deref(), Some("Kingston DataTraveler 3.0"));
}

#[test]
fn a_folder_not_on_the_drive_plugged_in_is_no_drive() {
    let root = drive();
    let other = scratch_dir("storage-other");
    let app = TestApp::open_on(tv(&root), Route::Storage { path: Some(other) });
    assert!(app.shows("No USB drive"));
    assert!(!app.shows("Film 10") && !app.shows(".."));
}

#[test]
fn audio_guidance_reads_the_page_then_each_line() {
    let root = drive();
    let device = tv(&root);
    device.set_screen_reader(true);
    let mut app = TestApp::open_on(device, Route::Home);
    app.ui.set_speaking(true);
    app.shell.navigate(NavCommand::Reset(Route::Storage { path: None }), &mut app.state, &mut app.ui);
    app.read();
    let first = app.ui.take_speech().unwrap_or_default();
    assert!(first.contains("USB drive") && first.contains("Season 1"), "{first}");
    app.press(Key::Down);
    app.advance(0.3);
    let next = app.ui.take_speech().unwrap_or_default();
    assert!(next.contains("Film 2, MP4, 1.5 KB"), "{next}");
}
