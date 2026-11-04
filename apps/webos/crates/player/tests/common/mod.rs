//! What the controller tests share: the TypeScript tests' fixtures, a
//! pretend server, and a world that moves time for the controller and
//! `SimEngine` together.
#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use pley_base::Waker;
use pley_core::api::LiveBrowse;
use pley_core::http::{Method, NetError};
use pley_core::media::AudioTrack;
use pley_core::model::*;
use pley_core::task::{Cancel, Tasks};
use pley_core::{ApiError, CapsReport};
use pley_player::Reporter;
use pley_player::autoquality::{AutoMemory, HeldMemory, Remembered};
use pley_player::controller::{PlaybackController, TrackChoice};
use pley_player::env::{PlayerEnv, QualityPrefs};
use pley_player::network::NetworkMemory;
use pley_player::plan::{Choices, PlayRequest};
use pley_player::quality::Quality;
use pley_player::server::{PlayerServer, SubtitleAnswer};
use pley_player::sim::{SIM_LOAD_MS, SimClock, SimEngine, SimHandle};

pub const SERVER: &str = "http://nas:7539";
/// The wall clock when the test's clock is at 0.
pub const WALL: i64 = 1_000_000_000_000;

// ---- fixtures

pub fn stream(
    index: i64,
    kind: &str,
    codec: &str,
    language: Option<&str>,
    default: bool,
    more: impl FnOnce(&mut MediaStream),
) -> MediaStream {
    let mut s = MediaStream {
        index,
        kind: StreamKind::from_wire(kind),
        codec: codec.into(),
        language: language.map(str::to_string),
        default,
        ..MediaStream::default()
    };
    more(&mut s);
    s
}

pub fn file() -> MediaFile {
    MediaFile {
        id: 7,
        path: "/media/Film (2024).mkv".into(),
        size: 40_000_000_000,
        container: Some("mkv".into()),
        duration_ms: Some(7_200_000),
        bitrate: Some(40_000_000),
        width: Some(3840),
        height: Some(2160),
        video_codec: Some("hevc".into()),
        audio_codec: Some("eac3".into()),
        streams: vec![
            stream(0, "video", "hevc", None, true, |s| {
                s.width = Some(3840);
                s.height = Some(2160);
                s.bit_depth = Some(10);
                s.profile = Some("Main 10".into());
                s.dynamic_range = Some(DynamicRange::Hdr10);
                s.level = Some(153);
            }),
            stream(1, "audio", "eac3", Some("eng"), true, |s| s.channels = Some(6)),
            stream(2, "audio", "truehd", Some("eng"), false, |s| s.channels = Some(8)),
            stream(3, "subtitle", "hdmv_pgs_subtitle", Some("eng"), false, |s| s.text_based = Some(false)),
            stream(4, "subtitle", "subrip", Some("eng"), false, |s| s.text_based = Some(true)),
        ],
        ..MediaFile::default()
    }
}

pub fn dv_file(profile: i64, container: &str) -> MediaFile {
    let mut f = file();
    f.container = Some(container.into());
    f.streams[0] = stream(0, "video", "hevc", None, true, |s| {
        s.width = Some(3840);
        s.height = Some(2160);
        s.bit_depth = Some(10);
        s.profile = Some("Main 10".into());
        s.dynamic_range = Some(DynamicRange::DolbyVision);
        s.dv_profile = Some(profile);
        s.dv_bl_compat = Some(if profile == 8 { 1 } else { 0 });
        s.level = Some(153);
    });
    f
}

pub fn item(f: MediaFile) -> ItemDetail {
    ItemDetail {
        summary: ItemSummary {
            id: 70,
            library_id: 1,
            kind: ItemKind::Movie,
            title: "Film".into(),
            sort_title: "film".into(),
            year: Some(2024),
            user_state: UserState { position_ms: 600_000, ..UserState::default() },
            duration_ms: Some(7_200_000),
            ..ItemSummary::default()
        },
        files: vec![f],
        ..ItemDetail::default()
    }
}

/// An LG OLED of 2023 (webOS 23): HDR10, HLG, Dolby Vision 5 and 8, Dolby audio to the receiver.
pub fn lg() -> ClientCaps {
    let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    ClientCaps {
        containers: words(&["mkv", "mp4", "ts"]),
        video_codecs: words(&["h264", "hevc", "av1"]),
        audio_codecs: words(&["aac", "ac3", "eac3"]),
        hdr: true,
        dolby_vision: true,
        caps_version: Some(2),
        video: vec![VideoDecoder {
            codec: "hevc".into(),
            hardware: true,
            profiles: Some(words(&["Main", "Main 10"])),
            max_width: Some(4096),
            max_height: Some(2160),
            max_fps: Some(60.0),
            max_bit_depth: Some(10),
            max_level: Some(153),
        }],
        hdr_types: vec![DynamicRange::Hdr10, DynamicRange::Hlg, DynamicRange::DolbyVision],
        dolby_vision_profiles: vec![
            DolbyVisionDecoder { profile: 5, max_level: None },
            DolbyVisionDecoder { profile: 8, max_level: None },
        ],
        audio_decode: words(&["aac", "ac3", "eac3"]),
        audio_passthrough: words(&["ac3", "eac3"]),
        audio_channels: 8,
        subtitle_codecs: words(&["subrip", "hdmv_pgs_subtitle"]),
        audio_track_switching: true,
        picture_subtitles_sideload: true,
        hls: HlsCaps { master_playlist: true, segments: words(&["ts", "fmp4"]), variants: None },
        display: None,
        device: Some(DeviceCaps {
            form: "tv".into(),
            tier: Some("high".into()),
            fire_tv: false,
            model: Some("OLED65C3PUA".into()),
        }),
    }
}

pub fn choices(more: impl FnOnce(&mut Choices)) -> Choices {
    let mut c = Choices {
        file: file(),
        start_ms: 600_000,
        audio: None,
        subtitle: None,
        quality: Quality::original(),
        mode: PlaybackMode::Auto,
        streaming: false,
        sideload: true,
    };
    more(&mut c);
    c
}

pub fn info(more: impl FnOnce(&mut PlaybackInfo)) -> PlaybackInfo {
    let mut i = PlaybackInfo {
        session_id: "s1".into(),
        method: PlaybackMethod::Direct,
        url: "/api/media/pley_mk_KEY/file.mkv".into(),
        hls: false,
        file_id: 7,
        duration_ms: 7_200_000,
        audio_index: Some(1),
        ..PlaybackInfo::default()
    };
    more(&mut i);
    i
}

/// The indexes of the selected rows.
pub fn selected(rows: &[TrackChoice]) -> Vec<Option<i64>> {
    rows.iter().filter(|r| r.selected).map(|r| r.index).collect()
}

/// A stream in two sizes, each in 4 s segments over the film's two hours.
pub fn playlist_for(url: &str) -> String {
    if url.ends_with("master.m3u8") {
        return [
            "#EXTM3U",
            "#EXT-X-STREAM-INF:BANDWIDTH=4400000,RESOLUTION=1280x720",
            "v0/index.m3u8",
            "#EXT-X-STREAM-INF:BANDWIDTH=2200000,RESOLUTION=854x480",
            "v1/index.m3u8",
        ]
        .join("\n");
    }
    let mut lines = vec!["#EXTM3U".to_string(), "#EXT-X-TARGETDURATION:4".to_string()];
    for n in 0..1800 {
        lines.push("#EXTINF:4.000,".into());
        lines.push(format!("{n}.ts"));
    }
    lines.push("#EXT-X-ENDLIST".into());
    lines.join("\n")
}

/// A refusal in Pley's words, as the server's JSON reads.
fn refused(status: u16, code: &str, message: &str) -> ApiError {
    let body = serde_json::json!({"code": code, "message": message});
    ApiError::Status { status, code: code.into(), message: message.into(), body: Some(body) }
}

// ---- the pretend server

type Answer = Box<dyn Fn(&PlaybackRequest) -> PlaybackInfo + Send + Sync>;
type Search = (i64, Option<i64>, Vec<String>);
type AddAnswer = Box<dyn Fn(&str) -> SubtitleTrack + Send + Sync>;

pub struct Fake {
    /// The server's address as playback keys its memories (`http://nas:7539` unless a test says).
    name: String,
    detail: ItemDetail,
    answer: Answer,
    calls: Mutex<Vec<String>>,
    requests: Mutex<Vec<PlaybackRequest>>,
    /// Every progress report, whole.
    reports: Mutex<Vec<ProgressReport>>,
    saved: Mutex<Vec<(AudioChoice, Option<i64>)>>,
    searched: Mutex<Vec<Search>>,
    /// The subtitle files asked for, by address.
    subtitle_asks: Mutex<Vec<String>>,
    on_add: Mutex<Option<AddAnswer>>,
    next_item: Mutex<Option<ItemSummary>>,
    /// What `GET /api/playback/network` answers; none: 404, an older server.
    network_answer: Mutex<Option<NetworkAnswer>>,
    told: Mutex<Vec<NetworkReport>>,
    /// What the speed test reads; none: it fails.
    speed: Mutex<Option<SpeedSample>>,
    speed_tests: AtomicUsize,
    /// Nothing answers (the Wi-Fi dropped, the server restarts).
    down: AtomicBool,
    /// No transcode free for a session asked for beside another (503).
    busy: AtomicBool,
    restarting: AtomicBool,
    refuse_bitrates: AtomicBool,
    fail_heads: AtomicBool,
    hold_heads: AtomicBool,
    released: Mutex<u32>,
    release: Condvar,
    /// A video on a drive: no server converts it (`409 local_file`).
    local: AtomicBool,
    /// What only a server answers, asked: `status`, `trickplay`, `next`.
    questions: Mutex<Vec<String>>,
}

impl Fake {
    pub fn new(
        detail: ItemDetail,
        answer: impl Fn(&PlaybackRequest) -> PlaybackInfo + Send + Sync + 'static,
    ) -> Arc<Fake> {
        Fake::named(SERVER, detail, answer)
    }

    /// A server known as `name` (tests sharing what is heard of servers keep apart so).
    pub fn named(
        name: &str,
        detail: ItemDetail,
        answer: impl Fn(&PlaybackRequest) -> PlaybackInfo + Send + Sync + 'static,
    ) -> Arc<Fake> {
        Arc::new(Fake {
            name: name.to_string(),
            detail,
            answer: Box::new(answer),
            calls: Mutex::default(),
            requests: Mutex::default(),
            reports: Mutex::default(),
            saved: Mutex::default(),
            searched: Mutex::default(),
            subtitle_asks: Mutex::default(),
            on_add: Mutex::default(),
            next_item: Mutex::default(),
            network_answer: Mutex::default(),
            told: Mutex::default(),
            speed: Mutex::default(),
            speed_tests: AtomicUsize::new(0),
            down: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            restarting: AtomicBool::new(false),
            refuse_bitrates: AtomicBool::new(false),
            fail_heads: AtomicBool::new(false),
            hold_heads: AtomicBool::new(false),
            released: Mutex::new(0),
            release: Condvar::new(),
            local: AtomicBool::new(false),
            questions: Mutex::default(),
        })
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    pub fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }

    pub fn requests(&self) -> Vec<PlaybackRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn modes(&self) -> Vec<PlaybackMode> {
        self.requests().iter().map(|r| r.mode).collect()
    }

    /// Every progress report, in order.
    pub fn reports(&self) -> Vec<ProgressReport> {
        self.reports.lock().unwrap().clone()
    }

    pub fn saved(&self) -> Vec<(AudioChoice, Option<i64>)> {
        self.saved.lock().unwrap().clone()
    }

    pub fn searched(&self) -> Vec<Search> {
        self.searched.lock().unwrap().clone()
    }

    pub fn subtitle_asks(&self) -> Vec<String> {
        self.subtitle_asks.lock().unwrap().clone()
    }

    pub fn bitrates(&self) -> Vec<String> {
        self.calls().into_iter().filter(|c| c.starts_with("bitrate ")).collect()
    }

    pub fn set_down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }

    /// The server answers, but its transcoder restarts: new sessions get `503 unavailable`.
    pub fn set_restarting(&self, restarting: bool) {
        self.restarting.store(restarting, Ordering::SeqCst);
    }

    pub fn set_busy(&self, busy: bool) {
        self.busy.store(busy, Ordering::SeqCst);
    }

    pub fn refuse_bitrates(&self) {
        self.refuse_bitrates.store(true, Ordering::SeqCst);
    }

    pub fn fail_heads(&self) {
        self.fail_heads.store(true, Ordering::SeqCst);
    }

    /// Segments answer only when the test says.
    pub fn hold_segments(&self) {
        self.hold_heads.store(true, Ordering::SeqCst);
    }

    /// The segment asked for is made.
    pub fn release_segment(&self) {
        *self.released.lock().unwrap() += 1;
        self.release.notify_all();
    }

    /// What `GET /api/playback/network` answers.
    pub fn set_network(&self, answer: Option<NetworkAnswer>) {
        *self.network_answer.lock().unwrap() = answer;
    }

    /// What the speed test reads.
    pub fn set_speed(&self, sample: Option<SpeedSample>) {
        *self.speed.lock().unwrap() = sample;
    }

    /// The network reports told, in order.
    pub fn told(&self) -> Vec<NetworkReport> {
        self.told.lock().unwrap().clone()
    }

    /// Speed tests run.
    pub fn speed_tests(&self) -> usize {
        self.speed_tests.load(Ordering::SeqCst)
    }

    /// What `GET /api/items/{id}/next` answers.
    pub fn set_next(&self, next: Option<ItemSummary>) {
        *self.next_item.lock().unwrap() = next;
    }

    /// Plays as a video on a drive does: as it is, nothing converted.
    pub fn set_local(&self) {
        self.local.store(true, Ordering::SeqCst);
    }

    /// What only a server answers that was asked, in order.
    pub fn questions(&self) -> Vec<String> {
        self.questions.lock().unwrap().clone()
    }

    fn question(&self, what: &str) {
        self.questions.lock().unwrap().push(what.to_string());
    }

    pub fn on_add(&self, f: impl Fn(&str) -> SubtitleTrack + Send + Sync + 'static) {
        *self.on_add.lock().unwrap() = Some(Box::new(f));
    }

    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn answered<T>(&self, value: impl FnOnce() -> Result<T, ApiError>) -> Result<T, ApiError> {
        if self.down.load(Ordering::SeqCst) { Err(ApiError::Net(NetError::Refused)) } else { value() }
    }

    fn held_segment(&self) -> Result<String, ApiError> {
        let cancel = Cancel::current();
        let mut released = self.released.lock().unwrap();
        loop {
            // A request let go never takes the segment made for the next one.
            if cancel.as_ref().is_some_and(Cancel::is_cancelled) {
                return Err(ApiError::Net(NetError::Cancelled));
            }
            if *released > 0 {
                *released -= 1;
                return Ok(String::new());
            }
            released = self.release.wait_timeout(released, Duration::from_millis(2)).unwrap().0;
        }
    }
}

impl PlayerServer for Fake {
    fn server(&self) -> &str {
        &self.name
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http") { path.to_string() } else { format!("{SERVER}{path}") }
    }

    fn item(&self, id: i64) -> Result<ItemDetail, ApiError> {
        let mut d = self.detail.clone();
        d.summary.id = id;
        Ok(d)
    }

    fn next(&self, _id: i64) -> Result<Option<ItemSummary>, ApiError> {
        self.question("next");
        Ok(self.next_item.lock().unwrap().clone())
    }

    fn playlist(&self, _id: i64) -> Result<PlaylistDetail, ApiError> {
        Err(refused(404, "not_found", "no"))
    }

    fn collection(&self, _id: i64) -> Result<CollectionDetail, ApiError> {
        Err(refused(404, "not_found", "no"))
    }

    fn play(&self, req: &PlaybackRequest) -> Result<PlaybackInfo, ApiError> {
        self.answered(|| {
            self.requests.lock().unwrap().push(req.clone());
            self.log(format!("start {}", req.mode.as_str()));
            if self.restarting.load(Ordering::SeqCst) {
                return Err(refused(503, "unavailable", "the transcoder isn't running"));
            }
            if self.busy.load(Ordering::SeqCst) && req.keep_previous {
                return Err(refused(503, "unavailable", "no transcode free"));
            }
            if self.local.load(Ordering::SeqCst) && matches!(req.mode, PlaybackMode::Remux | PlaybackMode::Transcode) {
                return Err(refused(409, "local_file", "nothing converts it"));
            }
            Ok((self.answer)(req))
        })
    }

    fn progress(&self, _item_id: i64, report: &ProgressReport) -> Result<(), ApiError> {
        self.log(format!("progress {} {}", report.state.as_str(), report.position_ms));
        self.reports.lock().unwrap().push(report.clone());
        Ok(())
    }

    fn stop_session(&self, session: &str) -> Result<(), ApiError> {
        self.log(format!("delete {session}"));
        Ok(())
    }

    fn session_status(&self, _session: &str, _timeout: Duration) -> Result<SessionStatus, ApiError> {
        self.question("status");
        self.answered(|| Ok(SessionStatus { session_id: "s1".into(), method: PlaybackMethod::Direct, transcode: None }))
    }

    fn set_bitrate(&self, session: &str, kbps: u32) -> Result<BitrateAnswer, ApiError> {
        self.answered(|| {
            self.log(format!("bitrate {session} {kbps}"));
            if self.refuse_bitrates.load(Ordering::SeqCst) {
                return Err(refused(400, "bad_request", "this stream's bitrate can't change as it plays"));
            }
            Ok(BitrateAnswer { bitrate: kbps })
        })
    }

    fn set_streams(&self, _item_id: i64, choice: &StreamChoice) -> Result<(), ApiError> {
        self.saved.lock().unwrap().push((choice.audio, choice.subtitle_index));
        Ok(())
    }

    fn reachable(&self, _timeout: Duration) -> Result<ServerInfo, ApiError> {
        self.answered(|| Ok(ServerInfo::default()))
    }

    fn media(&self, method: Method, url: &str, _timeout: Duration) -> Result<String, ApiError> {
        let short = url.replace("http://nas:7539/api/media/", "");
        if method == Method::Head {
            self.log(format!("HEAD {short}"));
            if self.fail_heads.load(Ordering::SeqCst) {
                return Err(refused(404, "not_found", "404"));
            }
            if self.hold_heads.load(Ordering::SeqCst) {
                return self.held_segment();
            }
            return Ok(String::new());
        }
        self.log(format!("GET {short}"));
        Ok(playlist_for(url))
    }

    fn network(&self, _client_id: &str, timeout: Duration) -> Result<NetworkAnswer, ApiError> {
        assert_eq!(timeout, Duration::from_millis(1_500));
        self.network_answer.lock().unwrap().clone().ok_or_else(|| refused(404, "not_found", "no"))
    }

    fn tell_network(
        &self,
        _client_id: &str,
        report: &NetworkReport,
        _timeout: Duration,
    ) -> Result<NetworkAnswer, ApiError> {
        self.told.lock().unwrap().push(report.clone());
        let ceiling = match report {
            NetworkReport::Probe { kbps, .. } => Some((kbps * 0.7).round() as i64),
            NetworkReport::Ceiling(_) => None,
        };
        Ok(NetworkAnswer { ceiling_kbps: ceiling, since: Some(7), ..NetworkAnswer::default() })
    }

    fn speed_test(&self, bytes: u64) -> Result<SpeedSample, ApiError> {
        assert_eq!(bytes, 32 * 1024 * 1024);
        self.speed_tests.fetch_add(1, Ordering::SeqCst);
        (*self.speed.lock().unwrap()).ok_or(ApiError::Net(NetError::Refused))
    }

    fn trickplay(&self, _file_id: i64) -> Result<TrickplayInfo, ApiError> {
        self.question("trickplay");
        Err(refused(404, "not_found", "not ready"))
    }

    fn subtitles(&self, url: &str, _accept: &str, _timeout: Duration) -> Result<SubtitleAnswer, ApiError> {
        self.subtitle_asks.lock().unwrap().push(url.to_string());
        // A picture track still being read out of its file, with nothing around there yet.
        if url.contains(".mks") {
            return Ok(SubtitleAnswer { status: 200, body: Vec::new(), complete: false });
        }
        Ok(SubtitleAnswer { status: 200, body: b"WEBVTT\n\n".to_vec(), complete: true })
    }

    fn search_subtitles(
        &self,
        item_id: i64,
        file_id: Option<i64>,
        languages: &[String],
    ) -> Result<Vec<SubtitleResult>, ApiError> {
        self.searched.lock().unwrap().push((item_id, file_id, languages.to_vec()));
        Ok(Vec::new())
    }

    fn add_subtitle(&self, _item_id: i64, add: &AddSubtitle) -> Result<SubtitleTrack, ApiError> {
        match &*self.on_add.lock().unwrap() {
            Some(f) => Ok(f(&add.result)),
            None => Err(refused(404, "not_found", "no")),
        }
    }

    fn watch_channel(&self, _channel: i64, _req: &WatchRequest) -> Result<LiveWatching, ApiError> {
        Err(refused(404, "not_found", "no"))
    }

    fn live_plan(&self, _session: &str) -> Result<Option<LivePlan>, ApiError> {
        Ok(None)
    }

    fn leave_channel(&self, _session: &str) -> Result<(), ApiError> {
        Ok(())
    }

    fn live_channel(&self, _id: i64) -> Result<LiveChannel, ApiError> {
        Err(refused(404, "not_found", "no"))
    }

    fn live_groups(&self) -> Result<LiveGroups, ApiError> {
        Ok(LiveGroups::default())
    }

    fn live_browse(&self, _b: &LiveBrowse) -> Result<LiveChannelPage, ApiError> {
        Ok(LiveChannelPage::default())
    }

    fn live_guide(&self, from: i64, to: i64, _ids: &[i64]) -> Result<LiveGuide, ApiError> {
        Ok(LiveGuide { from, to, ..LiveGuide::default() })
    }

    fn set_live_favorite(&self, _channel: i64, _favorite: bool) -> Result<(), ApiError> {
        Ok(())
    }

    fn local(&self) -> bool {
        self.local.load(Ordering::SeqCst)
    }
}

// ---- memories

#[derive(Clone, Default)]
pub struct Memory(pub Arc<Mutex<HeldMemory>>);

impl Memory {
    pub fn with(kbps: f64, at: i64) -> Memory {
        Memory(Arc::new(Mutex::new(HeldMemory(Some(Remembered { kbps, at })))))
    }

    pub fn value(&self) -> Option<Remembered> {
        self.0.lock().unwrap().get()
    }
}

/// The server's memory of networks: where Automatic may start; what it was told.
pub struct FakeNetwork {
    known: Option<Option<Remembered>>,
    told: Mutex<Vec<Option<f64>>>,
    asked: AtomicUsize,
}

impl FakeNetwork {
    pub fn new(known: Option<Option<Remembered>>) -> Arc<FakeNetwork> {
        Arc::new(FakeNetwork { known, told: Mutex::default(), asked: AtomicUsize::new(0) })
    }

    pub fn told(&self) -> Vec<Option<f64>> {
        self.told.lock().unwrap().clone()
    }

    pub fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

impl NetworkMemory for FakeNetwork {
    fn start(&self) -> Option<Option<Remembered>> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.known
    }

    fn report(&self, kbps: Option<f64>) {
        self.told.lock().unwrap().push(kbps);
    }
}

/// The quality remembered, and every pick.
pub struct Picked {
    remembered: Mutex<Option<String>>,
    picks: Mutex<Vec<String>>,
}

impl Picked {
    pub fn new(id: &str) -> Picked {
        Picked { remembered: Mutex::new(Some(id.into())), picks: Mutex::default() }
    }
}

impl QualityPrefs for Picked {
    fn remembered(&self) -> Option<String> {
        self.remembered.lock().unwrap().clone()
    }

    fn remember(&self, id: &str) {
        self.picks.lock().unwrap().push(id.into());
        *self.remembered.lock().unwrap() = Some(id.into());
    }
}

// ---- the world

/// A controller, its `SimEngine` and the pretend server, on a clock the test moves.
pub struct World {
    pub fake: Arc<Fake>,
    pub clock: SimClock,
    pub sim: SimHandle,
    pub c: PlaybackController,
    pub now: i64,
    /// Where what the engine has loaded ends (ms), for the Automatic tests.
    pub buffered_end: f64,
    /// Dropped and total frames, when counted.
    pub frames: Option<(u64, u64)>,
    /// Every notice shown, in order.
    pub notices: Vec<String>,
    seen_notice: u64,
    picked: Option<Arc<Picked>>,
}

impl World {
    pub fn new(fake: Arc<Fake>, tweak: impl FnOnce(&mut PlayerEnv)) -> World {
        World::build(fake, Vec::new(), tweak)
    }

    /// A TV that shows Dolby Vision only from `containers`.
    pub fn with_dv(fake: Arc<Fake>, containers: &[&str], tweak: impl FnOnce(&mut PlayerEnv)) -> World {
        World::build(fake, containers.iter().map(|s| s.to_string()).collect(), tweak)
    }

    fn build(fake: Arc<Fake>, dv: Vec<String>, tweak: impl FnOnce(&mut PlayerEnv)) -> World {
        let clock = SimClock::manual();
        let (engine, sim) = SimEngine::new(clock.clone());
        // The pipeline lists the file's audio tracks in its order.
        let tracks = fake.detail.files[0]
            .streams
            .iter()
            .filter(|s| s.kind == StreamKind::Audio)
            .enumerate()
            .map(|(ordinal, s)| AudioTrack {
                ordinal,
                language: s.language.clone(),
                codec: Some(s.codec.clone()),
                channels: None,
            })
            .collect();
        sim.set_tracks(tracks);
        let caps = Arc::new(move || Some(CapsReport { caps: lg(), lines: Vec::new(), dv_containers: dv.clone() }));
        let server: Arc<dyn PlayerServer> = fake.clone();
        let mut env = PlayerEnv::new(server, Tasks::with_threads(2, Waker::noop()), Reporter::new(), caps, "webos-abc");
        env.quality = Arc::new(Picked::new("original"));
        let wall_clock = clock.clone();
        env.wall = Arc::new(move || WALL + wall_clock.now());
        tweak(&mut env);
        let c = PlaybackController::new(PlayRequest::item(70), env, Box::new(engine), 0);
        World {
            fake,
            clock,
            sim,
            c,
            now: 0,
            buffered_end: 0.0,
            frames: None,
            notices: Vec::new(),
            seen_notice: 0,
            picked: None,
        }
    }

    /// The qualities picked, in order.
    pub fn picked(&self) -> Vec<String> {
        self.picked.as_ref().map(|p| p.picks.lock().unwrap().clone()).unwrap_or_default()
    }

    /// Starts, and lets the first session be asked for.
    pub fn start(&mut self) {
        self.c.start();
        self.settle();
    }

    /// Time enough for the engine to open what it loaded: its picture shows.
    pub fn open(&mut self) {
        self.advance(SIM_LOAD_MS);
    }

    fn tick(&mut self) {
        self.c.tick(self.now);
        let s = self.c.state();
        if s.notice_id != self.seen_notice {
            self.seen_notice = s.notice_id;
            if let Some(n) = s.notice.clone()
                && self.notices.last() != Some(&n)
            {
                self.notices.push(n);
            }
        }
    }

    /// Lets every answer come in, without time passing.
    pub fn settle(&mut self) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            self.tick();
            // A stream made ready beside the one playing is waited for, unless its segments are held on purpose.
            let warming = self.c.warming() && !self.fake.hold_heads.load(Ordering::SeqCst);
            if !self.c.busy() && !warming {
                break;
            }
            assert!(Instant::now() < until, "a server call never came back");
            thread::sleep(Duration::from_millis(1));
        }
        self.tick();
    }

    /// Ticks without time passing until `done` holds (5 s of real time at most).
    pub fn until(&mut self, done: impl Fn(&World) -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while !done(self) {
            assert!(Instant::now() < until, "the world never got there");
            self.tick();
            thread::sleep(Duration::from_millis(1));
        }
        self.settle();
    }

    /// Lets `ms` pass, 50 ms at a time.
    pub fn advance(&mut self, ms: i64) {
        let end = self.now + ms;
        while self.now < end {
            self.now = (self.now + 50).min(end);
            self.clock.set(self.now);
            self.settle();
        }
    }

    /// `seconds` of playback, the video arriving at `delivery` seconds a second.
    pub fn play(&mut self, seconds: u32, delivery: f64) {
        for _ in 0..seconds * 2 {
            self.step(delivery);
        }
    }

    fn step(&mut self, delivery: f64) {
        self.buffered_end += 500.0 * delivery;
        let ahead = (self.buffered_end - self.sim.position() as f64).max(0.0);
        self.sim.buffered(ahead as u64);
        if let Some(f) = self.frames {
            self.sim.set_frames(Some(f));
        }
        self.advance(500);
    }

    /// The picture stops for want of data for `ms`.
    pub fn wait(&mut self, ms: i64) {
        self.sim.wait(true);
        self.advance(ms);
    }

    /// It plays on.
    pub fn ready(&mut self) {
        self.sim.wait(false);
        self.settle();
    }

    /// `seconds` of playback at 24 frames a second, `dropped` of them dropped as it starts.
    pub fn drop_frames(&mut self, seconds: u32, dropped: u64) {
        let (d, t) = self.frames.unwrap_or((0, 0));
        self.frames = Some((d + dropped, t));
        for _ in 0..seconds * 2 {
            let (d, t) = self.frames.unwrap_or((0, 0));
            self.frames = Some((d, t + 12));
            self.step(1.2);
        }
    }
}

/// A playback in Automatic that has shown its picture; `memory` is the TV's.
pub fn automatic(
    detail: ItemDetail,
    memory: Memory,
    network: Option<Arc<FakeNetwork>>,
    answer: impl Fn(&PlaybackRequest) -> PlaybackInfo + Send + Sync + 'static,
) -> World {
    let picked = Arc::new(Picked::new("auto"));
    let quality = picked.clone();
    let mut w = World::new(Fake::new(detail, answer), move |env| {
        env.quality = quality;
        let shared: Arc<Mutex<dyn AutoMemory>> = memory.0.clone();
        env.auto_memory = Some(shared);
        env.network = network.map(|n| n as Arc<dyn NetworkMemory>);
    });
    w.picked = Some(picked);
    w.start();
    w.open();
    w.buffered_end = w.sim.position() as f64 + 8_000.0;
    w.sim.buffered(8_000);
    w.settle();
    w
}

/// Automatic's hint in the Quality menu.
pub fn auto_hint(w: &World) -> String {
    w.c.state().qualities.iter().find(|q| q.id == "auto").and_then(|q| q.hint.clone()).unwrap_or_default()
}
