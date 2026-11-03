// SPDX-License-Identifier: AGPL-3.0-only
//! Remuxes with Dolby Vision through FFmpeg, as jobs make them: segments
//! written by the planned command line, edited (`tsedit` for MPEG-TS; for
//! fMP4 cut and edited as jobs cut them, the init given its Dolby Vision
//! record), and read back with ffprobe; the master playlist next to them.
//! Skipped without FFmpeg (and libx265) on the PATH.
//!
//! The sources are made here: HEVC Main 10 HDR10 with HDR10+ (libx265),
//! E-AC-3 5.1, and Dolby Vision profile 7 put in by these tests (an RPU
//! with a full enhancement layer's residual data, and enhancement-layer NAL
//! units), since FFmpeg can't encode profile 7. Files of one's own can be
//! tried too: `PLEY_TEST_DV_SAMPLES` names a folder of Dolby Vision files
//! (profiles 5, 7, 8), whose RPUs must read and write back bit for bit, and
//! make it through a remux.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use pley_transcoder_api::{
    AudioMode, AudioRequest, Capabilities, DolbyVisionMode, DynamicRange, EncoderInfo, HwAccel, JobRequest, MediaInfo,
    OutputRange, SegmentFormat, StreamKind, TranscodeOptions, VideoCodec, VideoMode, VideoRequest,
};

use crate::dovi::{self, NAL_DV_EL, NAL_DV_RPU};
use crate::ffmpeg::Ffmpeg;
use crate::fmp4::{DoviConfig, Fragment, Init};
use crate::hls;
use crate::job;
use crate::plan::{ArgsContext, Plan};
use crate::tsedit::{self, nal_units};

fn has_ffmpeg() -> bool {
    let ok = |tool: &str| Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success());
    let found = ok("ffmpeg") && ok("ffprobe");
    if !found {
        eprintln!("ffmpeg not found: skipped");
    }
    found
}

fn ffmpeg(dir: &Path, args: &[&str]) -> bool {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    if !out.status.success() {
        eprintln!("ffmpeg {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
    out.status.success()
}

fn ff() -> Ffmpeg {
    Ffmpeg {
        ffmpeg: "ffmpeg".into(),
        ffprobe: "ffprobe".into(),
        caps: Capabilities { ffmpeg_available: true, ffprobe_available: true, ..Default::default() },
        segment_times_relative: true,
    }
}

fn probe(file: &Path) -> MediaInfo {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(file)
        .output()
        .unwrap();
    crate::probe::parse_ffprobe_json(&out.stdout, file).unwrap()
}

/// ffprobe's `key=value` lines for the side data of the video frames in
/// the first `packets`.
fn frames(file: &Path, packets: u32) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v", "-read_intervals"])
        .arg(format!("%+#{packets}"))
        .args(["-show_entries", "frame_side_data", "-of", "default"])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A few fields of the first stream of `kind` (`v`, `a`).
fn stream(file: &Path, kind: &str) -> HashMap<String, String> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", kind, "-show_entries"])
        .arg("stream=codec_name,profile,pix_fmt,width,height,channels,color_transfer,color_primaries")
        .args(["-of", "default=noprint_wrappers=1"])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
        .collect()
}

/// HDR10+ metadata for libx265 (`dhdr10-info`): the same for each frame.
fn hdr10plus_json(frames: usize) -> String {
    let scene = |i: usize| {
        format!(
            r#"{{"BezierCurveData":{{"Anchors":[102,205,307,410,512,614,717,819,922],"KneePointX":0,"KneePointY":0}},
            "LuminanceParameters":{{"AverageRGB":1000,"LuminanceDistributions":{{"DistributionIndex":[1,5,10,25,50,75,90,95,99],
            "DistributionValues":[10,50,100,500,1000,5000,10000,20000,40000]}},"MaxScl":[40000,40000,40000]}},
            "NumberOfWindows":1,"TargetedSystemDisplayMaximumLuminance":400,"SceneFrameIndex":{i},"SceneId":0,"SequenceFrameIndex":{i}}}"#
        )
    };
    let scenes: Vec<String> = (0..frames).map(scene).collect();
    format!(
        r#"{{"JSONInfo":{{"HDR10plusProfile":"B","Version":"1.0"}},"SceneInfo":[{}],
        "SceneInfoSummary":{{"SceneFirstFrameIndex":[0],"SceneFrameNumbers":[{frames}]}}}}"#,
        scenes.join(",")
    )
}

/// 6 s of 640×360 HEVC Main 10 HDR10 at 24 fps, a keyframe a second, with
/// HDR10+ when libx265 takes it (`hdr10plus.mkv`, else `hdr10.mkv`), and
/// E-AC-3 5.1. `None` when libx265 is missing.
fn hdr10_source(dir: &Path) -> Option<(PathBuf, bool)> {
    std::fs::write(dir.join("hdr10plus.json"), hdr10plus_json(144)).unwrap();
    let x265 = "log-level=error:keyint=24:min-keyint=24:scenecut=0:hdr10=1:repeat-headers=1:colorprim=bt2020:\
                transfer=smpte2084:colormatrix=bt2020nc:master-display=G(13250,34500)B(7500,3000)R(34000,16000)\
                WP(15635,16450)L(10000000,1):max-cll=1000,400";
    for (name, params) in
        [("hdr10plus.mkv", format!("{x265}:dhdr10-info=hdr10plus.json")), ("hdr10.mkv", x265.to_owned())]
    {
        let made = ffmpeg(
            dir,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=640x360:r=24:d=6",
                "-f",
                "lavfi",
                "-i",
                "sine=f=440:r=48000:d=6",
                "-filter_complex",
                "[1:a]pan=5.1(side)|FL=c0|FR=c0|FC=c0|LFE=c0|SL=c0|SR=c0[a]",
                "-map",
                "0:v",
                "-map",
                "[a]",
                "-c:v",
                "libx265",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p10le",
                "-x265-params",
                &params,
                "-c:a",
                "eac3",
                "-b:a",
                "384k",
                name,
            ],
        );
        if made {
            return Some((dir.join(name), name == "hdr10plus.mkv"));
        }
    }
    eprintln!("libx265 can't make HDR10 HEVC: skipped");
    None
}

/// HDR10+ shows only in the frames (an HEVC SEI message, not the
/// container): the prober reads the first ones and calls the video HDR10+
/// (see `hybrids_are_found_at_probe_time`), and plain HDR10 stays HDR10.
#[tokio::test]
async fn plain_hdr10_stays_hdr10() {
    if !has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let x265 = "log-level=error:hdr10=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc";
    let made = ffmpeg(
        dir.path(),
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=320x180:r=24:d=1",
            "-c:v",
            "libx265",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p10le",
            "-x265-params",
            x265,
            "hdr10.mkv",
        ],
    );
    if !made {
        eprintln!("libx265 can't make HDR10 HEVC: skipped");
        return;
    }
    let prober = crate::probe::Prober::new(Arc::new(ff()), Arc::default(), 1);
    let info = prober.probe(&dir.path().join("hdr10.mkv")).await.unwrap();
    let video = info.video().unwrap();
    assert_eq!((video.dynamic_range, video.hdr10plus), (Some(DynamicRange::Hdr10), Some(false)));
    assert_eq!(video.dynamic_range_label(), "HDR10");
}

/// `source` as MPEG-TS with Dolby Vision profile 7 put into every access
/// unit: enhancement-layer NAL units and an RPU with a full layer's
/// residual data.
fn with_profile_7(dir: &Path, source: &Path) -> PathBuf {
    assert!(ffmpeg(dir, &["-i", source.to_str().unwrap(), "-map", "0", "-c", "copy", "-f", "mpegts", "plain.ts"]));
    let plain = std::fs::read(dir.join("plain.ts")).unwrap();
    let rpu = dovi::tests::p7_rpu(false);
    let p7 = tsedit::rewrite_video(&plain, |es| {
        let mut au = es.to_vec();
        au.extend([0, 0, 1, 0x7E, 0x01, 0x10, 0x20, 0x30]);
        au.extend([0, 0, 1]);
        au.extend(&rpu);
        Some(au)
    })
    .unwrap()
    .unwrap();
    let path = dir.join("p7.ts");
    std::fs::write(&path, p7).unwrap();
    path
}

const FORMATS: [SegmentFormat; 2] = [SegmentFormat::Ts, SegmentFormat::Fmp4];

/// A copy job's segments, made by FFmpeg as the plan says and edited as
/// jobs edit them, in `dir/out`: for fMP4, each with the init before it
/// (`{n}.mp4`), which ffprobe reads alone.
fn remux(
    dir: &Path,
    input: &Path,
    media: &MediaInfo,
    mode: DolbyVisionMode,
    strip: bool,
    format: SegmentFormat,
) -> (Plan, Vec<PathBuf>) {
    let req = JobRequest {
        segment_format: format,
        input: input.to_string_lossy().into_owned(),
        video: Some(VideoRequest {
            mode: VideoMode::Copy,
            dolby_vision: mode,
            strip_hdr10plus: strip,
            ..Default::default()
        }),
        audio: Some(AudioRequest { mode: AudioMode::Copy, ..Default::default() }),
        segment_seconds: Some(2.0),
        options: TranscodeOptions { throttle: false, ..Default::default() },
        ..Default::default()
    };
    let start = media.start_ms.unwrap_or(0);
    let keyframes: Vec<i64> = (0..6).map(|s| start + s * 1_000).collect();
    let plan = Plan::build(&req, media, &ff(), Some(&keyframes)).unwrap();
    assert!(plan.video.as_ref().unwrap().copy);
    assert_eq!(plan.segment_format, format);
    let out = dir.join("out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir(&out).unwrap();
    let ctx = ArgsContext { input, encoder: None, relative_split_times: true, preset: None, shader_cache: None };
    let args = plan.ffmpeg_args(&ctx, 0, plan.segment_count());
    let ran = Command::new("ffmpeg").arg("-hide_banner").args(&args).current_dir(&out).output().unwrap();
    assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
    if format == SegmentFormat::Fmp4 {
        let segments = cut_fmp4(&plan, media, &ran.stdout, &out);
        return (plan, segments);
    }
    let segments: Vec<PathBuf> = (0..plan.segment_count()).map(|n| out.join(format!("{n}.ts"))).collect();
    if let Some(edits) = plan.segment_edits() {
        for s in &segments {
            let edited = tsedit::edit_segment(s, edits).unwrap();
            assert_eq!(edited.rpus_dropped, 0, "{:?}", edited.first_error);
        }
    }
    (plan, segments)
}

/// The top-level boxes of an MP4 file, whole.
fn top_boxes(data: &[u8]) -> Vec<Vec<u8>> {
    let mut boxes = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let size = match u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) {
            1 => u64::from_be_bytes(data[at + 8..at + 16].try_into().unwrap()) as usize,
            n => n as usize,
        };
        boxes.push(data[at..at + size].to_vec());
        at += size;
    }
    boxes
}

/// An fMP4 process's output cut as jobs cut it (see `job::read_fragments`):
/// `init.mp4`, and each segment with the init before it as `{n}.mp4`.
fn cut_fmp4(plan: &Plan, media: &MediaInfo, output: &[u8], out: &Path) -> Vec<PathBuf> {
    let boxes = top_boxes(output);
    assert_eq!((&boxes[0][4..8], &boxes[1][4..8]), (&b"ftyp"[..], &b"moov"[..]));
    let init = job::fmp4_init(plan, media, &boxes[0], &boxes[1]).unwrap();
    std::fs::write(out.join("ffmpeg-init.mp4"), [&boxes[0][..], &boxes[1]].concat()).unwrap();
    std::fs::write(out.join("init.mp4"), &init.bytes).unwrap();
    let mut segments = vec![Vec::new(); plan.segment_count() as usize];
    let mut current = 0;
    for pair in boxes[2..].chunks(2) {
        let mut fragment = Fragment::parse(pair[0].clone(), pair[1].clone(), &init).unwrap();
        let prepared = job::prepare_fragment(plan, &init, &mut fragment);
        assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);
        if let Some(edited) = &prepared.edited {
            assert_eq!(edited.rpus_dropped, 0, "{:?}", edited.first_error);
        }
        if let Some(t) = fragment.presentation_start_ms(&init) {
            current = job::segment_of(plan, t) as usize;
        }
        segments[current].extend_from_slice(prepared.bytes.as_deref().unwrap_or(&fragment.bytes));
    }
    segments
        .iter()
        .enumerate()
        .map(|(n, segment)| {
            assert!(!segment.is_empty(), "segment {n} empty");
            let path = out.join(format!("{n}.mp4"));
            std::fs::write(&path, [&init.bytes[..], segment].concat()).unwrap();
            path
        })
        .collect()
}

/// The init of an MP4 file (`None`: MPEG-TS).
fn mp4_init(file: &Path) -> Option<Init> {
    let data = std::fs::read(file).unwrap();
    if data.get(4..8) != Some(b"ftyp") {
        return None;
    }
    let boxes = top_boxes(&data);
    Some(Init::parse(&boxes[0], &boxes[1]).unwrap())
}

/// The NAL unit types of every video access unit of a segment: MPEG-TS,
/// or fMP4 with its init.
fn nal_types(segment: &Path) -> Vec<Vec<u8>> {
    let data = std::fs::read(segment).unwrap();
    if let Some(init) = mp4_init(segment) {
        let video = init.track(init.video.unwrap()).unwrap();
        let length_size = usize::from(video.nal_length_size.unwrap());
        let boxes = top_boxes(&data);
        let mut units = Vec::new();
        for pair in boxes[2..].chunks(2) {
            let fragment = Fragment::parse(pair[0].clone(), pair[1].clone(), &init).unwrap();
            for sample in fragment.samples(video.id) {
                let mut kinds = Vec::new();
                let mut at = 0;
                while at < sample.len() {
                    let length = sample[at..at + length_size].iter().fold(0, |n, &b| (n << 8) | usize::from(b));
                    kinds.push(dovi::nal_type(&sample[at + length_size..]).unwrap());
                    at += length_size + length;
                }
                assert_eq!(at, sample.len(), "{segment:?}: NAL units that fill their sample");
                units.push(kinds);
            }
        }
        return units;
    }
    let mut units = Vec::new();
    tsedit::rewrite_video(&data, |es| {
        units.push(nal_units(es).iter().filter_map(|u| dovi::nal_type(&es[u.nal.clone()])).collect());
        None
    })
    .unwrap();
    units
}

#[test]
fn profile_7_is_remuxed_as_8_1_without_hdr10_plus() {
    if !has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let Some((source, with_hdr10plus)) = hdr10_source(dir.path()) else { return };
    let input = with_profile_7(dir.path(), &source);

    // FFmpeg reads the profile 7 RPU put in: residual data, MMR chroma.
    let before = frames(&input, 3);
    assert!(before.contains("disable_residual_flag=0") && before.contains("el_spatial_resampling_filter_flag=1"));
    assert!(before.contains("nlq_method_idc_name=linear_dz") && before.contains("mapping_idc_name=mmr"), "{before}");
    assert_eq!(before.contains("2094-40"), with_hdr10plus);

    let mut media = probe(&input);
    let v = media.streams.iter_mut().find(|s| s.kind == StreamKind::Video).unwrap();
    (v.dynamic_range, v.dv_profile, v.dv_level, v.dv_bl_compat) =
        (Some(DynamicRange::DolbyVision), Some(7), Some(6), Some(6));
    v.hdr10plus = Some(with_hdr10plus);
    for format in FORMATS {
        let (plan, segments) = remux(dir.path(), &input, &media, DolbyVisionMode::To81, true, format);
        assert_eq!(segments.len(), 3);
        for segment in &segments {
            assert_made_8_1(segment);
        }
        let video_plan = plan.video_plan(&media, None).unwrap();
        assert_eq!((video_plan.dolby_vision, video_plan.hdr10plus_stripped), (Some(DolbyVisionMode::To81), true));

        // The master playlist says Dolby Vision 8.1 over HDR10, at the level
        // its 640×360 at 24 fps needs (1), though the stream says 6.
        let master = hls::master_playlist(&plan, &media);
        let level = media.video().unwrap().level.unwrap();
        assert!(
            master.contains(&format!(
                "CODECS=\"hvc1.2.4.L{level}.B0,ec-3\",SUPPLEMENTAL-CODECS=\"dvh1.08.01/db1p\",RESOLUTION=640x360,\
                 FRAME-RATE=24.000,VIDEO-RANGE=PQ\nindex.m3u8\n"
            )),
            "{format:?}: {master}"
        );
    }
}

/// A segment made from the profile 7 test file is profile 8.1 without
/// HDR10+, with its picture, audio and static HDR metadata as they were;
/// in fMP4 (with its init before it), its init says 8.1 at the level the
/// stream needs, as the master playlist does.
fn assert_made_8_1(segment: &Path) {
    if let Some(init) = mp4_init(segment) {
        let video = init.track(init.video.unwrap()).unwrap();
        assert_eq!(&video.format, b"hvc1", "{segment:?}");
        let p81 = DoviConfig { profile: 8, level: 1, rpu: true, el: false, bl: true, compat: 1 };
        assert_eq!(video.dovi, Some(p81), "{segment:?}");
        assert!(
            init.bytes[..init.bytes.windows(4).position(|w| w == b"moov").unwrap()].windows(4).any(|w| w == b"dby1")
        );
    }
    {
        let video = stream(segment, "v");
        assert_eq!(
            (video["codec_name"].as_str(), video["profile"].as_str(), video["pix_fmt"].as_str()),
            ("hevc", "Main 10", "yuv420p10le"),
            "{segment:?}"
        );
        assert_eq!((video["color_transfer"].as_str(), video["color_primaries"].as_str()), ("smpte2084", "bt2020"));
        let audio = stream(segment, "a");
        assert_eq!((audio["codec_name"].as_str(), audio["channels"].as_str()), ("eac3", "6"), "copied");
        // Profile 8.1 to FFmpeg: no residual, no-op curves (one linear
        // piece over the whole range), the P8.1 colour coefficients.
        let after = frames(segment, 4);
        assert!(after.contains("Dolby Vision Metadata"), "{after}");
        assert!(!after.contains("disable_residual_flag=0") && after.contains("disable_residual_flag=1"));
        assert!(after.contains("el_spatial_resampling_filter_flag=0") && after.contains("nlq_method_idc_name=none"));
        assert!(after.contains("pivots=0 1023") && after.contains("poly_coef=0 8388608"), "{after}");
        assert!(!after.contains("mmr"));
        assert!(after.contains("rgb_to_lms_matrix=7222/16384"), "{after}");
        assert!(after.contains("source_max_pq=3696"), "display management kept");
        // HDR10+ gone, the mastering display and light levels still there.
        assert!(!after.contains("2094-40"), "{after}");
        assert!(after.contains("Mastering display metadata") && after.contains("Content light level metadata"));
        // No enhancement layer; the RPU last, as decoders want it.
        for au in nal_types(segment) {
            assert!(!au.contains(&NAL_DV_EL), "{au:?}");
            assert_eq!(au.last(), Some(&NAL_DV_RPU), "{au:?}");
        }
    }
}

/// The same through a job: segments are edited before they are served,
/// and the job serves the master playlist.
#[tokio::test]
async fn jobs_edit_segments_before_serving_them() {
    if !has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let Some((source, true)) = hdr10_source(dir.path()) else { return };
    let input = with_profile_7(dir.path(), &source);
    // Matroska: its Cues are the keyframe index, at once.
    assert!(ffmpeg(dir.path(), &["-i", input.to_str().unwrap(), "-map", "0", "-c", "copy", "p7.mkv"]));
    let input = dir.path().join("p7.mkv");
    let mut media = probe(&input);
    let v = media.streams.iter_mut().find(|s| s.kind == StreamKind::Video).unwrap();
    (v.dynamic_range, v.dv_profile, v.dv_level, v.dv_bl_compat, v.hdr10plus) =
        (Some(DynamicRange::DolbyVision), Some(7), Some(6), Some(6), Some(true));

    let ff = Arc::new(ff());
    let metrics = Arc::new(crate::metrics::Metrics::default());
    let prober = Arc::new(crate::probe::Prober::new(ff.clone(), metrics.clone(), 1));
    let joined = dir.path().join("joined.mp4");
    let background = crate::background::Background::new();
    let keyframes = crate::keyframes::KeyframeStore::new(
        ff.clone(),
        metrics.clone(),
        background.clone(),
        dir.path().join("keyframes"),
    );
    let extractor = Arc::new(crate::extract::Extractor::new(
        ff.clone(),
        prober.clone(),
        metrics.clone(),
        dir.path().join("subs"),
        1,
    ));
    let limits = pley_transcoder_api::Limits { max_jobs: 2, max_hw_jobs: 0 };
    let jobs = crate::job::JobManager::new(
        ff,
        prober,
        keyframes,
        extractor,
        metrics,
        dir.path().join("jobs"),
        None,
        None,
        limits,
        background,
    );
    for format in FORMATS {
        let created = jobs
            .create(JobRequest {
                input: input.to_string_lossy().into_owned(),
                media: Some(media.clone()),
                video: Some(VideoRequest {
                    mode: VideoMode::Copy,
                    dolby_vision: DolbyVisionMode::To81,
                    strip_hdr10plus: true,
                    ..Default::default()
                }),
                audio: Some(AudioRequest { mode: AudioMode::Copy, ..Default::default() }),
                segment_seconds: Some(2.0),
                options: TranscodeOptions { throttle: false, ..Default::default() },
                segment_format: format,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(created.segment_format, format);
        let video = created.video.as_ref().unwrap();
        assert_eq!(
            (video.mode, video.dolby_vision, video.hdr10plus_stripped),
            (VideoMode::Copy, Some(DolbyVisionMode::To81), true)
        );
        let job = jobs.get(&created.id).unwrap();
        for n in 0..created.segment_count {
            let segment = jobs.segment(&job, n).await.unwrap();
            if format == SegmentFormat::Fmp4 {
                let (_, init) = jobs.rendition_init(&job, 0).await.unwrap();
                let bytes = [std::fs::read(init).unwrap(), std::fs::read(&segment).unwrap()].concat();
                std::fs::write(&joined, bytes).unwrap();
                assert_made_8_1(&joined);
            } else {
                assert_made_8_1(&segment);
            }
        }
        // At the level its 640×360 needs, as above.
        let master = job.master_playlist();
        assert!(master.contains("SUPPLEMENTAL-CODECS=\"dvh1.08.01/db1p\""), "{format:?}: {master}");
        assert!(job.status().warnings.is_empty(), "{format:?}: {:?}", job.status().warnings);
        jobs.remove(&created.id).await;
    }
    jobs.shutdown().await;
}

#[test]
fn hdr10_remuxes_are_left_as_they_are() {
    if !has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let Some((source, _)) = hdr10_source(dir.path()) else { return };
    let media = probe(&source);
    for format in FORMATS {
        let (plan, segments) = remux(dir.path(), &source, &media, DolbyVisionMode::Signal, false, format);
        assert_eq!(plan.segment_edits(), None, "nothing for the transcoder to do");
        assert_eq!(plan.init_dolby_vision(&media), None, "FFmpeg's init as it is");
        assert_eq!(plan.video_plan(&media, None).unwrap().dolby_vision, None, "no Dolby Vision to name");
        for segment in &segments {
            let video = stream(segment, "v");
            assert_eq!((video["codec_name"].as_str(), video["profile"].as_str()), ("hevc", "Main 10"));
            assert!(!frames(segment, 2).contains("Dolby Vision"));
            if let Some(init) = mp4_init(segment) {
                let video = init.track(init.video.unwrap()).unwrap();
                assert_eq!((&video.format, video.dovi), (b"hvc1", None));
            }
        }
        let master = hls::master_playlist(&plan, &media);
        assert!(master.contains(",CODECS=\"hvc1.2.4.L"), "{master}");
        assert!(!master.contains("SUPPLEMENTAL") && master.contains("VIDEO-RANGE=PQ"), "{master}");
    }
}

/// Transcodes that keep HDR, with x265 (NVENC is planned the same way and
/// writes the same metadata; docs/TRANSCODER.md): HEVC Main 10 with PQ and
/// BT.2020 said in the stream, the mastering display and light levels at the
/// start of every segment, and Dolby Vision (profile 7 here) left out: its
/// HDR10 base layer is what comes out.
#[test]
fn hdr_transcodes_keep_hdr10_and_leave_dolby_vision_out() {
    if !has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let Some((source, _)) = hdr10_source(dir.path()) else { return };
    let p7 = with_profile_7(dir.path(), &source);
    let mut ff = ff();
    ff.caps.encoders = vec![EncoderInfo {
        name: "libx265".into(),
        codec: VideoCodec::Hevc,
        hardware: false,
        accel: HwAccel::None,
        main10: true,
    }];
    for (input, dolby_vision) in [(&source, false), (&p7, true)] {
        let mut media = probe(input);
        let v = media.streams.iter_mut().find(|s| s.kind == StreamKind::Video).unwrap();
        assert_eq!(v.dynamic_range, Some(DynamicRange::Hdr10));
        if dolby_vision {
            assert!(frames(input, 1).contains("Dolby Vision Metadata"));
            (v.dynamic_range, v.dv_profile, v.dv_level, v.dv_bl_compat) =
                (Some(DynamicRange::DolbyVision), Some(7), Some(6), Some(6));
        }
        let req = JobRequest {
            input: input.to_string_lossy().into_owned(),
            video: Some(VideoRequest {
                codec: VideoCodec::Hevc,
                dynamic_range: OutputRange::Hdr10,
                max_height: Some(360),
                ..Default::default()
            }),
            audio: None,
            segment_seconds: Some(2.0),
            options: TranscodeOptions { throttle: false, hardware_decoding: false, ..Default::default() },
            ..Default::default()
        };
        let plan = Plan::build(&req, &media, &ff, None).unwrap();
        assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
        let out = dir.path().join(if dolby_vision { "out-p7" } else { "out-hdr10" });
        std::fs::create_dir(&out).unwrap();
        let ctx = ArgsContext { input, encoder: None, relative_split_times: true, preset: None, shader_cache: None };
        let args = plan.ffmpeg_args(&ctx, 0, plan.segment_count());
        let ran = Command::new("ffmpeg").arg("-hide_banner").args(&args).current_dir(&out).output().unwrap();
        assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
        assert_eq!(plan.segment_count(), 3);
        for n in 0..plan.segment_count() {
            let segment = out.join(format!("{n}.ts"));
            let video = stream(&segment, "v");
            assert_eq!(
                (video["codec_name"].as_str(), video["profile"].as_str(), video["pix_fmt"].as_str()),
                ("hevc", "Main 10", "yuv420p10le"),
                "{segment:?}"
            );
            assert_eq!((video["color_transfer"].as_str(), video["color_primaries"].as_str()), ("smpte2084", "bt2020"));
            let first = frames(&segment, 1);
            assert!(first.contains("Mastering display metadata") && first.contains("max_luminance=10000000/10000"));
            assert!(first.contains("Content light level metadata") && first.contains("max_content=1000"), "{first}");
            assert!(!frames(&segment, 4).contains("Dolby Vision"), "{segment:?}");
        }
        let master = hls::master_playlist(&plan, &media);
        assert!(master.contains("CODECS=\"hvc1.2.4.L93.B0\"") && master.contains("VIDEO-RANGE=PQ"), "{master}");
    }
}

/// Dolby Vision 8.1 + HDR10+ made from the profile 7 test file: its RPUs
/// made 8.1 here, then given a configuration record by FFmpeg's `dovi_rpu`
/// filter (FFmpeg 7.1 and later) in Matroska. `None` without the filter.
fn hybrid_81(dir: &Path) -> Option<PathBuf> {
    let (source, true) = hdr10_source(dir)? else {
        eprintln!("libx265 can't write HDR10+: skipped");
        return None;
    };
    let p7 = std::fs::read(with_profile_7(dir, &source)).unwrap();
    let edits = tsedit::Edits { to_81: true, strip_hdr10plus: false };
    let (p81, _) = tsedit::edit_ts(&p7, edits).unwrap();
    std::fs::write(dir.join("p81.ts"), p81.unwrap()).unwrap();
    if !ffmpeg(dir, &["-i", "p81.ts", "-map", "0", "-c", "copy", "-bsf:v", "dovi_rpu", "hybrid.mkv"]) {
        eprintln!("FFmpeg without the dovi_rpu filter: skipped");
        return None;
    }
    Some(dir.join("hybrid.mkv"))
}

#[tokio::test]
async fn hybrids_are_found_at_probe_time() {
    if !has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let Some(hybrid) = hybrid_81(dir.path()) else { return };
    let metrics = Arc::new(crate::metrics::Metrics::default());
    let prober = crate::probe::Prober::new(Arc::new(ff()), metrics, 1);
    let info = prober.probe(&hybrid).await.unwrap();
    let video = info.video().unwrap();
    assert_eq!(
        (video.dynamic_range, video.dv_profile, video.dv_bl_compat),
        (Some(DynamicRange::DolbyVision), Some(8), Some(1))
    );
    assert_eq!(video.hdr10plus, Some(true), "only its frames say");

    // Remuxed for a Dolby Vision decoder: HDR10+ out, Dolby Vision named
    // (in fMP4's init too).
    let p81 = DoviConfig { profile: 8, level: 1, rpu: true, el: false, bl: true, compat: 1 };
    for format in FORMATS {
        let (plan, segments) = remux(dir.path(), &hybrid, &info, DolbyVisionMode::Signal, true, format);
        for segment in &segments {
            let after = frames(segment, 4);
            assert!(after.contains("Dolby Vision Metadata") && !after.contains("2094-40"), "{after}");
            assert!(after.contains("disable_residual_flag=1"));
            if let Some(init) = mp4_init(segment) {
                assert_eq!(init.track(init.video.unwrap()).unwrap().dovi, Some(p81));
            }
        }
        assert!(hls::master_playlist(&plan, &info).contains("SUPPLEMENTAL-CODECS=\"dvh1.08.01/db1p\""));
    }
    // Copied for a player that doesn't decode Dolby Vision: its RPUs go
    // along, but the init names none, as the master playlist doesn't.
    let (plan, segments) = remux(dir.path(), &hybrid, &info, DolbyVisionMode::Copy, false, SegmentFormat::Fmp4);
    assert_eq!(plan.init_dolby_vision(&info), Some(None));
    // FFmpeg names it with the brand, from Matroska's record (and with a
    // record of its own only with `-strict unofficial`).
    let ffmpeg_init = std::fs::read(dir.path().join("out/ffmpeg-init.mp4")).unwrap();
    assert!(ffmpeg_init[..ffmpeg_init.windows(4).position(|w| w == b"moov").unwrap()].windows(4).any(|w| w == b"dby1"));
    for segment in &segments {
        let init = mp4_init(segment).unwrap();
        assert_eq!(init.track(init.video.unwrap()).unwrap().dovi, None);
        assert!(!init.bytes.windows(4).any(|w| w == b"dby1"));
    }
    assert!(!hls::master_playlist(&plan, &info).contains("SUPPLEMENTAL"));

    // The file without Dolby Vision: HDR10+, found the same way.
    let plain = prober.probe(&dir.path().join("hdr10plus.mkv")).await.unwrap();
    let video = plain.video().unwrap();
    assert_eq!((video.dynamic_range, video.hdr10plus), (Some(DynamicRange::Hdr10plus), Some(true)));
}

/// Files of one's own (see the module's notes).
#[test]
fn dolby_vision_samples_rewrite_and_remux() {
    let Some(folder) = std::env::var_os("PLEY_TEST_DV_SAMPLES") else {
        eprintln!("PLEY_TEST_DV_SAMPLES not set: skipped");
        return;
    };
    if !has_ffmpeg() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mut tried = 0;
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        let mut media = probe(&path);
        let Some(video) = media.streams.iter_mut().find(|s| s.kind == StreamKind::Video) else { continue };
        if video.dynamic_range != Some(DynamicRange::DolbyVision) || video.codec != "hevc" {
            continue;
        }
        let profile = video.dv_profile.unwrap_or(0);
        // A few seconds of it, in its own container.
        let name = format!("clip.{}", path.extension().and_then(|e| e.to_str()).unwrap_or("mkv"));
        let clip = dir.path().join(&name);
        let input = path.to_str().unwrap();
        assert!(ffmpeg(dir.path(), &["-t", "4", "-i", input, "-map", "0:v:0", "-map", "0:a?", "-c", "copy", &name]));
        let clip_media = probe(&clip);
        media = clip_media;
        let keyframes = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v:0", "-skip_frame", "nokey", "-show_entries", "frame=pts_time"])
            .args(["-of", "csv=p=0"])
            .arg(&clip)
            .output()
            .unwrap();
        let keyframes: Vec<i64> = String::from_utf8_lossy(&keyframes.stdout)
            .lines()
            .filter_map(|l| l.trim().trim_end_matches(',').parse::<f64>().ok())
            .map(|s| (s * 1000.0).round() as i64)
            .collect();
        let mode = if profile == 7 { DolbyVisionMode::To81 } else { DolbyVisionMode::Signal };
        let mut count = 0;
        for format in FORMATS {
            let req = JobRequest {
                input: clip.to_string_lossy().into_owned(),
                video: Some(VideoRequest { mode: VideoMode::Copy, dolby_vision: mode, ..Default::default() }),
                audio: None,
                segment_seconds: Some(2.0),
                options: TranscodeOptions { throttle: false, ..Default::default() },
                segment_format: format,
                ..Default::default()
            };
            let plan = Plan::build(&req, &media, &ff(), Some(&keyframes)).unwrap();
            let out = dir.path().join(format!("out{tried}-{format:?}"));
            std::fs::create_dir(&out).unwrap();
            let ctx = ArgsContext {
                input: &clip,
                encoder: None,
                relative_split_times: true,
                preset: None,
                shader_cache: None,
            };
            let args = plan.ffmpeg_args(&ctx, 0, plan.segment_count());
            let ran = Command::new("ffmpeg").arg("-hide_banner").args(&args).current_dir(&out).output().unwrap();
            assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
            count = plan.segment_count();
            if format == SegmentFormat::Fmp4 {
                // Cut, edited and named as jobs do it.
                for segment in cut_fmp4(&plan, &media, &ran.stdout, &out) {
                    let init = mp4_init(&segment).unwrap();
                    let dovi = init.track(init.video.unwrap()).unwrap().dovi.expect("a record");
                    assert_eq!(dovi.profile, if profile == 7 { 8 } else { profile as u8 }, "{path:?}");
                    let after = frames(&segment, 4);
                    assert!(after.contains("Dolby Vision Metadata"), "{path:?}");
                    if profile == 7 {
                        assert!(
                            after.contains("disable_residual_flag=1") && !after.contains("disable_residual_flag=0")
                        );
                        assert!(nal_types(&segment).iter().all(|au| !au.contains(&NAL_DV_EL)), "{path:?}");
                    }
                }
                continue;
            }
            for n in 0..plan.segment_count() {
                let segment = out.join(format!("{n}.ts"));
                // Every RPU of the source reads and writes back as it was.
                let data = std::fs::read(&segment).unwrap();
                tsedit::rewrite_video(&data, |es| {
                    for u in nal_units(es) {
                        let nal = &es[u.nal.clone()];
                        if dovi::nal_type(nal) == Some(NAL_DV_RPU) {
                            assert!(dovi::tests::rewrites_as_read(nal).unwrap(), "{path:?}");
                        }
                    }
                    None
                })
                .unwrap();
                if let Some(edits) = plan.segment_edits() {
                    let edited = tsedit::edit_segment(&segment, edits).unwrap();
                    assert_eq!(edited.rpus_dropped, 0, "{path:?}: {:?}", edited.first_error);
                }
                let after = frames(&segment, 4);
                assert!(after.contains("Dolby Vision Metadata"), "{path:?}");
                if profile == 7 {
                    assert!(after.contains("disable_residual_flag=1") && !after.contains("disable_residual_flag=0"));
                }
            }
        }
        eprintln!("{}: profile {profile}, {count} segments, MPEG-TS and fMP4", path.display());
        tried += 1;
        let _ = std::fs::remove_file(&clip);
    }
    assert!(tried > 0, "no Dolby Vision HEVC file in {folder:?}");
}
