// SPDX-License-Identifier: AGPL-3.0-only
//! One-shot extractions: video frame thumbnails, embedded cover art,
//! subtitles converted to WebVTT (read block by block through a Matroska
//! file's Cues, or else all text tracks of a file in one FFmpeg pass, since
//! extracting any track that way means reading the whole file), and the
//! fonts attached to a file, for burned-in ASS.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use futures::future::{BoxFuture, FutureExt, Shared};
use parking_lot::Mutex;
use pley_transcoder_api::{Capabilities, MediaInfo, MediaStream, StreamKind, ThumbnailRequest, Tonemapper};
use tokio::sync::{Semaphore, watch};

use crate::background::Background;
use crate::error::{ApiError, ApiResult};
use crate::ffmpeg::Ffmpeg;
use crate::matroska;
use crate::metrics::Metrics;
use crate::mks;
use crate::plan::{
    MAX_DURATION_MS, PLACEBO_CURVE, PLACEBO_FALLBACK_PEAK, PLACEBO_STATIC_PEAK, TextSource, ZSCALE_TONEMAP,
    untagged_hdr_fix,
};
use crate::probe::Prober;
use crate::util::{MEDIA_INPUT_OPTIONS, file_key, input_arg, secs, touch, write_atomically};
use crate::webvtt::{self, Event};

pub struct Extractor {
    ff: Arc<Ffmpeg>,
    prober: Arc<Prober>,
    metrics: Arc<Metrics>,
    subs_dir: PathBuf,
    /// Quick FFmpeg runs: thumbnails, cover art, subtitle files.
    limiter: Semaphore,
    /// Tracks read through the Cues at once (each with threads of its own):
    /// a text track's read holds its turn until it is over, a picture
    /// track's lets it go whenever it waits.
    cue_reads: Arc<Semaphore>,
    /// Reads at players elsewhere at once ([`Extractor::seek`]): apart from
    /// `cue_reads`, so that none waits for a track's read.
    seek_reads: Arc<Semaphore>,
    /// Reads at players elsewhere in a picture track under way
    /// ([`Extractor::seek`]): the rest of tracks' reads waits for them, as
    /// they read the same disks. Those on other disks wait too, for the
    /// second or so such a read takes.
    seeking: Arc<watch::Sender<usize>>,
    /// FFmpeg passes over whole files for their text tracks, which take
    /// hours for a big file on a slow share: apart from the quick runs, so
    /// they never hold those up.
    passes: Semaphore,
    /// The text tracks being extracted, by file version and index: the
    /// requests for a track wait for its extraction, which goes on without
    /// them...
    extractions: Mutex<HashMap<String, Extraction>>,
    /// The picture tracks being copied into Matroska files of their own, by
    /// [`picture_track_name`], each extracted once for all its requests.
    pictures: Mutex<HashMap<String, PictureExtraction>>,
    next_extraction: AtomicU64,
    /// ...and one FFmpeg pass per file (it writes every text track).
    file_locks: Locks,
    /// The latest text tracks the Cues couldn't give (by file version and
    /// index): FFmpeg extracts them without another try.
    unreadable: Mutex<VecDeque<String>>,
    /// The latest tracks whose pass over the whole file failed (by file
    /// version and index), when, and why: for a while, their requests hear
    /// why rather than start another pass each (players ask every few
    /// seconds, a pass on a slow share takes hours).
    failed_passes: Mutex<VecDeque<(String, Instant, Arc<ApiError>)>>,
    /// How often a pass marks its outputs as being written.
    keep_fresh: Duration,
    /// How long a pass over a file may take.
    pass_time: fn(&Path) -> Duration,
    /// How long a first answer for a picture track waits for FFmpeg to have
    /// written its header.
    header_wait: Duration,
    /// How long an answer for a player elsewhere waits for the read there
    /// ([`SEEK_WAIT`]).
    seek_wait: Duration,
    /// Playbacks starting, which picture tracks wait for (they read the same
    /// disk or share), and the background work subtitles a player waits for
    /// hold while they are read through the Cues.
    background: Option<Arc<Background>>,
}

/// FFmpeg passes over whole files at once: each reads a file from end to
/// end, so more would only share the disk or the network between them.
const PASSES: usize = 2;
/// How many text tracks the Cues couldn't give are remembered.
const UNREADABLE_KEPT: usize = 1_000;
/// How long a track whose pass failed answers with that failure.
const FAILED_PASS_KEPT: Duration = Duration::from_secs(30 * 60);
/// How many failed passes are remembered.
const FAILED_PASSES_KEPT: usize = 1_000;
/// How long a first answer for a picture track waits for FFmpeg to have
/// written its header (the extraction may wait for a playback starting,
/// FFmpeg for a sleeping disk), before it is a Matroska file with no block.
const HEADER_WAIT: Duration = Duration::from_secs(30);
/// How long an answer for a player elsewhere waits for the read there
/// ([`Extractor::seek`]): as long as such a read may take
/// ([`SEEK_READ_TIME`]), and a moment for the one before it. The pictures
/// the track's read has meanwhile are those around where it started, not
/// where this player is, and players that replace what they have with each
/// answer would lose the pictures they had there.
const SEEK_WAIT: Duration = Duration::from_secs(12);
/// How often an answer for a picture track looks at how far it has come.
const PICTURE_LOOK_EVERY: Duration = Duration::from_millis(100);

pub struct Image {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

impl Extractor {
    pub fn new(
        ff: Arc<Ffmpeg>,
        prober: Arc<Prober>,
        metrics: Arc<Metrics>,
        subs_dir: PathBuf,
        concurrency: usize,
    ) -> Self {
        let _ = std::fs::create_dir_all(&subs_dir);
        Self {
            ff,
            prober,
            metrics,
            subs_dir,
            limiter: Semaphore::new(concurrency.max(1)),
            cue_reads: Arc::new(Semaphore::new(concurrency.max(1))),
            seek_reads: Arc::new(Semaphore::new(concurrency.max(1))),
            seeking: Arc::new(watch::Sender::new(0)),
            passes: Semaphore::new(PASSES),
            extractions: Mutex::default(),
            pictures: Mutex::default(),
            next_extraction: AtomicU64::new(0),
            file_locks: Mutex::default(),
            unreadable: Mutex::default(),
            failed_passes: Mutex::default(),
            keep_fresh: KEEP_FRESH,
            pass_time: whole_file_timeout,
            header_wait: HEADER_WAIT,
            seek_wait: SEEK_WAIT,
            background: None,
        }
    }

    /// Picture tracks are extracted once no playback is starting, and reads
    /// through the Cues a player waits for hold background work.
    pub fn with_background(mut self, background: Arc<Background>) -> Self {
        self.background = Some(background);
        self
    }

    pub async fn thumbnail(&self, req: &ThumbnailRequest) -> ApiResult<Image> {
        let path = existing(&req.path)?;
        let hint = req.duration_ms.filter(|d| (1..=MAX_DURATION_MS).contains(d));
        // The probe (cached per file version) gives the default position, and
        // the colours of HDR video.
        let media = if req.hdr || (hint.is_none() && req.time_ms.is_none()) {
            self.prober.probe(&path).await.ok()
        } else {
            None
        };
        let duration = hint.or_else(|| media.as_ref().and_then(|m| m.duration_ms));
        let at = req.time_ms.unwrap_or_else(|| duration.map(|d| d / 5).unwrap_or(60_000));
        let at = match duration {
            Some(d) if d > 2_000 => at.clamp(0, d - 1_000),
            _ => at.max(0),
        };
        let width = req.width.unwrap_or(640).clamp(64, 1920);
        let plain = Picture::plain(width);
        let picture = if req.hdr {
            Picture::hdr(width, media.as_deref().and_then(MediaInfo::video), &self.ff.caps)
        } else {
            plain.clone()
        };

        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        Metrics::inc(&self.metrics.thumbnails);
        let (picture, mut bytes) = match self.grab_frame(&path, at, &picture).await {
            // Tone mapping can fail where a plain frame doesn't (no Vulkan
            // device, colours it can't convert): HDR colours beat no picture.
            Err(err) if picture.tone_mapped => {
                tracing::warn!("tone-mapped frame grab of {} failed, taking it as it is: {err:?}", path.display());
                let bytes = self.grab_frame(&path, at, &plain).await?;
                (plain, bytes)
            }
            result => (picture, result?),
        };
        if bytes.is_empty() && at > 0 {
            // Some files cannot seek precisely; fall back to the first frame.
            bytes = self.grab_frame(&path, 0, &picture).await?;
        }
        if bytes.is_empty() {
            return Err(ApiError::Failed("no frame could be decoded".into()));
        }
        Ok(Image { bytes, content_type: "image/jpeg" })
    }

    async fn grab_frame(&self, path: &Path, at_ms: i64, picture: &Picture) -> ApiResult<Vec<u8>> {
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error"]).args(picture.init);
        if at_ms > 0 {
            cmd.args(["-ss", &secs(at_ms)]);
        }
        cmd.args(MEDIA_INPUT_OPTIONS)
            .arg("-i")
            .arg(input_arg(path))
            .args(["-map", "0:V:0", "-frames:v", "1", "-an", "-sn", "-dn", "-vf", &picture.vf])
            .args(["-c:v", "mjpeg", "-q:v", "3", "-f", "image2pipe", "pipe:1"]);
        run_capture(cmd, Duration::from_secs(60)).await.map(|(out, _)| out)
    }

    /// Embedded cover art of an audio/video file.
    pub async fn cover(&self, path: &str) -> ApiResult<Image> {
        let path = existing(path)?;
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error"]).args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(&path)).args([
            "-map",
            "0:v:0",
            "-c",
            "copy",
            "-frames:v",
            "1",
            "-f",
            "image2pipe",
            "pipe:1",
        ]);
        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        let (bytes, _) = run_capture(cmd, Duration::from_secs(30)).await?;
        let content_type = sniff_image(&bytes).ok_or_else(|| ApiError::NotFound("no embedded cover art".into()))?;
        Ok(Image { bytes, content_type })
    }

    /// A subtitle track converted to WebVTT. `stream = None` means `path` is a
    /// subtitle file itself.
    pub async fn subtitle(self: &Arc<Self>, path: &str, stream: Option<i64>) -> ApiResult<String> {
        let path = existing(path)?;
        match stream {
            None => {
                Metrics::inc(&self.metrics.subtitle_extractions);
                self.convert_file(&path).await
            }
            Some(index) => self.embedded(&path, index).await,
        }
    }

    /// A subtitle track copied as it is into a Matroska file of its own, for
    /// players that show picture subtitles (PGS, VobSub, DVB) next to an HLS
    /// stream, whole: see [`Extractor::subtitle_matroska_so_far`].
    pub async fn subtitle_matroska(self: &Arc<Self>, path: &str, stream: Option<i64>) -> ApiResult<Vec<u8>> {
        let path = existing(path)?;
        let track = picture_track_name(&path, stream)?;
        let target = self.subs_dir.join(format!("{track}.mks"));
        if let Some(bytes) = cached_mks(&target).await {
            return Ok(bytes);
        }
        self.picture_extraction(&path, &track, stream, PictureStart::default())
            .result
            .await
            .map_err(|err| err.again())?;
        Ok(tokio::fs::read(&target).await?)
    }

    /// A picture subtitle track (`stream`, or the subtitle file at `path`
    /// itself: `.sup`, `.idx`) copied as it is into a Matroska file of its
    /// own, as far as it has been copied, and whether that is all of it,
    /// after `wait` at most. VobSub keeps its palette (the track's codec
    /// private data). Times are the original's from the start of the file
    /// (the file's start time taken off, as the HLS streams do it), so the
    /// track lines up with the playback position.
    ///
    /// The track's extraction (started now, unless it is under way) runs on
    /// its own, past this request. A Matroska file whose Cues list the
    /// track's blocks is read through them: those around where the player is
    /// (`start`) first, at once, answered with as soon as they are written;
    /// the rest once no playback is starting (a minute or so for a remux on
    /// a network share). Otherwise FFmpeg copies it out once no playback is
    /// starting, reading the whole file (minutes for a big one on a network
    /// share). Meanwhile the answer is those blocks (until FFmpeg's copy
    /// comes past them, when a read through the Cues failed), or else the
    /// Matroska FFmpeg has written so far, up to its last whole cluster, and
    /// players ask again for the rest. A first answer waits for the track's
    /// header to be written (the Matroska with no block, after
    /// [`HEADER_WAIT`]): every answer is a file players read.
    ///
    /// A player that says where it is (`start`), away from where the read
    /// through the Cues started, is answered with the blocks around there:
    /// those the read has, when it has them all, else a read of them at once
    /// beside it, waited for as long as such a read takes ([`SEEK_WAIT`]),
    /// or until the track's read has them. With `rest` (a player that
    /// reads the track once), the answer waits `wait` for the whole track,
    /// then is everything read through the Cues so far rather than the
    /// blocks around where the read started.
    pub async fn subtitle_matroska_so_far(
        self: &Arc<Self>,
        path: &str,
        stream: Option<i64>,
        wait: Duration,
        start: PictureStart,
        rest: bool,
    ) -> ApiResult<(Vec<u8>, bool)> {
        let path = existing(path)?;
        let track = picture_track_name(&path, stream)?;
        let target = self.subs_dir.join(format!("{track}.mks"));
        if let Some(bytes) = cached_mks(&target).await {
            return Ok((bytes, true));
        }
        let extraction = self.picture_extraction(&path, &track, stream, start.clone());
        let at_ms = start.from_ms;
        let mut seek = None;
        if let Some(index) = stream
            && start.asked
            && !near(extraction.from_ms, AROUND_START, start.from_ms)
        {
            // The same answer for the same place: a read there first (once
            // it is over) unless it went over its time, else what the
            // track's read has, else a read.
            let latest = extraction.seek.lock().clone().filter(|seek| seek.answers(start.from_ms));
            if latest.is_some() {
                seek = latest;
            } else if let Some(bytes) = extraction.so_far.lock().around(start.from_ms) {
                return Ok((bytes, false));
            } else {
                seek = self.seek(&extraction, &path, &track, index, start);
            }
        }
        let (done, first) = (extraction.result.clone(), extraction.first.clone());
        tokio::pin!(done);
        let (window, part) = (self.subs_dir.join(window_name(&track)), self.subs_dir.join(format!("{track}.mks.part")));
        let started = Instant::now();
        loop {
            tokio::select! {
                result = &mut done => {
                    result.map_err(|err| err.again())?;
                    return Ok((tokio::fs::read(&target).await?, true));
                }
                () = tokio::time::sleep(PICTURE_LOOK_EVERY) => {}
            }
            match seek.as_ref().map(|seek| seek.read.peek().cloned()) {
                Some(Some(Some(bytes))) => return Ok((bytes.to_vec(), false)),
                // Nothing read there (it can't be, it went over its budget,
                // or a later one left it out): what the track's read has
                // there, else what it has.
                Some(Some(None)) => {
                    seek = None;
                    if let Some(bytes) = extraction.so_far.lock().around(at_ms) {
                        return Ok((bytes, false));
                    }
                }
                // Read there as long as such a read takes ([`SEEK_WAIT`]),
                // unless the track's read comes past there meanwhile.
                Some(None) if started.elapsed() < self.seek_wait.min(self.header_wait) => {
                    if let Some(bytes) = extraction.so_far.lock().around(at_ms) {
                        return Ok((bytes, false));
                    }
                    continue;
                }
                _ => {}
            }
            let ready = started.elapsed() >= wait || (!rest && first.load(Ordering::Acquire));
            if ready && rest {
                if let Some(so_far) = so_far_bytes(&extraction.so_far).await {
                    return Ok((so_far, false));
                }
                // All of it is being written whole: it comes in a moment.
                if extraction.so_far.lock().writing && started.elapsed() < self.header_wait {
                    continue;
                }
            }
            // What the player wants first is there: no need to wait for more.
            let window_until_ms = extraction.from_ms.saturating_add(PICTURES_AFTER_MS);
            if ready && let Some(so_far) = picture_track_so_far(&window, &part, window_until_ms).await {
                return Ok((so_far, false));
            }
            if started.elapsed() >= self.header_wait {
                let codec = self.picture_codec(&path, stream).await;
                return Ok((mks::header_only(codec), false));
            }
        }
    }

    /// The extraction of picture track `track` under way, or one started
    /// now (from `start`, see [`Extractor::subtitle_matroska_so_far`]). It
    /// runs on its own, to the cache: requests waiting for it may go away. A
    /// track whose pass over the file failed lately has that failure for an
    /// answer.
    fn picture_extraction(
        self: &Arc<Self>,
        path: &Path,
        track: &str,
        stream: Option<i64>,
        start: PictureStart,
    ) -> PictureExtraction {
        let mut extractions = self.pictures.lock();
        if let Some(running) = extractions.get(track) {
            return running.clone();
        }
        let failed_key = format!("{track}.mks");
        let from_ms = start.from_ms;
        let read = PictureRead::default();
        if let Some(err) = self.failed_pass(&failed_key) {
            let failed = async move { Err(err) };
            return PictureExtraction::new(u64::MAX, from_ms, failed.boxed().shared(), read);
        }
        Metrics::inc(&self.metrics.subtitle_extractions);
        let id = self.next_extraction.fetch_add(1, Ordering::Relaxed);
        let (this, path, entry, reading) = (self.clone(), path.to_owned(), track.to_owned(), read.clone());
        let task = tokio::spawn(async move {
            let _over = ForgetPictures { extractor: this.clone(), track: entry.clone(), id };
            let mut by_ffmpeg = false;
            let result =
                this.extract_pictures(&path, &entry, stream, start, &reading, &mut by_ffmpeg).await.map_err(Arc::new);
            if let Err(err) = &result
                && by_ffmpeg
            {
                this.remember_failed_pass(&failed_key, err.clone());
            }
            result
        });
        let result = async move { task.await.unwrap_or_else(|err| Err(Arc::new(ApiError::Internal(err.into())))) };
        let extraction = PictureExtraction::new(id, from_ms, result.boxed().shared(), read);
        extractions.insert(track.to_owned(), extraction.clone());
        extraction
    }

    /// The blocks of picture track `index` around where a player is
    /// (`start`), read through the Cues beside the track's read
    /// (`extraction`): `None` when the track isn't read through its Cues.
    /// One is read at a time: a later one waits for it, and one a later one
    /// replaces meanwhile isn't read.
    fn seek(
        &self,
        extraction: &PictureExtraction,
        path: &Path,
        track: &str,
        index: i64,
        start: PictureStart,
    ) -> Option<Seek> {
        if self.unreadable.lock().iter().any(|t| t == track) {
            return None;
        }
        let mut latest = extraction.seek.lock();
        let (prober, seek_reads, path) = (self.prober.clone(), self.seek_reads.clone(), path.to_owned());
        let (seeking, seeks, background) = (self.seeking.clone(), extraction.seeks.clone(), self.background.clone());
        let before = latest.as_ref().map(|seek| seek.read.clone());
        let this_one = seeks.fetch_add(1, Ordering::Relaxed) + 1;
        let (at_ms, asked, path_name) = (start.from_ms, Instant::now(), path.display().to_string());
        let ran_out = Arc::new(AtomicBool::new(false));
        let over_time = ran_out.clone();
        let read = async move {
            if let Some(before) = before {
                before.await;
            }
            if seeks.load(Ordering::Relaxed) != this_one {
                return None;
            }
            let media = match start.media {
                Some(media) => media,
                None => prober.probe(&path).await.ok()?,
            };
            if !is_matroska(&media) {
                return None;
            }
            let permit = seek_reads.acquire_owned().await.ok()?;
            if seeks.load(Ordering::Relaxed) != this_one {
                return None;
            }
            let _seeking = Seeking::new(seeking);
            // A player waits for these: background work makes way.
            let _reading = background.as_ref().map(|background| background.subtitles());
            let read = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                window_through_cues(&path, &media, index, at_ms)
            })
            .await
            .ok()?;
            match read {
                Ok(window) => {
                    let took = asked.elapsed().as_millis();
                    tracing::debug!("picture subtitles of {path_name} at {at_ms} ms read in {took} ms");
                    Some(Arc::new(window))
                }
                Err(reason) => {
                    tracing::debug!("picture subtitles of {path_name} at {at_ms} ms not read: {reason}");
                    over_time.store(reason == matroska::TOOK_TOO_LONG, Ordering::Relaxed);
                    None
                }
            }
        }
        .boxed()
        .shared();
        // Read on its own: a player that goes away asks again.
        tokio::spawn(read.clone());
        let seek = Seek { at_ms, read, ran_out };
        *latest = Some(seek.clone());
        Some(seek)
    }

    /// Picture track `track` copied into `{track}.mks` in the cache: through
    /// the Cues of a Matroska file that lists its blocks (from around
    /// `start` first, into `read` as it goes), else by FFmpeg (`by_ffmpeg` is
    /// set then). What isn't wanted at once waits for playbacks starting:
    /// they read the same disk or share.
    async fn extract_pictures(
        &self,
        path: &Path,
        track: &str,
        stream: Option<i64>,
        start: PictureStart,
        read: &PictureRead,
        by_ffmpeg: &mut bool,
    ) -> ApiResult<()> {
        let target = self.subs_dir.join(format!("{track}.mks"));
        if cached_mks(&target).await.is_some() {
            return Ok(());
        }
        // What an earlier run left (the transcoder stopped): not this one's.
        let window = self.subs_dir.join(window_name(track));
        for left in [&window, &self.subs_dir.join(format!("{track}.mks.part"))] {
            let _ = tokio::fs::remove_file(left).await;
        }
        if let Some(index) = stream {
            let media = match start.media {
                Some(media) => media,
                None => self.prober.probe(path).await.map_err(|e| ApiError::Failed(format!("probe failed: {e:#}")))?,
            };
            let picture = media
                .streams
                .iter()
                .any(|s| s.index == index && s.kind == StreamKind::Subtitle && !s.is_text_subtitle());
            if !picture {
                return Err(ApiError::BadRequest(format!("stream {index} is not a picture subtitle track")));
            }
            if self.pictures_through_cues(path, track, &media, index, start.from_ms, read, &target).await? {
                return Ok(());
            }
        }
        if let Some(background) = &self.background {
            background.wait_for_starts().await;
        }
        *by_ffmpeg = true;
        let copied = self.pictures_through_ffmpeg(path, track, stream, &target).await;
        // A window a failed read through the Cues left stood in till now.
        let _ = tokio::fs::remove_file(&window).await;
        copied
    }

    /// Picture track `index` of a Matroska file read through its Cues (from
    /// around `from_ms` first, written whole to its window, then `first`
    /// set; each stretch into `so_far` as it is read) and written to
    /// `target`: whether the file allowed it (else FFmpeg has to read it).
    #[allow(clippy::too_many_arguments)]
    async fn pictures_through_cues(
        &self,
        path: &Path,
        track: &str,
        media: &Arc<MediaInfo>,
        index: i64,
        from_ms: i64,
        read: &PictureRead,
        target: &Path,
    ) -> ApiResult<bool> {
        if !is_matroska(media) || self.unreadable.lock().iter().any(|t| t == track) {
            return Ok(false);
        }
        let cue_reads = self.cue_reads.clone();
        let permit = cue_reads.clone().acquire_owned().await.map_err(|e| ApiError::Internal(e.into()))?;
        // A player waits for these: background work makes way.
        let reading = self.background.as_ref().map(|background| background.subtitles());
        let window = self.subs_dir.join(window_name(track));
        let (file, media, target, first) = (path.to_owned(), media.clone(), target.to_owned(), read.first.clone());
        let (so_far, around_read) = (read.so_far.clone(), read.first.clone());
        let (background, runtime, seeking) =
            (self.background.clone(), tokio::runtime::Handle::current(), self.seeking.clone());
        let read = tokio::task::spawn_blocking(move || {
            // Its turn let go of whenever it waits, and taken again after:
            // another track's first blocks, and text tracks, needn't wait for
            // this one's rest (minutes for a remux on a busy NAS).
            let permit = Mutex::new(Some(permit));
            // What a player waits for, which may read the same disks: a
            // playback starting, the pictures at a player's seek.
            let busy = || *seeking.borrow() > 0 || background.as_ref().is_some_and(|b| b.starting());
            // Its turn let go of until that is over and the turn comes back:
            // one that starts while it waits for the turn goes first too, the
            // turn given back. How long it waited.
            let wait = || {
                let started = Instant::now();
                drop(permit.lock().take());
                let mut seeking = seeking.subscribe();
                runtime.block_on(async {
                    loop {
                        if let Some(background) = &background {
                            background.wait_for_starts().await;
                        }
                        let _ = seeking.wait_for(|reads| *reads == 0).await;
                        let turn = cue_reads.clone().acquire_owned().await.ok();
                        if !busy() {
                            *permit.lock() = turn;
                            break;
                        }
                    }
                });
                started.elapsed()
            };
            let first_written = || {
                first.store(true, Ordering::Release);
                wait()
            };
            // Read by read too, within a stretch.
            let gate = Mutex::new(());
            let aside = |budget: &matroska::Budget| step_aside(&gate, busy, wait, budget);
            write_through_cues(&file, &media, index, from_ms, &so_far, &window, &target, first_written, wait, &aside)
        })
        .await
        .unwrap_or_else(|e| Err(format!("the read through the cues failed: {e}")));
        drop(reading);
        match read {
            Ok(()) => Ok(true),
            Err(reason) => {
                // It starts a pass over the whole file: minutes for a big one
                // on a network share.
                tracing::info!("picture subtitles of {} are copied by ffmpeg: {reason}", path.display());
                // A read that got the blocks around the player read through
                // these Cues (a busy disk most often runs it out of time):
                // reads at seeks go on while FFmpeg copies the track.
                if !around_read.load(Ordering::Acquire) {
                    let mut unreadable = self.unreadable.lock();
                    if unreadable.len() == UNREADABLE_KEPT {
                        unreadable.pop_front();
                    }
                    unreadable.push_back(track.to_owned());
                }
                Ok(false)
            }
        }
    }

    /// A picture track copied by FFmpeg, which reads the whole file: a pass,
    /// which waits for its turn. Written cluster by cluster to
    /// `{track}.mks.part` ([`Extractor::subtitle_matroska_so_far`] reads it
    /// meanwhile), renamed to `target` once whole. Written to a file, not a
    /// pipe, so the Matroska has its Cues and duration and players can seek
    /// in it. Only subtitles come out: any other track leaves FFmpeg nothing
    /// to write, and it fails.
    async fn pictures_through_ffmpeg(
        &self,
        path: &Path,
        track: &str,
        stream: Option<i64>,
        target: &Path,
    ) -> ApiResult<()> {
        let map = match stream {
            Some(index) => format!("0:{index}"),
            None => "0:s:0".to_owned(),
        };
        let part = self.subs_dir.join(format!("{track}.mks.part"));
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error"])
            .args(MEDIA_INPUT_OPTIONS)
            .arg("-i")
            .arg(input_arg(path))
            .args(["-map", &map, "-vn", "-an", "-dn", "-c", "copy", "-flush_packets", "1", "-f", "matroska", "-y"])
            // Stopped just past the most that is kept.
            .args(["-fs", &(MAX_EXTRACTED + 1).to_string()])
            .arg(input_arg(&part));
        let _permit = self.passes.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        // A window a failed read through the Cues left is served meanwhile.
        let window = self.subs_dir.join(window_name(track));
        let written = || mark_written(&[part.as_path(), window.as_path()]);
        let result = kept_fresh(run_capture(cmd, (self.pass_time)(path)), self.keep_fresh, written).await;
        let result = match result {
            Ok(_) => match tokio::fs::metadata(&part).await {
                Ok(made) if made.len() > MAX_EXTRACTED => {
                    Err(ApiError::Failed(format!("the subtitle track is over {} MB", MAX_EXTRACTED >> 20)))
                }
                Ok(_) => match flush(&part).await {
                    Ok(()) => tokio::fs::rename(&part, target).await.map_err(ApiError::from),
                    Err(err) => Err(err.into()),
                },
                Err(err) => Err(err.into()),
            },
            Err(err) => Err(err),
        };
        if result.is_err() {
            let _ = tokio::fs::remove_file(&part).await;
        }
        result
    }

    /// The Matroska codec ID of a picture track, for a file with no block
    /// yet: the probe's (cached), a subtitle file's by its extension; PGS
    /// when neither says.
    async fn picture_codec(&self, path: &Path, stream: Option<i64>) -> &'static str {
        let codec = match stream {
            Some(index) => match self.prober.probe(path).await {
                Ok(media) => {
                    media.streams.iter().find(|s| s.index == index).and_then(|s| matroska::picture_codec_id(&s.codec))
                }
                Err(_) => None,
            },
            None => {
                let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                matches!(ext.as_str(), "idx" | "sub").then_some("S_VOBSUB")
            }
        };
        codec.unwrap_or("S_HDMV/PGS")
    }

    /// An embedded text track's WebVTT as far as it has been pulled out of the
    /// file, and whether that is all of it, after `wait` at most: the track's
    /// extraction (started now, unless it is under way) runs on its own, past
    /// this request. A Matroska file whose Cues list the track's blocks is
    /// read whole by then, as a rule. Otherwise FFmpeg reads the whole file
    /// (minutes for a big one on a network share): once it does, the cues it
    /// has found come at once, and players ask again for those found since.
    pub async fn subtitle_so_far(
        self: &Arc<Self>,
        path: &str,
        index: i64,
        wait: Duration,
    ) -> ApiResult<(String, bool)> {
        let path = existing(path)?;
        let key = file_key(&path)?;
        let target = self.subs_dir.join(format!("{key}_{index}.vtt"));
        if let Some(text) = cached_vtt(&target).await {
            return Ok((text, true));
        }
        let extraction = self.extraction(&path, &key, index);
        if !extraction.by_ffmpeg.load(Ordering::Relaxed)
            && let Ok(done) = tokio::time::timeout(wait, extraction.done()).await
        {
            return done.map(|text| (text, true));
        }
        // Still under way: the cues FFmpeg has found, when it is reading the file.
        let part = self.subs_dir.join(format!("{key}_{index}.vtt.part"));
        match tokio::fs::read_to_string(&part).await {
            Ok(text) => Ok((whole_cues(&text), false)),
            // Finished (and renamed) meanwhile, or not started yet.
            Err(_) => match cached_vtt(&target).await {
                Some(text) => Ok((text, true)),
                None => Ok(("WEBVTT\n\n".to_owned(), false)),
            },
        }
    }

    async fn convert_file(&self, path: &Path) -> ApiResult<String> {
        let size = tokio::fs::metadata(path).await?.len();
        if size > MAX_SUBTITLE_FILE {
            return Err(ApiError::BadRequest(format!(
                "the subtitle file is {} MB: text subtitles take {} MB at most",
                size >> 20,
                MAX_SUBTITLE_FILE >> 20
            )));
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let head = if ext == "vtt" {
            match String::from_utf8(tokio::fs::read(path).await?) {
                Ok(text) => {
                    let text = text.trim_start_matches('\u{feff}').to_owned();
                    return Ok(if text.starts_with("WEBVTT") { text } else { format!("WEBVTT\n\n{text}") });
                }
                Err(err) => {
                    let mut raw = err.into_bytes();
                    raw.truncate(CHARSET_SAMPLE);
                    raw
                }
            }
        } else {
            read_head(path, CHARSET_SAMPLE).await?
        };
        let key = file_key(path)?;
        // Not `_file.vtt`: earlier versions wrote those in place, so a crash
        // or a full disk may have left them truncated.
        let cached = self.subs_dir.join(format!("{key}_sidecar.vtt"));
        if let Some(text) = cached_vtt(&cached).await {
            return Ok(text);
        }
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error"]);
        if let Some(charset) = guess_charset(&head) {
            cmd.args(["-sub_charenc", charset]);
        }
        cmd.args(MEDIA_INPUT_OPTIONS)
            .arg("-i")
            .arg(input_arg(path))
            .args(["-map", "0:s:0", "-c:s", "webvtt", "-f", "webvtt", "pipe:1"]);
        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        let (out, _) = run_capture(cmd, Duration::from_secs(60)).await?;
        let text = String::from_utf8_lossy(&out).into_owned();
        if !text.starts_with("WEBVTT") {
            return Err(ApiError::Failed("subtitle conversion produced no output".into()));
        }
        // Cached whole or not at all: a half-written file would keep being served.
        let data = text.clone().into_bytes();
        let write = tokio::task::spawn_blocking(move || write_atomically(&cached, &data)).await;
        if let Ok(Err(err)) = write {
            tracing::debug!("could not cache converted subtitles of {}: {err}", path.display());
        }
        Ok(text)
    }

    /// An embedded text track, whole: see [`Extractor::extract`].
    async fn embedded(self: &Arc<Self>, path: &Path, index: i64) -> ApiResult<String> {
        let key = file_key(path)?;
        if let Some(text) = cached_vtt(&self.subs_dir.join(format!("{key}_{index}.vtt"))).await {
            return Ok(text);
        }
        self.extraction(path, &key, index).done().await
    }

    /// The extraction of text track `index` of a file version (`key`) under
    /// way, or one started now. It runs on its own, to the cache: requests
    /// waiting for it may go away. A track whose pass over the file failed
    /// lately has that failure for an answer.
    fn extraction(self: &Arc<Self>, path: &Path, key: &str, index: i64) -> Extraction {
        let track = format!("{key}_{index}");
        let mut extractions = self.extractions.lock();
        if let Some(running) = extractions.get(&track) {
            return running.clone();
        }
        if let Some(err) = self.failed_pass(&track) {
            let failed = async move { Err(err) };
            return Extraction { id: u64::MAX, by_ffmpeg: Arc::default(), result: failed.boxed().shared() };
        }
        Metrics::inc(&self.metrics.subtitle_extractions);
        let id = self.next_extraction.fetch_add(1, Ordering::Relaxed);
        let by_ffmpeg = Arc::new(AtomicBool::new(false));
        let (this, path, key, entry) = (self.clone(), path.to_owned(), key.to_owned(), track.clone());
        let passing = by_ffmpeg.clone();
        let task = tokio::spawn(async move {
            let over = Forget { extractor: this.clone(), track: entry, id };
            let result = this.extract(&path, &key, index, &passing).await.map(Arc::new).map_err(Arc::new);
            // Before the extraction is over: no request in between starts
            // another pass.
            if let Err(err) = &result
                && passing.load(Ordering::Relaxed)
            {
                this.remember_failed_pass(&over.track, err.clone());
            }
            result
        });
        let result = async move { task.await.unwrap_or_else(|err| Err(Arc::new(ApiError::Internal(err.into())))) };
        let extraction = Extraction { id, by_ffmpeg, result: result.boxed().shared() };
        extractions.insert(track, extraction.clone());
        extraction
    }

    /// Why the latest pass over a whole file for `track` failed, when it did
    /// within [`FAILED_PASS_KEPT`].
    fn failed_pass(&self, track: &str) -> Option<Arc<ApiError>> {
        let failed = self.failed_passes.lock();
        let (_, at, err) = failed.iter().rev().find(|(t, ..)| t == track)?;
        (at.elapsed() < FAILED_PASS_KEPT).then(|| err.clone())
    }

    fn remember_failed_pass(&self, track: &str, err: Arc<ApiError>) {
        tracing::debug!("subtitle track {track}: its pass failed, said for {} min", FAILED_PASS_KEPT.as_secs() / 60);
        let mut failed = self.failed_passes.lock();
        failed.retain(|(t, at, _)| t != track && at.elapsed() < FAILED_PASS_KEPT);
        if failed.len() >= FAILED_PASSES_KEPT {
            failed.pop_front();
        }
        failed.push_back((track.to_owned(), Instant::now(), err));
    }

    /// Text track `index` of a file, cached: read through its Cues when they
    /// list its blocks (the track asked for alone: each read is a seek), and
    /// otherwise with every other text track of the file in a single pass of
    /// FFmpeg over it (`by_ffmpeg` is set then).
    async fn extract(&self, path: &Path, key: &str, index: i64, by_ffmpeg: &AtomicBool) -> ApiResult<String> {
        // Made meanwhile: by an extraction just over, or another track's pass.
        if let Some(text) = cached_vtt(&self.subs_dir.join(format!("{key}_{index}.vtt"))).await {
            return Ok(text);
        }
        let media = self.text_track(path, index).await?;
        match self.through_cues(path, key, &media, index).await? {
            Some(text) => Ok(text),
            None => {
                by_ffmpeg.store(true, Ordering::Relaxed);
                self.through_ffmpeg(path, key, &media, index).await
            }
        }
    }

    /// Text track `index` of a file read through its Cues, and cached; `None`
    /// when the file doesn't allow that (FFmpeg has to read it then).
    async fn through_cues(
        &self,
        path: &Path,
        key: &str,
        media: &Arc<MediaInfo>,
        index: i64,
    ) -> ApiResult<Option<String>> {
        let track = format!("{key}_{index}");
        // Only Matroska files have Cues, and a track they couldn't give isn't
        // tried again: no read waits for its turn for those.
        let matroska = media.format_name.as_deref().is_some_and(|names| names.split(',').any(|n| n == "matroska"));
        if !matroska || self.unreadable.lock().contains(&track) {
            return Ok(None);
        }
        let permit = self.cue_reads.clone().acquire_owned().await.map_err(|e| ApiError::Internal(e.into()))?;
        // A player waits for these: background work makes way.
        let reading = self.background.as_ref().map(|background| background.subtitles());
        let (file, media, target) = (path.to_owned(), media.clone(), self.subs_dir.join(format!("{track}.vtt")));
        let read = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            read_through_cues(&file, &media, index, &target)
        })
        .await
        .unwrap_or_else(|e| Err(format!("the read through the cues failed: {e}")));
        drop(reading);
        match read {
            Ok(text) => Ok(Some(text)),
            Err(reason) => {
                tracing::debug!("subtitles of {} extracted by ffmpeg: {reason}", path.display());
                let mut unreadable = self.unreadable.lock();
                if unreadable.len() == UNREADABLE_KEPT {
                    unreadable.pop_front();
                }
                unreadable.push_back(track);
                Ok(None)
            }
        }
    }

    /// Text track `index` of a file as FFmpeg extracts it, with every other
    /// text track of the file in the same pass over it.
    async fn through_ffmpeg(&self, path: &Path, key: &str, media: &MediaInfo, index: i64) -> ApiResult<String> {
        let target = self.subs_dir.join(format!("{key}_{index}.vtt"));
        let entry = Lock::acquire(&self.file_locks, key);
        let _guard = entry.lock.lock().await;
        if let Some(text) = cached_vtt(&target).await {
            return Ok(text);
        }
        let text_streams: Vec<i64> = media.streams.iter().filter(|s| s.is_text_subtitle()).map(|s| s.index).collect();
        let _permit = self.passes.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        match self.extract_vtt(path, key, &text_streams).await {
            Err(err) if retry_alone(&err, text_streams.len()) => {
                tracing::debug!("subtitles of {} extracted alone: {err:?}", path.display());
                self.extract_vtt(path, key, &[index]).await?;
            }
            all => all?,
        }
        Ok(tokio::fs::read_to_string(&target).await?)
    }

    /// The media facts of a file (cached per file version), checking that
    /// stream `index` is a text track.
    async fn text_track(&self, path: &Path, index: i64) -> ApiResult<Arc<MediaInfo>> {
        let media = self.prober.probe(path).await.map_err(|e| ApiError::Failed(format!("probe failed: {e:#}")))?;
        if !media.streams.iter().any(|s| s.index == index && s.is_text_subtitle()) {
            return Err(ApiError::BadRequest(format!("stream {index} is not a text subtitle track")));
        }
        Ok(media)
    }

    async fn extract_vtt(&self, path: &Path, key: &str, streams: &[i64]) -> ApiResult<()> {
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error", "-y"]).args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(path));
        let mut outputs = Vec::new();
        for index in streams {
            let part = self.subs_dir.join(format!("{key}_{index}.vtt.part"));
            // Each cue is written as soon as it is read, for `subtitle_so_far`.
            cmd.args(["-map", &format!("0:{index}"), "-c:s", "webvtt", "-flush_packets", "1", "-f", "webvtt"])
                .arg(&part);
            outputs.push((part, self.subs_dir.join(format!("{key}_{index}.vtt"))));
        }
        let parts: Vec<&Path> = outputs.iter().map(|(part, _)| part.as_path()).collect();
        let result =
            kept_fresh(run_capture(cmd, (self.pass_time)(path)), self.keep_fresh, || mark_written(&parts)).await;
        for (part, done) in &outputs {
            // Only complete files, flushed to disk, get their final name.
            if result.is_ok() && flush(part).await.is_ok() {
                let _ = tokio::fs::rename(part, done).await;
            } else {
                let _ = tokio::fs::remove_file(part).await;
            }
        }
        result.map(|_| ())
    }

    /// Write a subtitle track as ASS into a job directory for libass burn-in.
    /// This runs while the job is made, which the server waits 25 minutes
    /// for: after 20 (a big file on a slow share), it gives up with an error
    /// the server still hears.
    pub async fn extract_ass(&self, input: &Path, source: &TextSource, out: &Path) -> ApiResult<()> {
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error", "-y"]);
        match source {
            TextSource::Embedded(index) => {
                cmd.args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(input)).args(["-map", &format!("0:{index}")]);
            }
            TextSource::File(path) => {
                if let Ok(head) = read_head(path, CHARSET_SAMPLE).await
                    && let Some(charset) = guess_charset(&head)
                {
                    cmd.args(["-sub_charenc", charset]);
                }
                cmd.args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(path)).args(["-map", "0:s:0"]);
            }
        }
        cmd.args(["-c:s", "ass", "-f", "ass"]).arg(out);
        run_capture(cmd, Duration::from_secs(20 * 60)).await.map(|_| ())
    }

    /// The folder of the fonts attached to `input` (a Matroska file), for
    /// libass to draw burned-in ASS subtitles with the fonts they name;
    /// `None` when it has none. Read once per file version, from the
    /// Attachments element alone (the seek head says where), into the
    /// subtitle cache; whatever goes wrong only means the system's fonts.
    pub async fn attached_fonts(&self, input: &Path) -> Option<PathBuf> {
        let (input, subs_dir) = (input.to_owned(), self.subs_dir.clone());
        let found = tokio::task::spawn_blocking(move || cached_fonts(&input, &subs_dir)).await;
        match found {
            Ok(Ok(dir)) => dir,
            Ok(Err(err)) => {
                tracing::debug!("no attached fonts: {err}");
                None
            }
            Err(_) => None,
        }
    }
}

/// The most attached fonts kept for a file: CJK fonts take tens of
/// megabytes each, and an anime series may attach a few of them.
const MAX_FONT_BYTES: u64 = 256 << 20;

/// [`Extractor::attached_fonts`]: `{key}.fonts/fonts` in `subs_dir` (libass
/// takes every file of its folder for a font), beside an `index.json`
/// listing them, which says the file was read (an empty list, that it has
/// none) and which use refreshes for the cache's pruning. Blocking.
fn cached_fonts(input: &Path, subs_dir: &Path) -> std::io::Result<Option<PathBuf>> {
    let dir = subs_dir.join(format!("{}.fonts", file_key(input)?));
    let index = dir.join("index.json");
    if let Ok(listed) = std::fs::read(&index) {
        touch(&index);
        let names: Vec<String> = serde_json::from_slice(&listed).unwrap_or_default();
        return Ok((!names.is_empty()).then(|| dir.join("fonts")));
    }
    let fonts = matroska::attached_fonts(input, MAX_FONT_BYTES).unwrap_or_default();
    // Made whole beside it, then renamed into place: a reader never sees
    // half of it, and of two made at once, one stays.
    let part = subs_dir.join(format!(".{}.fonts.tmp", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(part.join("fonts"))?;
    let made = (|| {
        let mut names = Vec::new();
        for (n, font) in fonts.iter().enumerate() {
            // Numbered: libass knows a font by what's inside it, and the
            // file's names may be anything.
            let extension = font_extension(&font.name);
            let name = format!("{n}.{extension}");
            std::fs::write(part.join("fonts").join(&name), &font.data)?;
            names.push(name);
        }
        std::fs::write(part.join("index.json"), serde_json::to_vec(&names).unwrap_or_default())?;
        std::fs::rename(&part, &dir)
    })();
    if made.is_err() {
        let _ = std::fs::remove_dir_all(&part);
        if !index.exists() {
            made?;
        }
    }
    Ok((!fonts.is_empty()).then(|| dir.join("fonts")))
}

/// The extension a font is saved with: its own when it's a font's.
fn font_extension(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "otf" => "otf",
        "ttc" => "ttc",
        "otc" => "otc",
        _ => "ttf",
    }
}

/// A text track's extraction under way, and which one it is.
#[derive(Clone)]
struct Extraction {
    id: u64,
    /// Set once FFmpeg reads the whole file for the track: that may take
    /// hours, and the cues it has found are in the track's `.part` file.
    by_ffmpeg: Arc<AtomicBool>,
    result: Shared<BoxFuture<'static, Result<Arc<String>, Arc<ApiError>>>>,
}

impl Extraction {
    /// The track's WebVTT once the extraction is over, or why it couldn't be
    /// made.
    async fn done(self) -> ApiResult<String> {
        self.result.await.map(|text| String::clone(&text)).map_err(|err| err.again())
    }
}

/// Where a picture track's copy starts: where the player is (ms of the
/// track's time; the blocks around it come first), and the file's media
/// information when the caller has it (no probe then, while a playback
/// starts).
#[derive(Clone, Default)]
pub struct PictureStart {
    pub from_ms: i64,
    pub media: Option<Arc<MediaInfo>>,
    /// Whether the player said where it is.
    pub asked: bool,
}

impl PictureStart {
    /// From `from_ms`, with `media` when it says how long the file is and
    /// when it starts (as a job's media must).
    pub fn new(from_ms: Option<i64>, media: Option<MediaInfo>) -> Self {
        let media = media.filter(|m| {
            m.duration_ms.is_some_and(|d| d > 0 && d <= MAX_DURATION_MS)
                && m.start_ms.is_some()
                && !m.streams.is_empty()
        });
        Self { from_ms: from_ms.unwrap_or(0), media: media.map(Arc::new), asked: from_ms.is_some() }
    }
}

/// A picture track's extraction under way, and which one it is.
#[derive(Clone)]
struct PictureExtraction {
    id: u64,
    /// Where its read through the Cues starts (ms of the track's time).
    from_ms: i64,
    result: Shared<BoxFuture<'static, Result<(), Arc<ApiError>>>>,
    /// Set once the blocks from where the player is are written (read
    /// through the Cues): an answer needn't wait any longer.
    first: Arc<AtomicBool>,
    /// The track read through the Cues so far.
    so_far: Arc<Mutex<SoFar>>,
    /// The latest read at a player elsewhere...
    seek: Arc<Mutex<Option<Seek>>>,
    /// ...and how many there have been.
    seeks: Arc<AtomicU64>,
}

/// What an extraction's read through the Cues shares with its answers.
#[derive(Clone, Default)]
struct PictureRead {
    first: Arc<AtomicBool>,
    so_far: Arc<Mutex<SoFar>>,
}

impl PictureExtraction {
    fn new(
        id: u64,
        from_ms: i64,
        result: Shared<BoxFuture<'static, Result<(), Arc<ApiError>>>>,
        read: PictureRead,
    ) -> Self {
        let PictureRead { first, so_far } = read;
        Self { id, from_ms, result, first, so_far, seek: Arc::default(), seeks: Arc::default() }
    }
}

/// A picture track as read through the Cues so far: its codec and header,
/// the blocks of the stretches read (each stretch in order of time), and
/// the spans of the track's time whose blocks are all read.
#[derive(Default)]
struct SoFar {
    track: Option<(&'static str, Vec<u8>)>,
    blocks: Vec<mks::Block>,
    /// Apart from one another, from and until (ms).
    spans: Vec<(i64, i64)>,
    /// Its blocks are taken out to be written to the track's file: whole in
    /// a moment, unless that fails.
    writing: bool,
}

impl SoFar {
    /// Takes in a stretch read.
    fn add(&mut self, stretch: Stretch) {
        self.track.get_or_insert((stretch.codec, stretch.header));
        self.blocks.extend(stretch.blocks);
        // Stretches read one after the other make one span.
        let Some((mut from, mut until)) = stretch.span else { return };
        self.spans.retain(|&(a, b)| {
            let apart = b < from || a > until;
            if !apart {
                (from, until) = (from.min(a), until.max(b));
            }
            apart
        });
        self.spans.push((from, until));
    }

    /// The blocks around a player at `at_ms` ([`AROUND_SEEK`]) as a
    /// Matroska file, when they are all read.
    fn around(&self, at_ms: i64) -> Option<Vec<u8>> {
        let (codec, header) = self.track.as_ref()?;
        let (from, until) = (at_ms.saturating_sub(AROUND_SEEK.0), at_ms.saturating_add(AROUND_SEEK.1));
        self.spans.iter().find(|&&(a, b)| a <= from && until <= b)?;
        let mut blocks: Vec<mks::Block> =
            self.blocks.iter().filter(|b| (from..until).contains(&b.time_ms)).cloned().collect();
        blocks.sort_by_key(|b| b.time_ms);
        Some(mks::to_bytes(codec, header, &blocks))
    }

    /// A copy of what is read so far: the track's codec and header, and
    /// the blocks.
    fn copy(&self) -> Option<(&'static str, Vec<u8>, Vec<mks::Block>)> {
        let (codec, header) = self.track.clone()?;
        Some((codec, header, self.blocks.clone()))
    }
}

/// What a read through the Cues has read so far ([`SoFar`]), in order of
/// time, as a Matroska file; `None` before its first stretch is. Copied
/// under the lock, made off the async workers: it is tens of megabytes for
/// a Blu-ray's track.
async fn so_far_bytes(so_far: &Mutex<SoFar>) -> Option<Vec<u8>> {
    let (codec, header, mut blocks) = so_far.lock().copy()?;
    tokio::task::spawn_blocking(move || {
        blocks.sort_by_key(|b| b.time_ms);
        mks::to_bytes(codec, &header, &blocks)
    })
    .await
    .ok()
}

/// A read at a player elsewhere under way, counted in
/// [`Extractor::seeking`] while it lasts.
struct Seeking(Arc<watch::Sender<usize>>);

impl Seeking {
    fn new(reads: Arc<watch::Sender<usize>>) -> Self {
        reads.send_modify(|n| *n += 1);
        Self(reads)
    }
}

impl Drop for Seeking {
    fn drop(&mut self) {
        self.0.send_modify(|n| *n -= 1);
    }
}

/// The blocks of a picture track around where a player is, read beside the
/// track's read ([`Extractor::seek`]).
#[derive(Clone)]
struct Seek {
    /// Where the player is (ms of the track's time).
    at_ms: i64,
    /// The blocks as a Matroska file once read; `None` when they couldn't be.
    read: Shared<BoxFuture<'static, Option<Arc<Vec<u8>>>>>,
    /// Set when the read went over its time ([`matroska::TOOK_TOO_LONG`]).
    ran_out: Arc<AtomicBool>,
}

impl Seek {
    /// Whether this read is the answer for a player at `at_ms`: near it,
    /// unless it went over its time. The disk was busy then, and the
    /// player's next ask reads there again. One that got nothing otherwise
    /// would get nothing again (the blocks there can't be read through the
    /// Cues, or are more than its budget), and stays the answer.
    fn answers(&self, at_ms: i64) -> bool {
        near(self.at_ms, AROUND_SEEK, at_ms) && !self.ran_out.load(Ordering::Relaxed)
    }
}

/// Whether a player at `at_ms` has what it needs in the blocks read around
/// `from_ms` (`around`: that long before it and after it): what shows
/// there, and a while more, as it asks again every few seconds.
fn near(from_ms: i64, (before, after): (i64, i64), at_ms: i64) -> bool {
    let first = from_ms.saturating_sub(before / 2);
    let last = from_ms.saturating_add(after / 2);
    (first..=last).contains(&at_ms)
}

/// [`Forget`] for a picture track.
struct ForgetPictures {
    extractor: Arc<Extractor>,
    track: String,
    id: u64,
}

impl Drop for ForgetPictures {
    fn drop(&mut self) {
        let mut extractions = self.extractor.pictures.lock();
        if extractions.get(&self.track).is_some_and(|extraction| extraction.id == self.id) {
            extractions.remove(&self.track);
        }
    }
}

/// What a picture track's files in the cache are named after: the file's
/// version, and the track's index (`file`: the file is a subtitle file).
fn picture_track_name(path: &Path, stream: Option<i64>) -> ApiResult<String> {
    let key = file_key(path)?;
    Ok(match stream {
        Some(index) => format!("{key}_{index}"),
        None => format!("{key}_file"),
    })
}

/// A cached picture track, when it is a whole Matroska file (they are
/// renamed into place whole: the EBML header says enough).
async fn cached_mks(path: &Path) -> Option<Vec<u8>> {
    let bytes = tokio::fs::read(path).await.ok()?;
    if !bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return None;
    }
    // In use: cache pruning keeps it.
    touch(path);
    Some(bytes)
}

/// The Matroska FFmpeg is writing at `part`, as far as it can be read, and
/// how far into the track's time that goes ([`mks::readable_to`]); `None`
/// before its header is there.
async fn readable_mks(part: &Path) -> Option<(Vec<u8>, Option<i64>)> {
    let mut bytes = tokio::fs::read(part).await.ok()?;
    let (end, until_ms) = mks::readable_to(&bytes)?;
    bytes.truncate(end);
    Some((bytes, until_ms))
}

/// Where the blocks of picture track `track` around where the player is are
/// kept while the rest is read ([`write_through_cues`]): a temporary.
fn window_name(track: &str) -> String {
    format!("{track}.window.part")
}

/// What players are given of a picture track that isn't whole yet: the
/// blocks around where the player is (written whole at `window`, those
/// before `window_until_ms`), else what FFmpeg has written of it at `part`.
/// While FFmpeg copies the track after a read through the Cues failed, the
/// window stays until FFmpeg's copy has come past it: players keep the
/// pictures they were given, then have those after them as FFmpeg goes on.
async fn picture_track_so_far(window: &Path, part: &Path, window_until_ms: i64) -> Option<Vec<u8>> {
    let window = match tokio::fs::read(window).await {
        Ok(bytes) if mks::readable(&bytes) == Some(bytes.len()) => bytes,
        _ => return readable_mks(part).await.map(|(bytes, _)| bytes),
    };
    match readable_mks(part).await {
        Some((bytes, Some(until_ms))) if until_ms >= window_until_ms => Some(bytes),
        _ => Some(window),
    }
}

/// The most a picture track read through the Cues keeps in memory (see
/// [`matroska::Budget`]): a Blu-ray's track takes tens of megabytes, its
/// commentary's as much again. A file whose Cues claim more is left to
/// FFmpeg, which copies it a packet at a time.
const PICTURE_READ_BYTES: u64 = 192 << 20;

/// How long reading a picture track through the Cues of a file of `bytes`
/// may take: as long as FFmpeg, which reads the whole file instead, would
/// take at a gigabit network's pace (100 MB/s), a minute at least. A
/// Blu-ray's track takes thousands of reads, each a seek on a disk: a minute
/// or so on a busy NAS, where FFmpeg would take a quarter of an hour over a
/// remux.
fn picture_read_time(bytes: u64) -> Duration {
    CUE_READ_TIME.max(Duration::from_secs(bytes / 100_000_000))
}

/// How long reading a picture track through the Cues of the file at `path`
/// may take: see [`picture_read_time`].
fn picture_read_timeout(path: &Path) -> Duration {
    picture_read_time(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

/// The picture subtitles read through the Cues first: from this long before
/// where the player is (what shows there may have started a while before)
/// to this long after it (the rest comes after; each block more is a read,
/// which is what the first answer waits for).
const PICTURES_BEFORE_MS: i64 = 30_000;
const PICTURES_AFTER_MS: i64 = 2 * 60_000;
/// The blocks read around where a playback starts: see the two above.
const AROUND_START: (i64, i64) = (PICTURES_BEFORE_MS, PICTURES_AFTER_MS);
/// The blocks read around a player elsewhere, beside the track's read
/// ([`Extractor::seek`]): fewer, so they come sooner; the player asks again
/// as it plays on.
const AROUND_SEEK: (i64, i64) = (10_000, 40_000);

/// A reader of a track's rest steps aside while `busy`: the first to pass
/// `gate` waits that out (`wait` says how long) and gives the read's
/// `budget` that time, while the others wait for it, then go on. One that
/// comes while a wait is under way waits for it too, `busy` or not: the turn
/// may not be back yet.
fn step_aside(gate: &Mutex<()>, busy: impl Fn() -> bool, wait: impl FnOnce() -> Duration, budget: &matroska::Budget) {
    if !busy() && !gate.is_locked() {
        return;
    }
    let _gate = gate.lock();
    if busy() {
        budget.extend(wait());
    }
}

/// Picture track `index` of a Matroska file, read through its Cues as FFmpeg
/// would have copied it (its times as [`as_webvtt`] takes them) and written
/// to `target` (to a temporary file beside it, then renamed); an error says
/// why FFmpeg has to read the file instead. The blocks around `from_ms` are
/// read first, at once, and written to `window` the same way, unless they
/// are all of it; then `first_written` hears so, holds the rest up as
/// `between` does, and says how long it waited. Each stretch read is in
/// `so_far` from then on; after each but the last, `between` holds the next
/// up while playbacks start, reads at players elsewhere go on and other
/// reads take the turn, and says how long it waited. Within those stretches,
/// `step_aside` holds the readers up before each block while playbacks start
/// or reads at players elsewhere go on (one of them letting the turn go
/// meanwhile), and gives the budget that time. Blocking.
#[allow(clippy::too_many_arguments)]
fn write_through_cues(
    path: &Path,
    media: &MediaInfo,
    index: i64,
    from_ms: i64,
    so_far: &Mutex<SoFar>,
    window: &Path,
    target: &Path,
    first_written: impl FnOnce() -> Duration,
    mut between: impl FnMut() -> Duration,
    step_aside: matroska::StepAside,
) -> Result<(), String> {
    let started = Instant::now();
    let budget = matroska::Budget::new(PICTURE_READ_BYTES, picture_read_timeout(path));
    let mut first_written = Some(first_written);
    let (mut reads, mut bytes_read, mut first_ms) = (0, 0, None);
    let around = (from_ms.saturating_sub(PICTURES_BEFORE_MS), from_ms.saturating_add(PICTURES_AFTER_MS));
    let read = matroska::picture_tracks(path, media, &[index], &budget, around, step_aside, |found| {
        let (reads_now, bytes_now, more) = (found.reads, found.bytes_read, found.more);
        let stretch = stretch_of(found, media, index)?;
        (reads, bytes_read) = (reads + reads_now, bytes_read + bytes_now);
        let mut so_far = so_far.lock();
        so_far.add(stretch);
        if more && let Some(first_written) = first_written.take() {
            first_ms = Some(started.elapsed().as_millis());
            // Written outside the lock, which answers take.
            let (codec, header, blocks) = so_far.copy().ok_or("no track read")?;
            drop(so_far);
            write_mks(window, codec, &header, &blocks)?;
            // Time spent waiting isn't time spent reading.
            budget.extend(first_written());
        } else if more {
            drop(so_far);
            budget.extend(between());
        }
        Ok(())
    });
    // Taken out of the lock (answers meanwhile wait for the file), and let
    // go of once written.
    let writing = SoFar { writing: true, ..SoFar::default() };
    let SoFar { track, mut blocks, .. } = std::mem::replace(&mut *so_far.lock(), writing);
    // Around the player first, then after, then before.
    blocks.sort_by_key(|b| b.time_ms);
    let written = read.and_then(|()| {
        let (codec, header) = track.as_ref().ok_or("no track read")?;
        write_mks(target, codec, header, &blocks)
    });
    if let Err(reason) = written {
        // A window players were given stays theirs until FFmpeg's copy comes
        // past it; what was read stands in only where there was none.
        if first_ms.is_none()
            && let Some((codec, header)) = &track
        {
            let _ = write_mks(window, codec, header, &blocks);
        }
        so_far.lock().writing = false;
        return Err(reason);
    }
    let _ = std::fs::remove_file(window);
    tracing::debug!(
        "picture subtitles of {} read through the cues in {} ms, those from {} s in {} ms ({} blocks, {reads} \
         reads, {} KB)",
        path.display(),
        started.elapsed().as_millis(),
        around.0.max(0) / 1000,
        first_ms.unwrap_or_else(|| started.elapsed().as_millis()),
        blocks.len(),
        bytes_read / 1024
    );
    Ok(())
}

/// A stretch of a picture track read through the Cues: the track's codec
/// and header, the stretch's blocks as FFmpeg times them, in order, and the
/// span of the track's time (ms, from and until) whose blocks they are all
/// of.
struct Stretch {
    codec: &'static str,
    header: Vec<u8>,
    blocks: Vec<mks::Block>,
    /// `None` when it can't be told.
    span: Option<(i64, i64)>,
}

/// The stretch of picture track `index` (the one asked for) read through
/// the Cues.
fn stretch_of(found: matroska::PictureTracks, media: &MediaInfo, index: i64) -> Result<Stretch, String> {
    let out_of_range = || format!("stream {index} has a block at a time out of range");
    let (tick_ns, whole_ms) = (found.tick_ns, found.whole_ms);
    let ms = |ticks: u64| match i64::try_from(ticks) {
        Ok(ticks) => ffmpeg_times(tick_ns, whole_ms, media, ticks, 0).map(|(ms, _)| ms),
        Err(_) => Some(i64::MAX),
    };
    // From the start of the file: from the start of the track, whenever the
    // file starts.
    let span = found.span.and_then(|(from, until)| {
        let from = if from == 0 { Some(i64::MIN) } else { ms(from) };
        Some((from?, ms(until)?))
    });
    let stretch = found.tracks.into_iter().next().ok_or("no track read")?;
    let mut blocks: Vec<mks::Block> = Vec::with_capacity(stretch.blocks.len());
    for block in stretch.blocks {
        let (time_ms, duration_ms) =
            ffmpeg_times(tick_ns, whole_ms, media, block.time, block.duration).ok_or_else(out_of_range)?;
        if time_ms < 0 || blocks.last().is_some_and(|b| b.time_ms > time_ms) {
            return Err(format!("stream {index} has blocks before the start or out of order"));
        }
        blocks.push(mks::Block { time_ms, duration_ms, data: block.data });
    }
    Ok(Stretch { codec: stretch.codec, header: stretch.header, blocks, span })
}

/// What a read of the blocks around a player elsewhere may keep in memory
/// (see [`matroska::Budget`]), the Cues it finds them by included, and how
/// long it may take: a minute or so of a track, read in about a second.
const SEEK_READ_BYTES: u64 = 32 << 20;
const SEEK_READ_TIME: Duration = Duration::from_secs(10);

/// Whether FFmpeg took the file of `media` for Matroska (or WebM): only
/// those are read through their Cues.
fn is_matroska(media: &MediaInfo) -> bool {
    media.format_name.as_deref().is_some_and(|names| names.split(',').any(|n| n == "matroska"))
}

/// What a read of the blocks around a player stops with once it has them.
const WINDOW_READ: &str = "the pictures around the player are read";

/// The blocks of picture track `index` of a Matroska file around `at_ms`
/// ([`AROUND_SEEK`]), read through its Cues within a budget of their own
/// ([`SEEK_READ_BYTES`]), as a Matroska file of their own. Blocking.
fn window_through_cues(path: &Path, media: &MediaInfo, index: i64, at_ms: i64) -> Result<Vec<u8>, String> {
    let around = (at_ms.saturating_sub(AROUND_SEEK.0), at_ms.saturating_add(AROUND_SEEK.1));
    let budget = matroska::Budget::new(SEEK_READ_BYTES, SEEK_READ_TIME);
    let mut window = None;
    // Only the first stretch is read: nothing to step aside for.
    let read = matroska::picture_tracks(path, media, &[index], &budget, around, &|_| {}, |found| {
        let stretch = stretch_of(found, media, index)?;
        window = Some(mks::to_bytes(stretch.codec, &stretch.header, &stretch.blocks));
        Err(WINDOW_READ.into())
    });
    match (read, window) {
        (_, Some(window)) => Ok(window),
        (Err(reason), None) => Err(reason),
        (Ok(()), None) => Err("no track read".into()),
    }
}

/// A picture track (`codec`, its `header`, its `blocks` in order of time)
/// written whole to `target`: to a temporary file beside it, then renamed.
fn write_mks(target: &Path, codec: &str, header: &[u8], blocks: &[mks::Block]) -> Result<(), String> {
    let tmp = target.with_file_name(format!(".{}.mks.tmp", uuid::Uuid::new_v4().simple()));
    let written = mks::write(&tmp, codec, header, blocks).and_then(|()| std::fs::rename(&tmp, target));
    if let Err(err) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("the track couldn't be written: {err}"));
    }
    Ok(())
}

/// When a block of a file read through its Cues starts and how long it
/// lasts, in milliseconds, as FFmpeg times it: the file's start time taken
/// off (see [`as_webvtt`]), rounded the way FFmpeg rescales timestamps.
/// `None` when that can't be told the same.
fn ffmpeg_times(tick_ns: u64, whole_ms: bool, media: &MediaInfo, time: i64, duration: u64) -> Option<(i64, u64)> {
    let tick = (i64::try_from(tick_ns).ok()?, 1_000_000_000);
    let (us, ms) = ((1, 1_000_000), (1, 1000));
    if !whole_ms && media.start_ms.is_some() {
        return None;
    }
    let offset = rescale(-media.start_ms.unwrap_or(0) * 1000, us, tick)?;
    let start = rescale(rescale(time.checked_add(offset)?, tick, us)?, us, ms)?;
    let duration = rescale(i64::try_from(duration).ok()?, tick, ms)?;
    Some((start, u64::try_from(duration).ok()?))
}

/// Takes a track's extraction out of those under way once it's over, however
/// it ends: the next request finds the cache, or tries again after a failure.
/// A later extraction in its place stays.
struct Forget {
    extractor: Arc<Extractor>,
    track: String,
    id: u64,
}

impl Drop for Forget {
    fn drop(&mut self) {
        let mut extractions = self.extractor.extractions.lock();
        if extractions.get(&self.track).is_some_and(|extraction| extraction.id == self.id) {
            extractions.remove(&self.track);
        }
    }
}

/// Text track `index` of a Matroska file, read block by block through its
/// Cues as FFmpeg would have written it, within the limits of such a read,
/// and cached at `target` (it is served even when it can't be); an error says
/// why FFmpeg has to read the file instead. Blocking.
fn read_through_cues(path: &Path, media: &MediaInfo, index: i64, target: &Path) -> Result<String, String> {
    let budget = matroska::Budget::new(CUE_READ_BYTES, CUE_READ_TIME);
    let (_, text) = cue_tracks(path, media, &[index], &budget)?.into_iter().next().ok_or("no track read")?;
    // Cached whole or not at all.
    if let Err(err) = write_atomically(target, text.as_bytes()) {
        tracing::debug!("could not cache the subtitles of {} read through the cues: {err}", path.display());
    }
    Ok(text)
}

/// Whether a pass over every text track of a file that failed with `err` is
/// worth running again for the one asked for alone: when another track
/// couldn't be converted, not when it ran out of time (reading the file
/// again takes as long).
fn retry_alone(err: &ApiError, tracks: usize) -> bool {
    tracks > 1 && !matches!(err, ApiError::Timeout(_))
}

/// How often work over whole files (a pass, seek previews) marks what it
/// writes as being written. The cache's pruning takes temporaries left
/// alone for hours as abandoned, and such work may run or wait paused longer
/// than that (a big file on a slow share, a long transcode), with outputs
/// that get nothing for a long time (tracks with no cue).
pub(crate) const KEEP_FRESH: Duration = Duration::from_secs(10 * 60);

/// `work`, calling `freshen` (which marks what it writes as being written)
/// as it starts and every `every` while it runs.
pub(crate) async fn kept_fresh<T>(work: impl Future<Output = T>, every: Duration, mut freshen: impl FnMut()) -> T {
    tokio::pin!(work);
    let mut tick = tokio::time::interval(every);
    loop {
        tokio::select! {
            done = &mut work => return done,
            _ = tick.tick() => freshen(),
        }
    }
}

/// Marks the files `parts` as being written.
fn mark_written(parts: &[&Path]) {
    for part in parts {
        if let Ok(file) = std::fs::File::options().write(true).open(part) {
            let _ = file.set_modified(SystemTime::now());
        }
    }
}

/// How long FFmpeg may take to read the whole file at `path`: see
/// [`pass_timeout`].
fn whole_file_timeout(path: &Path) -> Duration {
    pass_timeout(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

/// 20 minutes, or more for a file too big to read in that time at 10 MB/s
/// (a slow disk or network share; a 70 GB film needs two hours at that pace).
pub(crate) fn pass_timeout(bytes: u64) -> Duration {
    Duration::from_secs((bytes / 10_000_000).max(20 * 60))
}

/// What reading a track through the Cues may keep in memory (see
/// [`matroska::Budget`]). A film's track takes under a megabyte, the most
/// elaborate ASS tens of megabytes; a file whose Cues claim more is left to
/// FFmpeg, which reads it a packet at a time.
const CUE_READ_BYTES: u64 = 64 << 20;
/// How long reading a track through the Cues may take, converting it
/// included: seconds, even on a network share. Longer, FFmpeg reads the file.
const CUE_READ_TIME: Duration = Duration::from_secs(60);

/// The text tracks `streams` of a Matroska file as WebVTT, read through its
/// Cues within `budget`, with FFmpeg's cue times: the file's start time taken
/// off, rounded the way FFmpeg rescales timestamps.
fn cue_tracks(
    path: &Path,
    media: &MediaInfo,
    streams: &[i64],
    budget: &matroska::Budget,
) -> Result<Vec<(i64, String)>, String> {
    let started = Instant::now();
    let found = matroska::text_tracks(path, media, streams, budget)?;
    tracing::debug!(
        "{} subtitle tracks of {} read through the cues in {} ms ({} reads, {} KB)",
        found.tracks.len(),
        path.display(),
        started.elapsed().as_millis(),
        found.reads,
        found.bytes_read / 1024
    );
    as_webvtt(found, media, budget)
}

/// Tracks read through the Cues as the WebVTT files FFmpeg would write.
fn as_webvtt(
    found: matroska::TextTracks,
    media: &MediaInfo,
    budget: &matroska::Budget,
) -> Result<Vec<(i64, String)>, String> {
    let tick_ns = i64::try_from(found.tick_ns).map_err(|_| "the file's timestamp scale is out of range")?;
    let tick = (tick_ns, 1_000_000_000);
    let (us, ms) = ((1, 1_000_000), (1, 1000));
    // FFmpeg takes off the file's start time in microseconds, while the media
    // info keeps it in whole milliseconds (`start_ms`, rounded from ffprobe's
    // microseconds): the same when every time is whole milliseconds.
    // Otherwise a start of a fraction of one (1.833332 s with ticks of
    // 333333 ns) would put some cues 1 ms off FFmpeg's. ffprobe gives any
    // file with audio or video a start time (0 for most), so such a file is
    // left to FFmpeg.
    if !found.whole_ms && media.start_ms.is_some() {
        return Err("the file's start isn't known to the microsecond".into());
    }
    let offset = rescale(-media.start_ms.unwrap_or(0) * 1000, us, tick).ok_or("the file's start is out of range")?;
    let mut out = Vec::new();
    for track in found.tracks {
        let index = track.stream_index;
        let mut events = Vec::with_capacity(track.blocks.len());
        for block in track.blocks {
            let start_ms =
                block.time.checked_add(offset).and_then(|t| rescale(t, tick, us)).and_then(|t| rescale(t, us, ms));
            // FFmpeg keeps how long a cue lasts in 32 bits of milliseconds:
            // a longer one (49 days, a damaged file) is left to it.
            let duration_ms = i64::try_from(block.duration).ok().and_then(|d| rescale(d, tick, ms));
            let (Some(start_ms), Some(duration_ms @ 0..=0xFFFF_FFFF)) = (start_ms, duration_ms) else {
                return Err(format!("stream {index} has a cue at a time out of range"));
            };
            // FFmpeg's muxer would shift a track starting before 0, and
            // refuses one going back in time: left to it.
            if start_ms < 0 || events.last().is_some_and(|e: &Event| e.start_ms > start_ms) {
                return Err(format!("stream {index} has cues before the start or out of order"));
            }
            events.push(Event { start_ms, duration_ms, data: block.data });
        }
        // The WebVTT made so far is kept too.
        let mut held = 0;
        let text = webvtt::to_webvtt(track.codec, &track.header, &events, |size| {
            budget.check()?;
            budget.spend(size.saturating_sub(held) as u64)?;
            held = held.max(size);
            Ok(())
        })?;
        out.push((index, text));
    }
    Ok(out)
}

/// `value` from one time base to another (`(num, den)`), rounded half away
/// from zero, as FFmpeg's `av_rescale_q`; `None` when it doesn't fit.
fn rescale(value: i64, from: (i64, i64), to: (i64, i64)) -> Option<i64> {
    let (b, c) = (from.0 as i128 * to.1 as i128, from.1 as i128 * to.0 as i128);
    let n = (value as i128 * b).abs();
    let q = (n + c / 2) / c;
    i64::try_from(if value < 0 { -q } else { q }).ok()
}

const VULKAN_DEVICE: [&str; 4] = ["-init_hw_device", "vulkan=vk", "-filter_hw_device", "vk"];

/// How a frame grab turns video into the picture: its filters, and the global
/// options they need.
#[derive(Debug, Clone, PartialEq)]
struct Picture {
    init: &'static [&'static str],
    vf: String,
    tone_mapped: bool,
}

impl Picture {
    fn plain(width: u32) -> Self {
        Self { init: &[], vf: format!("scale={width}:-2:flags=bicubic"), tone_mapped: false }
    }

    /// HDR tone-mapped to SDR the best way available. Dolby Vision without a
    /// compatible base layer (profile 5) only gets its colours right through
    /// libplacebo, which applies the RPU; other HDR is cheaper on the CPU with
    /// zimg, given the colour properties it needs. libplacebo tone maps as it
    /// does the video ([`PLACEBO_FALLBACK_PEAK`]).
    fn hdr(width: u32, video: Option<&MediaStream>, caps: &Capabilities) -> Self {
        let placebo = caps.tonemappers.contains(&Tonemapper::Libplacebo);
        let zscale = caps.tonemappers.contains(&Tonemapper::Zscale);
        if placebo && (!zscale || video.is_some_and(MediaStream::is_dolby_vision_only)) {
            let fallback = if caps.placebo_fallback_peak { format!(":{PLACEBO_FALLBACK_PEAK}") } else { String::new() };
            return Self {
                init: &VULKAN_DEVICE,
                vf: format!(
                    "libplacebo=w={width}:h=-2:format=yuv420p:colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv:{PLACEBO_CURVE}:{PLACEBO_STATIC_PEAK}{fallback}"
                ),
                tone_mapped: true,
            };
        }
        if zscale {
            let filters: Vec<String> = video
                .and_then(untagged_hdr_fix)
                .into_iter()
                .chain([ZSCALE_TONEMAP.to_owned(), format!("scale={width}:-2:flags=bicubic")])
                .collect();
            return Self { init: &[], vf: filters.join(","), tone_mapped: true };
        }
        Self::plain(width)
    }
}

/// A cached WebVTT file, unless it can't be read or doesn't even start like
/// one (then it is extracted again).
async fn cached_vtt(path: &Path) -> Option<String> {
    let text = tokio::fs::read_to_string(path).await.ok()?;
    if !text.starts_with("WEBVTT") {
        return None;
    }
    // In use: cache pruning keeps it.
    touch(path);
    Some(text)
}

/// WebVTT being written, up to its last complete cue (cues end with a blank line).
fn whole_cues(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    match text.rfind("\n\n") {
        Some(end) if text.starts_with("WEBVTT") => text[..end + 2].to_owned(),
        _ => "WEBVTT\n\n".to_owned(),
    }
}

/// Make sure a file another process wrote is on disk.
async fn flush(path: &Path) -> std::io::Result<()> {
    // (Windows needs write access to flush.)
    tokio::fs::OpenOptions::new().write(true).open(path).await?.sync_all().await
}

fn existing(path: &str) -> ApiResult<PathBuf> {
    let path = PathBuf::from(path);
    if path.is_file() { Ok(path) } else { Err(ApiError::NotFound(format!("file not found: {}", path.display()))) }
}

/// The most a capture takes from ffmpeg's output (a cover, a thumbnail, a
/// subtitle track): a crafted file's several hundred megabytes of "picture"
/// would otherwise be held whole, here and in the server.
const MAX_CAPTURE: usize = 64 << 20;
/// The most of its log a capture keeps (the rest is read and dropped).
const MAX_CAPTURE_LOG: usize = 1 << 20;
/// The most a subtitle track extracted to a file may be (a Blu-ray's
/// pictures are tens of megabytes).
const MAX_EXTRACTED: u64 = 512 << 20;

/// Run ffmpeg, returning stdout. Non-zero exit becomes `ApiError::Failed`;
/// output past [`MAX_CAPTURE`] stops it.
pub(crate) async fn run_capture(mut cmd: tokio::process::Command, timeout: Duration) -> ApiResult<(Vec<u8>, String)> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to start ffmpeg: {e}")))?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(ApiError::Internal(anyhow::anyhow!("ffmpeg's output isn't piped")));
    };
    // The log is read on its own, so that a full pipe never holds ffmpeg up.
    let log = tokio::spawn(read_capped(stderr, MAX_CAPTURE_LOG, true));
    let run = async {
        let (stdout, over) = read_capped(stdout, MAX_CAPTURE, false).await?;
        if over {
            let _ = child.kill().await;
            return Ok((stdout, None));
        }
        child.wait().await.map(|status| (stdout, Some(status)))
    };
    let result = tokio::time::timeout(timeout, run).await;
    let stderr = match log.await {
        Ok(Ok((log, _))) => String::from_utf8_lossy(&log).trim().to_owned(),
        _ => String::new(),
    };
    let (stdout, status) = result
        .map_err(|_| ApiError::Timeout("ffmpeg took too long".into()))?
        .map_err(|e: std::io::Error| ApiError::Internal(e.into()))?;
    let Some(status) = status else {
        return Err(ApiError::Failed(format!("ffmpeg wrote more than {} MB", MAX_CAPTURE >> 20)));
    };
    if !status.success() {
        let message = stderr.lines().last().unwrap_or("ffmpeg failed").to_owned();
        return Err(ApiError::Failed(message));
    }
    Ok((stdout, stderr))
}

/// What `reader` gives, up to `max` bytes: and whether there was more, which
/// is read and dropped with `drain`, else left (the writer is stopped).
async fn read_capped(
    mut reader: impl tokio::io::AsyncRead + Unpin,
    max: usize,
    drain: bool,
) -> std::io::Result<(Vec<u8>, bool)> {
    use tokio::io::AsyncReadExt;
    let mut kept = Vec::new();
    let mut chunk = vec![0; 64 << 10];
    let mut over = false;
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            return Ok((kept, over));
        }
        if kept.len() + n > max {
            over = true;
            if !drain {
                return Ok((kept, true));
            }
        } else {
            kept.extend_from_slice(&chunk[..n]);
        }
    }
}

fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

/// The largest subtitle file converted to WebVTT: a film's text track takes
/// well under a megabyte, the most elaborate ASS tens of them. Anything
/// bigger (a picture track's `.sup`, a file that isn't subtitles) is
/// refused before it is read into memory.
const MAX_SUBTITLE_FILE: u64 = 64 << 20;
/// How much of a subtitle file its charset is told from.
const CHARSET_SAMPLE: usize = 1 << 20;

/// The first `limit` bytes of the file at `path` (all of a shorter one).
async fn read_head(path: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut head = Vec::new();
    tokio::fs::File::open(path).await?.take(limit as u64).read_to_end(&mut head).await?;
    Ok(head)
}

/// Text subtitle files are frequently not UTF-8. UTF-16 with a BOM is handled by
/// ffmpeg; anything else that is not valid UTF-8 is assumed to be Windows-1252.
/// `raw` may be the start of the file only: a character cut off at its end
/// is no sign of another charset.
fn guess_charset(raw: &[u8]) -> Option<&'static str> {
    let utf8 = std::str::from_utf8(raw).map_or_else(|err| err.error_len().is_none(), |_| true);
    if raw.starts_with(&[0xFF, 0xFE]) || raw.starts_with(&[0xFE, 0xFF]) || utf8 { None } else { Some("CP1252") }
}

pub(crate) type Locks = Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>;

/// A lock by key (a file's extractions, a set of seek previews), forgotten
/// again once nobody uses it (on every path, including errors).
pub(crate) struct Lock<'a> {
    locks: &'a Locks,
    key: String,
    pub(crate) lock: Arc<tokio::sync::Mutex<()>>,
}

impl<'a> Lock<'a> {
    pub(crate) fn acquire(locks: &'a Locks, key: &str) -> Self {
        let lock = locks.lock().entry(key.to_owned()).or_default().clone();
        Self { locks, key: key.to_owned(), lock }
    }
}

impl Drop for Lock<'_> {
    fn drop(&mut self) {
        let mut locks = self.locks.lock();
        // Only the map and this guard hold it: nobody else is waiting.
        if Arc::strong_count(&self.lock) <= 2 {
            locks.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_fonts_are_kept_once_per_file() {
        let dir = tempfile::tempdir().unwrap();
        let subs = dir.path().join("subtitles");
        std::fs::create_dir(&subs).unwrap();
        let movie = dir.path().join("a.mkv");
        let files: [(&str, &str, &[u8]); 3] =
            [("A.OTF", "font/otf", b"one"), ("cover.jpg", "image/jpeg", b"picture"), ("b", "font/ttf", b"two")];
        std::fs::write(&movie, matroska::tests::with_attachments(&files)).unwrap();
        let fonts = cached_fonts(&movie, &subs).unwrap().unwrap();
        assert_eq!(fonts, subs.join(format!("{}.fonts", file_key(&movie).unwrap())).join("fonts"));
        assert_eq!(std::fs::read(fonts.join("0.otf")).unwrap(), b"one");
        assert_eq!(std::fs::read(fonts.join("1.ttf")).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&fonts).unwrap().count(), 2, "the fonts alone, for libass");
        // Read once: the next job finds them, and nothing is left beside them.
        assert_eq!(cached_fonts(&movie, &subs).unwrap(), Some(fonts));
        assert_eq!(std::fs::read_dir(&subs).unwrap().count(), 1);

        // None attached (not Matroska, here): nothing, which is remembered too.
        let plain = dir.path().join("b.mp4");
        std::fs::write(&plain, b"not matroska").unwrap();
        assert_eq!(cached_fonts(&plain, &subs).unwrap(), None);
        assert!(subs.join(format!("{}.fonts", file_key(&plain).unwrap())).join("index.json").exists());
        assert_eq!(cached_fonts(&plain, &subs).unwrap(), None);
    }

    #[test]
    fn sniffs_images() {
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"\x89PNG\r\n"), Some("image/png"));
        assert_eq!(sniff_image(b"hello"), None);
    }

    #[test]
    fn hdr_frame_grabs_use_a_tone_mapper_that_works() {
        use pley_transcoder_api::{DynamicRange, StreamKind};
        let p5 = MediaStream {
            kind: StreamKind::Video,
            dynamic_range: Some(DynamicRange::DolbyVision),
            dv_profile: Some(5),
            dv_bl_compat: Some(0),
            ..Default::default()
        };
        let hdr10 = MediaStream {
            kind: StreamKind::Video,
            dynamic_range: Some(DynamicRange::Hdr10),
            color_transfer: Some("smpte2084".into()),
            color_primaries: Some("bt2020".into()),
            color_space: Some("bt2020nc".into()),
            ..Default::default()
        };
        let caps =
            |tonemappers: &[Tonemapper]| Capabilities { tonemappers: tonemappers.to_vec(), ..Default::default() };
        let all = caps(&[Tonemapper::Libplacebo, Tonemapper::Opencl, Tonemapper::Zscale]);
        // Profile 5 went through zimg, which fails on its untagged frames
        // ("Nothing was written into output file"): libplacebo applies its RPU.
        let picture = Picture::hdr(640, Some(&p5), &all);
        assert!(picture.vf.starts_with("libplacebo=w=640:h=-2:"), "{picture:?}");
        assert!(!picture.vf.contains("fallback_peak"), "{picture:?}");
        assert_eq!(picture.init, VULKAN_DEVICE.as_slice());
        // Other HDR stays on the CPU.
        let picture = Picture::hdr(640, Some(&hdr10), &all);
        assert_eq!(picture.vf, format!("{ZSCALE_TONEMAP},scale=640:-2:flags=bicubic"));
        assert!(picture.init.is_empty() && picture.tone_mapped);
        // Without libplacebo, profile 5 frames are tagged for zimg: off
        // colours, but a picture.
        let picture = Picture::hdr(640, Some(&p5), &caps(&[Tonemapper::Zscale]));
        assert!(
            picture.vf.starts_with("setparams=color_trc=smpte2084:color_primaries=bt2020:colorspace=bt2020nc,zscale="),
            "{picture:?}"
        );
        // Unknown video (the probe failed) or no tone mapper.
        assert_eq!(Picture::hdr(640, None, &caps(&[Tonemapper::Libplacebo])).init, VULKAN_DEVICE.as_slice());
        assert_eq!(Picture::hdr(640, Some(&p5), &caps(&[Tonemapper::Opencl])), Picture::plain(640));
        // HDR10 that says nothing of its brightness, as the video is tone
        // mapped, where libplacebo takes a peak for it.
        let pleys = Capabilities { placebo_fallback_peak: true, ..caps(&[Tonemapper::Libplacebo]) };
        let picture = Picture::hdr(640, Some(&hdr10), &pleys);
        assert!(
            picture.vf.ends_with("tonemapping=bt.2390:tonemapping_param=0.5:peak_detect=0:fallback_peak=1000"),
            "{picture:?}"
        );
    }

    /// An extractor whose FFmpeg is `ffmpeg` and whose ffprobe doesn't exist.
    fn extractor_with(dir: &Path, ffmpeg: PathBuf) -> Extractor {
        let ff = Arc::new(Ffmpeg {
            ffmpeg,
            ffprobe: dir.join("no-ffprobe"),
            caps: Default::default(),
            segment_times_relative: true,
        });
        let metrics = Arc::new(Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics.clone(), 1));
        Extractor::new(ff, prober, metrics, dir.join("subtitles"), 1)
    }

    /// An extractor whose FFmpeg and ffprobe don't exist.
    fn extractor_without_ffmpeg(dir: &Path) -> Arc<Extractor> {
        Arc::new(extractor_with(dir, dir.join("no-ffmpeg")))
    }

    /// `file`, a version of the synthetic film of the Matroska tests (SubRip
    /// as stream 2, ASS as stream 4), written to `dir` and probed as ffprobe
    /// would see it: its path and its key.
    fn movie_in(extractor: &Extractor, dir: &Path, file: &matroska::tests::TestFile) -> (String, String) {
        let movie = file.write(dir);
        extractor.prober.remember(&movie, matroska::tests::movie_media());
        (movie.to_string_lossy().into_owned(), file_key(&movie).unwrap())
    }

    /// An extractor without FFmpeg, and the synthetic film: its path and key.
    fn with_movie(dir: &Path) -> (Arc<Extractor>, String, String) {
        let extractor = extractor_without_ffmpeg(dir);
        let (name, key) = movie_in(&extractor, dir, &matroska::tests::movie());
        (extractor, name, key)
    }

    /// The synthetic film with Cues that don't say where its blocks are in
    /// their clusters: FFmpeg has to read it.
    fn unplaced_movie() -> matroska::tests::TestFile {
        let mut file = matroska::tests::movie();
        file.no_relative = true;
        file
    }

    /// A stand-in for FFmpeg in `dir`. Each run writes its arguments on a
    /// line of `ffmpeg.log`, then fails at once, or runs on for 10 seconds
    /// when `hang`.
    fn stand_in_ffmpeg(dir: &Path, hang: bool) -> PathBuf {
        let log = dir.join("ffmpeg.log");
        #[cfg(windows)]
        let (path, script) = (
            dir.join("ffmpeg.cmd"),
            format!(
                "@echo %* >> \"{}\"\r\n{}\r\n",
                log.display(),
                if hang { "@ping -n 11 127.0.0.1 > nul" } else { "@exit /b 1" }
            ),
        );
        #[cfg(not(windows))]
        let (path, script) = (
            dir.join("ffmpeg"),
            format!(
                "#!/bin/sh\necho \"$@\" >> '{}'\n{}\n",
                log.display(),
                if hang { "exec sleep 10" } else { "exit 1" }
            ),
        );
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    /// The streams each run of the stand-in FFmpeg in `dir` was given.
    fn ffmpeg_runs(dir: &Path) -> Vec<Vec<i64>> {
        let log = std::fs::read_to_string(dir.join("ffmpeg.log")).unwrap_or_default();
        log.lines()
            .map(|line| {
                let args: Vec<&str> = line.split_whitespace().map(|arg| arg.trim_matches('"')).collect();
                let mapped = args.windows(2).filter(|pair| pair[0] == "-map");
                mapped.filter_map(|pair| pair[1].strip_prefix("0:")?.parse().ok()).collect()
            })
            .collect()
    }

    fn extractions(extractor: &Extractor) -> u64 {
        extractor.metrics.subtitle_extractions.load(Ordering::Relaxed)
    }

    /// Waits for `check` to hold (work running on its own), 10 seconds at most.
    async fn eventually(check: impl Fn() -> bool) {
        for _ in 0..1_000 {
            if check() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not reached");
    }

    #[tokio::test]
    async fn subtitle_caches_are_whole_or_absent() {
        let dir = tempfile::tempdir().unwrap();
        let extractor = extractor_without_ffmpeg(dir.path());
        let subs = dir.path().join("subtitles");
        let srt = dir.path().join("movie.en.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let name = srt.to_string_lossy().into_owned();

        // The conversion fails (no ffmpeg here): nothing is cached.
        assert!(extractor.subtitle(&name, None).await.is_err());
        assert_eq!(std::fs::read_dir(&subs).unwrap().count(), 0);
        // A cache entry cut short (a crash, a full disk) is not served…
        let cached = subs.join(format!("{}_sidecar.vtt", file_key(&srt).unwrap()));
        std::fs::write(&cached, "WEB").unwrap();
        assert!(extractor.subtitle(&name, None).await.is_err());
        // …a whole one is.
        std::fs::write(&cached, "WEBVTT\n\n00:01.000 --> 00:02.000\nHello\n").unwrap();
        assert!(extractor.subtitle(&name, None).await.unwrap().contains("Hello"));
    }

    #[tokio::test]
    async fn embedded_subtitles_come_as_far_as_they_are_read() {
        use pley_transcoder_api::StreamKind;
        let dir = tempfile::tempdir().unwrap();
        let extractor = extractor_without_ffmpeg(dir.path());
        let subs = dir.path().join("subtitles");
        let wait = Duration::from_millis(200);

        // An extraction that can't run (no ffprobe here) says so.
        let unknown = dir.path().join("unknown.mkv");
        std::fs::write(&unknown, "not really a movie").unwrap();
        assert!(extractor.subtitle_so_far(&unknown.to_string_lossy(), 2, wait).await.is_err());

        // A file without Cues: FFmpeg reads all of it for the track, in a
        // pass that waits for its turn (every pass is taken here)...
        let movie = dir.path().join("movie.mp4");
        std::fs::write(&movie, "not really a movie either").unwrap();
        let streams = vec![
            matroska::tests::stream(0, StreamKind::Video, "h264"),
            matroska::tests::stream(1, StreamKind::Subtitle, "subrip"),
        ];
        extractor.prober.remember(
            &movie,
            MediaInfo { format_name: Some("mov,mp4,m4a,3gp,3g2,mj2".into()), streams, ..Default::default() },
        );
        let (name, key) = (movie.to_string_lossy().into_owned(), file_key(&movie).unwrap());
        let passes = extractor.passes.acquire_many(PASSES as u32).await.unwrap();
        let started = extractions(&extractor);
        assert_eq!(extractor.subtitle_so_far(&name, 1, wait).await.unwrap(), ("WEBVTT\n\n".to_owned(), false));
        eventually(|| extractor.file_locks.lock().get(&key).is_some_and(|lock| lock.try_lock().is_err())).await;
        // ...then the cues it has read so far come at once (the pass may take
        // hours), the one being written left out, from that same extraction...
        std::fs::write(
            subs.join(format!("{key}_1.vtt.part")),
            "WEBVTT\r\n\r\n00:01.000 --> 00:02.000\r\nHello\r\n\r\n00:03.0",
        )
        .unwrap();
        let so_far = extractor.subtitle_so_far(&name, 1, Duration::from_secs(60));
        let (text, complete) = tokio::time::timeout(Duration::from_secs(10), so_far).await.expect("at once").unwrap();
        assert_eq!((text.as_str(), complete), ("WEBVTT\n\n00:01.000 --> 00:02.000\nHello\n\n", false));
        assert_eq!(extractions(&extractor), started + 1);
        // ...and once it's done, all of it.
        std::fs::write(
            subs.join(format!("{key}_1.vtt")),
            "WEBVTT\n\n00:01.000 --> 00:02.000\nHello\n\n00:03.000 --> 00:04.000\nBye\n",
        )
        .unwrap();
        let (text, complete) = extractor.subtitle_so_far(&name, 1, wait).await.unwrap();
        assert!(complete && text.contains("Bye"), "{text}");
        drop(passes);
    }

    #[tokio::test]
    async fn subtitles_read_through_the_cues_come_whole_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let (extractor, name, key) = with_movie(dir.path());
        let whole = "WEBVTT\n\n00:01.000 --> 00:02.500\nHello\n\n00:04.800 --> 00:05.800\nBefore the cluster\n\n\
            00:09.002 --> 00:09.002\nNo duration\n";

        // The first answer has all of it, so the player stops asking.
        let wait = Duration::from_secs(10);
        assert_eq!(extractor.subtitle_so_far(&name, 2, wait).await.unwrap(), (whole.to_owned(), true));
        let cached = dir.path().join("subtitles").join(format!("{key}_2.vtt"));
        assert_eq!(std::fs::read_to_string(cached).unwrap(), whole);
        // Whole-track requests read them the same way.
        assert!(extractor.subtitle(&name, Some(4)).await.unwrap().ends_with("\nHi\n"));
        assert!(matches!(extractor.subtitle(&name, Some(3)).await, Err(ApiError::BadRequest(_))), "PGS isn't text");
    }

    #[tokio::test]
    async fn tracks_of_a_file_are_read_side_by_side() {
        let dir = tempfile::tempdir().unwrap();
        let (extractor, name, key) = with_movie(dir.path());
        let soon = Duration::from_secs(10);
        // While stream 2 is being extracted...
        let (finish, finished) = tokio::sync::oneshot::channel::<String>();
        let result = async move { Ok::<_, Arc<ApiError>>(Arc::new(finished.await.unwrap_or_default())) };
        let running = Extraction { id: u64::MAX, by_ffmpeg: Arc::default(), result: result.boxed().shared() };
        extractor.extractions.lock().insert(format!("{key}_2"), running);
        // ...stream 4 is read all the same,
        let ass = tokio::time::timeout(soon, extractor.subtitle(&name, Some(4))).await.expect("not held up");
        assert!(ass.unwrap().ends_with("\nHi\n"));
        // and requests for stream 2 wait for that extraction rather than
        // start another one.
        let again = extractor.subtitle(&name, Some(2));
        tokio::pin!(again);
        assert!(tokio::time::timeout(Duration::from_millis(200), &mut again).await.is_err(), "waits");
        let wait = Duration::from_millis(50);
        assert_eq!(extractor.subtitle_so_far(&name, 2, wait).await.unwrap(), ("WEBVTT\n\n".to_owned(), false));
        finish.send("WEBVTT\n\nread meanwhile\n".to_owned()).unwrap();
        assert_eq!(again.await.unwrap(), "WEBVTT\n\nread meanwhile\n");
        assert_eq!(extractions(&extractor), 1, "stream 4's");
    }

    #[tokio::test]
    async fn tracks_the_cues_list_come_whole_while_ffmpeg_reads_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let (extractor, name, key) = with_movie(dir.path());
        // FFmpeg reading the whole file for another of its tracks (one the
        // Cues can't give), or waiting for its turn to...
        let pass = Lock::acquire(&extractor.file_locks, &key);
        let _reading = pass.lock.lock().await;
        // ...the SubRip track still comes whole, read through the Cues, rather
        // than as far as that pass has got.
        let (text, complete) = extractor.subtitle_so_far(&name, 2, Duration::from_secs(10)).await.unwrap();
        assert!(complete && text.contains("Before the cluster"), "{text}");
    }

    #[tokio::test]
    async fn reads_go_on_when_their_requests_go_away() {
        let dir = tempfile::tempdir().unwrap();
        let (extractor, name, key) = with_movie(dir.path());
        let cached = dir.path().join("subtitles").join(format!("{key}_2.vtt"));
        // While other tracks being read take every turn...
        let turns = extractor.cue_reads.clone().acquire_owned().await.unwrap();
        // ...a player asking for this one hears it isn't there yet, and asks
        // again,
        let wait = Duration::from_millis(50);
        for _ in 0..2 {
            assert_eq!(extractor.subtitle_so_far(&name, 2, wait).await.unwrap(), ("WEBVTT\n\n".to_owned(), false));
        }
        // and a whole-track request stops waiting (its client went away).
        assert!(tokio::time::timeout(wait, extractor.subtitle(&name, Some(2))).await.is_err());
        // The one read they started goes on when its turn comes, to the
        // cache.
        drop(turns);
        eventually(|| cached.exists()).await;
        assert!(std::fs::read_to_string(&cached).unwrap().contains("Before the cluster"));
        assert_eq!(extractions(&extractor), 1, "one for all the requests");
        let (text, complete) = extractor.subtitle_so_far(&name, 2, wait).await.unwrap();
        assert!(complete && text.contains("Before the cluster"), "{text}");
    }

    #[tokio::test]
    async fn long_passes_hold_up_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let (extractor, name, _) = with_movie(dir.path());
        let soon = Duration::from_secs(10);
        // Every FFmpeg pass over a whole file taken (hours each on a slow
        // share)...
        let _passes = extractor.passes.acquire_many(PASSES as u32).await.unwrap();
        // ...tracks read through the Cues come all the same...
        let subrip = tokio::time::timeout(soon, extractor.subtitle(&name, Some(2))).await.expect("not held up");
        assert!(subrip.unwrap().contains("Hello"));
        // ...and quick FFmpeg runs don't wait for them (they fail here: no
        // FFmpeg).
        let srt = dir.path().join("movie.en.srt");
        std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
        let srt = srt.to_string_lossy().into_owned();
        assert!(tokio::time::timeout(soon, extractor.subtitle(&srt, None)).await.expect("not held up").is_err());
        assert!(tokio::time::timeout(soon, extractor.cover(&name)).await.expect("not held up").is_err());
    }

    #[tokio::test]
    async fn quick_runs_hold_up_no_pass() {
        let dir = crate::testing::tempdir();
        let extractor = Arc::new(extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), false)));
        let (name, _) = movie_in(&extractor, dir.path(), &unplaced_movie());
        // Every quick FFmpeg run taken (thumbnails, cover art, subtitle
        // files)...
        let _quick = extractor.limiter.acquire().await.unwrap();
        // ...a pass over a file whose Cues can't be read runs all the same
        // (and fails here).
        let pass = tokio::time::timeout(Duration::from_secs(30), extractor.subtitle(&name, Some(2))).await;
        assert!(pass.expect("not held up").is_err());
        assert_eq!(ffmpeg_runs(dir.path()).first(), Some(&vec![2, 4]), "every text track at once");
    }

    #[tokio::test]
    async fn subtitles_read_through_the_cues_are_served_when_they_cant_be_cached() {
        let dir = tempfile::tempdir().unwrap();
        let (extractor, name, key) = with_movie(dir.path());
        // Something in the way of the cache file (a full disk, a file in use).
        std::fs::create_dir(dir.path().join("subtitles").join(format!("{key}_2.vtt"))).unwrap();
        // Served from memory, not left to FFmpeg (which would fail here).
        assert!(extractor.subtitle(&name, Some(2)).await.unwrap().contains("Before the cluster"));
    }

    #[tokio::test]
    async fn passes_that_ran_out_of_time_arent_run_again() {
        // A pass that fails is run again for the track asked for alone:
        // another track may be what FFmpeg couldn't convert...
        let dir = crate::testing::tempdir();
        let extractor = Arc::new(extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), false)));
        let (name, _) = movie_in(&extractor, dir.path(), &unplaced_movie());
        assert!(matches!(extractor.subtitle(&name, Some(2)).await, Err(ApiError::Failed(_))));
        assert_eq!(ffmpeg_runs(dir.path()), [vec![2, 4], vec![2]]);
        // ...but not one that ran out of time: reading the file again would
        // take as long.
        let dir = crate::testing::tempdir();
        let mut slow = extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), true));
        slow.pass_time = |_| Duration::from_secs(2);
        let slow = Arc::new(slow);
        let (name, _) = movie_in(&slow, dir.path(), &unplaced_movie());
        assert!(matches!(slow.subtitle(&name, Some(2)).await, Err(ApiError::Timeout(_))));
        assert_eq!(ffmpeg_runs(dir.path()), [vec![2, 4]]);
    }

    #[tokio::test]
    async fn a_failed_pass_is_told_rather_than_run_again() {
        let dir = crate::testing::tempdir();
        let extractor = Arc::new(extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), false)));
        let (name, key) = movie_in(&extractor, dir.path(), &unplaced_movie());
        let wait = Duration::from_millis(100);
        // A player's first look starts the pass (which fails here, run
        // again for the track alone)...
        let _ = extractor.subtitle_so_far(&name, 2, wait).await;
        eventually(|| extractor.extractions.lock().is_empty()).await;
        assert_eq!(ffmpeg_runs(dir.path()), [vec![2, 4], vec![2]]);
        // ...and the next ones hear why, rather than read the file again.
        for _ in 0..3 {
            assert!(matches!(extractor.subtitle_so_far(&name, 2, wait).await, Err(ApiError::Failed(_))));
        }
        assert!(matches!(extractor.subtitle(&name, Some(2)).await, Err(ApiError::Failed(_))));
        assert_eq!(ffmpeg_runs(dir.path()).len(), 2);
        // Another track's failure isn't this one's.
        let track = format!("{key}_2");
        assert!(extractor.failed_pass(&track).is_some() && extractor.failed_pass(&format!("{key}_4")).is_none());
        // For a while: then a pass is tried again.
        let Some(long_ago) = Instant::now().checked_sub(FAILED_PASS_KEPT) else { return };
        extractor.failed_passes.lock()[0].1 = long_ago;
        assert!(extractor.failed_pass(&track).is_none());
        assert!(extractor.subtitle(&name, Some(2)).await.is_err());
        assert_eq!(ffmpeg_runs(dir.path()).len(), 4);
        assert!(extractor.failed_pass(&track).is_some());
    }

    /// A Matroska file of a PGS track, as FFmpeg is writing it: its
    /// segment's size unknown, cut off `into` bytes into its second cluster;
    /// and where that cluster starts.
    fn mks_being_written(dir: &Path, into: usize) -> (Vec<u8>, usize) {
        let path = dir.join("whole.mks");
        let blocks = [
            mks::Block { time_ms: 1_000, duration_ms: 0, data: vec![0x16, 0, 1] },
            mks::Block { time_ms: 60_000, duration_ms: 0, data: vec![0x16, 0, 2] },
        ];
        mks::write(&path, "S_HDMV/PGS", &[], &blocks).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let segment = bytes.windows(4).position(|w| w == [0x18, 0x53, 0x80, 0x67]).unwrap();
        bytes[segment + 4..segment + 12].copy_from_slice(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let second = bytes.windows(4).enumerate().filter(|(_, w)| *w == [0x1F, 0x43, 0xB6, 0x75]).nth(1).unwrap().0;
        bytes.truncate(second + into);
        (bytes, second)
    }

    #[tokio::test]
    async fn picture_tracks_the_cues_list_come_whole_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let (extractor, name, key) = with_movie(dir.path());
        let wait = Duration::from_secs(10);
        // The first answer is the whole track, read through the Cues, so the
        // player stops asking...
        let (bytes, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await.unwrap();
        assert!(complete);
        assert_eq!(mks::readable(&bytes), Some(bytes.len()));
        assert!(bytes.windows(10).any(|w| w == b"S_HDMV/PGS"));
        assert!(bytes.windows(9).any(|w| w == [0xA3, 0x87, 0x81, 0, 0, 0x80, 0x16, 0, 0]), "its block, at 1 s");
        // ...cached for every later request, whole ones too.
        let cached = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        assert_eq!(std::fs::read(&cached).unwrap(), bytes);
        assert_eq!(extractor.subtitle_matroska(&name, Some(3)).await.unwrap(), bytes);
        assert_eq!(
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await.unwrap(),
            (bytes, true)
        );
        assert_eq!(extractions(&extractor), 1);
        // Text isn't pictures.
        let text = extractor.subtitle_matroska_so_far(&name, Some(2), wait, PictureStart::default(), false).await;
        assert!(matches!(text, Err(ApiError::BadRequest(_))), "{text:?}");
    }

    #[tokio::test]
    async fn picture_tracks_ffmpeg_copies_come_as_far_as_they_are_written() {
        let dir = crate::testing::tempdir();
        let mut extractor = extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), true));
        extractor.header_wait = Duration::from_millis(500);
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &unplaced_movie());
        let wait = Duration::from_millis(100);
        // FFmpeg reads the file (its Cues don't place the blocks): before it
        // has written the track's header, a Matroska file with the track and
        // no block...
        let (bytes, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await.unwrap();
        assert_eq!((bytes, complete), (mks::header_only("S_HDMV/PGS"), false));
        eventually(|| !ffmpeg_runs(dir.path()).is_empty()).await;
        let log = std::fs::read_to_string(dir.path().join("ffmpeg.log")).unwrap();
        assert!(log.contains("-map 0:3") && log.contains("-flush_packets 1") && log.contains(".mks.part"), "{log}");
        // ...then what it has written, up to its last whole cluster.
        let (written, second) = mks_being_written(dir.path(), 5);
        std::fs::write(dir.path().join("subtitles").join(format!("{key}_3.mks.part")), &written).unwrap();
        let (bytes, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await.unwrap();
        assert_eq!((bytes.as_slice(), complete), (&written[..second], false));
        assert_eq!(ffmpeg_runs(dir.path()).len(), 1, "one copy for every request");
        assert_eq!(extractions(&extractor), 1);
    }

    #[tokio::test]
    async fn picture_tracks_come_from_where_the_player_is_and_the_rest_waits_for_playbacks_starting() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(80));
        // A first answer that would wait this long for the whole track...
        let wait = Duration::from_secs(20);
        // ...comes as soon as the pictures around where the player is (from
        // half a minute before it) are written, while a playback starts...
        let starting = background.start();
        let asked = Instant::now();
        let start = PictureStart { from_ms: 50_000, media: None, asked: true };
        let (bytes, complete) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start, false).await.unwrap();
        assert!(!complete && asked.elapsed() < Duration::from_secs(10));
        assert_eq!(picture_seconds(&bytes), (20..80).collect::<Vec<u8>>());
        // ...the rest once it has started, as the player asks again: whole,
        // in order of time.
        drop(starting);
        let cached = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        eventually(|| cached.exists()).await;
        let (bytes, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await.unwrap();
        assert!(complete);
        let seconds: Vec<u8> = bytes.windows(4).filter(|w| w[..3] == [0x80, 0x16, 0]).map(|w| w[3]).collect();
        assert_eq!(seconds, (0..80).collect::<Vec<u8>>());
        assert_eq!(extractions(&extractor), 1);
        assert!(!dir.path().join("subtitles").join(window_name(&format!("{key}_3"))).exists());
    }

    #[tokio::test]
    async fn players_elsewhere_have_the_pictures_there_read_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(250));
        let wait = Duration::from_secs(20);
        let at = |from_ms: i64| PictureStart { from_ms, media: None, asked: true };
        // A playback starts 10 s in: the pictures around there, the rest
        // held up while it starts...
        let starting = background.start();
        let (bytes, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(10_000), false).await.unwrap();
        assert!(!complete);
        assert_eq!(picture_seconds(&bytes), (0..130).collect::<Vec<u8>>());
        // ...a player at 1:25 has those the read has there, without another
        // read...
        let read_there = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(85_000), false).await.unwrap();
        assert_eq!(picture_seconds(&read_there.0), (75..125).collect::<Vec<u8>>());
        let seek = || extractor.pictures.lock().get(&format!("{key}_3")).map(|e| e.seek.lock().is_some());
        assert_eq!(seek(), Some(false));
        // ...and a player that goes to 3:20 has the pictures there at once,
        // read beside it; a moment later, the same ones.
        let asked = Instant::now();
        let (there, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(200_000), false).await.unwrap();
        assert!(!complete && asked.elapsed() < Duration::from_secs(10));
        assert_eq!(picture_seconds(&there), (190..240).collect::<Vec<u8>>());
        let again = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(205_000), false).await.unwrap();
        assert_eq!(again, (there, false));
        assert_eq!(seek(), Some(true));
        // Back near where the read started: what it read there.
        let back = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(15_000), false).await.unwrap();
        assert_eq!(back, (bytes, false));
        // The whole track once the playback has started, from the same read.
        drop(starting);
        let cached = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        eventually(|| cached.exists()).await;
        assert_eq!(extractions(&extractor), 1);
    }

    #[tokio::test]
    async fn what_is_read_so_far_comes_in_order_of_time() {
        let stretch = |seconds: std::ops::Range<u8>, span: (i64, i64)| Stretch {
            codec: "S_HDMV/PGS",
            header: Vec::new(),
            blocks: seconds
                .map(|n| mks::Block { time_ms: i64::from(n) * 1_000, duration_ms: 0, data: vec![0x16, 0, n] })
                .collect(),
            span: Some(span),
        };
        let so_far = Mutex::new(SoFar::default());
        assert_eq!(so_far_bytes(&so_far).await, None, "nothing read yet");
        // Around the player, then after, then before.
        so_far.lock().add(stretch(40..44, (39_001, 44_000)));
        so_far.lock().add(stretch(44..60, (43_001, i64::MAX)));
        so_far.lock().add(stretch(0..2, (0, 2_000)));
        assert_eq!(so_far.lock().spans, [(39_001, i64::MAX), (0, 2_000)]);
        let bytes = so_far_bytes(&so_far).await.unwrap();
        let in_order: Vec<u8> = bytes.windows(4).filter(|w| w[..3] == [0x80, 0x16, 0]).map(|w| w[3]).collect();
        assert_eq!(in_order, [0, 1].into_iter().chain(40..60).collect::<Vec<u8>>());
        // The pictures around a player where all of them are read; none where
        // some may not be.
        assert_eq!(picture_seconds(&so_far.lock().around(50_000).unwrap()), (40..60).collect::<Vec<u8>>());
        assert_eq!(so_far.lock().around(20_000), None);
        assert_eq!(so_far.lock().around(5_000), None);
    }

    #[test]
    fn players_near_where_a_read_started_have_what_they_need() {
        let near_start = |from_ms, at_ms| near(from_ms, AROUND_START, at_ms);
        assert!(near_start(60_000, 60_000) && near_start(60_000, 45_000) && near_start(60_000, 120_000));
        assert!(!near_start(60_000, 44_999), "what shows there may have started before the read's");
        assert!(!near_start(60_000, 120_001), "too little ahead of the player");
        assert!(near_start(0, -5) && !near_start(i64::MAX, 0));
        assert!(near(60_000, AROUND_SEEK, 80_000) && !near(60_000, AROUND_SEEK, 80_001));
    }

    /// A read at a seek at `at_ms` as [`Extractor::seek`] leaves one: run as
    /// far as it goes on its own, and `ran_out` when it went over its time.
    fn seek_read(at_ms: i64, read: BoxFuture<'static, Option<Arc<Vec<u8>>>>, ran_out: bool) -> Seek {
        let read = read.shared();
        let _ = read.clone().now_or_never();
        Seek { at_ms, read, ran_out: Arc::new(AtomicBool::new(ran_out)) }
    }

    #[test]
    fn a_read_at_a_seek_answers_players_near_it_unless_it_went_over_its_time() {
        let got_pictures = seek_read(200_000, futures::future::ready(Some(Arc::new(vec![0x16]))).boxed(), false);
        let under_way = seek_read(200_000, futures::future::pending().boxed(), false);
        let got_nothing = seek_read(200_000, futures::future::ready(None).boxed(), false);
        let over_time = seek_read(200_000, futures::future::ready(None).boxed(), true);
        // Near where it read, one that has the pictures there, is still
        // reading them, or can't have them answers; one that went over its
        // time doesn't, so the next ask reads there again...
        assert!(got_pictures.answers(205_000) && under_way.answers(205_000) && got_nothing.answers(205_000));
        assert!(!over_time.answers(205_000));
        // ...and away from there, none does.
        assert!([got_pictures, under_way, got_nothing, over_time].iter().all(|seek| !seek.answers(100_000)));
    }

    #[tokio::test]
    async fn a_player_near_a_read_at_a_seek_that_went_over_its_time_has_it_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let extractor = extractor_without_ffmpeg(dir.path());
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(600));
        let at = |from_ms: i64| PictureStart { from_ms, media: None, asked: true };
        let wait = Duration::from_secs(20);
        // The track's read holds up after the pictures around the player.
        let _seeking = Seeking::new(extractor.seeking.clone());
        extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(0), false).await.unwrap();
        let extraction = extractor.pictures.lock().get(&format!("{key}_3")).cloned().unwrap();
        let latest = || extraction.seek.lock().as_ref().map(|seek| seek.at_ms);
        // A read at 3:20 that got nothing, the blocks there unreadable: a
        // player near it has that answer, and nothing more is read...
        *extraction.seek.lock() = Some(seek_read(200_000, futures::future::ready(None).boxed(), false));
        extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(205_000), false).await.unwrap();
        assert_eq!(latest(), Some(200_000));
        // ...while one that went over its time, the disk busy, is read again.
        *extraction.seek.lock() = Some(seek_read(200_000, futures::future::ready(None).boxed(), true));
        let (there, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(205_000), false).await.unwrap();
        assert_eq!(latest(), Some(205_000));
        assert_eq!(picture_seconds(&there), (195..245).collect::<Vec<u8>>());
    }

    /// The seconds of the pictures in a Matroska file of the track of
    /// [`matroska::tests::pictures_every_second`], in order: each picture's
    /// data (after its block's flags) ends in its second.
    fn picture_seconds(bytes: &[u8]) -> Vec<u8> {
        let mut found: Vec<u8> = bytes.windows(4).filter(|w| w[..3] == [0x80, 0x16, 0]).map(|w| w[3]).collect();
        found.sort_unstable();
        found
    }

    #[tokio::test]
    async fn the_pictures_around_the_player_stay_while_ffmpeg_copies_a_track_whose_cues_fail_past_them() {
        let dir = crate::testing::tempdir();
        let extractor = Arc::new(extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), true)));
        let mut file = matroska::tests::pictures_every_second(600);
        // A cue well after them points at the video's block: the read fails
        // there, after a stretch past them.
        file.extra_cues.push((5, 500, 0, 0));
        let (name, key) = movie_in(&extractor, dir.path(), &file);
        let start = PictureStart { from_ms: 50_000, media: None, asked: true };
        let wait = Duration::from_secs(20);
        let (around, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, start.clone(), false).await.unwrap();
        assert!(!complete);
        assert_eq!(picture_seconds(&around), (20..170).collect::<Vec<u8>>());
        // Neither what FFmpeg writes meanwhile nor more of what was read: the
        // pictures the player was given stay until the track is whole.
        eventually(|| !ffmpeg_runs(dir.path()).is_empty()).await;
        let (written, _) = mks_being_written(dir.path(), 5);
        std::fs::write(dir.path().join("subtitles").join(format!("{key}_3.mks.part")), &written).unwrap();
        let again = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start.clone(), false).await.unwrap();
        assert_eq!(again, (around, false));
        // What was read is in that file, not in memory any more.
        let so_far = extractor.pictures.lock().get(&format!("{key}_3")).map(|e| e.so_far.clone()).unwrap();
        assert!(so_far.lock().track.is_none() && so_far.lock().blocks.is_empty() && !so_far.lock().writing);
        // Once FFmpeg's copy has come past them (2:50), what it has written:
        // the pictures after them as it goes on.
        let past: Vec<mks::Block> =
            (0..200).map(|n| mks::Block { time_ms: n * 1_000, duration_ms: 0, data: vec![0x16, 0, n as u8] }).collect();
        let part = dir.path().join("subtitles").join(format!("{key}_3.mks.part"));
        mks::write(&part, "S_HDMV/PGS", &[], &past).unwrap();
        let later = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start, false).await.unwrap();
        assert_eq!(later, (std::fs::read(&part).unwrap(), false));
    }

    #[test]
    fn the_readers_of_a_tracks_rest_step_aside_together_one_of_them_waiting() {
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        let (gate, busy, turn_back) = (Mutex::new(()), AtomicBool::new(true), AtomicBool::new(false));
        let (entered, waits, gone_on) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
        // No time of its own: only the wait given to it lets it read on.
        let budget = matroska::Budget::new(u64::MAX, Duration::ZERO);
        // The one that waits has the turn back once a seek is over and the
        // turn comes.
        let wait = || {
            waits.fetch_add(1, Ordering::Relaxed);
            while !turn_back.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
            Duration::from_secs(60)
        };
        let gone_on_early = std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    entered.fetch_add(1, Ordering::Relaxed);
                    step_aside(&gate, || busy.load(Ordering::Relaxed), wait, &budget);
                    assert!(budget.check().is_ok(), "the read has the time it waited before any goes on");
                    gone_on.fetch_add(1, Ordering::Relaxed);
                });
            }
            while entered.load(Ordering::Relaxed) < 8 {
                std::thread::sleep(Duration::from_millis(1));
            }
            std::thread::sleep(Duration::from_millis(100));
            // The seek is over, the turn not back yet: one that comes now
            // waits for it too.
            busy.store(false, Ordering::Relaxed);
            scope.spawn(|| {
                step_aside(&gate, || busy.load(Ordering::Relaxed), || unreachable!(), &budget);
                gone_on.fetch_add(1, Ordering::Relaxed);
            });
            std::thread::sleep(Duration::from_millis(100));
            let early = gone_on.load(Ordering::Relaxed);
            turn_back.store(true, Ordering::Relaxed);
            early
        });
        assert_eq!(gone_on_early, 0, "none goes on before the turn is back");
        assert_eq!((waits.into_inner(), gone_on.into_inner()), (1, 9), "one of them waits");
        // Nothing to step aside for: they go on at once.
        step_aside(&gate, || false, || unreachable!(), &budget);
    }

    #[tokio::test]
    async fn the_rest_of_a_track_holds_while_a_playback_starts() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(1_000));
        // A seek holds the rest up after the pictures around the player, a
        // playback starts meanwhile, and the rest waits for that too once the
        // seek is over.
        let seek = Seeking::new(extractor.seeking.clone());
        let start = PictureStart { from_ms: 0, media: None, asked: true };
        let wait = Duration::from_secs(20);
        let (around, complete) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start, false).await.unwrap();
        assert!(!complete);
        let starting = background.start();
        drop(seek);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let so_far = extractor.pictures.lock().get(&format!("{key}_3")).map(|e| e.so_far.clone()).unwrap();
        assert_eq!(so_far.lock().blocks.len(), picture_seconds(&around).len());
        drop(starting);
        let whole = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        eventually(|| whole.exists()).await;
    }

    #[tokio::test]
    async fn the_rest_of_a_track_lets_its_turn_go_while_it_waits() {
        let dir = tempfile::tempdir().unwrap();
        // One turn for reads through the Cues.
        let extractor = extractor_without_ffmpeg(dir.path());
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(1_000));
        // A read at a seek under way holds the track's rest up after the
        // pictures around the player, as it does between stretches: it lets
        // its turn go while it waits. (With nothing to wait for, it takes its
        // turn again at once, behind the reads that asked for it meanwhile:
        // a test file's rest is read too quickly to tell that from keeping
        // it.)
        let seeking = Seeking::new(extractor.seeking.clone());
        let start = PictureStart { from_ms: 0, media: None, asked: true };
        let wait = Duration::from_secs(20);
        let (_, complete) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start, false).await.unwrap();
        assert!(!complete);
        // A text track read through the Cues meanwhile has the turn.
        let asked = Instant::now();
        let (_, complete) = extractor.subtitle_so_far(&name, 2, Duration::from_secs(5)).await.unwrap();
        assert!(complete && asked.elapsed() < Duration::from_secs(5), "{:?}", asked.elapsed());
        // The rest goes on once the read at the seek is over.
        drop(seeking);
        let whole = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        eventually(|| whole.exists()).await;
    }

    #[tokio::test]
    async fn the_rest_of_a_track_lets_seeks_go_first_those_that_start_while_it_waits_for_its_turn_too() {
        let dir = tempfile::tempdir().unwrap();
        // One turn for reads through the Cues.
        let extractor = extractor_without_ffmpeg(dir.path());
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(1_000));
        let first_seek = Seeking::new(extractor.seeking.clone());
        let start = PictureStart { from_ms: 0, media: None, asked: true };
        let wait = Duration::from_secs(20);
        let (around, complete) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start, false).await.unwrap();
        assert!(!complete);
        // The rest reads nothing past the pictures around the player while
        // the seek goes on, and another read takes the turn it lets go of.
        let turn = extractor.cue_reads.clone().acquire_owned().await.unwrap();
        let so_far = extractor.pictures.lock().get(&format!("{key}_3")).map(|e| e.so_far.clone()).unwrap();
        let read = picture_seconds(&around).len();
        assert_eq!(so_far.lock().blocks.len(), read);
        // Once the seek is over, the rest waits for that turn. A seek that
        // starts meanwhile goes first all the same: the rest gives the turn
        // back, and reads nothing more while that one lasts.
        drop(first_seek);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let second_seek = Seeking::new(extractor.seeking.clone());
        drop(turn);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(so_far.lock().blocks.len(), read);
        assert_eq!(extractor.cue_reads.available_permits(), 1, "its turn given back");
        drop(second_seek);
        let whole = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        eventually(|| whole.exists()).await;
    }

    #[tokio::test]
    async fn no_seek_is_read_through_cues_that_fail_around_the_player() {
        let dir = crate::testing::tempdir();
        let mut extractor = extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), true));
        extractor.header_wait = Duration::from_millis(500);
        let extractor = Arc::new(extractor);
        let mut file = matroska::tests::pictures_every_second(600);
        // A cue among those around the player points at the video's block.
        file.extra_cues.push((5, 60, 0, 0));
        let (name, key) = movie_in(&extractor, dir.path(), &file);
        let at = |from_ms: i64| PictureStart { from_ms, media: None, asked: true };
        let wait = Duration::from_millis(100);
        extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(50_000), false).await.unwrap();
        eventually(|| !ffmpeg_runs(dir.path()).is_empty()).await;
        assert!(extractor.unreadable.lock().contains(&format!("{key}_3")));
        // A seek has what FFmpeg has copied, nothing read through those Cues.
        let (there, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(200_000), false).await.unwrap();
        assert!(!complete);
        assert_eq!(picture_seconds(&there), Vec::<u8>::new());
    }

    #[tokio::test]
    async fn reads_at_seeks_go_on_while_ffmpeg_copies_a_track_whose_cues_fail_past_the_player() {
        let dir = crate::testing::tempdir();
        let extractor = Arc::new(extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), true)));
        let mut file = matroska::tests::pictures_every_second(600);
        file.extra_cues.push((5, 500, 0, 0));
        let (name, _) = movie_in(&extractor, dir.path(), &file);
        let at = |from_ms: i64| PictureStart { from_ms, media: None, asked: true };
        let wait = Duration::from_secs(20);
        let (around, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(50_000), false).await.unwrap();
        assert_eq!(picture_seconds(&around), (20..170).collect::<Vec<u8>>());
        eventually(|| !ffmpeg_runs(dir.path()).is_empty()).await;
        // The Cues gave the pictures around the player: a seek while FFmpeg
        // reads the file has those there, read through them at once.
        let (there, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(200_000), false).await.unwrap();
        assert!(!complete);
        assert_eq!(picture_seconds(&there), (190..240).collect::<Vec<u8>>());
    }

    #[tokio::test]
    async fn reads_at_seeks_wait_for_no_track_read_and_hold_an_answer_while_they_may_take() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let seek_wait = Duration::from_millis(500);
        extractor.seek_wait = seek_wait;
        let extractor = Arc::new(extractor);
        let (name, _) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(250));
        let at = |from_ms: i64| PictureStart { from_ms, media: None, asked: true };
        let _starting = background.start();
        let wait = Duration::from_secs(20);
        let (window, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(10_000), false).await.unwrap();
        // Every read through the Cues under way, as many as there may be: a
        // player at 3:20 has the pictures there all the same.
        let reads = extractor.cue_reads.available_permits() as u32;
        let _reads = extractor.cue_reads.clone().acquire_many_owned(reads).await.unwrap();
        let (there, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(200_000), false).await.unwrap();
        assert_eq!(picture_seconds(&there), (190..240).collect::<Vec<u8>>());
        // A read at a seek that can't start yet holds an answer up for as long
        // as such a read may take, as what the track's read has is elsewhere:
        // then that, and what is there when the player asks again.
        let seeks = extractor.seek_reads.available_permits() as u32;
        let held = extractor.seek_reads.clone().acquire_many_owned(seeks).await.unwrap();
        let short = Duration::from_millis(100);
        let asked = Instant::now();
        let meanwhile = extractor.subtitle_matroska_so_far(&name, Some(3), short, at(100_000), false).await.unwrap();
        assert!(asked.elapsed() >= seek_wait && asked.elapsed() < Duration::from_secs(5), "{:?}", asked.elapsed());
        assert_eq!(meanwhile, (window, false));
        drop(held);
        let (later, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(100_000), false).await.unwrap();
        assert_eq!(picture_seconds(&later), (90..140).collect::<Vec<u8>>());
    }

    #[tokio::test]
    async fn players_that_read_a_track_once_wait_while_all_that_is_read_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.header_wait = Duration::from_secs(5);
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(10));
        // A read through the Cues that has taken its blocks out to write them.
        let (done, written) = tokio::sync::oneshot::channel::<()>();
        let result = async move { written.await.map_err(|e| Arc::new(ApiError::Internal(e.into()))) };
        let read = PictureRead::default();
        read.so_far.lock().writing = true;
        let extraction = PictureExtraction::new(7, 0, result.boxed().shared(), read);
        extractor.pictures.lock().insert(format!("{key}_3"), extraction);
        let once = Duration::from_millis(100);
        let asking = {
            let (extractor, name) = (extractor.clone(), name.clone());
            tokio::spawn(async move {
                extractor.subtitle_matroska_so_far(&name, Some(3), once, PictureStart::default(), true).await
            })
        };
        // Past its wait, it waits for the file rather than taking what it
        // finds meanwhile...
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!asking.is_finished());
        // ...and has it whole.
        let blocks = [mks::Block { time_ms: 1_000, duration_ms: 0, data: vec![0x16, 0, 1] }];
        let target = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        mks::write(&target, "S_HDMV/PGS", &[], &blocks).unwrap();
        done.send(()).unwrap();
        let (bytes, complete) = asking.await.unwrap().unwrap();
        assert!(complete);
        assert_eq!(bytes, std::fs::read(&target).unwrap());
    }

    #[tokio::test]
    async fn reads_at_seeks_go_one_at_a_time_and_a_replaced_one_is_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let extractor = Arc::new(extractor);
        let (name, _) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(250));
        let wait = Duration::from_secs(20);
        let at = |from_ms: i64| PictureStart { from_ms, media: None, asked: true };
        let _starting = background.start();
        let (window, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(10_000), false).await.unwrap();
        // Reads at seeks wait for their turn: seeks to 3:20, 2:40 and 2:00
        // meanwhile, the last of which replaces the others.
        let turn = extractor.seek_reads.clone().acquire_owned().await.unwrap();
        let mut seeks = Vec::new();
        for seconds in [200, 160, 120] {
            let (extractor, name) = (extractor.clone(), name.clone());
            seeks.push(tokio::spawn(async move {
                extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(seconds * 1_000), false).await.unwrap()
            }));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        drop(turn);
        let mut answers = Vec::new();
        for seek in seeks {
            answers.push(seek.await.unwrap().0);
        }
        // Those replaced have what the track's read has; the last, what is
        // there.
        assert_eq!((&answers[0], &answers[1]), (&window, &window));
        assert_eq!(picture_seconds(&answers[2]), (110..160).collect::<Vec<u8>>());
    }

    /// A stand-in for the copy of picture track 3 of `key`, read through the
    /// Cues from 10 s in: what it has read goes in what this gives back, and
    /// it isn't over while the sender is kept.
    fn copy_under_way(extractor: &Extractor, key: &str) -> (Arc<Mutex<SoFar>>, tokio::sync::oneshot::Sender<()>) {
        let (unfinished, never) = tokio::sync::oneshot::channel::<()>();
        let result = async move { never.await.map_err(|e| Arc::new(ApiError::Internal(e.into()))) };
        let read = PictureRead::default();
        let so_far = read.so_far.clone();
        extractor
            .pictures
            .lock()
            .insert(format!("{key}_3"), PictureExtraction::new(7, 10_000, result.boxed().shared(), read));
        (so_far, unfinished)
    }

    /// The pictures of [`matroska::tests::pictures_every_second`]'s track at
    /// `seconds`, read through the Cues from `from_ms` to the end.
    fn read_through(seconds: std::ops::Range<u8>, from_ms: i64) -> Stretch {
        let blocks =
            seconds.map(|n| mks::Block { time_ms: i64::from(n) * 1_000, duration_ms: 0, data: vec![0x16, 0, n] });
        Stretch { codec: "S_HDMV/PGS", header: Vec::new(), blocks: blocks.collect(), span: Some((from_ms, i64::MAX)) }
    }

    #[tokio::test]
    async fn an_answer_held_for_a_read_at_a_seek_comes_once_the_copy_has_the_pictures_there() {
        let dir = tempfile::tempdir().unwrap();
        let extractor = Arc::new(extractor_with(dir.path(), dir.path().join("no-ffmpeg")));
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(250));
        let (so_far, _unfinished) = copy_under_way(&extractor, &key);
        // A read at 3:20 that can't start yet holds the answer...
        let seeks = extractor.seek_reads.available_permits() as u32;
        let _held = extractor.seek_reads.clone().acquire_many_owned(seeks).await.unwrap();
        let asking = {
            let (extractor, name) = (extractor.clone(), name.clone());
            let at = PictureStart { from_ms: 200_000, media: None, asked: true };
            let wait = Duration::from_millis(100);
            tokio::spawn(async move { extractor.subtitle_matroska_so_far(&name, Some(3), wait, at, false).await })
        };
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(!asking.is_finished());
        // ...until the copy comes past there: what it has there, at once.
        let came = Instant::now();
        so_far.lock().add(read_through(130..250, 129_001));
        let (bytes, complete) = asking.await.unwrap().unwrap();
        assert!(!complete && came.elapsed() < Duration::from_secs(2), "{:?}", came.elapsed());
        assert_eq!(picture_seconds(&bytes), (190..240).collect::<Vec<u8>>());
    }

    #[tokio::test]
    async fn a_read_at_a_seek_that_gets_nothing_leaves_the_answer_to_what_the_copy_has_there() {
        let dir = tempfile::tempdir().unwrap();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.header_wait = Duration::from_millis(300);
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(250));
        let (so_far, _unfinished) = copy_under_way(&extractor, &key);
        // Taken for another kind of file than Matroska, nothing is read at
        // 3:20, and the copy has nothing there yet: a header alone.
        let mp4 = Arc::new(MediaInfo { format_name: Some("mov,mp4,m4a,3gp,3g2,mj2".into()), ..Default::default() });
        let at = |from_ms: i64| PictureStart { from_ms, media: Some(mp4.clone()), asked: true };
        let wait = Duration::from_millis(100);
        let (first, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(200_000), false).await.unwrap();
        assert!(picture_seconds(&first).is_empty());
        // Once the copy has the pictures there, an ask near that read has
        // them, though the read is still the one there.
        so_far.lock().add(read_through(130..250, 129_001));
        let (bytes, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(205_000), false).await.unwrap();
        assert!(!complete);
        assert_eq!(picture_seconds(&bytes), (195..245).collect::<Vec<u8>>());
    }

    #[tokio::test]
    async fn reads_a_player_waits_for_hold_background_work() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(250));
        let at = |from_ms: i64| PictureStart { from_ms, media: None, asked: true };
        let wait = Duration::from_secs(20);
        assert!(!background.held());
        // A track read through its Cues holds seek previews and the like
        // until a moment after it...
        extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(10_000), false).await.unwrap();
        let cached = dir.path().join("subtitles").join(format!("{key}_3.mks"));
        eventually(|| cached.exists()).await;
        assert!(background.held());
        eventually(|| !background.held()).await;
        // ...and so does a read at a seek, beside a copy under way.
        std::fs::remove_file(&cached).unwrap();
        let (_so_far, _unfinished) = copy_under_way(&extractor, &key);
        let (there, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, at(200_000), false).await.unwrap();
        assert_eq!(picture_seconds(&there), (190..240).collect::<Vec<u8>>());
        assert!(background.held());
        eventually(|| !background.held()).await;
    }

    #[tokio::test]
    async fn a_text_track_read_through_the_cues_holds_background_work() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let extractor = Arc::new(extractor);
        let (name, _) = movie_in(&extractor, dir.path(), &matroska::tests::movie());
        assert!(!background.held());
        // A player's text track, read through the Cues, holds seek previews
        // and the like until a moment after it.
        let (_, complete) = extractor.subtitle_so_far(&name, 2, Duration::from_secs(10)).await.unwrap();
        assert!(complete);
        assert!(background.held());
        eventually(|| !background.held()).await;
    }

    #[test]
    fn spans_start_with_the_track_and_are_none_when_they_cant_be_told() {
        let stretch = |span, whole_ms| {
            let found = matroska::PictureTracks {
                tick_ns: 1_000_000,
                whole_ms,
                tracks: vec![matroska::SubtitleTrack {
                    stream_index: 3,
                    codec: "S_HDMV/PGS",
                    header: Vec::new(),
                    blocks: Vec::new(),
                }],
                reads: 0,
                bytes_read: 0,
                more: true,
                span,
            };
            let media = MediaInfo { start_ms: Some(1_000), ..Default::default() };
            stretch_of(found, &media, 3).unwrap().span
        };
        // From the file's start: from the track's, whenever the file starts.
        assert_eq!(stretch(Some((0, 5_000)), true), Some((i64::MIN, 4_000)));
        assert_eq!(stretch(Some((2_001, u64::MAX)), true), Some((1_001, i64::MAX)));
        assert_eq!(stretch(None, true), None);
        // Times FFmpeg's can't be told from: no span, and no error.
        assert_eq!(stretch(Some((2_001, 5_000)), false), None);
    }

    #[tokio::test]
    async fn players_that_read_a_track_once_have_all_that_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), dir.path().join("no-ffmpeg"));
        extractor.background = Some(background.clone());
        let extractor = Arc::new(extractor);
        let (name, _) = movie_in(&extractor, dir.path(), &matroska::tests::pictures_every_second(250));
        let wait = Duration::from_secs(20);
        let _starting = background.start();
        let start = PictureStart { from_ms: 10_000, media: None, asked: true };
        let (around, _) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start, false).await.unwrap();
        // A player that reads it once, and says nothing of where it is: after
        // a wait for all of it, what the read has so far, the same pictures
        // while it waits for the playback to start.
        let once = Duration::from_millis(300);
        let asked = Instant::now();
        let (all, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), once, PictureStart::default(), true).await.unwrap();
        assert!(!complete && asked.elapsed() >= once);
        assert_eq!(picture_seconds(&all), picture_seconds(&around));
    }

    #[tokio::test]
    async fn picture_tracks_need_no_probe_with_the_callers_media() {
        let dir = tempfile::tempdir().unwrap();
        let extractor = extractor_without_ffmpeg(dir.path());
        let name = matroska::tests::movie().write(dir.path()).to_string_lossy().into_owned();
        let wait = Duration::from_secs(10);
        // No ffprobe here: without the media, no copy...
        assert!(
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await.is_err()
        );
        // ...with it, the track read through the Cues.
        let media = MediaInfo { duration_ms: Some(10_000), start_ms: Some(0), ..matroska::tests::movie_media() };
        let start = PictureStart::new(Some(0), Some(media.clone()));
        let (bytes, complete) = extractor.subtitle_matroska_so_far(&name, Some(3), wait, start, false).await.unwrap();
        assert!(complete && bytes.windows(10).any(|w| w == b"S_HDMV/PGS"));
        // Media that doesn't say when the file starts is no use.
        assert!(PictureStart::new(None, Some(MediaInfo { start_ms: None, ..media })).media.is_none());
    }

    #[tokio::test]
    async fn picture_copies_by_ffmpeg_wait_for_playbacks_starting() {
        let dir = crate::testing::tempdir();
        let background = Background::new();
        let mut extractor = extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), false));
        extractor.background = Some(background.clone());
        extractor.header_wait = Duration::from_millis(300);
        let extractor = Arc::new(extractor);
        let (name, _) = movie_in(&extractor, dir.path(), &unplaced_movie());
        // While a playback starts, FFmpeg doesn't read the file (the
        // playback reads the same disk)...
        let starting = background.start();
        let wait = Duration::from_millis(100);
        let (bytes, complete) =
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await.unwrap();
        assert_eq!((bytes, complete), (mks::header_only("S_HDMV/PGS"), false));
        assert!(ffmpeg_runs(dir.path()).is_empty());
        // ...then it does.
        drop(starting);
        eventually(|| !ffmpeg_runs(dir.path()).is_empty()).await;
    }

    #[tokio::test]
    async fn a_failed_picture_copy_is_told_rather_than_run_again() {
        let dir = crate::testing::tempdir();
        let extractor = Arc::new(extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), false)));
        let (name, key) = movie_in(&extractor, dir.path(), &unplaced_movie());
        let wait = Duration::from_millis(100);
        assert!(matches!(
            extractor.subtitle_matroska_so_far(&name, Some(3), wait, PictureStart::default(), false).await,
            Err(ApiError::Failed(_))
        ));
        for _ in 0..3 {
            assert!(matches!(extractor.subtitle_matroska(&name, Some(3)).await, Err(ApiError::Failed(_))));
        }
        assert_eq!(ffmpeg_runs(dir.path()).len(), 1);
        assert!(!dir.path().join("subtitles").join(format!("{key}_3.mks.part")).exists(), "nothing left behind");
        // A subtitle file of its own is copied whole by FFmpeg (which fails here).
        let sup = dir.path().join("movie.en.sup");
        std::fs::write(&sup, b"PG").unwrap();
        assert!(extractor.subtitle_matroska(&sup.to_string_lossy(), None).await.is_err());
        let log = std::fs::read_to_string(dir.path().join("ffmpeg.log")).unwrap();
        assert!(log.lines().nth(1).is_some_and(|run| run.contains("-map 0:s:0")), "{log}");
    }

    #[tokio::test]
    async fn passes_keep_their_outputs_from_being_pruned() {
        let dir = crate::testing::tempdir();
        let mut extractor = extractor_with(dir.path(), stand_in_ffmpeg(dir.path(), true));
        extractor.keep_fresh = Duration::from_millis(50);
        let extractor = Arc::new(extractor);
        let (name, key) = movie_in(&extractor, dir.path(), &unplaced_movie());
        let part = dir.path().join("subtitles").join(format!("{key}_4.vtt.part"));
        std::fs::write(&part, "WEBVTT\n").unwrap();
        let file = std::fs::File::options().write(true).open(&part).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(4 * 3600)).unwrap();
        drop(file);
        // A track with no cue for hours, its pass still running, is marked as
        // being written...
        let pass = extractor.extraction(Path::new(&name), &key, 2);
        eventually(|| {
            let modified = std::fs::metadata(&part).and_then(|m| m.modified());
            modified.is_ok_and(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(3600))
        })
        .await;
        // ...so the cache's pruning, which takes old temporaries for abandoned
        // ones, keeps it.
        let policy = crate::cache::Policy { max_age: Duration::from_secs(3600), max_bytes: u64::MAX };
        crate::cache::prune(&dir.path().join("subtitles"), &policy, SystemTime::now() + Duration::from_secs(60));
        assert!(part.exists());
        drop(pass);
    }

    /// FFmpeg and ffprobe from PATH, and the media facts of `path`.
    async fn tools(path: &Path) -> (Arc<Ffmpeg>, Arc<Prober>, Arc<Metrics>, Arc<MediaInfo>) {
        let caps =
            pley_transcoder_api::Capabilities { ffmpeg_available: true, ffprobe_available: true, ..Default::default() };
        let ff =
            Arc::new(Ffmpeg { ffmpeg: "ffmpeg".into(), ffprobe: "ffprobe".into(), caps, segment_times_relative: true });
        let metrics = Arc::new(Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics.clone(), 1));
        let media = prober.probe(path).await.expect("ffprobe reads it");
        (ff, prober, metrics, media)
    }

    /// Room to read every track of a real file, however many it has.
    fn no_limit() -> matroska::Budget {
        matroska::Budget::new(u64::MAX, Duration::from_secs(3600))
    }

    /// Every text track of a local Matroska file, read through its Cues,
    /// is what FFmpeg extracts, byte for byte:
    /// `PLEY_TEST_MKV=<file> cargo test -p pley-transcoder cues_match_ffmpeg -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn cues_match_ffmpeg() {
        let path = PathBuf::from(std::env::var("PLEY_TEST_MKV").expect("set PLEY_TEST_MKV"));
        let dir = tempfile::tempdir().unwrap();
        let (ff, prober, metrics, media) = tools(&path).await;
        let streams: Vec<i64> = media.streams.iter().filter(|s| s.is_text_subtitle()).map(|s| s.index).collect();
        let started = Instant::now();
        let ours = cue_tracks(&path, &media, &streams, &no_limit()).expect("the cues list the subtitles");
        println!("{} tracks through the cues in {:?}", ours.len(), started.elapsed());
        let extractor = Extractor::new(ff, prober, metrics, dir.path().join("subtitles"), 1);
        let started = Instant::now();
        extractor.extract_vtt(&path, "ffmpeg", &streams).await.expect("ffmpeg extracts them");
        println!("{} tracks by ffmpeg in {:?}", streams.len(), started.elapsed());
        for (index, text) in &ours {
            let theirs =
                std::fs::read_to_string(dir.path().join("subtitles").join(format!("ffmpeg_{index}.vtt"))).unwrap();
            assert_eq!(text, &theirs, "stream {index}");
            println!("stream {index}: {} cues, identical", text.matches(" --> ").count());
        }
    }

    /// How long reading a file's text tracks through its Cues takes, and how
    /// much it reads, with no FFmpeg pass: the first track, then all of them
    /// (or each of `PLEY_TEST_STREAMS=3,5`):
    /// `PLEY_TEST_MKV=<file> cargo test -p pley-transcoder cues_reading_time -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn cues_reading_time() {
        let path = PathBuf::from(std::env::var("PLEY_TEST_MKV").expect("set PLEY_TEST_MKV"));
        let (_, _, _, media) = tools(&path).await;
        let streams: Vec<i64> = media.streams.iter().filter(|s| s.is_text_subtitle()).map(|s| s.index).collect();
        let only = std::env::var("PLEY_TEST_STREAMS").ok();
        let runs: Vec<Vec<i64>> = match &only {
            Some(list) => list.split(',').map(|s| vec![s.trim().parse().expect("stream indexes")]).collect(),
            None => vec![streams[..1].to_vec(), streams.clone()],
        };
        for wanted in runs {
            let started = Instant::now();
            let budget = no_limit();
            let found = matroska::text_tracks(&path, &media, &wanted, &budget).expect("the cues list the subtitles");
            let took = started.elapsed();
            let (reads, kb) = (found.reads, found.bytes_read / 1024);
            let blocks: usize = found.tracks.iter().map(|t| t.blocks.len()).sum();
            let vtt = as_webvtt(found, &media, &budget).unwrap();
            println!(
                "stream(s) {:?}: {blocks} subtitles in {took:?} ({reads} reads, {kb} KB), {} KB of WebVTT",
                if wanted.len() > 3 { format!("{} tracks", wanted.len()) } else { format!("{wanted:?}") },
                vtt.iter().map(|(_, t)| t.len()).sum::<usize>() / 1024
            );
        }
    }

    /// Stream 2, a SubRip track, as read through the Cues: blocks of
    /// `(time, duration, text)` in ticks of `tick_ns`.
    fn read(tick_ns: u64, blocks: &[(i64, u64, &str)]) -> matroska::TextTracks {
        let blocks = blocks
            .iter()
            .map(|&(time, duration, text)| matroska::TextBlock { time, duration, data: text.as_bytes().to_vec() })
            .collect();
        let track =
            matroska::SubtitleTrack { stream_index: 2, codec: webvtt::TextCodec::Subrip, header: Vec::new(), blocks };
        let whole_ms = tick_ns.is_multiple_of(1_000_000);
        matroska::TextTracks {
            tick_ns,
            whole_ms,
            tracks: vec![track],
            reads: 0,
            bytes_read: 0,
            more: false,
            span: None,
        }
    }

    /// Cue times are FFmpeg's: the file's start time taken off, block times
    /// rounded to microseconds and then to milliseconds, half away from zero.
    #[test]
    fn cue_times_are_ffmpegs() {
        let vtt = |found, start_ms| {
            let media = MediaInfo { start_ms, ..Default::default() };
            as_webvtt(found, &media, &no_limit()).map(|mut tracks| tracks.remove(0).1)
        };
        // Ticks of a millisecond, a file starting at 5.5 s.
        let found = read(1_000_000, &[(5_500, 1_000, "first"), (6_000, 1_500, "second")]);
        let first = "WEBVTT\n\n00:00.000 --> 00:01.000\nfirst\n\n00:00.500 --> 00:02.000\nsecond\n";
        assert_eq!(vtt(found, Some(5_500)).unwrap(), first);
        // Ticks of 100 µs: 0.5 ms is 1 ms, and so is 1.4 ms.
        let found = read(100_000, &[(5, 10, "half"), (14, 5, "rounded")]);
        let rounded = "WEBVTT\n\n00:00.001 --> 00:00.002\nhalf\n\n00:00.001 --> 00:00.002\nrounded\n";
        assert_eq!(vtt(found, None).unwrap(), rounded);
        // Ticks of 333333 ns: 3 of them are 999.999 µs, so 1000, so 1 ms...
        let found = read(333_333, &[(3, 3, "third")]);
        assert_eq!(vtt(found, None).unwrap(), "WEBVTT\n\n00:00.001 --> 00:00.002\nthird\n");
        // ...and 500003 of them, 166667.499999 ms, are 166667500 µs, so
        // 166668 ms (not 166667, as rounded at once).
        let found = read(333_333, &[(500_003, 3, "twice")]);
        assert_eq!(vtt(found, None).unwrap(), "WEBVTT\n\n02:46.668 --> 02:46.669\ntwice\n");
        // With such ticks, a start the media info has in milliseconds
        // (1.833332 s as 1833) may not be FFmpeg's: left to it. So is the
        // start of 0 that ffprobe gives most files with audio or video,
        // which may be a fraction of a millisecond.
        let found = read(333_333, &[(5_500, 3, "a")]);
        assert_eq!(vtt(found, Some(1_833)).unwrap_err(), "the file's start isn't known to the microsecond");
        let found = read(333_333, &[(3, 3, "third")]);
        assert_eq!(vtt(found, Some(0)).unwrap_err(), "the file's start isn't known to the microsecond");
        // A cue before the start, or going back in time: FFmpeg's muxer
        // would shift or refuse them, so they are left to it too.
        assert!(vtt(read(1_000_000, &[(5_000, 100, "early")]), Some(5_500)).is_err());
        assert!(vtt(read(1_000_000, &[(2_000, 100, "b"), (1_000, 100, "a")]), None).is_err());
        assert!(vtt(read(1_000_000, &[(1_000, 100, "a"), (1_000, 100, "b")]), None).is_ok(), "at once");
    }

    #[test]
    fn cues_too_long_or_too_late_are_left_to_ffmpeg() {
        let media = MediaInfo::default();
        let vtt = |found| as_webvtt(found, &media, &no_limit()).map(|mut tracks| tracks.remove(0).1);
        // FFmpeg keeps how long a cue lasts in 32 bits of milliseconds.
        let longest = vtt(read(1_000_000, &[(0, 0xFFFF_FFFF, "a")])).unwrap();
        assert_eq!(longest, "WEBVTT\n\n00:00.000 --> 1193:02:47.295\na\n");
        assert!(vtt(read(1_000_000, &[(0, 0x1_0000_0000, "a")])).is_err());
        assert!(vtt(read(1_000_000, &[(0, u64::MAX, "a")])).is_err());
        assert!(vtt(read(1_000_000, &[(i64::MAX, 1, "a")])).is_err(), "too far to tell in microseconds");
        assert!(vtt(read(u64::MAX, &[(1, 1, "a")])).is_err(), "a timestamp scale out of range");
    }

    #[test]
    fn rescaling_rounds_like_ffmpeg() {
        let (ms, us, ns) = ((1, 1000), (1, 1_000_000), (1, 1_000_000_000));
        assert_eq!(rescale(1_500, ms, us), Some(1_500_000));
        assert_eq!(rescale(1_500, us, ms), Some(2), "half away from zero");
        assert_eq!(rescale(-1_500, us, ms), Some(-2));
        assert_eq!(rescale(1_499, us, ms), Some(1));
        assert_eq!(rescale(3, (100_000, 1_000_000_000), ms), Some(0), "0.3 ms");
        assert_eq!(rescale(6_500_000, ns, ms), Some(7));
        assert_eq!(rescale(i64::MAX / 2, ms, us), None, "too far to tell");
    }

    #[test]
    fn whole_file_passes_get_time_for_big_files() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("small.mkv");
        std::fs::write(&small, b"tiny").unwrap();
        assert_eq!(whole_file_timeout(&small), Duration::from_secs(20 * 60));
        assert_eq!(whole_file_timeout(&dir.path().join("missing.mkv")), Duration::from_secs(20 * 60));
        assert_eq!(pass_timeout(5_000_000_000), Duration::from_secs(20 * 60));
        assert_eq!(pass_timeout(77_098_924_052), Duration::from_secs(7_709), "a 77 GB 4K remux: over 2 hours");
        // A picture track's read through the Cues: a minute, or as long as
        // FFmpeg would take at 100 MB/s.
        assert_eq!(picture_read_timeout(&small), Duration::from_secs(60));
        assert_eq!(picture_read_timeout(&dir.path().join("missing.mkv")), Duration::from_secs(60));
        assert_eq!(picture_read_time(1_100_000_000), Duration::from_secs(60), "an episode");
        assert_eq!(picture_read_time(81_700_000_000), Duration::from_secs(817), "a remux: over 13 minutes");
    }

    #[test]
    fn charset_guess() {
        assert_eq!(guess_charset("héllo".as_bytes()), None);
        assert_eq!(guess_charset(&[b'h', 0xE9, b'l']), Some("CP1252"));
        assert_eq!(guess_charset(&[0xFF, 0xFE, b'h', 0]), None);
        // A first megabyte that cuts "é" in two.
        assert_eq!(guess_charset(&"hé".as_bytes()[..2]), None);
    }

    #[tokio::test]
    async fn subtitle_files_too_big_to_be_text_are_refused_unread() {
        let dir = tempfile::tempdir().unwrap();
        let extractor = extractor_without_ffmpeg(dir.path());
        let sup = dir.path().join("movie.en.sup");
        let name = sup.to_string_lossy().into_owned();
        std::fs::File::create(&sup).unwrap().set_len(MAX_SUBTITLE_FILE + 1).unwrap();
        // Refused as it is, neither read nor given to FFmpeg (not there).
        match extractor.subtitle(&name, None).await {
            Err(ApiError::BadRequest(message)) => assert!(message.contains("64 MB at most"), "{message}"),
            other => panic!("{other:?}"),
        }
        // One just small enough goes to FFmpeg, its charset told from its start.
        std::fs::File::options().write(true).open(&sup).unwrap().set_len(MAX_SUBTITLE_FILE).unwrap();
        assert!(matches!(extractor.subtitle(&name, None).await, Err(ApiError::Internal(_))));
        assert_eq!(read_head(&sup, CHARSET_SAMPLE).await.unwrap().len(), CHARSET_SAMPLE);
    }
}
