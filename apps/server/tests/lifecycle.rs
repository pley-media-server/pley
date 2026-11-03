//! Starting and stopping the server: one server per data directory, nothing
//! touched before the ports are bound, what shutdown waits for and for how
//! long. Servers here are stopped through their tray context's Quit (see
//! `tests/shutdown.rs` for `pley::request_shutdown`).

mod common;

use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::StreamExt;
use pley::Config;
use pley::desktop::{Command, TrayContext, TrayLauncher};
use pley::events::ServerEvent;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;

use common::TestApp;

fn config(data: &std::path::Path) -> Config {
    let mut config = Config::for_data_dir(data);
    config.transcoder_path = Some(data.join("no-transcoder-in-tests"));
    config
}

/// The `pley` executable, without the `PLEY_*` settings of whoever runs the
/// tests.
fn pley_exe() -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_pley"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("PLEY_") {
            command.env_remove(key);
        }
    }
    command.kill_on_drop(true);
    command
}

/// pley running in this process: its address (`127.0.0.1:port`), the tray
/// context it handed out once it listened, and the task running it.
async fn start(config: Config) -> (String, TrayContext, JoinHandle<anyhow::Result<()>>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let launcher: TrayLauncher = Box::new(move |ctx: TrayContext| {
        let _ = tx.send(ctx);
        None
    });
    let server = tokio::spawn(pley::run_with_tray(config, Some(launcher)));
    let ctx = match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(ctx)) => ctx,
        _ => panic!("pley did not start: {:?}", tokio::time::timeout(Duration::from_secs(5), server).await),
    };
    let port: u16 = ctx.url.rsplit(':').next().and_then(|p| p.parse().ok()).expect("a port in the URL");
    (format!("127.0.0.1:{port}"), ctx, server)
}

/// How long pley may take to stop once asked, beyond the grace for requests
/// in progress: [`pley::run_with_tray`] gives stopping the transcoder and
/// closing the database 25 s. Closing the database copies its write-ahead
/// log into it and flushes it to disk, which takes as long as the disk does:
/// seconds on a busy one.
const STOPS_WITHIN: Duration = Duration::from_secs(30);

async fn stopped(server: JoinHandle<anyhow::Result<()>>) -> anyhow::Result<()> {
    tokio::time::timeout(STOPS_WITHIN, server).await.expect("pley stopped in time").expect("pley did not panic")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn more_addresses_are_served_and_one_taken_is_tried_until_free() {
    let data = common::tempdir();
    let mut config = config(data.path());
    let second = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    // Like an adapter's address that comes after pley (Wi-Fi, a VPN).
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let later = taken.local_addr().unwrap();
    config.extra_binds = vec![second, later];
    config.extra_bind_retry = Duration::from_millis(100);
    let (_first, ctx, server) = start(config).await;

    let http = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
    let answers = |addr: std::net::SocketAddr| {
        let http = http.clone();
        async move { http.get(format!("http://{addr}/api/health")).send().await.is_ok_and(|r| r.status().is_success()) }
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !answers(second).await {
        assert!(Instant::now() < deadline, "pley never listened on {second}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    drop(taken);
    while !answers(later).await {
        assert!(Instant::now() < deadline, "pley never listened on {later} once it was free");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
    assert!(!answers(second).await, "{second} closed with pley");
}

/// A port nothing listens on (as long as nothing takes it meanwhile).
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn where_pley_listens_changes_while_it_runs() {
    let data = common::tempdir();
    let mut config = config(data.path());
    config.extra_bind_retry = Duration::from_millis(100);
    let (first, ctx, server) = start(config).await;
    let made =
        pley_exe().args(["reset-password", "admin", "test-pass-1", "--admin", "--data-dir"]).arg(data.path()).output();
    assert!(tokio::time::timeout(Duration::from_secs(60), made).await.unwrap().unwrap().status.success());
    let http = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(10)).build().unwrap();
    let login: serde_json::Value = http
        .post(format!("http://{first}/api/auth/login"))
        .json(&json!({"username": "admin", "password": "test-pass-1"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = login["token"].as_str().unwrap().to_owned();
    let change = |at: String, body: serde_json::Value| {
        let (http, token) = (http.clone(), token.clone());
        async move {
            let reply =
                http.put(format!("http://{at}/api/network")).bearer_auth(token).json(&body).send().await.unwrap();
            assert_eq!(reply.status(), reqwest::StatusCode::OK);
            reply.json::<serde_json::Value>().await.unwrap()
        }
    };
    let answers = |at: String| {
        let http = http.clone();
        async move { http.get(format!("http://{at}/api/health")).send().await.is_ok_and(|r| r.status().is_success()) }
    };

    // Two addresses instead of the first: the answer comes once they listen.
    let (a, b) = (format!("127.0.0.1:{}", free_port()), format!("127.0.0.1:{}", free_port()));
    let now = change(first.clone(), json!({"web": [&a, &b], "jellyfin": [], "discovery": false})).await;
    let listening =
        |now: &serde_json::Value| now["listeners"].as_array().unwrap().iter().filter(|l| l["error"].is_null()).count();
    assert_eq!(listening(&now), 2, "{now}");
    assert!(answers(a.clone()).await && answers(b.clone()).await);
    assert!(!answers(first.clone()).await, "the first address closed");
    let saved = std::fs::read_to_string(data.path().join("pley.toml")).unwrap();
    assert!(saved.contains(&format!("bind = \"{a}, {b}\"")), "{saved}");

    // One taken by another program is listened on once it is free; the
    // Jellyfin-compatible API opens on a port of its own.
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let later = taken.local_addr().unwrap().to_string();
    let jellyfin = format!("127.0.0.1:{}", free_port());
    let now = change(b.clone(), json!({"web": [&b, &later], "jellyfin": [&jellyfin], "discovery": false})).await;
    let waiting: Vec<_> = now["listeners"].as_array().unwrap().iter().filter(|l| !l["error"].is_null()).collect();
    assert_eq!(waiting.len(), 1, "{now}");
    assert_eq!(waiting[0]["address"], later.as_str());
    assert!(!answers(a.clone()).await, "{a} closed");
    let info = http.get(format!("http://{jellyfin}/System/Info/Public")).send().await.unwrap();
    assert!(info.status().is_success(), "the Jellyfin-compatible API answers");
    drop(taken);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !answers(later.clone()).await {
        assert!(Instant::now() < deadline, "pley never listened on {later} once it was free");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
    assert!(!answers(b).await && !answers(later).await, "closed with pley");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_server_per_data_directory() {
    let data = common::tempdir();
    let (_, ctx, server) = start(config(data.path())).await;

    // A second copy gives up at once, before it opens the database.
    let second = tokio::time::timeout(Duration::from_secs(10), pley::run(config(data.path()))).await;
    let message =
        format!("{:#}", second.expect("the second copy gives up at once").expect_err("the directory is taken"));
    assert!(message.starts_with(&format!("another Pley (process {}) uses", std::process::id())), "{message}");

    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
    // The lock went with the server that held it. On Linux and macOS a
    // process another test starts meanwhile shares the lock's descriptor
    // until it runs its program, so it may stay a moment longer.
    let deadline = Instant::now() + Duration::from_secs(5);
    while let Err(err) = pley::instance::InstanceLock::acquire(data.path()) {
        assert!(Instant::now() < deadline, "the lock stayed after its server stopped: {err:#}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (_, ctx, server) = start(config(data.path())).await;
    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
}

#[tokio::test]
async fn a_taken_port_stops_the_start_before_the_database_is_opened() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let data = common::tempdir();
    let mut config = config(data.path());
    config.bind = taken.local_addr().unwrap();
    let message = format!("{:#}", pley::run(config).await.expect_err("the port is taken"));
    assert!(message.contains(&format!("cannot listen on {}", taken.local_addr().unwrap())), "{message}");
    for made_by_the_start in ["pley.db", "cache", "transcode"] {
        assert!(!data.path().join(made_by_the_start).exists(), "{made_by_the_start} exists");
    }
}

/// Runs `pley serve` with `args` and the data directory `data`, on a port
/// that is taken (so it never runs for real, whatever happens), and returns
/// its exit status, its standard error and its log file.
async fn failed_start(data: &std::path::Path, args: &[&str]) -> (std::process::ExitStatus, String, String) {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let run = pley_exe()
        .args(["--no-tray", "--jellyfin-bind", "off", "--bind"])
        .arg(taken.local_addr().unwrap().to_string())
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .output();
    let output = tokio::time::timeout(Duration::from_secs(60), run).await.expect("pley exits").unwrap();
    let log = std::fs::read_to_string(data.join("logs").join("pley.log")).unwrap_or_default();
    (output.status, String::from_utf8_lossy(&output.stderr).into_owned(), log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn why_the_server_could_not_start_is_in_its_log() {
    // The port is taken.
    let data = common::tempdir();
    let (status, stderr, log) = failed_start(data.path(), &[]).await;
    assert!(!status.success());
    assert!(log.contains("ERROR Pley could not start: cannot listen on 127.0.0.1:"), "{log}");
    assert_eq!(stderr.matches("cannot listen on").count(), 1, "on the console once: {stderr}");

    // Another pley uses the data directory (here, this process).
    let busy = common::tempdir();
    let (_, ctx, server) = start(config(busy.path())).await;
    let (status, _, log) = failed_start(busy.path(), &[]).await;
    assert!(!status.success());
    let holder = format!("Pley could not start: another Pley (process {}) uses the data", std::process::id());
    assert!(log.contains(&holder), "{log}");
    ctx.send(Command::Quit);
    stopped(server).await.unwrap();

    // pley.toml can't be read: the log of the data directory it is in.
    let broken = common::tempdir();
    std::fs::write(broken.path().join("pley.toml"), "bindd = \"8080\"\n").unwrap();
    let (status, _, log) = failed_start(broken.path(), &[]).await;
    assert!(!status.success());
    assert!(log.contains("ERROR Pley could not start: parsing config file"), "{log}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_pley_stops_when_one_answers_on_its_port() {
    let data = common::tempdir();
    let (addr, ctx, server) = start(config(data.path())).await;
    let port = addr.rsplit(':').next().unwrap();

    // Another data directory, and 0.0.0.0, which Windows would let it listen
    // on next to the first's 127.0.0.1: the two would share the port.
    let other = common::tempdir();
    let run = pley_exe()
        .args(["--no-tray", "--jellyfin-bind", "off", "--bind"])
        .arg(format!("0.0.0.0:{port}"))
        .arg("--data-dir")
        .arg(other.path())
        .output();
    let output = tokio::time::timeout(Duration::from_secs(60), run).await.expect("pley exits").unwrap();
    assert!(!output.status.success());
    let log = std::fs::read_to_string(other.path().join("logs").join("pley.log")).unwrap_or_default();
    let expected = format!("ERROR Pley could not start: another Pley already listens on http://localhost:{port}");
    assert!(log.contains(&expected), "{log}");
    assert!(!other.path().join("pley.db").exists(), "it stopped before the database");

    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_password_works_next_to_a_running_server() {
    let data = common::tempdir();
    let (addr, ctx, server) = start(config(data.path())).await;

    let run =
        pley_exe().args(["reset-password", "admin", "test-pass-1", "--admin", "--data-dir"]).arg(data.path()).output();
    let output = tokio::time::timeout(Duration::from_secs(60), run).await.expect("pley exits").unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("created user 'admin' (admin)"));
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let login = http
        .post(format!("http://{addr}/api/auth/login"))
        .json(&json!({"username": "admin", "password": "test-pass-1"}))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), reqwest::StatusCode::OK, "the running server sees the new account");

    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
}

#[tokio::test]
async fn the_event_stream_ends_when_the_server_stops() {
    let app = TestApp::new().await;
    let admin = app.setup_admin().await;
    let request = Request::builder()
        .uri("/api/events")
        .header("authorization", format!("Bearer {admin}"))
        .body(Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.api.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body().into_data_stream();

    app.state.events.send(ServerEvent::LibrariesChanged);
    let event = tokio::time::timeout(Duration::from_secs(5), stream.next()).await.expect("an event");
    let event = event.expect("the stream is open").unwrap();
    assert!(String::from_utf8_lossy(&event).contains("libraries_changed"));

    app.state.stopping.cancel();
    let end = tokio::time::timeout(Duration::from_secs(5), stream.next()).await.expect("the stream ends");
    assert!(end.is_none(), "{end:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_tray_quit_stops_its_server() {
    let data = common::tempdir();
    let (addr, ctx, server) = start(config(data.path())).await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let health = http.get(format!("http://{addr}/api/health")).send().await.unwrap();
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    assert_ne!(ctx.status(), "shutting down");

    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
    assert!(http.get(format!("http://{addr}/api/health")).send().await.is_err(), "the listener is closed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requests_in_progress_get_a_grace_period_and_no_more() {
    let data = common::tempdir();
    let mut config = config(data.path());
    config.shutdown_grace = Duration::from_millis(500);
    let (addr, ctx, server) = start(config).await;

    // A request whose body never comes keeps its connection busy, as a film
    // playing directly does. "100 Continue" shows the handler reads the body.
    let mut upload = tokio::net::TcpStream::connect(&addr).await.unwrap();
    upload
        .write_all(
            b"POST /api/setup HTTP/1.1\r\nhost: pley\r\ncontent-type: application/json\r\n\
              content-length: 64\r\nexpect: 100-continue\r\n\r\n",
        )
        .await
        .unwrap();
    let mut reply = [0u8; 64];
    let n = tokio::time::timeout(Duration::from_secs(5), upload.read(&mut reply)).await.unwrap().unwrap();
    let reply = String::from_utf8_lossy(&reply[..n]);
    assert!(reply.starts_with("HTTP/1.1 100"), "{reply}");

    let asked = Instant::now();
    ctx.send(Command::Quit);
    stopped(server).await.unwrap();
    let took = asked.elapsed();
    assert!(took >= Duration::from_millis(500), "the request in progress had its grace period");
    // Waiting for the request would have lasted until its body timed out.
    assert!(took < pley::http::BODY_IDLE_TIMEOUT / 2, "and no more: {took:?}");
}
