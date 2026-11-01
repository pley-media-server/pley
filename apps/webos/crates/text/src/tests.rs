//! The engine on the bundled Roboto (and, for scripts Roboto lacks, on the
//! system's faces when this machine has them).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use pley_base::{Align, Family, TextLayout, TextOpts, TextShaper, TextStyle, Weight};

use crate::{FontSet, SYNTHETIC_ITALIC, TextEngine, system_font_dirs};

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts")
}

/// The bundled faces alone, as on a TV without fallback faces.
fn engine() -> &'static TextEngine {
    static ENGINE: OnceLock<TextEngine> = OnceLock::new();
    ENGINE.get_or_init(|| TextEngine::new(FontSet::load(&assets(), &[])))
}

/// The bundled faces with this machine's own behind them.
fn system_engine() -> &'static TextEngine {
    static ENGINE: OnceLock<TextEngine> = OnceLock::new();
    ENGINE.get_or_init(|| TextEngine::new(FontSet::load(&assets(), &system_font_dirs())))
}

fn body() -> TextStyle {
    TextStyle::new(14.0, 20.0, Weight::Regular)
}

fn unbounded() -> TextOpts {
    TextOpts { max_width: None, max_lines: 0, ..TextOpts::default() }
}

fn wrap(width: f32) -> TextOpts {
    TextOpts { max_width: Some(width), max_lines: 0, ..TextOpts::default() }
}

fn width(text: &str, style: &TextStyle) -> f32 {
    engine().layout(text, style, &unbounded()).size.w
}

/// The characters a line draws, in display order, read back through the
/// faces' character maps.
fn line_text(engine: &TextEngine, layout: &TextLayout, line: usize) -> String {
    let line = &layout.lines[line];
    let candidates = (' '..='~').chain('\u{A0}'..='\u{17F}').chain('\u{5D0}'..='\u{5EA}').chain(['…']);
    layout.glyphs[line.glyphs.start as usize..line.glyphs.end as usize]
        .iter()
        .map(|glyph| {
            let face = engine.fonts().face(glyph.font).expect("a face for every glyph");
            candidates.clone().find(|&c| face.ttf.glyph_index(c).map(|id| id.0) == Some(glyph.id)).unwrap_or('?')
        })
        .collect()
}

fn texts(engine: &TextEngine, layout: &TextLayout) -> Vec<String> {
    (0..layout.lines.len()).map(|i| line_text(engine, layout, i)).collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn engine_is_shared_between_threads() {
    fn shared<T: Send + Sync>() {}
    shared::<TextEngine>();
}

#[test]
fn bundled_roboto_is_found() {
    let fonts = engine().fonts();
    assert!(fonts.has_family("Roboto"));
    assert!(fonts.has_family("sans-serif"));
    assert!(!fonts.has_family("No Such Face"));
    let layout = engine().layout("H", &body(), &unbounded());
    let info = fonts.info(layout.glyphs[0].font).unwrap();
    assert_eq!((info.family.as_str(), info.weight, info.italic), ("Roboto", 400, false));
    let bold = engine().layout("H", &body().with_weight(Weight::Bold), &unbounded());
    assert_eq!(fonts.info(bold.glyphs[0].font).unwrap().weight, 700);
    let medium = engine().layout("H", &body().with_weight(Weight::Medium), &unbounded());
    assert_eq!(fonts.info(medium.glyphs[0].font).unwrap().weight, 500);
}

#[test]
fn advances_follow_the_font() {
    let style = TextStyle::new(28.0, 34.0, Weight::Regular);
    // Roboto Regular's H advances 1461 of 2048 units.
    let h = 1461.0 / 2048.0 * 28.0;
    let one = engine().layout("H", &style, &unbounded());
    assert_eq!(one.glyphs.len(), 1);
    assert!(close(one.size.w, h), "{} != {h}", one.size.w);
    assert!(close(one.lines[0].width, h));
    assert!(close(width("HH", &style), 2.0 * h));
    // Kerning from GPOS: "AV" sits tighter than its letters apart.
    assert!(width("AV", &style) < width("A", &style) + width("V", &style) - 0.5);
    // Glyphs are placed along the line from its left.
    let two = engine().layout("HH", &style, &unbounded());
    assert!(close(two.glyphs[0].x, 0.0) && close(two.glyphs[1].x, h));
    assert_eq!(two.glyphs[0].size, 28.0);
}

#[test]
fn lines_wrap_at_spaces() {
    let style = body();
    let max = width("aaa aaa", &style) + 0.5;
    let layout = engine().layout("aaa aaa aaa aaa", &style, &wrap(max));
    assert_eq!(texts(engine(), &layout), ["aaa aaa", "aaa aaa"]);
    assert!(layout.lines.iter().all(|l| l.width <= max));
    assert!(!layout.truncated);
}

#[test]
fn lines_wrap_after_hyphens_without_adding_any() {
    let style = body();
    let max = width("well-known", &style) - 1.0;
    let layout = engine().layout("well-known", &style, &wrap(max));
    assert_eq!(texts(engine(), &layout), ["well-", "known"]);
    assert_eq!(layout.glyphs.len(), "well-known".len());
}

#[test]
fn a_word_too_long_breaks_between_letters_without_a_hyphen() {
    let style = body();
    let word = "Supercalifragilisticexpialidocious";
    let layout = engine().layout(word, &style, &wrap(60.0));
    assert!(layout.lines.len() > 2);
    assert!(layout.lines.iter().all(|l| l.width <= 60.0 + 1e-3));
    assert_eq!(texts(engine(), &layout).concat(), word);
    // A soft hyphen neither shows nor offers a break that would need one.
    let soft = engine().layout("co\u{AD}operation", &style, &unbounded());
    assert!(close(soft.size.w, width("cooperation", &style)));
    let hyphen = engine().layout("-", &style, &unbounded()).glyphs[0].id;
    let narrow = engine().layout("co\u{AD}operation co\u{AD}operation", &style, &wrap(80.0));
    assert!(narrow.glyphs.iter().all(|g| g.id != hyphen));
}

#[test]
fn one_line_is_cut_with_an_ellipsis() {
    let style = body();
    let text = "The quick brown fox jumps over the lazy dog";
    let layout = engine().layout(text, &style, &TextOpts::line(100.0));
    assert_eq!(layout.lines.len(), 1);
    assert!(layout.truncated);
    assert!(layout.lines[0].width <= 100.0 + 1e-3);
    let line = line_text(engine(), &layout, 0);
    assert!(line.ends_with('…') && text.starts_with(line.trim_end_matches('…')), "{line}");
    // What fits stays whole.
    let short = engine().layout("The quick", &style, &TextOpts::line(100.0));
    assert!(!short.truncated);
    assert_eq!(line_text(engine(), &short, 0), "The quick");
    // Without an ellipsis the line is only cut.
    let cut = TextOpts { ellipsis: false, ..TextOpts::line(100.0) };
    let layout = engine().layout(text, &style, &cut);
    assert!(layout.truncated && layout.lines[0].width <= 100.0 + 1e-3);
    assert!(!line_text(engine(), &layout, 0).contains('…'));
}

#[test]
fn a_clamped_last_line_holds_the_rest_of_its_paragraph() {
    let style = body();
    let max = width("aaa aaa", &style) + 2.0;
    let text = "x aaa aaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let layout = engine().layout(text, &style, &TextOpts::wrap(max, 2));
    let lines = texts(engine(), &layout);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "x aaa");
    // As Android's StaticLayout does: the long word runs on up to the "…".
    assert!(lines[1].starts_with("aaa b") && lines[1].ends_with('…'), "{lines:?}");
    assert!(layout.lines[1].width <= max + 1e-3);
    assert!(layout.truncated);
    // Without the ellipsis, the lines are kept as they wrapped.
    let plain = engine().layout(text, &style, &TextOpts { ellipsis: false, ..TextOpts::wrap(max, 2) });
    assert_eq!(texts(engine(), &plain), ["x aaa", "aaa"]);
    assert!(plain.truncated);
}

#[test]
fn balanced_titles_even_their_lines() {
    let style = TextStyle::new(28.0, 34.0, Weight::Bold);
    let text = "aaaa bbbb cccc dddd eeee";
    let max = width("aaaa bbbb cccc dddd", &style) + 1.0;
    let greedy = engine().layout(text, &style, &TextOpts::wrap(max, 3));
    let balanced = engine().layout(text, &style, &TextOpts::wrap(max, 3).balanced());
    assert_eq!(greedy.lines.len(), 2);
    assert_eq!(balanced.lines.len(), 2);
    let spread = |l: &TextLayout| (l.lines[0].width - l.lines[1].width).abs();
    assert!(spread(&balanced) < spread(&greedy) / 2.0);
    assert!(balanced.lines.iter().all(|l| l.width <= max));
    assert_eq!(texts(engine(), &balanced).join(" "), text);
    // A title that fits on one line stays on one.
    let short = engine().layout("aaaa bbbb", &style, &TextOpts::wrap(max, 3).balanced());
    assert_eq!(short.lines.len(), 1);
}

#[test]
fn lines_align_within_the_width() {
    let style = body();
    let w = width("abc", &style);
    let start = engine().layout("abc", &style, &TextOpts::line(200.0));
    assert_eq!(start.lines[0].x, 0.0);
    assert!(close(start.size.w, w));
    let center = engine().layout("abc", &style, &TextOpts::line(200.0).aligned(Align::Center));
    assert!(close(center.lines[0].x, (200.0 - w) / 2.0));
    assert!(close(center.glyphs[0].x, center.lines[0].x));
    assert_eq!(center.size.w, 200.0);
    let end = engine().layout("abc", &style, &TextOpts::line(200.0).aligned(Align::End));
    assert!(close(end.lines[0].x, 200.0 - w));
    // Without a width, lines align within the widest.
    let lines = engine().layout("abc\na", &style, &unbounded().aligned(Align::End));
    assert!(close(lines.size.w, w));
    assert!(close(lines.lines[1].x, w - width("a", &style)));
}

#[test]
fn tracking_spaces_every_letter_half_before_and_half_after() {
    let style = body();
    let plain = engine().layout("HHHH", &style, &unbounded());
    let spaced = engine().layout("HHHH", &style.with_tracking(2.0), &unbounded());
    assert!(close(spaced.size.w, plain.size.w + 4.0 * 2.0));
    assert!(close(spaced.glyphs[0].x, 1.0));
    assert!(close(spaced.glyphs[1].x - spaced.glyphs[0].x, plain.glyphs[1].x + 2.0));
    let negative = engine().layout("HHHH", &style.with_tracking(-0.5), &unbounded());
    assert!(close(negative.size.w, plain.size.w - 2.0));
    // Spaced text has no ligatures (Android turns them off).
    let fi = engine().layout("fi", &style, &unbounded());
    let spaced_fi = engine().layout("fi", &style.with_tracking(0.4), &unbounded());
    assert_eq!((fi.glyphs.len(), spaced_fi.glyphs.len()), (1, 2));
}

#[test]
fn glyphs_are_centred_in_line_boxes() {
    let style = body();
    let layout = engine().layout("Hg\nHg", &style, &unbounded());
    assert_eq!(layout.lines.len(), 2);
    // Roboto's line is 1900 above and 500 below of 2048; the box's spare
    // height is split evenly above and below.
    let (ascent, descent) = (14.0 * 1900.0 / 2048.0, 14.0 * 500.0 / 2048.0);
    let baseline = (20.0 - ascent - descent) / 2.0 + ascent;
    assert!(close(layout.lines[0].top, 0.0) && close(layout.lines[0].baseline, baseline));
    assert!(close(layout.lines[1].top, 20.0) && close(layout.lines[1].baseline, 20.0 + baseline));
    assert!(close(layout.first_baseline(), baseline));
    assert!(close(layout.size.h, 40.0));
    assert!(layout.glyphs[..2].iter().all(|g| close(g.y, baseline)));
    assert!(layout.glyphs[2..].iter().all(|g| close(g.y, 20.0 + baseline)));
    let metrics = engine().metrics(&style);
    assert!(close(metrics.baseline, baseline) && close(metrics.ascent, ascent));
    assert!(close(metrics.cap_height, 14.0 * 1456.0 / 2048.0));
    // A box smaller than the font overflows evenly (Trim.None).
    let pin = TextStyle::new(28.0, 28.0, Weight::Medium);
    let pin_baseline = engine().layout("1", &pin, &unbounded()).lines[0].baseline;
    let (a, d) = (28.0 * 1900.0 / 2048.0, 28.0 * 500.0 / 2048.0);
    assert!(close(pin_baseline, (28.0 - a - d) / 2.0 + a));
}

#[test]
fn line_breaks_start_lines() {
    let style = body();
    assert_eq!(texts(engine(), &engine().layout("a\nb", &style, &unbounded())), ["a", "b"]);
    assert_eq!(texts(engine(), &engine().layout("a\r\nb", &style, &unbounded())), ["a", "b"]);
    assert_eq!(texts(engine(), &engine().layout("a\n\nb", &style, &unbounded())), ["a", "", "b"]);
    assert_eq!(engine().layout("a\n", &style, &unbounded()).lines.len(), 2);
    let empty = engine().layout("", &style, &unbounded());
    assert_eq!((empty.lines.len(), empty.glyphs.len()), (1, 0));
    assert!(close(empty.size.h, 20.0) && empty.size.w == 0.0);
    // One line shows the first, with "…" for the rest.
    let one = engine().layout("alpha\nbeta", &style, &TextOpts::default());
    assert_eq!(texts(engine(), &one), ["alpha…"]);
    assert!(one.truncated);
}

#[test]
fn lines_beyond_the_limit_are_dropped() {
    let style = body();
    let layout = engine().layout("one\ntwo\nthree", &style, &TextOpts { max_lines: 2, ..unbounded() });
    assert_eq!(texts(engine(), &layout), ["one", "two…"]);
    assert!(layout.truncated);
    let all = engine().layout("one\ntwo", &style, &TextOpts { max_lines: 2, ..unbounded() });
    assert!(!all.truncated);
}

#[test]
fn latin_extended_letters_come_from_the_second_file() {
    let layout = engine().layout("Dvořák", &body(), &unbounded());
    assert!(layout.glyphs.iter().all(|g| g.id != 0), "no missing glyphs");
    let fonts: Vec<u16> = layout.glyphs.iter().map(|g| g.font).collect();
    assert!(fonts.iter().any(|&f| f != fonts[0]), "ř is in the Latin Extended file");
    assert_eq!(line_text(engine(), &layout, 0), "Dvořák");
}

#[test]
fn italic_without_an_italic_face_is_slanted() {
    let style = TextStyle { italic: true, ..TextStyle::new(28.0, 34.0, Weight::Regular) };
    let layout = engine().layout("H", &style, &unbounded());
    let glyph = layout.glyphs[0];
    assert_ne!(glyph.font & SYNTHETIC_ITALIC, 0);
    let upright = engine().rasterize(glyph.font & !SYNTHETIC_ITALIC, glyph.id, 28.0, 0.0).unwrap();
    let slanted = engine().rasterize(glyph.font, glyph.id, 28.0, 0.0).unwrap();
    // The top leans right by a quarter of the height.
    assert!(slanted.width >= upright.width + 4);
    assert_eq!(slanted.height, upright.height);
}

#[test]
fn named_and_monospace_families() {
    let style = body();
    let named = TextStyle { family: Family::Named(Arc::from("No Such Face")), ..body() };
    let a = engine().layout("Hello", &style, &unbounded());
    let b = engine().layout("Hello", &named, &unbounded());
    assert_eq!(a.glyphs, b.glyphs);
    // Without a monospace face, Roboto's digits stand in, all one width.
    let mono = TextStyle { family: Family::Mono, ..TextStyle::new(48.0, 56.0, Weight::Bold) };
    let digits = engine().layout("0123456789", &mono, &unbounded());
    let steps: Vec<f32> = digits.glyphs.windows(2).map(|w| w[1].x - w[0].x).collect();
    assert!(steps.iter().all(|s| close(*s, steps[0])), "{steps:?}");
}

#[test]
fn the_cache_returns_the_same_layout() {
    let engine = TextEngine::new(FontSet::load(&assets(), &[]));
    let style = body();
    let a = engine.layout("Continue watching", &style, &TextOpts::line(200.0));
    let b = engine.layout("Continue watching", &style, &TextOpts::line(200.0));
    assert!(Arc::ptr_eq(&a, &b));
    let c = engine.layout("Continue watching", &style, &TextOpts::line(100.0));
    assert!(!Arc::ptr_eq(&a, &c));
    assert_eq!(engine.cached_layouts(), 2);
    engine.clear_cache();
    let d = engine.layout("Continue watching", &style, &TextOpts::line(200.0));
    assert!(!Arc::ptr_eq(&a, &d));
    assert_eq!(*a, *d);
}

#[test]
fn h_rasterises_at_28_px() {
    let layout = engine().layout("H", &TextStyle::new(28.0, 34.0, Weight::Regular), &unbounded());
    let glyph = layout.glyphs[0];
    let bitmap = engine().rasterize(glyph.font, glyph.id, 28.0, 0.0).unwrap();
    // Roboto's H: ink from 169 to 1288 units across and 1456 up, of 2048.
    assert_eq!((bitmap.left, bitmap.top), (2, -20));
    assert_eq!(bitmap.height, 20);
    assert!((15..=17).contains(&bitmap.width), "{}", bitmap.width);
    assert_eq!(bitmap.coverage.len(), (bitmap.width * bitmap.height) as usize);
    let at = |x: u32, y: u32| bitmap.coverage[(y * bitmap.width + x) as usize];
    // Inside the left stem, between the stems above the bar, on the bar.
    assert_eq!(at(1, 2), 255);
    assert_eq!(at(bitmap.width / 2, 2), 0);
    assert_eq!(at(bitmap.width / 2, 9), 255);
    // A quarter-pixel shift moves the ink, not the stems' coverage sum.
    let shifted = engine().rasterize(glyph.font, glyph.id, 28.0, 0.5).unwrap();
    assert_ne!(shifted.coverage, bitmap.coverage);
    let ink = |b: &crate::GlyphBitmap| b.coverage.iter().map(|&c| u32::from(c)).sum::<u32>();
    assert!(ink(&shifted).abs_diff(ink(&bitmap)) < ink(&bitmap) / 50);
    // A space has no ink (and a line's trailing spaces aren't placed at all).
    let spaced = engine().layout("H H ", &TextStyle::new(28.0, 34.0, Weight::Regular), &unbounded());
    assert_eq!(spaced.glyphs.len(), 3);
    assert!(engine().rasterize(spaced.glyphs[1].font, spaced.glyphs[1].id, 28.0, 0.0).is_none());
}

#[test]
fn right_to_left_text_reads_in_display_order() {
    let engine = system_engine();
    let style = body();
    let probe = engine.layout("ש", &style, &unbounded());
    if engine.fonts().info(probe.glyphs[0].font).is_none_or(|info| info.family == "Roboto") {
        eprintln!("no system face with Hebrew here; skipped");
        return;
    }
    let mixed = engine.layout("abc שלום", &style, &unbounded());
    assert_eq!(line_text(engine, &mixed, 0), "abc םולש");
    // A paragraph that starts right to left starts on the right.
    let rtl = engine.layout("שלום abc", &style, &TextOpts::line(300.0));
    assert_eq!(line_text(engine, &rtl, 0), "abc םולש");
    assert!(close(rtl.lines[0].x + rtl.lines[0].width, 300.0));
    // Its ellipsis goes on its left, where the text ends.
    let cut = engine.layout("שלום שלום שלום שלום שלום שלום", &style, &TextOpts::line(60.0));
    let line = line_text(engine, &cut, 0);
    assert!(line.starts_with('…') && cut.truncated, "{line}");
    // Glyphs run left to right across the line.
    assert!(mixed.glyphs.windows(2).all(|w| w[1].x >= w[0].x));
}

#[test]
fn other_scripts_fall_back_to_system_faces() {
    let engine = system_engine();
    let layout = engine.layout("Tokyo 東京", &body(), &unbounded());
    let last = layout.glyphs.last().unwrap();
    let Some(info) = engine.fonts().info(last.font).filter(|info| info.family != "Roboto") else {
        eprintln!("no system face with Han here; skipped");
        return;
    };
    assert_ne!(last.id, 0, "drawn by {}", info.family);
    // Every face shares Roboto's baseline.
    assert!(layout.glyphs.iter().all(|g| close(g.y, layout.lines[0].baseline)));
    assert!(engine.rasterize(last.font, last.id, 28.0, 0.0).is_some());
}

#[test]
fn fallback_searches_every_file_of_a_split_family() {
    // Roboto ships as a Latin file and a Latin Extended one: a letter only the
    // second has still comes from Roboto, in the weight asked for.
    let fonts = engine().fonts();
    let face = fonts.fallback('ř', 700, false).expect("Latin Extended covers ř");
    let info = fonts.info(face.id).unwrap();
    assert_eq!((info.family.as_str(), info.weight), ("Roboto", 700));
    assert!(fonts.fallback('東', 400, false).is_none(), "no fallback faces here");
}

#[test]
fn without_any_face_text_still_has_lines() {
    let engine = TextEngine::new(FontSet::load(Path::new("no such folder"), &[]));
    let layout = engine.layout("Pley\nLG", &body(), &unbounded());
    assert_eq!(layout.lines.len(), 2);
    assert!(layout.glyphs.is_empty());
    assert!(close(layout.size.h, 40.0));
    assert!(engine.rasterize(0, 1, 28.0, 0.0).is_none());
}

#[test]
fn families_named_from_the_system() {
    let engine = system_engine();
    let fonts = engine.fonts();
    if !fonts.has_family("serif") {
        eprintln!("no serif face here; skipped");
        return;
    }
    let serif = TextStyle { family: Family::Named(Arc::from("serif")), ..body().with_weight(Weight::Bold) };
    let layout = engine.layout("Subtitle", &serif, &unbounded());
    let info = fonts.info(layout.glyphs[0].font).unwrap();
    assert!(crate::families::SERIF.contains(&info.family.to_lowercase().as_str()), "{}", info.family);
    assert!(info.weight >= 600, "the bold face of {}", info.family);
}
