//! Content ratings as minimum ages, for per-user age limits.
//!
//! TMDB delivers certifications of many countries ("PG-13", "TV-MA", "FSK 16",
//! "12A", "MA15+"); they are mapped to the youngest age they allow so one
//! number per user ("up to 12") works for all of them.

use crate::db::Db;

/// Minimum viewer age for a certification, `None` when unknown or unrated.
pub fn rating_age(rating: &str) -> Option<i64> {
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

/// Fill `rating_age` for items whose rating was stored before ages existed
/// (or whose certification is new). Cheap: only rated items without an age.
pub async fn backfill(db: &Db) -> sqlx::Result<()> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, content_rating FROM items WHERE content_rating IS NOT NULL AND content_rating != '' AND rating_age IS NULL",
    )
    .fetch_all(&db.read)
    .await?;
    let ages: Vec<(i64, i64)> = rows.iter().filter_map(|(id, r)| rating_age(r).map(|age| (*id, age))).collect();
    if ages.is_empty() {
        return Ok(());
    }
    let mut tx = db.write.begin().await?;
    for (id, age) in &ages {
        sqlx::query("UPDATE items SET rating_age = ? WHERE id = ?").bind(age).bind(id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    tracing::debug!("computed age ratings for {} items", ages.len());
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

    #[tokio::test]
    async fn backfill_fills_missing_ages_only() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        sqlx::query(
            "INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'Movies', 'movies', '[]', 0)",
        )
        .execute(&db.write)
        .await
        .unwrap();
        for (id, rating, age) in
            [(1, Some("PG-13"), None), (2, Some("R"), Some(3)), (3, Some("Unrated"), None), (4, None, None)]
        {
            sqlx::query(
                "INSERT INTO items (id, library_id, kind, key, title, sort_title, content_rating, rating_age, added_at, updated_at)
                 VALUES (?, 1, 'movie', ?, 't', 't', ?, ?, 0, 0)",
            )
            .bind(id)
            .bind(id.to_string())
            .bind(rating)
            .bind(age)
            .execute(&db.write)
            .await
            .unwrap();
        }
        backfill(&db).await.unwrap();
        let ages: Vec<Option<i64>> =
            sqlx::query_scalar("SELECT rating_age FROM items ORDER BY id").fetch_all(&db.read).await.unwrap();
        assert_eq!(ages, [Some(13), Some(3), None, None], "existing ages are kept, unknown ratings stay unrated");
    }
}
