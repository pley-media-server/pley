//! How a playback is asked for (packages/tv/src/player/plan.ts, the Android
//! app's `PlaybackPlan`): which file, from where, with which tracks, and the
//! body of `POST /api/playback`. Track indexes go to the server only when it
//! has to act on them: `audio_index` for a track picked, else the server
//! chooses the one this TV plays best, again for each session (a stream has
//! the one it copies); `subtitle_index` only to burn pictures into HLS, and
//! never when the TV draws them from the server's `.mks`.

use pley_core::caps::{CapsDecision, best_file, range_of, video_of};
use pley_core::model::{
    ClientCaps, DynamicRange, ItemDetail, MediaFile, MediaStream, PlaybackInfo, PlaybackMethod, PlaybackMode,
    PlaybackRequest, StreamKind,
};

use crate::quality::Quality;

/// A playlist or collection played through, for Up Next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Queue {
    Playlist(i64),
    Collection(i64),
}

/// What starts a playback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayRequest {
    pub item_id: i64,
    pub file_id: Option<i64>,
    /// Where to start; none: the resume point (or the start when watched or `from_start`).
    pub start_ms: Option<i64>,
    pub from_start: bool,
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
    pub queue: Option<Queue>,
}

impl PlayRequest {
    /// Plays `item_id` from where it was left.
    pub fn item(item_id: i64) -> PlayRequest {
        PlayRequest { item_id, ..PlayRequest::default() }
    }
}

/// A file's audio streams, in its order.
pub fn audio_streams(file: Option<&MediaFile>) -> Vec<&MediaStream> {
    file.map(|f| f.streams_of(StreamKind::Audio).collect()).unwrap_or_default()
}

/// A file's subtitle streams, in its order.
pub fn subtitle_streams(file: Option<&MediaFile>) -> Vec<&MediaStream> {
    file.map(|f| f.streams_of(StreamKind::Subtitle).collect()).unwrap_or_default()
}

/// The file to play: the one asked for, else the one that plays best here, else the first.
pub fn choose_file<'a>(
    item: &'a ItemDetail,
    requested: Option<i64>,
    caps: Option<&ClientCaps>,
) -> Option<&'a MediaFile> {
    if let Some(asked) = requested.and_then(|id| item.file(id)) {
        return Some(asked);
    }
    caps.and_then(|c| best_file(item, c)).or(item.files.first())
}

/// Where to start: as asked, else from the start when asked or watched, else the resume point.
pub fn start_ms(item: &ItemDetail, req: &PlayRequest) -> i64 {
    if let Some(ms) = req.start_ms {
        return ms.max(0);
    }
    if req.from_start || item.user_state.watched {
        return 0;
    }
    item.user_state.position_ms.max(0)
}

/// The audio to start with: the track asked for; else none, and the server
/// chooses, of the profile's language, the track this TV plays best, which
/// the session names ([`playing_audio`]).
pub fn initial_audio(file: Option<&MediaFile>, requested: Option<i64>) -> Option<MediaStream> {
    let requested = requested?;
    audio_streams(file).into_iter().find(|s| s.index == requested).cloned()
}

/// The audio a session plays: the server's `audio_index`, else the file's default, else the first.
pub fn playing_audio(file: Option<&MediaFile>, info: &PlaybackInfo) -> Option<MediaStream> {
    let audio = audio_streams(file);
    audio
        .iter()
        .find(|s| Some(s.index) == info.audio_index)
        .or_else(|| audio.iter().find(|s| s.default))
        .or(audio.first())
        .map(|s| (*s).clone())
}

/// The subtitles to start with: as asked, else the profile's choice; none is off, whatever the file flags.
pub fn initial_subtitle(file: Option<&MediaFile>, requested: Option<i64>) -> Option<MediaStream> {
    let subs = subtitle_streams(file);
    requested
        .and_then(|r| subs.iter().find(|s| s.index == r))
        .or_else(|| subs.iter().find(|s| Some(s.index) == file.and_then(|f| f.selected_subtitle)))
        .map(|s| (*s).clone())
}

/// `item` as the server answers once `audio` and `subtitle` (none: off) were
/// picked for its file `file_id`. `audio`: none leaves the audio as it was;
/// `Some(None)` is Automatic (no track picked).
pub fn with_tracks(item: &ItemDetail, file_id: i64, audio: Option<Option<i64>>, subtitle: Option<i64>) -> ItemDetail {
    let mut out = item.clone();
    for f in out.files.iter_mut().filter(|f| f.id == file_id) {
        if let Some(audio) = audio {
            f.selected_audio = audio;
            f.audio_picked = Some(audio.is_some());
        }
        f.selected_subtitle = subtitle;
    }
    out
}

/// Subtitles shown as pictures (PGS, VobSub, DVB): a stream burns them in unless the TV draws them.
pub fn is_picture(s: Option<&MediaStream>) -> bool {
    s.is_some_and(|s| s.text_based == Some(false))
}

/// The address of a picture track's `.mks`.
pub fn picture_path(file_id: i64, index: i64) -> String {
    format!("/api/files/{file_id}/subtitles/{index}.mks")
}

/// The address of a text track's WebVTT when the session doesn't name one.
pub fn text_path(file_id: i64, index: i64) -> String {
    format!("/api/files/{file_id}/subtitles/{index}.vtt")
}

/// Whether this TV decodes `stream`'s audio or sends it as it is to the receiver.
pub fn audio_plays_here(stream: Option<&MediaStream>, caps: Option<&ClientCaps>) -> bool {
    match (stream, caps) {
        (Some(s), Some(c)) => c.plays_audio(&s.codec),
        _ => true,
    }
}

/// Whether this TV opens `file`'s container as it is.
pub fn container_plays_here(file: Option<&MediaFile>, caps: Option<&ClientCaps>) -> bool {
    let container = file.and_then(|f| f.container.as_deref()).unwrap_or("").to_ascii_lowercase();
    let Some(caps) = caps.filter(|_| !container.is_empty()) else { return true };
    let has = |name: &str| caps.containers.iter().any(|c| c.eq_ignore_ascii_case(name));
    has(&container) || (container == "mkv" && has("matroska")) || (container == "matroska" && has("mkv"))
}

/// Whether the session will surely stream (HLS), before asking.
pub fn will_stream(
    mode: PlaybackMode,
    decision: Option<&CapsDecision>,
    file: Option<&MediaFile>,
    audio: Option<&MediaStream>,
    caps: Option<&ClientCaps>,
) -> bool {
    matches!(mode, PlaybackMode::Remux | PlaybackMode::Transcode)
        || decision.is_some_and(|d| *d != CapsDecision::Direct)
        || !container_plays_here(file, caps)
        || !audio_plays_here(audio, caps)
}

/// What a session is asked with.
#[derive(Clone, Debug, PartialEq)]
pub struct Choices {
    pub file: MediaFile,
    pub start_ms: i64,
    pub audio: Option<MediaStream>,
    pub subtitle: Option<MediaStream>,
    pub quality: Quality,
    pub mode: PlaybackMode,
    /// The session will stream (HLS): the server needs the track indexes.
    pub streaming: bool,
    /// Pictures are drawn by the TV from the server's `.mks`: none to burn in.
    pub sideload: bool,
}

/// The body of `POST /api/playback` for `c`.
pub fn playback_request(
    item_id: i64,
    c: &Choices,
    client_id: &str,
    caps: Option<&ClientCaps>,
    keyed: bool,
) -> PlaybackRequest {
    PlaybackRequest {
        item_id,
        file_id: Some(c.file.id),
        audio_index: c.audio.as_ref().map(|a| a.index),
        subtitle_index: c
            .subtitle
            .as_ref()
            .filter(|s| c.streaming && is_picture(Some(s)) && !c.sideload)
            .map(|s| s.index),
        max_bitrate: c.quality.max_bitrate_kbps,
        max_height: c.quality.max_height,
        start_ms: Some(c.start_ms),
        mode: c.mode,
        client_id: Some(client_id.to_string()),
        keep_previous: false,
        keyed_urls: keyed,
        caps: caps.cloned().unwrap_or_else(ClientCaps::empty),
    }
}

/// Whether `info`'s session must be asked for again with the subtitle
/// index: it streams, though it wasn't expected to, and the subtitles
/// chosen are pictures to burn in.
pub fn needs_indexes(info: &PlaybackInfo, sent: &PlaybackRequest, c: &Choices) -> bool {
    info.hls && is_picture(c.subtitle.as_ref()) && sent.subtitle_index.is_none() && !c.sideload
}

/// The container's MIME type, for players that can't tell from the address.
pub fn mime_type_of(container: Option<&str>) -> Option<&'static str> {
    match container.unwrap_or("").to_ascii_lowercase().as_str() {
        "mkv" | "matroska" | "mka" => Some("video/x-matroska"),
        "mp4" | "m4v" | "mov" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        "avi" => Some("video/x-msvideo"),
        "ts" | "m2ts" | "mpegts" => Some("video/mp2t"),
        _ => None,
    }
}

/// The Dolby Vision guards (the Android app's `DvGuards`): a session that
/// would show wrong colours is asked for again another way. A remux of
/// profile 5 loses its signalling in MPEG-TS on a TV that doesn't read the
/// master playlist's Dolby Vision codecs: transcode it. Direct play of
/// profile 7 on a TV that doesn't decode it: its HDR10 base layer.
pub fn dv_retry_mode(info: &PlaybackInfo, file: Option<&MediaFile>, caps: Option<&ClientCaps>) -> Option<PlaybackMode> {
    let video = file.and_then(video_of)?;
    if range_of(Some(video)) != DynamicRange::DolbyVision {
        return None;
    }
    match video.dv_profile {
        Some(5) => {
            let reads_master = caps.is_some_and(|c| c.hls.master_playlist && c.shows(DynamicRange::DolbyVision));
            (info.method == PlaybackMethod::Remux && !reads_master).then_some(PlaybackMode::Transcode)
        }
        Some(7) => {
            let decodes_p7 = caps.is_some_and(|c| c.dolby_vision_profiles.iter().any(|p| p.profile == 7));
            if info.method != PlaybackMethod::Direct || decodes_p7 {
                return None;
            }
            let hdr10 = caps.is_some_and(|c| c.shows(DynamicRange::Hdr10) || c.hdr);
            Some(if hdr10 { PlaybackMode::Remux } else { PlaybackMode::Transcode })
        }
        _ => None,
    }
}

/// A Dolby Vision file in a container the TV shows Dolby Vision only from
/// others (LG before webOS 25: not Matroska): played as it is, it would show
/// its base layer, so it is asked for as a remux, which keeps it.
pub fn dv_container_mode(
    file: Option<&MediaFile>,
    caps: Option<&ClientCaps>,
    dv_containers: Option<&[String]>,
) -> Option<PlaybackMode> {
    let (file, caps, dv_containers) = (file?, caps?, dv_containers?);
    let video = video_of(file)?;
    if range_of(Some(video)) != DynamicRange::DolbyVision || !caps.shows(DynamicRange::DolbyVision) {
        return None;
    }
    if !caps.dolby_vision_profiles.iter().any(|p| Some(p.profile) == video.dv_profile) {
        return None;
    }
    let lower = file.container.as_deref().unwrap_or("").to_ascii_lowercase().replacen("matroska", "mkv", 1);
    let container = lower.split(',').next().unwrap_or("");
    (!container.is_empty() && !dv_containers.iter().any(|c| c == container)).then_some(PlaybackMode::Remux)
}

/// Keyed playback addresses live under `/api/media/` (docs/API.md, "Keyed URLs").
pub fn is_keyed(url: &str) -> bool {
    let path = match url.find("://") {
        Some(i) => url[i + 3..].find('/').map_or("", |p| &url[i + 3 + p..]),
        None => url,
    };
    path.strip_prefix("/api/media/").and_then(|rest| rest.find('/')).is_some_and(|slash| slash > 0)
}

/// An image address asked for at `width` pixels (the server buckets it).
pub fn with_width(url: &str, width: u32) -> String {
    format!("{url}{}w={width}", if url.contains('?') { "&" } else { "?" })
}

/// The video stream the session shows: the default one, else the first.
pub fn video_stream_of(file: Option<&MediaFile>) -> Option<&MediaStream> {
    let file = file?;
    file.streams_of(StreamKind::Video).find(|s| s.default).or_else(|| file.streams_of(StreamKind::Video).next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_addresses() {
        assert!(is_keyed("/api/media/pley_mk_KEY/file.mkv"));
        assert!(is_keyed("http://nas:7539/api/media/pley_mk_KEY/master.m3u8"));
        assert!(!is_keyed("/api/files/7/stream"));
        assert!(!is_keyed("/api/media//x"));
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_type_of(Some("MKV")), Some("video/x-matroska"));
        assert_eq!(mime_type_of(Some("m2ts")), Some("video/mp2t"));
        assert_eq!(mime_type_of(Some("wmv")), None);
        assert_eq!(mime_type_of(None), None);
    }

    #[test]
    fn widths() {
        assert_eq!(with_width("/api/items/1/images/thumb", 480), "/api/items/1/images/thumb?w=480");
        assert_eq!(with_width("/api/items/1/images/thumb?v=2", 480), "/api/items/1/images/thumb?v=2&w=480");
    }
}
