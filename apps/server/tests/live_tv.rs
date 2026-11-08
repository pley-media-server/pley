//! Live TV (docs/API.md, "Live TV"): a source's channels and guide read from
//! a stand-in IPTV provider, favourites, who may watch, and the connections
//! to the provider: one per channel however many watch it, no more than the
//! source allows, and a device changing channels freeing its own.

mod common;

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{Method, Request, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use futures::StreamExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::TestApp;

/// A 1×1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[derive(Default)]
struct Provider {
    /// Streams open now, the most open at once, and opened in all.
    open: AtomicUsize,
    most: AtomicUsize,
    opened: AtomicUsize,
    /// Guides asked for, and while `held`, guides kept back until `released`.
    guides: AtomicUsize,
    held: AtomicBool,
    released: tokio::sync::Notify,
}

struct Open(Arc<Provider>);

impl Drop for Open {
    fn drop(&mut self) {
        self.0.open.fetch_sub(1, Ordering::SeqCst);
    }
}

fn guide_xml() -> Vec<u8> {
    let at = |offset: i64| {
        let t = chrono_like(pley::util::now() + offset);
        format!("{t} +0000")
    };
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<tv>
  <channel id="one.test"><display-name>One</display-name></channel>
  <channel id="two.test"><display-name>Two HD</display-name></channel>
  <channel id="quiz.test"><display-name>Quiz Time</display-name></channel>
  <programme start="{}" stop="{}" channel="one.test"><title>The News</title><desc>What happened.</desc><category>News</category></programme>
  <programme start="{}" stop="{}" channel="one.test"><title>The Film</title></programme>
  <programme start="{}" stop="{}" channel="two.test"><title>Football</title><live/></programme>
  <programme start="{}" stop="{}" channel="quiz.test"><title>The Quiz</title></programme>
</tv>"#,
        at(-600),
        at(1200),
        at(1200),
        at(7200),
        at(-60),
        at(5400),
        at(-300),
        at(3600)
    );
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(xml.as_bytes()).unwrap();
    gz.finish().unwrap()
}

/// Unix seconds as XMLTV's `YYYYMMDDHHMMSS` (UTC).
fn chrono_like(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // Civil date from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}{:02}{:02}{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// The stand-in provider: a playlist naming its guide, the guide, logos,
/// and channels that send MPEG-TS packets for as long as they are read.
async fn provider() -> (String, Arc<Provider>) {
    let counts = Arc::new(Provider::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let playlist = format!(
        "#EXTM3U url-tvg=\"{base}/guide.xml.gz\"\n\
         #EXTINF:-1 tvg-id=\"one.test\" tvg-chno=\"1\" tvg-logo=\"{base}/logo.png\" group-title=\"News\",One\n\
         {base}/live/1.ts\n\
         #EXTINF:-1 tvg-chno=\"2\" group-title=\"Sport\",Two HD\n\
         {base}/live/2.ts\n\
         #EXTINF:-1 tvg-chno=\"10\" group-title=\"Sport\",Ten\n\
         {base}/live/10.ts\n\
         #EXTINF:-1 group-title=\"Films\",A film\n\
         {base}/movie/u/p/5.mkv\n"
    );
    let app = Router::new()
        .route("/list.m3u", get(move || async move { playlist }))
        .route(
            "/guide.xml.gz",
            get(|State(counts): State<Arc<Provider>>| async move {
                counts.guides.fetch_add(1, Ordering::SeqCst);
                if counts.held.load(Ordering::SeqCst) {
                    counts.released.notified().await;
                }
                ([(header::CONTENT_TYPE, "application/gzip")], guide_xml())
            }),
        )
        .route("/logo.png", get(|| async { ([(header::CONTENT_TYPE, "image/png")], PNG) }))
        .route(
            "/live/{n}",
            get(|State(counts): State<Arc<Provider>>, Path(n): Path<String>| async move {
                let open = counts.open.fetch_add(1, Ordering::SeqCst) + 1;
                counts.most.fetch_max(open, Ordering::SeqCst);
                counts.opened.fetch_add(1, Ordering::SeqCst);
                let guard = Open(counts.clone());
                let id = n.trim_end_matches(".ts").parse::<u8>().unwrap_or(0);
                let packets = futures::stream::unfold(guard, move |guard| async move {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    let mut chunk = vec![0u8; 188 * 10];
                    for packet in chunk.chunks_mut(188) {
                        packet[0] = 0x47;
                        packet[4] = id;
                    }
                    Some((Ok::<_, std::io::Error>(bytes::Bytes::from(chunk)), guard))
                });
                ([(header::CONTENT_TYPE, "video/mp2t")], Body::from_stream(packets)).into_response()
            }),
        )
        .with_state(counts.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, counts)
}

async fn wait_until_read(app: &TestApp, admin: &str) -> Value {
    for _ in 0..100 {
        let sources = app.get("/api/live/sources", admin).await.body;
        let s = &sources[0];
        if s["guide_at"].is_i64() && !s["refreshing"].as_bool().unwrap_or(true) {
            return s.clone();
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the source wasn't read");
}

/// Starts reading a channel's MPEG-TS as `token`'s device `client`: the
/// status, and the body to read from.
async fn open_stream(app: &TestApp, token: &str, channel: i64, client: &str) -> (StatusCode, Body) {
    let req = Request::builder()
        .uri(format!("/api/live/channels/{channel}/stream.ts?client_id={client}"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.api.clone().oneshot(req).await.unwrap();
    (resp.status(), resp.into_body())
}

/// The first bytes of a stream (a few chunks' worth).
async fn first_bytes(body: Body) -> (Vec<u8>, impl futures::Stream) {
    let mut stream = body.into_data_stream();
    let mut got = Vec::new();
    while got.len() < 188 * 20 {
        let chunk = tokio::time::timeout(Duration::from_secs(5), stream.next()).await.unwrap().unwrap().unwrap();
        got.extend_from_slice(&chunk);
    }
    (got, stream)
}

#[tokio::test]
async fn channels_guide_favourites_and_connections() {
    let (base, provider) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;

    // Nothing yet: the apps show no Live TV.
    let summary = app.get("/api/live", &bob).await.body;
    assert_eq!((summary["available"].as_bool(), summary["allowed"].as_bool()), (Some(false), Some(true)));

    let added = app
        .post(
            "/api/live/sources",
            &admin,
            json!({"name": "Provider", "kind": "m3u", "url": format!("{base}/list.m3u"), "max_connections": 1}),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{:?}", added.body);
    let source = wait_until_read(&app, &admin).await;
    assert_eq!(source["channel_count"], 3, "{source}");
    assert_eq!(source["guide_url"], format!("{base}/guide.xml.gz"), "the playlist's own guide is taken");
    assert_eq!(source["guided_count"], 2, "Two HD is matched to the guide's Two by its name");
    assert!(source["channels_error"].is_null() && source["guide_error"].is_null(), "{source}");

    let channels = app.get("/api/live/channels", &bob).await.body;
    let names: Vec<&str> = channels.as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["One", "Two HD", "Ten"], "films are left out, channels go by number");
    let one = &channels[0];
    let (one_id, two_id) = (one["id"].as_i64().unwrap(), channels[1]["id"].as_i64().unwrap());
    assert_eq!(one["now"]["title"], "The News");
    assert_eq!(one["now"]["description"], "What happened.");
    assert_eq!(one["next"]["title"], "The Film");
    assert_eq!(channels[1]["now"]["title"], "Football");
    assert_eq!(channels[1]["now"]["is_live"], true);
    assert!(channels[2]["now"].is_null() && channels[2]["has_guide"] == false);

    // The logo comes through the server.
    let logo = app.get(one["logo"].as_str().unwrap(), &bob).await;
    assert_eq!(logo.status, StatusCode::OK);
    assert_eq!(logo.bytes, PNG);
    assert!(channels[1]["logo"].is_null());

    // Favourites are each user's own.
    assert_eq!(
        app.put(&format!("/api/live/channels/{two_id}/favorite"), &bob, json!({})).await.status,
        StatusCode::NO_CONTENT
    );
    let bobs = app.get("/api/live/channels", &bob).await.body;
    assert_eq!(bobs[1]["favorite"], true);
    assert_eq!(app.get("/api/live/channels", &admin).await.body[1]["favorite"], false);

    let guide = app.get("/api/live/guide", &bob).await.body;
    assert_eq!(guide["programs"][one_id.to_string()].as_array().unwrap().len(), 2, "{guide}");

    // Two watch One: one connection to the provider.
    let (status, body) = open_stream(&app, &admin, one_id, "tv").await;
    assert_eq!(status, StatusCode::OK);
    let (first, admin_stream) = first_bytes(body).await;
    assert_eq!((first[0], first[4]), (0x47, 1), "the provider's packets come as they are");
    let (status, body) = open_stream(&app, &bob, one_id, "phone").await;
    assert_eq!(status, StatusCode::OK);
    let (_, bob_stream) = first_bytes(body).await;
    assert_eq!(provider.opened.load(Ordering::SeqCst), 1, "one connection for both");

    // The one connection is in use: Bob can't tune Two as well.
    let refused = app.post(&format!("/api/live/channels/{two_id}/watch"), &bob, json!({"client_id": "laptop"})).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{:?}", refused.body);
    assert!(refused.body["message"].as_str().unwrap().contains("watching One"), "{:?}", refused.body);

    let tuners = app.get("/api/live/tuners", &admin).await.body;
    assert_eq!(tuners.as_array().unwrap().len(), 1);
    assert_eq!(tuners[0]["viewers"].as_array().unwrap().len(), 2, "{tuners}");

    // Bob stops; the TV changes to Two: its connection goes to Two.
    drop(bob_stream);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (status, body) = open_stream(&app, &admin, two_id, "tv").await;
    assert_eq!(status, StatusCode::OK);
    drop(admin_stream);
    let (first, _two) = first_bytes(body).await;
    assert_eq!(first[4], 2, "Two's packets");
    for _ in 0..50 {
        if provider.open.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(provider.open.load(Ordering::SeqCst), 1);
    assert_eq!(provider.most.load(Ordering::SeqCst), 1, "never more than the one connection at once");
    assert_eq!(provider.opened.load(Ordering::SeqCst), 2);

    let summary = app.get("/api/live", &bob).await.body;
    assert_eq!(summary["available"], true);
    assert_eq!(summary["recent"], json!([one_id]));

    // Hidden channels are nobody's but the administrators'.
    let hidden = app.patch("/api/live/lineup", &admin, json!({"ids": [two_id], "hidden": true})).await;
    assert_eq!(hidden.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/live/channels", &bob).await.body.as_array().unwrap().len(), 2);
    assert_eq!(app.get(&format!("/api/live/channels/{two_id}"), &bob).await.status, StatusCode::NOT_FOUND);
    let lineup = app.get("/api/live/lineup", &admin).await.body;
    assert_eq!(lineup.as_array().unwrap().len(), 3);
    assert!(app.get("/api/live/tuners", &admin).await.body.as_array().unwrap().is_empty(), "hiding it stopped it");

    // Renamed and renumbered, kept across a refresh.
    let renamed =
        app.patch("/api/live/lineup", &admin, json!({"ids": [one_id], "name": "One (UK)", "number": "50"})).await;
    assert_eq!(renamed.status, StatusCode::NO_CONTENT);
    let id = source["id"].as_i64().unwrap();
    assert_eq!(
        app.post(&format!("/api/live/sources/{id}/refresh"), &admin, json!({})).await.status,
        StatusCode::ACCEPTED
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    wait_until_read(&app, &admin).await;
    let names: Vec<String> = app
        .get("/api/live/channels", &bob)
        .await
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["Ten", "One (UK)"]);

    // Administrators only: Bob sees no Live TV.
    let settings = app.get("/api/live/settings", &admin).await.body;
    let mut admins_only = settings.clone();
    admins_only["access"] = json!("admins");
    assert_eq!(app.put("/api/live/settings", &admin, admins_only).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/live/channels", &bob).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get("/api/live", &bob).await.body["allowed"], false);
    assert_eq!(app.get("/api/live/sources", &bob).await.status, StatusCode::FORBIDDEN);
    let mut bad = settings;
    bad["timeshift_minutes"] = json!(0);
    assert_eq!(app.put("/api/live/settings", &admin, bad).await.status, StatusCode::BAD_REQUEST);
}

/// A guide channel set by hand that no channel wanted when the guide was
/// read has its programmes at once: the guide is read again for it, and
/// only for it.
#[tokio::test]
async fn a_guide_channel_set_by_hand_has_its_programmes_at_once() {
    let (base, provider) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let added = app
        .post(
            "/api/live/sources",
            &admin,
            json!({"name": "Provider", "kind": "m3u", "url": format!("{base}/list.m3u")}),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{:?}", added.body);
    wait_until_read(&app, &admin).await;
    assert_eq!(provider.guides.load(Ordering::SeqCst), 1);
    let channels = app.get("/api/live/channels", &admin).await.body;
    let ten = &channels[2];
    assert_eq!(ten["name"], "Ten");
    assert!(ten["now"].is_null() && ten["has_guide"] == false, "{ten}");
    let ten_id = ten["id"].as_i64().unwrap();

    // The guide lists Quiz Time, which no channel wanted when it was read.
    let set = app.patch("/api/live/lineup", &admin, json!({"ids": [ten_id], "guide_id": "quiz.test"})).await;
    assert_eq!(set.status, StatusCode::NO_CONTENT);
    assert_eq!(on_now(&app, &admin, ten_id).await["title"], "The Quiz", "read again for it");
    wait_until_read(&app, &admin).await;
    assert_eq!(provider.guides.load(Ordering::SeqCst), 2);

    // One's guide channel has its programmes already: no read for it.
    let set = app.patch("/api/live/lineup", &admin, json!({"ids": [ten_id], "guide_id": "one.test"})).await;
    assert_eq!(set.status, StatusCode::NO_CONTENT);
    let ten = app.get(&format!("/api/live/channels/{ten_id}"), &admin).await.body;
    assert_eq!(ten["now"]["title"], "The News");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(provider.guides.load(Ordering::SeqCst), 2);
}

/// Channel `id`'s programme on now, once it has one (within 10 s).
async fn on_now(app: &TestApp, admin: &str, id: i64) -> Value {
    for _ in 0..100 {
        let now = app.get(&format!("/api/live/channels/{id}"), admin).await.body["now"].clone();
        if !now.is_null() {
            return now;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Value::Null
}

/// A guide channel set while the guide downloads is seen by that read: no
/// second read for it. Refresh asked for during a read comes after it.
#[tokio::test]
async fn a_read_under_way_takes_the_channels_set_and_refresh_comes_after_it() {
    let (base, provider) = provider().await;
    provider.held.store(true, Ordering::SeqCst);
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let added = app
        .post(
            "/api/live/sources",
            &admin,
            json!({"name": "Provider", "kind": "m3u", "url": format!("{base}/list.m3u")}),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{:?}", added.body);
    let source_id = added.body["id"].as_i64().unwrap();
    guides_asked(&provider, 1).await;
    let lineup = app.get("/api/live/lineup", &admin).await.body;
    let ten_id = lineup.as_array().unwrap().iter().find(|c| c["name"] == "Ten").unwrap()["id"].as_i64().unwrap();

    // Set while the guide downloads, before the read lists the channels.
    let set = app.patch("/api/live/lineup", &admin, json!({"ids": [ten_id], "guide_id": "quiz.test"})).await;
    assert_eq!(set.status, StatusCode::NO_CONTENT);
    tokio::time::sleep(Duration::from_millis(200)).await;
    provider.held.store(false, Ordering::SeqCst);
    provider.released.notify_one();
    assert_eq!(on_now(&app, &admin, ten_id).await["title"], "The Quiz");
    wait_until_read(&app, &admin).await;
    assert_eq!(provider.guides.load(Ordering::SeqCst), 1, "no second read");

    // Refresh during a read: read again after it, not left out.
    provider.held.store(true, Ordering::SeqCst);
    let refresh = format!("/api/live/sources/{source_id}/refresh");
    assert_eq!(app.post(&refresh, &admin, json!({})).await.status, StatusCode::ACCEPTED);
    guides_asked(&provider, 2).await;
    assert_eq!(app.post(&refresh, &admin, json!({})).await.status, StatusCode::ACCEPTED);
    tokio::time::sleep(Duration::from_millis(200)).await;
    provider.held.store(false, Ordering::SeqCst);
    provider.released.notify_one();
    guides_asked(&provider, 3).await;
    wait_until_read(&app, &admin).await;
    assert_eq!(provider.guides.load(Ordering::SeqCst), 3);
}

/// Waits until the provider has been asked for `n` guides (within 5 s).
async fn guides_asked(provider: &Provider, n: usize) {
    for _ in 0..100 {
        if provider.guides.load(Ordering::SeqCst) >= n {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(provider.guides.load(Ordering::SeqCst), n);
}

/// A channel renamed to a name the guide knows is matched to it at once.
#[tokio::test]
async fn a_channel_renamed_as_the_guide_names_it_has_its_programmes_at_once() {
    let (base, provider) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let added = app
        .post(
            "/api/live/sources",
            &admin,
            json!({"name": "Provider", "kind": "m3u", "url": format!("{base}/list.m3u")}),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{:?}", added.body);
    wait_until_read(&app, &admin).await;
    let lineup = app.get("/api/live/lineup", &admin).await.body;
    let ten_id = lineup.as_array().unwrap().iter().find(|c| c["name"] == "Ten").unwrap()["id"].as_i64().unwrap();

    let renamed = app.patch("/api/live/lineup", &admin, json!({"ids": [ten_id], "name": "Quiz Time"})).await;
    assert_eq!(renamed.status, StatusCode::NO_CONTENT);
    assert_eq!(on_now(&app, &admin, ten_id).await["title"], "The Quiz");
    wait_until_read(&app, &admin).await;
    assert_eq!(provider.guides.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn changing_channels_frees_the_connection_the_old_stream_still_holds() {
    let (base, provider) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let source = json!({"name": "Provider", "kind": "m3u", "url": format!("{base}/list.m3u"), "max_connections": 1});
    assert_eq!(app.post("/api/live/sources", &admin, source).await.status, StatusCode::CREATED);
    wait_until_read(&app, &admin).await;
    let channels = app.get("/api/live/channels", &admin).await.body;
    let (one, two) = (channels[0]["id"].as_i64().unwrap(), channels[1]["id"].as_i64().unwrap());

    // As the apps watch: the session asked for as the device, its stream read as `<device>-stream`.
    let asked = app.post(&format!("/api/live/channels/{one}/watch"), &admin, json!({"client_id": "tv"})).await;
    assert!(asked.status.is_success(), "{:?}", asked.body);
    let (status, body) = open_stream(&app, &admin, one, "tv-stream").await;
    assert_eq!(status, StatusCode::OK);
    let (_, one_reader) = first_bytes(body).await;

    // Two is asked for before One's stream closed: One's connection is Two's all the same.
    let asked = app.post(&format!("/api/live/channels/{two}/watch"), &admin, json!({"client_id": "tv"})).await;
    assert!(asked.status.is_success(), "{:?}", asked.body);
    let (status, body) = open_stream(&app, &admin, two, "tv-stream").await;
    assert_eq!(status, StatusCode::OK);
    let (first, _two_reader) = first_bytes(body).await;
    assert_eq!(first[4], 2, "Two's packets");
    drop(one_reader);
    assert_eq!(provider.most.load(Ordering::SeqCst), 1, "never more than the one connection at once");
}

#[tokio::test]
async fn channels_by_group_a_page_at_a_time() {
    let (base, _) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let url = format!("{base}/list.m3u");
    app.post("/api/live/sources", &admin, json!({"name": "Provider", "kind": "m3u", "url": url})).await;
    wait_until_read(&app, &admin).await;
    let names = |page: &Value| -> Vec<String> {
        page["channels"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_owned()).collect()
    };

    let groups = app.get("/api/live/groups", &bob).await.body;
    assert_eq!((groups["total"].as_i64(), groups["favorites"].as_i64()), (Some(3), Some(0)));
    assert_eq!(
        groups["groups"],
        json!([
            {"name": "News", "region": null, "label": "News", "count": 1},
            {"name": "Sport", "region": null, "label": "Sport", "count": 2},
        ])
    );

    let sport = app.get("/api/live/browse?category=group&group=Sport", &bob).await.body;
    assert_eq!((sport["total"].as_i64(), sport["offset"].as_i64()), (Some(2), Some(0)));
    assert_eq!(names(&sport), ["Two HD", "Ten"]);
    assert_eq!(sport["channels"][0]["now"]["title"], "Football", "what's on comes with the page");
    // Channels name their source by its short id, which their addresses carry.
    let uid = sport["channels"][0]["source_uid"].as_str().unwrap().to_owned();
    assert_eq!(uid.len(), 6, "{uid}");
    assert_eq!(app.get("/api/live/sources", &admin).await.body[0]["uid"], uid);
    let page = app.get("/api/live/browse?offset=1&limit=1", &bob).await.body;
    assert_eq!((page["total"].as_i64(), names(&page)), (Some(3), vec!["Two HD".to_owned()]));
    let ten = sport["channels"][1]["id"].as_i64().unwrap();
    let around = app.get(&format!("/api/live/browse?limit=2&around={ten}"), &bob).await.body;
    assert_eq!((around["offset"].as_i64(), names(&around)), (Some(2), vec!["Ten".to_owned()]));

    // Found by name, by number, or by what's on.
    assert_eq!(names(&app.get("/api/live/browse?q=foot", &bob).await.body), ["Two HD"]);
    assert_eq!(names(&app.get("/api/live/browse?q=1", &bob).await.body), ["One", "Ten"]);
    assert_eq!(names(&app.get("/api/live/browse?q=ten", &bob).await.body), ["Ten"]);
    // A name's words in any order, and with a slip.
    assert_eq!(names(&app.get("/api/live/browse?q=hd%20two", &bob).await.body), ["Two HD"]);
    assert_eq!(names(&app.get("/api/live/browse?q=twoo", &bob).await.body), ["Two HD"]);
    assert_eq!(app.get("/api/live/browse?category=nope", &bob).await.status, StatusCode::BAD_REQUEST);
    // The app's search finds channels too.
    let found = app.get("/api/search?q=foot", &bob).await.body;
    let found: Vec<&str> = found["channels"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(found, ["Two HD"]);
    let first = app.get("/api/search?q=1&limit=1", &bob).await.body;
    assert_eq!(first["channels"].as_array().map(Vec::len), Some(1), "as many as titles: {first}");

    app.put(&format!("/api/live/channels/{ten}/favorite"), &bob, json!({})).await;
    assert_eq!(app.get("/api/live/groups", &bob).await.body["favorites"], 1);
    assert_eq!(names(&app.get("/api/live/browse?category=favorites", &bob).await.body), ["Ten"]);

    // The guide of a page of channels only.
    let all = app.get("/api/live/browse", &bob).await.body;
    assert_eq!(names(&all)[0], "One");
    let one_id = all["channels"][0]["id"].as_i64().unwrap();
    let guide = app.get(&format!("/api/live/guide?ids={one_id}"), &bob).await.body;
    let keys: Vec<&String> = guide["programs"].as_object().unwrap().keys().collect();
    assert_eq!(keys, [&one_id.to_string()]);
    assert_eq!(app.get("/api/live/guide?ids=x", &bob).await.status, StatusCode::BAD_REQUEST);

    // Administrators' lineup, a page at a time and filtered.
    let page = app.get("/api/live/lineup/page?group=Sport&limit=1", &admin).await.body;
    assert_eq!((page["total"].as_i64(), page["channels"].as_array().unwrap().len()), (Some(2), 1));
    assert_eq!(page["channels"][0]["name"], "Two HD");
    let page = app.get("/api/live/lineup/page?state=no-guide&q=ten", &admin).await.body;
    assert_eq!(page["total"], 1, "{page}");
    assert_eq!(app.get("/api/live/lineup/page", &bob).await.status, StatusCode::FORBIDDEN);
    let hidden =
        app.patch("/api/live/lineup", &admin, json!({"filter": {"group": "Sport", "q": "two"}, "hidden": true})).await;
    assert_eq!(hidden.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/live/lineup/page?state=hidden", &admin).await.body["total"], 1);
    app.patch("/api/live/lineup", &admin, json!({"filter": {"state": "hidden"}, "hidden": false})).await;

    // A whole group hidden, and shown again.
    let hidden = app.patch("/api/live/lineup", &admin, json!({"groups": ["Sport"], "hidden": true})).await;
    assert_eq!(hidden.status, StatusCode::NO_CONTENT);
    let groups = app.get("/api/live/groups", &bob).await.body;
    assert_eq!((groups["total"].as_i64(), groups["groups"].as_array().unwrap().len()), (Some(1), 1));
    assert_eq!(app.get("/api/live", &bob).await.body["channels"], 1, "the count follows at once");
    let admin_groups = app.get("/api/live/lineup/groups", &admin).await.body;
    assert_eq!(
        admin_groups[1],
        json!({"name": "Sport", "region": null, "label": "Sport", "count": 2, "hidden": 2, "offline": 0})
    );
    assert_eq!(app.get("/api/live/lineup/groups", &bob).await.status, StatusCode::FORBIDDEN);
    app.patch("/api/live/lineup", &admin, json!({"groups": ["Sport"], "hidden": false})).await;
    assert_eq!(app.get("/api/live/browse?category=group&group=Sport", &bob).await.body["total"], 2);
}

#[tokio::test]
async fn live_tv_is_open_to_the_household_or_to_people_by_name() {
    let (base, _) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let (carol, carol_id) = app.user(&admin, "carol", json!({"home": false})).await;
    let url = format!("{base}/list.m3u");
    app.post("/api/live/sources", &admin, json!({"name": "P", "kind": "m3u", "url": url})).await;
    wait_until_read(&app, &admin).await;

    let mut settings = app.get("/api/live/settings", &admin).await.body;
    assert_eq!(settings["refresh_hours"], 24, "read again daily");
    settings["access"] = json!("home");
    assert_eq!(app.put("/api/live/settings", &admin, settings.clone()).await.status, StatusCode::OK);
    assert!(may_watch(&app, &bob).await, "the household");
    assert!(!may_watch(&app, &carol).await, "not a friend given access");

    // By name, whatever the rule says.
    settings["allowed_users"] = json!([carol_id]);
    settings["denied_users"] = json!([bob_id]);
    app.put("/api/live/settings", &admin, settings).await;
    assert!(may_watch(&app, &carol).await);
    assert!(!may_watch(&app, &bob).await);
    assert!(may_watch(&app, &admin).await, "administrators always");
}

async fn may_watch(app: &TestApp, token: &str) -> bool {
    app.get("/api/live/channels", token).await.status == StatusCode::OK
}

#[tokio::test]
async fn a_connection_in_use_is_taken_over_by_its_watcher_or_an_administrator() {
    let (base, _) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let url = format!("{base}/list.m3u");
    app.post("/api/live/sources", &admin, json!({"name": "P", "kind": "m3u", "url": url, "max_connections": 1})).await;
    wait_until_read(&app, &admin).await;
    let channels = app.get("/api/live/channels", &bob).await.body;
    let (one, two) = (channels[0]["id"].as_i64().unwrap(), channels[1]["id"].as_i64().unwrap());

    // Bob watches One; the administrator wants Two and may take One's connection.
    // (as a TV's own player, with a keyed address)
    let bobs = app
        .post(&format!("/api/live/channels/{one}/watch"), &bob, json!({"client_id": "tv", "keyed_urls": true}))
        .await
        .body;
    let refused = app.post(&format!("/api/live/channels/{two}/watch"), &admin, json!({"client_id": "pc"})).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    let held = &refused.body["connections"][0];
    assert_eq!((held["channel_id"].as_i64(), held["can_take_over"].as_bool()), (Some(one), Some(true)), "{held}");
    assert_eq!(held["watchers"], json!(["bob"]), "named to administrators");

    // Bob's player hears at once who took it, both being in the home; it's
    // nobody else's to hear.
    let mut events = app.state.events.subscribe();
    let taken = app
        .post(&format!("/api/live/channels/{two}/watch"), &admin, json!({"client_id": "pc", "take_over": one}))
        .await;
    assert_eq!(taken.status, StatusCode::OK, "{:?}", taken.body);
    let session = bobs["session_id"].as_str().unwrap();
    let told = taken_over(&mut events);
    let said = "admin is watching Two HD now, on the connection this channel used.";
    assert_eq!(told, [json!({"type": "live_taken_over", "session_id": session, "channel_id": one, "message": said})]);

    // Bob may not take the administrator's back.
    let refused = app.post(&format!("/api/live/channels/{one}/watch"), &bob, json!({"client_id": "tv"})).await;
    let held = &refused.body["connections"][0];
    assert_eq!((held["can_take_over"].as_bool(), held["watchers"].clone()), (Some(false), json!([])), "{held}");
    let not_his =
        app.post(&format!("/api/live/channels/{one}/watch"), &bob, json!({"client_id": "tv", "take_over": two})).await;
    assert_eq!(not_his.status, StatusCode::FORBIDDEN);

    // His session says so from then on, at its keyed address too.
    let gone = app.get(&format!("/api/live/sessions/{session}/index.m3u8"), &bob).await;
    assert_eq!(gone.status, StatusCode::GONE);
    assert_eq!(gone.body["message"], said, "{:?}", gone.body);
    let keyed = bobs["url"].as_str().unwrap();
    assert!(keyed.starts_with("/api/live/media/"), "{keyed}");
    let gone = app.call(Method::GET, keyed, None, None, &[]).await;
    assert_eq!((gone.status, gone.body["message"].as_str()), (StatusCode::GONE, Some(said)), "{:?}", gone.body);

    // Someone outside the home learns neither who took it nor for what.
    let (carol, _) = app.user(&admin, "carol", json!({"home": false})).await;
    app.delete(&format!("/api/live/sessions/{}", taken.body["session_id"].as_str().unwrap()), &admin).await;
    let carols = app.post(&format!("/api/live/channels/{one}/watch"), &carol, json!({"client_id": "phone"})).await;
    assert_eq!(carols.status, StatusCode::OK, "{:?}", carols.body);
    let mut events = app.state.events.subscribe();
    let taken = app
        .post(&format!("/api/live/channels/{two}/watch"), &admin, json!({"client_id": "pc", "take_over": one}))
        .await;
    assert_eq!(taken.status, StatusCode::OK, "{:?}", taken.body);
    let told = taken_over(&mut events);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0]["message"], "Someone else needed this channel's connection to watch another channel.");

    // Joining the channel someone watches ends nobody.
    let mut events = app.state.events.subscribe();
    let joined = app.post(&format!("/api/live/channels/{two}/watch"), &carol, json!({"client_id": "phone"})).await;
    assert_eq!(joined.status, StatusCode::OK, "{:?}", joined.body);
    assert!(taken_over(&mut events).is_empty());
    let admins = taken.body["session_id"].as_str().unwrap();
    assert_ne!(app.get(&format!("/api/live/sessions/{admins}/index.m3u8"), &admin).await.status, StatusCode::GONE);
}

/// Who is in the home counts as the connection is taken over, not as its
/// watcher started: someone taken out of the home since isn't told who.
#[tokio::test]
async fn the_home_counts_as_the_connection_is_taken_over() {
    let (base, _) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let url = format!("{base}/list.m3u");
    app.post("/api/live/sources", &admin, json!({"name": "P", "kind": "m3u", "url": url, "max_connections": 1})).await;
    wait_until_read(&app, &admin).await;
    let channels = app.get("/api/live/channels", &bob).await.body;
    let (one, two) = (channels[0]["id"].as_i64().unwrap(), channels[1]["id"].as_i64().unwrap());
    let bobs = app.post(&format!("/api/live/channels/{one}/watch"), &bob, json!({"client_id": "tv"})).await;
    assert_eq!(bobs.status, StatusCode::OK, "{:?}", bobs.body);

    // Bob, in the home as he started, is taken out of it while he watches.
    sqlx::query("UPDATE users SET home = 0 WHERE username = 'bob'").execute(&app.state.db.write).await.unwrap();
    let mut events = app.state.events.subscribe();
    let taken = app
        .post(&format!("/api/live/channels/{two}/watch"), &admin, json!({"client_id": "pc", "take_over": one}))
        .await;
    assert_eq!(taken.status, StatusCode::OK, "{:?}", taken.body);
    let told = taken_over(&mut events);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0]["message"], "Someone else needed this channel's connection to watch another channel.");
}

/// The `live_taken_over` events sent so far, as their streams see them.
fn taken_over(events: &mut tokio::sync::broadcast::Receiver<pley::events::ServerEvent>) -> Vec<Value> {
    let mut told = Vec::new();
    while let Ok(event) = events.try_recv() {
        let json = serde_json::to_value(&event).unwrap();
        if json["type"] == "live_taken_over" {
            told.push(json);
        }
    }
    told
}

#[tokio::test]
async fn a_link_signed_in_by_the_cookie_takes_no_connection_over() {
    let (base, _) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let url = format!("{base}/list.m3u");
    app.post("/api/live/sources", &admin, json!({"name": "P", "kind": "m3u", "url": url, "max_connections": 1})).await;
    wait_until_read(&app, &admin).await;
    let channels = app.get("/api/live/channels", &bob).await.body;
    let (one, two) = (channels[0]["id"].as_i64().unwrap(), channels[1]["id"].as_i64().unwrap());
    let bobs = app.post(&format!("/api/live/channels/{one}/watch"), &bob, json!({"client_id": "tv"})).await;
    assert_eq!(bobs.status, StatusCode::OK);

    // Over plain HTTP, another site's link to Two's stream asking for One's
    // connection is a GET like any: with the administrator's cookie, it
    // takes nothing.
    let stream = format!("/api/live/channels/{two}/stream.ts?client_id=pc&take_over={one}");
    let req = Request::builder()
        .uri(&stream)
        .header(header::COOKIE, format!("{}={admin}", pley::auth::COOKIE_NAME))
        .body(Body::empty())
        .unwrap();
    let resp = app.api.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT, "Bob keeps the connection");
    drop(resp);
    // A player sending its token asks on purpose.
    let req = Request::builder()
        .uri(&stream)
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.api.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn channels_that_dont_play_and_divider_rows_are_left_out() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let playlist = format!(
        "#EXTM3U\n#EXTINF:-1 tvg-chno=\"1\" group-title=\"UK| News\",##### UK NEWS #####\n{base}/live/0.ts\n\
         #EXTINF:-1 tvg-chno=\"2\" group-title=\"UK| News\",Plays\n{base}/live/2.ts\n\
         #EXTINF:-1 tvg-chno=\"3\" group-title=\"UK| News\",Gone\n{base}/gone.ts\n"
    );
    let provider = Router::new().route("/list.m3u", get(move || async move { playlist })).route(
        "/live/{n}",
        get(|| async {
            let packets =
                futures::stream::repeat_with(|| Ok::<_, std::io::Error>(bytes::Bytes::from(vec![0x47u8; 188 * 10])));
            ([(header::CONTENT_TYPE, "video/mp2t")], Body::from_stream(packets.take(1000)))
        }),
    );
    tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let url = format!("{base}/list.m3u");
    app.post("/api/live/sources", &admin, json!({"name": "P", "kind": "m3u", "url": url, "max_connections": 0})).await;
    for _ in 0..100 {
        let source = &app.get("/api/live/sources", &admin).await.body[0];
        if source["channels_at"].is_i64() && source["refreshing"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let names = |list: &Value| -> Vec<String> {
        list.as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_owned()).collect()
    };

    assert_eq!(
        names(&app.get("/api/live/channels", &admin).await.body),
        ["Plays", "Gone"],
        "the divider row isn't a channel"
    );
    let gone = app.get("/api/live/channels", &admin).await.body[1]["id"].as_i64().unwrap();
    let watched = app.post(&format!("/api/live/channels/{gone}/watch"), &admin, json!({"client_id": "tv"})).await;
    assert_eq!(watched.status, StatusCode::OK, "{:?}", watched.body);
    // The provider answers 404 however often it is asked: the channel goes.
    let mut left = Vec::new();
    for _ in 0..60 {
        left = names(&app.get("/api/live/channels", &admin).await.body);
        if left.len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(left, ["Plays"]);
    assert_eq!(app.get("/api/live", &admin).await.body["channels"], 1);
    let lineup = app.get("/api/live/lineup", &admin).await.body;
    let offline: Vec<(String, bool)> = lineup
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["name"].as_str().unwrap().to_owned(), c["offline"] == true))
        .collect();
    assert!(
        offline.contains(&("Gone".to_owned(), true)) && offline.contains(&("Plays".to_owned(), false)),
        "{offline:?}"
    );
    assert_eq!(app.get("/api/live/lineup/groups", &admin).await.body[0]["offline"], 2);

    // Shown again when the administrators want to see them all.
    let mut settings = app.get("/api/live/settings", &admin).await.body;
    settings["hide_offline"] = json!(false);
    app.put("/api/live/settings", &admin, settings).await;
    assert_eq!(app.get("/api/live/channels", &admin).await.body.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn sources_are_tested_and_their_errors_said() {
    let (base, _) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let test = app
        .post("/api/live/sources/test", &admin, json!({"name": "P", "kind": "m3u", "url": format!("{base}/list.m3u")}))
        .await;
    assert_eq!(test.status, StatusCode::OK, "{:?}", test.body);
    assert_eq!((test.body["channels"].as_i64(), test.body["groups"].as_i64()), (Some(3), Some(2)));
    assert_eq!(test.body["guide_url"], format!("{base}/guide.xml.gz"));
    let missing = app
        .post(
            "/api/live/sources/test",
            &admin,
            json!({"name": "P", "kind": "m3u", "url": format!("{base}/nothing.m3u")}),
        )
        .await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    assert!(missing.body["message"].as_str().unwrap().contains("404"), "{:?}", missing.body);
    let bad = app.post("/api/live/sources", &admin, json!({"name": "P", "kind": "m3u", "url": "ftp://x"})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let xtream = app
        .post(
            "/api/live/sources",
            &admin,
            json!({"name": "X", "kind": "xtream", "url": format!("{base}/"), "username": "", "password": "p"}),
        )
        .await;
    assert_eq!(xtream.status, StatusCode::BAD_REQUEST, "an Xtream account needs its user name");
    let added = app
        .post(
            "/api/live/sources",
            &admin,
            json!({"name": "Broken", "kind": "m3u", "url": format!("{base}/nothing.m3u")}),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED);
    let mut error = Value::Null;
    for _ in 0..50 {
        error = app.get("/api/live/sources", &admin).await.body[0]["channels_error"].clone();
        if error.is_string() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(error.as_str().is_some_and(|e| e.contains("404")), "{error}");
    let id = added.body["id"].as_i64().unwrap();
    assert_eq!(app.delete(&format!("/api/live/sources/{id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert!(app.get("/api/live/sources", &admin).await.body.as_array().unwrap().is_empty());
}

/// A TV's own player can't send the token: `keyed_urls` gives it an address
/// whose key opens that watching session's files, and nothing else.
#[tokio::test]
async fn keyed_addresses_open_their_session_alone() {
    let (base, _) = provider().await;
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, bob) = app.user(&admin, "bob", json!({})).await;
    let added = app
        .post(
            "/api/live/sources",
            &admin,
            json!({"name": "Provider", "kind": "m3u", "url": format!("{base}/list.m3u"), "max_connections": 2}),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{:?}", added.body);
    wait_until_read(&app, &admin).await;
    let one = app.get("/api/live/channels", &admin).await.body[0]["id"].as_i64().unwrap();

    let (tv, _) = app.app_token(bob, "library play").await;
    let watch = format!("/api/live/channels/{one}/watch");
    let watching = app.app_call(Method::POST, &watch, &tv, Some(json!({"client_id": "tv", "keyed_urls": true}))).await;
    assert_eq!(watching.status, StatusCode::OK, "{:?}", watching.body);
    let url = watching.body["url"].as_str().unwrap().to_owned();
    let session = watching.body["session_id"].as_str().unwrap().to_owned();
    assert!(url.starts_with("/api/live/media/pley_mk_") && url.ends_with("/index.m3u8"), "{url}");
    assert!(!url.contains(&session), "the address carries the key, not the session");

    // Without a token, the key opens the session's segments (none made yet: no transcoder here).
    let segment = url.replace("index.m3u8", "s0.ts");
    let keyed = app.call(Method::GET, &segment, None, None, &[]).await;
    assert_eq!(keyed.status, StatusCode::NOT_FOUND, "{:?}", keyed.body);
    assert_eq!(keyed.body["message"], "the channel is starting again");
    // Another key opens nothing, and the key is no token.
    let wrong = app.call(Method::GET, "/api/live/media/pley_mk_wrong/s0.ts", None, None, &[]).await;
    assert_eq!(wrong.status, StatusCode::GONE);
    let key = url.trim_start_matches("/api/live/media/").split('/').next().unwrap().to_owned();
    assert_eq!(app.get("/api/live/channels", &key).await.status, StatusCode::UNAUTHORIZED);

    // Asked for without keyed_urls, the address needs the token.
    let plain = app.post(&watch, &admin, json!({"client_id": "web"})).await;
    assert!(plain.body["url"].as_str().unwrap().starts_with("/api/live/sessions/"), "{:?}", plain.body);

    // Leaving ends the key.
    let left = app.app_call(Method::DELETE, &format!("/api/live/sessions/{session}"), &tv, None).await;
    assert_eq!(left.status, StatusCode::NO_CONTENT);
    assert_eq!(app.call(Method::GET, &segment, None, None, &[]).await.status, StatusCode::GONE);
}
