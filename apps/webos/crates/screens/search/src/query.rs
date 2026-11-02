//! The query, from what is typed to what shows (`SearchViewModel`): the trimmed text is asked once
//! typing has paused 250 ms (at once when it empties), only the latest query's answer counts, and
//! the results shown stay, with "Searching…", while the next ones come. The results shown, asked
//! again quietly, stay when that fails (`load.stale`).

use pley_core::ApiError;

use crate::results::Found;

/// How long typing pauses before what is typed is asked (`DEBOUNCE_MS`).
pub const DEBOUNCE: f64 = 0.25;

/// How many titles a search asks for (`limit=60`, as both TV apps ask).
pub const LIMIT: u32 = 60;

/// After this long, results are asked again when the page shows again (the app's 30 s rule).
pub const REVALIDATE: f64 = pley_shell::REVALIDATE;

/// What the page shows under the field.
#[derive(Debug, Default)]
pub enum Shown {
    /// Nothing typed: recent searches, or what search finds.
    #[default]
    Idle,
    /// The first answer for `query` is on its way, with nothing earlier to keep showing.
    Loading { query: String },
    /// Results for `query`.
    Ready { query: String, found: Found },
    /// `No results for “north”`.
    Empty { query: String },
    /// "Search failed" (or "Can't reach your server"), with Retry.
    Failed { query: String, error: ApiError },
}

/// The query as typed, asked and answered.
#[derive(Debug, Default)]
pub struct Query {
    /// What is typed, trimmed: what is asked next.
    wanted: String,
    /// When `wanted` is asked.
    due: Option<f64>,
    /// The query whose answer is awaited; the answer to any other no longer counts.
    asking: Option<String>,
    /// Whether the query asked replaces what shows quietly (the same query asked again).
    quiet: bool,
    shown: Shown,
    /// When what shows was answered.
    answered_at: Option<f64>,
    /// Counts the times the results shown became another query's.
    turn: u64,
}

impl Query {
    /// Nothing typed.
    pub fn new() -> Query {
        Query::default()
    }

    /// The field now says `text`: a different query is asked 250 ms from `now`, unless more is
    /// typed first; an empty one shows the recent searches at once.
    pub fn typed(&mut self, text: &str, now: f64) {
        let q = text.trim();
        if q == self.wanted {
            return;
        }
        self.wanted = q.to_string();
        if q.is_empty() {
            self.due = None;
            self.asking = None;
            self.set_shown(Shown::Idle);
        } else {
            self.due = Some(now + DEBOUNCE);
        }
    }

    /// Asks for what is typed now, without waiting for typing to pause (the keyboard's Search).
    pub fn now(&mut self, now: f64) {
        if self.due.is_some() {
            self.due = Some(now);
        }
    }

    /// The query to ask now, when its time has come. What shows becomes "loading" unless results
    /// are showing, which stay until the answer comes.
    pub fn ask(&mut self, now: f64) -> Option<String> {
        if self.due.is_none_or(|due| now < due) || self.wanted.is_empty() {
            return None;
        }
        self.due = None;
        let q = self.wanted.clone();
        let same = matches!(&self.shown, Shown::Ready { query, .. } if *query == q);
        self.quiet = same;
        if !matches!(self.shown, Shown::Ready { .. }) {
            self.set_shown(Shown::Loading { query: q.clone() });
        }
        self.asking = Some(q.clone());
        Some(q)
    }

    /// The answer to `query` came at `now`, sorted into rows. An answer to a query no longer
    /// asked, or one cancelled, changes nothing. A failure takes the place of what an earlier
    /// search found, which no longer matches the field; the results of the same query, asked again
    /// (the page shown again, the server's content changed), stay with their rows and focus.
    pub fn answered(&mut self, query: &str, answer: Result<Found, ApiError>, now: f64) {
        if self.asking.as_deref() != Some(query) {
            return;
        }
        let shown = match answer {
            Err(e) if e.cancelled() => return,
            Err(_) if self.quiet => {
                // Still the results of what the field says: asked again at the next chance.
                self.asking = None;
                self.quiet = false;
                return;
            }
            Err(error) => Shown::Failed { query: query.to_string(), error },
            Ok(found) if found.is_empty() => Shown::Empty { query: query.to_string() },
            Ok(found) => Shown::Ready { query: query.to_string(), found },
        };
        self.asking = None;
        self.answered_at = Some(now);
        let quiet = std::mem::take(&mut self.quiet) && matches!(shown, Shown::Ready { .. });
        if quiet {
            self.shown = shown;
        } else {
            self.set_shown(shown);
        }
    }

    /// Retry after a failure: the same query, asked at once.
    pub fn retry(&mut self, now: f64) {
        if !self.wanted.is_empty() && self.asking.is_none() {
            self.due = Some(now);
        }
    }

    /// Asks what is typed again from the start (another profile is watching): the answer awaited
    /// no longer counts, and what shows is asked for anew.
    pub fn restart(&mut self, now: f64) {
        self.asking = None;
        self.quiet = false;
        if self.wanted.is_empty() {
            self.due = None;
            self.set_shown(Shown::Idle);
        } else {
            self.due = Some(now);
            self.set_shown(Shown::Loading { query: self.wanted.clone() });
        }
    }

    /// Asks the query shown again (the server's content changed): its results stay meanwhile.
    pub fn refresh(&mut self, now: f64) {
        if matches!(&self.shown, Shown::Ready { query, .. } if *query == self.wanted)
            && self.asking.is_none()
            && self.due.is_none()
        {
            self.due = Some(now);
        }
    }

    /// Asks the results shown again if they are older than 30 s (the page shows again).
    pub fn revalidate(&mut self, now: f64) {
        if self.answered_at.is_some_and(|t| now - t > REVALIDATE) {
            self.refresh(now);
        }
    }

    fn set_shown(&mut self, shown: Shown) {
        let before = self.shown_query().map(str::to_owned);
        self.shown = shown;
        if !matches!(self.shown, Shown::Ready { .. }) || before.as_deref() != self.shown_query() {
            self.turn += 1;
        }
    }

    /// What shows.
    pub fn shown(&self) -> &Shown {
        &self.shown
    }

    /// The query whose results show, if results show.
    pub fn shown_query(&self) -> Option<&str> {
        match &self.shown {
            Shown::Ready { query, .. } => Some(query),
            _ => None,
        }
    }

    /// The query the libraries answered, with results or none: a result opened from the page
    /// keeps it among the recent searches.
    pub fn answered_query(&self) -> Option<&str> {
        match &self.shown {
            Shown::Ready { query, .. } | Shown::Empty { query } => Some(query),
            _ => None,
        }
    }

    /// When the next query is asked.
    pub fn due(&self) -> Option<f64> {
        self.due
    }

    /// Whether an answer is on its way: "Searching…" shows beside the field.
    pub fn searching(&self) -> bool {
        self.asking.is_some()
    }

    /// The query whose answer still counts; a request for any other can stop.
    pub fn asking(&self) -> Option<&str> {
        self.asking.as_deref()
    }

    /// What is typed, trimmed.
    pub fn wanted(&self) -> &str {
        &self.wanted
    }

    /// Changes whenever the results shown become another query's (or go): rows then open at their
    /// first tile, and Back from a result comes to it, not to where the last results were.
    pub fn turn(&self) -> u64 {
        self.turn
    }
}

#[cfg(test)]
mod tests {
    use pley_base::ImageRequest;
    use pley_core::NetError;
    use pley_core::model::{ItemKind, ItemSummary, SearchResults};
    use pley_kit::TileKind;

    use super::*;

    fn none(_: &str, _: TileKind) -> Option<ImageRequest> {
        None
    }

    fn found(titles: &[&str]) -> Found {
        let items = titles
            .iter()
            .enumerate()
            .map(|(i, t)| ItemSummary {
                id: i as i64 + 1,
                kind: ItemKind::Movie,
                title: (*t).into(),
                ..Default::default()
            })
            .collect();
        Found::new(SearchResults { items, ..SearchResults::default() }, &none)
    }

    fn titles(q: &Query) -> Vec<String> {
        match q.shown() {
            Shown::Ready { found, .. } => {
                found.titles.iter().flat_map(|(_, t)| t.iter().map(|t| t.title.clone())).collect()
            }
            _ => Vec::new(),
        }
    }

    #[test]
    fn typing_is_asked_once_it_pauses() {
        let mut q = Query::new();
        q.typed("n", 0.0);
        q.typed("no", 0.1);
        q.typed("nor", 0.2);
        assert_eq!(q.ask(0.3), None, "250 ms after the last key, not the first");
        assert!(matches!(q.shown(), Shown::Idle));
        assert_eq!(q.due(), Some(0.45));
        assert_eq!(q.ask(0.45).as_deref(), Some("nor"));
        assert!(matches!(q.shown(), Shown::Loading { query } if query == "nor"));
        assert!(q.searching());
        assert_eq!(q.ask(0.5), None, "asked once");
        q.answered("nor", Ok(found(&["North"])), 0.6);
        assert_eq!(titles(&q), ["North"]);
        assert!(!q.searching());
    }

    #[test]
    fn spaces_around_the_text_ask_nothing_new() {
        let mut q = Query::new();
        q.typed("north", 0.0);
        q.ask(0.25);
        q.answered("north", Ok(found(&["North"])), 0.3);
        q.typed("north ", 1.0);
        q.typed(" north", 1.1);
        assert_eq!(q.due(), None);
        assert_eq!(q.wanted(), "north");
    }

    #[test]
    fn results_stay_while_the_next_ones_come_and_only_the_latest_counts() {
        let mut q = Query::new();
        q.typed("nor", 0.0);
        q.ask(0.25);
        q.answered("nor", Ok(found(&["North", "Northern"])), 0.3);
        let turn = q.turn();
        q.typed("north", 1.0);
        assert_eq!(q.ask(1.25).as_deref(), Some("north"));
        assert_eq!(titles(&q), ["North", "Northern"], "kept, with Searching…");
        assert!(q.searching());
        // An answer to the query before arrives late: it no longer counts.
        q.answered("nor", Ok(found(&["Nope"])), 1.3);
        assert_eq!(titles(&q), ["North", "Northern"]);
        q.answered("north", Ok(found(&["North"])), 1.4);
        assert_eq!(titles(&q), ["North"]);
        assert_ne!(q.turn(), turn, "another query's results");
    }

    #[test]
    fn emptying_the_field_shows_recent_searches_at_once() {
        let mut q = Query::new();
        q.typed("north", 0.0);
        q.ask(0.25);
        q.typed("", 0.3);
        assert!(matches!(q.shown(), Shown::Idle));
        assert!(!q.searching());
        assert_eq!(q.due(), None);
        // The answer to the query abandoned changes nothing.
        q.answered("north", Ok(found(&["North"])), 0.4);
        assert!(matches!(q.shown(), Shown::Idle));
    }

    #[test]
    fn nothing_found_and_failures_show_their_notices_and_retry_asks_again() {
        let mut q = Query::new();
        q.typed("zzz", 0.0);
        q.ask(0.25);
        q.answered("zzz", Ok(found(&[])), 0.3);
        assert!(matches!(q.shown(), Shown::Empty { query } if query == "zzz"));
        // Nothing in the libraries, and still the query of what shows (Discover's row).
        assert_eq!((q.answered_query(), q.shown_query()), (Some("zzz"), None));

        q.typed("north", 1.0);
        q.ask(1.25);
        assert!(matches!(q.shown(), Shown::Loading { .. }), "after a notice, the skeletons");
        q.answered("north", Err(ApiError::Net(NetError::Refused)), 1.3);
        assert!(matches!(q.shown(), Shown::Failed { error, .. } if error.unreachable()));

        q.retry(2.0);
        assert_eq!(q.ask(2.0).as_deref(), Some("north"));
        assert!(matches!(q.shown(), Shown::Loading { .. }));
        // A cancelled request says nothing.
        q.answered("north", Err(ApiError::Net(NetError::Cancelled)), 2.1);
        assert!(matches!(q.shown(), Shown::Loading { .. }));
        q.answered("north", Ok(found(&["North"])), 2.2);
        assert_eq!(titles(&q), ["North"]);
    }

    #[test]
    fn a_failure_replaces_results_that_no_longer_match_the_field() {
        let mut q = Query::new();
        q.typed("nor", 0.0);
        q.ask(0.25);
        q.answered("nor", Ok(found(&["North"])), 0.3);
        q.typed("north", 1.0);
        q.ask(1.25);
        q.answered("north", Err(ApiError::Net(NetError::TimedOut)), 1.3);
        assert!(matches!(q.shown(), Shown::Failed { query, .. } if query == "north"));
    }

    #[test]
    fn the_same_query_asked_again_replaces_its_results_quietly() {
        let mut q = Query::new();
        q.typed("north", 0.0);
        q.ask(0.25);
        q.answered("north", Ok(found(&["North"])), 0.3);
        let turn = q.turn();
        q.revalidate(10.0);
        assert_eq!(q.due(), None, "not yet 30 s old");
        q.revalidate(31.0);
        assert_eq!(q.ask(31.0).as_deref(), Some("north"));
        q.answered("north", Ok(found(&["North", "North by Northwest"])), 31.1);
        assert_eq!(titles(&q), ["North", "North by Northwest"]);
        assert_eq!(q.turn(), turn, "focus stays where it was");
    }

    #[test]
    fn the_results_shown_stay_when_asking_them_again_fails() {
        let mut q = Query::new();
        q.typed("north", 0.0);
        q.ask(0.25);
        q.answered("north", Ok(found(&["North"])), 0.3);
        let turn = q.turn();
        q.refresh(5.0);
        assert_eq!(q.ask(5.0).as_deref(), Some("north"));
        q.answered("north", Err(ApiError::Net(NetError::Refused)), 5.1);
        assert_eq!(titles(&q), ["North"], "kept, as Android keeps the stale answer");
        assert_eq!(q.turn(), turn, "rows and focus stay");
        assert!(!q.searching());
        // Asked again at the next chance, and a later answer replaces them quietly.
        q.refresh(6.0);
        assert_eq!(q.ask(6.0).as_deref(), Some("north"));
        q.answered("north", Ok(found(&["North", "Northern"])), 6.1);
        assert_eq!(titles(&q), ["North", "Northern"]);
        assert_eq!(q.turn(), turn);
    }

    #[test]
    fn search_on_the_keyboard_asks_without_waiting() {
        let mut q = Query::new();
        q.typed("north", 0.0);
        q.now(0.1);
        assert_eq!(q.ask(0.1).as_deref(), Some("north"));
        q.now(0.2);
        assert_eq!(q.due(), None, "nothing more to ask");
    }
}
