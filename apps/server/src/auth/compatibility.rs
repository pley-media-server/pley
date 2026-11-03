//! Tokens for other apps that use the Jellyfin-compatible API
//! (docs/COMPATIBILITY.md): Jellyfin's apps, Infuse, scripts. An app gets
//! one by signing in there (`/Users/AuthenticateByName`), or a person makes
//! one on the account page. They work on the compatibility API only, never
//! on Pley's own, and do only what their [`Scopes`] allow: play, for their
//! own account.
//!
//! Tokens carry a prefix (`pley_ct_`), so that leaked ones are recognisable
//! and never pass for a session's, and are stored as SHA-256, like session
//! tokens. One ends when it is revoked, 90 days after it was last used, and
//! with the account's other sign-ins; it doesn't work while the account is
//! disabled. [`super::AuthService`] validates them, caches what it read and
//! forgets it when they end.

use std::fmt;

use super::{SESSION_TTL_SECS, hash_token, new_token};
use crate::db::Db;
use crate::error::AppError;
use crate::util::now;

/// How compatibility tokens start.
pub const PREFIX: &str = "pley_ct_";
/// A token ends this long after it was last used, like a session.
pub const IDLE_SECS: i64 = SESSION_TTL_SECS;
/// Tokens one account keeps; past it, the least recently used ends.
pub const MAX_PER_USER: i64 = 100;
/// The longest name a token may have.
pub const MAX_NAME_CHARS: usize = 80;

/// One thing a compatibility token may be allowed to do. Scopes are stored
/// and answered by name, so that a later one is another name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Browse what the account may open, play, and keep its own watch state.
    /// Every token has it.
    Play,
}

impl Scope {
    /// Every scope, in the order they are written and shown.
    pub const ALL: [Scope; 1] = [Scope::Play];

    /// The name in `scope` fields and answers.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Play => "play",
        }
    }

    pub fn from_name(name: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|scope| scope.name() == name)
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// What a compatibility token may do: always [`Scope::Play`], and maybe more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scopes(u8);

impl Scopes {
    /// `play` alone, what a token gets unless more is asked for.
    pub fn play() -> Scopes {
        Scopes(Scope::Play.bit())
    }

    /// A `scope` field: names separated by spaces, each known; `play` is
    /// there whether it is named or not. The name that isn't known is the
    /// error.
    pub fn parse(text: &str) -> Result<Scopes, String> {
        let mut scopes = Scopes::play();
        for name in text.split(' ').filter(|name| !name.is_empty()) {
            scopes = scopes.with(Scope::from_name(name).ok_or_else(|| name.to_owned())?);
        }
        Ok(scopes)
    }

    /// A scope stored with a token. Names this version doesn't know (from a
    /// newer one) are left out rather than refused.
    pub fn from_stored(text: &str) -> Scopes {
        text.split(' ').filter_map(Scope::from_name).fold(Scopes::play(), Scopes::with)
    }

    pub fn with(self, scope: Scope) -> Scopes {
        Scopes(self.0 | scope.bit())
    }

    pub fn contains(self, scope: Scope) -> bool {
        self.0 & scope.bit() != 0
    }

    /// The names, in the fixed order of [`Scope::ALL`].
    pub fn names(self) -> Vec<&'static str> {
        Scope::ALL.into_iter().filter(|scope| self.contains(*scope)).map(Scope::name).collect()
    }
}

impl fmt::Display for Scopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.names().join(" "))
    }
}

/// A token someone asked for, its name and scope checked ([`asked`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub name: String,
    pub scopes: Scopes,
}

/// What a person or a tool asked for, as the account page and sign-ins send
/// it: a name (trimmed, 1 to [`MAX_NAME_CHARS`] characters, none of them a
/// control character) and a `scope` field (`play` without one).
pub fn asked(name: &str, scope: Option<&str>) -> Result<Asked, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "name the token with 1 to {MAX_NAME_CHARS} letters, digits, spaces or signs"
        )));
    }
    let scopes = Scopes::parse(scope.unwrap_or_default())
        .map_err(|unknown| AppError::bad_request(format!("unknown scope '{unknown}': tokens may play")))?;
    Ok(Asked { name: name.to_owned(), scopes })
}

/// A token, for the account page's list.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TokenRow {
    pub id: i64,
    pub name: String,
    pub scope: String,
    pub created_at: i64,
    /// `None` until it is first used.
    pub last_used_at: Option<i64>,
    pub expires_at: i64,
    /// Of its last use.
    pub user_agent: Option<String>,
    pub last_ip: Option<String>,
}

impl TokenRow {
    pub fn scopes(&self) -> Scopes {
        Scopes::from_stored(&self.scope)
    }
}

const COLUMNS: &str = "id, name, scope, created_at, last_used_at, expires_at, user_agent, last_ip";

/// Stores a new token for `user_id` and hands it out, with its row. The
/// account's tokens past [`MAX_PER_USER`] end (the least recently used):
/// their ids come third.
pub async fn create(db: &Db, user_id: i64, asked: &Asked) -> Result<(String, TokenRow, Vec<i64>), AppError> {
    let token = format!("{PREFIX}{}", new_token());
    let now = now();
    let mut tx = db.write.begin().await?;
    let row: TokenRow = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO compatibility_tokens (user_id, name, scope, token_hash, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING {COLUMNS}"
    )))
    .bind(user_id)
    .bind(&asked.name)
    .bind(asked.scopes.to_string())
    .bind(hash_token(&token))
    .bind(now)
    .bind(now + IDLE_SECS)
    .fetch_one(&mut *tx)
    .await?;
    let dropped: Vec<i64> = sqlx::query_scalar(
        "DELETE FROM compatibility_tokens WHERE user_id = ? AND id NOT IN
           (SELECT id FROM compatibility_tokens WHERE user_id = ?
            ORDER BY COALESCE(last_used_at, created_at) DESC, id DESC LIMIT ?)
         RETURNING id",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(MAX_PER_USER)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((token, row, dropped))
}

/// The tokens of `user_id` that still work, most recently used first.
pub async fn list(db: &Db, user_id: i64) -> Result<Vec<TokenRow>, AppError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM compatibility_tokens WHERE user_id = ? AND expires_at > ?
         ORDER BY COALESCE(last_used_at, created_at) DESC, id DESC"
    )))
    .bind(user_id)
    .bind(now())
    .fetch_all(&db.read)
    .await?)
}

/// Ends token `id` of `user_id`; whether there was one.
pub async fn delete(db: &Db, user_id: i64, id: i64) -> Result<bool, AppError> {
    let deleted = sqlx::query("DELETE FROM compatibility_tokens WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(&db.write)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// Ends the token `token`, if it is one; its id.
pub async fn delete_token(db: &Db, token: &str) -> Result<Option<i64>, AppError> {
    if !token.starts_with(PREFIX) {
        return Ok(None);
    }
    Ok(sqlx::query_scalar("DELETE FROM compatibility_tokens WHERE token_hash = ? RETURNING id")
        .bind(hash_token(token))
        .fetch_optional(&db.write)
        .await?)
}

/// Ends every token of `user_id`; their ids.
pub async fn delete_for(db: &Db, user_id: i64) -> Result<Vec<i64>, AppError> {
    Ok(sqlx::query_scalar("DELETE FROM compatibility_tokens WHERE user_id = ? RETURNING id")
        .bind(user_id)
        .fetch_all(&db.write)
        .await?)
}

/// Forgets tokens unused for [`IDLE_SECS`].
pub async fn purge(db: &Db) -> Result<(), AppError> {
    sqlx::query("DELETE FROM compatibility_tokens WHERE expires_at <= ?").bind(now()).execute(&db.write).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database(dir: &std::path::Path) -> Db {
        let db = Db::open(&dir.join("pley.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, is_admin, created_at)
             VALUES (1, 'ann', 'x', 1, 0), (2, 'bob', 'x', 0, 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        db
    }

    fn tool() -> Asked {
        asked("Infuse", None).unwrap()
    }

    #[test]
    fn scopes_always_play() {
        assert_eq!(Scopes::parse("").unwrap(), Scopes::play());
        assert_eq!(Scopes::parse("play  play").unwrap().to_string(), "play");
        assert_eq!(Scopes::parse("play admin"), Err("admin".to_owned()));
        assert_eq!(Scopes::parse("Play"), Err("Play".to_owned()), "names are lower case");
        assert_eq!(Scopes::parse("library"), Err("library".to_owned()), "apps' scopes aren't these");
        // Tokens made with scopes older versions had keep playing.
        assert_eq!(Scopes::from_stored("play activity manage"), Scopes::play());
        assert_eq!(Scopes::from_stored(""), Scopes::play());
        for scope in Scope::ALL {
            assert_eq!(Scope::from_name(scope.name()), Some(scope));
        }
    }

    #[test]
    fn names_and_scopes_are_checked() {
        assert_eq!(asked("  Infuse ", None).unwrap(), Asked { name: "Infuse".into(), scopes: Scopes::play() });
        assert_eq!(asked(&"é".repeat(MAX_NAME_CHARS), None).unwrap().name.chars().count(), MAX_NAME_CHARS);
        for bad in ["", "   ", "tab\there", "new\nline"] {
            assert!(matches!(asked(bad, None), Err(AppError::BadRequest(_))), "{bad:?}");
        }
        assert!(matches!(asked(&"a".repeat(MAX_NAME_CHARS + 1), None), Err(AppError::BadRequest(_))));
        for scope in ["play everything", "activity", "manage"] {
            let Err(AppError::BadRequest(message)) = asked("x", Some(scope)) else { panic!("{scope}") };
            assert!(message.contains("unknown scope"), "{message}");
        }
    }

    #[tokio::test]
    async fn tokens_are_prefixed_and_stored_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (token, row, dropped) = create(&db, 1, &tool()).await.unwrap();
        assert!(dropped.is_empty());
        assert!(token.starts_with(PREFIX) && token.len() == PREFIX.len() + 43, "{token}");
        assert_eq!((row.name.as_str(), row.scope.as_str(), row.last_used_at), ("Infuse", "play", None));
        assert_eq!(row.expires_at - row.created_at, IDLE_SECS);
        let dump: Vec<String> = sqlx::query_scalar("SELECT token_hash || name || scope FROM compatibility_tokens")
            .fetch_all(&db.read)
            .await
            .unwrap();
        assert_eq!(dump.len(), 1);
        assert!(dump[0].starts_with(&hash_token(&token)));
        assert!(!dump[0].contains(&token[PREFIX.len()..]), "never the token itself");
        assert_eq!(list(&db, 1).await.unwrap(), [row]);
        assert!(list(&db, 2).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn tokens_are_listed_ended_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (_, oldest, _) = create(&db, 1, &tool()).await.unwrap();
        let (used, used_row, _) = create(&db, 1, &tool()).await.unwrap();
        // The second is used later than every other: it stays, and comes first.
        sqlx::query("UPDATE compatibility_tokens SET last_used_at = created_at + 1000 WHERE id = ?")
            .bind(used_row.id)
            .execute(&db.write)
            .await
            .unwrap();
        let (_, bobs, _) = create(&db, 2, &tool()).await.unwrap();
        for _ in 2..MAX_PER_USER {
            let (_, _, dropped) = create(&db, 1, &tool()).await.unwrap();
            assert!(dropped.is_empty());
        }
        let (_, _, dropped) = create(&db, 1, &tool()).await.unwrap();
        assert_eq!(dropped, [oldest.id], "the least recently used one ends");
        let anns = list(&db, 1).await.unwrap();
        assert_eq!(anns.len() as i64, MAX_PER_USER);
        assert_eq!(anns[0].id, used_row.id, "most recently used first");

        assert!(!delete(&db, 2, used_row.id).await.unwrap(), "only one's own");
        assert!(delete(&db, 1, used_row.id).await.unwrap());
        assert!(!delete(&db, 1, used_row.id).await.unwrap());
        assert_eq!(delete_token(&db, &used).await.unwrap(), None, "gone already");
        assert_eq!(delete_for(&db, 2).await.unwrap(), [bobs.id]);
        assert_eq!(delete_for(&db, 1).await.unwrap().len() as i64, MAX_PER_USER - 1);

        // 90 days unused: no longer listed, then purged.
        let (token, row, _) = create(&db, 2, &tool()).await.unwrap();
        sqlx::query("UPDATE compatibility_tokens SET expires_at = ? WHERE id = ?")
            .bind(now() - 1)
            .bind(row.id)
            .execute(&db.write)
            .await
            .unwrap();
        assert!(list(&db, 2).await.unwrap().is_empty());
        purge(&db).await.unwrap();
        assert_eq!(delete_token(&db, &token).await.unwrap(), None);
        let left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM compatibility_tokens").fetch_one(&db.read).await.unwrap();
        assert_eq!(left, 0);

        let (token, row, _) = create(&db, 2, &tool()).await.unwrap();
        assert_eq!(delete_token(&db, "pley_at_x").await.unwrap(), None);
        assert_eq!(delete_token(&db, &token).await.unwrap(), Some(row.id));
    }
}
