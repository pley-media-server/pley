//! Requests (`/api/requests`) and Discover (`/api/discover`): asking for
//! movies and shows the libraries don't have (see `crate::requests`). Every
//! route answers in the requests' `{ code, message }` (malformed requests
//! too, never the framework's text), checking in one order: requests are
//! on, who asks may, then the query and the body, then the rest.

use std::collections::HashMap;

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::truthy;
use crate::auth::{AuthUser, CurrentToken};
use crate::discover::{List, MAX_PAGE, TitleKind, pictures};
use crate::error::{ApiResult, AppError};
use crate::requests::{
    Approval, DiscoverDetail, DiscoverPage, ItemRequest, ListQuery, MediaRequest, NewRequest, RequestList,
    RequestOptions, RequestResult, RequestStatus, Summary,
};
use crate::state::AppState;

/// What a search may be at most.
const QUERY_MAX: usize = 200;

type Params = Result<Query<HashMap<String, String>>, QueryRejection>;

/// A query's parameters.
fn params(query: Params) -> ApiResult<HashMap<String, String>> {
    query.map(|Query(query)| query).map_err(|err| AppError::bad_request(err.body_text()))
}

/// The id a path names: `what` says whose.
fn id_of(path: Result<Path<i64>, PathRejection>, what: &str) -> ApiResult<i64> {
    path.map(|Path(id)| id).map_err(|_| AppError::bad_request(format!("{what} id is a number")))
}

/// A body that may be left out (or empty): its fields' defaults then.
fn optional_body<T: DeserializeOwned + Default>(body: &Bytes) -> ApiResult<T> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(body).map_err(|err| AppError::bad_request(format!("invalid body: {err}")))
}

/// `page` of a query: from 1 (the default) to TMDB's last.
fn page_of(query: &HashMap<String, String>) -> ApiResult<u32> {
    match query.get("page") {
        None => Ok(1),
        Some(page) => page
            .parse::<u32>()
            .ok()
            .filter(|p| (1..=MAX_PAGE).contains(p))
            .ok_or_else(|| AppError::bad_request(format!("page is 1 to {MAX_PAGE}"))),
    }
}

/// `GET /api/requests/summary`: what this session may do with requests,
/// and what there is to see. Any session, any app: it is how apps learn
/// whether to show requests at all.
pub async fn summary(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Summary>> {
    Ok(Json(state.requests.summary(&user).await?))
}

/// `GET /api/discover/{list}?page=`.
pub async fn discover(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    query: Params,
) -> RequestResult<Json<DiscoverPage>> {
    let caller = state.requests.asker(&user, &headers).await?;
    let page = page_of(&params(query)?)?;
    let unknown = || AppError::not_found("Discover lists trending, movies, shows, upcoming and anime");
    let Path(list) = path.map_err(|_| unknown())?;
    let list: List = serde_json::from_value(serde_json::Value::String(list)).map_err(|_| unknown())?;
    Ok(Json(state.requests.discover(&caller, list, page).await?))
}

/// `GET /api/discover/search?q=&page=&missing=`.
pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    query: Params,
) -> RequestResult<Json<DiscoverPage>> {
    let caller = state.requests.asker(&user, &headers).await?;
    let query = params(query)?;
    let text = query.get("q").map(|q| q.trim()).unwrap_or_default();
    if text.is_empty() || text.chars().count() > QUERY_MAX {
        return Err(AppError::bad_request(format!("search for 1 to {QUERY_MAX} characters")).into());
    }
    let page = page_of(&query)?;
    let missing = query.get("missing").is_some_and(|v| truthy(v));
    Ok(Json(state.requests.search(&caller, text, page, missing).await?))
}

/// `GET /api/discover/movie/{tmdb_id}`.
pub async fn movie(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
) -> RequestResult<Json<DiscoverDetail>> {
    let caller = state.requests.asker(&user, &headers).await?;
    let tmdb_id = id_of(path, "a TMDB")?;
    Ok(Json(state.requests.detail(&caller, TitleKind::Movie, tmdb_id).await?))
}

/// `GET /api/discover/show/{tmdb_id}`.
pub async fn show(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
) -> RequestResult<Json<DiscoverDetail>> {
    let caller = state.requests.asker(&user, &headers).await?;
    let tmdb_id = id_of(path, "a TMDB")?;
    Ok(Json(state.requests.detail(&caller, TitleKind::Show, tmdb_id).await?))
}

/// `GET /api/discover/images/{kind}/{file}?w=`: one of TMDB's pictures,
/// served as library artwork is: any, to an account that may ask; those
/// of its own requests to one that may not.
pub async fn image(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<(String, String)>, PathRejection>,
    query: Params,
) -> RequestResult<Response> {
    let caller = state.requests.reader(&user, &headers).await?;
    // As for library artwork, a width that isn't one is left out.
    let width = params(query)?.get("w").and_then(|w| w.parse::<u32>().ok()).filter(|w| *w > 0);
    let Path((kind, file)) = path.map_err(|_| AppError::not_found("no such picture"))?;
    let bytes = state.requests.picture(&caller, &kind, &file, width).await?;
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static(pictures::content_type(&bytes))),
        // A file name is one picture for good. Private: a proxy in front
        // hands it to no one who isn't signed in.
        (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=604800, immutable")),
    ];
    Ok((headers, bytes).into_response())
}

/// `GET /api/items/{id}/request`: requests for a library's movie or show,
/// as its page shows them.
pub async fn item_request(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
) -> RequestResult<Json<ItemRequest>> {
    let caller = state.requests.reader(&user, &headers).await?;
    let id = id_of(path, "an item's")?;
    Ok(Json(state.requests.for_item(&caller, id).await?))
}

/// `GET /api/requests?status=&attention=&kind=&user_id=&q=&order=&mine=&unseen=&changed_since=&offset=&limit=`.
pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    query: Params,
) -> RequestResult<Json<RequestList>> {
    let caller = state.requests.reader(&user, &headers).await?;
    let query = params(query)?;
    let text = |name: &str| query.get(name).map(|v| v.trim()).filter(|v| !v.is_empty());
    let mut statuses = Vec::new();
    for status in text("status").into_iter().flat_map(|s| s.split(',')).map(str::trim).filter(|s| !s.is_empty()) {
        let status = RequestStatus::parse(status).ok_or_else(|| {
            AppError::bad_request("status is pending, approved, declined, failed or available, or several of them")
        })?;
        if !statuses.contains(&status) {
            statuses.push(status);
        }
    }
    let kind = match text("kind") {
        Some(kind) => Some(TitleKind::parse(kind).ok_or_else(|| AppError::bad_request("kind is movie or show"))?),
        None => None,
    };
    let oldest = match text("order") {
        None | Some("newest") => false,
        Some("oldest") => true,
        Some(_) => return Err(AppError::bad_request("order is newest or oldest").into()),
    };
    let q = text("q");
    if q.is_some_and(|q| q.chars().count() > QUERY_MAX) {
        return Err(AppError::bad_request(format!("q is {QUERY_MAX} characters at most")).into());
    }
    let number = |name: &str| -> ApiResult<Option<i64>> {
        text(name)
            .map(|v| v.parse::<i64>().map_err(|_| AppError::bad_request(format!("{name} is a number"))))
            .transpose()
    };
    let flag = |name: &str| query.get(name).is_some_and(|v| truthy(v));
    let list = ListQuery {
        statuses,
        attention: flag("attention"),
        kind,
        user_id: number("user_id")?,
        q: q.map(str::to_owned),
        oldest,
        mine: flag("mine"),
        unseen: flag("unseen"),
        changed_since: number("changed_since")?,
        offset: number("offset")?.unwrap_or(0),
        limit: number("limit")?,
    };
    Ok(Json(state.requests.list(&caller, list).await?))
}

/// `POST /api/requests`: `{ kind, tmdb_id, seasons? }`. `201` with the new
/// request; `200` with the one followed where nothing was left to ask for.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    body: Result<Json<NewRequest>, JsonRejection>,
) -> RequestResult<(StatusCode, Json<MediaRequest>)> {
    let caller = state.requests.asker(&user, &headers).await?;
    let Json(new) = body.map_err(|err| AppError::bad_request(err.body_text()))?;
    let (made, request) = state.requests.create(&caller, new).await?;
    Ok((if made { StatusCode::CREATED } else { StatusCode::OK }, Json(request)))
}

/// `GET /api/requests/{id}`.
pub async fn get(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
) -> RequestResult<Json<MediaRequest>> {
    let caller = state.requests.reader(&user, &headers).await?;
    let id = id_of(path, "a request's")?;
    Ok(Json(state.requests.get(&caller, id).await?))
}

/// `POST /api/requests/{id}/approve`: `{ quality_profile?, root_folder?,
/// seasons? }`.
pub async fn approve(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
    body: Bytes,
) -> RequestResult<Json<MediaRequest>> {
    let caller = state.requests.manager(&user, &headers).await?;
    let id = id_of(path, "a request's")?;
    let approval: Approval = optional_body(&body)?;
    Ok(Json(state.requests.approve(&caller, id, approval).await?))
}

#[derive(Debug, Default, Deserialize)]
pub struct Decline {
    #[serde(default)]
    reason: Option<String>,
}

/// `POST /api/requests/{id}/decline`: `{ reason? }`.
pub async fn decline(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
    body: Bytes,
) -> RequestResult<Json<MediaRequest>> {
    let caller = state.requests.manager(&user, &headers).await?;
    let id = id_of(path, "a request's")?;
    let decline: Decline = optional_body(&body)?;
    Ok(Json(state.requests.decline(&caller, id, decline.reason).await?))
}

/// `POST /api/requests/{id}/handled`: approved as added to Radarr or Sonarr
/// by hand, so never sent.
pub async fn handled(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
) -> RequestResult<Json<MediaRequest>> {
    let caller = state.requests.manager(&user, &headers).await?;
    let id = id_of(path, "a request's")?;
    Ok(Json(state.requests.handled(&caller, id).await?))
}

/// `GET /api/requests/options`: what approving may choose in Radarr and
/// Sonarr.
pub async fn options(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
) -> RequestResult<Json<RequestOptions>> {
    let caller = state.requests.manager(&user, &headers).await?;
    Ok(Json(state.requests.options(&caller).await))
}

/// `DELETE /api/requests/{id}`.
pub async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
) -> RequestResult<StatusCode> {
    let caller = state.requests.reader(&user, &headers).await?;
    let id = id_of(path, "a request's")?;
    state.requests.delete(&caller, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/requests/{id}/watch`: this account stops following it.
pub async fn unwatch(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    path: Result<Path<i64>, PathRejection>,
) -> RequestResult<StatusCode> {
    let caller = state.requests.reader(&user, &headers).await?;
    let id = id_of(path, "a request's")?;
    state.requests.unwatch(&caller, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/requests/seen`: what had news for this account is seen, on
/// its other devices too.
pub async fn seen(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    CurrentToken(token): CurrentToken,
) -> RequestResult<StatusCode> {
    let caller = state.requests.reader(&user, &headers).await?;
    state.requests.seen(&caller, token.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}
