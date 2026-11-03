//! Requests and Discover against the fixture server: the summary, the lists,
//! a title's page, asking and following, the listing's filters, managers'
//! decisions, and requests turned off.

mod common;

use std::time::Duration;

use common::{Api, Events};
use pley_fixtures::{FixtureServer, ids, request_titles as titles};
use serde_json::{Value, json};

fn ids_of(list: &Value) -> Vec<i64> {
    list["requests"].as_array().unwrap().iter().map(|r| r["id"].as_i64().unwrap()).collect()
}

#[test]
fn the_summary_says_what_each_profile_may_do() {
    let server = FixtureServer::start();
    let alex = Api::alex(&server).get("/api/requests/summary").json();
    assert_eq!(alex["enabled"], true);
    assert_eq!(alex["access"], "auto");
    assert_eq!(alex["manage"], true);
    assert_eq!((alex["pending"].as_i64(), alex["failed"].as_i64()), (Some(1), Some(1)));
    assert_eq!(alex["movies_left"], Value::Null, "no limits");

    let sam = Api::new(&server, ids::SAM).get("/api/requests/summary").json();
    assert_eq!(sam["access"], "ask");
    assert_eq!(sam["manage"], false);
    assert_eq!(sam["pending"], 0, "managers' counts only");
    assert_eq!(sam["own"], 4);
    assert_eq!(sam["unseen"], 2, "the partly ready show and the declined movie");
    assert_eq!((sam["movies_left"].as_i64(), sam["movies_limit"].as_i64()), (Some(1), Some(2)));
    assert_eq!((sam["seasons_left"].as_i64(), sam["seasons_limit"].as_i64()), (Some(2), Some(4)));

    let robin = Api::new(&server, ids::ROBIN).get("/api/requests/summary").json();
    assert_eq!(robin["access"], "off");
    assert_eq!(robin["own"], 1, "the request Robin follows");
}

#[test]
fn lists_page_through_titles_with_their_status() {
    let server = FixtureServer::start();
    let api = Api::alex(&server);
    let first = api.get("/api/discover/trending").json();
    assert_eq!(first["page"], 1);
    assert!(first["total_pages"].as_i64().unwrap() > 2);
    let results = first["results"].as_array().unwrap();
    assert_eq!(results.len(), 20);
    assert_eq!(results[0]["tmdb_id"], titles::TRENDING_FIRST);
    assert_eq!(results[0]["status"], "none");
    assert!(results[0]["poster_url"].as_str().unwrap().starts_with("/api/discover/images/poster/"));
    let kinds: Vec<&str> = results.iter().map(|t| t["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"movie") && kinds.contains(&"show"), "trending mixes both");
    let second = api.get("/api/discover/trending?page=2").json();
    assert_ne!(second["results"][0]["tmdb_id"], results[0]["tmdb_id"]);
    assert_eq!(api.get("/api/discover/nothing").status, 404);
    assert_eq!(api.get("/api/discover/movies?page=0").status, 400);

    let shows = api.get("/api/discover/shows").json();
    assert!(shows["results"].as_array().unwrap().iter().all(|t| t["kind"] == "show"));
    let all: Vec<Value> = (1..=2)
        .flat_map(|p| api.get(&format!("/api/discover/movies?page={p}")).json()["results"].as_array().unwrap().clone())
        .collect();
    let status = |id: i64| all.iter().find(|t| t["tmdb_id"] == id).unwrap()["status"].clone();
    assert_eq!(status(titles::IN_LIBRARY), "library");
    assert_eq!(status(titles::PENDING), "requested");
    assert_eq!(status(titles::DECLINED), "none", "declined for Sam, not for Alex");
    let sams = Api::new(&server, ids::SAM).get(&format!("/api/discover/movie/{}", titles::DECLINED)).json();
    assert_eq!(sams["status"], "declined");
    assert_eq!(sams["declined"]["reason"], "Not on any service");

    let found = api.get("/api/discover/search?q=fixture").json();
    assert_eq!(found["results"][0]["tmdb_id"], titles::SHOW_PARTIAL);
    assert_eq!(api.get("/api/discover/search?q=northern&missing=1").json()["results"], json!([]));
    assert_eq!(api.get("/api/discover/search?q=").status, 400);
    let picture = api.get("/api/discover/images/poster/p9001.jpg?w=240");
    assert_eq!((picture.status, picture.header("Content-Type")), (200, Some("image/jpeg")));
    assert_eq!(api.get("/api/discover/images/poster/..%2Fx.jpg").status, 404);
}

#[test]
fn a_shows_page_has_its_seasons_and_requests() {
    let server = FixtureServer::start();
    let sam = Api::new(&server, ids::SAM);
    let show = sam.get(&format!("/api/discover/show/{}", titles::SHOW_REQUESTED)).json();
    assert_eq!(show["status"], "partial");
    let states: Vec<(i64, &str)> = show["seasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["number"].as_i64().unwrap(), s["state"].as_str().unwrap()))
        .collect();
    assert_eq!(states, [(0, "missing"), (1, "library"), (2, "requested"), (3, "missing")]);
    let request = &show["requests"][0];
    assert_eq!((request["stage"].as_str(), request["mine"].as_bool()), (Some("partial"), Some(true)));
    assert_eq!(request["available_seasons"], json!([1]));
    assert_eq!(show["seasons_left"], 2);
    assert_eq!(show["cast"].as_array().unwrap().len(), 6);
    assert_eq!(sam.get("/api/discover/show/1").status, 404);

    let item = Api::alex(&server).get(&format!("/api/items/{}/request", ids::SHOW)).json();
    assert_eq!((item["status"].as_str(), item["askable"].clone()), (Some("partial"), json!([3])));
    assert_eq!(Api::alex(&server).get(&format!("/api/items/{}/request", ids::TRACK)).status, 404);
}

#[test]
fn asking_makes_a_request_follows_one_or_is_refused() {
    let server = FixtureServer::start();
    let alex = Api::alex(&server);
    let mut events = Events::open(&alex);
    let made = alex.post("/api/requests", &json!({ "kind": "movie", "tmdb_id": titles::TRENDING_FIRST }));
    assert_eq!(made.status, 201);
    let made = made.json();
    assert_eq!(
        (made["status"].as_str(), made["mine"].as_bool()),
        (Some("approved"), Some(true)),
        "Alex's are approved"
    );
    let event = events.wait_for("requests_changed", Duration::from_secs(5)).expect("an event");
    assert_eq!((event["kind"].as_str(), event["tmdb_id"].as_i64()), (Some("movie"), Some(titles::TRENDING_FIRST)));

    // Robin may not ask.
    let robin = Api::new(&server, ids::ROBIN);
    let refused = robin.post("/api/requests", &json!({ "kind": "movie", "tmdb_id": 10_002 }));
    assert_eq!((refused.status, refused.json()["code"].as_str()), (403, Some("no_access")));

    // Sam follows what someone asked for already, for nothing.
    let sam = Api::new(&server, ids::SAM);
    let followed = sam.post("/api/requests", &json!({ "kind": "movie", "tmdb_id": titles::DOWNLOADING }));
    assert_eq!(followed.status, 200);
    assert_eq!(followed.json()["watching"], true);
    // A pending request Sam makes; one more movie is past Sam's limit.
    let pending = sam.post("/api/requests", &json!({ "kind": "movie", "tmdb_id": 10_002 })).json();
    assert_eq!(pending["status"], "pending");
    let over = sam.post("/api/requests", &json!({ "kind": "movie", "tmdb_id": 10_003 }));
    assert_eq!(over.status, 429);
    assert!(over.json()["retry_at"].is_i64());
    // Declined lately.
    let declined = sam.post("/api/requests", &json!({ "kind": "movie", "tmdb_id": titles::DECLINED }));
    assert_eq!((declined.status, declined.json()["code"].as_str()), (409, Some("declined")));
    // A show's seasons: the missing ones, less what is asked for already.
    let show = alex.post("/api/requests", &json!({ "kind": "show", "tmdb_id": titles::SHOW_PARTIAL })).json();
    assert_eq!(show["seasons"], json!([3]));
    assert_eq!(alex.post("/api/requests", &json!({ "kind": "movie", "tmdb_id": titles::IN_LIBRARY })).status, 409);
}

#[test]
fn the_listing_filters_and_managers_decide() {
    let server = FixtureServer::start();
    let alex = Api::alex(&server);
    let all = alex.get("/api/requests").json();
    assert_eq!(all["total"], 6);
    assert_eq!(all["counts"], json!({ "pending": 1, "approved": 2, "declined": 1, "failed": 1, "available": 1 }));
    let names: Vec<&str> = all["people"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Alex", "Sam"]);
    let attention = alex.get("/api/requests?attention=1&order=oldest").json();
    assert_eq!(ids_of(&attention), [4, 1], "the oldest first");
    assert_eq!(ids_of(&alex.get("/api/requests?status=declined,failed").json()), [4, 5]);
    assert_eq!(ids_of(&alex.get(&format!("/api/requests?user_id={}", ids::ALEX)).json()), [2, 6]);
    assert_eq!(ids_of(&alex.get("/api/requests?q=glass").json()), [5]);
    assert_eq!(alex.get("/api/requests?status=nothing").status, 400);

    let pending = alex.get("/api/requests/1").json();
    assert_eq!(pending["actions"], json!(["approve", "decline", "handled", "delete"]));
    let approved = alex.post("/api/requests/1/approve", &json!({})).json();
    assert_eq!((approved["status"].as_str(), approved["stage"].as_str()), (Some("approved"), Some("searching")));
    let declined = alex.post("/api/requests/4/decline", &json!({ "reason": "Out of print" })).json();
    assert_eq!((declined["status"].as_str(), declined["reason"].as_str()), (Some("declined"), Some("Out of print")));
    let handled = alex.post("/api/requests/5/handled", &json!({})).json();
    assert_eq!(handled["note"], "Added by hand");
    assert_eq!(alex.delete("/api/requests/6").status, 204);
    assert_eq!(alex.get("/api/requests/6").status, 404);

    // Sam can't decide, reads only Sam's, and sees the reason.
    let sam = Api::new(&server, ids::SAM);
    assert_eq!(sam.post("/api/requests/3/approve", &json!({})).status, 403);
    assert_eq!(sam.get("/api/requests/2").status, 404);
    let mine = sam.get("/api/requests").json();
    assert_eq!(mine["people"], json!([]));
    assert_eq!(sam.get("/api/requests/4").json()["reason"], "Out of print");
    assert_eq!(sam.post("/api/requests/seen", &json!({})).status, 204);
    assert_eq!(sam.get("/api/requests/summary").json()["unseen"], 0);

    // A follower stops following.
    let robin = Api::new(&server, ids::ROBIN);
    assert_eq!(robin.get("/api/requests/2").json()["actions"], json!(["unwatch"]));
    assert_eq!(robin.delete("/api/requests/2/watch").status, 204);
    assert_eq!(robin.get("/api/requests/2").status, 404);
}

#[test]
fn requests_turned_off_answer_unavailable() {
    let server = FixtureServer::start();
    server.set_requests(false);
    let api = Api::alex(&server);
    let summary = api.get("/api/requests/summary").json();
    assert_eq!((summary["enabled"].as_bool(), summary["own"].as_i64()), (Some(false), Some(0)));
    let refused = api.get("/api/discover/trending");
    assert_eq!((refused.status, refused.json()["code"].as_str()), (503, Some("unavailable")));
    assert_eq!(api.get("/api/requests").status, 503);
}
