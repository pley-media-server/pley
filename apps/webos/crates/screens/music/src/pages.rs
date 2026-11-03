//! What the music pages say, worked out from the server's answers: an artist's counts and albums,
//! an album's facts and rows, a track's lines, and Now Playing's status and sleep timer words
//! (the Android app's `MusicPages.kt` and `strings_tv_music.xml`, the TV app's `music.tsx`).

use pley_core::format::{format_clock, format_duration, pluralize};
use pley_core::model::{ItemDetail, ItemKind, ItemSummary};
use pley_player::music::{MusicError, NowPlaying};
use pley_player::sleep::{SleepChoice, SleepState};
use pley_ui::theme;

/// Under a page that couldn't find its item.
pub const NOT_FOUND_BODY: &str = "It may have been removed, or this profile can't see it.";
/// Play or Shuffle on an artist or album without a track.
pub const NOTHING_TO_PLAY: &str = "Nothing to play: it has no tracks.";
/// The title of a failed page when the server answered nothing (the TV app's `failedTitle`).
pub const UNREACHABLE_TITLE: &str = "Can't reach your server";

/// An artist's albums, newest first, then by title (`artistPage`).
pub fn sorted_albums(albums: &[ItemSummary]) -> Vec<ItemSummary> {
    let mut sorted: Vec<ItemSummary> = albums.iter().filter(|a| a.kind == ItemKind::Album).cloned().collect();
    sorted.sort_by_cached_key(|a| {
        let title = if a.sort_title.is_empty() { &a.title } else { &a.sort_title };
        (std::cmp::Reverse(a.year.unwrap_or(0)), title.to_lowercase())
    });
    sorted
}

/// An album's tracks in the server's order (its listing may hold other kinds).
pub fn tracks_of(children: &[ItemSummary]) -> Vec<ItemSummary> {
    children.iter().filter(|t| t.kind == ItemKind::Track).cloned().collect()
}

/// The line under an artist's name: "4 albums · 40 tracks  ·  Rock · Pop" (three genres at most).
/// `albums` is how many the listing has, used until the artist says how many it has.
pub fn artist_meta(artist: &ItemDetail, albums: Option<usize>) -> String {
    let album_count = artist.child_count.or(albums.map(|n| n as i64));
    let counts: Vec<String> =
        [album_count.map(|n| pluralize(n, "album", None)), artist.leaf_count.map(|n| pluralize(n, "track", None))]
            .into_iter()
            .flatten()
            .collect();
    let genres = artist.genres.iter().take(3).map(String::as_str).collect::<Vec<_>>().join(" · ");
    [counts.join(" · "), genres].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("  ·  ")
}

/// An album's facts: "2001 · 10 tracks · 42m".
pub fn album_meta(year: Option<i64>, tracks: &[ItemSummary]) -> String {
    let total: i64 = tracks.iter().filter_map(|t| t.duration_ms).sum();
    [
        year.map(|y| y.to_string()),
        (!tracks.is_empty()).then(|| pluralize(tracks.len() as i64, "track", None)),
        Some(format_duration(Some(total))).filter(|d| !d.is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

/// The profile's rating of an item in half stars (1 to 10), when it rated it.
pub fn user_rating(item: &ItemSummary) -> Option<i64> {
    item.user_state.rating.map(|r| r.round() as i64).filter(|r| *r > 0)
}

/// Whether star `star` (1 to 5) is lit for a `rating` in half stars: a half star lights it too
/// (`Stars` in `TvAlbumScreen.kt`).
pub fn star_lit(rating: i64, star: i64) -> bool {
    rating >= star * 2 || rating == star * 2 - 1
}

/// A line of an album's track list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlbumRow {
    /// "Disc 2", before each disc's tracks when there are several.
    Disc(i64),
    /// The track at this place in the album.
    Track(usize),
}

/// The album's lines: its tracks, with disc headers only when they span several discs.
pub fn album_rows(tracks: &[ItemSummary]) -> Vec<AlbumRow> {
    let disc = |t: &ItemSummary| t.parent_index.unwrap_or(1);
    let several = tracks.iter().any(|t| disc(t) != tracks.first().map_or(1, disc));
    let mut rows = Vec::with_capacity(tracks.len() + if several { 4 } else { 0 });
    let mut last = None;
    for (i, t) in tracks.iter().enumerate() {
        let d = disc(t);
        if several && last != Some(d) {
            rows.push(AlbumRow::Disc(d));
        }
        last = Some(d);
        rows.push(AlbumRow::Track(i));
    }
    rows
}

/// A track's own artist when it isn't the album's (a compilation's), for the line under its title.
pub fn track_artist<'a>(track: &'a ItemSummary, album_artist: Option<&str>) -> Option<&'a str> {
    track.grandparent_title.as_deref().filter(|a| !a.trim().is_empty() && Some(*a) != album_artist)
}

/// "Artist · Album", under a track's title in Now Playing and its queue.
pub fn byline(track: &ItemSummary) -> String {
    [track.grandparent_title.as_deref(), track.parent_title.as_deref()]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A track's length as a list shows it ("3:12"), empty when unknown.
pub fn track_length(track: &ItemSummary) -> String {
    track.duration_ms.filter(|d| *d > 0).map(|d| format_clock(Some(d))).unwrap_or_default()
}

/// Now Playing's status line and whether it reports a failure (drawn in DangerText): what went
/// wrong, else "Loading…" while the track opens, else the sleep timer's last seconds.
pub fn status(error: Option<MusicError>, loading: bool, sleep: &SleepState) -> (String, bool) {
    if let Some(e) = error {
        let text = match e {
            MusicError::Offline => "Can't reach the server.",
            MusicError::SignedOut => "Sign in again to play.",
            MusicError::Stopped => "Playback was stopped by the server.",
            MusicError::Unplayable => "This track can't be played.",
        };
        return (text.into(), true);
    }
    if loading {
        return ("Loading…".into(), false);
    }
    match sleep.seconds_left {
        Some(s) => (format!("Pausing in {s} s"), false),
        None => (String::new(), false),
    }
}

/// The sleep timer button's label: "Sleep timer", "Sleep timer: 42 min left", "Sleep timer: End of
/// this track", and "Keep listening" during its last 30 s.
pub fn sleep_label(sleep: &SleepState) -> String {
    if sleep.seconds_left.is_some() {
        return "Keep listening".into();
    }
    match (sleep.choice, sleep.minutes_left) {
        (SleepChoice::Off, _) => "Sleep timer".into(),
        (SleepChoice::Minutes(_), Some(m)) => format!("Sleep timer: {m} min left"),
        (SleepChoice::Minutes(m), None) => format!("Sleep timer: {m} min"),
        (SleepChoice::EndOfItem, _) => "Sleep timer: End of this track".into(),
    }
}

/// Whether Play/Pause shows Pause: playing, or about to (loading without a failure).
pub fn shows_pause(now: &NowPlaying) -> bool {
    now.playing || (now.loading && now.error.is_none())
}

/// A queue line and the gap under it (the TV app's `QUEUE_ROW`: 49 + 2).
pub const QUEUE_PITCH: f32 = 51.0;
/// A queue line's height.
pub const QUEUE_LINE: f32 = 49.0;

/// Room after the queue's last line (`contentPadding` in Android's `Queue`: the safe area and
/// 60), so the track playing comes to the top near the queue's end too, and the last line stays
/// clear of the screen's bottom edge.
pub const QUEUE_END: f32 = theme::SAFE_Y + 60.0;

/// The farthest the queue moves up: its last line followed by [`QUEUE_END`] at the box's bottom,
/// rounded down to whole lines so no half line sits under "Up next" at the queue's end either
/// (the room after the last line is more than a line, so it stays in the box).
pub fn queue_max(count: usize, view: f32) -> f32 {
    let max = count as f32 * QUEUE_PITCH - (QUEUE_PITCH - QUEUE_LINE) + QUEUE_END - view;
    (max / QUEUE_PITCH).floor().max(0.0) * QUEUE_PITCH
}

/// How far the queue is moved up, in whole lines: while someone looks down it (`browsing`), the
/// focused line `at` shows second from the top (the TV app's way); otherwise the track playing is
/// at the top (Android's `Queue`). Never past [`queue_max`].
pub fn queue_shift(at: usize, count: usize, view: f32, browsing: bool) -> f32 {
    let top = if browsing { at.saturating_sub(1) } else { at };
    (top as f32 * QUEUE_PITCH).min(queue_max(count, view))
}

/// Left and Right on the scrubber, and the remote's ◀◀ and ▶▶ on Now Playing.
pub const SEEK_STEP_MS: i64 = 10_000;

/// Where a seek of `step` from `position` lands: within the track.
pub fn seek_target(position: i64, step: i64, duration: i64) -> i64 {
    let end = if duration > 0 { duration } else { i64::MAX };
    (position + step).clamp(0, end)
}

/// When the scrubber's times next change: just past the next whole second of the track while it
/// plays (no second shows for half a second or one and a half), at most every 0.2 s.
pub fn next_second(position_ms: i64) -> f64 {
    (1005 - position_ms.rem_euclid(1000)).max(200) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::UserState;

    fn item(id: i64, kind: ItemKind, title: &str) -> ItemSummary {
        ItemSummary { id, kind, title: title.into(), ..ItemSummary::default() }
    }

    fn track(id: i64, disc: Option<i64>, ms: i64) -> ItemSummary {
        ItemSummary { parent_index: disc, duration_ms: Some(ms), ..item(id, ItemKind::Track, "t") }
    }

    #[test]
    fn albums_go_newest_first_then_by_title() {
        let mut a = item(1, ItemKind::Album, "Beta");
        a.year = Some(2001);
        let mut b = item(2, ItemKind::Album, "alpha");
        b.year = Some(2001);
        let mut c = item(3, ItemKind::Album, "Zed");
        c.year = Some(2010);
        let d = item(4, ItemKind::Album, "No year");
        let mut e = item(5, ItemKind::Album, "The End");
        e.sort_title = "End".into();
        e.year = Some(2001);
        let t = item(6, ItemKind::Track, "A loose track");
        let ids: Vec<i64> = sorted_albums(&[a, b, c, d, e, t]).iter().map(|x| x.id).collect();
        assert_eq!(ids, [3, 2, 1, 5, 4]);
    }

    #[test]
    fn the_artist_line_counts_and_names_three_genres() {
        let mut artist = ItemDetail::default();
        artist.summary.child_count = Some(4);
        artist.summary.leaf_count = Some(40);
        artist.genres = vec!["Rock".into(), "Pop".into(), "Jazz".into(), "Folk".into()];
        assert_eq!(artist_meta(&artist, Some(3)), "4 albums · 40 tracks  ·  Rock · Pop · Jazz");
        artist.summary.child_count = None;
        artist.summary.leaf_count = Some(1);
        artist.genres.clear();
        assert_eq!(artist_meta(&artist, Some(1)), "1 album · 1 track");
        assert_eq!(artist_meta(&ItemDetail::default(), None), "");
    }

    #[test]
    fn the_album_line_has_its_year_tracks_and_length() {
        let tracks = [track(1, None, 1_500_000), track(2, None, 1_020_000)];
        assert_eq!(album_meta(Some(2001), &tracks), "2001 · 2 tracks · 42m");
        assert_eq!(album_meta(None, &tracks[..1]), "1 track · 25m");
        assert_eq!(album_meta(Some(1999), &[]), "1999");
    }

    #[test]
    fn half_stars_light_their_star() {
        let lit = |rating| (1..=5).map(|s| star_lit(rating, s)).collect::<Vec<_>>();
        assert_eq!(lit(7), [true, true, true, true, false]);
        assert_eq!(lit(8), [true, true, true, true, false]);
        assert_eq!(lit(10), [true; 5]);
        assert_eq!(lit(1), [true, false, false, false, false]);
        let mut album = item(1, ItemKind::Album, "a");
        assert_eq!(user_rating(&album), None);
        album.user_state = UserState { rating: Some(6.6), ..UserState::default() };
        assert_eq!(user_rating(&album), Some(7));
        album.user_state.rating = Some(0.0);
        assert_eq!(user_rating(&album), None);
    }

    #[test]
    fn disc_headers_only_when_there_are_several_discs() {
        let one = [track(1, Some(1), 1), track(2, None, 1)];
        assert_eq!(album_rows(&one), [AlbumRow::Track(0), AlbumRow::Track(1)]);
        let two = [track(1, Some(1), 1), track(2, Some(1), 1), track(3, Some(2), 1)];
        assert_eq!(
            album_rows(&two),
            [AlbumRow::Disc(1), AlbumRow::Track(0), AlbumRow::Track(1), AlbumRow::Disc(2), AlbumRow::Track(2)]
        );
        assert!(album_rows(&[]).is_empty());
    }

    #[test]
    fn a_track_names_its_artist_only_when_it_isnt_the_albums() {
        let mut t = item(1, ItemKind::Track, "Song");
        t.grandparent_title = Some("Guest".into());
        t.parent_title = Some("Album".into());
        assert_eq!(track_artist(&t, Some("Band")), Some("Guest"));
        assert_eq!(track_artist(&t, Some("Guest")), None);
        assert_eq!(byline(&t), "Guest · Album");
        t.grandparent_title = None;
        assert_eq!(track_artist(&t, Some("Band")), None);
        assert_eq!(byline(&t), "Album");
        t.duration_ms = Some(192_000);
        assert_eq!(track_length(&t), "3:12");
    }

    fn now(f: impl FnOnce(&mut NowPlaying)) -> NowPlaying {
        let mut n = NowPlaying {
            track: ItemSummary::default(),
            index: 0,
            count: 1,
            playing: false,
            loading: false,
            ended: false,
            shuffle: false,
            error: None,
        };
        f(&mut n);
        n
    }

    #[test]
    fn the_status_line_says_trouble_first() {
        let off = SleepState::default();
        let counting = SleepState { seconds_left: Some(12), ..off };
        assert_eq!(status(None, false, &off), (String::new(), false));
        assert_eq!(status(None, true, &counting), ("Loading…".into(), false));
        assert_eq!(status(None, false, &counting), ("Pausing in 12 s".into(), false));
        let failed = now(|n| {
            n.loading = true;
            n.error = Some(MusicError::Offline);
        });
        assert_eq!(status(failed.error, failed.loading, &counting), ("Can't reach the server.".into(), true));
        assert_eq!(status(Some(MusicError::SignedOut), false, &off).0, "Sign in again to play.");
        assert!(!shows_pause(&failed));
        assert!(shows_pause(&now(|n| n.loading = true)));
    }

    #[test]
    fn the_sleep_button_says_what_runs() {
        let mut s = SleepState::default();
        assert_eq!(sleep_label(&s), "Sleep timer");
        s.choice = SleepChoice::Minutes(45);
        s.minutes_left = Some(42);
        assert_eq!(sleep_label(&s), "Sleep timer: 42 min left");
        s.choice = SleepChoice::EndOfItem;
        s.minutes_left = None;
        assert_eq!(sleep_label(&s), "Sleep timer: End of this track");
        s.seconds_left = Some(20);
        assert_eq!(sleep_label(&s), "Keep listening");
    }

    #[test]
    fn the_queue_moves_in_whole_lines() {
        // Ten lines in a 215 dp box: at most 87 dp after the last line shows, in whole lines (7).
        assert_eq!(queue_max(10, 215.0), 7.0 * QUEUE_PITCH);
        assert_eq!(queue_shift(0, 10, 215.0, false), 0.0);
        assert_eq!(queue_shift(3, 10, 215.0, false), 3.0 * QUEUE_PITCH);
        assert_eq!(queue_shift(3, 10, 215.0, true), 2.0 * QUEUE_PITCH);
        assert_eq!(queue_shift(9, 10, 215.0, false), 7.0 * QUEUE_PITCH);
        // The last line stays in the box, clear of its bottom by more than the safe area.
        let bottom = 9.0 * QUEUE_PITCH + QUEUE_LINE - queue_max(10, 215.0);
        assert!(bottom <= 215.0 - theme::SAFE_Y, "{bottom}");
        // A queue that nearly fits doesn't move a part of a line.
        assert_eq!(queue_shift(1, 2, 215.0, false), 0.0);
        assert_eq!(queue_shift(2, 3, 215.0, false), 0.0);
        assert_eq!(queue_max(0, 215.0), 0.0);
    }

    #[test]
    fn seeking_stays_in_the_track_and_the_clock_ticks_on_whole_seconds() {
        assert_eq!(seek_target(5_000, -SEEK_STEP_MS, 60_000), 0);
        assert_eq!(seek_target(55_000, SEEK_STEP_MS, 60_000), 60_000);
        assert_eq!(seek_target(5_000, SEEK_STEP_MS, 0), 15_000);
        assert!((next_second(12_000) - 1.005).abs() < 1e-9);
        assert!((next_second(12_900) - 0.2).abs() < 1e-9);
        assert!((next_second(12_500) - 0.505).abs() < 1e-9);
    }
}
