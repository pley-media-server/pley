//! Scans of what changed (docs/FEATURES.md, Libraries): a change rescans the
//! folder of the title it is in, or the files of the library folder, and
//! nothing else; what lies outside stays as it was until the next scan of
//! the whole library.

mod common;

use std::path::{Path, PathBuf};

use axum::http::{Method, StatusCode};
use serde_json::json;

use common::{TestApp, titles};

/// A Pley that scans only when told: the tests say what changed.
async fn app() -> (TestApp, String) {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let reply = app.put("/api/settings", &admin, json!({"watch_libraries": false})).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    (app, admin)
}

/// When a library was last scanned, and last scanned whole.
async fn scan_times(app: &TestApp, lib: i64) -> (Option<i64>, Option<i64>) {
    sqlx::query_as("SELECT scanned_at, walked_at FROM libraries WHERE id = ?")
        .bind(lib)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

/// A library of the media folder `rel` whose files were probed, as if a
/// transcoder had read them: rescans find them unchanged.
async fn library(app: &TestApp, admin: &str, kind: &str, rel: &str) -> i64 {
    let lib = app.library(admin, rel, kind, rel).await;
    sqlx::query("UPDATE media_files SET probed = 1").execute(&app.state.db.write).await.unwrap();
    lib
}

/// Scans where `changed` (paths in the media folder) changed.
async fn changed(app: &TestApp, lib: i64, changed: &[&str]) {
    let paths: Vec<PathBuf> = changed.iter().map(|rel| app.media.path().join(rel)).collect();
    app.state.scanner.enqueue_changes(lib, paths);
    app.wait_for_scans().await;
}

async fn listed(app: &TestApp, admin: &str, lib: i64) -> Vec<String> {
    titles(&app.get(&format!("/api/libraries/{lib}/items"), admin).await.body["items"])
}

async fn rescan(app: &TestApp, admin: &str, lib: i64) {
    let reply = app.post(&format!("/api/libraries/{lib}/scan"), admin, json!({})).await;
    assert_eq!(reply.status, StatusCode::ACCEPTED);
    app.wait_for_scans().await;
}

/// Moves a folder, once nothing reads its files: on Windows a folder whose
/// file is open (fingerprinted after a scan) can't be moved.
async fn rename(from: &Path, to: &Path) {
    for _ in 0..100 {
        if std::fs::rename(from, to).is_ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    std::fs::rename(from, to).unwrap();
}

/// Languages of the sidecar subtitles of an item's first file.
async fn subtitles(app: &TestApp, admin: &str, item: i64) -> Vec<String> {
    let item = app.get(&format!("/api/items/{item}"), admin).await;
    item.body["files"][0]["streams"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["external"] == json!(true))
        .map(|s| s["language"].as_str().unwrap_or("?").to_owned())
        .collect()
}

#[tokio::test]
async fn a_change_rescans_only_the_folder_it_is_in() {
    let (app, admin) = app().await;
    app.media_file("movies/Film One (2001)/Film One (2001).mkv", 1024);
    app.media_file("movies/Film Two (2002)/Film Two (2002).mkv", 1024);
    app.media_file("movies/Film Four (2004)/Film Four (2004).mkv", 1024);
    app.media_file("movies/Loose (2003).mkv", 1024);
    let lib = library(&app, &admin, "movies", "movies").await;
    assert!(scan_times(&app, lib).await.1.is_some());
    sqlx::query("UPDATE libraries SET scanned_at = 1, walked_at = 1 WHERE id = ?")
        .bind(lib)
        .execute(&app.state.db.write)
        .await
        .unwrap();

    // Subtitles and a poster come for one film; unnoticed, another film's
    // video is deleted and a third film comes.
    let media = app.media.path();
    app.media_file("movies/Film One (2001)/Film One (2001).en.srt", 10);
    app.media_file("movies/Film One (2001)/poster.jpg", 10);
    std::fs::remove_file(media.join("movies/Film Two (2002)/Film Two (2002).mkv")).unwrap();
    app.media_file("movies/Film Two (2002)/poster.jpg", 10);
    app.media_file("movies/Film Three (2003)/Film Three (2003).mkv", 1024);
    changed(&app, lib, &["movies/Film One (2001)/Film One (2001).en.srt"]).await;
    let film = app.ids(lib, &admin).await["Film One"];
    assert_eq!(subtitles(&app, &admin, film).await, ["eng"]);
    assert!(app.get(&format!("/api/items/{film}"), &admin).await.body["poster_url"].is_string());
    // What is in the other folders is as it was.
    assert_eq!(listed(&app, &admin, lib).await, ["Film Four", "Film One", "Film Two", "Loose"]);
    let (scanned, walked) = scan_times(&app, lib).await;
    assert!(scanned > Some(1));
    assert_eq!(walked, Some(1), "the periodic scan is still due");

    // A file of the library folder: those files are looked at, with what
    // went from among them.
    std::fs::remove_file(media.join("movies/Loose (2003).mkv")).unwrap();
    app.media_file("movies/Another (2004).mkv", 1024);
    changed(&app, lib, &["movies/Another (2004).mkv"]).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Another", "Film Four", "Film One", "Film Two"]);

    // A video that went: its film goes.
    changed(&app, lib, &["movies/Film Two (2002)/Film Two (2002).mkv"]).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Another", "Film Four", "Film One"]);
    assert_eq!(scan_times(&app, lib).await.1, Some(1));

    // A folder gone from the library folder, which is listed anyway, that
    // no change named: changes went unnoticed, so the whole library is
    // scanned, as at the next full scan.
    std::fs::remove_dir_all(media.join("movies/Film Four (2004)")).unwrap();
    changed(&app, lib, &["movies/Film One (2001)/Film One (2001).en.srt"]).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Another", "Film One", "Film Three"]);
    assert!(scan_times(&app, lib).await.1 > Some(1));
    app.media_file("movies/Film Five (2005)/Film Five (2005).mkv", 1024);
    rescan(&app, &admin, lib).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Another", "Film Five", "Film One", "Film Three"]);
    assert_eq!(subtitles(&app, &admin, film).await, ["eng"]);
}

/// A change only where scans never look (a NAS's thumbnails) starts none.
#[tokio::test]
async fn a_change_where_scans_never_look_starts_none() {
    let (app, admin) = app().await;
    app.media_file("movies/Film One (2001)/Film One (2001).mkv", 1024);
    let lib = library(&app, &admin, "movies", "movies").await;
    let (scanned, _) = scan_times(&app, lib).await;
    sqlx::query("UPDATE libraries SET scanned_at = 1 WHERE id = ?")
        .bind(lib)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    app.media_file("movies/Film One (2001)/@eaDir/Film One (2001).mkv/SYNOFILE_THUMB_M.jpg", 10);
    changed(&app, lib, &["movies/Film One (2001)/@eaDir/Film One (2001).mkv/SYNOFILE_THUMB_M.jpg"]).await;
    assert_eq!(scan_times(&app, lib).await.0, Some(1));
    changed(&app, lib, &["movies/Film One (2001)/Film One (2001).mkv"]).await;
    assert!(scan_times(&app, lib).await.0 >= scanned);
}

#[tokio::test]
async fn a_folder_moved_in_the_library_keeps_its_title_and_what_was_done_with_it() {
    let (app, admin) = app().await;
    app.media_file("movies/Film One (2001)/Film One (2001).mkv", 1024);
    app.media_file("movies/Film One (2001)/Film One (2001).en.srt", 10);
    app.media_file("movies/Film Two (2002)/Film Two (2002).mkv", 1024);
    let lib = library(&app, &admin, "movies", "movies").await;
    let film = app.ids(lib, &admin).await["Film One"];
    let progress = json!({"position_ms": 50_000, "duration_ms": 100_000, "state": "paused"});
    assert!(app.post(&format!("/api/items/{film}/progress"), &admin, progress).await.status.is_success());

    let media = app.media.path();
    std::fs::create_dir_all(media.join("movies/Older")).unwrap();
    rename(&media.join("movies/Film One (2001)"), &media.join("movies/Older/Film One (2001)")).await;
    changed(&app, lib, &["movies/Film One (2001)", "movies/Older/Film One (2001)"]).await;
    assert_eq!(app.ids(lib, &admin).await["Film One"], film, "the same film");
    let item = app.get(&format!("/api/items/{film}"), &admin).await.body;
    let path = item["files"][0]["path"].as_str().unwrap().replace('\\', "/");
    assert!(path.ends_with("movies/Older/Film One (2001)/Film One (2001).mkv"), "{path}");
    assert_eq!(item["files"].as_array().unwrap().len(), 1);
    assert_eq!(item["user_state"]["position_ms"], json!(50_000));
    assert_eq!(subtitles(&app, &admin, film).await, ["eng"]);
    assert_eq!(listed(&app, &admin, lib).await, ["Film One", "Film Two"]);
}

/// Folders and a file renamed or moved while nothing watched (a share
/// another computer wrote to), then a change elsewhere in the library:
/// their titles aren't taken for deleted because where they were is gone,
/// they are found where they are now.
#[tokio::test]
async fn what_was_renamed_unnoticed_keeps_its_title_at_a_change_elsewhere() {
    let (app, admin) = app().await;
    app.media_file("movies/Film One (2001)/Film One (2001).mkv", 1024);
    app.media_file("movies/Film Two (2002)/Film Two (2002).mkv", 1024);
    app.media_file("movies/Film Three (2003)/Film Three (2003).mkv", 1024);
    app.media_file("movies/Older/Film Four (2004)/Film Four (2004).mkv", 1024);
    app.media_file("movies/Loose (2003).mkv", 1024);
    let lib = library(&app, &admin, "movies", "movies").await;
    let films = app.ids(lib, &admin).await;
    let progress = json!({"position_ms": 50_000, "duration_ms": 100_000, "state": "paused"});
    let moved = [
        ("Film One", "Film One (2001)", "Film One (2001) [Remux]", "Film One (2001) [Remux]/Film One (2001).mkv"),
        ("Film Three", "Film Three (2003)", "Older/Film Three (2003)", "Older/Film Three (2003)/Film Three (2003).mkv"),
        ("Loose", "Loose (2003).mkv", "Loose (2003) [Remux].mkv", "Loose (2003) [Remux].mkv"),
    ];
    let media = app.media.path().join("movies");
    for (film, from, to, _) in moved {
        let film = films[film];
        assert!(app.post(&format!("/api/items/{film}/progress"), &admin, progress.clone()).await.status.is_success());
        rename(&media.join(from), &media.join(to)).await;
    }
    app.media_file("movies/Film Two (2002)/Film Two (2002).en.srt", 10);
    changed(&app, lib, &["movies/Film Two (2002)/Film Two (2002).en.srt"]).await;
    assert_eq!(app.ids(lib, &admin).await, films, "the same films");
    for (film, _, _, now_at) in moved {
        let item = app.get(&format!("/api/items/{}", films[film]), &admin).await.body;
        assert_eq!(item["files"].as_array().unwrap().len(), 1, "{item}");
        let path = item["files"][0]["path"].as_str().unwrap().replace('\\', "/");
        assert!(path.ends_with(now_at), "{path}");
        assert_eq!(item["user_state"]["position_ms"], json!(50_000), "{film}");
    }
}

/// A library whose scans fail waits longer and longer between them: a
/// change only where scans never look, which scans nothing, doesn't end the
/// wait.
#[tokio::test]
async fn a_change_that_scans_nothing_leaves_a_failing_library_waiting() {
    let (app, admin) = app().await;
    app.media_file("movies/Film One (2001)/Film One (2001).mkv", 1024);
    let lib = library(&app, &admin, "movies", "movies").await;
    // A database that can't be written to where the scan ends.
    let db = &app.state.db.write;
    sqlx::query(
        "CREATE TRIGGER scans_fail BEFORE UPDATE OF scanned_at ON libraries
         BEGIN SELECT RAISE(ABORT, 'scans fail'); END",
    )
    .execute(db)
    .await
    .unwrap();
    rescan(&app, &admin, lib).await;
    assert!(app.state.scanner.backing_off(lib, 3600), "the scan failed");
    changed(&app, lib, &["movies/Film One (2001)/@eaDir/Film One (2001).mkv/SYNOFILE_THUMB_M.jpg"]).await;
    assert!(app.state.scanner.backing_off(lib, 3600), "nothing was scanned");
    // A scan that works ends it.
    sqlx::query("DROP TRIGGER scans_fail").execute(db).await.unwrap();
    changed(&app, lib, &["movies/Film One (2001)/Film One (2001).mkv"]).await;
    assert!(!app.state.scanner.backing_off(lib, 3600));
}

#[tokio::test]
async fn a_library_folder_left_empty_keeps_its_titles() {
    let (app, admin) = app().await;
    app.media_file("movies/Film One (2001)/Film One (2001).mkv", 1024);
    app.media_file("movies/Film Two (2002)/Film Two (2002).mkv", 1024);
    let lib = library(&app, &admin, "movies", "movies").await;
    // An unmounted drive: its mount point is there, with nothing in it.
    for film in ["Film One (2001)", "Film Two (2002)"] {
        std::fs::remove_dir_all(app.media.path().join("movies").join(film)).unwrap();
    }
    changed(&app, lib, &["movies/Film One (2001)"]).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Film One", "Film Two"]);
    // Gone for real when the drive holds something else: both folders went
    // from the library folder, the one not named too.
    app.media_file("movies/Film Three (2003)/Film Three (2003).mkv", 1024);
    changed(&app, lib, &["movies/Film One (2001)", "movies/Film Three (2003)"]).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Film Three"]);
}

#[tokio::test]
async fn an_episode_added_to_a_show_is_found_with_its_show() {
    let (app, admin) = app().await;
    app.media_file("tv/Show One (2015)/Season 01/Show One - S01E01.mkv", 1024);
    app.media_file("tv/Show Two (2016)/Season 01/Show Two - S01E01.mkv", 1024);
    let lib = library(&app, &admin, "shows", "tv").await;
    app.media_file("tv/Show One (2015)/Season 01/Show One - S01E02.mkv", 1024);
    app.media_file("tv/Show One (2015)/Season 02/Show One - S02E01.mkv", 1024);
    app.media_file("tv/Show Two (2016)/Season 01/Show Two - S01E02.mkv", 1024);
    changed(&app, lib, &["tv/Show One (2015)/Season 01/Show One - S01E02.mkv", "tv/Show One (2015)/Season 02"]).await;
    let shows = app.ids(lib, &admin).await;
    let count = |id: i64| {
        let app = &app;
        let admin = admin.clone();
        async move { app.get(&format!("/api/items/{id}"), &admin).await.body["leaf_count"].clone() }
    };
    assert_eq!(count(shows["Show One"]).await, json!(3));
    assert_eq!(count(shows["Show Two"]).await, json!(1), "not looked at");
}

/// Tools that put a title in its folder ask for that folder to be scanned
/// again; one the library doesn't hold has all of it scanned.
#[tokio::test]
async fn a_refresh_naming_a_folder_scans_that_folder() {
    let (app, admin) = app().await;
    app.media_file("movies/Film One (2001)/Film One (2001).mkv", 1024);
    let lib = library(&app, &admin, "movies", "movies").await;
    let token = app.plex_token(&admin).await;
    app.media_file("movies/Film Two (2002)/Film Two (2002).mkv", 1024);
    app.media_file("movies/Film Three (2003)/Film Three (2003).mkv", 1024);
    let refresh = |path: String| {
        let app = &app;
        let token = token.clone();
        async move {
            let uri = format!("/library/sections/{lib}/refresh?path={}", common::encode(&path));
            let reply = app.plex(Method::GET, &uri, &token, &[]).await;
            assert_eq!(reply.status, StatusCode::OK);
            app.wait_for_scans().await;
        }
    };
    refresh(app.media_path("movies/Film Two (2002)")).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Film One", "Film Two"]);
    refresh("/elsewhere/Film Three (2003)".to_owned()).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Film One", "Film Three", "Film Two"]);

    // A title's folder the tool renamed, named only as it is now: the film
    // keeps what was done with it, from its one file.
    let film = app.ids(lib, &admin).await["Film Two"];
    let progress = json!({"position_ms": 30_000, "duration_ms": 100_000, "state": "paused"});
    assert!(app.post(&format!("/api/items/{film}/progress"), &admin, progress).await.status.is_success());
    let media = app.media.path();
    rename(&media.join("movies/Film Two (2002)"), &media.join("movies/Film Two (2002) [Remux]")).await;
    refresh(app.media_path("movies/Film Two (2002) [Remux]")).await;
    assert_eq!(listed(&app, &admin, lib).await, ["Film One", "Film Three", "Film Two"]);
    let item = app.get(&format!("/api/items/{film}"), &admin).await.body;
    assert_eq!(item["files"].as_array().unwrap().len(), 1, "{item}");
    assert_eq!(item["user_state"]["position_ms"], json!(30_000));
}
