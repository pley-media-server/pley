//! Casting to Google Cast devices: listed for the household, a cast played
//! on the device as a session of the caster's, its place kept and the
//! remotes' commands passed on. The device is a fake that speaks Cast over
//! TLS on this computer.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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
async fn fake_device() -> (SocketAddr, Sent, Say, Refuse) {
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
    let refuse = Refuse::default();
    let refusing = refuse.clone();
    tokio::spawn(async move {
        // Each connection as it comes: a new cast connects again.
        while let Ok((tcp, _)) = listener.accept().await {
            let (acceptor, sent_tx, said, refuse) = (acceptor.clone(), sent_tx.clone(), said.clone(), refusing.clone());
            tokio::spawn(async move { serve(acceptor.accept(tcp).await.unwrap(), sent_tx, said, refuse).await });
        }
    });
    (address, sent, say, refuse)
}

/// How many loads the fake device refuses next, as a device does a stream
/// it can't decode.
type Refuse = Arc<AtomicUsize>;

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
    refuse: Refuse,
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
        let receiver = |id: &Value, volume: f64, muted: bool| {
            json!({"type": "RECEIVER_STATUS", "requestId": id, "status": {
                "applications": [{"appId": "CC1AD845", "transportId": "web-5", "sessionId": "s-1"}],
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
                Some("LAUNCH") => Some(reply("receiver-0", "receiver", receiver(&id, volume, muted))),
                Some("GET_STATUS") if message.destination == "receiver-0" => {
                    Some(reply("receiver-0", "receiver", receiver(&id, volume, muted)))
                }
                // Either the level or the mute, as Google's senders send them.
                Some("SET_VOLUME") => {
                    assert!(payload["volume"].as_object().unwrap().len() == 1, "{payload}");
                    volume = payload["volume"]["level"].as_f64().unwrap_or(volume);
                    muted = payload["volume"]["muted"].as_bool().unwrap_or(muted);
                    Some(reply("receiver-0", "receiver", receiver(&id, volume, muted)))
                }
                Some("LOAD") if refuse.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() => {
                    Some(reply("web-5", "media", json!({"type": "LOAD_FAILED", "requestId": id})))
                }
                Some("LOAD") => {
                    at = payload["currentTime"].as_f64().unwrap();
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
    refuse: Refuse,
}

async fn world() -> World {
    world_with(None).await
}

/// The world, with the device's setup API at `setup`.
async fn world_with(setup: Option<SocketAddr>) -> World {
    let (address, sent, say, refuse) = fake_device().await;
    let app = TestApp::with_config(|config| {
        config.cast_fixed.device = Some(address);
        config.cast_fixed.server = Some("http://pley.test:7539".into());
        config.cast_fixed.setup = setup;
    })
    .await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2001)/Film (2001).mp4", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    sqlx::query("UPDATE items SET duration_ms = ?").bind(LENGTH_MS).execute(&app.state.db.write).await.unwrap();
    // H.264 and AAC in MP4, as a Cast device plays it, with English text
    // subtitles.
    let info = json!({
        "container": "mov,mp4,m4a,3gp,3g2,mj2",
        "duration_ms": LENGTH_MS,
        "streams": [
            {"index": 0, "kind": "video", "codec": "h264", "width": 1920, "height": 1080, "pix_fmt": "yuv420p"},
            {"index": 1, "kind": "audio", "codec": "aac", "channels": 2, "language": "eng", "default": true},
            {"index": 2, "kind": "subtitle", "codec": "mov_text", "language": "eng", "text_based": true},
        ],
    });
    sqlx::query("UPDATE media_files SET media_info = ?, container = 'mp4', video_codec = 'h264', audio_codec = 'aac' WHERE id = ?")
        .bind(info.to_string())
        .bind(app.file_of(film).await)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    World { app, admin, film, sent, say, refuse }
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
    assert_eq!((cast["kind"].clone(), cast["casts"].clone()), (json!("tv"), json!(["item"])));

    let (friend, _) = w.app.user(&w.admin, "friend", json!({"home": false})).await;
    let theirs = w.app.get("/api/remote/devices?nearby=1", &friend).await;
    assert!(theirs.body.as_array().unwrap().iter().all(|d| d["protocol"].is_null()), "{}", theirs.text());
    let refused = w.app.post("/api/remote/devices/cast:dev/cast", &friend, json!({"item_id": w.film})).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    // A device no look found is as unknown.
    let unknown = w.app.post("/api/remote/devices/cast:elsewhere/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
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
async fn only_movies_and_episodes_are_cast() {
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
    w.refuse.store(2, Ordering::SeqCst);
    let body = json!({"item_id": w.film, "start_ms": 600_000});
    let failed = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, body).await;
    assert_eq!(failed.status, StatusCode::CONFLICT, "{}", failed.text());
    tokio::time::sleep(Duration::from_secs(6)).await;
    let listed = sessions(&w).await;
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0]["session_id"], playing);
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
    w.refuse.store(1, Ordering::SeqCst);
    let cast = w.app.post("/api/remote/devices/cast:dev/cast", &w.admin, json!({"item_id": w.film})).await;
    assert_eq!(cast.status, StatusCode::NO_CONTENT, "{}", cast.text());
    next(&mut w.sent, "LOAD").await;
    next(&mut w.sent, "LOAD").await;
    let playing = sessions(&w).await;
    assert_eq!(playing.len(), 1, "the refused session ended: {playing:?}");

    // Its model is remembered: the next cast is the basic one at once, and
    // nothing comes after it to fall back on.
    w.refuse.store(1, Ordering::SeqCst);
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
