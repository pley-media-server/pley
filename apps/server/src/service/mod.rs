//! pley as a Windows service (`pley service …`), as the Windows installer
//! (`deploy/windows`) sets it up and an administrator can by hand.
//!
//! The service runs `pley.exe --data-dir <folder> service run` as an account
//! of its own: the virtual account `NT SERVICE\pley`, which has no password
//! and can't sign in, or a local user, for media on network shares (which
//! the virtual account reaches as the computer, not as a user). Its options
//! are in `pley.toml` in the data folder, which only the service, the
//! administrators and SYSTEM (and whoever `--grant` names) may open. Unless
//! `--unrestricted`, Windows lets the service write only where it is
//! explicitly allowed to (its data folder) and keeps only the privileges it
//! needs.

mod accounts;
mod scm;

use std::fmt::Display;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Args, Subcommand, ValueEnum};

use crate::config::Cli;

/// The service's name, and so its account's: `NT SERVICE\pley`.
pub const NAME: &str = "pley";

/// How long starting or stopping the service may take.
const SERVICE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Subcommand)]
pub enum ServiceCommand {
    /// Install the service, or update it, and start it. The options given
    /// before `service` (--data-dir, --bind, --jellyfin-bind, --ffmpeg, --ffprobe)
    /// are saved in pley.toml in its data folder (default: %ProgramData%\pley)
    Install(InstallArgs),
    /// Stop and remove the service. Its data folder stays
    Uninstall(UninstallArgs),
    /// Start the service and wait until Pley answers
    Start,
    /// Stop the service and wait until it has stopped
    Stop,
    /// What Windows runs for the service, not for a terminal
    #[command(hide = true)]
    Run,
    /// Save a network share's sign-in for the account running this (the
    /// installer runs it as the service's user)
    #[command(hide = true)]
    StoreCredential,
    /// Wait for the Pley that started on --data-dir in the last minutes, and
    /// print its address, setup link and what it found FFmpeg can do (the
    /// installer runs it for Pley in the notification area)
    #[command(hide = true)]
    Report,
    /// Copy another data folder into --data-dir, which has no database yet
    /// (the installer runs it for Pley in the notification area)
    #[command(hide = true)]
    CopyData {
        #[arg(long, value_name = "DIR")]
        from: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Account {
    /// NT SERVICE\pley: no password, can't sign in, reaches network shares as the computer
    Virtual,
    /// A local user (--user), for media on network shares
    User,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// The account the service runs as
    #[arg(long, value_enum, default_value_t = Account::Virtual)]
    pub account: Account,
    /// The local user, with --account user
    #[arg(long, default_value = NAME)]
    pub user: String,
    /// The environment variable that holds the user's password, with --account user
    /// (with --create-user and none, a random one)
    #[arg(long, default_value = "PLEY_SERVICE_PASSWORD")]
    pub password_env: String,
    /// Create the user, a standard one, or set the password of the one an earlier install made
    #[arg(long)]
    pub create_user: bool,
    /// Let this account open the data folder too, such as the administrator installing Pley (repeatable)
    #[arg(long, value_name = "ACCOUNT")]
    pub grant: Vec<String>,
    /// Let the service write wherever its account may and keep all of its privileges
    #[arg(long)]
    pub unrestricted: bool,
    /// Set the owner and permissions of every file in the data folder, even when the folder is set up already
    #[arg(long)]
    pub reset_permissions: bool,
    /// Whether to answer the Pley apps looking for servers on the local network
    #[arg(long, value_name = "true|false")]
    pub discovery: Option<bool>,
    /// A network share's server (\\nas) to save a sign-in for, with --account user
    #[arg(long, value_name = "SERVER")]
    pub share: Option<String>,
    /// The share's user name, with --share
    #[arg(long, value_name = "NAME", requires = "share")]
    pub share_user: Option<String>,
    /// The environment variable that holds the share's password, with --share
    #[arg(long, default_value = "PLEY_SHARE_PASSWORD")]
    pub share_password_env: String,
    /// Delete this local user once the service no longer runs as it (one an
    /// earlier install made with --create-user)
    #[arg(long, value_name = "USER")]
    pub remove_user: Option<String>,
    /// Copy the data of the Pley that kept it in this folder first, while the
    /// data folder has no database yet (that Pley must be stopped)
    #[arg(long, value_name = "DIR")]
    pub copy_data_from: Option<PathBuf>,
    /// Install or update the service without starting it
    #[arg(long)]
    pub no_start: bool,
    /// Print what would be done and change nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct UninstallArgs {
    /// Also delete this local user and its profile (the one --create-user made)
    #[arg(long, value_name = "USER")]
    pub delete_user: Option<String>,
    /// Print what would be done and change nothing
    #[arg(long)]
    pub dry_run: bool,
}

/// `pley service …`.
pub fn main(cli: &Cli, command: &ServiceCommand) -> anyhow::Result<ExitCode> {
    match command {
        ServiceCommand::Run => return Ok(scm::dispatch(cli)),
        ServiceCommand::Install(args) => install(cli, args)?,
        ServiceCommand::Uninstall(args) => uninstall(args)?,
        ServiceCommand::Start => start(&data_dir(cli))?,
        ServiceCommand::Stop => {
            if !scm::stop(SERVICE_TIMEOUT)? {
                println!("the Pley service isn't installed");
            }
        }
        ServiceCommand::StoreCredential => accounts::store_credential_from_env()?,
        ServiceCommand::Report => report(&data_dir(cli), crate::util::now() - 120)?,
        ServiceCommand::CopyData { from } => {
            let to = cli.data_dir_or_default();
            println!("copy {} into {}", from.display(), to.display());
            copy_data(from, &to)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The service's data folder: the command line's, or `%ProgramData%\pley`
/// (not the default of `pley serve`, which is in a user's profile).
fn data_dir(cli: &Cli) -> PathBuf {
    match &cli.data_dir {
        Some(_) => cli.data_dir_or_default(),
        None => {
            let program_data =
                std::env::var_os("ProgramData").map_or_else(|| PathBuf::from(r"C:\ProgramData"), PathBuf::from);
            program_data.join("pley")
        }
    }
}

/// Prints each step before doing it, or instead of it for a dry run.
struct Steps {
    dry_run: bool,
}

impl Steps {
    fn run<T: Default>(&self, what: impl Display, step: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<T> {
        if self.dry_run {
            println!("would {what}");
            return Ok(T::default());
        }
        println!("{what}");
        step().with_context(|| what.to_string())
    }
}

fn install(cli: &Cli, args: &InstallArgs) -> anyhow::Result<()> {
    anyhow::ensure!(cli.config.is_none(), "the service reads pley.toml in its data folder: leave out --config");
    let exe = std::env::current_exe().context("locating pley.exe")?;
    let data_dir = data_dir(cli);
    check_data_dir(&data_dir)?;
    check_owner(&data_dir)?;
    let steps = Steps { dry_run: args.dry_run };

    let (account, password) = match args.account {
        Account::Virtual => {
            anyhow::ensure!(args.share.is_none(), "--share needs --account user: the virtual account has no sign-ins");
            (format!(r"NT SERVICE\{NAME}"), None)
        }
        Account::User => {
            let password = match std::env::var(&args.password_env) {
                Ok(password) if !password.is_empty() => password,
                // Made here, and set again by each install: nobody signs in as it.
                _ if args.create_user => accounts::random_password(),
                _ => anyhow::bail!("--account user needs the user's password in {}", args.password_env),
            };
            let user = &args.user;
            if args.create_user {
                steps.run(format!("create the local user {user}, or set its password"), || {
                    accounts::create_or_update_user(user, &password)
                })?;
            }
            steps.run(format!("let {user} run services"), || accounts::grant_right(user, "SeServiceLogonRight"))?;
            (format!(r".\{user}"), Some(password))
        }
    };

    steps.run("stop the Pley service if it runs", || scm::stop(SERVICE_TIMEOUT))?;

    let temp = data_dir.join("tmp");
    steps.run(format!("create {}", data_dir.display()), || {
        if !data_dir.exists() {
            if let Some(parent) = data_dir.parent() {
                std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
            }
            accounts::create_private_folder(&data_dir)?;
        }
        for dir in [&data_dir.join("logs"), &temp] {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    })?;
    let definition = scm::Definition {
        command_line: format!(
            "{} --data-dir {} service run",
            crate::desktop::autostart::windows_quote(&exe.to_string_lossy()),
            crate::desktop::autostart::windows_quote(&data_dir.to_string_lossy()),
        ),
        account,
        password: password.clone(),
        restricted: !args.unrestricted,
        // Its own temporary folder: a restricted service can't write in its
        // profile's, and others' are no place for its files.
        environment: vec![format!("TEMP={}", temp.display()), format!("TMP={}", temp.display())],
    };
    let restricted = if args.unrestricted { "" } else { ", writing only in its data folder" };
    steps
        .run(format!("install the Pley service as {}{restricted}", definition.account), || scm::install(&definition))?;

    // Once Windows knows the service's account (icacls refuses SIDs it can't
    // name), and before anything is copied or saved in the folder: from
    // ProgramData, it inherits read access for every user.
    let mut allowed = vec![(accounts::service_sid(NAME), "M")];
    if args.account == Account::User {
        allowed.push((steps.run(format!("look up {}", args.user), || accounts::local_user_sid(&args.user))?, "M"));
    }
    for account in &args.grant {
        allowed.push((steps.run(format!("look up {account}"), || accounts::sid(account))?, "F"));
    }
    steps.run(format!("let only {} into {}", allowed_in(&args.grant), data_dir.display()), || {
        if accounts::protect_folder(&data_dir, &allowed, args.reset_permissions)? {
            // Nothing was slipped in on the way (icacls follows junctions).
            check_data_dir(&data_dir)?;
        }
        Ok(())
    })?;
    if let Some(from) = &args.copy_data_from {
        steps.run(format!("copy the data in {} into {}", from.display(), data_dir.display()), || {
            copy_data(from, &data_dir)
        })?;
    }
    let config_file = data_dir.join("pley.toml");
    steps.run(format!("save the options in {}", config_file.display()), || {
        crate::config::save_config(cli, &config_file, args.discovery)
    })?;
    // Found by another account, or before the service was restricted: the
    // service finds out what FFmpeg can do itself.
    let capabilities = data_dir.join("transcode").join("capabilities.json");
    if capabilities.exists() {
        steps.run("forget what FFmpeg could do before", || Ok(std::fs::remove_file(&capabilities)?))?;
    }

    if let Some(old) = &args.remove_user {
        let current = args.account == Account::User && old.eq_ignore_ascii_case(&args.user);
        anyhow::ensure!(!current, "--remove-user {old}: the service runs as {old}");
        steps.run(format!("delete the local user {old} and its profile"), || accounts::delete_user(old))?;
    }

    if let Some(share) = &args.share {
        let server = share.trim_start_matches(['\\', '/']).split(['\\', '/']).next().unwrap_or_default();
        anyhow::ensure!(!server.is_empty(), "--share needs a server, such as \\\\nas");
        let share_user = args.share_user.as_deref().context("--share needs --share-user")?;
        let share_password = std::env::var(&args.share_password_env)
            .with_context(|| format!("--share needs the share's password in {}", args.share_password_env))?;
        let password = password.as_deref().unwrap_or_default();
        steps.run(format!("save {}'s sign-in to \\\\{server} as {share_user}", args.user), || {
            accounts::store_credential(&exe, &args.user, password, server, share_user, &share_password)
        })?;
    }

    if !args.no_start {
        steps.run("start the Pley service", || start(&data_dir))?;
    }
    Ok(())
}

/// "the service, administrators, SYSTEM and <accounts>".
fn allowed_in(accounts: &[String]) -> String {
    let mut names: Vec<&str> = vec!["the service", "administrators", "SYSTEM"];
    names.extend(accounts.iter().map(String::as_str));
    let last = names.pop().unwrap_or_default();
    format!("{} and {last}", names.join(", "))
}

fn uninstall(args: &UninstallArgs) -> anyhow::Result<()> {
    let steps = Steps { dry_run: args.dry_run };
    steps.run("stop and remove the Pley service", scm::uninstall)?;
    if let Some(user) = &args.delete_user {
        steps.run(format!("delete the local user {user} and its profile"), || accounts::delete_user(user))?;
    }
    Ok(())
}

/// What a data folder holds that isn't copied: the lock, the log and work
/// files of the pley that used it.
const NOT_COPIED: &[&str] = &["pley.lock", "logs", "transcode", "tmp"];

/// Copies pley's data in `from` (its database, key, artwork, backups…) into
/// `to`, which has no database yet. The pley using `from` must be stopped:
/// its lock is taken while copying, so that the database is copied whole.
fn copy_data(from: &Path, to: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(from.join("pley.db").is_file(), "{} isn't a Pley data folder (it has no pley.db)", from.display());
    anyhow::ensure!(!to.join("pley.db").exists(), "{} has a database already: nothing is copied over it", to.display());
    let _lock = crate::instance::InstanceLock::acquire(from).context("stop the Pley using it first")?;
    let (mut files, mut bytes) = (0u64, 0u64);
    for entry in walkdir::WalkDir::new(from).min_depth(1) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(from)?;
        let top = relative.components().next().map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase());
        if top.is_some_and(|top| NOT_COPIED.contains(&top.as_str())) {
            continue;
        }
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).with_context(|| format!("creating {}", target.display()))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            bytes +=
                std::fs::copy(entry.path(), &target).with_context(|| format!("copying {}", entry.path().display()))?;
            files += 1;
        }
    }
    println!("copied {files} files ({} MB)", bytes / 1_000_000);
    Ok(())
}

/// Folders whose permissions must not be replaced are refused: a drive's
/// root, a folder with other things in it than pley's, and links: a
/// junction made in ProgramData before pley was installed would lead its
/// changes (as an administrator) anywhere.
/// The data folder, when it is there already, must be the administrators'
/// (or SYSTEM's): one an ordinary user made beforehand in ProgramData stays
/// theirs to open up again, however it is locked down, and with it
/// pley.toml (the FFmpeg the service runs), its database and its log.
fn check_owner(dir: &Path) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let owner = accounts::owner(dir)?;
    anyhow::ensure!(
        matches!(owner.as_str(), "BA" | "SY" | "S-1-5-32-544" | "S-1-5-18"),
        "{} belongs to {owner}, not to the administrators: whoever owns it could open it up again. \
         Choose a new folder (Setup can copy the data into it), or give it to the administrators first: \
         takeown /a /r /d y /f \"{}\"",
        dir.display(),
        dir.display()
    );
    Ok(())
}

fn check_data_dir(dir: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(dir.parent().is_some(), "{} is a drive: choose a folder in it", dir.display());
    if dir.exists() {
        for entry in walkdir::WalkDir::new(dir).follow_links(false) {
            let entry = entry?;
            anyhow::ensure!(
                !is_link(entry.path()),
                "{} is a link or junction: Pley's data folder and what is in it must be ordinary folders and files",
                entry.path().display()
            );
        }
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        let names: Vec<String> =
            entries.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().to_ascii_lowercase()).collect();
        // Its data, or the folders an install that stopped short made.
        let pley = ["pley.db", "pley.toml", "secret.key"].iter().any(|file| dir.join(file).exists());
        let started = names.iter().all(|name| name == "logs" || name == "tmp");
        anyhow::ensure!(
            pley || started,
            "{} has other files in it than Pley's: choose an empty or a new folder for Pley's data",
            dir.display()
        );
    }
    Ok(())
}

/// Starts the service and reports on it (`report`); why it couldn't start,
/// from its log, when it stops at once.
fn start(data_dir: &Path) -> anyhow::Result<()> {
    let since = crate::util::now();
    if let Err(err) = scm::start(SERVICE_TIMEOUT) {
        let log = std::fs::read(data_dir.join("logs").join("pley.log")).unwrap_or_default();
        return Err(start_error(&String::from_utf8_lossy(&log), since).map_or(err, anyhow::Error::msg));
    }
    report(data_dir, since)
}

/// A symbolic link, junction or other reparse point (not followed).
fn is_link(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// Once the pley serving `data_dir` that started at unix time `since` or
/// later answers: its address, whether it has accounts already or else the
/// link that sets it up, and what it found FFmpeg can do, in lines the
/// installer reads (`running: `, `accounts: yes`, `setup: `, `ffmpeg: `).
fn report(data_dir: &Path, since: i64) -> anyhow::Result<()> {
    let log_file = data_dir.join("logs").join("pley.log");
    let read_log = || String::from_utf8_lossy(&std::fs::read(&log_file).unwrap_or_default()).into_owned();
    // The address is the one its start logged (pley.toml may not have it).
    let deadline = Instant::now() + SERVICE_TIMEOUT;
    let addr = loop {
        let log = read_log();
        if let Some(addr) = since_start(&log, since).and_then(listening_on) {
            break addr;
        }
        if let Some(error) = start_error(&log, since) {
            anyhow::bail!("{error} (its log is {})", log_file.display());
        }
        anyhow::ensure!(Instant::now() < deadline, "Pley didn't start: its log is {}", log_file.display());
        std::thread::sleep(Duration::from_millis(500));
    };
    while !answers(addr) {
        anyhow::ensure!(Instant::now() < deadline, "Pley didn't answer on {addr}: its log is {}", log_file.display());
        std::thread::sleep(Duration::from_millis(500));
    }
    println!("running: http://{addr}");
    // Set up already (its data copied from a pley that was): no setup code.
    if get(addr, "/api/server").is_some_and(|body| body.contains(r#""setup_required":false"#)) {
        println!("accounts: yes");
    }
    let link = |code: &str| format!("http://{addr}/setup?code={code}");
    // The setup code is logged as pley starts; what FFmpeg can do comes
    // once the transcoder has tried it (seconds, with a GPU).
    let deadline = Instant::now() + Duration::from_secs(45);
    let (mut setup, mut ffmpeg) = (None, None);
    loop {
        let log = read_log();
        if let Some(run) = since_start(&log, since) {
            setup = setup.or_else(|| setup_code(run));
            ffmpeg = ffmpeg.or_else(|| ffmpeg_line(run));
        }
        if ffmpeg.is_some() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Some(code) = setup {
        println!("setup: {}", link(&code));
    }
    if let Some(line) = ffmpeg {
        println!("ffmpeg: {line}");
    }
    Ok(())
}

/// Whether pley answers `/api/health` on `addr`.
fn answers(addr: SocketAddr) -> bool {
    get(addr, "/api/health").is_some()
}

/// The body of `path` on `addr`, when pley answers it with 200.
fn get(addr: SocketAddr, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let response = String::from_utf8_lossy(&response);
    let (head, body) = response.split_once("\r\n\r\n")?;
    head.lines().next()?.contains(" 200 ").then(|| body.to_owned())
}

/// How the log writes unix time `secs` (UTC, to the second), at the start
/// of its lines.
fn log_stamp(secs: i64) -> String {
    let [year, month, day, hour, minute, second] = crate::util::utc(secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

/// What the log says from pley's last start on, if that was at unix time
/// `since` or later. Each start logs `Pley <version> listening on …`
/// (`pley …` in older logs), with the time in UTC at the start of its line.
fn since_start(log: &str, since: i64) -> Option<&str> {
    let stamp = log_stamp(since);
    let (mut offset, mut start) = (0, None);
    for line in log.split_inclusive('\n') {
        let started =
            (line.contains(" INFO Pley ") || line.contains(" INFO pley ")) && line.contains(" listening on http://");
        if started && line.get(..stamp.len()).is_some_and(|time| time >= stamp.as_str()) {
            start = Some(offset);
        }
        offset += line.len();
    }
    start.map(|start| &log[start..])
}

/// Why pley couldn't start at unix time `since` or later, as it logged it
/// (its port is taken, say).
fn start_error(log: &str, since: i64) -> Option<String> {
    let stamp = log_stamp(since);
    log.lines().rev().find_map(|line| {
        let error = line
            .split(" ERROR Pley could not start: ")
            .nth(1)
            .or_else(|| line.split(" ERROR pley could not start: ").nth(1))?;
        (line.get(..stamp.len())? >= stamp.as_str()).then(|| format!("Pley could not start: {}", error.trim()))
    })
}

/// Where the run whose log is `run` listens, from its first line (`Pley …
/// listening on http://<addr>`; `localhost` when it listens everywhere).
fn listening_on(run: &str) -> Option<SocketAddr> {
    let first = run.lines().next()?;
    let addr = first.split("listening on http://").nth(1)?.split_whitespace().next()?;
    addr.replace("localhost", &Ipv4Addr::LOCALHOST.to_string()).parse().ok()
}

/// The setup code in the log, which `crate::run_with_tray` logs while the
/// server has no account (`setup?code=`, or `setup#code=` before 0.2).
fn setup_code(log: &str) -> Option<String> {
    log.lines().rev().find_map(|line| {
        let code = line.split("setup?code=").nth(1).or_else(|| line.split("setup#code=").nth(1))?;
        let code: String = code.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        (!code.is_empty()).then_some(code)
    })
}

/// The transcoder's line on what FFmpeg can do, or that it isn't found.
fn ffmpeg_line(log: &str) -> Option<String> {
    log.lines().rev().find_map(|line| {
        let message = line.split("[transcoder] ").nth(1)?;
        message.starts_with("ffmpeg ").then(|| message.trim().to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_folder_someone_else_made_is_refused() {
        // Made by whoever runs the tests, not by the administrators: as one
        // an ordinary user made in ProgramData before pley was installed.
        let dir = tempfile::tempdir().unwrap();
        let owner = accounts::owner(dir.path()).unwrap();
        assert!(owner.starts_with("S-1-5-21-") || owner == "BA", "{owner}");
        if owner != "BA" {
            let err = check_owner(dir.path()).unwrap_err().to_string();
            assert!(err.contains("not to the administrators"), "{err}");
        }
        assert!(check_owner(&dir.path().join("new")).is_ok(), "a folder still to make");
    }

    #[test]
    fn a_folder_protected_for_the_same_accounts_is_left_as_it_is() {
        let is_protected = |current: &str, allowed: &[(String, &str)]| {
            let wanted = accounts::canonical(&accounts::protected_sddl(allowed).unwrap()).unwrap();
            accounts::same_security(current, &wanted)
        };
        let service = accounts::service_sid("pley");
        let me = "S-1-5-21-1-2-3-1001".to_owned();
        let allowed = [(service.clone(), "M"), (me.clone(), "F")];
        // As Windows reads one an earlier install protected (entries in any order).
        let protected =
            format!("O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{me})(A;OICI;0x1301bf;;;{service})");
        assert!(is_protected(&protected, &allowed));
        let reordered =
            format!("O:BAD:PAI(A;OICI;0x1301bf;;;{service})(A;OICI;FA;;;{me})(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)");
        assert!(is_protected(&reordered, &allowed));

        // Owned by someone else, inheriting, open to one more, or for other accounts: every file again.
        assert!(!is_protected(&protected.replace("O:BA", &format!("O:{me}")), &allowed));
        assert!(!is_protected(&protected.replace("D:PAI", "D:AI"), &allowed));
        assert!(!is_protected(&format!("{protected}(A;OICI;0x1200a9;;;BU)"), &allowed));
        assert!(!is_protected(&protected, &[(service.clone(), "M")]));
        assert!(!is_protected(&protected, &[(service.clone(), "F"), (me.clone(), "F")]));
        // A new folder, made private before any of this.
        assert!(!is_protected("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)", &allowed));
        // Access icacls' M and F don't name: every file again.
        assert!(accounts::protected_sddl(&[(service.clone(), "RX")]).is_none());

        // An account given twice has its last grant, as icacls leaves it.
        let twice = [(me.clone(), "M"), (me.clone(), "F")];
        assert!(is_protected(&format!("O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{me})"), &twice));
        // Windows writes some accounts by a name of its own (BU for the
        // Users group, LA for the built-in Administrator).
        let read = "O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;BU)";
        assert!(is_protected(read, &[("S-1-5-32-545".to_owned(), "F")]));
    }

    #[test]
    #[ignore = "needs an elevated shell: run with --ignored as an administrator"]
    fn a_protected_folder_reads_back_as_protected() {
        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("data");
        accounts::create_private_folder(&dir).unwrap();
        std::fs::write(dir.join("pley.db"), b"").unwrap();
        let me = accounts::sid(&std::env::var("USERNAME").unwrap()).unwrap();
        let allowed = [(accounts::service_sid("pley"), "M"), (me, "F")];
        assert!(accounts::protect_folder(&dir, &allowed, false).unwrap(), "a new folder: every file");
        assert!(!accounts::protect_folder(&dir, &allowed, false).unwrap(), "the same accounts: left as it is");
        assert!(accounts::protect_folder(&dir, &allowed[..1], false).unwrap(), "other accounts: every file");
        assert!(accounts::protect_folder(&dir, &allowed[..1], true).unwrap(), "asked for: every file");
    }

    #[test]
    fn the_service_account_has_the_sid_windows_gives_it() {
        // `sc showsid pley`
        assert_eq!(accounts::service_sid("pley"), "S-1-5-80-3186206774-272637008-2588855599-4121769540-1582281795");
        assert_eq!(accounts::service_sid("PLEY"), accounts::service_sid("pley"));
    }

    #[test]
    fn accounts_are_looked_up_and_only_pleys_own_user_is_touched() {
        let me = std::env::var("USERNAME").unwrap();
        assert!(accounts::sid(&me).unwrap().starts_with("S-1-5-"));
        assert!(accounts::sid("no-such-user-for-pley").is_err());
        let err = accounts::create_or_update_user(&me, "x").unwrap_err();
        assert!(format!("{err:#}").contains("not one Pley made"), "{err:#}");
        assert!(accounts::delete_user(&me).is_err(), "someone's own account stays");
        accounts::delete_user("no-such-user-for-pley").unwrap();
    }

    #[test]
    fn service_passwords_are_random_and_have_every_kind_of_character() {
        let (a, b) = (accounts::random_password(), accounts::random_password());
        assert_ne!(a, b);
        assert!(a.len() >= 32);
        for kind in
            [char::is_ascii_uppercase, char::is_ascii_lowercase, char::is_ascii_digit, char::is_ascii_punctuation]
        {
            assert!(a.chars().any(|c| kind(&c)), "{a}");
        }
    }

    #[test]
    fn data_is_copied_without_the_lock_logs_and_work_files_and_never_from_a_running_pley() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("old"), dir.path().join("new"));
        for file in [
            "pley.db",
            "pley.db-wal",
            "secret.key",
            "pley.lock",
            "logs/pley.log",
            "transcode/a.ts",
            "metadata/1/poster.jpg",
        ] {
            let path = from.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, file).unwrap();
        }
        {
            let _running = crate::instance::InstanceLock::acquire(&from).unwrap();
            assert!(copy_data(&from, &to).is_err(), "a pley uses it");
        }
        copy_data(&from, &to).unwrap();
        for file in ["pley.db", "pley.db-wal", "secret.key", "metadata/1/poster.jpg"] {
            assert_eq!(std::fs::read_to_string(to.join(file)).unwrap(), file);
        }
        for left in ["pley.lock", "logs", "transcode"] {
            assert!(!to.join(left).exists(), "{left}");
        }
        assert!(copy_data(&from, &to).is_err(), "never over a database");
        assert!(copy_data(&to.join("metadata"), &dir.path().join("x")).is_err(), "not a data folder");
    }

    #[test]
    fn drives_and_folders_of_other_things_are_refused() {
        assert!(check_data_dir(Path::new(r"C:\")).is_err());
        assert!(check_data_dir(Path::new(r"Z:\pley")).is_ok(), "a folder at a drive's root");
        let dir = tempfile::tempdir().unwrap();
        assert!(check_data_dir(dir.path()).is_ok(), "empty");
        assert!(check_data_dir(&dir.path().join("new")).is_ok(), "new");
        std::fs::create_dir(dir.path().join("logs")).unwrap();
        std::fs::create_dir(dir.path().join("tmp")).unwrap();
        assert!(check_data_dir(dir.path()).is_ok(), "what an install that stopped short made");
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        assert!(check_data_dir(dir.path()).is_err(), "someone else's");
        std::fs::write(dir.path().join("pley.db"), "").unwrap();
        assert!(check_data_dir(dir.path()).is_ok(), "pley's");

        // A junction (any user may make one) in it, or as it.
        let elsewhere = tempfile::tempdir().unwrap();
        let junction = dir.path().join("media");
        let made = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(elsewhere.path())
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stdout));
        assert!(check_data_dir(dir.path()).is_err(), "a junction in it");
        assert!(check_data_dir(&junction).is_err(), "a junction");
        std::fs::remove_dir(&junction).unwrap();
    }

    #[test]
    fn the_report_reads_the_log_from_the_last_start_on() {
        let log = "2026-10-26T18:00:00.1Z  INFO pley 0.1.0 listening on http://127.0.0.1:7539\n\
                   2026-10-26T18:00:01.2Z  INFO [transcoder] ffmpeg 7.1 (cached capabilities): encoders [libx264]\n\
                   2026-10-26T18:50:19.9Z  INFO Pley 0.1.0 listening on http://127.0.0.1:7539\n\
                   2026-10-26T18:50:20.0Z  INFO Plex-compatible API listening on http://127.0.0.1:32400\n\
                   2026-10-26T18:50:20.3Z  INFO [transcoder] pley-transcoder 0.1.0 listening on http://127.0.0.1:60494\n";
        // 2026-10-26T18:50:00Z
        let since = 1_792_972_800 + 18 * 3600 + 50 * 60;
        assert_eq!(crate::util::utc(since), [2026, 10, 26, 18, 50, 0]);
        let run = since_start(log, since).unwrap();
        assert!(run.starts_with("2026-10-26T18:50:19.9Z"), "{run}");
        assert_eq!(listening_on(run), Some("127.0.0.1:7539".parse().unwrap()));
        assert_eq!(
            listening_on("t  INFO Pley 1.0 listening on http://localhost:7600\n"),
            Some("127.0.0.1:7600".parse().unwrap())
        );
        assert_eq!(ffmpeg_line(run), None, "the earlier start's");
        assert_eq!(since_start(log, since + 60), None, "not started since");
        let older = "2026-10-26T18:50:19.9Z  INFO pley 0.1.0 listening on http://127.0.0.1:7539\n";
        assert_eq!(since_start(older, since), Some(older), "an older pley's start");

        let failed = "2026-10-26T18:00:00.1Z ERROR pley could not start: cannot listen on 127.0.0.1:7539: in use\n\
                      2026-10-26T18:50:30.0Z ERROR Pley could not start: cannot listen on 127.0.0.1:7539: in use\n";
        assert_eq!(
            start_error(failed, since).as_deref(),
            Some("Pley could not start: cannot listen on 127.0.0.1:7539: in use")
        );
        assert_eq!(start_error(failed, since + 60), None, "an earlier start's");
        let older = "2026-10-26T18:50:30.0Z ERROR pley could not start: cannot listen on 127.0.0.1:7539: in use\n";
        assert_eq!(
            start_error(older, since).as_deref(),
            Some("Pley could not start: cannot listen on 127.0.0.1:7539: in use"),
            "an older pley's"
        );
    }

    #[test]
    fn the_report_finds_the_setup_code_and_what_ffmpeg_can_do() {
        let log = "2026-10-26T10:00:00Z  WARN Pley isn't set up yet: open http://127.0.0.1:7539/setup?code=AB12-CD34 to \
                   create the administrator account (the setup code is AB12-CD34)\n\
                   2026-10-26T10:00:03Z  INFO [transcoder] ffmpeg 7.1 detected in 2.9s: encoders [hevc_nvenc], tone \
                   mapping [libplacebo], dolby vision: true\n";
        assert_eq!(setup_code(log).as_deref(), Some("AB12-CD34"));
        assert_eq!(
            ffmpeg_line(log).as_deref(),
            Some("ffmpeg 7.1 detected in 2.9s: encoders [hevc_nvenc], tone mapping [libplacebo], dolby vision: true")
        );
        assert_eq!(
            setup_code("WARN … open http://localhost:7539/setup#code=EF56-GH78 to …\n").as_deref(),
            Some("EF56-GH78")
        );
        assert_eq!(setup_code("INFO Pley 1.0 listening on http://127.0.0.1:7539\n"), None);
    }
}
