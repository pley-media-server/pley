//! NFO files next to titles, as Kodi, Jellyfin and Radarr write them: their
//! ids decide the match, their fields win over the online source, and hand
//! edits and Fix Match win over them. TMDB and TVmaze are a local stand-in
//! that answers for a handful of titles; nothing leaves the machine.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use serde_json::{Value, json};

use common::TestApp;

const KEY: &str = "test-key";

#[derive(Clone, Default)]
struct Mock {
    /// "/tmdb/search/movie?query=Alien&…" for every request.
    requests: Arc<Mutex<Vec<String>>>,
}

impl Mock {
    fn requested(&self, prefix: &str) -> Vec<String> {
        self.requests.lock().iter().filter(|r| r.starts_with(prefix)).cloned().collect()
    }
}

fn movie(id: i64) -> Option<Value> {
    let (title, date, imdb) = match id {
        348 => ("Alien", "1979-05-25", "tt0078748"),
        679 => ("Aliens", "1986-07-18", "tt0090605"),
        8077 => ("Alien³", "1992-05-22", "tt0103644"),
        603 => ("The Matrix", "1999-03-31", "tt0133093"),
        _ => return None,
    };
    Some(json!({
        "title": title,
        "original_title": title,
        "overview": format!("The story of {title}."),
        "tagline": "",
        "vote_average": 7.5,
        "release_date": date,
        "runtime": 120,
        "genres": [{"name": "Science Fiction"}],
        "production_companies": [{"name": "A Studio"}],
        "poster_path": null,
        "backdrop_path": null,
        "imdb_id": imdb,
        "release_dates": {"results": [{"iso_3166_1": "US", "release_dates": [{"certification": "R"}]}]},
        "credits": {"cast": [], "crew": []},
        "belongs_to_collection": null
    }))
}

fn tmdb_answer(path: &str, query: &[(String, String)]) -> Option<Value> {
    let param = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let result = |id: i64, title: &str, date: &str| json!({"id": id, "title": title, "original_title": title, "release_date": date, "popularity": 10.0});
    Some(match path {
        "/search/movie" => json!({"results": match param("query") {
            Some("Alien") => vec![result(348, "Alien", "1979-05-25")],
            _ => vec![],
        }}),
        "/find/tt0133093" => json!({"movie_results": [result(603, "The Matrix", "1999-03-31")], "tv_results": []}),
        path => movie(path.strip_prefix("/movie/")?.parse().ok()?)?,
    })
}

/// Another show, which a search by the name "Harbour" finds, with no ids
/// at other sites.
fn tvmaze_other_show() -> Value {
    json!({
        "id": 77,
        "name": "Harbour",
        "premiered": "2001-01-01",
        "externals": {"thetvdb": null, "imdb": null},
        "_embedded": {"seasons": [], "episodes": [], "cast": [], "crew": [], "images": []}
    })
}

/// A show as TVmaze describes it, with no pictures to fetch.
fn tvmaze_show() -> Value {
    json!({
        "id": 4242,
        "name": "Harbour Lights",
        "genres": ["Drama"],
        "premiered": "2015-09-14",
        "summary": "<p>Storms bring old secrets ashore.</p>",
        "externals": {"thetvdb": 290001, "imdb": "tt4000001"},
        "_embedded": {
            "seasons": [{"number": 1, "name": "Gale Season", "premiereDate": "2015-09-14"}],
            "episodes": [{"id": 1, "name": "Landfall", "season": 1, "number": 1, "airdate": "2015-09-14"}],
            "cast": [],
            "crew": [],
            "images": []
        }
    })
}

async fn serve(State(mock): State<Mock>, uri: Uri, Query(query): Query<Vec<(String, String)>>) -> Response {
    let path = uri.path().to_owned();
    mock.requests.lock().push(format!("{path}?{}", uri.query().unwrap_or_default()));
    if let Some(path) = path.strip_prefix("/tmdb") {
        if !query.iter().any(|(k, v)| k == "api_key" && v == KEY) {
            return (StatusCode::UNAUTHORIZED, axum::Json(json!({"status_code": 7}))).into_response();
        }
        return match tmdb_answer(path, &query) {
            Some(body) => axum::Json(body).into_response(),
            None => (StatusCode::NOT_FOUND, axum::Json(json!({"status_code": 34}))).into_response(),
        };
    }
    match path.as_str() {
        // TVmaze answers a lookup with a redirect to the show.
        "/tvmaze/lookup/shows" if query.iter().any(|(k, v)| k == "thetvdb" && v == "290001") => {
            (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, "https://api.tvmaze.com/shows/4242")]).into_response()
        }
        "/tvmaze/search/shows" if query.iter().any(|(k, v)| k == "q" && v == "Harbour") => {
            axum::Json(json!([{"score": 1.0, "show": tvmaze_other_show()}])).into_response()
        }
        "/tvmaze/search/shows" => axum::Json(json!([])).into_response(),
        "/tvmaze/shows/77" => axum::Json(tvmaze_other_show()).into_response(),
        "/tvmaze/shows/4242" => axum::Json(tvmaze_show()).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// pley with TMDB and TVmaze at the stand-in, and an administrator.
async fn world() -> (TestApp, Mock, String) {
    let mock = Mock::default();
    let router = Router::new().fallback(serve).with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let base = format!("http://{addr}");
    let app = TestApp::with_config(|c| {
        c.metadata_endpoints = Some(pley::providers::Endpoints {
            tvmaze: format!("{base}/tvmaze"),
            tvdb: format!("{base}/tvdb"),
            musicbrainz: format!("{base}/mb"),
            cover_art: format!("{base}/caa"),
            tmdb: format!("{base}/tmdb"),
            tmdb_images: format!("{base}/tmdb-images"),
            opensubtitles: format!("{base}/opensubtitles"),
        });
    })
    .await;
    let admin = app.setup_admin().await;
    (app, mock, admin)
}

async fn use_key(app: &TestApp, admin: &str) {
    let reply = app.put("/api/settings", admin, json!({"tmdb_api_key": KEY})).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    app.wait_for_scans().await;
}

fn write(app: &TestApp, rel: &str, text: &str) {
    let path = app.media_path(rel);
    std::fs::create_dir_all(std::path::Path::new(&path).parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

async fn rescan(app: &TestApp, admin: &str, library: i64) {
    let reply = app.post(&format!("/api/libraries/{library}/scan"), admin, json!({})).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    app.wait_for_scans().await;
}

async fn item(app: &TestApp, admin: &str, id: i64) -> Value {
    app.get(&format!("/api/items/{id}"), admin).await.body
}

/// The only top-level item of a library.
async fn only(app: &TestApp, admin: &str, library: i64) -> i64 {
    let ids = app.ids(library, admin).await;
    assert_eq!(ids.len(), 1, "{ids:?}");
    *ids.values().next().unwrap()
}

async fn tvmaze_id(app: &TestApp, id: i64) -> Option<i64> {
    sqlx::query_scalar("SELECT tvmaze_id FROM items WHERE id = ?").bind(id).fetch_one(&app.state.db.read).await.unwrap()
}

/// Waits until the item (refreshed in the background) satisfies `done`.
async fn refreshed(app: &TestApp, admin: &str, id: i64, done: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..200 {
        let item = item(app, admin, id).await;
        if done(&item) {
            return item;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("item {id} was not refreshed: {}", item(app, admin, id).await);
}

#[tokio::test]
async fn a_movie_is_matched_by_its_nfo_files_tmdb_id_without_a_search() {
    let (app, mock, admin) = world().await;
    use_key(&app, &admin).await;
    app.media_file("movies/Some Rip/some.rip.mkv", 1024);
    write(&app, "movies/Some Rip/some.rip.nfo", r#"<movie><uniqueid type="tmdb">603</uniqueid></movie>"#);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;

    let movie = item(&app, &admin, only(&app, &admin, lib).await).await;
    assert_eq!((movie["title"].clone(), movie["tmdb_id"].clone()), (json!("The Matrix"), json!(603)));
    assert_eq!(movie["summary"], json!("The story of The Matrix."));
    assert!(mock.requested("/tmdb/search/movie").is_empty(), "{:?}", mock.requested("/tmdb/search"));
    // The ids come from it, no fields: the online source's stay.
    assert_eq!(movie["nfo_fields"], json!([]));
}

#[tokio::test]
async fn an_imdb_id_alone_is_found_through_tmdb() {
    let (app, mock, admin) = world().await;
    use_key(&app, &admin).await;
    app.media_file("movies/matrix/matrix.mkv", 1024);
    // Kodi's movie.nfo, in a folder holding one movie.
    write(&app, "movies/matrix/movie.nfo", "<movie><imdbid>tt0133093</imdbid></movie>");
    let lib = app.library(&admin, "Movies", "movies", "movies").await;

    let movie = item(&app, &admin, only(&app, &admin, lib).await).await;
    assert_eq!((movie["title"].clone(), movie["tmdb_id"].clone()), (json!("The Matrix"), json!(603)));
    assert_eq!(mock.requested("/tmdb/find/tt0133093").len(), 1);
    assert!(mock.requested("/tmdb/search/movie").is_empty());
}

#[tokio::test]
async fn a_show_is_matched_by_its_nfo_files_thetvdb_id_on_tvmaze() {
    let (app, mock, admin) = world().await;
    // No key anywhere: shows come from TVmaze.
    app.media_file("tv/HL/Season 01/HL - S01E01.mkv", 1024);
    write(&app, "tv/HL/tvshow.nfo", r#"<tvshow><uniqueid type="tvdb" default="true">290001</uniqueid></tvshow>"#);
    write(&app, "tv/HL/Season 01/season.nfo", "<season><title>The Storm</title><plot>Wind.</plot></season>");
    let lib = app.library(&admin, "TV", "shows", "tv").await;

    let show = only(&app, &admin, lib).await;
    let detail = item(&app, &admin, show).await;
    assert_eq!(detail["title"], json!("Harbour Lights"));
    assert_eq!(detail["summary"], json!("Storms bring old secrets ashore."));
    assert!(mock.requested("/tvmaze/search/shows").is_empty(), "found by its id, not its folder's name");
    assert_eq!(mock.requested("/tvmaze/lookup/shows?thetvdb=290001").len(), 1);
    let tvmaze: Option<i64> = sqlx::query_scalar("SELECT tvmaze_id FROM items WHERE id = ?")
        .bind(show)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(tvmaze, Some(4242));

    // A season's NFO file names it, over TVmaze's name ("Gale Season").
    let seasons = app.children(show, &admin).await;
    let season = item(&app, &admin, seasons["The Storm"]).await;
    assert_eq!(season["summary"], json!("Wind."));
    assert_eq!(season["nfo_fields"], json!(["summary", "title"]));
    let episodes = app.children(seasons["The Storm"], &admin).await;
    assert!(episodes.contains_key("Landfall"), "{episodes:?}");
}

#[tokio::test]
async fn an_nfo_files_id_overrules_a_match_found_by_name() {
    let (app, mock, admin) = world().await;
    app.media_file("tv/Harbour/Season 01/Harbour - S01E01.mkv", 1024);
    let lib = app.library(&admin, "TV", "shows", "tv").await;
    let show = only(&app, &admin, lib).await;
    assert_eq!(tvmaze_id(&app, show).await, Some(77), "the search found another show");

    // The NFO file gives a TheTVDB id, which TVmaze's other show lacks: its
    // id still decides.
    write(&app, "tv/Harbour/tvshow.nfo", "<tvshow><tvdbid>290001</tvdbid></tvshow>");
    rescan(&app, &admin, lib).await;
    assert_eq!(item(&app, &admin, show).await["title"], json!("Harbour Lights"));
    assert_eq!(tvmaze_id(&app, show).await, Some(4242));
    assert_eq!(mock.requested("/tvmaze/lookup/shows?thetvdb=290001").len(), 1);
}

#[tokio::test]
async fn without_a_tmdb_key_an_nfo_file_describes_a_movie_and_its_names_come_back_without_it() {
    let (app, _, admin) = world().await;
    app.media_file("movies/matrix (1999)/matrix (1999).mkv", 1024);
    let nfo = "movies/matrix (1999)/matrix (1999).nfo";
    write(
        &app,
        nfo,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
         <movie><title>The Matrix</title><plot>Neo wakes up.</plot><genre>Action</genre>
         <genre>Science Fiction</genre><mpaa>Rated R</mpaa><year>1999</year></movie>",
    );
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;

    let movie = item(&app, &admin, id).await;
    assert_eq!(movie["title"], json!("The Matrix"));
    assert_eq!(movie["sort_title"], json!("matrix"));
    assert_eq!(movie["summary"], json!("Neo wakes up."));
    assert_eq!(movie["genres"], json!(["Action", "Science Fiction"]));
    assert_eq!(movie["content_rating"], json!("R"));
    assert_eq!(movie["nfo_fields"], json!(["content_rating", "genres", "summary", "title", "year"]));
    // Its age limit holds.
    let (kid, _) = app.user(&admin, "kid", json!({"max_age": 12})).await;
    assert_eq!(app.get(&format!("/api/items/{id}"), &kid).await.status, StatusCode::NOT_FOUND);

    // Gone: the names' title is back, and what only the NFO file said goes.
    std::fs::remove_file(app.media_path(nfo)).unwrap();
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["year"].clone()), (json!("matrix"), json!(1999)));
    assert_eq!((movie["summary"].clone(), movie["genres"].clone()), (Value::Null, json!([])));
    assert_eq!((movie["content_rating"].clone(), movie["nfo_fields"].clone()), (Value::Null, json!([])));
}

#[tokio::test]
async fn an_nfo_file_edited_beside_an_unchanged_video_is_read_again_and_the_item_keeps_its_id() {
    let (app, _, admin) = world().await;
    app.media_file("movies/Heist (2001)/Heist (2001).mkv", 1024);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;
    assert_eq!(item(&app, &admin, id).await["title"], json!("Heist"));

    // It appears: the item is the same one, with the NFO file's title.
    let nfo = "movies/Heist (2001)/Heist (2001).nfo";
    write(&app, nfo, "<movie><title>The Heist</title></movie>");
    rescan(&app, &admin, lib).await;
    assert_eq!(only(&app, &admin, lib).await, id);
    assert_eq!(item(&app, &admin, id).await["title"], json!("The Heist"));

    write(&app, nfo, "<movie><title>The Big Heist</title><tagline>One last job.</tagline></movie>");
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["tagline"].clone()), (json!("The Big Heist"), json!("One last job.")));
    assert_eq!(only(&app, &admin, lib).await, id);
}

#[tokio::test]
async fn hand_edits_beat_the_nfo_file() {
    let (app, _, admin) = world().await;
    app.media_file("movies/Heist (2001)/Heist (2001).mkv", 1024);
    write(&app, "movies/Heist (2001)/Heist (2001).nfo", "<movie><title>The Heist</title><plot>A job.</plot></movie>");
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;

    let edited = app.patch(&format!("/api/items/{id}"), &admin, json!({"title": "My Heist"})).await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.body);
    assert_eq!(edited.body["title"], json!("My Heist"));
    assert_eq!(edited.body["sort_title"], json!("my heist"));
    assert_eq!(
        (edited.body["locked_fields"].clone(), edited.body["nfo_fields"].clone()),
        (json!(["title"]), json!(["summary", "title"]))
    );
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["summary"].clone()), (json!("My Heist"), json!("A job.")));

    // Unlocked, the NFO file's title is back.
    let unlocked = app.patch(&format!("/api/items/{id}"), &admin, json!({"title": null})).await;
    assert_eq!(unlocked.body["title"], json!("The Heist"));
}

#[tokio::test]
async fn fix_match_beats_an_nfo_files_id_until_it_is_undone() {
    let (app, _, admin) = world().await;
    use_key(&app, &admin).await;
    app.media_file("movies/Alien (1979)/Alien (1979).mkv", 1024);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;
    assert_eq!(item(&app, &admin, id).await["tmdb_id"], json!(348));

    let rematched = app.post(&format!("/api/items/{id}/match"), &admin, json!({"tmdb_id": 679})).await;
    assert_eq!(rematched.status, StatusCode::OK, "{:?}", rematched.body);
    assert_eq!(rematched.body["locked_fields"], json!(["match"]));
    // The match can't be set as a field, only undone.
    let set = app.patch(&format!("/api/items/{id}"), &admin, json!({"match": "8077"})).await;
    assert_eq!(set.status, StatusCode::BAD_REQUEST);

    // An NFO file naming another film after it doesn't undo it.
    write(&app, "movies/Alien (1979)/Alien (1979).nfo", r#"<movie><uniqueid type="tmdb">8077</uniqueid></movie>"#);
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["tmdb_id"].clone(), movie["title"].clone()), (json!(679), json!("Aliens")));

    // Undone, the NFO file's id decides.
    let undone = app.patch(&format!("/api/items/{id}"), &admin, json!({"match": null})).await;
    assert_eq!(undone.status, StatusCode::OK, "{:?}", undone.body);
    let movie = refreshed(&app, &admin, id, |i| i["title"] == "Alien³").await;
    assert_eq!((movie["tmdb_id"].clone(), movie["locked_fields"].clone()), (json!(8077), json!([])));
}

#[tokio::test]
async fn fix_match_to_another_title_sets_the_nfo_files_fields_aside_until_it_is_undone() {
    let (app, mock, admin) = world().await;
    use_key(&app, &admin).await;
    app.media_file("movies/Alien (1979)/Alien (1979).mkv", 1024);
    write(
        &app,
        "movies/Alien (1979)/Alien (1979).nfo",
        "<movie><title>Alien: Director's Cut</title><plot>Longer.</plot><mpaa>PG-13</mpaa>
         <uniqueid type=\"tmdb\">8077</uniqueid><lockdata>true</lockdata></movie>",
    );
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;
    assert_eq!(item(&app, &admin, id).await["title"], json!("Alien: Director's Cut"));
    assert!(mock.requested("/tmdb/movie/").is_empty());

    // The file names another film: the chosen one's metadata shows, its
    // content rating included, and the file's lockdata doesn't hold it.
    let rematched = app.post(&format!("/api/items/{id}/match"), &admin, json!({"tmdb_id": 679})).await;
    assert_eq!(rematched.status, StatusCode::OK, "{:?}", rematched.body);
    let movie = rematched.body;
    assert_eq!((movie["title"].clone(), movie["summary"].clone()), (json!("Aliens"), json!("The story of Aliens.")));
    assert_eq!((movie["content_rating"].clone(), movie["nfo_fields"].clone()), (json!("R"), json!([])));
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["content_rating"].clone()), (json!("Aliens"), json!("R")));

    // Undone, the file describes it again, and locks it.
    let undone = app.patch(&format!("/api/items/{id}"), &admin, json!({"match": null})).await;
    assert_eq!(undone.status, StatusCode::OK, "{:?}", undone.body);
    let movie = refreshed(&app, &admin, id, |i| i["title"] == "Alien: Director's Cut").await;
    assert_eq!((movie["tmdb_id"].clone(), movie["summary"].clone()), (json!(8077), json!("Longer.")));
    assert_eq!(movie["content_rating"], json!("PG-13"));
    let locked: bool = sqlx::query_scalar("SELECT metadata_locked FROM items WHERE id = ?")
        .bind(id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert!(locked);
}

#[tokio::test]
async fn an_nfo_file_naming_the_title_fix_match_chose_keeps_its_fields() {
    let (app, _, admin) = world().await;
    use_key(&app, &admin).await;
    app.media_file("movies/Alien (1979)/Alien (1979).mkv", 1024);
    let nfo = "movies/Alien (1979)/Alien (1979).nfo";
    write(&app, nfo, r#"<movie><plot>Mine.</plot><uniqueid type="tmdb">348</uniqueid></movie>"#);
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;

    // Fix Match to the film the file names: the file still describes it.
    let rematched = app.post(&format!("/api/items/{id}/match"), &admin, json!({"tmdb_id": 348})).await;
    assert_eq!(rematched.status, StatusCode::OK, "{:?}", rematched.body);
    assert_eq!(
        (rematched.body["summary"].clone(), rematched.body["nfo_fields"].clone()),
        (json!("Mine."), json!(["summary"]))
    );

    // To another, it doesn't; corrected to name that one, it does again.
    let rematched = app.post(&format!("/api/items/{id}/match"), &admin, json!({"tmdb_id": 679})).await;
    assert_eq!(rematched.body["summary"], json!("The story of Aliens."));
    write(&app, nfo, r#"<movie><plot>Mine too.</plot><uniqueid type="tmdb">679</uniqueid></movie>"#);
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["tmdb_id"].clone(), movie["title"].clone()), (json!(679), json!("Aliens")));
    assert_eq!((movie["summary"].clone(), movie["nfo_fields"].clone()), (json!("Mine too."), json!(["summary"])));
}

#[tokio::test]
async fn a_removed_nfo_file_gives_the_online_values_back() {
    let (app, _, admin) = world().await;
    use_key(&app, &admin).await;
    app.media_file("movies/Alien (1979)/Alien (1979).mkv", 1024);
    let nfo = "movies/Alien (1979)/Alien (1979).nfo";
    write(
        &app,
        nfo,
        "<movie><title>Alien: Director's Cut</title><plot>Longer.</plot><rating>9.5</rating>
         <uniqueid type=\"tmdb\">8077</uniqueid></movie>",
    );
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["summary"].clone()), (json!("Alien: Director's Cut"), json!("Longer.")));
    assert_eq!((movie["rating"].clone(), movie["tmdb_id"].clone()), (json!(9.5), json!(8077)));
    // What it doesn't say is TMDB's, for the film it names.
    assert_eq!(
        (movie["genres"].clone(), movie["release_date"].clone()),
        (json!(["Science Fiction"]), json!("1992-05-22"))
    );

    // Gone, its match goes too: the names' search finds the film again.
    std::fs::remove_file(app.media_path(nfo)).unwrap();
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["summary"].clone()), (json!("Alien"), json!("The story of Alien.")));
    assert_eq!((movie["tmdb_id"].clone(), movie["imdb_id"].clone()), (json!(348), json!("tt0078748")));
    assert_eq!((movie["rating"].clone(), movie["nfo_fields"].clone()), (json!(7.5), json!([])));
}

#[tokio::test]
async fn lockdata_keeps_an_item_from_being_fetched_online() {
    let (app, mock, admin) = world().await;
    use_key(&app, &admin).await;
    app.media_file("movies/Alien (1979)/Alien (1979).mkv", 1024);
    let nfo = "movies/Alien (1979)/Alien (1979).nfo";
    write(&app, nfo, "<movie><title>My Alien</title><tmdbid>348</tmdbid><lockdata>true</lockdata></movie>");
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;

    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["summary"].clone()), (json!("My Alien"), Value::Null));
    assert!(mock.requested("/tmdb/search/movie").is_empty() && mock.requested("/tmdb/movie/").is_empty());
    // Not by hand either.
    app.post(&format!("/api/items/{id}/refresh"), &admin, json!({})).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(mock.requested("/tmdb/movie/").is_empty());

    // Unlocked, it is.
    write(&app, nfo, "<movie><title>My Alien</title><tmdbid>348</tmdbid><lockdata>false</lockdata></movie>");
    rescan(&app, &admin, lib).await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["summary"].clone()), (json!("My Alien"), json!("The story of Alien.")));
}

#[tokio::test]
async fn nfo_files_turned_off_are_ignored_and_what_they_set_goes() {
    let (app, _, admin) = world().await;
    app.media_file("movies/Heist (2001)/Heist (2001).mkv", 1024);
    write(&app, "movies/Heist (2001)/Heist (2001).nfo", "<movie><title>The Heist</title><plot>A job.</plot></movie>");
    let lib = app.library(&admin, "Movies", "movies", "movies").await;
    let id = only(&app, &admin, lib).await;
    assert_eq!(item(&app, &admin, id).await["title"], json!("The Heist"));

    let off = app.put("/api/settings", &admin, json!({"read_nfo": false})).await;
    assert_eq!((off.status, off.body["read_nfo"].clone()), (StatusCode::OK, json!(false)));
    app.wait_for_scans().await;
    let movie = item(&app, &admin, id).await;
    assert_eq!((movie["title"].clone(), movie["summary"].clone()), (json!("Heist"), Value::Null));
    assert_eq!(movie["nfo_fields"], json!([]));

    app.put("/api/settings", &admin, json!({"read_nfo": true})).await;
    app.wait_for_scans().await;
    assert_eq!(item(&app, &admin, id).await["title"], json!("The Heist"));
}
