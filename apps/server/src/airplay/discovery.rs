//! Finding AirPlay devices on the local network: they announce
//! `_airplay._tcp` by mDNS, which [`crate::nearby::discovery`] asks for,
//! under the name their owner gave them, and say what they are in its TXT
//! record.

use crate::nearby::discovery::{Announced, Service, clean};
use crate::nearby::{Found, Protocol};

pub const SERVICE: Service = Service { name: &["_airplay", "_tcp", "local"], read };

/// The features (`features`, two 32-bit hex numbers, low then high) of a
/// device that plays video from an address, and pairs as HomeKit does
/// (either of two bits says so).
const VIDEO: u64 = 1;
const HOMEKIT_PAIRING: u64 = 1 << 46;
const COREUTILS_PAIRING: u64 = 1 << 48;

/// The AirPlay device an announcement describes: by its `deviceid` (a MAC
/// address), its `model` and its `features`. Speakers, and receivers too
/// old to pair as Pley does, show no video here.
fn read(announced: &Announced) -> Option<Found> {
    let txt = &announced.txt;
    let id: String = txt.get("deviceid")?.chars().filter(|c| *c != ':').collect();
    if id.len() != 12 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let name = Some(announced.instance.clone()).filter(|n| !n.is_empty())?;
    let model = txt.get("model").map(|m| clean(m)).filter(|m| !m.is_empty());
    let features = txt.get("features").map_or(0, |f| features(f));
    let video = features & VIDEO != 0 && features & (HOMEKIT_PAIRING | COREUTILS_PAIRING) != 0;
    Some(Found {
        id: id.to_ascii_lowercase(),
        name,
        model,
        address: announced.address,
        video,
        protocol: Protocol::AirPlay,
    })
}

/// `0x5A7FFFF7,0x1E` as one number; 0 for what doesn't read.
fn features(text: &str) -> u64 {
    let half = |s: &str| u32::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok();
    let mut parts = text.split(',');
    let low = parts.next().and_then(half).unwrap_or(0);
    let high = parts.next().and_then(half).unwrap_or(0);
    u64::from(high) << 32 | u64::from(low)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;
    use crate::nearby::discovery;

    fn read(txt: Vec<&str>) -> Vec<Found> {
        let source = Ipv4Addr::new(192, 168, 1, 40);
        let packet = discovery::tests::response("_airplay._tcp.local", "Basement Apple TV", 7000, txt, source);
        discovery::read(&[SERVICE], &packet, IpAddr::V4(source))
    }

    #[test]
    fn an_apple_tv_is_read_from_its_answer_and_a_speaker_shows_no_video() {
        let tv = read(vec!["deviceid=A1:B2:C3:D4:E5:F6", "model=AppleTV14,1", "features=0x4A7FDFD5,0xBC177FDE"]);
        assert_eq!(
            tv,
            vec![Found {
                id: "a1b2c3d4e5f6".into(),
                name: "Basement Apple TV".into(),
                model: Some("AppleTV14,1".into()),
                address: "192.168.1.40:7000".parse().unwrap(),
                video: true,
                protocol: Protocol::AirPlay,
            }]
        );
        let speaker =
            read(vec!["deviceid=A1:B2:C3:D4:E5:F7", "model=AudioAccessory6,1", "features=0x4A7FCA00,0xBC354BD0"]);
        assert!(!speaker[0].video);
        // Video, but no pairing Pley speaks: an old receiver.
        let old = read(vec!["deviceid=A1:B2:C3:D4:E5:F8", "model=AppleTV3,2", "features=0x5A7FFFF7,0x0"]);
        assert!(!old[0].video);
        assert!(read(vec!["deviceid=nonsense", "features=0x1,0x10000"]).is_empty());
        assert_eq!(features("0x1,0x10000"), 1 | COREUTILS_PAIRING);
    }
}
