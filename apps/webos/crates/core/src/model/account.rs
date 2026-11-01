//! The server itself, who the app acts as, and the household's profiles.

use serde::{Deserialize, Serialize};

use super::{nd, wire_enum};

fn yes() -> bool {
    true
}

fn nd_yes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(d)?.unwrap_or(true))
}

/// `GET /api/server`, answered without signing in: the server's name and
/// version, and whether it still has to be set up in a browser. Pley has no
/// id here: servers found by address match discovered ones by address only.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    /// Present on every Pley: a probe counts a JSON answer with a string
    /// `version` as Pley (`packages/tv/src/auth/servers.ts`).
    pub version: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub commit: String,
    #[serde(default, deserialize_with = "nd")]
    pub setup_required: bool,
    /// Setting up takes the code from the server's log.
    #[serde(default, deserialize_with = "nd")]
    pub setup_code_required: bool,
}

/// `GET /api/health`: `{ "status": "ok" }` while the server runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Health {
    #[serde(default, deserialize_with = "nd")]
    pub status: String,
}

wire_enum! {
    /// When subtitles come on by themselves for titles without a choice
    /// (apps/server/src/streams.rs, `SubtitleMode`).
    pub enum SubtitleMode or Unknown {
        /// Never.
        Off = "off",
        /// Forced subtitles (foreign dialogue) in the audio's language: the server's default.
        #[default]
        Forced = "forced",
        /// Full subtitles in the subtitle language when the audio is in another.
        Foreign = "foreign",
        /// Always, in the subtitle language.
        Always = "always",
        Unknown = "unknown",
    }
}

/// Who the app acts as: `GET /api/auth/me` with an app's token (the
/// server's `UserDto` plus the `scope` the app was allowed). `is_admin`,
/// `all_libraries` and `max_age` are the app's rights, never an
/// administrator's: `is_admin` is always false for an app.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Me {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub username: String,
    /// The display name, or the username: what to show.
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub is_admin: bool,
    /// A profile without a password, reached by switching from a home member.
    #[serde(default, deserialize_with = "nd")]
    pub managed: bool,
    /// Listed in the household's profile switcher.
    #[serde(default, deserialize_with = "nd")]
    pub home: bool,
    /// The profile has a PIN: "Who's watching?" asks for it at a cold start
    /// (the profiles list always says `none` for the current profile).
    #[serde(default, deserialize_with = "nd")]
    pub has_pin: bool,
    /// `/api/users/2/avatar?v=…`, or none without a picture.
    pub avatar_url: Option<String>,
    /// `#38bdf8`: the circle behind the initial when there's no picture.
    pub avatar_color: Option<String>,
    /// False: only the libraries in `library_ids`. Left out, true, as on Android.
    #[serde(default = "yes", deserialize_with = "nd_yes")]
    pub all_libraries: bool,
    #[serde(default, deserialize_with = "nd")]
    pub library_ids: Vec<i64>,
    /// The age limit for movies and shows; none: no limit.
    pub max_age: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub disabled: bool,
    #[serde(default, deserialize_with = "nd")]
    pub created_at: i64,
    /// ISO 639 code; none: the file's default track.
    pub audio_language: Option<String>,
    /// None: the audio's language.
    pub subtitle_language: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub subtitle_mode: SubtitleMode,
    #[serde(default, deserialize_with = "nd")]
    pub has_password: bool,
    #[serde(default, deserialize_with = "nd")]
    pub two_step: bool,
    /// What the app was allowed, separated by spaces (`library play playlists profiles`).
    pub scope: Option<String>,
}

impl Me {
    /// The scope as a list: `library`, `play`, `playlists`, `profiles`.
    pub fn scopes(&self) -> Vec<&str> {
        self.scope.as_deref().unwrap_or("").split(' ').filter(|s| !s.is_empty()).collect()
    }

    /// Whether the app was allowed `scope`.
    pub fn allows(&self, scope: &str) -> bool {
        self.scopes().contains(&scope)
    }

    /// The name to show: the display name, else the name, else the username.
    pub fn shown_name(&self) -> &str {
        [self.display_name.as_deref(), Some(self.name.as_str()), Some(self.username.as_str())]
            .into_iter()
            .flatten()
            .find(|s| !s.trim().is_empty())
            .unwrap_or("")
    }

    /// The tracks this profile starts titles with.
    pub fn track_defaults(&self) -> TrackDefaults {
        TrackDefaults {
            audio_language: self.audio_language.clone(),
            subtitle_language: self.subtitle_language.clone(),
            subtitle_mode: self.subtitle_mode,
        }
    }
}

/// The body of `PUT /api/me/track-defaults` (all three at once: a language
/// left out is `null`, a mode left out `forced`), and its fields in [`Me`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackDefaults {
    /// The main audio track in this language; none: the file's default track.
    pub audio_language: Option<String>,
    /// None: the audio's language.
    pub subtitle_language: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub subtitle_mode: SubtitleMode,
}

wire_enum! {
    /// What switching to a profile takes (`needs` of `GET /api/oauth/profiles`).
    pub enum ProfileNeeds or Locked {
        /// Nothing: it switches at once (always the case for the current profile).
        None = "none",
        /// Its PIN, typed on the TV.
        Pin = "pin",
        /// Its password, which only a browser asks for: approved with a new code.
        Password = "password",
        /// It can't be used from the app. An unknown `needs` reads as this, as on Android.
        #[default]
        Locked = "locked",
    }
}

/// One profile of the household (`GET /api/oauth/profiles`, scope
/// `profiles`). Empty when the app's profile isn't a home member.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppProfile {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    pub avatar_url: Option<String>,
    pub avatar_color: Option<String>,
    /// The profile the app acts as.
    #[serde(default, deserialize_with = "nd")]
    pub current: bool,
    #[serde(default, deserialize_with = "nd")]
    pub needs: ProfileNeeds,
}
