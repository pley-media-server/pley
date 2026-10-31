// SPDX-License-Identifier: AGPL-3.0-only
//
// pley-transcoder — FFmpeg media engine for the Pley media server.
// Copyright (c) 2026 the Pley authors. See LICENSE.

//! Standalone transcoding service. It is the only pley component that runs
//! FFmpeg; the media server talks to it over HTTP (docs/TRANSCODER.md).

mod adaptive;
mod background;
mod cache;
mod convert;
mod cpuload;
mod devices;
mod dovi;
mod error;
mod extract;
mod ffmpeg;
mod fingerprint;
// Nothing reads fMP4 output yet: jobs still make MPEG-TS segments only.
#[allow(dead_code)]
mod fmp4;
mod hls;
mod http;
mod job;
mod keyframes;
mod live;
mod matroska;
mod memory;
mod metrics;
mod mks;
mod mp4;
mod plan;
mod prefetch;
mod probe;
#[cfg(test)]
mod remux_tests;
mod share;
mod trickplay;
mod tsedit;
mod util;
mod webvtt;

use std::future::IntoFuture;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clap::parser::ValueSource;
use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use pley_transcoder_api::{API_VERSION, Announce, Limits, TOKEN_ENV, TranscoderInfo};
use tokio::io::AsyncReadExt;

const LICENSE_NOTICE: &str = "pley-transcoder  Copyright (c) 2026 the Pley authors
This program comes with ABSOLUTELY NO WARRANTY. It is free software: you can
redistribute it and/or modify it under the terms of the GNU Affero General
Public License, version 3. See the LICENSE file.";

/// Where this program's source is offered (`TranscoderInfo::source`). A
/// modified build points it at its own source, as the AGPL asks.
const SOURCE: &str = "https://github.com/pley-media-server/pley";

#[derive(Debug, Parser)]
#[command(
    name = "pley-transcoder",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("PLEY_COMMIT"), ")"),
    about = "Pley transcoder — FFmpeg media engine for the Pley media server"
)]
struct Args {
    /// Address to listen on (port 0 picks a free port)
    #[arg(long, env = "PLEY_TRANSCODER_LISTEN", default_value = "127.0.0.1:7540")]
    listen: SocketAddr,

    /// Shared secret; clients must send `Authorization: Bearer <token>`. Set
    /// it in the environment: the command line is in the list of processes
    #[arg(long, env = TOKEN_ENV, hide_env_values = true)]
    token: Option<String>,

    /// Directory for segments, caches and the capability cache
    #[arg(long, env = "PLEY_TRANSCODER_WORK_DIR")]
    work_dir: Option<PathBuf>,

    /// ffmpeg binary
    #[arg(long, env = "PLEY_FFMPEG", default_value = "ffmpeg")]
    ffmpeg: PathBuf,

    /// ffprobe binary
    #[arg(long, env = "PLEY_FFPROBE", default_value = "ffprobe")]
    ffprobe: PathBuf,

    /// Maximum concurrent ffmpeg transcode processes
    #[arg(long, env = "PLEY_TRANSCODER_MAX_JOBS", default_value_t = 8)]
    max_jobs: u32,

    /// Maximum concurrent hardware encoder sessions; extra jobs use software encoding
    #[arg(long, env = "PLEY_TRANSCODER_MAX_HW_JOBS", default_value_t = 8)]
    max_hw_jobs: u32,

    /// Concurrent probes / extractions
    #[arg(long, default_value_t = 8)]
    probe_concurrency: usize,

    /// Seek-preview sets made at once at most (default 5 where runs can be
    /// paused, 1 elsewhere)
    #[arg(long, env = "PLEY_TRANSCODER_MAX_PREVIEWS", value_parser = clap::value_parser!(u32).range(1..=16))]
    max_previews: Option<u32>,

    /// Whether HDR transcodes may decode their video on the CPU where the
    /// GPU falls behind and the CPU has cores to spare
    #[arg(long, env = "PLEY_TRANSCODER_CPU_DECODE", value_enum, default_value_t = CpuDecode::Auto)]
    cpu_decode: CpuDecode,

    /// Print `{"event":"listening","addr":"…"}` on stdout once ready
    #[arg(long)]
    announce: bool,

    /// Exit when stdin reaches EOF (used when supervised by the Pley server)
    #[arg(long)]
    exit_with_parent: bool,

    /// Ignore the cached FFmpeg capability detection
    #[arg(long)]
    redetect: bool,

    /// Print the license notice and exit
    #[arg(long)]
    license: bool,

    /// Log filter, e.g. `info` or `debug`
    #[arg(long, env = "PLEY_LOG", default_value = "info")]
    log: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CpuDecode {
    /// Where it keeps the transcodes ahead of their players
    Auto,
    /// The GPU decodes what it can, however far behind it falls
    Never,
}

fn main() -> anyhow::Result<()> {
    let matches = Args::command().get_matches();
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|err| err.exit());
    if args.license {
        println!("{LICENSE_NOTICE}\nSource: {SOURCE}");
        return Ok(());
    }
    let filter = if args.log.contains('=') { args.log.clone() } else { format!("{},hyper=warn", args.log) };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_new(filter).unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .compact()
        .init();
    if matches.value_source("token") == Some(ValueSource::CommandLine) {
        tracing::warn!(
            "the token is on the command line, which every account of this computer can read in its list of \
             processes: give it in {TOKEN_ENV} instead"
        );
    }
    tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("transcoder").build()?.block_on(run(args))
}

/// Tokens from examples, which are no secret.
const EXAMPLE_TOKENS: &[&str] = &["change-me", "changeme", "secret", "token", "password", "pley", "transcoder"];
/// The shortest token the transcoder takes.
const MIN_TOKEN_CHARS: usize = 16;

/// Whether the transcoder may listen on `listen` with `token`: on the
/// network it reads any file it is asked to, so it takes a real secret. On
/// this computer alone it may go without one (it then answers only requests
/// addressed to this computer, see `http.rs`), but one it is given is a
/// real secret there too.
fn check_token(token: Option<&str>, listen: SocketAddr) -> anyhow::Result<()> {
    let suggestion = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    match token {
        None if listen.ip().is_loopback() => Ok(()),
        None => anyhow::bail!(
            "listening on {listen} takes a token ({TOKEN_ENV}, and the same in Pley's transcoder_token): \
             anyone who can reach this port could otherwise read any file it can. A random one: {suggestion}"
        ),
        Some(token)
            if token.chars().count() < MIN_TOKEN_CHARS
                || EXAMPLE_TOKENS.contains(&token.to_ascii_lowercase().as_str()) =>
        {
            anyhow::bail!(
                "the token must be a secret of at least {MIN_TOKEN_CHARS} characters, not an example. A random one: {suggestion}"
            )
        }
        Some(_) => Ok(()),
    }
}

/// The work folder when none is given: in the user's own cache folder, not
/// the temporary folder every user shares (where someone else could make it
/// first, or read what is in it).
fn default_work_dir() -> PathBuf {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let cache = if cfg!(windows) {
        var("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library").join("Caches"))
    } else {
        var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|home| home.join(".cache")))
    };
    cache.unwrap_or_else(std::env::temp_dir).join("pley-transcoder")
}

/// Keeps a folder of the transcoder's (decoded video, subtitles, converted
/// downloads) to this account: on Unix, a folder another user made first
/// can't be taken over; on Windows, a folder on another drive doesn't let
/// every account in, as a drive's own permissions do.
///
/// Unless `through_link`, a link or a junction at `dir` is refused, never
/// followed: the permissions are set on the folder opened, once it is
/// known to be the one there, so one put back meanwhile changes nothing
/// it leads to.
fn keep_private(dir: &Path, through_link: bool) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let refused = |err: std::io::Error| {
            anyhow::anyhow!("{} isn't this account's to use ({err}): choose a folder with --work-dir", dir.display())
        };
        let folder = std::fs::File::open(dir).map_err(refused)?;
        if !through_link {
            let (opened, there) = (folder.metadata()?, std::fs::symlink_metadata(dir)?);
            anyhow::ensure!(
                !there.file_type().is_symlink() && (opened.dev(), opened.ino()) == (there.dev(), there.ino()),
                "{} is a link someone else put there: choose another folder",
                dir.display()
            );
        }
        folder.set_permissions(std::fs::Permissions::from_mode(0o700)).map_err(refused)?;
    }
    // A drive that keeps no permissions (FAT, some shares) goes without:
    // said once for each folder, though segments' are kept so for each job.
    #[cfg(windows)]
    if let Err(err) = windows_private::keep(dir, through_link) {
        static TOLD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
        let mut told = TOLD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !told.iter().any(|told| told == dir) {
            told.push(dir.to_path_buf());
            tracing::warn!("other accounts on this computer may reach {}: {err}", dir.display());
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = (dir, through_link);
    Ok(())
}

/// Makes `dir`, a folder of segments, the transcoder's own and keeps it
/// private ([`keep_private`]). A link or a junction there (put by another
/// account, in a work folder others can write) is removed rather than
/// followed, so no segment is written or read wherever it leads.
fn own_folder(dir: &Path) -> anyhow::Result<()> {
    unlink(dir)?;
    std::fs::create_dir_all(dir)?;
    keep_private(dir, false)?;
    // One put back meanwhile is refused rather than used (on Windows
    // keep_private only warns of it).
    anyhow::ensure!(!is_link(dir), "{} is a link someone else put there: choose another folder", dir.display());
    Ok(())
}

/// Whether `path` is a symbolic link, or a junction on Windows.
fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// Removes `path` when it is a link or a junction: the link itself, never
/// what it leads to.
fn unlink(path: &Path) -> std::io::Result<()> {
    if is_link(path) { std::fs::remove_dir_all(path) } else { Ok(()) }
}

/// Whether other accounts may write in `work_dir`, so that a link in place
/// of one of its folders may be theirs: on Unix, when it isn't this
/// account's (`own`, a folder the transcoder made there, is) or its group or
/// others may write in it; on Windows, unless its permissions are its own,
/// as [`keep_private`] leaves them, rather than the drive's.
fn shared_work_dir(work_dir: &Path, own: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(work_dir), std::fs::metadata(own)) {
            (Ok(dir), Ok(own)) => dir.uid() != own.uid() || dir.mode() & 0o022 != 0,
            _ => true,
        }
    }
    #[cfg(windows)]
    {
        let _ = own;
        !windows_private::is_protected(work_dir).unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (work_dir, own);
        true
    }
}

/// [`keep_private`] on Windows: this account, SYSTEM and the administrators
/// (who can take any folder anyway), nothing inherited. A folder whose
/// permissions are its own already (set by hand, or by an earlier start) is
/// left as it is.
#[cfg(windows)]
mod windows_private {
    use std::fs::File;
    use std::io;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_FILE_OBJECT, SetSecurityInfo,
    };
    use windows_sys::Win32::Security::{
        ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetTokenInformation,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, READ_CONTROL, WRITE_DAC,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    pub fn keep(dir: &Path, through_link: bool) -> io::Result<()> {
        if is_protected_folder(&open(dir, READ_CONTROL, through_link)?)? {
            return Ok(());
        }
        // Opened again to change it, and checked again: the folder whose
        // permissions are set is the one there, never where a link leads.
        let folder = open(dir, READ_CONTROL | WRITE_DAC, through_link)?;
        let sddl = format!("D:P(A;OICI;FA;;;{})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)", own_sid()?);
        let sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: a NUL-terminated string; Windows allocates the descriptor, freed below.
        let made = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if made == 0 {
            return Err(io::Error::last_os_error());
        }
        let (mut present, mut defaulted, mut dacl) = (0, 0, null_mut::<ACL>());
        // SAFETY: the descriptor made above, which the DACL points into.
        let status = unsafe {
            if GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(SetSecurityInfo(
                    folder.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    dacl,
                    null_mut(),
                ))
            }
        };
        // SAFETY: allocated by Windows with LocalAlloc.
        unsafe { LocalFree(descriptor) };
        match status? {
            ERROR_SUCCESS => Ok(()),
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }

    /// The folder `dir`, opened for `access`. Unless `through_link`, a link
    /// or a junction there is opened itself, and refused.
    fn open(dir: &Path, access: u32, through_link: bool) -> io::Result<File> {
        let flags = if through_link { 0 } else { FILE_FLAG_OPEN_REPARSE_POINT };
        let folder = File::options().access_mode(access).custom_flags(FILE_FLAG_BACKUP_SEMANTICS | flags).open(dir)?;
        if !through_link && folder.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("it is a link someone else put there"));
        }
        Ok(folder)
    }

    /// Whether the folder `dir` has permissions of its own, not those of
    /// the folder it is in.
    pub(super) fn is_protected(dir: &Path) -> io::Result<bool> {
        is_protected_folder(&open(dir, READ_CONTROL, true)?)
    }

    fn is_protected_folder(folder: &File) -> io::Result<bool> {
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: an open handle; Windows allocates the descriptor, freed below.
        let status = unsafe {
            GetSecurityInfo(
                folder.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: the descriptor read above.
        let ok = unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) };
        // SAFETY: allocated by Windows with LocalAlloc.
        unsafe { LocalFree(descriptor) };
        Ok(ok != 0 && control & SE_DACL_PROTECTED != 0)
    }

    /// The SID of the account the transcoder runs as.
    fn own_sid() -> io::Result<String> {
        let mut token: HANDLE = null_mut();
        // SAFETY: this process's pseudo-handle; the token is closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // Aligned for TOKEN_USER, and large enough for any SID.
        let mut buf = [0u64; 64];
        let mut len = 0u32;
        // SAFETY: a buffer of the size given.
        let ok = unsafe {
            GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), size_of_val(&buf) as u32, &mut len)
        };
        let error = io::Error::last_os_error();
        // SAFETY: the token opened above.
        unsafe { CloseHandle(token) };
        if ok == 0 {
            return Err(error);
        }
        // SAFETY: GetTokenInformation wrote a TOKEN_USER at the start of `buf`.
        let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text = null_mut();
        // SAFETY: a valid SID; Windows allocates the string, freed below.
        if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a NUL-terminated string from Windows.
        let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
        // SAFETY: the `len` units just read.
        let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        // SAFETY: allocated by Windows with LocalAlloc.
        unsafe { LocalFree(text.cast()) };
        Ok(sid)
    }
}

/// Makes the work folder ready, and says where the jobs go: the folders
/// the transcoder keeps in it (`jobs` and `live`, emptied of an earlier
/// run's segments, and the caches) are made, its own ([`own_folder`]), and
/// kept to this account. The work folder itself is made private only when
/// the transcoder makes it or it is the default one, named for it: one
/// given that is there already may be shared (`/tmp` on LibreELEC) and
/// keeps its permissions.
fn prepare_work_dir(work_dir: &Path, default: bool) -> anyhow::Result<PathBuf> {
    let made = !work_dir.exists();
    std::fs::create_dir_all(work_dir)?;
    if made || default {
        keep_private(work_dir, true)?;
    }
    let jobs_dir = work_dir.join("jobs");
    let _ = std::fs::remove_dir_all(&jobs_dir);
    own_folder(&jobs_dir)?;
    // Live TV's segments too: in a folder others can write, one of theirs
    // could read what plays, or swap a segment for a link. The folder is
    // made again for each channel, so one that can't be now (a file in the
    // way) fails Live TV only.
    let live = work_dir.join("live");
    let _ = std::fs::remove_dir_all(&live);
    if let Err(err) = own_folder(&live) {
        tracing::warn!("Live TV has no folder for its segments: {err}");
    }
    let shared = shared_work_dir(work_dir, &jobs_dir);
    for (folder, _) in cache::policies(work_dir) {
        // A cache that is a link, where another account could have put it,
        // would be written to and pruned wherever it leads. Elsewhere it is
        // this account's, keeping the cache on another disk say, and stays.
        if shared {
            unlink(&folder)?;
        }
        // One that can't be made (a file in the way) goes without, as each
        // cache does by itself. One put back meanwhile is refused, as in
        // own_folder.
        if std::fs::create_dir_all(&folder).is_ok() {
            keep_private(&folder, !shared)?;
            anyhow::ensure!(
                !shared || !is_link(&folder),
                "{} is a link someone else put there: choose another folder",
                folder.display()
            );
        }
    }
    Ok(jobs_dir)
}

/// The folder libplacebo keeps compiled shaders in. Its path is absolute:
/// FFmpeg runs in a job's folder, which goes with the job. It is made now,
/// since libplacebo makes none and then silently caches nothing.
fn shader_cache(work_dir: &Path) -> Option<PathBuf> {
    let dir = std::path::absolute(work_dir).ok()?.join("libplacebo");
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(err) => {
            tracing::warn!("no shader cache for tone mapping: {}: {err}", dir.display());
            None
        }
    }
}

async fn run(args: Args) -> anyhow::Result<()> {
    util::kill_children_on_exit();
    check_token(args.token.as_deref(), args.listen)?;
    let work_dir = args.work_dir.clone().unwrap_or_else(default_work_dir);
    let jobs_dir = prepare_work_dir(&work_dir, args.work_dir.is_none())?;

    let capabilities = work_dir.join("capabilities.json");
    let ff = Arc::new(ffmpeg::Ffmpeg::detect(&args.ffmpeg, &args.ffprobe, &capabilities, args.redetect).await);
    let metrics = Arc::new(metrics::Metrics::default());
    let prober = Arc::new(probe::Prober::new(ff.clone(), metrics.clone(), args.probe_concurrency));
    // Seek previews, fingerprints, keyframe scans and probes wait for starting
    // playbacks; all but probes wait for transcodes, and for subtitles a
    // player waits for, too.
    let background = background::Background::new();
    let keyframes =
        keyframes::KeyframeStore::new(ff.clone(), metrics.clone(), background.clone(), work_dir.join("keyframes"));
    let extractor = Arc::new(
        extract::Extractor::new(
            ff.clone(),
            prober.clone(),
            metrics.clone(),
            work_dir.join("subtitles"),
            args.probe_concurrency.div_ceil(2),
        )
        .with_background(background.clone()),
    );
    let fingerprinter = Arc::new(fingerprint::Fingerprinter::new(ff.clone(), background.clone(), 2));
    let limits = Limits { max_jobs: args.max_jobs.max(1), max_hw_jobs: args.max_hw_jobs };
    let jobs = job::JobManager::new(
        ff.clone(),
        prober.clone(),
        keyframes.clone(),
        extractor.clone(),
        metrics.clone(),
        jobs_dir.clone(),
        shader_cache(&work_dir),
        Some(capabilities),
        limits.clone(),
        background.clone(),
    );
    if args.cpu_decode == CpuDecode::Never {
        jobs.never_decode_on_cpu();
    }
    let runs = args.max_previews.unwrap_or_else(trickplay::default_runs);
    tracing::info!("seek previews: up to {runs} at once");
    // FFmpeg's threads fit a CPU limit the jobs' processors' meter finds.
    let trickplay = Arc::new(trickplay::Trickplay::new(
        ff.clone(),
        prober.clone(),
        background,
        work_dir.join("trickplay"),
        runs,
        jobs.cpu(),
    ));
    // Segments in memory, for jobs that ask, where this system has a folder in RAM.
    let memory = memory::MemoryStore::open();
    match &memory {
        Ok(store) => {
            tracing::info!(
                "segments can be kept in memory, {} MB at most, in {}",
                store.budget() / (1024 * 1024),
                store.root().display()
            );
            jobs.keep_in_memory(store.clone());
        }
        Err(why) => tracing::info!("segments are kept on disk, not in memory: {why}"),
    }
    jobs.spawn_reaper();
    let conversions = convert::Conversions::new(
        ff.clone(),
        prober.clone(),
        jobs.slots.clone(),
        shader_cache(&work_dir),
        work_dir.join("conversions"),
    );
    cache::spawn_janitor(work_dir.clone(), shared_work_dir(&work_dir, &jobs_dir));

    let info = TranscoderInfo {
        name: "pley-transcoder".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        commit: Some(env!("PLEY_COMMIT")).filter(|c| *c != "unknown").map(Into::into),
        api_version: API_VERSION,
        license: "AGPL-3.0-only".into(),
        source: Some(SOURCE.into()),
        started_at_ms: util::now_ms(),
        // Not FFmpeg's: this program's own (master playlists, segment edits,
        // subtitle tracks copied into files of their own).
        capabilities: pley_transcoder_api::Capabilities {
            hls_master_playlist: true,
            subtitle_matroska: true,
            subtitle_matroska_partial: true,
            conversions: true,
            conversions_mp4: true,
            hls_variants: true,
            live_bitrate: true,
            live: true,
            trickplay_version: trickplay::VERSION,
            trickplay_runs: runs,
            ..ff.caps.clone()
        },
        limits,
        memory: Some(memory::MemoryStore::info(&memory)),
    };
    let state = Arc::new(http::AppState::new(
        jobs.clone(),
        conversions.clone(),
        prober,
        extractor,
        keyframes,
        fingerprinter,
        trickplay,
        metrics,
        info,
        args.token.clone(),
        work_dir.clone(),
    ));
    state.spawn_disk_usage();
    let live = state.live.clone();

    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let addr = listener.local_addr()?;
    if args.announce {
        let line = serde_json::to_string(&Announce { event: "listening".into(), addr: addr.to_string() })?;
        println!("{line}");
        use std::io::Write;
        std::io::stdout().flush()?;
    }
    tracing::info!(
        "pley-transcoder {} ({}) listening on http://{addr}",
        env!("CARGO_PKG_VERSION"),
        env!("PLEY_COMMIT")
    );

    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let exit_with_parent = args.exit_with_parent;
    tokio::spawn(async move {
        shutdown_signal(exit_with_parent).await;
        let _ = stop_tx.send(true);
    });
    let mut drain = stop_rx.clone();
    let server = axum::serve(listener, http::router(state))
        .with_graceful_shutdown(async move {
            let _ = drain.wait_for(|stop| *stop).await;
        })
        .into_future();
    tokio::pin!(server);
    let mut stop = stop_rx;
    tokio::select! {
        result = &mut server => result?,
        _ = stop.wait_for(|stop| *stop) => {
            // Stop ffmpeg first: requests waiting for segments then fail right
            // away instead of keeping the graceful drain open.
            conversions.shutdown();
            live.shutdown();
            jobs.shutdown().await;
            if tokio::time::timeout(Duration::from_secs(5), &mut server).await.is_err() {
                tracing::warn!("some requests were still running at shutdown");
            }
        }
    }

    jobs.shutdown().await;
    let _ = std::fs::remove_dir_all(&jobs_dir);
    if let Ok(store) = &memory {
        let _ = std::fs::remove_dir_all(store.root());
    }
    tracing::info!("pley-transcoder stopped");
    Ok(())
}

async fn shutdown_signal(exit_with_parent: bool) {
    let parent_gone = async {
        if exit_with_parent {
            let mut stdin = tokio::io::stdin();
            let mut buf = [0u8; 64];
            loop {
                match stdin.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            tracing::info!("parent process went away");
        } else {
            std::future::pending::<()>().await;
        }
    };
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = parent_gone => {}
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_a_real_secret_and_on_the_network_a_must() {
        let local: SocketAddr = "127.0.0.1:7540".parse().unwrap();
        let network: SocketAddr = "0.0.0.0:7540".parse().unwrap();
        assert!(check_token(None, local).is_ok(), "on this computer alone");
        let refused = check_token(None, network).unwrap_err().to_string();
        assert!(refused.contains("A random one: "), "{refused}");
        for listen in [local, network] {
            assert!(check_token(Some("change-me"), listen).is_err(), "{listen}");
            assert!(check_token(Some("CHANGE-ME"), listen).is_err(), "{listen}");
            assert!(check_token(Some("short"), listen).is_err(), "{listen}");
            assert!(check_token(Some("9f2c5a7e1b3d4c6a8e0f1a2b3c4d5e6f"), listen).is_ok(), "{listen}");
        }
    }

    #[test]
    fn the_shader_cache_is_made_at_start_up() {
        let dir = tempfile::tempdir().unwrap();
        let cache = shader_cache(&dir.path().join("work")).unwrap();
        assert!(cache.is_absolute() && cache.is_dir() && cache.ends_with("libplacebo"));
        // Not where a file is in the way: tone mapping goes on without it.
        std::fs::write(dir.path().join("file"), b"").unwrap();
        assert_eq!(shader_cache(&dir.path().join("file")), None);
    }

    #[test]
    fn the_work_folder_is_made_ready() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(work.join("jobs").join("0123")).unwrap();
        std::fs::create_dir_all(work.join("live").join("4567")).unwrap();
        std::fs::create_dir_all(work.join("subtitles")).unwrap();
        std::fs::write(work.join("subtitles").join("k_2.vtt"), "WEBVTT\n").unwrap();
        let jobs = prepare_work_dir(&work, false).unwrap();
        assert!(jobs.is_dir() && !jobs.join("0123").exists(), "an earlier run's segments go");
        assert!(work.join("live").is_dir() && !work.join("live").join("4567").exists(), "a live channel's too");
        assert!(work.join("subtitles").join("k_2.vtt").exists(), "caches stay");
        for folder in ["keyframes", "trickplay", "libplacebo", "conversions"] {
            assert!(work.join(folder).is_dir(), "{folder}");
        }
    }

    #[test]
    fn a_file_in_place_of_live_tvs_folder_stops_live_tv_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("live"), b"").unwrap();
        assert!(prepare_work_dir(dir.path(), false).unwrap().is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_work_folder_there_already_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
        let dir = tempfile::tempdir().unwrap();
        // A folder every account shares, as /tmp.
        let shared = dir.path().join("tmp");
        std::fs::create_dir(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o1777)).unwrap();
        let jobs = prepare_work_dir(&shared, false).unwrap();
        assert_eq!(mode(&shared), 0o1777);
        assert_eq!((mode(&jobs), mode(&shared.join("subtitles"))), (0o700, 0o700), "what it keeps there is its own");
        assert_eq!(mode(&shared.join("live")), 0o700, "live channels' segments too");
        // A link in place of one of its folders is taken away, not followed:
        // what it led to isn't pruned, nor written to.
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::set_permissions(&elsewhere, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(elsewhere.join("keep.txt"), b"x").unwrap();
        for folder in ["trickplay", "live"] {
            std::fs::remove_dir_all(shared.join(folder)).unwrap();
            std::os::unix::fs::symlink(&elsewhere, shared.join(folder)).unwrap();
        }
        prepare_work_dir(&shared, false).unwrap();
        for folder in ["trickplay", "live"] {
            let meta = std::fs::symlink_metadata(shared.join(folder)).unwrap();
            assert!(meta.is_dir() && !meta.file_type().is_symlink(), "{folder}");
        }
        assert!(elsewhere.join("keep.txt").exists() && mode(&elsewhere) == 0o755);
        // One put back between making a folder and keeping it private is
        // refused, and what it leads to keeps its permissions.
        let link = shared.join("put-back");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        assert!(keep_private(&link, false).is_err());
        assert_eq!(mode(&elsewhere), 0o755);
        // One it makes is its own, as is the default one.
        let made = dir.path().join("made");
        prepare_work_dir(&made, false).unwrap();
        assert_eq!(mode(&made), 0o700);
        // There a cache that is a link is this account's, kept on another
        // disk say: it stays.
        std::fs::remove_dir_all(made.join("trickplay")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, made.join("trickplay")).unwrap();
        prepare_work_dir(&made, false).unwrap();
        assert!(std::fs::symlink_metadata(made.join("trickplay")).unwrap().file_type().is_symlink());
        prepare_work_dir(&shared, true).unwrap();
        assert_eq!(mode(&shared), 0o700);
    }

    #[cfg(windows)]
    #[test]
    fn a_work_folder_there_already_keeps_its_permissions_on_windows_too() {
        let dir = tempfile::tempdir().unwrap();
        // A folder every account may use, as one at the top of another drive.
        let shared = dir.path().join("shared");
        std::fs::create_dir(&shared).unwrap();
        let status = std::process::Command::new("icacls")
            .arg(&shared)
            .args(["/grant", "*S-1-5-11:(OI)(CI)M", "/Q"])
            .status()
            .unwrap();
        assert!(status.success());
        let jobs = prepare_work_dir(&shared, false).unwrap();
        let own = |path: &Path| windows_private::is_protected(path).unwrap();
        assert!(!own(&shared));
        assert!(own(&jobs) && own(&shared.join("conversions")), "what it keeps there is its own");
        assert!(own(&shared.join("live")), "live channels' segments too");
        let made = dir.path().join("made");
        prepare_work_dir(&made, false).unwrap();
        assert!(own(&made));
        // Still the transcoder's to use.
        std::fs::write(jobs.join("segment.ts"), b"").unwrap();
        prepare_work_dir(&shared, false).unwrap();
        // A junction in place of one of its folders (any account may make
        // one where it may write) is taken away, not followed: what it led
        // to isn't pruned, nor written to, nor given other permissions.
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("keep.txt"), b"x").unwrap();
        for folder in ["trickplay", "live"] {
            std::fs::remove_dir_all(shared.join(folder)).unwrap();
            let status = std::process::Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(shared.join(folder))
                .arg(&elsewhere)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }
        prepare_work_dir(&shared, false).unwrap();
        for folder in ["trickplay", "live"] {
            let meta = std::fs::symlink_metadata(shared.join(folder)).unwrap();
            assert!(meta.is_dir() && !meta.file_type().is_symlink(), "{folder}");
        }
        assert!(elsewhere.join("keep.txt").exists() && !own(&elsewhere));
        // One put back between making a folder and keeping it private is
        // refused, and what it leads to keeps its permissions.
        let link = shared.join("put-back");
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&elsewhere)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(windows_private::keep(&link, false).is_err());
        assert!(!own(&elsewhere));
        // In one it made, a cache that is a junction is this account's, kept
        // on another disk say: it stays.
        std::fs::remove_dir_all(made.join("trickplay")).unwrap();
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(made.join("trickplay"))
            .arg(&elsewhere)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        prepare_work_dir(&made, false).unwrap();
        assert!(std::fs::symlink_metadata(made.join("trickplay")).unwrap().file_type().is_symlink());
    }

    #[test]
    fn the_work_folder_is_the_users_own() {
        let dir = default_work_dir();
        assert!(dir.ends_with("pley-transcoder"));
        if std::env::var_os("HOME").is_some() || std::env::var_os("LOCALAPPDATA").is_some() {
            assert!(!dir.starts_with(std::env::temp_dir()) || cfg!(windows), "{}", dir.display());
        }
    }
}
