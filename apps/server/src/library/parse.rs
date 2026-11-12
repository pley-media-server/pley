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
    let max = max_year();
    let mut chosen = None;
    for caps in YEAR.captures_iter(&s) {
        let m = caps.get(1).unwrap();
        let year: i32 = m.as_str().parse().unwrap_or(0);
        if m.start() > 0 && (1900..=max).contains(&year) {
            chosen = Some((m.start(), year));
        }
    }
    match chosen {
        Some((start, year)) => {
            let title = clean_title(&s[..start]);
            if title.is_empty() { (clean_title(cut_at_junk(&s)), Some(year)) } else { (title, Some(year)) }
        }
        None => (clean_title(cut_at_junk(&s)), None),
    }
}

fn extract_ids(s: &str) -> (Option<i64>, Option<String>) {
    let tmdb = TMDB_ID.captures(s).and_then(|c| c[1].parse().ok());
    let imdb = IMDB_ID.captures(s).map(|c| c[1].to_owned());
    (tmdb, imdb)
}

/// The edition a movie file is tagged with (`{edition-Director's Cut}`), in
/// its name or its folder's. Editions of a film are versions of one movie.
pub fn edition(path: &Path) -> Option<String> {
    let tagged = |name: &str| EDITION.captures(name).map(|c| c[1].trim().to_owned()).filter(|e| !e.is_empty());
    tagged(&file_stem(path)).or_else(|| path.parent()?.file_name().and_then(|f| tagged(&f.to_string_lossy())))
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

    let from_file = title_and_year(&stem);
    let from_folder = folder.as_deref().map(title_and_year);
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
        let edition = |rel: &str| edition(&Path::new("/media/movies").join(rel));
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
