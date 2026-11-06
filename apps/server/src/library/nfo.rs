//! NFO files: the metadata Kodi, Jellyfin, Emby, Radarr, Sonarr,
//! tinyMediaManager and MediaElch keep next to a title, as XML with one of
//! the roots `movie`, `tvshow`, `season`, `episodedetails`, `artist` or
//! `album`, or as a bare link to the title's page (Kodi's URL NFO).
//!
//! ```xml
//! <movie>
//!   <title>The Matrix</title><year>1999</year><plot>…</plot><mpaa>Rated R</mpaa>
//!   <uniqueid type="tmdb" default="true">603</uniqueid><uniqueid type="imdb">tt0133093</uniqueid>
//!   <genre>Action</genre><genre>Science Fiction</genre><studio>Warner Bros. Pictures</studio>
//!   <ratings><rating name="imdb" max="10" default="true"><value>8.7</value></rating></ratings>
//! </movie>
//! ```
//!
//! NFO files come from wherever a library's files came from, so what is read
//! is bounded and checked: lengths, counts, the shape of ids. Only XML's own
//! entities are read and no DTD is, and `<thumb>` and `<fanart>` addresses
//! are never read, let alone fetched.

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// The most of an NFO file read.
pub const MAX_BYTES: u64 = 1 << 20;
/// The most characters of a title and other short texts.
const MAX_SHORT: usize = 500;
/// The most characters of a summary.
const MAX_SUMMARY: usize = 20_000;
const MAX_GENRES: usize = 50;
/// The most entries of a file: a multi-episode file holds one an episode.
const MAX_ENTRIES: usize = 100;
/// The most text gathered for one element, in bytes: a summary's worth, in
/// any script, before it is cut to its length.
const MAX_TEXT_BYTES: usize = MAX_SUMMARY * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NfoKind {
    Movie,
    TvShow,
    Season,
    Episode,
    Artist,
    Album,
    /// A file holding only a link to the title's page.
    Link,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NfoIds {
    pub tmdb: Option<i64>,
    pub imdb: Option<String>,
    pub tvdb: Option<i64>,
    pub tvmaze: Option<i64>,
    pub musicbrainz: Option<String>,
}

impl NfoIds {
    fn is_empty(&self) -> bool {
        *self == NfoIds::default()
    }
}

/// A content rating and the country an NFO file names for it (`GB:15`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NfoRating {
    /// ISO 3166-1 alpha-2, as the file writes it ("USA" is "US").
    pub country: Option<String>,
    pub rating: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NfoFields {
    pub title: Option<String>,
    pub original_title: Option<String>,
    pub sort_title: Option<String>,
    pub year: Option<i64>,
    /// `YYYY-MM-DD`.
    pub release_date: Option<String>,
    pub summary: Option<String>,
    pub tagline: Option<String>,
    /// The United States' rating, else the first (see [`content_rating`]).
    pub content_rating: Option<String>,
    /// Every country's rating, as the file gives them, for the metadata
    /// region's to be taken ([`NfoFields::rating_in`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_ratings: Vec<NfoRating>,
    pub genres: Vec<String>,
    pub studio: Option<String>,
    /// From 0 to 10.
    pub rating: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Nfo {
    pub kind: NfoKind,
    pub ids: NfoIds,
    pub fields: NfoFields,
    /// Jellyfin's `<lockdata>`: the item's metadata isn't refreshed online.
    pub locked: bool,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    /// For a [`NfoKind::Link`], the kind of title its link names, when it
    /// says: `Movie` for TMDB's `/movie/`, `TvShow` for TMDB's `/tv/`,
    /// TheTVDB's and TVmaze's shows, `None` for an IMDb title. TMDB numbers
    /// movies and shows apart, so a movie mustn't take a show's link.
    pub links_to: Option<NfoKind>,
}

/// Every entry of an NFO file: several for a multi-episode file, none for
/// one that isn't an NFO Pley reads.
pub fn parse(bytes: &[u8]) -> Vec<Nfo> {
    let Some(text) = decode(bytes) else { return Vec::new() };
    let (entries, outside) = read_xml(&text);
    if entries.is_empty() {
        return link_only(&text).into_iter().collect();
    }
    // Kodi's combined NFO: the XML, then a link saying which title it is.
    let links = Links::find(&outside);
    entries.into_iter().map(|entry| entry.finish(&links)).collect()
}

/// Reads and parses the file at `path`, opened with
/// [`super::walk::open_of_its_kind`] (a link must lead to a file of its
/// kind), and at most [`MAX_BYTES`] of it: `None` when it can't be opened or
/// is bigger.
pub fn read(path: &Path) -> Option<Vec<Nfo>> {
    let (file, meta) = super::walk::open_of_its_kind(path)?;
    if meta.len() > MAX_BYTES {
        tracing::debug!("NFO file {} skipped: {} bytes", path.display(), meta.len());
        return None;
    }
    // One more byte than allowed tells a file that has grown since.
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        tracing::debug!("NFO file {} skipped: it grew past {MAX_BYTES} bytes", path.display());
        return None;
    }
    Some(parse(&bytes))
}

/// The text of a file: UTF-8, or UTF-16 with its byte order mark, or else
/// Latin-1, which older Windows tools wrote (Windows-1252's quotes, in
/// 0x80–0x9F, come out as control characters and are dropped later).
/// `None` for a binary file.
fn decode(bytes: &[u8]) -> Option<String> {
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| {
        let units = rest.as_chunks::<2>().0.iter().map(|&pair| unit(pair));
        char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect::<String>()
    };
    let text = if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        String::from_utf8_lossy(rest).into_owned()
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        utf16(rest, u16::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        utf16(rest, u16::from_be_bytes)
    } else {
        match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
        }
    };
    (!text.contains('\0')).then_some(text)
}

/// The entries of `text` whose root Pley reads, and the text outside any
/// element (where a combined NFO's link is). Roots may follow one another,
/// as in Kodi's multi-episode files, though that isn't well-formed XML. An
/// entry is kept only once its root is closed: on an error, or at the end
/// of a cut-off file, the one being read is dropped.
fn read_xml(text: &str) -> (Vec<Entry>, String) {
    let mut reader = Reader::from_str(text);
    // A bare `&` (`Tom & Jerry`) is a common mistake of hand-written files,
    // not a reason to drop the whole file.
    reader.config_mut().allow_dangling_amp = true;
    let mut entries = Vec::new();
    let mut outside = String::new();
    let mut entry: Option<Entry> = None;
    // How many elements are open, the root included, and below the root the
    // names of those open and the text of each.
    let mut depth = 0usize;
    let mut path: Vec<String> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    while let Ok(event) = reader.read_event() {
        let text = match event {
            Event::Eof => break,
            Event::Start(e) => {
                depth += 1;
                let name = name_of(&e);
                if depth == 1 {
                    entry = root_kind(&name).filter(|_| entries.len() < MAX_ENTRIES).map(Entry::new);
                    path.clear();
                    texts.clear();
                } else if let Some(entry) = entry.as_mut() {
                    path.push(name);
                    texts.push(String::new());
                    entry.open(&path, &e);
                }
                continue;
            }
            Event::Empty(e) => {
                // `<movie/>`: an entry saying nothing, still an NFO.
                if depth == 0
                    && let Some(kind) = root_kind(&name_of(&e)).filter(|_| entries.len() < MAX_ENTRIES)
                {
                    entries.push(Entry::new(kind));
                }
                continue;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    entries.extend(entry.take());
                } else if let Some(entry) = entry.as_mut()
                    && let (Some(text), false) = (texts.pop(), path.is_empty())
                {
                    entry.close(&path, &text);
                    path.pop();
                    // Markup within a field (`<plot>An <i>odd</i> one</plot>`)
                    // is read as its text.
                    if path.len() == 1
                        && let Some(field) = texts.last_mut()
                    {
                        push_bounded(field, &text);
                    }
                }
                continue;
            }
            Event::Text(t) => t.xml10_content().map(|t| t.into_owned()).unwrap_or_default(),
            Event::CData(t) => String::from_utf8_lossy(&t).into_owned(),
            // `&amp;`, `&#233;`: XML's own; others (HTML's `&nbsp;`, or one a
            // DTD names) are left out.
            Event::GeneralRef(r) => match r.resolve_char_ref() {
                Ok(Some(c)) => c.to_string(),
                _ => r.decode().ok().as_deref().and_then(resolve_predefined_entity).unwrap_or_default().to_owned(),
            },
            _ => continue,
        };
        if depth == 0 {
            push_bounded(&mut outside, &text);
        } else if let Some(last) = texts.last_mut() {
            push_bounded(last, &text);
        }
    }
    (entries, outside)
}

fn push_bounded(into: &mut String, text: &str) {
    if into.len() < MAX_TEXT_BYTES {
        into.push_str(text);
    }
}

fn name_of(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase()
}

fn attribute(e: &BytesStart, name: &[u8]) -> Option<String> {
    let mut attributes = e.attributes();
    // The first of a name counts. Looking for repeats takes time quadratic
    // in the number of attributes, which a file can make large.
    attributes.with_checks(false);
    attributes
        .flatten()
        .find(|a| a.key.as_ref().eq_ignore_ascii_case(name))
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.trim().to_owned()))
        .filter(|v| !v.is_empty())
}

fn root_kind(name: &str) -> Option<NfoKind> {
    match name {
        "movie" => Some(NfoKind::Movie),
        "tvshow" => Some(NfoKind::TvShow),
        "season" => Some(NfoKind::Season),
        "episodedetails" => Some(NfoKind::Episode),
        "artist" => Some(NfoKind::Artist),
        "album" => Some(NfoKind::Album),
        _ => None,
    }
}

/// An entry as it is read: its elements' texts as they are, checked and
/// cut to length when it ends.
#[derive(Default)]
struct Entry {
    kind: Option<NfoKind>,
    /// Ids that say whose they are: `<uniqueid type="…">`, `<tmdbid>`…
    typed: NfoIds,
    /// `<id>`, whose source depends on the writer.
    id: Option<String>,
    musicbrainz_artist: Option<String>,
    musicbrainz_release_group: Option<String>,
    /// The `type` of the `<uniqueid>` being read.
    unique_type: Option<String>,
    title: Option<String>,
    original_title: Option<String>,
    sort_title: Option<String>,
    year: Option<String>,
    premiered: Option<String>,
    release_date: Option<String>,
    aired: Option<String>,
    plot: Option<String>,
    outline: Option<String>,
    tagline: Option<String>,
    mpaa: Option<String>,
    certification: Option<String>,
    genres: Vec<String>,
    studio: Option<String>,
    /// `<ratings>`' entries: the value on 0–10, and whether it's the default.
    ratings: Vec<(f64, bool)>,
    /// The `<rating>` of `<ratings>` being read: its `max`, its `default`
    /// and its value.
    rating_entry: Option<(Option<f64>, bool, Option<f64>)>,
    rating: Option<String>,
    lockdata: Option<String>,
    season: Option<String>,
    season_number: Option<String>,
    episode: Option<String>,
}

impl Entry {
    fn new(kind: NfoKind) -> Self {
        Entry { kind: Some(kind), ..Default::default() }
    }

    /// An element opened below the root: `path` names it and those it is in.
    fn open(&mut self, path: &[String], e: &BytesStart) {
        match path {
            [name] if name == "uniqueid" => self.unique_type = attribute(e, b"type").map(|t| t.to_ascii_lowercase()),
            [ratings, rating] if ratings == "ratings" && rating == "rating" => {
                let max = attribute(e, b"max").and_then(|max| number(&max));
                let default = attribute(e, b"default").is_some_and(|d| d.eq_ignore_ascii_case("true"));
                self.rating_entry = Some((max, default, None));
            }
            _ => {}
        }
    }

    /// An element below the root closed, with its text.
    fn close(&mut self, path: &[String], text: &str) {
        match path {
            [name] => self.field(name, text),
            [ratings, rating, value] if ratings == "ratings" && rating == "rating" && value == "value" => {
                if let Some(entry) = self.rating_entry.as_mut() {
                    entry.2 = entry.2.or_else(|| number(text));
                }
            }
            [ratings, rating] if ratings == "ratings" && rating == "rating" => {
                if let Some((max, default, Some(value))) = self.rating_entry.take()
                    && let Some(value) = out_of_ten(value, max)
                {
                    self.ratings.push((value, default));
                }
            }
            _ => {}
        }
    }

    fn field(&mut self, name: &str, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let first = |slot: &mut Option<String>| {
            if slot.is_none() {
                *slot = Some(text.to_owned());
            }
        };
        match name {
            "title" => first(&mut self.title),
            "originaltitle" => first(&mut self.original_title),
            "sorttitle" => first(&mut self.sort_title),
            "year" => first(&mut self.year),
            "premiered" => first(&mut self.premiered),
            "releasedate" => first(&mut self.release_date),
            "aired" => first(&mut self.aired),
            "plot" => first(&mut self.plot),
            "outline" => first(&mut self.outline),
            "tagline" => first(&mut self.tagline),
            "mpaa" => first(&mut self.mpaa),
            "certification" => first(&mut self.certification),
            "studio" => first(&mut self.studio),
            "rating" => first(&mut self.rating),
            "lockdata" => first(&mut self.lockdata),
            "season" => first(&mut self.season),
            "seasonnumber" => first(&mut self.season_number),
            "episode" => first(&mut self.episode),
            "id" => first(&mut self.id),
            "musicbrainzartistid" => first(&mut self.musicbrainz_artist),
            "musicbrainzreleasegroupid" => first(&mut self.musicbrainz_release_group),
            "genre" if self.genres.len() < MAX_GENRES => self.genres.push(text.to_owned()),
            "uniqueid" => match self.unique_type.take().as_deref() {
                Some("tmdb" | "themoviedb") => fill(&mut self.typed.tmdb, positive(text)),
                Some("imdb") => fill(&mut self.typed.imdb, imdb(text)),
                Some("tvdb" | "thetvdb") => fill(&mut self.typed.tvdb, positive(text)),
                Some("tvmaze") => fill(&mut self.typed.tvmaze, positive(text)),
                _ => {}
            },
            "tmdbid" => fill(&mut self.typed.tmdb, positive(text)),
            "imdbid" => fill(&mut self.typed.imdb, imdb(text)),
            "tvdbid" => fill(&mut self.typed.tvdb, positive(text)),
            "tvmazeid" => fill(&mut self.typed.tvmaze, positive(text)),
            _ => {}
        }
    }

    fn finish(self, links: &Links) -> Nfo {
        let kind = self.kind.unwrap_or(NfoKind::Link);
        let mut ids = self.typed.clone();
        if let Some(id) = self.id.as_deref() {
            if let Some(imdb) = imdb(id) {
                fill(&mut ids.imdb, Some(imdb));
            } else if self.typed.is_empty() {
                // A number in `<id>` is the id of the source the writer
                // scraped with. Kodi writes the default `<uniqueid>`'s value
                // there, which is TMDB's for movies since Kodi 19 (its TMDB
                // scraper) and was TheTVDB's for shows and episodes before
                // it; Jellyfin reads a show's `<id>` as TheTVDB's, a movie's
                // as TMDB's. Writers that know better (Kodi 17 on, Jellyfin,
                // Radarr, Sonarr, tinyMediaManager) also write typed ids, and
                // then `<id>` only repeats one of them: a Kodi 19 show's is
                // TMDB's, so it is read only from a file without typed ids.
                match kind {
                    NfoKind::Movie => ids.tmdb = positive(id),
                    NfoKind::TvShow | NfoKind::Episode => ids.tvdb = positive(id),
                    _ => {}
                }
            }
        }
        ids.musicbrainz = match kind {
            NfoKind::Artist => self.musicbrainz_artist.as_deref().and_then(musicbrainz),
            NfoKind::Album => self.musicbrainz_release_group.as_deref().and_then(musicbrainz),
            _ => None,
        };
        // A link names a movie or a show: a season's or an episode's ids
        // are their own, not the show's.
        match kind {
            NfoKind::Movie => {
                fill(&mut ids.tmdb, links.tmdb_movie.get());
                fill(&mut ids.imdb, links.imdb.get());
            }
            NfoKind::TvShow => {
                fill(&mut ids.tmdb, links.tmdb_tv.get());
                fill(&mut ids.imdb, links.imdb.get());
                fill(&mut ids.tvdb, links.tvdb.get());
                fill(&mut ids.tvmaze, links.tvmaze.get());
            }
            _ => {}
        }

        let release_date = [&self.premiered, &self.release_date, &self.aired]
            .into_iter()
            .find_map(|date| date.as_deref().and_then(date_of));
        let year = self
            .year
            .as_deref()
            .and_then(year_of)
            .or_else(|| release_date.as_deref().and_then(|date| date[..4].parse().ok()));
        let content_ratings = [&self.mpaa, &self.certification]
            .into_iter()
            .map(|rating| rating.as_deref().map(content_ratings).unwrap_or_default())
            .find(|ratings| !ratings.is_empty())
            .unwrap_or_default();
        let content_rating = content_rating(&content_ratings);
        let rating = self
            .ratings
            .iter()
            .find(|(_, default)| *default)
            .or(self.ratings.first())
            .map(|(value, _)| *value)
            .or_else(|| self.rating.as_deref().and_then(number).and_then(|value| out_of_ten(value, None)));
        let fields = NfoFields {
            title: self.title.as_deref().and_then(short),
            original_title: self.original_title.as_deref().and_then(short),
            sort_title: self.sort_title.as_deref().and_then(short),
            year,
            release_date,
            summary: self.plot.as_deref().and_then(summary).or_else(|| self.outline.as_deref().and_then(summary)),
            tagline: self.tagline.as_deref().and_then(short),
            content_rating,
            content_ratings,
            genres: genres(&self.genres),
            studio: self.studio.as_deref().and_then(|studio| studio.split('/').find_map(short)),
            rating,
        };
        // `<season>` and `<episode>` of a show are counts (-1, unknown, in
        // Kodi's): only an episode's are its numbers.
        let (season, episode) = match kind {
            NfoKind::Episode => {
                (self.season.as_deref().and_then(number_of), self.episode.as_deref().and_then(number_of))
            }
            NfoKind::Season => (self.season_number.as_deref().and_then(number_of), None),
            _ => (None, None),
        };
        Nfo {
            kind,
            ids,
            fields,
            locked: self.lockdata.is_some_and(|lock| lock.trim().eq_ignore_ascii_case("true")),
            season,
            episode,
            links_to: None,
        }
    }
}

fn fill<T>(slot: &mut Option<T>, value: Option<T>) {
    if slot.is_none() {
        *slot = value;
    }
}

/// The ids in links to a title's page, as Kodi reads them from an NFO.
#[derive(Default)]
struct Links {
    tmdb_movie: One<i64>,
    tmdb_tv: One<i64>,
    imdb: One<String>,
    tvdb: One<i64>,
    tvmaze: One<i64>,
}

/// A value seen once or more, or values that disagree: then none is
/// trusted (release notes naming several films).
#[derive(Default)]
struct One<T>(Option<Option<T>>);

impl<T: PartialEq + Clone> One<T> {
    fn see(&mut self, value: Option<T>) {
        let Some(value) = value else { return };
        match &self.0 {
            None => self.0 = Some(Some(value)),
            Some(Some(seen)) if *seen != value => self.0 = Some(None),
            _ => {}
        }
    }

    fn get(&self) -> Option<T> {
        self.0.clone().flatten()
    }
}

static LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?:^|[^a-z0-9.-])(?:[a-z0-9-]+\.)*(themoviedb\.org|imdb\.com|thetvdb\.com|tvmaze\.com)(/[^\s<>"']*)"#,
    )
    .unwrap()
});
static TMDB_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/(movie|tv)/(\d+)").unwrap());
static IMDB_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^/(?:[a-z]{2}(?:-[a-z]{2})?/)?title/(tt\d+)").unwrap());
static TVDB_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)/series/(\d+)(?:[/?#]|$)").unwrap());
static TVMAZE_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^/shows/(\d+)").unwrap());

impl Links {
    fn find(text: &str) -> Links {
        let mut links = Links::default();
        for found in LINK.captures_iter(text) {
            let path = &found[2];
            match found[1].to_ascii_lowercase().as_str() {
                "themoviedb.org" => {
                    if let Some(m) = TMDB_PATH.captures(path) {
                        let id = positive(&m[2]);
                        match m[1].to_ascii_lowercase().as_str() {
                            "movie" => links.tmdb_movie.see(id),
                            _ => links.tmdb_tv.see(id),
                        }
                    }
                }
                "imdb.com" => links.imdb.see(IMDB_PATH.captures(path).and_then(|m| imdb(&m[1]))),
                "thetvdb.com" => links.tvdb.see(tvdb_series(path)),
                _ => links.tvmaze.see(TVMAZE_PATH.captures(path).and_then(|m| positive(&m[1]))),
            }
        }
        links
    }
}

/// The series of a TheTVDB address: `/?tab=series&id=81189`,
/// `/?tab=episode&seriesid=81189&id=…`, `/series/81189`,
/// `/dereferrer/series/81189`.
fn tvdb_series(path: &str) -> Option<i64> {
    if let Some(m) = TVDB_PATH.captures(path) {
        return positive(&m[1]);
    }
    let query = path.split_once('?')?.1.split('#').next().unwrap_or_default();
    let param = |key: &str| {
        query
            .split('&')
            .find_map(|pair| pair.split_once('=').filter(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v))
    };
    if let Some(series) = param("seriesid") {
        return positive(series);
    }
    // Without `seriesid`, `id` is the series' only on its own page.
    param("tab").is_none_or(|tab| tab.eq_ignore_ascii_case("series")).then(|| param("id")).flatten().and_then(positive)
}

/// A file that isn't an NFO's XML, read for a link to the title's page.
fn link_only(text: &str) -> Option<Nfo> {
    let links = Links::find(text);
    let (tmdb_movie, tmdb_tv) = (links.tmdb_movie.get(), links.tmdb_tv.get());
    let (tvdb, tvmaze) = (links.tvdb.get(), links.tvmaze.get());
    let show = tmdb_tv.is_some() || tvdb.is_some() || tvmaze.is_some();
    let links_to = match (tmdb_movie.is_some(), show) {
        // Links to a film and to a show: which is meant can't be told.
        (true, true) => return None,
        (true, false) => Some(NfoKind::Movie),
        (false, true) => Some(NfoKind::TvShow),
        (false, false) => None,
    };
    let ids = NfoIds { tmdb: tmdb_movie.or(tmdb_tv), imdb: links.imdb.get(), tvdb, tvmaze, musicbrainz: None };
    (!ids.is_empty()).then(|| Nfo {
        kind: NfoKind::Link,
        ids,
        fields: NfoFields::default(),
        locked: false,
        season: None,
        episode: None,
        links_to,
    })
}

/// `text` without control characters, its spaces run together, and at most
/// `max` characters long; `lines` keeps line breaks (a paragraph's, at most
/// one empty line). `None` when nothing is left.
fn clean(text: &str, max: usize, lines: bool) -> Option<String> {
    let mut out = String::new();
    let mut count = 0;
    for c in text.replace("\r\n", "\n").chars() {
        let c = match c {
            '\n' if lines => '\n',
            c if c.is_whitespace() => ' ',
            c if c.is_control() => continue,
            c => c,
        };
        if c == '\n' {
            while out.ends_with(' ') {
                out.pop();
                count -= 1;
            }
            if out.is_empty() || out.ends_with("\n\n") {
                continue;
            }
        } else if c == ' ' && (out.is_empty() || out.ends_with([' ', '\n'])) {
            continue;
        }
        if count == max {
            break;
        }
        out.push(c);
        count += 1;
    }
    let out = out.trim_end();
    (!out.is_empty()).then(|| out.to_owned())
}

fn short(text: &str) -> Option<String> {
    clean(text, MAX_SHORT, false)
}

fn summary(text: &str) -> Option<String> {
    clean(text, MAX_SUMMARY, true)
}

/// Genres, given one an element or several in one (`Action / Comedy`),
/// each once and at most [`MAX_GENRES`].
fn genres(raw: &[String]) -> Vec<String> {
    let mut genres: Vec<String> = Vec::new();
    for genre in raw.iter().flat_map(|g| g.split(['/', '|'])).filter_map(short) {
        if genres.len() == MAX_GENRES {
            break;
        }
        if !genres.iter().any(|g| g.to_lowercase() == genre.to_lowercase()) {
            genres.push(genre);
        }
    }
    genres
}

/// Content ratings as an NFO writes them: `PG-13`, Kodi's `Rated PG-13`,
/// `US:PG-13`, or several countries' (`GB:15 / US:R`).
fn content_ratings(text: &str) -> Vec<NfoRating> {
    fn without_rated(text: &str) -> &str {
        let text = text.trim();
        match text.get(..6) {
            Some(prefix) if prefix.eq_ignore_ascii_case("rated ") => text[6..].trim(),
            _ => text,
        }
    }
    text.split('/')
        .filter_map(|entry| {
            let (country, rating) = match without_rated(entry).split_once(':') {
                Some((country, rating)) => (Some(country.trim()), without_rated(rating)),
                None => (None, without_rated(entry)),
            };
            let country = country.filter(|c| !c.is_empty()).map(|c| match c.to_ascii_uppercase().as_str() {
                "USA" => "US".to_owned(),
                upper => upper.to_owned(),
            });
            Some(NfoRating { country: country.and_then(|c| short(&c)), rating: short(rating)? })
        })
        .collect()
}

/// The United States' rating of `ratings`, else the first: what an NFO
/// file's rating is shown as.
fn content_rating(ratings: &[NfoRating]) -> Option<String> {
    ratings.iter().find(|r| r.country.as_deref() == Some("US")).or(ratings.first()).map(|r| r.rating.clone())
}

impl NfoFields {
    /// The rating to use where the metadata region is `region`, with its
    /// country: the region's, else the United States', else the first. A
    /// file read before ratings had countries has only `content_rating`.
    pub fn rating_in(&self, region: &str) -> Option<(Option<&str>, &str)> {
        let of = |country: &str| self.content_ratings.iter().find(|r| r.country.as_deref() == Some(country));
        match of(region).or_else(|| of("US")).or(self.content_ratings.first()) {
            Some(r) => Some((r.country.as_deref(), r.rating.as_str())),
            None => self.content_rating.as_deref().map(|r| (None, r)),
        }
    }
}

/// A date as `YYYY-MM-DD`, from one starting so (`2020-01-31 00:00:00`,
/// `2020-01-31T00:00:00Z`).
fn date_of(text: &str) -> Option<String> {
    let text = text.trim();
    let date = text.get(..10)?;
    let bytes = date.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes[range.clone()].iter().all(u8::is_ascii_digit).then(|| date[range].parse::<i64>().ok()).flatten()
    };
    if bytes[4] != b'-' || bytes[7] != b'-' || text.as_bytes().get(10).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let (year, month, day) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (year_in_range(year) && (1..=days).contains(&day)).then(|| date.to_owned())
}

fn year_of(text: &str) -> Option<i64> {
    let text = text.trim();
    match text.parse::<i64>() {
        Ok(year) => Some(year).filter(|&y| year_in_range(y)),
        // Some write a date there.
        Err(_) => date_of(text).and_then(|date| date[..4].parse().ok()),
    }
}

/// Kodi writes `0` for a year it doesn't know.
fn year_in_range(year: i64) -> bool {
    (1800..=2200).contains(&year)
}

/// A season or an episode number: 0 is the specials', -1 Kodi's unknown.
fn number_of(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok().filter(|n| (0..=100_000).contains(n))
}

/// A decimal, with a comma as some locales write it.
fn number(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse::<f64>().ok().filter(|n| n.is_finite())
}

/// A rating on 0–10, from one out of `max` (10 when it isn't said). Kodi
/// writes 0 for a title without one.
fn out_of_ten(value: f64, max: Option<f64>) -> Option<f64> {
    let value = match max {
        Some(max) if max > 0.0 => value * 10.0 / max,
        _ => value,
    };
    (value > 0.0 && value <= 10.0).then_some(value)
}

/// A positive integer id: TMDB's, TheTVDB's, TVmaze's.
fn positive(text: &str) -> Option<i64> {
    let text = text.trim();
    (!text.is_empty() && text.len() <= 12 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse::<i64>().ok())
        .flatten()
        .filter(|&id| id > 0)
}

/// An IMDb id: `tt` and seven or more digits.
fn imdb(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_lowercase();
    let digits = text.strip_prefix("tt")?;
    ((7..=10).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())).then_some(text)
}

/// A MusicBrainz id: a UUID, in lower case.
fn musicbrainz(text: &str) -> Option<String> {
    let text = text.trim().to_ascii_lowercase();
    let groups: Vec<&str> = text.split('-').collect();
    let shape = [8, 4, 4, 4, 12];
    (groups.len() == shape.len()
        && groups
            .iter()
            .zip(shape)
            .all(|(group, len)| group.len() == len && group.bytes().all(|b| b.is_ascii_hexdigit())))
    .then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(xml: &str) -> Nfo {
        one_of(xml.as_bytes())
    }

    fn one_of(bytes: &[u8]) -> Nfo {
        let mut entries = parse(bytes);
        assert_eq!(entries.len(), 1, "{entries:?}");
        entries.remove(0)
    }

    #[test]
    fn kodi_movie() {
        let nfo = one(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<!-- created on 2023-04-01 - Kodi -->
<movie>
    <title>The Matrix</title>
    <originaltitle>The Matrix</originaltitle>
    <sorttitle>Matrix 1</sorttitle>
    <ratings>
        <rating name="imdb" max="10">
            <value>8.700000</value>
            <votes>2000000</votes>
        </rating>
        <rating name="themoviedb" max="10" default="true">
            <value>8.200000</value>
            <votes>24000</votes>
        </rating>
    </ratings>
    <userrating>0</userrating>
    <top250>0</top250>
    <outline></outline>
    <plot>Set in the 22nd century, The Matrix tells the story of a computer hacker.</plot>
    <tagline>Welcome to the Real World.</tagline>
    <runtime>136</runtime>
    <thumb spoof="" cache="" aspect="poster">https://image.tmdb.org/t/p/original/poster.jpg</thumb>
    <mpaa>Rated R</mpaa>
    <playcount>1</playcount>
    <id>603</id>
    <uniqueid type="imdb">tt0133093</uniqueid>
    <uniqueid type="tmdb" default="true">603</uniqueid>
    <genre>Action</genre>
    <genre>Science Fiction</genre>
    <country>United States of America</country>
    <set><name>The Matrix Collection</name><overview></overview></set>
    <credits>Lilly Wachowski</credits>
    <director>Lana Wachowski</director>
    <premiered>1999-03-30</premiered>
    <year>1999</year>
    <studio>Warner Bros. Pictures</studio>
    <studio>Village Roadshow Pictures</studio>
    <actor><name>Keanu Reeves</name><role>Neo</role><order>0</order></actor>
    <fileinfo><streamdetails><video><codec>h264</codec></video></streamdetails></fileinfo>
</movie>"#);
        assert_eq!(nfo.kind, NfoKind::Movie);
        assert_eq!(nfo.ids.tmdb, Some(603));
        assert_eq!(nfo.ids.imdb.as_deref(), Some("tt0133093"));
        assert_eq!(nfo.ids.tvdb, None);
        let f = &nfo.fields;
        assert_eq!(f.title.as_deref(), Some("The Matrix"));
        assert_eq!(f.original_title.as_deref(), Some("The Matrix"));
        assert_eq!(f.sort_title.as_deref(), Some("Matrix 1"));
        assert_eq!(f.year, Some(1999));
        assert_eq!(f.release_date.as_deref(), Some("1999-03-30"));
        assert!(f.summary.as_deref().unwrap().starts_with("Set in the 22nd century"));
        assert_eq!(f.tagline.as_deref(), Some("Welcome to the Real World."));
        assert_eq!(f.content_rating.as_deref(), Some("R"));
        assert_eq!(f.genres, vec!["Action", "Science Fiction"]);
        assert_eq!(f.studio.as_deref(), Some("Warner Bros. Pictures"));
        assert_eq!(f.rating, Some(8.2));
        assert!(!nfo.locked);
        assert_eq!((nfo.season, nfo.episode), (None, None));
    }

    #[test]
    fn jellyfin_movie_with_lockdata() {
        let nfo = one(r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<movie>
  <plot><![CDATA[A thief who steals corporate secrets through dream-sharing.]]></plot>
  <outline><![CDATA[Not this one.]]></outline>
  <lockdata>true</lockdata>
  <dateadded>2023-01-01 10:00:00</dateadded>
  <title>Inception</title>
  <rating>8.4</rating>
  <year>2010</year>
  <mpaa>PG-13</mpaa>
  <imdbid>tt1375666</imdbid>
  <tmdbid>27205</tmdbid>
  <premiered>2010-07-15</premiered>
  <releasedate>2010-07-15</releasedate>
  <genre>Action</genre>
  <genre>Science Fiction</genre>
  <studio>Legendary Pictures</studio>
  <uniqueid type="imdb">tt1375666</uniqueid>
  <uniqueid type="tmdb">27205</uniqueid>
</movie>"#);
        assert!(nfo.locked);
        assert_eq!(nfo.ids.tmdb, Some(27205));
        assert_eq!(nfo.ids.imdb.as_deref(), Some("tt1375666"));
        assert_eq!(nfo.fields.summary.as_deref(), Some("A thief who steals corporate secrets through dream-sharing."));
        assert_eq!(nfo.fields.rating, Some(8.4));
        assert_eq!(nfo.fields.content_rating.as_deref(), Some("PG-13"));
        assert_eq!(nfo.fields.release_date.as_deref(), Some("2010-07-15"));
    }

    #[test]
    fn radarr_movie() {
        let nfo = one(r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<movie>
  <title>Arrival</title>
  <originaltitle>Arrival</originaltitle>
  <sorttitle>arrival</sorttitle>
  <ratings>
    <rating name="tmdb" max="10" default="true"><value>7.6</value><votes>17000</votes></rating>
    <rating name="imdb" max="10"><value>7.9</value><votes>780000</votes></rating>
  </ratings>
  <userrating />
  <plot>Taking place after alien crafts land around the world.</plot>
  <mpaa>US:PG-13 / GB:12A</mpaa>
  <uniqueid type="tmdb" default="true">329865</uniqueid>
  <uniqueid type="imdb">tt2543164</uniqueid>
  <tmdbid>329865</tmdbid>
  <id>tt2543164</id>
  <imdbid>tt2543164</imdbid>
  <premiered>2016-11-10</premiered>
  <year>2016</year>
  <studio>Paramount</studio>
  <genre>Drama</genre>
  <genre>Science Fiction</genre>
  <genre>Mystery</genre>
  <trailer>plugin://plugin.video.youtube/?action=play_video&amp;videoid=tFMo3UJ4B4g</trailer>
  <watched>false</watched>
</movie>"#);
        assert_eq!(nfo.ids.tmdb, Some(329865));
        assert_eq!(nfo.ids.imdb.as_deref(), Some("tt2543164"));
        assert_eq!(nfo.fields.content_rating.as_deref(), Some("PG-13"));
        assert_eq!(nfo.fields.rating, Some(7.6));
        assert_eq!(nfo.fields.genres, vec!["Drama", "Science Fiction", "Mystery"]);
    }

    #[test]
    fn tiny_media_manager_tvshow() {
        let nfo = one(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!--created on 2024-02-11 12:00:00 - tinyMediaManager 4.3.15-->
<tvshow>
  <title>Breaking Bad</title>
  <originaltitle>Breaking Bad</originaltitle>
  <showtitle>Breaking Bad</showtitle>
  <sorttitle/>
  <year>2008</year>
  <ratings>
    <rating default="false" max="10" name="imdb"><value>9.5</value><votes>2000000</votes></rating>
    <rating default="true" max="100" name="metacritic"><value>87</value><votes>0</votes></rating>
  </ratings>
  <userrating>0.0</userrating>
  <outline/>
  <plot>When Walter White, a New Mexico chemistry teacher, is diagnosed with Stage III cancer…</plot>
  <tagline/>
  <runtime>47</runtime>
  <season>5</season>
  <episode>62</episode>
  <mpaa>US:TV-MA</mpaa>
  <certification>US:TV-MA</certification>
  <episodeguide>{"tmdb":"1396","imdb":"tt0903747","tvdb":"81189"}</episodeguide>
  <id>81189</id>
  <imdbid>tt0903747</imdbid>
  <tmdbid>1396</tmdbid>
  <uniqueid default="true" type="tvdb">81189</uniqueid>
  <uniqueid default="false" type="imdb">tt0903747</uniqueid>
  <uniqueid default="false" type="tmdb">1396</uniqueid>
  <premiered>2008-01-20</premiered>
  <status>Ended</status>
  <genre>Drama</genre>
  <genre>Crime</genre>
  <studio>AMC</studio>
  <studio>Sony Pictures Television</studio>
  <namedseason number="1">Season 1</namedseason>
</tvshow>"#);
        assert_eq!(nfo.kind, NfoKind::TvShow);
        assert_eq!(
            nfo.ids,
            NfoIds { tmdb: Some(1396), imdb: Some("tt0903747".into()), tvdb: Some(81189), ..Default::default() }
        );
        assert_eq!(nfo.fields.title.as_deref(), Some("Breaking Bad"));
        assert_eq!(nfo.fields.sort_title, None);
        assert_eq!(nfo.fields.content_rating.as_deref(), Some("TV-MA"));
        // The default rating, out of 100.
        assert_eq!(nfo.fields.rating, Some(8.7));
        assert_eq!(nfo.fields.studio.as_deref(), Some("AMC"));
        // A show's season and episode are counts, not its numbers.
        assert_eq!((nfo.season, nfo.episode), (None, None));
    }

    #[test]
    fn a_shows_id_without_typed_ids_is_thetvdbs() {
        let nfo = one("<tvshow><title>Lost</title><id>73739</id></tvshow>");
        assert_eq!(nfo.ids.tvdb, Some(73739));
        assert_eq!(nfo.ids.tmdb, None);
        let nfo = one("<movie><title>Alien</title><id>348</id></movie>");
        assert_eq!(nfo.ids.tmdb, Some(348));
        // With a typed id, `<id>` repeats the default one: not TheTVDB's.
        let nfo = one(r#"<tvshow><uniqueid type="tmdb" default="true">4607</uniqueid><id>4607</id></tvshow>"#);
        assert_eq!(nfo.ids, NfoIds { tmdb: Some(4607), ..Default::default() });
    }

    #[test]
    fn multi_episode_file() {
        let entries = parse(
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>
<episodedetails>
    <title>Pilot (1)</title>
    <season>1</season>
    <episode>1</episode>
    <plot>Stripped of everything, the survivors of a plane crash are forced to work together.</plot>
    <aired>2004-09-22</aired>
    <uniqueid type="tvdb" default="true">127131</uniqueid>
</episodedetails>
<episodedetails>
    <title>Pilot (2)</title>
    <season>1</season>
    <episode>2</episode>
    <aired>2004-09-29</aired>
    <uniqueid type="tvdb" default="true">127132</uniqueid>
</episodedetails>
"#,
        );
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.kind == NfoKind::Episode));
        assert_eq!((entries[0].season, entries[0].episode), (Some(1), Some(1)));
        assert_eq!((entries[1].season, entries[1].episode), (Some(1), Some(2)));
        assert_eq!(entries[1].fields.title.as_deref(), Some("Pilot (2)"));
        assert_eq!(entries[1].fields.release_date.as_deref(), Some("2004-09-29"));
        assert_eq!(entries[1].fields.year, Some(2004));
        assert_eq!(entries[0].ids.tvdb, Some(127131));
    }

    #[test]
    fn season_nfo() {
        let nfo = one(r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<season>
  <plot><![CDATA[The second season.]]></plot>
  <lockdata>false</lockdata>
  <title>Season 2</title>
  <year>2009</year>
  <premiered>2009-03-08</premiered>
  <seasonnumber>2</seasonnumber>
</season>"#);
        assert_eq!(nfo.kind, NfoKind::Season);
        assert_eq!(nfo.season, Some(2));
        assert_eq!(nfo.fields.title.as_deref(), Some("Season 2"));
        assert_eq!(nfo.fields.summary.as_deref(), Some("The second season."));
        assert!(!nfo.locked);
    }

    #[test]
    fn url_only_nfo() {
        let nfo = one("https://www.themoviedb.org/movie/603-the-matrix\n");
        assert_eq!(nfo.kind, NfoKind::Link);
        assert_eq!(nfo.links_to, Some(NfoKind::Movie));
        assert_eq!(nfo.ids, NfoIds { tmdb: Some(603), ..Default::default() });

        let nfo = one("http://www.imdb.com/title/tt0133093/");
        assert_eq!((nfo.ids.imdb.as_deref(), nfo.links_to), (Some("tt0133093"), None));
        let nfo = one("https://www.themoviedb.org/tv/1396");
        assert_eq!((nfo.ids.tmdb, nfo.links_to), (Some(1396), Some(NfoKind::TvShow)));
        assert_eq!(one("https://thetvdb.com/?tab=series&id=81189").ids.tvdb, Some(81189));
        assert_eq!(one("https://thetvdb.com/?tab=episode&seriesid=81189&id=349232").ids.tvdb, Some(81189));
        assert_eq!(one("https://thetvdb.com/dereferrer/series/81189").ids.tvdb, Some(81189));
        assert_eq!(one("https://www.tvmaze.com/shows/169/breaking-bad").ids.tvmaze, Some(169));
        // Links to a film and a show, or to two films: which can't be told.
        assert!(parse(b"https://www.themoviedb.org/movie/603 https://www.themoviedb.org/tv/1396").is_empty());
        assert!(parse(b"https://www.themoviedb.org/movie/603 https://www.themoviedb.org/movie/604").is_empty());
        // Not those sites.
        assert!(parse(b"https://notimdb.com/title/tt0133093 https://example.com/?u=thetvdb.com").is_empty());
    }

    #[test]
    fn xml_with_a_trailing_link() {
        let nfo = one(
            "<movie>\n  <title>The Matrix</title>\n  <uniqueid type=\"imdb\">tt0133093</uniqueid>\n</movie>\nhttps://www.themoviedb.org/movie/603\n",
        );
        assert_eq!(nfo.kind, NfoKind::Movie);
        assert_eq!(nfo.fields.title.as_deref(), Some("The Matrix"));
        assert_eq!(nfo.ids.tmdb, Some(603));
        assert_eq!(nfo.ids.imdb.as_deref(), Some("tt0133093"));
        // The file's own id wins, and a show's link doesn't fill a movie's.
        let nfo = one("<movie><tmdbid>1</tmdbid></movie>https://www.themoviedb.org/movie/603");
        assert_eq!(nfo.ids.tmdb, Some(1));
        let nfo = one("<movie><title>A</title></movie>https://www.themoviedb.org/tv/1396");
        assert_eq!(nfo.ids.tmdb, None);
    }

    #[test]
    fn release_notes_are_not_read() {
        let notes = br#"
      _____  _____  ____  _    _ _____
     / ____|/ ____|/ __ \| |  | |  __ \   <<< PRESENTS >>>
    | |  __| |  __| |  | | |  | | |__) |
    | | |_ | | |_ | |  | | |  | |  ___/
    | |__| | |__| | |__| | |__| | |
     \_____|\_____|\____/ \____/|_|      & friends

  RELEASE DATE.....: 2020-01-01
  SIZE.............: 4.37 GB
  <movie> is out, enjoy </3
"#;
        assert!(parse(notes).is_empty());
        // With a link in them, it is read as Kodi reads it.
        let mut with_link = notes.to_vec();
        with_link.extend_from_slice(b"  IMDB.............: https://www.imdb.com/title/tt0133093/\n");
        let entries = parse(&with_link);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, NfoKind::Link);
        assert_eq!(entries[0].ids.imdb.as_deref(), Some("tt0133093"));
    }

    #[test]
    fn broken_or_binary_files_give_nothing() {
        assert!(parse(b"<movie><title>Unclosed</movie>").is_empty());
        assert!(parse(b"<movie><title>Cut short</title>").is_empty());
        assert!(parse(b"<html><body>Not an NFO</body></html>").is_empty());
        assert!(parse(b"").is_empty());
        assert!(parse(b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x01\x00").is_empty());
        let noise: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        assert!(parse(&noise).is_empty());
        // A broken entry after a whole one: the whole one is kept.
        let entries = parse(b"<episodedetails><episode>1</episode></episodedetails><episodedetails><episode>2</oops>");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].episode, Some(1));
    }

    #[test]
    fn entities() {
        let nfo = one(r#"<!DOCTYPE movie [<!ENTITY boom "BOOM">]>
<movie>
  <title>Tom &amp; Jerry &#8211; &#x2603; &lt;3&gt;</title>
  <plot>&boom; Fish &nbsp;&amp; Chips &quot;quoted&quot; &apos;s</plot>
  <tagline>Tom & Jerry</tagline>
  <studio>A <i>bold</i> one</studio>
</movie>"#);
        assert_eq!(nfo.fields.title.as_deref(), Some("Tom & Jerry – ☃ <3>"));
        // Entities a DTD names, and HTML's, are left out.
        assert_eq!(nfo.fields.summary.as_deref(), Some("Fish & Chips \"quoted\" 's"));
        assert_eq!(nfo.fields.tagline.as_deref(), Some("Tom & Jerry"));
        assert_eq!(nfo.fields.studio.as_deref(), Some("A bold one"));
    }

    #[test]
    fn over_long_and_odd_fields_are_bounded() {
        let long = "x".repeat(30_000);
        let genres: String = (0..80).map(|i| format!("<genre>G{i}</genre>")).collect();
        let xml = format!(
            "<movie><title>{long}</title><plot>{long}</plot>{genres}<genre>a / b|A</genre>\
             <year>0</year><rating>0.000000</rating><tmdbid>-5</tmdbid><imdbid>tt12</imdbid>\
             <tvdbid>99999999999999999999</tvdbid><originaltitle>  Spaced\u{7}\tout\n </originaltitle>\
             <sorttitle>   </sorttitle></movie>"
        );
        let nfo = one(&xml);
        let f = &nfo.fields;
        assert_eq!(f.title.as_deref().map(|t| t.chars().count()), Some(MAX_SHORT));
        assert_eq!(f.summary.as_deref().map(|s| s.chars().count()), Some(MAX_SUMMARY));
        assert_eq!(f.genres.len(), MAX_GENRES);
        assert_eq!(f.year, None);
        assert_eq!(f.rating, None);
        assert_eq!(f.original_title.as_deref(), Some("Spaced out"));
        assert_eq!(f.sort_title, None);
        assert_eq!(nfo.ids, NfoIds::default());

        let nfo = one(
            "<movie><genre>Action / Comedy</genre><genre>comedy</genre><plot>One\r\n\r\n\r\nTwo  \n three</plot></movie>",
        );
        assert_eq!(nfo.fields.genres, vec!["Action", "Comedy"]);
        assert_eq!(nfo.fields.summary.as_deref(), Some("One\n\nTwo\nthree"));
    }

    #[test]
    fn content_ratings() {
        let shown = |text: &str| content_rating(&super::content_ratings(text));
        assert_eq!(shown("US:PG-13").as_deref(), Some("PG-13"));
        assert_eq!(shown("Rated R").as_deref(), Some("R"));
        assert_eq!(shown("GB:15 / US:R / DE:16").as_deref(), Some("R"));
        assert_eq!(shown("GB:15 / DE:16").as_deref(), Some("15"));
        assert_eq!(shown("US:Rated PG").as_deref(), Some("PG"));
        assert_eq!(shown("USA:R").as_deref(), Some("R"));
        assert_eq!(shown("TV-14").as_deref(), Some("TV-14"));
        assert_eq!(shown(" / ").as_deref(), None);
        let nfo = one("<movie><mpaa></mpaa><certification>US:PG-13</certification></movie>");
        assert_eq!(nfo.fields.content_rating.as_deref(), Some("PG-13"));
    }

    /// The metadata region's rating is used, with its country; the United
    /// States' when the file has none of the region's.
    #[test]
    fn the_regions_rating_is_used() {
        let nfo = one("<movie><mpaa>GB:15 / US:R / dk:A</mpaa></movie>");
        assert_eq!(nfo.fields.rating_in("GB"), Some((Some("GB"), "15")));
        assert_eq!(nfo.fields.rating_in("DK"), Some((Some("DK"), "A")));
        assert_eq!(nfo.fields.rating_in("FR"), Some((Some("US"), "R")));
        let bare = one("<movie><mpaa>Rated PG-13</mpaa></movie>");
        assert_eq!(bare.fields.rating_in("GB"), Some((None, "PG-13")));
        let old = NfoFields { content_rating: Some("R".into()), ..NfoFields::default() };
        assert_eq!(old.rating_in("US"), Some((None, "R")), "fields stored before ratings had countries");
    }

    #[test]
    fn dates_and_years() {
        assert_eq!(date_of("2020-01-31").as_deref(), Some("2020-01-31"));
        assert_eq!(date_of("2020-01-31 00:00:00").as_deref(), Some("2020-01-31"));
        assert_eq!(date_of("2020-01-31T00:00:00Z").as_deref(), Some("2020-01-31"));
        assert_eq!(date_of("2020-02-30"), None);
        assert_eq!(date_of("2020-02-29").as_deref(), Some("2020-02-29"));
        assert_eq!(date_of("2021-02-29"), None);
        assert_eq!(date_of("31/01/2020"), None);
        assert_eq!(date_of("2020-01-311"), None);
        assert_eq!(date_of("2020-1-31é"), None);
        assert_eq!(year_of("2020-05-01"), Some(2020));
        let nfo = one("<movie><releasedate>2011-05-06</releasedate><premiered>bad</premiered></movie>");
        assert_eq!((nfo.fields.year, nfo.fields.release_date.as_deref()), (Some(2011), Some("2011-05-06")));
    }

    #[test]
    fn music_ids() {
        let artist = one(
            "<artist><name>Radiohead</name><musicBrainzArtistID>A74B1B7F-71A5-4011-9441-D0B5E4122711</musicBrainzArtistID></artist>",
        );
        assert_eq!(artist.ids.musicbrainz.as_deref(), Some("a74b1b7f-71a5-4011-9441-d0b5e4122711"));
        let album = one("<album><title>OK Computer</title>\
             <musicbrainzartistid>a74b1b7f-71a5-4011-9441-d0b5e4122711</musicbrainzartistid>\
             <musicbrainzreleasegroupid>b1392450-e666-3926-a536-22c65f834433</musicbrainzreleasegroupid></album>");
        assert_eq!(album.ids.musicbrainz.as_deref(), Some("b1392450-e666-3926-a536-22c65f834433"));
        assert_eq!(musicbrainz("not-a-uuid"), None);
    }

    #[test]
    fn encodings() {
        let mut utf8_bom = b"\xEF\xBB\xBF".to_vec();
        utf8_bom.extend_from_slice("<movie><title>Amélie</title></movie>".as_bytes());
        assert_eq!(one_of(&utf8_bom).fields.title.as_deref(), Some("Amélie"));
        let latin1 = b"<movie><title>Am\xe9lie</title></movie>";
        assert_eq!(one_of(latin1).fields.title.as_deref(), Some("Amélie"));
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend("<movie><title>Amélie</title></movie>".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(one_of(&utf16).fields.title.as_deref(), Some("Amélie"));
    }

    #[test]
    fn any_cut_or_flipped_byte_is_read_without_panicking() {
        let sample = "<?xml version=\"1.0\"?><movie><title>Amélie &amp; ☃</title><year>2001</year>\
                      <premiered>2001-04-25</premiered><mpaa>Rated R / FR:U</mpaa>\
                      <ratings><rating max=\"10\" default=\"true\"><value>8,3</value></rating></ratings>\
                      <uniqueid type=\"tmdb\">194</uniqueid><id>tt0211915</id></movie>\
                      https://www.themoviedb.org/movie/194"
            .as_bytes();
        for cut in 0..=sample.len() {
            parse(&sample[..cut]);
            let mut flipped = sample.to_vec();
            if let Some(byte) = flipped.get_mut(cut) {
                *byte ^= 0xA5;
            }
            parse(&flipped);
        }
        assert_eq!(one_of(sample).fields.rating, Some(8.3));
    }

    #[test]
    fn read_skips_files_too_big() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("movie.nfo");
        std::fs::write(&small, "<movie><title>Small</title></movie>").unwrap();
        let entries = read(&small).unwrap();
        assert_eq!(entries[0].fields.title.as_deref(), Some("Small"));

        let big = dir.path().join("big.nfo");
        let mut xml = b"<movie><title>Big</title><plot>".to_vec();
        xml.resize(2 << 20, b'x');
        xml.extend_from_slice(b"</plot></movie>");
        std::fs::write(&big, xml).unwrap();
        assert_eq!(read(&big), None);
        assert_eq!(read(&dir.path().join("missing.nfo")), None);
    }
}
