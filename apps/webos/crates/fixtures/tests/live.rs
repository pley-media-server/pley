//! Live TV: the lineup, the guide, watching at keyed addresses, plans,
//! leaving, favourites and the busy connection, against the fixture server.

mod common;

use std::time::Duration;

use common::{Api, Events, eventually, send};
use pley_fixtures::{FixtureServer, ManualClock, Options, ids};
use serde_json::{Value, json};

fn watch(api: &Api, channel: i64, body: Value) -> common::Reply {
    api.post(&format!("/api/live/channels/{channel}/watch"), &body)
}

#[test]
fn the_lineup_and_its_guide() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let summary = api.get("/api/live").json();
    assert_eq!(summary["available"], true);
    assert_eq!(summary["channels"], ids::LIVE_CHANNELS);
    assert_eq!(summary["recent"][0], ids::LIVE_CHANNEL);

    let groups = api.get("/api/live/groups").json();
    assert_eq!(groups["total"], ids::LIVE_CHANNELS);
    assert_eq!(groups["favorites"], 4);
    assert_eq!(groups["groups"][1], json!({ "name": "UK| Sport", "region": "UK", "label": "Sport", "count": 8 }));
    assert!(groups["groups"][4]["region"].is_null());

    let page = api.get("/api/live/browse?limit=10").json();
    assert_eq!((page["total"].as_i64(), page["offset"].as_i64()), (Some(ids::LIVE_CHANNELS), Some(0)));
    let first = &page["channels"][0];
    assert_eq!(first["id"], ids::LIVE_CHANNEL);
    assert_eq!(first["number"], "1");
    assert_eq!(first["group"], "UK| News");
    assert_eq!(first["favorite"], true);
    assert!(first["logo"].as_str().unwrap().starts_with("/api/live/channels/1/logo?v="));
    assert!(first["now"]["start"].as_i64().unwrap() <= first["now"]["stop"].as_i64().unwrap());
    assert_eq!(first["next"]["start"], first["now"]["stop"]);
    let around = api.get(&format!("/api/live/browse?limit=10&around={}", ids::LIVE_BUSY_CHANNEL)).json();
    assert_eq!(around["offset"], 20);
    let sport = api.get("/api/live/browse?category=group&group=UK%7C%20Sport").json();
    assert_eq!(sport["total"], 8);
    assert_eq!(api.get("/api/live/browse?category=favorites").json()["total"], 4);
    assert_eq!(api.get("/api/live/browse?category=recent").json()["channels"][0]["id"], ids::LIVE_CHANNEL);
    assert_eq!(api.get("/api/live/browse?category=group").status, 400);
    assert_eq!(api.get("/api/live/browse?category=nothing").status, 400);
    let searched = api.get("/api/live/browse?q=news").json();
    assert_eq!(searched["total"], 6);

    let no_guide = api.get(&format!("/api/live/channels/{}", ids::LIVE_NO_GUIDE_CHANNEL)).json();
    assert_eq!(no_guide["has_guide"], false);
    assert!(no_guide["now"].is_null());
    assert!(api.get(&format!("/api/live/channels/{}", ids::LIVE_CHANNELS)).json()["number"].is_null());
    assert_eq!(api.get("/api/live/channels").json().as_array().unwrap().len() as i64, ids::LIVE_CHANNELS);

    let from = 1_793_400_000;
    let guide = api
        .get(&format!("/api/live/guide?from={from}&to={}&ids=1,2,{}", from + 7_200, ids::LIVE_NO_GUIDE_CHANNEL))
        .json();
    assert_eq!((guide["from"].as_i64(), guide["to"].as_i64()), (Some(from), Some(from + 7_200)));
    let programs = guide["programs"].as_object().unwrap();
    assert_eq!(programs.len(), 3);
    let one = programs["1"].as_array().unwrap();
    assert!(one[0]["start"].as_i64().unwrap() <= from && one.last().unwrap()["stop"].as_i64().unwrap() >= from + 7_200);
    assert!(one[0]["title"].is_string() && one[0]["categories"].is_array() && one[0]["is_live"].is_boolean());
    assert_eq!(programs[&ids::LIVE_NO_GUIDE_CHANNEL.to_string()], json!([]));
    assert_eq!(api.get(&format!("/api/live/guide?from={from}&to={from}")).status, 400);
    assert_eq!(api.get(&format!("/api/live/guide?from={from}&to={}", from + 3 * 86_400)).status, 400);
    assert_eq!(api.get("/api/live/guide?ids=a").status, 400);
    assert_eq!(api.get("/api/live/guide").json()["programs"].as_object().unwrap().len() as i64, ids::LIVE_CHANNELS);

    let logo = api.get("/api/live/channels/1/logo?w=200");
    assert_eq!(logo.header("Content-Type"), Some("image/png"));
    assert_eq!(api.get("/api/live/channels/7/logo").status, 404, "every 7th channel has no logo");
    let picture = one.iter().find_map(|p| p["image"].as_str());
    assert!(picture.is_none(), "news has no pictures");
    let movies = api.get(&format!("/api/live/channels/{}", ids::LIVE_CONFLICT_CHANNEL)).json();
    let image = movies["now"]["image"].as_str().unwrap();
    assert_eq!(api.get(&format!("{image}?w=320")).header("Content-Type"), Some("image/jpeg"));

    assert_eq!(api.put("/api/live/channels/2/favorite", &json!({})).status, 204);
    assert_eq!(api.get("/api/live/groups").json()["favorites"], 5);
    assert_eq!(api.delete("/api/live/channels/2/favorite").status, 204);
}

#[test]
fn watching_at_a_keyed_address() {
    let clock = ManualClock::new(1_793_400_000_000);
    let server =
        FixtureServer::start_with(Options { clock: clock.clock(), scan_every: None, ..Options::default() }).unwrap();
    let api = Api::alex(&server);
    let watching = watch(&api, 9, json!({ "client_id": "tv", "keyed_urls": true }));
    assert_eq!(watching.status, 200, "{}", watching.text());
    let watching = watching.json();
    assert_eq!(watching["kind"], "hls");
    assert_eq!(watching["channel"]["id"], 9);
    let session = watching["session_id"].as_str().unwrap().to_string();
    let url = watching["url"].as_str().unwrap().to_string();
    assert!(url.starts_with("/api/live/media/pley_mk_") && url.ends_with("/index.m3u8"));

    let playlist = send(server.url(), "GET", &url, &[], b"").text();
    assert!(playlist.contains("#EXT-X-VERSION:6") && playlist.contains("#EXT-X-MEDIA-SEQUENCE:0"));
    assert!(playlist.contains("s0.ts") && playlist.contains("s2.ts") && !playlist.contains("ENDLIST"));
    let base = url.trim_end_matches("index.m3u8");
    let segment = send(server.url(), "GET", &format!("{base}s1.ts"), &[], b"");
    assert_eq!(segment.header("Content-Type"), Some("video/mp2t"));
    assert!(!segment.body.is_empty());
    assert_eq!(send(server.url(), "GET", &format!("{base}s9.ts"), &[], b"").status, 404, "not made yet");

    // The window slides; each wrap of the sample is a discontinuity.
    clock.advance(Duration::from_secs(20));
    let later = send(server.url(), "GET", &url, &[], b"").text();
    assert!(later.contains("#EXT-X-MEDIA-SEQUENCE:3") && later.contains("s7.ts"));
    assert!(later.contains("#EXT-X-DISCONTINUITY\n#EXTINF:4.000,\ns5.ts"));
    assert_eq!(send(server.url(), "GET", &format!("{base}s7.ts"), &[], b"").body, segment_bytes(&server, &api, 2));

    let plan = api.get(&format!("/api/live/sessions/{session}/plan")).json();
    assert_eq!((plan["video"].as_str(), plan["audio_codec"].as_str()), (Some("copy"), Some("aac")));
    assert_eq!(api.get(&format!("/api/live/sessions/{session}/index.m3u8")).status, 200);
    assert!(api.get("/api/live").json()["recent"].as_array().unwrap().contains(&json!(9)));
    assert_eq!(api.delete(&format!("/api/live/sessions/{session}")).status, 204);
    assert_eq!(api.delete(&format!("/api/live/sessions/{session}")).status, 404);
    assert_eq!(send(server.url(), "GET", &url, &[], b"").status, 404);

    let bearer = watch(&api, 2, json!({})).json();
    assert!(bearer["url"].as_str().unwrap().starts_with("/api/live/sessions/"));
    assert_eq!(watch(&api, 999, json!({})).status, 404);
}

fn segment_bytes(server: &FixtureServer, api: &Api, n: usize) -> Vec<u8> {
    let other = watch(api, 3, json!({ "keyed_urls": true })).json();
    let base = other["url"].as_str().unwrap().trim_end_matches("index.m3u8").to_string();
    send(server.url(), "GET", &format!("{base}s{n}.ts"), &[], b"").body
}

#[test]
fn a_busy_connection_answers_409_until_taken_over() {
    let server = FixtureServer::start();
    let alex = Api::alex(&server);
    let busy = watch(&alex, ids::LIVE_CONFLICT_CHANNEL, json!({ "client_id": "tv" }));
    assert_eq!(busy.status, 409);
    let busy = busy.json();
    assert_eq!(busy["code"], "conflict");
    assert!(busy["message"].as_str().unwrap().contains("allows one connection at once"));
    let connection = &busy["connections"][0];
    assert_eq!(connection["channel_id"], ids::LIVE_BUSY_CHANNEL);
    assert_eq!((connection["yours"].as_bool(), connection["can_take_over"].as_bool()), (Some(true), Some(true)));
    assert_eq!(connection["watchers"], json!([]));

    // Watching the busy channel itself shares the connection.
    let shared = watch(&alex, ids::LIVE_BUSY_CHANNEL, json!({ "client_id": "phone" }));
    assert_eq!(shared.status, 200);
    let shared_session = shared.json()["session_id"].as_str().unwrap().to_string();

    let mut events = Events::open(&alex);
    assert!(eventually(|| server.event_listeners() == 1));
    let taken =
        watch(&alex, ids::LIVE_CONFLICT_CHANNEL, json!({ "client_id": "tv", "take_over": ids::LIVE_BUSY_CHANNEL }));
    assert_eq!(taken.status, 200, "{}", taken.text());
    // Each viewer it ended hears why, by its channel.
    let told = events.wait_for("live_taken_over", Duration::from_secs(5)).expect("the viewers hear it");
    assert_eq!(told["channel_id"], ids::LIVE_BUSY_CHANNEL);
    assert!(told["message"].as_str().unwrap().starts_with("Watching moved to"));
    let moved = alex.get(&format!("/api/live/sessions/{shared_session}/index.m3u8"));
    assert_eq!(moved.status, 410);
    assert_eq!(moved.json()["message"], told["message"]);

    // Now Robin finds Alex's channel there, and may not take it.
    let robin = Api::new(&server, ids::ROBIN);
    let refused = watch(&robin, ids::LIVE_BUSY_CHANNEL, json!({}));
    assert_eq!(refused.status, 409);
    assert_eq!(refused.json()["connections"][0]["yours"], false);
    let not_yours = watch(&robin, ids::LIVE_BUSY_CHANNEL, json!({ "take_over": ids::LIVE_CONFLICT_CHANNEL }));
    assert_eq!(not_yours.status, 403);
    // The same device moving to another channel of the source frees its own first.
    assert_eq!(watch(&alex, ids::LIVE_BUSY_CHANNEL, json!({ "client_id": "tv" })).status, 200);
}

#[test]
fn a_viewer_obeys_the_remote_it_said_and_is_stopped_from_elsewhere() {
    let server =
        FixtureServer::start_with(Options { keep_alive: Duration::from_millis(200), ..Options::default() }).unwrap();
    let api = Api::alex(&server);
    let mut events = Events::open(&api);
    let body = json!({ "client_id": "tv", "keyed_urls": true, "controls": ["next", "previous"], "device_kind": "tv" });
    let watching = watch(&api, 9, body).json();
    let session = watching["session_id"].as_str().unwrap().to_string();
    let url = watching["url"].as_str().unwrap().to_string();
    let listed = api.get("/api/sessions").json();
    let viewer = listed["sessions"].as_array().unwrap().iter().find(|s| s["session_id"] == session).unwrap().clone();
    assert_eq!(
        (&viewer["kind"], &viewer["channel_id"], &viewer["controls"], &viewer["device_kind"]),
        (&json!("live"), &json!(9), &json!(["next", "previous"]), &json!("tv"))
    );

    let path = format!("/api/sessions/{session}/command");
    assert_eq!(api.post(&path, &json!({ "command": "next" })).status, 202);
    let heard = events.wait_for("player_command", Duration::from_secs(5)).expect("the viewer hears it");
    assert_eq!((heard["session_id"].as_str(), heard["command"].as_str()), (Some(session.as_str()), Some("next")));
    assert_eq!(api.post(&path, &json!({ "command": "pause" })).status, 409);

    let stop = api.json("DELETE", &format!("/api/sessions/{session}"), &json!({ "message": "Time for bed" }));
    assert_eq!(stop.status, 204);
    let stopped = events.wait_for("session_stopped", Duration::from_secs(5)).expect("the viewer hears it");
    assert_eq!((stopped["message"].as_str(), stopped["by_admin"].as_bool()), (Some("Time for bed"), Some(false)));
    let gone = send(server.url(), "GET", &url, &[], b"");
    assert_eq!((gone.status, gone.json()["message"].as_str()), (410, Some("Time for bed")));
    assert_eq!(api.post(&path, &json!({ "command": "next" })).status, 404);
}

#[test]
fn live_tv_isnt_open_to_sam() {
    let server = FixtureServer::start();
    let sam = Api::new(&server, ids::SAM);
    let summary = sam.get("/api/live").json();
    assert_eq!((summary["available"].as_bool(), summary["channels"].as_i64()), (Some(false), Some(0)));
    let refused = sam.get("/api/live/channels");
    assert_eq!(refused.status, 403);
    assert_eq!(refused.json()["message"], "Live TV isn't open to this account");
    assert_eq!(watch(&sam, ids::LIVE_CHANNEL, json!({})).status, 403);
    assert_eq!(sam.get("/api/search?q=news").json()["channels"], json!([]));
}
