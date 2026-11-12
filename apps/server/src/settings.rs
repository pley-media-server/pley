//! Runtime settings editable from the web UI, persisted as JSON in the
//! `settings` table and cached in memory.

use std::sync::Arc;

use pley_transcoder_api::{
    Capabilities, DolbyVisionFel, EncoderInfo, HwAccel, TonemapMode, Tonemapper, TranscodeOptions, TranscoderInfo,
    VideoCodec,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::db::Db;
use crate::error::AppError;

const KEY: &str = "server";

/// Where TV show metadata comes from.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TvMetadata {
    /// TMDB when it has a key, else TheTVDB when it has one, else TVmaze.
    #[default]
    Auto,
    Tmdb,
    Tvdb,
    /// No key needed.
    Tvmaze,
}

/// When library work runs: intro and credits detection, and seek previews
/// of whole libraries (see [`crate::schedule`]). What plays, titles people
/// open and analyses asked for don't wait for it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LibraryWork {
    /// What may be watched soon whenever nothing plays; the rest in the
    /// hours people rarely watch, as learned from when they do.
    #[default]
    Auto,
    /// Between set hours ([`Settings::library_work_from`]).
    Hours,
    /// Whenever nothing plays.
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub server_name: String,
    /// The server language (an ISO 639-1 code of [`crate::i18n::LANGUAGES`]):
    /// the server's own words, and devices in a language Pley doesn't have
    /// (see [`crate::i18n`]). Empty: not chosen, which reads as English.
    pub language: String,
    pub tmdb_api_key: String,
    pub metadata_language: String,
    pub tv_metadata: TvMetadata,
    pub tvdb_api_key: String,
    /// Subscriber PIN, for TheTVDB keys that need one.
    pub tvdb_pin: String,
    /// Look artists and albums up on MusicBrainz (covers from the Cover Art Archive).
    pub music_metadata: bool,
    /// Read the NFO files Kodi, Jellyfin and the like keep next to titles:
    /// their ids decide the match and their fields win over the online
    /// source's (see [`crate::library::nfo`]).
    pub read_nfo: bool,
    pub hwaccel: HwAccel,
    pub hardware_decoding: bool,
    pub tonemapping: TonemapMode,
    /// Transcode video for a player that can't play a file's as it is, or
    /// that asks for a lower quality. Off: video is only ever copied, with
    /// the sound transcoded where the player needs it, a quality asked for
    /// plays the original, and a file whose video the player can't play
    /// isn't played there ([`crate::playback::PlaybackRequest::copy_video_only`]).
    pub transcode_video: bool,
    /// Streams that transcode video, at once: counted within
    /// `max_transcodes` and never more than it, so a lower number leaves
    /// room for streams that only need their sound transcoded.
    pub max_video_transcodes: u32,
    /// Streams the transcoder makes at once, of every kind (video
    /// transcodes, remuxes, sound transcodes, music).
    pub max_transcodes: u32,
    pub transcode_preset: String,
    pub throttle_transcodes: bool,
    /// Keep transcodes' segments in memory where the transcoder can (a
    /// folder in RAM with room: Linux's `/dev/shm`), else on disk.
    pub transcode_in_memory: bool,
    /// What transcodes make of Dolby Vision profile 7 with a full
    /// enhancement layer (FEL): its HDR10 base layer alone, or that with
    /// its Dolby Vision metadata ([`DolbyVisionFel`]).
    pub dolby_vision_fel: DolbyVisionFel,
    /// Experimental: that layer is used, the full picture rebuilt from
    /// both, where the transcoder's FFmpeg can (Pley's own).
    pub dolby_vision_fel_full: bool,
    pub scan_interval_minutes: u32,
    pub watch_libraries: bool,
    /// Find intros and credits (Skip Intro / Skip Credits) in the background.
    pub detect_markers: bool,
    /// Make seek-bar preview thumbnails in the background.
    pub seek_previews: bool,
    /// Make them for whole libraries after scans, not only when a title is opened.
    pub previews_in_background: bool,
    pub library_work: LibraryWork,
    /// The hours library work runs between with [`LibraryWork::Hours`], in
    /// the server's time (0 to 23): from the hour `from` begins to the hour
    /// `to` begins, past midnight where `to` comes first; all day where
    /// they are the same.
    pub library_work_from: u8,
    pub library_work_to: u8,
    /// Library work waits while other programs keep more than this share
    /// of the processors busy, in percent; 100: it never waits for them.
    pub library_work_max_cpu: u8,
    /// Apps by other publishers may sign in (their client metadata documents
    /// are read from the internet: see [`crate::oauth`]). pley's own apps
    /// always may. Off by default; turning it off signs them out.
    pub third_party_apps: bool,
    /// Every account with a password uses two-step sign-in (see
    /// [`crate::two_step`]): one without it sets it up at its next sign-in,
    /// and is signed out everywhere until then. Off by default; only an
    /// administrator who uses it may turn it on.
    pub require_two_step: bool,
    /// Addresses with a port that pley is reached at through a reverse
    /// proxy which leaves the port out of `Host` (nginx's `$host`, Nginx
    /// Proxy Manager's default): `https://media.example.com:9999`. A request
    /// whose `Host` has no port gets the port of the one with its scheme
    /// and host name back, so pley names itself as apps reached it.
    pub proxy_addresses: Vec<String>,
    /// OpenSubtitles.com's API key, which each server's administrator gets
    /// from opensubtitles.com (pley can't ship one): subtitle search is off
    /// without it.
    pub opensubtitles_api_key: String,
    /// An opensubtitles.com account, for its larger daily download quota
    /// (without one, downloads count against the API key's).
    pub opensubtitles_username: String,
    /// Its password: saved, never sent back ([`Settings::for_admin`]).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub opensubtitles_password: String,
    /// People may ask for movies and shows the libraries don't have
    /// ([`crate::requests`]); needs a TMDB key.
    pub requests: bool,
    /// Requests an account may make in 7 days (administrators: any number);
    /// 0: no limit. A show counts each season asked for.
    pub request_limit_movies: u32,
    pub request_limit_seasons: u32,
    /// Radarr, which approved movie requests go to: its address
    /// (`http://localhost:7878`, a base path kept, no `/` at its end); empty:
    /// none, and approved movies are added by hand.
    pub radarr_url: String,
    /// Its API key: saved, never sent back ([`Settings::for_admin`]).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub radarr_api_key: String,
    /// What Radarr adds movies with: a quality profile's id (0: none chosen)
    /// and a root folder; anime's own where set (0, empty: the same).
    pub radarr_quality_profile: i64,
    pub radarr_root_folder: String,
    pub radarr_anime_quality_profile: i64,
    pub radarr_anime_root_folder: String,
    pub radarr_minimum_availability: MinimumAvailability,
    /// Radarr searches for a movie as soon as it's added.
    pub radarr_search: bool,
    /// Sonarr, as Radarr for movies, for shows.
    pub sonarr_url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sonarr_api_key: String,
    pub sonarr_quality_profile: i64,
    pub sonarr_root_folder: String,
    /// Sonarr 3's language profile (0: its first); Sonarr 4 has none.
    pub sonarr_language_profile: i64,
    pub sonarr_anime_quality_profile: i64,
    pub sonarr_anime_root_folder: String,
    pub sonarr_season_folders: bool,
    /// Sonarr searches for the seasons asked for as soon as they're added.
    pub sonarr_search: bool,
}

/// When Radarr may take a release of a movie it was sent.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MinimumAvailability {
    Announced,
    InCinemas,
    #[default]
    Released,
}

/// The settings as administrators see them: everything but the
/// OpenSubtitles password and Radarr's and Sonarr's API keys, which only
/// say whether one is saved.
#[derive(Debug, Clone, Serialize)]
pub struct AdminSettings {
    #[serde(flatten)]
    pub settings: Settings,
    pub opensubtitles_password_set: bool,
    pub radarr_api_key_set: bool,
    pub sonarr_api_key_set: bool,
}

impl Settings {
    pub fn for_admin(&self) -> AdminSettings {
        let mut settings = self.clone();
        let set = |secret: &mut String| !std::mem::take(secret).is_empty();
        AdminSettings {
            opensubtitles_password_set: set(&mut settings.opensubtitles_password),
            radarr_api_key_set: set(&mut settings.radarr_api_key),
            sonarr_api_key_set: set(&mut settings.sonarr_api_key),
            settings,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let host = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_default();
        Self {
            server_name: if host.is_empty() { "Pley".into() } else { host },
            language: String::new(),
            tmdb_api_key: String::new(),
            metadata_language: "en-US".into(),
            tv_metadata: TvMetadata::Auto,
            tvdb_api_key: String::new(),
            tvdb_pin: String::new(),
            music_metadata: true,
            read_nfo: true,
            hwaccel: HwAccel::Auto,
            hardware_decoding: true,
            tonemapping: TonemapMode::Auto,
            transcode_video: true,
            max_video_transcodes: 4,
            max_transcodes: 4,
            // Chosen by the transcoder from its load and lead (crate::transcoder).
            transcode_preset: "auto".into(),
            throttle_transcodes: true,
            transcode_in_memory: true,
            dolby_vision_fel: DolbyVisionFel::BaseLayer,
            dolby_vision_fel_full: false,
            scan_interval_minutes: 360,
            watch_libraries: true,
            detect_markers: true,
            seek_previews: true,
            previews_in_background: true,
            library_work: LibraryWork::Auto,
            library_work_from: 1,
            library_work_to: 7,
            library_work_max_cpu: 50,
            third_party_apps: false,
            require_two_step: false,
            proxy_addresses: Vec::new(),
            opensubtitles_api_key: String::new(),
            opensubtitles_username: String::new(),
            opensubtitles_password: String::new(),
            requests: false,
            request_limit_movies: 0,
            request_limit_seasons: 0,
            radarr_url: String::new(),
            radarr_api_key: String::new(),
            radarr_quality_profile: 0,
            radarr_root_folder: String::new(),
            radarr_anime_quality_profile: 0,
            radarr_anime_root_folder: String::new(),
            radarr_minimum_availability: MinimumAvailability::Released,
            radarr_search: true,
            sonarr_url: String::new(),
            sonarr_api_key: String::new(),
            sonarr_quality_profile: 0,
            sonarr_root_folder: String::new(),
            sonarr_language_profile: 0,
            sonarr_anime_quality_profile: 0,
            sonarr_anime_root_folder: String::new(),
            sonarr_season_folders: true,
            sonarr_search: true,
        }
    }
}

/// Proxy addresses kept at most.
const MAX_PROXY_ADDRESSES: usize = 16;

/// A proxy address as kept (`https://media.example.com:9999`): http or
/// https, a host and a port that isn't the scheme's own (without one,
/// `Host` names it right already), nothing else.
fn proxy_address(value: &str) -> Result<String, AppError> {
    let wrong = || AppError::bad_request(format!("{value} isn't an address like https://media.example.com:9999"));
    let url = url::Url::parse(value.trim()).map_err(|_| wrong())?;
    let plain = matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none();
    if !plain {
        return Err(wrong());
    }
    if url.port().is_none() {
        return Err(AppError::bad_request(format!(
            "{value} has no port of its own: Pley needs nothing to be reached there"
        )));
    }
    Ok(url.origin().ascii_serialization())
}

/// A Radarr or Sonarr address as kept (`http://192.168.1.5:7878/radarr`):
/// http or https and a host, its path (a base URL) kept without the `/` at
/// its end; empty: none.
pub(crate) fn arr_url(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let wrong = || AppError::bad_request(format!("{value} isn't an address like http://localhost:7878"));
    let url = url::Url::parse(value).map_err(|_| wrong())?;
    let plain = matches!(url.scheme(), "http" | "https")
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    if !plain {
        return Err(wrong());
    }
    // An address typed in; those a name leads to are checked as they are
    // resolved.
    let link_local = match url.host() {
        Some(url::Host::Ipv4(ip)) => crate::requests::arr::link_local(ip.into()),
        Some(url::Host::Ipv6(ip)) => crate::requests::arr::link_local(ip.into()),
        _ => false,
    };
    if link_local {
        return Err(AppError::bad_request(format!("{value} is a link-local address: Radarr and Sonarr aren't there")));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

impl Settings {
    /// Settings as they were saved. Ones saved before video transcodes had
    /// a limit of their own take the limit of every stream for it, so
    /// nothing changes on those servers.
    fn saved(json: &str) -> Option<Settings> {
        let mut settings: Settings = serde_json::from_str(json).ok()?;
        let written: serde_json::Value = serde_json::from_str(json).ok()?;
        if written.get("max_video_transcodes").is_none() {
            settings.max_video_transcodes = settings.max_transcodes;
        }
        Some(settings)
    }

    pub fn transcode_options(&self) -> TranscodeOptions {
        TranscodeOptions {
            hwaccel: self.hwaccel,
            hardware_decoding: self.hardware_decoding,
            tonemap: self.tonemapping,
            preset: Some(self.transcode_preset.clone()),
            throttle: self.throttle_transcodes,
            in_memory: self.transcode_in_memory,
            dolby_vision_fel: self.dolby_vision_fel,
            dolby_vision_fel_full: self.dolby_vision_fel_full,
        }
    }
}

/// The name Settings shows for a hardware acceleration.
fn accel_name(accel: HwAccel) -> &'static str {
    match accel {
        HwAccel::Auto => "Automatic",
        HwAccel::None => "Software encoding",
        HwAccel::Nvenc => "NVIDIA NVENC",
        HwAccel::Qsv => "Intel Quick Sync",
        HwAccel::Amf => "AMD AMF",
        HwAccel::Vaapi => "VAAPI",
        HwAccel::Videotoolbox => "VideoToolbox",
        HwAccel::Vulkan => "Vulkan",
    }
}

/// Why `hwaccel` can't work here, or `None` when it can (or nobody can
/// tell). `os` is the transcoder's operating system when known (a managed
/// transcoder runs on pley's), `caps` what it found. Without its encoder,
/// the transcoder encodes in software instead.
pub fn hwaccel_problem(hwaccel: HwAccel, os: Option<&str>, caps: Option<&Capabilities>) -> Option<String> {
    let name = accel_name(hwaccel);
    match (hwaccel, os) {
        (HwAccel::Auto | HwAccel::None, _) => return None,
        (HwAccel::Vaapi, Some(os)) if os != "linux" => return Some(format!("{name} works on Linux only")),
        (HwAccel::Videotoolbox, Some(os)) if os != "macos" => return Some(format!("{name} works on macOS only")),
        (HwAccel::Nvenc | HwAccel::Qsv | HwAccel::Amf, Some("macos")) => {
            return Some(format!("{name} doesn't work on macOS"));
        }
        _ => {}
    }
    // An FFmpeg that isn't there can't say what the GPU could do.
    let caps = caps.filter(|caps| caps.ffmpeg_available)?;
    let works = match hwaccel {
        HwAccel::Vulkan => caps.vulkan_encode,
        accel => caps.encoders.iter().any(|e| e.hardware && e.codec == VideoCodec::H264 && e.accel == accel),
    };
    let needs = match hwaccel {
        HwAccel::Nvenc => "an NVIDIA graphics card",
        HwAccel::Qsv => "an Intel graphics chip",
        HwAccel::Amf => "an AMD graphics card",
        HwAccel::Vaapi => "an Intel or AMD graphics chip",
        HwAccel::Videotoolbox => "an FFmpeg built with VideoToolbox",
        _ => "a GPU that FFmpeg encodes on through Vulkan",
    };
    (!works).then(|| format!("{name} isn't available here: it needs {needs}, and the transcoder found none"))
}

/// Why the tone mapper `mode` names can't work here, or `None` when it can
/// (or nobody can tell). Without it, the transcoder tone maps as Automatic
/// does.
pub fn tonemap_problem(mode: TonemapMode, caps: Option<&Capabilities>) -> Option<String> {
    let (wanted, why) = match mode {
        TonemapMode::Libplacebo => (
            Tonemapper::Libplacebo,
            "libplacebo isn't available here: it needs an FFmpeg with libplacebo and a GPU it reaches through Vulkan",
        ),
        TonemapMode::Opencl => {
            (Tonemapper::Opencl, "OpenCL tone mapping isn't available here: it needs a GPU with OpenCL drivers")
        }
        TonemapMode::Zscale => (Tonemapper::Zscale, "zscale isn't available here: this FFmpeg was built without zimg"),
        TonemapMode::Auto | TonemapMode::None => return None,
    };
    let caps = caps.filter(|caps| caps.ffmpeg_available)?;
    (!caps.tonemappers.contains(&wanted)).then(|| why.to_owned())
}

/// Why transcodes' segments can't be kept in memory here, or `None` when
/// they can (or nobody can tell: no transcoder). Without it, they go to disk.
pub fn memory_problem(info: Option<&TranscoderInfo>) -> Option<String> {
    match &info?.memory {
        Some(memory) if memory.available => None,
        Some(memory) => Some(memory.reason.clone().unwrap_or_else(|| "not available here".into())),
        None => Some("needs a newer transcoder".into()),
    }
}

/// Whether some video is decoded on a GPU here, so that
/// [`Settings::hardware_decoding`] changes something: as the transcoder's
/// planner chooses, through a GPU pipeline, Vulkan (for libplacebo) or the
/// decoder that goes with a hardware encoder.
pub fn decodes_on_gpu(caps: &Capabilities) -> bool {
    let has = |name: &str| caps.hwaccels.iter().any(|h| h == name);
    !caps.gpu_pipelines.is_empty()
        || !caps.vulkan_decode.is_empty()
        || has("videotoolbox")
        || caps.encoders.iter().filter(|e| e.hardware).any(|e| match e.accel {
            HwAccel::Nvenc => has("cuda"),
            HwAccel::Vaapi => has("vaapi"),
            HwAccel::Qsv => has("d3d11va") || has("vaapi"),
            HwAccel::Amf => has("d3d11va"),
            _ => false,
        })
}

/// Every hardware acceleration Settings can choose.
pub const ACCELS: [HwAccel; 8] = [
    HwAccel::Auto,
    HwAccel::Nvenc,
    HwAccel::Qsv,
    HwAccel::Amf,
    HwAccel::Vaapi,
    HwAccel::Videotoolbox,
    HwAccel::Vulkan,
    HwAccel::None,
];

/// The encoder transcodes in `codec` get with `hwaccel` chosen (`main10`:
/// those that keep HDR), as the transcoder picks it (`Ffmpeg::pick`): that
/// acceleration's, Automatic's first, else the software one. Vulkan's
/// encoder is no transcoder encoder of its own (only its pipeline uses it),
/// and HDR is encoded as Automatic encodes it.
fn picked_encoder(caps: &Capabilities, codec: VideoCodec, hwaccel: HwAccel, main10: bool) -> Option<&EncoderInfo> {
    let hwaccel = if main10 && hwaccel == HwAccel::Vulkan { HwAccel::Auto } else { hwaccel };
    let fits = |e: &&EncoderInfo| e.codec == codec && (e.main10 || !main10);
    let software = || caps.encoders.iter().filter(fits).find(|e| !e.hardware);
    let hardware = |wanted: Option<HwAccel>| {
        caps.encoders.iter().filter(fits).find(|e| e.hardware && wanted.is_none_or(|a| e.accel == a))
    };
    match hwaccel {
        HwAccel::None => software(),
        HwAccel::Auto => hardware(None).or_else(software),
        wanted => hardware(Some(wanted)).or_else(software),
    }
}

/// What choosing a hardware acceleration does here, by what the transcoder
/// found: Settings → Transcoder greys out what can't be used with it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AccelUse {
    /// Some video is decoded on a GPU with it: `hardware_decoding` changes
    /// something.
    pub hardware_decoding: bool,
    /// What it leaves to software encoders: `h264`, `hevc` and `hdr`
    /// (HEVC Main 10, for transcodes that keep HDR), where one works.
    pub software: Vec<&'static str>,
    /// The tone mappers that work with it, where the transcoder tried them
    /// (a VAAPI encoder takes the GPU mappers whose frames reached it in
    /// the start-up test, and zimg); `None`: every one it has.
    pub tonemappers: Option<Vec<Tonemapper>>,
}

/// What choosing `hwaccel` does with the transcoder's `caps`.
pub fn accel_use(caps: &Capabilities, hwaccel: HwAccel) -> AccelUse {
    let in_software = |codec, main10| picked_encoder(caps, codec, hwaccel, main10).is_some_and(|e| !e.hardware);
    let software =
        [("h264", VideoCodec::H264, false), ("hevc", VideoCodec::Hevc, false), ("hdr", VideoCodec::Hevc, true)]
            .into_iter()
            .filter(|(_, codec, main10)| in_software(*codec, *main10))
            .map(|(name, ..)| name)
            .collect();
    // A transcoder from before the test says nothing (zimg always passes).
    let vaapi =
        picked_encoder(caps, VideoCodec::H264, hwaccel, false).is_some_and(|e| e.hardware && e.accel == HwAccel::Vaapi);
    let tonemappers = (vaapi && !caps.vaapi_tonemappers.is_empty()).then(|| caps.vaapi_tonemappers.clone());
    AccelUse { hardware_decoding: decodes_on_gpu_with(caps, hwaccel), software, tonemappers }
}

/// Whether some video is decoded on a GPU with `hwaccel` chosen, as the
/// transcoder's planner decides: Vulkan's decoder (for libplacebo) with any
/// choice, else the zero-copy pipeline or the decoder that goes with the
/// H.264 encoder it gets (`pick_hw_decoder`). A software encoder still gets
/// CUDA's decoder where an NVIDIA card encodes, and VideoToolbox's on a Mac.
/// The transcoder's system is told by its decoders, as [`decodes_on_gpu`]
/// does: D3D11 on Windows, VAAPI on Linux.
pub fn decodes_on_gpu_with(caps: &Capabilities, hwaccel: HwAccel) -> bool {
    let has = |name: &str| caps.hwaccels.iter().any(|h| h == name);
    let nvidia = caps.encoders.iter().any(|e| e.accel == HwAccel::Nvenc) && has("cuda");
    let accel = picked_encoder(caps, VideoCodec::H264, hwaccel, false)
        .filter(|e| e.hardware)
        .map_or(HwAccel::None, |e| e.accel);
    !caps.vulkan_decode.is_empty()
        || caps.gpu_pipelines.iter().any(|p| p.accel == accel)
        || match accel {
            HwAccel::Nvenc => has("cuda"),
            HwAccel::Vaapi => has("vaapi"),
            HwAccel::Videotoolbox => has("videotoolbox"),
            HwAccel::Qsv => has("d3d11va") || has("vaapi"),
            HwAccel::Amf => has("d3d11va") || nvidia,
            _ => nvidia || has("videotoolbox"),
        }
}

/// Whether the tone mapper `mode` goes with `hwaccel`. Vulkan encodes on
/// the GPU only what libplacebo tone maps there (the planner's Vulkan
/// pipeline), and everything else in software: Automatic and libplacebo
/// only, then. Every tone mapper goes with the others, which tone map
/// before their encoder.
pub fn tonemap_goes_with(hwaccel: HwAccel, mode: TonemapMode) -> bool {
    hwaccel != HwAccel::Vulkan || matches!(mode, TonemapMode::Auto | TonemapMode::Libplacebo)
}

/// Whether hardware decoding may be `on` (or off) with `hwaccel`: Vulkan's
/// pipeline starts with Vulkan's decoder, so it is on with Vulkan.
pub fn decoding_goes_with(hwaccel: HwAccel, on: bool) -> bool {
    on || hwaccel != HwAccel::Vulkan
}

/// Fits the transcoder settings in `patch` to the hardware acceleration it
/// leaves chosen, as Settings → Transcoder does: a tone mapper or hardware
/// decoding newly asked for that doesn't go with it is refused; one saved
/// before that doesn't go with an acceleration newly chosen falls back
/// (Automatic tone mapping; hardware decoding on). A combination saved
/// before stays.
pub fn fit_to_accel(before: &Settings, patch: &mut SettingsPatch) -> Result<(), String> {
    let hwaccel = patch.hwaccel.unwrap_or(before.hwaccel);
    let switched = hwaccel != before.hwaccel;
    let name = accel_name(hwaccel);
    match patch.tonemapping {
        Some(mode) if mode != before.tonemapping && !tonemap_goes_with(hwaccel, mode) => {
            return Err(format!(
                "{name} encodes on the GPU only what libplacebo tone maps: choose libplacebo or Automatic tone mapping with it"
            ));
        }
        mode if switched && !tonemap_goes_with(hwaccel, mode.unwrap_or(before.tonemapping)) => {
            patch.tonemapping = Some(TonemapMode::Auto);
        }
        _ => {}
    }
    match patch.hardware_decoding {
        Some(on) if on != before.hardware_decoding && !decoding_goes_with(hwaccel, on) => {
            return Err(format!("{name} decodes on the GPU: hardware decoding stays on with it"));
        }
        on if switched && !decoding_goes_with(hwaccel, on.unwrap_or(before.hardware_decoding)) => {
            patch.hardware_decoding = Some(true);
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
pub struct SettingsPatch {
    pub server_name: Option<String>,
    /// Empty: not chosen (English).
    pub language: Option<String>,
    pub tmdb_api_key: Option<String>,
    pub metadata_language: Option<String>,
    pub tv_metadata: Option<TvMetadata>,
    pub tvdb_api_key: Option<String>,
    pub tvdb_pin: Option<String>,
    pub music_metadata: Option<bool>,
    pub read_nfo: Option<bool>,
    pub hwaccel: Option<HwAccel>,
    pub hardware_decoding: Option<bool>,
    pub tonemapping: Option<TonemapMode>,
    pub transcode_video: Option<bool>,
    pub max_video_transcodes: Option<u32>,
    pub max_transcodes: Option<u32>,
    pub transcode_preset: Option<String>,
    pub throttle_transcodes: Option<bool>,
    pub transcode_in_memory: Option<bool>,
    pub dolby_vision_fel: Option<DolbyVisionFel>,
    pub dolby_vision_fel_full: Option<bool>,
    pub scan_interval_minutes: Option<u32>,
    pub watch_libraries: Option<bool>,
    pub detect_markers: Option<bool>,
    pub seek_previews: Option<bool>,
    pub previews_in_background: Option<bool>,
    pub library_work: Option<LibraryWork>,
    pub library_work_from: Option<u8>,
    pub library_work_to: Option<u8>,
    pub library_work_max_cpu: Option<u8>,
    pub third_party_apps: Option<bool>,
    pub require_two_step: Option<bool>,
    pub proxy_addresses: Option<Vec<String>>,
    pub opensubtitles_api_key: Option<String>,
    pub opensubtitles_username: Option<String>,
    /// Left out: the saved one stays; empty: none.
    pub opensubtitles_password: Option<String>,
    pub requests: Option<bool>,
    pub request_limit_movies: Option<u32>,
    pub request_limit_seasons: Option<u32>,
    pub radarr_url: Option<String>,
    /// Left out: the saved one stays; empty: none.
    pub radarr_api_key: Option<String>,
    pub radarr_quality_profile: Option<i64>,
    pub radarr_root_folder: Option<String>,
    pub radarr_anime_quality_profile: Option<i64>,
    pub radarr_anime_root_folder: Option<String>,
    pub radarr_minimum_availability: Option<MinimumAvailability>,
    pub radarr_search: Option<bool>,
    pub sonarr_url: Option<String>,
    /// As Radarr's.
    pub sonarr_api_key: Option<String>,
    pub sonarr_quality_profile: Option<i64>,
    pub sonarr_root_folder: Option<String>,
    pub sonarr_language_profile: Option<i64>,
    pub sonarr_anime_quality_profile: Option<i64>,
    pub sonarr_anime_root_folder: Option<String>,
    pub sonarr_season_folders: Option<bool>,
    pub sonarr_search: Option<bool>,
}

pub struct SettingsStore {
    current: watch::Sender<Arc<Settings>>,
}

impl SettingsStore {
    pub async fn load(db: &Db) -> anyhow::Result<Self> {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = ?").bind(KEY).fetch_optional(&db.read).await?;
        let settings = stored.and_then(|json| Settings::saved(&json)).unwrap_or_default();
        Ok(Self { current: watch::Sender::new(Arc::new(settings)) })
    }

    pub fn get(&self) -> Arc<Settings> {
        self.current.borrow().clone()
    }

    /// The settings, which change as they are saved (a rename: see
    /// [`crate::bonjour`]).
    pub fn subscribe(&self) -> watch::Receiver<Arc<Settings>> {
        self.current.subscribe()
    }

    pub async fn update(&self, db: &Db, patch: SettingsPatch) -> Result<Arc<Settings>, AppError> {
        let current = self.get();
        let mut next = (*current).clone();
        if let Some(v) = patch.server_name {
            let v = v.trim().to_owned();
            if v.is_empty() || v.chars().count() > 64 {
                return Err(AppError::bad_request("server name must be 1-64 characters"));
            }
            next.server_name = v;
        }
        if let Some(v) = patch.language {
            next.language = match v.trim() {
                "" => String::new(),
                code => crate::i18n::LANGUAGES
                    .into_iter()
                    .find(|language| *language == code)
                    .ok_or_else(|| {
                        AppError::bad_request(format!("language is one of {}", crate::i18n::LANGUAGES.join(", ")))
                    })?
                    .to_owned(),
            };
        }
        if let Some(v) = patch.tmdb_api_key {
            next.tmdb_api_key = v.trim().to_owned();
        }
        if let Some(v) = patch.metadata_language {
            let v = v.trim().to_owned();
            if !(2..=10).contains(&v.len()) {
                return Err(AppError::bad_request("metadata language must look like 'en-US'"));
            }
            next.metadata_language = v;
        }
        if let Some(v) = patch.tv_metadata {
            next.tv_metadata = v;
        }
        if let Some(v) = patch.tvdb_api_key {
            next.tvdb_api_key = v.trim().to_owned();
        }
        if let Some(v) = patch.tvdb_pin {
            next.tvdb_pin = v.trim().to_owned();
        }
        if let Some(v) = patch.music_metadata {
            next.music_metadata = v;
        }
        if let Some(v) = patch.read_nfo {
            next.read_nfo = v;
        }
        match next.tv_metadata {
            TvMetadata::Tmdb if next.tmdb_api_key.is_empty() => {
                return Err(AppError::bad_request("TMDB needs an API key to be the TV source"));
            }
            TvMetadata::Tvdb if next.tvdb_api_key.is_empty() => {
                return Err(AppError::bad_request("TheTVDB needs an API key to be the TV source"));
            }
            _ => {}
        }
        if let Some(v) = patch.hwaccel {
            next.hwaccel = v;
        }
        if let Some(v) = patch.hardware_decoding {
            next.hardware_decoding = v;
        }
        if let Some(v) = patch.tonemapping {
            next.tonemapping = v;
        }
        if let Some(v) = patch.max_transcodes {
            if !(1..=64).contains(&v) {
                return Err(AppError::bad_request("max transcodes must be between 1 and 64"));
            }
            next.max_transcodes = v;
        }
        if let Some(v) = patch.max_video_transcodes {
            if !(1..=64).contains(&v) {
                return Err(AppError::bad_request("max video transcodes must be between 1 and 64"));
            }
            next.max_video_transcodes = v;
        }
        // Video transcodes are some of the streams: never more than all of them.
        next.max_video_transcodes = next.max_video_transcodes.min(next.max_transcodes);
        if let Some(v) = patch.transcode_video {
            next.transcode_video = v;
        }
        if let Some(v) = patch.transcode_preset {
            if !matches!(v.as_str(), "auto" | "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" | "medium") {
                return Err(AppError::bad_request("unknown transcode preset"));
            }
            next.transcode_preset = v;
        }
        if let Some(v) = patch.throttle_transcodes {
            next.throttle_transcodes = v;
        }
        if let Some(v) = patch.transcode_in_memory {
            next.transcode_in_memory = v;
        }
        if let Some(v) = patch.dolby_vision_fel {
            next.dolby_vision_fel = v;
        }
        if let Some(v) = patch.dolby_vision_fel_full {
            next.dolby_vision_fel_full = v;
        }
        if let Some(v) = patch.scan_interval_minutes {
            next.scan_interval_minutes = v.min(7 * 24 * 60);
        }
        if let Some(v) = patch.watch_libraries {
            next.watch_libraries = v;
        }
        if let Some(v) = patch.detect_markers {
            next.detect_markers = v;
        }
        if let Some(v) = patch.seek_previews {
            next.seek_previews = v;
        }
        if let Some(v) = patch.previews_in_background {
            next.previews_in_background = v;
        }
        if let Some(v) = patch.library_work {
            next.library_work = v;
        }
        for (hour, field) in
            [(patch.library_work_from, &mut next.library_work_from), (patch.library_work_to, &mut next.library_work_to)]
        {
            if let Some(v) = hour {
                if v > 23 {
                    return Err(AppError::bad_request("library work hours are 0 to 23"));
                }
                *field = v;
            }
        }
        if let Some(v) = patch.library_work_max_cpu {
            if !(10..=100).contains(&v) {
                return Err(AppError::bad_request("library work's processor limit must be between 10 and 100 %"));
            }
            next.library_work_max_cpu = v;
        }
        if let Some(v) = patch.third_party_apps {
            next.third_party_apps = v;
        }
        if let Some(v) = patch.require_two_step {
            next.require_two_step = v;
        }
        if let Some(v) = patch.proxy_addresses {
            let mut addresses: Vec<String> = Vec::new();
            for value in v.iter().filter(|value| !value.trim().is_empty()) {
                let address = proxy_address(value)?;
                // One port per scheme and host: which one a request came to
                // couldn't be told otherwise.
                let site =
                    |a: &str| url::Url::parse(a).map(|u| (u.scheme().to_owned(), u.host_str().map(str::to_owned)));
                if let Some(same) = addresses.iter().find(|a| site(a).ok() == site(&address).ok()) {
                    if *same != address {
                        return Err(AppError::bad_request(format!("{same} and {address} name the same host")));
                    }
                    continue;
                }
                addresses.push(address);
            }
            if addresses.len() > MAX_PROXY_ADDRESSES {
                return Err(AppError::bad_request(format!("at most {MAX_PROXY_ADDRESSES} proxy addresses")));
            }
            next.proxy_addresses = addresses;
        }
        if let Some(v) = patch.opensubtitles_api_key {
            next.opensubtitles_api_key = v.trim().to_owned();
        }
        if let Some(v) = patch.opensubtitles_username {
            let v = v.trim().to_owned();
            // Another account's password isn't this one's.
            if v != next.opensubtitles_username {
                next.opensubtitles_password.clear();
            }
            next.opensubtitles_username = v;
        }
        if let Some(v) = patch.opensubtitles_password {
            next.opensubtitles_password = v;
        }
        if next.opensubtitles_username.is_empty() {
            next.opensubtitles_password.clear();
        }
        if next.opensubtitles_api_key.chars().count() > 128
            || next.opensubtitles_username.chars().count() > 128
            || next.opensubtitles_password.chars().count() > 256
        {
            return Err(AppError::bad_request("the OpenSubtitles key, username or password is too long"));
        }
        if let Some(v) = patch.requests {
            next.requests = v;
        }
        if next.requests && next.tmdb_api_key.is_empty() {
            return Err(AppError::bad_request("requests need a TMDB key: titles are found on TMDB"));
        }
        for (limit, field) in [
            (patch.request_limit_movies, &mut next.request_limit_movies),
            (patch.request_limit_seasons, &mut next.request_limit_seasons),
        ] {
            if let Some(v) = limit {
                if v > 1000 {
                    return Err(AppError::bad_request("request limits are 0 (none) to 1000"));
                }
                *field = v;
            }
        }
        // A saved key goes only to the address it was saved for: a new one
        // needs its key, and no address keeps none.
        let addresses = [
            (
                "Radarr",
                patch.radarr_url,
                &mut next.radarr_url,
                &mut next.radarr_api_key,
                patch.radarr_api_key.is_some(),
            ),
            (
                "Sonarr",
                patch.sonarr_url,
                &mut next.sonarr_url,
                &mut next.sonarr_api_key,
                patch.sonarr_api_key.is_some(),
            ),
        ];
        for (name, url, field, key, key_given) in addresses {
            let Some(v) = url else { continue };
            let v = arr_url(&v)?;
            if v.is_empty() {
                key.clear();
            } else if v != *field && !key.is_empty() && !key_given {
                return Err(AppError::bad_request(format!("type {name}'s API key again for its new address")));
            }
            *field = v;
        }
        let texts = [
            (patch.radarr_api_key, &mut next.radarr_api_key),
            (patch.radarr_root_folder, &mut next.radarr_root_folder),
            (patch.radarr_anime_root_folder, &mut next.radarr_anime_root_folder),
            (patch.sonarr_api_key, &mut next.sonarr_api_key),
            (patch.sonarr_root_folder, &mut next.sonarr_root_folder),
            (patch.sonarr_anime_root_folder, &mut next.sonarr_anime_root_folder),
        ];
        for (text, field) in texts {
            if let Some(v) = text {
                let v = v.trim();
                if v.chars().count() > 512 {
                    return Err(AppError::bad_request("a Radarr or Sonarr key or folder is too long"));
                }
                *field = v.to_owned();
            }
        }
        let ids = [
            (patch.radarr_quality_profile, &mut next.radarr_quality_profile),
            (patch.radarr_anime_quality_profile, &mut next.radarr_anime_quality_profile),
            (patch.sonarr_quality_profile, &mut next.sonarr_quality_profile),
            (patch.sonarr_language_profile, &mut next.sonarr_language_profile),
            (patch.sonarr_anime_quality_profile, &mut next.sonarr_anime_quality_profile),
        ];
        for (id, field) in ids {
            if let Some(v) = id {
                if v < 0 {
                    return Err(AppError::bad_request("Radarr's and Sonarr's profiles are chosen by their ids"));
                }
                *field = v;
            }
        }
        if let Some(v) = patch.radarr_minimum_availability {
            next.radarr_minimum_availability = v;
        }
        for (on, field) in [
            (patch.radarr_search, &mut next.radarr_search),
            (patch.sonarr_season_folders, &mut next.sonarr_season_folders),
            (patch.sonarr_search, &mut next.sonarr_search),
        ] {
            if let Some(v) = on {
                *field = v;
            }
        }
        let json = serde_json::to_string(&next).map_err(|e| AppError::Internal(e.into()))?;
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(KEY)
        .bind(json)
        .execute(&db.write)
        .await?;
        // Ratings without a country of their own are read as the metadata
        // region's: a new region can change their ages.
        if crate::ratings::region_of(&next.metadata_language) != crate::ratings::region_of(&current.metadata_language) {
            crate::ratings::backfill(db).await?;
        }
        let next = Arc::new(next);
        self.current.send_replace(next.clone());
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> (tempfile::TempDir, Db, SettingsStore) {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        let store = SettingsStore::load(&db).await.unwrap();
        (dir, db, store)
    }

    fn patch(json: serde_json::Value) -> SettingsPatch {
        serde_json::from_value(json).unwrap()
    }

    #[tokio::test]
    async fn a_rejected_patch_changes_nothing() {
        let (_dir, db, store) = store().await;
        let before = store.get();
        let err = store
            .update(
                &db,
                patch(serde_json::json!({"max_transcodes": 2, "server_name": "Den", "transcode_preset": "slow"})),
            )
            .await;
        assert!(matches!(err, Err(AppError::BadRequest(_))));
        assert_eq!(*store.get(), *before);
        // Nor was anything saved.
        assert_eq!(*SettingsStore::load(&db).await.unwrap().get(), *before);
    }

    #[tokio::test]
    async fn values_are_checked_and_normalized() {
        let (_dir, db, store) = store().await;
        let bad = |json: serde_json::Value| {
            let (db, store) = (&db, &store);
            async move { store.update(db, patch(json)).await.is_err() }
        };
        assert!(bad(serde_json::json!({"server_name": "   "})).await);
        assert!(bad(serde_json::json!({"server_name": "x".repeat(65)})).await);
        assert!(bad(serde_json::json!({"metadata_language": "e"})).await);
        assert!(bad(serde_json::json!({"metadata_language": "en-US-x-twelve"})).await);
        assert!(bad(serde_json::json!({"language": "de"})).await);
        assert!(bad(serde_json::json!({"language": "fr-FR"})).await);
        assert!(bad(serde_json::json!({"max_transcodes": 0})).await);
        assert!(bad(serde_json::json!({"max_transcodes": 65})).await);
        assert!(bad(serde_json::json!({"max_video_transcodes": 0})).await);
        assert!(bad(serde_json::json!({"max_video_transcodes": 65})).await);
        assert!(bad(serde_json::json!({"transcode_preset": "placebo"})).await);
        assert!(bad(serde_json::json!({"library_work_from": 24})).await);
        assert!(bad(serde_json::json!({"library_work_max_cpu": 5})).await);
        assert!(bad(serde_json::json!({"library_work_max_cpu": 101})).await);

        let s = store
            .update(
                &db,
                patch(serde_json::json!({
                    "server_name": "  Den  ",
                    "language": " fr ",
                    "tmdb_api_key": "  abc  ",
                    "metadata_language": " fr-FR ",
                    "max_transcodes": 64,
                    "max_video_transcodes": 2,
                    "transcode_video": false,
                    "transcode_preset": "medium",
                    "scan_interval_minutes": 1_000_000,
                    "seek_previews": false,
                })),
            )
            .await
            .unwrap();
        assert_eq!(s.server_name, "Den");
        assert_eq!(s.language, "fr");
        assert_eq!(s.tmdb_api_key, "abc");
        assert_eq!(s.metadata_language, "fr-FR");
        assert_eq!((s.max_transcodes, s.transcode_preset.as_str()), (64, "medium"));
        assert_eq!((s.max_video_transcodes, s.transcode_video), (2, false));
        let unset = Settings::default();
        assert_eq!((unset.max_video_transcodes, unset.max_transcodes), (4, 4), "four streams of any kind, as ever");
        assert!(unset.transcode_video, "video is transcoded unless turned off");
        // Video transcodes are never more than all the streams: a limit above the total comes down to it.
        let both = serde_json::json!({"max_transcodes": 3, "max_video_transcodes": 5});
        let lowered = store.update(&db, patch(both)).await.unwrap();
        assert_eq!((lowered.max_video_transcodes, lowered.max_transcodes), (3, 3));
        let lowered = store.update(&db, patch(serde_json::json!({"max_transcodes": 2}))).await.unwrap();
        assert_eq!(
            (lowered.max_video_transcodes, lowered.max_transcodes),
            (2, 2),
            "also when only the total is lowered"
        );
        let s = store.update(&db, patch(serde_json::json!({"max_transcodes": 64}))).await.unwrap();
        assert_eq!(s.scan_interval_minutes, 7 * 24 * 60, "at most a week");
        assert_eq!(Settings::default().transcode_preset, "auto", "the transcoder chooses by default");
        assert!(!s.seek_previews);
        // Library work after the hours people rarely watch, unless set.
        assert_eq!(s.library_work, LibraryWork::Auto);
        let hours = serde_json::json!({"library_work": "hours", "library_work_from": 22, "library_work_to": 6});
        let s = store.update(&db, patch(hours)).await.unwrap();
        assert_eq!((s.library_work, s.library_work_from, s.library_work_to), (LibraryWork::Hours, 22, 6));
        // Fields left out keep their values.
        let s = store.update(&db, patch(serde_json::json!({"watch_libraries": false}))).await.unwrap();
        assert_eq!((s.server_name.as_str(), s.max_transcodes), ("Den", 64));
        assert!(!s.seek_previews && !s.watch_libraries);
        // ...and survive a restart.
        assert_eq!(*SettingsStore::load(&db).await.unwrap().get(), *s);
    }

    #[tokio::test]
    async fn subscribers_hear_of_a_rename() {
        let (_dir, db, store) = store().await;
        let mut settings = store.subscribe();
        settings.borrow_and_update();
        store.update(&db, patch(serde_json::json!({"server_name": " Den "}))).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), settings.changed()).await.unwrap().unwrap();
        assert_eq!(settings.borrow_and_update().server_name, "Den");
        // A rejected patch tells them nothing.
        assert!(store.update(&db, patch(serde_json::json!({"server_name": ""}))).await.is_err());
        assert!(!settings.has_changed().unwrap());
    }

    #[tokio::test]
    async fn a_tv_source_needs_its_key() {
        let (_dir, db, store) = store().await;
        let set = |json: serde_json::Value| store.update(&db, patch(json));
        assert!(set(serde_json::json!({"tv_metadata": "tmdb"})).await.is_err());
        assert!(set(serde_json::json!({"tv_metadata": "tvdb"})).await.is_err());
        assert_eq!(set(serde_json::json!({"tv_metadata": "tvmaze"})).await.unwrap().tv_metadata, TvMetadata::Tvmaze);
        // A key and the source that needs it can come in one patch.
        let s = set(serde_json::json!({"tv_metadata": "tvdb", "tvdb_api_key": "key", "tvdb_pin": " 1234 "})).await;
        let s = s.unwrap();
        assert_eq!((s.tv_metadata, s.tvdb_pin.as_str()), (TvMetadata::Tvdb, "1234"));
        // Removing the key of the source in use is refused.
        assert!(set(serde_json::json!({"tvdb_api_key": "  "})).await.is_err());
        assert_eq!(store.get().tvdb_api_key, "key");
        // Auto never needs a key.
        let s = set(serde_json::json!({"tv_metadata": "auto", "tvdb_api_key": ""})).await.unwrap();
        assert_eq!((s.tv_metadata, s.tvdb_api_key.as_str()), (TvMetadata::Auto, ""));
    }

    #[tokio::test]
    async fn stored_settings_from_older_versions_get_defaults() {
        let (_dir, db, _) = store().await;
        // Saved before the TV sources and previews existed.
        sqlx::query("INSERT INTO settings (key, value) VALUES ('server', ?)")
            .bind(r#"{"server_name":"Attic","max_transcodes":2,"hwaccel":"nvenc"}"#)
            .execute(&db.write)
            .await
            .unwrap();
        let s = SettingsStore::load(&db).await.unwrap().get();
        assert_eq!((s.server_name.as_str(), s.max_transcodes, s.hwaccel), ("Attic", 2, HwAccel::Nvenc));
        assert_eq!(s.max_video_transcodes, 2, "saved before video had a limit of its own: the one it had");
        assert_eq!(s.tv_metadata, TvMetadata::Auto);
        assert!(s.music_metadata && s.read_nfo && s.seek_previews && s.previews_in_background && s.detect_markers);
        assert_eq!((s.library_work, s.library_work_max_cpu), (LibraryWork::Auto, 50));

        // Unreadable settings fall back to the defaults instead of failing start-up.
        sqlx::query("UPDATE settings SET value = 'not json' WHERE key = 'server'").execute(&db.write).await.unwrap();
        let s = SettingsStore::load(&db).await.unwrap().get();
        assert_eq!(s.max_transcodes, Settings::default().max_transcodes);
    }

    #[tokio::test]
    async fn apps_by_other_publishers_stay_off_until_allowed() {
        let (_dir, db, store) = store().await;
        assert!(!store.get().third_party_apps, "off on a new server");
        // Saved by a version that didn't have the setting.
        sqlx::query("INSERT INTO settings (key, value) VALUES ('server', '{\"server_name\":\"Attic\"}')")
            .execute(&db.write)
            .await
            .unwrap();
        assert!(!SettingsStore::load(&db).await.unwrap().get().third_party_apps, "off after an upgrade");
        let s = store.update(&db, patch(serde_json::json!({"third_party_apps": true}))).await.unwrap();
        assert!(s.third_party_apps);
        assert!(SettingsStore::load(&db).await.unwrap().get().third_party_apps, "kept");
        // Other changes leave it alone.
        let s = store.update(&db, patch(serde_json::json!({"server_name": "Den"}))).await.unwrap();
        assert!(s.third_party_apps);
        let s = store.update(&db, patch(serde_json::json!({"third_party_apps": false}))).await.unwrap();
        assert!(!s.third_party_apps);
    }

    #[tokio::test]
    async fn two_step_sign_in_is_required_only_once_asked_for() {
        let (_dir, db, store) = store().await;
        assert!(!store.get().require_two_step, "off on a new server");
        sqlx::query("INSERT INTO settings (key, value) VALUES ('server', '{\"server_name\":\"Attic\"}')")
            .execute(&db.write)
            .await
            .unwrap();
        assert!(!SettingsStore::load(&db).await.unwrap().get().require_two_step, "off after an upgrade");
        let s = store.update(&db, patch(serde_json::json!({"require_two_step": true}))).await.unwrap();
        assert!(s.require_two_step);
        assert!(SettingsStore::load(&db).await.unwrap().get().require_two_step, "kept");
        let s = store.update(&db, patch(serde_json::json!({"server_name": "Den"}))).await.unwrap();
        assert!(s.require_two_step, "other changes leave it alone");
    }

    fn encoder(name: &str, accel: HwAccel) -> pley_transcoder_api::EncoderInfo {
        pley_transcoder_api::EncoderInfo {
            name: name.into(),
            codec: if name.starts_with("hevc") { VideoCodec::Hevc } else { VideoCodec::H264 },
            hardware: accel != HwAccel::None,
            accel,
            main10: false,
            low_power: false,
        }
    }

    /// An FFmpeg on a computer with an Intel graphics chip, on Windows.
    fn intel() -> Capabilities {
        Capabilities {
            ffmpeg_available: true,
            encoders: vec![encoder("h264_qsv", HwAccel::Qsv), encoder("libx264", HwAccel::None)],
            hwaccels: ["cuda", "qsv", "d3d11va", "vulkan"].map(String::from).to_vec(),
            tonemappers: vec![Tonemapper::Opencl, Tonemapper::Zscale],
            ..Default::default()
        }
    }

    #[test]
    fn a_hardware_acceleration_needs_its_encoder_and_system() {
        let caps = intel();
        let problem = |accel, os| hwaccel_problem(accel, os, Some(&caps));
        for fine in [HwAccel::Auto, HwAccel::None, HwAccel::Qsv] {
            assert_eq!(problem(fine, Some("windows")), None, "{fine:?}");
        }
        let nvenc = problem(HwAccel::Nvenc, Some("windows")).unwrap();
        assert!(nvenc.contains("NVIDIA graphics card"), "{nvenc}");
        assert!(problem(HwAccel::Amf, Some("windows")).unwrap().contains("AMD graphics card"));
        assert_eq!(problem(HwAccel::Vaapi, Some("windows")).unwrap(), "VAAPI works on Linux only");
        assert_eq!(problem(HwAccel::Videotoolbox, Some("linux")).unwrap(), "VideoToolbox works on macOS only");
        assert_eq!(problem(HwAccel::Qsv, Some("macos")).unwrap(), "Intel Quick Sync doesn't work on macOS");
        // A transcoder elsewhere (its own system unknown) is judged by its encoders only.
        assert!(problem(HwAccel::Vaapi, None).unwrap().contains("Intel or AMD graphics chip"));
        assert!(problem(HwAccel::Vulkan, None).is_some());
        let vulkan = Capabilities { vulkan_encode: true, ..intel() };
        assert_eq!(hwaccel_problem(HwAccel::Vulkan, None, Some(&vulkan)), None);
        // HEVC alone doesn't make H.264 transcodes: they'd be encoded in software.
        let hevc = Capabilities { encoders: vec![encoder("hevc_nvenc", HwAccel::Nvenc)], ..intel() };
        assert!(hwaccel_problem(HwAccel::Nvenc, Some("linux"), Some(&hevc)).is_some());
        // Without the transcoder or its FFmpeg, nothing is known but the system.
        assert_eq!(hwaccel_problem(HwAccel::Nvenc, Some("windows"), None), None);
        let missing = Capabilities { ffmpeg_available: false, ..Default::default() };
        assert_eq!(hwaccel_problem(HwAccel::Nvenc, Some("windows"), Some(&missing)), None);
        assert!(hwaccel_problem(HwAccel::Vaapi, Some("windows"), None).is_some());
    }

    #[test]
    fn a_tone_mapper_needs_to_work() {
        let caps = intel();
        for fine in [TonemapMode::Auto, TonemapMode::None, TonemapMode::Opencl, TonemapMode::Zscale] {
            assert_eq!(tonemap_problem(fine, Some(&caps)), None, "{fine:?}");
        }
        assert!(tonemap_problem(TonemapMode::Libplacebo, Some(&caps)).unwrap().contains("Vulkan"));
        assert_eq!(tonemap_problem(TonemapMode::Libplacebo, None), None);
        let none = Capabilities { tonemappers: vec![], ..intel() };
        assert!(tonemap_problem(TonemapMode::Zscale, Some(&none)).unwrap().contains("zimg"));
    }

    #[test]
    fn memory_needs_a_transcoder_that_has_it() {
        use pley_transcoder_api::MemoryInfo;
        assert_eq!(memory_problem(None), None, "no transcoder: nobody can tell");
        let info = |memory| TranscoderInfo { memory, ..Default::default() };
        let linux = MemoryInfo { available: true, path: Some("/dev/shm".into()), budget_bytes: 1 << 32, reason: None };
        assert_eq!(memory_problem(Some(&info(Some(linux)))), None);
        let windows = MemoryInfo { available: false, reason: Some("works on Linux only".into()), ..Default::default() };
        assert_eq!(memory_problem(Some(&info(Some(windows)))).as_deref(), Some("works on Linux only"));
        assert_eq!(memory_problem(Some(&info(None))).as_deref(), Some("needs a newer transcoder"));
        // On by default, and in every job's request.
        assert!(Settings::default().transcode_in_memory && Settings::default().transcode_options().in_memory);
        let off = Settings { transcode_in_memory: false, ..Settings::default() };
        assert!(!off.transcode_options().in_memory);
    }

    /// Dolby Vision profile 7's full enhancement layer: transcoded from its
    /// base layer unless the setting asks for its metadata, which every
    /// job's request then says. Settings saved before it read as the base
    /// layer.
    #[test]
    fn transcodes_are_told_what_to_make_of_a_full_enhancement_layer() {
        assert_eq!(Settings::default().transcode_options().dolby_vision_fel, DolbyVisionFel::BaseLayer);
        let metadata = Settings { dolby_vision_fel: DolbyVisionFel::Metadata, ..Settings::default() };
        assert_eq!(metadata.transcode_options().dolby_vision_fel, DolbyVisionFel::Metadata);
        let json = serde_json::to_value(&metadata).unwrap();
        assert_eq!(json["dolby_vision_fel"], "metadata");
        let before: Settings = serde_json::from_str(r#"{"server_name":"Home"}"#).unwrap();
        assert_eq!(before.dolby_vision_fel, DolbyVisionFel::BaseLayer);
        let patch: SettingsPatch = serde_json::from_str(r#"{"dolby_vision_fel":"metadata"}"#).unwrap();
        assert_eq!(patch.dolby_vision_fel, Some(DolbyVisionFel::Metadata));
        assert!(serde_json::from_str::<SettingsPatch>(r#"{"dolby_vision_fel":"enhanced"}"#).is_err());
        // The full layer, experimental: off unless turned on, and passed on to every job.
        assert!(!before.dolby_vision_fel_full && !Settings::default().transcode_options().dolby_vision_fel_full);
        let full = Settings { dolby_vision_fel_full: true, ..Settings::default() };
        assert!(full.transcode_options().dolby_vision_fel_full);
        let patch: SettingsPatch = serde_json::from_str(r#"{"dolby_vision_fel_full":true}"#).unwrap();
        assert_eq!(patch.dolby_vision_fel_full, Some(true));
    }

    #[test]
    fn decoding_on_a_gpu_goes_with_an_encoder_or_a_pipeline() {
        assert!(decodes_on_gpu(&intel()), "Quick Sync decodes through D3D11");
        let software = Capabilities { encoders: vec![encoder("libx264", HwAccel::None)], ..intel() };
        assert!(!decodes_on_gpu(&software), "FFmpeg lists cuda, but there's no NVIDIA card");
        let vulkan = Capabilities { vulkan_decode: vec!["hevc10".into()], ..software.clone() };
        assert!(decodes_on_gpu(&vulkan));
        let mac = Capabilities { hwaccels: vec!["videotoolbox".into()], ..software };
        assert!(decodes_on_gpu(&mac));
    }

    #[test]
    fn each_hardware_acceleration_says_what_it_decodes_and_leaves_to_software() {
        let hevc = |name: &str, accel, main10| EncoderInfo { codec: VideoCodec::Hevc, main10, ..encoder(name, accel) };
        // Quick Sync encodes HEVC, but not in 10 bits; x265 does.
        let mut intel = intel();
        intel.encoders.extend([hevc("hevc_qsv", HwAccel::Qsv, false), hevc("libx265", HwAccel::None, true)]);
        let qsv = accel_use(&intel, HwAccel::Qsv);
        assert!(qsv.hardware_decoding, "Quick Sync decodes through D3D11");
        assert_eq!(qsv.software, ["hdr"]);
        assert_eq!(accel_use(&intel, HwAccel::Auto), qsv);
        let software = accel_use(&intel, HwAccel::None);
        assert!(!software.hardware_decoding, "x264 gets no decoder: FFmpeg lists cuda, but there's no NVIDIA card");
        assert_eq!(software.software, ["h264", "hevc", "hdr"]);
        // A choice whose encoder isn't here is software encoding.
        assert_eq!(accel_use(&intel, HwAccel::Nvenc), software);
        assert_eq!(accel_use(&intel, HwAccel::Vulkan), software);

        // Software encoding on an NVIDIA computer still decodes with CUDA.
        let nvidia = Capabilities {
            encoders: vec![encoder("h264_nvenc", HwAccel::Nvenc), encoder("libx264", HwAccel::None)],
            hwaccels: vec!["cuda".into()],
            ..intel.clone()
        };
        assert!(accel_use(&nvidia, HwAccel::None).hardware_decoding);
        assert_eq!(accel_use(&nvidia, HwAccel::Nvenc).software, Vec::<&str>::new(), "no HEVC encoder at all");
        // Vulkan's decoder feeds libplacebo whatever encodes.
        let vulkan = Capabilities { vulkan_decode: vec!["hevc10".into()], ..intel };
        assert!(ACCELS.iter().all(|accel| accel_use(&vulkan, *accel).hardware_decoding));
        assert!(ACCELS.iter().all(|accel| accel_use(&vulkan, *accel).tonemappers.is_none()));
    }

    #[test]
    fn vaapi_takes_the_tone_mappers_that_reached_its_encoder() {
        let mut linux = Capabilities {
            encoders: vec![encoder("h264_vaapi", HwAccel::Vaapi), encoder("libx264", HwAccel::None)],
            hwaccels: vec!["vaapi".into()],
            tonemappers: vec![Tonemapper::Libplacebo, Tonemapper::Opencl, Tonemapper::Zscale],
            vaapi_tonemappers: vec![Tonemapper::Libplacebo, Tonemapper::Zscale],
            ..Capabilities::default()
        };
        let vaapi = Some(vec![Tonemapper::Libplacebo, Tonemapper::Zscale]);
        assert_eq!(accel_use(&linux, HwAccel::Vaapi).tonemappers, vaapi);
        assert_eq!(accel_use(&linux, HwAccel::Auto).tonemappers, vaapi);
        // x264 takes every tone mapper.
        assert_eq!(accel_use(&linux, HwAccel::None).tonemappers, None);
        // A transcoder from before the test: nothing known.
        linux.vaapi_tonemappers.clear();
        assert_eq!(accel_use(&linux, HwAccel::Vaapi).tonemappers, None);
    }

    #[test]
    fn vulkan_tone_maps_with_libplacebo_and_decodes_on_the_gpu() {
        let fit = |before: &Settings, json| {
            let mut patch = patch(json);
            fit_to_accel(before, &mut patch).map(|()| (patch.tonemapping, patch.hardware_decoding))
        };
        let saved = Settings { tonemapping: TonemapMode::Zscale, hardware_decoding: false, ..Settings::default() };
        // Chosen: what doesn't go with it falls back.
        assert_eq!(fit(&saved, serde_json::json!({"hwaccel": "vulkan"})), Ok((Some(TonemapMode::Auto), Some(true))));
        // A form sent whole, with them as saved, too.
        let whole = serde_json::json!({"hwaccel": "vulkan", "tonemapping": "zscale", "hardware_decoding": false});
        assert_eq!(fit(&saved, whole), Ok((Some(TonemapMode::Auto), Some(true))));
        // Asked for with it: refused.
        let refused = fit(&saved, serde_json::json!({"hwaccel": "vulkan", "tonemapping": "opencl"})).unwrap_err();
        assert!(refused.contains("libplacebo or Automatic"), "{refused}");
        let vulkan = Settings { hwaccel: HwAccel::Vulkan, ..Settings::default() };
        assert!(fit(&vulkan, serde_json::json!({"tonemapping": "none"})).is_err());
        assert!(fit(&vulkan, serde_json::json!({"hardware_decoding": false})).unwrap_err().contains("stays on"));
        assert_eq!(
            fit(&vulkan, serde_json::json!({"tonemapping": "libplacebo"})),
            Ok((Some(TonemapMode::Libplacebo), None))
        );
        // Away from Vulkan, anything goes.
        let away = serde_json::json!({"hwaccel": "nvenc", "tonemapping": "zscale", "hardware_decoding": false});
        assert_eq!(fit(&vulkan, away), Ok((Some(TonemapMode::Zscale), Some(false))));
        // A combination saved before stays.
        let stale = Settings { hwaccel: HwAccel::Vulkan, ..saved };
        let whole = serde_json::json!({"hwaccel": "vulkan", "tonemapping": "zscale", "server_name": "Den"});
        assert_eq!(fit(&stale, whole), Ok((Some(TonemapMode::Zscale), None)));
        // The others take every tone mapper.
        for accel in ACCELS.into_iter().filter(|a| *a != HwAccel::Vulkan) {
            for mode in [TonemapMode::Opencl, TonemapMode::Zscale, TonemapMode::None] {
                assert!(tonemap_goes_with(accel, mode) && decoding_goes_with(accel, false), "{accel:?} {mode:?}");
            }
        }
    }

    #[tokio::test]
    async fn the_opensubtitles_password_is_kept_but_never_shown() {
        let (_dir, db, store) = store().await;
        let set = |json: serde_json::Value| store.update(&db, patch(json));
        let s = set(serde_json::json!({
            "opensubtitles_api_key": " key ",
            "opensubtitles_username": " ana ",
            "opensubtitles_password": "s3cret",
        }))
        .await
        .unwrap();
        assert_eq!((s.opensubtitles_api_key.as_str(), s.opensubtitles_username.as_str()), ("key", "ana"));
        assert_eq!(s.opensubtitles_password, "s3cret");
        let shown = serde_json::to_value(s.for_admin()).unwrap();
        assert_eq!(shown["opensubtitles_password_set"], serde_json::json!(true));
        assert_eq!(shown["opensubtitles_username"], serde_json::json!("ana"));
        assert!(!shown.to_string().contains("s3cret"), "{shown}");
        assert!(shown.get("opensubtitles_password").is_none());
        // Saved, and kept by changes that leave it out.
        let s = set(serde_json::json!({"server_name": "Den"})).await.unwrap();
        assert_eq!(s.opensubtitles_password, "s3cret");
        assert_eq!(SettingsStore::load(&db).await.unwrap().get().opensubtitles_password, "s3cret");
        // Another account, or none, forgets it.
        let s = set(serde_json::json!({"opensubtitles_username": "bo"})).await.unwrap();
        assert_eq!(s.opensubtitles_password, "");
        let s = set(serde_json::json!({"opensubtitles_password": "pw"})).await.unwrap();
        assert_eq!(s.opensubtitles_password, "pw");
        let s = set(serde_json::json!({"opensubtitles_username": ""})).await.unwrap();
        assert!(!s.for_admin().opensubtitles_password_set);
    }

    #[tokio::test]
    async fn radarr_and_sonarr_keep_their_keys_to_themselves() {
        let (_dir, db, store) = store().await;
        let set = |json: serde_json::Value| store.update(&db, patch(json));
        let s = set(serde_json::json!({
            "radarr_url": " http://192.168.1.5:7878/radarr/ ",
            "radarr_api_key": " r4dd ",
            "sonarr_url": "HTTPS://Box.lan:8989",
            "sonarr_api_key": "s0nn",
            "radarr_minimum_availability": "in_cinemas",
            "sonarr_quality_profile": 4,
        }))
        .await
        .unwrap();
        assert_eq!(s.radarr_url, "http://192.168.1.5:7878/radarr");
        assert_eq!(s.sonarr_url, "https://box.lan:8989");
        assert_eq!((s.radarr_api_key.as_str(), s.sonarr_api_key.as_str()), ("r4dd", "s0nn"));
        assert_eq!(s.radarr_minimum_availability, MinimumAvailability::InCinemas);
        let shown = serde_json::to_value(s.for_admin()).unwrap();
        assert_eq!(shown["radarr_api_key_set"], serde_json::json!(true));
        assert_eq!(shown["sonarr_api_key_set"], serde_json::json!(true));
        assert_eq!(shown["radarr_minimum_availability"], serde_json::json!("in_cinemas"));
        for secret in ["r4dd", "s0nn"] {
            assert!(!shown.to_string().contains(secret), "{shown}");
        }
        // Saved, and kept by changes that leave them out, for the address
        // they were saved for only.
        let s = set(serde_json::json!({"radarr_url": "http://192.168.1.5:7878/radarr", "radarr_search": false}))
            .await
            .unwrap();
        assert_eq!((s.radarr_api_key.as_str(), s.radarr_search), ("r4dd", false));
        assert_eq!(SettingsStore::load(&db).await.unwrap().get().sonarr_api_key, "s0nn");
        let moved = set(serde_json::json!({"radarr_url": "http://evil.example:7878"})).await;
        assert!(moved.is_err(), "the saved key would go elsewhere");
        assert_eq!(store.get().radarr_url, "http://192.168.1.5:7878/radarr", "nothing changed");
        let s = set(serde_json::json!({"radarr_url": "http://localhost:7878", "radarr_api_key": "n3w"})).await.unwrap();
        assert_eq!((s.radarr_url.as_str(), s.radarr_api_key.as_str()), ("http://localhost:7878", "n3w"));
        let s = set(serde_json::json!({"sonarr_url": ""})).await.unwrap();
        assert!(s.sonarr_url.is_empty() && !s.for_admin().sonarr_api_key_set, "no address keeps no key");
        let s = set(serde_json::json!({"radarr_api_key": ""})).await.unwrap();
        assert!(!s.for_admin().radarr_api_key_set);
        let s = set(serde_json::json!({"radarr_url": "http://box.lan:7878"})).await.unwrap();
        assert_eq!(s.radarr_url, "http://box.lan:7878", "no key saved: none goes anywhere");

        for bad in [
            serde_json::json!({"radarr_url": "localhost:7878"}),
            serde_json::json!({"sonarr_url": "ftp://box.lan"}),
            serde_json::json!({"radarr_url": "http://user:pw@box.lan:7878"}),
            serde_json::json!({"radarr_url": "http://169.254.169.254/latest"}),
            serde_json::json!({"sonarr_url": "http://[fe80::1]:8989"}),
            serde_json::json!({"sonarr_url": "http://[::ffff:169.254.1.1]:8989"}),
            serde_json::json!({"request_limit_movies": 1001}),
            serde_json::json!({"radarr_quality_profile": -1}),
        ] {
            assert!(set(bad.clone()).await.is_err(), "{bad}");
        }
        // An availability Radarr no longer has.
        let tba = serde_json::json!({"radarr_minimum_availability": "tba"});
        assert!(serde_json::from_value::<SettingsPatch>(tba).is_err());
        // Requests find titles on TMDB.
        assert!(set(serde_json::json!({"requests": true})).await.is_err());
        let s =
            set(serde_json::json!({"requests": true, "tmdb_api_key": "k", "request_limit_seasons": 10})).await.unwrap();
        assert!(s.requests && s.request_limit_seasons == 10);
        assert!(set(serde_json::json!({"tmdb_api_key": ""})).await.is_err(), "the key stays while requests are on");
    }

    #[tokio::test]
    async fn old_single_sign_on_keys_are_ignored() {
        let (_dir, db, _) = store().await;
        // A row as single sign-on left it (upgrading removes these keys too).
        let row = serde_json::json!({
            "server_name": "Attic",
            "public_url": "https://pley.example.com",
            "sso_enabled": true,
            "sso_issuer": "https://idp.example.com",
            "sso_client_secret": "s3cret",
        });
        sqlx::query("INSERT INTO settings (key, value) VALUES ('server', ?)")
            .bind(row.to_string())
            .execute(&db.write)
            .await
            .unwrap();
        let store = SettingsStore::load(&db).await.unwrap();
        assert_eq!(store.get().server_name, "Attic", "the rest of the row loads");

        // A web app from before still sends them.
        let old = serde_json::json!({
            "server_name": "Den",
            "public_url": "https://pley.example.com",
            "sso_issuer": "https://idp.example.com",
            "sso_client_secret": "s3cret",
        });
        let s = store.update(&db, patch(old)).await.unwrap();
        assert_eq!(s.server_name, "Den");
        let saved: String =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'server'").fetch_one(&db.read).await.unwrap();
        for gone in ["public_url", "sso_", "s3cret"] {
            assert!(!saved.contains(gone), "{gone} is still saved: {saved}");
        }
        assert_eq!(*SettingsStore::load(&db).await.unwrap().get(), *s);
    }
}
