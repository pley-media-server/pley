//! Search over names kept in memory: Live TV's channels.
//!
//! A name is found when every word of the search begins one of its words,
//! in any order ("arena f1" finds "UK| ARENA SPORTS F1 HD"), or when it has the
//! search in it as typed. A word no name has is read as the closest words
//! names have (see [`super::fuzzy`]); when nothing is found that way either,
//! names alike enough to the search are (see [`super::trigram`]).

use std::collections::HashMap;

use super::fuzzy::{self, Term, Words};
use super::text::{closeness, fold, words};
use super::trigram::Trigrams;

/// How alike a name must be to the search to be found when its words find
/// nothing (see [`super::text::similarity`]).
const NEAR: f32 = 0.8;

/// Words for a picture's quality that providers add to names in their own
/// ways ("HD", "FHD", "ᴴᴰ"): a search with one finds the names without it
/// when none has it.
const QUALITY: &[&str] =
    &["hd", "fhd", "uhd", "sd", "4k", "8k", "hevc", "h265", "raw", "vip", "1080p", "720p", "50fps", "60fps"];

/// Names to search, by their place in a list. Each name is kept once,
/// folded (in its [`Trigrams`]), and split into words when searched.
#[derive(Debug, Default)]
pub struct Names {
    known: Words,
    trigrams: Trigrams,
}

/// How a name was found, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Found {
    /// By the search as typed.
    AsTyped,
    /// By words read as others, the closest names first.
    Corrected(std::cmp::Reverse<u8>),
    /// Alike to the search, the closest first.
    Alike(std::cmp::Reverse<u8>),
}

impl Names {
    pub fn new<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let folded: Vec<Box<str>> = names.into_iter().map(|n| fold(n).into_boxed_str()).collect();
        let known = Words::of(folded.iter().map(|n| &**n));
        Names { known, trigrams: Trigrams::list(folded) }
    }

    /// The names `search` finds, by their place, with how.
    pub fn find(&self, search: &str) -> HashMap<usize, Found> {
        let typed = words(search);
        if typed.is_empty() {
            return HashMap::new();
        }
        let mut found = self.find_words(&typed);
        if found.is_empty() {
            let plain: Vec<String> = typed.iter().filter(|w| !QUALITY.contains(&w.as_str())).cloned().collect();
            if !plain.is_empty() && plain.len() < typed.len() {
                found = self.find_words(&plain);
            }
        }
        if found.is_empty() {
            let query = typed.join(" ");
            for id in self.trigrams.near(&query, NEAR) {
                let i = id as usize;
                found.insert(i, Found::Alike(self.closeness(i, &query)));
            }
        }
        found
    }

    fn find_words(&self, typed: &[String]) -> HashMap<usize, Found> {
        let query = typed.join(" ");
        let reading = fuzzy::read(typed, &self.known);
        let as_typed = fuzzy::Reading::as_typed(typed);
        let mut found = HashMap::new();
        let mut words: Vec<&str> = Vec::new();
        for i in 0..self.trigrams.len() {
            let name = self.trigrams.name(i);
            words.clear();
            words.extend(name.split(' '));
            if name.contains(&query) || matches(&words, &as_typed.groups) {
                found.insert(i, Found::AsTyped);
            } else if reading.corrected && matches(&words, &reading.groups) {
                found.insert(i, Found::Corrected(self.closeness(i, &query)));
            }
        }
        found
    }

    /// How alike the name is to `query`, without a first word too (most
    /// often a region: "UK| …").
    fn closeness(&self, i: usize, query: &str) -> std::cmp::Reverse<u8> {
        let name = self.trigrams.name(i);
        let rest = name.split_once(' ').map_or(name, |(_, rest)| rest);
        closeness(name, query).min(closeness(rest, query))
    }
}

/// Whether every group has a term found in a name's words.
fn matches(name: &[&str], groups: &[Vec<Term>]) -> bool {
    groups.iter().all(|group| {
        group.iter().any(|term| match term {
            Term::Begins(w) => name.iter().any(|n| n.starts_with(w.as_str())),
            Term::Phrase(ws) => name.windows(ws.len()).any(|run| run.iter().eq(ws.iter())),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINEUP: &[&str] = &[
        "UK| NEWS ONE HD",
        "UK| ARENA SPORTS F1 FHD",
        "DE| ARENA SPORTS F1 HD",
        "US| CINEMA ᴴᴰ",
        "US| CINEMA 2",
        "BG| MOTORSPORT HD",
        "XX| HARBOR TV DOCU",
        "DE| DAS KINO FHD",
    ];

    fn find(search: &str) -> Vec<(usize, Found)> {
        let mut found: Vec<_> = Names::new(LINEUP.iter().copied()).find(search).into_iter().collect();
        found.sort_by_key(|&(i, how)| (how, i));
        found
    }

    fn places(search: &str) -> Vec<usize> {
        find(search).into_iter().map(|(i, _)| i).collect()
    }

    #[test]
    fn every_word_begins_a_word_of_the_name_in_any_order() {
        assert_eq!(places("arena f1"), [1, 2]);
        assert_eq!(places("f1 arena"), [1, 2]);
        assert_eq!(places("uk arena"), [1]);
        assert_eq!(places("cinema hd"), [3], "ᴴᴰ is hd");
        assert_eq!(places("sport"), [1, 2, 5], "motorsport has it in it");
        assert!(find("news").iter().all(|(_, how)| *how == Found::AsTyped));
    }

    #[test]
    fn a_quality_no_channel_has_is_left_out() {
        assert_eq!(places("cinema 2 hd"), [4]);
        assert_eq!(places("das kino uhd"), [7]);
    }

    #[test]
    fn slips_find_the_closest_names() {
        assert_eq!(places("motorsprt"), [5]);
        assert_eq!(places("harbr tv docu"), [6]);
        assert_eq!(places("das kinno"), [7]);
        assert!(matches!(find("motorsprt")[0].1, Found::Corrected(_)));
        assert_eq!(places("newsone hd"), [0], "two words run together");
    }

    #[test]
    fn nothing_else_found_names_alike_are() {
        // "onehd" is no word, nor two words of three letters or more.
        assert_eq!(places("news onehd"), [0]);
        assert!(matches!(find("news onehd")[0].1, Found::Alike(_)));
        assert!(places("cartoon network").is_empty());
        assert!(places("  ").is_empty());
    }
}
