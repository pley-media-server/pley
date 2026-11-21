//! Signing in with Tailscale, against a stand-in for Tailscale's service
//! (its LocalAPI on a Unix socket, or a named pipe on Windows): who it says a
//! connection comes from, linking, signing in, and sessions that work only
//! over the tailnet from the same person.

mod common;

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, Response, StatusCode, header};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::TestApp;

/// The server's own name and address on the stand-in tailnet.
const SERVER: &str = "media.example.ts.net:7539";
const SERVER_IP: &str = "100.100.1.1";
/// Alice's laptop, Bob's phone, a tagged TV.
const ALICE: &str = "100.64.0.9:50000";
const BOB: &str = "100.64.0.10:50000";
const TV: &str = "100.64.0.11:50000";
/// Dana's phone: an account that isn't a member of this home, whose
/// Tailscale user is another tailnet's, one the server's device is shared
/// with.
const DANA: &str = "100.64.0.13:50000";

/// Tailscale's service, as far as Pley asks it: what `whois` says of each
/// address (by IP, or by IP and port for this computer's), whether it has a
/// network adapter of its own, and whether it answers at all.
#[derive(Clone)]
struct Fake {
    devices: Arc<Mutex<HashMap<String, Value>>>,
    adapter: Arc<Mutex<bool>>,
    down: Arc<Mutex<bool>>,
}

impl Default for Fake {
    fn default() -> Self {
        Self { devices: Arc::default(), adapter: Arc::new(Mutex::new(true)), down: Arc::default() }
    }
}

impl Fake {
    fn device(&self, addr: &str, stable_id: &str, tags: &[&str], user: i64, login: &str) {
        self.device_routing(addr, stable_id, tags, user, login, &[]);
    }

    /// A device the tailnet routes `routes` to besides its own address.
    fn device_routing(&self, addr: &str, stable_id: &str, tags: &[&str], user: i64, login: &str, routes: &[&str]) {
        let addr: SocketAddr = addr.parse().unwrap();
        let key = if addr.ip().is_loopback() { addr.to_string() } else { addr.ip().to_string() };
        let own = format!("{}/32", addr.ip());
        let allowed: Vec<String> = std::iter::once(own.clone()).chain(routes.iter().map(|r| r.to_string())).collect();
        self.devices.lock().insert(
            key,
            json!({
                "Node": {"ID": 1, "StableID": stable_id, "Name": "device.example.ts.net.", "Tags": tags,
                         "Addresses": [own], "AllowedIPs": allowed},
                "UserProfile": {"ID": user, "LoginName": login, "DisplayName": login, "ProfilePicURL": ""},
                "CapMap": {}
            }),
        );
    }

    fn answer(&self, request: &Request<hyper::body::Incoming>) -> (StatusCode, String) {
        if *self.down.lock() {
            return (StatusCode::INTERNAL_SERVER_ERROR, "down".into());
        }
        if request.headers().get(header::HOST).and_then(|h| h.to_str().ok()) != Some("local-tailscaled.sock") {
            return (StatusCode::FORBIDDEN, "invalid localapi request".into());
        }
        match request.uri().path() {
            "/localapi/v0/status" => (
                StatusCode::OK,
                json!({"BackendState": "Running", "TUN": *self.adapter.lock(),
                       "Self": {"ID": "nSERVER", "DNSName": "media.example.ts.net.", "TailscaleIPs": [SERVER_IP]},
                       "CurrentTailnet": {"Name": "example.com", "StableID": "T1234CNTRL"}})
                .to_string(),
            ),
            "/localapi/v0/whois" => {
                let query = request.uri().query().unwrap_or_default();
                let addr = url::form_urlencoded::parse(query.as_bytes())
                    .find(|(k, _)| k == "addr")
                    .map(|(_, v)| v.into_owned())
                    .unwrap_or_default();
                let Ok(addr) = addr.parse::<SocketAddr>() else { return (StatusCode::BAD_REQUEST, addr) };
                if !query.contains("proto=tcp") {
                    return (StatusCode::BAD_REQUEST, "Pley asks about TCP connections".into());
                }
                let key = if addr.ip().is_loopback() { addr.to_string() } else { addr.ip().to_string() };
                match self.devices.lock().get(&key) {
                    Some(device) => (StatusCode::OK, device.to_string()),
                    None => (StatusCode::NOT_FOUND, "no match for IP:port".into()),
                }
            }
            _ => (StatusCode::NOT_FOUND, "not found".into()),
        }
    }

    /// Serves one connection.
    async fn serve<S>(self, stream: S)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let service = hyper::service::service_fn(move |request| {
            let (status, body) = self.answer(&request);
            async move { Ok::<_, Infallible>(Response::builder().status(status).body(Body::from(body)).unwrap()) }
        });
        let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
    }

    /// Listens where Pley is told Tailscale's service is.
    #[cfg(unix)]
    fn listen(&self, dir: &std::path::Path) -> PathBuf {
        let path = dir.join("tailscaled.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let fake = self.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(fake.clone().serve(stream));
            }
        });
        path
    }

    #[cfg(windows)]
    fn listen(&self, _dir: &std::path::Path) -> PathBuf {
        use tokio::net::windows::named_pipe::ServerOptions;
        let name = format!(r"\\.\pipe\pley-test-tailscale-{}", uuid::Uuid::new_v4().simple());
        let mut server = ServerOptions::new().first_pipe_instance(true).create(&name).unwrap();
        let fake = self.clone();
        let next = name.clone();
        tokio::spawn(async move {
            while server.connect().await.is_ok() {
                let connected = server;
                server = ServerOptions::new().create(&next).unwrap();
                tokio::spawn(fake.clone().serve(connected));
            }
        });
        PathBuf::from(name)
    }
}

struct Reply {
    status: StatusCode,
    body: Value,
}

/// A request from `peer`, the other end of its connection, to `host`.
#[allow(clippy::too_many_arguments)]
async fn call(
    app: &TestApp,
    method: Method,
    uri: &str,
    peer: &str,
    host: &str,
    token: Option<&str>,
    body: Option<Value>,
    extra: &[(&str, &str)],
) -> Reply {
    let mut req = Request::builder().method(method).uri(uri).header(header::HOST, host);
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let req = req.extension(axum::extract::ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
    let req = match body {
        Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let resp = app.api.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    Reply { status, body: serde_json::from_slice(&bytes).unwrap_or(Value::Null) }
}

/// Who Tailscale says `peer` is, as the sign-in page asks.
async fn here(app: &TestApp, peer: &str, host: &str) -> Value {
    let r = call(app, Method::GET, "/api/auth/tailscale", peer, host, None, None, &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    r.body["tailscale"].clone()
}

async fn sign_in(app: &TestApp, peer: &str, host: &str, body: Value) -> Reply {
    call(app, Method::POST, "/api/auth/tailscale", peer, host, None, Some(body), &[]).await
}

async fn me(app: &TestApp, peer: &str, token: &str) -> StatusCode {
    call(app, Method::GET, "/api/auth/me", peer, SERVER, Some(token), None, &[]).await.status
}

/// A Pley told where the stand-in is, with an administrator, Alice and Bob
/// (who aren't administrators), and the stand-in's devices.
async fn start() -> (TestApp, Fake, String, tempfile::TempDir) {
    let dir = common::tempdir();
    let fake = Fake::default();
    let socket = fake.listen(dir.path());
    let app = TestApp::with_config(|config| config.tailscale_socket = Ok(socket)).await;
    let admin = app.setup_admin().await;
    app.user(&admin, "alice", json!({})).await;
    app.user(&admin, "bob", json!({})).await;
    fake.device(ALICE, "nLAPTOP", &[], 41, "alice@example.com");
    fake.device(BOB, "nPHONE", &[], 42, "bob@example.com");
    fake.device(TV, "nTV", &["tag:tv"], 41, "alice@example.com");
    // The server's own device, whose address any program on it connects from.
    fake.device(&format!("{SERVER_IP}:1"), "nSERVER", &[], 41, "alice@example.com");
    (app, fake, admin, dir)
}

async fn turn_on(app: &TestApp, admin: &str, on: bool) {
    let r = app.put("/api/settings", admin, json!({"tailscale_sign_in": on})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(r.body["tailscale_sign_in"], json!(on));
}

/// Links the Tailscale user on `peer`'s device to the account signed in with `token`.
async fn link(app: &TestApp, peer: &str, token: &str, body: Value) -> Reply {
    call(app, Method::PUT, "/api/me/tailscale", peer, SERVER, Some(token), Some(body), &[]).await
}

#[tokio::test]
async fn nothing_is_asked_until_an_administrator_turns_it_on() {
    let (app, fake, admin, _dir) = start().await;
    *fake.down.lock() = true;
    let alice = app.login("alice", "test-pass-1").await;
    assert_eq!(here(&app, ALICE, SERVER).await, Value::Null);
    let r = sign_in(&app, ALICE, SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    let r = link(&app, ALICE, &alice, json!({"password": "test-pass-1"})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    let mine = call(&app, Method::GET, "/api/me/tailscale", ALICE, SERVER, Some(&admin), None, &[]).await;
    assert_eq!(mine.body["on"], json!(false));
    assert!(mine.body.get("problem").is_none(), "Tailscale wasn't asked: {}", mine.body);
}

#[tokio::test]
async fn a_linked_person_signs_in_from_their_device_alone() {
    let (app, _fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    let alice = app.login("alice", "test-pass-1").await;

    // Linking takes the password, from a device of the tailnet, at the
    // server's Tailscale name or address.
    let r = link(&app, ALICE, &alice, json!({"password": "wrong-pass"})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    for (peer, host) in [("192.168.1.30:50000", SERVER), (ALICE, "pley.local:7539"), (ALICE, "evil.example.com:7539")] {
        let r = call(
            &app,
            Method::PUT,
            "/api/me/tailscale",
            peer,
            host,
            Some(&alice),
            Some(json!({"password": "test-pass-1"})),
            &[],
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{peer} {host}: {}", r.body);
    }
    let r = link(&app, ALICE, &alice, json!({"password": "test-pass-1"})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["linked"]["login"], json!("alice@example.com"));
    assert_eq!(r.body["here"]["username"], json!("alice"));
    assert!(r.body.get("problem").is_none(), "for administrators only");

    // Bob can't link Alice's Tailscale user, nor sees which account has it.
    let bob = app.login("bob", "test-pass-1").await;
    let r = call(&app, Method::GET, "/api/me/tailscale", ALICE, SERVER, Some(&bob), None, &[]).await;
    assert_eq!(r.body["here"]["username"], Value::Null, "{}", r.body);
    let r = link(&app, ALICE, &bob, json!({"password": "test-pass-1"})).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{}", r.body);

    // The sign-in page sees Alice only at the server's own names: another
    // name may be a website's resolved to this server (DNS rebinding).
    assert_eq!(here(&app, ALICE, SERVER).await["username"], json!("alice"));
    assert_eq!(here(&app, ALICE, &format!("{SERVER_IP}:7539")).await["username"], json!("alice"));
    assert_eq!(here(&app, ALICE, "MEDIA.example.ts.net").await["username"], json!("alice"));
    for (peer, host) in [
        (ALICE, "evil.example.com:7539"),
        (ALICE, "media:7539"),
        ("192.168.1.30:50000", SERVER),
        (TV, SERVER),
        (&format!("{SERVER_IP}:50000"), SERVER),
        ("127.0.0.1:40000", SERVER),
    ] {
        assert_eq!(here(&app, peer, host).await, Value::Null, "{peer} {host}");
        let r = sign_in(&app, peer, host, json!({})).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{peer} {host}: {}", r.body);
    }
    assert_eq!(
        here(&app, BOB, SERVER).await,
        json!({"login": "bob@example.com", "name": "bob@example.com", "username": null})
    );
    let r = sign_in(&app, BOB, SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "Bob linked nothing: {}", r.body);

    // A device passing on other people's requests vouches for none of them,
    // when the request says so: a proxy's header, or nginx's HTTP/1.0.
    let forwarded = [("x-forwarded-for", "203.0.113.9"), ("via", "1.1 proxy"), ("forwarded", "for=203.0.113.9")];
    for header in forwarded {
        let r = call(&app, Method::GET, "/api/auth/tailscale", ALICE, SERVER, None, None, &[header]).await;
        assert_eq!(r.body["tailscale"], Value::Null, "{header:?}");
        let r = call(&app, Method::POST, "/api/auth/tailscale", ALICE, SERVER, None, Some(json!({})), &[header]).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{header:?}: {}", r.body);
    }
    let old = Request::builder()
        .method(Method::GET)
        .uri("/api/auth/tailscale")
        .version(axum::http::Version::HTTP_10)
        .header(header::HOST, SERVER)
        .extension(axum::extract::ConnectInfo(ALICE.parse::<SocketAddr>().unwrap()))
        .body(Body::empty())
        .unwrap();
    let r = app.api.clone().oneshot(old).await.unwrap();
    let body = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["tailscale"], Value::Null, "HTTP/1.0");

    // Another site's page can't sign its visitor in.
    let r = call(
        &app,
        Method::POST,
        "/api/auth/tailscale",
        ALICE,
        SERVER,
        None,
        Some(json!({})),
        &[("origin", "http://evil.example.com:7539")],
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);

    let r = sign_in(&app, ALICE, SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["user"]["username"], json!("alice"));
    let session = r.body["token"].as_str().unwrap().to_owned();

    // The session works over the tailnet from Alice's devices alone.
    assert_eq!(me(&app, ALICE, &session).await, StatusCode::OK);
    assert_eq!(me(&app, "100.64.0.9:50001", &session).await, StatusCode::OK, "another connection");
    for peer in ["192.168.1.30:50000", BOB, TV, "127.0.0.1:40000"] {
        assert_eq!(me(&app, peer, &session).await, StatusCode::UNAUTHORIZED, "{peer}");
    }
    let r = app.get("/api/auth/me", &session).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "from nowhere known");
    let r =
        call(&app, Method::GET, "/api/auth/me", ALICE, SERVER, Some(&session), None, &[("x-real-ip", "203.0.113.9")])
            .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "passed on by a proxy");
    // Her password's session goes on working anywhere.
    assert_eq!(me(&app, "192.168.1.30:50000", &alice).await, StatusCode::OK);
}

#[tokio::test]
async fn tailscale_running_without_an_adapter_hands_connections_over_on_this_computer() {
    let (app, fake, admin, _dir) = start().await;
    // Docker's tailscale image: the tailnet's connections come from this
    // computer, each told apart by its port; a proxy's are no one's.
    *fake.adapter.lock() = false;
    turn_on(&app, &admin, true).await;
    let alice = app.login("alice", "test-pass-1").await;
    fake.device("127.0.0.1:40001", "nLAPTOP", &[], 41, "alice@example.com");
    // Nothing keeps the local network from sending from a Tailscale address
    // then: those are no one.
    assert_eq!(here(&app, ALICE, SERVER).await, Value::Null);
    let r = link(&app, "127.0.0.1:40001", &alice, json!({"password": "test-pass-1"})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(here(&app, "127.0.0.1:40001", SERVER).await["username"], json!("alice"));
    assert_eq!(here(&app, "127.0.0.1:40002", SERVER).await, Value::Null);
    let r = sign_in(&app, "127.0.0.1:40001", SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let session = r.body["token"].as_str().unwrap().to_owned();
    assert_eq!(me(&app, "127.0.0.1:40001", &session).await, StatusCode::OK);
    assert_eq!(me(&app, "127.0.0.1:40002", &session).await, StatusCode::UNAUTHORIZED);
    // A port is the connection's alone: once another person's connection
    // takes it, what Tailscale said before doesn't hold.
    fake.device("127.0.0.1:40001", "nPHONE", &[], 42, "bob@example.com");
    assert_eq!(me(&app, "127.0.0.1:40001", &session).await, StatusCode::UNAUTHORIZED);
    assert_eq!(here(&app, "127.0.0.1:40001", SERVER).await["login"], json!("bob@example.com"));
}

#[tokio::test]
async fn each_person_signs_in_as_their_own_account_member_of_this_home_or_not() {
    let (app, fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    let (dana, dana_id) = app.user(&admin, "dana", json!({"home": false})).await;
    fake.device(DANA, "nFRIEND", &[], 77, "dana@elsewhere.example");
    let alice = app.login("alice", "test-pass-1").await;
    let bob = app.login("bob", "test-pass-1").await;
    for (peer, token) in [(ALICE, &alice), (BOB, &bob), (DANA, &dana)] {
        let r = link(&app, peer, token, json!({"password": "test-pass-1"})).await;
        assert_eq!(r.status, StatusCode::OK, "{peer}: {}", r.body);
    }

    let people = [(ALICE, "alice"), (BOB, "bob"), (DANA, "dana")];
    let mut sessions = Vec::new();
    for (peer, name) in people {
        assert_eq!(here(&app, peer, SERVER).await["username"], json!(name));
        let r = sign_in(&app, peer, SERVER, json!({})).await;
        assert_eq!(r.status, StatusCode::OK, "{name}: {}", r.body);
        assert_eq!(r.body["user"]["username"], json!(name));
        sessions.push(r.body["token"].as_str().unwrap().to_owned());
    }
    let [alice_ts, bob_ts, dana_ts] = &sessions[..] else { unreachable!() };
    // Each works from its own person's devices alone.
    for ((owner, _), session) in people.iter().zip(&sessions) {
        for (peer, _) in people {
            let works = if peer == *owner { StatusCode::OK } else { StatusCode::UNAUTHORIZED };
            assert_eq!(me(&app, peer, session).await, works, "{owner}'s session from {peer}");
        }
    }

    // Who's watching? lists the household to Alice, as after her password,
    // and no one to Dana, who can't switch into it either.
    let r = call(&app, Method::GET, "/api/auth/profiles", ALICE, SERVER, Some(alice_ts), None, &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let household: HashMap<String, i64> = r
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|u| (u["username"].as_str().unwrap().to_owned(), u["id"].as_i64().unwrap()))
        .collect();
    assert!(household.contains_key("bob") && !household.contains_key("dana"), "{household:?}");
    let r = call(&app, Method::GET, "/api/auth/profiles", DANA, SERVER, Some(dana_ts), None, &[]).await;
    assert_eq!(r.body, json!([]));
    let switch = json!({"user_id": household["alice"]});
    let r = call(&app, Method::POST, "/api/auth/switch", DANA, SERVER, Some(dana_ts), Some(switch), &[]).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);

    // A child's profile Alice switches to (in place of her session) stays on
    // her devices, and links nothing: it has no password.
    let r = app.post("/api/users", &admin, json!({"username": "kid", "managed": true})).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.body);
    let switch = json!({"user_id": r.body["id"]});
    let r = call(&app, Method::POST, "/api/auth/switch", ALICE, SERVER, Some(alice_ts), Some(switch), &[]).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let kid = r.body["token"].as_str().unwrap().to_owned();
    assert_eq!(me(&app, ALICE, &kid).await, StatusCode::OK);
    assert_eq!(me(&app, BOB, &kid).await, StatusCode::UNAUTHORIZED);
    let r = link(&app, ALICE, &kid, json!({"password": "test-pass-1"})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);

    // One person leaving goes no further: Dana's account disabled, then Bob
    // unlinking.
    let r = app.patch(&format!("/api/users/{dana_id}"), &admin, json!({"disabled": true})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(here(&app, DANA, SERVER).await["username"], Value::Null);
    assert_eq!(sign_in(&app, DANA, SERVER, json!({})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(me(&app, DANA, dana_ts).await, StatusCode::UNAUTHORIZED);
    let r = call(&app, Method::DELETE, "/api/me/tailscale", BOB, SERVER, Some(&bob), None, &[]).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(me(&app, BOB, bob_ts).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&app, ALICE, &kid).await, StatusCode::OK);

    // Alice's account deleted, the profile switched to with its link goes.
    let r = app.delete(&format!("/api/users/{}", household["alice"]), &admin).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    assert_eq!(me(&app, ALICE, &kid).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_administrators_tailscale_session_needs_two_step_sign_in() {
    let (app, _fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    let alice = app.login("alice", "test-pass-1").await;
    assert_eq!(link(&app, ALICE, &alice, json!({"password": "test-pass-1"})).await.status, StatusCode::OK);
    let session = sign_in(&app, ALICE, SERVER, json!({})).await.body["token"].as_str().unwrap().to_owned();
    assert_eq!(me(&app, ALICE, &session).await, StatusCode::OK);

    // Made an administrator's, Alice's account signs in with Tailscale only
    // with two-step sign-in on, and the session it had stops.
    let r = app.patch("/api/users/2", &admin, json!({"is_admin": true})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    assert_eq!(me(&app, ALICE, &session).await, StatusCode::UNAUTHORIZED);
    assert_eq!(sign_in(&app, ALICE, SERVER, json!({})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(me(&app, "192.168.1.30:50000", &alice).await, StatusCode::OK, "the password's session stays");

    // With it on, the account signs in; turning it off ends that session too.
    let (_, codes) = app.turn_on_two_step(&alice, "test-pass-1").await;
    let r = sign_in(&app, ALICE, SERVER, json!({"code": codes[0]})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let session = r.body["token"].as_str().unwrap().to_owned();
    assert_eq!(me(&app, ALICE, &session).await, StatusCode::OK);
    let off = json!({"password": "test-pass-1", "code": codes[1]});
    let r = app.call(Method::DELETE, "/api/me/two-step", Some(&alice), Some(off), &[]).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    assert_eq!(me(&app, ALICE, &session).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn routers_vouch_for_no_one() {
    let (app, fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    let alice = app.login("alice", "test-pass-1").await;
    assert_eq!(link(&app, ALICE, &alice, json!({"password": "test-pass-1"})).await.status, StatusCode::OK);
    // Alice's laptop becomes a subnet router: every device behind it would
    // come from its address.
    fake.device_routing("100.64.0.12:1", "nROUTER", &[], 41, "alice@example.com", &["192.168.1.0/24"]);
    assert_eq!(here(&app, "100.64.0.12:50000", SERVER).await, Value::Null);
    let r = sign_in(&app, "100.64.0.12:50000", SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
}

#[tokio::test]
async fn a_tailscale_session_makes_no_token_that_works_elsewhere_and_its_profiles_go_with_it() {
    let (app, _fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    let (_, carol_id) = app.user(&admin, "carol", json!({"home": true})).await;
    let r = app.patch("/api/users/2", &admin, json!({"home": true})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.text());
    let alice = app.login("alice", "test-pass-1").await;
    assert_eq!(link(&app, ALICE, &alice, json!({"password": "test-pass-1"})).await.status, StatusCode::OK);
    let session = sign_in(&app, ALICE, SERVER, json!({})).await.body["token"].as_str().unwrap().to_owned();

    // Tokens for other apps work from anywhere: not made from it.
    let r = call(
        &app,
        Method::POST,
        "/api/me/compatibility-tokens",
        ALICE,
        SERVER,
        Some(&session),
        Some(json!({"name": "Kometa"})),
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);

    // A profile switched to from it works only over the tailnet from Alice.
    let r = call(
        &app,
        Method::POST,
        "/api/auth/switch",
        ALICE,
        SERVER,
        Some(&session),
        Some(json!({"user_id": carol_id})),
        &[],
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let carol = r.body["token"].as_str().unwrap().to_owned();
    assert_eq!(me(&app, ALICE, &carol).await, StatusCode::OK);
    assert_eq!(me(&app, "192.168.1.30:50000", &carol).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&app, BOB, &carol).await, StatusCode::UNAUTHORIZED);

    // Unlinking signs it out too.
    let r =
        call(&app, Method::DELETE, "/api/me/tailscale", "192.168.1.30:50000", SERVER, Some(&alice), None, &[]).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(me(&app, ALICE, &carol).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn other_sites_pages_get_no_cors_answer() {
    let (app, _fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    // A sandboxed page elsewhere has the `null` origin the TV apps have,
    // which the rest of the API answers.
    let ask = |uri: &str| {
        app.api.clone().oneshot(
            Request::builder()
                .uri(uri)
                .header(header::HOST, SERVER)
                .header(header::ORIGIN, "null")
                .extension(axum::extract::ConnectInfo(ALICE.parse::<SocketAddr>().unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
    };
    let server = ask("/api/server").await.unwrap();
    assert!(server.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_some(), "{:?}", server.headers());
    let r = ask("/api/auth/tailscale").await.unwrap();
    assert!(r.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).is_none(), "{:?}", r.headers());
}

#[tokio::test]
async fn two_step_sign_in_still_asks_its_code() {
    let (app, _fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    // An administrator links only with two-step sign-in on, and gives a code.
    let r = link(&app, ALICE, &admin, json!({"password": "secret-one"})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    let (_, codes) = app.turn_on_two_step(&admin, "secret-one").await;
    let r = link(&app, ALICE, &admin, json!({"password": "secret-one"})).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{}", r.body);
    assert_eq!(r.body["code"], json!("two_step"));
    let r = link(&app, ALICE, &admin, json!({"password": "secret-one", "code": codes[0]})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert!(r.body["server_name"] == json!("media.example.ts.net"), "{}", r.body);

    let r = sign_in(&app, ALICE, SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{}", r.body);
    assert_eq!(r.body["code"], json!("two_step"));
    let r = sign_in(&app, ALICE, SERVER, json!({"code": codes[0]})).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED, "a recovery code works once: {}", r.body);
    let r = sign_in(&app, ALICE, SERVER, json!({"code": codes[1]})).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["user"]["username"], json!("admin"));

    // An administrator who turned it off since signs in with the password.
    let r = app
        .call(
            Method::DELETE,
            "/api/me/two-step",
            Some(&admin),
            Some(json!({"password": "secret-one", "code": codes[2]})),
            &[],
        )
        .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.text());
    let r = sign_in(&app, ALICE, SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
}

#[tokio::test]
async fn leaving_the_tailnet_unlinking_or_turning_it_off_signs_out() {
    let (app, fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    let alice = app.login("alice", "test-pass-1").await;
    assert_eq!(link(&app, ALICE, &alice, json!({"password": "test-pass-1"})).await.status, StatusCode::OK);
    let app_ref = &app;
    let signed_in =
        || async move { sign_in(app_ref, ALICE, SERVER, json!({})).await.body["token"].as_str().unwrap().to_owned() };

    // What Tailscale says of an address holds for 30 seconds: each step
    // below asks about another of Alice's devices.
    for device in ["100.64.0.20:1", "100.64.0.21:1", "100.64.0.22:1"] {
        fake.device(device, "nANOTHER", &[], 41, "alice@example.com");
    }

    // Tailscale can't be asked: no Tailscale session works, nobody signs in
    // with it, and an anonymous caller isn't told why.
    let session = signed_in().await;
    *fake.down.lock() = true;
    assert_eq!(me(&app, "100.64.0.20:50000", &session).await, StatusCode::UNAUTHORIZED);
    let r = sign_in(&app, "100.64.0.21:50000", SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.body);
    assert!(!r.body.to_string().contains("pipe") && !r.body.to_string().contains("sock"), "{}", r.body);
    *fake.down.lock() = false;
    assert_eq!(me(&app, "100.64.0.20:50000", &session).await, StatusCode::OK, "back once it answers");

    // A device of Alice's leaves the tailnet (its address goes to someone else).
    fake.device("100.64.0.22:1", "nOTHER", &[], 43, "carol@example.com");
    assert_eq!(me(&app, "100.64.0.22:50000", &session).await, StatusCode::UNAUTHORIZED);

    // Unlinking signs out the sessions signed in with it.
    let session = signed_in().await;
    let r =
        call(&app, Method::DELETE, "/api/me/tailscale", "192.168.1.30:50000", SERVER, Some(&alice), None, &[]).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(me(&app, "100.64.0.9:50006", &session).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&app, "192.168.1.30:50000", &alice).await, StatusCode::OK, "the password's session stays");

    // So does turning it off, for good.
    assert_eq!(link(&app, ALICE, &alice, json!({"password": "test-pass-1"})).await.status, StatusCode::OK);
    let session = signed_in().await;
    turn_on(&app, &admin, false).await;
    assert_eq!(me(&app, "100.64.0.9:50007", &session).await, StatusCode::UNAUTHORIZED);
    turn_on(&app, &admin, true).await;
    assert_eq!(me(&app, "100.64.0.9:50008", &session).await, StatusCode::UNAUTHORIZED);

    // A new password removes the link, made with the old one. The session
    // that gave it goes on, as one signed in with the password.
    let session = signed_in().await;
    let new = json!({"current_password": "test-pass-1", "new_password": "test-pass-2"});
    let r =
        call(&app, Method::PUT, "/api/auth/password", "100.64.0.9:50009", SERVER, Some(&session), Some(new), &[]).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{}", r.body);
    assert_eq!(here(&app, ALICE, SERVER).await["username"], Value::Null);
    let r = sign_in(&app, ALICE, SERVER, json!({})).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    assert_eq!(me(&app, "192.168.1.30:50000", &session).await, StatusCode::OK);
    assert_eq!(me(&app, "192.168.1.30:50000", &alice).await, StatusCode::UNAUTHORIZED, "the old password's");
}

#[tokio::test]
async fn apps_can_neither_link_nor_unlink() {
    let (app, _fake, admin, _dir) = start().await;
    turn_on(&app, &admin, true).await;
    let compat = app.compatibility_token(&admin, "play").await;
    for method in [Method::GET, Method::PUT, Method::DELETE] {
        let r =
            call(&app, method.clone(), "/api/me/tailscale", ALICE, SERVER, Some(&compat), Some(json!({})), &[]).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method}: {}", r.body);
    }
}
