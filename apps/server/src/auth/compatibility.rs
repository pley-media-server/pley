//! Tokens for other apps and tools that use the compatibility APIs
//! (docs/COMPATIBILITY.md): players such as Jellyfin's apps or Infuse,
//! library tools such as Kometa, activity monitors such as Tautulli,
//! scripts. An app gets one by signing in to the Jellyfin-compatible API
//! (`/Users/AuthenticateByName`), or a person makes one on the account page.
//! They work on the compatibility APIs only, never on Pley's own, and do
//! only what their [`Scopes`] allow, from where their [`Origins`] allow.
//! No token administers accounts: no scope lets one make, change or delete
//! a user, or change who may open what.
//!
//! Tokens carry a prefix (`pley_ct_`), so that leaked ones are recognisable
//! and never pass for a session's, and are stored as SHA-256, like session
//! tokens. One ends when it is revoked, 90 days after it was last used, and
//! with the account's other sign-ins; it doesn't work while the account is
//! disabled. [`super::AuthService`] validates them, caches what it read and
//! forgets it when they end.

use std::fmt;
use std::net::IpAddr;

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
/// The most origins a token may be limited to.
pub const MAX_ORIGINS: usize = 16;

/// One thing a compatibility token may be allowed to do. Scopes are stored
/// and answered by name, so that a later one is another name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Browse what the account may open, play, and keep its own watch state,
    /// sessions and playlists. Every token has it.
    Play,
    /// See every account's sessions, history and accounts, as activity
    /// monitors do (administrators only). Seeing, not changing.
    Activity,
    /// Refresh libraries and items, stop sessions, edit metadata, and make
    /// and change collections, as library tools do (administrators only).
    /// Never accounts, sign-in or the server's settings.
    Manage,
}

impl Scope {
    /// Every scope, in the order they are written and shown.
    pub const ALL: [Scope; 3] = [Scope::Play, Scope::Activity, Scope::Manage];

    /// The name in `scope` fields and answers.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Play => "play",
            Scope::Activity => "activity",
            Scope::Manage => "manage",
        }
    }

    pub fn from_name(name: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|scope| scope.name() == name)
    }

    /// Only an administrator's tokens may have it, and it does nothing once
    /// the account is no longer one.
    pub fn administrators_only(self) -> bool {
        self != Scope::Play
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

    /// Whether it has a scope only administrators' tokens may have.
    pub fn administrators_only(self) -> bool {
        Scope::ALL.into_iter().any(|scope| scope.administrators_only() && self.contains(scope))
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

/// Where a token may be used from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// An address, or a network of them (`192.168.1.0/24`): the address
    /// with its host bits cleared, and the length of its prefix.
    Network(IpAddr, u8),
    /// This computer and the home network: loopback, private, link-local
    /// and Tailscale's addresses (see [`crate::setup::is_local_address`]).
    Local,
}

impl Origin {
    /// `192.168.1.20`, `10.0.0.0/24`, `fd00::/8` or `local`.
    pub fn parse(text: &str) -> Option<Origin> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("local") {
            return Some(Origin::Local);
        }
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (text, None),
        };
        let address: IpAddr = address.parse().ok()?;
        let address = address.to_canonical();
        let bits = if address.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(prefix) => prefix.parse::<u8>().ok().filter(|p| *p <= bits)?,
            None => bits,
        };
        Some(Origin::Network(masked(address, prefix), prefix))
    }

    pub fn contains(self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        match self {
            Origin::Local => crate::setup::is_local_address(ip),
            Origin::Network(network, prefix) => ip.is_ipv4() == network.is_ipv4() && masked(ip, prefix) == network,
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Local => f.write_str("local"),
            Origin::Network(address, prefix) if u32::from(*prefix) == if address.is_ipv4() { 32 } else { 128 } => {
                write!(f, "{address}")
            }
            Origin::Network(address, prefix) => write!(f, "{address}/{prefix}"),
        }
    }
}

/// `ip` with all but its first `prefix` bits cleared.
fn masked(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(prefix)) };
            IpAddr::V4((u32::from(v4) & mask).into())
        }
        IpAddr::V6(v6) => {
            let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - u32::from(prefix)) };
            IpAddr::V6((u128::from(v6) & mask).into())
        }
    }
}

/// Where a token may be used from: anywhere, or only from its origins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origins(Vec<Origin>);

impl Origins {
    /// Entries as the account page sends them, each an [`Origin`]; none is
    /// anywhere. The entry that isn't one is the error.
    pub fn parse<S: AsRef<str>>(entries: &[S]) -> Result<Origins, String> {
        let mut origins: Vec<Origin> = Vec::new();
        for entry in entries.iter().map(AsRef::as_ref).filter(|e| !e.trim().is_empty()) {
            let origin = Origin::parse(entry).ok_or_else(|| entry.trim().to_owned())?;
            if !origins.contains(&origin) {
                origins.push(origin);
            }
        }
        Ok(Origins(origins))
    }

    /// Origins stored with a token (space-separated). One this version can't
    /// read is left out, and a token whose every origin is unreadable is used
    /// from nowhere rather than from anywhere.
    pub fn from_stored(text: Option<&str>) -> Origins {
        let Some(text) = text.filter(|t| !t.trim().is_empty()) else { return Origins::default() };
        let origins: Vec<Origin> = text.split(' ').filter_map(Origin::parse).collect();
        if origins.is_empty() { Origins(vec![Origin::Network(IpAddr::from([0u8; 16]), 128)]) } else { Origins(origins) }
    }

    /// What is stored: `None` for anywhere.
    pub fn stored(&self) -> Option<String> {
        (!self.0.is_empty()).then(|| self.names().join(" "))
    }

    pub fn names(&self) -> Vec<String> {
        self.0.iter().map(ToString::to_string).collect()
    }

    pub fn anywhere(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether a request from `ip` may use the token: anywhere, or from one
    /// of its origins. An address that isn't known is none of them.
    pub fn allow(&self, ip: Option<IpAddr>) -> bool {
        self.0.is_empty() || ip.is_some_and(|ip| self.0.iter().any(|origin| origin.contains(ip)))
    }
}

/// A token someone asked for, its name, scope and origins checked ([`asked`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub name: String,
    pub scopes: Scopes,
    pub origins: Origins,
}

/// What a person or an app asked for, as the account page and sign-ins send
/// it: a name (trimmed, 1 to [`MAX_NAME_CHARS`] characters, none of them a
/// control character), a `scope` field (`play` without one) and the
/// origins it may be used from (anywhere without any).
pub fn asked(name: &str, scope: Option<&str>, origins: &[String]) -> Result<Asked, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(AppError::bad_request(format!(
            "name the token with 1 to {MAX_NAME_CHARS} letters, digits, spaces or signs"
        )));
    }
    let scopes = Scopes::parse(scope.unwrap_or_default())
        .map_err(|unknown| AppError::bad_request(format!("unknown scope '{unknown}': use play, activity or manage")))?;
    if origins.len() > MAX_ORIGINS {
        return Err(AppError::bad_request(format!("{MAX_ORIGINS} origins at most")));
    }
    let origins = Origins::parse(origins).map_err(|bad| {
        AppError::bad_request(format!(
            "'{bad}' isn't an address, a network (192.168.1.0/24) or local (this computer and the home network)"
        ))
    })?;
    Ok(Asked { name: name.to_owned(), scopes, origins })
}

/// Refuses scopes only administrators' tokens may have to an account that
/// isn't one (`administrator`: the account's role).
pub fn may_have(scopes: Scopes, administrator: bool) -> Result<(), AppError> {
    if scopes.administrators_only() && !administrator {
        return Err(AppError::forbidden(
            "only administrators can make tokens that see everyone's activity or manage libraries",
        ));
    }
    Ok(())
}

/// A token, for the account page's list.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TokenRow {
    pub id: i64,
    pub name: String,
    pub scope: String,
    /// Its origins, space-separated (`None`: anywhere).
    pub allowed_from: Option<String>,
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

    pub fn origins(&self) -> Origins {
        Origins::from_stored(self.allowed_from.as_deref())
    }
}

const COLUMNS: &str = "id, name, scope, allowed_from, created_at, last_used_at, expires_at, user_agent, last_ip";

/// Stores a new token for `user_id` and hands it out, with its row. The
/// account's tokens past [`MAX_PER_USER`] end (the least recently used):
/// their ids come third.
pub async fn create(db: &Db, user_id: i64, asked: &Asked) -> Result<(String, TokenRow, Vec<i64>), AppError> {
    let token = format!("{PREFIX}{}", new_token());
    let now = now();
    let mut tx = db.write.begin().await?;
    let row: TokenRow = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO compatibility_tokens (user_id, name, scope, allowed_from, token_hash, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING {COLUMNS}"
    )))
    .bind(user_id)
    .bind(&asked.name)
    .bind(asked.scopes.to_string())
    .bind(asked.origins.stored())
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

    fn tool(scope: &str) -> Asked {
        asked("A tool", Some(scope), &[]).unwrap()
    }

    #[test]
    fn scopes_always_play_and_print_in_order() {
        assert_eq!(Scopes::parse("").unwrap(), Scopes::play());
        assert_eq!(Scopes::parse("manage activity").unwrap().to_string(), "play activity manage");
        assert_eq!(Scopes::parse("play  play").unwrap().to_string(), "play");
        assert_eq!(Scopes::parse("activity admin"), Err("admin".to_owned()));
        assert_eq!(Scopes::parse("Play"), Err("Play".to_owned()), "names are lower case");
        assert_eq!(Scopes::parse("users"), Err("users".to_owned()), "no scope manages accounts");
        assert_eq!(Scopes::parse("library"), Err("library".to_owned()), "apps' scopes aren't these");
        assert_eq!(Scopes::from_stored("activity future"), Scopes::parse("activity").unwrap());
        assert_eq!(Scopes::from_stored(""), Scopes::play());
        assert!(!Scopes::play().administrators_only());
        assert!(Scopes::parse("activity").unwrap().administrators_only());
        assert!(Scopes::parse("manage").unwrap().administrators_only());
        assert_eq!(Scopes::parse("manage").unwrap().names(), ["play", "manage"]);
        for scope in Scope::ALL {
            assert_eq!(Scope::from_name(scope.name()), Some(scope));
        }
    }

    #[test]
    fn origins_are_addresses_networks_or_the_home_network() {
        let ip = |text: &str| Some(text.parse::<IpAddr>().unwrap());
        let parsed = Origins::parse(&["192.168.1.20", " 10.1.2.3/16 ", "fd00::1/8", "LOCAL", "10.1.0.0/16"]).unwrap();
        assert_eq!(parsed.names(), ["192.168.1.20", "10.1.0.0/16", "fd00::/8", "local"], "kept once, in order");
        assert_eq!(Origins::from_stored(parsed.stored().as_deref()), parsed);

        let one = Origins::parse(&["192.168.1.20"]).unwrap();
        assert!(one.allow(ip("192.168.1.20")) && one.allow(ip("::ffff:192.168.1.20")), "mapped addresses too");
        assert!(!one.allow(ip("192.168.1.21")) && !one.allow(None), "an unknown address is none of them");
        let network = Origins::parse(&["10.0.0.0/8", "2001:db8::/32"]).unwrap();
        assert!(network.allow(ip("10.200.3.4")) && network.allow(ip("2001:db8::7")));
        assert!(!network.allow(ip("11.0.0.1")) && !network.allow(ip("2001:db9::7")));
        let local = Origins::parse(&["local"]).unwrap();
        assert!(local.allow(ip("127.0.0.1")) && local.allow(ip("192.168.4.5")) && local.allow(ip("100.70.0.2")));
        assert!(!local.allow(ip("8.8.8.8")));

        let anywhere = Origins::parse::<&str>(&[]).unwrap();
        assert!(anywhere.anywhere() && anywhere.allow(None) && anywhere.stored().is_none());
        assert_eq!(Origins::from_stored(Some("  ")), anywhere);
        for bad in ["somewhere", "10.0.0.0/33", "fd00::/129", "10.0.0/8", "/8", "*"] {
            assert_eq!(Origins::parse(&[bad]), Err(bad.to_owned()), "{bad}");
        }
        // Stored origins none of which can be read: from nowhere.
        assert!(!Origins::from_stored(Some("garbled")).allow(ip("127.0.0.1")));
    }

    #[test]
    fn names_scopes_and_origins_are_checked() {
        let plain = asked("  Kometa ", None, &[]).unwrap();
        assert_eq!(plain, Asked { name: "Kometa".into(), scopes: Scopes::play(), origins: Origins::default() });
        assert_eq!(asked(&"é".repeat(MAX_NAME_CHARS), None, &[]).unwrap().name.chars().count(), MAX_NAME_CHARS);
        for bad in ["", "   ", "tab\there", "new\nline"] {
            assert!(matches!(asked(bad, None, &[]), Err(AppError::BadRequest(_))), "{bad:?}");
        }
        assert!(matches!(asked(&"a".repeat(MAX_NAME_CHARS + 1), None, &[]), Err(AppError::BadRequest(_))));
        let Err(AppError::BadRequest(message)) = asked("x", Some("play everything"), &[]) else { panic!() };
        assert!(message.contains("'everything'"), "{message}");
        let Err(AppError::BadRequest(message)) = asked("x", None, &["here".into()]) else { panic!() };
        assert!(message.contains("'here'"), "{message}");
        let many: Vec<String> = (0..=MAX_ORIGINS).map(|i| format!("10.0.0.{i}")).collect();
        assert!(matches!(asked("x", None, &many), Err(AppError::BadRequest(_))));
        assert!(may_have(Scopes::play(), false).is_ok());
        assert!(may_have(Scopes::parse("activity manage").unwrap(), true).is_ok());
        assert!(matches!(may_have(Scopes::parse("activity").unwrap(), false), Err(AppError::Forbidden(_))));
        assert!(matches!(may_have(Scopes::parse("manage").unwrap(), false), Err(AppError::Forbidden(_))));
    }

    #[tokio::test]
    async fn tokens_are_prefixed_and_stored_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let limited = asked("A tool", Some("activity"), &["10.0.0.0/24".into()]).unwrap();
        let (token, row, dropped) = create(&db, 1, &limited).await.unwrap();
        assert!(dropped.is_empty());
        assert!(token.starts_with(PREFIX) && token.len() == PREFIX.len() + 43, "{token}");
        assert_eq!((row.name.as_str(), row.scope.as_str(), row.last_used_at), ("A tool", "play activity", None));
        assert_eq!(row.origins().names(), ["10.0.0.0/24"]);
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
        let (_, anywhere, _) = create(&db, 2, &tool("play")).await.unwrap();
        assert!(anywhere.allowed_from.is_none() && anywhere.origins().anywhere());
    }

    #[tokio::test]
    async fn tokens_are_listed_ended_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let db = database(dir.path()).await;
        let (_, oldest, _) = create(&db, 1, &tool("play")).await.unwrap();
        let (used, used_row, _) = create(&db, 1, &tool("play")).await.unwrap();
        // The second is used later than every other: it stays, and comes first.
        sqlx::query("UPDATE compatibility_tokens SET last_used_at = created_at + 1000 WHERE id = ?")
            .bind(used_row.id)
            .execute(&db.write)
            .await
            .unwrap();
        let (_, bobs, _) = create(&db, 2, &tool("play")).await.unwrap();
        for _ in 2..MAX_PER_USER {
            let (_, _, dropped) = create(&db, 1, &tool("play")).await.unwrap();
            assert!(dropped.is_empty());
        }
        let (_, _, dropped) = create(&db, 1, &tool("play")).await.unwrap();
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
        let (token, row, _) = create(&db, 2, &tool("play")).await.unwrap();
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

        let (token, row, _) = create(&db, 2, &tool("play")).await.unwrap();
        assert_eq!(delete_token(&db, "pley_at_x").await.unwrap(), None);
        assert_eq!(delete_token(&db, &token).await.unwrap(), Some(row.id));
    }
}
