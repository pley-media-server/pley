//! Smart collections: filled by a library filter (Plex's syntax), through
//! pley's API and the Jellyfin-compatible one, as each user may see them.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, jf_names, titles};

struct World {
    app: TestApp,
    admin: String,
    movies: i64,
}

/// Four films with genres and years; Alien is rated R, the others PG.
async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Brazil (1985)", "2001 A Space Odyssey (1968)", "Airplane (1980)"] {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    for (title, genres) in [
        ("Alien", r#"["Horror","Science Fiction"]"#),
        ("Brazil", r#"["Comedy","Science Fiction"]"#),
        ("2001 A Space Odyssey", r#"["Science Fiction"]"#),
        ("Airplane", r#"["Comedy"]"#),
    ] {
        sqlx::query("UPDATE items SET genres = ? WHERE title = ?")
            .bind(genres)
            .bind(title)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    app.set_rating("Alien", Some("R")).await;
    for title in ["Brazil", "2001 A Space Odyssey", "Airplane"] {
        app.set_rating(title, Some("PG")).await;
    }
    World { app, admin, movies }
}

async fn items_of(w: &World, token: &str, id: i64) -> Value {
    let got = w.app.get(&format!("/api/collections/{id}"), token).await;
    assert_eq!(got.status, StatusCode::OK, "{:?}", got.body);
    got.body["items"].clone()
}

#[tokio::test]
async fn smart_collections_fill_themselves() {
    let w = world().await;
    let filter = "genre=Science%20Fiction&sort=year";
    let created = w
        .app
        .post(
            &format!("/api/libraries/{}/collections", w.movies),
            &w.admin,
            json!({"title": "Sci-fi", "smart_filter": filter}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["smart_filter"], json!(filter));
    let id = created.body["id"].as_i64().unwrap();
    assert_eq!(
        titles(&items_of(&w, &w.admin, id).await),
        ["2001 A Space Odyssey", "Alien", "Brazil"],
        "in the filter's order"
    );

    // A film tagged later joins by itself.
    sqlx::query("UPDATE items SET genres = ? WHERE title = 'Airplane'")
        .bind(r#"["Comedy","Science Fiction"]"#)
        .execute(&w.app.state.db.write)
        .await
        .unwrap();
    assert_eq!(titles(&items_of(&w, &w.admin, id).await).len(), 4);
    let listed = w.app.get(&format!("/api/libraries/{}/collections", w.movies), &w.admin).await;
    assert_eq!(listed.body["collections"][0]["item_count"], json!(4));

    // Each viewer sees what they may.
    let (kid, _) = w.app.user(&w.admin, "kid", json!({"max_age": 12})).await;
    assert_eq!(titles(&items_of(&w, &kid, id).await), ["2001 A Space Odyssey", "Airplane", "Brazil"]);

    // Its items come from the filter only; the filter can change.
    let added = w.app.post(&format!("/api/collections/{id}/items"), &w.admin, json!({"item_ids": [1]})).await;
    assert_eq!(added.status, StatusCode::BAD_REQUEST);
    let changed =
        w.app.patch(&format!("/api/collections/{id}"), &w.admin, json!({"smart_filter": "genre=Comedy"})).await;
    assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.body);
    assert_eq!(titles(&items_of(&w, &w.admin, id).await), ["Airplane", "Brazil"]);
    let bad = w.app.patch(&format!("/api/collections/{id}"), &w.admin, json!({"smart_filter": "type=8"})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "artists aren't in a movie library");
}

#[tokio::test]
async fn smart_collections_through_the_jellyfin_api() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let created = app
        .post(
            &format!("/api/libraries/{}/collections", w.movies),
            admin,
            json!({"title": "Sci-fi", "smart_filter": "genre=Science%20Fiction&sort=year"}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    let movies = app.jf_library(w.movies);
    let listed = app.jf_json(&format!("/Items?ParentId={movies}&IncludeItemTypes=BoxSet"), admin).await;
    assert_eq!(jf_names(&listed), ["Sci-fi"]);
    let collection = &listed["Items"][0];
    assert_eq!((&collection["Type"], &collection["ChildCount"]), (&json!("BoxSet"), &json!(3)));
    let id = collection["Id"].as_str().unwrap();
    let items = app.jf_json(&format!("/Items?ParentId={id}"), admin).await;
    assert_eq!(jf_names(&items), ["2001 A Space Odyssey", "Alien", "Brazil"], "in the filter's order");
    // Each viewer sees what they may.
    let (kid, _) = app.user(admin, "kid", json!({"max_age": 12})).await;
    assert_eq!(app.jf_json(&format!("/Items/{id}"), &kid).await["ChildCount"], json!(2));
    assert_eq!(
        jf_names(&app.jf_json(&format!("/Items?ParentId={id}"), &kid).await),
        ["2001 A Space Odyssey", "Brazil"]
    );
}

#[tokio::test]
async fn smart_playlists_through_the_jellyfin_api() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let created = app
        .post(
            "/api/playlists",
            admin,
            json!({"title": "Scary", "smart_filter": "genre=Horror", "library_id": w.movies}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    let listed = app.jf_json("/Items?IncludeItemTypes=Playlist&Recursive=true", admin).await;
    assert_eq!(jf_names(&listed), ["Scary"]);
    let id = listed["Items"][0]["Id"].as_str().unwrap();
    assert_eq!(jf_names(&app.jf_json(&format!("/Playlists/{id}/Items"), admin).await), ["Alien"]);
    assert_eq!(jf_names(&app.jf_json(&format!("/Items?ParentId={id}"), admin).await), ["Alien"]);
    // Someone else's playlist stays theirs.
    let (bob, _) = app.user(admin, "bob", json!({})).await;
    assert!(jf_names(&app.jf_json("/Items?IncludeItemTypes=Playlist&Recursive=true", &bob).await).is_empty());
    let theirs = app.jf(Method::GET, &format!("/Playlists/{id}/Items"), &bob, None).await;
    assert_eq!(theirs.status, StatusCode::NOT_FOUND);
}

fn entry_titles(playlist: &Value) -> Vec<String> {
    playlist["entries"].as_array().unwrap().iter().map(|e| e["item"]["title"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn smart_playlists_play_what_their_filter_finds() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let created = app
        .post(
            "/api/playlists",
            admin,
            json!({"title": "Comedies", "smart_filter": "genre=Comedy&sort=titleSort", "library_id": w.movies}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(
        (&created.body["kind"], &created.body["smart_filter"]),
        (&json!("video"), &json!("genre=Comedy&sort=titleSort"))
    );
    let id = created.body["id"].as_i64().unwrap();
    assert_eq!(entry_titles(&created.body), ["Airplane", "Brazil"]);
    let added = app.post(&format!("/api/playlists/{id}/items"), admin, json!({"item_ids": [1]})).await;
    assert_eq!(added.status, StatusCode::BAD_REQUEST, "filled by its filter");
    let changed = app.patch(&format!("/api/playlists/{id}"), admin, json!({"smart_filter": "genre=Horror"})).await;
    assert_eq!(entry_titles(&changed.body), ["Alien"]);
    // Someone else's playlist stays theirs.
    let (bob, _) = app.user(admin, "bob", json!({})).await;
    assert_eq!(app.get(&format!("/api/playlists/{id}"), &bob).await.status, StatusCode::NOT_FOUND);

    // Shows play as their episodes.
    for e in [1, 2] {
        app.media_file(&format!("tv/Taxi (1978)/Season 01/Taxi - S01E0{e}.mkv"), 1024);
    }
    let tv = app.library(admin, "TV", "shows", "tv").await;
    let shows = app
        .post("/api/playlists", admin, json!({"title": "Taxi", "smart_filter": "type=2&title=taxi", "library_id": tv}))
        .await;
    assert_eq!(shows.status, StatusCode::CREATED, "{:?}", shows.body);
    let kinds: Vec<&str> =
        shows.body["entries"].as_array().unwrap().iter().map(|e| e["item"]["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["episode", "episode"]);
}
