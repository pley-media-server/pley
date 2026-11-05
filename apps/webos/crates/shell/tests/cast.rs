//! Casts from the user's remotes, against the fake server: the TV is a receiver while its event
//! stream is open, and what is cast to it opens as its own Play would, whatever page shows, with
//! "Playing from …" over it, the dimmed screen waking. A cast for another device, or one that
//! comes while the app is hidden or asks who's watching, plays nothing. One from another profile's
//! remote ends what plays, lets its last reports go out as the profile it played as, then
//! switches.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use common::{Heard, Page, TestApp, device, http, registry, sign_in, store};
use pley_core::model::{LiveCategory, ProgressReport, ProgressState};
use pley_fixtures::{CastBody, FixtureServer, Mode, Options, ServerEvent, ids};
use pley_player::PlayerEnv;
use pley_shell::cast::LAST_REPORTS_WAIT;
use pley_shell::{
    AppCx, AppEvent, CAST_NOTICE_SECONDS, CastTracks, Chrome, DIM_AFTER, NavCommand, PlayQueue, PlayRequest, Registry,
    Route, Screen,
};
use pley_ui::Ui;
use serde_json::json;

const PHONE: &str = "pleyAndroid/0.1.5 (Android 14; phone)";

/// The fake server, writing to its streams often, so it hears soon of one the app closed.
fn fixtures() -> FixtureServer {
    let options = Options { scan_every: None, keep_alive: Duration::from_millis(200), ..Options::default() };
    FixtureServer::start_with(options).unwrap()
}

/// The music a test registry's player was given to play.
type Played = Rc<RefCell<Vec<CastTracks>>>;

/// The app signed in as `profile`, its pages the test's, its music player one that records what
/// it was given.
fn app_as(server: &FixtureServer, profile: i64) -> (TestApp, Played) {
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(server, profile, &store, &http);
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let played: Played = Rc::default();
    let mut screens: Registry = registry(&heard, &[]);
    let given = played.clone();
    screens.cast_music(move |_, tracks| {
        given.borrow_mut().push(tracks);
        true
    });
    (TestApp::new(device, store, http, screens), played)
}

/// The app as Alex, once the server lists the TV as a receiver of titles and Live TV.
fn receiving(server: &FixtureServer) -> (TestApp, Played) {
    let (mut app, played) = app_as(server, ids::ALEX);
    let id = app.state.install_id().to_string();
    let listed = |s: &FixtureServer| s.receivers_of(ids::ALEX).into_iter().find(|r| r["receiver_id"] == *id);
    assert!(
        app.until(|_| listed(server).is_some_and(|r| r["casts"] == json!(["item", "live"]))),
        "the TV never took casts of Live TV: {:?}",
        server.receivers_of(ids::ALEX)
    );
    (app, played)
}

/// Alex's remote, on a phone, casting `body` to the TV; then the frames until `played` holds.
fn cast(app: &mut TestApp, server: &FixtureServer, body: CastBody, played: impl FnMut(&mut TestApp) -> bool) {
    let id = app.state.install_id().to_string();
    assert_eq!(server.cast(ids::ALEX, &id, &body, Some(PHONE)), 202);
    assert!(app.until(played), "{body:?} never played: {:?}", app.shell.top_route());
}

/// The video player's request, when it is on top.
fn player(app: &TestApp) -> Option<&pley_shell::PlayRequest> {
    match app.shell.top_route() {
        Some(Route::Player(r)) => Some(r),
        _ => None,
    }
}

#[test]
fn the_tv_takes_casts_as_a_tv_while_it_shows_and_of_live_tv_where_the_profile_has_it() {
    let server = fixtures();
    let (mut app, _) = receiving(&server);
    let id = app.state.install_id().to_string();
    let tv = server.receivers_of(ids::ALEX).into_iter().find(|r| r["receiver_id"] == *id).unwrap();
    // A TV, named by the server from the app's User-Agent; the app sends no name.
    assert_eq!((&tv["kind"], &tv["name"]), (&json!("tv"), &json!("LG TV")));
    assert!(id.starts_with("webos-"), "{id}");

    // Hidden, the stream closes and the TV takes no casts; shown, it does again.
    let now = app.time;
    app.shell.visible(false, &mut app.state, &mut app.ui, now);
    assert!(app.until(|_| server.receivers_of(ids::ALEX).is_empty()), "the TV stayed a receiver while hidden");
    assert_eq!(server.cast(ids::ALEX, &id, &CastBody::item(ids::SAMPLE_1080P), Some(PHONE)), 404);
    let now = app.time;
    app.shell.visible(true, &mut app.state, &mut app.ui, now);
    assert!(app.until(|_| !server.receivers_of(ids::ALEX).is_empty()), "the TV didn't take casts again");
    assert_eq!(app.shell.top_route(), Some(&Route::Home), "something played");

    // Sam has no Live TV: once the server says so, the TV takes titles alone.
    let (mut sams, _) = app_as(&server, ids::SAM);
    let sams_tv = sams.state.install_id().to_string();
    let listed = |s: &FixtureServer| s.receivers_of(ids::SAM).into_iter().find(|r| r["receiver_id"] == *sams_tv);
    let titles = |s: &FixtureServer| listed(s).is_some_and(|r| r["casts"] == json!(["item"]));
    assert!(sams.until(|_| titles(&server)), "Sam's TV takes Live TV: {:?}", listed(&server));
    assert!(!sams.state.live_available());
}

#[test]
fn a_cast_film_opens_the_player_where_it_starts_and_says_where_it_came_from() {
    let server = fixtures();
    let (mut app, _) = receiving(&server);
    let film = CastBody { start_ms: Some(5_000), ..CastBody::item(ids::SAMPLE_1080P) };
    cast(&mut app, &server, film, |a| player(a).is_some());
    let r = player(&app).unwrap();
    assert_eq!((r.item_id, r.start_ms, r.from_start, r.queue), (ids::SAMPLE_1080P, Some(5_000), false, None));
    assert_eq!(app.shell.depth(), 2, "the player opens over Home");
    assert_eq!(app.shell.toast(), Some("Playing from Pley for Android"));
    assert!(app.list.says("Playing from Pley for Android"), "the notice isn't drawn over the player");
    // About 4 s, as long as the player's title banner.
    app.advance(CAST_NOTICE_SECONDS - 0.3);
    assert!(app.list.says("Playing from Pley for Android"));
    app.advance(0.6);
    assert_eq!(app.shell.toast(), None);

    // Without a start, it plays as Play would, from where it was left, in place of the film
    // before; a remote the server couldn't name is another device.
    let id = app.state.install_id().to_string();
    assert_eq!(server.cast(ids::ALEX, &id, &CastBody::item(ids::PGS_MOVIE), None), 202);
    assert!(app.until(|a| player(a).is_some_and(|r| r.item_id == ids::PGS_MOVIE)), "the second film never played");
    assert_eq!(player(&app).unwrap().start_ms, None);
    assert_eq!(app.shell.toast(), Some("Playing from another device"));
    assert_eq!(app.shell.depth(), 2, "the film before wasn't replaced");
}

#[test]
fn a_cast_show_plays_its_next_episode_and_a_channel_replaces_the_player() {
    let server = fixtures();
    let (mut app, _) = receiving(&server);
    let next = app.state.api().unwrap().on_deck(ids::SHOW).unwrap().expect("the show has a next episode");
    // Where Play resumes it: a place named with the show isn't the episode's.
    let show = CastBody { start_ms: Some(30_000), ..CastBody::item(ids::SHOW) };
    cast(&mut app, &server, show, |a| player(a).is_some());
    assert_eq!(player(&app).map(|r| (r.item_id, r.start_ms, r.from_start)), Some((next.id, None, false)));

    let live = |a: &mut TestApp| matches!(a.shell.top_route(), Some(Route::LivePlayer { .. }));
    cast(&mut app, &server, CastBody::channel(ids::LIVE_CHANNEL), live);
    assert!(
        matches!(
            app.shell.top_route(),
            Some(Route::LivePlayer { channel_id: ids::LIVE_CHANNEL, category: LiveCategory::All, .. })
        ),
        "{:?}",
        app.shell.top_route()
    );
    assert_eq!(app.shell.depth(), 2, "the channel didn't take the episode's place");
}

#[test]
fn cast_music_plays_in_the_music_player_and_now_playing_shows_it() {
    let server = fixtures();
    let (mut app, played) = receiving(&server);
    let given = |n: usize| {
        let played = played.clone();
        move |_: &mut TestApp| played.borrow().len() == n
    };
    // An album, from its first track, in Now Playing over Home.
    cast(&mut app, &server, CastBody::item(ids::ALBUM), given(1));
    app.advance(0.1);
    assert_eq!(app.shell.top_route(), Some(&Route::NowPlaying));
    let album = played.borrow().last().cloned().unwrap();
    assert_eq!((album.tracks.len(), album.at, album.shuffle, album.start_ms), (10, 0, false, 0));
    assert_eq!(album.tracks[0].id, ids::TRACK);

    // A track: its album from it, where the remote said; Now Playing shows already.
    cast(&mut app, &server, CastBody { start_ms: Some(12_000), ..CastBody::item(ids::TRACK + 3) }, given(2));
    let track = played.borrow().last().cloned().unwrap();
    assert_eq!((track.tracks.len(), track.at, track.start_ms), (10, 3, 12_000));
    assert_eq!(track.tracks[3].id, ids::TRACK + 3);
    assert_eq!(app.shell.depth(), 2, "Now Playing opened twice");

    // An artist, album after album, shuffled from a random track.
    cast(&mut app, &server, CastBody { shuffle: true, ..CastBody::item(ids::ARTIST) }, given(3));
    let artist = played.borrow().last().cloned().unwrap();
    assert_eq!((artist.tracks.len(), artist.shuffle), (40, true));
    assert!(artist.at < 40);
    assert_eq!(artist.tracks[10].id, ids::TRACK + 10, "the second album follows the first");

    // Alex's playlist of tracks, in its order.
    cast(&mut app, &server, CastBody::playlist(ids::PLAYLIST_AUDIO), given(4));
    let playlist = played.borrow().last().cloned().unwrap();
    assert!(!playlist.tracks.is_empty() && playlist.at == 0 && !playlist.shuffle);

    // A playlist of films plays in the player, the playlist going on as its queue.
    cast(&mut app, &server, CastBody::playlist(ids::PLAYLIST_VIDEO), |a| player(a).is_some());
    assert_eq!(player(&app).unwrap().queue, Some(PlayQueue::Playlist(ids::PLAYLIST_VIDEO)));
    assert_eq!(app.shell.routes()[1], &Route::NowPlaying);
}

#[test]
fn a_cast_for_another_device_plays_nothing_and_what_cant_be_found_says_why() {
    let server = fixtures();
    let (mut app, played) = receiving(&server);
    let cast_to = |receiver: &str, item: i64| ServerEvent::Cast {
        receiver_id: receiver.into(),
        item_id: Some(item),
        playlist_id: None,
        channel_id: None,
        start_ms: None,
        shuffle: false,
        audio_index: None,
        subtitle_index: None,
        from: Some("Pley for Android".into()),
        profile_id: None,
        switch_ticket: None,
    };
    // The profile's streams hear every cast: one for another device is that device's.
    server.emit(cast_to("webos-another", ids::SAMPLE_1080P));
    app.advance(0.5);
    assert!(app.until(|a| !a.shell.casting()));
    app.advance(0.3);
    assert_eq!((app.shell.top_route(), app.shell.toast()), (Some(&Route::Home), None));

    // Heard as the app hides: nothing plays.
    let id = app.state.install_id().to_string();
    let now = app.time;
    app.shell.visible(false, &mut app.state, &mut app.ui, now);
    let heard = pley_core::model::ServerEvent::Cast {
        receiver_id: id.clone(),
        item_id: Some(ids::SAMPLE_1080P),
        playlist_id: None,
        channel_id: None,
        start_ms: None,
        shuffle: false,
        audio_index: None,
        subtitle_index: None,
        from: None,
        profile_id: None,
        switch_ticket: None,
    };
    app.shell.deliver(AppEvent::Server(heard), &mut app.state, &mut app.ui, now);
    assert!(!app.shell.casting());
    assert!(app.until(|_| server.receivers_of(ids::ALEX).is_empty()), "the TV stayed a receiver while hidden");
    let now = app.time;
    app.shell.visible(true, &mut app.state, &mut app.ui, now);
    assert!(app.until(|_| !server.receivers_of(ids::ALEX).is_empty()), "the TV didn't take casts again");
    assert_eq!(app.shell.top_route(), Some(&Route::Home));

    // A title gone since: the TV says why, and plays nothing.
    server.emit(cast_to(&id, 999_999));
    assert!(app.until(|a| a.shell.toast().is_some_and(|t| t.contains("not found"))), "{:?}", app.shell.toast());
    assert_eq!(app.shell.top_route(), Some(&Route::Home));
    assert!(played.borrow().is_empty());

    // While "Who's watching?" asks, a cast never skips it.
    app.navigate(NavCommand::Reset(Route::Profiles));
    server.emit(cast_to(&id, ids::SAMPLE_1080P));
    app.advance(0.5);
    assert_eq!(app.shell.top_route(), Some(&Route::Profiles));
}

#[test]
fn a_cast_still_being_found_as_whos_watching_opens_never_gets_past_it() {
    let server = fixtures();
    let (mut app, _) = receiving(&server);
    // The server slow, the film is still being found when the page of profiles opens.
    server.set_mode(Mode::Slow).unwrap();
    let id = app.state.install_id().to_string();
    assert_eq!(server.cast(ids::ALEX, &id, &CastBody::item(ids::SAMPLE_1080P), Some(PHONE)), 202);
    assert!(app.until(|a| a.shell.casting()), "the cast was never heard");
    app.navigate(NavCommand::Reset(Route::Profiles));
    assert!(app.until(|a| !a.shell.casting()), "the film was never found");
    app.advance(0.3);
    assert_eq!((app.shell.routes(), app.shell.toast()), (vec![&Route::Profiles], None));
}

/// The app as Robin, once the server lists the TV as a receiver of Robin's.
fn robins_tv(server: &FixtureServer) -> (TestApp, String) {
    let (mut app, _) = app_as(server, ids::ROBIN);
    let id = app.state.install_id().to_string();
    let listed = |s: &FixtureServer| s.receivers_of(ids::ROBIN).iter().any(|r| r["receiver_id"] == *id);
    assert!(app.until(|a| listed(server) && a.state.profile() == Some(ids::ROBIN)), "the TV never took casts");
    (app, id)
}

#[test]
fn a_cast_from_another_profiles_remote_switches_the_tv_to_it_then_plays_once() {
    let server = fixtures();
    let (mut app, id) = robins_tv(&server);
    app.navigate(NavCommand::Push(Route::Item { id: ids::SAMPLE_HDR10 }));

    // Alex's remote casts to Robin's TV: the TV becomes Alex's, then plays the film over Home.
    let film = CastBody { start_ms: Some(5_000), profile_id: Some(ids::ROBIN), ..CastBody::item(ids::SAMPLE_1080P) };
    assert_eq!(server.cast(ids::ALEX, &id, &film, Some(PHONE)), 202);
    assert!(app.until(|a| player(a).is_some()), "the film never played: {:?}", app.shell.top_route());
    assert_eq!(app.state.profile(), Some(ids::ALEX));
    assert_eq!(app.state.api().unwrap().me().unwrap().id, ids::ALEX, "the TV's sign-in acts as Alex");
    assert_eq!(app.shell.routes()[0], &Route::Home, "Robin's pages went");
    assert_eq!(app.shell.depth(), 2);
    let r = player(&app).unwrap();
    assert_eq!((r.item_id, r.start_ms), (ids::SAMPLE_1080P, Some(5_000)));
    assert_eq!(app.shell.toast(), Some("Playing from Pley for Android"));

    // The event stream opened again as Alex's: the TV is Alex's receiver now, and the cast, which
    // came before it, played once.
    let alexs = |s: &FixtureServer| s.receivers_of(ids::ALEX).iter().any(|r| r["receiver_id"] == *id);
    assert!(app.until(|_| alexs(&server) && server.receivers_of(ids::ROBIN).is_empty()), "still Robin's receiver");
    let page = app.shell.top().unwrap().id();
    app.advance(1.0);
    assert!(!app.shell.casting());
    assert_eq!(app.shell.top().map(|e| e.id()), Some(page), "the cast played again");

    // Alex's next cast is to Alex's own TV: it plays as it is.
    assert_eq!(server.cast(ids::ALEX, &id, &CastBody::item(ids::PGS_MOVIE), Some(PHONE)), 202);
    assert!(app.until(|a| player(a).is_some_and(|r| r.item_id == ids::PGS_MOVIE)), "the second film never played");
    assert_eq!(app.state.profile(), Some(ids::ALEX));
}

#[test]
fn a_cast_heard_as_the_tv_switches_for_another_is_dropped() {
    let server = fixtures();
    let (mut app, id) = robins_tv(&server);
    // The server slow, the switch is still under way when Alex casts again.
    server.set_mode(Mode::Slow).unwrap();
    let to_robins = |item: i64| CastBody { profile_id: Some(ids::ROBIN), ..CastBody::item(item) };
    assert_eq!(server.cast(ids::ALEX, &id, &to_robins(ids::SAMPLE_1080P), Some(PHONE)), 202);
    assert!(app.until(|a| a.shell.casting()), "the cast was never heard");
    assert_eq!(server.cast(ids::ALEX, &id, &to_robins(ids::PGS_MOVIE), Some(PHONE)), 202);
    let sent = Instant::now();
    assert!(app.until(|_| sent.elapsed() > Duration::from_millis(300)));
    assert!(app.shell.casting() && app.state.profile() == Some(ids::ROBIN), "the switch ended too soon");
    server.set_mode(Mode::Normal).unwrap();
    assert!(app.until(|a| player(a).is_some()), "nothing played: {:?}", app.shell.top_route());
    assert_eq!(player(&app).unwrap().item_id, ids::SAMPLE_1080P, "the cast heard during the switch played");
    assert_eq!(app.state.profile(), Some(ids::ALEX));
    let page = app.shell.top().unwrap().id();
    app.advance(1.0);
    assert_eq!(app.shell.top().map(|e| e.id()), Some(page), "a cast played twice");
}

/// The session of the film Robin's TV plays in the tests of what ends before a switch.
const ROBINS_SESSION: &str = "robins-session";

/// A video player as the app's: when it closes, its last progress report and then its session's
/// end go out on the app's report queue, with the sign-in its environment holds. `held` keeps
/// them from going out until the test lets them.
struct ReportingPlayer {
    env: PlayerEnv,
    item: i64,
    held: Option<mpsc::Receiver<()>>,
}

impl Screen for ReportingPlayer {
    fn ui(&mut self, _ui: &mut Ui, _cx: &mut AppCx) {}

    fn chrome(&self) -> Chrome {
        Chrome::Video
    }
}

impl Drop for ReportingPlayer {
    fn drop(&mut self) {
        let (server, item, held) = (self.env.server.clone(), self.item, self.held.take());
        self.env.reporter.send(move || {
            if let Some(held) = held {
                let _ = held.recv_timeout(Duration::from_secs(30));
            }
            let report = ProgressReport {
                position_ms: 61_000,
                state: ProgressState::Stopped,
                session_id: Some(ROBINS_SESSION.into()),
                ..ProgressReport::default()
            };
            let _ = server.progress(item, &report);
            let _ = server.stop_session(ROBINS_SESSION);
        });
    }
}

/// Robin's TV playing a film whose last reports wait for `held`, once the server lists the TV as
/// a receiver of Robin's; the server's journal starts then.
fn robins_tv_playing(server: &FixtureServer, held: mpsc::Receiver<()>) -> (TestApp, String) {
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(server, ids::ROBIN, &store, &http);
    let heard: Heard = Rc::default();
    let held = RefCell::new(Some(held));
    let mut screens = Registry::new();
    screens.add(move |route, cx| match route {
        Route::Player(r) if r.item_id == ids::SAMPLE_HDR10 => {
            let env = cx.player_env()?;
            Some(Box::new(ReportingPlayer { env, item: r.item_id, held: held.borrow_mut().take() }) as Box<dyn Screen>)
        }
        _ => Some(Box::new(Page { route: route.clone(), panel: false, heard: heard.clone(), ready: true })),
    });
    let mut app = TestApp::new(device, store, http, screens);
    let id = app.state.install_id().to_string();
    let listed = |s: &FixtureServer| s.receivers_of(ids::ROBIN).iter().any(|r| r["receiver_id"] == *id);
    assert!(app.until(|a| listed(server) && a.state.profile() == Some(ids::ROBIN)), "the TV never took casts");
    app.navigate(NavCommand::Push(Route::Player(PlayRequest::item(ids::SAMPLE_HDR10))));
    assert_eq!(player(&app).map(|r| r.item_id), Some(ids::SAMPLE_HDR10));
    server.clear_journal();
    (app, id)
}

/// What the server heard of the end of Robin's film: each write's method and the profile its
/// token acted as, in order.
fn robins_film_ended(server: &FixtureServer) -> Vec<(String, Option<i64>)> {
    let progress = format!("/api/items/{}/progress", ids::SAMPLE_HDR10);
    let session = format!("/api/playback/{ROBINS_SESSION}");
    let ends = server.journal().into_iter().filter(|e| e.path == progress || e.path == session);
    ends.map(|e| (e.method, e.profile_id)).collect()
}

#[test]
fn a_cast_from_another_profile_ends_what_plays_and_its_last_reports_go_as_the_old_profile_first() {
    let server = fixtures();
    let (release, held) = mpsc::channel();
    let (mut app, id) = robins_tv_playing(&server, held);

    let film = CastBody { profile_id: Some(ids::ROBIN), ..CastBody::item(ids::SAMPLE_1080P) };
    assert_eq!(server.cast(ids::ALEX, &id, &film, Some(PHONE)), 202);
    // Robin's film ends first, and the switch waits for its last reports.
    assert!(app.until(|a| a.shell.casting() && player(a).is_none()), "the film played on: {:?}", app.shell.top_route());
    let ended = Instant::now();
    assert!(app.until(|_| ended.elapsed() > Duration::from_millis(500)));
    assert!(app.shell.casting());
    assert_eq!(app.state.profile(), Some(ids::ROBIN));
    assert_eq!(app.state.api().unwrap().me().unwrap().id, ids::ROBIN, "the sign-in switched before the reports went");
    assert_eq!(robins_film_ended(&server), [], "the reports went before they were let go");

    // They go, as Robin's: the last report, then the session's end. Only then does the TV switch
    // and play the cast as Alex.
    release.send(()).unwrap();
    assert!(app.until(|a| player(a).is_some()), "the cast never played: {:?}", app.shell.top_route());
    assert_eq!(player(&app).unwrap().item_id, ids::SAMPLE_1080P);
    assert_eq!(app.state.profile(), Some(ids::ALEX));
    assert_eq!(app.state.api().unwrap().me().unwrap().id, ids::ALEX);
    let robins = Some(ids::ROBIN);
    assert_eq!(robins_film_ended(&server), [("POST".to_string(), robins), ("DELETE".to_string(), robins)]);
}

#[test]
fn last_reports_not_out_in_5_s_are_dropped_rather_than_sent_as_the_new_profile() {
    let server = fixtures();
    let (release, held) = mpsc::channel();
    let (mut app, id) = robins_tv_playing(&server, held);

    // Robin's film's last reports don't go out: the TV switches after 5 s all the same, and plays
    // the cast as Alex.
    let cast_at = Instant::now();
    let film = CastBody { profile_id: Some(ids::ROBIN), ..CastBody::item(ids::SAMPLE_1080P) };
    assert_eq!(server.cast(ids::ALEX, &id, &film, Some(PHONE)), 202);
    let cast_plays = |a: &mut TestApp| player(a).is_some_and(|r| r.item_id == ids::SAMPLE_1080P);
    assert!(app.until(cast_plays), "the cast never played: {:?}", app.shell.top_route());
    assert!(cast_at.elapsed() >= LAST_REPORTS_WAIT, "the TV switched before 5 s: {:?}", cast_at.elapsed());
    assert_eq!(app.state.profile(), Some(ids::ALEX));

    // Let go once the TV is Alex's, they are dropped: nothing reaches the server, as Alex's least
    // of all, and the sign-in carries on.
    release.send(()).unwrap();
    assert!(app.state.flush_reports(Duration::from_secs(5)), "the reports never finished");
    assert_eq!(robins_film_ended(&server), []);
    assert!(app.state.signed_in());
    assert_eq!(app.state.api().unwrap().me().unwrap().id, ids::ALEX);
}

#[test]
fn a_cast_from_another_profile_without_a_ticket_or_with_a_spent_one_plays_nothing() {
    let server = fixtures();
    let (mut app, id) = robins_tv(&server);
    let from_alex = |ticket: Option<&str>| ServerEvent::Cast {
        receiver_id: id.clone(),
        item_id: Some(ids::SAMPLE_1080P),
        playlist_id: None,
        channel_id: None,
        start_ms: None,
        shuffle: false,
        audio_index: None,
        subtitle_index: None,
        from: Some("Pley for Android".into()),
        profile_id: Some(ids::ALEX),
        switch_ticket: ticket.map(str::to_string),
    };
    // Without a ticket the TV can't switch: it ignores the cast.
    server.emit(from_alex(None));
    app.advance(0.5);
    assert!(!app.shell.casting());
    assert_eq!((app.shell.top_route(), app.shell.toast()), (Some(&Route::Home), None));

    // A ticket the server doesn't know: the switch is refused, the TV says so and stays Robin's.
    server.emit(from_alex(Some("spent")));
    let said = "Couldn't switch to the remote's profile";
    assert!(app.until(|a| a.shell.toast() == Some(said)), "{:?}", app.shell.toast());
    assert!(!app.shell.casting());
    assert_eq!(app.shell.top_route(), Some(&Route::Home));
    assert_eq!(app.state.profile(), Some(ids::ROBIN));
    assert!(app.state.signed_in());
    assert_eq!(app.state.api().unwrap().me().unwrap().id, ids::ROBIN);
}

#[test]
fn a_cast_wakes_the_dimmed_screen() {
    let server = fixtures();
    let (mut app, played) = receiving(&server);
    app.advance(DIM_AFTER + 1.0);
    assert!(app.shell.dimmed());
    // Music opens Now Playing, which dims again only after as long idle.
    cast(&mut app, &server, CastBody::item(ids::ALBUM), |_| !played.borrow().is_empty());
    assert!(!app.shell.dimmed(), "Now Playing opened under the dimmer");
    app.advance(DIM_AFTER - 1.0);
    assert!(!app.shell.dimmed());
}
