//! Every way pley signs an account out also ends, at once, the apps acting
//! as it or allowed by it: a password set by an administrator, signing out
//! everywhere, deleting the account and `pley reset-password`. Disabling an
//! account pauses its apps, and enabling it brings them back. Codes not
//! exchanged yet end with the sign-ins too, and with the session that got
//! them.

mod common;

use axum::http::{Method, StatusCode, header};
use pley::auth::grants::{RefreshError, RefreshInput};
use serde_json::json;

use common::{ANDROID_APP, Reply, TestApp};

/// RFC 7636 appendix B.
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const ANDROID_BACK: &str = "app.pley.android:/oauth/callback";

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish()
}

/// pley's page asks the person signed in with `cookie` to allow pley's
/// Android app: the request's id.
async fn consent(app: &TestApp, cookie: &str) -> String {
    let query = form(&[
        ("response_type", "code"),
        ("client_id", ANDROID_APP),
        ("redirect_uri", ANDROID_BACK),
        ("scope", "library play"),
        ("code_challenge", CHALLENGE),
        ("code_challenge_method", "S256"),
    ]);
    let started =
        app.browser_call(Method::POST, "/api/oauth/requests", cookie, Some(json!({"query": query})), &[]).await;
    assert_eq!(started.body["status"], "consent", "{}", started.text());
    started.body["id"].as_str().unwrap().to_owned()
}

/// The person signed in with `cookie` allows pley's Android app on pley's
/// page: the code the app is sent back with.
async fn code_from(app: &TestApp, cookie: &str) -> String {
    let id = consent(app, cookie).await;
    let allow = Some(json!({"allow": true}));
    let decided = app.browser_call(Method::POST, &format!("/api/oauth/requests/{id}"), cookie, allow, &[]).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    let back = app.browser_call(Method::GET, &format!("/api/oauth/requests/{id}/continue"), cookie, None, &[]).await;
    let location = url::Url::parse(back.headers[header::LOCATION].to_str().unwrap()).unwrap();
    location.query_pairs().find(|(name, _)| name == "code").unwrap().1.into_owned()
}

/// The app exchanges `code` for its tokens.
async fn exchange(app: &TestApp, code: &str) -> Reply {
    let body = form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", ANDROID_APP),
        ("redirect_uri", ANDROID_BACK),
        ("code_verifier", VERIFIER),
    ]);
    app.form_post("/api/oauth/token", &body, &[]).await
}

async fn grants(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap()
}

/// `code` gets nothing, and no app is signed in.
async fn signs_nothing_in(app: &TestApp, code: &str) {
    let reply = exchange(app, code).await;
    let refusal = (reply.status, reply.body["error"].as_str());
    assert_eq!(refusal, (StatusCode::BAD_REQUEST, Some("invalid_grant")), "{}", reply.text());
    assert_eq!(grants(app).await, 0);
}

/// Whether the app's next request gets through.
async fn works(app: &TestApp, access: &str) -> bool {
    let me = app.app_call(Method::GET, "/api/auth/me", access, None).await;
    match me.status {
        StatusCode::OK => true,
        StatusCode::UNAUTHORIZED => false,
        other => panic!("unexpected {other}: {:?}", me.body),
    }
}

/// Tries the app's refresh token: whether it gave a new pair.
async fn refreshes(app: &TestApp, refresh: &str) -> bool {
    let input = RefreshInput {
        token: refresh.to_owned(),
        client_id: common::ANDROID_APP.into(),
        scope: None,
        user_agent: None,
        ip: None,
    };
    match app.state.auth.refresh(input).await {
        Ok(_) => true,
        Err(RefreshError::Invalid(_)) => false,
        Err(other) => panic!("expected an invalid grant, got {other:?}"),
    }
}

/// The app ended: its next request answers 401, its refresh is refused.
async fn assert_ended(app: &TestApp, (access, refresh): &(String, String)) {
    assert!(!works(app, access).await, "the access token still works");
    assert!(!refreshes(app, refresh).await, "the refresh token still works");
}

async fn last_grant(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT id FROM app_grants ORDER BY id DESC LIMIT 1")
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

#[tokio::test]
async fn an_administrator_setting_a_password_ends_the_accounts_apps() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_bob, bob) = app.user(&admin, "bob", json!({})).await;
    let tokens = app.app_token(bob, "library play").await;
    assert!(works(&app, &tokens.0).await);

    let set = app.patch(&format!("/api/users/{bob}"), &admin, json!({"password": "another-pass-2"})).await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.body);
    assert_ended(&app, &tokens).await;

    // An administrator's own password changes in Account, with the current
    // one, not on the users page; their session stays, their apps end.
    let me: i64 = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let mine = app.app_token(me, "library").await;
    let users_page = app.patch(&format!("/api/users/{me}"), &admin, json!({"password": "secret-two"})).await;
    assert_eq!(users_page.status, StatusCode::BAD_REQUEST, "{:?}", users_page.body);
    assert!(works(&app, &mine.0).await, "refused: nothing changed");
    let change = json!({"current_password": "secret-one", "new_password": "secret-two"});
    let set = app.put("/api/auth/password", &admin, change).await;
    assert_eq!(set.status, StatusCode::NO_CONTENT, "{:?}", set.body);
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK, "the session stays");
    assert_ended(&app, &mine).await;
}

#[tokio::test]
async fn signing_out_everywhere_ends_apps_too() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (session, bob) = app.user(&admin, "bob", json!({})).await;
    let tokens = app.app_token(bob, "library play").await;
    assert!(works(&app, &tokens.0).await);

    let out = app.delete(&format!("/api/users/{bob}/devices"), &admin).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT, "{:?}", out.body);
    assert_eq!(app.get("/api/auth/me", &session).await.status, StatusCode::UNAUTHORIZED, "the session ends");
    assert_ended(&app, &tokens).await;
}

#[tokio::test]
async fn codes_from_before_a_sign_out_sign_nothing_in_after_it() {
    // Someone with a copy of a session keeps a fresh code ready, to sign an
    // app in after the account's sign-ins end: nothing allowed before gets
    // in after.
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;

    // Bob changes his password: the copy's code ends with its session, and
    // his own codes end with his apps.
    let copy = app.login("bob", "test-pass-1").await;
    let copys = code_from(&app, &copy).await;
    let his = code_from(&app, &bob).await;
    let password = json!({"current_password": "test-pass-1", "new_password": "test-pass-2"});
    let changed = app.put("/api/auth/password", &bob, password).await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{}", changed.text());
    signs_nothing_in(&app, &copys).await;
    signs_nothing_in(&app, &his).await;

    // An administrator signs him out everywhere.
    let bob = app.login("bob", "test-pass-2").await;
    let code = code_from(&app, &bob).await;
    let out = app.delete(&format!("/api/users/{bob_id}/devices"), &admin).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT, "{}", out.text());
    signs_nothing_in(&app, &code).await;

    // An administrator gives him a new password.
    let bob = app.login("bob", "test-pass-2").await;
    let code = code_from(&app, &bob).await;
    let set = app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"password": "test-pass-3"})).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    signs_nothing_in(&app, &code).await;

    // An administrator changes their own password (in Account): their
    // session stays, but their codes end with their apps, and so do the
    // requests their consent page shows.
    let code = code_from(&app, &admin).await;
    let shown = consent(&app, &admin).await;
    let change = json!({"current_password": "secret-one", "new_password": "secret-two"});
    let set = app.put("/api/auth/password", &admin, change).await;
    assert_eq!(set.status, StatusCode::NO_CONTENT, "{}", set.text());
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK, "the session stays");
    signs_nothing_in(&app, &code).await;
    let allow = Some(json!({"allow": true}));
    let late = app.browser_call(Method::POST, &format!("/api/oauth/requests/{shown}"), &admin, allow, &[]).await;
    assert_eq!(late.status, StatusCode::NOT_FOUND, "{}", late.text());

    // One session signs out: its codes end, other sessions' work.
    let bob = app.login("bob", "test-pass-3").await;
    let other = app.login("bob", "test-pass-3").await;
    let others = code_from(&app, &other).await;
    let his = code_from(&app, &bob).await;
    let out = app.browser_call(Method::POST, "/api/auth/logout", &other, None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    signs_nothing_in(&app, &others).await;
    let signed_in = exchange(&app, &his).await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    assert_eq!(grants(&app).await, 1);
}

#[tokio::test]
async fn deleting_an_account_ends_its_apps_at_once() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_bob, bob) = app.user(&admin, "bob", json!({})).await;
    let tokens = app.app_token(bob, "library play").await;
    // Cached now: the deletion must drop it, not wait for it to lapse.
    assert!(works(&app, &tokens.0).await);

    assert_eq!(app.delete(&format!("/api/users/{bob}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_ended(&app, &tokens).await;
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(left, 0, "the grant went with the account");
}

#[tokio::test]
async fn reset_password_ends_the_accounts_apps() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob_session, bob) = app.user(&admin, "bob", json!({})).await;
    // Never used, so nothing is cached: as with the server closed.
    let tokens = app.app_token(bob, "library play").await;
    let tool = app.compatibility_token(&bob_session, "play").await;

    pley::reset_password(app.state.config.clone(), "bob", "another-pass-2", false).await.unwrap();
    assert_ended(&app, &tokens).await;
    assert_eq!(app.plex(Method::GET, "/library/sections", &tool, &[]).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn disabling_an_account_pauses_its_apps_and_enabling_brings_them_back() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_bob, bob) = app.user(&admin, "bob", json!({})).await;
    let (access, refresh) = app.app_token(bob, "library play").await;
    assert!(works(&app, &access).await);

    let off = app.patch(&format!("/api/users/{bob}"), &admin, json!({"disabled": true})).await;
    assert_eq!(off.status, StatusCode::OK, "{:?}", off.body);
    assert!(!works(&app, &access).await, "paused at once");
    assert!(!refreshes(&app, &refresh).await, "no refresh while disabled");

    let on = app.patch(&format!("/api/users/{bob}"), &admin, json!({"disabled": false})).await;
    assert_eq!(on.status, StatusCode::OK, "{:?}", on.body);
    assert!(works(&app, &access).await, "back, unchanged");
    assert!(refreshes(&app, &refresh).await, "the refused refresh spent nothing");
}

#[tokio::test]
async fn apps_allowed_by_someone_end_with_their_sign_ins() {
    // Ana allowed an app that acts as Bo (a household profile switch does
    // this); what happens to Ana's sign-ins happens to it.
    async fn allowed_by_ana(app: &TestApp, ana: i64, bo: i64) -> (String, String) {
        let tokens = app.app_token(ana, "library play").await;
        let grant = last_grant(app).await;
        sqlx::query("UPDATE app_grants SET user_id = ? WHERE id = ?")
            .bind(bo)
            .bind(grant)
            .execute(&app.state.db.write)
            .await
            .unwrap();
        // Nothing cached from before the move.
        app.state.auth.forget_grants(&[grant]);
        assert!(works(app, &tokens.0).await);
        let me = app.app_call(Method::GET, "/api/auth/me", &tokens.0, None).await;
        assert_eq!(me.body["id"].as_i64(), Some(bo), "it acts as Bo");
        tokens
    }

    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_ana, ana) = app.user(&admin, "ana", json!({})).await;
    let (_bo, bo) = app.user(&admin, "bo", json!({})).await;

    // Ana's password set by an administrator.
    let tokens = allowed_by_ana(&app, ana, bo).await;
    let set = app.patch(&format!("/api/users/{ana}"), &admin, json!({"password": "another-pass-2"})).await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.body);
    assert_ended(&app, &tokens).await;

    // Ana signed out everywhere.
    let tokens = allowed_by_ana(&app, ana, bo).await;
    let out = app.delete(&format!("/api/users/{ana}/devices"), &admin).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT, "{:?}", out.body);
    assert_ended(&app, &tokens).await;

    // Ana disabled: paused, and back when she is enabled again.
    let (access, refresh) = allowed_by_ana(&app, ana, bo).await;
    assert_eq!(app.patch(&format!("/api/users/{ana}"), &admin, json!({"disabled": true})).await.status, StatusCode::OK);
    assert!(!works(&app, &access).await);
    assert!(!refreshes(&app, &refresh).await);
    assert_eq!(
        app.patch(&format!("/api/users/{ana}"), &admin, json!({"disabled": false})).await.status,
        StatusCode::OK
    );
    assert!(works(&app, &access).await);

    // Ana's account deleted.
    assert_eq!(app.delete(&format!("/api/users/{ana}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_ended(&app, &(access, refresh)).await;
    assert_eq!(app.get("/api/auth/me", &app.login("bo", "test-pass-1").await).await.status, StatusCode::OK, "Bo stays");
}

#[tokio::test]
async fn a_signed_out_apps_number_is_never_given_again() {
    // A TV's keyed media addresses carry its grant's number: once the app is
    // signed out, the next app to sign in must not get that number back.
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let me: i64 = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let first = app.app_token(me, "library").await;
    let gone = last_grant(&app).await;
    assert_eq!(app.delete(&format!("/api/me/apps/{gone}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_ended(&app, &first).await;
    app.app_token(me, "library").await;
    assert!(last_grant(&app).await > gone, "the next app has a number of its own");
}
