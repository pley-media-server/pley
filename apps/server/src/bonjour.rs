//! Bonjour: beside [`crate::discovery`]'s UDP answer, the server announces
//! itself on the local network by mDNS and DNS-SD as `_pley._tcp`, so apps
//! that may not broadcast (Apple's, without the multicast entitlement) find
//! it with the system's own browser. The announcement carries the UDP
//! answer's fields in its TXT records, and tells nothing more. It has a
//! switch of its own (Settings → Network, `--no-bonjour`; `--no-discovery`
//! turns it off too), goes out only from the IPv4 adapters on the local
//! network that the web app listens on (100.64.0.0/10's only when chosen),
//! or those of them chosen (a server
//! on a VPN, a container or virtual machine network or a guest Wi-Fi as
//! well need not be announced there), follows renames, and says goodbye
//! when the server stops.
//!
//! The responder is Pley's own, and answers for Pley's names only. It keeps
//! nothing of what other devices send (a general mDNS library caches the
//! answers it hears, which any device on the network could grow without
//! end), answers only devices on the networks it announces on, so that a
//! forged source address can't turn it on someone elsewhere, multicasts an
//! answer once a second at most on each network (RFC 6762 §6), and tells
//! each network Pley's address on it only. It shares UDP port 5353 with the
//! system's own responder (mDNSResponder, Avahi, Windows').

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use socket2::{Domain, Protocol, SockRef, Socket, Type};
use tokio::net::UdpSocket;
use tokio::time::Instant;

use crate::mdns::{self, Data, MAX_LABEL, Name, Packet, Question, Record};
use crate::network::Listen;
use crate::state::AppState;

/// mDNS's group and port.
const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const PORT: u16 = 5353;
/// The service type apps browse for, `_pley._tcp.local`.
const SERVICE: [&str; 3] = ["_pley", "_tcp", "local"];
/// The service types' list (RFC 6763 §9), which Bonjour tools browse.
const SERVICES: [&str; 4] = ["_services", "_dns-sd", "_udp", "local"];
/// How long the host's address and the service's port are kept by others,
/// and the rest (RFC 6762 §10).
const HOST_TTL: u32 = 120;
const OTHER_TTL: u32 = 4500;
/// An adapter's name's length at most, in bytes: Windows' are 256 UTF-16
/// characters at most, the others' far fewer.
const MAX_ADAPTER_NAME: usize = 256;
/// A TXT string's length at most, `name=` included.
const MAX_TXT: usize = 255;
/// A server name's length at most, as Settings take it (the default, the
/// computer's name, isn't checked there).
const MAX_NAME_CHARS: usize = 64;
/// What a read takes: any UDP datagram, so none is too big to read (Windows
/// fails the read of one larger than the buffer).
const READ_BUFFER: usize = 65_536;
/// Answers sent by unicast in a second, at most, whoever asks: a device
/// can't make Pley send more than this at a forged address.
const MAX_UNICAST_PER_SECOND: u32 = 100;
/// A probe's wait for another device's answer, three times (RFC 6762 §8.1).
const PROBE_WAIT: Duration = Duration::from_millis(250);
/// The second announcement follows the first (RFC 6762 §8.3).
const ANNOUNCE_AGAIN: Duration = Duration::from_secs(1);
/// How often the adapters are read again: one that comes later (Wi-Fi, a
/// VPN) is announced on too.
const RESCAN: Duration = Duration::from_secs(30);
/// A legacy querier's questions answered, at most (each gets a packet).
const MAX_LEGACY_ANSWERS: usize = 4;

/// Where the server is announced.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Plan {
    /// The web port.
    port: u16,
    /// The web addresses with that port: the announcement goes out from the
    /// adapters with these addresses, or for an unspecified one, from those
    /// it listens on (see [`announced_on`]).
    addresses: Vec<IpAddr>,
    /// The adapters chosen, by name: any when empty.
    adapters: Vec<String>,
    /// The host name chosen, a DNS label: the generated one when empty.
    host: String,
}

/// Where `listen` has the server announced: from the web addresses other
/// devices can reach, like discovery, on the first one's port (an
/// announcement has a single port). `None` with Bonjour off.
fn plan(listen: &Listen) -> Option<Plan> {
    if !listen.bonjour {
        return None;
    }
    let web = listen.web.iter().filter(|addr| !addr.ip().is_loopback() && addr.port() != 0);
    let port = web.clone().next()?.port();
    Some(Plan {
        port,
        addresses: web.filter(|addr| addr.port() == port).map(|addr| addr.ip()).collect(),
        adapters: listen.bonjour_adapters.clone(),
        host: listen.bonjour_host_name.clone(),
    })
}

/// Whether the announcement goes out from the address `ip` of the adapter
/// named `adapter`: one on the local network that the web app listens on,
/// among the adapters chosen, if some are.
fn announced_on(plan: &Plan, adapter: &str, ip: IpAddr) -> bool {
    let chosen = plan.adapters.iter().any(|chosen| chosen == adapter);
    !ip.is_loopback()
        && crate::setup::is_local_address(ip)
        // 100.64.0.0/10 is Tailscale's, which carries no multicast, and
        // carriers' shared addresses, other customers' network: only when
        // chosen.
        && (chosen || (plan.adapters.is_empty() && !shared_address(ip)))
        && plan.addresses.iter().any(|web| *web == ip || (web.is_unspecified() && listens_on_family(*web, ip)))
}

/// Whether `ip` is in 100.64.0.0/10 (RFC 6598).
fn shared_address(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if crate::setup::is_shared_address(v4))
}

/// Whether `name` may be a network adapter's: not blank, bounded, and
/// without control characters (it is written in `pley.toml`).
pub fn adapter_name_ok(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= MAX_ADAPTER_NAME && !name.chars().any(char::is_control)
}

/// Whether a socket bound to the unspecified address `web` takes `ip`'s
/// family. `0.0.0.0` takes IPv4 only. Pley leaves `::` as the system has it:
/// dual-stack on Linux and macOS, so IPv4 too, and IPv6 only on Windows.
fn listens_on_family(web: IpAddr, ip: IpAddr) -> bool {
    web.is_ipv4() == ip.is_ipv4() || (web.is_ipv6() && cfg!(not(windows)))
}

/// `text`'s first `max` bytes at most, whole characters.
fn bounded(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The server's name as announced: as Settings bound it, without control
/// characters.
fn shown_name(name: &str) -> String {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    if name.is_empty() { "Pley".into() } else { name }
}

/// The instance name: the server's name, in a DNS label; with ` (n)` after
/// it when another device has it (`n` from 2).
fn instance_name(name: &str, n: u32) -> String {
    let suffix = if n > 1 { format!(" ({n})") } else { String::new() };
    let name = shown_name(name);
    format!("{}{suffix}", bounded(&name, MAX_LABEL - suffix.len()).trim_end())
}

/// The TXT records: the UDP answer's fields, each in one TXT string.
fn txt(id: &str, name: &str) -> Vec<String> {
    let name = shown_name(name);
    vec![
        "pley=1".into(),
        format!("id={}", bounded(id, MAX_TXT - "id=".len())),
        format!("name={}", bounded(&name, MAX_TXT - "name=".len())),
        format!("version={}", env!("CARGO_PKG_VERSION")),
    ]
}

/// The host name the announcement's addresses go by, unless one is chosen:
/// the server's own, `pley-` and its id's start, not the computer's, which
/// the system's responder answers for.
pub fn generated_host(id: &str) -> String {
    let short: String = id.chars().filter(char::is_ascii_alphanumeric).take(12).collect();
    format!("pley-{}", short.to_lowercase())
}

fn host_name(label: &str) -> Name {
    vec![label.to_owned(), "local".into()]
}

/// A host name as typed (`Media`, `media.local`) as it is kept: its
/// label, lowercase. Empty stays empty, for the generated one.
pub fn host_label(typed: &str) -> String {
    let lower = typed.trim().trim_end_matches('.').to_ascii_lowercase();
    lower.strip_suffix(".local").unwrap_or(&lower).to_owned()
}

/// Whether `label` may be the host name: empty (the generated one), or one
/// DNS label of letters, digits and hyphens, not starting or ending with a
/// hyphen (RFC 952 and 1123), as every resolver takes it.
pub fn host_label_ok(label: &str) -> bool {
    label.is_empty()
        || (label.len() <= MAX_LABEL
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-'))
}

fn labels(fixed: &[&str]) -> Name {
    fixed.iter().map(|&label| label.to_owned()).collect()
}

/// An adapter's IPv4 address the announcement goes out from.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Link {
    adapter: String,
    ip: Ipv4Addr,
    mask: Ipv4Addr,
}

impl Link {
    /// Whether `ip` is on this link's network: only those devices are
    /// answered.
    fn reaches(&self, ip: Ipv4Addr) -> bool {
        u32::from(ip) & u32::from(self.mask) == u32::from(self.ip) & u32::from(self.mask)
    }
}

/// The links `plan` announces from, among this computer's adapters now.
fn links(plan: &Plan) -> Vec<Link> {
    let mut links: Vec<Link> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|intf| match intf.addr {
            if_addrs::IfAddr::V4(v4) if announced_on(plan, &intf.name, IpAddr::V4(v4.ip)) => {
                Some(Link { adapter: intf.name, ip: v4.ip, mask: v4.netmask })
            }
            _ => None,
        })
        .collect();
    links.dedup_by(|a, b| a.ip == b.ip);
    links
}

/// What Pley answers for, as `link` has it.
struct Records {
    service: Name,
    instance: Name,
    host: Name,
    ptr: Record,
    srv: Record,
    txt: Record,
    a: Record,
    services: Record,
}

impl Records {
    fn new(instance: &str, host: &Name, port: u16, txt: Vec<String>, ip: Ipv4Addr, ttl: Option<u32>) -> Self {
        let service = labels(&SERVICE);
        let instance: Name = std::iter::once(instance.to_owned()).chain(service.iter().cloned()).collect();
        let record =
            |name: &Name, data, normal, unique| Record { name: name.clone(), data, ttl: ttl.unwrap_or(normal), unique };
        Self {
            ptr: record(&service, Data::Ptr(instance.clone()), OTHER_TTL, false),
            srv: record(&instance, Data::Srv { port, target: host.clone() }, HOST_TTL, true),
            txt: record(&instance, Data::Txt(txt), OTHER_TTL, true),
            a: record(host, Data::A(ip), HOST_TTL, true),
            services: record(&labels(&SERVICES), Data::Ptr(service.clone()), OTHER_TTL, false),
            service,
            instance,
            host: host.clone(),
        }
    }

    /// Every record, as announcements carry them.
    fn all(&self) -> Vec<Record> {
        vec![self.ptr.clone(), self.srv.clone(), self.txt.clone(), self.a.clone(), self.services.clone()]
    }

    /// The answers to `question`, and the records that go with them.
    fn answer(&self, question: &Question) -> (Vec<Record>, Vec<Record>) {
        let any = question.kind == mdns::TYPE_ANY;
        let asks = |kind| any || question.kind == kind;
        let name = &question.name;
        if mdns::same(name, &self.service) && asks(mdns::TYPE_PTR) {
            return (vec![self.ptr.clone()], vec![self.srv.clone(), self.txt.clone(), self.a.clone()]);
        }
        if mdns::same(name, &self.services.name) && asks(mdns::TYPE_PTR) {
            return (vec![self.services.clone()], Vec::new());
        }
        if mdns::same(name, &self.instance) {
            let mut answers = Vec::new();
            if asks(mdns::TYPE_SRV) {
                answers.push(self.srv.clone());
            }
            if asks(mdns::TYPE_TXT) {
                answers.push(self.txt.clone());
            }
            let extra = if answers.iter().any(|r| r.data == self.srv.data) { vec![self.a.clone()] } else { Vec::new() };
            return (answers, extra);
        }
        if mdns::same(name, &self.host) && asks(mdns::TYPE_A) {
            return (vec![self.a.clone()], Vec::new());
        }
        (Vec::new(), Vec::new())
    }
}

/// The answers to `questions`, each record once, and what goes with them.
fn answers(records: &Records, questions: &[Question]) -> (Vec<Record>, Vec<Record>) {
    let mut answers: Vec<Record> = Vec::new();
    let mut extra: Vec<Record> = Vec::new();
    for question in questions {
        let (a, e) = records.answer(question);
        for record in a {
            if !answers.contains(&record) {
                answers.push(record);
            }
        }
        for record in e {
            if !extra.contains(&record) {
                extra.push(record);
            }
        }
    }
    extra.retain(|record| !answers.contains(record));
    (answers, extra)
}

/// The responder, announcing as a [`Plan`] says.
struct Responder {
    socket: UdpSocket,
    plan: Plan,
    id: String,
    host: Name,
    links: Vec<Link>,
    /// Whether `host` is known not to be another device's: the generated
    /// one is unique, a chosen one is probed for first.
    host_checked: bool,
    /// The server's name it stands for, and its instance label.
    announced: Option<(String, String)>,
    /// Whether the names were asked for on a network: with none to ask
    /// on, they are once one comes.
    probed: bool,
    /// Said in the log that none of the adapters chosen is here.
    said_none: bool,
    /// When the current second of unicast answers began, and how many went.
    unicast: (Instant, u32),
    /// When to announce again (the second announcement).
    again: Option<Instant>,
    /// When each answer last went out by multicast, by link and question.
    multicast: HashMap<(Ipv4Addr, String), Instant>,
}

impl Responder {
    fn start(plan: Plan, id: &str) -> std::io::Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        // Beside the system's own responder, which has the port too.
        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)).into())?;
        socket.set_multicast_ttl_v4(255)?;
        socket.set_multicast_loop_v4(true)?;
        // Only the groups this socket joined, on the adapters chosen; Linux
        // otherwise also delivers those other programs joined elsewhere.
        #[cfg(target_os = "linux")]
        if let Err(err) = socket.set_multicast_all_v4(false) {
            tracing::debug!("Bonjour hears other programs' groups too: {err}");
        }
        socket.set_nonblocking(true)?;
        let socket = UdpSocket::from_std(socket.into())?;
        let chosen = !plan.host.is_empty();
        let host = host_name(&if chosen { plan.host.clone() } else { generated_host(id) });
        let mut responder = Self {
            socket,
            plan,
            id: id.to_owned(),
            host,
            links: Vec::new(),
            host_checked: !chosen,
            announced: None,
            probed: false,
            said_none: false,
            unicast: (Instant::now(), 0),
            again: None,
            multicast: HashMap::new(),
        };
        responder.links = responder.joined(links(&responder.plan));
        responder.said_where();
        Ok(responder)
    }

    /// `links` with the mDNS group joined on each; those it can't be joined on are left out.
    fn joined(&self, links: Vec<Link>) -> Vec<Link> {
        let socket = SockRef::from(&self.socket);
        links
            .into_iter()
            .filter(|link| match socket.join_multicast_v4(&GROUP, &link.ip) {
                Ok(()) => true,
                // Joined already: the link was there before.
                Err(_) if self.links.iter().any(|l| l.ip == link.ip) => true,
                Err(err) => {
                    tracing::debug!("Bonjour can't listen on {} ({}): {err}", link.ip, link.adapter);
                    false
                }
            })
            .collect()
    }

    /// Says in the log, once until one comes, when adapters are chosen and
    /// none of them is here.
    fn said_where(&mut self) {
        if !self.links.is_empty() || self.plan.adapters.is_empty() {
            self.said_none = false;
        } else if !self.said_none {
            self.said_none = true;
            tracing::warn!(
                "Bonjour announces Pley on {} only, and none of them is on this computer now",
                self.plan.adapters.join(", ")
            );
        }
    }

    /// Whether another answer may go by unicast this second.
    fn may_unicast(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.unicast.0) >= Duration::from_secs(1) {
            self.unicast = (now, 0);
        }
        self.unicast.1 += 1;
        self.unicast.1 <= MAX_UNICAST_PER_SECOND
    }

    fn records(&self, link: &Link, instance: &str, name: &str, ttl: Option<u32>) -> Records {
        Records::new(instance, &self.host, self.plan.port, txt(&self.id, name), link.ip, ttl)
    }

    async fn send(&self, link: &Link, packet: &[u8], to: SocketAddr) {
        if to.ip() == IpAddr::V4(GROUP)
            && let Err(err) = SockRef::from(&self.socket).set_multicast_if_v4(&link.ip)
        {
            tracing::debug!("Bonjour can't send on {}: {err}", link.ip);
            return;
        }
        if let Err(err) = self.socket.send_to(packet, to).await {
            tracing::debug!("Bonjour can't send on {}: {err}", link.ip);
        }
    }

    /// Announces every record on `links`, or says goodbye with a TTL of 0.
    async fn announce_on(&self, links: &[Link], goodbye: bool) {
        let Some((name, instance)) = &self.announced else { return };
        for link in links {
            let records = self.records(link, instance, name, goodbye.then_some(0)).all();
            self.send(link, &mdns::write(&records, &[], None), SocketAddrV4::new(GROUP, PORT).into()).await;
        }
    }

    /// Announces the server as `name`, unless it already is. The new names
    /// are asked for first (the old one's goodbye, heard back, would
    /// otherwise pass for another device's), then the old one says goodbye.
    /// With no network to ask on, nothing is claimed until one comes.
    async fn announce(&mut self, name: &str) {
        if self.probed && self.announced.as_ref().is_some_and(|(announced, _)| announced == name) {
            return;
        }
        if self.links.is_empty() {
            // A name claimed on a network stays, so that the rename, once
            // a network comes, says goodbye to it.
            if !self.probed {
                self.announced = Some((name.to_owned(), instance_name(name, 1)));
            }
            return;
        }
        let old = if self.probed { self.announced.as_ref().map(|(_, label)| label.clone()) } else { None };
        if !self.host_checked {
            self.host_checked = true;
            if self.taken(&self.host, mdns::TYPE_A, &self.links).await {
                let generated = host_name(&generated_host(&self.id));
                tracing::warn!(
                    "another device on the network is {}: Bonjour gives Pley's addresses as {} instead",
                    self.host.join("."),
                    generated.join(".")
                );
                self.host = generated;
            }
        }
        let mut n = 1;
        let instance = loop {
            let instance = instance_name(name, n);
            // The label Pley has already, but for its case (a rename that
            // changes only that, or a long name's end): its own.
            if old.as_ref().is_some_and(|label| label.eq_ignore_ascii_case(&instance)) {
                break instance;
            }
            let service: Name = std::iter::once(instance.clone()).chain(labels(&SERVICE)).collect();
            if n >= 9 || !self.taken(&service, mdns::TYPE_SRV, &self.links).await {
                break instance;
            }
            n += 1;
        };
        if n > 1 {
            tracing::info!(
                "another server on the network is {}: Pley is announced as {instance}",
                instance_name(name, 1)
            );
        }
        // The same label in another case replaces its records as they are
        // (cache-flush); another one says goodbye first.
        if old.as_ref().is_some_and(|label| !label.eq_ignore_ascii_case(&instance)) {
            self.announce_on(&self.links.clone(), true).await;
        }
        self.announced = Some((name.to_owned(), instance));
        self.probed = true;
        self.announce_on(&self.links.clone(), false).await;
        self.again = Some(Instant::now() + ANNOUNCE_AGAIN);
        tracing::debug!("announced by Bonjour as {} on port {}", self.announced.as_ref().unwrap().1, self.plan.port);
    }

    /// Whether another device answers for `name`, an instance or a host (a
    /// probe, RFC 6762 §8.1). It asks for any record, as the RFC has it, and
    /// for `kind` too (SRV, A), which some responders answer when they don't
    /// answer "any" for a name. What is heard meanwhile is otherwise
    /// dropped: browsers ask again.
    async fn taken(&self, name: &Name, kind: u16, links: &[Link]) -> bool {
        let queries = [mdns::write_query(name, mdns::TYPE_ANY), mdns::write_query(name, kind)];
        let mut buf = vec![0; READ_BUFFER];
        for _ in 0..3 {
            for link in links {
                for query in &queries {
                    self.send(link, query, SocketAddrV4::new(GROUP, PORT).into()).await;
                }
            }
            let until = Instant::now() + PROBE_WAIT;
            let mut errors = 0;
            // The deadline is checked each time round: a flood that never
            // leaves the socket empty would otherwise keep the wait going.
            while Instant::now() < until && errors < 10 {
                match tokio::time::timeout_at(until, self.socket.recv_from(&mut buf)).await {
                    Err(_) => break,
                    // Windows' report of an unreachable earlier destination: read on.
                    Ok(Err(_)) => errors += 1,
                    Ok(Ok((n, SocketAddr::V4(from)))) => {
                        if self.link(*from.ip()).is_some()
                            && let Some(Packet::Response(names)) = mdns::read(&buf[..n])
                            && names.iter().any(|answered| mdns::same(answered, name))
                        {
                            return true;
                        }
                    }
                    Ok(Ok(_)) => {}
                }
            }
        }
        false
    }

    /// The link a device at `ip` is on, if any.
    fn link(&self, ip: Ipv4Addr) -> Option<&Link> {
        self.links.iter().find(|link| link.reaches(ip))
    }

    /// Answers `packet` from `from`, if it is a query for Pley's names from
    /// a device on one of the links.
    async fn heard(&mut self, packet: &[u8], from: SocketAddr) {
        let (SocketAddr::V4(from), Some((name, instance))) = (from, &self.announced) else { return };
        let Some(link) = self.link(*from.ip()).cloned() else { return };
        let Some(Packet::Query(query)) = mdns::read(packet) else { return };
        let records = self.records(&link, instance, name, None);
        if from.port() != PORT {
            // A plain DNS resolver asking (legacy unicast, RFC 6762 §6.7):
            // each question answered by itself, its id and question repeated,
            // with short TTLs.
            for question in query.questions.iter().take(MAX_LEGACY_ANSWERS) {
                let (mut found, _) = answers(&records, std::slice::from_ref(question));
                if found.is_empty() || !self.may_unicast() {
                    continue;
                }
                found.iter_mut().for_each(|r| r.ttl = r.ttl.min(10));
                self.send(&link, &mdns::write(&found, &[], Some((query.id, question))), from.into()).await;
            }
            return;
        }
        let (unicast, mut multicast): (Vec<Question>, Vec<Question>) =
            query.questions.into_iter().partition(|question| question.unicast);
        let (found, extra) = answers(&records, &unicast);
        if !found.is_empty() {
            if self.may_unicast() {
                self.send(&link, &mdns::write(&found, &extra, None), from.into()).await;
            } else {
                // Past the cap, they are answered as multicast questions are.
                multicast.extend(unicast);
            }
        }
        // Once a second at most per question on each link.
        let now = Instant::now();
        let fresh: Vec<Question> = multicast
            .into_iter()
            .filter(|question| {
                let (found, _) = records.answer(question);
                if found.is_empty() {
                    return false;
                }
                let key = (link.ip, format!("{}/{}", question.name.join(".").to_lowercase(), question.kind));
                match self.multicast.get(&key) {
                    Some(at) if now.duration_since(*at) < Duration::from_secs(1) => false,
                    _ => {
                        self.multicast.insert(key, now);
                        true
                    }
                }
            })
            .collect();
        let (found, extra) = answers(&records, &fresh);
        if !found.is_empty() {
            self.send(&link, &mdns::write(&found, &extra, None), SocketAddrV4::new(GROUP, PORT).into()).await;
        }
    }

    /// Reads the adapters again: a new one is announced on, one gone is left.
    async fn rescan(&mut self) {
        let now = links(&self.plan);
        let socket = SockRef::from(&self.socket);
        for gone in self.links.iter().filter(|link| !now.contains(link)) {
            let _ = socket.leave_multicast_v4(&GROUP, &gone.ip);
            self.multicast.retain(|(ip, _), _| *ip != gone.ip);
        }
        let new: Vec<Link> = now.iter().filter(|link| !self.links.contains(link)).cloned().collect();
        let had = self.links.len();
        let joined = self.joined(now);
        self.links = joined;
        let new: Vec<Link> = new.into_iter().filter(|link| self.links.contains(link)).collect();
        self.said_where();
        if !self.probed {
            // The first network to ask on: the names are asked for there.
            if let Some((name, _)) = self.announced.clone() {
                self.announce(&name).await;
            }
        } else if !new.is_empty() {
            self.join(new).await;
        }
        if had > 0 && self.links.is_empty() {
            tracing::debug!("Bonjour has no network to announce Pley on now");
        }
    }

    /// Claims the names on networks that came after they were claimed:
    /// asked for there first, and, when another device there has one,
    /// given up everywhere and claimed again (the host name for the
    /// generated one, the instance for the next free one).
    async fn join(&mut self, new: Vec<Link>) {
        let Some((name, instance)) = self.announced.clone() else { return };
        let service: Name = std::iter::once(instance).chain(labels(&SERVICE)).collect();
        let generated = host_name(&generated_host(&self.id));
        let host_taken = self.host != generated && self.taken(&self.host, mdns::TYPE_A, &new).await;
        if !host_taken && !self.taken(&service, mdns::TYPE_SRV, &new).await {
            self.announce_on(&new, false).await;
            self.again = Some(Instant::now() + ANNOUNCE_AGAIN);
            return;
        }
        self.announce_on(&self.links.clone(), true).await;
        if host_taken {
            tracing::warn!(
                "another device on the network is {}: Bonjour gives Pley's addresses as {} instead",
                self.host.join("."),
                generated.join(".")
            );
            self.host = generated;
        }
        self.probed = false;
        self.announce(&name).await;
    }

    /// Announces a second time, once.
    async fn again(&mut self) {
        self.again = None;
        self.announce_on(&self.links.clone(), false).await;
    }

    /// Says goodbye on every link.
    async fn stop(self) {
        self.announce_on(&self.links, true).await;
    }
}

/// Announces the server as where it listens and its name say, until it
/// stops.
pub async fn run(state: AppState) {
    let id = state.identity.id.clone();
    let mut applied = state.network.applied();
    let mut settings = state.settings.subscribe();
    let mut responder: Option<Responder> = None;
    let mut rescan = tokio::time::interval(RESCAN);
    rescan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut buf = vec![0u8; READ_BUFFER];
    // The plan Pley couldn't announce as, said once in the log.
    let mut failed: Option<Plan> = None;
    let mut errors = 0u32;
    loop {
        applied.borrow_and_update();
        let wanted = plan(&state.network.wanted());
        if responder.as_ref().map(|responder| &responder.plan) != wanted.as_ref() {
            if let Some(responder) = responder.take() {
                responder.stop().await;
            }
            responder = match wanted {
                Some(plan) => match Responder::start(plan.clone(), &id) {
                    Ok(responder) => {
                        failed = None;
                        Some(responder)
                    }
                    Err(err) => {
                        if failed.as_ref() == Some(&plan) {
                            tracing::debug!("Bonjour can't announce Pley: {err}");
                        } else {
                            tracing::warn!("Bonjour can't announce Pley: {err}");
                        }
                        failed = Some(plan);
                        None
                    }
                },
                None => None,
            };
        }
        let name = settings.borrow_and_update().server_name.clone();
        if let Some(responder) = &mut responder {
            responder.announce(&name).await;
        }
        tokio::select! {
            changed = applied.changed() => if changed.is_err() { break },
            changed = settings.changed() => if changed.is_err() { break },
            _ = rescan.tick() => if let Some(responder) = &mut responder { responder.rescan().await },
            () = again(&responder) => if let Some(responder) = &mut responder { responder.again().await },
            heard = receive(&responder, &mut buf) => match heard {
                Ok((n, from)) => {
                    errors = 0;
                    if let Some(responder) = &mut responder {
                        responder.heard(&buf[..n], from).await;
                    }
                }
                Err(err) => {
                    // Windows reports an unreachable earlier destination on
                    // the next read: read again at once, and slow down only
                    // when reads keep failing.
                    tracing::trace!("Bonjour: {err}");
                    errors += 1;
                    if errors > 10 {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            },
            () = state.stopping.cancelled() => break,
        }
    }
    if let Some(responder) = responder {
        responder.stop().await;
    }
}

/// The next packet `responder` hears; never, without one.
async fn receive(responder: &Option<Responder>, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
    let Some(responder) = responder else { return std::future::pending().await };
    responder.socket.recv_from(buf).await
}

/// When `responder` is to announce again; never, without one.
async fn again(responder: &Option<Responder>) {
    match responder.as_ref().and_then(|responder| responder.again) {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(web: &[&str], bonjour: bool) -> Listen {
        Listen {
            web: web.iter().map(|a| a.parse().unwrap()).collect(),
            plex: Vec::new(),
            plex_on_web: true,
            jellyfin: Vec::new(),
            jellyfin_on_web: true,
            discovery: true,
            bonjour,
            bonjour_adapters: Vec::new(),
            bonjour_host_name: String::new(),
        }
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    /// A plan from the web addresses `web`, on the adapters `adapters`.
    fn on(web: &[&str], adapters: &[&str]) -> Plan {
        Plan {
            port: 7539,
            addresses: web.iter().map(|a| ip(a)).collect(),
            adapters: adapters.iter().map(|&a| a.to_owned()).collect(),
            host: String::new(),
        }
    }

    fn name(text: &str) -> Name {
        text.split('.').map(str::to_owned).collect()
    }

    fn records() -> Records {
        let host = host_name(&generated_host("0123456789abcdef"));
        Records::new("Den", &host, 7539, txt("0123456789abcdef", "Den"), Ipv4Addr::new(192, 168, 1, 10), None)
    }

    fn ask(text: &str, kind: u16) -> Question {
        Question { name: name(text), kind, unicast: false }
    }

    #[test]
    fn announced_with_bonjour_on_the_web_port() {
        assert_eq!(plan(&listen(&["0.0.0.0:7539"], false)), None, "Bonjour off");
        assert_eq!(plan(&listen(&["127.0.0.1:7539"], true)), None, "this computer only");
        assert_eq!(
            plan(&listen(&["127.0.0.1:7539", "192.168.1.10:8000", "10.0.0.2:8000", "10.0.0.3:9000"], true)),
            Some(Plan {
                port: 8000,
                addresses: vec![ip("192.168.1.10"), ip("10.0.0.2")],
                adapters: Vec::new(),
                host: String::new()
            })
        );
        // Its own switch: the broadcast's answer can be off and Bonjour on.
        let mut only = listen(&["0.0.0.0:7539"], true);
        only.discovery = false;
        assert!(plan(&only).is_some());
    }

    #[test]
    fn announced_from_local_addresses_only() {
        let every = on(&["0.0.0.0"], &[]);
        assert!(announced_on(&every, "eth0", ip("192.168.1.10")));
        // 100.64.0.0/10 (Tailscale's, carriers'): only when chosen.
        assert!(!announced_on(&every, "tailscale0", ip("100.101.102.103")));
        assert!(announced_on(&on(&["0.0.0.0"], &["tailscale0"]), "tailscale0", ip("100.101.102.103")));
        assert!(shared_address(ip("100.64.0.0")) && shared_address(ip("100.127.255.255")));
        assert!(!shared_address(ip("100.63.255.255")) && !shared_address(ip("100.128.0.0")));
        assert!(!announced_on(&every, "lo", ip("127.0.0.1")), "loopback");
        assert!(!announced_on(&every, "eth1", ip("203.0.113.5")), "the internet");
        let dual = on(&["::"], &[]);
        assert_eq!(announced_on(&dual, "eth0", ip("192.168.1.10")), cfg!(not(windows)), "dual-stack but on Windows");
        assert!(!announced_on(&dual, "eth1", ip("203.0.113.5")), "the internet, dual-stack");
        let one = on(&["192.168.1.10"], &[]);
        assert!(announced_on(&one, "eth0", ip("192.168.1.10")));
        assert!(!announced_on(&one, "eth0", ip("192.168.1.11")), "another adapter");
    }

    #[test]
    fn announced_from_the_adapters_chosen_only() {
        // A server on the home network, a VPN, a container bridge and a
        // guest Wi-Fi, announced on the home network alone.
        let home = on(&["0.0.0.0"], &["Ethernet"]);
        assert!(announced_on(&home, "Ethernet", ip("192.168.1.10")));
        assert!(!announced_on(&home, "Tailscale", ip("100.101.102.103")));
        assert!(!announced_on(&home, "vEthernet (WSL)", ip("172.17.0.1")));
        assert!(!announced_on(&home, "Wi-Fi", ip("10.0.0.5")));
        assert!(!announced_on(&home, "ethernet", ip("192.168.1.10")), "names as the system gives them");
        // A chosen adapter still needs a web address there.
        let elsewhere = on(&["10.0.0.5"], &["Ethernet"]);
        assert!(!announced_on(&elsewhere, "Ethernet", ip("192.168.1.10")));
        let both = on(&["0.0.0.0"], &["Ethernet", "Wi-Fi"]);
        assert!(announced_on(&both, "Wi-Fi", ip("10.0.0.5")));
        assert!(adapter_name_ok("vEthernet (Default Switch)") && adapter_name_ok("en0"));
        assert!(
            !adapter_name_ok(" ") && !adapter_name_ok("eth0\n") && !adapter_name_ok(&"x".repeat(MAX_ADAPTER_NAME + 1))
        );
    }

    #[test]
    fn only_devices_on_the_link_are_answered() {
        let link = Link {
            adapter: "Ethernet".into(),
            ip: Ipv4Addr::new(192, 168, 1, 10),
            mask: Ipv4Addr::new(255, 255, 255, 0),
        };
        assert!(link.reaches(Ipv4Addr::new(192, 168, 1, 77)));
        assert!(!link.reaches(Ipv4Addr::new(192, 168, 2, 77)), "another network");
        assert!(!link.reaches(Ipv4Addr::new(203, 0, 113, 5)), "a forged address elsewhere");
    }

    #[test]
    fn browsers_get_the_service_and_what_resolves_it() {
        let r = records();
        // Browsing: the PTR, with SRV, TXT and the address beside it.
        let (found, extra) = answers(&r, &[ask("_pley._tcp.local", mdns::TYPE_PTR)]);
        assert_eq!(found, std::slice::from_ref(&r.ptr));
        assert_eq!(extra, [r.srv.clone(), r.txt.clone(), r.a.clone()]);
        assert_eq!(r.ptr.data, Data::Ptr(name("Den._pley._tcp.local")));
        // Resolving, without case.
        let (found, extra) = answers(&r, &[ask("den._PLEY._tcp.local", mdns::TYPE_SRV)]);
        assert_eq!((found, extra), (vec![r.srv.clone()], vec![r.a.clone()]));
        assert_eq!(answers(&r, &[ask("Den._pley._tcp.local", mdns::TYPE_ANY)]).0, [r.srv.clone(), r.txt.clone()]);
        assert_eq!(answers(&r, &[ask("pley-0123456789ab.local", mdns::TYPE_A)]).0, std::slice::from_ref(&r.a));
        assert_eq!(
            answers(&r, &[ask("_services._dns-sd._udp.local", mdns::TYPE_PTR)]).0,
            std::slice::from_ref(&r.services)
        );
        // Each record once, whatever was asked twice.
        let twice = [ask("_pley._tcp.local", mdns::TYPE_PTR), ask("_pley._tcp.local", mdns::TYPE_ANY)];
        assert_eq!(answers(&r, &twice).0.len(), 1);
        // Nothing for names or kinds that aren't Pley's.
        for q in [
            ask("_http._tcp.local", mdns::TYPE_PTR),
            ask("Other._pley._tcp.local", mdns::TYPE_SRV),
            ask("pley-0123456789ab.local", 28),
            ask("_pley._tcp.local", mdns::TYPE_SRV),
        ] {
            assert_eq!(answers(&r, std::slice::from_ref(&q)), (vec![], vec![]), "{q:?}");
        }
    }

    #[test]
    fn txt_records_are_the_udp_answers_fields() {
        let id = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            txt(id, "  Living room "),
            [
                "pley=1".to_owned(),
                format!("id={id}"),
                "name=Living room".to_owned(),
                format!("version={}", env!("CARGO_PKG_VERSION")),
            ]
        );
        assert_eq!(generated_host(id), "pley-0123456789ab");
        // Goodbyes are the same records, with a TTL of 0.
        let host = host_name(&generated_host(id));
        let gone = Records::new("Den", &host, 7539, txt(id, "Den"), Ipv4Addr::LOCALHOST, Some(0));
        assert!(gone.all().iter().all(|r| r.ttl == 0));
    }

    #[test]
    fn names_fit_their_records() {
        // 64 characters of 4 bytes each: more than a TXT string holds.
        let long = "😀".repeat(80);
        let records = txt("id", &long);
        assert_eq!(records[2], format!("name={}", "😀".repeat(62)));
        assert!(records[2].len() <= MAX_TXT);
        assert_eq!(instance_name(&long, 1), "😀".repeat(15));
        assert_eq!(instance_name(&"a".repeat(100), 1).len(), MAX_LABEL);
        assert_eq!(instance_name(&"a".repeat(100), 2), format!("{} (2)", "a".repeat(59)));
        assert_eq!(instance_name("Den", 3), "Den (3)");
        assert_eq!(instance_name(" \u{7}Den\n", 1), "Den", "no control characters");
        assert_eq!(instance_name("   ", 1), "Pley");
        assert_eq!(instance_name("media.home", 1), "media.home");
    }

    #[test]
    fn a_host_name_is_chosen_as_one_label() {
        assert_eq!(host_label(" Media.LOCAL. "), "media");
        assert_eq!(host_label("den-tv"), "den-tv");
        assert_eq!(host_label(""), "");
        for ok in ["", "media", "den-tv", "pley2", &"a".repeat(63)] {
            assert!(host_label_ok(ok), "{ok}");
        }
        for bad in ["my media", "media.home", "-media", "media-", "médiа", "a_b", &"a".repeat(64)] {
            assert!(!host_label_ok(bad), "{bad}");
        }
        // Chosen, it goes in the plan, so a change starts the responder again.
        let mut chosen = listen(&["0.0.0.0:7539"], true);
        chosen.bonjour_host_name = "media".into();
        assert_eq!(plan(&chosen).unwrap().host, "media");
        assert_ne!(plan(&chosen), plan(&listen(&["0.0.0.0:7539"], true)));
    }
}
