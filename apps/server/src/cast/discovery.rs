//! Finding Google Cast devices on the local network: they announce
//! `_googlecast._tcp` by mDNS. Pley asks as a legacy unicast querier (RFC
//! 6762 §6.7): from a port of its own, so devices answer it directly and
//! nothing shares port 5353 with the system's responder or Pley's own
//! ([`crate::bonjour`]). It asks when someone looks for devices, never on a
//! timer. What comes back is from any device on the network: only answers
//! from local addresses count, and what they say is bounded.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use crate::mdns::{self, Data, Name};

const SERVICE: [&str; 3] = ["_googlecast", "_tcp", "local"];
const GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353);
/// The longest device name kept, in characters, and id.
const MAX_NAME: usize = 100;
const MAX_ID: usize = 64;
/// Devices kept from one look at most.
const MAX_FOUND: usize = 64;

/// A Cast device that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Its own id (`id` in its TXT record), the same at every look.
    pub id: String,
    /// The name its owner gave it ("Living room TV").
    pub name: String,
    /// "Chromecast", "Chromecast Ultra", "Google TV Streamer", a TV's.
    pub model: Option<String>,
    pub address: SocketAddr,
    /// It shows video: a Cast speaker or a group of them doesn't.
    pub video: bool,
}

/// Asks the local network, from each of `from` (this computer's IPv4
/// addresses), and gathers the answers for `wait`.
pub async fn browse(from: &[Ipv4Addr], wait: Duration) -> std::io::Result<Vec<Found>> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)).into())?;
    socket.set_multicast_ttl_v4(255)?;
    let service: Name = SERVICE.iter().map(|s| (*s).to_owned()).collect();
    let query = mdns::write_query(&service, mdns::TYPE_PTR);
    let socket = UdpSocket::from_std(socket.into())?;
    let mut sent = false;
    for address in from {
        let raw = socket2::SockRef::from(&socket);
        if raw.set_multicast_if_v4(address).is_ok() {
            sent |= socket.send_to(&query, GROUP).await.is_ok();
        }
    }
    if !sent {
        return Ok(Vec::new());
    }
    let mut found: HashMap<String, Found> = HashMap::new();
    let mut buf = vec![0; 9000];
    let _ = tokio::time::timeout(wait, async {
        while found.len() < MAX_FOUND {
            let Ok((len, source)) = socket.recv_from(&mut buf).await else { break };
            if !local(source.ip()) {
                continue;
            }
            for device in read(&buf[..len], source.ip()) {
                found.insert(device.id.clone(), device);
            }
        }
    })
    .await;
    Ok(found.into_values().collect())
}

/// Whether `ip` is on a local network: a private or link-local address.
fn local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(_) => false,
    }
}

/// The devices a response from `source` describes. The address is the one
/// it named for itself, when that is on a local network, else the one it
/// answered from.
fn read(packet: &[u8], source: IpAddr) -> Vec<Found> {
    let Some(records) = mdns::records(packet) else { return Vec::new() };
    let service: Name = SERVICE.iter().map(|s| (*s).to_owned()).collect();
    let instances = records.iter().filter_map(|(name, data)| match data {
        Data::Ptr(instance) if mdns::same(name, &service) => Some(instance),
        _ => None,
    });
    let mut out = Vec::new();
    for instance in instances {
        let mut srv = None;
        let mut txt: HashMap<String, String> = HashMap::new();
        for (_, data) in records.iter().filter(|(name, _)| mdns::same(name, instance)) {
            match data {
                Data::Srv { port, target } => srv = Some((*port, target)),
                Data::Txt(strings) => {
                    for s in strings {
                        if let Some((key, value)) = s.split_once('=') {
                            txt.insert(key.to_ascii_lowercase(), value.to_owned());
                        }
                    }
                }
                _ => {}
            }
        }
        let Some((port, target)) = srv else { continue };
        let named = records.iter().find_map(|(name, data)| match data {
            Data::A(ip) if mdns::same(name, target) && local(IpAddr::V4(*ip)) => Some(IpAddr::V4(*ip)),
            _ => None,
        });
        let Some(id) = txt
            .get("id")
            .filter(|id| !id.is_empty() && id.len() <= MAX_ID && id.chars().all(|c| c.is_ascii_alphanumeric()))
        else {
            continue;
        };
        let clean =
            |s: &str| s.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect::<String>().trim().to_owned();
        let model = txt.get("md").map(|m| clean(m)).filter(|m| !m.is_empty());
        let name = txt.get("fn").map(|n| clean(n)).filter(|n| !n.is_empty()).or_else(|| model.clone());
        let Some(name) = name else { continue };
        // Its capabilities, a bit field: 1 is video out. A device that
        // doesn't say is taken to show video.
        let video = txt.get("ca").and_then(|ca| ca.parse::<u64>().ok()).is_none_or(|ca| ca & 1 != 0);
        out.push(Found {
            id: id.to_ascii_lowercase(),
            name,
            model,
            address: SocketAddr::new(named.unwrap_or(source), port),
            video,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdns::Record;

    fn name(text: &str) -> Name {
        text.split('.').map(str::to_owned).collect()
    }

    fn response(txt: Vec<&str>, address: Ipv4Addr) -> Vec<u8> {
        let instance = name("Chromecast-9f8e._googlecast._tcp.local");
        let host = name("9f8e.local");
        let ptr =
            Record { name: name("_googlecast._tcp.local"), data: Data::Ptr(instance.clone()), ttl: 120, unique: false };
        let rest = [
            Record {
                name: instance.clone(),
                data: Data::Srv { port: 8009, target: host.clone() },
                ttl: 120,
                unique: true,
            },
            Record {
                name: instance,
                data: Data::Txt(txt.into_iter().map(str::to_owned).collect()),
                ttl: 120,
                unique: true,
            },
            Record { name: host, data: Data::A(address), ttl: 120, unique: true },
        ];
        mdns::write(std::slice::from_ref(&ptr), &rest, None)
    }

    #[test]
    fn a_device_is_read_from_its_answer() {
        let source = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 30));
        let p = response(
            vec!["id=9F8E00", "md=Chromecast", "fn=Living room\u{7}", "ca=4101"],
            Ipv4Addr::new(192, 168, 1, 30),
        );
        let found = read(&p, source);
        assert_eq!(
            found,
            vec![Found {
                id: "9f8e00".into(),
                name: "Living room".into(),
                model: Some("Chromecast".into()),
                address: "192.168.1.30:8009".parse().unwrap(),
                video: true,
            }]
        );
        // A speaker group: no video.
        let group =
            response(vec!["id=77", "md=Google Cast Group", "fn=Downstairs", "ca=2084"], Ipv4Addr::new(192, 168, 1, 30));
        assert!(!read(&group, source)[0].video);
    }

    #[test]
    fn what_a_device_says_is_checked() {
        let source = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 30));
        // An address off the local network: the one it answered from.
        let elsewhere = response(vec!["id=ab", "fn=TV"], Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(read(&elsewhere, source)[0].address, "192.168.1.30:8009".parse().unwrap());
        // No id, an id that isn't one, no name at all.
        assert!(read(&response(vec!["fn=TV"], Ipv4Addr::new(192, 168, 1, 30)), source).is_empty());
        assert!(read(&response(vec!["id=a/b", "fn=TV"], Ipv4Addr::new(192, 168, 1, 30)), source).is_empty());
        assert!(read(&response(vec!["id=ab"], Ipv4Addr::new(192, 168, 1, 30)), source).is_empty());
        // A long name is cut.
        let long = format!("fn={}", "x".repeat(300));
        assert_eq!(
            read(&response(vec!["id=ab", &long], Ipv4Addr::new(192, 168, 1, 30)), source)[0].name.len(),
            MAX_NAME
        );
        assert!(!local("8.8.8.8".parse().unwrap()) && local("10.0.0.2".parse().unwrap()));
    }
}
