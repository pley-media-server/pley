//! The renderer's test page: every type style of the design system in its
//! colours, fills, outlines and radii, a focused tile with its ring, a
//! backdrop with its scrims, the brand glow, every icon, a picture from a file
//! and a punched-through rectangle.
//!
//! `cargo run -p pley-gfx --example gallery [-- --size 1920x1080] [--picture file]
//! [--screenshot out.png] [--bench frames] [--atlas side]`
//!
//! A small `--atlas` page (256) makes the atlas fill up mid-frame, to see
//! that a frame drawn across atlas resets still looks the same.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pley_base::{
    Cmd, Color, DisplayList, Family, Fetch, FetchError, GradientStop, Icon, ImageProvider, ImageRequest, ImageState,
    Point, Priority, Rect, TextOpts, TextShaper, TextStyle, Weight, time,
};
use pley_gfx::{ImageCache, ImageCacheConfig, Renderer, icons, screenshot};
use pley_platform::desktop::{DesktopOptions, DesktopPlatform};
use pley_platform::{Platform, PlatformEvent};
use pley_text::{FontSet, TextEngine};

const BG: Color = Color::rgb(0x0b0d10);
const SURFACE2: Color = Color::rgb(0x191d24);
const SURFACE3: Color = Color::rgb(0x222731);
const SURFACE4: Color = Color::rgb(0x2a303b);
const CONTROL: Color = Color::rgb(0x6c7584);
const FG: Color = Color::rgb(0xe9ebef);
const MUTED: Color = Color::rgb(0xa1a9b6);
const SUBTLE: Color = Color::rgb(0x8b95a5);
const ACCENT: Color = Color::rgb(0x38bdf8);
const ACCENT_BRIGHT: Color = Color::rgb(0x7dd3fc);
const ON_ACCENT: Color = Color::rgb(0x03131f);
const DANGER: Color = Color::rgb(0xe5484d);
const SUCCESS: Color = Color::rgb(0x3fb27f);
const BRAND_DEEP: Color = Color::rgb(0x0284c7);

struct Args {
    size: (u32, u32),
    picture: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    bench: Option<usize>,
    atlas: u32,
}

fn args() -> Args {
    let mut a = Args { size: (1280, 720), picture: None, screenshot: None, bench: None, atlas: 2048 };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--size" => {
                let v = it.next().expect("--size WxH");
                let (w, h) = v.split_once('x').expect("--size WxH");
                a.size = (w.parse().expect("width"), h.parse().expect("height"));
            }
            "--picture" => a.picture = it.next().map(PathBuf::from),
            "--screenshot" => a.screenshot = it.next().map(PathBuf::from),
            "--bench" => a.bench = it.next().and_then(|n| n.parse().ok()),
            "--atlas" => a.atlas = it.next().and_then(|n| n.parse().ok()).expect("--atlas side"),
            other => panic!("unknown argument {other}"),
        }
    }
    a
}

/// Reads pictures from files: the address is a path.
struct Files;

impl Fetch for Files {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        std::fs::read(url).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => FetchError::Missing,
            _ => FetchError::Failed(e.to_string()),
        })
    }
}

/// A design-system type style.
fn style(size: f32, line: f32, weight: Weight, tracking: f32) -> TextStyle {
    TextStyle::new(size, line, weight).with_tracking(tracking)
}

struct Page<'a> {
    text: &'a TextEngine,
    list: DisplayList,
}

impl Page<'_> {
    fn text(&mut self, s: &str, st: &TextStyle, x: f32, y: f32, color: Color) -> f32 {
        let layout = self.text.layout(s, st, &TextOpts::default());
        let h = layout.size.h;
        self.list.push(Cmd::Text { origin: Point::new(x, y), layout, color });
        h
    }

    fn fill(&mut self, rect: Rect, color: Color, radius: f32) {
        self.list.push(Cmd::Fill { rect, color, radius });
    }

    fn stroke(&mut self, rect: Rect, color: Color, radius: f32, width: f32) {
        self.list.push(Cmd::Stroke { rect, color, radius, width });
    }
}

/// Draws the page: what the interface's list would be.
fn page(text: &TextEngine, images: &mut ImageCache, picture: &str, scale: f32) -> (DisplayList, bool) {
    let mut p = Page { text, list: DisplayList::new(BG) };
    let mut ready = true;
    let mut picture_at = |images: &mut ImageCache, width_dp: f32| {
        let req = ImageRequest::new(picture, (width_dp * scale).round() as u32);
        match images.request(&req, Priority::Visible) {
            ImageState::Ready { id, width, height, since } => Some((id, width, height, since)),
            ImageState::Loading => {
                ready = false;
                None
            }
            ImageState::Failed => None,
        }
    };

    // The backdrop, its left and bottom scrims (`TvSurfaces.kt`: Rows, 120 → 300).
    if let Some((id, w, h, since)) = picture_at(images, 960.0) {
        let fade = (((time::now() - since) / 0.15) as f32).clamp(0.0, 1.0);
        p.list.push(Cmd::Image {
            rect: Rect::new(0.0, 0.0, 960.0, 540.0),
            image: id,
            src: Rect::new(0.0, 0.0, w as f32, h as f32),
            radius: 0.0,
            opacity: fade,
        });
    }
    p.list.push(Cmd::LinearGradient {
        rect: Rect::new(0.0, 0.0, 800.0, 540.0),
        vertical: false,
        stops: vec![
            GradientStop::new(0.0, BG),
            GradientStop::new(300.0 / 800.0, BG.alpha(0.92)),
            GradientStop::new(560.0 / 800.0, BG.alpha(0.85)),
            GradientStop::new(1.0, BG.alpha(0.0)),
        ],
    });
    p.list.push(Cmd::LinearGradient {
        rect: Rect::new(0.0, 120.0, 960.0, 180.0),
        vertical: true,
        stops: vec![GradientStop::new(0.0, BG.alpha(0.0)), GradientStop::new(1.0, BG)],
    });
    p.fill(Rect::new(0.0, 300.0, 960.0, 240.0), BG, 0.0);
    // The brand glow of the onboarding screens.
    p.list.push(Cmd::RadialGradient {
        rect: Rect::new(0.0, 0.0, 960.0, 540.0),
        center: Point::new(960.0, 540.0),
        radius: 576.0,
        inner: BRAND_DEEP.alpha(0.18),
        outer: Color::TRANSPARENT,
    });

    // Every type style (`PleyType.Tv`).
    let mut y = 18.0;
    for (name, st, color) in [
        ("HeroTitle 38/44", style(38.0, 44.0, Weight::Bold, -0.5), FG),
        ("PageTitle 28/34", style(28.0, 34.0, Weight::Bold, -0.25), FG),
        ("Section 18/24", style(18.0, 24.0, Weight::Bold, 0.0), FG),
        ("Title 16/22", style(16.0, 22.0, Weight::Bold, 0.0), FG),
        ("BodyLarge 15/22: hero summaries", style(15.0, 22.0, Weight::Regular, 0.0), FG.alpha(0.85)),
        ("Body 14/20: lists, panel rows", style(14.0, 20.0, Weight::Regular, 0.0), MUTED),
        ("Label 14/18 · 2024 · 2 h 14 min", style(14.0, 18.0, Weight::Medium, 0.1), MUTED),
        ("Caption 13/17: tile captions", style(13.0, 17.0, Weight::Medium, 0.2), SUBTLE),
        ("Badge 11/13 · 1080p", style(11.0, 13.0, Weight::Bold, 0.4), ACCENT),
    ] {
        y += p.text(name, &st, 48.0, y, color);
    }
    let code = TextStyle { family: Family::Mono, ..style(48.0, 56.0, Weight::Bold, 4.0) };
    p.text("4F7K-29QX", &code, 520.0, 24.0, FG);
    p.text("1 2 3 4", &style(28.0, 28.0, Weight::Medium, 0.0), 520.0, 92.0, ACCENT_BRIGHT);

    // A punched-through rectangle: the video plane shows there.
    let hole = Rect::new(800.0, 140.0, 128.0, 72.0);
    p.list.push(Cmd::Punch { rect: hole });
    p.stroke(hole, ACCENT_BRIGHT, 0.0, 1.0);
    p.text("Punch", &style(13.0, 17.0, Weight::Medium, 0.2), 800.0, 216.0, SUBTLE);

    // Buttons: fills, outlines and radii with their labels.
    let label = style(14.0, 18.0, Weight::Medium, 0.1);
    let row = 244.0;
    let play = Rect::new(48.0, row, 112.0, 40.0);
    p.list.push(Cmd::PushTransform { offset: Point::ZERO, scale: 1.05, pivot: play.center() });
    p.fill(play, ACCENT, 6.0);
    p.list.push(Cmd::Icon {
        rect: Rect::new(play.x + 16.0, row + 10.0, 20.0, 20.0),
        icon: Icon::Play,
        color: ON_ACCENT,
    });
    p.text("Play", &label, play.x + 44.0, row + 11.0, ON_ACCENT);
    p.list.push(Cmd::PopTransform);
    let trailer = Rect::new(176.0, row, 112.0, 40.0);
    p.fill(trailer, FG.alpha(0.12), 6.0);
    p.stroke(trailer.expand(-0.5), CONTROL, 5.5, 1.0);
    p.list.push(Cmd::Icon { rect: Rect::new(trailer.x + 16.0, row + 10.0, 20.0, 20.0), icon: Icon::Film, color: FG });
    p.text("Trailer", &label, trailer.x + 44.0, row + 11.0, FG);
    let round = Rect::new(298.0, row, 40.0, 40.0);
    p.fill(round, BG.alpha(0.8), 6.0);
    p.stroke(round.expand(-0.5), CONTROL, 5.5, 1.0);
    p.list.push(Cmd::Icon { rect: Rect::new(round.x + 10.0, row + 10.0, 20.0, 20.0), icon: Icon::Ellipsis, color: FG });
    p.fill(Rect::new(350.0, row + 8.0, 48.0, 24.0), SURFACE3, 6.0);
    p.text("4K", &label, 364.0, row + 11.0, FG);
    p.fill(Rect::new(408.0, row + 12.0, 38.0, 16.0), DANGER, 2.0);
    p.text("LIVE", &style(11.0, 13.0, Weight::Bold, 0.4), 413.0, row + 13.5, FG);
    // A text field focused (2 dp AccentBright edge) and a progress line.
    let field = Rect::new(460.0, row, 200.0, 40.0);
    p.fill(field, SURFACE2, 4.0);
    p.stroke(field.expand(-1.0), ACCENT_BRIGHT, 3.0, 2.0);
    p.list.push(Cmd::Icon {
        rect: Rect::new(field.x + 12.0, row + 10.0, 20.0, 20.0),
        icon: Icon::Search,
        color: MUTED,
    });
    p.text("Search", &style(14.0, 20.0, Weight::Regular, 0.0), field.x + 40.0, row + 10.0, MUTED);
    p.fill(Rect::new(680.0, row + 18.0, 240.0, 4.0), SURFACE4, 2.0);
    p.fill(Rect::new(680.0, row + 18.0, 150.0, 4.0), ACCENT, 2.0);

    // Radii 2, 4, 6, 8 and a circle.
    for (i, (color, radius)) in
        [(SURFACE2, 2.0), (SURFACE3, 4.0), (SURFACE4, 6.0), (ACCENT, 8.0), (SUCCESS, 16.0)].into_iter().enumerate()
    {
        p.fill(Rect::new(48.0 + i as f32 * 42.0, 300.0, 32.0, 32.0), color, radius);
    }

    // Posters, the middle one focused: 1.08 with its 3 dp ring 4.5 dp outside
    // the grown edge (drawn in the unscaled box, so divided by the scale).
    let (tw, th) = (100.0, 150.0);
    for i in 0..3 {
        let tile = Rect::new(48.0 + i as f32 * (tw + 14.0), 352.0, tw, th);
        let focused = i == 1;
        if focused {
            p.list.push(Cmd::PushTransform { offset: Point::ZERO, scale: 1.08, pivot: tile.center() });
        }
        p.fill(tile, SURFACE2, 4.0);
        // The backdrop's picture, cropped: a poster from the server would be
        // asked at the tile's width.
        if let Some((id, w, h, _)) = picture_at(images, 960.0) {
            // Cover: the middle of the landscape picture at 2:3.
            let (w, h) = (w as f32, h as f32);
            let cw = h * tw / th;
            let src = Rect::new((w - cw) * (0.3 + 0.2 * i as f32), 0.0, cw, h);
            p.list.push(Cmd::Image { rect: tile, image: id, src, radius: 4.0, opacity: 1.0 });
        }
        p.stroke(tile.expand(-0.5), Color::WHITE.alpha(0.06), 3.5, 1.0);
        if focused {
            let s = 1.08;
            p.stroke(tile.expand(4.5 / s), ACCENT_BRIGHT, 8.5 / s, 3.0 / s);
            p.list.push(Cmd::PopTransform);
        }
        let weight = if focused { Weight::Bold } else { Weight::Medium };
        p.text(["Dune", "Arrival", "Heat"][i], &style(13.0, 17.0, weight, 0.2), tile.x, 512.0, FG);
    }

    // Every icon at 24 dp.
    let (x0, y0, pitch, per_row) = (400.0, 346.0, 34.0, 16);
    for (i, icon) in icons::ALL.iter().enumerate() {
        let (col, line) = ((i % per_row) as f32, (i / per_row) as f32);
        let color = if icon.filled() { ACCENT } else { FG };
        p.list.push(Cmd::Icon { rect: Rect::new(x0 + col * pitch, y0 + line * pitch, 24.0, 24.0), icon: *icon, color });
    }
    (p.list, ready)
}

/// Writes a landscape test picture (sky, sun, hills) as PNG.
fn make_picture(path: &Path) {
    let (w, h) = (1280u32, 720u32);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let t = y as f32 / h as f32;
        for x in 0..w {
            let mut c = [30.0 + 200.0 * t.powf(1.5), 60.0 + 90.0 * t, 140.0 - 60.0 * t];
            let d = ((x as f32 - 860.0).powi(2) + (y as f32 - 300.0).powi(2)).sqrt();
            if d < 90.0 {
                c = [255.0, 214.0, 120.0];
            }
            let hill = 470.0 + 60.0 * (x as f32 / 140.0).sin() + 25.0 * (x as f32 / 47.0 + 1.0).sin();
            if y as f32 > hill {
                let s = ((y as f32 - hill) / 250.0).min(1.0);
                c = [40.0 - 25.0 * s, 90.0 - 50.0 * s, 60.0 - 35.0 * s];
            }
            pixels.extend(c.iter().map(|v| v.clamp(0.0, 255.0) as u8));
            pixels.push(255);
        }
    }
    let image = pley_base::RgbaImage { width: w, height: h, pixels };
    screenshot::save_png(path, &image).expect("write the test picture");
}

fn main() {
    let args = args();
    let picture = args.picture.clone().unwrap_or_else(|| {
        let path = std::env::temp_dir().join("pley-gallery-picture.png");
        if !path.exists() {
            make_picture(&path);
        }
        path
    });
    let picture = picture.to_string_lossy().into_owned();

    let mut platform = DesktopPlatform::new(&DesktopOptions {
        width: args.size.0,
        height: args.size.1,
        vsync: args.bench.is_none(),
        ..DesktopOptions::default()
    })
    .expect("open the window");
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts");
    let engine = Arc::new(TextEngine::new(FontSet::load(&fonts, &pley_text::system_font_dirs())));
    let (w, h) = platform.surface_size();
    let mut renderer = Renderer::with_atlas(platform.gl(), engine.clone(), (w, h), platform.scale(), args.atlas)
        .expect("start the renderer");
    let mut images = ImageCache::new(Arc::new(Files), platform.waker(), ImageCacheConfig::default());

    let mut events = Vec::new();
    let mut frames = 0usize;
    let mut settled = 0;
    let mut timings: Vec<(Duration, Duration, Duration)> = Vec::new();
    loop {
        let animate = args.bench.is_some() || args.screenshot.is_some();
        platform
            .wait_events(if animate { Some(Duration::ZERO) } else { Some(Duration::from_millis(500)) }, &mut events);
        for e in events.drain(..) {
            match e {
                PlatformEvent::Quit => return,
                PlatformEvent::Resized(w, h) => renderer.resize(w, h, platform.scale()),
                _ => {}
            }
        }
        renderer.prepare(&mut images);
        let t = Instant::now();
        let (list, ready) = page(&engine, &mut images, &picture, renderer.scale());
        let list_time = t.elapsed();
        let stats = renderer.render(&list, &mut images);
        frames += 1;
        if frames > 30 {
            timings.push((list_time, stats.build, stats.submit));
        }
        if let Some(path) = &args.screenshot {
            settled = if ready && !stats.more { settled + 1 } else { 0 };
            // A few settled frames let the pictures' fade-in finish.
            if settled > 12 || frames > 600 {
                let frame = platform.read_frame();
                screenshot::save_png(path, &frame).expect("write the screenshot");
                println!(
                    "{} quads in {} draw calls, {} atlas resets; wrote {}",
                    stats.quads,
                    stats.draw_calls,
                    stats.atlas_resets,
                    path.display()
                );
                return;
            }
            if settled > 0 {
                std::thread::sleep(Duration::from_millis(16));
            }
        }
        platform.present();
        if let Some(n) = args.bench
            && timings.len() >= n
        {
            report(&timings, &stats);
            return;
        }
    }
}

fn report(timings: &[(Duration, Duration, Duration)], last: &pley_gfx::FrameStats) {
    let stat = |pick: fn(&(Duration, Duration, Duration)) -> Duration| {
        let mut v: Vec<f64> = timings.iter().map(|t| pick(t).as_secs_f64() * 1e3).collect();
        v.sort_by(f64::total_cmp);
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        (mean, v[v.len() / 2], v[v.len() * 95 / 100])
    };
    for (name, s) in [
        ("display list", stat(|t| t.0)),
        ("render: quads", stat(|t| t.1)),
        ("render: GL calls", stat(|t| t.2)),
        ("CPU total", stat(|t| t.0 + t.1 + t.2)),
    ] {
        println!("{name:>16}: mean {:.3} ms, median {:.3} ms, p95 {:.3} ms", s.0, s.1, s.2);
    }
    println!("{} frames; {} quads in {} draw calls", timings.len(), last.quads, last.draw_calls);
}
