//! The A–Z jump's index of a listing (`AlphaIndex.kt`; the TV app's `AlphaIndex`): where each
//! letter's titles start, and the letter of any position.

use pley_kit::JumpControl;

/// The jump control shows only for listings of more than this many titles (`AlphaIndex.kt:62`).
pub const JUMP_FROM: usize = 30;

/// Where each of the 27 sections (`#`, then A to Z) starts in a listing sorted by title.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlphaIndex {
    firsts: [Option<usize>; 27],
    /// Each run of titles with the same letter: its section and first position.
    runs: Vec<(usize, usize)>,
}

impl AlphaIndex {
    /// The index of titles listed in order, each by its sort title (else its title).
    pub fn new<'a>(titles: impl IntoIterator<Item = &'a str>) -> AlphaIndex {
        let mut index = AlphaIndex::default();
        for (i, title) in titles.into_iter().enumerate() {
            let section = JumpControl::section_of(title.trim());
            if index.firsts[section].is_none() {
                index.firsts[section] = Some(i);
            }
            if index.runs.last().is_none_or(|r| r.0 != section) {
                index.runs.push((section, i));
            }
        }
        index
    }

    /// Which sections have titles, for the control's dimmed letters.
    pub fn available(&self) -> [bool; 27] {
        self.firsts.map(|f| f.is_some())
    }

    /// The section of the run `position` is in ("#" before any).
    pub fn letter_at(&self, position: usize) -> usize {
        let at = self.runs.partition_point(|r| r.1 <= position);
        at.checked_sub(1).map_or(0, |i| self.runs[i].0)
    }

    /// Whether a title starts with section `letter`.
    pub fn has(&self, letter: usize) -> bool {
        self.firsts.get(letter).is_some_and(|f| f.is_some())
    }

    /// Where choosing `letter` goes: its first title, else the next letter's, else the nearest
    /// before it, else the start.
    pub fn jump_target(&self, letter: usize) -> usize {
        let letter = letter.min(26);
        if let Some(at) = self.firsts[letter] {
            return at;
        }
        self.firsts[letter + 1..]
            .iter()
            .flatten()
            .next()
            .or_else(|| self.firsts[..letter].iter().rev().flatten().next())
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_come_from_sort_titles_without_accents() {
        assert_eq!(JumpControl::section_of("Amélie"), 1);
        assert_eq!(JumpControl::section_of("Édith"), 5);
        assert_eq!(JumpControl::section_of("2001"), 0);
        assert_eq!(JumpControl::section_of("zulu"), 26);
    }

    #[test]
    fn the_index_finds_runs_and_jumps() {
        let titles = ["10 Things", "Alien", "Amélie", "Brazil", "Dune", "Dunkirk", "Zodiac"];
        let index = AlphaIndex::new(titles);
        assert_eq!(index.letter_at(0), 0);
        assert_eq!(index.letter_at(2), 1);
        assert_eq!(index.letter_at(5), 4);
        assert_eq!(index.letter_at(6), 26);
        assert!(index.has(2) && !index.has(3));
        // C has nothing: D's first title; after Z nothing: back to Z's.
        assert_eq!(index.jump_target(3), 4);
        assert_eq!(index.jump_target(1), 1);
        assert_eq!(index.jump_target(25), 6);
        let available = index.available();
        assert!(available[0] && available[1] && !available[3] && available[26]);
        assert_eq!(AlphaIndex::new([]).jump_target(5), 0);
        assert_eq!(AlphaIndex::new([]).letter_at(3), 0);
    }
}
