//! Reading the client metadata documents of other publishers' apps (CIMD §5,
//! §8.6, §8.7), carefully: a client ID is an address anyone can choose, so
//! fetching it must not let anyone probe the server's network.
//!
//! * pley's own apps are built in and never fetched.
//! * Nothing is fetched unless an administrator allows apps by other
//!   publishers, and then only for someone signed in.
//! * Only public addresses are connected to: the filter runs inside the
//!   resolver, so the address checked is the address connected to (no DNS
//!   rebinding), and no proxy resolves names around it.
//! * No redirects, cookies or decompression; 3 seconds to connect and 5 in
//!   all; a 200 answer of at most 5,120 bytes of JSON.
//! * At most 4 fetches at once, one per client ID (others wait for its
//!   result), and 10 a minute per person.
//! * Valid documents are kept for 5 minutes to 24 hours (CIMD §5.2), at most
//!   256 of them; failures never are.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::http;
use parking_lot::Mutex;
use reqwest::StatusCode;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{self, HeaderMap, HeaderValue};
use tokio::sync::{Semaphore, watch};
use url::Url;

use super::clients::{self, Client, ClientError, MAX_DOCUMENT_BYTES};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const TIMEOUT: Duration = Duration::from_secs(5);
/// Documents kept at most.
const CACHE_ENTRIES: usize = 256;
/// How long a document is kept: its max-age, within these bounds.
const SHORTEST: Duration = Duration::from_secs(5 * 60);
const LONGEST: Duration = Duration::from_secs(24 * 60 * 60);
/// How long a document without max-age is kept.
const USUAL: Duration = Duration::from_secs(60 * 60);
const FETCHES_AT_ONCE: usize = 4;
const FETCHES_PER_MINUTE: usize = 10;
const MINUTE: Duration = Duration::from_secs(60);

/// Where documents come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Fetched from their addresses.
    Internet,
    /// Never fetched: only pley's own apps sign in.
    Off,
    /// Client ID → document, answered as a 200 JSON answer without cache
    /// headers (integration tests).
    Fixed(HashMap<String, String>),
}

/// Whether an address may be fetched from: not one of the special-use
/// addresses (CIMD §8.6 "MUST NOT fetch … special-use IP addresses", with no
/// exception for loopback). The lists are spelled out because std's
/// `is_global` isn't stable. See the IANA registries:
/// <https://www.iana.org/assignments/iana-ipv4-special-registry/> and
/// <https://www.iana.org/assignments/iana-ipv6-special-registry/>.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !REFUSED_V4.iter().any(|&(network, len)| within(ip.to_bits().into(), network.to_bits().into(), len, 32))
        }
        IpAddr::V6(ip) => {
            let ip = ip.to_bits();
            within(ip, GLOBAL_UNICAST.to_bits(), 3, 128)
                && !REFUSED_V6.iter().any(|&(network, len)| within(ip, network.to_bits(), len, 128))
        }
    }
}

/// IPv4 networks never fetched from.
const REFUSED_V4: &[(Ipv4Addr, u32)] = &[
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    // Carrier-grade NAT, and Tailscale.
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    // Link-local, where clouds answer with their metadata.
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 88, 99, 0), 24),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    // Multicast.
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    // Reserved, 255.255.255.255 included.
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

/// 2000::/3: IPv6 is fetched from only inside it. That leaves out ::, ::1,
/// IPv4-mapped addresses, NAT64 (64:ff9b::/96, 64:ff9b:1::/48), 100::/64,
/// unique local (fc00::/7), link-local (fe80::/10), multicast (ff00::/8)
/// and 5f00::/16.
const GLOBAL_UNICAST: Ipv6Addr = Ipv6Addr::new(0x2000, 0, 0, 0, 0, 0, 0, 0);

/// Networks inside 2000::/3 never fetched from.
const REFUSED_V6: &[(Ipv6Addr, u32)] = &[
    // IETF protocol assignments, Teredo among them.
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
    // Documentation.
    (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
    // 6to4, which embeds any IPv4 address.
    (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
    // Documentation.
    (Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
];

/// Whether the first `len` of `bits` bits of `ip` and `network` agree.
fn within(ip: u128, network: u128, len: u32, bits: u32) -> bool {
    // A /0 network shifts everything out: every address is in it.
    let shift = bits.saturating_sub(len);
    ip.checked_shr(shift).unwrap_or(0) == network.checked_shr(shift).unwrap_or(0)
}

/// A resolver that gives only public addresses (see [`is_public`]), and
/// fails when no address is left. hyper-util connects to exactly the
/// addresses it gives, so there is no window between the check and the
/// connection for a rebinding name to change its answer.
pub struct PublicOnly;

/// A name [`PublicOnly`] refused: it resolves to no public address.
#[derive(Debug)]
pub struct NoPublicAddress(String);

impl std::fmt::Display for NoPublicAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} has no public address", self.0)
    }
}

impl std::error::Error for NoPublicAddress {}

impl Resolve for PublicOnly {
    fn resolve(&self, name: Name) -> Resolving {
        let name = name.as_str().to_owned();
        Box::pin(async move {
            let public: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 443)).await?.filter(|addr| is_public(addr.ip())).collect();
            if public.is_empty() {
                return Err(Box::new(NoPublicAddress(name)) as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(public.into_iter()) as Addrs)
        })
    }
}

/// The HTTP client for documents, and nothing else.
fn http_client() -> reqwest::Client {
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    reqwest::Client::builder()
        // A proxy would resolve names itself, around the filter.
        .no_proxy()
        .https_only(true)
        // CIMD §5: "MUST NOT automatically follow HTTP redirects".
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(PublicOnly))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TIMEOUT)
        // Nothing to decompress, so no compression bombs.
        .no_gzip()
        .default_headers(headers)
        .user_agent(concat!("pley/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("http client")
}

/// Reads an answer to a document fetch (CIMD §5, §8.7): status 200 only,
/// JSON (application/json or application/*+json), at most 5,120 bytes,
/// whatever Content-Length says or doesn't say. Gives the body and how long
/// it may be kept (`None`: not at all).
pub async fn read_document(mut response: reqwest::Response) -> Result<(Vec<u8>, Option<Duration>), ClientError> {
    let bad = |why: &str| Err(ClientError::Unreachable(why.to_owned()));
    let status = response.status();
    if status != StatusCode::OK {
        // A redirect too: CIMD §5 forbids following them.
        return Err(ClientError::Unreachable(format!("its address answered {}", status.as_u16())));
    }
    let content_type = response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let essence = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let json = essence == "application/json"
        || (essence.len() > "application/+json".len()
            && essence.starts_with("application/")
            && essence.ends_with("+json"));
    if !json {
        return bad("its address didn't answer with JSON");
    }
    let declared =
        response.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()?.trim().parse::<u64>().ok());
    if [declared, response.content_length()].into_iter().flatten().any(|length| length > MAX_DOCUMENT_BYTES as u64) {
        return bad("it is larger than 5,120 bytes");
    }
    let lifetime = lifetime(response.headers());
    let mut body = Vec::new();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(e) => return Err(failed(e)),
        };
        if body.len() + chunk.len() > MAX_DOCUMENT_BYTES {
            return bad("it is larger than 5,120 bytes");
        }
        body.extend_from_slice(&chunk);
    }
    Ok((body, lifetime))
}

/// How long a document may be kept, from Cache-Control (CIMD §5.2): its
/// max-age within 5 minutes and 24 hours, an hour without one, and not at
/// all with no-store. no-cache (use only after asking again) is read like
/// no-store, since pley doesn't ask again.
fn lifetime(headers: &HeaderMap) -> Option<Duration> {
    let mut max_age = None;
    for value in headers.get_all(header::CACHE_CONTROL) {
        let Ok(value) = value.to_str() else { continue };
        for directive in value.split(',') {
            let directive = directive.trim().to_ascii_lowercase();
            let (name, argument) = match directive.split_once('=') {
                Some((name, argument)) => (name.trim(), argument.trim().trim_matches('"')),
                None => (directive.as_str(), ""),
            };
            match name {
                "no-store" | "no-cache" => return None,
                "max-age"
                    if max_age.is_none() && !argument.is_empty() && argument.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    // Too many digits for a u64 is still "very long".
                    max_age = Some(argument.parse::<u64>().unwrap_or(u64::MAX));
                }
                _ => {}
            }
        }
    }
    Some(max_age.map_or(USUAL, Duration::from_secs).clamp(SHORTEST, LONGEST))
}

/// A fetch that didn't get an answer, logged with every cause (the
/// resolver's, DNS's, TLS's), but not the address: the client ID is logged
/// on its own where it is known. An address on a private network is
/// refused for good; anything else may work later.
fn failed(e: reqwest::Error) -> ClientError {
    let e = e.without_url();
    let causes: Vec<&(dyn std::error::Error + 'static)> =
        std::iter::successors(Some(&e as &(dyn std::error::Error + 'static)), |e| e.source()).collect();
    let chain: Vec<String> = causes.iter().map(ToString::to_string).collect();
    tracing::info!("an app's description couldn't be read: {}", chain.join(": "));
    if causes.iter().any(|cause| cause.is::<NoPublicAddress>()) {
        return ClientError::Private;
    }
    let why = if e.is_timeout() {
        "its address took too long to answer"
    } else if e.is_connect() {
        "its address couldn't be reached"
    } else {
        "its address didn't answer"
    };
    ClientError::Unreachable(why.to_owned())
}

/// A [`Source::Fixed`] document as a server would send it.
fn fixed_answer(document: Option<&String>) -> reqwest::Response {
    let answer = match document {
        Some(document) => http::Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(document.clone()),
        None => http::Response::builder().status(StatusCode::NOT_FOUND).body(String::new()),
    };
    answer.expect("a valid answer").into()
}

/// Answers for unit tests, which count and hold fetches.
#[cfg(test)]
type TestSource = Box<dyn Fn(String) -> futures::future::BoxFuture<'static, reqwest::Response> + Send + Sync>;

enum Fetcher {
    Internet(reqwest::Client),
    Off,
    Fixed(HashMap<String, String>),
    #[cfg(test)]
    Test(TestSource),
}

struct Cached {
    client: Client,
    expires: Instant,
}

/// What a fetch in progress ends with, for those waiting on it.
type Answer = watch::Receiver<Option<Result<Client, ClientError>>>;

/// The clients that may ask people to allow them: pley's own apps, and
/// other publishers' apps by their documents.
pub struct Documents {
    fetcher: Fetcher,
    cache: Mutex<HashMap<String, Cached>>,
    in_flight: Mutex<HashMap<String, Answer>>,
    /// When each person's fetches of the last minute started.
    turns: Mutex<HashMap<i64, VecDeque<Instant>>>,
    permits: Semaphore,
    /// Documents read so far ([`Documents::fetched`]).
    fetched: AtomicUsize,
}

impl Documents {
    pub fn new(source: Source) -> Self {
        Self::with_fetcher(match source {
            Source::Internet => Fetcher::Internet(http_client()),
            Source::Off => Fetcher::Off,
            Source::Fixed(documents) => Fetcher::Fixed(documents),
        })
    }

    fn with_fetcher(fetcher: Fetcher) -> Self {
        Self {
            fetcher,
            cache: Mutex::default(),
            in_flight: Mutex::default(),
            turns: Mutex::default(),
            permits: Semaphore::new(FETCHES_AT_ONCE),
            fetched: AtomicUsize::new(0),
        }
    }

    /// Documents answered by `source`, for unit tests.
    #[cfg(test)]
    fn with_test_source(
        source: impl Fn(String) -> futures::future::BoxFuture<'static, reqwest::Response> + Send + Sync + 'static,
    ) -> Self {
        Self::with_fetcher(Fetcher::Test(Box::new(source)))
    }

    /// The client `client_id` names, for `user_id` who is signed in and
    /// about to be asked to allow it, on a server whose address has the
    /// host `own_host`. In this order:
    /// 1. pley's own apps, never fetched;
    /// 2. nothing else unless `third_party_allowed`;
    /// 3. a valid client ID, on neither pley's own apps' host nor this
    ///    server's;
    /// 4. a document kept from before;
    /// 5. a fetch: one per client ID at a time (others wait for its
    ///    result), 4 at once, 10 a minute per person
    ///    ([`ClientError::Busy`]).
    pub async fn get(
        &self,
        client_id: &str,
        user_id: i64,
        third_party_allowed: bool,
        own_host: &str,
    ) -> Result<Client, ClientError> {
        if let Some(client) = clients::builtin(client_id) {
            return Ok(client.clone());
        }
        if !third_party_allowed {
            return Err(ClientError::NotAllowed);
        }
        if matches!(self.fetcher, Fetcher::Off) {
            return Err(ClientError::Off);
        }
        let url = clients::check_client_id(client_id)?;
        let host = url.host_str().unwrap_or_default();
        if clients::is_reserved_host(host) || same_host(host, own_host) {
            return Err(ClientError::Reserved);
        }
        loop {
            if let Some(client) = self.cached(client_id) {
                return Ok(client);
            }
            let waiting = self.in_flight.lock().get(client_id).cloned();
            if let Some(mut answer) = waiting {
                if let Ok(answer) = answer.wait_for(Option::is_some).await
                    && let Some(result) = (*answer).clone()
                {
                    return result;
                }
                // That fetch was dropped half way: try again.
                continue;
            }
            let sender = {
                let mut in_flight = self.in_flight.lock();
                if in_flight.contains_key(client_id) {
                    continue;
                }
                // The fetch that just ended kept its document.
                if let Some(client) = self.cached(client_id) {
                    return Ok(client);
                }
                self.take_turn(user_id)?;
                let (sender, answer) = watch::channel(None);
                in_flight.insert(client_id.to_owned(), answer);
                sender
            };
            return self.lead(client_id, url, sender).await;
        }
    }

    /// Fetches for everyone asking for `client_id` now.
    async fn lead(
        &self,
        client_id: &str,
        url: Url,
        sender: watch::Sender<Option<Result<Client, ClientError>>>,
    ) -> Result<Client, ClientError> {
        // Even when this request is dropped half way, the next may fetch.
        let _done = InFlight { in_flight: &self.in_flight, client_id };
        let result = self.fetch(client_id, url).await.map(|(client, lifetime)| {
            if let Some(lifetime) = lifetime {
                self.keep(client.clone(), lifetime);
            }
            client
        });
        if let Err(e) = &result {
            tracing::info!(client_id, "an app's description was refused: {e}");
        }
        sender.send_replace(Some(result.clone()));
        result
    }

    async fn fetch(&self, client_id: &str, url: Url) -> Result<(Client, Option<Duration>), ClientError> {
        let _permit = self.permits.acquire().await.map_err(|_| ClientError::Unreachable("Pley is stopping".into()))?;
        self.fetched.fetch_add(1, Ordering::Relaxed);
        let response = match &self.fetcher {
            // The address is the client ID, byte for byte.
            Fetcher::Internet(http) => http.get(url).send().await.map_err(failed)?,
            Fetcher::Fixed(documents) => fixed_answer(documents.get(client_id)),
            Fetcher::Off => return Err(ClientError::Off),
            #[cfg(test)]
            Fetcher::Test(source) => source(client_id.to_owned()).await,
        };
        let (body, lifetime) = read_document(response).await?;
        Ok((clients::parse_document(client_id, &body, false)?, lifetime))
    }

    /// Counts a fetch for `user_id`, unless they had their 10 this minute.
    fn take_turn(&self, user_id: i64) -> Result<(), ClientError> {
        let now = Instant::now();
        let mut turns = self.turns.lock();
        let mine = turns.entry(user_id).or_default();
        while mine.front().is_some_and(|&at| now.duration_since(at) >= MINUTE) {
            mine.pop_front();
        }
        if mine.len() >= FETCHES_PER_MINUTE {
            return Err(ClientError::Busy);
        }
        mine.push_back(now);
        Ok(())
    }

    fn cached(&self, client_id: &str) -> Option<Client> {
        let mut cache = self.cache.lock();
        let cached = cache.get(client_id)?;
        if cached.expires > Instant::now() {
            return Some(cached.client.clone());
        }
        // Fetched again at the next authorization (CIMD §5).
        cache.remove(client_id);
        None
    }

    /// Keeps a valid document, making room among the 256 by dropping the
    /// expired ones, or else the one that would expire first.
    fn keep(&self, client: Client, lifetime: Duration) {
        let now = Instant::now();
        let mut cache = self.cache.lock();
        if cache.len() >= CACHE_ENTRIES && !cache.contains_key(&client.id) {
            cache.retain(|_, cached| cached.expires > now);
            if cache.len() >= CACHE_ENTRIES {
                let soonest = cache.iter().min_by_key(|(_, cached)| cached.expires).map(|(id, _)| id.clone());
                if let Some(soonest) = soonest {
                    cache.remove(&soonest);
                }
            }
        }
        cache.insert(client.id.clone(), Cached { client, expires: now + lifetime });
    }

    /// Drops expired documents and counts older than a minute.
    pub fn sweep(&self) {
        let now = Instant::now();
        self.cache.lock().retain(|_, cached| cached.expires > now);
        self.turns.lock().retain(|_, turns| {
            while turns.front().is_some_and(|&at| now.duration_since(at) >= MINUTE) {
                turns.pop_front();
            }
            !turns.is_empty()
        });
    }

    /// How many documents were read since pley started: for tests that
    /// check none was.
    #[doc(hidden)]
    pub fn fetched(&self) -> usize {
        self.fetched.load(Ordering::Relaxed)
    }
}

/// Lets others fetch `client_id` again once its fetch is over, or dropped.
struct InFlight<'a> {
    in_flight: &'a Mutex<HashMap<String, Answer>>,
    client_id: &'a str,
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.in_flight.lock().remove(self.client_id);
    }
}

/// Whether `host` is the host of `own_host`, which may carry a port.
fn same_host(host: &str, own_host: &str) -> bool {
    let own = own_host.trim();
    let own = if own.starts_with('[') { own } else { own.rsplit_once(':').map_or(own, |(own, _)| own) };
    let own = own.strip_suffix('.').unwrap_or(own);
    host.eq_ignore_ascii_case(own)
}

#[cfg(test)]
mod tests {
    use futures::future::BoxFuture;
    use futures::{FutureExt, StreamExt};

    use super::*;
    use crate::oauth::clients::{ANDROID_ID, BUILTIN_IDS};

    const ID: &str = "https://apps.example.com/pley.json";
    const OWN: &str = "pley.example.com";

    fn document(id: &str) -> String {
        format!(r#"{{"client_id": "{id}", "redirect_uris": ["http://127.0.0.1/callback"]}}"#)
    }

    fn answer(
        status: u16,
        content_type: Option<&str>,
        headers: &[(&str, &str)],
        body: impl Into<reqwest::Body>,
    ) -> reqwest::Response {
        let mut builder = http::Response::builder().status(status);
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(body.into()).unwrap().into()
    }

    fn json(body: String) -> reqwest::Response {
        answer(200, Some("application/json"), &[], body)
    }

    fn cache_control(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_str(value).unwrap());
        headers
    }

    /// A body without Content-Length, in chunks.
    fn streamed(chunks: Vec<Vec<u8>>) -> reqwest::Body {
        reqwest::Body::wrap_stream(futures::stream::iter(chunks.into_iter().map(Ok::<_, std::io::Error>)))
    }

    /// Documents from a source that answers through `respond` and counts
    /// its fetches.
    fn counting(
        respond: impl Fn(String) -> BoxFuture<'static, reqwest::Response> + Send + Sync + 'static,
    ) -> (Arc<Documents>, Arc<AtomicUsize>) {
        let fetches = Arc::new(AtomicUsize::new(0));
        let counter = fetches.clone();
        let documents = Documents::with_test_source(move |id| {
            counter.fetch_add(1, Ordering::SeqCst);
            respond(id)
        });
        (Arc::new(documents), fetches)
    }

    /// A source answering every client ID with its valid document.
    fn valid() -> (Arc<Documents>, Arc<AtomicUsize>) {
        counting(|id| async move { json(document(&id)) }.boxed())
    }

    impl Documents {
        fn cached_for(&self, client_id: &str) -> Option<Duration> {
            self.cache.lock().get(client_id).map(|cached| cached.expires.saturating_duration_since(Instant::now()))
        }

        fn expire(&self, client_id: &str) {
            if let Some(cached) = self.cache.lock().get_mut(client_id) {
                cached.expires = Instant::now();
            }
        }
    }

    fn about(duration: Option<Duration>, secs: u64) -> bool {
        duration.is_some_and(|d| d <= Duration::from_secs(secs) && d > Duration::from_secs(secs - 10))
    }

    /// Lets tasks run until `done` holds, for at most 5 seconds.
    async fn until(done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done() {
            assert!(Instant::now() < deadline, "waited too long");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[test]
    fn only_public_addresses_are_fetched() {
        let refused = [
            "::ffff:127.0.0.1",
            "::ffff:1.1.1.1",
            "64:ff9b::a00:1",
            "64:ff9b:1::1",
            "100::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
            "5f00::1",
            "::1",
            "::",
            "2001::1",
            "2001:1ff::1",
            "2001:db8::1",
            "2002:a00:1::1",
            "3fff::1",
            "0.0.0.0",
            "0.1.2.3",
            "10.1.2.3",
            "100.64.0.1",
            "100.127.255.254",
            "127.0.0.1",
            "127.255.255.255",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
        ];
        for ip in refused {
            assert!(!is_public(ip.parse().unwrap()), "{ip} is refused");
        }
        let allowed = [
            "1.1.1.1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "198.17.255.255",
            "198.20.0.1",
            "223.255.255.255",
            "2606:4700:4700::1111",
            "2001:200::1",
            "2a00:1450:4001::1",
            "3ffe::1",
        ];
        for ip in allowed {
            assert!(is_public(ip.parse().unwrap()), "{ip} is allowed");
        }
    }

    #[tokio::test]
    async fn the_resolver_refuses_names_with_only_private_addresses() {
        for name in ["localhost", "127.0.0.1", "169.254.169.254", "::1"] {
            let resolved = PublicOnly.resolve(name.parse().unwrap()).await;
            assert!(resolved.is_err(), "{name} resolved to {:?}", resolved.map(|addrs| addrs.collect::<Vec<_>>()));
        }
        // A public address goes through, on port 443.
        let resolved: Vec<SocketAddr> = PublicOnly.resolve("1.1.1.1".parse().unwrap()).await.unwrap().collect();
        assert_eq!(resolved, ["1.1.1.1:443".parse::<SocketAddr>().unwrap()]);
    }

    #[tokio::test]
    async fn answers_must_be_small_json_with_200() {
        let (body, lifetime) = read_document(json(document(ID))).await.unwrap();
        assert_eq!(body, document(ID).as_bytes());
        assert_eq!(lifetime, Some(USUAL));
        for content_type in ["application/json; charset=utf-8", "Application/JSON", "application/cimd+json"] {
            assert!(read_document(answer(200, Some(content_type), &[], document(ID))).await.is_ok(), "{content_type}");
        }

        let location = [("location", "https://apps.example.com/other.json")];
        let refused = [
            ("302", answer(302, Some("application/json"), &location, document(ID))),
            ("301", answer(301, Some("application/json"), &location, document(ID))),
            ("404", answer(404, Some("application/json"), &[], document(ID))),
            ("201", answer(201, Some("application/json"), &[], document(ID))),
            ("203", answer(203, Some("application/json"), &[], document(ID))),
            ("HTML", answer(200, Some("text/html"), &[], document(ID))),
            ("text", answer(200, Some("text/plain"), &[], document(ID))),
            ("no type", answer(200, None, &[], document(ID))),
            ("not quite JSON", answer(200, Some("application/jsonp"), &[], document(ID))),
            ("+json alone", answer(200, Some("application/+json"), &[], document(ID))),
            // Content-Length is believed when it says too much…
            (
                "Content-Length 5,121",
                answer(
                    200,
                    Some("application/json"),
                    &[("content-length", "5121")],
                    streamed(vec![document(ID).into_bytes()]),
                ),
            ),
            // …and the body is counted when it says nothing.
            (
                "5,121 bytes",
                answer(200, Some("application/json"), &[], streamed(vec![vec![b' '; 5_000], vec![b' '; 121]])),
            ),
        ];
        for (what, response) in refused {
            assert!(read_document(response).await.is_err(), "{what} is refused");
        }
        let at_most = answer(200, Some("application/json"), &[], streamed(vec![vec![b' '; 5_000], vec![b' '; 120]]));
        assert_eq!(read_document(at_most).await.unwrap().0.len(), 5_120, "5,120 bytes are fine");

        // Reading stops at the limit: this body never ends.
        let endless = futures::stream::iter(0..).map(|i: u32| {
            assert!(i < 10, "read past the limit");
            Ok::<_, std::io::Error>(vec![b' '; 1024])
        });
        let endless = answer(200, Some("application/json"), &[], reqwest::Body::wrap_stream(endless));
        assert!(read_document(endless).await.is_err());

        // Then the document must be valid JSON (and a valid document).
        let (documents, _) = counting(|_| async { json("{\"client_id\": ".into()) }.boxed());
        assert!(matches!(documents.get(ID, 1, true, OWN).await, Err(ClientError::Document(_))));
    }

    #[tokio::test]
    async fn built_in_apps_are_never_fetched() {
        let documents = Documents::with_test_source(|id| panic!("{id} was fetched"));
        for allowed in [false, true] {
            for id in BUILTIN_IDS {
                // Even on a server reached at their host.
                for own in [OWN, "pley-media-server.github.io"] {
                    let client = documents.get(id, 1, allowed, own).await.unwrap();
                    assert_eq!(client.id, *id);
                    assert!(client.first_party);
                }
            }
        }
        // And without any way of fetching.
        let off = Documents::new(Source::Off);
        assert!(off.get(ANDROID_ID, 1, false, OWN).await.unwrap().first_party);
        assert_eq!(off.get(ID, 1, true, OWN).await, Err(ClientError::Off));
        assert_eq!(off.get(ID, 1, false, OWN).await, Err(ClientError::NotAllowed));
    }

    #[test]
    fn lifetimes_come_from_cache_control_within_bounds() {
        let lifetime_of = |value: &str| lifetime(&cache_control(value));
        assert_eq!(lifetime(&HeaderMap::new()), Some(USUAL));
        assert_eq!(lifetime_of("max-age=10"), Some(SHORTEST));
        assert_eq!(lifetime_of("max-age=0"), Some(SHORTEST));
        assert_eq!(lifetime_of("max-age=1000000"), Some(LONGEST));
        assert_eq!(lifetime_of("max-age=99999999999999999999999"), Some(LONGEST));
        assert_eq!(lifetime_of("public, max-age=600"), Some(Duration::from_secs(600)));
        assert_eq!(lifetime_of("MAX-AGE=\"600\""), Some(Duration::from_secs(600)));
        assert_eq!(lifetime_of("max-age=600, max-age=7200"), Some(Duration::from_secs(600)), "the first one counts");
        assert_eq!(lifetime_of("max-age=-5"), Some(USUAL));
        assert_eq!(lifetime_of("max-age=ten"), Some(USUAL));
        assert_eq!(lifetime_of("no-store"), None);
        assert_eq!(lifetime_of("max-age=600, No-Store"), None);
        assert_eq!(lifetime_of("no-cache"), None);
    }

    #[tokio::test]
    async fn documents_are_cached_within_bounds_and_failures_never() {
        // Each client ID's file name says how it is answered.
        let answered = Arc::new(AtomicUsize::new(0));
        let count = answered.clone();
        let (documents, fetches) = counting(move |id| {
            let count = count.clone();
            async move {
                let headers: &[(&str, &str)] = match id.rsplit('/').next().unwrap() {
                    "short.json" => &[("cache-control", "max-age=10")],
                    "long.json" => &[("cache-control", "max-age=1000000")],
                    "never.json" => &[("cache-control", "no-store")],
                    // The first fetch fails, the next ones work.
                    "flaky.json" if count.fetch_add(1, Ordering::SeqCst) == 0 => {
                        return answer(500, Some("application/json"), &[], document(&id));
                    }
                    // The first answer isn't valid, the next ones are.
                    "invalid.json" if count.fetch_add(1, Ordering::SeqCst) == 2 => {
                        return json(document("https://apps.example.com/other.json"));
                    }
                    _ => &[],
                };
                answer(200, Some("application/json"), headers, document(&id))
            }
            .boxed()
        });
        let fetched = || fetches.load(Ordering::SeqCst);
        let url = |name: &str| format!("https://apps.example.com/{name}");
        let get = |name: &str| {
            let documents = documents.clone();
            let id = url(name);
            async move { documents.get(&id, 1, true, OWN).await }
        };

        get("short.json").await.unwrap();
        assert!(about(documents.cached_for(&url("short.json")), 300), "max-age 10 → 5 minutes");
        get("short.json").await.unwrap();
        assert_eq!(fetched(), 1, "kept");
        get("long.json").await.unwrap();
        assert!(about(documents.cached_for(&url("long.json")), 86_400), "max-age 10^6 → 24 hours");
        get("usual.json").await.unwrap();
        assert!(about(documents.cached_for(&url("usual.json")), 3_600), "an hour by default");
        assert_eq!(fetched(), 3);

        get("never.json").await.unwrap();
        assert_eq!(documents.cached_for(&url("never.json")), None, "no-store isn't kept");
        get("never.json").await.unwrap();
        assert_eq!(fetched(), 5);

        assert!(matches!(get("flaky.json").await, Err(ClientError::Unreachable(_))));
        assert_eq!(documents.cached_for(&url("flaky.json")), None, "failures aren't kept");
        get("flaky.json").await.unwrap();
        get("flaky.json").await.unwrap();
        assert_eq!(fetched(), 7, "an error, then a success that is kept");

        assert!(matches!(get("invalid.json").await, Err(ClientError::Document(_))));
        get("invalid.json").await.unwrap();
        assert_eq!(fetched(), 9, "invalid documents aren't kept");

        // An expired document is fetched again; the sweep drops it.
        documents.expire(&url("short.json"));
        get("short.json").await.unwrap();
        assert_eq!(fetched(), 10);
        documents.expire(&url("long.json"));
        documents.sweep();
        assert!(!documents.cache.lock().contains_key(&url("long.json")), "swept");
        assert!(documents.cached_for(&url("usual.json")).is_some());
    }

    #[tokio::test]
    async fn the_cache_keeps_256_documents_at_most() {
        let (documents, fetches) = valid();
        for i in 0..300 {
            // 30 people, each within their 10 a minute.
            let id = format!("https://apps.example.com/{i}.json");
            documents.get(&id, i / 10, true, OWN).await.unwrap();
            assert!(documents.cache.lock().len() <= CACHE_ENTRIES);
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 300);
        assert_eq!(documents.cache.lock().len(), CACHE_ENTRIES);
        assert!(documents.cached_for("https://apps.example.com/299.json").is_some(), "the newest is kept");
        assert!(documents.cached_for("https://apps.example.com/0.json").is_none(), "the oldest went");
    }

    #[tokio::test]
    async fn fetches_are_limited() {
        // One fetch per client ID at a time: the others wait for its
        // result, which isn't even kept.
        let gate = Arc::new(Semaphore::new(0));
        let held = gate.clone();
        let (documents, fetches) = counting(move |id| {
            let held = held.clone();
            async move {
                held.acquire().await.unwrap().forget();
                answer(200, Some("application/json"), &[("cache-control", "no-store")], document(&id))
            }
            .boxed()
        });
        let waiting: Vec<_> = (0..5)
            .map(|user| {
                let documents = documents.clone();
                tokio::spawn(async move { documents.get(ID, user, true, OWN).await })
            })
            .collect();
        until(|| fetches.load(Ordering::SeqCst) == 1).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        gate.add_permits(5);
        for task in waiting {
            assert_eq!(task.await.unwrap().unwrap().id, ID);
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1, "one fetch answered all five");
        assert!(documents.in_flight.lock().is_empty());

        // 4 at once in the server.
        let gate = Arc::new(Semaphore::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let most = Arc::new(AtomicUsize::new(0));
        let (held, now, peak) = (gate.clone(), active.clone(), most.clone());
        let (documents, fetches) = counting(move |id| {
            let (held, now, peak) = (held.clone(), now.clone(), peak.clone());
            async move {
                peak.fetch_max(now.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                held.acquire().await.unwrap().forget();
                now.fetch_sub(1, Ordering::SeqCst);
                json(document(&id))
            }
            .boxed()
        });
        let waiting: Vec<_> = (0..6)
            .map(|i| {
                let documents = documents.clone();
                let id = format!("https://apps.example.com/{i}.json");
                tokio::spawn(async move { documents.get(&id, 1, true, OWN).await })
            })
            .collect();
        until(|| active.load(Ordering::SeqCst) == FETCHES_AT_ONCE).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(active.load(Ordering::SeqCst), FETCHES_AT_ONCE);
        gate.add_permits(6);
        for task in waiting {
            task.await.unwrap().unwrap();
        }
        assert_eq!(most.load(Ordering::SeqCst), FETCHES_AT_ONCE);
        assert_eq!(fetches.load(Ordering::SeqCst), 6);

        // 10 a minute per person.
        let (documents, fetches) = valid();
        for i in 0..10 {
            documents.get(&format!("https://apps.example.com/{i}.json"), 1, true, OWN).await.unwrap();
        }
        let eleventh = documents.get("https://apps.example.com/10.json", 1, true, OWN).await;
        assert_eq!(eleventh, Err(ClientError::Busy));
        assert_eq!(fetches.load(Ordering::SeqCst), 10);
        documents.sweep();
        assert_eq!(documents.get("https://apps.example.com/10.json", 1, true, OWN).await, Err(ClientError::Busy));
        // Kept documents and pley's own apps need no fetch; others have
        // turns of their own.
        documents.get("https://apps.example.com/3.json", 1, true, OWN).await.unwrap();
        documents.get(ANDROID_ID, 1, true, OWN).await.unwrap();
        documents.get("https://apps.example.com/10.json", 2, true, OWN).await.unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 11);
        // A minute later the turns are back, and the sweep forgets them.
        for turn in documents.turns.lock().get_mut(&1).unwrap() {
            *turn -= MINUTE;
        }
        documents.sweep();
        assert!(!documents.turns.lock().contains_key(&1), "old counts are swept");
        documents.get("https://apps.example.com/11.json", 1, true, OWN).await.unwrap();
    }

    #[tokio::test]
    async fn a_fetch_dropped_half_way_lets_the_next_one_fetch() {
        let gate = Arc::new(Semaphore::new(0));
        let held = gate.clone();
        let (documents, fetches) = counting(move |id| {
            let held = held.clone();
            async move {
                held.acquire().await.unwrap().forget();
                json(document(&id))
            }
            .boxed()
        });
        let first = documents.clone();
        let first = tokio::spawn(async move { first.get(ID, 1, true, OWN).await });
        until(|| fetches.load(Ordering::SeqCst) == 1).await;
        let second = documents.clone();
        let second = tokio::spawn(async move { second.get(ID, 2, true, OWN).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        first.abort();
        until(|| fetches.load(Ordering::SeqCst) == 2).await;
        gate.add_permits(1);
        assert_eq!(second.await.unwrap().unwrap().id, ID);
    }

    #[tokio::test]
    async fn apps_by_other_publishers_need_the_setting() {
        let (documents, fetches) = valid();
        assert_eq!(documents.get(ID, 1, false, OWN).await, Err(ClientError::NotAllowed));
        assert_eq!(documents.get("not a client id", 1, false, OWN).await, Err(ClientError::NotAllowed));
        assert_eq!(fetches.load(Ordering::SeqCst), 0);
        let client = documents.get(ID, 1, true, OWN).await.unwrap();
        assert!(!client.first_party, "fetched documents never make pley's own apps");
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        // A kept document is refused too once the setting is off.
        assert_eq!(documents.get(ID, 1, false, OWN).await, Err(ClientError::NotAllowed));
    }

    #[tokio::test]
    async fn own_and_built_in_hosts_are_refused() {
        let (documents, fetches) = valid();
        for (id, own) in [
            ("https://pley-media-server.github.io/pley/oauth/other.json", OWN),
            ("https://pley-media-server.github.io/x.json", OWN),
            ("https://pley.example.com/app.json", OWN),
            ("https://pley.example.com/app.json", "PLEY.example.com:32400"),
            ("https://pley.example.com/app.json", "pley.example.com."),
        ] {
            assert_eq!(documents.get(id, 1, true, own).await, Err(ClientError::Reserved), "{id} on {own}");
        }
        for id in ["http://apps.example.com/pley.json", "https://127.0.0.1/pley.json", "https://apps.example.com/"] {
            assert!(matches!(documents.get(id, 1, true, OWN).await, Err(ClientError::ClientId(_))), "{id}");
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 0);
        assert!(documents.get("https://pley.example.com/app.json", 1, true, "other.example.com").await.is_ok());
        assert!(documents.get(ID, 1, true, "192.168.1.5:32400").await.is_ok());
        assert!(documents.get(ID, 1, true, "[::1]:32400").await.is_ok());
    }

    #[test]
    fn hosts_are_compared_without_port_or_case() {
        assert!(same_host("pley.example.com", "pley.example.com"));
        assert!(same_host("pley.example.com", "Pley.Example.COM:443"));
        assert!(same_host("pley.example.com", "pley.example.com.:8080"));
        assert!(!same_host("pley.example.com", "example.com"));
        assert!(!same_host("pley.example.com", "[::1]:8080"));
    }

    #[tokio::test]
    async fn fixed_documents_are_answered_like_a_server_would() {
        let documents = Documents::new(Source::Fixed(HashMap::from([(ID.to_owned(), document(ID))])));
        let client = documents.get(ID, 1, true, OWN).await.unwrap();
        assert_eq!(client.redirect_uris, ["http://127.0.0.1/callback"]);
        assert!(!client.first_party);
        assert!(about(documents.cached_for(ID), 3_600), "no cache headers: an hour");
        let unknown = documents.get("https://apps.example.com/unknown.json", 1, true, OWN).await;
        assert!(matches!(unknown, Err(ClientError::Unreachable(_))), "{unknown:?}");
        // Each was read once: kept documents and pley's own apps never are.
        documents.get(ID, 1, true, OWN).await.unwrap();
        documents.get(ANDROID_ID, 1, true, OWN).await.unwrap();
        assert_eq!(documents.fetched(), 2);
    }

    #[tokio::test]
    async fn lookups_can_be_awaited_anywhere() {
        fn send<T: Send>(_: &T) {}
        // The real client builds, too.
        let documents = Documents::new(Source::Internet);
        let lookup = documents.get(ANDROID_ID, 1, true, OWN);
        send(&lookup);
        assert!(lookup.await.unwrap().first_party);
    }

    #[tokio::test]
    async fn the_documents_client_resolves_through_the_filter() {
        // Something listening on this computer, under a name with only
        // private addresses: the resolver refuses the name, before any
        // connection is tried.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        tokio::spawn(async move {
            while listener.accept().await.is_ok() {
                count.fetch_add(1, Ordering::SeqCst);
            }
        });
        let http = http_client();
        let err = http.get(format!("https://localhost:{port}/pley.json")).send().await.unwrap_err();
        let chain: Vec<String> = std::iter::successors(Some(&err as &dyn std::error::Error), |e| e.source())
            .map(ToString::to_string)
            .collect();
        assert!(chain.iter().any(|e| e.contains("has no public address")), "{chain:?}");
        assert_eq!(accepted.load(Ordering::SeqCst), 0, "connected to a loopback address");
        // For good, and said so: trying again later wouldn't help.
        assert!(matches!(failed(err), ClientError::Private));
        // https only, refused before any lookup.
        let err = http.get("http://apps.example.com/pley.json").send().await.unwrap_err();
        assert!(err.is_builder(), "{err:?}");
    }
}
