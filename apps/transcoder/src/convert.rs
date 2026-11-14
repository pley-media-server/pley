// SPDX-License-Identifier: AGPL-3.0-only
//! Conversions: a [`JobRequest`] made into one Matroska file, in one FFmpeg
//! run at full speed (a converted download, for a device that doesn't play
//! the original as it is). One runs at a time, in a transcode slot like a
//! job's process (hardware when a session is free), so playback keeps the
//! others; background work waits for it as for a job ([`crate::background`]).
//! Asked again for the same input and request, the same conversion
//! answers, or the file it made. The files live in `conversions/` under the
//! work folder, which the janitor prunes ([`crate::cache`]); reading one
//! counts as using it. One left waiting while nobody asks about it isn't
//! made.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use parking_lot::Mutex;
use pley_transcoder_api::{ConversionState, ConversionStatus, FileContainer, JobRequest, MediaInfo, VideoMode};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{Semaphore, watch};
use tokio::time::Instant;

use crate::adaptive::AUTO;
use crate::error::{ApiError, ApiResult};
use crate::ffmpeg::Ffmpeg;
use crate::job::Slots;
use crate::plan::{ArgsContext, MAX_DURATION_MS, Plan};
use crate::probe::Prober;
use crate::util::{background_priority, touch};

/// How often a conversion waiting for a transcode slot looks again.
const SLOT_WAIT: Duration = Duration::from_secs(5);
/// FFmpeg's last lines kept, for a failure's message.
const TAIL_LINES: usize = 6;
/// The software encoder's preset when the plan leaves it to the transcoder:
/// a file isn't made in real time, so it can take a little more care.
const SOFTWARE_PRESET: &str = "fast";
/// How far FFmpeg must have come for a file to be whole: a little short of
/// the end, as containers' durations and the streams' ends differ a little.
const WHOLE: f64 = 0.95;
/// A conversion nobody asked for or about this long while it waited (the
/// device that wanted it went) isn't made when its turn comes. Devices
/// look every few seconds; one that sleeps a while asks again on waking.
const UNCLAIMED_AFTER: Duration = Duration::from_secs(60 * 60);

pub struct Conversions {
    ff: Arc<Ffmpeg>,
    prober: Arc<Prober>,
    slots: Arc<Slots>,
    shader_cache: Option<PathBuf>,
    dir: PathBuf,
    items: Mutex<HashMap<String, Arc<Conversion>>>,
    /// One conversion at a time.
    turn: Arc<Semaphore>,
}

struct Conversion {
    status: Mutex<ConversionStatus>,
    /// Set when the conversion is removed (or the transcoder stops): FFmpeg goes.
    stop: watch::Sender<bool>,
    /// When it was last asked for or about.
    asked: Mutex<Instant>,
}

impl Conversion {
    fn new(status: ConversionStatus) -> Arc<Self> {
        Arc::new(Self { status: Mutex::new(status), stop: watch::channel(false).0, asked: Mutex::new(Instant::now()) })
    }

    fn asked_now(&self) {
        *self.asked.lock() = Instant::now();
    }

    fn update(&self, change: impl FnOnce(&mut ConversionStatus)) {
        change(&mut self.status.lock());
    }

    /// Resolves once the conversion is to stop.
    async fn stopped(&self) {
        let mut stop = self.stop.subscribe();
        let _ = stop.wait_for(|stop| *stop).await;
    }
}

impl Conversions {
    pub fn new(
        ff: Arc<Ffmpeg>,
        prober: Arc<Prober>,
        slots: Arc<Slots>,
        shader_cache: Option<PathBuf>,
        dir: PathBuf,
    ) -> Arc<Self> {
        // What a crash left half made is of no use: it starts again when asked.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|e| e == "part") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Arc::new(Self {
            ff,
            prober,
            slots,
            shader_cache,
            dir,
            items: Mutex::new(HashMap::new()),
            turn: Arc::new(Semaphore::new(1)),
        })
    }

    /// Starts converting `req`, or finds the conversion (or the file) made
    /// for it already. A failed one is tried again.
    pub async fn create(self: &Arc<Self>, req: JobRequest) -> ApiResult<ConversionStatus> {
        if !self.ff.caps.ffmpeg_available {
            return Err(ApiError::Failed("ffmpeg is not available on the transcoder host".into()));
        }
        let meta = match tokio::fs::metadata(&req.input).await {
            Ok(meta) if meta.is_file() => meta,
            Ok(_) => return Err(ApiError::BadRequest(format!("{} is not a file", req.input))),
            Err(_) => return Err(ApiError::NotFound(format!("input not found: {}", req.input))),
        };
        let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
        let id = conversion_id(&req, meta.len(), modified.map_or(0, |d| d.as_millis()));
        let duration_ms = req.media.as_ref().and_then(|m| m.duration_ms).unwrap_or(0);
        let conversion = {
            let mut items = self.items.lock();
            // Failed, or done but its file pruned since: made again.
            if let Some(found) = items.get(&id).filter(|c| self.standing(&id, c)) {
                found.asked_now();
                return Ok(found.status.lock().clone());
            }
            if let Some(status) = self.finished(&id, duration_ms) {
                items.insert(id.clone(), Conversion::new(status.clone()));
                return Ok(status);
            }
            let conversion = Conversion::new(ConversionStatus { id: id.clone(), duration_ms, ..Default::default() });
            items.insert(id.clone(), conversion.clone());
            conversion
        };
        tracing::info!(conversion = %id, input = %req.input, "conversion queued");
        let status = conversion.status.lock().clone();
        let this = self.clone();
        tokio::spawn(async move {
            let result = this.convert(&conversion, req).await;
            if result.as_ref().is_err_and(|err| err.is::<Unclaimed>()) {
                tracing::info!(conversion = %id, "conversion dropped: nobody asked about it while it waited");
                let mut items = this.items.lock();
                if items.get(&id).is_some_and(|c| Arc::ptr_eq(c, &conversion)) {
                    items.remove(&id);
                }
                return;
            }
            let stopped = *conversion.stop.borrow();
            conversion.update(|s| match result {
                Ok(size) => {
                    tracing::info!(conversion = %s.id, size, "conversion done");
                    s.state = ConversionState::Done;
                    s.progress = 1.0;
                    s.size = Some(size);
                }
                Err(err) if stopped => {
                    tracing::info!(conversion = %s.id, "conversion stopped");
                    s.state = ConversionState::Failed;
                    s.error = Some(format!("{err:#}"));
                }
                Err(err) => {
                    tracing::warn!(conversion = %s.id, "conversion failed: {err:#}");
                    s.state = ConversionState::Failed;
                    s.error = Some(format!("{err:#}"));
                }
            });
        });
        Ok(status)
    }

    /// Conversion `id` as it stands; one made before a restart is known by
    /// its file, and one whose file was pruned since isn't any more (asked
    /// for again, it is made again).
    pub fn status(&self, id: &str) -> Option<ConversionStatus> {
        let mut items = self.items.lock();
        if let Some(found) = items.get(id) {
            let done = found.status.lock().state == ConversionState::Done;
            if done && !self.path(id).is_some_and(|p| p.is_file()) {
                items.remove(id);
                return None;
            }
            found.asked_now();
            return Some(found.status.lock().clone());
        }
        drop(items);
        self.finished(id, 0)
    }

    /// Whether `conversion` still answers for `id`: not failed, and when
    /// done, with its file still there.
    fn standing(&self, id: &str, conversion: &Conversion) -> bool {
        match conversion.status.lock().state {
            ConversionState::Failed => false,
            ConversionState::Done => self.path(id).is_some_and(|p| p.is_file()),
            ConversionState::Queued | ConversionState::Running => true,
        }
    }

    /// The file conversion `id` made, once whole.
    pub fn file(&self, id: &str) -> Option<PathBuf> {
        let path = self.path(id)?;
        if !path.is_file() {
            return None;
        }
        touch(&path);
        Some(path)
    }

    /// Stops conversion `id` and deletes what it made: whether there was one.
    pub async fn remove(&self, id: &str) -> bool {
        let Some(paths) = self.paths(id) else { return false };
        let known = self.items.lock().remove(id);
        if let Some(conversion) = &known {
            conversion.stop.send_replace(true);
        }
        let mut had_file = false;
        for path in paths {
            had_file |= tokio::fs::remove_file(&path).await.is_ok();
            // FFmpeg may still hold the half-made file a moment.
            let part = part_of(&path);
            for _ in 0..20 {
                if tokio::fs::remove_file(&part).await.is_ok() || !part.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        known.is_some() || had_file
    }

    /// Stops every conversion: the transcoder is going.
    pub fn shutdown(&self) {
        for conversion in self.items.lock().values() {
            conversion.stop.send_replace(true);
        }
    }

    /// Where conversion `id`'s file is: its MP4, else its Matroska file
    /// (the container is in the id); `None` for an id that isn't one.
    fn path(&self, id: &str) -> Option<PathBuf> {
        let [mkv, mp4] = self.paths(id)?;
        Some(if mp4.is_file() { mp4 } else { mkv })
    }

    /// Where conversion `id`'s file may be, in each container.
    fn paths(&self, id: &str) -> Option<[PathBuf; 2]> {
        (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())).then(|| {
            [FileContainer::Matroska, FileContainer::Mp4].map(|c| self.dir.join(format!("{id}.{}", c.extension())))
        })
    }

    /// The status of a conversion whose file is there already.
    fn finished(&self, id: &str, duration_ms: i64) -> Option<ConversionStatus> {
        let size = std::fs::metadata(self.path(id)?).ok().filter(|m| m.is_file())?.len();
        Some(ConversionStatus {
            id: id.to_owned(),
            state: ConversionState::Done,
            progress: 1.0,
            duration_ms,
            size: Some(size),
            ..Default::default()
        })
    }

    /// Makes the file: its turn, then a slot, then FFmpeg. The file's size.
    async fn convert(&self, conversion: &Arc<Conversion>, mut req: JobRequest) -> anyhow::Result<u64> {
        let _turn = tokio::select! {
            turn = self.turn.clone().acquire_owned() => turn?,
            () = conversion.stopped() => bail!("removed"),
        };
        if conversion.asked.lock().elapsed() > UNCLAIMED_AFTER {
            return Err(Unclaimed.into());
        }
        let id = conversion.status.lock().id.clone();
        let input = PathBuf::from(&req.input);
        let media: Arc<MediaInfo> = match req
            .media
            .clone()
            .filter(|m| m.duration_ms.is_some_and(|d| d > 0 && d <= MAX_DURATION_MS) && !m.streams.is_empty())
            .filter(|m| m.start_ms.is_some())
        {
            Some(media) => Arc::new(media),
            None => self.prober.probe(&input).await.context("probe failed")?,
        };
        // Made at full speed, from the start.
        req.options.throttle = false;
        req.start_ms = None;
        // One file is one segment: a copy needs no keyframes but the first.
        let first = [media.start_ms.unwrap_or(0)];
        let mut plan = Plan::build(&req, &media, &self.ff, Some(&first)).map_err(|err| anyhow!(err))?;
        if plan.segment_edits().is_some() {
            // A remux's edits (Dolby Vision, HDR10+) are made to segments as
            // they are written: a file can't have them, so its video is made anew.
            if let Some(video) = req.video.as_mut() {
                video.mode = VideoMode::Transcode;
            }
            plan = Plan::build(&req, &media, &self.ff, Some(&first)).map_err(|err| anyhow!(err))?;
        }
        let duration_ms = media.duration_ms.unwrap_or(0).max(1);
        conversion.update(|s| {
            s.state = ConversionState::Running;
            s.duration_ms = duration_ms;
            s.video = plan.video_plan(&media, None);
            s.audio = plan.audio_plan(&media);
        });

        // A transcode slot, as a job's process takes: waited for while every one is in use.
        let want_hw = plan.encoder().is_some_and(|e| e.hardware);
        let slot = loop {
            if let Some(slot) = self.slots.take(want_hw) {
                break slot;
            }
            tokio::select! {
                () = tokio::time::sleep(SLOT_WAIT) => {}
                () = conversion.stopped() => bail!("removed"),
            }
        };
        let fallback = if want_hw && !slot.hw {
            let software = plan.software_stand_in(&self.ff);
            Some(software.ok_or_else(|| anyhow!("no encoder is free: every hardware session is in use"))?)
        } else {
            None
        };
        let preset = plan
            .video
            .as_ref()
            .filter(|v| !v.copy)
            .map(|v| if v.preset == AUTO { SOFTWARE_PRESET } else { v.preset.as_str() });

        tokio::fs::create_dir_all(&self.dir).await?;
        let container = req.container.unwrap_or_default();
        let path = self.dir.join(format!("{id}.{}", container.extension()));
        let part = part_of(&path);
        let ctx = ArgsContext {
            input: &input,
            encoder: fallback,
            relative_split_times: self.ff.segment_times_relative,
            preset,
            shader_cache: self.shader_cache.as_deref(),
        };
        let args = plan.file_args(&ctx, &media, &part, container);
        tracing::debug!(conversion = %id, "ffmpeg {}", args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" "));
        let mut cmd = self.ff.command();
        // A file for later: never at the cost of a playback.
        background_priority(&mut cmd);
        cmd.args(&args).current_dir(&self.dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = cmd.spawn().context("failed to start ffmpeg")?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("ffmpeg's error output is unavailable"))?;
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let reader = tokio::spawn(read_progress(stderr, duration_ms, tail.clone(), conversion.clone()));
        let (status, removed) = tokio::select! {
            status = child.wait() => (status?, false),
            () = conversion.stopped() => {
                let _ = child.start_kill();
                (child.wait().await?, true)
            }
        };
        let _ = reader.await;
        if removed {
            let _ = tokio::fs::remove_file(&part).await;
            bail!("removed");
        }
        if !status.success() {
            let _ = tokio::fs::remove_file(&part).await;
            let lines: Vec<String> = tail.lock().iter().cloned().collect();
            if lines.is_empty() {
                bail!("ffmpeg exited with {status}");
            }
            bail!("ffmpeg exited with {status}: {}", lines.join(" / "));
        }
        // FFmpeg ends a file cut short by a read error (a share that went
        // away) as if it were whole: it must reach the media's end.
        let reached = conversion.status.lock().progress;
        if reached < WHOLE {
            let _ = tokio::fs::remove_file(&part).await;
            bail!("the file stopped at {:.0} % of the media", reached * 100.0);
        }
        tokio::fs::rename(&part, &path).await.context("couldn't keep the file")?;
        Ok(tokio::fs::metadata(&path).await?.len())
    }
}

/// Why a conversion wasn't made: nobody asked about it while it waited.
#[derive(Debug)]
struct Unclaimed;

impl std::fmt::Display for Unclaimed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("nobody asked about it while it waited")
    }
}

impl std::error::Error for Unclaimed {}

/// The file being made for `path`.
fn part_of(path: &Path) -> PathBuf {
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    PathBuf::from(part)
}

/// What a conversion makes besides the request's streams: its subtitle
/// tracks since 2. A file made before names another conversion.
const MADE: u32 = 2;

/// What names a conversion: the input as it is now (path, size and time)
/// and what the request makes of it, not who asked or where a playback
/// would have started.
fn conversion_id(req: &JobRequest, size: u64, modified_ms: u128) -> String {
    let mut what = serde_json::json!({
        "made": MADE,
        "input": req.input,
        "size": size,
        "modified": modified_ms.to_string(),
        "video": req.video,
        "audio": req.audio,
        "subtitle": req.subtitle,
    });
    // An MP4 is another file; Matroska ones keep the ids they had.
    if let Some(container) = req.container.filter(|c| *c != FileContainer::Matroska) {
        what["container"] = serde_json::json!(container);
    }
    let digest = Sha256::digest(what.to_string().as_bytes());
    hex::encode(&digest[..16])
}

/// Reads FFmpeg's `-progress` lines into how far `conversion` has come,
/// and its other lines (errors) into `tail`.
async fn read_progress(
    stderr: tokio::process::ChildStderr,
    duration_ms: i64,
    tail: Arc<Mutex<VecDeque<String>>>,
    conversion: Arc<Conversion>,
) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some((key, value)) = line.split_once('=') {
            if key.trim() == "out_time_us" {
                if let Ok(us) = value.trim().parse::<f64>() {
                    let progress = progress_of(us, duration_ms);
                    conversion.update(|s| s.progress = s.progress.max(progress));
                }
                continue;
            }
            // The other `-progress` keys: one word each.
            if !key.contains(' ') {
                continue;
            }
        }
        let line = line.trim();
        if !line.is_empty() {
            let mut tail = tail.lock();
            if tail.len() == TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line.to_owned());
        }
    }
}

/// `out_time_us` as a share of the media, 0 to 1.
fn progress_of(out_time_us: f64, duration_ms: i64) -> f64 {
    if duration_ms <= 0 || out_time_us <= 0.0 {
        return 0.0;
    }
    (out_time_us / 1000.0 / duration_ms as f64).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::Background;
    use pley_transcoder_api::{AudioRequest, VideoRequest};

    fn request(max_height: u32) -> JobRequest {
        JobRequest {
            input: "/media/movie.mkv".into(),
            label: Some("Movie".into()),
            owner: Some("anna".into()),
            video: Some(VideoRequest { max_height: Some(max_height), ..Default::default() }),
            audio: Some(AudioRequest::default()),
            start_ms: Some(60_000),
            ..Default::default()
        }
    }

    #[test]
    fn an_mp4_is_another_conversion_and_matroska_ones_keep_their_ids() {
        let mkv = conversion_id(&request(1080), 1_000, 7);
        assert_eq!(
            mkv,
            conversion_id(&JobRequest { container: Some(FileContainer::Matroska), ..request(1080) }, 1_000, 7)
        );
        let mp4 = conversion_id(&JobRequest { container: Some(FileContainer::Mp4), ..request(1080) }, 1_000, 7);
        assert_ne!(mkv, mp4);
        assert_eq!(mp4.len(), 32);
    }

    #[test]
    fn a_conversion_is_named_by_its_input_and_what_it_makes() {
        let id = conversion_id(&request(1080), 100, 5);
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        // Who asked and where a playback would start make no difference.
        let mut other = request(1080);
        other.owner = Some("ben".into());
        other.label = None;
        other.start_ms = None;
        assert_eq!(conversion_id(&other, 100, 5), id);
        // Another size, another file, another time or another target do.
        assert_ne!(conversion_id(&request(720), 100, 5), id);
        assert_ne!(conversion_id(&request(1080), 101, 5), id);
        assert_ne!(conversion_id(&request(1080), 100, 6), id);
    }

    fn has_ffmpeg() -> bool {
        let ok =
            |tool: &str| std::process::Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success());
        let found = ok("ffmpeg") && ok("ffprobe");
        if !found {
            eprintln!("ffmpeg not found: skipped");
        }
        found
    }

    /// An MP4 conversion, for players that open no Matroska (skipped without
    /// FFmpeg and its x265): HEVC copied and tagged hvc1, SubRip made MP4's
    /// own text, and the index ahead of the media, as QuickTime wants them.
    #[tokio::test]
    async fn an_mp4_conversion_is_one_quicktime_plays() {
        use pley_transcoder_api::{
            AudioCodec, AudioMode, AudioRequest, Capabilities, EncoderInfo, HwAccel, Limits, VideoCodec, VideoRequest,
        };
        if !has_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mkv");
        let srt = dir.path().join("source.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=duration=2:size=320x180:rate=24"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
            .arg("-i")
            .arg(&srt)
            .args(["-map", "0", "-map", "1", "-map", "2"])
            .args(["-c:v", "libx265", "-preset", "ultrafast", "-x265-params", "log-level=error"])
            .args(["-c:a", "aac", "-c:s", "srt"])
            .arg(&source)
            .status()
            .unwrap();
        if !made.success() {
            eprintln!("no x265 here: skipped");
            return;
        }
        let ff = Arc::new(Ffmpeg {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            caps: Capabilities {
                ffmpeg_available: true,
                ffprobe_available: true,
                encoders: vec![EncoderInfo {
                    name: "libx264".into(),
                    codec: VideoCodec::H264,
                    hardware: false,
                    accel: HwAccel::None,
                    main10: false,
                    low_power: false,
                }],
                ..Default::default()
            },
            segment_times_relative: true,
        });
        let metrics = Arc::new(crate::metrics::Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics, 1));
        let slots = Arc::new(Slots::new(&Limits { max_jobs: 2, max_hw_jobs: 0 }, Background::new()));
        let conversions = Conversions::new(ff, prober, slots, None, dir.path().join("conversions"));
        let req = JobRequest {
            input: source.to_string_lossy().into_owned(),
            video: Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() }),
            audio: Some(AudioRequest { mode: AudioMode::Copy, codec: AudioCodec::Aac, ..Default::default() }),
            container: Some(FileContainer::Mp4),
            ..Default::default()
        };

        let id = conversions.create(req).await.unwrap().id;
        let status = loop {
            let status = conversions.status(&id).unwrap();
            if matches!(status.state, ConversionState::Done | ConversionState::Failed) {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(status.state, ConversionState::Done, "{:?}", status.error);
        let file = conversions.file(&id).unwrap();
        assert_eq!(file.extension().unwrap(), "mp4");
        let out = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,codec_tag_string", "-of", "csv=p=0"])
            .arg(&file)
            .output()
            .unwrap();
        let streams = String::from_utf8_lossy(&out.stdout).replace("\r", "");
        assert_eq!(streams.trim(), "hevc,hvc1\naac,mp4a\nmov_text,tx3g", "{streams}");
        // Its index ahead of the media: it plays as soon as it opens.
        let bytes = std::fs::read(&file).unwrap();
        let mut boxes = Vec::new();
        let mut at = 0;
        while at + 8 <= bytes.len() {
            let size = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            boxes.push(String::from_utf8_lossy(&bytes[at + 4..at + 8]).into_owned());
            if size < 8 {
                break;
            }
            at += size;
        }
        let place = |name: &str| boxes.iter().position(|b| b == name).unwrap();
        assert!(place("moov") < place("mdat"), "{boxes:?}");

        assert!(conversions.remove(&id).await);
        assert!(!file.exists());
    }

    /// A conversion made by FFmpeg (skipped without it on the PATH): a 4 s
    /// clip of H.264 and E-AC-3 5.1 (sound many phones don't decode), its
    /// video copied and its sound made AAC stereo, into Matroska.
    #[tokio::test]
    async fn a_conversion_makes_a_file_of_the_streams_asked_for() {
        use pley_transcoder_api::{
            AudioCodec, AudioMode, AudioRequest, Capabilities, EncoderInfo, HwAccel, Limits, VideoCodec, VideoRequest,
        };
        if !has_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mkv");
        let srt = dir.path().join("source.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=duration=4:size=320x180:rate=24"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=4"])
            .arg("-i")
            .arg(&srt)
            .args(["-map", "0", "-map", "1", "-map", "2"])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-c:a", "eac3", "-ac", "6", "-c:s", "srt"])
            // A font, as files with styled subtitles carry.
            .arg("-attach")
            .arg(&srt)
            .args(["-metadata:s:t", "mimetype=application/x-truetype-font"])
            .arg(&source)
            .status()
            .unwrap();
        assert!(made.success());

        let ff = Arc::new(Ffmpeg {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            caps: Capabilities {
                ffmpeg_available: true,
                ffprobe_available: true,
                encoders: vec![EncoderInfo {
                    name: "libx264".into(),
                    codec: VideoCodec::H264,
                    hardware: false,
                    accel: HwAccel::None,
                    main10: false,
                    low_power: false,
                }],
                ..Default::default()
            },
            segment_times_relative: true,
        });
        let metrics = Arc::new(crate::metrics::Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics, 1));
        let background = Background::new();
        let slots = Arc::new(Slots::new(&Limits { max_jobs: 2, max_hw_jobs: 0 }, background.clone()));
        let conversions = Conversions::new(ff, prober, slots.clone(), None, dir.path().join("conversions"));
        let req = JobRequest {
            input: source.to_string_lossy().into_owned(),
            video: Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() }),
            audio: Some(AudioRequest {
                mode: AudioMode::Transcode,
                codec: AudioCodec::Aac,
                channels: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        };

        let created = conversions.create(req.clone()).await.unwrap();
        let id = created.id.clone();
        let status = loop {
            let status = conversions.status(&id).unwrap();
            if matches!(status.state, ConversionState::Done | ConversionState::Failed) {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(status.state, ConversionState::Done, "{:?}", status.error);
        assert_eq!(status.progress, 1.0);
        assert_eq!(status.video.as_ref().map(|v| v.mode), Some(VideoMode::Copy));
        assert_eq!(slots.counts(), (0, 0), "its slot is given back");
        assert!(background.held(), "background work waited for it, and waits a moment longer");

        let file = conversions.file(&id).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().len(), status.size.unwrap());
        let out = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,channels", "-of", "csv=p=0"])
            .arg(&file)
            .output()
            .unwrap();
        let streams = String::from_utf8_lossy(&out.stdout).replace("\r", "");
        assert_eq!(streams.trim(), "h264\naac,2\nsubrip", "{streams}");

        // Asked again: the file made.
        let again = conversions.create(req.clone()).await.unwrap();
        assert_eq!((again.id, again.state), (id.clone(), ConversionState::Done));
        // Pruned by the janitor: not done any more, and made again when asked for.
        std::fs::remove_file(&file).unwrap();
        assert!(conversions.status(&id).is_none());
        let remade = conversions.create(req.clone()).await.unwrap();
        assert_eq!(remade.id, id);
        assert_ne!(remade.state, ConversionState::Done);
        let status = loop {
            let status = conversions.status(&id).unwrap();
            if matches!(status.state, ConversionState::Done | ConversionState::Failed) {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(status.state, ConversionState::Done, "{:?}", status.error);
        assert!(conversions.remove(&id).await);
        assert!(conversions.file(&id).is_none());

        // Its video made anew, smaller: another conversion of the same file.
        let smaller = JobRequest {
            video: Some(VideoRequest { mode: VideoMode::Transcode, max_height: Some(90), ..Default::default() }),
            ..req
        };
        let created = conversions.create(smaller).await.unwrap();
        assert_ne!(created.id, id);
        let status = loop {
            let status = conversions.status(&created.id).unwrap();
            if matches!(status.state, ConversionState::Done | ConversionState::Failed) {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert_eq!(status.state, ConversionState::Done, "{:?}", status.error);
        let out = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v",
                "-show_entries",
                "stream=codec_name,height",
                "-of",
                "csv=p=0",
            ])
            .arg(conversions.file(&created.id).unwrap())
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "h264,90");
    }

    /// One left waiting while nobody asks about it (its device went) isn't
    /// made when its turn comes, and asking again starts it again; one
    /// asked about goes on.
    #[tokio::test(start_paused = true)]
    async fn a_conversion_nobody_asks_about_while_it_waits_is_dropped() {
        use pley_transcoder_api::{Capabilities, Limits};
        let dir = tempfile::tempdir().unwrap();
        let input = |name: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, b"not media").unwrap();
            path.to_string_lossy().into_owned()
        };
        // FFmpeg and FFprobe that aren't there: nothing here gets as far as running them.
        let ff = Arc::new(Ffmpeg {
            ffmpeg: "pley-test-no-ffmpeg".into(),
            ffprobe: "pley-test-no-ffprobe".into(),
            caps: Capabilities { ffmpeg_available: true, ffprobe_available: true, ..Default::default() },
            segment_times_relative: true,
        });
        let metrics = Arc::new(crate::metrics::Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics, 1));
        let slots = Arc::new(Slots::new(&Limits { max_jobs: 2, max_hw_jobs: 0 }, Background::new()));
        let conversions = Conversions::new(ff, prober, slots, None, dir.path().join("conversions"));
        // Another conversion's turn.
        let turn = conversions.turn.clone().acquire_owned().await.unwrap();
        let left_input = input("left.mkv");
        let left = conversions.create(JobRequest { input: left_input.clone(), ..Default::default() }).await.unwrap();
        let wanted = conversions.create(JobRequest { input: input("wanted.mkv"), ..Default::default() }).await.unwrap();
        assert_eq!((left.state, wanted.state), (ConversionState::Queued, ConversionState::Queued));

        tokio::time::advance(UNCLAIMED_AFTER - Duration::from_secs(60)).await;
        assert!(conversions.status(&wanted.id).is_some());
        tokio::time::advance(Duration::from_secs(120)).await;
        drop(turn);
        // Watched without asking about it: a look would count as one.
        let known = |id: &str| conversions.items.lock().contains_key(id);
        for _ in 0..100 {
            if !known(&left.id) {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(!known(&left.id), "dropped");
        assert!(conversions.status(&left.id).is_none());
        assert!(conversions.status(&wanted.id).is_some(), "made (here it fails: no FFprobe)");
        let again = conversions.create(JobRequest { input: left_input, ..Default::default() }).await.unwrap();
        assert_eq!((again.id, again.state), (left.id, ConversionState::Queued));
    }

    #[test]
    fn progress_is_a_share_of_the_media() {
        assert_eq!(progress_of(30_000_000.0, 60_000), 0.5);
        assert_eq!(progress_of(90_000_000.0, 60_000), 1.0);
        assert_eq!(progress_of(-1.0, 60_000), 0.0);
        assert_eq!(progress_of(1.0, 0), 0.0);
    }
}
