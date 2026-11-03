use super::*;
use crate::device::scratch_dir;
use crate::model::ProfileNeeds;

fn store(name: &str) -> Store {
    Store::open(scratch_dir(name)).unwrap()
}

#[test]
fn values_survive_reopening_and_leave_no_temporary_files() {
    let dir = scratch_dir("store-reopen");
    let s = Store::open(&dir).unwrap();
    s.set("pley.server", "http://192.168.1.20:7539").unwrap();
    s.set("pley.server", "http://nas:7539").unwrap();
    s.set_raw("weird key/with:colons", b"[1,2]").unwrap();
    let again = Store::open(&dir).unwrap();
    assert_eq!(again.get::<String>("pley.server").as_deref(), Some("http://nas:7539"));
    assert_eq!(again.get::<Vec<i32>>("weird key/with:colons"), Some(vec![1, 2]));
    assert_eq!(again.keys(), ["pley.server", "weird key/with:colons"]);
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());
    again.remove("pley.server").unwrap();
    again.remove("pley.server").unwrap();
    assert_eq!(again.get::<String>("pley.server"), None);
}

#[test]
fn a_value_of_another_shape_reads_as_absent() {
    let s = store("store-shape");
    s.set_raw(keys::SERVERS, b"{\"not\": \"a list\"}").unwrap();
    assert!(s.servers().is_empty());
    s.set_raw(keys::SERVER, b"not json").unwrap();
    assert_eq!(s.active_server(), None);
}

#[test]
fn the_install_id_is_made_once() {
    let s = store("store-install");
    let id = s.install_id();
    assert!(id.starts_with("webos-") && id.len() == 38, "{id}");
    assert_eq!(s.install_id(), id);
    s.set(keys::INSTALL_ID, "android-1").unwrap();
    assert_ne!(s.install_id(), "android-1");
}

#[test]
fn servers_are_newest_first_once_each() {
    let s = store("store-servers");
    s.remember_server("http://192.168.1.20:7539", "Living room", Some("abc"), 1).unwrap();
    s.remember_server("https://pley.example.com/", "Away", None, 2).unwrap();
    // The same server found at a new address replaces its old entry.
    s.remember_server("http://192.168.1.21:7539", "Living room", Some("abc"), 3).unwrap();
    let urls: Vec<String> = s.servers().into_iter().map(|x| x.url).collect();
    assert_eq!(urls, ["http://192.168.1.21:7539", "https://pley.example.com"]);
    for n in 0..30 {
        s.remember_server(&format!("http://10.0.0.{n}:7539"), "x", None, 10 + n).unwrap();
    }
    assert_eq!(s.servers().len(), MAX_SERVERS);
}

#[test]
fn forgetting_a_server_forgets_only_what_is_its() {
    let s = store("store-forget");
    let a = "http://192.168.1.20:7539";
    let b = "http://192.168.1.2:7539";
    for server in [a, b] {
        s.remember_server(server, "x", None, 1).unwrap();
        s.set(&keys::tokens(server), "secret").unwrap();
        s.set_last_profile(server, 2).unwrap();
        s.set(&keys::library_view(server, 2, 7), &crate::model::LibraryQuery::default()).unwrap();
        s.remember_search(server, 2, "matrix").unwrap();
    }
    s.set_active_server(Some(a)).unwrap();
    s.forget_server(a).unwrap();
    assert_eq!(s.active_server(), None);
    assert!(s.keys().iter().all(|k| !k.contains("192.168.1.20")), "{:?}", s.keys());
    assert_eq!(s.get::<String>(&keys::tokens(b)).as_deref(), Some("secret"));
    assert_eq!(s.searches(b, 2), ["matrix"]);
    assert_eq!(s.servers().len(), 1);
}

#[test]
fn households_and_profiles_are_kept_per_server() {
    let s = store("store-household");
    let server = "http://nas:7539";
    let kid = AppProfile { id: 4, name: "Kid".into(), needs: ProfileNeeds::Pin, ..AppProfile::default() };
    s.set_household(server, std::slice::from_ref(&kid)).unwrap();
    assert_eq!(s.household(server), [kid]);
    assert!(s.household("http://other:7539").is_empty());
}

#[test]
fn recent_searches_are_ten_newest_without_repeats() {
    let s = store("store-searches");
    for q in ["matrix", "Alien", "MATRIX", "  "] {
        s.remember_search("http://nas:7539", 2, q).unwrap();
    }
    assert_eq!(s.searches("http://nas:7539", 2), ["MATRIX", "Alien"]);
    for n in 0..15 {
        s.remember_search("http://nas:7539", 2, &format!("q{n}")).unwrap();
    }
    assert_eq!(s.searches("http://nas:7539", 2).len(), MAX_SEARCHES);
    assert_eq!(s.searches("http://nas:7539", 2)[0], "q14");
}

#[test]
fn subtitle_offsets_keep_the_newest_300() {
    let s = store("store-offsets");
    let server = "http://nas:7539";
    s.set_subtitle_offset(server, 7, 2, -250).unwrap();
    assert_eq!(s.subtitle_offset(server, 7, 2), -250);
    assert_eq!(s.subtitle_offset(server, 7, 3), 0);
    for f in 0..310 {
        s.set_subtitle_offset(server, 1000 + f, 2, 100).unwrap();
    }
    assert_eq!(s.subtitle_offset(server, 7, 2), 0, "the oldest went first");
    assert_eq!(s.subtitle_offset(server, 1309, 2), 100);
    s.set_subtitle_offset(server, 1309, 2, 0).unwrap();
    assert_eq!(s.subtitle_offset(server, 1309, 2), 0);
}

#[test]
fn automatics_memory_lasts_30_minutes() {
    let s = store("store-auto");
    s.set_auto_quality("http://nas:7539", Some(8000), 1_000).unwrap();
    assert_eq!(s.auto_quality("http://nas:7539", 1_000 + AUTO_QUALITY_MS - 1), Some(8000));
    assert_eq!(s.auto_quality("http://nas:7539", 1_000 + AUTO_QUALITY_MS), None);
}

#[test]
fn settings_keep_only_what_differs_from_the_defaults() {
    let s = store("store-settings");
    assert_eq!(Settings::load(&s), Settings::default());
    let mut settings = Settings { quality: "8m".into(), subtitle_size: SubtitleSize::Huge, ..Settings::default() };
    settings.passthrough.set(PassthroughFormat::TrueHd, PassthroughChoice::On);
    settings.set_always_start_as("http://nas:7539", Some(4));
    settings.save(&s).unwrap();
    let kept: serde_json::Value = s.get(keys::SETTINGS).unwrap();
    assert_eq!(
        kept,
        serde_json::json!({
            "quality": "8m",
            "passthrough": {"truehd": "on"},
            "subtitle_size": "huge",
            "always_start_as": {"http://nas:7539": 4}
        })
    );
    assert_eq!(Settings::load(&s), settings);
    assert_eq!(settings.quality_preset().max_bitrate_kbps, Some(8000));
    assert_eq!(settings.always_start_as("http://nas:7539"), Some(4));
}

#[test]
fn settings_this_version_doesnt_know_fall_back_alone() {
    let v = serde_json::json!({
        "quality": "100m",
        "autoplay_next": "yes",
        "subtitle_font": "comic",
        "reduce_motion": true,
        "passthrough": {"dts": "sometimes", "ac3": "off"}
    });
    let s = Settings::from_json(&v);
    assert_eq!(s.quality, "auto");
    assert!(s.autoplay_next);
    assert_eq!(s.subtitle_font, SubtitleFont::Sans);
    assert!(s.reduce_motion);
    assert_eq!(s.passthrough.get(PassthroughFormat::Dts), PassthroughChoice::Auto);
    assert_eq!(s.passthrough.get(PassthroughFormat::Ac3), PassthroughChoice::Off);
}

#[test]
fn snapshots_round_trip_and_skip_unchanged_writes() {
    let snaps = Snapshots::open(scratch_dir("snap-trip"));
    let server = "http://nas:7539";
    snaps.put(server, 2, snapshot_keys::HOME, &serde_json::json!({"hubs": []})).unwrap();
    assert_eq!(snaps.get::<serde_json::Value>(server, 2, "home"), Some(serde_json::json!({"hubs": []})));
    assert_eq!(snaps.get::<serde_json::Value>(server, 3, "home"), None);
    let path = snaps.path(server, 2, "home");
    let written = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    snaps.put(server, 2, "home", &serde_json::json!({"hubs": []})).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), written);
    snaps.forget_server(server);
    assert_eq!(snaps.read(server, 2, "home"), None);
    assert_eq!(snaps.total_bytes(), 0);
}

#[test]
fn snapshots_stay_within_their_budget_dropping_the_least_recently_used() {
    let snaps = Snapshots::with_budget(scratch_dir("snap-budget"), 10_000);
    let server = "http://nas:7539";
    let blob = vec![b'x'; 3_000];
    for n in 0..3 {
        snaps.write(server, 2, &snapshot_keys::item(n), &blob).unwrap();
    }
    // Reading the first makes the second the least recently used.
    assert!(snaps.read(server, 2, &snapshot_keys::item(0)).is_some());
    snaps.write(server, 2, &snapshot_keys::item(3), &blob).unwrap();
    assert!(snaps.total_bytes() <= 10_000);
    assert!(snaps.read(server, 2, &snapshot_keys::item(1)).is_none());
    assert!(snaps.read(server, 2, &snapshot_keys::item(0)).is_some());
    assert!(snaps.read(server, 2, &snapshot_keys::item(3)).is_some());
    // A reopened store finds what is on disk.
    let again = Snapshots::with_budget(snaps.root().to_path_buf(), 10_000);
    assert_eq!(again.total_bytes(), 9_000);
}

#[test]
fn each_server_keeps_its_last_three_profiles() {
    let snaps = Snapshots::open(scratch_dir("snap-profiles"));
    let server = "http://nas:7539";
    for profile in 1..=4 {
        snaps.put(server, profile, "home", &profile).unwrap();
    }
    assert_eq!(snaps.get::<i64>(server, 1, "home"), None);
    for profile in 2..=4 {
        assert_eq!(snaps.get::<i64>(server, profile, "home"), Some(profile));
    }
    snaps.put("http://other:7539", 1, "home", &1).unwrap();
    assert_eq!(snaps.get::<i64>(server, 2, "home"), Some(2), "another server's profiles count apart");
}
