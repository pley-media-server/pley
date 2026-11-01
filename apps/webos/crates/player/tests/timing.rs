//! The controller's clocks on `SimEngine`: the reports' cadence, Up Next's
//! timing, and the sleep timer.

mod common;

use common::*;
use pley_core::model::{ItemKind, ItemSummary, Marker, MarkerKind, UserState};
use pley_player::controller::Phase;
use pley_player::sleep::SleepChoice;

/// The progress reports and session ends said so far.
fn reports(w: &World) -> Vec<String> {
    w.fake.calls().into_iter().filter(|c| c.starts_with("progress") || c.starts_with("delete")).collect()
}

#[test]
fn reports_every_10_s_while_playing_at_once_on_pause_every_30_s_paused_and_1_s_after_the_last_seek() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    // The session was armed as it was taken, at 0: the first report 10 s on, then every 10 s.
    w.advance(20_000 - 300);
    assert_eq!(reports(&w), ["progress playing 609700", "progress playing 619700"]);
    w.c.pause();
    w.settle();
    assert_eq!(reports(&w).last().map(String::as_str), Some("progress paused 619700"));
    // Paused: every 30 s.
    w.advance(29_000);
    assert_eq!(reports(&w).len(), 3);
    w.advance(1_000);
    assert_eq!(reports(&w).last().map(String::as_str), Some("progress paused 619700"));
    assert_eq!(reports(&w).len(), 4);
    // Three seeks in a row: one report, 1 s after the last.
    w.c.seek_to(700_000);
    w.advance(500);
    w.c.seek_to(710_000);
    w.advance(500);
    w.c.seek_to(720_000);
    w.advance(900);
    assert_eq!(reports(&w).len(), 4);
    w.advance(100);
    assert_eq!(reports(&w).last().map(String::as_str), Some("progress paused 720000"));
    // Closing: stopped, then the session's end.
    w.c.close();
    w.settle();
    assert_eq!(reports(&w)[5..], ["progress stopped 720000", "delete s1"]);
}

/// An episode whose credits start 100 s before its end, and the episode after it.
fn episode() -> World {
    episode_with(|_| ())
}

fn episode_with(tweak: impl FnOnce(&mut pley_player::PlayerEnv)) -> World {
    let mut detail = item(file());
    detail.summary.kind = ItemKind::Episode;
    detail.summary.grandparent_title = Some("Show".into());
    detail.summary.parent_index = Some(1);
    detail.summary.index = Some(1);
    detail.markers = vec![Marker {
        file_id: 7,
        kind: MarkerKind::Credits,
        start_ms: 7_100_000,
        end_ms: 7_200_000,
        ..Marker::default()
    }];
    let fake = Fake::new(detail, |req| info(|i| i.session_id = format!("s{}", req.item_id)));
    fake.set_next(Some(ItemSummary {
        id: 71,
        kind: ItemKind::Episode,
        title: "Two".into(),
        parent_index: Some(1),
        index: Some(2),
        thumb_url: Some("/api/items/71/images/thumb?v=3".into()),
        user_state: UserState { position_ms: 0, ..UserState::default() },
        ..ItemSummary::default()
    }));
    let mut w = World::new(fake, tweak);
    w.start();
    w.open();
    w
}

#[test]
fn credits_show_up_next_and_its_countdown_plays_the_next_episode_10_s_on() {
    let mut w = episode();
    assert_eq!(w.c.state().title, "Show");
    assert_eq!(w.c.state().subtitle.as_deref(), Some("S1 · E1 · Film"));
    w.c.seek_to(7_099_000);
    w.advance(2_500);
    let card = w.c.state().up_next.clone().expect("Up Next shows");
    assert_eq!((card.item_id, card.heading.as_str()), (71, "Up next · S1 · E2"));
    assert_eq!(card.image_url.as_deref(), Some("http://nas:7539/api/items/71/images/thumb?v=3&w=480"));
    assert_eq!(w.c.state().skip_label.as_deref(), Some("Skip Credits"));
    let left = card.seconds_left.unwrap();
    assert!((9..=10).contains(&left), "{left}");
    w.advance(5_000);
    assert_eq!(w.c.state().up_next.as_ref().and_then(|c| c.seconds_left), Some(left - 5));
    assert_eq!(w.fake.requests().len(), 1);
    w.advance(5_100);
    let last = w.fake.requests().last().cloned().unwrap();
    assert_eq!((last.item_id, last.start_ms), (71, Some(0)));
    assert!(w.c.state().up_next.is_none());
    // The episode before was ended first.
    assert!(w.fake.calls().contains(&"delete s70".to_string()));
}

#[test]
fn up_next_waits_without_a_countdown_when_autoplay_is_off_and_cancelling_it_at_the_end_leaves() {
    let mut w = episode_with(|env| env.autoplay_next = false);
    w.sim.end();
    w.advance(100);
    let card = w.c.state().up_next.clone().expect("Up Next shows at the end");
    assert_eq!(card.seconds_left, None);
    assert_eq!(w.c.state().phase, Phase::Paused);
    w.advance(20_000);
    assert_eq!(w.fake.requests().len(), 1);
    w.c.cancel_up_next();
    assert_eq!(w.c.state().phase, Phase::Ended);
}

#[test]
fn the_sleep_timer_pauses_after_its_minutes_with_a_notice_in_its_last_30_s() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    w.c.set_sleep(SleepChoice::Minutes(15));
    assert_eq!(w.c.state().sleep.minutes_left, Some(15));
    w.advance(15 * 60_000 - 20_000);
    assert_eq!(w.c.state().sleep.seconds_left, Some(20));
    w.advance(20_000);
    assert!(w.c.state().sleep.fired);
    assert!(!w.c.wants_to_play());
    assert_eq!(w.c.state().phase, Phase::Paused);
    w.c.play();
    assert!(!w.c.state().sleep.fired);
}

#[test]
fn a_sleep_timer_ending_with_the_episode_holds_up_nexts_countdown() {
    let mut w = episode();
    w.c.set_sleep(SleepChoice::EndOfItem);
    w.sim.end();
    w.advance(100);
    assert!(w.c.state().sleep.fired);
    let card = w.c.state().up_next.clone().expect("Up Next shows");
    assert_eq!(card.seconds_left, None);
    w.advance(15_000);
    assert_eq!(w.fake.requests().len(), 1);
}
