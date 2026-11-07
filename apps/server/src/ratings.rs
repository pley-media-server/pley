//! Content ratings as minimum ages, for per-user age limits.
//!
//! TMDB delivers certifications of many countries ("PG-13", "TV-MA", "FSK 16",
//! "12A", "MA15+"); they are mapped to the youngest age they allow so one
//! number per user ("up to 12") works for all of them. Some ratings mean
//! different ages in different countries ("A" is for all ages in Denmark and
//! for adults in India), so a rating is read with its country's ratings
//! ([`SYSTEMS`]) when its country is known, else the metadata region's, and
//! only then with the table below, which knows the US's names and numbers.

mod systems;

use crate::db::Db;

pub use systems::SYSTEMS;

/// Bump with any change to [`SYSTEMS`] or [`rating_age`]: [`backfill`] then
/// works out every item's age again.
pub const VERSION: i64 = 1;

/// Films or TV: some countries rate them on different scales (Canada's PG
/// is 10 for a film and 8 on TV).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Film,
    Tv,
}

impl Kind {
    /// The kind of ratings an item of `kind` carries: shows' and their
    /// seasons' and episodes' are TV's.
    pub fn of(kind: &str) -> Kind {
        match kind {
            "show" | "season" | "episode" => Kind::Tv,
            _ => Kind::Film,
        }
    }
}

/// One rating of a country: its names (the board's, then other spellings,
/// TMDB's among them) and the youngest age it allows.
#[derive(Debug)]
pub struct Rating {
    pub names: &'static [&'static str],
    pub age: i64,
    /// `None`: for films and TV alike.
    pub kind: Option<Kind>,
}

/// A country's ratings, youngest first.
#[derive(Debug)]
pub struct System {
    /// ISO 3166-1 alpha-2.
    pub country: &'static str,
    pub name: &'static str,
    pub ratings: &'static [Rating],
}

impl Rating {
    /// The name to show: the board's own, or the shortest when that's a
    /// sentence (Denmark's "Tilladt for alle" is "A").
    pub fn short_name(&self) -> &'static str {
        let first = self.names[0];
        if first.chars().count() <= 12 {
            return first;
        }
        self.names.iter().copied().min_by_key(|n| n.chars().count()).unwrap_or(first)
    }
}

impl System {
    /// The age of `rating` in this country: a rating of the kind first, then
    /// one of the other kind (TMDB gives some TV shows a film rating).
    pub fn age(&self, kind: Kind, rating: &str) -> Option<i64> {
        let named = |r: &&Rating| r.names.iter().any(|n| same(n, rating));
        let found = self.ratings.iter().filter(named).find(|r| r.kind.is_none_or(|k| k == kind));
        found.or_else(|| self.ratings.iter().find(named)).map(|r| r.age)
    }
}

/// The ratings of `country` (ISO 3166-1 alpha-2, any case).
pub fn system(country: &str) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.country.eq_ignore_ascii_case(country))
}

/// The age `rating` stands for, `None` when unknown or unrated. `country`
/// is the rating's, or else the metadata region's; a rating written with
/// its country (an NFO file's "GB:15", Plex's "gb/12A") is read as that
/// country's whatever `country` says.
pub fn age(country: Option<&str>, kind: Kind, rating: &str) -> Option<i64> {
    let rating = without_rated(rating);
    let (written, rating) = with_country(rating);
    written.or(country).and_then(system).and_then(|s| s.age(kind, rating)).or_else(|| rating_age(rating))
}

/// The country (ISO 3166-1 alpha-2) of a metadata language: "GB" for
/// "en-GB", the US's for a language with no country.
pub fn region_of(language: &str) -> String {
    language.split(['-', '_']).nth(1).unwrap_or("US").to_ascii_uppercase()
}

/// The metadata region, read from the settings as they're stored.
pub async fn region(db: &Db) -> sqlx::Result<String> {
    let language: Option<String> = sqlx::query_scalar(
        "SELECT CASE WHEN json_valid(value) THEN json_extract(value, '$.metadata_language') END
             FROM settings WHERE key = 'server'",
    )
    .fetch_optional(&db.read)
    .await?
    .flatten();
    Ok(region_of(language.as_deref().unwrap_or("en-US")))
}

/// Names compare without regard to case or the spaces around them.
fn same(name: &str, rating: &str) -> bool {
    name.trim().to_lowercase() == rating.trim().to_lowercase()
}

/// Kodi's "Rated PG-13" is "PG-13".
fn without_rated(rating: &str) -> &str {
    let rating = rating.trim();
    match rating.get(..6) {
        Some(prefix) if prefix.eq_ignore_ascii_case("rated ") => rating[6..].trim(),
        _ => rating,
    }
}

/// "GB:15", "gb/12A" and "USA:R": the country written before a rating, when
/// it's one with ratings here (Portugal's "M/12" and Belgium's "KT/EA" are
/// ratings, not countries).
pub fn with_country(rating: &str) -> (Option<&'static str>, &str) {
    let Some((before, after)) = rating.split_once([':', '/']) else { return (None, rating) };
    let before = before.trim();
    let country = if before.eq_ignore_ascii_case("usa") { Some("US") } else { system(before).map(|s| s.country) };
    match country {
        Some(country) if !after.trim().is_empty() => (Some(country), without_rated(after)),
        _ => (None, rating),
    }
}

/// The age of a rating read without its country's ratings: US names, then
/// the first number in it. `None` when unknown or unrated.
fn rating_age(rating: &str) -> Option<i64> {
    let r = rating.trim().to_ascii_uppercase();
    let r = r.strip_prefix("RATED ").unwrap_or(&r).trim();
    let known = match r {
        // United States (MPA, TV Parental Guidelines)
        "G" | "TV-Y" | "TV-G" | "E" | "AL" | "ALL" | "U" | "TP" | "0+" => Some(0),
        "TV-Y7" | "TV-Y7-FV" => Some(7),
        "PG" | "TV-PG" => Some(10),
        "PG-13" | "PG13" => Some(13),
        "TV-14" | "14A" | "14+" => Some(14),
        "R" | "TV-MA" => Some(17),
        "NC-17" | "NC17" | "X" | "XXX" | "R18" | "R18+" | "R 18+" | "18A" | "18+" | "A" | "M18" => Some(18),
        // United Kingdom, Ireland, Australia, New Zealand
        "12A" | "12" | "PG-12" => Some(12),
        "15" | "15A" | "M" | "MA15+" | "MA 15+" => Some(15),
        "NR" | "UNRATED" | "NOT RATED" | "N/A" | "-" | "" => None,
        _ => None,
    };
    if known.is_some() {
        return known;
    }
    // "FSK 16", "FSK16", "PEGI 12", "16", "-12", "12+", "TV-14" style numbers.
    let digits: String = r.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok().filter(|age| (0..=21).contains(age))
}

/// Work out the items' ages: those without one (stored before ages
/// existed), and all of them when the ratings tables ([`VERSION`]) or the
/// metadata region (the country of ratings with none of their own) changed
/// since the last time.
///
/// The first time, ratings were all stored without their country, though
/// many are the United States' (TMDB's when the region had none, an NFO
/// file's "US:PG"): read as the region's, some would allow younger viewers
/// than before (the UK's PG is 8, the US's 10). Their ages then never go
/// down, and NFO files are read again at the next scan to learn their
/// countries; a refresh from TMDB or TheTVDB learns theirs.
pub async fn backfill(db: &Db) -> sqlx::Result<()> {
    let region = region(db).await?;
    let stamp = format!("{VERSION}:{region}");
    let done: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'rating_ages'").fetch_optional(&db.read).await?;
    let all = done.as_deref() != Some(stamp.as_str());
    let first = done.is_none();
    /// An item's id, kind, rating, rating's country and age.
    type Rated = (i64, String, String, Option<String>, Option<i64>);
    let rows: Vec<Rated> = sqlx::query_as(
        "SELECT id, kind, content_rating, content_rating_country, rating_age FROM items
         WHERE content_rating IS NOT NULL AND content_rating != '' AND (? OR rating_age IS NULL)",
    )
    .bind(all)
    .fetch_all(&db.read)
    .await?;
    let ages: Vec<(i64, Option<i64>)> = rows
        .iter()
        .filter_map(|(id, kind, rating, country, old)| {
            let mut age = age(Some(country.as_deref().unwrap_or(&region)), Kind::of(kind), rating);
            if first && country.is_none() {
                age = old.map_or(age, |old| Some(age.map_or(old, |age| age.max(old))));
            }
            (age != *old).then_some((*id, age))
        })
        .collect();
    let mut tx = db.write.begin().await?;
    for (id, age) in &ages {
        sqlx::query("UPDATE items SET rating_age = ? WHERE id = ?").bind(age).bind(id).execute(&mut *tx).await?;
    }
    if first {
        // NFO fields read before ratings had countries: a size no file has
        // makes the next scan read the file again.
        sqlx::query(
            "UPDATE item_nfo SET size = -1
             WHERE json_valid(fields) AND json_extract(fields, '$.content_rating') IS NOT NULL
               AND json_extract(fields, '$.content_ratings') IS NULL",
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('rating_ages', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(&stamp)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if !ages.is_empty() {
        tracing::debug!("worked out the age ratings of {} items", ages.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certifications_map_to_ages() {
        for (rating, age) in [
            ("G", Some(0)),
            ("PG", Some(10)),
            ("PG-13", Some(13)),
            ("R", Some(17)),
            ("NC-17", Some(18)),
            ("TV-Y7", Some(7)),
            ("TV-14", Some(14)),
            ("TV-MA", Some(17)),
            ("12A", Some(12)),
            ("FSK 16", Some(16)),
            ("16", Some(16)),
            ("MA15+", Some(15)),
            ("Rated PG-13", Some(13)),
            ("tv-pg", Some(10)),
            ("NR", None),
            ("Unrated", None),
            ("", None),
            ("Banned", None),
        ] {
            assert_eq!(rating_age(rating), age, "{rating}");
        }
    }

    #[test]
    fn certifications_of_other_countries() {
        for (rating, age) in [
            // United Kingdom
            ("U", Some(0)),
            ("15", Some(15)),
            ("18", Some(18)),
            ("R18", Some(18)),
            // Germany
            ("FSK 0", Some(0)),
            ("FSK18", Some(18)),
            // France
            ("-12", Some(12)),
            ("-16", Some(16)),
            ("TP", Some(0)),
            // Australia
            ("M", Some(15)),
            ("MA 15+", Some(15)),
            ("R 18+", Some(18)),
            ("X 18+", Some(18)),
            // Canada
            ("14A", Some(14)),
            ("18A", Some(18)),
            ("C8", Some(8)),
            // United States, other spellings
            ("TV-Y7-FV", Some(7)),
            ("NC17", Some(18)),
            ("  pg-13 ", Some(13)),
            ("rated r", Some(17)),
            // Numbers outside any age are not ages.
            ("21+", Some(21)),
            ("99", None),
            ("Not Rated", None),
            ("N/A", None),
            ("-", None),
        ] {
            assert_eq!(rating_age(rating), age, "{rating}");
        }
    }

    /// The ratings of the same name in several countries.
    #[test]
    fn ratings_are_read_with_their_country() {
        for (country, kind, rating, expected) in [
            ("DK", Kind::Film, "A", Some(0)),
            ("ES", Kind::Film, "A", Some(0)),
            ("MX", Kind::Film, "A", Some(0)),
            ("IN", Kind::Film, "A", Some(18)),
            ("GB", Kind::Film, "PG", Some(8)),
            ("US", Kind::Film, "PG", Some(10)),
            ("US", Kind::Film, "GP", Some(10)),
            ("CA", Kind::Film, "PG", Some(10)),
            ("CA", Kind::Tv, "PG", Some(8)),
            ("AU", Kind::Film, "PG", Some(10)),
            ("AU", Kind::Film, "M", Some(15)),
            ("gb", Kind::Film, "12a", Some(12)),
            // Not one of the country's: the shared table.
            ("GB", Kind::Film, "PG-13", Some(13)),
            ("DK", Kind::Film, "Unrated", None),
        ] {
            assert_eq!(age(Some(country), kind, rating), expected, "{country} {rating} {kind:?}");
        }
        // No country known: the shared table.
        assert_eq!(age(None, Kind::Film, "PG"), Some(10));
        assert_eq!(age(Some("ZZ"), Kind::Film, "PG"), Some(10));
    }

    /// A country written before the rating wins over the one given.
    #[test]
    fn ratings_written_with_their_country() {
        assert_eq!(age(Some("US"), Kind::Film, "GB:15"), Some(15));
        assert_eq!(age(Some("US"), Kind::Film, "gb/12A"), Some(12));
        assert_eq!(age(Some("IN"), Kind::Film, "dk:A"), Some(0));
        assert_eq!(age(Some("DK"), Kind::Film, "USA:R"), Some(17));
        assert_eq!(age(Some("DK"), Kind::Film, "Rated USA:R"), Some(17));
        // Ratings with a colon or slash in them aren't countries.
        assert_eq!(with_country("M/12"), (None, "M/12"));
        assert_eq!(with_country("KT/EA"), (None, "KT/EA"));
        assert_eq!(with_country("GB:"), (None, "GB:"));
    }

    /// A board's sentence is shown by its short name.
    #[test]
    fn ratings_have_short_names() {
        let named = |country: &str, age: i64| {
            system(country).unwrap().ratings.iter().find(|r| r.age == age).unwrap().short_name()
        };
        assert_eq!(named("DK", 0), "A");
        assert_eq!(named("DK", 7), "7");
        assert_eq!(named("GB", 12), "12");
        assert_eq!(named("FR", 0), "Tous publics");
        for system in SYSTEMS {
            for rating in system.ratings {
                assert!(rating.short_name().chars().count() <= 16, "{}: {}", system.country, rating.short_name());
            }
        }
    }

    #[test]
    fn regions_of_metadata_languages() {
        assert_eq!(region_of("en-GB"), "GB");
        assert_eq!(region_of("pt_br"), "BR");
        assert_eq!(region_of("fr"), "US");
    }

    /// Within a country, a name means one age for a kind, ages are ages,
    /// and each system's ratings go from the youngest up.
    #[test]
    fn systems_are_consistent() {
        let mut countries = Vec::new();
        for system in SYSTEMS {
            assert_eq!(system.country.len(), 2, "{}", system.country);
            assert_eq!(system.country, system.country.to_ascii_uppercase());
            assert!(!countries.contains(&system.country), "{} twice", system.country);
            countries.push(system.country);
            for kind in [Kind::Film, Kind::Tv] {
                let of_kind: Vec<&Rating> =
                    system.ratings.iter().filter(|r| r.kind.is_none_or(|k| k == kind)).collect();
                for (i, rating) in of_kind.iter().enumerate() {
                    assert!((0..=21).contains(&rating.age), "{} {:?}", system.country, rating.names);
                    assert!(!rating.names.is_empty());
                    for other in &of_kind[i + 1..] {
                        for name in rating.names {
                            assert!(
                                !other.names.iter().any(|n| same(n, name)),
                                "{}: {name} twice for {kind:?}",
                                system.country
                            );
                        }
                    }
                }
                assert!(
                    of_kind.windows(2).all(|w| w[0].age <= w[1].age),
                    "{} {kind:?}: youngest first",
                    system.country
                );
            }
        }
    }

    async fn database() -> (tempfile::TempDir, Db) {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'Movies', 'movies', '[]', 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        (dir, db)
    }

    async fn insert(db: &Db, id: i64, rating: Option<&str>, country: Option<&str>, age: Option<i64>) {
        sqlx::query(
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, content_rating, content_rating_country,
                                rating_age, added_at, updated_at)
             VALUES (?, 1, 'movie', ?, 't', 't', ?, ?, ?, 0, 0)",
        )
        .bind(id)
        .bind(id.to_string())
        .bind(rating)
        .bind(country)
        .bind(age)
        .execute(&db.write)
        .await
        .unwrap();
    }

    async fn ages(db: &Db) -> Vec<Option<i64>> {
        sqlx::query_scalar("SELECT rating_age FROM items ORDER BY id").fetch_all(&db.read).await.unwrap()
    }

    async fn set_language(db: &Db, language: &str) {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ('server', json_object('metadata_language', ?))
             ON CONFLICT(key) DO UPDATE SET value = json_set(value, '$.metadata_language', ?)",
        )
        .bind(language)
        .bind(language)
        .execute(&db.write)
        .await
        .unwrap();
    }

    /// Once the ages are worked out with these tables and region, only
    /// missing ones are filled.
    #[tokio::test]
    async fn backfill_fills_missing_ages_only() {
        let (_dir, db) = database().await;
        backfill(&db).await.unwrap();
        for (id, rating, age) in
            [(1, Some("PG-13"), None), (2, Some("R"), Some(3)), (3, Some("Unrated"), None), (4, None, None)]
        {
            insert(&db, id, rating, None, age).await;
        }
        backfill(&db).await.unwrap();
        assert_eq!(
            ages(&db).await,
            [Some(13), Some(3), None, None],
            "existing ages are kept, unknown ratings stay unrated"
        );
    }

    /// The first time, ratings stored without their country keep the age
    /// they had when the region's ratings would lower it, and NFO files
    /// read before ratings had countries are read again.
    #[tokio::test]
    async fn backfill_after_an_upgrade_never_lowers_ages() {
        let (_dir, db) = database().await;
        set_language(&db, "en-GB").await;
        insert(&db, 1, Some("PG"), None, Some(10)).await;
        insert(&db, 2, Some("12A"), None, None).await;
        insert(&db, 3, Some("PG"), None, Some(6)).await;
        insert(&db, 4, Some("PG"), Some("GB"), Some(10)).await;
        for (id, fields) in [(1, r#"{"content_rating":"PG"}"#), (4, r#"{"content_rating":"PG","content_ratings":[]}"#)]
        {
            sqlx::query(
                "INSERT INTO item_nfo (item_id, path, size, mtime, ids, fields) VALUES (?, 'x.nfo', 5, 5, '{}', ?)",
            )
            .bind(id)
            .bind(fields)
            .execute(&db.write)
            .await
            .unwrap();
        }
        backfill(&db).await.unwrap();
        assert_eq!(ages(&db).await, [Some(10), Some(12), Some(8), Some(8)], "the US's PG stays 10; GB's is 8");
        let sizes: Vec<i64> =
            sqlx::query_scalar("SELECT size FROM item_nfo ORDER BY item_id").fetch_all(&db.read).await.unwrap();
        assert_eq!(sizes, [-1, 5], "read again: only the file without countries");
        // Afterwards, ages follow the tables and the region as usual.
        set_language(&db, "en-US").await;
        backfill(&db).await.unwrap();
        set_language(&db, "en-GB").await;
        backfill(&db).await.unwrap();
        assert_eq!(ages(&db).await[0], Some(8));
    }

    /// New tables, or another region, work every age out again, each with
    /// its rating's country, else the region's.
    #[tokio::test]
    async fn backfill_reads_ratings_with_their_country() {
        let (_dir, db) = database().await;
        backfill(&db).await.unwrap();
        insert(&db, 1, Some("A"), Some("DK"), Some(18)).await;
        insert(&db, 2, Some("A"), Some("IN"), None).await;
        insert(&db, 3, Some("PG"), None, Some(10)).await;
        sqlx::query("UPDATE settings SET value = '0:US' WHERE key = 'rating_ages'").execute(&db.write).await.unwrap();
        backfill(&db).await.unwrap();
        assert_eq!(ages(&db).await, [Some(0), Some(18), Some(10)], "new tables: every age again");
        set_language(&db, "en-GB").await;
        backfill(&db).await.unwrap();
        assert_eq!(ages(&db).await, [Some(0), Some(18), Some(8)], "a British server: PG is 8");
    }
}
