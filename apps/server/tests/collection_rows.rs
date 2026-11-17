//! Collections as rows: the collections Pley makes in each movie and TV
//! library stand in for Home's and Recommended's built-in rows,
//! administrators choose which collections are rows and in what order,
//! collections of one title are one row on Home, and categories group them.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{TestApp, titles};

struct World {
    app: TestApp,
    admin: String,
    films: i64,
    kids: i64,
}

/// Two film libraries, with dated and rated films.
async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in ["Alien (1979)", "Brazil (1985)", "Heat (1995)"] {
        app.media_file(&format!("films/{name}.mkv"), 1024);
    }
    for name in ["Up (2009)", "Coco (2017)"] {
        app.media_file(&format!("kids/{name}.mkv"), 1024);
    }
    let films = app.library(&admin, "Films", "movies", "films").await;
    let kids = app.library(&admin, "Kids", "movies", "kids").await;
    for (title, rating, released) in [
        ("Alien", 8.5, "1979-05-25"),
        ("Brazil", 7.9, "1985-02-20"),
        ("Heat", 8.3, "1995-12-15"),
        ("Up", 8.2, "2009-05-29"),
        ("Coco", 8.4, "2017-10-27"),
    ] {
        sqlx::query("UPDATE items SET rating = ?, release_date = ? WHERE title = ?")
            .bind(rating)
            .bind(released)
            .bind(title)
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    World { app, admin, films, kids }
}

fn hub<'a>(hubs: &'a Value, key: &str) -> Option<&'a Value> {
    hubs["hubs"].as_array().unwrap().iter().find(|h| h["key"] == key)
}

fn keys(hubs: &Value) -> Vec<String> {
    hubs["hubs"].as_array().unwrap().iter().map(|h| h["key"].as_str().unwrap().to_owned()).collect()
}

async fn collections(w: &World, library: i64) -> Vec<Value> {
    let got = w.app.get(&format!("/api/libraries/{library}/collections"), &w.admin).await;
    assert_eq!(got.status, StatusCode::OK, "{:?}", got.body);
    got.body["collections"].as_array().unwrap().clone()
}

async fn builtin(w: &World, library: i64, which: &str) -> i64 {
    collections(w, library).await.iter().find(|c| c["builtin"] == which).unwrap()["id"].as_i64().unwrap()
}

#[tokio::test]
async fn a_new_library_has_pleys_collections_as_its_rows() {
    let w = world().await;
    let made = collections(&w, w.films).await;
    let mut builtins: Vec<_> = made.iter().filter_map(|c| c["builtin"].as_str()).collect();
    builtins.sort();
    assert_eq!(builtins, ["recently_added", "recently_released", "top_rated", "top_unwatched"]);
    for c in made.iter().filter(|c| c["builtin"].is_string()) {
        assert_eq!(c["category"]["key"], "highlights", "{c}");
    }

    let hubs = w.app.get(&format!("/api/libraries/{}/hubs", w.films), &w.admin).await;
    assert_eq!(hubs.status, StatusCode::OK, "{:?}", hubs.body);
    let shown = keys(&hubs.body);
    let at = |key: &str| shown.iter().position(|k| k == key).unwrap_or_else(|| panic!("{key} in {shown:?}"));
    let f = w.films;
    assert!(at(&format!("recently_added:{f}")) < at(&format!("recently_released:{f}")));
    assert!(at(&format!("recently_released:{f}")) < at(&format!("top_rated:{f}")));
    assert!(at(&format!("top_rated:{f}")) < at(&format!("top_unwatched:{f}")));
    let released = hub(&hubs.body, &format!("recently_released:{f}")).unwrap();
    assert_eq!(released["title"], "Recently Released");
    assert_eq!(titles(&released["items"]), ["Heat", "Brazil", "Alien"]);
    let rated = hub(&hubs.body, &format!("top_rated:{f}")).unwrap();
    assert_eq!(titles(&rated["items"]), ["Alien", "Heat", "Brazil"]);
    assert_eq!(rated["collection_ids"], json!([builtin(&w, f, "top_rated").await]));

    // Home has each library's Recently Added, named for its library.
    let home = w.app.get("/api/hubs/home", &w.admin).await;
    let row = hub(&home.body, &format!("recently_added:{f}")).unwrap();
    assert_eq!(row["title"], "Recently Added in Films");
    assert_eq!(titles(&row["items"]).len(), 3);
    assert!(hub(&home.body, &format!("recently_added:{}", w.kids)).is_some());
}

#[tokio::test]
async fn pleys_collections_are_named_in_the_users_language_until_renamed() {
    let w = world().await;
    let rated = builtin(&w, w.films, "top_rated").await;
    let fr = w.app.patch("/api/me", &w.admin, json!({"language": "fr"})).await;
    assert!(fr.status.is_success(), "{:?}", fr.body);
    let got = w.app.get(&format!("/api/collections/{rated}"), &w.admin).await;
    assert_eq!(got.body["title"], "Les mieux notés", "{:?}", got.body);
    assert_eq!(got.body["category"]["title"], "À la une");

    let renamed = w.app.patch(&format!("/api/collections/{rated}"), &w.admin, json!({"title": "Acclaimed"})).await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.body);
    let got = w.app.get(&format!("/api/collections/{rated}"), &w.admin).await;
    assert_eq!(got.body["title"], "Acclaimed");
}

#[tokio::test]
async fn collections_of_one_title_are_one_row_on_home() {
    let w = world().await;
    let mut ids = Vec::new();
    for (library, title) in [(w.films, "Best of"), (w.kids, "best of")] {
        let made = w
            .app
            .post(
                &format!("/api/libraries/{library}/collections"),
                &w.admin,
                json!({"title": title, "smart_filter": "rating!=-1&sort=rating:desc"}),
            )
            .await;
        assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
        let id = made.body["id"].as_i64().unwrap();
        let on = w.app.patch(&format!("/api/collections/{id}"), &w.admin, json!({"on_home": true})).await;
        assert_eq!(on.status, StatusCode::OK, "{:?}", on.body);
        assert!(on.body["home_position"].is_i64(), "{:?}", on.body);
        ids.push(id);
    }
    let key = format!("collections:{},{}", ids[0], ids[1]);
    let home = w.app.get("/api/hubs/home", &w.admin).await;
    let row = hub(&home.body, &key).unwrap_or_else(|| panic!("{key} in {:?}", keys(&home.body)));
    assert_eq!(row["title"], "Best of");
    assert_eq!(row["collection_ids"], json!(ids));
    // One order across both libraries.
    assert_eq!(titles(&row["items"]), ["Alien", "Coco", "Heat", "Up", "Brazil"]);
    let page = w.app.get(&format!("/api/collections/merged?ids={},{}", ids[0], ids[1]), &w.admin).await;
    assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
    assert_eq!(page.body["collections"].as_array().unwrap().len(), 2);
    assert_eq!(titles(&page.body["items"]), ["Alien", "Coco", "Heat", "Up", "Brazil"]);

    // Someone who may open Films alone sees Films' half, as its own row.
    let (viewer, _) = w.app.user(&w.admin, "viewer", json!({"all_libraries": false, "library_ids": [w.films]})).await;
    let home = w.app.get("/api/hubs/home", &viewer).await;
    let row = hub(&home.body, &format!("collection:{}", ids[0])).unwrap();
    assert_eq!(titles(&row["items"]), ["Alien", "Heat", "Brazil"]);
    let page = w.app.get(&format!("/api/collections/merged?ids={},{}", ids[0], ids[1]), &viewer).await;
    assert_eq!(page.body["collections"], json!([{"id": ids[0], "library_id": w.films}]));
    let none = w.app.get(&format!("/api/collections/merged?ids={}", ids[1]), &viewer).await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn administrators_arrange_home_and_recommended() {
    let w = world().await;
    let arranged = w.app.get("/api/home/rows", &w.admin).await;
    assert_eq!(arranged.status, StatusCode::OK, "{:?}", arranged.body);
    let rows: Vec<String> =
        arranged.body["rows"].as_array().unwrap().iter().map(|r| r["key"].as_str().unwrap().to_owned()).collect();
    assert_eq!(rows, [format!("recently_added:{}", w.films), format!("recently_added:{}", w.kids)]);
    let moved = w.app.put("/api/home/rows/order", &w.admin, json!({"rows": [rows[1]]})).await;
    assert_eq!(moved.status, StatusCode::NO_CONTENT, "{:?}", moved.body);
    let home = w.app.get("/api/hubs/home", &w.admin).await;
    let shown: Vec<_> = keys(&home.body).into_iter().filter(|k| k.starts_with("recently_added:")).collect();
    assert_eq!(shown, [rows[1].clone(), rows[0].clone()]);

    // Recommended: the top rated first, and Top Unwatched no longer a row.
    let rated = builtin(&w, w.films, "top_rated").await;
    let unwatched = builtin(&w, w.films, "top_unwatched").await;
    let off = w.app.patch(&format!("/api/collections/{unwatched}"), &w.admin, json!({"on_recommended": false})).await;
    assert_eq!(off.status, StatusCode::OK, "{:?}", off.body);
    let ordered = w
        .app
        .put(&format!("/api/libraries/{}/rows/order", w.films), &w.admin, json!({"collection_ids": [rated]}))
        .await;
    assert_eq!(ordered.status, StatusCode::NO_CONTENT, "{:?}", ordered.body);
    let hubs = w.app.get(&format!("/api/libraries/{}/hubs", w.films), &w.admin).await;
    let shown: Vec<_> = keys(&hubs.body).into_iter().filter(|k| !k.starts_with("continue")).collect();
    let f = w.films;
    assert_eq!(shown, [format!("top_rated:{f}"), format!("recently_added:{f}"), format!("recently_released:{f}")]);

    // A deleted collection of Pley's comes back on request.
    let deleted = w.app.delete(&format!("/api/collections/{rated}"), &w.admin).await;
    assert!(deleted.status.is_success(), "{:?}", deleted.body);
    let restored = w.app.post(&format!("/api/libraries/{f}/collections/defaults"), &w.admin, json!({})).await;
    assert_eq!(restored.body, json!({"restored": 1}));

    // Only administrators arrange.
    let (viewer, _) = w.app.user(&w.admin, "viewer", json!({})).await;
    assert_eq!(w.app.get("/api/home/rows", &viewer).await.status, StatusCode::FORBIDDEN);
    let refused = w.app.patch(&format!("/api/collections/{unwatched}"), &viewer, json!({"on_home": true})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn categories_group_collections() {
    let w = world().await;
    let got = w.app.get("/api/collection-categories", &w.admin).await;
    let names: Vec<_> = got.body["categories"].as_array().unwrap().iter().map(|c| c["title"].clone()).collect();
    assert_eq!(names, [json!("Highlights"), json!("Franchises")]);

    let made = w.app.post("/api/collection-categories", &w.admin, json!({"title": "Holidays"})).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    let holidays = made.body["id"].as_i64().unwrap();
    let again = w.app.post("/api/collection-categories", &w.admin, json!({"title": "highlights"})).await;
    assert_eq!(again.status, StatusCode::CONFLICT);

    let made = w
        .app
        .post(
            &format!("/api/libraries/{}/collections", w.films),
            &w.admin,
            json!({"title": "Christmas", "category_id": holidays}),
        )
        .await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    assert_eq!(made.body["category"]["title"], "Holidays");
    let id = made.body["id"].as_i64().unwrap();
    let wrong = w.app.patch(&format!("/api/collections/{id}"), &w.admin, json!({"category_id": 9999})).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);

    let gone = w.app.delete(&format!("/api/collection-categories/{holidays}"), &w.admin).await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let got = w.app.get(&format!("/api/collections/{id}"), &w.admin).await;
    assert!(got.body["category"].is_null(), "{:?}", got.body);

    let (viewer, _) = w.app.user(&w.admin, "viewer", json!({})).await;
    assert_eq!(w.app.get("/api/collection-categories", &viewer).await.status, StatusCode::OK);
    let refused = w.app.post("/api/collection-categories", &viewer, json!({"title": "Mine"})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_recently_added_put_back_on_home_comes_last_on_its_own() {
    let w = world().await;
    let rated = builtin(&w, w.kids, "top_rated").await;
    let on = w.app.patch(&format!("/api/collections/{rated}"), &w.admin, json!({"on_home": true})).await;
    assert_eq!(on.status, StatusCode::OK, "{:?}", on.body);
    let films = builtin(&w, w.films, "recently_added").await;
    for on in [false, true] {
        let reply = w.app.patch(&format!("/api/collections/{films}"), &w.admin, json!({"on_home": on})).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    }
    let home = w.app.get("/api/hubs/home", &w.admin).await;
    let shown: Vec<_> = keys(&home.body).into_iter().filter(|k| !k.starts_with("continue")).collect();
    let (f, k) = (w.films, w.kids);
    assert_eq!(shown, [format!("recently_added:{k}"), format!("top_rated:{k}"), format!("recently_added:{f}")]);

    // In French, Pley's Top Rated is "Les mieux notés"; a collection of an
    // administrator's called that is a row of its own all the same, as it is
    // for everyone.
    let made = w
        .app
        .post(
            &format!("/api/libraries/{f}/collections"),
            &w.admin,
            json!({"title": "Les mieux notés", "smart_filter": "rating!=-1&sort=rating:desc"}),
        )
        .await;
    let mine = made.body["id"].as_i64().unwrap();
    w.app.patch(&format!("/api/collections/{mine}"), &w.admin, json!({"on_home": true})).await;
    w.app.patch("/api/me", &w.admin, json!({"language": "fr"})).await;
    let home = w.app.get("/api/hubs/home", &w.admin).await;
    assert!(hub(&home.body, &format!("collection:{mine}")).is_some(), "{:?}", keys(&home.body));
    assert!(hub(&home.body, &format!("top_rated:{k}")).is_some(), "{:?}", keys(&home.body));
}
