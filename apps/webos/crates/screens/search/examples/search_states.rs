//! Search's states in the simulator, to look at them: the fake server slow (`loading`), out of
//! reach (`failed`), or answering with the profile's recent searches kept (`recent`).
//!
//! `cargo run -p pley-search --example search_states -- <state> [the simulator's options]`, for example
//! `-- loading --route search:ma --keys "wait:800" --screenshot loading.png`.
//!
//! It starts the fake server, signs in to it and keeps what the state needs, then runs the
//! simulator (this program again) against that server and folder.

use std::process::{Command, ExitCode};
use std::time::Duration;

use pley_core::auth::{StoredTokens, TokenAnswerParsed, Tokens};
use pley_core::{Http, Store};
use pley_fixtures::{FixtureServer, Mode, Options, ids};

const STATES: &[&str] = &["loading", "failed", "recent"];
/// Set for the simulator this program runs.
const CHILD: &str = "PLEY_SEARCH_STATES_CHILD";
/// The recent searches `recent` keeps, newest first.
const RECENT: &[&str] = &["severance", "daft punk", "the long dark night of the northern lights", "ma", "news"];

fn main() -> ExitCode {
    if std::env::var_os(CHILD).is_some() {
        pley_host::main(pley_host::App {
            version: env!("CARGO_PKG_VERSION"),
            screens: || {
                let mut screens = pley_shell::Registry::new();
                screens.add(pley_search::screen);
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
    let root = pley_core::device::scratch_dir("search-states");
    let data = root.join("data");
    let store = Store::open(data.join("store")).map_err(|e| e.to_string())?;
    let http = Http::network("pleyLG/0.0.0 (webOS 22; states)");
    sign_in(&server, &store, &http)?;
    // Kept oldest first, so the newest ends first.
    for q in RECENT.iter().rev() {
        store.remember_search(&url, ids::ALEX, q).map_err(|e| format!("not kept: {e}"))?;
    }
    match state {
        "loading" => server.set_mode(Mode::Slow).map_err(|e| e.to_string())?,
        "failed" => server.set_mode(Mode::Down).map_err(|e| e.to_string())?,
        _ => {}
    }

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
fn sign_in(server: &FixtureServer, store: &Store, http: &Http) -> Result<(), String> {
    let t = server.sign_in(ids::ALEX);
    let parsed = TokenAnswerParsed {
        access_token: t.access_token,
        refresh_token: Some(t.refresh_token),
        expires_in: t.expires_in as u64,
        scope: t.scope,
    };
    let url = server.url();
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let stored =
        StoredTokens::new(&parsed, url, &format!("{url}/api/oauth/token"), &format!("{url}/api/oauth/revoke"), now_ms);
    Tokens::shared(url, store, http).install(stored, &parsed.scope).map_err(|e| e.to_string())?;
    store.set_active_server(Some(url)).map_err(|e| e.to_string())?;
    store.set_last_profile(url, ids::ALEX).map_err(|e| e.to_string())?;
    store.remember_server(url, ids::SERVER_NAME, None, now_ms).map_err(|e| e.to_string())?;
    Ok(())
}
