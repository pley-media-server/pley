//! Casting to Google Cast devices: listed for the household, a cast played
//! on the device as a session of the caster's, its place kept and the
//! remotes' commands passed on. The device is a fake that speaks Cast over
//! TLS on this computer.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use axum::http::StatusCode;
use common::TestApp;
use pley::cast::frame::{self, Message};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::rustls;

const LENGTH_MS: i64 = 7_200_000;

/// What the fake device was sent: (destination, payload).
type Sent = mpsc::UnboundedReceiver<(String, Value)>;
/// What the test makes it say, as Google's player: a MEDIA_STATUS.
type Say = mpsc::UnboundedSender<Value>;

/// A Cast device on this computer, with a certificate of its own making as
/// real ones have.
async fn fake_device() -> (SocketAddr, Sent, Say, Fake) {
    let made = rcgen::generate_simple_self_signed(vec!["cast".to_owned()]).unwrap();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(made.signing_key.serialize_der().into());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![made.cert.der().clone()], key)
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sent_tx, sent) = mpsc::unbounded_channel();
    let (say, said) = mpsc::unbounded_channel::<Value>();
    let said = Arc::new(tokio::sync::Mutex::new(said));
    let fake = Fake::default();
    let knobs = fake.clone();
    tokio::spawn(async move {
        // Each connection as it comes: a new cast connects again.
        while let Ok((tcp, _)) = listener.accept().await {
            let (acceptor, sent_tx, said, fake) = (acceptor.clone(), sent_tx.clone(), said.clone(), knobs.clone());
            tokio::spawn(async move { serve(acceptor.accept(tcp).await.unwrap(), sent_tx, said, fake).await });
        }
    });
    (address, sent, say, fake)
}

/// How the fake device behaves, as the test sets it.
#[derive(Default)]
struct Knobs {
    /// How many loads it refuses next, as a device does a stream it can't
    /// decode.
    refuse: AtomicUsize,
    /// It answers no load.
    silent_load: AtomicBool,
    /// It answers no launch.
    silent_launch: AtomicBool,
    /// Its next answer to what runs comes this many milliseconds late.
    slow_status_ms: AtomicU64,
    /// Its next answer to what runs names another sender's session of
    /// Google's player.
    other_session: AtomicBool,
}
type Fake = Arc<Knobs>;

/// A Cast device's setup API on this computer, answering `answer` to
/// whatever it is asked.
async fn setup_api(answer: Value) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            let body = answer.to_string();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut request = [0; 4096];
                let _ = tcp.read(&mut request).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = tcp.write_all(head.as_bytes()).await;
                let _ = tcp.write_all(body.as_bytes()).await;
            });
        }
    });
    address
}

/// One connection to the fake device: Google's player, as far as Pley
/// asks it things.
async fn serve(
    stream: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    sent_tx: mpsc::UnboundedSender<(String, Value)>,
    said: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<Value>>>,
    fake: Fake,
) {
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let reply = |source: &str, namespace: &str, payload: Value| Message {
            source: source.into(),
            destination: "sender-pley".into(),
            namespace: format!("urn:x-cast:com.google.cast.{namespace}"),
            payload: payload.to_string(),
        };
        let mut at = 0.0;
        let mut volume = 0.5;
        let mut muted = false;
        // What the device says of itself: Google's player running, and its
        // volume.
        let receiver = |id: &Value, volume: f64, muted: bool, session: &str| {
            json!({"type": "RECEIVER_STATUS", "requestId": id, "status": {
                "applications": [{"appId": "CC1AD845", "transportId": "web-5", "sessionId": session}],
                "volume": {"level": volume, "muted": muted},
            }})
        };
        loop {
            let message = tokio::select! {
                read = frame::read(&mut reader) => match read {
                    Ok(Some(message)) => message,
                    _ => return,
                },
                Some(status) = async { said.lock().await.recv().await } => {
                    frame::write(&mut writer, &reply("web-5", "media", status)).await.unwrap();
                    continue;
                }
            };
            if message.namespace.ends_with("heartbeat") {
                continue;
            }
            let payload: Value = serde_json::from_str(&message.payload).unwrap();
            let _ = sent_tx.send((message.destination.clone(), payload.clone()));
            let id = payload["requestId"].clone();
            let answer = match payload["type"].as_str() {
                Some("LAUNCH") if fake.silent_launch.load(Ordering::SeqCst) => None,
                Some("LAUNCH") => Some(reply("receiver-0", "receiver", receiver(&id, volume, muted, "s-1"))),
                Some("GET_STATUS") if message.destination == "receiver-0" => {
                    let late = fake.slow_status_ms.swap(0, Ordering::SeqCst);
                    if late > 0 {
                        tokio::time::sleep(Duration::from_millis(late)).await;
                    }
                    let other = fake.other_session.swap(false, Ordering::SeqCst);
                    let session = if other { "s-other" } else { "s-1" };
                    Some(reply("receiver-0", "receiver", receiver(&id, volume, muted, session)))
                }
                // Either the level or the mute, as Google's senders send them.
                Some("SET_VOLUME") => {
                    assert!(payload["volume"].as_object().unwrap().len() == 1, "{payload}");
                    volume = payload["volume"]["level"].as_f64().unwrap_or(volume);
                    muted = payload["volume"]["muted"].as_bool().unwrap_or(muted);
                    Some(reply("receiver-0", "receiver", receiver(&id, volume, muted, "s-1")))
                }
                Some("LOAD") if fake.silent_load.load(Ordering::SeqCst) => None,
                Some("LOAD")
                    if fake.refuse.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() =>
                {
                    Some(reply("web-5", "media", json!({"type": "LOAD_FAILED", "requestId": id})))
                }
                Some("LOAD") => {
                    // A live stream names no place: it starts at its edge.
                    at = payload["currentTime"].as_f64().unwrap_or(0.0);
                    Some(reply("web-5", "media", status(&id, "PLAYING", at)))
                }
                Some("GET_STATUS") if message.destination == "web-5" => {
                    Some(reply("web-5", "media", status(&id, "PLAYING", at)))
                }
                Some("PAUSE") => Some(reply("web-5", "media", status(&id, "PAUSED", at))),
                _ => None,
            };
            if let Some(answer) = answer
                && frame::write(&mut writer, &answer).await.is_err()
            {
                return;
            }
        }
    }
}

fn status(id: &Value, state: &str, at: f64) -> Value {
    json!({"type": "MEDIA_STATUS", "requestId": id, "status": [
        {"mediaSessionId": 7, "playerState": state, "currentTime": at}
    ]})
}

/// The next message of `type` the device was sent, others skipped.
async fn next(sent: &mut Sent, kind: &str) -> (String, Value) {
    let wait = async {
        loop {
            let (to, payload) = sent.recv().await.expect("the connection is open");
            if payload["type"] == kind {
                return (to, payload);
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(15), wait).await.unwrap_or_else(|_| panic!("no {kind}"))
}

struct World {
    app: TestApp,
    admin: String,
    film: i64,
    sent: Sent,
    say: Say,
    fake: Fake,
}

async fn world() -> World {
    world_with(None).await
}

/// The world, with the device's setup API at `setup`.
async fn world_with(setup: Option<SocketAddr>) -> World {
    world_where(|config| config.cast_fixed.setup = setup).await
}

/// The world, where a cast has `within` to start.
async fn world_within(within: Duration) -> World {
    world_where(|config| config.cast_within = within).await
}

/// The world, with what `adjust` changes of the server's config.
async fn world_where(adjust: impl FnOnce(&mut pley::config::Config)) -> World {
    let (address, sent, say, fake) = fake_device().await;
    let app = TestApp::with_config(|config| {
        config.cast_fixed.device = Some(address);
        config.cast_fixed.server = Some("http://pley.test:7539".into());
        config.cast_fixed.setup = None;
        adjust(config);
    })
    .await;
    let admin = app.setup_admin().await;
    // Off until the administrator turns it on.
    let on = app.put("/api/settings", &admin, json!({"cast_devices": true})).await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text());
    app.media_file("movies/Film (2001)/Film (2001).mp4", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    sqlx::query("UPDATE items SET duration_ms = ?").bind(LENGTH_MS).execute(&app.state.db.write).await.unwrap();
    // H.264 and AAC in MP4, as a Cast device plays it, with English text
    // subtitles.
    film_in(&app, film, "h264").await;
    World { app, admin, film, sent, say, fake }
}

/// Makes `film` MP4 with `video` and AAC, and English text subtitles:
/// played as it is in H.264, transcoded in HEVC.
async fn film_in(app: &TestApp, film: i64, video: &str) {
    let info = json!({
        "container": "mov,mp4,m4a,3gp,3g2,mj2",
        "duration_ms": LENGTH_MS,
        "streams": [
            {"index": 0, "kind": "video", "codec": video, "width": 1920, "height": 1080, "pix_fmt": "yuv420p"},
            {"index": 1, "kind": "audio", "codec": "aac", "channels": 2, "language": "eng", "default": true},
            {"index": 2, "kind": "subtitle", "codec": "mov_text", "language": "eng", "text_based": true},
        ],
    });
    sqlx::query(
        "UPDATE media_files SET media_info = ?, container = 'mp4', video_codec = ?, audio_codec = 'aac' WHERE id = ?",
    )
    .bind(info.to_string())
    .bind(video)
    .bind(app.file_of(film).await)
    .execute(&app.state.db.write)
    .await
    .unwrap();
}

async fn sessions(w: &World) -> Vec<Value> {
    w.app.get("/api/sessions", &w.admin).await.body.as_array().unwrap().clone()
}

#[tokio::test]
async fn home_members_see_cast_devices_and_others_dont() {
    let w = world().await;
    // Only remotes that ask for them, which know what they are.
    assert!(!lists_cast(&w, &w.admin, "/api/remote/devices").await);
    let devices = w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    assert_eq!(devices.status, StatusCode::OK);
    let cast = devices.body.as_array().unwrap().iter().find(|d| d["receiver_id"] == "cast:dev").cloned();
    let cast = cast.expect("the Cast device is listed");
    assert_eq!((cast["device_id"].clone(), cast["protocol"].clone()), (json!("cast:dev"), json!("cast")));
    assert_eq!((cast["kind"].clone(), cast["casts"].clone()), (json!("tv"), json!(["item", "live"])));

    let (friend, _) = w.app.user(&w.admin, "friend", json!({"home": false})).await;
    let theirs = w.app.get("/api/remote/devices?nearby=1", &friend).await;
    assert!(theirs.body.as_array().unwrap().iter().all(|d| d["protocol"].is_null()), "{}", theirs.text());
    let refused = w.app.post("/api/remote/devices/cast:dev/cast", &friend, json!({"item_id": w.film})).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    // A device no look found is as unknown.
    let unknown = w.app.post("/api/remote/devices/cast:elsewhere/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn devices_are_listed_and_cast_to_only_while_the_administrator_has_them_on() {
    let w = world().await;
    assert!(lists_cast(&w, &w.admin, "/api/remote/devices?nearby=1").await);
    let off = w.app.put("/api/settings", &w.admin, json!({"cast_devices": false})).await;
    assert_eq!((off.status, off.body["cast_devices"].clone()), (StatusCode::OK, json!(false)));
    // Off: the device the server knew of is gone from the lists, and as
    // unknown to a remote that listed it before.
    assert!(!lists_cast(&w, &w.admin, "/api/remote/devices?nearby=1").await);
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NOT_FOUND, "{}", cast.text());
    let pairing = w.app.post("/api/remote/devices/airplay:a1b2c3d4e5f6/pairing", &w.admin, json!({})).await;
    assert_eq!(pairing.status, StatusCode::NOT_FOUND, "{}", pairing.text());
    // Only an administrator turns it on.
    let (member, _) = w.app.user(&w.admin, "member", json!({"home": true})).await;
    let refused = w.app.put("/api/settings", &member, json!({"cast_devices": true})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let on = w.app.put("/api/settings", &w.admin, json!({"cast_devices": true})).await;
    assert_eq!(on.status, StatusCode::OK);
    assert!(lists_cast(&w, &member, "/api/remote/devices?nearby=1").await);
}

#[tokio::test]
async fn a_new_server_casts_to_no_device() {
    let (address, ..) = fake_device().await;
    let app = TestApp::with_config(|config| {
        config.cast_fixed.device = Some(address);
        config.cast_fixed.server = Some("http://pley.test:7539".into());
        config.cast_fixed.setup = None;
    })
    .await;
    let admin = app.setup_admin().await;
    assert_eq!(app.get("/api/settings", &admin).await.body["cast_devices"], json!(false));
    let devices = app.get("/api/remote/devices?nearby=1", &admin).await;
    assert!(devices.body.as_array().unwrap().iter().all(|d| d["protocol"].is_null()), "{}", devices.text());
}

/// An event stream of `token`'s, as receiver `query` describes, connected
/// from `peer`: the receiver lasts while it is kept.
async fn receiver(w: &World, token: &str, query: &str, peer: &str) -> axum::body::Body {
    let request = axum::http::Request::builder()
        .uri(format!("/api/events?{query}"))
        .header("authorization", format!("Bearer {token}"))
        .extension(axum::extract::ConnectInfo(peer.parse::<SocketAddr>().unwrap()))
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(w.app.api.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body()
}

/// Whether `token`'s device list, at `uri`, has the Cast device.
async fn lists_cast(w: &World, token: &str, uri: &str) -> bool {
    let devices = w.app.get(uri, token).await;
    devices.body.as_array().unwrap().iter().any(|d| d["receiver_id"] == "cast:dev")
}

#[tokio::test]
async fn a_pley_app_on_the_tv_stands_for_its_cast_device() {
    let w = world().await;
    let mine = "/api/remote/devices?nearby=1";
    let household = "/api/remote/devices?household=1&nearby=1";
    // The fake device is at 127.0.0.1: an app there that takes titles is
    // the same TV.
    let elsewhere = receiver(&w, &w.admin, "receiver=phone&casts=item", "10.0.0.9:50000").await;
    assert!(lists_cast(&w, &w.admin, mine).await);
    let live_only = receiver(&w, &w.admin, "receiver=box&casts=live", "127.0.0.1:50001").await;
    assert!(lists_cast(&w, &w.admin, mine).await, "an app that takes no titles leaves it");
    let tv = receiver(&w, &w.admin, "receiver=tv&casts=item", "127.0.0.1:50002").await;
    assert!(!lists_cast(&w, &w.admin, mine).await);
    drop((tv, live_only, elsewhere));

    // On another profile, the app stands for it only where the caller
    // can cast to it: a TV that switches, in the household's list.
    let (kid, _) = w.app.user(&w.admin, "kid", json!({"home": true})).await;
    let theirs = receiver(&w, &kid, "receiver=tv&casts=item", "127.0.0.1:50003").await;
    assert!(lists_cast(&w, &w.admin, household).await, "a TV that doesn't switch leaves it");
    drop(theirs);
    let switching = receiver(&w, &kid, "receiver=tv&casts=item,switch", "127.0.0.1:50004").await;
    assert!(lists_cast(&w, &w.admin, mine).await);
    assert!(!lists_cast(&w, &w.admin, household).await);
    drop(switching);
}

#[tokio::test]
async fn devices_that_cant_reach_pley_arent_listed() {
    // Pley's web port listens on no address the device reaches, as when it
    // listens on this computer alone, behind a reverse proxy.
    let w = world_where(|config| config.cast_fixed.server = None).await;
    assert!(!lists_cast(&w, &w.admin, "/api/remote/devices?nearby=1").await);
    // A remote that listed it before is told why it can't cast there.
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!((cast.status, cast.body["code"].as_str()), (StatusCode::CONFLICT, Some("cast_unreachable")));
    // At once, without looking for the device: a device no look would find
    // is told the same, and a pairing finds none.
    let body = json!({"item_id": w.film});
    let elsewhere = w.app.post("/api/remote/devices/cast:elsewhere/cast", &w.admin, body).await;
    assert_eq!((elsewhere.status, elsewhere.body["code"].as_str()), (StatusCode::CONFLICT, Some("cast_unreachable")));
    let pairing = w.app.post("/api/remote/devices/cast:dev/pairing", &w.admin, json!({})).await;
    assert_eq!(pairing.status, StatusCode::NOT_FOUND, "{}", pairing.text());
    // Someone who may not reach the devices learns nothing more.
    let (friend, _) = w.app.user(&w.admin, "friend", json!({"home": false})).await;
    let refused = w.app.post("/api/remote/devices/cast:dev/cast", &friend, json!({"item_id": w.film})).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_airplay_devices_pair_and_only_administrators_list_pairings() {
    let w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/pairing", &w.admin, json!({"pin": "1234"})).await;
    assert_eq!((cast.status, cast.body["code"].clone()), (StatusCode::CONFLICT, json!("not_pairable")));
    let unknown = w.app.post("/api/remote/devices/airplay:a1b2c3d4e5f6/pairing", &w.admin, json!({})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);

    let listed = w.app.get("/api/network/airplay", &w.admin).await;
    assert_eq!((listed.status, listed.body.clone()), (StatusCode::OK, json!([])));
    let (member, _) = w.app.user(&w.admin, "member", json!({"home": true})).await;
    assert_eq!(w.app.get("/api/network/airplay", &member).await.status, StatusCode::FORBIDDEN);
    assert_eq!(w.app.delete("/api/network/airplay/a1b2c3d4e5f6", &member).await.status, StatusCode::FORBIDDEN);
    assert_eq!(w.app.delete("/api/network/airplay/a1b2c3d4e5f6", &w.admin).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_movies_episodes_and_channels_are_cast() {
    let w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let playlist = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"playlist_id": 1})).await;
    assert_eq!((playlist.status, playlist.body["code"].clone()), (StatusCode::CONFLICT, json!("not_castable")));
    let track = w
        .app
        .post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film, "subtitle_index": 9}))
        .await;
    assert_eq!(track.status, StatusCode::BAD_REQUEST, "a track the file hasn't");
}

#[tokio::test]
async fn a_cast_plays_on_the_device_as_a_session_the_remotes_control() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let body = json!({"item_id": w.film, "start_ms": 60_000, "subtitle_index": 2});
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());

    // Google's player, started and given the keyed stream and subtitles.
    let (to, launch) = next(&mut w.sent, "LAUNCH").await;
    assert_eq!((to.as_str(), launch["appId"].clone()), ("receiver-0", json!("CC1AD845")));
    let (to, load) = next(&mut w.sent, "LOAD").await;
    assert_eq!(to, "web-5");
    let url = load["media"]["contentId"].as_str().unwrap();
    assert!(url.starts_with("http://pley.test:7539/api/media/pley_mk_") && url.ends_with("/file.mp4"), "{url}");
    assert_eq!(load["media"]["contentType"], "video/mp4");
    assert_eq!(load["media"]["metadata"]["title"], "Film");
    assert_eq!(load["currentTime"], 60.0);
    let key = url.trim_start_matches("http://pley.test:7539").trim_end_matches("/file.mp4");
    assert_eq!(load["media"]["tracks"][0]["trackContentId"], format!("http://pley.test:7539{key}/subtitles-2.vtt"));
    assert_eq!(load["activeTrackIds"], json!([1]));
    // The device fetches it with the key, and no sign-in.
    let file = w.app.call(axum::http::Method::GET, &format!("{key}/file.mp4"), None, None, &[]).await;
    assert_eq!(file.status, StatusCode::OK);

    // The remotes see it on the device, with the controls it obeys.
    let listed = sessions(&w).await;
    assert_eq!(listed.len(), 1);
    let session = &listed[0];
    assert_eq!((session["device_id"].clone(), session["device"].clone()), (json!("cast:dev"), json!("Cast emulator")));
    assert_eq!((session["device_kind"].clone(), session["subtitle_index"].clone()), (json!("tv"), json!(2)));
    assert_eq!(session["controls"], json!(["play", "pause", "seek", "skip", "volume"]));
    let id = session["session_id"].as_str().unwrap().to_owned();
    // The dashboard says what plays it.
    let activity = w.app.get("/api/activity", &w.admin).await;
    assert_eq!(activity.body["sessions"][0]["player"], json!("cast"));

    // Its commands go to the device.
    let paused = w.app.post(&format!("/api/sessions/{id}/command"), &w.admin, json!({"command": "pause"})).await;
    assert_eq!(paused.status, StatusCode::ACCEPTED);
    let (_, pause) = next(&mut w.sent, "PAUSE").await;
    assert_eq!(pause["mediaSessionId"], 7);
    let seek = json!({"command": "seek", "position_ms": 1_800_000});
    w.app.post(&format!("/api/sessions/{id}/command"), &w.admin, seek).await;
    assert_eq!(next(&mut w.sent, "SEEK").await.1["currentTime"], 1800.0);

    // What it plays is the session's, and its place is kept.
    w.say.send(status(&json!(0), "PLAYING", 1810.0)).unwrap();
    let place = async {
        loop {
            let item = w.app.get(&format!("/api/items/{}", w.film), &w.admin).await;
            if item.body["user_state"]["position_ms"] == 1_810_000 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), place).await.expect("the place is kept");
    assert_eq!(sessions(&w).await[0]["state"], "playing");

    // It plays on: the session knows, the place kept every 10 s.
    w.say.send(status(&json!(0), "PLAYING", 1900.0)).unwrap();
    let moved = async {
        while sessions(&w).await[0]["position_ms"] != 1_900_000 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), moved).await.expect("the session moves on");

    // Stopped in Pley: stopped on the device at once, Google's player
    // closed, and its last place kept.
    let asked = std::time::Instant::now();
    let stopped = w.app.delete(&format!("/api/sessions/{id}"), &w.admin).await;
    assert!(stopped.status.is_success(), "{}", stopped.text());
    let (to, _) = next(&mut w.sent, "STOP").await;
    assert_eq!(to, "web-5");
    assert!(asked.elapsed() < Duration::from_secs(2), "stopped after {:?}", asked.elapsed());
    let kept = async {
        loop {
            let item = w.app.get(&format!("/api/items/{}", w.film), &w.admin).await;
            if item.body["user_state"]["position_ms"] == 1_900_000 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), kept).await.expect("the last place is kept");
    let (to, close) = next(&mut w.sent, "STOP").await;
    assert_eq!((to.as_str(), close["sessionId"].clone()), ("receiver-0", json!("s-1")));
}

/// A stand-in IPTV provider: two News channels and one of Sport, each
/// sending MPEG-TS packets for as long as it is read.
async fn provider() -> String {
    use axum::routing::get;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let playlist = format!(
        "#EXTM3U\n\
         #EXTINF:-1 tvg-chno=\"1\" group-title=\"News\",One\n{base}/live/1.ts\n\
         #EXTINF:-1 tvg-chno=\"2\" group-title=\"Sport\",Sport\n{base}/live/2.ts\n\
         #EXTINF:-1 tvg-chno=\"3\" group-title=\"News\",Three\n{base}/live/3.ts\n"
    );
    let app = axum::Router::new().route("/list.m3u", get(move || async move { playlist })).route(
        "/live/{n}",
        get(|| async {
            let packets = futures::stream::unfold((), |()| async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                let mut chunk = vec![0u8; 188 * 10];
                for packet in chunk.chunks_mut(188) {
                    packet[0] = 0x47;
                }
                Some((Ok::<_, std::io::Error>(bytes::Bytes::from(chunk)), ()))
            });
            ([(axum::http::header::CONTENT_TYPE, "video/mp2t")], axum::body::Body::from_stream(packets))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

/// The world's Live TV channels, by name, read from a stand-in provider.
async fn channels(w: &World) -> std::collections::HashMap<String, i64> {
    let base = provider().await;
    let source = json!({"name": "Provider", "kind": "m3u", "url": format!("{base}/list.m3u"), "max_connections": 1});
    let added = w.app.post("/api/live/sources", &w.admin, source).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    let read = async {
        loop {
            let channels = w.app.get("/api/live/channels", &w.admin).await.body;
            if channels.as_array().is_some_and(|c| c.len() == 3) {
                let named = channels.as_array().unwrap().iter();
                return named.map(|c| (c["name"].as_str().unwrap().to_owned(), c["id"].as_i64().unwrap())).collect();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read).await.expect("the channels are read")
}

/// The Live TV session on the device, once it watches `channel`.
async fn watching(w: &World, channel: i64) -> Value {
    let found = async {
        loop {
            let live = sessions(w).await.into_iter().find(|s| s["kind"] == "live" && s["channel"]["id"] == channel);
            if let Some(live) = live {
                return live;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), found).await.expect("the device watches the channel")
}

#[tokio::test]
async fn a_channel_plays_on_the_device_and_the_remote_changes_it() {
    let mut w = world().await;
    let channels = channels(&w).await;
    let (one, three) = (channels["One"], channels["Three"]);
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"channel_id": one})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());

    // Google's player is given the channel's keyed live playlist, from its
    // live edge.
    let (_, load) = next(&mut w.sent, "LOAD").await;
    let url = load["media"]["contentId"].as_str().unwrap();
    assert!(url.starts_with("http://pley.test:7539/api/live/media/pley_mk_") && url.ends_with("/index.m3u8"), "{url}");
    assert_eq!(load["media"]["streamType"], "LIVE");
    assert_eq!(load["media"]["metadata"]["title"], "One");
    assert!(load.get("currentTime").is_none(), "{load}");

    // The remotes see the channel on the device: channel up and down and
    // the volume, no pause.
    let session = watching(&w, one).await;
    assert_eq!((session["device_id"].clone(), session["device_kind"].clone()), (json!("cast:dev"), json!("tv")));
    assert_eq!(session["controls"], json!(["next", "previous", "volume"]));
    let id = session["session_id"].as_str().unwrap().to_owned();
    let pause = w.app.post(&format!("/api/sessions/{id}/command"), &w.admin, json!({"command": "pause"})).await;
    assert_eq!(pause.status, StatusCode::CONFLICT);

    // Channel up goes to the next News channel, past the Sport one, on the
    // source's one connection: the one before ends as it starts.
    let up = w.app.post(&format!("/api/sessions/{id}/command"), &w.admin, json!({"command": "next"})).await;
    assert_eq!(up.status, StatusCode::ACCEPTED);
    let (_, load) = next(&mut w.sent, "LOAD").await;
    assert_eq!(load["media"]["metadata"]["title"], "Three");
    let session = watching(&w, three).await;
    let settled = async {
        while sessions(&w).await.len() != 1 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), settled).await.expect("one channel on the device");

    // Channel up again goes round to the first.
    let id = session["session_id"].as_str().unwrap().to_owned();
    w.app.post(&format!("/api/sessions/{id}/command"), &w.admin, json!({"command": "next"})).await;
    assert_eq!(next(&mut w.sent, "LOAD").await.1["media"]["metadata"]["title"], "One");
    let session = watching(&w, one).await;

    // Stopped in Pley: the device stops at once.
    let id = session["session_id"].as_str().unwrap().to_owned();
    let stopped = w.app.delete(&format!("/api/sessions/{id}"), &w.admin).await;
    assert!(stopped.status.is_success(), "{}", stopped.text());
    next(&mut w.sent, "STOP").await;
}

#[tokio::test]
async fn a_cast_that_finishes_on_the_device_ends_its_session_watched() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let finished = json!({"type": "MEDIA_STATUS", "requestId": 0, "status": [
        {"mediaSessionId": 7, "playerState": "IDLE", "idleReason": "FINISHED", "currentTime": 0.0}
    ]});
    w.say.send(finished).unwrap();
    let watched = async {
        loop {
            let item = w.app.get(&format!("/api/items/{}", w.film), &w.admin).await;
            if item.body["user_state"]["watched"] == true && sessions(&w).await.is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), watched).await.expect("watched, and the session over");
}

#[tokio::test]
async fn a_cast_stopped_before_its_first_report_keeps_its_place() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let body = json!({"item_id": w.film, "start_ms": 600_000});
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let stopped = json!({"type": "MEDIA_STATUS", "requestId": 0, "status": [
        {"mediaSessionId": 7, "playerState": "IDLE", "idleReason": "CANCELLED", "currentTime": 0.0}
    ]});
    w.say.send(stopped).unwrap();
    let over = async {
        while !sessions(&w).await.is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), over).await.expect("the session over");
    let item = w.app.get(&format!("/api/items/{}", w.film), &w.admin).await;
    assert_eq!(item.body["user_state"]["position_ms"], 600_000, "{}", item.body);
}

#[tokio::test]
async fn a_cast_without_a_place_starts_where_play_would() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let progress = json!({"position_ms": 600_000, "duration_ms": LENGTH_MS});
    w.app.post(&format!("/api/items/{}/progress", w.film), &w.admin, progress).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    assert_eq!(next(&mut w.sent, "LOAD").await.1["currentTime"], 600.0);

    // Once watched, it starts at the beginning.
    w.app.post(&format!("/api/items/{}/watched", w.film), &w.admin, json!({})).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    assert_eq!(next(&mut w.sent, "LOAD").await.1["currentTime"], 0.0);
}

#[tokio::test]
async fn a_new_cast_takes_the_device_over_without_stopping_it() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    for start_ms in [0, 600_000] {
        let body = json!({"item_id": w.film, "start_ms": start_ms});
        let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body).await;
        assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
        assert_eq!(next(&mut w.sent, "LOAD").await.1["currentTime"], start_ms as f64 / 1000.0);
    }
    // The first cast's session ends as the second plays, and nothing
    // stopped the device on the way (the first checks its session every 5 s).
    let one = async {
        while sessions(&w).await.len() != 1 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), one).await.expect("one session");
    tokio::time::sleep(Duration::from_secs(6)).await;
    while let Ok((_, payload)) = w.sent.try_recv() {
        assert_ne!(payload["type"], "STOP", "{payload}");
    }
    assert_eq!(sessions(&w).await.len(), 1);
}

#[tokio::test]
async fn a_new_cast_that_fails_leaves_the_one_playing_be() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let playing = sessions(&w).await[0]["session_id"].clone();
    // The device refuses the next stream, and the basic one too.
    w.fake.refuse.store(2, Ordering::SeqCst);
    let body = json!({"item_id": w.film, "start_ms": 600_000});
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body).await;
    assert_eq!(failed.status, StatusCode::CONFLICT, "{}", failed.text());
    tokio::time::sleep(Duration::from_secs(6)).await;
    let listed = sessions(&w).await;
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["session_id"], playing);
    // Nor is the player it plays in closed.
    while let Ok((to, payload)) = w.sent.try_recv() {
        assert!(!(to == "receiver-0" && payload["type"] == "STOP"), "{payload}");
    }
}

#[tokio::test]
async fn a_cast_the_server_cant_stream_leaves_one_played_as_its_file_be() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let playing = sessions(&w).await[0]["session_id"].clone();
    // HEVC, which the device gets transcoded, with no transcoder running:
    // the next cast doesn't start, and the one playing, which holds no
    // stream of the server's, plays on.
    film_in(&w.app, w.film, "hevc").await;
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE, "{}", failed.text());
    tokio::time::sleep(Duration::from_millis(500)).await;
    let listed = sessions(&w).await;
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["session_id"], playing);
    while let Ok((_, payload)) = w.sent.try_recv() {
        assert_ne!(payload["type"], "STOP", "{payload}");
    }
}

#[tokio::test]
async fn a_cast_that_fails_closes_the_player_it_opened() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    w.fake.refuse.store(2, Ordering::SeqCst);
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(failed.status, StatusCode::CONFLICT, "{}", failed.text());
    // Google's player, opened for the cast, is closed: the TV goes back to
    // what it showed.
    let (to, stop) = next(&mut w.sent, "STOP").await;
    assert_eq!((to.as_str(), stop["sessionId"].as_str()), ("receiver-0", Some("s-1")));
    assert!(sessions(&w).await.is_empty());
}

/// Whether the device was told to close a session of Google's player
/// among what it was sent so far, given a moment to arrive.
async fn player_closed(sent: &mut Sent) -> bool {
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut closed = false;
    while let Ok((to, payload)) = sent.try_recv() {
        closed |= to == "receiver-0" && payload["type"] == "STOP";
    }
    closed
}

#[tokio::test]
async fn a_cast_closes_no_player_it_didnt_load_into() {
    // It never launched one: the device didn't answer before the deadline.
    let mut w = world_within(Duration::from_secs(5)).await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    w.fake.silent_launch.store(true, Ordering::SeqCst);
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!((failed.status, failed.body["code"].as_str()), (StatusCode::CONFLICT, Some("cast_failed")));
    assert!(!player_closed(&mut w.sent).await, "what runs there isn't the cast's");
    assert!(sessions(&w).await.is_empty());

    // Another sender's session runs by the time the player would close.
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    w.fake.refuse.store(1, Ordering::SeqCst);
    w.fake.other_session.store(true, Ordering::SeqCst);
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(failed.status, StatusCode::CONFLICT);
    assert_eq!(next(&mut w.sent, "GET_STATUS").await.0, "receiver-0", "asked what runs");
    assert!(!player_closed(&mut w.sent).await, "another sender's player is left be");
}

#[tokio::test]
async fn a_newer_cast_keeps_the_player_a_failed_one_was_closing() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // The first cast is refused, and the device takes its time to say what
    // runs as the cast closes the player: a second cast comes meanwhile,
    // into the same session of Google's player.
    w.fake.refuse.store(1, Ordering::SeqCst);
    w.fake.slow_status_ms.store(3_000, Ordering::SeqCst);
    let (app, admin, film, sent) = (&w.app, w.admin.as_str(), w.film, &mut w.sent);
    let (first, second) =
        tokio::join!(app.post("/api/remote/devices/cast:dev/cast", admin, json!({"item_id": film})), async {
            assert_eq!(next(sent, "GET_STATUS").await.0, "receiver-0");
            app.post("/api/remote/devices/cast:dev/cast", admin, json!({"item_id": film, "start_ms": 600_000})).await
        });
    assert_eq!((first.status, second.status), (StatusCode::CONFLICT, StatusCode::NO_CONTENT));
    assert!(!player_closed(&mut w.sent).await, "the newer cast plays on");
    assert_eq!(sessions(&w).await.len(), 1);
}

/// A transcoder that says what it can do, and never answers a request to
/// make a job: its address.
async fn stuck_transcoder() -> String {
    use axum::routing::{get, post};
    let info = transcoder_info();
    let app = axum::Router::new()
        .route("/v1/info", get(move || async move { axum::Json(info) }))
        .route("/v1/jobs", post(std::future::pending::<StatusCode>));
    serve_transcoder(app).await
}

/// A transcoder that makes every job at once, and is never asked for its
/// segments here (the fake device doesn't fetch them): its address.
async fn making_transcoder() -> String {
    making_transcoder_until(Arc::default()).await
}

/// [`making_transcoder`], which takes no new job once `busy` is set: it
/// answers `503`, as a transcoder that can't take one now does.
async fn making_transcoder_until(busy: Arc<AtomicBool>) -> String {
    use axum::Json;
    use axum::response::IntoResponse;
    use axum::routing::{delete, get, post};
    use pley_transcoder_api::{AudioPlan, JobCreated, JobRequest, VideoPlan};
    static MADE: AtomicUsize = AtomicUsize::new(0);
    let info = transcoder_info();
    let app = axum::Router::new()
        .route("/v1/info", get(move || async move { Json(info) }))
        .route(
            "/v1/jobs",
            post(move |Json(job): Json<JobRequest>| {
                let busy = busy.load(Ordering::SeqCst);
                async move {
                    if busy {
                        return StatusCode::SERVICE_UNAVAILABLE.into_response();
                    }
                    Json(JobCreated {
                        id: format!("job{}", MADE.fetch_add(1, Ordering::SeqCst)),
                        duration_ms: LENGTH_MS,
                        segment_seconds: 4.0,
                        segment_count: 1800,
                        video: job.video.as_ref().map(|v| VideoPlan { mode: v.mode, ..Default::default() }),
                        audio: job.audio.as_ref().map(|a| AudioPlan { mode: a.mode, ..Default::default() }),
                        segment_format: job.segment_format,
                        ..Default::default()
                    })
                    .into_response()
                }
            }),
        )
        .route("/v1/jobs/{id}", delete(|| async { StatusCode::NO_CONTENT }));
    serve_transcoder(app).await
}

async fn serve_transcoder(app: axum::Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

/// What the transcoders here say they can do: H.264 by software.
fn transcoder_info() -> pley_transcoder_api::TranscoderInfo {
    use pley_transcoder_api::{API_VERSION, Capabilities, EncoderInfo, HwAccel, Limits, TranscoderInfo, VideoCodec};
    TranscoderInfo {
        name: "pley-transcoder".into(),
        version: "9.9.9".into(),
        api_version: API_VERSION,
        license: "AGPL-3.0-only".into(),
        capabilities: Capabilities {
            ffmpeg_available: true,
            ffprobe_available: true,
            encoders: vec![EncoderInfo {
                name: "libx264".into(),
                codec: VideoCodec::H264,
                hardware: false,
                accel: HwAccel::None,
                main10: false,
                low_power: false,
            }],
            ..Default::default()
        },
        limits: Limits { max_jobs: 8, max_hw_jobs: 0 },
        ..Default::default()
    }
}

/// The world, with the transcoder at `transcoder`, ready.
async fn world_transcoding(transcoder: String) -> World {
    let w = world_where(|config| {
        config.transcoder_url = Some(transcoder);
        config.transcoder_token = Some("secret".into());
    })
    .await;
    w.app.state.transcoder.start();
    assert!(w.app.state.transcoder.wait_ready(Duration::from_secs(10)).await);
    w
}

/// A phone of the admin's plays the film, taking H.264 alone: a video
/// transcode of the server's while the film is HEVC.
async fn phone_plays(w: &World) {
    let caps = json!({"containers": ["mp4"], "video_codecs": ["h264"], "audio_codecs": ["aac"]});
    let body = json!({"item_id": w.film, "caps": caps, "client_id": "phone"});
    let phone = w.app.post("/api/playback", &w.admin, body).await;
    assert_eq!(phone.status, StatusCode::OK, "{}", phone.text());
}

/// The session of the cast playing on the device: its id.
async fn on_the_device(w: &World) -> Value {
    let listed = sessions(w).await;
    let cast = listed.iter().find(|s| s["device_id"] == "cast:dev");
    cast.unwrap_or_else(|| panic!("no cast plays: {listed:?}"))["session_id"].clone()
}

/// The cast of session `playing` plays on, a moment later: its session is
/// there, and nothing stopped the device.
async fn plays_on(w: &mut World, playing: &Value) {
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(&on_the_device(w).await, playing);
    while let Ok((_, payload)) = w.sent.try_recv() {
        assert_ne!(payload["type"], "STOP", "{payload}");
    }
}

#[tokio::test]
async fn a_cast_past_the_limit_on_streams_ends_the_stream_the_device_plays() {
    let mut w = world_transcoding(making_transcoder().await).await;
    let one = w.app.put("/api/settings", &w.admin, json!({"max_transcodes": 1, "seek_previews": false})).await;
    assert_eq!(one.status, StatusCode::OK, "{}", one.text());
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // HEVC, which the device gets transcoded: the server's one stream, in
    // another home member's cast.
    film_in(&w.app, w.film, "hevc").await;
    let (member, _) = w.app.user(&w.admin, "member", json!({"home": true})).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &member, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let theirs = || async { w.app.get("/api/sessions", &member).await.body.as_array().unwrap().len() };
    assert_eq!(theirs().await, 1);
    // The next cast to the device takes its stream over.
    let body = json!({"item_id": w.film, "start_ms": 600_000});
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    assert_eq!(next(&mut w.sent, "LOAD").await.1["currentTime"], 600.0);
    assert_eq!(theirs().await, 0, "the member's cast ended");
    assert_eq!(sessions(&w).await.len(), 1);
}

#[tokio::test]
async fn a_cast_past_the_video_limit_leaves_a_copy_the_device_plays_be() {
    let mut w = world_transcoding(making_transcoder().await).await;
    let limits = json!({"max_transcodes": 4, "max_video_transcodes": 1, "seek_previews": false});
    assert_eq!(w.app.put("/api/settings", &w.admin, limits).await.status, StatusCode::OK);
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // A phone transcodes HEVC: the server's one video transcode.
    film_in(&w.app, w.film, "hevc").await;
    phone_plays(&w).await;
    // The device plays H.264 from Matroska, copied into a stream.
    film_in(&w.app, w.film, "h264").await;
    sqlx::query("UPDATE media_files SET container = 'mkv'").execute(&w.app.state.db.write).await.unwrap();
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    assert_eq!(sessions(&w).await.len(), 2);
    // A cast that needs a video transcode finds none free; the copy, which
    // holds no video transcode's slot, plays on.
    film_in(&w.app, w.film, "hevc").await;
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE, "{}", failed.text());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(sessions(&w).await.len(), 2);
    while let Ok((_, payload)) = w.sent.try_recv() {
        assert_ne!(payload["type"], "STOP", "{payload}");
    }
}

#[tokio::test]
async fn a_cast_that_wouldnt_fit_with_the_devices_stream_gone_leaves_it_be() {
    let mut w = world_transcoding(making_transcoder().await).await;
    // Two streams, one of which may transcode video.
    let limits = json!({"max_transcodes": 2, "max_video_transcodes": 1, "seek_previews": false});
    assert_eq!(w.app.put("/api/settings", &w.admin, limits).await.status, StatusCode::OK);
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // A phone transcodes HEVC, and the device plays H.264 from Matroska,
    // copied into a stream: both streams are taken.
    film_in(&w.app, w.film, "hevc").await;
    phone_plays(&w).await;
    film_in(&w.app, w.film, "h264").await;
    sqlx::query("UPDATE media_files SET container = 'mkv'").execute(&w.app.state.db.write).await.unwrap();
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let playing = on_the_device(&w).await;
    // A cast that needs a video transcode finds no stream free, and with the
    // copy ended it would still find the phone's video transcode: the copy
    // plays on.
    film_in(&w.app, w.film, "hevc").await;
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE, "{}", failed.text());
    plays_on(&mut w, &playing).await;
    assert_eq!(sessions(&w).await.len(), 2);
}

#[tokio::test]
async fn a_cast_past_a_limit_lowered_under_the_streams_playing_leaves_the_devices_be() {
    let mut w = world_transcoding(making_transcoder().await).await;
    let previews = json!({"seek_previews": false});
    assert_eq!(w.app.put("/api/settings", &w.admin, previews).await.status, StatusCode::OK);
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // A phone and the device each transcode HEVC.
    film_in(&w.app, w.film, "hevc").await;
    phone_plays(&w).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let playing = on_the_device(&w).await;
    // An administrator lowers the limit to one stream while both play: with
    // the device's ended, the phone's would still reach it.
    let one = w.app.put("/api/settings", &w.admin, json!({"max_transcodes": 1})).await;
    assert_eq!(one.status, StatusCode::OK, "{}", one.text());
    let body = json!({"item_id": w.film, "start_ms": 600_000});
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE, "{}", failed.text());
    plays_on(&mut w, &playing).await;
    assert_eq!(sessions(&w).await.len(), 2);
}

#[tokio::test]
async fn a_cast_the_transcoder_refuses_leaves_the_devices_stream_be() {
    let busy = Arc::new(AtomicBool::new(false));
    let mut w = world_transcoding(making_transcoder_until(busy.clone()).await).await;
    let limits = json!({"max_transcodes": 4, "max_video_transcodes": 1, "seek_previews": false});
    assert_eq!(w.app.put("/api/settings", &w.admin, limits).await.status, StatusCode::OK);
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // The device transcodes HEVC: the server's one video transcode.
    film_in(&w.app, w.film, "hevc").await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    let playing = on_the_device(&w).await;
    // The transcoder takes no new job now. A cast it would only copy is
    // refused for that, not for the limit, which ending the device's stream
    // wouldn't change: it plays on.
    busy.store(true, Ordering::SeqCst);
    film_in(&w.app, w.film, "h264").await;
    sqlx::query("UPDATE media_files SET container = 'mkv'").execute(&w.app.state.db.write).await.unwrap();
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(failed.status, StatusCode::SERVICE_UNAVAILABLE, "{}", failed.text());
    plays_on(&mut w, &playing).await;
    assert_eq!(sessions(&w).await.len(), 1);
}

#[tokio::test]
async fn a_session_slow_to_start_ends_the_cast_in_time_and_frees_the_device() {
    let transcoder = stuck_transcoder().await;
    let mut w = world_where(|config| {
        config.cast_within = Duration::from_secs(5);
        config.transcoder_url = Some(transcoder);
        config.transcoder_token = Some("secret".into());
    })
    .await;
    w.app.state.transcoder.start();
    assert!(w.app.state.transcoder.wait_ready(Duration::from_secs(10)).await);
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // HEVC, which every Cast device doesn't play: a transcode, whose job the
    // transcoder never makes.
    film_in(&w.app, w.film, "hevc").await;
    let asked = std::time::Instant::now();
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!((failed.status, failed.body["code"].as_str()), (StatusCode::CONFLICT, Some("cast_failed")));
    assert!(asked.elapsed() < Duration::from_secs(20), "answered at the deadline, not the transcoder's");
    assert!(sessions(&w).await.is_empty());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(w.sent.try_recv().is_err(), "the device was never reached, nor closed");

    // The device is free for the next cast, which plays as it is.
    film_in(&w.app, w.film, "h264").await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    assert_eq!(sessions(&w).await.len(), 1);
}

#[tokio::test]
async fn a_cast_the_device_never_loads_stops_in_time() {
    let mut w = world_within(Duration::from_secs(5)).await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    w.fake.silent_load.store(true, Ordering::SeqCst);
    let asked = std::time::Instant::now();
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!((failed.status, failed.body["code"].as_str()), (StatusCode::CONFLICT, Some("cast_failed")));
    let waited = asked.elapsed();
    assert!(waited >= Duration::from_secs(5), "not before the deadline: {waited:?}");
    assert!(waited < Duration::from_secs(20), "answered at the deadline, not the device's own waits");
    // Nothing is left to play: no session, and Google's player closed.
    assert!(sessions(&w).await.is_empty());
    let (to, stop) = next(&mut w.sent, "STOP").await;
    assert_eq!((to.as_str(), stop["sessionId"].as_str()), ("receiver-0", Some("s-1")));

    // A remote that stops waiting doesn't stop the deadline: the cast,
    // started apart from its request, ends there all the same.
    let body = json!({"item_id": w.film});
    let dropped = tokio::time::timeout(
        Duration::from_millis(500),
        w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body),
    )
    .await;
    assert!(dropped.is_err(), "the device doesn't play it");
    next(&mut w.sent, "LOAD").await;
    assert_eq!(sessions(&w).await.len(), 1, "the cast goes on without its request");
    let (to, _) = next(&mut w.sent, "STOP").await;
    assert_eq!(to, "receiver-0");
    assert!(sessions(&w).await.is_empty());
}

#[tokio::test]
async fn two_casts_at_once_leave_one_playing() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let (first, second) = tokio::join!(
        w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})),
        w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film, "start_ms": 600_000})),
    );
    assert_eq!((first.status, second.status), (StatusCode::NO_CONTENT, StatusCode::NO_CONTENT));
    // One of the two plays, followed, and neither stopped the device.
    tokio::time::sleep(Duration::from_secs(6)).await;
    let listed = sessions(&w).await;
    assert_eq!(listed.len(), 1, "{listed:?}");
    while let Ok((_, payload)) = w.sent.try_recv() {
        assert_ne!(payload["type"], "STOP", "{payload}");
    }
}

#[tokio::test]
async fn a_device_that_refuses_its_profile_gets_the_basic_one() {
    // A TV with Google TV: HEVC is tried first.
    let setup = setup_api(json!({"device_info": {
        "model_name": "BRAVIA 4K VH2",
        "product_name": "BRAVIA_VH2",
        "4k_blocked": 0,
        "capabilities": {"cast_connect_supported": true},
    }}))
    .await;
    let mut w = world_with(Some(setup)).await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    w.fake.refuse.store(1, Ordering::SeqCst);
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    next(&mut w.sent, "LOAD").await;
    let playing = sessions(&w).await;
    assert_eq!(playing.len(), 1, "the refused session ended: {playing:?}");

    // Its model is remembered: the next cast is the basic one at once, and
    // nothing comes after it to fall back on.
    w.fake.refuse.store(1, Ordering::SeqCst);
    let refused = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(refused.body["code"], "cast_failed");
}

#[tokio::test]
async fn the_remote_sets_the_devices_volume() {
    let mut w = world().await;
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    // The device's own level, as it says it.
    let mut known = Value::Null;
    for _ in 0..50 {
        known = sessions(&w).await[0]["volume"].clone();
        if !known.is_null() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(known, json!(50));
    let session = sessions(&w).await[0]["session_id"].as_str().unwrap().to_owned();
    assert!(sessions(&w).await[0]["controls"].as_array().unwrap().contains(&json!("volume")));

    let path = format!("/api/sessions/{session}/command");
    for wrong in [
        json!({"command": "volume"}),
        json!({"command": "volume", "volume": 101}),
        json!({"command": "volume", "volume": -1}),
    ] {
        assert_eq!(w.app.post(&path, &w.admin, wrong).await.status, StatusCode::BAD_REQUEST);
    }
    let set = w.app.post(&path, &w.admin, json!({"command": "volume", "volume": 30})).await;
    assert_eq!(set.status, StatusCode::ACCEPTED, "{}", set.text());
    let (to, payload) = next(&mut w.sent, "SET_VOLUME").await;
    assert_eq!((to.as_str(), payload["volume"]["level"].as_f64()), ("receiver-0", Some(0.3)));
    // A level is heard: a muted device is unmuted too.
    assert_eq!(next(&mut w.sent, "SET_VOLUME").await.1["volume"], json!({"muted": false}));
    for _ in 0..50 {
        if sessions(&w).await[0]["volume"] == json!(30) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the session's volume didn't follow the device's");
}

#[tokio::test]
async fn a_channel_cast_says_when_every_connection_is_in_use() {
    let w = world().await;
    let channels = channels(&w).await;
    // The source's one connection is watched here, on Sport.
    let watch = w.app.post(&format!("/api/live/channels/{}/watch", channels["Sport"]), &w.admin, json!({})).await;
    assert_eq!(watch.status, StatusCode::OK, "{}", watch.text());
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"channel_id": channels["One"]})).await;
    // A cast takes no connection over: Play on… says they are all in use.
    assert_eq!(cast.status, StatusCode::CONFLICT, "{}", cast.text());
    assert_eq!(cast.body["code"], "connections_full");
}

#[tokio::test]
async fn a_refused_channel_cast_leaves_a_cast_loading_on_the_device_be() {
    let mut w = world().await;
    let channels = channels(&w).await;
    // The source's one connection is watched here, on Sport.
    let watch = w.app.post(&format!("/api/live/channels/{}/watch", channels["Sport"]), &w.admin, json!({})).await;
    assert_eq!(watch.status, StatusCode::OK, "{}", watch.text());
    w.app.get("/api/remote/devices?nearby=1", &w.admin).await;
    // A film loads, the device taking its time to answer; meanwhile casts
    // of a channel that isn't there, and of one with no connection free,
    // are refused without stopping it.
    w.fake.slow_status_ms.store(3_000, Ordering::SeqCst);
    let (app, admin, film, sent) = (&w.app, w.admin.as_str(), w.film, &mut w.sent);
    let (loaded, (gone, full)) =
        tokio::join!(app.post("/api/remote/devices/cast:dev/cast", admin, json!({"item_id": film})), async {
            assert_eq!(next(sent, "GET_STATUS").await.0, "receiver-0");
            let gone = app.post("/api/remote/devices/cast:dev/cast", admin, json!({"channel_id": 999_999})).await;
            let full =
                app.post("/api/remote/devices/cast:dev/cast", admin, json!({"channel_id": channels["One"]})).await;
            (gone, full)
        });
    assert_eq!(gone.status, StatusCode::NOT_FOUND, "{}", gone.text());
    assert_eq!((full.status, full.body["code"].clone()), (StatusCode::CONFLICT, json!("connections_full")));
    assert_eq!(loaded.status, StatusCode::NO_CONTENT, "{}", loaded.text());
    let film_on_device = sessions(&w).await.into_iter().any(|s| s["device_id"] == "cast:dev" && s["kind"] != "live");
    assert!(film_on_device, "the film plays on");
}
