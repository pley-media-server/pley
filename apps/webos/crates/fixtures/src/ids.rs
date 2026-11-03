//! What tests, the simulator and screenshots can count on in the fixture
//! server's catalogue, which is the same on every run (the Android app's
//! fixture server's `FixtureIds`, plus Live TV).
//!
//! Three libraries: [`MOVIES`] (1,500 movies), [`SHOWS`] (300 shows with
//! their seasons and episodes) and [`MUSIC`] (50 artists, 200 albums). Most
//! titles are made up in bulk with realistic lengths, and their files stream
//! one of the two sample clips. The items named here play exactly what their
//! files say: 20 s clips whose durations, streams and chapters are the real
//! ones.

/// The client id of Pley for LG TV, the only app the fixture server signs in.
pub const CLIENT_ID: &str = "https://pley-media-server.github.io/pley/oauth/webos.json";

/// The fixture server's id, as GET /api/server and every token answer give it
/// (40 hexadecimal characters, like a real server's `server-id`).
pub const SERVER_ID: &str = "f1c7e0f1c7e0f1c7e0f1c7e0f1c7e0f1c7e0f1c7";

/// The fixture server's name and version, as GET /api/server gives them.
pub const SERVER_NAME: &str = "Fixture Server";
pub const SERVER_VERSION: &str = "0.1.0-fixtures";

pub const MOVIES: i64 = 1;
pub const SHOWS: i64 = 2;
pub const MUSIC: i64 = 3;

/// 1080p24 H.264 and stereo AAC in MP4, played directly by most devices.
pub const SAMPLE_1080P: i64 = 1;
pub const SAMPLE_1080P_FILE: i64 = 1_000_001;

/// 2160p23.976 HEVC Main 10 HDR10, AC-3 5.1, an English SRT track and three
/// chapters in MKV.
pub const SAMPLE_HDR10: i64 = 2;
pub const SAMPLE_HDR10_FILE: i64 = 1_000_002;

/// The HDR10 sample's embedded English subtitles: partial
/// (`X-Pley-Complete: 0`) the first time they are asked for, whole after.
pub const SAMPLE_HDR10_SUBTITLE: i64 = 2;

/// Its French subtitle file beside the media.
pub const SAMPLE_HDR10_EXTERNAL_SUBTITLE: i64 = 1000;

/// Dolby Vision profile 5 (no compatible base layer); streams the HDR10 clip.
pub const DOLBY_VISION_P5: i64 = 3;
/// Dolby Vision profile 7 (dual layer, HDR10 base layer).
pub const DOLBY_VISION_P7: i64 = 4;
/// Dolby Vision profile 8.1 (HDR10 base layer).
pub const DOLBY_VISION_P81: i64 = 5;
/// HDR10+ dynamic metadata.
pub const HDR10_PLUS: i64 = 6;

/// A movie with a French E-AC-3 track, an English SRT and a French PGS track
/// (`.mks` answered), in progress for Alex.
pub const PGS_MOVIE: i64 = 8;
pub const PGS_MOVIE_FILE: i64 = 1_000_008;
/// Its PGS track's index.
pub const PGS_MOVIE_SUBTITLE: i64 = 4;

/// "Fixture Show": two seasons of three 20 s episodes with an intro and
/// credits each.
pub const SHOW: i64 = 10_001;
pub const SHOW_SEASON_1: i64 = 11_001;
pub const SHOW_SEASON_2: i64 = 11_002;
pub const SHOW_S1E1: i64 = 20_001;
pub const SHOW_S1E2: i64 = 20_002;
pub const SHOW_S1E3: i64 = 20_003;
pub const SHOW_S2E1: i64 = 20_004;

/// Where the fixture show's episodes' intro starts and ends, in ms.
pub const INTRO_START_MS: i64 = 2_000;
pub const INTRO_END_MS: i64 = 8_000;
/// Where their credits start; they run to the end.
pub const CREDITS_START_MS: i64 = 16_000;

pub const ARTIST: i64 = 60_001;
/// The first album: its tracks are 20 s, as their file.
pub const ALBUM: i64 = 61_001;
pub const TRACK: i64 = 62_001;

/// A smart collection of the movies library: its comedies of the 1990s.
pub const SMART_COLLECTION: i64 = 1;
pub const COLLECTION: i64 = 2;

/// Alex's playlists: movies, tracks, and a smart one (the library's
/// unwatched comedies).
pub const PLAYLIST_VIDEO: i64 = 1;
pub const PLAYLIST_AUDIO: i64 = 2;
pub const PLAYLIST_SMART: i64 = 3;

/// The household. Signing in acts as Alex unless told otherwise.
pub const ALEX: i64 = 1;
/// Has the PIN [`PIN`]; Live TV isn't open to Sam.
pub const SAM: i64 = 2;
/// Needs a password: switching to Robin means signing in again in the browser.
pub const ROBIN: i64 = 3;
/// Sam's PIN.
pub const PIN: &str = "1234";

/// Live TV: a news channel with a guide, on a source without a connection
/// limit.
pub const LIVE_CHANNEL: i64 = 1;
/// The channel another of Alex's devices watches at start, on a source that
/// allows one connection at once.
pub const LIVE_BUSY_CHANNEL: i64 = 25;
/// Another channel of that source: watching it answers 409 until the busy
/// channel is taken over (`take_over`).
pub const LIVE_CONFLICT_CHANNEL: i64 = 26;
/// A channel without a guide.
pub const LIVE_NO_GUIDE_CHANNEL: i64 = 41;
/// How many channels there are.
pub const LIVE_CHANNELS: i64 = 42;
