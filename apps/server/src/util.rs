use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// When this server process started (unix ms): part of tags that a newer
/// version may answer differently.
pub fn process_epoch() -> i64 {
    static EPOCH: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *EPOCH.get_or_init(now_ms)
}

/// `%XX` escapes decoded (and `+` as a space when `plus`); anything else is
/// kept as it is.
pub fn percent_decode(s: &str, plus: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b'+', ..) if plus => {
                out.push(b' ');
                i += 1;
            }
            (b, ..) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether `a` and `b` are equal, taking as long whichever byte differs
/// (secrets compared with what someone sent).
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// Current unix time in seconds.
pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or_default()
}

/// Current unix time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_default()
}

/// How fast a count goes up, from where it was first seen, to tell about
/// how long the rest of it takes.
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    since: Instant,
    from: u64,
}

impl Pace {
    /// A pace measured from now, with `done` done.
    pub fn new(done: u64) -> Self {
        Pace { since: Instant::now(), from: done }
    }

    /// A pace measured since `since`, from `done` done.
    #[cfg(test)]
    pub fn measured_since(since: Instant, done: u64) -> Self {
        Pace { since, from: done }
    }

    /// About how many seconds the rest of `total` takes, with `done` done.
    pub fn left(&self, done: u64, total: u64) -> Option<u64> {
        time_left(self.since.elapsed().as_secs_f64(), done.saturating_sub(self.from), total.saturating_sub(done))
    }
}

/// How long `left` more take at the pace of `done` in `elapsed_secs`,
/// rounded to the second: none before the pace can be told.
fn time_left(elapsed_secs: f64, done: u64, left: u64) -> Option<u64> {
    /// The time a pace is measured over before it's trusted.
    const MEASURED_FOR: f64 = 10.0;
    (elapsed_secs >= MEASURED_FOR && done > 0).then(|| (left as f64 * elapsed_secs / done as f64).round() as u64)
}

pub fn system_time_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or_default()
}

/// `YYYY-MM-DD` (and whatever follows it) → unix seconds (UTC midnight).
pub fn date_secs(date: &str) -> Option<i64> {
    let mut parts = date.get(..10)?.split('-');
    let (y, m, d): (i64, i64, i64) =
        (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

/// A UTC time as `2024-05-01T12:30:45Z` (fractions of a second left out)
/// → unix seconds.
pub fn utc_secs(time: &str) -> Option<i64> {
    let day = date_secs(time)?;
    let clock = time.get(10..)?.strip_prefix('T')?.trim_end_matches('Z');
    let mut parts = clock.split('.').next()?.split(':');
    let (h, m, s): (i64, i64, i64) =
        (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    if parts.next().is_some() || h > 23 || m > 59 || s > 60 {
        return None;
    }
    Some(day + h * 3600 + m * 60 + s)
}

/// The UTC date and time of unix time `secs`: year, month, day, hour,
/// minute and second.
pub fn utc(secs: i64) -> [i64; 6] {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    [year, month, day, rem / 3600, rem % 3600 / 60, rem % 60]
}

/// Keeps the folder `dir` to the account pley runs as: pley's data holds
/// password hashes, sessions, backups and the log, and its settings name
/// the programs pley starts. On Unix that is mode 0700 whatever the umask.
/// On Windows, a user's own folders are private already, but one outside
/// the profile (on D:\, say) lets every account on the computer in: then
/// only this account, SYSTEM and the administrators keep it (a service's
/// folder is kept so by `pley service install`).
pub fn keep_private(dir: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let open = std::fs::metadata(dir).is_ok_and(|meta| meta.permissions().mode() & 0o077 != 0);
        if open && let Err(err) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)) {
            tracing::warn!(
                "other users of this computer can read {}, and Pley can't change that: {err}",
                dir.display()
            );
        }
    }
    #[cfg(windows)]
    if let Err(err) = windows_acl::keep_private(dir) {
        tracing::warn!("other users of this computer can reach {}, and Pley can't change that: {err:#}", dir.display());
    }
    #[cfg(not(any(unix, windows)))]
    let _ = dir;
}

/// Warns when every account on this computer may change the folder pley's
/// programs are in (pley and its transcoder): any of them could replace
/// them, and run as pley's account, a service's included. Unpacked to the
/// top of a drive (`C:\pley`), a folder inherits the drive's permissions,
/// which let every account add and change files; Program Files and a user's
/// own folders don't.
pub fn check_program_folder() {
    #[cfg(windows)]
    {
        let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) else {
            return;
        };
        if windows_acl::dacl(&dir).is_ok_and(|sddl| windows_acl::writable_by_everyone(&sddl)) {
            tracing::warn!(
                "every account on this computer can change the programs in {}: any of them could replace Pley \
                 with its own and run as Pley's account. Install Pley to Program Files, or take away the \
                 Users' and Authenticated Users' right to change that folder",
                dir.display()
            );
        }
    }
}

#[cfg(windows)]
mod windows_acl {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::ptr::null_mut;

    use anyhow::Context;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetTokenInformation, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    /// Groups every account on the computer is in, as SDDL and as SIDs:
    /// Everyone, Authenticated Users, Users, Interactive.
    const EVERYONE: [(&str, &str); 4] =
        [("WD", "S-1-1-0"), ("AU", "S-1-5-11"), ("BU", "S-1-5-32-545"), ("IU", "S-1-5-4")];

    pub fn keep_private(dir: &Path) -> anyhow::Result<()> {
        if !open_to_everyone(&dacl(dir)?) {
            return Ok(());
        }
        let me = own_sid()?;
        let grants =
            [format!("*{me}:(OI)(CI)F"), "*S-1-5-18:(OI)(CI)F".to_owned(), "*S-1-5-32-544:(OI)(CI)F".to_owned()];
        // What it allows is all there is: nothing inherited from the drive…
        let mut args = vec!["/inheritance:r", "/grant:r"];
        args.extend(grants.iter().map(String::as_str));
        icacls(dir, &args)?;
        // …and nothing given to everyone on the folder itself.
        let everyone: Vec<String> = EVERYONE.iter().map(|(_, sid)| format!("*{sid}")).collect();
        let mut args = vec!["/remove:g"];
        args.extend(everyone.iter().map(String::as_str));
        icacls(dir, &args)?;
        tracing::info!("{} is now kept to the account Pley runs as", dir.display());
        Ok(())
    }

    /// Whether an entry of `sddl` (a folder's DACL) allows anything to
    /// every account on the computer.
    pub(super) fn open_to_everyone(sddl: &str) -> bool {
        sddl.split(['(', ')']).filter(|ace| ace.starts_with("A;")).any(|ace| {
            ace.rsplit(';')
                .next()
                .is_some_and(|trustee| EVERYONE.iter().any(|(name, sid)| trustee == *name || trustee == *sid))
        })
    }

    /// Whether an entry of `sddl` (a folder's DACL) that applies to the
    /// folder itself lets every account on the computer change it: add or
    /// replace files in it (writing, deleting them), or its permissions.
    pub(super) fn writable_by_everyone(sddl: &str) -> bool {
        sddl.split(['(', ')']).filter(|ace| ace.starts_with("A;")).any(|ace| {
            let fields: Vec<&str> = ace.split(';').collect();
            let [_, flags, rights, .., trustee] = fields[..] else { return false };
            !flags.contains("IO")
                && EVERYONE.iter().any(|(name, sid)| trustee == *name || trustee == *sid)
                && access_mask(rights) & CHANGES != 0
        })
    }

    /// Rights that change a folder or what is in it: add a file
    /// (`FILE_ADD_FILE`), delete one (`FILE_DELETE_CHILD`), delete or
    /// rename the folder, rewrite its permissions or owner, and the generic
    /// rights that include them.
    const CHANGES: u32 = 0x2 | 0x40 | 0x1_0000 | 0x4_0000 | 0x8_0000 | 0x1000_0000 | 0x4000_0000;

    /// An SDDL access mask: hexadecimal, or two-letter rights.
    fn access_mask(rights: &str) -> u32 {
        if let Some(hex) = rights.strip_prefix("0x") {
            return u32::from_str_radix(hex, 16).unwrap_or(0);
        }
        rights
            .as_bytes()
            .chunks(2)
            .map(|right| match right {
                b"GA" => 0x1000_0000,
                b"GW" => 0x4000_0000,
                b"FA" => 0x1F_01FF,
                b"FW" => 0x12_0116,
                b"SD" => 0x1_0000,
                b"WD" => 0x4_0000,
                b"WO" => 0x8_0000,
                b"DC" => 0x2,
                b"LC" => 0x4,
                b"DT" => 0x40,
                _ => 0,
            })
            .fold(0, |mask, right| mask | right)
    }

    fn icacls(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
        let system = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let output = std::process::Command::new(Path::new(&system).join("System32").join("icacls.exe"))
            .arg(dir)
            .args(args)
            .arg("/Q")
            .output()
            .context("running icacls")?;
        anyhow::ensure!(
            output.status.success(),
            "icacls {}: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }

    /// `dir`'s DACL, as SDDL.
    pub(super) fn dacl(dir: &Path) -> anyhow::Result<String> {
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: a NUL-terminated path; Windows allocates the descriptor, freed below.
        let status = unsafe {
            GetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut descriptor,
            )
        };
        anyhow::ensure!(status == ERROR_SUCCESS, "reading who may use it: {}", os_error(status));
        let mut text = null_mut();
        // SAFETY: the descriptor read above; Windows allocates the string, freed below.
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                null_mut(),
            )
        };
        let error = std::io::Error::last_os_error();
        // SAFETY: a NUL-terminated string from Windows, when it gave one.
        let sddl = (ok != 0).then(|| unsafe { wide_string(text) });
        // SAFETY: both allocated by Windows with LocalAlloc (or null).
        unsafe {
            LocalFree(text.cast());
            LocalFree(descriptor);
        }
        sddl.ok_or_else(|| anyhow::anyhow!("reading who may use it: {error}"))
    }

    /// The SID of the account pley runs as.
    fn own_sid() -> anyhow::Result<String> {
        let mut token: HANDLE = null_mut();
        // SAFETY: this process's pseudo-handle; the token is closed below.
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            anyhow::bail!("opening this process's token: {}", std::io::Error::last_os_error());
        }
        // Aligned for TOKEN_USER, and large enough for any SID.
        let mut buf = [0u64; 64];
        let mut len = 0u32;
        // SAFETY: a buffer of the size given.
        let ok = unsafe {
            GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), size_of_val(&buf) as u32, &mut len)
        };
        let error = std::io::Error::last_os_error();
        // SAFETY: the token opened above.
        unsafe { CloseHandle(token) };
        anyhow::ensure!(ok != 0, "reading this process's account: {error}");
        // SAFETY: GetTokenInformation wrote a TOKEN_USER at the start of `buf`.
        let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text = null_mut();
        // SAFETY: a valid SID; Windows allocates the string, freed below.
        if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
            anyhow::bail!("reading this process's account: {}", std::io::Error::last_os_error());
        }
        // SAFETY: a NUL-terminated string from Windows.
        let result = unsafe { wide_string(text) };
        // SAFETY: allocated by Windows with LocalAlloc.
        unsafe { LocalFree(text.cast()) };
        Ok(result)
    }

    fn os_error(code: u32) -> std::io::Error {
        std::io::Error::from_raw_os_error(code as i32)
    }

    /// # Safety
    /// `text` is a NUL-terminated UTF-16 string.
    unsafe fn wide_string(text: *const u16) -> String {
        // SAFETY: up to and without the NUL, which the caller promises.
        let len = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
        // SAFETY: the `len` units just read.
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) })
    }
}

/// Lossy UTF-8 string of a path (paths are stored as text in the database).
/// Scans leave out names that are not valid UTF-8 (library/walk.rs), so the
/// paths of library files convert back exactly.
pub fn path_str(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Canonical form used for identity keys: lowercase alphanumerics separated by
/// single spaces. `"The.Office (US)!"` → `"the office us"`.
pub fn normalize_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(ch.to_lowercase());
        } else if ch == '\'' || ch == '\u{2019}' {
            // "Grey's" and "Greys" should match
        } else {
            pending_space = true;
        }
    }
    out
}

/// Sort key: lowercase without leading English articles, so "The Matrix" sorts under M.
pub fn sort_title(title: &str) -> String {
    let lower = title.trim().to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article)
            && !rest.trim().is_empty()
        {
            return rest.trim_start().to_owned();
        }
    }
    lower
}

/// Deletes the files in the folders of `dir` older than `drop_after`, then
/// the oldest while they take more than `max_bytes` (down to nine tenths of
/// it): how many went. For caches of pictures (Live TV's, Discover's).
pub fn prune(dir: &Path, drop_after: Duration, max_bytes: u64) -> anyhow::Result<usize> {
    let mut files = Vec::new();
    let Ok(subdirs) = std::fs::read_dir(dir) else { return Ok(0) };
    for sub in subdirs.flatten() {
        let Ok(entries) = std::fs::read_dir(sub.path()) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_file() {
                let age = meta.modified().ok().and_then(|t| t.elapsed().ok()).unwrap_or_default();
                files.push((entry.path(), age, meta.len()));
            }
        }
    }
    let mut gone = 0;
    let mut total: u64 = files.iter().map(|f| f.2).sum();
    let keep = if total > max_bytes { max_bytes / 10 * 9 } else { u64::MAX };
    // Oldest first: the stale ones, then as many more as are too many.
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    for (path, age, len) in files {
        if age <= drop_after && total <= keep {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            gone += 1;
            total -= len;
        }
    }
    Ok(gone)
}

/// Run a blocking closure on the blocking pool and flatten the join error.
pub async fn blocking<T, F>(f: F) -> anyhow::Result<T>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("blocking task failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_left_is_told_from_the_pace_once_it_was_measured_long_enough() {
        // 500 files in 50 seconds: 1,500 more take 150.
        assert_eq!(time_left(50.0, 500, 1500), Some(150));
        assert_eq!(time_left(5.0, 500, 1500), None, "too soon to tell");
        assert_eq!(time_left(50.0, 0, 1500), None, "none done yet");
        assert_eq!(time_left(50.0, 500, 0), Some(0));
    }

    #[test]
    fn utc_times_are_read() {
        assert_eq!(utc_secs("1970-01-02T00:00:01Z"), Some(86_401));
        assert_eq!(utc_secs("2024-05-01T12:30:45.1234567Z"), Some(date_secs("2024-05-01").unwrap() + 45_045));
        assert_eq!(utc(utc_secs("2024-02-29T23:59:59Z").unwrap()), [2024, 2, 29, 23, 59, 59]);
        for wrong in ["2024-05-01", "2024-05-01T12:30Z", "2024-05-01T25:00:00Z", "2024-05-01T12:30:45+02:00", ""] {
            assert_eq!(utc_secs(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn pictures_go_when_stale_or_too_many() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("ab");
        std::fs::create_dir_all(&sub).unwrap();
        let file = |name: &str, len: usize, days_old: u64| {
            let path = sub.join(name);
            std::fs::write(&path, vec![0u8; len]).unwrap();
            let at = SystemTime::now() - Duration::from_secs(days_old * 86_400);
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(at).unwrap();
            path
        };
        let stale = file("stale.png", 10, 40);
        let (old, newer, newest) = (file("old.png", 100, 3), file("newer.png", 100, 2), file("newest.png", 100, 1));
        assert_eq!(prune(dir.path(), Duration::from_secs(30 * 86_400), 1000).unwrap(), 1, "only the stale one");
        assert!(!stale.exists() && old.exists());
        // Too many: the oldest go, down to nine tenths of what they may take.
        assert_eq!(prune(dir.path(), Duration::from_secs(30 * 86_400), 150).unwrap(), 2);
        assert!(!old.exists() && !newer.exists() && newest.exists());
        assert_eq!(prune(&dir.path().join("none"), Duration::ZERO, 0).unwrap(), 0, "nothing kept yet");
    }

    #[cfg(windows)]
    #[test]
    fn the_data_folder_is_kept_private_on_windows_too() {
        use windows_acl::{dacl, open_to_everyone};
        assert!(open_to_everyone("D:PAI(A;OICI;FA;;;SY)(A;OICIID;0x1301bf;;;AU)"));
        assert!(open_to_everyone("D:(A;OICI;0x1200a9;;;S-1-5-32-545)"));
        assert!(!open_to_everyone("D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(D;OICI;FA;;;AU)"));

        let dir = tempfile::tempdir().unwrap();
        let icacls = |args: &[&str]| {
            let status = std::process::Command::new("icacls").arg(dir.path()).args(args).arg("/Q").status().unwrap();
            assert!(status.success());
        };
        icacls(&["/grant", "*S-1-5-32-545:(OI)(CI)RX"]);
        assert!(open_to_everyone(&dacl(dir.path()).unwrap()));
        keep_private(dir.path());
        let after = dacl(dir.path()).unwrap();
        assert!(!open_to_everyone(&after), "{after}");
        // Still pley's own.
        std::fs::write(dir.path().join("pley.toml"), "").unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_program_folder_anyone_may_change_is_found() {
        use windows_acl::{dacl, writable_by_everyone};
        // Unpacked to C:\pley: the drive's Modify for Authenticated Users, inherited.
        assert!(writable_by_everyone("D:AI(A;OICIID;FA;;;BA)(A;OICIID;0x1301bf;;;AU)"));
        assert!(writable_by_everyone("D:(A;;GA;;;WD)"));
        // Program Files: everyone may read and run what is there, only.
        assert!(!writable_by_everyone(
            "D:PAI(A;OICIIO;GA;;;CO)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;;0x1200a9;;;BU)(A;OICIIO;GXGR;;;BU)"
        ));
        // The drive itself: everyone's Modify is for what is made in it, not for it.
        assert!(!writable_by_everyone("D:PAI(A;OICIIO;SDGXGWGR;;;AU)(A;;0x1200a9;;;AU)(A;OICI;FA;;;BA)"));

        let dir = tempfile::tempdir().unwrap();
        assert!(!writable_by_everyone(&dacl(dir.path()).unwrap()), "a user's own temporary folder");
        let status = std::process::Command::new("icacls")
            .arg(dir.path())
            .args(["/grant", "*S-1-5-11:(OI)(CI)M", "/Q"])
            .status()
            .unwrap();
        assert!(status.success());
        assert!(writable_by_everyone(&dacl(dir.path()).unwrap()));
    }

    #[cfg(unix)]
    #[test]
    fn the_data_folder_is_kept_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        keep_private(dir.path());
        let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn normalizes_keys() {
        assert_eq!(normalize_key("The.Office (US)!"), "the office us");
        assert_eq!(normalize_key("  Grey's Anatomy "), "greys anatomy");
        assert_eq!(normalize_key("Amélie"), "amélie");
    }

    #[test]
    fn sort_titles_drop_articles() {
        assert_eq!(sort_title("The Matrix"), "matrix");
        assert_eq!(sort_title("A Quiet Place"), "quiet place");
        assert_eq!(sort_title("The"), "the");
        assert_eq!(sort_title("Annie Hall"), "annie hall");
    }
}
