//! Remembered audio and subtitle choices, and each user's defaults.
//!
//! When a user picks an audio track or turns subtitles on or off, the choice
//! is stored for the show (episodes) or the movie, as track properties
//! (language, title, forced). Every episode then starts with the matching
//! tracks of its own file, so subtitles don't have to be re-enabled episode
//! after episode. Titles without a choice start with the user's default
//! languages ([`Defaults`]).

use pley_transcoder_api::{MediaStream, StreamKind};
use serde::{Deserialize, Serialize};

use crate::catalog::ItemRow;
use crate::db::Db;
use crate::util::now;

#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct StreamPreference {
    pub audio_language: Option<String>,
    pub audio_title: Option<String>,
    pub subtitles: bool,
    pub subtitle_language: Option<String>,
    pub subtitle_title: Option<String>,
    pub subtitle_forced: bool,
}

/// When subtitles come on by themselves, for titles without a choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleMode {
    /// Never.
    Off,
    /// Forced subtitles (foreign dialogue) in the language of the audio.
    #[default]
    Forced,
    /// Full subtitles in the subtitle language when the audio is in another.
    Foreign,
    /// Always, in the subtitle language.
    Always,
}

impl SubtitleMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SubtitleMode::Off => "off",
            SubtitleMode::Forced => "forced",
            SubtitleMode::Foreign => "foreign",
            SubtitleMode::Always => "always",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "off" => SubtitleMode::Off,
            "foreign" => SubtitleMode::Foreign,
            "always" => SubtitleMode::Always,
            _ => SubtitleMode::Forced,
        }
    }
}

/// A user's default tracks (account settings).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Defaults {
    /// Preferred audio language; `None`: the file's default track.
    pub audio_language: Option<String>,
    /// Language subtitles are read in; `None`: the audio language.
    pub subtitle_language: Option<String>,
    pub subtitle_mode: SubtitleMode,
}

pub async fn defaults(db: &Db, user_id: i64) -> sqlx::Result<Defaults> {
    let row: Option<(Option<String>, Option<String>, String)> =
        sqlx::query_as("SELECT audio_language, subtitle_language, subtitle_mode FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&db.read)
            .await?;
    Ok(row
        .map(|(audio_language, subtitle_language, mode)| Defaults {
            audio_language,
            subtitle_language,
            subtitle_mode: SubtitleMode::parse(&mode),
        })
        .unwrap_or_default())
}

/// Whether `code` looks like an ISO 639 language code (`en`, `eng`).
pub fn valid_language(code: &str) -> bool {
    (2..=3).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_lowercase())
}

/// Tracks to start a file with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Selection {
    pub audio: Option<i64>,
    pub subtitle: Option<i64>,
}

/// The item a preference is stored on: the show for episodes, else the item.
pub fn scope(row: &ItemRow) -> i64 {
    match row.kind.as_str() {
        "episode" => row.grandparent_id.unwrap_or(row.id),
        _ => row.id,
    }
}

pub async fn load(db: &Db, user_id: i64, scope_id: i64) -> sqlx::Result<Option<StreamPreference>> {
    sqlx::query_as(
        "SELECT audio_language, audio_title, subtitles, subtitle_language, subtitle_title, subtitle_forced
         FROM stream_preferences WHERE user_id = ? AND item_id = ?",
    )
    .bind(user_id)
    .bind(scope_id)
    .fetch_optional(&db.read)
    .await
}

pub async fn save(db: &Db, user_id: i64, scope_id: i64, pref: &StreamPreference) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO stream_preferences (user_id, item_id, audio_language, audio_title, subtitles, subtitle_language,
                                         subtitle_title, subtitle_forced, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id, item_id) DO UPDATE SET
            audio_language = excluded.audio_language, audio_title = excluded.audio_title,
            subtitles = excluded.subtitles, subtitle_language = excluded.subtitle_language,
            subtitle_title = excluded.subtitle_title, subtitle_forced = excluded.subtitle_forced,
            updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(scope_id)
    .bind(&pref.audio_language)
    .bind(&pref.audio_title)
    .bind(pref.subtitles)
    .bind(&pref.subtitle_language)
    .bind(&pref.subtitle_title)
    .bind(pref.subtitle_forced)
    .bind(now())
    .execute(&db.write)
    .await?;
    Ok(())
}

/// Describe the chosen tracks of a file as a preference.
pub fn from_selection(streams: &[MediaStream], audio: Option<i64>, subtitle: Option<i64>) -> StreamPreference {
    let find = |index: Option<i64>, kind: StreamKind| {
        index.and_then(|i| streams.iter().find(|s| s.index == i && s.kind == kind))
    };
    let audio = find(audio, StreamKind::Audio);
    let subtitle = find(subtitle, StreamKind::Subtitle);
    StreamPreference {
        audio_language: audio.and_then(|a| a.language.clone()),
        audio_title: audio.and_then(|a| a.title.clone()),
        subtitles: subtitle.is_some(),
        subtitle_language: subtitle.and_then(|s| s.language.clone()),
        subtitle_title: subtitle.and_then(|s| s.title.clone()),
        subtitle_forced: subtitle.is_some_and(is_forced),
    }
}

/// What a player does with audio, for choosing among the tracks of a
/// language: the play request's capabilities (`playback::Caps`). Without
/// them (an item's details) the file's default and order choose, as the
/// device isn't known.
pub trait AudioOut {
    /// Plays this codec as it is: decoded, or passed through to a receiver.
    fn plays(&self, codec: &str) -> bool;
    /// Sends this codec as it is to a receiver, which keeps its Atmos or DTS:X.
    fn passes_through(&self, codec: &str) -> bool;
    /// Switches a file's audio tracks itself; for any other player, a track
    /// but the file's default makes the file stream (HLS).
    fn switches_tracks(&self) -> bool;
    /// The most channels its output takes, when it says.
    fn channels(&self) -> Option<i64>;
    /// Decodes audio at this sample rate (Hz); a track above the player's
    /// highest is converted, which resamples it. A rate not known is taken.
    fn takes_rate(&self, _rate: Option<i64>) -> bool {
        true
    }
    /// Copies this codec as it is into a stream (HLS), for a track it can't
    /// have with the file as it is; the others are converted.
    fn stream_copies(&self, codec: &str) -> bool {
        self.plays(codec)
    }
    /// The file streams whatever its audio (its video is converted), so no
    /// track plays with the file as it is.
    fn streams(&self) -> bool {
        false
    }
}

/// A player whose file streams anyway (see [`AudioOut::streams`]): of a
/// language's tracks, one its stream copies beats one the file would have
/// played as it is, which the stream converts.
pub struct Streamed<'a>(pub &'a dyn AudioOut);

impl AudioOut for Streamed<'_> {
    fn plays(&self, codec: &str) -> bool {
        self.0.plays(codec)
    }
    fn passes_through(&self, codec: &str) -> bool {
        self.0.passes_through(codec)
    }
    fn switches_tracks(&self) -> bool {
        self.0.switches_tracks()
    }
    fn channels(&self) -> Option<i64> {
        self.0.channels()
    }
    fn takes_rate(&self, rate: Option<i64>) -> bool {
        self.0.takes_rate(rate)
    }
    fn stream_copies(&self, codec: &str) -> bool {
        self.0.stream_copies(codec)
    }
    fn streams(&self) -> bool {
        true
    }
}

/// How a track sounds on a player, better greater: as many channels as its
/// output takes (what a conversion makes of the others), mixed for no more
/// than that (a stereo mix beats a 5.1 one folded down), lossless, object
/// audio (Atmos, DTS:X) sent on to a receiver, then the bitrate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Sound {
    channels: i64,
    native: bool,
    lossless: bool,
    objects: bool,
    bitrate: i64,
}

/// Whether `out` gets `s` as it is, and how it sounds there. A track it
/// can't play, or one that a stream it needs (another track than the
/// default, for a player that doesn't switch them) would convert, is
/// converted: E-AC-3, AC-3 or AAC, 5.1 at most (stereo for version 1
/// players, which name no channels), so only its channels and its mix
/// count. A version 1 player's stream copies only AAC and MP3: its AC-3 5.1
/// came out as stereo AAC where the file's own stereo AAC would have been
/// copied.
fn fit(s: &MediaStream, out: &dyn AudioOut, default_audio: Option<&MediaStream>) -> (bool, Sound) {
    let channels = s.channels.unwrap_or(2);
    if !out.plays(&s.codec)
        || !out.takes_rate(s.sample_rate)
        || !(as_is(s, out, default_audio) || out.stream_copies(&s.codec))
    {
        let most = out.channels().unwrap_or(2).min(6);
        let sound = Sound { channels: channels.min(most), native: channels <= most, ..Sound::default() };
        return (false, sound);
    }
    let lossless = is_lossless(s);
    // Atmos and DTS:X, as FFmpeg names the profile ("Dolby TrueHD + Dolby Atmos"), or the title does.
    let objects = [s.profile.as_deref(), s.title.as_deref()].into_iter().flatten().any(|text| {
        let text = text.to_lowercase();
        text.contains("atmos") || text.contains("dts:x")
    });
    // Passed through, the receiver decodes it: the player's own channels
    // (what it outputs as PCM, 2.0 behind an optical or ARC receiver) don't
    // limit it. A 2.0 player passing AC-3 through had its stereo mix chosen
    // over the AC-3 5.1 the receiver would have played.
    let passed = out.passes_through(&s.codec);
    let sound = Sound {
        channels: if passed { channels } else { out.channels().map_or(channels, |most| channels.min(most)) },
        native: passed || out.channels().is_none_or(|most| channels <= most),
        lossless,
        objects: objects && passed,
        bitrate: s.bitrate.unwrap_or(0),
    };
    (true, sound)
}

/// DTS-HD Master Audio is lossless; its core and High Resolution Audio aren't.
fn is_lossless(s: &MediaStream) -> bool {
    let master_audio = s.profile.as_deref().is_some_and(|p| p.split(|c: char| !c.is_alphanumeric()).any(|w| w == "MA"));
    matches!(s.codec.as_str(), "truehd" | "mlp" | "flac" | "alac")
        || s.codec.starts_with("pcm_")
        || (s.codec == "dts" && master_audio)
}

/// Of tracks that come out alike, the one cheapest to convert, smaller
/// greater: lossy before lossless, then the fewest bits to decode. A track
/// that plays isn't converted, and costs nothing.
fn conversion(s: &MediaStream, fits: Option<(bool, Sound)>) -> Option<std::cmp::Reverse<(bool, i64)>> {
    fits.filter(|(plays, _)| !plays).map(|_| std::cmp::Reverse((is_lossless(s), s.bitrate.unwrap_or(0))))
}

/// Pick the tracks of `streams` that best match `pref`, or without one the
/// user's `defaults` (see [`by_default`]). With the player's `out`, of the
/// tracks of the language chosen the one it plays best: as it is rather
/// than converted, then (unless a choice names its title) with the file as
/// it is rather than streamed, then as it sounds there ([`Sound`]).
pub fn resolve(
    pref: Option<&StreamPreference>,
    defaults: &Defaults,
    streams: &[MediaStream],
    out: Option<&dyn AudioOut>,
) -> Selection {
    let audio_streams: Vec<&MediaStream> = streams.iter().filter(|s| s.kind == StreamKind::Audio).collect();
    let subtitle_streams: Vec<&MediaStream> = streams.iter().filter(|s| s.kind == StreamKind::Subtitle).collect();
    let default_audio = audio_streams.iter().copied().find(|s| s.default).or(audio_streams.first().copied());

    let Some(pref) = pref else {
        return by_default(defaults, &audio_streams, &subtitle_streams, default_audio, out);
    };

    let wanted = audio_kind(pref.audio_title.as_deref());
    let audio = match (pref.audio_language.as_deref(), pref.audio_title.as_deref()) {
        (Some(language), _) => {
            let in_language =
                audio_streams.iter().copied().filter(|s| same_language(s.language.as_deref(), Some(language)));
            in_language
                .max_by_key(|s| {
                    let fits = out.map(|out| fit(s, out, default_audio));
                    (
                        audio_kind(s.title.as_deref()) == wanted,
                        fits.is_some_and(|(plays, _)| plays),
                        same_title(s.title.as_deref(), pref.audio_title.as_deref()),
                        out.is_some_and(|out| as_is(s, out, default_audio)),
                        fits.map(|(_, sound)| sound),
                        conversion(s, fits),
                        s.default,
                        // Prefer the earliest track among equals.
                        std::cmp::Reverse(s.index),
                    )
                })
                // Not in this file (an episode without it): as without a choice.
                .or_else(|| automatic_audio(defaults, audio_streams.as_slice(), default_audio, out))
        }
        // A track without a language was chosen: the one with its title, as for subtitles.
        (None, Some(title)) => audio_streams
            .iter()
            .copied()
            .find(|s| s.language.is_none() && same_title(s.title.as_deref(), Some(title)))
            .or_else(|| automatic_audio(defaults, audio_streams.as_slice(), default_audio, out)),
        // Only subtitles were chosen: the audio is chosen as without a choice.
        (None, None) => automatic_audio(defaults, audio_streams.as_slice(), default_audio, out),
    };
    // Only pin the audio when it differs from what the file plays anyway.
    let audio = audio.map(|s| s.index).filter(|&i| default_audio.is_none_or(|d| d.index != i));

    let subtitle = if pref.subtitles {
        subtitle_streams
            .iter()
            .filter(|s| match pref.subtitle_language.as_deref() {
                Some(language) => same_language(s.language.as_deref(), Some(language)),
                // An untagged track was chosen: only a track with the same title will do.
                None => s.language.is_none() && same_title(s.title.as_deref(), pref.subtitle_title.as_deref()),
            })
            .max_by_key(|s| {
                (
                    is_forced(s) == pref.subtitle_forced,
                    same_title(s.title.as_deref(), pref.subtitle_title.as_deref()),
                    is_sdh(s.title.as_deref()) == is_sdh(pref.subtitle_title.as_deref()),
                    // Text is shown by the player; images need a transcode.
                    s.text_based.unwrap_or(true),
                    s.default,
                    std::cmp::Reverse(s.index),
                )
            })
            .map(|s| s.index)
    } else {
        None
    };
    Selection { audio, subtitle }
}

/// Whether `out` plays `s` with the file as it is: it plays the codec at
/// its rate, and the track is the file's default or the player switches
/// tracks itself.
fn as_is(s: &MediaStream, out: &dyn AudioOut, default_audio: Option<&MediaStream>) -> bool {
    !out.streams()
        && out.plays(&s.codec)
        && out.takes_rate(s.sample_rate)
        && (out.switches_tracks() || default_audio.is_some_and(|d| d.index == s.index))
}

/// The audio of a title the user picked none for: the main track in their
/// audio language; `None`, the file's default. With the player's `out`, the
/// main track of that language it plays best, and without their language
/// in the file, of the default track's language: a default the player can't
/// play gives way to one it can (an E-AC-3 track next to a TrueHD one).
fn automatic_audio<'a>(
    defaults: &Defaults,
    audio_streams: &[&'a MediaStream],
    default_audio: Option<&'a MediaStream>,
    out: Option<&dyn AudioOut>,
) -> Option<&'a MediaStream> {
    let present = |language: &str| audio_streams.iter().any(|s| same_language(s.language.as_deref(), Some(language)));
    let language = defaults
        .audio_language
        .as_deref()
        .filter(|l| present(l))
        .or_else(|| out.and(default_audio).and_then(|d| d.language.as_deref()).filter(|l| known_language(l)))?;
    audio_streams.iter().copied().filter(|s| same_language(s.language.as_deref(), Some(language))).max_by_key(|s| {
        let fits = out.map(|out| fit(s, out, default_audio));
        (
            audio_kind(s.title.as_deref()) == AudioKind::Main,
            fits.is_some_and(|(plays, _)| plays),
            out.is_some_and(|out| as_is(s, out, default_audio)),
            fits.map(|(_, sound)| sound),
            conversion(s, fits),
            s.default,
            std::cmp::Reverse(s.index),
        )
    })
}

/// Tracks for a title the user picked none for: the audio of
/// [`automatic_audio`], and subtitles as their mode says. Forced subtitles
/// in the language of the audio translate foreign dialogue the viewer is
/// meant to read: every mode but `off` shows them when nothing else is
/// shown.
fn by_default(
    defaults: &Defaults,
    audio_streams: &[&MediaStream],
    subtitle_streams: &[&MediaStream],
    default_audio: Option<&MediaStream>,
    out: Option<&dyn AudioOut>,
) -> Selection {
    let preferred = automatic_audio(defaults, audio_streams, default_audio, out);
    let playing = preferred.or(default_audio);
    // Only pin the audio when it differs from what the file plays anyway.
    let audio = preferred.filter(|p| default_audio.is_none_or(|d| d.index != p.index)).map(|s| s.index);

    let heard = playing.and_then(|a| a.language.as_deref());
    let read = defaults.subtitle_language.as_deref().or(defaults.audio_language.as_deref());
    let forced = || {
        subtitle_streams
            .iter()
            .filter(|s| is_forced(s) && heard.is_some() && same_language(s.language.as_deref(), heard))
            .max_by_key(|s| s.text_based.unwrap_or(true))
            .map(|s| s.index)
    };
    let full = |language: &str| {
        subtitle_streams
            .iter()
            .filter(|s| same_language(s.language.as_deref(), Some(language)))
            .max_by_key(|s| {
                (
                    !is_forced(s),
                    !is_sdh(s.title.as_deref()),
                    // Text is shown by the player; images need a transcode.
                    s.text_based.unwrap_or(true),
                    s.default,
                    std::cmp::Reverse(s.index),
                )
            })
            .map(|s| s.index)
    };
    let subtitle = match defaults.subtitle_mode {
        SubtitleMode::Off => None,
        SubtitleMode::Forced => forced(),
        // An untagged audio track isn't taken for foreign.
        SubtitleMode::Foreign => match read {
            Some(read) if heard.is_some_and(|h| !same_language(Some(h), Some(read))) => full(read).or_else(forced),
            _ => forced(),
        },
        SubtitleMode::Always => read.and_then(full).or_else(forced),
    };
    Selection { audio, subtitle }
}

/// Language codes as ffprobe reports them (`eng`, sometimes `en`), compared
/// on their ISO 639-2 form.
fn same_language(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => normalize_language(a) == normalize_language(b),
        _ => false,
    }
}

fn normalize_language(code: &str) -> String {
    let code = code.trim().to_ascii_lowercase();
    let code = code.split(['-', '_']).next().unwrap_or_default();
    match code {
        "en" => "eng",
        "fr" | "fra" => "fre",
        "de" | "deu" => "ger",
        "es" => "spa",
        "it" => "ita",
        "pt" => "por",
        "ja" => "jpn",
        "ko" => "kor",
        "zh" | "zho" => "chi",
        "ru" => "rus",
        "nl" | "nld" => "dut",
        "sv" => "swe",
        "no" | "nb" | "nob" => "nor",
        "da" => "dan",
        "fi" => "fin",
        "pl" => "pol",
        "ar" => "ara",
        "he" => "heb",
        "hi" => "hin",
        "tr" => "tur",
        "bg" => "bul",
        "ca" => "cat",
        "cs" | "ces" => "cze",
        "el" | "ell" => "gre",
        "fa" | "fas" => "per",
        "hr" => "hrv",
        "hu" => "hun",
        "id" => "ind",
        "ro" | "ron" => "rum",
        "sk" | "slk" => "slo",
        "sr" => "srp",
        "th" => "tha",
        "uk" => "ukr",
        "vi" => "vie",
        "sl" => "slv",
        "et" => "est",
        "lv" => "lav",
        "lt" => "lit",
        "is" | "isl" => "ice",
        "ms" | "msa" => "may",
        "sq" | "sqi" => "alb",
        "hy" | "hye" => "arm",
        "eu" | "eus" => "baq",
        "my" | "mya" => "bur",
        "ka" | "kat" => "geo",
        "mk" | "mkd" => "mac",
        "mi" | "mri" => "mao",
        "bo" | "bod" => "tib",
        "cy" | "cym" => "wel",
        "af" => "afr",
        "az" => "aze",
        "be" => "bel",
        "bn" => "ben",
        "bs" => "bos",
        "ga" => "gle",
        "gl" => "glg",
        "gu" => "guj",
        "kk" => "kaz",
        "km" => "khm",
        "kn" => "kan",
        "lb" => "ltz",
        "ml" => "mal",
        "mn" => "mon",
        "mr" => "mar",
        "ne" => "nep",
        "pa" => "pan",
        "si" => "sin",
        "sw" => "swa",
        "ta" => "tam",
        "te" => "tel",
        "tl" => "tgl",
        "ur" => "urd",
        "uz" => "uzb",
        other => other,
    }
    .to_owned()
}

fn same_title(a: Option<&str>, b: Option<&str>) -> bool {
    let norm = |t: Option<&str>| t.map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
    norm(a) == norm(b)
}

fn is_sdh(title: Option<&str>) -> bool {
    title.is_some_and(|t| {
        let t = t.to_lowercase();
        t.contains("sdh") || t.contains("hearing") || t.split(|c: char| !c.is_alphanumeric()).any(|w| w == "cc")
    })
}

/// Forced subtitles: flagged so, or only named so (`English (Forced)`,
/// `Signs & Songs`), as many files are.
pub fn is_forced(s: &MediaStream) -> bool {
    s.forced
        || s.title.as_deref().is_some_and(|t| {
            t.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| matches!(w, "forced" | "signs"))
        })
}

/// What an audio track holds, from its title: the film's own sound, or a
/// commentary or a description for blind viewers laid over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioKind {
    Main,
    Commentary,
    Description,
}

fn audio_kind(title: Option<&str>) -> AudioKind {
    let Some(title) = title else { return AudioKind::Main };
    let title = title.to_lowercase();
    if title.contains("commentary") {
        AudioKind::Commentary
    } else if title.contains("descri") || title.split(|c: char| !c.is_alphanumeric()).any(|w| w == "ad") {
        // "Audio Description", "Descriptive Audio", "Described Video", "English AD".
        AudioKind::Description
    } else {
        AudioKind::Main
    }
}

/// A language tag that names a language: not undetermined, several, none,
/// or uncoded (`und`, `mul`, `zxx`, `mis`), whose tracks may be anything.
fn known_language(code: &str) -> bool {
    !matches!(normalize_language(code).as_str(), "und" | "mul" | "zxx" | "mis" | "")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A user who set no defaults.
    const NONE: Defaults =
        Defaults { audio_language: None, subtitle_language: None, subtitle_mode: SubtitleMode::Forced };

    fn audio(index: i64, language: &str, title: Option<&str>, default: bool) -> MediaStream {
        MediaStream {
            index,
            kind: StreamKind::Audio,
            codec: "aac".into(),
            language: Some(language.into()),
            title: title.map(Into::into),
            default,
            ..Default::default()
        }
    }

    fn sub(index: i64, language: Option<&str>, title: Option<&str>, forced: bool, text: bool) -> MediaStream {
        MediaStream {
            index,
            kind: StreamKind::Subtitle,
            codec: if text { "subrip" } else { "hdmv_pgs_subtitle" }.into(),
            language: language.map(Into::into),
            title: title.map(Into::into),
            forced,
            text_based: Some(text),
            ..Default::default()
        }
    }

    /// Episode 1 and episode 2 of a show, with different track layouts.
    fn episode(shift: i64) -> Vec<MediaStream> {
        vec![
            MediaStream { index: 0, kind: StreamKind::Video, codec: "h264".into(), ..Default::default() },
            audio(1, "jpn", None, true),
            audio(2 + shift, "eng", Some("English 5.1"), false),
            audio(3 + shift, "eng", Some("Director's Commentary"), false),
            sub(4 + shift, Some("eng"), Some("Signs & Songs"), true, true),
            sub(5 + shift, Some("eng"), Some("Full"), false, true),
            sub(6 + shift, Some("eng"), Some("SDH"), false, false),
            sub(7 + shift, Some("fre"), None, false, true),
        ]
    }

    #[test]
    fn choices_carry_over_to_the_next_episode() {
        let first = episode(0);
        let pref = from_selection(&first, Some(1), Some(5));
        assert_eq!(pref.subtitle_language.as_deref(), Some("eng"));
        assert!(pref.subtitles && !pref.subtitle_forced);

        // Next episode: tracks moved; the full English subtitles are found again.
        let next = episode(10);
        assert_eq!(resolve(Some(&pref), &NONE, &next, None), Selection { audio: None, subtitle: Some(15) });
    }

    #[test]
    fn forced_and_sdh_choices_are_kept_apart() {
        let pref = from_selection(&episode(0), None, Some(4));
        assert!(pref.subtitle_forced);
        assert_eq!(resolve(Some(&pref), &NONE, &episode(10), None).subtitle, Some(14));

        let pref = from_selection(&episode(0), None, Some(6));
        assert_eq!(resolve(Some(&pref), &NONE, &episode(10), None).subtitle, Some(16));
    }

    #[test]
    fn audio_language_is_followed_but_commentary_avoided() {
        let pref = from_selection(&episode(0), Some(2), None);
        assert_eq!(pref.audio_language.as_deref(), Some("eng"));
        assert_eq!(resolve(Some(&pref), &NONE, &episode(10), None), Selection { audio: Some(12), subtitle: None });
        // Choosing the default track pins nothing.
        let pref = from_selection(&episode(0), Some(1), None);
        assert_eq!(resolve(Some(&pref), &NONE, &episode(10), None).audio, None);
    }

    #[test]
    fn subtitles_off_stays_off_and_missing_languages_are_skipped() {
        let pref = from_selection(&episode(0), None, None);
        assert!(!pref.subtitles);
        assert_eq!(resolve(Some(&pref), &NONE, &episode(10), None).subtitle, None);

        let pref = StreamPreference { subtitles: true, subtitle_language: Some("ger".into()), ..Default::default() };
        assert_eq!(resolve(Some(&pref), &NONE, &episode(0), None).subtitle, None);
    }

    #[test]
    fn language_codes_are_normalized() {
        let pref = StreamPreference { subtitles: true, subtitle_language: Some("en".into()), ..Default::default() };
        assert_eq!(resolve(Some(&pref), &NONE, &episode(0), None).subtitle, Some(5));
        assert!(same_language(Some("fra"), Some("fre")));
        assert!(same_language(Some("en-US"), Some("eng")));
    }

    #[test]
    fn a_chosen_commentary_track_is_followed() {
        let pref = from_selection(&episode(0), Some(3), None);
        assert_eq!(pref.audio_title.as_deref(), Some("Director's Commentary"));
        assert_eq!(resolve(Some(&pref), &NONE, &episode(10), None).audio, Some(13));
    }

    #[test]
    fn missing_audio_languages_keep_the_default_track() {
        let pref = StreamPreference { audio_language: Some("ger".into()), ..Default::default() };
        assert_eq!(resolve(Some(&pref), &NONE, &episode(0), None), Selection::default());
        // Indexes that are not audio (or don't exist) describe nothing.
        let pref = from_selection(&episode(0), Some(4), Some(1));
        assert_eq!(pref, StreamPreference::default());
    }

    #[test]
    fn untagged_subtitles_are_found_again_by_title_only() {
        let streams = |shift: i64| {
            vec![
                audio(1, "eng", None, true),
                sub(2 + shift, None, Some("Commentary"), false, true),
                sub(3 + shift, None, Some("Karaoke"), false, true),
                sub(4 + shift, Some("eng"), Some("Karaoke"), false, true),
            ]
        };
        let pref = from_selection(&streams(0), None, Some(3));
        assert_eq!((pref.subtitle_language.as_deref(), pref.subtitle_title.as_deref()), (None, Some("Karaoke")));
        assert_eq!(
            resolve(Some(&pref), &NONE, &streams(10), None).subtitle,
            Some(13),
            "the untagged track with that title"
        );
        // No untagged track with that title: subtitles stay off.
        let pref = StreamPreference { subtitles: true, subtitle_title: Some("Signs".into()), ..Default::default() };
        assert_eq!(resolve(Some(&pref), &NONE, &streams(0), None).subtitle, None);
    }

    #[test]
    fn text_subtitles_win_over_images_and_sdh_is_recognized() {
        let streams = vec![
            audio(1, "eng", None, true),
            sub(2, Some("eng"), None, false, false),
            sub(3, Some("eng"), None, false, true),
            sub(4, Some("eng"), Some("English [CC]"), false, true),
        ];
        let pref = StreamPreference { subtitles: true, subtitle_language: Some("eng".into()), ..Default::default() };
        assert_eq!(resolve(Some(&pref), &NONE, &streams, None).subtitle, Some(3), "text, no SDH, earliest");
        let sdh = StreamPreference { subtitle_title: Some("SDH".into()), ..pref };
        assert_eq!(resolve(Some(&sdh), &NONE, &streams, None).subtitle, Some(4), "`CC` marks SDH too");
        assert!(!is_sdh(Some("Accent")), "a word, not two letters inside one");
    }

    #[test]
    fn choices_are_kept_per_show_or_movie() {
        let mut episode = ItemRow::test_row(3, "episode", "Pilot");
        (episode.parent_id, episode.grandparent_id) = (Some(2), Some(1));
        assert_eq!(scope(&episode), 1);
        assert_eq!(scope(&ItemRow::test_row(9, "movie", "Alien")), 9);
        assert_eq!(scope(&ItemRow::test_row(8, "episode", "Orphan")), 8, "an episode without a show");
    }

    #[test]
    fn without_a_preference_only_matching_forced_subtitles_show() {
        let mut streams = episode(0);
        assert_eq!(resolve(None, &NONE, &streams, None), Selection::default());
        // English audio by default: the English forced track is shown.
        streams[1].default = false;
        streams[2].default = true;
        assert_eq!(resolve(None, &NONE, &streams, None).subtitle, Some(4));
    }

    #[test]
    fn titles_without_a_choice_start_with_the_users_defaults() {
        // A Japanese film: English 5.1 and a commentary, full and forced
        // English subtitles (and SDH), French ones.
        let film = episode(10);
        let with = |audio: Option<&str>, subtitles: Option<&str>, mode| Defaults {
            audio_language: audio.map(Into::into),
            subtitle_language: subtitles.map(Into::into),
            subtitle_mode: mode,
        };
        // English audio, its main track, not the commentary.
        let english = resolve(None, &with(Some("en"), None, SubtitleMode::Forced), &film, None);
        assert_eq!(english, Selection { audio: Some(12), subtitle: Some(14) }, "and its forced subtitles");
        // The default track is already Japanese: not pinned.
        assert_eq!(resolve(None, &with(Some("jpn"), None, SubtitleMode::Off), &film, None), Selection::default());
        // Foreign audio, subtitles in the reader's language: the full track, not SDH or forced.
        let reader = with(None, Some("eng"), SubtitleMode::Foreign);
        assert_eq!(resolve(None, &reader, &film, None).subtitle, Some(15));
        // The audio is in the reader's language: only forced ones.
        let at_home = with(Some("eng"), Some("eng"), SubtitleMode::Foreign);
        assert_eq!(resolve(None, &at_home, &film, None), Selection { audio: Some(12), subtitle: Some(14) });
        // Always, in French; in a language the file lacks, forced ones at most.
        assert_eq!(resolve(None, &with(None, Some("fr"), SubtitleMode::Always), &film, None).subtitle, Some(17));
        assert_eq!(resolve(None, &with(None, Some("ger"), SubtitleMode::Always), &film, None).subtitle, None);
        assert_eq!(
            resolve(None, &with(Some("eng"), Some("ger"), SubtitleMode::Always), &film, None).subtitle,
            Some(14)
        );
        // Off is off, forced subtitles included.
        assert_eq!(resolve(None, &with(Some("eng"), None, SubtitleMode::Off), &film, None).subtitle, None);
        // A remembered choice wins over the defaults.
        let chosen = StreamPreference { subtitles: false, ..Default::default() };
        assert_eq!(resolve(Some(&chosen), &with(None, Some("fre"), SubtitleMode::Always), &film, None).subtitle, None);
        assert!(valid_language("eng") && valid_language("fr") && !valid_language("English") && !valid_language("EN"));
        // Both ISO 639-2 forms and the two-letter code are one language.
        assert!(same_language(Some("ces"), Some("cze")) && same_language(Some("el"), Some("gre")));
        for (a, b) in [("sl", "slv"), ("isl", "ice"), ("msa", "may"), ("hy", "arm"), ("cym", "wel"), ("ta", "tam")] {
            assert!(same_language(Some(a), Some(b)), "{a} and {b}");
        }
        assert!(!same_language(Some("sl"), Some("slo")), "Slovenian isn't Slovak");
    }

    #[test]
    fn subtitles_named_forced_count_as_forced() {
        // Many files flag nothing and only name their forced tracks.
        let film = vec![
            audio(1, "eng", None, true),
            sub(2, Some("eng"), Some("English (Forced)"), false, true),
            sub(3, Some("eng"), Some("English"), false, true),
            sub(4, Some("eng"), Some("SDH"), false, true),
        ];
        let with = |mode| Defaults { audio_language: None, subtitle_language: Some("eng".into()), subtitle_mode: mode };
        assert_eq!(
            resolve(None, &with(SubtitleMode::Forced), &film, None).subtitle,
            Some(2),
            "forced, for English audio"
        );
        assert_eq!(
            resolve(None, &with(SubtitleMode::Always), &film, None).subtitle,
            Some(3),
            "full ones, not the forced"
        );
        // Picked, they are found again as forced in the next file.
        let pref = from_selection(&film, None, Some(2));
        assert!(pref.subtitle_forced);
        let mut next = film.clone();
        next.swap(1, 2);
        (next[1].index, next[2].index) = (2, 3);
        assert_eq!(resolve(Some(&pref), &NONE, &next, None).subtitle, Some(3));
        // A word, not letters inside one; anime's "Signs & Songs" too.
        assert!(!is_forced(&sub(5, None, Some("Unforced Errors"), false, true)));
        assert!(is_forced(&sub(5, None, Some("Signs & Songs"), false, true)));
    }

    /// A player for the tests: the codecs it decodes and those it passes
    /// through, whether it switches tracks, its output's channels.
    struct Player {
        decodes: &'static [&'static str],
        passes: &'static [&'static str],
        switches: bool,
        channels: Option<i64>,
        /// A version 1 player: its streams copy only AAC and MP3.
        v1: bool,
    }

    impl AudioOut for Player {
        fn plays(&self, codec: &str) -> bool {
            self.decodes.contains(&codec) || self.passes.contains(&codec)
        }
        fn passes_through(&self, codec: &str) -> bool {
            self.passes.contains(&codec)
        }
        fn switches_tracks(&self) -> bool {
            self.switches
        }
        fn channels(&self) -> Option<i64> {
            self.channels
        }
        fn stream_copies(&self, codec: &str) -> bool {
            let copied: &[&str] = if self.v1 { &["aac", "mp3"] } else { &["aac", "mp3", "ac3", "eac3"] };
            copied.contains(&codec) && self.plays(codec)
        }
    }

    /// A TV with a 7.1 receiver taking everything.
    const THEATER: Player = Player {
        decodes: &["aac"],
        passes: &["ac3", "eac3", "truehd", "dts"],
        switches: true,
        channels: Some(8),
        v1: false,
    };
    /// A TV with a 5.1 soundbar: Dolby Digital (Plus) passed through.
    const SOUNDBAR: Player =
        Player { decodes: &["aac"], passes: &["ac3", "eac3"], switches: true, channels: Some(6), v1: false };
    /// A phone: AAC and Dolby Digital (Plus) decoded, stereo.
    const PHONE: Player =
        Player { decodes: &["aac", "ac3", "eac3"], passes: &[], switches: true, channels: Some(2), v1: false };
    /// Chrome, a version 1 player: AAC, a file's default track only, no channels said.
    const CHROME: Player =
        Player { decodes: &["aac", "mp3", "opus", "flac"], passes: &[], switches: false, channels: None, v1: true };
    /// Safari, also version 1: Dolby Digital too.
    const SAFARI: Player =
        Player { decodes: &["aac", "ac3", "eac3"], passes: &[], switches: false, channels: None, v1: true };

    fn track(index: i64, codec: &str, channels: i64, title: Option<&str>, default: bool) -> MediaStream {
        MediaStream { codec: codec.into(), channels: Some(channels), ..audio(index, "eng", title, default) }
    }

    /// A 4K film: English TrueHD 7.1 with Atmos (the default), AC-3 5.1,
    /// AAC stereo, a commentary and an audio description, French AC-3 5.1.
    fn film() -> Vec<MediaStream> {
        let mut atmos = track(1, "truehd", 8, Some("English Atmos"), true);
        atmos.profile = Some("Dolby TrueHD + Dolby Atmos".into());
        vec![
            MediaStream { index: 0, kind: StreamKind::Video, codec: "hevc".into(), ..Default::default() },
            atmos,
            track(2, "ac3", 6, Some("English 5.1"), false),
            track(3, "aac", 2, Some("English Stereo"), false),
            track(4, "aac", 2, Some("Commentary"), false),
            track(5, "aac", 2, Some("Audio Description"), false),
            MediaStream { language: Some("fre".into()), ..track(6, "ac3", 6, None, false) },
        ]
    }

    #[test]
    fn each_player_gets_the_track_of_the_language_it_plays_best() {
        let play = |out: &Player| resolve(None, &NONE, &film(), Some(out)).audio;
        // The receiver takes the default's TrueHD and its Atmos: nothing to pin.
        assert_eq!(play(&THEATER), None);
        // The soundbar takes no TrueHD: Dolby Digital 5.1 as it is, not TrueHD converted.
        assert_eq!(play(&SOUNDBAR), Some(2));
        // The phone's stereo: the stereo mix, not 5.1 folded down.
        assert_eq!(play(&PHONE), Some(3));
        // Chrome plays AAC only: the stereo track, copied into a stream, never the
        // commentary or the description, though they play as they are too.
        assert_eq!(play(&CHROME), Some(3));
        // Safari plays Dolby Digital, but not in a stream of a version 1
        // player, which converts it: the stereo track, copied as it is.
        assert_eq!(play(&SAFARI), Some(3));
        // A version 2 player that doesn't switch tracks (a TV app's old
        // engine) has the Dolby Digital copied into its stream: 5.1.
        assert_eq!(play(&Player { v1: false, ..SAFARI }), Some(2));
        // An item's details don't know the player: the default, as ever.
        assert_eq!(resolve(None, &NONE, &film(), None).audio, None);
        // The profile's language first: French, as it is.
        let french = Defaults { audio_language: Some("fr".into()), ..NONE };
        assert_eq!(resolve(None, &french, &film(), Some(&CHROME)).audio, Some(6), "converted: the only French track");
    }

    #[test]
    fn a_player_that_plays_only_the_default_keeps_it_when_it_plays() {
        // Safari plays both: the default plays with the file as it is, the other only in a stream.
        let streams = vec![track(1, "aac", 2, None, true), track(2, "eac3", 6, None, false)];
        assert_eq!(resolve(None, &NONE, &streams, Some(&SAFARI)).audio, None);
        // A player that switches tracks itself takes the 5.1.
        let switching = Player { switches: true, ..SAFARI };
        assert_eq!(resolve(None, &NONE, &streams, Some(&switching)).audio, Some(2));
    }

    #[test]
    fn a_receiver_behind_a_stereo_output_gets_what_it_decodes() {
        // Kodi or a TV box set to 2.0 (its PCM output) with Dolby Digital
        // passed through to a receiver on optical or ARC: the AC-3 5.1, not
        // the stereo mix, nor the TrueHD decoded and folded down.
        let arc =
            Player { decodes: &["aac", "truehd"], passes: &["ac3"], switches: true, channels: Some(2), v1: false };
        assert_eq!(resolve(None, &NONE, &film(), Some(&arc)).audio, Some(2));
        // Without passthrough, stereo as before.
        assert_eq!(resolve(None, &NONE, &film(), Some(&Player { passes: &[], ..arc })).audio, Some(3));
    }

    #[test]
    fn a_track_its_stream_would_convert_gives_way_to_one_it_copies() {
        // A Fire TV Stick Lite's old app: plays AC-3 with the file as it is,
        // but a file whose TrueHD default it can't play streams, and its
        // stream copies only AAC: the English stereo, not the 5.1 converted.
        let lite =
            Player { decodes: &["aac", "ac3", "eac3", "dts"], passes: &[], switches: false, channels: None, v1: true };
        assert_eq!(resolve(None, &NONE, &film(), Some(&lite)).audio, Some(3));
        // With a Dolby Digital default, that one plays with the file as it is.
        let streams = vec![track(1, "ac3", 6, None, true), track(2, "aac", 2, None, false)];
        assert_eq!(resolve(None, &NONE, &streams, Some(&lite)).audio, None);
        // Only converted tracks in the language: the one cheaper to convert,
        // as both come out the same (the DTS's lossy core, not TrueHD).
        let only = vec![track(1, "truehd", 8, None, true), track(2, "dts", 6, None, false)];
        assert_eq!(resolve(None, &NONE, &only, Some(&lite)).audio, Some(2));
    }

    #[test]
    fn converted_audio_comes_from_the_track_cheapest_to_convert() {
        // Chrome converts both to stereo AAC: from the AC-3, not the TrueHD.
        let streams = vec![track(1, "truehd", 8, None, true), track(2, "ac3", 6, None, false)];
        assert_eq!(resolve(None, &NONE, &streams, Some(&CHROME)).audio, Some(2));
        // A 5.1 output: what's mixed for 5.1 before what's folded down to
        // it, though it costs more to convert.
        let six = Player { decodes: &["aac"], channels: Some(6), ..CHROME };
        let streams = vec![track(1, "truehd", 6, None, true), track(2, "eac3", 8, None, false)];
        assert_eq!(resolve(None, &NONE, &streams, Some(&six)).audio, None);
        // Of two lossy tracks alike, the fewer bits.
        let with_bits =
            |index, bits, default| MediaStream { bitrate: Some(bits), ..track(index, "dts", 6, None, default) };
        let streams = vec![with_bits(1, 1_509_000, true), with_bits(2, 768_000, false)];
        assert_eq!(resolve(None, &NONE, &streams, Some(&CHROME)).audio, Some(2));
    }

    #[test]
    fn a_file_that_streams_anyway_has_the_track_its_stream_copies() {
        // A TV box that decodes DTS and Dolby Digital, and switches tracks.
        let boxed = Player {
            decodes: &["aac", "ac3", "eac3", "dts"],
            passes: &[],
            switches: true,
            channels: Some(6),
            v1: false,
        };
        let streams = vec![track(1, "dts", 6, None, true), track(2, "ac3", 6, None, false)];
        // The file as it is: the DTS default plays.
        assert_eq!(resolve(None, &NONE, &streams, Some(&boxed)).audio, None);
        // Its video converted (Automatic lowered it): MPEG-TS takes no DTS,
        // so the stream copies the AC-3 rather than converting the DTS.
        assert_eq!(resolve(None, &NONE, &streams, Some(&Streamed(&boxed))).audio, Some(2));
        // A remembered DTS pick gives way the same, as on a player without DTS.
        let picked = from_selection(&streams, Some(1), None);
        assert_eq!(resolve(Some(&picked), &NONE, &streams, Some(&boxed)).audio, None);
        assert_eq!(resolve(Some(&picked), &NONE, &streams, Some(&Streamed(&boxed))).audio, Some(2));
        // Only DTS in the language: converted, as there's nothing to copy.
        let only = vec![track(1, "dts", 6, None, true)];
        assert_eq!(resolve(None, &NONE, &only, Some(&Streamed(&boxed))).audio, None);
    }

    #[test]
    fn a_remembered_choice_keeps_its_kind_and_title_where_they_play() {
        let streams = film();
        // The commentary, wherever.
        let commentary = from_selection(&streams, Some(4), None);
        assert_eq!(resolve(Some(&commentary), &NONE, &streams, Some(&THEATER)).audio, Some(4));
        // A stereo mix picked on purpose stays, where Atmos would play too.
        let stereo = from_selection(&streams, Some(3), None);
        assert_eq!(resolve(Some(&stereo), &NONE, &streams, Some(&THEATER)).audio, Some(3));
        // Atmos picked at home; on the phone, which takes no TrueHD, the English track it plays best.
        let atmos = from_selection(&streams, Some(1), None);
        assert_eq!(resolve(Some(&atmos), &NONE, &streams, Some(&THEATER)).audio, None);
        assert_eq!(resolve(Some(&atmos), &NONE, &streams, Some(&PHONE)).audio, Some(3));
        assert_eq!(resolve(Some(&atmos), &NONE, &streams, Some(&SOUNDBAR)).audio, Some(2));
    }

    #[test]
    fn subtitles_chosen_alone_leave_the_audio_to_the_defaults_and_the_player() {
        let subtitles =
            StreamPreference { subtitles: true, subtitle_language: Some("eng".into()), ..Default::default() };
        assert_eq!(resolve(Some(&subtitles), &NONE, &film(), Some(&SOUNDBAR)).audio, Some(2));
        let french = Defaults { audio_language: Some("fre".into()), ..NONE };
        assert_eq!(resolve(Some(&subtitles), &french, &film(), None).audio, Some(6));
    }

    #[test]
    fn tracks_without_a_known_language_are_never_swapped() {
        let mut streams = film();
        for s in streams.iter_mut().filter(|s| s.kind == StreamKind::Audio) {
            s.language = Some("und".into());
        }
        assert_eq!(resolve(None, &NONE, &streams, Some(&SOUNDBAR)).audio, None, "undetermined: may be any language");
        for s in &mut streams {
            s.language = None;
        }
        assert_eq!(resolve(None, &NONE, &streams, Some(&SOUNDBAR)).audio, None);
        // One picked without a language is found again by its title.
        let picked = from_selection(&streams, Some(2), None);
        assert_eq!((picked.audio_language.as_deref(), picked.audio_title.as_deref()), (None, Some("English 5.1")));
        assert_eq!(resolve(Some(&picked), &NONE, &streams, None).audio, Some(2));
    }

    #[test]
    fn lossless_and_object_audio_are_told_apart() {
        let dts = |profile: &str| MediaStream { profile: Some(profile.into()), ..track(1, "dts", 6, None, true) };
        assert!(fit(&dts("DTS-HD MA"), &THEATER, None).1.lossless);
        assert!(fit(&dts("DTS-HD MA + DTS:X"), &THEATER, None).1.objects);
        assert!(!fit(&dts("DTS"), &THEATER, None).1.lossless && !fit(&dts("DTS-HD HRA"), &THEATER, None).1.lossless);
        // Atmos decoded on the device is lost: it counts only passed through.
        let joc =
            MediaStream { profile: Some("Dolby Digital Plus + Dolby Atmos".into()), ..track(1, "eac3", 6, None, true) };
        assert!(fit(&joc, &SOUNDBAR, None).1.objects && !fit(&joc, &PHONE, None).1.objects);
        for (title, kind) in [
            ("Audio Description", AudioKind::Description),
            ("English AD", AudioKind::Description),
            ("Descriptive Video Service", AudioKind::Description),
            ("Director's Commentary", AudioKind::Commentary),
            ("Adventure Mix", AudioKind::Main),
        ] {
            assert_eq!(audio_kind(Some(title)), kind, "{title}");
        }
    }
}
