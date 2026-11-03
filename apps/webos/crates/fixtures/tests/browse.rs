//! Libraries, listings, items, Home, search, collections, playlists, My
//! List, history and pictures, against the fixture server.

mod common;

use common::Api;
use pley_fixtures::{FixtureServer, ids};
use serde_json::{Value, json};

fn titles(items: &Value) -> Vec<String> {
    items.as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap().to_string()).collect()
}

#[test]
fn libraries_list_sort_filter_and_page() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let libraries = api.get("/api/libraries").json();
    let names: Vec<&str> = libraries.as_array().unwrap().iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Movies", "TV Shows", "Music"]);
    assert_eq!(libraries[0]["kind"], "movies");
    assert_eq!(libraries[0]["item_count"], 1_500);
    assert_eq!(libraries[0]["paths"], json!([]));
    assert_eq!(libraries[0]["scanning"], false);
    assert_eq!(api.get("/api/libraries/2").json()["kind"], "shows");
    assert_eq!(api.get("/api/libraries/9").status, 404);

    let page = api.get("/api/libraries/1/items?limit=50");
    assert_eq!(page.status, 200);
    let etag = page.header("ETag").unwrap().to_string();
    assert!(etag.starts_with("W/\""));
    let page = page.json();
    assert_eq!(page["total"], 1_500);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 50);
    let sorts: Vec<String> = items.iter().map(|i| i["sort_title"].as_str().unwrap().to_lowercase()).collect();
    assert!(sorts.windows(2).all(|w| w[0] <= w[1]), "by title");
    assert!(items[0].get("backdrop_url").is_none() && items[0].get("thumb_url").is_none());
    assert!(items[0].get("summary").is_none(), "lists leave summaries out");
    assert!(items[0]["poster_url"].as_str().unwrap().starts_with("/api/items/"));
    assert!(items[0]["user_state"].is_object());

    let next = api.get("/api/libraries/1/items?limit=50&offset=50").json();
    assert_ne!(next["items"][0]["id"], items[0]["id"]);
    assert_eq!(api.get_with("/api/libraries/1/items?limit=50", &[("If-None-Match", &etag)]).status, 304);

    let added = api.get("/api/libraries/1/items?sort=added&limit=20").json();
    let dates: Vec<i64> = added["items"].as_array().unwrap().iter().map(|i| i["added_at"].as_i64().unwrap()).collect();
    assert!(dates.windows(2).all(|w| w[0] >= w[1]), "newest first");
    let ascending = api.get("/api/libraries/1/items?sort=added&order=asc&limit=5").json();
    assert!(ascending["items"][0]["added_at"].as_i64().unwrap() <= ascending["items"][1]["added_at"].as_i64().unwrap());
    assert_eq!(api.get("/api/libraries/1/items?sort=nothing").status, 400);

    let comedies = api.get("/api/libraries/1/items?genre=Comedy").json();
    assert!(comedies["total"].as_i64().unwrap() > 0);
    let watched = api.get("/api/libraries/1/items?watched=1").json();
    assert_eq!(watched["total"], 21);
    assert!(watched["items"].as_array().unwrap().iter().all(|i| i["user_state"]["watched"] == true));
    let by_bitrate = api.get("/api/libraries/1/items?sort=bitrate&limit=3").json();
    assert!(by_bitrate["items"][0]["media"]["bitrate"].as_i64().unwrap() > 20_000_000);
    // Every listing carries the media summary, as the server's does.
    assert!(items.iter().all(|i| i["media"].is_object()), "media whatever the sort");

    let filters = api.get("/api/libraries/1/filters").json();
    let years: Vec<i64> = filters["years"].as_array().unwrap().iter().map(|y| y.as_i64().unwrap()).collect();
    assert!(years.windows(2).all(|w| w[0] > w[1]));
    assert!(filters["genres"].as_array().unwrap().iter().any(|g| g == "Comedy"));
    // The resolutions the films have, best first, and the dynamic ranges
    // broadest first: HDR (which takes in Dolby Vision and HDR10+) leads,
    // and Any HDR, which would keep the same films without HLG, is left out.
    assert_eq!(filters["resolutions"], json!(["4k", "1080"]));
    assert_eq!(filters["dynamic_ranges"], json!(["hdr10", "dolby_vision", "hdr10plus", "sdr"]));
    assert_eq!(api.get("/api/libraries/3/filters").json()["resolutions"], json!([]), "music has none");
    // The listing filters by them: a title by its best file.
    let uhd = api.get("/api/libraries/1/items?resolution=4k").json();
    let all = |page: &serde_json::Value, f: &dyn Fn(&serde_json::Value) -> bool| {
        page["items"].as_array().unwrap().iter().all(|i| f(&i["media"]))
    };
    assert!(uhd["total"].as_i64().unwrap() > 0 && uhd["total"].as_i64().unwrap() < 1500);
    assert!(all(&uhd, &|m| m["height"] == 2160));
    let hdr = api.get("/api/libraries/1/items?dynamic_range=hdr").json();
    assert_eq!(hdr["total"], uhd["total"], "the 4K films are the HDR ones");
    assert!(all(&hdr, &|m| m["dynamic_range"] != "sdr"));
    let dv = api.get("/api/libraries/1/items?dynamic_range=dolby_vision&resolution=4k").json();
    assert!(all(&dv, &|m| m["dynamic_range"] == "dolby_vision"));
    // HDR takes in Dolby Vision and HDR10+: without HLG, the same films as Any HDR.
    let total = |query: &str| api.get(&format!("/api/libraries/1/items?{query}")).json()["total"].as_i64().unwrap();
    let (hdr10, dolby_vision, hdr10plus) =
        (total("dynamic_range=hdr10"), total("dynamic_range=dolby_vision"), total("dynamic_range=hdr10plus"));
    assert_eq!(hdr10, hdr["total"].as_i64().unwrap());
    assert!(
        dolby_vision > 0 && hdr10plus > 0 && hdr10 > dolby_vision + hdr10plus,
        "{hdr10} {dolby_vision} {hdr10plus}"
    );
    assert_eq!(api.get("/api/libraries/1/items?resolution=8k").status, 400);
    assert_eq!(api.get("/api/libraries/1/items?dynamic_range=hdr11").status, 400);
    assert_eq!(api.get("/api/libraries/1/items?resolution=").json()["total"], 1500, "empty is no filter");

    let shows = api.get("/api/libraries/2/items?limit=5").json();
    assert_eq!(shows["total"], 300);
    assert!(shows["items"][0]["leaf_count"].as_i64().unwrap() > 0);
    let hubs = api.get("/api/libraries/3/hubs").json();
    assert_eq!(hubs["hubs"][0]["title"], "Recently Added Albums");
    assert_eq!(hubs["hubs"][0]["items"][0]["kind"], "album");
}

#[test]
fn items_with_their_files_seasons_and_people() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let sample = api.get(&format!("/api/items/{}", ids::SAMPLE_HDR10)).json();
    assert_eq!(sample["title"], "Sample 2160p HDR10");
    assert_eq!(sample["kind"], "movie");
    assert!(sample["summary"].is_string(), "details keep the summary");
    let file = &sample["files"][0];
    assert_eq!(file["id"], ids::SAMPLE_HDR10_FILE);
    assert_eq!(file["container"], "mkv");
    assert_eq!(file["chapters"].as_array().unwrap().len(), 3);
    assert!(file["chapters"][0]["thumb_url"].as_str().unwrap().contains("/chapters/0.jpg?v="));
    assert_eq!(file["edition"], Value::Null);
    let video = &file["streams"][0];
    assert_eq!((video["codec"].as_str(), video["dynamic_range"].as_str()), (Some("hevc"), Some("hdr10")));
    assert_eq!(video["forced"], false);
    assert!(video.get("external").is_none(), "left out when false");
    let external = file["streams"].as_array().unwrap().iter().find(|s| s["index"] == 1_000).unwrap();
    assert_eq!(external["external"], true);
    assert_eq!(file["selected_audio"], 1);
    assert_eq!(file["audio_picked"], false);
    let markers: Vec<&str> =
        sample["markers"].as_array().unwrap().iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert_eq!(markers, ["intro", "credits"]);
    assert!(!sample["cast"].as_array().unwrap().is_empty());
    assert_eq!(sample["in_watchlist"], false);
    assert_eq!(api.get("/api/items/15").json()["in_watchlist"], true);
    assert_eq!(api.get("/api/items/99999999").status, 404);

    let dv = api.get(&format!("/api/items/{}", ids::DOLBY_VISION_P81)).json();
    assert_eq!(
        (dv["files"][0]["streams"][0]["dv_profile"].as_i64(), dv["files"][0]["streams"][0]["dv_bl_compat"].as_i64()),
        (Some(8), Some(1))
    );

    let seasons = api.get(&format!("/api/items/{}/children", ids::SHOW)).json();
    assert_eq!(titles(&seasons["items"]), ["Season 1", "Season 2"]);
    let episodes = api.get(&format!("/api/items/{}/children", ids::SHOW_SEASON_1)).json();
    assert_eq!(titles(&episodes["items"]), ["Episode 1", "Episode 2", "Episode 3"]);
    assert!(episodes["items"][0]["summary"].is_string(), "episodes keep their summaries in lists");
    assert_eq!(episodes["items"][1]["user_state"]["position_ms"], 9_000);
    let show = api.get(&format!("/api/items/{}", ids::SHOW)).json();
    assert_eq!(show["viewed_leaf_count"], 1);
    assert_eq!(show["child_count"], 2);
    let s1e1 = api.get(&format!("/api/items/{}", ids::SHOW_S1E1)).json();
    assert_eq!(s1e1["markers"][0]["start_ms"], ids::INTRO_START_MS);
    assert_eq!(s1e1["markers"][1]["start_ms"], ids::CREDITS_START_MS);

    let deck = api.get(&format!("/api/items/{}/on-deck", ids::SHOW)).json();
    assert_eq!(deck["item"]["id"], ids::SHOW_S1E2);
    let next = api.get(&format!("/api/items/{}/next", ids::SHOW_S1E3)).json();
    assert_eq!(next["item"]["id"], ids::SHOW_S2E1);
    let next_track = api.get(&format!("/api/items/{}/next", ids::TRACK)).json();
    assert_eq!(next_track["item"]["id"], ids::TRACK + 1);
    assert!(api.get(&format!("/api/items/{}/next", ids::TRACK + 9)).json()["item"].is_null());
    let tracks = api.get(&format!("/api/items/{}/children", ids::ALBUM)).json();
    assert_eq!(tracks["items"].as_array().unwrap().len(), 10);
    assert_eq!(tracks["items"][0]["grandparent_id"], ids::ARTIST);
    let related = api.get("/api/items/1/related?limit=5").json();
    assert_eq!(related["items"].as_array().unwrap().len(), 5);

    let person_id = sample["cast"][0]["id"].as_i64().unwrap();
    let person = api.get(&format!("/api/people/{person_id}")).json();
    assert!(!person["credits"].as_array().unwrap().is_empty());
    assert!(person["credits"][0]["roles"][0]["role"].is_string());
}

#[test]
fn home_search_and_collections() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let hubs = api.get("/api/hubs/home").json();
    let keys: Vec<&str> = hubs["hubs"].as_array().unwrap().iter().map(|h| h["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["continue_watching", "my_list", "recently_added:1", "recently_added:2", "recently_added:3"]);
    assert_eq!(hubs["hubs"][0]["items"][0]["id"], 7, "the last played first");
    assert_eq!(hubs["hubs"][0]["items"][1]["id"], ids::SHOW_S1E2);
    assert!(hubs["hubs"][0]["library_id"].is_null());
    assert_eq!(hubs["hubs"][1]["items"][0]["id"], 17, "My List: the newest added first");
    assert_eq!(hubs["hubs"][2]["title"], "Recently Added in Movies");
    let with_summaries = api.get("/api/hubs/home?summaries=1").json();
    assert!(with_summaries["hubs"][2]["items"][0]["summary"].is_string());

    let found = api.get("/api/search?q=fixture").json();
    assert_eq!(found["items"][0]["title"], "Fixture Show");
    let people = api.get("/api/search?q=ada").json();
    assert!(people["people"][0]["name"].as_str().unwrap().starts_with("Ada"));
    assert_eq!(api.get("/api/search?q=").json()["items"], json!([]));
    let channels = api.get("/api/search?q=news").json();
    assert!(!channels["channels"].as_array().unwrap().is_empty());
    // Up to `limit` of each: people and channels as titles, past the 12 they once stopped at.
    let len = |v: &Value, key: &str| v[key].as_array().unwrap().len();
    let wide = api.get("/api/search?q=a&limit=200").json();
    assert!(len(&wide, "people") > 12 && len(&wide, "people") <= 200, "{} people", len(&wide, "people"));
    assert!(len(&wide, "items") > 12 && len(&wide, "items") <= 200);
    assert!(len(&wide, "channels") > 12, "{} channels", len(&wide, "channels"));
    let narrow = api.get("/api/search?q=a&limit=5").json();
    assert_eq!((len(&narrow, "items"), len(&narrow, "people")), (5, 5));
    assert!(len(&narrow, "channels") <= 5);

    let collections = api.get("/api/libraries/1/collections").json();
    let list = collections["collections"].as_array().unwrap();
    assert!(list.len() >= 20);
    let smart = list.iter().find(|c| c["id"] == ids::SMART_COLLECTION).unwrap();
    assert!(smart["item_count"].as_i64().unwrap() > 0);
    assert_eq!(smart["smart_filter"], "genre=Comedy&decade=1990");
    let detail = api.get(&format!("/api/collections/{}", ids::COLLECTION)).json();
    assert_eq!(detail["items"].as_array().unwrap().len() as i64, detail["item_count"].as_i64().unwrap());
    let part_of = api.get(&format!("/api/items/{}/collections", detail["items"][0]["id"])).json();
    assert!(part_of["collections"].as_array().unwrap().iter().any(|c| c["id"] == ids::COLLECTION));
}

#[test]
fn playlists_made_changed_and_reordered() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let list = api.get("/api/playlists").json();
    assert_eq!(titles(&list["playlists"]), ["Favourites", "Road Trip", "Unwatched Comedies"]);
    let audio = api.get(&format!("/api/playlists/{}", ids::PLAYLIST_AUDIO)).json();
    assert_eq!(audio["kind"], "audio");
    assert_eq!(audio["entries"].as_array().unwrap().len(), 20);
    let smart = api.get(&format!("/api/playlists/{}", ids::PLAYLIST_SMART)).json();
    assert!(smart["item_count"].as_i64().unwrap() > 0);
    assert_eq!(
        api.post(&format!("/api/playlists/{}/items", ids::PLAYLIST_SMART), &json!({ "item_ids": [1] })).status,
        400
    );

    let made = api.post("/api/playlists", &json!({ "title": "Weekend", "item_ids": [1, ids::SHOW] }));
    assert_eq!(made.status, 201, "{}", made.text());
    let made = made.json();
    assert_eq!(made["kind"], "video");
    assert_eq!(made["item_count"], 7, "a show adds its episodes");
    let id = made["id"].as_i64().unwrap();
    let renamed = api.json("PATCH", &format!("/api/playlists/{id}"), &json!({ "title": "Sunday" })).json();
    assert_eq!(renamed["title"], "Sunday");
    let added = api.post(&format!("/api/playlists/{id}/items"), &json!({ "item_ids": [2] })).json();
    assert_eq!(added["added"], 1);
    assert_eq!(api.post(&format!("/api/playlists/{id}/items"), &json!({ "item_ids": [ids::TRACK] })).status, 400);
    let entries: Vec<i64> = api.get(&format!("/api/playlists/{id}")).json()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["entry_id"].as_i64().unwrap())
        .collect();
    let last = *entries.last().unwrap();
    let reordered = api.put(&format!("/api/playlists/{id}/order"), &json!({ "entry_ids": [last] })).json();
    assert_eq!(reordered["entries"][0]["entry_id"], last);
    assert_eq!(api.delete(&format!("/api/playlists/{id}/items/{}", entries[0])).status, 204);
    assert_eq!(api.get(&format!("/api/playlists/{id}")).json()["item_count"], 7);
    assert_eq!(api.delete(&format!("/api/playlists/{id}")).status, 204);
    assert_eq!(api.get(&format!("/api/playlists/{id}")).status, 404);
    let robin = Api::new(&server, ids::ROBIN);
    assert_eq!(robin.get(&format!("/api/playlists/{}", ids::PLAYLIST_VIDEO)).status, 404, "Alex's own");
    assert_eq!(server.writes("POST", "/api/playlists").len(), 1);
}

#[test]
fn my_list_continue_watching_and_history() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let listed = api.get("/api/watchlist").json();
    assert_eq!(listed["items"].as_array().unwrap().len(), 5);
    assert_eq!(api.put("/api/watchlist/1", &json!({})).status, 204);
    assert_eq!(api.get("/api/watchlist").json()["items"][0]["id"], 1);
    assert_eq!(api.delete("/api/watchlist/1").status, 204);
    assert_eq!(api.put(&format!("/api/watchlist/{}", ids::SHOW_S1E1), &json!({})).status, 400);

    assert_eq!(api.delete(&format!("/api/continue-watching/{}", ids::SHOW_S1E2)).status, 204);
    let hubs = api.get("/api/hubs/home").json();
    assert!(hubs["hubs"][0]["items"].as_array().unwrap().iter().all(|i| i["id"] != ids::SHOW_S1E2));

    let history = api.get("/api/history?limit=5").json();
    assert_eq!(history["total"], 22);
    let entries = history["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 5);
    assert!(entries[0]["viewed_at"].as_i64() >= entries[1]["viewed_at"].as_i64());
    assert_eq!(entries[0]["user"]["name"], "Alex");
    let shows = api.get(&format!("/api/history?item_id={}", ids::SHOW)).json();
    assert_eq!(shows["total"], 1);
    assert_eq!(shows["entries"][0]["item"]["id"], ids::SHOW_S1E1);
    let id = entries[0]["id"].as_i64().unwrap();
    assert_eq!(api.delete(&format!("/api/history/{id}")).status, 204);
    assert_eq!(api.get("/api/history").json()["total"], 21);
    assert_eq!(api.delete(&format!("/api/history/{id}")).status, 404);
}

#[test]
fn pictures_at_the_asked_width() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let poster = api.get("/api/items/1/images/poster?w=200&v=1");
    assert_eq!(poster.status, 200);
    assert_eq!(poster.header("Content-Type"), Some("image/jpeg"));
    assert_eq!(poster.header("Cache-Control"), Some("private, max-age=31536000, immutable"));
    let mut decoder = jpeg_decoder::Decoder::new(&poster.body[..]);
    decoder.decode().unwrap();
    let info = decoder.info().unwrap();
    assert_eq!((info.width, info.height), (240, 360), "200 rounds up to 240");
    let backdrop = api.get(&format!("/api/items/{}/images/backdrop", ids::SHOW_S1E1));
    assert_eq!(backdrop.header("Cache-Control"), Some("private, max-age=300"));
    let mut decoder = jpeg_decoder::Decoder::new(&backdrop.body[..]);
    decoder.decode().unwrap();
    assert_eq!(decoder.info().unwrap().width, 1_280);
    assert_eq!(api.get(&format!("/api/items/{}/images/thumb", ids::SHOW)).status, 404);
    assert_eq!(api.get(&format!("/api/items/{}/images/poster?w=160", ids::ALBUM)).status, 200);
    assert_eq!(api.get("/api/collections/2/images/poster?w=320").status, 200);
    assert_eq!(api.get("/api/users/1/avatar").header("Content-Type"), Some("image/jpeg"));
    assert_eq!(api.get(&format!("/api/users/{}/avatar", ids::SAM)).status, 404);
    assert_eq!(api.get("/api/people/1/photo?w=160").status, 200);
    assert_eq!(api.get("/api/people/7/photo").status, 404, "every 7th person has no photo");
    assert_eq!(api.get("/api/items/1/images/poster?w=200").status, 200);
}

#[test]
fn track_defaults_and_library_order() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let me = api.put(
        "/api/me/track-defaults",
        &json!({ "audio_language": "fre", "subtitle_language": null, "subtitle_mode": "always" }),
    );
    assert_eq!(me.status, 200, "{}", me.text());
    assert_eq!(me.json()["subtitle_mode"], "always");
    assert_eq!(api.put("/api/me/track-defaults", &json!({ "audio_language": "French" })).status, 400);
    let sample = api.get(&format!("/api/items/{}", ids::PGS_MOVIE)).json();
    assert_eq!(sample["files"][0]["selected_audio"], 2, "the French track");

    let order = api.put("/api/me/libraries/order", &json!({ "ids": [3, 1, 2] }));
    assert_eq!(order.status, 200, "{}", order.text());
    assert_eq!(api.get("/api/libraries").json()[0]["id"], 3);
    assert_eq!(api.get("/api/libraries?order=shared").json()[0]["id"], 1);
    assert_eq!(api.put("/api/me/libraries/order", &json!({ "ids": [3, 1] })).status, 409);
    assert_eq!(api.delete("/api/me/libraries/order").status, 200);
    assert_eq!(api.get("/api/libraries").json()[0]["id"], 1);
}
