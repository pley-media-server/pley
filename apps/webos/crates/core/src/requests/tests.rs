//! The TV app's tests of requests' words and rules, ported.

use super::*;
use crate::model::{RequestDownload, RequestUser};

/// Friday 30 October 2026, noon on the TV's clock, an hour ahead of UTC.
const AT: i64 = 1_793_358_000;
const OFFSET: i64 = 3600;
const MIN: i64 = 60;
const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

fn now() -> TvTime {
    TvTime::new(AT, OFFSET, true)
}

fn summary(f: impl FnOnce(&mut RequestSummary)) -> RequestSummary {
    let mut s = RequestSummary { enabled: true, access: RequestAccess::Ask, ..RequestSummary::default() };
    f(&mut s);
    s
}

fn request(f: impl FnOnce(&mut MediaRequest)) -> MediaRequest {
    let mut r = MediaRequest {
        id: 1,
        kind: TitleKind::Movie,
        tmdb_id: 10,
        title: "Arrival".into(),
        year: Some(2016),
        status: RequestStatus::Pending,
        stage: RequestStage::Pending,
        mine: true,
        created_at: AT - 3 * DAY,
        updated_at: AT,
        ..MediaRequest::default()
    };
    f(&mut r);
    r
}

fn with_status(status: RequestStatus, stage: RequestStage) -> impl FnOnce(&mut MediaRequest) {
    move |r| {
        r.status = status;
        r.stage = stage;
    }
}

fn season(number: i64, state: SeasonState) -> DiscoverSeason {
    DiscoverSeason {
        number,
        name: if number > 0 { format!("Season {number}") } else { "Specials".into() },
        episodes: 10,
        aired: 10,
        state,
        ..DiscoverSeason::default()
    }
}

fn detail(f: impl FnOnce(&mut DiscoverDetail)) -> DiscoverDetail {
    let mut d = DiscoverDetail {
        title: DiscoverTitle {
            kind: TitleKind::Movie,
            tmdb_id: 10,
            title: "Arrival".into(),
            year: Some(2016),
            rating: Some(7.9),
            ..DiscoverTitle::default()
        },
        runtime_minutes: Some(116),
        ..DiscoverDetail::default()
    };
    f(&mut d);
    d
}

fn show(seasons: Vec<DiscoverSeason>, f: impl FnOnce(&mut DiscoverDetail)) -> DiscoverDetail {
    detail(|d| {
        d.title.kind = TitleKind::Show;
        d.title.tmdb_id = 20;
        d.title.title = "Severance".into();
        d.runtime_minutes = None;
        d.seasons = Some(seasons);
        f(d);
    })
}

fn refusal(status: u16, code: &str, message: &str, body: serde_json::Value) -> ApiError {
    ApiError::Status { status, code: code.into(), message: message.into(), body: Some(body) }
}

#[test]
fn who_sees_what() {
    assert!(may_ask(Some(&summary(|_| {}))));
    assert!(may_ask(Some(&summary(|s| s.access = RequestAccess::Auto))));
    assert!(!may_ask(Some(&summary(|s| s.access = RequestAccess::Off))));
    assert!(!may_ask(Some(&summary(|s| s.enabled = false))));
    assert!(!may_ask(None));

    assert!(!has_requests(Some(&summary(|s| s.access = RequestAccess::Off))));
    assert!(has_requests(Some(&summary(|s| {
        s.access = RequestAccess::Off;
        s.own = 2;
    }))));
    assert!(!has_requests(Some(&summary(|s| s.sign_in_again = Some(SignInAgain::Requests)))));
    assert!(has_requests(Some(&summary(|s| s.sign_in_again = Some(SignInAgain::Approvals)))));
    assert!(!has_requests(Some(&summary(|s| {
        s.enabled = false;
        s.own = 3;
    }))));

    let manager = |pending, failed, unseen| {
        summary(move |s| {
            s.manage = true;
            (s.pending, s.failed, s.unseen) = (pending, failed, unseen);
        })
    };
    assert_eq!(requests_count(Some(&manager(2, 1, 1))), (4, true));
    assert_eq!(requests_count(Some(&manager(2, 0, 1))), (3, false));
    assert_eq!(
        requests_count(Some(&summary(|s| {
            s.pending = 5;
            s.unseen = 1;
        }))),
        (1, false)
    );
    assert_eq!(
        requests_count(Some(&summary(|s| {
            s.enabled = false;
            s.unseen = 4;
        }))),
        (0, false)
    );
    assert_eq!(requests_title(Some(&manager(0, 0, 0))), "Requests");
    assert_eq!(requests_title(None), "Your requests");
}

#[test]
fn tabs() {
    let labels: Vec<_> = requests_tabs(true).into_iter().map(|(_, l)| l).collect();
    assert_eq!(labels, ["Needs attention", "All", "Pending", "Approved", "Available", "Declined", "Failed"]);
    assert_eq!(requests_tabs(false)[0], (RequestsTab::All, "All"));
    // An empty list says why as Android's does: a status's tab is a filter.
    let pending = RequestsTab::Status(RequestStatus::Pending);
    assert_eq!(requests_empty(pending, false, true), ("No requests match", "Try other filters."));
    assert_eq!(requests_empty(RequestsTab::All, true, true).0, "No requests match");
    assert_eq!(
        requests_empty(RequestsTab::Attention, false, false),
        ("Nothing needs you", "Requests waiting for a decision, and those that couldn't be sent, show here.")
    );
    assert_eq!(
        requests_empty(RequestsTab::All, false, true),
        ("Nothing asked for yet", "Find movies and shows in Discover, and ask for what the libraries don't have.")
    );
    assert_eq!(
        requests_empty(RequestsTab::All, false, false),
        ("Nothing asked for yet", "Requests you made or follow show here.")
    );

    let q = requests_query(RequestsTab::Attention, &RequestFilters { q: "  dune ".into(), ..Default::default() }, 50);
    assert!(q.attention && q.oldest);
    assert_eq!((q.q.as_deref(), q.offset, q.limit), (Some("dune"), Some(0), Some(50)));

    let failed = request(|r| {
        r.status = RequestStatus::Failed;
        r.retry_at = Some(AT + MIN);
    });
    assert!(!in_requests_tab(&failed, RequestsTab::Attention));
    assert!(in_requests_tab(&failed, RequestsTab::Status(RequestStatus::Failed)));

    let counts = RequestCounts { pending: 1, approved: 2, declined: 3, failed: 4, available: 5 };
    assert_eq!(tab_count(RequestsTab::All, Some(&counts), None), Some(15));
    assert_eq!(tab_count(RequestsTab::Status(RequestStatus::Failed), Some(&counts), None), Some(4));
    assert_eq!(tab_count(RequestsTab::Attention, Some(&counts), None), None);
}

#[test]
fn a_titles_badge() {
    assert_eq!(title_badge(TitleStatus::Library, Some(5)), Some("In library"));
    assert_eq!(title_badge(TitleStatus::Partial, Some(5)), Some("Partly in library"));
    assert_eq!(title_badge(TitleStatus::Partial, None), Some("Requested"));
    assert_eq!(title_badge(TitleStatus::Requested, None), Some("Requested"));
    assert_eq!(title_badge(TitleStatus::Declined, None), Some("Declined"));
    assert_eq!(title_badge(TitleStatus::Restricted, None), None);
    assert_eq!(title_badge(TitleStatus::None, None), None);
    assert_eq!(title_tone(TitleStatus::Partial, None), Tone::Blue);
    assert_eq!(title_subtitle(TitleKind::Show, Some(2022), true), "2022 · Show");
    assert_eq!(title_subtitle(TitleKind::Movie, None, false), "");
    assert_eq!(title_facts(&detail(|_| {})), "2016 · 1h 56m");
    assert_eq!(
        title_facts(&show(vec![season(0, SeasonState::Missing), season(1, SeasonState::Missing)], |_| {})),
        "2016 · 1 season"
    );
}

#[test]
fn a_badge_follows_the_news_of_a_request_this_profile_made_or_follows() {
    use RequestBadge::{Ask, Status, Unchanged};
    let of = |f: &dyn Fn(&mut MediaRequest)| status_from_request(Some(&request(|r| f(r))));
    assert_eq!(of(&|_| {}), Status(TitleStatus::Requested));
    assert_eq!(of(&with_status_fn(RequestStatus::Failed, RequestStage::Failed)), Status(TitleStatus::Requested));
    assert_eq!(of(&with_status_fn(RequestStatus::Approved, RequestStage::Downloading)), Status(TitleStatus::Requested));
    assert_eq!(of(&with_status_fn(RequestStatus::Declined, RequestStage::Declined)), Status(TitleStatus::Declined));
    assert_eq!(of(&with_status_fn(RequestStatus::Available, RequestStage::Available)), Status(TitleStatus::Library));
    assert_eq!(of(&with_status_fn(RequestStatus::Available, RequestStage::Gone)), Status(TitleStatus::None));
    assert_eq!(
        of(&|r| {
            r.kind = TitleKind::Show;
            (r.status, r.stage) = (RequestStatus::Approved, RequestStage::Partial);
            r.available_seasons = vec![1];
        }),
        Status(TitleStatus::Partial)
    );
    // Only the title's details can say: a show wholly ready may lack seasons nobody asked for, or
    // have them all; a request no longer seen may leave others' asking for the title.
    assert_eq!(
        of(&|r| {
            r.kind = TitleKind::Show;
            (r.status, r.stage) = (RequestStatus::Available, RequestStage::Available);
        }),
        Ask
    );
    assert_eq!(status_from_request(None), Ask);
    // One it follows counts; someone else's, which a manager hears of too, says nothing of the title.
    assert_eq!(
        of(&|r| {
            (r.mine, r.watching) = (false, true);
            (r.status, r.stage) = (RequestStatus::Declined, RequestStage::Declined);
        }),
        Status(TitleStatus::Declined)
    );
    assert_eq!(
        of(&|r| {
            r.mine = false;
            (r.status, r.stage) = (RequestStatus::Declined, RequestStage::Declined);
        }),
        Unchanged
    );
}

fn with_status_fn(status: RequestStatus, stage: RequestStage) -> impl Fn(&mut MediaRequest) {
    move |r| with_status(status, stage)(r)
}

#[test]
fn keeps_each_title_once_as_pages_come() {
    let t = |tmdb_id, kind| DiscoverTitle { kind, tmdb_id, ..DiscoverTitle::default() };
    let mut have = vec![t(1, TitleKind::Movie), t(2, TitleKind::Movie)];
    add_titles(&mut have, [t(2, TitleKind::Movie), t(3, TitleKind::Movie), t(2, TitleKind::Show)]);
    let keys: Vec<_> = have.iter().map(|x| title_key(x.kind, x.tmdb_id)).collect();
    assert_eq!(keys, ["movie:1", "movie:2", "movie:3", "show:2"]);
}

#[test]
fn search_words_are_trimmed_and_cut_to_200_characters() {
    assert_eq!(search_text("  dune  "), "dune");
    assert_eq!(search_text(&"a".repeat(250)), "a".repeat(200));
    assert_eq!(search_text(&format!("{}x", "𝄞".repeat(150))), format!("{}x", "𝄞".repeat(150)));
    assert_eq!(search_text(&"𝄞".repeat(250)), "𝄞".repeat(200));
    // What the cut leaves at its end goes too.
    assert_eq!(search_text(&format!("{} b", "a".repeat(199))), "a".repeat(199));
}

#[test]
fn main_action_asks_to_sign_in_first_then_restricted_then_open() {
    let again = summary(|s| s.sign_in_again = Some(SignInAgain::Requests));
    let library = detail(|d| {
        d.title.status = TitleStatus::Library;
        d.title.item_id = Some(3);
    });
    assert_eq!(main_action(&library, Some(&again), now()), MainAction::SignIn);
    assert_eq!(main_action(&library, Some(&summary(|_| {})), now()), MainAction::Open { item_id: Some(3) });
    let restricted = detail(|d| {
        d.title.status = TitleStatus::Restricted;
        d.restriction = Some("Rated R: above this account's age limit".into());
    });
    assert_eq!(
        main_action(&restricted, Some(&summary(|_| {})), now()),
        MainAction::Restricted { text: "Rated R: above this account's age limit".into() }
    );
}

#[test]
fn a_declined_title_says_when_it_may_be_asked_for_again() {
    let d = detail(|d| {
        d.title.status = TitleStatus::Declined;
        d.declined =
            Some(Declined { reason: Some("Not on any service.".into()), at: AT - 18 * DAY, again_at: AT + 12 * DAY });
    });
    assert_eq!(
        main_action(&d, Some(&summary(|_| {})), now()),
        MainAction::Declined { text: "Declined: “Not on any service”. You can ask again in 12 days.".into() }
    );
    let plain = Declined { reason: None, at: AT, again_at: AT + DAY };
    assert_eq!(declined_text(Some(&plain), now()), "Declined. You can ask again tomorrow.");
}

#[test]
fn requested_shows_its_own_one_followed_or_tell_me_too() {
    let s = summary(|_| {});
    let mine = request(|r| r.id = 7);
    let d = detail(|d| {
        d.title.status = TitleStatus::Requested;
        d.title.request_id = Some(7);
        d.requests = vec![mine.clone()];
    });
    assert_eq!(main_action(&d, Some(&s), now()), MainAction::Requested { request: Some(&mine) });
    let followed = request(|r| {
        r.id = 8;
        (r.mine, r.watching) = (false, true);
    });
    let d = detail(|d| {
        d.title.status = TitleStatus::Requested;
        d.watching = true;
        d.requests = vec![followed.clone()];
    });
    assert_eq!(main_action(&d, Some(&s), now()), MainAction::Following { request: Some(&followed) });
    let theirs = show(
        vec![season(1, SeasonState::Library), season(2, SeasonState::Requested), season(3, SeasonState::Requested)],
        |d| d.title.status = TitleStatus::Requested,
    );
    assert_eq!(main_action(&theirs, Some(&s), now()), MainAction::TellMeToo { seasons: Some(vec![2, 3]) });
    let movie = detail(|d| d.title.status = TitleStatus::Requested);
    assert_eq!(main_action(&movie, Some(&s), now()), MainAction::TellMeToo { seasons: None });
    // Hearing costs nothing, but asking needs access.
    let off = summary(|s| s.access = RequestAccess::Off);
    assert_eq!(main_action(&movie, Some(&off), now()), MainAction::NoAccess);
}

#[test]
fn a_show_partly_there_offers_open_and_more_seasons() {
    let s = summary(|_| {});
    let d = show(vec![season(1, SeasonState::Library), season(2, SeasonState::Missing)], |d| {
        d.title.status = TitleStatus::Partial;
        d.title.item_id = Some(4);
    });
    assert_eq!(
        main_action(&d, Some(&s), now()),
        MainAction::More { item_id: Some(4), more: true, ask: true, disabled: false }
    );
    // Only seasons not out yet are left: nothing more to ask for now.
    let later = show(vec![season(1, SeasonState::Library), season(2, SeasonState::Unaired)], |d| {
        d.title.status = TitleStatus::Partial;
        d.title.item_id = Some(4);
    });
    assert!(matches!(main_action(&later, Some(&s), now()), MainAction::More { more: false, .. }));
}

#[test]
fn request_is_disabled_once_nothing_is_left() {
    let s = summary(|_| {});
    assert_eq!(main_action(&detail(|_| {}), Some(&s), now()), MainAction::Request { disabled: false });
    let used = detail(|d| {
        d.movies_left = Some(0);
        d.movies_next_at = Some(AT + 3 * DAY);
    });
    assert_eq!(main_action(&used, Some(&s), now()), MainAction::Request { disabled: true });
    // The seasons' allowance doesn't hold a movie back.
    let seasons = detail(|d| d.seasons_left = Some(0));
    assert_eq!(main_action(&seasons, Some(&s), now()), MainAction::Request { disabled: false });
    let off = summary(|s| s.access = RequestAccess::Off);
    assert_eq!(main_action(&detail(|_| {}), Some(&off), now()), MainAction::NoAccess);
    let listed = DiscoverTitle { status: TitleStatus::Requested, ..DiscoverTitle::default() };
    assert_eq!(list_action(&listed, Some(&s)), MainAction::Requested { request: None });
}

#[test]
fn the_stage_pill_follows_this_profiles_request_else_one_followed_else_the_newest() {
    let theirs = request(|r| r.mine = false);
    let followed = request(|r| {
        r.id = 2;
        (r.mine, r.watching) = (false, true);
    });
    let declined = request(|r| {
        r.id = 3;
        (r.status, r.stage) = (RequestStatus::Declined, RequestStage::Declined);
    });
    let d = detail(|d| d.requests = vec![declined.clone(), theirs.clone(), followed.clone()]);
    assert_eq!(title_request(&d), Some(&followed));
    let d = detail(|d| d.requests = vec![declined.clone(), theirs.clone()]);
    assert_eq!(title_request(&d), Some(&theirs));
    let d = detail(|d| d.requests = vec![declined.clone()]);
    assert_eq!(title_request(&d), None);
}

#[test]
fn seasons_are_named_as_the_web_names_them() {
    assert_eq!(seasons_label(&[2]), "Season 2");
    assert_eq!(seasons_label(&[5, 1, 2, 3]), "Seasons 1–3, 5");
    assert_eq!(seasons_label(&[0, 1, 2]), "Seasons 1–2 and specials");
    assert_eq!(seasons_label(&[0]), "Specials");
    assert_eq!(seasons_label(&[]), "");
}

#[test]
fn the_picker_lists_every_season_specials_last() {
    let d = show(
        vec![season(0, SeasonState::Missing), season(2, SeasonState::Missing), season(1, SeasonState::Library)],
        |_| {},
    );
    let numbers: Vec<_> = picker_seasons(&d).iter().map(|s| s.number).collect();
    assert_eq!(numbers, [1, 2, 0]);
}

#[test]
fn the_picker_ticks_missing_and_partly_had_seasons_up_to_what_may_be_asked_for() {
    let mut declined = season(4, SeasonState::Missing);
    declined.declined = true;
    let d = show(
        vec![
            season(0, SeasonState::Missing),
            season(1, SeasonState::Library),
            season(2, SeasonState::Partial),
            season(3, SeasonState::Missing),
            declined,
            season(5, SeasonState::Missing),
            season(6, SeasonState::Unaired),
        ],
        |_| {},
    );
    assert_eq!(preselected(&d, None), [2, 3, 5]);
    assert_eq!(preselected(&d, Some(2)), [2, 3]);
    assert_eq!(preselected(&d, Some(0)), Vec::<i64>::new());
    assert!(has_more_seasons(&d));
    let restricted = show(vec![season(1, SeasonState::Missing)], |d| d.title.status = TitleStatus::Restricted);
    assert_eq!(preselected(&restricted, None), Vec::<i64>::new());
}

#[test]
fn a_season_says_its_state_or_what_it_holds() {
    let line = |s: DiscoverSeason| season_line(&s, now());
    assert_eq!(line(season(1, SeasonState::Library)), "In library");
    assert_eq!(line(season(1, SeasonState::Requested)), "Requested");
    let mut declined = season(1, SeasonState::Missing);
    declined.declined = true;
    assert_eq!(line(declined), "Declined");
    assert_eq!(line(season(1, SeasonState::Partial)), "Partly in library");
    let mut one = season(1, SeasonState::Missing);
    one.episodes = 1;
    assert_eq!(line(one), "1 episode");
    let mut airs = season(3, SeasonState::Unaired);
    airs.air_date = Some("2026-11-04".into());
    assert_eq!(line(airs.clone()), "Airs Nov 4");
    airs.air_date = Some("2027-01-04".into());
    assert_eq!(line(airs), "Airs Jan 4, 2027");
    assert_eq!(line(season(3, SeasonState::Unaired)), "Not aired yet");

    assert_eq!(picker_button(3), "Request 3 seasons");
    assert_eq!(picker_button(1), "Request 1 season");
    assert_eq!(picker_button(0), "Pick a season.");
}

fn words(r: &MediaRequest) -> StageWords {
    stage_words(r, false, now())
}

#[test]
fn a_stage_is_coloured_by_where_it_has_got_to() {
    let at = |status, stage| words(&request(with_status(status, stage)));
    assert_eq!(at(RequestStatus::Pending, RequestStage::Pending), StageWords::new("Waiting for approval", Tone::Amber));
    assert_eq!(at(RequestStatus::Approved, RequestStage::Approved), StageWords::new("Approved", Tone::Blue));
    assert_eq!(at(RequestStatus::Approved, RequestStage::Searching), StageWords::new("Searching", Tone::Blue));
    assert_eq!(
        at(RequestStatus::Approved, RequestStage::Importing),
        StageWords::new("Adding to the library", Tone::Blue)
    );
    let ready = request(|r| {
        (r.status, r.stage) = (RequestStatus::Available, RequestStage::Available);
        r.item_id = Some(3);
    });
    assert_eq!(words(&ready), StageWords::new("Ready to watch", Tone::Green));
    assert_eq!(at(RequestStatus::Available, RequestStage::Gone), StageWords::new("No longer in a library", Tone::Gray));
    let declined = request(|r| {
        (r.status, r.stage) = (RequestStatus::Declined, RequestStage::Declined);
        r.reason = Some("Not here".into());
    });
    assert_eq!(words(&declined).note.as_deref(), Some("“Not here”"));
    // A stage this app doesn't know reads as its status.
    assert_eq!(at(RequestStatus::Declined, RequestStage::Unknown).text, "Declined");
    assert_eq!(at(RequestStatus::Unknown, RequestStage::Unknown).text, "Approved");
}

fn downloading(progress: f64, state: DownloadState, eta: Option<i64>) -> MediaRequest {
    request(|r| {
        (r.status, r.stage) = (RequestStatus::Approved, RequestStage::Downloading);
        r.download = Some(RequestDownload { progress, state, eta, episodes: 1 });
    })
}

#[test]
fn a_download_shows_its_share_when_it_ends_and_its_states() {
    assert_eq!(
        words(&downloading(0.423, DownloadState::Downloading, Some(AT + 20 * MIN))),
        StageWords { text: "Downloading 42% · in 20 min".into(), tone: Tone::Blue, progress: Some(0.423), note: None }
    );
    assert_eq!(words(&downloading(0.0, DownloadState::Queued, None)).text, "Waiting to download");
    assert_eq!(words(&downloading(0.5, DownloadState::Paused, None)).text, "Download paused");
    assert_eq!(words(&downloading(0.5, DownloadState::Warning, None)).text, "The download has a problem");
}

#[test]
fn a_show_partly_ready_says_which_seasons_with_the_bar_while_more_come() {
    let partial = request(|r| {
        r.kind = TitleKind::Show;
        (r.status, r.stage) = (RequestStatus::Approved, RequestStage::Partial);
        r.seasons = Some(vec![1, 2, 3]);
        r.available_seasons = vec![1, 2];
        r.download = Some(RequestDownload { progress: 0.1, state: DownloadState::Downloading, eta: None, episodes: 4 });
    });
    let w = words(&partial);
    assert_eq!((w.text.as_str(), w.tone, w.progress), ("Seasons 1–2 ready", Tone::Green, Some(0.1)));
}

#[test]
fn a_release_gives_its_day_and_an_episode_its_time_within_a_day() {
    let waiting = |kind, at| {
        request(move |r| {
            r.kind = kind;
            (r.status, r.stage) = (RequestStatus::Approved, RequestStage::Waiting);
            r.stage_at = at;
        })
    };
    // Radarr knows the day only: 12:00 UTC of 4 November.
    let nov4_noon_utc = AT + 5 * DAY + HOUR;
    assert_eq!(words(&waiting(TitleKind::Movie, Some(nov4_noon_utc))).text, "Out Nov 4");
    // The same day, far west of UTC.
    assert_eq!(
        stage_words(&waiting(TitleKind::Movie, Some(nov4_noon_utc)), false, TvTime::new(AT, -10 * HOUR, true)).text,
        "Out Nov 4"
    );
    let airs = waiting(TitleKind::Show, Some(AT + 9 * HOUR + 5 * MIN));
    assert_eq!(words(&airs).text, "Airs today at 21:05");
    assert_eq!(stage_words(&airs, false, TvTime::new(AT, OFFSET, false)).text, "Airs today at 9:05 PM");
    assert_eq!(words(&waiting(TitleKind::Show, Some(AT + 5 * DAY))).text, "Airs Nov 4");
    assert_eq!(words(&waiting(TitleKind::Movie, None)).text, "Not out yet");
}

#[test]
fn managers_read_why_sending_failed_and_everyone_else_that_an_administrator_will_look() {
    let failed = request(|r| {
        (r.status, r.stage) = (RequestStatus::Failed, RequestStage::Failed);
        r.reason = Some("Radarr isn't answering".into());
    });
    let w = stage_words(&failed, true, now());
    assert_eq!(
        (w.text.as_str(), w.tone, w.note.as_deref()),
        ("Couldn't send", Tone::Red, Some("Radarr isn't answering."))
    );
    let retrying = MediaRequest { retry_at: Some(AT + 5 * MIN), ..failed.clone() };
    assert_eq!(
        stage_words(&retrying, true, now()).note.as_deref(),
        Some("Radarr isn't answering. Trying again in 5 min.")
    );
    let plain = MediaRequest { reason: None, ..failed.clone() };
    let w = words(&plain);
    assert_eq!(
        (w.text.as_str(), w.note.as_deref()),
        ("Not sent yet", Some("It couldn't be sent to download yet: an administrator will look at it."))
    );
    let plain = MediaRequest { retry_at: Some(AT + 5 * MIN), ..plain };
    assert_eq!(words(&plain).note.as_deref(), Some("Trying again in 5 min."));
}

#[test]
fn managers_read_the_servers_note_as_a_sentence() {
    let note = |n: Option<&str>| request_note(&request(|r| r.note = n.map(str::to_string)));
    assert_eq!(
        note(Some("In Movies 4K, which Ann can't open")).as_deref(),
        Some("In Movies 4K, which Ann can't open.")
    );
    assert_eq!(
        note(Some("Radarr has the file, but no library has it: is its folder in one?")).as_deref(),
        Some("Radarr has the file, but no library has it: is its folder in one?")
    );
    assert_eq!(note(Some(" ")), None);
    assert_eq!(note(None), None);
}

#[test]
fn a_follower_hears_where_they_cant_open_what_is_ready() {
    let r = request(|r| {
        (r.status, r.stage) = (RequestStatus::Available, RequestStage::Available);
        (r.mine, r.watching) = (false, true);
    });
    assert_eq!(words(&r).note.as_deref(), Some("Ready, in a library you can't open: ask an administrator"));
}

#[test]
fn each_season_has_its_own_stage_while_a_show_is_partly_ready() {
    let download =
        RequestDownload { progress: 0.7, state: DownloadState::Downloading, eta: Some(AT + 2 * HOUR), episodes: 8 };
    let partial = request(|r| {
        r.kind = TitleKind::Show;
        (r.status, r.stage) = (RequestStatus::Approved, RequestStage::Partial);
        r.seasons = Some(vec![3, 4]);
        r.available_seasons = vec![3];
        r.download = Some(download);
    });
    assert_eq!(season_stage(&partial, 3, false, now()), Some(StageWords::new("Ready to watch", Tone::Green)));
    assert_eq!(
        season_stage(&partial, 4, false, now()),
        Some(StageWords {
            text: "Downloading 70% · in 2 hours".into(),
            tone: Tone::Blue,
            progress: Some(0.7),
            note: None
        })
    );
    assert!(moving(std::slice::from_ref(&partial)));
    let idle = MediaRequest { download: None, ..partial };
    assert_eq!(season_stage(&idle, 4, false, now()), None);
    assert!(!moving(&[idle]));
    let pending = request(|r| {
        r.kind = TitleKind::Show;
        r.seasons = Some(vec![3, 4]);
    });
    assert_eq!(season_stage(&pending, 4, false, now()).map(|w| w.text), Some("Waiting for approval".into()));
}

#[test]
fn asked_line_says_who_and_when() {
    assert_eq!(asked_line(&request(|_| {}), false, now()), "Asked by you · 3 days ago");
    let theirs = request(|r| {
        (r.mine, r.watching, r.watchers) = (false, true, 2);
        r.requested_by = Some(RequestUser { id: 2, name: "Ann".into(), ..RequestUser::default() });
        r.created_at = AT - DAY;
    });
    assert_eq!(asked_line(&theirs, true, now()), "You follow it · Asked by Ann · yesterday · 2 following");
}

#[test]
fn actions_are_the_servers_in_its_order_with_decline_with_a_reason_beside_decline() {
    use RequestAction::*;
    let rows = |actions: Vec<RequestAction>, f: &dyn Fn(&mut MediaRequest)| {
        action_rows(&request(|r| {
            r.actions = actions;
            f(r);
        }))
    };
    let labels: Vec<_> = rows(vec![Approve, Decline, Handled, Delete], &|_| {}).iter().map(|a| a.label).collect();
    assert_eq!(labels, ["Approve", "Decline", "Decline with a reason…", "Mark as added by hand", "Delete"]);
    let ids: Vec<_> = rows(vec![Retry, Decline], &|_| {}).iter().map(|a| a.id).collect();
    assert_eq!(ids, [ActionId::Action(Retry), ActionId::Action(Decline), ActionId::DeclineReason]);

    let withdraw = &rows(vec![Withdraw], &|_| {})[0];
    assert_eq!(withdraw.label, "Withdraw");
    assert_eq!(withdraw.confirm.as_ref().unwrap().title, "Withdraw your request for “Arrival”?");
    let remove = &rows(vec![Withdraw], &|r| (r.status, r.stage) = (RequestStatus::Declined, RequestStage::Declined))[0];
    assert_eq!(remove.label, "Remove");
    assert_eq!(remove.confirm.as_ref().unwrap().title, "Remove “Arrival” from your requests?");
    let del = &rows(vec![Delete], &|_| {})[0];
    assert!(del.danger);
    assert_eq!(
        del.confirm,
        Some(Confirm {
            title: "Delete this request?".into(),
            body: Some("Radarr and Sonarr keep what they have."),
            button: "Delete"
        })
    );
    // What this app doesn't know is left out.
    let labels: Vec<_> = rows(vec![Unwatch, Unknown, AskAgain], &|_| {}).iter().map(|a| a.label).collect();
    assert_eq!(labels, ["Stop following", "Request again"]);

    assert_eq!(action_done(ActionId::DeclineReason, &request(|_| {})), "Declined “Arrival”.");
}

fn allowance(
    movies_left: Option<i64>,
    seasons_left: Option<i64>,
    movies_next_at: Option<i64>,
    seasons_next_at: Option<i64>,
) -> Allowance {
    Allowance { movies_left, seasons_left, movies_next_at, seasons_next_at, ..Allowance::default() }
}

#[test]
fn the_allowance_says_how_many_and_the_limits_per_7_days() {
    let a = Allowance { movies_limit: Some(3), seasons_limit: Some(10), ..allowance(Some(2), Some(5), None, None) };
    assert_eq!(
        allowance_lines(&a, now()).unwrap(),
        ["You can ask for 2 more movies and 5 more seasons.", "Limits: 3 movies and 10 seasons per 7 days."]
    );
    let a = allowance(Some(0), None, Some(AT + 3 * DAY), None);
    assert_eq!(
        allowance_lines(&a, now()).unwrap(),
        ["You've asked for all the movies you can for now: you can ask for more in 3 days."]
    );
    let a = allowance(Some(0), Some(0), Some(AT + 3 * DAY), Some(AT + 5 * DAY));
    assert_eq!(
        allowance_lines(&a, now()).unwrap(),
        ["You've asked for all you can for now: more movies in 3 days, more seasons in 5 days."]
    );
    assert_eq!(allowance_lines(&allowance(None, None, None, None), now()), None);
}

#[test]
fn the_allowance_says_only_what_a_titles_kind_is_asked_within() {
    let a = Allowance {
        movies_limit: Some(3),
        seasons_limit: Some(10),
        ..allowance(Some(0), Some(0), Some(AT + 3 * DAY), Some(AT + 5 * DAY))
    };
    assert_eq!(
        allowance_lines(&Allowance { kind: Some(TitleKind::Movie), ..a }, now()).unwrap(),
        [
            "You've asked for all the movies you can for now: you can ask for more in 3 days.",
            "Limits: 3 movies per 7 days."
        ]
    );
    assert_eq!(
        allowance_lines(&Allowance { kind: Some(TitleKind::Show), ..a }, now()).unwrap(),
        [
            "You've asked for all the seasons you can for now: you can ask for more in 5 days.",
            "Limits: 10 seasons per 7 days."
        ]
    );
    let none = Allowance { movies_left: None, movies_next_at: None, kind: Some(TitleKind::Movie), ..a };
    assert_eq!(allowance_lines(&none, now()), None);
    // The details' counts, the summary's limits.
    let s = summary(|s| {
        s.movies_left = Some(9);
        s.movies_limit = Some(3);
    });
    let d = detail(|d| d.movies_left = Some(1));
    let a = Allowance::of(Some(&d), Some(&s), Some(TitleKind::Movie));
    assert_eq!((a.movies_left, a.movies_limit), (Some(1), Some(3)));
}

#[test]
fn asking_says_what_became_of_it() {
    let asked = |f: &dyn Fn(&mut MediaRequest), created| asked_text(&request(|r| f(r)), created);
    assert_eq!(asked(&|_| {}, true), "Asked for “Arrival”: an administrator decides.");
    assert_eq!(
        asked(&with_status_fn(RequestStatus::Approved, RequestStage::Approved), true),
        "“Arrival” is approved and on its way."
    );
    assert_eq!(
        asked(&with_status_fn(RequestStatus::Failed, RequestStage::Failed), true),
        "Asked for “Arrival”, but it couldn't be sent to download yet."
    );
    assert_eq!(asked(&|r| (r.mine, r.watching) = (false, true), false), "You'll hear when “Arrival” is ready.");
}

#[test]
fn a_refusal_says_when_one_may_ask_again_from_retry_at() {
    let declined = requests_problem(&refusal(
        409,
        "declined",
        "declined until 12 Nov",
        serde_json::json!({ "code": "declined", "message": "declined until 12 Nov", "retry_at": AT + 12 * DAY }),
    ));
    assert_eq!(ask_refusal(&declined, TitleKind::Movie, None, now()), "You can ask again in 12 days.");
    let limit = requests_problem(&refusal(
        429,
        "too_many_requests",
        "limit",
        serde_json::json!({ "code": "too_many_requests", "message": "limit", "retry_at": AT + 3 * DAY }),
    ));
    assert_eq!(ask_refusal(&limit, TitleKind::Show, None, now()), "You can ask for more in 3 days.");
    let too_many = requests_problem(&refusal(
        429,
        "too_many_requests",
        "limit",
        serde_json::json!({ "code": "too_many_requests", "message": "limit", "retry_at": null }),
    ));
    let limits = summary(|s| (s.movies_limit, s.seasons_limit) = (Some(3), Some(5)));
    assert_eq!(
        ask_refusal(&too_many, TitleKind::Show, Some(&limits), now()),
        "You can ask for at most 5 seasons per 7 days: pick fewer."
    );
}

#[test]
fn a_refusal_says_why_requests_cant_be_used() {
    let off = requests_problem(&refusal(
        503,
        "unavailable",
        "requests are off",
        serde_json::json!({ "code": "unavailable", "setup": false }),
    ));
    assert_eq!(
        problem_words(&off, false),
        Some(ProblemWords {
            title: "Requests are off",
            body: "This server doesn't take requests for movies and shows.",
            retry: false
        })
    );
    let old_down = requests_problem(&refusal(
        503,
        "unavailable",
        "x",
        serde_json::json!({ "code": "unavailable", "setup": true }),
    ));
    assert_eq!(old_down, RequestsProblem::TmdbDown);
    let down = requests_problem(&refusal(
        503,
        "tmdb_down",
        "TMDB isn't answering",
        serde_json::json!({ "code": "tmdb_down", "setup": true }),
    ));
    let w = problem_words(&down, false).unwrap();
    assert_eq!((w.title, w.retry), ("TMDB isn't answering", true));
    let key = requests_problem(&refusal(
        503,
        "tmdb_key",
        "refused",
        serde_json::json!({ "code": "tmdb_key", "setup": true, "retry": false }),
    ));
    let w = problem_words(&key, true).unwrap();
    assert_eq!((w.title, w.retry), ("TMDB refused this server's key", false));
    assert_eq!(problem_words(&key, false).unwrap().body, "An administrator needs to check it.");
    assert_eq!(ask_refusal(&key, TitleKind::Movie, None, now()), "TMDB refused this server's key.");
    assert_eq!(
        requests_problem(&refusal(403, "no_access", "no", serde_json::json!({ "code": "no_access" }))),
        RequestsProblem::NoAccess
    );
    assert_eq!(
        requests_problem(&refusal(
            403,
            "insufficient_scope",
            "scope",
            serde_json::json!({ "code": "insufficient_scope" })
        )),
        RequestsProblem::SignIn
    );
    assert_eq!(
        requests_problem(&refusal(
            403,
            "restricted",
            "rated R: above this account's age limit",
            serde_json::json!({ "code": "restricted" })
        )),
        RequestsProblem::Restricted { text: "Rated R: above this account's age limit.".into() }
    );
    // A gateway's 503, not Pley's: the server isn't there.
    let gateway = ApiError::Status { status: 503, code: "unavailable".into(), message: "x".into(), body: None };
    assert_eq!(requests_problem(&gateway), RequestsProblem::Other(gateway.clone()));
}

#[test]
fn a_conflict_hands_back_the_request_a_race_left_behind() {
    let current = request(with_status(RequestStatus::Approved, RequestStage::Approved));
    let p = requests_problem(&refusal(
        409,
        "conflict",
        "it was approved already",
        serde_json::json!({ "code": "conflict", "message": "it was approved already", "request": current }),
    ));
    assert_eq!(
        p,
        RequestsProblem::Conflict { text: "It was approved already.".into(), request: Some(Box::new(current)) }
    );
    assert_eq!(ask_refusal(&p, TitleKind::Movie, None, now()), "It was approved already.");
}

#[test]
fn news_names_what_is_ready_to_watch() {
    assert_eq!(ready_text("Arrival", None, true), "“Arrival” is ready to watch.");
    assert_eq!(ready_text("Severance", Some(&[2]), false), "Season 2 of “Severance” is ready to watch.");
    assert_eq!(ready_text("Severance", Some(&[1, 2]), false), "Seasons 1–2 of “Severance” are ready to watch.");
    assert_eq!(ready_text("Severance", Some(&[0]), false), "Specials of “Severance” are ready to watch.");
    assert_eq!(news_text(3), "3 of your requests have news");
    assert!(ready_news(&request(with_status(RequestStatus::Available, RequestStage::Available))));
    assert!(!ready_news(&request(|_| {})));
    assert_eq!(news_keys(4, true, Some(&[1])), ["4:a"]);
    assert_eq!(news_keys(4, false, None), ["4:p"]);
    assert_eq!(news_keys(4, false, Some(&[1, 2])), ["4:s1", "4:s2"]);
}

#[test]
fn relative_times_are_written_by_the_app() {
    let t = now();
    assert_eq!(time_until(AT + 10, t), "now");
    assert_eq!(time_until(AT + 5 * MIN, t), "in 5 min");
    assert_eq!(time_until(AT + HOUR, t), "in 1 hour");
    assert_eq!(time_until(AT + 3 * HOUR, t), "in 3 hours");
    assert_eq!(time_until(AT + DAY, t), "tomorrow");
    assert_eq!(time_until(AT + 12 * DAY, t), "in 12 days");
    assert_eq!(relative_time(AT - 30, t), "now");
    assert_eq!(relative_time(AT - 3 * HOUR, t), "3 hours ago");
    assert_eq!(relative_time(AT - DAY, t), "yesterday");
    assert_eq!(relative_time(AT - 3 * DAY, t), "3 days ago");
    assert_eq!(relative_time(AT - 40 * DAY, t), "Sep 20, 2026");
    assert_eq!(short_date(AT + 5 * DAY, t, false), "Nov 4");
    assert_eq!(short_date(AT + 90 * DAY, t, false), "Jan 28, 2027");
    assert_eq!(day_and_time(AT + 23 * HOUR, true, t), "tomorrow at 11:00");
    assert_eq!(day_and_time(AT + 2 * DAY, true, t), "Nov 1");
}
