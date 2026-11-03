//! LAN discovery (src/discovery.rs): a server answers `pley?` from the
//! local network with its name and web port, and nothing else.

mod common;

use std::time::Duration;

use serde_json::Value;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use common::TestApp;

async fn ask(server: std::net::SocketAddr, message: &[u8]) -> Option<Value> {
    let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client.send_to(message, server).await.unwrap();
    let mut buf = [0u8; 512];
    let (len, _) = tokio::time::timeout(Duration::from_millis(500), client.recv_from(&mut buf)).await.ok()?.ok()?;
    serde_json::from_slice(&buf[..len]).ok()
}

#[tokio::test]
async fn servers_tell_apps_on_the_network_where_they_are() {
    let app = TestApp::new().await;
    app.put("/api/settings", &app.setup_admin().await, serde_json::json!({"server_name": "Living room"})).await;
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    let stopping = CancellationToken::new();
    let served = tokio::spawn(pley::discovery::serve(app.state.clone(), socket, 7539, stopping.clone()));

    let answer = ask(addr, pley::discovery::REQUEST).await.expect("an answer");
    assert_eq!(answer["pley"], 1);
    assert_eq!(answer["name"], "Living room");
    assert_eq!(answer["port"], 7539);
    assert_eq!(answer["version"], env!("CARGO_PKG_VERSION"));
    let id = answer["id"].as_str().unwrap().to_owned();
    assert_eq!(id.len(), 40, "the server's id: {id}");
    assert_eq!(ask(addr, pley::discovery::REQUEST).await.unwrap()["id"], id.as_str(), "the same id every time");
    assert_eq!(ask(addr, b"hello").await, None, "anything else goes unanswered");

    stopping.cancel();
    tokio::time::timeout(Duration::from_secs(5), served).await.unwrap().unwrap();
}
