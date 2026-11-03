use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Serialize;

use super::items::item_or_404;
use super::libraries::{Hub, Hubs};
use super::wants_summaries;
use crate::auth::AuthUser;
use crate::catalog::{self, ItemSummary};
use crate::error::{ApiResult, AppError};
use crate::people::{self, PersonSummary};
use crate::state::AppState;

pub async fn home(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<Hubs>> {
    let summaries = wants_summaries(&query);
    // A row for each library the user may open, in their order.
    let libraries = crate::access::visible_libraries(&state.db, &user).await?;
    // Every hub at the same time: they don't depend on each other.
    let (continue_rows, my_list, recent) = tokio::try_join!(
        catalog::continue_watching(&state.db, user.id, None, 20),
        catalog::watchlist(&state.db, user.id, Some(20)),
        futures::future::try_join_all(libraries.iter().map(|lib| catalog::recently_added(
            &state.db,
            user.id,
            lib.id,
            lib.kind().as_str(),
            20
        )),),
    )?;
    let mut hubs = vec![
        Hub {
            key: "continue_watching".into(),
            title: "Continue Watching".into(),
            library_id: None,
            items: continue_rows.iter().map(|r| r.to_summary_with(summaries)).collect(),
        },
        Hub {
            key: "my_list".into(),
            title: "My List".into(),
            library_id: None,
            items: my_list.iter().map(|r| r.to_summary_with(summaries)).collect(),
        },
    ];
    for (lib, rows) in libraries.iter().zip(recent) {
        hubs.push(Hub {
            key: format!("recently_added:{}", lib.id),
            // "in" reads right whatever the library is called.
            title: format!("Recently Added in {}", lib.name),
            library_id: Some(lib.id),
            items: rows.iter().map(|r| r.to_summary_with(summaries)).collect(),
        });
    }
    hubs.retain(|h| !h.items.is_empty());
    Ok(Json(Hubs { hubs }))
}

/// `DELETE /api/continue-watching/{id}`: takes a row out of the user's
/// Continue Watching (every one: Home's, the libraries', the Jellyfin API's) until
/// they play it again. An episode takes its whole show out. Progress and
/// watched marks stay as they are.
pub async fn hide_continue_watching(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let row = item_or_404(&state, user.id, id).await?;
    let title = catalog::continue_watching_title(&row)
        .ok_or_else(|| AppError::bad_request("only movies and shows are in Continue Watching"))?;
    catalog::hide_from_continue_watching(&state.db, &state.events, user.id, title).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct SearchResults {
    items: Vec<ItemSummary>,
    people: Vec<PersonSummary>,
    /// Live TV channels by name, number or what's on, for those it's open to.
    channels: Vec<crate::livetv::Channel>,
}

pub async fn search(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<SearchResults>> {
    let q = query.get("q").map(|s| s.trim()).unwrap_or_default();
    // Up to `limit` of each: titles, people and channels.
    let limit = query.get("limit").and_then(|l| l.parse::<i64>().ok()).unwrap_or(40).clamp(1, 200);
    // Titles and people at the same time.
    let (rows, people) =
        tokio::try_join!(catalog::search(&state.db, user.id, q, limit), people::search(&state.db, user.id, q, limit))?;
    let summaries = wants_summaries(&query);
    let channels = if !q.is_empty() && state.live.may_watch(&user).await {
        state.live.browse(&user, &crate::livetv::Category::All, q, 0, limit as usize, None).await?.channels
    } else {
        Vec::new()
    };
    Ok(Json(SearchResults { items: rows.iter().map(|r| r.to_summary_with(summaries)).collect(), people, channels }))
}
