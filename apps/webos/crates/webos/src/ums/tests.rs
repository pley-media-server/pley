//! The engine's state machine on scripted uMediaServer messages.

use std::time::{Duration, Instant};

use pley_base::Rect;
use pley_core::media::{AudioTrack, EngineEvent, LoadRequest, MediaKind, VideoFacts};
use pley_core::model::DynamicRange;
use serde_json::{Value, json};

use super::machine::*;
use super::plane::{Candidate, Plane, PlaneMemory};

fn machine(plane: PlaneKind, acb: bool) -> Machine {
    Machine::new(
        Config { app_id: "app.pley.tv".into(), plane, acb_available: acb, table: None, scale: 2.0 },
        PlaneMemory::for_firmware(None, "4.10.2-31"),
    )
}

fn movie() -> LoadRequest {
    let mut req = LoadRequest::new("http://10.0.0.2:8096/api/media/k/file.mkv", MediaKind::File);
    req.start_ms = 60_000;
    req.duration_hint_ms = Some(7_200_000);
    req.container = Some("mkv".into());
    req.video = Some(VideoFacts {
        codec: "hevc".into(),
        width: 3840,
        height: 2160,
        frame_rate: Some(23.976),
        dynamic_range: DynamicRange::Hdr10,
        dv_profile: None,
        bit_depth: Some(10),
    });
    req
}

fn msg(m: &mut Machine, serial: u64, message: Value, now: Instant) {
    m.input(Input::Message { serial, message }, now);
}

/// Loads `req`, answers the load with `_m1` and completes it.
fn loaded(m: &mut Machine, req: LoadRequest, t0: Instant) -> u64 {
    let serial = m.load(req, None, t0);
    m.input(Input::Loaded { serial, answer: Ok("_m1".into()) }, t0);
    msg(m, serial, json!({ "loadCompleted": { "mediaId": "_m1" } }), t0);
    serial
}

fn calls(actions: &[Action]) -> Vec<(Method, Value)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Call { method, payload, .. } => Some((*method, payload.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_film_loads_plays_and_ends() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = m.load(movie(), Some("_Window_Id_4"), t0);
    let actions = m.take_actions();
    let (method, payload) = &calls(&actions)[0];
    assert_eq!(*method, Method::Load);
    assert_eq!(payload["payload"]["option"]["windowId"], "_Window_Id_4");
    assert_eq!(payload["payload"]["option"]["transmission"]["playTime"]["start"], 60_000);
    assert!(actions.contains(&Action::WakeAfter(LOAD_BUDGET)));
    assert!(actions.contains(&Action::PlaceWindow {
        serial,
        source: Rect::new(0.0, 0.0, 3840.0, 2160.0),
        dest: Rect::new(0.0, 0.0, 1920.0, 1080.0)
    }));
    assert_eq!(m.position_ms(t0), 60_000);

    m.input(Input::Loaded { serial, answer: Ok("_m1".into()) }, t0);
    assert_eq!(m.take_actions(), [Action::Subscribe { serial, media_id: "_m1".into() }]);
    msg(&mut m, serial, json!({ "subscribed": true, "returnValue": true }), t0);
    msg(&mut m, serial, json!({ "loadCompleted": { "mediaId": "_m1" } }), t0);
    assert_eq!(m.take_events(), [EngineEvent::Loaded { duration_ms: Some(7_200_000) }]);
    assert_eq!(calls(&m.take_actions()), [(Method::Play, json!({ "mediaId": "_m1" }))]);

    msg(
        &mut m,
        serial,
        json!({ "sourceInfo": { "container": "mkv", "numPrograms": 1, "programInfo": [{
            "duration": 7_195_000,
            "audioTrackInfo": [
                { "language": "eng", "codec": "eac3", "channels": 6 },
                { "language": "fre", "codec": "aac", "channels": 2 }
            ],
            "videoTrackInfo": [{ "codec": "hevc", "width": 3840, "height": 2160 }]
        }] } }),
        t0,
    );
    assert_eq!(
        m.take_events(),
        [
            EngineEvent::AudioTracks(vec![
                AudioTrack { ordinal: 0, language: Some("eng".into()), codec: Some("eac3".into()), channels: Some(6) },
                AudioTrack { ordinal: 1, language: Some("fre".into()), codec: Some("aac".into()), channels: Some(2) },
            ]),
            EngineEvent::Loaded { duration_ms: Some(7_195_000) },
        ]
    );
    msg(
        &mut m,
        serial,
        json!({ "videoInfo": { "width": 3840, "height": 2160, "frameRate": 23.976, "hdrType": "HDR10", "bitRate": 40_000_000 } }),
        t0,
    );
    assert_eq!(
        m.take_events(),
        [EngineEvent::VideoInfo { width: 3840, height: 2160, fps: Some(23.976), dynamic_range: DynamicRange::Hdr10 }]
    );
    assert_eq!(
        m.take_actions(),
        [Action::PlaceWindow {
            serial,
            source: Rect::new(0.0, 0.0, 3840.0, 2160.0),
            dest: Rect::new(0.0, 0.0, 1920.0, 1080.0)
        }]
    );
    msg(&mut m, serial, json!({ "playing": { "mediaId": "_m1" } }), t0);
    assert_eq!(m.take_events(), [EngineEvent::Playing, EngineEvent::FirstPicture]);
    msg(&mut m, serial, json!({ "currentTime": { "currentTime": 61_000, "mediaId": "_m1" } }), t0);
    assert_eq!(m.position_ms(t0 + Duration::from_millis(400)), 61_400);

    let stats = m.stats();
    assert_eq!(stats.player, "webOS media pipeline");
    assert_eq!((stats.width, stats.height, stats.fps), (Some(3840), Some(2160), Some(23.976)));
    assert_eq!(stats.dynamic_range.as_deref(), Some("HDR10"));
    assert_eq!((stats.audio_codec.as_deref(), stats.audio_channels), (Some("eac3"), Some(6)));
    assert_eq!(stats.bitrate_kbps, Some(40_000));
    assert_eq!(stats.extra[0], ("Picture".into(), "exported window".into()));

    msg(&mut m, serial, json!({ "endOfStream": { "mediaId": "_m1" } }), t0);
    assert_eq!(m.take_events(), [EngineEvent::Ended]);
    assert_eq!(m.position_ms(t0 + Duration::from_secs(5)), 7_195_000);

    m.unload();
    let actions = m.take_actions();
    assert_eq!(actions[0], Action::Unsubscribe { serial });
    assert_eq!(calls(&actions), [(Method::Unload, json!({ "mediaId": "_m1" }))]);
    // The window goes once the pipeline is released.
    m.input(Input::Answered { serial, method: Method::Unload, error: None }, t0);
    assert_eq!(m.take_actions(), [Action::DestroyWindow { serial }]);
}

#[test]
fn any_error_before_the_picture_fails_the_load() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = loaded(&mut m, movie(), t0);
    m.take_events();
    m.take_actions();
    msg(&mut m, serial, json!({ "error": { "errorCode": 601, "errorText": "Resource Allocation Error" } }), t0);
    assert_eq!(
        m.take_events(),
        [EngineEvent::Error { code: 601, message: "Resource Allocation Error".into(), before_first_picture: true }]
    );
    assert_eq!(calls(&m.take_actions()), [(Method::Unload, json!({ "mediaId": "_m1" }))]);
    // Later messages of that session change nothing.
    msg(&mut m, serial, json!({ "playing": {} }), t0);
    assert!(m.take_events().is_empty());

    let serial = m.load(movie(), None, t0);
    m.input(Input::Loaded { serial, answer: Err(Failure { code: -1, message: "Unknown method".into() }) }, t0);
    assert_eq!(
        m.take_events(),
        [EngineEvent::Error { code: -1, message: "Unknown method".into(), before_first_picture: true }]
    );
}

#[test]
fn no_picture_within_20_seconds_fails_the_load() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    loaded(&mut m, movie(), t0);
    m.take_events();
    m.tick(t0 + Duration::from_secs(19));
    assert!(m.take_events().is_empty());
    m.tick(t0 + LOAD_BUDGET);
    assert!(matches!(m.take_events()[..], [EngineEvent::Error { code: ENGINE_ERROR, before_first_picture: true, .. }]));
}

#[test]
fn commands_given_while_loading_wait_for_the_pipeline() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let mut req = movie();
    req.autoplay = false;
    let serial = m.load(req, None, t0);
    m.take_actions();
    m.seek(90_000, t0);
    assert!(m.select_audio(1));
    m.play();
    assert_eq!(m.position_ms(t0), 90_000);
    assert!(calls(&m.take_actions()).is_empty());
    m.input(Input::Loaded { serial, answer: Ok("_m1".into()) }, t0);
    m.take_actions();
    msg(&mut m, serial, json!({ "loadCompleted": {} }), t0);
    assert_eq!(
        calls(&m.take_actions()),
        [
            (Method::Seek, json!({ "mediaId": "_m1", "position": 90_000 })),
            (Method::SelectTrack, json!({ "mediaId": "_m1", "type": "audio", "index": 1 })),
            (Method::Play, json!({ "mediaId": "_m1" })),
        ]
    );
}

#[test]
fn a_source_prepared_paused_shows_its_first_picture_without_playing() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let mut req = movie();
    req.autoplay = false;
    loaded(&mut m, req, t0);
    assert_eq!(
        m.take_events(),
        [EngineEvent::Loaded { duration_ms: Some(7_200_000) }, EngineEvent::FirstPicture, EngineEvent::Paused]
    );
    assert!(!calls(&m.take_actions()).iter().any(|(method, _)| *method == Method::Play));
    // A shown picture is no load to fail.
    m.tick(t0 + Duration::from_secs(30));
    assert!(m.take_events().is_empty());
}

#[test]
fn buffering_is_reported_only_while_playing_and_not_seeking() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = loaded(&mut m, movie(), t0);
    msg(&mut m, serial, json!({ "playing": {} }), t0);
    m.take_events();
    m.seek(120_000, t0);
    msg(&mut m, serial, json!({ "bufferingStart": {} }), t0);
    msg(&mut m, serial, json!({ "bufferingEnd": {} }), t0);
    msg(&mut m, serial, json!({ "seekDone": {} }), t0);
    assert!(m.take_events().is_empty(), "a seek's wait isn't a stall");
    msg(&mut m, serial, json!({ "bufferingStart": {} }), t0);
    msg(&mut m, serial, json!({ "bufferingStart": {} }), t0);
    msg(
        &mut m,
        serial,
        json!({ "bufferRange": { "beginTime": 0, "endTime": 125_000, "remainingTime": 5_000, "percent": 2 } }),
        t0,
    );
    msg(&mut m, serial, json!({ "bufferingEnd": {} }), t0);
    assert_eq!(
        m.take_events(),
        [EngineEvent::Waiting(true), EngineEvent::Buffered { ahead_ms: 5_000 }, EngineEvent::Waiting(false)]
    );
    m.pause(t0);
    msg(&mut m, serial, json!({ "paused": {} }), t0);
    msg(&mut m, serial, json!({ "bufferingStart": {} }), t0);
    assert_eq!(m.take_events(), [EngineEvent::Paused]);
}

#[test]
fn the_position_runs_on_between_reports_at_the_rate_and_no_further_than_a_second() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = loaded(&mut m, movie(), t0);
    msg(&mut m, serial, json!({ "playing": {} }), t0);
    msg(&mut m, serial, json!({ "currentTime": 100_000 }), t0);
    assert_eq!(m.position_ms(t0 + Duration::from_millis(250)), 100_250);
    assert_eq!(m.position_ms(t0 + Duration::from_secs(4)), 101_000);
    assert!(m.set_rate(2.0));
    assert!(!m.set_rate(4.0));
    assert!(!m.set_rate(0.05));
    assert_eq!(m.position_ms(t0 + Duration::from_millis(250)), 100_500);
    assert_eq!(calls(&m.take_actions()).last().unwrap().0, Method::SetPlayRate);
    m.pause(t0 + Duration::from_millis(250));
    assert_eq!(m.position_ms(t0 + Duration::from_secs(3)), 100_500);
}

#[test]
fn a_tv_that_never_says_playing_still_moves_its_clock() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = loaded(&mut m, movie(), t0);
    m.take_events();
    msg(&mut m, serial, json!({ "currentTime": { "currentTime": 60_000 } }), t0);
    assert!(m.take_events().is_empty());
    msg(&mut m, serial, json!({ "currentTime": { "currentTime": 60_200 } }), t0);
    assert_eq!(m.take_events(), [EngineEvent::Playing, EngineEvent::FirstPicture]);
}

#[test]
fn a_decoder_taken_back_parks_until_the_next_command() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = loaded(&mut m, movie(), t0);
    msg(&mut m, serial, json!({ "playing": {} }), t0);
    msg(&mut m, serial, json!({ "currentTime": 500_000 }), t0);
    m.pause(t0);
    msg(&mut m, serial, json!({ "paused": {} }), t0);
    m.take_events();
    m.take_actions();
    msg(
        &mut m,
        serial,
        json!({ "error": { "errorCode": 600, "errorText": "VDEC resource is released by policy action" } }),
        t0,
    );
    assert!(m.take_events().is_empty(), "already paused: nothing to tell");
    assert!(m.is_parked());
    assert_eq!(calls(&m.take_actions()), [(Method::Unload, json!({ "mediaId": "_m1" }))]);
    assert_eq!(m.position_ms(t0 + Duration::from_secs(60)), 500_000);
    assert!(m.select_audio(1));

    let req = m.take_parked(None, true).unwrap();
    assert_eq!((req.start_ms, req.autoplay), (500_000, true));
    assert!(!m.is_parked());
    let serial = m.load(req, None, t0);
    m.input(Input::Loaded { serial, answer: Ok("_m2".into()) }, t0);
    msg(&mut m, serial, json!({ "loadCompleted": {} }), t0);
    let rebuilt = calls(&m.take_actions());
    assert_eq!(rebuilt[0].1["payload"]["option"]["transmission"]["playTime"]["start"], 500_000);
    assert!(rebuilt.contains(&(Method::SelectTrack, json!({ "mediaId": "_m2", "type": "audio", "index": 1 }))));
    assert!(rebuilt.contains(&(Method::Play, json!({ "mediaId": "_m2" }))));
}

#[test]
fn errors_after_the_picture_reach_the_controller() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = loaded(&mut m, movie(), t0);
    msg(&mut m, serial, json!({ "playing": {} }), t0);
    m.take_events();
    msg(&mut m, serial, json!({ "error": { "errorCode": 3, "errorText": "network" } }), t0);
    assert_eq!(
        m.take_events(),
        [EngineEvent::Error { code: 3, message: "network".into(), before_first_picture: false }]
    );
    assert!(!m.is_parked());
}

#[test]
fn a_load_answered_after_its_session_closed_is_unloaded() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let first = m.load(movie(), Some("_Window_Id_1"), t0);
    let second = m.load(movie(), Some("_Window_Id_2"), t0);
    m.take_actions();
    m.input(Input::Loaded { serial: first, answer: Ok("_old".into()) }, t0);
    assert_eq!(calls(&m.take_actions()), [(Method::Unload, json!({ "mediaId": "_old" }))]);
    msg(&mut m, first, json!({ "loadCompleted": {} }), t0);
    assert!(m.take_events().is_empty());
    m.input(Input::Answered { serial: first, method: Method::Unload, error: None }, t0);
    assert_eq!(m.take_actions(), [Action::DestroyWindow { serial: first }]);
    m.input(Input::Loaded { serial: second, answer: Ok("_new".into()) }, t0);
    assert_eq!(m.take_actions(), [Action::Subscribe { serial: second, media_id: "_new".into() }]);
}

#[test]
fn the_picture_follows_its_raster_and_the_rectangle_asked_for() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let serial = m.load(movie(), Some("_Window_Id_9"), t0);
    m.input(Input::Loaded { serial, answer: Ok("_m1".into()) }, t0);
    m.take_actions();
    // A 4:3 picture is pillarboxed in the full plane.
    msg(&mut m, serial, json!({ "videoInfo": { "width": 1440, "height": 1080 } }), t0);
    assert!(m.take_actions().contains(&Action::PlaceWindow {
        serial,
        source: Rect::new(0.0, 0.0, 1440.0, 1080.0),
        dest: Rect::new(240.0, 0.0, 1440.0, 1080.0)
    }));
    // The live channel list's 520 dp at the left, on the 2× plane.
    m.set_video_rect(Some(Rect::new(0.0, 0.0, 520.0, 540.0)));
    assert_eq!(
        m.take_actions(),
        [Action::PlaceWindow {
            serial,
            source: Rect::new(0.0, 0.0, 1440.0, 1080.0),
            dest: Rect::new(0.0, 150.0, 1040.0, 780.0)
        }]
    );
    // A rung of another size places it again; the same size doesn't.
    msg(&mut m, serial, json!({ "videoInfo": { "width": 1920, "height": 1080, "pixelAspectRatio": "1:1" } }), t0);
    assert_eq!(
        m.take_actions(),
        [Action::PlaceWindow {
            serial,
            source: Rect::new(0.0, 0.0, 1920.0, 1080.0),
            dest: Rect::new(0.0, 247.5, 1040.0, 585.0)
        }]
    );
    msg(&mut m, serial, json!({ "videoInfo": { "width": 1920, "height": 1080 } }), t0);
    assert!(m.take_actions().is_empty());
}

#[test]
fn webos_4_tries_its_bindings_in_order_and_remembers_the_one_that_works() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Legacy, true);
    assert_eq!(m.plane(), Plane::Legacy(Candidate::AppIdOnly));
    assert!(!m.wants_window());
    let serial = loaded(&mut m, movie(), t0);
    let first = m.take_actions();
    assert!(first.iter().all(|a| !matches!(a, Action::AcbBind { .. })), "the app id alone binds nothing more");
    msg(&mut m, serial, json!({ "playing": {} }), t0);
    assert!(m.take_actions().contains(&Action::WakeAfter(PLANE_CHECK_DELAY)));
    m.tick(t0 + Duration::from_secs(1));
    assert!(m.take_actions().is_empty());
    m.tick(t0 + PLANE_CHECK_DELAY);
    assert_eq!(m.take_actions(), [Action::CheckPlane { serial, media_id: "_m1".into() }]);

    // Not on screen: the app id alone fails; ACB binds the loaded pipeline.
    m.input(Input::PlaneShown { serial, shown: Some(false) }, t0 + PLANE_CHECK_DELAY);
    let actions = m.take_actions();
    let failed = PlaneMemory { firmware: "4.10.2-31".into(), winner: None, failed: vec![Candidate::AppIdOnly] };
    assert_eq!(actions[0], Action::Remember(failed));
    assert!(actions.contains(&Action::AcbBind {
        serial,
        media_id: "_m1".into(),
        dest: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        playing: true
    }));
    // ACB refuses: uMediaServer's own display window is next.
    m.input(Input::AcbBound { serial, ok: false }, t0 + PLANE_CHECK_DELAY);
    let actions = m.take_actions();
    assert!(matches!(&actions[0], Action::Remember(mem) if mem.failed == [Candidate::AppIdOnly, Candidate::Acb]));
    assert_eq!(
        calls(&actions),
        [(
            Method::SetDisplayWindow,
            json!({ "mediaId": "_m1", "destination": { "x": 0, "y": 0, "width": 1920, "height": 1080 } })
        )]
    );
    m.tick(t0 + PLANE_CHECK_DELAY * 2);
    assert_eq!(m.take_actions(), [Action::CheckPlane { serial, media_id: "_m1".into() }]);
    m.input(Input::PlaneShown { serial, shown: Some(true) }, t0 + PLANE_CHECK_DELAY * 2);
    let winner = PlaneMemory {
        firmware: "4.10.2-31".into(),
        winner: Some(Candidate::DisplayWindow),
        failed: vec![Candidate::AppIdOnly, Candidate::Acb],
    };
    assert_eq!(m.take_actions(), [Action::Remember(winner.clone())]);
    assert_eq!(m.stats().extra[0], ("Picture".into(), "display window".into()));

    // The next playback on this firmware binds that way at once and asks no more.
    let mut next = Machine::new(
        Config { app_id: "app.pley.tv".into(), plane: PlaneKind::Legacy, acb_available: true, table: None, scale: 2.0 },
        PlaneMemory::for_firmware(Some(&winner.to_json()), "4.10.2-31"),
    );
    assert_eq!(next.plane(), Plane::Legacy(Candidate::DisplayWindow));
    let serial = loaded(&mut next, movie(), t0);
    assert!(calls(&next.take_actions()).iter().any(|(method, _)| *method == Method::SetDisplayWindow));
    msg(&mut next, serial, json!({ "playing": {} }), t0);
    next.tick(t0 + PLANE_CHECK_DELAY);
    assert!(!next.take_actions().iter().any(|a| matches!(a, Action::CheckPlane { .. })));
}

#[test]
fn acb_follows_the_pipelines_play_state() {
    let t0 = Instant::now();
    let memory = PlaneMemory { firmware: "x".into(), winner: Some(Candidate::Acb), failed: Vec::new() };
    let mut m = Machine::new(
        Config { app_id: "app.pley.tv".into(), plane: PlaneKind::Legacy, acb_available: true, table: None, scale: 2.0 },
        memory,
    );
    let serial = loaded(&mut m, movie(), t0);
    assert!(m.take_actions().iter().any(|a| matches!(a, Action::AcbBind { .. })));
    m.input(Input::AcbBound { serial, ok: true }, t0);
    m.pause(t0);
    assert!(m.take_actions().contains(&Action::AcbState(AcbState::Paused)));
    m.play();
    assert!(m.take_actions().contains(&Action::AcbState(AcbState::Playing)));
    m.set_video_rect(None);
    assert_eq!(m.take_actions(), [Action::AcbPlace { dest: Rect::new(0.0, 0.0, 1920.0, 1080.0), full: true }]);
    m.unload();
    assert!(m.take_actions().contains(&Action::AcbState(AcbState::Unloaded)));
}

#[test]
fn a_live_channel_has_no_duration_and_no_start() {
    let t0 = Instant::now();
    let mut m = machine(PlaneKind::Exported, false);
    let mut req = LoadRequest::new("http://h/api/live/1/index.m3u8", MediaKind::LiveHls);
    req.start_ms = 3_000;
    req.duration_hint_ms = Some(10_000);
    let serial = loaded(&mut m, req, t0);
    assert_eq!(m.take_events(), [EngineEvent::Loaded { duration_ms: None }]);
    msg(&mut m, serial, json!({ "sourceInfo": { "programInfo": [{ "duration": 30_000 }] } }), t0);
    assert!(m.take_events().is_empty());
}

#[test]
fn messages_name_their_event_by_their_key() {
    assert_eq!(event_of(&json!({ "currentTime": { "currentTime": 5 }, "mediaId": "_m" })).unwrap().0, "currentTime");
    assert_eq!(event_of(&json!({ "returnValue": true, "subscribed": true })), None);
    assert_eq!(range_of("HDR10"), DynamicRange::Hdr10);
    assert_eq!(range_of("DolbyVision"), DynamicRange::DolbyVision);
    assert_eq!(range_of("HLG"), DynamicRange::Hlg);
    assert_eq!(range_of("none"), DynamicRange::Sdr);
    assert_eq!(range_of("HDR10+"), DynamicRange::Hdr10Plus);
    assert_eq!(range_of("odd"), DynamicRange::Unknown);
    assert_eq!(
        fit(Rect::new(0.0, 0.0, 1920.0, 1080.0), 2.39),
        Rect::new(0.0, (1080.0 - 1920.0 / 2.39) / 2.0, 1920.0, 1920.0 / 2.39)
    );
}
