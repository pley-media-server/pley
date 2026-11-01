//! The music pages against the fake server: what they open, where focus goes, and the music
//! playing on behind them.

mod common;

use std::time::Duration;

use common::TestApp;
use pley_base::Key;
use pley_fixtures::ids;
use pley_shell::{AppCx, Requests, Route};

/// The first album's tracks: ten of 20 s.
const FIRST_TRACK: i64 = ids::TRACK;

fn album(app: &mut TestApp, track_id: Option<i64>) {
    app.open(Route::Album { id: ids::ALBUM, track_id });
    assert!(app.until(|a| a.shows("Golden Summit")), "the album's tracks never showed");
}

fn playing(app: &mut TestApp) -> bool {
    app.until(|a| a.now_playing().is_some_and(|n| n.1))
}

#[test]
fn the_factory_draws_the_music_pages_only() {
    let mut app = TestApp::start();
    let mut requests = Requests::default();
    let mut cx = AppCx::new(&mut app.state, &mut requests, 0.0);
    for route in [Route::Artist { id: 1 }, Route::Album { id: 2, track_id: Some(3) }, Route::NowPlaying] {
        assert!(pley_music::screen(&route, &mut cx).is_some(), "{route:?}");
    }
    for route in [Route::Home, Route::Item { id: 1 }, Route::Playlists, Route::Search { query: String::new() }] {
        assert!(pley_music::screen(&route, &mut cx).is_none(), "{route:?}");
    }
}

#[test]
fn an_albums_play_opens_now_playing_on_its_tracks() {
    let mut app = TestApp::start();
    album(&mut app, None);
    assert_eq!(app.focused(), Some(app.id(&["left", "buttons", "play"])));
    assert!(app.shows("1976 · 10 tracks · 3m"));
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::NowPlaying));
    assert!(playing(&mut app), "the album never played");
    assert_eq!(app.now_playing().map(|n| n.0), Some(FIRST_TRACK));
    assert_eq!(app.music().unwrap().list().len(), 10);
    // Now Playing starts on Play/Pause, which pauses.
    assert!(app.until(|a| a.focused() == Some(a.id(&["controls", "play"]))));
    app.press(Key::Ok);
    assert!(app.until(|a| a.now_playing() == Some((FIRST_TRACK, false))), "Play/Pause didn't pause");
}

#[test]
fn an_album_opened_from_a_track_focuses_it_and_plays_from_it() {
    let mut app = TestApp::start();
    let fourth = FIRST_TRACK + 3;
    album(&mut app, Some(fourth));
    let line = app.id(&["tracks"]).with(fourth);
    assert!(app.until(|a| a.focused() == Some(line)), "the track it was opened from never took focus");
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::NowPlaying));
    assert!(playing(&mut app));
    let m = app.music().unwrap();
    assert_eq!(m.now_playing().map(|n| (n.track.id, n.index)), Some((fourth, 3)));
}

#[test]
fn ok_on_the_track_playing_pauses_and_resumes_it() {
    let mut app = TestApp::start();
    album(&mut app, None);
    app.press(Key::Ok);
    assert!(playing(&mut app));
    app.press(Key::Back);
    assert!(matches!(app.route(), Some(Route::Album { .. })));
    // Play is focused again; Right past the buttons (Play, Shuffle, Now playing) reaches the
    // tracks, at the one playing.
    assert!(app.until(|a| a.focused() == Some(a.id(&["left", "buttons", "play"]))));
    for _ in 0..3 {
        app.press(Key::Right);
    }
    assert_eq!(app.focused(), Some(app.id(&["tracks"]).with(FIRST_TRACK)));
    app.press(Key::Ok);
    assert!(app.until(|a| a.now_playing() == Some((FIRST_TRACK, false))), "OK didn't pause the track playing");
    assert!(matches!(app.route(), Some(Route::Album { .. })), "pausing stays on the album");
    app.press(Key::Ok);
    assert!(playing(&mut app), "OK didn't resume it");
}

#[test]
fn an_artists_play_gathers_every_album_and_down_reaches_them() {
    let mut app = TestApp::start();
    app.open(Route::Artist { id: ids::ARTIST });
    assert!(app.until(|a| a.shows("Albums")), "the albums never showed");
    assert!(app.shows("Artist"));
    assert!(app.shows("4 albums · 40 tracks  ·  Ambient"));
    assert_eq!(app.focused(), Some(app.id(&["buttons", "play"])));

    // The newest album comes first; OK opens it.
    app.press(Key::Down);
    let newest = ids::ALBUM + 3;
    assert_eq!(app.focused(), Some(app.id(&["albums"]).with(newest as u64)));
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::Album { id: newest, track_id: None }));
    app.press(Key::Back);

    app.press(Key::Up);
    app.press(Key::Ok);
    assert!(app.until(|a| a.route() == Some(&Route::NowPlaying)), "Play never opened Now Playing");
    assert!(playing(&mut app));
    let m = app.music().unwrap();
    assert_eq!(m.list().len(), 40);
    assert_eq!(m.list()[0].id, FIRST_TRACK, "in the albums' own order");
    assert_eq!(m.list()[10].id, FIRST_TRACK + 10, "the second album follows the first");

    // Back finds focus where it was, and Now playing beside Shuffle.
    app.press(Key::Back);
    assert!(app.until(|a| a.focused() == Some(a.id(&["buttons", "play"]))), "focus didn't come back to Play");
    assert!(app.shows("Silent Rivers"));
    app.press(Key::Right);
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(app.id(&["buttons", "now"])));
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::NowPlaying));
}

#[test]
fn now_playing_seeks_skips_and_jumps() {
    let mut app = TestApp::start();
    album(&mut app, None);
    app.press(Key::Ok);
    assert!(playing(&mut app));

    // The scrubber: Right 10 s on, Left back.
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["scrubber"])));
    let before = app.music().unwrap().position_ms();
    app.press(Key::Right);
    let after = app.music().unwrap().position_ms();
    assert!((after - before - 10_000).abs() < 1_000, "{before} → {after}");
    app.press(Key::Left);
    assert!(app.music().unwrap().position_ms() < 10_000);

    // The remote's Next, anywhere on the page.
    app.press(Key::Next);
    assert_eq!(app.now_playing().map(|n| n.0), Some(FIRST_TRACK + 1));

    // The queue: Down from the controls enters at the track playing; OK on the next one jumps.
    app.press(Key::Down);
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(app.id(&["queue"]).with(1usize)));
    app.press(Key::Down);
    app.press(Key::Ok);
    assert_eq!(app.now_playing().map(|n| n.0), Some(FIRST_TRACK + 2));
    assert!(app.shows("Up next"));

    // Up out of the queue and Down again lands on the line focused last, not the track playing.
    let queue = app.id(&["queue"]);
    for _ in 0..3 {
        app.press(Key::Up);
    }
    assert!(app.focused().is_some_and(|f| (0..10usize).all(|i| f != queue.with(i))), "focus stayed in the queue");
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(queue.with(0usize)));
}

#[test]
fn a_track_that_ends_gives_way_to_the_next_while_people_browse() {
    let mut app = TestApp::start();
    album(&mut app, None);
    app.press(Key::Ok);
    assert!(playing(&mut app));
    app.press(Key::Back);
    app.music().unwrap().seek_to(19_700);
    assert!(app.until(|a| a.now_playing() == Some((FIRST_TRACK + 1, true))), "the next track never started on its own");
    assert!(matches!(app.route(), Some(Route::Album { .. })));
}

#[test]
fn leaving_the_app_lets_go_of_the_music_and_play_comes_back_there() {
    let mut app = TestApp::start();
    album(&mut app, None);
    app.press(Key::Ok);
    assert!(playing(&mut app));
    app.music().unwrap().seek_to(8_000);
    let now = app.time;
    app.shell.visible(false, &mut app.state, &mut app.ui, now);
    app.frame(Vec::new());
    assert_eq!(app.now_playing(), Some((FIRST_TRACK, false)));
    app.shell.visible(true, &mut app.state, &mut app.ui, now);
    // Its place still shows, and the scrubber moves it without playing (the TV's engine let go
    // of the track and knows no place by then).
    let held = app.player().unwrap().position_ms();
    assert!((7_000..9_000).contains(&held), "{held}");
    app.press(Key::Up);
    app.press(Key::Right);
    assert_eq!(app.player().unwrap().position_ms(), held + 10_000);
    app.press(Key::Left);
    assert_eq!(app.player().unwrap().position_ms(), held);
    assert_eq!(app.now_playing(), Some((FIRST_TRACK, false)), "a seek played it");
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(playing(&mut app), "Play didn't start the music again");
    assert!(app.music().unwrap().position_ms() >= 7_000, "it started over instead of where it was");
}

#[test]
fn pause_pressed_while_the_track_starts_opens_it_paused() {
    let mut app = TestApp::start();
    album(&mut app, None);
    app.press(Key::Ok);
    assert_eq!(app.route(), Some(&Route::NowPlaying));
    let starting = app.music().unwrap().now_playing().is_some_and(|n| n.loading && !n.playing);
    assert!(starting, "the track was already playing");
    app.press(Key::Pause);
    let opened = |a: &mut TestApp| a.music().unwrap().now_playing().is_some_and(|n| !n.loading && !n.playing);
    assert!(app.until(opened), "the track never opened paused");
    std::thread::sleep(Duration::from_millis(300));
    app.until(|_| true);
    assert_eq!(app.now_playing(), Some((FIRST_TRACK, false)), "it played on");
    assert!(app.music().unwrap().position_ms() < 1_000);
    // Play/Pause then plays it.
    app.press(Key::PlayPause);
    assert!(playing(&mut app));
}

#[test]
fn the_remotes_media_keys_work_on_the_album_and_the_artist() {
    let mut app = TestApp::start();
    album(&mut app, None);
    app.press(Key::Ok);
    assert!(playing(&mut app));
    app.press(Key::Back);
    app.press(Key::PlayPause);
    assert!(app.until(|a| a.now_playing() == Some((FIRST_TRACK, false))), "Play/Pause didn't pause on the album");
    app.press(Key::Next);
    assert!(app.until(|a| a.now_playing() == Some((FIRST_TRACK + 1, true))), "Next didn't play the next track");
    app.open(Route::Artist { id: ids::ARTIST });
    assert!(app.until(|a| a.shows("Albums")));
    app.press(Key::Pause);
    assert!(app.until(|a| a.now_playing() == Some((FIRST_TRACK + 1, false))), "Pause didn't pause on the artist");
}

#[test]
fn now_playing_without_music_says_so() {
    let mut app = TestApp::start();
    app.open(Route::NowPlaying);
    assert!(app.shows("Nothing is playing"));
    assert!(app.shows("Play an album or an artist and it shows here."));
    assert_eq!(app.focused(), Some(app.id(&["message"])));
    let below = app.shell.routes().len();
    app.press(Key::Ok);
    assert_eq!(app.shell.routes().len(), below - 1, "Back didn't close the page");
}

#[test]
fn an_album_that_isnt_there_says_so() {
    let mut app = TestApp::start();
    app.open(Route::Album { id: 999_999, track_id: None });
    assert!(app.until(|a| a.shows("Album not found")));
    assert!(app.shows("It may have been removed, or this profile can't see it."));
    assert!(app.shows("Back"));
}
