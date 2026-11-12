//! Cast and crew. Credits come from the metadata provider along with the rest
//! of the metadata (TMDB, or TVmaze or TheTVDB for TV shows); people are
//! shared between items, and a person's photo is downloaded into the blob
//! store ([`crate::store`]) the first time someone looks at it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::catalog::{self, ItemSummary};
use crate::db::Db;
use crate::util::now;

/// Billed cast kept for a movie or a show.
const MAX_CAST: usize = 40;
/// Guest stars kept for an episode.
const MAX_GUESTS: usize = 15;
/// Producers kept for a movie (big productions list dozens).
const MAX_PRODUCERS: usize = 6;

// ------------------------------------------------------------------ TMDB

/// `credits` of a movie (`append_to_response=credits`).
#[derive(Debug, Default, Deserialize)]
pub struct Credits {
    #[serde(default)]
    pub cast: Vec<CastCredit>,
    #[serde(default)]
    pub crew: Vec<CrewCredit>,
}

#[derive(Debug, Deserialize)]
pub struct CastCredit {
    pub id: i64,
    pub name: String,
    pub character: Option<String>,
    pub profile_path: Option<String>,
    #[serde(default)]
    pub order: i64,
}

#[derive(Debug, Deserialize)]
pub struct CrewCredit {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub job: String,
    #[serde(default)]
    pub department: String,
    pub profile_path: Option<String>,
}

/// `aggregate_credits` of a show: everyone in any episode, with every part
/// they played.
#[derive(Debug, Default, Deserialize)]
pub struct AggregateCredits {
    #[serde(default)]
    pub cast: Vec<AggregateCast>,
}

#[derive(Debug, Deserialize)]
pub struct AggregateCast {
    pub id: i64,
    pub name: String,
    pub profile_path: Option<String>,
    #[serde(default)]
    pub roles: Vec<AggregateRole>,
    #[serde(default)]
    pub order: i64,
}

#[derive(Debug, Deserialize)]
pub struct AggregateRole {
    pub character: Option<String>,
    #[serde(default)]
    pub episode_count: i64,
}

/// `created_by` of a show.
#[derive(Debug, Deserialize)]
pub struct Creator {
    pub id: i64,
    pub name: String,
    pub profile_path: Option<String>,
}

// ------------------------------------------------------------------ credits

/// Ordered as shown: cast, then crew.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Actor,
    Creator,
    Director,
    Writer,
    Producer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Actor => "actor",
            Role::Creator => "creator",
            Role::Director => "director",
            Role::Writer => "writer",
            Role::Producer => "producer",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "actor" => Role::Actor,
            "creator" => Role::Creator,
            "director" => Role::Director,
            "writer" => Role::Writer,
            "producer" => Role::Producer,
            _ => return None,
        })
    }
}

/// A person's id at the provider their credit comes from. People are stored
/// once per provider id; the same actor from two providers is two people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersonId {
    Tmdb(i64),
    Tvmaze(i64),
    Tvdb(i64),
}

impl PersonId {
    /// The `people` column holding this kind of id, and the id.
    fn column(self) -> (&'static str, i64) {
        match self {
            PersonId::Tmdb(id) => ("tmdb_id", id),
            PersonId::Tvmaze(id) => ("tvmaze_id", id),
            PersonId::Tvdb(id) => ("tvdb_id", id),
        }
    }
}

/// A credit to store for an item.
#[derive(Debug, Clone, PartialEq)]
pub struct NewCredit {
    pub person: PersonId,
    pub name: String,
    /// Full URL of the person's photo.
    pub photo: Option<String>,
    pub role: Role,
    pub character: Option<String>,
}

fn clean(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned)
}

/// Full-size URL of a TMDB profile image path ("/abc.jpg").
pub fn tmdb_photo(profile_path: Option<&str>) -> Option<String> {
    let path = clean(profile_path)?;
    let name = path.strip_prefix('/')?;
    let (stem, ext) = name.rsplit_once('.')?;
    let plain = !stem.is_empty()
        && stem.len() <= 64
        && stem.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && matches!(ext, "jpg" | "jpeg" | "png" | "webp");
    plain.then(|| format!("https://image.tmdb.org/t/p/h632{path}"))
}

fn actor(c: &CastCredit) -> NewCredit {
    NewCredit {
        person: PersonId::Tmdb(c.id),
        name: c.name.clone(),
        photo: tmdb_photo(c.profile_path.as_deref()),
        role: Role::Actor,
        character: clean(c.character.as_deref()),
    }
}

/// Directors, writers and the first producers, in that order.
fn crew(crew: &[CrewCredit], producers: bool) -> Vec<NewCredit> {
    let mut out: Vec<NewCredit> = crew
        .iter()
        .filter_map(|c| {
            let role = match (c.department.as_str(), c.job.as_str()) {
                (_, "Director") => Role::Director,
                ("Writing", _) => Role::Writer,
                (_, "Producer") if producers => Role::Producer,
                _ => return None,
            };
            Some(NewCredit {
                person: PersonId::Tmdb(c.id),
                name: c.name.clone(),
                photo: tmdb_photo(c.profile_path.as_deref()),
                role,
                character: None,
            })
        })
        .collect();
    // Stable: TMDB's order within each role is kept.
    out.sort_by_key(|c| c.role);
    let mut seen_producers = 0;
    out.retain(|c| {
        if c.role != Role::Producer {
            return true;
        }
        seen_producers += 1;
        seen_producers <= MAX_PRODUCERS
    });
    out
}

/// One credit per person and role; an actor playing several parts keeps them
/// all ("Alice / Bob").
fn dedupe(credits: Vec<NewCredit>) -> Vec<NewCredit> {
    let mut out: Vec<NewCredit> = Vec::with_capacity(credits.len());
    let mut index: HashMap<(PersonId, Role), usize> = HashMap::new();
    for credit in credits {
        match index.get(&(credit.person, credit.role)) {
            Some(&i) => {
                if let Some(part) = credit.character {
                    let existing = &mut out[i].character;
                    match existing {
                        Some(parts) if !parts.split(" / ").any(|p| p == part) => {
                            parts.push_str(" / ");
                            parts.push_str(&part);
                        }
                        Some(_) => {}
                        None => *existing = Some(part),
                    }
                }
            }
            None => {
                index.insert((credit.person, credit.role), out.len());
                out.push(credit);
            }
        }
    }
    out
}

pub fn movie_credits(credits: &Credits) -> Vec<NewCredit> {
    let mut cast: Vec<&CastCredit> = credits.cast.iter().collect();
    cast.sort_by_key(|c| c.order);
    let mut out: Vec<NewCredit> = cast.into_iter().map(actor).collect();
    out = dedupe(out);
    out.truncate(MAX_CAST);
    out.extend(crew(&credits.crew, true));
    dedupe(out)
}

pub fn show_credits(credits: &AggregateCredits, creators: &[Creator]) -> Vec<NewCredit> {
    let mut cast: Vec<&AggregateCast> = credits.cast.iter().collect();
    cast.sort_by_key(|c| c.order);
    let mut out: Vec<NewCredit> = cast
        .into_iter()
        .take(MAX_CAST)
        .map(|c| NewCredit {
            person: PersonId::Tmdb(c.id),
            name: c.name.clone(),
            photo: tmdb_photo(c.profile_path.as_deref()),
            role: Role::Actor,
            // The part played in the most episodes.
            character: c.roles.iter().max_by_key(|r| r.episode_count).and_then(|r| clean(r.character.as_deref())),
        })
        .collect();
    out.extend(creators.iter().map(|c| NewCredit {
        person: PersonId::Tmdb(c.id),
        name: c.name.clone(),
        photo: tmdb_photo(c.profile_path.as_deref()),
        role: Role::Creator,
        character: None,
    }));
    dedupe(out)
}

/// Credits of a show from another provider (TVmaze, TheTVDB): the billed
/// cast in order, then the creators, with the same limits and merging as
/// TMDB's.
pub fn provider_show_credits(cast: Vec<NewCredit>, creators: Vec<NewCredit>) -> Vec<NewCredit> {
    let mut out = dedupe(cast);
    out.truncate(MAX_CAST);
    out.extend(creators);
    dedupe(out)
}

/// Guest stars, directors and writers of an episode (the regular cast is the
/// show's).
pub fn episode_credits(guests: &[CastCredit], crew_credits: &[CrewCredit]) -> Vec<NewCredit> {
    let mut cast: Vec<&CastCredit> = guests.iter().collect();
    cast.sort_by_key(|c| c.order);
    let mut out = dedupe(cast.into_iter().map(actor).collect());
    out.truncate(MAX_GUESTS);
    out.extend(crew(crew_credits, false));
    dedupe(out)
}

/// Replace the credits of an item. People are found by the id of the
/// provider the credit comes from.
pub async fn replace(db: &Db, item_id: i64, credits: &[NewCredit]) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    replace_in(&mut tx, item_id, credits).await?;
    tx.commit().await
}

/// [`replace`], in a transaction of the caller's: a season's episodes are
/// written together.
pub async fn replace_in(tx: &mut sqlx::SqliteConnection, item_id: i64, credits: &[NewCredit]) -> sqlx::Result<()> {
    let now = now();
    sqlx::query("DELETE FROM item_people WHERE item_id = ?").bind(item_id).execute(&mut *tx).await?;
    for (position, credit) in credits.iter().enumerate() {
        let (column, id) = credit.person.column();
        // `column` is one of three fixed names.
        let person: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO people ({column}, name, photo_url, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT ({column}) DO UPDATE SET name = excluded.name,
                 photo_url = COALESCE(excluded.photo_url, photo_url), updated_at = excluded.updated_at
             RETURNING id"
        )))
        .bind(id)
        .bind(&credit.name)
        .bind(&credit.photo)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO item_people (item_id, person_id, role, character, position) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(item_id)
        .bind(person)
        .bind(credit.role.as_str())
        .bind(&credit.character)
        .bind(position as i64)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE items SET credits_at = ? WHERE id = ?").bind(now).bind(item_id).execute(&mut *tx).await?;
    Ok(())
}

// ------------------------------------------------------------------ reading

/// A credit of an item, as clients see it.
#[derive(Debug, Clone, Serialize)]
pub struct Credit {
    /// The person's id.
    pub id: i64,
    pub name: String,
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
    /// Where the photo comes from (Plex clients load role thumbs directly).
    #[serde(skip)]
    pub photo_source: Option<String>,
}

#[derive(sqlx::FromRow)]
struct CreditRow {
    item_id: i64,
    person_id: i64,
    name: String,
    role: String,
    character: Option<String>,
    photo_url: Option<String>,
}

/// `/api/people/{id}/photo?v=…`; the version changes with the photo.
pub fn photo_url(person_id: i64, source: Option<&str>) -> Option<String> {
    let source = source.filter(|s| crate::store::allowed_image_url(s))?;
    let version = hex::encode(&Sha256::digest(source.as_bytes())[..5]);
    Some(format!("/api/people/{person_id}/photo?v={version}"))
}

/// Credits of each item, in billing order. The caller checks access.
pub async fn for_items(db: &Db, ids: &[i64]) -> sqlx::Result<HashMap<i64, Vec<Credit>>> {
    let mut out: HashMap<i64, Vec<Credit>> = HashMap::new();
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT ip.item_id, ip.person_id, pe.name, ip.role, ip.character, pe.photo_url
             FROM item_people ip JOIN people pe ON pe.id = ip.person_id
             WHERE ip.item_id IN ({}) ORDER BY ip.item_id, ip.position",
            vec!["?"; chunk.len()].join(",")
        );
        let mut query = sqlx::query_as::<_, CreditRow>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        for row in query.fetch_all(&db.read).await? {
            let Some(role) = Role::parse(&row.role) else { continue };
            out.entry(row.item_id).or_default().push(Credit {
                id: row.person_id,
                photo_url: photo_url(row.person_id, row.photo_url.as_deref()),
                name: row.name,
                role,
                character: row.character,
                photo_source: row.photo_url,
            });
        }
    }
    Ok(out)
}

pub async fn for_item(db: &Db, id: i64) -> sqlx::Result<Vec<Credit>> {
    Ok(for_items(db, &[id]).await?.remove(&id).unwrap_or_default())
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonSummary {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PersonDetail {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
    /// Newest first.
    pub credits: Vec<PersonCredit>,
}

/// A movie or show the person is credited in. Episode credits count towards
/// their show.
#[derive(Debug, Serialize)]
pub struct PersonCredit {
    pub item: ItemSummary,
    pub roles: Vec<PersonRole>,
}

#[derive(Debug, Serialize)]
pub struct PersonRole {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// Credits from single episodes (guest parts, episode directors): how many.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episodes: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct RoleRow {
    target: i64,
    role: String,
    character: Option<String>,
    episodes: i64,
}

/// A person and what they are credited in among the items `user_id` may see;
/// `None` when that is nothing (people only credited in hidden items don't
/// exist for this user).
pub async fn person(db: &Db, user_id: i64, person_id: i64) -> sqlx::Result<Option<PersonDetail>> {
    let found: Option<(String, Option<String>)> = sqlx::query_as("SELECT name, photo_url FROM people WHERE id = ?")
        .bind(person_id)
        .fetch_optional(&db.read)
        .await?;
    let Some((name, photo)) = found else { return Ok(None) };
    let rows: Vec<RoleRow> = sqlx::query_as(
        "SELECT CASE WHEN i.kind = 'episode' THEN i.grandparent_id ELSE i.id END AS target, ip.role, ip.character,
                SUM(i.kind = 'episode') AS episodes
         FROM item_people ip JOIN items i ON i.id = ip.item_id
         WHERE ip.person_id = ? AND i.kind IN ('movie', 'show', 'episode')
         GROUP BY target, ip.role, ip.character
         ORDER BY MIN(ip.position)",
    )
    .bind(person_id)
    .fetch_all(&db.read)
    .await?;
    let mut ids: Vec<i64> = Vec::new();
    let mut roles: HashMap<i64, Vec<PersonRole>> = HashMap::new();
    for row in rows {
        let Some(role) = Role::parse(&row.role) else { continue };
        if !roles.contains_key(&row.target) {
            ids.push(row.target);
        }
        roles.entry(row.target).or_default().push(PersonRole {
            role,
            character: row.character,
            episodes: (row.episodes > 0).then_some(row.episodes),
        });
    }
    // Access rules apply here.
    let items = catalog::items_by_ids(db, user_id, &ids).await?;
    if items.is_empty() {
        return Ok(None);
    }
    let mut credits: Vec<PersonCredit> = items
        .iter()
        .map(|row| {
            let mut roles = roles.remove(&row.id).unwrap_or_default();
            roles.sort_by_key(|r| (r.role, r.episodes.is_some()));
            PersonCredit { item: row.to_summary(), roles }
        })
        .collect();
    credits.sort_by(|a, b| {
        let date = |c: &PersonCredit| (c.item.release_date.clone(), c.item.year);
        date(b).cmp(&date(a)).then_with(|| a.item.sort_title.cmp(&b.item.sort_title))
    });
    Ok(Some(PersonDetail { id: person_id, name, photo_url: photo_url(person_id, photo.as_deref()), credits }))
}

/// Whether the person is credited in an item `user_id` may see.
pub async fn is_visible(db: &Db, user_id: i64, person_id: i64) -> sqlx::Result<bool> {
    let sql = format!(
        "{} SELECT EXISTS (SELECT 1 FROM item_people ip JOIN items i ON i.id = ip.item_id
                           LEFT JOIN items p ON p.id = i.parent_id
                           LEFT JOIN items gp ON gp.id = i.grandparent_id
                           WHERE ip.person_id = ? AND {})",
        catalog::ME_CTE,
        catalog::VISIBLE,
    );
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind(user_id).bind(person_id).fetch_one(&db.read).await
}

/// Forget people no item credits any more (their photos go with the next
/// storage clean-up).
pub async fn prune(db: &Db) -> sqlx::Result<u64> {
    let removed =
        sqlx::query("DELETE FROM people WHERE NOT EXISTS (SELECT 1 FROM item_people WHERE person_id = people.id)")
            .execute(&db.write)
            .await?
            .rows_affected();
    if removed > 0 {
        tracing::debug!("forgot {removed} people without credits");
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cast(id: i64, name: &str, character: &str, order: i64) -> CastCredit {
        CastCredit { id, name: name.into(), character: Some(character.into()), profile_path: None, order }
    }

    fn crew_member(id: i64, name: &str, department: &str, job: &str) -> CrewCredit {
        CrewCredit { id, name: name.into(), job: job.into(), department: department.into(), profile_path: None }
    }

    fn tmdb(credit: &NewCredit) -> i64 {
        match credit.person {
            PersonId::Tmdb(id) => id,
            other => panic!("not a TMDB person: {other:?}"),
        }
    }

    fn credit(person: PersonId, name: &str, role: Role, character: Option<&str>) -> NewCredit {
        NewCredit { person, name: name.into(), photo: None, role, character: character.map(str::to_owned) }
    }

    #[test]
    fn movie_credits_keep_billing_and_merge_duplicates() {
        let credits = Credits {
            cast: vec![
                cast(2, "Carrie-Anne Moss", "Trinity", 2),
                cast(1, "Keanu Reeves", "Neo", 0),
                cast(3, "Hugo Weaving", "Agent Smith", 1),
                cast(3, "Hugo Weaving", "Agent Smith (voice)", 5),
            ],
            crew: vec![
                crew_member(10, "Lilly Wachowski", "Writing", "Screenplay"),
                crew_member(11, "Lana Wachowski", "Directing", "Director"),
                crew_member(10, "Lilly Wachowski", "Directing", "Director"),
                crew_member(10, "Lilly Wachowski", "Writing", "Story"),
                crew_member(12, "Joel Silver", "Production", "Producer"),
                crew_member(13, "Don Davis", "Sound", "Original Music Composer"),
            ],
        };
        let out = movie_credits(&credits);
        let summary: Vec<(i64, Role, Option<&str>)> =
            out.iter().map(|c| (tmdb(c), c.role, c.character.as_deref())).collect();
        assert_eq!(
            summary,
            vec![
                (1, Role::Actor, Some("Neo")),
                (3, Role::Actor, Some("Agent Smith / Agent Smith (voice)")),
                (2, Role::Actor, Some("Trinity")),
                (11, Role::Director, None),
                (10, Role::Director, None),
                (10, Role::Writer, None),
                (12, Role::Producer, None),
            ]
        );
    }

    #[test]
    fn show_credits_use_the_main_part_and_creators() {
        let credits = AggregateCredits {
            cast: vec![AggregateCast {
                id: 1,
                name: "Bryan Cranston".into(),
                profile_path: Some("/abc.jpg".into()),
                roles: vec![
                    AggregateRole { character: Some("Walter White".into()), episode_count: 62 },
                    AggregateRole { character: Some("Heisenberg".into()), episode_count: 3 },
                ],
                order: 0,
            }],
        };
        let creators = [Creator { id: 9, name: "Vince Gilligan".into(), profile_path: None }];
        let out = show_credits(&credits, &creators);
        assert_eq!(out[0].character.as_deref(), Some("Walter White"));
        assert_eq!(out[0].photo.as_deref(), Some("https://image.tmdb.org/t/p/h632/abc.jpg"));
        assert_eq!((tmdb(&out[1]), out[1].role), (9, Role::Creator));
    }

    #[test]
    fn provider_credits_merge_parts_and_list_creators_last() {
        let out = provider_show_credits(
            vec![
                credit(PersonId::Tvmaze(1), "Tatiana Maslany", Role::Actor, Some("Sarah Manning")),
                credit(PersonId::Tvmaze(2), "Jordan Gavaris", Role::Actor, Some("Felix Dawkins")),
                credit(PersonId::Tvmaze(1), "Tatiana Maslany", Role::Actor, Some("Cosima Niehaus")),
            ],
            vec![credit(PersonId::Tvmaze(3), "Graeme Manson", Role::Creator, None)],
        );
        let summary: Vec<(PersonId, Role, Option<&str>)> =
            out.iter().map(|c| (c.person, c.role, c.character.as_deref())).collect();
        assert_eq!(
            summary,
            vec![
                (PersonId::Tvmaze(1), Role::Actor, Some("Sarah Manning / Cosima Niehaus")),
                (PersonId::Tvmaze(2), Role::Actor, Some("Felix Dawkins")),
                (PersonId::Tvmaze(3), Role::Creator, None),
            ]
        );
    }

    #[tokio::test]
    async fn people_are_kept_per_provider_id() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        sqlx::query("INSERT INTO libraries (id, name, kind, created_at) VALUES (1, 'TV', 'shows', 0)")
            .execute(&db.write)
            .await
            .unwrap();
        for id in [1, 2] {
            sqlx::query(
                "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
                 VALUES (?, 1, 'show', ?, 'Show', 'show', 0, 0)",
            )
            .bind(id)
            .bind(format!("show:{id}"))
            .execute(&db.write)
            .await
            .unwrap();
        }
        // The same number at two providers is two people; the same TVmaze id
        // on two shows is one.
        let tmdb = credit(PersonId::Tmdb(7), "Ann Actor", Role::Actor, Some("Ann"));
        let tvmaze = credit(PersonId::Tvmaze(7), "Ann Actor", Role::Actor, Some("Ann"));
        replace(&db, 1, &[tmdb, tvmaze.clone()]).await.unwrap();
        replace(&db, 2, &[NewCredit { name: "Ann B. Actor".into(), ..tvmaze }]).await.unwrap();
        /// TMDB, TVmaze and TheTVDB ids, and the name.
        type Person = (Option<i64>, Option<i64>, Option<i64>, String);
        let people: Vec<Person> = sqlx::query_as("SELECT tmdb_id, tvmaze_id, tvdb_id, name FROM people ORDER BY id")
            .fetch_all(&db.read)
            .await
            .unwrap();
        assert_eq!(
            people,
            vec![(Some(7), None, None, "Ann Actor".to_owned()), (None, Some(7), None, "Ann B. Actor".to_owned())]
        );
        assert_eq!(for_item(&db, 1).await.unwrap().len(), 2);
        assert_eq!(for_item(&db, 2).await.unwrap()[0].id, for_item(&db, 1).await.unwrap()[1].id);
    }

    #[test]
    fn episodes_skip_producers() {
        let out = episode_credits(
            &[cast(5, "Guest", "Clerk", 0)],
            &[crew_member(6, "Producer", "Production", "Producer"), crew_member(7, "Dir", "Directing", "Director")],
        );
        let roles: Vec<Role> = out.iter().map(|c| c.role).collect();
        assert_eq!(roles, vec![Role::Actor, Role::Director]);
    }

    #[test]
    fn only_plain_tmdb_paths_become_files() {
        assert!(tmdb_photo(Some("/kU3B75TyRiCgE270EyZnHjfivoq.jpg")).is_some());
        assert_eq!(tmdb_photo(Some("/../secret.jpg")), None);
        assert_eq!(tmdb_photo(Some("abc.jpg")), None);
        assert_eq!(tmdb_photo(Some("/abc.exe")), None);
        let url = photo_url(4, Some("https://image.tmdb.org/t/p/h632/abc_1.jpg")).unwrap();
        assert!(url.starts_with("/api/people/4/photo?v="));
        assert_eq!(photo_url(4, Some("https://example.com/x.jpg")), None);
    }
}
