//! Two-step sign-in required of everyone (docs/API.md: Two-step sign-in; the
//! `require_two_step` setting): who may require it, what happens to what
//! people without it had signed in, setting it up as one signs in, joins with
//! an invitation or is switched to, the apps' way through the browser and the
//! TVs' through `/link`, keeping it on while moving it to another app, and
//! what still holds while the server can't use its key.

mod common;

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};

use common::{ANDROID_APP, ROKU_APP, Reply, TestApp, encode};
use pley::auth::grants::NewGrant;
use pley::auth::scopes::Scopes;
use pley::two_step::{base32_decode, code_at};
use pley::util::now;

async fn sign_in(app: &TestApp, username: &str, password: &str, code: Option<&str>) -> Reply {
    let mut body = json!({"username": username, "password": password});
    if let Some(code) = code {
        body["code"] = json!(code);
    }
    app.call(Method::POST, "/api/auth/login", None, Some(body), &[]).await
}

/// Signs in setting two-step sign-in up, with the `id` of the secret shown.
async fn sign_in_with(app: &TestApp, username: &str, password: &str, id: &str, code: Option<&str>) -> Reply {
    let mut body = json!({"username": username, "password": password, "setup_id": id});
    if let Some(code) = code {
        body["code"] = json!(code);
    }
    app.call(Method::POST, "/api/auth/login", None, Some(body), &[]).await
}

/// The `id` a `two_step_setup` answer shows its secret with.
fn id_of(reply: &Reply) -> String {
    reply.body["setup"]["id"].as_str().unwrap().to_owned()
}

/// The secret a `two_step_setup` answer asks to add to an authenticator app.
fn secret_of(reply: &Reply) -> Vec<u8> {
    assert_eq!(
        (reply.status, reply.body["code"].as_str()),
        (StatusCode::UNAUTHORIZED, Some("two_step_setup")),
        "{}",
        reply.text()
    );
    assert!(reply.headers.get(header::SET_COOKIE).is_none(), "no session before it is set up");
    assert!(reply.body.get("token").is_none());
    base32_decode(reply.body["setup"]["secret"].as_str().unwrap()).unwrap()
}

/// The code the app shows next (the current one may have been used).
fn next_code(secret: &[u8]) -> String {
    code_at(secret, now() + 30)
}

/// A code that isn't the app's now, nor a step before or after.
fn wrong_code(secret: &[u8]) -> String {
    let near: Vec<String> = (-2..=2).map(|k| code_at(secret, now() + k * 30)).collect();
    (0..).map(|n| format!("{n:06}")).find(|code| !near.contains(code)).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    value.as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_owned()).collect()
}

/// Requires two-step sign-in (`on`) as the administrator signed in with `admin`.
async fn require(app: &TestApp, admin: &str, on: bool) -> Reply {
    app.put("/api/settings", admin, json!({"require_two_step": on})).await
}

async fn server_requires(app: &TestApp) -> Value {
    app.call(Method::GET, "/api/server", None, None, &[]).await.body["two_step_required"].clone()
}

/// A server whose administrator (password `secret-one`) uses two-step
/// sign-in: `(app, admin's token, admin's recovery codes)`.
async fn with_admin() -> (TestApp, String, Vec<String>) {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, codes) = app.turn_on_two_step(&admin, "secret-one").await;
    (app, admin, codes)
}

/// Signs `username` in, setting two-step sign-in up on the way: the
/// session's token, the secret and the recovery codes.
async fn set_up_signing_in(app: &TestApp, username: &str, password: &str) -> (String, Vec<u8>, Vec<String>) {
    let asked = sign_in(app, username, password, None).await;
    let secret = secret_of(&asked);
    let done = sign_in_with(app, username, password, &id_of(&asked), Some(&code_at(&secret, now()))).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
    (done.body["token"].as_str().unwrap().to_owned(), secret, strings(&done.body["recovery_codes"]))
}

/// A managed profile in the household: its id.
async fn managed_kid(app: &TestApp, admin: &str) -> i64 {
    let created = app.post("/api/users", admin, json!({"username": "kid", "managed": true})).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.body["id"].as_i64().unwrap()
}

/// The access token of pley's Android app acting as `user_id`, allowed by
/// `approved_by`.
async fn app_allowed_by(app: &TestApp, user_id: i64, approved_by: i64) -> String {
    let grant = NewGrant {
        user_id,
        approved_by,
        client_id: ANDROID_APP.into(),
        client_name: Some("pley".into()),
        scopes: Scopes::parse("library").unwrap(),
        refresh: true,
        first_party: true,
        session_hash: None,
        user_agent: None,
        ip: None,
    };
    app.state.auth.issue(grant).await.unwrap().access_token
}

async fn refresh(app: &TestApp, token: &str) -> Reply {
    let form = format!("grant_type=refresh_token&refresh_token={}&client_id={}", encode(token), encode(ANDROID_APP));
    app.form_post("/api/oauth/token", &form, &[]).await
}

/// Sessions and apps signed in as `user_id`, as the database keeps them.
async fn signed_in(app: &TestApp, user_id: i64) -> (i64, i64) {
    let sessions = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    let apps = sqlx::query_scalar("SELECT COUNT(*) FROM app_grants WHERE user_id = ? OR approved_by = ?")
        .bind(user_id)
        .bind(user_id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    (sessions, apps)
}

#[tokio::test]
async fn only_an_administrator_who_uses_it_can_require_it() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    assert_eq!(server_requires(&app).await, false);
    assert_eq!(app.get("/api/settings", &admin).await.body["require_two_step"], false, "off on a new server");

    assert_eq!(require(&app, &bob, true).await.status, StatusCode::FORBIDDEN);
    // Nobody locks themselves out by requiring what they don't use.
    let refused = require(&app, &admin, true).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{}", refused.text());
    assert!(refused.text().contains("your own account first"), "{}", refused.text());
    assert_eq!(app.get("/api/settings", &admin).await.body["require_two_step"], false, "nothing was saved");

    app.turn_on_two_step(&admin, "secret-one").await;
    let on = require(&app, &admin, true).await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.text());
    assert_eq!(on.body["require_two_step"], true);
    assert_eq!(server_requires(&app).await, true, "sign-in pages and apps can say so");
    let off = require(&app, &admin, false).await;
    assert_eq!(off.body["require_two_step"], false);
    assert_eq!(server_requires(&app).await, false);
}

#[tokio::test]
async fn requiring_it_signs_out_whatever_signed_in_with_a_password_alone() {
    let (app, admin, _) = with_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let (carol, carol_id) = app.user(&admin, "carol", json!({})).await;
    app.turn_on_two_step(&carol, "test-pass-1").await;
    let kid = managed_kid(&app, &admin).await;
    let (bob_app, bob_refresh) = app.app_token(bob_id, "library play").await;
    let (carol_app, carol_refresh) = app.app_token(carol_id, "library play").await;
    // The kid's profile on two TVs, one allowed by each of them.
    let kid_by_bob = app_allowed_by(&app, kid, bob_id).await;
    let kid_by_carol = app_allowed_by(&app, kid, carol_id).await;
    assert_eq!(signed_in(&app, bob_id).await, (1, 2));

    assert_eq!(require(&app, &admin, true).await.status, StatusCode::OK);

    // bob signed in with his password alone: signed out everywhere.
    for token in [&bob, &bob_app, &kid_by_bob] {
        assert_eq!(app.get("/api/auth/me", token).await.status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(refresh(&app, &bob_refresh).await.body["error"], "invalid_grant");
    assert_eq!(signed_in(&app, bob_id).await, (0, 0));
    // Whoever uses it carries on, with the managed profiles they allowed.
    for token in [&admin, &carol, &carol_app, &kid_by_carol] {
        assert_eq!(app.get("/api/auth/me", token).await.status, StatusCode::OK);
    }
    assert_eq!(refresh(&app, &carol_refresh).await.status, StatusCode::OK);

    // Required no more: a password alone signs in again.
    assert_eq!(require(&app, &admin, false).await.status, StatusCode::OK);
    let back = sign_in(&app, "bob", "test-pass-1", None).await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());
    assert!(back.body.get("recovery_codes").is_none());
}

#[tokio::test]
async fn signing_in_sets_it_up_right_after_the_password() {
    let (app, admin, _) = with_admin().await;
    app.user(&admin, "bob", json!({})).await;
    require(&app, &admin, true).await;

    let asked = sign_in(&app, "bob", "test-pass-1", None).await;
    let secret = secret_of(&asked);
    assert_eq!(secret.len(), 20);
    assert!(asked.text().contains("requires two-step sign-in"), "{}", asked.text());
    let setup = &asked.body["setup"];
    let uri = setup["uri"].as_str().unwrap();
    assert!(uri.starts_with("otpauth://totp/Pley:bob") && uri.contains(setup["secret"].as_str().unwrap()), "{uri}");
    assert!(setup["qr"]["path"].as_str().unwrap().starts_with("M0 0h7v1h-7z"));
    let expires_at = setup["expires_at"].as_i64().unwrap();
    assert!((now() + 1700..=now() + 1800).contains(&expires_at), "{expires_at}");

    // Asked again with its id, the same secret: what the app added still works.
    let id = id_of(&asked);
    assert_eq!(secret_of(&sign_in_with(&app, "bob", "test-pass-1", &id, None).await), secret);
    // A wrong password learns nothing about it.
    let wrong_password = sign_in_with(&app, "bob", "nope", &id, Some(&code_at(&secret, now()))).await;
    assert_eq!(wrong_password.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong_password.body["code"], "unauthorized");
    assert!(wrong_password.body.get("setup").is_none() && !wrong_password.mentions(setup["secret"].as_str().unwrap()));
    // A wrong code, or something that isn't one, keeps it.
    let wrong = sign_in_with(&app, "bob", "test-pass-1", &id, Some(&wrong_code(&secret))).await;
    assert_eq!(secret_of(&wrong), secret);
    assert!(wrong.text().contains("didn't work"), "{}", wrong.text());
    let letters = sign_in_with(&app, "bob", "test-pass-1", &id, Some("abcdef")).await;
    assert_eq!(secret_of(&letters), secret);
    assert!(letters.text().contains("6-digit code"), "{}", letters.text());

    let done = sign_in_with(&app, "bob", "test-pass-1", &id, Some(&code_at(&secret, now()))).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
    assert!(done.headers.get(header::SET_COOKIE).is_some());
    assert_eq!(done.body["user"]["two_step"], true);
    let codes = strings(&done.body["recovery_codes"]);
    assert_eq!(codes.len(), 10);
    let token = done.body["token"].as_str().unwrap();
    let status = app.get("/api/me/two-step", token).await;
    assert_eq!((&status.body["enabled"], &status.body["recovery_codes_left"]), (&json!(true), &json!(10)));

    // From now on it signs in as two-step sign-in does: a code each time.
    let asked = sign_in(&app, "bob", "test-pass-1", None).await;
    assert_eq!(asked.body["code"], "two_step", "{}", asked.text());
    assert!(asked.body.get("setup").is_none());
    let again = sign_in(&app, "bob", "test-pass-1", Some(&code_at(&secret, now()))).await;
    assert_eq!(again.body["code"], "two_step", "the code that set it up was used");
    let next = sign_in(&app, "bob", "test-pass-1", Some(&next_code(&secret))).await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.text());
    assert!(next.body.get("recovery_codes").is_none());
    assert_eq!(sign_in(&app, "bob", "test-pass-1", Some(&codes[0])).await.status, StatusCode::OK);
}

#[tokio::test]
async fn someone_else_with_the_password_never_gets_the_secret_being_set_up() {
    let (app, admin, _) = with_admin().await;
    app.user(&admin, "bob", json!({})).await;
    require(&app, &admin, true).await;
    let owners = sign_in(&app, "bob", "test-pass-1", None).await;
    let (secret, id) = (secret_of(&owners), id_of(&owners));

    // Without its id, another secret, which replaces it.
    let others = sign_in(&app, "bob", "test-pass-1", None).await;
    let theirs = secret_of(&others);
    assert!(theirs != secret && id_of(&others) != id);
    assert!(!others.mentions(owners.body["setup"]["secret"].as_str().unwrap()));
    // A right code without the id sets nothing up either.
    let no_id = sign_in(&app, "bob", "test-pass-1", Some(&code_at(&theirs, now()))).await;
    let third = secret_of(&no_id);
    assert!(third != theirs && third != secret);
    // The owner's stops working, and they are told so: never a secret both have.
    let stale = sign_in_with(&app, "bob", "test-pass-1", &id, Some(&code_at(&secret, now()))).await;
    let fresh = secret_of(&stale);
    assert!(fresh != secret && fresh != theirs && fresh != third);
    assert!(stale.text().contains("doesn't work any more"), "{}", stale.text());
    let done = sign_in_with(&app, "bob", "test-pass-1", &id_of(&stale), Some(&code_at(&fresh, now()))).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
}

#[tokio::test]
async fn too_many_wrong_codes_start_the_setup_over() {
    let (app, admin, _) = with_admin().await;
    app.user(&admin, "bob", json!({})).await;
    require(&app, &admin, true).await;
    let asked = sign_in(&app, "bob", "test-pass-1", None).await;
    let (first, id) = (secret_of(&asked), id_of(&asked));
    for _ in 0..4 {
        let wrong = sign_in_with(&app, "bob", "test-pass-1", &id, Some(&wrong_code(&first))).await;
        assert_eq!(secret_of(&wrong), first);
    }
    // Guessing at it gets a new secret, not a fifth try at the first.
    let fifth = sign_in_with(&app, "bob", "test-pass-1", &id, Some(&wrong_code(&first))).await;
    let second = secret_of(&fifth);
    assert_ne!(second, first);
    assert!(fifth.text().contains("doesn't work any more"), "{}", fifth.text());
    let id = id_of(&fifth);
    let old = sign_in_with(&app, "bob", "test-pass-1", &id, Some(&code_at(&first, now()))).await;
    assert_eq!(secret_of(&old), second, "the first secret's code sets nothing up");
    let done = sign_in_with(&app, "bob", "test-pass-1", &id, Some(&code_at(&second, now()))).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
}

#[tokio::test]
async fn a_disabled_account_sets_nothing_up() {
    let (app, admin, _) = with_admin().await;
    let (_, bob_id) = app.user(&admin, "bob", json!({})).await;
    require(&app, &admin, true).await;
    app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"disabled": true})).await;
    let refused = sign_in(&app, "bob", "test-pass-1", None).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert!(refused.body.get("setup").is_none());
}

#[tokio::test]
async fn invited_people_set_it_up_as_they_join() {
    let (app, admin, _) = with_admin().await;
    require(&app, &admin, true).await;
    let invite = app.post("/api/invites", &admin, json!({})).await;
    assert_eq!(invite.status, StatusCode::CREATED, "{}", invite.text());
    let code = invite.body["code"].as_str().unwrap();
    let body = json!({"username": "dave", "password": "dave-pass-1"});
    let joined = app.call(Method::POST, &format!("/api/invites/{code}/accept"), None, Some(body), &[]).await;
    let secret = secret_of(&joined);
    // The account is made: signing in to it, with the first code, finishes.
    let done = sign_in_with(&app, "dave", "dave-pass-1", &id_of(&joined), Some(&code_at(&secret, now()))).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
    assert_eq!(done.body["user"]["username"], "dave");
    assert_eq!(strings(&done.body["recovery_codes"]).len(), 10);
}

#[tokio::test]
async fn switching_to_a_profile_that_has_to_set_it_up_takes_its_password() {
    let (app, admin, admin_codes) = with_admin().await;
    let (_, mom_id) = app.user(&admin, "mom", json!({"pin": "1234"})).await;
    let kid = managed_kid(&app, &admin).await;
    require(&app, &admin, true).await;
    let switch = |body: Value| app.post("/api/auth/switch", &admin, body);

    // Her PIN can't set up how her account signs in, nor can an
    // administrator's own sign-in.
    let pin = switch(json!({"user_id": mom_id, "pin": "1234"})).await;
    assert_eq!(pin.status, StatusCode::FORBIDDEN, "{}", pin.text());
    assert!(pin.text().contains("takes its password"), "{}", pin.text());
    let wrong = switch(json!({"user_id": mom_id, "password": "nope"})).await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    assert!(wrong.text().contains("wrong password"), "{}", wrong.text());
    let asked = switch(json!({"user_id": mom_id, "password": "test-pass-1"})).await;
    let (secret, id) = (secret_of(&asked), id_of(&asked));
    let mistyped =
        switch(json!({"user_id": mom_id, "password": "test-pass-1", "code": wrong_code(&secret), "setup_id": id}))
            .await;
    assert_eq!(secret_of(&mistyped), secret);
    let body = json!({"user_id": mom_id, "password": "test-pass-1", "code": code_at(&secret, now()), "setup_id": id});
    let done = switch(body).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.text());
    assert_eq!(done.body["user"]["username"], "mom");
    assert_eq!(strings(&done.body["recovery_codes"]).len(), 10);
    let mom = done.body["token"].as_str().unwrap();
    assert_eq!(app.get("/api/auth/me", mom).await.body["two_step"], true);
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::UNAUTHORIZED, "the switch ended it");

    // Set up, her PIN switches to her again; a managed profile needs nothing.
    let admin =
        sign_in(&app, "admin", "secret-one", Some(&admin_codes[0])).await.body["token"].as_str().unwrap().to_owned();
    let to_mom = app.post("/api/auth/switch", &admin, json!({"user_id": mom_id, "pin": "1234"})).await;
    assert_eq!(to_mom.status, StatusCode::OK, "{}", to_mom.text());
    assert!(to_mom.body.get("recovery_codes").is_none());
    let mom = to_mom.body["token"].as_str().unwrap();
    let to_kid = app.post("/api/auth/switch", mom, json!({"user_id": kid})).await;
    assert_eq!(to_kid.status, StatusCode::OK, "{}", to_kid.text());
}

#[tokio::test]
async fn apps_pick_a_profile_that_has_to_set_it_up_in_the_browser() {
    let (app, admin, _) = with_admin().await;
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let (carol, carol_id) = app.user(&admin, "carol", json!({})).await;
    app.turn_on_two_step(&carol, "test-pass-1").await;
    let (_, mom_id) = app.user(&admin, "mom", json!({"pin": "1234"})).await;
    let kid = managed_kid(&app, &admin).await;
    require(&app, &admin, true).await;
    let (access, refresh_token) = app.app_token(carol_id, "library profiles").await;

    let needs =
        |list: &Value, id: i64| list.as_array().unwrap().iter().find(|p| p["id"] == id).unwrap()["needs"].clone();
    let listed = app.get("/api/oauth/profiles", &access).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    // Picked in the browser, as a profile that takes its password is; nothing
    // says how anyone signs in.
    assert_eq!(needs(&listed.body, mom_id), "password");
    assert_eq!(needs(&listed.body, admin_id), "password", "an administrator, as ever");
    assert_eq!(needs(&listed.body, carol_id), "none");
    assert_eq!(needs(&listed.body, kid), "none");
    assert!(!listed.mentions("two_step") && !listed.mentions("two-step"), "{}", listed.text());

    // Her PIN gets the app nowhere: the browser, where her password sets it up.
    let form = format!(
        "grant_type={}&client_id={}&refresh_token={}&profile={mom_id}&pin=1234",
        encode("urn:pley:params:oauth:grant-type:switch-profile"),
        encode(ANDROID_APP),
        encode(&refresh_token)
    );
    let refused = app.form_post("/api/oauth/token", &form, &[]).await;
    assert_eq!((&refused.body["error"], &refused.body["reason"]), (&json!("invalid_grant"), &json!("browser")));
    assert!(!refused.mentions("two-step"), "{}", refused.text());

    // Not required any more: her PIN, as before. The refusal spent nothing.
    require(&app, &admin, false).await;
    assert_eq!(needs(&app.get("/api/oauth/profiles", &access).await.body, mom_id), "pin");
    let switched = app.form_post("/api/oauth/token", &form, &[]).await;
    assert_eq!(switched.status, StatusCode::OK, "{}", switched.text());
}

#[tokio::test]
async fn a_tv_signs_in_at_link_once_its_person_has_set_it_up() {
    let (app, admin, _) = with_admin().await;
    app.user(&admin, "bob", json!({})).await;
    require(&app, &admin, true).await;

    let started = app.form_post("/api/oauth/device", &format!("client_id={}", encode(ROKU_APP)), &[]).await;
    assert_eq!(started.status, StatusCode::OK, "{}", started.text());
    let device_code = started.body["device_code"].as_str().unwrap().to_owned();
    let user_code = started.body["user_code"].as_str().unwrap().to_owned();

    // On the phone, /link signs bob in, which sets it up, then takes the code.
    let (cookie, _, _) = set_up_signing_in(&app, "bob", "test-pass-1").await;
    let shown = app
        .browser_call(Method::POST, "/api/oauth/device/verify", &cookie, Some(json!({"user_code": user_code})), &[])
        .await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    let id = shown.body["id"].as_str().unwrap();
    let allowed = app
        .browser_call(Method::POST, &format!("/api/oauth/requests/{id}"), &cookie, Some(json!({"allow": true})), &[])
        .await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.text());

    let form = format!(
        "grant_type={}&device_code={}&client_id={}",
        encode("urn:ietf:params:oauth:grant-type:device_code"),
        encode(&device_code),
        encode(ROKU_APP)
    );
    let tokens = app.form_post("/api/oauth/token", &form, &[]).await;
    assert_eq!(tokens.status, StatusCode::OK, "{}", tokens.text());
    let access = tokens.body["access_token"].as_str().unwrap();
    let me = app.get("/api/auth/me", access).await;
    assert_eq!((me.status, &me.body["username"]), (StatusCode::OK, &json!("bob")));
}

#[tokio::test]
async fn a_session_or_app_that_slips_past_the_sign_out_doesnt_work() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let (bob_app, bob_refresh) = app.app_token(bob_id, "library").await;
    // As if bob had signed in just as it became required.
    app.state.auth.require_two_step(true);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &bob_app).await.status, StatusCode::UNAUTHORIZED);
    let refused = refresh(&app, &bob_refresh).await;
    assert_eq!(refused.body["error"], "invalid_grant");
    assert!(refused.text().contains("two-step"), "{}", refused.text());
    // Nothing was spent or ended: not required, they work again.
    app.state.auth.require_two_step(false);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &bob_app).await.status, StatusCode::OK);
    assert_eq!(refresh(&app, &bob_refresh).await.status, StatusCode::OK);
}

#[tokio::test]
async fn compatibility_stream_addresses_stop_with_the_sign_out() {
    use pley::playback::{Method as Play, Session, TranscodeSpec};

    let (app, admin, _) = with_admin().await;
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let (_, bob_id) = app.user(&admin, "bob", json!({})).await;
    app.media_file("movies/Film (2020).mkv", 2048);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let film = app.ids(movies, &admin).await["Film"];
    let file = app.file_of(film).await;
    // Streams a compatibility client started, as the transcoder would have.
    let stream = |user: i64| {
        let spec = TranscodeSpec { request: Default::default(), segment_count: 1, duration_ms: 1000 };
        let session =
            Session::new(user, "u".into(), None, film, file, Play::Transcode, 1000, Some(spec), Some("job".into()), 0);
        app.state.sessions.insert(session).id.clone()
    };
    let playlist = |session: &str| {
        let req = axum::http::Request::builder()
            .uri(format!("/video/:/transcode/universal/session/{session}/base/index.m3u8"))
            .body(axum::body::Body::empty())
            .unwrap();
        let plex = app.plex.clone();
        async move { tower::ServiceExt::oneshot(plex, req).await.unwrap().status() }
    };
    let (bobs, admins) = (stream(bob_id), stream(admin_id));
    // No transcoder runs here: they get as far as asking it.
    assert_ne!(playlist(&bobs).await, StatusCode::NOT_FOUND);

    assert_eq!(require(&app, &admin, true).await.status, StatusCode::OK);
    // bob signed in with his password alone: his stream stops with the rest.
    assert_eq!(playlist(&bobs).await, StatusCode::NOT_FOUND);
    assert_ne!(playlist(&admins).await, StatusCode::NOT_FOUND, "whoever uses it goes on");
    // Not required any more: it goes on too.
    require(&app, &admin, false).await;
    assert_ne!(playlist(&bobs).await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn while_required_it_stays_on_and_moves_to_another_app() {
    let (app, admin, _) = with_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let (old, old_codes) = app.turn_on_two_step(&bob, "test-pass-1").await;
    require(&app, &admin, true).await;

    let off = app
        .call(
            Method::DELETE,
            "/api/me/two-step",
            Some(&bob),
            Some(json!({"password": "test-pass-1", "code": next_code(&old)})),
            &[],
        )
        .await;
    assert_eq!(off.status, StatusCode::FORBIDDEN, "{}", off.text());
    assert!(off.text().contains("stays on"), "{}", off.text());

    // Moving it takes the password and a code of the app it leaves (or a
    // recovery code), as turning it off does.
    let no_code = app.post("/api/me/two-step/setup", &bob, json!({"password": "test-pass-1"})).await;
    assert_eq!((no_code.status, &no_code.body["code"]), (StatusCode::UNAUTHORIZED, &json!("two_step")));
    let wrong_password =
        app.post("/api/me/two-step/setup", &bob, json!({"password": "nope", "code": old_codes[0]})).await;
    assert_eq!(wrong_password.status, StatusCode::FORBIDDEN);
    let setup =
        app.post("/api/me/two-step/setup", &bob, json!({"password": "test-pass-1", "code": old_codes[0]})).await;
    assert_eq!(setup.status, StatusCode::OK, "{}", setup.text());
    let new = base32_decode(setup.body["secret"].as_str().unwrap()).unwrap();
    assert_ne!(new, old);
    let wrong = app.post("/api/me/two-step", &bob, json!({"code": wrong_code(&new)})).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let since = app.get("/api/me/two-step", &bob).await.body["since"].clone();
    let moved = app.post("/api/me/two-step", &bob, json!({"code": code_at(&new, now())})).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    let fresh = strings(&moved.body["recovery_codes"]);
    assert_eq!(fresh.len(), 10);
    let status = app.get("/api/me/two-step", &bob).await.body;
    assert_eq!(status, json!({"enabled": true, "since": since, "recovery_codes_left": 10}), "on all along");

    // The old app's codes and recovery codes stop; the new ones sign in.
    let old_app = sign_in(&app, "bob", "test-pass-1", Some(&next_code(&old))).await;
    assert_eq!(old_app.body["code"], "two_step", "{}", old_app.text());
    assert_eq!(sign_in(&app, "bob", "test-pass-1", Some(&old_codes[1])).await.body["code"], "two_step");
    assert_eq!(sign_in(&app, "bob", "test-pass-1", Some(&next_code(&new))).await.status, StatusCode::OK);
    assert_eq!(sign_in(&app, "bob", "test-pass-1", Some(&fresh[0])).await.status, StatusCode::OK);
}

#[tokio::test]
async fn an_administrators_reset_signs_its_owner_out_to_set_it_up_again() {
    let (app, admin, _) = with_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    app.turn_on_two_step(&bob, "test-pass-1").await;
    require(&app, &admin, true).await;
    let (bob_app, _) = app.app_token(bob_id, "library").await;
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::OK);

    assert_eq!(app.delete(&format!("/api/users/{bob_id}/two-step"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get("/api/auth/me", &bob_app).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(signed_in(&app, bob_id).await, (0, 0));
    // With his new phone.
    let (token, _, _) = set_up_signing_in(&app, "bob", "test-pass-1").await;
    assert_eq!(app.get("/api/auth/me", &token).await.body["two_step"], true);
}

#[tokio::test]
async fn the_command_line_reset_leaves_it_to_be_set_up_at_the_next_sign_in() {
    let (app, admin, _) = with_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    app.turn_on_two_step(&bob, "test-pass-1").await;
    require(&app, &admin, true).await;
    pley::reset_password(app.state.config.clone(), "bob", "another-pass-2", false).await.unwrap();
    secret_of(&sign_in(&app, "bob", "another-pass-2", None).await);
}

#[tokio::test]
async fn it_stays_required_across_restarts() {
    let (mut app, admin, _) = with_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    require(&app, &admin, true).await;
    app.restart().await;
    assert_eq!(server_requires(&app).await, true);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    secret_of(&sign_in(&app, "bob", "test-pass-1", None).await);
}

/// A way in refused while the server can't use its key: `503` saying so,
/// and nothing signed in or set up.
fn refused_for_the_key(reply: &Reply) {
    assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE, "{}", reply.text());
    assert!(reply.mentions("fixes its secret.key"), "{}", reply.text());
    assert!(reply.headers.get(header::SET_COOKIE).is_none());
    assert!(reply.body.get("token").is_none() && reply.body.get("setup").is_none(), "{}", reply.text());
}

#[tokio::test]
async fn without_its_key_it_stays_required_and_recovery_codes_sign_in() {
    let mut app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (admin_secret, admin_codes) = app.turn_on_two_step(&admin, "secret-one").await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let invite = app.post("/api/invites", &admin, json!({})).await;
    let accept = format!("/api/invites/{}/accept", invite.body["code"].as_str().unwrap());
    let dave = json!({"username": "dave", "password": "dave-pass-1"});
    require(&app, &admin, true).await;
    // Pley can't read it any more (its account changed, say).
    std::fs::write(app.state.config.data_dir.join("secret.key"), "not a key").unwrap();
    app.restart().await;
    assert_eq!(server_requires(&app).await, true);

    // Nobody can set it up, and the password alone lets nobody in.
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    refused_for_the_key(&sign_in(&app, "bob", "test-pass-1", None).await);
    assert_eq!(sign_in(&app, "bob", "nope", None).await.status, StatusCode::UNAUTHORIZED);
    refused_for_the_key(&app.call(Method::POST, &accept, None, Some(dave.clone()), &[]).await);

    // An administrator's authenticator can't be checked: a recovery code
    // signs in, and the settings say why.
    let code = sign_in(&app, "admin", "secret-one", Some(&next_code(&admin_secret))).await;
    assert_eq!(code.status, StatusCode::SERVICE_UNAVAILABLE, "{}", code.text());
    assert!(code.mentions("recovery code"), "{}", code.text());
    let signed_in = sign_in(&app, "admin", "secret-one", Some(&admin_codes[0])).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    let admin = signed_in.body["token"].as_str().unwrap().to_owned();
    assert_eq!(app.get("/api/system", &admin).await.body["secret_key_ok"], false);
    // Switching to bob's profile is no way in either.
    let switch = app.post("/api/auth/switch", &admin, json!({"user_id": bob_id, "password": "test-pass-1"})).await;
    refused_for_the_key(&switch);
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK, "the refused switch ended nothing");

    // It can stop being required, and can't be required again until the key
    // is fixed.
    assert_eq!(require(&app, &admin, false).await.status, StatusCode::OK);
    assert_eq!(sign_in(&app, "bob", "test-pass-1", None).await.status, StatusCode::OK);
    let joined = app.call(Method::POST, &accept, None, Some(dave), &[]).await;
    assert_eq!(joined.status, StatusCode::OK, "no account was made, and the invitation stayed: {}", joined.text());
    let refused = require(&app, &admin, true).await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    assert!(refused.mentions("secret.key"), "{}", refused.text());
}
