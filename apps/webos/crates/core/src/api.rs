//! Pley's API for one server and one sign-in: a blocking method per
//! endpoint the TV uses (screens call them on [`crate::task::Tasks`]),
//! picture addresses sized to the server's buckets, and the picture fetcher
//! `pley-gfx` uses.
//!
//! Every request carries the access token in `Authorization: Bearer` (the
//! only place Pley takes an app's token) when it is for this server, and is
//! cancelled with the task that makes it.

use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pley_base::{Fetch, FetchError, ImageRequest};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::addresses::{Url, origin};
use crate::http::{
    ApiError, Authorized, Http, Method, NetError, Request, Response, TokenSource, encode_component, with_query,
};
use crate::model::*;

/// The widths the server resizes pictures to: a width asked for is rounded
/// up to one of these (`apps/server/src/images.rs`).
pub const IMAGE_BUCKETS: [u32; 8] = [160, 240, 320, 480, 640, 960, 1280, 1920];

/// The bucket a picture drawn `width_px` device pixels wide is asked at.
pub fn image_bucket(width_px: u32) -> u32 {
    IMAGE_BUCKETS.iter().copied().find(|b| *b >= width_px).unwrap_or(1920)
}

/// How long a play request may take: the server may start a conversion.
pub const PLAY_TIMEOUT: Duration = Duration::from_secs(60);
/// How long tuning a Live TV channel may take: the server waits for the provider.
pub const WATCH_TIMEOUT: Duration = Duration::from_secs(45);
/// How long a picture may take.
pub const PICTURE_TIMEOUT: Duration = Duration::from_secs(30);
/// The largest picture taken.
pub const MAX_PICTURE: u64 = 32 << 20;
/// How long a speed test reads from its first byte (docs/QUALITY.md).
pub const SPEED_TEST_WINDOW: Duration = Duration::from_secs(2);
/// The speed test's size: 32 MiB, more than 2 s of a fast network.
pub const SPEED_TEST_BYTES: u64 = 33_554_432;

/// A page of a Live TV category's channels to ask for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveBrowse {
    pub category: LiveCategory,
    /// Matches the name, the number's start or what's on now.
    pub q: Option<String>,
    pub offset: i64,
    /// 100 by default; 500 at most.
    pub limit: i64,
    /// The page holding this channel instead of `offset`.
    pub around: Option<i64>,
}

/// The API of one server as one sign-in. Cheap to clone.
#[derive(Clone)]
pub struct Api {
    auth: Authorized,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api").field("server", &self.server()).finish()
    }
}

fn id_path(fmt: &str, id: i64) -> String {
    fmt.replace("{id}", &id.to_string())
}

impl Api {
    /// The API of `server` (its origin) with `tokens`' sign-in.
    pub fn new(http: Http, server: &str, tokens: Arc<dyn TokenSource>) -> Api {
        Api { auth: Authorized::new(http, server, tokens) }
    }

    /// The server's origin.
    pub fn server(&self) -> &str {
        self.auth.origin()
    }

    /// The client requests go through.
    pub fn http(&self) -> &Http {
        self.auth.http()
    }

    /// Where the access token comes from.
    pub fn tokens(&self) -> &Arc<dyn TokenSource> {
        self.auth.tokens()
    }

    /// An absolute address on this server for `path` (`/api/…`); absolute addresses stay as they are.
    pub fn url(&self, path: &str) -> String {
        if Url::parse(path).is_some() { path.to_string() } else { format!("{}{}", self.server(), path) }
    }

    /// A raw request with the sign-in; anything but 2xx is an error.
    pub fn send(&self, req: Request) -> Result<Response, ApiError> {
        self.auth.send(req)
    }

    /// A raw request with the sign-in, its answer read as it arrives.
    pub fn open(&self, req: Request) -> Result<crate::http::Streamed, ApiError> {
        self.auth.open(req)
    }

    fn request(&self, method: Method, path: &str) -> Request {
        Request::new(method, self.url(path)).header("Accept", "application/json")
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.send(self.request(Method::Get, path))?.decode()
    }

    fn get_query<T: DeserializeOwned>(&self, path: &str, query: &[(&str, Option<String>)]) -> Result<T, ApiError> {
        self.get(&with_query(path, query))
    }

    /// `method` with a JSON body; the answer read as `T`.
    fn json<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.send(self.request(method, path).json(body))?.decode()
    }

    /// `method` without a body, whose answer says nothing (204).
    fn act(&self, method: Method, path: &str) -> Result<(), ApiError> {
        self.send(self.request(method, path)).map(drop)
    }

    // Account.

    /// Who the app acts as: `GET /api/auth/me`.
    pub fn me(&self) -> Result<Me, ApiError> {
        self.get("/api/auth/me")
    }

    /// The household's profiles (scope `profiles`); empty when the profile isn't a home member.
    pub fn profiles(&self) -> Result<Vec<AppProfile>, ApiError> {
        self.get("/api/oauth/profiles")
    }

    /// The profile's audio and subtitle defaults (scope `play`); the profile as it is now.
    pub fn set_track_defaults(&self, defaults: &TrackDefaults) -> Result<Me, ApiError> {
        self.json(Method::Put, "/api/me/track-defaults", defaults)
    }

    // Browsing.

    /// Home's rows.
    pub fn home(&self) -> Result<Hubs, ApiError> {
        self.get("/api/hubs/home")
    }

    /// The libraries, in the profile's order.
    pub fn libraries(&self) -> Result<Vec<Library>, ApiError> {
        self.get("/api/libraries")
    }

    /// One library.
    pub fn library(&self, id: i64) -> Result<Library, ApiError> {
        self.get(&id_path("/api/libraries/{id}", id))
    }

    /// The profile's own order of its libraries (each once); them in it.
    pub fn sort_libraries(&self, ids: &[i64]) -> Result<Vec<Library>, ApiError> {
        self.json(Method::Put, "/api/me/libraries/order", &LibraryIds { ids: ids.to_vec() })
    }

    /// Back to the order everyone sees; the libraries in it.
    pub fn forget_library_order(&self) -> Result<Vec<Library>, ApiError> {
        self.send(self.request(Method::Delete, "/api/me/libraries/order"))?.decode()
    }

    /// A page of a library's listing.
    pub fn library_items(&self, id: i64, query: &LibraryQuery, offset: i64, limit: i64) -> Result<ItemsPage, ApiError> {
        self.get_query(&id_path("/api/libraries/{id}/items", id), &query.params(offset, Some(limit)))
    }

    /// A library's front page rows.
    pub fn library_hubs(&self, id: i64) -> Result<Hubs, ApiError> {
        self.get(&id_path("/api/libraries/{id}/hubs", id))
    }

    /// The genres and years a library can be filtered by.
    pub fn library_filters(&self, id: i64) -> Result<LibraryFilters, ApiError> {
        self.get(&id_path("/api/libraries/{id}/filters", id))
    }

    /// A library's collections, by title.
    pub fn library_collections(&self, id: i64) -> Result<CollectionList, ApiError> {
        self.get(&id_path("/api/libraries/{id}/collections", id))
    }

    /// A collection and its items, in its order.
    pub fn collection(&self, id: i64) -> Result<CollectionDetail, ApiError> {
        self.get(&id_path("/api/collections/{id}", id))
    }

    /// An item with everything the server knows. For a movie or episode it
    /// also has the server prepare it (seek previews, keyframes, read-ahead).
    pub fn item(&self, id: i64) -> Result<ItemDetail, ApiError> {
        self.get(&id_path("/api/items/{id}", id))
    }

    /// Seasons, episodes, albums or tracks.
    pub fn children(&self, id: i64) -> Result<ItemList, ApiError> {
        self.get(&id_path("/api/items/{id}/children", id))
    }

    /// The episode a show's or season's Play starts.
    pub fn on_deck(&self, id: i64) -> Result<Option<ItemSummary>, ApiError> {
        self.get::<MaybeItem>(&id_path("/api/items/{id}/on-deck", id)).map(|m| m.item)
    }

    /// Up Next: the episode after this one, or the track after it in its album.
    pub fn next(&self, id: i64) -> Result<Option<ItemSummary>, ApiError> {
        self.get::<MaybeItem>(&id_path("/api/items/{id}/next", id)).map(|m| m.item)
    }

    /// "More Like This" (`limit` 1 to 60).
    pub fn related(&self, id: i64, limit: u32) -> Result<ItemList, ApiError> {
        self.get_query(&id_path("/api/items/{id}/related", id), &[("limit", Some(limit.to_string()))])
    }

    /// The collections an item is part of.
    pub fn item_collections(&self, id: i64) -> Result<ItemCollections, ApiError> {
        self.get(&id_path("/api/items/{id}/collections", id))
    }

    /// Has the server read ahead what Play reads first (focus resting on a title).
    pub fn prefetch(&self, id: i64) -> Result<(), ApiError> {
        self.act(Method::Post, &id_path("/api/items/{id}/prefetch", id))
    }

    /// A person and what they are credited in, newest first.
    pub fn person(&self, id: i64) -> Result<PersonDetail, ApiError> {
        self.get(&id_path("/api/people/{id}", id))
    }

    /// Items, people and (where Live TV is open to the profile) channels.
    pub fn search(&self, q: &str, limit: u32) -> Result<SearchResults, ApiError> {
        self.get_query("/api/search", &[("q", Some(q.to_string())), ("limit", Some(limit.to_string()))])
    }

    // Requests and Discover (docs/API.md, "Requests").

    /// What requests are for this sign-in: on or off, what asking does, the limits and the counts.
    pub fn request_summary(&self) -> Result<RequestSummary, ApiError> {
        self.get("/api/requests/summary")
    }

    /// One page of a Discover list.
    pub fn discover(&self, list: DiscoverList, page: i64) -> Result<DiscoverPage, ApiError> {
        self.get_query(&format!("/api/discover/{}", list.as_str()), &[("page", Some(page.to_string()))])
    }

    /// TMDB's movies and shows for `q`; `missing`: without those a library has whole.
    pub fn discover_search(&self, q: &str, page: i64, missing: bool) -> Result<DiscoverPage, ApiError> {
        self.get_query(
            "/api/discover/search",
            &[
                ("q", Some(q.to_string())),
                ("page", Some(page.to_string())),
                ("missing", missing.then(|| "1".to_string())),
            ],
        )
    }

    /// A movie's or show's page; TMDB numbers the two separately, so `kind` is the one it came with.
    pub fn discover_title(&self, kind: TitleKind, tmdb_id: i64) -> Result<DiscoverDetail, ApiError> {
        self.get(&format!("/api/discover/{}/{tmdb_id}", kind.as_str()))
    }

    /// A library title's place in requests: its status, the request for it, the seasons left to ask for.
    pub fn item_request(&self, id: i64) -> Result<ItemRequest, ApiError> {
        self.get(&id_path("/api/items/{id}/request", id))
    }

    /// Requests, as `query` picks them.
    pub fn requests(&self, query: &RequestQuery) -> Result<RequestList, ApiError> {
        self.get_query("/api/requests", &query.params())
    }

    /// One request.
    pub fn media_request(&self, id: i64) -> Result<MediaRequest, ApiError> {
        self.get(&id_path("/api/requests/{id}", id))
    }

    /// Asks for a title (or follows the request someone made for it); the request, and whether
    /// it is new (`201`). The server may take a while to reach TMDB.
    pub fn ask_for(&self, body: &NewRequest) -> Result<(MediaRequest, bool), ApiError> {
        let req = self.request(Method::Post, "/api/requests").json(body).timeout(Duration::from_secs(30));
        let answer = self.send(req)?;
        let created = answer.status == 201;
        Ok((answer.decode()?, created))
    }

    /// Withdraws this profile's own request (or removes a declined one); a manager's deletes anyone's.
    pub fn delete_request(&self, id: i64) -> Result<(), ApiError> {
        self.act(Method::Delete, &id_path("/api/requests/{id}", id))
    }

    /// Stops following someone else's request.
    pub fn unwatch_request(&self, id: i64) -> Result<(), ApiError> {
        self.act(Method::Delete, &id_path("/api/requests/{id}/watch", id))
    }

    /// Your requests was seen: its news is no longer new, on this profile's other devices too.
    pub fn mark_requests_seen(&self) -> Result<(), ApiError> {
        self.act(Method::Post, "/api/requests/seen")
    }

    /// Managers: approves (or tries again) with the settings' choices; the request as it is now.
    pub fn approve_request(&self, id: i64) -> Result<MediaRequest, ApiError> {
        self.send(self.request(Method::Post, &id_path("/api/requests/{id}/approve", id)))?.decode()
    }

    /// Managers: declines, with what its maker reads as the reason, if any.
    pub fn decline_request(&self, id: i64, reason: Option<&str>) -> Result<MediaRequest, ApiError> {
        let body = DeclineBody { reason: reason.map(str::trim).filter(|r| !r.is_empty()).map(str::to_string) };
        self.json(Method::Post, &id_path("/api/requests/{id}/decline", id), &body)
    }

    /// Managers: approved without sending it anywhere, as added to the library by hand.
    pub fn mark_request_handled(&self, id: i64) -> Result<MediaRequest, ApiError> {
        self.send(self.request(Method::Post, &id_path("/api/requests/{id}/handled", id)))?.decode()
    }

    // Playlists.

    /// The profile's playlists, by title.
    pub fn playlists(&self) -> Result<PlaylistList, ApiError> {
        self.get("/api/playlists")
    }

    /// A playlist and its entries.
    pub fn playlist(&self, id: i64) -> Result<PlaylistDetail, ApiError> {
        self.get(&id_path("/api/playlists/{id}", id))
    }

    /// A new playlist (scope `playlists`).
    pub fn create_playlist(&self, playlist: &NewPlaylist) -> Result<PlaylistDetail, ApiError> {
        self.json(Method::Post, "/api/playlists", playlist)
    }

    /// Renames a playlist, or changes a smart one's filter.
    pub fn patch_playlist(&self, id: i64, patch: &PlaylistPatch) -> Result<PlaylistDetail, ApiError> {
        self.json(Method::Patch, &id_path("/api/playlists/{id}", id), patch)
    }

    /// Deletes a playlist.
    pub fn delete_playlist(&self, id: i64) -> Result<(), ApiError> {
        self.act(Method::Delete, &id_path("/api/playlists/{id}", id))
    }

    /// Adds items (a show, season, album or artist adds its episodes or tracks).
    pub fn add_to_playlist(&self, id: i64, item_ids: &[i64]) -> Result<Added, ApiError> {
        self.json(Method::Post, &id_path("/api/playlists/{id}/items", id), &ItemIds { item_ids: item_ids.to_vec() })
    }

    /// Takes one entry out (the item stays in its library).
    pub fn remove_from_playlist(&self, id: i64, entry_id: i64) -> Result<(), ApiError> {
        self.act(Method::Delete, &format!("/api/playlists/{id}/items/{entry_id}"))
    }

    /// These entries first, the rest after.
    pub fn reorder_playlist(&self, id: i64, entry_ids: &[i64]) -> Result<PlaylistDetail, ApiError> {
        self.json(Method::Put, &id_path("/api/playlists/{id}/order", id), &EntryIds { entry_ids: entry_ids.to_vec() })
    }

    // My List, history, watch state.

    /// My List, newest first.
    pub fn watchlist(&self) -> Result<ItemList, ApiError> {
        self.get("/api/watchlist")
    }

    /// Puts a movie or show on My List, or takes it off.
    pub fn set_in_my_list(&self, id: i64, on: bool) -> Result<(), ApiError> {
        self.act(if on { Method::Put } else { Method::Delete }, &id_path("/api/watchlist/{id}", id))
    }

    /// A page of the profile's plays, newest first.
    pub fn history(&self, offset: i64, limit: i64) -> Result<HistoryPage, ApiError> {
        self.get_query("/api/history", &[("offset", Some(offset.to_string())), ("limit", Some(limit.to_string()))])
    }

    /// Removes a play from the history (the profile's own only).
    pub fn remove_history(&self, id: i64) -> Result<(), ApiError> {
        self.act(Method::Delete, &id_path("/api/history/{id}", id))
    }

    /// Hides a movie, or an episode's whole show, from Continue Watching until it plays again.
    pub fn hide_from_continue_watching(&self, id: i64) -> Result<(), ApiError> {
        self.act(Method::Delete, &id_path("/api/continue-watching/{id}", id))
    }

    /// The profile's rating, 0 to 10 (two points a star); none clears it.
    pub fn rate(&self, id: i64, rating: Option<f64>) -> Result<(), ApiError> {
        self.send(self.request(Method::Put, &id_path("/api/items/{id}/rating", id)).json(&RatingRequest { rating }))
            .map(drop)
    }

    /// Marks an item (and everything under it) watched or not.
    pub fn set_watched(&self, id: i64, watched: bool) -> Result<(), ApiError> {
        let path = if watched { "/api/items/{id}/watched" } else { "/api/items/{id}/unwatched" };
        self.act(Method::Post, &id_path(path, id))
    }

    /// The tracks picked, remembered for the whole show or the movie.
    pub fn set_streams(&self, item_id: i64, choice: &StreamChoice) -> Result<(), ApiError> {
        self.send(self.request(Method::Put, &id_path("/api/items/{id}/streams", item_id)).json(choice)).map(drop)
    }

    /// Where playing stands (every ~10 s while playing, on pause, `stopped` on closing).
    /// A session stopped from elsewhere answers [`ApiError::SessionStopped`].
    pub fn progress(&self, item_id: i64, report: &ProgressReport) -> Result<(), ApiError> {
        self.send(self.request(Method::Post, &id_path("/api/items/{id}/progress", item_id)).json(report)).map(drop)
    }

    // Playback.

    /// How the item plays and from where (60 s: the server may start a conversion).
    pub fn play(&self, req: &PlaybackRequest) -> Result<PlaybackInfo, ApiError> {
        self.send(self.request(Method::Post, "/api/playback").json(req).timeout(PLAY_TIMEOUT))?.decode()
    }

    /// How a session plays now (Playback details, recovery).
    pub fn session_status(&self, session: &str, timeout: Duration) -> Result<SessionStatus, ApiError> {
        let path = format!("/api/playback/{}", encode_component(session));
        self.send(self.request(Method::Get, &path).timeout(timeout))?.decode()
    }

    /// The session's converted video goes on at `kbps` from the segments its
    /// player hasn't fetched yet; the answer is what it goes on at.
    pub fn set_bitrate(&self, session: &str, kbps: u32) -> Result<BitrateAnswer, ApiError> {
        let path = format!("/api/playback/{}/bitrate", encode_component(session));
        let req = self.request(Method::Put, &path).json(&BitrateRequest { max_bitrate: kbps });
        self.send(req.timeout(Duration::from_secs(10)))?.decode()
    }

    /// Ends a session and its conversion.
    pub fn stop_session(&self, session: &str) -> Result<(), ApiError> {
        self.act(Method::Delete, &format!("/api/playback/{}", encode_component(session)))
    }

    /// What the server knows of this TV's network, for Automatic's start.
    pub fn network(&self, client_id: &str, timeout: Duration) -> Result<NetworkAnswer, ApiError> {
        let path = with_query("/api/playback/network", &[("client_id", Some(client_id.to_string()))]);
        self.send(self.request(Method::Get, &path).timeout(timeout))?.decode()
    }

    /// Tells the server a speed test's result or Automatic's new ceiling.
    pub fn tell_network(
        &self,
        client_id: &str,
        report: &NetworkReport,
        timeout: Duration,
    ) -> Result<NetworkAnswer, ApiError> {
        let req = self.request(Method::Put, "/api/playback/network").json(&report.body(client_id)).timeout(timeout);
        self.send(req)?.decode()
    }

    /// The speed test: reads the server's noise for [`SPEED_TEST_WINDOW`] from
    /// the first byte. A `404` means an older server without one.
    pub fn speed_test(&self, bytes: u64) -> Result<SpeedSample, ApiError> {
        let path = with_query("/api/playback/speedtest", &[("bytes", Some(bytes.to_string()))]);
        let req = Request::new(Method::Get, self.url(&path)).header("Accept", "*/*").timeout(Duration::from_secs(10));
        let mut answer = self.open(req)?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut got = 0u64;
        let mut first: Option<Instant> = None;
        loop {
            if first.is_some_and(|f| f.elapsed() >= SPEED_TEST_WINDOW) {
                break;
            }
            let n = match answer.body.read(&mut buf) {
                Ok(n) => n,
                // The window's time is up when the request's is: what came is the sample.
                Err(e) if NetError::from_io(&e) == NetError::TimedOut && first.is_some() => break,
                Err(e) => return Err(ApiError::Net(NetError::from_io(&e))),
            };
            if n == 0 {
                let ms = first.map_or(0, |f| f.elapsed().as_millis() as u64);
                return Ok(SpeedSample { bytes: got, ms, complete: true });
            }
            first.get_or_insert_with(Instant::now);
            got += n as u64;
        }
        let ms = first.map_or(0, |f| f.elapsed().as_millis() as u64);
        Ok(SpeedSample { bytes: got, ms, complete: false })
    }

    /// A session's playlist (`GET`, as text) or segment (`HEAD`: answered once
    /// it is made), asked for as its player will: keyed addresses as they are
    /// (they carry their own key), others with the token.
    pub fn media(&self, method: Method, url: &str, timeout: Duration) -> Result<String, ApiError> {
        let full = self.url(url);
        let req = Request::new(method, full.clone()).header("Accept", "*/*").timeout(timeout);
        let keyed = Url::parse(&full)
            .is_some_and(|u| u.path.starts_with("/api/media/") || u.path.starts_with("/api/live/media/"));
        let answer = if keyed {
            let r = self.http().send(req).map_err(ApiError::Net)?;
            if !r.ok() {
                return Err(ApiError::from_response(&r));
            }
            r
        } else {
            self.send(req)?
        };
        Ok(answer.text())
    }

    /// Whether the server answers at all (`GET /api/server`, no token).
    pub fn reachable(&self, timeout: Duration) -> Result<ServerInfo, ApiError> {
        let req = Request::get(self.url("/api/server")).header("Accept", "application/json").timeout(timeout);
        let r = self.http().send(req).map_err(ApiError::Net)?;
        if !r.ok() {
            return Err(ApiError::from_response(&r));
        }
        r.decode()
    }

    // Subtitles and seek previews.

    /// A text track as WebVTT (`url` from [`SubtitleTrack::url`]), and whether it is whole.
    pub fn subtitle_text(&self, url: &str) -> Result<(String, bool), ApiError> {
        let req = Request::get(self.url(url)).header("Accept", "text/vtt").timeout(Duration::from_secs(120));
        let r = self.send(req)?;
        Ok((r.text(), r.complete()))
    }

    /// Subtitles found online for a file, in `languages` (two-letter codes;
    /// the profile's own when empty). 30 s.
    pub fn search_subtitles(
        &self,
        item_id: i64,
        file_id: Option<i64>,
        languages: &[String],
    ) -> Result<Vec<SubtitleResult>, ApiError> {
        let query = [
            ("file_id", file_id.map(|f| f.to_string())),
            ("language", (!languages.is_empty()).then(|| languages.join(","))),
        ];
        let path = with_query(&id_path("/api/items/{id}/subtitles/search", item_id), &query);
        let r = self.send(self.request(Method::Get, &path).timeout(Duration::from_secs(30)))?;
        Ok(r.decode::<SubtitleResults>()?.results)
    }

    /// Adds a subtitle the search found, for everyone; the new track. 30 s.
    pub fn add_subtitle(&self, item_id: i64, add: &AddSubtitle) -> Result<SubtitleTrack, ApiError> {
        let req = self.request(Method::Post, &id_path("/api/items/{id}/subtitles", item_id)).json(add);
        self.send(req.timeout(Duration::from_secs(30)))?.decode()
    }

    /// A file's seek previews; `404` while they are being made.
    pub fn trickplay(&self, file_id: i64) -> Result<TrickplayInfo, ApiError> {
        self.get(&id_path("/api/files/{id}/trickplay", file_id))
    }

    // Live TV.

    /// Whether to show Live TV, and the channels watched last.
    pub fn live_summary(&self) -> Result<LiveSummary, ApiError> {
        self.get("/api/live")
    }

    /// The groups, and how many channels in all, marked and watched lately.
    pub fn live_groups(&self) -> Result<LiveGroups, ApiError> {
        self.get("/api/live/groups")
    }

    /// A page of a category's channels, each with what's on now and next.
    pub fn live_browse(&self, b: &LiveBrowse) -> Result<LiveChannelPage, ApiError> {
        let limit = if b.limit > 0 { b.limit.min(500) } else { 100 };
        let query = [
            ("category", Some(b.category.wire().to_string())),
            ("group", b.category.group().map(str::to_string)),
            ("q", b.q.clone().filter(|q| !q.trim().is_empty())),
            ("offset", Some(b.offset.max(0).to_string())),
            ("limit", Some(limit.to_string())),
            ("around", b.around.map(|a| a.to_string())),
        ];
        self.get_query("/api/live/browse", &query)
    }

    /// Every shown channel by number (fine for a few hundred).
    pub fn live_channels(&self) -> Result<Vec<LiveChannel>, ApiError> {
        self.get("/api/live/channels")
    }

    /// One channel with what's on now and next.
    pub fn live_channel(&self, id: i64) -> Result<LiveChannel, ApiError> {
        self.get(&id_path("/api/live/channels/{id}", id))
    }

    /// The guide of channels `ids` between `from` and `to` (unix seconds, two days at most).
    pub fn live_guide(&self, from: i64, to: i64, ids: &[i64]) -> Result<LiveGuide, ApiError> {
        let ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let query = [("from", Some(from.to_string())), ("to", Some(to.to_string())), ("ids", Some(ids))];
        self.get_query("/api/live/guide", &query)
    }

    /// Marks a channel a favourite, or not.
    pub fn set_live_favorite(&self, channel: i64, favorite: bool) -> Result<(), ApiError> {
        self.act(
            if favorite { Method::Put } else { Method::Delete },
            &id_path("/api/live/channels/{id}/favorite", channel),
        )
    }

    /// Starts watching a channel (45 s: the server tunes it). Every connection
    /// in use answers `409`: [`ApiError::busy`] lists them, and asking again
    /// with `take_over` takes one.
    pub fn watch_channel(&self, channel: i64, req: &WatchRequest) -> Result<LiveWatching, ApiError> {
        let r = self.request(Method::Post, &id_path("/api/live/channels/{id}/watch", channel)).json(req);
        self.send(r.timeout(WATCH_TIMEOUT))?.decode()
    }

    /// What a session's channel carries and what is made of it; none while
    /// unknown. A session that ended answers [`ApiError::SessionStopped`] with why.
    pub fn live_plan(&self, session: &str) -> Result<Option<LivePlan>, ApiError> {
        let path = format!("/api/live/sessions/{}/plan", encode_component(session));
        self.send(self.request(Method::Get, &path).timeout(Duration::from_secs(8)))?.decode()
    }

    /// Why a watching session ended (its connection taken over, say), or none
    /// while it goes on.
    pub fn why_live_ended(&self, session: &str) -> Result<Option<String>, ApiError> {
        match self.live_plan(session) {
            Ok(_) => Ok(None),
            Err(e @ ApiError::SessionStopped { .. }) => Ok(Some(e.to_string())),
            Err(e) if e.is(410) => Ok(Some(e.to_string())),
            Err(e) => Err(e),
        }
    }

    /// Stops watching: the connection is let go once nobody else watches. A
    /// session already gone is fine.
    pub fn leave_channel(&self, session: &str) -> Result<(), ApiError> {
        match self.act(Method::Delete, &format!("/api/live/sessions/{}", encode_component(session))) {
            Err(e) if e.is(404) => Ok(()),
            other => other,
        }
    }

    // Pictures.

    /// The absolute address of a picture the server gave (`/api/items/12/images/poster?v=…`)
    /// at the bucket for `width_px`: any `w` replaced, the rest kept.
    pub fn image_url(&self, path: &str, width_px: u32) -> String {
        let full = self.url(path);
        let (base, query) = full.split_once('?').unwrap_or((full.as_str(), ""));
        let mut params: Vec<&str> = query.split('&').filter(|p| !p.is_empty() && !p.starts_with("w=")).collect();
        let w = format!("w={}", image_bucket(width_px.max(1)));
        params.push(&w);
        format!("{base}?{}", params.join("&"))
    }

    /// What `pley-gfx` is asked for to draw a picture `width_px` device pixels wide.
    pub fn image(&self, path: &str, width_px: u32) -> ImageRequest {
        ImageRequest::new(self.image_url(path, width_px), width_px)
    }

    /// An item's picture by kind (a parent's or grandparent's art), with its
    /// version when known.
    pub fn item_image(&self, item_id: i64, kind: ImageKind, version: Option<&str>, width_px: u32) -> String {
        let mut path = format!("/api/items/{item_id}/images/{}", kind.as_str());
        if let Some(v) = version {
            path = with_query(&path, &[("v", Some(v.to_string()))]);
        }
        self.image_url(&path, width_px)
    }

    /// A sheet of a file's seek previews (`v` is the previews' id; JPEG, never resized).
    pub fn trickplay_sheet(&self, file_id: i64, info: &TrickplayInfo, sheet: i64) -> String {
        self.url(&with_query(&format!("/api/files/{file_id}/trickplay/{sheet}.jpg"), &[("v", Some(info.id.clone()))]))
    }

    /// The picture fetcher `pley-gfx` uses, signed in as this API.
    pub fn fetcher(&self) -> Arc<dyn Fetch> {
        Arc::new(self.clone())
    }
}

impl Fetch for Api {
    /// A picture's bytes: the token only for this server's own origin; a
    /// collection's redirect to an item's picture followed (three hops at most).
    fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        let mut url = self.url(url);
        for _ in 0..4 {
            let req = Request::get(url.clone()).header("Accept", "image/*").timeout(PICTURE_TIMEOUT);
            let answer = match self.auth.exchange(req) {
                Ok(a) => a,
                Err(e) if e.unreachable() => return Err(FetchError::Unreachable),
                Err(e) => return Err(FetchError::Failed(e.to_string())),
            };
            match answer.status {
                200..=299 => {
                    let whole = answer.read_all(MAX_PICTURE).map_err(|e| match e {
                        NetError::TooBig => FetchError::Failed("the picture is too big".into()),
                        e => FetchError::Failed(e.reason().into()),
                    })?;
                    return Ok(whole.body);
                }
                301 | 302 | 303 | 307 | 308 => {
                    let Some(next) = answer.header("Location").map(str::to_string) else {
                        return Err(FetchError::Failed("a redirect without an address".into()));
                    };
                    url = if Url::parse(&next).is_some() {
                        next
                    } else {
                        let here = origin(&url).unwrap_or_else(|| self.server().to_string());
                        format!("{here}{}{next}", if next.starts_with('/') { "" } else { "/" })
                    };
                }
                404 | 410 => return Err(FetchError::Missing),
                502..=504 => return Err(FetchError::Unreachable),
                status => return Err(FetchError::Failed(format!("the server refused (HTTP {status})"))),
            }
        }
        Err(FetchError::Failed("too many redirects".into()))
    }
}

#[cfg(test)]
mod tests;
