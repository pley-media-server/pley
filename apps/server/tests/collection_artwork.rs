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
    assert_eq!(automatic.status, StatusCode::OK, "drawn: its films have no posters");

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
    assert_eq!(after.status, StatusCode::OK);
    assert_eq!(after.bytes, automatic.bytes, "drawn as before");
}

#[tokio::test]
async fn with_no_posters_and_no_icon_the_librarys_icon_is_drawn_on_slate() {
    let app = TestApp::new().await;
    let (id, admin) = collection(&app).await;
    // Its two films have no posters; nor has a show's empty collection.
    app.media_file("series/Taskmaster/Season 01/Taskmaster - S01E01.mkv", 1024);
    let shows = app.library(&admin, "Shows", "shows", "series").await;
    let empty = app.post(&format!("/api/libraries/{shows}/collections"), &admin, json!({"title": "Panel shows"})).await;
    assert_eq!(empty.status, StatusCode::CREATED, "{:?}", empty.body);

    let mut drawn = Vec::new();
    for id in [id, empty.body["id"].as_i64().unwrap()] {
        let detail = app.get(&format!("/api/collections/{id}"), &admin).await;
        assert_eq!(detail.body["artwork"], json!("automatic"));
        let poster = app.get(detail.body["poster_url"].as_str().unwrap(), &admin).await;
        assert_eq!(poster.status, StatusCode::OK, "{id}");
        assert_eq!(poster.headers[header::CONTENT_TYPE], "image/jpeg");
        let shown = image::load_from_memory(&poster.bytes).unwrap().to_rgb8();
        // Slate's diagonal at the edge (gray: its blue a little above its red),
        // and white from the icon in the middle.
        let [r, g, b] = shown.get_pixel(300, 20).0;
        assert!(b > r && b - r < 50 && g.abs_diff(r) < 30, "slate: {r} {g} {b}");
        let center = (380..520).flat_map(|y| (220..380).map(move |x| (x, y)));
        assert!(center.map(|(x, y)| shown.get_pixel(x, y).0).any(|p| p.iter().all(|&c| c > 230)), "an icon");
        drawn.push(poster.bytes);
    }
    assert_ne!(drawn[0], drawn[1], "a clapperboard for films, a TV for shows");
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

/// Gives the films titled `titles` posters of one color each, as if TMDB had.
async fn posters(app: &TestApp, titles: &[(&str, [u8; 3])]) {
    for (title, rgb) in titles {
        let blob = app.state.store.put(&png(200, 300, *rgb), "png").await.unwrap();
        sqlx::query("UPDATE items SET poster = ? WHERE title = ?")
            .bind(blob)
            .bind(title)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
}

/// A film library of four films with posters and a collection of them.
async fn four(app: &TestApp) -> (i64, i64, String) {
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Brazil (1985)", "Heat (1995)", "Up (2009)"] {
        app.media_file(&format!("films/{name}.mkv"), 1024);
    }
    let films = app.library(&admin, "Films", "movies", "films").await;
    posters(app, &[("Alien", [250, 0, 0]), ("Brazil", [0, 250, 0]), ("Heat", [0, 0, 250]), ("Up", [250, 250, 0])])
        .await;
    let ids = app.ids(films, &admin).await;
    let made = app
        .post(
            &format!("/api/libraries/{films}/collections"),
            &admin,
            json!({"title": "Four", "item_ids": [ids["Alien"], ids["Brazil"], ids["Heat"], ids["Up"]]}),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    (films, made.body["id"].as_i64().unwrap(), admin)
}

fn pixel(reply: &common::Reply, x: u32, y: u32) -> [u8; 3] {
    image::load_from_memory(&reply.bytes).unwrap().to_rgb8().get_pixel(x, y).0
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) < 24)
}

#[tokio::test]
async fn four_films_with_posters_make_a_grid_and_an_icon_marks_it() {
    let app = TestApp::new().await;
    let (_, id, admin) = four(&app).await;

    let plain = app.get(&format!("/api/collections/{id}"), &admin).await;
    assert_eq!(plain.body["artwork"], json!("automatic"));
    let grid = app.get(plain.body["poster_url"].as_str().unwrap(), &admin).await;
    assert_eq!(grid.status, StatusCode::OK, "drawn, not a redirect");
    assert_eq!(grid.headers[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(grid.headers[header::CACHE_CONTROL], "private, max-age=300", "it changes with the films");
    assert!(near(pixel(&grid, 100, 100), [250, 0, 0]));
    assert!(near(pixel(&grid, 500, 100), [0, 250, 0]));
    assert!(near(pixel(&grid, 100, 800), [0, 0, 250]));

    let marked =
        app.patch(&format!("/api/collections/{id}"), &admin, json!({"icon": "ghost", "icon_color": "teal"})).await;
    assert_eq!(marked.status, StatusCode::OK, "{:?}", marked.body);
    assert_eq!(
        (&marked.body["artwork"], &marked.body["icon"], &marked.body["icon_color"]),
        (&json!("icon"), &json!("ghost"), &json!("teal"))
    );
    assert_ne!(marked.body["poster_url"], plain.body["poster_url"]);
    let poster = app.get(marked.body["poster_url"].as_str().unwrap(), &admin).await;
    assert!(near(pixel(&poster, 100, 100), [250, 0, 0]), "the top as it was");
    assert!(!near(pixel(&poster, 20, 890), [0, 0, 250]), "the bottom in the color");

    // A new icon keeps its color; null goes back to the grid.
    let star = app.patch(&format!("/api/collections/{id}"), &admin, json!({"icon": "star"})).await;
    assert_eq!(star.body["icon_color"], json!("teal"));
    let cleared = app.patch(&format!("/api/collections/{id}"), &admin, json!({"icon": null})).await;
    assert_eq!((&cleared.body["artwork"], &cleared.body["icon"]), (&json!("automatic"), &Value::Null));
}

#[tokio::test]
async fn two_films_fill_the_grid_side_by_side_without_an_icon() {
    let app = TestApp::new().await;
    let (films, id, admin) = four(&app).await;
    let ids = app.ids(films, &admin).await;
    for title in ["Heat", "Up"] {
        let gone = app.delete(&format!("/api/collections/{id}/items/{}", ids[title]), &admin).await;
        assert_eq!(gone.status, StatusCode::NO_CONTENT);
    }

    let url = app.get(&format!("/api/collections/{id}"), &admin).await.body["poster_url"].as_str().unwrap().to_owned();
    let grid = app.get(&url, &admin).await;
    assert_eq!(grid.status, StatusCode::OK, "drawn, not the first film's");
    for y in [100, 800] {
        assert!(near(pixel(&grid, 100, y), [250, 0, 0]), "Alien on the left: {:?}", pixel(&grid, 100, y));
        assert!(near(pixel(&grid, 500, y), [0, 250, 0]), "Brazil on the right: {:?}", pixel(&grid, 500, y));
    }
}

#[tokio::test]
async fn an_icon_and_an_upload_replace_each_other() {
    let app = TestApp::new().await;
    let (_, id, admin) = four(&app).await;

    let uploaded = upload(&app, id, &admin, png(200, 300, [200, 40, 40])).await;
    assert_eq!(uploaded.body["artwork"], json!("custom"));
    let marked =
        app.patch(&format!("/api/collections/{id}"), &admin, json!({"icon": "heart", "icon_color": "red"})).await;
    assert_eq!(marked.body["artwork"], json!("icon"));
    let uploaded = upload(&app, id, &admin, png(200, 300, [200, 40, 40])).await;
    assert_eq!((&uploaded.body["artwork"], &uploaded.body["icon"]), (&json!("custom"), &Value::Null));

    app.patch(&format!("/api/collections/{id}"), &admin, json!({"icon": "heart"})).await;
    let reset = app.delete(&format!("/api/collections/{id}/artwork/poster"), &admin).await;
    assert_eq!((&reset.body["artwork"], &reset.body["icon"]), (&json!("automatic"), &Value::Null));
}

#[tokio::test]
async fn a_colour_background_draws_the_icon_alone_and_starts_with_the_librarys() {
    let app = TestApp::new().await;
    let (_, id, admin) = four(&app).await;
    let path = format!("/api/collections/{id}");

    let alone = app.patch(&path, &admin, json!({"background": "color", "icon_color": "green"})).await;
    assert_eq!(alone.status, StatusCode::OK, "{:?}", alone.body);
    assert_eq!(
        (&alone.body["artwork"], &alone.body["icon"], &alone.body["icon_color"]),
        (&json!("color"), &json!("clapperboard"), &json!("green")),
        "a film library's icon"
    );
    let poster = app.get(alone.body["poster_url"].as_str().unwrap(), &admin).await;
    assert_eq!(poster.status, StatusCode::OK);
    assert!(!near(pixel(&poster, 100, 100), [250, 0, 0]), "no films' posters");
    let (top, bottom) = (pixel(&poster, 20, 20), pixel(&poster, 580, 880));
    assert!(
        top.iter().map(|&v| u32::from(v)).sum::<u32>() > bottom.iter().map(|&v| u32::from(v)).sum::<u32>(),
        "the color, lighter at the top: {top:?}, {bottom:?}"
    );

    // A new icon keeps the background; posters takes it back over the films'.
    let heart = app.patch(&path, &admin, json!({"icon": "heart"})).await;
    assert_eq!((&heart.body["artwork"], &heart.body["icon_color"]), (&json!("color"), &json!("green")));
    let over = app.patch(&path, &admin, json!({"background": "posters"})).await;
    assert_eq!((&over.body["artwork"], &over.body["icon"]), (&json!("icon"), &json!("heart")));
    let poster = app.get(over.body["poster_url"].as_str().unwrap(), &admin).await;
    assert!(near(pixel(&poster, 100, 100), [250, 0, 0]), "the films' posters again");

    // No icon is the automatic poster, and an upload takes the background away.
    app.patch(&path, &admin, json!({"background": "color"})).await;
    let cleared = app.patch(&path, &admin, json!({"icon": null})).await;
    assert_eq!(cleared.body["artwork"], json!("automatic"));
    app.patch(&path, &admin, json!({"background": "color"})).await;
    upload(&app, id, &admin, png(200, 300, [200, 40, 40])).await;
    let marked = app.patch(&path, &admin, json!({"icon": "star"})).await;
    assert_eq!(marked.body["artwork"], json!("icon"), "not on the color from before the upload");
}

#[tokio::test]
async fn unknown_icons_and_colours_are_refused() {
    let app = TestApp::new().await;
    let (_, id, admin) = four(&app).await;
    for body in [
        json!({"icon": "not-an-icon"}),
        json!({"icon": "star", "icon_color": "mauve"}),
        json!({"icon_color": "red"}),
        json!({"icon": "../../etc/passwd"}),
        json!({"background": "sky"}),
        json!({"background": "color", "icon": null}),
    ] {
        let refused = app.patch(&format!("/api/collections/{id}"), &admin, body.clone()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{body}");
    }
    let still = app.get(&format!("/api/collections/{id}"), &admin).await;
    assert_eq!(still.body["artwork"], json!("automatic"));
}

#[tokio::test]
async fn pleys_own_collections_start_with_their_icons() {
    let app = TestApp::new().await;
    let (films, _, admin) = four(&app).await;
    let listed = app.get(&format!("/api/libraries/{films}/collections"), &admin).await;
    let icons: Vec<(String, String)> = listed.body["collections"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| !c["builtin"].is_null())
        .map(|c| (c["icon"].as_str().unwrap().to_owned(), c["icon_color"].as_str().unwrap().to_owned()))
        .collect();
    assert_eq!(icons.len(), 4);
    for expected in [("clock", "blue"), ("calendar", "teal"), ("star", "amber"), ("eye-off", "pink")] {
        assert!(icons.contains(&(expected.0.into(), expected.1.into())), "{expected:?} in {icons:?}");
    }

    // Deleted and restored, they come back with them.
    let top =
        listed.body["collections"].as_array().unwrap().iter().find(|c| c["builtin"] == json!("top_rated")).unwrap();
    app.delete(&format!("/api/collections/{}", top["id"]), &admin).await;
    app.post(&format!("/api/libraries/{films}/collections/defaults"), &admin, json!({})).await;
    let again = app.get(&format!("/api/libraries/{films}/collections"), &admin).await;
    let top =
        again.body["collections"].as_array().unwrap().iter().find(|c| c["builtin"] == json!("top_rated")).unwrap();
    assert_eq!((&top["icon"], &top["icon_color"]), (&json!("star"), &json!("amber")));
}

#[tokio::test]
async fn the_grid_shows_only_films_the_viewer_may_see() {
    let app = TestApp::new().await;
    let (_, id, admin) = four(&app).await;
    for (title, rating) in [("Alien", "R"), ("Brazil", "PG"), ("Heat", "PG"), ("Up", "G")] {
        app.set_rating(title, Some(rating)).await;
    }
    let (kid, _) = app.user(&admin, "kid", json!({"max_age": 12})).await;
    let seen = app.get(&format!("/api/collections/{id}"), &kid).await;
    let url = seen.body["poster_url"].as_str().unwrap_or_else(|| panic!("{:?}", seen.body)).to_owned();

    // The three films they may see fill it, Alien's red nowhere, with or without an icon.
    for icon in [None, Some("star")] {
        if let Some(icon) = icon {
            app.patch(&format!("/api/collections/{id}"), &admin, json!({ "icon": icon })).await;
        }
        let theirs = app.get(&url, &kid).await;
        assert_eq!(theirs.status, StatusCode::OK, "{icon:?}");
        for (x, y) in [(100, 100), (500, 100), (500, 600)] {
            assert!(!near(pixel(&theirs, x, y), [250, 0, 0]), "{icon:?}: {:?}", pixel(&theirs, x, y));
        }
    }
    let mine = app.get(&url, &admin).await;
    assert!(near(pixel(&mine, 100, 100), [250, 0, 0]));
}

#[tokio::test]
async fn a_poster_file_gone_is_left_out_of_the_grid() {
    let app = TestApp::new().await;
    let (films, id, admin) = four(&app).await;
    app.media_file("films/Coco (2017).mkv", 1024);
    app.post(&format!("/api/libraries/{films}/scan"), &admin, json!({})).await;
    app.wait_for_scans().await;
    posters(&app, &[("Coco", [0, 250, 250])]).await;
    let coco: i64 =
        sqlx::query_scalar("SELECT id FROM items WHERE title = 'Coco'").fetch_one(&app.state.db.read).await.unwrap();
    app.post(&format!("/api/collections/{id}/items"), &admin, json!({"item_ids": [coco]})).await;
    // Alien's poster is named but its file is gone.
    sqlx::query("UPDATE items SET poster = 'blob:0000000000000000000000000000000000000000000000000000000000000000.jpg' WHERE title = 'Alien'")
        .execute(&app.state.db.write)
        .await
        .unwrap();

    let url = app.get(&format!("/api/collections/{id}"), &admin).await.body["poster_url"].as_str().unwrap().to_owned();
    let grid = app.get(&url, &admin).await;
    assert_eq!(grid.status, StatusCode::OK);
    assert!(near(pixel(&grid, 100, 100), [0, 250, 0]), "Brazil first: {:?}", pixel(&grid, 100, 100));
    assert!(near(pixel(&grid, 500, 800), [0, 250, 250]), "Coco takes the fourth place");

    // With only one that can be read and no icon, that film's own.
    sqlx::query("UPDATE items SET poster = (SELECT poster FROM items WHERE title = 'Alien') WHERE title <> 'Brazil'")
        .execute(&app.state.db.write)
        .await
        .unwrap();
    assert_eq!(app.get(&url, &admin).await.status, StatusCode::TEMPORARY_REDIRECT);
    // With an icon, it's drawn with the one there is.
    app.patch(&format!("/api/collections/{id}"), &admin, json!({"icon": "star"})).await;
    let url = app.get(&format!("/api/collections/{id}"), &admin).await.body["poster_url"].as_str().unwrap().to_owned();
    assert_eq!(app.get(&url, &admin).await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_posters_address_changes_with_its_films_and_differs_by_viewer() {
    let app = TestApp::new().await;
    let (films, id, admin) = four(&app).await;
    let (viewer, _) = app.user(&admin, "viewer", json!({})).await;
    let poster = |token: String| {
        let app = &app;
        async move { app.get(&format!("/api/collections/{id}"), &token).await.body["poster_url"].clone() }
    };
    let listed = |token: String| {
        let app = &app;
        async move {
            let all = app.get(&format!("/api/libraries/{films}/collections"), &token).await.body["collections"].clone();
            all.as_array()
                .unwrap()
                .iter()
                .map(|c| (c["builtin"].clone(), c["id"].clone(), c["poster_url"].clone()))
                .collect::<Vec<_>>()
        }
    };

    let before = poster(admin.clone()).await;
    // The same address listed and opened, for a collection that holds its
    // films and for a smart one.
    for (builtin, cid, url) in listed(admin.clone()).await {
        if cid == json!(id) || builtin == json!("recently_added") {
            let opened = app.get(&format!("/api/collections/{cid}"), &admin).await;
            assert_eq!(url, opened.body["poster_url"], "{builtin}");
        }
    }
    // Another viewer's cache never holds this one's.
    assert_ne!(poster(viewer).await, before);

    // Its films changing changes it, with nothing about the collection edited:
    // a new poster for one (as a refresh gives it), then one taken out.
    sqlx::query("UPDATE items SET updated_at = updated_at + 60 WHERE title = 'Heat'")
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let refreshed = poster(admin.clone()).await;
    assert_ne!(refreshed, before);
    let ids = app.ids(films, &admin).await;
    let removed = app.delete(&format!("/api/collections/{id}/items/{}", ids["Up"]), &admin).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_ne!(poster(admin).await, refreshed);
}

#[tokio::test]
async fn a_smart_collections_address_follows_the_films_its_poster_is_drawn_from() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let names: Vec<String> = (1..=10).map(|n| format!("Film {n:02} ({})", 1990 + n)).collect();
    for name in &names {
        app.media_file(&format!("films/{name}.mkv"), 1024);
    }
    let films = app.library(&admin, "Films", "movies", "films").await;
    // Recently Added shows the newest first: eight without posters, then the
    // two with posters its poster is drawn from.
    for n in 1..=10 {
        sqlx::query("UPDATE items SET added_at = ? WHERE title = ?")
            .bind(1_000 * n)
            .bind(format!("Film {n:02}"))
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    posters(&app, &[("Film 01", [250, 0, 0]), ("Film 02", [0, 250, 0])]).await;
    let before = recently_added(&app, films, &admin).await;
    sqlx::query("UPDATE items SET updated_at = updated_at + 60 WHERE title = 'Film 01'")
        .execute(&app.state.db.write)
        .await
        .unwrap();
    assert_ne!(recently_added(&app, films, &admin).await, before, "a new poster for a film past the first eight");
}

/// Recently Added's poster address, the same listed and opened.
async fn recently_added(app: &TestApp, films: i64, admin: &str) -> Value {
    let all = app.get(&format!("/api/libraries/{films}/collections"), admin).await.body["collections"].clone();
    let found = all.as_array().unwrap().iter().find(|c| c["builtin"] == json!("recently_added")).unwrap().clone();
    let opened = app.get(&format!("/api/collections/{}", found["id"]), admin).await;
    assert_eq!(found["poster_url"], opened.body["poster_url"], "listed and opened alike");
    found["poster_url"].clone()
}
