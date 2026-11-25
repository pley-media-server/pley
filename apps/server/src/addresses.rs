//! The addresses apps reach this server at, which they switch between as
//! they move (`GET /api/server/addresses`): those an administrator lists in
//! Settings → Network (`addresses`), in their order, then those the server
//! finds at home from where it listens. An app sends a sign-in's token to
//! one only after the server there proves itself with that sign-in's key
//! (see [`crate::identity`]), so an address that leads elsewhere costs it a
//! try and nothing more.
//!
//! Only someone signed in reads the list: in the public `/api/server`, it
//! would tell anyone a home address with a public one. Nothing in it comes
//! from a request's `Host`, which any client names as it likes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::Serialize;

use crate::network::Adapter;

/// Where an address in the list comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// An administrator's, in Settings → Network.
    Set,
    /// Found at home, from where the server listens.
    Found,
}

/// An address apps reach the server at, written as the server names itself
/// there (its issuer): `https://pley.example.com`, `http://192.168.1.20:7539`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Address {
    pub url: String,
    pub source: Source,
}

/// The most addresses found that the list carries, as the administrator's
/// are 16 at most: apps keep 32.
pub const MAX_FOUND: usize = 16;

/// The list: the administrator's addresses (`set`, as Settings keep them)
/// in their order, then those found ([`MAX_FOUND`] at most), but those the
/// administrator lists.
pub fn list(set: &[String], found: Vec<String>) -> Vec<Address> {
    let mut list: Vec<Address> = set.iter().map(|url| Address { url: url.clone(), source: Source::Set }).collect();
    for url in found.into_iter().filter(|url| !set.contains(url)).take(MAX_FOUND) {
        if !list.iter().any(|address| address.url == url) {
            list.push(Address { url, source: Source::Found });
        }
    }
    list
}

/// The addresses at home of a server whose web listeners listen on `web`,
/// with this computer's `adapters`: `http://`, an adapter's IPv4 address and
/// the port of each listener that takes it. Those of private addresses
/// (RFC 1918) come first: on the adapters Bonjour announces from when some
/// are `chosen` (Settings → Network), else on every one but the networks of
/// virtual machines and containers on this computer. Then Tailscale's
/// address (100.64.0.0/10), on its own adapter only: another adapter's
/// address there is an internet provider's shared one. Adapters that aren't
/// connected count for nothing, and in a container on Docker's own network
/// (`contained`, see [`crate::network::on_container_network`]) nothing is
/// found: devices at home don't reach those addresses.
pub fn found(web: &[SocketAddr], adapters: &[Adapter], contained: bool, chosen: &[String]) -> Vec<String> {
    if crate::network::on_container_network(contained, adapters) {
        return Vec::new();
    }
    let mut home: Vec<String> = Vec::new();
    let mut tailscale: Vec<String> = Vec::new();
    for adapter in adapters.iter().filter(|adapter| adapter.up) {
        let home_network = if chosen.is_empty() { !virtual_network(adapter) } else { chosen.contains(&adapter.name) };
        for &ip in &adapter.addresses {
            let into = if ip.is_private() && home_network {
                &mut home
            } else if crate::setup::is_shared_address(ip) && tailscale_adapter(adapter) {
                &mut tailscale
            } else {
                continue;
            };
            for port in ports_taking(web, ip) {
                let url = http_address(ip, port);
                if !into.contains(&url) {
                    into.push(url);
                }
            }
        }
    }
    home.extend(tailscale);
    home
}

/// The ports of the listeners in `web` that take `ip`: on it, or on every
/// address of its family (`0.0.0.0`, and `[::]` but on Windows, as Bonjour
/// has it).
fn ports_taking(web: &[SocketAddr], ip: Ipv4Addr) -> impl Iterator<Item = u16> + '_ {
    let ip = IpAddr::V4(ip);
    web.iter()
        .filter(move |addr| {
            addr.port() != 0
                && (addr.ip() == ip || (addr.ip().is_unspecified() && crate::bonjour::listens_on_family(addr.ip(), ip)))
        })
        .map(SocketAddr::port)
}

/// `http://` and `ip`, with `port` unless it is http's own: as the server
/// names itself there.
fn http_address(ip: Ipv4Addr, port: u16) -> String {
    if port == 80 { format!("http://{ip}") } else { format!("http://{ip}:{port}") }
}

/// Whether `adapter` is a network of virtual machines or containers on
/// this computer, by the names their software gives it: Docker's, Podman's,
/// LXC's and libvirt's, VirtualBox's, VMware's and Parallels' host
/// networks, those macOS shares its connection with, and Hyper-V's own
/// switches (WSL's, the Default Switch, and Docker's on Windows). Devices
/// at home don't reach them. Hyper-V's external switches and bridges such
/// as Unraid's `br0` carry the home network itself, and count.
fn virtual_network(adapter: &Adapter) -> bool {
    const NAMES: [&str; 10] =
        ["docker", "podman", "cni", "lxcbr", "lxdbr", "incusbr", "virbr", "vboxnet", "vmnet", "vnic"];
    const HYPER_V: [&str; 4] = ["wsl", "default switch", "nat)", "dockernat"];
    let name = adapter.name.to_lowercase();
    let description = adapter.description.as_deref().unwrap_or_default().to_lowercase();
    // Docker's networks of its own: br- and the start of the network's id.
    let docker = name.strip_prefix("br-").is_some_and(|id| id.len() == 12 && id.bytes().all(|b| b.is_ascii_hexdigit()));
    // bridge100 and on (bridge0 is a Thunderbolt bridge).
    let shared_by_macos = name.strip_prefix("bridge").and_then(|n| n.parse::<u32>().ok()).is_some_and(|n| n >= 100);
    let hyper_v =
        name.strip_prefix("vethernet (").is_some_and(|switch| HYPER_V.iter().any(|own| switch.starts_with(own)));
    NAMES.iter().any(|prefix| name.starts_with(prefix))
        || docker
        || shared_by_macos
        || hyper_v
        || description.starts_with("virtualbox host-only")
        || description.starts_with("vmware virtual ethernet adapter")
}

/// Whether `adapter` is Tailscale's, as Linux (`tailscale0`) and Windows
/// (Tailscale) name it. On macOS it is a `utun` like any VPN's, and its
/// address isn't found.
fn tailscale_adapter(adapter: &Adapter) -> bool {
    adapter.name.to_lowercase().contains("tailscale")
        || adapter.description.as_deref().is_some_and(|description| description.to_lowercase().contains("tailscale"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, up: bool, addresses: &[&str]) -> Adapter {
        Adapter {
            name: name.into(),
            description: None,
            up,
            addresses: addresses.iter().map(|a| a.parse().unwrap()).collect(),
            network: None,
        }
    }

    fn described(name: &str, description: &str, addresses: &[&str]) -> Adapter {
        Adapter { description: Some(description.into()), ..adapter(name, true, addresses) }
    }

    fn web(addresses: &[&str]) -> Vec<SocketAddr> {
        addresses.iter().map(|a| a.parse().unwrap()).collect()
    }

    fn set(urls: &[&str]) -> Vec<String> {
        urls.iter().map(|&url| url.to_owned()).collect()
    }

    #[test]
    fn the_administrators_come_first_in_their_order() {
        let listed = list(
            &set(&["https://pley.example.com", "http://192.168.1.20:7539"]),
            set(&["http://192.168.1.20:7539", "http://10.0.0.4:7539"]),
        );
        assert_eq!(
            listed,
            [
                Address { url: "https://pley.example.com".into(), source: Source::Set },
                Address { url: "http://192.168.1.20:7539".into(), source: Source::Set },
                Address { url: "http://10.0.0.4:7539".into(), source: Source::Found },
            ],
            "one found that is listed already is the administrator's"
        );
        assert_eq!(
            serde_json::to_value(&listed[2]).unwrap(),
            serde_json::json!({"url": "http://10.0.0.4:7539", "source": "found"})
        );
        assert_eq!(serde_json::to_value(&listed[0]).unwrap()["source"], "set");
        assert!(list(&[], Vec::new()).is_empty());
        assert_eq!(list(&[], set(&["http://10.0.0.4:7539"]))[0].source, Source::Found);
        // As many found as the administrator may list, after the administrator's own.
        let many: Vec<String> = (1..=20).map(|n| format!("http://192.168.1.{n}:7539")).collect();
        let listed = list(&many[..16], many.clone());
        assert_eq!(listed.len(), 20, "the four found beyond the administrator's");
        let found: Vec<String> = (1..=40).map(|n| format!("http://10.0.0.{n}:7539")).collect();
        assert_eq!(list(&[], found).len(), MAX_FOUND);
    }

    #[test]
    fn home_addresses_are_found_where_pley_listens() {
        let adapters = [
            adapter("eth0", true, &["192.168.1.20", "203.0.113.5"]),
            adapter("wlan0", true, &["10.0.0.4"]),
            adapter("eth1", false, &["172.20.0.9"]),
            adapter("eth2", true, &["169.254.3.4"]),
        ];
        // Every address: the private ones of the connected adapters, not
        // the internet's or a link-local one.
        assert_eq!(
            found(&web(&["0.0.0.0:7539"]), &adapters, false, &[]),
            ["http://192.168.1.20:7539", "http://10.0.0.4:7539"]
        );
        // One address, and loopback: that address only.
        assert_eq!(found(&web(&["127.0.0.1:7539", "10.0.0.4:8000"]), &adapters, false, &[]), ["http://10.0.0.4:8000"]);
        // Each port a listener takes the address on, http's own left out.
        assert_eq!(
            found(&web(&["0.0.0.0:80", "192.168.1.20:7539"]), &adapters, false, &[]),
            ["http://192.168.1.20", "http://192.168.1.20:7539", "http://10.0.0.4"]
        );
        // `[::]` takes IPv4 too but on Windows; an IPv6 address alone none.
        let dual = found(&web(&["[::]:7539"]), &adapters, false, &[]);
        assert_eq!(dual.is_empty(), cfg!(windows), "{dual:?}");
        assert!(found(&web(&["[fd00::5]:7539"]), &adapters, false, &[]).is_empty());
        // Not listening yet (tests, a port of 0): nothing.
        assert!(found(&[], &adapters, false, &[]).is_empty());
        assert!(found(&web(&["0.0.0.0:0"]), &adapters, false, &[]).is_empty());
    }

    #[test]
    fn virtual_machines_and_containers_networks_are_left_out() {
        let any = web(&["0.0.0.0:7539"]);
        for left_out in [
            adapter("docker0", true, &["172.17.0.1"]),
            adapter("br-3f2a1b4c5d6e", true, &["172.18.0.1"]),
            adapter("virbr0", true, &["192.168.122.1"]),
            adapter("lxdbr0", true, &["10.10.10.1"]),
            adapter("podman0", true, &["10.88.0.1"]),
            adapter("vboxnet0", true, &["192.168.56.1"]),
            adapter("vmnet8", true, &["192.168.100.1"]),
            adapter("bridge100", true, &["192.168.64.1"]),
            adapter("vEthernet (WSL (Hyper-V firewall))", true, &["172.28.16.1"]),
            adapter("vEthernet (Default Switch)", true, &["172.21.32.1"]),
            adapter("vEthernet (nat)", true, &["172.25.0.1"]),
            described("VirtualBox Host-Only Network", "VirtualBox Host-Only Ethernet Adapter", &["192.168.56.1"]),
            described("VMware Network Adapter VMnet1", "VMware Virtual Ethernet Adapter for VMnet1", &["192.168.80.1"]),
        ] {
            assert!(found(&any, std::slice::from_ref(&left_out), false, &[]).is_empty(), "{left_out:?}");
        }
        // The home network, on a bridge or a Hyper-V external switch.
        for home in [
            adapter("br0", true, &["192.168.1.20"]),
            adapter("br-lan", true, &["192.168.1.20"]),
            adapter("bridge0", true, &["192.168.1.20"]),
            described("vEthernet (External)", "Hyper-V Virtual Ethernet Adapter #2", &["192.168.1.20"]),
            described("Ethernet", "Intel(R) Ethernet Connection I219-V", &["192.168.1.20"]),
        ] {
            assert_eq!(found(&any, std::slice::from_ref(&home), false, &[]), ["http://192.168.1.20:7539"], "{home:?}");
        }
    }

    #[test]
    fn the_adapters_chosen_for_bonjour_are_the_home_network() {
        let any = web(&["0.0.0.0:7539"]);
        let adapters = [
            adapter("Ethernet", true, &["192.168.1.20"]),
            adapter("Wi-Fi", true, &["10.0.0.4"]),
            adapter("vEthernet (WSL)", true, &["172.28.16.1"]),
        ];
        assert_eq!(found(&any, &adapters, false, &set(&["Wi-Fi"])), ["http://10.0.0.4:7539"]);
        // Chosen, a virtual machines' network counts too, as Bonjour has it.
        assert_eq!(
            found(&any, &adapters, false, &set(&["vEthernet (WSL)", "Ethernet"])),
            ["http://192.168.1.20:7539", "http://172.28.16.1:7539"]
        );
        assert!(found(&any, &adapters, false, &set(&["Gone"])).is_empty(), "none of them here now");
    }

    #[test]
    fn tailscales_address_comes_after_home_and_no_other_shared_one() {
        let any = web(&["0.0.0.0:7539"]);
        let adapters = [
            adapter("tailscale0", true, &["100.101.102.103"]),
            adapter("eth0", true, &["192.168.1.20"]),
            adapter("wwan0", true, &["100.72.3.4"]),
        ];
        assert_eq!(found(&any, &adapters, false, &[]), ["http://192.168.1.20:7539", "http://100.101.102.103:7539"]);
        let windows = [described("Tailscale", "Tailscale Tunnel", &["100.101.102.103"])];
        assert_eq!(found(&any, &windows, false, &[]), ["http://100.101.102.103:7539"]);
        // An internet provider's shared address, chosen for Bonjour or not.
        assert!(found(&any, &[adapter("utun3", true, &["100.72.3.4"])], false, &set(&["utun3"])).is_empty());
        // Bonjour's choice is the home network's: Tailscale's stays.
        assert_eq!(
            found(&any, &adapters, false, &set(&["eth0"])),
            ["http://192.168.1.20:7539", "http://100.101.102.103:7539"]
        );
    }

    #[test]
    fn nothing_is_found_on_dockers_own_network() {
        let any = web(&["0.0.0.0:7539"]);
        let bridge = [adapter("eth0", true, &["172.17.0.2"])];
        assert!(found(&any, &bridge, true, &[]).is_empty());
        // Outside a container, 172.16.0.0/12 can be a home network.
        assert_eq!(found(&any, &bridge, false, &[]), ["http://172.17.0.2:7539"]);
        // Host networking, or Unraid's br0: the home network's address.
        let host = [adapter("docker0", true, &["172.17.0.1"]), adapter("eth0", true, &["192.168.1.20"])];
        assert_eq!(found(&any, &host, true, &[]), ["http://192.168.1.20:7539"]);
    }
}
