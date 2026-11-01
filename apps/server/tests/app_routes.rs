//! What pley's own apps do beyond playing and browsing: set the profile's
//! track defaults (`PUT /api/me/track-defaults`, src/api/accounts.rs), and
//! check the PIN of the profile they act as with the switch-profile grant
//! (src/api/oauth.rs), for a TV's "Who's watching?" as it starts.

mod common;

use axum::http::{StatusCode, header};
use serde_json::{Value, json};

use common::{ANDROID_APP, Reply, TestApp};

// ------------------------------------------------------------------ track defaults

const DEFAULTS: &str = "/api/me/track-defaults";

/// A profile's track defaults as stored: `(audio, subtitles, mode)`.
async fn stored(app: &TestApp, id: i64) -> (Option<String>, Option<String>, String) {
    sqlx::query_as("SELECT audio_language, subtitle_language, subtitle_mode FROM users WHERE id = ?")
        .bind(id)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap()
}

fn defaults(audio: Option<&str>, subtitles: Option<&str>, mode: &str) -> (Option<String>, Option<String>, String) {
    (audio.map(str::to_owned), subtitles.map(str::to_owned), mode.to_owned())
}

#[tokio::test]
async fn an_app_allowed_play_sets_the_profiles_track_defaults() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, ann) = app.user(&admin, "ann", json!({})).await;
    let (token, _) = app.app_token(ann, "library play").await;

    let set = app
        .put(DEFAULTS, &token, json!({"audio_language": "fra", "subtitle_language": "en", "subtitle_mode": "foreign"}))
        .await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    // The profile as GET /api/auth/me shows it to the app.
    assert_eq!(set.body, app.get("/api/auth/me", &token).await.body);
    assert_eq!(
        (&set.body["id"], &set.body["scope"], &set.body["audio_language"], &set.body["subtitle_mode"]),
        (&json!(ann), &json!("library play"), &json!("fra"), &json!("foreign"))
    );
    assert_eq!(stored(&app, ann).await, defaults(Some("fra"), Some("en"), "foreign"));

    // All three at once: a language left out is null, as the app sends a
    // null; a mode left out is the default one.
    let set = app.put(DEFAULTS, &token, json!({"subtitle_mode": "always"})).await;
    assert_eq!((set.status, &set.body["audio_language"]), (StatusCode::OK, &Value::Null));
    assert_eq!(stored(&app, ann).await, defaults(None, None, "always"));
    let set = app.put(DEFAULTS, &token, json!({"audio_language": "deu", "subtitle_language": null})).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    assert_eq!(stored(&app, ann).await, defaults(Some("deu"), None, "forced"));

    // PATCH /api/me's rules for languages; refused, nothing changes.
    for (body, message) in [
        (json!({"audio_language": "EN"}), "audio_language must be an ISO 639 code such as \"eng\""),
        (json!({"audio_language": "english"}), "audio_language must be an ISO 639 code such as \"eng\""),
        (json!({"subtitle_language": ""}), "subtitle_language must be an ISO 639 code such as \"eng\""),
        (json!({"subtitle_language": "e1"}), "subtitle_language must be an ISO 639 code such as \"eng\""),
        (json!({"subtitle_mode": "sometimes"}), "subtitle_mode is off, forced, foreign or always"),
        (json!({"subtitle_mode": "unknown"}), "subtitle_mode is off, forced, foreign or always"),
    ] {
        let refused = app.put(DEFAULTS, &token, body.clone()).await;
        assert_eq!((refused.status, &refused.body["message"]), (StatusCode::BAD_REQUEST, &json!(message)), "{body}");
    }
    assert_eq!(stored(&app, ann).await, defaults(Some("deu"), None, "forced"));
    // Only its own profile's.
    assert_eq!(stored(&app, 1).await, defaults(None, None, "forced"));
}

#[tokio::test]
async fn an_administrators_app_is_answered_as_an_app() {
    let app = TestApp::new().await;
    app.setup_admin().await;
    let (token, _) = app.app_token(1, "play").await;
    let set = app.put(DEFAULTS, &token, json!({"subtitle_mode": "off"})).await;
    assert_eq!(set.status, StatusCode::OK, "{}", set.text());
    assert_eq!((&set.body["is_admin"], &set.body["scope"]), (&json!(false), &json!("play")));
    // Still the administrator's account: what asking from one of pley's
    // own apps does.
    assert_eq!(set.body["request_access"], json!("auto"));
}

#[tokio::test]
async fn track_defaults_need_play() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, ann) = app.user(&admin, "ann", json!({})).await;
    for scope in ["library", "library playlists profiles"] {
        let (token, _) = app.app_token(ann, scope).await;
        let refused = app.put(DEFAULTS, &token, json!({"subtitle_mode": "always"})).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{scope}");
        assert_eq!(refused.headers[header::WWW_AUTHENTICATE], r#"Bearer error="insufficient_scope", scope="play""#);
        assert_eq!(refused.body["code"], "insufficient_scope");
        assert_eq!(
            refused.body["message"],
            "this app wasn't allowed to play titles, and keep the profile's place, watched marks, ratings, track \
             choices and library order: sign in to it again to allow it"
        );
    }
    assert_eq!(stored(&app, ann).await, defaults(None, None, "forced"));
}

#[tokio::test]
async fn the_web_keeps_patch_and_apps_dont_get_it() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (session, ann) = app.user(&admin, "ann", json!({})).await;

    // A session changes all of it with PATCH /api/me, as before.
    let patched = app
        .patch("/api/me", &session, json!({"display_name": "Ann", "audio_language": "spa", "subtitle_mode": "always"}))
        .await;
    assert_eq!(patched.status, StatusCode::OK, "{}", patched.text());
    assert_eq!((&patched.body["display_name"], &patched.body["audio_language"]), (&json!("Ann"), &json!("spa")));
    assert_eq!(stored(&app, ann).await, defaults(Some("spa"), None, "always"));
    // And may use the new route too: the profile, without a scope.
    let put = app.put(DEFAULTS, &session, json!({"audio_language": "ita"})).await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.text());
    assert!(put.body.get("scope").is_none(), "{}", put.text());
    assert_eq!(stored(&app, ann).await, defaults(Some("ita"), None, "forced"));

    // An app's token, whatever it was allowed, never changes the name or
    // picture: PATCH /api/me stays the web's.
    let (token, _) = app.app_token(ann, "library play playlists profiles").await;
    let refused = app.patch("/api/me", &token, json!({"display_name": "Mallory", "audio_language": "eng"})).await;
    assert_eq!((refused.status, &refused.body["code"]), (StatusCode::FORBIDDEN, &json!("insufficient_scope")));
    assert_eq!(refused.body["message"], "apps can't do that: use Pley's web app");
    let name: Option<String> = sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
        .bind(ann)
        .fetch_one(&app.state.db.read)
        .await
        .unwrap();
    assert_eq!(name.as_deref(), Some("Ann"));
    assert_eq!(stored(&app, ann).await, defaults(Some("ita"), None, "forced"));
}

// ------------------------------------------------------------------ the current profile's PIN

const SWITCH: &str = "urn:pley:params:oauth:grant-type:switch-profile";

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish()
}

/// The app showing `refresh` asks to act as `profile`, with `pin`.
async fn switch(app: &TestApp, refresh: &str, profile: i64, pin: Option<&str>) -> Reply {
    let profile = profile.to_string();
    let mut pairs =
        vec![("grant_type", SWITCH), ("client_id", ANDROID_APP), ("refresh_token", refresh), ("profile", &profile)];
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
    (reply.body["access_token"].as_str().unwrap().to_owned(), reply.body["refresh_token"].as_str().unwrap().to_owned())
}

/// `(status, error, reason)` of a refused token request; `reason` is empty
/// when there is none.
fn refused(reply: &Reply) -> (StatusCode, String, String) {
    let text = |name: &str| reply.body[name].as_str().unwrap_or_default().to_owned();
    (reply.status, text("error"), text("reason"))
}

fn invalid_grant(reason: &str) -> (StatusCode, String, String) {
    (StatusCode::BAD_REQUEST, "invalid_grant".into(), reason.into())
}

/// What older servers answered every switch to the current profile, which
/// apps read as "this server can't check the PIN".
fn same_profile() -> (StatusCode, String, String) {
    (StatusCode::BAD_REQUEST, "invalid_request".into(), String::new())
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

/// A household with a kid's profile (managed, 12, PIN 1234) and a teen's
/// (managed, 16, no PIN): `(app, administrator's session, kid, teen)`.
async fn household() -> (TestApp, String, i64, i64) {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let mut ids = Vec::new();
    for (name, age) in [("kid", 12), ("teen", 16)] {
        let created = app.post("/api/users", &admin, json!({"username": name, "managed": true, "max_age": age})).await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
        ids.push(created.body["id"].as_i64().unwrap());
    }
    let pin = app.patch(&format!("/api/users/{}", ids[0]), &admin, json!({"pin": "1234"})).await;
    assert_eq!(pin.status, StatusCode::OK, "{}", pin.text());
    (app, admin, ids[0], ids[1])
}

#[tokio::test]
async fn the_right_pin_for_the_current_profile_rotates_the_tokens() {
    let (app, _, kid, _) = household().await;
    let (old_access, old_refresh) = app.app_token(kid, "library play profiles").await;
    assert_eq!(app.get("/api/auth/me", &old_access).await.status, StatusCode::OK, "validated, so cached");
    sqlx::query("UPDATE users SET pin_failures = 3 WHERE id = ?").bind(kid).execute(&app.state.db.write).await.unwrap();

    let confirmed = switch(&app, &old_refresh, kid, Some("1234")).await;
    let (access, new_refresh) = pair(&confirmed);
    assert_eq!(
        (&confirmed.body["scope"], &confirmed.body["expires_in"]),
        (&json!("library play profiles"), &json!(3600))
    );
    assert_ne!(new_refresh, old_refresh);
    let me = app.get("/api/auth/me", &access).await;
    assert_eq!((&me.body["id"], &me.body["scope"]), (&json!(kid), &json!("library play profiles")));
    assert_eq!(pin_failures(&app, kid).await, 0, "a right one ends the run of wrong ones");
    // The tokens it had stop, as for a switch.
    assert_eq!(app.get("/api/auth/me", &old_access).await.status, StatusCode::UNAUTHORIZED);
    // Same grant, same profile.
    let grant: (i64, String) =
        sqlx::query_as("SELECT user_id, scope FROM app_grants").fetch_one(&app.state.db.read).await.unwrap();
    assert_eq!(grant, (kid, "library play profiles".to_owned()));

    // The refresh token shown was spent: shown again, someone has a copy, and
    // the app's access ends.
    let again = refresh(&app, &old_refresh).await;
    assert_eq!(refused(&again).1, "invalid_grant");
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(refused(&refresh(&app, &new_refresh).await).1, "invalid_grant");
}

#[tokio::test]
async fn the_new_refresh_token_goes_on_working() {
    let (app, _, kid, _) = household().await;
    let (_, refresh_token) = app.app_token(kid, "library profiles").await;
    let (_, after) = pair(&switch(&app, &refresh_token, kid, Some("1234")).await);
    let (access, _) = pair(&refresh(&app, &after).await);
    assert_eq!(app.get("/api/auth/me", &access).await.body["id"], json!(kid));
}

#[tokio::test]
async fn a_wrong_pin_for_the_current_profile_changes_nothing_and_counts() {
    let (app, admin, kid, _) = household().await;
    let (access, refresh_token) = app.app_token(kid, "library profiles").await;
    let before = app_tokens(&app).await;

    let wrong = switch(&app, &refresh_token, kid, Some("9999")).await;
    assert_eq!(refused(&wrong), invalid_grant("wrong_pin"));
    assert_eq!(wrong.body["error_description"], "wrong PIN");
    assert_eq!(app_tokens(&app).await, before, "nothing spent, nothing issued");
    assert_eq!(app.get("/api/auth/me", &access).await.status, StatusCode::OK);
    assert_eq!(pin_failures(&app, kid).await, 1, "counted in a row, as switches to it are");

    // 5 wrong ones in 15 minutes, then even the right one waits: the
    // profile's throttle, shared with switches to it on the web and in apps.
    for _ in 0..4 {
        assert_eq!(refused(&switch(&app, &refresh_token, kid, Some("0000")).await), invalid_grant("wrong_pin"));
    }
    let throttled = switch(&app, &refresh_token, kid, Some("1234")).await;
    assert_eq!(refused(&throttled), invalid_grant("throttled"));
    assert!(throttled.body["error_description"].as_str().unwrap().starts_with("too many attempts"));
    let web = app.post("/api/auth/switch", &admin, json!({"user_id": kid, "pin": "1234"})).await;
    assert_eq!(web.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(pin_failures(&app, kid).await, 5);
    assert_eq!(app_tokens(&app).await, before);
    pair(&refresh(&app, &refresh_token).await);
}

#[tokio::test]
async fn a_locked_pin_stays_locked_for_the_current_profile() {
    let (app, _, kid, _) = household().await;
    let (_, refresh_token) = app.app_token(kid, "library profiles").await;
    sqlx::query("UPDATE users SET pin_failures = 99 WHERE id = ?")
        .bind(kid)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    assert_eq!(refused(&switch(&app, &refresh_token, kid, Some("1111")).await), invalid_grant("wrong_pin"));
    // The hundredth wrong one in a row locks it, the right one included.
    let locked = switch(&app, &refresh_token, kid, Some("1234")).await;
    assert_eq!(refused(&locked), invalid_grant("pin_locked"));
    assert!(locked.body["error_description"].as_str().unwrap().contains("locked"), "{}", locked.text());
    pair(&refresh(&app, &refresh_token).await);
}

#[tokio::test]
async fn without_a_pin_to_check_the_current_profile_is_invalid_request() {
    let (app, _, kid, teen) = household().await;
    let (_, kids) = app.app_token(kid, "library profiles").await;
    let (_, teens) = app.app_token(teen, "library profiles").await;
    let before = app_tokens(&app).await;
    for (refresh_token, profile, pin) in [
        // No PIN given, for a profile with one.
        (&kids, kid, None),
        (&kids, kid, Some("")),
        // A PIN given, for a profile without one.
        (&teens, teen, Some("1234")),
        (&teens, teen, None),
    ] {
        let reply = switch(&app, refresh_token, profile, pin).await;
        assert_eq!(refused(&reply), same_profile(), "{profile} {pin:?}: {}", reply.text());
        assert_eq!(reply.body["error_description"], "the app uses this profile already");
    }
    assert_eq!(app_tokens(&app).await, before, "nothing spent");
    assert_eq!(pin_failures(&app, kid).await, 0);

    // The scope comes first: the check is for apps allowed to switch.
    let (_, library_only) = app.app_token(kid, "library").await;
    let reply = switch(&app, &library_only, kid, Some("1234")).await;
    assert_eq!(refused(&reply).1, "unauthorized_client");
}
