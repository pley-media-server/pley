//! Playing: sessions, keyed addresses, Range, HLS, stopping, subtitles,
//! seek previews, what playing writes, the network memory and the event
//! stream, against the fixture server.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{Api, Events, eventually, send};
use pley_fixtures::{FixtureServer, ManualClock, Options, ids};
use serde_json::{Value, json};

fn media(name: &str) -> Vec<u8> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../android/testing/fixtures/src/main/resources/app/pley/android/fixtures/media");
    std::fs::read(dir.join(name)).expect("the Android fixtures' media")
}

/// What Pley for LG TV reports on a 4K HDR10 TV.
fn tv_caps() -> Value {
    json!({
        "caps_version": 2,
        "containers": ["mkv", "mp4", "ts"],
        "video": [
            { "codec": "h264", "hardware": true, "profiles": ["Main", "High"], "max_width": 3840, "max_height": 2160, "max_fps": 60.0, "max_level": 51 },
            { "codec": "hevc", "hardware": true, "profiles": ["Main", "Main 10"], "max_width": 4096, "max_height": 2160, "max_fps": 60.0, "max_bit_depth": 10, "max_level": 153 }
        ],
        "hdr_types": ["hdr10", "hlg"],
        "audio_decode": ["aac", "eac3", "mp3"],
        "audio_channels": 6,
        "audio_track_switching": true,
        "picture_subtitles_sideload": true,
        "hls": { "master_playlist": true, "segments": ["ts"] },
    })
}

fn play(api: &Api, body: Value) -> Value {
    let reply = api.post("/api/playback", &body);
    assert_eq!(reply.status, 200, "{}", reply.text());
    reply.json()
}

#[test]
fn direct_play_at_a_keyed_address_with_range() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let info =
        play(&api, json!({ "item_id": ids::SAMPLE_1080P, "keyed_urls": true, "client_id": "tv", "caps": tv_caps() }));
    assert_eq!(info["method"], "direct");
    assert_eq!(info["hls"], false);
    assert_eq!(info["file_id"], ids::SAMPLE_1080P_FILE);
    assert_eq!(info["duration_ms"], 20_000);
    assert_eq!(info["session_id"].as_str().unwrap().len(), 32);
    assert!(info["pts_offset_ms"].is_null() && info["reason"].is_null());
    assert_eq!(info["reasons"], json!([]));
    let url = info["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("/api/media/pley_mk_") && url.ends_with("/file.mp4"), "{url}");

    let clip = media("sample-1080p.mp4");
    // The key is the credential: no token.
    let whole = send(server.url(), "GET", &url, &[], b"");
    assert_eq!(whole.status, 200);
    assert_eq!(whole.body.len(), clip.len());
    assert_eq!(whole.header("Accept-Ranges"), Some("bytes"));
    assert_eq!(whole.header("Cache-Control"), Some("private, no-store"));
    let part = send(server.url(), "GET", &url, &[("Range", "bytes=0-99")], b"");
    assert_eq!(part.status, 206);
    assert_eq!(part.header("Content-Range"), Some(format!("bytes 0-99/{}", clip.len()).as_str()));
    assert_eq!(part.body, clip[..100]);
    let tail = send(server.url(), "GET", &url, &[("Range", "bytes=-10")], b"");
    assert_eq!(tail.body, clip[clip.len() - 10..]);
    let open = send(server.url(), "GET", &url, &[("Range", &format!("bytes={}-", clip.len() - 5))], b"");
    assert_eq!(open.body.len(), 5);
    let past = send(server.url(), "GET", &url, &[("Range", &format!("bytes={}-", clip.len()))], b"");
    assert_eq!(past.status, 416);
    assert_eq!(past.header("Content-Range"), Some(format!("bytes */{}", clip.len()).as_str()));
    let head = send(server.url(), "HEAD", &url, &[], b"");
    assert_eq!(head.header("Content-Length"), Some(clip.len().to_string().as_str()));
    assert!(head.body.is_empty());

    // Without keys, the file's own address and a token.
    let bearer = play(&api, json!({ "item_id": ids::SAMPLE_1080P, "caps": tv_caps() }));
    let stream = bearer["url"].as_str().unwrap();
    assert_eq!(stream, format!("/api/files/{}/stream", ids::SAMPLE_1080P_FILE));
    assert_eq!(send(server.url(), "GET", stream, &[], b"").status, 401);
    assert_eq!(api.get_with(stream, &[("Range", "bytes=10-19")]).body, clip[10..20]);

    // A new session of the same client ended the first: its key is gone.
    let again =
        play(&api, json!({ "item_id": ids::SAMPLE_1080P, "keyed_urls": true, "client_id": "tv", "caps": tv_caps() }));
    assert_eq!(send(server.url(), "GET", &url, &[], b"").status, 404);
    assert_eq!(send(server.url(), "GET", again["url"].as_str().unwrap(), &[("Range", "bytes=0-0")], b"").status, 206);
}

#[test]
fn hls_sessions_transcode_and_change_bitrate() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let v1 = json!({ "containers": ["mp4"], "video_codecs": ["h264"], "audio_codecs": ["aac"] });
    let info = play(&api, json!({ "item_id": ids::SAMPLE_HDR10, "caps": v1 }));
    assert_eq!(info["method"], "transcode");
    assert_eq!(info["hls"], true);
    assert_eq!(info["pts_offset_ms"], 1_400);
    assert_eq!(info["live_bitrate"], true);
    assert_eq!(info["audio_converted"], true);
    assert!(info["auto_quality"].is_string());
    assert!(info["reason"].as_str().unwrap().contains("HEVC video not supported."));
    let session = info["session_id"].as_str().unwrap().to_string();
    let url = info["url"].as_str().unwrap();
    assert_eq!(url, format!("/api/playback/{session}/index.m3u8"));
    let subtitles: Vec<(i64, bool)> = info["subtitles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["index"].as_i64().unwrap(), s["url"].is_string()))
        .collect();
    assert_eq!(subtitles, [(2, true), (1_000, true)]);

    let playlist = api.get(url);
    assert_eq!(playlist.header("Content-Type"), Some("application/vnd.apple.mpegurl"));
    assert!(playlist.text().contains("#EXT-X-ENDLIST") && playlist.text().contains("0.ts"));
    let segment = api.get(&format!("/api/playback/{session}/0.ts"));
    assert_eq!(segment.header("Content-Type"), Some("video/mp2t"));
    assert_eq!(segment.body, media("hls/0.ts"));
    assert_eq!(api.get(&format!("/api/playback/{session}/7.ts")).status, 404);

    let status = api.get(&format!("/api/playback/{session}")).json();
    assert_eq!(status["method"], "transcode");
    assert_eq!(status["transcode"]["state"], "finished");
    // Within 1 Mbps and what the session aims for (2 Mbps for this small file).
    let changed = api.put(&format!("/api/playback/{session}/bitrate"), &json!({ "max_bitrate": 1_500 })).json();
    assert_eq!(changed["bitrate"], 1_500);
    assert_eq!(api.get(&format!("/api/playback/{session}")).json()["transcode"]["bitrate_out"], 1_500_000);
    let ceiling = api.put(&format!("/api/playback/{session}/bitrate"), &json!({ "max_bitrate": 90_000 })).json();
    assert_eq!(ceiling["bitrate"], 2_000);
    let floor = api.put(&format!("/api/playback/{session}/bitrate"), &json!({ "max_bitrate": 10 })).json();
    assert_eq!(floor["bitrate"], 1_000);
    let robin = Api::new(&server, ids::ROBIN);
    assert_eq!(robin.get(&format!("/api/playback/{session}")).status, 403);
    assert_eq!(api.delete(&format!("/api/playback/{session}")).status, 204);
    assert_eq!(api.get(&format!("/api/playback/{session}")).status, 404);

    // A remux named in a master playlist, at a keyed address.
    let remux = play(&api, json!({ "item_id": ids::DOLBY_VISION_P81, "keyed_urls": true, "caps": tv_caps() }));
    assert_eq!(remux["method"], "remux", "{remux}");
    assert_eq!(remux["video_range"], "Dolby Vision P8.1");
    let master_url = remux["url"].as_str().unwrap();
    assert!(master_url.ends_with("/master.m3u8"));
    let master = send(server.url(), "GET", master_url, &[], b"").text();
    assert!(
        master.contains("#EXT-X-STREAM-INF") && master.contains("VIDEO-RANGE=PQ") && master.ends_with("index.m3u8\n")
    );
    let base = master_url.trim_end_matches("master.m3u8");
    assert_eq!(send(server.url(), "GET", &format!("{base}index.m3u8"), &[], b"").status, 200);
    assert_eq!(send(server.url(), "GET", &format!("{base}4.ts"), &[], b"").body, media("hls/4.ts"));
    assert_eq!(send(server.url(), "GET", &format!("{base}file.mkv"), &[], b"").status, 404);

    let lower =
        play(&api, json!({ "item_id": ids::SAMPLE_1080P, "caps": tv_caps(), "max_bitrate": 400, "max_height": 480 }));
    assert_eq!(lower["method"], "transcode");
    assert_eq!(lower["reasons"], json!(["Lower quality picked."]));
    assert!(lower["auto_quality"].is_null());
    assert_eq!(api.post("/api/playback", &json!({ "item_id": ids::SHOW })).status, 400);
    assert_eq!(api.post("/api/playback", &json!({ "item_id": 1, "file_id": 5 })).status, 404);
}

#[test]
fn sessions_stopped_from_elsewhere_answer_410() {
    let server =
        FixtureServer::start_with(Options { keep_alive: Duration::from_millis(200), ..Options::default() }).unwrap();
    let api = Api::alex(&server);
    let mut events = Events::open(&api);
    assert_eq!(events.status, 200);
    let info = play(&api, json!({ "item_id": ids::SAMPLE_HDR10, "keyed_urls": true, "caps": {} }));
    let session = info["session_id"].as_str().unwrap().to_string();
    let url = info["url"].as_str().unwrap().to_string();
    assert!(events.wait_for("sessions_changed", Duration::from_secs(5)).is_some());

    let stop = api.json("DELETE", &format!("/api/sessions/{session}"), &json!({ "message": "Time for bed" }));
    assert_eq!(stop.status, 204);
    let stopped = events.wait_for("session_stopped", Duration::from_secs(5)).expect("the player hears it");
    assert_eq!(stopped["session_id"], session);
    assert_eq!(stopped["message"], "Time for bed");
    assert_eq!(stopped["by_admin"], false);
    let gone = send(server.url(), "GET", &url, &[], b"");
    assert_eq!(gone.status, 410);
    assert_eq!(
        gone.json(),
        json!({ "code": "session_stopped", "error": "session_stopped", "message": "Time for bed" })
    );
    let report = api.post(
        &format!("/api/items/{}/progress", ids::SAMPLE_HDR10),
        &json!({ "position_ms": 16_000, "session_id": session }),
    );
    assert_eq!(report.status, 410);
    assert_eq!(api.get(&format!("/api/items/{}", ids::SAMPLE_HDR10)).json()["user_state"]["position_ms"], 16_000);
    assert_eq!(api.get(&format!("/api/playback/{session}")).status, 410);
    assert_eq!(api.delete(&format!("/api/playback/{session}")).status, 204);
}

#[test]
fn the_remote_sends_a_players_command_only_to_its_own_user_and_only_what_it_obeys() {
    let server =
        FixtureServer::start_with(Options { keep_alive: Duration::from_millis(200), ..Options::default() }).unwrap();
    let api = Api::alex(&server);
    let mut events = Events::open(&api);
    let body = json!({
        "item_id": ids::SAMPLE_HDR10, "keyed_urls": true, "caps": {},
        "controls": ["play", "pause", "seek", "shuffle"], "device_kind": "tv",
    });
    let session = play(&api, body)["session_id"].as_str().unwrap().to_string();
    let listed = api.get("/api/sessions").json();
    let mine = listed["sessions"].as_array().unwrap().iter().find(|s| s["session_id"] == session).unwrap().clone();
    // What the server doesn't know is left out.
    assert_eq!(
        (&mine["kind"], &mine["controls"], &mine["device_kind"]),
        (&json!("item"), &json!(["play", "pause", "seek"]), &json!("tv"))
    );

    let path = format!("/api/sessions/{session}/command");
    assert_eq!(api.post(&path, &json!({ "command": "seek", "position_ms": 60_000 })).status, 202);
    let heard = events.wait_for("player_command", Duration::from_secs(5)).expect("the player hears it");
    assert_eq!(
        heard,
        json!({
            "type": "player_command", "session_id": session, "command": "seek",
            "position_ms": 60_000, "offset_ms": null,
        })
    );
    let refused = api.post(&path, &json!({ "command": "next" }));
    assert_eq!((refused.status, refused.json()["code"].as_str()), (409, Some("not_controllable")));
    assert_eq!(api.post(&path, &json!({ "command": "seek" })).status, 400);
    assert_eq!(api.post(&path, &json!({ "command": "shuffle" })).status, 400);
    assert_eq!(api.post("/api/sessions/nope/command", &json!({ "command": "pause" })).status, 404);
    // Someone else's session isn't theirs to command.
    let sam = Api::new(&server, ids::SAM);
    let theirs = play(&sam, json!({ "item_id": ids::SAMPLE_1080P, "caps": {}, "controls": ["pause"] }));
    let theirs = format!("/api/sessions/{}/command", theirs["session_id"].as_str().unwrap());
    assert_eq!(api.post(&theirs, &json!({ "command": "pause" })).status, 403);
}

#[test]
fn subtitles_as_webvtt_and_mks() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let path = format!("/api/files/{}/subtitles/{}.vtt", ids::SAMPLE_HDR10_FILE, ids::SAMPLE_HDR10_SUBTITLE);
    let partial = api.get(&path);
    assert_eq!(partial.header("Content-Type"), Some("text/vtt; charset=utf-8"));
    assert_eq!(partial.header("X-Pley-Complete"), Some("0"));
    assert_eq!(partial.text().matches(" --> ").count(), 2);
    let whole = api.get(&path);
    assert!(whole.header("X-Pley-Complete").is_none());
    assert_eq!(whole.header("Cache-Control"), Some("private, max-age=3600"));
    let text = whole.text();
    assert!(text.starts_with("WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nThe fixture server plays this."));
    assert_eq!(text.matches(" --> ").count(), 4);
    let shifted = api.get(&format!("{path}?offset_ms=1500")).text();
    assert!(shifted.contains("00:00:02.500 --> 00:00:05.500"));
    assert_eq!(api.get(&format!("{path}?offset_ms=soon")).status, 400);
    let french = api.get(&format!("/api/files/{}/subtitles/1000.vtt", ids::SAMPLE_HDR10_FILE)).text();
    assert!(french.contains("Le serveur de test joue ceci."));

    let pgs = format!("/api/files/{}/subtitles/{}.mks", ids::PGS_MOVIE_FILE, ids::PGS_MOVIE_SUBTITLE);
    let mks = api.get(&pgs);
    assert_eq!(mks.status, 200);
    assert_eq!(mks.header("Content-Type"), Some("video/x-matroska"));
    assert_eq!(mks.header("X-Pley-Complete"), Some("1"));
    assert_eq!(&mks.body[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
    assert!(mks.body.windows(10).any(|w| w == b"S_HDMV/PGS"));
    assert_eq!(api.get_with(&pgs, &[("Range", "bytes=0-3")]).status, 206);
    assert_eq!(api.get(&format!("/api/files/{}/subtitles/4.vtt", ids::PGS_MOVIE_FILE)).status, 404);
    assert_eq!(api.get(&format!("/api/files/{}/subtitles/3.mks", ids::PGS_MOVIE_FILE)).status, 404);
    let info = play(&api, json!({ "item_id": ids::PGS_MOVIE, "caps": tv_caps() }));
    let tracks = info["subtitles"].as_array().unwrap();
    let picture = tracks.iter().find(|t| t["index"] == 4).unwrap();
    assert!(picture["url"].is_null() && picture["text_based"] == false);
    assert!(picture.get("deletable").is_none());

    // Found online, added, served and deleted.
    let results = api.get(&format!("/api/items/{}/subtitles/search?language=en,fr", ids::SAMPLE_1080P)).json();
    let results = results["results"].as_array().unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0]["provider"], "opensubtitles");
    let added =
        api.post(&format!("/api/items/{}/subtitles", ids::SAMPLE_1080P), &json!({ "result": results[2]["id"] }));
    assert_eq!(added.status, 201, "{}", added.text());
    let added = added.json();
    assert_eq!(
        (added["index"].as_i64(), added["deletable"].as_bool(), added["added"].as_bool()),
        (Some(9_000), Some(true), Some(true))
    );
    let vtt = api.get(added["url"].as_str().unwrap()).text();
    assert!(vtt.contains("Found online (fr)"));
    let item = api.get(&format!("/api/items/{}", ids::SAMPLE_1080P)).json();
    assert!(item["files"][0]["streams"].as_array().unwrap().iter().any(|s| s["index"] == 9_000 && s["added"] == true));
    assert_eq!(api.delete(&format!("/api/items/{}/subtitles/9000", ids::SAMPLE_1080P)).status, 204);
    server.set_subtitle_search(false);
    let off = api.get(&format!("/api/items/{}/subtitles/search", ids::SAMPLE_1080P));
    assert_eq!(off.status, 503);
    assert_eq!(off.json()["setup"], false);
}

#[test]
fn seek_previews_and_chapter_pictures() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let info = api.get(&format!("/api/files/{}/trickplay", ids::SAMPLE_1080P_FILE)).json();
    assert_eq!(
        info,
        json!({
            "id": format!("tp{}", ids::SAMPLE_1080P_FILE), "interval_ms": 2_000, "width": 320, "height": 180,
            "columns": 10, "rows": 10, "count": 10, "sheets": 1,
        })
    );
    let sheet = api.get(&format!("/api/files/{}/trickplay/0.jpg?v=tp", ids::SAMPLE_1080P_FILE));
    assert_eq!(sheet.header("Content-Type"), Some("image/jpeg"));
    let mut decoder = jpeg_decoder::Decoder::new(&sheet.body[..]);
    decoder.decode().unwrap();
    let size = decoder.info().unwrap();
    assert_eq!((size.width, size.height), (3_200, 180));
    assert_eq!(api.get(&format!("/api/files/{}/trickplay/1.jpg", ids::SAMPLE_1080P_FILE)).status, 404);
    let long = api.get("/api/files/1000100/trickplay").json();
    assert!(long["sheets"].as_u64().unwrap() >= 1);

    let chapter = api.get(&format!("/api/files/{}/chapters/1.jpg?v=tp", ids::SAMPLE_HDR10_FILE));
    let mut decoder = jpeg_decoder::Decoder::new(&chapter.body[..]);
    decoder.decode().unwrap();
    assert_eq!(decoder.info().unwrap().width, 320);
    assert_eq!(api.get(&format!("/api/files/{}/chapters/3.jpg", ids::SAMPLE_HDR10_FILE)).status, 404);
}

#[test]
fn progress_watched_ratings_and_track_choices() {
    let server =
        FixtureServer::start_with(Options { keep_alive: Duration::from_millis(200), ..Options::default() }).unwrap();
    let api = Api::alex(&server);
    let mut events = Events::open(&api);
    let progress = format!("/api/items/{}/progress", ids::SAMPLE_1080P);
    let agent = [
        ("Authorization", format!("Bearer {}", api.token)),
        ("Content-Type", "application/json".into()),
        ("User-Agent", "pleyLG/0.1.0 (webOS 23; OLED65C3)".into()),
    ];
    let agent: Vec<(&str, &str)> = agent.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let paused = api.post("/api/items/100/progress", &json!({ "position_ms": 600_000, "state": "paused" }));
    assert_eq!(paused.status, 204);
    let changed = events.wait_for("user_state_changed", Duration::from_secs(5)).unwrap();
    assert_eq!(changed["item_ids"], json!([100]));
    assert_eq!(api.get("/api/items/100").json()["user_state"]["position_ms"], 600_000);
    let early = api.post("/api/items/3/progress", &json!({ "position_ms": 9_000 }));
    assert_eq!(early.status, 204);
    assert_eq!(api.get("/api/items/3").json()["user_state"]["position_ms"], 0, "under 15 s is kept as 0");
    assert_eq!(api.post(&progress, &json!({ "position_ms": 1, "state": "dancing" })).status, 400);
    assert_eq!(api.post(&format!("/api/items/{}/progress", ids::SHOW), &json!({ "position_ms": 1 })).status, 400);

    let before = api.get("/api/history").json()["total"].as_i64().unwrap();
    assert_eq!(send(server.url(), "POST", &progress, &agent, br#"{"position_ms":19000}"#).status, 204);
    let state = api.get("/api/items/1").json()["user_state"].clone();
    assert_eq!(
        (state["watched"].as_bool(), state["position_ms"].as_i64(), state["play_count"].as_i64()),
        (Some(true), Some(0), Some(1))
    );
    let history = api.get("/api/history?limit=1").json();
    assert_eq!(history["total"].as_i64().unwrap(), before + 1);
    assert_eq!(history["entries"][0]["device"], "LG TV");
    assert!(events.wait_for("library_changed", Duration::from_secs(5)).is_some());
    assert_eq!(server.writes("POST", &progress).len(), 2);

    assert_eq!(api.post(&format!("/api/items/{}/unwatched", ids::SHOW), &json!({})).status, 204);
    let show = api.get(&format!("/api/items/{}", ids::SHOW)).json();
    assert_eq!(show["viewed_leaf_count"], 0);
    assert_eq!(api.post(&format!("/api/items/{}/watched", ids::SHOW_SEASON_1), &json!({})).status, 204);
    assert_eq!(api.get(&format!("/api/items/{}", ids::SHOW)).json()["viewed_leaf_count"], 3);

    assert_eq!(api.put("/api/items/1/rating", &json!({ "rating": 7.6 })).status, 204);
    assert_eq!(api.get("/api/items/1").json()["user_state"]["rating"], 8.0);
    assert_eq!(api.put("/api/items/1/rating", &json!({ "rating": 11 })).status, 400);
    assert_eq!(api.put("/api/items/1/rating", &json!({})).status, 204);
    assert!(api.get("/api/items/1").json()["user_state"].get("rating").is_none());

    let streams = format!("/api/items/{}/streams", ids::PGS_MOVIE);
    assert_eq!(api.put(&streams, &json!({ "audio_index": 2, "subtitle_index": 3 })).status, 204);
    let file = api.get(&format!("/api/items/{}", ids::PGS_MOVIE)).json()["files"][0].clone();
    assert_eq!(
        (file["selected_audio"].as_i64(), file["audio_picked"].as_bool(), file["selected_subtitle"].as_i64()),
        (Some(2), Some(true), Some(3))
    );
    assert_eq!(api.put(&streams, &json!({ "subtitle_index": 4 })).status, 204);
    let file = api.get(&format!("/api/items/{}", ids::PGS_MOVIE)).json()["files"][0].clone();
    assert_eq!(
        (file["selected_audio"].as_i64(), file["selected_subtitle"].as_i64()),
        (Some(2), Some(4)),
        "the audio stays"
    );
    assert_eq!(api.put(&streams, &json!({ "audio_index": null })).status, 204);
    assert_eq!(api.get(&format!("/api/items/{}", ids::PGS_MOVIE)).json()["files"][0]["audio_picked"], false);
}

#[test]
fn the_event_stream_sends_scans_and_ends_with_the_token() {
    let clock = ManualClock::new(1_793_400_000_000);
    let options = Options {
        clock: clock.clock(),
        keep_alive: Duration::from_millis(100),
        scan_every: Some(Duration::from_millis(100)),
        ..Options::default()
    };
    let server = FixtureServer::start_with(options).unwrap();
    let api = Api::alex(&server);
    let mut events = Events::open(&api);
    assert!(eventually(|| server.event_listeners() == 1));
    let started = events.wait_for("scan_started", Duration::from_secs(5)).expect("a scan");
    assert!(started["library_id"].is_i64());
    server.emit_raw(r#"{"type":"something_new","n":1}"#);
    assert_eq!(events.wait_for("something_new", Duration::from_secs(5)).unwrap()["n"], 1);
    server.emit(pley_fixtures::ServerEvent::LibrariesChanged);
    assert!(events.wait_for("libraries_changed", Duration::from_secs(5)).is_some());
    // The stream ends when its access token does.
    clock.advance(Duration::from_secs(3_601));
    assert!(eventually(|| server.event_listeners() == 0));
}

#[test]
fn network_memory_and_speed_test() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let local = api.get("/api/playback/network?client_id=tv").json();
    assert_eq!(local, json!({ "local": true, "ceiling_kbps": null, "source": null, "since": null, "probe": false }));
    server.set_network_remote(true);
    assert_eq!(api.get("/api/playback/network?client_id=tv").json()["probe"], true);
    let probed = api.put("/api/playback/network", &json!({ "client_id": "tv", "probe_kbps": 50_000.0 })).json();
    assert_eq!((probed["ceiling_kbps"].as_i64(), probed["source"].as_str()), (Some(35_000), Some("probe")));
    let ceiling = api.put("/api/playback/network", &json!({ "client_id": "tv", "ceiling_kbps": 8_000 })).json();
    assert_eq!((ceiling["ceiling_kbps"].as_i64(), ceiling["source"].as_str()), (Some(8_000), Some("playback")));
    assert_eq!(api.get("/api/playback/network").status, 400);
    let bytes = api.get("/api/playback/speedtest?bytes=100000");
    assert_eq!(bytes.header("Content-Type"), Some("application/octet-stream"));
    assert_eq!(bytes.body.len(), 100_000);
    server.set_network_routes(false);
    assert_eq!(api.get("/api/playback/network?client_id=tv").status, 404);
    assert_eq!(api.put("/api/playback/network", &json!({ "client_id": "tv", "ceiling_kbps": 1 })).status, 405);
}

#[test]
fn big_answers_are_gzipped_when_asked() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let plain = api.get("/api/libraries/1/items?limit=40");
    let zipped = api.get_with("/api/libraries/1/items?limit=40", &[("Accept-Encoding", "gzip")]);
    assert_eq!(zipped.header("Content-Encoding"), Some("gzip"));
    let mut text = String::new();
    std::io::Read::read_to_string(&mut flate2_reader(&zipped.body), &mut text).unwrap();
    assert_eq!(text.as_bytes(), &plain.body[..]);
}

fn flate2_reader(bytes: &[u8]) -> impl std::io::Read + '_ {
    flate2::read::GzDecoder::new(bytes)
}
