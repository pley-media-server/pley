//! Server addresses: origins, what someone typed, and which hosts are on the
//! home network (packages/tv/src/auth/addresses.ts, the Android app's
//! `Addresses.kt`).
//!
//! A server is named by its origin (`http://192.168.1.20:7539`,
//! `https://pley.example.com`): its issuer, the key of its tokens and of
//! everything kept for it, and the only address the bearer token goes to.

use pley_i18n::t;

/// Pley's web port, for addresses typed without one.
pub const DEFAULT_PORT: u16 = 7539;

/// Why a TV won't sign in over plain http beyond the home network (the Android app's words), in
/// the language shown.
pub fn unencrypted_refusal() -> &'static str {
    t!("errors.signIn.unencrypted")
}

/// An absolute http(s) address taken apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Url {
    /// `http` or `https`, lower case.
    pub scheme: String,
    /// Lower case; IPv6 in brackets (`[fd00::1]`).
    pub host: String,
    /// The port as written; none when left out.
    pub port: Option<u16>,
    /// Path and query, starting with `/` (the fragment left out).
    pub path: String,
}

impl Url {
    /// `url` taken apart; none for anything that isn't an absolute http or https address.
    pub fn parse(url: &str) -> Option<Url> {
        let text = url.trim();
        let (scheme, rest) = text.split_once("://")?;
        let mut chars = scheme.chars();
        if !chars.next()?.is_ascii_alphabetic() || !chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)) {
            return None;
        }
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return None;
        }
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
            let close = inner.find(']')?;
            let literal = &inner[..close];
            if literal.is_empty() || !literal.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.') {
                return None;
            }
            (format!("[{}]", literal.to_ascii_lowercase()), &inner[close + 1..])
        } else {
            let at = authority.find(':').unwrap_or(authority.len());
            let host = &authority[..at];
            if host.is_empty() || host.chars().any(|c| "/?#:@".contains(c) || c.is_whitespace()) {
                return None;
            }
            (host.to_lowercase(), &authority[at..])
        };
        let port = match port {
            "" => None,
            p => {
                let digits = p.strip_prefix(':')?;
                if digits.is_empty() || digits.len() > 5 || !digits.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let n: u32 = digits.parse().ok()?;
                if !(1..=65535).contains(&n) {
                    return None;
                }
                Some(n as u16)
            }
        };
        if tail.chars().any(char::is_whitespace) {
            return None;
        }
        let path = tail.split('#').next().unwrap_or("");
        let path = if path.is_empty() {
            "/".to_string()
        } else if path.starts_with('?') {
            format!("/{path}")
        } else {
            path.to_string()
        };
        Some(Url { scheme, host, port, path })
    }

    /// The port connected to: the one written, else the scheme's own.
    pub fn port_or_default(&self) -> u16 {
        self.port.unwrap_or(if self.scheme == "https" { 443 } else { 80 })
    }

    /// The host without IPv6 brackets, as sockets and certificates name it.
    pub fn bare_host(&self) -> &str {
        self.host.trim_start_matches('[').trim_end_matches(']')
    }

    /// Whether the scheme is https.
    pub fn is_https(&self) -> bool {
        self.scheme == "https"
    }

    /// The origin as browsers write it: the port only when it isn't the scheme's own.
    pub fn origin(&self) -> String {
        let default = match self.port {
            None => true,
            Some(p) => (self.scheme == "http" && p == 80) || (self.scheme == "https" && p == 443),
        };
        match self.port {
            Some(p) if !default => format!("{}://{}:{}", self.scheme, self.host, p),
            _ => format!("{}://{}", self.scheme, self.host),
        }
    }

    /// The `Host` header: the host, and the port unless it is the scheme's own.
    pub fn host_header(&self) -> String {
        let origin = self.origin();
        origin.split_once("://").map_or(origin.clone(), |(_, h)| h.to_string())
    }
}

/// The origin of `url` (`https://pley.example.com`, `http://nas:7539`):
/// scheme and host in lower case, the port only when it isn't the scheme's own.
pub fn origin(url: &str) -> Option<String> {
    Url::parse(url).map(|u| u.origin())
}

fn has_scheme(text: &str) -> bool {
    text.split_once("://").is_some_and(|(s, _)| {
        let mut c = s.chars();
        c.next().is_some_and(|f| f.is_ascii_alphabetic()) && c.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    })
}

/// `nas`, `nas:8080`, `192.168.1.20` or `https://pley.example.com/` as a
/// server's origin; none when it can't be one. Without a scheme it is http,
/// and without a port Pley's.
pub fn normalize_server(input: &str) -> Option<String> {
    let text = input.trim();
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return None;
    }
    let typed_scheme = has_scheme(text);
    let mut url = Url::parse(&if typed_scheme { text.to_string() } else { format!("http://{text}") })?;
    if url.port.is_none() && !typed_scheme {
        url.port = Some(DEFAULT_PORT);
    }
    Some(url.origin())
}

fn ipv4_prefix(host: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 || !parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
}

/// Whether `host` is this device, on its local network (a private address, a
/// name without dots, `.local`…) or a Tailscale name: where plain http is how
/// Pley is reached and the network itself is trusted.
pub fn is_local_host(host: &str) -> bool {
    let h = host.to_lowercase();
    let h = h.trim_start_matches('[').trim_end_matches(']');
    if h == "localhost" || (!h.contains('.') && !h.contains(':')) {
        return true;
    }
    const SUFFIXES: [&str; 7] = [".local", ".lan", ".home", ".internal", ".localdomain", ".home.arpa", ".ts.net"];
    if SUFFIXES.iter().any(|s| h.ends_with(s)) {
        return true;
    }
    if let Some((a, b)) = ipv4_prefix(h) {
        return a == 10
            || a == 127
            || (a == 172 && (16..=31).contains(&b))
            || (a == 192 && b == 168)
            || (a == 169 && b == 254)
            || (a == 100 && (64..=127).contains(&b));
    }
    if h == "::1" {
        return true;
    }
    let b = h.as_bytes();
    let hex = |c: u8| c.is_ascii_hexdigit();
    // fc00::/7 (unique local) and fe80::/10 (link-local), as written before the first colon.
    let ula = b.len() >= 3 && b[0] == b'f' && (b[1] == b'c' || b[1] == b'd') && {
        let colon = h.find(':');
        colon.is_some_and(|i| (2..=4).contains(&i) && b[2..i].iter().all(|&c| hex(c)))
    };
    let link = b.len() >= 4 && b[0] == b'f' && b[1] == b'e' && b"89ab".contains(&b[2]) && {
        let colon = h.find(':');
        colon.is_some_and(|i| (3..=4).contains(&i) && b[3..i].iter().all(|&c| hex(c)))
    };
    ula || link
}

/// Whether `url`'s server is on the home network itself, which accepts a
/// connection at once or never: not a Tailscale address, which may be far
/// away (the Android app's `LocalRoute.isHomeHost`). Such servers get a 3 s
/// connect timeout.
pub fn at_home(url: &str) -> bool {
    let Some(u) = Url::parse(url) else { return false };
    let h = u.bare_host();
    if h.ends_with(".ts.net") {
        return false;
    }
    if let Some((100, b)) = ipv4_prefix(h)
        && (64..=127).contains(&b)
    {
        return false;
    }
    is_local_host(h)
}

/// Whether a connection to `url` would travel unencrypted beyond the local network.
pub fn unencrypted(url: &str) -> bool {
    Url::parse(url).is_some_and(|u| u.scheme == "http" && !is_local_host(&u.host))
}

/// The address to show for a server: its host and, unless it is Pley's, its port.
pub fn display_address(url: &str) -> String {
    match Url::parse(url) {
        None => url.to_string(),
        Some(u) => match u.port {
            Some(p) if !(u.scheme == "http" && p == DEFAULT_PORT) => format!("{}:{}", u.host, p),
            _ => u.host,
        },
    }
}

/// `host:port` as the server list shows it, never the scheme
/// (`192.168.1.20:7539`, `pley.example.com`).
pub fn host_port(url: &str) -> String {
    match origin(url) {
        Some(o) => o.split_once("://").map_or(o.clone(), |(_, h)| h.to_string()),
        None => url.to_string(),
    }
}

/// The addresses to try, in turn, for what someone typed: a name on the
/// internet typed without `http://` is tried with https first (Pley behind a
/// proxy with a certificate), then with plain http on Pley's port.
pub fn candidates(input: &str) -> Vec<String> {
    let Some(url) = normalize_server(input) else { return Vec::new() };
    let text = input.trim();
    let typed_scheme = has_scheme(text);
    let first = text.split('/').next().unwrap_or("");
    let typed_port =
        first.rsplit_once(':').is_some_and(|(_, p)| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let Some(parsed) = Url::parse(&url) else { return Vec::new() };
    if typed_scheme || typed_port || is_local_host(&parsed.host) {
        return vec![url];
    }
    vec![format!("https://{}", parsed.host), url]
}

/// Whether two addresses are the same server.
pub fn same_server(a: &str, b: &str) -> bool {
    match (origin(a), origin(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_drop_default_ports_and_paths() {
        assert_eq!(origin("HTTPS://Pley.Example.com:443/api/x?y#z").as_deref(), Some("https://pley.example.com"));
        assert_eq!(origin("http://nas:7539/").as_deref(), Some("http://nas:7539"));
        assert_eq!(origin("http://nas:80").as_deref(), Some("http://nas"));
        assert_eq!(origin("http://[FD00::1]:7539").as_deref(), Some("http://[fd00::1]:7539"));
        assert_eq!(origin("ftp://nas"), None);
        assert_eq!(origin("http://user@nas"), None);
        assert_eq!(origin("http://nas:0"), None);
        assert_eq!(origin("http://nas:70000"), None);
        assert_eq!(origin("nas:7539"), None);
    }

    #[test]
    fn typed_addresses_get_http_and_pleys_port() {
        assert_eq!(normalize_server(" nas ").as_deref(), Some("http://nas:7539"));
        assert_eq!(normalize_server("nas:8080").as_deref(), Some("http://nas:8080"));
        assert_eq!(normalize_server("192.168.1.20").as_deref(), Some("http://192.168.1.20:7539"));
        assert_eq!(normalize_server("https://pley.example.com/").as_deref(), Some("https://pley.example.com"));
        assert_eq!(normalize_server("http://nas").as_deref(), Some("http://nas"));
        assert_eq!(normalize_server("my nas"), None);
        assert_eq!(normalize_server(""), None);
    }

    #[test]
    fn home_hosts() {
        for h in [
            "localhost",
            "nas",
            "nas.local",
            "box.lan",
            "media.home.arpa",
            "nas.tail1234.ts.net",
            "10.0.0.2",
            "127.0.0.1",
            "172.16.0.1",
            "172.31.255.1",
            "192.168.1.20",
            "169.254.3.4",
            "100.64.0.1",
            "100.127.1.1",
            "[::1]",
            "fd00::1",
            "fc12:3::1",
            "fe80::1",
            "feb0::1",
        ] {
            assert!(is_local_host(h), "{h} is local");
        }
        for h in ["pley.example.com", "8.8.8.8", "172.32.0.1", "100.128.0.1", "2001:db8::1", "fec0::1", "192.169.0.1"] {
            assert!(!is_local_host(h), "{h} isn't local");
        }
    }

    #[test]
    fn tailscale_isnt_at_home() {
        assert!(at_home("http://192.168.1.20:7539"));
        assert!(at_home("http://nas.local:7539"));
        assert!(!at_home("http://100.101.102.103:7539"));
        assert!(!at_home("http://nas.tail1234.ts.net:7539"));
        assert!(!at_home("https://pley.example.com"));
    }

    #[test]
    fn plain_http_beyond_home_is_unencrypted() {
        assert!(unencrypted("http://pley.example.com:7539"));
        assert!(!unencrypted("https://pley.example.com"));
        assert!(!unencrypted("http://192.168.1.20:7539"));
    }

    #[test]
    fn display_addresses() {
        assert_eq!(display_address("http://192.168.1.20:7539"), "192.168.1.20");
        assert_eq!(display_address("https://pley.example.com"), "pley.example.com");
        assert_eq!(display_address("http://nas:8080"), "nas:8080");
        assert_eq!(host_port("http://192.168.1.20:7539"), "192.168.1.20:7539");
        assert_eq!(host_port("https://pley.example.com"), "pley.example.com");
    }

    #[test]
    fn names_on_the_internet_try_https_first() {
        assert_eq!(candidates("pley.example.com"), ["https://pley.example.com", "http://pley.example.com:7539"]);
        assert_eq!(candidates("pley.example.com:8443"), ["http://pley.example.com:8443"]);
        assert_eq!(candidates("http://pley.example.com"), ["http://pley.example.com"]);
        assert_eq!(candidates("nas"), ["http://nas:7539"]);
        assert_eq!(candidates("192.168.1.20"), ["http://192.168.1.20:7539"]);
        assert!(candidates("not an address").is_empty());
    }

    #[test]
    fn urls_split_into_parts() {
        let u = Url::parse("https://pley.example.com:8443/api/items/1?w=320#x").unwrap();
        assert_eq!(u.port_or_default(), 8443);
        assert_eq!(u.path, "/api/items/1?w=320");
        assert_eq!(u.host_header(), "pley.example.com:8443");
        let u = Url::parse("http://nas?x=1").unwrap();
        assert_eq!(u.path, "/?x=1");
        assert_eq!(u.port_or_default(), 80);
        assert_eq!(u.host_header(), "nas");
        assert!(same_server("http://NAS:7539/api", "http://nas:7539"));
    }
}
