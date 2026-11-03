//! No token leaves before the server at the address proves it is the one
//! signed in to (docs/API.md, "Telling servers apart"), as in the web TV
//! app's server-proof tests, Android's `ServerProofTest` and the native
//! Samsung app's `ServerProofTests`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::{Value, json};

use super::*;
use crate::auth::{AuthError, StoredTokens, Tokens, parse_token_answer};
use crate::device::scratch_dir;
use crate::http::{ApiError, Authorized, FakeTransport, Request, Response};
use crate::store::{Store, keys};
use crate::task::{Cancel, Clock};

const SERVER: &str = "http://192.168.1.20:7539";
const ID: &str = "0123456789abcdef0123456789abcdef01234567";
const KEY: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";
const OTHER_KEY: &str = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk";
/// The bytes 0 to 15 that the rigs' challenges are made of.
const CHALLENGE: &str = "AAECAwQFBgcICQoLDA0ODw";

/// The network count is the process's: tests that count proofs take turns.
fn one_at_a_time() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(|e| e.into_inner())
}

fn the_proof() -> ServerProof {
    ServerProof { server_id: ID.into(), proof_id: "17".into(), proof_key: KEY.into() }
}

fn proof_of(challenge: &str) -> String {
    proof(&from_base64url(KEY).unwrap(), ID, SERVER, challenge)
}

fn text(status: u16, body: &str) -> Response {
    Response { status, headers: Vec::new(), body: body.as_bytes().to_vec() }
}

fn tokens_answer(body: Value) -> Response {
    Response::json(200, &body)
}

struct ManualClock(AtomicU64);

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }

    fn sleep(&self, d: Duration, cancel: &Cancel) -> bool {
        self.0.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
        !cancel.is_cancelled()
    }
}

type Answer = Box<dyn Fn(&str) -> Option<Response> + Send + Sync>;

/// A TV signed in to [`SERVER`], whose server answers proofs as `answer`
/// says (none: nothing answers) and records what it was asked.
struct Rig {
    fake: Arc<FakeTransport>,
    clock: Arc<ManualClock>,
    answer: Arc<Mutex<Answer>>,
    token_answer: Arc<Mutex<Response>>,
    /// The API's other requests get no answer.
    api_down: Arc<AtomicBool>,
    store: Store,
    tokens: Arc<Tokens>,
    _turn: MutexGuard<'static, ()>,
}

impl Rig {
    fn new() -> Rig {
        Rig::with(true, 3_600_000)
    }

    fn with(with_proof: bool, expires_in_ms: u64) -> Rig {
        let turn = one_at_a_time();
        let clock = Arc::new(ManualClock(AtomicU64::new(1_000_000)));
        let answer: Arc<Mutex<Answer>> = Arc::new(Mutex::new(Box::new(|c: &str| {
            Some(Response::json(200, &json!({ "id": ID, "name": "Den", "proof": proof_of(c) })))
        })));
        let token_answer = Arc::new(Mutex::new(tokens_answer(json!({
            "access_token": "pley_at_new", "token_type": "Bearer", "expires_in": 3600, "refresh_token": "pley_rt_new",
        }))));
        let api_down = Arc::new(AtomicBool::new(false));
        let (a, t, down) = (answer.clone(), token_answer.clone(), api_down.clone());
        let fake = Arc::new(FakeTransport::new(move |req: &Request| {
            let rest = req.url.strip_prefix(SERVER).unwrap();
            let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
            match path {
                "/api/server" => {
                    let challenge = query.split('&').find_map(|p| p.strip_prefix("challenge=")).unwrap();
                    (**a.lock().unwrap())(challenge).ok_or(NetError::Refused)
                }
                "/api/oauth/token" => Ok(t.lock().unwrap().clone()),
                "/api/oauth/revoke" => Ok(Response::empty(200)),
                _ if down.load(Ordering::SeqCst) => Err(NetError::Refused),
                _ => Ok(Response::json(200, &json!({}))),
            }
        }));
        let store = Store::open(scratch_dir("proofs")).unwrap();
        let proof = with_proof.then(the_proof);
        let kept = StoredTokens {
            issuer: SERVER.into(),
            access_token: "pley_at_a".into(),
            refresh_token: Some("pley_rt_b".into()),
            expires_at: clock.now_ms() + expires_in_ms,
            scope: "library play".into(),
            token_endpoint: format!("{SERVER}/api/oauth/token"),
            revocation_endpoint: format!("{SERVER}/api/oauth/revoke"),
            server_id: proof.as_ref().map(|p| p.server_id.clone()),
            proof_id: proof.as_ref().map(|p| p.proof_id.clone()),
            proof_key: proof.as_ref().map(|p| p.proof_key.clone()),
        };
        store.set(&keys::tokens(SERVER), &kept).unwrap();
        let tokens = Arc::new(tokens_of(&fake, &store, &clock));
        Rig { fake, clock, answer, token_answer, api_down, store, tokens, _turn: turn }
    }

    /// Other tokens of the same sign-in, reading what is kept.
    fn new_tokens(&self) -> Tokens {
        tokens_of(&self.fake, &self.store, &self.clock)
    }

    fn answer(&self, f: impl Fn(&str) -> Option<Response> + Send + Sync + 'static) {
        *self.answer.lock().unwrap() = Box::new(f);
    }

    fn advance(&self, ms: i64) {
        let now = self.clock.0.load(Ordering::SeqCst) as i64;
        self.clock.0.store((now + ms) as u64, Ordering::SeqCst);
    }

    fn paths(&self) -> Vec<String> {
        self.fake
            .requests()
            .iter()
            .map(|r| r.url.strip_prefix(SERVER).unwrap().split('?').next().unwrap().to_string())
            .collect()
    }

    fn proofs(&self) -> usize {
        self.paths().iter().filter(|p| *p == "/api/server").count()
    }
}

/// The tokens kept in `store`, asking through `fake`, their challenges the bytes 0, 1, 2…
fn tokens_of(fake: &Arc<FakeTransport>, store: &Store, clock: &Arc<ManualClock>) -> Tokens {
    Tokens::with_clock(SERVER, store.clone(), Http::new(fake.clone(), "ua"), clock.clone())
        .with_random(|n| (0..n as u8).collect())
}

fn another_device(e: AuthError) {
    assert_eq!(e, AuthError::Unreachable(NetError::AnotherDevice));
}

#[test]
fn the_server_proves_itself_once_before_the_token_goes() {
    let rig = Rig::new();
    rig.answer(|c| {
        // Slow enough that the other caller waits on this proof.
        std::thread::sleep(Duration::from_millis(50));
        Some(Response::json(200, &json!({ "id": ID, "proof": proof_of(c) })))
    });
    let gate = Arc::new(Barrier::new(2));
    let both: Vec<_> = (0..2)
        .map(|_| {
            let (t, g) = (rig.tokens.clone(), gate.clone());
            std::thread::spawn(move || {
                g.wait();
                t.access_token().unwrap()
            })
        })
        .collect();
    let both: Vec<String> = both.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(both, ["pley_at_a", "pley_at_a"]);
    let asked = rig.fake.requests();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].url, format!("{SERVER}/api/server?challenge={CHALLENGE}&proof_id=17"));
    // Without the token, and never cancelled with the screen that asked.
    assert_eq!(asked[0].header_value("Authorization"), None);
    assert!(asked[0].cancel.is_none());
}

#[test]
fn it_proves_itself_again_after_half_an_hour_or_a_request_without_an_answer() {
    let rig = Rig::new();
    rig.tokens.access_token().unwrap();
    rig.advance(TTL_MS as i64 - 1);
    rig.tokens.access_token().unwrap();
    assert_eq!(rig.proofs(), 1);
    rig.advance(1);
    rig.tokens.access_token().unwrap();
    assert_eq!(rig.proofs(), 2);
    rig.tokens.doubt_server();
    rig.tokens.access_token().unwrap();
    assert_eq!(rig.proofs(), 3);
}

#[test]
fn it_proves_itself_again_on_another_network_or_after_the_clock_went_back() {
    let rig = Rig::new();
    rig.tokens.access_token().unwrap();
    network_may_have_changed();
    rig.tokens.access_token().unwrap();
    assert_eq!(rig.proofs(), 2);
    rig.advance(-60_000);
    rig.tokens.access_token().unwrap();
    assert_eq!(rig.proofs(), 3);
}

#[test]
fn it_proves_itself_again_when_the_network_changed_while_a_proof_was_under_way() {
    let rig = Rig::new();
    let changed = AtomicBool::new(false);
    rig.answer(move |c| {
        // The TV joins another network as the first proof is answered.
        if !changed.swap(true, Ordering::SeqCst) {
            network_may_have_changed();
        }
        Some(Response::json(200, &json!({ "id": ID, "proof": proof_of(c) })))
    });
    assert_eq!(rig.tokens.access_token().unwrap(), "pley_at_a");
    assert_eq!(rig.proofs(), 2);
}

#[test]
fn another_device_at_the_address_gets_no_token() {
    let others: Vec<(u16, String)> = vec![
        (200, json!({ "id": ID, "proof": "AAAA" }).to_string()),
        (200, json!({ "id": "another", "proof": proof_of(CHALLENGE) }).to_string()),
        (200, json!({ "id": ID, "proof": null }).to_string()),
        (200, json!({ "id": ID }).to_string()),
        // A redirect, which could lead to the real server from anywhere, proves nothing.
        (302, String::new()),
        (403, r#"{"message":"a proof for a plain-http address is made for the home network only"}"#.into()),
        (404, "Not found".into()),
        (200, "<html>a router</html>".into()),
    ];
    for (status, body) in others {
        let rig = Rig::new();
        rig.answer(move |_| Some(text(status, &body)));
        let e = rig.tokens.access_token().unwrap_err();
        // Out of reach, as the TV shows it.
        let shown = ApiError::from(e.clone());
        assert!(shown.unreachable(), "{status}");
        assert_eq!(shown.unreachable_reason(), "Another device answers there.");
        another_device(e);
        another_device(rig.tokens.refresh(Some("pley_at_a")).unwrap_err());
        assert!(rig.paths().iter().all(|p| p == "/api/server"), "{status}: {:?}", rig.paths());
    }
}

#[test]
fn a_server_error_is_out_of_reach_not_another_device() {
    let rig = Rig::new();
    rig.answer(|_| Some(text(502, "Bad Gateway")));
    assert_eq!(rig.tokens.access_token(), Err(AuthError::Unreachable(NetError::ServerFailed)));
}

#[test]
fn nothing_answering_is_out_of_reach_and_tried_again() {
    let rig = Rig::new();
    let up = Arc::new(AtomicBool::new(false));
    let u = up.clone();
    rig.answer(move |c| {
        u.load(Ordering::SeqCst).then(|| Response::json(200, &json!({ "id": ID, "proof": proof_of(c) })))
    });
    assert_eq!(rig.tokens.access_token(), Err(AuthError::Unreachable(NetError::Refused)));
    up.store(true, Ordering::SeqCst);
    assert_eq!(rig.tokens.access_token().unwrap(), "pley_at_a");
}

#[test]
fn a_refresh_waits_for_a_proof_made_then_and_keeps_the_key() {
    let rig = Rig::new();
    rig.tokens.access_token().unwrap();
    // A refresh's answer bringing another key doesn't replace the sign-in's.
    *rig.token_answer.lock().unwrap() = tokens_answer(json!({
        "access_token": "pley_at_new", "token_type": "Bearer", "expires_in": 3600, "refresh_token": "pley_rt_new",
        "server_id": ID, "proof_id": "18", "proof_key": OTHER_KEY,
    }));
    assert_eq!(rig.tokens.refresh(Some("pley_at_a")).unwrap(), "pley_at_new");
    assert_eq!(rig.paths(), ["/api/server", "/api/server", "/api/oauth/token"]);
    assert_eq!(rig.new_tokens().load().unwrap().proof(), Some(the_proof()));
}

#[test]
fn a_token_about_to_end_waits_for_one_proof_its_refreshs() {
    let rig = Rig::with(true, 30_000);
    assert_eq!(rig.tokens.access_token().unwrap(), "pley_at_new");
    assert_eq!(rig.paths(), ["/api/server", "/api/oauth/token"]);
}

#[test]
fn a_token_another_refresh_gave_goes_out_only_after_a_proof() {
    let rig = Rig::new();
    assert_eq!(rig.tokens.refresh(Some("pley_at_old")).unwrap(), "pley_at_a");
    assert_eq!(rig.paths(), ["/api/server"]);
}

#[test]
fn a_profile_switch_and_its_pin_wait_for_a_proof_made_then() {
    let rig = Rig::new();
    rig.tokens.access_token().unwrap();
    rig.tokens.switch_profile(2, Some("1234")).unwrap();
    assert_eq!(rig.paths(), ["/api/server", "/api/server", "/api/oauth/token"]);
    drop(rig);
    let away = Rig::new();
    away.answer(|_| Some(text(200, "{}")));
    another_device(away.tokens.switch_profile(2, Some("1234")).unwrap_err());
    assert!(away.fake.requests().iter().all(|r| r.body.is_none()), "the PIN stays on the TV");
}

#[test]
fn signing_out_sends_another_device_nothing() {
    let away = Rig::new();
    away.answer(|_| Some(Response::json(200, &json!({ "id": ID, "proof": "AAAA" }))));
    away.tokens.sign_out();
    assert_eq!(away.paths(), ["/api/server"]);
    assert!(!away.tokens.signed_in());
    drop(away);
    // At its own server, the sign-in ends there.
    let home = Rig::new();
    home.tokens.sign_out();
    assert_eq!(home.paths(), ["/api/server", "/api/oauth/revoke"]);
    let body = home.fake.requests().last().unwrap().body.clone().unwrap();
    assert!(String::from_utf8(body).unwrap().contains("pley_rt_b"));
}

#[test]
fn a_new_sign_in_ends_the_one_before_only_where_the_server_proves_its_key() {
    let fresh = StoredTokens {
        issuer: SERVER.into(),
        access_token: "pley_at_c".into(),
        refresh_token: Some("pley_rt_d".into()),
        expires_at: 9_000_000,
        scope: "library play".into(),
        token_endpoint: format!("{SERVER}/api/oauth/token"),
        revocation_endpoint: format!("{SERVER}/api/oauth/revoke"),
        server_id: Some(ID.into()),
        proof_id: Some("18".into()),
        proof_key: Some(OTHER_KEY.into()),
    };
    // Another device that got the TV to sign in again doesn't receive the old refresh token.
    let away = Rig::new();
    away.answer(|_| Some(Response::json(200, &json!({ "id": ID, "proof": "AAAA" }))));
    away.tokens.install(fresh.clone(), "library play").unwrap();
    assert_eq!(away.paths(), ["/api/server"]);
    assert_eq!(away.tokens.load().unwrap().access_token, "pley_at_c");
    drop(away);
    let home = Rig::new();
    home.tokens.install(fresh, "library play").unwrap();
    assert_eq!(home.paths(), ["/api/server", "/api/oauth/revoke"]);
    let body = home.fake.requests().last().unwrap().body.clone().unwrap();
    assert!(String::from_utf8(body).unwrap().contains("pley_rt_b"));
}

#[test]
fn a_sign_in_from_a_server_without_proofs_checks_nothing() {
    let rig = Rig::with(false, 3_600_000);
    assert_eq!(rig.tokens.access_token().unwrap(), "pley_at_a");
    rig.tokens.sign_out();
    assert_eq!(rig.paths(), ["/api/oauth/revoke"]);
}

#[test]
fn a_request_without_an_answer_has_the_server_prove_itself_again() {
    let rig = Rig::new();
    rig.api_down.store(true, Ordering::SeqCst);
    let api = Authorized::new(Http::new(rig.fake.clone(), "ua"), SERVER, rig.tokens.clone());
    let libraries = || api.send(Request::get(format!("{SERVER}/api/libraries")));
    assert!(libraries().unwrap_err().unreachable());
    assert_eq!(rig.proofs(), 1);
    rig.api_down.store(false, Ordering::SeqCst);
    libraries().unwrap();
    assert_eq!(rig.proofs(), 2);
}

#[test]
fn the_key_is_kept_with_the_tokens_and_never_printed() {
    let rig = Rig::new();
    let kept = rig.tokens.load().unwrap();
    assert_eq!(kept.proof(), Some(the_proof()));
    assert!(!format!("{:?}", the_proof()).contains(KEY));
    assert!(!format!("{kept:?}").contains(KEY));
    let answer = json!({ "access_token": "a", "server_id": ID, "proof_id": "17", "proof_key": KEY });
    let parsed = parse_token_answer(answer.to_string().as_bytes(), "library").unwrap();
    assert_eq!(parsed.proof, Some(the_proof()));
    assert!(!format!("{parsed:?}").contains(KEY));
}

#[test]
fn base64url_reads_only_its_one_spelling() {
    let bytes: Vec<u8> = (0..=255).collect();
    for n in 0..=bytes.len() {
        assert_eq!(from_base64url(&base64url(&bytes[..n])).as_deref(), Some(&bytes[..n]));
    }
    for wrong in ["A", "AB", "AAB", "AA==", "A+A", "A/A", "A A"] {
        assert_eq!(from_base64url(wrong), None, "{wrong}");
    }
}

/// The token answers and proofs of packages/tv/test/fixtures/auth.json,
/// which the web TV app's and the native Samsung app's tests read too.
/// `auth.json` here is a copy of its `tokenResponse` and `serverProof`
/// cases: change both together.
const SHARED: &str = include_str!("auth.json");

#[test]
fn token_answers_read_as_the_shared_fixture_says() {
    let fixture: Value = serde_json::from_str(SHARED).unwrap();
    let cases = fixture["tokenResponse"].as_array().unwrap();
    assert_eq!(cases.len(), 17);
    for case in cases {
        let body = &case["in"]["body"];
        // A body that is a string is the answer's text as it came.
        let bytes = body.as_str().map_or_else(|| serde_json::to_vec(body).unwrap(), |t| t.as_bytes().to_vec());
        let asked = case["in"]["asked"].as_str().unwrap();
        let out = &case["out"];
        match parse_token_answer(&bytes, asked) {
            Ok(parsed) => {
                let ok = &out["ok"];
                assert_eq!(parsed.access_token, ok["accessToken"].as_str().unwrap(), "{case}");
                assert_eq!(parsed.refresh_token.as_deref(), ok["refreshToken"].as_str(), "{case}");
                assert_eq!(parsed.expires_in, ok["expiresIn"].as_u64().unwrap(), "{case}");
                assert_eq!(parsed.scope, ok["scope"].as_str().unwrap(), "{case}");
                let proof = ok.get("proof").map(|p| ServerProof {
                    server_id: p["serverId"].as_str().unwrap().into(),
                    proof_id: p["proofId"].as_str().unwrap().into(),
                    proof_key: p["proofKey"].as_str().unwrap().into(),
                });
                assert_eq!(parsed.proof, proof, "{case}");
            }
            Err(e) => assert_eq!(Some(e.to_string().as_str()), out["error"].as_str(), "{case}"),
        }
    }
}

#[test]
fn proofs_are_the_shared_fixtures_known_answers() {
    let fixture: Value = serde_json::from_str(SHARED).unwrap();
    let cases = fixture["serverProof"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let i = &case["in"];
        let key = from_base64url(i["key"].as_str().unwrap()).unwrap();
        let made = proof(
            &key,
            i["serverId"].as_str().unwrap(),
            i["address"].as_str().unwrap(),
            i["challenge"].as_str().unwrap(),
        );
        assert_eq!(made, case["out"].as_str().unwrap(), "{case}");
    }
}
