//! What the app knows of requests between pages (the TV app's `state.ts`): the summary that
//! decides whether Discover and the requests show and the rail's number, when it is asked for
//! again, the news that pops up, and the changes that pages showing requests hear.
//!
//! [`crate::AppState`] owns it and does the asking; this keeps the rules: a burst of request
//! events asks for the summary once, 1 s after the last, and tells the pages once, 0.3 s after
//! the last; while the events are down the summary is asked for every 60 s, and the pages hear
//! when it moved; ready-to-watch news pops up once per request (or per batch of a show's
//! seasons), remembered per server and profile.

use std::collections::{BTreeSet, VecDeque};

use pley_core::model::{MediaRequest, RequestSummary};
use pley_core::requests::{news_keys, news_text, ready_news, ready_text};
use serde::{Deserialize, Serialize};

/// Request events come in bursts (a scan, an import, a manager's round): the summary is asked for
/// once, this long after the last (seconds).
pub const SUMMARY_GATHER: f64 = 1.0;
/// …and the pages that show requests hear once, this long after the last.
pub const REQUEST_GATHER: f64 = 0.3;
/// While the events are down, the summary is asked for this often.
pub const SUMMARY_POLL: f64 = 60.0;
/// Ready-to-watch notices kept while they can't show (a video plays): the last three.
pub const NOTICES_KEPT: usize = 3;
/// How many pieces of news are remembered as announced.
const NEWS_KEPT: usize = 200;

/// News of requests popped up over the pages: on TVs, only what is ready to watch.
#[derive(Clone, Debug, PartialEq)]
pub struct RequestNotice {
    pub id: u64,
    pub text: String,
    /// Open goes to the title in the library; `None` goes to Your requests.
    pub item_id: Option<i64>,
    /// The title's poster (TMDB's, through the server); none for news of several.
    pub poster: Option<String>,
}

/// The news announced to a profile on a server: the newest `updated_at`, and each piece once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewsKept {
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub keys: Vec<String>,
}

impl NewsKept {
    /// Keeps `keys` as announced, and `at` as the newest; whether any was new.
    pub fn remember(&mut self, keys: &[String], at: i64) -> bool {
        let fresh: Vec<String> = keys.iter().filter(|k| !self.keys.contains(k)).cloned().collect();
        self.at = self.at.max(at);
        let any = !fresh.is_empty();
        self.keys.extend(fresh);
        let over = self.keys.len().saturating_sub(NEWS_KEPT);
        self.keys.drain(..over);
        any
    }
}

/// What the requests summary, its timers and news hold for the profile in use.
#[derive(Debug, Default)]
pub struct RequestState {
    /// `None` while unknown, and for a server without requests.
    pub summary: Option<RequestSummary>,
    /// The server has no requests (404) or an older build of them (403): not asked again until
    /// the events reconnect.
    pub missing: bool,
    /// An event came since the summary on its way was asked for: once more after it.
    pub again: bool,
    pub summary_due: Option<f64>,
    pub poll_due: Option<f64>,
    change_due: Option<f64>,
    changed_titles: BTreeSet<String>,
    /// The events aren't connected: pages that show requests ask on their own meanwhile.
    pub events_down: bool,
    /// News events heard since the last summary: the `unseen` they raised needs no catching up.
    pub news_heard: u32,
    /// This profile's news was caught up with since it was picked.
    pub caught_up: bool,
    /// The news announced, once read for this profile.
    pub news: Option<NewsKept>,
    notices: VecDeque<RequestNotice>,
    notice_ids: u64,
    on_screen: Option<u64>,
}

/// What a new summary asks of the state.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SummaryNews {
    /// Pages that show requests ask again (it moved while the events were down).
    pub changed: bool,
    /// News no event explained: the profile's unseen requests are read for what is ready.
    pub catch_up: bool,
}

impl RequestState {
    /// Another profile, sign-in or server: everything goes.
    pub fn reset(&mut self) {
        *self = RequestState { events_down: self.events_down, poll_due: self.poll_due, ..RequestState::default() };
    }

    /// The summary came: kept, and what it means for the pages and the news.
    pub fn on_summary(&mut self, summary: RequestSummary) -> SummaryNews {
        let before = self.summary.replace(summary);
        let summary = self.summary.as_ref().expect("just set");
        let changed = self.events_down && before.as_ref().is_some_and(|b| b.changed_at != summary.changed_at);
        // News no event explained (it came while the app wasn't listening), once at start too.
        let unexplained = before.as_ref().is_some_and(|b| summary.unseen > b.unseen) && self.news_heard == 0;
        self.news_heard = 0;
        let catch_up = summary.enabled
            && summary.sign_in_again != Some(pley_core::model::SignInAgain::Requests)
            && summary.unseen > 0
            && (!self.caught_up || unexplained);
        SummaryNews { changed, catch_up }
    }

    /// The summary once, a second after the last request event of a burst.
    pub fn summary_soon(&mut self, now: f64) {
        self.summary_due = Some(now + SUMMARY_GATHER);
    }

    /// The pages that show requests hear, once for a burst: about one title (`kind:tmdb_id`), or
    /// about any (`None`).
    pub fn changed(&mut self, title: Option<String>, now: f64) {
        self.changed_titles.insert(title.unwrap_or_else(|| "*".into()));
        if self.change_due.is_none() {
            self.change_due = Some(now + REQUEST_GATHER);
        }
    }

    /// The change the pages hear now, when its time came: one title, or `None` for any.
    pub fn take_change(&mut self, now: f64) -> Option<Option<String>> {
        if !self.change_due.is_some_and(|t| now >= t) {
            return None;
        }
        self.change_due = None;
        let titles = std::mem::take(&mut self.changed_titles);
        let one = (titles.len() == 1).then(|| titles.into_iter().next()).flatten().filter(|t| t != "*");
        Some(one)
    }

    /// When a timer is next due.
    pub fn next_due(&self) -> Option<f64> {
        [self.summary_due, self.poll_due, self.change_due].into_iter().flatten().reduce(f64::min)
    }

    /// Ready-to-watch news from an event: popped up unless it was announced before.
    pub fn ready(
        &mut self,
        request_id: i64,
        title: &str,
        item_id: Option<i64>,
        poster: Option<String>,
        seasons: Option<&[i64]>,
        complete: bool,
    ) {
        let keys = news_keys(request_id, complete, seasons);
        // Whose news it is isn't known yet: announced anyway.
        let fresh = self.news.as_mut().is_none_or(|n| n.remember(&keys, 0));
        if fresh {
            self.notify(ready_text(title, seasons, complete), item_id, poster);
        }
    }

    /// The profile's unseen requests, read while catching up: what is ready and newer than the
    /// last announced pops up, one title by its words, several as "3 of your requests have news".
    /// Whether the news kept changed (to be saved).
    pub fn caught_up_with(&mut self, requests: &[MediaRequest]) -> bool {
        self.caught_up = true;
        let Some(kept) = self.news.as_mut() else { return false };
        let keys = |r: &MediaRequest| {
            news_keys(r.id, r.stage == pley_core::model::RequestStage::Available, Some(&r.available_seasons))
        };
        let news: Vec<&MediaRequest> = requests
            .iter()
            .filter(|r| ready_news(r) && r.updated_at > kept.at && keys(r).iter().any(|k| !kept.keys.contains(k)))
            .collect();
        if news.is_empty() {
            return false;
        }
        let all: Vec<String> = news.iter().flat_map(|r| keys(r)).collect();
        let newest = news.iter().map(|r| r.updated_at).max().unwrap_or(0);
        kept.remember(&all, newest);
        let one = news[0];
        if news.len() == 1 {
            let complete = one.stage == pley_core::model::RequestStage::Available;
            let text = ready_text(&one.title, Some(&one.available_seasons), complete);
            self.notify(text, one.item_id, one.poster_url.clone());
        } else {
            self.notify(news_text(news.len()), None, None);
        }
        true
    }

    /// A pop-up that waits its turn (while a video plays, among the last three); the one on
    /// screen stays until it closes.
    fn notify(&mut self, text: String, item_id: Option<i64>, poster: Option<String>) {
        self.notice_ids += 1;
        self.notices.push_back(RequestNotice { id: self.notice_ids, text, item_id, poster });
        let on_screen = self.on_screen;
        let waiting = self.notices.iter().filter(|n| Some(n.id) != on_screen).count();
        let mut drop = waiting.saturating_sub(NOTICES_KEPT);
        self.notices.retain(|n| {
            if drop > 0 && Some(n.id) != on_screen {
                drop -= 1;
                return false;
            }
            true
        });
    }

    /// The notice to show next, oldest first.
    pub fn notice(&self) -> Option<&RequestNotice> {
        self.notices.front()
    }

    /// The shell shows this notice (`None`: none).
    pub fn showing_notice(&mut self, id: Option<u64>) {
        self.on_screen = id;
    }

    /// The notice was opened, closed or timed out.
    pub fn drop_notice(&mut self, id: u64) {
        if self.on_screen == Some(id) {
            self.on_screen = None;
        }
        self.notices.retain(|n| n.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::{RequestStage, TitleKind};

    #[test]
    fn a_burst_tells_the_pages_once_about_one_title_or_any() {
        let mut s = RequestState::default();
        s.changed(Some("movie:1".into()), 10.0);
        s.changed(Some("movie:1".into()), 10.1);
        assert_eq!(s.take_change(10.2), None);
        assert_eq!(s.take_change(10.3), Some(Some("movie:1".into())));
        s.changed(Some("movie:1".into()), 11.0);
        s.changed(Some("show:2".into()), 11.0);
        assert_eq!(s.take_change(11.3), Some(None));
        assert_eq!(s.next_due(), None);
    }

    #[test]
    fn ready_news_pops_up_once_and_the_last_three_wait() {
        let mut s = RequestState { news: Some(NewsKept::default()), ..RequestState::default() };
        s.ready(4, "Arrival", Some(9), None, None, true);
        s.ready(4, "Arrival", Some(9), None, None, true);
        assert_eq!(s.notice().map(|n| n.text.as_str()), Some("“Arrival” is ready to watch."));
        let first = s.notice().unwrap().id;
        s.showing_notice(Some(first));
        for id in 5..10 {
            s.ready(id, "Title", None, None, None, true);
        }
        // The one on screen stays; three wait behind it.
        assert_eq!(s.notices.len(), 4);
        assert_eq!(s.notice().unwrap().id, first);
        s.drop_notice(first);
        assert_eq!(s.notices.len(), 3);
    }

    #[test]
    fn catching_up_announces_what_is_ready_and_newer_once() {
        let request = |id, stage, at| MediaRequest {
            id,
            kind: TitleKind::Movie,
            title: format!("T{id}"),
            stage,
            updated_at: at,
            ..MediaRequest::default()
        };
        let mut s = RequestState { news: Some(NewsKept { at: 100, keys: vec![] }), ..RequestState::default() };
        let list = [
            request(1, RequestStage::Available, 200),
            request(2, RequestStage::Pending, 300),
            request(3, RequestStage::Available, 50),
        ];
        assert!(s.caught_up_with(&list));
        assert_eq!(s.notice().map(|n| n.text.as_str()), Some("“T1” is ready to watch."));
        assert!(!s.caught_up_with(&list), "announced already");
        let mut s = RequestState { news: Some(NewsKept::default()), ..RequestState::default() };
        assert!(s.caught_up_with(&[request(1, RequestStage::Available, 1), request(5, RequestStage::Available, 2)]));
        assert_eq!(s.notice().map(|n| n.text.as_str()), Some("2 of your requests have news"));
    }

    #[test]
    fn a_summary_moved_while_the_events_are_down_tells_the_pages() {
        let mut s = RequestState::default();
        let summary = |changed_at, unseen| RequestSummary {
            enabled: true,
            changed_at: Some(changed_at),
            unseen,
            ..RequestSummary::default()
        };
        // The first, with news: caught up with at start.
        assert_eq!(s.on_summary(summary(1, 1)), SummaryNews { changed: false, catch_up: true });
        s.caught_up = true;
        s.events_down = true;
        assert_eq!(s.on_summary(summary(2, 1)), SummaryNews { changed: true, catch_up: false });
        // More unseen than before with no event heard: caught up with again.
        assert_eq!(s.on_summary(summary(2, 2)), SummaryNews { changed: false, catch_up: true });
        s.news_heard = 1;
        assert_eq!(s.on_summary(summary(2, 3)), SummaryNews { changed: false, catch_up: false });
    }
}
