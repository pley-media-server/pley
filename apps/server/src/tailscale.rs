//! Signing in with Tailscale: who a request comes from, as the Tailscale
//! service on this computer (tailscaled) knows them.
//!
//! A tailnet's traffic is WireGuard's: a packet from a device's Tailscale
//! address comes from that device. A device on the local network can't
//! open a connection from such an address either, since the server's
//! answers go to the tailnet, never to it. Tailscale's LocalAPI
//! (`/localapi/v0/whois`) names the device at the other end of a connection
//! and the person signed in on it. Pley asks about the connection itself,
//! never about an address a header names: clients choose those
//! ([`crate::auth::forwarded_for`]).
//!
//! That holds where Tailscale has a network adapter of its own. Without one
//! (its userspace mode, Docker's by default), it hands the tailnet's
//! connections over from this computer's own address, each told apart by
//! its port, and nothing keeps a device on the local network from sending
//! from a Tailscale address: then only connections from this computer are
//! asked about, and with one, only those from Tailscale's addresses. What
//! Tailscale says of a connection from this computer holds for that
//! connection alone (another takes its port once it closes), so it is
//! asked each time.
//!
//! Some connections are no one: a tagged device (a server, or a TV an
//! administrator tagged), a device that routes others' traffic (a subnet
//! router or an exit node: everyone behind it comes from its address), this
//! server's own device (any program on this computer can connect from its
//! address), and a connection Tailscale doesn't know (a proxy on this
//! computer, Tailscale Serve, the local network). People are known by
//! Tailscale's id for them in the tailnet: ids are a tailnet's own (a
//! Headscale server's start at 1), and a login name can change hands.
//!
//! Nothing asks Tailscale until an administrator turns signing in with it on.
//! On Windows, a program that connects to Tailscale's service as another
//! account than the one Tailscale runs for can make it switch to that
//! account's profile, so Pley's Windows service never asks it.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode, header, uri::Authority};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use serde::Deserialize;

/// How long what Tailscale said of a Tailscale address is kept: a device or
/// person removed from the tailnet is no one here within this long.
const PERSON_TTL: Duration = Duration::from_secs(30);
/// How long what Tailscale said of this server's own device is kept.
const NODE_TTL: Duration = Duration::from_secs(60);
/// How many addresses' answers are kept at most.
const PEOPLE_MAX: usize = 4096;
/// How long Tailscale's service gets to answer, waiting for a turn included.
const TIMEOUT: Duration = Duration::from_secs(3);
/// How many questions to Tailscale's service are asked at once at most: the
/// sign-in page asks it for whoever opens it.
const ASKING_MAX: usize = 8;
/// The most of an answer read: a device's description, with its
/// capabilities, takes a few KiB.
const ANSWER_MAX: usize = 1024 * 1024;
/// The longest login name or display name kept, in characters.
const NAME_MAX: usize = 256;
/// The `Host` Tailscale's LocalAPI takes.
const LOCALAPI_HOST: &str = "local-tailscaled.sock";

/// Where Tailscale's service answers when nothing else is chosen.
#[cfg(windows)]
pub const DEFAULT_SOCKET: &str = r"\\.\pipe\ProtectedPrefix\Administrators\Tailscale\tailscaled";
/// Where Tailscale's service answers when nothing else is chosen: the
/// open-source tailscaled's socket (Tailscale's Mac apps answer only with a
/// token of their own).
#[cfg(target_os = "macos")]
pub const DEFAULT_SOCKET: &str = "/var/run/tailscaled.socket";
/// Where Tailscale's service answers when nothing else is chosen.
#[cfg(all(unix, not(target_os = "macos")))]
pub const DEFAULT_SOCKET: &str = "/var/run/tailscale/tailscaled.sock";

/// Why the Windows service never asks Tailscale (see the module's doc).
pub const NOT_AS_A_SERVICE: &str = "Pley runs as a Windows service, which can't ask Tailscale: run Pley from the tray as the person Tailscale runs for";

/// A person signed in to Tailscale on the device at the other end of a
/// connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    /// Who they are, which accounts are linked to: Tailscale's id for them
    /// and the tailnet's (`42@T1234CNTRL`).
    pub id: String,
    /// Their login name (`alice@example.com`, `alice@github`), to show.
    pub login: String,
    /// Their name, to show.
    pub name: String,
}

/// This server's own device on the tailnet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    stable_id: String,
    /// The tailnet's stable id (its name, from a Tailscale that says none).
    tailnet: String,
    /// Tailscale has a network adapter of its own (see the module's doc).
    adapter: bool,
    /// Its MagicDNS name, without the final dot (`media.example.ts.net`);
    /// empty without MagicDNS.
    pub name: String,
    addresses: Vec<IpAddr>,
}

impl Node {
    /// Whether `host`, a request's `Host`, names this device: its MagicDNS
    /// name or one of its Tailscale addresses, with any port. Not a short
    /// name (`media`), which a device on the local network may answer for.
    pub fn named_by(&self, host: &str) -> bool {
        let Ok(authority) = host.trim().parse::<Authority>() else { return false };
        let host = authority.host();
        let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
        if let Ok(ip) = bare.parse::<IpAddr>() {
            return self.addresses.contains(&ip.to_canonical());
        }
        !self.name.is_empty() && host.trim_end_matches('.').eq_ignore_ascii_case(&self.name)
    }
}

/// Tailscale's service on this computer.
pub struct Tailscale {
    /// Where its LocalAPI answers, or why Pley never asks it.
    socket: Result<PathBuf, String>,
    /// What it said of each Tailscale address, and when.
    people: Mutex<HashMap<IpAddr, (Instant, Option<Person>)>>,
    /// What it said of this server's own device, and when.
    node: Mutex<Option<(Instant, Node)>>,
    /// Turns to ask it ([`ASKING_MAX`]).
    asking: tokio::sync::Semaphore,
}

impl Tailscale {
    pub fn new(socket: Result<PathBuf, String>) -> Self {
        Self {
            socket,
            people: Mutex::default(),
            node: Mutex::default(),
            asking: tokio::sync::Semaphore::new(ASKING_MAX),
        }
    }

    /// The person on the device at `peer`, the other end of a request's
    /// connection (never an address a header names): `None` for no one (see
    /// the module's doc), `Err` when Tailscale can't be asked.
    pub async fn person(&self, peer: SocketAddr) -> Result<Option<Person>, String> {
        let peer = SocketAddr::new(peer.ip().to_canonical(), peer.port());
        let loopback = peer.ip().is_loopback();
        if !loopback && !is_tailscale_address(peer.ip()) {
            return Ok(None);
        }
        let now = Instant::now();
        if !loopback
            && let Some((at, person)) = self.people.lock().get(&peer.ip())
            && now.duration_since(*at) < PERSON_TTL
        {
            return Ok(person.clone());
        }
        let node = self.node().await?;
        // With an adapter, the tailnet's connections come from its
        // addresses; without one, from this computer.
        if loopback == node.adapter {
            return Ok(None);
        }
        let person = self.whois(peer).await?.and_then(|whois| whois.person(&node));
        if !loopback {
            let mut people = self.people.lock();
            if people.len() >= PEOPLE_MAX {
                people.retain(|_, (at, _)| now.duration_since(*at) < PERSON_TTL);
                if people.len() >= PEOPLE_MAX {
                    people.clear();
                }
            }
            people.insert(peer.ip(), (now, person.clone()));
        }
        Ok(person)
    }

    /// This server's own device, while Tailscale is connected.
    pub async fn node(&self) -> Result<Node, String> {
        if let Some((at, node)) = self.node.lock().as_ref()
            && at.elapsed() < NODE_TTL
        {
            return Ok(node.clone());
        }
        let (status, body) = self.get("/localapi/v0/status?peers=false").await?;
        if status != StatusCode::OK {
            return Err(refused(status, &body));
        }
        let node = parse_status(&body)?;
        *self.node.lock() = Some((Instant::now(), node.clone()));
        Ok(node)
    }

    async fn whois(&self, peer: SocketAddr) -> Result<Option<WhoIs>, String> {
        let addr: String = url::form_urlencoded::byte_serialize(peer.to_string().as_bytes()).collect();
        // TCP's: without it, a port of this computer's that a UDP flow of
        // the tailnet holds would answer too.
        let (status, body) = self.get(&format!("/localapi/v0/whois?addr={addr}&proto=tcp")).await?;
        match status {
            StatusCode::OK => serde_json::from_slice(&body)
                .map(Some)
                .map_err(|err| format!("Tailscale's answer about a connection didn't read: {err}")),
            // A connection Tailscale didn't carry.
            StatusCode::NOT_FOUND => Ok(None),
            _ => Err(refused(status, &body)),
        }
    }

    async fn get(&self, path: &str) -> Result<(StatusCode, Bytes), String> {
        let socket = self.socket.as_ref().map_err(Clone::clone)?;
        tokio::time::timeout(TIMEOUT, async {
            let _turn = self.asking.acquire().await.map_err(|_| "Tailscale is no longer asked".to_owned())?;
            ask(socket, path).await
        })
        .await
        .map_err(|_| "Tailscale didn't answer within 3 seconds".to_owned())?
    }
}

/// Removes `user_id`'s link to a Tailscale user, and signs out the sessions
/// signed in with it and the profiles switched to from them, but the one
/// whose token hashes to `keep`. Once this commits, the caller has cached
/// sessions forgotten ([`crate::auth::AuthService::forget_sessions`]).
pub async fn remove_link_within(
    conn: &mut sqlx::SqliteConnection,
    user_id: i64,
    keep: Option<&str>,
) -> sqlx::Result<()> {
    let removed: Option<String> =
        sqlx::query_scalar("DELETE FROM tailscale_links WHERE user_id = ? RETURNING tailscale_user")
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?;
    if let Some(person) = removed {
        sqlx::query("DELETE FROM sessions WHERE tailscale_user = ? AND token_hash != ?")
            .bind(person)
            .bind(keep.unwrap_or_default())
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Whether `ip` is one of Tailscale's addresses: 100.64.0.0/10, and
/// fd7a:115c:a1e0::/48.
fn is_tailscale_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => crate::setup::is_shared_address(v4),
        IpAddr::V6(v6) => v6.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
    }
}

/// What Tailscale's service said when it refused, said back without its
/// answer's whole body.
fn refused(status: StatusCode, body: &[u8]) -> String {
    let said: String = String::from_utf8_lossy(body).trim().chars().take(200).collect();
    format!("Tailscale refused ({status}): {said}")
}

/// Asks Tailscale's LocalAPI for `path` at `socket`.
async fn ask(socket: &Path, path: &str) -> Result<(StatusCode, Bytes), String> {
    let stream = connect(socket)
        .await
        .map_err(|err| format!("Tailscale's service can't be reached at {}: {err}", socket.display()))?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .map_err(|err| format!("Tailscale's service didn't answer: {err}"))?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let request = Request::get(path)
        .header(header::HOST, LOCALAPI_HOST)
        .body(Body::empty())
        .map_err(|err| format!("a request to Tailscale didn't form: {err}"))?;
    let response =
        sender.send_request(request).await.map_err(|err| format!("Tailscale's service didn't answer: {err}"))?;
    let status = response.status();
    let body = axum::body::to_bytes(Body::new(response.into_body()), ANSWER_MAX)
        .await
        .map_err(|err| format!("Tailscale's answer didn't read: {err}"))?;
    Ok((status, body))
}

#[cfg(unix)]
async fn connect(socket: &Path) -> std::io::Result<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(socket).await
}

#[cfg(windows)]
async fn connect(pipe: &Path) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    /// Every instance of the pipe is busy with another client.
    const ERROR_PIPE_BUSY: i32 = 231;
    let mut tries = 0;
    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(pipe) {
            Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < 25 => {
                tries += 1;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            other => return other,
        }
    }
}

/// `/localapi/v0/status`, as far as Pley reads it.
#[derive(Deserialize)]
struct Status {
    #[serde(rename = "BackendState", default)]
    backend_state: String,
    #[serde(rename = "TUN", default)]
    tun: bool,
    #[serde(rename = "Self", default)]
    own: Option<OwnStatus>,
    #[serde(rename = "CurrentTailnet", default)]
    tailnet: Option<TailnetStatus>,
}

#[derive(Deserialize)]
struct TailnetStatus {
    #[serde(rename = "StableID", default)]
    stable_id: String,
    #[serde(rename = "Name", default)]
    name: String,
}

#[derive(Deserialize)]
struct OwnStatus {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(rename = "DNSName", default)]
    dns_name: String,
    #[serde(rename = "TailscaleIPs", default)]
    addresses: Vec<IpAddr>,
}

fn parse_status(body: &[u8]) -> Result<Node, String> {
    let status: Status =
        serde_json::from_slice(body).map_err(|err| format!("Tailscale's status didn't read: {err}"))?;
    if status.backend_state != "Running" {
        let state = if status.backend_state.is_empty() { "unknown" } else { status.backend_state.as_str() };
        return Err(format!("Tailscale isn't connected on this computer ({state})"));
    }
    let own = status.own.filter(|own| !own.id.is_empty()).ok_or("Tailscale didn't say which device this is")?;
    let tailnet = status
        .tailnet
        .map(|t| if t.stable_id.is_empty() { t.name } else { t.stable_id })
        .filter(|t| !t.is_empty())
        .ok_or("Tailscale didn't say which tailnet this is")?;
    Ok(Node {
        stable_id: own.id,
        tailnet,
        adapter: status.tun,
        name: own.dns_name.trim_end_matches('.').to_ascii_lowercase(),
        addresses: own.addresses.into_iter().map(|ip| ip.to_canonical()).collect(),
    })
}

/// `/localapi/v0/whois`, as far as Pley reads it.
#[derive(Deserialize)]
struct WhoIs {
    #[serde(rename = "Node")]
    node: WhoIsNode,
    #[serde(rename = "UserProfile")]
    user: WhoIsUser,
}

#[derive(Deserialize)]
struct WhoIsNode {
    #[serde(rename = "StableID", default)]
    stable_id: String,
    #[serde(rename = "Tags", default)]
    tags: Vec<String>,
    /// The device's own addresses, as prefixes (`100.64.0.9/32`).
    #[serde(rename = "Addresses", default)]
    addresses: Vec<String>,
    /// What the tailnet routes to it: its addresses, and a subnet router's
    /// or exit node's routes. Left out, its addresses.
    #[serde(rename = "AllowedIPs", default)]
    allowed_ips: Vec<String>,
    #[serde(rename = "PrimaryRoutes", default)]
    primary_routes: Vec<String>,
}

#[derive(Deserialize)]
struct WhoIsUser {
    #[serde(rename = "ID", default)]
    id: i64,
    #[serde(rename = "LoginName", default)]
    login: String,
    #[serde(rename = "DisplayName", default)]
    name: String,
}

impl WhoIs {
    /// The person, unless the device is tagged, routes others' traffic, or
    /// is this server's own.
    fn person(self, own: &Node) -> Option<Person> {
        if !self.node.tags.is_empty() || self.node.stable_id.is_empty() || self.node.stable_id == own.stable_id {
            return None;
        }
        let routes = !self.node.primary_routes.is_empty()
            || self.node.allowed_ips.iter().any(|prefix| !self.node.addresses.contains(prefix));
        if routes {
            return None;
        }
        if self.user.id <= 0 || self.user.login.is_empty() {
            return None;
        }
        let bounded = |s: &str| s.trim().chars().take(NAME_MAX).collect::<String>();
        let login = bounded(&self.user.login);
        let name = match bounded(&self.user.name) {
            name if name.is_empty() => login.clone(),
            name => name,
        };
        Some(Person { id: format!("{}@{}", self.user.id, own.tailnet), login, name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> Node {
        Node {
            stable_id: "nSERVER".into(),
            tailnet: "T1234CNTRL".into(),
            adapter: true,
            name: "media.example.ts.net".into(),
            addresses: vec!["100.101.102.103".parse().unwrap(), "fd7a:115c:a1e0::1".parse().unwrap()],
        }
    }

    fn whois(stable_id: &str, tags: &[&str], user: i64, login: &str) -> WhoIs {
        WhoIs {
            node: WhoIsNode {
                stable_id: stable_id.into(),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                addresses: vec!["100.64.0.9/32".into()],
                allowed_ips: vec!["100.64.0.9/32".into()],
                primary_routes: Vec::new(),
            },
            user: WhoIsUser { id: user, login: login.into(), name: "Alice".into() },
        }
    }

    #[test]
    fn only_the_servers_own_names_count() {
        let node = node();
        for host in [
            "media.example.ts.net",
            "media.example.ts.net:7539",
            "MEDIA.example.ts.net.:7539",
            "100.101.102.103",
            "100.101.102.103:7539",
            "[fd7a:115c:a1e0::1]:7539",
        ] {
            assert!(node.named_by(host), "{host}");
        }
        for host in [
            "media",
            "media:7539",
            "evil.example.com:7539",
            "media.example.ts.net.evil.example.com",
            "100.101.102.104:7539",
            "127.0.0.1:7539",
            "",
            "media.example.ts.net/x",
        ] {
            assert!(!node.named_by(host), "{host}");
        }
        let without_magic_dns = Node { name: String::new(), ..node };
        assert!(!without_magic_dns.named_by(""));
        assert!(without_magic_dns.named_by("100.101.102.103:7539"));
    }

    #[test]
    fn tagged_devices_routers_and_the_server_itself_are_no_one() {
        let own = node();
        let alice = Person { id: "7@T1234CNTRL".into(), login: "alice@example.com".into(), name: "Alice".into() };
        assert_eq!(whois("nLAPTOP", &[], 7, "alice@example.com").person(&own), Some(alice));
        let mut left_out = whois("nLAPTOP", &[], 7, "alice@example.com");
        left_out.node.allowed_ips.clear();
        assert!(left_out.person(&own).is_some(), "no AllowedIPs: its addresses");
        let mut router = whois("nROUTER", &[], 7, "alice@example.com");
        router.node.allowed_ips.push("192.168.1.0/24".into());
        assert_eq!(router.person(&own), None, "a subnet router");
        let mut exit = whois("nEXIT", &[], 7, "alice@example.com");
        exit.node.primary_routes.push("0.0.0.0/0".into());
        assert_eq!(exit.person(&own), None, "an exit node");
        assert_eq!(whois("nTV", &["tag:tv"], 7, "alice@example.com").person(&own), None);
        assert_eq!(whois("nSERVER", &[], 7, "alice@example.com").person(&own), None);
        assert_eq!(whois("", &[], 7, "alice@example.com").person(&own), None);
        assert_eq!(whois("nLAPTOP", &[], 0, "alice@example.com").person(&own), None);
        assert_eq!(whois("nLAPTOP", &[], 7, "").person(&own), None);
        let mut long = whois("nLAPTOP", &[], 7, &"a".repeat(1000));
        long.user.name = String::new();
        let person = long.person(&own).unwrap();
        assert_eq!(person.login.chars().count(), NAME_MAX);
        assert_eq!(person.name, person.login, "no name: the login");
    }

    #[test]
    fn tailscale_addresses() {
        for ip in ["100.64.0.1", "100.127.255.254", "fd7a:115c:a1e0:ab12::1"] {
            assert!(is_tailscale_address(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["127.0.0.1", "::1", "192.168.1.20", "10.0.0.1", "100.128.0.1", "8.8.8.8", "fd7a:115c:a1e1::1"] {
            assert!(!is_tailscale_address(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn status_reads_only_a_connected_tailscale() {
        let running =
            br#"{"BackendState":"Running","TUN":true,"Self":{"ID":"nSERVER","DNSName":"Media.example.ts.net.",
            "TailscaleIPs":["100.101.102.103","fd7a:115c:a1e0::1"]},
            "CurrentTailnet":{"Name":"example.com","StableID":"T1234CNTRL"}}"#;
        assert_eq!(parse_status(running).unwrap(), node());
        let older = br#"{"BackendState":"Running","Self":{"ID":"nSERVER"},"CurrentTailnet":{"Name":"example.com"}}"#;
        let older = parse_status(older).unwrap();
        assert_eq!((older.tailnet.as_str(), older.adapter), ("example.com", false));
        let stopped = br#"{"BackendState":"Stopped","Self":{"ID":"nSERVER"}}"#;
        assert!(parse_status(stopped).unwrap_err().contains("Stopped"));
        assert!(parse_status(br#"{"BackendState":"Running"}"#).is_err());
        assert!(parse_status(br#"{"BackendState":"Running","Self":{"ID":"nSERVER"}}"#).is_err(), "no tailnet");
        assert!(parse_status(b"not json").is_err());
    }

    #[test]
    fn whois_reads_tailscales_answer() {
        let body =
            br#"{"Node":{"ID":5,"StableID":"nLAPTOP","Name":"laptop.example.ts.net.","Addresses":["100.64.0.9/32"],
            "AllowedIPs":["100.64.0.9/32"]},
            "UserProfile":{"ID":42,"LoginName":"alice@github","DisplayName":"Alice","ProfilePicURL":""},"CapMap":{}}"#;
        let whois: WhoIs = serde_json::from_slice(body).unwrap();
        assert_eq!(
            whois.person(&node()),
            Some(Person { id: "42@T1234CNTRL".into(), login: "alice@github".into(), name: "Alice".into() })
        );
    }

    #[tokio::test]
    async fn unconfigured_tailscale_is_never_asked() {
        let tailscale = Tailscale::new(Err(NOT_AS_A_SERVICE.into()));
        assert_eq!(tailscale.person("192.168.1.20:5000".parse().unwrap()).await, Ok(None), "not a tailnet address");
        assert_eq!(tailscale.person("127.0.0.1:5000".parse().unwrap()).await, Err(NOT_AS_A_SERVICE.to_owned()));
        assert_eq!(tailscale.person("100.64.0.9:5000".parse().unwrap()).await, Err(NOT_AS_A_SERVICE.to_owned()));
    }
}
