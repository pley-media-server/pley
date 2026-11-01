//! Requests: people ask for movies and shows the libraries don't have,
//! found on TMDB ([`crate::discover`]). Managers (administrators, from
//! pley's own sign-in or apps allowed `approvals`) approve or decline them
//! (administrators' own, and those of accounts allowed to, are approved as
//! made); approved ones go to Radarr or Sonarr ([`arr`]) where they are
//! connected, and a request is available once a library has it: the movie,
//! or every season asked for, whole. See docs/API.md, Requests.

pub mod arr;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqliteConnection;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::accounts::avatar_url;
use crate::auth::scopes::Scope;
use crate::auth::{AuthUser, hash_token};
use crate::catalog::{ME_CTE, VISIBLE};
use crate::db::Db;
use crate::discover::pictures::{self, Picture, Pictures, Unavailable};
use crate::discover::{self, CastMember, Details, Discover, Found, List, TitleKind};
use crate::error::AppError;
use crate::events::{Audience, EventBus, ServerEvent};
use crate::metadata::{MetadataAgent, TmdbError};
use crate::ratings::rating_age;
use crate::settings::{Settings, SettingsStore};
use crate::store::like_escape;
use crate::util::{now, utc};
use arr::{Arr, ArrError, Connection, Download, DownloadState, Lists, Profile, RootFolder, Service};

/// What the limits count: requests of the last 7 days.
pub const LIMIT_WINDOW: i64 = 7 * 86_400;
/// How long a declined request keeps its title from being asked for again
/// by the account that made or followed it.
pub const DECLINED_FOR: i64 = 30 * 86_400;
/// How often the libraries are looked at for what is asked for (and after
/// every scan).
const SYNC_EVERY: Duration = Duration::from_secs(300);
/// Statuses of requests still asking for their title.
const ACTIVE: &str = "('pending', 'approved', 'failed')";
/// Requests of `r` an account made or follows (binds the account's id
/// twice).
const MINE: &str =
    "(r.user_id = ? OR EXISTS (SELECT 1 FROM request_watchers w WHERE w.request_id = r.id AND w.user_id = ?))";
/// How long a library title's page waits for TMDB.
const ITEM_WAIT: Duration = Duration::from_secs(2);
/// Requests listed at once, by default and at most.
const LIST_LIMIT: i64 = 50;
const LIST_MAX: i64 = 200;
/// A reason given for declining, and a root folder chosen, at most.
const REASON_MAX: usize = 500;
const FOLDER_MAX: usize = 512;
/// Titles whose details (or other ids) are looked up at once.
const LOOKUPS: usize = 8;
/// How many times asking looks at a title whose requests change meanwhile.
const ASK_LOOKS: usize = 3;
/// How long what Radarr and Sonarr offer approvals is reused, and how long
/// an answer waits for them after that.
const OFFER_TTL: Duration = Duration::from_secs(300);
const OFFER_WAIT: Duration = Duration::from_secs(3);
/// What managers read of a request added by hand.
const HAND_NOTE: &str = "Added by hand";

/// Whether an account may ask for titles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestAccess {
    /// May not ask.
    #[default]
    Off,
    /// Asks an administrator.
    Ask,
    /// Approved as asked.
    Auto,
}

impl RequestAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            RequestAccess::Off => "off",
            RequestAccess::Ask => "ask",
            RequestAccess::Auto => "auto",
        }
    }

    /// As stored (anything else asks).
    pub fn parse(text: &str) -> RequestAccess {
        match text {
            "off" => RequestAccess::Off,
            "auto" => RequestAccess::Auto,
            _ => RequestAccess::Ask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestStatus {
    Pending,
    Approved,
    Declined,
    Failed,
    Available,
}

impl RequestStatus {
    pub const ALL: [RequestStatus; 5] = [
        RequestStatus::Pending,
        RequestStatus::Approved,
        RequestStatus::Declined,
        RequestStatus::Failed,
        RequestStatus::Available,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RequestStatus::Pending => "pending",
            RequestStatus::Approved => "approved",
            RequestStatus::Declined => "declined",
            RequestStatus::Failed => "failed",
            RequestStatus::Available => "available",
        }
    }

    pub fn parse(text: &str) -> Option<RequestStatus> {
        RequestStatus::ALL.into_iter().find(|s| s.as_str() == text)
    }
}

/// Where a request is: its decision, then for an approved one how far
/// Radarr or Sonarr is with it (docs/API.md, Requests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestStage {
    Pending,
    Declined,
    Failed,
    /// Approved, not sent: no Radarr or Sonarr for its kind, sending, or
    /// added by hand.
    Approved,
    /// Sent; nothing downloading yet.
    Searching,
    Downloading,
    Importing,
    /// Shows: some of the seasons asked for are ready.
    Partial,
    Available,
    /// Was available; no library has it any more.
    Gone,
}

/// What a request endpoint answers when it can't, as `{ code, message, … }`
/// (docs/API.md, Requests: errors).
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    /// `503 unavailable`, `setup: false`.
    #[error("requests are off: an administrator can turn them on in Settings → Requests")]
    Off,
    /// `503 tmdb_key`, `setup: true`, `retry: false`.
    #[error("TMDB refused this server's key: an administrator can check it in Settings → Server")]
    TmdbKey,
    /// `503 tmdb_down`, `setup: true`.
    #[error("TMDB isn't answering: try again in a moment")]
    TmdbDown,
    /// `403 no_access`: the account may not ask.
    #[error("requests aren't open to this account: an administrator can open them in Settings → Users")]
    NoAccess,
    /// `403 restricted`: above the account's age limit, saying why.
    #[error("{0}")]
    Restricted(String),
    /// `409 in_library`: the libraries the account can open have it all.
    #[error("{0}")]
    InLibrary(String),
    /// `409 declined`: declined for this account lately; `retry_at`: when
    /// it may ask again.
    #[error("{message}")]
    Declined { message: String, retry_at: i64 },
    /// `429 too_many_requests` (and `Retry-After`): past the limit;
    /// `retry_at`: when enough of what counts stops counting for all that
    /// was asked, `None` when that is more than the limit itself.
    #[error("{message}")]
    Limit { message: String, retry_at: Option<i64> },
    /// `409 conflict`: what was asked can't be done to the request as it is
    /// now (`request`); `retry_at`: when it may be (a declined request its
    /// maker may remove once its 30 days are over).
    #[error("{message}")]
    Conflict { message: String, request: Box<MediaRequest>, retry_at: Option<i64> },
    /// `502`: TMDB's picture server didn't answer, or not with a picture.
    #[error("TMDB's pictures aren't answering: try again in a moment")]
    PictureDown,
    #[error(transparent)]
    App(#[from] AppError),
}

pub type RequestResult<T> = Result<T, RequestError>;

impl From<sqlx::Error> for RequestError {
    fn from(err: sqlx::Error) -> Self {
        RequestError::App(err.into())
    }
}

impl From<TmdbError> for RequestError {
    fn from(err: TmdbError) -> Self {
        match err {
            TmdbError::NotFound => RequestError::App(AppError::not_found("TMDB doesn't know this title")),
            TmdbError::Unauthorized => RequestError::TmdbKey,
            TmdbError::Other(err) => {
                tracing::warn!("TMDB: {err:#}");
                RequestError::TmdbDown
            }
        }
    }
}

impl IntoResponse for RequestError {
    fn into_response(self) -> Response {
        let message = self.to_string();
        let (status, body) = match self {
            RequestError::App(err) => return err.into_response(),
            RequestError::Off => {
                (StatusCode::SERVICE_UNAVAILABLE, json!({ "code": "unavailable", "message": message, "setup": false }))
            }
            RequestError::TmdbKey => (
                StatusCode::SERVICE_UNAVAILABLE,
                json!({ "code": "tmdb_key", "message": message, "setup": true, "retry": false }),
            ),
            RequestError::TmdbDown => {
                (StatusCode::SERVICE_UNAVAILABLE, json!({ "code": "tmdb_down", "message": message, "setup": true }))
            }
            RequestError::NoAccess => (StatusCode::FORBIDDEN, json!({ "code": "no_access", "message": message })),
            RequestError::Restricted(_) => (StatusCode::FORBIDDEN, json!({ "code": "restricted", "message": message })),
            RequestError::InLibrary(_) => (StatusCode::CONFLICT, json!({ "code": "in_library", "message": message })),
            RequestError::Declined { retry_at, .. } => {
                (StatusCode::CONFLICT, json!({ "code": "declined", "message": message, "retry_at": retry_at }))
            }
            RequestError::Limit { retry_at, .. } => {
                let body = json!({ "code": "too_many_requests", "message": message, "retry_at": retry_at });
                let mut response = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
                if let Some(at) = retry_at {
                    response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from((at - now()).max(1)));
                }
                return response;
            }
            RequestError::Conflict { request, retry_at, .. } => (
                StatusCode::CONFLICT,
                json!({ "code": "conflict", "message": message, "request": request, "retry_at": retry_at }),
            ),
            RequestError::PictureDown => {
                let body = json!({ "code": "tmdb_down", "message": message });
                let headers = [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))];
                return (StatusCode::BAD_GATEWAY, headers, Json(body)).into_response();
            }
        };
        (status, Json(body)).into_response()
    }
}

// ------------------------------------------------------------------ what the API shows

/// Who asks, and what their session may do with requests: what asking
/// does, whether the settings' limits apply, and whether it manages
/// requests (see [`Requests::caller`]).
#[derive(Debug, Clone, Copy)]
pub struct Caller<'a> {
    pub user: &'a AuthUser,
    pub access: RequestAccess,
    /// The settings' limits apply (never to an administrator's own
    /// requests).
    pub limited: bool,
    /// Approves and declines, sees and deletes everyone's requests
    /// ([`AuthUser::manages_requests`]).
    pub manager: bool,
}

/// `GET /api/requests/summary`: what this session may do with requests,
/// and what there is to see.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Summary {
    pub enabled: bool,
    pub access: RequestAccess,
    pub movies_left: Option<i64>,
    pub seasons_left: Option<i64>,
    /// The limits that apply, per 7 days.
    pub movies_limit: Option<i64>,
    pub seasons_limit: Option<i64>,
    /// With none left of a kind: when one more may be asked for.
    pub movies_next_at: Option<i64>,
    pub seasons_next_at: Option<i64>,
    /// This session manages requests.
    pub manage: bool,
    /// Managers: requests waiting for a decision.
    pub pending: i64,
    /// Managers: failed requests, which wait for one.
    pub failed: i64,
    /// Requests the account made or follows that became available, partly
    /// available or declined since it last looked.
    pub unseen: i64,
    /// Requests the account made or follows.
    pub own: i64,
    /// When the newest change was among the requests this session sees.
    pub changed_at: Option<i64>,
    /// The scope an app should sign in again for: `requests` to ask at
    /// all, `approvals` to manage (an administrator's, on pley's own apps).
    pub sign_in_again: Option<&'static str>,
}

/// A title for one account, the first of these that holds (docs/API.md,
/// Discover): in a library it can open, above its age limit, asked for
/// (all it lacks), declined for it lately, partly there or asked for, or
/// none of these (it may be asked for).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleStatus {
    Library,
    Restricted,
    Requested,
    Declined,
    Partial,
    None,
}

/// A season of a show for one account, the first that holds: whole in a
/// library it can open, asked for by an active request, nothing aired yet,
/// some of what aired there, none of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SeasonState {
    Library,
    Requested,
    Unaired,
    Partial,
    Missing,
}

/// A request this account made or followed that was declined lately: why
/// (its maker sees the reason), when, and when it may be asked for again.
#[derive(Debug, Clone, Serialize)]
pub struct Declined {
    pub reason: Option<String>,
    pub at: i64,
    pub again_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverTitle {
    pub kind: TitleKind,
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub overview: String,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub rating: Option<f64>,
    pub status: TitleStatus,
    pub item_id: Option<i64>,
    pub request_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverPage {
    pub page: u32,
    pub total_pages: u32,
    pub results: Vec<DiscoverTitle>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverDetail {
    #[serde(flatten)]
    pub title: DiscoverTitle,
    pub original_title: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub release_date: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    pub network: Option<String>,
    pub show_status: Option<String>,
    pub anime: bool,
    pub imdb_id: Option<String>,
    pub cast: Vec<CastMember>,
    pub seasons: Option<Vec<DiscoverSeason>>,
    /// The newest request for it this account made or followed that was
    /// declined within [`DECLINED_FOR`].
    pub declined: Option<Declined>,
    /// Why this account can't ask for it (its age limit).
    pub restriction: Option<String>,
    /// This account follows an active request for it.
    pub watching: bool,
    /// This account's and those it follows (everyone's, for managers),
    /// newest first.
    pub requests: Vec<MediaRequest>,
    pub movies_left: Option<i64>,
    pub seasons_left: Option<i64>,
    pub movies_next_at: Option<i64>,
    pub seasons_next_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoverSeason {
    pub number: i64,
    pub name: String,
    pub episodes: i64,
    pub aired: i64,
    pub air_date: Option<String>,
    pub poster_url: Option<String>,
    /// Episodes of it in the libraries this account can open.
    pub in_library: i64,
    /// An active request asks for it.
    pub requested: bool,
    pub state: SeasonState,
    /// A request for it this account made or followed was declined within
    /// [`DECLINED_FOR`].
    pub declined: bool,
    /// The active request asking for it, where this account made or follows
    /// it (any, for managers).
    pub request_id: Option<i64>,
}

/// `GET /api/items/{id}/request`: what a library title's page shows of
/// requests for it.
#[derive(Debug, Clone, Serialize)]
pub struct ItemRequest {
    pub kind: TitleKind,
    pub tmdb_id: i64,
    pub status: TitleStatus,
    /// The newest active request for it this account made or follows (any,
    /// for managers).
    pub request: Option<MediaRequest>,
    /// The seasons the picker would offer first with no limit (`[]` for
    /// movies); `None` while TMDB's details of it aren't known.
    pub askable: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaRequest {
    pub id: i64,
    pub kind: TitleKind,
    pub tmdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub seasons: Option<Vec<i64>>,
    pub available_seasons: Vec<i64>,
    pub anime: bool,
    pub status: RequestStatus,
    pub stage: RequestStage,
    /// Always `None`: no stage says when it ends.
    pub stage_at: Option<i64>,
    /// Declined: the administrator's, for its maker and managers. Failed:
    /// what went wrong, for managers.
    pub reason: Option<String>,
    /// Managers: what to know of it.
    pub note: Option<String>,
    /// The account made it.
    pub mine: bool,
    /// The account follows it.
    pub watching: bool,
    /// Managers: how many follow it.
    pub watchers: i64,
    /// Its maker's or a follower's: it became available, partly available
    /// or declined since the account last looked.
    pub unseen: bool,
    /// What this session may do to it now, in the order clients list them.
    pub actions: Vec<RequestAction>,
    /// Its maker and managers: who asked.
    pub requested_by: Option<RequestUser>,
    /// Its maker and managers: who approved or declined it.
    pub decided_by: Option<RequestUser>,
    pub decided_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub available_at: Option<i64>,
    /// Always `None`: a failed request waits for a manager.
    pub retry_at: Option<i64>,
    /// Managers: where it was sent.
    pub sent_to: Option<SentTo>,
    pub item_id: Option<i64>,
    pub download: Option<Download>,
}

/// What a session may do to a request (docs/API.md, Requests: actions), in
/// the order clients list them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestAction {
    /// Its maker: `DELETE /api/requests/{id}` (Remove, on a declined one).
    Withdraw,
    /// A follower: `DELETE /api/requests/{id}/watch`.
    Unwatch,
    Approve,
    /// A failed one: `POST /api/requests/{id}/approve`.
    Retry,
    Decline,
    /// `POST /api/requests/{id}/handled`.
    Handled,
    /// Managers: `DELETE /api/requests/{id}`, for everyone.
    Delete,
    /// `POST /api/requests` with its kind, TMDB id and seasons.
    AskAgain,
}

/// Where an approved request went, and with what: the quality profile's
/// name and the root folder.
#[derive(Debug, Clone, Serialize)]
pub struct SentTo {
    pub service: Service,
    pub quality_profile: Option<String>,
    pub root_folder: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestUser {
    pub id: i64,
    pub name: String,
    pub avatar_url: Option<String>,
    pub avatar_color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestList {
    pub total: i64,
    /// Of every status, with the filters but `status`.
    pub counts: BTreeMap<RequestStatus, i64>,
    pub requests: Vec<MediaRequest>,
    /// Managers: who made the requests the filters but `user_id` keep, by
    /// name (none for others).
    pub people: Vec<RequestUser>,
}

/// `GET /api/requests/options`: what approving may choose, for Radarr and
/// Sonarr where each is connected.
#[derive(Debug, Clone, Serialize)]
pub struct RequestOptions {
    pub radarr: Option<ArrOptions>,
    pub sonarr: Option<ArrOptions>,
}

/// What one of them offers, and the settings' choices (anime's own too).
#[derive(Debug, Clone, Serialize)]
pub struct ArrOptions {
    pub quality_profiles: Vec<Profile>,
    pub root_folders: Vec<RootFolder>,
    pub quality_profile: Option<i64>,
    pub root_folder: Option<String>,
    pub anime_quality_profile: Option<i64>,
    pub anime_root_folder: Option<String>,
    /// It didn't answer: the lists are the last it gave (none, if never).
    pub error: Option<String>,
}

/// `POST /api/requests`.
#[derive(Debug, Clone, Deserialize)]
pub struct NewRequest {
    pub kind: TitleKind,
    pub tmdb_id: i64,
    #[serde(default)]
    pub seasons: Option<Vec<i64>>,
}

/// What an approval may choose instead of the settings, and the seasons it
/// keeps of those asked for.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Approval {
    #[serde(default)]
    pub quality_profile: Option<i64>,
    #[serde(default)]
    pub root_folder: Option<String>,
    #[serde(default)]
    pub seasons: Option<Vec<i64>>,
}

/// Which requests `GET /api/requests` lists.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    /// Any of these (none: every status).
    pub statuses: Vec<RequestStatus>,
    /// Pending and failed ones (the managers' badge).
    pub attention: bool,
    pub kind: Option<TitleKind>,
    /// Managers: those this account made.
    pub user_id: Option<i64>,
    /// Titles containing it.
    pub q: Option<String>,
    /// Oldest first (newest by default).
    pub oldest: bool,
    /// Managers: only their own and those they follow.
    pub mine: bool,
    /// Only those with news unseen, newest news first.
    pub unseen: bool,
    /// Only those changed after it.
    pub changed_since: Option<i64>,
    pub offset: i64,
    pub limit: Option<i64>,
}

// ------------------------------------------------------------------ rows

const COLUMNS: &str = "r.id, r.kind, r.tmdb_id, r.tvdb_id, r.imdb_id, r.title, r.year, r.poster_path,
    r.backdrop_path, r.seasons, r.available_seasons, r.anime, r.user_id, r.status, r.reason, r.decided_by,
    r.decided_at, r.item_id, r.arr_id, r.created_at, r.updated_at, r.available_at, r.sent_to, r.sent_profile,
    r.sent_folder, r.sending_at, r.send_profile, r.send_folder, r.news_at, r.inherited_at,
    COALESCE(NULLIF(TRIM(u.display_name), ''), u.username) AS maker_name, u.avatar AS maker_avatar,
    u.avatar_color AS maker_color, COALESCE(NULLIF(TRIM(d.display_name), ''), d.username) AS decider_name,
    d.avatar AS decider_avatar, d.avatar_color AS decider_color";
const TABLES: &str = "requests r JOIN users u ON u.id = r.user_id LEFT JOIN users d ON d.id = r.decided_by";

#[derive(Debug, Clone, sqlx::FromRow)]
struct RequestRow {
    id: i64,
    kind: String,
    tmdb_id: i64,
    tvdb_id: Option<i64>,
    imdb_id: Option<String>,
    title: String,
    year: Option<i64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    seasons: Option<String>,
    available_seasons: String,
    anime: bool,
    user_id: i64,
    status: String,
    reason: Option<String>,
    decided_by: Option<i64>,
    decided_at: Option<i64>,
    item_id: Option<i64>,
    arr_id: Option<i64>,
    created_at: i64,
    updated_at: i64,
    available_at: Option<i64>,
    /// 'radarr', 'sonarr', or 'hand' (added by hand: never sent).
    sent_to: Option<String>,
    sent_profile: Option<String>,
    sent_folder: Option<String>,
    /// Being sent since then, with what its approval chose: the quality
    /// profile's id and the root folder.
    sending_at: Option<i64>,
    send_profile: Option<i64>,
    send_folder: Option<String>,
    news_at: Option<i64>,
    /// When its maker, its earliest follower then, became its maker.
    inherited_at: Option<i64>,
    maker_name: String,
    maker_avatar: Option<String>,
    maker_color: Option<String>,
    decider_name: Option<String>,
    decider_avatar: Option<String>,
    decider_color: Option<String>,
}

impl RequestRow {
    fn kind(&self) -> TitleKind {
        TitleKind::parse(&self.kind).unwrap_or(TitleKind::Movie)
    }

    fn status(&self) -> RequestStatus {
        RequestStatus::parse(&self.status).unwrap_or(RequestStatus::Pending)
    }

    fn seasons(&self) -> Option<Vec<i64>> {
        self.seasons.as_deref().map(seasons_of)
    }

    fn known(&self) -> Known {
        Known { tmdb_id: self.tmdb_id, tvdb_id: self.tvdb_id, imdb_id: self.imdb_id.clone() }
    }

    /// Sent to Radarr or Sonarr (not added by hand).
    fn sent(&self) -> Option<Service> {
        [Service::Radarr, Service::Sonarr].into_iter().find(|s| self.sent_to.as_deref() == Some(s.as_str()))
    }

    /// Added by hand: never sent.
    fn hand(&self) -> bool {
        self.sent_to.as_deref() == Some("hand")
    }

    /// Approved and being sent to Radarr or Sonarr.
    fn sending(&self) -> bool {
        self.status() == RequestStatus::Approved && self.sent_to.is_none() && self.sending_at.is_some()
    }

    /// Where its kind goes.
    fn service(&self) -> Service {
        match self.kind() {
            TitleKind::Movie => Service::Radarr,
            TitleKind::Show => Service::Sonarr,
        }
    }

    /// Decided while its maker followed it, before inheriting it: why, and
    /// by whom, were a follower's to know.
    fn decided_before_inherited(&self) -> bool {
        self.inherited_at.zip(self.decided_at).is_some_and(|(inherited, decided)| decided <= inherited)
    }

    /// Declined: when its maker may remove it, and ask again.
    fn declined_until(&self) -> Option<i64> {
        self.decided_at.filter(|_| self.status() == RequestStatus::Declined).map(|at| at + DECLINED_FOR)
    }

    fn maker(&self) -> RequestUser {
        RequestUser {
            id: self.user_id,
            name: self.maker_name.clone(),
            avatar_url: avatar_url(self.user_id, self.maker_avatar.as_deref()),
            avatar_color: self.maker_color.clone(),
        }
    }

    /// Where it is: an approved one's from its download, the seasons ready
    /// and where it went.
    fn stage(&self, download: Option<&Download>) -> RequestStage {
        match self.status() {
            RequestStatus::Pending => RequestStage::Pending,
            RequestStatus::Declined => RequestStage::Declined,
            RequestStatus::Failed => RequestStage::Failed,
            // Its library title went (`item_id` goes with it).
            RequestStatus::Available if self.item_id.is_none() => RequestStage::Gone,
            RequestStatus::Available => RequestStage::Available,
            RequestStatus::Approved => match download.map(|d| d.state) {
                Some(DownloadState::Importing) => RequestStage::Importing,
                Some(_) => RequestStage::Downloading,
                None if !seasons_of(&self.available_seasons).is_empty() => RequestStage::Partial,
                // Sent: Radarr or Sonarr looks for it.
                None if self.sent().is_some() => RequestStage::Searching,
                None => RequestStage::Approved,
            },
        }
    }

    /// What `viewer` may do to it now, in [`RequestAction`]'s order: its
    /// maker withdraws it while pending or failed, or a declined one once
    /// its 30 days are over (managers delete instead); a follower stops
    /// following, a declined one too once its 30 days are over; managers
    /// approve one pending, declined, or approved and not sent (where its
    /// service is connected), try a failed one again, decline one not
    /// available yet, mark one as added by hand; an account that may ask
    /// asks again for one gone from the library, or its own declined one
    /// after the 30 days.
    fn actions(&self, viewer: &Viewer, stage: RequestStage, mine: bool, watching: bool) -> Vec<RequestAction> {
        use RequestStatus::{Approved, Declined, Failed, Pending};
        let status = self.status();
        let manager = viewer.manager;
        let unsent = status == Approved && self.sent().is_none() && !self.sending();
        let over = status == Declined && self.declined_until().is_none_or(|at| at <= viewer.now);
        let lately = status == Declined && !over;
        [
            (RequestAction::Withdraw, mine && !manager && (matches!(status, Pending | Failed) || over)),
            (RequestAction::Unwatch, watching && !lately),
            (
                RequestAction::Approve,
                manager
                    && (matches!(status, Pending | Declined) || (unsent && viewer.connected.contains(&self.service()))),
            ),
            (RequestAction::Retry, manager && status == Failed),
            (RequestAction::Decline, manager && matches!(status, Pending | Failed | Approved)),
            (
                RequestAction::Handled,
                manager && (matches!(status, Pending | Failed | Declined) || (unsent && !self.hand())),
            ),
            (RequestAction::Delete, manager),
            (
                RequestAction::AskAgain,
                viewer.asks && ((stage == RequestStage::Gone && (mine || watching)) || (mine && over)),
            ),
        ]
        .into_iter()
        .filter_map(|(action, may)| may.then_some(action))
        .collect()
    }

    /// As `viewer` sees it: who asked and who decided for its maker and
    /// managers, why it was declined for its maker too (but a decision made
    /// before its maker inherited it); what went wrong, where it went and
    /// what to know for managers; the library's title where the viewer may
    /// open it.
    fn to_dto(&self, viewer: &Viewer, download: Option<Download>) -> MediaRequest {
        let status = self.status();
        let stage = self.stage(download.as_ref());
        let mine = self.user_id == viewer.id;
        let watching = viewer.following.contains(&self.id);
        let theirs = mine || viewer.manager;
        let decision = viewer.manager || (mine && !self.decided_before_inherited());
        let reason = match status {
            RequestStatus::Declined if decision => self.reason.clone(),
            RequestStatus::Failed if viewer.manager => self.reason.clone(),
            _ => None,
        };
        MediaRequest {
            id: self.id,
            kind: self.kind(),
            tmdb_id: self.tmdb_id,
            title: self.title.clone(),
            year: self.year,
            poster_url: pictures::url(Picture::Poster, self.poster_path.as_deref()),
            backdrop_url: pictures::url(Picture::Backdrop, self.backdrop_path.as_deref()),
            seasons: self.seasons(),
            available_seasons: seasons_of(&self.available_seasons),
            anime: self.anime,
            status,
            stage,
            stage_at: None,
            reason,
            note: self.hand().then(|| HAND_NOTE.to_owned()).filter(|_| viewer.manager),
            mine,
            watching,
            watchers: if viewer.manager { viewer.followers.get(&self.id).copied().unwrap_or(0) } else { 0 },
            unseen: (mine || watching) && self.news_at.is_some_and(|at| at > viewer.seen_at),
            actions: self.actions(viewer, stage, mine, watching),
            requested_by: theirs.then(|| self.maker()),
            decided_by: self.decided_by.filter(|_| decision).zip(self.decider_name.clone()).map(|(id, name)| {
                RequestUser {
                    id,
                    name,
                    avatar_url: avatar_url(id, self.decider_avatar.as_deref()),
                    avatar_color: self.decider_color.clone(),
                }
            }),
            decided_at: self.decided_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            available_at: self.available_at,
            retry_at: None,
            sent_to: self.sent().filter(|_| viewer.manager).map(|service| SentTo {
                service,
                quality_profile: self.sent_profile.clone(),
                root_folder: self.sent_folder.clone(),
            }),
            item_id: self.item_id.filter(|id| viewer.visible.contains(id)),
            download,
        }
    }
}

/// Who reads requests, with what of the requests read is theirs to know,
/// and what they may do to them.
#[derive(Debug)]
struct Viewer {
    id: i64,
    manager: bool,
    /// The account may ask for titles.
    asks: bool,
    now: i64,
    /// When the account last looked at its requests' news.
    seen_at: i64,
    /// The requests' library titles the account may open.
    visible: HashSet<i64>,
    /// How many follow each request (those someone follows).
    followers: HashMap<i64, i64>,
    /// The requests the account follows.
    following: HashSet<i64>,
    /// Radarr and Sonarr, where connected.
    connected: Vec<Service>,
}

fn seasons_of(json: &str) -> Vec<i64> {
    serde_json::from_str(json).unwrap_or_default()
}

fn seasons_json(seasons: &[i64]) -> String {
    serde_json::to_string(seasons).unwrap_or_else(|_| "[]".to_owned())
}

/// What a title is known by: TMDB's id, and for shows TheTVDB's and IMDb's.
#[derive(Debug, Clone)]
struct Known {
    tmdb_id: i64,
    tvdb_id: Option<i64>,
    imdb_id: Option<String>,
}

impl Known {
    fn of(details: &Details) -> Known {
        Known { tmdb_id: details.tmdb_id, tvdb_id: details.tvdb_id, imdb_id: details.imdb_id.clone() }
    }

    fn tmdb(tmdb_id: i64) -> Known {
        Known { tmdb_id, tvdb_id: None, imdb_id: None }
    }
}

/// A library's title, with the ids metadata gave it.
#[derive(Debug, Clone, sqlx::FromRow)]
struct LibraryTitle {
    kind: String,
    tmdb_id: Option<i64>,
    tvdb_id: Option<i64>,
    imdb_id: Option<String>,
}

/// An active request for a title, as one account sees it.
#[derive(Debug, Clone)]
struct Active {
    id: i64,
    /// Shows: the seasons it asks for.
    seasons: Vec<i64>,
    /// The account made or follows it (or manages requests).
    theirs: bool,
    /// The account made it.
    mine: bool,
    /// The account follows it.
    follows: bool,
    /// It waits for a manager.
    pending: bool,
}

/// What asking did ([`Requests::ask`]): the seasons it wanted, the active
/// requests asking for some of them already, those it followed and
/// approved, the request it made, and whether those approved or made are
/// sent.
struct Asked {
    wanted: Vec<i64>,
    covering: Vec<Active>,
    follow: Vec<i64>,
    approved: Vec<i64>,
    made: Option<i64>,
    sends: bool,
}

/// What the libraries and requests have of a title, for one account.
#[derive(Debug, Clone, Default)]
struct Mark {
    /// The title in a library the account can open (the first found).
    item_id: Option<i64>,
    /// Shows: every library show the account can open that is this one.
    shows: Vec<i64>,
    /// Its active requests, whoever made them, newest first.
    active: Vec<Active>,
    /// The account's last request for it (made or followed) was declined
    /// within [`DECLINED_FOR`].
    declined: bool,
    /// Shows: the seasons of its requests declined within it, with when
    /// each may be asked for again.
    declined_seasons: HashMap<i64, i64>,
    /// The newest of those declines.
    refusal: Option<Declined>,
}

impl Mark {
    fn requested(&self) -> bool {
        !self.active.is_empty()
    }

    /// Shows: the seasons its active requests ask for.
    fn asked(&self) -> HashSet<i64> {
        self.active.iter().flat_map(|a| a.seasons.iter().copied()).collect()
    }

    /// The newest active request the account made or follows (any, for
    /// managers).
    fn request_id(&self) -> Option<i64> {
        self.active.iter().find(|a| a.theirs).map(|a| a.id)
    }

    /// The same, asking for season `n`.
    fn season_request(&self, n: i64) -> Option<i64> {
        self.active.iter().find(|a| a.theirs && a.seasons.contains(&n)).map(|a| a.id)
    }

    fn watching(&self) -> bool {
        self.active.iter().any(|a| a.follows)
    }
}

/// A title's status for one account (see [`TitleStatus`]). `lacking`: a
/// show's aired seasons but specials that no library it can open has whole,
/// from its details; `None` for movies, and shows whose details aren't
/// known (one in a library it can open is then `library`, one asked for
/// `requested`).
fn status_of(kind: TitleKind, mark: &Mark, lacking: Option<&[i64]>, restricted: bool) -> TitleStatus {
    let in_library = mark.item_id.is_some();
    let Some(lacking) = lacking.filter(|_| kind == TitleKind::Show) else {
        return if in_library {
            TitleStatus::Library
        } else if restricted {
            TitleStatus::Restricted
        } else if mark.requested() {
            TitleStatus::Requested
        } else if mark.declined {
            TitleStatus::Declined
        } else {
            TitleStatus::None
        };
    };
    let asked = mark.asked();
    let unasked: Vec<i64> = lacking.iter().copied().filter(|n| !asked.contains(n)).collect();
    if in_library && lacking.is_empty() {
        TitleStatus::Library
    } else if restricted {
        TitleStatus::Restricted
    } else if mark.requested() && unasked.is_empty() {
        TitleStatus::Requested
    } else if mark.declined || (!unasked.is_empty() && unasked.iter().all(|n| mark.declined_seasons.contains_key(n))) {
        // Its last request was declined, or what is left to ask for was.
        TitleStatus::Declined
    } else if (in_library || mark.requested()) && !unasked.is_empty() {
        TitleStatus::Partial
    } else {
        TitleStatus::None
    }
}

/// A show's aired seasons but specials that no library with these `counts`
/// has whole.
fn lacking(details: &Details, counts: &HashMap<i64, (i64, i64)>) -> Vec<i64> {
    details
        .seasons
        .iter()
        .filter(|s| s.number > 0 && s.aired > 0 && whole(counts, s) != Whole::Yes)
        .map(|s| s.number)
        .collect()
}

/// A season's state for one account (see [`SeasonState`]).
fn season_state(season: &discover::Season, counts: &HashMap<i64, (i64, i64)>, asked: &HashSet<i64>) -> SeasonState {
    if whole(counts, season) == Whole::Yes {
        SeasonState::Library
    } else if asked.contains(&season.number) {
        SeasonState::Requested
    } else if season.aired <= 0 {
        SeasonState::Unaired
    } else if counts.get(&season.number).is_some_and(|c| c.0 > 0) {
        SeasonState::Partial
    } else {
        SeasonState::Missing
    }
}

/// The seasons the season picker ticks first, were there no limit: those
/// missing or partly there, but specials and those declined lately; none of
/// a title the account can't ask for.
fn askable(details: &Details, counts: &HashMap<i64, (i64, i64)>, mark: &Mark, restricted: bool) -> Vec<i64> {
    if restricted {
        return Vec::new();
    }
    let asked = mark.asked();
    details
        .seasons
        .iter()
        .filter(|s| s.number > 0 && !mark.declined_seasons.contains_key(&s.number))
        .filter(|s| matches!(season_state(s, counts, &asked), SeasonState::Missing | SeasonState::Partial))
        .map(|s| s.number)
        .collect()
}

/// The seasons asked for when none are named: the aired ones but specials
/// that no library with these `counts` has whole, less those `declined`
/// for the account lately; when none has aired yet, every season but
/// specials. Should every one lacking have been declined, or none lack,
/// those (to be refused as declined, or as in a library).
fn default_seasons(details: &Details, counts: &HashMap<i64, (i64, i64)>, declined: &HashMap<i64, i64>) -> Vec<i64> {
    let aired: Vec<i64> = details.seasons.iter().filter(|s| s.number > 0 && s.aired > 0).map(|s| s.number).collect();
    if aired.is_empty() {
        return details.seasons.iter().map(|s| s.number).filter(|n| *n > 0).collect();
    }
    let lacking = lacking(details, counts);
    let open: Vec<i64> = lacking.iter().copied().filter(|n| !declined.contains_key(n)).collect();
    match (open.is_empty(), lacking.is_empty()) {
        (false, _) => open,
        (true, false) => lacking,
        (true, true) => aired,
    }
}

/// Whether a title rated `certification` is above `user`'s age limit:
/// unrated ones are, for an account with one, as in the libraries.
fn restricted(user: &AuthUser, certification: Option<&str>) -> bool {
    user.max_age.is_some_and(|max| certification.and_then(rating_age).is_none_or(|age| age > max))
}

/// Why an account with an age limit can't ask for a title rated
/// `certification`.
fn restriction(certification: Option<&str>) -> String {
    match certification.map(str::trim).filter(|c| !c.is_empty()) {
        Some(rating) => format!("Rated {rating}: above this account's age limit"),
        None => "Not rated: with an age limit, this account may ask for rated titles only".to_owned(),
    }
}

fn title_of(found: &Found, mark: &Mark, status: TitleStatus) -> DiscoverTitle {
    DiscoverTitle {
        kind: found.kind,
        tmdb_id: found.tmdb_id,
        title: found.title.clone(),
        year: found.year,
        overview: found.overview.clone(),
        poster_url: pictures::url(Picture::Poster, found.poster_path.as_deref()),
        backdrop_url: pictures::url(Picture::Backdrop, found.backdrop_path.as_deref()),
        rating: found.rating,
        status,
        item_id: mark.item_id,
        request_id: mark.request_id(),
    }
}

/// Whether a library has a season whole: as many episodes as have aired.
/// `Maybe`: fewer, but the last one aired, as with files of several
/// episodes each (or some missing): Sonarr can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Whole {
    Yes,
    Maybe,
    No,
}

/// `counts`: episodes a library has of each season, with the highest
/// episode number.
fn whole(counts: &HashMap<i64, (i64, i64)>, season: &discover::Season) -> Whole {
    let (episodes, last) = counts.get(&season.number).copied().unwrap_or_default();
    if season.aired <= 0 {
        Whole::No
    } else if episodes >= season.aired {
        Whole::Yes
    } else if episodes > 0 && last >= season.aired {
        Whole::Maybe
    } else {
        Whole::No
    }
}

// ------------------------------------------------------------------ limits

/// What counts toward an account's limits in [`LIMIT_WINDOW`], and the
/// limits that apply to it.
#[derive(Debug, Clone, Default)]
struct Quota {
    /// The settings' limits; `None`: none, or none apply.
    movies_limit: Option<i64>,
    seasons_limit: Option<i64>,
    /// What counts, oldest first: when each request was made, and how much
    /// of the limit it takes (a movie 1, a show its seasons).
    movies: Vec<(i64, i64)>,
    seasons: Vec<(i64, i64)>,
}

impl Quota {
    fn of(&self, kind: TitleKind) -> (Option<i64>, &[(i64, i64)]) {
        match kind {
            TitleKind::Movie => (self.movies_limit, &self.movies),
            TitleKind::Show => (self.seasons_limit, &self.seasons),
        }
    }

    /// How many more of `kind` may be asked for now; `None`: no limit.
    fn left(&self, kind: TitleKind) -> Option<i64> {
        let (limit, counted) = self.of(kind);
        limit.map(|limit| (limit - counted.iter().map(|c| c.1).sum::<i64>()).max(0))
    }

    /// When enough of what counts stops counting for `more` of `kind` to
    /// be asked for; `None` with no limit, when they may be already, and
    /// when `more` is past the limit itself. With `more` 1: when one more
    /// may be, once none is left.
    fn free_at(&self, kind: TitleKind, more: i64) -> Option<i64> {
        let (limit, counted) = self.of(kind);
        let limit = limit.filter(|limit| more <= *limit)?;
        let mut used: i64 = counted.iter().map(|c| c.1).sum();
        if used + more <= limit {
            return None;
        }
        counted.iter().find_map(|(at, count)| {
            used -= count;
            (used + more <= limit).then_some(at + LIMIT_WINDOW)
        })
    }

    /// Refuses asking for `count` of `kind` past the limit, saying when all
    /// of it may be asked for.
    fn allows(&self, kind: TitleKind, count: i64) -> RequestResult<()> {
        let (Some(limit), Some(left)) = (self.of(kind).0, self.left(kind)) else { return Ok(()) };
        if count <= left {
            return Ok(());
        }
        let (one, many) = match kind {
            TitleKind::Movie => ("movie", "movies"),
            TitleKind::Show => ("season", "seasons"),
        };
        let retry_at = self.free_at(kind, count);
        let message = match (left, retry_at) {
            (_, None) => {
                let what = if limit == 1 { one } else { many };
                format!("this account may ask for at most {limit} {what} per 7 days: ask for fewer")
            }
            (0, Some(at)) => {
                format!(
                    "this account has asked for all the {many} it may for now: it may ask for more from {}",
                    day(at)
                )
            }
            (1, Some(_)) => format!("this account may ask for 1 more {one} for now"),
            (left, Some(_)) => format!("this account may ask for {left} more {many} for now"),
        };
        Err(RequestError::Limit { message, retry_at })
    }
}

/// `2026-11-03 14:05 UTC`.
fn day(secs: i64) -> String {
    let [y, m, d, h, min, _] = utc(secs);
    format!("{y}-{m:02}-{d:02} {h:02}:{min:02} UTC")
}

// ------------------------------------------------------------------ the service

pub struct Requests {
    db: Db,
    settings: Arc<SettingsStore>,
    events: EventBus,
    discover: Discover,
    pictures: Pictures,
    arr: Arr,
    /// Requests are made one at a time: two at once can't both find a title
    /// not asked for yet.
    making: tokio::sync::Mutex<()>,
    /// One look at the libraries at a time.
    syncing: tokio::sync::Mutex<()>,
    /// The requests whose send runs now ([`Self::send`]): one at a time
    /// for each.
    sending: watch::Sender<HashSet<i64>>,
    /// What Radarr, then Sonarr, offers approvals ([`Self::offered`]).
    offers: [parking_lot::Mutex<Offer>; 2],
}

/// What Radarr or Sonarr offered approvals, as last asked: where, when it
/// answered and with what; why not, when it didn't; and the question out
/// now, if one is (it says `true` once answered).
#[derive(Default)]
struct Offer {
    to: Option<Connection>,
    answered: Option<Instant>,
    lists: Lists,
    error: Option<String>,
    asking: Option<watch::Receiver<bool>>,
}

/// A request's send running: it no longer is once this is dropped, however
/// it ends.
struct Sending(Arc<Requests>, i64);

impl Drop for Sending {
    fn drop(&mut self) {
        let id = self.1;
        self.0.sending.send_modify(|sending| {
            sending.remove(&id);
        });
    }
}

impl Requests {
    pub fn new(
        db: Db,
        settings: Arc<SettingsStore>,
        events: EventBus,
        metadata: Arc<MetadataAgent>,
        pictures: Pictures,
    ) -> Arc<Self> {
        Arc::new(Self {
            discover: Discover::new(metadata, settings.clone()),
            arr: Arr::new(),
            pictures,
            db,
            settings,
            events,
            making: Default::default(),
            syncing: Default::default(),
            sending: watch::channel(HashSet::new()).0,
            offers: Default::default(),
        })
    }

    pub fn arr(&self) -> &Arr {
        &self.arr
    }

    /// Requests are on, and TMDB has a key.
    pub fn enabled(&self) -> bool {
        let settings = self.settings.get();
        settings.requests && !settings.tmdb_api_key.is_empty()
    }

    /// Refuses while requests are off (or TMDB has no key).
    pub fn on(&self) -> RequestResult<()> {
        if self.enabled() { Ok(()) } else { Err(RequestError::Off) }
    }

    /// Who `user` is to requests ([`Caller`]). An administrator's account
    /// asks without limits: approved as it asks from pley's own sign-in and
    /// apps, and for a manager to approve from other publishers' apps.
    /// Other accounts ask as their `request_access` says.
    pub async fn caller<'a>(&self, user: &'a AuthUser) -> sqlx::Result<Caller<'a>> {
        let manager = user.manages_requests();
        if user.account_admin {
            let access = if user.first_party() { RequestAccess::Auto } else { RequestAccess::Ask };
            return Ok(Caller { user, access, limited: false, manager });
        }
        let access: Option<String> = sqlx::query_scalar("SELECT request_access FROM users WHERE id = ?")
            .bind(user.id)
            .fetch_optional(&self.db.read)
            .await?;
        let access = access.as_deref().map_or(RequestAccess::Off, RequestAccess::parse);
        Ok(Caller { user, access, limited: true, manager })
    }

    /// Requests are on: who `user` is to them. Every route but the summary
    /// starts here (or with [`Self::asker`], [`Self::manager`]): an account
    /// that may not ask keeps its own requests.
    pub async fn reader<'a>(&self, user: &'a AuthUser) -> RequestResult<Caller<'a>> {
        self.on()?;
        Ok(self.caller(user).await?)
    }

    /// The same, for Discover and asking: the account may ask.
    pub async fn asker<'a>(&self, user: &'a AuthUser) -> RequestResult<Caller<'a>> {
        let caller = self.reader(user).await?;
        if caller.access == RequestAccess::Off {
            return Err(RequestError::NoAccess);
        }
        Ok(caller)
    }

    /// The same, for approving and declining: `user` manages requests.
    pub async fn manager<'a>(&self, user: &'a AuthUser) -> RequestResult<Caller<'a>> {
        let caller = self.reader(user).await?;
        if !caller.manager {
            return Err(AppError::forbidden("only administrators approve and decline requests").into());
        }
        Ok(caller)
    }

    // -------------------------------------------------------------- events

    /// Whom news of request `id` is for: its maker, its followers, and
    /// managers.
    async fn audience(&self, id: i64) -> sqlx::Result<Audience> {
        let users: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM requests WHERE id = ? UNION SELECT user_id FROM request_watchers WHERE request_id = ?",
        )
        .bind(id)
        .bind(id)
        .fetch_all(&self.db.read)
        .await?;
        Ok(Audience::of(users))
    }

    /// Who follows request `id`, the earliest first.
    async fn followers(&self, id: i64) -> sqlx::Result<Vec<i64>> {
        sqlx::query_scalar("SELECT user_id FROM request_watchers WHERE request_id = ? ORDER BY created_at, user_id")
            .bind(id)
            .fetch_all(&self.db.read)
            .await
    }

    /// Request `id` (of `kind` and `tmdb_id`) changed: `to` hears it.
    fn changed_to(&self, to: Audience, id: i64, kind: TitleKind, tmdb_id: i64) {
        self.events.send(ServerEvent::RequestsChanged {
            to,
            request_id: Some(id),
            kind: Some(kind),
            tmdb_id: Some(tmdb_id),
        });
    }

    /// The same, to its maker, its followers and managers.
    async fn changed(&self, id: i64, kind: TitleKind, tmdb_id: i64) {
        match self.audience(id).await {
            Ok(to) => self.changed_to(to, id, kind, tmdb_id),
            Err(err) => tracing::warn!("request {id}: who to tell of it: {err}"),
        }
    }

    /// Many requests changed: `to` hears it.
    fn many_changed(&self, to: Audience) {
        self.events.send(ServerEvent::RequestsChanged { to, request_id: None, kind: None, tmdb_id: None });
    }

    /// Someone asked (request `row`): managers but its maker hear it.
    fn created(&self, row: &RequestRow) {
        self.events.send(ServerEvent::RequestCreated {
            to: Audience { except_user: Some(row.user_id), ..Audience::of(Vec::new()) },
            request_id: row.id,
            title: row.title.clone(),
            kind: row.kind(),
            tmdb_id: row.tmdb_id,
            seasons: row.seasons(),
            requested_by: row.maker(),
            status: row.status(),
        });
    }

    /// Request `row` was approved or declined by account `decider`: its
    /// maker hears it, unless they decided it, and a declined one's
    /// followers do too, without the reason.
    async fn decided(&self, row: &RequestRow, decider: i64) -> sqlx::Result<()> {
        let status = row.status();
        let event = |to: Audience, reason: Option<String>| ServerEvent::RequestDecided {
            to,
            request_id: row.id,
            title: row.title.clone(),
            kind: row.kind(),
            tmdb_id: row.tmdb_id,
            status,
            reason,
        };
        let declined = status == RequestStatus::Declined;
        if row.user_id != decider {
            self.events.send(event(Audience::only(vec![row.user_id]), row.reason.clone().filter(|_| declined)));
        }
        if declined {
            let followers: Vec<i64> = self.followers(row.id).await?.into_iter().filter(|id| *id != decider).collect();
            if !followers.is_empty() {
                self.events.send(event(Audience::only(followers), None));
            }
        }
        Ok(())
    }

    /// Request `row` is ready (`complete`), or some more of its seasons are
    /// (`seasons`: those newly ready): its maker hears it, and each follower
    /// who can open the title in a library, each with the library's title
    /// where they may open it. The other followers hear only that it
    /// changed.
    async fn ready(&self, row: &RequestRow, seasons: Option<Vec<i64>>, complete: bool) -> sqlx::Result<()> {
        let mut people = vec![row.user_id];
        people.extend(self.followers(row.id).await?);
        for user_id in people {
            let library = self.in_library(user_id, row.kind(), &[row.known()]).await?;
            let item_id = library.iter().find(|l| l.2).map(|l| l.1);
            if item_id.is_none() && user_id != row.user_id {
                continue;
            }
            self.events.send(ServerEvent::RequestAvailable {
                to: Audience::only(vec![user_id]),
                request_id: row.id,
                title: row.title.clone(),
                kind: row.kind(),
                tmdb_id: row.tmdb_id,
                item_id,
                poster_url: pictures::url(Picture::Poster, row.poster_path.as_deref()),
                seasons: seasons.clone(),
                complete,
            });
        }
        Ok(())
    }

    /// An account was deleted with requests it made or followed: `to`
    /// (managers, and those who inherited some) hears that many changed.
    pub fn account_deleted(&self, to: Audience) {
        self.many_changed(to);
    }

    /// `409 conflict`: what was asked can't be done to request `id` as it
    /// is now, saying why (`message`, from the request as `caller` sees it)
    /// and when it may be (`retry_at`); `404` once it is gone.
    async fn refused(
        &self,
        caller: &Caller<'_>,
        id: i64,
        retry_at: Option<i64>,
        message: impl FnOnce(&MediaRequest) -> String,
    ) -> RequestError {
        match self.one(caller, id).await {
            Ok(request) => RequestError::Conflict { message: message(&request), request: Box::new(request), retry_at },
            Err(err) => err,
        }
    }

    // -------------------------------------------------------------- reading

    /// `GET /api/requests/summary`: for any session. While requests are
    /// off, and for an app that can't ask, every number is 0.
    pub async fn summary(&self, user: &AuthUser) -> sqlx::Result<Summary> {
        let caller = self.caller(user).await?;
        let enabled = self.enabled();
        if !enabled || !user.allowed(Scope::Requests) {
            return Ok(Summary {
                enabled,
                access: caller.access,
                movies_left: Some(0),
                seasons_left: Some(0),
                movies_limit: Some(0),
                seasons_limit: Some(0),
                // Only where signing in again lets the app ask.
                sign_in_again: (enabled && caller.access != RequestAccess::Off).then_some("requests"),
                ..Summary::default()
            });
        }
        let quota = self.quota(&caller).await?;
        let (pending, failed, unseen, own, changed_at): (i64, i64, i64, i64, Option<i64>) =
            sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "SELECT COALESCE(SUM(status = 'pending'), 0), COALESCE(SUM(status = 'failed'), 0),
                        COALESCE(SUM(mine AND news_at > COALESCE((SELECT requests_seen_at FROM users WHERE id = ?), 0)), 0),
                        COALESCE(SUM(mine), 0), MAX(CASE WHEN mine OR ? THEN updated_at END)
                 FROM (SELECT r.status, r.news_at, r.updated_at, {MINE} AS mine FROM requests r)"
            )))
            .bind(user.id)
            .bind(caller.manager)
            .bind(user.id)
            .bind(user.id)
            .fetch_one(&self.db.read)
            .await?;
        // An administrator's app of pley's own that was signed in before it
        // could manage requests.
        let approvals =
            user.account_admin && user.app.is_some() && user.first_party() && !user.allowed(Scope::Approvals);
        Ok(Summary {
            enabled,
            access: caller.access,
            movies_left: quota.left(TitleKind::Movie),
            seasons_left: quota.left(TitleKind::Show),
            movies_limit: quota.movies_limit,
            seasons_limit: quota.seasons_limit,
            movies_next_at: quota.free_at(TitleKind::Movie, 1),
            seasons_next_at: quota.free_at(TitleKind::Show, 1),
            manage: caller.manager,
            pending: if caller.manager { pending } else { 0 },
            failed: if caller.manager { failed } else { 0 },
            unseen,
            own,
            changed_at,
            sign_in_again: approvals.then_some("approvals"),
        })
    }

    /// What counts toward `caller`'s limits: its own requests of the last 7
    /// days, but those declined and those failed (following one costs
    /// nothing). One it inherited as its earliest follower counts from then,
    /// as its own asking would have: otherwise one account could ask for
    /// another without limit, following and having it withdrawn. None
    /// where limits don't apply.
    async fn quota(&self, caller: &Caller<'_>) -> sqlx::Result<Quota> {
        if !caller.limited {
            return Ok(Quota::default());
        }
        let settings = self.settings.get();
        let limit = |limit: u32| (limit > 0).then_some(i64::from(limit));
        let mut quota = Quota {
            movies_limit: limit(settings.request_limit_movies),
            seasons_limit: limit(settings.request_limit_seasons),
            ..Quota::default()
        };
        if quota.movies_limit.is_none() && quota.seasons_limit.is_none() {
            return Ok(quota);
        }
        let rows: Vec<(String, Option<String>, i64)> = sqlx::query_as(
            "SELECT kind, seasons, COALESCE(inherited_at, created_at) AS at FROM requests
             WHERE user_id = ? AND COALESCE(inherited_at, created_at) > ?
               AND status IN ('pending', 'approved', 'available')
             ORDER BY at, id",
        )
        .bind(caller.user.id)
        .bind(now() - LIMIT_WINDOW)
        .fetch_all(&self.db.read)
        .await?;
        for (kind, seasons, at) in rows {
            match TitleKind::parse(&kind) {
                Some(TitleKind::Movie) => quota.movies.push((at, 1)),
                _ => quota.seasons.push((at, seasons.as_deref().map_or(0, |s| seasons_of(s).len() as i64))),
            }
        }
        Ok(quota)
    }

    /// `GET /api/discover/{list}`.
    pub async fn discover(&self, caller: &Caller<'_>, list: List, page: u32) -> RequestResult<DiscoverPage> {
        // For an account with an age limit, TMDB itself keeps the movie
        // lists within it, so that few are left out.
        let ceiling = match (caller.user.max_age, list) {
            (Some(max_age), List::Movies | List::Upcoming) => self.discover.ceiling(max_age).await,
            _ => None,
        };
        let found = self.discover.list(list, page, ceiling.as_deref()).await?;
        self.page(caller, &found, false).await
    }

    /// `GET /api/discover/search`; `missing`: without the titles a library
    /// the account can open has.
    pub async fn search(
        &self,
        caller: &Caller<'_>,
        text: &str,
        page: u32,
        missing: bool,
    ) -> RequestResult<DiscoverPage> {
        let found = self.discover.search(text, page).await?;
        self.page(caller, &found, missing).await
    }

    /// A page of TMDB's titles as `caller` sees them. For an account with
    /// an age limit, those above it are left out (and unrated ones, and
    /// those whose details TMDB doesn't give), as in the libraries.
    async fn page(&self, caller: &Caller<'_>, found: &discover::Page, missing: bool) -> RequestResult<DiscoverPage> {
        let user = caller.user;
        let mut marks: HashMap<(TitleKind, i64), Mark> = HashMap::new();
        for kind in [TitleKind::Movie, TitleKind::Show] {
            let known: Vec<Known> =
                found.results.iter().filter(|t| t.kind == kind).map(|t| Known::tmdb(t.tmdb_id)).collect();
            for (tmdb_id, mark) in self.marks(caller, kind, &known).await? {
                marks.insert((kind, tmdb_id), mark);
            }
        }
        // Details where they matter: the seasons of shows a library has or
        // a request asks for, and every title's certification for an
        // account with an age limit.
        let limited = user.max_age.is_some();
        let wanted: Vec<(TitleKind, i64)> = found
            .results
            .iter()
            .filter(|t| {
                limited
                    || (t.kind == TitleKind::Show
                        && marks.get(&(t.kind, t.tmdb_id)).is_some_and(|m| m.item_id.is_some() || m.requested()))
            })
            .map(|t| (t.kind, t.tmdb_id))
            .collect();
        // Made before they run, as in `marks`.
        let lookups: Vec<_> = wanted.iter().map(|(kind, id)| self.discover.details(*kind, *id)).collect();
        let answers: Vec<_> = futures::stream::iter(lookups).buffered(LOOKUPS).collect().await;
        let mut details: HashMap<(TitleKind, i64), Arc<Details>> = HashMap::new();
        for (key, answer) in wanted.into_iter().zip(answers) {
            match answer {
                Ok(answer) => {
                    details.insert(key, answer);
                }
                Err(err) => tracing::debug!("TMDB: {} {}'s details: {err}", key.0.as_str(), key.1),
            }
        }
        let mut results = Vec::new();
        for title in &found.results {
            let key = (title.kind, title.tmdb_id);
            let known = details.get(&key);
            if known.is_some_and(|d| d.adult)
                || (limited && known.is_none_or(|d| restricted(user, d.certification.as_deref())))
            {
                continue;
            }
            let mark = marks.get(&key).cloned().unwrap_or_default();
            let lacking = match known {
                Some(details) if title.kind == TitleKind::Show => {
                    Some(lacking(details, &self.season_counts(&mark.shows).await?))
                }
                _ => None,
            };
            let status = status_of(title.kind, &mark, lacking.as_deref(), false);
            if missing && status == TitleStatus::Library {
                continue;
            }
            results.push(title_of(title, &mark, status));
        }
        Ok(DiscoverPage { page: found.page, total_pages: found.total_pages, results })
    }

    /// What the libraries it can open and the requests have of these titles
    /// of `kind`, for `caller`, by TMDB's id.
    async fn marks(&self, caller: &Caller<'_>, kind: TitleKind, known: &[Known]) -> sqlx::Result<HashMap<i64, Mark>> {
        let mut marks: HashMap<i64, Mark> = HashMap::new();
        if known.is_empty() {
            return Ok(marks);
        }
        let user_id = caller.user.id;
        let mut found = self.in_library(user_id, kind, known).await?;
        // Shows a TV source other than TMDB named have no TMDB id: the
        // ones not found are looked for by their other ids.
        if kind == TitleKind::Show && self.shows_without_tmdb().await? {
            let missing: Vec<usize> = (0..known.len())
                .filter(|i| known[*i].tvdb_id.is_none() && known[*i].imdb_id.is_none())
                .filter(|i| !found.iter().any(|f| f.0 == *i && f.2))
                .collect();
            // Made before they run: a stream mapping borrowed indexes
            // would keep the handlers' futures from being `Send`.
            let lookups: Vec<_> = missing.iter().map(|i| self.discover.show_ids(known[*i].tmdb_id)).collect();
            let ids: Vec<_> = futures::stream::iter(lookups).buffered(LOOKUPS).collect().await;
            let (mut others, mut index) = (Vec::new(), Vec::new());
            for (i, ids) in missing.iter().zip(ids) {
                match ids {
                    Ok(ids) if ids.tvdb_id.is_some() || ids.imdb_id.is_some() => {
                        others.push(Known {
                            tmdb_id: known[*i].tmdb_id,
                            tvdb_id: ids.tvdb_id,
                            imdb_id: ids.imdb_id.clone(),
                        });
                        index.push(*i);
                    }
                    Ok(_) => {}
                    Err(err) => tracing::debug!("TMDB: a show's ids: {err}"),
                }
            }
            if !others.is_empty() {
                let more = self.in_library(user_id, kind, &others).await?;
                found.extend(more.into_iter().map(|(j, item, visible)| (index[j], item, visible)));
            }
        }
        // Libraries the account can't open don't count: it may ask for
        // what only they have.
        for (i, item, _) in found.into_iter().filter(|f| f.2) {
            let mark = marks.entry(known[i].tmdb_id).or_default();
            mark.item_id.get_or_insert(item);
            if kind == TitleKind::Show && !mark.shows.contains(&item) {
                mark.shows.push(item);
            }
        }
        let ids: Vec<i64> = known.iter().map(|k| k.tmdb_id).collect();
        for (tmdb_id, id, seasons, mine, follows, status) in self.active_requests(user_id, kind, &ids).await? {
            marks.entry(tmdb_id).or_default().active.push(Active {
                id,
                seasons: seasons.as_deref().map(seasons_of).unwrap_or_default(),
                theirs: mine || follows || caller.manager,
                mine,
                follows,
                pending: status == "pending",
            });
        }
        let since = now() - DECLINED_FOR;
        let mut newest = HashSet::new();
        for (tmdb_id, status, seasons, reason, decided_at) in self.own_requests(user_id, kind, &ids).await? {
            let last = newest.insert(tmdb_id);
            let Some(at) = decided_at.filter(|at| status == "declined" && *at > since) else { continue };
            let mark = marks.entry(tmdb_id).or_default();
            mark.declined |= last;
            // The latest decline of a season says when it may be asked for.
            for n in seasons.as_deref().map(seasons_of).unwrap_or_default() {
                let again_at = mark.declined_seasons.entry(n).or_default();
                *again_at = (*again_at).max(at + DECLINED_FOR);
            }
            mark.refusal.get_or_insert(Declined { reason, at, again_at: at + DECLINED_FOR });
        }
        Ok(marks)
    }

    /// Library titles of `kind` that are any of `titles` (by TMDB's id, or
    /// TheTVDB's or IMDb's where known): the title's index in `titles`, the
    /// library's title, and whether `user_id` may open it.
    async fn in_library(
        &self,
        user_id: i64,
        kind: TitleKind,
        titles: &[Known],
    ) -> sqlx::Result<Vec<(usize, i64, bool)>> {
        let tvdb: Vec<i64> = titles.iter().filter_map(|t| t.tvdb_id).collect();
        let imdb: Vec<&str> = titles.iter().filter_map(|t| t.imdb_id.as_deref()).collect();
        let list = |n: usize| vec!["?"; n].join(",");
        let sql = format!(
            "{ME_CTE} SELECT i.id, i.tmdb_id, i.tvdb_id, i.imdb_id, CASE WHEN {VISIBLE} THEN 1 ELSE 0 END
             FROM items i LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
             WHERE i.kind = ? AND (i.tmdb_id IN ({}) OR i.tvdb_id IN ({}) OR i.imdb_id IN ({}))
             ORDER BY i.id",
            list(titles.len()),
            list(tvdb.len()),
            list(imdb.len()),
        );
        let mut query = sqlx::query_as::<_, (i64, Option<i64>, Option<i64>, Option<String>, bool)>(
            sqlx::AssertSqlSafe(sql.as_str()),
        )
        .bind(user_id)
        .bind(kind.as_str());
        for title in titles {
            query = query.bind(title.tmdb_id);
        }
        for id in &tvdb {
            query = query.bind(id);
        }
        for id in &imdb {
            query = query.bind(*id);
        }
        let mut found = Vec::new();
        for (item, tmdb_id, tvdb_id, imdb_id, visible) in query.fetch_all(&self.db.read).await? {
            for (i, title) in titles.iter().enumerate() {
                let same = tmdb_id == Some(title.tmdb_id)
                    || (title.tvdb_id.is_some() && tvdb_id == title.tvdb_id)
                    || (title.imdb_id.is_some() && imdb_id == title.imdb_id);
                if same {
                    found.push((i, item, visible));
                }
            }
        }
        Ok(found)
    }

    /// Whether some library show has no TMDB id (another TV source named
    /// it): shows are then looked for by their other ids too.
    async fn shows_without_tmdb(&self) -> sqlx::Result<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM items WHERE kind = 'show' AND tmdb_id IS NULL
                            AND (tvdb_id IS NOT NULL OR imdb_id IS NOT NULL))",
        )
        .fetch_one(&self.db.read)
        .await
    }

    /// Active requests for any of these titles, newest first: TMDB's id,
    /// the request's, its seasons, whether `user_id` made it and whether it
    /// follows it, and its status.
    #[allow(clippy::type_complexity)]
    async fn active_requests(
        &self,
        user_id: i64,
        kind: TitleKind,
        tmdb_ids: &[i64],
    ) -> sqlx::Result<Vec<(i64, i64, Option<String>, bool, bool, String)>> {
        let sql = format!(
            "SELECT r.tmdb_id, r.id, r.seasons, r.user_id = ?,
                    EXISTS (SELECT 1 FROM request_watchers w WHERE w.request_id = r.id AND w.user_id = ?), r.status
             FROM requests r WHERE r.kind = ? AND r.tmdb_id IN ({}) AND r.status IN {ACTIVE} ORDER BY r.id DESC",
            vec!["?"; tmdb_ids.len()].join(",")
        );
        let mut query =
            sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id).bind(user_id).bind(kind.as_str());
        for id in tmdb_ids {
            query = query.bind(id);
        }
        query.fetch_all(&self.db.read).await
    }

    /// `user_id`'s requests for any of these titles, made or followed,
    /// newest first: TMDB's id, the status, the seasons, the reason it was
    /// declined (its own only, but one decided before it inherited it), and
    /// when it was decided.
    #[allow(clippy::type_complexity)]
    async fn own_requests(
        &self,
        user_id: i64,
        kind: TitleKind,
        tmdb_ids: &[i64],
    ) -> sqlx::Result<Vec<(i64, String, Option<String>, Option<String>, Option<i64>)>> {
        let sql = format!(
            "SELECT r.tmdb_id, r.status, r.seasons,
                    CASE WHEN r.user_id = ? AND (r.inherited_at IS NULL OR r.decided_at > r.inherited_at)
                         THEN r.reason END,
                    r.decided_at
             FROM requests r WHERE r.kind = ? AND r.tmdb_id IN ({}) AND {MINE}
             ORDER BY r.created_at DESC, r.id DESC",
            vec!["?"; tmdb_ids.len()].join(",")
        );
        let mut query = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id).bind(kind.as_str());
        for id in tmdb_ids {
            query = query.bind(id);
        }
        query.bind(user_id).bind(user_id).fetch_all(&self.db.read).await
    }

    /// Episodes the libraries have of each season of these shows (the most
    /// any one of them has), with the highest episode number.
    async fn season_counts(&self, shows: &[i64]) -> sqlx::Result<HashMap<i64, (i64, i64)>> {
        if shows.is_empty() {
            return Ok(HashMap::new());
        }
        let sql = format!(
            "SELECT s.idx, COUNT(e.id), COALESCE(MAX(e.idx), 0) FROM items s
               JOIN items e ON e.parent_id = s.id AND e.kind = 'episode'
             WHERE s.parent_id IN ({}) AND s.kind = 'season' AND s.idx IS NOT NULL
             GROUP BY s.parent_id, s.idx",
            vec!["?"; shows.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, (i64, i64, i64)>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in shows {
            query = query.bind(id);
        }
        let mut counts: HashMap<i64, (i64, i64)> = HashMap::new();
        for (season, episodes, last) in query.fetch_all(&self.db.read).await? {
            let entry = counts.entry(season).or_default();
            *entry = (entry.0.max(episodes), entry.1.max(last));
        }
        Ok(counts)
    }

    /// `GET /api/discover/movie/{tmdb_id}` and `…/show/{tmdb_id}`.
    pub async fn detail(&self, caller: &Caller<'_>, kind: TitleKind, tmdb_id: i64) -> RequestResult<DiscoverDetail> {
        let user = caller.user;
        let details = self.title(caller, kind, tmdb_id).await?;
        let mark = self.marks(caller, kind, &[Known::of(&details)]).await?.remove(&tmdb_id).unwrap_or_default();
        let counts = self.season_counts(&mark.shows).await?;
        let restricted = restricted(user, details.certification.as_deref());
        let lacking = (kind == TitleKind::Show).then(|| lacking(&details, &counts));
        let status = status_of(kind, &mark, lacking.as_deref(), restricted);
        let asked = mark.asked();
        let seasons = (kind == TitleKind::Show).then(|| {
            details
                .seasons
                .iter()
                .map(|s| DiscoverSeason {
                    number: s.number,
                    name: s.name.clone(),
                    episodes: s.episodes,
                    aired: s.aired,
                    air_date: s.air_date.clone(),
                    poster_url: pictures::url(Picture::Poster, s.poster_path.as_deref()),
                    in_library: counts.get(&s.number).map_or(0, |c| c.0),
                    requested: asked.contains(&s.number),
                    state: season_state(s, &counts, &asked),
                    declined: mark.declined_seasons.contains_key(&s.number),
                    request_id: mark.season_request(s.number),
                })
                .collect()
        });
        let rows = self.title_rows(caller, kind, tmdb_id).await?;
        let requests = self.dtos(caller, rows).await?;
        let quota = self.quota(caller).await?;
        let found = Found {
            kind,
            tmdb_id,
            title: details.title.clone(),
            year: details.year,
            overview: details.overview.clone(),
            poster_path: details.poster_path.clone(),
            backdrop_path: details.backdrop_path.clone(),
            rating: details.rating,
        };
        Ok(DiscoverDetail {
            title: title_of(&found, &mark, status),
            original_title: details.original_title.clone(),
            tagline: details.tagline.clone(),
            genres: details.genres.clone(),
            release_date: details.release_date.clone(),
            runtime_minutes: details.runtime_minutes,
            certification: details.certification.clone(),
            network: details.network.clone(),
            show_status: details.show_status.clone(),
            anime: details.anime,
            imdb_id: details.imdb_id.clone(),
            cast: details.cast.clone(),
            seasons,
            declined: mark.refusal.clone(),
            restriction: restricted.then(|| restriction(details.certification.as_deref())),
            watching: mark.watching(),
            requests,
            movies_left: quota.left(TitleKind::Movie),
            seasons_left: quota.left(TitleKind::Show),
            movies_next_at: quota.free_at(TitleKind::Movie, 1),
            seasons_next_at: quota.free_at(TitleKind::Show, 1),
        })
    }

    /// A title's details, where `caller` may see it: TMDB's adult titles
    /// are for managers only.
    async fn title(&self, caller: &Caller<'_>, kind: TitleKind, tmdb_id: i64) -> RequestResult<Arc<Details>> {
        let unknown = || RequestError::from(AppError::not_found("TMDB doesn't know this title"));
        if tmdb_id <= 0 {
            return Err(unknown());
        }
        let details = self.discover.details(kind, tmdb_id).await?;
        if details.adult && !caller.manager {
            return Err(unknown());
        }
        Ok(details)
    }

    /// `GET /api/items/{id}/request`: a movie or show of the libraries
    /// `caller` can open, as Discover sees it. TMDB is waited for
    /// [`ITEM_WAIT`] at most in all, for which title the item is and its
    /// details (they go on to the cache after).
    pub async fn for_item(self: &Arc<Self>, caller: &Caller<'_>, item_id: i64) -> RequestResult<ItemRequest> {
        let user = caller.user;
        let sql = format!(
            "{ME_CTE} SELECT i.kind, i.tmdb_id, i.tvdb_id, i.imdb_id FROM items i
               LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
             WHERE i.id = ? AND {VISIBLE}"
        );
        let item: Option<LibraryTitle> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(user.id)
            .bind(item_id)
            .fetch_optional(&self.db.read)
            .await?;
        let LibraryTitle { kind, tmdb_id, tvdb_id, imdb_id } =
            item.ok_or_else(|| AppError::not_found(format!("item {item_id} not found")))?;
        let kind = TitleKind::parse(&kind).ok_or_else(|| AppError::not_found("only movies and shows are asked for"))?;
        let unknown = || RequestError::from(AppError::not_found("TMDB doesn't know this title"));
        let until = tokio::time::Instant::now() + ITEM_WAIT;
        // Named by TheTVDB or IMDb only: TMDB says which title it is.
        let tmdb_id = match tmdb_id.filter(|id| *id > 0) {
            Some(id) => id,
            None => {
                let (this, imdb) = (self.clone(), imdb_id.clone());
                let find = tokio::spawn(async move { this.discover.find(kind, tvdb_id, imdb.as_deref()).await });
                match tokio::time::timeout_at(until, find).await {
                    Ok(Ok(found)) => found?.ok_or_else(unknown)?,
                    // Still asking: the answer is kept for the next time.
                    Ok(Err(_)) | Err(_) => return Err(RequestError::TmdbDown),
                }
            }
        };
        let this = self.clone();
        let fetch = tokio::spawn(async move { this.discover.details(kind, tmdb_id).await });
        let details = match tokio::time::timeout_at(until, fetch).await {
            Ok(Ok(Ok(details))) => Some(details),
            Ok(Ok(Err(TmdbError::NotFound))) => return Err(unknown()),
            Ok(Ok(Err(err))) => {
                tracing::debug!("TMDB: {} {tmdb_id}'s details: {err}", kind.as_str());
                None
            }
            Ok(Err(_)) | Err(_) => None,
        };
        if details.as_ref().is_some_and(|d| d.adult) && !caller.manager {
            return Err(unknown());
        }
        let known = match &details {
            Some(details) => Known::of(details),
            None => Known { tmdb_id, tvdb_id, imdb_id },
        };
        let mark = self.marks(caller, kind, &[known]).await?.remove(&tmdb_id).unwrap_or_default();
        let counts = self.season_counts(&mark.shows).await?;
        let restricted = details.as_ref().is_some_and(|d| restricted(user, d.certification.as_deref()));
        let lacking = details.as_ref().filter(|_| kind == TitleKind::Show).map(|d| lacking(d, &counts));
        let status = status_of(kind, &mark, lacking.as_deref(), restricted);
        let askable = match kind {
            TitleKind::Movie => Some(Vec::new()),
            TitleKind::Show => details.as_ref().map(|d| askable(d, &counts, &mark, restricted)),
        };
        let request = match mark.request_id() {
            Some(id) => self.dtos(caller, self.row(id).await?.into_iter().collect()).await?.pop(),
            None => None,
        };
        Ok(ItemRequest { kind, tmdb_id, status, request, askable })
    }

    /// Requests for a title `caller` may see (its own and those it follows;
    /// everyone's for managers), newest first.
    async fn title_rows(&self, caller: &Caller<'_>, kind: TitleKind, tmdb_id: i64) -> sqlx::Result<Vec<RequestRow>> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM {TABLES} WHERE r.kind = ? AND r.tmdb_id = ? AND (? OR {MINE})
             ORDER BY r.created_at DESC, r.id DESC"
        )))
        .bind(kind.as_str())
        .bind(tmdb_id)
        .bind(caller.manager)
        .bind(caller.user.id)
        .bind(caller.user.id)
        .fetch_all(&self.db.read)
        .await
    }

    async fn row(&self, id: i64) -> sqlx::Result<Option<RequestRow>> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM {TABLES} WHERE r.id = ?")))
            .bind(id)
            .fetch_optional(&self.db.read)
            .await
    }

    /// Request `id`, where `caller` may see it: its own, or one it follows
    /// (any, for managers).
    async fn row_for(&self, caller: &Caller<'_>, id: i64) -> sqlx::Result<Option<RequestRow>> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {COLUMNS} FROM {TABLES} WHERE r.id = ? AND (? OR {MINE})")))
            .bind(id)
            .bind(caller.manager)
            .bind(caller.user.id)
            .bind(caller.user.id)
            .fetch_optional(&self.db.read)
            .await
    }

    /// Requests as `caller` sees them, with their downloads.
    async fn dtos(&self, caller: &Caller<'_>, rows: Vec<RequestRow>) -> sqlx::Result<Vec<MediaRequest>> {
        let viewer = self.viewer(caller, &rows).await?;
        let downloads = self.downloads(&rows);
        Ok(rows.iter().map(|r| r.to_dto(&viewer, downloads.get(&r.id).cloned())).collect())
    }

    /// When account `user_id` last looked at its requests' news.
    async fn seen_at(&self, user_id: i64) -> sqlx::Result<i64> {
        let seen_at: Option<i64> = sqlx::query_scalar("SELECT COALESCE(requests_seen_at, 0) FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(&self.db.read)
            .await?;
        Ok(seen_at.unwrap_or(0))
    }

    /// What of these requests is `caller`'s to know: the library titles it
    /// may open, who follows them, and when it last looked at its news; and
    /// what may be done to them now.
    async fn viewer(&self, caller: &Caller<'_>, rows: &[RequestRow]) -> sqlx::Result<Viewer> {
        let user = caller.user;
        let items: Vec<i64> = rows.iter().filter_map(|r| r.item_id).collect();
        let visible = visible_items(&self.db, user.id, &items).await?;
        let (mut followers, mut following) = (HashMap::new(), HashSet::new());
        if !rows.is_empty() {
            let sql = format!(
                "SELECT request_id, COUNT(*), MAX(user_id = ?) FROM request_watchers
                 WHERE request_id IN ({}) GROUP BY request_id",
                vec!["?"; rows.len()].join(",")
            );
            let mut query = sqlx::query_as::<_, (i64, i64, bool)>(sqlx::AssertSqlSafe(sql.as_str())).bind(user.id);
            for row in rows {
                query = query.bind(row.id);
            }
            for (id, count, follows) in query.fetch_all(&self.db.read).await? {
                followers.insert(id, count);
                if follows {
                    following.insert(id);
                }
            }
        }
        let settings = self.settings.get();
        Ok(Viewer {
            id: user.id,
            manager: caller.manager,
            asks: caller.access != RequestAccess::Off,
            now: now(),
            seen_at: self.seen_at(user.id).await?,
            visible,
            followers,
            following,
            connected: [Service::Radarr, Service::Sonarr]
                .into_iter()
                .filter(|s| s.connected(&settings).is_some())
                .collect(),
        })
    }

    /// Fetches Radarr's and Sonarr's queues now, both at once, where they
    /// are saved: at the start, so that the first answers have them.
    pub async fn refresh_queues(&self) {
        let settings = self.settings.get();
        let fetch = |service: Service| {
            let to = service.saved(&settings);
            async move {
                if let Some(to) = to {
                    self.arr.fetch_queue(service, &to).await;
                }
            }
        };
        tokio::join!(fetch(Service::Radarr), fetch(Service::Sonarr));
    }

    /// The downloads of approved requests that Radarr or Sonarr is busy
    /// with, from their queues as last fetched: answers don't wait for them
    /// ([`Arr::queue`]).
    fn downloads(&self, rows: &[RequestRow]) -> HashMap<i64, Download> {
        let settings = self.settings.get();
        let mut downloads = HashMap::new();
        for (service, kind) in [(Service::Radarr, TitleKind::Movie), (Service::Sonarr, TitleKind::Show)] {
            let wanted: Vec<(&RequestRow, i64)> = rows
                .iter()
                .filter(|r| r.kind() == kind && r.status() == RequestStatus::Approved)
                .filter_map(|r| Some((r, r.arr_id?)))
                .collect();
            let Some(to) = service.saved(&settings).filter(|_| !wanted.is_empty()) else { continue };
            let Some(queue) = self.arr.queue(service, &to) else { continue };
            for (row, arr_id) in wanted {
                let seasons = row.seasons();
                if let Some(download) = arr::download(&arr::records_of(&queue, arr_id, seasons.as_deref())) {
                    downloads.insert(row.id, download);
                }
            }
        }
        downloads
    }

    /// `GET /api/requests`: those `caller` made or follows (everyone's, for
    /// managers but with `mine`) that every filter keeps; the counts of
    /// each status with every filter but the statuses, and for managers who
    /// made those every filter but the maker keeps.
    pub async fn list(&self, caller: &Caller<'_>, query: ListQuery) -> RequestResult<RequestList> {
        let user = caller.user;
        let everyone = caller.manager && !query.mine;
        // Only managers list by maker.
        let maker = query.user_id.filter(|_| caller.manager);
        let like = query.q.as_deref().map(|q| format!("%{}%", like_escape(q)));
        let seen_at = if query.unseen { self.seen_at(user.id).await? } else { 0 };
        let every: &[RequestStatus] = &[];
        // With the statuses and the maker given (none: every one).
        let conditions = |statuses: &[RequestStatus], maker: Option<i64>| {
            let mut sql = format!("(? OR {MINE})");
            if !statuses.is_empty() {
                sql.push_str(&format!(" AND r.status IN ({})", vec!["?"; statuses.len()].join(",")));
            }
            if query.attention {
                sql.push_str(" AND r.status IN ('pending', 'failed')");
            }
            if query.kind.is_some() {
                sql.push_str(" AND r.kind = ?");
            }
            if maker.is_some() {
                sql.push_str(" AND r.user_id = ?");
            }
            if like.is_some() {
                sql.push_str(" AND r.title LIKE ? ESCAPE '^'");
            }
            if query.changed_since.is_some() {
                sql.push_str(" AND r.updated_at > ?");
            }
            if query.unseen {
                sql.push_str(&format!(" AND {MINE} AND r.news_at > ?"));
            }
            sql
        };
        macro_rules! bind_filter {
            ($query:expr, $statuses:expr, $maker:expr) => {{
                let mut bound = $query.bind(everyone).bind(user.id).bind(user.id);
                for status in $statuses {
                    bound = bound.bind(status.as_str());
                }
                if let Some(kind) = query.kind {
                    bound = bound.bind(kind.as_str());
                }
                if let Some(id) = $maker {
                    bound = bound.bind(id);
                }
                if let Some(like) = &like {
                    bound = bound.bind(like.as_str());
                }
                if let Some(at) = query.changed_since {
                    bound = bound.bind(at);
                }
                if query.unseen {
                    bound = bound.bind(user.id).bind(user.id).bind(seen_at);
                }
                bound
            }};
        }
        let sql =
            format!("SELECT r.status, COUNT(*) FROM requests r WHERE {} GROUP BY r.status", conditions(every, maker));
        let counted = bind_filter!(sqlx::query_as::<_, (String, i64)>(sqlx::AssertSqlSafe(sql.as_str())), every, maker)
            .fetch_all(&self.db.read)
            .await?;
        let mut counts: BTreeMap<RequestStatus, i64> = RequestStatus::ALL.into_iter().map(|s| (s, 0)).collect();
        for (status, count) in counted {
            if let Some(status) = RequestStatus::parse(&status) {
                counts.insert(status, count);
            }
        }
        let total = match query.statuses.as_slice() {
            [] => counts.values().sum(),
            statuses => statuses.iter().map(|s| counts[s]).sum(),
        };
        // Unseen ones by their news.
        let order = match (query.unseen, query.oldest) {
            (false, false) => "r.created_at DESC, r.id DESC",
            (false, true) => "r.created_at, r.id",
            (true, false) => "r.news_at DESC, r.id DESC",
            (true, true) => "r.news_at, r.id",
        };
        let limit = query.limit.unwrap_or(LIST_LIMIT).clamp(1, LIST_MAX);
        let sql = format!(
            "SELECT {COLUMNS} FROM {TABLES} WHERE {} ORDER BY {order} LIMIT ? OFFSET ?",
            conditions(&query.statuses, maker)
        );
        let rows =
            bind_filter!(sqlx::query_as::<_, RequestRow>(sqlx::AssertSqlSafe(sql.as_str())), &query.statuses, maker)
                .bind(limit)
                .bind(query.offset.max(0))
                .fetch_all(&self.db.read)
                .await?;
        let mut people = Vec::new();
        if caller.manager {
            let sql = format!(
                "SELECT u.id, COALESCE(NULLIF(TRIM(u.display_name), ''), u.username) AS name, u.avatar, u.avatar_color
                 FROM users u WHERE u.id IN (SELECT r.user_id FROM requests r WHERE {})
                 ORDER BY name COLLATE NOCASE, u.id",
                conditions(&query.statuses, None)
            );
            let found = bind_filter!(
                sqlx::query_as::<_, (i64, String, Option<String>, Option<String>)>(sqlx::AssertSqlSafe(sql.as_str())),
                &query.statuses,
                None::<i64>
            )
            .fetch_all(&self.db.read)
            .await?;
            people = found
                .into_iter()
                .map(|(id, name, avatar, avatar_color)| RequestUser {
                    id,
                    name,
                    avatar_url: avatar_url(id, avatar.as_deref()),
                    avatar_color,
                })
                .collect();
        }
        Ok(RequestList { total, counts, requests: self.dtos(caller, rows).await?, people })
    }

    /// `GET /api/requests/{id}`.
    pub async fn get(&self, caller: &Caller<'_>, id: i64) -> RequestResult<MediaRequest> {
        self.one(caller, id).await
    }

    /// A request `caller` may see.
    async fn one(&self, caller: &Caller<'_>, id: i64) -> RequestResult<MediaRequest> {
        let row = self.row_for(caller, id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        let rows = [row];
        let viewer = self.viewer(caller, &rows).await?;
        let downloads = self.downloads(&rows);
        Ok(rows[0].to_dto(&viewer, downloads.get(&id).cloned()))
    }

    /// Whether `user` may read a request naming account `id` (its maker,
    /// or who decided it): apps load their pictures, beside those of their
    /// household.
    pub async fn names(&self, user: &AuthUser, id: i64) -> sqlx::Result<bool> {
        if !self.enabled() || !user.allowed(Scope::Requests) {
            return Ok(false);
        }
        // Managers read everyone's requests; others, who decided theirs
        // (but before they inherited them).
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM requests r
                            WHERE (? AND (r.user_id = ? OR r.decided_by = ?))
                               OR (r.user_id = ? AND r.decided_by = ?
                                   AND (r.inherited_at IS NULL OR r.decided_at > r.inherited_at)))",
        )
        .bind(user.manages_requests())
        .bind(id)
        .bind(id)
        .bind(user.id)
        .bind(id)
        .fetch_one(&self.db.read)
        .await
    }

    /// `GET /api/discover/images/{kind}/{file}`: one of TMDB's pictures, at
    /// the size TMDB has for a client drawing it `width` pixels wide. An
    /// account that may not ask, and so has no Discover, gets those of the
    /// requests it made or follows only.
    pub async fn picture(
        &self,
        caller: &Caller<'_>,
        kind: &str,
        file: &str,
        width: Option<u32>,
    ) -> RequestResult<Bytes> {
        let missing = || RequestError::from(AppError::not_found("no such picture"));
        let kind = Picture::parse(kind).filter(|_| pictures::valid_file(file)).ok_or_else(missing)?;
        if caller.access == RequestAccess::Off && !self.pictured(caller.user.id, kind, file).await? {
            return Err(RequestError::NoAccess);
        }
        match self.pictures.get(kind.size(width), file).await {
            Ok(bytes) => Ok(bytes),
            Err(Unavailable::Missing) => Err(missing()),
            Err(Unavailable::Failed) => Err(RequestError::PictureDown),
        }
    }

    /// Whether TMDB's `file` is the poster or backdrop (`kind`) of a
    /// request account `user_id` made or follows.
    async fn pictured(&self, user_id: i64, kind: Picture, file: &str) -> sqlx::Result<bool> {
        let column = match kind {
            Picture::Poster => "poster_path",
            Picture::Backdrop => "backdrop_path",
            Picture::Profile | Picture::Still => return Ok(false),
        };
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS (SELECT 1 FROM requests r WHERE r.{column} = ? AND {MINE})"
        )))
        .bind(format!("/{file}"))
        .bind(user_id)
        .bind(user_id)
        .fetch_one(&self.db.read)
        .await
    }

    // -------------------------------------------------------------- changing

    /// `POST /api/requests`: `caller` asks for a title, or some of a show's
    /// seasons (none named: [`default_seasons`]). It follows each active
    /// request asking for some of that already (its own change nothing);
    /// approved as it asks, it approves those of them still pending. What
    /// is left is a new request: `true` with it, `false` with the request
    /// asking for the lowest season asked for when nothing is left. All of
    /// it or nothing: refused, nothing is followed or approved. A request
    /// that stopped asking for the title meanwhile is left out, as if it
    /// weren't there.
    pub async fn create(self: &Arc<Self>, caller: &Caller<'_>, new: NewRequest) -> RequestResult<(bool, MediaRequest)> {
        let user = caller.user;
        match (new.kind, new.seasons.as_deref()) {
            (TitleKind::Movie, Some(_)) => return Err(AppError::bad_request("movies have no seasons").into()),
            (TitleKind::Show, Some([])) => return Err(AppError::bad_request("ask for at least one season").into()),
            _ => {}
        }
        let details = self.title(caller, new.kind, new.tmdb_id).await?;
        if restricted(user, details.certification.as_deref()) {
            return Err(RequestError::Restricted(restriction(details.certification.as_deref())));
        }
        if let Some(n) = new.seasons.iter().flatten().find(|n| !details.seasons.iter().any(|s| s.number == **n)) {
            return Err(AppError::bad_request(format!("TMDB lists no season {n} of {}", details.title)).into());
        }
        let making = self.making.lock().await;
        // Looked at again while a request it found changes meanwhile.
        let mut asked = None;
        for _ in 0..ASK_LOOKS {
            asked = self.ask(caller, &new, &details).await?;
            if asked.is_some() {
                break;
            }
        }
        let Some(Asked { wanted, covering, follow, approved, made, sends }) = asked else {
            return Err(AppError::conflict("this title's requests are changing: ask again in a moment").into());
        };
        drop(making);
        for id in &follow {
            tracing::info!("{} follows request {id} ({})", user.username, details.title);
        }
        let mut touched: Vec<i64> = follow.iter().chain(&approved).copied().collect();
        touched.sort_unstable();
        touched.dedup();
        for id in touched {
            if approved.contains(&id)
                && let Some(row) = self.row(id).await?
            {
                self.decided(&row, user.id).await?;
            }
            self.changed(id, new.kind, new.tmdb_id).await;
        }
        if let Some(id) = made {
            tracing::info!("{} asked for {} (request {id})", user.username, details.title);
            if let Some(row) = self.row(id).await? {
                self.created(&row);
            }
            self.changed(id, new.kind, new.tmdb_id).await;
        }
        // The new request, or the one asking for the lowest season asked
        // for (its own first, then the newest).
        let lowest = wanted.first().copied();
        let answered = made.or_else(|| {
            covering
                .iter()
                .filter(|a| lowest.is_none_or(|n| a.seasons.contains(&n)))
                .max_by_key(|a| (a.mine, a.id))
                .map(|a| a.id)
        });
        let Some(answered) = answered else {
            return Err(AppError::conflict("this title is asked for already").into());
        };
        let answer = self.one(caller, answered).await?;
        if sends {
            for id in approved.iter().copied().chain(made) {
                self.send(id);
            }
        }
        Ok((made.is_some(), answer))
    }

    /// What [`Self::create`] does under `making`: looks at what the
    /// libraries and requests have of the title, and follows, approves and
    /// makes requests as it says, in one write. `None`, with nothing
    /// written, when an active request it found no longer asks for the
    /// title: approving, declining and deleting don't wait for asking, and
    /// it was declined, deleted or became available meanwhile.
    async fn ask(&self, caller: &Caller<'_>, new: &NewRequest, details: &Details) -> RequestResult<Option<Asked>> {
        let user = caller.user;
        let mark = self.marks(caller, new.kind, &[Known::of(details)]).await?.remove(&new.tmdb_id).unwrap_or_default();
        // The seasons asked for that no library it can open has whole (none
        // for a movie), and the active requests asking for any of them.
        let wanted = match new.kind {
            TitleKind::Movie if mark.item_id.is_some() => {
                return Err(RequestError::InLibrary("a library this account can open has this movie already".into()));
            }
            TitleKind::Movie => Vec::new(),
            TitleKind::Show => {
                let counts = self.season_counts(&mark.shows).await?;
                let mut wanted = match &new.seasons {
                    Some(asked) => asked.clone(),
                    None => default_seasons(details, &counts, &mark.declined_seasons),
                };
                wanted.sort_unstable();
                wanted.dedup();
                if wanted.is_empty() {
                    let message = format!("TMDB lists no seasons of {} yet", details.title);
                    return Err(AppError::bad_request(message).into());
                }
                wanted.retain(|n| !details.seasons.iter().any(|s| s.number == *n && whole(&counts, s) == Whole::Yes));
                if wanted.is_empty() {
                    let message = "the libraries this account can open have these seasons whole";
                    return Err(RequestError::InLibrary(message.into()));
                }
                wanted
            }
        };
        let covering: Vec<Active> = mark
            .active
            .iter()
            .filter(|a| new.kind == TitleKind::Movie || a.seasons.iter().any(|n| wanted.contains(n)))
            .cloned()
            .collect();
        let asked = mark.asked();
        let left: Vec<i64> = wanted.iter().copied().filter(|n| !asked.contains(n)).collect();
        let make = match new.kind {
            TitleKind::Movie => covering.is_empty(),
            TitleKind::Show => !left.is_empty(),
        };
        if make {
            // Declined for it lately: the movie, or some of the seasons left.
            let declined: Vec<i64> = left.iter().copied().filter(|n| mark.declined_seasons.contains_key(n)).collect();
            let again_at = match new.kind {
                TitleKind::Movie => mark.refusal.as_ref().filter(|_| mark.declined).map(|r| r.again_at),
                TitleKind::Show => declined.iter().filter_map(|n| mark.declined_seasons.get(n)).max().copied(),
            };
            if let Some(retry_at) = again_at {
                let message = declined_message(new.kind, &declined, retry_at);
                return Err(RequestError::Declined { message, retry_at });
            }
            let quota = self.quota(caller).await?;
            quota.allows(new.kind, if new.kind == TitleKind::Movie { 1 } else { left.len() as i64 })?;
        }
        let auto = caller.access == RequestAccess::Auto;
        let follow: Vec<i64> = covering.iter().filter(|a| !a.mine && !a.follows).map(|a| a.id).collect();
        let pending: Vec<i64> = covering.iter().filter(|a| auto && !a.mine && a.pending).map(|a| a.id).collect();
        let service = match new.kind {
            TitleKind::Movie => Service::Radarr,
            TitleKind::Show => Service::Sonarr,
        };
        // What it approves is sent where its service is connected.
        let sends = auto && service.connected(&self.settings.get()).is_some();
        let at = now();
        let mut tx = self.db.write.begin().await?;
        // The requests it counts on still ask for the title, as this write
        // (the only one at a time) sees them; else it looks again.
        if !covering.is_empty() {
            let sql = format!(
                "SELECT COUNT(*) FROM requests WHERE id IN ({}) AND status IN {ACTIVE}",
                vec!["?"; covering.len()].join(",")
            );
            let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()));
            for active in &covering {
                query = query.bind(active.id);
            }
            if query.fetch_one(&mut *tx).await? != covering.len() as i64 {
                return Ok(None);
            }
        }
        for id in &follow {
            sqlx::query("INSERT OR IGNORE INTO request_watchers (request_id, user_id, created_at) VALUES (?, ?, ?)")
                .bind(id)
                .bind(user.id)
                .bind(at)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE requests SET updated_at = ? WHERE id = ?").bind(at).bind(id).execute(&mut *tx).await?;
        }
        // Approved as if just asked for, by this account.
        let mut approved = Vec::new();
        for id in pending {
            let done = sqlx::query(
                "UPDATE requests SET status = 'approved', reason = NULL, decided_by = ?, decided_at = ?, sending_at = ?,
                                     updated_at = ?
                 WHERE id = ? AND status = 'pending'",
            )
            .bind(user.id)
            .bind(at)
            .bind(sends.then_some(at))
            .bind(at)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if done == 1 {
                approved.push(id);
            }
        }
        let made: Option<i64> = if make {
            let seasons = (new.kind == TitleKind::Show).then(|| seasons_json(&left));
            let id = sqlx::query_scalar(
                "INSERT INTO requests (kind, tmdb_id, tvdb_id, imdb_id, title, year, poster_path, backdrop_path,
                                       seasons, anime, user_id, status, decided_by, decided_at, sending_at, created_at,
                                       updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(new.kind.as_str())
            .bind(new.tmdb_id)
            .bind(details.tvdb_id)
            .bind(details.imdb_id.as_deref())
            .bind(&details.title)
            .bind(details.year)
            .bind(details.poster_path.as_deref())
            .bind(details.backdrop_path.as_deref())
            .bind(seasons)
            .bind(details.anime)
            .bind(user.id)
            .bind(if auto { "approved" } else { "pending" })
            .bind(auto.then_some(user.id))
            .bind(auto.then_some(at))
            .bind(sends.then_some(at))
            .bind(at)
            .bind(at)
            .fetch_one(&mut *tx)
            .await?;
            Some(id)
        } else {
            None
        };
        tx.commit().await?;
        Ok(Some(Asked { wanted, covering, follow, approved, made, sends }))
    }

    /// `POST /api/requests/{id}/approve`: a pending, failed, declined, or
    /// approved and not sent request is approved, keeping `approval`'s
    /// seasons of those asked for, and sent to Radarr or Sonarr in the
    /// background with what it chose, kept with it until then. One sent
    /// already, or being sent, is answered as it is, as is one approved
    /// meanwhile (a race).
    pub async fn approve(
        self: &Arc<Self>,
        caller: &Caller<'_>,
        id: i64,
        approval: Approval,
    ) -> RequestResult<MediaRequest> {
        let approval = Approval { root_folder: approval.root_folder.map(|f| f.trim().to_owned()), ..approval };
        if approval.quality_profile.is_some_and(|p| p <= 0) {
            return Err(AppError::bad_request("a quality profile is chosen by its id").into());
        }
        if approval.root_folder.as_deref().is_some_and(|f| f.is_empty() || f.chars().count() > FOLDER_MAX) {
            return Err(AppError::bad_request("a root folder is a folder's path").into());
        }
        if approval.seasons.as_ref().is_some_and(Vec::is_empty) {
            return Err(AppError::bad_request("keep at least one season").into());
        }
        let row = self.row(id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        let status = row.status();
        if status == RequestStatus::Approved && (row.sent().is_some() || row.sending()) {
            return self.one(caller, id).await;
        }
        if status == RequestStatus::Available {
            let message =
                |_: &MediaRequest| "this request is available already: there is nothing to approve".to_owned();
            return Err(self.refused(caller, id, None, message).await);
        }
        let seasons = match (row.kind(), approval.seasons.as_deref()) {
            (_, None) => row.seasons(),
            (TitleKind::Movie, Some(_)) => return Err(AppError::bad_request("movies have no seasons").into()),
            (TitleKind::Show, Some(kept)) => {
                let asked = row.seasons().unwrap_or_default();
                if let Some(n) = kept.iter().find(|n| !asked.contains(n)) {
                    return Err(AppError::bad_request(format!("season {n} isn't one this request asks for")).into());
                }
                let mut kept = kept.to_vec();
                kept.sort_unstable();
                kept.dedup();
                Some(kept)
            }
        };
        // Those left out aren't asked for any more.
        let ready: Vec<i64> = seasons_of(&row.available_seasons)
            .into_iter()
            .filter(|n| seasons.as_ref().is_some_and(|s| s.contains(n)))
            .collect();
        let sends = row.service().connected(&self.settings.get()).is_some();
        let at = now();
        // Being sent, with what it chose, from the moment it is approved: a
        // restart doesn't lose the send ([`Self::resume`]).
        let approved = sqlx::query(
            "UPDATE requests SET status = 'approved', reason = NULL,
                    decided_by = CASE WHEN status = 'approved' THEN decided_by ELSE ? END,
                    decided_at = CASE WHEN status = 'approved' THEN decided_at ELSE ? END,
                    seasons = ?, available_seasons = ?, sent_to = NULL, sent_profile = NULL, sent_folder = NULL,
                    sending_at = ?, send_profile = ?, send_folder = ?, updated_at = ?
             WHERE id = ? AND status = ? AND sent_to IS ? AND sending_at IS NULL",
        )
        .bind(caller.user.id)
        .bind(at)
        .bind(seasons.as_deref().map(seasons_json))
        .bind(seasons_json(&ready))
        .bind(sends.then_some(at))
        .bind(approval.quality_profile.filter(|_| sends))
        .bind(approval.root_folder.filter(|_| sends))
        .bind(at)
        .bind(id)
        .bind(&row.status)
        .bind(row.sent_to.as_deref())
        .execute(&self.db.write)
        .await?
        .rows_affected()
            == 1;
        if !approved {
            // Changed meanwhile: approved by another session by now (which
            // may not have started sending it yet), or not to be approved.
            if self.row(id).await?.is_some_and(|r| r.status() == RequestStatus::Approved) {
                return self.one(caller, id).await;
            }
            let message = |r: &MediaRequest| format!("this request changed meanwhile: it is {} now", r.status.as_str());
            return Err(self.refused(caller, id, None, message).await);
        }
        let row = self.row(id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        tracing::info!("{} approved request {id} ({})", caller.user.username, row.title);
        if matches!(status, RequestStatus::Pending | RequestStatus::Declined) {
            self.decided(&row, caller.user.id).await?;
        }
        self.changed(id, row.kind(), row.tmdb_id).await;
        let answer = self.one(caller, id).await?;
        if sends {
            self.send(id);
        }
        Ok(answer)
    }

    /// Sends request `id`, recorded as being sent, to Radarr or Sonarr in
    /// the background ([`Self::deliver`]): every send goes through here, and
    /// one at a time for a request. Should how it went not be saved, the
    /// request is `failed` for a manager where that can be saved; where not,
    /// it is sent again by [`Self::resume`].
    fn send(self: &Arc<Self>, id: i64) {
        if !self.sending.send_if_modified(|sending| sending.insert(id)) {
            return;
        }
        let requests = self.clone();
        tokio::spawn(async move {
            let _done = Sending(requests.clone(), id);
            if let Err(err) = requests.deliver(id).await {
                tracing::warn!("request {id}: sending it failed: {err:#}");
                if let Err(err) = requests.unsaved(id, &err).await {
                    tracing::warn!("request {id}: it is to be sent again: {err:#}");
                }
            }
        });
    }

    /// Sends again the requests recorded as being sent whose send isn't
    /// running (a restart cut it short, or how it went couldn't be saved),
    /// with what their approvals chose; at the start and at every look at
    /// the libraries, while requests are on.
    pub async fn resume(self: &Arc<Self>) -> sqlx::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM requests WHERE status = 'approved' AND sent_to IS NULL AND sending_at IS NOT NULL
             ORDER BY sending_at, id",
        )
        .fetch_all(&self.db.read)
        .await?;
        for id in ids {
            self.send(id);
        }
        Ok(())
    }

    /// Waits until no send runs.
    pub async fn settled(&self) {
        let mut sending = self.sending.subscribe();
        let _ = sending.wait_for(HashSet::is_empty).await;
    }

    /// Adds request `id`, being sent, to Radarr or Sonarr with what its
    /// approval chose, unless it no longer is to be sent; should that fail,
    /// the request is `failed` for a manager, saying why. With its kind's
    /// service no longer connected, it stays approved, not sent.
    async fn deliver(self: &Arc<Self>, id: i64) -> sqlx::Result<()> {
        let Some(row) = self.row(id).await? else { return Ok(()) };
        if !row.sending() {
            return Ok(());
        }
        let settings = self.settings.get();
        let service = row.service();
        let Some(to) = service.connected(&settings) else {
            sqlx::query(
                "UPDATE requests SET sending_at = NULL, send_profile = NULL, send_folder = NULL, updated_at = ?
                 WHERE id = ? AND status = 'approved' AND sent_to IS NULL",
            )
            .bind(now())
            .bind(id)
            .execute(&self.db.write)
            .await?;
            self.changed(id, row.kind(), row.tmdb_id).await;
            return Ok(());
        };
        let seasons = row.seasons().unwrap_or_default();
        let title = arr::Title {
            tmdb_id: row.tmdb_id,
            tvdb_id: row.tvdb_id,
            anime: row.anime,
            seasons: &seasons,
            quality_profile: row.send_profile,
            root_folder: row.send_folder.as_deref(),
        };
        let added = match service {
            Service::Radarr => self.arr.add_movie(&to, &settings, &title).await,
            Service::Sonarr => self.arr.add_series(&to, &settings, &title).await,
        };
        match added {
            Ok(added) => {
                tracing::info!("request {id}: {} added to {}", row.title, service.name());
                // Saved by its name: the profile's id may go.
                let profile = match added.quality_profile {
                    Some(profile) => {
                        let (lists, _) = self.offered(service, &to).await;
                        lists.quality_profiles.into_iter().find(|p| p.id == profile).map(|p| p.name)
                    }
                    None => None,
                };
                sqlx::query(
                    "UPDATE requests SET arr_id = ?, sent_to = ?, sent_profile = ?, sent_folder = ?, sending_at = NULL,
                                         send_profile = NULL, send_folder = NULL, updated_at = ?
                     WHERE id = ? AND status = 'approved' AND sent_to IS NULL",
                )
                .bind(added.id)
                .bind(service.as_str())
                .bind(profile)
                .bind(added.root_folder)
                .bind(now())
                .bind(id)
                .execute(&self.db.write)
                .await?;
            }
            Err(err) => {
                tracing::warn!("request {id}: {} wasn't added to {}: {err}", row.title, service.name());
                self.fail(&row, err.to_string()).await?;
            }
        }
        self.changed(id, row.kind(), row.tmdb_id).await;
        Ok(())
    }

    /// Request `row`, being sent, couldn't be: it is `failed` for a manager,
    /// saying why (`reason`), and managers hear it. `false`: it no longer
    /// was being sent.
    async fn fail(&self, row: &RequestRow, reason: String) -> sqlx::Result<bool> {
        let failed = sqlx::query(
            "UPDATE requests SET status = 'failed', reason = ?, sending_at = NULL, send_profile = NULL,
                                 send_folder = NULL, updated_at = ?
             WHERE id = ? AND status = 'approved' AND sent_to IS NULL AND sending_at IS NOT NULL",
        )
        .bind(&reason)
        .bind(now())
        .bind(row.id)
        .execute(&self.db.write)
        .await?
        .rows_affected();
        if failed == 1 {
            self.events.send(ServerEvent::RequestFailed {
                to: Audience::of(Vec::new()),
                request_id: row.id,
                title: row.title.clone(),
                kind: row.kind(),
                reason,
                retry_at: None,
            });
        }
        Ok(failed == 1)
    }

    /// Request `id`'s send ended with how it went not saved (`err`): it is
    /// `failed`, so that a manager sees it and approves it again (finding it
    /// in Radarr or Sonarr, should it have got there).
    async fn unsaved(&self, id: i64, err: &sqlx::Error) -> sqlx::Result<()> {
        let Some(row) = self.row(id).await? else { return Ok(()) };
        if self.fail(&row, format!("Pley couldn't save how sending it went: {err}")).await? {
            self.changed(id, row.kind(), row.tmdb_id).await;
        }
        Ok(())
    }

    /// `POST /api/requests/{id}/decline`: a pending, failed, or approved
    /// one not available yet.
    pub async fn decline(&self, caller: &Caller<'_>, id: i64, reason: Option<String>) -> RequestResult<MediaRequest> {
        let reason = reason.map(|r| r.trim().to_owned()).filter(|r| !r.is_empty());
        if reason.as_ref().is_some_and(|r| r.chars().count() > REASON_MAX) {
            return Err(AppError::bad_request(format!("a reason is {REASON_MAX} characters at most")).into());
        }
        if self.row(id).await?.is_none() {
            return Err(AppError::not_found("no such request").into());
        }
        let at = now();
        let declined = sqlx::query(
            "UPDATE requests SET status = 'declined', reason = ?, decided_by = ?, decided_at = ?, updated_at = ?,
                                 news_at = ?, sending_at = NULL, send_profile = NULL, send_folder = NULL
             WHERE id = ? AND status IN ('pending', 'failed', 'approved')",
        )
        .bind(reason)
        .bind(caller.user.id)
        .bind(at)
        .bind(at)
        .bind(at)
        .bind(id)
        .execute(&self.db.write)
        .await?
        .rows_affected();
        if declined == 0 {
            let message = |r: &MediaRequest| {
                format!("this request is {}: only pending, failed and approved ones are declined", r.status.as_str())
            };
            return Err(self.refused(caller, id, None, message).await);
        }
        let row = self.row(id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        tracing::info!("{} declined request {id} ({})", caller.user.username, row.title);
        self.decided(&row, caller.user.id).await?;
        self.changed(id, row.kind(), row.tmdb_id).await;
        self.one(caller, id).await
    }

    /// `POST /api/requests/{id}/handled`: approved as added to Radarr or
    /// Sonarr by hand, so never sent: a pending, failed, declined, or
    /// approved and not sent one. One added by hand already, or meanwhile
    /// (a race), is answered as it is.
    pub async fn handled(&self, caller: &Caller<'_>, id: i64) -> RequestResult<MediaRequest> {
        let row = self.row(id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        let status = row.status();
        if status == RequestStatus::Approved && row.hand() {
            return self.one(caller, id).await;
        }
        let unsent = status == RequestStatus::Approved && row.sent_to.is_none() && !row.sending();
        if !(unsent || matches!(status, RequestStatus::Pending | RequestStatus::Failed | RequestStatus::Declined)) {
            let message = |r: &MediaRequest| {
                format!("this request is {}: only one not sent yet is marked as added by hand", r.status.as_str())
            };
            return Err(self.refused(caller, id, None, message).await);
        }
        let at = now();
        let done = sqlx::query(
            "UPDATE requests SET status = 'approved', reason = NULL, sent_to = 'hand', sent_profile = NULL,
                    sent_folder = NULL, decided_by = CASE WHEN status = 'approved' THEN decided_by ELSE ? END,
                    decided_at = CASE WHEN status = 'approved' THEN decided_at ELSE ? END, updated_at = ?
             WHERE id = ? AND status = ? AND sent_to IS ? AND sending_at IS NULL",
        )
        .bind(caller.user.id)
        .bind(at)
        .bind(at)
        .bind(id)
        .bind(&row.status)
        .bind(row.sent_to.as_deref())
        .execute(&self.db.write)
        .await?
        .rows_affected();
        if done == 0 {
            // Changed meanwhile: marked by another session by now, or not
            // to be marked (being sent, say).
            if self.row(id).await?.is_some_and(|r| r.status() == RequestStatus::Approved && r.hand()) {
                return self.one(caller, id).await;
            }
            let message = |r: &MediaRequest| format!("this request changed meanwhile: it is {} now", r.status.as_str());
            return Err(self.refused(caller, id, None, message).await);
        }
        let row = self.row(id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        tracing::info!("{} marked request {id} ({}) as added by hand", caller.user.username, row.title);
        if matches!(status, RequestStatus::Pending | RequestStatus::Declined) {
            self.decided(&row, caller.user.id).await?;
        }
        self.changed(id, row.kind(), row.tmdb_id).await;
        self.one(caller, id).await
    }

    /// `DELETE /api/requests/{id}`: its maker withdraws it while pending or
    /// failed, or a declined one once its 30 days are over, and its
    /// earliest follower becomes its maker; managers delete any, for
    /// everyone. Nothing is removed from Radarr or Sonarr.
    pub async fn delete(&self, caller: &Caller<'_>, id: i64) -> RequestResult<()> {
        let row = self.row_for(caller, id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        let to = self.audience(id).await?;
        let (kind, tmdb_id) = (row.kind(), row.tmdb_id);
        if caller.manager {
            sqlx::query("DELETE FROM requests WHERE id = ?").bind(id).execute(&self.db.write).await?;
            tracing::info!("{} deleted request {id} ({})", caller.user.username, row.title);
            self.changed_to(to, id, kind, tmdb_id);
            return Ok(());
        }
        if row.user_id != caller.user.id {
            let message = |_: &MediaRequest| "only its maker withdraws a request: stop following it instead".to_owned();
            return Err(self.refused(caller, id, None, message).await);
        }
        if let Some(until) = row.declined_until().filter(|at| *at > now()) {
            let message = |_: &MediaRequest| {
                format!("a declined request stays for 30 days: it may be removed from {}", day(until))
            };
            return Err(self.refused(caller, id, Some(until), message).await);
        }
        let status = row.status();
        if !matches!(status, RequestStatus::Pending | RequestStatus::Failed | RequestStatus::Declined) {
            let message =
                |r: &MediaRequest| format!("this request is {}: an administrator can delete it", r.status.as_str());
            return Err(self.refused(caller, id, None, message).await);
        }
        let at = now();
        let mut tx = self.db.write.begin().await?;
        let same: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM requests WHERE id = ? AND user_id = ? AND status = ?)")
                .bind(id)
                .bind(caller.user.id)
                .bind(&row.status)
                .fetch_one(&mut *tx)
                .await?;
        if !same {
            drop(tx);
            let message = |r: &MediaRequest| format!("this request changed meanwhile: it is {} now", r.status.as_str());
            return Err(self.refused(caller, id, None, message).await);
        }
        let heir = inherit(&mut tx, id, at).await?;
        if heir.is_none() {
            sqlx::query("DELETE FROM requests WHERE id = ?").bind(id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        match heir {
            Some(heir) => tracing::info!(
                "{} withdrew request {id} ({}): account {heir} has it now",
                caller.user.username,
                row.title
            ),
            None => tracing::info!("{} withdrew request {id} ({})", caller.user.username, row.title),
        }
        self.changed_to(to, id, kind, tmdb_id);
        Ok(())
    }

    /// `DELETE /api/requests/{id}/watch`: `caller` follows it no more, but a
    /// declined one before its 30 days are over: they hold for its followers
    /// as for its maker, and one no longer following it could ask again.
    pub async fn unwatch(&self, caller: &Caller<'_>, id: i64) -> RequestResult<()> {
        let row = self.row_for(caller, id).await?.ok_or_else(|| AppError::not_found("no such request"))?;
        let to = self.audience(id).await?;
        let mut tx = self.db.write.begin().await?;
        let stopped = sqlx::query(
            "DELETE FROM request_watchers WHERE request_id = ? AND user_id = ?
               AND NOT EXISTS (SELECT 1 FROM requests WHERE id = ? AND status = 'declined' AND decided_at > ?)",
        )
        .bind(id)
        .bind(caller.user.id)
        .bind(id)
        .bind(now() - DECLINED_FOR)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if stopped == 0 {
            drop(tx);
            let request = self.one(caller, id).await?;
            let until = request
                .decided_at
                .map(|at| at + DECLINED_FOR)
                .filter(|at| request.watching && request.status == RequestStatus::Declined && *at > now());
            let message = match until {
                Some(until) => {
                    format!(
                        "a declined request stays for 30 days: this account may stop following it from {}",
                        day(until)
                    )
                }
                None if request.mine => "this is this account's own request: withdraw it instead".to_owned(),
                None => "this account doesn't follow this request".to_owned(),
            };
            return Err(RequestError::Conflict { message, request: Box::new(request), retry_at: until });
        }
        sqlx::query("UPDATE requests SET updated_at = ? WHERE id = ?").bind(now()).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        self.changed_to(to, id, row.kind(), row.tmdb_id);
        Ok(())
    }

    /// `POST /api/requests/seen`: what had news for `caller` is seen; the
    /// account's other streams hear it (not those of `token`, the session
    /// that saw it), so their badges clear too.
    pub async fn seen(&self, caller: &Caller<'_>, token: Option<&str>) -> RequestResult<()> {
        sqlx::query("UPDATE users SET requests_seen_at = ? WHERE id = ?")
            .bind(now())
            .bind(caller.user.id)
            .execute(&self.db.write)
            .await?;
        self.many_changed(Audience { except_token: token.map(hash_token), ..Audience::only(vec![caller.user.id]) });
        Ok(())
    }

    // -------------------------------------------------------------- choices

    /// `GET /api/requests/options`: what approving may choose in Radarr and
    /// Sonarr, where each is connected.
    pub async fn options(self: &Arc<Self>) -> RequestOptions {
        let settings = self.settings.get();
        let (radarr, sonarr) =
            tokio::join!(self.offer(Service::Radarr, &settings), self.offer(Service::Sonarr, &settings));
        RequestOptions { radarr, sonarr }
    }

    /// What `service` offers, where it is connected, and the settings'
    /// choices.
    async fn offer(self: &Arc<Self>, service: Service, settings: &Settings) -> Option<ArrOptions> {
        let to = service.connected(settings)?;
        let (lists, error) = self.offered(service, &to).await;
        let ((profile, folder), (anime_profile, anime_folder)) = service.chosen(settings);
        let profile_of = |id: i64| (id > 0).then_some(id);
        let folder_of = |path: &str| (!path.is_empty()).then(|| path.to_owned());
        Some(ArrOptions {
            quality_profiles: lists.quality_profiles,
            root_folders: lists.root_folders,
            quality_profile: profile_of(profile),
            root_folder: folder_of(folder),
            anime_quality_profile: profile_of(anime_profile),
            anime_root_folder: folder_of(anime_folder),
            error,
        })
    }

    /// The quality profiles and root folders `service` at `to` offers: as
    /// it last answered, asked again once that is [`OFFER_TTL`] old (one
    /// question at a time) and waited for [`OFFER_WAIT`] at most; with why
    /// not while it doesn't answer.
    async fn offered(self: &Arc<Self>, service: Service, to: &Connection) -> (Lists, Option<String>) {
        let slot = &self.offers[service as usize];
        let mut asking = {
            let mut offer = slot.lock();
            if offer.to.as_ref() != Some(to) {
                *offer = Offer { to: Some(to.clone()), ..Offer::default() };
            }
            if offer.answered.is_some_and(|at| at.elapsed() < OFFER_TTL) {
                return (offer.lists.clone(), offer.error.clone());
            }
            match &offer.asking {
                Some(asking) => asking.clone(),
                None => {
                    let (answered, asking) = watch::channel(false);
                    offer.asking = Some(asking.clone());
                    let (requests, to) = (self.clone(), to.clone());
                    tokio::spawn(async move {
                        let lists = requests.arr.lists(service, &to).await;
                        let mut offer = requests.offers[service as usize].lock();
                        if offer.to.as_ref() == Some(&to) {
                            offer.asking = None;
                            match lists {
                                Ok(lists) => {
                                    offer.lists = lists;
                                    offer.error = None;
                                    offer.answered = Some(Instant::now());
                                }
                                Err(err) => {
                                    tracing::debug!("{err}");
                                    offer.error = Some(not_answering(service, &err));
                                }
                            }
                        }
                        drop(offer);
                        let _ = answered.send(true);
                    });
                    asking
                }
            }
        };
        let answered = matches!(tokio::time::timeout(OFFER_WAIT, asking.wait_for(|done| *done)).await, Ok(Ok(_)));
        let offer = slot.lock();
        let error = if answered { offer.error.clone() } else { Some(format!("{} isn't answering", service.name())) };
        (offer.lists.clone(), error)
    }

    // -------------------------------------------------------------- availability

    /// Looks for what active requests ask for in the libraries: a request
    /// becomes available with its movie, or every season asked for whole.
    pub async fn sync(&self) -> anyhow::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        let _one = self.syncing.lock().await;
        let rows: Vec<RequestRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLUMNS} FROM {TABLES}
             WHERE r.status IN {ACTIVE} OR (r.status = 'available' AND r.item_id IS NULL) ORDER BY r.id"
        )))
        .fetch_all(&self.db.read)
        .await?;
        let sonarr = Service::Sonarr.saved(&self.settings.get());
        for row in rows {
            // As its maker sees the libraries: only those they can open
            // count, as they may ask for what only the others have.
            let library: Vec<i64> = self
                .in_library(row.user_id, row.kind(), &[row.known()])
                .await?
                .into_iter()
                .filter(|l| l.2)
                .map(|l| l.1)
                .collect();
            let Some(&item) = library.first() else { continue };
            if row.status() == RequestStatus::Available {
                // Gone and back (replaced, or scanned again): no news, as
                // it was told of once.
                sqlx::query("UPDATE requests SET item_id = ?, updated_at = ? WHERE id = ? AND item_id IS NULL")
                    .bind(item)
                    .bind(now())
                    .bind(row.id)
                    .execute(&self.db.write)
                    .await?;
                self.changed(row.id, row.kind(), row.tmdb_id).await;
                continue;
            }
            let asked = row.seasons().unwrap_or_default();
            if row.kind() == TitleKind::Movie {
                self.available(&row, item, &asked).await?;
                continue;
            }
            let details = match self.discover.details(TitleKind::Show, row.tmdb_id).await {
                Ok(details) => details,
                Err(err) => {
                    tracing::debug!("request {}: TMDB: {err}", row.id);
                    continue;
                }
            };
            let counts = self.season_counts(&library).await?;
            let mut have = Vec::new();
            for n in &asked {
                let Some(season) = details.seasons.iter().find(|s| s.number == *n) else { continue };
                let whole = match whole(&counts, season) {
                    Whole::Yes => true,
                    Whole::Maybe => match (&sonarr, row.arr_id) {
                        (Some(to), Some(series)) => self.arr.season_complete(to, series, *n).await.unwrap_or(false),
                        _ => false,
                    },
                    Whole::No => false,
                };
                if whole {
                    have.push(*n);
                }
            }
            let before = seasons_of(&row.available_seasons);
            if !asked.is_empty() && have.len() == asked.len() {
                self.available(&row, item, &have).await?;
            } else if have != before || row.item_id != Some(item) {
                // Seasons newly there are news to those waiting.
                let newly: Vec<i64> = have.iter().copied().filter(|n| !before.contains(n)).collect();
                let at = now();
                sqlx::query(
                    "UPDATE requests SET available_seasons = ?, item_id = ?, updated_at = ?,
                                         news_at = CASE WHEN ? THEN ? ELSE news_at END
                     WHERE id = ?",
                )
                .bind(seasons_json(&have))
                .bind(item)
                .bind(at)
                .bind(!newly.is_empty())
                .bind(at)
                .bind(row.id)
                .execute(&self.db.write)
                .await?;
                if !newly.is_empty() {
                    self.ready(&row, Some(newly), false).await?;
                }
                self.changed(row.id, row.kind(), row.tmdb_id).await;
            }
        }
        Ok(())
    }

    /// A request's title is in a library its maker can open: `item`.
    async fn available(&self, row: &RequestRow, item: i64, seasons: &[i64]) -> sqlx::Result<()> {
        let at = now();
        let done = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE requests SET status = 'available', item_id = ?, available_seasons = ?, available_at = ?,
                 updated_at = ?, news_at = ?, sending_at = NULL, send_profile = NULL, send_folder = NULL
             WHERE id = ? AND status IN {ACTIVE}"
        )))
        .bind(item)
        .bind(seasons_json(seasons))
        .bind(at)
        .bind(at)
        .bind(at)
        .bind(row.id)
        .execute(&self.db.write)
        .await?
        .rows_affected();
        if done == 1 {
            tracing::info!("request {}: {} is available", row.id, row.title);
            // A show's: the seasons ready since it was last looked at.
            let before = seasons_of(&row.available_seasons);
            let newly = (row.kind() == TitleKind::Show)
                .then(|| seasons.iter().copied().filter(|n| !before.contains(n)).collect());
            self.ready(row, newly, true).await?;
            self.changed(row.id, row.kind(), row.tmdb_id).await;
        }
        Ok(())
    }

    /// Fetches Radarr's and Sonarr's queues once, and sends what a restart
    /// cut short ([`Self::resume`]) and looks at the libraries at the start,
    /// every 5 minutes and after each scan, until the server stops.
    pub fn start(self: &Arc<Self>, events: &EventBus, stopping: CancellationToken) {
        let queues = self.clone();
        tokio::spawn(async move {
            if queues.enabled() {
                queues.refresh_queues().await;
            }
        });
        let (requests, mut events) = (self.clone(), events.subscribe());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(SYNC_EVERY);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = tick.tick() => {}
                    event = events.recv() => match event {
                        Ok(ServerEvent::ScanFinished { .. }) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        _ => continue,
                    },
                    () = stopping.cancelled() => break,
                }
                if let Err(err) = requests.resume().await {
                    tracing::warn!("requests: sending again what was being sent failed: {err:#}");
                }
                if let Err(err) = requests.sync().await {
                    tracing::warn!("requests: looking at the libraries failed: {err:#}");
                }
            }
        });
    }
}

/// Which of these items `user_id` may open.
async fn visible_items(db: &Db, user_id: i64, ids: &[i64]) -> sqlx::Result<HashSet<i64>> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let sql = format!(
        "{ME_CTE} SELECT i.id FROM items i LEFT JOIN items p ON p.id = i.parent_id
           LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE i.id IN ({}) AND {VISIBLE}",
        vec!["?"; ids.len()].join(",")
    );
    let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id);
    for id in ids {
        query = query.bind(id);
    }
    Ok(query.fetch_all(&db.read).await?.into_iter().collect())
}

/// Request `id`'s earliest follower becomes its maker (it counts toward
/// their limits from `at`) and follows it no more; the others keep
/// following it. `None`: nobody follows it.
async fn inherit(tx: &mut SqliteConnection, id: i64, at: i64) -> sqlx::Result<Option<i64>> {
    let heir: Option<i64> = sqlx::query_scalar(
        "SELECT user_id FROM request_watchers WHERE request_id = ? ORDER BY created_at, user_id LIMIT 1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(heir) = heir else { return Ok(None) };
    sqlx::query("DELETE FROM request_watchers WHERE request_id = ? AND user_id = ?")
        .bind(id)
        .bind(heir)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE requests SET user_id = ?, inherited_at = ?, updated_at = ? WHERE id = ?")
        .bind(heir)
        .bind(at)
        .bind(at)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    Ok(Some(heir))
}

/// Before account `user_id` is deleted, in the same transaction: each of
/// its requests someone follows goes to its earliest follower (the others
/// go with the account). Who to tell once it is gone
/// ([`Requests::account_deleted`]): managers, and those who inherited some;
/// `None` when it made and followed none.
pub async fn hand_down(tx: &mut SqliteConnection, user_id: i64) -> sqlx::Result<Option<Audience>> {
    let made: Vec<i64> = sqlx::query_scalar("SELECT id FROM requests WHERE user_id = ? ORDER BY id")
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await?;
    let follows: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM request_watchers WHERE user_id = ?)")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    if made.is_empty() && !follows {
        return Ok(None);
    }
    let at = now();
    let mut heirs = Vec::new();
    for id in made {
        if let Some(heir) = inherit(tx, id, at).await? {
            tracing::info!("request {id}: its maker's account is being deleted: account {heir} has it now");
            heirs.push(heir);
        }
    }
    heirs.sort_unstable();
    heirs.dedup();
    Ok(Some(Audience::of(heirs)))
}

/// What approvals are told when Radarr or Sonarr didn't answer them.
fn not_answering(service: Service, err: &ArrError) -> String {
    match err {
        ArrError::Unreachable(..) => format!("{} isn't answering", service.name()),
        err => err.to_string(),
    }
}

/// Why asking for a title again is refused for now: the movie, or these
/// seasons, were declined for the account lately.
fn declined_message(kind: TitleKind, seasons: &[i64], retry_at: i64) -> String {
    let what = match (kind, seasons) {
        (TitleKind::Movie, _) => "this movie was".to_owned(),
        (TitleKind::Show, [n]) => format!("season {n} was"),
        (TitleKind::Show, [first @ .., last]) => {
            let first: Vec<String> = first.iter().map(i64::to_string).collect();
            format!("seasons {} and {last} were", first.join(", "))
        }
        (TitleKind::Show, []) => "these seasons were".to_owned(),
    };
    format!("{what} declined for this account lately: it may ask again from {}", day(retry_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn season(number: i64, aired: i64) -> discover::Season {
        discover::Season { number, name: String::new(), episodes: aired, aired, air_date: None, poster_path: None }
    }

    #[test]
    fn a_season_is_whole_with_every_episode_aired() {
        let counts = HashMap::from([(1, (10, 10)), (2, (4, 10)), (3, (3, 5))]);
        assert_eq!(whole(&counts, &season(1, 10)), Whole::Yes);
        assert_eq!(whole(&counts, &season(1, 8)), Whole::Yes, "more than aired");
        assert_eq!(whole(&counts, &season(2, 10)), Whole::Maybe, "the last one there: files of two episodes?");
        assert_eq!(whole(&counts, &season(3, 6)), Whole::No);
        assert_eq!(whole(&counts, &season(4, 0)), Whole::No, "nothing aired yet");
        assert_eq!(whole(&counts, &season(5, 3)), Whole::No);
    }

    #[test]
    fn a_seasons_state_is_the_first_that_holds() {
        let counts = HashMap::from([(1, (10, 10)), (2, (4, 10)), (3, (2, 2))]);
        let asked = HashSet::from([3, 6]);
        assert_eq!(season_state(&season(1, 10), &counts, &asked), SeasonState::Library);
        assert_eq!(season_state(&season(2, 10), &counts, &asked), SeasonState::Partial, "files of two episodes?");
        assert_eq!(season_state(&season(3, 8), &counts, &asked), SeasonState::Requested);
        assert_eq!(season_state(&season(4, 0), &counts, &asked), SeasonState::Unaired);
        assert_eq!(season_state(&season(5, 8), &counts, &asked), SeasonState::Missing);
        assert_eq!(season_state(&season(6, 0), &counts, &asked), SeasonState::Requested, "asked for before it airs");
    }

    fn active(id: i64, seasons: &[i64], theirs: bool) -> Active {
        Active { id, seasons: seasons.to_vec(), theirs, mine: theirs, follows: false, pending: true }
    }

    #[test]
    fn a_titles_status_is_the_first_that_holds() {
        let movie = |mark: &Mark, restricted| status_of(TitleKind::Movie, mark, None, restricted);
        let mut mark = Mark::default();
        assert_eq!(movie(&mark, false), TitleStatus::None);
        mark.declined = true;
        assert_eq!(movie(&mark, false), TitleStatus::Declined);
        mark.active.push(active(7, &[], false));
        assert_eq!(movie(&mark, false), TitleStatus::Requested);
        assert_eq!((mark.request_id(), mark.watching()), (None, false), "someone else's");
        assert_eq!(movie(&mark, true), TitleStatus::Restricted);
        mark.item_id = Some(3);
        assert_eq!(movie(&mark, true), TitleStatus::Library);

        // A library show lacking seasons 2 and 3.
        let show = |mark: &Mark, lacking: &[i64]| status_of(TitleKind::Show, mark, Some(lacking), false);
        let mut mark = Mark { item_id: Some(3), ..Mark::default() };
        assert_eq!(show(&mark, &[]), TitleStatus::Library);
        assert_eq!(show(&mark, &[2, 3]), TitleStatus::Partial);
        mark.active.push(active(8, &[2], true));
        assert_eq!(show(&mark, &[2, 3]), TitleStatus::Partial);
        mark.declined_seasons.insert(3, 1_000);
        assert_eq!(show(&mark, &[2, 3]), TitleStatus::Declined, "what is left was declined");
        mark.active.push(active(9, &[3], false));
        assert_eq!(show(&mark, &[2, 3]), TitleStatus::Requested, "everything it lacks is asked for");
        assert_eq!((mark.request_id(), mark.season_request(2), mark.season_request(3)), (Some(8), Some(8), None));
        let unknown = Mark { active: vec![active(8, &[1], true)], ..Mark::default() };
        assert_eq!(status_of(TitleKind::Show, &unknown, None, false), TitleStatus::Requested, "seasons not known");
        assert_eq!(show(&Mark::default(), &[1]), TitleStatus::None);
    }

    #[test]
    fn age_limits_keep_unrated_titles_out() {
        let user = |max_age| AuthUser {
            id: 2,
            username: "ann".into(),
            is_admin: false,
            account_admin: false,
            all_libraries: true,
            max_age,
            app: None,
        };
        assert!(!restricted(&user(None), None));
        assert!(restricted(&user(Some(13)), None), "unrated");
        assert!(restricted(&user(Some(13)), Some("R")));
        assert!(!restricted(&user(Some(13)), Some("PG-13")));
        assert_eq!(restriction(Some("R")), "Rated R: above this account's age limit");
        assert_eq!(restriction(Some(" ")), "Not rated: with an age limit, this account may ask for rated titles only");
    }

    #[test]
    fn limits_say_when_enough_stops_counting() {
        let at = crate::util::date_secs("2026-11-03").unwrap() + 14 * 3600 + 5 * 60;
        let quota = Quota {
            movies_limit: Some(1),
            seasons_limit: Some(5),
            movies: vec![(at - LIMIT_WINDOW, 1)],
            seasons: vec![(at - LIMIT_WINDOW, 2), (at + 3600 - LIMIT_WINDOW, 1)],
        };
        assert_eq!((quota.left(TitleKind::Movie), quota.left(TitleKind::Show)), (Some(0), Some(2)));
        assert_eq!((quota.free_at(TitleKind::Movie, 1), quota.free_at(TitleKind::Show, 1)), (Some(at), None));
        let Err(RequestError::Limit { message, retry_at }) = quota.allows(TitleKind::Movie, 1) else { panic!() };
        assert_eq!(
            (message.as_str(), retry_at),
            (
                "this account has asked for all the movies it may for now: it may ask for more from 2026-11-03 14:05 UTC",
                Some(at)
            )
        );
        assert!(quota.allows(TitleKind::Show, 2).is_ok());
        // Four seasons need the oldest request's two to stop counting; five
        // need all three.
        let Err(RequestError::Limit { message, retry_at }) = quota.allows(TitleKind::Show, 4) else { panic!() };
        assert_eq!((message.as_str(), retry_at), ("this account may ask for 2 more seasons for now", Some(at)));
        assert_eq!(quota.free_at(TitleKind::Show, 5), Some(at + 3600));
        let Err(RequestError::Limit { message, retry_at }) = quota.allows(TitleKind::Show, 6) else { panic!() };
        assert_eq!(
            (message.as_str(), retry_at),
            ("this account may ask for at most 5 seasons per 7 days: ask for fewer", None)
        );
        let none = Quota::default();
        assert!(none.allows(TitleKind::Movie, 100).is_ok(), "no limit");
        assert_eq!((none.left(TitleKind::Movie), none.free_at(TitleKind::Movie, 1)), (None, None));
    }

    #[test]
    fn access_and_statuses_read_as_stored() {
        for access in [RequestAccess::Off, RequestAccess::Ask, RequestAccess::Auto] {
            assert_eq!(RequestAccess::parse(access.as_str()), access);
        }
        assert_eq!(RequestAccess::parse("later"), RequestAccess::Ask);
        for status in RequestStatus::ALL {
            assert_eq!(RequestStatus::parse(status.as_str()), Some(status));
            assert_eq!(serde_json::to_value(status).unwrap(), json!(status.as_str()));
        }
        assert_eq!(RequestStatus::parse("lost"), None);
    }
}
