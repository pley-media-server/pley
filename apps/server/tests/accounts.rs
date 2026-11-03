//! Accounts end to end (docs/API.md: Auth, Your account, Users,
//! Invitations): who may manage users, the rules that keep the server
//! usable, PINs and profile switching, invitations, devices and login
//! throttling.

mod common;

use std::time::{Duration, Instant};

use axum::http::{Method, StatusCode, header};
use serde_json::{Value, json};

use common::{Reply, TestApp};

async fn user_named(app: &TestApp, admin: &str, username: &str) -> Value {
    let users = app.get("/api/users", admin).await;
    assert_eq!(users.status, StatusCode::OK);
    users.body.as_array().unwrap().iter().find(|u| u["username"] == username).cloned().expect(username)
}

async fn switch(app: &TestApp, token: &str, body: Value) -> Reply {
    app.post("/api/auth/switch", token, body).await
}

async fn accept(app: &TestApp, code: &str, body: Value) -> Reply {
    app.call(Method::POST, &format!("/api/invites/{code}/accept"), None, Some(body), &[]).await
}

async fn uses_left(app: &TestApp, admin: &str, code: &str) -> Option<Value> {
    let list = app.get("/api/invites", admin).await;
    list.body.as_array().unwrap().iter().find(|i| i["code"] == code).map(|i| i["uses_left"].clone())
}

async fn change_password(app: &TestApp, token: &str, current: &str, new: &str) -> Reply {
    app.put("/api/auth/password", token, json!({"current_password": current, "new_password": new})).await
}

#[tokio::test]
async fn pley_always_keeps_an_enabled_administrator() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();

    let demote = app.patch(&format!("/api/users/{admin_id}"), &admin, json!({"is_admin": false})).await;
    assert_eq!(demote.status, StatusCode::BAD_REQUEST, "the only administrator");
    let disable = app.patch(&format!("/api/users/{admin_id}"), &admin, json!({"disabled": true})).await;
    assert_eq!(disable.status, StatusCode::BAD_REQUEST, "nobody disables their own account");
    // One's own PIN changes in Account, with the password: a session alone
    // can't set one, which would switch profiles to it without two-step.
    for pin in [json!("1234"), json!(null)] {
        let own_pin = app.patch(&format!("/api/users/{admin_id}"), &admin, json!({ "pin": pin })).await;
        assert_eq!(own_pin.status, StatusCode::BAD_REQUEST, "{:?}", own_pin.body);
    }
    assert_eq!(app.get("/api/auth/me", &admin).await.body["has_pin"], json!(false));

    // A second administrator who is disabled doesn't count.
    let (_, second_id) = app.user(&admin, "second", json!({"is_admin": true})).await;
    let disabled = app.patch(&format!("/api/users/{second_id}"), &admin, json!({"disabled": true})).await;
    assert_eq!(disabled.status, StatusCode::OK, "{:?}", disabled.body);
    let demote = app.patch(&format!("/api/users/{admin_id}"), &admin, json!({"is_admin": false})).await;
    assert_eq!(demote.status, StatusCode::BAD_REQUEST);

    // Once it is enabled again, the first one may step down, right away.
    app.patch(&format!("/api/users/{second_id}"), &admin, json!({"disabled": false})).await;
    let second = app.login("second", "test-pass-1").await;
    let demote = app.patch(&format!("/api/users/{admin_id}"), &admin, json!({"is_admin": false})).await;
    assert_eq!(demote.status, StatusCode::OK, "{:?}", demote.body);
    assert_eq!(app.get("/api/users", &admin).await.status, StatusCode::FORBIDDEN);
    let me_now = app.patch(&format!("/api/users/{second_id}"), &second, json!({"is_admin": false})).await;
    assert_eq!(me_now.status, StatusCode::BAD_REQUEST, "now the only one");

    // Managed profiles can't be administrators.
    let kid = app.post("/api/users", &second, json!({"username": "kid", "managed": true})).await;
    let kid_id = kid.body["id"].as_i64().unwrap();
    let promote = app.patch(&format!("/api/users/{kid_id}"), &second, json!({"is_admin": true})).await;
    assert_eq!(promote.status, StatusCode::BAD_REQUEST);
    let both = app.post("/api/users", &second, json!({"username": "kid2", "managed": true, "is_admin": true})).await;
    assert_eq!(both.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_rejected_user_update_changes_nothing() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;

    let bad_age =
        app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"display_name": "Robert", "max_age": 99})).await;
    assert_eq!(bad_age.status, StatusCode::BAD_REQUEST);
    assert_eq!(user_named(&app, &admin, "bob").await["display_name"], Value::Null);

    let bad_pin =
        app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"password": "brand-new", "pin": "12"})).await;
    assert_eq!(bad_pin.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::OK, "bob is still signed in");
    assert_eq!(app.try_login("bob", "test-pass-1").await.status, StatusCode::OK, "with his old password");

    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let last =
        app.patch(&format!("/api/users/{admin_id}"), &admin, json!({"display_name": "Boss", "is_admin": false})).await;
    assert_eq!(last.status, StatusCode::BAD_REQUEST);
    assert_eq!(user_named(&app, &admin, "admin").await["display_name"], Value::Null);
}

#[tokio::test]
async fn a_new_password_from_an_administrator_signs_the_user_out() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (laptop, bob_id) = app.user(&admin, "bob", json!({})).await;
    let phone = app.login("bob", "test-pass-1").await;

    let reset = app.patch(&format!("/api/users/{bob_id}"), &admin, json!({"password": "fresh-start"})).await;
    assert_eq!(reset.status, StatusCode::OK, "{:?}", reset.body);
    for token in [&laptop, &phone] {
        assert_eq!(app.get("/api/auth/me", token).await.status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(app.try_login("bob", "test-pass-1").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.try_login("bob", "fresh-start").await.status, StatusCode::OK);

    // A managed profile given a password becomes a regular account.
    let kid = app.post("/api/users", &admin, json!({"username": "kid", "managed": true})).await;
    let kid_id = kid.body["id"].as_i64().unwrap();
    assert_eq!(app.try_login("kid", "!managed").await.status, StatusCode::UNAUTHORIZED);
    let grown = app.patch(&format!("/api/users/{kid_id}"), &admin, json!({"password": "grown-up"})).await;
    assert_eq!(grown.body["managed"], json!(false));
    assert_eq!(app.try_login("kid", "grown-up").await.status, StatusCode::OK);

    // Administrators resetting their own password stay signed in.
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    app.patch(&format!("/api/users/{admin_id}"), &admin, json!({"password": "secret-two"})).await;
    assert_eq!(app.get("/api/auth/me", &admin).await.status, StatusCode::OK);
}

#[tokio::test]
async fn new_users_are_checked() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2001).mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;

    let create = |body: Value| app.post("/api/users", &admin, body);
    assert_eq!(create(json!({"username": "ADMIN", "password": "test-pass-1"})).await.status, StatusCode::CONFLICT);
    for body in [
        json!({"username": "bad name", "password": "test-pass-1"}),
        json!({"username": "", "password": "test-pass-1"}),
        json!({"username": "nopass"}),
        json!({"username": "short", "password": "abc"}),
        json!({"username": "old", "password": "test-pass-1", "max_age": 22}),
        json!({"username": "young", "password": "test-pass-1", "max_age": -1}),
        json!({"username": "pin", "password": "test-pass-1", "pin": "12"}),
    ] {
        let reply = create(body.clone()).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{body}: {:?}", reply.body);
    }

    let created = create(json!({
        "username": "  carol  ",
        "password": "test-pass-1",
        "display_name": "   ",
        "all_libraries": false,
        "library_ids": [movies, 999],
        "max_age": 0,
    }))
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["username"], json!("carol"));
    assert_eq!((created.body["name"].clone(), created.body["display_name"].clone()), (json!("carol"), Value::Null));
    assert_eq!(created.body["library_ids"], json!([movies]), "unknown libraries are dropped");
    assert_eq!((created.body["max_age"].clone(), created.body["home"].clone()), (json!(0), json!(true)));

    // None of this is open to other users.
    let carol = app.login("carol", "test-pass-1").await;
    let carol_id = created.body["id"].as_i64().unwrap();
    assert_eq!(app.get("/api/users", &carol).await.status, StatusCode::FORBIDDEN);
    assert_eq!(create_as(&app, &carol).await, StatusCode::FORBIDDEN);
    let selfish = app.patch(&format!("/api/users/{carol_id}"), &carol, json!({"max_age": null})).await;
    assert_eq!(selfish.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/users/{carol_id}"), &carol).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get(&format!("/api/users/{carol_id}/devices"), &carol).await.status, StatusCode::FORBIDDEN);
}

async fn create_as(app: &TestApp, token: &str) -> StatusCode {
    app.post("/api/users", token, json!({"username": "eve", "password": "test-pass-1", "is_admin": true})).await.status
}

#[tokio::test]
async fn deleting_a_user_signs_them_out_and_removes_their_picture() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let mut png = Vec::new();
    image::RgbImage::from_pixel(32, 32, image::Rgb([10, 20, 30]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let req = axum::http::Request::builder()
        .method(Method::PUT)
        .uri("/api/me/avatar")
        .header(header::AUTHORIZATION, format!("Bearer {bob}"))
        .body(axum::body::Body::from(png))
        .unwrap();
    let uploaded = tower::ServiceExt::oneshot(app.api.clone(), req).await.unwrap();
    assert_eq!(uploaded.status(), StatusCode::OK);
    let avatars = app.state.config.data_dir.join("avatars");
    assert_eq!(std::fs::read_dir(&avatars).unwrap().count(), 1);

    assert_eq!(app.delete(&format!("/api/users/{admin_id}"), &admin).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.delete("/api/users/999", &admin).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.delete(&format!("/api/users/{bob_id}"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.try_login("bob", "test-pass-1").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(std::fs::read_dir(&avatars).unwrap().count(), 0, "the picture is gone too");
    assert_eq!(app.get(&format!("/api/users/{bob_id}/avatar"), &admin).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn your_own_pin_needs_your_password() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (parent, parent_id) = app.user(&admin, "parent", json!({})).await;

    let set_pin = |body: Value| app.put("/api/me/pin", &parent, body);
    assert_eq!(set_pin(json!({"pin": "4321"})).await.status, StatusCode::FORBIDDEN, "no password");
    assert_eq!(set_pin(json!({"pin": "4321", "password": "nope"})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(set_pin(json!({"pin": "12", "password": "test-pass-1"})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(set_pin(json!({"pin": "4321", "password": "test-pass-1"})).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &parent).await.body["has_pin"], json!(true));

    // Switching to the profile now takes the PIN, even for an administrator.
    let without = app.post("/api/auth/switch", &admin, json!({"user_id": parent_id})).await;
    assert_eq!(without.status, StatusCode::FORBIDDEN);
    let with = app.post("/api/auth/switch", &admin, json!({"user_id": parent_id, "pin": "4321"})).await;
    assert_eq!(with.status, StatusCode::OK, "{:?}", with.body);

    assert_eq!(set_pin(json!({"pin": null, "password": "test-pass-1"})).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &parent).await.body["has_pin"], json!(false));

    // Managed profiles have no password: an administrator sets their PIN.
    let kid = app
        .post("/api/users", &app.login("admin", "secret-one").await, json!({"username": "kid", "managed": true}))
        .await;
    let kid_id = kid.body["id"].as_i64().unwrap();
    let kid = app.post("/api/auth/switch", &parent, json!({"user_id": kid_id})).await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let refused = app.put("/api/me/pin", &kid, json!({"pin": "1111", "password": ""})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_pin_stops_working_after_100_wrong_tries_in_a_row() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (_, kid_id) = app.user(&admin, "kid", json!({"pin": "1234", "max_age": 12})).await;
    let pin_failures = || async {
        sqlx::query_scalar::<_, i64>("SELECT pin_failures FROM users WHERE id = ?")
            .bind(kid_id)
            .fetch_one(&app.state.db.read)
            .await
            .unwrap()
    };
    let wrong = switch(&app, &admin, json!({"user_id": kid_id, "pin": "9999"})).await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    assert_eq!(pin_failures().await, 1, "kept across restarts, unlike the throttle");
    let admin = app.login("admin", "secret-one").await;
    let right = switch(&app, &admin, json!({"user_id": kid_id, "pin": "1234"})).await;
    assert_eq!(right.status, StatusCode::OK, "{}", right.text());
    assert_eq!(pin_failures().await, 0, "the right PIN starts the count over");

    // Guessing over weeks, 5 tries every 15 minutes: the 100th wrong PIN in
    // a row locks it, the right one included.
    sqlx::query("UPDATE users SET pin_failures = 99 WHERE id = ?")
        .bind(kid_id)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let admin = app.login("admin", "secret-one").await;
    assert_eq!(switch(&app, &admin, json!({"user_id": kid_id, "pin": "9999"})).await.status, StatusCode::FORBIDDEN);
    let locked = switch(&app, &admin, json!({"user_id": kid_id, "pin": "1234"})).await;
    assert_eq!((locked.status, &locked.body["code"]), (StatusCode::FORBIDDEN, &json!("pin_locked")));
    assert!(locked.text().contains("locked"), "{}", locked.text());

    // Setting it again opens it.
    assert_eq!(app.patch(&format!("/api/users/{kid_id}"), &admin, json!({"pin": "4321"})).await.status, StatusCode::OK);
    assert_eq!(switch(&app, &admin, json!({"user_id": kid_id, "pin": "4321"})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn who_can_switch_to_whom() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let (friend, friend_id) = app.user(&admin, "friend", json!({"home": false})).await;
    let kid =
        app.post("/api/users", &admin, json!({"username": "kid", "managed": true})).await.body["id"].as_i64().unwrap();
    let teen =
        app.post("/api/users", &admin, json!({"username": "teen", "managed": true})).await.body["id"].as_i64().unwrap();
    let away =
        app.post("/api/users", &admin, json!({"username": "away", "managed": true})).await.body["id"].as_i64().unwrap();
    app.patch(&format!("/api/users/{away}"), &admin, json!({"disabled": true})).await;

    // Friends are not part of the household, disabled profiles are not offered.
    assert_eq!(app.get("/api/auth/profiles", &friend).await.body, json!([]));
    let profiles = app.get("/api/auth/profiles", &admin).await;
    let names: Vec<&str> = profiles.body.as_array().unwrap().iter().map(|u| u["username"].as_str().unwrap()).collect();
    assert_eq!(names, ["admin", "kid", "teen"]);
    assert_eq!(switch(&app, &admin, json!({"user_id": friend_id})).await.status, StatusCode::FORBIDDEN);
    let to_admin = switch(&app, &friend, json!({"user_id": admin_id, "password": "secret-one"})).await;
    assert_eq!(to_admin.status, StatusCode::FORBIDDEN, "friends can't switch at all");
    assert_eq!(switch(&app, &admin, json!({"user_id": away})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(switch(&app, &admin, json!({"user_id": 999})).await.status, StatusCode::NOT_FOUND);

    // Profiles without a PIN are open between household members.
    let kid_token = switch(&app, &admin, json!({"user_id": kid})).await.body["token"].as_str().unwrap().to_owned();
    let teen_token = switch(&app, &kid_token, json!({"user_id": teen})).await;
    assert_eq!(teen_token.status, StatusCode::OK);
    let teen_token = teen_token.body["token"].as_str().unwrap().to_owned();
    assert_eq!(app.get("/api/auth/me", &kid_token).await.status, StatusCode::UNAUTHORIZED, "one profile per device");
    // Switching to yourself just renews the session.
    let again = switch(&app, &teen_token, json!({"user_id": teen})).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(app.get("/api/auth/me", &teen_token).await.status, StatusCode::UNAUTHORIZED);
    let teen_token = again.body["token"].as_str().unwrap().to_owned();

    // An administrator with a PIN asks for the PIN, not the password.
    let admin = app.login("admin", "secret-one").await;
    let pin = app.put("/api/me/pin", &admin, json!({"pin": "2468", "password": "secret-one"})).await;
    assert_eq!(pin.status, StatusCode::NO_CONTENT, "{:?}", pin.body);
    let by_password = switch(&app, &teen_token, json!({"user_id": admin_id, "password": "secret-one"})).await;
    assert_eq!(by_password.status, StatusCode::FORBIDDEN);
    let by_pin = switch(&app, &teen_token, json!({"user_id": admin_id, "pin": "2468"})).await;
    assert_eq!(by_pin.status, StatusCode::OK, "{:?}", by_pin.body);
    assert_eq!(by_pin.body["user"]["is_admin"], json!(true));
}

#[tokio::test]
async fn invitations() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.media_file("movies/Film (2001).mkv", 1024);
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let invite = |body: Value| app.post("/api/invites", &admin, body);

    assert_eq!(invite(json!({"max_age": 30})).await.status, StatusCode::BAD_REQUEST);
    let created = invite(json!({
        "uses": 2,
        "expires_in_days": 0,
        "all_libraries": false,
        "library_ids": [movies],
        "max_age": 12,
        "note": format!("  {}  ", "n".repeat(300)),
    }))
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.body);
    assert_eq!(created.body["expires_at"], Value::Null, "0 days: never expires");
    assert_eq!(created.body["note"].as_str().unwrap().len(), 200);
    let code = created.body["code"].as_str().unwrap().to_owned();
    let check = app.call(Method::GET, &format!("/api/invites/{code}"), None, None, &[]).await;
    assert_eq!(check.status, StatusCode::OK);

    // Refused sign-ups don't use the invitation up.
    let bad = accept(&app, &code, json!({"username": "bad name", "password": "test-pass-1"})).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let taken = accept(&app, &code, json!({"username": "Admin", "password": "test-pass-1"})).await;
    assert_eq!(taken.status, StatusCode::CONFLICT);
    assert_eq!(uses_left(&app, &admin, &code).await, Some(json!(2)));

    let first =
        accept(&app, &code, json!({"username": "ann", "password": "test-pass-1", "display_name": " Ann "})).await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.body);
    let user = &first.body["user"];
    assert_eq!(
        (user["name"].clone(), user["home"].clone(), user["max_age"].clone()),
        (json!("Ann"), json!(false), json!(12))
    );
    assert_eq!(user["library_ids"], json!([movies]));
    assert_eq!(user["request_access"], json!("off"), "an age limit: no asking until an administrator lets it");
    assert!(first.headers[header::SET_COOKIE].to_str().unwrap().starts_with("pley_token="));
    let second = accept(&app, &code, json!({"username": "ben", "password": "test-pass-1"})).await;
    assert_eq!(second.status, StatusCode::OK);
    let third = accept(&app, &code, json!({"username": "cid", "password": "test-pass-1"})).await;
    assert_eq!(third.status, StatusCode::NOT_FOUND);
    let check = app.call(Method::GET, &format!("/api/invites/{code}"), None, None, &[]).await;
    assert_eq!(check.status, StatusCode::NOT_FOUND, "used up");
    let open = invite(json!({})).await.body["code"].as_str().unwrap().to_owned();
    let joined = accept(&app, &open, json!({"username": "fay", "password": "test-pass-1"})).await;
    assert_eq!(joined.body["user"]["request_access"], json!("ask"), "{:?}", joined.body);

    // Expired and deleted invitations.
    let week = invite(json!({})).await.body["code"].as_str().unwrap().to_owned();
    sqlx::query("UPDATE invites SET expires_at = 1 WHERE code = ?")
        .bind(&week)
        .execute(&app.state.db.write)
        .await
        .unwrap();
    let late = accept(&app, &week, json!({"username": "dan", "password": "test-pass-1"})).await;
    assert_eq!(late.status, StatusCode::NOT_FOUND);
    let gone = invite(json!({})).await.body["code"].as_str().unwrap().to_owned();
    assert_eq!(app.delete(&format!("/api/invites/{gone}"), &admin).await.status, StatusCode::NO_CONTENT);
    let deleted = accept(&app, &gone, json!({"username": "eve", "password": "test-pass-1"})).await;
    assert_eq!(deleted.status, StatusCode::NOT_FOUND);

    // Only administrators manage invitations.
    let ann = app.login("ann", "test-pass-1").await;
    assert_eq!(app.get("/api/invites", &ann).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post("/api/invites", &ann, json!({})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/invites/{week}"), &ann).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn devices_belong_to_their_user() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let admin_id = app.get("/api/auth/me", &admin).await.body["id"].as_i64().unwrap();
    let (bob, bob_id) = app.user(&admin, "bob", json!({})).await;
    let (alice, _) = app.user(&admin, "alice", json!({})).await;

    let bobs = app.get(&format!("/api/users/{bob_id}/devices"), &admin).await;
    assert_eq!(bobs.body.as_array().unwrap().len(), 1);
    assert_eq!(bobs.body[0]["current"], json!(false), "nobody's current device in the admin view");
    let device = bobs.body[0]["id"].as_str().unwrap().to_owned();
    assert_eq!(device.len(), 16);

    for id in [device.as_str(), "not-hex-at-all-x", "abc"] {
        let reply = app.delete(&format!("/api/me/devices/{id}"), &alice).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{id}");
    }
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::OK, "alice can't sign bob out");

    assert_eq!(app.delete(&format!("/api/users/{admin_id}/devices"), &admin).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.delete(&format!("/api/users/{bob_id}/devices"), &admin).await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &bob).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.get(&format!("/api/users/{bob_id}/devices"), &admin).await.body, json!([]));
}

#[tokio::test]
async fn login_attempts_are_throttled_per_username() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    app.user(&admin, "bob", json!({})).await;
    app.user(&admin, "carol", json!({})).await;

    for _ in 0..5 {
        assert_eq!(app.try_login("bob", "wrong").await.status, StatusCode::UNAUTHORIZED);
    }
    let locked = app.try_login("bob", "test-pass-1").await;
    assert_eq!(locked.status, StatusCode::TOO_MANY_REQUESTS, "even with the right password");
    assert_eq!(locked.body["code"], json!("too_many_requests"));
    assert_eq!(app.try_login(" BOB ", "test-pass-1").await.status, StatusCode::TOO_MANY_REQUESTS, "same account");
    assert_eq!(app.try_login("carol", "test-pass-1").await.status, StatusCode::OK, "others are not affected");

    // Unknown names are throttled the same way, so they look like real ones.
    for _ in 0..5 {
        assert_eq!(app.try_login("ghost", "wrong").await.status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(app.try_login("ghost", "wrong").await.status, StatusCode::TOO_MANY_REQUESTS);

    // A successful sign-in starts the count again.
    for _ in 0..4 {
        app.try_login("carol", "wrong").await;
    }
    assert_eq!(app.try_login("carol", "test-pass-1").await.status, StatusCode::OK);
    for _ in 0..4 {
        app.try_login("carol", "wrong").await;
    }
    assert_eq!(app.try_login("carol", "test-pass-1").await.status, StatusCode::OK);
}

#[tokio::test]
async fn changing_your_password() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (bob, _) = app.user(&admin, "bob", json!({})).await;
    let short = change_password(&app, &bob, "test-pass-1", "abc").await;
    assert_eq!(short.status, StatusCode::BAD_REQUEST, "too short");
    assert_eq!(change_password(&app, &bob, "wrong", "password2").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(change_password(&app, &bob, "test-pass-1", "password2").await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.try_login("bob", "test-pass-1").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.try_login("bob", "password2").await.status, StatusCode::OK);

    let kid = app.post("/api/users", &admin, json!({"username": "kid", "managed": true})).await.body["id"].clone();
    let kid = switch(&app, &admin, json!({"user_id": kid})).await.body["token"].as_str().unwrap().to_owned();
    let managed = change_password(&app, &kid, "!managed", "password3").await;
    assert_eq!(managed.status, StatusCode::BAD_REQUEST);
    assert!(managed.body["message"].as_str().unwrap().contains("managed"));
}

#[tokio::test]
async fn a_new_password_signs_you_out_everywhere_else() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let (here, _) = app.user(&admin, "bob", json!({})).await;
    let elsewhere = app.login("bob", "test-pass-1").await;
    assert_eq!(app.get("/api/auth/me", &elsewhere).await.status, StatusCode::OK);
    assert_eq!(change_password(&app, &here, "test-pass-1", "a-new-one-2").await.status, StatusCode::NO_CONTENT);
    assert_eq!(app.get("/api/auth/me", &elsewhere).await.status, StatusCode::UNAUTHORIZED, "whoever knew the old one");
    assert_eq!(app.get("/api/auth/me", &here).await.status, StatusCode::OK, "not the device that changed it");
}

#[tokio::test]
async fn accounts_from_single_sign_on_need_a_password_from_an_administrator() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    // Made by single sign-on before it was removed: a marker instead of a
    // password hash, and a session from back then.
    let sam: i64 = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash, created_at) VALUES ('sam', '!sso', 0) RETURNING id",
    )
    .fetch_one(&app.state.db.write)
    .await
    .unwrap();
    let session = app.state.auth.create_session(sam, Some("a browser"), None).await.unwrap();

    assert_eq!(user_named(&app, &admin, "sam").await["has_password"], json!(false));
    let me = app.get("/api/auth/me", &session).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.text());
    assert_eq!((me.body["username"].as_str(), me.body["has_password"].as_bool()), (Some("sam"), Some(false)));

    // No password signs in to it, the marker included.
    assert_eq!(app.try_login("sam", "!sso").await.status, StatusCode::UNAUTHORIZED);
    // Trying takes as long as for a name that doesn't exist (a real check,
    // against a stand-in hash), so such accounts can't be found by timing.
    // The quickest of several tries leaves out waiting for the passwords of
    // other tests; each try has an account of its own, since 5 wrong
    // passwords hold a name up for 15 minutes.
    let (mut marker, mut unknown) = (Duration::MAX, Duration::MAX);
    for round in 0..8 {
        let name = format!("old-{round}");
        sqlx::query("INSERT INTO users (username, password_hash, created_at) VALUES (?, '!sso', 0)")
            .bind(&name)
            .execute(&app.state.db.write)
            .await
            .unwrap();
        let started = Instant::now();
        assert_eq!(app.try_login(&name, "!sso").await.status, StatusCode::UNAUTHORIZED);
        marker = marker.min(started.elapsed());
        let started = Instant::now();
        assert_eq!(app.try_login(&format!("nobody-{round}"), "!sso").await.status, StatusCode::UNAUTHORIZED);
        unknown = unknown.min(started.elapsed());
    }
    assert!(marker * 4 > unknown, "{marker:?}, and {unknown:?} for a name that doesn't exist");

    // Its session can't give it a password, nor a PIN.
    let no_password = Some("this account has no password: an administrator can give it one");
    let password = change_password(&app, &session, "", "a-new-one-2").await;
    assert_eq!((password.status, password.body["message"].as_str()), (StatusCode::BAD_REQUEST, no_password));
    let pin = app.put("/api/me/pin", &session, json!({"pin": "4321", "password": ""})).await;
    assert_eq!((pin.status, pin.body["message"].as_str()), (StatusCode::FORBIDDEN, no_password));

    // An administrator gives it one.
    let given = app.patch(&format!("/api/users/{sam}"), &admin, json!({"password": "sams-own-1"})).await;
    assert_eq!(given.status, StatusCode::OK, "{}", given.text());
    assert_eq!(given.body["has_password"], json!(true));
    let signed_in = app.try_login("sam", "sams-own-1").await;
    assert_eq!(signed_in.status, StatusCode::OK, "{}", signed_in.text());
    assert_eq!(signed_in.body["user"]["has_password"], json!(true));
}

#[tokio::test]
async fn single_sign_on_routes_are_gone() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (method, uri) in
        [(Method::GET, "/api/auth/sso/start"), (Method::GET, "/api/me/sso"), (Method::DELETE, "/api/users/1/sso")]
    {
        let reply = app.call(method, uri, Some(&admin), None, &[]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}: {}", reply.text());
        assert_eq!(reply.body["message"], json!("no such endpoint"), "{uri}");
    }
    let server = app.call(Method::GET, "/api/server", None, None, &[]).await;
    assert_eq!(server.status, StatusCode::OK);
    assert!(server.body.get("sso").is_none(), "{}", server.text());
}

#[tokio::test]
async fn restricted_profiles_cant_switch_into_freer_ones() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let new_profile = |body: Value| {
        let (app, admin) = (&app, &admin);
        async move { app.post("/api/users", admin, body).await.body["id"].as_i64().unwrap() }
    };
    let kid = new_profile(json!({"username": "kid", "managed": true, "max_age": 12})).await;
    let teen = new_profile(json!({"username": "teen", "managed": true, "max_age": 16})).await;
    let (_, parent) = app.user(&admin, "parent", json!({})).await;
    // Switching ends the session it started from: a separate one to switch with.
    let start = app.login("admin", "secret-one").await;
    let kid_token = switch(&app, &start, json!({"user_id": kid})).await.body["token"].as_str().unwrap().to_owned();
    let as_kid = |body: Value| switch(&app, &kid_token, body);

    // A higher age limit without a PIN: not from the kid's profile.
    let refused = as_kid(json!({"user_id": teen})).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(refused.body["message"].as_str().unwrap().contains("PIN"), "{:?}", refused.body);
    // No age limit at all: the profile's password.
    assert_eq!(as_kid(json!({"user_id": parent})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(as_kid(json!({"user_id": parent, "password": "wrong"})).await.status, StatusCode::FORBIDDEN);
    let parent_token = as_kid(json!({"user_id": parent, "password": "test-pass-1"})).await;
    assert_eq!(parent_token.status, StatusCode::OK, "{:?}", parent_token.body);
    let parent_token = parent_token.body["token"].as_str().unwrap().to_owned();

    // Down is always open; with a PIN, up is too.
    let teen_token = switch(&app, &parent_token, json!({"user_id": teen})).await;
    assert_eq!(teen_token.status, StatusCode::OK);
    let teen_token = teen_token.body["token"].as_str().unwrap().to_owned();
    let kid_token = switch(&app, &teen_token, json!({"user_id": kid})).await;
    assert_eq!(kid_token.status, StatusCode::OK);
    let kid_token = kid_token.body["token"].as_str().unwrap().to_owned();
    app.patch(&format!("/api/users/{teen}"), &admin, json!({"pin": "1357"})).await;
    assert_eq!(switch(&app, &kid_token, json!({"user_id": teen, "pin": "1357"})).await.status, StatusCode::OK);

    // Libraries count too: one the current profile can't see is more.
    let movies = app.library(&admin, "Movies", "movies", "movies").await;
    let shows = app.library(&admin, "Shows", "shows", "shows").await;
    let one =
        new_profile(json!({"username": "one", "managed": true, "all_libraries": false, "library_ids": [movies]})).await;
    let both = new_profile(
        json!({"username": "both", "managed": true, "all_libraries": false, "library_ids": [movies, shows]}),
    )
    .await;
    let start = app.login("admin", "secret-one").await;
    let one_token = switch(&app, &start, json!({"user_id": one})).await.body["token"].as_str().unwrap().to_owned();
    assert_eq!(switch(&app, &one_token, json!({"user_id": both})).await.status, StatusCode::FORBIDDEN);
    let start = app.login("admin", "secret-one").await;
    let both_token = switch(&app, &start, json!({"user_id": both})).await.body["token"].as_str().unwrap().to_owned();
    assert_eq!(switch(&app, &both_token, json!({"user_id": one})).await.status, StatusCode::OK);
}

#[tokio::test]
async fn pages_and_answers_carry_the_security_headers() {
    let app = TestApp::new().await;
    for uri in ["/api/health", "/", "/login"] {
        let reply = app.call(Method::GET, uri, None, None, &[]).await;
        let header = |name: &str| reply.headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
        assert!(header("content-security-policy").contains("frame-ancestors 'none'"), "{uri}");
        assert!(header("content-security-policy").contains("script-src 'self'"), "{uri}");
        assert_eq!(header("x-content-type-options"), "nosniff", "{uri}");
        assert_eq!(header("x-frame-options"), "DENY", "{uri}");
        assert_eq!(header("referrer-policy"), "same-origin", "{uri}");
    }
    let jellyfin = app.jf(Method::GET, "/System/Info/Public", "", None).await;
    assert_eq!(jellyfin.headers["x-content-type-options"], "nosniff");
    assert_eq!(jellyfin.headers["x-frame-options"], "DENY");
    assert!(jellyfin.headers["content-security-policy"].to_str().unwrap().contains("sandbox"));
}

#[tokio::test]
async fn browsers_keep_the_session_in_the_cookie_only() {
    let app = TestApp::new().await;
    app.setup_admin().await;
    let body = json!({"username": "admin", "password": "secret-one"});
    let browser = app
        .call(Method::POST, "/api/auth/login", None, Some(body.clone()), &[("origin", "http://pley.lan:7539")])
        .await;
    assert_eq!(browser.status, StatusCode::OK);
    assert!(browser.body.get("token").is_none(), "no page script sees it: {:?}", browser.body);
    assert!(browser.headers[header::SET_COOKIE].to_str().unwrap().starts_with("pley_token="));
    assert_eq!(browser.body["user"]["username"], "admin");
    // Scripts and apps (no Origin) get the token to send.
    let script = app.call(Method::POST, "/api/auth/login", None, Some(body), &[]).await;
    assert!(script.body["token"].as_str().is_some_and(|t| t.len() == 43));
}

/// `POST /api/setup` (or `GET /api/server` without a body) from `peer`,
/// with extra headers.
async fn from(
    app: &TestApp,
    peer: &str,
    method: Method,
    uri: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> Reply {
    let mut req = axum::http::Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let addr: std::net::SocketAddr = peer.parse().unwrap();
    let req = req.extension(axum::extract::ConnectInfo(addr));
    let req = match body {
        Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(axum::body::Body::from(b.to_string())),
        None => req.body(axum::body::Body::empty()),
    }
    .unwrap();
    let resp = tower::ServiceExt::oneshot(app.api.clone(), req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap().to_vec();
    Reply { status, headers: Default::default(), body: serde_json::from_slice(&bytes).unwrap_or(Value::Null), bytes }
}

#[tokio::test]
async fn setting_up_takes_the_code_in_the_log_from_anywhere() {
    let app = TestApp::new().await;
    let account = json!({"username": "owner", "password": "correct horse"});
    let with_code = |code: &str| {
        let mut body = account.clone();
        body["setup_code"] = json!(code);
        body
    };
    let server = |peer: &'static str| {
        let app = &app;
        async move { from(app, peer, Method::GET, "/api/server", None, &[]).await.body["setup_code_required"].clone() }
    };
    // Where a request seems to come from proves nothing (Docker, proxies and
    // DNS rebinding all make the internet look local): the code, always.
    for peer in ["127.0.0.1:50000", "192.168.1.20:50000", "203.0.113.9:50000"] {
        assert_eq!(server(peer).await, json!(true), "{peer}");
    }
    let setup = |body: Value, peer: &'static str| {
        let app = &app;
        async move { from(app, peer, Method::POST, "/api/setup", Some(body), &[]).await }
    };
    for peer in ["127.0.0.1:50000", "192.168.1.20:50000"] {
        let refused = setup(account.clone(), peer).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{peer}: {:?}", refused.body);
        assert!(refused.body["message"].as_str().unwrap().contains("setup code"));
    }
    let remote = "203.0.113.9:50000";
    assert_eq!(setup(with_code("AAAA-AAAA"), remote).await.status, StatusCode::FORBIDDEN);

    // The code works in any case, with or without its dash, and only once.
    let code = app.state.auth.setup.shown().unwrap();
    let done = setup(with_code(&code.replace('-', "").to_lowercase()), remote).await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.body);
    assert_eq!(app.state.auth.setup.shown(), None);
    let again = setup(with_code(&code), "127.0.0.1:50000").await;
    assert_eq!(again.status, StatusCode::CONFLICT, "already set up, whoever asks");
    assert_eq!(server(remote).await, json!(false));
}

#[tokio::test]
async fn setup_ends_when_an_account_is_made_elsewhere() {
    let app = TestApp::new().await;
    let code = app.state.auth.setup.shown().unwrap();
    // `pley reset-password admin … --admin` beside the running server.
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, is_admin, created_at)
         VALUES ((SELECT last + 1 FROM user_ids), 'admin', 'x', 1, 0)",
    )
    .execute(&app.state.db.write)
    .await
    .unwrap();
    let server = app.call(Method::GET, "/api/server", None, None, &[]).await;
    assert_eq!(server.body["setup_required"], json!(false));
    assert_eq!(app.state.auth.setup.shown(), None, "the code is done with");
    let body = json!({"username": "owner", "password": "correct horse", "setup_code": code});
    let late = app.call(Method::POST, "/api/setup", None, Some(body), &[]).await;
    assert_eq!(late.status, StatusCode::CONFLICT, "{:?}", late.body);
}

#[tokio::test]
async fn setup_codes_cant_be_guessed_and_guessing_locks_nobody_else_out() {
    let app = TestApp::new().await;
    let guess = || {
        let app = &app;
        async move {
            let body = json!({"username": "owner", "password": "correct horse", "setup_code": "AAAA-AAAA"});
            from(app, "203.0.113.9:50000", Method::POST, "/api/setup", Some(body), &[]).await.status
        }
    };
    for _ in 0..5 {
        assert_eq!(guess().await, StatusCode::FORBIDDEN);
    }
    assert_eq!(guess().await, StatusCode::TOO_MANY_REQUESTS);
    // The owner, elsewhere, isn't held up by someone else's guesses.
    let code = app.state.auth.setup.shown().unwrap();
    let body = json!({"username": "owner", "password": "correct horse", "setup_code": code});
    let owner = from(&app, "192.168.1.20:50000", Method::POST, "/api/setup", Some(body), &[]).await;
    assert_eq!(owner.status, StatusCode::OK, "{:?}", owner.body);
}

#[tokio::test]
async fn setup_checks_the_first_account_and_logout_clears_the_cookie() {
    let app = TestApp::new().await;
    let code = app.state.auth.setup.shown();
    for body in [
        json!({"username": "bad name", "password": "secret-one", "setup_code": code}),
        json!({"username": "admin", "password": "abc", "setup_code": code}),
        json!({"username": "admin", "password": "password123", "setup_code": code}),
        json!({"username": "admin", "password": "Admin", "setup_code": code}),
    ] {
        let reply = app.call(Method::POST, "/api/setup", None, Some(body), &[]).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{:?}", reply.body);
    }
    let server = app.call(Method::GET, "/api/server", None, None, &[]).await;
    assert_eq!(server.body["setup_required"], json!(true));
    let admin = app.setup_admin().await;
    let me = app.get("/api/auth/me", &admin).await;
    assert_eq!((me.body["is_admin"].clone(), me.body["home"].clone()), (json!(true), json!(true)));

    let out = app.call(Method::POST, "/api/auth/logout", Some(&admin), None, &[]).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    let cookie = out.headers[header::SET_COOKIE].to_str().unwrap();
    assert!(cookie.starts_with("pley_token=;") && cookie.contains("Max-Age=0"), "{cookie}");
    // Signing out without a session is fine too.
    assert_eq!(app.call(Method::POST, "/api/auth/logout", None, None, &[]).await.status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn each_profile_starts_episodes_with_its_own_tracks() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let season = "tv/Taxi/Season 01";
    for episode in ["S01E01", "S01E02"] {
        app.media_file(&format!("{season}/Taxi - {episode}.mkv"), 1024);
        app.media_file(&format!("{season}/Taxi - {episode}.fr.srt"), 64);
        // Two-letter and three-letter codes, in the other order the next time.
        let english = if episode == "S01E01" { "en" } else { "eng" };
        app.media_file(&format!("{season}/Taxi - {episode}.{english}.srt"), 64);
        app.media_file(&format!("{season}/Taxi - {episode}.eng.forced.srt"), 64);
    }
    let lib = app.library(&admin, "Shows", "shows", "tv").await;
    let show = app.ids(lib, &admin).await["Taxi"];
    let seasons = app.children(show, &admin).await;
    let episodes = app.children(seasons["Season 1"], &admin).await;
    let (first, second) = (episodes["Episode 1"], episodes["Episode 2"]);
    let kid_id =
        app.post("/api/users", &admin, json!({"username": "kid", "managed": true})).await.body["id"].as_i64().unwrap();
    let kid = switch(&app, &app.login("admin", "secret-one").await, json!({"user_id": kid_id})).await.body["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let admin = app.login("admin", "secret-one").await;
    // The subtitles a profile starts an episode with, by language and forced.
    let starts = |token: String, id: i64| {
        let app = &app;
        async move {
            let detail = app.get(&format!("/api/items/{id}"), &token).await.body;
            let file = &detail["files"][0];
            let selected = file["selected_subtitle"].clone();
            file["streams"].as_array().unwrap().iter().find(|s| s["index"] == selected).map(|s| {
                format!(
                    "{}{}",
                    s["language"].as_str().unwrap(),
                    if s["forced"] == json!(true) { " forced" } else { "" }
                )
            })
        }
    };

    // The administrator reads French; the kid keeps the defaults, and an
    // app of the kid's sets English, always.
    app.patch("/api/me", &admin, json!({"subtitle_mode": "always", "subtitle_language": "fr"})).await;
    assert_eq!(starts(admin.clone(), first).await.as_deref(), Some("fre"));
    assert_eq!(starts(kid.clone(), first).await, None, "untagged audio: no forced ones either");
    let (kid_app, _) = app.app_token(kid_id, "play").await;
    let set = app
        .app_call(
            Method::PUT,
            "/api/me/track-defaults",
            &kid_app,
            Some(json!({"subtitle_mode": "always", "subtitle_language": "en"})),
        )
        .await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.body);
    assert_eq!(starts(kid.clone(), first).await.as_deref(), Some("eng"), "the full ones, not the forced");
    assert_eq!(starts(admin.clone(), first).await.as_deref(), Some("fre"));

    // The kid picks the forced English ones in episode 1: episode 2 starts
    // with them, and the administrator's episodes don't change.
    let detail = app.get(&format!("/api/items/{first}"), &kid).await.body;
    let forced = detail["files"][0]["streams"].as_array().unwrap().iter().find(|s| s["forced"] == json!(true)).unwrap()
        ["index"]
        .clone();
    let picked = app
        .call(
            Method::PUT,
            &format!("/api/items/{first}/streams"),
            Some(&kid),
            Some(json!({"subtitle_index": forced})),
            &[],
        )
        .await;
    assert_eq!(picked.status, StatusCode::NO_CONTENT);
    assert_eq!(starts(kid.clone(), second).await.as_deref(), Some("eng forced"));
    assert_eq!(starts(admin.clone(), second).await.as_deref(), Some("fre"));
}

#[tokio::test]
async fn titles_start_with_the_users_default_tracks() {
    let app = TestApp::new().await;
    let token = app.setup_admin().await;
    app.media_file("movies/Amelie (2001)/Amelie (2001).mkv", 1024);
    app.media_file("movies/Amelie (2001)/Amelie (2001).en.srt", 64);
    app.media_file("movies/Amelie (2001)/Amelie (2001).fr.srt", 64);
    let lib = app.library(&token, "Movies", "movies", "movies").await;
    let film = app.ids(lib, &token).await["Amelie"];
    let selected = |detail: &Value| detail["files"][0]["selected_subtitle"].clone();
    let french = |detail: &Value| -> Value {
        detail["files"][0]["streams"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "subtitle" && s["language"] == "fre")
            .unwrap()["index"]
            .clone()
    };

    let me = app.get("/api/auth/me", &token).await.body;
    assert_eq!((&me["audio_language"], &me["subtitle_mode"]), (&Value::Null, &json!("forced")), "{me}");
    let detail = app.get(&format!("/api/items/{film}"), &token).await.body;
    assert_eq!(selected(&detail), Value::Null, "no defaults: forced subtitles at most");

    let saved = app.patch("/api/me", &token, json!({"subtitle_mode": "always", "subtitle_language": "fr"})).await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.body);
    assert_eq!((&saved.body["subtitle_mode"], &saved.body["subtitle_language"]), (&json!("always"), &json!("fr")));
    let detail = app.get(&format!("/api/items/{film}"), &token).await.body;
    assert_eq!(selected(&detail), french(&detail), "French subtitles, always");

    // Foreign mode needs to know the audio is foreign: an untagged track isn't.
    app.patch("/api/me", &token, json!({"subtitle_mode": "foreign"})).await;
    let detail = app.get(&format!("/api/items/{film}"), &token).await.body;
    assert_eq!(selected(&detail), Value::Null);

    for bad in
        [json!({"audio_language": "English"}), json!({"subtitle_language": "EN"}), json!({"subtitle_mode": "loud"})]
    {
        let refused = app.patch("/api/me", &token, bad.clone()).await;
        assert!(
            refused.status == StatusCode::BAD_REQUEST || refused.status == StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}"
        );
    }
    let cleared = app.patch("/api/me", &token, json!({"subtitle_language": null, "audio_language": "eng"})).await;
    assert_eq!((&cleared.body["subtitle_language"], &cleared.body["audio_language"]), (&Value::Null, &json!("eng")));

    // A choice made for the title wins over the defaults.
    app.patch("/api/me", &token, json!({"subtitle_mode": "always", "subtitle_language": "fre"})).await;
    let off = app
        .call(
            Method::PUT,
            &format!("/api/items/{film}/streams"),
            Some(&token),
            Some(json!({"subtitle_index": null})),
            &[],
        )
        .await;
    assert_eq!(off.status, StatusCode::NO_CONTENT, "{:?}", off.body);
    let detail = app.get(&format!("/api/items/{film}"), &token).await.body;
    assert_eq!(selected(&detail), Value::Null);
}
