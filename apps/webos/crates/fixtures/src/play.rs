//! Playing, and what playing writes (docs/API.md, "Playback", "Items"):
//! sessions answered direct or as HLS (at keyed addresses when the player
//! can't send a token), the sample clips with Range, the sample HLS stream,
//! subtitles as WebVTT (one track partial the first time) and pictures as
//! `.mks`, seek previews and chapter pictures, bitrate changes, sessions
//! stopped from elsewhere (`session_stopped`, then `410`), the user's remote
//! (`player_command`, to what a player said it obeys, a track one of the
//! session file's); progress, with the tracks a player plays, watched marks,
//! ratings, track choices and track defaults, which change what the profile
//! reads next.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};

use crate::catalog::{Catalog, FileEntry, trickplay_id};
use crate::http::{Body, Request, Response, lock};
use crate::media::{Clip, HLS_SEGMENTS};
use crate::model::{ItemDetail, Kind, MediaFile, MediaStream, is_false};
use crate::pictures::{Shape, picture};
use crate::random::{hex_id, token};
use crate::server::{
    Answer, Cx, Need, PLAY, Route, State, bad_request, error, forbidden, free, json, locked, no_content, not_found,
    session_stopped,
};
use crate::views;
use crate::{ServerEvent, ids, pgs};

/// Pley's qualities (docs/QUALITY.md): id, kbps, lines.
const QUALITIES: [(&str, i64, i64); 7] = [
    ("30m", 30_000, 2_160),
    ("15m", 15_000, 2_160),
    ("20m", 20_000, 1_080),
    ("12m", 12_000, 1_080),
    ("8m", 8_000, 1_080),
    ("4m", 4_000, 720),
    ("2m", 2_000, 480),
];
/// Segment timestamps of an HLS session are the file's plus this
/// (`HLS_PTS_OFFSET_MS`, FFmpeg's MPEG-TS start).
const HLS_PTS_OFFSET_MS: i64 = 1_400;
/// Picture subtitles have a cue this often.
const PICTURE_EVERY_MS: i64 = 6_000;
/// Added subtitles' indexes start here, clear of the file's and its sidecars'.
const ADDED_FROM: i64 = 9_000;
/// The highest stream index a report may name: files' own below 1000, subtitle files and added
/// subtitles below 10,000.
const MAX_STREAM_INDEX: i64 = 9_999;

/// A playback session: the file it plays and how.
pub(crate) struct PlaySession {
    pub id: String,
    pub profile: i64,
    pub item: i64,
    pub file: i64,
    /// `direct`, `remux` or `transcode`.
    pub method: &'static str,
    pub client_id: Option<String>,
    pub key: Option<String>,
    /// What the stream may use at most, and uses now (kbps; transcodes).
    pub ceiling_kbps: i64,
    pub bitrate_kbps: i64,
    /// The remote's commands its player obeys, and what it plays on (docs/API.md, "Remote control").
    pub controls: Vec<String>,
    pub device_kind: Option<String>,
    /// The audio it plays and the subtitles it shows (-1: none): what its player last said, else
    /// what its play request started with; none while not known.
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
    /// The quality its Quality menu has picked, by id, as its player last said; none until it does.
    pub quality: Option<&'static str>,
}

impl PlaySession {
    /// Keeps the quality a report says its menu has picked: one that isn't the menu's, or none,
    /// keeps what was known. Whether it changed.
    fn report_quality(&mut self, quality: Option<&str>) -> bool {
        let Some(id) = quality.and_then(|q| QUALITY_MENU.iter().map(|m| m.0).find(|id| *id == q)) else {
            return false;
        };
        let changed = self.quality != Some(id);
        self.quality = Some(id);
        changed
    }

    /// Keeps the tracks a report says its player plays: one left out, or out of range, keeps
    /// what was known. Whether either changed.
    fn report_tracks(&mut self, audio_index: Option<i64>, subtitle_index: Option<i64>) -> bool {
        let before = (self.audio_index, self.subtitle_index);
        if let Some(index) = audio_index.filter(|i| (0..=MAX_STREAM_INDEX).contains(i)) {
            self.audio_index = Some(index);
        }
        if let Some(index) = subtitle_index.filter(|i| (-1..=MAX_STREAM_INDEX).contains(i)) {
            self.subtitle_index = Some(index);
        }
        (self.audio_index, self.subtitle_index) != before
    }
}

/// The Quality menu every player offers, in its order (apps/server/src/playback.rs, `MENU`): id,
/// label, and the kbps and lines a copy holds the video to.
type MenuEntry = (&'static str, &'static str, Option<(i64, i64)>);
const QUALITY_MENU: [MenuEntry; 9] = [
    ("original", "Original", None),
    ("auto", "Automatic", None),
    ("30m", "30 Mbps 4K", Some((30_000, 2160))),
    ("15m", "15 Mbps 4K", Some((15_000, 2160))),
    ("20m", "20 Mbps 1080p", Some((20_000, 1080))),
    ("12m", "12 Mbps 1080p", Some((12_000, 1080))),
    ("8m", "8 Mbps 1080p", Some((8_000, 1080))),
    ("4m", "4 Mbps 720p", Some((4_000, 720))),
    ("2m", "2 Mbps 480p", Some((2_000, 480))),
];

/// The qualities a player's menu offers for `file`, as the server works them out
/// (apps/server/src/playback.rs, `offered_qualities`): none for a file without video (music);
/// else the original and Automatic, then each copy with fewer lines than the file or at most 80 %
/// of its bitrate (every one when neither is known), named by the file's own lines when taller,
/// once for each bitrate and lines, tallest first.
fn offered_qualities(file: &MediaFile) -> Vec<(&'static str, String)> {
    if file.video_codec.is_none() {
        return Vec::new();
    }
    let (w, h) = (file.width.unwrap_or(0), file.height.unwrap_or(0));
    let lines = (w > 0 || h > 0).then_some(if w >= 3800 || h >= 2100 {
        2160
    } else if w >= 1800 || h >= 1000 {
        1080
    } else if w >= 1200 || h >= 700 {
        720
    } else {
        480
    });
    let kbps = file.bitrate.filter(|b| *b > 0).map(|b| b as f64 / 1000.0);
    let height = file.height.filter(|h| *h > 0);
    let known = kbps.is_some() || height.is_some();
    let mut out = Vec::new();
    let mut copies: Vec<(&'static str, String, i64, i64)> = Vec::new();
    for (id, label, cap) in QUALITY_MENU {
        let Some((cap_kbps, cap_lines)) = cap.filter(|_| known) else {
            out.push((id, label.to_string()));
            continue;
        };
        let fewer_lines = height.is_some_and(|h| cap_lines < h);
        let lighter = kbps.is_some_and(|k| cap_kbps as f64 <= k * 0.8);
        let shown = lines.map_or(cap_lines, |l| cap_lines.min(l));
        if (!fewer_lines && !lighter) || copies.iter().any(|c| (c.2, c.3) == (cap_kbps, shown)) {
            continue;
        }
        let label = match lines {
            Some(l) if cap_lines > l => {
                format!("{} Mbps {}", cap_kbps / 1000, if l >= 2160 { "4K".to_string() } else { format!("{l}p") })
            }
            _ => label.to_string(),
        };
        copies.push((id, label, cap_kbps, shown));
    }
    copies.sort_by(|a, b| b.3.cmp(&a.3).then(b.2.cmp(&a.2)));
    out.extend(copies.into_iter().map(|c| (c.0, c.1)));
    out
}

/// The remote's commands a body's `controls` names; the ones the server doesn't know are left out.
pub(crate) fn controls_of(controls: &Value) -> Vec<String> {
    const KNOWN: [&str; 9] = ["play", "pause", "seek", "skip", "next", "previous", "audio", "subtitles", "quality"];
    let named = controls.as_array().into_iter().flatten().filter_map(Value::as_str);
    let mut list: Vec<String> = named.filter(|c| KNOWN.contains(c)).map(str::to_string).collect();
    list.dedup();
    list
}

/// A session the server stopped, and why.
pub(crate) struct Stopped {
    pub profile: i64,
    pub message: Option<String>,
}

/// The playback sessions.
#[derive(Default)]
pub(crate) struct Playback {
    pub sessions: HashMap<String, PlaySession>,
    pub stopped: HashMap<String, Stopped>,
    /// Keyed addresses' keys, by key: the session they open.
    pub keys: HashMap<String, String>,
}

impl Playback {
    fn remove(&mut self, id: &str) -> Option<PlaySession> {
        let session = self.sessions.remove(id)?;
        if let Some(key) = &session.key {
            self.keys.remove(key);
        }
        Some(session)
    }
}

/// A subtitle found online and added to a file.
#[derive(Clone, Debug)]
pub(crate) struct AddedSubtitle {
    pub index: i64,
    pub file: i64,
    pub profile: i64,
    pub language: String,
}

impl AddedSubtitle {
    pub fn stream(&self) -> MediaStream {
        MediaStream {
            index: self.index,
            kind: "subtitle",
            codec: "subrip".into(),
            language: Some(self.language.clone()),
            title: Some("Found online".into()),
            text_based: Some(true),
            external: true,
            added: true,
            ..MediaStream::default()
        }
    }

    pub fn track(&self, profile: i64) -> SubtitleTrack {
        let mut track = SubtitleTrack::of(self.file, &self.stream());
        track.deletable = self.profile == profile;
        track
    }

    fn cues(&self) -> String {
        format!("WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nFound online ({})\n\n", self.language)
    }
}

/// A subtitle track as players get it.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct SubtitleTrack {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub external: bool,
    pub text_based: bool,
    pub added: bool,
    pub url: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub deletable: bool,
}

impl SubtitleTrack {
    fn of(file: i64, stream: &MediaStream) -> SubtitleTrack {
        let text = stream.text_based == Some(true);
        SubtitleTrack {
            index: stream.index,
            codec: stream.codec.clone(),
            language: stream.language.clone(),
            title: stream.title.clone(),
            default: stream.default,
            forced: stream.forced,
            external: stream.external,
            text_based: text,
            added: stream.added,
            url: text.then(|| format!("/api/files/{file}/subtitles/{}.vtt", stream.index)),
            deletable: false,
        }
    }
}

#[derive(Serialize)]
struct PlaybackInfo {
    session_id: String,
    method: &'static str,
    url: String,
    hls: bool,
    file_id: i64,
    duration_ms: i64,
    audio_index: Option<i64>,
    subtitle_index: Option<i64>,
    burn_subtitles: bool,
    audio_converted: bool,
    subtitles: Vec<SubtitleTrack>,
    reasons: Vec<String>,
    reason: Option<String>,
    auto_quality: Option<&'static str>,
    video_range: Option<String>,
    adaptive: bool,
    live_bitrate: bool,
    pts_offset_ms: Option<i64>,
}

pub(crate) fn routes() -> Vec<Route> {
    let play = Need::Scope(PLAY);
    vec![
        locked("POST", "/api/playback", play, start),
        locked("GET", "/api/playback/*", play, status),
        locked("DELETE", "/api/playback/*", play, stop),
        locked("PUT", "/api/playback/*/bitrate", play, bitrate),
        free("GET", "/api/playback/*/*", play, session_file),
        free("GET", "/api/media/*/*", Need::Public, keyed_file),
        locked("DELETE", "/api/sessions/*", play, stop_from_elsewhere),
        locked("POST", "/api/sessions/*/command", play, command),
        // The fixture server's own (Pley lists sessions in /api/activity, for
        // its web app): to find one to stop or command by hand.
        locked("GET", "/api/sessions", play, |cx, state| {
            let me = cx.caller().profile;
            Ok(json(200, &json!({ "sessions": listed(cx.catalog(), state, me, me == ids::ALEX) })))
        }),
        free("GET", "/api/files/*/stream", play, |cx| {
            let (entry, _) = file_of(cx)?;
            let data = cx.shared.media.clip(entry.clip).ok_or_else(|| not_found("the sample media aren't here"))?;
            Ok(ranged(cx.req, data, entry.clip.content_type(), "private, max-age=3600"))
        }),
        locked("GET", "/api/files/*/subtitles/*", play, subtitles),
        free("GET", "/api/files/*/trickplay", play, |cx| {
            let (_, file) = file_of(cx)?;
            Ok(json(200, &trickplay(file)))
        }),
        free("GET", "/api/files/*/trickplay/*", play, |cx| {
            let (_, file) = file_of(cx)?;
            let info = trickplay(file);
            let sheet = cx
                .value(1)
                .strip_suffix(".jpg")
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|n| *n < info.sheets)
                .ok_or_else(|| not_found("unknown sheet"))?;
            let first = sheet * 100;
            let tiles = (info.count - first).min(100);
            let bytes = cx.shared.pictures.sheet(first, tiles, info.count, (info.width, info.height), info.columns);
            Ok(picture(bytes, "image/jpeg", "private, max-age=31536000, immutable"))
        }),
        free("GET", "/api/files/*/chapters/*", play, |cx| {
            let (entry, file) = file_of(cx)?;
            let n = cx
                .value(1)
                .strip_suffix(".jpg")
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| *n < file.chapters.len())
                .ok_or_else(|| not_found("unknown chapter"))?;
            let bytes = cx.shared.pictures.artwork(entry.id * 10 + n as i64, Shape::Wide, Some(320));
            Ok(picture(bytes, "image/jpeg", "private, max-age=31536000, immutable"))
        }),
        locked("POST", "/api/items/*/progress", play, progress),
        locked("POST", "/api/items/*/watched", play, |cx, state| mark(cx, state, true)),
        locked("POST", "/api/items/*/unwatched", play, |cx, state| mark(cx, state, false)),
        locked("PUT", "/api/items/*/streams", play, streams),
        locked("PUT", "/api/items/*/rating", play, rating),
        locked("DELETE", "/api/history/*", play, |cx, state| {
            let id = cx.id(0)?;
            let profile = state.profile_mut(cx.caller().profile);
            let before = profile.history.len();
            profile.history.retain(|r| r.id != id);
            if profile.history.len() == before {
                return Err(not_found("no such play"));
            }
            profile.version += 1;
            Ok(no_content())
        }),
        locked("PUT", "/api/me/track-defaults", play, track_defaults),
        locked("PUT", "/api/me/language", play, account_language),
    ]
}

fn file_of<'a>(cx: &'a Cx<'_>) -> Result<(FileEntry, &'a MediaFile), Response> {
    let id = cx.id(0)?;
    cx.catalog().file(id).ok_or_else(|| not_found("file not found"))
}

/// The first and last byte a `Range: bytes=…` header asks for (one range,
/// RFC 9110 §14.1.2), or `None` when it can't be served.
pub(crate) fn parse_range(header: &str, size: usize) -> Option<(usize, usize)> {
    let spec = header.strip_prefix("bytes=")?.trim();
    if spec.contains(',') || size == 0 {
        return None;
    }
    let (start, end) = spec.split_once('-')?;
    if start.is_empty() {
        let suffix: usize = end.parse().ok().filter(|n| *n > 0)?;
        return Some((size.saturating_sub(suffix), size - 1));
    }
    let first: usize = start.parse().ok()?;
    let last = if end.is_empty() { size - 1 } else { end.parse::<usize>().ok()?.min(size - 1) };
    (first < size && last >= first).then_some((first, last))
}

/// Bytes with Range support: `206` for a part, `416` for a part that isn't there.
pub(crate) fn ranged(request: &Request, data: Arc<[u8]>, content_type: &str, cache: &str) -> Response {
    let size = data.len();
    let answer = |status| {
        Response::new(status)
            .with_header("Content-Type", content_type)
            .with_header("Accept-Ranges", "bytes")
            .with_header("Cache-Control", cache)
    };
    let Some(range) = request.header("range") else { return answer(200).with_body(Body::Shared(data, 0..size)) };
    match parse_range(range, size) {
        Some((first, last)) => answer(206)
            .with_header("Content-Range", format!("bytes {first}-{last}/{size}"))
            .with_body(Body::Shared(data, first..last + 1)),
        None => Response::new(416).with_header("Content-Range", format!("bytes */{size}")),
    }
}

/// What the player said it plays (the request's `caps`), version 1 or 2.
struct Caps {
    v2: bool,
    containers: Vec<String>,
    video_codecs: Vec<String>,
    decoders: Vec<Value>,
    hdr: bool,
    dolby_vision: bool,
    hdr_types: Vec<String>,
    dv_profiles: Vec<i64>,
    audio: Vec<String>,
    switches_tracks: bool,
    master_playlist: bool,
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(str::to_ascii_lowercase).collect())
        .unwrap_or_default()
}

impl Caps {
    fn read(caps: &Value) -> Caps {
        let v2 = caps["caps_version"].as_i64().unwrap_or(1) >= 2;
        let mut audio = strings(&caps["audio_decode"]);
        audio.extend(strings(&caps["audio_passthrough"]));
        if audio.is_empty() {
            audio = strings(&caps["audio_codecs"]);
        }
        Caps {
            v2,
            containers: strings(&caps["containers"]),
            video_codecs: strings(&caps["video_codecs"]),
            decoders: caps["video"].as_array().cloned().unwrap_or_default(),
            hdr: caps["hdr"].as_bool().unwrap_or(false),
            dolby_vision: caps["dolby_vision"].as_bool().unwrap_or(false),
            hdr_types: strings(&caps["hdr_types"]),
            dv_profiles: caps["dolby_vision_profiles"]
                .as_array()
                .map(|a| a.iter().filter_map(|p| p["profile"].as_i64()).collect())
                .unwrap_or_default(),
            audio,
            switches_tracks: v2 && caps["audio_track_switching"].as_bool().unwrap_or(false),
            master_playlist: v2 && caps["hls"]["master_playlist"].as_bool().unwrap_or(false),
        }
    }

    fn container(&self, container: &str) -> bool {
        self.containers.iter().any(|c| c == container || (container == "mkv" && c == "matroska"))
    }

    /// Whether a decoder takes the video stream as it is (codec, size, rate,
    /// depth, level and profile).
    fn decodes(&self, video: &MediaStream) -> bool {
        if !self.v2 || self.decoders.is_empty() {
            return self.video_codecs.contains(&video.codec);
        }
        let (w, h) = (video.width.unwrap_or(0), video.height.unwrap_or(0));
        self.decoders.iter().any(|d| {
            let limit = |name: &str| d[name].as_i64();
            let fits_size = match (limit("max_width"), limit("max_height")) {
                (Some(mw), Some(mh)) => (w <= mw && h <= mh) || (w <= mh && h <= mw),
                _ => true,
            };
            let profiles = strings(&d["profiles"]);
            let profile = video.profile.as_deref().unwrap_or("").to_ascii_lowercase();
            d["codec"].as_str().is_some_and(|c| c.eq_ignore_ascii_case(&video.codec))
                && fits_size
                && d["max_fps"].as_f64().is_none_or(|max| video.frame_rate.unwrap_or(0.0) <= max + 0.01)
                && limit("max_bit_depth").is_none_or(|max| video.bit_depth.unwrap_or(8) <= max)
                && limit("max_level").is_none_or(|max| video.level.unwrap_or(0) <= max)
                && (profiles.is_empty() || profile.is_empty() || profiles.contains(&profile))
        })
    }

    /// Whether the screen shows the stream's dynamic range.
    fn shows_range(&self, video: &MediaStream) -> bool {
        let range = video.dynamic_range.unwrap_or("sdr");
        if !self.v2 {
            return match range {
                "sdr" => true,
                "dolby_vision" if video.dv_profile == Some(5) => self.dolby_vision,
                "dolby_vision" => self.hdr || self.dolby_vision,
                _ => self.hdr,
            };
        }
        let has = |t: &str| self.hdr_types.iter().any(|h| h == t);
        match range {
            "sdr" => true,
            "hdr10plus" => has("hdr10plus") || has("hdr10"),
            "dolby_vision" => {
                let profile = video.dv_profile.unwrap_or(0);
                // A base layer of HDR10 plays where Dolby Vision doesn't.
                (has("dolby_vision") && self.dv_profiles.contains(&profile))
                    || (matches!(video.dv_bl_compat, Some(1 | 6)) && has("hdr10"))
            }
            other => has(other),
        }
    }

    fn plays_audio(&self, codec: &str) -> bool {
        self.audio.iter().any(|a| a == codec)
    }
}

/// How the stream's picture is named ("Dolby Vision P8.1", "HDR10").
fn range_label(video: &MediaStream) -> Option<String> {
    Some(match video.dynamic_range? {
        "hdr10" => "HDR10".into(),
        "hdr10plus" => "HDR10+".into(),
        "hlg" => "HLG".into(),
        "dolby_vision" => match (video.dv_profile, video.dv_bl_compat) {
            (Some(8), Some(compat)) => format!("Dolby Vision P8.{compat}"),
            (Some(profile), _) => format!("Dolby Vision P{profile}"),
            _ => "Dolby Vision".into(),
        },
        _ => return None,
    })
}

/// A file's subtitle tracks for players, with the ones added online.
fn subtitle_tracks(state: &State, file: &MediaFile, profile: i64) -> Vec<SubtitleTrack> {
    let mut tracks: Vec<SubtitleTrack> =
        file.streams.iter().filter(|s| s.is("subtitle")).map(|s| SubtitleTrack::of(file.id, s)).collect();
    if let Some(added) = state.added_subtitles.get(&file.id) {
        tracks.extend(added.iter().map(|a| a.track(profile)));
    }
    tracks
}

/// The audio of a request that names none, as Pley picks it, in short: the
/// profile's track for the item (its choice, else its language's, else the
/// file's default); when the player can't play that one as it is, another of
/// its language it can, not a commentary.
fn audio_for<'a>(
    item: &ItemDetail,
    file: &'a MediaFile,
    profile: &crate::household::Profile,
    caps: &Caps,
) -> Option<&'a MediaStream> {
    let audio: Vec<&MediaStream> = file.streams.iter().filter(|s| s.is("audio")).collect();
    let selected = views::with_choices(item, file, profile).selected_audio;
    let chosen = audio
        .iter()
        .find(|s| Some(s.index) == selected)
        .or_else(|| audio.iter().find(|s| s.default))
        .or(audio.first())
        .copied()?;
    if caps.plays_audio(&chosen.codec) {
        return Some(chosen);
    }
    let other = audio.into_iter().find(|s| {
        s.language == chosen.language
            && caps.plays_audio(&s.codec)
            && !s.title.as_deref().unwrap_or("").to_ascii_lowercase().contains("commentary")
    });
    Some(other.unwrap_or(chosen))
}

/// The quality a transcode of the original aims for: the best that fits the
/// file's lines and 1.5 times its bitrate.
fn auto_quality(file: &MediaFile) -> (&'static str, i64) {
    let kbps = file.bitrate.unwrap_or(0) / 1_000;
    let lines = file.height.unwrap_or(1_080);
    QUALITIES
        .iter()
        .find(|(_, q, l)| *l <= lines.max(480) && *q * 2 <= kbps * 3)
        .map(|(id, q, _)| (*id, *q))
        .unwrap_or(("2m", 2_000))
}

/// POST /api/playback: direct when the caps play the file as it is
/// (container, codecs, dynamic range, the default audio or a player that
/// switches tracks itself, nothing burned in, bitrate and height), else HLS:
/// remux when the video plays, else transcode.
fn start(cx: &Cx<'_>, state: &mut State) -> Answer {
    let body =
        cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't a playback request"))?;
    let item_id = body["item_id"].as_i64().ok_or_else(|| bad_request("item_id is needed"))?;
    let catalog = cx.catalog();
    let item = catalog.item(item_id).ok_or_else(|| not_found(&format!("item {item_id} not found")))?;
    if !item.kind().playable() {
        return Err(bad_request("this item cannot be played directly"));
    }
    let file = match body["file_id"].as_i64() {
        Some(id) => item.files.iter().find(|f| f.id == id).ok_or_else(|| not_found("file not found"))?,
        None => item.files.first().ok_or_else(|| not_found("file not found"))?,
    };
    let me = cx.caller().profile;
    let caps = Caps::read(&body["caps"]);
    let audio: Vec<&MediaStream> = file.streams.iter().filter(|s| s.is("audio")).collect();
    let default_audio = audio.iter().find(|s| s.default).or(audio.first()).copied();
    let asked_audio = body["audio_index"].as_i64();
    if let Some(index) = asked_audio
        && !audio.iter().any(|s| s.index == index)
    {
        return Err(bad_request("no such audio track"));
    }
    let chosen_audio = match asked_audio {
        Some(index) => audio.iter().find(|s| s.index == index).copied(),
        None => audio_for(item, file, state.profile(me), &caps),
    };
    let burn = body["subtitle_index"]
        .as_i64()
        .and_then(|index| file.streams.iter().find(|s| s.index == index && s.is("subtitle")))
        .filter(|s| s.text_based == Some(false));
    let video = file.streams.iter().find(|s| s.is("video"));
    let max_kbps = body["max_bitrate"].as_i64().filter(|k| *k > 0);
    let max_height = body["max_height"].as_i64().filter(|h| *h > 0);

    let mut reasons: Vec<String> = Vec::new();
    let container = file.container.clone().unwrap_or_default();
    if !caps.container(&container) {
        reasons.push(format!("{} container not supported.", container.to_uppercase()));
    }
    let decodes = video.is_none_or(|v| caps.decodes(v));
    let shows_range = video.is_none_or(|v| caps.shows_range(v));
    if let Some(v) = video {
        if !decodes {
            reasons.push(format!("{} video not supported.", v.codec.to_uppercase()));
        } else if !shows_range {
            reasons.push(format!("{} not supported.", range_label(v).unwrap_or_default()));
        }
    }
    let audio_plays = chosen_audio.is_none_or(|a| caps.plays_audio(&a.codec));
    if let Some(a) = chosen_audio.filter(|_| !audio_plays) {
        reasons.push(format!("{} audio not supported.", a.codec.to_uppercase()));
    }
    if chosen_audio.is_some()
        && chosen_audio.map(|a| a.index) != default_audio.map(|a| a.index)
        && !caps.switches_tracks
    {
        reasons.push("Another audio track than the file's default is played.".into());
    }
    if let Some(burn) = burn {
        reasons.push(format!(
            "{} subtitles burned in.",
            if burn.codec == "hdmv_pgs_subtitle" { "PGS" } else { "Picture" }
        ));
    }
    let lower = max_kbps.is_some_and(|k| file.bitrate.unwrap_or(0) > k * 1_000)
        || max_height.is_some_and(|h| file.height.unwrap_or(0) > h);
    if lower {
        reasons.push("Lower quality picked.".into());
    }
    let method = match body["mode"].as_str().unwrap_or("auto") {
        "direct" => "direct",
        "remux" => "remux",
        "transcode" => {
            if reasons.is_empty() {
                reasons.push("Transcode requested.".into());
            }
            "transcode"
        }
        _ if reasons.is_empty() => "direct",
        _ if decodes && shows_range && burn.is_none() && !lower => "remux",
        _ => "transcode",
    };
    if method == "direct" {
        reasons.clear();
    }

    // A new session with a client id ends that client's earlier ones, unless
    // it is made ready beside the one playing.
    let client = body["client_id"].as_str().filter(|c| (1..=128).contains(&c.len())).map(str::to_string);
    if let Some(client) = &client
        && !body["keep_previous"].as_bool().unwrap_or(false)
    {
        let old: Vec<String> = state
            .playback
            .sessions
            .values()
            .filter(|s| s.profile == me && s.client_id.as_deref() == Some(client))
            .map(|s| s.id.clone())
            .collect();
        for id in old {
            state.playback.remove(&id);
        }
    }
    let (quality, quality_kbps) = auto_quality(file);
    let ceiling = max_kbps.unwrap_or(quality_kbps);
    let id = hex_id();
    let keyed = body["keyed_urls"].as_bool().unwrap_or(false);
    let key = keyed.then(|| format!("pley_mk_{}", token(32)));
    let hls = method != "direct";
    let base = match &key {
        Some(key) => format!("/api/media/{key}"),
        None if hls => format!("/api/playback/{id}"),
        None => String::new(),
    };
    let url = if !hls {
        match &key {
            Some(_) => format!("{base}/file.{}", extension(&container)),
            None => format!("/api/files/{}/stream", file.id),
        }
    } else if caps.master_playlist {
        format!("{base}/master.m3u8")
    } else {
        format!("{base}/index.m3u8")
    };
    let video_range = video.and_then(|v| {
        let label = range_label(v)?;
        Some(if method == "transcode" && !shows_range { "SDR (tone-mapped)".to_string() } else { label })
    });
    let info = PlaybackInfo {
        session_id: id.clone(),
        method,
        url,
        hls,
        file_id: file.id,
        duration_ms: file.duration_ms.unwrap_or(0),
        audio_index: chosen_audio.map(|a| a.index),
        subtitle_index: burn.map(|s| s.index),
        burn_subtitles: burn.is_some(),
        audio_converted: hls && !audio_plays,
        subtitles: subtitle_tracks(state, file, me),
        reason: (!reasons.is_empty()).then(|| reasons.join(" ")),
        reasons,
        auto_quality: (method == "transcode" && max_kbps.is_none()).then_some(quality),
        video_range,
        adaptive: false,
        live_bitrate: method == "transcode",
        pts_offset_ms: hls.then_some(HLS_PTS_OFFSET_MS),
    };
    if let Some(key) = &key {
        state.playback.keys.insert(key.clone(), id.clone());
    }
    // Until the player says, remotes see what it starts with: the subtitles burned in, else those
    // the profile's choices pick (which players start with), none for a file without subtitles.
    let subtitle_index = match burn {
        Some(s) => Some(s.index),
        None if info.subtitles.is_empty() => None,
        None => Some(views::with_choices(item, file, state.profile(me)).selected_subtitle.unwrap_or(-1)),
    };
    state.playback.sessions.insert(
        id.clone(),
        PlaySession {
            id,
            profile: me,
            item: item.id(),
            file: file.id,
            method,
            client_id: client,
            key,
            ceiling_kbps: ceiling,
            bitrate_kbps: ceiling,
            controls: controls_of(&body["controls"]),
            device_kind: body["device_kind"].as_str().map(str::to_string),
            audio_index: chosen_audio.map(|a| a.index),
            subtitle_index,
            quality: None,
        },
    );
    cx.shared.events.emit(&ServerEvent::SessionsChanged);
    Ok(json(200, &info))
}

/// A keyed direct address's extension: the container when it is a short
/// word, else `bin`.
fn extension(container: &str) -> &str {
    if !container.is_empty() && container.len() <= 8 && container.bytes().all(|b| b.is_ascii_alphanumeric()) {
        container
    } else {
        "bin"
    }
}

/// The caller's own session `id`, or why not.
fn own_session<'a>(state: &'a mut State, id: &str, me: i64) -> Result<&'a mut PlaySession, Response> {
    if let Some(stopped) = state.playback.stopped.get(id)
        && stopped.profile == me
    {
        return Err(session_stopped(stopped.message.as_deref()));
    }
    match state.playback.sessions.get_mut(id) {
        Some(session) if session.profile == me => Ok(session),
        Some(_) => Err(forbidden("not your session")),
        None => Err(not_found("playback session expired")),
    }
}

/// GET /api/playback/{session}: how it plays, and a transcode's progress.
fn status(cx: &Cx<'_>, state: &mut State) -> Answer {
    let session = own_session(state, cx.value(0), cx.caller().profile)?;
    let transcode = (session.method != "direct").then(|| {
        let file = cx.catalog().file(session.file).map(|(_, f)| f);
        let video = file.and_then(|f| f.streams.iter().find(|s| s.is("video")));
        let copy = session.method == "remux";
        let describe =
            |v: &MediaStream| format!("{} {}×{}", v.codec.to_uppercase(), v.width.unwrap_or(0), v.height.unwrap_or(0));
        json!({
            "job_id": format!("job-{}", &session.id[..8]),
            "state": "finished",
            "video": if copy { "copy" } else { "transcode" },
            "audio": "transcode",
            "video_in": video.map(describe),
            "video_out": if copy { video.map(describe) } else { Some("H.264 1920×1080".to_string()) },
            "audio_in": file.and_then(|f| f.audio_codec.as_deref()).map(str::to_uppercase),
            "audio_out": "AAC 2.0",
            "encoder": if copy { None } else { Some("libx264") },
            "preset": if copy { None } else { Some("veryfast") },
            "decoder": "cpu",
            "pipeline": if copy { "copy" } else { "cpu" },
            "tonemap": null,
            "dolby_vision": null,
            "hdr10plus_stripped": false,
            "subtitle_burn": false,
            "speed": 4.0,
            "fps": 96.0,
            "transcoded_ms": Clip::Sdr1080.duration_ms(),
            "segments_ready": HLS_SEGMENTS,
            "segments_total": HLS_SEGMENTS,
            "bitrate_out": session.bitrate_kbps * 1_000,
            "last_error": null,
            "rendition": 0,
            "variants": 0,
        })
    });
    Ok(json(200, &json!({ "session_id": session.id, "method": session.method, "transcode": transcode })))
}

/// DELETE /api/playback/{session}: the player is done with it.
fn stop(cx: &Cx<'_>, state: &mut State) -> Answer {
    let (id, me) = (cx.value(0), cx.caller().profile);
    if state.playback.stopped.get(id).is_some_and(|s| s.profile == me) {
        return Ok(no_content());
    }
    match state.playback.sessions.get(id) {
        Some(session) if session.profile == me => {
            state.playback.remove(id);
            cx.shared.events.emit(&ServerEvent::SessionsChanged);
            Ok(no_content())
        }
        Some(_) => Err(forbidden("not your session")),
        None => Err(not_found("playback session expired")),
    }
}

/// PUT /api/playback/{session}/bitrate `{ "max_bitrate": kbps }`: a
/// transcode's bitrate changes in place, within 1 Mbps and its own.
fn bitrate(cx: &Cx<'_>, state: &mut State) -> Answer {
    let asked =
        cx.req.json().and_then(|b| b["max_bitrate"].as_i64()).ok_or_else(|| bad_request("max_bitrate is needed"))?;
    let session = own_session(state, cx.value(0), cx.caller().profile)?;
    if session.method != "transcode" {
        return Err(bad_request("this stream's bitrate can't change as it plays"));
    }
    session.bitrate_kbps = asked.clamp(1_000, session.ceiling_kbps.max(1_000));
    Ok(json(200, &json!({ "bitrate": session.bitrate_kbps })))
}

/// DELETE /api/sessions/{id} with an optional `{ "message" }`: Alex (the
/// administrator here) stops any session, a live viewer's too, others their
/// own. The player hears `session_stopped`; the session answers `410` from
/// then on.
fn stop_from_elsewhere(cx: &Cx<'_>, state: &mut State) -> Answer {
    let (id, me) = (cx.value(0).to_string(), cx.caller().profile);
    let admin = me == ids::ALEX;
    let message =
        cx.req.json().and_then(|b| b["message"].as_str().map(|m| m.trim().to_string())).filter(|m| !m.is_empty());
    if message.as_ref().is_some_and(|m| m.chars().count() > 200) {
        return Err(bad_request("the message is longer than 200 characters"));
    }
    if let Some((owner, _)) = state.live.viewer(&id) {
        if owner != me && !admin {
            return Err(forbidden("not your session"));
        }
        state.live.end(&id, message.clone());
        cx.shared
            .events
            .emit_to(owner, &ServerEvent::SessionStopped { session_id: id, message, by_admin: owner != me });
        cx.shared.events.emit(&ServerEvent::SessionsChanged);
        return Ok(no_content());
    }
    let Some(owner) = state.playback.sessions.get(&id).map(|s| s.profile) else {
        let stopped = state.playback.stopped.get(&id).ok_or_else(|| not_found("no such session"))?;
        return if stopped.profile == me || admin { Ok(no_content()) } else { Err(forbidden("not your session")) };
    };
    if owner != me && !admin {
        return Err(forbidden("not your session"));
    }
    // Its key keeps answering, with 410.
    if let Some(session) = state.playback.sessions.remove(&id)
        && let Some(key) = session.key
    {
        state.playback.keys.insert(key, id.clone());
    }
    state.playback.stopped.insert(id.clone(), Stopped { profile: owner, message: message.clone() });
    cx.shared.events.emit_to(owner, &ServerEvent::SessionStopped { session_id: id, message, by_admin: owner != me });
    cx.shared.events.emit(&ServerEvent::SessionsChanged);
    Ok(no_content())
}

/// The sessions `me` sees in the fixture's own list: their own, or everyone's for an administrator;
/// videos and music (`kind: "item"`), with the tracks their file has and which play, and the
/// qualities its menu offers and which is picked, then live viewers.
pub(crate) fn listed(catalog: &Catalog, state: &State, me: i64, admin: bool) -> Vec<Value> {
    let mut list: Vec<&PlaySession> = state.playback.sessions.values().filter(|s| admin || s.profile == me).collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    let mut list: Vec<Value> = list
        .into_iter()
        .map(|s| {
            let file = catalog.file(s.file).map(|(_, f)| f);
            let tracks = file.map(|f| track_options(state, f));
            let qualities: Vec<Value> = file
                .map(offered_qualities)
                .unwrap_or_default()
                .into_iter()
                .map(|(id, label)| json!({ "id": id, "label": label }))
                .collect();
            json!({
                "session_id": s.id, "kind": "item", "profile_id": s.profile, "item_id": s.item,
                "method": s.method, "controls": s.controls, "device_kind": s.device_kind,
                "audio_index": s.audio_index, "subtitle_index": s.subtitle_index,
                "tracks": tracks.unwrap_or_else(|| json!({ "audio": [], "subtitles": [] })),
                "quality": s.quality, "qualities": qualities,
            })
        })
        .collect();
    list.extend(state.live.listed(me, admin));
    list
}

/// A file's tracks as remotes pick among them (`PlayingSession.tracks`): its audio, then its
/// subtitles with those added to it.
fn track_options(state: &State, file: &MediaFile) -> Value {
    let option = |s: &MediaStream| {
        json!({
            "index": s.index, "language": s.language, "title": s.title,
            "codec": Some(&s.codec).filter(|c| !c.is_empty()),
            "channels": if s.is("audio") { s.channels } else { None },
            "default": s.default, "forced": s.forced,
        })
    };
    let audio: Vec<Value> = file.streams.iter().filter(|s| s.is("audio")).map(option).collect();
    let mut subtitles: Vec<Value> = file.streams.iter().filter(|s| s.is("subtitle")).map(option).collect();
    if let Some(added) = state.added_subtitles.get(&file.id) {
        subtitles.extend(added.iter().map(|a| option(&a.stream())));
    }
    json!({ "audio": audio, "subtitles": subtitles })
}

/// Whether `index` is one of file `file`'s tracks of `kind` (`audio`, `subtitle`): its own, or a
/// subtitle added to it.
pub(crate) fn has_track(catalog: &Catalog, state: &State, file: i64, kind: &str, index: i64) -> bool {
    let own = catalog.file(file).is_some_and(|(_, f)| f.streams.iter().any(|s| s.is(kind) && s.index == index));
    let added =
        kind == "subtitle" && state.added_subtitles.get(&file).is_some_and(|a| a.iter().any(|a| a.index == index));
    own || added
}

/// POST /api/sessions/{id}/command `{ command, position_ms?, offset_ms?, stream_index?, quality? }`
/// (docs/API.md, "Remote control"): one of the caller's own players, a video's, the music's or a
/// live viewer's, is sent `player_command`. Someone else's session is refused, an administrator's
/// request too; a track must be one of the session file's (subtitles -1: none), a quality one of
/// the qualities its menu offers, and Live TV has neither; a command its player didn't list
/// answers `409`.
fn command(cx: &Cx<'_>, state: &mut State) -> Answer {
    let (id, me) = (cx.value(0).to_string(), cx.caller().profile);
    let body = cx.req.json().unwrap_or(Value::Null);
    let command = body["command"].as_str().unwrap_or_default().to_string();
    if controls_of(&json!([command])).is_empty() {
        return Err(bad_request("unknown command"));
    }
    let (position_ms, offset_ms) = (body["position_ms"].as_i64(), body["offset_ms"].as_i64());
    let stream_index = body["stream_index"].as_i64();
    match command.as_str() {
        "seek" if position_ms.is_none_or(|p| p < 0) => return Err(bad_request("seek needs position_ms")),
        "skip" if offset_ms.is_none_or(|o| o.abs() > 600_000) => return Err(bad_request("skip needs offset_ms")),
        _ => {}
    }
    let (owner, controls, file) = match state.live.viewer(&id) {
        Some((owner, controls)) => (owner, controls, None),
        None => {
            let s = state.playback.sessions.get(&id).ok_or_else(|| not_found("no such session"))?;
            (s.profile, s.controls.clone(), Some(s.file))
        }
    };
    if owner != me {
        return Err(forbidden("not your session"));
    }
    let track = match command.as_str() {
        "audio" => Some("audio"),
        "subtitles" => Some("subtitle"),
        _ => None,
    };
    if let Some(kind) = track {
        let file = file.ok_or_else(|| bad_request("Live TV has no tracks to pick"))?;
        let index = stream_index.ok_or_else(|| bad_request(&format!("{command} needs stream_index")))?;
        let off = kind == "subtitle" && index == -1;
        if !off && !has_track(cx.catalog(), state, file, kind, index) {
            return Err(bad_request(&format!("stream_index must be one of the file's {kind} tracks")));
        }
    }
    let quality = body["quality"].as_str().map(str::to_string);
    if command == "quality" {
        let file = file.ok_or_else(|| bad_request("Live TV has no qualities to pick"))?;
        let offered = cx.catalog().file(file).map(|(_, f)| offered_qualities(f)).unwrap_or_default();
        if !quality.as_deref().is_some_and(|q| offered.iter().any(|(id, _)| *id == q)) {
            return Err(bad_request("quality must be one of the session's qualities"));
        }
    }
    if !controls.contains(&command) {
        return Err(json(409, &json!({ "code": "not_controllable", "message": "That player doesn't do that." })));
    }
    let (position_ms, offset_ms, stream_index, quality) = match command.as_str() {
        "seek" => (position_ms, None, None, None),
        "skip" => (None, offset_ms, None, None),
        "audio" | "subtitles" => (None, None, stream_index, None),
        "quality" => (None, None, None, quality),
        _ => (None, None, None, None),
    };
    let event = ServerEvent::PlayerCommand { session_id: id, command, position_ms, offset_ms, stream_index, quality };
    cx.shared.events.emit_to(owner, &event);
    Ok(Response::new(202))
}

/// What a session's address may name: its playlists, segments or file.
enum SessionFile {
    Index,
    Master,
    Segment(usize),
    Direct,
}

fn session_file_kind(name: &str) -> Option<SessionFile> {
    match name {
        "index.m3u8" => Some(SessionFile::Index),
        "master.m3u8" => Some(SessionFile::Master),
        _ if name.starts_with("file.") => Some(SessionFile::Direct),
        _ => {
            let n = name.strip_suffix(".ts")?;
            (!n.is_empty() && n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit()))
                .then(|| SessionFile::Segment(n.parse().unwrap_or(usize::MAX)))
        }
    }
}

/// GET /api/playback/{session}/{file}: the sample HLS stream, whatever the item.
fn session_file(cx: &Cx<'_>) -> Answer {
    let me = cx.caller().profile;
    let (method, file) = {
        let mut state = lock(&cx.shared.state);
        let session = own_session(&mut state, cx.value(0), me)?;
        (session.method, session.file)
    };
    if method == "direct" {
        return Err(not_found("playback session expired"));
    }
    serve_session_file(cx, cx.value(1), method, file, false)
}

/// GET /api/media/{key}/{file}: a session's file at its keyed address.
fn keyed_file(cx: &Cx<'_>) -> Answer {
    let (method, file) = {
        let state = lock(&cx.shared.state);
        let id = state.playback.keys.get(cx.value(0)).ok_or_else(|| not_found("playback session expired"))?;
        if let Some(stopped) = state.playback.stopped.get(id) {
            return Err(session_stopped(stopped.message.as_deref()));
        }
        let session = state.playback.sessions.get(id).ok_or_else(|| not_found("playback session expired"))?;
        (session.method, session.file)
    };
    serve_session_file(cx, cx.value(1), method, file, true)
}

fn serve_session_file(cx: &Cx<'_>, name: &str, method: &str, file: i64, keyed: bool) -> Answer {
    let kind = session_file_kind(name).ok_or_else(|| not_found("playback session expired"))?;
    let media = &cx.shared.media;
    let missing = || not_found("the sample media aren't here");
    match (kind, method) {
        (SessionFile::Direct, "direct") if keyed => {
            let (entry, _) = cx.catalog().file(file).ok_or_else(|| not_found("file not found"))?;
            let data = media.clip(entry.clip).ok_or_else(missing)?;
            Ok(ranged(cx.req, data, entry.clip.content_type(), "private, no-store"))
        }
        (SessionFile::Index, "remux" | "transcode") => {
            let data = media.bytes("hls/index.m3u8").ok_or_else(missing)?;
            Ok(Response::bytes(200, "application/vnd.apple.mpegurl", data.to_vec())
                .with_header("Cache-Control", "no-cache"))
        }
        (SessionFile::Master, "remux" | "transcode") => {
            let video = cx.catalog().file(file).and_then(|(_, f)| f.streams.iter().find(|s| s.is("video")).cloned());
            let range = match video.as_ref().and_then(|v| v.dynamic_range) {
                Some("hdr10" | "hdr10plus" | "dolby_vision") if method == "remux" => "PQ",
                _ => "SDR",
            };
            let playlist = format!(
                "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-INDEPENDENT-SEGMENTS\n\
                 #EXT-X-STREAM-INF:BANDWIDTH=2400000,AVERAGE-BANDWIDTH=2000000,CODECS=\"avc1.640028,mp4a.40.2\",\
                 RESOLUTION=1920x1080,FRAME-RATE=24.000,VIDEO-RANGE={range}\nindex.m3u8\n"
            );
            Ok(Response::bytes(200, "application/vnd.apple.mpegurl", playlist.into_bytes())
                .with_header("Cache-Control", "no-cache"))
        }
        (SessionFile::Segment(n), "remux" | "transcode") => {
            let data = media.segment(n).ok_or_else(|| not_found("playback session expired"))?;
            let len = data.len();
            Ok(Response::new(200)
                .with_header("Content-Type", "video/mp2t")
                .with_header("Cache-Control", "no-cache")
                .with_body(Body::Shared(data, 0..len)))
        }
        _ => Err(not_found("playback session expired")),
    }
}

/// The seek previews of a file: every 2 to 10 s (a 360th of it), 320×180
/// tiles, 10×10 a sheet.
#[derive(Serialize)]
struct Trickplay {
    id: String,
    interval_ms: i64,
    width: u32,
    height: u32,
    columns: u32,
    rows: u32,
    count: u32,
    sheets: u32,
}

fn trickplay(file: &MediaFile) -> Trickplay {
    let duration = file.duration_ms.unwrap_or(0);
    let interval = (duration / 360).clamp(2_000, 10_000) / 1_000 * 1_000;
    let count = (duration / interval).max(1) as u32;
    Trickplay {
        id: trickplay_id(file.id),
        interval_ms: interval,
        width: 320,
        height: 180,
        columns: 10,
        rows: 10,
        count,
        sheets: count.div_ceil(100),
    }
}

/// GET /api/files/{id}/subtitles/{index}.vtt or `.mks`. The HDR10 sample's
/// embedded track comes partial (`X-Pley-Complete: 0`) the first time it is
/// asked for, as a track still being read out of its file, and whole after.
fn subtitles(cx: &Cx<'_>, state: &mut State) -> Answer {
    let (entry, file) = file_of(cx)?;
    let name = cx.value(1);
    let unknown = || not_found("subtitle not found");
    if let Some(index) = name.strip_suffix(".mks") {
        let index: i64 = index.parse().map_err(|_| unknown())?;
        let stream = file
            .streams
            .iter()
            .find(|s| s.index == index && s.is("subtitle") && s.text_based == Some(false))
            .ok_or_else(unknown)?;
        let duration = file.duration_ms.unwrap_or(0);
        let cues: Vec<(i64, i64, String)> = (0..(duration / PICTURE_EVERY_MS).clamp(1, 400))
            .map(|n| (n * PICTURE_EVERY_MS + 500, n * PICTURE_EVERY_MS + 4_500, format!("PGS {}", n + 1)))
            .collect();
        let bytes: Arc<[u8]> = pgs::build(&cues, duration, stream.language.as_deref()).into();
        return Ok(
            ranged(cx.req, bytes, "video/x-matroska", "private, max-age=3600").with_header("X-Pley-Complete", "1")
        );
    }
    let index: i64 = name.strip_suffix(".vtt").and_then(|i| i.parse().ok()).ok_or_else(unknown)?;
    let offset = match cx.query("offset_ms") {
        Some(v) => v.parse::<i64>().map_err(|_| bad_request("offset_ms must be a number"))?.clamp(-600_000, 600_000),
        None => 0,
    };
    let vtt = |text: String| Response::bytes(200, "text/vtt; charset=utf-8", text.into_bytes());
    if let Some(added) = state.added_subtitles.get(&file.id).and_then(|l| l.iter().find(|a| a.index == index)) {
        return Ok(vtt(added.cues()).with_header("Cache-Control", "private, max-age=3600"));
    }
    let stream = file.streams.iter().find(|s| s.index == index && s.is("subtitle")).ok_or_else(unknown)?;
    if stream.text_based != Some(true) {
        return Err(unknown());
    }
    let mut cues = cues(file, stream);
    for cue in &mut cues {
        cue.0 = (cue.0 + offset).max(0);
        cue.1 = (cue.1 + offset).max(0);
    }
    let asked = state.subtitle_asks.entry((entry.id, index)).or_insert(0);
    *asked += 1;
    let partial = entry.id == ids::SAMPLE_HDR10_FILE && index == ids::SAMPLE_HDR10_SUBTITLE && *asked == 1;
    if partial {
        cues.truncate(cues.len() / 2);
        return Ok(vtt(webvtt(&cues)).with_header("X-Pley-Complete", "0").with_header("Cache-Control", "no-store"));
    }
    Ok(vtt(webvtt(&cues)).with_header("Cache-Control", "private, max-age=3600"))
}

/// A track's cues: the sample's own, else one every 30 s, one to three lines
/// in turn (with descenders: what a film's subtitles look like at the bottom).
fn cues(file: &MediaFile, stream: &MediaStream) -> Vec<(i64, i64, String)> {
    if file.id == ids::SAMPLE_HDR10_FILE {
        let french = stream.language.as_deref() == Some("fre");
        let pick = |en: &str, fr: &str| if french { fr.to_string() } else { en.to_string() };
        return vec![
            (1_000, 4_000, pick("The fixture server plays this.", "Le serveur de test joue ceci.")),
            (
                6_000,
                9_000,
                pick(
                    "Nothing here is a film:\njust a glossy, jumpy pattern.",
                    "Rien ici n'est un film :\njuste une mire qui gigote.",
                ),
            ),
            (
                11_000,
                14_000,
                pick(
                    "Test pattern and tone only,\nplaying quietly, gently, joyfully,\nlong enough to judge the edges.",
                    "Mire et tonalité seulement,\nquelques gags typographiques,\npuis le générique.",
                ),
            ),
            (16_000, 19_000, pick("The end credits.", "Le générique de fin.")),
        ];
    }
    let duration = file.duration_ms.unwrap_or(0);
    let more = [
        "",
        "\nquietly playing a gentle, jumpy pattern",
        "\nquietly playing a gentle, jumpy pattern\nlong enough to judge the edges",
    ];
    let language = stream.language.as_deref().unwrap_or("und");
    (0..(duration / 30_000).clamp(1, 400))
        .map(|n| {
            (n * 30_000 + 1_000, n * 30_000 + 4_000, format!("Line {} ({language}){}", n + 1, more[n as usize % 3]))
        })
        .collect()
}

fn webvtt(cues: &[(i64, i64, String)]) -> String {
    let time =
        |ms: i64| format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1_000 % 60, ms % 1_000);
    let mut out = String::from("WEBVTT\n\n");
    for (start, end, text) in cues {
        out.push_str(&format!("{} --> {}\n{text}\n\n", time(*start), time(*end)));
    }
    out
}

/// The device a User-Agent names, for watch history, receivers and casts.
pub(crate) fn device_name(agent: Option<&str>) -> Option<String> {
    let agent = agent?;
    Some(if agent.contains("pleyLG") || agent.contains("Web0S") || agent.contains("webOS") {
        "LG TV".to_string()
    } else if agent.starts_with("pleyAndroid") {
        "Pley for Android".to_string()
    } else {
        agent.split([' ', '/']).next().unwrap_or(agent).to_string()
    })
}

/// An item and its parent and grandparent: what `user_state_changed` names.
fn lineage(item: &ItemDetail) -> Vec<i64> {
    [Some(item.id()), item.base.parent_id, item.base.grandparent_id].into_iter().flatten().collect()
}

/// POST /api/items/{id}/progress: at 90 % the item is watched, its position
/// cleared and the play recorded; positions under 15 s are kept as 0.
fn progress(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let item = cx.catalog().item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    if !item.kind().playable() {
        return Err(bad_request("only movies, episodes, tracks and extras have a position"));
    }
    let body = cx.req.json().ok_or_else(|| bad_request("the body isn't a progress report"))?;
    let position = body["position_ms"].as_i64().ok_or_else(|| bad_request("position_ms is needed"))?.max(0);
    let report_state = body["state"].as_str().unwrap_or("playing");
    if !matches!(report_state, "playing" | "paused" | "stopped" | "buffering") {
        return Err(bad_request(&format!("unknown state '{report_state}'")));
    }
    let duration = item.base.duration_ms.or(body["duration_ms"].as_i64().filter(|d| *d > 0)).unwrap_or(0);
    let now = cx.now();
    let me = cx.caller().profile;
    let profile = state.profile_mut(me);
    let before = profile.state(id);
    let done = duration > 0 && position >= duration * 9 / 10;
    // A play counts once: reports after the 90 % mark find it watched with no position.
    let counts = done && (before.position_ms > 0 || !before.watched);
    profile.update(id, |s| {
        if done {
            s.position_ms = 0;
            s.watched = true;
            s.play_count += i64::from(counts);
        } else {
            s.position_ms = if position < 15_000 { 0 } else { position };
        }
        s.last_played_at = Some(now);
    });
    // The profile's devices hear of what changes what they show: watched, a
    // resume point started or cleared, or moved as the player pauses or stops.
    let after = profile.state(id);
    let settled = matches!(report_state, "paused" | "stopped");
    if after.watched != before.watched
        || (after.position_ms > 0) != (before.position_ms > 0)
        || (settled && after.position_ms != before.position_ms)
    {
        cx.shared.events.emit_to(me, &ServerEvent::UserStateChanged { item_ids: lineage(item), children: false });
    }
    if counts {
        state.household.add_history(me, id, now, device_name(cx.req.header("user-agent")));
        cx.shared.events.emit(&ServerEvent::LibraryChanged { library_id: item.base.library_id });
    }
    if let Some(session) = body["session_id"].as_str()
        && let Some(stopped) = state.playback.stopped.get(session).filter(|s| s.profile == me)
    {
        return Err(session_stopped(stopped.message.as_deref()));
    }
    // The tracks its player plays and the quality its menu has picked are kept on its session for
    // the profile's remotes, which hear of a change at once.
    if let Some(session) = body["session_id"].as_str().and_then(|s| state.playback.sessions.get_mut(s))
        && session.profile == me
    {
        let switched = session.report_tracks(body["audio_index"].as_i64(), body["subtitle_index"].as_i64());
        if session.report_quality(body["quality"].as_str()) || switched {
            cx.shared.events.emit(&ServerEvent::SessionsChanged);
        }
    }
    Ok(no_content())
}

/// Watched or unwatched by hand (containers: all their episodes or tracks).
/// Not a play.
fn mark(cx: &Cx<'_>, state: &mut State, watched: bool) -> Answer {
    let id = cx.id(0)?;
    let catalog = cx.catalog();
    let item = catalog.item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    let leaves = catalog.leaves(id);
    let me = cx.caller().profile;
    let profile = state.profile_mut(me);
    let changes = leaves.iter().any(|leaf| {
        let s = profile.state(*leaf);
        s.watched != watched || s.position_ms > 0
    });
    for leaf in &leaves {
        profile.update(*leaf, |s| {
            s.watched = watched;
            s.position_ms = 0;
        });
    }
    if changes {
        let children = matches!(item.kind(), Kind::Show | Kind::Season | Kind::Album | Kind::Artist);
        cx.shared.events.emit_to(me, &ServerEvent::UserStateChanged { item_ids: lineage(item), children });
    }
    cx.shared.events.emit(&ServerEvent::LibraryChanged { library_id: item.base.library_id });
    Ok(no_content())
}

/// PUT /api/items/{id}/streams: remembered for the movie, or for the whole
/// show. Without `audio_index` the audio chosen before stays: subtitles alone
/// were picked.
fn streams(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let item = cx.catalog().item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't a track choice"))?;
    let owner = if item.kind() == Kind::Episode { item.base.grandparent_id.unwrap_or(id) } else { id };
    let profile = state.profile_mut(cx.caller().profile);
    let before = profile.stream_choices.get(&owner).and_then(|c| c.audio_index);
    let audio = if body.get("audio_index").is_some() { body["audio_index"].as_i64() } else { before };
    profile.stream_choices.insert(
        owner,
        crate::household::StreamChoice { audio_index: audio, subtitle_index: body["subtitle_index"].as_i64() },
    );
    profile.version += 1;
    Ok(no_content())
}

/// PUT /api/items/{id}/rating `{ "rating": 0..10 | null }`: whole numbers
/// (half stars); `{}` or `null` clears it.
fn rating(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    cx.catalog().item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    let body = cx.req.json().ok_or_else(|| bad_request("the body isn't a rating"))?;
    let rating = match &body["rating"] {
        Value::Null => None,
        value => {
            let r = value.as_f64().ok_or_else(|| bad_request("a rating is a number"))?;
            if !(0.0..=10.0).contains(&r) {
                return Err(bad_request("a rating is 0 to 10"));
            }
            Some(r.round())
        }
    };
    state.profile_mut(cx.caller().profile).update(id, |s| s.rating = rating);
    Ok(no_content())
}

/// PUT /api/me/track-defaults: languages of 2 or 3 lower-case letters, and
/// when subtitles show; answers the profile.
fn track_defaults(cx: &Cx<'_>, state: &mut State) -> Answer {
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't track defaults"))?;
    let language = |name: &str| -> Result<Option<String>, crate::http::Response> {
        match &body[name] {
            Value::Null => Ok(None),
            Value::String(l) if (2..=3).contains(&l.len()) && l.bytes().all(|b| b.is_ascii_lowercase()) => {
                Ok(Some(l.clone()))
            }
            _ => Err(bad_request("a language is 2 or 3 lower-case letters")),
        }
    };
    let audio = language("audio_language")?;
    let subtitle = language("subtitle_language")?;
    let mode = body["subtitle_mode"].as_str().unwrap_or("forced");
    if !matches!(mode, "off" | "forced" | "foreign" | "always") {
        return Err(bad_request("subtitle_mode is off, forced, foreign or always"));
    }
    let profile = state.profile_mut(cx.caller().profile);
    profile.track_defaults = crate::household::TrackDefaults {
        audio_language: audio,
        subtitle_language: subtitle,
        subtitle_mode: mode.to_string(),
    };
    profile.version += 1;
    Ok(json(200, &views::me(profile, cx.caller().scope())))
}

/// PUT /api/me/language: `en`, `fr`, `es`, or null for Automatic; answers the profile.
fn account_language(cx: &Cx<'_>, state: &mut State) -> Answer {
    if let Some(status) = state.language_route_refusal {
        let code = if status == 405 { "method_not_allowed" } else { "not_found" };
        return Err(error(status, code, "no such route"));
    }
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't a language"))?;
    let language = match &body["language"] {
        Value::Null => None,
        Value::String(l) if matches!(l.as_str(), "en" | "fr" | "es") => Some(l.clone()),
        _ => return Err(bad_request("language is en, fr, es or null")),
    };
    let profile = state.profile_mut(cx.caller().profile);
    profile.language = language;
    profile.version += 1;
    Ok(json(200, &views::me(profile, cx.caller().scope())))
}

/// Subtitles added online to a file, for the routes that list them.
pub(crate) fn add_subtitle(state: &mut State, file: i64, profile: i64, language: &str) -> AddedSubtitle {
    let added = state.added_subtitles.entry(file).or_default();
    let index = added.iter().map(|a| a.index + 1).max().unwrap_or(ADDED_FROM);
    let subtitle = AddedSubtitle { index, file, profile, language: language.to_string() };
    added.push(subtitle.clone());
    subtitle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_as_rfc_9110_has_them() {
        assert_eq!(parse_range("bytes=0-99", 1_000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1_000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1_000), Some((900, 999)));
        assert_eq!(parse_range("bytes=990-2000", 1_000), Some((990, 999)));
        assert_eq!(parse_range("bytes=1000-", 1_000), None);
        assert_eq!(parse_range("bytes=0-1,5-6", 1_000), None);
        assert_eq!(parse_range("items=0-1", 1_000), None);
    }

    #[test]
    fn previews_every_2_to_10_seconds() {
        let catalog = crate::catalog::Catalog::new(1_379_196, 2_502_426);
        let (_, sample) = catalog.file(ids::SAMPLE_1080P_FILE).unwrap();
        let info = trickplay(sample);
        assert_eq!((info.interval_ms, info.count, info.sheets), (2_000, 10, 1));
        let long = catalog.item(100).unwrap().files.first().unwrap();
        let info = trickplay(long);
        assert!(info.interval_ms >= 2_000 && info.interval_ms <= 10_000);
        assert_eq!(info.sheets, info.count.div_ceil(100));
    }

    #[test]
    fn webvtt_times() {
        let text = webvtt(&[(3_723_004, 3_724_000, "Hi".into())]);
        assert_eq!(text, "WEBVTT\n\n01:02:03.004 --> 01:02:04.000\nHi\n\n");
    }
}
