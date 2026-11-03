//! Times the text engine on this machine:
//!
//! ```text
//! cargo run -p pley-text --release --example bench
//! ```
//!
//! Laying out titles without the cache, a cache hit, rasterising glyphs, and
//! the one-off costs: loading the app's faces and indexing the system's.

use std::path::Path;
use std::time::{Duration, Instant};

use pley_base::{Align, TextOpts, TextShaper, TextStyle, Weight};
use pley_text::{FontSet, TextEngine, system_font_dirs};

const TITLES: &[&str] = &[
    "The Lord of the Rings: The Return of the King",
    "Continue watching",
    "Everything Everywhere All at Once",
    "S2 · E5 · The Long Night",
    "Spider-Man: Across the Spider-Verse",
    "Amélie",
    "Dvořák: Symphony No. 9 \"From the New World\"",
    "2001: A Space Odyssey",
    "Recently added in Movies",
    "Pirates of the Caribbean: Dead Man's Chest",
    "Mad Max: Fury Road",
    "Up next",
];

fn per_call(total: Duration, calls: usize) -> String {
    format!("{:.2} µs", total.as_secs_f64() * 1e6 / calls as f64)
}

fn main() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts");
    let start = Instant::now();
    let fonts = FontSet::load(&assets, &system_font_dirs());
    println!("load the app's faces: {:?}", start.elapsed());

    // A cache of one layout: every call below is a miss.
    let uncached = TextEngine::with_cache(fonts, 1, 1);
    let hero = TextStyle::new(38.0, 44.0, Weight::Bold).with_tracking(-0.5);
    let label = TextStyle::new(14.0, 18.0, Weight::Medium).with_tracking(0.1);
    let balanced = TextOpts::wrap(400.0, 2).balanced();
    let line = TextOpts::line(180.0).aligned(Align::Center);
    for title in TITLES {
        uncached.layout(title, &hero, &balanced);
    }
    let rounds = 200;
    let start = Instant::now();
    for _ in 0..rounds {
        for title in TITLES {
            uncached.layout(title, &hero, &balanced);
        }
    }
    println!("lay out a hero title, balanced in two lines: {}", per_call(start.elapsed(), rounds * TITLES.len()));
    let start = Instant::now();
    for _ in 0..rounds {
        for title in TITLES {
            uncached.layout(title, &label, &line);
        }
    }
    println!("lay out a label on one line, cut with \"…\": {}", per_call(start.elapsed(), rounds * TITLES.len()));

    let cached = TextEngine::new(FontSet::load(&assets, &system_font_dirs()));
    for title in TITLES {
        cached.layout(title, &label, &line);
    }
    let hits = 100_000;
    let start = Instant::now();
    for i in 0..hits {
        cached.layout(TITLES[i % TITLES.len()], &label, &line);
    }
    println!("cache hit: {}", per_call(start.elapsed(), hits));

    let layout = cached.layout("Handgloves 0123456789", &label, &TextOpts::default());
    let glyphs = &layout.glyphs;
    let rasters = 2_000;
    let start = Instant::now();
    for i in 0..rasters {
        let glyph = glyphs[i % glyphs.len()];
        std::hint::black_box(cached.rasterize(glyph.font, glyph.id, 28.0, (i % 4) as f32 / 4.0));
    }
    println!("rasterise a glyph at 28 px: {}", per_call(start.elapsed(), rasters));
    let start = Instant::now();
    for i in 0..rasters {
        let glyph = glyphs[i % glyphs.len()];
        std::hint::black_box(cached.rasterize(glyph.font, glyph.id, 76.0, 0.0));
    }
    println!("rasterise a glyph at 76 px: {}", per_call(start.elapsed(), rasters));

    let start = Instant::now();
    cached.fonts().index_system_fonts();
    println!("index the system's faces (once): {:?}", start.elapsed());
    let start = Instant::now();
    cached.layout("東京物語 שלום مرحبا", &label, &TextOpts::default());
    println!("first string needing fallback faces: {:?}", start.elapsed());
    let start = Instant::now();
    for _ in 0..rounds {
        uncached.layout("東京物語 שלום مرحبا", &label, &TextOpts::default());
        uncached.layout("Tokyo Story", &label, &TextOpts::default());
    }
    println!("lay out a mixed-script string after that: {}", per_call(start.elapsed(), rounds * 2));
}
