//! packages/tv/test/player/autoquality.test.ts, ported: the Quality menu,
//! the server's own quality, Automatic's ladder and its decisions.

use std::sync::{Arc, Mutex};

use pley_core::model::{MediaFile, PlaybackMode};
use pley_player::autoquality::{AutoMemory, AutoOptions, AutoQuality, AutoStep, Lowered, MEMORY_MS, Remembered};
use pley_player::quality::{
    Quality, auto_ladder, choose_quality, named_for, preset, presets, qualities_for, qualities_in, server_quality,
};

fn file(width: Option<i64>, height: Option<i64>, mbps: Option<f64>) -> MediaFile {
    MediaFile {
        id: 1,
        path: "/f.mkv".into(),
        size: 1,
        container: Some("mkv".into()),
        duration_ms: Some(7_200_000),
        bitrate: mbps.map(|m| (m * 1_000_000.0) as i64),
        width,
        height,
        video_codec: Some("hevc".into()),
        audio_codec: Some("eac3".into()),
        ..MediaFile::default()
    }
}

fn f(w: i64, h: i64, mbps: f64) -> MediaFile {
    file(Some(w), Some(h), Some(mbps))
}

fn ids(list: &[Quality]) -> Vec<&str> {
    list.iter().map(|q| q.id).collect()
}

#[test]
fn the_menu_lists_original_automatic_the_4k_qualities_then_1080p_and_below() {
    let all: Vec<(&str, String)> = presets().into_iter().map(|p| (p.id, p.label)).collect();
    let want = [
        ("original", "Original"),
        ("auto", "Automatic"),
        ("30m", "30 Mbps 4K"),
        ("15m", "15 Mbps 4K"),
        ("20m", "20 Mbps 1080p"),
        ("12m", "12 Mbps 1080p"),
        ("8m", "8 Mbps 1080p"),
        ("4m", "4 Mbps 720p"),
        ("2m", "2 Mbps 480p"),
    ];
    assert_eq!(all, want.map(|(i, l)| (i, l.to_string())));
}

#[test]
fn offers_original_automatic_and_qualities_that_make_a_file_smaller() {
    // 8 Mbps is too close to a 9 Mbps file to play any better; to a 10 Mbps one it isn't.
    assert_eq!(ids(&qualities_for(Some(&f(1920, 1080, 9.0)))), ["original", "auto", "4m", "2m"]);
    assert_eq!(ids(&qualities_for(Some(&f(1920, 1080, 10.0)))), ["original", "auto", "8m", "4m", "2m"]);
    assert_eq!(ids(&qualities_for(Some(&f(720, 480, 1.5)))), ["original", "auto"]);
    assert_eq!(
        ids(&qualities_for(Some(&f(3840, 2160, 60.0)))),
        ["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"]
    );
    // Every one, by the table's names, without a file (Settings' default quality).
    assert_eq!(qualities_for(None), presets());
}

#[test]
fn names_a_quality_taller_than_the_file_by_the_files_own_lines() {
    let named = |file: MediaFile| qualities_for(Some(&file)).into_iter().map(|q| q.label).collect::<Vec<_>>();
    assert_eq!(named(f(1280, 720, 12.0)), ["Original", "Automatic", "8 Mbps 720p", "4 Mbps 720p", "2 Mbps 480p"]);
    // A film 800 lines tall in 1080p's width is 1080p: its 4K qualities are named 1080p, and fall in among them.
    assert_eq!(
        named(f(1920, 800, 25.0)),
        [
            "Original",
            "Automatic",
            "20 Mbps 1080p",
            "15 Mbps 1080p",
            "12 Mbps 1080p",
            "8 Mbps 1080p",
            "4 Mbps 720p",
            "2 Mbps 480p"
        ]
    );
    let eight = preset("8m").unwrap();
    assert_eq!(named_for(Some(&f(1280, 720, 12.0)), &eight).label, "8 Mbps 720p");
    assert_eq!(named_for(Some(&f(3840, 2160, 60.0)), &eight).label, "8 Mbps 1080p");
    assert_eq!(named_for(None, &eight).label, "8 Mbps 1080p");
}

#[test]
fn shows_two_qualities_that_come_to_the_same_bitrate_and_lines_once() {
    let q = |id: &'static str, label: &str, kbps: u32, h: u32| Quality {
        id,
        label: label.into(),
        max_bitrate_kbps: Some(kbps),
        max_height: Some(h),
    };
    let table =
        [Quality::original(), Quality::auto(), q("8m", "8 Mbps 1080p", 8000, 1080), q("8s", "8 Mbps 720p", 8000, 720)];
    let got: Vec<(&str, String)> =
        qualities_in(Some(&f(1280, 720, 12.0)), &table).into_iter().map(|q| (q.id, q.label)).collect();
    assert_eq!(
        got,
        [("original", "Original".to_string()), ("auto", "Automatic".to_string()), ("8m", "8 Mbps 720p".to_string())]
    );
}

#[test]
fn starts_with_automatic_when_nothing_was_picked_and_keeps_a_pick() {
    let offered = qualities_for(Some(&f(1920, 1080, 12.0)));
    assert_eq!(choose_quality(None, &offered), Quality::auto());
    assert_eq!(choose_quality(Some("nonsense"), &offered), Quality::auto());
    assert_eq!(choose_quality(Some("original"), &offered), Quality::original());
    assert_eq!(choose_quality(Some("auto"), &offered), Quality::auto());
    assert_eq!(choose_quality(Some("8m"), &offered).id, "8m");
    // A quality this file doesn't need plays the original.
    assert_eq!(choose_quality(Some("20m"), &offered), Quality::original());
}

#[test]
fn asks_the_server_to_choose_for_the_original_and_automatic_and_to_convert_the_rest() {
    assert_eq!(Quality::original().mode(), PlaybackMode::Auto);
    assert_eq!(Quality::auto().mode(), PlaybackMode::Auto);
    assert_eq!(presets()[6].mode(), PlaybackMode::Transcode);
}

#[test]
fn the_servers_own_quality_matches_its_closest_quality() {
    let pick = |w, h, mbps| server_quality(Some(&f(w, h, mbps))).id;
    assert_eq!(pick(3840, 2160, 57.0), "30m");
    assert_eq!(pick(3840, 2160, 18.0), "15m");
    assert_eq!(pick(3840, 2160, 6.0), "15m");
    assert_eq!(pick(3840, 1600, 40.0), "30m");
    assert_eq!(pick(1920, 1080, 25.0), "20m");
    assert_eq!(pick(1920, 1080, 10.0), "12m");
    assert_eq!(pick(1920, 1080, 4.0), "8m");
    assert_eq!(pick(1920, 800, 9.0), "12m");
    assert_eq!(pick(1280, 720, 3.0), "4m");
    assert_eq!(pick(720, 480, 2.0), "2m");
    assert_eq!(server_quality(Some(&file(None, None, None))).id, "20m");
}

#[test]
fn automatics_steps_go_down_in_bitrate_never_above_the_servers_quality_nor_the_files_lines() {
    let ladder = |file: MediaFile| auto_ladder(Some(&file)).into_iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ladder(f(3840, 2160, 60.0)), ["30m", "15m", "12m", "8m", "4m", "2m"]);
    // 15 Mbps is too close to an 18 Mbps file to play any better: 12 Mbps 1080p is the first step.
    assert_eq!(ladder(f(3840, 2160, 18.0)), ["12m", "8m", "4m", "2m"]);
    assert_eq!(ladder(f(1920, 1080, 40.0)), ["20m", "12m", "8m", "4m", "2m"]);
    assert_eq!(ladder(f(1920, 1080, 5.0)), ["4m", "2m"]);
    assert!(ladder(f(720, 480, 1.5)).is_empty());
}

// ---- the decisions

const WALL: i64 = 1_000_000_000_000;

#[derive(Clone, Default)]
struct Memory(Arc<Mutex<Option<Remembered>>>);

impl Memory {
    fn with(kbps: f64, at: i64) -> Memory {
        Memory(Arc::new(Mutex::new(Some(Remembered { kbps, at }))))
    }

    fn value(&self) -> Option<Remembered> {
        *self.0.lock().unwrap()
    }
}

impl AutoMemory for Memory {
    fn get(&self) -> Option<Remembered> {
        self.value()
    }

    fn set(&mut self, value: Option<Remembered>) {
        *self.0.lock().unwrap() = value;
    }
}

/// Automatic on a clock the test moves (`t`, ms; the wall clock is 1e12 + t).
struct Auto {
    a: AutoQuality,
    t: Arc<Mutex<i64>>,
    mem: Memory,
    told: Arc<Mutex<Vec<Option<f64>>>>,
    pos: f64,
    end: f64,
}

/// Automatic for `file` (4K of 60 Mbps by default); `blanks`: on a player that goes black between streams.
fn auto_with(mem: Memory, file: MediaFile, known: Option<Option<Remembered>>, blanks: bool) -> Auto {
    let t = Arc::new(Mutex::new(0i64));
    let told = Arc::new(Mutex::new(Vec::new()));
    let clock = t.clone();
    let heard = told.clone();
    let mut a = AutoQuality::new(AutoOptions {
        ladder: auto_ladder(Some(&file)),
        original_kbps: file.bitrate.map(|b| b as f64 / 1000.0),
        memory: Some(Box::new(mem.clone())),
        wall: Box::new(move || WALL + *clock.lock().unwrap()),
        on_ceiling: Some(Box::new(move |k| heard.lock().unwrap().push(k))),
        blanks,
    });
    a.start(0, known);
    Auto { a, t, mem, told, pos: 100.0, end: 130.0 }
}

fn auto() -> Auto {
    auto_with(Memory::default(), f(3840, 2160, 60.0), None, false)
}

fn hd(mbps: f64) -> Auto {
    auto_with(Memory::default(), f(1920, 1080, mbps), None, false)
}

impl Auto {
    fn now(&self) -> i64 {
        *self.t.lock().unwrap()
    }

    fn pass(&mut self, ms: i64) {
        *self.t.lock().unwrap() += ms;
    }

    fn told(&self) -> Vec<Option<f64>> {
        self.told.lock().unwrap().clone()
    }

    /// Plays `seconds` at `delivery` seconds of video a second, starting with
    /// `ahead` seconds loaded ahead; checks each second and returns the first step.
    fn play(&mut self, seconds: u32, delivery: f64, ahead: Option<f64>) -> Option<AutoStep> {
        if let Some(ahead) = ahead {
            self.end = self.pos + ahead;
        }
        for _ in 0..seconds {
            self.pass(1000);
            // The playhead moves on as far as what has loaded.
            self.pos += (self.end - self.pos).clamp(0.0, 1.0);
            self.end += delivery;
            let now = self.now();
            self.a.sample(now, self.pos, Some(self.end));
            if let Some(step) = self.a.check(now) {
                return Some(step);
            }
        }
        None
    }

    /// A wait for data of `ms`.
    fn wait(&mut self, ms: i64) -> Option<AutoStep> {
        self.a.waiting(self.now());
        let mut step = None;
        let mut left = ms;
        while left > 0 {
            self.pass(left.min(500));
            if step.is_none() {
                step = self.a.check(self.now());
            }
            left -= 500;
        }
        self.a.resumed(self.now());
        step.or_else(|| self.a.check(self.now()))
    }

    /// Up to `seconds` pass with segments loading at `kbps`, one every 4 s, until a step.
    fn network(&mut self, seconds: u32, kbps: f64) -> Option<AutoStep> {
        for _ in (0..seconds).step_by(4) {
            self.pass(4000);
            let ms = 1000.0;
            self.a.loaded(self.now(), kbps * ms / 8.0, ms);
            if let Some(step) = self.a.check(self.now()) {
                return Some(step);
            }
        }
        None
    }
}

fn step(to: &str, why: Option<Lowered>) -> Option<AutoStep> {
    Some(AutoStep { to: to.into(), why, bitrate: None })
}

fn live_step(to: &str, why: Option<Lowered>, bitrate: f64) -> Option<AutoStep> {
    Some(AutoStep { to: to.into(), why, bitrate: Some(bitrate) })
}

const NET: Option<Lowered> = Some(Lowered::Network);
const DEVICE: Option<Lowered> = Some(Lowered::Device);

fn to(s: Option<AutoStep>) -> Option<String> {
    s.map(|s| s.to)
}

fn bitrate(s: Option<AutoStep>) -> Option<f64> {
    s.and_then(|s| s.bitrate)
}

#[test]
fn starts_at_the_original() {
    let t = auto();
    assert_eq!(t.a.playing, "original");
    assert_eq!(t.a.lowered, None);
}

#[test]
fn plays_on_while_the_video_arrives_fast_enough() {
    let mut t = auto();
    assert_eq!(t.play(120, 1.2, Some(8.0)), None);
    assert_eq!(t.a.playing, "original");
}

#[test]
fn a_wait_of_2_s_steps_down_to_the_bitrate_times_the_delivery_times_0_75() {
    let mut t = hd(20.0);
    // 20 Mbps arriving at half speed: 20 × 0.5 × 0.75 = 7.5 → 4 Mbps 720p.
    t.play(20, 0.5, Some(9.0));
    assert_eq!(t.wait(2_000), step("4m", NET));
    assert_eq!(t.a.playing, "4m");
    assert_eq!(t.a.lowered, NET);
}

#[test]
fn a_wait_of_2_s_counts_while_it_lasts() {
    let mut t = auto();
    t.a.waiting(0);
    t.pass(1_500);
    assert_eq!(t.a.check(t.now()), None);
    t.pass(500);
    assert_eq!(t.a.check(t.now()).and_then(|s| s.why), NET);
}

#[test]
fn two_short_waits_within_a_minute_step_down_one_doesnt_nor_two_a_minute_apart() {
    let mut one = auto();
    assert_eq!(one.wait(800), None);
    assert_eq!(one.play(30, 1.2, None), None);
    assert_eq!(one.wait(800).and_then(|s| s.why), NET);

    let mut apart = auto();
    assert_eq!(apart.wait(800), None);
    apart.pass(61_000);
    assert_eq!(apart.wait(800), None);
    assert_eq!(apart.a.playing, "original");
}

#[test]
fn a_wait_isnt_one_when_a_seek_a_pause_or_a_new_session_ends_it() {
    let mut t = auto();
    t.a.waiting(t.now());
    t.pass(1_000);
    t.a.interrupted();
    t.pass(5_000);
    t.a.resumed(t.now());
    assert_eq!(t.a.check(t.now()), None);
    // Short waits of the session before don't pair with the next session's.
    t.a.waiting(t.now());
    t.pass(500);
    t.a.resumed(t.now());
    t.a.began(None);
    t.a.waiting(t.now());
    t.pass(500);
    t.a.resumed(t.now());
    assert_eq!(t.a.check(t.now()), None);
}

#[test]
fn slow_delivery_steps_down_before_it_waits() {
    let mut t = hd(20.0);
    // 20 × 0.8 × 0.75 = 12 → 12 Mbps 1080p.
    assert_eq!(t.play(40, 0.8, Some(9.0)), step("12m", NET));
    assert_eq!(t.a.playing, "12m");
}

#[test]
fn slow_delivery_doesnt_with_10_s_or_more_ahead_or_ahead_not_falling() {
    assert_eq!(auto().play(60, 0.9, Some(25.0)), None);
    assert_eq!(auto().play(60, 1.05, Some(5.0)), None);
}

#[test]
fn slow_delivery_cant_tell_when_the_player_doesnt_know_what_it_has_loaded() {
    let mut t = auto();
    for i in 0..60 {
        t.pass(1000);
        t.a.sample(t.now(), 100.0 + i as f64, None);
        assert_eq!(t.a.check(t.now()), None);
    }
}

#[test]
fn stepping_down_never_goes_above_the_servers_own_quality() {
    let mut t = auto();
    // 60 × 0.9 × 0.75 = 40.5, above the server's 30 Mbps 4K.
    t.play(20, 0.9, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("30m"));
}

#[test]
fn stepping_down_never_goes_below_2_mbps_480p() {
    let mut t = hd(20.0);
    t.play(20, 0.05, Some(2.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("2m"));
    t.pass(30_000);
    assert_eq!(t.wait(3_000), None);
    assert_eq!(t.a.playing, "2m");
}

#[test]
fn stepping_down_goes_one_quality_down_at_least_again_on_more_issues() {
    let mut t = auto();
    t.play(20, 1.0, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("30m"));
    t.pass(25_000);
    // At 30 Mbps, arriving as fast as it plays: 30 × 1 × 0.75 = 22.5, still one down.
    t.play(20, 1.0, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("15m"));
    assert_eq!(t.a.playing, "15m");
}

#[test]
fn stepping_down_waits_20_s_after_a_change_before_the_next() {
    let mut t = auto();
    t.play(20, 0.9, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("30m"));
    t.pass(5_000);
    assert_eq!(t.wait(2_000), None);
    assert_eq!(t.a.playing, "30m");
    t.pass(15_000);
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("15m"));
}

#[test]
fn dropped_frames_step_one_quality_down_as_the_devices_not_remembered() {
    let mut t = auto();
    t.play(20, 0.9, Some(9.0));
    t.wait(2_000);
    let kept = t.mem.value();
    t.pass(21_000);
    t.a.decoding();
    assert_eq!(t.a.check(t.now()), step("15m", DEVICE));
    assert_eq!(t.a.lowered, DEVICE);
    assert_eq!(t.mem.value(), kept);
}

#[test]
fn waits_after_the_players_own_fallback_as_after_a_change_of_its_own() {
    let mut t = auto();
    t.a.fell_back(t.now());
    t.pass(10_000);
    t.a.decoding();
    assert_eq!(t.a.check(t.now()), None);
}

#[test]
fn steps_from_what_the_original_plays_at_once_the_player_converts_it() {
    let mut t = auto();
    t.a.set_original_kbps(Some(15_000.0));
    t.a.decoding();
    t.pass(1_000);
    // 15 × 0.75 = 11.25 → 8 Mbps 1080p (12 is above it).
    assert_eq!(to(t.a.check(t.now())).as_deref(), Some("8m"));
}

fn lowered() -> Auto {
    let mut t = hd(20.0);
    t.play(20, 0.5, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    t
}

#[test]
fn stepping_up_waits_5_minutes_after_a_step_down_then_goes_one_step_up() {
    let mut t = lowered();
    // 3 calm minutes aren't enough so soon after the step down.
    assert_eq!(t.network(180, 50_000.0), None);
    assert_eq!(t.network(124, 50_000.0), step("8m", None));
    assert_eq!(t.a.playing, "8m");
}

#[test]
fn stepping_up_stays_when_the_network_doesnt_carry_1_6_times_the_next_quality_or_cant_say() {
    let mut t = lowered();
    assert_eq!(t.network(400, 8_000.0 * 1.5), None);
    t.pass(400_000);
    assert_eq!(t.a.check(t.now()), None);
    assert_eq!(t.a.playing, "4m");
}

#[test]
fn stepping_up_needs_3_minutes_without_an_issue() {
    let mut t = hd(20.0);
    t.play(20, 0.05, Some(2.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("2m"));
    assert_eq!(t.network(296, 50_000.0), None);
    // An issue at the lowest quality changes nothing, but 3 calm minutes count from it.
    t.wait(800);
    assert_eq!(t.wait(800), None);
    assert_eq!(t.a.playing, "2m");
    assert_eq!(t.network(176, 50_000.0), None);
    assert_eq!(to(t.network(8, 50_000.0)).as_deref(), Some("4m"));
}

#[test]
fn one_step_every_3_minutes_back_to_the_original_which_forgets_the_network() {
    let mut t = lowered();
    assert!(t.mem.value().is_some());
    assert_eq!(to(t.network(304, 50_000.0)).as_deref(), Some("8m"));
    assert_eq!(t.network(176, 50_000.0), None);
    assert_eq!(to(t.network(8, 50_000.0)).as_deref(), Some("12m"));
    assert_eq!(to(t.network(184, 50_000.0)).as_deref(), Some("original"));
    assert_eq!(t.a.lowered, None);
    assert_eq!(t.mem.value(), None);
}

#[test]
fn settles_a_step_up_undone_within_10_minutes_isnt_tried_again() {
    let mut t = lowered();
    assert_eq!(to(t.network(304, 50_000.0)).as_deref(), Some("8m"));
    // Two minutes on, the network can't carry 8 Mbps after all (8 × 0.8 × 0.75 = 4.8).
    t.network(120, 50_000.0);
    t.play(20, 0.8, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    let kept = t.mem.value();
    // However fast the network says it is, it stays where it held, for the rest of the title.
    assert_eq!(t.network(3_600, 80_000.0), None);
    assert_eq!(t.a.playing, "4m");
    assert_eq!(t.mem.value(), kept);
}

#[test]
fn a_step_down_more_than_10_minutes_after_a_step_up_leaves_that_quality_open() {
    let mut t = lowered();
    assert_eq!(to(t.network(304, 50_000.0)).as_deref(), Some("8m"));
    t.network(604, 9_000.0);
    t.play(20, 0.8, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    assert_eq!(to(t.network(304, 50_000.0)).as_deref(), Some("8m"));
    assert_eq!(t.a.playing, "8m");
}

#[test]
fn each_step_up_undone_doubles_the_calm_the_next_one_waits_for() {
    let mut t = lowered();
    assert_eq!(to(t.network(304, 50_000.0)).as_deref(), Some("8m"));
    assert_eq!(to(t.network(184, 50_000.0)).as_deref(), Some("12m"));
    // 12 Mbps doesn't hold (12 × 0.9 × 0.75 = 8.1): back to 8, and 12 is off for the title.
    t.play(20, 0.9, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("8m"));
    // Later 8 Mbps falters too, more than 10 minutes after it was reached: 4, with 8 still open.
    t.network(604, 9_000.0);
    t.play(20, 0.8, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    // 5 minutes after the step down 3 calm minutes would have done; now it takes 6.
    assert_eq!(t.network(352, 50_000.0), None);
    assert_eq!(to(t.network(16, 50_000.0)).as_deref(), Some("8m"));
    assert_eq!(t.a.playing, "8m");
}

#[test]
fn on_lg_goes_up_on_the_fastest_refill_the_buffer_showed() {
    let mut t = hd(20.0);
    t.play(20, 0.5, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    // Back at 4 Mbps, the buffer refills at 4 times the video's pace: the network carries 16 Mbps at least.
    t.play(8, 4.0, None);
    // Full, it fills only as fast as it plays: every figure since says hardly more than 4 Mbps.
    assert_eq!(t.play(300, 1.02, None), step("8m", None));
}

#[test]
fn on_lg_stays_down_when_no_refill_carried_1_6_times_the_next_quality() {
    let mut t = hd(20.0);
    t.play(20, 0.5, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    t.play(8, 1.3, None);
    assert_eq!(t.play(330, 1.02, None), None);
    assert_eq!(t.a.playing, "4m");
}

#[test]
fn measures_the_network_from_engine_figures_too_by_their_median() {
    let mut t = lowered();
    t.pass(301_000);
    // 1.6 × 8 Mbps is 12.8: the median, 20, carries it (the mean of the first three wouldn't).
    for kbps in [100.0, 7_000.0, 20_000.0, 25_000.0, 30_000.0] {
        t.a.bandwidth(t.now(), kbps);
    }
    assert_eq!(t.a.check(t.now()), step("8m", None));
}

#[test]
fn memory_keeps_the_ceiling_of_a_step_down_for_the_device_and_server() {
    let mut t = hd(20.0);
    t.play(20, 0.5, Some(9.0));
    t.wait(2_000);
    assert_eq!(t.mem.value(), Some(Remembered { kbps: 7_500.0, at: WALL + 22_000 }));
}

#[test]
fn memory_starts_the_next_title_at_most_at_it_and_not_up_again_before_5_minutes_from_then() {
    let mut t = auto_with(Memory::with(7_500.0, WALL - 60_000), f(1920, 1080, 20.0), None, false);
    assert_eq!(t.a.playing, "4m");
    assert_eq!(t.a.lowered, NET);
    assert_eq!(t.network(200, 50_000.0), None);
    assert_eq!(to(t.network(44, 50_000.0)).as_deref(), Some("8m"));
}

#[test]
fn memory_starts_at_the_original_when_the_file_needs_less_than_it() {
    let t = auto_with(Memory::with(7_500.0, WALL - 60_000), f(1920, 1080, 5.0), None, false);
    assert_eq!(t.a.playing, "original");
}

#[test]
fn memory_is_forgotten_after_30_minutes() {
    let mem = Memory::with(2_000.0, WALL - MEMORY_MS);
    let t = auto_with(mem.clone(), f(1920, 1080, 20.0), None, false);
    assert_eq!(t.a.playing, "original");
    assert_eq!(mem.value(), None);
}

#[test]
fn the_servers_ceiling_takes_the_place_of_the_devices_memory() {
    let server = auto_with(
        Memory::with(2_000.0, WALL - 60_000),
        f(1920, 1080, 20.0),
        Some(Some(Remembered { kbps: 9_000.0, at: WALL - 3_600_000 })),
        false,
    );
    assert_eq!(server.a.playing, "8m");
    assert_eq!(server.a.lowered, NET);
    let none = auto_with(Memory::with(2_000.0, WALL - 60_000), f(1920, 1080, 20.0), Some(None), false);
    assert_eq!(none.a.playing, "original");
    assert_eq!(none.a.lowered, None);
    // Nothing is told back for what was only heard.
    assert!(server.told().is_empty() && none.told().is_empty());
}

#[test]
fn the_servers_ceiling_keeps_its_own_time() {
    let mut t = auto_with(
        Memory::default(),
        f(1920, 1080, 20.0),
        Some(Some(Remembered { kbps: 4_500.0, at: WALL - 86_000_000 })),
        false,
    );
    assert_eq!(t.a.playing, "4m");
    assert_eq!(to(t.network(184, 50_000.0)).as_deref(), Some("8m"));
}

#[test]
fn the_servers_ceiling_plays_the_original_when_the_file_needs_less_and_never_below_2_mbps() {
    let at = |kbps| Some(Some(Remembered { kbps, at: WALL }));
    assert_eq!(auto_with(Memory::default(), f(1920, 1080, 5.0), at(7_000.0), false).a.playing, "original");
    assert_eq!(auto_with(Memory::default(), f(1920, 1080, 20.0), at(300.0), false).a.playing, "2m");
}

#[test]
fn the_server_hears_each_change_of_the_ceiling() {
    let mut t = hd(20.0);
    assert!(t.a.started());
    t.play(20, 0.5, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    assert_eq!(to(t.network(304, 50_000.0)).as_deref(), Some("8m"));
    // The step up remembers the quality it plays now.
    assert_eq!(t.mem.value().map(|m| m.kbps), Some(8_000.0));
    assert_eq!(to(t.network(184, 50_000.0)).as_deref(), Some("12m"));
    assert_eq!(to(t.network(184, 50_000.0)).as_deref(), Some("original"));
    assert_eq!(t.told(), [Some(7_500.0), Some(8_000.0), Some(12_000.0), None]);
}

#[test]
fn the_server_isnt_told_of_a_step_down_for_the_device_nor_of_a_memory_that_ran_out() {
    let mut t = auto_with(Memory::with(2_000.0, WALL - MEMORY_MS), f(1920, 1080, 20.0), None, false);
    t.a.decoding();
    assert_eq!(t.a.check(30_000).and_then(|s| s.why), DEVICE);
    assert!(t.told().is_empty());
}

// ---- on a player that goes black between streams (LG)

fn blank(known: Option<Option<Remembered>>, file: MediaFile) -> Auto {
    auto_with(Memory::default(), file, known, true)
}

fn blank_hd() -> Auto {
    blank(None, f(1920, 1080, 20.0))
}

#[test]
fn the_original_moves_on_a_wait_of_2_s_on_a_network_too_slow_for_the_file() {
    let mut t = blank_hd();
    // The player measured 16 Mbps: under 1.5 × the file's 20.
    t.a.bandwidth(t.now(), 16_000.0);
    // 20 × 0.5 × 0.75 = 7.5 → 4 Mbps 720p.
    t.play(20, 0.5, Some(9.0));
    assert_eq!(t.wait(2_000), step("4m", NET));
    assert_eq!(t.a.playing, "4m");
}

#[test]
fn the_original_plays_on_through_waits_on_a_network_that_carries_1_5_times_the_file() {
    let mut t = blank_hd();
    t.a.bandwidth(t.now(), 30_000.0);
    t.play(20, 0.5, Some(9.0));
    assert_eq!(t.wait(2_000), None);
    assert_eq!(t.play(20, 0.5, None), None);
    assert_eq!(t.wait(6_000), None);
    assert_eq!(t.a.playing, "original");
}

#[test]
fn where_nothing_measured_the_network_the_original_moves_at_the_second_wait_within_5_minutes() {
    let mut t = blank_hd();
    t.play(20, 0.5, Some(9.0));
    assert_eq!(t.wait(3_000), None);
    assert_eq!(t.play(60, 0.5, Some(9.0)), None);
    assert_eq!(t.wait(2_000), step("4m", NET));
    let mut apart = blank_hd();
    assert_eq!(apart.wait(2_000), None);
    apart.pass(5 * 60_000);
    assert_eq!(apart.wait(2_000), None);
    assert_eq!(apart.a.playing, "original");
}

#[test]
fn a_wait_counts_as_unmeasured_once_the_networks_figures_run_out_as_it_goes_on() {
    let mut t = blank_hd();
    t.a.bandwidth(t.now(), 30_000.0);
    // 2.5 s into this wait the figure is 2 minutes old: from then on nothing measured the network.
    *t.t.lock().unwrap() = 118_000;
    assert_eq!(t.wait(5_000), None);
    t.pass(10_000);
    assert_eq!(t.wait(2_000).and_then(|s| s.why), NET);
}

#[test]
fn the_original_the_server_converts_moves_on_any_wait_of_2_s() {
    let mut t = blank_hd();
    t.a.set_original_kbps(Some(15_000.0));
    t.a.bandwidth(t.now(), 80_000.0);
    // 15 × 0.5 × 0.75 = 5.6 → 4 Mbps 720p.
    t.play(20, 0.5, Some(9.0));
    assert_eq!(t.wait(2_000), step("4m", NET));
}

#[test]
fn the_original_doesnt_move_for_two_short_waits_or_slow_delivery() {
    let mut t = blank_hd();
    assert!(t.a.patient());
    assert_eq!(t.wait(800), None);
    assert_eq!(t.play(30, 1.2, None), None);
    assert_eq!(t.wait(800), None);
    assert_eq!(t.play(60, 0.8, Some(9.0)), None);
    assert_eq!(t.a.playing, "original");
}

#[test]
fn never_goes_back_up_to_another_stream_however_fast_the_network() {
    let mut t = blank_hd();
    t.a.bandwidth(t.now(), 16_000.0);
    t.play(20, 0.5, Some(9.0));
    assert_eq!(to(t.wait(2_000)).as_deref(), Some("4m"));
    // A conversion whose bitrate can't change (an older server): it stays.
    t.a.began(None);
    assert_eq!(t.network(3_600, 80_000.0), None);
    assert_eq!(t.a.playing, "4m");
}

#[test]
fn plays_on_through_dropped_frames_and_starts_the_next_title_a_step_lower_once() {
    let mut t = blank(None, f(3840, 2160, 40.0));
    t.a.struggled();
    // 40 × 0.75 = 30 → 30 Mbps 4K next time.
    assert_eq!(t.told(), [Some(30_000.0)]);
    assert_eq!(t.mem.value().map(|m| m.kbps), Some(30_000.0));
    t.a.struggled();
    assert_eq!(t.told(), [Some(30_000.0)]);
    assert_eq!(t.a.playing, "original");
}

/// Started at 8 Mbps 1080p by the network's memory, its stream's bitrate changing as it is sent, at 3 Mbps at least.
fn live() -> Auto {
    let mut t = blank(Some(Some(Remembered { kbps: 9_000.0, at: WALL })), f(1920, 1080, 20.0));
    assert_eq!(t.a.playing, "8m");
    t.a.began(Some(pley_player::autoquality::LiveStream { top: 8_000.0, floor: 3_000.0 }));
    t
}

fn stream(top: f64, floor: f64) -> Option<pley_player::autoquality::LiveStream> {
    Some(pley_player::autoquality::LiveStream { top, floor })
}

#[test]
fn a_live_bitrate_stream_steps_the_bitrate_down_not_the_stream() {
    let mut t = live();
    // 8 × 0.7 × 0.75 = 4.2 → 4 Mbps, still 1080p.
    t.play(20, 0.7, Some(9.0));
    assert_eq!(t.wait(2_000), live_step("8m", NET, 4_000.0));
    assert_eq!(
        (t.a.playing.as_str(), t.a.live_kbps, t.a.lowered, t.a.live_lowered()),
        ("8m", Some(4_000.0), NET, true)
    );
    assert_eq!(t.a.playing_kbps(), Some(4_000.0));
    // The network's memory keeps what it carries, for the next title.
    assert_eq!(t.told(), [Some(4_200.0)]);
}

#[test]
fn a_live_bitrate_stream_keeps_its_floor() {
    let mut t = live();
    // 8 × 0.3 × 0.75 = 1.8, but 1080p keeps 3.
    t.play(20, 0.3, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(3_000.0));
    t.pass(25_000);
    t.play(20, 0.3, Some(9.0));
    assert_eq!(t.wait(3_000), None);
    assert_eq!((t.a.playing.as_str(), t.a.live_kbps), ("8m", Some(3_000.0)));

    let mut uhd = blank(Some(Some(Remembered { kbps: 16_000.0, at: WALL })), f(3840, 2160, 60.0));
    assert_eq!(uhd.a.playing, "15m");
    uhd.a.began(stream(15_000.0, 8_000.0));
    uhd.play(20, 0.3, Some(9.0));
    assert_eq!(bitrate(uhd.wait(2_000)), Some(8_000.0));
}

#[test]
fn a_live_bitrate_stream_steps_on_short_waits_and_slow_delivery_too() {
    let mut one = live();
    assert!(!one.a.patient());
    assert_eq!(one.wait(800), None);
    assert_eq!(one.play(30, 1.2, None), None);
    // Arriving as fast as it plays: 8 × 1 × 0.75 = 6.
    assert_eq!(bitrate(one.wait(800)), Some(6_000.0));

    let mut slow = live();
    // 8 × 0.8 × 0.75 = 4.8 → 4.
    assert_eq!(slow.play(40, 0.8, Some(9.0)), live_step("8m", NET, 4_000.0));
}

#[test]
fn a_live_bitrate_stream_steps_for_a_decoder_falling_behind_without_telling_the_memory() {
    let mut t = live();
    t.pass(1_000);
    t.a.decoding();
    assert_eq!(t.a.check(t.now()), live_step("8m", DEVICE, 6_000.0));
    assert!(t.told().is_empty());
}

#[test]
fn a_live_bitrate_stream_steps_back_up_after_calm_minutes() {
    let mut t = live();
    t.play(20, 0.3, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(3_000.0));
    // 5 minutes after the step down, the network carries 1.6 × 4 Mbps (4 Mbps 720p's bitrate, at 1080p here).
    assert_eq!(t.network(304, 50_000.0), live_step("8m", None, 4_000.0));
    assert_eq!(t.network(176, 50_000.0), None);
    assert_eq!(t.network(8, 50_000.0), live_step("8m", None, 8_000.0));
    assert!(!t.a.live_lowered());
    // Its own bitrate is as high as it goes: never another stream.
    assert_eq!(t.network(3_600, 80_000.0), None);
    // Each step told as the network's ceiling: the next title starts there.
    assert_eq!(t.told(), [Some(1_800.0), Some(4_000.0), Some(8_000.0)]);
}

#[test]
fn a_live_bitrate_stream_settles_and_doubles_the_next_calm() {
    let mut t = live();
    t.play(20, 0.7, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(4_000.0));
    assert_eq!(bitrate(t.network(304, 50_000.0)), Some(8_000.0));
    // Two minutes on, 8 Mbps doesn't hold after all: 8 × 0.8 × 0.75 = 4.8 → 4.
    t.network(120, 50_000.0);
    t.play(20, 0.8, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(4_000.0));
    assert_eq!(t.network(3_600, 80_000.0), None);
    assert_eq!(t.a.live_kbps, Some(4_000.0));
    // Later 4 Mbps falters too: 3, with 4 still open, after 6 calm minutes rather than 3.
    t.play(20, 0.5, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(3_000.0));
    assert_eq!(t.network(352, 50_000.0), None);
    assert_eq!(bitrate(t.network(16, 50_000.0)), Some(4_000.0));
    assert_eq!(t.network(3_600, 80_000.0), None);
    assert_eq!(t.a.live_kbps, Some(4_000.0));
}

#[test]
fn the_original_the_server_converts_steps_its_bitrate_and_is_the_original_again_at_its_own() {
    let mut t = blank(None, f(3840, 2160, 60.0));
    t.a.set_original_kbps(Some(30_000.0));
    t.a.began(stream(30_000.0, 8_000.0));
    // 30 × 0.5 × 0.75 = 11.25 → 11 Mbps 4K.
    t.play(20, 0.5, Some(9.0));
    assert_eq!(t.wait(2_000), live_step("original", NET, 11_000.0));
    assert_eq!((t.a.lowered, t.a.live_lowered()), (NET, true));
    // Up through the file's qualities' bitrates: 12, 15, then its own 30.
    assert_eq!(bitrate(t.network(304, 80_000.0)), Some(12_000.0));
    assert_eq!(bitrate(t.network(184, 80_000.0)), Some(15_000.0));
    assert_eq!(bitrate(t.network(184, 80_000.0)), Some(30_000.0));
    assert_eq!((t.a.playing.as_str(), t.a.lowered, t.a.live_lowered()), ("original", None, false));
    assert_eq!(t.told(), [Some(11_250.0), Some(12_000.0), Some(15_000.0), None]);
}

#[test]
fn a_new_stream_of_the_same_quality_goes_on_at_the_bitrate_it_came_to() {
    let mut t = live();
    t.play(20, 0.7, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(4_000.0));
    t.a.began(stream(8_000.0, 3_000.0));
    assert_eq!((t.a.live_kbps, t.a.live_lowered()), (Some(4_000.0), true));
    t.a.began(stream(12_000.0, 3_000.0));
    assert_eq!((t.a.live_kbps, t.a.live_lowered()), (Some(12_000.0), false));
    t.a.began(None);
    assert_eq!((t.a.live_kbps, t.a.patient()), (None, true));
}

#[test]
fn goes_by_the_bitrate_the_server_says() {
    let mut t = live();
    t.play(20, 0.7, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(4_000.0));
    t.a.live_at(4_000.0, 4_000.0);
    assert_eq!(bitrate(t.network(304, 50_000.0)), Some(8_000.0));
    t.a.live_at(6_000.0, 8_000.0);
    assert_eq!((t.a.live_kbps, t.a.live_top(), t.a.live_lowered()), (Some(6_000.0), Some(6_000.0), false));
    assert_eq!(t.network(3_600, 80_000.0), None);
}

#[test]
fn goes_on_as_it_was_when_the_server_doesnt_change_the_bitrate_and_stops_asking_when_it_cant() {
    let mut t = live();
    t.play(20, 0.7, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(4_000.0));
    t.a.live_refused(false);
    assert_eq!((t.a.live_kbps, t.a.lowered, t.a.live_lowered()), (Some(8_000.0), NET, false));
    t.pass(25_000);
    t.play(20, 0.7, Some(9.0));
    assert_eq!(bitrate(t.wait(2_000)), Some(4_000.0));
    // An older transcoder: a stream whose bitrate can't change, which short waits don't move.
    t.a.live_refused(true);
    assert_eq!((t.a.live_kbps, t.a.patient()), (None, true));
    t.pass(25_000);
    assert_eq!(t.wait(800), None);
    assert_eq!(t.wait(800), None);
    assert_eq!(t.a.playing, "8m");
}

/// RES/tv-player-logic.md §7.4: a 1080p file transcoded at `8m` on a
/// `live_bitrate` session (top 8000, floor 3000); a 2 s wait with delivery
/// 0.6 gives a ceiling of 3600, so 3 Mbps, remembered as 3600.
#[test]
fn the_worked_example_of_a_1080p_file_at_8_mbps() {
    let mut t = live();
    t.play(20, 0.6, Some(9.0));
    assert_eq!(t.wait(2_000), live_step("8m", NET, 3_000.0));
    assert_eq!(t.mem.value().map(|m| m.kbps), Some(3_600.0));
    assert_eq!(t.told(), [Some(3_600.0)]);
}
