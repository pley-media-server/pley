//! Finding Pley servers: the UDP broadcast the Android app sends, the /24
//! HTTP sweep of "Search this network" (packages/tv/src/auth/servers.ts),
//! and probing an address someone typed or a saved server.

use std::collections::HashSet;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use pley_base::Waker;
use pley_i18n::t;

use crate::addresses::{DEFAULT_PORT, candidates, display_address, host_port, is_local_host, origin};
use crate::http::{Http, NetError, Request};
use crate::model::{Announcement, ServerInfo};
use crate::task::Cancel;

/// What a broadcast asks: the 5 bytes `pley?`.
pub const PROBE: &[u8] = b"pley?";
/// When the question is sent, from the start (Wi-Fi drops datagrams).
pub const SCHEDULE: [Duration; 3] = [Duration::ZERO, Duration::from_millis(200), Duration::from_millis(600)];
/// How long answers are listened for, from the start.
pub const WINDOW: Duration = Duration::from_millis(1500);
/// How many addresses a sweep asks at once, how long each may take, and how long it may run.
pub const SWEEP_PARALLEL: usize = 48;
/// How long one address may take in a sweep.
pub const SWEEP_TIMEOUT: Duration = Duration::from_millis(1500);
/// How long a whole sweep may take.
pub const SWEEP_TOTAL: Duration = Duration::from_secs(6);

/// A Pley server that answered.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FoundServer {
    /// Its origin: `http://<address>:<port>`.
    pub url: String,
    pub name: String,
    pub version: String,
    /// The discovery id (40 hex digits), when a broadcast found it.
    pub id: Option<String>,
}

/// The server a broadcast answer from `from` names; none unless it is a
/// version 1 answer with a port.
pub fn parse_answer(bytes: &[u8], from: SocketAddr) -> Option<FoundServer> {
    let a: Announcement = serde_json::from_slice(bytes).ok()?;
    if a.pley != 1 || a.port == 0 {
        return None;
    }
    let host = match from {
        SocketAddr::V4(v4) => v4.ip().to_string(),
        // IPv6 in brackets, its zone left out.
        SocketAddr::V6(v6) => format!("[{}]", v6.ip()),
    };
    let url = origin(&format!("http://{host}:{}", a.port))?;
    let name = if a.name.trim().is_empty() { display_address(&url) } else { a.name.trim().to_string() };
    Some(FoundServer { url, name, version: a.version, id: a.id.filter(|i| !i.is_empty()) })
}

/// Broadcasts `pley?` to UDP 7539 (everywhere, and to the subnet of `local`
/// when known) at 0, 200 and 600 ms, telling `on_found` each server once
/// as it answers, for 1.5 s.
pub fn broadcast(local: Option<Ipv4Addr>, on_found: &mut dyn FnMut(FoundServer), cancel: &Cancel) -> io::Result<()> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    let mut targets = vec![SocketAddr::from((Ipv4Addr::BROADCAST, DEFAULT_PORT))];
    if let Some(ip) = local {
        let [a, b, c, _] = ip.octets();
        targets.push(SocketAddr::from((Ipv4Addr::new(a, b, c, 255), DEFAULT_PORT)));
    }
    let start = Instant::now();
    let mut sent = 0;
    let mut seen = HashSet::new();
    let mut buf = [0u8; 2048];
    while start.elapsed() < WINDOW && !cancel.is_cancelled() {
        while sent < SCHEDULE.len() && start.elapsed() >= SCHEDULE[sent] {
            for t in &targets {
                // A network that refuses one target still gets the others.
                let _ = socket.send_to(PROBE, t);
            }
            sent += 1;
        }
        match socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                if let Some(server) = parse_answer(&buf[..n], from) {
                    let key = server.id.clone().unwrap_or_else(|| server.url.clone());
                    if seen.insert(key) {
                        on_found(server);
                    }
                }
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            // Windows reports an unreachable port of an earlier datagram here: keep listening.
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// The /24 of an IPv4 address without its last number (`192.168.1.`); none
/// for anything else, and for `0.x` and `127.x`.
pub fn subnet_of(address: &str) -> Option<String> {
    let ip: Ipv4Addr = address.trim().parse().ok()?;
    let [a, b, c, _] = ip.octets();
    if a == 0 || a == 127 {
        return None;
    }
    Some(format!("{a}.{b}.{c}."))
}

/// How long a probe of `url` may take: 3 s on the home network, 10 s elsewhere.
pub fn probe_timeout(url: &str) -> Duration {
    let local = crate::addresses::Url::parse(url).is_some_and(|u| is_local_host(&u.host));
    if local { Duration::from_secs(3) } else { Duration::from_secs(10) }
}

/// Asks `url` whether it is Pley (`GET /api/server`, no token): its name and
/// version, none when something else answered.
pub fn probe(http: &Http, url: &str, timeout: Duration) -> Result<Option<FoundServer>, NetError> {
    let req = Request::get(format!("{}/api/server", url.trim_end_matches('/')))
        .header("Accept", "application/json")
        .timeout(timeout)
        .connect_timeout(timeout);
    let answer = http.send(req)?;
    if answer.status != 200 {
        return Ok(None);
    }
    let Ok(info) = serde_json::from_slice::<ServerInfo>(&answer.body) else { return Ok(None) };
    let Some(version) = info.version else { return Ok(None) };
    let url = origin(url).unwrap_or_else(|| url.to_string());
    let name = if info.name.trim().is_empty() { display_address(&url) } else { info.name.trim().to_string() };
    Ok(Some(FoundServer { url, name, version, id: None }))
}

/// Why an address isn't a server to sign in to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectProblem {
    BadAddress,
    NotPley { address: String },
    Unreachable { address: String, reason: String },
}

impl ConnectProblem {
    /// The Android app's words for it.
    pub fn text(&self) -> String {
        match self {
            ConnectProblem::BadAddress => t!("errors.connect.badAddress").into(),
            ConnectProblem::NotPley { address } => t!("errors.connect.notPley", address = address),
            ConnectProblem::Unreachable { address, reason } => {
                t!("errors.connect.unreachable", address = address, reason = reason)
            }
        }
    }
}

/// The server someone typed (Enter an address's Connect): each candidate in
/// turn, https first for names on the internet; the first Pley answer wins.
pub fn connect(http: &Http, input: &str) -> Result<FoundServer, ConnectProblem> {
    let list = candidates(input);
    if list.is_empty() {
        return Err(ConnectProblem::BadAddress);
    }
    let mut not_pley = None;
    let mut last = None;
    for url in &list {
        match probe(http, url, probe_timeout(url)) {
            Ok(Some(server)) => return Ok(server),
            Ok(None) => {
                not_pley.get_or_insert_with(|| host_port(url));
            }
            Err(e) => last = Some((host_port(url), e.reason().to_string())),
        }
    }
    if let Some(address) = not_pley {
        return Err(ConnectProblem::NotPley { address });
    }
    let (address, reason) = last.unwrap_or_else(|| (input.to_string(), NetError::Refused.reason().into()));
    Err(ConnectProblem::Unreachable { address, reason })
}

/// A server already known (a saved row): it answers as Pley, or why not.
pub fn check(http: &Http, url: &str) -> Result<FoundServer, ConnectProblem> {
    match probe(http, url, probe_timeout(url)) {
        Ok(Some(server)) => Ok(server),
        Ok(None) => Err(ConnectProblem::NotPley { address: host_port(url) }),
        Err(e) => Err(ConnectProblem::Unreachable { address: host_port(url), reason: e.reason().into() }),
    }
}

/// What a search finds, as it goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchNews {
    Found(FoundServer),
    /// Nothing more will come.
    Done,
}

/// Looks for servers on the discovery thread, as the servers screen opens
/// (the broadcast) or asks to "Search this network" (`sweep`, the /24 of
/// `local`): each server once to `tx` as it answers, then [`SearchNews::Done`],
/// each with a wake of the main loop. Cancelling the token stops it.
pub fn search(http: Http, local: Option<Ipv4Addr>, sweep_too: bool, tx: Sender<SearchNews>, waker: Waker) -> Cancel {
    let cancel = Cancel::new();
    let c = cancel.clone();
    std::thread::Builder::new()
        .name("pley-discovery".into())
        .spawn(move || {
            let seen = Mutex::new(HashSet::new());
            let tell = |server: FoundServer| {
                let fresh = seen.lock().unwrap_or_else(|e| e.into_inner()).insert(server.url.clone());
                if fresh && !c.is_cancelled() && tx.send(SearchNews::Found(server)).is_ok() {
                    waker.wake();
                }
            };
            // A TV that refuses broadcasts still has the sweep.
            let _ = broadcast(local, &mut |s| tell(s), &c);
            if sweep_too && let Some(subnet) = local.and_then(|ip| subnet_of(&ip.to_string())) {
                sweep(&http, &subnet, &tell, &c);
            }
            if !c.is_cancelled() && tx.send(SearchNews::Done).is_ok() {
                waker.wake();
            }
        })
        .expect("the discovery thread starts");
    cancel
}

/// "Search this network": `GET /api/server` on every address of `subnet`
/// (`192.168.1.`) at Pley's port, 48 at a time, 1.5 s each, 6 s in all.
/// Servers are told to `on_found` as they answer.
pub fn sweep(http: &Http, subnet: &str, on_found: &(dyn Fn(FoundServer) + Sync), cancel: &Cancel) {
    let started = Instant::now();
    let next = AtomicUsize::new(1);
    let found = Mutex::new(HashSet::new());
    std::thread::scope(|s| {
        for _ in 0..SWEEP_PARALLEL {
            s.spawn(|| {
                loop {
                    let left = SWEEP_TOTAL.saturating_sub(started.elapsed());
                    if cancel.is_cancelled() || left.is_zero() {
                        return;
                    }
                    let n = next.fetch_add(1, Ordering::SeqCst);
                    if n > 254 {
                        return;
                    }
                    let url = format!("http://{subnet}{n}:{DEFAULT_PORT}");
                    let wait = SWEEP_TIMEOUT.min(left).max(Duration::from_millis(1));
                    if let Ok(Some(server)) = cancel.scope(|| probe(http, &url, wait))
                        && !cancel.is_cancelled()
                        && found.lock().unwrap_or_else(|e| e.into_inner()).insert(server.url.clone())
                    {
                        on_found(server);
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::http::{FakeTransport, Response};

    fn from(ip: &str) -> SocketAddr {
        SocketAddr::new(ip.parse().unwrap(), 50000)
    }

    #[test]
    fn broadcast_answers_name_the_server_at_the_address_they_came_from() {
        let answer = br#"{"pley":1,"id":"3f2a","name":"Living room","version":"0.9.0","port":7539}"#;
        assert_eq!(
            parse_answer(answer, from("192.168.1.20")),
            Some(FoundServer {
                url: "http://192.168.1.20:7539".into(),
                name: "Living room".into(),
                version: "0.9.0".into(),
                id: Some("3f2a".into())
            })
        );
        let v6 = parse_answer(br#"{"pley":1,"name":"","version":"1","port":8080}"#, from("fd00::5")).unwrap();
        assert_eq!((v6.url.as_str(), v6.name.as_str(), v6.id), ("http://[fd00::5]:8080", "[fd00::5]:8080", None));
        assert_eq!(parse_answer(br#"{"pley":2,"port":7539}"#, from("10.0.0.1")), None);
        assert_eq!(parse_answer(br#"{"pley":1,"port":0}"#, from("10.0.0.1")), None);
        assert_eq!(parse_answer(b"pley?", from("10.0.0.1")), None);
    }

    #[test]
    fn subnets_come_from_ipv4_addresses() {
        assert_eq!(subnet_of("192.168.1.50").as_deref(), Some("192.168.1."));
        assert_eq!(subnet_of("127.0.0.1"), None);
        assert_eq!(subnet_of("0.0.0.0"), None);
        assert_eq!(subnet_of("fe80::1"), None);
    }

    fn server_at(ok: &'static str) -> Http {
        Http::new(
            Arc::new(FakeTransport::new(move |r| {
                if r.url.starts_with(ok) {
                    Ok(Response::json(200, &serde_json::json!({"name": " Den ", "version": "0.42.0"})))
                } else if r.url.contains("example.com") {
                    Ok(Response::json(200, &serde_json::json!({"hello": "world"})))
                } else {
                    Err(NetError::Refused)
                }
            })),
            "ua",
        )
    }

    #[test]
    fn a_probe_counts_a_string_version_as_pley() {
        let http = server_at("http://nas:7539");
        let found = probe(&http, "http://nas:7539", Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!((found.name.as_str(), found.version.as_str()), ("Den", "0.42.0"));
        assert_eq!(probe(&http, "https://www.example.com", Duration::from_secs(1)), Ok(None));
        assert_eq!(probe(&http, "http://elsewhere:7539", Duration::from_secs(1)), Err(NetError::Refused));
        assert_eq!(probe_timeout("http://nas:7539"), Duration::from_secs(3));
        assert_eq!(probe_timeout("https://pley.example.com"), Duration::from_secs(10));
    }

    #[test]
    fn connect_tries_https_first_then_says_why_nothing_worked() {
        let http = server_at("http://pley.example.org:7539");
        assert_eq!(connect(&http, "pley.example.org").unwrap().url, "http://pley.example.org:7539");
        assert_eq!(
            connect(&http, "www.example.com").unwrap_err().text(),
            "Something answers at www.example.com, but it isn't Pley."
        );
        assert_eq!(
            connect(&http, "10.9.9.9").unwrap_err().text(),
            "Couldn't reach 10.9.9.9:7539: nothing answers there."
        );
        assert_eq!(connect(&http, "two words").unwrap_err(), ConnectProblem::BadAddress);
        assert!(matches!(check(&http, "http://10.9.9.9:7539"), Err(ConnectProblem::Unreachable { .. })));
    }

    #[test]
    fn connect_says_why_in_french_and_spanish() {
        use pley_i18n::{Language, with_language};
        let http = server_at("http://pley.example.org:7539");
        with_language(Language::Fr, || {
            assert_eq!(
                connect(&http, "10.9.9.9").unwrap_err().text(),
                "Impossible de joindre 10.9.9.9:7539\u{a0}: rien ne répond à cette adresse."
            );
            assert_eq!(
                connect(&http, "www.example.com").unwrap_err().text(),
                "Quelque chose répond à www.example.com, mais ce n’est pas Pley."
            );
        });
        with_language(Language::Es, || {
            assert_eq!(
                ConnectProblem::BadAddress.text(),
                "Introduce una dirección como nas, 192.168.1.20 o pley.example.com."
            );
        });
    }

    #[test]
    fn the_sweep_asks_every_address_once_and_reports_each_server() {
        let fake = Arc::new(FakeTransport::new(|r| {
            if r.url == "http://192.168.1.20:7539/api/server" || r.url == "http://192.168.1.200:7539/api/server" {
                Ok(Response::json(200, &serde_json::json!({"name": "Pley", "version": "1"})))
            } else {
                Err(NetError::TimedOut)
            }
        }));
        let http = Http::new(fake.clone(), "ua");
        let found = Mutex::new(Vec::new());
        sweep(&http, "192.168.1.", &|s| found.lock().unwrap().push(s.url), &Cancel::new());
        let mut found = found.into_inner().unwrap();
        found.sort();
        assert_eq!(found, ["http://192.168.1.200:7539", "http://192.168.1.20:7539"]);
        assert_eq!(fake.requests().len(), 254);
    }

    #[test]
    fn a_broadcast_with_nobody_answering_ends_after_its_window() {
        let started = Instant::now();
        let mut found = Vec::new();
        // Broadcasts may be refused on a build machine; either way it must end on time.
        let _ = broadcast(Some(Ipv4Addr::new(127, 0, 0, 1)), &mut |s| found.push(s), &Cancel::new());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn a_search_reports_what_the_sweep_finds_then_ends() {
        let fake = Arc::new(FakeTransport::new(|r| {
            if r.url == "http://10.11.12.20:7539/api/server" {
                Ok(Response::json(200, &serde_json::json!({"name": "Attic", "version": "1"})))
            } else {
                Err(NetError::Refused)
            }
        }));
        let (tx, rx) = std::sync::mpsc::channel();
        let _search = search(Http::new(fake, "ua"), Some(Ipv4Addr::new(10, 11, 12, 50)), true, tx, Waker::noop());
        let mut news = Vec::new();
        while let Ok(n) = rx.recv_timeout(Duration::from_secs(15)) {
            let done = n == SearchNews::Done;
            news.push(n);
            if done {
                break;
            }
        }
        assert_eq!(news.last(), Some(&SearchNews::Done));
        assert!(news.iter().any(|n| matches!(n, SearchNews::Found(s) if s.name == "Attic")));
    }
}
