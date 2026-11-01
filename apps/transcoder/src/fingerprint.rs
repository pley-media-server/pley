// SPDX-License-Identifier: AGPL-3.0-only
//! Audio fingerprints of parts of a file, computed by FFmpeg's `chromaprint`
//! muxer. The server compares them across the episodes of a season to find
//! the intro and credits they share. And how dark the key frames of part of
//! a video are, for credits with no audio in common (text on black).
//! Background work: few at a time, at low process priority, and paused
//! while a playback starts or a transcode runs (see [`crate::background`]),
//! so playback never waits for it.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pley_transcoder_api::{Fingerprint, FingerprintRequest, FrameStats, Frames, FramesRequest};
use tokio::sync::Semaphore;

use crate::background::Background;
use crate::error::{ApiError, ApiResult};
use crate::ffmpeg::Ffmpeg;
use crate::util::{MEDIA_INPUT_OPTIONS, background_priority, input_arg, secs};

/// Chromaprint's default algorithm works on 11025 Hz audio in frames that
/// advance by 1365 samples: one value per 0.1238 s.
pub const ITEM_SECONDS: f64 = 1365.0 / 11025.0;
const MAX_DURATION_MS: i64 = 30 * 60_000;
const TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub struct Fingerprinter {
    ff: Arc<Ffmpeg>,
    background: Arc<Background>,
    limiter: Semaphore,
}

impl Fingerprinter {
    pub fn new(ff: Arc<Ffmpeg>, background: Arc<Background>, concurrency: usize) -> Self {
        Self { ff, background, limiter: Semaphore::new(concurrency.max(1)) }
    }

    pub async fn fingerprint(&self, req: &FingerprintRequest) -> ApiResult<Fingerprint> {
        if !self.ff.caps.chromaprint {
            return Err(ApiError::BadRequest("this FFmpeg build has no chromaprint muxer".into()));
        }
        let path = Path::new(&req.path);
        if !path.is_file() {
            return Err(ApiError::NotFound(format!("file not found: {}", req.path)));
        }
        let start_ms = req.start_ms.max(0);
        let duration_ms = req.duration_ms.clamp(1_000, MAX_DURATION_MS);
        let map = match req.stream_index {
            Some(index) => format!("0:{index}"),
            None => "0:a:0".to_owned(),
        };

        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error"]);
        if start_ms > 0 {
            cmd.args(["-ss", &secs(start_ms)]);
        }
        cmd.args(["-t", &secs(duration_ms)]).args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(path));
        // Mono 11025 Hz is what Chromaprint analyses anyway; resampling here
        // is cheaper than letting it resample a 48 kHz surround track.
        cmd.args(["-map", &map, "-vn", "-sn", "-dn", "-ac", "1", "-ar", "11025"]);
        cmd.args(["-f", "chromaprint", "-fp_format", "raw", "-"]);
        background_priority(&mut cmd);

        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        let (raw, _) = self.background.run(cmd, TIMEOUT).await?;
        Ok(Fingerprint { start_ms, item_seconds: ITEM_SECONDS, data: parse_raw(&raw) })
    }

    /// Darkness and contrast of the key frames of part of the first video
    /// stream. Only key frames are decoded (a few seconds apart), shrunk
    /// before measuring: cheap next to reading the file.
    pub async fn frames(&self, req: &FramesRequest) -> ApiResult<Frames> {
        let path = Path::new(&req.path);
        if !path.is_file() {
            return Err(ApiError::NotFound(format!("file not found: {}", req.path)));
        }
        let start_ms = req.start_ms.max(0);
        let duration_ms = req.duration_ms.clamp(1_000, MAX_DURATION_MS);
        let mut cmd = self.ff.command();
        // The measurements come through the log.
        cmd.args(["-nostats", "-loglevel", "info", "-skip_frame", "nokey"]);
        if start_ms > 0 {
            cmd.args(["-ss", &secs(start_ms)]);
        }
        cmd.args(["-t", &secs(duration_ms)]).args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(path));
        cmd.args(["-map", "0:V:0", "-an", "-sn", "-dn", "-vf", FRAME_STATS, "-f", "null", "-"]);
        background_priority(&mut cmd);

        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        let (_, log) = self.background.run(cmd, TIMEOUT).await?;
        Ok(Frames { frames: parse_frame_stats(&log, start_ms) })
    }
}

/// Small 8-bit frames (HDR ones too), then the share of near-black pixels and
/// the luma and saturation statistics of each.
const FRAME_STATS: &str = "scale=320:-2,format=yuv420p,blackframe=amount=0:threshold=32,signalstats,metadata=print";

/// The `metadata=print` lines of an FFmpeg log: a `frame:… pts_time:…` line
/// opens each frame, `lavfi.<filter>.<key>=<value>` lines follow.
fn parse_frame_stats(log: &str, start_ms: i64) -> Vec<FrameStats> {
    let mut frames: Vec<FrameStats> = Vec::new();
    for line in log.lines().filter(|l| l.contains("Parsed_metadata")) {
        let text = line.split_once("] ").map_or(line, |(_, rest)| rest).trim();
        if let Some(time) = text.split_whitespace().find_map(|w| w.strip_prefix("pts_time:")) {
            if let Ok(seconds) = time.parse::<f64>() {
                frames.push(FrameStats { ms: start_ms + (seconds * 1000.0).round() as i64, ..Default::default() });
            }
            continue;
        }
        let (Some(frame), Some((key, value))) = (frames.last_mut(), text.split_once('=')) else { continue };
        let Ok(value) = value.parse::<f32>() else { continue };
        match key {
            "lavfi.blackframe.pblack" => frame.black = value,
            "lavfi.signalstats.YLOW" => frame.ylow = value,
            "lavfi.signalstats.YHIGH" => frame.yhigh = value,
            "lavfi.signalstats.YMAX" => frame.ymax = value,
            "lavfi.signalstats.SATAVG" => frame.sat = value,
            _ => {}
        }
    }
    frames.sort_by_key(|f| f.ms);
    frames
}

/// `fp_format=raw` writes the values as native-endian (little-endian on every
/// platform FFmpeg runs on) 32-bit integers.
fn parse_raw(bytes: &[u8]) -> Vec<u32> {
    bytes.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frame_stats_from_the_log() {
        let log = "Input #0, matroska,webm, from 'x.mkv':\n\
            [Parsed_metadata_4 @ 000002341e3513c0] frame:0    pts:259     pts_time:0.259\n\
            [Parsed_metadata_4 @ 000002341e3513c0] lavfi.blackframe.pblack=95\n\
            [Parsed_metadata_4 @ 000002341e3513c0] lavfi.signalstats.YMIN=16\n\
            [Parsed_metadata_4 @ 000002341e3513c0] lavfi.signalstats.YLOW=16\n\
            [Parsed_metadata_4 @ 000002341e3513c0] lavfi.signalstats.YHIGH=16\n\
            [Parsed_metadata_4 @ 000002341e3513c0] lavfi.signalstats.YMAX=194\n\
            [Parsed_metadata_4 @ 000002341e3513c0] lavfi.signalstats.SATAVG=0.119403\n\
            [Parsed_metadata_4 @ 000002341e3513c0] frame:1    pts:4096    pts_time:4.096\n\
            [Parsed_metadata_4 @ 000002341e3513c0] lavfi.blackframe.pblack=8\n";
        let frames = parse_frame_stats(log, 3_220_000);
        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames[0],
            FrameStats { ms: 3_220_259, black: 95.0, ylow: 16.0, yhigh: 16.0, ymax: 194.0, sat: 0.119403 }
        );
        assert_eq!((frames[1].ms, frames[1].black), (3_224_096, 8.0));
    }

    #[test]
    fn parses_little_endian_values() {
        assert_eq!(parse_raw(&[1, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 9]), vec![1, u32::MAX]);
        assert!((ITEM_SECONDS - 0.1238).abs() < 0.0001);
    }
}
