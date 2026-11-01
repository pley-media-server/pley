//! What the player writes: times, the episode line, chapters, speeds, the subtitle search's rows
//! and messages, and the colour of how a video plays (the Android TV app's `TvPlayerText`,
//! `TvPlayerControls.kt`, `strings.xml`; the TV app's `ui/player/panels.tsx`).

use pley_base::Color;
use pley_core::ApiError;
use pley_core::format::{grouped, language_name};
use pley_core::model::SubtitleResult;
use pley_player::controller::ChapterMark;
use pley_ui::theme;

/// "4:05", "32:40", "1:02:03".
pub fn time(ms: i64) -> String {
    let total = ms.max(0) / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// What is left, with a real minus: "−25:20".
pub fn remaining(position_ms: i64, duration_ms: i64) -> String {
    format!("−{}", time((duration_ms - position_ms).max(0)))
}

/// An episode's line as the player shows it: "S1 E2 — Pilot" and "S1 E2 · Pilot" become
/// "S1 · E2 · Pilot" (`TvPlayerText.episodeLine`).
pub fn episode_line(subtitle: Option<&str>) -> Option<String> {
    let s = subtitle.map(str::trim).filter(|s| !s.is_empty())?;
    let mut out = String::with_capacity(s.len() + 4);
    let words: Vec<&str> = s.split(' ').collect();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let next = words.get(i + 1).copied();
        let numbered = |w: &str, p: char| w.len() > 1 && w.starts_with(p) && w[1..].bytes().all(|b| b.is_ascii_digit());
        if !out.is_empty() {
            out.push(' ');
        }
        if numbered(w, 'S') && next.is_some_and(|n| numbered(n, 'E')) {
            out.push_str(w);
            out.push_str(" · ");
            out.push_str(next.unwrap_or_default());
            i += 2;
            continue;
        }
        out.push_str(if w == "—" { "·" } else { w });
        i += 1;
    }
    Some(out)
}

/// The chapter playing at `position_ms`: the last one started, while inside it.
pub fn chapter_at(chapters: &[ChapterMark], position_ms: i64) -> Option<usize> {
    let i = chapters.iter().rposition(|c| c.start_ms <= position_ms)?;
    let c = &chapters[i];
    (position_ms < c.end_ms.max(c.start_ms + 1)).then_some(i)
}

/// The chapter line under a seek preview: "Chapter 3" without a title, the title as it is when it
/// already says "Chapter…", else "Chapter 3 · Title".
pub fn chapter_line(chapter: &ChapterMark, index: usize) -> String {
    match chapter.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        None => format!("Chapter {}", index + 1),
        Some(t) if t.to_lowercase().starts_with("chapter") => t.to_string(),
        Some(t) => format!("Chapter {} · {t}", index + 1),
    }
}

/// A chapter's row in the Chapters panel: its title, else "Chapter N".
pub fn chapter_row(chapter: &ChapterMark, index: usize) -> String {
    match chapter.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => t.to_string(),
        None => format!("Chapter {}", index + 1),
    }
}

/// A speed as the Speed panel names it: "0.5×", "1×", "1.25×" (`PlaybackSpeeds`).
pub fn speed(rate: f32) -> String {
    let mut s = format!("{:.2}", rate);
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    format!("{s}×")
}

/// "1,234 downloads", "1 download".
pub fn downloads(n: i64) -> String {
    format!("{} download{}", grouped(n), if n == 1 { "" } else { "s" })
}

/// A found subtitle's name: its release, else its language, else its id.
pub fn result_label(r: &SubtitleResult) -> String {
    if !r.name.trim().is_empty() {
        return r.name.clone();
    }
    r.language.as_deref().and_then(|l| language_name(Some(l))).unwrap_or_else(|| r.id.clone())
}

/// A found subtitle's hint: "English · Made for this file · SDH · 1,234 downloads".
pub fn result_hint(r: &SubtitleResult) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(l) = r.language.as_deref().and_then(|l| language_name(Some(l))) {
        parts.push(l);
    }
    if r.matched.as_deref() == Some("hash") {
        parts.push("Made for this file".into());
    }
    if r.hearing_impaired {
        parts.push("SDH".into());
    }
    if r.forced {
        parts.push("Forced".into());
    }
    parts.push(downloads(r.downloads));
    parts.join(" · ")
}

/// The search can't run on this server at all: Find subtitles… isn't offered again.
pub fn search_unsupported(e: &ApiError) -> bool {
    e.is(404) || e.is(405)
}

/// What Find subtitles… says when the server's search failed (`TvPlayerScreen.kt`'s search).
pub fn search_problem(e: &ApiError) -> String {
    let setup = match e {
        ApiError::Status { body: Some(b), .. } => b.get("setup").and_then(|v| v.as_bool()),
        _ => None,
    };
    let not_set_up = e.is(503) && setup == Some(false);
    if not_set_up {
        "Subtitle search isn't set up on this server".into()
    } else if search_unsupported(e) {
        "This server can't search for subtitles: it may need an update".into()
    } else {
        format!("Couldn't find subtitles: {}", reason(e))
    }
}

/// What a failed add says.
pub fn add_problem(e: &ApiError) -> String {
    format!("These subtitles couldn't be added. {}", e.problem())
}

/// An error's reason, without the sentence's capital and full stop it is set in elsewhere.
fn reason(e: &ApiError) -> String {
    let p = e.problem();
    let p = p.strip_suffix('.').unwrap_or(&p);
    let mut chars = p.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The colour of how a video plays, for the badge at the top right: a file played as it is in
/// Success, a direct stream in Info, a transcode in the Accent (`MethodBadge`).
pub fn method_color(method: &str) -> Color {
    if method.starts_with("Direct play") || method == "Downloaded" {
        theme::SUCCESS
    } else if method.starts_with("Direct stream") {
        theme::INFO
    } else {
        theme::ACCENT
    }
}

/// The notice that says what playing an item rather than the next means at the end: Previous and
/// Next as Android shows them, for an episode or more than one chapter.
pub fn shows_previous_next(subtitle: Option<&str>, chapters: usize) -> bool {
    subtitle.is_some() || chapters > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(start: i64, end: i64, title: Option<&str>) -> ChapterMark {
        ChapterMark { title: title.map(str::to_string), start_ms: start, end_ms: end, image_url: None }
    }

    #[test]
    fn writes_times_as_the_player_does() {
        assert_eq!(time(245_000), "4:05");
        assert_eq!(time(1_960_000), "32:40");
        assert_eq!(time(3_723_000), "1:02:03");
        assert_eq!(time(-5), "0:00");
        assert_eq!(remaining(1_960_000, 3_480_000), "−25:20");
        assert_eq!(remaining(10, 5), "−0:00");
    }

    #[test]
    fn writes_the_episode_line_with_dots() {
        assert_eq!(episode_line(Some("S1 E2 — Pilot")).as_deref(), Some("S1 · E2 · Pilot"));
        assert_eq!(episode_line(Some("S1 E2 · Pilot")).as_deref(), Some("S1 · E2 · Pilot"));
        assert_eq!(episode_line(Some("S1 · E2 · Pilot")).as_deref(), Some("S1 · E2 · Pilot"));
        assert_eq!(episode_line(Some("  ")), None);
        assert_eq!(episode_line(None), None);
    }

    #[test]
    fn names_chapters_as_the_preview_and_the_panel_do() {
        let c = [
            chapter(0, 60_000, None),
            chapter(60_000, 120_000, Some("The Heist")),
            chapter(120_000, 180_000, Some("Chapter 3")),
        ];
        assert_eq!(chapter_line(&c[0], 0), "Chapter 1");
        assert_eq!(chapter_line(&c[1], 1), "Chapter 2 · The Heist");
        assert_eq!(chapter_line(&c[2], 2), "Chapter 3");
        assert_eq!(chapter_row(&c[0], 0), "Chapter 1");
        assert_eq!(chapter_row(&c[1], 1), "The Heist");
        assert_eq!(chapter_at(&c, 61_000), Some(1));
        assert_eq!(chapter_at(&c, 200_000), None);
        assert_eq!(chapter_at(&[chapter(5_000, 9_000, None)], 1_000), None);
    }

    #[test]
    fn writes_speeds_and_downloads() {
        assert_eq!(speed(0.5), "0.5×");
        assert_eq!(speed(1.0), "1×");
        assert_eq!(speed(1.25), "1.25×");
        assert_eq!(speed(2.0), "2×");
        assert_eq!(downloads(1), "1 download");
        assert_eq!(downloads(1234), "1,234 downloads");
    }

    #[test]
    fn describes_found_subtitles() {
        let r = SubtitleResult {
            id: "os-1".into(),
            language: Some("en".into()),
            name: "Movie.2019.1080p".into(),
            hearing_impaired: true,
            downloads: 1234,
            matched: Some("hash".into()),
            ..SubtitleResult::default()
        };
        assert_eq!(result_label(&r), "Movie.2019.1080p");
        assert_eq!(result_hint(&r), "English · Made for this file · SDH · 1,234 downloads");
        let bare = SubtitleResult { id: "x".into(), ..SubtitleResult::default() };
        assert_eq!(result_label(&bare), "x");
        assert_eq!(result_hint(&bare), "0 downloads");
    }

    #[test]
    fn says_why_subtitle_search_failed() {
        let status = |status: u16, body: Option<serde_json::Value>| ApiError::Status {
            status,
            code: "x".into(),
            message: "it broke".into(),
            body,
        };
        let setup = status(503, Some(serde_json::json!({ "setup": false })));
        assert_eq!(search_problem(&setup), "Subtitle search isn't set up on this server");
        assert_eq!(search_problem(&status(404, None)), "This server can't search for subtitles: it may need an update");
        assert!(search_unsupported(&status(405, None)));
        assert!(search_problem(&status(500, None)).starts_with("Couldn't find subtitles: "));
    }

    #[test]
    fn colours_how_it_plays() {
        assert_eq!(method_color("Direct play"), theme::SUCCESS);
        assert_eq!(method_color("Direct stream · Audio transcoding"), theme::INFO);
        assert_eq!(method_color("Transcoding"), theme::ACCENT);
        assert!(shows_previous_next(Some("S1 · E2"), 0));
        assert!(!shows_previous_next(None, 1));
        assert!(shows_previous_next(None, 2));
    }
}
