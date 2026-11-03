use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::http::FakeTransport;

const SERVER: &str = "http://192.168.1.20:7539";

struct Fixed;

impl TokenSource for Fixed {
    fn access_token(&self) -> Result<String, ApiError> {
        Ok("pley_at_x".into())
    }

    fn refresh(&self, _stale: &str) -> Result<String, ApiError> {
        Ok("pley_at_y".into())
    }

    fn refused_after_refresh(&self, _sent: &str) -> bool {
        false
    }
}

fn api(answer: impl Fn(&Request) -> Result<Response, NetError> + Send + Sync + 'static) -> (Api, Arc<FakeTransport>) {
    let fake = Arc::new(FakeTransport::new(answer));
    (Api::new(Http::new(fake.clone(), "pleyLG/test"), SERVER, Arc::new(Fixed)), fake)
}

fn token(r: &Request) -> Option<&str> {
    r.header_value("Authorization")
}

#[test]
fn widths_snap_up_to_the_servers_buckets() {
    assert_eq!(image_bucket(1), 160);
    assert_eq!(image_bucket(160), 160);
    assert_eq!(image_bucket(161), 240);
    assert_eq!(image_bucket(500), 640);
    assert_eq!(image_bucket(4000), 1920);
}

#[test]
fn picture_addresses_keep_the_version_and_replace_the_width() {
    let (api, _) = api(|_| Ok(Response::empty(200)));
    assert_eq!(
        api.image_url("/api/items/12/images/poster?v=1761100000", 216),
        "http://192.168.1.20:7539/api/items/12/images/poster?v=1761100000&w=240"
    );
    assert_eq!(
        api.image_url("/api/items/12/images/poster?w=160&v=9", 600),
        "http://192.168.1.20:7539/api/items/12/images/poster?v=9&w=640"
    );
    assert_eq!(
        api.item_image(7, ImageKind::Backdrop, Some("5"), 1920),
        "http://192.168.1.20:7539/api/items/7/images/backdrop?v=5&w=1920"
    );
    let info = TrickplayInfo { id: "abc".into(), ..TrickplayInfo::default() };
    assert_eq!(api.trickplay_sheet(3, &info, 2), "http://192.168.1.20:7539/api/files/3/trickplay/2.jpg?v=abc");
    let req = api.image("https://cdn.example.com/logo.png", 96);
    assert_eq!(&*req.url, "https://cdn.example.com/logo.png?w=160");
    assert_eq!(req.width_px, 96);
}

#[test]
fn pictures_carry_the_token_only_to_the_server_and_follow_its_redirects() {
    let (api, fake) = api(|r| {
        let path = crate::addresses::Url::parse(&r.url).unwrap().path;
        Ok(match path.as_str() {
            "/api/collections/4/images/poster?w=320" => Response {
                status: 307,
                headers: vec![("Location".into(), "/api/items/9/images/poster?w=320".into())],
                body: vec![],
            },
            "/api/items/9/images/poster?w=320" | "/logo.png" => {
                Response { status: 200, headers: vec![], body: b"\xff\xd8jpeg".to_vec() }
            }
            _ => Response::json(404, &json!({"code": "not_found", "message": "no artwork"})),
        })
    });
    assert_eq!(api.fetch("/api/collections/4/images/poster?w=320").unwrap(), b"\xff\xd8jpeg");
    assert_eq!(api.fetch("https://cdn.example.com/logo.png").unwrap(), b"\xff\xd8jpeg");
    assert_eq!(api.fetch("/api/items/1/images/thumb?w=320"), Err(FetchError::Missing));
    let tokens: Vec<Option<String>> = fake.requests().iter().map(|r| token(r).map(str::to_string)).collect();
    assert_eq!(
        tokens,
        [Some("Bearer pley_at_x".into()), Some("Bearer pley_at_x".into()), None, Some("Bearer pley_at_x".into())]
    );
}

#[test]
fn a_librarys_listing_asks_in_the_tv_apps_order() {
    let (api, fake) = api(|_| Ok(Response::json(200, &json!({"total": 0, "items": []}))));
    let query = LibraryQuery {
        sort: LibrarySort::Added,
        order: Some(SortOrder::Desc),
        watched: Some(false),
        genre: Some("Science Fiction".into()),
        ..LibraryQuery::default()
    };
    api.library_items(3, &query, 200, 1000).unwrap();
    // Resolution and dynamic range after the year, and only when chosen.
    let query =
        LibraryQuery { resolution: Some(ResolutionFilter::Uhd), dynamic_range: Some(RangeFilter::Hdr), ..query };
    api.library_items(3, &query, 0, 200).unwrap();
    assert_eq!(
        fake.asked(),
        [
            "GET /api/libraries/3/items?sort=added&order=desc&watched=false&genre=Science%20Fiction&offset=200&limit=1000",
            "GET /api/libraries/3/items?sort=added&order=desc&watched=false&genre=Science%20Fiction&resolution=4k&dynamic_range=hdr&offset=0&limit=200",
        ]
    );
    assert_eq!(query.key(), "added.desc.unwatched.g=Science Fiction.r=4k.d=hdr");
}

#[test]
fn views_kept_before_the_quality_filters_still_read() {
    let old: LibraryQuery = serde_json::from_str(r#"{"sort":"year","order":"desc","genre":"Drama"}"#).unwrap();
    assert_eq!((old.genre.as_deref(), old.resolution, old.dynamic_range), (Some("Drama"), None, None));
    // A token this app doesn't know reads as Unknown, for the library to drop.
    let odd: LibraryQuery = serde_json::from_str(r#"{"resolution":"8k","dynamic_range":"hdr"}"#).unwrap();
    assert_eq!((odd.resolution, odd.dynamic_range), (Some(ResolutionFilter::Unknown), Some(RangeFilter::Hdr)));
    let kept = LibraryQuery { resolution: Some(ResolutionFilter::Sd), ..LibraryQuery::default() };
    let back: LibraryQuery = serde_json::from_str(&serde_json::to_string(&kept).unwrap()).unwrap();
    assert_eq!(back, kept);
    // An older server's filters have neither list.
    let filters: LibraryFilters = serde_json::from_str(r#"{"genres":["Drama"],"years":[1999]}"#).unwrap();
    assert!(filters.resolutions.is_empty() && filters.dynamic_ranges.is_empty());
}

#[test]
fn live_tv_pages_name_their_category() {
    let (api, fake) = api(|_| Ok(Response::json(200, &json!({"total": 0, "offset": 0, "channels": []}))));
    api.live_browse(&LiveBrowse { category: LiveCategory::Group("UK| Sport".into()), limit: 50, ..Default::default() })
        .unwrap();
    api.live_browse(&LiveBrowse { category: LiveCategory::Favorites, around: Some(12), ..Default::default() }).unwrap();
    assert_eq!(
        fake.asked(),
        [
            "GET /api/live/browse?category=group&group=UK%7C%20Sport&offset=0&limit=50",
            "GET /api/live/browse?category=favorites&offset=0&limit=100&around=12",
        ]
    );
}

#[test]
fn a_busy_source_and_an_ended_session_are_read_from_their_bodies() {
    let busy = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/live/busy.json")).unwrap();
    let (api, fake) = api(move |r| {
        let path = crate::addresses::Url::parse(&r.url).unwrap().path;
        Ok(if path.ends_with("/watch") {
            Response { status: 409, headers: vec![], body: busy.clone() }
        } else {
            Response::json(
                410,
                &json!({"code": "session_stopped", "error": "session_stopped", "message": "Watching moved to BBC One on another device."}),
            )
        })
    });
    let e = api
        .watch_channel(2, &WatchRequest { client_id: Some("webos-1".into()), keyed_urls: true, take_over: None })
        .unwrap_err();
    assert!(e.busy().unwrap().connections[0].can_take_over);
    let sent: serde_json::Value = serde_json::from_slice(fake.requests()[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(sent, json!({"client_id": "webos-1", "keyed_urls": true}));
    assert_eq!(api.why_live_ended("abc").unwrap().as_deref(), Some("Watching moved to BBC One on another device."));
}

#[test]
fn keyed_media_addresses_go_without_the_token() {
    let (api, fake) = api(|_| Ok(Response { status: 200, headers: vec![], body: b"#EXTM3U\n".to_vec() }));
    assert_eq!(
        api.media(Method::Get, "/api/media/pley_mk_abc/master.m3u8", Duration::from_secs(5)).unwrap(),
        "#EXTM3U\n"
    );
    api.media(Method::Head, "/api/playback/s1/4.ts", Duration::from_secs(5)).unwrap();
    let tokens: Vec<bool> = fake.requests().iter().map(|r| token(r).is_some()).collect();
    assert_eq!(tokens, [false, true]);
}

#[test]
fn progress_and_track_choices_send_what_the_server_reads() {
    let (api, fake) = api(|_| Ok(Response::empty(204)));
    let report = ProgressReport {
        position_ms: 754_000,
        duration_ms: Some(5_400_000),
        state: ProgressState::Paused,
        session_id: Some("9c1e".into()),
        age_ms: None,
    };
    api.progress(42, &report).unwrap();
    api.set_streams(42, &StreamChoice { file_id: Some(7), audio: AudioChoice::Keep, subtitle_index: None }).unwrap();
    api.set_streams(42, &StreamChoice { file_id: None, audio: AudioChoice::Automatic, subtitle_index: Some(3) })
        .unwrap();
    api.rate(42, None).unwrap();
    let bodies: Vec<serde_json::Value> =
        fake.requests().iter().map(|r| serde_json::from_slice(r.body.as_ref().unwrap()).unwrap()).collect();
    assert_eq!(
        bodies,
        [
            json!({"position_ms": 754000, "duration_ms": 5400000, "state": "paused", "session_id": "9c1e"}),
            json!({"file_id": 7}),
            json!({"audio_index": null, "subtitle_index": 3}),
            json!({}),
        ]
    );
    assert_eq!(fake.asked()[0], "POST /api/items/42/progress");
}

#[test]
fn a_stopped_session_answers_its_note() {
    let (api, _) = api(|_| {
        Ok(Response::json(410, &json!({"code": "session_stopped", "error": "session_stopped", "message": null})))
    });
    let e = api.progress(1, &ProgressReport::default()).unwrap_err();
    assert_eq!(e, ApiError::SessionStopped { message: None });
}

#[test]
fn the_speed_test_measures_what_arrives() {
    let (api, fake) = api(|_| Ok(Response { status: 200, headers: vec![], body: vec![7u8; 512 * 1024] }));
    let sample = api.speed_test(512 * 1024).unwrap();
    assert_eq!(sample.bytes, 512 * 1024);
    assert!(sample.complete);
    assert!(sample.kbps().is_some());
    assert_eq!(fake.asked(), ["GET /api/playback/speedtest?bytes=524288"]);
}

#[test]
fn up_next_and_on_deck_may_be_none() {
    let (api, _) = api(|_| Ok(Response::json(200, &json!({"item": null}))));
    assert_eq!(api.next(5).unwrap(), None);
    assert_eq!(api.on_deck(5).unwrap(), None);
}

#[test]
fn a_channel_s_stream_ts_is_read_with_the_token_under_its_own_client_id() {
    let (api, fake) = api(|r| {
        assert!(r.url.contains("/stream.ts"));
        Ok(Response { status: 200, headers: vec![("Content-Type".into(), "video/mp2t".into())], body: vec![0x47; 376] })
    });
    let mut answer = api.live_stream(7, "webos-1-stream", Cancel::new()).unwrap();
    let mut body = Vec::new();
    answer.body.read_to_end(&mut body).unwrap();
    assert_eq!(body.len(), 376);
    let req = &fake.requests()[0];
    assert_eq!(req.url, "http://192.168.1.20:7539/api/live/channels/7/stream.ts?client_id=webos-1-stream");
    assert_eq!(token(req), Some("Bearer pley_at_x"));
    assert_eq!(req.header_value("Accept-Encoding"), Some("identity"));
    // Read for as long as it runs: no limit on the whole, one on each wait.
    assert_eq!((req.timeout, req.read_timeout), (None, Some(LIVE_STREAM_READ_TIMEOUT)));
}

#[test]
fn a_refused_stream_ts_is_the_server_s_sentence() {
    let (api, _) =
        api(|_| Ok(Response::json(409, &json!({"code": "conflict", "message": "every connection is in use"}))));
    let e = api.live_stream(7, "webos-1-stream", Cancel::new()).unwrap_err();
    assert!(e.is(409));
    assert_eq!(e.to_string(), "every connection is in use");
}
