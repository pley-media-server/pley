//! Where people's media may be on this machine: the folders Browse starts
//! from, and those mounted into a container. The Plex import looks there
//! for Plex's folders when Plex sees them at another path.

use std::path::{Path, PathBuf};

/// A folder Browse starts from.
pub struct Root {
    pub name: String,
    pub path: String,
}

/// The drives on Windows; elsewhere `/` and the usual places for disks and
/// shares. Home on both.
pub fn roots() -> Vec<Root> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            if Path::new(&root).exists() {
                out.push(Root { name: format!("{}:", letter as char), path: root });
            }
        }
    } else {
        out.push(Root { name: "/".into(), path: "/".into() });
        for candidate in ["/mnt", "/media", "/srv", "/Volumes"] {
            if Path::new(candidate).is_dir() {
                out.push(Root { name: candidate.into(), path: candidate.into() });
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        out.push(Root { name: "Home".into(), path: home.to_string_lossy().into_owned() });
    }
    out
}

/// The folders mounted into the container Pley runs in (`/media`,
/// `/movies`); none outside one.
pub fn mounts(data_dir: &Path) -> Vec<String> {
    if !crate::network::in_container() {
        return Vec::new();
    }
    std::fs::read_to_string("/proc/self/mountinfo").map(|text| mounted_folders(&text, data_dir)).unwrap_or_default()
}

/// Where a container's own mounts are: Docker's `/etc/hosts`, `/proc` and
/// `/dev`, NVIDIA's driver files, Podman's `/run/.containerenv`,
/// Kubernetes' secrets. Media mounted there would be odd.
const SYSTEM_FOLDERS: &[&str] =
    &["/proc", "/sys", "/dev", "/etc", "/run", "/usr", "/var", "/tmp", "/lib", "/lib64", "/bin", "/sbin"];

/// The folders mounted into a container, from `/proc/self/mountinfo`'s
/// text: what someone gave Pley (`/media`, `/movies`), without the
/// container's own mounts or Pley's data folder. A folder inside another
/// that's listed is left out.
pub(crate) fn mounted_folders(mountinfo: &str, data_dir: &Path) -> Vec<String> {
    let mut points: Vec<PathBuf> = mountinfo
        .lines()
        .filter_map(|line| line.split(' ').nth(4))
        .map(|point| PathBuf::from(unescape_mount(point)))
        .filter(|point| {
            point.parent().is_some()
                && !point.starts_with(data_dir)
                && !SYSTEM_FOLDERS.iter().any(|system| point.starts_with(system))
        })
        .collect();
    points.sort();
    points.dedup();
    let mut kept: Vec<PathBuf> = Vec::new();
    for point in points {
        if !kept.iter().any(|outer| point.starts_with(outer)) {
            kept.push(point);
        }
    }
    kept.into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
}

/// mountinfo writes a space, a tab, a newline and a backslash in a path as
/// `\040`, `\011`, `\012` and `\134`.
fn unescape_mount(point: &str) -> String {
    let bytes = point.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal =
            bytes.get(i + 1..i + 4).filter(|d| bytes[i] == b'\\' && d.iter().all(|c| (b'0'..=b'7').contains(c)));
        match octal {
            Some(d) => {
                out.push((d[0] - b'0') * 64 + (d[1] - b'0') * 8 + (d[2] - b'0'));
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// How many folders inside each root are looked in, so that a full disk's
/// top level doesn't make a search slow.
const PER_ROOT: usize = 64;

/// Where to look for a folder another machine sees at another path: `/`,
/// the roots and mounts, and the folders directly inside each root but `/`
/// (`/mnt/user`, `/mnt/nas`).
pub fn bases(data_dir: &Path) -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = roots().into_iter().map(|r| PathBuf::from(r.path)).collect();
    bases.extend(mounts(data_dir).into_iter().map(PathBuf::from));
    let inside: Vec<PathBuf> = bases
        .iter()
        .filter(|base| base.parent().is_some())
        .flat_map(|base| {
            let Ok(entries) = std::fs::read_dir(base) else { return Vec::new() };
            entries
                .flatten()
                .filter(|e| {
                    e.file_type().is_ok_and(|t| t.is_dir()) && !e.file_name().to_string_lossy().starts_with('.')
                })
                .map(|e| e.path())
                .take(PER_ROOT)
                .collect()
        })
        .collect();
    bases.extend(inside);
    let mut seen = std::collections::HashSet::new();
    bases.retain(|base| seen.insert(base.clone()) && !base.starts_with(data_dir));
    bases
}

/// Where `location`, a folder as another machine sees it (`/data/media/movies`,
/// `D:\Movies`), is on this one: its path's longest tail (`media/movies`,
/// then `movies`) under one of `bases`, followed a part at a time by name,
/// exactly or else ignoring case. The first that `is_it` takes.
pub fn find(location: &str, bases: &[PathBuf], is_it: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let parts: Vec<&str> = location.split(['/', '\\']).filter(|p| !p.is_empty() && !p.ends_with(':')).collect();
    (0..parts.len())
        .flat_map(|from| bases.iter().map(move |base| (from, base)))
        .filter_map(|(from, base)| follow(base, &parts[from..]))
        .find(|dir| is_it(dir))
}

/// `parts` under `base`, each the folder of that name or else of that name
/// in another case, as the folder spells it (on a disk that ignores case,
/// `movies` opens `Movies`, but its name is `Movies`).
fn follow(base: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut here = base.to_path_buf();
    for part in parts {
        let folders: Vec<PathBuf> = std::fs::read_dir(&here)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() || t.is_symlink()) && e.path().is_dir())
            .map(|e| e.path())
            .collect();
        let named = |exact: bool| {
            folders.iter().find(|f| {
                let name = f.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                if exact { name == *part } else { name.eq_ignore_ascii_case(part) }
            })
        };
        here = named(true).or_else(|| named(false))?.clone();
    }
    Some(here)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_containers_mounted_folders_are_told_from_its_own() {
        // Docker with the Compose file's two folders, one with a space, and NVIDIA's files.
        let docker = "\
612 540 0:56 / / rw,relatime master:301 - overlay overlay rw,lowerdir=/var/lib/docker/overlay2/l/A
613 612 0:58 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw
614 612 0:59 / /dev rw,nosuid - tmpfs tmpfs rw,size=65536k,mode=755
620 612 8:1 /srv/pley /config rw,relatime - ext4 /dev/sda1 rw
621 612 8:17 /tank/media /media ro,relatime - ext4 /dev/sdb1 rw
622 621 8:17 /tank/media/Movies /media/Movies ro,relatime - ext4 /dev/sdb1 rw
623 612 8:17 /tank/Home\\040videos /home\\040videos ro,relatime - ext4 /dev/sdb1 rw
624 612 8:1 /var/lib/docker/containers/x/resolv.conf /etc/resolv.conf rw,relatime - ext4 /dev/sda1 rw
625 612 8:1 /var/lib/docker/containers/x/hosts /etc/hosts rw,relatime - ext4 /dev/sda1 rw
626 612 8:1 /usr/bin/nvidia-smi /usr/bin/nvidia-smi ro,nosuid,nodev,relatime - ext4 /dev/sda1 rw
627 612 0:25 /nvidia-persistenced/socket /run/nvidia-persistenced/socket rw - tmpfs tmpfs rw
628 613 0:5 /driver/nvidia /proc/driver/nvidia rw - proc proc rw
";
        assert_eq!(mounted_folders(docker, Path::new("/config")), vec!["/home videos", "/media"]);

        // Kubernetes adds its own, and nothing was mounted for media.
        let kubernetes = "\
1 0 0:56 / / rw - overlay overlay rw
2 1 8:1 /pods/x/volumes/config /config rw - ext4 /dev/sda1 rw
3 1 8:1 /pods/x/etc-hosts /etc/hosts rw - ext4 /dev/sda1 rw
4 1 8:1 /pods/x/termination-log /dev/termination-log rw - ext4 /dev/sda1 rw
5 1 0:61 / /var/run/secrets/kubernetes.io/serviceaccount ro - tmpfs tmpfs rw
";
        assert!(mounted_folders(kubernetes, Path::new("/config")).is_empty());
        assert!(mounted_folders("", Path::new("/config")).is_empty());
    }

    /// A folder of `files` (name, size) under `root`.
    fn folder_of(root: &Path, path: &str, files: &[(&str, usize)]) -> PathBuf {
        let dir = root.join(path);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, size) in files {
            std::fs::write(dir.join(name), vec![0u8; *size]).unwrap();
        }
        dir
    }

    #[test]
    fn plexs_folders_are_found_where_this_machine_sees_them() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let movies = folder_of(root, "media/Movies", &[("Heat (1995).mkv", 3)]);
        let tv = folder_of(root, "mnt/nas/tv", &[("Show - S01E01.mkv", 4)]);
        // The same name, without Plex's files.
        folder_of(root, "srv/movies", &[]);
        let bases = [root.to_path_buf(), root.join("srv"), root.join("mnt/nas")];
        let has = |name: &'static str| move |dir: &Path| dir.join(name).is_file();

        // A longer tail first, and the name in another case.
        assert_eq!(find("/data/media/movies", &bases, has("Heat (1995).mkv")), Some(movies.clone()));
        // Under a folder inside a root, and from a Windows path.
        assert_eq!(find("D:\\Plex\\TV", &bases, has("Show - S01E01.mkv")), Some(tv));
        // `srv/movies` is a movies folder, but not Plex's.
        assert_eq!(find("/plex/movies", &[root.join("srv"), root.to_path_buf()], has("Heat (1995).mkv")), None);
        assert_eq!(find("/data/media/music", &bases, |_| true), None);
        assert_eq!(find("/", &bases, |_| true), None);
    }
}
