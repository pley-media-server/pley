// SPDX-License-Identifier: AGPL-3.0-only
//! Pruning of the work directory's caches (keyframe indexes, converted
//! subtitles, seek-preview sheets, libplacebo's shaders), which would otherwise grow with every file
//! ever played. Entries unused for too long go, then the least recently used
//! ones until a folder fits its budget; "used" is the modification time,
//! which readers refresh ([`crate::util::touch`]). Temporaries left by a crash
//! go once they are clearly abandoned.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::util::dir_size;

const DAY: Duration = Duration::from_secs(24 * 3600);
/// Temporaries left alone longer than this are abandoned: seek previews and
/// FFmpeg passes over whole files for their subtitles, which may run or wait
/// paused longer, mark theirs as being written every few minutes.
const STALE_TEMPORARY: Duration = Duration::from_secs(3 * 3600);
const FIRST_RUN: Duration = Duration::from_secs(120);
const EVERY: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// Entries unused for longer go.
    pub max_age: Duration,
    /// The least recently used entries go until the folder fits.
    pub max_bytes: u64,
}

/// What each cache folder of the work directory may keep.
pub fn policies(work_dir: &Path) -> Vec<(PathBuf, Policy)> {
    vec![
        // Read again from the file in milliseconds (Matroska, MP4) or rebuilt
        // in the background (other containers).
        (work_dir.join("keyframes"), Policy { max_age: 90 * DAY, max_bytes: 256 << 20 }),
        // Extracting a track reads the whole file again: keep them a while.
        (work_dir.join("subtitles"), Policy { max_age: 60 * DAY, max_bytes: 512 << 20 }),
        // The server copies the sheets right after they are made (and they
        // are deleted once every one has been fetched); what's left is only
        // there for a retry.
        (work_dir.join("trickplay"), Policy { max_age: DAY, max_bytes: 1 << 30 }),
        // libplacebo's compiled shaders, a few MB for each kind of tone
        // mapping: rebuilt in a fraction of a second by the next process
        // that needs them.
        (work_dir.join("libplacebo"), Policy { max_age: 30 * DAY, max_bytes: 128 << 20 }),
        // Converted downloads: made again when asked once gone, which takes
        // minutes. A week unused, or more than this many movies' worth.
        (work_dir.join("conversions"), Policy { max_age: 7 * DAY, max_bytes: 64 << 30 }),
    ]
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    pub entries: usize,
    pub bytes: u64,
}

/// Prune the caches every hour, starting shortly after start-up (`shared`:
/// see [`prune_all`]).
pub fn spawn_janitor(work_dir: PathBuf, shared: bool) {
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_RUN).await;
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tick.tick().await;
            let dir = work_dir.clone();
            let pruned = tokio::task::spawn_blocking(move || prune_all(&dir, shared, SystemTime::now()))
                .await
                .unwrap_or_default();
            if pruned.entries > 0 {
                tracing::info!(
                    "cache pruning removed {} entries ({:.1} MB)",
                    pruned.entries,
                    pruned.bytes as f64 / 1_048_576.0
                );
            }
        }
    });
}

/// Prune every cache of `work_dir`. In a work folder other accounts may
/// write in (`shared`), a cache that became a link or a junction may be
/// theirs, leading to files that aren't the cache's: it is left as it is.
/// Elsewhere one is this account's own (the cache kept on another disk), and
/// pruned. Blocking.
pub fn prune_all(work_dir: &Path, shared: bool, now: SystemTime) -> Pruned {
    let mut total = Pruned::default();
    for (folder, policy) in policies(work_dir) {
        if shared && std::fs::symlink_metadata(&folder).is_ok_and(|meta| meta.file_type().is_symlink()) {
            tracing::warn!("{} is a link, not a cache: left as it is", folder.display());
            continue;
        }
        let p = prune(&folder, &policy, now);
        total.entries += p.entries;
        total.bytes += p.bytes;
    }
    total
}

/// Prune one cache folder whose entries are files, or folders of files.
/// Blocking.
pub fn prune(dir: &Path, policy: &Policy, now: SystemTime) -> Pruned {
    let mut pruned = Pruned::default();
    let Ok(entries) = std::fs::read_dir(dir) else { return pruned };
    let mut kept = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let size = if meta.is_dir() { dir_size(&path) } else { meta.len() };
        let used = last_used(&path, &meta);
        let age = now.duration_since(used).unwrap_or_default();
        let temporary = path.extension().is_some_and(|e| e == "tmp" || e == "part");
        let limit = if temporary { STALE_TEMPORARY } else { policy.max_age };
        if age > limit {
            remove(&path, meta.is_dir(), size, &mut pruned);
        } else if !temporary {
            kept.push((used, size, path, meta.is_dir()));
        }
    }
    let mut total: u64 = kept.iter().map(|(_, size, ..)| size).sum();
    if total > policy.max_bytes {
        kept.sort_by_key(|(used, ..)| *used);
        for (_, size, path, is_dir) in kept {
            if total <= policy.max_bytes {
                break;
            }
            if remove(&path, is_dir, size, &mut pruned) {
                total -= size;
            }
        }
    }
    pruned
}

/// When an entry was last used: a folder (a set of preview sheets) by its
/// index file, which readers touch.
fn last_used(path: &Path, meta: &std::fs::Metadata) -> SystemTime {
    let index = meta.is_dir().then(|| std::fs::metadata(path.join("index.json")).and_then(|m| m.modified()).ok());
    index.flatten().or_else(|| meta.modified().ok()).unwrap_or(SystemTime::UNIX_EPOCH)
}

fn remove(path: &Path, is_dir: bool, size: u64, pruned: &mut Pruned) -> bool {
    let removed = if is_dir { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
    match removed {
        Ok(()) => {
            pruned.entries += 1;
            pruned.bytes += size;
            true
        }
        Err(err) => {
            tracing::debug!("could not prune {}: {err}", path.display());
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(dir: &Path, name: &str, bytes: usize, age: Duration) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, vec![b'x'; bytes]).unwrap();
        set_age(&path, age);
        path
    }

    fn set_age(path: &Path, age: Duration) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - age).unwrap();
    }

    #[test]
    fn old_entries_go_then_the_least_recently_used() {
        let dir = tempfile::tempdir().unwrap();
        let policy = Policy { max_age: 10 * DAY, max_bytes: 250 };
        let fresh = file(dir.path(), "a.txt", 100, DAY);
        let older = file(dir.path(), "b.txt", 100, 3 * DAY);
        let oldest = file(dir.path(), "c.txt", 100, 5 * DAY);
        let expired = file(dir.path(), "d.txt", 10, 11 * DAY);
        let pruned = prune(dir.path(), &policy, SystemTime::now());
        assert_eq!(pruned, Pruned { entries: 2, bytes: 110 });
        assert!(fresh.exists() && older.exists());
        assert!(!oldest.exists(), "least recently used, over the budget");
        assert!(!expired.exists(), "unused for too long");
        // Within budget and age: nothing happens.
        assert_eq!(prune(dir.path(), &policy, SystemTime::now()), Pruned::default());
        assert_eq!(prune(&dir.path().join("missing"), &policy, SystemTime::now()), Pruned::default());
    }

    #[test]
    fn abandoned_temporaries_go_but_ones_being_written_stay() {
        let dir = tempfile::tempdir().unwrap();
        let policy = Policy { max_age: 365 * DAY, max_bytes: u64::MAX };
        let part = file(dir.path(), "k_3.vtt.part", 10, Duration::ZERO);
        let tmp = file(dir.path(), "k.txt.0123.tmp", 10, Duration::ZERO);
        let set = dir.path().join("id-2000-320.part");
        std::fs::create_dir(&set).unwrap();
        file(&set, "0.jpg", 10, Duration::ZERO);
        let entry = file(dir.path(), "k.txt", 10, Duration::ZERO);
        let hour = Duration::from_secs(3600);
        // An hour on, they may still be being written.
        assert_eq!(prune(dir.path(), &policy, SystemTime::now() + hour), Pruned::default());
        // Four hours on, they are abandoned; finished entries stay.
        assert_eq!(prune(dir.path(), &policy, SystemTime::now() + 4 * hour).entries, 3);
        assert!(!part.exists() && !tmp.exists() && !set.exists() && entry.exists());
    }

    #[test]
    fn preview_sets_count_as_used_when_their_index_is() {
        let dir = tempfile::tempdir().unwrap();
        let policy = Policy { max_age: DAY, max_bytes: u64::MAX };
        let old = dir.path().join("old-2000-320");
        let used = dir.path().join("used-2000-320");
        for set in [&old, &used] {
            std::fs::create_dir(set).unwrap();
            file(set, "0.jpg", 100, 2 * DAY);
        }
        file(&old, "index.json", 10, 2 * DAY);
        file(&used, "index.json", 10, Duration::from_secs(60));
        let pruned = prune(dir.path(), &policy, SystemTime::now());
        assert_eq!(pruned, Pruned { entries: 1, bytes: 110 });
        assert!(!old.exists() && used.exists());
    }

    /// A folder link at `link` to `target`: a symbolic link, or on Windows a
    /// junction, which any account may make where it may write.
    fn link_folder(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        {
            let made = std::process::Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(made.success());
        }
    }

    #[test]
    fn a_cache_that_is_a_link_is_pruned_only_where_it_is_this_accounts() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let old = file(&elsewhere, "photos.zip", 100, 400 * DAY);
        let work = dir.path().join("work");
        std::fs::create_dir(&work).unwrap();
        link_folder(&elsewhere, &work.join("trickplay"));
        // Where other accounts may write, the link may be theirs: its files
        // aren't the cache's to prune, however old.
        assert_eq!(prune_all(&work, true, SystemTime::now()), Pruned::default());
        assert!(old.exists());
        // Elsewhere it is this account's, keeping the cache on another disk.
        assert_eq!(prune_all(&work, false, SystemTime::now()).entries, 1);
        assert!(!old.exists());
    }
}
