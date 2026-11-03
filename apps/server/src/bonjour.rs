//! Bonjour: beside [`crate::discovery`]'s UDP answer, the server announces
//! itself on the local network by mDNS and DNS-SD as `_pley._tcp`, so apps
//! that may not broadcast (Apple's, without the multicast entitlement) find
//! it with the system's own browser. The announcement carries the UDP
//! answer's fields in its TXT records, and tells nothing more. It has a
//! switch of its own (Settings → Network, `--no-bonjour`; `--no-discovery`
//! turns it off too), goes out only from the web addresses' adapters on the
//! local network, or those of them chosen (a server on a VPN, a container
//! bridge or a guest Wi-Fi as well need not be announced there), follows
//! renames, and says goodbye when the server stops. Each adapter announces
//! its own addresses only, so a device is never handed another network's.
//!
//! The web app is announced too, as `_http._tcp` on the host name chosen,
//! `pley.local` by default, so a browser opens `http://pley.local:7539`.
//! That name is one for the whole network: when another device has it, the
//! responder would take `pley-2.local` instead, so the web app's
//! announcement goes and the server is reached by its own name,
//! `pley-<id>.local`, which the apps' announcement always uses. With no
//! host name chosen, the web app isn't announced.
//!
//! The responder is mdns-sd's, which shares UDP port 5353 with the system's
//! own (mDNSResponder, Avahi, Windows').

use std::net::IpAddr;
use std::time::Duration;

use mdns_sd::{DaemonEvent, IfKind, IfPredicate, ServiceDaemon, ServiceInfo};
use tokio::sync::mpsc;

use crate::network::Listen;
use crate::state::AppState;

/// The service type apps browse for.
pub const SERVICE_TYPE: &str = "_pley._tcp.local.";
/// The web app's service type, which browsers and Bonjour tools list.
const WEB_SERVICE_TYPE: &str = "_http._tcp.local.";
/// The web app's host name on the local network unless another is chosen:
/// `pley.local`.
pub const DEFAULT_WEB_HOST_NAME: &str = "pley";
/// An adapter's name's length at most, in bytes: Windows' are 256 UTF-16
/// characters at most, the others' far fewer.
const MAX_ADAPTER_NAME: usize = 256;
/// A DNS label's length at most, which an instance name is.
const MAX_LABEL: usize = 63;
/// A TXT string's length at most, `name=` included.
const MAX_TXT: usize = 255;
/// A server name's length at most, as Settings take it (the default, the
/// computer's name, isn't checked there).
const MAX_NAME_CHARS: usize = 64;
/// How long a goodbye, and the responder's stopping, may take.
const GOODBYE_WAIT: Duration = Duration::from_secs(1);

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
    /// The web app's host name, `name.local.`: not announced when `None`.
    web_host: Option<String>,
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
    let host = listen.web_host_name.trim().to_ascii_lowercase();
    Some(Plan {
        port,
        addresses: web.filter(|addr| addr.port() == port).map(|addr| addr.ip()).collect(),
        adapters: listen.bonjour_adapters.clone(),
        web_host: (!host.is_empty()).then(|| format!("{host}.local.")),
    })
}

/// Whether the announcement goes out from the address `ip` of the adapter
/// named `adapter`: one on the local network that the web app listens on,
/// among the adapters chosen, if some are.
fn announced_on(plan: &Plan, adapter: &str, ip: IpAddr) -> bool {
    !ip.is_loopback()
        && crate::setup::is_local_address(ip)
        && (plan.adapters.is_empty() || plan.adapters.iter().any(|chosen| chosen == adapter))
        && plan.addresses.iter().any(|web| *web == ip || (web.is_unspecified() && listens_on_family(*web, ip)))
}

/// Whether `name` may be a network adapter's: not blank, bounded, and
/// without control characters (it is written in `pley.toml`).
pub fn adapter_name_ok(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= MAX_ADAPTER_NAME && !name.chars().any(char::is_control)
}

/// Why `name` can't be the web app's host name, if it can't: one DNS label
/// of letters, digits and hyphens (RFC 1123), or empty for none.
pub fn check_host_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    let label = name.len() <= MAX_LABEL
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if label {
        Ok(())
    } else {
        Err(format!(
            "{name:?} can't be a name on the local network: letters, digits and hyphens, {MAX_LABEL} at most, \
             not starting or ending with a hyphen"
        ))
    }
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

/// The instance name: the server's name, in a DNS label.
fn instance_name(name: &str) -> String {
    bounded(&shown_name(name), MAX_LABEL).trim_end().to_owned()
}

/// The TXT records: the UDP answer's fields, each in one TXT string.
fn txt(id: &str, name: &str) -> Vec<(&'static str, String)> {
    let name = shown_name(name);
    vec![
        ("pley", "1".into()),
        ("id", bounded(id, MAX_TXT - "id=".len()).into()),
        ("name", bounded(&name, MAX_TXT - "name=".len()).into()),
        ("version", env!("CARGO_PKG_VERSION").into()),
    ]
}

/// The host name the announcement's addresses go by: the server's own, not
/// the computer's, which the system's responder answers for.
fn host_name(id: &str) -> String {
    let short: String = id.chars().filter(char::is_ascii_alphanumeric).take(12).collect();
    format!("pley-{}.local.", short.to_lowercase())
}

/// The responder, announcing as a [`Plan`] says.
struct Announcer {
    daemon: ServiceDaemon,
    plan: Plan,
    /// The name announced and the service's full name, once announced.
    announced: Option<(String, String)>,
    /// The web app's service's full name, while it is announced.
    web: Option<String>,
    /// Another device has `pley.local`: the web app isn't announced again
    /// until the responder starts afresh.
    web_host_taken: bool,
    /// The host names the responder had to change because another device
    /// had them.
    renamed: mpsc::UnboundedReceiver<String>,
}

impl Announcer {
    fn start(plan: Plan) -> Result<Self, mdns_sd::Error> {
        let daemon = ServiceDaemon::new()?;
        // The adapters are checked again every few seconds: one that comes
        // later (Wi-Fi, a VPN) is announced on too.
        let chosen = plan.clone();
        daemon.disable_interface(IfKind::All)?;
        daemon.enable_interface(IfKind::Predicate(IfPredicate::new(move |intf| {
            announced_on(&chosen, &intf.name, intf.ip())
        })))?;
        // The responder's events come on a channel that blocks; a thread
        // passes its renames on, and ends when the responder stops.
        let events = daemon.monitor()?;
        let (renames, renamed) = mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("bonjour-events".into())
            .spawn(move || {
                while let Ok(event) = events.recv() {
                    if let DaemonEvent::NameChange(change) = event
                        && renames.send(change.original).is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|err| mdns_sd::Error::Msg(format!("no thread for its events: {err}")))?;
        Ok(Self { daemon, plan, announced: None, web: None, web_host_taken: false, renamed })
    }

    /// Announces the server as `name`, unless it already is.
    fn announce(&mut self, id: &str, name: &str) {
        if self.announced.as_ref().is_some_and(|(announced, _)| announced == name) {
            return;
        }
        let info = match ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name(name),
            &host_name(id),
            (),
            self.plan.port,
            &txt(id, name)[..],
        ) {
            Ok(info) => info.enable_addr_auto(),
            Err(err) => {
                tracing::warn!("Bonjour can't announce Pley: {err}");
                return;
            }
        };
        let fullname = info.get_fullname().to_owned();
        // Registering again under the same name updates the records; under
        // another, the old one says goodbye first.
        if let Some((_, old)) = self.announced.take().filter(|(_, old)| *old != fullname) {
            let _ = self.daemon.unregister(&old);
        }
        match self.daemon.register(info) {
            Ok(()) => {
                tracing::debug!("announced by Bonjour as {fullname} on port {}", self.plan.port);
                self.announced = Some((name.to_owned(), fullname));
            }
            Err(err) => tracing::warn!("Bonjour can't announce Pley: {err}"),
        }
        self.announce_web(name);
    }

    /// Announces the web app on its host name as `name`, unless none is
    /// chosen or another device has it.
    fn announce_web(&mut self, name: &str) {
        let Some(host) = self.plan.web_host.clone().filter(|_| !self.web_host_taken) else {
            return;
        };
        let info = match ServiceInfo::new(
            WEB_SERVICE_TYPE,
            &instance_name(name),
            &host,
            (),
            self.plan.port,
            &[("path", "/")][..],
        ) {
            Ok(info) => info.enable_addr_auto(),
            Err(err) => {
                tracing::debug!("Bonjour can't announce the web app: {err}");
                return;
            }
        };
        let fullname = info.get_fullname().to_owned();
        if let Some(old) = self.web.take().filter(|old| *old != fullname) {
            let _ = self.daemon.unregister(&old);
        }
        match self.daemon.register(info) {
            Ok(()) => self.web = Some(fullname),
            Err(err) => tracing::debug!("Bonjour can't announce the web app: {err}"),
        }
    }

    /// Takes in a host name the responder changed: the web app's taken by
    /// another device withdraws its announcement, rather than have it
    /// answer as `pley-2.local`.
    fn renamed(&mut self, original: &str) {
        let Some(host) = self.plan.web_host.clone() else {
            return;
        };
        if !web_host_taken_by(original, &host) || self.web_host_taken {
            return;
        }
        self.web_host_taken = true;
        if let Some(web) = self.web.take() {
            let _ = self.daemon.unregister(&web);
        }
        tracing::info!(
            "another device on the network is {}; Pley's web app is announced by its own name",
            host.trim_end_matches('.')
        );
    }

    /// Says goodbye, and stops the responder.
    async fn stop(self) {
        let goodbyes: Vec<_> = self
            .announced
            .map(|(_, fullname)| fullname)
            .into_iter()
            .chain(self.web)
            .filter_map(|fullname| self.daemon.unregister(&fullname).ok())
            .collect();
        let daemon = self.daemon;
        let _ = tokio::task::spawn_blocking(move || {
            for goodbye in goodbyes {
                let _ = goodbye.recv_timeout(GOODBYE_WAIT);
            }
            if let Ok(stopped) = daemon.shutdown() {
                let _ = stopped.recv_timeout(GOODBYE_WAIT);
            }
        })
        .await;
    }
}

/// Announces the server as where it listens and its name say, until it
/// stops.
pub async fn run(state: AppState) {
    let id = state.server_id();
    let mut applied = state.network.applied();
    let mut settings = state.settings.subscribe();
    let mut announcer: Option<Announcer> = None;
    loop {
        applied.borrow_and_update();
        let wanted = plan(&state.network.wanted());
        if announcer.as_ref().map(|announcer| &announcer.plan) != wanted.as_ref() {
            if let Some(announcer) = announcer.take() {
                announcer.stop().await;
            }
            announcer = wanted.and_then(|plan| {
                Announcer::start(plan).map_err(|err| tracing::warn!("Bonjour can't announce Pley: {err}")).ok()
            });
        }
        let name = settings.borrow_and_update().server_name.clone();
        if let Some(announcer) = &mut announcer {
            announcer.announce(&id, &name);
        }
        tokio::select! {
            changed = applied.changed() => if changed.is_err() { break },
            changed = settings.changed() => if changed.is_err() { break },
            Some(original) = renamed(&mut announcer) => {
                if let Some(announcer) = &mut announcer {
                    announcer.renamed(&original);
                }
            }
            () = state.stopping.cancelled() => break,
        }
    }
    if let Some(announcer) = announcer {
        announcer.stop().await;
    }
}

/// The next host name `announcer`'s responder changed; never, without one.
async fn renamed(announcer: &mut Option<Announcer>) -> Option<String> {
    match announcer {
        Some(announcer) => announcer.renamed.recv().await,
        None => std::future::pending().await,
    }
}

/// Whether the responder changing `original` means another device has the
/// web app's host name `host`. Names compare without case, as DNS's do.
fn web_host_taken_by(original: &str, host: &str) -> bool {
    original.eq_ignore_ascii_case(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listen(web: &[&str], bonjour: bool) -> Listen {
        Listen {
            web: web.iter().map(|a| a.parse().unwrap()).collect(),
            jellyfin: Vec::new(),
            jellyfin_on_web: true,
            discovery: true,
            bonjour,
            bonjour_adapters: Vec::new(),
            web_host_name: DEFAULT_WEB_HOST_NAME.into(),
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
            web_host: None,
        }
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
                web_host: Some("pley.local.".into()),
            })
        );
        // Its own switch: the broadcast's answer can be off and Bonjour on.
        let mut only = listen(&["0.0.0.0:7539"], true);
        only.discovery = false;
        assert!(plan(&only).is_some());
    }

    #[test]
    fn the_web_app_goes_by_the_host_name_chosen() {
        let mut l = listen(&["0.0.0.0:7539"], true);
        l.web_host_name = " Media-Room ".into();
        assert_eq!(plan(&l).unwrap().web_host.as_deref(), Some("media-room.local."), "DNS names have no case");
        l.web_host_name = String::new();
        assert_eq!(plan(&l).unwrap().web_host, None, "none: the web app isn't announced");
        assert!(check_host_name("pley").is_ok() && check_host_name("").is_ok() && check_host_name("a1-b2").is_ok());
        for refused in ["pley.local", "-pley", "pley-", "pley tv", "pley_tv", "plé", &"a".repeat(MAX_LABEL + 1)] {
            assert!(check_host_name(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn announced_from_local_addresses_only() {
        let every = on(&["0.0.0.0"], &[]);
        assert!(announced_on(&every, "eth0", ip("192.168.1.10")));
        assert!(announced_on(&every, "tailscale0", ip("100.101.102.103")), "Tailscale");
        assert!(!announced_on(&every, "lo", ip("127.0.0.1")), "loopback");
        assert!(!announced_on(&every, "eth1", ip("203.0.113.5")), "the internet");
        assert!(!announced_on(&every, "eth0", ip("fe80::1")), "IPv6, where Pley doesn't listen");
        let dual = on(&["::"], &[]);
        assert!(announced_on(&dual, "eth0", ip("fe80::1")));
        assert!(!announced_on(&dual, "eth0", ip("2001:db8::1")), "a global IPv6 address");
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
    fn txt_records_are_the_udp_answers_fields() {
        let id = "0123456789abcdef0123456789abcdef01234567";
        let records = txt(id, "  Living room ");
        assert_eq!(
            records,
            [
                ("pley", "1".to_owned()),
                ("id", id.to_owned()),
                ("name", "Living room".to_owned()),
                ("version", env!("CARGO_PKG_VERSION").to_owned()),
            ]
        );
        let info = ServiceInfo::new(SERVICE_TYPE, "Living room", &host_name(id), (), 7539, &records[..]).unwrap();
        assert_eq!(info.get_property_val_str("name"), Some("Living room"));
        assert_eq!(info.get_fullname(), "Living room._pley._tcp.local.");
        assert_eq!(host_name(id), "pley-0123456789ab.local.");
    }

    #[test]
    fn the_web_app_is_announced_on_its_host_name() {
        let info = ServiceInfo::new(
            WEB_SERVICE_TYPE,
            &instance_name("Living room"),
            "pley.local.",
            (),
            7539,
            &[("path", "/")][..],
        )
        .unwrap();
        assert_eq!(info.get_fullname(), "Living room._http._tcp.local.");
        assert_eq!(info.get_hostname(), "pley.local.");
        assert_eq!(info.get_property_val_str("path"), Some("/"));
    }

    #[test]
    fn only_the_web_apps_host_name_taken_withdraws_it() {
        assert!(web_host_taken_by("pley.local.", "pley.local."));
        assert!(web_host_taken_by("Pley.Local.", "pley.local."), "DNS names compare without case");
        assert!(web_host_taken_by("media.local.", "media.local."));
        assert!(!web_host_taken_by("pley.local.", "media.local."), "another name chosen");
        assert!(!web_host_taken_by("pley-0123456789ab.local.", "pley.local."), "the server's own name");
        assert!(
            !web_host_taken_by("Living room._http._tcp.local.", "pley.local."),
            "an instance name, renamed with (2)"
        );
    }

    #[test]
    fn names_fit_their_records() {
        // 64 characters of 4 bytes each: more than a TXT string holds.
        let long = "😀".repeat(80);
        let records = txt("id", &long);
        let name = &records[2].1;
        assert_eq!(name, &"😀".repeat(62));
        assert!("name=".len() + name.len() <= MAX_TXT);
        assert_eq!(instance_name(&long), "😀".repeat(15));
        assert_eq!(instance_name(&"a".repeat(100)).len(), MAX_LABEL);
        assert_eq!(instance_name(" \u{7}Den\n"), "Den", "no control characters");
        assert_eq!(instance_name("   "), "Pley");
        assert_eq!(instance_name("media.home"), "media.home");
        let info = ServiceInfo::new(SERVICE_TYPE, &instance_name(&long), "pley-0.local.", (), 7539, &records[..]);
        assert!(info.is_ok(), "{info:?}");
    }
}
