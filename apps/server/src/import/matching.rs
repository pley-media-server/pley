//! Plex's items as Pley's: Pley's scan makes the items, and the import finds
//! which of them each Plex item is. First found:
//!
//! 1. its file: Plex's path, through the folders' mapping, is a file of
//!    Pley's of the same size;
//! 2. its ids: TMDB, IMDb or TheTVDB, and an episode by its show's and its
//!    season and episode numbers (not music: Plex's music ids are mostly
//!    its own);
//! 3. nothing, and the report says so. Titles are never guessed from.
//!
//! Shows, seasons, artists and albums are also found through an episode or
//! track found inside them.

use std::collections::HashMap;

use serde::Serialize;

use crate::db::Db;

use super::plex::Item;

/// Where a Plex library's files are for Pley: Plex's folder, Pley's folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub plex: String,
    pub pley: String,
}

/// Plex's path `file` as Pley sees it, through the first folder it's in,
/// `None` when it's in none.
pub fn map_path(folders: &[Folder], file: &str) -> Option<String> {
    folders.iter().find_map(|f| {
        let rest = within(file, &f.plex)?;
        let sep = if f.pley.contains('\\') && !f.pley.contains('/') { '\\' } else { '/' };
        let rest: String = rest.chars().map(|c| if c == '/' || c == '\\' { sep } else { c }).collect();
        let base = f.pley.trim_end_matches(['/', '\\']);
        Some(if rest.is_empty() { base.to_owned() } else { format!("{base}{sep}{rest}") })
    })
}

/// What's after `folder` in `path`, without the separator; `None` when
/// `path` isn't in it ("/movies2" isn't in "/movies").
fn within<'a>(path: &'a str, folder: &str) -> Option<&'a str> {
    let folder = folder.trim_end_matches(['/', '\\']);
    let rest = path.strip_prefix(folder)?;
    if rest.is_empty() {
        return Some(rest);
    }
    rest.strip_prefix(['/', '\\'])
}

/// The folders that make Plex's `plex` path Pley's `pley`: the parts they
/// end with in common taken off both. `None` when even their file names
/// differ.
pub fn folder_between(plex: &str, pley: &str) -> Option<Folder> {
    let a: Vec<&str> = plex.split(['/', '\\']).collect();
    let b: Vec<&str> = pley.split(['/', '\\']).collect();
    let same = a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count();
    if same == 0 {
        return None;
    }
    let cut = |path: &str, parts: &[&str]| {
        // The length of the parts kept, with their separators.
        let keep: usize = parts[..parts.len() - same].iter().map(|p| p.len() + 1).sum();
        path[..keep.saturating_sub(1)].to_owned()
    };
    Some(Folder { plex: cut(plex, &a), pley: cut(pley, &b) })
}

/// The folders that make Plex's `plex` path Pley's `pley`, Plex's being
/// one of its library's `locations` when it can: `/data/movies` →
/// `/mnt/movies` rather than `/data` → `/mnt`, which says the same of
/// these files but reads less plainly.
pub fn folder_for(locations: &[String], plex: &str, pley: &str) -> Option<Folder> {
    let found = folder_between(plex, pley)?;
    // The same path on both: the folder it's in, for both, which maps any
    // path in it to itself.
    if found.plex.is_empty() && found.pley.is_empty() {
        let folder = locations
            .iter()
            .find(|l| within(plex, l).is_some())
            .map(|l| l.trim_end_matches(['/', '\\']).to_owned())
            .or_else(|| plex.rsplit_once(['/', '\\']).map(|(dir, _)| dir.to_owned()))?;
        return Some(Folder { plex: folder.clone(), pley: folder });
    }
    for location in locations {
        let Some(rest) = within(plex, location) else { continue };
        if within(location, &found.plex).is_none() {
            continue;
        }
        let rest: String = rest.chars().map(|c| if c == '\\' { '/' } else { c }).collect();
        let pley_slashes: String = pley.chars().map(|c| if c == '\\' { '/' } else { c }).collect();
        if let Some(base) = pley_slashes.strip_suffix(&rest).and_then(|b| b.strip_suffix('/')) {
            return Some(Folder {
                plex: location.trim_end_matches(['/', '\\']).to_owned(),
                pley: pley[..base.len()].to_owned(),
            });
        }
    }
    Some(found)
}

/// What Pley has to match against, read once for the libraries imported.
#[derive(Default)]
pub struct Index {
    /// A file's path: its library, item and size.
    files: HashMap<String, (i64, i64, i64)>,
    /// An item's parent, grandparent and number, for every item indexed.
    items: HashMap<i64, Parents>,
    /// (library, "movie", "tmdb", "348") and the like: the item, so a Plex
    /// library's titles are found in the Pley library it's paired with.
    ids: HashMap<(i64, String, &'static str, String), i64>,
    /// A season's or an episode's place: (parent, number) to the item.
    children: HashMap<(i64, i64), i64>,
}

#[derive(Debug, Clone, Copy)]
struct Parents {
    parent: Option<i64>,
    grandparent: Option<i64>,
    /// A season's, an episode's or a track's number.
    idx: Option<i64>,
    /// A movie's or a show's TMDB id, as Pley matched it.
    tmdb: Option<i64>,
}

impl Index {
    /// Pley's items in `libraries`.
    pub async fn load(db: &Db, libraries: &[i64]) -> sqlx::Result<Index> {
        let mut index = Index::default();
        let list = serde_json::to_string(libraries).unwrap_or_else(|_| "[]".into());
        let files: Vec<(String, i64, i64, i64)> = sqlx::query_as(
            "SELECT path, library_id, item_id, size FROM media_files WHERE library_id IN (SELECT value FROM json_each(?))",
        )
        .bind(&list)
        .fetch_all(&db.read)
        .await?;
        for (path, library, item, size) in files {
            index.files.insert(path, (library, item, size));
        }
        type Row = (i64, i64, String, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<String>, Option<i64>);
        let items: Vec<Row> = sqlx::query_as(
            "SELECT id, library_id, kind, parent_id, grandparent_id, idx, tmdb_id, imdb_id, tvdb_id FROM items
             WHERE library_id IN (SELECT value FROM json_each(?)) AND extra_type IS NULL",
        )
        .bind(&list)
        .fetch_all(&db.read)
        .await?;
        for (id, library, kind, parent, grandparent, idx, tmdb, imdb, tvdb) in items {
            index.items.insert(id, Parents { parent, grandparent, idx, tmdb });
            if let (Some(parent), Some(idx), "season" | "episode") = (parent, idx, kind.as_str()) {
                index.children.insert((parent, idx), id);
            }
            if matches!(kind.as_str(), "movie" | "show") {
                for (source, id_there) in
                    [("tmdb", tmdb.map(|n| n.to_string())), ("imdb", imdb), ("tvdb", tvdb.map(|n| n.to_string()))]
                {
                    if let Some(found) = id_there.filter(|s| !s.is_empty()) {
                        index.ids.entry((library, kind.clone(), source, found)).or_insert(id);
                    }
                }
            }
        }
        Ok(index)
    }

    /// The item of a file Plex lists, by its path and size, in `library`.
    pub fn by_file(&self, library: i64, folders: &[Folder], file: &str, size: Option<i64>) -> Option<i64> {
        let path = map_path(folders, file)?;
        let (in_library, item, known) = self.files.get(&path)?;
        (*in_library == library && size.is_none_or(|s| s == *known)).then_some(*item)
    }

    /// The movie or show `item` is by its ids, in `library`.
    pub fn by_ids(&self, library: i64, kind: &str, item: &Item) -> Option<i64> {
        ["tmdb", "imdb", "tvdb"].into_iter().find_map(|source| {
            let id = item.guid(source)?;
            self.ids.get(&(library, kind.to_owned(), source, id.to_owned())).copied()
        })
    }

    /// Season `number` of show `show`, or episode `number` of season
    /// `season`.
    pub fn child(&self, parent: i64, number: i64) -> Option<i64> {
        self.children.get(&(parent, number)).copied()
    }

    /// A movie's or show's TMDB id, as Pley matched it.
    pub fn tmdb(&self, item: i64) -> Option<i64> {
        self.items.get(&item).and_then(|p| p.tmdb)
    }

    /// An item's parent and grandparent (an episode's season and show).
    pub fn parents(&self, item: i64) -> (Option<i64>, Option<i64>) {
        self.items.get(&item).map(|p| (p.parent, p.grandparent)).unwrap_or_default()
    }
}

/// Plex's items found in Pley, by Plex's `ratingKey`.
#[derive(Default, Clone)]
pub struct Matches {
    found: HashMap<String, i64>,
}

impl Matches {
    pub fn get(&self, rating_key: &str) -> Option<i64> {
        self.found.get(rating_key).copied()
    }

    pub fn len(&self) -> usize {
        self.found.len()
    }

    pub fn is_empty(&self) -> bool {
        self.found.is_empty()
    }

    pub fn insert(&mut self, rating_key: &str, item: i64) {
        self.found.insert(rating_key.to_owned(), item);
    }

    /// What's been found since `before`, which these matches grew from.
    pub fn since(&self, before: &Matches) -> Matches {
        let found = self.found.iter().filter(|(key, _)| !before.found.contains_key(*key));
        Matches { found: found.map(|(key, id)| (key.clone(), *id)).collect() }
    }

    /// Finds `item`, and its parents through it: an episode's season and
    /// show, a track's album and artist.
    pub fn find(&mut self, index: &Index, library: i64, folders: &[Folder], item: &Item) -> Option<i64> {
        // Of the items its files are, the one numbered closest below or at
        // its own: Plex lists a file named for a range (S05E10-E21) under
        // every episode in it, and a two-episode file is Pley's item for
        // its first.
        let files: Vec<i64> =
            item.files().filter_map(|(file, size)| index.by_file(library, folders, file, size)).collect();
        let number = |id: &i64| index.items.get(id).and_then(|p| p.idx);
        let nearest = item
            .index
            .and_then(|own| files.iter().filter(|id| number(id).is_some_and(|n| n <= own)).max_by_key(|id| number(id)));
        let by_file = nearest.or(files.first()).copied();
        let found = by_file.or_else(|| match item.kind.as_str() {
            "movie" | "show" => index.by_ids(library, &item.kind, item),
            "season" => {
                let show = self.get(item.parent_rating_key.as_deref()?)?;
                index.child(show, item.index?)
            }
            "episode" => {
                let show = self.get(item.grandparent_rating_key.as_deref()?)?;
                let season = index.child(show, item.parent_index?)?;
                index.child(season, item.index?)
            }
            _ => None,
        });
        let found = found?;
        self.insert(&item.rating_key, found);
        let (parent, grandparent) = index.parents(found);
        for (key, id) in [(&item.parent_rating_key, parent), (&item.grandparent_rating_key, grandparent)] {
            if let (Some(key), Some(id)) = (key, id) {
                self.found.entry(key.clone()).or_insert(id);
            }
        }
        Some(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_map_through_the_first_folder_they_are_in() {
        let folders = [
            Folder { plex: "/data/movies".into(), pley: "/mnt/media/Movies/".into() },
            Folder { plex: "D:\\TV".into(), pley: "/mnt/tv".into() },
        ];
        assert_eq!(
            map_path(&folders, "/data/movies/A (1979)/A.mkv").as_deref(),
            Some("/mnt/media/Movies/A (1979)/A.mkv")
        );
        assert_eq!(map_path(&folders, "D:\\TV\\Show\\S01\\E1.mkv").as_deref(), Some("/mnt/tv/Show/S01/E1.mkv"));
        assert_eq!(map_path(&folders, "/data/movies2/B.mkv"), None);
    }

    #[test]
    fn folders_are_read_off_a_file_both_see() {
        assert_eq!(
            folder_between("/data/movies/A (1979)/A.mkv", "/volume1/Media/Movies/A (1979)/A.mkv"),
            Some(Folder { plex: "/data/movies".into(), pley: "/volume1/Media/Movies".into() })
        );
        assert_eq!(
            folder_between("/m/A.mkv", "/m/A.mkv"),
            Some(Folder { plex: "".into(), pley: "".into() }),
            "the same paths need no change"
        );
        assert_eq!(folder_between("/m/A.mkv", "/m/B.mkv"), None);
        let same = folder_for(&["C:\\Movies".into()], "C:\\Movies\\A.mkv", "C:\\Movies\\A.mkv").unwrap();
        assert_eq!(same, Folder { plex: "C:\\Movies".into(), pley: "C:\\Movies".into() });
        assert_eq!(map_path(&[same], "C:\\Movies\\B\\B.mkv").as_deref(), Some("C:\\Movies\\B\\B.mkv"));
        assert_eq!(
            folder_for(&["/data/movies".into()], "/data/movies/A.mkv", "/mnt/movies/A.mkv"),
            Some(Folder { plex: "/data/movies".into(), pley: "/mnt/movies".into() }),
            "Plex's library folder, rather than the shortest"
        );
    }

    fn item(json: &str) -> Item {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn an_episode_with_a_range_files_file_is_its_own() {
        // Plex lists "S05E10-E21" under every episode from 10 to 21, before
        // each one's own file.
        let mut index = Index::default();
        index.files.insert("/p/S05E10-E21.avi".into(), (9, 20, 10));
        index.files.insert("/p/S05E11-E12.avi".into(), (9, 21, 11));
        index.items.insert(20, Parents { parent: Some(5), grandparent: Some(1), idx: Some(10), tmdb: None });
        index.items.insert(21, Parents { parent: Some(5), grandparent: Some(1), idx: Some(11), tmdb: None });
        let folders = [Folder { plex: "/x".into(), pley: "/p".into() }];
        let mut m = Matches::default();
        let episode = |key: &str, number: i64| {
            item(&format!(
                r#"{{"ratingKey":"{key}","type":"episode","index":{number},"parentIndex":5,"Media":[
                    {{"Part":[{{"file":"/x/S05E10-E21.avi","size":10}}]}},{{"Part":[{{"file":"/x/S05E11-E12.avi","size":11}}]}}]}}"#
            ))
        };
        assert_eq!(m.find(&index, 9, &folders, &episode("e11", 11)), Some(21));
        // E12 is the second half of E11's file, which Pley numbers 11.
        assert_eq!(m.find(&index, 9, &folders, &episode("e12", 12)), Some(21));
        assert_eq!(m.find(&index, 9, &folders, &episode("e10", 10)), Some(20));
    }

    #[test]
    fn items_are_found_by_file_then_ids_and_parents_through_children() {
        let mut index = Index::default();
        index.files.insert("/p/A.mkv".into(), (9, 1, 100));
        index.files.insert("/p/S/E1.mkv".into(), (9, 12, 50));
        index.ids.insert((9, "movie".into(), "tmdb", "7".into()), 2);
        index.ids.insert((8, "movie".into(), "tmdb", "70".into()), 3);
        index.items.insert(12, Parents { parent: Some(11), grandparent: Some(10), idx: Some(1), tmdb: None });
        index.children.insert((10, 1), 11);
        index.children.insert((11, 2), 13);
        let folders = [Folder { plex: "/x".into(), pley: "/p".into() }];
        let mut m = Matches::default();

        assert_eq!(
            m.find(
                &index,
                9,
                &folders,
                &item(r#"{"ratingKey":"a","type":"movie","Media":[{"Part":[{"file":"/x/A.mkv","size":100}]}]}"#)
            ),
            Some(1)
        );
        // A file of another size isn't the same file.
        assert_eq!(
            m.find(
                &index,
                9,
                &folders,
                &item(r#"{"ratingKey":"b","type":"movie","Media":[{"Part":[{"file":"/x/A.mkv","size":99}]}]}"#)
            ),
            None
        );
        assert_eq!(
            m.find(&index, 9, &folders, &item(r#"{"ratingKey":"c","type":"movie","Guid":[{"id":"tmdb://7"}]}"#)),
            Some(2)
        );
        let e1 = r#"{"ratingKey":"e1","type":"episode","parentRatingKey":"s1","grandparentRatingKey":"sh","index":1,"parentIndex":1,
                     "Media":[{"Part":[{"file":"/x/S/E1.mkv","size":50}]}]}"#;
        assert_eq!(m.find(&index, 9, &folders, &item(e1)), Some(12));
        assert_eq!((m.get("s1"), m.get("sh")), (Some(11), Some(10)));
        // Its sibling, whose file Pley doesn't have, by its numbers.
        let e2 = r#"{"ratingKey":"e2","type":"episode","parentRatingKey":"s1","grandparentRatingKey":"sh","index":2,"parentIndex":1}"#;
        assert_eq!(m.find(&index, 9, &folders, &item(e2)), Some(13));
        // Only in the Pley library it's paired with.
        assert_eq!(
            m.find(&index, 9, &folders, &item(r#"{"ratingKey":"d","type":"movie","Guid":[{"id":"tmdb://70"}]}"#)),
            None
        );
    }
}
