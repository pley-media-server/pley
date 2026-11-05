//! Casting against the fixture server: an event stream that names its device is a receiver while
//! it is open, listed for the profile's remotes, and a remote's cast is checked in the server's
//! order and sent to the profile's streams.

mod common;

use std::time::Duration;

use common::{Api, Events, eventually, send};
use pley_fixtures::{CastBody, FixtureServer, Options, ids};
use serde_json::{Value, json};

const TV: &str = "/api/events?receiver=den-tv&device_name=Den%20TV&device_kind=tv&casts=item,live";

fn server() -> FixtureServer {
    FixtureServer::start_with(Options { keep_alive: Duration::from_millis(200), ..Options::default() }).unwrap()
}

/// `api`'s remote, on a device of User-Agent `agent`, casting `body` to `receiver`.
fn cast(api: &Api, receiver: &str, body: Value, agent: &str) -> common::Reply {
    let auth = format!("Bearer {}", api.token);
    let headers = [("Authorization", auth.as_str()), ("Content-Type", "application/json"), ("User-Agent", agent)];
    let path = format!("/api/remote/devices/{receiver}/cast");
    send(&api.base, "POST", &path, &headers, body.to_string().as_bytes())
}

const PHONE: &str = "pleyAndroid/0.1.5 (Android 14; phone)";

#[test]
fn a_stream_naming_its_device_takes_casts_while_it_is_open() {
    let server = server();
    let api = Api::alex(&server);
    let mut remote = Events::open(&api);
    assert!(api.get("/api/remote/devices").json().as_array().unwrap().is_empty());

    let mut tv = Events::open_at(&api, TV);
    assert_eq!(tv.status, 200);
    assert!(remote.wait_for("sessions_changed", Duration::from_secs(5)).is_some(), "the remote never heard of it");
    let listed = api.get("/api/remote/devices").json();
    let device = listed[0]["device_id"].as_str().unwrap().to_string();
    assert!(device.starts_with("app-"), "{device}");
    assert_eq!(
        listed,
        json!([{ "receiver_id": "den-tv", "device_id": device, "name": "Den TV", "kind": "tv", "casts": ["item", "live"] }])
    );
    // A second stream of the same receiver counts once.
    let second = Events::open_at(&api, TV);
    assert_eq!(api.get("/api/remote/devices").json().as_array().unwrap().len(), 1);
    assert_eq!(server.receivers_of(ids::ALEX), listed.as_array().unwrap().clone());

    let sent = cast(&api, "den-tv", json!({ "item_id": ids::SAMPLE_1080P, "start_ms": 60_000 }), PHONE);
    assert_eq!(sent.status, 202);
    let heard = tv.wait_for("cast", Duration::from_secs(5)).expect("the TV hears it");
    assert_eq!(
        heard,
        json!({
            "type": "cast", "receiver_id": "den-tv", "item_id": ids::SAMPLE_1080P, "playlist_id": null,
            "channel_id": null, "start_ms": 60_000, "shuffle": false, "audio_index": null,
            "subtitle_index": null, "from": "Pley for Android",
        })
    );
    // A movie with the tracks picked on the remote's page.
    let tracks = json!({ "item_id": ids::SAMPLE_HDR10, "audio_index": 1, "subtitle_index": -1 });
    assert_eq!(cast(&api, "den-tv", tracks, PHONE).status, 202);
    let heard = tv.wait_for("cast", Duration::from_secs(5)).unwrap();
    assert_eq!((&heard["audio_index"], &heard["subtitle_index"]), (&json!(1), &json!(-1)));
    assert!(remote.wait_for("cast", Duration::from_secs(5)).is_some());
    // Every stream of the profile hears it: a receiver acts on its own only.
    assert!(remote.wait_for("cast", Duration::from_secs(5)).is_some());
    let shuffled = cast(&api, "den-tv", json!({ "playlist_id": ids::PLAYLIST_AUDIO, "shuffle": true }), PHONE);
    assert_eq!(shuffled.status, 202);
    let heard = tv.wait_for("cast", Duration::from_secs(5)).unwrap();
    assert_eq!((&heard["playlist_id"], &heard["shuffle"]), (&json!(ids::PLAYLIST_AUDIO), &json!(true)));

    // It goes with its last stream, and the remotes hear of it.
    drop(second);
    drop(tv);
    assert!(eventually(|| api.get("/api/remote/devices").json().as_array().is_some_and(|l| l.is_empty())));
    assert!(remote.wait_for("sessions_changed", Duration::from_secs(5)).is_some());
    assert_eq!(cast(&api, "den-tv", json!({ "item_id": ids::SAMPLE_1080P }), PHONE).status, 404);
}

#[test]
fn a_cast_is_checked_in_the_servers_order() {
    let server = server();
    let api = Api::alex(&server);
    let _tv = Events::open_at(&api, "/api/events?receiver=den-tv&casts=item");
    assert!(eventually(|| !server.receivers_of(ids::ALEX).is_empty()));
    // Named from the User-Agent when the stream gives no name; a TV, as the app is.
    let listed = &server.receivers_of(ids::ALEX)[0];
    assert_eq!((&listed["kind"], &listed["casts"]), (&json!("tv"), &json!(["item"])));
    let status = |receiver: &str, body: Value| cast(&api, receiver, body, PHONE).status;

    assert_eq!(status("kitchen-tv", json!({ "item_id": ids::SAMPLE_1080P })), 404, "a receiver that isn't there");
    assert_eq!(status("den-tv", json!({})), 400, "nothing named");
    assert_eq!(status("den-tv", json!({ "item_id": 1, "channel_id": 1 })), 400, "two named");
    assert_eq!(status("den-tv", json!({ "item_id": 999_999 })), 404, "a title that isn't there");
    assert_eq!(status("den-tv", json!({ "playlist_id": 999 })), 404, "a playlist that isn't Alex's");
    assert_eq!(status("den-tv", json!({ "channel_id": 999 })), 404, "a channel that isn't there");
    let refused = cast(&api, "den-tv", json!({ "channel_id": ids::LIVE_CHANNEL }), PHONE);
    assert_eq!(
        (refused.status, refused.json()["code"].as_str()),
        (409, Some("not_castable")),
        "Live TV it didn't name"
    );
    assert_eq!(status("den-tv", json!({ "item_id": ids::SHOW, "start_ms": -1 })), 400);
    assert_eq!(status("den-tv", json!({ "item_id": ids::SHOW, "start_ms": 86_400_001 })), 400);
    // Tracks: a movie's or an episode's, of one of its files.
    assert_eq!(status("den-tv", json!({ "item_id": ids::SAMPLE_HDR10, "audio_index": 9 })), 400, "no such audio");
    assert_eq!(status("den-tv", json!({ "item_id": ids::SAMPLE_HDR10, "subtitle_index": 1 })), 400, "not subtitles");
    assert_eq!(status("den-tv", json!({ "item_id": ids::SHOW, "audio_index": 1 })), 400, "a show has no tracks");
    // Ten a second to one receiver at most.
    let sent: Vec<u16> = (0..11).map(|_| status("den-tv", json!({ "item_id": ids::ALBUM }))).collect();
    assert_eq!(sent[..10], [202; 10]);
    assert_eq!(sent[10], 429);

    // Someone else's receiver is as unknown as one that isn't there; Live TV isn't open to Sam.
    let sam = Api::new(&server, ids::SAM);
    assert_eq!(cast(&sam, "den-tv", json!({ "item_id": ids::SAMPLE_1080P }), PHONE).status, 404);
    let _sams = Events::open_at(&sam, "/api/events?receiver=sams-tv&casts=item,live");
    assert!(eventually(|| !server.receivers_of(ids::SAM).is_empty()));
    assert_eq!(cast(&sam, "sams-tv", json!({ "channel_id": ids::LIVE_CHANNEL }), PHONE).status, 403);
    assert_eq!(server.cast(ids::SAM, "sams-tv", &CastBody::item(ids::SAMPLE_1080P), None), 202);
}

#[test]
fn a_stream_with_a_receiver_it_cant_be_still_gets_its_events() {
    let server = server();
    let api = Api::alex(&server);
    let mut odd = Events::open_at(&api, "/api/events?receiver=den%20tv&casts=item");
    assert_eq!(odd.status, 200);
    assert!(server.receivers_of(ids::ALEX).is_empty());
    server.emit(pley_fixtures::ServerEvent::WatchlistChanged);
    assert!(odd.wait_for("watchlist_changed", Duration::from_secs(5)).is_some());
}
