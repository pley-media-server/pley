//! The player's server for a video on a drive (the Samsung app's
//! `LocalPlayerSource`): what a Pley server would answer for direct play,
//! from the file itself. Its tracks and chapters come from its head, the
//! subtitle files beside it are read as WebVTT, its own subtitle tracks are
//! read out of it through its Cues, and where it stopped is kept on the TV.
//! What only a server does (converting, seek previews, the next episode,
//! finding subtitles, Live TV) answers as a server without it would.
//!
//! The TV's media pipeline opens the file at a `file://` address. LG's
//! documentation of `com.webos.media` doesn't cover a native app's files;
//! webOS OSE's documents `file://` addresses for `load`
//! (<https://www.webosose.org/docs/reference/ls2-api/com-webos-media/>), and
//! the pipeline runs outside the app's jail, so it reads the TV's own path
//! (<https://www.webosbrew.org/pages/native-development.html>).

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use pley_core::api::LiveBrowse;
use pley_core::caps::{container_of, direct_range_shown, video_of};
use pley_core::http::{Method, encode_component};
use pley_core::model::{
    AddSubtitle, BitrateAnswer, ClientCaps, CollectionDetail, ItemDetail, ItemKind, ItemSummary, LiveChannel,
    LiveChannelPage, LiveGroups, LiveGuide, LivePlan, LiveWatching, MediaFile, NetworkAnswer, NetworkReport,
    PlaybackInfo, PlaybackMethod, PlaybackMode, PlaybackRequest, PlaylistDetail, ProgressReport, ServerInfo,
    SessionStatus, SpeedSample, StreamChoice, StreamKind, SubtitleResult, SubtitleTrack, TrickplayInfo, UserState,
    WatchRequest,
};
use pley_core::{ApiError, CapsReport, Store};
use pley_player::env::CapsProbe;
use pley_player::server::{PlayerServer, SubtitleAnswer};
use pley_subs::{PictureSample, PictureTrack};

use crate::folder::{self, Sidecar};
use crate::{FileReader, LocalFacts, MatroskaTrack, SubtitleBlock, cues, resume};

/// The item id a video on a drive plays as: no server's item is negative.
pub const ITEM_ID: i64 = -1;
/// How long an answer waits for a track being read out of the file before
/// it gives what it has, marked partial.
pub const EXTRACT_WAIT: Duration = Duration::from_secs(1);
/// What the player's offsets and memories are kept under.
const SERVER: &str = "local";
/// The address of a text track of the file's own: `embedded:3`.
const EMBEDDED: &str = "embedded:";
/// Why a conversion can't be had (409 `local_file`), in the language shown: the player shows it.
pub fn nothing_converts() -> &'static str {
    pley_i18n::t!("player.errors.driveCantConvert")
}

/// What the file says of itself, read once.
struct Described {
    detail: ItemDetail,
    /// The head's facts, for reading the file's own subtitle tracks.
    facts: Option<Arc<LocalFacts>>,
    sidecars: Vec<Sidecar>,
}

/// A subtitle track of the file's own being read out of it, on a thread of
/// its own: what is read so far, and whether all of it is.
struct Extraction {
    track: MatroskaTrack,
    /// Pictures (PGS, VobSub, DVB), handed over as they are; else text.
    pictures: bool,
    /// Text: the blocks as they come.
    blocks: Mutex<Vec<SubtitleBlock>>,
    /// Text: the WebVTT last made, and of how many blocks: the player asks again and again
    /// while the track is read, and nothing new is made of what didn't change.
    webvtt: Mutex<Option<(usize, Vec<u8>)>>,
    /// Pictures: each sample made once, handed over in every answer.
    samples: Mutex<Vec<PictureSample>>,
    done: Mutex<bool>,
    finished: Condvar,
}

impl Extraction {
    fn new(track: MatroskaTrack, pictures: bool) -> Extraction {
        Extraction {
            track,
            pictures,
            blocks: Mutex::default(),
            webvtt: Mutex::default(),
            samples: Mutex::default(),
            done: Mutex::new(false),
            finished: Condvar::new(),
        }
    }

    /// Keeps a block read out of the file: its bytes move here, the one copy held.
    fn found(&self, block: SubtitleBlock) {
        if self.pictures {
            let duration = block.duration_ms.map(|d| d.max(0) as u64);
            let sample = PictureSample::new(block.start_ms.max(0) as u64, duration, block.data);
            self.samples.lock().unwrap_or_else(|e| e.into_inner()).push(sample);
        } else {
            self.blocks.lock().unwrap_or_else(|e| e.into_inner()).push(block);
        }
    }

    fn finish(&self) {
        *self.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.finished.notify_all();
    }

    /// Waits up to `wait` for the reading to finish; whether it did.
    fn wait(&self, wait: Duration) -> bool {
        let done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        let (done, _) = self.finished.wait_timeout_while(done, wait, |d| !*d).unwrap_or_else(|e| e.into_inner());
        *done
    }

    /// The text read out so far, as WebVTT: made again only when more has been read.
    fn webvtt(&self) -> Vec<u8> {
        let mut blocks = self.blocks.lock().unwrap_or_else(|e| e.into_inner());
        let mut made = self.webvtt.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((count, body)) = &*made
            && *count == blocks.len()
        {
            return body.clone();
        }
        // In place: the blocks are held once, never copied to be sorted.
        blocks.sort_by_key(|b| b.start_ms);
        let body = cues::webvtt(&blocks, &self.track.codec_id).into_bytes();
        *made = Some((blocks.len(), body.clone()));
        body
    }
}

/// A video on a drive as the player's server.
pub struct LocalServer {
    file: PathBuf,
    root: PathBuf,
    store: Store,
    caps: CapsProbe,
    described: OnceLock<Described>,
    report: OnceLock<Option<CapsReport>>,
    extractions: Mutex<HashMap<i64, Arc<Extraction>>>,
    /// Stops every track being read when the player is done with the file.
    cancel: Arc<AtomicBool>,
}

impl LocalServer {
    /// The server for `file`, on the drive mounted at `drive_root`; where it
    /// stopped is kept in `store`. Nothing is read before it is asked for.
    pub fn new(file: PathBuf, drive_root: PathBuf, store: Store, caps: CapsProbe) -> LocalServer {
        LocalServer {
            file,
            root: drive_root,
            store,
            caps,
            described: OnceLock::new(),
            report: OnceLock::new(),
            extractions: Mutex::default(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The file's address for the TV's player: `file://` and its path, each
    /// part percent-encoded.
    pub fn file_url(path: &Path) -> String {
        let mut url = String::from("file://");
        for part in path.components() {
            match part {
                Component::Prefix(p) => url.push_str(&format!("/{}", p.as_os_str().to_string_lossy())),
                Component::RootDir => {}
                Component::CurDir | Component::ParentDir => continue,
                Component::Normal(name) => {
                    url.push('/');
                    match name.to_str() {
                        Some(text) => url.push_str(&encode_component(text)),
                        // A name that isn't UTF-8 (an old drive's code page): its bytes as they are.
                        None => {
                            for b in name.as_encoded_bytes() {
                                url.push_str(&format!("%{b:02X}"));
                            }
                        }
                    }
                }
            }
        }
        url
    }

    fn described(&self) -> &Described {
        self.described.get_or_init(|| self.describe())
    }

    fn describe(&self) -> Described {
        let size = fs::metadata(&self.file).map_or(0, |m| m.len());
        let relative = self.file.strip_prefix(&self.root).unwrap_or(&self.file);
        let id = resume::file_id(relative, size);
        // Matroska's head, else MP4's movie box, whatever the file is called.
        let facts = FileReader::open(&self.file).ok().and_then(|f| crate::facts(&f));
        // The file's own subtitles are read out of it through its Cues, which an MP4
        // hasn't: without them, only the files beside it show.
        let mut streams: Vec<_> = facts
            .as_ref()
            .map(|f| {
                f.streams
                    .iter()
                    .filter(|s| {
                        s.kind != StreamKind::Subtitle
                            || (f.cues_at.is_some() && f.tracks.iter().any(|t| t.index == s.index))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        // Only files on the drive: a subtitle file may be a link to anywhere, or a pipe that a
        // read would wait on for ever.
        let sidecars: Vec<_> = folder::sidecars_beside(&self.file)
            .into_iter()
            .filter(|s| fs::metadata(&s.path).is_ok_and(|m| m.is_file()) && folder::within(&self.root, &s.path))
            .collect();
        streams.extend(folder::sidecar_streams(&sidecars));
        let video = streams.iter().find(|s| s.kind == StreamKind::Video);
        let audio = streams.iter().find(|s| s.kind == StreamKind::Audio);
        let extension = self.file.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let duration_ms = facts.as_ref().and_then(|f| f.duration_ms);
        let file = MediaFile {
            id,
            path: self.file.to_string_lossy().into_owned(),
            size: size.min(i64::MAX as u64) as i64,
            container: Some(container_of(&extension)).filter(|c| !c.is_empty()),
            duration_ms,
            width: video.and_then(|v| v.width),
            height: video.and_then(|v| v.height),
            video_codec: video.map(|v| v.codec.clone()),
            audio_codec: audio.map(|a| a.codec.clone()),
            chapters: facts.as_ref().map(|f| f.chapters.clone()).unwrap_or_default(),
            streams,
            ..MediaFile::default()
        };
        let position_ms = resume::kept(&self.store, id).map_or(0, |k| k.position_ms);
        let detail = ItemDetail {
            summary: ItemSummary {
                id: ITEM_ID,
                kind: ItemKind::Movie,
                title: folder::title(&self.file),
                duration_ms,
                user_state: UserState { position_ms, ..UserState::default() },
                ..ItemSummary::default()
            },
            files: vec![file],
            ..ItemDetail::default()
        };
        Described { detail, facts: facts.map(Arc::new), sidecars }
    }

    fn file_id(&self) -> i64 {
        self.described().detail.files.first().map_or(0, |f| f.id)
    }

    /// The subtitle tracks the player draws, with where each is read from: a
    /// file beside the video by its path, a text track of the file's own as
    /// `embedded:N`; pictures have none (the player asks for their `.mks`).
    fn subtitle_tracks(&self, file: &MediaFile) -> Vec<SubtitleTrack> {
        let d = self.described();
        file.streams_of(StreamKind::Subtitle)
            .map(|s| {
                let url = if s.external {
                    let at = s.index - folder::SIDECAR_FIRST_INDEX;
                    usize::try_from(at).ok().and_then(|i| d.sidecars.get(i)).map(|c| c.path.to_string_lossy().into())
                } else if s.text_based != Some(false) {
                    Some(format!("{EMBEDDED}{}", s.index))
                } else {
                    None
                };
                SubtitleTrack {
                    index: s.index,
                    codec: s.codec.clone(),
                    language: s.language.clone(),
                    title: s.title.clone(),
                    default: s.default,
                    forced: s.forced,
                    external: s.external,
                    text_based: s.text_based != Some(false),
                    url,
                    ..SubtitleTrack::default()
                }
            })
            .collect()
    }

    /// The track `index` read out of the file, started the first time it's
    /// asked for, on a thread of its own with its own handle to the file.
    fn extract(&self, index: i64) -> Option<Arc<Extraction>> {
        let d = self.described();
        // Only a track it listed: one the file's Cues lead to.
        let listed =
            d.detail.files.first()?.streams_of(StreamKind::Subtitle).find(|s| s.index == index && !s.external)?;
        let pictures = listed.text_based == Some(false);
        let facts = d.facts.clone()?;
        let mut running = self.extractions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(x) = running.get(&index) {
            return Some(x.clone());
        }
        let track = facts.tracks.iter().find(|t| t.index == index)?.clone();
        let x = Arc::new(Extraction::new(track, pictures));
        let (reading, path, cancel) = (x.clone(), self.file.clone(), self.cancel.clone());
        let spawned = thread::Builder::new().name(format!("pley-subtitles-{index}")).spawn(move || {
            if let Ok(file) = FileReader::open(&path) {
                cues::read(&file, &facts, &reading.track, |b| reading.found(b), &cancel);
            }
            reading.finish();
        });
        if spawned.is_err() {
            x.finish();
        }
        running.insert(index, x.clone());
        Some(x)
    }

    /// A subtitle file beside the video, by the path this server gave for it.
    fn sidecar(&self, url: &str) -> Option<&Sidecar> {
        let found = self.described().sidecars.iter().find(|s| s.path.to_string_lossy() == url)?;
        // Read only on the drive: a subtitle file may be a link to anywhere.
        folder::within(&self.root, &found.path).then_some(found)
    }
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The answer for what a file on a drive doesn't have.
fn absent() -> ApiError {
    ApiError::Status {
        status: 404,
        code: "not_found".into(),
        message: "A file on a drive has no server to ask".into(),
        body: None,
    }
}

/// The audio a server would choose: the default track the TV plays, else
/// the first it plays, else the first.
fn audio_for(file: &MediaFile, caps: &ClientCaps) -> Option<i64> {
    let tracks: Vec<_> = file.streams_of(StreamKind::Audio).collect();
    let plays = |codec: &str| caps.plays_audio(codec);
    tracks
        .iter()
        .find(|t| t.default && plays(&t.codec))
        .or_else(|| tracks.iter().find(|t| plays(&t.codec)))
        .or(tracks.first())
        .map(|t| t.index)
}

/// `n` of `/api/files/{id}/subtitles/{n}.mks`, the address the player asks
/// picture subtitles at.
fn picture_index(url: &str) -> Option<i64> {
    let name = url.rsplit('/').next()?.strip_suffix(".mks")?;
    url.contains("/subtitles/").then(|| name.parse().ok()).flatten()
}

impl PlayerServer for LocalServer {
    fn server(&self) -> &str {
        SERVER
    }

    fn url(&self, path: &str) -> String {
        if path == self.file.to_string_lossy() {
            LocalServer::file_url(&self.file)
        } else {
            LocalServer::file_url(Path::new(path))
        }
    }

    fn item(&self, _id: i64) -> Result<ItemDetail, ApiError> {
        Ok(self.described().detail.clone())
    }

    fn next(&self, _id: i64) -> Result<Option<ItemSummary>, ApiError> {
        Ok(None)
    }

    fn playlist(&self, _id: i64) -> Result<PlaylistDetail, ApiError> {
        Err(absent())
    }

    fn collection(&self, _id: i64) -> Result<CollectionDetail, ApiError> {
        Err(absent())
    }

    /// The file as it is, from where it was asked: what a server answers for
    /// direct play. A conversion asked for (the TV refused the file) can't be had.
    fn play(&self, req: &PlaybackRequest) -> Result<PlaybackInfo, ApiError> {
        if matches!(req.mode, PlaybackMode::Remux | PlaybackMode::Transcode) {
            return Err(ApiError::Status {
                status: 409,
                code: "local_file".into(),
                message: nothing_converts().into(),
                body: None,
            });
        }
        // Only a video on the drive, not one a link on it leads to elsewhere on the TV.
        if !folder::within(&self.root, &self.file) {
            return Err(absent());
        }
        let file = self.described().detail.files.first().ok_or_else(absent)?;
        let report = self.report.get_or_init(|| (self.caps)());
        // An empty list says nothing: no container rule then, as the player has it.
        let dv_containers = report.as_ref().map(|r| &r.dv_containers).filter(|c| !c.is_empty());
        let container = file.container.as_deref().unwrap_or("");
        let video_range =
            video_of(file).and_then(|v| direct_range_shown(v, &req.caps, container, dv_containers.map(|c| &c[..])));
        Ok(PlaybackInfo {
            session_id: SERVER.into(),
            method: PlaybackMethod::Direct,
            url: file.path.clone(),
            hls: false,
            file_id: file.id,
            duration_ms: file.duration_ms.unwrap_or(0),
            audio_index: req.audio_index.or_else(|| audio_for(file, &req.caps)),
            subtitles: self.subtitle_tracks(file),
            video_range,
            ..PlaybackInfo::default()
        })
    }

    /// Where it stopped, kept on the TV; at 95 % it was watched, and starts over next time.
    fn progress(&self, _item_id: i64, report: &ProgressReport) -> Result<(), ApiError> {
        let duration = report.duration_ms.or(self.described().detail.files.first().and_then(|f| f.duration_ms));
        // Storage full or blocked: it starts where it did last time.
        let _ = resume::keep(&self.store, self.file_id(), report.position_ms, duration);
        Ok(())
    }

    fn stop_session(&self, _session: &str) -> Result<(), ApiError> {
        Ok(())
    }

    fn session_status(&self, session: &str, _timeout: Duration) -> Result<SessionStatus, ApiError> {
        Ok(SessionStatus { session_id: session.into(), method: PlaybackMethod::Direct, transcode: None })
    }

    fn set_bitrate(&self, _session: &str, _kbps: u32) -> Result<BitrateAnswer, ApiError> {
        Err(absent())
    }

    fn set_streams(&self, _item_id: i64, _choice: &StreamChoice) -> Result<(), ApiError> {
        Ok(())
    }

    fn reachable(&self, _timeout: Duration) -> Result<ServerInfo, ApiError> {
        Ok(ServerInfo::default())
    }

    fn media(&self, _method: Method, _url: &str, _timeout: Duration) -> Result<String, ApiError> {
        Err(absent())
    }

    fn network(&self, _client_id: &str, _timeout: Duration) -> Result<NetworkAnswer, ApiError> {
        Err(absent())
    }

    fn tell_network(
        &self,
        _client_id: &str,
        _report: &NetworkReport,
        _timeout: Duration,
    ) -> Result<NetworkAnswer, ApiError> {
        Err(absent())
    }

    fn speed_test(&self, _bytes: u64) -> Result<SpeedSample, ApiError> {
        Err(absent())
    }

    /// None: the TV can't make seek previews. An answer, so nothing asks again.
    fn trickplay(&self, _file_id: i64) -> Result<TrickplayInfo, ApiError> {
        Ok(TrickplayInfo::default())
    }

    /// A subtitle file beside the video, as WebVTT, whole; or a text track of
    /// the file's own, as much of it as is read out yet (asked again until all
    /// is). Only addresses this server gave are read.
    fn subtitles(&self, url: &str, _accept: &str, timeout: Duration) -> Result<SubtitleAnswer, ApiError> {
        if let Some(n) = url.strip_prefix(EMBEDDED) {
            let index: i64 = n.parse().map_err(|_| absent())?;
            let x = self.extract(index).filter(|x| !x.pictures).ok_or_else(absent)?;
            let complete = x.wait(EXTRACT_WAIT.min(timeout));
            return Ok(SubtitleAnswer { status: 200, body: x.webvtt(), complete });
        }
        let sidecar = self.sidecar(url).ok_or_else(absent)?;
        let text = folder::read_sidecar(&sidecar.path).map_err(|_| absent())?;
        Ok(SubtitleAnswer { status: 200, body: text.into_bytes(), complete: true })
    }

    fn search_subtitles(
        &self,
        _item_id: i64,
        _file_id: Option<i64>,
        _languages: &[String],
    ) -> Result<Vec<SubtitleResult>, ApiError> {
        Err(absent())
    }

    fn add_subtitle(&self, _item_id: i64, _add: &AddSubtitle) -> Result<SubtitleTrack, ApiError> {
        Err(absent())
    }

    fn watch_channel(&self, _channel: i64, _req: &WatchRequest) -> Result<LiveWatching, ApiError> {
        Err(absent())
    }

    fn live_plan(&self, _session: &str) -> Result<Option<LivePlan>, ApiError> {
        Err(absent())
    }

    fn leave_channel(&self, _session: &str) -> Result<(), ApiError> {
        Err(absent())
    }

    fn live_channel(&self, _id: i64) -> Result<LiveChannel, ApiError> {
        Err(absent())
    }

    fn live_groups(&self) -> Result<LiveGroups, ApiError> {
        Err(absent())
    }

    fn live_browse(&self, _b: &LiveBrowse) -> Result<LiveChannelPage, ApiError> {
        Err(absent())
    }

    fn live_guide(&self, _from: i64, _to: i64, _ids: &[i64]) -> Result<LiveGuide, ApiError> {
        Err(absent())
    }

    fn set_live_favorite(&self, _channel: i64, _favorite: bool) -> Result<(), ApiError> {
        Err(absent())
    }

    fn local(&self) -> bool {
        true
    }

    /// A picture track of the file's own (`…/subtitles/4.mks`): as much as is
    /// read out yet, its samples as they are, no `.mks` made of them.
    fn picture_track(&self, url: &str, timeout: Duration) -> Option<Result<(PictureTrack, bool), ApiError>> {
        let x = picture_index(url).and_then(|index| self.extract(index)).filter(|x| x.pictures);
        let Some(x) = x else { return Some(Err(absent())) };
        let complete = x.wait(EXTRACT_WAIT.min(timeout));
        let mut samples = x.samples.lock().unwrap_or_else(|e| e.into_inner()).clone();
        samples.sort_by_key(|s| s.time_ms);
        let track =
            PictureTrack { codec_id: x.track.codec_id.clone(), codec_private: x.track.private.clone(), samples };
        Some(Ok((track, complete)))
    }
}
