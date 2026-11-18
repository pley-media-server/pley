//! A collection's poster: one an administrator uploads wins over the
//! automatic one, until it's reset.

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};

use common::TestApp;

/// A PNG of one color, `width` × `height`.
fn png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(width, height, image::Rgb(rgb)))
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

/// A film library with a collection of its two films; its id and the admin's token.
async fn collection(app: &TestApp) -> (i64, String) {
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Heat (1995)"] {
        app.media_file(&format!("films/{name}.mkv"), 1024);
    }
    let films = app.library(&admin, "Films", "movies", "films").await;
    let ids = app.ids(films, &admin).await;
    let made = app.post(&format!("/api/libraries/{films}/collections"), &admin, json!({"title": "Favorites"})).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    let id = made.body["id"].as_i64().unwrap();
    let added = app
        .post(&format!("/api/collections/{id}/items"), &admin, json!({"item_ids": [ids["Alien"], ids["Heat"]]}))
        .await;
    assert_eq!(added.status, StatusCode::OK, "{:?}", added.body);
    (id, admin)
}

async fn upload(app: &TestApp, id: i64, token: &str, body: Vec<u8>) -> common::Reply {
    app.send_body(Method::POST, &format!("/api/collections/{id}/artwork/poster/upload"), token, "image/png", body).await
}

#[tokio::test]
async fn an_uploaded_poster_wins_until_it_is_reset() {
    let app = TestApp::new().await;
    let (id, admin) = collection(&app).await;

    let before = app.get(&format!("/api/collections/{id}"), &admin).await;
    assert_eq!(before.body["artwork"], json!("automatic"));
    let automatic = app.get(before.body["poster_url"].as_str().unwrap(), &admin).await;
    assert_eq!(automatic.status, StatusCode::TEMPORARY_REDIRECT, "its first film's poster");

    let uploaded = upload(&app, id, &admin, png(200, 300, [200, 40, 40])).await;
    assert_eq!(uploaded.status, StatusCode::OK, "{:?}", uploaded.body);
    assert_eq!(uploaded.body["artwork"], json!("custom"));
    assert_ne!(uploaded.body["poster_url"], before.body["poster_url"], "a new version, past caches");
    let poster = app.get(uploaded.body["poster_url"].as_str().unwrap(), &admin).await;
    assert_eq!(poster.status, StatusCode::OK);
    assert_eq!(poster.headers[header::CONTENT_TYPE], "image/jpeg");
    let shown = image::load_from_memory(&poster.bytes).unwrap().to_rgb8();
    let [r, g, b] = shown.get_pixel(100, 150).0;
    assert!(r > 180 && g < 70 && b < 70, "the uploaded picture: {r} {g} {b}");

    // Listed with its library's collections the same way.
    let listed = app.get(&format!("/api/libraries/{}/collections", uploaded.body["library_id"]), &admin).await;
    let found: &Value = listed.body["collections"].as_array().unwrap().iter().find(|c| c["id"] == json!(id)).unwrap();
    assert_eq!(found["artwork"], json!("custom"));

    let reset = app.delete(&format!("/api/collections/{id}/artwork/poster"), &admin).await;
    assert_eq!(reset.status, StatusCode::OK, "{:?}", reset.body);
    assert_eq!(reset.body["artwork"], json!("automatic"));
    let after = app.get(reset.body["poster_url"].as_str().unwrap(), &admin).await;
    assert_eq!(after.status, StatusCode::TEMPORARY_REDIRECT);
}

#[tokio::test]
async fn only_administrators_upload_and_only_pictures() {
    let app = TestApp::new().await;
    let (id, admin) = collection(&app).await;
    let (viewer, _) = app.user(&admin, "viewer", json!({})).await;

    let denied = upload(&app, id, &viewer, png(20, 30, [0, 0, 0])).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);
    let denied = app.delete(&format!("/api/collections/{id}/artwork/poster"), &viewer).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN);

    let junk = upload(&app, id, &admin, b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec()).await;
    assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    let empty = upload(&app, id, &admin, Vec::new()).await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
    let missing = upload(&app, 9999, &admin, png(20, 30, [0, 0, 0])).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    let still = app.get(&format!("/api/collections/{id}"), &admin).await;
    assert_eq!(still.body["artwork"], json!("automatic"));
}

#[tokio::test]
async fn clean_up_keeps_uploaded_posters() {
    let app = TestApp::new().await;
    let (id, admin) = collection(&app).await;
    let uploaded = upload(&app, id, &admin, png(200, 300, [40, 200, 40])).await;
    assert_eq!(uploaded.status, StatusCode::OK);

    app.state.store.clean_older_than(std::time::Duration::ZERO).await.unwrap();
    let poster = app.get(uploaded.body["poster_url"].as_str().unwrap(), &admin).await;
    assert_eq!(poster.status, StatusCode::OK, "the blob is still there");
}

#[tokio::test]
async fn a_poster_set_through_the_plex_api_is_the_uploaded_one() {
    let app = TestApp::new().await;
    let (id, admin) = collection(&app).await;
    // The Plex-compatible API's ratingKey of collection `id`.
    let key = 1_000_000_000 + id;
    let uploaded = app
        .plex_upload(Method::POST, &format!("/library/collections/{key}/posters"), &admin, png(40, 60, [30, 30, 200]))
        .await;
    assert_eq!(uploaded.status, StatusCode::OK, "{}", uploaded.text());
    let after = app.get(&format!("/api/collections/{id}"), &admin).await;
    assert_eq!(after.body["artwork"], json!("custom"));

    let reset = app.delete(&format!("/api/collections/{id}/artwork/poster"), &admin).await;
    assert_eq!(reset.body["artwork"], json!("automatic"));
}
