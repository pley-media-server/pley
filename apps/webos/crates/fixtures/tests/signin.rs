//! Signing in a TV with a code, tokens, profiles, and every route's token
//! check, against the fixture server.

mod common;

use std::net::TcpStream;
use std::time::Duration;

use common::{Api, post_form, send};
use pley_fixtures::{DeviceApproval, FixtureServer, ManualClock, Mode, Options, ids};
use serde_json::Value;

const SCOPE: &str = "library play playlists profiles";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const SWITCH_PROFILE: &str = "urn:pley:params:oauth:grant-type:switch-profile";

fn code(base: &str) -> Value {
    let reply = post_form(base, "/api/oauth/device", &[("client_id", ids::CLIENT_ID), ("scope", SCOPE)]);
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.header("Cache-Control"), Some("no-store"));
    reply.json()
}

fn poll(base: &str, device_code: &str) -> common::Reply {
    post_form(
        base,
        "/api/oauth/token",
        &[("grant_type", DEVICE_GRANT), ("device_code", device_code), ("client_id", ids::CLIENT_ID)],
    )
}

fn refresh(base: &str, token: &str) -> common::Reply {
    post_form(
        base,
        "/api/oauth/token",
        &[("grant_type", "refresh_token"), ("refresh_token", token), ("client_id", ids::CLIENT_ID)],
    )
}

fn me(base: &str, token: &str) -> common::Reply {
    send(base, "GET", "/api/auth/me", &[("Authorization", &format!("Bearer {token}"))], b"")
}

#[test]
fn the_server_and_its_metadata() {
    let server = FixtureServer::start();
    let info = send(server.url(), "GET", "/api/server", &[], b"").json();
    assert_eq!(info["name"], ids::SERVER_NAME);
    assert_eq!(info["version"], ids::SERVER_VERSION);
    assert_eq!(info["setup_required"], false);
    assert_eq!(send(server.url(), "GET", "/api/health", &[], b"").json()["status"], "ok");

    let metadata = send(server.url(), "GET", "/.well-known/oauth-authorization-server", &[], b"").json();
    assert_eq!(metadata["issuer"], server.url());
    assert_eq!(metadata["device_authorization_endpoint"], format!("{}/api/oauth/device", server.url()));
    assert_eq!(metadata["token_endpoint"], format!("{}/api/oauth/token", server.url()));
    let grants: Vec<&str> =
        metadata["grant_types_supported"].as_array().unwrap().iter().map(|g| g.as_str().unwrap()).collect();
    assert!(grants.contains(&DEVICE_GRANT) && grants.contains(&SWITCH_PROFILE));
    assert_eq!(
        metadata["scopes_supported"],
        serde_json::json!(["library", "play", "playlists", "profiles", "requests", "approvals"])
    );
    assert_eq!(send(server.url(), "GET", "/.well-known/other", &[], b"").status, 404);
}

#[test]
fn a_code_is_approved_at_once() {
    let server = FixtureServer::start();
    let base = server.url();
    let device = code(base);
    let user_code = device["user_code"].as_str().unwrap();
    assert_eq!(user_code.len(), 9);
    assert_eq!(&user_code[4..5], "-");
    assert!(user_code.replace('-', "").bytes().all(|b| b"BCDFGHJKLMNPQRSTVWXZ".contains(&b)));
    assert_eq!(device["verification_uri"], format!("{base}/link"));
    assert_eq!(device["verification_uri_complete"], format!("{base}/link?code={}", user_code.replace('-', "")));
    assert_eq!(device["expires_in"], 600);
    assert_eq!(device["interval"], 5);
    assert_eq!(device["device_code"].as_str().unwrap().len(), 43);

    let tokens = poll(base, device["device_code"].as_str().unwrap());
    assert_eq!(tokens.status, 200, "{}", tokens.text());
    let tokens = tokens.json();
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 3_600);
    assert_eq!(tokens["scope"], SCOPE);
    let access = tokens["access_token"].as_str().unwrap();
    let refresh_token = tokens["refresh_token"].as_str().unwrap();
    assert!(access.starts_with("pley_at_") && access.len() == 51);
    assert!(refresh_token.starts_with("pley_rt_") && refresh_token.len() == 51);

    let profile = me(base, access).json();
    assert_eq!(profile["id"], ids::ALEX);
    assert_eq!(profile["name"], "Alex");
    assert_eq!(profile["is_admin"], false);
    assert_eq!(profile["has_pin"], false);
    assert_eq!(profile["subtitle_mode"], "forced");
    assert_eq!(profile["scope"], SCOPE);
    assert!(profile["display_name"].is_string() && profile["max_age"].is_null());

    // Answered once.
    let again = poll(base, device["device_code"].as_str().unwrap());
    assert_eq!(again.status, 400);
    assert_eq!(again.json()["error"], "invalid_grant");
}

#[test]
fn a_slow_answer_waits_and_slows_polls_down() {
    let clock = ManualClock::new(1_793_400_000_000);
    let options = Options {
        clock: clock.clock(),
        device_delay: Duration::from_secs(30),
        device_interval_secs: 5,
        scan_every: None,
        ..Options::default()
    };
    let server = FixtureServer::start_with(options).unwrap();
    let base = server.url();
    let device_code = code(base)["device_code"].as_str().unwrap().to_string();
    assert_eq!(poll(base, &device_code).json()["error"], "authorization_pending");
    clock.advance(Duration::from_secs(1));
    assert_eq!(poll(base, &device_code).json()["error"], "slow_down");
    // The interval is 5 s longer for good: 6 s is too soon now.
    clock.advance(Duration::from_secs(6));
    assert_eq!(poll(base, &device_code).json()["error"], "slow_down");
    clock.advance(Duration::from_secs(15));
    assert_eq!(poll(base, &device_code).json()["error"], "authorization_pending");
    clock.advance(Duration::from_secs(15));
    let tokens = poll(base, &device_code);
    assert_eq!(tokens.status, 200, "{}", tokens.text());
}

#[test]
fn refused_and_expired_codes() {
    let clock = ManualClock::new(1_793_400_000_000);
    let server =
        FixtureServer::start_with(Options { clock: clock.clock(), scan_every: None, ..Options::default() }).unwrap();
    let base = server.url();
    server.set_device_approval(DeviceApproval::Deny);
    let denied = code(base)["device_code"].as_str().unwrap().to_string();
    assert_eq!(poll(base, &denied).json()["error"], "access_denied");
    assert_eq!(poll(base, &denied).json()["error"], "invalid_grant");

    server.set_device_approval(DeviceApproval::Never);
    let never = code(base)["device_code"].as_str().unwrap().to_string();
    assert_eq!(poll(base, &never).json()["error"], "authorization_pending");
    clock.advance(Duration::from_secs(601));
    assert_eq!(poll(base, &never).json()["error"], "expired_token");

    let wrong_client = post_form(base, "/api/oauth/device", &[("client_id", "https://example.invalid/app.json")]);
    assert_eq!(wrong_client.status, 401);
    assert_eq!(wrong_client.json()["error"], "invalid_client");
    let wrong_scope = post_form(base, "/api/oauth/device", &[("client_id", ids::CLIENT_ID), ("scope", "openid")]);
    assert_eq!(wrong_scope.json()["error"], "invalid_scope");
    let not_a_form = send(base, "POST", "/api/oauth/device", &[("Content-Type", "application/json")], b"{}");
    assert_eq!(not_a_form.json()["error"], "invalid_request");
}

#[test]
fn refresh_rotates_and_a_reused_token_ends_the_sign_in() {
    let server = FixtureServer::start();
    let base = server.url();
    let first = server.sign_in(ids::ALEX);
    let second = refresh(base, &first.refresh_token);
    assert_eq!(second.status, 200, "{}", second.text());
    let second = second.json();
    let access = second["access_token"].as_str().unwrap().to_string();
    assert_ne!(access, first.access_token);
    assert_eq!(me(base, &access).status, 200);

    // The first refresh token again: the whole sign-in ends.
    let reused = refresh(base, &first.refresh_token);
    assert_eq!(reused.status, 400);
    assert_eq!(reused.json()["error"], "invalid_grant");
    let refused = me(base, &access);
    assert_eq!(refused.status, 401);
    assert_eq!(refused.header("WWW-Authenticate"), Some("Bearer error=\"invalid_token\""));
    assert_eq!(refresh(base, second["refresh_token"].as_str().unwrap()).status, 400);
}

#[test]
fn the_server_proves_itself_with_the_sign_ins_key() {
    use ring::hmac;
    let server = FixtureServer::start();
    let base = server.url();
    let first = server.sign_in(ids::ALEX);
    // The same key at every refresh, one of its own for every sign-in.
    let second = refresh(base, &first.refresh_token).json();
    assert_eq!(second["server_id"], ids::SERVER_ID);
    assert_eq!(second["proof_id"], first.proof_id);
    assert_eq!(second["proof_key"], first.proof_key);
    assert_eq!(first.proof_key.len(), 43);
    assert_ne!(server.sign_in(ids::ALEX).proof_key, first.proof_key);

    let challenge = "AAECAwQFBgcICQoLDA0ODw";
    let path = format!("/api/server?challenge={challenge}&proof_id={}", first.proof_id);
    let reply = send(base, "GET", &path, &[], b"");
    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(reply.header("Cache-Control"), Some("no-store"));
    let answer = reply.json();
    assert_eq!(answer["id"], ids::SERVER_ID);
    assert_eq!(answer["address"], base);
    let decode = |text: &str| {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let bits: Vec<u32> = text.bytes().map(|c| alphabet.iter().position(|a| *a == c).unwrap() as u32).collect();
        let mut out = Vec::new();
        for chunk in bits.chunks(4) {
            let n = chunk.iter().enumerate().fold(0, |n, (i, b)| n | (b << (18 - 6 * i)));
            out.extend((0..chunk.len() - 1).map(|i| (n >> (16 - 8 * i)) as u8));
        }
        out
    };
    let key = hmac::Key::new(hmac::HMAC_SHA256, &decode(&first.proof_key));
    let text = format!("pley-server-proof\n{}\n{base}\n{challenge}", ids::SERVER_ID);
    hmac::verify(&key, text.as_bytes(), &decode(answer["proof"].as_str().unwrap())).expect("the proof verifies");

    // Without a challenge, no proof; half a request, or one written wrong, is refused.
    assert!(send(base, "GET", "/api/server", &[], b"").json().get("proof").is_none());
    for wrong in [
        format!("/api/server?challenge={challenge}"),
        "/api/server?proof_id=1".to_string(),
        format!("/api/server?challenge={challenge}&proof_id=01"),
        "/api/server?challenge=AAECAwQ&proof_id=1".to_string(),
        format!("/api/server?challenge={challenge}=&proof_id=1"),
    ] {
        assert_eq!(send(base, "GET", &wrong, &[], b"").status, 400, "{wrong}");
    }
}

#[test]
fn expired_access_tokens_refresh() {
    let server = FixtureServer::start();
    let base = server.url();
    let tokens = server.sign_in(ids::ALEX);
    server.expire_access_tokens();
    assert_eq!(me(base, &tokens.access_token).status, 401);
    let fresh = refresh(base, &tokens.refresh_token).json();
    assert_eq!(me(base, fresh["access_token"].as_str().unwrap()).status, 200);
}

#[test]
fn revoking_and_signing_out_end_the_sign_in() {
    let server = FixtureServer::start();
    let base = server.url();
    let tokens = server.sign_in(ids::ALEX);
    let revoked =
        post_form(base, "/api/oauth/revoke", &[("token", &tokens.refresh_token), ("client_id", ids::CLIENT_ID)]);
    assert_eq!(revoked.status, 200);
    assert_eq!(me(base, &tokens.access_token).status, 401);

    let other = server.sign_in(ids::ALEX);
    let out =
        send(base, "POST", "/api/auth/logout", &[("Authorization", &format!("Bearer {}", other.access_token))], b"");
    assert_eq!(out.status, 204);
    assert_eq!(me(base, &other.access_token).status, 401);
}

#[test]
fn profiles_and_switching() {
    let server = FixtureServer::start();
    let base = server.url();
    let tokens = server.sign_in(ids::ALEX);
    let alex = Api { base: base.to_string(), token: tokens.access_token.clone() };
    let profiles = alex.get("/api/oauth/profiles").json();
    let list = profiles.as_array().unwrap();
    assert_eq!(list.len(), 3);
    let needs: Vec<(i64, &str, bool)> = list
        .iter()
        .map(|p| (p["id"].as_i64().unwrap(), p["needs"].as_str().unwrap(), p["current"].as_bool().unwrap()))
        .collect();
    assert_eq!(needs, vec![(ids::ALEX, "none", true), (ids::SAM, "pin", false), (ids::ROBIN, "password", false)]);
    assert!(list[1]["avatar_url"].is_null() && list[1]["avatar_color"] == "#a3e635");

    let switch = |profile: i64, pin: Option<&str>, refresh: &str| {
        let profile = profile.to_string();
        let mut form = vec![
            ("grant_type", SWITCH_PROFILE),
            ("client_id", ids::CLIENT_ID),
            ("refresh_token", refresh),
            ("profile", profile.as_str()),
        ];
        if let Some(pin) = pin {
            form.push(("pin", pin));
        }
        post_form(base, "/api/oauth/token", &form)
    };
    let no_pin = switch(ids::SAM, None, &tokens.refresh_token).json();
    assert_eq!((no_pin["error"].as_str(), no_pin["reason"].as_str()), (Some("invalid_grant"), Some("wrong_pin")));
    let wrong = switch(ids::SAM, Some("9999"), &tokens.refresh_token).json();
    assert_eq!(wrong["reason"], "wrong_pin");
    let robin = switch(ids::ROBIN, None, &tokens.refresh_token).json();
    assert_eq!(robin["reason"], "browser");
    let own = switch(ids::ALEX, None, &tokens.refresh_token).json();
    assert_eq!(own["error"], "invalid_request");

    // A refusal spends nothing: the right PIN still works with the same token.
    let sam = switch(ids::SAM, Some(ids::PIN), &tokens.refresh_token);
    assert_eq!(sam.status, 200, "{}", sam.text());
    let sam_token = sam.json()["access_token"].as_str().unwrap().to_string();
    let sam_me = me(base, &sam_token).json();
    assert_eq!((sam_me["id"].as_i64(), sam_me["has_pin"].as_bool()), (Some(ids::SAM), Some(true)));
    assert_eq!(me(base, &tokens.access_token).status, 401, "the old tokens stop at once");
    let sam_api = Api { base: base.to_string(), token: sam_token };
    assert_eq!(sam_api.get("/api/oauth/profiles").json()[1]["current"], true);
}

#[test]
fn every_route_checks_the_token() {
    let server = FixtureServer::start();
    let base = server.url();
    let none = send(base, "GET", "/api/libraries", &[], b"");
    assert_eq!(none.status, 401);
    assert_eq!(none.header("WWW-Authenticate"), Some("Bearer"));
    assert_eq!(none.json()["code"], "unauthorized");
    let bad = send(base, "GET", "/api/libraries", &[("Authorization", "Bearer pley_at_nothing")], b"");
    assert_eq!(bad.status, 401);
    let api = Api::alex(&server);
    let in_query = api.get(&format!("/api/libraries?token={}", api.token));
    assert_eq!(in_query.status, 400);
    let web_only = api.get("/api/activity");
    assert_eq!(web_only.status, 403);
    assert_eq!(web_only.json()["code"], "insufficient_scope");
    assert_eq!(api.get("/api/nothing/here").json()["code"], "not_found");
    assert_eq!(api.json("POST", "/api/libraries", &serde_json::json!({})).status, 405);
    let not_a_number = api.get("/api/items/abc");
    assert_eq!(not_a_number.status, 400);
    assert!(not_a_number.header("Content-Type").unwrap().starts_with("text/plain"));

    // Without a scope: libraries need `library`.
    let device = post_form(base, "/api/oauth/device", &[("client_id", ids::CLIENT_ID), ("scope", "play")]).json();
    let narrow = poll(base, device["device_code"].as_str().unwrap()).json();
    let narrow = Api { base: base.to_string(), token: narrow["access_token"].as_str().unwrap().to_string() };
    let refused = narrow.get("/api/libraries");
    assert_eq!(refused.status, 403);
    assert_eq!(refused.header("WWW-Authenticate"), Some("Bearer error=\"insufficient_scope\", scope=\"library\""));
    assert_eq!(narrow.get("/api/auth/me").status, 200);
}

#[test]
fn unknown_tokens_are_adopted_when_asked() {
    let server = FixtureServer::start();
    server.set_adopt_unknown_tokens(true);
    let saved = Api { base: server.url().to_string(), token: format!("pley_at_{}", "x".repeat(43)) };
    assert_eq!(saved.get("/api/auth/me").json()["id"], ids::ALEX);
    let refreshed = refresh(server.url(), &format!("pley_rt_{}", "y".repeat(43)));
    assert_eq!(refreshed.status, 200, "{}", refreshed.text());
}

#[test]
fn down_refuses_connections_on_the_same_port() {
    let server = FixtureServer::start();
    let address = server.url().strip_prefix("http://").unwrap().to_string();
    server.set_mode(Mode::Down).unwrap();
    assert!(TcpStream::connect(&address).is_err());
    server.set_mode(Mode::Normal).unwrap();
    assert_eq!(send(server.url(), "GET", "/api/health", &[], b"").status, 200);
    server.stop();
    assert!(TcpStream::connect(&address).is_err());
    server.stop();
}
