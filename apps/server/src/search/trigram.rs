//! Names found by their pieces of three letters: what search falls back on
//! when a query's words find nothing, as with two slips in one word
//! ("sppjder man") or words run together in ways words can't say ("unit40").

use std::collections::HashMap;

use super::text::{fold, similarity};

/// Names sharing pieces with a query that are looked at closely, at most.
const CANDIDATES: usize = 300;

/// Names, and every name's pieces of three letters (spaces count, and a
/// name begins and ends with one). The pieces are worked out when first
/// looked for, as most searches find what they want without them: the
/// entries having each piece in one array, a few bytes a piece.
#[derive(Debug, Default)]
pub struct Trigrams {
    /// The names folded, by entry.
    names: Vec<Box<str>>,
    /// What each entry is the name of (an item, a person); none when it is
    /// the entry's own place (a list of names).
    ids: Vec<i64>,
    pieces: std::sync::OnceLock<Pieces>,
}

#[derive(Debug, Default)]
struct Pieces {
    /// Where each piece's entries are in `entries`: from, and how many.
    at: HashMap<u64, (u32, u32)>,
    entries: Vec<u32>,
}

/// A piece as a number: three characters of 21 bits.
fn key(piece: &[char]) -> u64 {
    (u64::from(piece[0]) << 42) | (u64::from(piece[1]) << 21) | u64::from(piece[2])
}

/// Each piece of `folded` once.
fn pieces(folded: &str, out: &mut Vec<u64>) {
    out.clear();
    let chars: Vec<char> = std::iter::once(' ').chain(folded.chars()).chain(std::iter::once(' ')).collect();
    out.extend(chars.windows(3).filter(|w| w.iter().any(|c| *c != ' ')).map(key));
    out.sort_unstable();
    out.dedup();
}

impl Trigrams {
    /// `names`: what each name is of, and the name. A thing may have more
    /// than one (a title and an original title).
    pub fn new<S: AsRef<str>>(names: impl IntoIterator<Item = (i64, S)>) -> Self {
        let (ids, names): (Vec<i64>, Vec<Box<str>>) =
            names.into_iter().map(|(id, name)| (id, fold(name.as_ref()).into_boxed_str())).unzip();
        Self::index(names, ids)
    }

    /// A list of names, found by their place in it. They are folded already
    /// (see [`fold`]).
    pub fn list(folded: Vec<Box<str>>) -> Self {
        Self::index(folded, Vec::new())
    }

    fn index(mut names: Vec<Box<str>>, mut ids: Vec<i64>) -> Self {
        names.shrink_to_fit();
        ids.shrink_to_fit();
        Trigrams { names, ids, pieces: std::sync::OnceLock::new() }
    }
}

impl Pieces {
    fn of(names: &[Box<str>]) -> Self {
        // Counted first, then placed: no list grows past what it holds.
        let mut buffer = Vec::new();
        let mut counts: HashMap<u64, u32> = HashMap::new();
        for name in names {
            pieces(name, &mut buffer);
            for &piece in &buffer {
                *counts.entry(piece).or_default() += 1;
            }
        }
        let mut pieces_at: HashMap<u64, (u32, u32)> = HashMap::with_capacity(counts.len());
        let mut from = 0u32;
        for (piece, count) in counts {
            pieces_at.insert(piece, (from, 0));
            from += count;
        }
        let mut entries = vec![0u32; from as usize];
        for (entry, name) in names.iter().enumerate() {
            pieces(name, &mut buffer);
            for piece in &buffer {
                let (from, filled) = pieces_at.get_mut(piece).expect("counted");
                entries[(*from + *filled) as usize] = entry as u32;
                *filled += 1;
            }
        }
        Pieces { at: pieces_at, entries }
    }
}

impl Trigrams {
    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The name of entry `entry`, folded.
    pub fn name(&self, entry: usize) -> &str {
        &self.names[entry]
    }

    fn id(&self, entry: u32) -> i64 {
        self.ids.get(entry as usize).copied().unwrap_or(i64::from(entry))
    }

    /// What has a name at least `min` alike to `query` (see [`similarity`]),
    /// the closest first, each once.
    pub fn near(&self, query: &str, min: f32) -> Vec<i64> {
        let query = fold(query);
        if query.chars().count() < 3 {
            return Vec::new();
        }
        let mut wanted = Vec::new();
        pieces(&query, &mut wanted);
        let pieces = self.pieces.get_or_init(|| Pieces::of(&self.names));
        let mut shared: HashMap<u32, u16> = HashMap::new();
        for piece in &wanted {
            if let Some(&(from, count)) = pieces.at.get(piece) {
                for &entry in &pieces.entries[from as usize..(from + count) as usize] {
                    *shared.entry(entry).or_default() += 1;
                }
            }
        }
        let mut most: Vec<(u32, u16)> = shared.into_iter().collect();
        most.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        most.truncate(CANDIDATES);
        let mut close: Vec<(f32, u32)> = most
            .into_iter()
            .map(|(entry, _)| (similarity(&self.names[entry as usize], &query), entry))
            .filter(|(s, _)| *s >= min)
            .collect();
        close.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut out: Vec<i64> = Vec::new();
        for (_, entry) in close {
            let id = self.id(entry);
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_with_slips_too_many_for_words_are_found_by_their_pieces() {
        let index = Trigrams::new([
            (1, "Harbor Lights: Far From Home"),
            (2, "Harbor Lights 2"),
            (3, "Unit 4.0"),
            (4, "Home Again"),
            (5, "Far and Wide"),
        ]);
        assert_eq!(index.len(), 5);
        assert_eq!(index.near("harbbro lights far from home", 0.8), [1]);
        assert_eq!(index.near("unit 40", 0.8), [3]);
        assert_eq!(index.near("harbor lights", 0.8), [2], "only close enough");
        assert!(index.near("the godfather", 0.8).is_empty());
        assert!(index.near("sp", 0.0).is_empty(), "too short to have pieces");
    }

    #[test]
    fn a_list_is_found_by_place() {
        let index = Trigrams::list(vec!["news one hd".into(), "".into(), "arena sports f1".into()]);
        assert_eq!((index.len(), index.name(2)), (3, "arena sports f1"));
        assert_eq!(index.near("arena sprts f1", 0.8), [2]);
    }

    #[test]
    fn a_thing_with_two_names_is_found_once() {
        let index = Trigrams::new([(7, "Élodie"), (7, "Le Monde d'Élodie Martin")]);
        assert_eq!(index.near("elodie", 0.5), [7]);
    }
}
