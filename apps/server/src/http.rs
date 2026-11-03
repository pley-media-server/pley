//! The HTTP listeners (the web app and API, the Jellyfin-compatible API): like
//! `axum::serve`, but a client gets [`HEADER_TIMEOUT`] to send a request's
//! headers, so connections opened and never used (or fed a byte at a time)
//! don't pile up until pley runs out of them.

use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::extract::Request;
use axum::response::Response;
use dashmap::DashMap;
use hyper::body::Incoming;
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower::{Service, ServiceExt};
use tower_http::timeout::TimeoutBody;

/// How long a client has to send the headers of a request (the first one,
/// or the next one on a connection kept open).
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a request's body may send nothing: a body fed a byte now and
/// then would otherwise hold its connection for the request's whole time.
pub const BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(20);
/// Connections open at once: past it, new ones wait in the system's queue.
const MAX_CONNECTIONS: usize = 8192;
/// Connections open at once from one address on the internet. A reverse
/// proxy (on this computer or network) carries everyone's, so it has none.
const MAX_CONNECTIONS_EACH: usize = 256;

/// Serves `service` on `listener` until `shutdown` completes; then stops
/// taking connections, lets the requests in progress finish (streams end
/// with the server's own stop signal) and returns once every connection is
/// closed. Handlers see the client's address as `ConnectInfo<SocketAddr>`.
pub async fn serve<S>(listener: TcpListener, service: S, shutdown: impl Future<Output = ()> + Send + 'static)
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    serve_with(listener, service, shutdown, HEADER_TIMEOUT, BODY_IDLE_TIMEOUT).await;
}

async fn serve_with<S>(
    listener: TcpListener,
    service: S,
    shutdown: impl Future<Output = ()> + Send + 'static,
    header_timeout: Duration,
    body_timeout: Duration,
) where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    let stop = CancellationToken::new();
    let connections = TaskTracker::new();
    let room = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let per_peer: Arc<DashMap<IpAddr, usize>> = Arc::default();
    tokio::pin!(shutdown);
    loop {
        let permit = tokio::select! {
            permit = room.clone().acquire_owned() => permit.expect("never closed"),
            () = &mut shutdown => break,
        };
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(connection) => connection,
                Err(err) => {
                    if !is_connection_error(&err) {
                        // Out of file descriptors, most likely: wait for some to close.
                        tracing::warn!("cannot accept a connection: {err}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    continue;
                }
            },
            () = &mut shutdown => break,
        };
        let Some(peer_slot) = PeerSlot::take(&per_peer, peer.ip()) else {
            tracing::debug!("refused a connection from {peer}: it has {MAX_CONNECTIONS_EACH} open");
            continue;
        };
        let _ = stream.set_nodelay(true);
        let service = service.clone();
        let stop = stop.clone();
        connections.spawn(async move {
            let _held = (permit, peer_slot);
            let service = hyper::service::service_fn(move |mut request: hyper::Request<Incoming>| {
                request.extensions_mut().insert(ConnectInfo(peer));
                service.clone().oneshot(request.map(|body| Body::new(TimeoutBody::new(body_timeout, body))))
            });
            let connection = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                .header_read_timeout(header_timeout)
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades();
            tokio::pin!(connection);
            tokio::select! {
                result = connection.as_mut() => log_end(result, peer),
                () = stop.cancelled() => {
                    connection.as_mut().graceful_shutdown();
                    log_end(connection.await, peer);
                }
            }
        });
    }
    drop(listener);
    stop.cancel();
    connections.close();
    connections.wait().await;
}

/// One of an address's connections, counted while it is open; addresses of
/// this computer and its network (proxies) aren't counted.
struct PeerSlot {
    counts: Option<(Arc<DashMap<IpAddr, usize>>, IpAddr)>,
}

impl PeerSlot {
    fn take(counts: &Arc<DashMap<IpAddr, usize>>, ip: IpAddr) -> Option<Self> {
        let ip = ip.to_canonical();
        if crate::setup::is_local_address(ip) {
            return Some(Self { counts: None });
        }
        let mut open = counts.entry(ip).or_insert(0);
        if *open >= MAX_CONNECTIONS_EACH {
            return None;
        }
        *open += 1;
        Some(Self { counts: Some((counts.clone(), ip)) })
    }
}

impl Drop for PeerSlot {
    fn drop(&mut self) {
        if let Some((counts, ip)) = &self.counts {
            counts.remove_if_mut(ip, |_, open| {
                *open = open.saturating_sub(1);
                *open == 0
            });
        }
    }
}

/// Raises this process's limit of open files to the most it may have (Unix
/// systems often start it at 1024): each connection takes one, and so do
/// the database, logs and media being read.
pub fn raise_open_files_limit() {
    #[cfg(unix)]
    {
        let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: plain calls with a valid out pointer, then a value read back.
        unsafe {
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) == 0 && limit.rlim_cur < limit.rlim_max {
                limit.rlim_cur = limit.rlim_max;
                let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
            }
        }
    }
}

fn log_end(result: Result<(), hyper::Error>, peer: SocketAddr) {
    if let Err(err) = result {
        tracing::trace!("connection from {peer} ended: {err}");
    }
}

/// Errors of one connection (it went away before it was accepted), not of
/// the listener.
fn is_connection_error(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const TIMEOUT: Duration = Duration::from_millis(300);

    async fn server() -> (SocketAddr, CancellationToken, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new()
            .route(
                "/who",
                axum::routing::get(|ConnectInfo(peer): ConnectInfo<SocketAddr>| async move { peer.ip().to_string() }),
            )
            .route("/echo", axum::routing::post(|body: axum::body::Bytes| async move { body }));
        let stop = CancellationToken::new();
        let task = tokio::spawn(serve_with(listener, app, stop.clone().cancelled_owned(), TIMEOUT, TIMEOUT));
        (addr, stop, task)
    }

    #[tokio::test]
    async fn answers_with_the_clients_address_and_stops() {
        let (addr, stop, task) = server().await;
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"GET /who HTTP/1.1\r\nhost: x\r\nconnection: close\r\n\r\n").await.unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.unwrap();
        assert!(answer.starts_with("HTTP/1.1 200") && answer.ends_with("127.0.0.1"), "{answer}");
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), task).await.expect("stopped").unwrap();
    }

    #[tokio::test]
    async fn slow_clients_are_let_go() {
        let (addr, _stop, _task) = server().await;
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"GET /who HTTP/1.1\r\n").await.unwrap();
        // Never finishing the headers: the connection is closed after the timeout.
        let mut answer = Vec::new();
        let read = tokio::time::timeout(TIMEOUT * 10, stream.read_to_end(&mut answer)).await;
        assert!(read.is_ok(), "still open after {:?}", TIMEOUT * 10);
        assert!(!String::from_utf8_lossy(&answer).contains("200 OK"));
    }

    #[tokio::test]
    async fn a_body_that_stops_coming_is_let_go() {
        let (addr, _stop, _task) = server().await;
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        // 100 bytes announced, one sent, then nothing.
        stream.write_all(b"POST /echo HTTP/1.1\r\nhost: x\r\ncontent-length: 100\r\n\r\nx").await.unwrap();
        let mut answer = Vec::new();
        let read = tokio::time::timeout(TIMEOUT * 10, stream.read_to_end(&mut answer)).await;
        assert!(read.is_ok(), "still open after {:?}", TIMEOUT * 10);
        assert!(!String::from_utf8_lossy(&answer).contains("200 OK"), "{}", String::from_utf8_lossy(&answer));
    }
}
