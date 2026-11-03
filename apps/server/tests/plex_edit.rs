//! Metadata edits through the Plex-compatible API (docs/COMPATIBILITY.md), sent the
//! way python-plexapi (and so Kometa) sends them: fields and their locks,
//! genres, labels, collections and posters.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, plex_titles};

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
    for name in ["Alien (1979)", "Brazil (1985)"] {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let ids = app.ids(movies, &admin).await;
    sqlx::query("UPDATE items SET genres = ? WHERE id = ?")
        .bind(r#"["Science Fiction"]"#)
        .bind(ids["Alien"])
        .execute(&app.state.db.write)
        .await
        .unwrap();
    World { alien: ids["Alien"], brazil: ids["Brazil"], app, admin, movies }
}

/// `PUT` with an edit's query, as python-plexapi sends it.
async fn put(w: &World, path: &str, query: &str) -> StatusCode {
    w.app.plex(Method::PUT, &format!("{path}?{query}"), &w.admin, &[]).await.status
}

async fn details(w: &World, id: i64) -> Value {
    w.app.plex_json(&format!("/library/metadata/{id}"), &w.admin).await["Metadata"][0].clone()
}

fn tags(item: &Value, element: &str) -> Vec<String> {
    item[element]
        .as_array()
        .map(|t| t.iter().map(|t| t["tag"].as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn fields_are_edited_and_locked() {
    let w = world().await;
    let section = format!("/library/sections/{}/all", w.movies);
    let query = format!(
        "type=1&id={}&title.value=Alien%3A%20Director%27s%20Cut&title.locked=1&summary.value=In%20space&summary.locked=1&year.value=1979&year.locked=1",
        w.alien
    );
    assert_eq!(put(&w, &section, &query).await, StatusCode::OK);
    let item = details(&w, w.alien).await;
    assert_eq!((&item["title"], &item["summary"]), (&json!("Alien: Director's Cut"), &json!("In space")));
    let pley = w.app.get(&format!("/api/items/{}", w.alien), &w.admin).await.body;
    assert_eq!(pley["locked_fields"], json!(["summary", "title", "year"]), "kept through refreshes");
    let locked: Vec<&str> = item["Field"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(locked, ["summary", "title", "year"], "as Plex clients read locks");

    // Unlocked: the metadata agent may fill it again.
    assert_eq!(put(&w, &format!("/library/metadata/{}", w.alien), "summary.locked=0").await, StatusCode::OK);
    let pley = w.app.get(&format!("/api/items/{}", w.alien), &w.admin).await.body;
    assert_eq!(pley["locked_fields"], json!(["title", "year"]));
    // Locked as it is.
    assert_eq!(put(&w, &format!("/library/metadata/{}", w.brazil), "title.locked=1").await, StatusCode::OK);
    let pley = w.app.get(&format!("/api/items/{}", w.brazil), &w.admin).await.body;
    assert_eq!((&pley["title"], &pley["locked_fields"]), (&json!("Brazil"), &json!(["title"])));

    // A value sent unlocked: written, not kept.
    let unlocked = format!("/library/metadata/{}", w.alien);
    assert_eq!(put(&w, &unlocked, "title.value=Alien&title.locked=0").await, StatusCode::OK);
    let pley = w.app.get(&format!("/api/items/{}", w.alien), &w.admin).await.body;
    assert_eq!((&pley["title"], &pley["locked_fields"]), (&json!("Alien"), &json!(["year"])));

    assert_eq!(put(&w, &section, &format!("type=1&id={}&year.value=soon", w.alien)).await, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn genres_labels_and_collections_as_tags() {
    let w = world().await;
    let alien = format!("/library/metadata/{}", w.alien);
    let both = format!("/library/sections/{}/all", w.movies);

    // python-plexapi sends the current genres along with the new ones.
    let added = put(&w, &alien, "genre[0].tag.tag=Science%20Fiction&genre[1].tag.tag=Horror&genre.locked=1").await;
    assert_eq!(added, StatusCode::OK);
    assert_eq!(tags(&details(&w, w.alien).await, "Genre"), ["Science Fiction", "Horror"]);
    // Names to remove come percent-encoded once more.
    assert_eq!(put(&w, &alien, "genre[].tag.tag-=Science%2520Fiction&genre.locked=1").await, StatusCode::OK);
    assert_eq!(tags(&details(&w, w.alien).await, "Genre"), ["Horror"]);

    assert_eq!(put(&w, &alien, "label[0].tag.tag=Overlay&label[1].tag.tag=4K&label.locked=1").await, StatusCode::OK);
    assert_eq!(tags(&details(&w, w.alien).await, "Label"), ["4K", "Overlay"]);
    let found = w.app.plex_json(&format!("{both}?label=overlay"), &w.admin).await;
    assert_eq!(plex_titles(&found), ["Alien"], "found by label, whatever the case");
    let choices = w.app.plex_json(&format!("/library/sections/{}/label", w.movies), &w.admin).await;
    let listed: Vec<&str> =
        choices["Directory"].as_array().unwrap().iter().map(|d| d["title"].as_str().unwrap()).collect();
    assert_eq!(listed, ["4K", "Overlay"]);
    assert_eq!(put(&w, &alien, "label[].tag.tag-=Overlay").await, StatusCode::OK);
    assert_eq!(tags(&details(&w, w.alien).await, "Label"), ["4K"]);

    // A collection by name: made once, joined by both films.
    let query =
        format!("type=1&id={},{}&collection[0].tag.tag=Gilliam%20%26%20Scott&collection.locked=1", w.alien, w.brazil);
    assert_eq!(put(&w, &both, &query).await, StatusCode::OK);
    let collections = w.app.plex_json(&format!("/library/sections/{}/collections", w.movies), &w.admin).await;
    let collection = &collections["Metadata"][0];
    assert_eq!((&collection["title"], &collection["childCount"]), (&json!("Gilliam & Scott"), &json!(2)));
    assert_eq!(tags(&details(&w, w.brazil).await, "Collection"), ["Gilliam & Scott"]);
    let left = format!("/library/metadata/{}", w.brazil);
    assert_eq!(put(&w, &left, "collection[].tag.tag-=Gilliam%2520%2526%2520Scott").await, StatusCode::OK);
    let collections = w.app.plex_json(&format!("/library/sections/{}/collections", w.movies), &w.admin).await;
    assert_eq!(collections["Metadata"][0]["childCount"], json!(1));

    // The collection itself, edited through its section as python-plexapi does.
    let id = collections["Metadata"][0]["ratingKey"].as_str().unwrap().to_owned();
    let query = format!("type=18&id={id}&summary.value=Two%20films&summary.locked=1&titleSort.value=%21001%20Gilliam");
    assert_eq!(put(&w, &both, &query).await, StatusCode::OK);
    let collection = w.app.plex_json(&format!("/library/collections/{id}"), &w.admin).await;
    assert_eq!(collection["Metadata"][0]["summary"], json!("Two films"));
    assert_eq!(collection["Metadata"][0]["titleSort"], json!("!001 Gilliam"), "as it was set");
    let mut png = Vec::new();
    image::RgbImage::from_pixel(40, 60, image::Rgb([30, 30, 200]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let uploaded = w.app.plex_upload(Method::POST, &format!("/library/collections/{id}/posters"), &w.admin, png).await;
    assert_eq!(uploaded.status, StatusCode::OK, "{}", uploaded.text());
    let poster = w.app.plex(Method::GET, &format!("/library/collections/{id}/thumb"), &w.admin, &[]).await;
    assert_eq!(poster.status, StatusCode::OK);
    // Not a collection of this section.
    let elsewhere = "type=18&id=999&summary.value=x";
    assert_eq!(put(&w, &both, elsewhere).await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn posters_are_uploaded() {
    let w = world().await;
    let before = w.app.get(&format!("/api/items/{}", w.alien), &w.admin).await.body["poster_url"].clone();
    let mut png = Vec::new();
    image::RgbImage::from_pixel(40, 60, image::Rgb([200, 30, 30]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let uploaded =
        w.app.plex_upload(Method::POST, &format!("/library/metadata/{}/posters", w.alien), &w.admin, png).await;
    assert_eq!(uploaded.status, StatusCode::OK, "{}", uploaded.text());
    let after = w.app.get(&format!("/api/items/{}", w.alien), &w.admin).await.body;
    assert_ne!(after["poster_url"], before);
    assert_eq!(after["locked_fields"], json!(["poster"]));
    let thumb = w.app.plex(Method::GET, &format!("/library/metadata/{}/thumb", w.alien), &w.admin, &[]).await;
    assert_eq!(thumb.status, StatusCode::OK);

    // Not a picture; a URL the server won't fetch.
    let junk = w
        .app
        .plex_upload(Method::POST, &format!("/library/metadata/{}/posters", w.alien), &w.admin, b"no".to_vec())
        .await;
    assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    let elsewhere = format!("/library/metadata/{}/poster?url=http%3A%2F%2F127.0.0.1%3A9%2Fx.jpg", w.alien);
    assert_eq!(w.app.plex(Method::PUT, &elsewhere, &w.admin, &[]).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn what_pley_doesnt_keep_is_refused_whole() {
    let w = world().await;
    let section = format!("/library/sections/{}/all", w.movies);
    let alien = format!("/library/metadata/{}", w.alien);
    let pley = |id: i64| format!("/api/items/{id}");

    // Kometa's combined edits: a genre with a rating Pley doesn't keep. Nothing is applied.
    for refused in [
        "genre[0].tag.tag=Horror&genre.locked=1&audienceRating.value=8.1&audienceRating.locked=1",
        "genre[0].tag.tag=Horror&rating.value=7.5&rating.locked=1",
        "genre[0].tag.tag=Horror&director[0].tag.tag=Someone&director.locked=1",
        "genre[0].tag.tag=Horror&country[0].tag.tag=France",
        "genre[0].tag.tag=Horror&addedAt.value=1700000000&addedAt.locked=1",
    ] {
        let reply = w.app.plex(Method::PUT, &format!("{alien}?{refused}"), &w.admin, &[]).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{refused}");
        assert!(reply.text().contains("can't be edited"), "{}", reply.text());
    }
    assert_eq!(tags(&details(&w, w.alien).await, "Genre"), ["Science Fiction"]);
    assert_eq!(w.app.get(&pley(w.alien), &w.admin).await.body["locked_fields"], json!([]));

    // One film that isn't there: neither is edited.
    let both = format!("type=1&id={},999999&title.value=Renamed&title.locked=1", w.brazil);
    assert_eq!(put(&w, &section, &both).await, StatusCode::NOT_FOUND);
    assert_eq!(details(&w, w.brazil).await["title"], json!("Brazil"));

    // The user's own rating is kept, as /:/rate keeps it.
    let rated = format!("type=1&id={}&userRating.value=8.0&userRating.locked=1", w.alien);
    assert_eq!(put(&w, &section, &rated).await, StatusCode::OK);
    assert_eq!(details(&w, w.alien).await["userRating"].as_f64(), Some(8.0));
    assert_eq!(put(&w, &alien, "userRating.value=&userRating.locked=1").await, StatusCode::OK);
    assert_eq!(details(&w, w.alien).await.get("userRating"), None);
    assert_eq!(put(&w, &alien, "userRating.value=12").await, StatusCode::BAD_REQUEST);

    // Locks: the genres as they are, the poster given back to the agent.
    assert_eq!(put(&w, &alien, "genre.locked=1").await, StatusCode::OK);
    assert_eq!(w.app.get(&pley(w.alien), &w.admin).await.body["locked_fields"], json!(["genres"]));
    let mut png = Vec::new();
    image::RgbImage::from_pixel(40, 60, image::Rgb([200, 30, 30]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let uploaded = w.app.plex_upload(Method::POST, &format!("{alien}/posters"), &w.admin, png).await;
    assert_eq!(uploaded.status, StatusCode::OK);
    assert_eq!(put(&w, &alien, "thumb.locked=0").await, StatusCode::OK);
    assert_eq!(w.app.get(&pley(w.alien), &w.admin).await.body["locked_fields"], json!(["genres"]));

    // Collections keep their title, sort title, summary and labels alone.
    let query = format!("type=1&id={}&collection[0].tag.tag=Two%20Films", w.alien);
    assert_eq!(put(&w, &section, &query).await, StatusCode::OK);
    let collections = w.app.plex_json(&format!("/library/sections/{}/collections", w.movies), &w.admin).await;
    let key = collections["Metadata"][0]["ratingKey"].as_str().unwrap().to_owned();
    let rated = format!("type=18&id={key}&summary.value=Both&summary.locked=1&contentRating.value=PG");
    assert_eq!(put(&w, &section, &rated).await, StatusCode::BAD_REQUEST);
    let unlocked = format!("type=18&id={key}&summary.value=Both&summary.locked=0");
    assert_eq!(put(&w, &section, &unlocked).await, StatusCode::BAD_REQUEST);
    let collection = w.app.plex_json(&format!("/library/collections/{key}"), &w.admin).await;
    assert_eq!(collection["Metadata"][0].get("summary"), None);

    // Playlists keep their title alone.
    let uri = common::encode(&format!("server://m/com.plexapp.plugins.library/library/metadata/{}", w.alien));
    let made = w.app.plex(Method::POST, &format!("/playlists?type=video&title=Tonight&uri={uri}"), &w.admin, &[]).await;
    let playlist = made.body["MediaContainer"]["Metadata"][0]["ratingKey"].as_str().unwrap().to_owned();
    let renamed = format!("/playlists/{playlist}?title.value=Tomorrow&title.locked=1");
    let renamed = w.app.plex(Method::PUT, &renamed, &w.admin, &[]).await;
    assert_eq!(renamed.body["MediaContainer"]["Metadata"][0]["title"], json!("Tomorrow"));
    let summary = format!("/playlists/{playlist}?summary.value=Later&summary.locked=1");
    assert_eq!(w.app.plex(Method::PUT, &summary, &w.admin, &[]).await.status, StatusCode::BAD_REQUEST);
}

fn png(rgb: [u8; 3]) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbImage::from_pixel(40, 60, image::Rgb(rgb))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    png
}

/// The pictures `/library/metadata/{key}/{list}` lists.
async fn pictures(w: &World, key: i64, list: &str) -> Vec<Value> {
    let listed = w.app.plex_json(&format!("/library/metadata/{key}/{list}"), &w.admin).await;
    listed["Photo"].as_array().cloned().unwrap_or_default()
}

#[tokio::test]
async fn pictures_are_listed_as_python_plexapi_reads_them() {
    let w = world().await;
    assert!(pictures(&w, w.alien, "posters").await.is_empty(), "none yet");

    // The metadata agent's: Kometa takes it as it is.
    sqlx::query("UPDATE items SET poster = 'agent.jpg' WHERE id = ?")
        .bind(w.brazil)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    let agents = pictures(&w, w.brazil, "posters").await;
    assert_eq!(agents.len(), 1);
    assert!(agents[0]["ratingKey"].as_str().unwrap().starts_with("metadata://posters/"), "{agents:?}");
    assert_eq!((&agents[0]["selected"], &agents[0]["provider"]), (&json!(true), &json!("tv.plex.agents.movie")));

    // An upload: Kometa checks it for its overlay before it takes it.
    let posters = format!("/library/metadata/{}/posters", w.alien);
    assert_eq!(w.app.plex_upload(Method::POST, &posters, &w.admin, png([9, 9, 9])).await.status, StatusCode::OK);
    let uploaded = pictures(&w, w.alien, "posters").await;
    let name = uploaded[0]["ratingKey"].as_str().unwrap().to_owned();
    assert!(name.starts_with("upload://posters/"), "{name}");
    let file = uploaded[0]["key"].as_str().unwrap().to_owned();
    assert!(file.starts_with(&format!("/library/metadata/{}/file?url=", w.alien)), "a query Kometa adds its token to");
    let served = w.app.plex(Method::GET, &file, &w.admin, &[]).await;
    assert_eq!(served.status, StatusCode::OK);
    assert!(pictures(&w, w.alien, "arts").await.is_empty());

    // setPoster() of the one shown changes nothing; of one Pley no longer keeps, 400.
    let chosen = format!("/library/metadata/{}/poster?url={}", w.alien, common::encode(&name));
    assert_eq!(w.app.plex(Method::PUT, &chosen, &w.admin, &[]).await.status, StatusCode::OK);
    let stale = format!("/library/metadata/{}/poster?url=upload%3A%2F%2Fposters%2F1", w.alien);
    assert_eq!(w.app.plex(Method::PUT, &stale, &w.admin, &[]).await.status, StatusCode::BAD_REQUEST);

    // A collection's: its first item's until it has its own.
    let query = format!("type=1&id={}&collection[0].tag.tag=Pair", w.brazil);
    assert_eq!(put(&w, &format!("/library/sections/{}/all", w.movies), &query).await, StatusCode::OK);
    let collections = w.app.plex_json(&format!("/library/sections/{}/collections", w.movies), &w.admin).await;
    let key: i64 = collections["Metadata"][0]["ratingKey"].as_str().unwrap().parse().unwrap();
    assert!(pictures(&w, key, "posters").await[0]["ratingKey"].as_str().unwrap().starts_with("metadata://"));
    let own = format!("/library/metadata/{key}/posters");
    assert_eq!(w.app.plex_upload(Method::POST, &own, &w.admin, png([1, 2, 3])).await.status, StatusCode::OK);
    assert!(pictures(&w, key, "posters").await[0]["ratingKey"].as_str().unwrap().starts_with("upload://"));
}

#[tokio::test]
async fn kometas_overlay_mark_is_kept_through_uploads() {
    use image::ImageEncoder;
    let w = world().await;
    let jpeg = |marked: bool| {
        let mut out = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80);
        if marked {
            encoder.set_exif_metadata(pley::images::overlay_exif()).unwrap();
        }
        encoder.encode_image(&image::RgbImage::from_pixel(40, 60, image::Rgb([8, 8, 8]))).unwrap();
        out
    };
    for (id, marked) in [(w.alien, true), (w.brazil, false)] {
        let posters = format!("/library/metadata/{id}/posters");
        assert_eq!(w.app.plex_upload(Method::POST, &posters, &w.admin, jpeg(marked)).await.status, StatusCode::OK);
        // What Kometa downloads to check a poster before it draws on it.
        let shown = w.app.plex(Method::GET, &format!("/library/metadata/{id}/thumb"), &w.admin, &[]).await;
        assert_eq!(shown.status, StatusCode::OK);
        assert_eq!(pley::images::has_overlay_mark(&shown.bytes), marked);
    }
}

#[tokio::test]
async fn an_episodes_poster_is_its_still() {
    let w = world().await;
    w.app.media_file("tv/Taxi (1978)/Season 01/Taxi - S01E01 - Pilot.mkv", 1024);
    let tv = w.app.library(&w.admin, "TV", "shows", "tv").await;
    let show = w.app.ids(tv, &w.admin).await["Taxi"];
    let season = w.app.children(show, &w.admin).await["Season 1"];
    let pilot = w.app.children(season, &w.admin).await["Pilot"];
    let posters = format!("/library/metadata/{pilot}/posters");
    assert_eq!(w.app.plex_upload(Method::POST, &posters, &w.admin, png([5, 5, 5])).await.status, StatusCode::OK);
    let pley = w.app.get(&format!("/api/items/{pilot}"), &w.admin).await.body;
    assert_eq!(pley["locked_fields"], json!(["thumb"]), "what Plex clients show of it");
    assert!(pictures(&w, pilot, "posters").await[0]["ratingKey"].as_str().unwrap().starts_with("upload://"));
}

#[tokio::test]
async fn only_administrators_edit_and_only_in_the_section_named() {
    let w = world().await;
    let (kid, _) = w.app.user(&w.admin, "kid", json!({})).await;
    let edit = format!("/library/metadata/{}?title.value=Mine", w.alien);
    assert_eq!(w.app.plex(Method::PUT, &edit, &kid, &[]).await.status, StatusCode::FORBIDDEN);
    std::fs::create_dir_all(w.app.media.path().join("other")).unwrap();
    let other = w.app.library(&w.admin, "Other", "movies", "other").await;
    let wrong = format!("/library/sections/{other}/all?type=1&id={}&title.value=Moved", w.alien);
    assert_eq!(w.app.plex(Method::PUT, &wrong, &w.admin, &[]).await.status, StatusCode::NOT_FOUND);
    assert_eq!(details(&w, w.alien).await["title"], json!("Alien"));
}
