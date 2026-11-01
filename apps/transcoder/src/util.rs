// SPDX-License-Identifier: AGPL-3.0-only
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// Stable cache key for a file: hash of path, size and modification time, so a
/// replaced file never hits stale caches.
pub fn file_key(path: &Path) -> std::io::Result<String> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(meta.len().to_le_bytes());
    hasher.update(mtime.to_le_bytes());
    Ok(hex::encode(&hasher.finalize()[..16]))
}

/// Write `data` to `target` through a temporary file of its own in the same
/// folder, flushed to disk before it is renamed into place: a crash or a full
/// disk never leaves a truncated file under the final name, and concurrent
/// writers never mix. Blocking.
pub fn write_atomically(target: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    let tmp = target.with_file_name(name);
    let result = std::fs::File::create(&tmp)
        .and_then(|mut file| {
            file.write_all(data)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, target));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Mark a cache file as used now, so cache pruning keeps it (see
/// `cache.rs`). At most once a day per file: it costs a write.
pub fn touch(path: &Path) {
    const EVERY: std::time::Duration = std::time::Duration::from_secs(24 * 3600);
    let recent = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().map(|age| age < EVERY).unwrap_or(true));
    if !recent && let Ok(file) = std::fs::File::options().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Seconds with millisecond precision, the way ffmpeg expects time arguments.
pub fn secs(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// Demuxers allowed for files from the libraries: containers, audio and
/// subtitle formats. Playlists and the like (hls, dash, imf, concat, image
/// sequences, avisynth, network formats) are left out: a library file with
/// one of those inside, whatever its name, would make FFmpeg open other
/// files on the disk.
pub const MEDIA_DEMUXERS: &str = concat!(
    // Video containers and raw streams.
    "matroska,mov,avi,asf,mpegts,mpegtsraw,mpeg,mpegvideo,flv,ogg,wtv,nut,mxf,rm,dv,ivf,obu,m4v,h264,hevc,vc1,",
    // Audio.
    "mp3,flac,wav,w64,aiff,caf,au,aac,loas,ac3,eac3,dts,dtshd,truehd,mlp,spdif,ape,wv,dsf,iff,tak,tta,mpc,mpc8,amr,",
    // Subtitles.
    "srt,ass,webvtt,microdvd,subviewer,subviewer1,mpsub,mpl2,sami,sup,vobsub,pjs,realtext,jacosub,aqtitle,stl,scc"
);

/// Input options for a library file: only [`MEDIA_DEMUXERS`] may open it.
pub const MEDIA_INPUT_OPTIONS: [&str; 2] = ["-format_whitelist", MEDIA_DEMUXERS];

/// `file:` prefixed path so ffmpeg never interprets it as a protocol or option.
pub fn input_arg(path: &Path) -> OsString {
    let mut arg = OsString::from("file:");
    arg.push(path.as_os_str());
    arg
}

/// Homebrew's ffmpeg-full, which is never on `PATH` (it is keg-only): its
/// FFmpeg has the tone mappers and subtitle burn-in Homebrew's ffmpeg leaves
/// out. Apple silicon's folder, then Intel's.
const MACOS_BEFORE_PATH: [&str; 2] = ["/opt/homebrew/opt/ffmpeg-full/bin", "/usr/local/opt/ffmpeg-full/bin"];
/// Homebrew's own folders, which an app started from the Finder doesn't
/// have on `PATH`.
const MACOS_AFTER_PATH: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// Resolve a program name through `PATH` (and `PATHEXT` on Windows); on
/// macOS, Homebrew's ffmpeg-full first and its folders last.
pub fn resolve_executable(program: &Path) -> Option<PathBuf> {
    if program.components().count() > 1 || program.is_absolute() {
        return program.is_file().then(|| program.to_path_buf());
    }
    let homebrew = |dirs: [&str; 2]| {
        dirs.into_iter().map(|dir| Path::new(dir).join(program)).find(|candidate| candidate.is_file())
    };
    if cfg!(target_os = "macos") {
        if let Some(found) = homebrew(MACOS_BEFORE_PATH) {
            return Some(found);
        }
        return on_path(program).or_else(|| homebrew(MACOS_AFTER_PATH));
    }
    on_path(program)
}

fn on_path(program: &Path) -> Option<PathBuf> {
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(program);
        if candidate.is_file() && (!cfg!(windows) || candidate.extension().is_some()) {
            return Some(candidate);
        }
        for ext in &exts {
            let mut name = program.as_os_str().to_owned();
            name.push(ext);
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Apply platform tweaks to child processes: no console window on Windows; on
/// Linux the child is killed when the transcoder dies without cleaning up.
pub fn configure_child(cmd: &mut tokio::process::Command) {
    cmd.kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(target_os = "linux")]
    {
        let parent = std::process::id() as libc::pid_t;
        // SAFETY: the closure only makes async-signal-safe calls (prctl,
        // getppid, _exit) between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // The transcoder may have exited before the signal was armed.
                if libc::getppid() != parent {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
    }
}

/// Run a child at reduced CPU priority (background analysis that must never
/// slow down playback). Call after [`configure_child`].
pub fn background_priority(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: setpriority is async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
                Ok(())
            });
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = cmd;
}

/// Make every child process die with the transcoder, whatever ends it.
///
/// Windows: the transcoder joins a job object that kills all of its members
/// when its last handle closes, which happens when this process exits for any
/// reason (including a crash or a forced kill). ffmpeg and ffprobe inherit the
/// job. Linux uses a parent-death signal per child instead (see
/// [`configure_child`]).
pub fn kill_children_on_exit() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        // SAFETY: plain Win32 calls on a handle owned here. The job handle is
        // deliberately never closed: it must live as long as the process.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                tracing::warn!("could not create a job object: {}", std::io::Error::last_os_error());
                return;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0;
            if !configured || AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
                tracing::warn!(
                    "could not tie ffmpeg processes to the transcoder's lifetime: {}",
                    std::io::Error::last_os_error()
                );
                CloseHandle(job);
            }
        }
    }
}

/// The logical processors the transcoder (and the FFmpeg it starts) may
/// run on. On Windows the standard library counts them all, whatever the
/// process's affinity (`start /affinity`, a job object) says; FFmpeg counts
/// what the affinity allows, and so does this.
pub fn processors() -> u32 {
    let all = std::thread::available_parallelism().map_or(4, |n| n.get() as u32);
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessAffinityMask};
        let (mut process, mut system) = (0usize, 0usize);
        // SAFETY: plain Win32 call with the pseudo handle of this process.
        let ok = unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut process, &mut system) } != 0;
        let allowed = process.count_ones();
        if ok && allowed > 0 {
            return all.min(allowed);
        }
    }
    all
}

/// Total size of all files below `dir`.
pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(_) => total += entry.metadata().map(|m| m.len()).unwrap_or(0),
                Err(_) => {}
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_seconds() {
        assert_eq!(secs(0), "0.000");
        assert_eq!(secs(4000), "4.000");
        assert_eq!(secs(123_456), "123.456");
        assert_eq!(secs(-5), "0.000");
    }

    #[test]
    fn input_arg_is_prefixed() {
        assert_eq!(input_arg(Path::new("/m/a.mkv")), OsString::from("file:/m/a.mkv"));
        // A file named like an option or a protocol stays a file.
        assert_eq!(input_arg(Path::new("-i")), OsString::from("file:-i"));
        assert_eq!(input_arg(Path::new("http://x")), OsString::from("file:http://x"));
    }

    #[test]
    fn file_keys_change_with_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mkv");
        let b = dir.path().join("b.mkv");
        std::fs::write(&a, b"one").unwrap();
        std::fs::write(&b, b"one").unwrap();
        let key = file_key(&a).unwrap();
        assert_eq!(key.len(), 32);
        assert_eq!(file_key(&a).unwrap(), key, "stable");
        assert_ne!(file_key(&b).unwrap(), key, "another path");
        std::fs::write(&a, b"longer").unwrap();
        assert_ne!(file_key(&a).unwrap(), key, "a replaced file");
        assert!(file_key(&dir.path().join("missing.mkv")).is_err());
    }

    #[test]
    fn files_are_replaced_whole_or_not_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.vtt");
        write_atomically(&target, b"WEBVTT\n\nfirst").unwrap();
        write_atomically(&target, b"WEBVTT\n\nsecond").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"WEBVTT\n\nsecond");
        // A failure leaves nothing behind, under either name.
        let blocked = dir.path().join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        assert!(write_atomically(&blocked, b"WEBVTT").is_err(), "a folder is in the way");
        assert!(write_atomically(&dir.path().join("missing").join("b.vtt"), b"WEBVTT").is_err());
        let mut names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().map(|e| e.file_name()).collect();
        names.sort();
        assert_eq!(names, vec![OsString::from("a.vtt"), OsString::from("blocked")]);
    }

    #[test]
    fn touching_marks_files_used_at_most_daily() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"x").unwrap();
        let set = |ago: u64| {
            let when = SystemTime::now() - std::time::Duration::from_secs(ago);
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
        };
        let age = || std::fs::metadata(&path).unwrap().modified().unwrap().elapsed().unwrap().as_secs();
        set(3 * 24 * 3600);
        touch(&path);
        assert!(age() < 60, "used now");
        set(3600);
        touch(&path);
        assert!(age() >= 3600, "used an hour ago: not worth a write");
        touch(&dir.path().join("missing"));
    }

    #[test]
    fn dir_sizes_count_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("jobs/abc")).unwrap();
        std::fs::write(dir.path().join("a"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.path().join("jobs/abc/0.ts"), vec![0u8; 100]).unwrap();
        assert_eq!(dir_size(dir.path()), 110);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }

    #[test]
    fn explicit_paths_are_used_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("ffmpeg-custom");
        std::fs::write(&tool, b"").unwrap();
        assert_eq!(resolve_executable(&tool), Some(tool.clone()));
        assert_eq!(resolve_executable(&dir.path().join("missing")), None);
        assert_eq!(resolve_executable(Path::new("no-such-program-pley-test")), None);
    }
}
