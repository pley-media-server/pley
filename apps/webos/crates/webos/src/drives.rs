//! The USB drive plugged into the TV (the Samsung app's `Kit/Drives.cs`): the first one mounted,
//! followed as drives come and go, on a thread of its own (`pley-drives`).
//!
//! webOS mounts a drive's partitions at `/tmp/usb/<disk>/<partition>` (`/tmp/usb/sda/sda1`), and
//! the letters move with what is plugged in and in what order, so the drive is found by looking,
//! never by name (<https://www.webosbrew.org/develop/guides/filesystem/>;
//! <https://gist.github.com/maatthc/792471812150010c935aedd34c93aedf>). The app runs in LG's
//! jail, a chroot whose mounts are its own (`jail_native_devmode.conf`,
//! <https://github.com/lennylxx/lg-devmode-forever>): `/tmp` is the TV's, but whether a drive's
//! mount shows inside it is the jail's to say. The app's own mount table answers that:
//! `/proc/self/mountinfo` lists the mounts the app sees, with their points relative to its root
//! (proc(5), <https://man7.org/linux/man-pages/man5/proc_pid_mountinfo.5.html>). A partition
//! folder under `/tmp/usb` that isn't one of them is a drive the TV mounted and the jail doesn't
//! show: [`Drive::hidden`].
//!
//! The watcher waits on `/proc/self/mounts`, which wakes `poll` with `POLLPRI` when a mount comes
//! or goes (proc(5), <https://man7.org/linux/man-pages/man5/proc_pid_mounts.5.html>), and looks
//! again every 3 s besides. It also subscribes to `com.webos.service.pdm`, whose news of drives
//! is documented for webOS OSE (<https://www.webosose.org/docs/reference/ls2-api/com-webos-service-pdm/>)
//! behind the `physicaldevice.query` group, which LG's TV group table doesn't list
//! (<https://webostv.developer.lge.com/develop/guides/acg-guide>): any answer only makes it look
//! again, and a refusal leaves the watcher as it was.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pley_core::Drive;
use serde_json::json;

use crate::luna::{Luna, Subscription};

/// How often the drives are looked at when the mount table says nothing.
pub const LOOK_EVERY: Duration = Duration::from_secs(3);
/// The most of the mount table read: a TV's is a few KB.
const MOUNTINFO_MOST: u64 = 1024 * 1024;
/// The most disks, and partitions of each, looked at under `/tmp/usb`.
const FOLDERS_MOST: usize = 32;
/// pdm's drive list, for news of drives coming and going.
const PDM: &str = "luna://com.webos.service.pdm/getAttachedStorageDeviceList";

/// The mount points of `/proc/self/mountinfo`'s text, in mount order: each line's fifth field,
/// with the kernel's octal escapes (`\040` a space, `\011` a tab, `\012` a new line, `\134` a
/// backslash) undone.
pub fn mount_points(mountinfo: &str) -> Vec<PathBuf> {
    mountinfo.lines().filter_map(|line| line.split(' ').nth(4)).map(|point| PathBuf::from(unescape(point))).collect()
}

/// `\ooo` as the byte it names; anything else as it is. Bytes that aren't UTF-8 are replaced: a
/// drive's mount point is the kernel's device name.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let octal = b.get(i + 1..i + 4).filter(|d| b[i] == b'\\' && d.iter().all(|c| (b'0'..=b'7').contains(c)));
        match octal.map(|d| d.iter().fold(0u32, |n, c| n * 8 + u32::from(c - b'0'))) {
            Some(n) if n <= 0xFF => {
                out.push(n as u8);
                i += 4;
            }
            _ => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The drive the app has now: the first mount under `usb` in mount order whose folder can be
/// read; else a partition folder under it that is mounted all the same (a mount table that
/// couldn't be read); else one the TV made for a drive the app doesn't see (hidden); else none.
pub fn look(mountinfo: &str, usb: &Path) -> Option<Drive> {
    let readable = |p: &Path| fs::read_dir(p).is_ok();
    if let Some(root) = mount_points(mountinfo).into_iter().find(|p| p.starts_with(usb) && p != usb && readable(p)) {
        return Some(Drive { root, hidden: false });
    }
    let mut hidden = None;
    for disk in folders(usb) {
        for partition in folders(&disk) {
            if mounted_apart(&partition, usb) && readable(&partition) {
                return Some(Drive { root: partition, hidden: false });
            }
            hidden.get_or_insert(partition);
        }
    }
    hidden.map(|root| Drive { root, hidden: true })
}

/// The folders in `dir`, by name, at most [`FOLDERS_MOST`].
fn folders(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .take(FOLDERS_MOST * 4)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    found.sort();
    found.truncate(FOLDERS_MOST);
    found
}

/// Whether `folder` is on another file system than `usb` (something is mounted there).
#[cfg(unix)]
fn mounted_apart(folder: &Path, usb: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(folder), fs::metadata(usb)) {
        (Ok(a), Ok(b)) => a.dev() != b.dev(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn mounted_apart(_folder: &Path, _usb: &Path) -> bool {
    false
}

/// Reads the mount table under `root` (`/` on the TV), at most [`MOUNTINFO_MOST`].
fn read_mountinfo(root: &Path) -> String {
    let mut text = String::new();
    if let Ok(file) = fs::File::open(root.join("proc/self/mountinfo")) {
        let _ = file.take(MOUNTINFO_MOST).read_to_string(&mut text);
    }
    text
}

/// Follows the drives under `root` (`/` on the TV): `changed` hears the drive whenever it
/// differs from the last, first with the one there at start (none included), while `wanted`
/// says the device is still there to hear it. pdm's news, when the TV gives it, makes it look at
/// once.
pub fn watch(
    root: &Path,
    luna: Option<&Luna>,
    wanted: impl Fn() -> bool + Send + 'static,
    mut changed: impl FnMut(Option<Drive>) + Send + 'static,
) {
    let root = root.to_path_buf();
    let wake = Arc::new(Wake::new());
    let news = luna.map(|luna| pdm_news(luna, wake.clone()));
    let spawned = std::thread::Builder::new().name("pley-drives".into()).spawn(move || {
        // pdm's subscription lasts as long as the watcher.
        let _news: Option<Subscription> = news;
        let usb = root.join(pley_core::device::USB_ROOT.trim_start_matches('/'));
        let mounts = root.join("proc/self/mounts");
        let mut last: Option<Option<Drive>> = None;
        while wanted() {
            let now = look(&read_mountinfo(&root), &usb);
            if last.as_ref() != Some(&now) {
                match &now {
                    Some(d) if d.hidden => crate::log::line(&format!(
                        "drives: {} is plugged in, but the app's jail doesn't show it",
                        d.root.display()
                    )),
                    Some(d) => crate::log::line(&format!("drives: {} is the drive", d.root.display())),
                    None if last.is_some() => crate::log::line("drives: the drive was pulled out"),
                    None => {}
                }
                changed(now.clone());
                last = Some(now);
            }
            wake.wait(&mounts, LOOK_EVERY);
        }
    });
    if let Err(e) = spawned {
        crate::log::line(&format!("drives: no watcher ({e})"));
    }
}

/// pdm's drive list, subscribed to: any answer wakes the watcher; a refusal (a Developer Mode app
/// may not ask) is logged once and changes nothing.
fn pdm_news(luna: &Luna, wake: Arc<Wake>) -> Subscription {
    let mut said = false;
    luna.subscribe(PDM, &json!({}), Duration::from_secs(5), move |answer| match answer {
        Ok(_) => wake.now(),
        Err(e) if !said => {
            said = true;
            crate::log::line(&format!("drives: pdm doesn't answer the app ({e}); the mount table is watched"));
        }
        Err(_) => {}
    })
}

/// What the watcher sleeps on: a mount coming or going, pdm's news, or [`LOOK_EVERY`].
struct Wake {
    #[cfg(unix)]
    pipe: Option<(std::os::fd::OwnedFd, std::os::fd::OwnedFd)>,
}

#[cfg(unix)]
impl Wake {
    fn new() -> Wake {
        use std::os::fd::{FromRawFd, OwnedFd};
        let mut fds = [0; 2];
        // SAFETY: `fds` has room for the two descriptors pipe(2) writes.
        let pipe = (unsafe { libc::pipe(fds.as_mut_ptr()) } == 0)
            // SAFETY: pipe(2) just made both descriptors and nothing else owns them.
            .then(|| unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) });
        if let Some((read, _)) = &pipe {
            use std::os::fd::AsRawFd;
            // SAFETY: a valid descriptor; non-blocking, so draining it never waits.
            unsafe { libc::fcntl(read.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) };
        }
        Wake { pipe }
    }

    /// Wakes the watcher (from pdm's answer, on the luna thread).
    fn now(&self) {
        use std::os::fd::AsRawFd;
        if let Some((_, write)) = &self.pipe {
            // SAFETY: a valid descriptor and a one-byte buffer; a full pipe already wakes it.
            unsafe { libc::write(write.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
        }
    }

    /// Waits for a mount to come or go in `mounts` (`POLLPRI`), the pipe, or `most`.
    fn wait(&self, mounts: &Path, most: Duration) {
        use std::os::fd::AsRawFd;
        let table = fs::File::open(mounts).ok();
        let mut fds = Vec::with_capacity(2);
        if let Some(t) = &table {
            fds.push(libc::pollfd { fd: t.as_raw_fd(), events: libc::POLLPRI, revents: 0 });
        }
        if let Some((read, _)) = &self.pipe {
            fds.push(libc::pollfd { fd: read.as_raw_fd(), events: libc::POLLIN, revents: 0 });
        }
        if fds.is_empty() {
            std::thread::sleep(most);
            return;
        }
        // SAFETY: `fds` holds valid descriptors that outlive the call.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, most.as_millis() as libc::c_int) };
        if n < 0 {
            // Interrupted: the next look comes after the usual wait.
            std::thread::sleep(most);
        }
        if let Some((read, _)) = &self.pipe {
            let mut sink = [0u8; 64];
            // SAFETY: a valid non-blocking descriptor and a buffer of its length.
            while unsafe { libc::read(read.as_raw_fd(), sink.as_mut_ptr().cast(), sink.len()) } > 0 {}
        }
    }
}

#[cfg(not(unix))]
impl Wake {
    fn new() -> Wake {
        Wake {}
    }

    fn now(&self) {}

    fn wait(&self, _mounts: &Path, most: Duration) {
        std::thread::sleep(most);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mount_table_gives_its_points_unescaped() {
        let table = "\
22 1 179:2 / / rw,relatime shared:1 - ext4 /dev/root rw
36 22 0:30 / /tmp rw,nosuid shared:7 - tmpfs tmpfs rw
81 36 8:1 / /tmp/usb/sda/sda1 rw,noatime - vfat /dev/sda1 rw
82 36 8:17 / /tmp/usb/sdb/My\\040Drive\\134x rw - exfat /dev/sdb1 rw
short line
";
        let points = mount_points(table);
        assert_eq!(points.len(), 4);
        assert_eq!(points[2], PathBuf::from("/tmp/usb/sda/sda1"));
        assert_eq!(points[3], PathBuf::from("/tmp/usb/sdb/My Drive\\x"));
        assert_eq!(unescape("a\\011b\\012c\\0"), "a\tb\nc\\0");
        assert_eq!(unescape("\\777"), "\\777");
    }

    #[test]
    fn the_first_mounted_drive_the_app_can_read_is_the_drive() {
        let root = pley_core::device::scratch_dir("drives");
        let usb = root.join("tmp/usb");
        for p in ["sda/sda1", "sdb/sdb1"] {
            fs::create_dir_all(usb.join(p)).unwrap();
        }
        let line = |n: u32, p: &Path| {
            format!("{n} 36 8:1 / {} rw - vfat /dev/x rw\n", p.display().to_string().replace(' ', "\\040"))
        };
        // The second mounted is first in the table's order, as the kernel lists them.
        let table = [line(81, &usb.join("sdb/sdb1")), line(82, &usb.join("sda/sda1"))].concat();
        assert_eq!(look(&table, &usb), Some(Drive { root: usb.join("sdb/sdb1"), hidden: false }));
        // One that can't be read is passed over.
        let gone = [line(80, &usb.join("sdc/sdc1")), line(82, &usb.join("sda/sda1"))].concat();
        assert_eq!(look(&gone, &usb), Some(Drive { root: usb.join("sda/sda1"), hidden: false }));
        // Mounts elsewhere aren't drives, nor is /tmp/usb itself.
        assert_eq!(look(&line(1, &usb), &root.join("nowhere")), None);
    }

    #[test]
    fn a_drive_the_tv_mounted_outside_the_jail_is_plugged_in_but_hidden() {
        let root = pley_core::device::scratch_dir("drives-hidden");
        let usb = root.join("tmp/usb");
        assert_eq!(look("", &usb), None);
        fs::create_dir_all(&usb).unwrap();
        assert_eq!(look("", &usb), None);
        fs::create_dir_all(usb.join("sdb/sdb1")).unwrap();
        fs::create_dir_all(usb.join("sda/sda1")).unwrap();
        // Folders the TV made, on /tmp's own file system: the mounts aren't in the app's view.
        assert_eq!(look("", &usb), Some(Drive { root: usb.join("sda/sda1"), hidden: true }));
    }
}
