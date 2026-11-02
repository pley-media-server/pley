//! Finding pley servers on the local network: `pley?` broadcast to UDP port
//! 7539, the answers collected for a moment (the server side is
//! apps/server/src/discovery.rs). A server on this computer that only
//! listens on 127.0.0.1 doesn't answer broadcasts, so it is asked directly.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::task::Poll;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::ReadBuf;
use tokio::net::UdpSocket;

use crate::servers::check_server;

pub const PORT: u16 = 7539;
const REQUEST: &[u8] = b"pley?";
/// How long answers are waited for.
const WAIT: Duration = Duration::from_millis(1500);

/// A server that answered.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoundServer {
    pub url: String,
    pub name: String,
    pub id: Option<String>,
    pub version: String,
}

/// A server's answer.
#[derive(Debug, Deserialize)]
struct Announcement {
    pley: u32,
    id: String,
    name: String,
    version: String,
    port: u16,
}

/// The server behind an answer from `from`, if it is one.
fn read_answer(bytes: &[u8], from: SocketAddr) -> Option<FoundServer> {
    let answer: Announcement = serde_json::from_slice(bytes).ok()?;
    if answer.pley != 1 || answer.port == 0 {
        return None;
    }
    let host = match from.ip() {
        ip if ip.is_loopback() => "localhost".to_owned(),
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => format!("[{v6}]"),
        },
    };
    Some(FoundServer {
        url: format!("http://{host}:{}", answer.port),
        name: answer.name,
        id: Some(answer.id),
        version: answer.version,
    })
}

/// Where to send the request, and from which of this computer's addresses:
/// the general broadcast address and this computer from any, and, from each
/// network's own address, the general broadcast address again and that
/// network's. Windows sends a general broadcast out of one network only (often
/// a virtual machine's), and a server whose network is set narrower than this
/// computer's doesn't take the network's broadcast address for one.
fn targets() -> Vec<(Ipv4Addr, SocketAddr)> {
    let any = Ipv4Addr::UNSPECIFIED;
    let mut out = vec![
        (any, SocketAddr::from((Ipv4Addr::BROADCAST, PORT))),
        (any, SocketAddr::from((Ipv4Addr::LOCALHOST, PORT))),
    ];
    for interface in if_addrs::get_if_addrs().unwrap_or_default() {
        if let if_addrs::IfAddr::V4(v4) = interface.addr {
            if v4.ip.is_loopback() {
                continue;
            }
            for target in [Some(Ipv4Addr::BROADCAST), v4.broadcast].into_iter().flatten() {
                let target = (v4.ip, SocketAddr::from((target, PORT)));
                if !out.contains(&target) {
                    out.push(target);
                }
            }
        }
    }
    out
}

/// The next datagram on any of `sockets`. Errors are skipped: Windows reports
/// an earlier request's ICMP "port unreachable" (no server on this computer)
/// as one.
async fn recv_any(sockets: &[UdpSocket], buf: &mut [u8]) -> (usize, SocketAddr) {
    std::future::poll_fn(|cx| {
        for socket in sockets {
            loop {
                let mut read = ReadBuf::new(&mut *buf);
                match socket.poll_recv_from(cx, &mut read) {
                    Poll::Ready(Ok(from)) => return Poll::Ready((read.filled().len(), from)),
                    Poll::Ready(Err(_)) => continue,
                    Poll::Pending => break,
                }
            }
        }
        Poll::Pending
    })
    .await
}

/// Sends the request to each `(from, to)` of `targets`, from a socket on that
/// address of this computer, and gathers the answers for `wait`, one entry per
/// server (by id): this computer's address wins over its network ones.
pub async fn search(targets: &[(Ipv4Addr, SocketAddr)], wait: Duration) -> Vec<FoundServer> {
    let mut sources: Vec<Ipv4Addr> = Vec::new();
    for &(from, _) in targets {
        if !sources.contains(&from) {
            sources.push(from);
        }
    }
    let mut sockets = Vec::new();
    for source in sources {
        // A network that has just gone away can't be bound; the others still are.
        let Ok(socket) = UdpSocket::bind((source, 0)).await else { continue };
        let _ = socket.set_broadcast(true);
        for &(_, to) in targets.iter().filter(|(from, _)| *from == source) {
            let _ = socket.send_to(REQUEST, to).await;
        }
        sockets.push(socket);
    }
    let mut found: HashMap<String, FoundServer> = HashMap::new();
    let mut buf = [0u8; 1024];
    let deadline = tokio::time::Instant::now() + wait;
    while let Ok((len, from)) = tokio::time::timeout_at(deadline, recv_any(&sockets, &mut buf)).await {
        let Some(server) = read_answer(&buf[..len], from) else { continue };
        let key = server.id.clone().unwrap_or_else(|| server.url.clone());
        let local = server.url.starts_with("http://localhost:");
        if local || !found.contains_key(&key) {
            found.insert(key, server);
        }
    }
    let mut list: Vec<FoundServer> = found.into_values().collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.url.cmp(&b.url)));
    list
}

#[tauri::command]
pub async fn discover(app: tauri::AppHandle) -> Vec<FoundServer> {
    let mut found = search(&targets(), WAIT).await;
    if !found.iter().any(|s| s.url.starts_with("http://localhost:")) {
        let local = format!("http://localhost:{PORT}");
        if let Ok(info) = check_server(app, local.clone()).await {
            found.insert(0, FoundServer { url: local, name: info.name, id: None, version: info.version });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_become_addresses() {
        let answer = br#"{"pley":1,"id":"ab","name":"Den","version":"0.1.0","port":7539}"#;
        let found = read_answer(answer, "192.168.1.20:7539".parse().unwrap()).unwrap();
        assert_eq!(
            (found.url.as_str(), found.name.as_str(), found.id.as_deref()),
            ("http://192.168.1.20:7539", "Den", Some("ab"))
        );
        assert_eq!(read_answer(answer, "127.0.0.1:7539".parse().unwrap()).unwrap().url, "http://localhost:7539");
        assert_eq!(read_answer(answer, "[fd00::5]:7539".parse().unwrap()).unwrap().url, "http://[fd00::5]:7539");
        assert_eq!(read_answer(b"hello", "192.168.1.20:7539".parse().unwrap()), None);
        let future = br#"{"pley":2,"id":"ab","name":"Den","version":"9.0.0","port":7539}"#;
        assert_eq!(read_answer(future, "192.168.1.20:7539".parse().unwrap()), None);
    }

    #[tokio::test]
    async fn gathers_one_answer_per_server() {
        // Two sockets standing in for one server's two addresses, and another server.
        let same_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let other = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let targets = [same_a.local_addr().unwrap(), other.local_addr().unwrap()].map(|to| (Ipv4Addr::LOCALHOST, to));
        for (socket, id, name) in [(same_a, "a", "Den"), (other, "b", "Attic")] {
            tokio::spawn(async move {
                let mut buf = [0u8; 16];
                let (len, from) = socket.recv_from(&mut buf).await.unwrap();
                assert_eq!(&buf[..len], REQUEST);
                let answer = format!(r#"{{"pley":1,"id":"{id}","name":"{name}","version":"0.1.0","port":7539}}"#);
                socket.send_to(answer.as_bytes(), from).await.unwrap();
                socket.send_to(answer.as_bytes(), from).await.unwrap();
            });
        }
        let found = search(&targets, Duration::from_millis(400)).await;
        assert_eq!(found.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Attic", "Den"]);
    }

    #[tokio::test]
    async fn a_port_nobody_listens_on_doesnt_end_the_search() {
        // On Windows the request to the closed port comes back as an error on
        // the next receive, ahead of the server's answer.
        let closed = UdpSocket::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
        let server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let targets = [closed, server.local_addr().unwrap()].map(|to| (Ipv4Addr::LOCALHOST, to));
        tokio::spawn(async move {
            let mut buf = [0u8; 16];
            let (_, from) = server.recv_from(&mut buf).await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            let answer = br#"{"pley":1,"id":"a","name":"Den","version":"0.1.0","port":7539}"#;
            server.send_to(answer, from).await.unwrap();
        });
        let found = search(&targets, Duration::from_millis(400)).await;
        assert_eq!(found.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Den"]);
    }
}
