//! A small HTTP/1.1 client for the tests: one request a connection, answers
//! read whole (sized or chunked), and the event stream read as it comes.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use pley_fixtures::{FixtureServer, ids};
use serde_json::Value;

/// An answer.
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}), status {}: {}", self.status, self.text()))
    }
}

/// Sends one request to `base` (`http://127.0.0.1:<port>`).
pub fn send(base: &str, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Reply {
    let address = base.strip_prefix("http://").expect("an http:// address");
    let mut stream = TcpStream::connect(address).expect("the fixture server listens");
    stream.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n");
    if !body.is_empty() || matches!(method, "POST" | "PUT" | "PATCH") {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    parse(&raw, method == "HEAD")
}

fn parse(raw: &[u8], head_only: bool) -> Reply {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("a whole answer head");
    let head = String::from_utf8_lossy(&raw[..end]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
    let headers: Vec<(String, String)> =
        lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
    let rest = &raw[end + 4..];
    let chunked = headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    let body = if head_only {
        Vec::new()
    } else if chunked {
        dechunk(rest)
    } else {
        rest.to_vec()
    };
    Reply { status, headers, body }
}

fn dechunk(mut data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(line_end) = data.windows(2).position(|w| w == b"\r\n") else { return out };
        let size = usize::from_str_radix(String::from_utf8_lossy(&data[..line_end]).trim(), 16).unwrap_or(0);
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&data[line_end + 2..line_end + 2 + size]);
        data = &data[line_end + 2 + size + 2..];
    }
}

/// `a=b&c=d`, encoded.
pub fn form_body(pairs: &[(&str, &str)]) -> Vec<u8> {
    let encode = |text: &str| {
        text.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>()
    };
    pairs.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&").into_bytes()
}

/// POSTs a form, as the OAuth endpoints take them.
pub fn post_form(base: &str, path: &str, pairs: &[(&str, &str)]) -> Reply {
    send(base, "POST", path, &[("Content-Type", "application/x-www-form-urlencoded")], &form_body(pairs))
}

/// A signed-in app.
pub struct Api {
    pub base: String,
    pub token: String,
}

impl Api {
    pub fn new(server: &FixtureServer, profile: i64) -> Api {
        Api { base: server.url().to_string(), token: server.access_token(profile) }
    }

    pub fn alex(server: &FixtureServer) -> Api {
        Api::new(server, ids::ALEX)
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.token)
    }

    pub fn get(&self, path: &str) -> Reply {
        send(&self.base, "GET", path, &[("Authorization", &self.auth())], b"")
    }

    pub fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> Reply {
        let auth = self.auth();
        let mut all = vec![("Authorization", auth.as_str())];
        all.extend_from_slice(headers);
        send(&self.base, "GET", path, &all, b"")
    }

    pub fn head(&self, path: &str) -> Reply {
        send(&self.base, "HEAD", path, &[("Authorization", &self.auth())], b"")
    }

    pub fn json(&self, method: &str, path: &str, body: &Value) -> Reply {
        let text = body.to_string();
        send(
            &self.base,
            method,
            path,
            &[("Authorization", &self.auth()), ("Content-Type", "application/json")],
            text.as_bytes(),
        )
    }

    pub fn post(&self, path: &str, body: &Value) -> Reply {
        self.json("POST", path, body)
    }

    pub fn put(&self, path: &str, body: &Value) -> Reply {
        self.json("PUT", path, body)
    }

    pub fn delete(&self, path: &str) -> Reply {
        send(&self.base, "DELETE", path, &[("Authorization", &self.auth())], b"")
    }
}

/// The event stream, read as it comes.
pub struct Events {
    reader: BufReader<TcpStream>,
    pub status: u16,
}

impl Events {
    pub fn open(api: &Api) -> Events {
        let address = api.base.strip_prefix("http://").unwrap();
        let mut stream = TcpStream::connect(address).unwrap();
        stream.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        let head = format!(
            "GET /api/events HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {}\r\nAccept: text/event-stream\r\n\r\n",
            api.token
        );
        stream.write_all(head.as_bytes()).unwrap();
        let mut reader = BufReader::new(stream);
        let mut status = 0;
        let started = Instant::now();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) if status == 0 => status = line.split(' ').nth(1).unwrap().parse().unwrap(),
                Ok(_) if line == "\r\n" => break,
                Ok(_) => {}
                Err(_) if started.elapsed() < Duration::from_secs(10) => {}
                Err(e) => panic!("no answer from the event stream: {e}"),
            }
        }
        Events { reader, status }
    }

    /// The next `data:` line's JSON, waiting up to `wait`.
    pub fn next(&mut self, wait: Duration) -> Option<Value> {
        let started = Instant::now();
        let mut line = String::new();
        while started.elapsed() < wait {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {
                    if let Some(data) = line.trim_end().strip_prefix("data: ") {
                        return serde_json::from_str(data).ok();
                    }
                }
                Err(_) => {}
            }
        }
        None
    }

    /// Waits for an event of `kind`, skipping others.
    pub fn wait_for(&mut self, kind: &str, wait: Duration) -> Option<Value> {
        let started = Instant::now();
        while started.elapsed() < wait {
            if let Some(event) = self.next(wait.saturating_sub(started.elapsed()))
                && event["type"] == kind
            {
                return Some(event);
            }
        }
        None
    }
}

/// Waits until `check` holds, up to two seconds.
pub fn eventually(check: impl Fn() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        if check() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}
