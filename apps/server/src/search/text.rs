//! Text as search compares it: folded, split into words, and how close two
//! strings are.

use unicode_normalization::UnicodeNormalization;

/// Most words of a query that count; the rest are left out.
pub const MAX_WORDS: usize = 8;

/// `s` as search compares it: lower case, without accents, apostrophes
/// left out ("king's" is "kings"), other punctuation a space, compatibility
/// forms plain ("ᴴᴰ" is "hd"), spaces single.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for c in s.nfkd() {
        if unicode_normalization::char::is_combining_mark(c) || matches!(c, '\'' | '\u{2019}' | '`') {
            continue;
        }
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// The words of `s`, folded.
pub fn words(s: &str) -> Vec<String> {
    fold(s).split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect()
}

/// A search as typed: its words, and a year it ends with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub words: Vec<String>,
    /// "alien 1979": the year narrows the titles found (a year either side
    /// too, as release dates differ), unless no title of those years has
    /// the other words, when it counts as a word.
    pub year: Option<i64>,
}

impl Query {
    pub fn parse(input: &str) -> Self {
        let mut words = words(input);
        words.truncate(MAX_WORDS);
        let year = match words.as_slice() {
            [.., _, last] if is_year(last) => last.parse().ok(),
            _ => None,
        };
        if year.is_some() {
            words.pop();
        }
        Query { words, year }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// The words, a year as a word too.
    pub fn all_words(&self) -> Vec<String> {
        let mut words = self.words.clone();
        words.extend(self.year.map(|y| y.to_string()));
        words
    }

    /// The query folded, as names are compared with it.
    pub fn text(&self) -> String {
        self.words.join(" ")
    }
}

fn is_year(word: &str) -> bool {
    word.len() == 4 && (word.starts_with("19") || word.starts_with("20")) && word.bytes().all(|b| b.is_ascii_digit())
}

/// A search as names are found by it as typed: when they have it in them,
/// or every word of it begins one of theirs, in any order.
pub struct Finder {
    folded: String,
    words: Vec<String>,
}

impl Finder {
    pub fn new(search: &str) -> Self {
        let folded = fold(search);
        let words = folded.split(' ').filter(|w| !w.is_empty()).map(str::to_owned).collect();
        Finder { folded, words }
    }

    /// Whether `name` is found ("lakeside games" finds "Winter Games - Lakeside
    /// 2020"); nothing is found by an empty search.
    pub fn finds(&self, name: &str) -> bool {
        if self.words.is_empty() {
            return false;
        }
        let name = fold(name);
        name.contains(&self.folded) || self.words.iter().all(|w| name.split(' ').any(|n| n.starts_with(w.as_str())))
    }
}

/// How closely `name` answers a search for `input`, best first: 0 the whole
/// name, 1 its beginning, 2 a word of it onwards, 3 its words in any order.
/// Case, accents and punctuation don't count ("spider-man" is "Spider Man",
/// "captains log" is "Captain's Log").
pub fn match_rank(input: &str, name: &str) -> u8 {
    let (query, name) = (fold(input), fold(name));
    if query.is_empty() {
        3
    } else if name == query {
        0
    } else if name.starts_with(&query) {
        1
    } else if name.contains(&format!(" {query}")) {
        2
    } else {
        3
    }
}

/// How alike `a` and `b` are, from 0 (nothing in common) to 1 (the same):
/// twice the letters they share in order over the letters of both, so a
/// slip or two in a long name keeps it close and a short name with one
/// word in common stays far.
pub fn similarity(a: &str, b: &str) -> f32 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    // The longest common subsequence, one row at a time.
    let mut row = vec![0u16; b.len() + 1];
    for &ca in &a {
        let mut diagonal = 0;
        for (j, &cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == cb { diagonal + 1 } else { above.max(row[j]) };
            diagonal = above;
        }
    }
    2.0 * f32::from(row[b.len()]) / (a.len() + b.len()) as f32
}

/// [`similarity`] in steps of 5%, higher first when sorted: names this
/// close are equals, and other things decide between them.
pub fn closeness(a: &str, b: &str) -> std::cmp::Reverse<u8> {
    std::cmp::Reverse((similarity(a, b) * 20.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_leaves_case_accents_apostrophes_and_punctuation_out() {
        assert_eq!(fold("The Captain's Log"), "the captains log");
        assert_eq!(fold("  Élodie — l’été  "), "elodie lete");
        assert_eq!(fold("Spider-Man: Far From Home"), "spider man far from home");
        assert_eq!(fold("UK| ARENA SPORTS ᴴᴰ"), "uk arena sports hd");
        assert_eq!(fold("НОВОЕ КИНО"), "новое кино");
        assert_eq!(words("Unit 4.0"), ["unit", "4", "0"]);
    }

    #[test]
    fn a_year_at_the_end_is_kept_apart_unless_it_is_all_there_is() {
        let q = Query::parse("Alien 1979");
        assert_eq!((q.words.as_slice(), q.year), (&["alien".to_owned()][..], Some(1979)));
        assert_eq!(q.all_words(), ["alien", "1979"]);
        assert_eq!(Query::parse("1917").year, None, "a title that is a year");
        assert_eq!(Query::parse("blade runner 2049 x").year, None, "only at the end");
        assert_eq!(Query::parse("room 1408").year, None, "not a year");
        assert!(Query::parse(" - ").is_empty());
        assert_eq!(Query::parse("a b c d e f g h i j").words.len(), MAX_WORDS);
    }

    #[test]
    fn names_are_found_by_the_search_in_them_or_its_words() {
        let games = Finder::new("winter games lakeside 2020");
        assert!(games.finds("Winter Games - Lakeside 2020"));
        assert!(Finder::new("lakeside winter").finds("Winter Games - Lakeside 2020"), "any order");
        assert!(Finder::new("nuit montreal").finds("Une nuit à Montréal"));
        assert!(Finder::new("ames - lake").finds("Winter Games - Lakeside 2020"), "in it as typed");
        assert!(!Finder::new("lakeside 2021").finds("Winter Games - Lakeside 2020"));
        assert!(!Finder::new(" - ").finds("Winter Games - Lakeside 2020"));
    }

    #[test]
    fn whole_names_rank_before_beginnings_and_words() {
        assert_eq!(match_rank("the matrix", "The Matrix"), 0);
        assert_eq!(match_rank("captains log", "The Captain's Log"), 2);
        assert_eq!(match_rank("the captains log", "The Captain's Log"), 0);
        assert_eq!(match_rank("matr", "The Matrix"), 2);
        assert_eq!(match_rank("the mat", "The Matrix Reloaded"), 1);
        assert_eq!(match_rank("reloaded matrix", "The Matrix Reloaded"), 3);
        assert_eq!(match_rank("", "x"), 3);
    }

    #[test]
    fn similarity_counts_letters_in_order() {
        assert_eq!(similarity("matrix", "matrix"), 1.0);
        assert!(similarity("the matirx", "the matrix") > 0.85);
        assert!(similarity("chaptre", "chapter") > 0.8);
        assert!(similarity("heat", "alien") < 0.5);
        assert_eq!(similarity("", ""), 1.0);
        assert_eq!(similarity("abc", ""), 0.0);
        assert!(closeness("the matrix", "the matrix") < closeness("the matirx", "the matrix"));
    }
}
