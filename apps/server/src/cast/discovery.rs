//! Finding Google Cast devices on the local network: they announce
//! `_googlecast._tcp` by mDNS, which [`crate::nearby::discovery`] asks
//! for, and say who they are in its TXT record.

use crate::nearby::discovery::{Announced, Service, clean};
use crate::nearby::{Found, Protocol};

/// The longest device id kept.
const MAX_ID: usize = 64;

pub const SERVICE: Service = Service { name: &["_googlecast", "_tcp", "local"], read };

/// The Cast device an announcement describes: by the `id`, `fn` (its
/// name), `md` (its model) and `ca` (its capabilities) in its TXT record.
fn read(announced: &Announced) -> Option<Found> {
    let txt = &announced.txt;
    let id = txt
        .get("id")
        .filter(|id| !id.is_empty() && id.len() <= MAX_ID && id.chars().all(|c| c.is_ascii_alphanumeric()))?;
    let model = txt.get("md").map(|m| clean(m)).filter(|m| !m.is_empty());
    let name = txt.get("fn").map(|n| clean(n)).filter(|n| !n.is_empty()).or_else(|| model.clone())?;
    // Its capabilities, a bit field: 1 is video out. A device that
    // doesn't say is taken to show video.
    let video = txt.get("ca").and_then(|ca| ca.parse::<u64>().ok()).is_none_or(|ca| ca & 1 != 0);
    Some(Found {
        id: id.to_ascii_lowercase(),
        name,
        model,
        address: announced.address,
        video,
        protocol: Protocol::Cast,
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;
    use crate::nearby::discovery::{self, MAX_NAME};

    fn response(txt: Vec<&str>, address: Ipv4Addr) -> Vec<u8> {
        discovery::tests::response("_googlecast._tcp.local", "Chromecast-9f8e", 8009, txt, address)
    }

    fn read(packet: &[u8], source: IpAddr) -> Vec<Found> {
        discovery::read(&[SERVICE], packet, source)
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
                protocol: Protocol::Cast,
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
    }
}
