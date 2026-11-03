//! The device's settings, each with its default (the Android TV app's
//! Settings): stored only when they
//! differ from the default, so a new default reaches everyone who never
//! changed it; read one by one, so a value this version doesn't know falls
//! back to the default without losing the others.
//!
//! The profile's track defaults are the server's (`PUT /api/me/track-defaults`),
//! not settings of the TV.

use std::collections::BTreeMap;
use std::io;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Store, keys};
use crate::model::{QUALITY_PRESETS, QualityPreset, quality_preset};

/// Text subtitles' size, a share of the video's height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleSize {
    Small,
    #[default]
    Normal,
    Large,
    Huge,
}

impl SubtitleSize {
    /// In Settings' order.
    pub const ALL: [SubtitleSize; 4] =
        [SubtitleSize::Small, SubtitleSize::Normal, SubtitleSize::Large, SubtitleSize::Huge];

    /// The text's size as a share of the video's height: 3.8, 4.8, 6 and 7.6 %.
    pub fn fraction(self) -> f32 {
        match self {
            SubtitleSize::Small => 0.038,
            SubtitleSize::Normal => 0.048,
            SubtitleSize::Large => 0.06,
            SubtitleSize::Huge => 0.076,
        }
    }

    /// The name Settings shows.
    pub fn label(self) -> &'static str {
        match self {
            SubtitleSize::Small => "Small",
            SubtitleSize::Normal => "Normal",
            SubtitleSize::Large => "Large",
            SubtitleSize::Huge => "Huge",
        }
    }
}

/// What text subtitles are drawn on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleBackground {
    /// An outline around the letters.
    #[default]
    None,
    DarkBox,
}

impl SubtitleBackground {
    /// In Settings' order.
    pub const ALL: [SubtitleBackground; 2] = [SubtitleBackground::None, SubtitleBackground::DarkBox];

    /// The name Settings shows.
    pub fn label(self) -> &'static str {
        match self {
            SubtitleBackground::None => "None",
            SubtitleBackground::DarkBox => "Dark box",
        }
    }
}

/// The face text subtitles are drawn in (bold).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleFont {
    #[default]
    Sans,
    Serif,
    Monospace,
    /// Offered only where the TV has a casual face.
    Casual,
}

impl SubtitleFont {
    /// In Settings' order.
    pub const ALL: [SubtitleFont; 4] =
        [SubtitleFont::Sans, SubtitleFont::Serif, SubtitleFont::Monospace, SubtitleFont::Casual];

    /// The name Settings shows.
    pub fn label(self) -> &'static str {
        match self {
            SubtitleFont::Sans => "Sans serif",
            SubtitleFont::Serif => "Serif",
            SubtitleFont::Monospace => "Monospace",
            SubtitleFont::Casual => "Casual",
        }
    }
}

/// How far text subtitles sit above the bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitlePosition {
    #[default]
    Bottom,
    Raised,
    Higher,
    Highest,
}

impl SubtitlePosition {
    /// In Settings' order.
    pub const ALL: [SubtitlePosition; 4] =
        [SubtitlePosition::Bottom, SubtitlePosition::Raised, SubtitlePosition::Higher, SubtitlePosition::Highest];

    /// The raise as a share of the video's height: 0, 5, 10 and 15 %.
    pub fn raise(self) -> f32 {
        match self {
            SubtitlePosition::Bottom => 0.0,
            SubtitlePosition::Raised => 0.05,
            SubtitlePosition::Higher => 0.10,
            SubtitlePosition::Highest => 0.15,
        }
    }

    /// The name Settings shows.
    pub fn label(self) -> &'static str {
        match self {
            SubtitlePosition::Bottom => "Bottom",
            SubtitlePosition::Raised => "Raised",
            SubtitlePosition::Higher => "Higher",
            SubtitlePosition::Highest => "Highest",
        }
    }
}

/// A surround format that can be sent as it is to a TV or receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PassthroughFormat {
    Ac3,
    Eac3,
    /// Dolby Atmos in E-AC-3.
    Eac3Joc,
    TrueHd,
    Dts,
    /// DTS-HD and DTS:X.
    DtsHd,
}

impl PassthroughFormat {
    /// In the Audio settings' order.
    pub const ALL: [PassthroughFormat; 6] = [
        PassthroughFormat::Ac3,
        PassthroughFormat::Eac3,
        PassthroughFormat::Eac3Joc,
        PassthroughFormat::TrueHd,
        PassthroughFormat::Dts,
        PassthroughFormat::DtsHd,
    ];

    /// The name the capability report uses.
    pub fn codec(self) -> &'static str {
        match self {
            PassthroughFormat::Ac3 => "ac3",
            PassthroughFormat::Eac3 => "eac3",
            PassthroughFormat::Eac3Joc => "eac3-joc",
            PassthroughFormat::TrueHd => "truehd",
            PassthroughFormat::Dts => "dts",
            PassthroughFormat::DtsHd => "dts-hd",
        }
    }

    /// The name Settings shows.
    pub fn label(self) -> &'static str {
        match self {
            PassthroughFormat::Ac3 => "Dolby Digital",
            PassthroughFormat::Eac3 => "Dolby Digital Plus",
            PassthroughFormat::Eac3Joc => "Dolby Atmos",
            PassthroughFormat::TrueHd => "Dolby TrueHD",
            PassthroughFormat::Dts => "DTS",
            PassthroughFormat::DtsHd => "DTS-HD and DTS:X",
        }
    }
}

/// Whether a format is sent as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassthroughChoice {
    /// As the TV or receiver says it plays.
    #[default]
    Auto,
    /// Always pass it through.
    On,
    /// Never: Pley converts it.
    Off,
}

impl PassthroughChoice {
    /// In Settings' order.
    pub const ALL: [PassthroughChoice; 3] = [PassthroughChoice::Auto, PassthroughChoice::On, PassthroughChoice::Off];

    /// The name Settings shows.
    pub fn label(self) -> &'static str {
        match self {
            PassthroughChoice::Auto => "Auto",
            PassthroughChoice::On => "On",
            PassthroughChoice::Off => "Off",
        }
    }

    /// What Settings says under it.
    pub fn hint(self) -> &'static str {
        match self {
            PassthroughChoice::Auto => "As the TV or receiver says it plays",
            PassthroughChoice::On => "Always pass it through",
            PassthroughChoice::Off => "Never: Pley converts it",
        }
    }
}

/// The passthrough choice of every format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct PassthroughChoices {
    pub ac3: PassthroughChoice,
    pub eac3: PassthroughChoice,
    pub eac3_joc: PassthroughChoice,
    pub truehd: PassthroughChoice,
    pub dts: PassthroughChoice,
    pub dts_hd: PassthroughChoice,
}

impl PassthroughChoices {
    /// The choice for `f`.
    pub fn get(&self, f: PassthroughFormat) -> PassthroughChoice {
        match f {
            PassthroughFormat::Ac3 => self.ac3,
            PassthroughFormat::Eac3 => self.eac3,
            PassthroughFormat::Eac3Joc => self.eac3_joc,
            PassthroughFormat::TrueHd => self.truehd,
            PassthroughFormat::Dts => self.dts,
            PassthroughFormat::DtsHd => self.dts_hd,
        }
    }

    /// Sets the choice for `f`.
    pub fn set(&mut self, f: PassthroughFormat, choice: PassthroughChoice) {
        let slot = match f {
            PassthroughFormat::Ac3 => &mut self.ac3,
            PassthroughFormat::Eac3 => &mut self.eac3,
            PassthroughFormat::Eac3Joc => &mut self.eac3_joc,
            PassthroughFormat::TrueHd => &mut self.truehd,
            PassthroughFormat::Dts => &mut self.dts,
            PassthroughFormat::DtsHd => &mut self.dts_hd,
        };
        *slot = choice;
    }
}

/// The TV's settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// The id of the quality the player starts at: `original`, `auto` or a
    /// preset (`30m` … `2m`). Automatic by default.
    pub quality: String,
    /// Up Next plays the next episode after a 10 second countdown.
    pub autoplay_next: bool,
    /// The readout of how a video plays, over it.
    pub playback_details: bool,
    /// Which surround formats are sent as they are; feeds the capability report.
    pub passthrough: PassthroughChoices,
    pub subtitle_size: SubtitleSize,
    pub subtitle_background: SubtitleBackground,
    pub subtitle_font: SubtitleFont,
    pub subtitle_position: SubtitlePosition,
    /// No zooming, sliding or fading: things change in place.
    pub reduce_motion: bool,
    /// "Always start as": the profile that skips "Who's watching?", by server
    /// (profile ids are a server's own).
    pub always_start_as: BTreeMap<String, i64>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            quality: "auto".into(),
            autoplay_next: true,
            playback_details: false,
            passthrough: PassthroughChoices::default(),
            subtitle_size: SubtitleSize::Normal,
            subtitle_background: SubtitleBackground::None,
            subtitle_font: SubtitleFont::Sans,
            subtitle_position: SubtitlePosition::Bottom,
            reduce_motion: false,
            always_start_as: BTreeMap::new(),
        }
    }
}

fn field<T: DeserializeOwned>(map: &Map<String, Value>, key: &str, into: &mut T) {
    if let Some(v) = map.get(key)
        && let Ok(x) = serde_json::from_value(v.clone())
    {
        *into = x;
    }
}

impl Settings {
    /// The settings kept in `store`, over the defaults.
    pub fn load(store: &Store) -> Settings {
        store.get::<Value>(keys::SETTINGS).map(|v| Settings::from_json(&v)).unwrap_or_default()
    }

    /// Keeps the settings that differ from their defaults.
    pub fn save(&self, store: &Store) -> io::Result<()> {
        store.set(keys::SETTINGS, &self.to_json())
    }

    /// Settings read field by field over the defaults: a field of another
    /// shape, or a quality this version doesn't offer, keeps its default.
    pub fn from_json(v: &Value) -> Settings {
        let mut s = Settings::default();
        let Some(map) = v.as_object() else { return s };
        field(map, "quality", &mut s.quality);
        if quality_preset(&s.quality).is_none() {
            s.quality = Settings::default().quality;
        }
        field(map, "autoplay_next", &mut s.autoplay_next);
        field(map, "playback_details", &mut s.playback_details);
        if let Some(p) = map.get("passthrough").and_then(Value::as_object) {
            for (key, slot) in [
                ("ac3", &mut s.passthrough.ac3),
                ("eac3", &mut s.passthrough.eac3),
                ("eac3_joc", &mut s.passthrough.eac3_joc),
                ("truehd", &mut s.passthrough.truehd),
                ("dts", &mut s.passthrough.dts),
                ("dts_hd", &mut s.passthrough.dts_hd),
            ] {
                field(p, key, slot);
            }
        }
        field(map, "subtitle_size", &mut s.subtitle_size);
        field(map, "subtitle_background", &mut s.subtitle_background);
        field(map, "subtitle_font", &mut s.subtitle_font);
        field(map, "subtitle_position", &mut s.subtitle_position);
        field(map, "reduce_motion", &mut s.reduce_motion);
        field(map, "always_start_as", &mut s.always_start_as);
        s
    }

    /// The settings that differ from their defaults, as kept.
    pub fn to_json(&self) -> Value {
        let (Ok(Value::Object(mut mine)), Ok(Value::Object(defaults))) =
            (serde_json::to_value(self), serde_json::to_value(Settings::default()))
        else {
            return Value::Object(Map::new());
        };
        if let (Some(Value::Object(p)), Some(Value::Object(d))) =
            (mine.get_mut("passthrough"), defaults.get("passthrough"))
        {
            p.retain(|k, v| d.get(k) != Some(v));
        }
        mine.retain(|k, v| match v {
            Value::Object(o) if o.is_empty() => false,
            _ => defaults.get(k) != Some(v),
        });
        Value::Object(mine)
    }

    /// The quality the player starts at.
    pub fn quality_preset(&self) -> &'static QualityPreset {
        quality_preset(&self.quality).unwrap_or(&QUALITY_PRESETS[1])
    }

    /// The profile that skips "Who's watching?" on `server`.
    pub fn always_start_as(&self, server: &str) -> Option<i64> {
        self.always_start_as.get(server).copied()
    }

    /// Sets (or clears) the profile that skips "Who's watching?" on `server`.
    pub fn set_always_start_as(&mut self, server: &str, profile: Option<i64>) {
        match profile {
            Some(p) => self.always_start_as.insert(server.to_string(), p),
            None => self.always_start_as.remove(server),
        };
    }
}
