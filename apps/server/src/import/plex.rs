//! Plex's HTTP API, as the Plex import reads it: plex.tv for signing in
//! (Plex's PIN sign-in), the account's servers and its Home, and a Plex
//! Media Server for its libraries and each person's watch state.
//!
//! * Answers are JSON (`/api/v2/…` and the server's), except plex.tv's older
//!   `/api/users`, the only place a Home user's restrictions are kept, which
//!   is XML.
//! * Every answer is read under [`MAX_ANSWER`], whatever it says its length
//!   is; no redirects are followed.
//! * The server's address comes only from plex.tv's list of the account's
//!   servers, or from the administrator.
//! * Tokens are sent in headers, never in addresses, and never logged.
//! * Link-local addresses, where clouds answer for the machine itself, are
//!   never connected to, as for Radarr and Sonarr.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::de::{self, DeserializeOwned};
use serde::{Deserialize, Deserializer};
use url::Url;

/// plex.tv, where sign-in, servers and Home users are.
pub const PLEX_TV: &str = "https://plex.tv";
/// Plex's sign-in page, opened in the person's browser.
const AUTH_PAGE: &str = "https://app.plex.tv/auth";
/// The largest answer read (a page of a listing is far smaller).
pub const MAX_ANSWER: usize = 8 << 20;
/// Items asked for at a time from a listing.
pub const PAGE: usize = 200;
/// How long a request may take in all.
const TIMEOUT: Duration = Duration::from_secs(30);
/// How long a connection to a server may take: its addresses are tried in
/// turn, and those that don't answer are usually unreachable.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, thiserror::Error)]
pub enum PlexError {
    /// Plex couldn't be reached, or answered with something unreadable.
    #[error("{0}")]
    Unreachable(String),
    /// Plex refused: the token isn't (or is no longer) good, or the account
    /// may not do this.
    #[error("Plex refused the request ({0})")]
    Refused(u16),
    /// Switching to a Home user needs their PIN (Plex's error 1041).
    #[error("this Home user's PIN is needed")]
    NeedsPin,
}

pub type PlexResult<T> = Result<T, PlexError>;

/// Plex's client, for one Pley server: `client_id` is how plex.tv knows it
/// (the PIN and the device in the account's Authorized Devices).
#[derive(Clone)]
pub struct Plex {
    http: reqwest::Client,
    tv: Url,
    client_id: String,
}

impl Plex {
    /// `tv` is plex.tv's address (tests give a local one).
    pub fn new(tv: &str, client_id: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("pley/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TIMEOUT)
            .dns_resolver(Arc::new(NotLinkLocal))
            .build()
            .expect("http client");
        let tv = Url::parse(tv).unwrap_or_else(|_| Url::parse(PLEX_TV).expect("plex.tv"));
        Self { http, tv, client_id: client_id.to_owned() }
    }

    fn headers(&self, token: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        let mut put = |name: &'static str, value: &str| {
            if let Ok(v) = HeaderValue::from_str(value) {
                h.insert(name, v);
            }
        };
        put("X-Plex-Product", "Pley");
        put("X-Plex-Version", env!("CARGO_PKG_VERSION"));
        put("X-Plex-Client-Identifier", &self.client_id);
        put("X-Plex-Platform", "Pley");
        put("X-Plex-Device", "Pley");
        put("X-Plex-Device-Name", "Pley");
        if let Some(token) = token {
            put("X-Plex-Token", token);
        }
        h.insert(ACCEPT, HeaderValue::from_static("application/json"));
        h
    }

    fn tv(&self, path: &str) -> Url {
        self.tv.join(path).unwrap_or_else(|_| self.tv.clone())
    }

    async fn send(&self, method: Method, url: Url, token: Option<&str>) -> PlexResult<Vec<u8>> {
        refuse_link_local(&url)?;
        let response = self
            .http
            .request(method, url)
            .headers(self.headers(token))
            .send()
            .await
            .map_err(|e| PlexError::Unreachable(describe(e)))?;
        read(response).await
    }

    async fn json<T: DeserializeOwned>(&self, method: Method, url: Url, token: Option<&str>) -> PlexResult<T> {
        let body = self.send(method, url, token).await?;
        serde_json::from_slice(&body)
            .map_err(|e| PlexError::Unreachable(format!("Plex's answer wasn't understood: {e}")))
    }

    // ---- Signing in ----

    /// Starts a sign-in: a PIN the person allows on Plex's own page.
    pub async fn create_pin(&self) -> PlexResult<Pin> {
        let mut url = self.tv("/api/v2/pins");
        url.query_pairs_mut().append_pair("strong", "true");
        self.json(Method::POST, url, None).await
    }

    /// The token of PIN `id` once the person has allowed Pley, `None` until
    /// then. An expired PIN is [`PlexError::Refused`] (404).
    pub async fn pin_token(&self, id: i64) -> PlexResult<Option<String>> {
        let pin: Pin = self.json(Method::GET, self.tv(&format!("/api/v2/pins/{id}")), None).await?;
        Ok(pin.auth_token.filter(|t| !t.is_empty()))
    }

    /// Plex's sign-in page for PIN `code`, which goes on to `forward` (a
    /// page of Pley's) once the person has allowed Pley.
    pub fn auth_url(&self, code: &str, forward: Option<&str>) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query.append_pair("clientID", &self.client_id);
        query.append_pair("code", code);
        query.append_pair("context[device][product]", "Pley");
        if let Some(forward) = forward {
            query.append_pair("forwardUrl", forward);
        }
        format!("{AUTH_PAGE}#?{}", query.finish())
    }

    // ---- The account ----

    /// The account's devices that are servers, with their tokens and
    /// addresses.
    pub async fn servers(&self, token: &str) -> PlexResult<Vec<Resource>> {
        let mut url = self.tv("/api/v2/resources");
        url.query_pairs_mut().append_pair("includeHttps", "1").append_pair("includeRelay", "1");
        let all: Vec<Resource> = self.json(Method::GET, url, Some(token)).await?;
        Ok(all.into_iter().filter(|r| r.provides.split(',').any(|p| p.trim() == "server")).collect())
    }

    /// The account's Plex Home: its users, the account itself among them.
    pub async fn home_users(&self, token: &str) -> PlexResult<Vec<HomeUser>> {
        let home: Home = self.json(Method::GET, self.tv("/api/v2/home/users"), Some(token)).await?;
        Ok(home.users)
    }

    /// A Home user's own token, switching to them as the Home's
    /// administrator: without a PIN first, [`PlexError::NeedsPin`] when Plex
    /// wants it.
    pub async fn switch(&self, token: &str, uuid: &str, pin: Option<&str>) -> PlexResult<String> {
        let mut url = self.tv(&format!("/api/v2/home/users/{}/switch", encode_segment(uuid)));
        if let Some(pin) = pin {
            url.query_pairs_mut().append_pair("pin", pin);
        }
        let response = self
            .http
            .post(url)
            .headers(self.headers(Some(token)))
            .send()
            .await
            .map_err(|e| PlexError::Unreachable(describe(e)))?;
        let status = response.status();
        let body = read_any(response).await?;
        if status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED {
            let code = serde_json::from_slice::<Errors>(&body).ok().and_then(|e| e.errors.first().map(|e| e.code));
            return Err(if code == Some(1041) { PlexError::NeedsPin } else { PlexError::Refused(status.as_u16()) });
        }
        if !status.is_success() {
            return Err(PlexError::Refused(status.as_u16()));
        }
        let user: Switched = serde_json::from_slice(&body)
            .map_err(|e| PlexError::Unreachable(format!("Plex's answer wasn't understood: {e}")))?;
        user.auth_token.filter(|t| !t.is_empty()).ok_or_else(|| PlexError::Unreachable("Plex gave no token".into()))
    }

    /// What each user the account shares with may see: plex.tv's older API,
    /// in XML, is the only place that has it.
    pub async fn user_filters(&self, token: &str) -> PlexResult<Vec<UserFilters>> {
        let body = self.send(Method::GET, self.tv("/api/users"), Some(token)).await?;
        parse_user_filters(&body)
    }

    // ---- A server ----

    /// Connects to `server`: its addresses in turn, those on the home
    /// network first and Plex's relay last, keeping the first that answers
    /// as that server. `token` is the server's own (its `accessToken`).
    pub async fn connect(&self, server: &Resource) -> PlexResult<Server> {
        let token = server.access_token.clone().unwrap_or_default();
        let mut addresses: Vec<&Connection> = server.connections.iter().collect();
        addresses.sort_by_key(|c| (c.relay, !c.local, c.ipv6));
        let mut tried = Vec::new();
        for connection in addresses {
            let mut candidates = vec![connection.uri.clone()];
            // Some routers block plex.direct names (DNS rebinding protection):
            // the plain address, when the server allows it, and only on the
            // home network, so the token never crosses the internet unencrypted.
            if !server.https_required && !connection.relay && connection.local {
                candidates.push(format!("http://{}:{}", connection.address, connection.port));
            }
            for uri in candidates {
                let Ok(base) = Url::parse(&uri) else { continue };
                let candidate = Server { base, token: token.clone(), machine_id: server.client_identifier.clone() };
                match self.identity(&candidate).await {
                    Ok(id) if id == server.client_identifier => return Ok(candidate),
                    Ok(_) => tried.push(format!("{uri}: another server")),
                    Err(e) => tried.push(format!("{uri}: {e}")),
                }
            }
        }
        Err(PlexError::Unreachable(if tried.is_empty() {
            "Plex lists no address for this server".into()
        } else {
            format!("none of the server's addresses answered ({})", tried.join("; "))
        }))
    }

    /// A server at an address the administrator gave, with their token.
    pub async fn server_at(&self, address: &str, token: &str) -> PlexResult<Server> {
        let base = Url::parse(address.trim()).map_err(|_| PlexError::Unreachable("that isn't an address".into()))?;
        if !matches!(base.scheme(), "http" | "https") || base.host().is_none() {
            return Err(PlexError::Unreachable("the address must start with http:// or https://".into()));
        }
        let mut server = Server { base, token: token.trim().to_owned(), machine_id: String::new() };
        server.machine_id = self.identity(&server).await?;
        // Without a token, /identity answers anyone: check the token too.
        self.server_json::<Container<Section>>(&server, "/library/sections", &[]).await?;
        Ok(server)
    }

    async fn identity(&self, server: &Server) -> PlexResult<String> {
        let id: Container<()> = self.server_json(server, "/identity", &[]).await?;
        id.media_container
            .machine_identifier
            .filter(|m| !m.is_empty())
            .ok_or_else(|| PlexError::Unreachable("it doesn't answer as a Plex server".into()))
    }

    /// A server's answer at `path` with `query`.
    pub async fn server_json<T: DeserializeOwned>(
        &self,
        server: &Server,
        path: &str,
        query: &[(&str, String)],
    ) -> PlexResult<T> {
        let mut url = server.base.join(path).map_err(|_| PlexError::Unreachable("bad path".into()))?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
        }
        self.json(Method::GET, url, Some(&server.token)).await
    }

    /// The server's libraries.
    pub async fn sections(&self, server: &Server) -> PlexResult<Vec<Section>> {
        let c: Container<Section> = self.server_json(server, "/library/sections", &[]).await?;
        Ok(c.media_container.directory)
    }

    /// One page of a listing: `start` onwards, with the total.
    pub async fn page(&self, server: &Server, path: &str, query: &[(&str, String)], start: usize) -> PlexResult<Page> {
        self.page_of(server, path, query, start, PAGE).await
    }

    /// `size` items of a listing from `start`, with the total.
    pub async fn page_of(
        &self,
        server: &Server,
        path: &str,
        query: &[(&str, String)],
        start: usize,
        size: usize,
    ) -> PlexResult<Page> {
        let mut query = query.to_vec();
        query.push(("X-Plex-Container-Start", start.to_string()));
        query.push(("X-Plex-Container-Size", size.to_string()));
        let c: Container<Item> = self.server_json(server, path, &query).await?;
        let mc = c.media_container;
        let total = mc.total_size.unwrap_or(start as i64 + mc.metadata.len() as i64).max(0) as usize;
        Ok(Page { items: mc.metadata, total })
    }

    /// Item `rating_key`'s own page: its locked fields and labels, which
    /// listings leave out.
    pub async fn metadata(&self, server: &Server, rating_key: &str) -> PlexResult<Option<Item>> {
        let path = format!("/library/metadata/{}", encode_segment(rating_key));
        let c: Container<()> = self.server_json(server, &path, &[]).await?;
        Ok(c.media_container.metadata.into_iter().next())
    }

    /// The labels used in library `section`.
    pub async fn labels(&self, server: &Server, section: &str) -> PlexResult<Vec<Tag>> {
        let c: Container<Tag> =
            self.server_json(server, &format!("/library/sections/{}/label", encode_segment(section)), &[]).await?;
        Ok(c.media_container.directory)
    }

    /// The server's devices, which name the plays in its history.
    pub async fn devices(&self, server: &Server) -> PlexResult<Vec<Device>> {
        let c: Container<()> = self.server_json(server, "/devices", &[]).await?;
        Ok(c.media_container.device)
    }
}

/// Refuses an address written as a link-local IP (names are checked as
/// they resolve, by [`NotLinkLocal`]).
fn refuse_link_local(url: &Url) -> PlexResult<()> {
    let ip = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.into(),
        Some(url::Host::Ipv6(ip)) => ip.into(),
        _ => return Ok(()),
    };
    if crate::requests::arr::link_local(ip) {
        return Err(PlexError::Unreachable("that address is link-local, where no Plex server is".into()));
    }
    Ok(())
}

/// Resolves names without their link-local addresses, failing when none
/// is left.
struct NotLinkLocal;

impl Resolve for NotLinkLocal {
    fn resolve(&self, name: Name) -> Resolving {
        let name = name.as_str().to_owned();
        Box::pin(async move {
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), 0))
                .await?
                .filter(|addr| !crate::requests::arr::link_local(addr.ip()))
                .collect();
            if addrs.is_empty() {
                return Err(format!("{name} has only link-local addresses, where no Plex server is").into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// A server reached at `base`, with a token for it.
#[derive(Clone)]
pub struct Server {
    pub base: Url,
    pub token: String,
    /// Its `machineIdentifier` (plex.tv's `clientIdentifier` for it).
    pub machine_id: String,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the token.
        f.debug_struct("Server").field("base", &self.base.as_str()).field("machine_id", &self.machine_id).finish()
    }
}

/// A page of a listing.
pub struct Page {
    pub items: Vec<Item>,
    pub total: usize,
}

// ---- What Plex answers ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    #[serde(deserialize_with = "num")]
    pub id: i64,
    pub code: String,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub name: String,
    #[serde(default)]
    pub product_version: Option<String>,
    pub client_identifier: String,
    #[serde(default)]
    pub provides: String,
    #[serde(default, deserialize_with = "flag")]
    pub owned: bool,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default, deserialize_with = "flag")]
    pub https_required: bool,
    #[serde(default)]
    pub connections: Vec<Connection>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub uri: String,
    #[serde(default)]
    pub address: String,
    #[serde(default, deserialize_with = "num")]
    pub port: i64,
    #[serde(default, deserialize_with = "flag")]
    pub local: bool,
    #[serde(default, deserialize_with = "flag")]
    pub relay: bool,
    #[serde(default, rename = "IPv6", deserialize_with = "flag")]
    pub ipv6: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeUser {
    #[serde(deserialize_with = "num")]
    pub id: i64,
    pub uuid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, deserialize_with = "flag")]
    pub admin: bool,
    /// Has a PIN.
    #[serde(default, deserialize_with = "flag")]
    pub protected: bool,
}

/// The account's Plex Home, as `/api/v2/home/users` answers.
#[derive(Deserialize)]
struct Home {
    #[serde(default)]
    users: Vec<HomeUser>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Switched {
    #[serde(default)]
    auth_token: Option<String>,
}

#[derive(Deserialize)]
struct Errors {
    #[serde(default)]
    errors: Vec<ErrorCode>,
}

#[derive(Deserialize)]
struct ErrorCode {
    #[serde(default, deserialize_with = "num")]
    code: i64,
}

/// What one user may see, as `/api/users` writes it (URL-encoded rules,
/// see [`super::restrictions`]).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UserFilters {
    pub id: i64,
    pub home: bool,
    pub movies: String,
    pub television: String,
    pub music: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Container<T> {
    pub media_container: MediaContainer<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaContainer<T> {
    #[serde(default)]
    pub machine_identifier: Option<String>,
    #[serde(default, deserialize_with = "opt_num")]
    pub total_size: Option<i64>,
    #[serde(default = "Vec::new", rename = "Directory")]
    pub directory: Vec<T>,
    #[serde(default = "Vec::new", rename = "Metadata")]
    pub metadata: Vec<Item>,
    #[serde(default, rename = "Device")]
    pub device: Vec<Device>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    #[serde(deserialize_with = "text")]
    pub key: String,
    pub title: String,
    /// movie, show, artist, photo.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, rename = "Location")]
    pub location: Vec<Location>,
}

#[derive(Debug, Deserialize)]
pub struct Location {
    pub path: String,
}

/// A label, as a library lists them.
#[derive(Debug, Deserialize)]
pub struct Tag {
    #[serde(deserialize_with = "text")]
    pub key: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    #[serde(deserialize_with = "num")]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub platform: Option<String>,
}

/// An item of a listing, or a play of the history: only what the import
/// reads.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Item {
    #[serde(deserialize_with = "text")]
    pub rating_key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    #[serde(deserialize_with = "opt_num")]
    pub index: Option<i64>,
    #[serde(deserialize_with = "opt_num")]
    pub parent_index: Option<i64>,
    #[serde(deserialize_with = "opt_text")]
    pub parent_rating_key: Option<String>,
    #[serde(deserialize_with = "opt_text")]
    pub grandparent_rating_key: Option<String>,
    pub grandparent_title: Option<String>,
    #[serde(deserialize_with = "opt_num")]
    pub added_at: Option<i64>,
    #[serde(deserialize_with = "opt_num")]
    pub view_count: Option<i64>,
    #[serde(deserialize_with = "opt_num")]
    pub view_offset: Option<i64>,
    #[serde(deserialize_with = "opt_num")]
    pub last_viewed_at: Option<i64>,
    #[serde(deserialize_with = "opt_float")]
    pub user_rating: Option<f64>,
    #[serde(deserialize_with = "opt_num")]
    pub last_rated_at: Option<i64>,
    #[serde(rename = "Guid")]
    pub guids: Vec<Guid>,
    /// The id of Plex's older agents, such as
    /// `com.plexapp.agents.imdb://tt0078748?lang=en`.
    #[serde(rename = "guid")]
    pub legacy_guid: Option<String>,
    #[serde(rename = "Media")]
    pub media: Vec<Media>,
    // An item's own page's (`/library/metadata/{id}`): its fields, which of
    // them are locked (edited by hand, or by a tool such as Kometa), and
    // its labels.
    pub title_sort: Option<String>,
    pub original_title: Option<String>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    #[serde(deserialize_with = "opt_num")]
    pub year: Option<i64>,
    pub originally_available_at: Option<String>,
    pub content_rating: Option<String>,
    pub studio: Option<String>,
    #[serde(rename = "Genre")]
    pub genres: Vec<Named>,
    #[serde(rename = "Field")]
    pub fields: Vec<Field>,
    #[serde(rename = "Label")]
    pub labels: Vec<Named>,
    // Collections' and playlists'.
    #[serde(deserialize_with = "flag")]
    pub smart: bool,
    /// 0 by release, 1 by title, 2 in its own order.
    #[serde(deserialize_with = "opt_num")]
    pub collection_sort: Option<i64>,
    /// `video`, `audio` or `photo`.
    pub playlist_type: Option<String>,
    // The history's.
    #[serde(rename = "accountID", deserialize_with = "opt_num")]
    pub account_id: Option<i64>,
    #[serde(rename = "deviceID", deserialize_with = "opt_num")]
    pub device_id: Option<i64>,
    #[serde(deserialize_with = "opt_num")]
    pub viewed_at: Option<i64>,
}

impl Item {
    /// Its files, with their sizes.
    pub fn files(&self) -> impl Iterator<Item = (&str, Option<i64>)> {
        self.media.iter().flat_map(|m| m.parts.iter()).map(|p| (p.file.as_str(), p.size))
    }

    /// The id Plex has for it at `source` ("tmdb", "imdb", "tvdb"), from
    /// its `Guid` tags or, in a library on Plex's older agents, its `guid`.
    pub fn guid(&self, source: &str) -> Option<&str> {
        let tagged = self.guids.iter().find_map(|g| g.id.strip_prefix(source)?.strip_prefix("://"));
        tagged.or_else(|| {
            let agent = match source {
                "tmdb" => "com.plexapp.agents.themoviedb://",
                "imdb" => "com.plexapp.agents.imdb://",
                "tvdb" => "com.plexapp.agents.thetvdb://",
                _ => return None,
            };
            // A show's id, before an episode's /season/episode and ?lang=.
            let id = self.legacy_guid.as_deref()?.strip_prefix(agent)?;
            let id = id.split(['?', '/']).next()?;
            (!id.is_empty()).then_some(id)
        })
    }
}

/// A genre or a label, as an item's page lists them.
#[derive(Debug, Default, Deserialize)]
pub struct Named {
    #[serde(default, deserialize_with = "text")]
    pub tag: String,
}

/// One of an item's fields, and whether it's locked.
#[derive(Debug, Default, Deserialize)]
pub struct Field {
    #[serde(default)]
    pub name: String,
    #[serde(default, deserialize_with = "flag")]
    pub locked: bool,
}

impl Item {
    /// Whether Plex's field `name` is locked.
    pub fn locked(&self, name: &str) -> bool {
        self.fields.iter().any(|f| f.locked && f.name == name)
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct Guid {
    #[serde(default)]
    pub id: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct Media {
    #[serde(default, rename = "Part")]
    pub parts: Vec<Part>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Part {
    #[serde(default)]
    pub file: String,
    #[serde(default, deserialize_with = "opt_num")]
    pub size: Option<i64>,
}

// ---- Reading answers ----

/// A 2xx answer's body, at most [`MAX_ANSWER`] bytes.
async fn read(response: reqwest::Response) -> PlexResult<Vec<u8>> {
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN || status == StatusCode::NOT_FOUND {
        return Err(PlexError::Refused(status.as_u16()));
    }
    if !status.is_success() {
        return Err(PlexError::Unreachable(format!("Plex answered {}", status.as_u16())));
    }
    read_any(response).await
}

/// Any answer's body, at most [`MAX_ANSWER`] bytes.
async fn read_any(mut response: reqwest::Response) -> PlexResult<Vec<u8>> {
    if response.content_length().is_some_and(|n| n > MAX_ANSWER as u64) {
        return Err(PlexError::Unreachable("Plex's answer is too large".into()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| PlexError::Unreachable(describe(e)))? {
        if body.len() + chunk.len() > MAX_ANSWER {
            return Err(PlexError::Unreachable("Plex's answer is too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// What went wrong with a request, without its address (which may hold
/// nothing secret, but is long and says little).
fn describe(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "it took too long to answer".into()
    } else if e.is_connect() {
        "it couldn't be reached".into()
    } else {
        e.without_url().to_string()
    }
}

/// `text` as one segment of a path.
fn encode_segment(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>().replace('+', "%20")
}

/// The users of `/api/users` with their filters. Attributes are read as
/// they are; anything else in the answer is ignored.
pub fn parse_user_filters(xml: &[u8]) -> PlexResult<Vec<UserFilters>> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    let mut users = Vec::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) if e.name().as_ref() == b"User" => {
                let mut user = UserFilters::default();
                for attr in e.attributes().flatten() {
                    let value = attr
                        .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                        .map(|v| v.into_owned())
                        .unwrap_or_default();
                    match attr.key.as_ref() {
                        b"id" => user.id = value.parse().unwrap_or(0),
                        b"home" => user.home = value == "1" || value == "true",
                        b"filterMovies" => user.movies = value,
                        b"filterTelevision" => user.television = value,
                        b"filterMusic" => user.music = value,
                        _ => {}
                    }
                }
                if user.id != 0 {
                    users.push(user);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(PlexError::Unreachable(format!("Plex's answer wasn't understood: {e}"))),
            _ => {}
        }
        buf.clear();
    }
    Ok(users)
}

// Plex writes some numbers as strings ("smart": "1"), and some flags as 0
// and 1: these read either.

#[derive(Deserialize)]
#[serde(untagged)]
enum Loose {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
}

impl Loose {
    fn int(self) -> Option<i64> {
        match self {
            Loose::Int(n) => Some(n),
            Loose::Float(f) => Some(f as i64),
            Loose::Bool(b) => Some(b as i64),
            Loose::Text(s) => s.trim().parse().ok(),
        }
    }

    fn float(self) -> Option<f64> {
        match self {
            Loose::Int(n) => Some(n as f64),
            Loose::Float(f) => Some(f),
            Loose::Bool(_) => None,
            Loose::Text(s) => s.trim().parse().ok(),
        }
    }

    fn text(self) -> String {
        match self {
            Loose::Int(n) => n.to_string(),
            Loose::Float(f) => f.to_string(),
            Loose::Bool(b) => b.to_string(),
            Loose::Text(s) => s,
        }
    }
}

fn num<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Loose::deserialize(d)?.int().ok_or_else(|| de::Error::custom("not a number"))
}

fn opt_num<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Ok(Option::<Loose>::deserialize(d)?.and_then(Loose::int))
}

fn opt_float<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    Ok(Option::<Loose>::deserialize(d)?.and_then(Loose::float))
}

fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Option::<Loose>::deserialize(d)? {
        Some(Loose::Bool(b)) => b,
        Some(Loose::Text(s)) => matches!(s.as_str(), "1" | "true"),
        Some(other) => other.int().is_some_and(|n| n != 0),
        None => false,
    })
}

fn text<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Loose::deserialize(d)?.text())
}

fn opt_text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<Loose>::deserialize(d)?.map(Loose::text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbers_and_flags_written_either_way() {
        let r: Resource = serde_json::from_str(
            r#"{"name":"Den","clientIdentifier":"abc","provides":"server,player","owned":"1",
                "httpsRequired":0,"connections":[{"uri":"https://x","address":"10.0.0.2","port":"32400","local":true}]}"#,
        )
        .unwrap();
        assert!(r.owned && !r.https_required);
        assert_eq!(r.connections[0].port, 32400);
        let i: Item = serde_json::from_str(
            r#"{"ratingKey":12,"type":"movie","title":"A","userRating":"7.5","viewCount":"2",
                "Guid":[{"id":"tmdb://348"},{"id":"imdb://tt0078748"}],
                "Media":[{"Part":[{"file":"/m/A.mkv","size":10}]},{"Part":[{"file":"/m/A 4K.mkv","size":"20"}]}]}"#,
        )
        .unwrap();
        assert_eq!((i.rating_key.as_str(), i.user_rating, i.view_count), ("12", Some(7.5), Some(2)));
        assert_eq!((i.guid("tmdb"), i.guid("imdb"), i.guid("tvdb")), (Some("348"), Some("tt0078748"), None));
        let old: Item =
            serde_json::from_str(r#"{"ratingKey":"5","guid":"com.plexapp.agents.thetvdb://121361/1/2?lang=en"}"#)
                .unwrap();
        assert_eq!((old.guid("tvdb"), old.guid("imdb")), (Some("121361"), None));
        assert_eq!(i.files().collect::<Vec<_>>(), [("/m/A.mkv", Some(10)), ("/m/A 4K.mkv", Some(20))]);
    }

    #[test]
    fn reads_users_filters_from_plex_tv_xml() {
        let xml = br#"<?xml version="1.0"?><MediaContainer>
            <User id="885" title="Kid" home="1" filterMovies="contentRating=03%2C04|label=Override%3A%20Kids&amp;label!=Override%3A%20Adult" filterTelevision="" filterMusic="">
              <Server id="1" name="Den"/></User>
            <User id="15" title="Friend" home="0" filterMovies="" filterTelevision="label!=Scary" filterMusic=""/>
          </MediaContainer>"#;
        let users = parse_user_filters(xml).unwrap();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].id, 885);
        assert!(users[0].home && !users[1].home);
        assert_eq!(users[0].movies, "contentRating=03%2C04|label=Override%3A%20Kids&label!=Override%3A%20Adult");
        assert_eq!(users[1].television, "label!=Scary");
    }

    #[test]
    fn link_local_addresses_are_refused() {
        for address in ["http://169.254.169.254/identity", "http://[fe80::1]:32400/"] {
            assert!(refuse_link_local(&Url::parse(address).unwrap()).is_err(), "{address}");
        }
        assert!(refuse_link_local(&Url::parse("http://192.168.1.2:32400/").unwrap()).is_ok());
    }

    #[test]
    fn the_sign_in_page_carries_the_pin_and_where_to_go_after() {
        let plex = Plex::new(PLEX_TV, "pley-1");
        let url = plex.auth_url("ABCD", Some("https://pley.example/setup/plex/done"));
        assert!(url.starts_with("https://app.plex.tv/auth#?clientID=pley-1&code=ABCD&"));
        assert!(url.contains("context%5Bdevice%5D%5Bproduct%5D=Pley"));
        assert!(url.ends_with("forwardUrl=https%3A%2F%2Fpley.example%2Fsetup%2Fplex%2Fdone"));
    }
}
