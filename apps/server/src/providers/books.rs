//! Audiobooks' metadata from Open Library
//! (<https://openlibrary.org/developers/api>) and, for books whose files
//! carry an Audible ASIN, Audnexus (<https://github.com/laxamentumtech/audnexus>).
//! Neither needs a key.
//!
//! Open Library is searched by a book's title and writer. Its data is public
//! domain (CC0), but it describes printed books, so it fills in only the
//! summary, the year and a cover. Audnexus has what is particular to
//! audiobooks (narrators, a book's place in its series, the cover of the
//! recording), but only looks a book up by its ASIN, which tools such as
//! Libation and OpenAudible write into the files; Pley never searches
//! Audible. Audnexus is asked in the region of the server's country, then in
//! the United States', where most ASINs are sold.
//!
//! As with music, the files' tags win: what is fetched fills in what the
//! book lacks.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;
use reqwest::header::ACCEPT;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{
    Api, ProviderError, Throttle, artwork, full_date, loose_id, loose_text, nullable, strip_html, title_score, year_of,
};
use crate::db::Db;
use crate::library::LibraryRow;
use crate::store::Store;
use crate::util::{normalize_key, now};

pub const AUDNEXUS: &str = "https://api.audnex.us";
pub const OPEN_LIBRARY: &str = "https://openlibrary.org";
pub const OPEN_LIBRARY_COVERS: &str = "https://covers.openlibrary.org";
/// Genres kept for a book.
const MAX_GENRES: usize = 5;
/// Writers or narrators kept for a book, and the characters kept of a name,
/// of a series and of a summary: as many as the scanner keeps of tags.
const MAX_NAMES: usize = 50;
const MAX_TEXT: usize = 500;
const MAX_SUMMARY: usize = 20_000;
/// A library pass stops after this many failures in a row (a source doesn't
/// answer); what's left is tried after the next scan.
const MAX_FAILURES: u32 = 5;
/// What the scanner calls a book's writer when nothing names one.
const UNKNOWN_AUTHOR: &str = "Unknown Author";
/// Audnexus's region where a country has none of its own.
const DEFAULT_REGION: &str = "us";

pub struct Books {
    audnexus: Api,
    audnexus_base: String,
    openlibrary: Api,
    openlibrary_base: String,
    covers: String,
    db: Db,
    store: Arc<Store>,
}

// ------------------------------------------------------------------ Audnexus's answers

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct AudnexusBook {
    title: Option<String>,
    #[serde(deserialize_with = "nullable")]
    authors: Vec<Person>,
    #[serde(deserialize_with = "nullable")]
    narrators: Vec<Person>,
    series_primary: Option<SeriesRef>,
    #[serde(deserialize_with = "nullable")]
    genres: Vec<Genre>,
    /// "2021-05-04T00:00:00.000Z".
    release_date: Option<String>,
    /// HTML.
    summary: Option<String>,
    image: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Person {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SeriesRef {
    name: String,
    #[serde(deserialize_with = "loose_text")]
    position: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Genre {
    name: String,
    /// "genre", or "tag" for Audible's narrower themes.
    #[serde(rename = "type")]
    kind: String,
}

// ------------------------------------------------------------------ Open Library's answers

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WorkSearch {
    #[serde(deserialize_with = "nullable")]
    docs: Vec<WorkDoc>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WorkDoc {
    key: String,
    title: String,
    subtitle: Option<String>,
    #[serde(deserialize_with = "nullable")]
    author_name: Vec<String>,
    #[serde(deserialize_with = "loose_id")]
    first_publish_year: Option<i64>,
    #[serde(deserialize_with = "loose_id")]
    cover_i: Option<i64>,
    #[serde(deserialize_with = "loose_id")]
    edition_count: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Work {
    description: Option<serde_json::Value>,
}

impl Work {
    /// Its description: text, or `{ "type": "/type/text", "value": text }`;
    /// none in any other shape.
    fn description(&self) -> Option<&str> {
        match self.description.as_ref()? {
            serde_json::Value::String(text) => Some(text),
            serde_json::Value::Object(typed) => typed.get("value")?.as_str(),
            _ => None,
        }
    }
}

// ------------------------------------------------------------------ ours

#[derive(Debug, Default, sqlx::FromRow)]
struct BookRow {
    title: String,
    authors: Option<String>,
    poster: Option<String>,
    audible_asin: Option<String>,
    openlibrary_id: Option<String>,
}

/// A book as it is matched: what the files and folders say of it.
#[derive(Debug, Default, Clone)]
struct Book {
    title: String,
    /// Writers named (none when the scanner knows of none).
    authors: Vec<String>,
}

impl Book {
    fn from_row(row: &BookRow) -> Self {
        let authors: Vec<String> =
            row.authors.as_deref().and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default();
        Self { title: row.title.clone(), authors: authors.into_iter().filter(|a| a != UNKNOWN_AUTHOR).collect() }
    }
}

/// What a part's probe says about the book (`media_files.media_info`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PartInfo {
    tags: HashMap<String, String>,
    has_cover_art: bool,
}

/// What the book's files carry: an Audible ASIN, and cover art.
#[derive(Debug, Default)]
struct FileFacts {
    /// Whether any of them has been read.
    read: bool,
    asin: Option<String>,
    cover: bool,
}

impl Books {
    pub fn new(
        http: reqwest::Client,
        audnexus: &str,
        openlibrary: &str,
        covers: &str,
        db: Db,
        store: Arc<Store>,
    ) -> Self {
        let base = |b: &str| b.trim_end_matches('/').to_owned();
        Self {
            // Audnexus allows 300 requests a minute; Open Library asks for
            // gentleness. Both are asked well below that.
            audnexus: Api::new(http.clone(), "Audnexus", Throttle::new(2, Duration::from_secs(1))),
            audnexus_base: base(audnexus),
            openlibrary: Api::new(http, "Open Library", Throttle::new(1, Duration::from_secs(1))),
            openlibrary_base: base(openlibrary),
            covers: base(covers),
            db,
            store,
        }
    }

    /// Look up the books of a library that have no metadata yet (all of
    /// them when `force`), and `again` a few of those found nothing for a
    /// while ago ([`crate::metadata::looked_up_again`]). `country` is the
    /// server's (ISO 3166-1, "GB").
    pub async fn refresh_library(
        &self,
        lib: &LibraryRow,
        force: bool,
        again: bool,
        country: &str,
        progress: &(dyn Fn(u64, u64) + Sync),
    ) -> anyhow::Result<()> {
        let mut books: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM items WHERE library_id = ? AND kind = 'audiobook' AND metadata_locked = 0
               AND (? OR metadata_at IS NULL) ORDER BY added_at DESC",
        )
        .bind(lib.id)
        .bind(force)
        .fetch_all(&self.db.read)
        .await?;
        if again && !force {
            let due: Vec<i64> = sqlx::query_scalar(concat!(
                "SELECT id FROM items WHERE library_id = ? AND kind = 'audiobook' AND metadata_locked = 0
                   AND metadata_at < 0 AND ",
                crate::metadata::looked_up_again!("(-metadata_at)", "added_at"),
                " ORDER BY ABS(metadata_at) LIMIT ?"
            ))
            .bind(lib.id)
            .bind(crate::metadata::LOOKED_UP_AGAIN)
            .fetch_all(&self.db.read)
            .await?;
            books.extend(due);
        }
        let total = books.len() as u64;
        if total == 0 {
            return Ok(());
        }
        tracing::info!("fetching metadata for {total} audiobooks in library {}", lib.name);
        let (mut done, mut failures) = (0u64, 0u32);
        for id in books {
            let result = self.refresh_book(id, country).await;
            done += 1;
            match result {
                Ok(None) => failures = 0,
                Err(ProviderError::NotFound) => {
                    failures = 0;
                    tracing::debug!("no Audnexus or Open Library match for item {id}");
                    sqlx::query("UPDATE items SET metadata_at = ? WHERE id = ?")
                        .bind(-now())
                        .bind(id)
                        .execute(&self.db.write)
                        .await?;
                }
                Ok(Some(err)) | Err(err) => {
                    tracing::warn!("audiobook metadata for item {id} failed: {err:#}");
                    failures += 1;
                    if failures >= MAX_FAILURES {
                        bail!("Audnexus or Open Library isn't answering; the rest waits for the next scan");
                    }
                }
            }
            // Hand edits win.
            crate::overrides::apply(&self.db, &[id]).await?;
            if done % 5 == 0 || done == total {
                progress(done, total);
            }
        }
        Ok(())
    }

    /// Refresh a book (or the book of a part).
    pub async fn refresh_item(&self, id: i64, country: &str) -> Result<(), ProviderError> {
        let row: Option<(String, Option<i64>)> = sqlx::query_as("SELECT kind, parent_id FROM items WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db.read)
            .await?;
        let book = match row {
            Some((kind, _)) if kind == "audiobook" => id,
            Some((kind, Some(book))) if kind == "part" => book,
            _ => return Ok(()),
        };
        match self.refresh_book(book, country).await? {
            None => Ok(()),
            // What Open Library has is written, and hand edits go back over
            // it; Audnexus is asked again after the next scan.
            Some(err) => {
                tracing::warn!("audiobook metadata for item {book}: {err:#}");
                Ok(())
            }
        }
    }

    /// Audnexus for a book with an ASIN; Open Library for the rest, for a
    /// book Audnexus doesn't know, and while Audnexus can't be reached. In
    /// that last case the answer is Audnexus's error, and the book stays to
    /// be fetched: Audnexus is asked again after the next scan.
    async fn refresh_book(&self, id: i64, country: &str) -> Result<Option<ProviderError>, ProviderError> {
        let row: BookRow = sqlx::query_as(
            "SELECT title, authors, poster, audible_asin, openlibrary_id FROM items WHERE id = ? AND kind = 'audiobook'",
        )
        .bind(id)
        .fetch_optional(&self.db.read)
        .await?
        .ok_or(ProviderError::NotFound)?;
        let book = Book::from_row(&row);
        if normalize_key(&book.title).is_empty() {
            return Err(ProviderError::NotFound);
        }
        let files = self.file_facts(id).await?;

        // The files' ASIN: it is the recording they hold. The one kept from
        // before stands in only while none of them has been read.
        let asin = if files.read { files.asin.clone() } else { row.audible_asin.clone().filter(|a| is_asin(a)) };
        let audnexus_error = match &asin {
            None => None,
            Some(asin) => match self.audnexus_book(asin, country).await {
                Ok(Some(found)) => return self.write_audnexus(id, asin, &row, &files, found).await.map(|()| None),
                Ok(None) => None,
                Err(err) => {
                    tracing::debug!("Audnexus: {err:#}");
                    Some(err)
                }
            },
        };
        match self.openlibrary_work(&book, row.openlibrary_id.as_deref()).await {
            Ok(Some((doc, description))) => {
                let done = audnexus_error.is_none();
                self.write_openlibrary(id, asin.as_deref(), &row, &files, doc, description, done).await?;
                Ok(audnexus_error)
            }
            // Audnexus not answering isn't "no such book": tried again later.
            Ok(None) => Err(audnexus_error.unwrap_or(ProviderError::NotFound)),
            Err(err) => Err(audnexus_error.unwrap_or(err)),
        }
    }

    // -------------------------------------------------------------- Audnexus

    /// A book by its ASIN, in the country's region, else the United States'.
    async fn audnexus_book(&self, asin: &str, country: &str) -> Result<Option<AudnexusBook>, ProviderError> {
        let region = audnexus_region(country);
        let regions = if region == DEFAULT_REGION { vec![region] } else { vec![region, DEFAULT_REGION] };
        let mut error = None;
        for region in regions {
            let request = self
                .audnexus
                .http
                .get(format!("{}/books/{asin}", self.audnexus_base))
                .query(&[("region", region)])
                .header(ACCEPT, "application/json");
            match self.audnexus.json::<AudnexusBook>(request).await {
                Ok(book) if book.title.as_deref().is_some_and(|t| !t.trim().is_empty()) => return Ok(Some(book)),
                // An ASIN not sold in a region is a 404 there.
                Ok(_) | Err(ProviderError::NotFound) => {}
                // The United States' region may have it all the same.
                Err(err) => error = error.or(Some(err)),
            }
        }
        error.map_or(Ok(None), Err)
    }

    async fn write_audnexus(
        &self,
        id: i64,
        asin: &str,
        row: &BookRow,
        files: &FileFacts,
        found: AudnexusBook,
    ) -> Result<(), ProviderError> {
        let summary = found.summary.as_deref().map(|s| cut(&strip_html(s), MAX_SUMMARY)).filter(|s| !s.is_empty());
        let released = release_day(found.release_date.as_deref());
        let genres = genres(&found.genres);
        let genres = (!genres.is_empty()).then(|| serde_json::to_string(&genres).unwrap_or_default());
        let authors = names(found.authors.iter().map(|a| a.name.as_str()).filter(|a| !is_credited_role(a)));
        let narrators = names(found.narrators.iter().map(|n| n.name.as_str()));
        let series = found.series_primary.as_ref().filter(|s| !s.name.trim().is_empty());
        let series_title = series.map(|s| cut(&s.name, MAX_TEXT));
        let series_index = series.and_then(|s| sequence(s.position.as_deref()));
        // Covers in the files or next to them win.
        let poster =
            if files.cover { None } else { artwork(&self.store, row.poster.as_deref(), found.image.as_deref()).await };
        let now = now();
        // Every expression reads the row as it was: a series filled in brings
        // its place, and a place is filled in for the series the book has.
        sqlx::query(
            "UPDATE items SET audible_asin = ?, year = COALESCE(year, ?), release_date = COALESCE(release_date, ?),
                summary = COALESCE(summary, ?), genres = COALESCE(genres, ?), poster = COALESCE(?, poster),
                authors = CASE WHEN authors IS NULL OR authors IN ('[]', '[\"Unknown Author\"]')
                               THEN COALESCE(?, authors) ELSE authors END,
                narrators = CASE WHEN narrators IS NULL OR narrators = '[]' THEN COALESCE(?, narrators) ELSE narrators END,
                series = COALESCE(series, ?),
                series_index = CASE WHEN series IS NULL THEN ?
                                    WHEN series_index IS NULL AND lower(series) = lower(?) THEN ?
                                    ELSE series_index END,
                metadata_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(asin)
        .bind(year_of(released))
        .bind(full_date(released))
        .bind(summary)
        .bind(genres)
        .bind(poster)
        .bind(authors)
        .bind(narrators)
        .bind(&series_title)
        .bind(series_index)
        .bind(&series_title)
        .bind(series_index)
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    // -------------------------------------------------------------- Open Library

    async fn ol_get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T, ProviderError> {
        let request = self
            .openlibrary
            .http
            .get(format!("{}{path}", self.openlibrary_base))
            .query(query)
            .header(ACCEPT, "application/json");
        self.openlibrary.json(request).await
    }

    /// The work for a book, with its description: the best search result,
    /// or the one it was matched to before while that still fits it.
    async fn openlibrary_work(
        &self,
        book: &Book,
        known: Option<&str>,
    ) -> Result<Option<(WorkDoc, Option<String>)>, ProviderError> {
        let title = clean_title(&book.title);
        let mut query = vec![
            ("title", title.as_str()),
            ("limit", "10"),
            ("fields", "key,title,subtitle,author_name,first_publish_year,cover_i,edition_count"),
        ];
        if let Some(author) = book.authors.first() {
            query.push(("author", author.as_str()));
        }
        let found: WorkSearch = self.ol_get("/search.json", &query).await?;
        let Some(doc) = best_work(found.docs, book, known) else { return Ok(None) };
        let Some(key) = doc.key.strip_prefix("/works/").filter(|k| is_work_id(k)) else { return Ok(None) };
        let work: Work = match self.ol_get(&format!("/works/{key}.json"), &[]).await {
            Err(ProviderError::NotFound) => return Ok(None),
            other => other?,
        };
        let description = work.description().map(|d| cut(&clean_description(d), MAX_SUMMARY)).filter(|d| !d.is_empty());
        Ok(Some((doc, description)))
    }

    /// `done`: the book is fetched; else it is left to be fetched again.
    #[allow(clippy::too_many_arguments)]
    async fn write_openlibrary(
        &self,
        id: i64,
        asin: Option<&str>,
        row: &BookRow,
        files: &FileFacts,
        doc: WorkDoc,
        description: Option<String>,
        done: bool,
    ) -> Result<(), ProviderError> {
        let key = doc.key.trim_start_matches("/works/").to_owned();
        let cover = doc.cover_i.filter(|c| *c > 0).map(|c| format!("{}/b/id/{c}-L.jpg", self.covers));
        let poster =
            if files.cover { None } else { artwork(&self.store, row.poster.as_deref(), cover.as_deref()).await };
        let authors = names(doc.author_name.iter().map(String::as_str));
        let year = doc.first_publish_year.filter(|y| (1..=9999).contains(y));
        let now = now();
        sqlx::query(
            "UPDATE items SET openlibrary_id = ?, audible_asin = ?, year = COALESCE(year, ?),
                summary = COALESCE(summary, ?), poster = COALESCE(?, poster),
                authors = CASE WHEN authors IS NULL OR authors IN ('[]', '[\"Unknown Author\"]')
                               THEN COALESCE(?, authors) ELSE authors END,
                metadata_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(key)
        .bind(asin)
        .bind(year)
        .bind(description)
        .bind(poster)
        .bind(authors)
        .bind(done.then_some(now))
        .bind(now)
        .bind(id)
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    // -------------------------------------------------------------- files

    async fn file_facts(&self, book: i64) -> sqlx::Result<FileFacts> {
        let infos: Vec<String> = sqlx::query_scalar(
            "SELECT f.media_info FROM media_files f JOIN items p ON p.id = f.item_id
             WHERE p.parent_id = ? AND f.media_info IS NOT NULL ORDER BY p.disc, p.idx LIMIT 5",
        )
        .bind(book)
        .fetch_all(&self.db.read)
        .await?;
        let mut out = FileFacts::default();
        for info in infos {
            let Ok(info) = serde_json::from_str::<PartInfo>(&info) else { continue };
            out.read = true;
            out.cover |= info.has_cover_art;
            out.asin = out.asin.or_else(|| asin_tag(&info.tags));
        }
        Ok(out)
    }

    /// Whether Audnexus and Open Library answer.
    pub async fn check(&self) -> Result<(), ProviderError> {
        // A known book: Audnexus has no search to ask instead.
        let request = self
            .audnexus
            .http
            .get(format!("{}/books/B08G9PRS1K", self.audnexus_base))
            .query(&[("region", DEFAULT_REGION)])
            .header(ACCEPT, "application/json");
        self.audnexus.json::<serde::de::IgnoredAny>(request).await?;
        self.ol_get::<serde::de::IgnoredAny>("/search.json", &[("title", "pley"), ("limit", "1"), ("fields", "key")])
            .await
            .map(|_| ())
    }
}

// ------------------------------------------------------------------ matching

/// Audnexus's region for a country: Audible sells in ten, and the United
/// States' region serves the rest.
pub fn audnexus_region(country: &str) -> &'static str {
    match country.to_ascii_uppercase().as_str() {
        "GB" | "UK" | "IE" => "uk",
        "CA" => "ca",
        "AU" | "NZ" => "au",
        "IN" => "in",
        "DE" | "AT" | "CH" | "LI" => "de",
        "FR" | "BE" | "LU" | "MC" => "fr",
        "IT" | "SM" => "it",
        "ES" => "es",
        "JP" => "jp",
        _ => DEFAULT_REGION,
    }
}

/// The ASIN in a file's tags, as Libation, OpenAudible and Audiobookshelf
/// write it.
fn asin_tag(tags: &HashMap<String, String>) -> Option<String> {
    ["asin", "audible_asin", "audibleasin", "cdek"]
        .iter()
        .find_map(|k| tags.get(*k))
        .map(|v| v.trim().to_ascii_uppercase())
        .filter(|v| is_asin(v))
}

/// A book's title as catalogs have it: without the "(Unabridged)" that rips
/// carry.
fn clean_title(title: &str) -> String {
    let mut t = title.trim().to_owned();
    loop {
        let lower = t.to_ascii_lowercase();
        let cut = [
            "(unabridged)",
            "[unabridged]",
            "unabridged",
            "(abridged)",
            "[abridged]",
            "(dramatized adaptation)",
            "(dramatised adaptation)",
        ]
        .iter()
        .find(|s| lower.ends_with(*s))
        .map(|s| s.len());
        match cut {
            Some(len) => t = t[..t.len() - len].trim_end_matches([' ', '-', ':', ',']).to_owned(),
            None => return t,
        }
    }
}

/// A credit with a role after a dash, as Audible lists translators among
/// the authors ("Juan Trejo - traductor").
fn is_credited_role(name: &str) -> bool {
    name.contains(" - ")
}

/// A name's words in order, so that "Tolkien, J. R. R." and "J.R.R. Tolkien"
/// are the same writer.
fn name_words(name: &str) -> Vec<String> {
    let name = name.split(" - ").next().unwrap_or(name);
    let mut words: Vec<String> = normalize_key(name).split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect();
    words.sort();
    words
}

/// The same writer, however the name is written: the same words, or one
/// name's two or more words all in the other ("Mary Shelley" and "Mary
/// Wollstonecraft Shelley").
fn same_person(a: &str, b: &str) -> bool {
    let (a, b) = (name_words(a), name_words(b));
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    !short.is_empty() && (a == b || (short.len() >= 2 && short.iter().all(|w| long.contains(w))))
}

fn any_same<'a>(ours: &[String], theirs: impl IntoIterator<Item = &'a str> + Clone) -> bool {
    ours.iter().any(|o| theirs.clone().into_iter().any(|t| same_person(o, t)))
}

/// A series' place ("6", "1.5", "Book 3"): its first number.
fn sequence(value: Option<&str>) -> Option<f64> {
    let value = value?;
    let start = value.find(|c: char| c.is_ascii_digit())?;
    let number: String = value[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    number.trim_end_matches('.').parse().ok().filter(|n: &f64| n.is_finite() && *n >= 0.0)
}

/// The Open Library work for a book: the same title (or one holding the
/// other) by one of its writers, or with no writer to go by, the same
/// title. The work it was matched to before (`known`) while it still fits;
/// else among works of the same name, the one with the most editions: the
/// book itself, not a translation or an adaptation catalogued apart.
fn best_work(docs: Vec<WorkDoc>, book: &Book, known: Option<&str>) -> Option<WorkDoc> {
    let wanted = clean_title(&book.title);
    let mut best: Option<(f64, WorkDoc)> = None;
    for doc in docs {
        let full = doc.subtitle.as_deref().map(|s| format!("{}: {s}", doc.title));
        // "Frankenstein; or, The Modern Prometheus" is "Frankenstein".
        let main = doc.title.split([';', ':']).next().map(str::trim).filter(|m| *m != doc.title);
        let titles = std::iter::once(doc.title.as_str()).chain(full.as_deref()).chain(main);
        let mut score = title_score(&wanted, titles);
        let key = doc.key.trim_start_matches("/works/");
        if score == 0.0 || !is_work_id(key) {
            continue;
        }
        if book.authors.is_empty() {
            // A title holding another names a different book too often.
            if score < 3.0 {
                continue;
            }
        } else {
            if !any_same(&book.authors, doc.author_name.iter().map(String::as_str)) {
                continue;
            }
            score += 2.0;
        }
        if known == Some(key) {
            return Some(doc);
        }
        // A work with a cover, and many editions, is the one people know.
        if doc.cover_i.is_some() {
            score += 0.5;
        }
        score += (doc.edition_count.unwrap_or(0).max(0) as f64 + 1.0).log10();
        if best.as_ref().is_none_or(|(b, _)| score > *b) {
            best = Some((score, doc));
        }
    }
    best.map(|(_, d)| d)
}

/// Text from a source, trimmed and at most `max` characters long.
fn cut(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect::<String>().trim_end().to_owned()
}

/// Writers' or narrators' names as the column keeps them: a JSON list, of
/// at most [`MAX_NAMES`]; none when no name is left.
fn names<'a>(found: impl Iterator<Item = &'a str>) -> Option<String> {
    let names: Vec<String> = found.map(|n| cut(n, MAX_TEXT)).filter(|n| !n.is_empty()).take(MAX_NAMES).collect();
    (!names.is_empty()).then(|| serde_json::to_string(&names).unwrap_or_default())
}

/// The day of Audnexus's release time ("2021-05-04T00:00:00.000Z").
fn release_day(value: Option<&str>) -> Option<&str> {
    value.map(|v| v.get(..10).unwrap_or(v))
}

/// Audnexus's genres, without its narrower tags.
fn genres(found: &[Genre]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for g in found.iter().filter(|g| g.kind.is_empty() || g.kind.eq_ignore_ascii_case("genre")) {
        let name = cut(&g.name, MAX_TEXT);
        if !name.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
        if out.len() == MAX_GENRES {
            break;
        }
    }
    out
}

/// Open Library's description as plain text: without the links and the
/// "([source][1])" notes contributors add below it, or Markdown's
/// `*emphasis*`.
fn clean_description(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        // Markdown link definitions ("[1]: https://…") and separators end it.
        if t.starts_with("----------") || (t.starts_with('[') && t.contains("]: http")) {
            break;
        }
        lines.push(line);
    }
    strip_html(&lines.join("\n").replace('*', ""))
}

/// An Amazon/Audible id: ten letters and digits ("B08G9PRS1K").
fn is_asin(value: &str) -> bool {
    value.len() == 10 && value.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// An Open Library work id: "OL21745884W".
fn is_work_id(value: &str) -> bool {
    value.len() > 3
        && value.starts_with("OL")
        && value.ends_with('W')
        && value[2..value.len() - 1].bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(title: &str, authors: &[&str]) -> Book {
        Book { title: title.into(), authors: authors.iter().map(|a| a.to_string()).collect() }
    }

    #[test]
    fn regions_follow_the_country() {
        assert_eq!(audnexus_region("GB"), "uk");
        assert_eq!(audnexus_region("nz"), "au");
        assert_eq!(audnexus_region("AT"), "de");
        assert_eq!(audnexus_region("US"), "us");
        assert_eq!(audnexus_region("BR"), "us");
        assert_eq!(audnexus_region(""), "us");
    }

    #[test]
    fn asins_from_the_tags_rippers_write() {
        let tags = |k: &str, v: &str| HashMap::from([(k.to_owned(), v.to_owned())]);
        assert_eq!(asin_tag(&tags("asin", " b08g9prs1k ")).as_deref(), Some("B08G9PRS1K"));
        assert_eq!(asin_tag(&tags("audible_asin", "B08G9PRS1K")).as_deref(), Some("B08G9PRS1K"));
        assert_eq!(asin_tag(&tags("cdek", "B08G9PRS1K")).as_deref(), Some("B08G9PRS1K"));
        assert_eq!(asin_tag(&tags("asin", "9780593135204")), None);
        assert_eq!(asin_tag(&tags("comment", "B08G9PRS1K")), None);
    }

    #[test]
    fn titles_lose_what_rips_add() {
        assert_eq!(clean_title("Dungeon Crawler Carl (Unabridged)"), "Dungeon Crawler Carl");
        assert_eq!(clean_title("Dune - Unabridged"), "Dune");
        assert_eq!(clean_title("Dune [Abridged] (Unabridged)"), "Dune");
        assert_eq!(clean_title("The Abridged Life"), "The Abridged Life");
    }

    #[test]
    fn writers_match_however_their_names_are_written() {
        assert!(same_person("Tolkien, J. R. R.", "J.R.R. Tolkien"));
        assert!(same_person("Andy Weir", "andy weir"));
        assert!(same_person("Matt Dinniman", "Matt Dinniman - foreword"));
        assert!(same_person("Mary Shelley", "Mary Wollstonecraft Shelley"));
        assert!(!same_person("Andy Weir", "Andy Warhol"));
        assert!(!same_person("Shelley", "Mary Shelley"));
        assert!(!same_person("", ""));
    }

    #[test]
    fn places_in_a_series() {
        assert_eq!(sequence(Some("6")), Some(6.0));
        assert_eq!(sequence(Some("1.5")), Some(1.5));
        assert_eq!(sequence(Some("Book 3")), Some(3.0));
        assert_eq!(sequence(Some("2.")), Some(2.0));
        assert_eq!(sequence(Some("Prequel")), None);
        assert_eq!(sequence(None), None);
    }

    #[test]
    fn audnexus_books_and_their_genres() {
        let found: AudnexusBook = serde_json::from_value(serde_json::json!({
            "asin": "B0LANTERN1", "title": "The Lantern Sea",
            "authors": [{"asin": "B0QUILL", "name": "Ada Quill"}], "narrators": [{"name": "June Ashby"}],
            "seriesPrimary": {"asin": "B0TIDE", "name": "Tidewardens", "position": "2"},
            "genres": [
                {"asin": "1", "name": "Science Fiction & Fantasy", "type": "genre"},
                {"asin": "2", "name": "Fantasy", "type": "genre"},
                {"asin": "3", "name": "Sea Adventures", "type": "tag"}
            ],
            "releaseDate": "2019-03-05T00:00:00.000Z", "runtimeLengthMin": "612", "summary": "<p>A tale.</p>",
            "image": "https://example.invalid/x.jpg"
        }))
        .unwrap();
        assert_eq!(found.series_primary.as_ref().map(|s| s.name.as_str()), Some("Tidewardens"));
        assert_eq!(sequence(found.series_primary.as_ref().and_then(|s| s.position.as_deref())), Some(2.0));
        assert_eq!(genres(&found.genres), ["Science Fiction & Fantasy", "Fantasy"]);
        let released = release_day(found.release_date.as_deref());
        assert_eq!(year_of(released), Some(2019));
        assert_eq!(full_date(released).as_deref(), Some("2019-03-05"));
    }

    #[test]
    fn open_library_works_by_the_books_writer() {
        let docs: Vec<WorkDoc> = serde_json::from_value(serde_json::json!([
            {"key": "/works/OL1W", "title": "Small Hours", "author_name": ["Someone Else"], "cover_i": 9},
            {"key": "/works/OL2W", "title": "Small Hours", "author_name": ["Imogen Vale"]},
            {"key": "/works/OL3W", "title": "Small Hours", "author_name": ["Imogen Vale"], "cover_i": 7},
            {"key": "/books/OL4M", "title": "Small Hours", "author_name": ["Imogen Vale"], "cover_i": 7}
        ]))
        .unwrap();
        assert_eq!(best_work(docs, &book("Small Hours", &["Imogen Vale"]), None).unwrap().key, "/works/OL3W");
        // The work itself, under its full title, over a later one of the
        // same short name with fewer editions.
        let docs = serde_json::json!([
            {"key": "/works/OL7W", "title": "Small Hours", "author_name": ["Imogen Vale"], "cover_i": 1,
             "edition_count": 40},
            {"key": "/works/OL6W", "title": "Small Hours; or, A Porter's Year", "author_name": ["Imogen Vale"],
             "cover_i": 2, "edition_count": 900}
        ]);
        let pick = |docs: &serde_json::Value, book: &Book, known: Option<&str>| {
            best_work(serde_json::from_value(docs.clone()).unwrap(), book, known).map(|d| d.key)
        };
        let vale = book("Small Hours", &["Imogen Vale"]);
        assert_eq!(pick(&docs, &vale, None).as_deref(), Some("/works/OL6W"));
        // The work matched before, while it still fits; else the best.
        assert_eq!(pick(&docs, &vale, Some("OL7W")).as_deref(), Some("/works/OL7W"));
        assert_eq!(pick(&docs, &book("Small Hours", &["Someone Else"]), Some("OL7W")), None);
        let other: Vec<WorkDoc> = serde_json::from_value(serde_json::json!([
            {"key": "/works/OL5W", "title": "Small Hours", "author_name": ["Someone Else"]}
        ]))
        .unwrap();
        assert!(best_work(other, &vale, None).is_none());
        // With no writer to go by, only the same title.
        let collected = serde_json::json!([
            {"key": "/works/OL8W", "title": "The Collected Small Hours", "author_name": ["Imogen Vale"],
             "edition_count": 300},
            {"key": "/works/OL9W", "title": "Small Hours", "author_name": ["Imogen Vale"]}
        ]);
        assert_eq!(pick(&collected, &book("Small Hours", &[]), None).as_deref(), Some("/works/OL9W"));
        assert_eq!(pick(&collected, &book("Collected", &[]), None), None);
    }

    #[test]
    fn descriptions_and_ids() {
        assert_eq!(
            clean_description(
                "A keeper's tale.\r\n\r\n----------\r\nAlso contained in: [Collected][1]\r\n[1]: https://x"
            ),
            "A keeper's tale."
        );
        assert_eq!(clean_description("*Small Hours* is a **quiet** novel."), "Small Hours is a quiet novel.");
        let typed: Work = serde_json::from_str(r#"{"description": {"type": "/type/text", "value": "Text."}}"#).unwrap();
        assert_eq!(typed.description(), Some("Text."));
        let plain: Work = serde_json::from_str(r#"{"description": "Plain."}"#).unwrap();
        assert_eq!(plain.description(), Some("Plain."));
        // Another shape is no description, not a book that can't be read.
        for odd in [r#"{"description": {"type": "/type/text"}}"#, r#"{"description": 7}"#, r#"{"description": null}"#] {
            assert_eq!(serde_json::from_str::<Work>(odd).unwrap().description(), None, "{odd}");
        }
        assert_eq!(cut("  a long name  ", 6), "a long");
        assert_eq!(cut(&"é".repeat(MAX_TEXT + 10), MAX_TEXT).chars().count(), MAX_TEXT);
        let many: Vec<String> = (0..MAX_NAMES + 5).map(|n| format!("Reader {n}")).collect();
        let kept: Vec<String> = serde_json::from_str(&names(many.iter().map(String::as_str)).unwrap()).unwrap();
        assert_eq!(kept.len(), MAX_NAMES);
        assert_eq!(names(["", "  "].into_iter()), None);
        assert!(is_asin("B08G9PRS1K") && !is_asin("b08g9prs1k") && !is_asin("B08G9PRS1"));
        assert!(is_work_id("OL21745884W") && !is_work_id("OL21745884M") && !is_work_id("OLW"));
        assert!(is_credited_role("Juan Trejo - traductor") && !is_credited_role("Jean-Luc Picard"));
    }
}
