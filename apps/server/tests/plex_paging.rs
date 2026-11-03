//! Paged, sorted and filtered Plex section listings (filtered, sorted and
//! paged in SQL, see `plex::library`): the pages add up to the whole
//! listing, `totalSize` counts every match, ties always come out in the same
//! order, and access rules hold on every page.

mod common;

use std::collections::HashMap;

use axum::http::{Method, StatusCode};
use serde_json::Value;

use common::{TestApp, plex_titles};

const MOVIES: &[&str] = &[
    "Alien (1979)",
    "The Alien (2003)",
    "Brazil (1985)",
    "Ran (1985)",
    "2001 A Space Odyssey (1968)",
    "Zardoz (1974)",
    "École (2010)",
    "Heat (1995)",
    "Up (2009)",
    "Akira (1988)",
    "Jaws (1975)",
    "Klute (1971)",
    "Ikiru (1952)",
    "Mud (2012)",
    "Rope (1948)",
];

struct World {
    app: TestApp,
    admin: String,
    kid: String,
    movies: i64,
    ids: HashMap<String, i64>,
}

/// The movies above, a few with genres and ratings, "Alien" and "The Alien"
/// sharing a sort title and a year, and a kid who may watch titles up to 12
/// (rated PG or G here).
async fn world() -> World {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for name in MOVIES {
        app.media_file(&format!("movies/{name}.mkv"), 1024);
    }
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let ids = app.ids(movies, &admin).await;
    for (title, year, rating, genres) in [
        ("The Alien", Some(1979), Some(7.0), r#"["Horror","Science Fiction"]"#),
        ("Alien", None, Some(7.0), r#"["Horror","Science Fiction"]"#),
        ("Heat", None, None, r#"["Crime"]"#),
        ("Up", None, Some(8.2), r#"["Animation","Family"]"#),
        ("Zardoz", None, None, r#"["science fiction"]"#),
    ] {
        sqlx::query("UPDATE items SET year = COALESCE(?, year), rating = ?, genres = ? WHERE id = ?")
            .bind(year)
            .bind(rating)
            .bind(genres)
            .bind(ids[title])
            .execute(&app.state.db.write)
            .await
            .unwrap();
    }
    for title in ["Up", "Ran", "Akira", "Heat", "École", "2001 A Space Odyssey"] {
        app.set_rating(title, Some("PG")).await;
    }
    app.set_rating("Alien", Some("R")).await;
    app.set_rating("Brazil", Some("G")).await;
    let (kid, _) = app.user(&admin, "kid", serde_json::json!({"max_age": 12})).await;
    World { app, admin, kid, movies, ids }
}

/// `MediaContainer` of one page.
async fn page(app: &TestApp, token: &str, uri: &str, start: usize, size: usize) -> Value {
    let sep = if uri.contains('?') { '&' } else { '?' };
    app.plex_json(&format!("{uri}{sep}X-Plex-Container-Start={start}&X-Plex-Container-Size={size}"), token).await
}

/// `ratingKey`s of a container's items.
fn keys(container: &Value) -> Vec<String> {
    container["Metadata"]
        .as_array()
        .map(|items| items.iter().map(|m| m["ratingKey"].as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// Every page of `uri`, `size` at a time: each says how many items there are
/// in all, and together they are the whole listing. Returns its titles.
async fn paged_titles(app: &TestApp, token: &str, uri: &str, size: usize) -> Vec<String> {
    let whole = app.plex_json(uri, token).await;
    let total = whole["totalSize"].as_u64().unwrap() as usize;
    assert_eq!(whole["size"].as_u64(), Some(total as u64), "{uri}");
    let (mut paged, mut titles) = (Vec::new(), Vec::new());
    let mut start = 0;
    while start < total + size {
        let found = page(app, token, uri, start, size).await;
        let on_page = keys(&found);
        assert_eq!(found["totalSize"].as_u64(), Some(total as u64), "{uri} from {start}");
        assert_eq!(found["offset"].as_u64(), Some(start as u64), "{uri} from {start}");
        assert_eq!(found["size"].as_u64(), Some(on_page.len() as u64), "{uri} from {start}");
        assert_eq!(on_page.len(), size.min(total.saturating_sub(start)), "{uri} from {start}");
        paged.extend(on_page);
        titles.extend(plex_titles(&found));
        start += size;
    }
    assert_eq!(paged, keys(&whole), "the pages of {uri} add up to the whole listing");
    titles
}

/// "Alien" and "The Alien" share their sort title: they come by id.
fn aliens(w: &World) -> [&'static str; 2] {
    if w.ids["Alien"] < w.ids["The Alien"] { ["Alien", "The Alien"] } else { ["The Alien", "Alien"] }
}

#[tokio::test]
async fn pages_add_up_in_every_order() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let all = format!("/library/sections/{}/all", w.movies);
    let [first, second] = aliens(&w);

    let by_title = paged_titles(app, admin, &all, 4).await;
    let expected = [
        "2001 A Space Odyssey",
        "Akira",
        first,
        second,
        "Brazil",
        "Heat",
        "Ikiru",
        "Jaws",
        "Klute",
        "Mud",
        "Ran",
        "Rope",
        "Up",
        "Zardoz",
        "École",
    ];
    assert_eq!(by_title, expected);
    assert_eq!(paged_titles(app, admin, &format!("{all}?type=1&sort=titleSort"), 5).await, expected);
    // Descending titles; equal titles still by id.
    let descending = paged_titles(app, admin, &format!("{all}?sort=titleSort:desc"), 3).await;
    assert_eq!(descending[..3], ["École", "Zardoz", "Up"]);
    assert_eq!(descending[11..13], [first, second]);
    assert_eq!(descending[13..], ["Akira", "2001 A Space Odyssey"]);

    // Years: ties by sort title, then id.
    let by_year = paged_titles(app, admin, &format!("{all}?sort=year"), 4).await;
    assert_eq!(by_year[..5], ["Rope", "Ikiru", "2001 A Space Odyssey", "Klute", "Zardoz"]);
    assert_eq!(by_year[5..8], ["Jaws", first, second]);
    assert_eq!(by_year[8..10], ["Brazil", "Ran"], "1985 twice: by title");
    let by_year_desc = paged_titles(app, admin, &format!("{all}?sort=year:desc"), 6).await;
    assert_eq!(by_year_desc[..3], ["Mud", "École", "Up"]);
    assert_eq!(by_year_desc[5..9], ["Brazil", "Ran", first, second]);
    assert_eq!(by_year_desc[13..], ["Ikiru", "Rope"]);

    // Ratings: missing ones last whichever the direction, ties by title.
    let by_rating = paged_titles(app, admin, &format!("{all}?sort=rating:desc"), 2).await;
    assert_eq!(by_rating[..3], ["Up", first, second]);
    let unrated: Vec<&str> = expected.iter().copied().filter(|t| !["Up", "Alien", "The Alien"].contains(t)).collect();
    assert_eq!(by_rating[3..], unrated);
    let by_rating = paged_titles(app, admin, &format!("{all}?sort=rating,titleSort:desc"), 2).await;
    assert_eq!(by_rating[..3], [first, second, "Up"]);
    assert_eq!(by_rating[3..5], ["École", "Zardoz"]);

    // Every item was added by the same scan: titles decide.
    assert_eq!(paged_titles(app, admin, &format!("{all}?sort=addedAt:desc"), 7).await.len(), 15);
}

#[tokio::test]
async fn filters_page_and_count_their_matches() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    let all = format!("/library/sections/{}/all", w.movies);
    let [first, second] = aliens(&w);
    for title in ["Akira", "Up", "École", "Alien"] {
        let scrobble = format!("/:/scrobble?key={}&identifier=com.plexapp.plugins.library", w.ids[title]);
        assert_eq!(app.plex(Method::GET, &scrobble, admin, &[]).await.status, StatusCode::OK);
    }

    let unwatched = paged_titles(app, admin, &format!("{all}?unwatched=1"), 4).await;
    assert_eq!(unwatched.len(), 11);
    assert!(!unwatched.iter().any(|t| ["Akira", "Up", "École", "Alien"].contains(&t.as_str())), "{unwatched:?}");
    assert_eq!(
        paged_titles(app, admin, &format!("{all}?unwatched=0&sort=titleSort:desc"), 3).await,
        ["École", "Up", "Alien", "Akira"]
    );
    let science = paged_titles(app, admin, &format!("{all}?genre=Science%20Fiction"), 2).await;
    assert_eq!(science, [first, second, "Zardoz"], "genres ignore case");
    let found = paged_titles(app, admin, &format!("{all}?genre=horror&unwatched=1"), 1).await;
    assert_eq!(found, ["The Alien"]);
    assert_eq!(
        paged_titles(app, admin, &format!("{all}?year%3E%3E=1990&sort=year:desc"), 2).await,
        ["Mud", "École", "Up", "Heat"]
    );
    assert_eq!(paged_titles(app, admin, &format!("{all}?title=al"), 1).await, [first, second]);
    assert!(paged_titles(app, admin, &format!("{all}?genre=Western"), 5).await.is_empty());

    // `limit` caps the whole listing; pages go through what's left of it.
    let capped = format!("{all}?limit=5&sort=titleSort:desc");
    assert_eq!(paged_titles(app, admin, &capped, 2).await, ["École", "Zardoz", "Up", "Rope", "Ran"]);
    let beyond = page(app, admin, &capped, 6, 3).await;
    assert_eq!((beyond["size"].as_u64(), beyond["totalSize"].as_u64()), (Some(0), Some(5)));
}

#[tokio::test]
async fn access_rules_hold_on_every_page() {
    let w = world().await;
    let (app, kid) = (&w.app, w.kid.as_str());
    let all = format!("/library/sections/{}/all", w.movies);
    let allowed = ["2001 A Space Odyssey", "Akira", "Brazil", "Heat", "Ran", "Up", "École"];
    assert_eq!(paged_titles(app, kid, &all, 2).await, allowed);
    assert_eq!(
        paged_titles(app, kid, &format!("{all}?sort=year:desc"), 3).await,
        ["École", "Up", "Heat", "Akira", "Brazil", "Ran", "2001 A Space Odyssey"]
    );
    assert_eq!(paged_titles(app, kid, &format!("{all}?sort=rating:desc"), 4).await[0], "Up");
    // Past the end of what the kid may see: nothing, and the right total.
    let beyond = page(app, kid, &all, 10, 5).await;
    assert_eq!((beyond["size"].as_u64(), beyond["totalSize"].as_u64()), (Some(0), Some(7)));
    assert!(plex_titles(&beyond).is_empty());
    // Filters count only what the kid may see.
    let horror = page(app, kid, &format!("{all}?genre=horror"), 0, 10).await;
    assert_eq!((horror["size"].as_u64(), horror["totalSize"].as_u64()), (Some(0), Some(0)));
    let found = page(app, kid, &format!("{all}?title=a"), 1, 2).await;
    assert_eq!(plex_titles(&found), ["Akira", "Brazil"]);
    assert_eq!(found["totalSize"].as_u64(), Some(5));
    // Across sections too.
    assert_eq!(paged_titles(app, kid, "/library/all?type=1", 3).await, allowed);
    // The administrator sees everything.
    assert_eq!(page(app, &w.admin, &all, 10, 5).await["totalSize"].as_u64(), Some(15));
}

#[tokio::test]
async fn first_characters_count_every_title_the_user_may_see() {
    let w = world().await;
    let letters = |container: &Value| -> Vec<(String, i64)> {
        container["Directory"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| (d["title"].as_str().unwrap().to_owned(), d["size"].as_i64().unwrap()))
            .collect()
    };
    let uri = format!("/library/sections/{}/firstCharacter", w.movies);
    let counts = letters(&w.app.plex_json(&uri, &w.admin).await);
    let expected: Vec<(String, i64)> = [
        ("#", 1),
        ("A", 3),
        ("B", 1),
        ("H", 1),
        ("I", 1),
        ("J", 1),
        ("K", 1),
        ("M", 1),
        ("R", 2),
        ("U", 1),
        ("Z", 1),
        ("É", 1),
    ]
    .iter()
    .map(|(l, n)| ((*l).to_owned(), *n))
    .collect();
    assert_eq!(counts, expected, "digits are '#', 'The Alien' is under A, 'École' under É");
    let kid = letters(&w.app.plex_json(&uri, &w.kid).await);
    let expected: Vec<(String, i64)> = [("#", 1), ("A", 1), ("B", 1), ("H", 1), ("R", 1), ("U", 1), ("É", 1)]
        .iter()
        .map(|(l, n)| ((*l).to_owned(), *n))
        .collect();
    assert_eq!(kid, expected);
}

#[tokio::test]
async fn sections_paged_together_and_newest_first() {
    let w = world().await;
    let (app, admin) = (&w.app, w.admin.as_str());
    // A second movie section with an "Alien" of its own.
    for name in ["Alien (1986)", "Aliens (1986)", "Zulu (1964)"] {
        app.media_file(&format!("more/{name}.mkv"), 1024);
    }
    let more = app.library(admin, "More Movies", "movies", "more").await;
    let more_ids = app.ids(more, admin).await;
    let whole = paged_titles(app, admin, "/library/all?type=1", 4).await;
    assert_eq!(whole.len(), 18);
    assert_eq!(whole.iter().filter(|t| *t == "Alien").count(), 2);
    for size in [1, 2, 5, 7] {
        assert_eq!(paged_titles(app, admin, "/library/all?type=1&sort=year:desc", size).await.len(), 18);
    }
    // Equal titles in different sections: section by section (in the order
    // they are listed, the newest last), then by id.
    let uri = "/library/all?type=1&title=alien&sort=titleSort:desc";
    assert_eq!(paged_titles(app, admin, uri, 1).await.len(), 4);
    let [first, second] = aliens(&w);
    let expected = [more_ids["Aliens"], w.ids[first], w.ids[second], more_ids["Alien"]].map(|id| id.to_string());
    assert_eq!(keys(&app.plex_json(uri, admin).await), expected);

    let recent = app.plex_json(&format!("/hubs/home/recentlyAdded?type=1&sectionID={more}"), admin).await;
    assert_eq!(recent["size"].as_u64(), Some(3));
    let recent = app.plex_json("/hubs/home/recentlyAdded?type=1", admin).await;
    assert_eq!(recent["size"].as_u64(), Some(18));
}

#[tokio::test]
async fn dates_out_of_range_are_harmless() {
    let w = world().await;
    let all = format!("/library/sections/{}/all", w.movies);
    for (filter, count) in [
        ("addedAt%3E%3E=-9999999999999y", 15),
        ("addedAt%3C%3C=-9999999999999y", 0),
        ("addedAt=-9999999999999y", 0),
        ("addedAt!=-9999999999999y", 15),
        ("addedAt=9223372036854775807", 0),
        ("addedAt%3E%3E=-99999999999999999999d", 15),
    ] {
        let found = w.app.plex_json(&format!("{all}?{filter}"), &w.admin).await;
        assert_eq!(found["totalSize"].as_u64(), Some(count), "{filter}");
    }
}
