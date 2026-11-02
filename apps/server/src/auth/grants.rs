//! What people allowed apps to do (`app_grants`), and the tokens of those
//! apps (`app_tokens`): access tokens that last an hour, and refresh tokens
//! that are exchanged once each. A spent refresh token stays on record until
//! it expires, so one shown again, however many exchanges later, ends its
//! grant: someone has a copy.
//!
//! Tokens are stored as SHA-256, like session tokens, and carry a prefix
//! (`pley_at_`, `pley_rt_`) so that leaked ones are recognisable and never
//! pass for a session token, or for each other. Everything here runs in short
//! transactions on the writer (see [`crate::db`]); [`super::AuthService`]
//! wraps it and forgets cached tokens.

use sqlx::{Sqlite, Transaction};

use super::scopes::Scopes;
use super::{SESSION_TTL_SECS, hash_token, new_token};
use crate::db::Db;
use crate::error::AppError;
use crate::util::now;

/// How access tokens start.
pub const ACCESS_PREFIX: &str = "pley_at_";
/// How refresh tokens start.
pub const REFRESH_PREFIX: &str = "pley_rt_";
/// What every app token starts with (and no session token).
pub const APP_TOKEN_PREFIX: &str = "pley_";

/// An access token lasts an hour.
pub const ACCESS_TOKEN_SECS: i64 = 3600;
/// A grant ends this long after its last token request, like a session.
pub const GRANT_IDLE_SECS: i64 = SESSION_TTL_SECS;
/// Apps by other publishers are asked about again after a year.
pub const THIRD_PARTY_GRANT_SECS: i64 = 365 * 24 * 3600;
/// Grants acting as one profile; past it, the least recently used ends.
pub const MAX_GRANTS_PER_USER: i64 = 100;
/// Access tokens kept per grant; older ones stop working.
pub const MAX_LIVE_ACCESS_TOKENS: i64 = 10;
/// Refresh tokens on record per grant (spent ones included); past it the
/// grant ends. About 111 refreshes a day for 90 days: more than any app needs.
pub const MAX_REFRESH_TOKENS_ON_RECORD: i64 = 10_000;

/// A grant to record, when a person allowed an app.
#[derive(Debug, Clone)]
pub struct NewGrant {
    /// The profile the app acts as.
    pub user_id: i64,
    /// Who allowed it.
    pub approved_by: i64,
    pub client_id: String,
    /// What the app called itself.
    pub client_name: Option<String>,
    pub scopes: Scopes,
    /// The app gets a refresh token (its client document lists them).
    pub refresh: bool,
    /// One of pley's own apps: no yearly end.
    pub first_party: bool,
    /// SHA-256 of the token of the browser session where the person allowed
    /// it: the grant is recorded only while that session is signed in, to
    /// the account that allowed it. `None` for grants made without a
    /// browser (tests, tools).
    pub session_hash: Option<String>,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

/// Why a grant wasn't recorded.
#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    /// The browser session where the person allowed the app has signed out
    /// since, or ended with the account's other sign-ins.
    #[error("the person who allowed it signed out since")]
    SignedOut,
    #[error(transparent)]
    Internal(#[from] AppError),
}

impl From<sqlx::Error> for IssueError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(err.into())
    }
}

/// Tokens handed to an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    pub grant_id: i64,
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Seconds the access token lasts.
    pub expires_in: i64,
    /// What the tokens allow: the grant's whole scope.
    pub scopes: Scopes,
}

impl Issued {
    /// The SHA-256 of a token the grant keeps on record as long as it
    /// lasts, to end it by later (see [`delete_of_token`]): its refresh
    /// token, kept when spent until it expires, or else its access token,
    /// the only one an app without refresh tokens ever gets.
    pub fn on_record(&self) -> String {
        hash_token(self.refresh_token.as_deref().unwrap_or(&self.access_token))
    }
}

/// A refresh token exchange.
#[derive(Debug, Clone)]
pub struct RefreshInput {
    pub token: String,
    pub client_id: String,
    /// A narrower scope asked for is ignored (the grant's whole scope is
    /// issued); a wider one is refused.
    pub scope: Option<Scopes>,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

/// Why a refresh token got nothing. Every one is `invalid_grant` to the app,
/// but `WiderScope` (`invalid_scope`).
#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    /// The token was spent already: its grant is ended.
    #[error("refresh token used again: the app's access is ended")]
    Reused { grant_id: i64 },
    /// The grant had too many refresh tokens on record and is ended.
    #[error("refreshed too often: the app's access is ended")]
    Ended { grant_id: i64 },
    /// Unknown, another app's, expired, or its grant or account isn't usable.
    /// Nothing changed.
    #[error("{0}")]
    Invalid(&'static str),
    #[error("the app asked for more than it was allowed")]
    WiderScope,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for RefreshError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(err.into())
    }
}

/// A grant, for the lists of apps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRow {
    pub id: i64,
    pub user_id: i64,
    pub user_name: String,
    pub approved_by: i64,
    pub approved_by_name: String,
    pub client_id: String,
    pub client_name: Option<String>,
    pub scopes: Scopes,
    pub created_at: i64,
    pub last_used_at: i64,
    pub expires_at: i64,
    pub ends_at: Option<i64>,
    pub user_agent: Option<String>,
    pub last_ip: Option<String>,
}

fn agent(user_agent: Option<&str>) -> Option<String> {
    user_agent.map(|ua| ua.chars().take(200).collect())
}

/// `when`, but never past the grant's end.
fn capped(when: i64, ends_at: Option<i64>) -> i64 {
    ends_at.map_or(when, |end| when.min(end))
}

/// Stores a new token of `kind` for the grant; the token.
async fn insert_token(
    tx: &mut Transaction<'_, Sqlite>,
    grant_id: i64,
    kind: &str,
    expires_at: i64,
) -> sqlx::Result<String> {
    let prefix = if kind == "access" { ACCESS_PREFIX } else { REFRESH_PREFIX };
    let token = format!("{prefix}{}", new_token());
    sqlx::query("INSERT INTO app_tokens (token_hash, grant_id, kind, expires_at) VALUES (?, ?, ?, ?)")
        .bind(hash_token(&token))
        .bind(grant_id)
        .bind(kind)
        .bind(expires_at)
        .execute(&mut **tx)
        .await?;
    Ok(token)
}

/// Records what a person allowed an app and hands it its first tokens. The
/// grants of the profile past [`MAX_GRANTS_PER_USER`] end (the least
/// recently used): their ids come second.
///
/// Nothing is recorded when the browser session where the person allowed
/// it has ended. That is checked in the same transaction, on the one
/// writer: a sign-out either comes after the grant, and ends it with the
/// account's apps, or before, and the grant is never made.
pub async fn issue(db: &Db, grant: NewGrant) -> Result<(Issued, Vec<i64>), IssueError> {
    let now = now();
    let ends_at = (!grant.first_party).then_some(now + THIRD_PARTY_GRANT_SECS);
    let mut tx = db.write.begin().await?;
    if let Some(session_hash) = &grant.session_hash {
        let signed_in: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sessions WHERE token_hash = ? AND user_id = ? AND expires_at > ?)",
        )
        .bind(session_hash)
        .bind(grant.approved_by)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        if !signed_in {
            return Err(IssueError::SignedOut);
        }
    }
    let grant_id: i64 = sqlx::query_scalar(
        "INSERT INTO app_grants (id, user_id, approved_by, client_id, client_name, scope, created_at, last_used_at,
                                 expires_at, ends_at, user_agent, last_ip)
         VALUES ((SELECT last + 1 FROM grant_ids), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(grant.user_id)
    .bind(grant.approved_by)
    .bind(&grant.client_id)
    .bind(&grant.client_name)
    .bind(grant.scopes.to_string())
    .bind(now)
    .bind(now)
    .bind(now + GRANT_IDLE_SECS)
    .bind(ends_at)
    .bind(agent(grant.user_agent.as_deref()))
    .bind(&grant.ip)
    .fetch_one(&mut *tx)
    .await?;
    let access_expires = capped(now + ACCESS_TOKEN_SECS, ends_at);
    let access_token = insert_token(&mut tx, grant_id, "access", access_expires).await?;
    let refresh_token = if grant.refresh {
        Some(insert_token(&mut tx, grant_id, "refresh", capped(now + GRANT_IDLE_SECS, ends_at)).await?)
    } else {
        None
    };
    let dropped: Vec<i64> = sqlx::query_scalar(
        "DELETE FROM app_grants WHERE user_id = ? AND id NOT IN
           (SELECT id FROM app_grants WHERE user_id = ? ORDER BY last_used_at DESC, id DESC LIMIT ?)
         RETURNING id",
    )
    .bind(grant.user_id)
    .bind(grant.user_id)
    .bind(MAX_GRANTS_PER_USER)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let issued =
        Issued { grant_id, access_token, refresh_token, expires_in: access_expires - now, scopes: grant.scopes };
    Ok((issued, dropped))
}

/// Why an app's refresh gets nothing while the server requires two-step
/// sign-in of an account that doesn't use it (see
/// [`super::AuthService::require_two_step`]).
pub const TWO_STEP_REQUIRED: &str = "this server requires two-step sign-in: sign in again to set it up";

/// Exchanges a refresh token for a new pair (OAuth 2.1 §4.3.1, rotated as
/// RFC 9700 §4.14.2 says): the token is spent, and a spent one shown again
/// ends its grant. With `two_step_required`, nothing for a grant acting as,
/// or allowed by, an account that signs in with a password alone.
pub async fn refresh(db: &Db, input: RefreshInput, two_step_required: bool) -> Result<Issued, RefreshError> {
    if !input.token.starts_with(REFRESH_PREFIX) {
        return Err(RefreshError::Invalid("unknown refresh token"));
    }
    let key = hash_token(&input.token);
    let now = now();
    let mut tx = db.write.begin().await?;
    #[allow(clippy::type_complexity)]
    let row: Option<(i64, i64, Option<i64>, String, String, i64, Option<i64>, bool, bool, bool)> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT t.grant_id, t.expires_at, t.used_at, g.client_id, g.scope, g.expires_at, g.ends_at,
                    u.disabled, a.disabled, {} OR {}
             FROM app_tokens t
             JOIN app_grants g ON g.id = t.grant_id
             JOIN users u ON u.id = g.user_id
             JOIN users a ON a.id = g.approved_by
             WHERE t.token_hash = ? AND t.kind = 'refresh'",
            super::without_two_step("u"),
            super::without_two_step("a")
        )))
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((
        grant_id,
        token_expires,
        used_at,
        client_id,
        scope,
        grant_expires,
        ends_at,
        user_off,
        approver_off,
        no_two_step,
    )) = row
    else {
        return Err(RefreshError::Invalid("unknown refresh token"));
    };
    if client_id != input.client_id {
        return Err(RefreshError::Invalid("this refresh token belongs to another app"));
    }
    if used_at.is_some() {
        end_grant_in(&mut tx, grant_id).await?;
        tx.commit().await?;
        return Err(RefreshError::Reused { grant_id });
    }
    if token_expires <= now || grant_expires <= now || ends_at.is_some_and(|end| end <= now) {
        return Err(RefreshError::Invalid("the app's access has expired"));
    }
    if user_off || approver_off {
        return Err(RefreshError::Invalid("the account is disabled"));
    }
    if no_two_step && two_step_required {
        return Err(RefreshError::Invalid(TWO_STEP_REQUIRED));
    }
    let on_record: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_tokens WHERE grant_id = ? AND kind = 'refresh'")
        .bind(grant_id)
        .fetch_one(&mut *tx)
        .await?;
    if on_record >= MAX_REFRESH_TOKENS_ON_RECORD {
        end_grant_in(&mut tx, grant_id).await?;
        tx.commit().await?;
        return Err(RefreshError::Ended { grant_id });
    }
    let scopes = Scopes::from_stored(&scope);
    if input.scope.is_some_and(|asked| !asked.is_subset_of(scopes)) {
        return Err(RefreshError::WiderScope);
    }
    let spent = sqlx::query("UPDATE app_tokens SET used_at = ? WHERE token_hash = ? AND used_at IS NULL")
        .bind(now)
        .bind(&key)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if spent == 0 {
        // Spent by another exchange in the meantime: two holders.
        end_grant_in(&mut tx, grant_id).await?;
        tx.commit().await?;
        return Err(RefreshError::Reused { grant_id });
    }
    let refresh_token = insert_token(&mut tx, grant_id, "refresh", capped(now + GRANT_IDLE_SECS, ends_at)).await?;
    let access_expires = capped(now + ACCESS_TOKEN_SECS, ends_at);
    let access_token = insert_token(&mut tx, grant_id, "access", access_expires).await?;
    sqlx::query("UPDATE app_grants SET last_used_at = ?, expires_at = ?, user_agent = ?, last_ip = ? WHERE id = ?")
        .bind(now)
        .bind(now + GRANT_IDLE_SECS)
        .bind(agent(input.user_agent.as_deref()))
        .bind(&input.ip)
        .bind(grant_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM app_tokens WHERE grant_id = ? AND expires_at <= ?")
        .bind(grant_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM app_tokens WHERE grant_id = ? AND kind = 'access' AND token_hash != ? AND token_hash NOT IN
           (SELECT token_hash FROM app_tokens WHERE grant_id = ? AND kind = 'access' AND token_hash != ?
            ORDER BY expires_at DESC LIMIT ?)",
    )
    .bind(grant_id)
    .bind(hash_token(&access_token))
    .bind(grant_id)
    .bind(hash_token(&access_token))
    .bind(MAX_LIVE_ACCESS_TOKENS - 1)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Issued { grant_id, access_token, refresh_token: Some(refresh_token), expires_in: access_expires - now, scopes })
}

/// The grant of a refresh token about to be shown for a profile switch,
/// read without spending the token (see [`holder`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holder {
    pub grant_id: i64,
    pub client_id: String,
    pub scopes: Scopes,
    pub approved_by: i64,
    /// The profile the app acts as, with the app's rights: never an
    /// administrator's (as [`super::AuthService::validate_app`]).
    pub user: super::AuthUser,
}

/// Reads the grant of refresh token `token` of app `client_id`, as a
/// refresh would ([`refresh`]), without spending it: unknown,
/// another app's, expired, a disabled profile or approver, one without
/// two-step sign-in when `two_step_required`, or a grant past its refresh
/// limit get nothing. A spent one ends its grant, as a refresh does: someone
/// has a copy.
pub async fn holder(db: &Db, token: &str, client_id: &str, two_step_required: bool) -> Result<Holder, RefreshError> {
    if !token.starts_with(REFRESH_PREFIX) {
        return Err(RefreshError::Invalid("unknown refresh token"));
    }
    let key = hash_token(token);
    let now = now();
    let mut tx = db.write.begin().await?;
    #[allow(clippy::type_complexity)]
    let row: Option<(i64, i64, Option<i64>, String, String, i64, i64, Option<i64>, bool, bool, bool)> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT t.grant_id, t.expires_at, t.used_at, g.client_id, g.scope, g.approved_by, g.expires_at,
                    g.ends_at, u.disabled, a.disabled, {} OR {}
             FROM app_tokens t
             JOIN app_grants g ON g.id = t.grant_id
             JOIN users u ON u.id = g.user_id
             JOIN users a ON a.id = g.approved_by
             WHERE t.token_hash = ? AND t.kind = 'refresh'",
            super::without_two_step("u"),
            super::without_two_step("a")
        )))
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((
        grant_id,
        token_expires,
        used_at,
        owner,
        scope,
        approved_by,
        grant_expires,
        ends_at,
        user_off,
        approver_off,
        no_two_step,
    )) = row
    else {
        return Err(RefreshError::Invalid("unknown refresh token"));
    };
    if owner != client_id {
        return Err(RefreshError::Invalid("this refresh token belongs to another app"));
    }
    if used_at.is_some() {
        end_grant_in(&mut tx, grant_id).await?;
        tx.commit().await?;
        return Err(RefreshError::Reused { grant_id });
    }
    if token_expires <= now || grant_expires <= now || ends_at.is_some_and(|end| end <= now) {
        return Err(RefreshError::Invalid("the app's access has expired"));
    }
    if user_off || approver_off {
        return Err(RefreshError::Invalid("the account is disabled"));
    }
    if no_two_step && two_step_required {
        return Err(RefreshError::Invalid(TWO_STEP_REQUIRED));
    }
    let on_record: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_tokens WHERE grant_id = ? AND kind = 'refresh'")
        .bind(grant_id)
        .fetch_one(&mut *tx)
        .await?;
    if on_record >= MAX_REFRESH_TOKENS_ON_RECORD {
        end_grant_in(&mut tx, grant_id).await?;
        tx.commit().await?;
        return Err(RefreshError::Ended { grant_id });
    }
    let (user_id, username, role_admin, all_libraries, max_age): (i64, String, bool, bool, Option<i64>) =
        sqlx::query_as(
            "SELECT u.id, u.username, u.is_admin, u.all_libraries, u.max_age
             FROM app_grants g JOIN users u ON u.id = g.user_id WHERE g.id = ?",
        )
        .bind(grant_id)
        .fetch_one(&mut *tx)
        .await?;
    let scopes = Scopes::from_stored(&scope);
    let user = super::AuthUser {
        id: user_id,
        username,
        is_admin: false,
        account_admin: role_admin,
        all_libraries: all_libraries || role_admin,
        max_age: if role_admin { None } else { max_age },
        app: Some(std::sync::Arc::new(super::AppAccess {
            grant_id,
            client_id: owner.as_str().into(),
            scopes,
            approved_by,
        })),
        compatibility: None,
    };
    Ok(Holder { grant_id, client_id: owner, scopes, approved_by, user })
}

/// A profile switch (pley's switch-profile grant, an extension grant of RFC
/// 6749 §4.5), once the switch is allowed.
#[derive(Debug, Clone)]
pub struct SwitchInput {
    /// The refresh token shown: spent by the switch.
    pub token: String,
    pub grant_id: i64,
    /// The profile the app acts as from now on.
    pub user_id: i64,
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

/// Moves a grant to another profile in one transaction: the refresh token
/// shown is spent (one spent meanwhile means two holders, and ends the
/// grant), the grant acts as the new profile (whoever allowed it, its app
/// and its scope stay), every access token it had stops working, and a new
/// pair is issued. The new profile's grants past [`MAX_GRANTS_PER_USER`]
/// end (the least recently used): their ids come second.
pub async fn switch_profile(db: &Db, input: SwitchInput) -> Result<(Issued, Vec<i64>), RefreshError> {
    let key = hash_token(&input.token);
    let now = now();
    let mut tx = db.write.begin().await?;
    let spent = sqlx::query(
        "UPDATE app_tokens SET used_at = ? WHERE token_hash = ? AND grant_id = ? AND kind = 'refresh' AND used_at IS NULL",
    )
    .bind(now)
    .bind(&key)
    .bind(input.grant_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if spent == 0 {
        // Spent by another exchange in the meantime: two holders, and its
        // grant ends. Or gone with its grant, whose number may be another
        // grant's by now: nothing to end.
        let owner: Option<i64> =
            sqlx::query_scalar("SELECT grant_id FROM app_tokens WHERE token_hash = ? AND kind = 'refresh'")
                .bind(&key)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(grant_id) = owner else {
            return Err(RefreshError::Invalid("unknown refresh token"));
        };
        end_grant_in(&mut tx, grant_id).await?;
        tx.commit().await?;
        return Err(RefreshError::Reused { grant_id });
    }
    let row: Option<(String, Option<i64>)> = sqlx::query_as(
        "UPDATE app_grants SET user_id = ?, last_used_at = ?, expires_at = ?, user_agent = ?, last_ip = ?
         WHERE id = ? RETURNING scope, ends_at",
    )
    .bind(input.user_id)
    .bind(now)
    .bind(now + GRANT_IDLE_SECS)
    .bind(agent(input.user_agent.as_deref()))
    .bind(&input.ip)
    .bind(input.grant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((scope, ends_at)) = row else {
        return Err(RefreshError::Invalid("unknown refresh token"));
    };
    // Tokens of the profile it acted as stop now, and expired ones go.
    sqlx::query("DELETE FROM app_tokens WHERE grant_id = ? AND (kind = 'access' OR expires_at <= ?)")
        .bind(input.grant_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let refresh_token =
        insert_token(&mut tx, input.grant_id, "refresh", capped(now + GRANT_IDLE_SECS, ends_at)).await?;
    let access_expires = capped(now + ACCESS_TOKEN_SECS, ends_at);
    let access_token = insert_token(&mut tx, input.grant_id, "access", access_expires).await?;
    let dropped: Vec<i64> = sqlx::query_scalar(
        "DELETE FROM app_grants WHERE user_id = ? AND id NOT IN
           (SELECT id FROM app_grants WHERE user_id = ? ORDER BY last_used_at DESC, id DESC LIMIT ?)
         RETURNING id",
    )
    .bind(input.user_id)
    .bind(input.user_id)
    .bind(MAX_GRANTS_PER_USER)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let issued = Issued {
        grant_id: input.grant_id,
        access_token,
        refresh_token: Some(refresh_token),
        expires_in: access_expires - now,
        scopes: Scopes::from_stored(&scope),
    };
    Ok((issued, dropped))
}

async fn end_grant_in(tx: &mut Transaction<'_, Sqlite>, grant_id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM app_grants WHERE id = ?").bind(grant_id).execute(&mut **tx).await?;
    Ok(())
}

/// The grant an app token (access or refresh, spent or not) belongs to, and
/// its app: `(grant id, client_id)`.
pub async fn find_grant_of_token(db: &Db, token: &str) -> Result<Option<(i64, String)>, AppError> {
    if !token.starts_with(APP_TOKEN_PREFIX) {
        return Ok(None);
    }
    Ok(sqlx::query_as(
        "SELECT g.id, g.client_id FROM app_tokens t JOIN app_grants g ON g.id = t.grant_id WHERE t.token_hash = ?",
    )
    .bind(hash_token(token))
    .fetch_optional(&db.read)
    .await?)
}

/// What grant `grant_id` allows, while it lasts: not ended, not expired, and
/// its approver not disabled; `None` otherwise. For requests that carry a
/// session's key instead of the app's token (keyed playback URLs).
pub async fn access_of(db: &Db, grant_id: i64) -> Result<Option<super::AppAccess>, AppError> {
    let row: Option<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.client_id, g.scope, g.approved_by FROM app_grants g JOIN users a ON a.id = g.approved_by
          WHERE g.id = ? AND g.expires_at > ? AND (g.ends_at IS NULL OR g.ends_at > ?) AND NOT a.disabled",
    )
    .bind(grant_id)
    .bind(now())
    .bind(now())
    .fetch_optional(&db.read)
    .await?;
    Ok(row.map(|(grant_id, client_id, scope, approved_by)| super::AppAccess {
        grant_id,
        client_id: client_id.into(),
        scopes: Scopes::parse(&scope).unwrap_or_default(),
        approved_by,
    }))
}

/// The grants acting as `user_id` or allowed by them, most recently used first.
pub async fn list(db: &Db, user_id: i64) -> Result<Vec<GrantRow>, AppError> {
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        i64,
        i64,
        String,
        i64,
        String,
        String,
        Option<String>,
        String,
        i64,
        i64,
        i64,
        Option<i64>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT g.id, g.user_id, COALESCE(NULLIF(TRIM(u.display_name), ''), u.username),
                g.approved_by, COALESCE(NULLIF(TRIM(a.display_name), ''), a.username),
                g.client_id, g.client_name, g.scope, g.created_at, g.last_used_at, g.expires_at, g.ends_at,
                g.user_agent, g.last_ip
         FROM app_grants g
         JOIN users u ON u.id = g.user_id
         JOIN users a ON a.id = g.approved_by
         WHERE g.user_id = ? OR g.approved_by = ?
         ORDER BY g.last_used_at DESC, g.id DESC",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&db.read)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                user_id,
                user_name,
                approved_by,
                approved_by_name,
                client_id,
                client_name,
                scope,
                created_at,
                last_used_at,
                expires_at,
                ends_at,
                user_agent,
                last_ip,
            )| GrantRow {
                id,
                user_id,
                user_name,
                approved_by,
                approved_by_name,
                client_id,
                client_name,
                scopes: Scopes::from_stored(&scope),
                created_at,
                last_used_at,
                expires_at,
                ends_at,
                user_agent,
                last_ip,
            },
        )
        .collect())
}

/// Ends one grant (its tokens go with it); whether there was one.
pub async fn delete(db: &Db, grant_id: i64) -> Result<bool, AppError> {
    let deleted = sqlx::query("DELETE FROM app_grants WHERE id = ?").bind(grant_id).execute(&db.write).await?;
    Ok(deleted.rows_affected() > 0)
}

/// Ends the grant of the token whose SHA-256 is `token_hash`, if it is
/// still there; its id. A token belongs to one grant only, while a grant's
/// number is given again once the newest grant is gone (SQLite's rowid):
/// what was remembered of a grant a while ago ends only that grant.
pub async fn delete_of_token(db: &Db, token_hash: &str) -> Result<Option<i64>, AppError> {
    Ok(sqlx::query_scalar(
        "DELETE FROM app_grants WHERE id = (SELECT grant_id FROM app_tokens WHERE token_hash = ?) RETURNING id",
    )
    .bind(token_hash)
    .fetch_optional(&db.write)
    .await?)
}

/// Ends the grants acting as `user_id` or allowed by them; their ids.
pub async fn delete_for(db: &Db, user_id: i64) -> Result<Vec<i64>, AppError> {
    Ok(sqlx::query_scalar("DELETE FROM app_grants WHERE user_id = ? OR approved_by = ? RETURNING id")
        .bind(user_id)
        .bind(user_id)
        .fetch_all(&db.write)
        .await?)
}

/// Ends the grants of every app but `clients` (pley's own, when apps by
/// other publishers are turned off); their ids.
pub async fn delete_except_clients(db: &Db, clients: &[&str]) -> Result<Vec<i64>, AppError> {
    let mut sql = String::from("DELETE FROM app_grants");
    if !clients.is_empty() {
        sql.push_str(" WHERE client_id NOT IN (");
        sql.push_str(&vec!["?"; clients.len()].join(", "));
        sql.push(')');
    }
    sql.push_str(" RETURNING id");
    let mut query = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()));
    for client in clients {
        query = query.bind(*client);
    }
    Ok(query.fetch_all(&db.write).await?)
}

/// Forgets expired tokens, and grants unused for [`GRANT_IDLE_SECS`] or past
/// their end.
pub async fn purge(db: &Db) -> Result<(), AppError> {
    let now = now();
    sqlx::query("DELETE FROM app_tokens WHERE expires_at <= ?").bind(now).execute(&db.write).await?;
    sqlx::query("DELETE FROM app_grants WHERE expires_at <= ? OR (ends_at IS NOT NULL AND ends_at <= ?)")
        .bind(now)
        .bind(now)
        .execute(&db.write)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::scopes::Scope;

    const ANDROID: &str = "https://pley-media-server.github.io/pley/oauth/android.json";
    const OTHER: &str = "https://apps.example.com/pley.json";

    async fn database(dir: &std::path::Path) -> Db {
        let db = Db::open(&dir.join("pley.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, is_admin, created_at)
             VALUES (1, 'ann', 'x', 0, 0), (2, 'bob', 'x', 0, 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        db
    }

    fn grant(client_id: &str, scopes: &str) -> NewGrant {
        NewGrant {
            user_id: 1,
            approved_by: 1,
            client_id: client_id.into(),
            client_name: Some("pley".into()),
            scopes: Scopes::parse(scopes).unwrap(),
            refresh: true,
            first_party: client_id == ANDROID,
            session_hash: None,
            user_agent: Some("okhttp".into()),
            ip: Some("10.0.0.2".into()),
        }
    }

    fn exchange(token: &str, client_id: &str) -> RefreshInput {
        RefreshInput {
            token: token.into(),
            client_id: client_id.into(),
            scope: None,
            user_agent: Some("okhttp/5".into()),
            ip: Some("10.0.0.3".into()),
        }
    }

    async fn grants(db: &Db) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&db.read).await.unwrap()
    }

    async fn tokens(db: &Db) -> Vec<(String, String, Option<i64>)> {
        sqlx::query_as("SELECT token_hash, kind, used_at FROM app_tokens ORDER BY kind, token_hash")
            .fetch_all(&db.read)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn requiring_two_step_sign_in_leaves_apps_of_accounts_without_it_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (issued, _) = issue(&db, grant(ANDROID, "library profiles")).await.unwrap();
        let token = issued.refresh_token.unwrap();
        // ann signs in with her password alone.
        let refused = refresh(&db, exchange(&token, ANDROID), true).await;
        assert!(matches!(refused, Err(RefreshError::Invalid(TWO_STEP_REQUIRED))), "{refused:?}");
        assert!(matches!(holder(&db, &token, ANDROID, true).await, Err(RefreshError::Invalid(TWO_STEP_REQUIRED))));
        // Nothing was spent: once she uses it, the app goes on.
        sqlx::query("INSERT INTO two_step (user_id, secret, created_at) VALUES (1, 'sealed', 0)")
            .execute(&db.write)
            .await
            .unwrap();
        holder(&db, &token, ANDROID, true).await.unwrap();
        let next = refresh(&db, exchange(&token, ANDROID), true).await.unwrap();
        // Allowed by bob, who doesn't use it.
        sqlx::query("UPDATE app_grants SET approved_by = 2").execute(&db.write).await.unwrap();
        let token = next.refresh_token.unwrap();
        let refused = refresh(&db, exchange(&token, ANDROID), true).await;
        assert!(matches!(refused, Err(RefreshError::Invalid(TWO_STEP_REQUIRED))), "{refused:?}");
        // An account without a password never needs it.
        sqlx::query("UPDATE users SET password_hash = '!managed' WHERE id = 2").execute(&db.write).await.unwrap();
        refresh(&db, exchange(&token, ANDROID), true).await.unwrap();
    }

    #[tokio::test]
    async fn app_tokens_are_prefixed_and_stored_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (issued, dropped) = issue(&db, grant(ANDROID, "library play")).await.unwrap();
        assert!(dropped.is_empty());
        assert!(issued.access_token.starts_with(ACCESS_PREFIX) && issued.access_token.len() == 8 + 43);
        let refresh = issued.refresh_token.clone().unwrap();
        assert!(refresh.starts_with(REFRESH_PREFIX) && refresh.len() == 8 + 43);
        assert_eq!(issued.expires_in, ACCESS_TOKEN_SECS);
        assert_eq!(issued.scopes.to_string(), "library play");
        let stored = tokens(&db).await;
        assert_eq!(stored.len(), 2);
        for (hash, ..) in &stored {
            assert_eq!(hash.len(), 64);
            assert_ne!(hash, &issued.access_token);
            assert_ne!(hash, &refresh);
        }
        assert!(stored.iter().any(|(hash, kind, _)| *hash == hash_token(&issued.access_token) && kind == "access"));
        assert!(stored.iter().any(|(hash, kind, _)| *hash == hash_token(&refresh) && kind == "refresh"));
        // Nothing in the database holds a token itself.
        let dump: Vec<String> =
            sqlx::query_scalar("SELECT token_hash || grant_id || kind || expires_at FROM app_tokens")
                .fetch_all(&db.read)
                .await
                .unwrap();
        assert!(dump.iter().all(|row| !row.contains(&issued.access_token[8..]) && !row.contains(&refresh[8..])));
        let (scope, agent): (String, String) =
            sqlx::query_as("SELECT scope, user_agent FROM app_grants").fetch_one(&db.read).await.unwrap();
        assert_eq!((scope.as_str(), agent.as_str()), ("library play", "okhttp"));
        assert_eq!(find_grant_of_token(&db, &refresh).await.unwrap(), Some((issued.grant_id, ANDROID.to_owned())));
        assert_eq!(find_grant_of_token(&db, "pley_at_unknown").await.unwrap(), None);
    }

    #[tokio::test]
    async fn refresh_tokens_rotate_and_a_reused_one_ends_the_grant() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let old = first.refresh_token.clone().unwrap();
        let second = refresh(&db, exchange(&old, ANDROID), false).await.unwrap();
        assert_eq!(second.grant_id, first.grant_id);
        let new = second.refresh_token.clone().unwrap();
        assert_ne!(new, old);
        assert_ne!(second.access_token, first.access_token);
        let (agent, ip): (String, String) =
            sqlx::query_as("SELECT user_agent, last_ip FROM app_grants").fetch_one(&db.read).await.unwrap();
        assert_eq!((agent.as_str(), ip.as_str()), ("okhttp/5", "10.0.0.3"));

        // The spent token is kept, and shown again it ends everything.
        let err = refresh(&db, exchange(&old, ANDROID), false).await.unwrap_err();
        assert!(matches!(err, RefreshError::Reused { grant_id } if grant_id == first.grant_id), "{err:?}");
        assert_eq!(grants(&db).await, 0);
        assert!(tokens(&db).await.is_empty(), "tokens go with their grant");
        assert!(matches!(refresh(&db, exchange(&new, ANDROID), false).await, Err(RefreshError::Invalid(_))));
    }

    #[tokio::test]
    async fn a_refresh_token_used_after_two_rotations_still_ends_the_grant() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let stolen = first.refresh_token.unwrap();
        let second = refresh(&db, exchange(&stolen, ANDROID), false).await.unwrap();
        let third = refresh(&db, exchange(second.refresh_token.as_ref().unwrap(), ANDROID), false).await.unwrap();
        assert!(third.refresh_token.is_some());
        let err = refresh(&db, exchange(&stolen, ANDROID), false).await.unwrap_err();
        assert!(matches!(err, RefreshError::Reused { .. }), "{err:?}");
        assert_eq!(grants(&db).await, 0);
    }

    #[tokio::test]
    async fn two_refreshes_with_one_token_end_the_grant() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let token = first.refresh_token.unwrap();
        let (a, b) = tokio::join!(
            refresh(&db, exchange(&token, ANDROID), false),
            refresh(&db, exchange(&token, ANDROID), false)
        );
        assert!(a.is_ok() != b.is_ok(), "one gets a pair, the other ends the grant");
        assert!(matches!(a.err().or(b.err()), Some(RefreshError::Reused { .. })));
        assert_eq!(grants(&db).await, 0);
    }

    #[tokio::test]
    async fn unknown_refresh_tokens_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let before = tokens(&db).await;
        for token in ["pley_rt_nothing", "garbage", "", &first.access_token] {
            let err = refresh(&db, exchange(token, ANDROID), false).await.unwrap_err();
            assert!(matches!(err, RefreshError::Invalid(_)), "{token}: {err:?}");
        }
        assert_eq!(tokens(&db).await, before);
        assert!(refresh(&db, exchange(first.refresh_token.as_ref().unwrap(), ANDROID), false).await.is_ok());
    }

    #[tokio::test]
    async fn another_clients_refresh_token_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let token = first.refresh_token.unwrap();
        let before = tokens(&db).await;
        let err = refresh(&db, exchange(&token, OTHER), false).await.unwrap_err();
        assert!(matches!(err, RefreshError::Invalid(_)), "{err:?}");
        assert_eq!(tokens(&db).await, before, "not spent");
        assert!(refresh(&db, exchange(&token, ANDROID), false).await.is_ok());
    }

    #[tokio::test]
    async fn refresh_never_widens_scope() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library play")).await.unwrap();
        let token = first.refresh_token.unwrap();
        let mut wider = exchange(&token, ANDROID);
        wider.scope = Some(Scopes::all());
        assert!(matches!(refresh(&db, wider, false).await, Err(RefreshError::WiderScope)));
        // Narrower: ignored, the answer says what was issued.
        let mut narrower = exchange(&token, ANDROID);
        narrower.scope = Some(Scopes::default().with(Scope::Library));
        let issued = refresh(&db, narrower, false).await.expect("the refused one wasn't spent");
        assert_eq!(issued.scopes.to_string(), "library play");
        let stored: String = sqlx::query_scalar("SELECT scope FROM app_grants").fetch_one(&db.read).await.unwrap();
        assert_eq!(stored, "library play");
    }

    #[tokio::test]
    async fn clients_without_refresh_tokens_get_none() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let mut new = grant(OTHER, "library");
        new.refresh = false;
        let (issued, _) = issue(&db, new).await.unwrap();
        assert_eq!(issued.refresh_token, None);
        let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM app_tokens").fetch_all(&db.read).await.unwrap();
        assert_eq!(kinds, ["access"]);
    }

    #[tokio::test]
    async fn grants_expire_after_90_days_unused_and_other_publishers_after_a_year() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (own, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let (other, _) = issue(&db, grant(OTHER, "library")).await.unwrap();
        let row = |id: i64| {
            let db = db.clone();
            async move {
                sqlx::query_as::<_, (i64, i64, Option<i64>)>(
                    "SELECT created_at, expires_at, ends_at FROM app_grants WHERE id = ?",
                )
                .bind(id)
                .fetch_one(&db.read)
                .await
                .unwrap()
            }
        };
        let (created, expires, ends) = row(own.grant_id).await;
        assert_eq!((expires - created, ends), (GRANT_IDLE_SECS, None), "pley's own apps have no yearly end");
        let (created, expires, ends) = row(other.grant_id).await;
        assert_eq!((expires - created, ends.map(|end| end - created)), (GRANT_IDLE_SECS, Some(THIRD_PARTY_GRANT_SECS)));

        // 90 days unused: refused, then purged.
        sqlx::query("UPDATE app_grants SET expires_at = ? WHERE id = ?")
            .bind(now() - 1)
            .bind(own.grant_id)
            .execute(&db.write)
            .await
            .unwrap();
        let err = refresh(&db, exchange(own.refresh_token.as_ref().unwrap(), ANDROID), false).await.unwrap_err();
        assert!(matches!(err, RefreshError::Invalid(_)), "{err:?}");
        // A year for other publishers, however recently used.
        sqlx::query("UPDATE app_grants SET ends_at = ? WHERE id = ?")
            .bind(now() - 1)
            .bind(other.grant_id)
            .execute(&db.write)
            .await
            .unwrap();
        let err = refresh(&db, exchange(other.refresh_token.as_ref().unwrap(), OTHER), false).await.unwrap_err();
        assert!(matches!(err, RefreshError::Invalid(_)), "{err:?}");
        let (third, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        sqlx::query("UPDATE app_tokens SET expires_at = ? WHERE token_hash = ?")
            .bind(now() - 1)
            .bind(hash_token(&third.access_token))
            .execute(&db.write)
            .await
            .unwrap();
        purge(&db).await.unwrap();
        let left: Vec<i64> = sqlx::query_scalar("SELECT id FROM app_grants").fetch_all(&db.read).await.unwrap();
        assert_eq!(left, [third.grant_id]);
        let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM app_tokens").fetch_all(&db.read).await.unwrap();
        assert_eq!(kinds, ["refresh"], "the expired access token went");

        // A refresh keeps an app of another publisher within its year.
        let (fourth, _) = issue(&db, grant(OTHER, "library")).await.unwrap();
        let end = now() + 100;
        sqlx::query("UPDATE app_grants SET ends_at = ? WHERE id = ?")
            .bind(end)
            .bind(fourth.grant_id)
            .execute(&db.write)
            .await
            .unwrap();
        let next = refresh(&db, exchange(fourth.refresh_token.as_ref().unwrap(), OTHER), false).await.unwrap();
        assert!(next.expires_in <= 100, "never past the grant's end");
    }

    #[tokio::test]
    async fn a_grant_refreshed_too_often_ends() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        sqlx::query(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?)
             INSERT INTO app_tokens (token_hash, grant_id, kind, expires_at, used_at)
             SELECT 'spent' || i, ?, 'refresh', ?, ? FROM n",
        )
        .bind(MAX_REFRESH_TOKENS_ON_RECORD - 1)
        .bind(first.grant_id)
        .bind(now() + 1000)
        .bind(now())
        .execute(&db.write)
        .await
        .unwrap();
        let err = refresh(&db, exchange(first.refresh_token.as_ref().unwrap(), ANDROID), false).await.unwrap_err();
        assert!(matches!(err, RefreshError::Ended { grant_id } if grant_id == first.grant_id), "{err:?}");
        assert_eq!(grants(&db).await, 0);
    }

    #[tokio::test]
    async fn access_tokens_per_grant_are_capped() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (mut issued, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        for _ in 0..15 {
            issued = refresh(&db, exchange(issued.refresh_token.as_ref().unwrap(), ANDROID), false).await.unwrap();
        }
        let access: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM app_tokens WHERE kind = 'access'")
            .fetch_all(&db.read)
            .await
            .unwrap();
        assert_eq!(access.len() as i64, MAX_LIVE_ACCESS_TOKENS);
        assert!(access.contains(&hash_token(&issued.access_token)), "the newest is kept");
    }

    #[tokio::test]
    async fn grants_per_user_are_capped() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (oldest, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let (used, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        // The second is used (later) than every other: it stays.
        sqlx::query("UPDATE app_grants SET last_used_at = last_used_at + 1000 WHERE id = ?")
            .bind(used.grant_id)
            .execute(&db.write)
            .await
            .unwrap();
        let mut bobs = grant(ANDROID, "library");
        bobs.user_id = 2;
        bobs.approved_by = 2;
        let (bob, _) = issue(&db, bobs).await.unwrap();
        for _ in 2..MAX_GRANTS_PER_USER {
            let (_, dropped) = issue(&db, grant(ANDROID, "library")).await.unwrap();
            assert!(dropped.is_empty());
        }
        let (_, dropped) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        assert_eq!(dropped, [oldest.grant_id], "the least recently used one ends");
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM app_grants WHERE user_id = 1").fetch_one(&db.read).await.unwrap();
        assert_eq!(count, MAX_GRANTS_PER_USER);
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM app_grants").fetch_all(&db.read).await.unwrap();
        assert!(ids.contains(&used.grant_id) && ids.contains(&bob.grant_id), "other people's grants stay");
    }

    fn moving(token: &str, grant_id: i64, user_id: i64) -> SwitchInput {
        SwitchInput {
            token: token.into(),
            grant_id,
            user_id,
            user_agent: Some("okhttp/5".into()),
            ip: Some("10.0.0.4".into()),
        }
    }

    #[tokio::test]
    async fn a_profile_switch_reads_without_spending_and_moves_only_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        sqlx::query("UPDATE users SET is_admin = 1, max_age = 12 WHERE id = 1").execute(&db.write).await.unwrap();
        let (first, _) = issue(&db, grant(ANDROID, "library profiles")).await.unwrap();
        let token = first.refresh_token.unwrap();
        let before = tokens(&db).await;
        let held = holder(&db, &token, ANDROID, false).await.unwrap();
        assert_eq!(tokens(&db).await, before, "reading spends nothing");
        assert_eq!((held.grant_id, held.approved_by), (first.grant_id, 1));
        assert_eq!(held.scopes.to_string(), "library profiles");
        // The app's rights: an administrator's role gives every library and
        // no age limit, never administration.
        assert_eq!((held.user.id, held.user.is_admin, held.user.all_libraries), (1, false, true));
        assert_eq!(held.user.max_age, None);
        assert!(matches!(holder(&db, &token, OTHER, false).await, Err(RefreshError::Invalid(_))));
        assert!(matches!(holder(&db, &first.access_token, ANDROID, false).await, Err(RefreshError::Invalid(_))));

        let (moved, dropped) = switch_profile(&db, moving(&token, first.grant_id, 2)).await.unwrap();
        assert!(dropped.is_empty());
        assert_eq!((moved.grant_id, moved.scopes.to_string()), (first.grant_id, "library profiles".to_owned()));
        let row: (i64, i64, String, String, String) =
            sqlx::query_as("SELECT user_id, approved_by, client_id, scope, last_ip FROM app_grants")
                .fetch_one(&db.read)
                .await
                .unwrap();
        assert_eq!(row, (2, 1, ANDROID.to_owned(), "library profiles".to_owned(), "10.0.0.4".to_owned()));
        let access: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM app_tokens WHERE kind = 'access'")
            .fetch_all(&db.read)
            .await
            .unwrap();
        assert_eq!(access, [hash_token(&moved.access_token)], "the old access token is gone");
        // The token shown was spent: shown again, it ends the grant.
        let err = holder(&db, &token, ANDROID, false).await.unwrap_err();
        assert!(matches!(err, RefreshError::Reused { .. }), "{err:?}");
        assert_eq!(grants(&db).await, 0);
    }

    #[tokio::test]
    async fn a_refresh_token_spent_while_switching_ends_the_grant() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (first, _) = issue(&db, grant(ANDROID, "library profiles")).await.unwrap();
        let token = first.refresh_token.unwrap();
        holder(&db, &token, ANDROID, false).await.unwrap();
        // A refresh with the same token gets there first: two holders.
        refresh(&db, exchange(&token, ANDROID), false).await.unwrap();
        let err = switch_profile(&db, moving(&token, first.grant_id, 2)).await.unwrap_err();
        assert!(matches!(err, RefreshError::Reused { grant_id } if grant_id == first.grant_id), "{err:?}");
        assert_eq!(grants(&db).await, 0);
    }

    #[tokio::test]
    async fn grants_are_listed_and_ended() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        sqlx::query("UPDATE users SET display_name = 'Ann B' WHERE id = 1").execute(&db.write).await.unwrap();
        let (own, _) = issue(&db, grant(ANDROID, "library")).await.unwrap();
        let mut for_bob = grant(OTHER, "play");
        for_bob.user_id = 2;
        let (allowed, _) = issue(&db, for_bob).await.unwrap();
        let mut bobs = grant(ANDROID, "library");
        (bobs.user_id, bobs.approved_by) = (2, 2);
        let (bob, _) = issue(&db, bobs).await.unwrap();

        let anns = list(&db, 1).await.unwrap();
        let ids: Vec<i64> = anns.iter().map(|g| g.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&own.grant_id) && ids.contains(&allowed.grant_id));
        let row = anns.iter().find(|g| g.id == allowed.grant_id).unwrap();
        assert_eq!((row.user_name.as_str(), row.approved_by_name.as_str()), ("bob", "Ann B"));
        assert_eq!(row.scopes.to_string(), "play");
        assert_eq!(list(&db, 2).await.unwrap().len(), 2);

        assert_eq!(delete_except_clients(&db, &[ANDROID]).await.unwrap(), [allowed.grant_id]);
        assert!(delete(&db, bob.grant_id).await.unwrap());
        assert!(!delete(&db, bob.grant_id).await.unwrap());
        assert_eq!(delete_for(&db, 1).await.unwrap(), [own.grant_id]);
        assert_eq!(grants(&db).await, 0);
        issue(&db, grant(OTHER, "library")).await.unwrap();
        assert_eq!(delete_except_clients(&db, &[]).await.unwrap().len(), 1);
    }
}
