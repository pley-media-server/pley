//! The controller's clocks on `SimEngine`: the reports' cadence and Up Next's
//! timing.

mod common;

use common::*;
use pley_core::model::{ItemKind, ItemSummary, Marker, MarkerKind, UserState};
use pley_player::controller::Phase;

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
    credits_until(7_200_000, true, tweak)
}

/// An episode of 2 h whose credits run from 7,100 s to `end_ms`, and the episode after it. Unless
/// `known`, neither the item, the session nor the player says how long it is.
fn credits_until(end_ms: i64, known: bool, tweak: impl FnOnce(&mut pley_player::PlayerEnv)) -> World {
    let mut detail = item(file());
    detail.summary.kind = ItemKind::Episode;
    detail.summary.grandparent_title = Some("Show".into());
    detail.summary.parent_index = Some(1);
    detail.summary.index = Some(1);
    if !known {
        detail.summary.duration_ms = None;
        detail.files[0].duration_ms = None;
    }
    detail.markers =
        vec![Marker { file_id: 7, kind: MarkerKind::Credits, start_ms: 7_100_000, end_ms, ..Marker::default() }];
    let fake = Fake::new(detail, move |req| {
        info(|i| {
            i.session_id = format!("s{}", req.item_id);
            if !known {
                i.duration_ms = 0;
            }
        })
    });
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
fn credits_that_something_follows_bring_skip_credits_and_up_next_comes_at_the_end() {
    // The credits stop a minute before the end: an after-credits scene, or a show's last minutes.
    let mut w = credits_until(7_140_000, true, |_| ());
    w.c.seek_to(7_099_000);
    w.advance(2_500);
    assert_eq!(w.c.state().skip_label.as_deref(), Some("Skip Credits"));
    assert!(w.c.state().up_next.is_none());
    // Through the rest of the credits and what follows them: nothing until the end.
    w.advance(97_000);
    assert!(w.c.state().up_next.is_none());
    w.advance(3_000);
    let card = w.c.state().up_next.clone().expect("Up Next shows at the end");
    assert_eq!(card.item_id, 71);
    assert!(card.seconds_left.is_some());
}

#[test]
fn credits_ending_10_s_before_the_end_bring_up_next() {
    // What follows them is a studio's logo, not a scene.
    let mut w = credits_until(7_190_000, true, |_| ());
    w.c.seek_to(7_099_000);
    w.advance(2_500);
    assert_eq!(w.c.state().up_next.as_ref().map(|c| c.item_id), Some(71));
}

#[test]
fn skip_shows_alone_for_8_s_from_entering_a_marker_then_only_with_the_controls() {
    // Credits something follows: Skip Credits without Up Next.
    let mut w = credits_until(7_140_000, true, |_| ());
    w.c.seek_to(7_110_000);
    w.advance(600);
    assert_eq!(w.c.state().skip_label.as_deref(), Some("Skip Credits"));
    assert!(w.c.state().skip_alone);
    w.advance(7_000);
    assert!(w.c.state().skip_alone, "7.6 s in");
    w.advance(1_000);
    assert!(!w.c.state().skip_alone, "8.6 s in");
    assert_eq!(w.c.state().skip_label.as_deref(), Some("Skip Credits"), "still offered, with the controls");
    assert_eq!(w.c.state().phase, Phase::Playing, "nothing else happens by itself");
    // A seek inside the same credits: nothing new.
    w.c.seek_to(7_120_000);
    w.advance(1_000);
    assert!(!w.c.state().skip_alone);
    // Out of them and back in: a fresh 8 s.
    w.c.seek_to(7_000_000);
    w.advance(1_000);
    assert_eq!(w.c.state().skip_label, None);
    assert!(!w.c.state().skip_alone);
    w.c.seek_to(7_110_000);
    w.advance(1_000);
    assert!(w.c.state().skip_alone);
}

#[test]
fn skip_credits_shows_alone_for_8_s_from_when_up_next_is_canceled() {
    let mut w = episode();
    w.c.seek_to(7_099_000);
    w.advance(2_500);
    assert!(w.c.state().up_next.is_some());
    assert!(!w.c.state().skip_alone, "the card takes its place");
    w.advance(3_000);
    w.c.cancel_up_next();
    w.advance(100);
    assert_eq!(w.c.state().skip_label.as_deref(), Some("Skip Credits"));
    assert!(w.c.state().skip_alone);
    w.advance(7_800);
    assert!(w.c.state().skip_alone, "7.9 s after Cancel");
    w.advance(200);
    assert!(!w.c.state().skip_alone, "8.1 s after Cancel");
}

#[test]
fn credits_of_a_file_whose_length_isnt_known_bring_skip_credits_only() {
    let mut w = credits_until(7_200_000, false, |_| ());
    w.c.seek_to(7_099_000);
    w.advance(2_500);
    assert_eq!(w.c.state().skip_label.as_deref(), Some("Skip Credits"));
    assert!(w.c.state().up_next.is_none());
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
