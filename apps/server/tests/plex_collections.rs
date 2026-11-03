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

fn tags(node: &Value, element: &str) -> Vec<String> {
    node[element]
        .as_array()
        .map(|t| t.iter().map(|t| t["tag"].as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn directory_titles(container: &Value) -> Vec<String> {
    container["Directory"]
        .as_array()
        .map(|d| d.iter().map(|d| d["title"].as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
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

#[tokio::test]
async fn collections_keep_labels_and_are_found_by_them() {
    let w = world().await;
    let made = w.collection("Made By Kometa", &[w.alien]).await;
    let other = w.collection("Kept By Hand", &[w.brazil]).await;
    let section = format!("/library/sections/{}/all", w.movies);

    // As python-plexapi's addLabel() sends it: the current labels and the new one.
    let labelled = format!("{section}?type=18&id={made}&label[0].tag.tag=Kometa&label[1].tag.tag=Space&label.locked=1");
    assert_eq!(w.send(Method::PUT, &labelled).await.status, StatusCode::OK);
    assert_eq!(tags(&w.details(&format!("/library/collections/{made}")).await, "Label"), ["Kometa", "Space"]);
    let listed = w.app.plex_json(&format!("{section}?type=18"), &w.admin).await;
    let listed: Vec<Vec<String>> = listed["Metadata"].as_array().unwrap().iter().map(|c| tags(c, "Label")).collect();
    assert_eq!(listed, [vec![], vec!["Kometa".to_owned(), "Space".to_owned()]], "Kept By Hand, then Made By Kometa");

    // Found by label, as Kometa finds the collections it made.
    let found = w.app.plex_json(&format!("{section}?type=18&label=kometa"), &w.admin).await;
    assert_eq!(plex_titles(&found), ["Made By Kometa"]);
    let found = w.app.plex_json(&format!("{section}?type=18&label!=Kometa"), &w.admin).await;
    assert_eq!(plex_titles(&found), ["Kept By Hand"]);
    let none = w.app.plex_json(&format!("{section}?type=18&label=Elsewhere"), &w.admin).await;
    assert_eq!(none["size"], json!(0));
    // And by title, as python-plexapi's collection() searches.
    let found = w.app.plex_json(&format!("{section}?includeGuids=1&title=by%20hand&type=18"), &w.admin).await;
    assert_eq!(plex_titles(&found), ["Kept By Hand"]);
    let sorted = format!("/library/sections/{}/collections?sort=titleSort:desc", w.movies);
    assert_eq!(plex_titles(&w.app.plex_json(&sorted, &w.admin).await), ["Made By Kometa", "Kept By Hand"]);
    // A filter that can't be applied lists nothing.
    for unknown in ["genre=Horror", "label%3E=Kometa", "push=1&label=Kometa&or=1&label=Space&pop=1"] {
        let refused = w.send(Method::GET, &format!("{section}?type=18&{unknown}")).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{unknown}: {}", refused.text());
    }

    // The section's label choices name collections' labels too.
    let choices = w.app.plex_json(&format!("/library/sections/{}/label?type=1", w.movies), &w.admin).await;
    assert_eq!(directory_titles(&choices), ["Kometa", "Space"]);
    let choices = w.app.plex_json(&format!("/library/sections/{}/label?type=18", w.movies), &w.admin).await;
    assert_eq!(directory_titles(&choices), ["Kometa", "Space"]);
    assert_eq!(choices["Directory"][0]["fastKey"], json!(format!("{section}?type=18&label=Kometa")));

    // Removed as python-plexapi sends it, names percent-encoded once more.
    let removed = format!("{section}?type=18&id={made}&label[].tag.tag-=Space&label.locked=1");
    assert_eq!(w.send(Method::PUT, &removed).await.status, StatusCode::OK);
    assert_eq!(tags(&w.details(&format!("/library/metadata/{made}")).await, "Label"), ["Kometa"]);
    assert_eq!(tags(&w.details(&format!("/library/metadata/{other}")).await, "Label"), Vec::<String>::new());

    let play = w.app.compatibility_token(&w.admin, "play").await;
    assert_eq!(w.app.plex(Method::PUT, &labelled, &play, &[]).await.status, StatusCode::FORBIDDEN);
}

/// The value of setting `id` in `node`'s preferences.
fn preference(node: &Value, id: &str) -> Option<i64> {
    let settings = node["Preferences"]["Setting"].as_array()?;
    settings.iter().find(|s| s["id"] == id).and_then(|s| s["value"].as_i64())
}

#[tokio::test]
async fn collection_order_through_preferences() {
    let w = world().await;
    let casablanca = w.app.ids(w.movies, &w.admin).await["Casablanca"];
    let collection = w.collection("Two Films", &[w.brazil, casablanca]).await;
    let children = format!("/library/collections/{collection}/children");
    assert_eq!(plex_titles(&w.app.plex_json(&children, &w.admin).await), ["Casablanca", "Brazil"], "by release");
    let details = format!("/library/collections/{collection}?includePreferences=1");
    let node = w.details(&details).await;
    assert_eq!((&node["collectionMode"], &node["collectionSort"]), (&json!(-1), &json!(0)), "Plex's defaults");
    assert_eq!((preference(&node, "collectionMode"), preference(&node, "collectionSort")), (Some(-1), Some(0)));
    let modes = node["Preferences"]["Setting"][0]["enumValues"].as_str().unwrap();
    assert!(modes.starts_with("-1:Library default|0:Hide collection"), "{modes}");

    // python-plexapi's sortUpdate("alpha"): Pley's title order.
    let prefs = format!("/library/collections/{collection}/prefs");
    assert_eq!(w.send(Method::PUT, &format!("{prefs}?collectionSort=1")).await.status, StatusCode::OK);
    assert_eq!(w.details(&details).await["collectionSort"], json!(1));
    let id = collection - 1_000_000_000;
    assert_eq!(w.app.get(&format!("/api/collections/{id}"), &w.admin).await.body["sort"], json!("title"));
    assert_eq!(plex_titles(&w.app.plex_json(&children, &w.admin).await), ["Brazil", "Casablanca"]);

    // Modes Pley doesn't have: the default is kept, a change refused whole.
    assert_eq!(w.send(Method::PUT, &format!("{prefs}?collectionMode=-1")).await.status, StatusCode::OK);
    for refused in ["collectionMode=0", "collectionSort=2&collectionMode=1", "collectionSort=7", "hidden=1"] {
        let reply = w.send(Method::PUT, &format!("{prefs}?{refused}")).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{refused}");
    }
    assert_eq!(w.details(&details).await["collectionSort"], json!(1), "unchanged");

    // Smart collections are in their filter's order, as each viewer may see them.
    let listing =
        encode(&format!("server://m/com.plexapp.plugins.library/library/sections/{}/all?type=1&title=a", w.movies));
    let uri = format!("/library/collections?type=1&title=Smart&smart=1&sectionId={}&uri={listing}", w.movies);
    let smart = key(&w.send(Method::POST, &uri).await.body["MediaContainer"]["Metadata"][0]);
    let node = w.details(&format!("/library/collections/{smart}?includePreferences=1")).await;
    assert_eq!(preference(&node, "collectionFilterBasedOnUser"), Some(1));
    let smart_prefs = format!("/library/collections/{smart}/prefs");
    assert_eq!(
        w.send(Method::PUT, &format!("{smart_prefs}?collectionFilterBasedOnUser=1")).await.status,
        StatusCode::OK
    );
    for refused in ["collectionFilterBasedOnUser=0", "collectionSort=1"] {
        assert_eq!(w.send(Method::PUT, &format!("{smart_prefs}?{refused}")).await.status, StatusCode::BAD_REQUEST);
    }

    let play = w.app.compatibility_token(&w.admin, "play").await;
    let reply = w.app.plex(Method::PUT, &format!("{prefs}?collectionSort=0"), &play, &[]).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
}
