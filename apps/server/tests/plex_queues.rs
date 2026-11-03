//! Play queues (`/playQueues`) as python-plexapi drives them: made from items,
//! containers, listings and playlists; continuous play; next, move, remove,
//! shuffle; and only for the user who made them.

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use common::{TestApp, encode, plex_titles};

struct World {
    app: TestApp,
    admin: String,
    season: i64,
    episodes: Vec<i64>,
    movies: Vec<i64>,
}

async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (e, title) in [(1, "Pilot"), (2, "Second"), (3, "Third")] {
        app.media_file(&format!("tv/Taxi (1978)/Season 01/Taxi - S01E0{e} - {title}.mkv"), 1024);
    }
    for name in ["Alien (1979)", "Brazil (1985)"] {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    let tv = app.library(&admin, "TV", "shows", "tv").await;
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let show = app.ids(tv, &admin).await["Taxi"];
    let season = app.children(show, &admin).await["Season 1"];
    let names = app.children(season, &admin).await;
    let episodes = ["Pilot", "Second", "Third"].iter().map(|t| names[*t]).collect();
    let ids = app.ids(movies, &admin).await;
    World { app, admin, season, episodes, movies: vec![ids["Alien"], ids["Brazil"]] }
}

async fn queue(w: &World, method: Method, uri: &str) -> Value {
    let reply = w.app.plex(method, uri, &w.admin, &[]).await;
    assert_eq!(reply.status, StatusCode::OK, "{uri}: {}", reply.text());
    reply.body["MediaContainer"].clone()
}

fn entries(mc: &Value) -> Vec<i64> {
    mc["Metadata"].as_array().unwrap().iter().map(|m| m["playQueueItemID"].as_i64().unwrap()).collect()
}

fn item(id: i64) -> String {
    encode(&format!("server://m/com.plexapp.plugins.library/library/metadata/{id}"))
}

#[tokio::test]
async fn queues_play_on_and_follow_the_client() {
    let w = world().await;
    let second = w.episodes[1];

    // Continuous play: an episode, then the rest of its show.
    let mc = queue(&w, Method::POST, &format!("/playQueues?type=video&continuous=1&uri={}", item(second))).await;
    assert_eq!(plex_titles(&mc), ["Second", "Third"]);
    assert_eq!(mc["playQueueSelectedMetadataItemID"], json!(second.to_string()));

    // A season, starting at its second episode.
    let start = format!("/playQueues?type=video&uri={}&key=%2Flibrary%2Fmetadata%2F{second}", item(w.season));
    let mc = queue(&w, Method::POST, &start).await;
    let id = mc["playQueueID"].as_i64().unwrap();
    assert_eq!(plex_titles(&mc), ["Pilot", "Second", "Third"]);
    assert_eq!((&mc["playQueueSelectedItemOffset"], &mc["playQueueTotalCount"]), (&json!(1), &json!(3)));
    let [e1, e2, e3] = entries(&mc)[..] else { panic!() };

    // A movie to play next, as python-plexapi's addItem(playNext=True) sends it.
    let next = format!(
        "/playQueues/{id}?uri={}&next=1",
        encode(&format!("library://u/item/library/metadata/{}", w.movies[0]))
    );
    let mc = queue(&w, Method::PUT, &next).await;
    assert_eq!(plex_titles(&mc), ["Pilot", "Second", "Alien", "Third"]);
    let alien = entries(&mc)[2];

    let mc = queue(&w, Method::PUT, &format!("/playQueues/{id}/items/{e3}/move?after={e1}")).await;
    assert_eq!(entries(&mc), [e1, e3, e2, alien]);
    let mc = queue(&w, Method::DELETE, &format!("/playQueues/{id}/items/{alien}")).await;
    assert_eq!(entries(&mc), [e1, e3, e2]);

    let mc = queue(&w, Method::PUT, &format!("/playQueues/{id}/shuffle")).await;
    assert_eq!(mc["playQueueShuffled"], json!(true));
    assert_eq!(entries(&mc)[0], e2, "the playing episode first");
    let mc = queue(&w, Method::PUT, &format!("/playQueues/{id}/unshuffle")).await;
    assert_eq!(entries(&mc), [e1, e3, e2]);
    let mc = queue(&w, Method::DELETE, &format!("/playQueues/{id}/items")).await;
    assert_eq!(entries(&mc), [e2], "only what plays stays");

    // Someone else's queue isn't theirs to see.
    let (bob, _) = w.app.user(&w.admin, "bob", json!({})).await;
    assert_eq!(w.app.plex(Method::GET, &format!("/playQueues/{id}"), &bob, &[]).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn queues_from_lists_listings_and_playlists() {
    let w = world().await;
    // python-plexapi's PlayQueue.create(items=[…]) encodes the path once more.
    let list = encode(&format!(
        "library:///directory/{}",
        encode(&format!("/library/metadata/{},{}", w.movies[1], w.movies[0]))
    ));
    let mc = queue(&w, Method::POST, &format!("/playQueues?type=video&uri={list}")).await;
    assert_eq!(plex_titles(&mc), ["Brazil", "Alien"]);

    let movies = w
        .app
        .get("/api/libraries", &w.admin)
        .await
        .body
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "Movies")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let listing =
        encode(&format!("server://m/com.plexapp.plugins.library/library/sections/{movies}/all?type=1&sort=titleSort"));
    let mc = queue(&w, Method::POST, &format!("/playQueues?type=video&uri={listing}")).await;
    assert_eq!(plex_titles(&mc), ["Alien", "Brazil"]);

    let created =
        w.app.post("/api/playlists", &w.admin, json!({"title": "Two", "item_ids": [w.movies[1], w.episodes[0]]})).await;
    let playlist = created.body["id"].as_i64().unwrap();
    let mc = queue(&w, Method::POST, &format!("/playQueues?type=video&playlistID={playlist}&shuffle=0")).await;
    assert_eq!(plex_titles(&mc), ["Brazil", "Pilot"]);
}
