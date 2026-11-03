// SPDX-License-Identifier: AGPL-3.0-only
//! `ffprobe` wrapper producing a normalised [`MediaInfo`], including HDR10, HLG
//! and Dolby Vision detection. Results are cached in memory per file version.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use parking_lot::Mutex;
use pley_transcoder_api::{
    Chapter, DolbyVisionLayer, DynamicRange, MediaInfo, MediaStream, StreamKind, is_text_subtitle_codec,
};
use serde::Deserialize;
use tokio::sync::Semaphore;

use crate::ffmpeg::Ffmpeg;
use crate::metrics::Metrics;
use crate::plan::MAX_DURATION_MS;
use crate::util::{MEDIA_INPUT_OPTIONS, file_key, input_arg};

const CACHE_ENTRIES: usize = 4096;
/// Packets read from the start of an MP4 or MOV file with its streams, for
/// the times they carry (see [`MediaStream::decode_order_times`]).
const TIMED_PACKETS: &str = "%+#128";
/// Fewest packets of a stream that tell anything about its times.
const MIN_TIMED_PACKETS: usize = 8;
/// Packets of a suspect stream decoded to see the order its frames come out in.
const ORDERED_PACKETS: &str = "%+#48";
/// What ffprobe shows of the first frames' side data: each kind's type, and
/// of Dolby Vision's metadata what tells its profile
/// ([`dolby_vision_profile_in_frames`]) and what profile 7's enhancement
/// layer holds ([`dolby_vision_layer_in_frames`]), not its curves.
const FRAME_ENTRIES: &str = "frame=key_frame:frame_side_data=side_data_type,vdr_rpu_profile,vdr_bit_depth,\
    bl_video_full_range_flag,el_spatial_resampling_filter_flag,disable_residual_flag,coef_data_type,coef_log2_denom:\
    frame_side_data_component=nlq_offset,vdr_in_max,linear_deadzone_slope,linear_deadzone_threshold";

pub struct Prober {
    ff: Arc<Ffmpeg>,
    metrics: Arc<Metrics>,
    limiter: Semaphore,
    cache: Mutex<ProbeCache>,
}

#[derive(Default)]
struct ProbeCache {
    map: HashMap<String, Arc<MediaInfo>>,
    order: VecDeque<String>,
}

impl Prober {
    pub fn new(ff: Arc<Ffmpeg>, metrics: Arc<Metrics>, concurrency: usize) -> Self {
        Self { ff, metrics, limiter: Semaphore::new(concurrency.max(1)), cache: Mutex::default() }
    }

    pub async fn probe(&self, path: &Path) -> anyhow::Result<Arc<MediaInfo>> {
        let key = file_key(path).with_context(|| format!("cannot access {}", path.display()))?;
        if let Some(hit) = self.cache.lock().map.get(&key) {
            return Ok(hit.clone());
        }
        let _permit = self.limiter.acquire().await?;
        let started = Instant::now();
        Metrics::inc(&self.metrics.probes);
        let result = self.run_ffprobe(path).await;
        self.metrics
            .probe_ms_total
            .fetch_add(started.elapsed().as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
        match result {
            Ok(info) => {
                let info = Arc::new(info);
                let mut cache = self.cache.lock();
                if cache.map.insert(key.clone(), info.clone()).is_none() {
                    cache.order.push_back(key);
                    while cache.order.len() > CACHE_ENTRIES {
                        if let Some(old) = cache.order.pop_front() {
                            cache.map.remove(&old);
                        }
                    }
                }
                Ok(info)
            }
            Err(err) => {
                Metrics::inc(&self.metrics.probe_failures);
                Err(err)
            }
        }
    }

    /// Takes `info` for what probing `path` says, as if ffprobe had (tests
    /// run without it).
    #[cfg(test)]
    pub fn remember(&self, path: &Path, info: MediaInfo) {
        let key = file_key(path).expect("the file exists");
        let mut cache = self.cache.lock();
        cache.map.insert(key.clone(), Arc::new(info));
        cache.order.push_back(key);
    }

    async fn run_ffprobe(&self, path: &Path) -> anyhow::Result<MediaInfo> {
        if !self.ff.caps.ffprobe_available {
            bail!("ffprobe is not available");
        }
        let mut cmd = self.ff.probe_command();
        cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", "-show_chapters"]);
        // Other containers time frames as they are shown (Matroska) or both
        // ways (MPEG-TS): only MP4 and MOV files have their packets read.
        if crate::mp4::sniff(&head(path).await) {
            cmd.args(["-show_entries", "packet=stream_index,pts,dts", "-read_intervals", TIMED_PACKETS]);
        }
        cmd.args(MEDIA_INPUT_OPTIONS)
            .arg(input_arg(path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn().context("spawning ffprobe")?;
        let output = tokio::time::timeout(Duration::from_secs(120), child.wait_with_output())
            .await
            .context("ffprobe timed out")??;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Rejected(format!("ffprobe failed: {}", stderr.trim())).into());
        }
        let mut info = parse_ffprobe_json(&output.stdout, path).map_err(|e| Rejected(format!("{e:#}")))?;
        // The packets' times only point at it: the frames, decoded, tell.
        for stream in info.streams.iter_mut().filter(|s| s.decode_order_times) {
            stream.decode_order_times = self.frames_out_of_order(path, stream.index).await;
            if stream.decode_order_times {
                tracing::info!(
                    "{}: the frames of stream {} carry their decode times, not the times they're shown",
                    path.display(),
                    stream.index
                );
            }
        }
        self.read_first_frames(path, &mut info).await;
        Ok(info)
    }

    /// What only a video's frames say, so the first few are decoded (a
    /// fraction of a second): whether the video carries closed captions
    /// (CEA-608/708) in its frames, whether 10-bit HEVC the container
    /// doesn't name Dolby Vision carries it all the same (RPUs in its
    /// frames, see [`MediaStream::dv_unnamed`]), whether a Dolby Vision or
    /// PQ stream carries HDR10+ (a PQ one is then HDR10+, not HDR10), and
    /// what a profile 7 stream's enhancement layer holds. Left unknown when
    /// that fails.
    async fn read_first_frames(&self, path: &Path, info: &mut MediaInfo) {
        let Some(video) = info.streams.iter_mut().find(|s| s.kind == StreamKind::Video) else {
            return;
        };
        // An ffprobe before 7.1 says whether there are captions itself.
        if !hdr_wanted(video) && video.closed_captions.is_some() {
            return;
        }
        let mut cmd = self.ff.probe_command();
        cmd.args(["-v", "error", "-select_streams"])
            .arg(video.index.to_string())
            .args(["-read_intervals", "%+#12", "-show_entries", FRAME_ENTRIES])
            .args(["-of", "json"])
            .args(MEDIA_INPUT_OPTIONS)
            .arg(input_arg(path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let Ok(child) = cmd.spawn() else { return };
        let output = match tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await {
            Ok(Ok(output)) if output.status.success() => output.stdout,
            _ => {
                tracing::debug!("{}: the first frames were not read", path.display());
                return;
            }
        };
        take_first_frames(video, &output);
        if video.dv_unnamed {
            tracing::info!(
                "{}: Dolby Vision profile {} found in the frames of a file that doesn't name it",
                path.display(),
                video.dv_profile.unwrap_or_default()
            );
        }
    }

    /// Whether the first frames of `stream`, decoded, come out with times
    /// going back (see [`MediaStream::decode_order_times`]). Unknown when
    /// ffprobe can't tell: no.
    async fn frames_out_of_order(&self, path: &Path, stream: i64) -> bool {
        let mut cmd = self.ff.probe_command();
        cmd.args(["-v", "error", "-select_streams"])
            .arg(stream.to_string())
            .args(["-read_intervals", ORDERED_PACKETS, "-show_entries", "frame=pts", "-of", "csv=p=0"])
            .args(MEDIA_INPUT_OPTIONS)
            .arg(input_arg(path))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let output = match cmd.spawn() {
            Ok(child) => tokio::time::timeout(Duration::from_secs(60), child.wait_with_output()).await,
            Err(err) => {
                tracing::debug!("{}: ffprobe for the frame order did not start: {err}", path.display());
                return false;
            }
        };
        let Ok(Ok(output)) = output else {
            tracing::debug!("{}: ffprobe for the frame order failed or timed out", path.display());
            return false;
        };
        let times: Vec<i64> =
            String::from_utf8_lossy(&output.stdout).lines().filter_map(|l| l.trim().parse().ok()).collect();
        steps_back(&times) > 1
    }
}

/// How many times are earlier than the one before them.
fn steps_back(times: &[i64]) -> usize {
    times.windows(2).filter(|w| w[1] < w[0]).count()
}

/// The first bytes of `path`, to tell its format by (none when unreadable).
async fn head(path: &Path) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut head = Vec::with_capacity(12);
    if let Ok(file) = tokio::fs::File::open(path).await {
        let _ = file.take(12).read_to_end(&mut head).await;
    }
    head
}

/// Whether `video` may carry Dolby Vision its file doesn't name: 10-bit
/// HEVC (profiles 4, 5, 7 and 8) not named Dolby Vision.
fn unnamed_dolby_vision_wanted(video: &MediaStream) -> bool {
    video.dynamic_range() != DynamicRange::DolbyVision
        && video.codec == "hevc"
        && video.bit_depth.is_none_or(|bits| bits >= 10)
}

/// Whether the first frames of `video` are read for its dynamic range:
/// Dolby Vision its file doesn't name, HDR10+, or what profile 7's
/// enhancement layer holds, not known yet.
fn hdr_wanted(video: &MediaStream) -> bool {
    unnamed_dolby_vision_wanted(video)
        || match video.dynamic_range() {
            DynamicRange::DolbyVision => {
                video.hdr10plus.is_none() || (video.dv_profile == Some(7) && video.dv_el.is_none())
            }
            DynamicRange::Hdr10 => video.hdr10plus.is_none(),
            _ => false,
        }
}

/// Takes into `video` what its first frames, as ffprobe shows them
/// ([`FRAME_ENTRIES`]), say ([`Prober::read_first_frames`]). Dolby Vision
/// found in the frames of a stream the file doesn't name so makes it Dolby
/// Vision, its base layer's HDR10+ kept.
fn take_first_frames(video: &mut MediaStream, frames: &[u8]) {
    if video.closed_captions.is_none() {
        video.closed_captions = captions_in_frames(frames);
    }
    if !hdr_wanted(video) {
        return;
    }
    let mut range = video.dynamic_range();
    let profile = dolby_vision_profile_in_frames(frames).filter(|_| unnamed_dolby_vision_wanted(video));
    if let Some(profile) = profile {
        video.dv_profile = Some(profile);
        video.dv_bl_compat = Some(unnamed_compat(profile, range));
        video.dv_unnamed = true;
        range = DynamicRange::DolbyVision;
        video.dynamic_range = Some(range);
        video.hdr = Some(true);
    }
    if video.hdr10plus.is_none() && matches!(range, DynamicRange::DolbyVision | DynamicRange::Hdr10) {
        video.hdr10plus = hdr10plus_in_frames(frames);
    }
    if range == DynamicRange::Hdr10 && video.hdr10plus == Some(true) {
        video.dynamic_range = Some(DynamicRange::Hdr10plus);
    }
    if range == DynamicRange::DolbyVision && video.dv_profile == Some(7) && video.dv_el.is_none() {
        video.dv_el = dolby_vision_layer_in_frames(frames);
    }
}

/// Whether frames ffprobe showed (`-show_entries frame_side_data`) carry
/// HDR10+ metadata; `None` without frames.
pub fn hdr10plus_in_frames(json: &[u8]) -> Option<bool> {
    side_data_in_frames(json, "2094-40")
}

/// Whether frames ffprobe showed carry closed captions ("ATSC A53 Part 4
/// Closed Captions", CEA-608/708); `None` without frames.
pub fn captions_in_frames(json: &[u8]) -> Option<bool> {
    side_data_in_frames(json, "A53")
}

/// Whether any frame ffprobe showed has side data whose type names `kind`;
/// `None` without frames.
fn side_data_in_frames(json: &[u8], kind: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    let frames = value.get("frames")?.as_array().filter(|f| !f.is_empty())?;
    Some(frames.iter().any(|frame| {
        frame.get("side_data_list").and_then(|l| l.as_array()).is_some_and(|list| {
            list.iter().any(|sd| sd.get("side_data_type").and_then(|t| t.as_str()).is_some_and(|t| t.contains(kind)))
        })
    }))
}

/// The Dolby Vision profile the RPUs of frames ffprobe showed point to, as
/// FFmpeg (libavcodec/dovi_rpudec.c) and `dovi_tool` guess it when the
/// container doesn't say: 5 for full-range RPUs of RPU profile 0; for RPU
/// profile 1, 7 (12-bit) or 4 with a residual from a resampled
/// enhancement layer, 8 without. `None` without Dolby Vision metadata (the
/// decoder exports it only from RPUs it could read), or for a profile it
/// can't tell.
pub fn dolby_vision_profile_in_frames(json: &[u8]) -> Option<i64> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    value
        .get("frames")?
        .as_array()?
        .iter()
        .flat_map(|frame| frame.get("side_data_list").and_then(|l| l.as_array()).into_iter().flatten())
        .filter(|sd| sd.get("side_data_type").and_then(|t| t.as_str()) == Some("Dolby Vision Metadata"))
        .find_map(|metadata| {
            let int = |key: &str| metadata.get(key).and_then(|n| n.as_i64());
            match int("vdr_rpu_profile")? {
                0 if int("bl_video_full_range_flag") == Some(1) => Some(5),
                1 if int("el_spatial_resampling_filter_flag") == Some(1) && int("disable_residual_flag") != Some(1) => {
                    Some(if int("vdr_bit_depth") == Some(12) { 7 } else { 4 })
                }
                1 => Some(8),
                _ => None,
            }
        })
}

/// The base layer's signal compatibility id of Dolby Vision the file
/// doesn't name, from its profile and what its transfer makes it alone
/// (`base`): none for profile 5, Blu-ray HDR10 for 7, else HDR10 for PQ,
/// HLG for HLG and SDR for the rest.
fn unnamed_compat(profile: i64, base: DynamicRange) -> i64 {
    match (profile, base) {
        (5, _) => 0,
        (7, _) => 6,
        (_, DynamicRange::Hdr10 | DynamicRange::Hdr10plus) => 1,
        (_, DynamicRange::Hlg) => 4,
        _ => 2,
    }
}

/// What a profile 7 stream's enhancement layer holds, from the Dolby
/// Vision metadata of frames ffprobe showed (`-show_entries
/// frame_side_data`): minimal (MEL) when its residual is zero in every
/// component (no offset, an input maximum of one, no dead zone), as
/// `dovi_tool` tells them apart, else full (FEL). `None` without that
/// metadata.
pub fn dolby_vision_layer_in_frames(json: &[u8]) -> Option<DolbyVisionLayer> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    let mut found = None;
    for frame in value.get("frames")?.as_array()? {
        let metadata = frame.get("side_data_list").and_then(|l| l.as_array()).into_iter().flatten().filter(|sd| {
            sd.get("side_data_type").and_then(|t| t.as_str()).is_some_and(|t| t == "Dolby Vision Metadata")
        });
        for sd in metadata {
            match enhancement_layer(sd) {
                Some(DolbyVisionLayer::Fel) => return Some(DolbyVisionLayer::Fel),
                layer => found = found.or(layer),
            }
        }
    }
    found
}

fn enhancement_layer(metadata: &serde_json::Value) -> Option<DolbyVisionLayer> {
    let int = |v: &serde_json::Value, key: &str| v.get(key).and_then(|n| n.as_i64());
    // No residual at all: the layer adds nothing to the picture.
    if int(metadata, "disable_residual_flag") == Some(1) {
        return Some(DolbyVisionLayer::Mel);
    }
    // One, in the coefficients' fixed point (or a 32-bit float's bits).
    let one = match int(metadata, "coef_data_type") {
        Some(0) => 1i64.checked_shl(u32::try_from(int(metadata, "coef_log2_denom")?).ok()?)?,
        _ => i64::from(1.0f32.to_bits()),
    };
    let residual: Vec<&serde_json::Value> =
        metadata.get("components")?.as_array()?.iter().filter(|c| c.get("nlq_offset").is_some()).collect();
    if residual.is_empty() {
        return None;
    }
    let minimal = residual.iter().all(|c| {
        int(c, "nlq_offset") == Some(0)
            && int(c, "vdr_in_max") == Some(one)
            && int(c, "linear_deadzone_slope").unwrap_or(0) == 0
            && int(c, "linear_deadzone_threshold").unwrap_or(0) == 0
    });
    Some(if minimal { DolbyVisionLayer::Mel } else { DolbyVisionLayer::Fel })
}

/// ffprobe ran and could not read the file, as opposed to a timeout or a
/// problem starting it. Only this kind of failure is worth remembering.
#[derive(Debug)]
pub struct Rejected(pub String);

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Rejected {}

#[derive(Deserialize)]
struct RawOutput {
    #[serde(default)]
    streams: Vec<RawStream>,
    format: Option<RawFormat>,
    #[serde(default)]
    chapters: Vec<RawChapter>,
    #[serde(default)]
    packets: Vec<RawPacket>,
}

/// A packet from the start of the file. Times are `"N/A"` when unknown.
#[derive(Deserialize)]
struct RawPacket {
    stream_index: i64,
    pts: Option<serde_json::Value>,
    dts: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RawFormat {
    format_name: Option<String>,
    duration: Option<String>,
    start_time: Option<String>,
    bit_rate: Option<String>,
    size: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawChapter {
    start_time: Option<String>,
    end_time: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawStream {
    index: i64,
    codec_type: Option<String>,
    codec_name: Option<String>,
    profile: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    pix_fmt: Option<String>,
    level: Option<i64>,
    field_order: Option<String>,
    has_b_frames: Option<i64>,
    closed_captions: Option<i64>,
    bits_per_raw_sample: Option<String>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    color_space: Option<String>,
    channels: Option<i64>,
    channel_layout: Option<String>,
    sample_rate: Option<String>,
    bit_rate: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    disposition: HashMap<String, i64>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    side_data_list: Vec<serde_json::Value>,
}

/// The rotation of a stream's display matrix, as ffprobe gives it, to the
/// nearest degree; none when it doesn't turn the picture.
fn display_rotation(side_data: &[serde_json::Value]) -> Option<i64> {
    side_data
        .iter()
        .filter(|sd| sd.get("side_data_type").and_then(|t| t.as_str()) == Some("Display Matrix"))
        .find_map(|sd| sd.get("rotation").and_then(|r| r.as_f64()))
        .map(|r| r.round() as i64)
        .filter(|r| r.rem_euclid(360) != 0)
}

pub fn parse_ffprobe_json(json: &[u8], path: &Path) -> anyhow::Result<MediaInfo> {
    let raw: RawOutput = serde_json::from_slice(json).context("parsing ffprobe output")?;
    let format = raw.format.as_ref();
    let format_name = format.and_then(|f| f.format_name.clone());

    let mut has_cover_art = false;
    let mut streams = Vec::new();
    let mut stream_durations = Vec::new();
    for s in &raw.streams {
        let kind = match s.codec_type.as_deref() {
            Some("video") => StreamKind::Video,
            Some("audio") => StreamKind::Audio,
            Some("subtitle") => StreamKind::Subtitle,
            _ => continue,
        };
        let disposition = |k: &str| s.disposition.get(k).copied().unwrap_or(0) == 1;
        if kind == StreamKind::Video && (disposition("attached_pic") || is_image_codec(s.codec_name.as_deref())) {
            has_cover_art = true;
            continue;
        }
        let tags = lowercase_tags(&s.tags);
        let codec = s.codec_name.clone().unwrap_or_else(|| "unknown".into());
        // Matroska tracks carry theirs as a DURATION tag (mkvmerge, FFmpeg).
        let duration = s
            .duration
            .as_deref()
            .and_then(parse_secs_ms)
            .or_else(|| tags.get("duration").and_then(|d| parse_clock_ms(d)));
        stream_durations.extend(duration);
        let mut stream = MediaStream {
            index: s.index,
            kind,
            language: tags.get("language").filter(|l| !l.is_empty() && l.as_str() != "und").cloned(),
            title: tags.get("title").filter(|t| !t.is_empty()).cloned(),
            default: disposition("default"),
            forced: disposition("forced"),
            bitrate: s
                .bit_rate
                .as_deref()
                .and_then(|b| b.parse().ok())
                .or_else(|| tags.get("bps").and_then(|b| b.parse().ok())),
            profile: s.profile.clone().filter(|p| !p.is_empty() && p != "unknown"),
            codec: codec.clone(),
            ..Default::default()
        };
        match kind {
            StreamKind::Video => {
                stream.width = s.width;
                stream.height = s.height;
                stream.frame_rate =
                    parse_rate(s.avg_frame_rate.as_deref()).or_else(|| parse_rate(s.r_frame_rate.as_deref()));
                stream.pix_fmt = s.pix_fmt.clone();
                stream.bit_depth = bit_depth(s.pix_fmt.as_deref(), s.bits_per_raw_sample.as_deref());
                stream.level = s.level.filter(|l| *l > 0);
                stream.interlaced = s.field_order.as_deref().map(|f| !matches!(f, "progressive" | "unknown"));
                stream.rotation = display_rotation(&s.side_data_list);
                stream.has_b_frames = s.has_b_frames.map(|n| n > 0);
                stream.closed_captions = s.closed_captions.map(|n| n > 0);
                // Suspected only: the prober decodes a few frames to know.
                stream.decode_order_times =
                    stream.has_b_frames == Some(true) && decode_times_only(&raw.packets, s.index);
                stream.color_transfer = s.color_transfer.clone();
                stream.color_primaries = s.color_primaries.clone();
                stream.color_space = s.color_space.clone();
                let dovi = s.side_data_list.iter().find(|sd| {
                    sd.get("side_data_type").and_then(|t| t.as_str()).is_some_and(|t| t.contains("DOVI configuration"))
                });
                let hdr10plus = s
                    .side_data_list
                    .iter()
                    .any(|sd| sd.get("side_data_type").and_then(|t| t.as_str()).is_some_and(|t| t.contains("2094-40")));
                let range = if let Some(dovi) = dovi {
                    let get = |k: &str| dovi.get(k).and_then(|v| v.as_i64());
                    stream.dv_profile = get("dv_profile");
                    stream.dv_level = get("dv_level");
                    stream.dv_bl_compat = get("dv_bl_signal_compatibility_id");
                    // Usually only frames say (see `Prober::find_hdr10plus`).
                    stream.hdr10plus = hdr10plus.then_some(true);
                    DynamicRange::DolbyVision
                } else {
                    match s.color_transfer.as_deref() {
                        Some("smpte2084") => {
                            if hdr10plus {
                                stream.hdr10plus = Some(true);
                                DynamicRange::Hdr10plus
                            } else {
                                DynamicRange::Hdr10
                            }
                        }
                        Some("arib-std-b67") => DynamicRange::Hlg,
                        // Untagged transfer but 10+ bit BT.2020: in practice this is PQ (HDR10).
                        None | Some("unknown") | Some("unspecified")
                            if stream.bit_depth.unwrap_or(8) >= 10
                                && (s.color_primaries.as_deref() == Some("bt2020")
                                    || s.color_space.as_deref().is_some_and(|c| c.starts_with("bt2020"))) =>
                        {
                            DynamicRange::Hdr10
                        }
                        _ => DynamicRange::Sdr,
                    }
                };
                stream.dynamic_range = Some(range);
                stream.hdr = Some(range.is_hdr());
            }
            StreamKind::Audio => {
                stream.channels = s.channels;
                stream.channel_layout = s.channel_layout.clone();
                stream.sample_rate = s.sample_rate.as_deref().and_then(|r| r.parse().ok());
            }
            StreamKind::Subtitle => {
                stream.text_based = Some(is_text_subtitle_codec(&codec));
            }
        }
        streams.push(stream);
    }

    // A damaged or crafted file can claim any length, and jobs are laid out
    // per segment of it: beyond what is plausible, the streams' own durations
    // or the size over the streams' bitrates are believed instead.
    let plausible = |ms: &i64| (1..=MAX_DURATION_MS).contains(ms);
    let format_duration = format.and_then(|f| f.duration.as_deref()).and_then(parse_secs_ms);
    let duration_ms = format_duration
        .filter(plausible)
        .or_else(|| stream_durations.iter().copied().filter(plausible).max())
        .or_else(|| size_over_bitrate(format, &streams).filter(plausible));
    if let Some(claimed) = format_duration.filter(|d| !plausible(d) && *d > 0) {
        tracing::warn!(
            "{}: ignoring an implausible duration of {claimed} ms, using {}",
            path.display(),
            duration_ms.map_or("none".into(), |d| format!("{d} ms"))
        );
    }
    let bitrate = format.and_then(|f| f.bit_rate.as_deref()).and_then(|b| b.parse().ok());
    let tags: BTreeMap<String, String> =
        format.map(|f| lowercase_tags(&f.tags)).unwrap_or_default().into_iter().collect();
    let chapters = raw
        .chapters
        .iter()
        .filter_map(|c| {
            Some(Chapter {
                start_ms: parse_secs_ms(c.start_time.as_deref()?)?,
                end_ms: parse_secs_ms(c.end_time.as_deref()?)?,
                title: c.tags.iter().find(|(k, _)| k.eq_ignore_ascii_case("title")).map(|(_, v)| v.clone()),
            })
        })
        .collect();

    let start_ms = format
        .and_then(|f| f.start_time.as_deref())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && v.abs() * 1000.0 <= MAX_DURATION_MS as f64)
        .map(|v| (v * 1000.0).round() as i64);

    Ok(MediaInfo {
        container: normalize_container(format_name.as_deref(), path, &streams),
        format_name,
        duration_ms,
        start_ms,
        bitrate,
        streams,
        tags,
        chapters,
        has_cover_art,
    })
}

/// Whether every packet of `stream` read is shown at the time it is decoded
/// at, and enough were read to say so. A stream that reorders frames and
/// has them right shows some later than it decodes them.
fn decode_times_only(packets: &[RawPacket], stream: i64) -> bool {
    let time = |t: &Option<serde_json::Value>| t.as_ref().and_then(serde_json::Value::as_i64);
    let mut read = 0;
    for packet in packets.iter().filter(|p| p.stream_index == stream) {
        match (time(&packet.pts), time(&packet.dts)) {
            (Some(pts), Some(dts)) if pts == dts => read += 1,
            _ => return false,
        }
    }
    read >= MIN_TIMED_PACKETS
}

fn lowercase_tags(tags: &HashMap<String, String>) -> HashMap<String, String> {
    tags.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned())).collect()
}

fn is_image_codec(codec: Option<&str>) -> bool {
    matches!(codec, Some("mjpeg" | "png" | "bmp" | "gif" | "webp"))
}

fn parse_secs_ms(value: &str) -> Option<i64> {
    let secs: f64 = value.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as i64)
}

/// `"01:02:03.500000000"` (a Matroska `DURATION` tag) → `Some(3_723_500)`.
fn parse_clock_ms(value: &str) -> Option<i64> {
    let mut parts = value.trim().split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let secs = h.parse::<f64>().ok()? * 3600.0 + m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as i64)
}

/// How long the file lasts at its streams' declared bitrates.
fn size_over_bitrate(format: Option<&RawFormat>, streams: &[MediaStream]) -> Option<i64> {
    let size: i64 = format?.size.as_deref()?.trim().parse().ok()?;
    let bitrate: i128 = streams.iter().filter_map(|s| s.bitrate).filter(|b| *b > 0).map(i128::from).sum();
    if size <= 0 || bitrate <= 0 {
        return None;
    }
    i64::try_from(i128::from(size) * 8_000 / bitrate).ok()
}

fn parse_rate(value: Option<&str>) -> Option<f64> {
    let value = value?;
    let (num, den) = value.split_once('/').unwrap_or((value, "1"));
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    if den <= 0.0 || num <= 0.0 {
        return None;
    }
    let rate = num / den;
    (rate > 0.0 && rate < 1000.0).then(|| (rate * 1000.0).round() / 1000.0)
}

/// Bits per component from the pixel format (`yuv420p10le` → 10, `p010le` → 10,
/// `nv12` → 8), falling back to `bits_per_raw_sample`.
fn bit_depth(pix_fmt: Option<&str>, raw: Option<&str>) -> Option<i64> {
    if let Some(fmt) = pix_fmt {
        let f = fmt.trim_end_matches("le").trim_end_matches("be");
        if f.starts_with("nv") {
            return Some(8);
        }
        if let Some(depth) = f.strip_prefix("p0").and_then(|d| d.parse().ok()) {
            return Some(depth);
        }
        let digits: String = f.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
        let digits: String = digits.chars().rev().collect();
        return Some(digits.parse::<i64>().ok().filter(|d| (9..=16).contains(d)).unwrap_or(8));
    }
    raw.and_then(|r| r.parse().ok())
}

/// Map FFmpeg's demuxer names to the short container names clients understand.
fn normalize_container(format_name: Option<&str>, path: &Path, streams: &[MediaStream]) -> Option<String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let format_name = format_name?;
    let first = format_name.split(',').next().unwrap_or(format_name);
    let container = match first {
        "matroska" => {
            let webm_codecs = streams
                .iter()
                .all(|s| matches!(s.codec.as_str(), "vp8" | "vp9" | "av1" | "opus" | "vorbis" | "webvtt"));
            if ext == "webm" && webm_codecs { "webm" } else { "mkv" }
        }
        "mov" => match ext.as_str() {
            "m4v" | "m4a" | "mov" | "3gp" | "m4b" => leak_ext(&ext),
            _ => "mp4",
        },
        "mpegts" => "ts",
        "mpeg" => "mpeg",
        "asf" => {
            if streams.iter().any(|s| s.kind == StreamKind::Video) {
                "wmv"
            } else {
                "wma"
            }
        }
        "ogg" => "ogg",
        "flac" => "flac",
        "mp3" => "mp3",
        "wav" => "wav",
        "aiff" => "aiff",
        "ape" => "ape",
        "wv" => "wv",
        "avi" => "avi",
        "flv" => "flv",
        "aac" => "aac",
        other => return Some(other.to_owned()),
    };
    Some(container.to_owned())
}

fn leak_ext(ext: &str) -> &'static str {
    match ext {
        "m4v" => "m4v",
        "m4a" => "m4a",
        "mov" => "mov",
        "3gp" => "3gp",
        "m4b" => "m4b",
        _ => "mp4",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "streams": [
        {"index":0,"codec_name":"hevc","codec_type":"video","profile":"Main 10","width":3840,"height":2160,
         "pix_fmt":"yuv420p10le","level":153,"color_transfer":"smpte2084","color_primaries":"bt2020","color_space":"bt2020nc",
         "field_order":"progressive","r_frame_rate":"24000/1001","avg_frame_rate":"24000/1001",
         "disposition":{"default":1,"forced":0},
         "side_data_list":[{"side_data_type":"DOVI configuration record","dv_version_major":1,"dv_version_minor":0,
           "dv_profile":8,"dv_level":6,"rpu_present_flag":1,"el_present_flag":0,"bl_present_flag":1,"dv_bl_signal_compatibility_id":1}],
         "tags":{"language":"und"}},
        {"index":1,"codec_name":"eac3","codec_type":"audio","channels":6,"channel_layout":"5.1(side)","sample_rate":"48000",
         "disposition":{"default":1},"tags":{"language":"eng","title":"Surround 5.1","BPS":"640000"}},
        {"index":2,"codec_name":"subrip","codec_type":"subtitle","disposition":{"default":0,"forced":1},"tags":{"language":"eng"}},
        {"index":3,"codec_name":"hdmv_pgs_subtitle","codec_type":"subtitle","disposition":{"default":0},"tags":{"language":"fre"}},
        {"index":4,"codec_name":"mjpeg","codec_type":"video","disposition":{"attached_pic":1}}
      ],
      "format": {"format_name":"matroska,webm","start_time":"1.400000","duration":"7265.123000","bit_rate":"25000000","tags":{"TITLE":"Movie"}},
      "chapters": [{"start_time":"0.000000","end_time":"300.500000","tags":{"title":"Opening"}}]
    }"#;

    /// A phone's portrait video: landscape frames, turned by the display
    /// matrix (ffprobe 9.0's output for an MP4 so tagged).
    #[test]
    fn reads_how_far_the_picture_is_turned() {
        let json = |rotation: &str| {
            format!(
                r#"{{"streams":[{{"index":0,"codec_name":"h264","codec_type":"video","width":1920,"height":1080,
                "side_data_list":[{{"side_data_type":"Display Matrix","displaymatrix":"\n00000000:            0      -65536           0\n",
                "rotation":{rotation}}}]}}],"format":{{"duration":"10.0"}}}}"#
            )
        };
        let rotation = |r: &str| {
            let info = parse_ffprobe_json(json(r).as_bytes(), Path::new("/m/clip.mp4")).unwrap();
            let video = info.video().unwrap().clone();
            (video.rotation, video.shown_sideways())
        };
        assert_eq!(rotation("90"), (Some(90), true));
        assert_eq!(rotation("-90"), (Some(-90), true));
        assert_eq!(rotation("-90.0"), (Some(-90), true));
        assert_eq!(rotation("180"), (Some(180), false));
        // A matrix that only flips, or turns nothing.
        assert_eq!(rotation("0"), (None, false));
        assert_eq!(rotation("-0"), (None, false));
        let info = parse_ffprobe_json(SAMPLE.as_bytes(), Path::new("/m/movie.mkv")).unwrap();
        assert_eq!(info.video().unwrap().rotation, None);
    }

    /// Captions in the frames (a TV recording's CEA-608), as ffprobe says.
    #[test]
    fn reads_whether_the_video_carries_captions() {
        let captions = |cc: &str| {
            let json = format!(
                r#"{{"streams":[{{"index":0,"codec_name":"h264","codec_type":"video","width":1920,"height":1080{cc}}}],
                "format":{{"duration":"10.0"}}}}"#
            );
            parse_ffprobe_json(json.as_bytes(), Path::new("/m/clip.ts")).unwrap().video().unwrap().closed_captions
        };
        assert_eq!(captions(r#","closed_captions":1"#), Some(true));
        assert_eq!(captions(r#","closed_captions":0"#), Some(false));
        assert_eq!(captions(""), None);
    }

    #[test]
    fn parses_streams_and_dolby_vision() {
        let info = parse_ffprobe_json(SAMPLE.as_bytes(), Path::new("/m/Movie.mkv")).unwrap();
        assert_eq!(info.container.as_deref(), Some("mkv"));
        assert_eq!(info.duration_ms, Some(7_265_123));
        assert_eq!(info.start_ms, Some(1_400));
        assert_eq!(info.bitrate, Some(25_000_000));
        assert!(info.has_cover_art);
        assert_eq!(info.streams.len(), 4);
        assert_eq!(info.tags.get("title").map(String::as_str), Some("Movie"));
        assert_eq!(info.chapters.len(), 1);

        let video = info.video().unwrap();
        assert_eq!(video.dynamic_range, Some(DynamicRange::DolbyVision));
        assert_eq!(video.dv_profile, Some(8));
        assert_eq!(video.dv_bl_compat, Some(1));
        assert_eq!(video.bit_depth, Some(10));
        assert_eq!(video.hdr, Some(true));
        assert_eq!(video.interlaced, Some(false));
        assert!((video.frame_rate.unwrap() - 23.976).abs() < 0.001);
        assert_eq!(video.language, None);

        let audio = info.default_audio().unwrap();
        assert_eq!(audio.channels, Some(6));
        assert_eq!(audio.bitrate, Some(640_000));
        assert_eq!(audio.title.as_deref(), Some("Surround 5.1"));

        let srt = info.stream(2).unwrap();
        assert!(srt.forced);
        assert_eq!(srt.text_based, Some(true));
        let pgs = info.stream(3).unwrap();
        assert_eq!(pgs.text_based, Some(false));
    }

    #[test]
    fn container_names() {
        let s = |codec: &str, kind| MediaStream { codec: codec.into(), kind, ..Default::default() };
        let vp9 = vec![s("vp9", StreamKind::Video), s("opus", StreamKind::Audio)];
        assert_eq!(normalize_container(Some("matroska,webm"), Path::new("a.webm"), &vp9).as_deref(), Some("webm"));
        assert_eq!(normalize_container(Some("matroska,webm"), Path::new("a.mkv"), &vp9).as_deref(), Some("mkv"));
        assert_eq!(
            normalize_container(Some("mov,mp4,m4a,3gp,3g2,mj2"), Path::new("a.m4v"), &[]).as_deref(),
            Some("m4v")
        );
        assert_eq!(
            normalize_container(Some("mov,mp4,m4a,3gp,3g2,mj2"), Path::new("a.MP4"), &[]).as_deref(),
            Some("mp4")
        );
        assert_eq!(normalize_container(Some("mpegts"), Path::new("a.m2ts"), &[]).as_deref(), Some("ts"));
    }

    #[test]
    fn bit_depths() {
        assert_eq!(bit_depth(Some("yuv420p"), None), Some(8));
        assert_eq!(bit_depth(Some("yuv420p10le"), None), Some(10));
        assert_eq!(bit_depth(Some("yuv444p12be"), None), Some(12));
        assert_eq!(bit_depth(Some("p010le"), None), Some(10));
        assert_eq!(bit_depth(Some("nv12"), None), Some(8));
        assert_eq!(bit_depth(Some("rgb24"), None), Some(8));
        assert_eq!(bit_depth(None, Some("10")), Some(10));
    }

    #[test]
    fn hdr_classification() {
        let json = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"hevc","pix_fmt":"yuv420p10le","color_transfer":"arib-std-b67"}],"format":{"format_name":"mov,mp4","duration":"10"}}"#;
        let info = parse_ffprobe_json(json.as_bytes(), Path::new("a.mp4")).unwrap();
        assert_eq!(info.video().unwrap().dynamic_range, Some(DynamicRange::Hlg));
    }

    fn video_range(stream: &str) -> Option<DynamicRange> {
        let json = format!(r#"{{"streams":[{stream}],"format":{{"format_name":"matroska","duration":"10"}}}}"#);
        parse_ffprobe_json(json.as_bytes(), Path::new("a.mkv")).unwrap().video().unwrap().dynamic_range
    }

    #[test]
    fn more_dynamic_ranges() {
        let hdr10plus = r#"{"index":0,"codec_type":"video","codec_name":"hevc","pix_fmt":"yuv420p10le","color_transfer":"smpte2084",
            "side_data_list":[{"side_data_type":"HDR Dynamic Metadata SMPTE2094-40 (HDR10+)"}]}"#;
        assert_eq!(video_range(hdr10plus), Some(DynamicRange::Hdr10plus));
        // Untagged transfer, 10-bit BT.2020: PQ in practice.
        let untagged = r#"{"index":0,"codec_type":"video","codec_name":"hevc","pix_fmt":"yuv420p10le","color_primaries":"bt2020"}"#;
        assert_eq!(video_range(untagged), Some(DynamicRange::Hdr10));
        let eight_bit =
            r#"{"index":0,"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p","color_primaries":"bt2020"}"#;
        assert_eq!(video_range(eight_bit), Some(DynamicRange::Sdr));
        let bt709 =
            r#"{"index":0,"codec_type":"video","codec_name":"h264","pix_fmt":"yuv420p","color_transfer":"bt709"}"#;
        assert_eq!(video_range(bt709), Some(DynamicRange::Sdr));
    }

    #[test]
    fn hdr10_plus_shows_in_frames() {
        let frames = |types: &[&str]| {
            let list: Vec<String> = types.iter().map(|t| format!(r#"{{"side_data_type":"{t}"}}"#)).collect();
            format!(r#"{{"frames":[{{"key_frame":1}},{{"key_frame":0,"side_data_list":[{}]}}]}}"#, list.join(","))
        };
        let hybrid = frames(&["Dolby Vision RPU Data", "HDR Dynamic Metadata SMPTE2094-40 (HDR10+)"]);
        assert_eq!(hdr10plus_in_frames(hybrid.as_bytes()), Some(true));
        assert_eq!(hdr10plus_in_frames(frames(&["Dolby Vision RPU Data"]).as_bytes()), Some(false));
        assert_eq!(hdr10plus_in_frames(br#"{"frames":[]}"#), None, "no frames read: unknown");
        assert_eq!(hdr10plus_in_frames(b"not json"), None);
        // Closed captions in the frames, as the H.264 and MPEG-2 decoders give them.
        let captions = frames(&["ATSC A53 Part 4 Closed Captions"]);
        assert_eq!(captions_in_frames(captions.as_bytes()), Some(true));
        assert_eq!(captions_in_frames(hybrid.as_bytes()), Some(false));
        assert_eq!(captions_in_frames(br#"{"frames":[]}"#), None);
        // Said by the stream itself (rare): no need to look at frames.
        let json = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"hevc","pix_fmt":"yuv420p10le",
            "color_transfer":"smpte2084","side_data_list":[{"side_data_type":"DOVI configuration record","dv_profile":8,
            "dv_level":6,"dv_bl_signal_compatibility_id":1},{"side_data_type":"HDR Dynamic Metadata SMPTE2094-40 (HDR10+)"}]}],
            "format":{"format_name":"matroska","duration":"10"}}"#;
        let info = parse_ffprobe_json(json.as_bytes(), Path::new("a.mkv")).unwrap();
        let video = info.video().unwrap();
        assert_eq!((video.dynamic_range, video.hdr10plus), (Some(DynamicRange::DolbyVision), Some(true)));
        let info = parse_ffprobe_json(SAMPLE.as_bytes(), Path::new("/m/Movie.mkv")).unwrap();
        assert_eq!(info.video().unwrap().hdr10plus, None, "not known before frames are read");
    }

    /// The Dolby Vision metadata ffprobe shows for a profile 7 frame, with
    /// the residual data of each component as given.
    fn profile_7_frame(residual: [(i64, i64, i64, i64); 3]) -> String {
        let components: Vec<String> = residual
            .iter()
            .map(|(offset, max, slope, threshold)| {
                format!(
                    r#"{{"pivots":"0 1023","nlq_offset":{offset},"vdr_in_max":{max},"linear_deadzone_slope":{slope},
                    "linear_deadzone_threshold":{threshold}}}"#
                )
            })
            .collect();
        format!(
            r#"{{"frames":[{{"key_frame":1,"side_data_list":[{{"side_data_type":"Dolby Vision RPU Data"}},
            {{"side_data_type":"Dolby Vision Metadata","rpu_type":2,"coef_data_type":0,"coef_log2_denom":23,
            "disable_residual_flag":0,"nlq_method_idc":0,"components":[{}]}}]}}]}}"#,
            components.join(",")
        )
    }

    #[test]
    fn profile_7_layers_are_told_apart() {
        // What the test clips made with dovi_tool show (`ffprobe -show_entries frame_side_data`).
        let mel = profile_7_frame([(0, 8_388_608, 0, 0); 3]);
        assert_eq!(dolby_vision_layer_in_frames(mel.as_bytes()), Some(DolbyVisionLayer::Mel));
        let fel = profile_7_frame([
            (512, 8_392_704, 16_777_249, 2048),
            (511, 8_392_703, 8_388_640, 2048),
            (510, 8_392_702, 8_388_639, 2048),
        ]);
        assert_eq!(dolby_vision_layer_in_frames(fel.as_bytes()), Some(DolbyVisionLayer::Fel));
        // One component with residual data is enough.
        let partly = profile_7_frame([(0, 8_388_608, 0, 0), (0, 8_388_608, 0, 0), (0, 8_388_608, 0, 2048)]);
        assert_eq!(dolby_vision_layer_in_frames(partly.as_bytes()), Some(DolbyVisionLayer::Fel));
        // Residual turned off: nothing comes from the layer.
        let off = r#"{"frames":[{"side_data_list":[{"side_data_type":"Dolby Vision Metadata","coef_data_type":0,
            "coef_log2_denom":23,"disable_residual_flag":1,"components":[{"pivots":"0 1023"}]}]}]}"#;
        assert_eq!(dolby_vision_layer_in_frames(off.as_bytes()), Some(DolbyVisionLayer::Mel));
        // No Dolby Vision metadata (an RPU FFmpeg didn't read), no frames.
        let none = r#"{"frames":[{"side_data_list":[{"side_data_type":"Dolby Vision RPU Data"}]}]}"#;
        assert_eq!(dolby_vision_layer_in_frames(none.as_bytes()), None);
        assert_eq!(dolby_vision_layer_in_frames(br#"{"frames":[]}"#), None);
        assert_eq!(dolby_vision_layer_in_frames(b"not json"), None);
    }

    /// The first frame of a profile 8.1 stream with HDR10+, as ffprobe shows
    /// it with [`FRAME_ENTRIES`].
    const P81_FRAMES: &str = r#"{"frames":[{"key_frame":1,"side_data_list":[
        {"side_data_type":"Mastering display metadata"},{"side_data_type":"Content light level metadata"},
        {"side_data_type":"HDR Dynamic Metadata SMPTE2094-40 (HDR10+)"},{"side_data_type":"Dolby Vision RPU Data"},
        {"side_data_type":"Dolby Vision Metadata","vdr_rpu_profile":1,"coef_data_type":0,"coef_log2_denom":23,
         "bl_video_full_range_flag":0,"vdr_bit_depth":12,"el_spatial_resampling_filter_flag":0,
         "disable_residual_flag":1,"components":[{},{},{}]}]}]}"#;

    /// The same of a profile 7 stream with a full enhancement layer.
    const P7_FEL_FRAMES: &str = r#"{"frames":[{"key_frame":1,"side_data_list":[
        {"side_data_type":"Mastering display metadata"},{"side_data_type":"Content light level metadata"},
        {"side_data_type":"Dolby Vision RPU Data"},
        {"side_data_type":"Dolby Vision Metadata","vdr_rpu_profile":1,"coef_data_type":0,"coef_log2_denom":23,
         "bl_video_full_range_flag":0,"vdr_bit_depth":12,"el_spatial_resampling_filter_flag":1,
         "disable_residual_flag":0,"components":[
          {"nlq_offset":512,"vdr_in_max":1048576,"linear_deadzone_slope":2048,"linear_deadzone_threshold":0},
          {"nlq_offset":512,"vdr_in_max":1048576,"linear_deadzone_slope":2048,"linear_deadzone_threshold":0},
          {"nlq_offset":512,"vdr_in_max":1048576,"linear_deadzone_slope":2048,"linear_deadzone_threshold":0}]}]}]}"#;

    /// The same of plain HDR10.
    const HDR10_FRAMES: &str = r#"{"frames":[{"key_frame":1,"side_data_list":[
        {"side_data_type":"H.26[45] User Data Unregistered SEI message"},
        {"side_data_type":"Mastering display metadata"},{"side_data_type":"Content light level metadata"}]}]}"#;

    /// Dolby Vision metadata with only the header fields the profile is
    /// told by.
    fn rpu_header(rpu_profile: i64, full_range: i64, el_resampling: i64, no_residual: i64, vdr_bits: i64) -> String {
        format!(
            r#"{{"frames":[{{"side_data_list":[{{"side_data_type":"Dolby Vision RPU Data"}},
            {{"side_data_type":"Dolby Vision Metadata","vdr_rpu_profile":{rpu_profile},
            "bl_video_full_range_flag":{full_range},"el_spatial_resampling_filter_flag":{el_resampling},
            "disable_residual_flag":{no_residual},"vdr_bit_depth":{vdr_bits}}}]}}]}}"#
        )
    }

    #[test]
    fn the_rpus_tell_the_profile() {
        assert_eq!(dolby_vision_profile_in_frames(P81_FRAMES.as_bytes()), Some(8));
        assert_eq!(dolby_vision_profile_in_frames(P7_FEL_FRAMES.as_bytes()), Some(7));
        // FFmpeg's guess (libavcodec/dovi_rpudec.c), as dovi_tool's.
        assert_eq!(dolby_vision_profile_in_frames(rpu_header(0, 1, 0, 1, 12).as_bytes()), Some(5));
        assert_eq!(dolby_vision_profile_in_frames(rpu_header(1, 0, 1, 0, 12).as_bytes()), Some(7));
        assert_eq!(dolby_vision_profile_in_frames(rpu_header(1, 0, 1, 0, 10).as_bytes()), Some(4));
        assert_eq!(dolby_vision_profile_in_frames(rpu_header(1, 0, 0, 0, 12).as_bytes()), Some(8));
        assert_eq!(dolby_vision_profile_in_frames(rpu_header(1, 0, 1, 1, 12).as_bytes()), Some(8), "no residual");
        // Limited-range RPUs of profile 0, and other RPU profiles: unknown.
        assert_eq!(dolby_vision_profile_in_frames(rpu_header(0, 0, 0, 1, 12).as_bytes()), None);
        assert_eq!(dolby_vision_profile_in_frames(rpu_header(2, 0, 0, 1, 12).as_bytes()), None);
        // An RPU FFmpeg couldn't read gives no metadata; no Dolby Vision at all.
        let unread = r#"{"frames":[{"side_data_list":[{"side_data_type":"Dolby Vision RPU Data"}]}]}"#;
        assert_eq!(dolby_vision_profile_in_frames(unread.as_bytes()), None);
        assert_eq!(dolby_vision_profile_in_frames(HDR10_FRAMES.as_bytes()), None);
        assert_eq!(dolby_vision_profile_in_frames(br#"{"frames":[]}"#), None);
        assert_eq!(dolby_vision_profile_in_frames(b"not json"), None);
    }

    /// The video stream ffprobe shows for 10-bit HEVC with `transfer`, its
    /// container naming no Dolby Vision.
    fn hevc(transfer: &str) -> MediaStream {
        let json = format!(
            r#"{{"streams":[{{"index":0,"codec_type":"video","codec_name":"hevc","pix_fmt":"yuv420p10le",
            "width":3840,"height":2160,"avg_frame_rate":"24000/1001","color_transfer":"{transfer}",
            "color_primaries":"bt2020"}}],"format":{{"format_name":"matroska","duration":"10"}}}}"#
        );
        parse_ffprobe_json(json.as_bytes(), Path::new("a.mkv")).unwrap().video().unwrap().clone()
    }

    /// Dolby Vision in the frames of a file that doesn't name it (a remux
    /// without its configuration record): the probe makes it Dolby Vision,
    /// from what the RPUs and the base layer's transfer say.
    #[test]
    fn dolby_vision_the_file_doesnt_name_is_found_in_its_frames() {
        let found = |mut video: MediaStream, frames: &str| {
            take_first_frames(&mut video, frames.as_bytes());
            video
        };
        let p81 = found(hevc("smpte2084"), P81_FRAMES);
        assert_eq!(p81.dynamic_range, Some(DynamicRange::DolbyVision));
        assert_eq!((p81.dv_profile, p81.dv_bl_compat, p81.dv_level), (Some(8), Some(1), None));
        assert!(p81.dv_unnamed && p81.hdr == Some(true));
        // Its HDR10+ is kept: a hybrid, as when the file names it.
        assert_eq!(p81.hdr10plus, Some(true));
        assert_eq!(p81.dynamic_range_label(), "Dolby Vision P8.1 + HDR10+");
        // Named at the level its size and frame rate need.
        assert_eq!(p81.dv_level_named(), Some(6));

        let p7 = found(hevc("smpte2084"), P7_FEL_FRAMES);
        assert_eq!((p7.dv_profile, p7.dv_bl_compat, p7.dv_el), (Some(7), Some(6), Some(DolbyVisionLayer::Fel)));
        assert_eq!((p7.hdr10plus, p7.dynamic_range_label().as_str()), (Some(false), "Dolby Vision P7 FEL"));

        // The base layer's transfer says what it is alone.
        assert_eq!(found(hevc("arib-std-b67"), P81_FRAMES).dolby_vision_label(), "Dolby Vision P8.4");
        assert_eq!(found(hevc("bt709"), P81_FRAMES).dolby_vision_label(), "Dolby Vision P8.2");
        // Without Dolby Vision, HLG isn't looked at for HDR10+.
        assert_eq!(found(hevc("arib-std-b67"), HDR10_FRAMES).hdr10plus, None);
        let p5 = found(hevc("unknown"), &rpu_header(0, 1, 0, 1, 12));
        assert_eq!((p5.dolby_vision_label().as_str(), p5.dv_bl_compat), ("Dolby Vision P5", Some(0)));
        assert!(p5.is_dolby_vision_only());

        // HDR10 and HDR10+ without Dolby Vision stay what they are.
        let hdr10 = found(hevc("smpte2084"), HDR10_FRAMES);
        assert_eq!(
            (hdr10.dynamic_range, hdr10.hdr10plus, hdr10.dv_unnamed),
            (Some(DynamicRange::Hdr10), Some(false), false)
        );
        let hdr10plus =
            r#"{"frames":[{"side_data_list":[{"side_data_type":"HDR Dynamic Metadata SMPTE2094-40 (HDR10+)"}]}]}"#;
        let hdr10plus = found(hevc("smpte2084"), hdr10plus);
        assert_eq!((hdr10plus.dynamic_range, hdr10plus.dv_profile), (Some(DynamicRange::Hdr10plus), None));

        // A file that names its Dolby Vision keeps what it says.
        let named = parse_ffprobe_json(SAMPLE.as_bytes(), Path::new("/m/Movie.mkv")).unwrap().video().unwrap().clone();
        let kept = found(named.clone(), &rpu_header(1, 0, 1, 0, 12));
        assert_eq!(
            (kept.dv_profile, kept.dv_bl_compat, kept.dv_unnamed),
            (named.dv_profile, named.dv_bl_compat, false)
        );

        // 8-bit HEVC and other codecs aren't Dolby Vision's profiles 4 to 8.
        let mut eight_bit = hevc("bt709");
        eight_bit.bit_depth = Some(8);
        assert!(!found(eight_bit, P81_FRAMES).dv_unnamed);
        let mut avc = hevc("smpte2084");
        avc.codec = "h264".into();
        assert!(!found(avc, P81_FRAMES).dv_unnamed);
    }

    #[test]
    fn odd_ffprobe_output() {
        let json = r#"{
          "streams": [
            {"index":0,"codec_type":"video","codec_name":"mpeg2video","field_order":"tt","avg_frame_rate":"0/0","r_frame_rate":"30000/1001","tags":{"LANGUAGE":"und"}},
            {"index":1,"codec_type":"audio","codec_name":"mp2","duration":"95.5","tags":{"language":""}},
            {"index":2,"codec_type":"data","codec_name":"bin_data"},
            {"index":3,"codec_type":"attachment","codec_name":"ttf"},
            {"index":4,"codec_type":"audio","codec_name":"ac3","duration":"120.25","profile":"unknown"}
          ],
          "format": {"format_name":"mpeg","start_time":"N/A"},
          "chapters": [{"start_time":"0.0","end_time":"60.0","tags":{"TITLE":"Part 1"}}, {"start_time":"60.0"}]
        }"#;
        let info = parse_ffprobe_json(json.as_bytes(), Path::new("old.vob")).unwrap();
        assert_eq!(info.container.as_deref(), Some("mpeg"));
        assert_eq!(
            info.streams.iter().map(|s| s.index).collect::<Vec<_>>(),
            [0, 1, 4],
            "data and attachments are not streams"
        );
        let video = info.video().unwrap();
        assert_eq!(video.interlaced, Some(true));
        assert_eq!(video.frame_rate, Some(29.97), "the real frame rate when the average is unknown");
        assert_eq!(video.language, None, "und is no language");
        assert_eq!(info.stream(1).unwrap().language, None);
        assert_eq!(info.stream(4).unwrap().profile, None);
        assert_eq!(info.duration_ms, Some(120_250), "the longest stream without a format duration");
        assert_eq!(info.start_ms, None);
        assert_eq!(info.chapters.len(), 1, "a chapter without an end is dropped");
        assert_eq!(info.chapters[0].title.as_deref(), Some("Part 1"));
        assert!(parse_ffprobe_json(b"not json", Path::new("a.mkv")).is_err());
    }

    #[test]
    fn implausible_durations_fall_back_to_the_streams() {
        let parse = |json: &str| parse_ffprobe_json(json.as_bytes(), Path::new("a.mkv")).unwrap();
        // A Matroska Duration patched to 1e12 s (what ffprobe reports for
        // it): the tracks' DURATION tags tell the truth.
        let crafted = r#"{"streams":[
            {"index":0,"codec_type":"video","codec_name":"h264","tags":{"DURATION":"00:00:04.000000000"}},
            {"index":1,"codec_type":"audio","codec_name":"aac","tags":{"DURATION":"00:00:04.023000000"}}],
          "format":{"format_name":"matroska,webm","duration":"1000000000000.000000","size":"60953","start_time":"1e300"}}"#;
        let info = parse(crafted);
        assert_eq!(info.duration_ms, Some(4_023));
        assert_eq!(info.start_ms, None, "nor is such a start time believed");
        // No stream durations: the size over the streams' bitrates (1 GB at 8 Mbit/s).
        let sized = r#"{"streams":[
            {"index":0,"codec_type":"video","codec_name":"h264","bit_rate":"7000000"},
            {"index":1,"codec_type":"audio","codec_name":"aac","tags":{"BPS":"1000000"}}],
          "format":{"format_name":"matroska,webm","duration":"1e15","size":"1000000000"}}"#;
        assert_eq!(parse(sized).duration_ms, Some(1_000_000));
        // Nothing believable: unknown (jobs refuse the file).
        let hopeless = r#"{"streams":[{"index":0,"codec_type":"video","codec_name":"h264"}],
          "format":{"format_name":"matroska,webm","duration":"1e15"}}"#;
        assert_eq!(parse(hopeless).duration_ms, None);
        // Long but possible: a nine-day recording is believed, and a zero
        // format duration doesn't hide the streams' one.
        let long = r#"{"streams":[],"format":{"format_name":"mpegts","duration":"777600.0"}}"#;
        assert_eq!(parse(long).duration_ms, Some(777_600_000));
        let zero = r#"{"streams":[{"index":0,"codec_type":"audio","codec_name":"mp3","duration":"95.5"}],
          "format":{"format_name":"mp3","duration":"0.000000"}}"#;
        assert_eq!(parse(zero).duration_ms, Some(95_500));
        assert_eq!(parse_clock_ms("01:02:03.500000000"), Some(3_723_500));
        assert_eq!(parse_clock_ms("03.5"), None);
    }

    #[test]
    fn frame_rates_and_windows_media() {
        assert_eq!(parse_rate(Some("25")), Some(25.0));
        assert_eq!(parse_rate(Some("24000/1001")), Some(23.976));
        assert_eq!(parse_rate(Some("0/0")), None);
        assert_eq!(parse_rate(Some("90000/1")), None, "a time base, not a frame rate");
        assert_eq!(parse_rate(Some("N/A")), None);
        assert_eq!(parse_secs_ms("-1.5"), None);
        assert_eq!(parse_secs_ms(" 1.0006 "), Some(1_001));
        let s = |kind| MediaStream { kind, ..Default::default() };
        assert_eq!(
            normalize_container(Some("asf"), Path::new("a.wmv"), &[s(StreamKind::Video)]).as_deref(),
            Some("wmv")
        );
        assert_eq!(
            normalize_container(Some("asf"), Path::new("a.wma"), &[s(StreamKind::Audio)]).as_deref(),
            Some("wma")
        );
        assert_eq!(normalize_container(Some("matroska,webm"), Path::new("a.webm"), &[]).as_deref(), Some("webm"));
        assert_eq!(
            normalize_container(Some("dv"), Path::new("a.dv"), &[]).as_deref(),
            Some("dv"),
            "unknown names pass through"
        );
        assert_eq!(normalize_container(None, Path::new("a.mkv"), &[]), None);
    }

    #[test]
    fn frames_shown_at_their_decode_times_are_suspected() {
        // Video and audio packets in turn; with `shown_later`, every fifth
        // video frame is shown after the four decoded behind it.
        let json = |b_frames: i64, shown_later: bool, packets: i64| {
            let packets: Vec<String> = (0..packets)
                .flat_map(|n| {
                    let dts = n * 1001;
                    let pts = if shown_later && n % 5 == 0 { dts + 4004 } else { dts };
                    [
                        format!(r#"{{"stream_index":0,"pts":{pts},"dts":{dts}}}"#),
                        format!(r#"{{"stream_index":1,"pts":{n},"dts":{n}}}"#),
                    ]
                })
                .collect();
            format!(
                r#"{{"streams":[{{"index":0,"codec_type":"video","codec_name":"hevc","has_b_frames":{b_frames}}},
                    {{"index":1,"codec_type":"audio","codec_name":"aac"}}],
                  "format":{{"format_name":"mov,mp4","duration":"10"}},"packets":[{}]}}"#,
                packets.join(",")
            )
        };
        let suspected = |json: String| {
            parse_ffprobe_json(json.as_bytes(), Path::new("a.mp4")).unwrap().video().unwrap().decode_order_times
        };
        assert!(suspected(json(2, false, 40)), "B-frames, every one shown when it is decoded");
        assert!(!suspected(json(2, true, 40)), "B-frames with times of their own");
        assert!(!suspected(json(0, false, 40)), "no B-frames: decode order is display order");
        assert!(!suspected(json(2, false, 4)), "too few packets to tell");
        let unknown = json(2, false, 40).replacen(r#""pts":0,"dts":0"#, r#""pts":"N/A","dts":0"#, 1);
        assert!(!suspected(unknown), "a packet without a time says nothing");
        assert!(!suspected(SAMPLE.into()), "nothing read, nothing suspected");

        // What decoding such a file gives: frames shown before frames decoded earlier.
        assert_eq!(steps_back(&[50050, 100099, 0, 150149, 300298, 350347, 250248]), 2);
        assert_eq!(steps_back(&[0, 512, 1024, 1536]), 0);
    }
}
