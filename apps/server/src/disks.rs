//! Which storage a media file is read from: the disk, array, pool or
//! network server behind its path. Seek previews read whole files, and two
//! whole-file reads from one hard disk, array or server go no faster in all
//! than one, since each moves the other's heads or shares its link. So the
//! previews of files on different storage are made at once, and those on
//! the same storage one at a time, or up to four from a solid-state disk. A
//! file's [`Disk`] is a key, the same for the files read from the same
//! storage, and how many files may be read from it at once.
//!
//! Keys follow whole disks:
//! - On Linux, in a container too, a file's folder is on the mount with the
//!   longest mount point above it in `/proc/self/mountinfo`. A mount's device
//!   is followed through `/sys` to the disks under it: a partition to its
//!   disk, a mapping (LVM, LUKS) or an array (md) to the disks of its
//!   members. Mounts that share a disk are one storage, keyed by the first of
//!   their disks' names (`disk:sdb`). A share is its server (`net:nas`), a ZFS
//!   dataset its pool (`zfs:tank`), and a file of Unraid's user shares or of
//!   mergerfs is on the disk the union says holds it (in the attribute
//!   `system.LOCATION` or `user.mergerfs.basepath`).
//! - On Windows, a share or a mapped drive is its server, and a volume is the
//!   disks it is on, by number (`disk:1`), with every volume on any of them.
//! - Elsewhere (macOS), each file system is a storage of its own (`dev:`).
//!
//! Taking two storages for one only makes previews slower, while taking one
//! for two has two files read from one disk at once, both slowly. So where a
//! rule can't tell, it merges: devices whose disks `/sys` doesn't show are one
//! storage, and so are shares whose server isn't said. A file whose union
//! doesn't say which disk holds it, a file system on no disk the rules know
//! (a virtual machine's share, a FUSE file system over a disk) and a Windows
//! volume whose disks can't be asked for are read alone, nothing beside them.
//!
//! Widths are rules, not measurements: up to four files at once from a
//! solid-state disk, or from an array of nothing else (there are no heads to
//! move, and the processors hold such runs back), one from anything else.
//! Only real disks are taken at their word (Linux's `rotational`, Windows'
//! seek penalty): md and dm devices and ZFS volumes needn't say what their
//! disks are (Unraid's md driver).
//!
//! Files aren't read, and their folders are looked at once: each folder's
//! storage is remembered, and a file of a union costs one attribute's lookup.
//! The mounts are read again after a minute and Windows' volumes after ten,
//! so that disks plugged in and shares mounted since count. With a transcoder
//! Pley didn't start, perhaps on another computer, every file is one storage:
//! it reads them through its own mounts and network.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

/// Files read at once from a solid-state disk at most: as many as the
/// libraries' previews are made at once.
const SOLID: u8 = 4;
/// The key of files whose storage can't be found at all (without `/proc`, say).
const UNKNOWN: &str = "unknown";
/// Folders remembered at most: past that, they are all forgotten.
const MOST_FOLDERS: usize = 100_000;
/// Places whose storage the log told at most (mounts, volumes).
const MOST_TOLD: usize = 1024;

/// Where a file is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    /// The same for the files read from the same storage: `disk:sdb`,
    /// `net:nas`, `zfs:tank`.
    pub key: Arc<str>,
    /// How many files may be read from it at once; 0 where that is unknown,
    /// and a file is read alone.
    pub width: u8,
}

impl Disk {
    /// A storage keyed `key`, its control characters (from a mount's or a
    /// union's own words) replaced: keys are logged.
    fn new(key: impl Into<Arc<str>>, width: u8) -> Self {
        let key: Arc<str> = key.into();
        if key.chars().any(char::is_control) {
            let key: String = key.chars().map(|c| if c.is_control() { '?' } else { c }).collect();
            return Self { key: key.into(), width };
        }
        Self { key, width }
    }
}

/// Where [`Disks`] learns which storage files are read from.
#[derive(Debug, Clone)]
pub enum Source {
    /// This computer: its mounts, volumes or file systems.
    System,
    /// Fixed storage, for tests: a file is on that of the first entry whose
    /// text its path contains (`/` and `\` alike), else on `fixed`. It is
    /// taken with a transcoder Pley didn't start too.
    Fixed(Vec<(String, Disk)>),
}

/// Which storage files are read from: see the module's documentation.
pub struct Disks {
    source: Source,
    /// The transcoder isn't one Pley started: all files are one storage to it.
    external: bool,
    table: Mutex<os::Table>,
}

impl Disks {
    pub fn new(source: Source, external: bool) -> Self {
        Self { source, external, table: Mutex::new(os::Table::default()) }
    }

    /// Where the media file `path` is read from. It blocks, possibly for a
    /// while on a share that is slow to answer: it looks at the file's
    /// folder the first time, and at the file itself in a union.
    pub fn of(&self, path: &Path) -> Disk {
        match &self.source {
            Source::Fixed(fixed) => fixed_disk(fixed, path),
            Source::System if self.external => Disk::new("transcoder", 1),
            Source::System => os::of(&self.table, path, Instant::now()),
        }
    }
}

/// `path`'s storage among `fixed` ([`Source::Fixed`]).
fn fixed_disk(fixed: &[(String, Disk)], path: &Path) -> Disk {
    let path = path.to_string_lossy().replace('\\', "/");
    fixed
        .iter()
        .find(|(part, _)| path.contains(&part.replace('\\', "/")))
        .map_or_else(|| Disk::new("fixed", 1), |(_, disk)| disk.clone())
}

/// The folder of `path`, whose storage its files share: a file is never a
/// mount (nor a volume), and the folder is looked at once for all of them.
fn folder(path: &Path) -> &Path {
    path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(path)
}

/// Remembers `value` for folder `dir`, after forgetting all others when
/// there are [`MOST_FOLDERS`].
fn remember<T>(folders: &mut HashMap<PathBuf, T>, dir: &Path, value: T) {
    if folders.len() >= MOST_FOLDERS {
        folders.clear();
    }
    folders.insert(dir.to_path_buf(), value);
}

/// Logs, at debug, where the files of `place` are read from: the first time,
/// and when that changes.
fn tell(told: &mut HashMap<String, Arc<str>>, place: String, disk: &Disk, how: &str) {
    if told.get(&place) == Some(&disk.key) {
        return;
    }
    let width = match disk.width {
        0 => "alone".to_owned(),
        1 => "one at a time".to_owned(),
        n => format!("up to {n} at a time"),
    };
    tracing::debug!("files in {place} are read from {}{how}, {width}", disk.key);
    if told.len() >= MOST_TOLD {
        told.clear();
    }
    told.insert(place, disk.key.clone());
}

/// Which of `sets` share a member, directly or through others (a
/// union-find): for each set, the index of the first set of its group.
#[cfg_attr(not(any(target_os = "linux", windows, test)), allow(dead_code))]
fn groups<T: Ord>(sets: &[&BTreeSet<T>]) -> Vec<usize> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..sets.len()).collect();
    // The first set each member was seen in.
    let mut first: BTreeMap<&T, usize> = BTreeMap::new();
    for (i, set) in sets.iter().enumerate() {
        for member in set.iter() {
            let j = *first.entry(member).or_insert(i);
            let (a, b) = (root(&mut parent, i), root(&mut parent, j));
            // The group's first set stays its root.
            parent[a.max(b)] = a.min(b);
        }
    }
    (0..sets.len()).map(|i| root(&mut parent, i)).collect()
}

/// Linux's rules: the mounts of `/proc/self/mountinfo`, the disks under each
/// as `/sys` tells them, and the members of unions. They are built on every
/// system, to be tested there; [`os`] reads Linux's files for them.
mod mounts {
    #![cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]

    use std::collections::{BTreeSet, HashMap};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use parking_lot::Mutex;

    use super::{Disk, SOLID, UNKNOWN, folder, groups, remember, tell};

    /// How long the mounts, and the folders found on them, are trusted.
    pub(super) const MOUNTS_FOR: Duration = Duration::from_secs(60);
    /// How soon the mounts are read again for a folder found on an
    /// automount, at most: the share is mounted once a path below it is
    /// followed, but one that can't be mounted stays an automount.
    const AUTOMOUNT_AGAIN: Duration = Duration::from_secs(1);
    /// Members of unions remembered at most.
    const MOST_MEMBERS: usize = 1024;

    /// File systems on a network, whose shares are their server's.
    const NETWORK: &[&str] = &[
        "nfs",
        "nfs4",
        "cifs",
        "smb3",
        "smbfs",
        "ceph",
        "glusterfs",
        "fuse.glusterfs",
        "fuse.sshfs",
        "fuse.rclone",
        "fuse.s3fs",
        "davfs",
    ];
    /// Block devices that aren't disks themselves (arrays, mappings, files,
    /// network devices and ZFS volumes), whose flags needn't be those of the
    /// disks under them.
    const NOT_DISKS: &[&str] = &["md", "dm-", "loop", "nbd", "rbd", "drbd", "zd"];

    /// What the rules need from Linux.
    pub(super) trait Linux {
        /// `/proc/self/mountinfo`.
        fn mountinfo(&self) -> Option<String>;
        /// `dir` with its links followed.
        fn canonicalize(&self, dir: &Path) -> Option<PathBuf>;
        /// The block device `mount` is on, as `/sys` names it: `sdb1`, `dm-0`.
        fn device(&self, mount: &Mount) -> Option<String>;
        /// What `/sys` tells of block device `name`.
        fn device_info(&self, name: &str) -> Option<DevInfo>;
        /// The member of `union` that holds `file`, as the union names it.
        fn member(&self, file: &Path, union: Union) -> Option<String>;
        /// Whether there is something at `path`.
        fn exists(&self, path: &Path) -> bool;
    }

    /// A line of `/proc/self/mountinfo`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct Mount {
        /// Its device's major and minor numbers: major 0 where there is no
        /// block device (shares, unions, btrfs's subvolumes).
        pub dev: (u32, u32),
        /// Where it is mounted.
        pub point: PathBuf,
        /// The type of its file system: `ext4`, `nfs4`, `fuse.shfs`.
        pub fstype: String,
        /// What is mounted: `/dev/sdb1`, `nas:/export`, `tank/movies`.
        pub source: String,
        /// The server of a share, as its options say it (`addr=`): Docker's
        /// NFS volumes are mounted from `:/export`, naming none.
        pub addr: Option<String>,
    }

    /// The mounts of `/proc/self/mountinfo` (proc(5)), in its order. A line
    /// is `36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 - ext3 /dev/root rw`:
    /// its optional fields (`master:1`) end at ` - `, and its paths have
    /// spaces, tabs, newlines and backslashes written in octal (`\040`).
    pub(super) fn parse_mountinfo(text: &str) -> Vec<Mount> {
        text.lines()
            .filter_map(|line| {
                let (mount, filesystem) = line.split_once(" - ")?;
                let mut fields = mount.split(' ');
                let dev = fields.nth(2)?;
                let point = fields.nth(1)?;
                let (major, minor) = dev.split_once(':')?;
                let mut fields = filesystem.split(' ');
                let (fstype, source) = (fields.next()?, fields.next()?);
                let addr = fields.next().and_then(|options| {
                    options.split(',').find_map(|option| option.strip_prefix("addr=")).map(unescape)
                });
                Some(Mount {
                    dev: (major.parse().ok()?, minor.parse().ok()?),
                    point: PathBuf::from(unescape(point)),
                    fstype: unescape(fstype),
                    source: unescape(source),
                    addr,
                })
            })
            .collect()
    }

    /// `text` with the octal escapes of `/proc` undone: `\040` is a space.
    pub(super) fn unescape(text: &str) -> String {
        if !text.contains('\\') {
            return text.to_owned();
        }
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let digits = bytes.get(i + 1..i + 4).filter(|digits| digits.iter().all(|d| (b'0'..=b'7').contains(d)));
            match digits {
                // Three digits, at most `\377`: a byte.
                Some(digits) if bytes[i] == b'\\' && digits[0] <= b'3' => {
                    out.push(digits.iter().fold(0u8, |byte, d| byte * 8 + (d - b'0')));
                    i += 4;
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// The mount `path` is on: the one with the longest mount point above
    /// it, and the last of equal ones, as a mount over another hides it.
    pub(super) fn mount_of(mounts: &[Mount], path: &Path) -> Option<usize> {
        let mut found: Option<usize> = None;
        for (i, mount) in mounts.iter().enumerate() {
            let longer = found.is_none_or(|f| mount.point.as_os_str().len() >= mounts[f].point.as_os_str().len());
            if longer && path.starts_with(&mount.point) {
                found = Some(i);
            }
        }
        found
    }

    /// The server of a share, lowercased, from the source of its mount:
    /// `nas:/export`, `user@nas:/x`, `[fe80::1]:/x`, `//nas/media`,
    /// `https://nas/dav`, or rclone's `remote:path`.
    pub(super) fn host_of(source: &str) -> Option<String> {
        let server = if let Some(rest) = source.strip_prefix("//").or_else(|| source.strip_prefix(r"\\")) {
            rest.split(['/', '\\']).next().unwrap_or_default()
        } else if let Some((_, rest)) = source.split_once("://") {
            rest.split('/').next().unwrap_or_default()
        } else if let Some(end) = source.find("]:") {
            &source[..=end]
        } else {
            source.split_once(':')?.0
        };
        // Without a user, an IPv6 address's brackets, or a port.
        let server = server.rsplit_once('@').map_or(server, |(_, host)| host);
        let host = match server.strip_prefix('[') {
            Some(v6) => v6.split(']').next().unwrap_or_default(),
            None if server.matches(':').count() == 1 => server.split(':').next().unwrap_or_default(),
            None => server,
        };
        (!host.is_empty()).then(|| host.to_lowercase())
    }

    /// The block device a link of /dev names, from the link's place below
    /// /dev (`vg0/media`, `disk/by-uuid/…`) and its text (`../dm-3`): taken
    /// by its text alone, and only a device of /dev itself (`dm-3`).
    pub(super) fn dev_link_target(link: &str, target: &str) -> Option<String> {
        let mut at: Vec<&str> = link.split('/').collect();
        at.pop();
        let target = match target.strip_prefix("/dev/") {
            Some(rest) => {
                at.clear();
                rest
            }
            None if target.starts_with('/') => return None,
            None => target,
        };
        for part in target.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    at.pop()?;
                }
                name => at.push(name),
            }
        }
        match at.as_slice() {
            [name] => Some((*name).to_owned()),
            _ => None,
        }
    }

    /// What `/sys` tells of a block device.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub(super) struct DevInfo {
        /// The disk it is a partition of: `sdb` for `sdb1`.
        pub partition_of: Option<String>,
        /// The devices it is made of (`slaves`): an array's, a mapping's.
        pub slaves: Vec<String>,
        /// Whether it says its disk spins (`queue/rotational`).
        pub rotational: Option<bool>,
    }

    /// The disks under a block device, and whether all are solid-state.
    type Under = (BTreeSet<String>, bool);

    /// The disks block device `dev` is read from: a partition's disk, the
    /// disks under the devices an array or a mapping is made of, else the
    /// device itself. With them, whether all are solid-state: real disks
    /// that say they don't spin, as arrays and mappings needn't say what
    /// their disks are. None where a device on the way can't be read.
    pub(super) fn whole_disks(dev: &str, info: &impl Fn(&str) -> Option<DevInfo>) -> Option<Under> {
        fn follow(dev: &str, info: &impl Fn(&str) -> Option<DevInfo>, depth: usize, under: &mut Under) -> Option<()> {
            // Devices stack a few deep (a partition of a mapping of an
            // array); deeper is a loop.
            if depth > 8 {
                return None;
            }
            let found = info(dev)?;
            if let Some(disk) = &found.partition_of {
                return follow(disk, info, depth + 1, under);
            }
            if found.slaves.is_empty() {
                under.1 &= found.rotational == Some(false) && !NOT_DISKS.iter().any(|kind| dev.starts_with(kind));
                under.0.insert(dev.to_owned());
                return Some(());
            }
            for slave in &found.slaves {
                follow(slave, info, depth + 1, under)?;
            }
            Some(())
        }
        let mut under = (BTreeSet::new(), true);
        follow(dev, info, 0, &mut under)?;
        Some(under)
    }

    /// A union of disks that says which of them holds a file.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(super) enum Union {
        /// Unraid's user shares (`fuse.shfs`): the attribute
        /// `system.LOCATION` names the disk (`disk3`), mounted at
        /// `/mnt/disk3`, or the pool (`cache`).
        Shfs,
        /// mergerfs (`fuse.mergerfs`): the attribute `user.mergerfs.basepath`
        /// is the branch, a disk's folder (`/mnt/disk2`).
        Mergerfs,
    }

    impl Union {
        fn of(fstype: &str) -> Option<Self> {
            match fstype {
                "fuse.shfs" => Some(Self::Shfs),
                "fuse.mergerfs" => Some(Self::Mergerfs),
                _ => None,
            }
        }

        fn prefix(self) -> &'static str {
            match self {
                Self::Shfs => "shfs",
                Self::Mergerfs => "mergerfs",
            }
        }

        /// Whether `name` is a member as the union names one: an Unraid disk
        /// or pool, or the path of a mergerfs branch.
        fn names(self, name: &str) -> bool {
            match self {
                Self::Shfs => {
                    (1..=64).contains(&name.len())
                        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                }
                Self::Mergerfs => name.starts_with('/') && name.len() <= 4096 && !name.chars().any(char::is_control),
            }
        }
    }

    /// Where the files of a mount are read from.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) enum Storage {
        Disk(Disk),
        /// From the member of the union that holds each, or from the whole
        /// union (the disk) where it doesn't say.
        Union(Union, Disk),
    }

    /// Where the files of each of `mounts` are read from.
    pub(super) fn classify(mounts: &[Mount], sys: &impl Linux) -> Vec<Storage> {
        /// What a mount is, before the mounts sharing a disk are joined.
        enum Found {
            Storage(Storage),
            /// On a block device, over these disks.
            Disks(Under),
            /// On a block device whose disks `/sys` doesn't show, by its
            /// numbers or its source.
            Unknown(String),
        }
        // Mounts of one device (bind mounts, subvolumes) follow it once.
        let mut devices: HashMap<String, Option<Under>> = HashMap::new();
        let found: Vec<Found> = mounts
            .iter()
            .map(|mount| {
                if NETWORK.contains(&mount.fstype.as_str()) {
                    // Shares whose server isn't said are one storage.
                    let server = host_of(&mount.source)
                        .or_else(|| mount.addr.as_deref().map(str::to_lowercase))
                        .unwrap_or_default();
                    Found::Storage(Storage::Disk(Disk::new(format!("net:{server}"), 1)))
                } else if mount.fstype == "zfs" {
                    let pool = mount.source.split('/').next().unwrap_or_default();
                    Found::Storage(Storage::Disk(Disk::new(format!("zfs:{pool}"), 1)))
                } else if let Some(union) = Union::of(&mount.fstype) {
                    // Where it doesn't say which disk holds a file, the file
                    // may be on any of them: it goes alone.
                    let whole = Disk::new(format!("{}:{}", union.prefix(), mount.point.display()), 0);
                    Found::Storage(Storage::Union(union, whole))
                } else if mount.dev.0 != 0 || mount.source.starts_with("/dev/") {
                    let id = if mount.dev.0 != 0 {
                        format!("{}:{}", mount.dev.0, mount.dev.1)
                    } else {
                        mount.source.clone()
                    };
                    let under = devices
                        .entry(id.clone())
                        .or_insert_with(|| whole_disks(&sys.device(mount)?, &|name| sys.device_info(name)))
                        .clone();
                    match under {
                        Some(under) if !under.0.is_empty() => Found::Disks(under),
                        _ => Found::Unknown(id),
                    }
                } else {
                    // A file system on no disk the rules know (a virtual
                    // machine's share, a FUSE file system over one): its
                    // files may be read from any, so each goes alone. Keyed
                    // by its numbers, which its bind mounts share: its
                    // source may hold anything, a password too.
                    Found::Storage(Storage::Disk(Disk::new(format!("fs:{}:{}", mount.dev.0, mount.dev.1), 0)))
                }
            })
            .collect();

        // Mounts that share a disk are one storage, keyed by the first of the
        // disks under any of them, and solid-state where all of those are.
        let on_disks: Vec<(usize, &Under)> = found
            .iter()
            .enumerate()
            .filter_map(|(i, found)| match found {
                Found::Disks(under) => Some((i, under)),
                _ => None,
            })
            .collect();
        let group = groups(&on_disks.iter().map(|(_, under)| &under.0).collect::<Vec<_>>());
        let mut joined: HashMap<usize, (&str, bool)> = HashMap::new();
        for ((_, (disks, solid)), &g) in on_disks.iter().zip(&group) {
            let first = disks.first().map_or("", String::as_str);
            let both = joined.entry(g).or_insert((first, true));
            both.0 = both.0.min(first);
            both.1 &= *solid;
        }
        let keys: HashMap<usize, Disk> = joined
            .into_iter()
            .map(|(g, (first, solid))| (g, Disk::new(format!("disk:{first}"), if solid { SOLID } else { 1 })))
            .collect();
        let mut storage: HashMap<usize, Disk> =
            on_disks.iter().zip(&group).map(|((i, _), g)| (*i, keys[g].clone())).collect();
        // Devices whose disks are unknown are one storage: partitions or
        // mappings of one disk couldn't be told apart.
        let unknown = found
            .iter()
            .filter_map(|found| match found {
                Found::Unknown(id) => Some(id),
                _ => None,
            })
            .min()
            .map(|id| Disk::new(format!("fs:{id}"), 1));

        found
            .into_iter()
            .enumerate()
            .map(|(i, found)| match found {
                Found::Storage(storage) => storage,
                Found::Disks(_) => Storage::Disk(storage.remove(&i).unwrap_or_else(|| Disk::new(UNKNOWN, 1))),
                Found::Unknown(_) => Storage::Disk(unknown.clone().unwrap_or_else(|| Disk::new(UNKNOWN, 1))),
            })
            .collect()
    }

    /// Where member `name` of `union` is read from: Unraid's disk (`disk3`)
    /// from the mount at `/mnt/disk3` where there is one, mergerfs's branch
    /// (`/mnt/disk2`) from the mount it is on where it is `here`, else the
    /// member is a storage of its own.
    pub(super) fn member(union: Union, name: &str, here: bool, mounts: &[Mount], storage: &[Storage]) -> Disk {
        let mount = match union {
            Union::Shfs => {
                let point = Path::new("/mnt").join(name);
                mounts.iter().rposition(|mount| mount.point == point)
            }
            Union::Mergerfs if here => mount_of(mounts, Path::new(name)),
            Union::Mergerfs => None,
        };
        match mount.and_then(|i| storage.get(i)) {
            Some(Storage::Disk(disk)) => disk.clone(),
            _ => Disk::new(format!("{}:{name}", union.prefix()), 1),
        }
    }

    /// The mounts as last read, and what was found on them since.
    #[derive(Default)]
    pub(super) struct Table {
        /// When the mounts were read.
        read: Option<Instant>,
        /// When they were last read again for an automount.
        automount: Option<Instant>,
        mounts: Vec<Mount>,
        /// Where each mount's files are read from, in the same order.
        storage: Vec<Storage>,
        /// Folders found since, and where their files are read from.
        folders: HashMap<PathBuf, Storage>,
        /// Members of unions found since, and where they are read from.
        members: HashMap<(Union, String), Disk>,
        /// Where the log said each mount's files are read from.
        told: HashMap<String, Arc<str>>,
    }

    /// The mounts as they are now, and where each one's files are read from.
    fn read_mounts(sys: &impl Linux) -> (Vec<Mount>, Vec<Storage>) {
        let mounts = sys.mountinfo().map(|text| parse_mountinfo(&text)).unwrap_or_default();
        let storage = classify(&mounts, sys);
        (mounts, storage)
    }

    impl Table {
        /// Whether the mounts are to be read again at `now`.
        fn stale(&self, now: Instant) -> bool {
            self.read.is_none_or(|at| now.saturating_duration_since(at) >= MOUNTS_FOR)
        }

        /// Takes the mounts read at `now`, forgetting what was found on the
        /// ones before.
        fn take(&mut self, (mounts, storage): (Vec<Mount>, Vec<Storage>), now: Instant) {
            if mounts.is_empty() && self.read.is_none() {
                tracing::debug!("the mounts can't be read (/proc/self/mountinfo): every file counts as one storage");
            }
            self.mounts = mounts;
            self.storage = storage;
            self.folders.clear();
            self.members.clear();
            self.read = Some(now);
        }

        /// Where the files of folder `real` (its links followed) are read
        /// from; `None` where it is on an automount, whose share is mounted
        /// once a path below it is followed (as finding `real` just did):
        /// the mounts are to be read again first.
        fn storage_of(&mut self, real: &Path, now: Instant) -> Option<Storage> {
            let mount = mount_of(&self.mounts, real);
            if mount.is_some_and(|i| self.mounts[i].fstype == "autofs")
                && self.automount.is_none_or(|at| now.saturating_duration_since(at) >= AUTOMOUNT_AGAIN)
            {
                self.automount = Some(now);
                return None;
            }
            let Some(i) = mount else { return Some(Storage::Disk(Disk::new(UNKNOWN, 1))) };
            let storage = self.storage[i].clone();
            let place = self.mounts[i].point.display().to_string();
            match &storage {
                Storage::Disk(disk) => tell(&mut self.told, place, disk, ""),
                Storage::Union(_, whole) => tell(&mut self.told, place, whole, " where it doesn't say which disk"),
            }
            Some(storage)
        }
    }

    /// Where `path` is read from, by Linux's mounts. They are read under
    /// the lock, so that every lookup sees the mounts as last read: `/proc`
    /// and `/sys` answer at once, and nothing a mount names is followed.
    pub(super) fn disk_of(table: &Mutex<Table>, path: &Path, sys: &impl Linux, now: Instant) -> Disk {
        let dir = folder(path);
        let known = {
            let mut table = table.lock();
            if table.stale(now) {
                table.take(read_mounts(sys), now);
            }
            table.folders.get(dir).cloned()
        };
        let storage = known.unwrap_or_else(|| {
            // Outside the lock: a share's folder may be slow to answer.
            let real = sys.canonicalize(dir).unwrap_or_else(|| dir.to_path_buf());
            let mut table = table.lock();
            let storage = match table.storage_of(&real, now) {
                Some(storage) => storage,
                None => {
                    table.take(read_mounts(sys), now);
                    // An automount still (its share couldn't be mounted): as it is.
                    table.storage_of(&real, now).unwrap_or_else(|| Storage::Disk(Disk::new(UNKNOWN, 1)))
                }
            };
            remember(&mut table.folders, dir, storage.clone());
            storage
        });
        match storage {
            Storage::Disk(disk) => disk,
            Storage::Union(union, whole) => member_disk(table, path, union, whole, sys),
        }
    }

    /// Where `file`, in `union`, is read from: the member that holds it as
    /// the union says, else the whole union.
    fn member_disk(table: &Mutex<Table>, file: &Path, union: Union, whole: Disk, sys: &impl Linux) -> Disk {
        let Some(name) = sys.member(file, union).filter(|name| union.names(name)) else { return whole };
        let known = (union, name);
        if let Some(disk) = table.lock().members.get(&known) {
            return disk.clone();
        }
        // Outside the lock: a branch may be a share's folder.
        let here = union == Union::Mergerfs && sys.exists(Path::new(&known.1));
        let mut table = table.lock();
        let disk = member(union, &known.1, here, &table.mounts, &table.storage);
        if table.members.len() >= MOST_MEMBERS {
            table.members.clear();
        }
        table.members.insert(known, disk.clone());
        disk
    }
}

/// Windows' rules: a share or a mapped drive is its server, and a volume is
/// the disks it is on, joined with every volume on any of them. They are
/// built on every system, to be tested there; [`os`] asks Windows for them.
mod volumes {
    #![cfg_attr(not(any(windows, test)), allow(dead_code))]

    use std::collections::{BTreeSet, HashMap};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use parking_lot::Mutex;

    use super::{Disk, SOLID, folder, groups, remember, tell};

    /// How long the volumes, their disks and the folders found on them are trusted.
    const VOLUMES_FOR: Duration = Duration::from_secs(10 * 60);

    /// What the rules need from Windows.
    pub(super) trait Windows {
        /// `dir` with its links followed: `\\?\C:\…`, or `\\?\UNC\nas\media\…`
        /// on a share (a mapped drive's too).
        fn canonicalize(&self, dir: &Path) -> Option<PathBuf>;
        /// Whether `drive` (`Z:`) is a network drive.
        fn is_remote(&self, drive: &str) -> bool;
        /// What `drive` stands for: `\Device\LanmanRedirector\;Z:0…\nas\media`.
        fn dos_device(&self, drive: &str) -> Option<String>;
        /// The root of the volume `path` is on: `C:\`, or `C:\mnt\disk2\` for
        /// a volume mounted in a folder.
        fn volume_root(&self, path: &Path) -> Option<String>;
        /// The name of the volume at `root`: `\\?\Volume{…}\`.
        fn volume_name(&self, root: &str) -> Option<String>;
        /// The names of all volumes.
        fn volumes(&self) -> Vec<String>;
        /// The disks a volume is on, by number.
        fn disks(&self, volume: &str) -> Option<BTreeSet<u32>>;
        /// Whether reading disk `disk` costs seeks: not on a solid-state disk.
        fn seek_penalty(&self, disk: u32) -> Option<bool>;
    }

    /// The server of a share's path, lowercased: `\\nas\media\…`, or
    /// `\\?\UNC\nas\media\…` as links are followed.
    pub(super) fn unc_server(path: &str) -> Option<String> {
        let rest = match path.get(..8) {
            Some(start) if start.eq_ignore_ascii_case(r"\\?\UNC\") || start.eq_ignore_ascii_case(r"\\.\UNC\") => {
                &path[8..]
            }
            _ => {
                let rest = path.strip_prefix(r"\\").or_else(|| path.strip_prefix("//"))?;
                // `\\?\C:\…` and `\\.\PhysicalDrive0` are on this computer.
                if rest.starts_with(['?', '.']) {
                    return None;
                }
                rest
            }
        };
        let server = rest.split(['\\', '/']).next()?;
        (!server.is_empty()).then(|| server.to_lowercase())
    }

    /// The server of a mapped drive, lowercased, from what the drive stands
    /// for: `\Device\LanmanRedirector\;Z:0000000000012345\nas\media`, or
    /// `\Device\Mup\;LanmanRedirector\;Z:000000000002a3f5\nas\media`. It
    /// follows the last `;Z:` and the number of the session that mapped it.
    pub(super) fn mapped_server(target: &str) -> Option<String> {
        let rest = target.rmatch_indices(';').find_map(|(i, _)| {
            let rest = &target[i + 1..];
            let mut chars = rest.chars();
            if !chars.next()?.is_ascii_alphabetic() || chars.next()? != ':' {
                return None;
            }
            let session = rest[2..].bytes().take_while(u8::is_ascii_hexdigit).count();
            rest[2 + session..].strip_prefix('\\').filter(|_| session > 0)
        })?;
        let server = rest.split('\\').next()?;
        (!server.is_empty()).then(|| server.to_lowercase())
    }

    /// The drive of a path on a drive letter: `C:` for `C:\…` and `\\?\C:\…`.
    pub(super) fn drive_of(path: &str) -> Option<String> {
        let path = path.strip_prefix(r"\\?\").unwrap_or(path);
        let bytes = path.as_bytes();
        let drive = bytes.len() >= 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes.get(2).is_none_or(|&b| b == b'\\' || b == b'/');
        drive.then(|| path[..2].to_ascii_uppercase())
    }

    /// The `{GUID}` of a volume's name, `\\?\Volume{GUID}\`.
    fn guid(volume: &str) -> &str {
        match (volume.find('{'), volume.rfind('}')) {
            (Some(start), Some(end)) if start < end => &volume[start..=end],
            _ => volume,
        }
    }

    /// The volumes and disks as last asked for, and what was found on them since.
    #[derive(Default)]
    pub(super) struct Table {
        /// When the volumes were listed.
        read: Option<Instant>,
        /// The disks of each volume and of the volumes sharing one with it,
        /// by the volume's name.
        volumes: HashMap<String, Arc<BTreeSet<u32>>>,
        /// What disks said of their seek penalty, by number.
        penalties: HashMap<u32, Option<bool>>,
        /// Volume roots found since, and where their files are read from.
        roots: HashMap<String, Disk>,
        /// Folders found since, and where their files are read from.
        folders: HashMap<PathBuf, Disk>,
        /// Where the log said each volume's files are read from.
        told: HashMap<String, Arc<str>>,
    }

    /// Every volume's disks, with those of the volumes sharing one, by the
    /// volume's name.
    fn read_volumes(sys: &impl Windows) -> HashMap<String, Arc<BTreeSet<u32>>> {
        let names = sys.volumes();
        let disks: Vec<BTreeSet<u32>> = names.iter().map(|name| sys.disks(name).unwrap_or_default()).collect();
        let group = groups(&disks.iter().collect::<Vec<_>>());
        let mut joined: HashMap<usize, BTreeSet<u32>> = HashMap::new();
        for (set, &g) in disks.iter().zip(&group) {
            joined.entry(g).or_default().extend(set);
        }
        let joined: HashMap<usize, Arc<BTreeSet<u32>>> =
            joined.into_iter().filter(|(_, set)| !set.is_empty()).map(|(g, set)| (g, Arc::new(set))).collect();
        names.into_iter().zip(group).filter_map(|(name, g)| Some((name, joined.get(&g)?.clone()))).collect()
    }

    impl Table {
        /// Takes the volumes listed at `now`, forgetting what was found on
        /// them before.
        fn take(&mut self, volumes: HashMap<String, Arc<BTreeSet<u32>>>, now: Instant) {
            self.volumes = volumes;
            self.penalties.clear();
            self.roots.clear();
            self.folders.clear();
            self.read = Some(now);
        }
    }

    /// Where `path` is read from, by Windows' volumes. Windows is asked
    /// outside the lock, which is held only to read and keep its answers, so
    /// that a disk slow to answer delays only the lookup that asked.
    pub(super) fn disk_of(table: &Mutex<Table>, path: &Path, sys: &impl Windows, now: Instant) -> Disk {
        let dir = folder(path);
        let mut listed = table.lock().read;
        if listed.is_none_or(|at| now.saturating_duration_since(at) >= VOLUMES_FOR) {
            let volumes = read_volumes(sys);
            table.lock().take(volumes, now);
            listed = Some(now);
        }
        if let Some(disk) = table.lock().folders.get(dir) {
            return disk.clone();
        }
        // Outside the lock: a share may be slow to answer.
        let real = sys.canonicalize(dir).unwrap_or_else(|| dir.to_path_buf());
        let disk = folder_disk(table, listed, &real, sys);
        let mut table = table.lock();
        if table.read == listed {
            remember(&mut table.folders, dir, disk.clone());
        }
        disk
    }

    /// Where the files of folder `real` (its links followed) are read from.
    /// What it finds is kept only while the volumes are still those listed
    /// at `listed`: another lookup may have listed them again meanwhile.
    fn folder_disk(table: &Mutex<Table>, listed: Option<Instant>, real: &Path, sys: &impl Windows) -> Disk {
        let text = real.to_string_lossy();
        // Before any volume is asked for: Windows takes a share it can't
        // reach for its own boot volume.
        if let Some(server) = unc_server(&text) {
            return Disk::new(format!("net:{server}"), 1);
        }
        let drive = drive_of(&text);
        if let Some(drive) = drive.as_deref().filter(|drive| sys.is_remote(drive)) {
            // A mapped drive whose share didn't answer: the server it is
            // mapped to, else the drive.
            let server = sys.dos_device(drive).and_then(|target| mapped_server(&target));
            return Disk::new(format!("net:{}", server.as_deref().unwrap_or(drive)), 1);
        }
        let root = sys
            .volume_root(real)
            .or_else(|| drive.map(|drive| format!(r"{drive}\")))
            .unwrap_or_else(|| real.ancestors().last().unwrap_or(real).to_string_lossy().into_owned());
        if let Some(disk) = table.lock().roots.get(&root) {
            return disk.clone();
        }
        let disk = volume_disk(table, listed, &root, sys);
        let mut table = table.lock();
        if table.read == listed {
            tell(&mut table.told, root.clone(), &disk, "");
            table.roots.insert(root, disk.clone());
        }
        disk
    }

    /// Where the files of the volume at `root` are read from: its disks,
    /// with those of the volumes sharing one, solid-state where all are.
    fn volume_disk(table: &Mutex<Table>, listed: Option<Instant>, root: &str, sys: &impl Windows) -> Disk {
        let Some(volume) = sys.volume_name(root) else { return Disk::new(format!("volume:{root}"), 0) };
        let joined = table.lock().volumes.get(&volume).cloned();
        let disks = match joined {
            Some(disks) => disks,
            // A volume that came after they were listed, joined with none.
            None => match sys.disks(&volume) {
                Some(disks) if !disks.is_empty() => Arc::new(disks),
                _ => return Disk::new(format!("volume:{}", guid(&volume)), 0),
            },
        };
        let Some(&first) = disks.first() else { return Disk::new(format!("volume:{}", guid(&volume)), 0) };
        // A disk that doesn't say counts as one that spins.
        let solid = disks.iter().all(|&disk| penalty(table, listed, disk, sys) == Some(false));
        Disk::new(format!("disk:{first}"), if solid { SOLID } else { 1 })
    }

    /// What disk `disk` says of its seek penalty, asked once while the
    /// volumes are those listed at `listed`.
    fn penalty(table: &Mutex<Table>, listed: Option<Instant>, disk: u32, sys: &impl Windows) -> Option<bool> {
        if let Some(&penalty) = table.lock().penalties.get(&disk) {
            return penalty;
        }
        let penalty = sys.seek_penalty(disk);
        let mut table = table.lock();
        if table.read == listed {
            table.penalties.insert(disk, penalty);
        }
        penalty
    }
}

#[cfg(target_os = "linux")]
mod os {
    use std::ffi::CString;
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use parking_lot::Mutex;

    use super::Disk;
    pub(super) use super::mounts::Table;
    use super::mounts::{DevInfo, Linux, Mount, Union, dev_link_target, disk_of};

    /// `/proc/self/mountinfo` read at most: thousands of mounts.
    const MOST_MOUNTINFO: u64 = 16 << 20;

    pub fn of(table: &Mutex<Table>, path: &Path, now: Instant) -> Disk {
        disk_of(table, path, &System, now)
    }

    struct System;

    impl Linux for System {
        fn mountinfo(&self) -> Option<String> {
            let mut text = Vec::new();
            std::fs::File::open("/proc/self/mountinfo").ok()?.take(MOST_MOUNTINFO).read_to_end(&mut text).ok()?;
            Some(String::from_utf8_lossy(&text).into_owned())
        }

        fn canonicalize(&self, dir: &Path) -> Option<PathBuf> {
            std::fs::canonicalize(dir).ok()
        }

        fn device(&self, mount: &Mount) -> Option<String> {
            let (major, minor) = mount.dev;
            if major != 0
                && let Some(name) = name_of(Path::new(&format!("/sys/dev/block/{major}:{minor}")))
            {
                return Some(name);
            }
            let name = mount.source.strip_prefix("/dev/")?;
            if let Some(mapped) = name.strip_prefix("mapper/") {
                return mapper(mapped);
            }
            if one_name(name) {
                return Some(name.to_owned());
            }
            // A link of /dev's own (LVM's /dev/vg0/media, /dev/disk/by-uuid/…),
            // read once and taken by its text, never followed: a mount's source
            // can name anything (a FUSE file system's owner chooses its own),
            // and following it could wait on whatever it leads to. Never in
            // the folders anyone may write in, where such a file system could
            // be mounted.
            let parts: Vec<&str> = name.split('/').collect();
            if !(2..=3).contains(&parts.len())
                || parts.iter().any(|part| part.is_empty() || *part == "." || *part == "..")
                || matches!(parts[0], "shm" | "mqueue")
            {
                return None;
            }
            let target = std::fs::read_link(&mount.source).ok()?;
            dev_link_target(name, target.to_str()?)
        }

        fn device_info(&self, name: &str) -> Option<DevInfo> {
            if !one_name(name) {
                return None;
            }
            let dir = std::fs::canonicalize(Path::new("/sys/class/block").join(name)).ok()?;
            // A partition's folder is in its disk's.
            let partition_of = if dir.join("partition").exists() {
                Some(dir.parent()?.file_name()?.to_str()?.to_owned())
            } else {
                None
            };
            let slaves = std::fs::read_dir(dir.join("slaves"))
                .map(|entries| entries.flatten().filter_map(|entry| entry.file_name().into_string().ok()).collect())
                .unwrap_or_default();
            let rotational =
                std::fs::read_to_string(dir.join("queue/rotational")).ok().and_then(|flag| match flag.trim() {
                    "0" => Some(false),
                    "1" => Some(true),
                    _ => None,
                });
            Some(DevInfo { partition_of, slaves, rotational })
        }

        fn member(&self, file: &Path, union: Union) -> Option<String> {
            let attribute = match union {
                Union::Shfs => c"system.LOCATION",
                Union::Mergerfs => c"user.mergerfs.basepath",
            };
            let path = CString::new(file.as_os_str().as_bytes()).ok()?;
            let mut value = [0u8; 4096];
            // SAFETY: a terminated path and name, and a buffer of the size given.
            let len =
                unsafe { libc::getxattr(path.as_ptr(), attribute.as_ptr(), value.as_mut_ptr().cast(), value.len()) };
            let value = value.get(..usize::try_from(len).ok()?)?;
            let value = std::str::from_utf8(value).ok()?.trim_end_matches('\0').trim();
            (!value.is_empty()).then(|| value.to_owned())
        }

        fn exists(&self, path: &Path) -> bool {
            path.exists()
        }
    }

    /// The name of the block device a link of /sys leads to.
    fn name_of(link: &Path) -> Option<String> {
        std::fs::canonicalize(link).ok()?.file_name()?.to_str().map(str::to_owned)
    }

    /// The device-mapper device /dev/mapper/`name` is: the `dm-N` whose `dm/name` it is.
    fn mapper(name: &str) -> Option<String> {
        std::fs::read_dir("/sys/class/block").ok()?.flatten().find_map(|entry| {
            let dev = entry.file_name().into_string().ok().filter(|dev| dev.starts_with("dm-"))?;
            let named = std::fs::read_to_string(entry.path().join("dm/name")).ok()?;
            (named.trim() == name).then_some(dev)
        })
    }

    /// Whether `name` can name a block device in /sys: one component.
    fn one_name(name: &str) -> bool {
        !name.is_empty() && !name.contains('/') && name != "." && name != ".."
    }
}

#[cfg(windows)]
mod os {
    use std::collections::BTreeSet;
    use std::ffi::OsStr;
    use std::fs::{File, OpenOptions};
    use std::mem::{offset_of, size_of};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::{Path, PathBuf};
    use std::ptr::null_mut;
    use std::time::Instant;

    use parking_lot::Mutex;
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_SHARE_READ, FILE_SHARE_WRITE, FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, GetDriveTypeW,
        GetVolumeNameForVolumeMountPointW, GetVolumePathNameW, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, QueryDosDeviceW,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        DISK_EXTENT, IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_PROPERTY_QUERY,
        StorageDeviceSeekPenaltyProperty, VOLUME_DISK_EXTENTS,
    };

    use super::Disk;
    pub(super) use super::volumes::Table;
    use super::volumes::{Windows, disk_of};

    /// What `GetDriveTypeW` answers for a network drive (`DRIVE_REMOTE`).
    const DRIVE_REMOTE: u32 = 4;
    /// Volumes listed at most.
    const MOST_VOLUMES: usize = 1024;
    /// Pieces of a volume asked for at most (one spanning many disks).
    const MOST_EXTENTS: usize = 1024;

    pub fn of(table: &Mutex<Table>, path: &Path, now: Instant) -> Disk {
        disk_of(table, path, &System, now)
    }

    struct System;

    impl Windows for System {
        fn canonicalize(&self, dir: &Path) -> Option<PathBuf> {
            std::fs::canonicalize(dir).ok()
        }

        fn is_remote(&self, drive: &str) -> bool {
            let root = wide(format!(r"{drive}\"));
            // SAFETY: a terminated path.
            unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
        }

        fn dos_device(&self, drive: &str) -> Option<String> {
            let drive = wide(drive);
            let mut target = vec![0u16; 1024];
            // SAFETY: a terminated name, and a buffer of the length given.
            let len = unsafe { QueryDosDeviceW(drive.as_ptr(), target.as_mut_ptr(), target.len() as u32) };
            (len > 0).then(|| text(&target))
        }

        fn volume_root(&self, path: &Path) -> Option<String> {
            let path = wide(path);
            // The root is never longer than the path and a backslash.
            let mut root = vec![0u16; path.len().max(260) + 1];
            // SAFETY: a terminated path, and a buffer of the length given.
            let found = unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) } != 0;
            found.then(|| text(&root))
        }

        fn volume_name(&self, root: &str) -> Option<String> {
            let root = wide(root);
            // `\\?\Volume{GUID}\` takes 49 and the terminator.
            let mut name = [0u16; 64];
            // SAFETY: a terminated path, and a buffer of the length given.
            let found =
                unsafe { GetVolumeNameForVolumeMountPointW(root.as_ptr(), name.as_mut_ptr(), name.len() as u32) } != 0;
            found.then(|| text(&name))
        }

        fn volumes(&self) -> Vec<String> {
            let mut names = Vec::new();
            let mut name = [0u16; 64];
            // SAFETY: a buffer of the length given; the search is closed below.
            let search = unsafe { FindFirstVolumeW(name.as_mut_ptr(), name.len() as u32) };
            if search == INVALID_HANDLE_VALUE {
                return names;
            }
            loop {
                names.push(text(&name));
                // SAFETY: the search opened above, and a buffer of the length given.
                if names.len() >= MOST_VOLUMES
                    || unsafe { FindNextVolumeW(search, name.as_mut_ptr(), name.len() as u32) } == 0
                {
                    break;
                }
            }
            // SAFETY: the search opened above, closed once.
            unsafe { FindVolumeClose(search) };
            names
        }

        fn disks(&self, volume: &str) -> Option<BTreeSet<u32>> {
            // `\\?\Volume{…}\` without its backslash is the volume, not its root folder.
            let device = open(&format!(r"\\.\{}", volume.strip_prefix(r"\\?\")?.trim_end_matches('\\')))?;
            let mut room = 32;
            loop {
                let size = size_of::<VOLUME_DISK_EXTENTS>() + (room - 1) * size_of::<DISK_EXTENT>();
                // u64s: aligned for the extents written into it.
                let mut buffer = vec![0u64; size.div_ceil(8)];
                let mut written = 0u32;
                // SAFETY: an open volume, and a buffer of `size` bytes.
                let answered = unsafe {
                    DeviceIoControl(
                        device.as_raw_handle(),
                        IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
                        std::ptr::null(),
                        0,
                        buffer.as_mut_ptr().cast(),
                        size as u32,
                        &mut written,
                        null_mut(),
                    )
                } != 0;
                let error = std::io::Error::last_os_error();
                let extents = buffer.as_ptr().cast::<VOLUME_DISK_EXTENTS>();
                // SAFETY: the buffer holds a VOLUME_DISK_EXTENTS, zeroed or written.
                let count = unsafe { (*extents).NumberOfDiskExtents } as usize;
                if !answered {
                    // A volume over more pieces says how many.
                    if error.raw_os_error() == Some(ERROR_MORE_DATA as i32)
                        && (room + 1..=MOST_EXTENTS).contains(&count)
                    {
                        room = count;
                        continue;
                    }
                    return None;
                }
                // SAFETY: the extents Windows wrote, `room` of them at most in the buffer.
                let first = unsafe { (&raw const (*extents).Extents).cast::<DISK_EXTENT>() };
                // SAFETY: as above.
                return Some((0..count.min(room)).map(|i| unsafe { (*first.add(i)).DiskNumber }).collect());
            }
        }

        fn seek_penalty(&self, disk: u32) -> Option<bool> {
            let device = open(&format!(r"\\.\PhysicalDrive{disk}"))?;
            let query = STORAGE_PROPERTY_QUERY {
                PropertyId: StorageDeviceSeekPenaltyProperty,
                QueryType: PropertyStandardQuery,
                AdditionalParameters: [0],
            };
            let mut answer = SeekPenalty::default();
            let mut written = 0u32;
            // SAFETY: an open disk, and a query and an answer of the sizes given.
            let answered = unsafe {
                DeviceIoControl(
                    device.as_raw_handle(),
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    (&raw const query).cast(),
                    size_of::<STORAGE_PROPERTY_QUERY>() as u32,
                    (&raw mut answer).cast(),
                    size_of::<SeekPenalty>() as u32,
                    &mut written,
                    null_mut(),
                )
            } != 0;
            let whole = written as usize > offset_of!(SeekPenalty, incurs_seek_penalty);
            (answered && whole).then_some(answer.incurs_seek_penalty != 0)
        }
    }

    /// `DEVICE_SEEK_PENALTY_DESCRIPTOR`, its `BOOLEAN` read as the byte it is.
    #[repr(C)]
    #[derive(Default)]
    struct SeekPenalty {
        _version: u32,
        _size: u32,
        incurs_seek_penalty: u8,
    }

    /// Opens a volume or a disk to ask it about itself: with no access to
    /// what it holds, which needs no administrator, and shared with all.
    /// It is closed when dropped.
    fn open(device: &str) -> Option<File> {
        OpenOptions::new().access_mode(0).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE).open(device).ok()
    }

    fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
        text.as_ref().encode_wide().chain(Some(0)).collect()
    }

    /// The text of a buffer Windows wrote, up to its terminator.
    fn text(wide: &[u16]) -> String {
        let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..len])
    }
}

/// Elsewhere (macOS): each file system is a storage of its own. The
/// transcoder makes one seek-preview set at a time there anyway.
#[cfg(all(unix, not(target_os = "linux")))]
mod os {
    use std::collections::HashMap;
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use parking_lot::Mutex;

    use super::mounts::MOUNTS_FOR;
    use super::{Disk, UNKNOWN, folder, remember};

    /// The folders found lately, and the file system each is on.
    #[derive(Default)]
    pub struct Table {
        read: Option<Instant>,
        folders: HashMap<PathBuf, Disk>,
    }

    pub fn of(table: &Mutex<Table>, path: &Path, now: Instant) -> Disk {
        let dir = folder(path);
        {
            let mut table = table.lock();
            // Disks come and go: what was found on them is kept as long as
            // Linux's mounts are.
            if table.read.is_none_or(|at| now.saturating_duration_since(at) >= MOUNTS_FOR) {
                table.folders.clear();
                table.read = Some(now);
            }
            if let Some(disk) = table.folders.get(dir) {
                return disk.clone();
            }
        }
        // Outside the lock: a share's folder may be slow to answer.
        let disk = match std::fs::metadata(dir) {
            Ok(meta) => Disk::new(format!("dev:{}", meta.dev()), 1),
            Err(_) => Disk::new(UNKNOWN, 1),
        };
        remember(&mut table.lock().folders, dir, disk.clone());
        disk
    }
}

#[cfg(not(any(unix, windows)))]
mod os {
    use std::path::Path;
    use std::time::Instant;

    use parking_lot::Mutex;

    use super::{Disk, UNKNOWN};

    #[derive(Default)]
    pub struct Table;

    pub fn of(_table: &Mutex<Table>, _path: &Path, _now: Instant) -> Disk {
        Disk::new(UNKNOWN, 1)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use super::mounts::{
        self, DevInfo, Linux, Mount, Union, classify, host_of, mount_of, parse_mountinfo, whole_disks,
    };
    use super::volumes::{self, Windows, mapped_server, unc_server};
    use super::*;

    fn disk(key: &str, width: u8) -> Disk {
        Disk::new(key, width)
    }

    fn mount(dev: (u32, u32), point: &str, fstype: &str, source: &str) -> Mount {
        Mount { dev, point: point.into(), fstype: fstype.into(), source: source.into(), addr: None }
    }

    /// A Linux of fixture files, which counts what it is asked.
    #[derive(Default)]
    struct FakeLinux {
        mountinfo: RefCell<Option<String>>,
        /// Folders that are links, and where they lead.
        links: HashMap<PathBuf, PathBuf>,
        /// Block devices by their numbers (`8:17`) or their source.
        devices: HashMap<String, String>,
        info: HashMap<String, DevInfo>,
        /// Files of unions, and the member each is said to be on.
        members: HashMap<PathBuf, String>,
        here: HashSet<PathBuf>,
        reads: Cell<usize>,
        canonicalized: Cell<usize>,
    }

    impl FakeLinux {
        fn with(mountinfo: &str) -> Self {
            Self { mountinfo: RefCell::new(Some(mountinfo.to_owned())), ..Self::default() }
        }

        fn device(mut self, id: &str, name: &str) -> Self {
            self.devices.insert(id.into(), name.into());
            self
        }

        fn disk(mut self, name: &str, rotational: Option<bool>) -> Self {
            self.info.insert(name.into(), DevInfo { rotational, ..DevInfo::default() });
            self
        }

        fn partition(mut self, name: &str, of: &str) -> Self {
            self.info.insert(name.into(), DevInfo { partition_of: Some(of.into()), ..DevInfo::default() });
            self
        }

        fn made_of(mut self, name: &str, slaves: &[&str], rotational: Option<bool>) -> Self {
            let slaves = slaves.iter().map(|&slave| slave.to_owned()).collect();
            self.info.insert(name.into(), DevInfo { slaves, rotational, ..DevInfo::default() });
            self
        }
    }

    impl Linux for FakeLinux {
        fn mountinfo(&self) -> Option<String> {
            self.reads.set(self.reads.get() + 1);
            self.mountinfo.borrow().clone()
        }

        fn canonicalize(&self, dir: &Path) -> Option<PathBuf> {
            self.canonicalized.set(self.canonicalized.get() + 1);
            Some(self.links.get(dir).cloned().unwrap_or_else(|| dir.to_path_buf()))
        }

        fn device(&self, mount: &Mount) -> Option<String> {
            let id = if mount.dev.0 != 0 { format!("{}:{}", mount.dev.0, mount.dev.1) } else { mount.source.clone() };
            self.devices.get(&id).cloned()
        }

        fn device_info(&self, name: &str) -> Option<DevInfo> {
            self.info.get(name).cloned()
        }

        fn member(&self, file: &Path, _union: Union) -> Option<String> {
            self.members.get(file).cloned()
        }

        fn exists(&self, path: &Path) -> bool {
            self.here.contains(path)
        }
    }

    fn linux_of(table: &Mutex<mounts::Table>, sys: &FakeLinux, path: &str) -> Disk {
        mounts::disk_of(table, Path::new(path), sys, Instant::now())
    }

    #[test]
    fn mount_points_are_read_with_their_escapes_and_the_longest_wins() {
        let text = "\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw,errors=remount-ro
30 22 0:44 / /mnt/user rw,nosuid,nodev shared:20 master:3 - fuse.shfs shfs rw,user_id=0,allow_other
31 30 0:52 / /mnt/user/media rw shared:21 - nfs4 nas:/export/media rw,vers=4.2
32 22 8:33 /films /mnt/My\\040Films rw,relatime - ext4 /dev/sdc1 rw
33 22 0:60 / /mnt/back\\134slash\\011tab rw - tmpfs tmp\\040fs rw
34 22 8:49 / /mnt/disk2 rw - xfs /dev/sdd1 rw
35 22 8:65 / /mnt/disk2 rw - xfs /dev/sde1 rw
37 22 0:70 / /mnt/n rw - nfs :/volume1/movies rw,vers=3,addr=192.168.1.10
not a mount line
36 22 8:81 / /mnt/x rw no-separator-here
";
        let mounts = parse_mountinfo(text);
        assert_eq!(mounts.len(), 8, "{mounts:?}");
        assert_eq!(mounts[0], mount((8, 2), "/", "ext4", "/dev/sda2"));
        assert_eq!(mounts[1], mount((0, 44), "/mnt/user", "fuse.shfs", "shfs"), "optional fields before ' - '");
        assert_eq!(mounts[3], mount((8, 33), "/mnt/My Films", "ext4", "/dev/sdc1"));
        assert_eq!(mounts[4], mount((0, 60), "/mnt/back\\slash\ttab", "tmpfs", "tmp fs"));
        assert_eq!(mounts::unescape(r"a\040b\12c\9999\377"), "a b\\12c\\9999\u{FFFD}");
        let docker = Mount { addr: Some("192.168.1.10".into()), ..mount((0, 70), "/mnt/n", "nfs", ":/volume1/movies") };
        assert_eq!(mounts[7], docker, "the server in its options");

        let on = |path: &str| mount_of(&mounts, Path::new(path)).map(|i| mounts[i].point.clone());
        assert_eq!(on("/mnt/user/media/Movies/Alpha (2001)"), Some("/mnt/user/media".into()));
        assert_eq!(on("/mnt/user/tv/Show"), Some("/mnt/user".into()));
        assert_eq!(on("/mnt/username/x"), Some("/".into()), "a point is a whole folder");
        assert_eq!(on("/mnt/My Films/a.mkv"), Some("/mnt/My Films".into()));
        // A mount over another hides it.
        assert_eq!(mount_of(&mounts, Path::new("/mnt/disk2/a.mkv")), Some(6));
        assert_eq!(mount_of(&[], Path::new("/x")), None);
    }

    #[test]
    fn links_of_dev_are_taken_by_their_text() {
        let target = mounts::dev_link_target;
        assert_eq!(target("vg0/media", "../dm-3").as_deref(), Some("dm-3"));
        assert_eq!(target("disk/by-uuid/0a1b", "../../sdb1").as_deref(), Some("sdb1"));
        assert_eq!(target("disk/by-uuid/0a1b", "/dev/sdb1").as_deref(), Some("sdb1"));
        assert_eq!(target("vg0/media", "../../etc/passwd"), None, "out of /dev");
        assert_eq!(target("vg0/media", "/home/eve/x"), None);
        assert_eq!(target("vg0/media", "dm-3"), None, "not a device of /dev itself");
        assert_eq!(target("disk/by-id/x", "../../mapper/vg-lv"), None);
    }

    #[test]
    fn network_shares_are_their_server_and_pools_their_pool() {
        assert_eq!(host_of("nas:/export/a").as_deref(), Some("nas"));
        assert_eq!(host_of("NAS:/export/b").as_deref(), Some("nas"));
        assert_eq!(host_of("//nas/media").as_deref(), Some("nas"));
        assert_eq!(host_of("user@host:/x").as_deref(), Some("host"));
        assert_eq!(host_of("[fe80::1]:/x").as_deref(), Some("fe80::1"));
        assert_eq!(host_of("user@[fe80::1]:/x").as_deref(), Some("fe80::1"));
        assert_eq!(host_of("https://dav.example.org:8443/remote.php").as_deref(), Some("dav.example.org"));
        assert_eq!(host_of("gdrive:Movies").as_deref(), Some("gdrive"));
        assert_eq!(host_of("10.0.0.1:6789,10.0.0.2:6789:/").as_deref(), Some("10.0.0.1"));
        assert_eq!(host_of("s3fs"), None);

        let mounts = [
            mount((0, 50), "/mnt/a", "nfs", "nas:/export/a"),
            mount((0, 51), "/mnt/b", "nfs4", "NAS:/export/b"),
            mount((0, 52), "/mnt/c", "cifs", "//nas/media"),
            mount((0, 53), "/mnt/d", "fuse.sshfs", "user@host:/x"),
            mount((0, 54), "/mnt/e", "nfs", "[fe80::1]:/x"),
            mount((0, 55), "/mnt/f", "fuse.s3fs", "s3fs"),
            mount((0, 56), "/tank/movies", "zfs", "tank/movies"),
            mount((0, 57), "/tank/tv", "zfs", "tank/tv"),
            mount((0, 58), "/tmp", "tmpfs", "tmpfs"),
            mount((0, 59), "/", "overlay", "overlay"),
            // Docker's NFS volumes name their server in their options only.
            Mount { addr: Some("192.168.1.10".into()), ..mount((0, 60), "/movies", "nfs", ":/volume1/movies") },
            Mount { addr: Some("192.168.1.10".into()), ..mount((0, 61), "/tv", "nfs", ":/volume1/tv") },
            // A FUSE file system the rules don't know, whose source may hold anything.
            mount((0, 62), "/ftp", "fuse.curlftpfs", "curlftpfs#ftp://user:secret@nas/"),
        ];
        let keys: Vec<Disk> = classify(&mounts, &FakeLinux::default())
            .into_iter()
            .map(|storage| match storage {
                mounts::Storage::Disk(disk) => disk,
                other => panic!("{other:?}"),
            })
            .collect();
        let expected = [
            ("net:nas", 1),
            ("net:nas", 1),
            ("net:nas", 1),
            ("net:host", 1),
            ("net:fe80::1", 1),
            // Shares whose server isn't said are one storage.
            ("net:", 1),
            ("zfs:tank", 1),
            ("zfs:tank", 1),
            // On no disk the rules know: alone, keyed by numbers rather than a source.
            ("fs:0:58", 0),
            ("fs:0:59", 0),
            ("net:192.168.1.10", 1),
            ("net:192.168.1.10", 1),
            ("fs:0:62", 0),
        ];
        assert_eq!(keys, expected.map(|(key, width)| disk(key, width)));
    }

    #[test]
    fn partitions_mappings_and_arrays_come_down_to_their_disks() {
        let info: HashMap<&str, DevInfo> = [
            ("sda", DevInfo { rotational: Some(true), ..DevInfo::default() }),
            ("sdb", DevInfo { rotational: Some(true), ..DevInfo::default() }),
            ("sdb1", DevInfo { partition_of: Some("sdb".into()), ..DevInfo::default() }),
            ("sda2", DevInfo { partition_of: Some("sda".into()), ..DevInfo::default() }),
            ("dm-0", DevInfo { slaves: vec!["sda2".into()], rotational: Some(false), ..DevInfo::default() }),
        ]
        .into_iter()
        .collect();
        let info = |name: &str| info.get(name).cloned();
        assert_eq!(whole_disks("sdb1", &info), Some((["sdb".to_owned()].into(), false)));
        assert_eq!(
            whole_disks("dm-0", &info),
            Some((["sda".to_owned()].into(), false)),
            "the mapping's flag isn't its disk's"
        );
        assert_eq!(whole_disks("sdz", &info), None);
        let looped = |_: &str| Some(DevInfo { slaves: vec!["dm-1".into()], ..DevInfo::default() });
        assert_eq!(whole_disks("dm-1", &looped), None, "a loop ends");

        let sys = FakeLinux::with(
            "\
1 0 8:2 / / rw - ext4 /dev/sda2 rw
2 1 9:0 / /boot rw - ext4 /dev/md0 rw
3 1 9:2 / /data rw - ext4 /dev/md2 rw
4 1 8:65 / /media rw - xfs /dev/sde1 rw
5 1 259:1 / /fast rw - ext4 /dev/nvme0n1p1 rw
6 1 9:3 / /mirror rw - btrfs /dev/md3 rw
7 1 9:1 / /mnt/disk1 rw - xfs /dev/md1 rw
8 1 8:97 / /lost rw - ext4 /dev/sdg1 rw
9 1 253:7 / /lost2 rw - ext4 /dev/mapper/gone rw
10 1 0:33 /media /srv/btrfs rw - btrfs /dev/sdf2 rw
11 1 230:0 / /zvol rw - ext4 /dev/zd0 rw
",
        )
        .device("8:2", "sda2")
        .device("9:0", "md0")
        .device("9:2", "md2")
        .device("8:65", "sde1")
        .device("259:1", "nvme0n1p1")
        .device("9:3", "md3")
        .device("9:1", "md1")
        .device("/dev/sdf2", "sdf2")
        .device("230:0", "zd0")
        .partition("sda1", "sda")
        .partition("sda2", "sda")
        .partition("sda3", "sda")
        .partition("sdb1", "sdb")
        .partition("sdb3", "sdb")
        .partition("sdc3", "sdc")
        .partition("sdd3", "sdd")
        .partition("sde1", "sde")
        .partition("sdf2", "sdf")
        .partition("nvme0n1p1", "nvme0n1")
        .partition("nvme1n1p1", "nvme1n1")
        .partition("nvme2n1p1", "nvme2n1")
        .disk("sda", Some(true))
        .disk("sdb", Some(true))
        .disk("sdc", Some(true))
        .disk("sdd", Some(true))
        .disk("sde", Some(true))
        .disk("sdf", Some(false))
        .disk("nvme0n1", Some(false))
        .disk("nvme1n1", Some(false))
        .disk("nvme2n1", Some(false))
        .made_of("md0", &["sda1", "sdb1"], Some(true))
        .made_of("md2", &["sda3", "sdb3", "sdc3", "sdd3"], Some(true))
        .made_of("md3", &["nvme1n1p1", "nvme2n1p1"], Some(true))
        // Unraid's: no members shown, and a flag that isn't its disk's.
        .disk("md1", Some(false))
        // A ZFS volume says it doesn't spin, whatever the pool's disks do.
        .disk("zd0", Some(false));
        let under = |name: &str| whole_disks(name, &|name| sys.device_info(name));
        let md2 = ["sda", "sdb", "sdc", "sdd"].map(str::to_owned);
        assert_eq!(under("md2"), Some((md2.into(), false)));

        let table = Mutex::default();
        // md0 (/boot, sda1 and sdb1) and md2 (sda3 to sdd3) share disks with
        // the root's sda2: one storage.
        for path in ["/etc/x", "/boot/vmlinuz", "/data/movies/a.mkv"] {
            assert_eq!(linux_of(&table, &sys, path), disk("disk:sda", 1), "{path}");
        }
        assert_eq!(linux_of(&table, &sys, "/media/a.mkv"), disk("disk:sde", 1));
        assert_eq!(linux_of(&table, &sys, "/fast/a.mkv"), disk("disk:nvme0n1", SOLID));
        assert_eq!(
            linux_of(&table, &sys, "/mirror/a.mkv"),
            disk("disk:nvme1n1", SOLID),
            "an array of solid-state disks"
        );
        assert_eq!(linux_of(&table, &sys, "/mnt/disk1/a.mkv"), disk("disk:md1", 1), "md flags aren't believed");
        assert_eq!(linux_of(&table, &sys, "/zvol/a.mkv"), disk("disk:zd0", 1), "nor a ZFS volume's");
        assert_eq!(linux_of(&table, &sys, "/srv/btrfs/a.mkv"), disk("disk:sdf", SOLID), "a subvolume's source");
        // Devices /sys doesn't show are one storage: they could be one disk.
        assert_eq!(linux_of(&table, &sys, "/lost/a.mkv"), disk("fs:253:7", 1));
        assert_eq!(linux_of(&table, &sys, "/lost2/a.mkv"), disk("fs:253:7", 1));

        let alone = FakeLinux::with("1 0 8:97 / / rw - ext4 /dev/sdg1 rw\n");
        assert_eq!(linux_of(&Mutex::default(), &alone, "/a.mkv"), disk("fs:8:97", 1));
    }

    #[test]
    fn union_pools_say_which_disk_holds_a_file() {
        let mut sys = FakeLinux::with(
            "\
1 0 0:30 / / rw - overlay overlay rw
2 1 0:44 / /mnt/user rw - fuse.shfs shfs rw
3 1 9:3 / /mnt/disk3 rw - xfs /dev/md3p1 rw
4 1 0:45 / /media rw - fuse.shfs shfs rw
5 1 0:46 / /pool rw - fuse.mergerfs /mnt/disk1:/mnt/disk2 rw
6 1 8:33 / /mnt/disk2 rw - xfs /dev/sdc1 rw
",
        )
        .device("9:3", "md3p1")
        .device("8:33", "sdc1")
        .partition("md3p1", "md3")
        .disk("md3", Some(false))
        .partition("sdc1", "sdc")
        .disk("sdc", Some(true));
        for (file, member) in [
            ("/mnt/user/movies/a.mkv", "disk3"),
            ("/media/movies/b.mkv", "disk4"),
            ("/media/movies/c.mkv", "../etc"),
            ("/pool/tv/d.mkv", "/mnt/disk2"),
            ("/pool/tv/e.mkv", "/mnt/disk1"),
        ] {
            sys.members.insert(file.into(), member.into());
        }
        sys.here.insert("/mnt/disk2".into());
        let table = Mutex::default();
        // /mnt/disk3 is mounted here: its own disk.
        assert_eq!(linux_of(&table, &sys, "/mnt/user/movies/a.mkv"), disk("disk:md3", 1));
        assert_eq!(linux_of(&table, &sys, "/mnt/disk3/movies/a.mkv"), disk("disk:md3", 1));
        // /mnt/disk4 isn't (a container sees the share only).
        assert_eq!(linux_of(&table, &sys, "/media/movies/b.mkv"), disk("shfs:disk4", 1));
        // No answer, or one that names no disk: the file may be on any, and goes alone.
        assert_eq!(linux_of(&table, &sys, "/media/movies/x.mkv"), disk("shfs:/media", 0));
        assert_eq!(linux_of(&table, &sys, "/media/movies/c.mkv"), disk("shfs:/media", 0));
        // mergerfs's branch: the disk of the mount it is on, where it is here.
        assert_eq!(linux_of(&table, &sys, "/pool/tv/d.mkv"), disk("disk:sdc", 1));
        assert_eq!(linux_of(&table, &sys, "/pool/tv/e.mkv"), disk("mergerfs:/mnt/disk1", 1));
        assert_eq!(linux_of(&table, &sys, "/pool/tv/f.mkv"), disk("mergerfs:/pool", 0));
    }

    #[test]
    fn folders_are_looked_at_once_and_mounts_read_again_after_a_minute() {
        let mut sys =
            FakeLinux::with("1 0 8:1 / / rw - ext4 /dev/sda1 rw\n").device("8:1", "sda1").disk("sda1", Some(true));
        sys.links.insert("/media/movies".into(), "/mnt/disk2/movies".into());
        let table = Mutex::default();
        let start = Instant::now();
        let of = |path: &str, at: Duration| mounts::disk_of(&table, Path::new(path), &sys, start + at);
        for file in ["a.mkv", "b.mkv", "c.mkv"] {
            assert_eq!(of(&format!("/media/movies/{file}"), Duration::ZERO), disk("disk:sda1", 1));
        }
        assert_eq!((sys.reads.get(), sys.canonicalized.get()), (1, 1));
        // A disk mounted since, under the folder a link leads to.
        *sys.mountinfo.borrow_mut() =
            Some("1 0 8:1 / / rw - ext4 /dev/sda1 rw\n2 1 8:17 / /mnt/disk2 rw - ext4 /dev/sdb1 rw\n".to_owned());
        assert_eq!(of("/media/movies/a.mkv", Duration::from_secs(59)), disk("disk:sda1", 1));
        assert_eq!(of("/media/movies/a.mkv", Duration::from_secs(61)), disk("fs:8:17", 1));
        assert_eq!((sys.reads.get(), sys.canonicalized.get()), (2, 2));
        // Without /proc, all files are one storage.
        *sys.mountinfo.borrow_mut() = None;
        assert_eq!(of("/media/movies/a.mkv", Duration::from_secs(122)), disk(UNKNOWN, 1));
    }

    #[test]
    fn an_automounted_share_is_read_once_it_is_mounted() {
        let sys = FakeLinux::with("1 0 8:1 / / rw - ext4 /dev/sda1 rw\n2 1 0:40 / /mnt/nas rw - autofs systemd-1 rw\n");
        let table = Mutex::default();
        let start = Instant::now();
        // Following the folder mounts the share, which then shows.
        let mounted = "1 0 8:1 / / rw - ext4 /dev/sda1 rw\n2 1 0:40 / /mnt/nas rw - autofs systemd-1 rw\n\
                       3 2 0:41 / /mnt/nas rw - nfs4 nas:/media rw\n";
        mounts::disk_of(&table, Path::new("/etc/x"), &sys, start);
        *sys.mountinfo.borrow_mut() = Some(mounted.to_owned());
        assert_eq!(mounts::disk_of(&table, Path::new("/mnt/nas/a.mkv"), &sys, start), disk("net:nas", 1));
        assert_eq!(sys.reads.get(), 2);
        // One that doesn't mount is read again once a second at most.
        let sys = FakeLinux::with("1 0 0:40 / /mnt/nas rw - autofs systemd-1 rw\n");
        let table = Mutex::default();
        for dir in ["a", "b", "c"] {
            let path = format!("/mnt/nas/{dir}/x.mkv");
            assert_eq!(mounts::disk_of(&table, Path::new(&path), &sys, start), disk("fs:0:40", 0));
        }
        assert_eq!(sys.reads.get(), 2);
    }

    /// A Windows of fixtures, which counts the volumes asked for.
    #[derive(Default)]
    struct FakeWindows {
        /// Network drives, and what each stands for.
        remote: HashMap<String, String>,
        /// Volume roots, and the volume mounted at each.
        roots: Vec<(String, String)>,
        /// Volumes, and the disks each is on where it says.
        volumes: Vec<(String, Option<BTreeSet<u32>>)>,
        /// Disks, and whether they have a seek penalty.
        penalties: HashMap<u32, bool>,
        /// A disk slow to answer.
        slow: Option<Slow>,
        asked: AtomicUsize,
    }

    /// A disk of [`FakeWindows`] that says when it is asked for its seek
    /// penalty, and answers once told to.
    struct Slow {
        disk: u32,
        asked: mpsc::Sender<()>,
        answer: Mutex<mpsc::Receiver<()>>,
    }

    impl Windows for FakeWindows {
        fn canonicalize(&self, dir: &Path) -> Option<PathBuf> {
            Some(dir.to_path_buf())
        }

        fn is_remote(&self, drive: &str) -> bool {
            self.remote.contains_key(drive)
        }

        fn dos_device(&self, drive: &str) -> Option<String> {
            self.remote.get(drive).cloned()
        }

        fn volume_root(&self, path: &Path) -> Option<String> {
            // A volume's root is its own: `C:\mnt\disk6` is on `C:\mnt\disk6\`.
            let path = format!(r"{}\", path.to_string_lossy().to_lowercase());
            let roots = self.roots.iter().map(|(root, _)| root);
            roots.filter(|root| path.starts_with(&root.to_lowercase())).max_by_key(|root| root.len()).cloned()
        }

        fn volume_name(&self, root: &str) -> Option<String> {
            self.asked.fetch_add(1, Ordering::Relaxed);
            self.roots.iter().find(|(r, _)| r == root).map(|(_, volume)| volume.clone())
        }

        fn volumes(&self) -> Vec<String> {
            self.volumes.iter().map(|(volume, _)| volume.clone()).collect()
        }

        fn disks(&self, volume: &str) -> Option<BTreeSet<u32>> {
            self.volumes.iter().find(|(v, _)| v == volume)?.1.clone()
        }

        fn seek_penalty(&self, disk: u32) -> Option<bool> {
            if let Some(slow) = self.slow.as_ref().filter(|slow| slow.disk == disk) {
                let _ = slow.asked.send(());
                let _ = slow.answer.lock().recv();
            }
            self.penalties.get(&disk).copied()
        }
    }

    #[test]
    fn windows_paths_are_their_server_or_disk() {
        assert_eq!(unc_server(r"\\NAS\Media\x").as_deref(), Some("nas"));
        assert_eq!(unc_server(r"\\?\UNC\nas\tv\x").as_deref(), Some("nas"));
        assert_eq!(unc_server(r"\\?\C:\Movies"), None);
        assert_eq!(unc_server(r"\\.\PhysicalDrive0"), None);
        assert_eq!(unc_server(r"C:\Movies"), None);
        assert_eq!(mapped_server(r"\Device\LanmanRedirector\;Z:0000000000012345\nas\media").as_deref(), Some("nas"));
        assert_eq!(
            mapped_server(r"\Device\Mup\;LanmanRedirector\;Z:000000000002a3f5\nas\media").as_deref(),
            Some("nas")
        );
        assert_eq!(mapped_server(r"\Device\HarddiskVolume3"), None);
        assert_eq!(mapped_server(r"\Device\Mup\;RdpDr\;:1\tsclient\C"), None);
        assert_eq!(volumes::drive_of(r"\\?\d:\Movies").as_deref(), Some("D:"));
        assert_eq!(volumes::drive_of(r"\\nas\media"), None);
        // Disk numbers: three volumes, two of them sharing disk 2.
        let volumes = [BTreeSet::from([0]), BTreeSet::from([1, 2]), BTreeSet::from([2])];
        assert_eq!(groups(&volumes.iter().collect::<Vec<_>>()), [0, 1, 1]);
        let strings =
            [BTreeSet::from(["b"]), BTreeSet::from(["c"]), BTreeSet::from(["a", "c"]), BTreeSet::from(["a", "b"])];
        assert_eq!(groups(&strings.iter().collect::<Vec<_>>()), [0, 0, 0, 0], "joined through others");

        let volume = |n: u32| format!(r"\\?\Volume{{0000000{n}-0000-0000-0000-000000000000}}\");
        let sys = FakeWindows {
            remote: [
                ("Z:".to_owned(), r"\Device\Mup\;LanmanRedirector\;Z:000000000002a3f5\nas\media".to_owned()),
                ("Y:".to_owned(), r"\Device\Mup\;RdpDr\;:1\tsclient\C".to_owned()),
            ]
            .into(),
            roots: vec![
                (r"C:\".into(), volume(1)),
                (r"D:\".into(), volume(2)),
                (r"E:\".into(), volume(3)),
                (r"F:\".into(), volume(4)),
                (r"G:\".into(), volume(5)),
                (r"C:\mnt\disk6\".into(), volume(6)),
            ],
            volumes: vec![
                (volume(1), Some([0].into())),
                (volume(2), Some([1, 2].into())),
                (volume(3), Some([2].into())),
                (volume(4), None),
                (volume(5), Some([5].into())),
                (volume(6), Some([6].into())),
            ],
            penalties: [(0, false), (1, true), (2, false), (6, true)].into(),
            ..FakeWindows::default()
        };
        let table = Mutex::default();
        let of = |path: &str| volumes::disk_of(&table, Path::new(path), &sys, Instant::now());
        assert_eq!(of(r"C:\Media\Alpha (2001).mkv"), disk("disk:0", SOLID));
        assert_eq!(of(r"D:\Movies\a.mkv"), disk("disk:1", 1), "joined with E:, on disk 1 that spins");
        assert_eq!(of(r"E:\Movies\a.mkv"), disk("disk:1", 1));
        assert_eq!(of(r"C:\mnt\disk6\a.mkv"), disk("disk:6", 1), "a volume in a folder");
        assert_eq!(of(r"F:\a.mkv"), disk("volume:{00000004-0000-0000-0000-000000000000}", 0), "its disks unknown");
        assert_eq!(of(r"G:\a.mkv"), disk("disk:5", 1), "a disk that doesn't say spins");
        assert_eq!(of(r"Q:\a.mkv"), disk(r"volume:Q:\", 0), "no volume");
        let asked = sys.asked.load(Ordering::Relaxed);
        assert_eq!(of(r"\\NAS\Media\Movies\a.mkv"), disk("net:nas", 1));
        assert_eq!(of(r"\\?\UNC\nas\tv\a.mkv"), disk("net:nas", 1));
        assert_eq!(of(r"Z:\Movies\a.mkv"), disk("net:nas", 1), "a mapped drive whose share didn't answer");
        assert_eq!(of(r"Y:\a.mkv"), disk("net:Y:", 1));
        assert_eq!(sys.asked.load(Ordering::Relaxed), asked, "no volume asked for shares");
    }

    #[test]
    fn a_disk_slow_to_answer_holds_up_only_its_own_lookup() {
        let volume = |n: u32| format!(r"\\?\Volume{{0000000{n}-0000-0000-0000-000000000000}}\");
        let (asked, asking) = mpsc::channel();
        let (answer, answering) = mpsc::channel();
        let sys = FakeWindows {
            roots: vec![(r"C:\".into(), volume(1)), (r"D:\".into(), volume(2)), (r"E:\".into(), volume(3))],
            volumes: vec![(volume(1), Some([0].into())), (volume(2), Some([1].into())), (volume(3), Some([2].into()))],
            penalties: [(0, false), (1, true), (2, false)].into(),
            slow: Some(Slow { disk: 1, asked, answer: Mutex::new(answering) }),
            ..FakeWindows::default()
        };
        let table = Mutex::default();
        let of = |path: &str| volumes::disk_of(&table, Path::new(path), &sys, Instant::now());
        assert_eq!(of(r"C:\Media\a.mkv"), disk("disk:0", SOLID));
        let wait = Duration::from_secs(10);
        std::thread::scope(|scope| {
            let slow = scope.spawn(|| of(r"D:\Movies\a.mkv"));
            asking.recv_timeout(wait).expect("disk 1 is asked");
            // While disk 1 doesn't answer, a folder found before and one on
            // another volume are found.
            let (found, finding) = mpsc::channel();
            scope.spawn(move || {
                for path in [r"C:\Media\b.mkv", r"E:\Shows\a.mkv"] {
                    let _ = found.send(of(path));
                }
            });
            let quick: Vec<Disk> = (0..2).map_while(|_| finding.recv_timeout(wait).ok()).collect();
            answer.send(()).unwrap();
            assert_eq!(quick, [disk("disk:0", SOLID), disk("disk:2", SOLID)]);
            assert_eq!(slow.join().unwrap(), disk("disk:1", 1));
        });
    }

    #[test]
    fn fixed_disks_and_external_transcoders() {
        let fixed = Source::Fixed(vec![("disk1/".into(), disk("disk1", 1)), ("disk2/".into(), disk("disk2", 4))]);
        for external in [false, true] {
            let disks = Disks::new(fixed.clone(), external);
            assert_eq!(disks.of(Path::new("/media/movies/disk1/Alpha (2001).mp4")), disk("disk1", 1));
            assert_eq!(disks.of(Path::new(r"C:\media\movies\disk2\Bravo (2002).mp4")), disk("disk2", 4));
            assert_eq!(disks.of(Path::new("/media/movies/disk3/Charlie.mp4")), disk("fixed", 1));
        }
        let external = Disks::new(Source::System, true);
        for path in ["/", "/mnt/user/media/a.mkv", r"C:\Windows", r"\\nas\media\a.mkv"] {
            assert_eq!(external.of(Path::new(path)), disk("transcoder", 1));
        }
        fn shared<T: Send + Sync>() {}
        shared::<Disks>();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_computer_s_root_is_on_a_storage() {
        let disk = Disks::new(Source::System, false).of(Path::new("/"));
        assert!(!disk.key.is_empty() && disk.width >= 1, "{disk:?}");
    }

    #[cfg(windows)]
    #[test]
    fn this_computer_s_windows_folder_is_on_a_disk() {
        let windows = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let disk = Disks::new(Source::System, false).of(&windows);
        // Not its width: CI's disks are virtual.
        assert!(disk.key.starts_with("disk:") || disk.key.starts_with("volume:"), "{disk:?}");
    }
}
