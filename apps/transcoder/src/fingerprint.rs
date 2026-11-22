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

use pley_transcoder_api::{Cue, CuesSource, Fingerprint, FingerprintRequest, FrameStats, Frames, FramesRequest};
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
        // An embedded picture track is read with the key frames, in the same
        // pass over the file: its packets' times and sizes, as `framecrc`.
        let picture = req.subtitles.as_ref().filter(|s| !s.text).and_then(|s| s.stream);
        let command = |with_picture: bool| {
            let mut cmd = self.ff.command();
            // The measurements come through the log.
            cmd.args(["-nostats", "-loglevel", "info", "-skip_frame", "nokey"]);
            if start_ms > 0 {
                cmd.args(["-ss", &secs(start_ms)]);
            }
            cmd.args(["-t", &secs(duration_ms)]).args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(path));
            cmd.args(["-map", "0:V:0", "-an", "-sn", "-dn", "-vf", FRAME_STATS, "-f", "null", "-"]);
            if let (true, Some(stream)) = (with_picture, picture) {
                cmd.args(["-map", &format!("0:{stream}"), "-c", "copy", "-f", "framecrc", "pipe:1"]);
            }
            background_priority(&mut cmd);
            cmd
        };

        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;
        let (out, log, read_picture) = match self.background.run(command(picture.is_some()), TIMEOUT).await {
            Ok((out, log)) => (out, log, picture.is_some()),
            // The subtitles in the way: the key frames alone. A pass that ran
            // out of time isn't run again.
            Err(err) if picture.is_some() && !matches!(err, ApiError::Timeout(_)) => {
                tracing::debug!("key frames of {} without their subtitles: {err:?}", req.path);
                let (out, log) = self.background.run(command(false), TIMEOUT).await?;
                (out, log, false)
            }
            Err(err) => return Err(err),
        };
        let frames = parse_frame_stats(&log, start_ms);
        // Subtitles that can't be read leave the cues unknown, not empty.
        let cues = match (&req.subtitles, picture) {
            (Some(_), Some(_)) => {
                read_picture.then(|| picture_cues(&parse_framecrc(&String::from_utf8_lossy(&out), start_ms)))
            }
            (Some(source), None) => match self.text_cues(path, source, start_ms, duration_ms).await {
                Ok(cues) => Some(cues),
                Err(err) => {
                    tracing::debug!("no subtitle cues for {}: {err:?}", req.path);
                    None
                }
            },
            (None, _) => None,
        };
        Ok(Frames { frames, cues: cues.map(bounded) })
    }

    /// The lines of an embedded text track over part of a file. `-copyts`
    /// keeps their times as they are in the file: seeking otherwise shifts
    /// them by the lines between the key frame before the start and the
    /// start. Without a length FFmpeg reads only the subtitle packets to the
    /// end of the file, many times faster than with one.
    async fn text_cues(
        &self,
        video: &Path,
        source: &CuesSource,
        start_ms: i64,
        duration_ms: i64,
    ) -> ApiResult<Vec<Cue>> {
        let Some(stream) = source.stream.filter(|_| source.text) else {
            return Err(ApiError::BadRequest("no text subtitles to read".into()));
        };
        let end_ms = start_ms + duration_ms;
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error", "-copyts", "-ss", &secs(start_ms)])
            .args(MEDIA_INPUT_OPTIONS)
            .arg("-i")
            .arg(input_arg(video))
            .args(["-map", &format!("0:{stream}"), "-f", "srt", "-"]);
        background_priority(&mut cmd);
        let (out, _) = self.background.run(cmd, CUES_TIMEOUT).await?;
        let cues = parse_srt(&String::from_utf8_lossy(&out), 0);
        Ok(cues.into_iter().filter(|c| c.end_ms > start_ms && c.start_ms < end_ms).collect())
    }
}

/// Cues kept, and the length of their text: a track can't make the reply or
/// the server's cache grow without end.
const MAX_CUES: usize = 4_000;
const MAX_CUE_TEXT: usize = 200;
/// Reading a text track's lines takes seconds; a stuck read is given up.
const CUES_TIMEOUT: Duration = Duration::from_secs(2 * 60);

fn bounded(mut cues: Vec<Cue>) -> Vec<Cue> {
    cues.truncate(MAX_CUES);
    for cue in &mut cues {
        if let Some((cut, _)) = cue.text.char_indices().nth(MAX_CUE_TEXT) {
            cue.text.truncate(cut);
        }
    }
    cues
}

/// The cues of SRT text, `offset_ms` added to their times. Each block is a
/// number, a `00:00:01,000 --> 00:00:02,500` line and the text.
fn parse_srt(srt: &str, offset_ms: i64) -> Vec<Cue> {
    let time = |s: &str| -> Option<i64> {
        let (hms, ms) = s.trim().split_once([',', '.'])?;
        let mut parts = hms.split(':').map(|p| p.trim().parse::<i64>());
        let (h, m, s) = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
        Some(((h * 60 + m) * 60 + s) * 1000 + ms.trim().get(..3.min(ms.trim().len()))?.parse::<i64>().ok()?)
    };
    let mut cues = Vec::new();
    for block in srt.replace('\r', "").split("\n\n") {
        let mut lines = block.lines().skip_while(|l| !l.contains("-->"));
        let Some((from, to)) = lines.next().and_then(|l| l.split_once("-->")) else { continue };
        let (Some(start), Some(end)) = (time(from), time(to.split_whitespace().next().unwrap_or(""))) else { continue };
        let text = lines.collect::<Vec<_>>().join("\n");
        cues.push(Cue { start_ms: start + offset_ms, end_ms: end + offset_ms, text });
    }
    cues
}

/// `(ms, size)` of each packet of a `framecrc` listing of one stream, its
/// `#tb` time base turned into ms and `offset_ms` added.
fn parse_framecrc(listing: &str, offset_ms: i64) -> Vec<(i64, usize)> {
    let mut base = (1_i64, 1_000_i64);
    let mut packets = Vec::new();
    for line in listing.lines().map(str::trim) {
        if let Some(tb) = line.strip_prefix("#tb 0:") {
            if let Some((num, den)) = tb.trim().split_once('/')
                && let (Ok(num), Ok(den)) = (num.parse::<i64>(), den.parse::<i64>())
                && den > 0
            {
                base = (num, den);
            }
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if let [_, _, pts, _, size, ..] = fields[..]
            && let (Ok(pts), Ok(size)) = (pts.parse::<i64>(), size.parse::<usize>())
        {
            packets.push((offset_ms + pts * base.0 * 1_000 / base.1, size));
        }
    }
    packets
}

/// When picture subtitles show, from their packets: a picture (more than
/// [`PICTURE_PACKET`] bytes, the largest packet at its time: a picture can
/// come in several segments) shows until the next time a packet comes, often
/// the small one that clears it.
fn picture_cues(packets: &[(i64, usize)]) -> Vec<Cue> {
    let mut times: Vec<(i64, usize)> = packets.to_vec();
    times.sort_unstable();
    times.dedup_by(|later, earlier| {
        let same = later.0 == earlier.0;
        if same {
            earlier.1 = earlier.1.max(later.1);
        }
        same
    });
    times
        .iter()
        .enumerate()
        .filter(|(_, (_, size))| *size > PICTURE_PACKET)
        .map(|(i, &(t, _))| Cue {
            start_ms: t,
            end_ms: times.get(i + 1).map_or(t + 3_000, |p| p.0),
            text: String::new(),
        })
        .collect()
}

/// Picture subtitle packets larger than this hold a picture; smaller ones
/// clear the screen.
const PICTURE_PACKET: usize = 200;

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
    fn parses_srt_cues() {
        let srt = "1\r\n00:00:02,562 --> 00:00:04,062\r\nWhere were you?\r\n\r\n\
            2\n00:00:04,097 --> 00:00:06,630 X1:0\n<i>Out.</i>\nLate.\n\n3\nbroken\n\n";
        let cues = parse_srt(srt, 2_529_000);
        assert_eq!(
            cues,
            [
                Cue { start_ms: 2_531_562, end_ms: 2_533_062, text: "Where were you?".into() },
                Cue { start_ms: 2_533_097, end_ms: 2_535_630, text: "<i>Out.</i>\nLate.".into() },
            ]
        );
        assert!(parse_srt("", 0).is_empty());
    }

    #[test]
    fn picture_cues_show_until_the_next_packet() {
        // A picture, the small packet clearing it, a picture at the end.
        let listing = "#tb 0: 1/1000
#media_type 0: subtitle
#codec_id 0: hdmv_pgs_subtitle
            0,          0,          0,        0,     4810, 0x5e3a20c1
            0,       2550,       2550,        0,       30, 0x0aa30c2b
            0,       4754,       4754,        0,     5120, 0x1c0f4e22
";
        let packets = parse_framecrc(listing, 2_526_808);
        assert_eq!(packets, [(2_526_808, 4810), (2_529_358, 30), (2_531_562, 5120)]);
        assert_eq!(
            picture_cues(&packets),
            [
                Cue { start_ms: 2_526_808, end_ms: 2_529_358, text: String::new() },
                Cue { start_ms: 2_531_562, end_ms: 2_534_562, text: String::new() },
            ]
        );
        // A 90 kHz time base.
        assert_eq!(
            parse_framecrc(
                "#tb 0: 1/90000
0, 900000, 900000, 0, 500, 0x0
",
                0
            ),
            [(10_000, 500)]
        );
    }

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
