//! A video on a drive plugged into the TV, played with no server behind it
//! (the Samsung app's `LocalPlayerSource` and PlayerModelTests): as it is,
//! never converted, its audio switched in place, and nothing asked of a
//! server that only a server answers.

mod common;

use common::*;
use pley_core::media::{AudioTrack, MediaKind};
use pley_core::model::PlaybackMode;
use pley_player::controller::Phase;
use pley_player::details::LOCAL_STUCK;

/// A drive's film played by `World`'s controller: a pretend server that says it is local.
fn on_drive(file: pley_core::model::MediaFile, dv: &[&str]) -> World {
    let fake = Fake::new(item(file), |_| info(|i| i.url = "file:///tmp/usb/sda/sda1/Film.mkv".into()));
    fake.set_local();
    World::with_dv(fake, dv, |env| env.keyed_urls = false)
}

#[test]
fn a_dolby_vision_profile_7_file_on_a_drive_plays_as_it_is() {
    let mut w = on_drive(dv_file(7, "mkv"), &[]);
    w.start();
    w.open();
    // A server would be asked again for a remux; a drive has none, and would refuse it.
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto]);
    assert_eq!(w.sim.loads().len(), 1);
    assert_eq!(w.c.state().phase, Phase::Playing);
}

#[test]
fn a_dolby_vision_mkv_on_a_tv_before_webos_25_asks_for_no_remux() {
    let mut w = on_drive(dv_file(8, "mkv"), &["mp4", "ts"]);
    w.start();
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto]);
    assert_eq!(w.sim.loads()[0].kind, MediaKind::File);
}

#[test]
fn a_picture_that_never_starts_fails_after_12_s_with_what_to_do() {
    let mut w = on_drive(file(), &[]);
    w.sim.set_load_ms(60_000);
    w.start();
    w.advance(11_500);
    assert_eq!(w.c.state().phase, Phase::Preparing);
    w.advance(1_000);
    assert_eq!(w.c.state().phase, Phase::Error { message: LOCAL_STUCK.into(), can_retry: true, title: None });
    assert!(w.fake.questions().is_empty(), "{:?}", w.fake.questions());
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto]);
}

#[test]
fn an_engine_error_asks_no_server_and_says_nothing_converts_it() {
    let mut w = on_drive(file(), &[]);
    w.start();
    w.open();
    // The network: the file again, as it is.
    w.sim.fail(2, "connection failed");
    w.advance(100);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto, PlaybackMode::Auto]);
    w.open();
    // The decoder: a conversion, which nothing makes.
    w.sim.fail(3, "decoder");
    w.advance(100);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto, PlaybackMode::Auto, PlaybackMode::Transcode]);
    assert_eq!(
        w.c.state().phase,
        Phase::Error { message: "Nothing converts it.".into(), can_retry: true, title: None }
    );
    assert!(w.fake.questions().is_empty(), "{:?}", w.fake.questions());
}

#[test]
fn an_audio_switch_never_loads_the_file_again() {
    let mut f = file();
    f.streams[2] = stream(2, "audio", "ac3", Some("jpn"), false, |s| s.channels = Some(6));
    let mut w = on_drive(f, &[]);
    // The TV names the file's second track another way.
    w.sim.set_tracks(
        ["eng", "und"]
            .iter()
            .enumerate()
            .map(|(ordinal, l)| AudioTrack { ordinal, language: Some(l.to_string()), ..AudioTrack::default() })
            .collect(),
    );
    w.start();
    w.open();
    w.c.select_audio(2);
    w.settle();
    // The track it starts with, then the one asked for, by the file's order.
    assert_eq!(w.sim.switched(), [0, 1]);
    assert_eq!((w.sim.loads().len(), w.fake.requests().len()), (1, 1));
    w.c.select_audio(1);
    w.settle();
    assert_eq!(w.sim.switched(), [0, 1, 0]);
    assert_eq!((w.sim.loads().len(), w.fake.requests().len()), (1, 1));
}

#[test]
fn a_drives_film_offers_no_quality_previews_next_or_search() {
    let mut w = on_drive(file(), &[]);
    w.start();
    w.open();
    w.play(30, 1.5);
    w.sim.end();
    w.settle();
    let s = w.c.state();
    assert!(s.qualities.is_empty());
    assert_eq!(s.quality, "Original");
    assert_eq!((s.trickplay.is_none(), s.can_find_subtitles), (true, false));
    assert!(w.fake.questions().is_empty(), "{:?}", w.fake.questions());
}
