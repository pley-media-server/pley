//! Your requests, or for managers Requests (`requests-page.tsx`): tabs by status, managers' opening
//! on Needs attention (pending, and failed ones the server won't try again), with filters for
//! movies or shows, a person and a title; cards with the poster, the seasons, who asked and when,
//! the reason or note, the stage pill and the download's bar; OK opens what may be done with one
//! in a side panel. The page tells the server its news was seen, and asks again when requests
//! change, every 10 s while something downloads, and every 30 s while the events are down.

use std::collections::{HashMap, HashSet};

use pley_base::{Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::model::{
    MediaRequest, NewRequest, RequestAction, RequestList, RequestUser, SignInAgain as Scope, TitleKind,
};
use pley_core::requests::{
    ActionId, ActionRow, RequestFilters, RequestsProblem, RequestsTab, SIGN_IN_APPROVALS, TvTime, action_done,
    action_rows, ask_refusal, asked_line, has_requests, in_requests_tab, may_ask, moving, problem_words, request_note,
    requests_empty, requests_problem, requests_query, requests_tabs, requests_title, seasons_label, stage_words,
    tab_count, title_key,
};
use pley_core::{ApiError, Promise};
use pley_kit::look::focus_progress;
use pley_kit::{
    Button, Heading, Keyboard, KeyboardState, List, ListRow, ListState, Message, MessageStyle, SidePanel, TextEdit,
    TextField, Trailing, bar_group, inert, placeholder, skeleton_block,
};
use pley_shell::{AppCx, AppEvent, Chrome, Route, Screen};
use pley_ui::theme::{self, focus, radius};
use pley_ui::{Dir, Edge, Group, Id, Ui};

use crate::common::{
    Asking, Poll, ProblemAction, SignInAgain, TvClock, answer_of, art, download_bar, failed_title, problem_page,
    stage_block, stage_pill, summary_problem,
};
use crate::widgets::{Counted, count_text};

/// Requests come 30 at a time, 30 more as focus nears the end, up to the 200 the server gives at once.
pub const REQUESTS_PAGE: i64 = 30;
const REQUESTS_MAX: i64 = 200;
/// The managers' title filter asks once typing pauses this long.
const FILTER_DEBOUNCE: f64 = 0.4;
/// The list starts under the tabs, or under the filters (or the line asking to sign in again).
const LIST_TOP: f32 = 128.0;
const LIST_TOP_LOW: f32 = 178.0;
const TABS_Y: f32 = 76.0;
const FILTERS_Y: f32 = 124.0;
/// A card, and the gap under it.
const CARD_H: f32 = 88.0;
const CARD_GAP: f32 = 10.0;
const CARD_W: f32 = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
/// The line after the newest 200, under the last card.
const MORE_LINE: f32 = 21.0;
const MORE_TEXT: &str = "The newest 200 show here: filter to find the others.";
/// Answers kept for the tabs and filters seen, so going back to one draws it at once.
const LISTS_KEPT: usize = 8;

/// An action's icon.
fn action_icon(a: ActionId) -> Icon {
    match a {
        ActionId::Action(RequestAction::Withdraw) => Icon::X,
        ActionId::Action(RequestAction::Unwatch) => Icon::BellOff,
        ActionId::Action(RequestAction::Approve) => Icon::Check,
        ActionId::Action(RequestAction::Retry) => Icon::RefreshCw,
        ActionId::Action(RequestAction::Decline) => Icon::Ban,
        ActionId::DeclineReason => Icon::Pencil,
        ActionId::Action(RequestAction::Handled) => Icon::CircleCheck,
        ActionId::Action(RequestAction::Delete) => Icon::Trash2,
        ActionId::Action(RequestAction::AskAgain | RequestAction::Unknown) => Icon::RotateCcw,
    }
}

/// "Movie", or a show's seasons ("Seasons 1–2").
pub fn request_what(r: &MediaRequest) -> String {
    match r.kind {
        TitleKind::Show => {
            let label = seasons_label(r.seasons.as_deref().unwrap_or_default());
            if label.is_empty() { "Show".into() } else { label }
        }
        _ => "Movie".into(),
    }
}

/// Movies and shows, then movies, then shows.
fn next_kind(kind: Option<TitleKind>) -> Option<TitleKind> {
    match kind {
        None => Some(TitleKind::Movie),
        Some(TitleKind::Movie) => Some(TitleKind::Show),
        Some(_) => None,
    }
}

fn kind_label(kind: Option<TitleKind>) -> &'static str {
    match kind {
        Some(TitleKind::Movie) => "Movies",
        Some(TitleKind::Show) => "Shows",
        _ => "Movies and shows",
    }
}

/// Where the panel is.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Actions,
    Confirm(ActionRow),
    Reason,
}

#[derive(Clone, Debug, PartialEq)]
enum Panel {
    Request { id: i64, step: Step },
    People,
}

/// An action on its way.
struct Acting {
    request: MediaRequest,
    action: ActionId,
    job: Promise<Result<Option<MediaRequest>, ApiError>>,
}

/// A listing on its way.
struct Loading {
    key: String,
    limit: i64,
    job: Promise<Result<RequestList, ApiError>>,
}

/// Your requests, or Requests.
pub struct RequestsScreen {
    tab: Option<RequestsTab>,
    filters: RequestFilters,
    person: Option<RequestUser>,
    text: TextEdit,
    typing: bool,
    typed_at: Option<f64>,
    keyboard: KeyboardState,
    limit: i64,
    /// The newest answer for each tab and filter, the newest last.
    lists: Vec<(String, RequestList)>,
    loading: Option<Loading>,
    error: Option<(String, ApiError)>,
    /// Asked again at the next frame (requests changed, a timer).
    stale: bool,
    /// What an action answered, until the list is asked again: the request as it is now, or
    /// `None` once it left the list.
    changed: HashMap<i64, Option<MediaRequest>>,
    /// News since Your requests was last looked at keeps its dot while the page shows.
    news: HashSet<i64>,
    told: bool,
    seen: Option<Promise<Result<(), ApiError>>>,
    poll: Poll,
    list: ListState,
    panel: Option<Panel>,
    /// The panel's row pressed last: focus comes back to it.
    last_row: Option<ActionId>,
    focus_panel: bool,
    reason: TextEdit,
    reason_typing: bool,
    reason_keyboard: KeyboardState,
    focus_decline: bool,
    acting: Vec<Acting>,
    asking: Option<(i64, Asking)>,
    sign_in: SignInAgain,
    clock: TvClock,
    /// The card focus is on: the tab's listing, its place and its request. Should the request
    /// leave the list (withdrawn, approved off Needs attention), the card now in its place takes
    /// focus, else the tab.
    list_focus: Option<(String, usize, i64)>,
    /// The first listing hasn't come yet: the first card takes focus when it does.
    first: bool,
    art: HashMap<i64, Vec<ImageRequest>>,
    settled: bool,
    railed: bool,
}

impl Default for RequestsScreen {
    fn default() -> RequestsScreen {
        RequestsScreen::new()
    }
}

impl RequestsScreen {
    pub fn new() -> RequestsScreen {
        RequestsScreen {
            tab: None,
            filters: RequestFilters::default(),
            person: None,
            text: TextEdit::new("").limit(200),
            typing: false,
            typed_at: None,
            keyboard: KeyboardState::default(),
            limit: REQUESTS_PAGE,
            lists: Vec::new(),
            loading: None,
            error: None,
            stale: false,
            changed: HashMap::new(),
            news: HashSet::new(),
            told: false,
            seen: None,
            poll: Poll::default(),
            list: ListState::default(),
            panel: None,
            last_row: None,
            focus_panel: false,
            reason: TextEdit::new("").limit(500),
            reason_typing: false,
            reason_keyboard: KeyboardState::default(),
            focus_decline: false,
            acting: Vec::new(),
            asking: None,
            sign_in: SignInAgain::default(),
            clock: TvClock::default(),
            list_focus: None,
            first: true,
            art: HashMap::new(),
            settled: false,
            railed: false,
        }
    }

    /// The focus id of tab `tab` on the page `page`.
    pub fn tab_id(page: Id, tab: RequestsTab) -> Id {
        page.with("tabs").with(format!("{tab:?}"))
    }

    /// The focus id of the card at `index` on the page `page`.
    pub fn card_id(page: Id, index: usize) -> Id {
        page.with("list").with(index as u64)
    }

    /// The focus id of the panel's row `key` (`open`, `details`, `cancel`, `confirm`, `decline`,
    /// `reason`, or an action's `{:?}`) on the page `page`.
    pub fn panel_row_id(page: Id, key: &str) -> Id {
        page.with("panel").with(key)
    }

    /// The tab shown: the one chosen here, else the first (Needs attention for managers).
    fn tab(&self, manage: bool) -> RequestsTab {
        let tabs = requests_tabs(manage);
        match self.tab {
            Some(t) if tabs.iter().any(|(x, _)| *x == t) => t,
            _ => tabs[0].0,
        }
    }

    fn filters(&self, manage: bool) -> RequestFilters {
        if manage { self.filters.clone() } else { RequestFilters::default() }
    }

    fn key(tab: RequestsTab, f: &RequestFilters) -> String {
        format!("{:?}", requests_query(tab, f, 0).params())
    }

    fn list_of(&self, key: &str) -> Option<&RequestList> {
        self.lists.iter().find(|(k, _)| k == key).map(|(_, l)| l)
    }

    /// Shows the first page of what the tab and filters keep.
    fn start_over(&mut self) {
        self.limit = REQUESTS_PAGE;
        self.list = ListState::default();
        self.changed.clear();
    }

    /// Asks for the listing when it isn't there, is stale, or wants more.
    fn load(&mut self, ui: &mut Ui, cx: &mut AppCx, tab: RequestsTab, f: &RequestFilters, key: &str) {
        if let Some(answer) = self.loading.as_mut().and_then(|l| answer_of(&mut l.job)) {
            let loading = self.loading.take().expect("an answer came");
            match answer {
                Ok(list) => {
                    if loading.key == key {
                        self.changed.clear();
                    }
                    for r in list.requests.iter().filter(|r| r.unseen) {
                        self.news.insert(r.id);
                    }
                    self.tell_seen(cx, &list);
                    self.lists.retain(|(k, _)| *k != loading.key);
                    self.lists.push((loading.key.clone(), list));
                    let over = self.lists.len().saturating_sub(LISTS_KEPT);
                    self.lists.drain(..over);
                    self.error = None;
                    cx.state.server_answered();
                }
                Err(e) if e.cancelled() => {}
                Err(e) => {
                    cx.state.report_failure(&e);
                    self.error = Some((loading.key, e));
                }
            }
        }
        let have = self.list_of(key).map(|l| l.requests.len() as i64);
        let wants_more = have.is_some_and(|n| n < self.limit)
            && self.list_of(key).is_some_and(|l| l.total > l.requests.len() as i64);
        let failed_here = self.error.as_ref().is_some_and(|(k, _)| k == key);
        let needed = (have.is_none() && !failed_here) || self.stale || wants_more;
        let on_its_way = self.loading.as_ref().is_some_and(|l| l.key == key && l.limit == self.limit);
        if needed && !on_its_way {
            self.stale = false;
            let query = requests_query(tab, f, self.limit);
            let job = cx.spawn(move |api| api.requests(&query));
            self.loading = Some(Loading { key: key.to_string(), limit: self.limit, job });
        }
        let shown: Vec<MediaRequest> = self.shown(key, tab);
        let every = if moving(&shown) {
            Some(10.0)
        } else if cx.state.requests_events_down() {
            Some(30.0)
        } else {
            None
        };
        if self.poll.due(ui, cx, every) {
            self.stale = true;
        }
    }

    /// The page tells the server its news was seen, once, when the first answer comes.
    fn tell_seen(&mut self, cx: &mut AppCx, list: &RequestList) {
        if std::mem::replace(&mut self.told, true) {
            return;
        }
        let unseen = cx.state.request_summary().is_some_and(|s| s.unseen > 0);
        if unseen || list.requests.iter().any(|r| r.unseen) {
            self.seen = Some(cx.spawn(|api| api.mark_requests_seen()));
        }
    }

    /// The requests the tab shows now: as the list has them, as actions left them.
    fn shown(&self, key: &str, tab: RequestsTab) -> Vec<MediaRequest> {
        let Some(list) = self.list_of(key) else { return Vec::new() };
        list.requests
            .iter()
            .filter_map(|r| match self.changed.get(&r.id) {
                Some(now) => now.clone(),
                None => Some(r.clone()),
            })
            .filter(|r| in_requests_tab(r, tab))
            .collect()
    }

    /// Does `a` to `r` (`reason`: Decline with a reason's words).
    fn act(&mut self, cx: &AppCx, r: MediaRequest, a: ActionId, reason: Option<String>) {
        self.panel = None;
        let busy =
            self.acting.iter().any(|x| x.request.id == r.id) || self.asking.as_ref().is_some_and(|(id, _)| *id == r.id);
        if busy {
            return;
        }
        if a == ActionId::Action(RequestAction::AskAgain) {
            // Asking again is asking: its toast says how it went.
            let seasons = r.seasons.clone().filter(|s| !s.is_empty());
            let body = NewRequest { kind: r.kind, tmdb_id: r.tmdb_id, seasons };
            self.asking = Some((r.id, Asking::start(cx, body)));
            return;
        }
        let id = r.id;
        let job = cx.spawn(move |api| match a {
            ActionId::Action(RequestAction::Approve | RequestAction::Retry) => api.approve_request(id).map(Some),
            ActionId::Action(RequestAction::Decline) => api.decline_request(id, None).map(Some),
            ActionId::DeclineReason => api.decline_request(id, reason.as_deref()).map(Some),
            ActionId::Action(RequestAction::Handled) => api.mark_request_handled(id).map(Some),
            ActionId::Action(RequestAction::Withdraw | RequestAction::Delete) => api.delete_request(id).map(|_| None),
            ActionId::Action(RequestAction::Unwatch) => api.unwatch_request(id).map(|_| None),
            ActionId::Action(RequestAction::AskAgain | RequestAction::Unknown) => Ok(None),
        });
        self.acting.push(Acting { request: r, action: a, job });
    }

    /// Takes what actions, asking and the news being seen answered.
    fn collect(&mut self, cx: &mut AppCx, manage: bool, time: TvTime) {
        let mut i = 0;
        while i < self.acting.len() {
            let Some(answer) = answer_of(&mut self.acting[i].job) else {
                i += 1;
                continue;
            };
            let Acting { request: r, action: a, .. } = self.acting.swap_remove(i);
            match answer {
                Ok(now) => {
                    cx.toast(action_done(a, &r));
                    // Withdrawn or no longer followed, it leaves this profile's list; a manager's
                    // list of everyone's is asked again.
                    if let Some(now) = now {
                        self.changed.insert(r.id, Some(now));
                    } else if a == ActionId::Action(RequestAction::Delete) || !manage {
                        self.changed.insert(r.id, None);
                    }
                }
                Err(e) if e.cancelled() => {}
                Err(e) => {
                    let why = requests_problem(&e);
                    if let RequestsProblem::Conflict { request: Some(now), .. } = &why {
                        self.changed.insert(r.id, Some((**now).clone()));
                    }
                    if why == RequestsProblem::Missing {
                        self.changed.insert(r.id, None);
                        cx.toast_error("This request was deleted meanwhile.");
                    } else {
                        cx.toast_error(ask_refusal(&why, r.kind, None, time));
                    }
                }
            }
            cx.state.requests_changed(Some(title_key(r.kind, r.tmdb_id)));
            cx.state.load_requests(true);
        }
        if let Some((_, asking)) = self.asking.as_mut()
            && asking.poll(cx, time).is_some()
        {
            self.asking = None;
        }
        if let Some(answer) = self.seen.as_mut().and_then(answer_of) {
            self.seen = None;
            if answer.is_ok() {
                cx.state.load_requests(true);
            }
        }
    }

    fn rail_if(&mut self, cx: &mut AppCx, nothing: bool) {
        if nothing && !std::mem::replace(&mut self.railed, true) {
            cx.to_rail();
        }
    }
}

impl Screen for RequestsScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let page = ui.scope_id();
        let time = self.clock.time(cx);
        let s = cx.state.request_summary().cloned();
        let manage = s.as_ref().is_some_and(|s| s.manage);
        self.collect(cx, manage, time);

        // An account that may not ask still sees what it asked for before; one with nothing to
        // see is told why.
        if s.is_some() && !has_requests(s.as_ref()) {
            self.settled = true;
            let problem = summary_problem(s.as_ref()).unwrap_or(RequestsProblem::Off);
            let (pressed, focusable) = problem_page(ui, cx, page.with("gate"), &problem, "Couldn't load the requests");
            self.rail_if(cx, !focusable);
            match pressed {
                Some(ProblemAction::Retry) => cx.state.load_requests(false),
                Some(ProblemAction::SignIn) => self.sign_in.open = true,
                Some(ProblemAction::Requests) | None => {}
            }
            self.sign_in.show(ui, cx, page.with("sign-in"));
            return;
        }

        // The title filter asks once typing pauses.
        if let Some(at) = self.typed_at {
            let q = self.text.text.trim().to_string();
            if cx.now - at >= FILTER_DEBOUNCE {
                self.typed_at = None;
                if q != self.filters.q {
                    self.filters.q = q;
                    self.start_over();
                }
            } else {
                ui.repaint_after(at + FILTER_DEBOUNCE - cx.now);
            }
        }

        // Until the summary says whose list this is, the first tab isn't known.
        if s.is_none() {
            for i in 0..4 {
                let y = LIST_TOP + 8.0 + i as f32 * (CARD_H + CARD_GAP);
                skeleton_block(ui, Rect::new(theme::GUTTER, y, CARD_W, CARD_H), radius::CARD);
            }
            return;
        }
        let tab = self.tab(manage);
        let f = self.filters(manage);
        let key = RequestsScreen::key(tab, &f);
        self.load(ui, cx, tab, &f, &key);
        let list = self.list_of(&key).cloned();
        let shown = self.shown(&key, tab);
        for r in &shown {
            self.art.entry(r.id).or_insert_with(|| art(cx, ui, r.poster_url.as_deref(), 48.0));
        }
        let error = self.error.as_ref().filter(|(k, _)| *k == key).map(|(_, e)| e.clone());

        if list.is_none()
            && let Some(e) = &error
        {
            let why = requests_problem(e);
            if problem_words(&why, manage).is_some() {
                self.settled = true;
                let (pressed, focusable) =
                    problem_page(ui, cx, page.with("problem"), &why, "Couldn't load the requests");
                self.rail_if(cx, !focusable);
                match pressed {
                    Some(ProblemAction::Retry) => self.stale = true,
                    Some(ProblemAction::SignIn) => self.sign_in.open = true,
                    _ => {}
                }
                self.sign_in.show(ui, cx, page.with("sign-in"));
                return;
            }
        }

        let low = manage || s.as_ref().is_some_and(|s| s.sign_in_again == Some(Scope::Approvals));
        let list_top = if low { LIST_TOP_LOW } else { LIST_TOP };
        let filtered = f.kind.is_some() || f.user_id.is_some() || !f.q.is_empty();
        let find_some = may_ask(s.as_ref()) && tab == RequestsTab::All && !manage && !filtered;
        let panel_open = self.panel.is_some();
        let typing = self.typing;

        // The list, or what it says instead.
        let list_id = page.with("list");
        let mut opened = None;
        let mut find = false;
        let mut retry = false;
        let mut draw_body = |this: &mut RequestsScreen, ui: &mut Ui| {
            let title_w = CARD_W;
            match &list {
                None => match &error {
                    Some(e) => {
                        let body = e.problem();
                        let r = Message::new(
                            MessageStyle::Column { top: list_top + 24.0 },
                            failed_title(Some(e), "Couldn't load the requests"),
                        )
                        .body(&body)
                        .action("Retry", Icon::RefreshCw)
                        .show(ui, page.with("note"));
                        retry |= r.action.is_some_and(|a| a.activated);
                    }
                    None => {
                        for i in 0..4 {
                            let y = list_top + 8.0 + i as f32 * (CARD_H + CARD_GAP);
                            skeleton_block(ui, Rect::new(theme::GUTTER, y, CARD_W, CARD_H), radius::CARD);
                        }
                    }
                },
                Some(_) if shown.is_empty() => {
                    let words = if filtered {
                        "No requests match these filters.".to_string()
                    } else {
                        requests_empty(tab, manage)
                    };
                    let mut message = Message::new(MessageStyle::Column { top: list_top + 24.0 }, &words);
                    if find_some {
                        message = message.action("Find something to ask for", Icon::Compass);
                    }
                    let r = message.show(ui, page.with("note"));
                    find |= r.action.is_some_and(|a| a.activated);
                }
                Some(l) => {
                    let more_line = l.total > l.requests.len() as i64 && this.limit >= REQUESTS_MAX;
                    let view = Rect::new(0.0, list_top, theme::CANVAS_W, theme::CANVAS_H - list_top);
                    let len = shown.len() + usize::from(more_line);
                    let art = &this.art;
                    let news = &this.news;
                    let r = List::new(view).gap(CARD_GAP).edges(Edge::Pass, Edge::Stop).show(
                        ui,
                        list_id,
                        &mut this.list,
                        len,
                        |_, i| if i < shown.len() { CARD_H } else { MORE_LINE },
                        |ui, slot| {
                            let y = slot.rect.y + if slot.index == 0 { 8.0 } else { 0.0 };
                            let Some(r) = shown.get(slot.index) else {
                                let at = Point::new(theme::GUTTER, y + 4.0);
                                ui.text(at, MORE_TEXT, &theme::CAPTION, &TextOpts::line(title_w), theme::MUTED);
                                return;
                            };
                            let rect = Rect::new(theme::GUTTER, y, CARD_W, CARD_H);
                            let response = ui.focusable(slot.id, rect);
                            let pictures = art.get(&r.id).map_or(&[][..], Vec::as_slice);
                            let p = focus_progress(ui, slot.id, response.focused);
                            card(ui, rect, r, manage, time, news.contains(&r.id), pictures, p);
                            if response.activated {
                                opened = Some(r.id);
                            }
                        },
                    );
                    if let Some(i) = r.focused.filter(|i| *i < shown.len()) {
                        this.list_focus = Some((key.clone(), i, shown[i].id));
                        // Five from the end: 30 more, while the server has more.
                        let have = l.requests.len() as i64;
                        if i + 5 >= shown.len() && l.total > have && this.limit <= have && this.limit < REQUESTS_MAX {
                            this.limit = (have + REQUESTS_PAGE).min(REQUESTS_MAX);
                        }
                    }
                    ui.initial_focus(RequestsScreen::card_id(page, 0));
                    // More below: the list fades out over its last 40.
                    let end = list_top + 8.0 + (CARD_H + CARD_GAP) * shown.len() as f32 - CARD_GAP;
                    if end - r.scroll > theme::CANVAS_H {
                        let fade = Rect::new(0.0, theme::CANVAS_H - 40.0, theme::CANVAS_W, 40.0);
                        ui.gradient(
                            fade,
                            true,
                            vec![
                                pley_base::GradientStop { at: 0.0, color: theme::BG.alpha(0.0) },
                                pley_base::GradientStop { at: 1.0, color: theme::BG },
                            ],
                        );
                    }
                }
            }
        };
        if panel_open || typing {
            inert(ui, |ui| draw_body(self, ui));
        } else {
            draw_body(self, ui);
        }
        if retry {
            self.stale = true;
        }
        if find {
            cx.push(Route::Discover);
        }
        if let Some(id) = opened {
            self.panel = Some(Panel::Request { id, step: Step::Actions });
            self.last_row = None;
            self.focus_panel = true;
        }

        // Over the list: the page's title, the tabs, the filters.
        ui.fill(Rect::new(0.0, 0.0, theme::CANVAS_W, list_top), theme::BG, 0.0);
        let width = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;
        ui.text(
            Point::new(theme::GUTTER, theme::SAFE_Y),
            requests_title(s.as_ref()),
            &theme::PAGE_TITLE,
            &TextOpts::line(width),
            theme::FG,
        );
        let counts = list.as_ref().map(|l| &l.counts);
        let tabs_id = page.with("tabs");
        let mut chosen = None;
        let empty = list.is_some() && shown.is_empty() && !find_some;
        let mut draw_top = |this: &mut RequestsScreen, ui: &mut Ui| {
            ui.group(tabs_id, Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop), |ui| {
                let mut x = theme::GUTTER;
                for (t, label) in requests_tabs(manage) {
                    // Filtered, Needs attention counts what its list holds (the summary's number is
                    // everyone's): known while it shows.
                    let n = if t == RequestsTab::Attention && filtered {
                        if tab == RequestsTab::Attention { list.as_ref().map(|l| l.total) } else { None }
                    } else {
                        tab_count(t, counts, s.as_ref())
                    };
                    let control = Counted {
                        look: if t == tab { focus::TAB_CURRENT } else { focus::TAB },
                        icon: None,
                        label,
                        count: n.filter(|n| *n > 0).map(|n| count_text(n, 999)),
                        danger: false,
                        padding: 12.0,
                    };
                    let id = RequestsScreen::tab_id(page, t);
                    if t == tab && empty {
                        ui.initial_focus(id);
                    }
                    let w = control.size(ui).w;
                    if control.show(ui, id, Point::new(x, TABS_Y)).activated {
                        chosen = Some(t);
                    }
                    x += w + 8.0;
                }
            });
            if manage {
                this.filters_ui(ui, page);
            } else if s.as_ref().is_some_and(|s| s.sign_in_again == Some(Scope::Approvals)) {
                ui.group(page.with("filters"), bar_group(), |ui| {
                    let y = FILTERS_Y + 2.0;
                    ui.icon(Rect::new(theme::GUTTER, y + 10.0, 20.0, 20.0), Icon::UserRound, theme::MUTED);
                    let x = theme::GUTTER + 30.0;
                    let pill = Button::pill("Sign in again");
                    let w = theme::CANVAS_W - theme::SAFE_X - x - pill.size(ui).w - 16.0;
                    let used = ui
                        .text(
                            Point::new(x, y + 10.0),
                            SIGN_IN_APPROVALS,
                            &theme::BODY,
                            &TextOpts::line(w),
                            theme::MUTED,
                        )
                        .w;
                    if pill.show(ui, ui.id("signin"), Point::new(x + used + 16.0, y)).activated {
                        this.sign_in.open = true;
                    }
                });
            }
        };
        if panel_open || typing {
            inert(ui, |ui| draw_top(self, ui));
        } else {
            draw_top(self, ui);
        }
        if let Some(t) = chosen.filter(|t| *t != tab) {
            self.tab = Some(t);
            self.start_over();
        }
        if self.first && list.is_some() && self.panel.is_none() {
            self.first = false;
            match shown.is_empty() {
                true if !find_some => ui.request_focus(RequestsScreen::tab_id(page, tab)),
                true => {}
                false => ui.request_focus(RequestsScreen::card_id(page, 0)),
            }
        }
        if let Some((k, i, id)) = self.list_focus.clone()
            && k == key
            && list.is_some()
            && !shown.iter().any(|r| r.id == id)
            && self.panel.is_none()
        {
            self.list_focus = None;
            match shown.len() {
                0 => ui.request_focus(RequestsScreen::tab_id(page, tab)),
                n => ui.request_focus(RequestsScreen::card_id(page, i.min(n - 1))),
            }
        }
        self.settled = list.is_some() || error.is_some();

        // The title filter's keyboard.
        if self.typing {
            let r = Keyboard::new().action_label("Search").show(
                ui,
                page.with("keyboard"),
                &mut self.keyboard,
                &mut self.text,
            );
            if r.changed {
                self.typed_at = Some(cx.now);
            }
            if r.done || r.closed {
                self.typing = false;
                ui.request_focus(page.with("filters").with("q"));
            }
        }

        self.panel_ui(ui, cx, page, &shown, list.as_ref(), manage, time);
        self.sign_in.show(ui, cx, page.with("sign-in"));
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if std::mem::take(&mut self.reason_typing) || std::mem::take(&mut self.typing) {
            return true;
        }
        match &mut self.panel {
            // Back from a question goes back to the actions, not out of the panel.
            Some(Panel::Request { step, .. }) if *step != Step::Actions => {
                *step = Step::Actions;
                self.focus_panel = true;
                true
            }
            Some(_) => {
                self.panel = None;
                true
            }
            None => false,
        }
    }

    fn event(&mut self, event: &AppEvent, _cx: &mut AppCx) {
        match event {
            AppEvent::RequestsChanged { .. } => self.stale = true,
            AppEvent::Shown | AppEvent::Visible(true) => {
                self.clock.forget();
                self.stale = true;
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn takes_focus(&self) -> bool {
        self.settled
    }

    fn modal(&self) -> bool {
        self.panel.is_some() || self.typing || self.reason_typing || self.sign_in.open
    }
}

impl RequestsScreen {
    /// The managers' filters: movies or shows, a person, a title's words.
    fn filters_ui(&mut self, ui: &mut Ui, page: Id) {
        let id = page.with("filters");
        let person = self.person.as_ref().filter(|p| Some(p.id) == self.filters.user_id).map(|p| p.name.clone());
        ui.group(id, bar_group(), |ui| {
            let y = FILTERS_Y + 2.0;
            let mut x = theme::GUTTER;
            let kind =
                Button::chip(kind_label(self.filters.kind)).icon(Icon::Film).selected(self.filters.kind.is_some());
            if kind.show(ui, ui.id("kind"), Point::new(x, y)).activated {
                self.filters.kind = next_kind(self.filters.kind);
                self.start_over();
            }
            x += kind.size(ui).w + 10.0;
            let name = person.clone().unwrap_or_else(|| "Everyone".into());
            let who =
                Button::chip(&name).icon(Icon::Users).selected(self.filters.user_id.is_some()).max_label_width(140.0);
            if who.show(ui, ui.id("person"), Point::new(x, y)).activated {
                self.panel = Some(Panel::People);
                self.focus_panel = true;
            }
            x += who.size(ui).w + 10.0;
            let field =
                TextField::new(&self.text.text, 240.0).placeholder("Title").icon(Icon::Search).typing(self.typing);
            if field.show(ui, ui.id("q"), Point::new(x, FILTERS_Y)).activated {
                self.typing = true;
                self.keyboard = KeyboardState::default();
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn panel_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppCx,
        page: Id,
        shown: &[MediaRequest],
        list: Option<&RequestList>,
        manage: bool,
        time: TvTime,
    ) {
        let id = page.with("panel");
        // The request went while its panel was open: the panel goes too.
        if let Some(Panel::Request { id: rid, .. }) = &self.panel
            && !shown.iter().any(|r| r.id == *rid)
        {
            self.panel = None;
        }
        let panel = self.panel.clone();
        let request = match &panel {
            Some(Panel::Request { id, .. }) => shown.iter().find(|r| r.id == *id).cloned(),
            _ => None,
        };
        let mut pressed: Option<&'static str> = None;
        let mut action: Option<ActionRow> = None;
        let mut pick: Option<Option<RequestUser>> = None;
        let focus = std::mem::take(&mut self.focus_panel);
        let mut focus_to = None;
        let rows = request.as_ref().map(action_rows).unwrap_or_default();
        let last = self.last_row;
        let reason_text = self.reason.text.clone();
        let reason_typing = self.reason_typing;
        let people: Vec<RequestUser> = {
            let mut people = list.map(|l| l.people.clone()).unwrap_or_default();
            // The person picked stays offered when the other filters leave them out.
            if let (Some(uid), Some(p)) = (self.filters.user_id, &self.person)
                && p.id == uid
                && !people.iter().any(|u| u.id == uid)
            {
                people.insert(0, p.clone());
            }
            people
        };
        let picked = self.filters.user_id;
        let mut show = |ui: &mut Ui| {
            SidePanel::new().back_closes(false).show(ui, id, panel.is_some(), |ui, area| {
                let mut y = area.y;
                let w = area.w;
                let row = |ui: &mut Ui, y: &mut f32, key: &str, r: ListRow| {
                    let rid = id.with(key);
                    let resp = r.show(ui, rid, Point::new(area.x, *y), w);
                    *y += r.height(ui, w) + 2.0;
                    (rid, resp.activated)
                };
                match (&panel, &request) {
                    (Some(Panel::People), _) => {
                        y += Heading::PanelTitle.show(ui, "Asked by", Point::new(area.x, y), w);
                        let (rid, hit) = row(
                            ui,
                            &mut y,
                            "person/all",
                            ListRow::new("Everyone").icon(Icon::Users).checked(picked.is_none()),
                        );
                        if picked.is_none() && focus {
                            focus_to = Some(rid);
                        }
                        if hit {
                            pick = Some(None);
                        }
                        for u in &people {
                            let r = ListRow::new(&u.name).icon(Icon::UserRound).checked(Some(u.id) == picked);
                            let (rid, hit) = row(ui, &mut y, &format!("person/{}", u.id), r);
                            if Some(u.id) == picked && focus {
                                focus_to = Some(rid);
                            }
                            if hit {
                                pick = Some(Some(u.clone()));
                            }
                        }
                    }
                    (Some(Panel::Request { step: Step::Confirm(c), .. }), Some(_)) => {
                        let q = c.confirm.clone().unwrap_or_else(|| pley_core::requests::Confirm {
                            title: c.label.to_string(),
                            body: None,
                            button: c.label,
                        });
                        y += Heading::PanelTitle.show(ui, &q.title, Point::new(area.x, y), w);
                        if let Some(body) = q.body {
                            y += Heading::Note.show(ui, body, Point::new(area.x, y), w);
                        }
                        // Cancel first, as the app's other questions: OK pressed twice undoes nothing.
                        let (rid, hit) = row(ui, &mut y, "cancel", ListRow::new("Cancel").icon(Icon::ArrowLeft));
                        if focus {
                            focus_to = Some(rid);
                        }
                        if hit {
                            pressed = Some("cancel");
                        }
                        let (_, hit) =
                            row(ui, &mut y, "confirm", ListRow::new(q.button).icon(action_icon(c.id)).danger(true));
                        if hit {
                            action = Some(c.clone());
                        }
                    }
                    (Some(Panel::Request { step: Step::Reason, .. }), Some(r)) => {
                        let title = format!("Decline “{}”?", r.title);
                        y += Heading::PanelTitle.show(ui, &title, Point::new(area.x, y), w);
                        if !r.mine {
                            let who = r.requested_by.as_ref().map_or("Whoever asked", |u| u.name.as_str());
                            let text = format!("{who} sees that it was declined, and your reason if you give one.");
                            y += Heading::Note.show(ui, &text, Point::new(area.x, y), w);
                        }
                        y += 4.0;
                        let fid = id.with("reason");
                        let field =
                            TextField::new(&reason_text, w).placeholder("Reason (optional)").typing(reason_typing);
                        if field.show(ui, fid, Point::new(area.x, y)).activated {
                            pressed = Some("reason");
                        }
                        if focus {
                            focus_to = Some(fid);
                        }
                        y += pley_kit::keyboard::FIELD_HEIGHT + 12.0;
                        let (_, hit) = row(ui, &mut y, "decline", ListRow::new("Decline").icon(Icon::Ban));
                        if hit {
                            pressed = Some("decline");
                        }
                        let (_, hit) = row(ui, &mut y, "cancel", ListRow::new("Cancel").icon(Icon::ArrowLeft));
                        if hit {
                            pressed = Some("cancel");
                        }
                    }
                    (Some(Panel::Request { .. }), Some(r)) => {
                        let heading = match r.year {
                            Some(year) => format!("{} ({year})", r.title),
                            None => r.title.clone(),
                        };
                        y += Heading::PanelTitle.show(ui, &heading, Point::new(area.x, y), w);
                        let x = area.x + 16.0;
                        let inner = w - 32.0;
                        y += 4.0;
                        let stage = stage_words(r, manage, time);
                        y += stage_pill(ui, Point::new(x, y), &stage, true, inner).h;
                        if let Some(p) = stage.progress {
                            download_bar(ui, Rect::new(x, y + 10.0, inner, 4.0), p);
                            y += 14.0;
                        }
                        y += 8.0;
                        let facts = format!("{} · {}", request_what(r), asked_line(r, manage, time));
                        y += ui
                            .text(Point::new(x, y), &facts, &theme::CAPTION, &TextOpts::wrap(inner, 2), theme::MUTED)
                            .h;
                        if let Some(note) = &stage.note {
                            y += 2.0
                                + ui.text(
                                    Point::new(x, y + 2.0),
                                    note,
                                    &theme::CAPTION,
                                    &TextOpts::wrap(inner, 3),
                                    theme::MUTED,
                                )
                                .h;
                        }
                        if manage && let Some(note) = request_note(r) {
                            y += 2.0
                                + ui.text(
                                    Point::new(x, y + 2.0),
                                    &note,
                                    &theme::CAPTION,
                                    &TextOpts::wrap(inner, 2),
                                    theme::MUTED,
                                )
                                .h;
                        }
                        y += 12.0;
                        // Open, when the library has it for this profile; the actions; Details.
                        let first = if r.item_id.is_some() {
                            "open"
                        } else if rows.is_empty() {
                            "details"
                        } else {
                            ""
                        };
                        let mut wants = |key: &str, rid: Id, row_id: Option<ActionId>| {
                            let initial = match last {
                                Some(l) => row_id == Some(l),
                                None => key == first || (first.is_empty() && row_id == rows.first().map(|x| x.id)),
                            };
                            if focus && initial {
                                focus_to = Some(rid);
                            }
                        };
                        if r.item_id.is_some() {
                            let (rid, hit) = row(ui, &mut y, "open", ListRow::new("Open").icon(Icon::Play));
                            wants("open", rid, None);
                            if hit {
                                pressed = Some("open");
                            }
                        }
                        for a in &rows {
                            let key = format!("{:?}", a.id);
                            let (rid, hit) =
                                row(ui, &mut y, &key, ListRow::new(a.label).icon(action_icon(a.id)).danger(a.danger));
                            wants(&key, rid, Some(a.id));
                            if hit {
                                action = Some(a.clone());
                            }
                        }
                        let (rid, hit) = row(
                            ui,
                            &mut y,
                            "details",
                            ListRow::new("Details").icon(Icon::Info).trailing(Trailing::None),
                        );
                        wants("details", rid, None);
                        if hit {
                            pressed = Some("details");
                        }
                    }
                    _ => {}
                }
            })
        };
        let response = if self.reason_typing { inert(ui, show) } else { show(ui) };
        if let Some(target) = focus_to {
            ui.request_focus(target);
        }
        if response.closed {
            self.panel = None;
        }
        if let Some(p) = pick {
            self.panel = None;
            self.filters.user_id = p.as_ref().map(|u| u.id);
            self.person = p;
            self.start_over();
        }
        let Some(r) = request else { return };
        if let Some(a) = action {
            self.last_row = Some(a.id);
            match (&self.panel, a.id) {
                (Some(Panel::Request { step: Step::Confirm(_), .. }), id) => self.act(cx, r.clone(), id, None),
                (_, ActionId::DeclineReason) => {
                    self.panel = Some(Panel::Request { id: r.id, step: Step::Reason });
                    self.reason = TextEdit::new("").limit(500);
                    self.focus_panel = true;
                }
                (_, _) if a.confirm.is_some() => {
                    self.panel = Some(Panel::Request { id: r.id, step: Step::Confirm(a.clone()) });
                    self.focus_panel = true;
                }
                (_, id) => self.act(cx, r.clone(), id, None),
            }
        }
        match pressed {
            Some("open") => {
                if let Some(id) = r.item_id {
                    self.panel = None;
                    cx.push(Route::Item { id });
                }
            }
            Some("details") => {
                self.panel = None;
                cx.push(Route::Title { kind: r.kind, tmdb_id: r.tmdb_id, pick: false });
            }
            Some("cancel") => {
                self.panel = Some(Panel::Request { id: r.id, step: Step::Actions });
                self.focus_panel = true;
            }
            Some("reason") => {
                self.reason_typing = true;
                self.reason_keyboard = KeyboardState::default();
            }
            Some("decline") => {
                let reason = Some(self.reason.text.trim().to_string()).filter(|t| !t.is_empty());
                self.act(cx, r.clone(), ActionId::DeclineReason, reason);
            }
            _ => {}
        }
        // The reason's keyboard: Done, and Decline is next.
        if self.reason_typing {
            let k = Keyboard::new().action_label("Done").show(
                ui,
                page.with("reason-keyboard"),
                &mut self.reason_keyboard,
                &mut self.reason,
            );
            if k.done || k.closed {
                self.reason_typing = false;
                self.focus_decline = k.done;
                if !k.done {
                    ui.request_focus(id.with("reason"));
                }
            }
        }
        if std::mem::take(&mut self.focus_decline) {
            ui.request_focus(id.with("decline"));
        }
    }
}

/// A request as a card (`RequestCard`): poster, title and year, what was asked for, who asked and
/// when, the reason or note, the stage; focus lifts it with the tiles' ring.
#[allow(clippy::too_many_arguments)]
fn card(
    ui: &mut Ui,
    rect: Rect,
    r: &MediaRequest,
    manage: bool,
    time: TvTime,
    news: bool,
    art: &[ImageRequest],
    p: f32,
) {
    let scale = 1.0 + 0.02 * p;
    let fill = theme::SURFACE.lerp(theme::SURFACE3, p);
    ui.transform(Point::ZERO, scale, rect.center(), |ui| {
        ui.fill(rect, fill, radius::DIALOG);
        // The poster.
        let poster = Rect::new(rect.x + 12.0, rect.y + (rect.h - 72.0) / 2.0, 48.0, 72.0);
        let shown = pley_kit::first_art(ui, art, pley_base::Priority::Visible);
        if !shown.is_some_and(|s| s.opaque()) {
            placeholder(ui, poster, &r.title, "", radius::TILE);
        }
        if let Some(s) = shown {
            ui.draw_image(poster, &s, &pley_ui::ImageOpts::cover().rounded(radius::TILE));
        }
        if news {
            let dot = Rect::new(rect.x + 54.0, rect.y + 6.0, 10.0, 10.0);
            ui.fill(dot.expand(2.0), fill, 7.0);
            ui.fill(dot, theme::ACCENT, 5.0);
        }
        // The stage at the right.
        let stage = stage_words(r, manage, time);
        let stage_w = 260.0;
        let right = Point::new(rect.right() - 20.0, 0.0);
        let h =
            crate::common::pill_size(ui, &stage, false, stage_w).h + if stage.progress.is_some() { 8.0 } else { 0.0 };
        stage_block(ui, Point::new(right.x, rect.y + (rect.h - h) / 2.0), &stage, stage_w, true);
        // The words.
        let x = poster.right() + 16.0;
        let w = (rect.right() - 20.0 - stage_w - 16.0 - x).max(40.0);
        let sub = format!("{} · {}", request_what(r), asked_line(r, manage, time));
        let note = [stage.note.clone(), if manage { request_note(r) } else { None }]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let block = theme::TITLE.line_height
            + 3.0
            + theme::CAPTION.line_height
            + if note.is_empty() { 0.0 } else { 2.0 + theme::CAPTION.line_height };
        let mut y = rect.y + (rect.h - block) / 2.0;
        let title = ui.text(Point::new(x, y), &r.title, &theme::TITLE, &TextOpts::line(w), theme::FG);
        if let Some(year) = r.year.filter(|y| *y != 0) {
            let rest = (w - title.w).max(0.0);
            if rest > 40.0 {
                let at = Point::new(x + title.w, y + 2.0);
                ui.text(at, &format!(" · {year}"), &theme::LABEL, &TextOpts::line(rest), theme::MUTED);
            }
        }
        y += theme::TITLE.line_height + 3.0;
        ui.text(Point::new(x, y), &sub, &theme::CAPTION, &TextOpts::line(w), theme::MUTED);
        y += theme::CAPTION.line_height + 2.0;
        if !note.is_empty() {
            ui.text(Point::new(x, y), &note, &theme::CAPTION, &TextOpts::line(w), theme::SUBTLE);
        }
    });
    if p > 0.0 {
        let grown = rect.scale_about(rect.center(), scale);
        let ring = grown.expand(5.56);
        ui.stroke(ring, focus::RING_COLOR.alpha(p), 13.56, 2.78);
    }
}
