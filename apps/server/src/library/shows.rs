//! Which show each show folder of a TV library is.
//!
//! A show is found by its folder's name: its TMDB id (`{tmdb-1234}`), else
//! its title and year. Folders with the same name in a library's several
//! folders (drives) are one show, as a show spread over them is, a season
//! on each. So are names that differ only in their punctuation
//! (`Show.Name`, `Show Name`). Two different shows can have such names,
//! though, when their folders give no year: the ids of a folder's
//! `tvshow.nfo`, else those in its name (`{tvdb-123456}`,
//! `{imdb-tt1234567}`), tell them apart. A folder whose ids differ from
//! those of the other folders of its show is a show of its own, keyed by
//! its name's key and one of its ids (`show:<title>:<year>:tvdb:123456`),
//! with the folders that have its ids. Folders without ids stay with the
//! show of their name.
//!
//! A folder stays in the show its files are in while its ids fit the one
//! the show's key names and those of the folders that stay there before
//! it: the folder the show is named after (its path) first, so that a show
//! keeps its own folder, and with it its match and what was done with it.
//! It doesn't stay when another show's key names one of its ids and its
//! own show's names none. While a show's own folder can't be read, the show
//! is known by the ids that folder had. The folders that don't stay go to
//! the show of their ids, else to the show of their name (for one without
//! ids, the only show of the name when the name makes none).
//! A folder that leaves its show takes what was watched of its episodes
//! along, as files renamed do (the scanner's `relink`).
//!
//! A show's folders are placed together. A scan of the part of a library
//! that changed takes those of the rest to be as they were: when a folder
//! would go into or out of a show that has folders there, comes with its
//! name, or the `tvshow.nfo` of one of its folders changed, all of the
//! library is scanned instead ([`plan`]).

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use sqlx::SqliteConnection;

use super::nfo::{self, NfoIds};
use super::parse::{parse_episode, show_dir, show_ids};
use super::scanner::{ItemIds, is_show_entry};
use super::sidecar::Sidecars;
use super::walk::FoundFile;
use crate::db::Db;
use crate::util::{normalize_key, path_str};

/// The key of the show a folder's name makes: its TMDB id, else its title
/// and year.
pub(super) fn name_key(title: &str, year: Option<i32>, tmdb_id: Option<i64>) -> String {
    match tmdb_id {
        Some(id) => format!("show:tmdb:{id}"),
        None => format!("show:{}:{}", normalize_key(title), year.unwrap_or(0)),
    }
}

/// Of a show's key, the key its folders' name makes: the key itself, or
/// what comes before the id that tells it apart. Titles in keys hold no
/// colons ([`normalize_key`]).
fn name_key_of(key: &str) -> &str {
    match key.match_indices(':').nth(2) {
        Some((at, _)) => &key[..at],
        None => key,
    }
}

/// The ids a show's key tells it apart by (`…:tvdb:123456`).
fn ids_in_key(key: &str) -> ItemIds {
    let mut ids = ItemIds::default();
    let Some((source, id)) = key.get(name_key_of(key).len() + 1..).and_then(|rest| rest.split_once(':')) else {
        return ids;
    };
    match source {
        "tmdb" => ids.tmdb_id = id.parse().ok(),
        "tvdb" => ids.tvdb_id = id.parse().ok(),
        "imdb" => ids.imdb_id = Some(id.to_owned()),
        "tvmaze" => ids.tvmaze_id = id.parse().ok(),
        _ => {}
    }
    ids
}

/// The key of a show of the name `key` for a folder with `ids`, which
/// differ from those of the show of that name: by the first of its ids no
/// show has yet (`taken`), else by its folder.
fn apart_key(key: &str, ids: &ItemIds, folder: &Path, taken: impl Fn(&str) -> bool) -> String {
    [
        ids.tmdb_id.map(|id| format!("{key}:tmdb:{id}")),
        ids.tvdb_id.map(|id| format!("{key}:tvdb:{id}")),
        ids.imdb_id.as_ref().map(|id| format!("{key}:imdb:{id}")),
        ids.tvmaze_id.map(|id| format!("{key}:tvmaze:{id}")),
    ]
    .into_iter()
    .flatten()
    .find(|key| !taken(key))
    .unwrap_or_else(|| format!("{key}:folder:{}", path_str(folder)))
}

/// The ids in a show folder's name.
fn named_ids(folder: &Path) -> ItemIds {
    let name = folder.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
    let (tmdb_id, tvdb_id, imdb_id) = show_ids(&name);
    ItemIds { tmdb_id, imdb_id, tvdb_id, tvmaze_id: None }
}

/// The ids a folder is known by: those of its NFO file, else those in its
/// name.
fn folder_ids(nfo: Option<ItemIds>, folder: &Path) -> ItemIds {
    nfo.filter(|ids| *ids != ItemIds::default()).unwrap_or_else(|| named_ids(folder))
}

/// A show folder of the library, as the scan found it.
#[derive(Debug, Clone)]
pub(super) struct Folder {
    pub path: PathBuf,
    /// The key of the show its name makes.
    pub key: String,
    /// The show most of its episodes are in, when any are known.
    pub current: Option<i64>,
}

/// A show of the library as the database has it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct KnownShow {
    pub id: i64,
    pub key: String,
    pub path: Option<String>,
    /// The ids of its NFO file, as the last scan read it.
    pub nfo_ids: Option<String>,
}

impl KnownShow {
    /// The ids its own folder had when it was last read.
    fn ids(&self) -> ItemIds {
        let nfo = self.nfo_ids.as_deref().and_then(|ids| serde_json::from_str::<NfoIds>(ids).ok());
        let own = self.path.as_deref().map(Path::new).unwrap_or(Path::new(""));
        folder_ids(nfo.map(|ids| ItemIds::of_nfo("show", &ids)), own)
    }
}

/// Where a scan puts the files of a library's show folders.
#[derive(Debug, Default)]
pub(super) struct Placement {
    /// Show folder → the key of the show its files go into.
    pub keys: HashMap<PathBuf, String>,
    /// Folders that leave the show their files are in, with that show.
    pub leaving: Vec<(PathBuf, i64)>,
    /// Shows named after another folder now: the one they were named after
    /// went, or left them.
    pub paths: Vec<(i64, PathBuf)>,
    /// Shows whose key changes, with the new key: all their folders go to a
    /// show of that key, which there isn't yet.
    pub keys_changed: Vec<(i64, String)>,
}

/// The ids of the show `key` fit `ids`: none of them differs.
fn fits(shows: &[(String, ItemIds)], key: &str, ids: &ItemIds) -> bool {
    shows.iter().find(|(k, _)| k == key).is_none_or(|(_, of)| !ids.conflicts_with(of))
}

/// The show `key` is known by `ids` too.
fn add(shows: &mut Vec<(String, ItemIds)>, key: &str, ids: &ItemIds) {
    match shows.iter_mut().find(|(k, _)| k == key) {
        Some((_, of)) => join(of, ids),
        None => shows.push((key.to_owned(), ids.clone())),
    }
}

/// `of` with the ids of `ids` it lacks.
fn join(of: &mut ItemIds, ids: &ItemIds) {
    of.tmdb_id = of.tmdb_id.or(ids.tmdb_id);
    of.imdb_id = of.imdb_id.take().or_else(|| ids.imdb_id.clone());
    of.tvdb_id = of.tvdb_id.or(ids.tvdb_id);
    of.tvmaze_id = of.tvmaze_id.or(ids.tvmaze_id);
}

/// Which show each of `folders` goes into, of the library's `shows`.
/// `unavailable` are the folders the scan couldn't read; `held` keeps every
/// folder in its show (while an import brings what was done with them);
/// `nfo_ids` gives the ids of a folder's `tvshow.nfo` file.
pub(super) fn place(
    folders: Vec<Folder>,
    shows: &[KnownShow],
    unavailable: &[PathBuf],
    held: bool,
    mut nfo_ids: impl FnMut(&Path) -> Option<ItemIds>,
) -> Placement {
    let by_id: HashMap<i64, &KnownShow> = shows.iter().map(|s| (s.id, s)).collect();
    let mut known: HashMap<&str, Vec<&KnownShow>> = HashMap::new();
    for show in shows {
        known.entry(name_key_of(&show.key)).or_default().push(show);
    }
    let mut named: BTreeMap<String, Vec<Folder>> = BTreeMap::new();
    for folder in folders {
        named.entry(folder.key.clone()).or_default().push(folder);
    }

    let mut out = Placement::default();
    // Show key → the folders placed in it, in order.
    let mut placed: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut settle = |out: &mut Placement, folder: Folder, target: String| {
        if let Some(show) = folder.current.and_then(|id| by_id.get(&id))
            && show.key != target
        {
            out.leaving.push((folder.path.clone(), show.id));
        }
        placed.entry(target.clone()).or_default().push(folder.path.clone());
        out.keys.insert(folder.path, target);
    };
    for (key, mut folders) in named {
        let group = known.get(key.as_str()).map(Vec::as_slice).unwrap_or_default();
        let found: HashSet<PathBuf> = folders.iter().map(|f| f.path.clone()).collect();
        // The shows of this name and the ids they're known by: the one their
        // key names, those of the folders placed in them, and while a show's
        // own folder wasn't found, the ids it had. The show the name makes
        // first.
        let mut ids_of: Vec<(String, ItemIds)> = group
            .iter()
            .map(|s| {
                let mut ids = ids_in_key(&s.key);
                if !s.path.as_deref().is_some_and(|p| found.contains(Path::new(p))) {
                    join(&mut ids, &s.ids());
                }
                (s.key.clone(), ids)
            })
            .collect();
        ids_of.sort_by(|(a, _), (b, _)| (*a != key, a).cmp(&(*b != key, b)));
        let current = |f: &Folder| f.current.and_then(|id| by_id.get(&id).copied());
        // Its show, when it is one of this name: one of another name came
        // from names read otherwise before, and the folder goes by its own
        // name now. While held, any.
        let stays_in = |f: &Folder| current(f).filter(|s| held || name_key_of(&s.key) == key);

        // A show found by its TMDB id, and a folder alone with its name
        // whose show knows no ids, have nothing to be told apart from.
        if key.starts_with("show:tmdb:")
            || (folders.len() == 1 && ids_of.iter().all(|(k, ids)| *k == key && *ids == ItemIds::default()))
        {
            for folder in folders {
                let target = stays_in(&folder).map_or_else(|| key.clone(), |s| s.key.clone());
                settle(&mut out, folder, target);
            }
            continue;
        }

        // First the folders that stay in their shows, each show's own
        // folder before its others: their ids fit those of the folders
        // there before them, and no other show of the name is told apart by
        // one of them while theirs isn't.
        folders.sort_by_cached_key(|f| {
            let rank = match stays_in(f) {
                Some(s) if s.path.as_deref().map(Path::new) == Some(f.path.as_path()) => 0,
                Some(_) => 1,
                None => 2,
            };
            (rank, f.path.clone())
        });
        let named_by = |key: &str, ids: &ItemIds| ids_in_key(key).shares_with(ids);
        let mut others = Vec::new();
        for folder in folders {
            let ids = folder_ids(nfo_ids(&folder.path), &folder.path);
            let stays = stays_in(&folder).filter(|s| {
                held || (fits(&ids_of, &s.key, &ids)
                    && (named_by(&s.key, &ids) || !group.iter().any(|t| named_by(&t.key, &ids))))
            });
            match stays {
                Some(show) => {
                    let target = show.key.clone();
                    add(&mut ids_of, &target, &ids);
                    settle(&mut out, folder, target);
                }
                None => others.push((folder, ids)),
            }
        }
        // Then the others: to the show of this name that has their ids, else
        // the show their name makes (the only show of the name, for a folder
        // without ids, when the name makes none), else one of their own.
        for (folder, ids) in others {
            let only = match ids_of.as_slice() {
                [(only, _)] if ids == ItemIds::default() => Some(only.clone()),
                _ => None,
            };
            let target = ids_of
                .iter()
                .find(|(_, of)| ids.shares_with(of) && !ids.conflicts_with(of))
                .map(|(k, _)| k.clone())
                .or(only)
                .or_else(|| fits(&ids_of, &key, &ids).then(|| key.clone()))
                .unwrap_or_else(|| apart_key(&key, &ids, &folder.path, |k| ids_of.iter().any(|(s, _)| s == k)));
            add(&mut ids_of, &target, &ids);
            settle(&mut out, folder, target);
        }
    }

    // A show all of whose folders go to one show that isn't there yet (their
    // names are read otherwise now, say) becomes that show: it takes its
    // key, and keeps its items and what was done with it.
    let mut taken: HashSet<String> = shows.iter().map(|s| s.key.clone()).collect();
    for show in shows {
        if placed.contains_key(&show.key) {
            continue;
        }
        let targets: HashSet<&String> =
            out.leaving.iter().filter(|(_, id)| *id == show.id).filter_map(|(f, _)| out.keys.get(f)).collect();
        if let [target] = targets.into_iter().collect::<Vec<_>>()[..]
            && taken.insert(target.clone())
        {
            out.keys_changed.push((show.id, target.clone()));
        }
    }
    let renamed: HashMap<i64, &str> = out.keys_changed.iter().map(|(id, key)| (*id, key.as_str())).collect();
    out.leaving.retain(|(_, id)| !renamed.contains_key(id));

    // A show is named after its first folder (its path) for as long as that
    // folder is its: when it went, or left the show, the first folder placed
    // in it. A folder that couldn't be read is still the show's.
    for show in shows {
        let key = renamed.get(&show.id).copied().unwrap_or(&show.key);
        let Some(folders) = placed.get(key) else { continue };
        if let Some(own) = show.path.as_deref().map(Path::new)
            && (folders.iter().any(|f| f == own)
                || (!out.keys.contains_key(own) && unavailable.iter().any(|u| own.starts_with(u))))
        {
            continue;
        }
        out.paths.push((show.id, folders[0].clone()));
    }
    out
}

/// The ids of the show in a `tvshow.nfo` file, if any; `None` when the file
/// can't be read.
fn read_nfo_ids(path: &Path) -> Option<Option<ItemIds>> {
    let entries = nfo::read(path)?;
    Some(entries.into_iter().find(is_show_entry).map(|entry| ItemIds::of_nfo("show", &entry.ids)))
}

/// The `tvshow.nfo` files a scan read, by path: their size and
/// modification time, and the ids they gave. The next scan reads only
/// those that changed.
pub(super) type NfoCache = HashMap<PathBuf, (i64, i64, Option<ItemIds>)>;

/// What a scan knows when it places a TV library's show folders.
pub(super) struct Scan<'a> {
    pub library_id: i64,
    /// The files the walk found.
    pub media: Vec<&'a FoundFile>,
    /// The library's known files, by path: the show each is in, and the
    /// length read from it.
    pub known: HashMap<&'a str, (i64, Option<i64>)>,
    pub sidecars: Arc<Sidecars>,
    /// Whether NFO files are read.
    pub read_nfo: bool,
    /// The size and modification time of the NFO files the walk listed.
    pub listed: HashMap<PathBuf, (i64, i64)>,
    /// Folders the walk couldn't read.
    pub unavailable: Vec<PathBuf>,
    /// Folders stay in their shows ([`place`]).
    pub held: bool,
    /// In a scan of part of the library, the shows with show folders
    /// outside it.
    pub outside: Option<HashSet<i64>>,
}

/// Where the files of a TV library's show folders go ([`place`]), and the
/// files that change shows, with their length: they are written again
/// without being read again. `nfos` holds, by library, the `tvshow.nfo`
/// files the last scans read.
///
/// A scan of part of the library places its folders with the shows' folders
/// in the rest as they were, a show's own folder there kept as one that
/// can't be read is. `None` when that may not be where all of the library
/// would place them: a folder would go into or out of a show with folders
/// in the rest, a new folder has the name of such a show, the `tvshow.nfo`
/// of one of its folders changed, or such a show would take another key or
/// folder.
pub(super) async fn plan(
    db: &Db,
    scan: Scan<'_>,
    nfos: &Mutex<HashMap<i64, NfoCache>>,
) -> anyhow::Result<Option<(Placement, HashMap<String, Option<i64>>)>> {
    let shows: Vec<KnownShow> = sqlx::query_as(
        "SELECT i.id, i.key, i.path, n.ids AS nfo_ids FROM items i LEFT JOIN item_nfo n ON n.item_id = i.id
         WHERE i.library_id = ? AND i.kind = 'show'",
    )
    .bind(scan.library_id)
    .fetch_all(&db.read)
    .await?;

    // The show folders, an episode of each (whose name is the folder's),
    // and how many of their episodes each show has; then their bonus
    // material, which goes where they go.
    let mut found: BTreeMap<PathBuf, (&FoundFile, BTreeMap<i64, usize>)> = BTreeMap::new();
    let mut files: Vec<(&FoundFile, PathBuf)> = Vec::new();
    let (episodes, extras): (Vec<&FoundFile>, Vec<&FoundFile>) =
        scan.media.iter().copied().partition(|f| f.extra.is_none());
    for file in episodes {
        let Some(dir) = show_dir(&file.path, &file.root) else { continue };
        let (_, counts) = found.entry(dir.clone()).or_insert_with(|| (file, BTreeMap::new()));
        if let Some((show, _)) = scan.known.get(path_str(&file.path).as_str()) {
            *counts.entry(*show).or_default() += 1;
        }
        files.push((file, dir));
    }
    for file in extras {
        if let Some(dir) = show_dir(&file.path, &file.root).filter(|dir| found.contains_key(dir)) {
            files.push((file, dir));
        }
    }
    let folders: Vec<Folder> = found
        .into_iter()
        .map(|(path, (file, counts))| {
            let name = parse_episode(&file.path, &file.root);
            Folder {
                path,
                key: name_key(&name.show, name.show_year, name.show_tmdb_id),
                // The show with the most of them, the oldest of those.
                current: counts.iter().max_by_key(|&(id, n)| (*n, Reverse(*id))).map(|(id, _)| *id),
            }
        })
        .collect();
    // Of a scan of part of the library, the shows it touches (by their files,
    // or by their folders' names) that have show folders in the rest too.
    let spread: HashSet<i64> = match &scan.outside {
        Some(outside) => {
            let names: HashSet<&str> = folders.iter().map(|f| f.key.as_str()).collect();
            scan.known
                .values()
                .map(|(show, _)| *show)
                .chain(shows.iter().filter(|s| names.contains(name_key_of(&s.key))).map(|s| s.id))
                .filter(|show| outside.contains(show))
                .collect()
        }
        None => HashSet::new(),
    };
    // Their own folders in the rest, which the scan didn't look at.
    let walked: HashSet<&Path> = folders.iter().map(|f| f.path.as_path()).collect();
    let kept: Vec<PathBuf> = shows
        .iter()
        .filter(|s| spread.contains(&s.id))
        .filter_map(|s| s.path.as_deref().map(PathBuf::from))
        .filter(|own| !walked.contains(own.as_path()) && !scan.known.keys().any(|f| Path::new(f).starts_with(own)))
        .collect();
    let currents: Vec<(PathBuf, String, Option<i64>)> =
        folders.iter().map(|f| (f.path.clone(), f.key.clone(), f.current)).collect();

    let (library_id, whole) = (scan.library_id, scan.outside.is_none());
    let cache = nfos.lock().remove(&library_id).unwrap_or_default();
    let (placement, changed, cache) = {
        let shows = shows.clone();
        let Scan { sidecars, read_nfo, listed, mut unavailable, held, .. } = scan;
        unavailable.extend(kept);
        let spread = spread.clone();
        crate::util::blocking(move || {
            let nfo_of = |folder: &Path| sidecars.folder_nfo(folder, &["tvshow"]).filter(|_| read_nfo);
            // The folders of shows with folders in the rest whose
            // `tvshow.nfo` came or changed since it was last read.
            let changed: HashSet<PathBuf> = folders
                .iter()
                .filter(|f| f.current.is_some_and(|show| spread.contains(&show)))
                .filter(|f| {
                    nfo_of(&f.path)
                        .and_then(|nfo| Some((listed.get(&nfo)?, cache.get(&nfo))))
                        .is_some_and(|(now, before)| before.is_none_or(|(size, mtime, _)| *now != (*size, *mtime)))
                })
                .map(|f| f.path.clone())
                .collect();
            let mut read = NfoCache::new();
            let nfo_ids = |folder: &Path| {
                let path = nfo_of(folder)?;
                let listed = listed.get(&path).copied();
                let ids = match (listed, cache.get(&path)) {
                    (Some(now), Some((size, mtime, ids))) if now == (*size, *mtime) => Some(ids.clone()),
                    _ => read_nfo_ids(&path),
                };
                // One that couldn't be read is read again the next time.
                if let (Some((size, mtime)), Some(ids)) = (listed, &ids) {
                    read.insert(path, (size, mtime, ids.clone()));
                }
                ids.flatten()
            };
            let placement = place(folders, &shows, &unavailable, held, nfo_ids);
            // A scan of part of the library listed only its files: those of
            // the rest are as they were.
            if !whole {
                let mut cache = cache;
                cache.extend(read);
                read = cache;
            }
            Ok((placement, changed, read))
        })
        .await?
    };
    nfos.lock().insert(library_id, cache);

    if !spread.is_empty() {
        let key_of: HashMap<i64, &str> = shows.iter().map(|s| (s.id, s.key.as_str())).collect();
        let spread_keys: HashSet<&str> = spread.iter().filter_map(|show| key_of.get(show).copied()).collect();
        let spread_names: HashSet<&str> = spread_keys.iter().map(|key| name_key_of(key)).collect();
        // (A new folder of such a show's name may share ids with one of its
        // folders in the rest.)
        let moves = currents.iter().any(|(folder, name, current)| {
            let target = placement.keys[folder].as_str();
            let from_spread = current.is_some_and(|show| spread.contains(&show));
            let stays = current.and_then(|show| key_of.get(&show).copied()) == Some(target);
            (!stays && (from_spread || spread_keys.contains(target)))
                || (current.is_none() && spread_names.contains(name.as_str()))
                || changed.contains(folder)
        });
        if moves
            || placement.keys_changed.iter().any(|(show, _)| spread.contains(show))
            || placement.paths.iter().any(|(show, _)| spread.contains(show))
        {
            return Ok(None);
        }
    }

    let mut key_of: HashMap<i64, &str> = shows.iter().map(|s| (s.id, s.key.as_str())).collect();
    key_of.extend(placement.keys_changed.iter().map(|(id, key)| (*id, key.as_str())));
    let mut moving = HashMap::new();
    for (file, dir) in files {
        let path = path_str(&file.path);
        let (Some(target), Some((show, duration))) = (placement.keys.get(&dir), scan.known.get(path.as_str())) else {
            continue;
        };
        if key_of.get(show).is_none_or(|key| key != target) {
            moving.insert(path, *duration);
        }
    }
    Ok(Some((placement, moving)))
}

/// Before the scan writes: shows take the keys and are named after the
/// folders [`place`] says, and the seasons of a show in a folder that leaves it lose their
/// folder until [`season_folders`] finds them another.
pub(super) async fn prepare(conn: &mut SqliteConnection, placement: &Placement) -> sqlx::Result<()> {
    for (show, key) in &placement.keys_changed {
        sqlx::query("UPDATE items SET key = ? WHERE id = ?").bind(key).bind(show).execute(&mut *conn).await?;
    }
    for (show, folder) in &placement.paths {
        sqlx::query("UPDATE items SET path = ? WHERE id = ?")
            .bind(path_str(folder))
            .bind(show)
            .execute(&mut *conn)
            .await?;
    }
    for (folder, show) in &placement.leaving {
        let seasons: Vec<(i64, Option<String>)> =
            sqlx::query_as("SELECT id, path FROM items WHERE parent_id = ? AND kind = 'season'")
                .bind(show)
                .fetch_all(&mut *conn)
                .await?;
        for (season, path) in seasons {
            if path.is_some_and(|path| Path::new(&path).starts_with(folder)) {
                sqlx::query("UPDATE items SET path = NULL WHERE id = ?").bind(season).execute(&mut *conn).await?;
            }
        }
    }
    Ok(())
}

/// The library's seasons without a folder (those [`prepare`] let go of,
/// also in a scan that stopped before) get the season folder of one of
/// their episodes' files, under the library's `roots`.
pub(super) async fn season_folders(
    conn: &mut SqliteConnection,
    library_id: i64,
    roots: &[PathBuf],
) -> sqlx::Result<()> {
    let seasons: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT s.id, (SELECT f.path FROM media_files f JOIN items e ON e.id = f.item_id
                       WHERE e.parent_id = s.id ORDER BY f.id LIMIT 1)
         FROM items s WHERE s.library_id = ? AND s.kind = 'season' AND s.path IS NULL",
    )
    .bind(library_id)
    .fetch_all(&mut *conn)
    .await?;
    for (season, file) in seasons {
        let Some(file) = file else { continue };
        let file = Path::new(&file);
        let Some(root) = roots.iter().filter(|root| file.starts_with(root)).max_by_key(|root| root.as_os_str().len())
        else {
            continue;
        };
        let Some(folder) = parse_episode(file, root).season_dir else { continue };
        sqlx::query("UPDATE items SET path = ? WHERE id = ?")
            .bind(path_str(&folder))
            .bind(season)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "show:lantern bay:0";

    fn folder(path: &str, current: Option<i64>) -> Folder {
        Folder { path: PathBuf::from(path), key: KEY.to_owned(), current }
    }

    fn show(id: i64, key: &str, path: &str) -> KnownShow {
        KnownShow { id, key: key.to_owned(), path: Some(path.to_owned()), nfo_ids: None }
    }

    fn tvdb(id: i64) -> ItemIds {
        ItemIds { tvdb_id: Some(id), ..Default::default() }
    }

    /// The ids of each folder's `tvshow.nfo` file.
    fn nfo(files: &[(&str, ItemIds)]) -> impl FnMut(&Path) -> Option<ItemIds> + use<> {
        let files: HashMap<PathBuf, ItemIds> = files.iter().map(|(p, ids)| (PathBuf::from(p), ids.clone())).collect();
        move |folder| files.get(folder).cloned()
    }

    fn place_all(folders: Vec<Folder>, shows: &[KnownShow], ids: impl FnMut(&Path) -> Option<ItemIds>) -> Placement {
        place(folders, shows, &[], false, ids)
    }

    fn key<'a>(placed: &'a Placement, folder: &str) -> &'a str {
        &placed.keys[Path::new(folder)]
    }

    #[test]
    fn folders_of_one_name_are_one_show_unless_their_ids_differ() {
        let folders = vec![
            folder("/b/Lantern Bay", None),
            folder("/a/Lantern.Bay", None),
            folder("/a/Lantern Bay", None),
            folder("/c/Lantern Bay", None),
        ];
        let ids = nfo(&[("/a/Lantern Bay", tvdb(1)), ("/b/Lantern Bay", tvdb(2)), ("/c/Lantern Bay", tvdb(2))]);
        let placed = place_all(folders, &[], ids);
        assert_eq!(key(&placed, "/a/Lantern Bay"), KEY);
        // Without ids, a folder stays with the show of its name.
        assert_eq!(key(&placed, "/a/Lantern.Bay"), KEY);
        // Another show's ids: a show of its own, with the folders that share them.
        assert_eq!(key(&placed, "/b/Lantern Bay"), "show:lantern bay:0:tvdb:2");
        assert_eq!(key(&placed, "/c/Lantern Bay"), "show:lantern bay:0:tvdb:2");
        assert!(placed.leaving.is_empty() && placed.paths.is_empty());
    }

    #[test]
    fn ids_from_different_sources_name_no_other_show() {
        let folders = vec![folder("/a/Lantern Bay", None), folder("/b/Lantern Bay", None)];
        let tmdb = ItemIds { tmdb_id: Some(10), ..Default::default() };
        let placed = place_all(folders, &[], nfo(&[("/a/Lantern Bay", tvdb(1)), ("/b/Lantern Bay", tmdb)]));
        assert_eq!(key(&placed, "/b/Lantern Bay"), KEY);
    }

    #[test]
    fn the_ids_in_a_folders_name_count_unless_its_nfo_file_has_some() {
        let mut named = folder("/b/Lantern Bay {tvdb-2}", None);
        named.key = KEY.to_owned();
        let folders = vec![folder("/a/Lantern Bay", None), named, folder("/c/Lantern Bay {tvdb-3}", None)];
        let placed = place_all(folders, &[], nfo(&[("/a/Lantern Bay", tvdb(1)), ("/c/Lantern Bay {tvdb-3}", tvdb(1))]));
        assert_eq!(key(&placed, "/b/Lantern Bay {tvdb-2}"), "show:lantern bay:0:tvdb:2");
        assert_eq!(key(&placed, "/c/Lantern Bay {tvdb-3}"), KEY);
    }

    #[test]
    fn a_show_merged_by_name_keeps_its_own_folder() {
        for (own, leaves, apart) in [
            ("/a/Lantern Bay", "/b/Lantern Bay", "show:lantern bay:0:tvdb:2"),
            ("/b/Lantern Bay", "/a/Lantern Bay", "show:lantern bay:0:tvdb:1"),
        ] {
            let shows = [show(7, KEY, own)];
            let folders = vec![folder("/a/Lantern Bay", Some(7)), folder("/b/Lantern Bay", Some(7))];
            let placed = place_all(folders, &shows, nfo(&[("/a/Lantern Bay", tvdb(1)), ("/b/Lantern Bay", tvdb(2))]));
            assert_eq!(key(&placed, own), KEY);
            assert_eq!(key(&placed, leaves), apart);
            assert_eq!(placed.leaving, [(PathBuf::from(leaves), 7)]);
            assert!(placed.paths.is_empty(), "its own folder stays");
        }
    }

    #[test]
    fn folders_stay_in_their_shows_until_their_ids_say_otherwise() {
        let apart = "show:lantern bay:0:tvdb:2";
        let shows = [show(7, KEY, "/a/Lantern Bay"), show(8, apart, "/b/Lantern Bay")];
        let folders = vec![
            folder("/a/Lantern Bay", Some(7)),
            // Its NFO file went: it stays.
            folder("/b/Lantern Bay", Some(8)),
            // New ones: without ids, or with those of the show of the name,
            // that show; with others, one of their own.
            folder("/c/Lantern Bay", None),
            folder("/d/Lantern Bay", None),
            folder("/e/Lantern Bay", None),
            // With the id the other show's key names, which its folder no
            // longer gives: that show.
            folder("/f/Lantern Bay", None),
        ];
        let ids = nfo(&[
            ("/a/Lantern Bay", tvdb(1)),
            ("/d/Lantern Bay", tvdb(1)),
            ("/e/Lantern Bay", tvdb(3)),
            ("/f/Lantern Bay", tvdb(2)),
        ]);
        let placed = place_all(folders, &shows, ids);
        assert_eq!(key(&placed, "/a/Lantern Bay"), KEY);
        assert_eq!(key(&placed, "/b/Lantern Bay"), apart);
        assert_eq!(key(&placed, "/c/Lantern Bay"), KEY);
        assert_eq!(key(&placed, "/d/Lantern Bay"), KEY);
        assert_eq!(key(&placed, "/e/Lantern Bay"), "show:lantern bay:0:tvdb:3");
        assert_eq!(key(&placed, "/f/Lantern Bay"), apart);
        assert!(placed.leaving.is_empty());
    }

    #[test]
    fn a_folder_whose_ids_changed_doesnt_push_out_the_folder_a_show_is_named_after() {
        // The show of the name, and another told apart by TheTVDB id 2,
        // whose only folder's NFO file now names a third show (and which
        // comes first by name): each keeps its own folder.
        let apart = "show:lantern bay:0:tvdb:2";
        let shows = [show(7, KEY, "/b/Lantern Bay x"), show(8, apart, "/a/Lantern Bay")];
        let folders = vec![folder("/b/Lantern Bay x", Some(7)), folder("/a/Lantern Bay", Some(8))];
        let placed = place_all(folders, &shows, nfo(&[("/b/Lantern Bay x", tvdb(1)), ("/a/Lantern Bay", tvdb(3))]));
        assert_eq!(key(&placed, "/b/Lantern Bay x"), KEY);
        // Its key names another show now: it takes the third show's, with
        // its items.
        let third = "show:lantern bay:0:tvdb:3";
        assert_eq!(key(&placed, "/a/Lantern Bay"), third);
        assert_eq!(placed.keys_changed, [(8, third.to_owned())]);
        assert!(placed.leaving.is_empty());
    }

    #[test]
    fn a_folder_goes_to_the_show_whose_key_names_its_ids() {
        // Told apart, then its NFO file corrected to the other show's id:
        // it joins that show instead of staying a second one.
        let apart = "show:lantern bay:0:tvdb:2";
        let shows = [show(7, KEY, "/a/Lantern Bay"), show(8, apart, "/b/Lantern Bay")];
        let folders = vec![folder("/a/Lantern Bay", Some(7)), folder("/b/Lantern Bay", Some(8))];
        let placed = place_all(folders, &shows, nfo(&[("/a/Lantern Bay", tvdb(1)), ("/b/Lantern Bay", tvdb(1))]));
        assert_eq!(key(&placed, "/b/Lantern Bay"), KEY);
        assert_eq!(placed.leaving, [(PathBuf::from("/b/Lantern Bay"), 8)]);

        // The show of the name went, the other stayed: a folder without ids
        // joins it, the only show of the name.
        let only = [show(8, apart, "/b/Lantern Bay")];
        let folders = vec![folder("/b/Lantern Bay", Some(8)), folder("/c/Lantern Bay", None)];
        let placed = place_all(folders, &only, nfo(&[("/b/Lantern Bay", tvdb(2))]));
        assert_eq!(key(&placed, "/c/Lantern Bay"), apart);

        // A show of the name whose folder gets the id the other's key names
        // goes into that one.
        let shows = [show(7, KEY, "/c/Lantern Bay"), show(8, apart, "/b/Lantern Bay")];
        let folders = vec![folder("/b/Lantern Bay", Some(8)), folder("/c/Lantern Bay", Some(7))];
        let placed = place_all(folders, &shows, nfo(&[("/c/Lantern Bay", tvdb(2))]));
        assert_eq!(key(&placed, "/c/Lantern Bay"), apart);
        assert_eq!(placed.leaving, [(PathBuf::from("/c/Lantern Bay"), 7)]);
        assert!(placed.keys_changed.is_empty(), "{:?}", placed.keys_changed);
    }

    #[test]
    fn a_show_whose_own_folder_cant_be_read_is_known_by_its_ids() {
        let mut own = show(7, KEY, "/a/Lantern Bay");
        own.nfo_ids = Some(r#"{"tvdb":1}"#.to_owned());
        // Alone with its name, a new folder with other ids is another show.
        let placed = place(vec![folder("/b/Lantern Bay", None)], &[own.clone()], &[PathBuf::from("/a")], false, {
            nfo(&[("/b/Lantern Bay", tvdb(2))])
        });
        assert_eq!(key(&placed, "/b/Lantern Bay"), "show:lantern bay:0:tvdb:2");
        // The show stays named after its folder; one with its ids joins it.
        let placed = place(vec![folder("/c/Lantern Bay", None)], &[own.clone()], &[PathBuf::from("/a")], false, {
            nfo(&[("/c/Lantern Bay", tvdb(1))])
        });
        assert_eq!(key(&placed, "/c/Lantern Bay"), KEY);
        assert!(placed.paths.is_empty());
        // A folder gone for good: the show is named after the one that came.
        let placed = place_all(vec![folder("/c/Lantern Bay", None)], &[own], nfo(&[("/c/Lantern Bay", tvdb(1))]));
        assert_eq!(placed.paths, [(7, PathBuf::from("/c/Lantern Bay"))]);
    }

    #[test]
    fn a_show_whose_own_folder_leaves_is_named_after_one_that_stays() {
        let shows = [show(7, KEY, "/a/Lantern Bay")];
        let folders = vec![folder("/a/Lantern Bay", Some(7)), folder("/b/Lantern Bay", Some(7))];
        // /a/ is out of the show's name now (`Lantern Bay` elsewhere).
        let mut renamed = folders;
        renamed[0].key = "show:harbor lights:0".to_owned();
        let placed = place_all(renamed, &shows, |_: &Path| None);
        assert_eq!(key(&placed, "/a/Lantern Bay"), "show:harbor lights:0");
        assert_eq!(key(&placed, "/b/Lantern Bay"), KEY);
        assert_eq!(placed.leaving, [(PathBuf::from("/a/Lantern Bay"), 7)]);
        assert_eq!(placed.paths, [(7, PathBuf::from("/b/Lantern Bay"))]);
    }

    #[test]
    fn a_folder_alone_with_its_name_reads_no_nfo_file() {
        let shows = [show(9, "show:other show:0", "/a/Other Show")];
        let mut other = folder("/a/Other Show", Some(9));
        other.key = "show:other show:0".to_owned();
        // A show found by its TMDB id, in two folders.
        let mut by_id = folder("/b/Lantern Bay {tmdb-5}", None);
        by_id.key = "show:tmdb:5".to_owned();
        let mut also = by_id.clone();
        also.path = PathBuf::from("/c/Lantern Bay {tmdb-5}");
        let folders = vec![folder("/a/Lantern Bay", None), other, by_id, also];
        let placed = place_all(folders, &shows, |folder: &Path| panic!("read {folder:?}"));
        assert_eq!(key(&placed, "/a/Lantern Bay"), KEY);
        assert_eq!(key(&placed, "/a/Other Show"), "show:other show:0");
        assert_eq!(key(&placed, "/b/Lantern Bay {tmdb-5}"), "show:tmdb:5");
        assert_eq!(key(&placed, "/c/Lantern Bay {tmdb-5}"), "show:tmdb:5");
    }

    #[test]
    fn a_folder_in_a_show_of_another_name_goes_by_its_own() {
        // Its files went to a show of another name, as names were read
        // before: it goes to the show of its name, unless that's held.
        let shows = [show(9, "show:tv:0", "/media/tv")];
        for held in [false, true] {
            let folders = vec![folder("/media/tv/Lantern Bay", Some(9))];
            let placed = place(folders, &shows, &[], held, |_: &Path| None);
            assert_eq!(key(&placed, "/media/tv/Lantern Bay"), if held { "show:tv:0" } else { KEY });
            // Its only folder: the show takes the key, keeping its items.
            let renamed: &[(i64, String)] = if held { &[] } else { &[(9, KEY.to_owned())] };
            assert_eq!(placed.keys_changed, renamed);
            assert!(placed.leaving.is_empty());
        }
        // Two shows whose folders now make one name: the first takes the
        // key, the other's folder goes into it.
        let shows = [show(8, "show:bay:0", "/a/Lantern Bay"), show(9, "show:tv:0", "/b/Lantern Bay")];
        let folders = vec![folder("/a/Lantern Bay", Some(8)), folder("/b/Lantern Bay", Some(9))];
        let placed = place_all(folders, &shows, |_: &Path| None);
        assert_eq!(placed.keys_changed, [(8, KEY.to_owned())]);
        assert_eq!(placed.leaving, [(PathBuf::from("/b/Lantern Bay"), 9)]);
    }

    #[test]
    fn held_folders_stay_in_their_shows() {
        let shows = [show(7, KEY, "/a/Lantern Bay")];
        let folders = vec![folder("/a/Lantern Bay", Some(7)), folder("/b/Lantern Bay", Some(7))];
        let ids = nfo(&[("/a/Lantern Bay", tvdb(1)), ("/b/Lantern Bay", tvdb(2))]);
        let placed = place(folders, &shows, &[], true, ids);
        assert_eq!(key(&placed, "/b/Lantern Bay"), KEY);
        assert!(placed.leaving.is_empty());
    }

    #[test]
    fn keys_say_which_ids_tell_a_show_apart() {
        assert_eq!(name_key("Lantern Bay", None, None), KEY);
        assert_eq!(name_key("Lantern.Bay!", Some(2004), None), "show:lantern bay:2004");
        assert_eq!(name_key("Lantern Bay", Some(2004), Some(5)), "show:tmdb:5");
        assert_eq!(name_key_of("show:lantern bay:0:tvdb:2"), KEY);
        assert_eq!(name_key_of(KEY), KEY);
        assert_eq!(name_key_of("show:tmdb:5"), "show:tmdb:5");
        assert_eq!(ids_in_key("show:lantern bay:0:tvdb:2"), tvdb(2));
        let imdb = ItemIds { imdb_id: Some("tt7000001".into()), ..Default::default() };
        assert_eq!(ids_in_key("show:lantern bay:0:imdb:tt7000001"), imdb);
        assert_eq!(ids_in_key(KEY), ItemIds::default());
        assert_eq!(ids_in_key("show:lantern bay:0:folder:C:\\tv\\Lantern Bay"), ItemIds::default());

        let ids = ItemIds { tmdb_id: Some(5), tvdb_id: Some(2), ..Default::default() };
        let folder = Path::new("/b/Lantern Bay");
        assert_eq!(apart_key(KEY, &ids, folder, |_| false), "show:lantern bay:0:tmdb:5");
        let taken = |k: &str| k.ends_with(":tmdb:5");
        assert_eq!(apart_key(KEY, &ids, folder, taken), "show:lantern bay:0:tvdb:2");
        assert_eq!(apart_key(KEY, &ids, folder, |_| true), "show:lantern bay:0:folder:/b/Lantern Bay");
    }
}
