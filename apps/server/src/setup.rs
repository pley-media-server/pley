//! First run: the administrator account is made with a one-time setup code,
//! which the server prints in its log (in a link that fills it in) and the
//! tray icon's "Open pley" passes along. Nobody else can claim a new server
//! before its owner gets to it, wherever requests seem to come from: Docker
//! and reverse proxies hide that, and a web page can reach servers on its
//! visitor's network (DNS rebinding).

use std::net::{IpAddr, Ipv4Addr};

use parking_lot::Mutex;

/// Letters and digits that can't be mistaken for one another.
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const LENGTH: usize = 8;

/// The setup code, while the server has no account.
#[derive(Default)]
pub struct SetupCode(Mutex<Option<String>>);

impl SetupCode {
    /// A new code (`ABCD-EFGH`), replacing any earlier one.
    pub fn issue(&self) -> String {
        let mut bytes = [0u8; LENGTH];
        getrandom::fill(&mut bytes).expect("the system's random numbers");
        let code: String = bytes.iter().map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char).collect();
        *self.0.lock() = Some(code.clone());
        format!("{}-{}", &code[..4], &code[4..])
    }

    /// The code as shown (`ABCD-EFGH`), while there is one.
    pub fn shown(&self) -> Option<String> {
        self.0.lock().as_deref().map(|code| format!("{}-{}", &code[..4], &code[4..]))
    }

    /// Whether `given` is the code, in any case, with or without the dash.
    pub fn matches(&self, given: &str) -> bool {
        let given: String =
            given.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
        let current = self.0.lock();
        current.as_deref().is_some_and(|code| crate::util::constant_time_eq(code.as_bytes(), given.as_bytes()))
    }

    /// Setup is done: the code no longer claims anything.
    pub fn clear(&self) {
        *self.0.lock() = None;
    }
}

/// Loopback, private, link-local or Tailscale (carrier-grade NAT) addresses.
pub fn is_local_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local() || is_shared_address(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_local_address(IpAddr::V4(v4)),
            None => {
                let first = v6.segments()[0];
                v6.is_loopback()
                    // Unique local (fc00::/7) and link-local (fe80::/10) addresses.
                    || (first & 0xfe00) == 0xfc00
                    || (first & 0xffc0) == 0xfe80
            }
        },
    }
}

/// The carrier-grade NAT space (100.64.0.0/10), which Tailscale uses for its
/// devices and internet providers for their subscribers.
pub fn is_shared_address(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    a == 100 && (64..128).contains(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_once_issued_whatever_the_case_or_dash() {
        let code = SetupCode::default();
        assert!(!code.matches(""), "no code before one is issued");
        let shown = code.issue();
        assert_eq!(code.shown().as_deref(), Some(shown.as_str()));
        assert_eq!(shown.len(), LENGTH + 1);
        assert_eq!(&shown[4..5], "-");
        assert!(shown.chars().filter(|c| *c != '-').all(|c| ALPHABET.contains(&(c as u8))));
        assert!(code.matches(&shown));
        assert!(code.matches(&shown.replace('-', "").to_lowercase()));
        assert!(code.matches(&format!(" {shown} ")));
        assert!(!code.matches(&shown[..LENGTH]), "part of it");
        assert!(!code.matches("AAAA-AAAA"));
        let again = code.issue();
        assert!(code.matches(&again));
        code.clear();
        assert!(!code.matches(&again), "setup is done");
        assert_eq!(code.shown(), None);
    }

    #[test]
    fn local_addresses_are_this_computer_and_its_networks() {
        let local = |ip: &str| is_local_address(ip.parse().unwrap());
        for ip in ["127.0.0.1", "::1", "192.168.1.20", "10.0.0.7", "172.20.0.1", "169.254.3.4", "100.101.102.103"] {
            assert!(local(ip), "{ip}");
        }
        for ip in ["fd12:3456:789a::1", "fe80::1", "::ffff:192.168.1.20"] {
            assert!(local(ip), "{ip}");
        }
        for ip in ["8.8.8.8", "172.32.0.1", "100.128.0.1", "2001:4860:4860::8888", "::ffff:8.8.8.8"] {
            assert!(!local(ip), "{ip}");
        }
    }
}
