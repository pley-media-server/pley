//! Collections and playlists through the Plex-compatible API
//! (docs/COMPATIBILITY.md), as python-plexapi (and so Kometa) reaches them:
//! by `ratingKey`s of their own under `/library/metadata/{key}`.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, encode, plex_titles};

struct World {
    app: TestApp,
    admin: String,
    movies: i64,
    alien: i64,
    brazil: i64,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Brazil (1985)", "Casablanca (1942)"] {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let ids = app.ids(movies, &admin).await;
    World { alien: ids["Alien"], brazil: ids["Brazil"], app, admin, movies }
}

fn items(ids: &[i64]) -> String {
    let ids: Vec<String> = ids.iter().map(i64::to_string).collect();
    encode(&format!("server://m/com.plexapp.plugins.library/library/metadata/{}", ids.join(",")))
}

impl World {
    async fn send(&self, method: Method, uri: &str) -> common::Reply {
        self.app.plex(method, uri, &self.admin, &[]).await
    }

    /// A collection of `ids` made as python-plexapi makes one: its `ratingKey`.
    async fn collection(&self, title: &str, ids: &[i64]) -> i64 {
        let uri = format!(
            "/library/collections?type=1&title={}&smart=0&sectionId={}&uri={}",
            encode(title),
            self.movies,
            items(ids)
        );
        let made = self.send(Method::POST, &uri).await;
        assert_eq!(made.status, StatusCode::OK, "{}", made.text());
        key(&made.body["MediaContainer"]["Metadata"][0])
    }

    /// The collection with `key`, as `/library/collections/{key}` lists it.
    async fn details(&self, path: &str) -> Value {
        self.app.plex_json(path, &self.admin).await["Metadata"][0].clone()
    }
}

fn key(node: &Value) -> i64 {
    node["ratingKey"].as_str().unwrap().parse().unwrap()
}

fn png() -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(40, 60, image::Rgb([30, 120, 30]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

#[tokio::test]
async fn collections_and_playlists_have_keys_of_their_own() {
    let w = world().await;
    let collection = w.collection("Two Films", &[w.alien, w.brazil]).await;
    let id: i64 = sqlx::query_scalar("SELECT id FROM collections").fetch_one(&w.app.state.db.read).await.unwrap();
    assert_eq!(collection, 1_000_000_000 + id);
    assert!(collection > w.alien.max(w.brazil), "not an item's id");

    // Fetched by its key, as python-plexapi's fetchItem() does: the collection, not a film.
    let fetched = w.details(&format!("/library/metadata/{collection}")).await;
    assert_eq!((&fetched["type"], &fetched["title"]), (&json!("collection"), &json!("Two Films")));
    assert_eq!(fetched["key"], json!(format!("/library/collections/{collection}/children")));
    let children = w.app.plex_json(&format!("/library/metadata/{collection}/children"), &w.admin).await;
    assert_eq!(plex_titles(&children), ["Alien", "Brazil"]);
    // Its id as this API gave it before still names it where collections are named.
    assert_eq!(w.details(&format!("/library/collections/{id}")).await["title"], json!("Two Films"));

    // Its poster goes onto the collection, not onto the item that has its id.
    let uploaded =
        w.app.plex_upload(Method::POST, &format!("/library/metadata/{collection}/posters"), &w.admin, png()).await;
    assert_eq!(uploaded.status, StatusCode::OK, "{}", uploaded.text());
    let own: Option<String> =
        sqlx::query_scalar("SELECT poster FROM collections").fetch_one(&w.app.state.db.read).await.unwrap();
    assert!(own.is_some());
    let untouched: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM item_overrides").fetch_one(&w.app.state.db.read).await.unwrap();
    assert_eq!(untouched, 0, "no item was given the picture");
    let thumb = w.send(Method::GET, &format!("/library/metadata/{collection}/thumb")).await;
    assert_eq!(thumb.status, StatusCode::OK);
    let refreshed = w.send(Method::PUT, &format!("/library/metadata/{collection}/refresh")).await;
    assert_eq!(refreshed.status, StatusCode::OK);
    // Its summary, edited by its key.
    let edited =
        w.send(Method::PUT, &format!("/library/metadata/{collection}?summary.value=Both&summary.locked=1")).await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.text());
    assert_eq!(w.details(&format!("/library/collections/{collection}")).await["summary"], json!("Both"));

    // A playlist: a key of its own too, and no picture of its own to upload.
    let made = w.send(Method::POST, &format!("/playlists?type=video&title=Tonight&uri={}", items(&[w.alien]))).await;
    assert_eq!(made.status, StatusCode::OK, "{}", made.text());
    let playlist = key(&made.body["MediaContainer"]["Metadata"][0]);
    assert!(playlist > 2_000_000_000);
    let fetched = w.details(&format!("/library/metadata/{playlist}")).await;
    assert_eq!((&fetched["type"], &fetched["title"]), (&json!("playlist"), &json!("Tonight")));
    let entries = w.app.plex_json(&format!("/playlists/{playlist}/items"), &w.admin).await;
    assert_eq!(plex_titles(&entries), ["Alien"]);
    let upload =
        w.app.plex_upload(Method::POST, &format!("/library/metadata/{playlist}/posters"), &w.admin, png()).await;
    assert_eq!(upload.status, StatusCode::BAD_REQUEST, "{}", upload.text());
    let queue = w.send(Method::POST, &format!("/playQueues?type=video&playlistID={playlist}")).await;
    assert_eq!(plex_titles(&queue.body["MediaContainer"]), ["Alien"]);
    let from_collection = format!(
        "/playQueues?type=video&uri={}",
        encode(&format!("server://m/com.plexapp.plugins.library/library/collections/{collection}"))
    );
    let queue = w.send(Method::POST, &from_collection).await;
    assert_eq!(queue.status, StatusCode::OK, "{}", queue.text());
    assert_eq!(plex_titles(&queue.body["MediaContainer"]), ["Alien", "Brazil"]);

    // Both in one request, beside an item.
    let several = w.app.plex_json(&format!("/library/metadata/{},{collection},{playlist}", w.alien), &w.admin).await;
    assert_eq!(plex_titles(&several), ["Alien", "Two Films", "Tonight"]);

    // Only a token that may manage changes a collection.
    let play = w.app.compatibility_token(&w.admin, "play").await;
    let upload =
        w.app.plex_upload(Method::POST, &format!("/library/metadata/{collection}/posters"), &play, png()).await;
    assert_eq!(upload.status, StatusCode::FORBIDDEN);
    let refresh = w.app.plex(Method::PUT, &format!("/library/metadata/{collection}/refresh"), &play, &[]).await;
    assert_eq!(refresh.status, StatusCode::FORBIDDEN);
}
