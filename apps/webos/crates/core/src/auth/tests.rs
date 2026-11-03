//! The device flow, refreshes, the scope guard and profile switches against
//! a fake server and a fake clock.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::device::scratch_dir;
use crate::http::{FakeTransport, Request, Response};
use crate::store::Store;
use crate::task::{Cancel, Clock};

const SERVER: &str = "http://192.168.1.20:7539";

/// Time that moves only when someone waits.
struct FakeClock {
    now: AtomicU64,
    waits: Mutex<Vec<Duration>>,
}

impl FakeClock {
    fn new() -> Arc<FakeClock> {
        Arc::new(FakeClock { now: AtomicU64::new(1_700_000_000_000), waits: Mutex::new(Vec::new()) })
    }

    fn waits(&self) -> Vec<u64> {
        self.waits.lock().unwrap().iter().map(|d| d.as_secs()).collect()
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }

    fn sleep(&self, d: Duration, cancel: &Cancel) -> bool {
        self.waits.lock().unwrap().push(d);
        self.now.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
        !cancel.is_cancelled()
    }
}

fn path_of(r: &Request) -> String {
    crate::addresses::Url::parse(&r.url).unwrap().path
}

fn form_of(r: &Request) -> Vec<(String, String)> {
    let body = String::from_utf8(r.body.clone().unwrap_or_default()).unwrap();
    body.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            let decode = |s: &str| {
                let mut out = Vec::new();
                let b = s.as_bytes();
                let mut i = 0;
                while i < b.len() {
                    if b[i] == b'%' {
                        out.push(u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap(), 16).unwrap());
                        i += 3;
                    } else {
                        out.push(b[i]);
                        i += 1;
                    }
                }
                String::from_utf8(out).unwrap()
            };
            (decode(k), decode(v))
        })
        .collect()
}

fn field(r: &Request, name: &str) -> Option<String> {
    form_of(r).into_iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn metadata(scopes: &[&str]) -> Response {
    Response::json(
        200,
        &json!({
            "issuer": SERVER,
            "token_endpoint": format!("{SERVER}/api/oauth/token"),
            "revocation_endpoint": format!("{SERVER}/api/oauth/revoke"),
            "device_authorization_endpoint": format!("{SERVER}/api/oauth/device"),
            "grant_types_supported": ["refresh_token", DEVICE_CODE_GRANT, SWITCH_PROFILE_GRANT],
            "scopes_supported": scopes,
        }),
    )
}

fn oauth_error(error: &str) -> Response {
    Response::json(400, &json!({"error": error}))
}

fn tokens_answer(n: u32, scope: &str) -> Response {
    Response::json(
        200,
        &json!({
            "access_token": format!("pley_at_{n}"),
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": scope,
            "refresh_token": format!("pley_rt_{n}"),
        }),
    )
}

/// A server whose token endpoint answers `polls` in turn (then `authorization_pending`).
fn device_server(polls: Vec<Result<Response, crate::http::NetError>>) -> (Arc<FakeTransport>, Arc<AtomicUsize>) {
    let codes = Arc::new(AtomicUsize::new(0));
    let c = codes.clone();
    let polls = Mutex::new(polls.into_iter());
    let fake = FakeTransport::new(move |r| match path_of(r).as_str() {
        METADATA_PATH => Ok(metadata(&["library", "play", "playlists", "profiles"])),
        "/api/oauth/device" => {
            let n = c.fetch_add(1, Ordering::SeqCst);
            Ok(Response::json(
                200,
                &json!({
                    "device_code": format!("device-{n}"),
                    "user_code": "bcdfghjk",
                    "verification_uri": format!("{SERVER}/link"),
                    "verification_uri_complete": format!("{SERVER}/link?code=BCDFGHJK"),
                    "expires_in": 600,
                    "interval": 5
                }),
            ))
        }
        "/api/oauth/token" => polls.lock().unwrap().next().unwrap_or_else(|| Ok(oauth_error("authorization_pending"))),
        other => panic!("unexpected {other}"),
    });
    (Arc::new(fake), codes)
}

fn sign_in(fake: &Arc<FakeTransport>, clock: &Arc<FakeClock>) -> DeviceSignIn {
    DeviceSignIn::with_clock(Http::new(fake.clone(), "ua"), clock.clone())
}

#[test]
fn a_code_is_asked_for_with_pleys_client_and_scopes_and_shown_as_the_server_says() {
    let (fake, _) = device_server(vec![]);
    let code = sign_in(&fake, &FakeClock::new()).start(SERVER).unwrap();
    assert_eq!(code.shown_code(), "BCDF-GHJK");
    assert_eq!(code.address(), "192.168.1.20:7539/link");
    assert_eq!(code.qr_address(), "http://192.168.1.20:7539/link?code=BCDFGHJK");
    assert_eq!(code.interval, Duration::from_secs(5));
    let device = fake.requests().into_iter().find(|r| path_of(r) == "/api/oauth/device").unwrap();
    assert_eq!(field(&device, "client_id").as_deref(), Some(CLIENT_ID));
    assert_eq!(field(&device, "scope").as_deref(), Some("library play playlists profiles"));
    assert_eq!(device.header_value("Content-Type"), Some("application/x-www-form-urlencoded"));
}

#[test]
fn the_scope_is_what_the_server_offers_of_pleys_own() {
    let m = ServerMetadata { scopes_supported: Some(vec!["library".into(), "play".into()]), ..Default::default() };
    assert_eq!(scope_for(&m), "library play");
    assert_eq!(scope_for(&ServerMetadata::default()), "library play playlists profiles");
    // Requests' scopes only where the server lists them: one that doesn't know a scope refuses
    // the whole sign-in.
    let all = ServerMetadata {
        scopes_supported: Some(SCOPES.iter().map(|s| s.to_string()).chain(["admin".to_string()]).collect()),
        ..ServerMetadata::default()
    };
    assert_eq!(scope_for(&all), "library play playlists profiles requests approvals");
}

#[test]
fn plain_http_beyond_home_is_refused_before_asking() {
    let (fake, _) = device_server(vec![]);
    let e = sign_in(&fake, &FakeClock::new()).start("http://pley.example.com:7539").unwrap_err();
    assert_eq!(e.to_string(), crate::addresses::UNENCRYPTED);
    assert!(fake.requests().is_empty());
}

#[test]
fn approval_is_polled_at_the_interval_and_slowed_down_for_good() {
    let (fake, _) = device_server(vec![
        Ok(oauth_error("authorization_pending")),
        Ok(oauth_error("slow_down")),
        Ok(oauth_error("authorization_pending")),
        Ok(tokens_answer(1, "library play playlists profiles")),
    ]);
    let clock = FakeClock::new();
    let flow = sign_in(&fake, &clock);
    let code = flow.start(SERVER).unwrap();
    let mut seen = Vec::new();
    let end = flow.poll(&code, &mut |p| seen.push(p.clone()), &Cancel::new());
    let DevicePoll::SignedIn(tokens) = end else { panic!("{end:?}") };
    assert_eq!(tokens.issuer, SERVER);
    assert_eq!(tokens.access_token, "pley_at_1");
    assert_eq!(tokens.refresh_token.as_deref(), Some("pley_rt_1"));
    assert_eq!(clock.waits(), [5, 5, 10, 10]);
    let poll = fake.requests().into_iter().find(|r| path_of(r) == "/api/oauth/token").unwrap();
    assert_eq!(field(&poll, "grant_type").as_deref(), Some(DEVICE_CODE_GRANT));
    assert_eq!(field(&poll, "device_code").as_deref(), Some("device-0"));
}

#[test]
fn network_failures_back_off_but_never_below_the_interval() {
    use crate::http::NetError::Refused;
    let (fake, _) = device_server(vec![
        Err(Refused),
        Err(Refused),
        Err(Refused),
        Err(Refused),
        Err(Refused),
        Err(Refused),
        Ok(oauth_error("authorization_pending")),
        Ok(oauth_error("access_denied")),
    ]);
    let clock = FakeClock::new();
    let flow = sign_in(&fake, &clock);
    let code = flow.start(SERVER).unwrap();
    let mut seen = Vec::new();
    assert_eq!(flow.poll(&code, &mut |p| seen.push(p.clone()), &Cancel::new()), DevicePoll::Denied);
    assert_eq!(clock.waits(), [5, 5, 5, 8, 16, 30, 30, 5]);
    let unreachable = |s: u64| DevicePoll::Unreachable { retry_in: Duration::from_secs(s) };
    assert_eq!(
        seen,
        [
            DevicePoll::Waiting,
            unreachable(5),
            unreachable(5),
            unreachable(8),
            unreachable(16),
            unreachable(30),
            unreachable(30),
            DevicePoll::Waiting,
            DevicePoll::Denied
        ]
    );
}

#[test]
fn an_expired_code_is_renewed_three_times_then_given_up() {
    let polls = (0..10).map(|_| Ok(oauth_error("expired_token"))).collect();
    let (fake, codes) = device_server(polls);
    let clock = FakeClock::new();
    let mut states = Vec::new();
    let end = sign_in(&fake, &clock).run(SERVER, None, &mut |s| states.push(s), &Cancel::new());
    assert_eq!(end, LinkEnd::Expired);
    assert_eq!(codes.load(Ordering::SeqCst), 4, "the first code and three renewals");
    assert_eq!(states.first(), Some(&LinkState::Starting));
    assert_eq!(states.iter().filter(|s| **s == LinkState::Starting).count(), 1, "renewals are silent");
    assert_eq!(states.iter().filter(|s| matches!(s, LinkState::Code(_))).count(), 4);
    assert_eq!(states.last(), Some(&LinkState::Expired));
}

#[test]
fn a_code_past_its_time_is_expired_without_asking() {
    let (fake, codes) = device_server(vec![]);
    let clock = FakeClock::new();
    let flow = sign_in(&fake, &clock);
    let code = flow.start(SERVER).unwrap();
    clock.now.fetch_add(601_000, Ordering::SeqCst);
    assert_eq!(flow.poll(&code, &mut |_| {}, &Cancel::new()), DevicePoll::Expired);
    assert_eq!(codes.load(Ordering::SeqCst), 1);
    assert!(fake.requests().iter().all(|r| path_of(r) != "/api/oauth/token"));
}

#[test]
fn getting_a_code_retries_by_itself_while_the_server_is_away() {
    let tries = Arc::new(AtomicUsize::new(0));
    let t = tries.clone();
    let fake = Arc::new(FakeTransport::new(move |r| {
        if path_of(r) == METADATA_PATH && t.fetch_add(1, Ordering::SeqCst) < 2 {
            return Err(crate::http::NetError::Refused);
        }
        match path_of(r).as_str() {
            METADATA_PATH => Ok(metadata(&["library", "play"])),
            "/api/oauth/device" => Ok(Response::json(
                200,
                &json!({"device_code": "d", "user_code": "BCDFGHJK", "verification_uri": format!("{SERVER}/link")}),
            )),
            _ => Ok(tokens_answer(1, "library play")),
        }
    }));
    let clock = FakeClock::new();
    let mut states = Vec::new();
    let end = sign_in(&fake, &clock).run(SERVER, None, &mut |s| states.push(s), &Cancel::new());
    assert!(matches!(end, LinkEnd::SignedIn(_)), "{end:?}");
    let retry = |s: u64| LinkState::Unreachable { retry_in: Duration::from_secs(s) };
    assert_eq!(states[..3], [LinkState::Starting, retry(2), retry(4)]);
}

#[test]
fn a_verification_page_elsewhere_is_refused() {
    let fake = Arc::new(FakeTransport::new(|r| match path_of(r).as_str() {
        METADATA_PATH => Ok(metadata(&["library"])),
        _ => Ok(Response::json(
            200,
            &json!({"device_code": "d", "user_code": "BCDFGHJK", "verification_uri": "https://evil.example.com/link"}),
        )),
    }));
    let e = sign_in(&fake, &FakeClock::new()).start(SERVER).unwrap_err();
    assert_eq!(e.to_string(), "this server sends people to another address to approve TVs: Pley doesn't");
}

/// A token endpoint that counts refreshes and hands out numbered pairs.
struct TokenServer {
    fake: Arc<FakeTransport>,
    refreshes: Arc<AtomicUsize>,
}

fn token_server(answer: impl Fn(&Request, usize) -> Response + Send + Sync + 'static) -> TokenServer {
    let refreshes = Arc::new(AtomicUsize::new(0));
    let r = refreshes.clone();
    let fake = Arc::new(FakeTransport::new(move |req| {
        if path_of(req) == "/api/oauth/revoke" {
            return Ok(Response::empty(200));
        }
        let n = r.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(answer(req, n))
    }));
    TokenServer { fake, refreshes }
}

fn stored(expires_at: u64, scope: &str) -> StoredTokens {
    StoredTokens {
        issuer: SERVER.into(),
        access_token: "pley_at_0".into(),
        refresh_token: Some("pley_rt_0".into()),
        expires_at,
        scope: scope.into(),
        token_endpoint: format!("{SERVER}/api/oauth/token"),
        revocation_endpoint: format!("{SERVER}/api/oauth/revoke"),
        server_id: None,
        proof_id: None,
        proof_key: None,
    }
}

fn tokens_with(server: &TokenServer, clock: &Arc<FakeClock>, kept: Option<StoredTokens>) -> (Tokens, Store) {
    let store = Store::open(scratch_dir("tokens")).unwrap();
    if let Some(t) = kept {
        store.set(&crate::store::keys::tokens(SERVER), &t).unwrap();
    }
    (Tokens::with_clock(SERVER, store.clone(), Http::new(server.fake.clone(), "ua"), clock.clone()), store)
}

#[test]
fn a_token_with_time_left_is_used_as_it_is_and_one_ending_soon_is_refreshed() {
    let server = token_server(|_, n| tokens_answer(n as u32, "library play"));
    let clock = FakeClock::new();
    let now = clock.now_ms();
    let (tokens, store) = tokens_with(&server, &clock, Some(stored(now + 120_000, "library play")));
    assert_eq!(tokens.access_token().unwrap(), "pley_at_0");
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 0);
    clock.now.fetch_add(61_000, Ordering::SeqCst);
    assert_eq!(tokens.access_token().unwrap(), "pley_at_1");
    let req = &server.fake.requests()[0];
    assert_eq!(field(req, "grant_type").as_deref(), Some("refresh_token"));
    assert_eq!(field(req, "refresh_token").as_deref(), Some("pley_rt_0"));
    assert_eq!(field(req, "client_id").as_deref(), Some(CLIENT_ID));
    // The new pair was kept before it was handed out.
    let kept: StoredTokens = store.get(&crate::store::keys::tokens(SERVER)).unwrap();
    assert_eq!((kept.access_token.as_str(), kept.refresh_token.as_deref()), ("pley_at_1", Some("pley_rt_1")));
}

#[test]
fn refreshes_happen_one_at_a_time_and_callers_share_the_result() {
    let gate = Arc::new(Barrier::new(4));
    let server = token_server(|_, n| {
        // Slow enough that the other callers queue behind the first refresh.
        std::thread::sleep(Duration::from_millis(50));
        tokens_answer(n as u32, "library")
    });
    let clock = FakeClock::new();
    let now = clock.now_ms();
    let (tokens, _) = tokens_with(&server, &clock, Some(stored(now + 120_000, "library")));
    let tokens = Arc::new(tokens);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (t, g) = (tokens.clone(), gate.clone());
            std::thread::spawn(move || {
                g.wait();
                t.refresh(Some("pley_at_0")).unwrap()
            })
        })
        .collect();
    let answers: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 1, "the refresh token was sent once");
    assert!(answers.iter().all(|a| a == "pley_at_1"), "{answers:?}");
}

#[test]
fn a_refused_refresh_ends_the_sign_in() {
    let server = token_server(|_, _| oauth_error("invalid_grant"));
    let clock = FakeClock::new();
    let (tokens, store) = tokens_with(&server, &clock, Some(stored(0, "library")));
    let ended = Arc::new(Mutex::new(Vec::new()));
    let e = ended.clone();
    tokens.listen(move |r| e.lock().unwrap().push(r));
    assert_eq!(tokens.access_token(), Err(AuthError::SignedOut(SignOutReason::Ended)));
    assert!(!tokens.signed_in());
    assert_eq!(store.get::<StoredTokens>(&crate::store::keys::tokens(SERVER)), None);
    assert_eq!(*ended.lock().unwrap(), [SignOutReason::Ended]);
}

#[test]
fn a_network_failure_during_a_refresh_keeps_the_sign_in() {
    let fake = Arc::new(FakeTransport::new(|_| Err(crate::http::NetError::TimedOut)));
    let clock = FakeClock::new();
    let store = Store::open(scratch_dir("tokens-offline")).unwrap();
    store.set(&crate::store::keys::tokens(SERVER), &stored(0, "library")).unwrap();
    let tokens = Tokens::with_clock(SERVER, store, Http::new(fake, "ua"), clock);
    assert!(tokens.access_token().unwrap_err().unreachable());
    assert!(tokens.signed_in());
}

#[test]
fn the_scope_guard_signs_out_when_a_refresh_widens_the_scope() {
    let server = token_server(|_, n| tokens_answer(n as u32, "library play profiles"));
    let clock = FakeClock::new();
    let (tokens, _) = tokens_with(&server, &clock, Some(stored(0, "library play")));
    assert_eq!(tokens.access_token(), Err(AuthError::SignedOut(SignOutReason::WiderScope)));
    assert_eq!(AuthError::SignedOut(SignOutReason::WiderScope).to_string(), WIDER_SCOPE);
    assert!(!tokens.signed_in());
    let revoked = server.fake.requests().into_iter().find(|r| path_of(r) == "/api/oauth/revoke").unwrap();
    assert_eq!(field(&revoked, "token").as_deref(), Some("pley_rt_1"), "the wider grant is ended");
}

#[test]
fn installing_checks_the_scope_and_ends_the_sign_in_it_replaces() {
    let server = token_server(|_, _| Response::empty(200));
    let clock = FakeClock::new();
    let (tokens, _) = tokens_with(&server, &clock, Some(stored(0, "library")));
    let mut wider = stored(clock.now_ms() + 3_600_000, "library play playlists profiles");
    wider.refresh_token = Some("pley_rt_new".into());
    assert_eq!(tokens.install(wider.clone(), "library play"), Err(AuthError::SignedOut(SignOutReason::WiderScope)));
    tokens.install(wider, "library play playlists profiles").unwrap();
    assert_eq!(tokens.load().unwrap().refresh_token.as_deref(), Some("pley_rt_new"));
    let revoked: Vec<String> = server
        .fake
        .requests()
        .iter()
        .filter(|r| path_of(r) == "/api/oauth/revoke")
        .filter_map(|r| field(r, "token"))
        .collect();
    assert_eq!(revoked, ["pley_rt_new", "pley_rt_0"]);
}

#[test]
fn refused_right_after_a_refresh_means_the_sign_in_is_over() {
    let server = token_server(|_, n| tokens_answer(n as u32, "library"));
    let clock = FakeClock::new();
    let (tokens, _) = tokens_with(&server, &clock, Some(stored(0, "library")));
    let fresh = tokens.access_token().unwrap();
    assert!(!tokens.refused_after_refresh("pley_at_0"), "an older token is just refreshed");
    assert!(tokens.refused_after_refresh(&fresh));
    assert!(!tokens.signed_in());
}

#[test]
fn switching_profile_sends_the_pin_and_keeps_the_new_pair() {
    let server = token_server(|req, n| {
        if field(req, "pin").as_deref() == Some("1234") {
            tokens_answer(n as u32, "library play profiles")
        } else {
            Response::json(
                400,
                &json!({"error": "invalid_grant", "error_description": "wrong PIN", "reason": "wrong_pin"}),
            )
        }
    });
    let clock = FakeClock::new();
    let (tokens, _) = tokens_with(&server, &clock, Some(stored(clock.now_ms() + 3_600_000, "library play profiles")));
    let wrong = tokens.switch_profile(4, Some("0000")).unwrap_err();
    assert_eq!(SwitchRefusal::of(&wrong), SwitchRefusal::WrongPin);
    assert_eq!(SwitchRefusal::of(&wrong).message(), "Wrong PIN.");
    assert_eq!(tokens.load().unwrap().access_token, "pley_at_0", "a refusal changes nothing");
    tokens.switch_profile(4, Some("1234")).unwrap();
    assert_eq!(tokens.load().unwrap().refresh_token.as_deref(), Some("pley_rt_2"));
    let req = server.fake.requests().last().cloned().unwrap();
    assert_eq!(field(&req, "grant_type").as_deref(), Some(SWITCH_PROFILE_GRANT));
    assert_eq!(field(&req, "profile").as_deref(), Some("4"));
    assert_eq!(field(&req, "refresh_token").as_deref(), Some("pley_rt_0"));
}

#[test]
fn a_spent_refresh_token_in_a_switch_signs_out_and_reasons_are_worded() {
    let server = token_server(|_, _| oauth_error("invalid_grant"));
    let clock = FakeClock::new();
    let (tokens, _) = tokens_with(&server, &clock, Some(stored(clock.now_ms() + 3_600_000, "library profiles")));
    assert_eq!(tokens.switch_profile(3, None), Err(AuthError::SignedOut(SignOutReason::Ended)));
    let refusal = |error: &str, reason: Option<&str>| {
        SwitchRefusal::of(&AuthError::Refused {
            status: 400,
            oauth: Some(crate::model::OAuthError {
                error: error.into(),
                error_description: None,
                reason: reason.map(Into::into),
            }),
        })
    };
    assert_eq!(refusal("invalid_grant", Some("pin_locked")), SwitchRefusal::PinLocked);
    assert_eq!(refusal("invalid_grant", Some("browser")), SwitchRefusal::Browser);
    assert_eq!(refusal("invalid_request", None), SwitchRefusal::CantCheckPin);
    assert_eq!(refusal("unauthorized_client", None), SwitchRefusal::CantSwitch);
    assert_eq!(
        refusal("invalid_grant", Some("throttled")).message(),
        "Too many wrong PINs: try again in a few minutes."
    );
}

#[test]
fn signing_out_revokes_and_forgets_whatever_the_server_says() {
    let fake = Arc::new(FakeTransport::new(|_| Err(crate::http::NetError::Refused)));
    let clock = FakeClock::new();
    let store = Store::open(scratch_dir("tokens-signout")).unwrap();
    store.set(&crate::store::keys::tokens(SERVER), &stored(0, "library")).unwrap();
    let tokens = Tokens::with_clock(SERVER, store, Http::new(fake.clone(), "ua"), clock);
    tokens.sign_out();
    assert!(!tokens.signed_in());
    assert_eq!(field(&fake.requests()[0], "token").as_deref(), Some("pley_rt_0"));
}

#[test]
fn tokens_of_one_server_are_shared() {
    let store = Store::open(scratch_dir("tokens-shared")).unwrap();
    let http = Http::new(Arc::new(FakeTransport::new(|_| Ok(Response::empty(200)))), "ua");
    let a = Tokens::shared(SERVER, &store, &http);
    let b = Tokens::shared(SERVER, &store, &http);
    assert!(Arc::ptr_eq(&a, &b));
    assert!(!Arc::ptr_eq(&a, &Tokens::shared("http://nas:7539", &store, &http)));
}

#[test]
fn stored_tokens_keep_the_web_apps_shape_and_never_print_secrets() {
    let t = stored(1, "library");
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["accessToken"], "pley_at_0");
    assert_eq!(v["expiresAt"], 1);
    assert_eq!(v["tokenEndpoint"], format!("{SERVER}/api/oauth/token"));
    assert!(!format!("{t:?}").contains("pley_at_0"));
}
