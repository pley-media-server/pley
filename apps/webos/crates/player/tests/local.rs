//! A video on a drive plugged into the TV, played with no server behind it
//! (the Samsung app's `LocalPlayerSource` and PlayerModelTests): as it is,
//! never converted, its audio switched in place, and nothing asked of a
//! server that only a server answers.

mod common;

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use pley_base::Waker;
use pley_core::device::scratch_dir;
use pley_core::media::{AudioTrack, MediaKind};
use pley_core::model::PlaybackMode;
use pley_core::task::Tasks;
use pley_core::{CapsReport, Store};
use pley_local::LocalServer;
use pley_local::server::ITEM_ID;
use pley_player::Reporter;
use pley_player::controller::{Phase, PlaybackController};
use pley_player::details::{drive_unreadable, local_stuck};
use pley_player::env::{CapsProbe, PlayerEnv};
use pley_player::plan::PlayRequest;
use pley_player::sim::{SimClock, SimEngine};

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
    assert_eq!(w.c.state().phase, Phase::Error { message: local_stuck().into(), can_retry: true, title: None });
    assert!(w.fake.questions().is_empty(), "{:?}", w.fake.questions());
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto]);
}

#[test]
fn a_picture_that_played_then_stood_still_12_s_says_the_drive_may_be_out() {
    let mut w = on_drive(file(), &[]);
    w.start();
    w.open();
    w.advance(2_000);
    // The drive pulled out: the TV's player waits for data that never comes, saying nothing else.
    w.sim.wait(true);
    w.advance(11_500);
    assert_eq!(w.c.state().phase, Phase::Buffering);
    w.advance(1_000);
    // It played: the file is one this TV plays, so not "try another copy".
    let unreadable = Phase::Error { message: drive_unreadable().into(), can_retry: true, title: None };
    assert_eq!(w.c.state().phase, unreadable);
    assert!(w.fake.questions().is_empty(), "{:?}", w.fake.questions());
}

#[test]
fn a_file_that_played_and_then_never_starts_again_says_the_drive_may_be_out() {
    let mut w = on_drive(file(), &[]);
    w.start();
    w.open();
    w.advance(2_000);
    // The drive goes bad: the pipeline fails, the file starts again, and its picture never comes.
    w.sim.set_load_ms(60_000);
    w.sim.fail(2, "read error");
    w.advance(100);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto, PlaybackMode::Auto]);
    w.advance(12_500);
    let unreadable = Phase::Error { message: drive_unreadable().into(), can_retry: true, title: None };
    assert_eq!(w.c.state().phase, unreadable);
}

#[test]
fn a_drives_picture_track_waiting_to_be_read_again_is_read_at_once_after_a_seek() {
    let mut w = on_drive(file(), &[]);
    w.start();
    w.open();
    w.c.select_subtitle(Some(3));
    w.settle();
    let asks = |w: &World| w.fake.subtitle_asks().len();
    assert_eq!(asks(&w), 1);
    // Still being read out of the file: asked again 1 s after the answer, then 2 s, then 4 s.
    w.advance(1_000);
    assert_eq!(asks(&w), 2);
    w.advance(2_000);
    assert_eq!(asks(&w), 3);
    w.advance(1_000);
    assert_eq!(asks(&w), 3, "the next ask waits 4 s");
    // A seek: the reading goes where the film went, so what it has is asked for now.
    w.c.seek_to(600_000);
    w.settle();
    assert_eq!(asks(&w), 4, "not 3 s later");
    // And the waits start over, as in the Samsung app.
    w.advance(1_000);
    assert_eq!(asks(&w), 5);
    assert!(w.fake.subtitle_asks().iter().all(|a| a.ends_with("/subtitles/3.mks")), "{:?}", w.fake.subtitle_asks());
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
fn a_drive_pulled_out_mid_play_says_so_not_that_a_server_is_away() {
    let mut w = on_drive(file(), &[]);
    w.start();
    w.open();
    // The file can't be read any more: the TV's network errors, the file tried again each time.
    for _ in 0..10 {
        if matches!(w.c.state().phase, Phase::Error { .. }) {
            break;
        }
        w.sim.fail(2, "connection failed");
        w.advance(100);
        w.open();
    }
    let unreadable = Phase::Error { message: drive_unreadable().into(), can_retry: true, title: None };
    assert_eq!(w.c.state().phase, unreadable);
    assert!(w.fake.questions().is_empty(), "{:?}", w.fake.questions());
}

#[test]
fn a_drive_that_cant_be_read_at_the_start_says_so() {
    let mut w = on_drive(file(), &[]);
    w.fake.set_down(true);
    w.start();
    w.settle();
    let unreadable = Phase::Error { message: drive_unreadable().into(), can_retry: true, title: None };
    assert_eq!(w.c.state().phase, unreadable);
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

// ---- a film on a drive, played through its own server

/// An EBML element with an 8-byte size.
fn el(id: u64, parts: &[Vec<u8>]) -> Vec<u8> {
    let data = parts.concat();
    let mut out: Vec<u8> = id.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
    out.push(0x01);
    out.extend(&(data.len() as u64).to_be_bytes()[1..]);
    out.extend(data);
    out
}

fn u(v: u64) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn track(kind: u64, codec: &str, language: &str, default: bool, extra: Vec<u8>) -> Vec<u8> {
    let parts = [el(0xD7, &[u(1)]), el(0x83, &[u(kind)]), el(0x86, &[codec.as_bytes().to_vec()])];
    let flags = [el(0x88, &[u(default as u64)]), el(0x22B59C, &[language.as_bytes().to_vec()]), extra];
    el(0xAE, &[parts.concat(), flags.concat()])
}

/// Mad Max as the Samsung app's tests make it: HEVC in PQ with HDR10+ in its
/// first frame, TrueHD (the default), AC-3 and AAC, a text track without
/// Cues, and two chapters.
fn mad_max() -> Vec<u8> {
    let info = el(0x1549A966, &[el(0x2AD7B1, &[u(1_000_000)]), el(0x4489, &[7_380_000f64.to_be_bytes().to_vec()])]);
    let colour = el(0x55B0, &[el(0x55BA, &[u(16)]), el(0x55BB, &[u(9)]), el(0x55B2, &[u(10)])]);
    let video = el(0xE0, &[el(0xB0, &[u(1920)]), el(0xBA, &[u(800)]), colour]);
    let tracks = el(
        0x1654AE6B,
        &[
            track(1, "V_MPEGH/ISO/HEVC", "und", true, video),
            track(2, "A_TRUEHD", "eng", true, el(0xE1, &[el(0x9F, &[u(8)])])),
            track(2, "A_AC3", "eng", false, el(0xE1, &[el(0x9F, &[u(6)])])),
            track(2, "A_AAC", "rus", false, vec![]),
            track(0x11, "S_TEXT/UTF8", "eng", true, vec![]),
        ],
    );
    let chapter =
        |at: u64, name: &str| el(0xB6, &[el(0x91, &[u(at)]), el(0x80, &[el(0x85, &[name.as_bytes().to_vec()])])]);
    let chapters = el(0x1043A770, &[el(0x45B9, &[chapter(0, "Opening"), chapter(600_000_000_000, "The Road")])]);
    // A SimpleBlock of track 1 holding a prefix SEI with HDR10+'s T.35 message.
    let sei = [0x4E, 0x01, 0x04, 0x08, 0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x01, 0x40, 0x80];
    let frame = [&[0x81, 0, 0, 0x80][..], &(sei.len() as u32).to_be_bytes(), &sei].concat();
    let cluster = el(0x1F43B675, &[el(0xE7, &[u(0)]), el(0xA3, &[frame])]);
    let header = el(0x1A45DFA3, &[el(0x4282, &[b"matroska".to_vec()])]);
    [header, el(0x18538067, &[info, tracks, chapters, cluster])].concat()
}

#[test]
fn the_player_offers_a_drive_films_tracks_and_subtitle_files_and_its_hdr() {
    let dir = scratch_dir("player-drive");
    let film = dir.join("Mad Max.mkv");
    std::fs::write(&film, mad_max()).unwrap();
    std::fs::write(dir.join("Mad Max.en.srt"), "1\n00:00:01,000 --> 00:00:02,000\nWitness me\n").unwrap();
    let store = Store::open(scratch_dir("player-drive-store")).unwrap();
    let report = CapsReport { caps: lg(), lines: Vec::new(), dv_containers: vec!["mp4".into(), "ts".into()] };
    let caps: CapsProbe = Arc::new(move || Some(report.clone()));
    let server = Arc::new(LocalServer::new(film, dir.clone(), store, caps.clone()));
    let (engine, sim) = SimEngine::new(SimClock::manual());
    let mut env = PlayerEnv::new(server, Tasks::with_threads(2, Waker::noop()), Reporter::new(), caps, "webos-abc");
    env.keyed_urls = false;
    let mut c = PlaybackController::new(PlayRequest::item(ITEM_ID), env, Box::new(engine), 0);
    c.start();
    let until = Instant::now() + Duration::from_secs(5);
    while sim.loads().is_empty() {
        assert!(Instant::now() < until, "the film never loaded");
        c.tick(0);
        thread::sleep(Duration::from_millis(1));
    }
    let s = c.state();
    let subtitles: Vec<_> = s.subtitles.iter().map(|t| t.label.split(' ').next().unwrap_or("")).collect();
    assert_eq!(subtitles, ["Off", "English"]);
    assert_eq!(s.audio.len(), 3);
    assert!(s.qualities.is_empty());
    assert_eq!(s.chapters.len(), 2);
    let load = &sim.loads()[0];
    assert_eq!(load.kind, MediaKind::File);
    assert!(load.url.starts_with("file:///") && load.url.ends_with("/Mad%20Max.mkv"), "{}", load.url);
    // Playback details at once: nothing is asked of a server. LG shows no HDR10+.
    c.set_details_open(true);
    c.tick(0);
    let details = c.state().details.clone().expect("the details");
    let shown = details.rows.iter().find(|r| r.label == "Shown as").map(|r| r.value.as_str());
    assert_eq!(shown, Some("HDR10 (this screen doesn't show HDR10+)"));
    let _ = std::fs::remove_dir_all(&dir);
}
