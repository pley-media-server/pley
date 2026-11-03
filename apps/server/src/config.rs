//! Static configuration: command line > environment > config file > defaults.
//!
//! Settings that admins change at runtime (TMDB key, hardware acceleration, …)
//! live in the database instead, see [`crate::settings`].

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use serde::Deserialize;

/// Default HTTP port ("PLEY" on a phone keypad).
pub const DEFAULT_PORT: u16 = 7539;
/// The port the Plex-compatible API takes when it has one of its own: Plex
/// Media Server's.
pub const PLEX_PORT: u16 = 32400;
/// The port the Jellyfin-compatible API takes when it has one of its own:
/// Jellyfin's, which its apps try for an address typed without a port.
pub const JELLYFIN_PORT: u16 = 8096;
/// How long requests in progress may take to finish once the server is told
/// to stop. Streams (a film playing directly) are cut after that. Short
/// enough to leave time for stopping the transcoder and closing the database
/// before Docker's default stop timeout (10 s) kills the process.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// How often an open event stream checks that its token still works.
pub const EVENT_RECHECK: Duration = Duration::from_secs(30);
/// How often an address the server can't listen on yet is tried again
/// (see [`Config::extra_binds`]).
pub const EXTRA_BIND_RETRY: Duration = Duration::from_secs(30);

#[derive(Debug, Parser)]
#[command(name = "pley", version = crate::VERSION_AND_COMMIT, about = "Pley — a fast, self-hosted media server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Path to a TOML config file (default: <data-dir>/pley.toml when it exists)
    #[arg(long, short, env = "PLEY_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    /// Directory holding the database, caches, artwork and transcodes
    #[arg(long, env = "PLEY_DATA_DIR", global = true)]
    pub data_dir: Option<PathBuf>,

    /// Folder the transcoder works in: stream segments, converted downloads, its caches
    /// (default: `transcode` in the data folder, which is used when this one can't be)
    #[arg(long, env = "PLEY_TRANSCODE_DIR", global = true)]
    pub transcode_dir: Option<PathBuf>,

    /// Address to listen on, e.g. `0.0.0.0:7539` or just a port; several separated by commas,
    /// such as `127.0.0.1:7539,192.168.1.10:7539` (one adapter's address, and this computer)
    #[arg(long, env = "PLEY_BIND", global = true)]
    pub bind: Option<String>,

    /// Where the Plex-compatible API listens: `web` (default) on Pley's own port, `off`, or
    /// addresses of its own (`0.0.0.0:32400`), several separated by commas
    #[arg(long, env = "PLEY_PLEX_BIND", global = true)]
    pub plex_bind: Option<String>,

    /// Where the Jellyfin-compatible API listens: `off` (default), `web` on Pley's own port, or
    /// addresses of its own (`0.0.0.0:8096`), several separated by commas
    #[arg(long, env = "PLEY_JELLYFIN_BIND", global = true)]
    pub jellyfin_bind: Option<String>,

    /// Path to the ffmpeg binary
    #[arg(long, env = "PLEY_FFMPEG", global = true)]
    pub ffmpeg: Option<PathBuf>,

    /// Path to the ffprobe binary
    #[arg(long, env = "PLEY_FFPROBE", global = true)]
    pub ffprobe: Option<PathBuf>,

    /// Serve the web UI from this directory instead of the copy embedded in the binary
    #[arg(long, env = "PLEY_WEB_DIR", global = true)]
    pub web_dir: Option<PathBuf>,

    /// pley-transcoder executable to spawn (default: next to the pley binary)
    #[arg(long, env = "PLEY_TRANSCODER_PATH", global = true)]
    pub transcoder_path: Option<PathBuf>,

    /// Use an already running transcoder at this URL instead of spawning one
    #[arg(long, env = "PLEY_TRANSCODER_URL", global = true)]
    pub transcoder_url: Option<String>,

    /// Shared secret for an external transcoder (`--transcoder-url`)
    #[arg(long, env = "PLEY_TRANSCODER_TOKEN", global = true, hide_env_values = true)]
    pub transcoder_token: Option<String>,

    /// Log filter, e.g. `info` or `debug,sqlx=warn`
    #[arg(long, env = "PLEY_LOG", default_value = "info", global = true)]
    pub log: String,

    /// Never show the tray icon (it only appears in interactive desktop sessions anyway)
    #[arg(long, env = "PLEY_NO_TRAY", global = true)]
    pub no_tray: bool,

    /// Don't answer the Pley apps looking for servers on the local network, nor announce Pley
    /// there by Bonjour
    #[arg(long, env = "PLEY_NO_DISCOVERY", global = true)]
    pub no_discovery: bool,

    /// Don't announce Pley on the local network by Bonjour (Apple's apps then need its address
    /// typed in, and the web app has no .local name)
    #[arg(long, env = "PLEY_NO_BONJOUR", global = true)]
    pub no_bonjour: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the media server (default)
    Serve,
    /// Set a user's password, creating the user if it does not exist
    ResetPassword {
        username: String,
        /// The new password; asked for when left out (or read from standard
        /// input), so it isn't kept in the shell's history or shown in the
        /// list of processes
        password: Option<String>,
        /// Grant administrator rights
        #[arg(long)]
        admin: bool,
    },
    /// Exit with status 0 when the server on this machine answers (for container health checks)
    Health,
    /// Save the options given before it (--bind, --plex-bind, --jellyfin-bind, --transcode-dir,
    /// --ffmpeg, --ffprobe, --no-discovery, --no-bonjour) in pley.toml in the data folder, for every
    /// later start
    SaveConfig {
        /// Whether to answer the Pley apps looking for servers on the local network
        #[arg(long, value_name = "true|false")]
        discovery: Option<bool>,
    },
    /// Run Pley as a Windows service: install, start, stop or remove it
    #[cfg(windows)]
    #[command(subcommand)]
    Service(crate::service::ServiceCommand),
}

/// Shape of `pley.toml`. Every key is optional.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    bind: Option<String>,
    plex_bind: Option<String>,
    jellyfin_bind: Option<String>,
    data_dir: Option<PathBuf>,
    transcode_dir: Option<PathBuf>,
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
    web_dir: Option<PathBuf>,
    transcoder_path: Option<PathBuf>,
    transcoder_url: Option<String>,
    transcoder_token: Option<String>,
    transcoder_max_jobs: Option<u32>,
    discovery: Option<bool>,
    /// Off when left out: Bonjour tells every device on the local network
    /// about the server, so an administrator turns it on.
    bonjour: Option<bool>,
    bonjour_adapters: Option<Vec<String>>,
    bonjour_host_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Where the server listens, which it must be able to at startup.
    pub bind: SocketAddr,
    /// Where it listens too, once it can: a network adapter's address may
    /// come after pley (Wi-Fi, a VPN like Tailscale).
    pub extra_binds: Vec<SocketAddr>,
    /// Plex-compatible API listener on a port of its own (`None`: none).
    pub plex_bind: Option<SocketAddr>,
    /// Where that API listens too, as [`Config::extra_binds`].
    pub plex_extra_binds: Vec<SocketAddr>,
    /// That API answers on pley's own port (`plex_bind = "web"`, the
    /// default; see [`crate::plex::on_web_port`]).
    pub plex_on_web: bool,
    /// Jellyfin-compatible API listener on a port of its own (`None`: none).
    pub jellyfin_bind: Option<SocketAddr>,
    /// Where that API listens too, as [`Config::extra_binds`].
    pub jellyfin_extra_binds: Vec<SocketAddr>,
    /// That API answers on pley's own port (`jellyfin_bind = "web"`, see
    /// [`crate::jellyfin::on_web_port`]).
    pub jellyfin_on_web: bool,
    /// What Pley left out of `pley.toml` as it read it, said once in the log
    /// (which starts after the file is read).
    pub file_warnings: Vec<String>,
    pub data_dir: PathBuf,
    /// Work directory handed to the managed transcoder (segments, caches):
    /// the one chosen, or [`Config::default_transcode_dir`] when it can't
    /// be used (see [`Config::check_transcode_dir`]).
    pub transcode_dir: PathBuf,
    /// The transcode folder chosen, when it isn't the one in use, and why.
    pub transcode_dir_problem: Option<(PathBuf, String)>,
    /// The command line or the environment chooses the transcode folder:
    /// the web app can't change it.
    pub transcode_dir_fixed: bool,
    /// ffmpeg/ffprobe are only passed through to the managed transcoder.
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub web_dir: Option<PathBuf>,
    pub transcoder_path: Option<PathBuf>,
    pub transcoder_url: Option<String>,
    pub transcoder_token: Option<String>,
    pub transcoder_max_jobs: u32,
    /// Where TVmaze, TheTVDB, MusicBrainz and the Cover Art Archive are
    /// reached. `None` (tests) never contacts them.
    pub metadata_endpoints: Option<crate::providers::Endpoints>,
    /// See [`SHUTDOWN_GRACE`] (tests shorten it).
    pub shutdown_grace: Duration,
    /// Answer apps looking for servers on the local network (see
    /// [`crate::discovery`]).
    pub discovery: bool,
    /// Announce pley on the local network by Bonjour (see [`crate::bonjour`]).
    pub bonjour: bool,
    /// The adapters Bonjour announces from, by name: any when empty.
    pub bonjour_adapters: Vec<String>,
    /// Bonjour's host name, a DNS label: the generated one when empty.
    pub bonjour_host_name: String,
    /// See [`EVENT_RECHECK`] (tests shorten it).
    pub event_recheck: Duration,
    /// See [`EXTRA_BIND_RETRY`] (tests shorten it).
    pub extra_bind_retry: Duration,
    /// Where the client metadata documents of other publishers' apps come
    /// from: the internet, or nowhere in tests (which may also give fixed
    /// ones). pley's own apps are built in either way.
    pub client_documents: crate::oauth::fetch::Source,
    /// How long seek previews' worker waits between its steps (tests shorten it).
    pub trickplay_pace: crate::trickplay::Pace,
    /// Where media files are read from, for seek previews: the system's
    /// disks, or fixed ones in tests.
    pub trickplay_disks: crate::disks::Source,
    /// The config file given, or `pley.toml` in the data directory: where
    /// the web app saves where pley listens (see [`crate::network`]).
    pub config_file: PathBuf,
    /// Where pley listens, as far as the command line or the environment
    /// sets it: the web app can't change that.
    pub network_fixed: crate::network::Fixed,
}

impl Config {
    pub fn load(cli: &Cli) -> anyhow::Result<Self> {
        let default_data_dir = cli.data_dir.clone().unwrap_or_else(default_data_dir);
        let config_file = absolutize(&cli.config.clone().unwrap_or_else(|| default_data_dir.join("pley.toml")));
        let config_path = (cli.config.is_some() || config_file.exists()).then(|| config_file.clone());
        let file: FileConfig = match &config_path {
            Some(path) => {
                let text =
                    std::fs::read_to_string(path).with_context(|| format!("reading config file {}", path.display()))?;
                toml::from_str(&text).with_context(|| format!("parsing config file {}", path.display()))?
            }
            None => FileConfig::default(),
        };

        let data_dir = cli.data_dir.clone().or(file.data_dir).unwrap_or(default_data_dir);
        let data_dir = absolutize(&data_dir);
        let binds = match cli.bind.clone().or(file.bind) {
            None => vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT)],
            Some(value) => parse_binds(&value)?,
        };
        // By default on pley's own port: wherever pley is reached, through
        // its reverse proxy's HTTPS too.
        let plex_bind = cli.plex_bind.clone().or(file.plex_bind);
        let plex_on_web = plex_bind.as_deref().is_none_or(is_web);
        let plex_binds = match plex_bind {
            Some(value) if !is_off(&value) && !is_web(&value) => parse_binds(&value)?,
            _ => Vec::new(),
        };
        // Off unless an administrator turns it on.
        let jellyfin_bind = cli.jellyfin_bind.clone().or(file.jellyfin_bind);
        let jellyfin_on_web = jellyfin_bind.as_deref().is_some_and(is_web);
        let jellyfin_binds = match jellyfin_bind {
            Some(value) if !is_off(&value) && !is_web(&value) => parse_binds(&value)?,
            _ => Vec::new(),
        };
        let mut file_warnings = Vec::new();
        let bonjour_adapters = bonjour_adapters(file.bonjour_adapters.unwrap_or_default(), &mut file_warnings);
        let bonjour_host_name = bonjour_host_name(&file.bonjour_host_name.unwrap_or_default(), &mut file_warnings);
        let transcode_dir = cli
            .transcode_dir
            .clone()
            .or(file.transcode_dir)
            .map(|p| absolutize(&p))
            .unwrap_or_else(|| data_dir.join("transcode"));

        Ok(Self {
            bind: binds[0],
            extra_binds: binds[1..].to_vec(),
            plex_bind: plex_binds.first().copied(),
            plex_extra_binds: plex_binds.iter().skip(1).copied().collect(),
            plex_on_web,
            jellyfin_bind: jellyfin_binds.first().copied(),
            jellyfin_extra_binds: jellyfin_binds.iter().skip(1).copied().collect(),
            jellyfin_on_web,
            file_warnings,
            ffmpeg: cli.ffmpeg.clone().or(file.ffmpeg).unwrap_or_else(|| PathBuf::from("ffmpeg")),
            ffprobe: cli.ffprobe.clone().or(file.ffprobe).unwrap_or_else(|| PathBuf::from("ffprobe")),
            web_dir: cli.web_dir.clone().or(file.web_dir),
            transcoder_path: cli.transcoder_path.clone().or(file.transcoder_path),
            transcoder_url: cli.transcoder_url.clone().or(file.transcoder_url).filter(|u| !u.trim().is_empty()),
            transcoder_token: cli.transcoder_token.clone().or(file.transcoder_token),
            transcoder_max_jobs: file.transcoder_max_jobs.unwrap_or(8).max(1),
            metadata_endpoints: Some(crate::providers::Endpoints::default()),
            shutdown_grace: SHUTDOWN_GRACE,
            discovery: !cli.no_discovery && file.discovery.unwrap_or(true),
            bonjour: !cli.no_discovery && !cli.no_bonjour && file.bonjour.unwrap_or(false),
            bonjour_adapters,
            bonjour_host_name,
            event_recheck: EVENT_RECHECK,
            extra_bind_retry: EXTRA_BIND_RETRY,
            client_documents: crate::oauth::fetch::Source::Internet,
            trickplay_pace: crate::trickplay::Pace::default(),
            trickplay_disks: crate::disks::Source::System,
            network_fixed: crate::network::Fixed {
                web: cli.bind.is_some(),
                plex: cli.plex_bind.is_some(),
                jellyfin: cli.jellyfin_bind.is_some(),
                discovery: cli.no_discovery,
                bonjour: cli.no_discovery || cli.no_bonjour,
            },
            config_file,
            data_dir,
            transcode_dir,
            transcode_dir_problem: None,
            transcode_dir_fixed: cli.transcode_dir.is_some(),
        })
    }

    /// Configuration rooted in `data_dir`, used by tests. Metadata providers
    /// that need no key are not contacted.
    pub fn for_data_dir(data_dir: &Path) -> Self {
        Self {
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            extra_binds: Vec::new(),
            plex_bind: None,
            plex_extra_binds: Vec::new(),
            plex_on_web: true,
            jellyfin_bind: None,
            jellyfin_extra_binds: Vec::new(),
            jellyfin_on_web: false,
            file_warnings: Vec::new(),
            data_dir: data_dir.to_path_buf(),
            transcode_dir: data_dir.join("transcode"),
            transcode_dir_problem: None,
            transcode_dir_fixed: false,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            web_dir: None,
            transcoder_path: None,
            transcoder_url: None,
            transcoder_token: None,
            transcoder_max_jobs: 8,
            metadata_endpoints: None,
            shutdown_grace: SHUTDOWN_GRACE,
            discovery: false,
            bonjour: false,
            bonjour_adapters: Vec::new(),
            bonjour_host_name: String::new(),
            event_recheck: EVENT_RECHECK,
            extra_bind_retry: EXTRA_BIND_RETRY,
            client_documents: crate::oauth::fetch::Source::Off,
            trickplay_pace: crate::trickplay::Pace::default(),
            trickplay_disks: crate::disks::Source::System,
            config_file: data_dir.join("pley.toml"),
            network_fixed: crate::network::Fixed::default(),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("pley.db")
    }

    /// The transcode folder when none is chosen, or the one chosen can't be
    /// used: `transcode` in the data folder.
    pub fn default_transcode_dir(&self) -> PathBuf {
        self.data_dir.join("transcode")
    }

    /// Falls back to [`Config::default_transcode_dir`] when the transcode
    /// folder chosen can't be made or written to (a drive that isn't
    /// there, a share without the right to write), saying why in
    /// `transcode_dir_problem`: transcodes and downloads go on working.
    pub fn check_transcode_dir(&mut self) {
        let default = self.default_transcode_dir();
        if self.transcode_dir == default {
            return;
        }
        if let Err(problem) = usable_folder(&self.transcode_dir) {
            tracing::warn!(
                "the transcode folder {} can't be used ({problem}): {} is, until it can",
                self.transcode_dir.display(),
                default.display()
            );
            self.transcode_dir_problem = Some((std::mem::replace(&mut self.transcode_dir, default), problem));
        }
    }

    /// Server log, rotated by size (see [`crate::logging`]).
    pub fn log_file(&self) -> PathBuf {
        log_file(&self.data_dir)
    }

    /// The server log when the configuration can't be loaded: in the data
    /// directory of the command line, or the default one (where `pley.toml`
    /// is looked for).
    pub fn fallback_log_file(cli: &Cli) -> PathBuf {
        log_file(&absolutize(&cli.data_dir.clone().unwrap_or_else(default_data_dir)))
    }

    /// Resized images, extracted subtitles, …: safe to delete at any time.
    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }

    /// Downloaded / generated artwork referenced from the database.
    pub fn metadata_dir(&self) -> PathBuf {
        self.data_dir.join("metadata")
    }

    /// Files generated from media files, by fingerprint (see [`crate::store`]).
    pub fn media_dir(&self) -> PathBuf {
        self.data_dir.join("media")
    }

    /// Subtitles people added, by media file (see [`crate::subtitles`]).
    pub fn added_subtitles_dir(&self) -> PathBuf {
        self.data_dir.join("subtitles")
    }
}

/// Bonjour's host name as `pley.toml` has it, as Settings → Network saves
/// it. One that can't be a host name is left out, for the generated one,
/// and said in `warnings`, rather than stop Pley from starting.
fn bonjour_host_name(typed: &str, warnings: &mut Vec<String>) -> String {
    let label = crate::bonjour::host_label(typed);
    if crate::bonjour::host_label_ok(&label) {
        return label;
    }
    warnings.push(format!(
        "bonjour_host_name: {typed:?} can't be a host name (letters, digits and hyphens), and the generated one is used"
    ));
    String::new()
}

/// The Bonjour adapters `pley.toml` lists, as Settings → Network saves
/// them: trimmed, sorted, each once, 32 at most. One written by hand that no
/// adapter could have (blank, control characters) is left out, and said in
/// `warnings`, rather than stop Pley from starting.
fn bonjour_adapters(listed: Vec<String>, warnings: &mut Vec<String>) -> Vec<String> {
    let mut adapters: Vec<String> = listed
        .into_iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| {
            let ok = crate::bonjour::adapter_name_ok(name);
            if !ok {
                warnings.push(format!("bonjour_adapters: {name:?} isn't a network adapter's name, and is left out"));
            }
            ok
        })
        .collect();
    adapters.sort();
    adapters.dedup();
    if adapters.len() > crate::network::MAX_ADDRESSES {
        warnings.push(format!("bonjour_adapters: {} at most, the rest left out", crate::network::MAX_ADDRESSES));
        adapters.truncate(crate::network::MAX_ADDRESSES);
    }
    adapters
}

impl Cli {
    /// The data directory of the command line, or the default one.
    pub fn data_dir_or_default(&self) -> PathBuf {
        absolutize(&self.data_dir.clone().unwrap_or_else(default_data_dir))
    }
}

/// `pley save-config`: the options given (on the command line or in the
/// environment) written into the config file `path`, whose other lines and
/// comments stay. `discovery` overrides `--no-discovery`; either turned off
/// saves Bonjour off too, as `--no-bonjour` does.
pub fn save_config(cli: &Cli, path: &Path, discovery: Option<bool>) -> anyhow::Result<()> {
    let mut values: Vec<(&str, toml::Value)> = Vec::new();
    if let Some(bind) = &cli.bind {
        parse_binds(bind)?;
        values.push(("bind", bind.trim().into()));
    }
    for (key, value) in [("plex_bind", &cli.plex_bind), ("jellyfin_bind", &cli.jellyfin_bind)] {
        if let Some(value) = value {
            if !is_off(value) && !is_web(value) {
                parse_binds(value)?;
            }
            values.push((key, value.trim().into()));
        }
    }
    for (key, path) in [("transcode_dir", &cli.transcode_dir), ("ffmpeg", &cli.ffmpeg), ("ffprobe", &cli.ffprobe)] {
        if let Some(path) = path {
            values.push((key, path.to_string_lossy().into_owned().into()));
        }
    }
    if let Some(discovery) = discovery.or(cli.no_discovery.then_some(false)) {
        values.push(("discovery", discovery.into()));
    }
    // Discovery turned off (Windows Setup's checkbox) turns Bonjour off too;
    // turned on, it leaves Bonjour as Settings → Network has it, since Setup
    // saves its checkbox again on every update.
    if cli.no_bonjour || discovery == Some(false) || (discovery.is_none() && cli.no_discovery) {
        values.push(("bonjour", false.into()));
    }
    write_keys(path, &values)
}

/// Saves where pley listens (Settings → Network, see [`crate::network`]) in
/// the config file `path`, but what `fixed` says the command line or the
/// environment sets.
pub fn save_listen(path: &Path, listen: &crate::network::Listen, fixed: crate::network::Fixed) -> anyhow::Result<()> {
    let list = |addrs: &[SocketAddr]| addrs.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    let mut values: Vec<(&str, toml::Value)> = Vec::new();
    if !fixed.web {
        values.push(("bind", list(&listen.web).into()));
    }
    let api = |on_web: bool, addrs: &[SocketAddr]| {
        if on_web {
            "web".to_owned()
        } else if addrs.is_empty() {
            "off".to_owned()
        } else {
            list(addrs)
        }
    };
    if !fixed.plex {
        values.push(("plex_bind", api(listen.plex_on_web, &listen.plex).into()));
    }
    if !fixed.jellyfin {
        values.push(("jellyfin_bind", api(listen.jellyfin_on_web, &listen.jellyfin).into()));
    }
    if !fixed.discovery {
        values.push(("discovery", listen.discovery.into()));
    }
    if !fixed.bonjour {
        values.push(("bonjour", listen.bonjour.into()));
    }
    let adapters: Vec<toml::Value> = listen.bonjour_adapters.iter().map(|name| name.as_str().into()).collect();
    values.push(("bonjour_adapters", adapters.into()));
    values.push(("bonjour_host_name", listen.bonjour_host_name.as_str().into()));
    write_keys(path, &values)
}

/// Saves the transcode folder chosen in Settings → Transcoder in the config
/// file `path`, for pley's next start; `None` goes back to the default one.
pub fn save_transcode_dir(path: &Path, dir: Option<&Path>) -> anyhow::Result<()> {
    match dir {
        Some(dir) => write_keys(path, &[("transcode_dir", dir.to_string_lossy().into_owned().into())]),
        None => {
            let text = read_config_text(path)?;
            let kept: Vec<&str> = text.lines().filter(|line| !defines(line, "transcode_dir")).collect();
            if kept.len() == text.lines().count() {
                return Ok(());
            }
            replace_config(path, kept.join("\n") + "\n")
        }
    }
}

/// The transcode folder the config file `path` chooses: pley's next start
/// uses it (unless the command line or the environment says otherwise).
pub fn saved_transcode_dir(path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str::<FileConfig>(&text).ok()?.transcode_dir.map(|dir| absolutize(&dir))
}

/// Whether pley can make files in the folder `dir` (made if missing): why
/// not, if it can't.
pub fn usable_folder(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let probe = dir.join(format!(".pley-{}", crate::auth::new_token()));
    std::fs::write(&probe, b"").map_err(|err| err.to_string())?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

fn read_config_text(path: &Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err).with_context(|| format!("reading config file {}", path.display())),
    }
}

/// Sets `values` in the config file `path` (made if missing), whose other
/// lines and comments stay.
fn write_keys(path: &Path, values: &[(&str, toml::Value)]) -> anyhow::Result<()> {
    let text = set_keys(&read_config_text(path)?, values);
    replace_config(path, text)
}

/// Replaces the config file `path` with `text`, once it is known to be
/// valid. The file is replaced whole: pley never reads half of it.
fn replace_config(path: &Path, text: String) -> anyhow::Result<()> {
    toml::from_str::<FileConfig>(&text).with_context(|| format!("checking the new {}", path.display()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let new = path.with_extension("toml.new");
    std::fs::write(&new, text).with_context(|| format!("writing config file {}", new.display()))?;
    std::fs::rename(&new, path).with_context(|| format!("replacing config file {}", path.display()))
}

/// `text` with each key of `values` set: on its line, or a new one at the
/// end. pley.toml has top-level keys only, so the end is never in a table.
fn set_keys(text: &str, values: &[(&str, toml::Value)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    for (key, value) in values {
        let line = format!("{key} = {value}");
        let mut found = false;
        // The lines an array written by hand goes on over, after its key's.
        let mut open = 0;
        // A second line for the key would make the file invalid.
        lines.retain_mut(|existing| {
            if open > 0 {
                open += brackets(existing);
                return false;
            }
            if !defines(existing, key) {
                return true;
            }
            open = brackets(existing).max(0);
            if found {
                return false;
            }
            existing.clone_from(&line);
            found = true;
            true
        });
        if !found {
            lines.push(line);
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// How many more brackets `line` opens than it closes, outside strings and
/// comments: an array that goes on over the next lines.
fn brackets(line: &str) -> i32 {
    let mut depth = 0;
    let mut quote = None;
    let mut escaped = false;
    for c in line.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '[' => depth += 1,
                ']' => depth -= 1,
                '#' => break,
                _ => {}
            },
        }
    }
    depth
}

/// Whether `line` gives `key` a value (`key = …`, not `# key = …` nor `key2 = …`).
fn defines(line: &str, key: &str) -> bool {
    line.trim_start().strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='))
}

/// A Plex-compatible API address that turns it off.
fn is_off(value: &str) -> bool {
    matches!(value.trim(), "" | "off" | "none" | "false" | "0")
}

/// A compatibility API's address that puts it on pley's own port.
fn is_web(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("web")
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir().map(|d| d.join("pley")).unwrap_or_else(|| PathBuf::from("pley-data"))
}

fn log_file(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join("pley.log")
}

fn absolutize(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map(|cwd| cwd.join(path)).unwrap_or_else(|_| path.to_path_buf())
    }
}

/// Accepts `host:port`, `:port` or a bare `port`.
fn parse_bind(value: &str) -> anyhow::Result<SocketAddr> {
    let value = value.trim();
    if let Ok(port) = value.trim_start_matches(':').parse::<u16>() {
        return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port));
    }
    value.parse().with_context(|| format!("invalid bind address '{value}'"))
}

/// One address or several, separated by commas; the first is the one the
/// server must be able to listen on as it starts.
fn parse_binds(value: &str) -> anyhow::Result<Vec<SocketAddr>> {
    let binds =
        value.split(',').filter(|part| !part.trim().is_empty()).map(parse_bind).collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(!binds.is_empty(), "no address to listen on in '{value}'");
    Ok(binds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_forms() {
        assert_eq!(parse_bind("8080").unwrap().port(), 8080);
        assert_eq!(parse_bind(":9000").unwrap().port(), 9000);
        assert_eq!(parse_bind("127.0.0.1:1234").unwrap(), "127.0.0.1:1234".parse().unwrap());
        assert!(parse_bind("nope").is_err());
        assert_eq!(
            parse_bind(" 8080 ").unwrap().ip(),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            "a bare port listens everywhere"
        );
        assert_eq!(parse_bind("[::1]:7539").unwrap(), "[::1]:7539".parse().unwrap());
        assert!(parse_bind("70000").is_err());

        let several = parse_binds(" 127.0.0.1:7539, 192.168.1.10:7539 ,").unwrap();
        assert_eq!(several, ["127.0.0.1:7539".parse().unwrap(), "192.168.1.10:7539".parse().unwrap()]);
        assert!(parse_binds(" , ").is_err());
        assert!(parse_binds("127.0.0.1:7539, nope").is_err(), "every address must be one");
    }

    #[test]
    fn several_addresses_listen_and_the_jellyfin_api_follows() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = cli(dir.path());
        args.bind = Some("127.0.0.1:7539,100.64.0.7:7539".into());
        args.jellyfin_bind = Some("127.0.0.1:8096, 100.64.0.7:8096".into());
        let config = Config::load(&args).unwrap();
        assert_eq!(config.bind, "127.0.0.1:7539".parse().unwrap());
        assert_eq!(config.extra_binds, ["100.64.0.7:7539".parse::<SocketAddr>().unwrap()]);
        assert_eq!(config.jellyfin_bind, Some("127.0.0.1:8096".parse().unwrap()));
        assert_eq!(config.jellyfin_extra_binds, ["100.64.0.7:8096".parse::<SocketAddr>().unwrap()]);
    }

    #[test]
    fn the_jellyfin_api_is_off_unless_turned_on() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = cli(dir.path());
        let config = Config::load(&args).unwrap();
        assert!(!config.jellyfin_on_web, "off by default");
        assert_eq!(config.jellyfin_bind, None);
        for (value, on_web, bind) in [
            ("web", true, None),
            (" Web ", true, None),
            ("off", false, None),
            ("0.0.0.0:8096", false, Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), JELLYFIN_PORT))),
        ] {
            args.jellyfin_bind = Some(value.into());
            let config = Config::load(&args).unwrap();
            assert_eq!((config.jellyfin_on_web, config.jellyfin_bind), (on_web, bind), "{value}");
        }
    }

    #[test]
    fn bonjour_is_off_until_turned_on() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pley.toml");
        let config = Config::load(&cli(dir.path())).unwrap();
        assert!(!config.bonjour && config.discovery && config.bonjour_adapters.is_empty());

        // A switch of its own, apart from discovery's.
        std::fs::write(&file, "bonjour = true\n").unwrap();
        assert!(Config::load(&cli(dir.path())).unwrap().bonjour);
        std::fs::write(&file, "discovery = false\nbonjour = true\n").unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert!(config.bonjour && !config.discovery);

        // --no-bonjour, or --no-discovery, turns it off for good.
        std::fs::write(&file, "bonjour = true\n").unwrap();
        for (no_discovery, no_bonjour) in [(false, true), (true, false)] {
            let mut args = cli(dir.path());
            (args.no_discovery, args.no_bonjour) = (no_discovery, no_bonjour);
            let config = Config::load(&args).unwrap();
            assert!(!config.bonjour && config.network_fixed.bonjour);
            assert_eq!(config.discovery, !no_discovery);
        }

        // Adapters written by hand: a blank or odd one is left out, not
        // a reason to stop, each is kept once, and 32 at most.
        std::fs::write(&file, "bonjour_adapters = [\" Wi-Fi \", \"\", \"eth\\u0007\", \"Ethernet\", \"Wi-Fi\"]\n")
            .unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!(config.bonjour_adapters, ["Ethernet", "Wi-Fi"]);
        assert_eq!(config.file_warnings.len(), 2, "{:?}", config.file_warnings);
        let many: Vec<String> = (0..40).map(|i| format!("\"eth{i:02}\"")).collect();
        std::fs::write(&file, format!("bonjour_adapters = [{}]\n", many.join(", "))).unwrap();
        assert_eq!(Config::load(&cli(dir.path())).unwrap().bonjour_adapters.len(), 32);

        // A host name as typed is kept as its label; one that can't be a
        // host name leaves the generated one.
        std::fs::write(&file, "bonjour_host_name = \" Media.local \"\n").unwrap();
        assert_eq!(Config::load(&cli(dir.path())).unwrap().bonjour_host_name, "media");
        std::fs::write(&file, "bonjour_host_name = \"my media\"\n").unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!(config.bonjour_host_name, "");
        assert_eq!(config.file_warnings.len(), 1, "{:?}", config.file_warnings);

        let mut args = cli(dir.path());
        args.no_bonjour = true;
        std::fs::write(&file, "").unwrap();
        save_config(&args, &file, None).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "bonjour = false\n");
        // Setup saves its discovery checkbox on every update: on, it leaves
        // Bonjour turned off in Settings alone; off, it turns Bonjour off.
        save_config(&cli(dir.path()), &file, Some(true)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "bonjour = false\ndiscovery = true\n");
        std::fs::write(&file, "bonjour = true\n").unwrap();
        save_config(&cli(dir.path()), &file, Some(false)).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "bonjour = false\ndiscovery = false\n");
    }

    #[test]
    fn the_plex_api_is_on_pleys_port_unless_given_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let mut args = cli(dir.path());
        let config = Config::load(&args).unwrap();
        assert!(config.plex_on_web, "on by default, on Pley's port");
        assert_eq!(config.plex_bind, None);
        for (value, on_web, bind) in [
            ("web", true, None),
            ("off", false, None),
            ("0.0.0.0:32400", false, Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), PLEX_PORT))),
        ] {
            args.plex_bind = Some(value.into());
            let config = Config::load(&args).unwrap();
            assert_eq!((config.plex_on_web, config.plex_bind), (on_web, bind), "{value}");
        }

        // Each API is saved under its own key.
        std::fs::write(dir.path().join("pley.toml"), "plex_bind = \"0.0.0.0:32400\"\n").unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!((config.plex_bind, config.jellyfin_on_web), (Some("0.0.0.0:32400".parse().unwrap()), false));
        save_listen(&config.config_file, &crate::network::Listen::of(&config), config.network_fixed).unwrap();
        let text = std::fs::read_to_string(&config.config_file).unwrap();
        assert!(text.contains("plex_bind = \"0.0.0.0:32400\"") && text.contains("jellyfin_bind = \"off\""), "{text}");
    }

    /// Command line options as if none were given (built directly, so the
    /// `PLEY_*` environment of the test run cannot leak in).
    fn cli(data_dir: &Path) -> Cli {
        Cli {
            command: None,
            config: None,
            data_dir: Some(data_dir.to_path_buf()),
            transcode_dir: None,
            bind: None,
            jellyfin_bind: None,
            plex_bind: None,
            ffmpeg: None,
            ffprobe: None,
            web_dir: None,
            transcoder_path: None,
            transcoder_url: None,
            transcoder_token: None,
            log: "info".into(),
            no_tray: false,
            no_discovery: false,
            no_bonjour: false,
        }
    }

    #[test]
    fn defaults_without_a_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!(config.bind, SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_PORT));
        assert_eq!((config.plex_bind, config.plex_on_web), (None, true));
        assert_eq!((config.jellyfin_bind, config.jellyfin_on_web), (None, false));
        assert_eq!(config.data_dir, dir.path());
        assert_eq!(config.transcode_dir, dir.path().join("transcode"));
        assert_eq!(config.db_path(), dir.path().join("pley.db"));
        assert_eq!((config.ffmpeg.as_path(), config.ffprobe.as_path()), (Path::new("ffmpeg"), Path::new("ffprobe")));
        assert_eq!(config.transcoder_max_jobs, 8);
        assert_eq!(config.transcoder_url, None);
    }

    #[test]
    fn the_config_file_in_the_data_dir_is_read_and_the_command_line_wins() {
        let dir = tempfile::tempdir().unwrap();
        let transcode = dir.path().join("scratch");
        std::fs::write(
            dir.path().join("pley.toml"),
            format!(
                "bind = \"127.0.0.1:8000\"\njellyfin_bind = \"web\"\nffmpeg = \"/opt/ff/ffmpeg\"\n\
                 transcode_dir = {:?}\ntranscoder_url = \"  \"\ntranscoder_max_jobs = 0\n",
                transcode.to_string_lossy()
            ),
        )
        .unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!(config.bind, "127.0.0.1:8000".parse().unwrap());
        assert!(config.jellyfin_on_web, "`web` puts the Jellyfin-compatible API on Pley's port");
        assert_eq!(config.ffmpeg, PathBuf::from("/opt/ff/ffmpeg"));
        assert_eq!(config.transcode_dir, transcode);
        assert_eq!(config.transcoder_url, None, "a blank URL means: spawn the transcoder");
        assert_eq!(config.transcoder_max_jobs, 1, "at least one job");

        let mut args = cli(dir.path());
        args.bind = Some("9000".into());
        args.jellyfin_bind = Some("127.0.0.1:8097".into());
        args.ffmpeg = Some("ffmpeg7".into());
        let config = Config::load(&args).unwrap();
        assert_eq!(config.bind, SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 9000));
        assert_eq!((config.jellyfin_bind, config.jellyfin_on_web), (Some("127.0.0.1:8097".parse().unwrap()), false));
        assert_eq!(config.ffmpeg, PathBuf::from("ffmpeg7"));
    }

    #[test]
    fn the_transcode_folder_is_chosen_saved_and_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pley.toml");
        let chosen = dir.path().join("fast disk");
        std::fs::write(&file, "# mine\nbind = \"7539\"\n").unwrap();
        save_transcode_dir(&file, Some(&chosen)).unwrap();
        assert_eq!(saved_transcode_dir(&file), Some(chosen.clone()));
        let mut config = Config::load(&cli(dir.path())).unwrap();
        config.check_transcode_dir();
        assert_eq!((config.transcode_dir.as_path(), config.transcode_dir_fixed), (chosen.as_path(), false));
        assert!(chosen.is_dir() && config.transcode_dir_problem.is_none());

        // One that can't be made (a file is in the way): the default one is used.
        let blocked = dir.path().join("a file");
        std::fs::write(&blocked, "").unwrap();
        save_transcode_dir(&file, Some(&blocked.join("transcode"))).unwrap();
        let mut config = Config::load(&cli(dir.path())).unwrap();
        config.check_transcode_dir();
        assert_eq!(config.transcode_dir, dir.path().join("transcode"));
        let problem = config.transcode_dir_problem.map(|(dir, _)| dir);
        assert_eq!(problem, Some(blocked.join("transcode")), "the one chosen is tried again next time");

        // Back to the default: the key goes, the rest stays.
        save_transcode_dir(&file, None).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "# mine\nbind = \"7539\"\n");
        assert_eq!(saved_transcode_dir(&file), None);

        // The command line wins, and the web app can't change it.
        let mut args = cli(dir.path());
        args.transcode_dir = Some(chosen.clone());
        let config = Config::load(&args).unwrap();
        assert_eq!((config.transcode_dir, config.transcode_dir_fixed), (chosen, true));
    }

    #[test]
    fn an_explicit_config_file_can_move_the_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("elsewhere");
        let file = dir.path().join("custom.toml");
        std::fs::write(&file, format!("data_dir = {:?}\n", data.to_string_lossy())).unwrap();
        let mut args = cli(dir.path());
        args.data_dir = None;
        args.config = Some(file);
        let config = Config::load(&args).unwrap();
        assert_eq!(config.data_dir, data);
        assert_eq!(config.transcode_dir, data.join("transcode"));
    }

    #[test]
    fn set_keys_replaces_lines_keeps_the_rest_and_appends_new_keys() {
        let text = "# pley\nbind = \"0.0.0.0:7539\"\n# jellyfin_bind = \"off\"\nbind_extra = 1\nbind=\"x\"\n";
        let out = set_keys(text, &[("bind", "127.0.0.1:7539".into()), ("jellyfin_bind", "off".into())]);
        assert_eq!(
            out,
            "# pley\nbind = \"127.0.0.1:7539\"\n# jellyfin_bind = \"off\"\nbind_extra = 1\njellyfin_bind = \"off\"\n"
        );
        assert_eq!(set_keys("", &[("discovery", false.into())]), "discovery = false\n");

        // An array written by hand over several lines is replaced whole,
        // brackets in its strings and comments notwithstanding.
        let text = "bonjour_adapters = [\n  \"Ethernet\", # wired [main]\n  \"vEthernet [x]\",\n]\nbind = \"9000\"\n";
        let adapters: Vec<toml::Value> = vec!["Wi-Fi".into()];
        let out = set_keys(text, &[("bonjour_adapters", adapters.into())]);
        assert_eq!(out, "bonjour_adapters = [\"Wi-Fi\"]\nbind = \"9000\"\n");
        assert!(toml::from_str::<FileConfig>(&out).is_ok());
        assert_eq!(brackets("x = [\"a]\", 'b[' # ] ["), 1);
        assert_eq!(brackets("x = [1, [2]]"), 0);
    }

    #[test]
    fn saved_options_are_read_back_and_other_lines_stay() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pley.toml");
        std::fs::write(&file, "# mine\ntranscoder_max_jobs = 3\nbind = \"9000\"\n").unwrap();
        let mut args = cli(dir.path());
        args.bind = Some("127.0.0.1:7540".into());
        args.jellyfin_bind = Some("web".into());
        args.ffmpeg = Some(PathBuf::from(r"C:\Program Files\pley\ffmpeg\ffmpeg.exe"));
        save_config(&args, &file, Some(false)).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with("# mine\ntranscoder_max_jobs = 3\n"), "{text}");

        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!(config.bind, "127.0.0.1:7540".parse().unwrap());
        assert!(config.jellyfin_on_web);
        assert_eq!(config.ffmpeg, PathBuf::from(r"C:\Program Files\pley\ffmpeg\ffmpeg.exe"));
        assert!(!config.discovery);
        assert_eq!(config.transcoder_max_jobs, 3);

        args = cli(dir.path());
        args.bind = Some("nope".into());
        assert!(save_config(&args, &file, None).is_err(), "an unreadable address isn't saved");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    }

    #[test]
    fn where_pley_listens_is_saved_but_what_the_command_line_sets() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!(config.config_file, dir.path().join("pley.toml"), "made when first saved");
        assert_eq!(config.network_fixed, crate::network::Fixed::default());
        let listen = crate::network::Listen {
            web: vec!["127.0.0.1:7540".parse().unwrap(), "192.168.1.10:7540".parse().unwrap()],
            plex: Vec::new(),
            plex_on_web: false,
            jellyfin: Vec::new(),
            jellyfin_on_web: true,
            discovery: false,
            bonjour: true,
            bonjour_adapters: vec!["Ethernet".into(), "vEthernet (Default Switch)".into()],
            bonjour_host_name: "media".into(),
        };
        save_listen(&config.config_file, &listen, config.network_fixed).unwrap();
        let config = Config::load(&cli(dir.path())).unwrap();
        assert_eq!(crate::network::Listen::of(&config), listen);

        // PLEY_BIND (or --bind) wins: it isn't saved over, the rest is.
        let mut args = cli(dir.path());
        args.bind = Some("0.0.0.0:7539".into());
        let fixed = Config::load(&args).unwrap();
        assert_eq!(
            fixed.network_fixed,
            crate::network::Fixed { web: true, plex: false, jellyfin: false, discovery: false, bonjour: false }
        );
        let mut changed = crate::network::Listen::of(&fixed);
        changed.plex = vec!["127.0.0.1:32400".parse().unwrap()];
        changed.jellyfin_on_web = false;
        changed.jellyfin = vec!["127.0.0.1:8096".parse().unwrap()];
        save_listen(&fixed.config_file, &changed, fixed.network_fixed).unwrap();
        let text = std::fs::read_to_string(&fixed.config_file).unwrap();
        assert!(text.contains("bind = \"127.0.0.1:7540, 192.168.1.10:7540\""), "{text}");
        assert!(text.contains("plex_bind = \"127.0.0.1:32400\""), "{text}");
        assert!(text.contains("jellyfin_bind = \"127.0.0.1:8096\""), "{text}");
        assert!(!dir.path().join("pley.toml.new").exists());
    }

    #[test]
    fn bad_config_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pley.toml"), "bindd = \"8080\"\n").unwrap();
        let err = Config::load(&cli(dir.path())).unwrap_err();
        assert!(format!("{err:#}").contains("pley.toml"), "{err:#}");

        std::fs::write(dir.path().join("pley.toml"), "jellyfin_bind = \"somewhere\"\n").unwrap();
        assert!(Config::load(&cli(dir.path())).is_err(), "an unreadable address is an error, not a silent default");

        let mut args = cli(dir.path());
        args.config = Some(dir.path().join("missing.toml"));
        assert!(Config::load(&args).is_err());
    }
}
