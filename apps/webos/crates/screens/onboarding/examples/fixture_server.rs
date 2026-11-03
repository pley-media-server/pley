//! The fake Pley server on its own, for looking at the sign-in screens in the simulator without a
//! sign-in already made: `cargo run -p pley-onboarding --example fixture_server -- --port 47539
//! --approval never`, then `cargo run -p pley-onboarding --example onboarding_sim -- --server
//! http://127.0.0.1:47539`. `--approval allow|deny|never` is what happens to sign-in codes and
//! `--delay <s>` how long the pretend person takes; `--for <s>` how long the server runs (10 min);
//! `--adopt` takes sign-ins it didn't make, so a simulator's data kept with `--data` from an earlier
//! run stays signed in.

use std::time::Duration;

use pley_fixtures::{DeviceApproval, FixtureServer, Options};

fn main() {
    let mut port = 47539u16;
    let mut approval = DeviceApproval::Never;
    let mut delay = 0u64;
    let mut run_for = 600u64;
    let mut adopt = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| panic!("{arg} needs a value"));
        match arg.as_str() {
            "--port" => port = value().parse().expect("a port"),
            "--approval" => {
                approval = match value().as_str() {
                    "allow" => DeviceApproval::Allow,
                    "deny" => DeviceApproval::Deny,
                    _ => DeviceApproval::Never,
                }
            }
            "--delay" => delay = value().parse().expect("seconds"),
            "--for" => run_for = value().parse().expect("seconds"),
            "--adopt" => adopt = true,
            other => panic!("unknown option {other}"),
        }
    }
    let options =
        Options { port, scan_every: None, device_interval_secs: 1, adopt_unknown_tokens: adopt, ..Options::default() };
    let server = FixtureServer::start_with(options).expect("the fake server starts");
    server.set_device_approval(approval);
    server.set_device_delay(Duration::from_secs(delay));
    println!("the fake server listens at {}", server.url());
    std::thread::sleep(Duration::from_secs(run_for));
}
