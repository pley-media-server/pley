//! Turning file and folder names into titles, years, seasons and episodes.
//!
//! Supported layouts (the usual Plex/Jellyfin conventions):
//! * `Movies/The Matrix (1999)/The Matrix (1999) - 1080p.mkv`, `The.Matrix.1999.1080p.BluRay.x264.mkv`
//! * `TV/Show (2005)/Season 01/Show - S01E02 - Title.mkv`, `Show.S01E02E03.mkv`, `Show/1x02.mkv`,
//!   `Show/Season 2/05 - Title.mkv`, `Show/Season 1/Season 1 Episode 4.mkv`,
//!   `Show/Season 1/104 - Title.mkv` (season and episode run together, in a
//!   season folder of that number), `Daily Show/2024-05-01.mkv`,
//!   `[Group] Anime - 12 [1080p].mkv`, and an episode without a number
//!   (`Show/Season 1/Title.mkv`), which keeps its own place (see
//!   [`EpisodeName::episode`])
//! * `Music/Artist/Album (2001)/01 - Track.flac` (tags take precedence)
//! * `Audiobooks/Author/Series/1 - Book (2001)/01 - Chapter.mp3`,
//!   `Author/Book/Book.m4b`, `Author/Book.m4b` (tags take precedence; see
//!   [`parse_audiobook`])

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::util::normalize_key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovieName {
    pub title: String,
    pub year: Option<i32>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeName {
    pub show: String,
    pub show_year: Option<i32>,
    pub show_dir: Option<PathBuf>,
    pub show_tmdb_id: Option<i64>,
    pub season: i32,
    pub season_dir: Option<PathBuf>,
    /// `None` when the name gives none: such an episode is told apart by its
    /// title, never merged with the season's other unnumbered episodes.
    pub episode: Option<i32>,
    pub episode_end: Option<i32>,
    pub title: Option<String>,
    pub air_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackName {
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub title: String,
    pub track: Option<i32>,
    pub disc: Option<i32>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub album_dir: PathBuf,
    pub artist_dir: Option<PathBuf>,
}

static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b((?:19|20)\d{2})\b").unwrap());
/// `Season 1 Episode 4`, `season 2 - ep 5`.
static SEASON_EPISODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-z0-9])season[ ._-]*(\d{1,4})[ ._,-]*(?:episode|ep|e)[ ._-]*(\d{1,4})(?:[^0-9]|$)")
        .unwrap()
});
static SXXEYY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-z0-9])s(\d{1,4})[ ._-]*e(\d{1,4})(?:(?:[ ._]*-[ ._]*e?|[ ._]*e)(\d{1,4}))?").unwrap()
});
static NXNN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[^a-z0-9])(\d{1,2})x(\d{2,3})(?:[^0-9]|$)").unwrap());
static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b((?:19|20)\d{2})[-._ ](\d{2})[-._ ](\d{2})\b").unwrap());
static EP_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[^a-z0-9])(?:e|ep|episode)[ ._-]*(\d{1,4})(?:[^0-9]|$)").unwrap());
static ANIME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s-\s(\d{1,4})(?:v\d)?(?:\s|$|\[|\()").unwrap());
static LEADING_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d{1,4})(?:[ ._-]+|$)").unwrap());
/// Tags in any brackets: `(Group)`, `[1080p]`, `{tmdb-1}`, `(x264)(CRC32)`.
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]|\{[^}]*\}|\([^)]*\)").unwrap());
/// A name, then a number at the end or before ` - Title` (`Monster 01v2`).
static NAME_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+?)[ ._-]+(\d{1,4})(?:v\d+)?(?:\s+-\s+(.+))?$").unwrap());
static SEASON_DIR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:season|series|staffel|saison|temporada|stagione|seizoen|s)[ ._-]*(\d{1,4})\b").unwrap()
});
static SPECIALS_DIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^specials?$").unwrap());
static JUNK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:2160p|1440p|1080p|1080i|720p|576p|480p|4k|uhd|hdr10\+?|hdr|dv|dovi|sdr|blu-?ray|bdrip|brrip|bdremux|webrip|web-?dl|webdl|hdtv|pdtv|dvdrip|dvdscr|dvd|hdrip|remux|x264|x265|h\.?264|h\.?265|hevc|avc|xvid|divx|aac(?:2\.0|5\.1)?|ac3|eac3|ddp?5\.1|dts(?:-hd)?|truehd|atmos|flac|10bit|8bit|proper|repack|rerip|extended|unrated|internal|limited|multi|dubbed|subbed|imax|amzn|nf|dsnp|hmax|atvp|hulu|criterion)\b",
    )
    .unwrap()
});
static BRACKETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]|\{[^}]*\}").unwrap());
static TMDB_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)[\[{(]tmdb(?:id)?[-= ](\d+)[\]})]").unwrap());
static IMDB_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(tt\d{7,9})\b").unwrap());
/// Plex's edition tag: `Blade Runner (1982) {edition-Final Cut}`.
static EDITION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\{edition-([^}]+)\}").unwrap());
/// Words that name an edition of a film: `Director's Cut`, `Extended
/// Edition`, `Unrated`, `Remastered`, `25th Anniversary Edition`, `IMAX`.
/// Words alone that are as likely a title's (`Final`, `Special`) only count
/// with the word after them.
const EDITION_WORDS: &str = concat!(
    r"(?:director(?:['’]?s)?[ ._-]+cut",
    r"|(?:extended|theatrical|unrated)(?:[ ._-]+(?:cut|edition|version))?",
    r"|(?:ultimate|final)[ ._-]+cut",
    r"|(?:ultimate|special|collector(?:['’]?s)?)[ ._-]+edition",
    r"|\d{1,3}(?:st|nd|rd|th)[ ._-]+anniversary(?:[ ._-]+edition)?|anniversary[ ._-]+edition",
    r"|uncut|remaster(?:ed)?|imax(?:[ ._-]+edition)?)"
);
/// One edition, or several in a row that name one together (`Unrated
/// Director's Cut`).
static EDITIONS: LazyLock<String> = LazyLock::new(|| format!(r"{EDITION_WORDS}(?:[ ._-]+{EDITION_WORDS})*"));
static EDITION_IN: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"(?i)\b{}\b", *EDITIONS)).unwrap());
static EDITION_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"(?i)^{}\b", *EDITIONS)).unwrap());
/// What is in brackets, parentheses or braces.
static GROUPS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]|\(([^)]*)\)|\{([^}]*)\}").unwrap());
static PART_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[ ._-]+(?:cd|dvd|disc|disk|part|pt)[ ._-]?\d{1,2}$").unwrap());
static TRACK_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:(\d{1,2})[-.])?(\d{1,3})[ ._-]+(.+)$").unwrap());
static PAREN_YEAR_SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*[\[(]((?:19|20)\d{2})[\])]\s*$").unwrap());

fn max_year() -> i32 {
    // Rough current year without a date library; generous upper bound.
    let secs = crate::util::now();
    1970 + (secs / 31_556_952) as i32 + 2
}

fn file_stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Release names use dots/underscores as spaces ("The.Matrix.1999").
fn spaces(s: &str) -> String {
    let dots = s.matches(['.', '_']).count();
    let spaces = s.matches(' ').count();
    let s = if dots > spaces { s.replace(['_', '.'], " ") } else { s.replace('_', " ") };
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Final tidy-up of a title fragment.
pub fn clean_title(s: &str) -> String {
    let separator = |c: char| c.is_whitespace() || matches!(c, '-' | '–' | '(' | '[' | ',' | '_' | ':');
    let s = spaces(s);
    let mut s = s.trim_start_matches(|c: char| separator(c) || c == '.').trim_end_matches(separator);
    // A final dot after a letter is part of the title ("Sherlock Jr.",
    // "Crazy, Stupid, Love."); a stray one is not.
    while let Some(rest) = s.strip_suffix('.') {
        if rest.ends_with(char::is_alphanumeric) {
            break;
        }
        s = rest.trim_end_matches(separator);
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_brackets(s: &str) -> String {
    BRACKETS.replace_all(s, " ").into_owned()
}

fn cut_at_junk(s: &str) -> &str {
    match JUNK.find(s) {
        Some(m) if m.start() > 0 => &s[..m.start()],
        _ => s,
    }
}

/// `"The Matrix (1999) - 1080p"` → `("The Matrix", Some(1999))`.
pub fn title_and_year(raw: &str) -> (String, Option<i32>) {
    let s = spaces(&strip_brackets(raw));
    match year_in(&s) {
        Some((start, _, year)) => {
            let title = clean_title(&s[..start]);
            if title.is_empty() { (clean_title(cut_at_junk(&s)), Some(year)) } else { (title, Some(year)) }
        }
        None => (clean_title(cut_at_junk(&s)), None),
    }
}

/// The year a name (without its brackets, spaced) gives, where it starts
/// and ends: the last one after the start of it.
fn year_in(s: &str) -> Option<(usize, usize, i32)> {
    let max = max_year();
    let mut chosen = None;
    for caps in YEAR.captures_iter(s) {
        let m = caps.get(1).unwrap();
        let year: i32 = m.as_str().parse().unwrap_or(0);
        if m.start() > 0 && (1900..=max).contains(&year) {
            chosen = Some((m.start(), m.end(), year));
        }
    }
    chosen
}

/// An edition's words as they are shown: `directors.cut` → "Director's Cut".
fn edition_name(words: &str) -> String {
    words
        .split([' ', '.', '_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let lower = word.to_lowercase();
            if lower.starts_with("director") {
                "Director's".to_owned()
            } else if lower.starts_with("collector") {
                "Collector's".to_owned()
            } else if lower == "imax" {
                "IMAX".to_owned()
            } else {
                let mut chars = lower.chars();
                chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The edition `text` names when it names nothing else, release details
/// after it aside (`Director's Cut 1080p`).
fn edition_alone(text: &str) -> Option<String> {
    let separator = |c: char| c.is_whitespace() || matches!(c, '-' | '.' | '_' | ',');
    let text = text.trim_matches(separator);
    // Release details may begin with one of an edition's words (`Extended
    // 1080p`), so they are what follows the edition, not what a cut at the
    // first of them leaves.
    let words = EDITION_START.find(text)?;
    let rest = text[words.end()..].trim_start_matches(separator);
    (rest.is_empty() || JUNK.find(rest).is_some_and(|junk| junk.start() == 0)).then(|| edition_name(words.as_str()))
}

/// The edition a movie's name gives in words, apart from its title: after
/// its year, or without one, after where release details begin; alone in
/// brackets or parentheses; or without a year, alone after ` - `. Words
/// before a year are the title's ("The Final Cut (2004)").
fn edition_words(name: &str) -> Option<String> {
    let s = spaces(&strip_brackets(name));
    let year = year_in(&s);
    let details = match year {
        Some((_, end, _)) => Some(end),
        None => JUNK.find(&s).map(|m| m.start()).filter(|start| *start > 0),
    };
    if let Some(found) = details.and_then(|start| EDITION_IN.find(&s[start..])) {
        return Some(edition_name(found.as_str()));
    }
    let grouped =
        GROUPS.captures_iter(name).find_map(|c| c.iter().skip(1).flatten().find_map(|g| edition_alone(g.as_str())));
    grouped.or_else(|| year.is_none().then(|| s.rsplit_once(" - ").and_then(|(_, rest)| edition_alone(rest))).flatten())
}

/// A title without a year, less an edition set apart at its end
/// (`Film - Director's Cut`, `Film (Uncut)`); the title itself when that
/// would leave nothing.
fn without_edition(title: &str) -> String {
    let rest = if let Some(open) = title.strip_suffix(')').and_then(|t| t.rfind('(')) {
        edition_alone(&title[open + 1..title.len() - 1]).map(|_| &title[..open])
    } else {
        title.rsplit_once(" - ").and_then(|(left, right)| edition_alone(right).map(|_| left))
    };
    rest.map(clean_title).filter(|t| !t.is_empty()).unwrap_or_else(|| title.to_owned())
}

/// The edition after the title of the folder a file is in, when the file
/// is named as that film and the edition, without a year
/// (`Film/Film Directors Cut.mkv`).
fn edition_after_folder(stem: &str, folder: &str) -> Option<String> {
    let (title, year) = title_and_year(stem);
    if year.is_some() {
        return None;
    }
    let (folder, _) = title_and_year(folder);
    edition_alone(&title[prefix_end(&title, &folder)?..])
}

fn extract_ids(s: &str) -> (Option<i64>, Option<String>) {
    let tmdb = TMDB_ID.captures(s).and_then(|c| c[1].parse().ok());
    let imdb = IMDB_ID.captures(s).map(|c| c[1].to_owned());
    (tmdb, imdb)
}

/// The edition a file is tagged with (`{edition-Director's Cut}`), in its
/// name or its folder's, else, of a `movie`'s, the one its name or its
/// folder's gives in words (`Film (2010) - Director's Cut`, see
/// [`edition_words`]). Editions of a film are versions of one movie.
pub fn edition(path: &Path, movie: bool) -> Option<String> {
    let tagged = |name: &str| EDITION.captures(name).map(|c| c[1].trim().to_owned()).filter(|e| !e.is_empty());
    let stem = file_stem(path);
    let folder = path.parent().and_then(Path::file_name).map(|f| f.to_string_lossy().into_owned());
    let found = tagged(&stem).or_else(|| folder.as_deref().and_then(tagged));
    // Words name only a movie's edition: an episode's title may well be
    // such words ("Final Cut").
    if found.is_some() || !movie {
        return found;
    }
    let stem = PART_SUFFIX.replace(&stem, "").into_owned();
    edition_words(&stem)
        .or_else(|| folder.as_deref().and_then(|folder| edition_after_folder(&stem, folder)))
        .or_else(|| folder.as_deref().and_then(edition_words))
}

fn rel_components(path: &Path, root: &Path) -> Vec<String> {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn is_generic_name(stem: &str) -> bool {
    is_generic_word(stem) || stem.chars().all(|c| c.is_ascii_digit() || c == '_' || c == '-')
}

/// A name a disc rip or a camera gives every file: never a film's title.
fn is_generic_word(stem: &str) -> bool {
    let lower = stem.to_ascii_lowercase();
    matches!(lower.as_str(), "movie" | "video" | "film" | "main" | "feature" | "title" | "untitled")
        || lower.starts_with("title_t")
}

pub fn parse_movie(path: &Path, root: &Path) -> MovieName {
    let stem = PART_SUFFIX.replace(&file_stem(path), "").into_owned();
    let comps = rel_components(path, root);
    let folder = (comps.len() >= 2).then(|| comps[comps.len() - 2].clone());

    let (mut tmdb_id, mut imdb_id) = extract_ids(&stem);
    if let Some(f) = &folder {
        let (t, i) = extract_ids(f);
        tmdb_id = tmdb_id.or(t);
        imdb_id = imdb_id.or(i);
    }

    // An edition set apart at the end of a name without a year is no part
    // of its title, as one after a year isn't: copies of a film in its
    // editions are one movie.
    let mut from_file = title_and_year(&stem);
    let mut from_folder = folder.as_deref().map(title_and_year);
    if from_file.1.is_none() {
        from_file.0 = without_edition(&from_file.0);
        if let Some((folder_title, _)) = &from_folder
            && let Some(f) = &folder
            && edition_after_folder(&stem, f).is_some()
        {
            from_file.0 = folder_title.clone();
        }
    }
    if let Some((folder_title, None)) = &mut from_folder {
        *folder_title = without_edition(folder_title);
    }
    let (title, year) = match (from_file, from_folder) {
        // A title of digits alone ("300", "1917") is the film's when the
        // name gives its year too, as a renamed file's does.
        ((t, Some(y)), _) if !t.is_empty() && !is_generic_word(&t) => (t, Some(y)),
        (_, Some((t, Some(y)))) if !t.is_empty() => (t, Some(y)),
        ((t, None), Some((ft, None))) if (t.is_empty() || is_generic_name(&stem)) && !ft.is_empty() => (ft, None),
        ((t, y), _) => (if t.is_empty() { clean_title(&stem) } else { t }, y),
    };
    MovieName { title: if title.is_empty() { stem } else { title }, year, tmdb_id, imdb_id }
}

/// Season number encoded in a folder name ("Season 02", "S2", "Specials").
pub fn season_from_dir(name: &str) -> Option<i32> {
    if SPECIALS_DIR.is_match(name.trim()) {
        return Some(0);
    }
    SEASON_DIR.captures(name.trim()).and_then(|c| c[1].parse().ok())
}

pub fn parse_episode(path: &Path, root: &Path) -> EpisodeName {
    let comps = rel_components(path, root);
    let dirs = &comps[..comps.len().saturating_sub(1)];
    let stem = file_stem(path);
    let clean_stem = strip_brackets(&stem);

    // Show folder: the first directory below the library root, unless that
    // directory is itself a season folder.
    let show_folder = dirs.first().filter(|d| season_from_dir(d).is_none()).cloned();
    let folder_show = show_folder.as_deref().map(|f| title_and_year(f).0).filter(|t| !t.is_empty());
    let mut season_dir = None;
    let mut dir_season = None;
    for (i, d) in dirs.iter().enumerate().rev() {
        if let Some(n) = season_from_dir(d) {
            dir_season = Some(n);
            season_dir = Some(dirs[..=i].iter().fold(root.to_path_buf(), |p, c| p.join(c)));
            break;
        }
    }

    let mut season = None;
    let mut episode = None;
    let mut episode_end = None;
    let mut air_date = None;
    let mut prefix = String::new();
    let mut rest = String::new();
    // The name starts with the number (`104 - Title`).
    let mut leading = false;

    if let Some(c) = SEASON_EPISODE.captures(&clean_stem) {
        let m = c.get(0).unwrap();
        season = c[1].parse().ok();
        episode = c[2].parse().ok();
        prefix = clean_stem[..m.start()].to_owned();
        rest = clean_stem[m.end()..].to_owned();
    } else if let Some(c) = SXXEYY.captures(&clean_stem) {
        let m = c.get(0).unwrap();
        season = c[1].parse().ok();
        episode = c[2].parse().ok();
        episode_end = c.get(3).and_then(|e| e.as_str().parse().ok()).filter(|e| Some(*e) > episode);
        prefix = clean_stem[..m.start()].to_owned();
        rest = clean_stem[m.end()..].to_owned();
    } else if let Some(c) = NXNN.captures(&clean_stem) {
        let m = c.get(0).unwrap();
        season = c[1].parse().ok();
        episode = c[2].parse().ok();
        prefix = clean_stem[..m.start()].to_owned();
        rest = clean_stem[m.end()..].to_owned();
    } else if let Some(c) = DATE.captures(&clean_stem) {
        let m = c.get(0).unwrap();
        let (y, mo, d) = (&c[1], &c[2], &c[3]);
        air_date = Some(format!("{y}-{mo}-{d}"));
        season = y.parse().ok();
        episode = format!("{mo}{d}").parse().ok();
        prefix = clean_stem[..m.start()].to_owned();
        rest = clean_stem[m.end()..].to_owned();
    } else if let Some(c) = EP_ONLY.captures(&clean_stem) {
        let m = c.get(0).unwrap();
        episode = c[1].parse().ok();
        prefix = clean_stem[..m.start()].to_owned();
        rest = clean_stem[m.end()..].to_owned();
    } else if let Some(c) = ANIME.captures(&stem.replace('_', " ")) {
        let s = stem.replace('_', " ");
        let m = c.get(0).unwrap();
        episode = c[1].parse().ok();
        prefix = strip_brackets(&s[..m.start()]);
        // A title may follow after another dash (`Show - 04 - Title`); what
        // follows otherwise is release tags (`- 01 (Dual Audio 10bit)`).
        let after = &s[c.get(1).unwrap().end()..];
        let after =
            after.strip_prefix('v').map(|r| r.trim_start_matches(|c: char| c.is_ascii_digit())).unwrap_or(after);
        if after.trim_start().starts_with('-') {
            rest = strip_brackets(after);
        }
    } else if let Some((number, after)) = folder_show.as_deref().and_then(|show| number_after_show(&stem, show)) {
        // `(Group) Monster 01v2 (x264)`: the show's own name, then the number.
        episode = Some(number);
        rest = after;
    } else if let Some(c) = LEADING_NUMBER.captures(clean_stem.trim()) {
        let trimmed = clean_stem.trim();
        let m = c.get(0).unwrap();
        episode = c[1].parse().ok();
        rest = trimmed[m.end()..].to_owned();
        leading = true;
    }

    // Season and episode run together (`104 - Title` for 1x04, `1012` for
    // 10x12): only at the start of the name, in a season folder whose number
    // is the hundreds. Anime numbers its episodes on from one season to the
    // next (`Show - 104`), never at the start of the name.
    if leading
        && let (Some(s), Some(n)) = (dir_season, episode)
        && s >= 1
        && n >= 100
        && n / 100 == s
        && n % 100 >= 1
    {
        episode = Some(n % 100);
    }

    let season = season.or(dir_season).unwrap_or(1);

    let (show, show_year, show_tmdb_id) = match &show_folder {
        Some(folder) => {
            let (title, year) = title_and_year(folder);
            (title, year, extract_ids(folder).0)
        }
        None => {
            let (title, year) = title_and_year(&prefix);
            (title, year, None)
        }
    };
    let show = if show.is_empty() { "Unknown Show".to_owned() } else { show };

    let title = {
        let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '.' | '_' | ':'));
        let t = clean_title(cut_at_junk(&spaces(rest)));
        let t = PART_SUFFIX.replace(&t, "").into_owned();
        (!t.is_empty() && !t.eq_ignore_ascii_case(&show) && !JUNK.is_match(&t)).then_some(t)
    };
    // Without a number, the name (less the show's) is the title, which tells
    // the episode apart.
    let title = match (episode, title) {
        (None, None) => {
            let t = clean_title(cut_at_junk(&spaces(&clean_stem)));
            let t = without_prefix(&t, &show);
            let t = PART_SUFFIX.replace(&t, "").into_owned();
            (!t.is_empty() && !t.eq_ignore_ascii_case(&show)).then_some(t)
        }
        (_, title) => title,
    };

    EpisodeName {
        show,
        show_year,
        show_dir: show_folder.map(|f| root.join(f)),
        show_tmdb_id,
        season,
        season_dir,
        episode,
        episode_end,
        title,
        air_date,
    }
}

/// `title` without `prefix` (the show's name) and the separators after it,
/// when it starts with it: the same words, whatever the case and
/// punctuation (`Full Metal Panic S4` in `Full Metal Panic!`).
fn without_prefix(title: &str, prefix: &str) -> String {
    match prefix_end(title, prefix) {
        Some(end) => {
            clean_title(title[end..].trim_start_matches(|c: char| c.is_whitespace() || "!?.:,-_–".contains(c)))
        }
        None => title.to_owned(),
    }
}

/// Where `name` ends at the start of `text`, compared as
/// [`normalize_key`] does; `None` when `text` doesn't start with it as whole
/// words ("Lost" doesn't start "Lostboys").
fn prefix_end(text: &str, name: &str) -> Option<usize> {
    let wanted = normalize_key(name);
    if wanted.is_empty() {
        return None;
    }
    text.char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .find(|&end| normalize_key(&text[..end]) == wanted)
        .filter(|&end| !text[end..].starts_with(char::is_alphanumeric))
}

/// The episode number in a name made of the show's name and a number
/// (`(Group) Monster 01v2 (x264)`, `Monster 05 - Title`), with what follows
/// it; `None` for any other name, and for a year.
fn number_after_show(stem: &str, show: &str) -> Option<(i32, String)> {
    let bare = spaces(&TAGS.replace_all(stem, " "));
    let c = NAME_NUMBER.captures(&bare)?;
    if normalize_key(&c[1]) != normalize_key(show) {
        return None;
    }
    let number: i32 = c[2].parse().ok()?;
    (!(1900..=2099).contains(&number)).then(|| (number, c.get(3).map(|t| t.as_str().to_owned()).unwrap_or_default()))
}

fn tag<'a>(tags: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| tags.get(*k)).map(|v| v.trim()).filter(|v| !v.is_empty())
}

fn leading_int(s: &str) -> Option<i32> {
    let digits: String = s.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok().filter(|n| *n > 0)
}

pub fn parse_track(path: &Path, root: &Path, tags: &BTreeMap<String, String>) -> TrackName {
    let comps = rel_components(path, root);
    let dirs = &comps[..comps.len().saturating_sub(1)];
    let album_dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| root.to_path_buf());
    let artist_dir = (dirs.len() >= 2).then(|| album_dir.parent().map(Path::to_path_buf)).flatten();
    let folder_album = dirs.last().map(|d| PAREN_YEAR_SUFFIX.replace(d, "").trim().to_owned());
    let folder_artist = (dirs.len() >= 2).then(|| dirs[dirs.len() - 2].clone());

    let stem = file_stem(path);
    let (file_disc, file_track, file_title) = match TRACK_PREFIX.captures(&stem) {
        Some(c) => (
            c.get(1).and_then(|d| d.as_str().parse().ok()),
            c[2].parse().ok(),
            clean_track_title(&c[3], folder_artist.as_deref()),
        ),
        None => (None, None, stem.replace('_', " ").trim().to_owned()),
    };

    let artist = tag(tags, &["artist", "album_artist", "albumartist"])
        .map(str::to_owned)
        .or_else(|| folder_artist.clone())
        .unwrap_or_else(|| "Unknown Artist".into());
    let album_artist = tag(tags, &["album_artist", "albumartist", "album artist"])
        .map(str::to_owned)
        .or_else(|| folder_artist.clone())
        .unwrap_or_else(|| artist.clone());
    let album = tag(tags, &["album"])
        .map(str::to_owned)
        .or(folder_album)
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "Unknown Album".into());
    let title = tag(tags, &["title"]).map(str::to_owned).unwrap_or(file_title);
    let year = tag(tags, &["date", "year", "originaldate", "original_date"])
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse().ok())
        .filter(|y| (1000..=9999).contains(y))
        .or_else(|| dirs.last().and_then(|d| PAREN_YEAR_SUFFIX.captures(d)).and_then(|c| c[1].parse().ok()));

    TrackName {
        artist,
        album_artist,
        album,
        title,
        track: tag(tags, &["track", "tracknumber"]).and_then(leading_int).or(file_track),
        disc: tag(tags, &["disc", "discnumber", "disk"]).and_then(leading_int).or(file_disc),
        year,
        genre: tag(tags, &["genre"]).map(str::to_owned),
        album_dir,
        artist_dir,
    }
}

/// A file of an audiobook: the book it belongs to and the part it is.
#[derive(Debug, Clone, PartialEq)]
pub struct AudiobookName {
    pub title: String,
    /// Writers, in the order the tags or folder name them.
    pub authors: Vec<String>,
    pub narrators: Vec<String>,
    pub series: Option<String>,
    /// The book's place in its series: `1`, `2.5`.
    pub series_index: Option<f64>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub summary: Option<String>,
    /// What tells the book apart in its library: its folder, its folder and
    /// its album, or the file itself (see [`parse_audiobook`]).
    pub book_key: String,
    /// The book's own folder, which its files (or its `CD 1`… folders) are
    /// in: none for a book in the library's folder or in its author's,
    /// whose pictures aren't the book's.
    pub book_dir: Option<PathBuf>,
    pub part_title: String,
    pub part: Option<i32>,
    pub disc: Option<i32>,
    /// What the part is sorted by after its disc and number: its file's
    /// name (see [`natural_key`]), which tells parts apart when their tags
    /// number none.
    pub part_sort: String,
}

/// A folder of one disc of a book: `CD 1`, `Disc 02`, `Part 3`.
static DISC_DIR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:cd|disc|disk|part|pt)[ ._-]*(\d{1,3})$").unwrap());
/// A book's place in its series at the start of its folder's name:
/// `1 - Title`, `Book 2 - Title`, `Vol. 3. Title`, `#4 Title`; a bare number
/// only before a dash, a colon or a dot, so `3 Days Later` keeps its own.
static SERIES_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(?:(?:book|vol(?:ume)?|part|#)[ .]*(\d{1,3}(?:\.\d{1,2})?)(?:[ ._]*[-–:.][ ._]*|\s+)|(\d{1,3}(?:\.\d{1,2})?)[ ._]*[-–:.][ ._]*)(.+)$",
    )
    .unwrap()
});
/// A year before the title: `2001 - Book One`.
static YEAR_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^((?:1[5-9]|20)\d{2})[ ._]*[-–][ ._]*(.+)$").unwrap());
/// A narrator in braces, as Audiobookshelf names folders: `Book One {Narrator B}`.
static NARRATOR_BRACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\{([^}]+)\}\s*").unwrap());
/// A series and the book's place in it, as Audiobookshelf writes them in a
/// `grouping` tag: `Series #2`, `Series, Book 2.5`.
static GROUPING_SERIES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^([^#;]+?)(?:\s*#\s*|\s*,\s*book\s+)(\d{1,4}(?:\.\d{1,2})?)$").unwrap());

/// The series a `grouping` tag names, and the book's place in it: only one
/// written as Audiobookshelf writes it, as others put genres or anything
/// else there.
fn series_in_grouping(grouping: &str) -> Option<(String, Option<f64>)> {
    let c = GROUPING_SERIES.captures(grouping.trim())?;
    let series = c[1].trim();
    (!series.is_empty()).then(|| (series.to_owned(), c[2].parse().ok()))
}

// What an audiobook's tags say is bounded as an NFO file's is (SECURITY.md,
// "Untrusted media"): a crafted file's tags cost a scan, which holds the
// database's only writer, no more than a real file's.
/// The characters of a tag that names are read from…
const MAX_NAMES_TEXT: usize = 2_000;
/// …the names read from it…
const MAX_NAMES: usize = 50;
/// …and the characters of a name, a title, a series or a genre.
const MAX_TEXT: usize = 500;
const MAX_SUMMARY: usize = 20_000;
/// The highest place in a series.
const MAX_SERIES_INDEX: f64 = 10_000.0;

/// `value`'s first `max` characters.
fn cut(value: &str, max: usize) -> &str {
    value.char_indices().nth(max).map_or(value, |(end, _)| &value[..end])
}

/// Names in one tag: `Author A; Author B`, `A & B`, `A / B`.
/// Commas stay, as in `Last, First`.
fn names(value: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let names = cut(value, MAX_NAMES_TEXT).split([';', '&', '/']).map(|n| cut(n.trim(), MAX_TEXT).trim_end());
    for name in names.filter(|n| !n.is_empty()) {
        if out.len() == MAX_NAMES {
            break;
        }
        if !out.iter().any(|o| o.eq_ignore_ascii_case(name)) {
            out.push(name.to_owned());
        }
    }
    out
}

/// A book's folder name: its title, its place in its series and its year,
/// and a narrator in braces.
fn book_folder(name: &str) -> (String, Option<f64>, Option<i32>, Option<String>) {
    let narrator = NARRATOR_BRACES.captures(name).map(|c| c[1].trim().to_owned());
    let mut title = NARRATOR_BRACES.replace_all(name, " ").trim().to_owned();
    let mut year = PAREN_YEAR_SUFFIX.captures(&title).and_then(|c| c[1].parse().ok());
    title = PAREN_YEAR_SUFFIX.replace(&title, "").trim().to_owned();
    if let Some(c) = YEAR_PREFIX.captures(&title) {
        year = year.or_else(|| c[1].parse().ok());
        title = c[2].trim().to_owned();
    }
    let mut index = None;
    if let Some(c) = SERIES_PREFIX.captures(&title)
        // A bare number before another is the title's (`10.20.30`).
        && (c.get(2).is_none() || !c[3].starts_with(|c: char| c.is_ascii_digit()))
    {
        index = c.get(1).or(c.get(2)).and_then(|n| n.as_str().parse().ok());
        title = c[3].trim().to_owned();
    }
    (title.replace('_', " ").trim().to_owned(), index, year, narrator)
}

/// How many digits each number in a part's [`natural_key`] takes.
const NUMBER_WIDTH: usize = 10;

/// A file's name as a book's parts are sorted by it: in any case, and with
/// each number padded to the same width, so `Part 2` comes before `Part 10`
/// and `Chapter 9` before `chapter 10`.
pub(crate) fn natural_key(name: &str) -> String {
    let mut key = String::with_capacity(name.len() + NUMBER_WIDTH);
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if !c.is_ascii_digit() {
            key.extend(c.to_lowercase());
            continue;
        }
        let mut number = String::from(c);
        while let Some(d) = chars.next_if(char::is_ascii_digit) {
            number.push(d);
        }
        let number = number.trim_start_matches('0');
        key.extend(std::iter::repeat_n('0', NUMBER_WIDTH.saturating_sub(number.len())));
        key.push_str(number);
    }
    key
}

/// Reads an audiobook's file: the book it is part of, from its tags
/// (`album`, `artist`, `composer` for the narrator, `series`…) and else from
/// its folders, as Audiobookshelf reads them, and the part it is.
///
/// The file alone says which book it is part of: a scan reads a book's
/// files in any order, and the files beside it change between scans.
/// * A file in the library's folder is a book.
/// * An `.m4b` with an `album` tag is part of the book of that album in its
///   folder: a book in several `.m4b` files is one book, and books side by
///   side in their author's or their series' folder are each their own.
/// * An `.m4b` without one is a book.
/// * Any other file is part of its folder's book, as with Audiobookshelf:
///   loose files side by side are parts of one book.
///
/// The book's folder is the file's, or the one above a `CD 1`, `Disc 2`…
/// folder. Of the folders it is in, the first is its author's
/// (`Author/Book`) and the second its series' (`Author/Series/Book`); a
/// book's folder at the top of the library is its own alone. An `.m4b` in a
/// folder at the top (`Author/Book.m4b`) is in its author's, though, unless
/// the folder is named as the book or the file is (`Book/Book.m4b`).
pub fn parse_audiobook(path: &Path, root: &Path, tags: &BTreeMap<String, String>) -> AudiobookName {
    let comps = rel_components(path, root);
    let mut dirs: Vec<String> = comps[..comps.len().saturating_sub(1)].to_vec();
    let mut folder = path.parent().map(Path::to_path_buf).unwrap_or_else(|| root.to_path_buf());

    // `Book/CD 1/01.mp3`: the book is the folder above the disc's.
    let mut folder_disc = None;
    if dirs.len() >= 2
        && let Some(c) = DISC_DIR.captures(dirs.last().unwrap())
    {
        folder_disc = c[1].parse().ok();
        dirs.pop();
        folder = folder.parent().map(Path::to_path_buf).unwrap_or(folder);
    }

    // A tag, cut to `max` characters.
    let text = |keys: &[&str], max: usize| tag(tags, keys).map(|v| cut(v, max).trim_end().to_owned());

    let stem = file_stem(path);
    let m4b = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("m4b"));
    let album = text(&["album"], MAX_TEXT);
    let from_stem = book_folder(&stem);

    // `Author/Book.m4b`: the folder is the author's, unless it has the
    // book's name, by its album or its file. Above a `CD 1` folder, it is
    // the book's.
    let in_authors_folder = m4b && dirs.len() == 1 && folder_disc.is_none() && {
        let name = normalize_key(&book_folder(&dirs[0]).0);
        album.as_deref().is_none_or(|a| normalize_key(a) != name) && normalize_key(&from_stem.0) != name
    };
    let book_dir = (!dirs.is_empty() && !in_authors_folder).then(|| folder.clone());
    // The folders above the book's: its author's, then its series'.
    let above = if book_dir.is_some() { &dirs[..dirs.len() - 1] } else { &dirs[..] };
    let folder_author = above.first();
    let folder_series = above.get(1);

    // An `.m4b` on a disc of a book is a part of it, tagged or not.
    let by_file = dirs.is_empty() || (m4b && album.is_none() && folder_disc.is_none());
    let book_key = match &album {
        _ if by_file => path_key(path),
        // A line break, which no folder's name has, keeps the folder and
        // the album apart.
        Some(album) if m4b => format!("{}\n{}", path_key(&folder), album_key(album)),
        _ => path_key(&folder),
    };

    // The name that tells the book: its folder's, unless the file is the
    // book on its own (its folder may hold others) or it has none.
    let (named, folder_index, folder_year, folder_narrator) = match dirs.last() {
        Some(dir) if !by_file && book_dir.is_some() => book_folder(dir),
        _ => from_stem,
    };
    let title = album.clone().or(Some(named)).filter(|t| !t.is_empty()).unwrap_or_else(|| stem.clone());
    let authors = tag(tags, &["album_artist", "albumartist", "album artist", "author", "artist"])
        .map(names)
        .filter(|a| !a.is_empty())
        .or_else(|| folder_author.map(|a| names(a)))
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| vec!["Unknown Author".into()]);
    let narrators = tag(tags, &["narrator", "narrated_by", "composer"])
        .map(names)
        .or_else(|| folder_narrator.map(|n| names(&n)))
        .unwrap_or_default();
    let tagged_series = text(&["series", "movementname", "mvnm"], MAX_TEXT);
    let grouping =
        text(&["grouping"], MAX_TEXT).filter(|_| tagged_series.is_none()).and_then(|g| series_in_grouping(&g));
    let series = tagged_series
        .or_else(|| grouping.as_ref().map(|(series, _)| series.clone()))
        .or_else(|| folder_series.map(|s| s.replace('_', " ").trim().to_owned()))
        .filter(|s| !s.is_empty());
    // `part` last: Libation and tone write the place in a series as
    // `----:com.pilabor.tone:PART`, which FFmpeg reads as `part`.
    let series_index = tag(tags, &["series-part", "series_part", "seriespart", "movement", "mvin", "part"])
        .and_then(|v| v.trim().parse().ok())
        .filter(|i: &f64| i.is_finite() && (0.0..=MAX_SERIES_INDEX).contains(i))
        .or(grouping.and_then(|(_, index)| index))
        .or(folder_index)
        .filter(|_| series.is_some());
    let year = tag(tags, &["date", "year", "originaldate", "original_date"])
        .and_then(|d| d.get(..4))
        .and_then(|y| y.parse().ok())
        .filter(|y| (1000..=9999).contains(y))
        .or(folder_year);

    // Parts are named by their titles, unless that is the book's, else by
    // their files: an `.m4b` too, which may be one of a book's.
    let (file_disc, file_part, file_title) = match TRACK_PREFIX.captures(&stem) {
        Some(c) => (c.get(1).and_then(|d| d.as_str().parse().ok()), c[2].parse().ok(), clean_title(&c[3])),
        None => (None, None, clean_title(&stem)),
    };
    let part_title = text(&["title"], MAX_TEXT)
        .filter(|t| album.as_ref() != Some(t))
        .unwrap_or(if file_title.is_empty() { stem.clone() } else { file_title });

    AudiobookName {
        title,
        authors,
        narrators,
        series,
        series_index,
        year,
        genre: text(&["genre"], MAX_TEXT),
        // A comment is often a ripper's or a shop's note: a summary only
        // when the file has no other.
        summary: text(&["description", "synopsis"], MAX_SUMMARY).or_else(|| text(&["comment"], MAX_SUMMARY)),
        book_key,
        book_dir,
        part_title,
        part: tag(tags, &["track", "tracknumber"]).and_then(leading_int).or(file_part),
        disc: folder_disc.or_else(|| tag(tags, &["disc", "discnumber", "disk"]).and_then(leading_int)).or(file_disc),
        part_sort: natural_key(&stem),
    }
}

/// An album as books are told apart by it: trimmed, in any case.
fn album_key(album: &str) -> String {
    album.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// "Artist - Title" → "Title" when the artist matches the folder.
fn clean_track_title(raw: &str, artist: Option<&str>) -> String {
    let t = raw.replace('_', " ").trim().to_owned();
    if let Some(artist) = artist
        && let Some(rest) = t.strip_prefix(artist)
    {
        let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '-');
        if !rest.is_empty() {
            return rest.to_owned();
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(rel: &str) -> MovieName {
        parse_movie(&Path::new("/media/movies").join(rel), Path::new("/media/movies"))
    }

    fn episode(rel: &str) -> EpisodeName {
        parse_episode(&Path::new("/media/tv").join(rel), Path::new("/media/tv"))
    }

    #[test]
    fn editions_are_read_from_the_file_or_its_folder() {
        let edition = |rel: &str| edition(&Path::new("/media/movies").join(rel), true);
        assert_eq!(edition("Blade Runner (1982) {edition-Final Cut}.mkv").as_deref(), Some("Final Cut"));
        assert_eq!(
            edition("Blade Runner (1982) {Edition-Director's Cut}/Blade Runner (1982).mkv").as_deref(),
            Some("Director's Cut")
        );
        assert_eq!(edition("Alien (1979) {edition- }.mkv"), None, "an empty tag is no edition");
        assert_eq!(edition("Alien (1979) {tmdb-348}.mkv"), None);
        // The tag stays out of the title, as every brace tag does.
        assert_eq!(movie("Blade Runner (1982) {edition-Final Cut}.mkv").title, "Blade Runner");
    }

    #[test]
    fn editions_are_read_from_the_words_after_a_title() {
        let edition = |rel: &str| edition(&Path::new("/media/movies").join(rel), true);
        for (name, expected) in [
            ("Film Name (2010) - Director's Cut.mkv", "Director's Cut"),
            ("Film.Name.2010.Directors.Cut.1080p.BluRay.x264-GRP.mkv", "Director's Cut"),
            ("Film Name (2010) [Extended Edition].mkv", "Extended Edition"),
            ("Film Name (2010) EXTENDED.mkv", "Extended"),
            ("Film Name (2010) Unrated Version.mkv", "Unrated Version"),
            ("Film Name 2010 Remastered 2160p.mkv", "Remastered"),
            ("Film Name (2010) 25th Anniversary Edition.mkv", "25th Anniversary Edition"),
            ("Film Name (2010) IMAX.mkv", "IMAX"),
            ("Film Name (2010) - Special Edition.mkv", "Special Edition"),
            ("Film Name (2010) Collectors Edition.mkv", "Collector's Edition"),
            ("Film Name (2010) Theatrical Cut - cd1.mkv", "Theatrical Cut"),
            ("Film Name (2010) Theatrical Cut/Film Name (2010).mkv", "Theatrical Cut"),
            ("Film Name (2010) {edition-Final Cut} Extended.mkv", "Final Cut"),
            // An edition of several of the words has them all.
            ("Film Name (2010) Unrated Director's Cut.mkv", "Unrated Director's Cut"),
            ("Film.Name.2010.EXTENDED.UNRATED.1080p.BluRay.x264-GRP.mkv", "Extended Unrated"),
            ("Film Name (2010) - 25th Anniversary Extended Edition.mkv", "25th Anniversary Extended Edition"),
            ("Film Name - Extended Director's Cut.mkv", "Extended Director's Cut"),
            ("Film Name (Unrated Extended Cut).mkv", "Unrated Extended Cut"),
            // Release details after it, whichever word they begin with.
            ("Film Name (2010) [Extended 1080p].mkv", "Extended"),
            ("Film Name (2010) [Director's Cut 1080p].mkv", "Director's Cut"),
            ("Film Name [Unrated 1080p].mkv", "Unrated"),
            ("Film Name - IMAX 2160p.mkv", "IMAX"),
            // Without a year: set apart, or after the folder's title.
            ("Film Name - Director's Cut.mkv", "Director's Cut"),
            ("Film Name (Uncut).mkv", "Uncut"),
            ("Film Name [Remastered].mkv", "Remastered"),
            ("Film Name Unrated 1080p.mkv", "Unrated"),
            ("Film Name/Film Name Directors Cut.mkv", "Director's Cut"),
        ] {
            assert_eq!(edition(name).as_deref(), Some(expected), "{name}");
            assert_eq!(movie(name).title, "Film Name", "{name}");
        }
        assert_eq!(movie("Film Name (2010) - Director's Cut.mkv").year, Some(2010));
    }

    #[test]
    fn titles_with_edition_words_in_them_are_no_editions() {
        let edition = |rel: &str| edition(&Path::new("/media/movies").join(rel), true);
        for (name, title) in [
            ("The Final Cut Society (2004).mkv", "The Final Cut Society"),
            ("Director's Cut (2027).mkv", "Director's Cut"),
            ("Extended Family Picnic (2019).mkv", "Extended Family Picnic"),
            ("Uncut Diamonds (2019)/Uncut Diamonds (2019).mkv", "Uncut Diamonds"),
            ("Special Edition Blues (2005).mkv", "Special Edition Blues"),
            ("Theatrical Lives - The Final Cut (1983).mkv", "Theatrical Lives - The Final Cut"),
            ("The Uncut Story.mkv", "The Uncut Story"),
            ("Remastered Hearts.mkv", "Remastered Hearts"),
            ("A Night - Special Guests.mkv", "A Night - Special Guests"),
            ("Final Cut/Final Cut.mkv", "Final Cut"),
            ("Ultimate Lanterns (2012).mkv", "Ultimate Lanterns"),
            ("Film Name [Extended Family 1080p].mkv", "Film Name"),
        ] {
            assert_eq!(edition(name), None, "{name}");
            assert_eq!(movie(name).title, title, "{name}");
        }
    }

    #[test]
    fn movies() {
        let m = movie("The Matrix (1999)/The Matrix (1999) - 1080p.mkv");
        assert_eq!((m.title.as_str(), m.year), ("The Matrix", Some(1999)));
        let m = movie("The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv");
        assert_eq!((m.title.as_str(), m.year), ("The Matrix", Some(1999)));
        let m = movie("Blade.Runner.2049.2017.2160p.UHD.mkv");
        assert_eq!((m.title.as_str(), m.year), ("Blade Runner 2049", Some(2017)));
        let m = movie("1917 (2019)/1917 (2019).mp4");
        assert_eq!((m.title.as_str(), m.year), ("1917", Some(2019)));
        // The file's year wins over a folder's that is out of date.
        let m = movie("300 (2006)/300 (2007) Remux-1080p.mkv");
        assert_eq!((m.title.as_str(), m.year), ("300", Some(2007)));
        let m = movie("Some Movie (2019)/01.mkv");
        assert_eq!((m.title.as_str(), m.year), ("Some Movie", Some(2019)));
        let m = movie("2001 A Space Odyssey (1968).mkv");
        assert_eq!((m.title.as_str(), m.year), ("2001 A Space Odyssey", Some(1968)));
        let m = movie("Some Movie/movie.mkv");
        assert_eq!((m.title.as_str(), m.year), ("Some Movie", None));
        let m = movie("Inception (2010) {tmdb-27205}/Inception.mkv");
        assert_eq!((m.title.as_str(), m.year, m.tmdb_id), ("Inception", Some(2010), Some(27205)));
        let m = movie("Heat (1995)/Heat (1995) - cd1.avi");
        assert_eq!((m.title.as_str(), m.year), ("Heat", Some(1995)));
        let m = movie("Amélie.2001.1080p.mkv");
        assert_eq!((m.title.as_str(), m.year), ("Amélie", Some(2001)));
        let m = movie("Untitled Home Video.mp4");
        assert_eq!((m.title.as_str(), m.year), ("Untitled Home Video", None));
        let m = movie("[Group] Movie Name 2020 [1080p].mkv");
        assert_eq!((m.title.as_str(), m.year), ("Movie Name", Some(2020)));
        // A final dot after a word belongs to the title; stray ones don't.
        let m = movie("Sherlock Jr. (1924)/Sherlock Jr. (1924).mkv");
        assert_eq!((m.title.as_str(), m.year), ("Sherlock Jr.", Some(1924)));
        let m = movie("Crazy, Stupid, Love. (2011).mkv");
        assert_eq!((m.title.as_str(), m.year), ("Crazy, Stupid, Love.", Some(2011)));
        let m = movie("Some Title . - (2005).mkv");
        assert_eq!((m.title.as_str(), m.year), ("Some Title", Some(2005)));
    }

    #[test]
    fn standard_episodes() {
        let e = episode("Breaking Bad (2008)/Season 01/Breaking Bad - S01E02 - Cat's in the Bag.mkv");
        assert_eq!(e.show, "Breaking Bad");
        assert_eq!(e.show_year, Some(2008));
        assert_eq!((e.season, e.episode), (1, Some(2)));
        assert_eq!(e.title.as_deref(), Some("Cat's in the Bag"));
        assert_eq!(e.season_dir, Some(PathBuf::from("/media/tv/Breaking Bad (2008)/Season 01")));
        assert_eq!(e.show_dir, Some(PathBuf::from("/media/tv/Breaking Bad (2008)")));

        let e = episode("The.Office.US.S03E10E11.720p.HDTV.x264.mkv");
        assert_eq!(e.show, "The Office US");
        assert_eq!((e.season, e.episode, e.episode_end), (3, Some(10), Some(11)));
        assert_eq!(e.title, None);

        let e = episode("Firefly/1x05 - Safe.avi");
        assert_eq!(e.show, "Firefly");
        assert_eq!((e.season, e.episode), (1, Some(5)));
        assert_eq!(e.title.as_deref(), Some("Safe"));
    }

    #[test]
    fn folder_based_episodes() {
        let e = episode("Show Name/Season 2/05 - Some Title.mkv");
        assert_eq!((e.show.as_str(), e.season, e.episode), ("Show Name", 2, Some(5)));
        assert_eq!(e.title.as_deref(), Some("Some Title"));

        let e = episode("Show Name/Specials/Show Name - S00E01.mkv");
        assert_eq!((e.season, e.episode), (0, Some(1)));

        let e = episode("Show Name/Season 3/Episode 7.mkv");
        assert_eq!((e.season, e.episode), (3, Some(7)));
    }

    #[test]
    fn date_and_anime_episodes() {
        let e = episode("The Daily Show/The Daily Show 2024-05-01.mkv");
        assert_eq!(e.show, "The Daily Show");
        assert_eq!(e.air_date.as_deref(), Some("2024-05-01"));
        assert_eq!((e.season, e.episode), (2024, Some(501)));

        let e = episode("Frieren/[SubsPlease] Sousou no Frieren - 12 (1080p) [ABCD1234].mkv");
        assert_eq!(e.show, "Frieren");
        assert_eq!((e.season, e.episode, e.title), (1, Some(12), None));
        let e = episode("Frieren/[Group] Frieren - 12v2 [GRP].mkv");
        assert_eq!((e.episode, e.title), (Some(12), None));
    }

    #[test]
    fn loosely_named_episodes() {
        // A number and a title in a season folder (no S01E04).
        let e = episode("Babylon 5/Season 1/04 - Infection.mkv");
        assert_eq!((e.show.as_str(), e.season, e.episode), ("Babylon 5", 1, Some(4)));
        assert_eq!(e.title.as_deref(), Some("Infection"));
        let e = episode("Babylon 5/Season 1/Babylon 5 - 04 - Infection.mkv");
        assert_eq!((e.season, e.episode, e.title.as_deref()), (1, Some(4), Some("Infection")));

        // Season and episode run together, in a season folder of that number.
        let e = episode("The West Wing/Season 1/101 - Pilot.mkv");
        assert_eq!((e.season, e.episode, e.title.as_deref()), (1, Some(1), Some("Pilot")));
        let e = episode("The West Wing/Season 2/222 - Two Cathedrals.mkv");
        assert_eq!((e.season, e.episode, e.title.as_deref()), (2, Some(22), Some("Two Cathedrals")));
        let e = episode("Long Show/Season 10/1012 - Late One.mkv");
        assert_eq!((e.season, e.episode), (10, Some(12)));
        // Not when the hundreds aren't the folder's season, nor without a
        // season folder, nor for anime's running numbers.
        let e = episode("Long Show/Season 2/104 - Title.mkv");
        assert_eq!((e.season, e.episode), (2, Some(104)));
        let e = episode("Long Show/104 - Title.mkv");
        assert_eq!((e.season, e.episode), (1, Some(104)));
        let e = episode("One Piece/Season 1/[Group] One Piece - 104 [1080p].mkv");
        assert_eq!((e.season, e.episode), (1, Some(104)));

        // Spelled out.
        let e = episode("Voyager/Season 3/Season 3 Episode 26 - Scorpion.mkv");
        assert_eq!((e.season, e.episode, e.title.as_deref()), (3, Some(26), Some("Scorpion")));
        let e = episode("Voyager/Voyager season 4 ep 1.mkv");
        assert_eq!((e.season, e.episode), (4, Some(1)));
    }

    #[test]
    fn the_shows_name_then_a_number() {
        // Anime named without ` - `, tags in parentheses.
        let e = episode("Monster (2004)/(Group) Monster 01v2(x264)(53CFA614).mkv");
        assert_eq!((e.show.as_str(), e.season, e.episode, e.title), ("Monster", 1, Some(1), None));
        let e = episode("Monster (2004)/Monster 05 - The Night of the Execution.mkv");
        assert_eq!((e.episode, e.title.as_deref()), (Some(5), Some("The Night of the Execution")));
        // Only the show's own name, and never a year.
        let e = episode("Monster (2004)/Pluto 05.mkv");
        assert_eq!(e.episode, None);
        let e = episode("Monster (2004)/Monster 2004.mkv");
        assert_eq!(e.episode, None);
        // A number that is part of the show's name isn't an episode's.
        let e = episode("Babylon 5 (1994)/Season 1/Babylon 5.mkv");
        assert_eq!(e.episode, None);
    }

    #[test]
    fn release_tags_after_a_number_are_no_title() {
        let e =
            episode("Star Blazers 2199 (2012)/[DB]Uchuu Senkan Yamato 2199_-_01_(Dual Audio_10bit_BD1080p_x265).mkv");
        assert_eq!((e.episode, e.title), (Some(1), None));
    }

    #[test]
    fn episodes_without_a_number_keep_their_title() {
        let e = episode("Babylon 5/Season 1/Infection.mkv");
        assert_eq!((e.season, e.episode, e.title.as_deref()), (1, None, Some("Infection")));
        let e = episode("Babylon 5/Season 1/Babylon 5 - The Parliament of Dreams.mkv");
        assert_eq!((e.episode, e.title.as_deref()), (None, Some("The Parliament of Dreams")));
        let e = episode("Babylon 5/Season 1/Babylon.5.Mind.War.720p.mkv");
        assert_eq!((e.episode, e.title.as_deref()), (None, Some("Mind War")));
        // A title that starts like the show's name keeps it.
        let e = episode("Lost/Season 1/Lostboys.mkv");
        assert_eq!(e.title.as_deref(), Some("Lostboys"));
        // The show's name with other punctuation is still the show's name.
        let e = episode("Full Metal Panic! (2002)/Season 4/[Judas] Full Metal Panic S4 - Special 01.mkv");
        assert_eq!((e.season, e.episode, e.title.as_deref()), (4, None, Some("S4 - Special 01")));
    }

    #[test]
    fn resolution_is_not_an_episode() {
        let e = episode("Show/Show S01E01 1920x1080.mkv");
        assert_eq!((e.season, e.episode), (1, Some(1)));
    }

    #[test]
    fn season_dirs() {
        assert_eq!(season_from_dir("Season 01"), Some(1));
        assert_eq!(season_from_dir("season.12"), Some(12));
        assert_eq!(season_from_dir("S03"), Some(3));
        assert_eq!(season_from_dir("Specials"), Some(0));
        assert_eq!(season_from_dir("Staffel 4"), Some(4));
        assert_eq!(season_from_dir("Sherlock"), None);
    }

    #[test]
    fn audiobooks_from_folders() {
        let root = Path::new("/books");
        let none = BTreeMap::new();

        // Author/Series/Book, a folder of files.
        let b = parse_audiobook(
            &root.join("Author A/Series X/Book 2 - The Second Book (2007)/03 - Chapter Three.mp3"),
            root,
            &none,
        );
        assert_eq!(b.title, "The Second Book");
        assert_eq!(b.authors, ["Author A"]);
        assert_eq!((b.series.as_deref(), b.series_index), (Some("Series X"), Some(2.0)));
        assert_eq!(b.year, Some(2007));
        assert_eq!((b.part, b.part_title.as_str()), (Some(3), "Chapter Three"));
        assert_eq!(b.book_dir, Some(root.join("Author A/Series X/Book 2 - The Second Book (2007)")));
        assert_eq!(b.book_key, path_key(&root.join("Author A/Series X/Book 2 - The Second Book (2007)")));

        // A novella between two books, and a narrator in braces.
        let b = parse_audiobook(&root.join("Author A/Series X/2.5 - Novella {Narrator B}/01.mp3"), root, &none);
        assert_eq!((b.title.as_str(), b.series_index), ("Novella", Some(2.5)));
        assert_eq!(b.narrators, ["Narrator B"]);
        assert_eq!(b.part_title, "01");

        // Discs inside a book are its parts, numbered by disc.
        let b = parse_audiobook(&root.join("Author A/Book One/CD 2/04 Track.mp3"), root, &none);
        assert_eq!((b.title.as_str(), b.disc, b.part), ("Book One", Some(2), Some(4)));
        assert_eq!(b.book_dir, Some(root.join("Author A/Book One")));
        assert_eq!(b.book_key, path_key(&root.join("Author A/Book One")));
        assert_eq!(b.series, None);

        // A number that is the title's own, not a place in a series.
        let b = parse_audiobook(&root.join("Author C/3 Days Later/01.mp3"), root, &none);
        assert_eq!((b.title.as_str(), b.series_index), ("3 Days Later", None));
        let b = parse_audiobook(&root.join("Author C/1234/01.mp3"), root, &none);
        assert_eq!(b.title, "1234");

        // An `.m4b` is a whole book, also beside other books by its author.
        let b = parse_audiobook(&root.join("Author B/Book Three.m4b"), root, &none);
        assert_eq!((b.title.as_str(), b.part_title.as_str()), ("Book Three", "Book Three"));
        assert_eq!(b.authors, ["Author B"]);
        assert_eq!(b.book_key, path_key(&root.join("Author B/Book Three.m4b")));
        assert_eq!(b.book_dir, None, "the author's folder isn't the book's");

        // A file alone in the library's folder, by no one known.
        let b = parse_audiobook(&root.join("Loose Book.mp3"), root, &none);
        assert_eq!(b.book_key, path_key(&root.join("Loose Book.mp3")));
        assert_eq!(b.book_dir, None);
        assert_eq!((b.title.as_str(), b.authors.as_slice()), ("Loose Book", ["Unknown Author".to_owned()].as_slice()));
    }

    fn tags(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    /// Which book a file is part of, and the folders that name it, as
    /// Audiobookshelf has them, from the file alone.
    #[test]
    fn audiobooks_are_grouped_by_their_files_alone() {
        let root = Path::new("/books");
        let book = |rel: &str, pairs: &[(&str, &str)]| parse_audiobook(&root.join(rel), root, &tags(pairs));
        let key = |rel: &str| path_key(&root.join(rel));
        let album_key = |rel: &str, album: &str| format!("{}\n{album}", key(rel));

        // `Author/Book/Book.m4b`: the folder is the book's, its author above
        // it, and no series.
        let b = book("Author A/Book One/Book One.m4b", &[("album", "Book One")]);
        assert_eq!(b.book_key, album_key("Author A/Book One", "book one"));
        assert_eq!(b.book_dir, Some(root.join("Author A/Book One")));
        assert_eq!(
            (b.title.as_str(), b.authors.as_slice(), b.series),
            ("Book One", ["Author A".to_owned()].as_slice(), None)
        );
        // Without an album, the file is the book, named by its file.
        let b = book("Author A/Book One/Rip Name.m4b", &[]);
        assert_eq!((b.book_key, b.title.as_str()), (key("Author A/Book One/Rip Name.m4b"), "Rip Name"));
        assert_eq!(b.book_dir, Some(root.join("Author A/Book One")));
        assert_eq!(b.authors, ["Author A"]);
        // `Author/Series/Book/Book.m4b`.
        let b = book("Author A/Series X/Book Two/Book Two.m4b", &[]);
        assert_eq!((b.title.as_str(), b.series.as_deref()), ("Book Two", Some("Series X")));
        assert_eq!(b.authors, ["Author A"]);

        // A book in two `.m4b` files is one book, its parts named by their
        // titles or files.
        let one = book("Author B/Split Book/Split Book 1.m4b", &[("album", "Split Book"), ("title", "Split Book")]);
        let two = book("Author B/Split Book/Split Book 2.m4b", &[("album", " split  BOOK "), ("title", "The End")]);
        assert_eq!(one.book_key, two.book_key);
        assert_eq!(one.book_key, album_key("Author B/Split Book", "split book"));
        assert_eq!((one.part_title.as_str(), two.part_title.as_str()), ("Split Book 1", "The End"));
        let one = book("Author B/Split Book/01 - Split Book.m4b", &[("album", "Split Book")]);
        assert_eq!((one.part, one.part_title.as_str()), (Some(1), "Split Book"));

        // Books side by side in their author's folder, or their series',
        // are each their own.
        let four = book("Author C/Book Four.m4b", &[("album", "Book Four")]);
        let five = book("Author C/Book Five.m4b", &[("album", "Book Five")]);
        assert_ne!(four.book_key, five.book_key);
        assert_eq!((four.book_dir, four.authors.as_slice()), (None, ["Author C".to_owned()].as_slice()));
        let four = book("Author C/Series Y/Book Four.m4b", &[("album", "Book Four")]);
        let five = book("Author C/Series Y/Book Five.m4b", &[("album", "Book Five")]);
        assert_ne!(four.book_key, five.book_key);
        assert_eq!((four.title.as_str(), five.title.as_str()), ("Book Four", "Book Five"));
        assert_eq!(four.authors, ["Author C"]);
        assert_eq!(four.series, None, "a series is never read from two folders");
        let four = book("Author C/Series Y/Book Four.m4b", &[]);
        assert_eq!((four.book_key, four.title.as_str()), (key("Author C/Series Y/Book Four.m4b"), "Book Four"));

        // A folder at the top named as its `.m4b` (or its album) is the
        // book's, not an author's.
        let b = book("Book Seven/Book Seven.m4b", &[]);
        assert_eq!(
            (b.book_dir, b.authors.as_slice()),
            (Some(root.join("Book Seven")), ["Unknown Author".to_owned()].as_slice())
        );
        let b = book("Book Seven (2001)/Rip Name.m4b", &[("album", "Book Seven")]);
        assert_eq!((b.book_dir, b.title.as_str()), (Some(root.join("Book Seven (2001)")), "Book Seven"));
        assert_eq!((b.year, b.authors.as_slice()), (Some(2001), ["Unknown Author".to_owned()].as_slice()));

        // Untagged `.m4b` files on the discs of a book are its parts, and the
        // folder above the discs is the book's, at the top of the library too.
        let one = book("Author D/Book Eight/CD 1/01.m4b", &[]);
        let two = book("Author D/Book Eight/CD 2/01.m4b", &[]);
        assert_eq!(
            (one.book_key.as_str(), two.book_key.as_str()),
            (key("Author D/Book Eight").as_str(), key("Author D/Book Eight").as_str())
        );
        assert_eq!((one.title.as_str(), one.disc, two.disc), ("Book Eight", Some(1), Some(2)));
        let b = book("Book Eight/CD 1/Rip Name.m4b", &[]);
        assert_eq!(
            (b.book_dir, b.title.as_str(), b.authors.as_slice()),
            (Some(root.join("Book Eight")), "Book Eight", ["Unknown Author".to_owned()].as_slice())
        );

        // Files at the top of a book's folder at the top: one book, by no
        // one known.
        let one = book("Book Six/01.mp3", &[]);
        let two = book("Book Six/02.mp3", &[]);
        assert_eq!((one.book_key.as_str(), one.title.as_str()), (key("Book Six").as_str(), "Book Six"));
        assert_eq!(one.book_key, two.book_key);
        assert_eq!(one.authors, ["Unknown Author"]);
        // Files in the library's folder are books of their own.
        assert_ne!(book("Loose One.mp3", &[]).book_key, book("Loose Two.mp3", &[]).book_key);
        // Other files side by side are parts of their folder's book, as
        // with Audiobookshelf.
        let a = book("Author F/Book A.mp3", &[]);
        let b = book("Author F/Book B.mp3", &[]);
        assert_eq!((a.book_key.as_str(), a.title.as_str()), (b.book_key.as_str(), "Author F"));

        // Discs inside a book's folder are the book's.
        let one = book("Author D/Book Eight/CD 1/01 - Start.mp3", &[]);
        let two = book("Author D/Book Eight/Disc 2/01 - End.mp3", &[]);
        assert_eq!(one.book_key, key("Author D/Book Eight"));
        assert_eq!(one.book_key, two.book_key);
        assert_eq!((one.disc, two.disc, one.part), (Some(1), Some(2), Some(1)));
        assert_eq!((one.authors.as_slice(), one.series), (["Author D".to_owned()].as_slice(), None));
    }

    #[test]
    fn series_from_the_tags_tools_write() {
        let series = |pairs: &[(&str, &str)]| {
            let b = parse_audiobook(Path::new("/books/Author/Book/01.mp3"), Path::new("/books"), &tags(pairs));
            (b.series, b.series_index)
        };
        let some = |name: &str, index: Option<f64>| (Some(name.to_owned()), index);
        // Libation's and tone's place in a series, after the others.
        assert_eq!(series(&[("series", "Series Y"), ("part", "3")]), some("Series Y", Some(3.0)));
        assert_eq!(series(&[("series", "Series Y"), ("series-part", "2"), ("part", "3")]), some("Series Y", Some(2.0)));
        assert_eq!(series(&[("part", "3")]), (None, None), "a place in no series is nothing");
        // Audiobookshelf's `grouping`, when no other tag names a series.
        assert_eq!(series(&[("grouping", "Series Z #2")]), some("Series Z", Some(2.0)));
        assert_eq!(series(&[("grouping", "Series Z, Book 2.5")]), some("Series Z", Some(2.5)));
        assert_eq!(series(&[("grouping", "Series Z #2"), ("series-part", "4")]), some("Series Z", Some(4.0)));
        assert_eq!(series(&[("grouping", "Series Z #2"), ("series", "Series Y")]), some("Series Y", None));
        // Anything else there is no series.
        for grouping in ["Fantasy", "Series Z", "Series A #1; Series B #2", "#2", "Series Z #two"] {
            assert_eq!(series(&[("grouping", grouping)]), (None, None), "{grouping}");
        }
        let b = parse_audiobook(
            Path::new("/books/Author/Series X/Book/01.mp3"),
            Path::new("/books"),
            &tags(&[("grouping", "Fantasy")]),
        );
        assert_eq!(b.series.as_deref(), Some("Series X"), "the folders still say");
    }

    /// A crafted file's tags are cut before they are read, at a
    /// character's boundary, as an NFO file's are.
    #[test]
    fn audiobook_tags_are_bounded() {
        let long = |c: &str| c.repeat(1_000_000);
        // Names apart, each different: deduplicated one by one, these took
        // minutes.
        let many: String = (0..200_000).map(|n| format!("Author {n}; ")).collect();
        let narrators = format!("{};Narrator B", long("n"));
        let (album, title, series, genre, summary) = (long("é"), long("t"), long("s"), long("g"), long("d"));
        let pairs = [
            ("artist", many.as_str()),
            ("composer", narrators.as_str()),
            ("album", album.as_str()),
            ("title", title.as_str()),
            ("series", series.as_str()),
            ("genre", genre.as_str()),
            ("description", summary.as_str()),
        ];
        let started = std::time::Instant::now();
        let b = parse_audiobook(Path::new("/books/Book/01.m4b"), Path::new("/books"), &tags(&pairs));
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
        assert_eq!(b.authors.len(), 50);
        assert_eq!((b.authors[0].as_str(), b.authors[49].as_str()), ("Author 0", "Author 49"));
        assert_eq!(b.narrators, ["n".repeat(500)], "names after the first 2,000 characters are left out");
        assert_eq!(b.title, "é".repeat(500));
        assert_eq!(b.part_title, "t".repeat(500));
        assert_eq!(b.series.as_deref(), Some("s".repeat(500).as_str()));
        assert_eq!(b.genre.as_deref(), Some("g".repeat(500).as_str()));
        assert_eq!(b.summary.as_deref(), Some("d".repeat(20_000).as_str()));
        assert!(b.book_key.len() < 2_000, "{}", b.book_key.len());

        // A place in a series is a number from 0 to 10,000.
        let index = |value: &str| {
            let tags = tags(&[("series", "Series Y"), ("series-part", value)]);
            parse_audiobook(Path::new("/books/Book/01.mp3"), Path::new("/books"), &tags).series_index
        };
        assert_eq!((index("0"), index("2.5"), index("10000")), (Some(0.0), Some(2.5), Some(10_000.0)));
        for value in ["NaN", "inf", "-1", "10000.5", "1e308"] {
            assert_eq!(index(value), None, "{value}");
        }
    }

    #[test]
    fn a_summary_is_a_description_before_a_comment() {
        let summary = |pairs: &[(&str, &str)]| {
            parse_audiobook(Path::new("/books/Book/01.mp3"), Path::new("/books"), &tags(pairs)).summary
        };
        assert_eq!(summary(&[("comment", "A note."), ("description", "About it.")]).as_deref(), Some("About it."));
        assert_eq!(summary(&[("comment", "A note."), ("synopsis", "About it.")]).as_deref(), Some("About it."));
        assert_eq!(summary(&[("comment", "A note."), ("description", " ")]).as_deref(), Some("A note."));
        assert_eq!(summary(&[]), None);
    }

    #[test]
    fn a_title_of_numbers_keeps_them() {
        assert_eq!(book_folder("10.20.30"), ("10.20.30".to_owned(), None, None, None));
        assert_eq!(book_folder("Book 1 - 10.20.30").0, "10.20.30");
        // A place in a series still comes before a title.
        assert_eq!(book_folder("01.Title"), ("Title".to_owned(), Some(1.0), None, None));
        assert_eq!(book_folder("1_-_Title"), ("Title".to_owned(), Some(1.0), None, None));
        assert_eq!(book_folder("2.5 - Novella"), ("Novella".to_owned(), Some(2.5), None, None));
        assert_eq!(book_folder("Book 2 - 2048"), ("2048".to_owned(), Some(2.0), None, None));
    }

    #[test]
    fn parts_sort_by_their_files_numbers() {
        let mut names = ["Part 10", "part 2", "Part 1", "Part 1b", "Intro", "Part 01a"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, ["Intro", "Part 1", "Part 01a", "Part 1b", "part 2", "Part 10"]);
        assert_eq!(natural_key("Disc 0 - 7"), "disc 0000000000 - 0000000007");
        let b = parse_audiobook(Path::new("/books/Book/Chapter 9.mp3"), Path::new("/books"), &BTreeMap::new());
        assert_eq!((b.part, b.part_sort.as_str()), (None, "chapter 0000000009"));
    }

    #[test]
    fn audiobooks_from_tags() {
        let root = Path::new("/books");
        let tags: BTreeMap<String, String> = [
            ("album", "Book One"),
            ("artist", "Author A; Author B"),
            ("composer", "Narrator C"),
            ("title", "Book One"),
            ("date", "2006"),
            ("series", "Series X"),
            ("series-part", "41"),
            ("description", "A summary of the book."),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        let b = parse_audiobook(&root.join("Author A/whatever.m4b"), root, &tags);
        assert_eq!(b.title, "Book One");
        assert_eq!(b.authors, ["Author A", "Author B"]);
        assert_eq!(b.narrators, ["Narrator C"]);
        assert_eq!((b.series.as_deref(), b.series_index), (Some("Series X"), Some(41.0)));
        assert_eq!(b.year, Some(2006));
        assert_eq!(b.summary.as_deref(), Some("A summary of the book."));
        // A part titled as its book is named by its file.
        let b = parse_audiobook(&root.join("Author A/Book One/02 - Wednesday.mp3"), root, &tags);
        assert_eq!((b.part_title.as_str(), b.part), ("Wednesday", Some(2)));
        // A series index alone, without a series, is nothing.
        let mut tags = tags;
        tags.remove("series");
        let b = parse_audiobook(&root.join("Author A/Book One.m4b"), root, &tags);
        assert_eq!((b.series, b.series_index), (None, None));
    }

    #[test]
    fn tracks_from_tags_and_folders() {
        let root = Path::new("/music");
        let mut tags = BTreeMap::new();
        tags.insert("artist".to_owned(), "Daft Punk".to_owned());
        tags.insert("album".to_owned(), "Discovery".to_owned());
        tags.insert("title".to_owned(), "One More Time".to_owned());
        tags.insert("track".to_owned(), "1/14".to_owned());
        tags.insert("date".to_owned(), "2001-03-12".to_owned());
        let t = parse_track(&root.join("Daft Punk/Discovery/01 - One More Time.flac"), root, &tags);
        assert_eq!(t.artist, "Daft Punk");
        assert_eq!(t.album, "Discovery");
        assert_eq!(t.title, "One More Time");
        assert_eq!(t.track, Some(1));
        assert_eq!(t.year, Some(2001));

        let t = parse_track(
            &root.join("Radiohead/OK Computer (1997)/1-03 Subterranean Homesick Alien.mp3"),
            root,
            &BTreeMap::new(),
        );
        assert_eq!(t.artist, "Radiohead");
        assert_eq!(t.album, "OK Computer");
        assert_eq!(t.year, Some(1997));
        assert_eq!((t.disc, t.track), (Some(1), Some(3)));
        assert_eq!(t.title, "Subterranean Homesick Alien");

        let t = parse_track(&root.join("Loose/song.mp3"), root, &BTreeMap::new());
        assert_eq!(t.artist, "Unknown Artist");
        assert_eq!(t.album, "Loose");
        assert_eq!(t.title, "song");
    }
}
