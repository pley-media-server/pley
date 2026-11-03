//! Collections and playlists through pley's API and the Jellyfin-compatible one,
//! in-process against a temporary data directory like tests/api.rs.

mod common;

use std::collections::HashMap;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

use common::jf_names;
use pley::collections::TmdbCollection;
use pley::{AppState, Config};

struct TestApp {
    api: Router,
    jellyfin: pley::jellyfin::Service,
    state: AppState,
    media: tempfile::TempDir,
    _data: tempfile::TempDir,
    jellyfin_tokens: common::JellyfinTokens,
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
    text: String,
}

impl TestApp {
    async fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let mut config = Config::for_data_dir(data.path());
        config.transcoder_path = Some(data.path().join("no-transcoder-in-tests"));
        let state = AppState::init(config).await.unwrap();
        let api = pley::api::router(state.clone());
        let jellyfin = pley::jellyfin::service(state.clone());
        Self {
            api,
            jellyfin,
            state,
            media: tempfile::tempdir().unwrap(),
            _data: data,
            jellyfin_tokens: Default::default(),
        }
    }

    fn media_file(&self, rel: &str) {
        let path = self.media.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; 1024]).unwrap();
    }

    async fn reply(resp: axum::response::Response) -> Reply {
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 24).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply { status, headers, body, text }
    }

    async fn call(&self, method: Method, uri: &str, token: Option<&str>, body: Option<Value>) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        Self::reply(self.api.clone().oneshot(req).await.unwrap()).await
    }

    async fn get(&self, uri: &str, token: &str) -> Reply {
        self.call(Method::GET, uri, Some(token), None).await
    }

    async fn post(&self, uri: &str, token: &str, body: Value) -> Reply {
        self.call(Method::POST, uri, Some(token), Some(body)).await
    }

    async fn patch(&self, uri: &str, token: &str, body: Value) -> Reply {
        self.call(Method::PATCH, uri, Some(token), Some(body)).await
    }

    async fn put(&self, uri: &str, token: &str, body: Value) -> Reply {
        self.call(Method::PUT, uri, Some(token), Some(body)).await
    }

    async fn delete(&self, uri: &str, token: &str) -> Reply {
        self.call(Method::DELETE, uri, Some(token), None).await
    }

    /// The Jellyfin API, with a token for other apps for a session's (see
    /// `common::JellyfinTokens`), or none for `""`.
    async fn jf(&self, method: Method, uri: &str, token: &str) -> Reply {
        let mut req = Request::builder().method(method).uri(uri);
        if !token.is_empty() {
            req = req.header("x-emby-token", self.jellyfin_tokens.get(&self.state, &self.api, token).await);
        }
        Self::reply(self.jellyfin.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap()).await
    }

    async fn jf_json(&self, uri: &str, token: &str) -> Value {
        let reply = self.jf(Method::GET, uri, token).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.text);
        reply.body
    }

    /// The Jellyfin id of a `kind`'s `number`.
    fn jf_id(&self, kind: pley::jellyfin::ids::Kind, number: i64) -> String {
        let key = std::fs::read_to_string(self.state.config.data_dir.join("secret.key")).unwrap();
        pley::jellyfin::ids::Ids::new(&hex::decode(key.trim()).unwrap()).of(kind, number)
    }

    async fn setup_admin(&self) -> String {
        let r = self
            .call(
                Method::POST,
                "/api/setup",
                None,
                Some(
                    json!({"username": "admin", "password": "secret-one", "setup_code": self.state.auth.setup.shown()}),
                ),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        r.body["token"].as_str().unwrap().to_owned()
    }

    /// A user with a password and the given restrictions: `(token, id)`.
    async fn user(&self, admin: &str, username: &str, mut fields: Value) -> (String, i64) {
        fields["username"] = json!(username);
        fields["password"] = json!("test-pass-1");
        let created = self.post("/api/users", admin, fields).await;
        assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
        let login = self
            .call(Method::POST, "/api/auth/login", None, Some(json!({"username": username, "password": "test-pass-1"})))
            .await;
        assert_eq!(login.status, StatusCode::OK, "{:?}", login.body);
        (login.body["token"].as_str().unwrap().to_owned(), created.body["id"].as_i64().unwrap())
    }

    async fn library(&self, token: &str, name: &str, kind: &str, rel: &str) -> i64 {
        std::fs::create_dir_all(self.media.path().join(rel)).unwrap();
        let path = self.media.path().join(rel).to_string_lossy().into_owned();
        let r = self.post("/api/libraries", token, json!({"name": name, "kind": kind, "paths": [path]})).await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.body);
        for _ in 0..200 {
            if self.state.scanner.statuses().is_empty() {
                return r.body["id"].as_i64().unwrap();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("scan did not finish");
    }

    /// Title → id of the top-level items of a library.
    async fn ids(&self, library: i64, token: &str) -> HashMap<String, i64> {
        let page = self.get(&format!("/api/libraries/{library}/items"), token).await;
        assert_eq!(page.status, StatusCode::OK, "{:?}", page.body);
        page.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| (i["title"].as_str().unwrap().to_owned(), i["id"].as_i64().unwrap()))
            .collect()
    }

    /// A collection made by an administrator.
    async fn collection(&self, admin: &str, library: i64, title: &str, items: &[i64]) -> i64 {
        let r = self
            .post(&format!("/api/libraries/{library}/collections"), admin, json!({"title": title, "item_ids": items}))
            .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.body);
        r.body["id"].as_i64().unwrap()
    }

    async fn collections(&self, library: i64, token: &str) -> Value {
        let r = self.get(&format!("/api/libraries/{library}/collections"), token).await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
        r.body["collections"].clone()
    }

    /// What the metadata agent does after matching a movie.
    async fn link(&self, item: i64, found: Option<&TmdbCollection>) {
        pley::collections::link_tmdb(&self.state.db, &self.state.store, item, found).await.unwrap();
    }

    async fn set_rating(&self, title: &str, rating: &str) {
        sqlx::query("UPDATE items SET content_rating = ?, rating_age = ? WHERE title = ?")
            .bind(rating)
            .bind(pley::ratings::rating_age(rating))
            .bind(title)
            .execute(&self.state.db.write)
            .await
            .unwrap();
    }
}

fn titles(items: &Value) -> Vec<String> {
    items.as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap().to_owned()).collect()
}

fn entry_titles(entries: &Value) -> Vec<String> {
    entries.as_array().unwrap().iter().map(|e| e["item"]["title"].as_str().unwrap().to_owned()).collect()
}

fn entry_ids(entries: &Value) -> Vec<i64> {
    entries.as_array().unwrap().iter().map(|e| e["entry_id"].as_i64().unwrap()).collect()
}

#[tokio::test]
async fn collections_are_curated_by_admins() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alpha (2001).mkv");
    app.media_file("movies/Beta (1999).mkv");
    app.media_file("movies/Gamma (2010).mkv");
    app.media_file("tv/Show (2015)/Season 01/Show - S01E01 - Pilot.mkv");
    app.media_file("music/Artist/Album (2000)/01 - Song.mp3");
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let music = app.library(&admin, "Music", "music", "music").await;
    let m = app.ids(movies, &admin).await;
    let show = app.ids(tv, &admin).await["Show"];

    // Release order by default.
    let created = app
        .post(
            &format!("/api/libraries/{movies}/collections"),
            &admin,
            json!({"title": "Favourites", "summary": "The best", "item_ids": [m["Alpha"], m["Beta"]]}),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["automatic"], json!(false));
    assert_eq!(created.body["summary"], json!("The best"));
    assert_eq!(titles(&created.body["items"]), vec!["Beta", "Alpha"]);
    let id = created.body["id"].as_i64().unwrap();
    let listed = app.get(&format!("/api/libraries/{movies}/collections"), &admin).await;
    assert_eq!(listed.body["collections"][0]["item_count"], json!(2));

    // Only movies and shows of the collection's library go in; no duplicates.
    let other = app.post(&format!("/api/collections/{id}/items"), &admin, json!({"item_ids": [show]})).await;
    assert_eq!(other.status, StatusCode::BAD_REQUEST);
    let added =
        app.post(&format!("/api/collections/{id}/items"), &admin, json!({"item_ids": [m["Gamma"], m["Alpha"]]})).await;
    assert_eq!(added.body["added"], json!(1));
    let again = app.post(&format!("/api/collections/{id}/items"), &admin, json!({"item_ids": [m["Alpha"]]})).await;
    assert_eq!((again.status, again.body["added"].clone()), (StatusCode::OK, json!(0)));

    // By title, then a custom order.
    let patched = app.patch(&format!("/api/collections/{id}"), &admin, json!({"sort": "title", "summary": null})).await;
    assert_eq!(patched.status, StatusCode::OK, "{:?}", patched.body);
    assert_eq!(titles(&patched.body["items"]), vec!["Alpha", "Beta", "Gamma"]);
    assert_eq!(patched.body["summary"], Value::Null);
    let bad = app.patch(&format!("/api/collections/{id}"), &admin, json!({"sort": "random"})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let order =
        app.put(&format!("/api/collections/{id}/order"), &admin, json!({"item_ids": [m["Gamma"], m["Alpha"]]})).await;
    assert_eq!(order.status, StatusCode::NO_CONTENT);
    let detail = app.get(&format!("/api/collections/{id}"), &admin).await;
    assert_eq!(detail.body["sort"], json!("custom"));
    assert_eq!(titles(&detail.body["items"]), vec!["Gamma", "Alpha", "Beta"]);

    // Item pages know their collections.
    let of_item = app.get(&format!("/api/items/{}/collections", m["Gamma"]), &admin).await;
    assert_eq!(of_item.body["collections"], json!([{"id": id, "title": "Favourites"}]));

    // Without artwork of its own, the first item's is used.
    let poster = app.get(&format!("/api/collections/{id}/images/poster?w=320"), &admin).await;
    assert_eq!(poster.status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(poster.headers[header::LOCATION], format!("/api/items/{}/images/poster?w=320", m["Gamma"]).as_str());

    let removed = app.delete(&format!("/api/collections/{id}/items/{}", m["Gamma"]), &admin).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get(&format!("/api/collections/{id}"), &admin).await.body["item_count"], json!(2));

    // Regular users see collections but can't change them; music has none.
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let mine = app.post(&format!("/api/libraries/{movies}/collections"), &bob, json!({"title": "Mine"})).await;
    assert_eq!(mine.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/collections/{id}"), &bob).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get(&format!("/api/collections/{id}"), &bob).await.body["item_count"], json!(2));
    let hits = app.post(&format!("/api/libraries/{music}/collections"), &admin, json!({"title": "Hits"})).await;
    assert_eq!(hits.status, StatusCode::BAD_REQUEST);

    // An empty collection is only shown to administrators.
    let later = app.post(&format!("/api/libraries/{movies}/collections"), &admin, json!({"title": "Later"})).await;
    let later = later.body["id"].as_i64().unwrap();
    assert_eq!(app.get(&format!("/api/collections/{later}"), &bob).await.status, StatusCode::NOT_FOUND);
    let for_bob = app.get(&format!("/api/libraries/{movies}/collections"), &bob).await;
    assert_eq!(titles(&for_bob.body["collections"]), vec!["Favourites"]);
    let for_admin = app.get(&format!("/api/libraries/{movies}/collections"), &admin).await;
    assert_eq!(titles(&for_admin.body["collections"]), vec!["Favourites", "Later"]);

    assert_eq!(app.delete(&format!("/api/collections/{id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get(&format!("/api/collections/{id}"), &admin).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn automatic_collections_follow_tmdb() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/First (2001).mkv");
    app.media_file("movies/Second (2004).mkv");
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let m = app.ids(movies, &admin).await;
    let saga = TmdbCollection { id: 10, name: "Saga Collection".into(), poster_path: None, backdrop_path: None };

    // A single film is not a collection yet.
    app.link(m["First"], Some(&saga)).await;
    assert_eq!(app.collections(movies, &admin).await, json!([]));
    app.link(m["Second"], Some(&saga)).await;
    let listed = app.collections(movies, &admin).await;
    assert_eq!(titles(&listed), vec!["Saga Collection"]);
    assert_eq!(listed[0]["automatic"], json!(true));
    assert_eq!(listed[0]["item_count"], json!(2));
    let id = listed[0]["id"].as_i64().unwrap();
    assert_eq!(
        titles(&app.get(&format!("/api/collections/{id}"), &admin).await.body["items"]),
        vec!["First", "Second"]
    );

    // TMDB renames apply until an administrator changes the title (saving
    // it unchanged doesn't count); then refreshes keep the edited one.
    let same = app
        .patch(&format!("/api/collections/{id}"), &admin, json!({"title": "Saga Collection", "sort": "title"}))
        .await;
    assert_eq!(same.status, StatusCode::OK, "{:?}", same.body);
    let renamed = TmdbCollection { name: "The Saga Collection".into(), ..saga.clone() };
    app.link(m["First"], Some(&renamed)).await;
    assert_eq!(titles(&app.collections(movies, &admin).await), vec!["The Saga Collection"]);
    app.patch(&format!("/api/collections/{id}"), &admin, json!({"title": "The Saga"})).await;
    app.link(m["First"], Some(&saga)).await;
    assert_eq!(titles(&app.collections(movies, &admin).await), vec!["The Saga"]);

    // Films matched elsewhere leave; an emptied collection is removed.
    app.link(m["First"], None).await;
    assert_eq!(app.collections(movies, &admin).await, json!([]));
    let of_second = app.get(&format!("/api/items/{}/collections", m["Second"]), &admin).await;
    assert_eq!(of_second.body["collections"], json!([]));
    app.link(m["Second"], None).await;
    assert_eq!(app.get(&format!("/api/collections/{id}"), &admin).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn hidden_items_stay_hidden_in_collections_and_playlists() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Adult Film (2020).mkv");
    app.media_file("movies/Family Film (2019).mkv");
    app.media_file("movies/Cartoon (2018).mkv");
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    app.set_rating("Adult Film", "R").await;
    app.set_rating("Family Film", "PG").await;
    app.set_rating("Cartoon", "G").await;
    let m = app.ids(movies, &admin).await;
    let (adult, family, cartoon) = (m["Adult Film"], m["Family Film"], m["Cartoon"]);
    let (kid, kid_id) =
        app.user(&admin, "kid", json!({"all_libraries": false, "library_ids": [movies], "max_age": 12})).await;

    let mixed = app.collection(&admin, movies, "Mixed", &[adult, family]).await;
    let grown_ups = app.collection(&admin, movies, "Grown-ups", &[adult]).await;
    let saga = TmdbCollection { id: 7, name: "Saga".into(), poster_path: None, backdrop_path: None };
    app.link(adult, Some(&saga)).await;
    app.link(cartoon, Some(&saga)).await;
    assert_eq!(titles(&app.collections(movies, &admin).await), vec!["Grown-ups", "Mixed", "Saga"]);

    // Collections only show what the kid may watch; ones left with nothing
    // (or an automatic one left with a single film) are not listed.
    let listed = app.collections(movies, &kid).await;
    assert_eq!(titles(&listed), vec!["Mixed"]);
    assert_eq!(listed[0]["item_count"], json!(1));
    let detail = app.get(&format!("/api/collections/{mixed}"), &kid).await;
    assert_eq!(titles(&detail.body["items"]), vec!["Family Film"]);
    assert!(!detail.text.contains("Adult Film"));
    assert_eq!(app.get(&format!("/api/collections/{grown_ups}"), &kid).await.status, StatusCode::NOT_FOUND);
    let image = app.get(&format!("/api/collections/{grown_ups}/images/poster"), &kid).await;
    assert_eq!(image.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&format!("/api/items/{adult}/collections"), &kid).await.status, StatusCode::NOT_FOUND);
    let of_cartoon = app.get(&format!("/api/items/{cartoon}/collections"), &kid).await;
    assert_eq!(of_cartoon.body["collections"], json!([]));
    // The artwork fallback never points at the hidden film.
    let poster = app.get(&format!("/api/collections/{mixed}/images/poster"), &kid).await;
    assert_eq!(poster.headers[header::LOCATION], format!("/api/items/{family}/images/poster").as_str());
    // The Jellyfin API applies the same rules.
    use pley::jellyfin::ids::Kind;
    let library = app.jf_id(Kind::Library, movies);
    let boxsets = app.jf_json(&format!("/Items?ParentId={library}&IncludeItemTypes=BoxSet"), &kid).await;
    assert_eq!(jf_names(&boxsets), vec!["Mixed"]);
    let mixed_id = app.jf_id(Kind::Collection, mixed);
    let children = app.jf_json(&format!("/Items?ParentId={mixed_id}"), &kid).await;
    assert_eq!(jf_names(&children), vec!["Family Film"]);
    let grown_ups_id = app.jf_id(Kind::Collection, grown_ups);
    for uri in [format!("/Items?ParentId={grown_ups_id}"), format!("/Items/{grown_ups_id}")] {
        assert_eq!(app.jf(Method::GET, &uri, &kid).await.status, StatusCode::NOT_FOUND, "{uri}");
    }
    // Its picture, loaded without a token, is chosen as the kid would: by
    // the kid's tag, never the hidden film's; another's tag or none, nothing.
    let tag = boxsets["Items"][0]["ImageTags"]["Primary"].as_str().unwrap().to_owned();
    let picture = |tag: String, token: &'static str| {
        let (app, uri) = (&app, format!("/Items/{mixed_id}/Images/Primary?tag={tag}"));
        async move { app.jf(Method::GET, &uri, token).await.status }
    };
    assert_eq!(picture(tag.clone(), "").await, StatusCode::NOT_FOUND, "the family film has no poster");
    let forged = format!("{}{}", &tag[..tag.len() - 1], if tag.ends_with('0') { '1' } else { '0' });
    assert_eq!(picture(forged, "").await, StatusCode::NOT_FOUND);
    let grown_ups_picture = app.jf(Method::GET, &format!("/Items/{grown_ups_id}/Images/Primary?tag={tag}"), "").await;
    assert_eq!(grown_ups_picture.status, StatusCode::NOT_FOUND, "a collection with nothing the kid may see");

    // Hidden items can't be put in a playlist (the kind comes from the first
    // visible one)...
    let created =
        app.post("/api/playlists", &kid, json!({"title": "Movie night", "item_ids": [adult, family, cartoon]})).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(entry_titles(&created.body["entries"]), vec!["Family Film", "Cartoon"]);
    let playlist = created.body["id"].as_i64().unwrap();

    // ...and entries that become hidden are skipped, not deleted.
    app.set_rating("Family Film", "R").await;
    let detail = app.get(&format!("/api/playlists/{playlist}"), &kid).await;
    assert_eq!(entry_titles(&detail.body["entries"]), vec!["Cartoon"]);
    assert_eq!(detail.body["item_count"], json!(1));
    assert!(!detail.text.contains("Family Film"));
    let listed = app.get("/api/playlists", &kid).await;
    assert_eq!(listed.body["playlists"][0]["item_count"], json!(1));
    let items = app.jf_json(&format!("/Playlists/{}/Items", app.jf_id(Kind::Playlist, playlist)), &kid).await;
    assert_eq!(jf_names(&items), vec!["Cartoon"]);
    // The collection now has nothing the kid may see.
    assert_eq!(app.get(&format!("/api/collections/{mixed}"), &kid).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.collections(movies, &kid).await, json!([]));

    // Lifting the restrictions brings the entry back.
    let lifted =
        app.patch(&format!("/api/users/{kid_id}"), &admin, json!({"max_age": null, "all_libraries": true})).await;
    assert_eq!(lifted.status, StatusCode::OK, "{:?}", lifted.body);
    let detail = app.get(&format!("/api/playlists/{playlist}"), &kid).await;
    assert_eq!(entry_titles(&detail.body["entries"]), vec!["Family Film", "Cartoon"]);

    // Playlists are private, administrators included.
    assert_eq!(app.get(&format!("/api/playlists/{playlist}"), &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.delete(&format!("/api/playlists/{playlist}"), &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/api/playlists", &admin).await.body["playlists"], json!([]));
}

#[tokio::test]
async fn lists_carry_synopses_when_asked() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alpha (2001).mkv");
    app.media_file("tv/Show (2015)/Season 01/Show - S01E01 - Pilot.mkv");
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let alpha = app.ids(movies, &admin).await["Alpha"];
    let show = app.ids(tv, &admin).await["Show"];
    let season = app.get(&format!("/api/items/{show}/children"), &admin).await.body["items"][0]["id"].as_i64().unwrap();
    for (id, text) in [(alpha, "A film."), (season, "A season.")] {
        let edited = app.patch(&format!("/api/items/{id}"), &admin, json!({"summary": text})).await;
        assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.body);
    }
    let collection = app.collection(&admin, movies, "Favourites", &[alpha]).await;
    let playlist = app.post("/api/playlists", &admin, json!({"title": "Tonight", "item_ids": [alpha]})).await;
    let playlist = playlist.body["id"].as_i64().unwrap();
    let recent = format!("recently_added:{movies}");

    // Each list, and where its first title is in the answer.
    let lists = [
        (format!("/api/libraries/{movies}/items?sort=title"), "/items/0"),
        ("/api/hubs/home?x=1".to_owned(), ""),
        (format!("/api/items/{show}/children?x=1"), "/items/0"),
        (format!("/api/collections/{collection}?x=1"), "/items/0"),
        (format!("/api/playlists/{playlist}?x=1"), "/entries/0/item"),
        ("/api/search?q=alpha".to_owned(), "/items/0"),
    ];
    for (path, at) in lists {
        let first = |body: &Value| -> Value {
            if at.is_empty() {
                let hubs = body["hubs"].as_array().unwrap();
                return hubs.iter().find(|h| h["key"] == json!(recent)).unwrap()["items"][0].clone();
            }
            body.pointer(at).unwrap().clone()
        };
        let plain = app.get(&path, &admin).await;
        assert_eq!(plain.status, StatusCode::OK, "{path}: {:?}", plain.body);
        assert_eq!(first(&plain.body).get("summary"), None, "{path} leaves synopses out");
        let asked = app.get(&format!("{path}&summaries=1"), &admin).await;
        let expected = if path.contains("children") { "A season." } else { "A film." };
        assert_eq!(first(&asked.body)["summary"], json!(expected), "{path}");
    }
}

#[tokio::test]
async fn playlists_expand_reorder_and_keep_to_one_kind() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (s, e, title) in [(2, 1, "Return"), (1, 2, "Second"), (1, 1, "Pilot")] {
        app.media_file(&format!("tv/Show (2015)/Season {s:02}/Show - S{s:02}E{e:02} - {title}.mkv"));
    }
    app.media_file("movies/Film (2001).mkv");
    app.media_file("music/Artist/Album (2000)/01 - One.mp3");
    app.media_file("music/Artist/Album (2000)/02 - Two.mp3");
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let music = app.library(&admin, "Music", "music", "music").await;
    let show = app.ids(tv, &admin).await["Show"];
    let film = app.ids(movies, &admin).await["Film"];
    let artist = app.ids(music, &admin).await["Artist"];
    let album =
        app.get(&format!("/api/items/{artist}/children"), &admin).await.body["items"][0]["id"].as_i64().unwrap();

    // A show adds its episodes in order; the kind comes from the items.
    let created = app.post("/api/playlists", &admin, json!({"title": "Binge", "item_ids": [show]})).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["kind"], json!("video"));
    assert_eq!(entry_titles(&created.body["entries"]), vec!["Pilot", "Second", "Return"]);
    let id = created.body["id"].as_i64().unwrap();

    // Music doesn't go in a video playlist; the same film may go in twice.
    let wrong = app.post(&format!("/api/playlists/{id}/items"), &admin, json!({"item_ids": [album]})).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    for _ in 0..2 {
        let added = app.post(&format!("/api/playlists/{id}/items"), &admin, json!({"item_ids": [film]})).await;
        assert_eq!(added.body["added"], json!(1));
    }

    // Reorder (unlisted entries keep their order after the listed ones).
    let entries = entry_ids(&app.get(&format!("/api/playlists/{id}"), &admin).await.body["entries"]);
    assert_eq!(entries.len(), 5);
    let reordered =
        app.put(&format!("/api/playlists/{id}/order"), &admin, json!({"entry_ids": [entries[4], entries[0]]})).await;
    assert_eq!(reordered.status, StatusCode::OK, "{:?}", reordered.body);
    assert_eq!(entry_titles(&reordered.body["entries"]), vec!["Film", "Pilot", "Second", "Return", "Film"]);

    // Remove one entry; rename.
    let removed = app.delete(&format!("/api/playlists/{id}/items/{}", entries[3]), &admin).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    let again = app.delete(&format!("/api/playlists/{id}/items/{}", entries[3]), &admin).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    let renamed = app.patch(&format!("/api/playlists/{id}"), &admin, json!({"title": "  Weekend  "})).await;
    assert_eq!(renamed.body["title"], json!("Weekend"));
    assert_eq!(entry_titles(&renamed.body["entries"]), vec!["Film", "Pilot", "Second", "Return"]);
    let blank = app.patch(&format!("/api/playlists/{id}"), &admin, json!({"title": " "})).await;
    assert_eq!(blank.status, StatusCode::BAD_REQUEST);

    // Music playlists from an album; a kind is needed to start empty.
    let songs = app.post("/api/playlists", &admin, json!({"title": "Songs", "item_ids": [album]})).await;
    assert_eq!(songs.body["kind"], json!("audio"));
    assert_eq!(entry_titles(&songs.body["entries"]), vec!["One", "Two"]);
    let no_kind = app.post("/api/playlists", &admin, json!({"title": "Empty"})).await;
    assert_eq!(no_kind.status, StatusCode::BAD_REQUEST);
    let empty = app.post("/api/playlists", &admin, json!({"title": "Empty", "kind": "audio"})).await;
    assert_eq!(empty.status, StatusCode::CREATED);
    assert_eq!(empty.body["entries"], json!([]));
    // A refused first batch leaves no playlist behind.
    let mixed = app.post("/api/playlists", &admin, json!({"title": "Mixed", "item_ids": [film, album]})).await;
    assert_eq!(mixed.status, StatusCode::BAD_REQUEST);

    let listed = app.get("/api/playlists", &admin).await;
    assert_eq!(titles(&listed.body["playlists"]), vec!["Empty", "Songs", "Weekend"]);
    let weekend = &listed.body["playlists"][2];
    assert_eq!((weekend["kind"].clone(), weekend["item_count"].clone()), (json!("video"), json!(4)));

    assert_eq!(app.delete(&format!("/api/playlists/{id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get(&format!("/api/playlists/{id}"), &admin).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_jellyfin_api_lists_collections_and_playlists_with_their_pictures() {
    use pley::jellyfin::ids::Kind;

    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Alpha (2001)/Alpha (2001).mkv");
    app.media_file("movies/Alpha (2001)/poster.jpg");
    std::fs::write(app.media.path().join("movies/Alpha (2001)/poster.jpg"), tiny_jpeg()).unwrap();
    app.media_file("movies/Beta (2002).mkv");
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let m = app.ids(movies, &admin).await;
    let created = app
        .post(
            &format!("/api/libraries/{movies}/collections"),
            &admin,
            json!({"title": "Pair", "item_ids": [m["Beta"], m["Alpha"]]}),
        )
        .await;
    let c = app.jf_id(Kind::Collection, created.body["id"].as_i64().unwrap());

    // Collections, as BoxSets of their library, and their titles.
    let library = app.jf_id(Kind::Library, movies);
    let listed = app.jf_json(&format!("/Items?ParentId={library}&IncludeItemTypes=BoxSet"), &admin).await;
    let node = &listed["Items"][0];
    assert_eq!((&node["Name"], &node["Type"], &node["Id"]), (&json!("Pair"), &json!("BoxSet"), &json!(c)));
    assert_eq!((&node["IsFolder"], &node["ChildCount"]), (&json!(true), &json!(2)));
    assert_eq!(jf_names(&app.jf_json(&format!("/Items?ParentId={c}"), &admin).await), vec!["Alpha", "Beta"]);
    assert_eq!(app.jf_json(&format!("/Items/{c}"), &admin).await["Name"], json!("Pair"));
    // Its picture comes from its first title with one, without a token.
    let tag = node["ImageTags"]["Primary"].as_str().unwrap();
    let picture = app.jf(Method::GET, &format!("/Items/{c}/Images/Primary?tag={tag}&maxWidth=100"), "").await;
    assert_eq!(picture.status, StatusCode::OK, "{}", picture.text);
    assert_eq!(picture.headers[header::CONTENT_TYPE], "image/jpeg");
    let untagged = app.jf(Method::GET, &format!("/Items/{c}/Images/Primary"), "").await;
    assert_eq!(untagged.status, StatusCode::NOT_FOUND, "whose eyes to choose by isn't known");
    let with_token = app.jf(Method::GET, &format!("/Items/{c}/Images/Primary"), &admin).await;
    assert_eq!(with_token.status, StatusCode::OK);

    // Playlists, by their kind.
    let made = app.post("/api/playlists", &admin, json!({"title": "Mix", "item_ids": [m["Alpha"], m["Beta"]]})).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.body);
    let id = app.jf_id(Kind::Playlist, made.body["id"].as_i64().unwrap());
    let playlists = app.jf_json("/Items?IncludeItemTypes=Playlist&Recursive=true", &admin).await;
    assert_eq!(jf_names(&playlists), vec!["Mix"]);
    assert_eq!((&playlists["Items"][0]["Type"], &playlists["Items"][0]["ChildCount"]), (&json!("Playlist"), &json!(2)));
    assert_eq!(jf_names(&app.jf_json(&format!("/Playlists/{id}/Items"), &admin).await), vec!["Alpha", "Beta"]);
    assert_eq!(app.delete(&format!("/api/playlists/{}", made.body["id"]), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.jf(Method::GET, &format!("/Playlists/{id}/Items"), &admin).await.status, StatusCode::NOT_FOUND);
}

/// The smallest JPEG Pley's image code reads: one white pixel.
fn tiny_jpeg() -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut out)
        .encode(&[255, 255, 255], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    out
}
