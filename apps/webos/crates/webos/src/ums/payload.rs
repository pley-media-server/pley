//! What the engine sends uMediaServer (`luna://com.webos.media`), URI mode:
//! the methods and their payloads, and the declared ceiling of an HLS stream.
//!
//! The load payload carries what the web app's `mediaOption` carried: the
//! start in `option.transmission`, never a
//! seek after loading (that buffers from the beginning first), none for live
//! (the pipeline starts at the live edge), and `mediaTransportType: "HLS"`
//! with an `adaptiveStreaming` ceiling for HLS. Natively it adds `appId`, which
//! ties the pipeline to the app's window and foreground, and on webOS 5 and
//! later the exported window's id.

use pley_core::media::{LoadRequest, VideoFacts};
use serde_json::{Value, json};

use crate::facts::CodecTable;

/// uMediaServer's service.
pub const SERVICE: &str = "luna://com.webos.media";

/// The address of one of uMediaServer's methods.
pub fn uri(method: &str) -> String {
    format!("{SERVICE}/{method}")
}

/// The largest picture a stream declares, which LG's pipeline allocates its
/// decoder for (from webOS 10, literally: an H.264 declared at 4K 60 is
/// refused).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ceiling {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

/// Frame-rate classes a ceiling declares.
const FPS_CLASSES: [u32; 5] = [24, 25, 30, 50, 60];

/// The class of a stream's frame rate: the smallest whose frame lasts at most
/// a millisecond longer than the stream's (so 23.976 is 24, 29.97 and 30.303
/// are 30, 48 is 50); 60 when unknown or faster.
pub fn fps_class(fps: Option<f64>) -> u32 {
    let Some(fps) = fps.filter(|f| f.is_finite() && *f > 0.0) else { return 60 };
    let frame_ms = 1000.0 / fps;
    FPS_CLASSES.into_iter().find(|c| 1000.0 / f64::from(*c) <= frame_ms + 1.0).unwrap_or(60)
}

/// The ceiling for an HLS stream of `video`: H.264 (and a transcode, whose
/// codec isn't said) declares Full HD when it fits 1920×1088 (a 1080p stream's
/// coded height) and 4K otherwise, at its frame-rate class; HEVC, VP9 and AV1
/// declare 4K at 60 (8K for a larger stream). The TV's codec table, when read,
/// clamps each axis.
pub fn hls_ceiling(video: Option<&VideoFacts>, table: Option<&CodecTable>) -> Ceiling {
    let (w, h) = video.map_or((0, 0), |v| (v.width.max(v.height), v.width.min(v.height)));
    let codec = video.map_or("unknown", |v| v.codec.as_str());
    let mut c = match codec {
        "hevc" | "vp9" | "av1" if w > 4096 || h > 2304 => Ceiling { width: 7680, height: 4320, fps: 60 },
        "hevc" | "vp9" | "av1" => Ceiling { width: 3840, height: 2160, fps: 60 },
        _ => {
            let fps = fps_class(video.and_then(|v| v.frame_rate));
            if video.is_some() && w <= 1920 && h <= 1088 {
                Ceiling { width: 1920, height: 1080, fps }
            } else {
                Ceiling { width: 3840, height: 2160, fps }
            }
        }
    };
    let row_codec = if codec == "unknown" { "h264" } else { codec };
    if let Some(row) = table.and_then(|t| t.video(row_codec)) {
        c.width = c.width.min(row.max_width);
        c.height = c.height.min(row.max_height);
        c.fps = c.fps.min(row.max_fps.ceil() as u32);
    }
    c
}

/// `load`'s payload for `req`.
pub fn load(req: &LoadRequest, app_id: &str, window_id: Option<&str>, table: Option<&CodecTable>) -> Value {
    let mut option = json!({ "appId": app_id });
    if let Some(id) = window_id {
        option["windowId"] = json!(id);
    }
    if !req.kind.is_live() {
        option["transmission"] = json!({ "playTime": { "start": req.start_ms } });
    }
    let mut payload = json!({});
    if req.kind.is_hls() {
        option["adaptiveStreaming"] = if req.video.is_none() && req.audio.is_some() {
            // A song: no picture to allocate a decoder for.
            json!({ "audioOnly": true })
        } else {
            let c = hls_ceiling(req.video.as_ref(), table);
            json!({ "maxWidth": c.width, "maxHeight": c.height, "maxFrameRate": c.fps })
        };
        payload["mediaTransportType"] = json!("HLS");
    }
    payload["option"] = option;
    json!({ "uri": req.url, "type": "media", "payload": payload })
}

/// The payload naming only the media (`play`, `pause`, `unload`, `subscribe`).
pub fn media(media_id: &str) -> Value {
    json!({ "mediaId": media_id })
}

/// `seek`'s payload: milliseconds from the start.
pub fn seek(media_id: &str, position_ms: u64) -> Value {
    json!({ "mediaId": media_id, "position": position_ms })
}

/// `setPlayRate`'s payload, with the sound kept (which LG does up to 2×).
pub fn rate(media_id: &str, rate: f32) -> Value {
    json!({ "mediaId": media_id, "playRate": f64::from(rate), "audioOutput": true })
}

/// `selectTrack`'s payload for the pipeline's `index`-th audio track.
pub fn audio_track(media_id: &str, index: usize) -> Value {
    json!({ "mediaId": media_id, "type": "audio", "index": index })
}

/// `setDisplayWindow`'s payload: the picture's part and where it goes, in
/// device pixels.
pub fn display_window(media_id: &str, source: Option<[i32; 4]>, dest: [i32; 4]) -> Value {
    let rect = |r: [i32; 4]| json!({ "x": r[0], "y": r[1], "width": r[2], "height": r[3] });
    let mut payload = json!({ "mediaId": media_id, "destination": rect(dest) });
    if let Some(s) = source {
        payload["source"] = rect(s);
    }
    payload
}

#[cfg(test)]
mod tests {
    use pley_core::media::{AudioFacts, MediaKind};
    use pley_core::model::DynamicRange;

    use super::*;

    fn video(codec: &str, width: u32, height: u32, fps: Option<f64>) -> VideoFacts {
        VideoFacts {
            codec: codec.into(),
            width,
            height,
            frame_rate: fps,
            dynamic_range: DynamicRange::Sdr,
            dv_profile: None,
            bit_depth: None,
        }
    }

    #[test]
    fn a_resumed_file_starts_in_the_payload_and_names_the_window() {
        let mut req = LoadRequest::new("http://10.0.0.2:8096/api/media/k/file.mkv", MediaKind::File);
        req.start_ms = 754_000;
        assert_eq!(
            load(&req, "app.pley.tv", Some("_Window_Id_3"), None),
            json!({
                "uri": "http://10.0.0.2:8096/api/media/k/file.mkv",
                "type": "media",
                "payload": { "option": {
                    "appId": "app.pley.tv",
                    "windowId": "_Window_Id_3",
                    "transmission": { "playTime": { "start": 754_000 } }
                } }
            })
        );
    }

    #[test]
    fn a_file_on_a_drive_loads_as_a_file_at_its_address() {
        let mut req = LoadRequest::new("file:///tmp/usb/sda/sda1/Films/Mad%20Max.mkv", MediaKind::File);
        req.start_ms = 1_234_000;
        req.duration_hint_ms = Some(7_380_000);
        assert_eq!(
            load(&req, "app.pley.tv", Some("_Window_Id_3"), None),
            json!({
                "uri": "file:///tmp/usb/sda/sda1/Films/Mad%20Max.mkv",
                "type": "media",
                "payload": { "option": {
                    "appId": "app.pley.tv",
                    "windowId": "_Window_Id_3",
                    "transmission": { "playTime": { "start": 1_234_000 } }
                } }
            })
        );
        // Before webOS 5 there is no exported window to name.
        let old = load(&req, "app.pley.tv", None, None);
        assert!(old["payload"]["option"].get("windowId").is_none());
        assert!(old["payload"].get("mediaTransportType").is_none());
    }

    #[test]
    fn hls_declares_its_ceiling_and_transport() {
        let mut req = LoadRequest::new("http://h/api/media/k/master.m3u8", MediaKind::Hls);
        req.video = Some(video("h264", 1920, 1080, Some(23.976)));
        assert_eq!(
            load(&req, "app.pley.tv", None, None)["payload"],
            json!({
                "mediaTransportType": "HLS",
                "option": {
                    "appId": "app.pley.tv",
                    "transmission": { "playTime": { "start": 0 } },
                    "adaptiveStreaming": { "maxWidth": 1920, "maxHeight": 1080, "maxFrameRate": 24 }
                }
            })
        );
    }

    #[test]
    fn a_live_channel_starts_at_its_edge() {
        let mut req = LoadRequest::new("http://h/live/1/index.m3u8", MediaKind::LiveHls);
        req.start_ms = 5_000;
        let option = &load(&req, "app.pley.tv", None, None)["payload"]["option"];
        assert!(option.get("transmission").is_none());
        assert_eq!(option["adaptiveStreaming"], json!({ "maxWidth": 3840, "maxHeight": 2160, "maxFrameRate": 60 }));
        let ts = load(&LoadRequest::new("http://h/live/1.ts", MediaKind::LiveTs), "app.pley.tv", None, None);
        assert!(ts["payload"].get("mediaTransportType").is_none());
        assert!(ts["payload"]["option"].get("transmission").is_none());
    }

    #[test]
    fn a_song_over_hls_declares_no_picture() {
        let mut req = LoadRequest::new("http://h/a/index.m3u8", MediaKind::Hls);
        req.audio = Some(AudioFacts { codec: "aac".into(), channels: Some(2) });
        assert_eq!(load(&req, "x", None, None)["payload"]["option"]["adaptiveStreaming"], json!({ "audioOnly": true }));
    }

    #[test]
    fn frame_rates_fall_into_classes() {
        assert_eq!(fps_class(Some(23.976)), 24);
        assert_eq!(fps_class(Some(25.0)), 25);
        assert_eq!(fps_class(Some(29.97)), 30);
        assert_eq!(fps_class(Some(30.303)), 30);
        assert_eq!(fps_class(Some(48.0)), 50);
        assert_eq!(fps_class(Some(59.94)), 60);
        assert_eq!(fps_class(Some(120.0)), 60);
        assert_eq!(fps_class(None), 60);
        assert_eq!(fps_class(Some(f64::NAN)), 60);
    }

    #[test]
    fn ceilings_follow_the_codec_the_size_and_the_table() {
        let c = |v: Option<VideoFacts>| hls_ceiling(v.as_ref(), None);
        assert_eq!(c(Some(video("h264", 3840, 2160, Some(24.0)))), Ceiling { width: 3840, height: 2160, fps: 24 });
        assert_eq!(c(Some(video("h264", 1920, 1088, Some(50.0)))), Ceiling { width: 1920, height: 1080, fps: 50 });
        assert_eq!(c(Some(video("h264", 1080, 1920, None))), Ceiling { width: 1920, height: 1080, fps: 60 });
        assert_eq!(c(Some(video("hevc", 1920, 1080, Some(24.0)))), Ceiling { width: 3840, height: 2160, fps: 60 });
        assert_eq!(c(Some(video("av1", 7680, 4320, None))), Ceiling { width: 7680, height: 4320, fps: 60 });
        assert_eq!(c(Some(video("unknown", 1280, 720, Some(25.0)))), Ceiling { width: 1920, height: 1080, fps: 25 });
        assert_eq!(c(None), Ceiling { width: 3840, height: 2160, fps: 60 });
        let table = CodecTable::parse(
            r#"{"videoCodecs":[{"name":"HEVC","maxWidth":4096,"maxHeight":2176,"maxFrameRate":59.94},
                {"name":"H.264","maxWidth":1920,"maxHeight":1088,"maxFrameRate":30}]}"#,
        )
        .unwrap();
        assert_eq!(
            hls_ceiling(Some(&video("hevc", 3840, 2160, None)), Some(&table)),
            Ceiling { width: 3840, height: 2160, fps: 60 }
        );
        assert_eq!(
            hls_ceiling(Some(&video("h264", 3840, 2160, Some(60.0))), Some(&table)),
            Ceiling { width: 1920, height: 1088, fps: 30 }
        );
    }

    #[test]
    fn commands_name_the_media() {
        assert_eq!(uri("load"), "luna://com.webos.media/load");
        assert_eq!(media("_m1"), json!({ "mediaId": "_m1" }));
        assert_eq!(seek("_m1", 61_000), json!({ "mediaId": "_m1", "position": 61_000 }));
        assert_eq!(rate("_m1", 1.5), json!({ "mediaId": "_m1", "playRate": 1.5, "audioOutput": true }));
        assert_eq!(audio_track("_m1", 2), json!({ "mediaId": "_m1", "type": "audio", "index": 2 }));
        assert_eq!(
            display_window("_m1", None, [0, 0, 1040, 1080]),
            json!({ "mediaId": "_m1", "destination": { "x": 0, "y": 0, "width": 1040, "height": 1080 } })
        );
    }
}
