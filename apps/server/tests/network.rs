//! Settings → Network (src/network.rs): where pley listens, which an
//! administrator changes and pley saves in its config file. Listening anew
//! while pley runs is in lifecycle.rs.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::TestApp;

#[tokio::test]
async fn administrators_see_where_pley_listens_and_could() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let reply = app.get("/api/network", &admin).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["web"], json!(["127.0.0.1:0"]));
    assert_eq!((&reply.body["plex"], &reply.body["plex_on_web"]), (&json!([]), &json!(true)));
    assert_eq!(reply.body["jellyfin"], json!([]));
    assert_eq!(
        reply.body["fixed"],
        json!({"web": false, "plex": false, "jellyfin": false, "discovery": false, "bonjour": false})
    );
    assert_eq!((&reply.body["bonjour"], &reply.body["bonjour_adapters"]), (&json!(false), &json!([])));
    assert_eq!(reply.body["bonjour_host_name"], json!(""));
    assert!(reply.body["bonjour_host_default"].as_str().unwrap().starts_with("pley-"), "{:?}", reply.body);
    assert_eq!(reply.body["container_network"], false, "the tests run on the computer itself");
    assert_eq!(reply.body["windows"], cfg!(windows));
    assert!(reply.body["adapters"].is_array());
    assert!(reply.body["config_file"].as_str().unwrap().ends_with("pley.toml"));

    let (user, _) = app.user(&admin, "viewer", json!({})).await;
    assert_eq!(app.get("/api/network", &user).await.status, StatusCode::FORBIDDEN);
    let change = json!({"web": ["0.0.0.0:7539"], "jellyfin": [], "discovery": true});
    assert_eq!(app.put("/api/network", &user, change).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_change_is_saved_for_the_next_start() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let change = json!({
        "web": ["127.0.0.1:7539", "192.168.1.10:7539"],
        "plex": [],
        "plex_on_web": true,
        "jellyfin": [],
        "discovery": false,
    });
    let reply = app.put("/api/network", &admin, change).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["web"], json!(["127.0.0.1:7539", "192.168.1.10:7539"]));

    let saved = std::fs::read_to_string(&app.state.config.config_file).unwrap();
    assert!(saved.contains("bind = \"127.0.0.1:7539, 192.168.1.10:7539\"\n"), "{saved}");
    assert!(saved.contains("plex_bind = \"web\"\n"), "{saved}");
    assert!(saved.contains("jellyfin_bind = \"off\"\n"), "{saved}");
    assert!(saved.contains("discovery = false\n"), "{saved}");
    assert_eq!(app.get("/api/network", &admin).await.body["web"], json!(["127.0.0.1:7539", "192.168.1.10:7539"]));
}

#[tokio::test]
async fn the_jellyfin_api_is_off_until_turned_on_on_pleys_port_or_its_own() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let now = app.get("/api/network", &admin).await.body;
    assert_eq!((&now["jellyfin"], &now["jellyfin_on_web"]), (&json!([]), &json!(false)), "off by default");
    for (change, saved) in [
        (json!({"web": ["127.0.0.1:7539"], "jellyfin": [], "jellyfin_on_web": true, "discovery": false}), "web"),
        (json!({"web": ["127.0.0.1:7539"], "jellyfin": ["0.0.0.0:8096"], "discovery": false}), "0.0.0.0:8096"),
        (json!({"web": ["127.0.0.1:7539"], "jellyfin": [], "discovery": false}), "off"),
    ] {
        let reply = app.put("/api/network", &admin, change.clone()).await;
        assert_eq!(reply.status, StatusCode::OK, "{change}: {:?}", reply.body);
        assert_eq!(reply.body["jellyfin_on_web"], json!(change["jellyfin_on_web"].as_bool().unwrap_or(false)));
        let file = std::fs::read_to_string(&app.state.config.config_file).unwrap();
        assert!(file.contains(&format!("jellyfin_bind = \"{saved}\"\n")), "{file}");
    }
}

#[tokio::test]
async fn the_plex_api_is_on_pleys_port_until_given_its_own_or_turned_off() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let now = app.get("/api/network", &admin).await.body;
    assert_eq!((&now["plex"], &now["plex_on_web"]), (&json!([]), &json!(true)), "on Pley's port by default");
    for (change, saved) in [
        (json!({"web": ["127.0.0.1:7539"], "plex": ["0.0.0.0:32400"], "discovery": false}), "0.0.0.0:32400"),
        (json!({"web": ["127.0.0.1:7539"], "plex": [], "discovery": false}), "off"),
        (json!({"web": ["127.0.0.1:7539"], "plex": [], "plex_on_web": true, "discovery": false}), "web"),
        // Both APIs on Pley's port.
        (json!({"web": ["127.0.0.1:7539"], "plex_on_web": true, "jellyfin_on_web": true, "discovery": false}), "web"),
    ] {
        let reply = app.put("/api/network", &admin, change.clone()).await;
        assert_eq!(reply.status, StatusCode::OK, "{change}: {:?}", reply.body);
        assert_eq!(reply.body["plex_on_web"], json!(change["plex_on_web"].as_bool().unwrap_or(false)));
        let file = std::fs::read_to_string(&app.state.config.config_file).unwrap();
        assert!(file.contains(&format!("plex_bind = \"{saved}\"\n")), "{file}");
    }
    let one_port =
        json!({"web": ["127.0.0.1:7539"], "plex": ["0.0.0.0:8096"], "jellyfin": ["0.0.0.0:8096"], "discovery": false});
    assert_eq!(app.put("/api/network", &admin, one_port).await.status, StatusCode::BAD_REQUEST, "one port, two APIs");
}

#[tokio::test]
async fn what_pley_cant_listen_on_is_refused() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    for (change, why) in [
        (json!({"web": [], "jellyfin": [], "discovery": true}), "nowhere"),
        (json!({"web": ["127.0.0.1:0"], "jellyfin": [], "discovery": true}), "no port"),
        (
            json!({"web": ["0.0.0.0:7539", "192.168.1.10:7539"], "jellyfin": [], "discovery": true}),
            "0.0.0.0 has them all",
        ),
        (json!({"web": ["127.0.0.1:7539"], "jellyfin": ["127.0.0.1:7539"], "discovery": true}), "one port, two APIs"),
        (json!({"web": ["somewhere"], "jellyfin": [], "discovery": true}), "not an address"),
        (
            json!({"web": ["127.0.0.1:7539"], "jellyfin": ["127.0.0.1:8096"], "jellyfin_on_web": true, "discovery": true}),
            "Pley's port or its own, not both",
        ),
    ] {
        let reply = app.put("/api/network", &admin, change).await;
        assert!(reply.status.is_client_error(), "{why}: {:?}", reply.body);
    }
    assert!(!app.state.config.config_file.exists(), "nothing was saved");
}

#[tokio::test]
async fn bonjour_is_announced_from_the_adapters_chosen() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let change = json!({
        "web": ["0.0.0.0:7539"], "jellyfin": [], "discovery": false, "bonjour": true,
        "bonjour_adapters": ["Wi-Fi", "Ethernet ", "Ethernet"], "bonjour_host_name": " Media.local",
    });
    let reply = app.put("/api/network", &admin, change).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["bonjour"], json!(true));
    assert_eq!(reply.body["bonjour_adapters"], json!(["Ethernet", "Wi-Fi"]), "each once");
    assert_eq!(reply.body["bonjour_host_name"], json!("media"), "its label, lowercase");
    let saved = std::fs::read_to_string(&app.state.config.config_file).unwrap();
    assert!(saved.contains("bonjour = true\n"), "{saved}");
    assert!(saved.contains("bonjour_adapters = [\"Ethernet\", \"Wi-Fi\"]\n"), "{saved}");
    assert!(saved.contains("bonjour_host_name = \"media\"\n"), "{saved}");

    let no_name = json!({"web": ["0.0.0.0:7539"], "jellyfin": [], "discovery": true, "bonjour_adapters": [" "]});
    let reply = app.put("/api/network", &admin, no_name).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{:?}", reply.body);
    let spaced = json!({"web": ["0.0.0.0:7539"], "jellyfin": [], "discovery": true, "bonjour_host_name": "my media"});
    let reply = app.put("/api/network", &admin, spaced).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{:?}", reply.body);
}

#[tokio::test]
async fn what_the_command_line_sets_stays() {
    // As with --bind 127.0.0.1:7539 (this app listens nowhere).
    let app = TestApp::with_config(|config| {
        config.bind = "127.0.0.1:7539".parse().unwrap();
        config.network_fixed.web = true;
    })
    .await;
    let admin = app.setup_admin().await;
    assert_eq!(app.get("/api/network", &admin).await.body["fixed"]["web"], true);
    let elsewhere = json!({"web": ["0.0.0.0:7539"], "jellyfin": [], "discovery": false});
    let reply = app.put("/api/network", &admin, elsewhere).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{:?}", reply.body);

    // The rest can change, and is saved without the address.
    let jellyfin = json!({"web": ["127.0.0.1:7539"], "jellyfin": ["127.0.0.1:8096"], "discovery": false});
    let reply = app.put("/api/network", &admin, jellyfin).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    let saved = std::fs::read_to_string(&app.state.config.config_file).unwrap();
    assert!(!saved.lines().any(|line| line.starts_with("bind")), "{saved}");
    assert!(saved.contains("jellyfin_bind = \"127.0.0.1:8096\""), "{saved}");
}

#[tokio::test]
async fn the_transcode_folder_is_chosen_for_the_next_start() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let config = &app.state.config;
    let default = config.default_transcode_dir().to_string_lossy().into_owned();
    let folder = |reply: &common::Reply| reply.body["transcode_dir"].clone();
    let system = app.get("/api/system", &admin).await;
    assert_eq!(
        folder(&system),
        json!({"in_use": default, "next": null, "default": default, "problem": null, "fixed": false})
    );

    let put = |path: Value| app.put("/api/system/transcode-dir", &admin, json!({"path": path}));
    for refused in [json!("relative/folder"), json!(config.data_dir.to_string_lossy())] {
        assert_eq!(put(refused.clone()).await.status, StatusCode::BAD_REQUEST, "{refused}");
    }
    let blocked = config.data_dir.join("a file");
    std::fs::write(&blocked, "").unwrap();
    let cannot = put(json!(blocked.join("transcode").to_string_lossy())).await;
    assert_eq!(cannot.status, StatusCode::BAD_REQUEST, "pley can't write there");

    let chosen = app.media.path().join("fast disk");
    let saved = put(json!(chosen.to_string_lossy())).await;
    assert_eq!((saved.status, &saved.body["next"]), (StatusCode::OK, &json!(chosen.to_string_lossy())));
    assert_eq!(saved.body["in_use"], json!(default), "the transcoder goes on where it is");
    assert_eq!(pley::config::saved_transcode_dir(&config.config_file), Some(chosen));
    assert_eq!(folder(&app.get("/api/system", &admin).await)["next"], saved.body["next"]);

    let (viewer, _) = app.user(&admin, "viewer", json!({})).await;
    assert_eq!(
        app.put("/api/system/transcode-dir", &viewer, json!({"path": null})).await.status,
        StatusCode::FORBIDDEN
    );
    let back = put(Value::Null).await;
    assert_eq!((back.status, &back.body["next"]), (StatusCode::OK, &Value::Null));
    assert_eq!(pley::config::saved_transcode_dir(&config.config_file), None);
}
