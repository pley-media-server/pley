//! Jellyfin's network discovery (docs/COMPATIBILITY.md): Jellyfin's apps
//! broadcast `who is JellyfinServer?` to UDP port 7359 and list the servers
//! that answer. While the Jellyfin-compatible API is on, and discovery is
//! (Settings → Network), Pley answers there, on the addresses the API
//! listens on, as Jellyfin does: the API's address on the network the
//! question came from, the server's id as the API gives it, and its name.
//!
//! Anyone on the network could send the question in someone else's name, so
//! the port mustn't make their traffic bigger: only the question itself gets
//! an answer, only from addresses on the local network, at most 10 in 10
//! seconds to one address and 100 a second in all, and the answer stays
//! small: the name is cut to 64 characters and 128 bytes.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use crate::network::Listen;
use crate::state::AppState;

/// Jellyfin's discovery port.
pub const PORT: u16 = 7359;
/// What Jellyfin's apps send, in any case (one app writes `Who`).
pub const QUESTION: &str = "who is JellyfinServer?";
/// Answers a second, in all.
const MAX_ANSWERS: u32 = 100;
/// Answers to one address within [`SOURCE_WINDOW`]: apps ask once or twice,
/// and again when they look again.
const PER_SOURCE: u32 = 10;
const SOURCE_WINDOW: Duration = Duration::from_secs(10);
/// Addresses whose answers are counted at once; past it, a new address waits.
const MAX_SOURCES: usize = 1024;
/// The longest server name an answer carries, in characters and in bytes.
const MAX_NAME_CHARS: usize = 64;
const MAX_NAME_BYTES: usize = 128;

/// Jellyfin's `ServerDiscoveryInfo`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Answer<'a> {
    /// The API's address: `http://192.168.1.10:8096`.
    pub address: String,
    pub id: &'a str,
    pub name: &'a str,
    /// Jellyfin leaves it `null`; an app that reads it takes its host
    /// instead of the address's.
    pub endpoint_address: Option<&'a str>,
}

/// Whether a datagram is the question: that text alone, in any case, with
/// whitespace around it at most.
pub fn is_question(datagram: &[u8]) -> bool {
    datagram.trim_ascii().eq_ignore_ascii_case(QUESTION.as_bytes())
}

/// How many answers went out lately: to each address, and in all.
#[derive(Default)]
struct Limits {
    second: Option<(Instant, u32)>,
    sources: HashMap<IpAddr, (Instant, u32)>,
}

impl Limits {
    /// Whether `from` may have another answer now; it counts if so.
    fn allow(&mut self, from: IpAddr, now: Instant) -> bool {
        let second = match self.second {
            Some((start, count)) if now.duration_since(start) < Duration::from_secs(1) => (start, count),
            _ => (now, 0),
        };
        if second.1 >= MAX_ANSWERS {
            return false;
        }
        if self.sources.len() >= MAX_SOURCES && !self.sources.contains_key(&from) {
            self.sources.retain(|_, (start, _)| now.duration_since(*start) < SOURCE_WINDOW);
            if self.sources.len() >= MAX_SOURCES {
                return false;
            }
        }
        let source = self.sources.entry(from).or_insert((now, 0));
        if now.duration_since(source.0) >= SOURCE_WINDOW {
            *source = (now, 0);
        }
        if source.1 >= PER_SOURCE {
            return false;
        }
        source.1 += 1;
        self.second = Some((second.0, second.1 + 1));
        true
    }
}

/// The API's port on `local`, an address of this computer: that of an
/// address it listens on that is `local`, or that covers it (`0.0.0.0`,
/// and `[::]`, which takes IPv4 too where the system lets it).
pub fn api_port(listen: &Listen, local: IpAddr) -> Option<u16> {
    let addresses = if listen.jellyfin_on_web { &listen.web } else { &listen.jellyfin };
    let local = local.to_canonical();
    let exact = addresses.iter().find(|addr| addr.ip().to_canonical() == local);
    let covering = || {
        addresses
            .iter()
            .filter(|addr| addr.ip().is_unspecified() && (addr.is_ipv6() || local.is_ipv4()))
            .min_by_key(|addr| addr.is_ipv6() != local.is_ipv6())
    };
    exact.or_else(covering).map(SocketAddr::port).filter(|&port| port != 0)
}

/// The address of this computer that answers `peer`: the socket's own, or,
/// on an address that covers several, the one the system sends to `peer`
/// from, which is on the network the question came from.
async fn local_towards(bound: IpAddr, peer: SocketAddr) -> Option<IpAddr> {
    if !bound.is_unspecified() {
        return Some(bound.to_canonical());
    }
    let any = if peer.is_ipv4() {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))
    } else {
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))
    };
    // Connecting a UDP socket sends nothing: it only picks the route.
    let socket = UdpSocket::bind(any).await.ok()?;
    socket.connect(peer).await.ok()?;
    Some(socket.local_addr().ok()?.ip().to_canonical())
}

/// The server's name as answers carry it: cut to [`MAX_NAME_CHARS`]
/// characters and [`MAX_NAME_BYTES`] bytes.
fn short_name(name: &str) -> String {
    let mut short = String::new();
    for c in name.chars().take(MAX_NAME_CHARS) {
        if short.len() + c.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        short.push(c);
    }
    short
}

/// `http://` and the address with its port, IPv6 in brackets.
fn url(ip: IpAddr, port: u16) -> String {
    format!("http://{}", SocketAddr::new(ip, port))
}

/// Answers Jellyfin's question on `socket` until `stopping` is cancelled.
pub async fn serve(state: AppState, socket: UdpSocket, stopping: CancellationToken) {
    let Ok(bound) = socket.local_addr() else { return };
    // As `/System/Info/Public` gives it.
    let id: String = state.identity.id.chars().take(32).collect();
    let mut buf = [0u8; 64];
    let mut limits = Limits::default();
    loop {
        let (len, from) = tokio::select! {
            received = socket.recv_from(&mut buf) => match received {
                Ok(received) => received,
                // Windows reports an earlier answer's ICMP "port unreachable"
                // here, and a datagram longer than the buffer, which isn't
                // the question.
                Err(_) => continue,
            },
            () = stopping.cancelled() => return,
        };
        let source = from.ip().to_canonical();
        if !is_question(&buf[..len]) || !crate::setup::is_local_address(source) {
            continue;
        }
        if !limits.allow(source, Instant::now()) {
            continue;
        }
        let Some(local) = local_towards(bound.ip(), from).await else { continue };
        // Where the API is now: it may have moved since this socket opened.
        let Some(port) = api_port(&state.network.wanted(), local) else { continue };
        let settings = state.settings.get();
        let name = short_name(&settings.server_name);
        let answer = Answer { address: url(local, port), id: &id, name: &name, endpoint_address: None };
        if let Ok(bytes) = serde_json::to_vec(&answer) {
            let _ = socket.send_to(&bytes, from).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_question_is_answered() {
        assert!(is_question(b"who is JellyfinServer?"));
        assert!(is_question(b"Who is JellyfinServer?"), "as one app writes it");
        assert!(is_question(b"who is JellyfinServer?\n"));
        for other in [&b"who is JellyfinServer? and more"[..], b"who is EmbyServer?", b"pley?", b""] {
            assert!(!is_question(other), "{other:?}");
        }
    }

    #[test]
    fn answers_are_counted_per_address_and_in_all() {
        let mut limits = Limits::default();
        let now = Instant::now();
        let one: IpAddr = "192.168.1.20".parse().unwrap();
        for _ in 0..PER_SOURCE {
            assert!(limits.allow(one, now));
        }
        assert!(!limits.allow(one, now), "one address");
        assert!(limits.allow("192.168.1.21".parse().unwrap(), now), "another");
        assert!(limits.allow(one, now + SOURCE_WINDOW), "later");

        let mut limits = Limits::default();
        for n in 0..MAX_ANSWERS {
            assert!(limits.allow(IpAddr::from([10, 0, (n / 256) as u8, (n % 256) as u8]), now));
        }
        assert!(!limits.allow("10.1.0.1".parse().unwrap(), now), "in all");
        assert!(limits.allow("10.1.0.1".parse().unwrap(), now + Duration::from_secs(1)));
    }

    fn listen(web: &[&str], jellyfin: &[&str], on_web: bool) -> Listen {
        Listen {
            web: web.iter().map(|a| a.parse().unwrap()).collect(),
            plex: Vec::new(),
            plex_on_web: false,
            jellyfin: jellyfin.iter().map(|a| a.parse().unwrap()).collect(),
            jellyfin_on_web: on_web,
            discovery: true,
            bonjour: false,
            bonjour_adapters: Vec::new(),
            bonjour_host_name: String::new(),
        }
    }

    #[test]
    fn the_address_is_the_apis_on_that_network() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let own = listen(&["0.0.0.0:7539"], &["0.0.0.0:8096"], false);
        assert_eq!(api_port(&own, ip("192.168.1.10")), Some(8096));
        let on_web = listen(&["192.168.1.10:7539", "10.0.0.5:7540"], &[], true);
        assert_eq!(api_port(&on_web, ip("10.0.0.5")), Some(7540));
        assert_eq!(api_port(&on_web, ip("172.16.0.1")), None, "not where the API is");
        let dual = listen(&["[::]:7539"], &[], true);
        assert_eq!(api_port(&dual, ip("192.168.1.10")), Some(7539));
        let v4 = listen(&["0.0.0.0:7539"], &[], true);
        assert_eq!(api_port(&v4, ip("fd00::5")), None);
        assert_eq!(url(ip("192.168.1.10"), 8096), "http://192.168.1.10:8096");
        assert_eq!(url(ip("fd00::5"), 8096), "http://[fd00::5]:8096");
    }

    #[test]
    fn answers_stay_small() {
        assert_eq!(short_name(&"n".repeat(100)).len(), MAX_NAME_CHARS);
        assert_eq!(short_name(&"é".repeat(100)).chars().count(), MAX_NAME_CHARS);
        assert_eq!(short_name(&"😀".repeat(100)).len(), MAX_NAME_BYTES);
        let name = short_name(&"😀".repeat(100));
        let id = "0123456789abcdef0123456789abcdef";
        let address = url("fd00:1234:5678:9abc:def0:1234:5678:9abc".parse().unwrap(), 65535);
        let answer = Answer { address, id, name: &name, endpoint_address: None };
        assert!(serde_json::to_vec(&answer).unwrap().len() < 300);
    }
}
