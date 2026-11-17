//! `/api/collection-categories`: the categories a library's collections are
//! grouped by. Everyone may read them; administrators make, rename, order
//! and delete their own.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

use crate::auth::{AdminUser, AuthUser};
use crate::categories::{self, Category, Refused};
use crate::error::{ApiResult, AppError};
use crate::events::ServerEvent;
use crate::i18n;
use crate::state::AppState;

/// The longest name a category may have.
const MAX_TITLE: usize = 100;

#[derive(Serialize)]
pub struct CategoryList {
    categories: Vec<Category>,
}

async fn language(state: &AppState, user: &AuthUser, headers: &HeaderMap) -> ApiResult<&'static str> {
    i18n::for_user(&state.db, user.id, &state.settings.get().language, headers).await
}

pub async fn list(State(state): State<AppState>, user: AuthUser, headers: HeaderMap) -> ApiResult<Json<CategoryList>> {
    let language = language(&state, &user, &headers).await?;
    Ok(Json(CategoryList { categories: categories::list(&state.db, language).await? }))
}

#[derive(Deserialize)]
pub struct Title {
    title: String,
}

fn checked_title(title: &str) -> ApiResult<&str> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE {
        return Err(AppError::bad_request("a category's title is 1 to 100 characters"));
    }
    Ok(title)
}

fn refused(refused: Refused) -> AppError {
    match refused {
        Refused::Taken => AppError::conflict("a category already has that name"),
        Refused::Builtin => AppError::bad_request("Pley's own categories keep their names"),
        Refused::NotFound => AppError::not_found("category not found"),
    }
}

async fn one(state: &AppState, admin: &AuthUser, headers: &HeaderMap, id: i64) -> ApiResult<Json<Category>> {
    let language = language(state, admin, headers).await?;
    categories::get(&state.db, id, language).await?.map(Json).ok_or_else(|| AppError::not_found("category not found"))
}

pub async fn create(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Json(body): Json<Title>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    let id = categories::create(&state.db, checked_title(&body.title)?).await?.map_err(refused)?;
    state.events.send(ServerEvent::LibrariesChanged);
    Ok((StatusCode::CREATED, one(&state, &admin, &headers, id).await?))
}

pub async fn rename(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<Title>,
) -> ApiResult<Json<Category>> {
    categories::rename(&state.db, id, checked_title(&body.title)?).await?.map_err(refused)?;
    state.events.send(ServerEvent::LibrariesChanged);
    one(&state, &admin, &headers, id).await
}

pub async fn delete(State(state): State<AppState>, _admin: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    categories::delete(&state.db, id).await?.map_err(refused)?;
    state.events.send(ServerEvent::LibrariesChanged);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Order {
    ids: Vec<i64>,
}

pub async fn reorder(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<Order>,
) -> ApiResult<StatusCode> {
    categories::reorder(&state.db, &body.ids).await?;
    state.events.send(ServerEvent::LibrariesChanged);
    Ok(StatusCode::NO_CONTENT)
}
