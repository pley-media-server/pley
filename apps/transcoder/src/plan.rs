// SPDX-License-Identifier: AGPL-3.0-only
//! Turns a [`JobRequest`] plus [`MediaInfo`] into a concrete plan (segment
//! layout, encoder, filters, tone mapping) and renders ffmpeg command lines for
//! any window of segments.
//!
//! Segment alignment: every process is started with `-ss <boundary>` and
//! `-output_ts_offset <boundary>` (a remux a little past its boundary
//! keyframe, see [`remux_seek_points`]), so segment timestamps stay on the absolute
//! timeline no matter where ffmpeg was (re)started. Split points are passed via
//! the segment muxer's `-segment_times`; when transcoding, keyframes are forced
//! on the same grid, when remuxing the grid *is* the source keyframes.

use crate::adaptive::AUTO;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pley_transcoder_api::{
    AudioCodec, AudioMode, AudioPlan, DolbyVisionFel, DolbyVisionLayer, DolbyVisionMode, DynamicRange, EncoderInfo,
    FileContainer, HwAccel, JobRequest, MediaInfo, MediaStream, SegmentFormat, StreamKind, TonemapMode, TonemapQuality,
    TonemapSpeed, Tonemapper, VideoCodec, VideoMode, VideoPlan, VideoRequest, decode_key, default_video_kbps,
    is_text_subtitle_codec,
};

use crate::devices::{qsv_device, vaapi_device};
use crate::ffmpeg::Ffmpeg;
use crate::fmp4::DoviConfig;
use crate::share::Share;
use crate::tsedit::Edits;
use crate::util::{MEDIA_INPUT_OPTIONS, input_arg, secs};

pub const DEFAULT_SEGMENT_SECONDS: f64 = 4.0;
/// Most segments a job may have. Its layout, segment states and playlist grow
/// with the count, so a file claiming an absurd length (damaged or crafted)
/// must not turn into gigabytes of memory.
pub const MAX_SEGMENTS: i64 = 200_000;
/// Longest media the transcoder believes: 200 000 segments of 4 s, about 9
/// days. Anything longer comes from a damaged or crafted file.
pub const MAX_DURATION_MS: i64 = MAX_SEGMENTS * 4_000;
/// Maximum segments per ffmpeg process (keeps command lines short on Windows).
pub const PROCESS_WINDOW: u32 = 1200;
/// Throttling: read at real time after a 120 s burst, so a process stays
/// as far ahead of the player as the burst took it.
pub const READRATE: f64 = 1.0;
pub const READRATE_BURST_MS: i64 = 120_000;
/// How fast a throttled read that fell behind (a slow disk or share)
/// catches up, where FFmpeg can.
pub const READRATE_CATCHUP: f64 = 2.0;
/// How a process making fMP4 segments writes its output: one fragmented
/// MP4, a fragment at each video keyframe, which the transcoder cuts into
/// segments (see [`crate::fmp4`]). Every fragment's decode time is absolute
/// (`default_base_moof`, `frag_discont`), so processes started anywhere
/// write the same `ftyp` and `moov`; `delay_moov` waits for the first
/// packets (AC-3 needs them), and `skip_trailer` leaves out the index at
/// the end.
pub const FMP4_MOVFLAGS: &str = "+frag_keyframe+empty_moov+default_base_moof+delay_moov+frag_discont+skip_trailer";

/// CPU tone mapping chain (zimg). Expects HDR input, outputs 8-bit BT.709.
pub const ZSCALE_TONEMAP: &str = "zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p";

/// [`ZSCALE_TONEMAP`] at `w×h`: the first zscale scales as it makes the
/// picture linear, so the tone mapping, in 32-bit floats for each of three
/// colours, works on the output's pixels rather than the source's (4K to
/// 1080p: a quarter of them).
pub fn zscale_tonemap(w: i64, h: i64) -> String {
    ZSCALE_TONEMAP.replacen("zscale=t=linear", &format!("zscale=w={w}:h={h}:t=linear"), 1)
}
/// libplacebo tone maps by the stream's own metadata (HDR10's mastering
/// display and content light levels, Dolby Vision's and HDR10+'s per scene)
/// rather than by the peak it measures in the frames it has seen. Each
/// FFmpeg process starts measuring afresh, so with detection on, the first
/// frames after a seek or a process window's start came out brighter or
/// darker than the same frames made by a process already running, a jump at
/// the seam between their segments (measured: 5 % in a face's brightness on
/// a test clip whose highlight changes). Without it every process makes the
/// same frames, bit for bit, whatever it started with.
pub const PLACEBO_STATIC_PEAK: &str = "peak_detect=0";

/// libplacebo's options at each [`TonemapQuality`], besides its defaults,
/// for a picture made `out` (width, height) from `src`, SDR or not (`sdr`).
/// On a Ryzen 5 5600G's Radeon (Vega 7), 4K HDR10 to 4K SDR into AMF ran
/// at 23 frames a second at `High`, 34 at `Balanced` and 43 at `Fast`:
/// contrast recovery and the polar chroma upscaler take a third of the
/// time between them, dithering, perceptual gamut mapping and the
/// downscaler a few percent each. Side by side, `Balanced` shows a little
/// less local contrast in skin and nothing else.
///
/// What the rest costs is memory: an integrated GPU's is the computer's,
/// which libplacebo's pictures between its passes (16 bits a channel)
/// fill. At `Fast`, where the picture shrinks to no less than half each
/// way, its bilinear downscaler samples the source as it is (`skip_aa`;
/// at exactly half, an average of 2×2, chroma's included), which leaves
/// one picture between passes: the output's, before it is split into its
/// planes. Already SDR, that one is kept in 8 bits as the output is. Two
/// 4K to 4K transcodes at once went from 21.7 to 25.0 frames a second
/// each, 4K to 1080p from 24.7 to 38.4.
pub fn placebo_quality_options(quality: TonemapQuality, src: (i64, i64), out: (i64, i64), sdr: bool) -> String {
    let mut options = match quality {
        TonemapQuality::High => "",
        TonemapQuality::Balanced => ":upscaler=bilinear:contrast_recovery=0",
        TonemapQuality::Fast => {
            ":upscaler=bilinear:downscaler=bilinear:dithering=none:gamut_mode=clip:contrast_recovery=0"
        }
    }
    .to_owned();
    if quality == TonemapQuality::Fast && out.0 * 2 >= src.0 && out.1 * 2 >= src.1 {
        options.push_str(":skip_aa=1");
        if sdr {
            options.push_str(":extra_opts=force_low_bit_depth_fbos=yes");
        }
    }
    options
}

/// How much faster than a stream's frames come the tone mapping alone must
/// measure ([`TonemapSpeed`]) for a quality to be taken: decoding, copies
/// between memory and the GPU, and encoding left 0.6 of the measured speed
/// end to end at every quality (an integrated Radeon), and 2× keeps 1.2×
/// real time.
const TONEMAP_HEADROOM: f64 = 2.0;

/// What a quality must measure ([`TonemapSpeed`]) to keep a transcode of
/// `pixels_in` to `pixels_out` at `fps` ahead, alone on the GPU. With
/// others, it must measure what they all need together.
pub fn tonemap_need(pixels_in: u64, pixels_out: u64, fps: f64) -> f64 {
    // libplacebo reads and upsamples the source and renders the output:
    // 4K to 1080p takes 0.7 of the measured 4K to 4K's time.
    let work = (0.6 * pixels_in as f64 + 0.4 * pixels_out as f64) / (3840.0 * 2160.0);
    fps * TONEMAP_HEADROOM * work
}

/// The best quality fast enough for libplacebo transcodes that `need` this
/// together ([`tonemap_need`]); the fastest that works when none is.
pub fn tonemap_quality(speed: &TonemapSpeed, need: f64) -> TonemapQuality {
    let working: Vec<TonemapQuality> = [TonemapQuality::High, TonemapQuality::Balanced, TonemapQuality::Fast]
        .into_iter()
        .filter(|q| speed.at(*q) > 0.0)
        .collect();
    working.iter().copied().find(|q| speed.at(*q) >= need).or(working.last().copied()).unwrap_or_default()
}

/// Whether no quality is fast enough for [`tonemap_quality`]: the GPU, not
/// the quality, holds the transcodes back then. Not known where nothing
/// was measured.
pub fn tonemap_falls_behind(speed: &TonemapSpeed, need: f64) -> bool {
    let fastest = [TonemapQuality::High, TonemapQuality::Balanced, TonemapQuality::Fast]
        .into_iter()
        .map(|q| speed.at(q))
        .fold(0.0, f64::max);
    fastest > 0.0 && fastest < need
}

/// [`tonemap_need`] of `src` made `w×h`. The source's pixels count where
/// Quick Sync's scaler brings libplacebo the frames at the output's size
/// too: Intel's decoder and scaler work on the GPU libplacebo does, and
/// with them a 4K60 HDR10 film made 1080p went at 0.45 of what libplacebo
/// alone measured at its size on an Iris Xe (99 frames a second at `high`).
fn need_of(src: &MediaStream, w: i64, h: i64) -> f64 {
    let output = w.max(1) as u64 * h.max(1) as u64;
    let source = src.width.unwrap_or(1920).max(1) as u64 * src.height.unwrap_or(1080).max(1) as u64;
    let fps = src.frame_rate.filter(|f| f.is_finite() && *f > 0.0).unwrap_or(24.0).min(120.0);
    tonemap_need(source, output, fps)
}

/// [`tonemap_quality`] for `src` made `w×h` ([`need_of`]), beside
/// transcodes needing `others`; `High` where nothing was measured.
fn quality_for(speed: Option<&TonemapSpeed>, src: &MediaStream, (w, h): (i64, i64), others: f64) -> TonemapQuality {
    let Some(speed) = speed else { return TonemapQuality::High };
    tonemap_quality(speed, need_of(src, w, h) + others)
}

/// The Direct3D 11 device AMF encodes on: the AMD GPU, whichever adapter
/// it is.
pub const AMF_D3D11_DEVICE: &str = "d3d11va=dx:,vendor_id=0x1002";
/// Quick Sync's tone mapping of HDR10 at `w×h` ([`Tonemapper::QuickSync`]).
/// Each step at the output's size, its scaler's first: a pass that tone
/// maps, or lifts the brightness, does it at the size it is given (a 4K60
/// film made 1080p went at 131 frames a second tone mapped at 4K, 218 at
/// 1080p on an Iris Xe; 166 lifted at 4K, 223 at 1080p). Brightness 16 up
/// before the curve: Intel's alone made two HDR10 films a third darker
/// than libplacebo's (mean SDR luma 53 and 78 against 80 and 109); lifted,
/// as bright, with shadows no lighter.
///
/// On Linux the curve is VAAPI's (`tonemap_vaapi`, Intel's driver's), the
/// frames mapped to VAAPI's device and back rather than copied, as Jellyfin
/// does there: VAAPI's tone maps on older Intel graphics than Quick Sync's.
pub fn qsv_tonemap(w: i64, h: i64) -> String {
    if QSV_ON_VAAPI {
        format!(
            "vpp_qsv=w={w}:h={h}:format=p010le,vpp_qsv=procamp=1:brightness=16,hwmap=derive_device=vaapi,\
             format=vaapi,tonemap_vaapi=format=nv12:p=bt709:t=bt709:m=bt709,hwmap=derive_device=qsv,format=qsv"
        )
    } else {
        format!(
            "vpp_qsv=w={w}:h={h}:format=p010le,vpp_qsv=procamp=1:brightness=16,vpp_qsv=tonemap=1:format=nv12,\
             setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709"
        )
    }
}

const OPENCL_TONEMAP: &str = "format=p010,hwupload,tonemap_opencl=tonemap=hable:desat=0:t=bt709:m=bt709:p=bt709:format=nv12,hwdownload,format=nv12";

/// The last filter before a VAAPI encoder, which takes frames in VAAPI's
/// memory only. A plain `hwupload` uploads to the filters' device, which a
/// tone mapper makes Vulkan's or OpenCL's: it uploads to VAAPI's device
/// instead, the one the filters' device was made from
/// ([`vaapi_filter_devices`]), or the filters' own when it is VAAPI's.
pub const VAAPI_UPLOAD: &str = "hwupload=derive_device=vaapi";

/// The devices for filters on `device` (`vulkan=vk`, `opencl=ocl`) whose
/// frames go on to a VAAPI encoder: VAAPI's on `node` first, then the
/// filters' made from it (`vulkan=vk@va`), so that [`VAAPI_UPLOAD`] finds
/// VAAPI's again. Vulkan's is then the same GPU where VAAPI says which
/// it is. OpenCL's can only be made from VAAPI's by Intel's OpenCL.
pub fn vaapi_filter_devices(device: &str, node: &str) -> [String; 4] {
    ["-init_hw_device".into(), format!("vaapi=va:{node}"), "-init_hw_device".into(), format!("{device}@va")]
}

/// A `setparams` filter giving HDR frames the colour properties they lack
/// (PQ, BT.2020), for the zimg and OpenCL tone mappers, which refuse to
/// convert from unknown ones ("no path between colorspaces"). Dolby Vision
/// profile 5 frames come out of the decoder untagged (their IPTPQc2 colours
/// are only defined by the RPU, which libplacebo applies), and so do HDR
/// streams that tag only some properties. For profile 5 the colours are then
/// off, but there is a picture.
pub fn untagged_hdr_fix(stream: &MediaStream) -> Option<String> {
    let unknown =
        |v: &Option<String>| v.as_deref().is_none_or(|v| matches!(v, "" | "unknown" | "unspecified" | "reserved"));
    let params: Vec<&str> = [
        (&stream.color_transfer, "color_trc=smpte2084"),
        (&stream.color_primaries, "color_primaries=bt2020"),
        (&stream.color_space, "colorspace=bt2020nc"),
    ]
    .into_iter()
    .filter(|(value, _)| unknown(value))
    .map(|(_, param)| param)
    .collect();
    (!params.is_empty()).then(|| format!("setparams={}", params.join(":")))
}

/// Where frames live between decode and encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pipeline {
    /// CPU filters. Decoding may use `-hwaccel` with frames copied to system
    /// memory, scaled by its GPU first where it can ([`VideoOut::scales_on_gpu`]).
    Software,
    /// Vulkan decode → libplacebo tone mapping → `h264_vulkan` (`encode_on_gpu`)
    /// or a single download into the selected encoder.
    Vulkan { encode_on_gpu: bool },
    /// Decode, scale and encode on one GPU API without any copies.
    Gpu(HwAccel),
    /// Decode into Direct3D 11 → `scale_d3d11` → AMF (`encode_on_gpu`), or
    /// a single download into the software encoder standing in for it
    /// (Windows, AMD).
    D3d11 { encode_on_gpu: bool },
}

impl Pipeline {
    /// The pipeline to use when the planned (hardware) encoder is replaced by a
    /// software one because hardware sessions ran out.
    fn for_software_encoder(self) -> Pipeline {
        match self {
            Pipeline::Vulkan { .. } => Pipeline::Vulkan { encode_on_gpu: false },
            // Tone mapped as the hardware encoder's frames are.
            Pipeline::D3d11 { .. } => Pipeline::D3d11 { encode_on_gpu: false },
            _ => Pipeline::Software,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VideoOut {
    pub stream_index: i64,
    pub copy: bool,
    pub encoder: Option<EncoderInfo>,
    pub codec: VideoCodec,
    pub src_width: i64,
    pub src_height: i64,
    pub width: i64,
    pub height: i64,
    pub bitrate_kbps: u32,
    pub tonemap: Option<Tonemapper>,
    /// What libplacebo does of its work (see [`tonemap_quality`]).
    pub tonemap_quality: TonemapQuality,
    /// Tags the tone mapper's input when the source leaves colours unknown
    /// ([`untagged_hdr_fix`]): for zscale, OpenCL and Quick Sync, which a
    /// libplacebo plan can move to ([`Plan::with_share`]). libplacebo itself
    /// takes the frames untagged.
    pub tonemap_input: Option<String>,
    /// libplacebo was asked for (`tonemap=libplacebo`): it tone maps however
    /// busy the GPU is, Quick Sync's scaler never in its place.
    pub libplacebo_asked: bool,
    /// Quick Sync could decode and scale this video for its HDR10 base layer
    /// alone, its Dolby Vision or HDR10+ metadata unread, where FFmpeg's own
    /// decoder brings libplacebo the frames with it: what
    /// [`Plan::with_share`] does where that decoder's GPU decoding failed
    /// (the CPU would decode in its place), or the computer falls behind.
    pub qsv_base_layer: bool,
    /// Dolby Vision profile 7 with a full enhancement layer, made from its
    /// HDR10 base layer alone ([`DolbyVisionFel::BaseLayer`]): HDR10 to
    /// every choice of decoder and tone mapper, and libplacebo leaves its
    /// Dolby Vision metadata unread.
    pub base_layer_only: bool,
    /// Experimental: Dolby Vision profile 7's full picture, rebuilt from its
    /// base layer and its full enhancement layer by FFmpeg's `dovi_compose`
    /// ([`TranscodeOptions::dolby_vision_fel_full`]), the file opened a second
    /// time for that layer ([`Plan::fel_input`]). Decoded on the CPU, and
    /// HDR10 after it.
    pub fel_compose: bool,
    /// FFmpeg uploads the frames to the AMD GPU for AMF ([`AMF_D3D11_DEVICE`]),
    /// which copies them from memory itself several times slower.
    pub amf_upload: bool,
    /// The colour primaries `h264_amf`'s stream is made to say (its code in
    /// H.264's VUI), which it leaves unsaid: players then guess, and some
    /// take BT.2020 for 4K.
    pub vui_primaries: Option<u8>,
    pub hw_decode: Option<String>,
    /// The GPU that decodes the frames scales them too, before they come to
    /// memory for the CPU's filters or encoder ([`VideoOut::scales_on_gpu`]):
    /// NVIDIA's (`scale_cuda`), VAAPI's (`scale_vaapi`) or Quick Sync's
    /// (`vpp_qsv`), where its decode and scale pipeline passed the start-up
    /// test for the codec.
    pub gpu_scale: Option<HwAccel>,
    pub deinterlace: bool,
    pub preset: String,
    pub pipeline: Pipeline,
    /// What a remux of a Dolby Vision HEVC stream does with it (`None`:
    /// transcoded, or no Dolby Vision).
    pub dolby_vision: Option<DolbyVisionMode>,
    /// A remux of HEVC leaves out its HDR10+ metadata.
    pub strip_hdr10plus: bool,
    /// The video comes out as HEVC, copied or encoded: tagged `hvc1` in MP4,
    /// the only way Apple's players take it (FFmpeg writes `hev1`).
    pub hevc: bool,
    /// A copy of Dolby Vision profile 5 that is named so: tagged `dvh1` in
    /// MP4 instead, as its `dvh1.05` codec says (it has no base layer
    /// anything else decodes).
    pub dvh1: bool,
    /// A transcode that keeps HDR: HEVC Main 10 in HDR10 or HLG (see
    /// [`hdr_output`]); `None` when it comes out SDR.
    pub hdr: Option<HdrOut>,
}

/// What a transcode that keeps HDR comes out as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HdrOut {
    /// [`DynamicRange::Hdr10`] or [`DynamicRange::Hlg`].
    pub range: DynamicRange,
    /// libplacebo converts the video to it (HLG or Dolby Vision profile 5
    /// to HDR10, say); otherwise the decoded frames are in it already,
    /// their HDR10 metadata going to the encoder with them.
    pub convert: bool,
}

impl VideoOut {
    /// The GPU that scales the frames it decoded, when `pipeline` brings them
    /// to memory (the CPU's filters, a software encoder) and their size
    /// changes: only the output's pixels are copied and scaled no more on the
    /// CPU (4K to 1080p, a quarter of them). Quick Sync's frames whatever
    /// their size: its decoder keeps them on the GPU, so they come to memory
    /// through its scaler.
    pub fn scales_on_gpu(&self, pipeline: Pipeline) -> Option<HwAccel> {
        let resized = self.width != self.src_width || self.height != self.src_height;
        self.gpu_scale.filter(|accel| pipeline == Pipeline::Software && (resized || *accel == HwAccel::Qsv))
    }
}

impl HdrOut {
    /// FFmpeg's name for the output's transfer function.
    fn transfer(self) -> &'static str {
        if self.range == DynamicRange::Hlg { "arib-std-b67" } else { "smpte2084" }
    }
}

/// The 10-bit pixel format an encoder takes: P010 for hardware encoders,
/// planar for x265.
pub fn ten_bit_format(accel: HwAccel) -> &'static str {
    match accel {
        HwAccel::None | HwAccel::Auto => "yuv420p10le",
        _ => "p010le",
    }
}

/// Whether a stream's base picture is HLG rather than PQ: HLG itself, and
/// Dolby Vision profile 8.4.
fn hlg_source(src: &MediaStream) -> bool {
    match src.dynamic_range() {
        DynamicRange::Hlg => true,
        DynamicRange::DolbyVision => src.dv_bl_compat == Some(4),
        _ => false,
    }
}

/// What a transcode of `src` keeps of its HDR, as `vreq` asks (see
/// [`pley_transcoder_api::VideoRequest::dynamic_range`]), with the encoder
/// for it; `None` with a warning when it can't be kept, and quietly for SDR
/// video and SDR requests.
///
/// The HDR10 metadata (mastering display, content light levels) goes along
/// with the decoded frames, through scaling and libplacebo, to the encoders
/// (NVENC and x265 write it). Dolby Vision metadata doesn't: NVENC never
/// writes it and x265 is told not to, so Dolby Vision comes out as its
/// HDR10 base layer, or converted by libplacebo when it has none (profile 5).
fn hdr_output<'a>(
    vreq: &VideoRequest,
    src: &MediaStream,
    burn: bool,
    ff: &'a Ffmpeg,
    accel: HwAccel,
    warnings: &mut Vec<String>,
) -> Option<(HdrOut, &'a EncoderInfo)> {
    let range = src.dynamic_range();
    let wanted = vreq.dynamic_range.dynamic_range();
    // Dolby Vision on an SDR base layer (profile 8.2) is SDR underneath.
    let sdr_base = range == DynamicRange::DolbyVision && src.dv_bl_compat == Some(2);
    if !range.is_hdr() || !wanted.is_hdr() || sdr_base {
        return None;
    }
    let mut no = |why: &str| -> Option<(HdrOut, &'a EncoderInfo)> {
        warnings.push(format!("{why}: {} is tone-mapped to SDR", src.dynamic_range_label()));
        None
    };
    if vreq.codec != VideoCodec::Hevc {
        return no("HDR is kept in HEVC only");
    }
    if burn {
        return no("subtitles drawn onto the video are SDR");
    }
    let convert = src.is_dolby_vision_only() || hlg_source(src) != (wanted == DynamicRange::Hlg);
    if convert && !ff.caps.tonemappers.contains(&Tonemapper::Libplacebo) {
        return no("converting to another HDR format needs libplacebo");
    }
    let Some(encoder) = ff.pick_main10_encoder(accel).0 else {
        return no("no 10-bit HEVC encoder works");
    };
    if convert && encoder.accel == HwAccel::Vaapi && !ff.caps.vaapi_tonemappers.contains(&Tonemapper::Libplacebo) {
        return no("libplacebo's frames don't reach the VAAPI encoder");
    }
    Some((HdrOut { range: wanted, convert }, encoder))
}

/// The encoder used by fully-GPU Vulkan pipelines.
pub fn vulkan_encoder() -> EncoderInfo {
    EncoderInfo {
        name: "h264_vulkan".into(),
        codec: VideoCodec::H264,
        hardware: true,
        accel: HwAccel::Vulkan,
        main10: false,
        low_power: false,
    }
}

#[derive(Debug, Clone)]
pub struct AudioOut {
    pub stream_index: i64,
    pub copy: bool,
    /// The codec when converting.
    pub codec: AudioCodec,
    pub channels: u32,
    pub bitrate_kbps: u32,
    pub resample: bool,
    /// A decoder option that decodes only what the conversion keeps (see
    /// [`partial_decode`]), as its name and value.
    pub partial_decode: Option<(&'static str, &'static str)>,
    /// The filter that makes surround stereo ([`stereo_downmix`]).
    pub downmix: Option<&'static str>,
}

/// 5.1 (and 5.0) to stereo: FFmpeg's own matrix (`-ac 2`: each front
/// whole, the surrounds at -3 dB, no LFE, not normalized for AAC's and
/// AC-3's float samples) with the centre, where dialogue is, at 0 dB
/// rather than -3 dB, and a limiter at -2 dBFS (-1 dB true peak once encoded) for the peaks the sum would
/// push past full scale, which `-ac 2` lets clip. The layout is made 5.1
/// first (a 5.0 one gains a silent LFE, a broadcast switching to stereo a
/// silent centre and surrounds), so the matrix always finds its channels.
/// The limiter's look-ahead is taken back out (`latency`), keeping the
/// sound in time with the picture.
pub const DOWNMIX_51: &str = "aformat=channel_layouts=5.1(side)|5.1,\
     pan=stereo|c0=c0+c2+0.707*c4|c1=c1+c2+0.707*c5,alimiter=limit=0.794:level=false:latency=1";
/// [`DOWNMIX_51`] for 7.1: side and back surrounds at -3 dB each, as `-ac 2` has them.
pub const DOWNMIX_71: &str = "aformat=channel_layouts=7.1,\
     pan=stereo|c0=c0+c2+0.707*c4+0.707*c6|c1=c1+c2+0.707*c5+0.707*c7,alimiter=limit=0.794:level=false:latency=1";

/// The filter that makes `src` stereo, when it is 5.0 to 7.1 and converted
/// to 2 channels; `None` leaves it to `-ac 2`: fewer channels, layouts
/// other than 5.0, 5.1 and 7.1, and TrueHD whose decoder makes stereo
/// itself ([`partial_decode`]). A DTS-HD MA 7.1 decoded from its core
/// (`core_only`), 5.1, is made 7.1 first like any other.
fn stereo_downmix(src: &MediaStream, channels: u32, partial: Option<(&str, &str)>) -> Option<&'static str> {
    if channels != 2 || partial.is_some_and(|(option, _)| option == "downmix") {
        return None;
    }
    let layout = src.channel_layout.as_deref().unwrap_or("");
    match (src.channels.unwrap_or(0), layout.split('(').next().unwrap_or("")) {
        (5 | 6, "5.0" | "5.1") => Some(DOWNMIX_51),
        (8, "7.1") if layout != "7.1(wide)" => Some(DOWNMIX_71),
        _ => None,
    }
}

/// What of a lossless track a conversion to `channels` needs decoded: a
/// DTS-HD Master Audio track's lossy 5.1 core, which the lossy result can't
/// tell from the whole; of TrueHD, the substream mastered with that many
/// channels (Blu-rays carry a 2.0 and a 5.1 one inside the 7.1), which the
/// decoder falls back from to the whole where it's missing.
fn partial_decode(src: &MediaStream, channels: u32) -> Option<(&'static str, &'static str)> {
    let src_channels = src.channels.unwrap_or(2);
    match src.codec.as_str() {
        "dts" if channels <= 6 && src.profile.as_deref().is_some_and(|p| p.contains("MA")) => Some(("core_only", "1")),
        "truehd" if channels <= 2 && src_channels > 2 => Some(("downmix", "stereo")),
        "truehd" if channels <= 6 && src_channels > 6 => Some(("downmix", "5.1")),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum Burn {
    /// Bitmap subtitle stream of the input (PGS, VobSub, DVB).
    Image { stream_index: i64, label: String },
    /// Bitmap subtitle file (`.sup`), added as a second input.
    ImageFile { path: PathBuf, label: String },
    /// Text subtitles rendered by libass from `file_name` inside the job dir,
    /// with the fonts attached to the file in `fonts_dir` (set once they
    /// are read, see [`Plan::use_fonts`]).
    Text { source: TextSource, file_name: String, fonts_dir: Option<PathBuf>, label: String },
}

#[derive(Debug, Clone)]
pub enum TextSource {
    Embedded(i64),
    File(PathBuf),
}

impl Burn {
    pub fn label(&self) -> &str {
        match self {
            Burn::Image { label, .. } | Burn::ImageFile { label, .. } | Burn::Text { label, .. } => label,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub duration_ms: i64,
    /// Segment start times in ms; `boundaries[segment_count]` is the duration.
    pub boundaries: Vec<i64>,
    /// Absolute split times passed to the segment muxer (slightly before each
    /// boundary to absorb timestamp jitter). `split_at[k]` belongs to boundary k.
    pub split_at: Vec<i64>,
    /// Where a process starting at boundary k seeks (`-ss`): the boundary
    /// itself when transcoding, a little past the boundary keyframe when
    /// remuxing (see [`remux_seek_points`]).
    pub seek_at: Vec<i64>,
    pub segment_seconds: f64,
    pub video: Option<VideoOut>,
    pub audio: Option<AudioOut>,
    pub burn: Option<Burn>,
    pub throttle: bool,
    /// A throttled read catches up faster when it falls behind ([`READRATE_CATCHUP`]).
    pub catchup: bool,
    pub warnings: Vec<String>,
    /// How its segments are packaged (see [`Plan::build`]).
    pub segment_format: SegmentFormat,
}

impl Plan {
    pub fn segment_count(&self) -> u32 {
        (self.boundaries.len() - 1) as u32
    }

    /// Burned-in text subtitles are drawn with the fonts in `dir` (those
    /// attached to the file) besides the system's.
    pub fn use_fonts(&mut self, dir: Option<PathBuf>) {
        if let Some(Burn::Text { fonts_dir, .. }) = &mut self.burn {
            *fonts_dir = dir;
        }
    }

    pub fn segment_for(&self, position_ms: i64) -> u32 {
        let idx = self.boundaries.partition_point(|&b| b <= position_ms);
        (idx.saturating_sub(1) as u32).min(self.segment_count().saturating_sub(1))
    }

    pub fn build(req: &JobRequest, media: &MediaInfo, ff: &Ffmpeg, keyframes: Option<&[i64]>) -> Result<Plan, String> {
        let duration_ms = media.duration_ms.filter(|d| *d > 0).ok_or("media duration is unknown")?;
        if duration_ms > MAX_DURATION_MS {
            return Err(format!("a media duration of {} days is not plausible", duration_ms / 86_400_000));
        }
        let requested =
            req.segment_seconds.filter(|s| s.is_finite()).unwrap_or(DEFAULT_SEGMENT_SECONDS).clamp(1.0, 10.0);
        // Very long files get longer segments rather than more of them.
        let target_ms = ((requested * 1000.0).round() as i64).max((duration_ms + MAX_SEGMENTS - 1) / MAX_SEGMENTS);
        let segment_seconds = target_ms as f64 / 1000.0;
        let mut warnings = Vec::new();

        // ---- subtitle burn-in ----
        let burn = match &req.subtitle {
            None => None,
            Some(sub) => Some(resolve_burn(sub, media, ff)?),
        };

        // ---- video ----
        let video = match (&req.video, media_video(media, req)) {
            (Some(vreq), Some(src)) => {
                // A picture shown turned (a phone's portrait video, stored
                // as landscape frames) is planned at the size it is shown:
                // FFmpeg turns the frames that come to memory before the
                // filters (`-autorotate`, its default), and only those.
                let turned = src.rotation_degrees() != 0;
                let (coded_width, coded_height) = (src.width.unwrap_or(1920), src.height.unwrap_or(1080));
                let (src_width, src_height) =
                    if src.shown_sideways() { (coded_height, coded_width) } else { (coded_width, coded_height) };
                let want_copy = vreq.mode == VideoMode::Copy && burn.is_none();
                let copy = want_copy && !turned && keyframes.is_some_and(|k| !k.is_empty());
                if want_copy && turned {
                    // MPEG-TS has no display matrix: copied, it plays on its side.
                    warnings.push(
                        "the picture is turned, which HLS segments can't say: transcoding instead of remuxing".into(),
                    );
                } else if want_copy && !copy {
                    warnings.push("keyframe index not ready yet: transcoding instead of remuxing".into());
                }
                if copy {
                    let dv_hevc = src.dynamic_range() == DynamicRange::DolbyVision && src.codec == "hevc";
                    let dolby_vision = dv_hevc.then_some(match vreq.dolby_vision {
                        DolbyVisionMode::To81 if src.dv_profile != Some(7) => DolbyVisionMode::Signal,
                        mode => mode,
                    });
                    Some(VideoOut {
                        stream_index: src.index,
                        copy: true,
                        encoder: None,
                        codec: vreq.codec,
                        src_width,
                        src_height,
                        width: src_width,
                        height: src_height,
                        bitrate_kbps: 0,
                        tonemap: None,
                        tonemap_quality: TonemapQuality::High,
                        tonemap_input: None,
                        libplacebo_asked: false,
                        qsv_base_layer: false,
                        base_layer_only: false,
                        fel_compose: false,
                        amf_upload: false,
                        vui_primaries: None,
                        hw_decode: None,
                        gpu_scale: None,
                        deinterlace: false,
                        preset: String::new(),
                        pipeline: Pipeline::Software,
                        dolby_vision,
                        strip_hdr10plus: vreq.strip_hdr10plus && src.codec == "hevc",
                        hevc: src.codec == "hevc",
                        dvh1: dolby_vision == Some(DolbyVisionMode::Signal) && src.dv_profile == Some(5),
                        hdr: None,
                    })
                } else {
                    let (width, height) = fit(src_width, src_height, vreq.max_width, vreq.max_height);
                    // The bitrate asked for, else the one its size gets.
                    let bitrate =
                        vreq.max_bitrate_kbps.map_or_else(|| default_video_kbps(width, height), |max| max.max(300));
                    let accel = req.options.hwaccel;
                    let kept = hdr_output(vreq, src, burn.is_some(), ff, accel, &mut warnings);
                    let hdr = kept.map(|(hdr, _)| hdr);
                    let mut encoder = match kept {
                        Some((_, encoder)) => encoder.clone(),
                        None => {
                            let (encoder, warning) = ff.pick_encoder(vreq.codec, accel);
                            warnings.extend(warning);
                            encoder.cloned().ok_or("no working video encoder found (is ffmpeg installed?)")?
                        }
                    };
                    let range = src.dynamic_range();
                    // Profile 7's full enhancement layer, which no FFmpeg
                    // reads: where its base layer alone is asked for (the
                    // default), the video is HDR10 to every choice below,
                    // its Dolby Vision metadata left unread. That metadata's
                    // brightness is of the picture with the layer.
                    let fel = range == DynamicRange::DolbyVision
                        && src.dv_profile == Some(7)
                        && src.dv_el == Some(DolbyVisionLayer::Fel);
                    // Experimental: the full picture rebuilt from both layers,
                    // where FFmpeg can (Pley's own). `dovi_compose` takes
                    // frames from memory, so the CPU decodes both layers.
                    let fel_compose = fel && req.options.dolby_vision_fel_full && ff.caps.dolby_vision_fel_full;
                    let fel_base = fel && (fel_compose || req.options.dolby_vision_fel == DolbyVisionFel::BaseLayer);
                    let hardware_decoding = req.options.hardware_decoding && !fel_compose;
                    let tonemap = match hdr {
                        // libplacebo converts between HDR formats; nothing
                        // is needed for frames in the output's already.
                        Some(hdr) => hdr.convert.then_some(Tonemapper::Libplacebo),
                        None if range.is_hdr() => {
                            let (mode, dv5) = (req.options.tonemap, src.is_dolby_vision_only());
                            let vaapi = encoder.accel == HwAccel::Vaapi;
                            let (mut choice, mut w) = ff.pick_tonemapper(mode, dv5, vaapi);
                            // A VAAPI encoder takes the frames of some tone
                            // mappers only. Where that would lose the picture
                            // (no tone mapping, or Dolby Vision 5's colours),
                            // the software encoder takes them instead.
                            let (any, any_w) = ff.pick_tonemapper(mode, dv5, false);
                            let lost = (choice.is_none() && any.is_some())
                                || (dv5 && any == Some(Tonemapper::Libplacebo) && choice != any);
                            if let Some(software) = ff.software_encoder_for(vreq.codec, false).filter(|_| vaapi && lost)
                            {
                                let mapper = any.map_or("", |m| m.as_str());
                                warnings.push(format!(
                                    "{mapper}'s frames don't reach the VAAPI encoder: encoding with {}",
                                    software.name
                                ));
                                encoder = software.clone();
                                (choice, w) = (any, any_w);
                            }
                            warnings.extend(w);
                            choice
                        }
                        None => None,
                    };
                    let vaapi = encoder.accel == HwAccel::Vaapi;
                    let deinterlace = src.interlaced == Some(true);
                    let key = decode_key(&src.codec, src.bit_depth, src.pix_fmt.as_deref());
                    let tonemap_input = match tonemap {
                        Some(Tonemapper::Zscale | Tonemapper::Opencl | Tonemapper::Libplacebo) => untagged_hdr_fix(src),
                        _ => None,
                    };
                    // Before a VAAPI encoder, Vulkan's device is made from VAAPI's.
                    let vulkan_decode = if vaapi { &ff.caps.vaapi_vulkan_decode } else { &ff.caps.vulkan_decode };
                    // Quick Sync lays picture subtitles over 8-bit SDR
                    // without the frames leaving the GPU, where its start-up
                    // test passed (`qsv_overlay`): at the source's size,
                    // scaled after (laid over frames its scaler had made
                    // smaller, FFmpeg 9.0 ran out of memory on Windows).
                    let qsv_burns = ff.caps.qsv_overlay
                        && encoder.accel == HwAccel::Qsv
                        && hdr.is_none()
                        && src.bit_depth.unwrap_or(8) <= 8
                        && burn.as_ref().is_some_and(|b| !matches!(b, Burn::Text { .. }));
                    // Turned frames stay in memory, where FFmpeg turns them.
                    let pipeline = if !hardware_decoding || deinterlace || turned {
                        Pipeline::Software
                    } else if tonemap == Some(Tonemapper::Libplacebo) && vulkan_decode.contains(&key) {
                        // Encoding on Vulkan too only when asked for: browsers
                        // don't play what h264_vulkan makes of 4K (and of
                        // anything, with B-frames). The frames go to the
                        // usual encoder after one download instead.
                        let encode_on_gpu = ff.caps.vulkan_encode
                            && burn.is_none()
                            && vreq.codec == VideoCodec::H264
                            && req.options.hwaccel == HwAccel::Vulkan;
                        Pipeline::Vulkan { encode_on_gpu }
                    } else if tonemap.is_none() && (burn.is_none() || qsv_burns) {
                        // HDR in 10 bits stays on the GPU with CUDA, and with
                        // Quick Sync where its start-up test passed in 10
                        // bits; the others scale on the CPU. Quick Sync lays
                        // picture subtitles on SDR itself.
                        ff.caps
                            .gpu_pipelines
                            .iter()
                            .filter(|p| hdr.is_none() || p.accel == HwAccel::Nvenc || p.main10.contains(&key))
                            .find(|p| p.accel == encoder.accel && p.codecs.contains(&key))
                            .map_or(Pipeline::Software, |p| match p.accel {
                                HwAccel::Amf => Pipeline::D3d11 { encode_on_gpu: true },
                                accel => Pipeline::Gpu(accel),
                            })
                    } else {
                        Pipeline::Software
                    };
                    let amf_upload = ff.caps.amf_upload
                        && encoder.accel == HwAccel::Amf
                        && pipeline == Pipeline::Software
                        && tonemap != Some(Tonemapper::Opencl);
                    let ten_bits = tonemap.is_some() || hdr.is_some();
                    let qsv_decodes = |accel: HwAccel| {
                        accel == HwAccel::Qsv
                            && ff.caps.gpu_pipelines.iter().any(|p| p.accel == accel && p.codecs.contains(&key))
                    };
                    // Quick Sync decodes the frames that come to memory
                    // itself, its scaler bringing them down at the output's
                    // size, where Direct3D 11's decoder copies them whole (25
                    // MB a frame of 4K HDR) and some drivers give it no HEVC
                    // at all, and VAAPI's scales none in 10 bits. Not 8-bit
                    // video kept at its size: Quick Sync's copy to memory
                    // costs the CPU more than Direct3D's.
                    // Nor Dolby Vision or HDR10+ that libplacebo tone maps:
                    // Quick Sync's decoder reads neither's metadata, and
                    // profile 5's colours are only in it.
                    let resized = (width, height) != (src_width, src_height);
                    let scene_metadata = tonemap == Some(Tonemapper::Libplacebo)
                        && ((range == DynamicRange::DolbyVision && !fel_base)
                            || range == DynamicRange::Hdr10plus
                            || src.hdr10plus == Some(true));
                    let qsv_first = qsv_decodes(encoder.accel)
                        && !deinterlace
                        && !turned
                        && !scene_metadata
                        && (resized || ten_bits || src.bit_depth.unwrap_or(8) > 8);
                    // What Quick Sync could take over where the computer
                    // falls behind: the HDR10 picture under that metadata.
                    // Not from Vulkan, which decodes it straight into
                    // libplacebo's memory with its metadata.
                    let qsv_base_layer = scene_metadata
                        && pipeline == Pipeline::Software
                        && hdr.is_none()
                        && src.has_hdr10_base()
                        && hardware_decoding
                        && qsv_decodes(encoder.accel)
                        && !deinterlace
                        && !turned;
                    let hw_decode = match pipeline {
                        Pipeline::Vulkan { .. } => Some("vulkan".to_owned()),
                        Pipeline::D3d11 { .. } => Some("d3d11va".to_owned()),
                        Pipeline::Gpu(accel) => Some(gpu_decoder_name(accel).to_owned()),
                        Pipeline::Software if hardware_decoding && qsv_first => Some("qsv".to_owned()),
                        Pipeline::Software if hardware_decoding => pick_hw_decoder(ff, &encoder),
                        Pipeline::Software => None,
                    };
                    // Frames that come to memory are scaled by the GPU that
                    // decoded them first, where its pipeline handles the codec;
                    // not those deinterlaced (by fields, at their own size)
                    // nor those libplacebo scales as it tone maps, except
                    // Quick Sync's, which come to memory through its scaler
                    // anyway (a software encoder standing in for its own too).
                    // VAAPI's scaler in 8 bits only: its start-up test makes
                    // NV12, and 10 bits on the GPU were tried with CUDA and
                    // Quick Sync alone.
                    let gpu_scale = match hw_decode.as_deref() {
                        Some("cuda") => Some(HwAccel::Nvenc),
                        Some("vaapi") if !ten_bits => Some(HwAccel::Vaapi),
                        Some("qsv") => Some(HwAccel::Qsv),
                        _ => None,
                    }
                    .filter(|accel| {
                        !deinterlace
                            && !turned
                            && (tonemap != Some(Tonemapper::Libplacebo) || *accel == HwAccel::Qsv)
                            && ff.caps.gpu_pipelines.iter().any(|p| p.accel == *accel && p.codecs.contains(&key))
                    });
                    let encoder = match pipeline {
                        Pipeline::Vulkan { encode_on_gpu: true } => vulkan_encoder(),
                        _ => encoder,
                    };
                    Some(VideoOut {
                        stream_index: src.index,
                        copy: false,
                        codec: vreq.codec,
                        src_width,
                        src_height,
                        width,
                        height,
                        bitrate_kbps: bitrate,
                        tonemap,
                        // Alone on the GPU: a job counts the others (see
                        // `Plan::share_tonemapping`).
                        tonemap_quality: quality_for(ff.caps.tonemap_speed.as_ref(), src, (width, height), 0.0),
                        tonemap_input,
                        libplacebo_asked: req.options.tonemap == TonemapMode::Libplacebo,
                        qsv_base_layer,
                        base_layer_only: fel_base,
                        fel_compose,
                        amf_upload,
                        // BT.709 out of every tone mapper; the source's otherwise.
                        vui_primaries: (ff.caps.h264_metadata
                            && encoder.accel == HwAccel::Amf
                            && vreq.codec == VideoCodec::H264
                            && hdr.is_none())
                        .then(|| if tonemap.is_some() { Some(1) } else { h264_primaries(src) })
                        .flatten(),
                        hw_decode,
                        gpu_scale,
                        deinterlace,
                        pipeline,
                        preset: req
                            .options
                            .preset
                            .clone()
                            .filter(|p| is_x264_preset(p) || p == AUTO)
                            .unwrap_or_else(|| "veryfast".into()),
                        encoder: Some(encoder),
                        dolby_vision: None,
                        strip_hdr10plus: false,
                        hevc: vreq.codec == VideoCodec::Hevc,
                        dvh1: false,
                        hdr,
                    })
                }
            }
            (Some(_), None) => {
                warnings.push("file has no video stream".into());
                None
            }
            _ => None,
        };
        if burn.is_some() && video.is_none() {
            return Err("subtitle burn-in requires a video stream".into());
        }

        // ---- audio ----
        let audio = match (&req.audio, media_audio(media, req)) {
            (Some(areq), Some(src)) => {
                let copy_ok = matches!(src.codec.as_str(), "aac" | "mp3" | "ac3" | "eac3");
                let copy = areq.mode == AudioMode::Copy && copy_ok;
                let codec = areq.codec;
                if areq.mode == AudioMode::Copy && !copy_ok {
                    warnings.push(format!("{} audio transcoded to {}", src.codec, codec_name(codec)));
                }
                let src_channels = src.channels.unwrap_or(2).max(1) as u32;
                // AC-3 and E-AC-3 (as Dolby makes it, and FFmpeg's encoders)
                // stop at 5.1; AAC goes to 7.1.
                let most = if codec == AudioCodec::Aac { 8 } else { 6 };
                let channels = areq.channels.unwrap_or(2).clamp(1, most).min(src_channels);
                let default_bitrate = match codec {
                    _ if channels <= 2 => 192,
                    AudioCodec::Aac => 64 * channels,
                    AudioCodec::Ac3 | AudioCodec::Eac3 => 640,
                };
                let bitrate = areq.bitrate_kbps.unwrap_or(default_bitrate).clamp(64, 640);
                // FFmpeg's AC-3 encoders take 32, 44.1 and 48 kHz only.
                let resample = !matches!(src.sample_rate, Some(44_100) | Some(48_000));
                let partial_decode = (!copy).then(|| partial_decode(src, channels)).flatten();
                let downmix = (!copy).then(|| stereo_downmix(src, channels, partial_decode)).flatten();
                Some(AudioOut {
                    stream_index: src.index,
                    copy,
                    codec,
                    channels,
                    bitrate_kbps: bitrate,
                    resample,
                    partial_decode,
                    downmix,
                })
            }
            (Some(_), None) if video.is_none() => return Err("file has no audio or video stream".into()),
            _ => None,
        };
        if video.is_none() && audio.is_none() {
            return Err("nothing to transcode: no video or audio requested".into());
        }

        // ---- segment layout ----
        let (boundaries, split_at, seek_at) = match (&video, keyframes) {
            (Some(v), Some(kf)) if v.copy => {
                // Keyframe times are packet timestamps; `-ss` and the output
                // timeline count from the container's start time (often far
                // from zero in MPEG-TS).
                let offset = media.start_ms.unwrap_or(0);
                let relative: Vec<i64> = kf.iter().map(|t| t.saturating_sub(offset)).collect();
                let (boundaries, split_at) = keyframe_layout(&relative, duration_ms, target_ms);
                let seek_at = remux_seek_points(&boundaries, &relative, seeks_back(media));
                (boundaries, split_at, seek_at)
            }
            _ => {
                let (boundaries, split_at) = uniform_layout(duration_ms, target_ms);
                (boundaries.clone(), split_at, boundaries)
            }
        };

        // fMP4 segments when asked for, but not without video: fragments are
        // cut at video keyframes, and Apple's players take AAC, AC-3 and
        // E-AC-3 in MPEG-TS. No warning (what the job makes is in
        // `JobCreated`).
        let segment_format = match (req.segment_format, &video) {
            (SegmentFormat::Fmp4, None) => {
                tracing::debug!("no video: MPEG-TS segments rather than fMP4");
                SegmentFormat::Ts
            }
            (format, _) => format,
        };

        Ok(Plan {
            duration_ms,
            boundaries,
            split_at,
            seek_at,
            segment_seconds,
            video,
            audio,
            burn,
            throttle: req.options.throttle && ff.caps.readrate,
            catchup: req.options.throttle && ff.caps.readrate && ff.caps.readrate_catchup,
            warnings,
            segment_format,
        })
    }

    pub fn video_plan(&self, media: &MediaInfo, encoder_override: Option<&EncoderInfo>) -> Option<VideoPlan> {
        let v = self.video.as_ref()?;
        let src = media.stream(v.stream_index);
        Some(VideoPlan {
            mode: if v.copy { VideoMode::Copy } else { VideoMode::Transcode },
            codec_in: src.map(|s| s.codec.clone()).unwrap_or_default(),
            codec_out: if v.copy {
                src.map(|s| s.codec.clone()).unwrap_or_default()
            } else {
                match v.codec {
                    VideoCodec::H264 => "h264".into(),
                    VideoCodec::Hevc => "hevc".into(),
                }
            },
            width_in: Some(v.src_width),
            height_in: Some(v.src_height),
            width_out: Some(v.width),
            height_out: Some(v.height),
            dynamic_range_in: src.map(|s| s.dynamic_range()).unwrap_or_default(),
            dynamic_range_out: (!v.copy).then(|| v.hdr.map_or(DynamicRange::Sdr, |h| h.range)),
            dv_profile: src.and_then(|s| s.dv_profile),
            dv_bl_compat: src.and_then(|s| s.dv_bl_compat),
            tonemap: v.tonemap.filter(|t| *t != Tonemapper::QuickSync),
            tonemap_quick_sync: v.tonemap == Some(Tonemapper::QuickSync),
            tonemap_quality: (v.tonemap == Some(Tonemapper::Libplacebo)).then_some(v.tonemap_quality),
            encoder: if v.copy { None } else { encoder_override.or(v.encoder.as_ref()).map(|e| e.name.clone()) },
            decoder: Some(if v.copy { "copy".into() } else { v.hw_decode.clone().unwrap_or_else(|| "cpu".into()) }),
            pipeline: Some(
                if v.copy {
                    "copy"
                } else {
                    let pipeline =
                        if encoder_override.is_some() { v.pipeline.for_software_encoder() } else { v.pipeline };
                    match pipeline {
                        Pipeline::Gpu(_)
                        | Pipeline::Vulkan { encode_on_gpu: true }
                        | Pipeline::D3d11 { encode_on_gpu: true } => "gpu",
                        Pipeline::Vulkan { encode_on_gpu: false } | Pipeline::D3d11 { encode_on_gpu: false } => {
                            "hybrid"
                        }
                        Pipeline::Software if v.hw_decode.is_some() => "hybrid",
                        Pipeline::Software => "cpu",
                    }
                }
                .to_owned(),
            ),
            bitrate_kbps: (!v.copy).then_some(v.bitrate_kbps),
            subtitle_burn: self.burn.as_ref().map(|b| b.label().to_owned()),
            dolby_vision: v.dolby_vision,
            hdr10plus_stripped: v.strip_hdr10plus,
            dv_el: src.and_then(|s| s.dv_el),
            hdr10plus: src.is_some_and(|s| s.hdr10plus == Some(true)),
        })
    }

    /// Whether libplacebo tone maps its video, on the GPU.
    pub fn tone_maps_with_libplacebo(&self) -> bool {
        self.video.as_ref().is_some_and(|v| !v.copy && v.tonemap == Some(Tonemapper::Libplacebo))
    }

    /// Tone maps at the best quality the GPU keeps ahead of real time
    /// beside libplacebo transcodes needing `others` (see
    /// [`tonemap_quality`]): what its first process does. Later ones follow
    /// the GPU's load (see [`crate::share`]).
    pub fn share_tonemapping(&mut self, speed: Option<&TonemapSpeed>, media: &MediaInfo, others: f64) {
        if let Some(quality) = self.shared_quality(speed, media, others)
            && let Some(v) = self.video.as_mut()
        {
            v.tonemap_quality = quality;
        }
    }

    /// The quality [`Plan::share_tonemapping`] takes; `None` when
    /// libplacebo doesn't tone map the video.
    pub fn shared_quality(
        &self,
        speed: Option<&TonemapSpeed>,
        media: &MediaInfo,
        others: f64,
    ) -> Option<TonemapQuality> {
        let (v, src) = self.placebo_source(media)?;
        Some(quality_for(speed, src, (v.width, v.height), others))
    }

    /// What libplacebo's tone mapping of this video asks of the GPU (see
    /// [`tonemap_need`]); `None` when libplacebo doesn't tone map it.
    pub fn tonemap_need(&self, media: &MediaInfo) -> Option<f64> {
        let (v, src) = self.placebo_source(media)?;
        Some(need_of(src, v.width, v.height))
    }

    /// Whether the GPU falls behind with this and libplacebo transcodes
    /// needing `others` on it, even at the fastest quality (see
    /// [`tonemap_falls_behind`]): where decoding on the CPU relieves it
    /// (see [`crate::share`]). Not without a measurement.
    pub fn tonemapping_falls_behind(&self, speed: Option<&TonemapSpeed>, media: &MediaInfo, others: f64) -> bool {
        let (Some(speed), Some(need)) = (speed, self.tonemap_need(media)) else { return false };
        tonemap_falls_behind(speed, need + others)
    }

    /// The video libplacebo tone maps, and its stream in `media`.
    fn placebo_source<'a>(&'a self, media: &'a MediaInfo) -> Option<(&'a VideoOut, &'a MediaStream)> {
        let v = self.video.as_ref().filter(|v| !v.copy && v.tonemap == Some(Tonemapper::Libplacebo))?;
        Some((v, media.stream(v.stream_index)?))
    }

    /// Whether the video is decoded by a GPU that encodes on the same video
    /// engine (AMD's, Intel's) into memory, where the CPU could decode it
    /// instead (see [`crate::share`]): libplacebo's frames, which come to
    /// memory whichever decodes them, before a hardware encoder.
    pub fn decodes_on_shared_engine(&self) -> bool {
        self.video.as_ref().is_some_and(|v| {
            !v.copy
                && v.pipeline == Pipeline::Software
                && v.tonemap == Some(Tonemapper::Libplacebo)
                && v.gpu_scale.is_none()
                && matches!(v.hw_decode.as_deref(), Some("d3d11va" | "vaapi" | "qsv"))
                && v.encoder.as_ref().is_some_and(|e| matches!(e.accel, HwAccel::Amf | HwAccel::Vaapi | HwAccel::Qsv))
        })
    }

    /// Whether Quick Sync's scaler could tone map this video in libplacebo's
    /// place ([`Tonemapper::QuickSync`]), where its start-up test passed
    /// (`qsv_tonemap`): HDR10 made SDR, decoded and scaled by Quick Sync,
    /// or the HDR10 base layer of Dolby Vision or HDR10+ that it could
    /// decode ([`VideoOut::qsv_base_layer`]), where libplacebo wasn't asked
    /// for. Only video that carries HDR10's static metadata: Quick Sync's
    /// scaler tone maps by it, and leaves the picture as it is without
    /// (VAAPI's refuses it).
    pub fn quick_sync_could_tonemap(&self, media: &MediaInfo, qsv_tonemap: bool) -> bool {
        qsv_tonemap
            && self.placebo_source(media).is_some_and(|(v, src)| {
                !v.libplacebo_asked
                    && v.hdr.is_none()
                    && src.hdr10_metadata == Some(true)
                    && (v.qsv_base_layer
                        || (v.gpu_scale == Some(HwAccel::Qsv)
                            && (src.dynamic_range() == DynamicRange::Hdr10 || v.base_layer_only)))
            })
    }

    /// The input the full enhancement layer comes from: after the picture
    /// subtitles' file where there is one.
    fn fel_input(&self) -> usize {
        if matches!(self.burn, Some(Burn::ImageFile { .. })) { 2 } else { 1 }
    }

    /// Whether Quick Sync could decode this video's HDR10 base layer in
    /// place of FFmpeg's own decoder ([`VideoOut::qsv_base_layer`]): what
    /// relieves a CPU that decodes it, the GPU's decoder having failed.
    pub fn quick_sync_would_decode(&self) -> bool {
        self.video.as_ref().is_some_and(|v| !v.copy && v.qsv_base_layer)
    }

    /// The key of the video's decoder ([`decode_key`]), where it is
    /// transcoded.
    pub fn decode_key(&self, media: &MediaInfo) -> Option<String> {
        let v = self.video.as_ref().filter(|v| !v.copy)?;
        let src = media.stream(v.stream_index)?;
        Some(decode_key(&src.codec, src.bit_depth, src.pix_fmt.as_deref()))
    }

    /// The pixels a frame of the source has and its frame rate: what
    /// decoding it takes (see [`crate::share::decode_cores`]).
    pub fn source_rate(&self, media: &MediaInfo) -> Option<(u64, f64)> {
        let v = self.video.as_ref().filter(|v| !v.copy)?;
        let fps = media.stream(v.stream_index).and_then(|s| s.frame_rate).filter(|f| f.is_finite() && *f > 0.0);
        Some((v.src_width.max(1) as u64 * v.src_height.max(1) as u64, fps.unwrap_or(24.0).min(120.0)))
    }

    /// This plan for one process with `share` of the GPU: its tone mapping
    /// quality, and its video decoded by the CPU where the share says so.
    pub fn with_share(&self, share: Share) -> Plan {
        let mut plan = self.clone();
        let burn = plan.burn.is_some();
        if let Some(v) = plan.video.as_mut() {
            v.tonemap_quality = share.quality;
            if share.cpu_decode {
                v.hw_decode = None;
            }
            // The base layer alone: Quick Sync decodes and scales it, for
            // libplacebo or for its own tone mapping.
            if (share.base_layer || share.quick_sync) && v.qsv_base_layer {
                v.hw_decode = Some("qsv".to_owned());
                v.gpu_scale = Some(HwAccel::Qsv);
            }
            if share.quick_sync {
                v.tonemap = Some(Tonemapper::QuickSync);
                // Into Quick Sync's encoder without leaving the GPU.
                if !burn && v.encoder.as_ref().is_some_and(|e| e.accel == HwAccel::Qsv && e.hardware) {
                    v.pipeline = Pipeline::Gpu(HwAccel::Qsv);
                }
            }
        }
        plan
    }

    /// Tone maps as `other` does: a job's renditions look the same, so a
    /// player moving between them sees no change.
    pub fn tone_map_like(&mut self, other: &Plan) {
        if let (Some(v), Some(o)) = (self.video.as_mut(), other.video.as_ref()) {
            v.tonemap_quality = o.tonemap_quality;
        }
    }

    /// The encoder the video is made with: none for a copy, or no video.
    pub fn encoder(&self) -> Option<&EncoderInfo> {
        self.video.as_ref().filter(|v| !v.copy).and_then(|v| v.encoder.as_ref())
    }

    /// The software encoder that stands in for the planned hardware one
    /// when hardware sessions run out or the hardware encoder fails: for HDR
    /// kept, one that makes Main 10 too.
    pub fn software_stand_in<'a>(&self, ff: &'a Ffmpeg) -> Option<&'a EncoderInfo> {
        let planned = self.encoder().filter(|e| e.hardware)?;
        let main10 = self.video.as_ref().is_some_and(|v| v.hdr.is_some());
        ff.software_encoder_for(planned.codec, main10)
    }

    /// What finished segments need done to their video by the transcoder
    /// itself (see [`crate::tsedit`]), when anything.
    pub fn segment_edits(&self) -> Option<Edits> {
        let v = self.video.as_ref().filter(|v| v.copy)?;
        let edits = Edits { to_81: v.dolby_vision == Some(DolbyVisionMode::To81), strip_hdr10plus: v.strip_hdr10plus };
        edits.any().then_some(edits)
    }

    /// The Dolby Vision configuration record an fMP4 remux's init gets
    /// ([`crate::fmp4::set_dovi`]), the `dby1` brand with it: `None` to leave
    /// FFmpeg's init as it is (no Dolby Vision copied), `Some(None)` for no
    /// record at all (Dolby Vision copied but not named, as the codecs the
    /// master playlist names say). The level is the one the codecs name.
    pub fn init_dolby_vision(&self, media: &MediaInfo) -> Option<Option<DoviConfig>> {
        let v = self.video.as_ref().filter(|v| v.copy)?;
        let mode = v.dolby_vision?;
        let src = media.stream(v.stream_index)?;
        let level = src.dv_level_named().unwrap_or(0).clamp(0, 63) as u8;
        Some(match mode {
            DolbyVisionMode::Copy => None,
            // The enhancement layer left out and the RPUs made 8.1 over the
            // HDR10 base layer.
            DolbyVisionMode::To81 => Some(DoviConfig { profile: 8, level, rpu: true, el: false, bl: true, compat: 1 }),
            DolbyVisionMode::Signal => {
                let profile = src.dv_profile.unwrap_or(0).clamp(0, 127) as u8;
                let compat = src.dv_bl_compat.unwrap_or(if profile == 5 { 0 } else { 1 }).clamp(0, 15) as u8;
                let el = matches!(profile, 4 | 7);
                Some(DoviConfig { profile, level, rpu: true, el, bl: true, compat })
            }
        })
    }

    pub fn audio_plan(&self, media: &MediaInfo) -> Option<AudioPlan> {
        let a = self.audio.as_ref()?;
        let src = media.stream(a.stream_index);
        Some(AudioPlan {
            mode: if a.copy { AudioMode::Copy } else { AudioMode::Transcode },
            codec_in: src.map(|s| s.codec.clone()).unwrap_or_default(),
            codec_out: if a.copy { src.map(|s| s.codec.clone()).unwrap_or_default() } else { a.codec.as_str().into() },
            channels_in: src.and_then(|s| s.channels),
            channels_out: Some(if a.copy { src.and_then(|s| s.channels).unwrap_or(2) } else { a.channels as i64 }),
            bitrate_kbps: (!a.copy).then_some(a.bitrate_kbps),
        })
    }

    /// ffmpeg arguments producing segments `[start, end)`: MPEG-TS
    /// segments it writes itself and lists on stdout, or for fMP4 one
    /// fragmented MP4 on stdout that the transcoder cuts into segments at
    /// [`Plan::split_at`] (see `job.rs`).
    pub fn ffmpeg_args(&self, ctx: &ArgsContext<'_>, start: u32, end: u32) -> Vec<OsString> {
        let start_ms = self.boundaries[start as usize];
        let mut a = self.stream_args(ctx, start, end);
        if self.segment_format == SegmentFormat::Fmp4 {
            match &self.video {
                Some(v) if v.dvh1 => a.push_all(["-tag:v", "dvh1"]),
                Some(v) if v.hevc => a.push_all(["-tag:v", "hvc1"]),
                _ => {}
            }
            // Frames at a constant rate, as the segment muxer gives MPEG-TS:
            // the mp4 muxer's variable rate drops a frame that hardware
            // decoding hands back late after a seek, which moves the forced
            // keyframes and every segment of the process one frame later.
            if self.video.as_ref().is_some_and(|v| !v.copy) {
                a.push_all(["-fps_mode:v", "cfr"]);
            }
            // No timecode track from a MOV source; each fragment reaches the
            // pipe as soon as it is cut.
            a.push_all(["-f", "mp4", "-movflags", FMP4_MOVFLAGS, "-write_tmcd", "0", "-flush_packets", "1", "pipe:1"]);
            return a.0;
        }

        // Segmenting. Relative split times count from the first video packet,
        // which is the boundary (a remux's seek point lies past it).
        let times: Vec<String> = ((start + 1)..end)
            .map(|k| {
                let at = self.split_at[k as usize];
                secs(if ctx.relative_split_times { at - start_ms } else { at })
            })
            .collect();
        a.push_all([
            "-f",
            "segment",
            "-segment_format",
            "mpegts",
            "-segment_format_options",
            "avoid_negative_ts=disabled",
        ]);
        if !times.is_empty() {
            a.push_all(["-segment_times", &times.join(",")]);
        } else {
            // Single segment window: never split.
            a.push_all(["-segment_time", "999999"]);
        }
        a.push_all([
            "-segment_start_number",
            &start.to_string(),
            "-segment_list",
            "pipe:1",
            "-segment_list_type",
            "csv",
            "%d.ts",
        ]);
        a.0
    }

    /// The whole media as one file at `output` (a converted download): the
    /// streams the segments would have, made in one go, and the subtitle
    /// tracks of `media` in their order, for the player to show itself. In
    /// Matroska every one as it is (MP4's text made SRT); in MP4 the text
    /// ones made MP4's own, the pictures left out.
    /// Players find a track by its place among the subtitle tracks, the
    /// original's as the file's.
    pub fn file_args(
        &self,
        ctx: &ArgsContext<'_>,
        media: &MediaInfo,
        output: &Path,
        container: FileContainer,
    ) -> Vec<OsString> {
        let mut a = self.stream_args(ctx, 0, self.segment_count());
        // Their languages and names (stream_args leaves the input's metadata out).
        let name = |a: &mut Args, out: &str, s: &MediaStream| {
            let key = format!("-metadata:s:{out}");
            if let Some(language) = s.language.as_deref().filter(|l| !l.is_empty()) {
                a.push_all([key.as_str(), &format!("language={language}")]);
            }
            if let Some(title) = s.title.as_deref().filter(|t| !t.is_empty()) {
                a.push_all([key.as_str(), &format!("title={title}")]);
            }
        };
        if let Some(sound) = self.audio.as_ref().and_then(|au| media.stream(au.stream_index)) {
            name(&mut a, "a:0", sound);
        }
        let mut n = 0;
        for s in media.streams.iter().filter(|s| s.kind == StreamKind::Subtitle) {
            let codec = match (container, s.codec.as_str()) {
                (FileContainer::Matroska, c) if MATROSKA_SUBTITLES.contains(&c) => "copy",
                (FileContainer::Matroska, "mov_text") => "srt",
                (FileContainer::Mp4, c) if MP4_TEXT_SUBTITLES.contains(&c) => "mov_text",
                // Teletext and the like, and pictures in MP4: no place for them.
                _ => continue,
            };
            a.push_all(["-map", &format!("0:{}", s.index), &format!("-c:s:{n}"), codec]);
            name(&mut a, &format!("s:{n}"), s);
            n += 1;
        }
        match container {
            FileContainer::Matroska => a.push_all(["-f", "matroska"]),
            FileContainer::Mp4 => {
                let hevc = self.video.as_ref().is_some_and(|v| {
                    if v.copy {
                        media.stream(v.stream_index).is_some_and(|s| s.codec == "hevc")
                    } else {
                        v.codec == VideoCodec::Hevc
                    }
                });
                // QuickTime plays HEVC tagged hvc1 only (FFmpeg writes hev1).
                if hevc {
                    a.push_all(["-tag:v", "hvc1"]);
                }
                // The index first: it plays as soon as it opens, from anywhere.
                a.push_all(["-movflags", "+faststart", "-f", "mp4"]);
            }
        }
        a.push_os(output.as_os_str().to_owned());
        a.0
    }

    /// Everything but the output: devices, inputs, the video and audio
    /// streams, from segment `start` up to `end`.
    fn stream_args(&self, ctx: &ArgsContext<'_>, start: u32, end: u32) -> Args {
        let start_ms = self.boundaries[start as usize];
        let end_ms = self.boundaries[end as usize];
        let copy = self.video.as_ref().is_some_and(|v| v.copy);
        // Remux seeks land on the keyframe at or before the seek point, which
        // sits a little past the boundary keyframe. The output starts at that
        // keyframe (the boundary), so the split times count from there.
        let seek_ms = if copy { self.seek_at[start as usize] } else { start_ms };
        let encoder = ctx.encoder.or_else(|| self.video.as_ref().and_then(|v| v.encoder.as_ref()));
        let pipeline = self.video.as_ref().map_or(Pipeline::Software, |v| {
            if ctx.encoder.is_some() { v.pipeline.for_software_encoder() } else { v.pipeline }
        });

        let mut a = Args::default();
        a.push_all(["-loglevel", "error", "-nostats", "-progress", "pipe:2", "-y"]);

        // Global hardware devices and decoding. A tone mapper's device is
        // made from VAAPI's when a VAAPI encoder takes its frames.
        let vaapi = encoder.is_some_and(|e| e.accel == HwAccel::Vaapi);
        let filter_device = |a: &mut Args, device: &str| {
            if vaapi {
                a.extend(vaapi_filter_devices(device, vaapi_device()).to_vec());
            } else {
                a.push_all(["-init_hw_device", device]);
            }
        };
        if let Some(v) = self.video.as_ref().filter(|v| !v.copy) {
            // Quick Sync on Linux decodes and encodes on Intel's GPU, its
            // device made from VAAPI's (VAAPI's decoder takes that one too).
            let qsv_on_vaapi = QSV_ON_VAAPI
                && (encoder.is_some_and(|e| e.accel == HwAccel::Qsv) || v.hw_decode.as_deref() == Some("qsv"));
            match pipeline {
                Pipeline::Gpu(HwAccel::Qsv) if qsv_on_vaapi => a.extend(qsv_gpu_devices(&[], qsv_device())),
                _ if qsv_on_vaapi => a.extend(qsv_vaapi_devices(qsv_device()).to_vec()),
                _ => {}
            }
            match pipeline {
                Pipeline::Gpu(HwAccel::Qsv) if qsv_on_vaapi => {}
                Pipeline::Vulkan { .. } => {
                    filter_device(&mut a, "vulkan=vk");
                    a.push_all([
                        "-filter_hw_device",
                        "vk",
                        "-hwaccel",
                        "vulkan",
                        "-hwaccel_device",
                        "vk",
                        "-hwaccel_output_format",
                        "vulkan",
                    ]);
                }
                Pipeline::Gpu(HwAccel::Vaapi) => a.push_all([
                    "-hwaccel",
                    "vaapi",
                    "-hwaccel_device",
                    vaapi_device(),
                    "-hwaccel_output_format",
                    "vaapi",
                ]),
                Pipeline::D3d11 { .. } => a.push_all([
                    "-init_hw_device",
                    AMF_D3D11_DEVICE,
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
                ]),
                // Subtitles uploaded to Quick Sync need its device made from
                // Direct3D 11's: on the one FFmpeg makes itself, the upload
                // fails to synchronise.
                Pipeline::Gpu(HwAccel::Qsv) if self.burn.is_some() => a.push_all(QSV_D3D11_DEVICES),
                Pipeline::Gpu(HwAccel::Qsv) => a.push_all(QSV_DECODE),
                Pipeline::Gpu(HwAccel::Videotoolbox) => {
                    a.push_all(["-hwaccel", "videotoolbox", "-hwaccel_output_format", "videotoolbox_vld"])
                }
                Pipeline::Gpu(_) => {
                    a.push_all(["-hwaccel", "cuda", "-hwaccel_output_format", "cuda", "-extra_hw_frames", "8"])
                }
                Pipeline::Software => {
                    // AMF's frames are uploaded to the AMD GPU by FFmpeg: its
                    // device is the filters' then, and libplacebo makes a
                    // Vulkan device of its own.
                    let upload = amf_upload(v, encoder);
                    if upload {
                        a.push_all(["-init_hw_device", AMF_D3D11_DEVICE, "-filter_hw_device", "dx"]);
                    }
                    match v.tonemap {
                        Some(Tonemapper::Libplacebo) if !upload => {
                            filter_device(&mut a, "vulkan=vk");
                            a.push_all(["-filter_hw_device", "vk"]);
                        }
                        Some(Tonemapper::Opencl) => {
                            filter_device(&mut a, "opencl=ocl");
                            a.push_all(["-filter_hw_device", "ocl"]);
                        }
                        _ if vaapi => a.push_all(["-vaapi_device", vaapi_device()]),
                        _ => {}
                    }
                    if let Some(dec) = &v.hw_decode {
                        a.push_all(["-hwaccel", dec.as_str()]);
                        // Decoded on the GPU that encodes.
                        if upload && dec == "d3d11va" {
                            a.push_all(["-hwaccel_device", "dx"]);
                        }
                        if qsv_on_vaapi && dec == "qsv" {
                            a.push_all(["-hwaccel_device", "qs"]);
                        }
                        // Frames kept on the GPU for its scaler.
                        match v.scales_on_gpu(pipeline) {
                            Some(HwAccel::Vaapi) => a.push_all(["-hwaccel_output_format", "vaapi"]),
                            Some(HwAccel::Qsv) => a.push_all(["-hwaccel_output_format", "qsv"]),
                            Some(_) => a.push_all(["-hwaccel_output_format", "cuda", "-extra_hw_frames", "8"]),
                            None => {}
                        }
                    }
                }
            }
        }

        // Main input.
        if self.throttle {
            let (rate, burst) = (trim_float(READRATE), (READRATE_BURST_MS / 1000).to_string());
            a.push_all(["-readrate", &rate, "-readrate_initial_burst", &burst]);
            if self.catchup {
                a.push_all(["-readrate_catchup", &trim_float(READRATE_CATCHUP)]);
            }
        }
        if seek_ms > 0 {
            // Converted audio starts at the keyframe like the copied video,
            // instead of at the seek point: no hole where an older process's
            // segment plays into this one's first.
            if copy {
                a.push("-noaccurate_seek");
            }
            a.push_all(["-ss", &secs(seek_ms)]);
        }
        if let Some((au, (option, value))) = self.audio.as_ref().and_then(|au| au.partial_decode.map(|d| (au, d))) {
            a.push_all([&format!("-{option}:{}", au.stream_index), value]);
        }
        a.push_all(MEDIA_INPUT_OPTIONS);
        a.push("-i");
        a.push_os(input_arg(ctx.input));

        // Optional bitmap subtitle file as a second input.
        if let Some(Burn::ImageFile { path, .. }) = &self.burn {
            if seek_ms > 0 {
                a.push_all(["-ss", &secs(seek_ms)]);
            }
            a.push_all(MEDIA_INPUT_OPTIONS);
            a.push("-i");
            a.push_os(input_arg(path));
        }
        // Dolby Vision's full enhancement layer: the file again, its video
        // split down to that layer, seeking where the first input does.
        if let Some(v) = self.video.as_ref().filter(|v| v.fel_compose && !v.copy) {
            if seek_ms > 0 {
                a.push_all(["-ss", &secs(seek_ms)]);
            }
            a.push_all([format!("-bsf:{}", v.stream_index).as_str(), "dovi_split=mode=el"]);
            a.push_all(MEDIA_INPUT_OPTIONS);
            a.push("-i");
            a.push_os(input_arg(ctx.input));
        }

        if seek_ms > 0 {
            a.push_all(["-output_ts_offset", &secs(seek_ms)]);
        }

        // Video.
        match &self.video {
            Some(v) if v.copy => {
                a.push_all(["-map", &format!("0:{}", v.stream_index), "-c:v", "copy"]);
            }
            Some(v) => {
                let graph = self.filter_graph(v, encoder, pipeline, start_ms, ctx.shader_cache);
                a.push_all(["-filter_complex", &graph, "-map", "[vout]"]);
                if let Some(enc) = encoder {
                    a.extend(encoder_args(enc, v, ctx.preset));
                }
                if let Some(primaries) = v.vui_primaries.filter(|_| encoder.is_some_and(|e| e.accel == HwAccel::Amf)) {
                    a.push_all(["-bsf:v", &format!("h264_metadata=colour_primaries={primaries}")]);
                }
                let keyint = format!("expr:gte(t,n_forced*{})", trim_float(self.segment_seconds));
                a.push_all(["-force_key_frames", &keyint]);
            }
            None => a.push("-vn"),
        }

        // Audio.
        match &self.audio {
            Some(au) => {
                a.push_all(["-map", &format!("0:{}", au.stream_index)]);
                if au.copy {
                    a.push_all(["-c:a", "copy"]);
                    // FFmpeg copies the packets the demuxer's seek landed
                    // on before the seek point too (`-copypriorss` -1): up
                    // to a keyframe interval of sound (1.5 s of E-AC-3 in
                    // Matroska) that the previous process's segment has
                    // already played. Beside transcoded video, which starts
                    // at the seek point, they are left out. (A remux's video
                    // starts at the keyframe before its seek point, and its
                    // sound with it.)
                    if seek_ms > 0 && !copy {
                        a.push_all(["-copypriorss:a", "0"]);
                    }
                } else {
                    if let Some(downmix) = au.downmix {
                        a.push_all(["-af", downmix]);
                    }
                    a.push_all([
                        "-c:a",
                        au.codec.as_str(),
                        "-ac",
                        &au.channels.to_string(),
                        "-b:a",
                        &format!("{}k", au.bitrate_kbps),
                    ]);
                    if au.resample {
                        a.push_all(["-ar", "48000"]);
                    }
                }
            }
            None => a.push("-an"),
        }

        a.push_all(["-map_metadata", "-1", "-map_chapters", "-1", "-max_muxing_queue_size", "8192"]);
        // Never shift timestamps (B-frame delay would otherwise move only the
        // process that starts at 0), so every process shares one timeline.
        a.push_all(["-avoid_negative_ts", "disabled"]);
        if end < self.segment_count() {
            // (Counted from the seek point.)
            a.push_all(["-t", &secs(end_ms - seek_ms)]);
        }
        a
    }

    fn filter_graph(
        &self,
        v: &VideoOut,
        encoder: Option<&EncoderInfo>,
        pipeline: Pipeline,
        start_ms: i64,
        shader_cache: Option<&Path>,
    ) -> String {
        // With the full enhancement layer, the picture both layers make.
        let input = if v.fel_compose {
            let si = v.stream_index;
            format!("[0:{si}][{}:{si}]dovi_compose[fel];[fel]", self.fel_input())
        } else {
            format!("[0:{}]", v.stream_index)
        };
        let (w, h) = (v.width, v.height);
        // The pixel format the encoder gets (from the CPU, or downloaded).
        let ten_bit = v.hdr.map(|_| ten_bit_format(encoder.map_or(HwAccel::None, |e| e.accel)));
        // Zero-copy: one GPU scaler converts to NV12 (P010 for HDR) for the encoder.
        if let Pipeline::Gpu(accel) = pipeline {
            // Picture subtitles laid on by Quick Sync (see `qsv_burns`), over
            // the frame at its own size, then scaled. The subtitles' last
            // picture stays at their end (`repeat`): with `pass` Quick Sync
            // drops the video's last frame.
            let subs = match &self.burn {
                Some(Burn::Image { stream_index, .. }) => Some(format!("[0:{stream_index}]")),
                Some(Burn::ImageFile { .. }) => Some("[1:0]".to_owned()),
                _ => None,
            };
            if let Some(subs) = subs.filter(|_| accel == HwAccel::Qsv) {
                let scale = if (w, h) != (v.src_width, v.src_height) {
                    format!(",vpp_qsv=w={w}:h={h}:format=nv12")
                } else {
                    String::new()
                };
                return format!(
                    "{subs}scale={}:-1,format=bgra,hwupload=extra_hw_frames=16[subs];{input}[subs]overlay_qsv=\
                     x=(main_w-overlay_w)/2:y=main_h-overlay_h:eof_action=repeat:repeatlast=1{scale}[vout]",
                    v.src_width
                );
            }
            let format = if v.hdr.is_some() { "p010le" } else { "nv12" };
            let scaler = match accel {
                HwAccel::Vaapi => format!("scale_vaapi=w={w}:h={h}:format={format}"),
                // Tagged first where the source isn't: Quick Sync leaves
                // frames of unknown colours as they are.
                HwAccel::Qsv if v.tonemap == Some(Tonemapper::QuickSync) => {
                    v.tonemap_input.iter().map(|f| format!("{f},")).chain([qsv_tonemap(w, h)]).collect()
                }
                HwAccel::Qsv => format!("vpp_qsv=w={w}:h={h}:format={format}"),
                HwAccel::Videotoolbox => format!("scale_vt=w={w}:h={h}"),
                _ => format!("scale_cuda=w={w}:h={h}:format={format}"),
            };
            return format!("{input}{scaler}[vout]");
        }
        let scale_needed = v.width != v.src_width || v.height != v.src_height;
        // Zero-copy on Direct3D 11: scaled into NV12 for AMF.
        if let Pipeline::D3d11 { encode_on_gpu } = pipeline {
            let mut steps = vec![format!("scale_d3d11=width={w}:height={h}:format=nv12")];
            if !encode_on_gpu {
                // x265 takes no NV12.
                steps.push("hwdownload,format=nv12,format=yuv420p".into());
            }
            return format!("{input}{}[vout]", steps.join(","));
        }

        // Everything is done at the output's size as early as it can be:
        // deinterlacing at the source's, then tone mapping and scaling in one
        // step where the tone mapper scales (libplacebo, zimg), picture
        // subtitles laid on the scaled frame.
        let mut pre = Vec::<String>::new(); // before the (optional) overlay
        let mut post = Vec::<String>::new(); // after it

        if v.deinterlace {
            pre.push("yadif=mode=send_frame:deint=interlaced".into());
        }
        // Scaled by the GPU that decoded the frames, then copied to memory:
        // in 10 bits for tone mapping and HDR kept.
        let scale_needed = match v.scales_on_gpu(pipeline) {
            // Tone mapped as it scales, for a software encoder standing in
            // for Quick Sync's.
            Some(HwAccel::Qsv) if v.tonemap == Some(Tonemapper::QuickSync) => {
                pre.extend(v.tonemap_input.clone());
                pre.push(format!("{},hwdownload,format=nv12", qsv_tonemap(w, h)));
                false
            }
            Some(accel) => {
                let format = if v.tonemap.is_some() || v.hdr.is_some() { "p010le" } else { "nv12" };
                let scaler = match accel {
                    HwAccel::Vaapi => "scale_vaapi",
                    HwAccel::Qsv => "vpp_qsv",
                    _ => "scale_cuda",
                };
                pre.push(format!("{scaler}=w={w}:h={h}:format={format},hwdownload,format={format}"));
                false
            }
            None => scale_needed,
        };
        let hardware = encoder.is_some_and(|e| e.hardware);
        match v.tonemap {
            Some(Tonemapper::Libplacebo) => {
                let vulkan = matches!(pipeline, Pipeline::Vulkan { .. });
                let format = match (vulkan, v.hdr.is_some()) {
                    (true, false) => "nv12",
                    (true, true) => "p010",
                    // What hardware encoders take as it is.
                    (false, false) if hardware => "nv12",
                    (false, false) => "yuv420p",
                    (false, true) => "p010le",
                };
                // To SDR (tone mapping), or to the HDR format kept.
                let target = match v.hdr {
                    Some(hdr) => format!("colorspace=bt2020nc:color_primaries=bt2020:color_trc={}", hdr.transfer()),
                    None => "colorspace=bt709:color_primaries=bt709:color_trc=bt709".to_owned(),
                };
                // Shaders compiled by earlier processes save ~0.2 s at the
                // start of each one.
                let cache = shader_cache.and_then(shader_cache_option).map(|o| format!(":{o}")).unwrap_or_default();
                // What libplacebo gets: the source, or frames the GPU scaled.
                let size_in = if v.scales_on_gpu(pipeline).is_some() { (w, h) } else { (v.src_width, v.src_height) };
                let quality = placebo_quality_options(v.tonemap_quality, size_in, (w, h), v.hdr.is_none());
                // The base layer alone: whichever decoder brings the frames,
                // with their Dolby Vision metadata or without.
                let unread = if v.base_layer_only { ":apply_dolbyvision=0" } else { "" };
                pre.push(format!(
                    "libplacebo=w={w}:h={h}:format={format}:{target}:range=tv:tonemapping=auto:{PLACEBO_STATIC_PEAK}{unread}{quality}{cache}"
                ));
                match pipeline {
                    // Frames stay in Vulkan memory all the way into h264_vulkan.
                    Pipeline::Vulkan { encode_on_gpu: true } => return format!("{input}{}[vout]", pre.join(",")),
                    // One download of the (already scaled) frame.
                    Pipeline::Vulkan { encode_on_gpu: false } => pre.push(format!("hwdownload,format={format}")),
                    _ => {}
                }
            }
            Some(Tonemapper::Opencl) => {
                pre.extend(v.tonemap_input.clone());
                pre.push(OPENCL_TONEMAP.into());
                if scale_needed {
                    pre.push(format!("scale={}:{}:flags=bicubic", v.width, v.height));
                }
            }
            Some(Tonemapper::Zscale) => {
                pre.extend(v.tonemap_input.clone());
                pre.push(if scale_needed { zscale_tonemap(v.width, v.height) } else { ZSCALE_TONEMAP.into() });
            }
            // Done as Quick Sync scaled (only it decodes for it).
            Some(Tonemapper::QuickSync) => {}
            None => {
                if scale_needed {
                    pre.push(format!("scale={}:{}:flags=bicubic", v.width, v.height));
                }
            }
        }
        if let Some(Burn::Text { file_name, fonts_dir, .. }) = &self.burn {
            // libass works on absolute subtitle times; frames are on the local
            // (offset) timeline, so shift around the filter.
            let offset = secs(start_ms);
            let fonts = fonts_dir.as_deref().map(|dir| format!(":fontsdir={}", filter_path(dir))).unwrap_or_default();
            post.push(format!(
                "setpts=PTS+{offset}/TB,subtitles=filename={file_name}{fonts}:original_size={}x{},setpts=PTS-{offset}/TB",
                v.src_width, v.src_height
            ));
        }
        let vulkan = matches!(pipeline, Pipeline::Vulkan { .. });
        match ten_bit {
            _ if encoder.is_some_and(|e| e.accel == HwAccel::Vaapi) => {
                post.push(format!("format={},{VAAPI_UPLOAD}", if ten_bit.is_some() { "p010" } else { "nv12" }));
            }
            // Uploaded to the GPU for AMF (to the filters' device).
            _ if pipeline == Pipeline::Software && amf_upload(v, encoder) => {
                post.push(format!("format={},hwupload", if ten_bit.is_some() { "p010le" } else { "nv12" }));
            }
            // The Vulkan path already delivers NV12, which every encoder
            // accepts; P010 for HDR, which x265 doesn't.
            None if vulkan => {}
            Some("p010le") if vulkan => {}
            Some(format) => post.push(format!("format={format}")),
            None if hardware && v.tonemap == Some(Tonemapper::Libplacebo) => post.push("format=nv12".into()),
            None => post.push("format=yuv420p".into()),
        }

        let join = |parts: &[String]| if parts.is_empty() { "null".to_owned() } else { parts.join(",") };
        // Picture subtitles come on a canvas of their own (1080p on 4K discs):
        // it is fitted to the (scaled) frame's width, keeping its shape, and
        // laid on the bottom of the frame.
        let overlay = |subs: &str| {
            format!(
                "{input}{}[base];{subs}scale={}:-1[subs];[base][subs]overlay=x=(W-w)/2:y=H-h:eof_action=pass:repeatlast=0[ov];[ov]{}[vout]",
                join(&pre),
                v.width,
                join(&post)
            )
        };
        match &self.burn {
            Some(Burn::Image { stream_index, .. }) => overlay(&format!("[0:{stream_index}]")),
            Some(Burn::ImageFile { .. }) => overlay("[1:0]"),
            _ => {
                let mut all = pre;
                all.extend(post);
                format!("{input}{}[vout]", join(&all))
            }
        }
    }
}

pub struct ArgsContext<'a> {
    pub input: &'a Path,
    /// Overrides the planned encoder (software fallback when hardware sessions run out).
    pub encoder: Option<&'a EncoderInfo>,
    pub relative_split_times: bool,
    /// The software encoder's preset for this process (see [`crate::adaptive`]).
    pub preset: Option<&'a str>,
    /// Folder for libplacebo's compiled shaders (see [`shader_cache_option`]).
    pub shader_cache: Option<&'a Path>,
}

/// Subtitle codecs a Matroska file holds as they are.
const MATROSKA_SUBTITLES: [&str; 7] =
    ["subrip", "ass", "ssa", "webvtt", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"];

/// The text subtitles an MP4 conversion makes its own (`mov_text`).
const MP4_TEXT_SUBTITLES: [&str; 6] = ["subrip", "ass", "ssa", "webvtt", "mov_text", "text"];

#[derive(Default)]
struct Args(Vec<OsString>);

impl Args {
    fn push(&mut self, s: &str) {
        self.0.push(s.into());
    }
    fn push_os(&mut self, s: OsString) {
        self.0.push(s);
    }
    fn push_all<'a>(&mut self, items: impl IntoIterator<Item = &'a str>) {
        self.0.extend(items.into_iter().map(OsString::from));
    }
    fn extend(&mut self, items: Vec<String>) {
        self.0.extend(items.into_iter().map(OsString::from));
    }
}

/// Quick Sync's devices where it lays subtitles on (see `Plan::filter_graph`):
/// its device made from Direct3D 11's, on Intel's GPU.
pub const QSV_D3D11_DEVICES: [&str; 12] = [
    "-init_hw_device",
    "d3d11va=dx:,vendor_id=0x8086",
    "-init_hw_device",
    "qsv=qs@dx",
    "-filter_hw_device",
    "qs",
    "-hwaccel",
    "qsv",
    "-hwaccel_device",
    "qs",
    "-hwaccel_output_format",
    "qsv",
];

/// Whether Quick Sync's device is made from VAAPI's (Linux) rather than
/// from Direct3D 11's or by FFmpeg itself (Windows).
pub const QSV_ON_VAAPI: bool = cfg!(not(windows));

/// Quick Sync's device on Linux: VAAPI's on Intel's render node, with
/// Intel's driver, and Quick Sync's made from it (`qs`), which its decoder,
/// filters and encoder then use. FFmpeg's own is on the first render node,
/// whichever GPU that is, and frames map between the two for
/// `tonemap_vaapi`.
pub fn qsv_vaapi_devices(node: &str) -> [String; 4] {
    ["-init_hw_device".into(), format!("vaapi=va:{node},driver=iHD"), "-init_hw_device".into(), "qsv=qs@va".into()]
}

/// Quick Sync's decoder into its own frames, for its scaler: on Windows on
/// the device FFmpeg makes.
pub const QSV_DECODE: [&str; 4] = ["-hwaccel", "qsv", "-hwaccel_output_format", "qsv"];

/// Quick Sync's devices for frames that stay on the GPU: on Linux its
/// device made from VAAPI's ([`qsv_vaapi_devices`]) on `node`, the
/// filters' too; `windows` (one of the lists above) on Windows.
pub fn qsv_gpu_devices(windows: &[&str], node: &str) -> Vec<String> {
    if !QSV_ON_VAAPI {
        return windows.iter().map(|s| s.to_string()).collect();
    }
    let mut args = qsv_vaapi_devices(node).to_vec();
    let decode =
        ["-filter_hw_device", "qs", "-hwaccel", "qsv", "-hwaccel_device", "qs", "-hwaccel_output_format", "qsv"];
    args.extend(decode.map(String::from));
    args
}

/// Quick Sync's rate control besides the bitrates: bitrate control per
/// macroblock, for the picture's sake.
pub const QSV_VBR: [&str; 2] = ["-mbbrc", "1"];
/// Quick Sync's low-power encoder (VDEnc), where its start-up test passed
/// ([`EncoderInfo::low_power`]). On an Iris Xe, 4K HDR10 made 1080p went
/// 13 to 25 % faster beside libplacebo, which it leaves the shader cores
/// to, and 4K SDR 17 % faster; a film scored as high on VMAF or higher.
pub const QSV_LOW_POWER: [&str; 2] = ["-low_power", "1"];
/// VAAPI's rate control: variable, capped by `-maxrate`.
pub const VAAPI_VBR: [&str; 2] = ["-rc_mode", "VBR"];

fn encoder_args(enc: &EncoderInfo, v: &VideoOut, preset: Option<&str>) -> Vec<String> {
    let br = v.bitrate_kbps;
    // `auto` is the transcoder's to choose; one it didn't is a plain default.
    let preset = preset.unwrap_or(if v.preset == AUTO { "veryfast" } else { &v.preset }).to_owned();
    let rate = |s: &mut Vec<String>| {
        s.extend([
            "-b:v".into(),
            format!("{br}k"),
            "-maxrate".into(),
            format!("{br}k"),
            "-bufsize".into(),
            format!("{}k", br * 2),
        ]);
    };
    let h264 = v.codec == VideoCodec::H264;
    // HDR is kept in HEVC Main 10 (see `hdr_output`).
    let profile = match (h264, v.hdr.is_some()) {
        (true, _) => "high",
        (false, false) => "main",
        (false, true) => "main10",
    };
    let mut s: Vec<String> = vec!["-c:v".into(), enc.name.clone()];
    match enc.accel {
        HwAccel::Nvenc => {
            s.extend(
                ["-preset", "p4", "-tune", "hq", "-rc", "vbr", "-cq", if h264 { "23" } else { "26" }].map(String::from),
            );
            rate(&mut s);
            s.extend(["-forced-idr", "1", "-no-scenecut", "1", "-spatial-aq", "1", "-g", "9999"].map(String::from));
            s.extend(["-profile:v", profile].map(String::from));
        }
        HwAccel::Qsv => {
            s.extend(["-preset", "veryfast", "-look_ahead", "0", "-forced_idr", "1", "-g", "9999"].map(String::from));
            if enc.low_power {
                s.extend(QSV_LOW_POWER.map(String::from));
            }
            s.extend(QSV_VBR.iter().map(|o| o.to_string()));
            // A maxrate equal to the bitrate is CBR to FFmpeg (qsvenc.c's
            // select_rc_mode), which spends the bitrate on still pictures
            // too: one bit more makes it VBR, capped at the same rate
            // (Quick Sync counts in kbit/s).
            s.extend([
                "-b:v".into(),
                format!("{br}k"),
                "-maxrate".into(),
                (u64::from(br) * 1000 + 1).to_string(),
                "-bufsize".into(),
                format!("{}k", br * 2),
            ]);
            s.extend(["-profile:v", profile].map(String::from));
        }
        HwAccel::Amf => {
            s.extend(
                ["-usage", "transcoding", "-quality", "balanced", "-rc", "vbr_peak", "-forced_idr", "1", "-g", "9999"]
                    .map(String::from),
            );
            rate(&mut s);
            s.extend(["-profile:v", profile].map(String::from));
        }
        HwAccel::Vaapi => {
            // VBR capped at the bitrate: with the two equal FFmpeg picks
            // CBR (vaapi_encode.c's rate control selection).
            s.extend(VAAPI_VBR.iter().map(|o| o.to_string()));
            rate(&mut s);
            s.extend(["-g", "9999"].map(String::from));
            if v.hdr.is_some() {
                s.extend(["-profile:v", profile].map(String::from));
            }
        }
        HwAccel::Videotoolbox => {
            rate(&mut s);
            s.extend(["-allow_sw", "1", "-realtime", "1", "-g", "9999"].map(String::from));
            if v.hdr.is_some() {
                s.extend(["-profile:v", profile].map(String::from));
            }
        }
        HwAccel::Vulkan => {
            s.extend(["-rc_mode", "vbr", "-tune", "hq", "-usage", "transcode"].map(String::from));
            rate(&mut s);
            // No B-frames: Chrome's decoder never starts on h264_vulkan's
            // (the first frame shows, then playback waits for good).
            s.extend(["-profile:v", "high", "-bf", "0", "-g", "9999"].map(String::from));
        }
        HwAccel::None | HwAccel::Auto => {
            if h264 {
                s.extend(["-preset".into(), preset, "-crf".into(), "21".into()]);
                s.extend(["-maxrate".into(), format!("{br}k"), "-bufsize".into(), format!("{}k", br * 2)]);
                s.extend(
                    ["-profile:v", "high", "-sc_threshold", "0", "-forced-idr", "1", "-g", "9999"].map(String::from),
                );
            } else {
                s.extend(["-preset".into(), preset, "-crf".into(), "24".into()]);
                s.extend(["-maxrate".into(), format!("{br}k"), "-bufsize".into(), format!("{}k", br * 2)]);
                // Parameter sets (and the HDR10 metadata) at every keyframe:
                // each segment starts on one.
                s.extend(
                    ["-x265-params", "log-level=error:scenecut=0:open-gop=0:keyint=9999:repeat-headers=1"]
                        .map(String::from),
                );
                if v.hdr.is_some() {
                    // A source's Dolby Vision metadata stays out.
                    s.extend(["-profile:v", profile, "-dolbyvision", "0"].map(String::from));
                }
            }
        }
    }
    if let Some(hdr) = v.hdr {
        // Said in the stream itself, whatever the frames were tagged with.
        s.extend(
            ["-color_primaries", "bt2020", "-color_trc", hdr.transfer(), "-colorspace", "bt2020nc"].map(String::from),
        );
    }
    s
}

/// `src`'s colour primaries as H.264's VUI codes them (ITU-T H.264, table
/// E-3), where they are known.
fn h264_primaries(src: &MediaStream) -> Option<u8> {
    match src.color_primaries.as_deref()? {
        "bt709" => Some(1),
        "bt470m" => Some(4),
        "bt470bg" => Some(5),
        "smpte170m" => Some(6),
        "smpte240m" => Some(7),
        "film" => Some(8),
        "bt2020" => Some(9),
        _ => None,
    }
}

/// Whether FFmpeg uploads `v`'s frames for AMF: planned so, and AMF encodes
/// them (not a software encoder standing in for it).
fn amf_upload(v: &VideoOut, encoder: Option<&EncoderInfo>) -> bool {
    v.amf_upload && encoder.is_some_and(|e| e.accel == HwAccel::Amf)
}

fn gpu_decoder_name(accel: HwAccel) -> &'static str {
    match accel {
        HwAccel::Vaapi => "vaapi",
        HwAccel::Qsv => "qsv",
        HwAccel::Videotoolbox => "videotoolbox",
        HwAccel::Vulkan => "vulkan",
        _ => "cuda",
    }
}

/// Decoder hwaccel paired with an encoder, only when FFmpeg reports it.
fn pick_hw_decoder(ff: &Ffmpeg, encoder: &EncoderInfo) -> Option<String> {
    let has = |name: &str| ff.caps.hwaccels.iter().any(|h| h == name);
    let nvidia = ff.caps.encoders.iter().any(|e| e.accel == HwAccel::Nvenc);
    let choice = match encoder.accel {
        HwAccel::Nvenc => "cuda",
        HwAccel::Vaapi => "vaapi",
        HwAccel::Videotoolbox => "videotoolbox",
        HwAccel::Qsv | HwAccel::Amf if cfg!(windows) => "d3d11va",
        HwAccel::Qsv => "vaapi",
        _ if nvidia => "cuda",
        _ if cfg!(target_os = "macos") => "videotoolbox",
        _ => return None,
    };
    has(choice).then(|| choice.to_owned())
}

fn resolve_burn(
    sub: &pley_transcoder_api::SubtitleBurnRequest,
    media: &MediaInfo,
    ff: &Ffmpeg,
) -> Result<Burn, String> {
    if let Some(path) = sub.external_path.as_deref().filter(|p| !p.is_empty()) {
        let path = PathBuf::from(path);
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return match ext.as_str() {
            "sup" => {
                if !ff.caps.subtitle_burn_image {
                    return Err("ffmpeg lacks the overlay filter".into());
                }
                Ok(Burn::ImageFile { path, label: name })
            }
            "srt" | "ass" | "ssa" | "vtt" | "sub" | "smi" => {
                if !ff.caps.subtitle_burn_text {
                    return Err("ffmpeg was built without libass (subtitles filter)".into());
                }
                Ok(Burn::Text {
                    source: TextSource::File(path),
                    file_name: "burn.ass".into(),
                    fonts_dir: None,
                    label: name,
                })
            }
            other => Err(format!("unsupported subtitle file type '.{other}'")),
        };
    }
    let index = sub.stream_index.ok_or("subtitle burn-in needs a stream index or a file")?;
    let stream = media
        .stream(index)
        .filter(|s| s.kind == StreamKind::Subtitle)
        .ok_or_else(|| format!("stream {index} is not a subtitle stream"))?;
    let label = format!("{} #{index}", stream.codec);
    if stream.text_based.unwrap_or_else(|| is_text_subtitle_codec(&stream.codec)) {
        if !ff.caps.subtitle_burn_text {
            return Err("ffmpeg was built without libass (subtitles filter)".into());
        }
        Ok(Burn::Text { source: TextSource::Embedded(index), file_name: "burn.ass".into(), fonts_dir: None, label })
    } else {
        if !ff.caps.subtitle_burn_image {
            return Err("ffmpeg lacks the overlay filter".into());
        }
        Ok(Burn::Image { stream_index: index, label })
    }
}

fn media_video<'a>(media: &'a MediaInfo, req: &JobRequest) -> Option<&'a MediaStream> {
    let wanted = req.video.as_ref().and_then(|v| v.stream_index);
    match wanted {
        Some(i) => media.stream(i).filter(|s| s.kind == StreamKind::Video),
        None => media.video(),
    }
}

/// A codec as the transcoder's warnings name it.
fn codec_name(codec: AudioCodec) -> &'static str {
    match codec {
        AudioCodec::Aac => "AAC",
        AudioCodec::Ac3 => "AC-3",
        AudioCodec::Eac3 => "E-AC-3",
    }
}

fn media_audio<'a>(media: &'a MediaInfo, req: &JobRequest) -> Option<&'a MediaStream> {
    let wanted = req.audio.as_ref().and_then(|a| a.stream_index);
    match wanted {
        Some(i) => media.stream(i).filter(|s| s.kind == StreamKind::Audio).or_else(|| media.default_audio()),
        None => media.default_audio(),
    }
}

/// Fit `w×h` inside the limits keeping aspect ratio, never upscaling, even sizes.
pub fn fit(w: i64, h: i64, max_w: Option<u32>, max_h: Option<u32>) -> (i64, i64) {
    let (w, h) = (w.max(2), h.max(2));
    let mut scale = 1.0f64;
    if let Some(mw) = max_w.filter(|m| *m > 0) {
        scale = scale.min(mw as f64 / w as f64);
    }
    if let Some(mh) = max_h.filter(|m| *m > 0) {
        scale = scale.min(mh as f64 / h as f64);
    }
    let scale = scale.min(1.0);
    let even = |v: f64| ((v / 2.0).round() as i64 * 2).max(2);
    (even(w as f64 * scale), even(h as f64 * scale))
}

fn is_x264_preset(p: &str) -> bool {
    matches!(p, "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" | "medium" | "slow" | "slower" | "veryslow")
}

/// libplacebo's `shader_cache` option for a folder, written for a
/// filtergraph. The value is a prefix of file names, hence the trailing
/// separator. A drive's colon is escaped twice: once for the filter's
/// options, once for the graph. Folders whose names hold other characters
/// that either level reads (`, ; [ ] ' : \`) are left without a cache.
pub fn shader_cache_option(dir: &Path) -> Option<String> {
    let path = dir.to_str()?;
    let path = if cfg!(windows) { path.replace('\\', "/") } else { path.to_owned() };
    let (drive, rest) = match path.as_bytes() {
        [letter, b':', ..] if letter.is_ascii_alphabetic() => (&path[..1], &path[2..]),
        _ => ("", path.as_str()),
    };
    if rest.is_empty() || rest.contains([',', ';', '[', ']', '\'', '"', ':', '\\']) {
        return None;
    }
    let drive = if drive.is_empty() { String::new() } else { format!("{drive}\\\\:") };
    let slash = if rest.ends_with('/') { "" } else { "/" };
    Some(format!("shader_cache={drive}{rest}{slash}"))
}

/// A path as a filter option's value in a filter graph: escaped once for
/// the option (`\`, `'`, `:`), then again for the graph (`\`, `'`, `[`, `]`,
/// `,`, `;`). With forward slashes on Windows, which takes them too.
pub fn filter_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    let path = if cfg!(windows) { path.replace('\\', "/") } else { path.into_owned() };
    let escape = |text: &str, special: &[char]| {
        let mut out = String::with_capacity(text.len() + 8);
        for c in text.chars() {
            if special.contains(&c) {
                out.push('\\');
            }
            out.push(c);
        }
        out
    };
    let option = escape(&path, &['\\', '\'', ':']);
    escape(&option, &['\\', '\'', '[', ']', ',', ';'])
}

fn trim_float(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Fixed-length segments. A trailing sliver shorter than a quarter segment is
/// merged into the previous one.
pub fn uniform_layout(duration_ms: i64, target_ms: i64) -> (Vec<i64>, Vec<i64>) {
    let mut boundaries: Vec<i64> = (0..).map(|k| k * target_ms).take_while(|&t| t < duration_ms).collect();
    if boundaries.len() > 1 && duration_ms - boundaries[boundaries.len() - 1] < target_ms / 4 {
        boundaries.pop();
    }
    boundaries.push(duration_ms);
    let jitter = (target_ms / 4).min(500);
    let split_at = boundaries.iter().map(|&b| (b - jitter).max(0)).collect();
    (boundaries, split_at)
}

/// Segments that start on source keyframes, each at least `target_ms` long.
/// Split points sit halfway between a boundary keyframe and its predecessor.
pub fn keyframe_layout(keyframes: &[i64], duration_ms: i64, target_ms: i64) -> (Vec<i64>, Vec<i64>) {
    let mut boundaries = vec![0i64];
    let mut split_at = vec![0i64];
    let mut previous_kf = 0i64;
    for &kf in keyframes {
        if kf <= 0 || kf >= duration_ms {
            previous_kf = previous_kf.max(kf.min(duration_ms));
            continue;
        }
        let last = *boundaries.last().unwrap_or(&0);
        if kf - last >= target_ms && duration_ms - kf >= target_ms / 4 {
            boundaries.push(kf);
            split_at.push(kf - ((kf - previous_kf) / 2).min(500));
        }
        previous_kf = kf;
    }
    boundaries.push(duration_ms);
    split_at.push(duration_ms);
    (boundaries, split_at)
}

/// Longest nudge past a remux boundary: just more than FFmpeg's back-off of
/// 3/23 s (130 ms).
const SEEK_NUDGE_MS: i64 = 140;

/// Whether FFmpeg seeks this file 3/23 s before the point asked for: it does
/// when a stream has B-frames and the demuxer doesn't seek by presentation
/// time (all but MP4 and MOV), to be sure to land on a keyframe whose
/// decoding timestamp comes first. Unknown B-frames (media info from an
/// older probe) count as B-frames.
pub fn seeks_back(media: &MediaInfo) -> bool {
    let seeks_by_pts = match media.format_name.as_deref() {
        Some(name) => name.split(',').any(|n| n == "mov"),
        None => matches!(media.container.as_deref(), Some("mp4" | "mov" | "m4v")),
    };
    !seeks_by_pts && media.streams.iter().any(|s| s.kind == StreamKind::Video && s.has_b_frames != Some(false))
}

/// Where processes starting at each remux boundary seek. A seek lands on the
/// keyframe at or before its point, and stream copy keeps everything from
/// there, so a seek right at a boundary keyframe that FFmpeg moves back
/// (see [`seeks_back`]) would start one GOP early, in the previous segment.
/// It goes past the back-off then, but no further than halfway to the next
/// keyframe, which must not be the one found. Otherwise 1 ms past the
/// boundary is enough: rounding never selects the previous keyframe.
pub fn remux_seek_points(boundaries: &[i64], keyframes: &[i64], seeks_back: bool) -> Vec<i64> {
    let last = boundaries.len().saturating_sub(1);
    boundaries
        .iter()
        .enumerate()
        .map(|(k, &b)| {
            if k == 0 || k == last {
                return b;
            }
            if !seeks_back {
                return b + 1;
            }
            let next = keyframes.get(keyframes.partition_point(|&kf| kf <= b)).copied().unwrap_or(boundaries[last]);
            b + ((next - b) / 2).clamp(1, SEEK_NUDGE_MS)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::MEDIA_DEMUXERS;
    use pley_transcoder_api::{
        AudioRequest, Capabilities, GpuPipeline, SubtitleBurnRequest, TranscodeOptions, VideoRequest,
    };

    fn ffmpeg_with(encoders: Vec<EncoderInfo>, tonemappers: Vec<Tonemapper>) -> Ffmpeg {
        Ffmpeg {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            caps: Capabilities {
                ffmpeg_available: true,
                encoders,
                tonemappers,
                hwaccels: vec!["cuda".into(), "d3d11va".into()],
                subtitle_burn_image: true,
                subtitle_burn_text: true,
                readrate: true,
                ..Default::default()
            },
            segment_times_relative: true,
        }
    }

    fn x264() -> EncoderInfo {
        EncoderInfo {
            name: "libx264".into(),
            codec: VideoCodec::H264,
            hardware: false,
            accel: HwAccel::None,
            main10: false,
            low_power: false,
        }
    }

    fn nvenc() -> EncoderInfo {
        EncoderInfo {
            name: "h264_nvenc".into(),
            codec: VideoCodec::H264,
            hardware: true,
            accel: HwAccel::Nvenc,
            main10: false,
            low_power: false,
        }
    }

    fn hevc_nvenc() -> EncoderInfo {
        EncoderInfo { name: "hevc_nvenc".into(), codec: VideoCodec::Hevc, main10: true, ..nvenc() }
    }

    fn x265() -> EncoderInfo {
        EncoderInfo { name: "libx265".into(), codec: VideoCodec::Hevc, main10: true, ..x264() }
    }

    fn media(range: DynamicRange, dv_profile: Option<i64>) -> MediaInfo {
        MediaInfo {
            container: Some("mkv".into()),
            duration_ms: Some(60_000),
            streams: vec![
                MediaStream {
                    index: 0,
                    kind: StreamKind::Video,
                    codec: "hevc".into(),
                    width: Some(3840),
                    height: Some(2160),
                    dynamic_range: Some(range),
                    dv_profile,
                    dv_bl_compat: dv_profile.map(|p| if p == 5 { 0 } else { 1 }),
                    ..Default::default()
                },
                MediaStream {
                    index: 1,
                    kind: StreamKind::Audio,
                    codec: "eac3".into(),
                    channels: Some(6),
                    sample_rate: Some(48_000),
                    default: true,
                    ..Default::default()
                },
                MediaStream {
                    index: 2,
                    kind: StreamKind::Subtitle,
                    codec: "hdmv_pgs_subtitle".into(),
                    text_based: Some(false),
                    ..Default::default()
                },
                MediaStream {
                    index: 3,
                    kind: StreamKind::Subtitle,
                    codec: "subrip".into(),
                    text_based: Some(true),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn request() -> JobRequest {
        JobRequest {
            input: "/media/movie.mkv".into(),
            video: Some(VideoRequest { max_height: Some(1080), ..Default::default() }),
            audio: Some(AudioRequest::default()),
            options: TranscodeOptions::default(),
            ..Default::default()
        }
    }

    /// Quick Sync's decoder into its own frames, as jobs here ask for it: on
    /// Linux on its device made from VAAPI's.
    const QSV_DECODE_ARGS: &str = if QSV_ON_VAAPI {
        "-hwaccel qsv -hwaccel_device qs -hwaccel_output_format qsv"
    } else {
        "-hwaccel qsv -hwaccel_output_format qsv"
    };

    /// Quick Sync's device on Linux, on the render node the tests' computer
    /// has (none: the default).
    const QSV_VAAPI_DEVICES: &str = "-init_hw_device vaapi=va:/dev/dri/renderD128,driver=iHD -init_hw_device qsv=qs@va";

    fn args_string(plan: &Plan, start: u32, end: u32) -> String {
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: None,
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        plan.ffmpeg_args(&ctx, start, end)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_conversion_is_one_matroska_file_of_the_whole_media() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.options.throttle = false;
        let mut media = media(DynamicRange::Sdr, None);
        media.streams[1].language = Some("eng".into());
        (media.streams[3].language, media.streams[3].title) = (Some("fre".into()), Some("Forced".into()));
        media.streams.push(MediaStream {
            index: 4,
            kind: StreamKind::Subtitle,
            codec: "dvb_teletext".into(),
            ..Default::default()
        });
        media.streams.push(MediaStream {
            index: 5,
            kind: StreamKind::Subtitle,
            codec: "mov_text".into(),
            text_based: Some(true),
            ..Default::default()
        });
        let plan = Plan::build(&req, &media, &ff, None).unwrap();
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: None,
            relative_split_times: true,
            preset: Some("fast"),
            shader_cache: None,
        };
        let args: Vec<String> = plan
            .file_args(&ctx, &media, Path::new("/work/conversions/abc.mkv.part"), FileContainer::Matroska)
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let line = args.join(" ");
        assert!(line.ends_with("-f matroska /work/conversions/abc.mkv.part"), "{line}");
        assert!(line.contains("-c:v libx264") && line.contains("-preset fast"), "{line}");
        assert!(line.contains("-c:a aac"), "{line}");
        // The subtitles as they are, in their order; teletext has no place in Matroska.
        assert!(
            line.contains(
                "-map 0:2 -c:s:0 copy -map 0:3 -c:s:1 copy -metadata:s:s:1 language=fre -metadata:s:s:1 title=Forced"
            ),
            "{line}"
        );
        assert!(line.contains("-map 0:5 -c:s:2 srt -f matroska"), "{line}");
        assert!(line.contains("-metadata:s:a:0 language=eng"), "{line}");
        for absent in ["-segment", "-ss ", "-t ", "-readrate", "-output_ts_offset"] {
            assert!(!line.contains(absent), "{absent} in {line}");
        }
    }

    #[test]
    fn a_conversion_to_mp4_makes_text_subtitles_its_own_and_tags_hevc_for_quicktime() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut media = media(DynamicRange::Sdr, None);
        media.streams.push(MediaStream {
            index: 4,
            kind: StreamKind::Subtitle,
            codec: "ass".into(),
            text_based: Some(true),
            ..Default::default()
        });
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: None,
            relative_split_times: true,
            preset: Some("fast"),
            shader_cache: None,
        };
        let line = |req: &JobRequest| {
            let plan = Plan::build(req, &media, &ff, Some(&[0])).unwrap();
            plan.file_args(&ctx, &media, Path::new("/work/conversions/abc.mp4.part"), FileContainer::Mp4)
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        };

        // The HEVC copied: tagged hvc1, as QuickTime wants it.
        let mut copy = request();
        copy.video.as_mut().unwrap().mode = VideoMode::Copy;
        let copied = line(&copy);
        assert!(copied.contains("-c:v copy"), "{copied}");
        assert!(copied.ends_with("-tag:v hvc1 -movflags +faststart -f mp4 /work/conversions/abc.mp4.part"), "{copied}");
        // SubRip and ASS made MP4's own text, in their order; the PGS pictures have no place.
        assert!(!copied.contains("0:2"), "{copied}");
        assert!(copied.contains("-map 0:3 -c:s:0 mov_text"), "{copied}");
        assert!(copied.contains("-map 0:4 -c:s:1 mov_text"), "{copied}");

        // Made H.264: no tag to change.
        let made = line(&request());
        assert!(made.contains("-c:v libx264"), "{made}");
        assert!(!made.contains("hvc1"), "{made}");
        assert!(made.ends_with("-movflags +faststart -f mp4 /work/conversions/abc.mp4.part"), "{made}");
    }

    #[test]
    fn uniform_layout_merges_tail() {
        let (b, s) = uniform_layout(10_500, 4_000);
        assert_eq!(b, vec![0, 4_000, 8_000, 10_500]);
        assert_eq!(s, vec![0, 3_500, 7_500, 10_000]);
        let (b, _) = uniform_layout(8_400, 4_000);
        assert_eq!(b, vec![0, 4_000, 8_400]);
    }

    #[test]
    fn keyframe_layout_groups_short_gops() {
        let kf = vec![0, 2_002, 4_004, 6_006, 8_008, 10_010, 12_012];
        let (b, s) = keyframe_layout(&kf, 13_000, 4_000);
        assert_eq!(b, vec![0, 4_004, 8_008, 13_000]);
        assert_eq!(s[1], 4_004 - 500);
        let kf = vec![0, 9_000, 18_000];
        let (b, _) = keyframe_layout(&kf, 20_000, 4_000);
        assert_eq!(b, vec![0, 9_000, 18_000, 20_000]);
    }

    #[test]
    fn segment_counts_are_bounded() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut m = media(DynamicRange::Sdr, None);
        // Damaged or crafted files: 31 years, 31 700 years.
        for secs in [1_000_000_000i64, 1_000_000_000_000] {
            m.duration_ms = Some(secs * 1000);
            let err = Plan::build(&request(), &m, &ff, None).unwrap_err();
            assert!(err.contains("not plausible"), "{err}");
        }
        // Nine days in 1 s segments would be 800 000 of them: segments get
        // longer instead.
        m.duration_ms = Some(MAX_DURATION_MS);
        let mut req = request();
        req.segment_seconds = Some(1.0);
        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        assert_eq!(i64::from(plan.segment_count()), MAX_SEGMENTS);
        assert_eq!(plan.segment_seconds, 4.0);
        assert!(args_string(&plan, 0, 2).contains("-force_key_frames expr:gte(t,n_forced*4)"));
        // Ordinary lengths keep the segments they asked for.
        m.duration_ms = Some(3 * 3_600_000);
        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        assert_eq!((plan.segment_seconds, plan.segment_count()), (1.0, 10_800));
    }

    #[test]
    fn segment_lookup() {
        let (boundaries, split_at) = uniform_layout(10_000, 4_000);
        let plan = Plan {
            duration_ms: 10_000,
            seek_at: boundaries.clone(),
            boundaries,
            split_at,
            segment_seconds: 4.0,
            video: None,
            audio: None,
            burn: None,
            throttle: false,
            catchup: false,
            warnings: vec![],
            segment_format: SegmentFormat::Ts,
        };
        assert_eq!(plan.segment_for(0), 0);
        assert_eq!(plan.segment_for(3_999), 0);
        assert_eq!(plan.segment_for(4_000), 1);
        assert_eq!(plan.segment_for(99_999), 2);
    }

    #[test]
    fn fits_within_limits() {
        assert_eq!(fit(3840, 2160, None, Some(1080)), (1920, 1080));
        assert_eq!(fit(1920, 800, Some(1280), Some(720)), (1280, 534));
        assert_eq!(fit(640, 480, None, Some(1080)), (640, 480));
        assert_eq!(fit(1440, 1080, Some(1920), Some(1080)), (1440, 1080));
        // Films wider than 16:9 fit the box by their width.
        assert_eq!(fit(3840, 2024, Some(1920), Some(1080)), (1920, 1012));
        assert_eq!(fit(3840, 1600, Some(1920), Some(1080)), (1920, 800));
    }

    #[test]
    fn hdr10_transcode_uses_nvenc_and_tonemapping() {
        let ff = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Libplacebo, Tonemapper::Zscale]);
        let plan = Plan::build(&request(), &media(DynamicRange::Hdr10, None), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!(v.encoder.as_ref().unwrap().name, "h264_nvenc");
        assert_eq!(v.tonemap, Some(Tonemapper::Libplacebo));
        assert_eq!(v.hw_decode.as_deref(), Some("cuda"));
        assert_eq!((v.width, v.height), (1920, 1080));
        let a = plan.audio.as_ref().unwrap();
        assert!(!a.copy);
        assert_eq!(a.channels, 2);

        let args = args_string(&plan, 3, 8);
        assert!(args.contains("-init_hw_device vulkan=vk -filter_hw_device vk"));
        assert!(args.contains("-hwaccel cuda"));
        // Only media demuxers may open the file (no playlists pointing elsewhere).
        assert!(
            args.contains(&format!("-ss 12.000 -format_whitelist {MEDIA_DEMUXERS} -i file:/media/movie.mkv")),
            "{args}"
        );
        assert!(args.contains("-output_ts_offset 12.000"));
        assert!(args.contains("libplacebo=w=1920:h=1080"));
        // By the stream's metadata: every process makes the same frames.
        assert!(args.contains("range=tv:tonemapping=auto:peak_detect=0"), "{args}");
        assert!(args.contains("-c:v h264_nvenc"));
        assert!(args.contains("-force_key_frames expr:gte(t,n_forced*4)"));
        // Splits for boundaries 16,20,24,28 s relative to the 12 s start, 0.5 s early.
        assert!(args.contains("-segment_times 3.500,7.500,11.500,15.500"), "{args}");
        assert!(args.contains("-segment_start_number 3"));
        assert!(args.contains("-t 20.000"));
        assert!(args.contains("-readrate 1 -readrate_initial_burst 120"));
    }

    #[test]
    fn libplacebo_reuses_compiled_shaders() {
        // A prefix of file names in the folder; the drive's colon escaped for
        // the filter's options and again for the graph.
        assert_eq!(
            shader_cache_option(Path::new("C:/Users/Jo Smith/pley-transcoder/libplacebo")).as_deref(),
            Some(r"shader_cache=C\\:/Users/Jo Smith/pley-transcoder/libplacebo/")
        );
        assert_eq!(
            shader_cache_option(Path::new("/var/cache/pley-transcoder/libplacebo/")).as_deref(),
            Some("shader_cache=/var/cache/pley-transcoder/libplacebo/")
        );
        if cfg!(windows) {
            assert_eq!(
                shader_cache_option(Path::new(r"D:\cache\libplacebo")).as_deref(),
                Some(r"shader_cache=D\\:/cache/libplacebo/")
            );
        }
        // What the graph or the options would read: no cache rather than a failed job.
        for odd in ["/a,b/libplacebo", "/a;b", "/[x]", "/it's", "/a:b", "C:/a:b", ""] {
            assert_eq!(shader_cache_option(Path::new(odd)), None, "{odd}");
        }
        if !cfg!(windows) {
            assert_eq!(shader_cache_option(Path::new(r"/a\b")), None, "a backslash in a name");
        }

        let ff = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Libplacebo]);
        let plan = Plan::build(&request(), &media(DynamicRange::Hdr10, None), &ff, None).unwrap();
        let cache = Path::new("/var/cache/pley-transcoder/libplacebo");
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: None,
            relative_split_times: true,
            preset: None,
            shader_cache: Some(cache),
        };
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(
            args.join(" ")
                .contains("tonemapping=auto:peak_detect=0:shader_cache=/var/cache/pley-transcoder/libplacebo/,"),
            "{args:?}"
        );
        // Other tone mappers have no such cache.
        let ff = ffmpeg_with(vec![x264()], vec![Tonemapper::Zscale]);
        let plan = Plan::build(&request(), &media(DynamicRange::Hdr10, None), &ff, None).unwrap();
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(!args.join(" ").contains("shader_cache"));
    }

    fn ten_bit(mut m: MediaInfo) -> MediaInfo {
        m.streams[0].bit_depth = Some(10);
        m
    }

    #[test]
    fn hdr_tone_maps_on_vulkan_and_encodes_with_the_usual_encoder() {
        let mut ff = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.vulkan_decode = vec!["hevc10".into()];
        ff.caps.vulkan_encode = true;
        let m = ten_bit(media(DynamicRange::DolbyVision, Some(5)));

        // Automatic: decoded and tone mapped on Vulkan, encoded with NVENC
        // after one download (browsers don't play h264_vulkan's 4K).
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Vulkan { encode_on_gpu: false });
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-hwaccel vulkan -hwaccel_device vk -hwaccel_output_format vulkan"), "{args}");
        assert!(args.contains("tonemapping=auto:peak_detect=0,hwdownload,format=nv12[vout]"), "{args}");
        assert!(args.contains("-c:v h264_nvenc"), "{args}");
        assert_eq!(plan.video_plan(&m, None).unwrap().pipeline.as_deref(), Some("hybrid"));

        // Asked for Vulkan: everything stays on it.
        let mut req = request();
        req.options.hwaccel = HwAccel::Vulkan;
        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!(v.pipeline, Pipeline::Vulkan { encode_on_gpu: true });
        assert_eq!(v.encoder.as_ref().unwrap().name, "h264_vulkan");
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-hwaccel vulkan -hwaccel_device vk -hwaccel_output_format vulkan"), "{args}");
        assert!(args.contains("libplacebo=w=1920:h=1080:format=nv12"));
        assert!(!args.contains("hwdownload"));
        assert!(args.contains("-c:v h264_vulkan -rc_mode vbr"));
        assert!(args.contains("-bf 0"), "browsers don't play its B-frames: {args}");
        assert_eq!(plan.video_plan(&m, None).unwrap().pipeline.as_deref(), Some("gpu"));

        // An explicit NVENC preference keeps NVENC: one download after tone mapping.
        let mut req = request();
        req.options.hwaccel = HwAccel::Nvenc;
        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Vulkan { encode_on_gpu: false });
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("tonemapping=auto:peak_detect=0,hwdownload,format=nv12[vout]"), "{args}");
        assert!(args.contains("-c:v h264_nvenc"));
        assert_eq!(plan.video_plan(&m, None).unwrap().pipeline.as_deref(), Some("hybrid"));

        // Codecs that failed the self-test stay on the copy-through path.
        let plan = Plan::build(&request(), &media(DynamicRange::Hdr10, None), &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Software);
    }

    /// Quick Sync encodes in low-power mode where its start-up test passed.
    #[test]
    fn quick_sync_encodes_in_low_power_mode_where_it_passed() {
        let args = |low_power: bool| {
            let enc = EncoderInfo {
                name: "h264_qsv".into(),
                codec: VideoCodec::H264,
                hardware: true,
                accel: HwAccel::Qsv,
                main10: false,
                low_power,
            };
            let plan = Plan::build(&request(), &media(DynamicRange::Sdr, None), &ffmpeg_with(vec![enc], vec![]), None)
                .unwrap();
            args_string(&plan, 0, 2)
        };
        assert!(
            args(true)
                .contains("-c:v h264_qsv -preset veryfast -look_ahead 0 -forced_idr 1 -g 9999 -low_power 1 -mbbrc 1")
        );
        assert!(!args(false).contains("-low_power"));
    }

    /// Each encoder's rate control: capped at the bitrate, variable below it.
    #[test]
    fn hardware_encoders_vary_their_bitrate_under_the_cap() {
        let line = |name: &str, accel: HwAccel| {
            let enc = EncoderInfo {
                name: name.into(),
                codec: VideoCodec::H264,
                hardware: true,
                accel,
                main10: false,
                low_power: false,
            };
            let plan = Plan::build(&request(), &media(DynamicRange::Sdr, None), &ffmpeg_with(vec![enc], vec![]), None)
                .unwrap();
            assert_eq!(plan.video.as_ref().unwrap().bitrate_kbps, 8000);
            let args = args_string(&plan, 0, 2);
            let from = args.find("-c:v").unwrap();
            args[from..args.find(" -force_key_frames").unwrap()].to_owned()
        };
        // NVENC as it was: VBR by quality (-cq), capped.
        assert_eq!(
            line("h264_nvenc", HwAccel::Nvenc),
            "-c:v h264_nvenc -preset p4 -tune hq -rc vbr -cq 23 -b:v 8000k -maxrate 8000k -bufsize 16000k \
             -forced-idr 1 -no-scenecut 1 -spatial-aq 1 -g 9999 -profile:v high"
        );
        // Quick Sync: a maxrate one bit above the bitrate is VBR, equal is CBR.
        assert_eq!(
            line("h264_qsv", HwAccel::Qsv),
            "-c:v h264_qsv -preset veryfast -look_ahead 0 -forced_idr 1 -g 9999 -mbbrc 1 \
             -b:v 8000k -maxrate 8000001 -bufsize 16000k -profile:v high"
        );
        // VAAPI: VBR said outright (equal rates would be CBR).
        assert_eq!(
            line("h264_vaapi", HwAccel::Vaapi),
            "-c:v h264_vaapi -rc_mode VBR -b:v 8000k -maxrate 8000k -bufsize 16000k -g 9999"
        );
        // AMF's own peak-constrained VBR, and VideoToolbox, as they were.
        assert_eq!(
            line("h264_amf", HwAccel::Amf),
            "-c:v h264_amf -usage transcoding -quality balanced -rc vbr_peak -forced_idr 1 -g 9999 \
             -b:v 8000k -maxrate 8000k -bufsize 16000k -profile:v high"
        );
        assert_eq!(
            line("h264_videotoolbox", HwAccel::Videotoolbox),
            "-c:v h264_videotoolbox -b:v 8000k -maxrate 8000k -bufsize 16000k -allow_sw 1 -realtime 1 -g 9999"
        );
    }

    /// The bitrate asked for is the one aimed for, whatever the size;
    /// without one, the size's.
    #[test]
    fn the_bitrate_asked_for_is_aimed_for() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let kbps = |max_height, max_bitrate_kbps| {
            let mut req = request();
            req.video = Some(VideoRequest { max_height, max_bitrate_kbps, ..Default::default() });
            Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap().video.unwrap().bitrate_kbps
        };
        assert_eq!(kbps(Some(1080), None), 8_000);
        assert_eq!(kbps(Some(1080), Some(19_808)), 19_808, "20 Mbps 1080p, not 1080p's 8");
        assert_eq!(kbps(None, Some(29_808)), 29_808, "30 Mbps 4K, not 4K's 20");
        assert_eq!(kbps(Some(480), Some(1_808)), 1_808);
        assert_eq!(kbps(Some(720), Some(2_000)), 2_000, "less than the size's");
        assert_eq!(kbps(Some(1080), Some(100)), 300, "never below 300 kbps");
    }

    #[test]
    fn sdr_uses_cuda_zero_copy_and_falls_back_with_software_encoder() {
        let mut ff = ffmpeg_with(vec![nvenc(), x264()], vec![]);
        ff.caps.gpu_pipelines = vec![pley_transcoder_api::GpuPipeline {
            accel: HwAccel::Nvenc,
            codecs: vec!["hevc".into()],
            ..Default::default()
        }];
        let plan = Plan::build(&request(), &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Gpu(HwAccel::Nvenc));
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-hwaccel cuda -hwaccel_output_format cuda"), "{args}");
        assert!(args.contains("[0:0]scale_cuda=w=1920:h=1080:format=nv12[vout]"));
        assert!(args.contains("-c:v h264_nvenc"));

        // Hardware sessions exhausted → software encoder → frames to
        // memory, scaled by the GPU first.
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&ff.caps.encoders[1]),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let args = args.join(" ");
        assert!(args.contains("-hwaccel cuda -hwaccel_output_format cuda -extra_hw_frames 8"), "{args}");
        assert!(
            args.contains("[0:0]scale_cuda=w=1920:h=1080:format=nv12,hwdownload,format=nv12,format=yuv420p[vout]"),
            "{args}"
        );
        assert!(!args.contains("flags=bicubic"), "{args}");
        assert!(args.contains("-c:v libx264"));

        // Burn-in needs CPU frames: scaled by the GPU, the subtitles drawn
        // on the 1080p frame.
        let mut req = request();
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(3), external_path: None });
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.gpu_scale), (Pipeline::Software, Some(HwAccel::Nvenc)));
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-hwaccel cuda -hwaccel_output_format cuda -extra_hw_frames 8"), "{args}");
        assert!(
            args.contains(
                "[0:0]scale_cuda=w=1920:h=1080:format=nv12,hwdownload,format=nv12,setpts=PTS+0.000/TB,subtitles="
            ),
            "{args}"
        );
        assert!(args.contains("-c:v h264_nvenc"), "{args}");
        // Picture subtitles go over the frame the GPU scaled.
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(2), external_path: None });
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        let args = args_string(&plan, 0, 2);
        assert!(
            args.contains(
                "[0:0]scale_cuda=w=1920:h=1080:format=nv12,hwdownload,format=nv12[base];[0:2]scale=1920:-1[subs]"
            ),
            "{args}"
        );

        // Not at the source's size: nothing to scale, frames copied as they were.
        let mut small = media(DynamicRange::Sdr, None);
        (small.streams[0].width, small.streams[0].height) = (Some(1920), Some(1080));
        let plan = Plan::build(&req, &small, &ff, None).unwrap();
        let args = args_string(&plan, 0, 2);
        assert!(!args.contains("hwaccel_output_format") && !args.contains("scale_cuda"), "{args}");
        // Nor a codec the GPU pipeline didn't pass, nor interlaced video,
        // which is deinterlaced at its own size first.
        let mut h264 = media(DynamicRange::Sdr, None);
        h264.streams[0].codec = "h264".into();
        let mut interlaced = media(DynamicRange::Sdr, None);
        interlaced.streams[0].interlaced = Some(true);
        for m in [h264, interlaced] {
            let plan = Plan::build(&req, &m, &ff, None).unwrap();
            assert_eq!(plan.video.as_ref().unwrap().gpu_scale, None);
            let args = args_string(&plan, 0, 2);
            assert!(!args.contains("hwaccel_output_format") && !args.contains("scale_cuda"), "{args}");
            assert!(args.contains("scale=1920:1080:flags=bicubic"), "{args}");
        }
    }

    /// HDR tone mapped by zimg or OpenCL comes to memory: the GPU scales it
    /// in 10 bits first, and the tone mapping works at the output's size.
    #[test]
    fn hdr_tone_mapped_on_the_cpu_is_scaled_by_the_gpu_first() {
        let mut ff = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Zscale]);
        ff.caps.gpu_pipelines = vec![pley_transcoder_api::GpuPipeline {
            accel: HwAccel::Nvenc,
            codecs: vec!["hevc10".into()],
            ..Default::default()
        }];
        let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!(
            (v.pipeline, v.tonemap, v.gpu_scale),
            (Pipeline::Software, Some(Tonemapper::Zscale), Some(HwAccel::Nvenc))
        );
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-hwaccel cuda -hwaccel_output_format cuda -extra_hw_frames 8"), "{args}");
        assert!(
            args.contains(&format!(
                "[0:0]scale_cuda=w=1920:h=1080:format=p010le,hwdownload,format=p010le,setparams=color_trc=smpte2084\
                 :color_primaries=bt2020:colorspace=bt2020nc,{ZSCALE_TONEMAP},format=yuv420p[vout]"
            )),
            "{args}"
        );

        // VAAPI (Intel and AMD graphics on Linux): its scaler makes 8 bits
        // only here, so HDR for OpenCL's tone mapping comes to memory whole.
        let ff = intel(HwAccel::Vaapi, false);
        let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.tonemap, v.gpu_scale), (Some(Tonemapper::Opencl), None));
        let args = args_string(&plan, 0, 2);
        assert!(!args.contains("hwaccel_output_format") && !args.contains("scale_vaapi"), "{args}");
        assert!(args.contains("tonemap_opencl=") && args.contains("scale=1920:1080:flags=bicubic"), "{args}");
        // SDR with burned-in subtitles: scaled by VAAPI, uploaded back for its encoder.
        let mut req = request();
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(3), external_path: None });
        let plan = Plan::build(&req, &ten_bit(media(DynamicRange::Sdr, None)), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.gpu_scale), (Pipeline::Software, Some(HwAccel::Vaapi)));
        let args = args_string(&plan, 0, 2);
        assert!(
            args.contains("-vaapi_device /dev/dri/renderD128 -hwaccel vaapi -hwaccel_output_format vaapi"),
            "{args}"
        );
        assert!(args.contains("[0:0]scale_vaapi=w=1920:h=1080:format=nv12,hwdownload,format=nv12,setpts="), "{args}");
        assert!(args.contains("format=nv12,hwupload=derive_device=vaapi[vout]"), "{args}");

        // libplacebo scales as it tone maps: its frames are left alone.
        let mut ff = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.gpu_pipelines = vec![pley_transcoder_api::GpuPipeline {
            accel: HwAccel::Nvenc,
            codecs: vec!["hevc10".into()],
            ..Default::default()
        }];
        let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.hw_decode.as_deref(), v.gpu_scale), (Pipeline::Software, Some("cuda"), None));
        assert!(!args_string(&plan, 0, 2).contains("hwaccel_output_format"));
    }

    /// What an Intel N100 (or any recent Intel graphics) passes at start-up:
    /// Quick Sync's encoders on Windows (`Qsv`) or VAAPI's on Linux, their
    /// zero-copy pipeline for the codecs they decode, and Vulkan's decoder
    /// with libplacebo on the GPU (unless `vulkan` is false: an older
    /// driver), OpenCL and zimg.
    fn intel(accel: HwAccel, vulkan: bool) -> Ffmpeg {
        let prefix = if accel == HwAccel::Qsv { "qsv" } else { "vaapi" };
        let hw = |codec: VideoCodec, main10: bool| EncoderInfo {
            name: format!("{}_{prefix}", if codec == VideoCodec::H264 { "h264" } else { "hevc" }),
            codec,
            hardware: true,
            accel,
            main10,
            low_power: false,
        };
        let mappers = [Tonemapper::Libplacebo, Tonemapper::Opencl, Tonemapper::Zscale];
        let mappers = mappers.into_iter().filter(|m| vulkan || *m != Tonemapper::Libplacebo).collect();
        let mut ff =
            ffmpeg_with(vec![hw(VideoCodec::H264, false), hw(VideoCodec::Hevc, true), x264(), x265()], mappers);
        ff.caps.hwaccels = ["d3d11va", "qsv", "vaapi", "vulkan"].map(String::from).to_vec();
        let codecs: Vec<String> = ["h264", "hevc", "hevc10"].map(String::from).to_vec();
        ff.caps.gpu_pipelines =
            vec![pley_transcoder_api::GpuPipeline { accel, codecs: codecs.clone(), ..Default::default() }];
        if vulkan {
            ff.caps.vulkan_decode = codecs.clone();
        }
        if accel == HwAccel::Vaapi {
            // The tone mappers' frames reach VAAPI's encoder from devices made from its.
            ff.caps.vaapi_tonemappers = ff.caps.tonemappers.clone();
            if vulkan {
                ff.caps.vaapi_vulkan_decode = codecs;
            }
        }
        ff
    }

    #[test]
    fn intel_graphics_keep_sdr_on_the_gpu_and_tone_map_on_vulkan() {
        for accel in [HwAccel::Qsv, HwAccel::Vaapi] {
            let ff = intel(accel, true);
            let encoder = if accel == HwAccel::Qsv { "h264_qsv" } else { "h264_vaapi" };

            // SDR: decoded, scaled and encoded without leaving the GPU.
            let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Sdr, None)), &ff, None).unwrap();
            let v = plan.video.as_ref().unwrap();
            assert_eq!(v.pipeline, Pipeline::Gpu(accel));
            let args = args_string(&plan, 0, 2);
            let scaler = if accel == HwAccel::Qsv {
                "vpp_qsv=w=1920:h=1080:format=nv12"
            } else {
                "scale_vaapi=w=1920:h=1080:format=nv12"
            };
            assert!(args.contains(&format!("[0:0]{scaler}[vout]")), "{args}");
            assert!(args.contains(&format!("-c:v {encoder}")), "{args}");

            // HDR10 to SDR: decoded and tone mapped (and scaled) on Vulkan,
            // one download of the 1080p frame to the encoder.
            let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
            let v = plan.video.as_ref().unwrap();
            assert_eq!(
                (v.pipeline, v.tonemap),
                (Pipeline::Vulkan { encode_on_gpu: false }, Some(Tonemapper::Libplacebo))
            );
            let args = args_string(&plan, 0, 2);
            assert!(args.contains("libplacebo=w=1920:h=1080:format=nv12"), "{args}");
            assert!(args.contains("hwdownload,format=nv12"), "{args}");
            assert!(args.contains(&format!("-c:v {encoder}")), "{args}");
            if accel == HwAccel::Vaapi {
                // VAAPI's encoders take frames in VAAPI's memory only: the
                // filters' Vulkan device is made from VAAPI's, and the frames
                // are uploaded to that rather than to Vulkan's.
                assert!(
                    args.contains(
                        "-init_hw_device vaapi=va:/dev/dri/renderD128 -init_hw_device vulkan=vk@va -filter_hw_device vk \
                         -hwaccel vulkan -hwaccel_device vk"
                    ),
                    "{args}"
                );
                assert!(
                    args.contains("hwdownload,format=nv12,format=nv12,hwupload=derive_device=vaapi[vout]"),
                    "{args}"
                );
                assert!(!args.contains("-vaapi_device"), "{args}");
            } else {
                assert!(args.contains("-init_hw_device vulkan=vk -filter_hw_device vk"), "{args}");
                // Quick Sync's device on Linux, for its encoder, made from
                // VAAPI's on Intel's render node; nothing of VAAPI's else.
                let qsv = if QSV_ON_VAAPI { QSV_VAAPI_DEVICES } else { "" };
                assert!(args.contains(&format!("-y {qsv}")), "{args}");
                assert!(!args.replace(qsv, "").contains("vaapi"), "{args}");
            }

            // An older driver without Vulkan: hardware decoding, OpenCL's tone
            // mapping, then the encoder. Quick Sync decodes the frames
            // itself and scales them in 10 bits before they come to memory;
            // VAAPI's scaler makes 8 bits only.
            let ff = intel(accel, false);
            let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
            let v = plan.video.as_ref().unwrap();
            assert_eq!((v.pipeline, v.tonemap), (Pipeline::Software, Some(Tonemapper::Opencl)));
            let decoder = if accel == HwAccel::Qsv { "qsv" } else { "vaapi" };
            assert_eq!(v.hw_decode.as_deref(), Some(decoder));
            let args = args_string(&plan, 0, 2);
            assert!(args.contains(&format!("-hwaccel {decoder}")), "{args}");
            assert!(args.contains(&format!("-c:v {encoder}")), "{args}");
            if decoder == "qsv" {
                assert!(args.contains(QSV_DECODE_ARGS), "{args}");
                // On Linux on Intel's render node, the encoder too.
                assert_eq!(args.contains(QSV_VAAPI_DEVICES), QSV_ON_VAAPI, "{args}");
                assert!(args.contains("[0:0]vpp_qsv=w=1920:h=1080:format=p010le,hwdownload,format=p010le,"), "{args}");
                assert!(!args.contains("scale=1920:1080:flags=bicubic"), "scaled once, on the GPU: {args}");
            }
            if accel == HwAccel::Vaapi {
                // OpenCL's device made from VAAPI's (Intel's OpenCL), the
                // frames uploaded back to it.
                assert!(
                    args.contains(
                        "-init_hw_device vaapi=va:/dev/dri/renderD128 -init_hw_device opencl=ocl@va -filter_hw_device ocl"
                    ),
                    "{args}"
                );
                assert!(args.contains("format=nv12,hwupload=derive_device=vaapi[vout]"), "{args}");
            } else {
                assert!(args.contains("-init_hw_device opencl=ocl -filter_hw_device ocl"), "{args}");
            }
        }
    }

    /// Quick Sync where Vulkan doesn't decode the codec (some Windows
    /// computers' Intel drivers give 10-bit HEVC to neither Vulkan nor
    /// Direct3D 11): it decodes the HDR itself and scales it to the output's
    /// size in 10 bits, and libplacebo tone maps that.
    #[test]
    fn quick_sync_decodes_and_scales_what_libplacebo_tone_maps() {
        let mut ff = intel(HwAccel::Qsv, true);
        ff.caps.vulkan_decode = vec!["h264".into(), "hevc".into()];
        let m = ten_bit(media(DynamicRange::Hdr10, None));
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.tonemap), (Pipeline::Software, Some(Tonemapper::Libplacebo)));
        let args = args_string(&plan, 0, 2);
        assert_eq!((v.hw_decode.as_deref(), v.gpu_scale), (Some("qsv"), Some(HwAccel::Qsv)));
        assert!(args.contains(&format!("-init_hw_device vulkan=vk -filter_hw_device vk {QSV_DECODE_ARGS}")), "{args}");
        if QSV_ON_VAAPI {
            assert!(args.contains(&format!("{QSV_VAAPI_DEVICES} -init_hw_device vulkan=vk")), "{args}");
        }
        assert!(
            args.contains(
                "[0:0]vpp_qsv=w=1920:h=1080:format=p010le,hwdownload,format=p010le,libplacebo=w=1920:h=1080:format=nv12"
            ),
            "{args}"
        );
        // libplacebo reads 1080p, but Quick Sync decodes and scales the
        // 4K on the same GPU: the source's pixels count.
        let need = plan.tonemap_need(&m).unwrap();
        assert!((need - tonemap_need(3840 * 2160, 1920 * 1080, 24.0)).abs() < 1e-9, "{need}");
        // Frames Quick Sync scales stay on the GPU: the CPU never decodes
        // them instead (its frames would meet vpp_qsv in memory).
        assert!(!plan.decodes_on_shared_engine());

        // Dolby Vision and HDR10+ keep FFmpeg's own decoder, which reads
        // their metadata for libplacebo: profile 5's colours are in it.
        let p5 = ten_bit(media(DynamicRange::DolbyVision, Some(5)));
        let mut hdr10plus = ten_bit(media(DynamicRange::Hdr10plus, None));
        hdr10plus.streams[0].hdr10plus = Some(true);
        for source in [&p5, &hdr10plus] {
            for req in [request(), hdr_request(pley_transcoder_api::OutputRange::Hdr10)] {
                let plan = Plan::build(&req, source, &ff, None).unwrap();
                let v = plan.video.as_ref().unwrap();
                if v.tonemap != Some(Tonemapper::Libplacebo) {
                    continue; // HDR10+ kept as it is: nothing for libplacebo to read
                }
                let decoder = if cfg!(windows) { "d3d11va" } else { "vaapi" };
                assert_eq!(
                    (v.pipeline, v.hw_decode.as_deref(), v.gpu_scale),
                    (Pipeline::Software, Some(decoder), None)
                );
                let args = args_string(&plan, 0, 2);
                assert!(!args.contains("-hwaccel qsv") && !args.contains("vpp_qsv"), "{args}");
            }
        }
    }

    /// Picture subtitles are burned in on the CPU. Quick Sync decodes and
    /// scales the video first where its size changes; 8-bit video kept at
    /// its size comes from Direct3D 11's decoder (VAAPI's on Linux), whose
    /// copy to memory costs the CPU less.
    #[test]
    fn quick_sync_scales_video_before_subtitles_are_burned_in() {
        let ff = intel(HwAccel::Qsv, true);
        let mut m = media(DynamicRange::Sdr, None);
        (m.streams[0].codec, m.streams[0].width, m.streams[0].height) = ("h264".into(), Some(1920), Some(1080));
        let burn = |max_height| {
            let mut req = request();
            req.video.as_mut().unwrap().max_height = Some(max_height);
            req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(2), external_path: None });
            Plan::build(&req, &m, &ff, None).unwrap()
        };
        let plan = burn(720);
        let v = plan.video.as_ref().unwrap();
        let args = args_string(&plan, 0, 2);
        assert_eq!((v.pipeline, v.hw_decode.as_deref()), (Pipeline::Software, Some("qsv")));
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(args.contains("[0:0]vpp_qsv=w=1280:h=720:format=nv12,hwdownload,format=nv12[base]"), "{args}");
        let plan = burn(1080);
        let v = plan.video.as_ref().unwrap();
        let decoder = if cfg!(windows) { "d3d11va" } else { "vaapi" };
        assert_eq!((v.pipeline, v.hw_decode.as_deref(), v.gpu_scale), (Pipeline::Software, Some(decoder), None));
    }

    /// Quick Sync lays picture subtitles on 8-bit SDR itself where its
    /// start-up test passed, on its device made from Direct3D 11's (VAAPI's
    /// on Linux): at the source's size, then scaled; on the CPU for x264
    /// standing in.
    #[test]
    fn quick_sync_lays_picture_subtitles_on_without_leaving_the_gpu() {
        let mut ff = intel(HwAccel::Qsv, true);
        ff.caps.qsv_overlay = true;
        let mut m = media(DynamicRange::Sdr, None);
        (m.streams[0].codec, m.streams[0].width, m.streams[0].height) = ("h264".into(), Some(1920), Some(1080));
        let burn = |max_height| {
            let mut req = request();
            req.video.as_mut().unwrap().max_height = Some(max_height);
            req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(2), external_path: None });
            Plan::build(&req, &m, &ff, None).unwrap()
        };
        let plan = burn(720);
        let v = plan.video.as_ref().unwrap();
        assert_eq!(v.pipeline, Pipeline::Gpu(HwAccel::Qsv));
        let args = args_string(&plan, 0, 2);
        let devices = if QSV_ON_VAAPI {
            QSV_VAAPI_DEVICES
        } else {
            "-init_hw_device d3d11va=dx:,vendor_id=0x8086 -init_hw_device qsv=qs@dx"
        };
        let decode = "-filter_hw_device qs -hwaccel qsv -hwaccel_device qs -hwaccel_output_format qsv";
        assert!(args.contains(&format!("{devices} {decode}")), "{args}");
        assert!(
            args.contains(
                "[0:2]scale=1920:-1,format=bgra,hwupload=extra_hw_frames=16[subs];[0:0][subs]overlay_qsv=\
                 x=(main_w-overlay_w)/2:y=main_h-overlay_h:eof_action=repeat:repeatlast=1,\
                 vpp_qsv=w=1280:h=720:format=nv12[vout]"
            ),
            "{args}"
        );
        // Kept at its size: no scaler after.
        let args = args_string(&burn(1080), 0, 2);
        assert!(args.contains("eof_action=repeat:repeatlast=1[vout]"), "{args}");

        // x264 standing in: scaled by Quick Sync, laid on in memory.
        let x264 = x264();
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&x264),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let args = args.join(" ");
        assert!(!args.contains("overlay_qsv"), "{args}");
        assert!(args.contains("[0:0]vpp_qsv=w=1280:h=720:format=nv12,hwdownload,format=nv12[base]"), "{args}");

        // Without the start-up test passed, as before.
        ff.caps.qsv_overlay = false;
        let mut req = request();
        req.video.as_mut().unwrap().max_height = Some(720);
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(2), external_path: None });
        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Software);
    }

    /// Where the GPU falls behind, Quick Sync's scaler tone maps HDR10 that
    /// it decodes and scales, in libplacebo's place: on the GPU into its own
    /// encoder, to memory for x264 standing in or for burned-in subtitles.
    #[test]
    fn quick_sync_tone_maps_hdr10_in_libplacebos_place() {
        let mut ff = intel(HwAccel::Qsv, true);
        // Vulkan doesn't decode 10-bit HEVC (Iris Xe's driver doesn't).
        ff.caps.vulkan_decode = vec!["h264".into(), "hevc".into()];
        let mut m = ten_bit(media(DynamicRange::Hdr10, None));
        m.streams[0].hdr10_metadata = Some(true);
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert!(!plan.quick_sync_could_tonemap(&m, false));
        ff.caps.qsv_tonemap = true;
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert!(plan.quick_sync_could_tonemap(&m, true));
        let share = Share { quality: TonemapQuality::Fast, cpu_decode: false, quick_sync: true, base_layer: false };
        let intel = plan.with_share(share);
        let v = intel.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.tonemap), (Pipeline::Gpu(HwAccel::Qsv), Some(Tonemapper::QuickSync)));
        let args = args_string(&intel, 0, 2);
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(!args.contains("vulkan") && !args.contains("libplacebo"), "{args}");
        // The test media is untagged: tagged first, Quick Sync leaving
        // unknown colours as they are.
        let mapped = qsv_tonemap(1920, 1080);
        assert!(
            args.contains(&format!(
                "[0:0]setparams=color_trc=smpte2084:color_primaries=bt2020:colorspace=bt2020nc,{mapped}[vout]"
            )),
            "{args}"
        );
        if QSV_ON_VAAPI {
            // VAAPI's curve, the frames mapped to its device and back.
            assert!(
                mapped.ends_with(
                    "procamp=1:brightness=16,hwmap=derive_device=vaapi,format=vaapi,\
                     tonemap_vaapi=format=nv12:p=bt709:t=bt709:m=bt709,hwmap=derive_device=qsv,format=qsv"
                ),
                "{mapped}"
            );
            assert!(args.contains(&format!("{QSV_VAAPI_DEVICES} -filter_hw_device qs {QSV_DECODE_ARGS}")), "{args}");
        } else {
            assert!(
                mapped.ends_with(
                    "procamp=1:brightness=16,vpp_qsv=tonemap=1:format=nv12,\
                     setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709"
                ),
                "{mapped}"
            );
        }
        let shown = intel.video_plan(&m, None).unwrap();
        assert_eq!((shown.tonemap, shown.tonemap_quick_sync, shown.tonemap_quality), (None, true, None));
        assert_eq!(shown.pipeline.as_deref(), Some("gpu"));
        // Not as a `Tonemapper`, which a server older than it can't read.
        let json = serde_json::to_string(&shown).unwrap();
        assert!(json.contains(r#""tonemap":null"#) && !json.contains("vpp_qsv"), "{json}");

        // x264 standing in: the tone mapped frames come to memory.
        let x264 = x264();
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&x264),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let args: Vec<String> =
            intel.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let args = args.join(" ");
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(args.contains(&format!("{mapped},hwdownload,format=nv12,format=yuv420p[vout]")), "{args}");

        // Subtitles burned in over the tone mapped frame, in memory.
        let mut req = request();
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(2), external_path: None });
        let burned = Plan::build(&req, &m, &ff, None).unwrap();
        assert!(burned.quick_sync_could_tonemap(&m, true));
        let burned = burned.with_share(share);
        assert_eq!(burned.video.as_ref().unwrap().pipeline, Pipeline::Software);
        let args = args_string(&burned, 0, 2);
        assert!(args.contains(&format!("{mapped},hwdownload,format=nv12[base]")), "{args}");

        // Not HDR kept, nor HLG: libplacebo's, as planned.
        let kept = Plan::build(&hdr_request(pley_transcoder_api::OutputRange::Hdr10), &m, &ff, None).unwrap();
        assert!(!kept.quick_sync_could_tonemap(&m, true));
        let hlg = ten_bit(media(DynamicRange::Hlg, None));
        let plan = Plan::build(&request(), &hlg, &ff, None).unwrap();
        assert!(!plan.quick_sync_could_tonemap(&hlg, true));

        // Nor libplacebo asked for.
        let mut req = request();
        req.options.tonemap = TonemapMode::Libplacebo;
        assert!(!Plan::build(&req, &m, &ff, None).unwrap().quick_sync_could_tonemap(&m, true));

        // Tagged HDR10 goes to Quick Sync as it is.
        let mut tagged = m.clone();
        tagged.streams[0].color_transfer = Some("smpte2084".into());
        tagged.streams[0].color_primaries = Some("bt2020".into());
        tagged.streams[0].color_space = Some("bt2020nc".into());
        let args = args_string(&Plan::build(&request(), &tagged, &ff, None).unwrap().with_share(share), 0, 2);
        assert!(args.contains("[0:0]vpp_qsv=w=1920:h=1080:format=p010le,vpp_qsv=procamp="), "{args}");

        // Video without HDR10's static metadata, or not known to carry it:
        // Quick Sync's scaler would pass its picture through, untouched.
        for metadata in [Some(false), None] {
            let mut bare = m.clone();
            bare.streams[0].hdr10_metadata = metadata;
            let plan = Plan::build(&request(), &bare, &ff, None).unwrap();
            assert!(!plan.quick_sync_could_tonemap(&bare, true), "{metadata:?}");
        }
    }

    /// Experimental: with the full layer asked for and an FFmpeg that has
    /// `dovi_compose`, the file is opened again for the enhancement layer and
    /// the full picture rebuilt on the CPU, then tone mapped as HDR10.
    #[test]
    fn profile_7s_full_enhancement_layer_is_composed_where_ffmpeg_can() {
        let mut ff = intel(HwAccel::Qsv, true);
        ff.caps.qsv_tonemap = true;
        let mut fel = ten_bit(media(DynamicRange::DolbyVision, Some(7)));
        fel.streams[0].dv_bl_compat = Some(6);
        fel.streams[0].dv_el = Some(DolbyVisionLayer::Fel);
        fel.streams[0].hdr10_metadata = Some(true);
        let mut req = request();
        req.options.dolby_vision_fel_full = true;

        // Not without the filter: the base layer, as before.
        let plan = Plan::build(&req, &fel, &ff, None).unwrap();
        assert!(!plan.video.as_ref().unwrap().fel_compose);
        assert!(!args_string(&plan, 0, 2).contains("dovi_split"));

        ff.caps.dolby_vision_fel_full = true;
        let plan = Plan::build(&req, &fel, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert!(v.fel_compose && v.base_layer_only);
        assert_eq!((v.pipeline, v.hw_decode.as_deref(), v.gpu_scale), (Pipeline::Software, None, None));
        let args = args_string(&plan, 2, 4);
        // Both inputs seek to the same place.
        assert!(args.contains(" -bsf:0 dovi_split=mode=el -format_whitelist"), "{args}");
        assert_eq!(args.matches("-i file:/media/movie.mkv").count(), 2, "{args}");
        assert_eq!(args.matches("-ss ").count(), 2, "{args}");
        assert!(args.contains("[0:0][1:0]dovi_compose[fel];[fel]libplacebo=w=1920:h=1080"), "{args}");
        assert!(args.contains(":apply_dolbyvision=0"), "{args}");
        // Never handed to Quick Sync's tone mapping: its frames are in memory.
        assert!(!plan.quick_sync_could_tonemap(&fel, true));

        // After a picture subtitles' file, the third input.
        let mut burned = req.clone();
        burned.subtitle =
            Some(SubtitleBurnRequest { stream_index: None, external_path: Some("/media/movie.sup".into()) });
        let args = args_string(&Plan::build(&burned, &fel, &ff, None).unwrap(), 0, 2);
        assert!(args.contains("[0:0][2:0]dovi_compose[fel];[fel]"), "{args}");

        // Not asked for, or a minimal layer: as before.
        let plan = Plan::build(&request(), &fel, &ff, None).unwrap();
        assert!(!plan.video.as_ref().unwrap().fel_compose);
        let mut mel = fel.clone();
        mel.streams[0].dv_el = Some(DolbyVisionLayer::Mel);
        assert!(!Plan::build(&req, &mel, &ff, None).unwrap().video.as_ref().unwrap().fel_compose);
    }

    /// Dolby Vision profile 7 with a full enhancement layer is made from its
    /// HDR10 base layer alone unless its metadata is asked for: no FFmpeg
    /// reads the layer, and the metadata's brightness is of the picture with
    /// it. As HDR10, Quick Sync decodes it, and libplacebo is told to leave
    /// the metadata unread whichever decoder brings the frames.
    #[test]
    fn profile_7s_full_enhancement_layer_is_made_from_its_base_layer_unless_its_metadata_is_asked_for() {
        let mut ff = intel(HwAccel::Qsv, true);
        ff.caps.vulkan_decode = vec!["h264".into(), "hevc".into()];
        ff.caps.qsv_tonemap = true;
        let p7 = |layer: Option<DolbyVisionLayer>| {
            let mut m = ten_bit(media(DynamicRange::DolbyVision, Some(7)));
            m.streams[0].dv_bl_compat = Some(6);
            m.streams[0].dv_el = layer;
            m.streams[0].hdr10_metadata = Some(true);
            m
        };
        let fel = p7(Some(DolbyVisionLayer::Fel));
        let own_decoder = if cfg!(windows) { "d3d11va" } else { "vaapi" };

        // The base layer (the default): planned as HDR10 is.
        let plan = Plan::build(&request(), &fel, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert!(v.base_layer_only && !v.qsv_base_layer);
        assert_eq!(
            (v.tonemap, v.hw_decode.as_deref(), v.gpu_scale),
            (Some(Tonemapper::Libplacebo), Some("qsv"), Some(HwAccel::Qsv))
        );
        let args = args_string(&plan, 0, 2);
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(args.contains("tonemapping=auto:peak_detect=0:apply_dolbyvision=0"), "{args}");
        // And tone mapped by Quick Sync where the GPU falls behind.
        assert!(plan.quick_sync_could_tonemap(&fel, true));

        // Its metadata asked for: FFmpeg's own decoder brings libplacebo
        // the frames with it, as for profile 8.1.
        let mut req = request();
        req.options.dolby_vision_fel = DolbyVisionFel::Metadata;
        let plan = Plan::build(&req, &fel, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert!(!v.base_layer_only && v.qsv_base_layer);
        assert_eq!((v.hw_decode.as_deref(), v.gpu_scale), (Some(own_decoder), None));
        assert!(!args_string(&plan, 0, 2).contains("apply_dolbyvision"));

        // A minimal layer's metadata matches its base layer, and a layer
        // not known isn't taken for a full one: read, whatever is asked.
        // Profile 8.1 has no layer.
        let p81 = ten_bit(media(DynamicRange::DolbyVision, Some(8)));
        for other in [p7(Some(DolbyVisionLayer::Mel)), p7(None), p81] {
            let plan = Plan::build(&request(), &other, &ff, None).unwrap();
            let v = plan.video.as_ref().unwrap();
            assert!(!v.base_layer_only);
            assert_eq!((v.hw_decode.as_deref(), v.gpu_scale), (Some(own_decoder), None));
            assert!(!args_string(&plan, 0, 2).contains("apply_dolbyvision"));
        }

        // HDR10+ in the base layer is the base layer's own: FFmpeg's decoder
        // reads it for libplacebo, the Dolby Vision metadata still unread.
        let mut hybrid = fel.clone();
        hybrid.streams[0].hdr10plus = Some(true);
        let plan = Plan::build(&request(), &hybrid, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert!(v.base_layer_only);
        assert_eq!((v.hw_decode.as_deref(), v.gpu_scale), (Some(own_decoder), None));
        assert!(args_string(&plan, 0, 2).contains("peak_detect=0:apply_dolbyvision=0"));

        // On other GPUs, whose decoders read the metadata, and on the CPU:
        // libplacebo leaves it unread all the same.
        let nvidia = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Libplacebo, Tonemapper::Zscale]);
        let software = ffmpeg_with(vec![x264()], vec![Tonemapper::Libplacebo, Tonemapper::Zscale]);
        for ff in [&nvidia, &software] {
            let plan = Plan::build(&request(), &fel, ff, None).unwrap();
            assert!(args_string(&plan, 0, 2).contains("peak_detect=0:apply_dolbyvision=0"));
            let plan = Plan::build(&req, &fel, ff, None).unwrap();
            assert!(!args_string(&plan, 0, 2).contains("apply_dolbyvision"));
        }

        // HDR kept is the base layer already: nothing for libplacebo to read.
        let kept = Plan::build(&hdr_request(pley_transcoder_api::OutputRange::Hdr10), &fel, &ff, None).unwrap();
        assert_eq!(kept.video.as_ref().unwrap().tonemap, None);
    }

    /// Dolby Vision over an HDR10 base layer and HDR10+ keep FFmpeg's own
    /// decoder for libplacebo, which reads their metadata. Where the
    /// computer falls behind, Quick Sync decodes, scales and tone maps the
    /// base layer instead, without the frames leaving the GPU.
    #[test]
    fn quick_sync_takes_over_the_hdr10_base_layer_of_dolby_vision_and_hdr10plus() {
        let mut ff = intel(HwAccel::Qsv, true);
        ff.caps.vulkan_decode = vec!["h264".into(), "hevc".into()];
        ff.caps.qsv_tonemap = true;
        let with_metadata = |mut m: MediaInfo| {
            m.streams[0].hdr10_metadata = Some(true);
            m
        };
        let p81 = with_metadata(ten_bit(media(DynamicRange::DolbyVision, Some(8))));
        let mut p7 = with_metadata(ten_bit(media(DynamicRange::DolbyVision, Some(7))));
        p7.streams[0].dv_bl_compat = Some(6);
        let mut plus = with_metadata(ten_bit(media(DynamicRange::Hdr10plus, None)));
        plus.streams[0].hdr10plus = Some(true);
        let share = Share { quality: TonemapQuality::Fast, cpu_decode: false, quick_sync: true, base_layer: false };
        for source in [&p81, &p7, &plus] {
            let plan = Plan::build(&request(), source, &ff, None).unwrap();
            let v = plan.video.as_ref().unwrap();
            // As planned: FFmpeg's decoder, libplacebo reading the metadata.
            let decoder = if cfg!(windows) { "d3d11va" } else { "vaapi" };
            assert_eq!(
                (v.tonemap, v.hw_decode.as_deref(), v.gpu_scale),
                (Some(Tonemapper::Libplacebo), Some(decoder), None)
            );
            assert!(v.qsv_base_layer && plan.quick_sync_would_decode());
            assert!(plan.quick_sync_could_tonemap(source, true));
            let intel = plan.with_share(share);
            let v = intel.video.as_ref().unwrap();
            assert_eq!((v.pipeline, v.tonemap), (Pipeline::Gpu(HwAccel::Qsv), Some(Tonemapper::QuickSync)));
            assert_eq!((v.hw_decode.as_deref(), v.gpu_scale), (Some("qsv"), Some(HwAccel::Qsv)));
            let args = args_string(&intel, 0, 2);
            assert!(args.contains(QSV_DECODE_ARGS), "{args}");
            assert!(args.contains(&qsv_tonemap(1920, 1080)), "{args}");
            assert!(!args.contains("libplacebo") && !args.contains("d3d11va"), "{args}");
            assert_eq!(intel.video_plan(source, None).unwrap().decoder.as_deref(), Some("qsv"));

            // Where FFmpeg's decoder failed on the GPU, Quick Sync decodes
            // and scales the base layer for libplacebo, as it does HDR10:
            // the CPU doesn't decode in its place.
            let taken = plan.with_share(Share { quick_sync: false, base_layer: true, ..share });
            let v = taken.video.as_ref().unwrap();
            assert_eq!((v.pipeline, v.tonemap), (Pipeline::Software, Some(Tonemapper::Libplacebo)));
            assert_eq!((v.hw_decode.as_deref(), v.gpu_scale), (Some("qsv"), Some(HwAccel::Qsv)));
            let args = args_string(&taken, 0, 2);
            assert!(args.contains(QSV_DECODE_ARGS) && !args.contains("d3d11va"), "{args}");
            assert!(
                args.contains("vpp_qsv=w=1920:h=1080:format=p010le,hwdownload,format=p010le,libplacebo=w=1920:h=1080"),
                "{args}"
            );
        }
        // Profile 5 has no HDR10 under its metadata, and a base layer
        // without HDR10's static metadata nothing for Quick Sync to tone
        // map by.
        let p5 = with_metadata(ten_bit(media(DynamicRange::DolbyVision, Some(5))));
        let plan = Plan::build(&request(), &p5, &ff, None).unwrap();
        assert!(!plan.quick_sync_would_decode() && !plan.quick_sync_could_tonemap(&p5, true));
        let mut bare = p81.clone();
        bare.streams[0].hdr10_metadata = Some(false);
        assert!(!Plan::build(&request(), &bare, &ff, None).unwrap().quick_sync_could_tonemap(&bare, true));
        // Nor where Vulkan decodes it straight into libplacebo's memory,
        // its metadata with it: nothing there for Quick Sync to relieve.
        let mut vulkan = intel(HwAccel::Qsv, true);
        vulkan.caps.qsv_tonemap = true;
        let plan = Plan::build(&request(), &p81, &vulkan, None).unwrap();
        assert!(matches!(plan.video.as_ref().unwrap().pipeline, Pipeline::Vulkan { .. }));
        assert!(!plan.quick_sync_would_decode() && !plan.quick_sync_could_tonemap(&p81, true));
        // Nor with hardware decoding off, or libplacebo asked for.
        let mut req = request();
        req.options.hardware_decoding = false;
        assert!(!Plan::build(&req, &p81, &ff, None).unwrap().quick_sync_would_decode());
        let mut req = request();
        req.options.tonemap = TonemapMode::Libplacebo;
        assert!(!Plan::build(&req, &p81, &ff, None).unwrap().quick_sync_could_tonemap(&p81, true));
    }

    /// HDR kept on Quick Sync stays on the GPU in 10 bits where its start-up
    /// test passed for the codec; elsewhere it comes to memory.
    #[test]
    fn quick_sync_keeps_hdr_on_the_gpu_where_it_passed_in_10_bits() {
        let mut ff = intel(HwAccel::Qsv, true);
        let m = ten_bit(media(DynamicRange::Hdr10, None));
        let plan = Plan::build(&hdr_request(pley_transcoder_api::OutputRange::Hdr10), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!(v.pipeline, Pipeline::Software, "not tried in 10 bits");
        // Scaled in 10 bits by Quick Sync before it comes to memory.
        assert_eq!((v.hw_decode.as_deref(), v.gpu_scale), (Some("qsv"), Some(HwAccel::Qsv)));
        let args = args_string(&plan, 0, 2);
        assert!(
            args.contains("[0:0]vpp_qsv=w=1920:h=1080:format=p010le,hwdownload,format=p010le,format=p010le[vout]"),
            "{args}"
        );

        ff.caps.gpu_pipelines[0].main10 = vec!["hevc10".into()];
        let plan = Plan::build(&hdr_request(pley_transcoder_api::OutputRange::Hdr10), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.hw_decode.as_deref()), (Pipeline::Gpu(HwAccel::Qsv), Some("qsv")));
        let args = args_string(&plan, 0, 2);
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(args.contains("[0:0]vpp_qsv=w=1920:h=1080:format=p010le[vout]"), "{args}");
        assert!(args.contains("-c:v hevc_qsv") && args.contains("-profile:v main10"), "{args}");
        assert!(args.contains("-color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc"), "{args}");

        // x265 standing in: the frames come down in 10 bits.
        let x265 = x265();
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&x265),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let args = args.join(" ");
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(
            args.contains("[0:0]vpp_qsv=w=1920:h=1080:format=p010le,hwdownload,format=p010le,format=yuv420p10le[vout]"),
            "{args}"
        );
        assert!(args.contains("-c:v libx265") && args.contains("-profile:v main10"), "{args}");
    }

    /// A software encoder standing in for Quick Sync's (its sessions all
    /// taken, or its encoder failing) gets frames in memory: Quick Sync's
    /// decoder keeps them on the GPU, so its scaler brings them down.
    #[test]
    fn a_software_encoder_standing_in_for_quick_sync_gets_frames_in_memory() {
        let ff = intel(HwAccel::Qsv, true);
        let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Sdr, None)), &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Gpu(HwAccel::Qsv));
        let x264 = x264();
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&x264),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let args = args.join(" ");
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(
            args.contains("[0:0]vpp_qsv=w=1920:h=1080:format=nv12,hwdownload,format=nv12,format=yuv420p[vout]"),
            "{args}"
        );
        assert!(args.contains("-c:v libx264"), "{args}");

        // Kept at its size, the same: Quick Sync's frames don't come down by themselves.
        let mut m = media(DynamicRange::Sdr, None);
        (m.streams[0].codec, m.streams[0].width, m.streams[0].height) = ("h264".into(), Some(1920), Some(1080));
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Gpu(HwAccel::Qsv));
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let args = args.join(" ");
        assert!(args.contains(QSV_DECODE_ARGS), "{args}");
        assert!(
            args.contains("[0:0]vpp_qsv=w=1920:h=1080:format=nv12,hwdownload,format=nv12,format=yuv420p[vout]"),
            "{args}"
        );
    }

    /// Video the start-up tests never decoded (4:2:2 H.264, 4:4:4) stays off
    /// the GPU pipelines and Quick Sync's decoder: for example, Intel's
    /// decoders fail on 4:2:2 H.264, where Direct3D 11's leaves it to the CPU.
    #[test]
    fn video_that_isnt_4_2_0_stays_off_the_gpu_pipelines() {
        let ff = intel(HwAccel::Qsv, true);
        let mut m = media(DynamicRange::Sdr, None);
        m.streams[0].codec = "h264".into(); // 4K made 1080p
        m.streams[0].pix_fmt = Some("yuv420p".into());
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::Gpu(HwAccel::Qsv));
        m.streams[0].pix_fmt = Some("yuv422p".into());
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        let decoder = if cfg!(windows) { "d3d11va" } else { "vaapi" };
        assert_eq!((v.pipeline, v.hw_decode.as_deref(), v.gpu_scale), (Pipeline::Software, Some(decoder), None));
    }

    /// VAAPI's encoders where the start-up test found that no GPU tone
    /// mapper's frames reach them (AMD's OpenCL, which can't share VAAPI's
    /// memory, or a Vulkan device that couldn't be made from VAAPI's).
    #[test]
    fn vaapi_encoders_take_tone_mapped_frames_only_from_mappers_that_reach_them() {
        let mut ff = intel(HwAccel::Vaapi, true);
        ff.caps.vaapi_tonemappers = vec![Tonemapper::Zscale];
        ff.caps.vaapi_vulkan_decode.clear();
        let m = ten_bit(media(DynamicRange::Hdr10, None));

        // zimg on the CPU, its frames uploaded to VAAPI's only device.
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.tonemap), (Pipeline::Software, Some(Tonemapper::Zscale)));
        assert_eq!(v.encoder.as_ref().unwrap().name, "h264_vaapi");
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-vaapi_device /dev/dri/renderD128 -hwaccel vaapi"), "{args}");
        assert!(!args.contains("init_hw_device"), "{args}");
        assert!(args.contains("format=yuv420p,format=nv12,hwupload=derive_device=vaapi[vout]"), "{args}");

        // libplacebo's Vulkan device couldn't be made from VAAPI's: Vulkan
        // decoding then goes without it, and the job uses zimg.
        ff.caps.vaapi_tonemappers = vec![Tonemapper::Opencl, Tonemapper::Zscale];
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.tonemap), (Pipeline::Software, Some(Tonemapper::Opencl)));

        // Dolby Vision 5 keeps its colours: libplacebo, and x264 since its
        // frames don't reach VAAPI's encoder.
        ff.caps.vaapi_tonemappers = vec![Tonemapper::Zscale];
        let p5 = ten_bit(media(DynamicRange::DolbyVision, Some(5)));
        let plan = Plan::build(&request(), &p5, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.tonemap, v.encoder.as_ref().unwrap().name.as_str()), (Some(Tonemapper::Libplacebo), "libx264"));
        assert!(plan.warnings.iter().any(|w| w.contains("don't reach the VAAPI encoder")), "{:?}", plan.warnings);
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-init_hw_device vulkan=vk -filter_hw_device vk"), "{args}");
        assert!(!args.contains("vaapi"), "{args}");

        // Only OpenCL, which doesn't reach it: tone mapped all the same, by x264.
        ff.caps.tonemappers = vec![Tonemapper::Opencl];
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.tonemap, v.encoder.as_ref().unwrap().name.as_str()), (Some(Tonemapper::Opencl), "libx264"));

        // HDR kept, converted by libplacebo: not to VAAPI's encoder, so it is
        // tone mapped (by zimg) instead.
        ff.caps.tonemappers = vec![Tonemapper::Libplacebo, Tonemapper::Zscale];
        ff.caps.vaapi_tonemappers = vec![Tonemapper::Zscale];
        let hlg = ten_bit(media(DynamicRange::Hlg, None));
        let plan = Plan::build(&hdr_request(pley_transcoder_api::OutputRange::Hdr10), &hlg, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.hdr, v.tonemap), (None, Some(Tonemapper::Zscale)));
        assert!(plan.warnings.iter().any(|w| w.contains("libplacebo's frames")), "{:?}", plan.warnings);
    }

    /// AMD's Vega graphics on Windows (a Ryzen 5600U): AMF encodes and
    /// libplacebo runs on Vulkan, but the driver has no Vulkan video decoding.
    #[test]
    fn libplacebo_without_vulkan_decoding_has_the_encoders_gpu_decode() {
        let amf = EncoderInfo {
            name: "h264_amf".into(),
            codec: VideoCodec::H264,
            hardware: true,
            accel: HwAccel::Amf,
            main10: false,
            low_power: false,
        };
        let mut ff = ffmpeg_with(vec![amf, x264()], vec![Tonemapper::Libplacebo, Tonemapper::Opencl]);
        ff.caps.hwaccels = ["d3d11va", "vulkan"].map(String::from).to_vec();
        let m = ten_bit(media(DynamicRange::DolbyVision, Some(7)));

        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.tonemap), (Pipeline::Software, Some(Tonemapper::Libplacebo)));
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-init_hw_device vulkan=vk -filter_hw_device vk"), "{args}");
        assert!(!args.contains("-hwaccel vulkan"), "{args}");
        // NV12, which AMF takes as it is.
        assert!(args.contains("libplacebo=w=1920:h=1080:format=nv12"), "{args}");
        assert!(args.contains("-c:v h264_amf"), "{args}");
        let shown = plan.video_plan(&m, None).unwrap();
        if cfg!(windows) {
            // Decoded by the GPU's video engine, through memory to libplacebo.
            assert!(args.contains("-hwaccel d3d11va"), "{args}");
            assert_eq!((shown.decoder.as_deref(), shown.pipeline.as_deref()), (Some("d3d11va"), Some("hybrid")));
        }
    }

    fn amf() -> EncoderInfo {
        EncoderInfo {
            name: "h264_amf".into(),
            codec: VideoCodec::H264,
            hardware: true,
            accel: HwAccel::Amf,
            main10: false,
            low_power: false,
        }
    }

    /// libplacebo's speed measured on a Ryzen 5 5600G's Radeon (Vega 7).
    const VEGA_7: TonemapSpeed = TonemapSpeed { high: 36.6, balanced: 55.9, fast: 64.2 };

    #[test]
    fn tone_mapping_does_what_the_gpu_keeps_ahead_of_real_time() {
        let (uhd, hd) = (3840 * 2160, 1920 * 1080);
        let film = tonemap_need(uhd, uhd, 23.976);
        // An integrated Radeon keeps a 4K film ahead at Balanced, 1080p at High.
        assert_eq!(tonemap_quality(&VEGA_7, film), TonemapQuality::Balanced);
        assert_eq!(tonemap_quality(&VEGA_7, tonemap_need(uhd, hd, 23.976)), TonemapQuality::High);
        // Two 4K films at once: as fast as it goes.
        assert_eq!(tonemap_quality(&VEGA_7, 2.0 * film), TonemapQuality::Fast);
        // A bigger GPU does all of libplacebo's work, for two films too.
        let big = TonemapSpeed { high: 240.0, balanced: 300.0, fast: 320.0 };
        assert_eq!(tonemap_quality(&big, 2.0 * film), TonemapQuality::High);
        // A quality whose options this FFmpeg refused (measured 0) is never taken.
        let old = TonemapSpeed { high: 20.0, balanced: 0.0, fast: 0.0 };
        assert_eq!(tonemap_quality(&old, film), TonemapQuality::High);
    }

    #[test]
    fn the_gpu_falls_behind_where_even_the_fastest_quality_is_too_slow() {
        let (uhd, hd) = (3840 * 2160, 1920 * 1080);
        let (film, made_hd) = (tonemap_need(uhd, uhd, 23.976), tonemap_need(uhd, hd, 23.976));
        // Fast as it measures sampling the source directly.
        let vega = TonemapSpeed { fast: 84.0, ..VEGA_7 };
        assert!(!tonemap_falls_behind(&vega, film));
        // Two 4K films at once; made 1080p, three.
        assert!(tonemap_falls_behind(&vega, 2.0 * film));
        assert!(!tonemap_falls_behind(&vega, 2.0 * made_hd));
        assert!(tonemap_falls_behind(&vega, 3.0 * made_hd));
        // One of each: what they need together, 1080p's less than 4K's.
        assert!(!tonemap_falls_behind(&vega, film + made_hd));
        let big = TonemapSpeed { high: 240.0, balanced: 300.0, fast: 320.0 };
        assert!(!tonemap_falls_behind(&big, 3.0 * film));
        // Nothing measured: not known to.
        assert!(!tonemap_falls_behind(&TonemapSpeed::default(), 2.0 * film));
    }

    #[test]
    fn a_job_tone_maps_at_the_quality_its_gpu_keeps_ahead_of_the_player() {
        let mut ff = ffmpeg_with(vec![amf(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.tonemap_speed = Some(VEGA_7);
        let mut req = request();
        req.video.as_mut().unwrap().max_height = None;
        let mut m = ten_bit(media(DynamicRange::Hdr10, None));
        m.streams[0].frame_rate = Some(23.976);

        let mut plan = Plan::build(&req, &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().tonemap_quality, TonemapQuality::Balanced);
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("tonemapping=auto:peak_detect=0:upscaler=bilinear:contrast_recovery=0,"), "{args}");
        assert_eq!(plan.video_plan(&m, None).unwrap().tonemap_quality, Some(TonemapQuality::Balanced));
        // Another 4K film on the GPU.
        let film = plan.tonemap_need(&m).unwrap();
        plan.share_tonemapping(ff.caps.tonemap_speed.as_ref(), &m, film);
        assert_eq!(plan.video.as_ref().unwrap().tonemap_quality, TonemapQuality::Fast);
        // Its renditions look the same.
        let mut lower = Plan::build(&request(), &m, &ff, None).unwrap();
        assert_eq!(lower.video.as_ref().unwrap().tonemap_quality, TonemapQuality::High);
        lower.tone_map_like(&plan);
        assert_eq!(lower.video.as_ref().unwrap().tonemap_quality, TonemapQuality::Fast);

        // Not measured (no libplacebo on a GPU, an older cache): all of its work.
        ff.caps.tonemap_speed = None;
        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().tonemap_quality, TonemapQuality::High);
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("tonemapping=auto:peak_detect=0,"), "{args}");
    }

    #[test]
    fn fast_tone_mapping_samples_the_source_where_the_picture_shrinks_by_half_at_most() {
        let uhd = (3840, 2160);
        let fast = |out, sdr| placebo_quality_options(TonemapQuality::Fast, uhd, out, sdr);
        // 4K and 1080p: no picture between libplacebo's passes but the
        // output's, in 8 bits when it is SDR.
        let direct = ":skip_aa=1:extra_opts=force_low_bit_depth_fbos=yes";
        assert!(fast(uhd, true).ends_with(&format!(":contrast_recovery=0{direct}")), "{}", fast(uhd, true));
        assert!(fast((1920, 1080), true).ends_with(direct));
        let scope = placebo_quality_options(TonemapQuality::Fast, (3840, 1600), (1920, 800), true);
        assert!(scope.ends_with(direct), "{scope}");
        // HDR kept: in 16 bits.
        assert!(fast((1920, 1080), false).ends_with(":contrast_recovery=0:skip_aa=1"));
        // Smaller than half: anti-aliased, as before.
        assert!(!fast((1280, 720), true).contains("skip_aa"));
        // The other qualities as they were.
        let at = |q| placebo_quality_options(q, uhd, (1920, 1080), true);
        assert_eq!(at(TonemapQuality::Balanced), ":upscaler=bilinear:contrast_recovery=0");
        assert_eq!(at(TonemapQuality::High), "");
    }

    #[test]
    fn a_share_of_the_gpu_sets_the_quality_and_can_move_the_decoding_to_the_cpu() {
        // VAAPI decoding into memory for libplacebo (no Vulkan decoder), then
        // encoding on the same video engine.
        let mut ff = intel(HwAccel::Vaapi, true);
        ff.caps.vulkan_decode.clear();
        ff.caps.vaapi_vulkan_decode.clear();
        let m = ten_bit(media(DynamicRange::Hdr10, None));
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.hw_decode.as_deref()), (Pipeline::Software, Some("vaapi")));
        assert!(plan.decodes_on_shared_engine());
        assert!(args_string(&plan, 0, 2).contains("-hwaccel vaapi "));
        assert_eq!(plan.source_rate(&m).map(|(pixels, _)| pixels), Some(3840 * 2160));
        // 4K made 1080p: an integrated Radeon keeps two ahead, not three.
        let vega = TonemapSpeed { fast: 84.0, ..VEGA_7 };
        let film = plan.tonemap_need(&m).unwrap();
        assert!(!plan.tonemapping_falls_behind(Some(&vega), &m, film));
        assert!(plan.tonemapping_falls_behind(Some(&vega), &m, 2.0 * film));
        assert!(!plan.tonemapping_falls_behind(None, &m, 2.0 * film));

        let shared = plan.with_share(Share {
            quality: TonemapQuality::Fast,
            cpu_decode: true,
            quick_sync: false,
            base_layer: false,
        });
        let args = args_string(&shared, 0, 2);
        assert!(!args.contains("-hwaccel "), "{args}");
        assert!(args.contains("contrast_recovery=0:skip_aa=1:extra_opts=force_low_bit_depth_fbos=yes"), "{args}");
        assert!(args.contains("-init_hw_device vulkan=vk@va -filter_hw_device vk"), "{args}");
        assert!(args.contains("hwupload=derive_device=vaapi[vout]"), "{args}");
        // The GPU keeps decoding where the share leaves it the decoding.
        let kept = plan.with_share(Share {
            quality: TonemapQuality::Balanced,
            cpu_decode: false,
            quick_sync: false,
            base_layer: false,
        });
        assert!(args_string(&kept, 0, 2).contains("-hwaccel vaapi "));

        // NVIDIA's decoder works apart from its encoder.
        let mut ff = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.hwaccels = vec!["cuda".into()];
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().hw_decode.as_deref(), Some("cuda"));
        assert!(!plan.decodes_on_shared_engine());
        // AMF's on Windows decodes with Direct3D 11.
        let mut ff = ffmpeg_with(vec![amf(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.hwaccels = vec!["d3d11va".into()];
        ff.caps.amf_upload = true;
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert_eq!(plan.decodes_on_shared_engine(), cfg!(windows));
        // SDR video isn't tone mapped: it stays on the GPU it is decoded on.
        let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Sdr, None)), &ff, None).unwrap();
        assert!(!plan.decodes_on_shared_engine());
    }

    #[test]
    fn amf_gets_its_frames_uploaded_to_the_gpu() {
        let mut ff = ffmpeg_with(vec![amf(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.hwaccels = vec!["d3d11va".into()];
        ff.caps.amf_upload = true;
        let m = ten_bit(media(DynamicRange::Hdr10, None));
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        let args = args_string(&plan, 0, 2);
        // The AMD GPU is the filters' device; libplacebo makes a Vulkan one of its own.
        assert!(args.contains("-init_hw_device d3d11va=dx:,vendor_id=0x1002 -filter_hw_device dx"), "{args}");
        assert!(!args.contains("vulkan=vk"), "{args}");
        assert!(args.contains("libplacebo=w=1920:h=1080:format=nv12:"), "{args}");
        assert!(args.contains(",format=nv12,hwupload[vout]"), "{args}");
        if cfg!(windows) {
            assert!(args.contains("-hwaccel d3d11va -hwaccel_device dx"), "{args}");
        }

        // A software encoder standing in takes frames from memory, as before.
        let stand_in = x264();
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&stand_in),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let line =
            plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        assert!(!line.contains("d3d11va=dx") && !line.contains("hwupload"), "{line}");
        assert!(line.contains("-init_hw_device vulkan=vk -filter_hw_device vk"), "{line}");
        assert!(line.contains("libplacebo=w=1920:h=1080:format=yuv420p:"), "{line}");

        // OpenCL uploads to its own device: AMF copies from memory then.
        let mut ff = ffmpeg_with(vec![amf(), x264()], vec![Tonemapper::Opencl]);
        ff.caps.amf_upload = true;
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert!(!plan.video.as_ref().unwrap().amf_upload);
    }

    #[test]
    fn h264_from_amf_says_its_colour_primaries() {
        let mut ff = ffmpeg_with(vec![amf(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.h264_metadata = true;
        // Tone mapped: BT.709, which h264_amf leaves unsaid.
        let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
        assert!(args_string(&plan, 0, 2).contains("-bsf:v h264_metadata=colour_primaries=1 "));
        // A software encoder standing in says them itself.
        let stand_in = x264();
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&stand_in),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let line =
            plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        assert!(!line.contains("h264_metadata"), "{line}");
        // SDR keeps the source's (a DVD's BT.601), and says nothing of unknown ones.
        let mut sd = media(DynamicRange::Sdr, None);
        sd.streams[0].color_primaries = Some("bt470bg".into());
        let plan = Plan::build(&request(), &sd, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().vui_primaries, Some(5));
        let plan = Plan::build(&request(), &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().vui_primaries, None);
        // An FFmpeg without the filter: as before.
        ff.caps.h264_metadata = false;
        let plan = Plan::build(&request(), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
        assert!(!args_string(&plan, 0, 2).contains("h264_metadata"));
    }

    #[test]
    fn eight_bit_video_is_scaled_on_direct3d_11_into_amf() {
        let mut ff = ffmpeg_with(vec![amf(), x264()], vec![]);
        ff.caps.hwaccels = vec!["d3d11va".into()];
        ff.caps.gpu_pipelines =
            vec![GpuPipeline { accel: HwAccel::Amf, codecs: vec!["h264".into()], ..Default::default() }];
        let mut m = media(DynamicRange::Sdr, None);
        m.streams[0].codec = "h264".into();
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().pipeline, Pipeline::D3d11 { encode_on_gpu: true });
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("[0:0]scale_d3d11=width=1920:height=1080:format=nv12[vout]"), "{args}");
        // HEVC didn't pass the self-test: through memory.
        let plan = Plan::build(&request(), &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert_eq!(plan.video.unwrap().pipeline, Pipeline::Software);
    }

    #[test]
    fn dolby_vision_profile5_requires_libplacebo() {
        let ff = ffmpeg_with(vec![x264()], vec![Tonemapper::Zscale]);
        let plan = Plan::build(&request(), &media(DynamicRange::DolbyVision, Some(5)), &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().tonemap, Some(Tonemapper::Zscale));
        assert!(plan.warnings.iter().any(|w| w.contains("Dolby Vision")));
        // Its frames are untagged, which zimg refuses: they are tagged PQ
        // BT.2020 so the transcode works (with off colours, as warned).
        let args = args_string(&plan, 0, 2);
        assert!(
            args.contains(
                "setparams=color_trc=smpte2084:color_primaries=bt2020:colorspace=bt2020nc,zscale=w=1920:h=1080:t=linear"
            ),
            "{args}"
        );

        let ff = ffmpeg_with(vec![x264()], vec![Tonemapper::Opencl, Tonemapper::Libplacebo]);
        let plan = Plan::build(&request(), &media(DynamicRange::DolbyVision, Some(5)), &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().tonemap, Some(Tonemapper::Libplacebo));
        assert!(!args_string(&plan, 0, 2).contains("setparams"), "libplacebo reads the RPU");
        let plan = Plan::build(&request(), &media(DynamicRange::DolbyVision, Some(8)), &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().tonemap, Some(Tonemapper::Opencl));
        assert!(args_string(&plan, 0, 2).contains("setparams=color_trc=smpte2084"));
    }

    #[test]
    fn only_unknown_colour_properties_are_filled_in() {
        let mut stream = MediaStream { kind: StreamKind::Video, ..Default::default() };
        assert_eq!(
            untagged_hdr_fix(&stream).as_deref(),
            Some("setparams=color_trc=smpte2084:color_primaries=bt2020:colorspace=bt2020nc")
        );
        stream.color_transfer = Some("arib-std-b67".into());
        stream.color_primaries = Some("bt2020".into());
        stream.color_space = Some("unknown".into());
        assert_eq!(untagged_hdr_fix(&stream).as_deref(), Some("setparams=colorspace=bt2020nc"));
        stream.color_space = Some("bt2020nc".into());
        assert_eq!(untagged_hdr_fix(&stream), None, "HLG keeps its transfer");
        // Tagged HDR10 through zimg: nothing added.
        let ff = ffmpeg_with(vec![x264()], vec![Tonemapper::Zscale]);
        let mut m = media(DynamicRange::Hdr10, None);
        m.streams[0].color_transfer = Some("smpte2084".into());
        m.streams[0].color_primaries = Some("bt2020".into());
        m.streams[0].color_space = Some("bt2020nc".into());
        let plan = Plan::build(&request(), &m, &ff, None).unwrap();
        assert!(!args_string(&plan, 0, 2).contains("setparams"));
    }

    /// A request for HEVC in `range`, at 1080p.
    fn hdr_request(range: pley_transcoder_api::OutputRange) -> JobRequest {
        let mut req = request();
        let video = req.video.as_mut().unwrap();
        (video.codec, video.dynamic_range) = (VideoCodec::Hevc, range);
        req
    }

    /// NVENC for H.264 and HEVC (Main 10), x264 and x265, libplacebo on
    /// Vulkan, 10-bit HEVC decoded straight into CUDA and Vulkan memory.
    fn hdr_ffmpeg() -> Ffmpeg {
        let mut ff = ffmpeg_with(vec![nvenc(), hevc_nvenc(), x264(), x265()], vec![Tonemapper::Libplacebo]);
        ff.caps.vulkan_decode = vec!["hevc10".into()];
        ff.caps.gpu_pipelines = vec![pley_transcoder_api::GpuPipeline {
            accel: HwAccel::Nvenc,
            codecs: vec!["hevc10".into()],
            ..Default::default()
        }];
        ff
    }

    #[test]
    fn hdr10_is_kept_in_hevc_main_10_on_the_gpu() {
        use pley_transcoder_api::OutputRange;
        let ff = hdr_ffmpeg();
        let m = ten_bit(media(DynamicRange::Hdr10, None));
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &m, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!(v.hdr, Some(HdrOut { range: DynamicRange::Hdr10, convert: false }));
        assert_eq!((v.tonemap, v.pipeline), (None, Pipeline::Gpu(HwAccel::Nvenc)));
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
        let args = args_string(&plan, 0, 2);
        // Decoded, scaled to 10 bits and encoded without leaving the GPU;
        // the frames' HDR10 metadata goes with them to the encoder.
        assert!(args.contains("-hwaccel cuda -hwaccel_output_format cuda"), "{args}");
        assert!(args.contains("[0:0]scale_cuda=w=1920:h=1080:format=p010le[vout]"), "{args}");
        assert!(args.contains("-c:v hevc_nvenc"), "{args}");
        assert!(args.contains("-profile:v main10"), "{args}");
        assert!(args.contains("-color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc"), "{args}");
        assert!(!args.contains("libplacebo"), "{args}");
        let vp = plan.video_plan(&m, None).unwrap();
        assert_eq!((vp.codec_out.as_str(), vp.dynamic_range_out), ("hevc", Some(DynamicRange::Hdr10)));
        assert_eq!((vp.encoder.as_deref(), vp.tonemap), (Some("hevc_nvenc"), None));

        // Hardware sessions ran out: x265 in 10 bits, from the CPU.
        let ctx = ArgsContext {
            input: Path::new("/media/movie.mkv"),
            encoder: Some(&ff.caps.encoders[3]),
            relative_split_times: true,
            preset: None,
            shader_cache: None,
        };
        let args: Vec<String> = plan.ffmpeg_args(&ctx, 0, 2).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        let args = args.join(" ");
        // Scaled by the GPU in 10 bits, x265's planar format on the CPU.
        assert!(args.contains("-hwaccel cuda -hwaccel_output_format cuda -extra_hw_frames 8"), "{args}");
        assert!(
            args.contains("scale_cuda=w=1920:h=1080:format=p010le,hwdownload,format=p010le,format=yuv420p10le[vout]"),
            "{args}"
        );
        assert!(args.contains("-c:v libx265"), "{args}");
        assert!(args.contains("repeat-headers=1 -profile:v main10 -dolbyvision 0"), "{args}");
        assert!(args.contains("-color_trc smpte2084"), "{args}");

        // Without a zero-copy pipeline: decoded on the GPU, scaled on the CPU.
        let mut ff = hdr_ffmpeg();
        ff.caps.gpu_pipelines.clear();
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &m, &ff, None).unwrap();
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("-hwaccel cuda ") && !args.contains("hwaccel_output_format"), "{args}");
        assert!(args.contains("scale=1920:1080:flags=bicubic,format=p010le[vout]"), "{args}");

        // Dolby Vision 8.1 and 7 are their HDR10 base layer: kept as it is,
        // the Dolby Vision metadata left out.
        for profile in [8, 7] {
            let m = ten_bit(media(DynamicRange::DolbyVision, Some(profile)));
            let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &m, &hdr_ffmpeg(), None).unwrap();
            let v = plan.video.as_ref().unwrap();
            assert_eq!((v.hdr.map(|h| h.convert), v.tonemap), (Some(false), None), "P{profile}");
        }
    }

    #[test]
    fn other_hdr_formats_are_converted_by_libplacebo() {
        use pley_transcoder_api::OutputRange;
        let ff = hdr_ffmpeg();
        // Dolby Vision profile 5 to HDR10: libplacebo applies the RPU, on
        // Vulkan, then one 10-bit download for NVENC.
        let p5 = ten_bit(media(DynamicRange::DolbyVision, Some(5)));
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &p5, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!(v.hdr, Some(HdrOut { range: DynamicRange::Hdr10, convert: true }));
        assert_eq!((v.tonemap, v.pipeline), (Some(Tonemapper::Libplacebo), Pipeline::Vulkan { encode_on_gpu: false }));
        let args = args_string(&plan, 0, 2);
        assert!(
            args.contains(
                "libplacebo=w=1920:h=1080:format=p010:colorspace=bt2020nc:color_primaries=bt2020:color_trc=smpte2084:\
                 range=tv:tonemapping=auto:peak_detect=0,hwdownload,format=p010[vout]"
            ),
            "{args}"
        );
        assert!(args.contains("-c:v hevc_nvenc") && args.contains("-profile:v main10"), "{args}");

        // HLG to HDR10 for a screen that shows only HDR10; kept as it is for
        // one that shows HLG.
        let hlg = ten_bit(media(DynamicRange::Hlg, None));
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &hlg, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().hdr, Some(HdrOut { range: DynamicRange::Hdr10, convert: true }));
        let plan = Plan::build(&hdr_request(OutputRange::Hlg), &hlg, &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.hdr, v.tonemap), (Some(HdrOut { range: DynamicRange::Hlg, convert: false }), None));
        assert!(args_string(&plan, 0, 2).contains("-color_trc arib-std-b67"));
        // Dolby Vision 8.4 is HLG underneath.
        let mut p84 = ten_bit(media(DynamicRange::DolbyVision, Some(8)));
        p84.streams[0].dv_bl_compat = Some(4);
        let plan = Plan::build(&hdr_request(OutputRange::Hlg), &p84, &ff, None).unwrap();
        assert_eq!(plan.video.as_ref().unwrap().hdr.map(|h| h.convert), Some(false));
        // HDR10 to HLG, for a screen that shows HLG alone, in software
        // (no Vulkan decoding): libplacebo outputs 10 bits.
        let mut ff = hdr_ffmpeg();
        ff.caps.vulkan_decode.clear();
        let plan =
            Plan::build(&hdr_request(OutputRange::Hlg), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None).unwrap();
        let args = args_string(&plan, 0, 2);
        assert!(
            args.contains("libplacebo=w=1920:h=1080:format=p010le:colorspace=bt2020nc:color_primaries=bt2020:color_trc=arib-std-b67"),
            "{args}"
        );
        assert!(args.contains("format=p010le[vout]"), "{args}");
    }

    #[test]
    fn hdr_is_tone_mapped_when_it_cant_be_kept() {
        use pley_transcoder_api::OutputRange;
        let p5 = ten_bit(media(DynamicRange::DolbyVision, Some(5)));
        let sdr = |plan: &Plan| plan.video.as_ref().unwrap().hdr.is_none();
        // H.264 asked for.
        let mut req = hdr_request(OutputRange::Hdr10);
        req.video.as_mut().unwrap().codec = VideoCodec::H264;
        let plan = Plan::build(&req, &p5, &hdr_ffmpeg(), None).unwrap();
        assert!(sdr(&plan) && plan.warnings.iter().any(|w| w.contains("HEVC only")), "{:?}", plan.warnings);
        assert_eq!(plan.video.as_ref().unwrap().encoder.as_ref().unwrap().name, "h264_nvenc");
        // Subtitles drawn onto the video.
        let mut req = hdr_request(OutputRange::Hdr10);
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(3), external_path: None });
        let plan = Plan::build(&req, &p5, &hdr_ffmpeg(), None).unwrap();
        assert!(sdr(&plan) && plan.warnings.iter().any(|w| w.contains("subtitles")), "{:?}", plan.warnings);
        assert_eq!(plan.video.as_ref().unwrap().tonemap, Some(Tonemapper::Libplacebo));
        // A conversion without libplacebo.
        let mut ff = hdr_ffmpeg();
        ff.caps.tonemappers = vec![Tonemapper::Zscale];
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &p5, &ff, None).unwrap();
        assert!(sdr(&plan) && plan.warnings.iter().any(|w| w.contains("libplacebo")), "{:?}", plan.warnings);
        assert_eq!(plan.video.as_ref().unwrap().tonemap, Some(Tonemapper::Zscale));
        // ...though HDR10 itself needs none.
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &ten_bit(media(DynamicRange::Hdr10, None)), &ff, None)
            .unwrap();
        assert!(!sdr(&plan));
        // No 10-bit HEVC encoder: HEVC in SDR.
        let ff =
            ffmpeg_with(vec![nvenc(), EncoderInfo { main10: false, ..hevc_nvenc() }], vec![Tonemapper::Libplacebo]);
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &p5, &ff, None).unwrap();
        assert!(sdr(&plan) && plan.warnings.iter().any(|w| w.contains("10-bit")), "{:?}", plan.warnings);
        let args = args_string(&plan, 0, 2);
        assert!(args.contains("color_trc=bt709") && args.contains("-profile:v main "), "{args}");
        assert_eq!(plan.video_plan(&p5, None).unwrap().dynamic_range_out, Some(DynamicRange::Sdr));
        // SDR video, and SDR asked for: nothing to say.
        let plan = Plan::build(&hdr_request(OutputRange::Hdr10), &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert!(sdr(&plan) && plan.warnings.is_empty());
        let plan = Plan::build(&hdr_request(OutputRange::Sdr), &p5, &hdr_ffmpeg(), None).unwrap();
        assert!(sdr(&plan) && plan.warnings.is_empty(), "{:?}", plan.warnings);
    }

    /// A phone's portrait video: 1920×1080 frames turned by the display
    /// matrix. FFmpeg turns frames in memory only (`-autorotate`), so they
    /// stay there, and the plan is made at the size they are shown.
    #[test]
    fn turned_video_is_planned_as_it_is_shown() {
        let mut ff = ffmpeg_with(vec![nvenc(), x264()], vec![Tonemapper::Libplacebo]);
        ff.caps.gpu_pipelines = vec![pley_transcoder_api::GpuPipeline {
            accel: HwAccel::Nvenc,
            codecs: vec!["hevc".into(), "hevc10".into()],
            ..Default::default()
        }];
        ff.caps.vulkan_decode = vec!["hevc10".into()];
        let phone = |range: DynamicRange, rotation: i64| {
            let mut m = media(range, None);
            (m.streams[0].width, m.streams[0].height, m.streams[0].rotation) = (Some(1920), Some(1080), Some(rotation));
            m
        };

        for rotation in [90, -90, 270] {
            let plan = Plan::build(&request(), &phone(DynamicRange::Sdr, rotation), &ff, None).unwrap();
            let v = plan.video.as_ref().unwrap();
            assert_eq!((v.src_width, v.src_height, v.width, v.height), (1080, 1920, 608, 1080), "{rotation}");
            // Not the zero-copy pipeline, nor scaled by the GPU: hardware
            // decoded into memory, turned, then scaled to the shown shape.
            assert_eq!((v.pipeline, v.gpu_scale, v.hw_decode.as_deref()), (Pipeline::Software, None, Some("cuda")));
            let args = args_string(&plan, 0, 2);
            assert!(args.contains("-hwaccel cuda -readrate"), "{args}");
            assert!(!args.contains("hwaccel_output_format") && !args.contains("noautorotate"), "{args}");
            assert!(args.contains("[0:0]scale=608:1080:flags=bicubic,format=yuv420p[vout]"), "{args}");
            let shown = plan.video_plan(&phone(DynamicRange::Sdr, rotation), None).unwrap();
            assert_eq!((shown.width_in, shown.height_in), (Some(1080), Some(1920)));
        }

        // Upside down: the same size, turned in memory too.
        let plan = Plan::build(&request(), &phone(DynamicRange::Sdr, 180), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.width, v.height, v.pipeline), (1920, 1080, Pipeline::Software));

        // HDR: libplacebo takes the frames from memory, not from Vulkan's decoder.
        let plan = Plan::build(&request(), &ten_bit(phone(DynamicRange::Hdr10, -90)), &ff, None).unwrap();
        let v = plan.video.as_ref().unwrap();
        assert_eq!((v.pipeline, v.tonemap), (Pipeline::Software, Some(Tonemapper::Libplacebo)));
        let args = args_string(&plan, 0, 2);
        assert!(!args.contains("-hwaccel vulkan") && args.contains("libplacebo=w=608:h=1080"), "{args}");

        // Subtitles drawn for the picture as it is shown.
        let mut req = request();
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(3), external_path: None });
        let plan = Plan::build(&req, &phone(DynamicRange::Sdr, 90), &ff, None).unwrap();
        assert!(args_string(&plan, 0, 2).contains(":original_size=1080x1920,"));

        // Copied into MPEG-TS it would play on its side: transcoded instead.
        let mut req = request();
        req.video = Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() });
        let kf: Vec<i64> = (0..30).map(|i| i * 2_000).collect();
        let plan = Plan::build(&req, &phone(DynamicRange::Sdr, 90), &ff, Some(&kf)).unwrap();
        assert!(!plan.video.as_ref().unwrap().copy);
        assert!(plan.warnings.iter().any(|w| w.contains("turned")), "{:?}", plan.warnings);
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, Some(&kf)).unwrap();
        assert!(plan.video.as_ref().unwrap().copy);
    }

    /// Copied sound beside transcoded video starts at the seek point, as the
    /// video does, not with the packets before it the seek landed on.
    #[test]
    fn copied_audio_after_a_restart_leaves_out_what_came_before_the_seek() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.audio = Some(AudioRequest { mode: AudioMode::Copy, ..Default::default() });
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert!(plan.audio.as_ref().unwrap().copy && !plan.video.as_ref().unwrap().copy);
        let args = args_string(&plan, 3, 8);
        assert!(args.contains("-ss 12.000") && args.contains("-c:a copy -copypriorss:a 0 -map_metadata"), "{args}");
        // Nothing to leave out from the start.
        assert!(!args_string(&plan, 0, 8).contains("copypriorss"));
        // A remux keeps the sound from its keyframe, with its video.
        req.video = Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() });
        let kf: Vec<i64> = (0..30).map(|i| i * 2_000).collect();
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, Some(&kf)).unwrap();
        let args = args_string(&plan, 3, 8);
        assert!(args.contains("-noaccurate_seek") && !args.contains("copypriorss"), "{args}");
        // Converted sound has no packets to leave out.
        let mut req = request();
        req.audio = Some(AudioRequest::default());
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert!(!args_string(&plan, 3, 8).contains("copypriorss"));
    }

    #[test]
    fn copy_needs_keyframes() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.video = Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() });
        req.audio = Some(AudioRequest { mode: AudioMode::Copy, ..Default::default() });
        let m = media(DynamicRange::Sdr, None);

        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        assert!(!plan.video.as_ref().unwrap().copy);
        assert!(plan.warnings.iter().any(|w| w.contains("keyframe")));

        let kf: Vec<i64> = (0..30).map(|i| i * 2_000).collect();
        let plan = Plan::build(&req, &m, &ff, Some(&kf)).unwrap();
        assert!(plan.video.as_ref().unwrap().copy);
        assert!(plan.audio.as_ref().unwrap().copy, "eac3 can be copied into MPEG-TS");
        let args = args_string(&plan, 2, 4);
        // Matroska with B-frames (or none known): past FFmpeg's back-off,
        // with the split for 12 s counted from the 8 s keyframe.
        assert!(args.contains("-noaccurate_seek -ss 8.140"), "{args}");
        assert!(args.contains("-output_ts_offset 8.140"), "{args}");
        assert!(args.contains("-segment_times 3.500 "), "{args}");
        assert!(args.contains("-t 7.860"), "ends at the 16 s boundary: {args}");
        assert!(args.contains("-c:v copy"));
        assert!(!args.contains("-force_key_frames"));
        assert!(!args_string(&plan, 0, 2).contains("-ss"), "nothing to seek at the start");

        // MP4 seeks by presentation time: no back-off to get past.
        let mut mp4 = media(DynamicRange::Sdr, None);
        mp4.format_name = Some("mov,mp4,m4a,3gp,3g2,mj2".into());
        let plan = Plan::build(&req, &mp4, &ff, Some(&kf)).unwrap();
        assert!(args_string(&plan, 2, 4).contains("-noaccurate_seek -ss 8.001"));
        // Neither does Matroska without B-frames.
        let mut no_b = media(DynamicRange::Sdr, None);
        no_b.streams[0].has_b_frames = Some(false);
        let plan = Plan::build(&req, &no_b, &ff, Some(&kf)).unwrap();
        assert!(args_string(&plan, 2, 4).contains("-ss 8.001"));

        // MPEG-TS style start time: keyframe timestamps are absolute, the layout is not.
        let mut ts = media(DynamicRange::Sdr, None);
        ts.start_ms = Some(600_000);
        let kf: Vec<i64> = (0..30).map(|i| 600_000 + i * 2_000).collect();
        let plan = Plan::build(&req, &ts, &ff, Some(&kf)).unwrap();
        assert!(plan.video.as_ref().unwrap().copy);
        assert_eq!(&plan.boundaries[..3], &[0, 4_000, 8_000]);
        assert_eq!(plan.segment_count(), 15);
    }

    #[test]
    fn remux_seeks_stay_between_the_boundary_and_the_next_keyframe() {
        let kf = [0, 1_000, 1_100, 1_200, 4_000, 4_200, 9_000];
        let boundaries = [0, 1_000, 4_000, 9_000, 10_000];
        // Past the 130 ms back-off where the next keyframe allows it,
        // halfway to it where it is closer.
        assert_eq!(remux_seek_points(&boundaries, &kf, true), vec![0, 1_050, 4_100, 9_140, 10_000]);
        assert_eq!(remux_seek_points(&boundaries, &kf, false), vec![0, 1_001, 4_001, 9_001, 10_000]);

        let mut m = media(DynamicRange::Sdr, None);
        assert!(seeks_back(&m), "Matroska, B-frames unknown");
        m.streams[0].has_b_frames = Some(true);
        m.format_name = Some("mpegts".into());
        assert!(seeks_back(&m));
        m.format_name = Some("mov,mp4,m4a,3gp,3g2,mj2".into());
        assert!(!seeks_back(&m), "MP4 seeks by presentation time");
        m.format_name = None;
        m.container = Some("mp4".into());
        assert!(!seeks_back(&m));
        m.container = Some("mkv".into());
        m.streams[0].has_b_frames = Some(false);
        assert!(!seeks_back(&m), "no B-frames, no back-off");
    }

    /// FFmpeg, when it is on the PATH (it isn't in CI).
    fn real_ffmpeg() -> bool {
        let found = std::process::Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success());
        if !found {
            eprintln!("ffmpeg not found: skipped");
        }
        found
    }

    /// First packet time of each stream kind (`v`, `a`) in an MPEG-TS file.
    fn first_packet(file: &Path, stream: &str) -> f64 {
        let out = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", stream, "-show_entries", "packet=pts_time", "-of", "csv=p=0"])
            .arg(file)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.trim().trim_end_matches(',').parse::<f64>().ok())
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn remux_restarts_start_their_segments_on_their_boundaries() {
        if !real_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("b-frames.mkv");
        // H.264 with B-frames and a keyframe every second, AC3 audio.
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=s=320x240:r=24:d=12", "-f", "lavfi", "-i", "sine=r=48000:d=12"])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-bf", "2", "-g", "24", "-sc_threshold", "0"])
            .args(["-c:a", "ac3"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        let probed = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
            .arg(&input)
            .output()
            .unwrap();
        let m = crate::probe::parse_ffprobe_json(&probed.stdout, &input).unwrap();
        assert_eq!(m.video().unwrap().has_b_frames, Some(true));

        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.video = Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() });
        req.audio = Some(AudioRequest { mode: AudioMode::Transcode, ..Default::default() });
        req.segment_seconds = Some(2.0);
        req.options.throttle = false;
        let kf: Vec<i64> = (0..12).map(|s| s * 1_000).collect();
        let plan = Plan::build(&req, &m, &ff, Some(&kf)).unwrap();
        assert_eq!(&plan.boundaries[..4], &[0, 2_000, 4_000, 6_000]);

        // The whole file from the start, then from the boundary at 4 s.
        let ctx =
            ArgsContext { input: &input, encoder: None, relative_split_times: true, preset: None, shader_cache: None };
        let run = |name: &str, start: u32| {
            let out = dir.path().join(name);
            std::fs::create_dir(&out).unwrap();
            let args = plan.ffmpeg_args(&ctx, start, plan.segment_count());
            let ok = std::process::Command::new("ffmpeg").args(&args).current_dir(&out).output().unwrap();
            assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
            out
        };
        let whole = run("whole", 0);
        let resumed = run("resumed", 2);
        for n in 2..plan.segment_count() {
            let (a, b) = (whole.join(format!("{n}.ts")), resumed.join(format!("{n}.ts")));
            let (expected, got) = (first_packet(&a, "v"), first_packet(&b, "v"));
            assert!((expected - got).abs() < 0.001, "segment {n} starts at {got}, not {expected}");
        }
        // The converted audio starts with the video, not at the seek point.
        let first = resumed.join("2.ts");
        let (video, audio) = (first_packet(&first, "v"), first_packet(&first, "a"));
        assert!((audio - video).abs() < 0.1, "audio at {audio}, video at {video}");
    }

    #[test]
    fn image_subtitles_are_overlaid() {
        let ff = ffmpeg_with(vec![x264()], vec![Tonemapper::Zscale]);
        let mut req = request();
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(2), external_path: None });
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        let args = args_string(&plan, 0, 2);
        // The 4K frame goes to 1080p first, then the subtitles' canvas,
        // fitted to its width, is laid on it.
        assert!(args.contains("scale=1920:1080:flags=bicubic[base];"), "{args}");
        assert!(args.contains("[0:2]scale=1920:-1[subs];[base][subs]overlay=x=(W-w)/2:y=H-h"), "{args}");
        assert!(args.contains("[ov]format=yuv420p[vout]"), "{args}");
        assert!(!args.contains("-ss"));
    }

    #[test]
    fn text_subtitles_use_libass_with_offset() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(3), external_path: None });
        let plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert!(matches!(plan.burn, Some(Burn::Text { .. })));
        let args = args_string(&plan, 5, 6);
        assert!(args.contains("setpts=PTS+20.000/TB,subtitles=filename=burn.ass:original_size="), "{args}");
        assert!(args.contains("-segment_time 999999"));
    }

    #[test]
    fn text_subtitles_are_drawn_with_the_attached_fonts() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(3), external_path: None });
        let mut plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        plan.use_fonts(Some(PathBuf::from("/cache/subtitles/k.fonts")));
        let args = args_string(&plan, 0, 2);
        assert!(
            args.contains("subtitles=filename=burn.ass:fontsdir=/cache/subtitles/k.fonts:original_size="),
            "{args}"
        );
        // Picture subtitles have no fonts.
        req.subtitle = Some(SubtitleBurnRequest { stream_index: Some(2), external_path: None });
        let mut plan = Plan::build(&req, &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        plan.use_fonts(Some(PathBuf::from("/cache/subtitles/k.fonts")));
        assert!(!args_string(&plan, 0, 2).contains("fontsdir"));
    }

    #[test]
    fn paths_in_filters_are_escaped_twice() {
        assert_eq!(filter_path(Path::new("/var/cache/pley/k.fonts")), "/var/cache/pley/k.fonts");
        // `'` and `\` are escaped for the option, then each backslash and the
        // `'` again for the graph; `:` once, `,` and `[` for the graph only.
        assert_eq!(filter_path(Path::new("/a'b/c:d/e,f[g]")), r"/a\\\'b/c\\:d/e\,f\[g\]");
        if cfg!(windows) {
            assert_eq!(filter_path(Path::new(r"C:\pley\cache\k.fonts")), r"C\\:/pley/cache/k.fonts");
        }
    }

    /// FFmpeg finds the fonts in a folder whose name needs escaping.
    #[test]
    fn ffmpeg_reads_fonts_from_an_escaped_folder() {
        if !real_ffmpeg() {
            return;
        }
        let filters = std::process::Command::new("ffmpeg").args(["-hide_banner", "-filters"]).output().unwrap();
        if !String::from_utf8_lossy(&filters.stdout).contains(" subtitles ") {
            eprintln!("ffmpeg without libass: skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "it's [a], fonts; dir" } else { "it's [a], fonts; dir: x" };
        let fonts = dir.path().join(name);
        std::fs::create_dir(&fonts).unwrap();
        std::fs::write(fonts.join("0.ttf"), b"not really a font").unwrap();
        let ass = "[Script Info]\nScriptType: v4.00+\n\n[Events]\nFormat: Layer, Start, End, Style, Text\n";
        std::fs::write(dir.path().join("burn.ass"), ass).unwrap();
        let graph = format!("[0:v]subtitles=filename=burn.ass:fontsdir={}[vout]", filter_path(&fonts));
        let ran = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "verbose", "-f", "lavfi", "-i", "color=s=64x64:d=0.1"])
            .args(["-filter_complex", &graph, "-map", "[vout]", "-frames:v", "1", "-f", "null", "-"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&ran.stderr);
        assert!(ran.status.success(), "{log}");
        assert!(log.contains("Loading font file") && log.contains("0.ttf"), "{log}");
    }

    #[test]
    fn audio_only_job() {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let req =
            JobRequest { input: "/music/a.flac".into(), audio: Some(AudioRequest::default()), ..Default::default() };
        let m = MediaInfo {
            duration_ms: Some(200_000),
            streams: vec![MediaStream {
                index: 0,
                kind: StreamKind::Audio,
                codec: "flac".into(),
                channels: Some(2),
                sample_rate: Some(96_000),
                ..Default::default()
            }],
            ..Default::default()
        };
        let plan = Plan::build(&req, &m, &ff, None).unwrap();
        let args = args_string(&plan, 0, 3);
        assert!(args.contains("-vn"));
        assert!(args.contains("-c:a aac -ac 2 -b:a 192k -ar 48000"));
    }

    /// `media` with its audio replaced by one track of `codec`.
    fn with_audio(codec: &str, channels: i64, sample_rate: i64) -> MediaInfo {
        let mut m = media(DynamicRange::Sdr, None);
        let a = &mut m.streams[1];
        (a.codec, a.channels, a.sample_rate) = (codec.into(), Some(channels), Some(sample_rate));
        m
    }

    fn audio_args(codec: AudioCodec, channels: Option<u32>, source: &MediaInfo) -> (Plan, String) {
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.audio = Some(AudioRequest { codec, channels, ..Default::default() });
        let plan = Plan::build(&req, source, &ff, None).unwrap();
        let args = args_string(&plan, 0, 2);
        (plan, args)
    }

    #[test]
    fn audio_is_converted_to_e_ac_3_or_ac_3_in_5_1() {
        let truehd = with_audio("truehd", 8, 48_000);
        let (plan, args) = audio_args(AudioCodec::Eac3, Some(6), &truehd);
        assert!(args.contains("-map 0:1 -c:a eac3 -ac 6 -b:a 640k"), "{args}");
        assert!(!args.contains("-ar 48000"), "already 48 kHz");
        let out = plan.audio_plan(&truehd).unwrap();
        assert_eq!((out.codec_out.as_str(), out.channels_out, out.bitrate_kbps), ("eac3", Some(6), Some(640)));

        // DTS at 96 kHz: AC-3 takes 48 kHz at most.
        let dts = with_audio("dts", 6, 96_000);
        let (_, args) = audio_args(AudioCodec::Ac3, Some(6), &dts);
        assert!(args.contains("-c:a ac3 -ac 6 -b:a 640k -ar 48000"), "{args}");

        // 7.1 asked for: the AC-3 family stops at 5.1, AAC doesn't.
        let (_, args) = audio_args(AudioCodec::Eac3, Some(8), &truehd);
        assert!(args.contains("-c:a eac3 -ac 6 -b:a 640k"), "{args}");
        let (_, args) = audio_args(AudioCodec::Aac, Some(8), &truehd);
        assert!(args.contains("-c:a aac -ac 8 -b:a 512k"), "{args}");

        // Never more channels than the source; stereo is 192 kbit/s.
        let stereo = with_audio("flac", 2, 44_100);
        let (_, args) = audio_args(AudioCodec::Eac3, Some(6), &stereo);
        assert!(args.contains("-c:a eac3 -ac 2 -b:a 192k"), "{args}");

        // Copies stay copies, and the default is still stereo AAC.
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.audio = Some(AudioRequest { mode: AudioMode::Copy, codec: AudioCodec::Eac3, ..Default::default() });
        let plan = Plan::build(&req, &with_audio("ac3", 6, 48_000), &ff, None).unwrap();
        assert!(args_string(&plan, 0, 2).contains("-c:a copy"));
        let plan = Plan::build(&req, &truehd, &ff, None).unwrap();
        assert_eq!(plan.warnings, ["truehd audio transcoded to E-AC-3"]);
        let (_, args) = audio_args(AudioCodec::default(), None, &truehd);
        assert!(args.contains("-c:a aac -ac 2 -b:a 192k"), "{args}");
    }

    #[test]
    fn surround_made_stereo_keeps_dialogue_up_front_without_clipping() {
        let track = |codec: &str, channels: i64, layout: &str| {
            let mut m = with_audio(codec, channels, 48_000);
            m.streams[1].channel_layout = Some(layout.into());
            m
        };
        // A DTS-HD MA 7.1 is decoded from its 5.1 core, made 7.1 again first.
        let mut ma = track("dts", 8, "7.1");
        ma.streams[1].profile = Some("DTS-HD MA".into());
        for (m, layout, filter) in [
            (track("eac3", 6, "5.1(side)"), "5.1(side)", DOWNMIX_51),
            (track("aac", 6, "5.1"), "5.1", DOWNMIX_51),
            (track("ac3", 5, "5.0(side)"), "5.0(side)", DOWNMIX_51),
            (track("aac", 8, "7.1"), "7.1", DOWNMIX_71),
            (ma, "7.1 DTS-HD MA", DOWNMIX_71),
        ] {
            let (plan, args) = audio_args(AudioCodec::Aac, Some(2), &m);
            assert_eq!(plan.audio.as_ref().unwrap().downmix, Some(filter), "{layout}");
            assert!(args.contains(&format!("-map 0:1 -af {filter} -c:a aac -ac 2 -b:a 192k")), "{args}");
        }
        assert!(DOWNMIX_51.contains("pan=stereo|c0=c0+c2+0.707*c4|c1=c1+c2+0.707*c5,alimiter=limit=0.794:level=false"));
        // E-AC-3 in stereo too.
        let (_, args) = audio_args(AudioCodec::Eac3, Some(2), &track("eac3", 6, "5.1(side)"));
        assert!(args.contains(&format!("-af {DOWNMIX_51} -c:a eac3 -ac 2")), "{args}");

        // Left to -ac 2: surround kept, stereo or other layouts, TrueHD
        // decoded to stereo by its decoder, an unknown layout, copies.
        let none = |m: &MediaInfo, codec: AudioCodec, channels: u32| {
            let (plan, args) = audio_args(codec, Some(channels), m);
            assert_eq!(plan.audio.as_ref().unwrap().downmix, None, "{args}");
            assert!(!args.contains(" -af "), "{args}");
        };
        none(&track("eac3", 6, "5.1(side)"), AudioCodec::Eac3, 6);
        none(&track("aac", 2, "stereo"), AudioCodec::Aac, 2);
        none(&track("aac", 4, "quad"), AudioCodec::Aac, 2);
        none(&track("aac", 8, "7.1(wide)"), AudioCodec::Aac, 2);
        none(&track("truehd", 8, "7.1"), AudioCodec::Aac, 2);
        none(&with_audio("eac3", 6, 48_000), AudioCodec::Aac, 2);
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.audio = Some(AudioRequest { mode: AudioMode::Copy, ..Default::default() });
        let plan = Plan::build(&req, &track("ac3", 6, "5.1(side)"), &ff, None).unwrap();
        assert_eq!(plan.audio.as_ref().unwrap().downmix, None);
    }

    #[test]
    fn lossless_audio_is_decoded_only_as_far_as_the_conversion_keeps() {
        let input = |args: &str| args.split(" -i ").next().unwrap().to_owned();
        // TrueHD 7.1: its 2.0 substream for stereo, its 5.1 one for 5.1.
        let truehd = with_audio("truehd", 8, 48_000);
        let (_, args) = audio_args(AudioCodec::Aac, Some(2), &truehd);
        assert!(input(&args).contains("-downmix:1 stereo"), "{args}");
        let (_, args) = audio_args(AudioCodec::Eac3, Some(6), &truehd);
        assert!(input(&args).contains("-downmix:1 5.1"), "{args}");
        // 7.1 kept (AAC): all of it.
        let (_, args) = audio_args(AudioCodec::Aac, Some(8), &truehd);
        assert!(!args.contains("-downmix"), "{args}");
        // DTS-HD Master Audio: its core, for any conversion of 5.1 or fewer.
        let mut ma = with_audio("dts", 8, 48_000);
        ma.streams[1].profile = Some("DTS-HD MA + DTS:X".into());
        let (_, args) = audio_args(AudioCodec::Eac3, Some(6), &ma);
        assert!(input(&args).contains("-core_only:1 1"), "{args}");
        // A core-only DTS track, and a copy, are left as they are.
        let (_, args) = audio_args(AudioCodec::Ac3, Some(6), &with_audio("dts", 6, 48_000));
        assert!(!args.contains("-core_only"), "{args}");
        let ff = ffmpeg_with(vec![x264()], vec![]);
        let mut req = request();
        req.audio = Some(AudioRequest { mode: AudioMode::Copy, codec: AudioCodec::Eac3, ..Default::default() });
        let plan = Plan::build(&req, &with_audio("eac3", 8, 48_000), &ff, None).unwrap();
        assert!(!args_string(&plan, 0, 2).contains("-downmix"));
    }

    /// One stream's fields as ffprobe reports them: `codec_name=eac3`, ….
    fn probe_stream(file: &Path, stream: &str, entries: &str) -> std::collections::HashMap<String, String> {
        let out = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", stream, "-show_entries"])
            .arg(format!("stream={entries}"))
            .args(["-of", "default=noprint_wrappers=1"])
            .arg(file)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
            .collect()
    }

    #[test]
    fn surround_sources_become_e_ac_3_5_1_segments() {
        if !real_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // Local samples made here: H.264 with a keyframe every second, and
        // 5.1 audio as AC-3 or DTS.
        for (name, codec) in [("ac3.mkv", "ac3"), ("dts.mkv", "dca")] {
            let input = dir.path().join(name);
            let made = std::process::Command::new("ffmpeg")
                .args(["-hide_banner", "-loglevel", "error", "-y"])
                .args(["-f", "lavfi", "-i", "testsrc2=s=320x240:r=24:d=6"])
                .args(["-f", "lavfi", "-i", "sine=f=440:r=48000:d=6"])
                .args(["-filter_complex", "[1:a]pan=5.1(side)|FL=c0|FR=c0|FC=c0|LFE=c0|SL=c0|SR=c0[a]"])
                .args(["-map", "0:v", "-map", "[a]"])
                .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "24", "-sc_threshold", "0"])
                .args(["-c:a", codec, "-strict", "-2"])
                .arg(&input)
                .status()
                .unwrap();
            assert!(made.success(), "{name}");
            let probed = std::process::Command::new("ffprobe")
                .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
                .arg(&input)
                .output()
                .unwrap();
            let m = crate::probe::parse_ffprobe_json(&probed.stdout, &input).unwrap();
            let source = m.default_audio().unwrap();
            assert_eq!((source.codec.as_str(), source.channels), (if codec == "dca" { "dts" } else { codec }, Some(6)));

            // A remux for a TV with a 5.1 output: the video copied, the audio
            // converted to E-AC-3 5.1.
            let ff = ffmpeg_with(vec![x264()], vec![]);
            let mut req = request();
            req.video = Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() });
            req.audio = Some(AudioRequest { codec: AudioCodec::Eac3, channels: Some(6), ..Default::default() });
            req.segment_seconds = Some(2.0);
            req.options.throttle = false;
            let kf: Vec<i64> = (0..6).map(|s| s * 1_000).collect();
            let plan = Plan::build(&req, &m, &ff, Some(&kf)).unwrap();
            let out = dir.path().join(name.replace(".mkv", ""));
            std::fs::create_dir(&out).unwrap();
            let ctx = ArgsContext {
                input: &input,
                encoder: None,
                relative_split_times: true,
                preset: None,
                shader_cache: None,
            };
            let args = plan.ffmpeg_args(&ctx, 0, plan.segment_count());
            let ran = std::process::Command::new("ffmpeg").args(&args).current_dir(&out).output().unwrap();
            assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));

            for n in 0..plan.segment_count() {
                let segment = out.join(format!("{n}.ts"));
                let audio = probe_stream(&segment, "a", "codec_name,channels,channel_layout,sample_rate,bit_rate");
                assert_eq!(audio.get("codec_name").map(String::as_str), Some("eac3"), "{name} {n}: {audio:?}");
                assert_eq!(audio.get("channels").map(String::as_str), Some("6"), "{name} {n}: {audio:?}");
                assert!(audio.get("channel_layout").is_some_and(|l| l.starts_with("5.1")), "{audio:?}");
                assert_eq!(audio.get("sample_rate").map(String::as_str), Some("48000"), "{name} {n}");
                assert_eq!(audio.get("bit_rate").map(String::as_str), Some("640000"), "{name} {n}");
                let video = probe_stream(&segment, "v", "codec_name");
                assert_eq!(video.get("codec_name").map(String::as_str), Some("h264"), "copied");
            }
        }
    }

    #[test]
    fn fmp4_processes_write_one_fragmented_mp4_on_stdout() {
        let ff = ffmpeg_with(vec![x264(), x265()], vec![Tonemapper::Zscale]);
        let mut req = request();
        req.segment_format = SegmentFormat::Fmp4;
        let sdr = media(DynamicRange::Sdr, None);
        let plan = Plan::build(&req, &sdr, &ff, None).unwrap();
        assert_eq!(plan.segment_format, SegmentFormat::Fmp4);
        let args = args_string(&plan, 3, 8);
        assert!(
            args.contains(
                "-f mp4 -movflags +frag_keyframe+empty_moov+default_base_moof+delay_moov+frag_discont+skip_trailer \
                 -write_tmcd 0 -flush_packets 1 pipe:1"
            ),
            "{args}"
        );
        assert!(args.ends_with(" pipe:1"), "{args}");
        assert!(!args.contains("-segment"), "cut by the transcoder: {args}");
        assert!(!args.contains("hvc1"), "H.264: {args}");
        // Seeking, its absolute times and the window are those of MPEG-TS.
        assert!(args.contains("-ss 12.000") && args.contains("-output_ts_offset 12.000"), "{args}");
        assert!(args.contains("-avoid_negative_ts disabled -t 20.000"), "{args}");
        assert!(args.contains("-force_key_frames expr:gte(t,n_forced*4)"), "{args}");
        // Encoded frames at a constant rate, as the segment muxer has them.
        assert!(args.contains("-fps_mode:v cfr -f mp4"), "{args}");

        // HEVC, encoded or copied, is tagged as Apple's players take it.
        let mut hevc = req.clone();
        hevc.video.as_mut().unwrap().codec = VideoCodec::Hevc;
        let plan = Plan::build(&hevc, &sdr, &ff, None).unwrap();
        assert!(args_string(&plan, 0, 8).contains("-tag:v hvc1 -fps_mode:v cfr -f mp4"));
        let mut copy = req.clone();
        copy.video = Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() });
        let kf: Vec<i64> = (0..30).map(|i| i * 2_000).collect();
        let plan = Plan::build(&copy, &media(DynamicRange::Hdr10, None), &ff, Some(&kf)).unwrap();
        assert!(plan.video.as_ref().unwrap().copy && plan.segment_format == SegmentFormat::Fmp4);
        assert!(args_string(&plan, 2, 8).contains("-c:v copy"));
        assert!(args_string(&plan, 2, 8).contains("-tag:v hvc1 -f mp4"), "a copy keeps its timing");
        assert!(!args_string(&plan, 2, 8).contains("-fps_mode"));

        // Asked for MPEG-TS, nothing changes.
        let plan = Plan::build(&request(), &sdr, &ff, None).unwrap();
        assert_eq!(plan.segment_format, SegmentFormat::Ts);
        let args = args_string(&plan, 3, 8);
        assert!(args.contains("-f segment -segment_format mpegts") && !args.contains("movflags"), "{args}");
    }

    #[test]
    fn fmp4_is_made_for_video_dolby_vision_remuxes_included() {
        let ff = ffmpeg_with(vec![x264()], vec![Tonemapper::Zscale]);
        let kf: Vec<i64> = (0..30).map(|i| i * 2_000).collect();
        let fmp4 = |video: Option<VideoRequest>| JobRequest { video, segment_format: SegmentFormat::Fmp4, ..request() };
        // Audio only: MPEG-TS, which Apple's players take its sound in.
        let plan = Plan::build(&fmp4(None), &media(DynamicRange::Sdr, None), &ff, None).unwrap();
        assert_eq!(plan.segment_format, SegmentFormat::Ts);
        assert_eq!(plan.init_dolby_vision(&media(DynamicRange::Sdr, None)), None);

        // A remux of Dolby Vision, whatever is done with it: fMP4, its init
        // given the record the stream has once edited.
        let dv = |profile: i64| {
            let mut m = media(DynamicRange::DolbyVision, Some(profile));
            m.streams[0].dv_level = Some(6);
            m
        };
        let config = |profile, el, compat| DoviConfig { profile, level: 6, rpu: true, el, bl: true, compat };
        for (profile, mode, record, tag) in [
            (8, DolbyVisionMode::Copy, None, "hvc1"),
            (8, DolbyVisionMode::Signal, Some(config(8, false, 1)), "hvc1"),
            (7, DolbyVisionMode::To81, Some(config(8, false, 1)), "hvc1"),
            (7, DolbyVisionMode::Signal, Some(config(7, true, 1)), "hvc1"),
            (5, DolbyVisionMode::Signal, Some(config(5, false, 0)), "dvh1"),
            // Profile 5 can't be made 8.1: named as it is.
            (5, DolbyVisionMode::To81, Some(config(5, false, 0)), "dvh1"),
            (5, DolbyVisionMode::Copy, None, "hvc1"),
        ] {
            let copy = VideoRequest { mode: VideoMode::Copy, dolby_vision: mode, ..Default::default() };
            let plan = Plan::build(&fmp4(Some(copy)), &dv(profile), &ff, Some(&kf)).unwrap();
            assert_eq!(plan.segment_format, SegmentFormat::Fmp4, "{profile} {mode:?}");
            assert_eq!(plan.init_dolby_vision(&dv(profile)), Some(record), "{profile} {mode:?}");
            let args = args_string(&plan, 0, 8);
            assert!(args.contains(&format!("-tag:v {tag} -f mp4")), "{profile} {mode:?}: {args}");
            assert_eq!(plan.segment_edits().is_some(), mode == DolbyVisionMode::To81 && profile == 7);
        }
        let strip = VideoRequest { mode: VideoMode::Copy, strip_hdr10plus: true, ..Default::default() };
        let hdr10plus = media(DynamicRange::Hdr10plus, None);
        let plan = Plan::build(&fmp4(Some(strip)), &hdr10plus, &ff, Some(&kf)).unwrap();
        assert!(plan.segment_edits().is_some());
        assert_eq!(plan.segment_format, SegmentFormat::Fmp4);
        assert_eq!(plan.init_dolby_vision(&hdr10plus), None, "no Dolby Vision: FFmpeg's init as it is");
        // Dolby Vision transcoded is video like any other.
        let plan = Plan::build(&fmp4(request().video), &dv(8), &ff, None).unwrap();
        assert!(!plan.video.as_ref().unwrap().copy);
        assert_eq!(plan.segment_format, SegmentFormat::Fmp4);
        assert_eq!(plan.init_dolby_vision(&dv(8)), None);
        assert!(plan.warnings.iter().all(|w| !w.contains("MPEG-TS")), "{:?}", plan.warnings);
    }
}
