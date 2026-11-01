// SPDX-License-Identifier: AGPL-3.0-only
//! Seek-bar preview thumbnails ("trickplay"): one small frame every few
//! seconds, tiled into JPEG sprite sheets. Only keyframes are decoded, so even
//! a long film takes seconds to a minute (paused while a playback starts or a
//! transcode runs, see [`crate::background`]); HDR and Dolby Vision are
//! tone-mapped so previews look like the transcoded picture. Results are cached per file
//! version until every sheet has been fetched (the server keeps its own copy),
//! or for a day (see `cache.rs`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use pley_transcoder_api::{Tonemapper, TrickplayInfo, TrickplayRequest};
use tokio::sync::Semaphore;

use crate::background::Background;
use crate::error::{ApiError, ApiResult};
use crate::extract::{Lock, Locks};
use crate::ffmpeg::Ffmpeg;
use crate::plan::{PLACEBO_STATIC_PEAK, ZSCALE_TONEMAP, untagged_hdr_fix};
use crate::probe::Prober;
use crate::util::{MEDIA_INPUT_OPTIONS, background_priority, file_key, input_arg, touch};

const COLUMNS: u32 = 10;
const ROWS: u32 = 10;
const TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// Sets whose fetched sheets are tracked at once (sets fetched only in part
/// are forgotten beyond that, and left to cache pruning).
const MAX_TRACKED: usize = 1000;

pub struct Trickplay {
    ff: Arc<Ffmpeg>,
    prober: Arc<Prober>,
    background: Arc<Background>,
    dir: PathBuf,
    /// One generation at a time: it is background work.
    limiter: Semaphore,
    /// One generation per set, the lock forgotten however it ends.
    locks: Locks,
    /// Sheets fetched so far, per set.
    served: Mutex<HashMap<String, HashSet<u32>>>,
}

/// Thumbnails every 2–10 s: about 360 for a film, more density for short clips.
pub fn default_interval_ms(duration_ms: i64) -> i64 {
    ((duration_ms / 360).clamp(2_000, 10_000) / 1_000) * 1_000
}

impl Trickplay {
    pub fn new(ff: Arc<Ffmpeg>, prober: Arc<Prober>, background: Arc<Background>, dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self {
            ff,
            prober,
            background,
            dir,
            limiter: Semaphore::new(1),
            locks: Mutex::default(),
            served: Mutex::default(),
        }
    }

    pub fn sheet_path(&self, id: &str, n: u32) -> Option<PathBuf> {
        valid_id(id).then(|| self.dir.join(id).join(format!("{n}.jpg"))).filter(|p| p.is_file())
    }

    /// Note that sheet `n` of set `id` has been read for a client. Once every
    /// sheet of the set has been, the set is deleted: the server stores the
    /// sheets it fetches, so this copy would only take space. Blocking.
    pub fn sheet_served(&self, id: &str, n: u32) {
        if !valid_id(id) {
            return;
        }
        let dir = self.dir.join(id);
        let Some(info) = read_index(&dir) else { return };
        let complete = {
            let mut served = self.served.lock();
            if served.len() >= MAX_TRACKED && !served.contains_key(id) {
                served.clear();
            }
            let sheets = served.entry(id.to_owned()).or_default();
            sheets.insert(n);
            let complete = (0..info.sheets).all(|k| sheets.contains(&k));
            if complete {
                served.remove(id);
            }
            complete
        };
        if complete {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => tracing::debug!("seek previews {id} fetched in full, removed"),
                Err(err) => tracing::debug!("could not remove seek previews {id}: {err}"),
            }
        }
    }

    pub async fn generate(&self, req: &TrickplayRequest) -> ApiResult<TrickplayInfo> {
        let path = Path::new(&req.path);
        if !path.is_file() {
            return Err(ApiError::NotFound(format!("file not found: {}", req.path)));
        }
        let media = self.prober.probe(path).await.map_err(|e| ApiError::Failed(format!("probe failed: {e:#}")))?;
        let video = media.video().ok_or_else(|| ApiError::BadRequest("the file has no video".into()))?;
        let duration =
            media.duration_ms.filter(|d| *d > 0).ok_or_else(|| ApiError::BadRequest("unknown duration".into()))?;
        let interval = req.interval_ms.unwrap_or_else(|| default_interval_ms(duration)).clamp(1_000, 60_000);
        let width = req.width.unwrap_or(320).clamp(96, 640);
        let (src_w, src_h) = (video.width.unwrap_or(1920).max(2), video.height.unwrap_or(1080).max(2));
        let height = ((width as f64 * src_h as f64 / src_w as f64 / 2.0).round() as u32 * 2).max(2);

        let id = format!("{}-{interval}-{width}", file_key(path)?);
        let dir = self.dir.join(&id);
        if let Some(info) = read_index(&dir) {
            touch(&dir.join("index.json"));
            return Ok(info);
        }
        let entry = Lock::acquire(&self.locks, &id);
        let _guard = entry.lock.lock().await;
        if let Some(info) = read_index(&dir) {
            touch(&dir.join("index.json"));
            return Ok(info);
        }
        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;

        let range = video.dynamic_range();
        let caps = &self.ff.caps;
        let placebo = range.is_hdr() && caps.tonemappers.contains(&Tonemapper::Libplacebo);
        let picture = if placebo {
            // Handles Dolby Vision profile 5 too (its RPU is on every keyframe).
            // Tone mapped as the video is, by the stream's metadata.
            format!(
                "libplacebo=w={width}:h={height}:format=yuv420p:colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv:tonemapping=auto:{PLACEBO_STATIC_PEAK}"
            )
        } else if range.is_hdr() && caps.tonemappers.contains(&Tonemapper::Zscale) {
            // zimg needs colours: Dolby Vision profile 5 frames have none.
            let tags = untagged_hdr_fix(video).map(|f| format!("{f},")).unwrap_or_default();
            format!("{tags}scale={width}:{height}:flags=bilinear,{ZSCALE_TONEMAP}")
        } else {
            format!("scale={width}:{height}:flags=bilinear")
        };
        let filter = format!("fps=1000/{interval},{picture},tile={COLUMNS}x{ROWS}");

        let part = self.dir.join(format!("{id}.part"));
        let _ = tokio::fs::remove_dir_all(&part).await;
        tokio::fs::create_dir_all(&part).await?;
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error"]);
        if placebo {
            cmd.args(["-init_hw_device", "vulkan=vk", "-filter_hw_device", "vk"]);
        }
        // Decoding keyframes only is what makes this cheap.
        cmd.args(["-skip_frame", "nokey"]).args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(path));
        cmd.args(["-map", &format!("0:{}", video.index), "-an", "-sn", "-dn", "-vf", &filter]);
        cmd.args(["-fps_mode", "passthrough", "-c:v", "mjpeg", "-q:v", "5", "-start_number", "0"]);
        cmd.arg(part.join("%d.jpg"));
        background_priority(&mut cmd);
        let started = std::time::Instant::now();
        let result = self.background.run(cmd, TIMEOUT).await;
        let sheets = count_sheets(&part);
        if let Err(err) = result {
            let _ = tokio::fs::remove_dir_all(&part).await;
            return Err(err);
        }
        if sheets == 0 {
            let _ = tokio::fs::remove_dir_all(&part).await;
            return Err(ApiError::Failed("no thumbnails could be made".into()));
        }
        let per_sheet = COLUMNS * ROWS;
        let count = ((duration + interval - 1) / interval) as u32;
        let info = TrickplayInfo {
            id: id.clone(),
            interval_ms: interval,
            width,
            height,
            columns: COLUMNS,
            rows: ROWS,
            count: count.min(sheets * per_sheet),
            sheets,
        };
        tokio::fs::write(part.join("index.json"), serde_json::to_vec(&info).map_err(anyhow::Error::from)?).await?;
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::rename(&part, &dir).await?;
        tracing::info!(
            "seek previews for {} ({} thumbnails) made in {:.1}s",
            path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
            info.count,
            started.elapsed().as_secs_f32()
        );
        Ok(info)
    }
}

/// Set ids are file keys plus numbers: nothing that could leave the folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn read_index(dir: &Path) -> Option<TrickplayInfo> {
    serde_json::from_slice(&std::fs::read(dir.join("index.json")).ok()?).ok()
}

fn count_sheets(dir: &Path) -> u32 {
    (0..).take_while(|n| dir.join(format!("{n}.jpg")).is_file()).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_scale_with_length() {
        assert_eq!(default_interval_ms(2 * 3_600_000), 10_000);
        assert_eq!(default_interval_ms(45 * 60_000), 7_000);
        assert_eq!(default_interval_ms(20 * 60_000), 3_000);
        assert_eq!(default_interval_ms(60_000), 2_000);
    }

    #[tokio::test]
    async fn failed_generations_leave_no_lock_behind() {
        use pley_transcoder_api::{MediaInfo, StreamKind};
        let dir = tempfile::tempdir().unwrap();
        let ff = Arc::new(Ffmpeg {
            ffmpeg: dir.path().join("no-ffmpeg"),
            ffprobe: dir.path().join("no-ffprobe"),
            caps: Default::default(),
            segment_times_relative: true,
        });
        let prober = Arc::new(Prober::new(ff.clone(), Arc::new(crate::metrics::Metrics::default()), 1));
        let trickplay = Trickplay::new(ff, prober.clone(), Background::new(), dir.path().join("trickplay"));
        let movie = dir.path().join("movie.mkv");
        std::fs::write(&movie, b"not really a movie").unwrap();
        let video = crate::matroska::tests::stream(0, StreamKind::Video, "h264");
        prober.remember(&movie, MediaInfo { duration_ms: Some(60_000), streams: vec![video], ..Default::default() });
        let req = TrickplayRequest { path: movie.to_string_lossy().into_owned(), ..Default::default() };
        // FFmpeg isn't there: the set isn't made, and its lock goes.
        assert!(trickplay.generate(&req).await.is_err());
        assert!(trickplay.locks.lock().is_empty());
    }
}
