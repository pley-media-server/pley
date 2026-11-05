//! What TMDB has for the words that the libraries lack (the Discover row in the TV app's
//! `search.tsx`), for whoever may ask for titles: asked beside the libraries' search, with the
//! same pause, so it never holds them up; only the latest words' answer counts. Titles shown stay
//! while the same words are asked again (requests changed); a failure says why, with Retry, and
//! is asked again only by Retry or other words.

use pley_core::ApiError;
use pley_core::requests::title_key;

use crate::results::TmdbTile;

/// What the Discover row shows for its words.
#[derive(Debug)]
pub enum TmdbState {
    /// On its way: grey wells.
    Loading,
    /// The titles; none leaves the row out.
    Ready(Vec<TmdbTile>),
    /// Why TMDB didn't answer, with Retry where it may help.
    Failed(ApiError),
}

/// The Discover row's words, asked and answered.
#[derive(Debug, Default)]
pub struct OnTmdb {
    /// The words of what shows, or is coming.
    words: String,
    state: Option<TmdbState>,
    /// The words whose answer is awaited; the answer to any other no longer counts.
    asking: Option<String>,
    /// Changes when the row's words change: it opens at its first tile then.
    turn: u64,
}

impl OnTmdb {
    /// Nothing asked.
    pub fn new() -> OnTmdb {
        OnTmdb::default()
    }

    /// The libraries are asked for words that TMDB's search takes as `words`: the words to ask
    /// TMDB for, unless they are what shows or is coming, or failed (Retry asks those again).
    pub fn want(&mut self, words: &str) -> Option<String> {
        if words.is_empty() || self.asking.as_deref() == Some(words) {
            return None;
        }
        if self.words == words && self.state.is_some() {
            return None;
        }
        if self.words != words {
            self.words = words.to_string();
            self.state = None;
            self.turn += 1;
        }
        self.state.get_or_insert(TmdbState::Loading);
        self.asking = Some(words.to_string());
        Some(words.to_string())
    }

    /// Retry after a failure: the same words again; why it failed shows until the answer.
    pub fn retry(&mut self) -> Option<String> {
        if !matches!(self.state, Some(TmdbState::Failed(_))) || self.asking.is_some() {
            return None;
        }
        self.asking = Some(self.words.clone());
        self.asking.clone()
    }

    /// Requests changed: the titles shown are asked again for their badges, and stay meanwhile.
    pub fn refresh(&mut self) -> Option<String> {
        if !matches!(self.state, Some(TmdbState::Ready(_))) || self.asking.is_some() {
            return None;
        }
        self.asking = Some(self.words.clone());
        self.asking.clone()
    }

    /// The answer to `words` came. Asked again, titles shown stay when it fails; a cancelled
    /// request says nothing, and its words are asked again at the next chance.
    pub fn answered(&mut self, words: &str, answer: Result<Vec<TmdbTile>, ApiError>) {
        if self.asking.as_deref() != Some(words) {
            return;
        }
        self.asking = None;
        match answer {
            Err(e) if e.cancelled() => {
                if matches!(self.state, Some(TmdbState::Loading)) {
                    self.state = None;
                }
            }
            Err(e) => {
                log::info!("TMDB search failed: {e}");
                if !matches!(self.state, Some(TmdbState::Ready(_))) {
                    self.state = Some(TmdbState::Failed(e));
                }
            }
            Ok(tiles) => self.state = Some(TmdbState::Ready(tiles)),
        }
    }

    /// Forgets everything (another sign-in or profile): their titles' badges were another's.
    pub fn clear(&mut self) {
        *self = OnTmdb { turn: self.turn + 1, ..OnTmdb::default() };
    }

    /// Whether the titles shown hold the title known as `key` (`kind:tmdb_id`).
    pub fn holds(&self, key: &str) -> bool {
        match &self.state {
            Some(TmdbState::Ready(tiles)) => tiles.iter().any(|t| title_key(t.kind, t.tmdb_id) == key),
            _ => false,
        }
    }

    /// What the row shows for `words`: `None` before they were asked.
    pub fn shown(&self, words: &str) -> Option<&TmdbState> {
        self.state.as_ref().filter(|_| self.words == words)
    }

    /// The words whose answer still counts; a request for any other can stop.
    pub fn asking(&self) -> Option<&str> {
        self.asking.as_deref()
    }

    /// Changes when the row's words change.
    pub fn turn(&self) -> u64 {
        self.turn
    }
}

/// Whether the Discover row shows for words whose state is `state`: while it comes, when it
/// failed, and with titles; TMDB having nothing leaves it out.
pub fn shows(state: Option<&TmdbState>) -> bool {
    !matches!(state, Some(TmdbState::Ready(tiles)) if tiles.is_empty())
}

#[cfg(test)]
mod tests {
    use pley_core::NetError;
    use pley_core::model::TitleKind;
    use pley_core::requests::Tone;

    use super::*;

    fn tile(id: i64) -> TmdbTile {
        TmdbTile {
            kind: TitleKind::Movie,
            tmdb_id: id,
            title: format!("T{id}"),
            subtitle: String::new(),
            art: Vec::new(),
            badge: None,
            tone: Tone::Blue,
            item_id: None,
        }
    }

    fn titles(t: &OnTmdb, words: &str) -> Vec<i64> {
        match t.shown(words) {
            Some(TmdbState::Ready(tiles)) => tiles.iter().map(|t| t.tmdb_id).collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn words_are_asked_once_and_only_the_latest_answer_counts() {
        let mut t = OnTmdb::new();
        assert_eq!(t.want("dune").as_deref(), Some("dune"));
        assert!(matches!(t.shown("dune"), Some(TmdbState::Loading)));
        assert_eq!(t.want("dune"), None, "asked once");
        let turn = t.turn();
        assert_eq!(t.want("dune part").as_deref(), Some("dune part"));
        assert_ne!(t.turn(), turn);
        // The answer to the words before comes late: it no longer counts.
        t.answered("dune", Ok(vec![tile(1)]));
        assert!(t.shown("dune").is_none());
        t.answered("dune part", Ok(vec![tile(2)]));
        assert_eq!(titles(&t, "dune part"), [2]);
        assert_eq!(t.want("dune part"), None, "the libraries asked again: TMDB's answer stands");
        assert_eq!(t.want(""), None);
    }

    #[test]
    fn a_failure_waits_for_retry_and_shows_until_the_answer() {
        let mut t = OnTmdb::new();
        t.want("dune");
        t.answered("dune", Err(ApiError::Net(NetError::Refused)));
        assert!(matches!(t.shown("dune"), Some(TmdbState::Failed(_))));
        assert_eq!(t.want("dune"), None, "not asked again by itself");
        assert_eq!(t.retry().as_deref(), Some("dune"));
        assert!(matches!(t.shown("dune"), Some(TmdbState::Failed(_))), "Retry stays where it was");
        assert_eq!(t.retry(), None, "once at a time");
        t.answered("dune", Ok(vec![tile(1)]));
        assert_eq!(titles(&t, "dune"), [1]);
    }

    #[test]
    fn titles_asked_again_stay_when_that_fails() {
        let mut t = OnTmdb::new();
        t.want("dune");
        t.answered("dune", Ok(vec![tile(1)]));
        let turn = t.turn();
        assert!(t.holds("movie:1") && !t.holds("show:1"));
        assert_eq!(t.refresh().as_deref(), Some("dune"));
        t.answered("dune", Err(ApiError::Net(NetError::TimedOut)));
        assert_eq!(titles(&t, "dune"), [1]);
        assert_eq!(t.refresh().as_deref(), Some("dune"));
        t.answered("dune", Ok(vec![tile(1), tile(3)]));
        assert_eq!(titles(&t, "dune"), [1, 3]);
        assert_eq!(t.turn(), turn, "the row keeps its place");
    }

    #[test]
    fn a_cancelled_request_is_asked_again() {
        let mut t = OnTmdb::new();
        t.want("dune");
        t.answered("dune", Err(ApiError::Net(NetError::Cancelled)));
        assert!(t.shown("dune").is_none());
        assert_eq!(t.want("dune").as_deref(), Some("dune"));
    }

    #[test]
    fn the_row_shows_unless_tmdb_has_nothing() {
        assert!(shows(None));
        assert!(shows(Some(&TmdbState::Loading)));
        assert!(shows(Some(&TmdbState::Failed(ApiError::Net(NetError::Refused)))));
        assert!(shows(Some(&TmdbState::Ready(vec![tile(1)]))));
        assert!(!shows(Some(&TmdbState::Ready(Vec::new()))));
    }
}
