//! Filters and sorts of collection listings
//! (`/library/sections/{id}/all?type=18`, `/library/sections/{id}/collections`):
//! `title` and `label`, with or without the `collection.` prefix
//! python-plexapi gives them, and sorts by title, added and updated dates.
//! A filter Pley can't apply answers `400`: a tool that finds collections
//! by label (Kometa, to know which ones it made) must never be handed every
//! collection instead.

use std::collections::HashMap;

use super::{Params, PlexError, PlexResult};
use crate::collections::CollectionDto;

/// Values one filter may name.
const MAX_VALUES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// `field=v`: contains (titles), has (labels).
    Is,
    /// `field!=v`
    Not,
    /// `field==v`
    Exactly,
    /// `field!==v`
    NotExactly,
    /// `field<=v`
    Begins,
    /// `field>=v`
    Ends,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    Label,
}

#[derive(Debug)]
struct Filter {
    field: Field,
    op: Op,
    /// Lowercase; any of them matches.
    values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Title,
    Added,
    Updated,
}

/// A collection listing's filters, sorts and limit.
#[derive(Debug, Default)]
pub struct Search {
    filters: Vec<Filter>,
    sorts: Vec<(SortKey, bool)>,
    limit: Option<usize>,
}

/// Parameters of listings that don't filter them.
fn not_a_filter(key: &str) -> bool {
    key.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("x-plex-"))
        || key.starts_with("include")
        || key.starts_with("exclude")
        || matches!(key, "type" | "checkFiles" | "asyncAugmentMetadata" | "sectionId")
}

impl Search {
    pub fn parse(params: &Params) -> PlexResult<Search> {
        let mut search = Search::default();
        for (raw_key, raw_value) in &params.0 {
            if not_a_filter(raw_key) {
                continue;
            }
            match raw_key.as_str() {
                "sort" => {
                    for sort in raw_value.split(',').filter(|s| !s.trim().is_empty()) {
                        search.sorts.push(parse_sort(sort.trim())?);
                    }
                    continue;
                }
                "limit" => {
                    let limit = raw_value.trim().parse().map_err(|_| bad("limit is a number"))?;
                    search.limit = Some(limit);
                    continue;
                }
                _ => {}
            }
            // The operator's `=` may come in the key, encoded, as Kometa
            // writes it: `title%3D=x` is `title==x`.
            let (key, equals_in_key) = match raw_key.strip_suffix('=') {
                Some(key) => (key, true),
                None => (raw_key.as_str(), false),
            };
            let key = key.strip_prefix("collection.").unwrap_or(key);
            let (name, op) = if let Some(f) = key.strip_suffix('!') {
                (f, Op::Not)
            } else if let Some(f) = key.strip_suffix('<') {
                (f, Op::Begins)
            } else if let Some(f) = key.strip_suffix('>') {
                (f, Op::Ends)
            } else {
                (key, Op::Is)
            };
            let (op, value) = match (op, equals_in_key, raw_value.strip_prefix('=')) {
                (Op::Is, true, _) => (Op::Exactly, raw_value.as_str()),
                (Op::Not, true, _) => (Op::NotExactly, raw_value.as_str()),
                (Op::Is, false, Some(rest)) => (Op::Exactly, rest),
                (Op::Not, false, Some(rest)) => (Op::NotExactly, rest),
                (op, _, _) => (op, raw_value.as_str()),
            };
            let field = match (name, op) {
                ("title", _) => Field::Title,
                // A label is matched whole either way.
                ("label", Op::Is | Op::Exactly | Op::Not | Op::NotExactly) => Field::Label,
                ("label", _) => return Err(bad("labels of collections are filtered with = and !=")),
                _ => return Err(bad(&format!("collections can't be filtered by `{raw_key}`"))),
            };
            let values: Vec<String> =
                value.split(',').map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty()).collect();
            if values.is_empty() || values.len() > MAX_VALUES {
                return Err(bad(&format!("`{raw_key}` names 1 to {MAX_VALUES} values")));
            }
            search.filters.push(Filter { field, op, values });
        }
        Ok(search)
    }

    /// The collections that match, in the order asked for, limited.
    pub fn apply(&self, collections: Vec<CollectionDto>, labels: &HashMap<i64, Vec<String>>) -> Vec<CollectionDto> {
        let mut found: Vec<CollectionDto> = collections
            .into_iter()
            .filter(|c| {
                let own = labels.get(&c.id).map(Vec::as_slice).unwrap_or_default();
                self.filters.iter().all(|f| f.matches(c, own))
            })
            .collect();
        if !self.sorts.is_empty() {
            found.sort_by(|a, b| {
                self.sorts
                    .iter()
                    .map(|(key, desc)| {
                        let order = match key {
                            SortKey::Title => a.sort_title.to_lowercase().cmp(&b.sort_title.to_lowercase()),
                            SortKey::Added => a.created_at.cmp(&b.created_at),
                            SortKey::Updated => a.updated_at.cmp(&b.updated_at),
                        };
                        if *desc { order.reverse() } else { order }
                    })
                    .find(|o| o.is_ne())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        if let Some(limit) = self.limit {
            found.truncate(limit);
        }
        found
    }
}

impl Filter {
    fn matches(&self, collection: &CollectionDto, labels: &[String]) -> bool {
        match self.field {
            Field::Title => {
                let title = collection.title.to_lowercase();
                let hit = |v: &String| match self.op {
                    Op::Is | Op::Not => title.contains(v.as_str()),
                    Op::Exactly | Op::NotExactly => title == *v,
                    Op::Begins => title.starts_with(v.as_str()),
                    Op::Ends => title.ends_with(v.as_str()),
                };
                let any = self.values.iter().any(hit);
                if matches!(self.op, Op::Not | Op::NotExactly) { !any } else { any }
            }
            Field::Label => {
                let any = labels.iter().any(|l| self.values.contains(&l.to_lowercase()));
                if matches!(self.op, Op::Not | Op::NotExactly) { !any } else { any }
            }
        }
    }
}

fn parse_sort(sort: &str) -> PlexResult<(SortKey, bool)> {
    let (field, dir) = sort.split_once(':').unwrap_or((sort, ""));
    let key = match field.strip_prefix("collection.").unwrap_or(field) {
        "titleSort" | "title" => SortKey::Title,
        "addedAt" => SortKey::Added,
        "updatedAt" => SortKey::Updated,
        _ => return Err(bad(&format!("collections can't be sorted by `{field}`"))),
    };
    match dir {
        "" | "asc" | "nullsLast" => Ok((key, false)),
        "desc" => Ok((key, true)),
        _ => Err(bad(&format!("`{dir}` isn't a direction"))),
    }
}

fn bad(message: &str) -> PlexError {
    PlexError::BadRequest(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(query: &[(&str, &str)]) -> Params {
        Params(query.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect())
    }

    fn collection(id: i64, title: &str) -> CollectionDto {
        CollectionDto {
            id,
            library_id: 1,
            title: title.to_owned(),
            sort_title: crate::util::sort_title(title),
            summary: None,
            automatic: false,
            sort: "release".into(),
            smart_filter: None,
            item_count: 1,
            poster_url: String::new(),
            backdrop_url: String::new(),
            created_at: id,
            updated_at: 10 - id,
        }
    }

    fn found(query: &[(&str, &str)]) -> PlexResult<Vec<i64>> {
        let all = vec![collection(1, "Space Films"), collection(2, "The Heist Films"), collection(3, "Westerns")];
        let labels = HashMap::from([(1, vec!["Kometa".to_owned()]), (2, vec!["kometa".to_owned(), "Mine".to_owned()])]);
        Ok(Search::parse(&params(query))?.apply(all, &labels).iter().map(|c| c.id).collect())
    }

    #[test]
    fn filters_by_label_and_title() {
        assert_eq!(found(&[("type", "18"), ("X-Plex-Token", "t")]).unwrap(), [1, 2, 3]);
        assert_eq!(found(&[("label", "KOMETA")]).unwrap(), [1, 2], "whatever the case");
        assert_eq!(found(&[("collection.label", "Mine,Other")]).unwrap(), [2]);
        assert_eq!(found(&[("label!", "kometa")]).unwrap(), [3]);
        assert_eq!(found(&[("label", "Nobody's")]).unwrap(), Vec::<i64>::new());
        assert_eq!(found(&[("title", "films")]).unwrap(), [1, 2]);
        assert_eq!(found(&[("title", "=westerns")]).unwrap(), [3]);
        assert_eq!(found(&[("title=", "Westerns")]).unwrap(), [3], "the `=` in the key, as Kometa sends it");
        assert_eq!(found(&[("title!=", "westerns")]).unwrap(), [1, 2]);
        assert_eq!(found(&[("label!=", "kometa")]).unwrap(), [3]);
        assert_eq!(found(&[("collection.title<", "the")]).unwrap(), [2]);
        assert_eq!(found(&[("title!", "heist")]).unwrap(), [1, 3]);
        assert_eq!(found(&[("sort", "titleSort:desc")]).unwrap(), [3, 1, 2], "sort titles drop The");
        assert_eq!(found(&[("sort", "collection.updatedAt"), ("limit", "2")]).unwrap(), [3, 2]);
    }

    #[test]
    fn what_it_cant_apply_is_refused() {
        for query in [
            [("genre", "Horror")],
            [("label>", "x")],
            [("show.title", "x")],
            [("push", "1")],
            [("sort", "random")],
            [("label", "")],
        ] {
            assert!(found(&query).is_err(), "{query:?}");
        }
    }
}
