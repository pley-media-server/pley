//! Requests and Discover (docs/API.md, "Requests"): what requests are for this sign-in, TMDB's
//! lists and titles, the requests themselves, and a library title's place among them.

use serde::{Deserialize, Serialize};

use super::{nd, wire_enum};

wire_enum! {
    /// A movie or a show: TMDB numbers the two separately.
    pub enum TitleKind or Unknown {
        Movie = "movie",
        Show = "show",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// What asking from this sign-in does.
    pub enum RequestAccess or Off {
        #[default]
        Off = "off",
        Ask = "ask",
        Auto = "auto",
    }
}

wire_enum! {
    /// The decision.
    pub enum RequestStatus or Unknown {
        Pending = "pending",
        Approved = "approved",
        Declined = "declined",
        Failed = "failed",
        Available = "available",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// Where a request has got to, from the decision to the library.
    pub enum RequestStage or Unknown {
        Pending = "pending",
        Declined = "declined",
        Failed = "failed",
        Approved = "approved",
        Waiting = "waiting",
        Searching = "searching",
        Downloading = "downloading",
        Importing = "importing",
        Partial = "partial",
        Available = "available",
        Gone = "gone",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// What this sign-in may do to a request now, in the order to list them.
    pub enum RequestAction or Unknown {
        Withdraw = "withdraw",
        Unwatch = "unwatch",
        Approve = "approve",
        Retry = "retry",
        Decline = "decline",
        Handled = "handled",
        Delete = "delete",
        AskAgain = "ask_again",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// Discover's lists.
    pub enum DiscoverList or Trending {
        #[default]
        Trending = "trending",
        Movies = "movies",
        Shows = "shows",
        Upcoming = "upcoming",
        Anime = "anime",
    }
}

wire_enum! {
    /// A title for this profile: the first of these that holds, in this order.
    pub enum TitleStatus or None {
        Library = "library",
        Restricted = "restricted",
        Requested = "requested",
        Declined = "declined",
        Partial = "partial",
        #[default]
        None = "none",
    }
}

wire_enum! {
    /// A season for this profile, by the server's own rule for a whole season.
    pub enum SeasonState or Missing {
        Library = "library",
        Partial = "partial",
        Requested = "requested",
        #[default]
        Missing = "missing",
        Unaired = "unaired",
    }
}

wire_enum! {
    /// Where a download is.
    pub enum DownloadState or Downloading {
        Queued = "queued",
        #[default]
        Downloading = "downloading",
        Paused = "paused",
        Importing = "importing",
        Warning = "warning",
    }
}

/// What the sign-in predates: `requests` to ask at all, `approvals` to decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInAgain {
    Requests,
    Approvals,
}

/// What requests are for this sign-in (`GET /api/requests/summary`, any app may ask).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestSummary {
    /// Requests are on, and TMDB has a key.
    #[serde(default, deserialize_with = "nd")]
    pub enabled: bool,
    #[serde(default, deserialize_with = "nd")]
    pub access: RequestAccess,
    /// What may still be asked for in the last 7 days; `None`: no limit.
    #[serde(default)]
    pub movies_left: Option<i64>,
    #[serde(default)]
    pub seasons_left: Option<i64>,
    #[serde(default)]
    pub movies_limit: Option<i64>,
    #[serde(default)]
    pub seasons_limit: Option<i64>,
    /// None left: when one more may be asked for (unix seconds).
    #[serde(default)]
    pub movies_next_at: Option<i64>,
    #[serde(default)]
    pub seasons_next_at: Option<i64>,
    /// This sign-in approves and declines (an administrator's, with `approvals`).
    #[serde(default, deserialize_with = "nd")]
    pub manage: bool,
    /// For managers: requests waiting for a decision, and failed ones the server won't try again.
    #[serde(default, deserialize_with = "nd")]
    pub pending: i64,
    #[serde(default, deserialize_with = "nd")]
    pub failed: i64,
    /// This profile's requests (and those it follows) with news since Your requests was seen.
    #[serde(default, deserialize_with = "nd")]
    pub unseen: i64,
    /// This profile's requests and those it follows, any status.
    #[serde(default, deserialize_with = "nd")]
    pub own: i64,
    /// The newest change among the requests this sign-in sees.
    #[serde(default)]
    pub changed_at: Option<i64>,
    #[serde(default)]
    pub sign_in_again: Option<SignInAgain>,
}

impl RequestSummary {
    /// The profile may ask for titles.
    pub fn may_ask(&self) -> bool {
        self.enabled && self.access != RequestAccess::Off && self.sign_in_again != Some(SignInAgain::Requests)
    }

    /// The rail's number: news since last seen, and for managers what waits for them.
    pub fn badge(&self) -> i64 {
        self.unseen + if self.manage { self.pending + self.failed } else { 0 }
    }
}

/// A movie or show on TMDB, as Discover lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoverTitle {
    #[serde(default, deserialize_with = "nd")]
    pub kind: TitleKind,
    #[serde(default, deserialize_with = "nd")]
    pub tmdb_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub overview: String,
    /// Server paths (`/api/discover/images/…`), sized with `w` as library artwork is.
    #[serde(default)]
    pub poster_url: Option<String>,
    #[serde(default)]
    pub backdrop_url: Option<String>,
    /// TMDB users', 0–10; `None` with too few votes.
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default, deserialize_with = "nd")]
    pub status: TitleStatus,
    /// In a library this profile can open.
    #[serde(default)]
    pub item_id: Option<i64>,
    /// An active request for it this profile made or follows (any, for managers).
    #[serde(default)]
    pub request_id: Option<i64>,
}

/// One page of a Discover list or of TMDB's search.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoverPage {
    #[serde(default, deserialize_with = "nd")]
    pub page: i64,
    /// 0 to 500; a page may hold fewer than 20 (even none) before the last.
    #[serde(default, deserialize_with = "nd")]
    pub total_pages: i64,
    #[serde(default, deserialize_with = "nd")]
    pub results: Vec<DiscoverTitle>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CastMember {
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub photo_url: Option<String>,
}

/// A show's season on TMDB, for this profile.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoverSeason {
    /// 0: specials.
    #[serde(default, deserialize_with = "nd")]
    pub number: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    #[serde(default, deserialize_with = "nd")]
    pub episodes: i64,
    #[serde(default, deserialize_with = "nd")]
    pub aired: i64,
    #[serde(default)]
    pub air_date: Option<String>,
    #[serde(default)]
    pub poster_url: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub in_library: i64,
    #[serde(default, deserialize_with = "nd")]
    pub requested: bool,
    /// What the season is for this profile: drawn from this, never from the counts.
    #[serde(default, deserialize_with = "nd")]
    pub state: SeasonState,
    /// A request for it this profile made or followed was declined in the last 30 days.
    #[serde(default, deserialize_with = "nd")]
    pub declined: bool,
    #[serde(default)]
    pub request_id: Option<i64>,
}

/// This profile's declined request for a title, within its 30 days.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Declined {
    /// `None` for a follower.
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub at: i64,
    #[serde(default, deserialize_with = "nd")]
    pub again_at: i64,
}

/// A movie's or show's page (`GET /api/discover/{kind}/{tmdb_id}`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscoverDetail {
    #[serde(flatten)]
    pub title: DiscoverTitle,
    #[serde(default)]
    pub original_title: Option<String>,
    #[serde(default)]
    pub tagline: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub genres: Vec<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub runtime_minutes: Option<i64>,
    #[serde(default)]
    pub certification: Option<String>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub show_status: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub anime: bool,
    #[serde(default, deserialize_with = "nd")]
    pub cast: Vec<CastMember>,
    /// Shows, in TMDB's order; `None` for movies.
    #[serde(default)]
    pub seasons: Option<Vec<DiscoverSeason>>,
    #[serde(default)]
    pub imdb_id: Option<String>,
    #[serde(default)]
    pub declined: Option<Declined>,
    /// Why it can't be asked for ("Rated R: above this account's age limit").
    #[serde(default)]
    pub restriction: Option<String>,
    /// This profile follows an active request for it.
    #[serde(default, deserialize_with = "nd")]
    pub watching: bool,
    /// This profile's and those it follows (everyone's, for managers), newest first.
    #[serde(default, deserialize_with = "nd")]
    pub requests: Vec<MediaRequest>,
    #[serde(default)]
    pub movies_left: Option<i64>,
    #[serde(default)]
    pub seasons_left: Option<i64>,
    #[serde(default)]
    pub movies_next_at: Option<i64>,
    #[serde(default)]
    pub seasons_next_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestUser {
    #[serde(default, deserialize_with = "nd")]
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub avatar_color: Option<String>,
}

/// A request's download in Radarr or Sonarr.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestDownload {
    /// 0–1, over everything queued for it.
    #[serde(default, deserialize_with = "nd")]
    pub progress: f64,
    #[serde(default, deserialize_with = "nd")]
    pub state: DownloadState,
    #[serde(default)]
    pub eta: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub episodes: i64,
}

/// Where an approved request went.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SentTo {
    #[serde(default, deserialize_with = "nd")]
    pub service: String,
    #[serde(default)]
    pub quality_profile: Option<String>,
    #[serde(default)]
    pub root_folder: Option<String>,
}

/// A request.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaRequest {
    #[serde(default, deserialize_with = "nd")]
    pub id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub kind: TitleKind,
    #[serde(default, deserialize_with = "nd")]
    pub tmdb_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub title: String,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub poster_url: Option<String>,
    #[serde(default)]
    pub backdrop_url: Option<String>,
    /// Shows: the seasons asked for; `None` for movies.
    #[serde(default)]
    pub seasons: Option<Vec<i64>>,
    /// Shows: those ready.
    #[serde(default, deserialize_with = "nd")]
    pub available_seasons: Vec<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub anime: bool,
    #[serde(default, deserialize_with = "nd")]
    pub status: RequestStatus,
    #[serde(default, deserialize_with = "nd")]
    pub stage: RequestStage,
    /// Waiting: when it comes out.
    #[serde(default)]
    pub stage_at: Option<i64>,
    /// Declined: the administrator's (none for followers); failed: what went wrong, for managers.
    #[serde(default)]
    pub reason: Option<String>,
    /// For managers: what to know about it.
    #[serde(default)]
    pub note: Option<String>,
    /// This profile made it.
    #[serde(default, deserialize_with = "nd")]
    pub mine: bool,
    /// This profile follows it.
    #[serde(default, deserialize_with = "nd")]
    pub watching: bool,
    #[serde(default, deserialize_with = "nd")]
    pub watchers: i64,
    /// It became available, partly available or declined since this profile last looked.
    #[serde(default, deserialize_with = "nd")]
    pub unseen: bool,
    #[serde(default, deserialize_with = "nd")]
    pub actions: Vec<RequestAction>,
    /// `None` for someone else's request seen by someone who doesn't manage.
    #[serde(default)]
    pub requested_by: Option<RequestUser>,
    #[serde(default)]
    pub decided_by: Option<RequestUser>,
    #[serde(default)]
    pub decided_at: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub created_at: i64,
    #[serde(default, deserialize_with = "nd")]
    pub updated_at: i64,
    #[serde(default)]
    pub available_at: Option<i64>,
    /// Failed: when the server tries again by itself; `None`: it won't.
    #[serde(default)]
    pub retry_at: Option<i64>,
    #[serde(default)]
    pub sent_to: Option<SentTo>,
    /// The library's title, where this profile can open it.
    #[serde(default)]
    pub item_id: Option<i64>,
    #[serde(default)]
    pub download: Option<RequestDownload>,
}

impl MediaRequest {
    /// Whether this sign-in may do `action` to it now.
    pub fn can(&self, action: RequestAction) -> bool {
        self.actions.contains(&action)
    }
}

/// How many requests have each status, every filter but the status applied.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestCounts {
    #[serde(default, deserialize_with = "nd")]
    pub pending: i64,
    #[serde(default, deserialize_with = "nd")]
    pub approved: i64,
    #[serde(default, deserialize_with = "nd")]
    pub declined: i64,
    #[serde(default, deserialize_with = "nd")]
    pub failed: i64,
    #[serde(default, deserialize_with = "nd")]
    pub available: i64,
}

/// `GET /api/requests`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RequestList {
    #[serde(default, deserialize_with = "nd")]
    pub total: i64,
    #[serde(default, deserialize_with = "nd")]
    pub counts: RequestCounts,
    #[serde(default, deserialize_with = "nd")]
    pub requests: Vec<MediaRequest>,
    /// For managers: everyone with a request matching every filter but the person, by name.
    #[serde(default, deserialize_with = "nd")]
    pub people: Vec<RequestUser>,
}

/// A library title's place in requests (`GET /api/items/{id}/request`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemRequest {
    #[serde(default, deserialize_with = "nd")]
    pub kind: TitleKind,
    #[serde(default, deserialize_with = "nd")]
    pub tmdb_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub status: TitleStatus,
    /// The newest active request for it this profile made or follows (any, for managers).
    #[serde(default)]
    pub request: Option<MediaRequest>,
    /// The seasons the picker would tick with no limit (empty for movies); `None` while the
    /// server has no TMDB details for it.
    #[serde(default)]
    pub askable: Option<Vec<i64>>,
}

/// `POST /api/requests`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewRequest {
    pub kind: TitleKind,
    pub tmdb_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seasons: Option<Vec<i64>>,
}

/// `POST /api/requests/{id}/decline`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct DeclineBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Which requests to list (`GET /api/requests`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestQuery {
    pub status: Vec<RequestStatus>,
    /// Managers: pending ones, and failed ones the server won't try again by itself.
    pub attention: bool,
    pub kind: Option<TitleKind>,
    /// Managers: one person's.
    pub user_id: Option<i64>,
    /// The title has these words.
    pub q: Option<String>,
    /// Oldest first.
    pub oldest: bool,
    /// Managers: only this profile's and those it follows.
    pub mine: bool,
    /// Only those with news since Your requests was last seen, newest news first.
    pub unseen: bool,
    pub changed_since: Option<i64>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

impl RequestQuery {
    /// The query's parameters; what isn't set is left out.
    pub fn params(&self) -> Vec<(&'static str, Option<String>)> {
        let flag = |on: bool| on.then(|| "1".to_string());
        let status =
            (!self.status.is_empty()).then(|| self.status.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","));
        vec![
            ("status", status),
            ("attention", flag(self.attention)),
            ("kind", self.kind.map(|k| k.as_str().to_string())),
            ("user_id", self.user_id.map(|v| v.to_string())),
            ("q", self.q.clone().filter(|q| !q.is_empty())),
            ("order", self.oldest.then(|| "oldest".to_string())),
            ("mine", flag(self.mine)),
            ("unseen", flag(self.unseen)),
            ("changed_since", self.changed_since.map(|v| v.to_string())),
            ("offset", self.offset.map(|v| v.to_string())),
            ("limit", self.limit.map(|v| v.to_string())),
        ]
    }
}
