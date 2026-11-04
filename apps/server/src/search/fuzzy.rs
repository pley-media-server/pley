//! Words the library doesn't have, read as the ones it does: a slip ("matirx"
//! for "matrix"), a word joined or split ("spiderman", "spider man"), an
//! apostrophe left out ("kings" for "king's").
//!
//! Each word of a query stands for a few words of the library, and the
//! other words of the query choose between them: "sider man" finds "Spider-Man"
//! though "sider" is as close to "side" and "sided". The thresholds are search
//! engines' (Meilisearch and Typesense allow one slip from 4 or 5 letters, two
//! from 7 to 9); a swap of two letters counts as one, as Elasticsearch counts it.

use std::collections::HashMap;

/// Words a word may stand for, at most: more find too much.
const MAX_ALTERNATIVES: usize = 8;

/// The words of a set of names, with how many names have each: in order,
/// one allocation per word and none beside it, as a library's words run to
/// tens of thousands.
#[derive(Debug, Default)]
pub struct Words {
    sorted: Vec<(Box<str>, u32)>,
}

impl Words {
    pub fn new(counts: impl IntoIterator<Item = (String, u32)>) -> Self {
        let mut sorted: Vec<(Box<str>, u32)> =
            counts.into_iter().filter(|(w, _)| !w.is_empty()).map(|(w, n)| (w.into_boxed_str(), n)).collect();
        sorted.sort_unstable();
        sorted.dedup_by(|a, b| a.0 == b.0);
        sorted.shrink_to_fit();
        Words { sorted }
    }

    /// The words of `names` (folded, see [`super::text::fold`]).
    pub fn of<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut count: HashMap<&str, u32> = HashMap::new();
        let mut seen: Vec<&str> = Vec::new();
        for name in names {
            seen.clear();
            seen.extend(name.split(' ').filter(|w| !w.is_empty()));
            seen.sort_unstable();
            seen.dedup();
            for w in &seen {
                *count.entry(w).or_default() += 1;
            }
        }
        Words::new(count.into_iter().map(|(w, n)| (w.to_owned(), n)))
    }

    pub fn len(&self) -> usize {
        self.sorted.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }

    pub fn contains(&self, word: &str) -> bool {
        self.sorted.binary_search_by(|(w, _)| (**w).cmp(word)).is_ok()
    }

    /// Whether some word begins with `prefix`.
    pub fn begins(&self, prefix: &str) -> bool {
        let at = self.sorted.partition_point(|(w, _)| &**w < prefix);
        self.sorted.get(at).is_some_and(|(w, _)| w.starts_with(prefix))
    }

    /// The words `word` may be a slip for, closest and most common first;
    /// for the last word of a query (perhaps typed halfway), the beginnings
    /// of words too. They begin with its first letter: people seldom get
    /// that wrong, and "eanu" is no more "keanu" than "lien" is "alien".
    pub fn near(&self, word: &str, last: bool) -> Vec<String> {
        let max = slips_allowed(word);
        let Some(first) = word.chars().next().filter(|_| max > 0) else { return Vec::new() };
        let typed: Vec<char> = word.chars().collect();
        let mut whole: Vec<(usize, u32, &str)> = Vec::new();
        let mut begun: Vec<(usize, String)> = Vec::new();
        let mut chars: Vec<char> = Vec::new();
        // Words are sorted: those beginning with `first` are together.
        let from = self.sorted.partition_point(|(w, _)| w.chars().next() < Some(first));
        for (w, count) in self.sorted[from..].iter().take_while(|(w, _)| w.starts_with(first)) {
            chars.clear();
            chars.extend(w.chars());
            if let Some(d) = distance(&typed, &chars, max) {
                whole.push((d, *count, w));
            }
            if last && chars.len() > typed.len() {
                let head = &chars[..typed.len()];
                if let Some(d) = distance(&typed, head, max) {
                    begun.push((d, head.iter().collect()));
                }
            }
        }
        whole.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(b.2)));
        begun.sort();
        let mut out: Vec<String> = Vec::new();
        for w in whole.into_iter().map(|(_, _, w)| w.to_owned()).chain(begun.into_iter().map(|(_, w)| w)) {
            if out.len() == MAX_ALTERNATIVES {
                break;
            }
            if !out.contains(&w) {
                out.push(w);
            }
        }
        out
    }
}

/// Slips a word may have and still be read as another: none under 4
/// letters, one under 8, two from 8.
pub fn slips_allowed(word: &str) -> usize {
    match word.chars().count() {
        0..4 => 0,
        4..8 => 1,
        _ => 2,
    }
}

/// The edits between `a` and `b` (a letter added, dropped or changed, or two
/// swapped), when there are at most `max`.
pub fn distance(a: &[char], b: &[char], max: usize) -> Option<usize> {
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let width = b.len() + 1;
    let mut rows = vec![vec![0usize; width]; 3];
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        let (cur, prev, prev2) = (i % 3, (i + 2) % 3, (i + 1) % 3);
        rows[cur][0] = i;
        let mut best = i;
        for j in 1..width {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut d = (rows[prev][j] + 1).min(rows[cur][j - 1] + 1).min(rows[prev][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d = d.min(rows[prev2][j - 2] + 1);
            }
            rows[cur][j] = d;
            best = best.min(d);
        }
        if best > max {
            return None;
        }
    }
    let d = rows[a.len() % 3][b.len()];
    (d <= max).then_some(d)
}

/// One way to read a word of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// A word, or the beginning of one.
    Begins(String),
    /// Words one after the other: "king s" for "kings", as "King's" is indexed.
    Phrase(Vec<String>),
}

impl Term {
    /// As FTS5 reads it.
    fn fts(&self) -> String {
        match self {
            Term::Begins(w) => format!("\"{}\"*", w.replace('"', "\"\"")),
            Term::Phrase(ws) => format!("\"{}\"", ws.join(" ").replace('"', "\"\"")),
        }
    }
}

/// The ways to read a query: every group must be found, by any of its terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub groups: Vec<Vec<Term>>,
    /// Whether it reads a word as another (not only as typed).
    pub corrected: bool,
}

impl Reading {
    /// The query as typed: every word, or a word it begins.
    pub fn as_typed(words: &[String]) -> Self {
        Reading { groups: words.iter().map(|w| vec![Term::Begins(w.clone())]).collect(), corrected: false }
    }

    /// As FTS5 reads it: `("matirx"* OR "matrix"*) AND "the"*`.
    pub fn fts(&self) -> Option<String> {
        let groups: Vec<String> = self
            .groups
            .iter()
            .filter(|g| !g.is_empty())
            .map(|g| format!("({})", g.iter().map(Term::fts).collect::<Vec<_>>().join(" OR ")))
            .collect();
        (!groups.is_empty()).then(|| groups.join(" AND "))
    }
}

/// How `words` may be read with the words of `known`.
///
/// A word the names have stands for itself (and the last word for those it
/// begins). One they don't is read as the closest words they do (see
/// [`Words::near`]), or, when there are none, as words it joins ("spiderman"
/// as "spider man", "spidrman" too). Two words that are one there are read
/// joined ("sea view" as "seaview"). Numbers and words under 4 letters are only
/// read as typed. A word ending in "s" may be a word with an apostrophe
/// ("kings", "King's").
pub fn read(words: &[String], known: &Words) -> Reading {
    let mut groups: Vec<Vec<Term>> = Vec::new();
    let mut corrected = false;
    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        let last = i == words.len() - 1;
        let has = known.contains(word) || (last && known.begins(word));
        let mut group = vec![Term::Begins(word.clone())];
        if let Some(next) = words.get(i + 1) {
            let joined = format!("{word}{next}");
            if known.contains(&joined) && !(has && known.contains(next)) {
                groups.push(vec![Term::Begins(joined)]);
                corrected = true;
                i += 2;
                continue;
            }
        }
        if let Some(stem) = word.strip_suffix('s')
            && stem.chars().count() >= 3
            && known.contains(stem)
            && known.contains("s")
        {
            group.push(Term::Phrase(vec![stem.to_owned(), "s".to_owned()]));
        }
        let plain = word.chars().all(char::is_numeric) || slips_allowed(word) == 0;
        if !has && !plain {
            let near = known.near(word, last);
            if !near.is_empty() {
                group.extend(near.into_iter().map(Term::Begins));
                corrected = true;
            } else if let Some(parts) = split(word, known, last) {
                groups.extend(parts);
                corrected = true;
                i += 1;
                continue;
            }
        }
        groups.push(group);
        i += 1;
    }
    Reading { groups, corrected }
}

/// `word` as two or three words of `known` written together ("spiderman",
/// "rentaboat"), or two of which one has a slip ("spidrman"). Parts have 3
/// letters or more, but for a middle word of one ("rent a boat").
fn split(word: &str, known: &Words, last: bool) -> Option<Vec<Vec<Term>>> {
    let chars: Vec<char> = word.chars().collect();
    let n = chars.len();
    let part = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    let one = |w: String| vec![Term::Begins(w)];
    for k in 3..n.saturating_sub(2) {
        let (a, b) = (part(0, k), part(k, n));
        if known.contains(&a) && known.contains(&b) {
            return Some(vec![one(a), one(b)]);
        }
    }
    if n >= 7 {
        for k in 3..n - 3 {
            for m in k + 1..n - 2 {
                let (a, mid, b) = (part(0, k), part(k, m), part(m, n));
                let fits = mid.chars().count() == 1 || mid.chars().count() >= 3;
                if fits && known.contains(&a) && known.contains(&mid) && known.contains(&b) {
                    return Some(vec![one(a), one(mid), one(b)]);
                }
            }
        }
        for k in 3..n - 2 {
            let (a, b) = (part(0, k), part(k, n));
            if known.contains(&a) {
                let near = known.near(&b, last);
                if !near.is_empty() {
                    return Some(vec![one(a), near.into_iter().map(Term::Begins).collect()]);
                }
            }
            if known.contains(&b) {
                let near = known.near(&a, false);
                if !near.is_empty() {
                    return Some(vec![near.into_iter().map(Term::Begins).collect(), one(b)]);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(names: &[&str]) -> Words {
        let folded: Vec<String> = names.iter().map(|n| super::super::text::fold(n)).collect();
        Words::of(folded.iter().map(String::as_str))
    }

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn words(s: &str) -> Vec<String> {
        super::super::text::words(s)
    }

    #[test]
    fn distance_counts_a_swap_as_one_edit() {
        assert_eq!(distance(&chars("matirx"), &chars("matrix"), 2), Some(1));
        assert_eq!(distance(&chars("hosue"), &chars("house"), 2), Some(1));
        assert_eq!(distance(&chars("chaptre"), &chars("chapter"), 2), Some(1));
        assert_eq!(distance(&chars("kitten"), &chars("sitting"), 3), Some(3));
        assert_eq!(distance(&chars("kitten"), &chars("sitting"), 2), None);
        assert_eq!(distance(&chars("abc"), &chars("abc"), 0), Some(0));
        assert_eq!(distance(&chars(""), &chars("ab"), 2), Some(2));
    }

    #[test]
    fn short_words_and_numbers_have_no_slips() {
        assert_eq!(slips_allowed("her"), 0);
        assert_eq!(slips_allowed("heat"), 1);
        assert_eq!(slips_allowed("treasure"), 2);
        let k = known(&["Her", "1917", "1918"]);
        assert_eq!(read(&words("hir"), &k), Reading::as_typed(&words("hir")));
        assert_eq!(read(&words("1916"), &k), Reading::as_typed(&words("1916")));
    }

    #[test]
    fn a_slip_keeps_the_first_letter() {
        let k = known(&["Keanu Reeves", "Alien"]);
        assert_eq!(read(&words("eanu"), &k), Reading::as_typed(&words("eanu")));
        assert_eq!(read(&words("lien"), &k), Reading::as_typed(&words("lien")));
        assert!(read(&words("kaenu"), &k).corrected);
    }

    #[test]
    fn a_slip_reads_as_the_close_words_and_the_other_words_choose() {
        let k = known(&["Spider-Man: Far From Home", "Easy Rider", "The Side Show"]);
        let r = read(&words("sider man"), &k);
        assert!(r.corrected);
        assert_eq!(r.groups[0][0], Term::Begins("sider".into()), "as typed too");
        for w in ["spider", "side"] {
            assert!(r.groups[0].contains(&Term::Begins(w.into())), "{w}: {r:?}");
        }
        assert!(!r.groups[0].contains(&Term::Begins("rider".into())), "not the first letter: {r:?}");
        assert_eq!(r.fts().unwrap().matches(" AND ").count(), 1);
    }

    #[test]
    fn known_words_stand_for_themselves() {
        let k = known(&["The Matrix", "Interstellar"]);
        assert_eq!(read(&words("the matrix"), &k), Reading::as_typed(&words("the matrix")));
        // Typed halfway: the beginning of a word the library has.
        assert_eq!(read(&words("inters"), &k), Reading::as_typed(&words("inters")));
        // Halfway with a slip.
        let r = read(&words("intesr"), &k);
        assert!(r.groups[0].contains(&Term::Begins("inters".into())), "{r:?}");
    }

    #[test]
    fn joined_and_split_words() {
        let k = known(&["Spider-Man", "Rent-a-Boat", "Seaview"]);
        assert_eq!(read(&words("spiderman"), &k).fts().unwrap(), "(\"spider\"*) AND (\"man\"*)");
        assert_eq!(read(&words("spidrman"), &k).groups[0], vec![Term::Begins("spider".into())]);
        assert_eq!(read(&words("rentaboat"), &k).groups.len(), 3);
        assert_eq!(read(&words("sea view"), &k).groups, vec![vec![Term::Begins("seaview".into())]]);
        // Both words known: read as typed.
        let k = known(&["Seaview", "Sea View"]);
        assert_eq!(read(&words("sea view"), &k), Reading::as_typed(&words("sea view")));
    }

    #[test]
    fn a_word_ending_in_s_may_have_had_an_apostrophe() {
        // As FTS5 indexes "The Captain's Log".
        let k = Words::new([("captain".to_owned(), 1), ("s".to_owned(), 1), ("log".to_owned(), 1)]);
        let r = read(&words("captains log"), &k);
        assert!(r.groups[0].contains(&Term::Phrase(vec!["captain".into(), "s".into()])), "{r:?}");
        // One slip from "captain" too.
        assert_eq!(r.fts().unwrap(), "(\"captains\"* OR \"captain s\" OR \"captain\"*) AND (\"log\"*)");
    }

    #[test]
    fn quotes_in_a_word_stay_inside_its_term() {
        let r = Reading::as_typed(&["a\"b".to_owned()]);
        assert_eq!(r.fts().unwrap(), "(\"a\"\"b\"*)");
        assert_eq!(Reading::as_typed(&[]).fts(), None);
    }
}
