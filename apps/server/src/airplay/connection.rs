//! A connection to an AirPlay device: HTTP and RTSP requests over one TCP
//! stream, in the clear until pairing gives it keys, then in frames of at
//! most 1 KiB, each sealed with its own count as the nonce. What the
//! device sends is read within bounds.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

use super::crypto;

/// The most a message's head and body may hold, in bytes.
const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 1024 * 1024;
/// The most a sealed frame holds.
const FRAME: usize = 1024;
/// How long connecting and an answer may take.
pub const CONNECT_WITHIN: Duration = Duration::from_secs(5);
pub const ANSWER_WITHIN: Duration = Duration::from_secs(15);

/// The User-Agent of Apple's own senders of the version that plays
/// through the play queue (tvOS 26 and later want no older one).
pub const USER_AGENT: &str = "AirPlay/870.14.1";

/// A message: a request's or a response's first line, its headers (names
/// in lowercase) and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub first: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Message {
    /// A response's status: 0 when it isn't one.
    pub fn status(&self) -> u16 {
        self.first.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// The keys of an encrypted channel, and how many frames each way used.
struct Keys {
    write: [u8; 32],
    read: [u8; 32],
    written: u64,
    read_count: u64,
}

/// Which way a channel's keys go: Pley's control connection writes with
/// the "Write" key; the event connection, which the device opened the
/// other way, with the "Read" one.
#[derive(Debug, Clone, Copy)]
pub enum Keyed {
    Control,
    Events,
}

pub struct Channel<S> {
    stream: S,
    keys: Option<Keys>,
    /// Read and opened, not yet taken.
    buffered: Vec<u8>,
    cseq: u32,
    /// The headers every RTSP request carries.
    rtsp: Vec<(&'static str, String)>,
}

impl Channel<TcpStream> {
    pub async fn connect(address: SocketAddr) -> io::Result<Channel<TcpStream>> {
        let stream = tokio::time::timeout(CONNECT_WITHIN, TcpStream::connect(address))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the device didn't answer"))??;
        stream.set_nodelay(true)?;
        Ok(Channel::over(stream))
    }

    pub fn local_address(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> Channel<S> {
    pub fn over(stream: S) -> Channel<S> {
        Channel { stream, keys: None, buffered: Vec::new(), cseq: 0, rtsp: Vec::new() }
    }

    /// Seals what is sent from here on with keys from `secret`.
    pub fn encrypt(&mut self, secret: &[u8], keyed: Keyed) {
        let (salt, write, read) = match keyed {
            Keyed::Control => ("Control-Salt", "Control-Write-Encryption-Key", "Control-Read-Encryption-Key"),
            Keyed::Events => ("Events-Salt", "Events-Read-Encryption-Key", "Events-Write-Encryption-Key"),
        };
        self.keys = Some(Keys {
            write: crypto::derive(secret, salt, write),
            read: crypto::derive(secret, salt, read),
            written: 0,
            read_count: 0,
        });
    }

    /// Headers each RTSP request carries from here on (the sender's ids).
    pub fn rtsp_headers(&mut self, headers: Vec<(&'static str, String)>) {
        self.rtsp = headers;
    }

    async fn send(&mut self, data: &[u8]) -> io::Result<()> {
        match &mut self.keys {
            None => self.stream.write_all(data).await?,
            Some(keys) => {
                let mut out = Vec::with_capacity(data.len() + data.len() / FRAME * 18 + 18);
                for chunk in data.chunks(FRAME) {
                    let aad = (chunk.len() as u16).to_le_bytes();
                    out.extend_from_slice(&aad);
                    out.extend(crypto::seal(&keys.write, crypto::counted_nonce(keys.written), &aad, chunk));
                    keys.written += 1;
                }
                self.stream.write_all(&out).await?;
            }
        }
        self.stream.flush().await
    }

    /// Reads until `buffered` holds more than `have` bytes.
    async fn more(&mut self, have: usize) -> io::Result<()> {
        while self.buffered.len() <= have {
            match &mut self.keys {
                None => {
                    let mut buf = [0u8; 4096];
                    let got = self.stream.read(&mut buf).await?;
                    if got == 0 {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                    self.buffered.extend_from_slice(&buf[..got]);
                }
                Some(keys) => {
                    let mut len = [0u8; 2];
                    self.stream.read_exact(&mut len).await?;
                    let size = usize::from(u16::from_le_bytes(len));
                    if size > FRAME {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "a frame too large"));
                    }
                    let mut sealed = vec![0u8; size + 16];
                    self.stream.read_exact(&mut sealed).await?;
                    let plain = crypto::open(&keys.read, crypto::counted_nonce(keys.read_count), &len, &sealed)
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "a frame that doesn't open"))?;
                    keys.read_count += 1;
                    self.buffered.extend_from_slice(&plain);
                }
            }
        }
        Ok(())
    }

    /// The next message the device sends: a response, or a request on the
    /// event connection.
    pub async fn read(&mut self) -> io::Result<Message> {
        let end = loop {
            if let Some(end) = self.buffered.windows(4).position(|w| w == b"\r\n\r\n") {
                break end;
            }
            if self.buffered.len() > MAX_HEAD {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "a head too large"));
            }
            self.more(self.buffered.len()).await?;
        };
        let head = String::from_utf8_lossy(&self.buffered[..end]).into_owned();
        self.buffered.drain(..end + 4);
        let mut lines = head.split("\r\n");
        let first = lines.next().unwrap_or_default().to_owned();
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
            .collect();
        let len: usize =
            headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
        if len > MAX_BODY {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "a body too large"));
        }
        if len > 0 {
            self.more(len - 1).await?;
        }
        let body = self.buffered.drain(..len).collect();
        Ok(Message { first, headers, body })
    }

    /// Sends a request and reads its answer. `rtsp` requests carry a CSeq
    /// and the sender's ids.
    pub async fn request(
        &mut self,
        method: &str,
        path: &str,
        rtsp: bool,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> io::Result<Message> {
        let protocol = if rtsp { "RTSP/1.0" } else { "HTTP/1.1" };
        let mut head =
            format!("{method} {path} {protocol}\r\nContent-Length: {}\r\nUser-Agent: {USER_AGENT}\r\n", body.len());
        if rtsp {
            self.cseq += 1;
            head.push_str(&format!("CSeq: {}\r\n", self.cseq));
            for (name, value) in &self.rtsp {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
        }
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let mut data = head.into_bytes();
        data.extend_from_slice(body);
        tokio::time::timeout(ANSWER_WITHIN, async {
            self.send(&data).await?;
            self.read().await
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the device didn't answer"))?
    }

    /// Answers a request the device sent on the event connection.
    pub async fn answer(&mut self, request: &Message) -> io::Result<()> {
        let cseq = request.header("cseq").unwrap_or("0");
        self.reply("RTSP/1.0 200 OK", &[("Audio-Latency", "0"), ("CSeq", cseq)], &[]).await
    }

    /// Sends a response: its status line, headers and body.
    pub async fn reply(&mut self, status: &str, headers: &[(&str, &str)], body: &[u8]) -> io::Result<()> {
        let mut head = format!("{status}\r\nContent-Length: {}\r\n", body.len());
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let mut data = head.into_bytes();
        data.extend_from_slice(body);
        self.send(&data).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn requests_and_answers_cross_sealed_both_ways() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (mut ours, mut theirs) = (Channel::over(ours), Channel::over(theirs));
        ours.encrypt(b"shared", Keyed::Control);
        // The device's end of the control connection: Pley's keys, swapped.
        theirs.keys = Some(Keys {
            write: crypto::derive(b"shared", "Control-Salt", "Control-Read-Encryption-Key"),
            read: crypto::derive(b"shared", "Control-Salt", "Control-Write-Encryption-Key"),
            written: 0,
            read_count: 0,
        });
        ours.rtsp_headers(vec![("DACP-ID", "1A".into())]);
        let body = vec![b'x'; 3000];
        let device = async {
            let request = theirs.read().await.unwrap();
            assert_eq!(request.first, "SETUP rtsp://pley/1 RTSP/1.0");
            assert_eq!((request.header("CSeq"), request.header("dacp-id")), (Some("1"), Some("1A")));
            assert_eq!(request.body.len(), 3000);
            theirs.answer(&request).await.unwrap();
        };
        let (answer, ()) = tokio::join!(ours.request("SETUP", "rtsp://pley/1", true, &[], &body), device);
        let answer = answer.unwrap();
        assert_eq!((answer.status(), answer.header("cseq")), (200, Some("1")));
    }

    #[tokio::test]
    async fn what_is_too_large_or_tampered_with_ends_the_read() {
        let (ours, mut theirs) = tokio::io::duplex(64 * 1024);
        let mut ours = Channel::over(ours);
        theirs
            .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1).as_bytes())
            .await
            .unwrap();
        assert!(ours.read().await.is_err());

        let (ours, mut theirs) = tokio::io::duplex(64 * 1024);
        let mut ours = Channel::over(ours);
        ours.encrypt(b"shared", Keyed::Control);
        theirs.write_all(&[4, 0, 1, 2, 3, 4]).await.unwrap();
        theirs.write_all(&[0; 16]).await.unwrap();
        assert!(ours.read().await.is_err());
    }
}
