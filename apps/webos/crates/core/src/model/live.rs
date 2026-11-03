//! Live TV: the summary, channels and programmes, categories, the guide,
//! watching, and the answers of a source whose connections are all in use.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{is_false, nd};

/// `GET /api/live`: whether to show Live TV. `allowed`: it is open to this
/// profile; `available`: and it has channels (the rail shows it then).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveSummary {
    #[serde(default, deserialize_with = "nd")]
    pub available: bool,
    #[serde(default, deserialize_with = "nd")]
    pub allowed: bool,
    #[serde(default, deserialize_with = "nd")]
    pub channels: i64,
    /// Sources set up (administrators; 0 for others).
    #[serde(default, deserialize_with = "nd")]
    pub sources: i64,
    /// The channels watched last, newest first.
    #[serde(default, deserialize_with = "nd")]
    pub recent: Vec<i64>,
}

/// A programme of the guide. Times are unix seconds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveProgram {
    #[serde(default, deserialize_with = "nd")]
    pub channel_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub start: i64,
    #[serde(default, deserialize_with = "nd")]
    pub stop: i64,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub categories: Vec<String>,
    /// The picture's address on the server, when the guide has one.
    pub image: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub is_new: bool,
    /// Shown as it happens (a match, the news).
    #[serde(default, deserialize_with = "nd")]
    pub is_live: bool,
    pub rating: Option<String>,
}

impl LiveProgram {
    /// On at `now` (unix seconds).
    pub fn airing_at(&self, now: i64) -> bool {
        self.start <= now && self.stop > now
    }

    /// How far it is at `now`, from 0 to 1.
    pub fn progress_at(&self, now: i64) -> f32 {
        ((now - self.start) as f32 / (self.stop - self.start).max(1) as f32).clamp(0.0, 1.0)
    }

    /// `S2 E14`, or `E14`, when the guide numbers the episode.
    pub fn episode_label(&self) -> Option<String> {
        match (self.season, self.episode) {
            (Some(s), Some(e)) => Some(format!("S{s} E{e}")),
            (None, Some(e)) => Some(format!("E{e}")),
            _ => None,
        }
    }
}

/// A channel of the lineup with what's on now and next.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveChannel {
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub source_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    /// A string: `101`, `5.1`.
    pub number: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub group: String,
    /// The logo's address on the server.
    pub logo: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub favorite: bool,
    /// The guide has programmes for it.
    #[serde(default, deserialize_with = "nd")]
    pub has_guide: bool,
    pub now: Option<LiveProgram>,
    pub next: Option<LiveProgram>,
}

impl LiveChannel {
    /// What is on at `now` (unix seconds); none once the listed ones ended.
    pub fn on_at(&self, now: i64) -> Option<&LiveProgram> {
        self.now.as_ref().filter(|p| p.airing_at(now)).or_else(|| self.next.as_ref().filter(|p| p.airing_at(now)))
    }

    /// What comes after [`LiveChannel::on_at`]: `next`, or what was listed as now when it hasn't begun.
    pub fn next_at(&self, now: i64) -> Option<&LiveProgram> {
        let listed = [self.now.as_ref(), self.next.as_ref()];
        match self.on_at(now) {
            None => listed.into_iter().flatten().find(|p| p.start > now),
            Some(on) => listed.into_iter().flatten().find(|p| p.start >= on.stop),
        }
    }
}

/// A group of channels as the provider names it: `region` is the `UK` of `UK| Sport`, `label` the rest.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveGroup {
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    pub region: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub label: String,
    #[serde(default, deserialize_with = "nd")]
    pub count: i64,
}

impl LiveGroup {
    /// The label to show: the label, else the whole name.
    pub fn shown_label(&self) -> &str {
        if self.label.is_empty() { &self.name } else { &self.label }
    }
}

/// `GET /api/live/groups`: the groups in the lineup's order and the counts of the lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveGroups {
    #[serde(default, deserialize_with = "nd")]
    pub total: i64,
    #[serde(default, deserialize_with = "nd")]
    pub favorites: i64,
    #[serde(default, deserialize_with = "nd")]
    pub recent: i64,
    #[serde(default, deserialize_with = "nd")]
    pub groups: Vec<LiveGroup>,
}

/// `GET /api/live/browse`: `channels` from `offset` of a list of `total`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveChannelPage {
    #[serde(default, deserialize_with = "nd")]
    pub total: i64,
    #[serde(default, deserialize_with = "nd")]
    pub offset: i64,
    #[serde(default, deserialize_with = "nd")]
    pub channels: Vec<LiveChannel>,
}

/// Which channels a list has (`category` of `GET /api/live/browse`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum LiveCategory {
    #[default]
    All,
    Favorites,
    Recent,
    /// One group's, by its name.
    Group(String),
}

impl LiveCategory {
    /// How it is kept: `all`, `favorites`, `recent` or `group:<name>` (the TV's `pley.live.category`).
    pub fn key(&self) -> String {
        match self {
            LiveCategory::All => "all".into(),
            LiveCategory::Favorites => "favorites".into(),
            LiveCategory::Recent => "recent".into(),
            LiveCategory::Group(name) => format!("group:{name}"),
        }
    }

    /// `category` on the wire.
    pub fn wire(&self) -> &'static str {
        match self {
            LiveCategory::All => "all",
            LiveCategory::Favorites => "favorites",
            LiveCategory::Recent => "recent",
            LiveCategory::Group(_) => "group",
        }
    }

    /// The group's name for `group=`, for a group.
    pub fn group(&self) -> Option<&str> {
        match self {
            LiveCategory::Group(name) => Some(name),
            _ => None,
        }
    }

    /// The category kept as `key`; anything else is All.
    pub fn parse(key: &str) -> LiveCategory {
        match key {
            "favorites" => LiveCategory::Favorites,
            "recent" => LiveCategory::Recent,
            _ => match key.strip_prefix("group:") {
                Some(name) => LiveCategory::Group(name.to_string()),
                None => LiveCategory::All,
            },
        }
    }
}

/// `GET /api/live/guide`: the programmes between `from` and `to` (unix seconds), by channel id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveGuide {
    #[serde(default, deserialize_with = "nd")]
    pub from: i64,
    #[serde(default, deserialize_with = "nd")]
    pub to: i64,
    /// Keyed by channel id (strings on the wire).
    #[serde(default, deserialize_with = "nd")]
    pub programs: BTreeMap<i64, Vec<LiveProgram>>,
}

/// The body of `POST /api/live/channels/{id}/watch`: `client_id` ends what
/// this TV watched before; `keyed_urls` because the TV's player sends no
/// headers; `take_over`, every connection being in use, is the channel whose
/// connection to take.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "is_false")]
    pub keyed_urls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_over: Option<i64>,
}

/// `POST /api/live/channels/{id}/watch`'s answer: the live playlist's
/// address (keyed when asked), watched as `session_id` until
/// `DELETE /api/live/sessions/{session}` or 45 s without a request.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveWatching {
    #[serde(default, deserialize_with = "nd")]
    pub session_id: String,
    #[serde(default, deserialize_with = "nd")]
    pub url: String,
    /// `hls`.
    #[serde(default, deserialize_with = "nd")]
    pub kind: String,
    pub channel: Option<LiveChannel>,
}

/// `GET /api/live/sessions/{session}/plan`: what the channel carries and
/// what the transcoder makes of it (`video`: `copy` or the encoder;
/// `audio`: `copy`, `aac` or `none`). Answers `null` while unknown.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LivePlan {
    pub video_codec: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub frame_rate: Option<f64>,
    #[serde(default, deserialize_with = "nd")]
    pub interlaced: bool,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub video: String,
    #[serde(default, deserialize_with = "nd")]
    pub audio: String,
}

impl LivePlan {
    /// The channel is sent as it is (or without picture or sound).
    pub fn direct(&self) -> bool {
        let as_is = |s: &str| s == "copy" || s == "none";
        as_is(&self.video) && as_is(&self.audio)
    }
}

/// A connection of a source whose connections are all in use (a `409`'s
/// `connections`): the channel it watches, who (named to administrators
/// only), how many, whether only you do, and whether you may take it over.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BusyConnection {
    #[serde(default, deserialize_with = "nd")]
    pub channel_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub channel_name: String,
    #[serde(default, deserialize_with = "nd")]
    pub watchers: Vec<String>,
    #[serde(default, deserialize_with = "nd")]
    pub people: i64,
    #[serde(default, deserialize_with = "nd")]
    pub yours: bool,
    #[serde(default, deserialize_with = "nd")]
    pub can_take_over: bool,
}
