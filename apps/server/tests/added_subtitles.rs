//! Subtitles people add to a title: uploaded from the web app, or found on
//! OpenSubtitles (a local stand-in here, answering with the fixtures in
//! `tests/fixtures/opensubtitles`). They are kept in the data folder and
//! listed as the file's own external tracks, to every client.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use parking_lot::Mutex;
use serde_json::{Value, json};

use common::TestApp;

const SEARCH: &str = include_str!("fixtures/opensubtitles/search.json");
const QUOTA: &str = include_str!("fixtures/opensubtitles/download_quota.json");
const SRT: &str = "1\r\n00:00:01,000 --> 00:00:02,000\r\nHello\r\n";

// ------------------------------------------------------------------ OpenSubtitles

/// A request: what was asked, with its Api-Key, User-Agent and Authorization.
type Asked = (String, String, String, String);

#[derive(Clone, Default)]
struct Mock {
    /// "GET /os/subtitles?…" and the rest, for every request.
    requests: Arc<Mutex<Vec<Asked>>>,
    /// The next download is over the day's quota.
    quota: Arc<Mutex<bool>>,
}

impl Mock {
    fn requested(&self, prefix: &str) -> Vec<Asked> {
        self.requests.lock().iter().filter(|r| r.0.starts_with(prefix)).cloned().collect()
    }
}

async fn record(State(mock): State<Mock>, req: Request<Body>, next: axum::middleware::Next) -> Response {
    let line = {
        let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
        (format!("{} {}", req.method(), req.uri()), header("api-key"), header("user-agent"), header("authorization"))
    };
    mock.requests.lock().push(line);
    next.run(req).await
}

async fn start_mock() -> (Mock, String) {
    let mock = Mock::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let link_base = base.clone();
    let app = Router::new()
        .route(
            "/os/subtitles",
            get(|headers: HeaderMap| async move {
                if headers.get("api-key").is_some_and(|k| k == "wrong") {
                    return (StatusCode::UNAUTHORIZED, "{}").into_response();
                }
                ([(header::CONTENT_TYPE, "application/json")], SEARCH).into_response()
            }),
        )
        .route(
            "/os/login",
            post(|axum::Json(body): axum::Json<Value>| async move {
                if body["password"] == json!("right") {
                    axum::Json(json!({"token": "jwt-1", "base_url": "api.opensubtitles.com", "status": 200}))
                        .into_response()
                } else {
                    (StatusCode::UNAUTHORIZED, "{}").into_response()
                }
            }),
        )
        .route(
            "/os/download",
            post(move |State(mock): State<Mock>, axum::Json(body): axum::Json<Value>| {
                let base = link_base.clone();
                async move {
                    if std::mem::take(&mut *mock.quota.lock()) {
                        return (StatusCode::NOT_ACCEPTABLE, QUOTA).into_response();
                    }
                    let id = body["file_id"].as_i64().unwrap();
                    let name = if id == 2081045 { "Joker.2019.SDH.ass" } else { "Joker.2019.1080p.BluRay.x264-SPARKS.srt" };
                    axum::Json(json!({
                        "link": format!("{base}/os/file/{id}"),
                        "file_name": name,
                        "requests": 1,
                        "remaining": 99,
                    }))
                    .into_response()
                }
            }),
        )
        .route(
            "/os/file/{id}",
            get(|Path(id): Path<i64>| async move {
                match id {
                    // Windows-1252, as many older subtitles are.
                    1923552 | 4934512 => b"1\r\n00:00:01,000 --> 00:00:02,000\r\nGar\xe7on!\r\n".to_vec(),
                    2081045 => b"[Script Info]\nScriptType: v4.00+\n\n[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,[DOOR SLAMS]\n".to_vec(),
                    _ => b"not subtitles".to_vec(),
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(mock.clone(), record))
        .with_state(mock.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (mock, base)
}

struct World {
    app: TestApp,
    mock: Mock,
    admin: String,
    joker: i64,
    file: i64,
}

/// pley with OpenSubtitles at the stand-in (the other providers too; they
/// are never asked here) and a movie library with "Joker", whose IMDb id is
/// known.
async fn world() -> World {
    let (mock, base) = start_mock().await;
    let app = TestApp::with_config(|c| {
        c.metadata_endpoints = Some(pley::providers::Endpoints {
            tvmaze: format!("{base}/tvmaze"),
            tvdb: format!("{base}/tvdb"),
            musicbrainz: format!("{base}/mb"),
            cover_art: format!("{base}/caa"),
            tmdb: format!("{base}/tmdb"),
            tmdb_images: format!("{base}/tmdb-images"),
            opensubtitles: format!("{base}/os"),
        });
    })
    .await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Joker (2019)/Joker (2019).mkv", 200_000);
    let library = app.library(&admin, "Movies", "movies", "movies").await;
    let joker = app.ids(library, &admin).await["Joker"];
    sqlx::query("UPDATE items SET imdb_id = 'tt7286456' WHERE id = ?")
        .bind(joker)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let file = app.file_of(joker).await;
    World { app, mock, admin, joker, file }
}

/// A `multipart/form-data` body of these parts (name, file name, contents).
fn form(parts: &[(&str, Option<&str>, &[u8])]) -> (String, Vec<u8>) {
    let boundary = "----pley-test-boundary";
    let mut body = Vec::new();
    for (name, filename, data) in parts {
        body.extend(format!("--{boundary}\r\n").bytes());
        match filename {
            Some(f) => body
                .extend(format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\n\r\n").bytes()),
            None => body.extend(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").bytes()),
        }
        body.extend_from_slice(data);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

async fn upload(app: &TestApp, token: &str, item: i64, parts: &[(&str, Option<&str>, &[u8])]) -> common::Reply {
    let (content_type, body) = form(parts);
    app.send_body(Method::POST, &format!("/api/items/{item}/subtitles"), token, &content_type, body).await
}

/// The subtitle streams of a title's first file, as its page lists them.
async fn listed(app: &TestApp, token: &str, item: i64) -> Vec<Value> {
    let detail = app.get(&format!("/api/items/{item}"), token).await;
    let streams = detail.body["files"][0]["streams"].as_array().cloned().unwrap_or_default();
    streams.into_iter().filter(|s| s["kind"] == json!("subtitle")).collect()
}

#[tokio::test]
async fn uploaded_subtitles_are_kept_in_the_data_folder_and_listed_as_the_files_own() {
    let w = world().await;
    let (app, admin, joker, file) = (&w.app, w.admin.as_str(), w.joker, w.file);
    // UTF-16 with its byte order mark, named from a Windows path.
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain("1\r\n00:00:01,000 --> 00:00:02,000\r\nCafé\r\n".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let added = upload(
        app,
        admin,
        joker,
        &[("file", Some("C:\\Subs\\Joker.Commentary.srt"), &utf16), ("language", None, b"fr")],
    )
    .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    assert_eq!(
        added.body,
        json!({
            "index": 9000,
            "codec": "subrip",
            "language": "fre",
            "title": "Joker.Commentary",
            "default": false,
            "forced": false,
            "external": true,
            "text_based": true,
            "url": format!("/api/files/{file}/subtitles/9000.vtt"),
            "added": true,
            "deletable": true,
        })
    );
    // In the data folder, as UTF-8; nothing next to the film.
    let data = app.state.config.added_subtitles_dir().join(file.to_string());
    let kept: Vec<_> = std::fs::read_dir(&data).unwrap().flatten().map(|e| e.path()).collect();
    assert_eq!(kept.len(), 1);
    assert!(std::fs::read_to_string(&kept[0]).unwrap().contains("Café"));
    let folder = app.media.path().join("movies/Joker (2019)");
    assert_eq!(std::fs::read_dir(folder).unwrap().count(), 1, "only the film");

    // WebVTT named .srt, forced, for SDH: the next index.
    let vtt = b"WEBVTT\n\n00:01.000 --> 00:02.000\nHi\n";
    let second = upload(
        app,
        admin,
        joker,
        &[
            ("file", Some("signs.srt"), vtt),
            ("language", None, b"English"),
            ("forced", None, b"true"),
            ("hearing_impaired", None, b"1"),
        ],
    )
    .await;
    assert_eq!(second.status, StatusCode::CREATED, "{}", second.text());
    assert_eq!(
        (&second.body["index"], &second.body["codec"], &second.body["forced"]),
        (&json!(9001), &json!("webvtt"), &json!(true))
    );
    let streams = listed(app, admin, joker).await;
    let indexes: Vec<&Value> = streams.iter().map(|s| &s["index"]).collect();
    assert_eq!(indexes, [&json!(9000), &json!(9001)]);
    assert!(streams.iter().all(|s| s["added"] == json!(true) && s["external"] == json!(true)));

    // Files that aren't subtitles pley takes, or that are too big, aren't.
    let bad = [
        (&[("file", Some("x.srt"), b"hello".as_slice()), ("language", None, b"en".as_slice())][..], "not subtitles"),
        (&[("file", Some("x.sub"), SRT.as_bytes()), ("language", None, b"en")][..], "not a format taken"),
        (&[("file", Some("x.srt"), SRT.as_bytes())][..], "no language"),
        (&[("file", Some("x.srt"), SRT.as_bytes()), ("language", None, b"not a language")][..], "a wrong language"),
        (&[("language", None, b"en".as_slice())][..], "no file"),
    ];
    for (parts, why) in bad {
        assert_eq!(upload(app, admin, joker, parts).await.status, StatusCode::BAD_REQUEST, "{why}");
    }
    let big = SRT.repeat(2 * 1024 * 1024 / SRT.len() + 1);
    let too_big =
        upload(app, admin, joker, &[("file", Some("big.srt"), big.as_bytes()), ("language", None, b"en")]).await;
    assert!(matches!(too_big.status, StatusCode::BAD_REQUEST | StatusCode::PAYLOAD_TOO_LARGE), "{}", too_big.status);
    assert_eq!(listed(app, admin, joker).await.len(), 2);

    // The Jellyfin-compatible API lists them and serves them, as WebVTT or SubRip.
    let (item, source) = (app.jf_item(joker), app.jf_source(file, admin).await);
    for name in ["Stream.vtt", "Stream.srt"] {
        let stream = app.jf(Method::GET, &format!("/Videos/{item}/{source}/Subtitles/9000/{name}"), "", None).await;
        assert_eq!(stream.status, StatusCode::OK, "{name}");
        assert!(stream.text().contains("Café"), "{}", stream.text());
    }
    let meta = app.jf_json(&format!("/Items/{item}"), admin).await.to_string();
    assert!(meta.contains(&format!("/Videos/{item}/{source}/Subtitles/9001/Stream.")), "{meta}");
}

#[tokio::test]
async fn whoever_added_subtitles_or_an_administrator_deletes_them() {
    let w = world().await;
    let (app, admin, joker) = (&w.app, w.admin.as_str(), w.joker);
    let (ana, _) = app.user(admin, "ana", json!({})).await;
    let (bo, _) = app.user(admin, "bo", json!({})).await;
    let theirs = |token: &str| {
        let token = token.to_owned();
        async move {
            upload(app, &token, joker, &[("file", Some("a.srt"), SRT.as_bytes()), ("language", None, b"en")]).await
        }
    };
    let by_ana = theirs(&ana).await;
    assert_eq!(by_ana.status, StatusCode::CREATED, "anyone who sees the title adds");
    let by_admin = theirs(admin).await;
    let (ana_index, admin_index) = (by_ana.body["index"].as_i64().unwrap(), by_admin.body["index"].as_i64().unwrap());

    // Each sees which they may delete when playing.
    let delete = |token: &str, index: i64| {
        let (uri, token) = (format!("/api/items/{joker}/subtitles/{index}"), token.to_owned());
        async move { app.delete(&uri, &token).await }
    };
    assert_eq!(delete(&bo, ana_index).await.status, StatusCode::FORBIDDEN);
    assert_eq!(delete(&ana, admin_index).await.status, StatusCode::FORBIDDEN);
    assert_eq!(delete(&ana, ana_index).await.status, StatusCode::NO_CONTENT);
    assert_eq!(delete(&ana, ana_index).await.status, StatusCode::NOT_FOUND, "gone");
    let by_bo = theirs(&bo).await;
    assert_eq!(by_bo.body["index"], json!(admin_index + 1), "indexes aren't given twice while the highest stays");
    assert_eq!(delete(admin, by_bo.body["index"].as_i64().unwrap()).await.status, StatusCode::NO_CONTENT);
    // Only added tracks: not the file's own, nor sidecars.
    assert_eq!(delete(admin, 2).await.status, StatusCode::NOT_FOUND);
    assert_eq!(delete(admin, 1000).await.status, StatusCode::NOT_FOUND);
    assert_eq!(listed(app, admin, joker).await.len(), 1);
    // Their files go with them.
    let data = app.state.config.added_subtitles_dir().join(w.file.to_string());
    assert_eq!(std::fs::read_dir(&data).unwrap().count(), 1);

    // A title someone can't see can't be given subtitles by them.
    let (kid, _) = app.user(admin, "kid", json!({"all_libraries": false, "library_ids": []})).await;
    let hidden = upload(app, &kid, joker, &[("file", Some("a.srt"), SRT.as_bytes()), ("language", None, b"en")]).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn subtitle_search_says_when_it_isnt_set_up() {
    let w = world().await;
    let (app, admin, joker) = (&w.app, w.admin.as_str(), w.joker);
    let search = app.get(&format!("/api/items/{joker}/subtitles/search"), admin).await;
    assert_eq!(search.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!((&search.body["code"], &search.body["setup"]), (&json!("unavailable"), &json!(false)));
    assert!(search.body["message"].as_str().unwrap().contains("Settings"));
    let pick =
        app.post(&format!("/api/items/{joker}/subtitles"), admin, json!({"result": "opensubtitles:1923552:en"})).await;
    assert_eq!((pick.status, &pick.body["setup"]), (StatusCode::SERVICE_UNAVAILABLE, &json!(false)));
    assert!(w.mock.requested("").is_empty(), "OpenSubtitles isn't asked");

    // A key OpenSubtitles rejects: set up, but not working.
    app.put("/api/settings", admin, json!({"opensubtitles_api_key": "wrong"})).await;
    let search = app.get(&format!("/api/items/{joker}/subtitles/search"), admin).await;
    assert_eq!((search.status, &search.body["setup"]), (StatusCode::SERVICE_UNAVAILABLE, &json!(true)));
    assert!(search.text().contains("rejected the API key"), "{}", search.text());
}

#[tokio::test]
async fn subtitles_are_found_on_opensubtitles_and_added() {
    let w = world().await;
    let (app, admin, joker, file) = (&w.app, w.admin.as_str(), w.joker, w.file);
    let settings = app
        .put(
            "/api/settings",
            admin,
            json!({"opensubtitles_api_key": "key-1", "opensubtitles_username": "ana", "opensubtitles_password": "right"}),
        )
        .await;
    assert_eq!(settings.status, StatusCode::OK);
    assert_eq!(settings.body["opensubtitles_password_set"], json!(true));
    assert!(!settings.mentions("\"right\""), "the password is never sent back");
    // The user's subtitle language, else English.
    app.put("/api/me/track-defaults", admin, json!({"subtitle_language": "fre"})).await;

    let search = app.get(&format!("/api/items/{joker}/subtitles/search"), admin).await;
    assert_eq!(search.status, StatusCode::OK, "{}", search.text());
    let results = search.body["results"].as_array().unwrap();
    assert_eq!(results[0]["matched"], json!("hash"));
    assert_eq!(
        results[1],
        json!({
            "id": "opensubtitles:1923552:en",
            "provider": "opensubtitles",
            "language": "en",
            "name": "Joker.2019.1080p.WEBRip.x264-[YTS.LT]",
            "format": "srt",
            "hearing_impaired": false,
            "forced": false,
            "downloads": 113298,
            "matched": "title",
        })
    );
    let asked = w.mock.requested("GET /os/subtitles");
    let (line, key, agent, _) = &asked[0];
    let fingerprint: String = sqlx::query_scalar("SELECT fingerprint FROM media_files WHERE id = ?")
        .bind(file)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(line, &format!("GET /os/subtitles?imdb_id=7286456&languages=fr&moviehash={fingerprint}"));
    assert_eq!(key, "key-1");
    assert!(agent.starts_with("pley v"), "{agent}");
    // Languages asked for, in OpenSubtitles' codes.
    app.get(&format!("/api/items/{joker}/subtitles/search?language=eng,por"), admin).await;
    assert!(
        w.mock.requested("GET /os/subtitles")[1].0.contains("languages=en%2Cpt-br%2Cpt-pt"),
        "{:?}",
        w.mock.requested("GET")
    );
    let wrong = app.get(&format!("/api/items/{joker}/subtitles/search?language=en,!!"), admin).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);

    // Picking one downloads it with the account's token, as UTF-8.
    let added =
        app.post(&format!("/api/items/{joker}/subtitles"), admin, json!({"result": "opensubtitles:1923552:en"})).await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.text());
    assert_eq!(
        (&added.body["index"], &added.body["language"], &added.body["added"]),
        (&json!(9000), &json!("eng"), &json!(true))
    );
    assert_eq!(added.body["title"], json!("Joker.2019.1080p.BluRay.x264-SPARKS"));
    let (_, _, _, authorization) = &w.mock.requested("POST /os/download")[0];
    assert_eq!(authorization, "Bearer jwt-1");
    assert_eq!(w.mock.requested("POST /os/login").len(), 1);
    let (item, source) = (app.jf_item(joker), app.jf_source(file, admin).await);
    let stream = app.jf(Method::GET, &format!("/Videos/{item}/{source}/Subtitles/9000/Stream.vtt"), admin, None).await;
    assert!(stream.text().contains("Garçon!"), "{}", stream.text());

    // An SDH ASS, as ASS; the token is used again.
    let sdh = app
        .post(&format!("/api/items/{joker}/subtitles"), admin, json!({"result": "opensubtitles:2081045:en:hi"}))
        .await;
    assert_eq!(sdh.status, StatusCode::CREATED, "{}", sdh.text());
    assert_eq!(sdh.body["codec"], json!("ass"));
    assert_eq!(w.mock.requested("POST /os/login").len(), 1);

    // The day's quota used up: said with OpenSubtitles' words.
    *w.mock.quota.lock() = true;
    let over =
        app.post(&format!("/api/items/{joker}/subtitles"), admin, json!({"result": "opensubtitles:4934512:en"})).await;
    assert_eq!(over.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(over.text().contains("allowed 20 subtitles"), "{}", over.text());
    // What isn't subtitles isn't added; nor are ids that aren't results.
    let junk = app.post(&format!("/api/items/{joker}/subtitles"), admin, json!({"result": "opensubtitles:5:en"})).await;
    assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    let made_up = app.post(&format!("/api/items/{joker}/subtitles"), admin, json!({"result": "elsewhere:5"})).await;
    assert_eq!(made_up.status, StatusCode::BAD_REQUEST);
    assert_eq!(listed(app, admin, joker).await.len(), 2);

    // The settings' test signs the account in.
    let test = app.post("/api/settings/test", admin, json!({"service": "opensubtitles"})).await;
    assert_eq!(test.body["ok"], json!(true), "{}", test.text());
    let test = app
        .post("/api/settings/test", admin, json!({"service": "opensubtitles", "username": "ana", "password": "nope"}))
        .await;
    assert_eq!(test.body["ok"], json!(false));
    assert!(test.body["message"].as_str().unwrap().contains("username or password"), "{}", test.text());
}
