//! The live player against the fake server, on the simulator's engine: tuning, changing channel
//! round the category, typed numbers, the controls, the channel list beside the picture, and every
//! connection in use.

mod common;

use common::TestApp;
use pley_base::{Cmd, Key, KeyEvent};
use pley_core::model::LiveCategory;
use pley_fixtures::{CastBody, Mode, ServerEvent, ids};
use pley_live::picture_aside;
use pley_shell::Route;
use pley_ui::InputEvent;

/// The app on the live player of `channel`, channel up and down going through `category`; the
/// channel watched.
fn watching(channel: i64, category: LiveCategory) -> TestApp {
    let mut app = TestApp::start();
    app.open(Route::LivePlayer { channel_id: channel, category, pressed_at: 0.0 });
    assert!(app.until(|a| a.watches(channel) == 1), "channel {channel} wasn't watched");
    app
}

/// Waits until channel `id` is the one watched last.
fn tuned(app: &mut TestApp, id: i64) -> bool {
    app.until(|a| a.state.live_last_watched.as_ref().is_some_and(|l| l.0 == id) && a.watches(id) > 0)
}

#[test]
fn up_and_down_change_channel_round_the_category() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    assert_eq!(app.state.live_last_watched, Some((ids::LIVE_CHANNEL, LiveCategory::All)));
    app.press(Key::Up);
    assert!(tuned(&mut app, 2), "Up isn't the next channel");
    app.press(Key::ChannelDown);
    assert!(app.until(|a| a.watches(ids::LIVE_CHANNEL) == 2), "channel down isn't the one before");
    // Down from the first goes round to the last.
    app.press(Key::Down);
    assert!(tuned(&mut app, ids::LIVE_CHANNELS), "Down from the first channel didn't go round");
    // Left, Right and the media keys do nothing on live.
    for key in [Key::Left, Key::Right, Key::PlayPause, Key::Rewind, Key::FastForward] {
        app.press(key);
    }
    app.advance(0.3);
    assert_eq!(app.state.live_last_watched.as_ref().map(|l| l.0), Some(ids::LIVE_CHANNELS));
}

#[test]
fn next_and_previous_change_channel_as_channel_up_and_down_do() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    app.press(Key::Next);
    assert!(tuned(&mut app, 2), "Next isn't the next channel");
    app.press(Key::Previous);
    assert!(tuned(&mut app, ids::LIVE_CHANNEL), "Previous isn't the one before");
    assert_eq!(app.watches(2), 1, "Next tuned more than once");
}

#[test]
fn favourites_zap_among_themselves() {
    // Alex's favourites are 1, 3, 9 and 17.
    let mut app = watching(3, LiveCategory::Favorites);
    app.press(Key::ChannelUp);
    assert!(tuned(&mut app, 9), "the next favourite");
    app.press(Key::ChannelUp);
    app.press(Key::ChannelUp);
    assert!(tuned(&mut app, ids::LIVE_CHANNEL), "round the favourites' end");
    assert_eq!(app.state.live_last_watched, Some((ids::LIVE_CHANNEL, LiveCategory::Favorites)));
}

#[test]
fn a_typed_number_tunes_its_channel_or_says_there_is_none() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    app.press(Key::Digit(1));
    app.press(Key::Digit(2));
    assert!(app.shows("12"), "the digits typed");
    assert_eq!(app.watches(12), 0, "tuned before 1.4 s");
    app.advance(1.4);
    assert!(tuned(&mut app, 12), "12 wasn't tuned");
    assert!(!app.shows("12"));
    for _ in 0..6 {
        app.press(Key::Digit(9));
    }
    assert!(app.shows("99999"), "five digits at most");
    app.advance(1.4);
    assert!(app.until(|a| a.shows("No channel 99999")), "no word of the missing number");
    app.advance(2.1);
    assert!(!app.shows("No channel 99999"), "the notice stays past 2 s");
}

#[test]
fn ok_held_shows_the_controls_and_their_channels_opens_the_list() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    let engine = app.engine().unwrap();
    assert!(app.until(|_| engine.playing()), "the channel didn't play");
    app.frame(vec![InputEvent::Key(KeyEvent { key: Key::Ok, pressed: true, repeat: false, code: 0 })]);
    app.advance(0.6);
    app.frame(vec![InputEvent::Key(KeyEvent { key: Key::Ok, pressed: false, repeat: false, code: 0 })]);
    app.advance(0.2);
    let channels = app.id(&["bar", "channels"]);
    assert!(app.until(|a| a.focused() == Some(channels)), "the controls' Channels hasn't focus");
    assert!(app.shows("LIVE"));
    assert!(!app.shows("Search channels"), "the held OK opened the list as it was let go");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Search channels")), "the list didn't open");
}

/// Where the last frame drew `text`'s top, if it did.
fn text_top(app: &TestApp, text: &str) -> Option<f32> {
    app.list.cmds.iter().find_map(|c| match c {
        Cmd::Text { origin, layout, .. } if &*layout.text == text => Some(origin.y),
        _ => None,
    })
}

#[test]
fn sound_only_keeps_clear_of_the_controls() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    // The simulator's player shows no picture: a channel plays as sound only.
    assert!(app.until(|a| a.shows("Sound only")), "the channel isn't sound only");
    let alone = text_top(&app, "Sound only").unwrap();
    app.frame(vec![InputEvent::Key(KeyEvent { key: Key::Ok, pressed: true, repeat: false, code: 0 })]);
    app.advance(0.6);
    app.frame(vec![InputEvent::Key(KeyEvent { key: Key::Ok, pressed: false, repeat: false, code: 0 })]);
    app.advance(0.4);
    let channels = app.id(&["bar", "channels"]);
    assert!(app.until(|a| a.focused() == Some(channels)), "the controls' Channels hasn't focus");
    // Its caption and what's on rise above the bar's channel name, which they crowded.
    let caption = text_top(&app, "Sound only").unwrap();
    let name = text_top(&app, "Voyage News").expect("the bar names the channel");
    assert!(caption < alone, "the block didn't rise: {caption} from {alone}");
    let on = app.list.cmds.iter().filter_map(|c| match c {
        Cmd::Text { origin, layout, .. } if origin.y > caption && origin.y < name => Some(&*layout.text),
        _ => None,
    });
    assert_eq!(on.count(), 1, "what's on isn't between the caption and the bar");
    assert!(caption + 2.0 * 30.0 < name, "the caption at {caption} crowds the bar's name at {name}");
}

#[test]
fn ok_opens_the_channel_list_which_moves_the_picture_aside() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    let engine = app.engine().unwrap();
    assert!(app.until(|_| engine.playing()), "the channel didn't play");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Search channels")), "OK didn't open the list");
    assert_eq!(app.engine().unwrap().rect(), Some(picture_aside()));
    // The list opens on the channel playing; the next row tunes the next channel.
    let row = |a: &TestApp, i: u64| a.id(&["list", "rows"]).with(i);
    assert!(app.until(|a| a.focused() == Some(row(a, 0))), "focus isn't on the channel playing: {:?}", app.focused());
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(row(&app, 1)));
    app.press(Key::Ok);
    assert!(tuned(&mut app, 2), "the list's row didn't tune");
    app.advance(0.4);
    assert!(!app.shows("Search channels"), "choosing a channel closes the list");
    assert_eq!(app.engine().unwrap().rect(), None, "the picture is full screen again");
}

#[test]
fn menu_opens_and_closes_the_list_and_back_leaves_last() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    app.press(Key::Menu);
    assert!(app.until(|a| a.shows("Search channels")), "Menu didn't open the list");
    assert!(app.shows("All channels"), "the list's heading names the category");
    app.press(Key::Menu);
    app.advance(0.4);
    assert!(!app.shows("Search channels"), "Menu didn't close the list");
    app.press(Key::Menu);
    assert!(app.until(|a| a.shows("Search channels")));
    app.press(Key::Back);
    app.advance(0.4);
    assert!(!app.shows("Search channels"), "Back didn't close the list");
    assert!(matches!(app.route(), Some(Route::LivePlayer { .. })), "Back left with the list open");
    app.press(Key::Back);
    assert!(!matches!(app.route(), Some(Route::LivePlayer { .. })), "Back didn't leave");
    // Leaving lets the session go on the server.
    assert!(app.until(|a| {
        a.server.journal().iter().any(|e| e.method == "DELETE" && e.path.starts_with("/api/live/sessions/"))
    }));
}

/// The app on the player of a channel the server doesn't have, its trouble card showing.
fn missing() -> TestApp {
    let mut app = TestApp::start();
    app.open(Route::LivePlayer { channel_id: 9999, category: LiveCategory::All, pressed_at: 0.0 });
    assert!(app.until(|a| a.shows("This channel isn't here any more")), "no card for the missing channel");
    app
}

#[test]
fn other_channels_play_after_the_first_couldnt_be_had() {
    let mut app = missing();
    let others = app.id(&["trouble", "others"]);
    assert!(app.until(|a| a.focused() == Some(others)), "Other channels hasn't focus");
    app.press(Key::Ok);
    assert!(app.until(|a| a.shows("Search channels")), "the list didn't open");
    // The missing channel isn't listed: the list opens on the first.
    let first = app.id(&["list", "rows"]).with(0_u64);
    assert!(app.until(|a| a.focused() == Some(first)), "focus isn't on the first channel");
    app.press(Key::Ok);
    assert!(tuned(&mut app, ids::LIVE_CHANNEL), "the channel chosen wasn't watched");
    assert!(app.until(|a| !a.shows("This channel isn't here any more")), "the card stayed");
}

#[test]
fn a_number_or_channel_up_plays_after_the_first_couldnt_be_had() {
    let mut app = missing();
    app.press(Key::Digit(4));
    app.advance(1.4);
    assert!(tuned(&mut app, 4), "the number typed wasn't watched");

    let mut app = missing();
    // Not in the category: channel up goes to its first channel.
    app.press(Key::ChannelUp);
    assert!(tuned(&mut app, ids::LIVE_CHANNEL), "channel up did nothing");
}

#[test]
fn the_channel_playing_isnt_tuned_again() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    app.press(Key::Menu);
    let first = app.id(&["list", "rows"]).with(0_u64);
    assert!(app.until(|a| a.focused() == Some(first)), "the list isn't on the channel playing");
    app.press(Key::Ok);
    app.advance(0.4);
    assert!(!app.shows("Search channels"), "choosing it closes the list");
    // Its number typed: the box goes, nothing else happens.
    app.press(Key::Digit(1));
    assert!(app.shows("1"), "the digit typed");
    app.advance(1.4);
    assert!(app.until(|a| !a.shows("1")), "the number wasn't looked up");
    app.advance(0.5);
    assert_eq!(app.watches(ids::LIVE_CHANNEL), 1, "the channel playing was watched again");
}

#[test]
fn the_groups_open_on_the_category_listed() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    app.press(Key::Menu);
    let first = app.id(&["list", "rows"]).with(0_u64);
    assert!(app.until(|a| a.focused() == Some(first)));
    app.press(Key::Up);
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(app.id(&["list", "heading"])));
    app.press(Key::Ok);
    // Favourites, Recently watched, then All channels.
    let all = app.id(&["list", "groups"]).with(2_u64);
    assert!(app.until(|a| a.focused() == Some(all)), "focus isn't on All channels: {:?}", app.focused());
}

#[test]
fn every_connection_in_use_offers_to_watch_here_instead() {
    let mut app = TestApp::start();
    app.open(Route::LivePlayer {
        channel_id: ids::LIVE_CONFLICT_CHANNEL,
        category: LiveCategory::All,
        pressed_at: 0.0,
    });
    assert!(app.until(|a| a.shows("All connections are in use")), "no busy card");
    assert!(app.shows("Its one connection is watching:"));
    assert!(app.shows("You, on another device"));
    assert!(app.shows("Try again") && app.shows("Other channels") && app.shows("Back to the guide"));
    let take = app.id(&["trouble"]).with(("take", 0_u64));
    assert!(app.until(|a| a.focused() == Some(take)), "Watch here instead hasn't focus");
    app.press(Key::Ok);
    let path = format!("/api/live/channels/{}/watch", ids::LIVE_CONFLICT_CHANNEL);
    let took = |a: &mut TestApp| {
        a.server
            .writes("POST", &path)
            .iter()
            .any(|e| e.body.contains(&format!("\"take_over\":{}", ids::LIVE_BUSY_CHANNEL)))
    };
    assert!(app.until(took), "no watch taking the connection over");
    assert!(app.until(|a| !a.shows("All connections are in use")), "the card stayed");
}

/// The live session the player watches as, once the server lists it with what its player obeys,
/// and the event stream it hears the remote on is open.
fn remote_session(app: &mut TestApp) -> String {
    let mut found = None;
    let heard = app.until(|a| {
        found = a
            .server
            .sessions_of(ids::ALEX)
            .into_iter()
            .find(|s| s["kind"] == "live" && s["controls"][0] == "next" && s["controls"][1] == "previous");
        found.is_some() && a.server.event_listeners() > 0
    });
    assert!(heard, "the player never said it obeys the remote, or the events never connected");
    let session = found.unwrap();
    assert_eq!(session["device_kind"], "tv");
    session["session_id"].as_str().unwrap().to_string()
}

fn remote(session: &str, command: &str) -> ServerEvent {
    ServerEvent::PlayerCommand {
        session_id: session.into(),
        command: command.into(),
        position_ms: None,
        offset_ms: None,
        stream_index: None,
        quality: None,
    }
}

#[test]
fn the_remotes_next_and_previous_change_channel_as_channel_up_and_down_do() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    let first = remote_session(&mut app);
    // Another session's, or what live doesn't do: nothing.
    app.server.emit(remote("someone-elses", "next"));
    app.server.emit(remote(&first, "pause"));
    app.advance(0.5);
    assert_eq!(app.watches(2), 0);
    app.server.emit(remote(&first, "next"));
    assert!(tuned(&mut app, 2), "the remote's next isn't the next channel");
    let second = remote_session(&mut app);
    assert_ne!(first, second);
    app.server.emit(remote(&second, "previous"));
    assert!(app.until(|a| a.watches(ids::LIVE_CHANNEL) == 2), "the remote's previous isn't the one before");
    // Down from the first goes round to the last, as the keys' does.
    let third = remote_session(&mut app);
    app.server.emit(remote(&third, "previous"));
    assert!(tuned(&mut app, ids::LIVE_CHANNELS), "the remote's previous from the first didn't go round");
}

#[test]
fn live_tv_stopped_from_another_device_says_so_with_the_message_on_its_own_line() {
    let mut app = watching(ids::LIVE_CHANNEL, LiveCategory::All);
    let session = remote_session(&mut app);
    app.server.emit(ServerEvent::SessionStopped {
        session_id: session.clone(),
        message: Some("Time for bed".into()),
        by_admin: false,
    });
    assert!(
        app.until(|a| a.shows("Live TV stopped") && a.shows("Live TV was stopped from another device.\nTime for bed")),
        "the card never said who stopped it"
    );
    assert!(app.shows("Watch again"));
    // Nothing is watched again by itself, and the remote changes nothing now.
    app.server.emit(remote(&session, "next"));
    app.advance(1.0);
    assert_eq!((app.watches(ids::LIVE_CHANNEL), app.watches(2)), (1, 0));
}

/// For a player watching channel 2 through Alex's favourites, which don't have it: the server made
/// slow, so a step from 2, which asks the server where it is, finds its channel 1.5 s later. The live
/// session the remote acts on.
fn stepping_from_outside_the_favourites(app: &mut TestApp) -> String {
    let session = remote_session(app);
    app.server.set_mode(Mode::Slow).unwrap();
    session
}

/// Lets `secs` pass on the real clock, drawing frames, so answers from other threads come.
fn wait(app: &mut TestApp, secs: f64) {
    let start = std::time::Instant::now();
    app.until(|_| start.elapsed().as_secs_f64() > secs);
}

#[test]
fn a_channel_picked_while_the_remotes_step_finds_its_channel_stays() {
    let mut app = watching(2, LiveCategory::Favorites);
    app.press(Key::Menu);
    // Not a favourite: the list opens on the first, 1 (where the step goes); the next is 3.
    let row = |a: &TestApp, i: u64| a.id(&["list", "rows"]).with(i);
    assert!(app.until(|a| a.focused() == Some(row(a, 0))), "the list didn't open on the first favourite");
    let session = stepping_from_outside_the_favourites(&mut app);
    app.server.emit(remote(&session, "next"));
    // The remote's next comes, and its step asks where channel 2 is.
    wait(&mut app, 0.5);
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(tuned(&mut app, 3), "the channel picked wasn't watched");
    // The step's answer comes, and the channel picked plays on.
    wait(&mut app, 2.5);
    assert_eq!(app.watches(ids::LIVE_CHANNEL), 0, "the remote's step tuned over the channel picked");
    assert_eq!(app.state.live_last_watched.as_ref().map(|l| l.0), Some(3));
}

#[test]
fn a_remote_step_whose_channel_comes_after_a_stop_tunes_nothing() {
    let mut app = watching(2, LiveCategory::Favorites);
    let session = stepping_from_outside_the_favourites(&mut app);
    app.server.emit(remote(&session, "next"));
    wait(&mut app, 0.5);
    app.server.emit(ServerEvent::SessionStopped { session_id: session, message: None, by_admin: false });
    assert!(app.until(|a| a.shows("Live TV stopped")), "Live TV didn't stop");
    wait(&mut app, 2.5);
    assert_eq!(app.watches(ids::LIVE_CHANNEL), 0, "the remote's step tuned after Live TV stopped");
    assert!(app.shows("Live TV stopped"), "the card went");
}

#[test]
fn watch_again_while_the_remotes_step_finds_its_channel_stays() {
    let mut app = watching(2, LiveCategory::Favorites);
    let session = stepping_from_outside_the_favourites(&mut app);
    app.server.emit(remote(&session, "next"));
    wait(&mut app, 0.5);
    app.server.emit(ServerEvent::SessionStopped { session_id: session, message: None, by_admin: false });
    let again = app.id(&["trouble"]).with("retry");
    assert!(app.until(|a| a.focused() == Some(again)), "Watch again hasn't focus");
    // The same channel again, before the step's answer comes: the step doesn't tune over it.
    app.press(Key::Ok);
    assert!(app.until(|a| a.watches(2) == 2), "Watch again didn't watch the channel again");
    wait(&mut app, 2.5);
    assert_eq!(app.watches(ids::LIVE_CHANNEL), 0, "the remote's step tuned over Watch again");
}

#[test]
fn a_cast_channel_is_tuned_in_the_live_player_with_where_it_came_from_over_it() {
    let mut app = TestApp::start();
    let id = app.state.install_id().to_string();
    // The TV takes Live TV once the server said the profile has it.
    let live = |a: &TestApp| {
        a.server.receivers_of(ids::ALEX).iter().any(|r| r["receiver_id"] == *id && r["casts"][1] == "live")
    };
    assert!(app.until(|a| live(a)), "the TV never took casts of Live TV: {:?}", app.server.receivers_of(ids::ALEX));
    let phone = Some("pleyAndroid/0.1.5 (Android 14; phone)");
    assert_eq!(app.server.cast(ids::ALEX, &id, &CastBody::channel(2), phone), 202);
    assert!(app.until(|a| a.watches(2) == 1), "the channel cast was never watched");
    assert!(matches!(app.route(), Some(Route::LivePlayer { channel_id: 2, category: LiveCategory::All, .. })));
    assert!(app.until(|a| a.shows("Playing from Pley for Android")), "no word of where it came from");
    // Channel up goes on through every channel, as from search.
    app.press(Key::ChannelUp);
    assert!(tuned(&mut app, 3), "channel up from the cast channel");

    // Another channel cast takes this one's place: it leaves, and the other is watched.
    assert_eq!(app.server.cast(ids::ALEX, &id, &CastBody::channel(12), phone), 202);
    assert!(app.until(|a| a.watches(12) == 1), "the second channel cast was never watched");
    assert!(app.until(|a| {
        a.server.journal().iter().any(|e| e.method == "DELETE" && e.path.starts_with("/api/live/sessions/"))
    }));
    assert_eq!(app.shell.depth(), 2, "the second player opened over the first");
}

#[test]
fn a_step_still_finding_its_channel_as_the_tv_hides_the_app_tunes_nothing() {
    let mut app = watching(2, LiveCategory::Favorites);
    stepping_from_outside_the_favourites(&mut app);
    app.press(Key::ChannelUp);
    let now = app.time;
    app.shell.visible(false, &mut app.state, &mut app.ui, now);
    wait(&mut app, 2.5);
    assert_eq!(app.watches(ids::LIVE_CHANNEL), 0, "the step tuned while the app was hidden");
}
