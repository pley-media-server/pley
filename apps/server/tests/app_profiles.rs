//! pley's own apps switch the household profile they act as
//! (src/api/oauth.rs, `GET /api/oauth/profiles` and the switch-profile
//! grant; docs/API.md): by the web app's rules, without the
//! administrators' exemption, never with a password or a two-step code, and
//! keeping exactly what they were allowed.

mod common;

use std::collections::{HashMap, HashSet};

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};

use common::{ANDROID_APP, Reply, TestApp};
use pley::oauth::fetch::Source;

const SWITCH: &str = "urn:pley:params:oauth:grant-type:switch-profile";
const OTHER_APP: &str = "https://apps.example.com/pley.json";
const OTHER_BACK: &str = "https://apps.example.com/callback";
/// RFC 7636 appendix B.
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish()
}

/// The app showing `refresh` asks to act as `profile`, with `pin`.
async fn switch(app: &TestApp, refresh: &str, profile: i64, pin: Option<&str>) -> Reply {
    switch_as(app, ANDROID_APP, refresh, profile, pin).await
}

async fn switch_as(app: &TestApp, client_id: &str, refresh: &str, profile: i64, pin: Option<&str>) -> Reply {
    let profile = profile.to_string();
    let mut pairs =
        vec![("grant_type", SWITCH), ("client_id", client_id), ("refresh_token", refresh), ("profile", &profile)];
    if let Some(pin) = pin {
        pairs.push(("pin", pin));
    }
    app.form_post("/api/oauth/token", &form(&pairs), &[]).await
}

async fn refresh(app: &TestApp, token: &str) -> Reply {
    let body = form(&[("grant_type", "refresh_token"), ("refresh_token", token), ("client_id", ANDROID_APP)]);
    app.form_post("/api/oauth/token", &body, &[]).await
}

/// `(access token, refresh token)` of a token answer.
fn pair(reply: &Reply) -> (String, String) {
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(reply.body["token_type"], "Bearer");
    (reply.body["access_token"].as_str().unwrap().to_owned(), reply.body["refresh_token"].as_str().unwrap().to_owned())
}

/// `(status, error, reason)` of a refused token request.
fn refused(reply: &Reply) -> (StatusCode, String, String) {
    let text = |name: &str| reply.body[name].as_str().unwrap_or_default().to_owned();
    (reply.status, text("error"), text("reason"))
}

fn invalid_grant(reason: &str) -> (StatusCode, String, String) {
    (StatusCode::BAD_REQUEST, "invalid_grant".into(), reason.into())
}

/// What `GET /api/oauth/profiles` says switching to each profile needs, by
/// name.
async fn needs(app: &TestApp, token: &str) -> HashMap<String, String> {
    let list = app.get("/api/oauth/profiles", token).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    list.body
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap().to_owned(), p["needs"].as_str().unwrap().to_owned()))
        .collect()
}

/// Who an access token acts as: `(id, max_age, scope)`.
async fn who(app: &TestApp, access: &str) -> (i64, Value, String) {
    let me = app.get("/api/auth/me", access).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.text());
    assert_eq!(me.body["is_admin"], false);
    (me.body["id"].as_i64().unwrap(), me.body["max_age"].clone(), me.body["scope"].as_str().unwrap().to_owned())
}

async fn pin_failures(app: &TestApp, id: i64) -> i64 {
    sqlx::query_scalar("SELECT pin_failures FROM users WHERE id = ?")
        .bind(id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

/// Every app token on record: `(hash, kind, used_at)`.
async fn app_tokens(app: &TestApp) -> Vec<(String, String, Option<i64>)> {
    sqlx::query_as("SELECT token_hash, kind, used_at FROM app_tokens ORDER BY token_hash")
        .fetch_all(&app.state.db.read)
        .await
        .unwrap()
}

/// A household: an administrator (1), a kid (12, PIN 1234), a teen (16, no
/// PIN), both managed, and a parent with a password. A friend outside the
/// household and a disabled profile are never offered.
struct Household {
    app: TestApp,
    admin: String,
    admin_id: i64,
    kid: i64,
    teen: i64,
    parent: i64,
    friend: i64,
}

async fn household() -> Household {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let profile = |body: Value| {
        let (app, admin) = (&app, &admin);
        async move {
            let created = app.post("/api/users", admin, body).await;
            assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
            created.body["id"].as_i64().unwrap()
        }
    };
    let kid = profile(json!({"username": "kid", "managed": true, "max_age": 12})).await;
    assert_eq!(app.patch(&format!("/api/users/{kid}"), &admin, json!({"pin": "1234"})).await.status, StatusCode::OK);
    let teen = profile(json!({"username": "teen", "managed": true, "max_age": 16})).await;
    let away = profile(json!({"username": "away", "managed": true})).await;
    app.patch(&format!("/api/users/{away}"), &admin, json!({"disabled": true})).await;
    let (_, parent) = app.user(&admin, "parent", json!({})).await;
    let (_, friend) = app.user(&admin, "friend", json!({"home": false})).await;
    Household { app, admin, admin_id: 1, kid, teen, parent, friend }
}

// ------------------------------------------------------------------ the list

#[tokio::test]
async fn apps_list_the_household_with_what_a_switch_needs() {
    let h = household().await;
    let app = &h.app;
    let (admins, _) = app.app_token(h.admin_id, "library profiles").await;
    let list = app.get("/api/oauth/profiles", &admins).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.text());
    let profiles = list.body.as_array().unwrap();
    let ids: HashSet<i64> = profiles.iter().map(|p| p["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, HashSet::from([h.admin_id, h.kid, h.teen, h.parent]), "home members, enabled ones only");
    // Only what a switcher shows: no usernames, roles, libraries, age limits
    // or sign-in settings.
    let fields: HashSet<&str> = ["id", "name", "avatar_url", "avatar_color", "current", "needs"].into();
    for profile in profiles {
        let keys: HashSet<&str> = profile.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, fields, "{profile}");
    }
    for leak in ["username", "is_admin", "library_ids", "max_age", "has_password", "two_step", "has_pin"] {
        assert!(!list.mentions(leak), "{leak}: {}", list.text());
    }
    let current: Vec<i64> =
        profiles.iter().filter(|p| p["current"] == true).map(|p| p["id"].as_i64().unwrap()).collect();
    assert_eq!(current, [h.admin_id]);
    assert_eq!(
        needs(app, &admins).await,
        HashMap::from([
            ("admin".into(), "none".into()),
            ("kid".into(), "pin".into()),
            ("teen".into(), "none".into()),
            ("parent".into(), "none".into()),
        ])
    );

    // From the kid's profile: up needs a password (in the browser), or is
    // closed to a managed profile without a PIN.
    let (kids, _) = app.app_token(h.kid, "library profiles").await;
    assert_eq!(
        needs(app, &kids).await,
        HashMap::from([
            ("admin".into(), "password".into()),
            ("kid".into(), "none".into()),
            ("teen".into(), "locked".into()),
            ("parent".into(), "password".into()),
        ])
    );

    // Someone outside the household sees nobody.
    let (friends, _) = app.app_token(h.friend, "library profiles").await;
    let list = app.get("/api/oauth/profiles", &friends).await;
    assert_eq!((list.status, list.body.clone()), (StatusCode::OK, json!([])));
}

// ------------------------------------------------------------------ switching

#[tokio::test]
async fn apps_switch_to_profiles_that_need_nothing_or_their_pin() {
    let h = household().await;
    let app = &h.app;
    let (_, refresh_token) = app.app_token(h.admin_id, "library play profiles").await;

    // Down without a PIN: nothing to give.
    let to_teen = switch(app, &refresh_token, h.teen, None).await;
    let (teen_access, teen_refresh) = pair(&to_teen);
    assert_eq!(to_teen.body["scope"], "library play profiles");
    assert_eq!(to_teen.body["expires_in"], 3600);
    assert_eq!(who(app, &teen_access).await, (h.teen, json!(16), "library play profiles".into()));

    // A profile with a PIN takes it.
    let without = switch(app, &teen_refresh, h.kid, None).await;
    assert_eq!(refused(&without), invalid_grant("wrong_pin"));
    assert_eq!(without.body["error_description"], "this profile is protected by a PIN");
    let (kid_access, kid_refresh) = pair(&switch(app, &teen_refresh, h.kid, Some("1234")).await);
    // Only the kid's own rights.
    assert_eq!(who(app, &kid_access).await, (h.kid, json!(12), "library play profiles".into()));

    // Mistakes in the request.
    let to_self = switch(app, &kid_refresh, h.kid, None).await;
    assert_eq!((to_self.status, to_self.body["error"].clone()), (StatusCode::BAD_REQUEST, json!("invalid_request")));
    for body in [
        form(&[("grant_type", SWITCH), ("client_id", ANDROID_APP), ("refresh_token", &kid_refresh)]),
        form(&[("grant_type", SWITCH), ("client_id", ANDROID_APP), ("profile", "1")]),
        form(&[("grant_type", SWITCH), ("refresh_token", &kid_refresh), ("profile", "1")]),
        form(&[("grant_type", SWITCH), ("client_id", ANDROID_APP), ("refresh_token", &kid_refresh), ("profile", "me")]),
    ] {
        let reply = app.form_post("/api/oauth/token", &body, &[]).await;
        assert_eq!(reply.body["error"], "invalid_request", "{body}: {}", reply.text());
    }
    let nobody = switch(app, &kid_refresh, 999, None).await;
    assert_eq!(refused(&nobody), invalid_grant("not_allowed"));
    let friend = switch(app, &kid_refresh, h.friend, None).await;
    assert_eq!(refused(&friend), invalid_grant("not_allowed"));
    // None of that spent the token.
    pair(&refresh(app, &kid_refresh).await);
}

#[tokio::test]
async fn a_wrong_pin_changes_nothing_and_counts() {
    let h = household().await;
    let app = &h.app;
    let (access, refresh_token) = app.app_token(h.admin_id, "library profiles").await;
    let before = app_tokens(app).await;

    let wrong = switch(app, &refresh_token, h.kid, Some("9999")).await;
    assert_eq!(refused(&wrong), invalid_grant("wrong_pin"));
    assert_eq!(wrong.body["error_description"], "wrong PIN");
    assert_eq!(wrong.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(app_tokens(app).await, before, "nothing spent, nothing issued");
    assert_eq!(who(app, &access).await.0, h.admin_id, "still the administrator's profile");
    assert_eq!(pin_failures(app, h.kid).await, 1, "counted in a row, as on the web");

    // The web app's throttle: 5 wrong ones every 15 minutes, then even the
    // right one waits, for apps and the web alike.
    for _ in 0..4 {
        assert_eq!(refused(&switch(app, &refresh_token, h.kid, Some("0000")).await), invalid_grant("wrong_pin"));
    }
    let throttled = switch(app, &refresh_token, h.kid, Some("1234")).await;
    assert_eq!(refused(&throttled), invalid_grant("throttled"));
    assert!(throttled.body["error_description"].as_str().unwrap().starts_with("too many attempts"));
    let web = app.post("/api/auth/switch", &h.admin, json!({"user_id": h.kid, "pin": "1234"})).await;
    assert_eq!(web.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(pin_failures(app, h.kid).await, 5);
    assert_eq!(app_tokens(app).await, before);

    // 100 wrong ones in a row lock the PIN, the right one included.
    let little = app.post("/api/users", &h.admin, json!({"username": "little", "managed": true, "max_age": 7})).await;
    let little = little.body["id"].as_i64().unwrap();
    app.patch(&format!("/api/users/{little}"), &h.admin, json!({"pin": "2468"})).await;
    sqlx::query("UPDATE users SET pin_failures = 99 WHERE id = ?")
        .bind(little)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    assert_eq!(needs(app, &access).await["little"], "pin");
    assert_eq!(refused(&switch(app, &refresh_token, little, Some("1111")).await), invalid_grant("wrong_pin"));
    let locked = switch(app, &refresh_token, little, Some("2468")).await;
    assert_eq!(refused(&locked), invalid_grant("pin_locked"));
    assert!(locked.body["error_description"].as_str().unwrap().contains("locked"), "{}", locked.text());
    assert_eq!(needs(app, &access).await["little"], "locked");

    // The refresh token still works.
    assert_eq!(app_tokens(app).await, before);
    pair(&refresh(app, &refresh_token).await);
}

#[tokio::test]
async fn profiles_that_need_a_password_go_through_the_browser() {
    let h = household().await;
    let app = &h.app;
    let (_, refresh_token) = app.app_token(h.kid, "library profiles").await;
    let before = app_tokens(app).await;
    for profile in [h.parent, h.admin_id] {
        let up = switch(app, &refresh_token, profile, None).await;
        assert_eq!(refused(&up), invalid_grant("browser"), "{}", up.text());
        assert_eq!(
            up.body["error_description"],
            "this profile can see more than yours: sign in to the app again, and pick the profile in the browser"
        );
    }
    // Apps never hand over passwords or codes: a password changes nothing.
    let body = form(&[
        ("grant_type", SWITCH),
        ("client_id", ANDROID_APP),
        ("refresh_token", &refresh_token),
        ("profile", &h.parent.to_string()),
        ("password", "test-pass-1"),
    ]);
    let with_password = app.form_post("/api/oauth/token", &body, &[]).await;
    assert_eq!(refused(&with_password), invalid_grant("browser"));
    assert_eq!(app_tokens(app).await, before);
    pair(&refresh(app, &refresh_token).await);
}

#[tokio::test]
async fn locked_profiles_say_so() {
    let h = household().await;
    let app = &h.app;
    // From the kid's profile, the teen sees more and has neither a PIN nor
    // a password.
    let (kids, refresh_token) = app.app_token(h.kid, "library profiles").await;
    assert_eq!(needs(app, &kids).await["teen"], "locked");
    let locked = switch(app, &refresh_token, h.teen, Some("1234")).await;
    assert_eq!(refused(&locked), invalid_grant("not_allowed"));
    assert_eq!(
        locked.body["error_description"],
        "this profile can see more than yours: an administrator can give it a PIN to switch to it"
    );
    // A PIN opens it, as on the web.
    app.patch(&format!("/api/users/{}", h.teen), &h.admin, json!({"pin": "1357"})).await;
    assert_eq!(needs(app, &kids).await["teen"], "pin");
    let (teens, _) = pair(&switch(app, &refresh_token, h.teen, Some("1357")).await);
    assert_eq!(who(app, &teens).await.0, h.teen);
}

#[tokio::test]
async fn a_switch_keeps_the_scope_and_ends_old_access_tokens() {
    let h = household().await;
    let app = &h.app;
    let (old_access, refresh_token) = app.app_token(h.admin_id, "library play profiles").await;
    // Validated, so cached.
    assert_eq!(who(app, &old_access).await.0, h.admin_id);
    let grant: (i64, String, String, i64) = sqlx::query_as("SELECT id, client_id, scope, approved_by FROM app_grants")
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();

    let (access, _) = pair(&switch(app, &refresh_token, h.teen, None).await);
    // The old access token stops at once.
    let old = app.get("/api/auth/me", &old_access).await;
    assert_eq!(old.status, StatusCode::UNAUTHORIZED);
    assert_eq!(old.headers[header::WWW_AUTHENTICATE], r#"Bearer error="invalid_token""#);
    assert_eq!(app.get("/api/libraries", &old_access).await.status, StatusCode::UNAUTHORIZED);

    // Same grant, app, scope and approver; only the profile moved.
    let (id, client_id, scope, approved_by, user_id): (i64, String, String, i64, i64) =
        sqlx::query_as("SELECT id, client_id, scope, approved_by, user_id FROM app_grants")
            .fetch_one(&app.state.db.read)
            .await
            .unwrap();
    assert_eq!((id, client_id, scope, approved_by), grant);
    assert_eq!(user_id, h.teen);
    let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM app_tokens WHERE kind = 'access'")
        .fetch_all(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(kinds.len(), 1, "only the new access token");
    assert_eq!(who(app, &access).await, (h.teen, json!(16), "library play profiles".into()));

    // Whoever allowed it still sees it, using the teen's profile, and can
    // sign it out.
    let listed = app.get("/api/me/apps", &h.admin).await;
    assert_eq!(listed.body[0]["profile"]["id"], h.teen);
    assert_eq!(listed.body[0]["approved_by"]["id"], h.admin_id);
    assert_eq!(listed.body[0]["scopes"], json!(["library", "play", "profiles"]));
    assert_eq!(app.delete(&format!("/api/me/apps/{id}"), &h.admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_refresh_token_from_before_a_switch_ends_the_grant() {
    let h = household().await;
    let app = &h.app;
    let (access, refresh_token) = app.app_token(h.admin_id, "library profiles").await;
    // An access token is no refresh token.
    let wrong_kind = switch(app, &access, h.teen, None).await;
    assert_eq!(refused(&wrong_kind).1, "invalid_grant");
    let (new_access, new_refresh) = pair(&switch(app, &refresh_token, h.teen, None).await);

    // The token shown for the switch was spent: shown again, someone has a
    // copy, and the app's access ends.
    let again = switch(app, &refresh_token, h.parent, None).await;
    assert_eq!(refused(&again).1, "invalid_grant");
    assert_eq!(app.get("/api/auth/me", &new_access).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(refused(&refresh(app, &new_refresh).await).1, "invalid_grant");
    let grants: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(grants, 0);

    // A refresh after a switch, the same.
    let (_, refresh_token) = app.app_token(h.admin_id, "library profiles").await;
    let (_, after) = pair(&switch(app, &refresh_token, h.teen, None).await);
    assert_eq!(refused(&refresh(app, &refresh_token).await).1, "invalid_grant");
    assert_eq!(refused(&refresh(app, &after).await).1, "invalid_grant");
}

#[tokio::test]
async fn an_administrators_app_needs_pins_like_anyone() {
    let h = household().await;
    let app = &h.app;
    let (other_admin, other_admin_id) = app.user(&h.admin, "boss", json!({"is_admin": true})).await;
    assert_eq!(app.get("/api/auth/me", &other_admin).await.body["is_admin"], true);

    // On the web, an administrator goes to another administrator's profile
    // without anything.
    let session = app.login("admin", "secret-one").await;
    let web = app.post("/api/auth/switch", &session, json!({"user_id": other_admin_id})).await;
    assert_eq!(web.status, StatusCode::OK, "{}", web.text());

    // An administrator's app has no such exemption.
    let (access, refresh_token) = app.app_token(h.admin_id, "library profiles").await;
    let listed = needs(app, &access).await;
    assert_eq!((listed["boss"].as_str(), listed["kid"].as_str()), ("password", "pin"));
    assert_eq!(refused(&switch(app, &refresh_token, other_admin_id, None).await), invalid_grant("browser"));
    assert_eq!(refused(&switch(app, &refresh_token, h.kid, None).await), invalid_grant("wrong_pin"));
    // The kid's profile with its PIN: the kid's rights, no more.
    let (kids, _) = pair(&switch(app, &refresh_token, h.kid, Some("1234")).await);
    assert_eq!(who(app, &kids).await, (h.kid, json!(12), "library profiles".into()));
}

/// An app by another publisher whose description asks for every scope.
fn other_document() -> String {
    json!({
        "client_id": OTHER_APP,
        "client_name": "Example Player",
        "redirect_uris": [OTHER_BACK],
        "grant_types": ["authorization_code", "refresh_token"],
        "scope": "library play profiles requests approvals",
    })
    .to_string()
}

fn authorization_query(client_id: &str, redirect_uri: &str, scope: &str) -> String {
    form(&[
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", scope),
        ("state", "xyz"),
        ("code_challenge", CHALLENGE),
        ("code_challenge_method", "S256"),
    ])
}

#[tokio::test]
async fn apps_by_other_publishers_never_get_profiles() {
    let documents = HashMap::from([(OTHER_APP.to_owned(), other_document())]);
    let app = TestApp::with_config(|config| config.client_documents = Source::Fixed(documents)).await;
    let admin = app.setup_admin().await;
    assert_eq!(app.put("/api/settings", &admin, json!({"third_party_apps": true})).await.status, StatusCode::OK);
    let start = |query: String| {
        let (app, admin) = (&app, &admin);
        async move { app.browser_call(Method::POST, "/api/oauth/requests", admin, Some(json!({"query": query})), &[]).await }
    };

    // At authorization: invalid_scope, though its description lists it.
    let asked = start(authorization_query(OTHER_APP, OTHER_BACK, "library profiles")).await;
    assert_eq!(asked.status, StatusCode::OK, "{}", asked.text());
    assert_eq!((asked.body["status"].as_str(), asked.body["error"].as_str()), (Some("error"), Some("invalid_scope")));
    let fine = start(authorization_query(OTHER_APP, OTHER_BACK, "library play")).await;
    assert_eq!(fine.body["status"], "consent", "{}", fine.text());
    // Nor approvals (deciding what everyone asks for), though asking may be
    // allowed to any app.
    let approvals = start(authorization_query(OTHER_APP, OTHER_BACK, "library requests approvals")).await;
    assert_eq!(approvals.body["error"], "invalid_scope", "{}", approvals.text());
    let requests = start(authorization_query(OTHER_APP, OTHER_BACK, "library requests")).await;
    assert_eq!(requests.body["scopes"], json!(["library", "requests"]), "{}", requests.text());
    // pley's own apps may ask for it.
    let own = start(authorization_query(ANDROID_APP, "app.pley.android:/oauth/callback", "library profiles")).await;
    assert_eq!(own.body["status"], "consent", "{}", own.text());
    assert_eq!(own.body["scopes"], json!(["library", "profiles"]));

    // At the switch, even a grant that somehow holds it: unauthorized_client.
    let (_, kid) = app.user(&admin, "kid", json!({"max_age": 12})).await;
    let issued = app
        .state
        .auth
        .issue(pley::auth::grants::NewGrant {
            user_id: 1,
            approved_by: 1,
            client_id: OTHER_APP.into(),
            client_name: Some("Example Player".into()),
            scopes: pley::auth::scopes::Scopes::parse("library profiles").unwrap(),
            refresh: true,
            first_party: false,
            session_hash: None,
            user_agent: None,
            ip: None,
        })
        .await
        .unwrap();
    let refresh_token = issued.refresh_token.unwrap();
    let refused_switch = switch_as(&app, OTHER_APP, &refresh_token, kid, None).await;
    assert_eq!(
        (refused_switch.status, refused_switch.body["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("unauthorized_client"))
    );
    assert_eq!(who(&app, &issued.access_token).await.0, 1, "nothing changed");
}

#[tokio::test]
async fn the_switch_needs_the_profiles_scope() {
    let h = household().await;
    let app = &h.app;
    let (library, refresh_token) = app.app_token(h.admin_id, "library play").await;
    let list = app.get("/api/oauth/profiles", &library).await;
    assert_eq!(list.status, StatusCode::FORBIDDEN);
    assert_eq!(list.headers[header::WWW_AUTHENTICATE], r#"Bearer error="insufficient_scope", scope="profiles""#);
    assert_eq!(
        list.body["message"],
        "this app wasn't allowed to switch to the household's other profiles: sign in to it again to allow it"
    );
    let before = app_tokens(app).await;
    let refused_switch = switch(app, &refresh_token, h.teen, None).await;
    assert_eq!(
        (refused_switch.status, refused_switch.body["error"].as_str()),
        (StatusCode::BAD_REQUEST, Some("unauthorized_client"))
    );
    assert_eq!(app_tokens(app).await, before);
    assert_eq!(who(app, &library).await.0, h.admin_id);
    // The web app's own switch stays closed to apps, whatever they hold.
    let (every, _) = app.app_token(h.admin_id, "library play playlists profiles").await;
    let web = app.post("/api/auth/switch", &every, json!({"user_id": h.teen})).await;
    assert_eq!(web.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get("/api/auth/profiles", &every).await.status, StatusCode::FORBIDDEN);
}
