//! A connection to a Google Cast device: TLS to its port (8009), the
//! messages of [`super::frame`] both ways, and the heartbeat that keeps it
//! open. What comes in is JSON from a device on the network: anything not
//! an object, or past [`super::frame::MAX_MESSAGE`], ends the connection.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_rustls::rustls;
use tokio_util::sync::CancellationToken;

use super::frame::{self, Message};

/// Who Pley is on the connection.
pub const SENDER: &str = "sender-pley";
/// The device itself, as opposed to an app running on it.
pub const PLATFORM: &str = "receiver-0";

pub const NS_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub const NS_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub const NS_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";

/// How often Pley pings, and how long the device may stay silent before
/// the connection is taken for lost (Google's senders: 5 s and 10 s; a
/// little more slack here, as a busy TV answers late).
const PING_EVERY: Duration = Duration::from_secs(5);
const SILENT_AT_MOST: Duration = Duration::from_secs(20);
/// How long connecting, TLS included, may take.
const CONNECT_WITHIN: Duration = Duration::from_secs(10);
/// Messages waiting to go, and come in, at most.
const QUEUE: usize = 64;

/// A message that came in: from whom, on what, saying what.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub source: String,
    pub namespace: String,
    pub payload: Value,
}

/// An open connection. Dropping it, or [`Connection::close`], ends it; the
/// receiving side then ends too.
pub struct Connection {
    out: mpsc::Sender<Message>,
    closed: CancellationToken,
    next_request: Arc<AtomicU64>,
}

impl Connection {
    /// Connects to the device at `address` over TLS. Cast devices show a
    /// certificate of their own making, which no authority signed: it isn't
    /// checked (nor is the device's own proof, which only Google's senders
    /// ask for). The address is one the device announced on the local
    /// network.
    pub async fn connect(address: SocketAddr) -> std::io::Result<(Connection, mpsc::Receiver<Incoming>)> {
        let config =
            rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(std::io::Error::other)?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(AnyCertificate))
                .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        // The name isn't checked; one is needed all the same.
        let name = rustls::pki_types::ServerName::IpAddress(address.ip().into());
        let stream = tokio::time::timeout(CONNECT_WITHIN, async {
            let tcp = tokio::net::TcpStream::connect(address).await?;
            tcp.set_nodelay(true)?;
            connector.connect(name, tcp).await
        })
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "the device didn't answer"))??;
        Ok(Connection::over(stream))
    }

    /// The connection over `stream`, already open: its messages start
    /// flowing.
    pub fn over<S>(stream: S) -> (Connection, mpsc::Receiver<Incoming>)
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let (out, mut outgoing) = mpsc::channel::<Message>(QUEUE);
        let (incoming_tx, received) = mpsc::channel::<Incoming>(QUEUE);
        let closed = CancellationToken::new();

        // Writes what is sent, and pings. A device that stops reading ends
        // the connection as one that stops answering does: a write waits
        // for it no longer than its silence is borne.
        let done = closed.clone();
        tokio::spawn(async move {
            let mut ping = tokio::time::interval(PING_EVERY);
            loop {
                let message = tokio::select! {
                    biased;
                    () = done.cancelled() => {
                        // What was sent before closing still goes (a STOP
                        // just before the end).
                        while let Ok(message) = outgoing.try_recv() {
                            if !write_within(&mut writer, &message).await {
                                break;
                            }
                        }
                        break;
                    }
                    message = outgoing.recv() => match message {
                        Some(message) => message,
                        None => break,
                    },
                    _ = ping.tick() => message(PLATFORM, NS_HEARTBEAT, &json!({"type": "PING"})),
                };
                if !write_within(&mut writer, &message).await {
                    break;
                }
            }
            done.cancel();
        });

        // Reads what comes, answers pings, and ends the connection when
        // the device goes quiet or Pley stops listening.
        let done = closed.clone();
        let pong = out.clone();
        tokio::spawn(async move {
            loop {
                let read = tokio::select! {
                    () = done.cancelled() => break,
                    read = tokio::time::timeout(SILENT_AT_MOST, frame::read(&mut reader)) => read,
                };
                let Ok(Ok(Some(message))) = read else { break };
                let Ok(payload @ Value::Object(_)) = serde_json::from_str::<Value>(&message.payload) else { break };
                if message.namespace == NS_HEARTBEAT {
                    if payload["type"] == "PING" {
                        let _ = pong.try_send(self::message(&message.source, NS_HEARTBEAT, &json!({"type": "PONG"})));
                    }
                    continue;
                }
                let incoming = Incoming { source: message.source, namespace: message.namespace, payload };
                let sent = tokio::select! {
                    () = done.cancelled() => break,
                    sent = incoming_tx.send(incoming) => sent,
                };
                if sent.is_err() {
                    break;
                }
            }
            done.cancel();
        });
        let connection = Connection { out, closed, next_request: Arc::new(AtomicU64::new(1)) };
        (connection, received)
    }

    /// Sends `payload` to `destination` on `namespace`. `false` once the
    /// connection has ended, also while waiting for room to send it.
    pub async fn send(&self, destination: &str, namespace: &str, payload: &Value) -> bool {
        let message = message(destination, namespace, payload);
        tokio::select! {
            biased;
            () = self.closed.cancelled() => false,
            sent = self.out.send(message) => sent.is_ok(),
        }
    }

    /// Sends `payload` with a `requestId` of its own, which the answer
    /// repeats; the id, or `None` once the connection has ended.
    pub async fn request(&self, destination: &str, namespace: &str, mut payload: Value) -> Option<u64> {
        let id = self.next_request.fetch_add(1, Ordering::Relaxed);
        payload["requestId"] = json!(id);
        self.send(destination, namespace, &payload).await.then_some(id)
    }

    /// Connects to `destination` (an app's transport, or the platform):
    /// Cast devices answer only those that did.
    pub async fn open(&self, destination: &str) -> bool {
        self.send(destination, NS_CONNECTION, &json!({"type": "CONNECT"})).await
    }

    /// Ends the connection.
    pub fn close(&self) {
        self.closed.cancel();
    }

    /// Whether it has ended, by either side.
    pub fn is_closed(&self) -> bool {
        self.closed.is_cancelled()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.closed.cancel();
    }
}

/// Accepts any certificate: see [`Connection::connect`].
#[derive(Debug)]
struct AnyCertificate;

impl rustls::client::danger::ServerCertVerifier for AnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

/// Writes `message`: whether it went within [`SILENT_AT_MOST`].
async fn write_within(writer: &mut (impl AsyncWrite + Unpin), message: &Message) -> bool {
    matches!(tokio::time::timeout(SILENT_AT_MOST, frame::write(writer, message)).await, Ok(Ok(())))
}

fn message(destination: &str, namespace: &str, payload: &Value) -> Message {
    Message {
        source: SENDER.to_owned(),
        destination: destination.to_owned(),
        namespace: namespace.to_owned(),
        payload: payload.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_device_that_talks_but_stops_reading_ends_the_connection() {
        let (ours, theirs) = tokio::io::duplex(1024);
        let (connection, _incoming) = Connection::over(ours);
        // The device pings, so it never goes silent, but reads nothing.
        let (_unread, mut device) = tokio::io::split(theirs);
        let pinging = tokio::spawn(async move {
            let ping = Message {
                source: PLATFORM.into(),
                destination: SENDER.into(),
                namespace: NS_HEARTBEAT.into(),
                payload: r#"{"type":"PING"}"#.into(),
            };
            while frame::write(&mut device, &ping).await.is_ok() {
                tokio::time::sleep(PING_EVERY).await;
            }
        });
        let payload = json!({"type": "GET_STATUS", "padding": "x".repeat(1000)});
        let ended = tokio::time::timeout(Duration::from_secs(120), async {
            while connection.send(PLATFORM, NS_RECEIVER, &payload).await {}
        })
        .await;
        assert!(ended.is_ok(), "sending gives up once the device stops reading");
        assert!(connection.is_closed());
        pinging.abort();
    }
}
