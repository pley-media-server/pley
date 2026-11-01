use std::sync::Mutex;

use super::*;

/// Tokens that hand out `token-<n>` and count refreshes.
struct Counting {
    current: Mutex<String>,
    refreshes: Mutex<u32>,
    just_issued: bool,
}

impl Counting {
    fn new() -> Arc<Counting> {
        Arc::new(Counting { current: Mutex::new("token-0".into()), refreshes: Mutex::new(0), just_issued: false })
    }
}

impl TokenSource for Counting {
    fn access_token(&self) -> Result<String, ApiError> {
        Ok(self.current.lock().unwrap().clone())
    }

    fn refresh(&self, _stale: &str) -> Result<String, ApiError> {
        let mut n = self.refreshes.lock().unwrap();
        *n += 1;
        let fresh = format!("token-{n}");
        *self.current.lock().unwrap() = fresh.clone();
        Ok(fresh)
    }

    fn refused_after_refresh(&self, _sent: &str) -> bool {
        self.just_issued
    }
}

fn bearer(r: &Request) -> Option<String> {
    r.header_value("Authorization").map(str::to_string)
}

#[test]
fn the_token_goes_to_the_servers_own_origin_only() {
    let fake = Arc::new(FakeTransport::new(|_| Ok(Response::json(200, &serde_json::json!({})))));
    let http = Http::new(fake.clone(), "pleyLG/0.0.0 (webOS 23; test)");
    let api = Authorized::new(http, "https://pley.example.com", Counting::new());
    for url in [
        "https://pley.example.com/api/hubs/home",
        "https://pley.example.com:443/api/items/1",
        "https://evil.example.com/api/items/1",
        "http://pley.example.com/api/items/1",
        "https://pley.example.com:8443/x",
    ] {
        api.send(Request::get(url)).unwrap();
    }
    let seen: Vec<Option<String>> = fake.requests().iter().map(bearer).collect();
    assert_eq!(seen, [Some("Bearer token-0".to_string()), Some("Bearer token-0".to_string()), None, None, None]);
    assert!(fake.requests().iter().all(|r| r.header_value("User-Agent") == Some("pleyLG/0.0.0 (webOS 23; test)")));
}

#[test]
fn a_refused_token_is_refreshed_once_and_the_request_repeated() {
    let fake = Arc::new(FakeTransport::new(|r| {
        Ok(if bearer(r).as_deref() == Some("Bearer token-0") {
            let mut a = Response::json(
                401,
                &serde_json::json!({"code": "unauthorized", "message": "this token doesn't work any more"}),
            );
            a.headers.push(("WWW-Authenticate".into(), "Bearer error=\"invalid_token\"".into()));
            a
        } else {
            Response::json(200, &serde_json::json!({"hubs": []}))
        })
    }));
    let tokens = Counting::new();
    let api = Authorized::new(Http::new(fake.clone(), "ua"), "http://192.168.1.20:7539", tokens.clone());
    let answer = api.send(Request::get("http://192.168.1.20:7539/api/hubs/home")).unwrap();
    assert_eq!(answer.status, 200);
    assert_eq!(*tokens.refreshes.lock().unwrap(), 1);
    assert_eq!(fake.asked(), ["GET /api/hubs/home", "GET /api/hubs/home"]);
}

#[test]
fn other_refusals_are_not_refreshed() {
    let fake = Arc::new(FakeTransport::new(|_| {
        let mut a = Response::json(
            403,
            &serde_json::json!({"code": "insufficient_scope", "message": "this app wasn't allowed to play titles"}),
        );
        a.headers.push(("WWW-Authenticate".into(), "Bearer error=\"insufficient_scope\", scope=\"play\"".into()));
        Ok(a)
    }));
    let tokens = Counting::new();
    let api = Authorized::new(Http::new(fake, "ua"), "http://nas:7539", tokens.clone());
    let e = api.send(Request::get("http://nas:7539/api/playback")).unwrap_err();
    assert_eq!(e.code(), Some("insufficient_scope"));
    assert_eq!(e.to_string(), "this app wasn't allowed to play titles");
    assert_eq!(*tokens.refreshes.lock().unwrap(), 0);
}

#[test]
fn refused_right_after_a_refresh_is_signed_out() {
    let fake = Arc::new(FakeTransport::new(|_| Ok(Response::empty(401))));
    let tokens = Arc::new(Counting { just_issued: true, ..Arc::into_inner(Counting::new()).unwrap() });
    let api = Authorized::new(Http::new(fake, "ua"), "http://nas:7539", tokens);
    assert_eq!(api.send(Request::get("http://nas:7539/api/auth/me")).unwrap_err(), ApiError::SignedOut);
}

#[test]
fn errors_read_pleys_body_or_fall_back_to_the_status() {
    let e = ApiError::from_response(&Response::json(
        404,
        &serde_json::json!({"code": "not_found", "message": "item 42 not found"}),
    ));
    assert_eq!((e.status(), e.code(), e.to_string().as_str()), (Some(404), Some("not_found"), "item 42 not found"));
    let plain = Response { status: 422, headers: vec![], body: b"Failed to deserialize the JSON body".to_vec() };
    let e = ApiError::from_response(&plain);
    assert_eq!((e.code(), e.to_string().as_str()), (Some("bad_request"), "the server refused (HTTP 422)"));
    let e = ApiError::from_response(&Response::empty(503));
    assert!(e.unreachable());
    assert_eq!(e.code(), Some("unavailable"));
    let stopped = Response::json(
        410,
        &serde_json::json!({"code": "session_stopped", "error": "session_stopped", "message": "Time for bed"}),
    );
    assert_eq!(ApiError::from_response(&stopped), ApiError::SessionStopped { message: Some("Time for bed".into()) });
    let bare = Response::json(
        410,
        &serde_json::json!({"code": "session_stopped", "error": "session_stopped", "message": null}),
    );
    assert_eq!(ApiError::from_response(&bare).to_string(), "Playback was stopped by the server's administrator");
}

#[test]
fn a_busy_source_lists_its_connections() {
    let body = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/live/busy.json")).unwrap();
    let e = ApiError::from_response(&Response { status: 409, headers: vec![], body });
    let busy = e.busy().expect("a 409 with connections");
    assert!(!busy.connections.is_empty());
    assert!(busy.message.contains("in use"));
}

#[test]
fn unreachable_is_never_a_cancel() {
    assert!(ApiError::Net(NetError::Refused).unreachable());
    assert_eq!(ApiError::Net(NetError::Refused).unreachable_reason(), "Nothing answers there.");
    assert_eq!(ApiError::Net(NetError::TimedOut).unreachable_reason(), "No answer in time.");
    assert!(!ApiError::Net(NetError::Cancelled).unreachable());
    assert_eq!(
        ApiError::Net(NetError::TimedOut).problem(),
        "No answer in time. Check that the server is on and that this device is on its network."
    );
    assert_eq!(ApiError::SignedOut.problem(), "You were signed out: sign in again.");
}

#[test]
fn queries_and_forms_encode_as_javascript_does() {
    assert_eq!(
        with_query(
            "/api/libraries/1/items",
            &[("sort", Some("title".into())), ("genre", None), ("q", Some("Tom & Jerry's".into()))]
        ),
        "/api/libraries/1/items?sort=title&q=Tom%20%26%20Jerry's"
    );
    assert_eq!(with_query("/x?a=1", &[("b", Some("2".into()))]), "/x?a=1&b=2");
    assert_eq!(
        form(&[("grant_type", "refresh_token"), ("scope", "library play")]),
        "grant_type=refresh_token&scope=library%20play"
    );
}

#[test]
fn gzip_answers_are_unpacked() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(br#"{"status":"ok"}"#).unwrap();
    let packed = gz.finish().unwrap();
    let fake = Arc::new(FakeTransport::new(move |_| {
        Ok(Response {
            status: 200,
            headers: vec![("Content-Encoding".into(), "gzip".into()), ("Content-Length".into(), "99".into())],
            body: packed.clone(),
        })
    }));
    let answer = Http::new(fake.clone(), "ua").send(Request::get("http://nas:7539/api/health")).unwrap();
    assert_eq!(answer.text(), r#"{"status":"ok"}"#);
    assert_eq!(answer.header("Content-Length"), None);
    assert_eq!(fake.requests()[0].header_value("Accept-Encoding"), Some("gzip"));
}

#[test]
fn home_servers_get_a_short_connect_timeout() {
    assert_eq!(Request::get("http://192.168.1.20:7539/").effective_connect_timeout(), HOME_CONNECT_TIMEOUT);
    assert_eq!(Request::get("http://100.101.102.103:7539/").effective_connect_timeout(), CONNECT_TIMEOUT);
    assert_eq!(Request::get("https://pley.example.com/").effective_connect_timeout(), CONNECT_TIMEOUT);
}
