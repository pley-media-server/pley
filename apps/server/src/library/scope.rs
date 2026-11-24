//! The part of a library a scan of what changed looks at.
//!
//! A change anywhere in a folder directly in a library folder (a movie's, a
//! show's, an artist's, an author's) rescans that folder whole: everything
//! a title's files are told apart by, and its artwork, subtitles and NFO
//! files, is in it. A change to a file directly in the library folder
//! rescans those files, with the subtitle folders beside them. A folder
//! that went is looked at too, so that its files go; one moved elsewhere in
//! the library is both a folder that went and one that came, rescanned
//! together, so its titles keep what was done with them.
//!
//! What lies outside the part looked at stays as it is: its files aren't
//! taken for deleted, and its items' artwork, subtitles and NFO files
//! aren't worked out from a part of the library's sidecar files. Only what
//! went from the library folder itself is noticed without its path, since
//! the library folder is listed anyway: changes went unnoticed, and the
//! whole library is scanned instead ([`Scope::missed`]).

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use super::LibraryKind;
use super::walk::{Part, is_subtitle_folder, left_out_folder};

/// More paths than this changing at once (a folder of titles copied in)
/// have the whole library scanned: some systems drop notifications when
/// that many come, and Windows doesn't say when it did.
pub const MOST_PATHS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    roots: Vec<PathBuf>,
    /// What of each library folder is looked at.
    parts: Vec<Part>,
}

/// The names in a library folder: as they are, and in lowercase.
struct Listing {
    names: HashSet<String>,
    lowercase: HashMap<String, Vec<String>>,
}

impl Listing {
    fn of(root: &Path) -> Option<Self> {
        let mut listing = Self { names: HashSet::new(), lowercase: HashMap::new() };
        for entry in std::fs::read_dir(root).ok()? {
            let Ok(entry) = entry else { return None };
            let Ok(name) = entry.file_name().into_string() else { continue };
            listing.lowercase.entry(name.to_lowercase()).or_default().push(name.clone());
            listing.names.insert(name);
        }
        Some(listing)
    }

    /// The entry `name` finds: itself, or the one entry that is `name` in
    /// another case, as Windows and macOS find it.
    fn find(&self, name: &str) -> Option<&str> {
        if let Some(name) = self.names.get(name) {
            return Some(name);
        }
        match self.lowercase.get(&name.to_lowercase())?.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }
}

impl Scope {
    /// What a scan of the library folders `roots` looks at for the changed
    /// `paths`; `None` when that is the whole library: a library folder
    /// itself changed, too many paths did, or one names an entry by another
    /// name than the library folder lists (Windows' short names). Paths
    /// outside the library, and in what a walk leaves out, change nothing.
    /// Asks the disk which paths are folders.
    pub fn of(roots: &[PathBuf], kind: LibraryKind, paths: &[PathBuf]) -> Option<Scope> {
        if paths.len() > MOST_PATHS {
            return None;
        }
        let mut parts = vec![Part::default(); roots.len()];
        let mut listings: HashMap<usize, Option<Listing>> = HashMap::new();
        for path in paths {
            // Library folders can be nested: the deepest one holding it.
            let Some((index, root)) = roots
                .iter()
                .enumerate()
                .filter(|(_, root)| path.starts_with(root))
                .max_by_key(|(_, root)| root.components().count())
            else {
                continue;
            };
            let Ok(rel) = path.strip_prefix(root) else { continue };
            let mut names = Vec::new();
            for component in rel.components() {
                match component {
                    // A walk leaves out what isn't named in UTF-8.
                    Component::Normal(name) => match name.to_str() {
                        Some(name) => names.push(name),
                        None => break,
                    },
                    // `..`: out of the library folder.
                    _ => break,
                }
            }
            if names.len() != rel.components().count() {
                continue;
            }
            // The library folder itself: all of it.
            let &name = names.first()?;
            // Nothing changes in what a walk never looks at, whatever the
            // depth: hidden files and folders, system folders, samples.
            if names.iter().any(|name| left_out_folder(name, kind)) {
                continue;
            }
            let part = &mut parts[index];
            let Ok(meta) = std::fs::metadata(root.join(name)) else {
                // What went: a folder, whose files go, or a file of the
                // library folder.
                part.folders.insert(name.to_owned());
                part.top = true;
                continue;
            };
            let listing = listings.entry(index).or_insert_with(|| Listing::of(root));
            let name = listing.as_ref()?.find(name)?.to_owned();
            if !meta.is_dir() {
                part.top = true;
            } else if !left_out_folder(&name, kind) {
                // Subtitles beside the library folder's files are theirs.
                part.top |= is_subtitle_folder(&name);
                part.folders.insert(name);
            }
        }
        Some(Scope { roots: roots.to_vec(), parts })
    }

    /// What of each library folder is walked, in the order of the library's
    /// folders.
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    /// Whether nothing at all is looked at.
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(Part::is_empty)
    }

    /// How many folders, and library folders' files, are looked at.
    pub fn len(&self) -> usize {
        self.parts.iter().map(|part| part.folders.len() + usize::from(part.top)).sum()
    }

    /// Whether the file at `path` is in what is looked at.
    pub fn holds(&self, path: &Path) -> bool {
        self.find(path, |part, first, alone| {
            if alone { part.top } else { part.folders.contains(first) || (part.top && is_subtitle_folder(first)) }
        })
    }

    /// Whether the folder `dir` is looked at, with all it holds: its files
    /// and sidecar files are all found.
    pub fn covers(&self, dir: &Path) -> bool {
        self.roots.iter().zip(&self.parts).any(|(root, part)| dir == root && part.top)
            || self.find(dir, |part, first, _| part.takes(first, true))
    }

    /// Whether changes went unnoticed: an entry holding one of the `known`
    /// files went from a library folder (`listed` has the names in those
    /// walked) and no changed path named it. Its files may be anywhere in
    /// the library under another name (a title's folder renamed while
    /// nothing watched, or by a tool that named only the new one), where
    /// only a scan of the whole library finds them: taken for deleted,
    /// their titles would go with what was done with them and come back
    /// as new.
    pub fn missed<'a>(
        &self,
        listed: &HashMap<PathBuf, HashSet<String>>,
        mut known: impl Iterator<Item = &'a str>,
    ) -> bool {
        known.any(|path| !self.holds(Path::new(path)) && went(listed, Path::new(path)))
    }

    /// Whether `decide` takes `path` in one of the library folders holding
    /// it: it is given the folder's part, the first name of `path` in it,
    /// and whether that is all of it.
    fn find(&self, path: &Path, decide: impl Fn(&Part, &str, bool) -> bool) -> bool {
        self.roots.iter().zip(&self.parts).any(|(root, part)| {
            let Ok(rel) = path.strip_prefix(root) else { return false };
            let mut components = rel.components();
            match components.next() {
                Some(Component::Normal(first)) => {
                    first.to_str().is_some_and(|first| decide(part, first, components.next().is_none()))
                }
                _ => false,
            }
        })
    }
}

/// Whether the entry of a library folder that `path` is in went from it, by
/// `listed`, every name in the library folders listed.
fn went(listed: &HashMap<PathBuf, HashSet<String>>, path: &Path) -> bool {
    listed.iter().any(|(root, names)| {
        let Ok(rel) = path.strip_prefix(root) else { return false };
        match rel.components().next() {
            Some(Component::Normal(first)) => first.to_str().is_some_and(|first| !names.contains(first)),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_went_from_a_library_folder_is_told_by_its_listing() {
        let listed =
            HashMap::from([(PathBuf::from("/m"), HashSet::from(["Kept (2001)".to_owned(), "Loose.mkv".to_owned()]))]);
        assert!(went(&listed, Path::new("/m/Renamed (2002)/Renamed (2002).mkv")));
        assert!(went(&listed, Path::new("/m/Gone.mkv")));
        assert!(!went(&listed, Path::new("/m/Kept (2001)/Kept (2001).mkv")));
        assert!(!went(&listed, Path::new("/m/Loose.mkv")));
        assert!(!went(&listed, Path::new("/elsewhere/Gone (2003)/Gone.mkv")), "not listed");
    }

    fn file(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn a_change_looks_at_the_folder_of_the_title_it_is_in() {
        let dir = crate::testing::tempdir();
        let (movies, more) = (dir.path().join("Movies"), dir.path().join("More"));
        file(&movies, "Film One (2001)/Film One (2001).mkv");
        file(&movies, "Film Two (2002)/Subs/2_English.srt");
        file(&movies, "Loose (2003).mkv");
        file(&more, "Film Three (2003)/Film Three (2003).mkv");
        let roots = [movies.clone(), more.clone()];
        let changed = |paths: &[&Path]| {
            Scope::of(&roots, LibraryKind::Movies, &paths.iter().map(|p| p.to_path_buf()).collect::<Vec<_>>())
        };

        let scope = changed(&[
            &movies.join("Film One (2001)/Film One (2001).en.srt"),
            &movies.join("Film Two (2002)/Subs/2_English.srt"),
            &dir.path().join("Elsewhere/Other (2004).mkv"),
        ])
        .unwrap();
        assert_eq!(scope.parts()[0].folders.iter().collect::<Vec<_>>(), ["Film One (2001)", "Film Two (2002)"]);
        assert!(!scope.parts()[0].top && scope.parts()[1].is_empty());
        assert_eq!(scope.len(), 2);
        assert!(scope.holds(&movies.join("Film One (2001)/Extras/Making Of.mkv")));
        assert!(scope.covers(&movies.join("Film Two (2002)/Subs")));
        assert!(!scope.holds(&movies.join("Loose (2003).mkv")));
        assert!(!scope.holds(&movies.join("Film Three (2003)/Film Three (2003).mkv")));
        assert!(!scope.covers(&movies), "the library folder's own files aren't looked at");
        assert!(!scope.holds(&more.join("Film One (2001)/Film One (2001).mkv")));

        // A file of the library folder: its files, with its subtitles.
        let scope = changed(&[&movies.join("Loose (2003).mkv")]).unwrap();
        assert!(scope.parts()[0].top);
        assert!(scope.holds(&movies.join("Loose (2003).mkv")) && scope.holds(&movies.join("Other (2005).mkv")));
        assert!(scope.covers(&movies) && scope.covers(&movies.join("subs/Loose (2003)")));
        assert!(!scope.holds(&movies.join("Film One (2001)/Film One (2001).mkv")));

        // A folder that went: its files go, and so does a file of that name.
        let scope = changed(&[&movies.join("Gone (1999)")]).unwrap();
        assert!(scope.holds(&movies.join("Gone (1999)/Gone (1999).mkv")) && scope.parts()[0].top);

        // Subtitles beside the library folder's files: those files.
        file(&movies, "Subs/Loose (2003)/2_English.srt");
        let scope = changed(&[&movies.join("Subs/Loose (2003)/2_English.srt")]).unwrap();
        assert!(scope.parts()[0].top && scope.holds(&movies.join("Loose (2003).mkv")));

        // What a walk leaves out changes nothing, however deep.
        let left_out = [
            movies.join(".hidden/Film.mkv"),
            movies.join("#recycle/Film.mkv"),
            movies.join("Sample/x.mkv"),
            movies.join("Film One (2001)/@eaDir/Film One (2001).mkv/SYNOFILE_THUMB_M.jpg"),
            movies.join("Film One (2001)/.DS_Store"),
        ];
        for path in &left_out {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        let scope = changed(&left_out.iter().map(PathBuf::as_path).collect::<Vec<_>>()).unwrap();
        assert!(scope.is_empty(), "{scope:?}");
        // Music keeps sample folders.
        let music = Scope::of(&roots, LibraryKind::Music, &[movies.join("Sample/x.mp3")]).unwrap();
        assert!(music.holds(&movies.join("Sample/x.mp3")));

        // Out of the library folder by its name.
        assert!(changed(&[&movies.join("../More/Film Three (2003)")]).unwrap().is_empty());
        // The library folder itself, or too many paths: the whole library.
        assert_eq!(changed(&[&movies]), None);
        assert_eq!(changed(&[&more, &movies.join("Loose (2003).mkv")]), None);
        let many: Vec<PathBuf> = (0..=MOST_PATHS).map(|n| movies.join(format!("Film {n} (2001)"))).collect();
        assert_eq!(Scope::of(&roots, LibraryKind::Movies, &many), None);
    }

    /// An entry with known files in it that went from a library folder
    /// without a changed path naming it: changes went unnoticed.
    #[test]
    fn what_went_unnamed_says_changes_were_missed() {
        let movies = PathBuf::from("/m");
        let known = [
            "/m/Film One (2001)/Film One (2001).mkv",
            "/m/Film Two (2002)/Film Two (2002).mkv",
            "/m/Loose (2003).mkv",
            "/more/Other (2004)/Other (2004).mkv",
        ];
        let scope = |folder: &str, top: bool| Scope {
            roots: vec![movies.clone(), PathBuf::from("/more")],
            parts: vec![Part { folders: [folder.to_owned()].into(), top }, Part::default()],
        };
        let missed = |scope: Scope, names: &[&str]| {
            let listed = HashMap::from([(movies.clone(), names.iter().map(|name| name.to_string()).collect())]);
            scope.missed(&listed, known.iter().copied())
        };
        // Nothing went, whatever came; a library folder not walked isn't
        // listed, and nothing is known to have gone from it.
        let all = ["Film One (2001)", "Film Two (2002)", "Loose (2003).mkv", "New (2005)"];
        assert!(!missed(scope("Film Two (2002)", false), &all));
        // What went was named: a folder, a file of the library folder.
        assert!(!missed(scope("Film One (2001)", false), &all[1..]));
        assert!(!missed(scope("Film Two (2002)", true), &all[..2]));
        // It wasn't: a folder renamed, a file of the library folder gone.
        assert!(missed(scope("Film Two (2002)", false), &["Film One (2001) [Remux]", "Film Two (2002)", all[2]]));
        assert!(missed(scope("Film One (2001) [Remux]", false), &["Film One (2001) [Remux]", all[1], all[2]]));
        assert!(missed(scope("Film Two (2002)", false), &all[..2]));
    }

    /// Windows and macOS find a folder by its name in any case: it is the
    /// folder the library folder lists that is walked.
    #[test]
    fn a_folder_named_in_another_case_is_the_one_listed() {
        let dir = crate::testing::tempdir();
        let movies = dir.path().join("Movies");
        file(&movies, "Film One (2001)/Film One (2001).mkv");
        let roots = [movies.clone()];
        let scope = Scope::of(&roots, LibraryKind::Movies, &[movies.join("FILM ONE (2001)/x.srt")]).unwrap();
        if std::fs::metadata(movies.join("FILM ONE (2001)")).is_ok() {
            assert_eq!(scope.parts()[0].folders.iter().collect::<Vec<_>>(), ["Film One (2001)"]);
            assert!(!scope.parts()[0].top);
        } else {
            // Another folder, which isn't there.
            assert_eq!(scope.parts()[0].folders.iter().collect::<Vec<_>>(), ["FILM ONE (2001)"]);
        }
        assert_eq!(
            scope.holds(&movies.join("Film One (2001)/Film One (2001).mkv")),
            scope.parts()[0].folders.contains("Film One (2001)")
        );
    }

    #[test]
    fn nested_library_folders_take_the_deepest() {
        let dir = crate::testing::tempdir();
        let (outer, inner) = (dir.path().join("Movies"), dir.path().join("Movies/Kids"));
        file(&inner, "Small Film (2010)/Small Film (2010).mkv");
        let roots = [outer.clone(), inner.clone()];
        let scope = Scope::of(&roots, LibraryKind::Movies, &[inner.join("Small Film (2010)/x.srt")]).unwrap();
        assert!(scope.parts()[0].is_empty());
        assert!(scope.holds(&inner.join("Small Film (2010)/Small Film (2010).mkv")));
        assert!(!scope.holds(&outer.join("Kids/Other (2011)/Other (2011).mkv")));
    }
}
