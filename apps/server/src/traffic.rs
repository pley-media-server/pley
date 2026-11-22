//! What pley sends its clients, for the dashboard's network chart and its
//! list of who is using the connection: each response's body, counted as it
//! goes out, by who asked for it. That is the account and device a request
//! signed in as ([`Slot`], which the sign-in fills: Pley's own, a keyed
//! address's session, a compatibility API's token), or else only the
//! address it came from (the web app's files).
//!
//! Every listener goes through [`crate::http::serve`], which counts; the
//! headers and what TCP adds aren't counted, which is a few hundred bytes a
//! request against the media's megabytes.

use std::collections::HashMap;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::task::{Context, Poll};

use axum::body::{Body, Bytes};
use axum::response::Response;
use hyper::body::{Body as HttpBody, Frame, SizeHint};
use parking_lot::Mutex;
use serde::Serialize;

/// The clients counted apart at most: addresses are anyone's to come from,
/// so past this the rest share one count, with no name or address.
const MAX_CLIENTS: usize = 512;

/// Who a response went to.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Client {
    /// The account the request signed in as.
    pub user: Option<String>,
    /// Its device, as the user's devices show it ("Chrome on Windows").
    pub device: Option<String>,
    /// Where the request came from, through trusted proxies.
    pub address: Option<IpAddr>,
}

/// Who a request signed in as, once its handler knows: put in each
/// request's extensions, filled by the sign-in, read when the response
/// goes out.
#[derive(Default)]
pub struct Slot(Mutex<Option<(String, String)>>);

impl Slot {
    pub fn name(&self, user: &str, device: impl FnOnce() -> String) {
        let mut named = self.0.lock();
        if named.is_none() {
            *named = Some((user.to_owned(), device()));
        }
    }

    /// The account and device it names so far.
    pub fn named(&self) -> Option<(String, String)> {
        self.0.lock().clone()
    }
}

/// Names the slot of a request with these `extensions`, when it has one
/// (every request [`crate::http::serve`] takes).
pub fn name(extensions: &axum::http::Extensions, user: &str, device: impl FnOnce() -> String) {
    if let Some(slot) = extensions.get::<Arc<Slot>>() {
        slot.name(user, device);
    }
}

/// Everything sent so far, and by client.
#[derive(Default)]
pub struct Traffic {
    sent: AtomicU64,
    clients: Mutex<HashMap<Client, Arc<AtomicU64>>>,
}

static TRAFFIC: LazyLock<Traffic> = LazyLock::new(Traffic::default);

/// This process's count.
pub fn traffic() -> &'static Traffic {
    &TRAFFIC
}

impl Traffic {
    /// Bytes sent so far, to everyone.
    pub fn sent(&self) -> u64 {
        self.sent.load(Ordering::Relaxed)
    }

    fn counter(&self, client: Client) -> Arc<AtomicU64> {
        let mut clients = self.clients.lock();
        let client =
            if clients.len() >= MAX_CLIENTS && !clients.contains_key(&client) { Client::default() } else { client };
        clients.entry(client).or_default().clone()
    }

    /// Each client's bytes so far. A client nothing is being sent to any
    /// more, whose count is still what `before` says (the totals a while
    /// ago), is forgotten.
    pub fn clients(&self, before: &HashMap<Client, u64>) -> Vec<(Client, u64)> {
        let mut clients = self.clients.lock();
        clients.retain(|client, count| {
            Arc::strong_count(count) > 1 || before.get(client) != Some(&count.load(Ordering::Relaxed))
        });
        clients.iter().map(|(client, count)| (client.clone(), count.load(Ordering::Relaxed))).collect()
    }

    /// `response`, its body counted for whoever `slot` names, or for
    /// `address`.
    pub fn count(&'static self, response: Response, slot: &Slot, address: Option<IpAddr>) -> Response {
        let (user, device) = slot.0.lock().take().unzip();
        let device = device.filter(|d| !d.is_empty());
        let counter = self.counter(Client { user, device, address });
        response.map(|body| Body::new(Counted { inner: body, counter, traffic: self }))
    }
}

/// A body that counts what it sends.
struct Counted {
    inner: Body,
    counter: Arc<AtomicU64>,
    traffic: &'static Traffic,
}

impl HttpBody for Counted {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        let polled = Pin::new(&mut self.inner).poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &polled
            && let Some(data) = frame.data_ref()
        {
            let n = data.len() as u64;
            self.counter.fetch_add(n, Ordering::Relaxed);
            self.traffic.sent.fetch_add(n, Ordering::Relaxed);
        }
        polled
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read(response: Response) -> usize {
        axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().len()
    }

    #[tokio::test]
    async fn bodies_are_counted_by_who_asked() {
        let traffic: &'static Traffic = Box::leak(Box::default());
        let home: IpAddr = "203.0.113.5".parse().unwrap();
        let slot = Slot::default();
        slot.name("ana", || "Chrome on Windows".into());
        slot.name("someone else", || unreachable!("the first sign-in names it"));
        let response = traffic.count(Response::new(Body::from(vec![0u8; 1000])), &slot, Some(home));
        assert_eq!(response.body().size_hint().exact(), Some(1000), "the length goes along");
        assert_eq!(read(response).await, 1000);
        assert_eq!(read(traffic.count(Response::new(Body::from("hello")), &Slot::default(), Some(home))).await, 5);
        assert_eq!(traffic.sent(), 1005);

        let ana = Client { user: Some("ana".into()), device: Some("Chrome on Windows".into()), address: Some(home) };
        let bare = Client { user: None, device: None, address: Some(home) };
        let mut clients = traffic.clients(&HashMap::new());
        clients.sort_by_key(|(_, sent)| *sent);
        assert_eq!(clients, [(bare.clone(), 5), (ana.clone(), 1000)]);

        // Unchanged since and sending nothing: forgotten. One still being
        // sent to stays, however quiet.
        let before: HashMap<Client, u64> = clients.into_iter().collect();
        let open = traffic.count(Response::new(Body::from("x")), &Slot::default(), Some(home));
        assert_eq!(traffic.clients(&before), [(bare, 5)]);
        drop(open);
        assert!(traffic.clients(&before).is_empty());
    }

    #[test]
    fn too_many_clients_share_one_count() {
        let traffic: &'static Traffic = Box::leak(Box::default());
        for i in 0..MAX_CLIENTS + 5 {
            let address = Some(IpAddr::from([10, 0, (i / 256) as u8, (i % 256) as u8]));
            drop(traffic.count(Response::new(Body::empty()), &Slot::default(), address));
        }
        let clients = traffic.clients(&HashMap::new());
        assert_eq!(clients.len(), MAX_CLIENTS + 1);
        assert!(clients.iter().any(|(c, _)| *c == Client::default()));
    }
}
