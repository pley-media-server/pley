//! Sidecar files next to videos: local artwork (`poster.jpg`,
//! `Movie (2020)-fanart.jpg`, `season01-poster.jpg`, …), external subtitles
//! (`Movie.srt`, `Movie.en.srt`, `Movie.eng.forced.srt`,
//! `Movie.English.sdh.srt`, `Subs/2_English.srt`, `Subs/<video>/3_French.srt`)
//! and NFO files (`Movie (2020).nfo`, `movie.nfo`, `tvshow.nfo`,
//! `season.nfo`).
//!
//! A scan collects the sidecar files of the whole library and indexes them
//! once ([`Sidecars`]): finding the artwork, the subtitles or the NFO file of
//! a video then takes a few look-ups, however large the library.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use pley_transcoder_api::{MediaStream, StreamKind, is_text_subtitle_codec};
use serde::{Deserialize, Serialize};

use super::walk::{IMAGE_EXTENSIONS, NFO_EXTENSIONS, SUBTITLE_EXTENSIONS};

/// External subtitle streams get indexes from this value up, below
/// [`crate::subtitles::ADDED_INDEX_BASE`].
pub const EXTERNAL_INDEX_BASE: i64 = 1000;
/// The most subtitle files of a video that are listed.
const MAX_SUBTITLE_FILES: usize = (crate::subtitles::ADDED_INDEX_BASE - EXTERNAL_INDEX_BASE) as usize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternalSub {
    pub index: i64,
    pub path: String,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub forced: bool,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub sdh: bool,
}

impl ExternalSub {
    pub fn to_stream(&self) -> MediaStream {
        MediaStream {
            index: self.index,
            kind: StreamKind::Subtitle,
            codec: self.codec.clone(),
            language: self.language.clone(),
            title: self.title.clone(),
            default: self.default,
            forced: self.forced,
            text_based: Some(is_text_subtitle_codec(&self.codec)),
            external: true,
            ..Default::default()
        }
    }
}

fn codec_for(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "srt" => "subrip",
        "ass" => "ass",
        "ssa" => "ssa",
        "vtt" => "webvtt",
        "sub" => "microdvd",
        "sup" => "hdmv_pgs_subtitle",
        "smi" => "sami",
        _ => return None,
    })
}

/// A subtitle file and the tags in its name (`en.forced`).
type Tagged = (PathBuf, String);

/// The sidecar files of a library, indexed by folder and video.
#[derive(Debug, Default)]
pub struct Sidecars {
    /// Pictures by folder: lowercase name without extension → file name
    /// (the first one found when pictures differ only by extension).
    images: HashMap<PathBuf, HashMap<String, String>>,
    /// NFO files by folder: lowercase name without extension → file name
    /// (the first one found when names differ only by case).
    nfos: HashMap<PathBuf, HashMap<String, String>>,
    /// Subtitle files by folder, sorted by lowercase name without extension,
    /// so that the ones named after a video (`<stem>.srt`,
    /// `<stem>.<tags>.srt`) are next to each other.
    subtitles: HashMap<PathBuf, Vec<(String, PathBuf)>>,
    /// Subtitles in `Subs` or `Subtitles` folders, which belong to every video
    /// in the folder that holds them.
    shared: HashMap<PathBuf, Vec<Tagged>>,
    /// Subtitles in `Subs/<video stem>/` folders, by the video's folder and
    /// lowercase stem.
    per_video: HashMap<PathBuf, HashMap<String, Vec<Tagged>>>,
}

fn is_subs_folder(dir: &Path) -> bool {
    dir.file_name().is_some_and(|name| matches!(name.to_string_lossy().to_lowercase().as_str(), "subs" | "subtitles"))
}

impl Sidecars {
    /// Indexes sidecar file names by folder, as the walk found them.
    pub fn new(files: HashMap<PathBuf, Vec<String>>) -> Self {
        let mut index = Self::default();
        for (dir, names) in files {
            // Subs/2_English.srt belongs to the videos next to Subs, and
            // Subs/<stem>/2_English.srt to the video named <stem> there.
            let shared_by = dir.parent().filter(|_| is_subs_folder(&dir)).map(Path::to_path_buf);
            let per_video = dir
                .parent()
                .filter(|parent| is_subs_folder(parent))
                .and_then(Path::parent)
                .zip(dir.file_name())
                .map(|(owner, stem)| (owner.to_path_buf(), stem.to_string_lossy().to_lowercase()));
            let mut subtitles = Vec::new();
            for name in names {
                let lower = name.to_ascii_lowercase();
                if let Some((stem, ext)) = lower.rsplit_once('.')
                    && IMAGE_EXTENSIONS.contains(&ext)
                {
                    let images = index.images.entry(dir.clone()).or_default();
                    images.entry(stem.to_owned()).or_insert_with(|| name.clone());
                }
                let lower = name.to_lowercase();
                let Some((base, ext)) = lower.rsplit_once('.') else { continue };
                if NFO_EXTENSIONS.contains(&ext) {
                    let nfos = index.nfos.entry(dir.clone()).or_default();
                    nfos.entry(base.to_owned()).or_insert_with(|| name.clone());
                    continue;
                }
                if !SUBTITLE_EXTENSIONS.contains(&ext) {
                    continue;
                }
                let path = dir.join(&name);
                if shared_by.is_some() || per_video.is_some() {
                    // "2_English" → "english"
                    let tags =
                        base.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_' || c == ' ').replace('_', ".");
                    if let Some(owner) = &shared_by {
                        index.shared.entry(owner.clone()).or_default().push((path.clone(), tags.clone()));
                    }
                    if let Some((owner, stem)) = &per_video {
                        let by_stem = index.per_video.entry(owner.clone()).or_default();
                        by_stem.entry(stem.clone()).or_default().push((path.clone(), tags));
                    }
                }
                subtitles.push((base.to_owned(), path));
            }
            if !subtitles.is_empty() {
                subtitles.sort_unstable();
                index.subtitles.insert(dir, subtitles);
            }
        }
        index
    }

    /// The picture in `dir` named after the first of `bases` that has one
    /// (`poster` finds `Poster.JPG`).
    pub fn image(&self, dir: &Path, bases: &[String]) -> Option<PathBuf> {
        let images = self.images.get(dir)?;
        bases.iter().find_map(|base| images.get(&base.to_ascii_lowercase())).map(|name| dir.join(name))
    }

    /// The NFO file named after `video`, next to it (`Movie (2020).mkv` finds
    /// `Movie (2020).NFO`).
    pub fn nfo(&self, video: &Path) -> Option<PathBuf> {
        let dir = video.parent()?;
        let stem = video.file_stem()?.to_string_lossy().to_lowercase();
        self.nfos.get(dir)?.get(&stem).map(|name| dir.join(name))
    }

    /// The NFO file in `dir` named after the first of `names` that has one
    /// (`tvshow` finds `TVShow.nfo`).
    pub fn folder_nfo(&self, dir: &Path, names: &[&str]) -> Option<PathBuf> {
        let nfos = self.nfos.get(dir)?;
        names.iter().find_map(|name| nfos.get(&name.to_lowercase())).map(|file| dir.join(file))
    }

    /// Subtitle files belonging to `video`.
    pub fn subtitles(&self, video: &Path) -> Vec<ExternalSub> {
        let Some(dir) = video.parent() else { return Vec::new() };
        let stem = video.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut found: Vec<Tagged> = Vec::new();
        if let Some(files) = self.subtitles.get(dir) {
            // `<stem>` and `<stem>.<tags>`: within the run of names that start
            // with the stem.
            let start = files.partition_point(|(base, _)| base.as_str() < stem.as_str());
            for (base, path) in files[start..].iter().take_while(|(base, _)| base.starts_with(stem.as_str())) {
                match &base[stem.len()..] {
                    "" => found.push((path.clone(), String::new())),
                    rest => {
                        if let Some(tags) = rest.strip_prefix('.') {
                            found.push((path.clone(), tags.to_owned()));
                        }
                    }
                }
            }
        }
        found.extend(self.shared.get(dir).into_iter().flatten().cloned());
        found.extend(self.per_video.get(dir).and_then(|by_stem| by_stem.get(&stem)).into_iter().flatten().cloned());
        describe(found)
    }
}

/// Streams for subtitle files, numbered in path order.
fn describe(mut found: Vec<Tagged>) -> Vec<ExternalSub> {
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.dedup_by(|a, b| a.0 == b.0);
    found
        .into_iter()
        .take(MAX_SUBTITLE_FILES)
        .enumerate()
        .filter_map(|(i, (path, tags))| {
            let ext = path.extension()?.to_string_lossy().to_lowercase();
            let codec = codec_for(&ext)?;
            let mut language = None;
            let mut forced = false;
            let mut default = false;
            let mut sdh = false;
            let mut extra = Vec::new();
            for token in tags.split(['.', ' ', '-']).filter(|t| !t.is_empty()) {
                match token {
                    "forced" | "foreign" => forced = true,
                    "default" => default = true,
                    "sdh" | "cc" | "hi" => sdh = true,
                    t => match normalize_language(t) {
                        Some(lang) if language.is_none() => language = Some(lang),
                        _ => extra.push(t.to_owned()),
                    },
                }
            }
            let title = (!extra.is_empty()).then(|| extra.join(" "));
            Some(ExternalSub {
                index: EXTERNAL_INDEX_BASE + i as i64,
                path: path.to_string_lossy().into_owned(),
                codec: codec.to_owned(),
                language,
                title,
                forced,
                default,
                sdh,
            })
        })
        .collect()
}

/// ISO 639-1/639-2 codes and English names → ISO 639-2/B.
pub fn normalize_language(token: &str) -> Option<String> {
    const TABLE: &[(&str, &[&str])] = &[
        ("eng", &["en", "eng", "english"]),
        ("fre", &["fr", "fre", "fra", "french", "francais", "français"]),
        ("ger", &["de", "ger", "deu", "german", "deutsch"]),
        ("spa", &["es", "spa", "spanish", "espanol", "español", "castellano"]),
        ("ita", &["it", "ita", "italian", "italiano"]),
        ("por", &["pt", "por", "portuguese", "pt-br", "ptbr", "brazilian"]),
        ("dut", &["nl", "dut", "nld", "dutch"]),
        ("swe", &["sv", "swe", "swedish"]),
        ("nor", &["no", "nor", "nob", "norwegian"]),
        ("dan", &["da", "dan", "danish"]),
        ("fin", &["fi", "fin", "finnish"]),
        ("pol", &["pl", "pol", "polish"]),
        ("cze", &["cs", "cze", "ces", "czech"]),
        ("hun", &["hu", "hun", "hungarian"]),
        ("rum", &["ro", "rum", "ron", "romanian"]),
        ("gre", &["el", "gre", "ell", "greek"]),
        ("tur", &["tr", "tur", "turkish"]),
        ("rus", &["ru", "rus", "russian"]),
        ("ukr", &["uk", "ukr", "ukrainian"]),
        ("ara", &["ar", "ara", "arabic"]),
        ("heb", &["he", "heb", "hebrew"]),
        ("hin", &["hi-in", "hin", "hindi"]),
        ("jpn", &["ja", "jpn", "japanese"]),
        ("kor", &["ko", "kor", "korean"]),
        ("chi", &["zh", "chi", "zho", "chinese", "chs", "cht"]),
        ("tha", &["th", "tha", "thai"]),
        ("vie", &["vi", "vie", "vietnamese"]),
        ("ind", &["id", "ind", "indonesian"]),
        ("may", &["ms", "may", "msa", "malay"]),
        ("bul", &["bg", "bul", "bulgarian"]),
        ("hrv", &["hr", "hrv", "croatian"]),
        ("srp", &["sr", "srp", "serbian"]),
        ("slv", &["sl", "slv", "slovenian"]),
        ("slo", &["sk", "slo", "slk", "slovak"]),
        ("est", &["et", "est", "estonian"]),
        ("lav", &["lv", "lav", "latvian"]),
        ("lit", &["lt", "lit", "lithuanian"]),
        ("ice", &["is", "ice", "isl", "icelandic"]),
        ("per", &["fa", "per", "fas", "persian", "farsi"]),
        ("cat", &["ca", "cat", "catalan"]),
    ];
    let t = token.trim().to_lowercase();
    TABLE.iter().find(|(_, names)| names.contains(&t.as_str())).map(|(code, _)| (*code).to_owned())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn sidecars(folders: &[(&Path, &[&str])]) -> Sidecars {
        Sidecars::new(
            folders
                .iter()
                .map(|(dir, names)| (dir.to_path_buf(), names.iter().map(|n| (*n).to_owned()).collect()))
                .collect(),
        )
    }

    #[test]
    fn finds_sidecar_subtitles() {
        let dir = PathBuf::from("/m/Movie (2020)");
        let index = sidecars(&[
            (
                &dir,
                &[
                    "Movie (2020).srt",
                    "Movie (2020).en.forced.srt",
                    "Movie (2020).French.sdh.ass",
                    "Other Movie.srt",
                    "poster.jpg",
                ],
            ),
            (&dir.join("Subs"), &["2_English.srt"]),
        ]);
        let subs = index.subtitles(&dir.join("Movie (2020).mkv"));
        assert_eq!(subs.len(), 4);
        let forced = subs.iter().find(|s| s.forced).unwrap();
        assert_eq!(forced.language.as_deref(), Some("eng"));
        let french = subs.iter().find(|s| s.codec == "ass").unwrap();
        assert_eq!(french.language.as_deref(), Some("fre"));
        assert!(french.sdh);
        let from_subs_dir = subs.iter().find(|s| s.path.contains("Subs")).unwrap();
        assert_eq!(from_subs_dir.language.as_deref(), Some("eng"));
        assert!(subs.iter().all(|s| s.index >= EXTERNAL_INDEX_BASE));
        let stream = subs[0].to_stream();
        assert!(stream.external);
        assert_eq!(stream.text_based, Some(true));
    }

    #[test]
    fn subtitle_folders_and_videos_whose_names_start_alike() {
        let season = PathBuf::from("/tv/Show/Season 01");
        let index = sidecars(&[
            (&season, &["Show - S01E01.en.srt", "Show - S01E01 Part 2.srt", "Show - S01E010.fr.srt", "Show.srt"]),
            (&season.join("Subs").join("Show - S01E01"), &["3_French.srt", "notes.txt"]),
            (&season.join("SUBTITLES"), &["English.forced.srt"]),
        ]);
        let names = |video: &str| -> Vec<(String, Option<String>, bool)> {
            index
                .subtitles(&season.join(video))
                .into_iter()
                .map(|s| {
                    let name = Path::new(&s.path).strip_prefix(&season).unwrap().to_string_lossy().replace('\\', "/");
                    (name, s.language, s.forced)
                })
                .collect()
        };
        assert_eq!(
            names("Show - S01E01.mkv"),
            [
                ("SUBTITLES/English.forced.srt".to_owned(), Some("eng".to_owned()), true),
                ("Show - S01E01.en.srt".to_owned(), Some("eng".to_owned()), false),
                ("Subs/Show - S01E01/3_French.srt".to_owned(), Some("fre".to_owned()), false),
            ]
        );
        assert_eq!(names("Show - S01E010.mkv").len(), 2, "its own subtitle and the shared one");
        assert_eq!(names("Show - S01E01 Part 2.mkv").len(), 2);
        assert_eq!(names("Show.mkv").len(), 2);
        assert!(index.subtitles(Path::new("/elsewhere/Show - S01E01.mkv")).is_empty());
    }

    #[test]
    fn finds_images_case_insensitively() {
        let dir = Path::new("/m/Movie (2020)");
        let index = sidecars(&[(dir, &["Poster.JPG", "fanart.png", "poster.png", "movie.srt"])]);
        let bases = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        assert_eq!(index.image(dir, &bases(&["poster"])), Some(dir.join("Poster.JPG")), "the first one found");
        assert_eq!(index.image(dir, &bases(&["folder", "FANART"])), Some(dir.join("fanart.png")));
        assert_eq!(index.image(dir, &bases(&["movie"])), None, "subtitles are not pictures");
        assert_eq!(index.image(Path::new("/m/other"), &bases(&["poster"])), None);
    }

    #[test]
    fn finds_nfo_files_case_insensitively() {
        let movie = Path::new("/m/Movie (2020)");
        let season = Path::new("/tv/Show/Season 01");
        let show = Path::new("/tv/Show");
        let index = sidecars(&[
            (movie, &["Movie (2020).NFO", "movie.nfo", "Movie (2020).srt", "poster.jpg", "notes.txt"]),
            (show, &["TVShow.nfo", "Show.nfo"]),
            (season, &["Season.NFO", "Show - S01E01.nfo", "show - s01e02.Nfo", "Show - S01E03.en.nfo"]),
            (Path::new("/m/Ünïcode (2003)"), &["ÜNÏCODE (2003).nfo"]),
        ]);
        assert_eq!(index.nfo(&movie.join("Movie (2020).mkv")), Some(movie.join("Movie (2020).NFO")));
        assert_eq!(index.nfo(&movie.join("movie.mkv")), Some(movie.join("movie.nfo")));
        assert_eq!(index.nfo(&movie.join("Movie (2020) - Extended.mkv")), None, "named after another video");
        assert_eq!(index.nfo(Path::new("/elsewhere/Movie (2020).mkv")), None);
        assert_eq!(index.nfo(&season.join("Show - S01E02.mkv")), Some(season.join("show - s01e02.Nfo")));
        assert_eq!(index.nfo(&season.join("Show - S01E03.mkv")), None, "only `<stem>.nfo`, no tags");
        assert_eq!(
            index.nfo(Path::new("/m/Ünïcode (2003)/Ünïcode (2003).mkv")),
            Some(PathBuf::from("/m/Ünïcode (2003)/ÜNÏCODE (2003).nfo"))
        );
        assert_eq!(index.folder_nfo(movie, &["movie"]), Some(movie.join("movie.nfo")));
        assert_eq!(index.folder_nfo(show, &["tvshow"]), Some(show.join("TVShow.nfo")));
        assert_eq!(index.folder_nfo(season, &["SEASON"]), Some(season.join("Season.NFO")));
        assert_eq!(index.folder_nfo(movie, &["tvshow", "movie"]), Some(movie.join("movie.nfo")), "the first found");
        assert_eq!(index.folder_nfo(show, &["movie", "season"]), None);
        assert_eq!(index.folder_nfo(movie, &["poster", "notes"]), None, "a picture or a note is no NFO file");
        assert_eq!(index.folder_nfo(Path::new("/m/other"), &["movie"]), None);
        assert_eq!(index.image(movie, &["movie".to_owned()]), None, "NFO files are not pictures");
        assert_eq!(index.subtitles(&movie.join("Movie (2020).mkv")).len(), 1, "nor subtitles");
    }

    #[test]
    fn the_first_of_nfo_files_differing_by_case_is_kept() {
        let dir = Path::new("/m/Movie (2020)");
        let index = sidecars(&[(dir, &["Movie.NFO", "movie.nfo"])]);
        assert_eq!(index.folder_nfo(dir, &["movie"]), Some(dir.join("Movie.NFO")));
        assert_eq!(index.nfo(&dir.join("MOVIE.mkv")), Some(dir.join("Movie.NFO")));
    }

    /// What the scanner did before the index: every folder of the library
    /// looked at for every video.
    fn reference(video: &Path, sidecars: &HashMap<PathBuf, Vec<String>>) -> Vec<ExternalSub> {
        let Some(dir) = video.parent() else { return Vec::new() };
        let stem = video.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut found: Vec<Tagged> = Vec::new();
        if let Some(files) = sidecars.get(dir) {
            for name in files {
                let lower = name.to_lowercase();
                let Some((base, ext)) = lower.rsplit_once('.') else { continue };
                if !SUBTITLE_EXTENSIONS.contains(&ext) {
                    continue;
                }
                if base == stem {
                    found.push((dir.join(name), String::new()));
                } else if let Some(tags) = base.strip_prefix(&stem).and_then(|r| r.strip_prefix('.')) {
                    found.push((dir.join(name), tags.to_owned()));
                }
            }
        }
        for sub_dir_name in ["subs", "subtitles"] {
            for (sub_dir, files) in sidecars {
                let Some(parent) = sub_dir.parent() else { continue };
                let name = sub_dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                let matches_dir = (parent == dir && name == sub_dir_name)
                    || (parent.parent() == Some(dir)
                        && parent.file_name().is_some_and(|p| p.to_string_lossy().to_lowercase() == sub_dir_name)
                        && name == stem);
                if !matches_dir {
                    continue;
                }
                for file in files {
                    let lower = file.to_lowercase();
                    let Some((base, ext)) = lower.rsplit_once('.') else { continue };
                    if SUBTITLE_EXTENSIONS.contains(&ext) {
                        let tags = base
                            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '_' || c == ' ')
                            .replace('_', ".");
                        found.push((sub_dir.join(file), tags));
                    }
                }
            }
        }
        describe(found)
    }

    #[test]
    fn the_index_finds_what_looking_everywhere_found() {
        let root = PathBuf::from("/lib");
        let flat = root.join("Flat");
        let mut files: HashMap<PathBuf, Vec<String>> = HashMap::new();
        let mut videos = Vec::new();
        // Release names with dots, names that start alike, upper case.
        for stem in ["Movie", "Movie.2019", "Movie.2019.1080p", "MOVIE 2", "Movie2", "Ünïcode Film", "a.b", "a"] {
            videos.push(flat.join(format!("{stem}.mkv")));
            for suffix in ["", ".en", ".EN.Forced", ".fr.sdh", ".x.y.z", "-commentary"] {
                files.entry(flat.clone()).or_default().push(format!("{stem}{suffix}.srt"));
            }
        }
        files.entry(flat.clone()).or_default().extend(["poster.jpg".to_owned(), "notes.txt".to_owned()]);
        // Subs folders of several shapes and cases, with an NFO file in one:
        // indexed as an NFO file, never taken for subtitles.
        files.insert(flat.join("Subs"), vec!["2_English.srt".into(), "3_French.SDH.ass".into(), "x.nfo".into()]);
        files.insert(flat.join("subtitles").join("movie.2019"), vec!["English.srt".into()]);
        files.insert(flat.join("Subs").join("Movie"), vec!["1_Dutch.srt".into()]);
        files.insert(root.join("Other").join("Subs"), vec!["German.srt".into()]);
        videos.push(root.join("Other").join("Film.mkv"));
        videos.push(root.join("Film.mkv"));

        let index = Sidecars::new(files.clone());
        for video in &videos {
            assert_eq!(index.subtitles(video), reference(video, &files), "{}", video.display());
        }
        assert_eq!(index.nfo(&flat.join("Subs").join("x.mkv")), Some(flat.join("Subs").join("x.nfo")));
        assert!(videos.iter().map(|v| index.subtitles(v).len()).sum::<usize>() > 40);
    }

    #[test]
    fn matching_takes_linear_time() {
        // 20k movie folders, each with a subtitle and a poster: looking at
        // every folder for every video took about 8 minutes (release build).
        let mut files: HashMap<PathBuf, Vec<String>> = HashMap::new();
        let mut videos = Vec::new();
        for d in 0..20_000 {
            let dir = PathBuf::from(format!("/m/Title {d} (2000)"));
            files.insert(dir.clone(), vec![format!("Title {d} (2000).en.srt"), "poster.jpg".to_owned()]);
            videos.push(dir.join(format!("Title {d} (2000).mkv")));
        }
        let started = Instant::now();
        let index = Sidecars::new(files);
        let found: usize = videos.iter().map(|v| index.subtitles(v).len()).sum();
        assert_eq!(found, 20_000);
        assert!(started.elapsed() < Duration::from_secs(20), "{:?}", started.elapsed());
    }

    #[test]
    fn languages() {
        assert_eq!(normalize_language("EN").as_deref(), Some("eng"));
        assert_eq!(normalize_language("deu").as_deref(), Some("ger"));
        assert_eq!(normalize_language("Japanese").as_deref(), Some("jpn"));
        assert_eq!(normalize_language("xx"), None);
    }
}
