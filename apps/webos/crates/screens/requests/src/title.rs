//! A movie or show from TMDB (`title.tsx`): its backdrop (else its poster at the right), the hero
//! with its facts, tagline and overview and its one main action, its seasons each with its state
//! (a requested one with its own stage), the cast, and the season picker beside it (`pick`: it
//! opens with the page, from Request more seasons). Its details are asked again when requests
//! change for the title, and every 10 s while one of its downloads moves.

use pley_base::{Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::http::sentence;
use pley_core::model::{DiscoverDetail, DiscoverSeason, NewRequest, RequestSummary, TitleKind};
use pley_core::requests::{
    Allowance, MainAction, RequestsProblem, TMDB_CREDIT, Tone, TvTime, allowance_lines, askable, has_requests,
    main_action, moving, picker_button, picker_seasons, preselected, requests_problem, requests_title, season_line,
    season_stage, title_key,
};
use pley_kit::{
    Backdrop, BackdropArt, BackdropScrim, BackdropState, Button, ColumnState, Dialog, Heading, InfoBlock, List,
    ListRow, ListState, Message, MessageStyle, Row, RowColumn, RowState, Section, SidePanel, Tile, TileKind, Trailing,
    backdrop_px, bar_group, skeleton_block,
};
use pley_shell::{AppCx, AppEvent, Chrome, Resource, Route, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Edge, Id, Ui};

use crate::actions::{Actions, Extra, TitleIntent};
use crate::common::{
    Asking, Poll, ProblemAction, SignInAgain, ToneTile, TvClock, Unwatching, WARNING, art, keep_last_words, keep_title,
    problem_page,
};
use crate::widgets::title_meta;

/// The hero's section: 52 above it, the focused button's label and 8 below (as the item page).
const HERO_TOP: f32 = 52.0;
const HERO_PAD: f32 = HERO_TOP + 23.0 + 8.0;
/// A short hero still puts the first row's header at 400, so the row's captions start under the
/// screen rather than cut at its foot.
const HERO_MIN: f32 = 372.0;
const TEXT_COLUMN: f32 = 464.0;
/// A season's stage may take a second line under its name ("Waiting for / approval").
const SEASON_STAGE_LINE: f32 = 17.0;
/// The overview's lines in the hero.
const OVERVIEW_LINES: u16 = 3;

const HERO: u64 = 0;
const SEASONS: u64 = 1;
const CAST: u64 = 2;
const CREDIT: u64 = 3;

/// Whether a show's seasons may be picked to ask for now.
pub fn can_pick(d: &DiscoverDetail, s: Option<&RequestSummary>, time: TvTime) -> bool {
    if d.title.kind != TitleKind::Show || !picker_seasons(d).iter().any(|x| askable(x, d.title.status)) {
        return false;
    }
    match main_action(d, s, time) {
        MainAction::Request { .. } => true,
        MainAction::More { more, ask, .. } => more && ask,
        _ => false,
    }
}

/// A season's badge on its poster: what the season is for this profile, none where it's still to
/// ask for.
pub fn season_badge(s: &DiscoverSeason) -> Option<(&'static str, Tone)> {
    use pley_core::model::SeasonState;
    match s.state {
        SeasonState::Library => Some(("In library", Tone::Green)),
        SeasonState::Partial => Some(("Partly in library", Tone::Green)),
        SeasonState::Requested => Some(("Requested", Tone::Blue)),
        _ if s.declined => Some(("Declined", Tone::Red)),
        _ => None,
    }
}

/// A season's name.
fn season_name(s: &DiscoverSeason) -> String {
    if !s.name.is_empty() {
        s.name.clone()
    } else if s.number == 0 {
        "Specials".into()
    } else {
        format!("Season {}", s.number)
    }
}

/// The season picker, open.
struct Picker {
    ticked: Vec<i64>,
    list: ListState,
    focus: bool,
}

/// What a frame's presses asked for.
enum Press {
    Intent(TitleIntent),
    Season(i64),
}

/// A title's page.
pub struct TitleScreen {
    kind: TitleKind,
    tmdb_id: i64,
    pick: bool,
    detail: Resource<DiscoverDetail>,
    /// The details shown: kept when asking again fails.
    shown: Option<DiscoverDetail>,
    poll: Poll,
    column: ColumnState,
    rows: [RowState; 2],
    backdrop: BackdropState,
    picker: Option<Picker>,
    asking: Option<Asking>,
    picker_ask: Option<Asking>,
    unwatching: Option<Unwatching>,
    overview: bool,
    sign_in: SignInAgain,
    clock: TvClock,
    in_hero: bool,
    settled: bool,
    railed: bool,
    seasons_art: Vec<Vec<ImageRequest>>,
    cast_art: Vec<Vec<ImageRequest>>,
    art_for: Option<usize>,
}

impl TitleScreen {
    pub fn new(kind: TitleKind, tmdb_id: i64, pick: bool) -> TitleScreen {
        TitleScreen {
            kind,
            tmdb_id,
            pick,
            detail: Resource::new(None, move |api| api.discover_title(kind, tmdb_id)),
            shown: None,
            poll: Poll::default(),
            column: ColumnState::default(),
            rows: [RowState::default(), RowState::default()],
            backdrop: BackdropState::default(),
            picker: None,
            asking: None,
            picker_ask: None,
            unwatching: None,
            overview: false,
            sign_in: SignInAgain::default(),
            clock: TvClock::default(),
            in_hero: true,
            settled: false,
            railed: false,
            seasons_art: Vec::new(),
            cast_art: Vec::new(),
            art_for: None,
        }
    }

    /// The focus id of the hero's main button on the page `page`.
    pub fn main_id(page: Id) -> Id {
        page.with("hero").with("main")
    }

    /// The focus id of the picker's row `index` on the page `page`.
    pub fn pick_row_id(page: Id, index: usize) -> Id {
        page.with("picker").with("list").with(index as u64)
    }

    /// The focus id of the picker's button on the page `page`.
    pub fn pick_ask_id(page: Id) -> Id {
        page.with("picker").with("foot").with("ask")
    }

    /// Whether the season picker is open.
    pub fn picking(&self) -> bool {
        self.picker.is_some()
    }

    /// The seasons ticked in the picker.
    pub fn ticked(&self) -> Option<&[i64]> {
        self.picker.as_ref().map(|p| p.ticked.as_slice())
    }

    fn key(&self) -> String {
        title_key(self.kind, self.tmdb_id)
    }

    /// Opens the picker, `start` (a season chosen) ticked too.
    fn open_picker(&mut self, d: &DiscoverDetail, start: Option<i64>) {
        let list = picker_seasons(d);
        let mut ticked = preselected(d, d.seasons_left);
        let start = start.and_then(|n| list.iter().find(|x| x.number == n).copied());
        if let Some(s) = start.filter(|s| askable(s, d.title.status))
            && !ticked.contains(&s.number)
        {
            ticked.push(s.number);
        }
        let focus_at =
            start.map(|s| s.number).or_else(|| list.iter().find(|x| askable(x, d.title.status)).map(|x| x.number));
        let index = focus_at.and_then(|n| list.iter().position(|x| x.number == n)).unwrap_or(0);
        self.picker = Some(Picker { ticked, list: ListState::at(index), focus: true });
    }

    fn collect(&mut self, cx: &mut AppCx, time: TvTime) {
        if self.asking.as_mut().and_then(|a| a.poll(cx, time)).is_some() {
            self.asking = None;
        }
        if let Some(done) = self.picker_ask.as_mut().and_then(|a| a.poll(cx, time)) {
            self.picker_ask = None;
            if done {
                self.picker = None;
            }
        }
        if self.unwatching.as_mut().and_then(|u| u.poll(cx, time)).is_some() {
            self.unwatching = None;
        }
    }

    fn rail_if(&mut self, cx: &mut AppCx, nothing: bool) {
        if nothing && !std::mem::replace(&mut self.railed, true) {
            cx.to_rail();
        }
    }
}

impl Screen for TitleScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let page = ui.scope_id();
        let time = self.clock.time(cx);
        self.collect(cx, time);
        let had_error = self.detail.error().is_some();
        if self.detail.update(cx)
            && let Some(d) = self.detail.value()
        {
            keep_title(cx, d);
            self.shown = Some(d.clone());
        }
        if self.detail.value().is_some() {
            self.shown = self.detail.value().cloned();
        }
        // What was shown stays when asking again fails: a toast says why (the shell's notice, when
        // the server doesn't answer).
        match self.detail.error() {
            Some(e) if self.shown.is_some() && !had_error && !e.unreachable() && !e.cancelled() => {
                cx.toast_error(sentence(&e.to_string()));
            }
            _ => {}
        }
        let failed = self.detail.error().is_some();
        let every = self.shown.as_ref().filter(|d| !failed && moving(&d.requests)).map(|_| 10.0);
        if self.poll.due(ui, cx, every) {
            self.detail.invalidate();
        }

        let Some(d) = self.shown.clone() else {
            match self.detail.error().cloned() {
                Some(e) => {
                    self.settled = true;
                    let why = requests_problem(&e);
                    if why == RequestsProblem::Missing {
                        let r = Message::new(MessageStyle::Item, "Not on TMDB")
                            .body("TMDB doesn't know this title, or it can't be shown here.")
                            .action("Back", Icon::ArrowLeft)
                            .show(ui, page.with("missing"));
                        if r.action.is_some_and(|a| a.activated) {
                            cx.pop();
                        }
                    } else {
                        let (pressed, focusable) =
                            problem_page(ui, cx, page.with("problem"), &why, "Couldn't load this title");
                        self.rail_if(cx, !focusable);
                        match pressed {
                            Some(ProblemAction::Retry) => self.detail.invalidate(),
                            Some(ProblemAction::SignIn) => self.sign_in.open = true,
                            Some(ProblemAction::Requests) => cx.push(Route::Requests),
                            None => {}
                        }
                    }
                }
                None => skeleton(ui),
            }
            self.sign_in.show(ui, cx, page.with("sign-in"));
            return;
        };
        self.settled = true;

        let s = cx.state.request_summary().cloned();
        let s = s.as_ref();
        let pickable = can_pick(&d, s, time);
        // Opened to pick (Request more seasons): the picker, once, over the page's main action.
        if std::mem::take(&mut self.pick) && pickable {
            self.open_picker(&d, None);
        }
        self.page_ui(ui, cx, page, &d, s, time, pickable);
        self.picker_ui(ui, cx, page, &d, s, time);
        if self.overview {
            let text = [d.tagline.clone().unwrap_or_default(), d.title.overview.clone()]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            let r = Dialog::reading(&d.title.title, &text).show(ui, page.with("overview"), true);
            if r.dismissed || r.chosen.is_some() {
                self.overview = false;
            }
        }
        self.sign_in.show(ui, cx, page.with("sign-in"));
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if self.picker.take().is_some() {
            return true;
        }
        if std::mem::take(&mut self.overview) {
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::RequestsChanged { title } if title.as_ref().is_none_or(|t| *t == self.key()) => {
                self.detail.invalidate();
            }
            AppEvent::Shown | AppEvent::Visible(true) => {
                self.clock.forget();
                self.detail.revalidate(cx.now);
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
        self.picker.is_some() || self.overview || self.sign_in.open
    }
}

impl TitleScreen {
    #[allow(clippy::too_many_arguments)]
    fn page_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppCx,
        page: Id,
        d: &DiscoverDetail,
        s: Option<&RequestSummary>,
        time: TvTime,
        pickable: bool,
    ) {
        let column_id = page.with("rows");
        let hero_id = page.with("hero");
        let t = &d.title;
        if ui.focused_in(hero_id).is_some() {
            self.in_hero = true;
        } else if ui.focused_in(column_id).is_some() {
            self.in_hero = false;
        }
        let in_hero = self.in_hero;

        // Pictures, made again when the details change.
        let seasons = if t.kind == TitleKind::Show { picker_seasons(d) } else { Vec::new() };
        let source = d as *const DiscoverDetail as usize ^ seasons.len() ^ (d.cast.len() << 16);
        if self.art_for != Some(source) {
            self.art_for = Some(source);
            let w = TileKind::Poster.size().w;
            self.seasons_art = seasons
                .iter()
                .map(|x| {
                    let mut a = art(cx, ui, x.poster_url.as_deref(), w);
                    a.extend(art(cx, ui, t.poster_url.as_deref(), w));
                    a
                })
                .collect();
            let pw = TileKind::Person.size().w;
            self.cast_art = d.cast.iter().map(|c| art(cx, ui, c.photo_url.as_deref(), pw)).collect();
        }

        let px = backdrop_px(ui);
        let backdrop: Vec<ImageRequest> = t.backdrop_url.iter().filter_map(|p| cx.image(p, px)).collect();
        let has_backdrop = !backdrop.is_empty();
        let wanted = BackdropArt::new(t.tmdb_id as u64, backdrop, t.title.clone());
        Backdrop::new(BackdropScrim::Item).show(ui, page.with("backdrop"), &mut self.backdrop, Some(&wanted));
        // No backdrop on TMDB: the poster stands at the right instead.
        if !has_backdrop && in_hero {
            let poster = art(cx, ui, t.poster_url.as_deref(), 200.0);
            if let Some(req) = poster.first() {
                let rect = Rect::new(664.0, 52.0, 200.0, 300.0);
                ui.fill(rect, theme::SURFACE2, radius::CARD);
                ui.image(rect, req, &pley_ui::ImageOpts::cover().rounded(radius::CARD));
            }
        }
        InfoBlock::show(ui, page.with("info"), !in_hero, |ui| draw_info(ui, d));

        // The hero's height, from what it says.
        let actions = Actions::of(Some(d), t, s, time);
        let words = HeroWords::of(ui, d);
        let hero_h = (HERO_PAD + words.height + 20.0 + actions.height(ui, false, TEXT_COLUMN)).max(HERO_MIN);
        let mut sections = vec![Section::hero(HERO, hero_h)];
        if !seasons.is_empty() {
            sections.push(Section::row(SEASONS, TileKind::Poster.row_height(true) + SEASON_STAGE_LINE));
        }
        if !d.cast.is_empty() {
            sections.push(Section::row(CAST, TileKind::Person.row_height(true)));
        }
        sections.push(Section::new(CREDIT, 18.0, None));

        let has_req = has_requests(s);
        let label = requests_title(s);
        let mut extras = Vec::new();
        if has_req {
            extras.push(Extra { key: "requests", icon: Icon::Inbox, label, unclaimed_only: true });
        }
        if words.cut {
            extras.push(Extra { key: "overview", icon: Icon::Ellipsis, label: "Overview", unclaimed_only: false });
        }
        // A page with nothing to focus (it can't be asked for, and no rows): the rail, so the
        // remote is never dead.
        let focusable = actions.claims() || has_req || words.cut || sections.len() > 2;
        self.rail_if(cx, !focusable);
        ui.initial_focus(TitleScreen::main_id(page));

        let manage = s.is_some_and(|s| s.manage);
        let mut press = None;
        let rows = &mut self.rows;
        let seasons_art = &self.seasons_art;
        let cast_art = &self.cast_art;
        let clip = if in_hero { None } else { Some(theme::FOCUSED_ROW_HEADER_Y - theme::ROWS_CLIP_ABOVE) };
        RowColumn::new(0.0).clip_above(clip).show(ui, column_id, &mut self.column, &sections, |ui, i, top| {
            match sections[i].key {
                HERO => {
                    if !in_hero {
                        // Faded under the info block; its buttons stay for focus to come back to.
                        let y = top + HERO_TOP + words.height + 20.0;
                        let hidden = ui.opacity(0.0, |ui| {
                            actions.show(ui, hero_id, Point::new(theme::GUTTER, y), TEXT_COLUMN, false, true, &extras)
                        });
                        if let Some(i) = hidden {
                            press = Some(Press::Intent(i));
                        }
                        return;
                    }
                    let y = words.draw(ui, d, Point::new(theme::GUTTER, top + HERO_TOP));
                    let pressed = actions.show(
                        ui,
                        hero_id,
                        Point::new(theme::GUTTER, y + 20.0),
                        TEXT_COLUMN,
                        false,
                        true,
                        &extras,
                    );
                    if let Some(i) = pressed {
                        press = Some(Press::Intent(i));
                    }
                }
                SEASONS => {
                    let r = Row::new(TileKind::Poster, top).header("Seasons").show(
                        ui,
                        ui.id("row"),
                        &mut rows[0],
                        seasons.len(),
                        |i| seasons[i].number as u64,
                        |ui, slot| {
                            let x = seasons[slot.index];
                            let request = x.request_id.and_then(|id| d.requests.iter().find(|q| q.id == id));
                            let stage = request.and_then(|q| season_stage(q, x.number, manage, time));
                            let (line, tone) = match &stage {
                                Some(w) => (w.text.clone(), Some(w.tone)),
                                None if x.state == pley_core::model::SeasonState::Unaired => {
                                    (season_line(x, time), None)
                                }
                                None if x.episodes > 0 => {
                                    (pley_core::format::pluralize(x.episodes, "episode", None), None)
                                }
                                None => (season_line(x, time), None),
                            };
                            ToneTile {
                                kind: TileKind::Poster,
                                title: &season_name(x),
                                line: &line,
                                tone,
                                lines: 2,
                                art: seasons_art.get(slot.index).map_or(&[][..], Vec::as_slice),
                                progress: stage.as_ref().and_then(|w| w.progress),
                                badge: season_badge(x),
                                news: false,
                            }
                            .paint(ui, slot);
                        },
                    );
                    if let Some(a) = r.activated {
                        let x = seasons[a];
                        if pickable && askable(x, d.title.status) {
                            press = Some(Press::Season(x.number));
                        }
                    }
                }
                CAST => {
                    Row::new(TileKind::Person, top).header("Cast").show(
                        ui,
                        ui.id("row"),
                        &mut rows[1],
                        d.cast.len(),
                        |i| i as u64,
                        |ui, slot| {
                            let c = &d.cast[slot.index];
                            let art = cast_art.get(slot.index).map_or(&[][..], Vec::as_slice);
                            Tile::new(TileKind::Person, &c.name)
                                .subtitle(c.character.as_deref().unwrap_or_default())
                                .art(art)
                                .seed(&c.name)
                                .paint(ui, slot);
                        },
                    );
                }
                _ => {
                    ui.text(
                        Point::new(theme::GUTTER, top),
                        TMDB_CREDIT,
                        &theme::CAPTION,
                        &TextOpts::line(800.0),
                        theme::SUBTLE,
                    );
                }
            }
        });

        match press {
            Some(Press::Season(n)) => self.open_picker(d, Some(n)),
            Some(Press::Intent(intent)) => match intent {
                TitleIntent::Ask(body) => {
                    if self.asking.is_none() {
                        self.asking = Some(Asking::start(cx, body));
                    }
                }
                TitleIntent::Pick => self.open_picker(d, None),
                TitleIntent::Open(id) => cx.push(Route::Item { id }),
                TitleIntent::Unwatch(r) => {
                    if self.unwatching.is_none() {
                        self.unwatching = Some(Unwatching::start(cx, *r));
                    }
                }
                TitleIntent::SignIn => self.sign_in.open = true,
                TitleIntent::Extra("requests") => cx.push(Route::Requests),
                TitleIntent::Extra(_) => self.overview = true,
            },
            None => {}
        }
    }

    /// Which seasons? (`SeasonPicker`): every season, specials last; the askable missing and partly
    /// had ones ticked as it opens, lowest first, up to what may still be asked for; seasons not
    /// aired yet and specials offered unticked; the others say what they are and can't be ticked.
    /// Ticking more than may be asked for disables the button and says why.
    fn picker_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppCx,
        page: Id,
        d: &DiscoverDetail,
        s: Option<&RequestSummary>,
        time: TvTime,
    ) {
        let id = page.with("picker");
        let list = picker_seasons(d);
        let left = d.seasons_left;
        let busy = self.picker_ask.is_some();
        let mut submit = None;
        let mut toggle = None;
        let mut focus_list = None;
        let open = self.picker.is_some();
        let picker = self.picker.as_mut();
        let ticked: Vec<i64> = picker.as_ref().map(|p| p.ticked.clone()).unwrap_or_default();
        let mut list_state = picker.as_ref().map(|p| p.list).unwrap_or_default();
        let wants_focus = picker.is_some_and(|p| std::mem::take(&mut p.focus));
        let over = left.is_some_and(|l| ticked.len() as i64 > l);
        let lines = (over || left.is_some_and(|l| l <= 0)).then(|| {
            let a = Allowance::of(Some(d), s, Some(TitleKind::Show));
            allowance_lines(&a, time).unwrap_or_default()
        });
        let panel = SidePanel::new().back_closes(false).show(ui, id, open, |ui, area| {
            let mut y = area.y;
            y += Heading::PanelTitle.show(ui, "Which seasons?", Point::new(area.x, y), area.w);
            y +=
                ui.text(
                    Point::new(area.x + 16.0, y),
                    &d.title.title,
                    &theme::CAPTION,
                    &TextOpts::line(area.w - 16.0),
                    theme::MUTED,
                )
                .h + 8.0;
            // The foot: the allowance's lines, then the button.
            let note_w = area.w - 32.0;
            let lines = lines.clone().unwrap_or_default();
            let notes_h: f32 = lines
                .iter()
                .map(|l| ui.measure(&keep_last_words(l), &theme::CAPTION, &TextOpts::wrap(note_w, 0)).h + 4.0)
                .sum();
            let foot_h = notes_h + 16.0 + theme::BUTTON_HEIGHT + 8.0;
            let view = Rect::new(area.x, y, area.w, (area.bottom() - foot_h - y).max(44.0));
            let names: Vec<String> = list.iter().map(|x| season_name(x)).collect();
            let subs: Vec<String> = list.iter().map(|x| season_line(x, time)).collect();
            let row_of = |i: usize| {
                let x = list[i];
                let can = askable(x, d.title.status);
                ListRow::new(&names[i]).supporting(&subs[i]).enabled(can).trailing(if can {
                    Trailing::Check(ticked.contains(&x.number))
                } else {
                    Trailing::None
                })
            };
            let list_id = id.with("list");
            List::new(view).edges(Edge::Stop, Edge::Pass).show(
                ui,
                list_id,
                &mut list_state,
                list.len(),
                |ui, i| row_of(i).height(ui, view.w),
                |ui, slot| {
                    if row_of(slot.index).show(ui, slot.id, slot.rect.origin(), slot.rect.w).activated {
                        toggle = Some(list[slot.index].number);
                    }
                },
            );
            let mut fy = view.bottom() + 8.0;
            for (i, l) in lines.iter().enumerate() {
                let at = Point::new(area.x + 16.0, fy);
                // More ticked than may be asked for: the line saying how many may stands out.
                let (indent, color) = if over && i == 0 {
                    ui.icon(Rect::new(at.x, at.y + 1.0, 14.0, 14.0), Icon::CircleAlert, WARNING);
                    (20.0, theme::FG)
                } else {
                    (0.0, theme::MUTED)
                };
                let opts = TextOpts::wrap(note_w - indent, 0);
                fy += ui.text(Point::new(at.x + indent, at.y), &keep_last_words(l), &theme::CAPTION, &opts, color).h
                    + 4.0;
            }
            let label = picker_button(ticked.len() as i64);
            let mut button = Button::pill(&label).enabled(!busy && !ticked.is_empty() && !over);
            if !ticked.is_empty() {
                button = button.icon(Icon::Plus);
            }
            let foot = id.with("foot");
            ui.group(foot, bar_group(), |ui| {
                if button.show(ui, ui.id("ask"), Point::new(area.x + 16.0, fy + 16.0)).activated {
                    submit = Some(());
                }
            });
            if wants_focus {
                focus_list = Some(list_id.with(list_state.index as u64));
            }
        });
        if let Some(target) = focus_list {
            ui.request_focus(target);
        }
        if let Some(p) = self.picker.as_mut() {
            p.list = list_state;
            if let Some(n) = toggle {
                if let Some(at) = p.ticked.iter().position(|x| *x == n) {
                    p.ticked.remove(at);
                } else {
                    p.ticked.push(n);
                }
            }
        }
        if panel.closed {
            self.picker = None;
        }
        if submit.is_some()
            && !busy
            && let Some(p) = &self.picker
            && !p.ticked.is_empty()
            && !over
        {
            let mut seasons = p.ticked.clone();
            seasons.sort_unstable();
            let body = NewRequest { kind: TitleKind::Show, tmdb_id: d.title.tmdb_id, seasons: Some(seasons) };
            self.picker_ask = Some(Asking::start(cx, body));
        }
    }
}

/// What the hero says above its buttons, measured once a frame.
struct HeroWords {
    height: f32,
    /// The overview is longer than its three lines: Overview shows it whole.
    cut: bool,
}

impl HeroWords {
    fn facts(d: &DiscoverDetail) -> String {
        let genres = d.genres.join(" · ");
        [d.network.clone().unwrap_or_default(), genres]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    fn of(ui: &Ui, d: &DiscoverDetail) -> HeroWords {
        let t = &d.title;
        let mut h = ui.measure(&t.title, &theme::HERO_TITLE, &TextOpts::wrap(TEXT_COLUMN, 2)).h;
        h += 12.0 + 18.0;
        if !HeroWords::facts(d).is_empty() {
            h += 6.0 + theme::CAPTION.line_height;
        }
        if d.tagline.as_deref().is_some_and(|t| !t.is_empty()) {
            h += 10.0 + theme::BODY.line_height;
        }
        let mut cut = false;
        if !t.overview.is_empty() {
            let whole = ui.measure(&t.overview, &theme::BODY_LARGE, &TextOpts::wrap(TEXT_COLUMN, 0)).h;
            let shown = ui.measure(&t.overview, &theme::BODY_LARGE, &TextOpts::wrap(TEXT_COLUMN, OVERVIEW_LINES)).h;
            cut = whole > shown + 1.0;
            h += 10.0 + shown;
        }
        HeroWords { height: h, cut }
    }

    /// Draws them from `at`; where they end.
    fn draw(&self, ui: &mut Ui, d: &DiscoverDetail, at: Point) -> f32 {
        let t = &d.title;
        let x = at.x;
        let mut y = at.y;
        y += ui.text(Point::new(x, y), &t.title, &theme::HERO_TITLE, &TextOpts::wrap(TEXT_COLUMN, 2), theme::FG).h;
        y += 12.0;
        title_meta(ui, Point::new(x, y), t, Some(d), TEXT_COLUMN);
        y += 18.0;
        let facts = HeroWords::facts(d);
        if !facts.is_empty() {
            y += 6.0;
            y += ui.text(Point::new(x, y), &facts, &theme::CAPTION, &TextOpts::line(TEXT_COLUMN), theme::MUTED).h;
        }
        if let Some(tagline) = d.tagline.as_deref().filter(|t| !t.is_empty()) {
            y += 10.0;
            y += ui.text(Point::new(x, y), tagline, &theme::BODY, &TextOpts::line(TEXT_COLUMN), theme::FG).h;
        }
        if !t.overview.is_empty() {
            y += 10.0;
            let opts = TextOpts::wrap(TEXT_COLUMN, OVERVIEW_LINES);
            y += ui.text(Point::new(x, y), &t.overview, &theme::BODY_LARGE, &opts, theme::SUMMARY).h;
        }
        y
    }
}

/// The block over the rows once focus leaves the hero: the title, its facts, the overview.
fn draw_info(ui: &mut Ui, d: &DiscoverDetail) {
    let r = InfoBlock::RECT;
    let t = &d.title;
    let mut y = r.y;
    y += ui.text(Point::new(r.x, y), &t.title, &theme::HERO_TITLE, &TextOpts::line(r.w), theme::FG).h + 10.0;
    title_meta(ui, Point::new(r.x, y), t, Some(d), r.w);
    y += 18.0 + 8.0;
    if !t.overview.is_empty() {
        ui.text(Point::new(r.x, y), &t.overview, &theme::BODY, &TextOpts::wrap(r.w, 2), theme::SUMMARY);
    }
}

fn skeleton(ui: &mut Ui) {
    let b = |ui: &mut Ui, y: f32, w: f32, h: f32, r: f32| skeleton_block(ui, Rect::new(theme::GUTTER, y, w, h), r);
    b(ui, 82.0, 320.0, 40.0, radius::BADGE);
    b(ui, 136.0, 220.0, 18.0, radius::BADGE);
    b(ui, 166.0, 160.0, 14.0, radius::BADGE);
    b(ui, 196.0, 464.0, 15.0, radius::BADGE);
    b(ui, 218.0, 440.0, 15.0, radius::BADGE);
    b(ui, 240.0, 300.0, 15.0, radius::BADGE);
    b(ui, 282.0, 140.0, 40.0, radius::CONTROL);
}
