//! Making sure the server at an address is the one the window signed in
//! to before its session goes there (docs/API.md, "Telling servers
//! apart"): an address can lead to another device later, on another network
//! or after the router hands it out again. The window's session gets a key
//! from its server (`/api/server/proof-key`), kept by address in
//! `proofs.json`; before the app opens a server, and before its own
//! requests with the session (notices, downloads), the server at that
//! address proves it holds the key, for that address.
//!
//! The pages the window shows send the session themselves, which the app
//! can't hold back. They open only after a proof, and when the computer's
//! own addresses change (another network) the app takes the window off a
//! plain-http server within a second and opens it again through a proof
//! ([`watch_network`]); a page may still send a request in that second.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use base64::Engine;
use ring::hmac;
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::said::Said;

/// A proof counts this long; a dropped connection or another network asks
/// for a new one sooner.
const PROVED_FOR: Duration = Duration::from_secs(30 * 60);
/// How long a server gets to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
/// Before the server's id and the challenge in what a proof covers.
const PROOF_CONTEXT: &str = "pley-server-proof";

/// A key the server at an address gave the window's session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub server_id: String,
    pub proof_id: String,
    /// base64url without padding, 32 bytes.
    pub proof_key: String,
}

impl Key {
    fn bytes(&self) -> Option<Vec<u8>> {
        b64().decode(&self.proof_key).ok().filter(|k| k.len() == 32)
    }
}

/// Why the app keeps the session to itself.
#[derive(Debug, PartialEq)]
pub enum Refused {
    /// Nothing answered, or not as a Pley server does.
    Unreachable(Said),
    /// Something answered without the proof.
    NotYours,
    /// The server proved itself for another address than the one asked: a
    /// proxy in front of it passes another Host (or scheme) on. It gets no
    /// session either, as another device could say the same.
    Misnamed(String),
}

impl Refused {
    /// What the picker says of it.
    pub fn said(self) -> Said {
        match self {
            Self::Unreachable(why) => why,
            Self::NotYours => Said::NotYours,
            Self::Misnamed(address) => Said::ProvesItselfAs { address },
        }
    }
}

#[derive(Default)]
struct Kept {
    keys: HashMap<String, Key>,
    /// When the server at each address last proved itself: by the clock,
    /// which also runs while the computer sleeps.
    proved: HashMap<String, SystemTime>,
}

/// The keys, by address (an origin, as `http://nas:7539`).
pub struct Proofs {
    path: Option<PathBuf>,
    kept: Mutex<Kept>,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

fn address(server: &Url) -> String {
    server.origin().ascii_serialization()
}

impl Proofs {
    /// The keys kept in `path`. None when it can't be read, which is said:
    /// each server is then trusted as it was before it gave a key.
    pub fn load(path: PathBuf) -> Self {
        let keys = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|err| err.to_string()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(err) => Err(err.to_string()),
        }
        .unwrap_or_else(|err| {
            eprintln!("cannot read the servers' keys in {}: {err}", path.display());
            HashMap::new()
        });
        Self { path: Some(path), kept: Mutex::new(Kept { keys, proved: HashMap::new() }) }
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self { path: None, kept: Mutex::default() }
    }

    /// Keys for `server`, which it proved it holds a moment ago (or not).
    #[cfg(test)]
    pub fn for_tests(server: &Url, proved: bool) -> Self {
        let proofs = Self::in_memory();
        proofs.keep(server, tests::key());
        if proved {
            proofs.proved(server);
        }
        proofs
    }

    fn key(&self, server: &Url) -> Option<Key> {
        self.kept.lock().unwrap().keys.get(&address(server)).cloned()
    }

    fn keep(&self, server: &Url, key: Key) {
        let mut kept = self.kept.lock().unwrap();
        kept.keys.insert(address(server), key);
        self.save(&kept.keys);
    }

    /// Forgets the key for `server` (the server was forgotten).
    pub fn forget(&self, server: &Url) {
        let mut kept = self.kept.lock().unwrap();
        let address = address(server);
        kept.proved.remove(&address);
        if kept.keys.remove(&address).is_some() {
            self.save(&kept.keys);
        }
    }

    /// The next use of `server` asks for a proof again: a connection there
    /// dropped, and the computer may be on another network now.
    pub fn doubt(&self, server: &Url) {
        self.kept.lock().unwrap().proved.remove(&address(server));
    }

    /// The computer is on another network: every server proves itself again.
    pub fn doubt_all(&self) {
        self.kept.lock().unwrap().proved.clear();
    }

    fn proved(&self, server: &Url) {
        self.kept.lock().unwrap().proved.insert(address(server), SystemTime::now());
    }

    /// Whether the window may open `server`: it has no key yet (the window
    /// asks for one with its session there), or it proved itself lately on
    /// this network.
    pub fn may_open(&self, server: &Url) -> bool {
        self.key(server).is_none() || self.proved_lately(server)
    }

    /// Whether `server` has no key while another address on its host has
    /// one: the window's cookies go to every port of a host, so that
    /// server's session would go to `server` too.
    pub fn host_keyed_elsewhere(&self, server: &Url) -> bool {
        let kept = self.kept.lock().unwrap();
        let address = address(server);
        !kept.keys.contains_key(&address)
            && kept.keys.keys().any(|other| {
                Url::parse(other).is_ok_and(|other| other.host_str().is_some() && other.host_str() == server.host_str())
            })
    }

    fn proved_lately(&self, server: &Url) -> bool {
        // A clock set back gives an error: proved again.
        let kept = self.kept.lock().unwrap();
        kept.proved.get(&address(server)).and_then(|at| at.elapsed().ok()).is_some_and(|age| age < PROVED_FOR)
    }

    fn save(&self, keys: &HashMap<String, Key>) {
        let Some(path) = &self.path else { return };
        let written = (|| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let partial = path.with_extension("json.part");
            let mut file = std::fs::OpenOptions::new();
            file.write(true).create(true).truncate(true);
            // The keys are for this account only.
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut file, 0o600);
            std::io::Write::write_all(&mut file.open(&partial)?, &serde_json::to_vec_pretty(keys)?)?;
            std::fs::rename(&partial, path)
        })();
        if let Err(err) = written {
            eprintln!("cannot keep the servers' keys: {err}");
        }
    }
}

/// Whether `answered` proves `challenge` for `key`, asked at `address`.
fn proves(key: &Key, address: &str, challenge: &str, server_id: Option<&str>, answered: Option<&str>) -> bool {
    let (Some(bytes), Some(answered)) = (key.bytes(), answered.and_then(|a| b64().decode(a).ok())) else {
        return false;
    };
    server_id == Some(key.server_id.as_str())
        && hmac::verify(
            &hmac::Key::new(hmac::HMAC_SHA256, &bytes),
            format!("{PROOF_CONTEXT}\n{}\n{address}\n{challenge}", key.server_id).as_bytes(),
            &answered,
        )
        .is_ok()
}

/// The client proofs and keys are asked with: it follows no redirect, as a
/// proof counts only from the address itself (a redirect could lead to the
/// real server from anywhere), and the session goes nowhere else.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ANSWER_TIMEOUT)
            .build()
            // As reqwest::Client::new(): only a TLS backend that can't start fails.
            .expect("an HTTP client")
    })
}

fn challenge() -> Option<String> {
    let mut bytes = [0u8; 16];
    ring::rand::SystemRandom::new().fill(&mut bytes).ok()?;
    Some(b64().encode(bytes))
}

/// What `/api/server` says, as far as proofs go.
#[derive(Deserialize)]
struct Answer {
    id: Option<String>,
    proof: Option<String>,
    /// The address the server proved itself for.
    address: Option<String>,
}

/// `/api/server` of `server`, asked to prove it holds `key` when there is
/// one, as JSON: what the app checks a server with before it opens it.
pub async fn ask(server: &Url, proofs: &Proofs) -> Result<serde_json::Value, Refused> {
    let key = proofs.key(server);
    let mut endpoint = server.join("/api/server").map_err(|e| Refused::Unreachable(e.to_string().into()))?;
    let challenge = match &key {
        Some(key) => {
            let challenge = challenge().ok_or_else(|| Refused::Unreachable("no random numbers".to_owned().into()))?;
            endpoint.query_pairs_mut().append_pair("challenge", &challenge).append_pair("proof_id", &key.proof_id);
            Some(challenge)
        }
        None => None,
    };
    let response = client().get(endpoint).send().await.map_err(|err| {
        let why = if err.is_timeout() {
            Said::NoAnswerWithin { seconds: ANSWER_TIMEOUT.as_secs() }
        } else if err.is_connect() {
            Said::NothingAnswers
        } else {
            err.without_url().to_string().into()
        };
        Refused::Unreachable(why)
    })?;
    let status = response.status();
    if status.is_server_error() {
        // The server, or a proxy before it, out of reach for now.
        return Err(Refused::Unreachable(Said::AnsweredHttp { status: status.as_u16() }));
    }
    if !status.is_success() {
        // A redirect or a refusal doesn't prove anything: another device,
        // when a key came from the server that was there.
        return Err(match key {
            Some(_) => Refused::NotYours,
            None => Refused::Unreachable(Said::NotPleyHttp { status: status.as_u16() }),
        });
    }
    let body: serde_json::Value = response.json().await.map_err(|_| match key {
        Some(_) => Refused::NotYours,
        None => Refused::Unreachable(Said::NotPley),
    })?;
    if let (Some(key), Some(challenge)) = (key, challenge) {
        let answer: Answer =
            serde_json::from_value(body.clone()).unwrap_or(Answer { id: None, proof: None, address: None });
        if !proves(&key, &address(server), &challenge, answer.id.as_deref(), answer.proof.as_deref()) {
            return Err(match answer.address {
                Some(other) if answer.id.as_deref() == Some(key.server_id.as_str()) && other != address(server) => {
                    Refused::Misnamed(other)
                }
                _ => Refused::NotYours,
            });
        }
        proofs.proved(server);
    }
    Ok(body)
}

/// Gets a key for `session` from `server` when the app has none for it.
async fn ask_for_key(server: &Url, proofs: &Proofs, session: &str) {
    if proofs.key(server).is_some() {
        return;
    }
    let Ok(url) = server.join("/api/server/proof-key") else { return };
    let request = client().get(url).header(reqwest::header::COOKIE, session);
    let Ok(response) = request.send().await else { return };
    if !response.status().is_success() {
        return;
    }
    if let Ok(key) = response.json::<Key>().await
        && key.bytes().is_some()
    {
        proofs.keep(server, key);
        // It came from the server the window has been using: proved as much as that.
        proofs.proved(server);
    }
}

/// `session` (the window's cookies for `server`), once the server proved
/// itself: within the last half hour, or now. Without a key yet, it asks
/// for one: the window uses the session there already.
pub async fn session(server: &Url, proofs: &Proofs, session: Option<String>) -> Result<Option<String>, Refused> {
    let Some(session) = session else { return Ok(None) };
    if proofs.key(server).is_none() {
        ask_for_key(server, proofs, &session).await;
    } else if !proofs.proved_lately(server) {
        ask(server, proofs).await?;
    }
    Ok(Some(session))
}

/// The computer's own IPv4 addresses, which change with the network it is
/// on (loopback left out).
pub fn local_addresses() -> Vec<IpAddr> {
    let mut addresses: Vec<IpAddr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .map(|i| i.ip())
        .filter(|ip| ip.is_ipv4() && !ip.is_loopback())
        .collect();
    addresses.sort();
    addresses.dedup();
    addresses
}

/// The address of this computer that a connection to `server` would leave
/// from (a UDP socket "connected" there, which sends nothing): it changes
/// with the network that leads there.
pub fn route_to(server: &Url) -> Option<IpAddr> {
    let target = server.socket_addrs(|| None).ok()?.into_iter().next()?;
    let any: IpAddr =
        if target.is_ipv4() { std::net::Ipv4Addr::UNSPECIFIED.into() } else { std::net::Ipv6Addr::UNSPECIFIED.into() };
    let socket = std::net::UdpSocket::bind((any, 0)).ok()?;
    socket.connect(target).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// How long the watch's second of rest may last by the clock before the
/// computer counts as having slept in it (it may wake on another network).
const SLEPT: Duration = Duration::from_secs(5);

/// Looks at the computer's addresses every second, for as long as the app
/// runs, and calls `on_change` when they change, or after the computer
/// slept (`true`): every server proves itself again then, and the window
/// leaves a server whose pages could otherwise send the session to whatever
/// answers at its address now. A network that gives the computer the same
/// addresses, joined in less than a second, goes unseen. Only the rest
/// counts towards sleep, not the time `on_change` takes.
pub async fn watch_network(on_change: impl Fn(bool)) {
    let mut seen = local_addresses();
    loop {
        let rested = SystemTime::now();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let slept = rested.elapsed().map_or(true, |gap| gap > SLEPT);
        let now = local_addresses();
        if now != seen || slept {
            seen = now;
            on_change(slept);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// The proof of `challenge` under `key` from server `server_id`.
    fn proof(key: &[u8], server_id: &str, address: &str, challenge: &str) -> hmac::Tag {
        hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, key),
            format!("{PROOF_CONTEXT}\n{server_id}\n{address}\n{challenge}").as_bytes(),
        )
    }

    const ID: &str = "0123456789abcdef0123456789abcdef01234567";
    const CHALLENGE: &str = "AAECAwQFBgcICQoLDA0ODw";

    pub fn key() -> Key {
        let bytes = [1u8; 32];
        Key { server_id: ID.into(), proof_id: "sAAECAwQFBgcICQoLDA0ODw".into(), proof_key: b64().encode(bytes) }
    }

    #[test]
    fn a_proof_is_the_servers_hmac() {
        // The server's known answer (apps/server/src/identity.rs).
        let at = "http://192.168.1.20:7539";
        let known = "YwkvaCQfaIQCZkoOP5yOE3yR4xRKU5T5wYWz4b85SCM";
        assert_eq!(b64().encode(proof(&key().bytes().unwrap(), ID, at, CHALLENGE).as_ref()), known);
        assert!(proves(&key(), at, CHALLENGE, Some(ID), Some(known)));
        assert!(!proves(&key(), at, "AAECAwQFBgcICQoLDA0OEA", Some(ID), Some(known)), "another challenge");
        assert!(!proves(&key(), at, CHALLENGE, Some("another"), Some(known)), "another server");
        assert!(!proves(&key(), "https://pley.example.com", CHALLENGE, Some(ID), Some(known)), "another address");
        assert!(!proves(&key(), at, CHALLENGE, Some(ID), None));
        assert!(!proves(&key(), at, CHALLENGE, None, Some(known)));
        let short = Key { proof_key: b64().encode([1u8; 16]), ..key() };
        assert!(!proves(&short, at, CHALLENGE, Some(ID), Some(known)));
    }

    #[test]
    fn a_host_keyed_at_another_port_keeps_its_session() {
        let server = Url::parse("http://192.168.1.20:7539").unwrap();
        let proofs = Proofs::for_tests(&server, false);
        assert!(!proofs.host_keyed_elsewhere(&server), "its own key");
        // The window's cookies would go to these too.
        for same_host in ["http://192.168.1.20:7540", "https://192.168.1.20"] {
            assert!(proofs.host_keyed_elsewhere(&Url::parse(same_host).unwrap()), "{same_host}");
        }
        assert!(!proofs.host_keyed_elsewhere(&Url::parse("http://192.168.1.21:7539").unwrap()));
    }

    /// A server that answers each connection with the next of `answers`
    /// (status and JSON body), and tells what each request asked.
    async fn serve(answers: Vec<(u16, String)>) -> (Url, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            for (status, body) in answers {
                let (mut socket, _) = listener.accept().await.unwrap();
                let _ = tx.send(request_on(&mut socket).await);
                let answer = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(answer.as_bytes()).await.unwrap();
            }
        });
        (url, rx)
    }

    /// The challenge in a request for `/api/server`.
    fn challenge_in(request: &str) -> String {
        let path = request.split_whitespace().nth(1).unwrap();
        let url = Url::parse(&format!("http://x{path}")).unwrap();
        url.query_pairs().find(|(k, _)| k == "challenge").map(|(_, v)| v.into_owned()).unwrap()
    }

    /// Reads a request up to its blank line.
    async fn request_on(socket: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = socket.read(&mut buf).await.unwrap();
            request.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&request).into_owned()
    }

    /// The server itself, which proves it holds [`key`] `times` times (its
    /// answer depends on the challenge), and then says how often it did.
    async fn proving(times: usize) -> (Url, tokio::task::JoinHandle<usize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let bytes = key().bytes().unwrap();
        let at = address(&server);
        let answering = tokio::spawn(async move {
            for _ in 0..times {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = request_on(&mut socket).await;
                assert!(!request.contains("pley_token"), "{request}");
                let proved = b64().encode(proof(&bytes, ID, &at, &challenge_in(&request)).as_ref());
                let body = serde_json::json!({"id": ID, "name": "Den", "proof": proved}).to_string();
                let answer = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(answer.as_bytes()).await.unwrap();
            }
            times
        });
        (server, answering)
    }

    #[tokio::test]
    async fn the_session_goes_only_where_the_server_proves_itself() {
        let proofs = Proofs::in_memory();
        // No key yet: the session asks for one.
        let given = serde_json::json!({"server_id": ID, "proof_id": key().proof_id, "proof_key": key().proof_key});
        let (server, mut asked) = serve(vec![(200, given.to_string())]).await;
        let cookie = Some("pley_token=abc".to_owned());
        assert_eq!(session(&server, &proofs, cookie.clone()).await, Ok(cookie.clone()));
        let request = asked.recv().await.unwrap();
        assert!(request.starts_with("GET /api/server/proof-key "), "{request}");
        assert!(request.contains("cookie: pley_token=abc"), "{request}");
        assert_eq!(proofs.key(&server), Some(key()));
        // Proved lately: nothing to ask.
        assert_eq!(session(&server, &proofs, cookie.clone()).await, Ok(cookie.clone()));
        assert!(asked.try_recv().is_err());
    }

    #[tokio::test]
    async fn another_device_at_the_address_gets_nothing() {
        let cookie = Some("pley_token=abc".to_owned());
        let wrong =
            serde_json::json!({"id": ID, "name": "Den", "proof": "-OpCw2gYEDYBr-d7wh4JEYd7CHzZ8nc7pTc_UlsqVas"});
        let another = serde_json::json!({"id": "another", "name": "Den", "version": "0.1.2"});
        for answer in
            [(200, wrong.to_string()), (200, another.to_string()), (400, "{}".to_owned()), (200, "<html>".to_owned())]
        {
            let (server, mut asked) = serve(vec![answer.clone()]).await;
            let proofs = Proofs::for_tests(&server, false);
            assert_eq!(session(&server, &proofs, cookie.clone()).await, Err(Refused::NotYours), "{answer:?}");
            let request = asked.recv().await.unwrap();
            assert!(request.contains("proof_id=sAAECAwQFBgcICQoLDA0ODw"), "{request}");
            assert!(!request.contains("pley_token"), "the session stays home: {request}");
        }
    }

    #[tokio::test]
    async fn a_redirect_to_the_server_proves_nothing() {
        let (real, _) = proving(1).await;
        // Another device at the address sends the challenge on to the server.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let impostor = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = request_on(&mut socket).await;
            let path = request.split_whitespace().nth(1).unwrap();
            let to = real.join(path).unwrap();
            let answer =
                format!("HTTP/1.1 302 Found\r\nlocation: {to}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            socket.write_all(answer.as_bytes()).await.unwrap();
        });
        let proofs = Proofs::for_tests(&impostor, false);
        let cookie = Some("pley_token=abc".to_owned());
        assert_eq!(session(&impostor, &proofs, cookie).await, Err(Refused::NotYours));
    }

    #[tokio::test]
    async fn a_server_error_is_out_of_reach_not_another_device() {
        let (server, _asked) = serve(vec![(502, "Bad Gateway".to_owned())]).await;
        let proofs = Proofs::for_tests(&server, false);
        let refused = session(&server, &proofs, Some("pley_token=abc".to_owned())).await;
        assert!(matches!(refused, Err(Refused::Unreachable(_))), "{refused:?}");
    }

    #[tokio::test]
    async fn the_server_proves_itself_again_after_a_doubt() {
        let cookie = Some("pley_token=abc".to_owned());
        let (server, answering) = proving(2).await;
        let proofs = Proofs::for_tests(&server, false);
        assert_eq!(session(&server, &proofs, cookie.clone()).await, Ok(cookie.clone()));
        assert_eq!(session(&server, &proofs, cookie.clone()).await, Ok(cookie.clone()), "proved lately");
        proofs.doubt(&server);
        assert_eq!(session(&server, &proofs, cookie.clone()).await, Ok(cookie.clone()));
        assert_eq!(answering.await.unwrap(), 2, "proved at first and after the doubt");
        // Without a session, nothing to prove.
        assert_eq!(session(&server, &proofs, None).await, Ok(None));
    }

    #[tokio::test]
    async fn another_network_has_every_server_prove_itself_again() {
        let cookie = Some("pley_token=abc".to_owned());
        let (server, answering) = proving(2).await;
        let proofs = Proofs::for_tests(&server, false);
        assert_eq!(session(&server, &proofs, cookie.clone()).await, Ok(cookie.clone()));
        proofs.doubt_all();
        assert_eq!(session(&server, &proofs, cookie.clone()).await, Ok(cookie.clone()));
        assert_eq!(answering.await.unwrap(), 2);
    }

    #[tokio::test]
    async fn a_proof_for_another_address_names_the_proxy() {
        // The real server behind a proxy that passes another Host on: its
        // proof is for that address, which doesn't count here either.
        let bytes = key().bytes().unwrap();
        let other = "https://pley.example.com";
        let elsewhere = b64().encode(proof(&bytes, ID, other, CHALLENGE).as_ref());
        let body = serde_json::json!({"id": ID, "proof": elsewhere, "address": other}).to_string();
        let (server, _asked) = serve(vec![(200, body)]).await;
        let proofs = Proofs::for_tests(&server, false);
        let refused = session(&server, &proofs, Some("pley_token=abc".to_owned())).await;
        assert_eq!(refused, Err(Refused::Misnamed(other.to_owned())));
        assert!(Refused::Misnamed(other.to_owned()).said().to_string().contains("X-Forwarded-Proto"));
    }

    #[test]
    fn a_server_opens_once_it_proved_itself_on_this_network() {
        let server = Url::parse("http://192.168.1.20:7539").unwrap();
        assert!(Proofs::in_memory().may_open(&server), "no key yet: the window asks for one");
        assert!(!Proofs::for_tests(&server, false).may_open(&server));
        let proofs = Proofs::for_tests(&server, true);
        assert!(proofs.may_open(&server));
        proofs.doubt_all();
        assert!(!proofs.may_open(&server), "another network since");
    }

    #[test]
    fn the_way_to_a_server_is_the_address_it_leaves_from() {
        let local = Url::parse("http://127.0.0.1:7539").unwrap();
        assert_eq!(route_to(&local), Some("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn keys_are_kept_by_address_and_forgotten_with_the_server() {
        let dir = std::env::temp_dir().join(format!("pley-proofs-{}", std::process::id()));
        let path = dir.join("proofs.json");
        let server = Url::parse("http://nas:7539/library").unwrap();
        let proofs = Proofs::load(path.clone());
        proofs.keep(&server, key());
        assert_eq!(Proofs::load(path.clone()).key(&Url::parse("http://nas:7539").unwrap()), Some(key()));
        assert_eq!(Proofs::load(path.clone()).key(&Url::parse("http://nas:7540").unwrap()), None);
        proofs.forget(&server);
        assert_eq!(Proofs::load(path).key(&server), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
