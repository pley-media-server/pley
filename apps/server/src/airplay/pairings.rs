//! The AirPlay devices the server paired with, in the database: what
//! pair-verify needs of each (its pairing id and public key), and what the
//! administrators' list shows.

use serde::Serialize;

use super::pairing::Paired;
use crate::db::Db;

/// A device's pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Pairing {
    /// The id the device announces, without its protocol's prefix.
    pub device_id: String,
    #[serde(skip)]
    pub pairing_id: String,
    #[serde(skip)]
    pub public_key: Vec<u8>,
    /// Its name when it was paired.
    pub name: String,
    /// Who typed the PIN, while their account exists.
    pub paired_by: Option<String>,
    pub paired_at: i64,
}

macro_rules! select {
    ($rest:literal) => {
        concat!(
            "SELECT p.device_id, p.pairing_id, p.public_key, p.name,
                 COALESCE(u.display_name, u.username) AS paired_by, p.paired_at
             FROM airplay_pairings p LEFT JOIN users u ON u.id = p.paired_by ",
            $rest
        )
    };
}

pub async fn get(db: &Db, device_id: &str) -> sqlx::Result<Option<Pairing>> {
    sqlx::query_as(select!("WHERE p.device_id = ?")).bind(device_id).fetch_optional(&db.read).await
}

pub async fn list(db: &Db) -> sqlx::Result<Vec<Pairing>> {
    sqlx::query_as(select!("ORDER BY p.name COLLATE NOCASE")).fetch_all(&db.read).await
}

/// Keeps a new pairing with the device, in place of any before it.
pub async fn save(db: &Db, device_id: &str, paired: &Paired, name: &str, user_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO airplay_pairings (device_id, pairing_id, public_key, name, paired_by, paired_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (device_id) DO UPDATE SET pairing_id = excluded.pairing_id,
             public_key = excluded.public_key, name = excluded.name,
             paired_by = excluded.paired_by, paired_at = excluded.paired_at",
    )
    .bind(device_id)
    .bind(&paired.id)
    .bind(&paired.public_key)
    .bind(name)
    .bind(user_id)
    .bind(crate::util::now())
    .execute(&db.write)
    .await
    .map(|_| ())
}

/// Forgets the pairing: whether there was one.
pub async fn remove(db: &Db, device_id: &str) -> sqlx::Result<bool> {
    let done =
        sqlx::query("DELETE FROM airplay_pairings WHERE device_id = ?").bind(device_id).execute(&db.write).await?;
    Ok(done.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_pairing_is_kept_replaced_and_removed_and_outlives_its_maker() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash, created_at) VALUES (7, 'sam', '', 0)")
            .execute(&db.write)
            .await
            .unwrap();
        let paired = Paired { id: "AA:BB".into(), public_key: vec![1; 32] };
        save(&db, "aabb", &paired, "Den", 7).await.unwrap();
        let again = Paired { id: "AA:BB".into(), public_key: vec![2; 32] };
        save(&db, "aabb", &again, "Den TV", 7).await.unwrap();
        let kept = get(&db, "aabb").await.unwrap().unwrap();
        assert_eq!(
            (kept.public_key, kept.name.as_str(), kept.paired_by.as_deref()),
            (vec![2; 32], "Den TV", Some("sam"))
        );
        sqlx::query("DELETE FROM users WHERE id = 7").execute(&db.write).await.unwrap();
        assert_eq!(list(&db).await.unwrap()[0].paired_by, None);
        assert!(remove(&db, "aabb").await.unwrap());
        assert!(!remove(&db, "aabb").await.unwrap());
        assert_eq!(get(&db, "aabb").await.unwrap(), None);
    }
}
