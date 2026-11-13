// SPDX-License-Identifier: AGPL-3.0-only
//! Discovery of the local FFmpeg installation: version, working encoders
//! (test-encodes, not just "compiled in"), tone mappers, throttling support and
//! segment muxer semantics. Results are cached on disk keyed by the binaries'
//! size and modification time, and the GPUs the system shows
//! ([`crate::devices`]), because FFmpeg start-up can be slow (antivirus
//! scanning of large static builds).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use pley_transcoder_api::{
    Capabilities, EncoderInfo, GpuPipeline, HwAccel, TonemapMode, TonemapQuality, TonemapSpeed, Tonemapper, VideoCodec,
};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::Semaphore;

use crate::devices;
use crate::util::{configure_child, resolve_executable};

/// Bump when detection logic changes so stale caches are ignored.
const DETECTION_REVISION: u32 = 27;

pub struct Ffmpeg {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub caps: Capabilities,
    /// `-segment_times` are measured from the first output packet rather than as
    /// absolute timestamps (true for every FFmpeg version tested so far).
    pub segment_times_relative: bool,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    key: String,
    caps: Capabilities,
    segment_times_relative: bool,
}

impl Ffmpeg {
    pub async fn detect(ffmpeg: &Path, ffprobe: &Path, cache_path: &Path, force: bool) -> Self {
        let ffmpeg = resolve_executable(ffmpeg).unwrap_or_else(|| ffmpeg.to_path_buf());
        let ffprobe = resolve_executable(ffprobe).unwrap_or_else(|| ffprobe.to_path_buf());
        let key = cache_key(&ffmpeg, &ffprobe);

        if !force && let Some(cached) = read_cache(cache_path, &key) {
            tracing::info!(
                "ffmpeg {} (cached capabilities): encoders [{}], tone mapping [{}]",
                cached.caps.ffmpeg_version.as_deref().unwrap_or("?"),
                encoder_names(&cached.caps),
                tonemapper_names(&cached.caps),
            );
            return Self { ffmpeg, ffprobe, caps: cached.caps, segment_times_relative: cached.segment_times_relative };
        }

        let started = Instant::now();
        let (caps, relative) = detect_capabilities(&ffmpeg, &ffprobe).await;
        if caps.ffmpeg_available {
            tracing::info!(
                "ffmpeg {} detected in {:.1}s: encoders [{}], tone mapping [{}], dolby vision: {}{}",
                caps.ffmpeg_version.as_deref().unwrap_or("?"),
                started.elapsed().as_secs_f32(),
                encoder_names(&caps),
                tonemapper_names(&caps),
                caps.dolby_vision,
                gpu_summary(&caps),
            );
            let file = CacheFile { key, caps: caps.clone(), segment_times_relative: relative };
            if let Ok(json) = serde_json::to_vec_pretty(&file) {
                let _ = std::fs::write(cache_path, json);
            }
        } else {
            tracing::error!("ffmpeg not found or not working at {}", ffmpeg.display());
        }
        Self { ffmpeg, ffprobe, caps, segment_times_relative: relative }
    }

    /// An ffmpeg command with the common flags applied.
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.ffmpeg);
        configure_child(&mut cmd);
        cmd.arg("-hide_banner").arg("-nostdin");
        cmd
    }

    pub fn probe_command(&self) -> Command {
        let mut cmd = Command::new(&self.ffprobe);
        configure_child(&mut cmd);
        cmd.arg("-hide_banner");
        cmd
    }

    /// The software encoder for `codec`; with `main10`, one that encodes
    /// HEVC Main 10.
    pub fn software_encoder_for(&self, codec: VideoCodec, main10: bool) -> Option<&EncoderInfo> {
        self.caps.encoders.iter().find(|e| e.codec == codec && !e.hardware && (e.main10 || !main10))
    }

    /// Choose an encoder for `codec` honouring the requested acceleration.
    /// Returns the encoder plus a warning when the request could not be met.
    pub fn pick_encoder(&self, codec: VideoCodec, accel: HwAccel) -> (Option<&EncoderInfo>, Option<String>) {
        self.pick(codec, accel, false)
    }

    /// [`Ffmpeg::pick_encoder`] among the HEVC encoders that encode Main 10
    /// (transcodes that keep HDR): a hardware one, else the software one.
    pub fn pick_main10_encoder(&self, accel: HwAccel) -> (Option<&EncoderInfo>, Option<String>) {
        self.pick(VideoCodec::Hevc, accel, true)
    }

    fn pick(&self, codec: VideoCodec, accel: HwAccel, main10: bool) -> (Option<&EncoderInfo>, Option<String>) {
        // Vulkan's encoder makes H.264 only: HEVC is encoded as if nothing was asked.
        let accel = if main10 && accel == HwAccel::Vulkan { HwAccel::Auto } else { accel };
        let hardware = |a: Option<HwAccel>| {
            self.caps
                .encoders
                .iter()
                .find(|e| e.codec == codec && e.hardware && (e.main10 || !main10) && a.is_none_or(|a| e.accel == a))
        };
        let software = || self.software_encoder_for(codec, main10);
        match accel {
            HwAccel::None => (software(), None),
            HwAccel::Auto => (hardware(None).or_else(software), None),
            wanted => match hardware(Some(wanted)) {
                Some(enc) => (Some(enc), None),
                None => (
                    software(),
                    Some(format!("hardware encoder for {wanted:?} is not available, using software encoding")),
                ),
            },
        }
    }

    /// Choose a tone mapper. `dolby_vision_p5` requests a mapper that applies
    /// the Dolby Vision RPU (only libplacebo does). With `vaapi` (a VAAPI
    /// encoder takes the frames) only those whose frames reach it are
    /// chosen ([`Capabilities::vaapi_tonemappers`], and zimg on the CPU).
    pub fn pick_tonemapper(
        &self,
        mode: TonemapMode,
        dolby_vision_p5: bool,
        vaapi: bool,
    ) -> (Option<Tonemapper>, Vec<String>) {
        let reaches_encoder =
            |m: &Tonemapper| !vaapi || *m == Tonemapper::Zscale || self.caps.vaapi_tonemappers.contains(m);
        let available: &Vec<Tonemapper> = &self.caps.tonemappers.iter().copied().filter(reaches_encoder).collect();
        let mut warnings = Vec::new();
        let auto = || {
            if dolby_vision_p5 && available.contains(&Tonemapper::Libplacebo) {
                Some(Tonemapper::Libplacebo)
            } else {
                available.first().copied()
            }
        };
        let choice = match mode {
            TonemapMode::None => {
                warnings.push("tone mapping off".into());
                None
            }
            TonemapMode::Auto => auto(),
            specific => {
                let wanted = match specific {
                    TonemapMode::Libplacebo => Tonemapper::Libplacebo,
                    TonemapMode::Opencl => Tonemapper::Opencl,
                    _ => Tonemapper::Zscale,
                };
                if available.contains(&wanted) {
                    Some(wanted)
                } else if self.caps.tonemappers.contains(&wanted) {
                    warnings
                        .push(format!("tone mapper {} can't hand its frames to the VAAPI encoder", wanted.as_str()));
                    auto()
                } else {
                    warnings.push(format!("tone mapper {} is not available", wanted.as_str()));
                    auto()
                }
            }
        };
        if choice.is_none() && mode != TonemapMode::None {
            warnings.push("no tone mapper available".into());
        }
        // OpenCL with BT.2390 (Pley's FFmpeg) applies Dolby Vision's reshaping too.
        let reshapes =
            |t: Tonemapper| t == Tonemapper::Libplacebo || (t == Tonemapper::Opencl && self.caps.opencl_bt2390);
        if dolby_vision_p5 && choice.is_some_and(|t| !reshapes(t)) {
            warnings.push("Dolby Vision P5 needs libplacebo".into());
        }
        (choice, warnings)
    }
}

fn encoder_names(caps: &Capabilities) -> String {
    caps.encoders
        .iter()
        .map(|e| if e.main10 { format!("{} (main10)", e.name) } else { e.name.clone() })
        .collect::<Vec<_>>()
        .join(", ")
}

/// libplacebo's measured speed and the AMD paths, for the log.
fn gpu_summary(caps: &Capabilities) -> String {
    let mut parts = Vec::new();
    if let Some(s) = caps.tonemap_speed {
        parts.push(format!("libplacebo 4K at {:.0}/{:.0}/{:.0} fps (high/balanced/fast)", s.high, s.balanced, s.fast));
    }
    if caps.amf_upload {
        parts.push("AMF fed from the GPU".to_owned());
    }
    if caps.qsv_opencl_dovi {
        parts.push("Dolby Vision on OpenCL".to_owned());
    }
    parts.iter().map(|p| format!(", {p}")).collect()
}

fn tonemapper_names(caps: &Capabilities) -> String {
    let quick_sync = caps.qsv_tonemap.then_some(Tonemapper::QuickSync);
    caps.tonemappers.iter().copied().chain(quick_sync).map(|t| t.as_str()).collect::<Vec<_>>().join(", ")
}

fn cache_key(ffmpeg: &Path, ffprobe: &Path) -> String {
    let stamp = |p: &Path| {
        std::fs::metadata(p)
            .map(|m| {
                let mtime =
                    m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                format!("{}:{}:{}", p.display(), m.len(), mtime)
            })
            .unwrap_or_else(|_| format!("{}:missing", p.display()))
    };
    format!(
        "rev{DETECTION_REVISION}|{}|{}|{}|{}",
        env!("CARGO_PKG_VERSION"),
        stamp(ffmpeg),
        stamp(ffprobe),
        devices::stamp()
    )
}

fn read_cache(path: &Path, key: &str) -> Option<CacheFile> {
    let bytes = std::fs::read(path).ok()?;
    let file: CacheFile = serde_json::from_slice(&bytes).ok()?;
    (file.key == key && file.caps.ffmpeg_available).then_some(file)
}

struct Output {
    success: bool,
    text: String,
}

async fn run(program: &Path, args: &[&str], timeout: Duration) -> Option<Output> {
    let mut cmd = Command::new(program);
    configure_child(&mut cmd);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().ok()?;
    let output = tokio::time::timeout(timeout, child.wait_with_output()).await.ok()?.ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(Output { success: output.status.success(), text })
}

/// Encoder candidates in order of preference, with the extra arguments needed
/// to drive a test encode.
fn encoder_candidates() -> Vec<(&'static str, VideoCodec, HwAccel)> {
    let mut list = Vec::new();
    for (codec, prefix, sw) in [(VideoCodec::H264, "h264", "libx264"), (VideoCodec::Hevc, "hevc", "libx265")] {
        if cfg!(any(windows, target_os = "linux")) {
            list.push((leak(format!("{prefix}_nvenc")), codec, HwAccel::Nvenc));
            list.push((leak(format!("{prefix}_qsv")), codec, HwAccel::Qsv));
            list.push((leak(format!("{prefix}_amf")), codec, HwAccel::Amf));
        }
        if cfg!(target_os = "linux") {
            list.push((leak(format!("{prefix}_vaapi")), codec, HwAccel::Vaapi));
        }
        if cfg!(target_os = "macos") {
            list.push((leak(format!("{prefix}_videotoolbox")), codec, HwAccel::Videotoolbox));
        }
        list.push((sw, codec, HwAccel::None));
    }
    list
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// A test encode with `name`; `main10`: 10-bit HEVC (Main 10), as
/// transcodes that keep HDR encode it ([`crate::plan`]'s encoder arguments);
/// `low_power`: Quick Sync's low-power mode, which older Intel GPUs lack.
async fn test_encoder(ffmpeg: &Path, name: &str, accel: HwAccel, main10: bool, low_power: bool) -> bool {
    let mut args: Vec<&str> = vec!["-hide_banner", "-nostdin", "-loglevel", "error"];
    if accel == HwAccel::Vaapi {
        args.extend(["-vaapi_device", devices::vaapi_device()]);
    }
    // On Intel's GPU, as jobs encode.
    let qsv = crate::plan::qsv_vaapi_devices(devices::qsv_device());
    if accel == HwAccel::Qsv && crate::plan::QSV_ON_VAAPI {
        args.extend(qsv.iter().map(String::as_str));
    }
    args.extend(["-f", "lavfi", "-i", "color=black:s=320x240:r=25:d=0.2", "-frames:v", "3"]);
    match (accel, main10) {
        (HwAccel::Vaapi, false) => args.extend(["-vf", "format=nv12,hwupload"]),
        (HwAccel::Vaapi, true) => args.extend(["-vf", "format=p010,hwupload"]),
        (_, false) => args.extend(["-pix_fmt", "yuv420p"]),
        (_, true) => args.extend(["-pix_fmt", crate::plan::ten_bit_format(accel)]),
    }
    args.extend(["-c:v", name]);
    if low_power {
        args.extend(crate::plan::QSV_LOW_POWER);
    }
    // Rate control as jobs ask for it.
    match accel {
        HwAccel::Qsv => args.extend(crate::plan::QSV_VBR.iter().chain(&["-b:v", "1000k", "-maxrate", "1000001"])),
        HwAccel::Vaapi => args.extend(crate::plan::VAAPI_VBR.iter().chain(&["-b:v", "1000k", "-maxrate", "1000k"])),
        _ => {}
    }
    if main10 {
        args.extend(["-profile:v", "main10"]);
        // libx265 would write a source's Dolby Vision metadata, which
        // transcodes leave out (FFmpeg 7.1 and later have the option).
        if accel == HwAccel::None {
            args.extend(["-dolbyvision", "0"]);
        }
    }
    args.extend(["-f", "null", "-"]);
    matches!(run(ffmpeg, &args, Duration::from_secs(30)).await, Some(Output { success: true, .. }))
}

const HDR_TAGS: &str = "setparams=color_trc=smpte2084:color_primaries=bt2020:colorspace=bt2020nc";

/// Frames without colour properties, as Dolby Vision profile 5's come:
/// what tone maps them goes by their metadata, or fails.
const UNTAGGED: &str = "setparams=color_trc=unknown:color_primaries=unknown:colorspace=unknown";

/// The tone mappers' start-up tests, on HDR-tagged frames.
fn libplacebo_test_graph() -> String {
    format!(
        "{HDR_TAGS},libplacebo=w=160:h=120:format=yuv420p:colorspace=bt709:color_primaries=bt709:color_trc=bt709:tonemapping=auto:{}",
        crate::plan::PLACEBO_STATIC_PEAK
    )
}

fn opencl_test_graph() -> String {
    format!(
        "{HDR_TAGS},format=p010,hwupload,tonemap_opencl=tonemap=hable:desat=0:t=bt709:m=bt709:p=bt709:format=nv12,hwdownload,format=nv12"
    )
}

/// Whether a tone mapper's frames reach a VAAPI encoder as transcodes hand
/// them over: the mapper's device (`vulkan=vk`, `opencl=ocl`, named `name`)
/// made from VAAPI's, the frames uploaded back to VAAPI's at the end
/// ([`crate::plan::VAAPI_UPLOAD`]). A plain `hwupload` there would upload
/// to the mapper's device, which the encoder can't take.
async fn test_vaapi_chain(ffmpeg: &Path, device: &str, name: &str, graph: &str) -> bool {
    let vf = format!("{graph},format=nv12,{}", crate::plan::VAAPI_UPLOAD);
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error"].map(String::from).to_vec();
    args.extend(crate::plan::vaapi_filter_devices(device, devices::vaapi_device()));
    args.extend(
        [
            "-filter_hw_device",
            name,
            "-f",
            "lavfi",
            "-i",
            "color=gray:s=320x240:r=25:d=0.2",
            "-vf",
            &vf,
            "-frames:v",
            "3",
        ]
        .map(String::from),
    );
    args.extend(["-c:v", "h264_vaapi", "-f", "null", "-"].map(String::from));
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    matches!(run(ffmpeg, &refs, Duration::from_secs(30)).await, Some(Output { success: true, .. }))
}

/// Whether FFmpeg's Vulkan device is a GPU, as far as it says: Mesa's
/// llvmpipe (in every Mesa install, and in the Docker image) is Vulkan on
/// the CPU.
async fn vulkan_on_gpu(ffmpeg: &Path) -> bool {
    let args = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "verbose",
        "-init_hw_device",
        "vulkan=vk",
        "-f",
        "lavfi",
        "-i",
        "nullsrc=s=16x16:d=0.04",
        "-frames:v",
        "1",
        "-f",
        "null",
        "-",
    ];
    run(ffmpeg, &args, Duration::from_secs(30)).await.is_none_or(|out| !software_vulkan(&out.text))
}

/// FFmpeg's verbose log choosing a CPU device: "Device 0 selected: llvmpipe
/// (LLVM 19.1.7, 256 bits) (software) (0x0)".
fn software_vulkan(log: &str) -> bool {
    log.lines().any(|line| line.contains(" selected: ") && line.contains("(software)"))
}

/// Whether AMF encodes frames FFmpeg uploads to the AMD GPU (Direct3D 11),
/// as transcodes that filter in memory send them ([`crate::plan`]).
async fn test_amf_upload(ffmpeg: &Path) -> bool {
    let args = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-init_hw_device",
        crate::plan::AMF_D3D11_DEVICE,
        "-filter_hw_device",
        "dx",
        "-f",
        "lavfi",
        "-i",
        "color=black:s=320x240:r=25:d=0.2",
        "-frames:v",
        "3",
        "-vf",
        "format=nv12,hwupload",
        "-c:v",
        "h264_amf",
        "-f",
        "null",
        "-",
    ];
    matches!(run(ffmpeg, &args, Duration::from_secs(30)).await, Some(Output { success: true, .. }))
}

/// How long measuring libplacebo may take at most: the server waits two
/// minutes for the transcoder to start.
const TONEMAP_MEASURE_BUDGET: Duration = Duration::from_secs(20);

/// libplacebo's speed at each quality ([`TonemapSpeed`]): synthetic 4K PQ
/// frames from memory tone mapped to 4K SDR and back, as transcodes without
/// Vulkan decoding do, timed over 6 and 42 frames so that FFmpeg's and
/// Vulkan's start drop out (fewer frames gave speeds 40 % apart from one run
/// to the next). Measured alone, after the other tests, the cheapest quality
/// first: where a slow GPU runs out of time, the qualities left unmeasured
/// are never taken.
async fn measure_tonemap_speed(ffmpeg: &Path) -> Option<TonemapSpeed> {
    let time = |quality: TonemapQuality, frames: u32| async move {
        let vf = format!(
            "{HDR_TAGS},libplacebo=w=3840:h=2160:format=nv12:colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv:tonemapping=auto:{}{}",
            crate::plan::PLACEBO_STATIC_PEAK,
            crate::plan::placebo_quality_options(quality, (3840, 2160), (3840, 2160), true)
        );
        let frames = frames.to_string();
        let args = [
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-init_hw_device",
            "vulkan=vk",
            "-filter_hw_device",
            "vk",
            "-f",
            "lavfi",
            "-i",
            "color=gray:s=3840x2160:r=24,format=p010le",
            "-vf",
            &vf,
            "-frames:v",
            &frames,
            "-f",
            "null",
            "-",
        ];
        let started = Instant::now();
        let ok = matches!(run(ffmpeg, &args, Duration::from_secs(60)).await, Some(Output { success: true, .. }));
        ok.then(|| started.elapsed().as_secs_f64())
    };
    let started = Instant::now();
    // The first libplacebo process starts slower than the rest (Vulkan's
    // and the driver's first start, shaders compiled): timed, its 6 frames
    // took as long as 42 after it, and `fast` read 720 frames a second on an
    // Iris Xe that makes 150. It warms up untimed.
    let _ = time(TonemapQuality::Fast, 6).await;
    let mut speed = TonemapSpeed::default();
    for quality in [TonemapQuality::Fast, TonemapQuality::Balanced, TonemapQuality::High] {
        // Twice where the time allows, the faster short and the faster long
        // run counting, each on its own: other work on the GPU (seek
        // previews being made, say) and a process's start only ever slow a
        // run down. Measured alone, balanced read 37 frames a second beside a
        // library's seek previews against 53 to 56 without. Pairing the runs
        // instead, a slow short run read as a fast quality: `high` at 172
        // and 283 frames a second where it makes 52.
        let (mut short, mut long) = (f64::INFINITY, f64::INFINITY);
        for attempt in 0..2 {
            let limit = if attempt == 0 { TONEMAP_MEASURE_BUDGET } else { TONEMAP_MEASURE_BUDGET * 3 / 4 };
            if started.elapsed() > limit {
                break;
            }
            if let (Some(s), Some(l)) = (time(quality, 6).await, time(quality, 42).await) {
                (short, long) = (short.min(s), long.min(l));
            }
        }
        if let Some(fps) = frames_a_second(short, long) {
            let measured = match quality {
                TonemapQuality::Fast => &mut speed.fast,
                TonemapQuality::Balanced => &mut speed.balanced,
                TonemapQuality::High => &mut speed.high,
            };
            *measured = fps;
        }
    }
    // Each quality does less than the one above it: one that read slower was
    // slowed by something else (`fast` read 51 frames a second where
    // `balanced` read 97, the first runs of a start on an Iris Xe). One that
    // measured nothing (too fast to time) is as fast as the one above.
    speed.balanced = speed.balanced.max(speed.high);
    speed.fast = speed.fast.max(speed.balanced);
    (speed.fast > 0.0 || speed.balanced > 0.0 || speed.high > 0.0).then_some(speed)
}

/// The frames a second the 36 frames between a 6-frame run of `short`
/// seconds and a 42-frame one of `long` show; none where the long run was
/// no slower than the short one, which measured nothing.
fn frames_a_second(short: f64, long: f64) -> Option<f64> {
    (short.is_finite() && long.is_finite() && long > short + 0.05).then(|| 36.0 / (long - short))
}

async fn test_filter(ffmpeg: &Path, pre: &[&str], vf: &str) -> bool {
    let mut args: Vec<&str> = vec!["-hide_banner", "-nostdin", "-loglevel", "error"];
    args.extend_from_slice(pre);
    args.extend([
        "-f",
        "lavfi",
        "-i",
        "color=gray:s=320x240:r=25:d=0.1",
        "-vf",
        vf,
        "-frames:v",
        "1",
        "-f",
        "null",
        "-",
    ]);
    matches!(run(ffmpeg, &args, Duration::from_secs(30)).await, Some(Output { success: true, .. }))
}

/// Determine whether `-segment_times` are relative to the first packet. A 6 s
/// source is cut from 2 s with a single split at 2.5: relative semantics end
/// the first segment around 5 s, absolute semantics around 3 s.
async fn test_segment_semantics(ffmpeg: &Path) -> bool {
    let dir = std::env::temp_dir().join(format!("pley-selftest-{}", uuid::Uuid::new_v4().simple()));
    if std::fs::create_dir_all(&dir).is_err() {
        return true;
    }
    let pattern = dir.join("%d.ts");
    let pattern = pattern.to_string_lossy();
    let args = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-ss",
        "2",
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=64x64:rate=10:duration=6",
        "-output_ts_offset",
        "2",
        "-c:v",
        "mpeg2video",
        "-g",
        "1000",
        "-force_key_frames",
        "expr:gte(t,n_forced*1)",
        "-f",
        "segment",
        "-segment_format",
        "mpegts",
        "-segment_times",
        "2.5",
        "-segment_list",
        "pipe:1",
        "-segment_list_type",
        "csv",
        &pattern,
    ];
    let result = run(ffmpeg, &args, Duration::from_secs(30)).await;
    let _ = std::fs::remove_dir_all(&dir);
    let Some(out) = result.filter(|o| o.success) else {
        tracing::warn!("segment muxer self-test failed; assuming relative segment times");
        return true;
    };
    let first_end = out
        .text
        .lines()
        .find(|l| crate::job::parse_list_line(l) == Some(0))
        .and_then(|l| l.split(',').nth(2))
        .and_then(|v| v.trim().parse::<f64>().ok());
    match first_end {
        Some(end) => end > 4.0,
        None => true,
    }
}

async fn detect_capabilities(ffmpeg: &Path, ffprobe: &Path) -> (Capabilities, bool) {
    let mut caps = Capabilities {
        ffmpeg_path: ffmpeg.display().to_string(),
        ffprobe_path: ffprobe.display().to_string(),
        ..Default::default()
    };
    let timeout = Duration::from_secs(30);
    let (version, probe, encoders, hwaccels, filters, muxers, bsfs, placebo_help, opencl_help) = tokio::join!(
        run(ffmpeg, &["-hide_banner", "-version"], timeout),
        run(ffprobe, &["-hide_banner", "-version"], timeout),
        run(ffmpeg, &["-hide_banner", "-encoders"], timeout),
        run(ffmpeg, &["-hide_banner", "-hwaccels"], timeout),
        run(ffmpeg, &["-hide_banner", "-filters"], timeout),
        run(ffmpeg, &["-hide_banner", "-muxers"], timeout),
        run(ffmpeg, &["-hide_banner", "-bsfs"], timeout),
        run(ffmpeg, &["-hide_banner", "-h", "filter=libplacebo"], timeout),
        run(ffmpeg, &["-hide_banner", "-h", "filter=tonemap_opencl"], timeout),
    );
    let Some(version) = version.filter(|o| o.success) else {
        return (caps, true);
    };
    caps.ffmpeg_available = true;
    caps.ffprobe_available = probe.is_some_and(|o| o.success);
    caps.ffmpeg_version = version
        .text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("ffmpeg version "))
        .and_then(|rest| rest.split_whitespace().next())
        .map(str::to_owned);

    let encoder_names = second_column(encoders.map(|o| o.text).unwrap_or_default());
    let filter_names = second_column(filters.map(|o| o.text).unwrap_or_default());
    caps.chromaprint = second_column(muxers.map(|o| o.text).unwrap_or_default()).contains("chromaprint");
    // `-bsfs` lists one name a line.
    caps.h264_metadata = bsfs.is_some_and(|o| o.text.lines().any(|l| l.trim() == "h264_metadata"));
    caps.hwaccels = hwaccels
        .map(|o| o.text)
        .unwrap_or_default()
        .lines()
        .skip_while(|l| !l.starts_with("Hardware acceleration methods"))
        .skip(1)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();

    // Run the (slow) test encodes concurrently, a few at a time.
    let limiter = Arc::new(Semaphore::new(6));
    let candidates: Vec<_> =
        encoder_candidates().into_iter().filter(|(name, ..)| encoder_names.contains(*name)).collect();
    let encoder_tests = futures::future::join_all(candidates.iter().map(|(name, codec, accel)| {
        let limiter = limiter.clone();
        async move {
            let _permit = limiter.acquire().await.ok();
            let ok = test_encoder(ffmpeg, name, *accel, false, false).await;
            let main10 = ok && *codec == VideoCodec::Hevc && test_encoder(ffmpeg, name, *accel, true, false).await;
            // Low power where every mode jobs use passes in it, Main 10 included.
            let low_power = ok
                && *accel == HwAccel::Qsv
                && test_encoder(ffmpeg, name, *accel, false, true).await
                && (!main10 || test_encoder(ffmpeg, name, *accel, true, true).await);
            (
                EncoderInfo {
                    name: (*name).to_owned(),
                    codec: *codec,
                    hardware: *accel != HwAccel::None,
                    accel: *accel,
                    main10,
                    low_power,
                },
                ok,
            )
        }
    }));

    let has = |f: &str| filter_names.contains(f);
    // Whether libplacebo works, and on a GPU.
    let libplacebo_test = async {
        if !has("libplacebo") {
            return (false, false);
        }
        let vf = libplacebo_test_graph();
        tokio::join!(
            test_filter(ffmpeg, &["-init_hw_device", "vulkan=vk", "-filter_hw_device", "vk"], &vf),
            vulkan_on_gpu(ffmpeg)
        )
    };
    let opencl_test = async {
        if !has("tonemap_opencl") {
            return false;
        }
        test_filter(ffmpeg, &["-init_hw_device", "opencl=ocl", "-filter_hw_device", "ocl"], &opencl_test_graph()).await
    };
    let zscale_test = async {
        if !(has("zscale") && has("tonemap")) {
            return false;
        }
        let vf = format!("{HDR_TAGS},{}", crate::plan::ZSCALE_TONEMAP);
        test_filter(ffmpeg, &[], &vf).await
    };
    // Whether -readrate works, and -readrate_catchup with it (FFmpeg 8).
    let readrate_test = async {
        let reads = |catchup: &'static [&'static str]| {
            let mut args = vec!["-hide_banner", "-nostdin", "-loglevel", "error", "-readrate", "50"];
            args.extend_from_slice(catchup);
            args.extend_from_slice(&[
                "-readrate_initial_burst",
                "1",
                "-f",
                "lavfi",
                "-i",
                "color=black:s=64x64:r=25:d=0.1",
                "-frames:v",
                "1",
                "-f",
                "null",
                "-",
            ]);
            async move { matches!(run(ffmpeg, &args, timeout).await, Some(Output { success: true, .. })) }
        };
        let readrate = reads(&[]).await;
        (readrate, readrate && reads(&["-readrate_catchup", "60"]).await)
    };
    let amf_upload_test = async {
        cfg!(windows) && encoder_names.contains("h264_amf") && hwaccel_listed(&caps, "d3d11va") && {
            test_amf_upload(ffmpeg).await
        }
    };

    let (encoder_results, (libplacebo, libplacebo_on_gpu), opencl, zscale, (readrate, catchup), relative, amf_upload) = tokio::join!(
        encoder_tests,
        libplacebo_test,
        opencl_test,
        zscale_test,
        readrate_test,
        test_segment_semantics(ffmpeg),
        amf_upload_test,
    );

    // Hardware encoders first (in candidate order), then software.
    let working: Vec<EncoderInfo> = encoder_results.into_iter().filter(|(_, ok)| *ok).map(|(e, _)| e).collect();
    caps.encoders =
        working.iter().filter(|e| e.hardware).cloned().chain(working.iter().filter(|e| !e.hardware).cloned()).collect();
    caps.hdr_transcode = caps.encoders.iter().any(|e| e.main10);

    // libplacebo on a CPU's Vulkan (Mesa's llvmpipe) tone maps several
    // times slower than zscale: it comes last then, for Dolby Vision
    // profile 5, which only it shows in its colors.
    let mappers = if libplacebo_on_gpu {
        [(libplacebo, Tonemapper::Libplacebo), (opencl, Tonemapper::Opencl), (zscale, Tonemapper::Zscale)]
    } else {
        [(opencl, Tonemapper::Opencl), (zscale, Tonemapper::Zscale), (libplacebo, Tonemapper::Libplacebo)]
    };
    for (ok, mapper) in mappers {
        if ok {
            caps.tonemappers.push(mapper);
        }
    }
    caps.dolby_vision = libplacebo;
    // FFmpeg 7.1 (Debian 13's) has no `shader_cache`, and a graph naming
    // it fails: the tests above leave it out, so they can't tell.
    caps.placebo_shader_cache =
        libplacebo && placebo_help.as_ref().is_some_and(|o| lists_option(&o.text, "shader_cache"));
    // Pley's FFmpeg (patch 1002): a peak for HDR10 that says none.
    caps.placebo_fallback_peak =
        libplacebo && placebo_help.as_ref().is_some_and(|o| lists_option(&o.text, "fallback_peak"));
    // jellyfin-ffmpeg's tonemap_opencl (Pley's FFmpeg carries it) lists
    // BT.2390 among its curves; FFmpeg's own doesn't.
    caps.opencl_bt2390 = opencl_help.is_some_and(|o| lists_option(&o.text, "bt2390"));
    caps.subtitle_burn_image = has("overlay");
    caps.subtitle_burn_text = has("subtitles");
    caps.readrate = readrate;
    caps.readrate_catchup = catchup;
    caps.amf_upload = amf_upload && caps.encoders.iter().any(|e| e.name == "h264_amf");

    // The GPU tone mappers whose frames reach VAAPI's encoder (Linux).
    if caps.encoders.iter().any(|e| e.name == "h264_vaapi") {
        let (placebo, ocl) = tokio::join!(
            async { libplacebo && test_vaapi_chain(ffmpeg, "vulkan=vk", "vk", &libplacebo_test_graph()).await },
            async { opencl && test_vaapi_chain(ffmpeg, "opencl=ocl", "ocl", &opencl_test_graph()).await },
        );
        caps.vaapi_tonemappers = caps
            .tonemappers
            .iter()
            .copied()
            .filter(|m| match m {
                Tonemapper::Libplacebo => placebo,
                Tonemapper::Opencl => ocl,
                Tonemapper::Zscale => true,
                Tonemapper::QuickSync => false,
            })
            .collect();
    }

    let gpu = detect_gpu_pipelines(ffmpeg, &encoder_names, &filter_names, &caps, libplacebo && libplacebo_on_gpu).await;
    caps.vulkan_decode = gpu.vulkan_decode;
    caps.vaapi_vulkan_decode = gpu.vaapi_vulkan_decode;
    caps.vulkan_encode = gpu.vulkan_encode;
    caps.gpu_pipelines = gpu.pipelines;
    caps.qsv_tonemap = gpu.qsv_tonemap;
    caps.qsv_overlay = gpu.qsv_overlay;
    caps.qsv_opencl_dovi = gpu.qsv_opencl_dovi;
    // Pley's FFmpeg rebuilds Dolby Vision FEL's full picture (experimental).
    caps.dolby_vision_fel_full = filter_names.contains("dovi_compose");
    if libplacebo && libplacebo_on_gpu {
        caps.tonemap_speed = measure_tonemap_speed(ffmpeg).await;
    }
    (caps, relative)
}

fn hwaccel_listed(caps: &Capabilities, name: &str) -> bool {
    caps.hwaccels.iter().any(|h| h == name)
}

#[derive(Default)]
struct GpuDetection {
    vulkan_decode: Vec<String>,
    vaapi_vulkan_decode: Vec<String>,
    vulkan_encode: bool,
    pipelines: Vec<GpuPipeline>,
    qsv_tonemap: bool,
    qsv_overlay: bool,
    qsv_opencl_dovi: bool,
}

/// x265's options for the 10-bit test clip: a 1000-nit P3 mastering display
/// and the content's light levels, HDR10's static metadata.
const HDR10_TEST_PARAMS: &str = "log-level=error:max-cll=1000,400:\
    master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1)";

/// Encoder arguments for the tiny clips used to probe hardware decoding,
/// keyed like [`pley_transcoder_api::decode_key`].
fn test_clip_specs(encoders: &HashSet<String>) -> Vec<(&'static str, Vec<&'static str>)> {
    let mut specs = Vec::new();
    if encoders.contains("libx264") {
        specs.push(("h264", vec!["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"]));
    }
    if encoders.contains("libx265") {
        let x265 = ["-c:v", "libx265", "-preset", "ultrafast", "-x265-params", "log-level=error"];
        specs.push(("hevc", [x265.as_slice(), &["-pix_fmt", "yuv420p"]].concat()));
        // With HDR10's static metadata, as HDR10 video carries it: Quick
        // Sync's scaler tone maps only frames that have it, and passes the
        // others through, which would test nothing.
        let hdr10 = ["-c:v", "libx265", "-preset", "ultrafast", "-x265-params", HDR10_TEST_PARAMS];
        specs.push(("hevc10", [hdr10.as_slice(), &["-pix_fmt", "yuv420p10le"]].concat()));
        // What cameras record: some GPUs decode it (Intel's), others don't.
        specs.push(("hevc10-yuv422p10le", [x265.as_slice(), &["-pix_fmt", "yuv422p10le"]].concat()));
    }
    if encoders.contains("libsvtav1") {
        specs.push(("av1", vec!["-c:v", "libsvtav1", "-preset", "12", "-pix_fmt", "yuv420p10le"]));
    } else if encoders.contains("libaom-av1") {
        specs.push(("av1", vec!["-c:v", "libaom-av1", "-cpu-used", "8", "-pix_fmt", "yuv420p"]));
    }
    if encoders.contains("libvpx-vp9") {
        specs.push((
            "vp9",
            vec!["-c:v", "libvpx-vp9", "-deadline", "realtime", "-cpu-used", "8", "-pix_fmt", "yuv420p"],
        ));
    }
    specs.push(("mpeg2video", vec!["-c:v", "mpeg2video", "-pix_fmt", "yuv420p"]));
    specs
}

/// Self-test zero-copy GPU pipelines per input codec. A pipeline only counts
/// for a codec if decoding, filtering and encoding all stay on the GPU; when
/// the decoder cannot handle a codec, ffmpeg falls back to system-memory
/// frames and the GPU-only filter fails, so the test is conservative.
/// libplacebo takes frames from memory too: Vulkan decoding counts only when
/// FFmpeg says the frames reached it in Vulkan's.
async fn detect_gpu_pipelines(
    ffmpeg: &Path,
    encoders: &HashSet<String>,
    filters: &HashSet<String>,
    caps: &Capabilities,
    libplacebo: bool,
) -> GpuDetection {
    let working = |name: &str| caps.encoders.iter().any(|e| e.name == name);
    let hwaccel = |name: &str| caps.hwaccels.iter().any(|h| h == name);
    // Quick Sync's frames on the GPU: on Linux on Intel's render node, its
    // device made from VAAPI's, as jobs make it.
    let qsv_gpu = crate::plan::qsv_gpu_devices(&crate::plan::QSV_DECODE, devices::qsv_device());
    let qsv_gpu: Vec<&str> = qsv_gpu.iter().map(String::as_str).collect();
    let qsv_overlay_devices = crate::plan::qsv_gpu_devices(&crate::plan::QSV_D3D11_DEVICES, devices::qsv_device());
    let qsv_overlay_devices: Vec<&str> = qsv_overlay_devices.iter().map(String::as_str).collect();
    let mut candidates: Vec<(HwAccel, Vec<&str>, &str, Vec<&str>)> = Vec::new();
    if working("h264_nvenc") && hwaccel("cuda") && filters.contains("scale_cuda") {
        candidates.push((
            HwAccel::Nvenc,
            vec!["-hwaccel", "cuda", "-hwaccel_output_format", "cuda", "-extra_hw_frames", "4"],
            "scale_cuda=160:120:format=nv12",
            vec!["-c:v", "h264_nvenc"],
        ));
    }
    if cfg!(target_os = "linux") && working("h264_vaapi") && hwaccel("vaapi") && filters.contains("scale_vaapi") {
        candidates.push((
            HwAccel::Vaapi,
            vec!["-hwaccel", "vaapi", "-hwaccel_device", devices::vaapi_device(), "-hwaccel_output_format", "vaapi"],
            "scale_vaapi=w=160:h=120:format=nv12",
            vec!["-c:v", "h264_vaapi"],
        ));
    }
    if working("h264_qsv") && hwaccel("qsv") && filters.contains("vpp_qsv") {
        candidates.push((HwAccel::Qsv, qsv_gpu.clone(), "vpp_qsv=w=160:h=120:format=nv12", vec!["-c:v", "h264_qsv"]));
    }
    if cfg!(target_os = "macos")
        && working("h264_videotoolbox")
        && hwaccel("videotoolbox")
        && filters.contains("scale_vt")
    {
        candidates.push((
            HwAccel::Videotoolbox,
            vec!["-hwaccel", "videotoolbox", "-hwaccel_output_format", "videotoolbox_vld"],
            "scale_vt=w=160:h=120",
            vec!["-c:v", "h264_videotoolbox"],
        ));
    }
    // AMD on Windows: Direct3D 11 frames from the decoder to AMF. Upstream
    // FFmpeg 9.0's scale_d3d11 makes no textures AMF takes on Vega; patched
    // builds' does, for 8-bit video.
    const D3D11: [&str; 12] = [
        "-init_hw_device",
        crate::plan::AMF_D3D11_DEVICE,
        "-filter_hw_device",
        "dx",
        "-hwaccel",
        "d3d11va",
        "-hwaccel_device",
        "dx",
        "-hwaccel_output_format",
        "d3d11",
        "-extra_hw_frames",
        "8",
    ];
    let amf = cfg!(windows) && working("h264_amf") && hwaccel("d3d11va") && filters.contains("scale_d3d11");
    if amf {
        candidates.push((
            HwAccel::Amf,
            D3D11.to_vec(),
            "scale_d3d11=width=160:height=120:format=nv12",
            vec!["-c:v", "h264_amf"],
        ));
    }
    let vulkan = libplacebo && hwaccel("vulkan");
    if candidates.is_empty() && !vulkan {
        return GpuDetection::default();
    }

    let dir = std::env::temp_dir().join(format!("pley-gpu-selftest-{}", uuid::Uuid::new_v4().simple()));
    if std::fs::create_dir_all(&dir).is_err() {
        return GpuDetection::default();
    }
    let limiter = Arc::new(Semaphore::new(6));
    let clips: Vec<(&str, PathBuf)> =
        futures::future::join_all(test_clip_specs(encoders).into_iter().map(|(key, args)| {
            let dir = dir.clone();
            let limiter = limiter.clone();
            async move {
                let _permit = limiter.acquire().await.ok();
                let out = dir.join(format!("{key}.mkv"));
                let out_arg = out.to_string_lossy().into_owned();
                let mut cmd = vec![
                    "-hide_banner",
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=320x240:rate=25:duration=0.4",
                ];
                cmd.extend(args);
                cmd.push(&out_arg);
                let ok = matches!(run(ffmpeg, &cmd, Duration::from_secs(60)).await, Some(Output { success: true, .. }));
                ok.then_some((key, out))
            }
        }))
        .await
        .into_iter()
        .flatten()
        .collect();

    let run_logged = |args: Vec<String>| {
        let limiter = limiter.clone();
        async move {
            let _permit = limiter.acquire().await.ok();
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run(ffmpeg, &refs, Duration::from_secs(30)).await
        }
    };
    let run_test = |args: Vec<String>| {
        let output = run_logged(args);
        async move { matches!(output.await, Some(Output { success: true, .. })) }
    };
    let base = |loglevel: &str, pre: &[&str], clip: &Path| -> Vec<String> {
        let mut v: Vec<String> =
            ["-hide_banner", "-nostdin", "-loglevel", loglevel].iter().map(|s| s.to_string()).collect();
        v.extend(pre.iter().map(|s| s.to_string()));
        v.push("-i".into());
        v.push(clip.to_string_lossy().into_owned());
        v
    };

    // Vulkan decode → libplacebo (HDR/Dolby Vision tone mapping path).
    const VK: [&str; 10] = [
        "-init_hw_device",
        "vulkan=vk",
        "-filter_hw_device",
        "vk",
        "-hwaccel",
        "vulkan",
        "-hwaccel_device",
        "vk",
        "-hwaccel_output_format",
        "vulkan",
    ];
    let vulkan_tests = futures::future::join_all(clips.iter().filter(|_| vulkan).map(|(key, clip)| {
        let mut args = base("verbose", &VK, clip);
        args.extend(
            ["-vf", "libplacebo=w=160:h=120:format=nv12,hwdownload,format=nv12", "-f", "null", "-"].map(String::from),
        );
        let test = run_logged(args);
        async move {
            let ok = test.await.is_some_and(|o| o.success && frames_arrive_in(&o.text, "vulkan"));
            (key.to_string(), ok)
        }
    }));
    // The same before a VAAPI encoder: Vulkan's device made from VAAPI's,
    // the frames uploaded back to VAAPI's.
    let mut vk_va: Vec<String> = crate::plan::vaapi_filter_devices("vulkan=vk", devices::vaapi_device()).to_vec();
    vk_va.extend(VK[2..].iter().map(|s| s.to_string()));
    let vk_va: Vec<&str> = vk_va.iter().map(String::as_str).collect();
    let vaapi_vulkan = vulkan && working("h264_vaapi");
    let vaapi_vulkan_tests = futures::future::join_all(clips.iter().filter(|_| vaapi_vulkan).map(|(key, clip)| {
        let mut args = base("verbose", &vk_va, clip);
        let vf = format!("libplacebo=w=160:h=120:format=nv12,hwdownload,format=nv12,{}", crate::plan::VAAPI_UPLOAD);
        args.extend(["-vf".to_string(), vf]);
        args.extend(["-c:v", "h264_vaapi", "-f", "null", "-"].map(String::from));
        let test = run_logged(args);
        async move {
            let ok = test.await.is_some_and(|o| o.success && frames_arrive_in(&o.text, "vulkan"));
            (key.to_string(), ok)
        }
    }));
    let vulkan_encode_test = async {
        let Some((_, clip)) = clips.iter().find(|(k, _)| *k == "hevc10").or(clips.first()) else { return false };
        if !(vulkan && encoders.contains("h264_vulkan")) {
            return false;
        }
        let mut args = base("error", &VK, clip);
        args.extend(
            [
                "-vf",
                "libplacebo=w=160:h=120:format=nv12",
                "-c:v",
                "h264_vulkan",
                "-rc_mode",
                "vbr",
                "-b:v",
                "1M",
                "-force_key_frames",
                "expr:gte(t,n_forced*0.2)",
                "-f",
                "null",
                "-",
            ]
            .map(String::from),
        );
        run_test(args).await
    };
    let pipeline_tests = futures::future::join_all(candidates.iter().map(|(accel, pre, vf, enc)| {
        let tests = futures::future::join_all(clips.iter().map(|(key, clip)| {
            let mut args = base("error", pre, clip);
            args.extend(["-vf".to_string(), vf.to_string()]);
            args.extend(enc.iter().map(|s| s.to_string()));
            args.extend(["-f", "null", "-"].map(String::from));
            let test = run_test(args);
            async move { (key.to_string(), test.await) }
        }));
        async move { (*accel, tests.await) }
    }));
    // HDR kept on Quick Sync: the 10-bit sources' frames stay in 10 bits
    // from its decoder through its scaler into its HEVC Main 10 encoder.
    let qsv_main10 = candidates.iter().any(|(accel, ..)| *accel == HwAccel::Qsv)
        && caps.encoders.iter().any(|e| e.name == "hevc_qsv" && e.main10);
    let main10_tests = futures::future::join_all(
        clips.iter().filter(|(key, _)| qsv_main10 && MAIN10_KEYS.contains(key)).map(|(key, clip)| {
            let mut args = base("error", &qsv_gpu, clip);
            args.extend(
                [
                    "-vf",
                    "vpp_qsv=w=160:h=120:format=p010le",
                    "-c:v",
                    "hevc_qsv",
                    "-profile:v",
                    "main10",
                    "-f",
                    "null",
                    "-",
                ]
                .map(String::from),
            );
            let test = run_test(args);
            async move { (key.to_string(), test.await) }
        }),
    );
    // Quick Sync into its H.264 encoder: what the next three tests need.
    let qsv_h264 = candidates.iter().any(|(accel, ..)| *accel == HwAccel::Qsv) && working("h264_qsv");
    // HDR10 tone mapped by Quick Sync's scaler (VAAPI's on Linux), from its
    // decoder into its H.264 encoder (see `Tonemapper::QuickSync`).
    let qsv_tonemap_test = async {
        let Some((_, clip)) = clips.iter().find(|(k, _)| *k == "hevc10") else { return false };
        if !qsv_h264 {
            return false;
        }
        let mut args = base("error", &qsv_gpu, clip);
        let vf = format!("{HDR_TAGS},{}", crate::plan::qsv_tonemap(160, 120));
        args.extend(["-vf".to_string(), vf]);
        args.extend(["-c:v", "h264_qsv", "-f", "null", "-"].map(String::from));
        run_test(args).await
    };
    // Picture subtitles laid on by Quick Sync: a translucent picture
    // uploaded to its device, made from Direct3D 11's (VAAPI's on Linux).
    let qsv_overlay_test = async {
        let Some((_, clip)) = clips.iter().find(|(k, _)| *k == "h264") else { return false };
        if !qsv_h264 {
            return false;
        }
        let mut args = base("error", &qsv_overlay_devices, clip);
        args.extend(
            [
                "-f",
                "lavfi",
                "-i",
                "color=c=white@0.5:s=160x40:r=25:d=0.4",
                "-filter_complex",
                "[1:0]format=bgra,hwupload=extra_hw_frames=16[subs];[0:0][subs]overlay_qsv=y=main_h-overlay_h[v]",
                "-map",
                "[v]",
                "-c:v",
                "h264_qsv",
                "-f",
                "null",
                "-",
            ]
            .map(String::from),
        );
        run_test(args).await
    };
    // Dolby Vision tone mapped by OpenCL between FFmpeg's decoder on
    // Direct3D 11 and Quick Sync's encoder, the frames mapped all the way
    // (Windows; see `plan::qsv_opencl_dovi`). Its clip carries a profile 5
    // RPU on every frame, and its frames come untagged, as profile 5's do:
    // OpenCL's tone mapper refuses such frames without the metadata, so the
    // test passes only where it arrives. FFmpeg's own builds aren't tried:
    // their tone mapper has no BT.2390 and reads no Dolby Vision, and they
    // map no 10-bit Quick Sync frames to OpenCL.
    let qsv_opencl_dovi_test = async {
        let Some((_, clip)) = clips.iter().find(|(k, _)| *k == "hevc10") else { return false };
        if !(cfg!(windows) && qsv_h264 && caps.opencl_bt2390 && hwaccel("d3d11va")) {
            return false;
        }
        let Some(clip) = dolby_vision_clip(ffmpeg, clip, &dir).await else { return false };
        let mut args = base("error", &crate::plan::QSV_OPENCL_DEVICES, &clip);
        let vf = format!("{UNTAGGED},{}", crate::plan::qsv_opencl_dovi(160, 120, true));
        args.extend(["-vf".to_string(), vf]);
        args.extend(["-c:v", "h264_qsv", "-f", "null", "-"].map(String::from));
        run_test(args).await
    };
    let (
        vulkan_results,
        vaapi_vulkan_results,
        vulkan_encode,
        pipeline_results,
        main10_results,
        qsv_tonemap,
        qsv_overlay,
        qsv_opencl_dovi,
    ) = tokio::join!(
        vulkan_tests,
        vaapi_vulkan_tests,
        vulkan_encode_test,
        pipeline_tests,
        main10_tests,
        qsv_tonemap_test,
        qsv_overlay_test,
        qsv_opencl_dovi_test
    );
    let _ = std::fs::remove_dir_all(&dir);

    let ok_keys =
        |results: Vec<(String, bool)>| results.into_iter().filter(|(_, ok)| *ok).map(|(k, _)| k).collect::<Vec<_>>();
    let main10 = ok_keys(main10_results);
    GpuDetection {
        vulkan_decode: ok_keys(vulkan_results),
        vaapi_vulkan_decode: ok_keys(vaapi_vulkan_results),
        vulkan_encode,
        pipelines: pipeline_results
            .into_iter()
            .map(|(accel, results)| {
                let codecs = ok_keys(results);
                let main10 = match accel {
                    HwAccel::Qsv => main10.iter().filter(|k| codecs.contains(k)).cloned().collect(),
                    _ => Vec::new(),
                };
                GpuPipeline { accel, codecs, main10 }
            })
            .filter(|p| !p.codecs.is_empty())
            .collect(),
        qsv_tonemap,
        qsv_overlay,
        qsv_opencl_dovi,
    }
}

/// `clip` (HEVC in Matroska) as a bare stream in `dir`, a Dolby Vision
/// profile 5 RPU after each of its frames ([`crate::dovi::test_rpu`]):
/// FFmpeg's encoders write RPUs only from metadata a decoder read, so a
/// clip made from nothing gets its own. FFmpeg tells the profile from the
/// RPUs where a stream has no Dolby Vision configuration.
async fn dolby_vision_clip(ffmpeg: &Path, clip: &Path, dir: &Path) -> Option<PathBuf> {
    let (plain, out) = (dir.join("plain.hevc"), dir.join("dolby-vision.hevc"));
    let (from, to) = (clip.to_string_lossy(), plain.to_string_lossy());
    let args = ["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i", &from, "-c:v", "copy", "-f", "hevc", &to];
    run(ffmpeg, &args, Duration::from_secs(30)).await.filter(|o| o.success)?;
    let stream = tokio::fs::read(&plain).await.ok()?;
    let rpu = crate::dovi::test_rpu().ok()?;
    tokio::fs::write(&out, crate::dovi::with_rpus(&stream, &rpu)).await.ok()?;
    Some(out)
}

/// The test clips HDR comes in: 10-bit HEVC and AV1 (10 bits from SVT-AV1).
const MAIN10_KEYS: [&str; 2] = ["hevc10", "av1"];

/// Whether every frame came to the filters in `format`, by an FFmpeg log at
/// `-loglevel verbose` (each filter graph input names its frames' format as
/// it is set up). FFmpeg decodes in software when the hardware decoder asked
/// for can't, saying so for one that fails to start ([`hardware_decoder_failed`])
/// and nothing for a codec it has none for.
fn frames_arrive_in(log: &str, format: &str) -> bool {
    let formats: Vec<&str> = log
        .lines()
        .filter_map(|line| line.split_whitespace().find_map(|w| w.strip_prefix("pixfmt:")))
        .filter(|f| !f.is_empty())
        .collect();
    !formats.is_empty() && formats.iter().all(|f| *f == format)
}

/// An FFmpeg error line saying a hardware decoder couldn't start: FFmpeg
/// goes on, decoding in software.
pub fn hardware_decoder_failed(line: &str) -> bool {
    line.contains("Failed setup for format") || line.contains("Invalid setup for format")
}

/// Names in the second whitespace-separated column of `ffmpeg -encoders` /
/// `-filters` / `-muxers` output.
fn second_column(text: String) -> HashSet<String> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            (flags.len() <= 7 && flags.chars().all(|c| c == '.' || c.is_ascii_uppercase())).then(|| name.to_owned())
        })
        .collect()
}

/// Whether `ffmpeg -h filter=…` output lists the option `name`: one line
/// an option, its name first.
fn lists_option(help: &str, name: &str) -> bool {
    help.lines().any(|line| line.split_whitespace().next() == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_mapping_speed_takes_the_fastest_short_and_long_runs() {
        // Runs paired as they came, the slow short one would read 36 / 0.2.
        let (shorts, longs) = ([0.9_f64, 0.6], [1.1_f64, 1.3]);
        let short = shorts.iter().copied().fold(f64::INFINITY, f64::min);
        let long = longs.iter().copied().fold(f64::INFINITY, f64::min);
        let fps = frames_a_second(short, long).unwrap();
        assert!((fps - 72.0).abs() < 1e-9, "{fps}");
        // A long run no slower than the short one, or none at all: nothing.
        assert_eq!(frames_a_second(0.6, 0.62), None);
        assert_eq!(frames_a_second(f64::INFINITY, f64::INFINITY), None);
    }

    #[test]
    fn tells_a_tonemap_opencl_with_bt2390() {
        // jellyfin-ffmpeg's (and Pley's) `-h filter=tonemap_opencl`, cut down.
        let patched = "tonemap_opencl AVOptions:\n   tonemap           <int>        ..FV....... Tonemap algorithm selection (from 0 to 7) (default bt2390)\n     \
            mobius          6            ..FV.......\n     bt2390          7            ..FV.......\n";
        assert!(lists_option(patched, "bt2390"));
        let own = "tonemap_opencl AVOptions:\n   tonemap           <int>        ..FV....... tonemap algorithm selection (from 0 to 6) (default none)\n     \
            mobius          6            ..FV.......\n";
        assert!(!lists_option(own, "bt2390"));
    }

    #[test]
    fn finds_libplacebo_options() {
        // FFmpeg 8's `-h filter=libplacebo`, cut down.
        let eight = "Filter libplacebo\n  Apply various GPU filters from libplacebo\n\
            libplacebo AVOptions:\n   extra_opts        <dictionary> ..FV....... Pass extra libplacebo-specific options\n   \
            shader_cache      <string>     ..FV....... Set shader cache path\n   peak_detect       <boolean>    ..FV.......\n";
        assert!(lists_option(eight, "shader_cache"));
        assert!(lists_option(eight, "peak_detect"));
        // FFmpeg 7.1's has none, though another option's help may name it.
        let seven = "libplacebo AVOptions:\n   extra_opts        <dictionary> ..FV....... unlike shader_cache\n";
        assert!(!lists_option(seven, "shader_cache"));
        assert!(!lists_option("Unknown filter 'libplacebo'.\n", "shader_cache"));
    }

    #[test]
    fn parses_encoder_list() {
        let text = " V....D libx264              libx264 H.264\n V....D h264_nvenc           NVIDIA NVENC\n ------\n A....D aac  AAC\n".to_owned();
        let names = second_column(text);
        assert!(names.contains("libx264"));
        assert!(names.contains("h264_nvenc"));
        assert!(names.contains("aac"));
    }

    #[test]
    fn vulkan_decoding_counts_only_with_frames_in_vulkan_memory() {
        // FFmpeg 9's verbose log, cut down: the decoder's line, then the graph's.
        let on_gpu = "[hevc @ 0000011743e6fe40] Chosen frame pixfmt: p010le (Vulkan ID: 1000156013)\n\
            [graph -1 input from stream 0:0 @ 000001174404c480] w:320 h:240 pixfmt:vulkan tb:1/1000 fr:25/1 sar:1/1 csp:unknown\n";
        assert!(frames_arrive_in(on_gpu, "vulkan"));
        // A GPU without Vulkan video decoding.
        let fell_back = "[hevc @ 000001c1] Failed setup for format vulkan: hwaccel initialisation returned error.\n\
            [graph -1 input from stream 0:0 @ 000001c2] w:320 h:240 pixfmt:p010le tb:1/1000 fr:25/1 sar:1/1 csp:unknown\n";
        assert!(!frames_arrive_in(fell_back, "vulkan"));
        assert!(fell_back.lines().any(hardware_decoder_failed));
        // A codec FFmpeg has no Vulkan decoder for: silently in software.
        let none = "[graph -1 input from stream 0:0 @ 000001fc] w:320 h:240 pixfmt:yuv420p tb:1/1000 fr:25/1 sar:1/1\n";
        assert!(!frames_arrive_in(none, "vulkan"));
        assert!(!none.lines().any(hardware_decoder_failed));
        // Nothing said: not counted.
        assert!(!frames_arrive_in("", "vulkan"));
    }

    #[test]
    fn parses_muxer_list() {
        let text = "File formats:\n D. = Demuxing supported\n .E = Muxing supported\n ---\n  E chromaprint     Chromaprint\n  E mp4             MP4 (MPEG-4 Part 14)\n".to_owned();
        let names = second_column(text);
        assert!(names.contains("chromaprint"));
        assert!(names.contains("mp4"));
    }

    #[test]
    fn parses_filter_list() {
        let text = " ... libplacebo        N->N       Apply various GPU filters\n TSC zscale            V->V       Apply resizing\n".to_owned();
        let names = second_column(text);
        assert!(names.contains("libplacebo"));
        assert!(names.contains("zscale"));
    }

    fn fake(encoders: Vec<EncoderInfo>, tonemappers: Vec<Tonemapper>) -> Ffmpeg {
        Ffmpeg {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            caps: Capabilities { encoders, tonemappers, ..Default::default() },
            segment_times_relative: true,
        }
    }

    fn enc(name: &str, accel: HwAccel) -> EncoderInfo {
        EncoderInfo {
            name: name.into(),
            codec: VideoCodec::H264,
            hardware: accel != HwAccel::None,
            accel,
            main10: false,
            low_power: false,
        }
    }

    fn hevc(name: &str, accel: HwAccel, main10: bool) -> EncoderInfo {
        EncoderInfo { codec: VideoCodec::Hevc, main10, ..enc(name, accel) }
    }

    #[test]
    fn encoder_selection() {
        let ff = fake(vec![enc("h264_nvenc", HwAccel::Nvenc), enc("libx264", HwAccel::None)], vec![]);
        assert_eq!(ff.pick_encoder(VideoCodec::H264, HwAccel::Auto).0.unwrap().name, "h264_nvenc");
        assert_eq!(ff.pick_encoder(VideoCodec::H264, HwAccel::None).0.unwrap().name, "libx264");
        let (enc, warning) = ff.pick_encoder(VideoCodec::H264, HwAccel::Qsv);
        assert_eq!(enc.unwrap().name, "libx264");
        assert!(warning.is_some());
    }

    #[test]
    fn main10_encoders_for_hdr() {
        // An 8-bit only hardware encoder doesn't count: x265 does.
        let ff = fake(vec![hevc("hevc_qsv", HwAccel::Qsv, false), hevc("libx265", HwAccel::None, true)], vec![]);
        assert_eq!(ff.pick_main10_encoder(HwAccel::Auto).0.unwrap().name, "libx265");
        assert_eq!(ff.pick_encoder(VideoCodec::Hevc, HwAccel::Auto).0.unwrap().name, "hevc_qsv");
        let ff = fake(vec![hevc("hevc_nvenc", HwAccel::Nvenc, true), hevc("libx265", HwAccel::None, true)], vec![]);
        assert_eq!(ff.pick_main10_encoder(HwAccel::Auto).0.unwrap().name, "hevc_nvenc");
        // Vulkan encodes only H.264: as if nothing was asked, and no warning.
        assert_eq!(ff.pick_main10_encoder(HwAccel::Vulkan), (ff.caps.encoders.first(), None));
        assert_eq!(ff.pick_main10_encoder(HwAccel::None).0.unwrap().name, "libx265");
        assert_eq!(ff.software_encoder_for(VideoCodec::Hevc, true).unwrap().name, "libx265");
        // None at all.
        let ff = fake(vec![hevc("libx265", HwAccel::None, false)], vec![]);
        assert_eq!(ff.pick_main10_encoder(HwAccel::Auto).0, None);
        assert_eq!(ff.software_encoder_for(VideoCodec::Hevc, true), None);
        assert!(ff.software_encoder_for(VideoCodec::Hevc, false).is_some());
    }

    #[test]
    fn vulkan_on_the_cpu_is_told_apart() {
        let device =
            |line: &str| format!("[AVHWDeviceContext @ 0x55] GPU listing:\n[AVHWDeviceContext @ 0x55] {line}\n");
        assert!(software_vulkan(&device("Device 0 selected: llvmpipe (LLVM 19.1.7, 256 bits) (software) (0x0)")));
        assert!(!software_vulkan(&device("Device 0 selected: NVIDIA GeForce RTX 5070 (discrete) (0x2c05)")));
        assert!(!software_vulkan(&device("Device 1 selected: Intel(R) Arc(tm) Graphics (integrated) (0x7d55)")));
        // Listed, not chosen.
        assert!(!software_vulkan(&device("    1: llvmpipe (LLVM 19.1.7, 256 bits) (software) (0x0)")));
    }

    #[test]
    fn tonemapper_selection_prefers_libplacebo_for_dv() {
        let ff = fake(vec![], vec![Tonemapper::Opencl, Tonemapper::Libplacebo, Tonemapper::Zscale]);
        assert_eq!(ff.pick_tonemapper(TonemapMode::Auto, true, false).0, Some(Tonemapper::Libplacebo));
        assert_eq!(ff.pick_tonemapper(TonemapMode::Auto, false, false).0, Some(Tonemapper::Opencl));
        let ff = fake(vec![], vec![Tonemapper::Zscale]);
        let (choice, warnings) = ff.pick_tonemapper(TonemapMode::Auto, true, false);
        assert_eq!(choice, Some(Tonemapper::Zscale));
        assert!(warnings.iter().any(|w| w.contains("Dolby Vision")));
        // OpenCL with BT.2390 (Pley's FFmpeg) reshapes profile 5 itself.
        let mut ff = fake(vec![], vec![Tonemapper::Opencl]);
        assert!(ff.pick_tonemapper(TonemapMode::Auto, true, false).1.iter().any(|w| w.contains("Dolby Vision")));
        ff.caps.opencl_bt2390 = true;
        assert_eq!(ff.pick_tonemapper(TonemapMode::Auto, true, false), (Some(Tonemapper::Opencl), vec![]));
    }

    #[test]
    fn a_vaapi_encoder_gets_frames_only_from_tone_mappers_that_reach_it() {
        let mut ff = fake(vec![], vec![Tonemapper::Libplacebo, Tonemapper::Opencl, Tonemapper::Zscale]);
        // AMD: libplacebo's Vulkan device made from VAAPI's works, OpenCL's
        // can't be made from it.
        ff.caps.vaapi_tonemappers = vec![Tonemapper::Libplacebo, Tonemapper::Zscale];
        assert_eq!(ff.pick_tonemapper(TonemapMode::Auto, false, true).0, Some(Tonemapper::Libplacebo));
        let (choice, warnings) = ff.pick_tonemapper(TonemapMode::Opencl, false, true);
        assert_eq!(choice, Some(Tonemapper::Libplacebo));
        assert!(warnings.iter().any(|w| w.contains("VAAPI")), "{warnings:?}");
        // Other encoders take any.
        assert_eq!(ff.pick_tonemapper(TonemapMode::Opencl, false, false), (Some(Tonemapper::Opencl), vec![]));

        // Neither reaches it: zimg, on the CPU, always does.
        ff.caps.vaapi_tonemappers = vec![Tonemapper::Zscale];
        assert_eq!(ff.pick_tonemapper(TonemapMode::Auto, false, true).0, Some(Tonemapper::Zscale));
        let (choice, warnings) = ff.pick_tonemapper(TonemapMode::Auto, true, true);
        assert_eq!(choice, Some(Tonemapper::Zscale));
        assert!(warnings.iter().any(|w| w.contains("Dolby Vision")), "{warnings:?}");
        ff.caps.tonemappers = vec![Tonemapper::Opencl];
        let (choice, warnings) = ff.pick_tonemapper(TonemapMode::Auto, false, true);
        assert_eq!(choice, None);
        assert!(warnings.iter().any(|w| w.contains("no tone mapper")), "{warnings:?}");
    }
}
