//! Every JSON fixture of the Android app's model tests
//! (apps/android/core/testing/src/main/resources/fixtures, copied here)
//! decodes into the model; request bodies write back what they read.

use std::path::Path;

use pley_core::model::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

fn read(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn decode<T: DeserializeOwned>(name: &str) -> T {
    serde_json::from_slice(&read(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Decodes `name` as `T` and writes it back: the same JSON as the fixture.
fn round_trip<T: DeserializeOwned + Serialize>(name: &str) -> T {
    let value: T = decode(name);
    let original: serde_json::Value = serde_json::from_slice(&read(name)).unwrap();
    assert_eq!(serde_json::to_value(&value).unwrap(), original, "{name} doesn't write back as it was");
    value
}

#[test]
fn every_fixture_has_a_test() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut count = 0;
    for dir in std::fs::read_dir(&root).unwrap() {
        count += std::fs::read_dir(dir.unwrap().path()).unwrap().count();
    }
    // The fixtures named in the tests below; a new one needs a line here.
    assert_eq!(count, 74);
}

#[test]
fn server_and_account() {
    let info: ServerInfo = decode("server/info.json");
    assert_eq!((info.name.as_str(), info.version.as_deref()), ("Living Room", Some("0.42.0")));
    assert_eq!(decode::<Health>("server/health.json").status, "ok");
    let me: Me = decode("me/app_token.json");
    assert!(me.id > 0 && !me.is_admin);
    assert!(me.allows("library"));
    let managed: Me = decode("me/managed_profile.json");
    assert!(managed.managed);
    let defaults: TrackDefaults = round_trip("me/track_defaults.json");
    assert_eq!(defaults.subtitle_mode, SubtitleMode::Always);
    let household: Vec<AppProfile> = decode("profiles/household.json");
    let needs: Vec<ProfileNeeds> = household.iter().map(|p| p.needs).collect();
    assert_eq!(
        needs,
        [ProfileNeeds::None, ProfileNeeds::Pin, ProfileNeeds::None, ProfileNeeds::Password, ProfileNeeds::Locked]
    );
    assert!(household[0].current);
}

#[test]
fn libraries_and_browsing() {
    let list: Vec<Library> = decode("libraries/list.json");
    assert!(!list.is_empty());
    assert!(decode::<Library>("libraries/library.json").id > 0);
    let page: ItemsPage = decode("libraries/items.json");
    assert!(page.total >= page.items.len() as i64);
    let by_bitrate: ItemsPage = decode("libraries/items_by_bitrate.json");
    assert!(by_bitrate.items.iter().any(|i| i.media.is_some()));
    let filters: LibraryFilters = decode("libraries/filters.json");
    assert!(!filters.genres.is_empty());
    assert_eq!(filters.resolutions, [ResolutionFilter::Uhd, ResolutionFilter::Fhd, ResolutionFilter::Sd]);
    assert_eq!(filters.dynamic_ranges, [RangeFilter::Hdr10, RangeFilter::DolbyVision, RangeFilter::Sdr]);
    let hubs: Hubs = decode("libraries/hubs.json");
    assert!(!hubs.hubs.is_empty());
    let home: Hubs = decode("hubs/home.json");
    assert!(home.hubs.iter().any(Hub::continue_watching));
    let collections: CollectionList = decode("libraries/collections.json");
    assert!(!collections.collections.is_empty());
    let detail: CollectionDetail = decode("collections/detail.json");
    assert!(!detail.items.is_empty());
    let smart: CollectionDetail = decode("collections/smart_detail.json");
    assert!(smart.smart_filter.is_some());
    let results: SearchResults = decode("search/results.json");
    assert!(!results.items.is_empty());
    let watchlist: ItemList = decode("watchlist/list.json");
    assert!(watchlist.items.iter().all(|i| i.kind.listable()));
    let history: HistoryPage = decode("history/page.json");
    assert!(!history.entries.is_empty());
    let person: PersonDetail = decode("people/detail.json");
    assert!(!person.credits.is_empty());
}

#[test]
fn items() {
    let movie: ItemDetail = decode("items/movie_detail.json");
    assert_eq!(movie.kind, ItemKind::Movie);
    let file = &movie.files[0];
    assert!(file.streams_of(StreamKind::Video).next().is_some());
    for name in
        ["items/episode_detail.json", "items/show_detail.json", "items/track_detail.json", "items/minimal_detail.json"]
    {
        let _: ItemDetail = decode(name);
    }
    let extra: ItemDetail = decode("items/extra.json");
    assert_eq!((extra.kind, extra.extra_type), (ItemKind::Extra, Some(ExtraType::Trailer)));
    // Nulls read as defaults; names this app doesn't know read as their fallback.
    let nulls: ItemDetail = decode("items/explicit_nulls.json");
    assert_eq!((nulls.sort_title.as_str(), nulls.added_at, nulls.user_state.position_ms), ("", 0, 0));
    let unknown: ItemDetail = decode("items/unknown_fields.json");
    assert_eq!(unknown.kind, ItemKind::Unknown);
    assert_eq!(unknown.extra_type, Some(ExtraType::Unknown));
    assert_eq!(unknown.media.as_ref().and_then(|m| m.dynamic_range), Some(DynamicRange::Unknown));
    let seasons: ItemList = decode("items/children_seasons.json");
    assert!(seasons.items.iter().all(|i| i.kind == ItemKind::Season));
    let episodes: ItemList = decode("items/children_episodes.json");
    assert!(episodes.items.iter().all(|i| i.kind == ItemKind::Episode));
    assert!(decode::<MaybeItem>("items/on_deck.json").item.is_some());
    assert!(decode::<MaybeItem>("items/next_none.json").item.is_none());
    let related: ItemList = decode("items/related.json");
    assert!(!related.items.is_empty());
    let part_of: ItemCollections = decode("items/collections.json");
    assert!(!part_of.collections.is_empty());
}

#[test]
fn playlists() {
    let list: PlaylistList = decode("playlists/list.json");
    assert!(!list.playlists.is_empty());
    let detail: PlaylistDetail = decode("playlists/detail.json");
    assert!(!detail.smart() && !detail.entries.is_empty());
    let smart: PlaylistDetail = decode("playlists/smart_detail.json");
    assert!(smart.smart());
    assert_eq!(decode::<Added>("playlists/added.json").added, 9);
    round_trip::<ItemIds>("playlists/add_items_request.json");
    round_trip::<NewPlaylist>("playlists/create_request.json");
    round_trip::<NewPlaylist>("playlists/create_smart_request.json");
    round_trip::<EntryIds>("playlists/order_request.json");
    round_trip::<PlaylistPatch>("playlists/patch_request.json");
}

#[test]
fn playback() {
    let request: PlaybackRequest = round_trip("playback/request.json");
    assert!(!request.caps.is_v2());
    let direct: PlaybackInfo = decode("playback/info_direct.json");
    assert_eq!(direct.method, PlaybackMethod::Direct);
    let transcode: PlaybackInfo = decode("playback/info_transcode.json");
    assert_eq!(transcode.method, PlaybackMethod::Transcode);
    assert!(transcode.hls);
    let status: SessionStatus = decode("playback/status_transcode.json");
    assert!(status.transcode.is_some());
    let status: SessionStatus = decode("playback/status_direct.json");
    assert!(status.transcode.is_none());
    let progress: ProgressReport = round_trip("playback/progress_request.json");
    assert_eq!(progress.state, ProgressState::Buffering);
    round_trip::<RatingRequest>("playback/rating_request.json");
    let streams: StreamChoice = round_trip("playback/streams_request.json");
    assert_eq!(streams.audio, AudioChoice::Track(2));
    let trickplay: TrickplayInfo = decode("trickplay/info.json");
    assert!(trickplay.count > 0);
    let _: ConversionInfo = decode("conversions/direct.json");
    let _: ConversionInfo = decode("conversions/running.json");
}

#[test]
fn capability_reports() {
    let v1: ClientCaps = decode("caps/v1.json");
    assert!(!v1.is_v2());
    assert_eq!(
        serde_json::to_value(&v1).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&read("caps/v1.json")).unwrap()
    );
    let v2: ClientCaps = decode("caps/v2.json");
    assert!(v2.is_v2());
    assert_eq!(v2.video.len(), 4);
    assert_eq!(v2.dolby_vision_profiles[1].max_level, Some(6));
    assert_eq!(v2.hls.variants, Some(false));
    assert!(v2.plays_container("Matroska"));
    assert!(v2.shows(DynamicRange::Hdr10Plus));
    // Written back, whole numbers lose their decimal point as JavaScript writes them.
    let written = serde_json::to_string(&v2).unwrap();
    assert!(written.contains(r#""max_fps":60,"#), "{written}");
    assert!(written.contains(r#""refresh_rates":[23.976,24,50,59.94,60]"#), "{written}");
}

#[test]
fn live_tv() {
    let summary: LiveSummary = decode("live/summary.json");
    assert!(summary.allowed);
    let groups: LiveGroups = decode("live/groups.json");
    assert!(!groups.groups.is_empty());
    let page: LiveChannelPage = decode("live/browse.json");
    assert!(!page.channels.is_empty());
    let channels: Vec<LiveChannel> = decode("live/channels.json");
    assert!(channels[0].now.is_some());
    let guide: LiveGuide = decode("live/guide.json");
    assert!(!guide.programs.is_empty());
    let watching: LiveWatching = decode("live/watching.json");
    assert!(!watching.url.is_empty());
    let plan: LivePlan = decode("live/plan.json");
    assert!(plan.direct());
    let busy: LiveBusy = decode("live/busy.json");
    assert!(busy.connections[0].can_take_over);
    round_trip::<WatchRequest>("live/watch_request.json");
}

#[test]
fn events_and_errors() {
    let e = |name: &str| decode::<ServerEvent>(&format!("events/{name}.json"));
    assert_eq!(e("libraries_changed"), ServerEvent::LibrariesChanged);
    assert_eq!(e("library_changed"), ServerEvent::LibraryChanged { library_id: 2 });
    assert_eq!(e("scan_finished"), ServerEvent::ScanFinished { library_id: 1, added: 3, updated: 1, removed: 0 });
    assert_eq!(
        e("scan_progress"),
        ServerEvent::ScanProgress { library_id: 1, phase: ScanPhase::Probing, processed: 118, total: 412 }
    );
    assert_eq!(e("scan_started"), ServerEvent::ScanStarted { library_id: 1 });
    assert_eq!(e("sessions_changed"), ServerEvent::SessionsChanged);
    assert_eq!(e("unknown_type"), ServerEvent::Other("transcoder_restarted".into()));
    let scope: ErrorBody = decode("errors/insufficient_scope.json");
    assert_eq!(scope.code, "insufficient_scope");
    let missing: ErrorBody = decode("errors/not_found.json");
    assert_eq!(missing.message, "item 42 not found");
}
