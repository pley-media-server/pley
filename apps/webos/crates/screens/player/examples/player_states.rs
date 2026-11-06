//! The player's states drawn by the app's renderer over a stand-in playback, to look at the ones a
//! real video passes through too quickly for the simulator's screenshots (a seek on its way, Skip,
//! Up Next, the sleep notice, buffering, an error, subtitles):
//!
//! `cargo run -p pley-player-ui --example player_states -- [--locale fr-FR] <folder> [state …]`
//!
//! writes `<folder>/<state>.png` for each state named, or for every state, in the language of
//! `--locale` (English by default). The picture's place is transparent, as on the TV. `--list`
//! names the states.

#[path = "../tests/common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use common::Fake;
use pley_base::{DisplayList, Fetch, FetchError, Key, KeyEvent, Rect, RgbaImage, Size, Waker};
use pley_core::model::PlaybackMethod;
use pley_core::model::{SubtitleResult, TrickplayInfo};
use pley_core::store::SubtitleBackground;
use pley_core::{Settings, Tasks};
use pley_gfx::{ImageCache, ImageCacheConfig, Renderer, screenshot};
use pley_i18n::t;
use pley_platform::Platform;
use pley_platform::desktop::{DesktopOptions, DesktopPlatform};
use pley_player::Phase;
use pley_player::controller::{QualityChoice, SeekPreview, Trickplay};
use pley_player::details::{self, DetailRow, PlaybackDetails};
use pley_player::quality::preset;
use pley_player_ui::{FrameEnv, PlayerView, Previews};
use pley_subs::{PictureCue, Shown, parse_vtt};
use pley_text::{FontSet, TextEngine};
use pley_ui::{FrameInput, InputEvent, Ui, UiState};

/// A state: how the stand-in starts, the settings, and the keys that lead there.
struct State {
    name: &'static str,
    setup: fn(&mut Fake, &mut Settings),
    /// Words: up down left right ok menu info subs audio back, `wait:<ms>`, `hold:<key>:<ms>`
    /// (a direction held that long, still held when the picture is taken).
    script: &'static str,
    reduced_motion: bool,
}

const fn state(name: &'static str, setup: fn(&mut Fake, &mut Settings), script: &'static str) -> State {
    State { name, setup, script, reduced_motion: false }
}

fn states() -> Vec<State> {
    vec![
        state("controls-paused", |_, _| {}, "ok wait:400"),
        state(
            "controls-row",
            |p, _| p.state.method = Some(t!("player.method.transcode").into()),
            "down right wait:400",
        ),
        // Sleep timer first in the row, its label nearest the edge.
        state("controls-left", |p, _| p.state.chapters.clear(), "down left left left wait:400"),
        state("seek-preview", preview, "up wait:300 hold:right:1500"),
        state("seek-chapter", |_, _| {}, "up wait:300 hold:left:700"),
        state("seek-badge", |_, _| {}, "right right right"),
        state("skip", |p, _| p.state.skip_label = Some(t!("player.skip.intro").into()), "wait:400"),
        state(
            "skip-lifted",
            |p, _| p.state.skip_label = Some(t!("player.skip.credits").into()),
            "wait:200 info wait:400",
        ),
        state("up-next", up_next, "wait:400"),
        State { reduced_motion: true, ..state("up-next-reduced", up_next, "wait:400") },
        state("up-next-no-still", |p, _| p.state.up_next = Some(card(7)), "wait:400"),
        state("sleep-notice", |p, _| p.state.sleep.seconds_left = Some(28), "wait:400"),
        state("buffering", |p, _| p.state.phase = Phase::Buffering, "wait:900"),
        state(
            "reconnecting",
            |p, _| {
                p.state.phase = Phase::Buffering;
                p.state.reconnecting = true;
            },
            "wait:900",
        ),
        state("error", error, "wait:400"),
        state("stopped", stopped, "wait:400"),
        state("subtitles", |p, _| p.shown = cues(), "wait:200"),
        state("subtitles-lifted", |p, _| p.shown = cues(), "up wait:400"),
        state(
            "subtitles-box",
            |p, s| {
                p.shown = cues();
                s.subtitle_background = SubtitleBackground::DarkBox;
            },
            "wait:200",
        ),
        state("subtitles-picture", |p, _| p.shown = picture(), "up wait:400"),
        state("subtitles-panel", |p, _| p.state.subtitle_sync = true, "down right right ok wait:400"),
        state("find", find, "down right right ok wait:300 down down ok wait:400"),
        state("audio-panel", |_, _| {}, "audio wait:400"),
        state("quality", quality, "down right right right right ok wait:400"),
        state("chapters", |_, _| {}, "down left left left left left left left left ok wait:400"),
        state(
            "sleep",
            |p, _| {
                p.state.sleep.choice = pley_player::sleep::SleepChoice::Minutes(45);
                p.state.sleep.minutes_left = Some(42);
            },
            "down left left left ok wait:400",
        ),
        state("speed", |p, _| p.state.speeds = vec![0.5, 0.75, 1.0, 1.25, 1.5, 2.0], "down left left ok wait:400"),
        state("info", details, "down right right right right ok wait:400"),
        state("info-end", details, "down right right right right ok wait:300 down down down wait:600"),
        state(
            "readout",
            |p, s| {
                details(p, s);
                s.playback_details = true;
            },
            "wait:400",
        ),
        state("toast", toast, "wait:400"),
    ]
}

fn preview(p: &mut Fake, _: &mut Settings) {
    let info = TrickplayInfo {
        id: "1".into(),
        interval_ms: 10_000,
        width: 160,
        height: 90,
        columns: 5,
        rows: 5,
        count: 25,
        sheets: 1,
    };
    p.state.trickplay = Some(Trickplay { file_id: 1, info });
    p.preview = Some(SeekPreview::Sheet { url: "sheet:0".into(), x: 320, y: 180, width: 160, height: 90 });
}

/// The stand-in's card, its heading in the language shown.
fn card(seconds: i64) -> pley_player::controller::UpNextCard {
    let mut card = Fake::card(seconds);
    card.heading = details::up_next(Some("S1 · E3"));
    card
}

/// The card as it first shows, its countdown at its widest ("Next in 10" under Reduce motion).
fn up_next(p: &mut Fake, _: &mut Settings) {
    let mut card = card(pley_player::controller::COUNTDOWN_SECONDS);
    card.image_url = Some("still:next".into());
    p.state.up_next = Some(card);
}

fn error(p: &mut Fake, _: &mut Settings) {
    p.state.phase = Phase::Error { message: t!("player.errors.sameSpot").into(), can_retry: true, title: None };
}

fn stopped(p: &mut Fake, _: &mut Settings) {
    p.state.stopped_by_server = true;
    p.state.phase = Phase::Error { message: details::stopped_text(None, true), can_retry: false, title: None };
}

fn cues() -> Shown {
    let vtt = "WEBVTT\n\n00:00.000 --> 01:00.000\nWe should have left an hour ago.\n<i>I know.</i> Keep driving.\n";
    Shown { text: parse_vtt(vtt).into_iter().map(Arc::new).collect(), pictures: Vec::new(), key: 1 }
}

/// A picture subtitle of a DVD's 720 × 576 plane: a white caption on black, low in the frame.
fn picture() -> Shown {
    let (w, h) = (360, 48);
    let mut image = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let edge = x < 3 || y < 3 || x >= w - 3 || y >= h - 3;
            let bar = (y / 8) % 2 == 0 && (x / 12) % 3 != 2;
            let c: [u8; 4] = if edge {
                [0, 0, 0, 255]
            } else if bar {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 0]
            };
            let i = ((y * w + x) * 4) as usize;
            image.pixels[i..i + 4].copy_from_slice(&c);
        }
    }
    let cue = PictureCue {
        start_ms: 0,
        end_ms: None,
        image: Arc::new(image),
        rect: Rect::new(180.0, 500.0, w as f32, h as f32),
        plane: Size::new(720.0, 576.0),
        key: 9,
    };
    Shown { text: Vec::new(), pictures: vec![cue], key: 2 }
}

fn find(p: &mut Fake, _: &mut Settings) {
    let result = |id: &str, name: &str, downloads: i64, hash: bool| SubtitleResult {
        id: id.into(),
        provider: "opensubtitles".into(),
        language: Some("en".into()),
        name: name.into(),
        format: Some("srt".into()),
        downloads,
        matched: hash.then(|| "hash".into()),
        ..SubtitleResult::default()
    };
    p.finds = Some(vec![
        result("a", "Fixture.Show.S01E02.1080p.WEB.H264", 1_234, true),
        result("b", "Fixture.Show.S01E02.720p.HDTV", 312, false),
        SubtitleResult { hearing_impaired: true, ..result("c", "Fixture.Show.S01E02.SDH", 87, false) },
    ]);
}

fn quality(p: &mut Fake, _: &mut Settings) {
    p.state.note = Some(details::fallback_text(pley_player::recovery::FallbackReason::NotSmooth).into());
    p.state.can_play_original = true;
    let q = |id: &'static str, hint: Option<String>, selected: bool| QualityChoice {
        id,
        label: preset(id).expect("a preset").label,
        hint,
        selected,
    };
    let now = preset("8m").expect("a preset").label;
    p.state.qualities = vec![
        q("auto", Some(details::auto_now(&now, Some(t!("player.quality.lowered.network")))), true),
        q("original", Some("2160p · 40 Mbps".into()), false),
        q("20m", None, false),
        q("8m", None, false),
        q("4m", None, false),
    ];
}

fn details(p: &mut Fake, _: &mut Settings) {
    let row = |label: &str, value: &str| DetailRow { label: label.into(), value: value.into() };
    let speed = t!("player.details.realTimeReady", speed = pley_i18n::decimal(2.5, 1), seconds = 40);
    p.state.details = Some(PlaybackDetails {
        method: t!("player.method.transcode").into(),
        // The server's words.
        why: Some("The TV can't decode TrueHD audio".into()),
        pipeline: Some("GPU".into()),
        rows: vec![
            row(t!("player.details.video"), "HEVC 4K HDR10 → H.264 1080p SDR"),
            row(t!("player.details.audio"), "TrueHD 7.1 → AAC 2.0"),
            row(t!("player.details.bitrate"), "40 Mbps → 8 Mbps"),
            row(t!("player.details.encoder"), "NVIDIA NVENC (GPU)"),
            row(t!("player.details.speed"), &speed),
        ],
        playback_method: PlaybackMethod::Transcode,
        file: None,
        video: None,
        audio: None,
        subtitle: None,
        subtitle_delivery: None,
        transcode: None,
    });
    p.state.method = Some(t!("player.method.transcode").into());
    p.state.first_frame_ms = Some(843);
    p.stats.bitrate_kbps = Some(7_800);
    p.stats.network_bps = Some(48_000_000);
    p.stats.width = Some(1920);
    p.stats.height = Some(1080);
    p.stats.fps = Some(23.976);
    p.stats.dynamic_range = Some("SDR".into());
    p.stats.video_codec = Some("h264".into());
    p.stats.buffered_ahead_ms = Some(18_000);
    p.stats.dropped_frames = Some(12);
    p.stats.total_frames = Some(4_210);
}

fn toast(p: &mut Fake, _: &mut Settings) {
    p.state.notice = Some(details::subtitles_added("English (SRT)"));
    p.state.notice_id = 1;
}

/// The stand-in's words that the player makes, in the language shown (the tests keep English's).
fn in_language(p: &mut Fake) {
    p.state.method = Some(t!("player.method.direct").into());
    p.state.quality = t!("player.quality.original").into();
    if let Some(off) = p.state.subtitles.iter_mut().find(|s| s.index.is_none()) {
        off.label = details::subtitles_off().into();
    }
}

/// Pictures the stand-in names: a sheet of seek previews (5 × 5 tiles of 160 × 90, each its own
/// shade) and a still.
struct Pictures;

impl Fetch for Pictures {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let image = if url.starts_with("sheet:") {
            paint(800, 450, |x, y| {
                let tile = (y / 90) * 5 + x / 160;
                let (tx, ty) = (x % 160, y % 90);
                let k = (tile * 9 % 25) as f32 / 25.0;
                let sky = ty < 50;
                let c = if sky { [80.0 + 100.0 * k, 120.0 + 60.0 * k, 170.0] } else { [60.0, 90.0 + 50.0 * k, 50.0] };
                let shade = 1.0 - (tx as f32 / 160.0) * 0.3;
                [(c[0] * shade) as u8, (c[1] * shade) as u8, (c[2] * shade) as u8, 255]
            })
        } else {
            paint(480, 270, |x, y| [(40 + x / 6) as u8, (60 + y / 5) as u8, 110, 255])
        };
        screenshot::encode_png(&image).map_err(|e| FetchError::Failed(e.to_string()))
    }
}

fn paint(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
    let mut image = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            image.pixels[i..i + 4].copy_from_slice(&f(x, y));
        }
    }
    image
}

fn key_of(word: &str) -> Key {
    match word {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "ok" => Key::Ok,
        "menu" => Key::Menu,
        "info" => Key::Info,
        "subs" => Key::Subtitles,
        "audio" => Key::Audio,
        other => panic!("not a key: {other}"),
    }
}

/// Draws the frames of `s` and writes the last one to `out`.
struct Shooter {
    platform: DesktopPlatform,
    engine: Arc<TextEngine>,
    renderer: Renderer,
    images: ImageCache,
}

impl Shooter {
    fn frame(
        &mut self,
        ui_state: &mut UiState,
        view: &mut PlayerView,
        p: &mut Fake,
        settings: &Settings,
        events: Vec<InputEvent>,
    ) {
        let mut list = DisplayList::default();
        self.renderer.prepare(&mut self.images);
        let input = FrameInput { time: pley_base::time::now(), events };
        let env = FrameEnv { settings, hours24: true, unix_now: 1_700_000_000.0 };
        let redraw = {
            let mut ui = Ui::new(ui_state, &mut list, &*self.engine, &mut self.images, self.renderer.scale(), &input);
            view.frame(&mut ui, p, &env);
            ui.finish().redraw
        };
        if redraw {
            let input = FrameInput { time: pley_base::time::now(), events: Vec::new() };
            list = DisplayList::default();
            let mut ui = Ui::new(ui_state, &mut list, &*self.engine, &mut self.images, self.renderer.scale(), &input);
            view.frame(&mut ui, p, &env);
            ui.finish();
        }
        self.renderer.render(&list, &mut self.images);
    }

    /// Frames for `secs` of real time, about 60 a second.
    fn run(
        &mut self,
        secs: f64,
        ui: &mut UiState,
        view: &mut PlayerView,
        p: &mut Fake,
        settings: &Settings,
        repeat: Option<Key>,
    ) {
        let end = pley_base::time::now() + secs;
        let mut next_repeat = pley_base::time::now() + 0.4;
        while pley_base::time::now() < end {
            let mut events = Vec::new();
            if let Some(k) = repeat
                && pley_base::time::now() >= next_repeat
            {
                events.push(InputEvent::Key(KeyEvent { key: k, pressed: true, repeat: true, code: 0 }));
                next_repeat += 0.1;
            }
            self.frame(ui, view, p, settings, events);
            self.platform.present();
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    fn shoot(&mut self, s: &State, out: &Path) {
        let mut p = Fake::episode();
        in_language(&mut p);
        let mut settings = Settings::default();
        (s.setup)(&mut p, &mut settings);
        let mut ui = UiState::new();
        ui.set_reduced_motion(s.reduced_motion);
        let mut view = PlayerView::new();
        view.set_previews(Previews::new(Arc::new(Pictures), Tasks::new(Waker::noop())));
        self.run(0.3, &mut ui, &mut view, &mut p, &settings, None);
        let mut held = None;
        for word in s.script.split_whitespace() {
            if let Some(ms) = word.strip_prefix("wait:") {
                let secs = ms.parse::<f64>().expect("a wait in ms") / 1000.0;
                self.run(secs, &mut ui, &mut view, &mut p, &settings, None);
            } else if let Some(rest) = word.strip_prefix("hold:") {
                let (k, ms) = rest.split_once(':').expect("hold:<key>:<ms>");
                let key = key_of(k);
                let down = KeyEvent { key, pressed: true, repeat: false, code: 0 };
                self.frame(&mut ui, &mut view, &mut p, &settings, vec![InputEvent::Key(down)]);
                let secs = ms.parse::<f64>().expect("a hold in ms") / 1000.0;
                self.run(secs, &mut ui, &mut view, &mut p, &settings, Some(key));
                held = Some(key);
            } else if word == "back" {
                view.back(&mut p);
                self.run(0.05, &mut ui, &mut view, &mut p, &settings, None);
            } else {
                let key = key_of(word);
                let events = vec![
                    InputEvent::Key(KeyEvent { key, pressed: true, repeat: false, code: 0 }),
                    InputEvent::Key(KeyEvent { key, pressed: false, repeat: false, code: 0 }),
                ];
                self.frame(&mut ui, &mut view, &mut p, &settings, events);
                self.run(0.05, &mut ui, &mut view, &mut p, &settings, None);
            }
        }
        // Pictures arrive from their workers: a few more frames, the key still held.
        self.run(0.25, &mut ui, &mut view, &mut p, &settings, held);
        self.frame(&mut ui, &mut view, &mut p, &settings, Vec::new());
        let frame = self.platform.read_frame();
        let path = out.join(format!("{}.png", s.name));
        screenshot::save_png(&path, &frame).expect("write the picture");
        println!("wrote {}", path.display());
        self.platform.present();
    }
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|a| a == "--locale") {
        args.next();
        let tag = args.next().unwrap_or_default();
        let language = pley_i18n::supported(Some(&tag)).unwrap_or_else(|| panic!("not a language Pley has: {tag}"));
        pley_i18n::set_language(language);
    }
    let first = args.next().unwrap_or_default();
    let all = states();
    if first.is_empty() || first == "--list" {
        let names: Vec<&str> = all.iter().map(|s| s.name).collect();
        println!("states <folder> [state …]: {}", names.join(" "));
        return;
    }
    let out = PathBuf::from(first);
    std::fs::create_dir_all(&out).expect("make the folder");
    let wanted: Vec<String> = args.collect();
    let options = DesktopOptions { width: 1280, height: 720, vsync: false, ..DesktopOptions::default() };
    let mut platform = DesktopPlatform::new(&options).expect("open a window");
    let fonts = pley_host::assets_dir().join("fonts");
    let engine = Arc::new(TextEngine::new(FontSet::load(&fonts, &pley_text::system_font_dirs())));
    let (w, h) = platform.surface_size();
    let renderer = Renderer::new(platform.gl(), engine.clone(), (w, h), platform.scale()).expect("a renderer");
    let images = ImageCache::new(Arc::new(Pictures), platform.waker(), ImageCacheConfig::default());
    let mut events = Vec::new();
    platform.wait_events(Some(Duration::ZERO), &mut events);
    let mut shooter = Shooter { platform, engine, renderer, images };
    for s in all.iter().filter(|s| wanted.is_empty() || wanted.iter().any(|w| w == s.name)) {
        shooter.shoot(s, &out);
    }
}
