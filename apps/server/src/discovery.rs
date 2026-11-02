//! LAN discovery: the pley apps find servers on the local network by
//! broadcasting `pley?` to UDP port 7539 (the web port); each server answers
//! with its id, name, version and web port. Only addresses on the local
//! network get an answer, and only so many a second, so the port can't be
//! used to flood anyone.

use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;

/// What apps send.
pub const REQUEST: &[u8] = b"pley?";
/// Answers sent per second at most.
const MAX_ANSWERS: u32 = 100;

/// A server's answer, as JSON.
#[derive(Debug, Serialize)]
pub struct Announcement<'a> {
    /// The protocol's version.
    pub pley: u32,
    /// Stable across restarts and renames (the Plex machine id).
    pub id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    /// The web port, on the address the answer comes from.
    pub port: u16,
}

/// Answer discovery requests on `socket` until `stopping` is cancelled.
pub async fn serve(state: AppState, socket: UdpSocket, web_port: u16, stopping: CancellationToken) {
    let id = state.identity.id.clone();
    let mut buf = [0u8; 64];
    let mut window = (Instant::now(), 0u32);
    loop {
        let (len, from) = tokio::select! {
            received = socket.recv_from(&mut buf) => match received {
                Ok(received) => received,
                // Windows reports an earlier answer's ICMP "port unreachable" here.
                Err(_) => continue,
            },
            () = stopping.cancelled() => return,
        };
        if &buf[..len] != REQUEST || !crate::setup::is_local_address(from.ip()) {
            continue;
        }
        if window.0.elapsed() >= Duration::from_secs(1) {
            window = (Instant::now(), 0);
        }
        if window.1 >= MAX_ANSWERS {
            continue;
        }
        window.1 += 1;
        let settings = state.settings.get();
        let answer = Announcement {
            pley: 1,
            id: &id,
            name: &settings.server_name,
            version: env!("CARGO_PKG_VERSION"),
            port: web_port,
        };
        if let Ok(bytes) = serde_json::to_vec(&answer) {
            let _ = socket.send_to(&bytes, from).await;
        }
    }
}
