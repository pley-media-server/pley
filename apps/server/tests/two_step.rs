//! Two-step sign-in end to end (docs/API.md: Two-step sign-in): setting it
//! up, signing in with codes and recovery codes, the limits on guessing,
//! the other ways into an account, and the ways back in for someone who
//! lost their phone.

mod common;

use axum::http::{Method, StatusCode, header};
use base64::Engine;
use serde_json::{Value, json};

use common::{Reply, TestApp};
use pley::two_step::code_at;
use pley::util::now;

async fn sign_in(app: &TestApp, username: &str, password: &str, code: Option<&str>) -> Reply {
    let mut body = json!({"username": username, "password": password});
    if let Some(code) = code {
        body["code"] = json!(code);
    }
    app.call(Method::POST, "/api/auth/login", None, Some(body), &[]).await
}

/// The code the app shows next (the current one turned two-step sign-in on).
fn next_code(secret: &[u8]) -> String {
    code_at(secret, now() + 30)
}

/// A code that isn't the app's now, nor a step before or after.
fn wrong_code(secret: &[u8]) -> String {
    let near: Vec<String> = (-2..=2).map(|k| code_at(secret, now() + k * 30)).collect();
    (0..).map(|n| format!("{n:06}")).find(|code| !near.contains(code)).unwrap()
}

async fn two_step_of(app: &TestApp, token: &str) -> Value {
    let status = app.get("/api/me/two-step", token).await;
    assert_eq!(status.status, StatusCode::OK, "{}", status.text());
    status.body
}

/// Wrong codes in a row, as the database keeps them.
async fn failures(app: &TestApp, user_id: i64) -> i64 {
    sqlx::query_scalar("SELECT failures FROM two_step WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

/// A server with an administrator and bob (password `test-pass-1`), who
/// turned two-step sign-in on: `(app, admin, bob, bob_id, secret, recovery codes)`.
async fn with_bob() -> (TestApp, String, String, i64, Vec<u8>, Vec<String>) {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let (secret, codes) = app.turn_on_two_step(&bob, "test-pass-1").await;
    (app, admin, bob, bob_id, secret, codes)
}

#[tokio::test]
async fn turning_it_on_takes_the_password_and_a_first_code() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    assert_eq!(two_step_of(&app, &bob).await, json!({"enabled": false, "since": null, "recovery_codes_left": 0}));

    let refused = app.post("/api/me/two-step/setup", &bob, json!({"password": "wrong"})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "a session alone isn't enough");
    let setup = app.post("/api/me/two-step/setup", &bob, json!({"password": "test-pass-1"})).await;
    assert_eq!(setup.status, StatusCode::OK, "{}", setup.text());
    let text = setup.body["secret"].as_str().unwrap();
    let uri = setup.body["uri"].as_str().unwrap();
    assert!(uri.starts_with("otpauth://totp/Pley:bob") && uri.contains(&format!("secret={text}&")), "{uri}");
    assert!(setup.body["qr"]["path"].as_str().unwrap().starts_with("M0 0h7v1h-7z"));
    assert!(setup.body["expires_at"].as_i64().unwrap() > now());
    let secret = pley::two_step::base32_decode(text).unwrap();
    assert_eq!(secret.len(), 20);

    // Off until a code from it shows the app has it.
    assert_eq!(two_step_of(&app, &bob).await["enabled"], false);
    let wrong = app.post("/api/me/two-step", &bob, json!({"code": wrong_code(&secret)})).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let on = app.post("/api/me/two-step", &bob, json!({"code": code_at(&secret, now())})).await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text());
    let codes = on.body["recovery_codes"].as_array().unwrap();
    assert_eq!(codes.len(), 10);
    let status = two_step_of(&app, &bob).await;
    assert_eq!((&status["enabled"], &status["recovery_codes_left"]), (&json!(true), &json!(10)));
    assert!(status["since"].as_i64().unwrap() >= now() - 60);
    assert_eq!(app.get("/api/auth/me", &bob).await.body["two_step"], true);
    let users = app.get("/api/users", &admin).await.body;
    assert!(users.as_array().unwrap().iter().any(|u| u["username"] == "bob" && u["two_step"] == true));

    // Setting it up again moves it to another authenticator, which takes a
    // code of this one too, as turning it off does.
    let again = app.post("/api/me/two-step/setup", &bob, json!({"password": "test-pass-1"})).await;
    assert_eq!((again.status, &again.body["code"]), (StatusCode::UNAUTHORIZED, &json!("two_step")));

    // The database alone can't make codes: the secret is sealed, the
    // recovery codes hashed.
    let stored: String = sqlx::query_scalar("SELECT secret FROM two_step WHERE user_id = ?")
        .bind(bob_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    let stored = base64::engine::general_purpose::STANDARD.decode(stored).unwrap();
    assert!(!stored.windows(secret.len()).any(|w| w == secret.as_slice()));
    let hashes: Vec<String> = sqlx::query_scalar("SELECT code_hash FROM recovery_codes WHERE user_id = ?")
        .bind(bob_id)
        .fetch_all(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(hashes.len(), 10);
    for code in codes {
        let code = code.as_str().unwrap().replace('-', "");
        assert!(hashes.iter().all(|h| !h.contains(&code)));
    }
}

#[tokio::test]
async fn managed_profiles_and_accounts_without_a_password_have_no_second_step() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let kid = app.post("/api/users", &admin, json!({"username": "kid", "managed": true})).await;
    let kid = app.post("/api/auth/switch", &admin, json!({"user_id": kid.body["id"]})).await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let refused = app.post("/api/me/two-step/setup", &kid, json!({"password": ""})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn signing_in_takes_a_code_and_each_code_works_once() {
    let (app, _admin, _bob, _, secret, _) = with_bob().await;

    let asked = sign_in(&app, "bob", "test-pass-1", None).await;
    assert_eq!((asked.status, asked.body["code"].clone()), (StatusCode::UNAUTHORIZED, json!("two_step")));
    assert!(asked.text().contains("authenticator app"), "{}", asked.text());
    assert!(asked.headers.get(header::SET_COOKIE).is_none(), "no session before the second step");
    assert!(asked.body.get("token").is_none());

    // A wrong password says just that, whatever the code.
    let wrong_password = sign_in(&app, "bob", "nope", Some(&next_code(&secret))).await;
    assert_eq!(wrong_password.body["code"], "unauthorized");
    let wrong = sign_in(&app, "bob", "test-pass-1", Some(&wrong_code(&secret))).await;
    assert_eq!((wrong.status, wrong.body["code"].clone()), (StatusCode::UNAUTHORIZED, json!("two_step")));
    assert!(wrong.text().contains("didn't work"), "{}", wrong.text());

    let next = next_code(&secret);
    let typed = format!("{} {}", &next[..3], &next[3..]);
    let signed_in = sign_in(&app, "bob", "test-pass-1", Some(&typed)).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    assert!(signed_in.body["token"].is_string());

    // Someone who saw it can't use it again.
    let again = sign_in(&app, "bob", "test-pass-1", Some(&next)).await;
    assert_eq!(again.body["code"], "two_step");
    assert!(again.text().contains("used already"), "{}", again.text());
}

#[tokio::test]
async fn recovery_codes_sign_in_once_and_can_be_replaced() {
    let (app, _admin, bob, _, _secret, codes) = with_bob().await;

    // As people type them: any case, spaces for dashes.
    let typed = codes[0].to_uppercase().replace('-', " ");
    assert_eq!(sign_in(&app, "bob", "test-pass-1", Some(&typed)).await.status, StatusCode::OK);
    let again = sign_in(&app, "bob", "test-pass-1", Some(&codes[0])).await;
    assert_eq!(again.body["code"], "two_step");
    assert_eq!(two_step_of(&app, &bob).await["recovery_codes_left"], 9);

    let refused = app.post("/api/me/two-step/recovery-codes", &bob, json!({"password": "nope"})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    let fresh = app.post("/api/me/two-step/recovery-codes", &bob, json!({"password": "test-pass-1"})).await;
    assert_eq!(fresh.status, StatusCode::OK, "{}", fresh.text());
    let fresh: Vec<String> =
        fresh.body["recovery_codes"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_owned()).collect();
    assert_eq!(fresh.len(), 10);
    assert!(fresh.iter().all(|c| !codes.contains(c)));
    assert_eq!(two_step_of(&app, &bob).await["recovery_codes_left"], 10);
    let old = sign_in(&app, "bob", "test-pass-1", Some(&codes[1])).await;
    assert_eq!(old.body["code"], "two_step", "the old ones stopped working");
    assert_eq!(sign_in(&app, "bob", "test-pass-1", Some(&fresh[0])).await.status, StatusCode::OK);
}

#[tokio::test]
async fn guessing_is_throttled_and_100_wrong_codes_in_a_row_leave_recovery_codes() {
    let (app, admin, _bob, bob_id, secret, _) = with_bob().await;
    for _ in 0..5 {
        let wrong = sign_in(&app, "bob", "test-pass-1", Some(&wrong_code(&secret))).await;
        assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    }
    let locked = sign_in(&app, "bob", "test-pass-1", Some(&next_code(&secret))).await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS, "even the right code, for a while");
    assert_eq!(failures(&app, bob_id).await, 5, "counted in the database too");

    // Weeks of guessing later: 100 wrong in a row, and codes from the app
    // stop working; a recovery code still signs in, and starts the count over.
    let (ann, ann_id) = app.user(&admin, "ann", json!({})).await;
    let (secret, codes) = app.turn_on_two_step(&ann, "test-pass-1").await;
    sqlx::query("UPDATE two_step SET failures = 100 WHERE user_id = ?")
        .bind(ann_id)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let refused = sign_in(&app, "ann", "test-pass-1", Some(&next_code(&secret))).await;
    assert_eq!(refused.body["code"], "two_step");
    assert!(refused.text().contains("recovery code"), "{}", refused.text());
    assert_eq!(sign_in(&app, "ann", "test-pass-1", Some(&codes[0])).await.status, StatusCode::OK);
    assert_eq!(failures(&app, ann_id).await, 0);
    let back = sign_in(&app, "ann", "test-pass-1", Some(&next_code(&secret))).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());
}

#[tokio::test]
async fn turning_it_off_takes_the_password_and_a_code() {
    let (app, _admin, bob, _, secret, _) = with_bob().await;
    let off = |body: Value| app.call(Method::DELETE, "/api/me/two-step", Some(&bob), Some(body), &[]);
    let no_code = off(json!({"password": "test-pass-1"})).await;
    assert_eq!(no_code.body["code"], "two_step", "the password alone can't");
    let no_password = off(json!({"password": "nope", "code": next_code(&secret)})).await;
    assert_eq!(no_password.status, StatusCode::FORBIDDEN, "nor a code alone");
    let done = off(json!({"password": "test-pass-1", "code": next_code(&secret)})).await;
    assert_eq!(done.status, StatusCode::NO_CONTENT, "{}", done.text());
    assert_eq!(two_step_of(&app, &bob).await, json!({"enabled": false, "since": null, "recovery_codes_left": 0}));
    assert_eq!(sign_in(&app, "bob", "test-pass-1", None).await.status, StatusCode::OK);
    assert_eq!(off(json!({"password": "test-pass-1"})).await.status, StatusCode::BAD_REQUEST, "off already");
}

#[tokio::test]
async fn administrators_turn_it_off_for_someone_who_lost_their_phone() {
    let (app, admin, bob, bob_id, _, _) = with_bob().await;
    let path = format!("/api/users/{bob_id}/two-step");
    assert_eq!(app.delete(&path, &bob).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&path, &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.delete(&path, &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(sign_in(&app, "bob", "test-pass-1", None).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &bob).await.body["two_step"], false);
}

#[tokio::test]
async fn the_command_line_reset_turns_it_off() {
    let (app, _admin, _bob, _, _, _) = with_bob().await;
    pley::reset_password(app.state.config.clone(), "bob", "another-pass-2", false).await.unwrap();
    assert_eq!(sign_in(&app, "bob", "another-pass-2", None).await.status, StatusCode::OK);
}

#[tokio::test]
async fn switching_to_a_profile_with_two_step_sign_in_takes_its_code() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (secret, _) = app.turn_on_two_step(&admin, "secret-one").await;
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let (kid, _) = app.user(&admin, "kid", json!({"max_age": 12})).await;

    // The administrator's password signs in to it: so does its code.
    let password_only =
        app.post("/api/auth/switch", &kid, json!({"user_id": admin_id, "password": "secret-one"})).await;
    assert_eq!(password_only.body["code"], "two_step");
    let body = json!({"user_id": admin_id, "password": "secret-one", "code": next_code(&secret)});
    let switched = app.post("/api/auth/switch", &kid, body).await;
    assert_eq!(switched.status, StatusCode::OK, "{}", switched.text());
    assert_eq!(switched.body["user"]["username"], "admin");
}

#[tokio::test]
async fn a_database_without_its_key_leaves_recovery_codes() {
    let (app, _admin, _bob, bob_id, secret, codes) = with_bob().await;
    // The same database next to another key (brought over without secret.key).
    let elsewhere = tempfile::tempdir().unwrap();
    let moved = pley::two_step::TwoStep::new(app.state.db.clone(), elsewhere.path());
    let throttle = pley::accounts::Throttle::default();
    match moved.check(&throttle, bob_id, Some(&next_code(&secret))).await {
        Err(pley::error::AppError::Unavailable(message)) => assert!(message.contains("recovery code"), "{message}"),
        other => panic!("expected the key to be missed, got {other:?}"),
    }
    assert!(moved.check(&throttle, bob_id, Some(&codes[0])).await.is_ok());
}

#[tokio::test]
async fn a_key_the_server_cant_use_spends_no_recovery_code_on_another_app() {
    let (mut app, _admin, bob, _, secret, codes) = with_bob().await;
    // Pley can't read it any more (its account changed, say).
    std::fs::write(app.state.config.data_dir.join("secret.key"), "not a key").unwrap();
    app.restart().await;

    // The authenticator's codes can't be checked: fixing the key brings them
    // back, a recovery code signs in meanwhile.
    let code = sign_in(&app, "bob", "test-pass-1", Some(&next_code(&secret))).await;
    assert_eq!(code.status, StatusCode::SERVICE_UNAVAILABLE, "{}", code.text());
    assert!(code.mentions("fixes its secret.key") && code.mentions("recovery code"), "{}", code.text());
    // No other app can be set up either, which is said before the code is
    // checked: the recovery code stays.
    let body = json!({"password": "test-pass-1", "code": codes[0]});
    let moving = app.post("/api/me/two-step/setup", &bob, body).await;
    assert_eq!(moving.status, StatusCode::SERVICE_UNAVAILABLE, "{}", moving.text());
    assert!(moving.mentions("secret.key"), "{}", moving.text());
    assert_eq!(two_step_of(&app, &bob).await["recovery_codes_left"], 10);
    assert_eq!(sign_in(&app, "bob", "test-pass-1", Some(&codes[0])).await.status, StatusCode::OK);
}
