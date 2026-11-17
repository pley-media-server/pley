//! Watch history: one entry per play that reached the end (see
//! [`crate::catalog::record_progress`]).

use std::collections::HashMap;

use serde::Serialize;

use crate::accounts;
use crate::catalog::{self, ItemRow, ItemSummary};
use crate::db::Db;

/// Who played it, as shown next to an entry.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryUser {
    pub id: i64,
    /// The display name, or the username.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_color: Option<String>,
}

/// An entry with the item as loaded through the catalog (access rules applied).
#[derive(Debug)]
pub struct Record {
    pub id: i64,
    pub viewed_at: i64,
    pub device: Option<String>,
    pub user: HistoryUser,
    pub item: ItemRow,
}

#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub viewed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    pub user: HistoryUser,
    pub item: ItemSummary,
}

impl Record {
    pub fn to_entry(&self) -> HistoryEntry {
        HistoryEntry {
            id: self.id,
            viewed_at: self.viewed_at,
            device: self.device.clone(),
            user: self.user.clone(),
            item: self.item.to_summary(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Filter {
    /// Whose plays; `None`: everyone's (for administrators).
    pub user_id: Option<i64>,
    /// An item: itself, or the episodes or tracks under it.
    pub item_id: Option<i64>,
    pub library_id: Option<i64>,
    /// Only plays in these libraries, the ones the request gets
    /// ([`crate::access::library_ids`]); `None`: any.
    pub libraries: Option<Vec<i64>>,
    /// Played at or after (unix seconds).
    pub since: Option<i64>,
    pub offset: i64,
    pub limit: i64,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    user_id: i64,
    item_id: i64,
    viewed_at: i64,
    device: Option<String>,
    username: String,
    display_name: Option<String>,
    avatar: Option<String>,
    avatar_color: Option<String>,
}

/// Matching entries, newest first, and how many there are. Entries of items
/// the viewer can't see are left out.
pub async fn list(db: &Db, viewer_id: i64, filter: &Filter) -> sqlx::Result<(Vec<Record>, i64)> {
    let mut conditions = String::new();
    if filter.user_id.is_some() {
        conditions.push_str(" AND h.user_id = ?");
    }
    if filter.item_id.is_some() {
        conditions.push_str(" AND ? IN (h.item_id, i.parent_id, i.grandparent_id)");
    }
    if filter.library_id.is_some() {
        conditions.push_str(" AND i.library_id = ?");
    }
    if filter.since.is_some() {
        conditions.push_str(" AND h.viewed_at >= ?");
    }
    let libraries = filter.libraries.as_ref().map(|ids| serde_json::to_string(ids).unwrap_or_default());
    if libraries.is_some() {
        conditions.push_str(" AND i.library_id IN (SELECT value FROM json_each(?))");
    }
    let from = format!(
        "FROM play_history h JOIN users u ON u.id = h.user_id JOIN items i ON i.id = h.item_id
         LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
         WHERE {}{conditions}",
        catalog::VISIBLE
    );
    macro_rules! bind_filter {
        ($query:expr) => {{
            let mut query = $query.bind(viewer_id);
            for value in [filter.user_id, filter.item_id, filter.library_id, filter.since].into_iter().flatten() {
                query = query.bind(value);
            }
            if let Some(libraries) = &libraries {
                query = query.bind(libraries.as_str());
            }
            query
        }};
    }
    let sql = format!(
        "{} SELECT h.id, h.user_id, h.item_id, h.viewed_at, h.device, u.username, u.display_name, u.avatar,
                u.avatar_color {from} ORDER BY h.viewed_at DESC, h.id DESC LIMIT ? OFFSET ?",
        catalog::ME_CTE
    );
    let rows: Vec<Row> = bind_filter!(sqlx::query_as::<_, Row>(sqlx::AssertSqlSafe(sql.as_str())))
        .bind(filter.limit.max(0))
        .bind(filter.offset.max(0))
        .fetch_all(&db.read)
        .await?;
    let total = if filter.offset == 0 && (rows.len() as i64) < filter.limit {
        rows.len() as i64
    } else {
        // Without the users join, which made SQLite read every play first.
        let sql = format!(
            "{} SELECT COUNT(*) FROM play_history h JOIN items i ON i.id = h.item_id
                 LEFT JOIN items p ON p.id = i.parent_id LEFT JOIN items gp ON gp.id = i.grandparent_id
             WHERE {}{conditions}",
            catalog::ME_CTE,
            catalog::VISIBLE
        );
        bind_filter!(sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))).fetch_one(&db.read).await?
    };

    let mut ids: Vec<i64> = rows.iter().map(|r| r.item_id).collect();
    ids.sort_unstable();
    ids.dedup();
    let items: HashMap<i64, ItemRow> =
        catalog::items_by_ids(db, viewer_id, &ids).await?.into_iter().map(|r| (r.id, r)).collect();
    let records = rows
        .into_iter()
        .filter_map(|row| {
            let item = items.get(&row.item_id)?.clone();
            Some(Record {
                id: row.id,
                viewed_at: row.viewed_at,
                device: row.device,
                user: HistoryUser {
                    id: row.user_id,
                    name: row.display_name.filter(|n| !n.trim().is_empty()).unwrap_or(row.username),
                    avatar_url: accounts::avatar_url(row.user_id, row.avatar.as_deref()),
                    avatar_color: row.avatar_color,
                },
                item,
            })
        })
        .collect();
    Ok((records, total))
}

/// Remove an entry: your own, or anyone's for administrators.
pub async fn delete(db: &Db, viewer_id: i64, admin: bool, id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM play_history WHERE id = ? AND (? OR user_id = ?)")
        .bind(id)
        .bind(admin)
        .bind(viewer_id)
        .execute(&db.write)
        .await?;
    Ok(done.rows_affected() > 0)
}
