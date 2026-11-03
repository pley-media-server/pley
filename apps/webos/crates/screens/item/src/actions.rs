//! A title's actions (Android's `ItemActions.kt`, `TvTileActions.kt`, `TvItemActionsScreen.kt`;
//! the TV app's `actions.tsx`): the list, what each does, the side panel over the page when a tile
//! is held or the menu key pressed on it, and the page that shows them when opened by address. On
//! a title's page the same list shows in its own side panel (`crate::item`), without My List and
//! Continue Watching.

use pley_base::Icon;
use pley_core::model::{ItemDetail, ItemKind, ItemSummary};
use pley_core::store::snapshot_keys;
use pley_core::{ApiError, Promise};
use pley_kit::{
    Action, ActionSheet, Backdrop, BackdropArt, BackdropScrim, BackdropState, Heading, List, ListRow, ListState,
    SidePanel, backdrop_px,
};
use pley_shell::{AppCx, AppEvent, Chrome, Load, Overlay, PlayRequest, Resource, Route, Screen};
use pley_ui::Ui;

use crate::common::{self, ArtKind, Change, NOTHING_TO_PLAY};
use crate::page;
use crate::words;

/// One of a title's actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemAction {
    /// An artist, album or track: its page.
    Open,
    Resume,
    /// "Play from start".
    FromStart,
    Play,
    MarkWatched,
    MarkUnwatched,
    RemoveFromContinueWatching,
    AddToMyList,
    RemoveFromMyList,
    AddToPlaylist,
    GoToShow,
    GoToSeason,
}

impl ItemAction {
    /// What its row says.
    pub fn label(self) -> &'static str {
        match self {
            ItemAction::Open => "Open",
            ItemAction::Resume => "Resume",
            ItemAction::FromStart => "Play from start",
            ItemAction::Play => "Play",
            ItemAction::MarkWatched => "Mark as watched",
            ItemAction::MarkUnwatched => "Mark as unwatched",
            ItemAction::RemoveFromContinueWatching => "Remove from Continue Watching",
            ItemAction::AddToMyList => "Add to My List",
            ItemAction::RemoveFromMyList => "Remove from My List",
            ItemAction::AddToPlaylist => "Add to playlist",
            ItemAction::GoToShow => "Go to show",
            ItemAction::GoToSeason => "Go to season",
        }
    }

    /// Its icon (`TvPanels.kt`).
    pub fn icon(self) -> Icon {
        match self {
            ItemAction::Open => Icon::ArrowRight,
            ItemAction::Resume | ItemAction::Play => Icon::Play,
            ItemAction::FromStart => Icon::RotateCcw,
            ItemAction::MarkWatched => Icon::CircleCheck,
            ItemAction::MarkUnwatched => Icon::EyeOff,
            ItemAction::RemoveFromContinueWatching => Icon::X,
            ItemAction::AddToMyList => Icon::BookmarkPlus,
            ItemAction::RemoveFromMyList => Icon::BookmarkMinus,
            ItemAction::AddToPlaylist => Icon::ListPlus,
            ItemAction::GoToShow => Icon::Tv,
            ItemAction::GoToSeason => Icon::Layers,
        }
    }
}

/// A title's actions, in order (`itemActions`, `ItemActions.kt:67-89`): music opens its page; in
/// progress, Resume and Play from start, else Play; watched or not; out of Continue Watching when
/// opened from it; My List (movies and shows, when known); a playlist (not extras); its show and
/// season. `in_panel`: the title's page's panel, which leaves out Continue Watching and My List.
pub fn item_actions(
    item: &ItemSummary,
    in_my_list: Option<bool>,
    continue_watching: bool,
    in_panel: bool,
) -> Vec<ItemAction> {
    let mut out = Vec::new();
    match item.kind {
        ItemKind::Artist | ItemKind::Album | ItemKind::Track => out.push(ItemAction::Open),
        ItemKind::Unknown => {}
        _ if page::in_progress(item) => out.extend([ItemAction::Resume, ItemAction::FromStart]),
        _ => out.push(ItemAction::Play),
    }
    if matches!(item.kind, ItemKind::Movie | ItemKind::Episode | ItemKind::Show | ItemKind::Season) {
        out.push(if page::watched(item) { ItemAction::MarkUnwatched } else { ItemAction::MarkWatched });
    }
    if continue_watching && !in_panel {
        out.push(ItemAction::RemoveFromContinueWatching);
    }
    if item.kind.listable()
        && !in_panel
        && let Some(listed) = in_my_list
    {
        out.push(if listed { ItemAction::RemoveFromMyList } else { ItemAction::AddToMyList });
    }
    if !matches!(item.kind, ItemKind::Extra | ItemKind::Unknown) {
        out.push(ItemAction::AddToPlaylist);
    }
    let show = match item.kind {
        ItemKind::Episode => item.grandparent_id,
        ItemKind::Season => item.parent_id,
        _ => None,
    };
    if show.is_some() {
        out.push(ItemAction::GoToShow);
    }
    if item.kind == ItemKind::Episode && item.parent_id.is_some() {
        out.push(ItemAction::GoToSeason);
    }
    out
}

/// What choosing an action comes to.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// A page to open.
    Open(Route),
    /// A video to play.
    Play(PlayRequest),
    /// A show or season plays its next episode, which the server names.
    PlayNext { id: i64, from_start: bool },
    /// A change on the server.
    Change(Change),
}

/// What `action` does to `item`.
pub fn effect(action: ItemAction, item: &ItemSummary) -> Effect {
    let play = |from_start| match item.kind {
        ItemKind::Show | ItemKind::Season => Effect::PlayNext { id: item.id, from_start },
        ItemKind::Movie | ItemKind::Episode | ItemKind::Extra => {
            Effect::Play(common::play_request(item.id, from_start))
        }
        _ => Effect::Open(common::page_of(item)),
    };
    match action {
        ItemAction::Open => Effect::Open(common::page_of(item)),
        ItemAction::Resume | ItemAction::Play => play(false),
        ItemAction::FromStart => play(true),
        ItemAction::MarkWatched => Effect::Change(Change::Watched { id: item.id, watched: true }),
        ItemAction::MarkUnwatched => Effect::Change(Change::Watched { id: item.id, watched: false }),
        ItemAction::RemoveFromContinueWatching => Effect::Change(Change::HideFromContinueWatching { id: item.id }),
        ItemAction::AddToMyList => Effect::Change(Change::MyList { id: item.id, on: true }),
        ItemAction::RemoveFromMyList => Effect::Change(Change::MyList { id: item.id, on: false }),
        ItemAction::AddToPlaylist => {
            Effect::Open(Route::AddToPlaylist { item_ids: vec![item.id], title: words::actions_title(item) })
        }
        ItemAction::GoToShow => {
            let show = if item.kind == ItemKind::Season { item.parent_id } else { item.grandparent_id };
            Effect::Open(Route::Item { id: show.unwrap_or(item.id) })
        }
        ItemAction::GoToSeason => Effect::Open(Route::Item { id: item.parent_id.unwrap_or(item.id) }),
    }
}

/// A tile's actions in a side panel over the page they were asked on (Android's
/// `TvTileActionsPanel`, the Samsung app's `TileActions.cs`): from Home, a library's grid and
/// Recommended, Collections, search, a person's and an artist's page. The panel slides in once the
/// title's details are in (the page keeps focus meanwhile): its title, then one row per action
/// with its icon, the first focused. Back closes it. A choice closes it and runs the frame after,
/// once focus is back on the tile, so a page opened over it comes back to the tile and a tile the
/// choice takes away hands focus on as in its row. A failure to load, and what a choice comes to
/// (nothing to play, a show's next episode), show after the panel has gone.
pub struct TileActions {
    continue_watching: bool,
    detail: Resource<ItemDetail>,
    seed: Option<ItemDetail>,
    /// What the panel shows, made once when the title's details come: it shows what they were
    /// when it opened.
    shown: Option<Shown>,
    open: bool,
    list: ListState,
    /// Chosen in the panel: runs the frame after it closed.
    chosen: Option<ItemAction>,
    /// A show's or season's next episode, asked once Play was chosen.
    next: Option<NextEpisode>,
}

/// A show's or season's next episode on its way, to play from the start or where it was left.
struct NextEpisode {
    answer: Promise<Result<Option<ItemSummary>, ApiError>>,
    from_start: bool,
}

/// The title a tile's actions are of, and what the panel lists.
struct Shown {
    item: ItemSummary,
    no_files: bool,
    title: String,
    actions: Vec<ItemAction>,
}

impl TileActions {
    /// The actions of item `id`; `continue_watching`: its tile is in that row.
    pub fn new(id: i64, continue_watching: bool, cx: &mut AppCx) -> TileActions {
        TileActions {
            continue_watching,
            detail: Resource::new(Some(snapshot_keys::item(id)), move |api| api.item(id)),
            seed: cx.state.cached_item(id).cloned(),
            shown: None,
            open: false,
            list: ListState::at(1),
            chosen: None,
            next: None,
        }
    }

    /// Makes what the panel shows once the title is known; whether there is anything to show
    /// (`None` while it loads).
    fn show_when_known(&mut self, cx: &mut AppCx) -> Option<bool> {
        if self.shown.is_some() {
            return Some(true);
        }
        self.detail.update(cx);
        let d = self.detail.value().or(self.seed.as_ref())?;
        let item = d.summary.clone();
        let actions = item_actions(&item, Some(d.in_watchlist), self.continue_watching, false);
        if actions.is_empty() {
            return Some(false);
        }
        let title = words::actions_title(&item);
        self.shown = Some(Shown { no_files: d.files.is_empty(), title, actions, item });
        self.open = true;
        Some(true)
    }

    fn act(&mut self, action: ItemAction, cx: &mut AppCx) {
        let Some(shown) = &self.shown else { return };
        match effect(action, &shown.item) {
            Effect::Open(route) => cx.push(route),
            Effect::Play(_) if shown.no_files => cx.toast(NOTHING_TO_PLAY),
            Effect::Play(request) => cx.play(request),
            Effect::PlayNext { id, from_start } => {
                self.next = Some(NextEpisode { answer: cx.spawn(move |api| api.on_deck(id)), from_start });
            }
            Effect::Change(change) => change.detach(cx),
        }
    }

    /// Draws the panel: its title, then a row per action. The action chosen, and how far in the
    /// panel is.
    fn panel(&mut self, ui: &mut Ui) -> (Option<ItemAction>, f32) {
        let Some(shown) = &self.shown else { return (None, 0.0) };
        let root = ui.scope_id();
        let list_id = root.with("list");
        let (state, open) = (&mut self.list, self.open);
        let mut chosen = None;
        let r = SidePanel::new().show(ui, root.with("panel"), open, |ui, area| {
            let row = |a: ItemAction| ListRow::new(a.label()).icon(a.icon());
            let height = |ui: &Ui, i: usize| match i {
                0 => Heading::PanelTitle.height(ui, &shown.title, area.w),
                _ => row(shown.actions[i - 1]).height(ui, area.w),
            };
            List::new(area).show(ui, list_id, state, shown.actions.len() + 1, height, |ui, slot| {
                if slot.index == 0 {
                    Heading::PanelTitle.show(ui, &shown.title, slot.rect.origin(), slot.rect.w);
                    return;
                }
                if slot.index == 1 {
                    ui.initial_focus(slot.id);
                }
                let action = shown.actions[slot.index - 1];
                if row(action).show(ui, slot.id, slot.rect.origin(), slot.rect.w).activated {
                    chosen = Some(action);
                }
            });
        });
        if r.closed {
            self.open = false;
        }
        (chosen.filter(|_| open), r.progress)
    }

    /// A show's next episode came: it plays, or why not is said.
    fn play_next(&mut self, cx: &mut AppCx) {
        let Some(NextEpisode { answer, from_start }) = self.next.as_mut() else { return };
        if !(answer.ready() || answer.failed()) {
            return;
        }
        let from_start = *from_start;
        let next = answer.take();
        self.next = None;
        match next {
            Some(Ok(Some(e))) => cx.play(common::play_request(e.id, from_start)),
            Some(Ok(None)) => cx.toast(NOTHING_TO_PLAY),
            Some(Err(e)) => cx.toast_error(e.problem()),
            None => cx.toast_error("The request stopped."),
        }
    }
}

impl Overlay for TileActions {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) -> bool {
        match self.show_when_known(cx) {
            Some(true) => {}
            Some(false) => return false,
            None => {
                if let Load::Failed { error, .. } = self.detail.state() {
                    cx.toast_error(error.problem());
                    return false;
                }
                return true;
            }
        }
        // Chosen last frame: the panel has closed, and focus is back on the tile when this frame
        // ends, before a page this opens keeps where the page's focus was.
        if let Some(action) = self.chosen.take() {
            self.act(action, cx);
        }
        let (chosen, progress) = self.panel(ui);
        if let Some(action) = chosen {
            self.open = false;
            self.chosen = Some(action);
            ui.request_repaint();
        }
        self.play_next(cx);
        self.open || progress > 0.0 || self.chosen.is_some() || self.next.is_some()
    }

    fn modal(&self) -> bool {
        self.open
    }
}

/// A title's actions as a page of their own, opened by address (`TvItemActionsScreen`): the title's backdrop, at x 96,
/// y 58, its name, the line under it, and one row per action, the first focused. A row closes the
/// page, then acts; a change on the server carries on after it.
pub struct ItemActionsScreen {
    id: i64,
    continue_watching: bool,
    detail: Resource<ItemDetail>,
    seed: Option<ItemDetail>,
    /// What the page shows, made when the title's details come (not every frame).
    sheet: Option<Sheet>,
    /// A show's or season's next episode, asked as soon as the title is known, for Play.
    next: Option<Promise<Result<Option<ItemSummary>, ApiError>>>,
    /// The server's answer: the episode, none, or why it couldn't say.
    next_known: Option<Result<Option<ItemSummary>, String>>,
    /// Play was chosen before the next episode was known.
    waiting: Option<bool>,
    backdrop: BackdropState,
}

/// The title the actions are of, and what the page says of it.
struct Sheet {
    item: ItemSummary,
    no_files: bool,
    title: String,
    subtitle: String,
    actions: Vec<ItemAction>,
    rows: Vec<Action<'static>>,
    art: BackdropArt,
}

impl ItemActionsScreen {
    /// The actions of item `id`; `continue_watching`: opened from that row.
    pub fn new(id: i64, continue_watching: bool, cx: &mut AppCx) -> ItemActionsScreen {
        ItemActionsScreen {
            id,
            continue_watching,
            detail: Resource::new(Some(snapshot_keys::item(id)), move |api| api.item(id)),
            seed: cx.state.cached_item(id).cloned(),
            sheet: None,
            next: None,
            next_known: None,
            waiting: None,
            backdrop: BackdropState::default(),
        }
    }

    fn item(&self) -> Option<&ItemDetail> {
        self.detail.value().or(self.seed.as_ref())
    }

    fn update(&mut self, ui: &Ui, cx: &mut AppCx) {
        let changed = self.detail.update(cx);
        if (changed || self.sheet.is_none())
            && let Some(d) = self.item()
        {
            self.sheet = Some(self.sheet_of(d, ui, cx));
        }
        let Some(kind) = self.item().map(|d| d.kind) else { return };
        if matches!(kind, ItemKind::Show | ItemKind::Season) && self.next.is_none() && self.next_known.is_none() {
            let id = self.id;
            self.next = Some(cx.spawn(move |api| api.on_deck(id)));
        }
        if let Some(p) = self.next.as_mut()
            && (p.ready() || p.failed())
        {
            self.next_known = match p.take() {
                Some(Ok(next)) => Some(Ok(next)),
                Some(Err(e)) => Some(Err(e.problem())),
                None => Some(Err("The request stopped.".to_string())),
            };
            self.next = None;
        }
        if let (Some(from_start), Some(next)) = (self.waiting, self.next_known.clone()) {
            self.waiting = None;
            play_next(cx, next, from_start);
        }
    }

    fn sheet_of(&self, d: &ItemDetail, ui: &Ui, cx: &AppCx) -> Sheet {
        let item = d.summary.clone();
        let actions = item_actions(&item, Some(d.in_watchlist), self.continue_watching, false);
        // Only Play and Resume carry an icon here (`TvItemActionsScreen`).
        let rows = actions
            .iter()
            .map(|a| match a {
                ItemAction::Play | ItemAction::Resume => Action::new(a.label()).icon(Icon::Play),
                _ => Action::new(a.label()),
            })
            .collect();
        let title = words::actions_title(&item);
        let art = common::requests(cx, &common::art_paths(&item, ArtKind::Backdrop), backdrop_px(ui));
        Sheet {
            no_files: d.files.is_empty(),
            subtitle: words::actions_subtitle(&item),
            art: BackdropArt::new(item.id as u64, art, title.clone()),
            title,
            actions,
            rows,
            item,
        }
    }

    fn act(&mut self, action: ItemAction, cx: &mut AppCx) {
        let Some(sheet) = &self.sheet else { return };
        let (item, no_files) = (&sheet.item, sheet.no_files);
        match effect(action, item) {
            Effect::Open(route) => {
                cx.pop();
                cx.push(route);
            }
            Effect::Play(_) if no_files => {
                cx.pop();
                cx.toast(NOTHING_TO_PLAY);
            }
            Effect::Play(request) => {
                cx.pop();
                cx.play(request);
            }
            Effect::PlayNext { from_start, .. } => match self.next_known.clone() {
                Some(next) => play_next(cx, next, from_start),
                None => self.waiting = Some(from_start),
            },
            Effect::Change(change) => {
                cx.pop();
                change.detach(cx);
            }
        }
    }
}

/// Closes the actions page and plays `next`, or says there is nothing to play, or why the server
/// couldn't say.
fn play_next(cx: &mut AppCx, next: Result<Option<ItemSummary>, String>, from_start: bool) {
    cx.pop();
    match next {
        Ok(Some(e)) => cx.play(common::play_request(e.id, from_start)),
        Ok(None) => cx.toast(NOTHING_TO_PLAY),
        Err(why) => cx.toast_error(why),
    }
}

impl Screen for ItemActionsScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.update(ui, cx);
        let root = ui.scope_id();
        let Some(sheet) = &self.sheet else {
            if let Load::Failed { error, .. } = self.detail.state() {
                let title = common::failed_title(error, "Couldn't load this item");
                let body = error.problem();
                if common::message(ui, root.with("message"), Icon::CircleAlert, title, &body, "Back").activated {
                    cx.back();
                }
            }
            return;
        };
        Backdrop::new(BackdropScrim::Item).show(ui, root.with("backdrop"), &mut self.backdrop, Some(&sheet.art));
        let chosen =
            ActionSheet::new(&sheet.title, &sheet.rows).subtitle(&sheet.subtitle).show(ui, root.with("actions"));
        if let Some(action) = chosen.and_then(|i| sheet.actions.get(i).copied())
            && self.waiting.is_none()
        {
            self.act(action, cx);
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        if let AppEvent::Shown = event {
            self.detail.revalidate(cx.now);
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::UserState;

    fn item(kind: ItemKind) -> ItemSummary {
        ItemSummary { id: 5, kind, title: "Office Olympics".into(), ..ItemSummary::default() }
    }

    #[test]
    fn a_movie_offers_play_watched_my_list_and_a_playlist() {
        let movie = item(ItemKind::Movie);
        assert_eq!(
            item_actions(&movie, Some(false), false, false),
            [ItemAction::Play, ItemAction::MarkWatched, ItemAction::AddToMyList, ItemAction::AddToPlaylist]
        );
        // My List only when it is known, and never in the page's panel.
        assert_eq!(item_actions(&movie, None, false, false).len(), 3);
        assert_eq!(item_actions(&movie, Some(true), false, true).len(), 3);
    }

    #[test]
    fn an_episode_in_progress_resumes_and_leads_to_its_show() {
        let episode = ItemSummary {
            parent_id: Some(4),
            grandparent_id: Some(3),
            duration_ms: Some(1_000_000),
            user_state: UserState { position_ms: 400_000, ..UserState::default() },
            ..item(ItemKind::Episode)
        };
        assert_eq!(
            item_actions(&episode, None, true, false),
            [
                ItemAction::Resume,
                ItemAction::FromStart,
                ItemAction::MarkWatched,
                ItemAction::RemoveFromContinueWatching,
                ItemAction::AddToPlaylist,
                ItemAction::GoToShow,
                ItemAction::GoToSeason,
            ]
        );
        assert_eq!(effect(ItemAction::GoToShow, &episode), Effect::Open(Route::Item { id: 3 }));
        assert_eq!(effect(ItemAction::GoToSeason, &episode), Effect::Open(Route::Item { id: 4 }));
        assert_eq!(effect(ItemAction::FromStart, &episode), Effect::Play(common::play_request(5, true)));
        assert_eq!(
            effect(ItemAction::RemoveFromContinueWatching, &episode),
            Effect::Change(Change::HideFromContinueWatching { id: 5 })
        );
    }

    #[test]
    fn a_watched_season_offers_unwatched_and_its_show() {
        let season = ItemSummary {
            parent_id: Some(3),
            leaf_count: Some(8),
            viewed_leaf_count: Some(8),
            ..item(ItemKind::Season)
        };
        let list = item_actions(&season, None, false, false);
        assert_eq!(
            list,
            [ItemAction::Play, ItemAction::MarkUnwatched, ItemAction::AddToPlaylist, ItemAction::GoToShow]
        );
        assert_eq!(effect(ItemAction::Play, &season), Effect::PlayNext { id: 5, from_start: false });
        assert_eq!(effect(ItemAction::GoToShow, &season), Effect::Open(Route::Item { id: 3 }));
    }

    #[test]
    fn music_opens_its_page_and_extras_take_no_playlist() {
        let album = item(ItemKind::Album);
        assert_eq!(item_actions(&album, None, false, false), [ItemAction::Open, ItemAction::AddToPlaylist]);
        assert_eq!(effect(ItemAction::Open, &album), Effect::Open(Route::Album { id: 5, track_id: None }));
        assert_eq!(item_actions(&item(ItemKind::Extra), None, false, false), [ItemAction::Play]);
    }

    #[test]
    fn a_playlist_is_named_after_the_show() {
        let episode = ItemSummary { grandparent_title: Some("The Office".into()), ..item(ItemKind::Episode) };
        assert_eq!(
            effect(ItemAction::AddToPlaylist, &episode),
            Effect::Open(Route::AddToPlaylist { item_ids: vec![5], title: "The Office".into() })
        );
    }
}
