//! Requests and Discover (docs/API.md, "Requests"): a small TMDB of made-up
//! movies and shows in Discover's five lists, titles' pages with their
//! seasons and cast, and requests to ask for them, follow, withdraw, approve,
//! decline and mark as added by hand, with the summary, the listing's filters
//! and the events a change sends.
//!
//! Alex manages requests (an administrator's sign-in with `approvals`) and
//! asks without limits, approved as made; Sam asks for an administrator to
//! decide, at most 2 movies and 4 seasons per 7 days; Robin may not ask, and
//! follows one of Alex's requests. At start there are six requests: Sam's
//! pending movie, failed movie, declined movie and partly ready show, and
//! Alex's downloading movie and available one. Every route but the summary
//! answers `503 unavailable` while requests are off
//! ([`FixtureServer::set_requests`](crate::FixtureServer::set_requests)).
//!
//! Apps' tokens here carry no `requests` or `approvals` scope (the fixture
//! server's sign-in knows four), so these routes take any app's token.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::http::Response;
use crate::ids;
use crate::pictures::{Shape, picture};
use crate::server::{
    APPROVALS, Answer, Cx, Need, REQUESTS, Route, State, bad_request, error, free, json, locked, no_content, not_found,
};

const DAY: i64 = 86_400;
/// A declined title stays declined this long.
const DECLINED_FOR: i64 = 30 * DAY;
/// Limits count the requests of this long.
const WEEK: i64 = 7 * DAY;
/// Titles on a page of a list.
const PAGE: usize = 20;
/// The requests a listing gives at most at once.
const LIST_MAX: usize = 200;

/// Who manages requests.
const MANAGER: i64 = ids::ALEX;

/// The made-up titles tests can count on.
pub mod titles {
    /// A movie in the movies library ([`crate::ids::SAMPLE_1080P`]).
    pub const IN_LIBRARY: i64 = 9001;
    /// Sam's pending request.
    pub const PENDING: i64 = 9002;
    /// Alex's request, downloading (42 %).
    pub const DOWNLOADING: i64 = 9003;
    /// Sam's declined request ("Not on any service").
    pub const DECLINED: i64 = 9004;
    /// Sam's request that couldn't be sent.
    pub const FAILED: i64 = 9005;
    /// Alex's request, available ([`crate::ids::SAMPLE_HDR10`]).
    pub const AVAILABLE: i64 = 9006;
    /// "Fixture Show" ([`crate::ids::SHOW`]): seasons 1 and 2 in the library, 3 missing.
    pub const SHOW_PARTIAL: i64 = 7001;
    /// Sam's request for seasons 1 and 2, season 1 ready; season 3 and specials left.
    pub const SHOW_REQUESTED: i64 = 7002;
    /// A show nobody asked for: season 1 aired, season 2 not yet.
    pub const SHOW_NEW: i64 = 7003;
    /// The first title of Trending: a movie nobody asked for.
    pub const TRENDING_FIRST: i64 = 10_001;
}

const ADJECTIVES: [&str; 12] = [
    "Silent", "Crimson", "Hidden", "Broken", "Golden", "Distant", "Frozen", "Burning", "Lost", "Bright", "Hollow",
    "Wild",
];
const NOUNS: [&str; 14] = [
    "Harbour", "Signal", "Garden", "Kingdom", "Orbit", "Valley", "Mirror", "Engine", "Archive", "Tide", "Forest",
    "Station", "Empire", "Shore",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Movie,
    Show,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Movie => "movie",
            Kind::Show => "show",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        match s {
            "movie" => Some(Kind::Movie),
            "show" => Some(Kind::Show),
            _ => None,
        }
    }
}

struct Season {
    number: i64,
    episodes: i64,
    aired: i64,
}

/// A movie or show on the pretend TMDB.
struct Title {
    kind: Kind,
    tmdb_id: i64,
    title: String,
    year: i64,
    rating: Option<f64>,
    /// The library's title, for everyone.
    item_id: Option<i64>,
    /// Shows: TMDB's seasons, specials first.
    seasons: Vec<Season>,
    /// Shows: the seasons a library has whole.
    library: Vec<i64>,
    anime: bool,
    upcoming: bool,
}

impl Title {
    fn movie(tmdb_id: i64, title: &str, year: i64) -> Title {
        Title {
            kind: Kind::Movie,
            tmdb_id,
            title: title.to_string(),
            year,
            rating: Some(6.0 + (tmdb_id % 30) as f64 / 10.0),
            item_id: None,
            seasons: Vec::new(),
            library: Vec::new(),
            anime: false,
            upcoming: false,
        }
    }

    fn show(tmdb_id: i64, title: &str, year: i64, seasons: Vec<Season>) -> Title {
        Title { kind: Kind::Show, seasons, ..Title::movie(tmdb_id, title, year) }
    }

    fn poster(&self) -> String {
        format!("/api/discover/images/poster/p{}.jpg", self.tmdb_id)
    }

    fn backdrop(&self) -> String {
        format!("/api/discover/images/backdrop/b{}.jpg", self.tmdb_id)
    }

    /// The aired seasons but specials.
    fn aired(&self) -> Vec<i64> {
        self.seasons.iter().filter(|s| s.number > 0 && s.aired > 0).map(|s| s.number).collect()
    }
}

/// A request.
#[derive(Clone)]
struct Req {
    id: i64,
    kind: Kind,
    tmdb_id: i64,
    seasons: Option<Vec<i64>>,
    available: Vec<i64>,
    status: &'static str,
    stage: &'static str,
    reason: Option<String>,
    note: Option<String>,
    maker: i64,
    followers: Vec<i64>,
    created_at: i64,
    updated_at: i64,
    decided_by: Option<i64>,
    decided_at: Option<i64>,
    available_at: Option<i64>,
    /// (progress, state, eta)
    download: Option<(f64, &'static str, Option<i64>)>,
    /// Profiles it has news for.
    unseen: Vec<i64>,
    sent: bool,
    handled: bool,
}

impl Req {
    fn active(&self) -> bool {
        matches!(self.status, "pending" | "approved" | "failed")
    }

    /// Declined within the last 30 days, at `now`.
    fn declined_lately(&self, now: i64) -> bool {
        self.status == "declined" && self.decided_at.is_some_and(|at| now < at + DECLINED_FOR)
    }

    fn asks_for(&self, season: i64) -> bool {
        self.seasons.as_ref().is_some_and(|s| s.contains(&season))
    }
}

/// What requests hold on the fixture server.
pub(crate) struct Requests {
    pub enabled: bool,
    titles: Vec<Title>,
    requests: Vec<Req>,
    next_id: i64,
    /// What asking does for each profile: `auto`, `ask` or `off`.
    access: HashMap<i64, &'static str>,
}

impl Requests {
    /// The titles and the requests of the start, `now` in unix seconds.
    pub fn new(now: i64) -> Requests {
        let mut t: Vec<Title> = Vec::new();
        let mut first = Title::movie(titles::TRENDING_FIRST, "Silent Harbour", 2026);
        first.rating = Some(7.8);
        t.push(first);
        t.push(Title { item_id: Some(ids::SAMPLE_1080P), ..Title::movie(titles::IN_LIBRARY, "Northern Lights", 2021) });
        t.push(Title::movie(titles::PENDING, "Paper Moons", 2025));
        t.push(Title::movie(titles::DOWNLOADING, "Iron Harbour", 2024));
        t.push(Title::movie(titles::DECLINED, "Glass River", 2019));
        t.push(Title::movie(titles::FAILED, "Quiet Fields", 2023));
        t.push(Title { item_id: Some(ids::SAMPLE_HDR10), ..Title::movie(titles::AVAILABLE, "Silver Line", 2022) });
        let three = |n| Season { number: n, episodes: 3, aired: 3 };
        t.push(Title {
            item_id: Some(ids::SHOW),
            library: vec![1, 2],
            ..Title::show(titles::SHOW_PARTIAL, "Fixture Show", 2020, vec![three(1), three(2), three(3)])
        });
        let eight = |n| Season { number: n, episodes: 8, aired: 8 };
        let specials = Season { number: 0, episodes: 2, aired: 2 };
        t.push(Title {
            library: vec![1],
            ..Title::show(titles::SHOW_REQUESTED, "Long Night", 2018, vec![specials, eight(1), eight(2), eight(3)])
        });
        let unaired = Season { number: 2, episodes: 10, aired: 0 };
        t.push(Title::show(
            titles::SHOW_NEW,
            "Starfall",
            2025,
            vec![Season { number: 1, episodes: 10, aired: 10 }, unaired],
        ));
        // The rest, made up in bulk.
        for i in 1..100 {
            let name = format!("{} {}", ADJECTIVES[i % 12], NOUNS[(i * 5 + 3) % 14]);
            let mut movie = Title::movie(
                titles::TRENDING_FIRST + i as i64,
                &format!("{name} {}", i / 12 + 1),
                1990 + (i as i64 * 7) % 36,
            );
            movie.upcoming = i % 4 == 0;
            t.push(movie);
        }
        for i in 1..=60 {
            let name = format!("The {} {}", NOUNS[i % 14], ADJECTIVES[(i * 7) % 12]);
            let seasons = (1..=(i as i64 % 4 + 1)).map(|n| Season { number: n, episodes: 10, aired: 10 }).collect();
            let mut show =
                Title::show(20_000 + i as i64, &format!("{name} {}", i / 14 + 1), 2000 + i as i64 % 26, seasons);
            show.anime = i % 3 == 0;
            t.push(show);
        }

        let req = |id, kind, tmdb_id, maker, status, stage, created_at| Req {
            id,
            kind,
            tmdb_id,
            seasons: None,
            available: Vec::new(),
            status,
            stage,
            reason: None,
            note: None,
            maker,
            followers: Vec::new(),
            created_at,
            updated_at: created_at,
            decided_by: None,
            decided_at: None,
            available_at: None,
            download: None,
            unseen: Vec::new(),
            sent: false,
            handled: false,
        };
        let mut requests = vec![
            req(1, Kind::Movie, titles::PENDING, ids::SAM, "pending", "pending", now - 2 * DAY),
            Req {
                download: Some((0.42, "downloading", Some(now + 20 * 60))),
                followers: vec![ids::ROBIN],
                decided_by: Some(ids::ALEX),
                decided_at: Some(now - DAY),
                sent: true,
                ..req(2, Kind::Movie, titles::DOWNLOADING, ids::ALEX, "approved", "downloading", now - DAY)
            },
            Req {
                seasons: Some(vec![1, 2]),
                available: vec![1],
                decided_by: Some(ids::ALEX),
                decided_at: Some(now - 6 * DAY),
                unseen: vec![ids::SAM],
                sent: true,
                ..req(3, Kind::Show, titles::SHOW_REQUESTED, ids::SAM, "approved", "partial", now - 6 * DAY)
            },
            Req {
                reason: Some("Radarr isn't answering".into()),
                ..req(4, Kind::Movie, titles::FAILED, ids::SAM, "failed", "failed", now - 3 * DAY)
            },
            Req {
                reason: Some("Not on any service".into()),
                decided_by: Some(ids::ALEX),
                decided_at: Some(now - 3 * DAY),
                unseen: vec![ids::SAM],
                ..req(5, Kind::Movie, titles::DECLINED, ids::SAM, "declined", "declined", now - 4 * DAY)
            },
            Req {
                decided_by: Some(ids::ALEX),
                decided_at: Some(now - 9 * DAY),
                available_at: Some(now - 5 * DAY),
                sent: true,
                ..req(6, Kind::Movie, titles::AVAILABLE, ids::ALEX, "available", "available", now - 10 * DAY)
            },
        ];
        for r in &mut requests {
            r.updated_at = r.decided_at.unwrap_or(r.created_at).max(r.available_at.unwrap_or(0));
        }
        let access = [(ids::ALEX, "auto"), (ids::SAM, "ask"), (ids::ROBIN, "off")].into_iter().collect();
        Requests { enabled: true, titles: t, requests, next_id: 7, access }
    }

    fn title(&self, kind: Kind, tmdb_id: i64) -> Option<&Title> {
        self.titles.iter().find(|t| t.kind == kind && t.tmdb_id == tmdb_id)
    }

    fn access(&self, profile: i64) -> &'static str {
        self.access.get(&profile).copied().unwrap_or("off")
    }

    /// Sets what asking does for `profile`.
    pub fn set_access(&mut self, profile: i64, access: &'static str) {
        self.access.insert(profile, access);
    }

    /// Changes the download of a request: its progress, or none.
    pub fn set_download(&mut self, id: i64, progress: Option<f64>, now: i64) {
        if let Some(r) = self.requests.iter_mut().find(|r| r.id == id) {
            r.download = progress.map(|p| (p, "downloading", Some(now + 20 * 60)));
            r.stage = if progress.is_some() { "downloading" } else { "searching" };
            r.updated_at = now;
        }
    }

    /// The limits of `profile` per 7 days: (movies, seasons), 0 for none.
    fn limits(profile: i64) -> (i64, i64) {
        if profile == ids::SAM { (2, 4) } else { (0, 0) }
    }

    /// What `profile` may still ask for and when one more may be: (movies_left, seasons_left,
    /// movies_next_at, seasons_next_at).
    fn left(&self, profile: i64, now: i64) -> (Option<i64>, Option<i64>, Option<i64>, Option<i64>) {
        let (ml, sl) = Requests::limits(profile);
        let counting: Vec<&Req> = self
            .requests
            .iter()
            .filter(|r| {
                r.maker == profile
                    && now - r.created_at < WEEK
                    && matches!(r.status, "pending" | "approved" | "available")
            })
            .collect();
        let movies: Vec<&&Req> = counting.iter().filter(|r| r.kind == Kind::Movie).collect();
        let seasons: i64 = counting.iter().filter_map(|r| r.seasons.as_ref()).map(|s| s.len() as i64).sum();
        let oldest = |kind: Kind| counting.iter().filter(|r| r.kind == kind).map(|r| r.created_at + WEEK).min();
        let movies_left = (ml > 0).then(|| (ml - movies.len() as i64).max(0));
        let seasons_left = (sl > 0).then(|| (sl - seasons).max(0));
        (
            movies_left,
            seasons_left,
            movies_left.filter(|l| *l == 0).and_then(|_| oldest(Kind::Movie)),
            seasons_left.filter(|l| *l == 0).and_then(|_| oldest(Kind::Show)),
        )
    }

    /// Whether `profile` reads `r`.
    fn sees(r: &Req, profile: i64) -> bool {
        profile == MANAGER || r.maker == profile || r.followers.contains(&profile)
    }

    /// The requests for a title `profile` reads.
    fn of_title(&self, kind: Kind, tmdb_id: i64, profile: i64) -> Vec<&Req> {
        let mut out: Vec<&Req> = self
            .requests
            .iter()
            .filter(|r| r.kind == kind && r.tmdb_id == tmdb_id && Requests::sees(r, profile))
            .collect();
        out.sort_by_key(|r| std::cmp::Reverse((r.created_at, r.id)));
        out
    }

    /// A season's state for `profile`.
    fn season_state(&self, t: &Title, s: &Season) -> &'static str {
        if t.library.contains(&s.number) {
            "library"
        } else if self
            .requests
            .iter()
            .any(|r| r.tmdb_id == t.tmdb_id && r.kind == t.kind && r.active() && r.asks_for(s.number))
        {
            "requested"
        } else if s.aired == 0 {
            "unaired"
        } else {
            "missing"
        }
    }

    /// A season declined lately for `profile`.
    fn season_declined(&self, t: &Title, number: i64, profile: i64, now: i64) -> bool {
        self.requests.iter().any(|r| {
            r.tmdb_id == t.tmdb_id
                && r.kind == t.kind
                && (r.maker == profile || r.followers.contains(&profile))
                && r.declined_lately(now)
                && r.asks_for(number)
        })
    }

    /// A title's status for `profile`, and its active request's id.
    fn status(&self, t: &Title, profile: i64, now: i64) -> (&'static str, Option<i64>) {
        let active: Vec<&Req> =
            self.requests.iter().filter(|r| r.kind == t.kind && r.tmdb_id == t.tmdb_id && r.active()).collect();
        let request_id = active.iter().find(|r| Requests::sees(r, profile)).map(|r| r.id);
        let declined = self.requests.iter().any(|r| {
            r.kind == t.kind
                && r.tmdb_id == t.tmdb_id
                && (r.maker == profile || r.followers.contains(&profile))
                && r.declined_lately(now)
        });
        let status = match t.kind {
            Kind::Movie if t.item_id.is_some() => "library",
            Kind::Movie if !active.is_empty() => "requested",
            Kind::Movie if declined => "declined",
            Kind::Movie => "none",
            Kind::Show => {
                let aired = t.aired();
                let lacking: Vec<i64> = aired.iter().copied().filter(|n| !t.library.contains(n)).collect();
                if !aired.is_empty() && lacking.is_empty() {
                    "library"
                } else if !lacking.is_empty() && lacking.iter().all(|n| active.iter().any(|r| r.asks_for(*n))) {
                    "requested"
                } else if declined {
                    "declined"
                } else if !t.library.is_empty() || !active.is_empty() {
                    "partial"
                } else {
                    "none"
                }
            }
        };
        (status, request_id)
    }

    fn title_json(&self, t: &Title, profile: i64, now: i64) -> Value {
        let (status, request_id) = self.status(t, profile, now);
        json!({
            "kind": t.kind.as_str(),
            "tmdb_id": t.tmdb_id,
            "title": t.title,
            "year": t.year,
            "overview": format!("{} is a made-up {} of the fixture server's own TMDB.", t.title, t.kind.as_str()),
            "poster_url": t.poster(),
            "backdrop_url": t.backdrop(),
            "rating": t.rating,
            "status": status,
            "item_id": t.item_id,
            "request_id": request_id,
        })
    }

    fn user(&self, cx: &Cx<'_>, state: &State, id: i64) -> Value {
        let _ = cx;
        match state.household.profile(id) {
            Some(p) => json!({
                "id": p.id,
                "name": p.name,
                "avatar_url": p.avatar_url(),
                "avatar_color": p.avatar_color,
            }),
            None => Value::Null,
        }
    }

    /// The actions `profile` may take on `r` now.
    fn actions(&self, r: &Req, profile: i64, now: i64) -> Vec<&'static str> {
        let manage = profile == MANAGER;
        let mut out = Vec::new();
        let lately = r.declined_lately(now);
        if r.maker == profile
            && !manage
            && (matches!(r.status, "pending" | "failed") || (r.status == "declined" && !lately))
        {
            out.push("withdraw");
        }
        if r.followers.contains(&profile) && !lately {
            out.push("unwatch");
        }
        if manage {
            if matches!(r.status, "pending" | "declined") {
                out.push("approve");
            }
            if r.status == "failed" {
                out.push("retry");
            }
            if matches!(r.status, "pending" | "failed" | "approved") {
                out.push("decline");
            }
            if matches!(r.status, "pending" | "failed" | "declined")
                || (r.status == "approved" && !r.sent && !r.handled)
            {
                out.push("handled");
            }
            out.push("delete");
        }
        let may_ask = self.access(profile) != "off";
        let theirs = r.maker == profile || r.followers.contains(&profile);
        if may_ask && theirs && (r.stage == "gone" || (r.maker == profile && r.status == "declined" && !lately)) {
            out.push("ask_again");
        }
        out
    }

    fn request_json(&self, cx: &Cx<'_>, state: &State, r: &Req, profile: i64) -> Value {
        let now = cx.now();
        let manage = profile == MANAGER;
        let mine = r.maker == profile;
        let t = self.title(r.kind, r.tmdb_id);
        let reason = match r.status {
            "declined" if mine || manage => r.reason.clone(),
            "failed" if manage => r.reason.clone(),
            _ => None,
        };
        let by = |id: Option<i64>| match id {
            Some(id) if mine || manage => self.user(cx, state, id),
            _ => Value::Null,
        };
        json!({
            "id": r.id,
            "kind": r.kind.as_str(),
            "tmdb_id": r.tmdb_id,
            "title": t.map(|t| t.title.clone()).unwrap_or_default(),
            "year": t.map(|t| t.year),
            "poster_url": t.map(Title::poster),
            "backdrop_url": t.map(Title::backdrop),
            "seasons": r.seasons,
            "available_seasons": r.available,
            "anime": t.is_some_and(|t| t.anime),
            "status": r.status,
            "stage": r.stage,
            "stage_at": Value::Null,
            "reason": reason,
            "note": if manage { r.note.clone() } else { None },
            "mine": mine,
            "watching": r.followers.contains(&profile),
            "watchers": if manage { r.followers.len() } else { 0 },
            "unseen": r.unseen.contains(&profile),
            "actions": self.actions(r, profile, now),
            "requested_by": by(Some(r.maker)),
            "decided_by": by(r.decided_by),
            "decided_at": r.decided_at,
            "created_at": r.created_at,
            "updated_at": r.updated_at,
            "available_at": r.available_at,
            "retry_at": Value::Null,
            "sent_to": if manage && r.sent {
                json!({ "service": if r.kind == Kind::Movie { "radarr" } else { "sonarr" }, "quality_profile": "HD-1080p", "root_folder": null })
            } else { Value::Null },
            "item_id": if r.status == "available" || r.stage == "partial" { t.and_then(|t| t.item_id) } else { None },
            "download": r.download.map(|(progress, state, eta)| json!({
                "progress": progress, "state": state, "eta": eta, "episodes": if r.kind == Kind::Movie { 1 } else { 8 },
            })),
        })
    }

    fn summary(&self, profile: i64, now: i64) -> Value {
        let access = self.access(profile);
        if !self.enabled {
            return json!({
                "enabled": false, "access": access, "movies_left": null, "seasons_left": null,
                "movies_limit": null, "seasons_limit": null, "movies_next_at": null, "seasons_next_at": null,
                "manage": false, "pending": 0, "failed": 0, "unseen": 0, "own": 0, "changed_at": null,
                "sign_in_again": null,
            });
        }
        let manage = profile == MANAGER;
        let (ml, sl) = Requests::limits(profile);
        let (movies_left, seasons_left, movies_next_at, seasons_next_at) = self.left(profile, now);
        let theirs = |r: &&Req| r.maker == profile || r.followers.contains(&profile);
        let count = |status: &str| if manage { self.requests.iter().filter(|r| r.status == status).count() } else { 0 };
        json!({
            "enabled": true,
            "access": access,
            "movies_left": movies_left,
            "seasons_left": seasons_left,
            "movies_limit": (ml > 0).then_some(ml),
            "seasons_limit": (sl > 0).then_some(sl),
            "movies_next_at": movies_next_at,
            "seasons_next_at": seasons_next_at,
            "manage": manage,
            "pending": count("pending"),
            "failed": count("failed"),
            "unseen": self.requests.iter().filter(theirs).filter(|r| r.unseen.contains(&profile)).count(),
            "own": self.requests.iter().filter(theirs).count(),
            "changed_at": self.requests.iter().filter(|r| Requests::sees(r, profile)).map(|r| r.updated_at).max(),
            "sign_in_again": null,
        })
    }

    fn detail_json(&self, cx: &Cx<'_>, state: &State, t: &Title, profile: i64) -> Value {
        let now = cx.now();
        let mut v = self.title_json(t, profile, now);
        let requests = self.of_title(t.kind, t.tmdb_id, profile);
        let declined =
            requests.iter().find(|r| r.declined_lately(now) && (r.maker == profile || r.followers.contains(&profile)));
        let (movies_left, seasons_left, movies_next_at, seasons_next_at) = self.left(profile, now);
        let seasons: Option<Vec<Value>> = (t.kind == Kind::Show).then(|| {
            t.seasons
                .iter()
                .map(|s| {
                    let state = self.season_state(t, s);
                    let request_id = requests.iter().find(|r| r.active() && r.asks_for(s.number)).map(|r| r.id);
                    json!({
                        "number": s.number,
                        "name": if s.number == 0 { "Specials".to_string() } else { format!("Season {}", s.number) },
                        "episodes": s.episodes,
                        "aired": s.aired,
                        "air_date": if s.aired == 0 { Some("2027-03-04") } else { Some("2020-01-10") },
                        "poster_url": format!("/api/discover/images/poster/s{}x{}.jpg", t.tmdb_id, s.number),
                        "in_library": if t.library.contains(&s.number) { s.episodes } else { 0 },
                        "requested": state == "requested",
                        "state": state,
                        "declined": self.season_declined(t, s.number, profile, now),
                        "request_id": request_id,
                    })
                })
                .collect()
        });
        let cast: Vec<Value> = (0..6)
            .map(|i| {
                json!({
                    "name": format!("{} {}", ["Ana", "Ben", "Cleo", "Dev", "Eli", "Fay"][i], NOUNS[(t.tmdb_id as usize + i) % 14]),
                    "character": format!("{} {}", ADJECTIVES[(t.tmdb_id as usize + i) % 12], ["Captain", "Pilot", "Doctor"][i % 3]),
                    "photo_url": (i % 3 != 2).then(|| format!("/api/discover/images/profile/c{}x{i}.jpg", t.tmdb_id)),
                })
            })
            .collect();
        let extra = json!({
            "original_title": null,
            "tagline": format!("Every {} has its night.", NOUNS[t.tmdb_id as usize % 14].to_lowercase()),
            "genres": if t.kind == Kind::Movie { vec!["Drama", "Thriller"] } else { vec!["Drama", "Mystery"] },
            "release_date": format!("{}-05-01", t.year),
            "runtime_minutes": (t.kind == Kind::Movie).then_some(112),
            "certification": "PG-13",
            "network": if t.kind == Kind::Show { "Fixture Network" } else { "Fixture Pictures" },
            "show_status": (t.kind == Kind::Show).then_some("Returning Series"),
            "anime": t.anime,
            "cast": cast,
            "seasons": seasons,
            "imdb_id": format!("tt{:07}", t.tmdb_id),
            "declined": declined.map(|r| json!({
                "reason": if r.maker == profile { r.reason.clone() } else { None },
                "at": r.decided_at,
                "again_at": r.decided_at.unwrap_or(now) + DECLINED_FOR,
            })),
            "restriction": null,
            "watching": requests.iter().any(|r| r.active() && r.followers.contains(&profile)),
            "requests": requests.iter().map(|r| self.request_json(cx, state, r, profile)).collect::<Vec<_>>(),
            "movies_left": movies_left,
            "seasons_left": seasons_left,
            "movies_next_at": movies_next_at,
            "seasons_next_at": seasons_next_at,
        });
        if let (Value::Object(base), Value::Object(more)) = (&mut v, extra) {
            base.extend(more);
        }
        v
    }

    /// The titles of a list, in its order.
    fn list(&self, list: &str) -> Option<Vec<&Title>> {
        let all = self.titles.iter();
        Some(match list {
            "trending" => {
                let movies: Vec<&Title> = all.clone().filter(|t| t.kind == Kind::Movie).collect();
                let shows: Vec<&Title> = all.filter(|t| t.kind == Kind::Show).collect();
                let mut out = Vec::new();
                for i in 0..movies.len().max(shows.len()) {
                    out.extend(movies.get(i));
                    out.extend(shows.get(i));
                }
                out
            }
            "movies" => all.filter(|t| t.kind == Kind::Movie).collect(),
            "shows" => all.filter(|t| t.kind == Kind::Show && !t.anime).collect(),
            "upcoming" => all.filter(|t| t.upcoming).collect(),
            "anime" => all.filter(|t| t.anime).collect(),
            _ => return None,
        })
    }

    fn page(&self, titles: Vec<&Title>, page: usize, profile: i64, now: i64) -> Value {
        let total_pages = titles.len().div_ceil(PAGE).max(1);
        let results: Vec<Value> =
            titles.iter().skip((page - 1) * PAGE).take(PAGE).map(|t| self.title_json(t, profile, now)).collect();
        json!({ "page": page, "total_pages": total_pages, "results": results })
    }
}

pub(crate) fn routes() -> Vec<Route> {
    vec![
        locked("GET", "/api/requests/summary", Need::AnyApp, |cx, state| {
            let me = cx.caller().profile;
            Ok(json(200, &state.requests.summary(me, cx.now())))
        }),
        free("GET", "/api/discover/images/*/*", Need::Scope(REQUESTS), image),
        locked("GET", "/api/discover/search", Need::Scope(REQUESTS), search),
        locked("GET", "/api/discover/*", Need::Scope(REQUESTS), list),
        locked("GET", "/api/discover/*/*", Need::Scope(REQUESTS), title),
        locked("GET", "/api/items/*/request", Need::Scope(REQUESTS), item_request),
        locked("GET", "/api/requests", Need::Scope(REQUESTS), listing),
        locked("POST", "/api/requests", Need::Scope(REQUESTS), ask),
        locked("POST", "/api/requests/seen", Need::Scope(REQUESTS), |cx, state| {
            on(state)?;
            let me = cx.caller().profile;
            for r in &mut state.requests.requests {
                r.unseen.retain(|p| *p != me);
            }
            Ok(no_content())
        }),
        locked("GET", "/api/requests/*", Need::Scope(REQUESTS), |cx, state| {
            on(state)?;
            let (i, me) = (readable(cx, state)?, cx.caller().profile);
            let r = state.requests.requests[i].clone();
            Ok(json(200, &state.requests.request_json(cx, state, &r, me)))
        }),
        locked("DELETE", "/api/requests/*", Need::Scope(REQUESTS), withdraw),
        locked("DELETE", "/api/requests/*/watch", Need::Scope(REQUESTS), unwatch),
        locked("POST", "/api/requests/*/approve", Need::Scope(APPROVALS), |cx, state| {
            decide(cx, state, Decision::Approve)
        }),
        locked("POST", "/api/requests/*/decline", Need::Scope(APPROVALS), |cx, state| {
            decide(cx, state, Decision::Decline)
        }),
        locked("POST", "/api/requests/*/handled", Need::Scope(APPROVALS), |cx, state| {
            decide(cx, state, Decision::Handled)
        }),
    ]
}

/// `503 unavailable` while requests are off.
fn on(state: &State) -> Result<(), Response> {
    if state.requests.enabled {
        Ok(())
    } else {
        Err(json(503, &json!({ "code": "unavailable", "message": "requests are off", "setup": false })))
    }
}

/// `403 no_access` for an account that may not ask.
fn may_ask(cx: &Cx<'_>, state: &State) -> Result<(), Response> {
    if state.requests.access(cx.caller().profile) == "off" {
        Err(error(403, "no_access", "this account may not ask for movies and shows"))
    } else {
        Ok(())
    }
}

fn page_of(cx: &Cx<'_>) -> Result<usize, Response> {
    match cx.query("page") {
        None => Ok(1),
        Some(p) => {
            p.parse::<usize>().ok().filter(|p| (1..=500).contains(p)).ok_or_else(|| bad_request("page is 1 to 500"))
        }
    }
}

/// GET /api/discover/{list}?page=
fn list(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    may_ask(cx, state)?;
    let page = page_of(cx)?;
    let titles = state.requests.list(cx.value(0)).ok_or_else(|| not_found("no such list"))?;
    Ok(json(200, &state.requests.page(titles, page, cx.caller().profile, cx.now())))
}

/// GET /api/discover/search?q=&page=&missing=
fn search(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    may_ask(cx, state)?;
    let q = cx.query("q").unwrap_or("").trim().to_lowercase();
    if q.is_empty() || q.chars().count() > 200 {
        return Err(bad_request("q is 1 to 200 characters"));
    }
    let page = page_of(cx)?;
    let missing = matches!(cx.query("missing"), Some("1" | "true" | "yes"));
    let (me, now) = (cx.caller().profile, cx.now());
    let r = &state.requests;
    let titles: Vec<&Title> = r
        .titles
        .iter()
        .filter(|t| t.title.to_lowercase().contains(&q))
        .filter(|t| !missing || r.status(t, me, now).0 != "library")
        .collect();
    Ok(json(200, &r.page(titles, page, me, now)))
}

/// GET /api/discover/{movie|show}/{tmdb_id}
fn title(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    may_ask(cx, state)?;
    let kind = Kind::parse(cx.value(0)).ok_or_else(|| not_found("no such list"))?;
    let tmdb_id = cx.id(1)?;
    let t = state.requests.title(kind, tmdb_id).ok_or_else(|| not_found("TMDB doesn't know this title"))?;
    Ok(json(200, &state.requests.detail_json(cx, state, t, cx.caller().profile)))
}

/// GET /api/items/{id}/request: the library's titles the pretend TMDB knows.
fn item_request(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    let id = cx.id(0)?;
    let (me, now) = (cx.caller().profile, cx.now());
    let r = &state.requests;
    let t = r.titles.iter().find(|t| t.item_id == Some(id)).ok_or_else(|| not_found("TMDB doesn't know this title"))?;
    let (status, _) = r.status(t, me, now);
    let request = r.of_title(t.kind, t.tmdb_id, me).into_iter().find(|q| q.active()).cloned();
    let askable: Vec<i64> = if t.kind == Kind::Show {
        t.aired().into_iter().filter(|n| !t.library.contains(n) && !r.season_declined(t, *n, me, now)).collect()
    } else {
        Vec::new()
    };
    Ok(json(
        200,
        &json!({
            "kind": t.kind.as_str(),
            "tmdb_id": t.tmdb_id,
            "status": status,
            "request": request.map(|q| r.request_json(cx, state, &q, me)),
            "askable": askable,
        }),
    ))
}

/// GET /api/requests with its filters.
fn listing(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    let me = cx.caller().profile;
    let manage = me == MANAGER;
    let flag = |name: &str| matches!(cx.query(name), Some("1" | "true" | "yes"));
    let statuses: Option<Vec<&str>> = match cx.query("status") {
        None | Some("") => None,
        Some(s) => {
            let list: Vec<&str> = s.split(',').collect();
            if list.iter().any(|s| !matches!(*s, "pending" | "approved" | "declined" | "failed" | "available")) {
                return Err(bad_request("no such status"));
            }
            Some(list)
        }
    };
    let kind = match cx.query("kind") {
        None | Some("") => None,
        Some(k) => Some(Kind::parse(k).ok_or_else(|| bad_request("kind is movie or show"))?),
    };
    let number = |name: &str| -> Result<Option<i64>, Response> {
        match cx.query(name) {
            None | Some("") => Ok(None),
            Some(v) => v.parse::<i64>().map(Some).map_err(|_| bad_request(&format!("{name} is a number"))),
        }
    };
    let user_id = number("user_id")?.filter(|_| manage);
    let offset = number("offset")?.unwrap_or(0).max(0) as usize;
    let limit = (number("limit")?.unwrap_or(50).clamp(0, LIST_MAX as i64)) as usize;
    let oldest = match cx.query("order") {
        None | Some("" | "newest") => false,
        Some("oldest") => true,
        Some(_) => return Err(bad_request("order is newest or oldest")),
    };
    let q = cx.query("q").map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
    let (attention, mine, unseen) = (flag("attention"), flag("mine"), flag("unseen"));
    let changed_since = number("changed_since")?;
    let r = &state.requests;
    let title_of = |q: &Req| r.title(q.kind, q.tmdb_id).map(|t| t.title.to_lowercase()).unwrap_or_default();
    // Every filter but the status and the person.
    let base: Vec<&Req> = r
        .requests
        .iter()
        .filter(|q| Requests::sees(q, me))
        .filter(|q| !mine || q.maker == me || q.followers.contains(&me))
        .filter(|q| !unseen || q.unseen.contains(&me))
        .filter(|q| !attention || matches!(q.status, "pending" | "failed"))
        .filter(|x| kind.is_none_or(|k| x.kind == k))
        .filter(|x| q.as_ref().is_none_or(|w| title_of(x).contains(w.as_str())))
        .filter(|x| changed_since.is_none_or(|t| x.updated_at > t))
        .collect();
    let by_person: Vec<&Req> = base.iter().copied().filter(|x| user_id.is_none_or(|u| x.maker == u)).collect();
    let count = |status: &str| by_person.iter().filter(|x| x.status == status).count();
    let counts = json!({
        "pending": count("pending"), "approved": count("approved"), "declined": count("declined"),
        "failed": count("failed"), "available": count("available"),
    });
    let mut kept: Vec<&Req> =
        by_person.iter().copied().filter(|x| statuses.as_ref().is_none_or(|s| s.contains(&x.status))).collect();
    kept.sort_by_key(|x| (x.created_at, x.id));
    if !oldest {
        kept.reverse();
    }
    let mut people: Vec<Value> = Vec::new();
    if manage {
        let mut makers: Vec<i64> =
            base.iter().filter(|x| statuses.as_ref().is_none_or(|s| s.contains(&x.status))).map(|x| x.maker).collect();
        makers.sort_unstable();
        makers.dedup();
        people = makers.into_iter().map(|id| r.user(cx, state, id)).collect();
        people.sort_by_key(|p| p["name"].as_str().unwrap_or("").to_string());
    }
    let page: Vec<Value> = kept.iter().skip(offset).take(limit).map(|x| r.request_json(cx, state, x, me)).collect();
    Ok(json(200, &json!({ "total": kept.len(), "counts": counts, "requests": page, "people": people })))
}

/// The index of the request of the path that the caller reads, or `404`.
fn readable(cx: &Cx<'_>, state: &State) -> Result<usize, Response> {
    let id = cx.id(0)?;
    let me = cx.caller().profile;
    state
        .requests
        .requests
        .iter()
        .position(|r| r.id == id && Requests::sees(r, me))
        .ok_or_else(|| not_found("request not found"))
}

/// Tells every stream that requests changed, for a title.
fn changed(cx: &Cx<'_>, id: Option<i64>, kind: Option<&str>, tmdb_id: Option<i64>) {
    let event = json!({ "type": "requests_changed", "request_id": id, "kind": kind, "tmdb_id": tmdb_id });
    cx.shared.events.emit_text(event.to_string(), None);
}

/// `409 conflict` with the request as it is now.
fn conflict(cx: &Cx<'_>, state: &State, i: usize, message: &str, retry_at: Option<i64>) -> Response {
    let me = cx.caller().profile;
    let r = state.requests.requests[i].clone();
    json(
        409,
        &json!({
            "code": "conflict",
            "message": message,
            "request": state.requests.request_json(cx, state, &r, me),
            "retry_at": retry_at,
        }),
    )
}

/// POST /api/requests `{ kind, tmdb_id, seasons? }`.
fn ask(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    may_ask(cx, state)?;
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't a request"))?;
    let kind = body["kind"].as_str().and_then(Kind::parse).ok_or_else(|| bad_request("kind is movie or show"))?;
    let tmdb_id = body["tmdb_id"].as_i64().ok_or_else(|| bad_request("tmdb_id is a number"))?;
    let asked: Option<Vec<i64>> = match &body["seasons"] {
        Value::Null => None,
        Value::Array(list) => Some(
            list.iter()
                .map(|v| v.as_i64().ok_or_else(|| bad_request("seasons are numbers")))
                .collect::<Result<_, _>>()?,
        ),
        _ => return Err(bad_request("seasons is a list")),
    };
    if kind == Kind::Movie && asked.is_some() {
        return Err(bad_request("a movie has no seasons"));
    }
    if asked.as_ref().is_some_and(Vec::is_empty) {
        return Err(bad_request("seasons: none asked for"));
    }
    let (me, now) = (cx.caller().profile, cx.now());
    let reqs = &state.requests;
    let t = reqs.title(kind, tmdb_id).ok_or_else(|| not_found("TMDB doesn't know this title"))?;
    if let Some(seasons) = &asked
        && seasons.iter().any(|n| !t.seasons.iter().any(|s| s.number == *n))
    {
        return Err(bad_request("TMDB doesn't list that season"));
    }
    let title_status = reqs.status(t, me, now).0;
    if kind == Kind::Movie && title_status == "library" {
        return Err(error(409, "in_library", "the library has it"));
    }
    let wanted: Vec<i64> = match (&asked, kind) {
        (_, Kind::Movie) => Vec::new(),
        (Some(s), _) => s.iter().copied().filter(|n| !t.library.contains(n)).collect(),
        (None, _) => {
            t.aired().into_iter().filter(|n| !t.library.contains(n) && !reqs.season_declined(t, *n, me, now)).collect()
        }
    };
    if kind == Kind::Show && wanted.is_empty() {
        return Err(error(409, "in_library", "the library has every season asked for"));
    }
    // What active requests ask for already is followed.
    let covering: Vec<usize> = reqs
        .requests
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.kind == kind
                && r.tmdb_id == tmdb_id
                && r.active()
                && (kind == Kind::Movie || wanted.iter().any(|n| r.asks_for(*n)))
        })
        .map(|(i, _)| i)
        .collect();
    let left: Vec<i64> =
        wanted.iter().copied().filter(|n| !covering.iter().any(|i| reqs.requests[*i].asks_for(*n))).collect();
    let nothing_left = if kind == Kind::Movie { !covering.is_empty() } else { left.is_empty() };
    let declined_at = reqs
        .requests
        .iter()
        .filter(|r| {
            r.kind == kind
                && r.tmdb_id == tmdb_id
                && (r.maker == me || r.followers.contains(&me))
                && r.declined_lately(now)
                && (kind == Kind::Movie || left.iter().any(|n| r.asks_for(*n)))
        })
        .filter_map(|r| r.decided_at)
        .max();
    if !nothing_left {
        if let Some(at) = declined_at {
            return Err(json(
                409,
                &json!({ "code": "declined", "message": "it was declined lately", "retry_at": at + DECLINED_FOR }),
            ));
        }
        let (movies_left, seasons_left, movies_next_at, seasons_next_at) = reqs.left(me, now);
        let (left_now, next_at, limit) = match kind {
            Kind::Movie => (movies_left, movies_next_at, Requests::limits(me).0),
            Kind::Show => (seasons_left, seasons_next_at, Requests::limits(me).1),
        };
        let need = if kind == Kind::Movie { 1 } else { left.len() as i64 };
        if left_now.is_some_and(|l| l < need) {
            let retry_at = if need > limit { None } else { next_at };
            return Err(json(
                429,
                &json!({ "code": "too_many_requests", "message": "past this account's limits", "retry_at": retry_at }),
            ));
        }
    }
    // Followed: what someone else asked for already.
    let auto = reqs.access(me) == "auto";
    let requests = &mut state.requests.requests;
    for i in &covering {
        let r = &mut requests[*i];
        if r.maker != me && !r.followers.contains(&me) {
            r.followers.push(me);
            r.updated_at = now;
        }
        if auto && r.status == "pending" {
            (r.status, r.stage, r.decided_by, r.decided_at, r.sent) =
                ("approved", "searching", Some(me), Some(now), true);
        }
    }
    let (status, i) = if nothing_left {
        let mut pick: Vec<usize> = covering.clone();
        pick.sort_by_key(|i| {
            let r = &requests[*i];
            (r.maker != me, std::cmp::Reverse(r.created_at))
        });
        (200, pick[0])
    } else {
        let id = state.requests.next_id;
        state.requests.next_id += 1;
        let (status, stage) = if auto { ("approved", "searching") } else { ("pending", "pending") };
        state.requests.requests.push(Req {
            id,
            kind,
            tmdb_id,
            seasons: (kind == Kind::Show).then_some(left),
            available: Vec::new(),
            status,
            stage,
            reason: None,
            note: None,
            maker: me,
            followers: Vec::new(),
            created_at: now,
            updated_at: now,
            decided_by: auto.then_some(me),
            decided_at: auto.then_some(now),
            available_at: None,
            download: None,
            unseen: Vec::new(),
            sent: auto,
            handled: false,
        });
        (201, state.requests.requests.len() - 1)
    };
    let r = state.requests.requests[i].clone();
    changed(cx, Some(r.id), Some(kind.as_str()), Some(tmdb_id));
    Ok(json(status, &state.requests.request_json(cx, state, &r, me)))
}

/// DELETE /api/requests/{id}: its maker withdraws it, a manager deletes it.
fn withdraw(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    let i = readable(cx, state)?;
    let (me, now) = (cx.caller().profile, cx.now());
    let r = state.requests.requests[i].clone();
    if me != MANAGER {
        if r.maker != me {
            return Err(conflict(cx, state, i, "only its maker withdraws it: stop following it instead", None));
        }
        if r.declined_lately(now) {
            let at = r.decided_at.map(|d| d + DECLINED_FOR);
            return Err(conflict(cx, state, i, "a declined request stays for 30 days", at));
        }
        if matches!(r.status, "approved" | "available") {
            return Err(conflict(cx, state, i, "it was approved already", None));
        }
        if let Some(first) = r.followers.first().copied() {
            // Its earliest follower takes it over.
            let taken = &mut state.requests.requests[i];
            taken.maker = first;
            taken.followers.remove(0);
            taken.updated_at = now;
            changed(cx, Some(r.id), Some(r.kind.as_str()), Some(r.tmdb_id));
            return Ok(no_content());
        }
    }
    state.requests.requests.remove(i);
    changed(cx, Some(r.id), Some(r.kind.as_str()), Some(r.tmdb_id));
    Ok(no_content())
}

/// DELETE /api/requests/{id}/watch.
fn unwatch(cx: &Cx<'_>, state: &mut State) -> Answer {
    on(state)?;
    let i = readable(cx, state)?;
    let (me, now) = (cx.caller().profile, cx.now());
    let r = state.requests.requests[i].clone();
    if !r.followers.contains(&me) {
        return Err(conflict(cx, state, i, "this account doesn't follow it", None));
    }
    if r.declined_lately(now) {
        let at = r.decided_at.map(|d| d + DECLINED_FOR);
        return Err(conflict(cx, state, i, "a declined request stays for 30 days", at));
    }
    state.requests.requests[i].followers.retain(|p| *p != me);
    changed(cx, Some(r.id), Some(r.kind.as_str()), Some(r.tmdb_id));
    Ok(no_content())
}

enum Decision {
    Approve,
    Decline,
    Handled,
}

/// POST /api/requests/{id}/approve, /decline and /handled (managers).
fn decide(cx: &Cx<'_>, state: &mut State, decision: Decision) -> Answer {
    on(state)?;
    let (me, now) = (cx.caller().profile, cx.now());
    if me != MANAGER {
        return Err(error(403, "forbidden", "managers only"));
    }
    let i = readable(cx, state)?;
    let reason = match decision {
        Decision::Decline => match cx.req.json() {
            Some(Value::Object(body)) => match body.get("reason") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) if s.chars().count() <= 500 => {
                    Some(s.trim().to_string()).filter(|s| !s.is_empty())
                }
                Some(_) => return Err(bad_request("reason is at most 500 characters")),
            },
            _ if cx.req.body.is_empty() => None,
            _ => return Err(bad_request("the body isn't a decision")),
        },
        _ => None,
    };
    let r = &state.requests.requests[i];
    let allowed = match decision {
        Decision::Approve => matches!(r.status, "pending" | "failed" | "declined" | "approved"),
        Decision::Decline => matches!(r.status, "pending" | "failed" | "approved"),
        Decision::Handled => {
            matches!(r.status, "pending" | "failed" | "declined") || (r.status == "approved" && !r.sent)
        }
    };
    if !allowed {
        return Err(conflict(cx, state, i, "that can't be done to it as it is now", None));
    }
    let r = &mut state.requests.requests[i];
    match decision {
        Decision::Approve if r.status == "approved" && r.sent => {}
        Decision::Approve => {
            (r.status, r.stage, r.sent, r.handled, r.note, r.reason) =
                ("approved", "searching", true, false, None, None);
            (r.decided_by, r.decided_at) = (Some(me), Some(now));
        }
        Decision::Decline => {
            (r.status, r.stage, r.reason, r.download) = ("declined", "declined", reason, None);
            (r.decided_by, r.decided_at) = (Some(me), Some(now));
            let mut news = vec![r.maker];
            news.extend(r.followers.iter().copied());
            r.unseen = news;
        }
        Decision::Handled => {
            (r.status, r.stage, r.handled, r.sent, r.reason) = ("approved", "approved", true, false, None);
            r.note = Some("Added by hand".into());
            (r.decided_by, r.decided_at) = (Some(me), Some(now));
        }
    }
    r.updated_at = now;
    let r = r.clone();
    changed(cx, Some(r.id), Some(r.kind.as_str()), Some(r.tmdb_id));
    Ok(json(200, &state.requests.request_json(cx, state, &r, me)))
}

/// GET /api/discover/images/{kind}/{file}?w=: drawn from the file's name.
fn image(cx: &Cx<'_>) -> Answer {
    let shape = match cx.value(0) {
        "poster" | "profile" => Shape::Poster,
        "backdrop" | "still" => Shape::Wide,
        _ => return Err(not_found("no such picture")),
    };
    let file = cx.value(1);
    let (stem, ext) = file.rsplit_once('.').ok_or_else(|| not_found("no such picture"))?;
    if !matches!(ext, "jpg" | "png")
        || stem.is_empty()
        || stem.len() > 64
        || !stem.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(not_found("no such picture"));
    }
    let seed = stem.bytes().fold(7i64, |h, b| h.wrapping_mul(31).wrapping_add(b as i64)).rem_euclid(1_000_000);
    let width = cx.query("w").and_then(|w| w.parse::<u32>().ok()).filter(|w| *w > 0);
    let bytes = cx.shared.pictures.artwork(seed, shape, width);
    Ok(picture(bytes, "image/jpeg", "private, max-age=604800, immutable"))
}
