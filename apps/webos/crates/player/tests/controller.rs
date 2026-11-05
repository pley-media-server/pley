//! packages/tv/test/player/negotiation.test.ts (and prepare.test.ts's
//! warming), ported: the play request, the Dolby Vision rules, and the
//! controller against a pretend server on `SimEngine`.
//!
//! Where the TypeScript drives Samsung's player (its network figures), the
//! tests feed what LG's pipeline gives instead: waits and the buffer's growth.

mod common;

use std::sync::Arc;

use common::*;
use pley_core::media::MediaKind;
use pley_core::model::{
    AudioChoice, ClientCaps, Control, DeviceKind, DynamicRange, ItemKind, ItemSummary, PlaybackMethod, PlaybackMode,
    PlaybackRequest, ServerEvent, SubtitleResult, SubtitleTrack,
};
use pley_player::autoquality::Remembered;
use pley_player::controller::{Phase, SubtitleSearch, TrackChoice};
use pley_player::plan::{
    dv_container_mode, dv_retry_mode, initial_audio, initial_subtitle, playback_request, playing_audio, with_tracks,
};
use pley_player::prepare::warm_stream;
use pley_player::speed::SPEEDS;

// ---- the request

/// A remux's tracks: TrueHD Atmos first and default, its E-AC-3, a commentary.
fn remux() -> pley_core::model::MediaFile {
    let mut f = file();
    f.streams = vec![
        f.streams[0].clone(),
        stream(1, "audio", "truehd", Some("eng"), true, |s| s.channels = Some(8)),
        stream(2, "audio", "ac3", Some("eng"), false, |s| {
            s.channels = Some(2);
            s.title = Some("Director's Commentary".into());
        }),
        stream(3, "audio", "eac3", Some("eng"), false, |s| s.channels = Some(6)),
        stream(4, "audio", "eac3", Some("fra"), false, |s| s.channels = Some(6)),
    ];
    f
}

#[test]
fn the_audio_to_start_with_is_the_track_asked_for_else_none() {
    assert_eq!(initial_audio(Some(&remux()), Some(2)).map(|s| s.index), Some(2));
    assert_eq!(initial_audio(Some(&remux()), None), None);
    assert_eq!(initial_audio(Some(&remux()), Some(9)), None);
}

#[test]
fn the_audio_is_named_in_the_request_only_when_there_is_one() {
    let f = remux();
    let with =
        playback_request(70, &choices(|c| c.audio = initial_audio(Some(&f), Some(3))), "webos-abc", Some(&lg()), true);
    assert_eq!(with.audio_index, Some(3));
    let without =
        playback_request(70, &choices(|c| c.audio = initial_audio(Some(&f), None)), "webos-abc", Some(&lg()), true);
    assert_eq!(without.audio_index, None);
}

#[test]
fn the_audio_once_the_session_starts_is_the_one_the_server_says_plays() {
    assert_eq!(playing_audio(Some(&remux()), &info(|i| i.audio_index = Some(3))).map(|s| s.index), Some(3));
    assert_eq!(playing_audio(Some(&remux()), &info(|i| i.audio_index = None)).map(|s| s.index), Some(1));
}

#[test]
fn tracks_picked_on_the_titles_page_are_the_ones_a_playback_begins_with() {
    let picked = with_tracks(&item(file()), 7, Some(Some(2)), Some(4));
    let f = &picked.files[0];
    // The subtitles from the details kept; the audio is the server's to choose, the pick saved with it.
    assert_eq!(initial_audio(Some(f), None), None);
    assert_eq!(initial_subtitle(Some(f), None).map(|s| s.index), Some(4));
    assert_eq!(initial_subtitle(Some(&with_tracks(&picked, 7, Some(Some(2)), None).files[0]), None), None);
    assert_eq!(with_tracks(&item(file()), 8, Some(Some(2)), Some(4)).files[0], file());
}

#[test]
fn tracks_picked_say_whether_the_audio_is_a_pick() {
    let picked = &with_tracks(&item(file()), 7, Some(Some(2)), Some(4)).files[0];
    assert_eq!((picked.selected_audio, picked.audio_picked), (Some(2), Some(true)));
    let automatic = &with_tracks(&item(file()), 7, Some(None), Some(4)).files[0];
    assert_eq!((automatic.selected_audio, automatic.audio_picked), (None, Some(false)));
    let mut kept = file();
    kept.selected_audio = Some(2);
    kept.audio_picked = Some(true);
    let alone = &with_tracks(&item(kept), 7, None, None).files[0];
    assert_eq!((alone.selected_audio, alone.audio_picked, alone.selected_subtitle), (Some(2), Some(true), None));
}

#[test]
fn the_play_request_asks_for_keyed_addresses_says_it_plays_on_a_tv_and_sends_the_caps() {
    let req = playback_request(70, &choices(|_| ()), "webos-abc", Some(&lg()), true);
    let want = PlaybackRequest {
        item_id: 70,
        file_id: Some(7),
        start_ms: Some(600_000),
        mode: PlaybackMode::Auto,
        client_id: Some("webos-abc".into()),
        caps: lg(),
        keyed_urls: true,
        device_kind: Some(DeviceKind::Tv),
        ..PlaybackRequest::default()
    };
    assert_eq!(req, want);
}

#[test]
fn the_play_request_names_the_audio_picked_even_in_a_direct_play() {
    let f = file();
    let req = playback_request(70, &choices(|c| c.audio = Some(f.streams[2].clone())), "c", Some(&lg()), true);
    assert_eq!(req.audio_index, Some(2));
}

#[test]
fn picture_subtitles_are_named_only_to_burn_them_in() {
    let f = file();
    let pgs = f.streams[3].clone();
    let req = |sub: &pley_core::model::MediaStream, sideload: bool| {
        playback_request(
            70,
            &choices(|c| {
                c.subtitle = Some(sub.clone());
                c.streaming = true;
                c.sideload = sideload;
            }),
            "c",
            Some(&lg()),
            true,
        )
        .subtitle_index
    };
    assert_eq!(req(&pgs, true), None);
    assert_eq!(req(&pgs, false), Some(3));
    assert_eq!(req(&f.streams[4], false), None);
}

#[test]
fn dolby_vision_in_matroska_is_remuxed_where_the_tv_shows_it_only_from_mp4_and_ts() {
    let mp4_ts = ["mp4".to_string(), "ts".to_string()];
    let all = ["mp4".to_string(), "ts".to_string(), "mkv".to_string()];
    assert_eq!(dv_container_mode(Some(&dv_file(8, "mkv")), Some(&lg()), Some(&mp4_ts)), Some(PlaybackMode::Remux));
    assert_eq!(dv_container_mode(Some(&dv_file(8, "mp4")), Some(&lg()), Some(&mp4_ts)), None);
    assert_eq!(dv_container_mode(Some(&dv_file(8, "mkv")), Some(&lg()), Some(&all)), None);
    assert_eq!(dv_container_mode(Some(&dv_file(8, "mkv")), Some(&lg()), None), None);
}

#[test]
fn files_are_left_alone_on_a_tv_without_dolby_vision_and_hdr10_files_everywhere() {
    let samsung = ClientCaps {
        hdr_types: vec![DynamicRange::Hdr10, DynamicRange::Hdr10Plus, DynamicRange::Hlg],
        dolby_vision: false,
        dolby_vision_profiles: Vec::new(),
        ..lg()
    };
    let mp4 = ["mp4".to_string()];
    assert_eq!(dv_container_mode(Some(&dv_file(8, "mkv")), Some(&samsung), Some(&mp4)), None);
    assert_eq!(dv_container_mode(Some(&file()), Some(&lg()), Some(&mp4)), None);
}

#[test]
fn the_android_guards_profile_7_played_as_it_is_becomes_a_remux() {
    assert_eq!(
        dv_retry_mode(&info(|i| i.method = PlaybackMethod::Direct), Some(&dv_file(7, "mkv")), Some(&lg())),
        Some(PlaybackMode::Remux)
    );
    let remuxed = info(|i| {
        i.method = PlaybackMethod::Remux;
        i.hls = true;
    });
    assert_eq!(dv_retry_mode(&remuxed, Some(&dv_file(5, "mkv")), Some(&lg())), None);
    let mut no_master = lg();
    no_master.hls.master_playlist = false;
    assert_eq!(dv_retry_mode(&remuxed, Some(&dv_file(5, "mkv")), Some(&no_master)), Some(PlaybackMode::Transcode));
}

// ---- a playback session

#[test]
fn starts_at_the_resume_point_from_the_keyed_address_with_the_containers_type() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    let req = &w.fake.requests()[0];
    assert_eq!((req.item_id, req.file_id, req.start_ms, req.keyed_urls), (70, Some(7), Some(600_000), true));
    assert_eq!(req.client_id.as_deref(), Some("webos-abc"));
    let load = &w.sim.loads()[0];
    assert_eq!(load.url, "http://nas:7539/api/media/pley_mk_KEY/file.mkv");
    assert_eq!(
        (load.kind, load.container.as_deref(), load.start_ms),
        (MediaKind::File, Some("video/x-matroska"), 600_000)
    );
}

#[test]
fn asks_for_a_remux_of_dolby_vision_in_matroska_on_a_tv_that_shows_it_only_from_mp4_and_ts() {
    let fake = Fake::new(item(dv_file(8, "mkv")), |req| {
        info(|i| {
            let remux = req.mode == PlaybackMode::Remux;
            i.method = if remux { PlaybackMethod::Remux } else { PlaybackMethod::Direct };
            i.hls = remux;
            i.url = "/api/media/pley_mk_KEY/master.m3u8".into();
        })
    });
    let mut w = World::with_dv(fake, &["mp4", "ts"], |_| ());
    w.start();
    assert_eq!(w.fake.modes(), [PlaybackMode::Remux]);
    assert_eq!(w.sim.loads()[0].kind, MediaKind::Hls);
}

#[test]
fn plays_the_audio_the_server_chose_for_this_tv_and_its_menu_says_so() {
    // The server's pick: the TrueHD, second of the file's audio tracks (a receiver that takes it).
    let mut w = World::new(Fake::new(item(file()), |_| info(|i| i.audio_index = Some(2))), |_| ());
    w.start();
    w.open();
    assert_eq!(w.fake.requests()[0].audio_index, None);
    assert_eq!(w.sim.switched(), [1]);
    assert_eq!(selected(&w.c.state().audio), [Some(2)]);
}

#[test]
fn lets_the_server_choose_the_audio_again_for_a_lower_quality() {
    // The TrueHD as it is; converted at a lower quality, the server copies the first track instead.
    let fake =
        Fake::new(item(file()), |req| info(|i| i.audio_index = Some(if req.max_bitrate.is_some() { 1 } else { 2 })));
    let mut w = World::new(fake, |_| ());
    w.start();
    w.c.select_quality("4m");
    w.settle();
    let requests = w.fake.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].max_bitrate.is_some());
    assert_eq!(requests[1].audio_index, None);
    assert_eq!(selected(&w.c.state().audio), [Some(1)]);
}

#[test]
fn saves_subtitles_picked_without_the_audio_the_server_chose_and_the_audio_once_picked() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.c.select_subtitle(Some(4));
    w.c.select_audio(2);
    w.c.select_subtitle(None);
    w.settle();
    assert_eq!(
        w.fake.saved(),
        [(AudioChoice::Keep, Some(4)), (AudioChoice::Track(2), Some(4)), (AudioChoice::Track(2), None)]
    );
    // And the request names it from then on.
    assert_eq!(w.fake.requests().last().unwrap().audio_index, Some(2));
}

#[test]
fn refuses_a_server_too_old_for_keyed_addresses_ending_its_session_unplayed() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|i| i.url = "/api/files/7/stream".into())), |_| ());
    w.start();
    assert!(matches!(w.c.state().phase, Phase::Error { can_retry: false, .. }));
    assert!(w.sim.loads().is_empty());
    assert!(w.fake.calls().contains(&"delete s1".to_string()));
    assert!(w.fake.calls().iter().all(|c| !c.starts_with("progress")));
}

#[test]
fn reports_the_end_then_ends_the_session_when_the_player_closes() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.c.close();
    w.settle();
    assert_eq!(w.fake.calls(), ["start auto", "progress stopped 600000", "delete s1"]);
    assert_eq!(w.sim.commands().last().map(String::as_str), Some("unload"));
}

#[test]
fn stops_for_good_when_the_server_stops_the_session_keeping_the_resume_point() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.c.server_event(&ServerEvent::SessionStopped {
        session_id: "s1".into(),
        message: Some("Bedtime".into()),
        by_admin: true,
    });
    w.settle();
    let Phase::Error { message, can_retry, .. } = &w.c.state().phase else { panic!("not an error") };
    assert!(!can_retry);
    assert!(message.contains("Bedtime"));
    assert!(w.c.state().stopped_by_server);
    assert!(w.fake.calls().iter().all(|c| !c.starts_with("delete")));
}

// ---- the user's remote, on another device

fn command(session: &str, command: Control, position_ms: Option<i64>, offset_ms: Option<i64>) -> ServerEvent {
    ServerEvent::PlayerCommand {
        session_id: session.into(),
        command,
        position_ms,
        offset_ms,
        stream_index: None,
        quality: None,
    }
}

/// The progress reports said since the last look.
fn reports_since(w: &World, from: usize) -> Vec<String> {
    w.fake.calls().into_iter().filter(|c| c.starts_with("progress")).skip(from).collect()
}

fn reports_so_far(w: &World) -> usize {
    reports_since(w, 0).len()
}

#[test]
fn a_video_says_what_the_remote_may_do_and_obeys_it_on_its_own_session_reporting_at_once() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    // A film alone has nothing next: no next for the remote (its tracks follow the rest).
    let req = w.fake.requests()[0].clone();
    assert_eq!(req.controls[..4], [Control::Play, Control::Pause, Control::Seek, Control::Skip]);
    assert!(!req.controls.contains(&Control::Next));
    assert_eq!(req.device_kind, Some(DeviceKind::Tv));

    // Someone else's session: nothing.
    let before = reports_so_far(&w);
    w.c.server_event(&command("s2", Control::Pause, None, None));
    w.settle();
    assert_eq!(w.c.state().phase, Phase::Playing);
    assert_eq!(reports_so_far(&w), before);

    w.c.server_event(&command("s1", Control::Pause, None, None));
    w.settle();
    assert_eq!(w.c.state().phase, Phase::Paused);
    assert_eq!(reports_since(&w, before), ["progress paused 600000"]);
    // Paused already: said again at once, so a remote a little behind settles.
    w.c.server_event(&command("s1", Control::Pause, None, None));
    w.settle();
    assert_eq!(reports_since(&w, before), ["progress paused 600000", "progress paused 600000"]);
    w.c.server_event(&command("s1", Control::Play, None, None));
    w.settle();
    assert_eq!(w.c.state().phase, Phase::Playing);
    assert_eq!(reports_since(&w, before + 2), ["progress playing 600000"]);

    // A seek reports at once where it went, not a second later; past the end it stops short of it.
    w.c.server_event(&command("s1", Control::Seek, Some(1_200_000), None));
    w.settle();
    assert_eq!(reports_since(&w, before + 3), ["progress playing 1200000"]);
    assert_eq!(w.sim.position(), 1_200_000);
    w.advance(1_500);
    assert_eq!(reports_since(&w, before + 3).len(), 1, "the seek reported twice");
    let at = w.sim.position() as i64;
    w.c.server_event(&command("s1", Control::Skip, None, Some(-10_000)));
    w.settle();
    assert_eq!(w.sim.position() as i64, at - 10_000);
    w.c.server_event(&command("s1", Control::Seek, Some(9_000_000), None));
    w.settle();
    assert_eq!(w.sim.position(), 7_200_000 - 250);

    // Nothing next, and no previous on a video: nothing, and nothing said on the TV.
    let before = reports_so_far(&w);
    w.c.server_event(&command("s1", Control::Next, None, None));
    w.c.server_event(&command("s1", Control::Previous, None, None));
    w.settle();
    assert_eq!(reports_so_far(&w), before);
    assert!(w.fake.questions().iter().all(|q| q != "next"));
    assert!(w.notices.is_empty(), "{:?}", w.notices);
    assert_eq!(w.fake.requests().len(), 1);
}

#[test]
fn an_episodes_next_from_the_remote_plays_the_next_and_its_last_says_nothing_on_the_tv() {
    let mut detail = item(file());
    detail.summary.kind = ItemKind::Episode;
    let fake = Fake::new(detail, |req| info(|i| i.session_id = format!("s{}", req.item_id)));
    let mut w = World::new(fake, |_| ());
    w.start();
    w.open();
    assert_eq!(w.fake.requests()[0].controls.get(4), Some(&Control::Next));
    // The last episode: the remote's next finds nothing, and says nothing here.
    w.c.server_event(&command("s70", Control::Next, None, None));
    w.settle();
    assert!(w.fake.questions().iter().any(|q| q == "next"));
    assert!(w.notices.is_empty(), "{:?}", w.notices);
    assert_eq!(w.fake.requests().len(), 1);
    // With one after it, it plays that.
    w.fake.set_next(Some(ItemSummary { id: 71, kind: ItemKind::Episode, ..ItemSummary::default() }));
    w.c.server_event(&command("s70", Control::Next, None, None));
    w.settle();
    assert_eq!(w.fake.requests().last().map(|r| r.item_id), Some(71));
}

fn pick(session: &str, command: Control, stream_index: i64) -> ServerEvent {
    ServerEvent::PlayerCommand {
        session_id: session.into(),
        command,
        position_ms: None,
        offset_ms: None,
        stream_index: Some(stream_index),
        quality: None,
    }
}

/// The tracks the reports said, from the `from`-th on: `(state, audio, subtitles)`.
fn tracks_reported(w: &World, from: usize) -> Vec<(String, Option<i64>, Option<i64>)> {
    let reports = w.fake.reports();
    reports.iter().skip(from).map(|r| (r.state.as_str().to_string(), r.audio_index, r.subtitle_index)).collect()
}

#[test]
fn a_video_says_the_remote_may_pick_its_tracks_and_every_report_says_which_play() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    // Two audio tracks and two subtitle tracks: the remote picks among them, and the quality.
    let req = w.fake.requests()[0].clone();
    assert_eq!(
        req.controls,
        [
            Control::Play,
            Control::Pause,
            Control::Seek,
            Control::Skip,
            Control::Audio,
            Control::Subtitles,
            Control::Quality
        ]
    );
    // The audio the server chose, no subtitles: in the reports every 10 s, on pause, and at the end.
    w.advance(10_000);
    w.c.pause();
    w.settle();
    w.c.close();
    w.settle();
    let said = tracks_reported(&w, 0);
    assert_eq!(said.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["playing", "paused", "stopped"]);
    assert!(said.iter().all(|r| (r.1, r.2) == (Some(1), Some(-1))), "{said:?}");

    // A file without subtitles has none for the remote to pick.
    let mut bare = file();
    bare.streams.retain(|s| s.kind != pley_core::model::StreamKind::Subtitle);
    let mut w = World::new(Fake::new(item(bare), |_| info(|_| ())), |_| ());
    w.start();
    let controls = &w.fake.requests()[0].controls;
    assert_eq!(controls[controls.len() - 2..], [Control::Audio, Control::Quality]);
}

fn quality(session: &str, id: &str) -> ServerEvent {
    ServerEvent::PlayerCommand {
        session_id: session.into(),
        command: Control::Quality,
        position_ms: None,
        offset_ms: None,
        stream_index: None,
        quality: Some(id.into()),
    }
}

/// The quality the reports said, from the `from`-th on: `(state, quality)`.
fn qualities_reported(w: &World, from: usize) -> Vec<(String, Option<String>)> {
    w.fake.reports().iter().skip(from).map(|r| (r.state.as_str().to_string(), r.quality.clone())).collect()
}

#[test]
fn the_remotes_quality_plays_as_the_menus_does_and_every_report_says_which_is_picked() {
    let answer = |req: &PlaybackRequest| info(|i| i.session_id = format!("s{}", req.max_bitrate.map_or(1, |_| 2)));
    let mut w = World::new(Fake::new(item(file()), answer), |_| ());
    w.start();
    w.open();
    let from = w.fake.reports().len();
    let said = |state: &str, id: &str| (state.to_string(), Some(id.to_string()));
    // The one picked already: said again at once. One the menu doesn't offer, or another
    // session's: nothing.
    w.c.server_event(&quality("s1", "original"));
    w.c.server_event(&quality("s1", "best"));
    w.c.server_event(&quality("s2", "4m"));
    w.settle();
    assert_eq!(qualities_reported(&w, from), [said("playing", "original")]);
    assert_eq!(w.fake.requests().len(), 1);
    // Another: as picked in the menu, a new session made ready beside this one, which says the
    // pick as it begins.
    w.c.server_event(&quality("s1", "4m"));
    w.settle();
    let asked = w.fake.requests().last().cloned().unwrap();
    assert_eq!((asked.max_bitrate.is_some(), asked.keep_previous), (true, true));
    assert_eq!(
        qualities_reported(&w, from),
        [said("playing", "original"), said("stopped", "4m"), said("playing", "4m")]
    );
    assert_eq!(w.c.state().quality, "4 Mbps 720p");
}

#[test]
fn the_remotes_track_plays_as_the_menus_does_and_is_reported_at_once() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    let from = w.fake.reports().len();

    // Text subtitles, then picture ones drawn here from the server's .mks, then off: each said at once.
    w.c.server_event(&pick("s1", Control::Subtitles, 4));
    w.settle();
    assert!(w.fake.subtitle_asks().iter().any(|a| a.contains("/subtitles/4.vtt")), "{:?}", w.fake.subtitle_asks());
    w.c.server_event(&pick("s1", Control::Subtitles, 3));
    w.settle();
    assert!(w.fake.subtitle_asks().iter().any(|a| a.contains("/subtitles/3.mks")), "{:?}", w.fake.subtitle_asks());
    assert_eq!(selected(&w.c.state().subtitles), [Some(3)], "the menu shows the remote's pick");
    w.c.server_event(&pick("s1", Control::Subtitles, -1));
    w.settle();
    assert_eq!(selected(&w.c.state().subtitles), [None]);
    // What plays already changes nothing, and says so again, so the remote settles.
    w.c.server_event(&pick("s1", Control::Subtitles, -1));
    w.c.server_event(&pick("s1", Control::Audio, 1));
    w.settle();
    let playing = |audio, subtitles| ("playing".to_string(), Some(audio), Some(subtitles));
    assert_eq!(
        tracks_reported(&w, from),
        [playing(1, 4), playing(1, 3), playing(1, -1), playing(1, -1), playing(1, -1)]
    );
    // Saved as the menu's picks are, for the next episode or viewing.
    assert_eq!(w.fake.saved(), [(AudioChoice::Keep, Some(4)), (AudioChoice::Keep, Some(3)), (AudioChoice::Keep, None)]);
    // Another session's pick, or a track the file doesn't have: nothing changes.
    w.c.server_event(&pick("s2", Control::Subtitles, 4));
    w.c.server_event(&pick("s1", Control::Audio, 9));
    w.settle();
    assert_eq!((selected(&w.c.state().audio), selected(&w.c.state().subtitles)), (vec![Some(1)], vec![None]));
    assert_eq!(w.fake.requests().len(), 1, "a pick took a new session");
}

#[test]
fn an_audio_track_the_tv_cant_switch_to_in_place_plays_in_a_new_session_that_says_so() {
    // The session's id names its audio: the first the server's choice, then the pick.
    let answer = |req: &PlaybackRequest| {
        info(|i| {
            i.session_id = format!("s{}", req.audio_index.unwrap_or(1));
            i.audio_index = req.audio_index.or(Some(1));
        })
    };
    let mut w = World::new(Fake::new(item(file()), answer), |_| ());
    w.start();
    w.open();
    let from = w.fake.reports().len();
    // An E-AC-3 track switches in place: said at once.
    w.c.server_event(&pick("s1", Control::Audio, 1));
    w.settle();
    assert_eq!(tracks_reported(&w, from), [("playing".to_string(), Some(1), Some(-1))]);
    // TrueHD isn't decoded here: a new session, made ready beside this one, names it; this one's
    // last report still says the track it played.
    w.c.server_event(&pick("s1", Control::Audio, 2));
    w.settle();
    let asked = w.fake.requests().last().cloned().unwrap();
    assert_eq!((asked.audio_index, asked.keep_previous), (Some(2), true));
    assert_eq!(tracks_reported(&w, from + 1), [("stopped".to_string(), Some(1), Some(-1))]);
    assert_eq!(selected(&w.c.state().audio), [Some(2)]);
    w.open();
    w.c.pause();
    w.settle();
    assert_eq!(tracks_reported(&w, from + 2), [("paused".to_string(), Some(2), Some(-1))]);
    assert_eq!(w.c.session().map(|s| s.session_id.as_str()), Some("s2"));
}

/// What the screen is told the remote did with `event`, if anything.
fn said(w: &mut World, event: ServerEvent) -> Option<String> {
    let before = w.c.state().remote_notice_id;
    w.c.server_event(&event);
    w.settle();
    let s = w.c.state();
    (s.remote_notice_id != before).then(|| s.remote_notice.clone().unwrap_or_default())
}

#[test]
fn a_video_says_what_the_remote_changed_for_the_screen_to_show_and_nothing_for_the_rest() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    let said_as = |w: &mut World, event| said(w, event).unwrap_or_else(|| panic!("nothing was said"));
    // Another session's, a play while playing, and nothing next: nothing to say.
    assert_eq!(said(&mut w, command("s2", Control::Pause, None, None)), None);
    assert_eq!(said(&mut w, command("s1", Control::Play, None, None)), None);
    assert_eq!(said(&mut w, command("s1", Control::Next, None, None)), None);

    // A pause, the remote's until it plays again; a pause while paused says nothing.
    assert_eq!(said_as(&mut w, command("s1", Control::Pause, None, None)), "Paused");
    assert!(w.c.state().paused_by_remote);
    assert_eq!(said(&mut w, command("s1", Control::Pause, None, None)), None);
    // A skip says how far it was asked to go, a seek where to, in the player's times.
    assert_eq!(said_as(&mut w, command("s1", Control::Skip, None, Some(30_000))), "+30 s");
    assert_eq!(said_as(&mut w, command("s1", Control::Skip, None, Some(-10_000))), "−10 s");
    assert_eq!(said_as(&mut w, command("s1", Control::Seek, Some(754_000), None)), "Jumped to 12:34");
    assert_eq!(said_as(&mut w, command("s1", Control::Seek, Some(3_723_000), None)), "Jumped to 1:02:03");
    assert!(w.c.state().paused_by_remote, "a seek ended the remote's pause");
    assert_eq!(said_as(&mut w, command("s1", Control::Play, None, None)), "Playing");
    assert!(!w.c.state().paused_by_remote);
    // Played here, a pause of the remote's is over too.
    said_as(&mut w, command("s1", Control::Pause, None, None));
    w.c.play();
    w.settle();
    assert!(!w.c.state().paused_by_remote);

    // Tracks and qualities named as their menus name them; what is picked already says nothing.
    let label = |tracks: &[TrackChoice], i: i64| tracks.iter().find(|t| t.index == Some(i)).unwrap().label.clone();
    let english = label(&w.c.state().subtitles, 4);
    assert_eq!(said_as(&mut w, pick("s1", Control::Subtitles, 4)), format!("Subtitles: {english}"));
    assert_eq!(said(&mut w, pick("s1", Control::Subtitles, 4)), None);
    assert_eq!(said_as(&mut w, pick("s1", Control::Subtitles, -1)), "Subtitles off");
    assert_eq!(said(&mut w, pick("s1", Control::Subtitles, -1)), None);
    let truehd = label(&w.c.state().audio, 2);
    assert_eq!(said(&mut w, pick("s1", Control::Audio, 1)), None);
    assert_eq!(said_as(&mut w, pick("s1", Control::Audio, 2)), format!("Audio: {truehd}"));
    assert_eq!(said(&mut w, quality("s1", "original")), None);
    assert_eq!(said(&mut w, quality("s1", "best")), None);
    let menu = w.c.state().qualities.iter().find(|q| q.id == "4m").map(|q| q.label.clone()).unwrap();
    assert_eq!(said_as(&mut w, quality("s1", "4m")), format!("Quality: {menu}"));
    assert_eq!(said(&mut w, quality("s1", "4m")), None);
}

#[test]
fn a_video_stopped_or_in_trouble_obeys_nothing() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    w.c.server_event(&ServerEvent::SessionStopped { session_id: "s1".into(), message: None, by_admin: false });
    w.settle();
    let before = w.fake.calls().len();
    w.c.server_event(&command("s1", Control::Play, None, None));
    w.c.server_event(&command("s1", Control::Seek, Some(0), None));
    w.settle();
    assert_eq!(w.fake.calls().len(), before);
    assert!(matches!(w.c.state().phase, Phase::Error { .. }));
}

#[test]
fn tries_again_with_a_conversion_when_the_tv_cant_decode_the_file() {
    let fake = Fake::new(item(file()), |req| {
        info(|i| {
            if req.mode != PlaybackMode::Auto {
                i.session_id = "s2".into();
                i.method =
                    if req.mode == PlaybackMode::Remux { PlaybackMethod::Remux } else { PlaybackMethod::Transcode };
                i.hls = true;
                i.url = "/api/media/pley_mk_KEY2/master.m3u8".into();
            }
        })
    });
    let mut w = World::new(fake, |_| ());
    w.start();
    w.sim.fail(3, "decoder");
    w.advance(100);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto, PlaybackMode::Transcode]);
    let load = &w.sim.loads()[1];
    assert_eq!((load.kind, load.url.as_str()), (MediaKind::Hls, "http://nas:7539/api/media/pley_mk_KEY2/master.m3u8"));
}

// ---- the server out of reach

/// A playback that has shown its picture, standing still.
fn playing() -> World {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    w
}

#[test]
fn keeps_the_place_and_waits_for_the_server_then_starts_again_where_it_was() {
    let mut w = playing();
    w.fake.set_down(true);
    w.sim.wait(true);
    w.sim.fail(2, "connection failed");
    w.advance(10);
    assert!(w.c.state().reconnecting);
    assert_eq!(w.c.state().phase, Phase::Buffering);
    // Away 30 s: no session spent, no conversion, no error.
    w.advance(30_000);
    assert_eq!(w.fake.requests().len(), 1);
    assert_eq!(w.c.state().phase, Phase::Buffering);
    w.fake.set_down(false);
    w.advance(2_100);
    assert!(!w.c.state().reconnecting);
    let starts: Vec<(PlaybackMode, Option<i64>)> = w.fake.requests().iter().map(|r| (r.mode, r.start_ms)).collect();
    assert_eq!(starts, [(PlaybackMode::Auto, Some(600_000)), (PlaybackMode::Auto, Some(600_000))]);
}

#[test]
fn says_so_after_90_s_and_try_again_starts_over() {
    let mut w = playing();
    w.fake.set_down(true);
    w.sim.fail(2, "connection failed");
    w.advance(89_000);
    assert_eq!(w.c.state().phase, Phase::Buffering);
    w.advance(3_000);
    let Phase::Error { title, can_retry, .. } = &w.c.state().phase else { panic!("not an error") };
    assert_eq!((title.as_deref(), *can_retry), (Some("Can't reach your server"), true));
    assert!(!w.c.state().reconnecting);
    w.fake.set_down(false);
    w.c.retry();
    w.advance(2_500);
    assert_eq!(w.fake.requests().len(), 2);
    assert!(!matches!(w.c.state().phase, Phase::Error { .. }));
}

#[test]
fn waits_out_a_transcoder_restart_mid_film_then_plays_on() {
    let mut w = playing();
    w.fake.set_restarting(true);
    w.sim.fail(2, "connection failed");
    w.advance(3_000);
    assert!(w.c.state().reconnecting);
    // The server answers all along; each new session it gives way to fails the same way, and the
    // wait goes on rather than ending in an error.
    w.advance(30_000);
    assert!(w.c.state().reconnecting);
    assert_eq!(w.c.state().phase, Phase::Buffering);
    w.fake.set_restarting(false);
    w.advance(2_500);
    assert!(!w.c.state().reconnecting);
    assert!(!matches!(w.c.state().phase, Phase::Error { .. }));
}

#[test]
fn a_transcoder_away_90_s_in_all_shows_the_server_s_words() {
    let mut w = playing();
    w.fake.set_restarting(true);
    w.sim.fail(2, "connection failed");
    w.advance(89_000);
    assert_eq!(w.c.state().phase, Phase::Buffering);
    w.advance(4_000);
    let Phase::Error { message, title, .. } = &w.c.state().phase else { panic!("not an error") };
    assert_eq!((message.as_str(), title.as_deref()), ("The transcoder isn't running.", None));
    assert!(!w.c.state().reconnecting);
}

#[test]
fn says_so_at_once_when_the_server_is_off_as_play_is_pressed() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.fake.set_down(true);
    w.start();
    let Phase::Error { title, can_retry, .. } = &w.c.state().phase else { panic!("not an error") };
    assert_eq!((title.as_deref(), *can_retry), (Some("Can't reach your server"), true));
}

#[test]
fn waits_for_the_server_rather_than_converting_when_a_stall_finds_it_gone() {
    let mut w = playing();
    w.fake.set_down(true);
    w.sim.wait(true);
    // The TV's own player says nothing: 12 s without moving.
    w.advance(13_000);
    assert!(w.c.state().reconnecting);
    w.fake.set_down(false);
    w.advance(2_100);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto, PlaybackMode::Auto]);
}

// ---- dropped frames

/// A file played as it is, its frames counted: E-AC-3, and an AC-3 track the TV plays too.
fn counting() -> World {
    let mut f = file();
    f.streams[2] = stream(2, "audio", "ac3", Some("eng"), false, |s| s.channels = Some(6));
    let mut w = World::new(Fake::new(item(f), |_| info(|_| ())), |_| ());
    w.start();
    w.open();
    w.frames = Some((0, 0));
    w
}

#[test]
fn dropped_frames_convert_a_video_that_drops_many() {
    let mut w = counting();
    w.drop_frames(4, 0);
    w.drop_frames(14, 130);
    assert_eq!(&w.fake.modes()[..2], [PlaybackMode::Auto, PlaybackMode::Transcode]);
}

#[test]
fn dropped_frames_arent_counted_as_a_track_switch_drops_them() {
    let mut w = counting();
    w.drop_frames(4, 0);
    w.c.select_audio(2);
    assert_eq!(w.sim.switched().last(), Some(&1));
    w.drop_frames(14, 130);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto]);
    // Counted again after: a video that drops them later still converts.
    w.drop_frames(14, 130);
    assert_eq!(&w.fake.modes()[..2], [PlaybackMode::Auto, PlaybackMode::Transcode]);
}

#[test]
fn the_tvs_network_speed_is_read_for_info_alone_not_all_through_playback() {
    let mut w = counting();
    w.drop_frames(10, 0);
    // The frames are sampled every 2 s; LG's network speed reads a file, so they leave it.
    assert_eq!(w.sim.network_asks(), 0);
    // Nor does Playback details' readout, which shows no network speed.
    let _ = w.c.readout_stats();
    assert_eq!(w.sim.network_asks(), 0);
    let _ = w.c.engine_stats();
    assert_eq!(w.sim.network_asks(), 1);
}

// ---- stalls (Info's count)

#[test]
fn stalls_count_a_playing_video_waiting_for_more_not_the_wait_of_its_own_seek() {
    let mut w = playing();
    assert_eq!(w.c.state().phase, Phase::Playing);
    w.sim.wait(true);
    w.advance(10);
    w.sim.wait(false);
    w.advance(10);
    assert_eq!(w.c.state().stalls, 1);
    w.c.seek_to(1_200_000);
    w.sim.wait(true);
    w.advance(10);
    w.sim.wait(false);
    w.advance(10);
    assert_eq!(w.c.state().stalls, 1);
    w.advance(3_000);
    w.sim.wait(true);
    w.advance(10);
    assert_eq!(w.c.state().stalls, 2);
}

// ---- Automatic

/// The original as it is; a conversion asked for, or the original converted at the server's quality for it.
fn converted(req: &PlaybackRequest) -> pley_core::model::PlaybackInfo {
    info(|i| {
        if req.mode != PlaybackMode::Auto {
            i.session_id = format!("s{}", req.max_bitrate.map_or("t".to_string(), |b| b.to_string()));
            i.method = PlaybackMethod::Transcode;
            i.hls = true;
            i.url = "/api/media/pley_mk_KEY2/master.m3u8".into();
            i.auto_quality = if req.max_bitrate.is_some() { None } else { Some("30m".into()) };
        }
    })
}

#[test]
fn automatic_plays_the_original_and_its_menu_says_so_in_the_menus_order() {
    let mut w = automatic(item(file()), Memory::default(), None, converted);
    let asked: Vec<(PlaybackMode, Option<u32>)> = w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate)).collect();
    assert_eq!(asked, [(PlaybackMode::Auto, None)]);
    assert_eq!(w.c.state().quality, "Automatic · Original");
    let ids: Vec<&str> = w.c.state().qualities.iter().map(|q| q.id).collect();
    assert_eq!(ids, ["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"]);
    let chosen: Vec<&str> = w.c.state().qualities.iter().filter(|q| q.selected).map(|q| q.id).collect();
    assert_eq!(chosen, ["auto"]);
    w.c.close();
}

#[test]
fn automatic_keeps_the_dolby_vision_guard_of_the_original() {
    let fake = Fake::new(item(dv_file(8, "mkv")), converted);
    let mut w = World::with_dv(fake, &["mp4", "ts"], |env| env.quality = Arc::new(Picked::new("auto")));
    w.start();
    assert_eq!(w.fake.modes(), [PlaybackMode::Remux]);
}

/// A network too slow for the file, as LG's pipeline shows it: the buffer
/// refilling at 1.25 times the video's pace (50 Mbps at least, under 1.5 × the
/// file's 40), then 40 Mbps arriving at half speed, and a wait of 2.5 s:
/// 40 × 0.5 × 0.75 = 15 → 15 Mbps 4K.
fn slow_network(w: &mut World) {
    w.play(8, 1.25);
    w.play(20, 0.5);
    w.wait(2_500);
    w.advance(2_500);
}

#[test]
fn automatic_steps_down_on_a_2_s_wait_from_where_it_was_and_says_so_once() {
    let memory = Memory::default();
    let mut w = automatic(item(file()), memory.clone(), None, converted);
    slow_network(&mut w);
    let asked: Vec<(PlaybackMode, Option<u32>, Option<u32>)> =
        w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate, r.max_height)).collect();
    assert_eq!(asked, [(PlaybackMode::Auto, None, None), (PlaybackMode::Transcode, Some(15_000), Some(2160))]);
    assert!(w.fake.requests()[1].start_ms.unwrap() > 600_000);
    // Made ready beside the original, which ends once the step's stream is there.
    assert!(w.fake.requests()[1].keep_previous);
    let calls = w.fake.calls();
    let at = |c: &str| calls.iter().position(|x| x == c);
    assert!(at("start transcode").is_some() && at("delete s1") > at("start transcode"), "{calls:?}");
    assert_eq!(w.notices, ["Quality lowered to 15 Mbps 4K"]);
    assert_eq!(w.c.state().quality, "Automatic · 15 Mbps 4K");
    assert_eq!(w.c.state().note.as_deref(), Some("Lowered for your connection"));
    assert_eq!(auto_hint(&w), "Now 15 Mbps 4K · Lowered for your connection");
    assert_eq!(memory.value().map(|m| m.kbps), Some(15_000.0));
}

#[test]
fn automatic_plays_the_original_on_through_waits_on_a_network_that_carries_1_5_times_the_file() {
    let mut w = automatic(item(file()), Memory::default(), None, converted);
    // LG's measure: the buffer refilling at twice the video's pace shows 80 Mbps at least.
    w.play(12, 2.0);
    w.wait(2_500);
    w.ready();
    w.play(4, 0.5);
    w.wait(2_500);
    w.ready();
    w.play(4, 0.5);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto]);
    assert_eq!(w.c.state().quality, "Automatic · Original");
}

#[test]
fn automatic_doesnt_count_the_wait_of_a_seek_nor_waits_while_paused() {
    let mut w = automatic(item(file()), Memory::default(), None, converted);
    w.play(6, 1.2);
    w.c.seek_to(1_200_000);
    w.sim.set_position(1_200_000);
    w.buffered_end = 1_200_000.0;
    w.wait(3_000);
    w.ready();
    w.c.pause();
    w.wait(3_000);
    w.advance(3_000);
    w.wait(3_000);
    w.advance(3_000);
    assert_eq!(w.fake.requests().len(), 1);
}

#[test]
fn automatic_leaves_the_originals_dropped_frames_to_the_fallback_and_lowers_a_conversion_that_drops_them() {
    let mut w = automatic(item(file()), Memory::default(), None, converted);
    w.frames = Some((0, 0));
    w.drop_frames(4, 0);
    w.drop_frames(14, 130);
    w.advance(2_500);
    // As ever: the server's own quality for the file, once.
    let asked = |w: &World| w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate)).collect::<Vec<_>>();
    assert_eq!(asked(&w), [(PlaybackMode::Auto, None), (PlaybackMode::Transcode, None)]);
    w.ready();
    // Within 20 s of that change, nothing more.
    w.frames = Some((0, 0));
    w.drop_frames(14, 130);
    assert_eq!(w.fake.requests().len(), 2);
    // Then the conversion, at 30 Mbps 4K, goes one step down.
    w.drop_frames(4, 0);
    w.drop_frames(14, 130);
    w.advance(2_500);
    assert_eq!(asked(&w)[2..], [(PlaybackMode::Transcode, Some(15_000))]);
    assert_eq!(w.c.state().note.as_deref(), Some("The original didn't play smoothly on this device."));
}

#[test]
fn automatic_starts_the_next_title_at_the_remembered_network_and_picking_it_again_starts_afresh() {
    let memory = Memory::default();
    let mut first = automatic(item(file()), memory.clone(), None, converted);
    slow_network(&mut first);
    first.c.close();

    let mut next = automatic(item(file()), memory.clone(), None, converted);
    let asked = |w: &World| w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate)).collect::<Vec<_>>();
    assert_eq!(asked(&next), [(PlaybackMode::Transcode, Some(15_000))]);
    assert_eq!(next.c.state().quality, "Automatic · 15 Mbps 4K");
    next.c.select_quality("auto");
    next.advance(2_500);
    assert_eq!(asked(&next)[1..], [(PlaybackMode::Auto, None)]);
    assert_eq!(memory.value(), None);
    assert_eq!(next.picked(), ["auto"]);
}

#[test]
fn automatic_starts_where_the_server_says_this_network_allows_over_the_tvs_own_memory() {
    let memory = Memory::with(3_000.0, WALL - 60_000);
    let server = FakeNetwork::new(Some(Some(Remembered { kbps: 16_000.0, at: WALL - 3_600_000 })));
    let mut w = automatic(item(file()), memory.clone(), Some(server.clone()), converted);
    assert_eq!(server.asked(), 1);
    let asked: Vec<(PlaybackMode, Option<u32>)> = w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate)).collect();
    assert_eq!(asked, [(PlaybackMode::Transcode, Some(15_000))]);
    assert_eq!(w.c.state().note.as_deref(), Some("Lowered for your connection"));
    w.c.close();

    let none = FakeNetwork::new(Some(None));
    let fresh = automatic(item(file()), memory, Some(none), converted);
    let asked: Vec<(PlaybackMode, Option<u32>)> =
        fresh.fake.requests().iter().map(|r| (r.mode, r.max_bitrate)).collect();
    assert_eq!(asked, [(PlaybackMode::Auto, None)]);
}

#[test]
fn automatic_goes_by_the_tvs_own_memory_when_the_server_cant_say() {
    let memory = Memory::with(9_000.0, WALL - 60_000);
    let w = automatic(item(file()), memory, Some(FakeNetwork::new(None)), converted);
    let asked: Vec<(PlaybackMode, Option<u32>)> = w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate)).collect();
    assert_eq!(asked, [(PlaybackMode::Transcode, Some(8_000))]);
}

#[test]
fn automatic_asks_nothing_of_the_network_for_a_quality_picked() {
    let server = FakeNetwork::new(Some(Some(Remembered { kbps: 2_000.0, at: 0 })));
    let network = server.clone();
    let mut w = World::new(Fake::new(item(file()), converted), move |env| {
        env.quality = Arc::new(Picked::new("original"));
        env.network = Some(network);
    });
    w.start();
    assert_eq!(server.asked(), 0);
    assert_eq!(w.fake.modes(), [PlaybackMode::Auto]);
}

#[test]
fn automatic_tells_the_server_the_ceiling_of_a_step_down_and_to_forget_it_when_picked_again() {
    let server = FakeNetwork::new(Some(None));
    let mut w = automatic(item(file()), Memory::default(), Some(server.clone()), converted);
    slow_network(&mut w);
    assert_eq!(server.told(), [Some(15_000.0)]);
    w.c.select_quality("auto");
    w.settle();
    assert_eq!(server.told(), [Some(15_000.0), None]);
}

#[test]
fn automatic_doesnt_start_again_when_original_is_picked_while_it_plays_the_original() {
    let mut w = automatic(item(file()), Memory::default(), None, converted);
    w.c.select_quality("original");
    w.advance(2_500);
    assert_eq!(w.fake.requests().len(), 1);
    assert_eq!(w.picked(), ["original"]);
    assert_eq!(w.c.state().quality, "Original");
}

// ---- on LG, whose player goes black between streams

/// As `converted`, from a server whose conversions' bitrate changes as they are sent.
fn live_converted(req: &PlaybackRequest) -> pley_core::model::PlaybackInfo {
    let mut i = converted(req);
    i.live_bitrate = req.mode != PlaybackMode::Auto;
    i
}

/// The original, converted by the server at 30 Mbps 4K, its bitrate changing as it is sent.
fn converts_original(_: &PlaybackRequest) -> pley_core::model::PlaybackInfo {
    info(|i| {
        i.method = PlaybackMethod::Transcode;
        i.hls = true;
        i.url = "/api/media/pley_mk_KEY2/master.m3u8".into();
        i.auto_quality = Some("30m".into());
        i.live_bitrate = true;
    })
}

/// A 1080p film of 20 Mbps.
fn hd() -> pley_core::model::MediaFile {
    let mut f = file();
    f.bitrate = Some(20_000_000);
    f.width = Some(1920);
    f.height = Some(1080);
    f.streams[0] = stream(0, "video", "hevc", None, true, |s| {
        s.width = Some(1920);
        s.height = Some(1080);
    });
    f
}

#[test]
fn lg_steps_a_conversions_bitrate_as_it_is_sent() {
    let server = FakeNetwork::new(Some(Some(Remembered { kbps: 9_000.0, at: WALL })));
    let mut w = automatic(item(hd()), Memory::default(), Some(server.clone()), live_converted);
    let asked: Vec<(PlaybackMode, Option<u32>, Option<u32>)> =
        w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate, r.max_height)).collect();
    assert_eq!(asked, [(PlaybackMode::Transcode, Some(8_000), Some(1080))]);
    assert_eq!(w.c.state().quality, "Automatic · 8 Mbps 1080p");
    // 8 Mbps arriving at 0.7 of its pace: 8 × 0.7 × 0.75 = 4.2 → 4 Mbps, still 1080p.
    w.play(12, 0.7);
    w.wait(2_500);
    w.ready();
    assert_eq!(w.fake.bitrates(), ["bitrate s8000 4000"]);
    assert_eq!(w.fake.requests().len(), 1);
    assert_eq!(w.sim.loads().len(), 1);
    assert_eq!(w.c.state().quality, "Automatic · 4 Mbps 1080p");
    assert_eq!(w.c.state().note.as_deref(), Some("Lowered for your connection"));
    assert_eq!(auto_hint(&w), "Now 4 Mbps 1080p · Lowered for your connection");
    assert_eq!(w.notices, ["Quality lowered to 4 Mbps 1080p"]);
    // What the network carries, for the next title.
    assert_eq!(server.told(), [Some(4_200.0)]);

    // Again 20 s on: 4 × 0.3 × 0.75 is 0.9, but 1080p keeps 3 Mbps.
    w.play(21, 0.3);
    w.wait(2_500);
    w.ready();
    assert_eq!(w.fake.bitrates(), ["bitrate s8000 4000", "bitrate s8000 3000"]);
    // Below that, the network waits: no new stream, ever.
    w.play(21, 0.3);
    w.wait(2_500);
    w.ready();
    w.play(21, 0.3);
    assert_eq!(w.fake.bitrates().len(), 2);
    assert_eq!(w.fake.requests().len(), 1);
    assert_eq!(w.c.state().quality, "Automatic · 3 Mbps 1080p");
}

#[test]
fn lg_steps_back_up_the_same_way_and_a_new_stream_of_the_quality_goes_on_where_it_came_to() {
    let server = FakeNetwork::new(Some(Some(Remembered { kbps: 9_000.0, at: WALL })));
    let mut w = automatic(item(hd()), Memory::default(), Some(server), live_converted);
    w.play(12, 0.7);
    w.wait(2_500);
    w.ready();
    assert_eq!(w.fake.bitrates(), ["bitrate s8000 4000"]);
    // A recovery's new stream goes on at 4 Mbps too.
    w.sim.fail(2, "connection reset");
    w.advance(2_500);
    let asked: Vec<(PlaybackMode, Option<u32>)> = w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate)).collect();
    assert_eq!(asked, [(PlaybackMode::Transcode, Some(8_000)), (PlaybackMode::Transcode, Some(8_000))]);
    assert_eq!(w.fake.bitrates(), ["bitrate s8000 4000", "bitrate s8000 4000"]);
    w.ready();
    // The buffer refilling at 4 times its pace says the network carries 16 Mbps at least: 1.6 × 8 Mbps.
    w.play(8, 4.0);
    w.play(310, 1.02);
    assert_eq!(w.fake.bitrates(), ["bitrate s8000 4000", "bitrate s8000 4000", "bitrate s8000 8000"]);
    assert_eq!(w.fake.requests().len(), 2);
    assert_eq!(w.c.state().quality, "Automatic · 8 Mbps 1080p");
}

#[test]
fn lg_plays_on_as_it_was_when_the_server_cant_change_the_bitrate() {
    let server = FakeNetwork::new(Some(Some(Remembered { kbps: 9_000.0, at: WALL })));
    let mut w = automatic(item(hd()), Memory::default(), Some(server), live_converted);
    w.fake.refuse_bitrates();
    w.play(12, 0.7);
    w.wait(2_500);
    w.ready();
    assert_eq!(w.fake.bitrates(), ["bitrate s8000 4000"]);
    assert_eq!(w.c.state().quality, "Automatic · 8 Mbps 1080p");
    // From then on, a stream that can't change: short waits don't move it, and nothing is asked again.
    w.play(21, 1.2);
    w.wait(800);
    w.ready();
    w.play(10, 1.2);
    w.wait(800);
    w.ready();
    w.play(10, 1.2);
    assert_eq!(w.fake.bitrates().len(), 1);
    assert_eq!(w.fake.requests().len(), 1);
}

#[test]
fn lg_steps_the_original_the_server_converts_and_automatic_picked_again_sends_it_back_to_its_own_bitrate() {
    let mut w = automatic(item(file()), Memory::default(), None, converts_original);
    assert_eq!(w.c.state().quality, "Automatic · 30 Mbps 4K");
    // 30 × 0.5 × 0.75 = 11.25 → 11 Mbps, still 4K.
    w.play(12, 0.5);
    w.wait(2_500);
    w.ready();
    assert_eq!(w.fake.bitrates(), ["bitrate s1 11000"]);
    assert_eq!(w.c.state().quality, "Automatic · 11 Mbps 4K");
    assert_eq!(w.c.state().note.as_deref(), Some("Lowered for your connection"));
    w.c.select_quality("auto");
    w.advance(10);
    assert_eq!(w.fake.bitrates(), ["bitrate s1 11000", "bitrate s1 30000"]);
    assert_eq!(w.fake.requests().len(), 1);
    assert_eq!(w.c.state().quality, "Automatic · 30 Mbps 4K");
    assert_eq!(w.c.state().note, None);
}

#[test]
fn lg_moves_the_original_to_a_conversion_only_once_its_picture_stops_for_2_s() {
    let mut w = automatic(item(file()), Memory::default(), None, live_converted);
    w.wait(800);
    w.ready();
    w.play(10, 1.2);
    w.wait(800);
    w.ready();
    w.play(40, 0.8);
    assert_eq!(w.fake.requests().len(), 1);
    // 40 Mbps arriving at 0.8 of its pace: 40 × 0.8 × 0.75 = 24 → 15 Mbps 4K, a new stream made ready
    // beside it. The buffer refilling at 1.2 showed 48 Mbps at least: under 1.5 times the file's.
    w.wait(2_500);
    w.advance(2_500);
    let asked: Vec<(PlaybackMode, Option<u32>, bool)> =
        w.fake.requests().iter().map(|r| (r.mode, r.max_bitrate, r.keep_previous)).collect();
    assert_eq!(asked, [(PlaybackMode::Auto, None, false), (PlaybackMode::Transcode, Some(15_000), true)]);
    assert!(w.fake.bitrates().is_empty());
}

#[test]
fn lg_plays_the_original_on_through_dropped_frames_past_its_first_minute() {
    let server = FakeNetwork::new(Some(None));
    let mut w = automatic(item(file()), Memory::default(), Some(server.clone()), converted);
    w.frames = Some((0, 0));
    w.drop_frames(62, 0);
    w.drop_frames(14, 130);
    w.advance(2_500);
    assert_eq!(w.fake.requests().len(), 1);
    // 40 × 0.75 = 30 → 30 Mbps 4K, where the next title starts.
    assert_eq!(server.told(), [Some(30_000.0)]);
    assert_eq!(w.c.state().quality, "Automatic · Original");
}

// ---- a change while it plays

/// The original as it is (s1); a conversion asked for (s4000, s2000…).
fn answer(req: &PlaybackRequest) -> pley_core::model::PlaybackInfo {
    info(|i| {
        if let Some(b) = req.max_bitrate {
            i.session_id = format!("s{b}");
            i.method = PlaybackMethod::Transcode;
            i.hls = true;
            i.url = "/api/media/pley_mk_KEY2/master.m3u8".into();
        }
    })
}

/// A playback whose picture shows; the calls so far forgotten.
fn shown() -> World {
    let mut w = World::new(Fake::new(item(file()), answer), |_| ());
    w.start();
    w.open();
    w.fake.clear_calls();
    w
}

#[test]
fn makes_the_new_stream_ready_beside_the_one_playing_then_moves_to_it_from_where_that_one_got_to() {
    let mut w = shown();
    w.fake.hold_segments();
    w.c.select_quality("4m");
    w.until(|w| w.fake.calls().len() == 4);
    let r = &w.fake.requests()[1];
    assert_eq!((r.keep_previous, r.max_bitrate, r.start_ms), (true, Some(4000), Some(600_000)));
    // Asked for as the TV's player will: the playlist, the size it lists first, and the segment 4 s on.
    assert_eq!(
        w.fake.calls(),
        [
            "start transcode",
            "GET pley_mk_KEY2/master.m3u8",
            "GET pley_mk_KEY2/v0/index.m3u8",
            "HEAD pley_mk_KEY2/v0/151.ts"
        ]
    );
    // The one playing plays on meanwhile, without a ring.
    assert_eq!(w.sim.loads().len(), 1);
    assert_eq!(w.c.state().phase, Phase::Playing);
    w.sim.set_position(601_500);
    w.fake.release_segment();
    w.until(|w| w.sim.loads().len() == 2);
    w.settle();
    // Then it ends, at where it got to, and the engine moves on from there.
    assert_eq!(w.fake.calls()[4..], ["progress stopped 601500", "delete s1"]);
    let load = &w.sim.loads()[1];
    assert_eq!((load.kind, load.start_ms), (MediaKind::Hls, 601_500));
    assert_eq!(load.url, "http://nas:7539/api/media/pley_mk_KEY2/master.m3u8");
    assert_eq!(w.c.state().phase, Phase::Preparing);
    assert!(w.c.state().switching);
    w.advance(400);
    assert!(!w.c.state().switching);
}

#[test]
fn moves_after_20_s_at_most_when_the_stream_isnt_ready_by_then() {
    let mut w = shown();
    w.fake.hold_segments();
    w.c.select_quality("4m");
    w.advance(19_000);
    assert_eq!(w.sim.loads().len(), 1);
    assert!(!w.fake.calls().contains(&"delete s1".to_string()));
    w.advance(1_100);
    assert_eq!(w.sim.loads().len(), 2);
    assert!(w.fake.calls().contains(&"delete s1".to_string()));
}

#[test]
fn ends_the_one_playing_first_when_no_transcode_is_free_for_two_at_once() {
    let mut w = shown();
    w.fake.set_busy(true);
    w.c.select_quality("4m");
    w.settle();
    let kept: Vec<bool> = w.fake.requests()[1..].iter().map(|r| r.keep_previous).collect();
    assert_eq!(kept, [true, false]);
    assert_eq!(w.fake.calls(), ["start transcode", "progress stopped 600000", "delete s1", "start transcode"]);
    let load = &w.sim.loads()[1];
    assert_eq!((load.kind, load.start_ms), (MediaKind::Hls, 600_000));
    assert!(!w.c.state().switching);
    assert!(!matches!(w.c.state().phase, Phase::Error { .. }));
}

#[test]
fn lets_a_session_it_was_making_ready_go_when_another_change_comes_first() {
    let mut w = shown();
    w.fake.hold_segments();
    w.c.select_quality("4m");
    w.until(|w| w.fake.calls().iter().any(|c| c.starts_with("HEAD")));
    w.c.select_quality("2m");
    w.until(|w| w.fake.calls().iter().filter(|c| c.starts_with("HEAD")).count() == 2);
    assert!(w.fake.calls().contains(&"delete s4000".to_string()));
    assert!(!w.fake.calls().contains(&"delete s1".to_string()));
    let r = &w.fake.requests()[2];
    assert_eq!((r.keep_previous, r.max_bitrate), (true, Some(2000)));
    w.fake.release_segment();
    w.until(|w| w.sim.loads().len() == 2);
    w.settle();
    assert!(w.fake.calls().contains(&"delete s1".to_string()));
}

#[test]
fn starts_again_at_once_to_recover_from_an_error_ending_the_one_that_failed_first() {
    let mut w = shown();
    w.sim.fail(3, "decoder");
    w.advance(10);
    let asked: Vec<(PlaybackMode, bool)> = w.fake.requests().iter().map(|r| (r.mode, r.keep_previous)).collect();
    assert_eq!(asked, [(PlaybackMode::Auto, false), (PlaybackMode::Transcode, false)]);
    assert_eq!(w.fake.calls()[..3], ["progress stopped 600000", "delete s1", "start transcode"]);
    assert!(w.fake.calls().iter().all(|c| !c.starts_with("HEAD")));
    assert!(!w.c.state().switching);
}

#[test]
fn starts_at_once_when_nothing_plays_yet() {
    let mut w = World::new(Fake::new(item(file()), answer), |_| ());
    w.start();
    // Its picture hasn't shown: nothing to keep playing beside the next.
    w.c.select_quality("4m");
    w.settle();
    let kept: Vec<bool> = w.fake.requests().iter().map(|r| r.keep_previous).collect();
    assert_eq!(kept, [false, false]);
}

// ---- warming a stream (prepare.test.ts)

#[test]
fn warming_asks_for_a_single_size_streams_segment_directly_and_is_done_when_anything_fails() {
    let fake = Fake::new(item(file()), answer);
    fake.fail_heads();
    warm_stream(fake.as_ref(), "http://nas:7539/api/media/k/index.m3u8", 7_000);
    assert_eq!(fake.calls(), ["GET k/index.m3u8", "HEAD k/1.ts"]);
}

// ---- speed

#[test]
fn speed_isnt_offered_where_the_player_cant_keep_the_sound() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |env| env.speeds = Vec::new());
    w.start();
    assert!(w.c.state().speeds.is_empty());
    w.c.set_speed(2.0);
    assert_eq!(w.c.state().speed, 1.0);
}

#[test]
fn plays_at_the_speed_picked_only_those_offered_and_keeps_it_for_the_titles_next_session() {
    let mut w =
        World::new(Fake::new(item(file()), |req| info(|i| i.session_id = format!("s-{}", req.mode.as_str()))), |_| ());
    w.start();
    assert_eq!(w.c.state().speeds, SPEEDS);
    w.c.set_speed(1.5);
    w.c.set_speed(3.0);
    let rates = |w: &World| w.sim.commands().into_iter().filter(|c| c.starts_with("rate")).collect::<Vec<_>>();
    assert_eq!((w.c.state().speed, rates(&w)), (1.5, vec!["rate 1.5".to_string()]));
    // Another session of the same title plays at it too.
    w.c.select_quality("8m");
    w.settle();
    assert_eq!(w.sim.loads().len(), 2);
    assert_eq!(rates(&w), ["rate 1.5", "rate 1.5"]);
}

// ---- picture subtitles

#[test]
fn picture_subtitles_are_asked_for_from_where_the_player_is_and_at_once_from_a_seek() {
    let mut w = World::new(Fake::new(item(file()), |_| info(|_| ())), |_| ());
    w.start();
    w.play(2, 1.0);
    let at = w.c.live_position_ms();
    w.c.select_subtitle(Some(3));
    w.settle();
    let asks = w.fake.subtitle_asks();
    assert_eq!(asks.len(), 1, "{asks:?}");
    assert!(at > 0 && asks[0].ends_with(&format!("/subtitles/3.mks?at={at}")), "{at} {asks:?}");
    // A seek asks at once from where it went, not 3 s after the last answer.
    w.c.seek_to(90_000);
    w.settle();
    let asks = w.fake.subtitle_asks();
    assert_eq!(asks.len(), 2, "{asks:?}");
    assert!(asks[1].ends_with("/subtitles/3.mks?at=90000"), "{asks:?}");
    // Then 3 s after that ask, from where the player is then.
    w.advance(2_900);
    assert_eq!(w.fake.subtitle_asks().len(), 2);
    w.advance(200);
    let asks = w.fake.subtitle_asks();
    assert_eq!(asks.len(), 3, "{asks:?}");
    assert!(asks[2].contains("/subtitles/3.mks?at="), "{asks:?}");
}

// ---- subtitles found on the server

#[test]
fn subtitles_found_are_searched_in_two_letter_codes_and_one_added_is_listed_and_shows() {
    let fake = Fake::new(item(file()), |_| {
        info(|i| {
            i.method = PlaybackMethod::Remux;
            i.hls = true;
            i.url = "/api/media/pley_mk_KEY/master.m3u8".into();
        })
    });
    fake.on_add(|result| SubtitleTrack {
        index: 2000,
        codec: "subrip".into(),
        language: Some("fre".into()),
        external: true,
        text_based: true,
        url: Some(format!("/api/files/7/subtitles/2000.vtt?r={result}")),
        added: true,
        ..SubtitleTrack::default()
    });
    let mut w = World::new(fake, |_| ());
    w.start();
    w.c.find_subtitles(Some("fre"));
    w.settle();
    assert_eq!(w.fake.searched(), [(70, Some(7), vec!["fr".to_string()])]);
    assert_eq!(w.c.state().search, SubtitleSearch::Found(Vec::new()));

    let found = SubtitleResult {
        id: "opensubtitles:9".into(),
        provider: "opensubtitles".into(),
        language: Some("fr".into()),
        name: "Film".into(),
        format: Some("srt".into()),
        downloads: 3,
        matched: Some("hash".into()),
        ..SubtitleResult::default()
    };
    w.c.add_found_subtitle(&found);
    w.settle();
    let last = w.c.state().subtitles.last().cloned().unwrap();
    assert_eq!((last.index, last.selected, last.added), (Some(2000), true, true));
    assert!(w.c.state().notice.as_deref().unwrap().starts_with("Subtitles added: French"));
    // Text: drawn by the TV from the same session, chosen for the title as any track picked.
    assert_eq!(w.fake.requests().len(), 1);
    assert_eq!(w.fake.saved().last(), Some(&(AudioChoice::Keep, Some(2000))));
}
