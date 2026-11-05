//! Requests and Discover in words and rules, as every client has them (docs/API.md, "Requests"):
//! who sees what, a title's badge and its one main action, the season picker, a request's stage
//! and actions, the allowance, the news of what is ready, and what each of the server's refusals
//! says. A platform may shorten these words, never reword them; these are the TV app's
//! (`requests.ts`), word for word.
//!
//! Times are unix seconds. Words about days ("tomorrow", "Nov 4") are the TV's own days, so the
//! functions that write them take a [`TvTime`]: the time now, the TV's offset from UTC and its
//! clock format, never the clock read here.

use crate::format::{format_runtime, format_time_of_day, pluralize};
use crate::http::{ApiError, sentence};
use crate::model::{
    Declined, DiscoverDetail, DiscoverList, DiscoverSeason, DiscoverTitle, DownloadState, MediaRequest, RequestAccess,
    RequestAction, RequestCounts, RequestQuery, RequestStage, RequestStatus, RequestSummary, SeasonState, SignInAgain,
    TitleKind, TitleStatus,
};

#[cfg(test)]
mod tests;

/// Discover's foot and every title page say where their details come from.
pub const TMDB_CREDIT: &str = "Details and pictures from TMDB.";
/// Settings → About.
pub const TMDB_NOTICE: &str = "This product uses the TMDB API but is not endorsed or certified by TMDB.";

pub const SIGN_IN_TITLE: &str = "Sign in again to ask for movies and shows";
pub const SIGN_IN_APPROVALS: &str = "Sign in again to approve and decline here.";
pub const NO_ACCESS: &str = "Requests aren't open to this account.";

/// The TV's clock, for words about time: now, the offset of its zone from UTC, and whether it
/// shows "21:05" or "9:05 PM". One offset serves every time written, so a change to or from
/// summer time between now and a time can move that time's day by an hour's worth at most.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TvTime {
    /// Unix seconds now.
    pub now: i64,
    /// The TV's offset from UTC, in seconds.
    pub offset: i64,
    pub hours24: bool,
}

impl TvTime {
    pub fn new(now: i64, offset: i64, hours24: bool) -> TvTime {
        TvTime { now, offset, hours24 }
    }

    /// The TV's day of `t`, in days since 1970-01-01.
    fn day(&self, t: i64) -> i64 {
        (t + self.offset).div_euclid(86_400)
    }
}

/// One of Discover's rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscoverRow {
    pub list: DiscoverList,
    pub title: &'static str,
    /// The row holds movies and shows both, so its tiles say which.
    pub mixed: bool,
}

/// Discover's rows, in order.
pub const DISCOVER_ROWS: [DiscoverRow; 5] = [
    DiscoverRow { list: DiscoverList::Trending, title: "Trending this week", mixed: true },
    DiscoverRow { list: DiscoverList::Movies, title: "Popular movies", mixed: false },
    DiscoverRow { list: DiscoverList::Shows, title: "Popular shows", mixed: false },
    DiscoverRow { list: DiscoverList::Upcoming, title: "Coming to cinemas", mixed: false },
    DiscoverRow { list: DiscoverList::Anime, title: "Popular anime", mixed: false },
];

/// The status tabs of a list of requests, after All.
pub const STATUS_TABS: [(RequestStatus, &str); 5] = [
    (RequestStatus::Pending, "Pending"),
    (RequestStatus::Approved, "Approved"),
    (RequestStatus::Available, "Available"),
    (RequestStatus::Declined, "Declined"),
    (RequestStatus::Failed, "Failed"),
];

/// A tab of a list of requests: Needs attention (managers), All, or one status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RequestsTab {
    Attention,
    All,
    Status(RequestStatus),
}

/// The tabs and their labels, Needs attention first for managers (they open on it).
pub fn requests_tabs(manage: bool) -> Vec<(RequestsTab, &'static str)> {
    let mut tabs = Vec::with_capacity(STATUS_TABS.len() + 2);
    if manage {
        tabs.push((RequestsTab::Attention, "Needs attention"));
    }
    tabs.push((RequestsTab::All, "All"));
    tabs.extend(STATUS_TABS.iter().map(|&(status, label)| (RequestsTab::Status(status), label)));
    tabs
}

/// What an empty list says, its title and the line under it (Android's `TvRequestsScreen.kt`
/// `emptyTitle` and `emptyBody`): `filtered`, a filter narrows it (a status's tab counts as one);
/// `may_ask`, the profile may ask for titles.
pub fn requests_empty(tab: RequestsTab, filtered: bool, may_ask: bool) -> (&'static str, &'static str) {
    if filtered || matches!(tab, RequestsTab::Status(_)) {
        return ("No requests match", "Try other filters.");
    }
    if tab == RequestsTab::Attention {
        return ("Nothing needs you", "Requests waiting for a decision, and those that couldn't be sent, show here.");
    }
    let body = if may_ask {
        "Find movies and shows in Discover, and ask for what the libraries don't have."
    } else {
        "Requests you made or follow show here."
    };
    ("Nothing asked for yet", body)
}

/// The filters of the managers' list: movies or shows, one person's, a title's words.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestFilters {
    pub kind: Option<TitleKind>,
    pub user_id: Option<i64>,
    pub q: String,
}

/// The listing for a tab (Needs attention: the oldest first) and the filters, the first `limit`.
pub fn requests_query(tab: RequestsTab, f: &RequestFilters, limit: i64) -> RequestQuery {
    let mut q = RequestQuery::default();
    match tab {
        RequestsTab::Attention => {
            q.attention = true;
            q.oldest = true;
        }
        RequestsTab::Status(s) => q.status = vec![s],
        RequestsTab::All => {}
    }
    q.kind = f.kind;
    q.user_id = f.user_id;
    let words = f.q.trim();
    if !words.is_empty() {
        q.q = Some(words.to_string());
    }
    q.offset = Some(0);
    q.limit = Some(limit);
    q
}

/// Whether a request, as an action left it, still belongs on a tab: what was approved leaves Needs
/// attention and Pending at once.
pub fn in_requests_tab(r: &MediaRequest, tab: RequestsTab) -> bool {
    match tab {
        RequestsTab::All => true,
        RequestsTab::Attention => {
            r.status == RequestStatus::Pending || (r.status == RequestStatus::Failed && r.retry_at.is_none())
        }
        RequestsTab::Status(s) => r.status == s,
    }
}

/// A tab's number: its status's count (every filter but the status applied), their sum for All,
/// what waits for managers for Needs attention.
pub fn tab_count(tab: RequestsTab, counts: Option<&RequestCounts>, s: Option<&RequestSummary>) -> Option<i64> {
    let of = |c: &RequestCounts, status: RequestStatus| match status {
        RequestStatus::Pending => c.pending,
        RequestStatus::Approved => c.approved,
        RequestStatus::Declined => c.declined,
        RequestStatus::Failed => c.failed,
        RequestStatus::Available => c.available,
        RequestStatus::Unknown => 0,
    };
    match tab {
        RequestsTab::Attention => s.map(|s| s.pending + s.failed),
        RequestsTab::All => counts.map(|c| STATUS_TABS.iter().map(|&(status, _)| of(c, status)).sum()),
        RequestsTab::Status(status) => counts.map(|c| of(c, status)),
    }
}

// ---- who sees what

/// Discover shows, and asking works: requests are on and this profile may ask. A sign-in from
/// before requests still sees Discover; its title pages offer to sign in again.
pub fn may_ask(s: Option<&RequestSummary>) -> bool {
    s.is_some_and(|s| s.enabled && s.access != RequestAccess::Off)
}

/// Requests show at all: on, and this sign-in can reach them; an account that may not ask still
/// sees what it asked for before.
pub fn has_requests(s: Option<&RequestSummary>) -> bool {
    s.is_some_and(|s| {
        s.enabled
            && s.sign_in_again != Some(SignInAgain::Requests)
            && (s.access != RequestAccess::Off || s.own > 0 || s.manage)
    })
}

/// "Requests" for managers (it opens on Needs attention), "Your requests" for everyone else.
pub fn requests_title(s: Option<&RequestSummary>) -> &'static str {
    if s.is_some_and(|s| s.manage) { "Requests" } else { "Your requests" }
}

/// The one number on the way to requests, and whether it takes the danger colour: for managers
/// what waits for them and their own news, in the danger colour while some failed; for everyone
/// else their news.
pub fn requests_count(s: Option<&RequestSummary>) -> (i64, bool) {
    match s {
        Some(s) if s.enabled && s.manage => (s.pending + s.failed + s.unseen, s.failed > 0),
        Some(s) if s.enabled => (s.unseen, false),
        _ => (0, false),
    }
}

// ---- titles

/// What a title is known by across lists: "movie:603". TMDB numbers movies and shows apart.
pub fn title_key(kind: TitleKind, tmdb_id: i64) -> String {
    format!("{kind}:{tmdb_id}")
}

/// `more` after `have`, each kind and TMDB id once (a title can come again on a later page).
pub fn add_titles(have: &mut Vec<DiscoverTitle>, more: impl IntoIterator<Item = DiscoverTitle>) {
    let mut seen: std::collections::HashSet<(TitleKind, i64)> = have.iter().map(|t| (t.kind, t.tmdb_id)).collect();
    for t in more {
        if seen.insert((t.kind, t.tmdb_id)) {
            have.push(t);
        }
    }
}

/// The most characters TMDB's search takes, as the server has it.
pub const SEARCH_MAX: usize = 200;

/// Words as TMDB's search takes them: trimmed, and cut to [`SEARCH_MAX`] characters (never halfway
/// through one), with what the cut leaves at its end trimmed too.
pub fn search_text(q: &str) -> String {
    let cut: String = trim(q).chars().take(SEARCH_MAX).collect();
    trim(&cut).to_string()
}

/// Trimmed as JavaScript trims: white space and the byte order mark.
fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// A title's badge: "In library", "Partly in library" (where it can be opened), "Requested",
/// "Declined"; none otherwise.
pub fn title_badge(status: TitleStatus, item_id: Option<i64>) -> Option<&'static str> {
    match status {
        TitleStatus::Library => Some("In library"),
        TitleStatus::Partial if item_id.is_some() => Some("Partly in library"),
        TitleStatus::Partial | TitleStatus::Requested => Some("Requested"),
        TitleStatus::Declined => Some("Declined"),
        TitleStatus::Restricted | TitleStatus::None => None,
    }
}

/// A badge's or a stage pill's colour: waiting for a decision amber, on its way blue, ready green,
/// refused or failed red, gone gray.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    Amber,
    Blue,
    Green,
    Red,
    Gray,
}

/// A badge's colour: what can be watched green, what is on its way blue, what was refused red.
pub fn title_tone(status: TitleStatus, item_id: Option<i64>) -> Tone {
    match status {
        TitleStatus::Library => Tone::Green,
        TitleStatus::Partial if item_id.is_some() => Tone::Green,
        TitleStatus::Declined => Tone::Red,
        _ => Tone::Blue,
    }
}

/// A tile's second line: the year, and in rows of both kinds which it is.
pub fn title_subtitle(kind: TitleKind, year: Option<i64>, mixed: bool) -> String {
    let year = year.filter(|y| *y != 0).map(|y| y.to_string());
    let which = mixed.then_some(if kind == TitleKind::Show { "Show" } else { "Movie" });
    join([year.as_deref(), which], " · ")
}

/// A title page's facts line: the year, then the runtime or the seasons; the certification is
/// shown apart.
pub fn title_facts(d: &DiscoverDetail) -> String {
    let seasons = d.seasons.as_deref().unwrap_or_default().iter().filter(|s| s.number > 0).count() as i64;
    let length = if d.title.kind == TitleKind::Movie {
        format_runtime(d.runtime_minutes.filter(|m| *m != 0).map(|m| m * 60_000))
    } else if seasons > 0 {
        pluralize(seasons, "season", None)
    } else {
        String::new()
    };
    let year = d.title.year.filter(|y| *y != 0).map(|y| y.to_string());
    join([year.as_deref(), Some(length.as_str())], " · ")
}

/// The parts that aren't empty, joined.
fn join<'a>(parts: impl IntoIterator<Item = Option<&'a str>>, by: &str) -> String {
    parts.into_iter().flatten().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(by)
}

/// What a title's badge becomes from the news of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestBadge {
    /// The title is now this.
    Status(TitleStatus),
    /// Only the title's details can say: ask for them again.
    Ask,
    /// The news says nothing of the title as this profile sees it.
    Unchanged,
}

/// What a title's badge becomes from the news of a request this profile made or follows: active →
/// Requested, declined → Declined, a movie available → In library, some of a show's seasons ready
/// → Partly in library, gone → none. [`RequestBadge::Ask`] where only the title's details can say:
/// a show whose request is wholly ready (it may lack seasons nobody asked for, or have them all),
/// and a request this profile no longer sees (`None`: withdrawn, deleted or no longer followed,
/// while others may have asked). [`RequestBadge::Unchanged`] for anyone else's request (a manager
/// hears of everyone's).
pub fn status_from_request(r: Option<&MediaRequest>) -> RequestBadge {
    let Some(r) = r else { return RequestBadge::Ask };
    if !r.mine && !r.watching {
        return RequestBadge::Unchanged;
    }
    let status = if r.stage == RequestStage::Gone {
        TitleStatus::None
    } else if r.status == RequestStatus::Declined {
        TitleStatus::Declined
    } else if r.status == RequestStatus::Available {
        if r.kind == TitleKind::Show {
            return RequestBadge::Ask;
        }
        TitleStatus::Library
    } else if r.stage == RequestStage::Partial {
        TitleStatus::Partial
    } else {
        TitleStatus::Requested
    };
    RequestBadge::Status(status)
}

/// Pending, approved or failed: it still covers what it asks for.
pub fn is_active(r: &MediaRequest) -> bool {
    matches!(r.status, RequestStatus::Pending | RequestStatus::Approved | RequestStatus::Failed)
}

// ---- a title page's main action

/// What a title page offers first.
#[derive(Clone, Debug, PartialEq)]
pub enum MainAction<'a> {
    /// The app signed in before it could ask: the page says so, and offers to sign in again.
    SignIn,
    /// No button: the restriction is said instead.
    Restricted { text: String },
    /// [Open] what the library has.
    Open { item_id: Option<i64> },
    /// No button: "Declined: “…”. You can ask again in 12 days."
    Declined { text: String },
    /// [Requested], which does nothing, and its stage.
    Requested { request: Option<&'a MediaRequest> },
    /// [Following], which does nothing, its stage, and [Stop following].
    Following { request: Option<&'a MediaRequest> },
    /// Someone asked for it already: hearing when it's ready costs nothing (a show's: the seasons
    /// asked for).
    TellMeToo { seasons: Option<Vec<i64>> },
    /// Partly here: [Open], and [Request more seasons] where some are left to ask for.
    More { item_id: Option<i64>, more: bool, ask: bool, disabled: bool },
    /// [Request], disabled when nothing is left to ask for.
    Request { disabled: bool },
    /// No Request: "Requests aren't open to this account."
    NoAccess,
}

const AGE_LIMIT: &str = "Above this account's age limit";

/// A title page's one main action, the first that applies: sign in again, restricted, in the
/// library, declined, requested (by this profile, or followed, or by someone else), partly in the
/// library, and Request, disabled when nothing is left to ask for.
pub fn main_action<'a>(d: &'a DiscoverDetail, s: Option<&RequestSummary>, time: TvTime) -> MainAction<'a> {
    if s.is_some_and(|s| s.sign_in_again == Some(SignInAgain::Requests)) {
        return MainAction::SignIn;
    }
    let t = &d.title;
    match t.status {
        TitleStatus::Restricted => {
            let text = d.restriction.as_deref().filter(|r| !r.is_empty()).unwrap_or(AGE_LIMIT);
            return MainAction::Restricted { text: text.to_string() };
        }
        TitleStatus::Library => return MainAction::Open { item_id: t.item_id },
        TitleStatus::Declined => return MainAction::Declined { text: declined_text(d.declined.as_ref(), time) },
        _ => {}
    }
    let ask = s.is_none_or(|s| s.access != RequestAccess::Off);
    let active: Vec<&MediaRequest> = d.requests.iter().filter(|r| is_active(r)).collect();
    if t.status == TitleStatus::Requested {
        let mine: Vec<_> = active.iter().copied().filter(|r| r.mine).collect();
        if !mine.is_empty() {
            return MainAction::Requested { request: pick(&mine, t.request_id) };
        }
        if d.watching {
            let watching: Vec<_> = active.iter().copied().filter(|r| r.watching).collect();
            return MainAction::Following {
                request: pick(&watching, t.request_id).or_else(|| pick(&active, t.request_id)),
            };
        }
        if !ask {
            return MainAction::NoAccess;
        }
        return MainAction::TellMeToo { seasons: (t.kind == TitleKind::Show).then(|| requested_seasons(d)) };
    }
    let left = if t.kind == TitleKind::Movie { d.movies_left } else { d.seasons_left };
    let none = left.is_some_and(|l| l <= 0);
    if t.status == TitleStatus::Partial {
        return MainAction::More { item_id: t.item_id, more: has_more_seasons(d), ask, disabled: none };
    }
    if ask { MainAction::Request { disabled: none } } else { MainAction::NoAccess }
}

/// The main action from a list's title alone, while its details load or when they can't be had:
/// never "Tell me too" for what this profile may have asked for itself.
pub fn list_action(t: &DiscoverTitle, s: Option<&RequestSummary>) -> MainAction<'static> {
    if s.is_some_and(|s| s.sign_in_again == Some(SignInAgain::Requests)) {
        return MainAction::SignIn;
    }
    match t.status {
        TitleStatus::Restricted => return MainAction::Restricted { text: AGE_LIMIT.to_string() },
        TitleStatus::Library => return MainAction::Open { item_id: t.item_id },
        TitleStatus::Declined => return MainAction::Declined { text: "Declined.".to_string() },
        TitleStatus::Requested => return MainAction::Requested { request: None },
        _ => {}
    }
    let ask = s.is_none_or(|s| s.access != RequestAccess::Off);
    let left = s.and_then(|s| if t.kind == TitleKind::Movie { s.movies_left } else { s.seasons_left });
    let none = left.is_some_and(|l| l <= 0);
    if t.status == TitleStatus::Partial {
        return MainAction::More { item_id: t.item_id, more: true, ask, disabled: none };
    }
    if ask { MainAction::Request { disabled: none } } else { MainAction::NoAccess }
}

/// The request with `id` in `list`, else the list's first.
fn pick<'a>(list: &[&'a MediaRequest], id: Option<i64>) -> Option<&'a MediaRequest> {
    id.and_then(|id| list.iter().find(|r| r.id == id)).or(list.first()).copied()
}

/// "Declined: “Not on any service”. You can ask again in 12 days.", without the reason when
/// there's none.
pub fn declined_text(declined: Option<&Declined>, time: TvTime) -> String {
    let reason = declined
        .and_then(|d| d.reason.as_deref())
        .map(|r| trim(r).trim_end_matches(|c: char| c == '.' || c.is_whitespace()))
        .unwrap_or_default();
    let lead = if reason.is_empty() { "Declined.".to_string() } else { format!("Declined: “{reason}”.") };
    match declined {
        Some(d) => format!("{lead} You can ask again {}.", time_until(d.again_at, time)),
        None => lead,
    }
}

/// The request a title page's stage pill follows: this profile's own, else one it follows, else
/// the newest active one.
pub fn title_request(d: &DiscoverDetail) -> Option<&MediaRequest> {
    let active: Vec<&MediaRequest> = d.requests.iter().filter(|r| is_active(r)).collect();
    let mine: Vec<_> = active.iter().copied().filter(|r| r.mine).collect();
    let watching: Vec<_> = active.iter().copied().filter(|r| r.watching).collect();
    let id = d.title.request_id;
    pick(&mine, id).or_else(|| pick(&watching, id)).or_else(|| pick(&active, id))
}

// ---- seasons

/// "Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials"; empty for none.
pub fn seasons_label(seasons: &[i64]) -> String {
    let mut list = seasons.to_vec();
    list.sort_unstable();
    list.dedup();
    let specials = list.contains(&0);
    let numbered: Vec<i64> = list.into_iter().filter(|n| *n > 0).collect();
    let Some(&first) = numbered.first() else {
        return if specials { "Specials".into() } else { String::new() };
    };
    let mut runs: Vec<String> = Vec::new();
    let (mut start, mut prev) = (first, first);
    for &n in numbered[1..].iter().chain(std::iter::once(&-1)) {
        if n == prev + 1 {
            prev = n;
            continue;
        }
        runs.push(if start == prev { start.to_string() } else { format!("{start}–{prev}") });
        start = n;
        prev = n;
    }
    let word = if numbered.len() == 1 { "Season" } else { "Seasons" };
    format!("{word} {}{}", runs.join(", "), if specials { " and specials" } else { "" })
}

/// The seasons asked for already: Tell me too follows those.
pub fn requested_seasons(d: &DiscoverDetail) -> Vec<i64> {
    seasons_of(d).iter().filter(|s| s.state == SeasonState::Requested).map(|s| s.number).collect()
}

fn seasons_of(d: &DiscoverDetail) -> &[DiscoverSeason] {
    d.seasons.as_deref().unwrap_or_default()
}

/// A season that may be ticked: missing, partly had or not aired yet, not declined, and its title
/// (of `status`) not restricted.
pub fn askable(s: &DiscoverSeason, status: TitleStatus) -> bool {
    status != TitleStatus::Restricted
        && !s.declined
        && matches!(s.state, SeasonState::Missing | SeasonState::Partial | SeasonState::Unaired)
}

/// An askable numbered season that is missing or partly had: one the picker ticks, and one that
/// makes "Request more seasons" show.
fn wanted(s: &DiscoverSeason, status: TitleStatus) -> bool {
    s.number > 0 && askable(s, status) && matches!(s.state, SeasonState::Missing | SeasonState::Partial)
}

/// "Request more seasons" shows: a numbered season is askable, and missing or partly had.
pub fn has_more_seasons(d: &DiscoverDetail) -> bool {
    seasons_of(d).iter().any(|s| wanted(s, d.title.status))
}

/// The picker lists every season: the numbered ones in order, specials last.
pub fn picker_seasons(d: &DiscoverDetail) -> Vec<&DiscoverSeason> {
    let all = seasons_of(d);
    let mut out: Vec<&DiscoverSeason> = all.iter().filter(|s| s.number > 0).collect();
    out.sort_by_key(|s| s.number);
    out.extend(all.iter().filter(|s| s.number <= 0));
    out
}

/// What the picker ticks as it opens: the askable missing and partly had seasons other than
/// specials, lowest first, as many as may be asked for (`left`, `None`: no limit); seasons not
/// aired yet and specials are offered unticked.
pub fn preselected(d: &DiscoverDetail, left: Option<i64>) -> Vec<i64> {
    let out = picker_seasons(d).into_iter().filter(|s| wanted(s, d.title.status)).map(|s| s.number);
    match left {
        None => out.collect(),
        Some(left) => out.take(left.max(0) as usize).collect(),
    }
}

/// The picker's button: "Request 3 seasons", "Request 1 season"; "Pick a season." while none is
/// ticked.
pub fn picker_button(n: i64) -> String {
    match n {
        ..=0 => "Pick a season.".into(),
        1 => "Request 1 season".into(),
        n => format!("Request {n} seasons"),
    }
}

/// A season's line under its name: its state ("In library", "4 of 10 episodes in library",
/// "Requested", "Declined"…; Android's `RequestTexts.seasonState`), else what it holds.
pub fn season_line(s: &DiscoverSeason, time: TvTime) -> String {
    match s.state {
        SeasonState::Library => return "In library".into(),
        SeasonState::Requested => return "Requested".into(),
        _ => {}
    }
    if s.declined {
        return "Declined".into();
    }
    match s.state {
        SeasonState::Partial => format!("{} of {} episodes in library", s.in_library, s.episodes.max(s.in_library)),
        SeasonState::Unaired => match s.air_date.as_deref().filter(|d| !d.is_empty()) {
            Some(date) => format!("Airs {}", iso_day(date, time)),
            None => "Not aired yet".into(),
        },
        _ => pluralize(s.episodes, "episode", None),
    }
}

/// "2026-11-04" → "Nov 4" (with the year when it isn't this one's); what isn't such a date comes
/// back as it is.
fn iso_day(iso: &str, time: TvTime) -> String {
    let b = iso.as_bytes();
    let digits = |r: std::ops::Range<usize>| -> Option<i64> {
        let part = iso.get(r)?;
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse().ok())?
    };
    let parsed = (b.len() >= 10 && b[4] == b'-' && b[7] == b'-')
        .then(|| Some((digits(0..4)?, digits(5..7)?, digits(8..10)?)))
        .flatten()
        .filter(|&(_, m, d)| (1..=12).contains(&m) && (1..=31).contains(&d));
    match parsed {
        Some((y, m, d)) => date_words(y, m as u32, d as u32, y != civil(time.day(time.now)).0),
        None => iso.to_string(),
    }
}

// ---- what may still be asked for

/// What this profile may still ask for, and the limits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Allowance {
    pub movies_left: Option<i64>,
    pub seasons_left: Option<i64>,
    pub movies_next_at: Option<i64>,
    pub seasons_next_at: Option<i64>,
    pub movies_limit: Option<i64>,
    pub seasons_limit: Option<i64>,
    /// What is being asked for: its lines alone (a movie's page says nothing of seasons).
    pub kind: Option<TitleKind>,
}

impl Allowance {
    /// The details' own counts when there are details (no second call), else the summary's; the
    /// summary's limits.
    pub fn of(d: Option<&DiscoverDetail>, s: Option<&RequestSummary>, kind: Option<TitleKind>) -> Allowance {
        let (movies_left, seasons_left, movies_next_at, seasons_next_at) = match (d, s) {
            (Some(d), _) => (d.movies_left, d.seasons_left, d.movies_next_at, d.seasons_next_at),
            (None, Some(s)) => (s.movies_left, s.seasons_left, s.movies_next_at, s.seasons_next_at),
            (None, None) => (None, None, None, None),
        };
        Allowance {
            movies_left,
            seasons_left,
            movies_next_at,
            seasons_next_at,
            movies_limit: s.and_then(|s| s.movies_limit),
            seasons_limit: s.and_then(|s| s.seasons_limit),
            kind,
        }
    }

    /// The allowance of one kind: the other's counts and limit left out.
    fn only(mut self, kind: TitleKind) -> Allowance {
        if kind == TitleKind::Movie {
            (self.seasons_left, self.seasons_next_at, self.seasons_limit) = (None, None, None);
        } else {
            (self.movies_left, self.movies_next_at, self.movies_limit) = (None, None, None);
        }
        self
    }
}

/// What this profile may still ask for, in a line or two, the limits last ("per 7 days": they're a
/// rolling week, never "this week"); `None` without limits.
pub fn allowance_lines(given: &Allowance, time: TvTime) -> Option<Vec<String>> {
    let a = match given.kind {
        Some(kind) => given.only(kind),
        None => *given,
    };
    let (m, s) = (a.movies_left, a.seasons_left);
    if m.is_none() && s.is_none() {
        return None;
    }
    let m_out = m.is_some_and(|m| m <= 0);
    let s_out = s.is_some_and(|s| s <= 0);
    let when = |at: Option<i64>| at.map(|at| time_until(at, time));
    let mut lines = Vec::new();
    if m_out && s_out {
        lines.push(match (when(a.movies_next_at), when(a.seasons_next_at)) {
            (Some(mw), Some(sw)) => {
                format!("You've asked for all you can for now: more movies {mw}, more seasons {sw}.")
            }
            _ => "You've asked for all you can for now.".into(),
        });
    } else if m_out || s_out {
        let what = if m_out { "movies" } else { "seasons" };
        lines.push(match when(if m_out { a.movies_next_at } else { a.seasons_next_at }) {
            Some(w) => format!("You've asked for all the {what} you can for now: you can ask for more {w}."),
            None => format!("You've asked for all the {what} you can for now."),
        });
        if let Some(other) = if m_out { s } else { m } {
            let (one, many) = if m_out { ("more season", "more seasons") } else { ("more movie", "more movies") };
            lines.push(format!("You can ask for {}.", pluralize(other, one, Some(many))));
        }
    } else {
        let parts = [
            m.map(|m| pluralize(m, "more movie", Some("more movies"))),
            s.map(|s| pluralize(s, "more season", Some("more seasons"))),
        ];
        lines.push(format!("You can ask for {}.", parts.into_iter().flatten().collect::<Vec<_>>().join(" and ")));
    }
    let limits: Vec<String> = [
        a.movies_limit.filter(|l| *l != 0).map(|l| pluralize(l, "movie", None)),
        a.seasons_limit.filter(|l| *l != 0).map(|l| pluralize(l, "season", None)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !limits.is_empty() {
        lines.push(format!("Limits: {} per 7 days.", limits.join(" and ")));
    }
    Some(lines)
}

// ---- a request's stage

/// Where a request has got to, in words and a colour.
#[derive(Clone, Debug, PartialEq)]
pub struct StageWords {
    pub text: String,
    pub tone: Tone,
    /// The download bar, 0–1, while something downloads.
    pub progress: Option<f64>,
    /// A line under it: why it couldn't be sent (managers), "Trying again in 5 min.".
    pub note: Option<String>,
}

impl StageWords {
    fn new(text: impl Into<String>, tone: Tone) -> StageWords {
        StageWords { text: text.into(), tone, progress: None, note: None }
    }

    fn note(mut self, note: Option<String>) -> StageWords {
        self.note = note;
        self
    }
}

/// Where a request has got to, in words and a colour (managers read why one failed).
pub fn stage_words(r: &MediaRequest, manage: bool, time: TvTime) -> StageWords {
    stage_words_at(r, r.stage, manage, time)
}

fn stage_words_at(r: &MediaRequest, stage: RequestStage, manage: bool, time: TvTime) -> StageWords {
    match stage {
        RequestStage::Pending => StageWords::new("Waiting for approval", Tone::Amber),
        RequestStage::Declined => StageWords::new("Declined", Tone::Red)
            .note(r.reason.as_deref().filter(|r| !r.is_empty()).map(|r| format!("“{}”", trim(r)))),
        RequestStage::Failed => {
            let retry = r.retry_at.map(|at| format!("Trying again {}.", time_until(at, time)));
            if manage {
                // "Couldn't send: {reason}" in two parts, so a long reason is read whole under a
                // short pill.
                let reason = r.reason.as_deref().filter(|r| !r.is_empty()).map(sentence);
                let note = join([reason.as_deref(), retry.as_deref()], " ");
                return StageWords::new("Couldn't send", Tone::Red).note((!note.is_empty()).then_some(note));
            }
            let note = retry
                .unwrap_or_else(|| "It couldn't be sent to download yet: an administrator will look at it.".into());
            StageWords::new("Not sent yet", Tone::Red).note(Some(note))
        }
        RequestStage::Approved => StageWords::new("Approved", Tone::Blue),
        RequestStage::Waiting => match r.stage_at {
            None => StageWords::new("Not out yet", Tone::Blue),
            // A release is known by its day only (sent as 12:00 UTC of it); an episode's airing
            // exactly.
            Some(at) if r.kind == TitleKind::Movie => {
                StageWords::new(format!("Out {}", day_and_time(utc_day(at, time), false, time)), Tone::Blue)
            }
            Some(at) => StageWords::new(format!("Airs {}", day_and_time(at, true, time)), Tone::Blue),
        },
        RequestStage::Searching => StageWords::new("Searching", Tone::Blue),
        RequestStage::Downloading => download_words(r, time),
        RequestStage::Importing => StageWords::new("Adding to the library", Tone::Blue),
        RequestStage::Partial => {
            let label = seasons_label(&r.available_seasons);
            let label = if label.is_empty() { "Some seasons".into() } else { label };
            StageWords {
                progress: r.download.as_ref().map(|d| d.progress),
                ..StageWords::new(format!("{label} ready"), Tone::Green)
            }
        }
        RequestStage::Available => StageWords::new("Ready to watch", Tone::Green)
            .note(r.item_id.is_none().then(|| "Ready, in a library you can't open: ask an administrator".into())),
        RequestStage::Gone => StageWords::new("No longer in a library", Tone::Gray),
        // A stage this app doesn't know (or a server from before stages): its status says as much.
        RequestStage::Unknown => match status_stage(r.status) {
            Some(stage) => stage_words_at(r, stage, manage, time),
            None => StageWords::new("Approved", Tone::Blue),
        },
    }
}

/// The stage a status stands for, when a request's own stage isn't known.
fn status_stage(status: RequestStatus) -> Option<RequestStage> {
    match status {
        RequestStatus::Pending => Some(RequestStage::Pending),
        RequestStatus::Approved => Some(RequestStage::Approved),
        RequestStatus::Declined => Some(RequestStage::Declined),
        RequestStatus::Failed => Some(RequestStage::Failed),
        RequestStatus::Available => Some(RequestStage::Available),
        RequestStatus::Unknown => None,
    }
}

/// The release day sent as 12:00 UTC of it, as noon of that day on the TV's clock, so every time
/// zone shows the same day.
fn utc_day(seconds: i64, time: TvTime) -> i64 {
    seconds.div_euclid(86_400) * 86_400 + 12 * 3600 - time.offset
}

/// A download's words: its state, else its share and when it ends ("Downloading 42% · in 20 min").
fn download_words(r: &MediaRequest, time: TvTime) -> StageWords {
    let Some(d) = &r.download else { return StageWords::new("Downloading", Tone::Blue) };
    let text = match d.state {
        DownloadState::Queued => "Waiting to download".into(),
        DownloadState::Paused => "Download paused".into(),
        DownloadState::Warning => "The download has a problem".into(),
        DownloadState::Importing => "Adding to the library".into(),
        DownloadState::Downloading => {
            let eta = match d.eta {
                Some(eta) if eta > time.now + 30 => format!(" · {}", time_until(eta, time)),
                _ => String::new(),
            };
            let share = (d.progress.clamp(0.0, 1.0) * 100.0).floor() as i64;
            format!("Downloading {share}%{eta}")
        }
    };
    StageWords { progress: Some(d.progress), ..StageWords::new(text, Tone::Blue) }
}

/// A season's own stage from the request asking for it: of a show's request partly ready, a season
/// is ready, or downloads while the request does; `None` for one still to come with nothing
/// downloading (its own state says the rest). Other stages are the request's.
pub fn season_stage(r: &MediaRequest, season: i64, manage: bool, time: TvTime) -> Option<StageWords> {
    if r.stage != RequestStage::Partial {
        return Some(stage_words(r, manage, time));
    }
    if r.available_seasons.contains(&season) {
        return Some(StageWords::new("Ready to watch", Tone::Green));
    }
    r.download.is_some().then(|| download_words(r, time))
}

/// Something on the list downloads or goes into the library: the list is asked for again every
/// 10 s.
pub fn moving(list: &[MediaRequest]) -> bool {
    list.iter().any(|r| {
        matches!(r.stage, RequestStage::Downloading | RequestStage::Importing)
            || (r.stage == RequestStage::Partial && r.download.is_some())
    })
}

/// Who asked and when: "Asked by you · 3 days ago", "Asked by Ann · yesterday", "Asked for by
/// someone else · now"; for managers, how many follow it.
pub fn asked_line(r: &MediaRequest, manage: bool, time: TvTime) -> String {
    let who = if r.mine {
        "Asked by you".to_string()
    } else if let Some(by) = &r.requested_by {
        format!("Asked by {}", by.name)
    } else {
        "Asked for by someone else".to_string()
    };
    let mut parts = Vec::with_capacity(4);
    if r.watching && !r.mine {
        parts.push("You follow it".to_string());
    }
    parts.push(who);
    parts.push(relative_time(r.created_at, time));
    if manage && r.watchers > 0 {
        parts.push(format!("{} following", r.watchers));
    }
    parts.join(" · ")
}

/// What the server tells managers of a request ("In Movies 4K, which Ann can't open."), a sentence
/// as the stage's line is.
pub fn request_note(r: &MediaRequest) -> Option<String> {
    r.note.as_deref().filter(|n| !trim(n).is_empty()).map(sentence)
}

// ---- a request's actions

/// One of a request's actions as a row offers it: the server's, or Decline with a reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionId {
    Action(RequestAction),
    /// Decline, with a reason typed on the TV's keyboard.
    DeclineReason,
}

/// What is asked before an action that can't be undone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub title: String,
    pub body: Option<&'static str>,
    pub button: &'static str,
}

/// A row of a request's actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionRow {
    pub id: ActionId,
    pub label: &'static str,
    pub danger: bool,
    /// Asked first: what can't be undone.
    pub confirm: Option<Confirm>,
}

impl ActionRow {
    fn new(id: ActionId, label: &'static str) -> ActionRow {
        ActionRow { id, label, danger: false, confirm: None }
    }
}

/// The actions the server lists for a request, in its order, as rows: TVs offer Decline and
/// "Decline with a reason…" (the TV's keyboard), and no approval dialog. An action a newer server
/// offers is left out: the web has it.
pub fn action_rows(r: &MediaRequest) -> Vec<ActionRow> {
    let mut out = Vec::with_capacity(r.actions.len() + 1);
    for &a in &r.actions {
        let id = ActionId::Action(a);
        match a {
            RequestAction::Withdraw if r.status == RequestStatus::Declined => out.push(ActionRow {
                confirm: Some(Confirm {
                    title: format!("Remove “{}” from your requests?", r.title),
                    body: None,
                    button: "Remove",
                }),
                ..ActionRow::new(id, "Remove")
            }),
            RequestAction::Withdraw => out.push(ActionRow {
                confirm: Some(Confirm {
                    title: format!("Withdraw your request for “{}”?", r.title),
                    body: None,
                    button: "Withdraw",
                }),
                ..ActionRow::new(id, "Withdraw")
            }),
            RequestAction::Unwatch => out.push(ActionRow::new(id, "Stop following")),
            RequestAction::Approve => out.push(ActionRow::new(id, "Approve")),
            RequestAction::Retry => out.push(ActionRow::new(id, "Try again")),
            RequestAction::Decline => {
                out.push(ActionRow::new(id, "Decline"));
                out.push(ActionRow::new(ActionId::DeclineReason, "Decline with a reason…"));
            }
            RequestAction::Handled => out.push(ActionRow::new(id, "Mark as added by hand")),
            RequestAction::Delete => out.push(ActionRow {
                danger: true,
                confirm: Some(Confirm {
                    title: "Delete this request?".into(),
                    body: Some("Radarr and Sonarr keep what they have."),
                    button: "Delete",
                }),
                ..ActionRow::new(id, "Delete")
            }),
            RequestAction::AskAgain => out.push(ActionRow::new(id, "Request again")),
            RequestAction::Unknown => {}
        }
    }
    out
}

/// What a toast says once an action is done.
pub fn action_done(a: ActionId, r: &MediaRequest) -> String {
    let x = format!("“{}”", r.title);
    match a {
        ActionId::Action(RequestAction::Withdraw) if r.status == RequestStatus::Declined => {
            format!("Removed {x} from your requests.")
        }
        ActionId::Action(RequestAction::Withdraw) => format!("Withdrew your request for {x}."),
        ActionId::Action(RequestAction::Unwatch) => format!("You no longer follow {x}."),
        ActionId::Action(RequestAction::Approve) => format!("Approved {x}."),
        ActionId::Action(RequestAction::Retry) => format!("Trying {x} again."),
        ActionId::Action(RequestAction::Decline) | ActionId::DeclineReason => format!("Declined {x}."),
        ActionId::Action(RequestAction::Handled) => format!("Marked {x} as added by hand."),
        ActionId::Action(RequestAction::Delete) => format!("Deleted the request for {x}."),
        ActionId::Action(RequestAction::AskAgain | RequestAction::Unknown) => String::new(),
    }
}

// ---- asking, and news

/// What asking answered: a new request (`created`), by how it was decided, or the requests
/// followed.
pub fn asked_text(r: &MediaRequest, created: bool) -> String {
    let x = format!("“{}”", r.title);
    if !created {
        return format!("You'll hear when {x} is ready.");
    }
    match r.status {
        RequestStatus::Failed => format!("Asked for {x}, but it couldn't be sent to download yet."),
        RequestStatus::Pending => format!("Asked for {x}: an administrator decides."),
        _ => format!("{x} is approved and on its way."),
    }
}

/// "“X” is ready to watch.", "Season 2 of “X” is ready to watch.", "Seasons 1–2 of “X” are ready
/// to watch."
pub fn ready_text(title: &str, seasons: Option<&[i64]>, complete: bool) -> String {
    match seasons.filter(|s| !complete && !s.is_empty()) {
        None => format!("“{title}” is ready to watch."),
        Some(seasons) => {
            let several = seasons.len() > 1 || seasons[0] == 0;
            format!("{} of “{title}” {} ready to watch.", seasons_label(seasons), if several { "are" } else { "is" })
        }
    }
}

/// Several pieces of news at once.
pub fn news_text(n: usize) -> String {
    format!("{n} of your requests have news")
}

/// A request whose news is that it's ready to watch, whole or in part: the TVs pop up only these.
pub fn ready_news(r: &MediaRequest) -> bool {
    matches!(r.stage, RequestStage::Available | RequestStage::Partial)
}

/// What ready-to-watch news is remembered by, so none is announced twice: its request whole, or
/// each of a show's seasons ready so far, since they come a batch at a time.
pub fn news_keys(request_id: i64, complete: bool, seasons: Option<&[i64]>) -> Vec<String> {
    if complete {
        return vec![format!("{request_id}:a")];
    }
    match seasons.filter(|s| !s.is_empty()) {
        None => vec![format!("{request_id}:p")],
        Some(seasons) => seasons.iter().map(|n| format!("{request_id}:s{n}")).collect(),
    }
}

// ---- refusals

/// What the server's refusal of a request route means.
#[derive(Clone, Debug, PartialEq)]
pub enum RequestsProblem {
    /// Requests are off on this server.
    Off,
    /// TMDB refused the server's key.
    TmdbKey,
    /// TMDB isn't answering.
    TmdbDown,
    /// This account may not ask.
    NoAccess,
    /// The sign-in is from before requests: sign in again.
    SignIn,
    /// Above this account's age limit, in the server's words.
    Restricted { text: String },
    /// Declined lately; when it may be asked for again.
    Declined { retry_at: Option<i64> },
    /// Over the allowance; when more may be asked for.
    Limit { retry_at: Option<i64> },
    /// A race: the request changed meanwhile, in the server's words, with the request as it is now.
    Conflict { text: String, request: Option<Box<MediaRequest>> },
    /// TMDB doesn't know the title.
    Missing,
    /// Anything else, or no answer.
    Other(ApiError),
}

/// What the server's refusal of a request route means (its `code`, and the fields beside it).
pub fn requests_problem(e: &ApiError) -> RequestsProblem {
    let ApiError::Status { status, code, message, body } = e else { return RequestsProblem::Other(e.clone()) };
    if e.unreachable() {
        return RequestsProblem::Other(e.clone());
    }
    let field = |name: &str| body.as_ref().and_then(|b| b.get(name));
    let retry_at = field("retry_at").and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)));
    match (*status, code.as_str()) {
        (503, "tmdb_key") => RequestsProblem::TmdbKey,
        (503, "tmdb_down") => RequestsProblem::TmdbDown,
        // A server from before the reasons said TMDB's trouble with `setup: true`.
        (503, "unavailable") if field("setup") == Some(&serde_json::Value::Bool(true)) => RequestsProblem::TmdbDown,
        (503, "unavailable") => RequestsProblem::Off,
        (503, _) => RequestsProblem::Other(e.clone()),
        (403, "insufficient_scope") => RequestsProblem::SignIn,
        (403, "restricted") => RequestsProblem::Restricted { text: sentence(message) },
        (403, _) => RequestsProblem::NoAccess,
        (409, "declined") => RequestsProblem::Declined { retry_at },
        (429, _) => RequestsProblem::Limit { retry_at },
        (409, _) => {
            let request = field("request")
                .filter(|r| r.get("id").is_some_and(serde_json::Value::is_number))
                .and_then(|r| serde_json::from_value::<MediaRequest>(r.clone()).ok())
                .map(Box::new);
            RequestsProblem::Conflict { text: sentence(message), request }
        }
        (404, _) => RequestsProblem::Missing,
        _ => RequestsProblem::Other(e.clone()),
    }
}

/// Why a page can't show requests, and whether Retry may help.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProblemWords {
    pub title: &'static str,
    /// Empty where the title says it all.
    pub body: &'static str,
    pub retry: bool,
}

/// A page that can't show requests: why, and whether Retry may help; `None` for what isn't about
/// requests being usable.
pub fn problem_words(p: &RequestsProblem, manage: bool) -> Option<ProblemWords> {
    let words = |title, body, retry| Some(ProblemWords { title, body, retry });
    match p {
        RequestsProblem::Off => {
            words("Requests are off", "This server doesn't take requests for movies and shows.", false)
        }
        RequestsProblem::NoAccess => words(
            "Requests aren't open to this account",
            "An administrator can let it ask for movies and shows.",
            false,
        ),
        RequestsProblem::TmdbDown => words("TMDB isn't answering", "Try again in a moment.", true),
        RequestsProblem::TmdbKey => words(
            "TMDB refused this server's key",
            if manage { "Check it in Settings → Server, on the web." } else { "An administrator needs to check it." },
            false,
        ),
        RequestsProblem::SignIn => words(SIGN_IN_TITLE, "", false),
        _ => None,
    }
}

/// What a toast says when asking for a title of `kind` was refused: when one may ask again (from
/// `retry_at`, never the message's date), the limit (the summary's), the restriction.
pub fn ask_refusal(p: &RequestsProblem, kind: TitleKind, limits: Option<&RequestSummary>, time: TvTime) -> String {
    match p {
        RequestsProblem::Declined { retry_at: Some(at) } => format!("You can ask again {}.", time_until(*at, time)),
        RequestsProblem::Declined { retry_at: None } => "It was declined.".into(),
        RequestsProblem::Limit { retry_at: Some(at) } => {
            format!("You can ask for more {}.", time_until(*at, time))
        }
        RequestsProblem::Limit { retry_at: None } => {
            let limit = limits
                .and_then(|l| if kind == TitleKind::Movie { l.movies_limit } else { l.seasons_limit })
                .filter(|l| *l != 0);
            match limit {
                Some(limit) => {
                    let what = if kind == TitleKind::Movie { "movie" } else { "season" };
                    format!("You can ask for at most {} per 7 days: pick fewer.", pluralize(limit, what, None))
                }
                None => "That's more than you can ask for at once: pick fewer.".into(),
            }
        }
        RequestsProblem::Restricted { text } | RequestsProblem::Conflict { text, .. } => text.clone(),
        RequestsProblem::Missing => "TMDB doesn't know this title.".into(),
        RequestsProblem::Other(e) => sentence(&e.to_string()),
        _ => problem_words(p, false).map(|w| format!("{}.", w.title)).unwrap_or_default(),
    }
}

// ---- times in words

const SHORT_MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Days since 1970-01-01 as (year, month 1–12, day 1–31) (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// "Nov 4", or "Nov 4, 2027" with the year.
fn date_words(year: i64, month: u32, day: u32, with_year: bool) -> String {
    let month = SHORT_MONTHS[(month as usize).clamp(1, 12) - 1];
    if with_year { format!("{month} {day}, {year}") } else { format!("{month} {day}") }
}

/// Unix seconds as the TV's day: "Nov 4", with the year when it isn't now's (or with `year`).
pub fn short_date(seconds: i64, time: TvTime, year: bool) -> String {
    let (y, m, d) = civil(time.day(seconds));
    date_words(y, m, d, year || y != civil(time.day(time.now)).0)
}

/// How long ago: "now", "5 min ago", "3 hours ago", "yesterday", "3 days ago"; a month on, the
/// date.
pub fn relative_time(seconds: i64, time: TvTime) -> String {
    let diff = (time.now - seconds).max(0);
    match diff {
        0..60 => "now".into(),
        60..3600 => format!("{} min ago", diff / 60),
        3600..86_400 => match diff / 3600 {
            1 => "1 hour ago".into(),
            h => format!("{h} hours ago"),
        },
        _ => match diff / 86_400 {
            1 => "yesterday".into(),
            days @ ..30 => format!("{days} days ago"),
            _ => short_date(seconds, time, true),
        },
    }
}

/// How long until `seconds`, to follow a verb ("you can ask for more in 3 days"): "in 5 min" and
/// "in 2 hours" within a day, then by the TV's calendar, "tomorrow", "in 3 days"; "now" once it
/// has come.
pub fn time_until(seconds: i64, time: TvTime) -> String {
    let diff = seconds - time.now;
    if diff <= 30 {
        return "now".into();
    }
    if diff < 3600 {
        return format!("in {} min", ((diff as f64 / 60.0).round() as i64).max(1));
    }
    if diff < 86_400 {
        return match ((diff as f64 / 3600.0).round() as i64).max(1) {
            1 => "in 1 hour".into(),
            h => format!("in {h} hours"),
        };
    }
    match (time.day(seconds) - time.day(time.now)).max(1) {
        1 => "tomorrow".into(),
        days => format!("in {days} days"),
    }
}

/// When something comes out, after "Out" or "Airs": its day ("today", "tomorrow", "Nov 4"), and
/// with `with_time` its time as well within the next 24 hours ("today at 21:05").
pub fn day_and_time(seconds: i64, with_time: bool, time: TvTime) -> String {
    let day = match time.day(seconds) - time.day(time.now) {
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        _ => short_date(seconds, time, false),
    };
    let ahead = seconds - time.now;
    if with_time && ahead > 0 && ahead <= 86_400 {
        let of_day = (seconds + time.offset).rem_euclid(86_400);
        format!("{day} at {}", format_time_of_day((of_day / 3600) as u32, ((of_day % 3600) / 60) as u32, time.hours24))
    } else {
        day
    }
}
