//! Home's states in the simulator, to look at them: the fake server slow (`loading`), out of reach
//! with nothing saved (`failed`), out of reach with what was saved (`offline`), or answering
//! empty rows with libraries that are empty (`empty`), scanning (`scanning`) or none
//! (`no-libraries`).
//!
//! `cargo run -p pley-home --example home_states -- <state> [the simulator's options]`
//!
//! It starts the fake server, signs in to it and saves what the state needs (for `offline`, the
//! hero's details too, as Home saves them), then runs the
//! simulator (this program again) against that server and folder.

use std::process::{Command, ExitCode};
use std::time::Duration;

use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::model::{Hubs, Library};
use pley_core::store::snapshot_keys;
use pley_core::{Api, Http, Snapshots, Store};
use pley_fixtures::{FixtureServer, Mode, Options, ids};

const STATES: &[&str] = &["loading", "failed", "offline", "empty", "scanning", "no-libraries"];
/// Set for the simulator this program runs.
const CHILD: &str = "PLEY_HOME_STATES_CHILD";

fn main() -> ExitCode {
    if std::env::var_os(CHILD).is_some() {
        pley_host::main(pley_host::App {
            version: env!("CARGO_PKG_VERSION"),
            screens: || {
                let mut screens = pley_shell::Registry::new();
                screens.add(pley_home::screen);
                screens
            },
        });
        return ExitCode::SUCCESS;
    }
    let mut args = std::env::args().skip(1);
    let state = args.next().unwrap_or_default();
    if !STATES.contains(&state.as_str()) {
        eprintln!("usage: states <{}> [the simulator's options]", STATES.join("|"));
        return ExitCode::from(2);
    }
    match run(&state, args.collect()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run(state: &str, rest: Vec<String>) -> Result<ExitCode, String> {
    let options = Options {
        scan_every: None,
        adopt_unknown_tokens: true,
        slow_latency: Duration::from_secs(60),
        ..Options::default()
    };
    let server = FixtureServer::start_with(options).map_err(|e| format!("the fake server didn't start: {e}"))?;
    let url = server.url().to_string();
    let root = pley_core::device::scratch_dir("home-states");
    let data = root.join("data");
    let store = Store::open(data.join("store")).map_err(|e| e.to_string())?;
    let http = Http::network("pleyLG/0.0.0 (webOS 22; states)");
    let tokens = sign_in(&server, &store, &http)?;
    let api = Api::new(http.clone(), &url, tokens);
    let snapshots = Snapshots::open(data.join("snapshots"));
    let alex = ids::ALEX;
    let text = |e: pley_core::ApiError| e.to_string();
    let saved = |r: std::io::Result<()>| r.map_err(|e| format!("not saved: {e}"));

    let libraries = api.libraries().map_err(text)?;
    let empty = Hubs { hubs: Vec::new() };
    match state {
        "offline" => {
            saved(snapshots.put(&url, alex, snapshot_keys::ME, &api.me().map_err(text)?))?;
            saved(snapshots.put(&url, alex, snapshot_keys::LIBRARIES, &libraries))?;
            let home = api.home().map_err(text)?;
            // The hero's details, as Home saved them the last time it showed.
            if let Some(hero) = home.hubs.iter().find(|h| h.continue_watching()).and_then(|h| h.items.first()) {
                let detail = api.item(hero.id).map_err(text)?;
                saved(snapshots.put(&url, alex, &snapshot_keys::item(hero.id), &detail))?;
            }
            saved(snapshots.put(&url, alex, snapshot_keys::HOME, &home))?;
        }
        "empty" | "scanning" | "no-libraries" => {
            let libraries: Vec<Library> = match state {
                "empty" => libraries,
                "scanning" => libraries.into_iter().map(|l| Library { scanning: true, ..l }).collect(),
                _ => Vec::new(),
            };
            saved(snapshots.put(&url, alex, snapshot_keys::LIBRARIES, &libraries))?;
            saved(snapshots.put(&url, alex, snapshot_keys::HOME, &empty))?;
        }
        _ => {}
    }
    // Every state but loading has the server gone: what shows is what was saved, or nothing.
    let mode = if state == "loading" { Mode::Slow } else { Mode::Down };
    server.set_mode(mode).map_err(|e| e.to_string())?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let status = Command::new(exe)
        .env(CHILD, "1")
        .arg("--server")
        .arg(&url)
        .arg("--data")
        .arg(&root)
        .args(rest)
        .status()
        .map_err(|e| e.to_string())?;
    server.stop();
    let _ = std::fs::remove_dir_all(&root);
    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Signs in to `server` as Alex, keeps the tokens in `store` and makes it the server in use.
fn sign_in(server: &FixtureServer, store: &Store, http: &Http) -> Result<std::sync::Arc<Tokens>, String> {
    let t = server.sign_in(ids::ALEX);
    let parsed = TokenAnswerParsed {
        access_token: t.access_token,
        refresh_token: Some(t.refresh_token),
        expires_in: t.expires_in as u64,
        scope: t.scope,
        proof: None,
    };
    let url = server.url();
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let stored =
        StoredTokens::new(&parsed, url, &format!("{url}/api/oauth/token"), &format!("{url}/api/oauth/revoke"), now_ms);
    let tokens = Tokens::shared(url, store, http);
    tokens.install(stored, &parsed.scope).map_err(|e| e.to_string())?;
    store.set_active_server(Some(url)).map_err(|e| e.to_string())?;
    store.set_last_profile(url, ids::ALEX).map_err(|e| e.to_string())?;
    store.remember_server(url, ids::SERVER_NAME, None, now_ms).map_err(|e| e.to_string())?;
    Ok(tokens)
}
