//! Finding devices on the local network by the services they announce by
//! mDNS (`_googlecast._tcp`). Pley asks as a legacy unicast querier (RFC
//! 6762 §6.7): from a port of its own, so devices answer it directly and
//! nothing shares port 5353 with the system's responder or Pley's own
//! ([`crate::bonjour`]). It asks when someone looks for devices, never on a
//! timer. What comes back is from any device on the network: only answers
//! from local addresses count, and what they say is bounded. Each protocol
//! reads its own service's announcements ([`Service`]).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use super::Found;
use crate::mdns::{self, Data, Name};

const GROUP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(224, 0, 0, 251), 5353);
/// The longest device name kept, in characters.
pub const MAX_NAME: usize = 100;
/// Devices kept from one look at most.
const MAX_FOUND: usize = 64;

/// A service looked for, and how its announcements read as devices.
pub struct Service {
    /// Its name, `["_googlecast", "_tcp", "local"]`.
    pub name: &'static [&'static str],
    /// The device an announcement describes, if it is one.
    pub read: fn(&Announced) -> Option<Found>,
}

/// What a device announced of one instance of a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announced {
    /// The instance's own label ("Living room" of
    /// `Living room._airplay._tcp.local`), cleaned as a name is.
    pub instance: String,
    /// Where it answers: the address it named for itself, when that is on a
    /// local network, else the one it answered from.
    pub address: SocketAddr,
    /// Its TXT record, the keys in lowercase.
    pub txt: HashMap<String, String>,
}

/// `s` as a name is kept: no control characters, at most [`MAX_NAME`]
/// characters, trimmed.
pub fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect::<String>().trim().to_owned()
}

/// Asks the local network for `services`, from each of `from` (this
/// computer's IPv4 addresses), and gathers the answers for `wait`.
pub async fn browse(services: &[Service], from: &[Ipv4Addr], wait: Duration) -> std::io::Result<Vec<Found>> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)).into())?;
    socket.set_multicast_ttl_v4(255)?;
    let socket = UdpSocket::from_std(socket.into())?;
    let mut sent = false;
    for address in from {
        let raw = socket2::SockRef::from(&socket);
        if raw.set_multicast_if_v4(address).is_err() {
            continue;
        }
        for service in services {
            let query = mdns::write_query(&name(service), mdns::TYPE_PTR);
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
            for device in read(services, &buf[..len], source.ip()) {
                found.insert(device.key(), device);
            }
        }
    })
    .await;
    Ok(found.into_values().collect())
}

fn name(service: &Service) -> Name {
    service.name.iter().map(|s| (*s).to_owned()).collect()
}

/// Whether `ip` is on a local network: a private or link-local address.
fn local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(_) => false,
    }
}

/// The devices a response from `source` describes.
pub fn read(services: &[Service], packet: &[u8], source: IpAddr) -> Vec<Found> {
    let Some(records) = mdns::records(packet) else { return Vec::new() };
    let mut out = Vec::new();
    for service in services {
        let service_name = name(service);
        let instances = records.iter().filter_map(|(name, data)| match data {
            Data::Ptr(instance) if mdns::same(name, &service_name) => Some(instance),
            _ => None,
        });
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
            let announced = Announced {
                instance: instance.first().map(|label| clean(label)).unwrap_or_default(),
                address: SocketAddr::new(named.unwrap_or(source), port),
                txt,
            };
            out.extend((service.read)(&announced));
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::mdns::Record;

    fn name(text: &str) -> Name {
        text.split('.').map(str::to_owned).collect()
    }

    /// An answer for `instance` of `service`, at `address` and `port`.
    pub(crate) fn response(service: &str, instance: &str, port: u16, txt: Vec<&str>, address: Ipv4Addr) -> Vec<u8> {
        let instance = name(&format!("{instance}.{service}"));
        let host = name("device.local");
        let ptr = Record { name: name(service), data: Data::Ptr(instance.clone()), ttl: 120, unique: false };
        let rest = [
            Record { name: instance.clone(), data: Data::Srv { port, target: host.clone() }, ttl: 120, unique: true },
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

    fn everything(a: &Announced) -> Option<Found> {
        Some(Found {
            id: a.txt.get("id")?.clone(),
            name: a.instance.clone(),
            model: None,
            address: a.address,
            video: true,
            protocol: crate::nearby::Protocol::Cast,
        })
    }

    #[test]
    fn an_announcement_is_read_within_bounds() {
        let services = [Service { name: &["_test", "_tcp", "local"], read: everything }];
        let source = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 30));
        let here = response("_test._tcp.local", "Den", 7000, vec!["ID=1"], Ipv4Addr::new(192, 168, 1, 31));
        let found = read(&services, &here, source);
        assert_eq!((found[0].name.as_str(), found[0].address), ("Den", "192.168.1.31:7000".parse().unwrap()));
        // An address off the local network: the one it answered from.
        let elsewhere = response("_test._tcp.local", "Den", 7000, vec!["id=1"], Ipv4Addr::new(8, 8, 8, 8));
        assert_eq!(read(&services, &elsewhere, source)[0].address, "192.168.1.30:7000".parse().unwrap());
        // Another service's announcement is no device of this one's.
        let other = response("_other._tcp.local", "Den", 7000, vec!["id=1"], Ipv4Addr::new(192, 168, 1, 31));
        assert!(read(&services, &other, source).is_empty());
        assert_eq!(clean(&format!("\u{7}{}", "x".repeat(300))).len(), MAX_NAME);
        assert!(!local("8.8.8.8".parse().unwrap()) && local("10.0.0.2".parse().unwrap()));
    }
}
