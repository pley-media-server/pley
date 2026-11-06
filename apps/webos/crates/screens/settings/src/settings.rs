//! Settings (`TvSettingsScreen.kt`, `TvSettingsKit.kt`; the TV app's `settings.tsx`): "Settings"
//! at y 27, the categories at x 96 (256 wide, from y 81), and the focused category's rows in the
//! pane at x 376 to 912. Focus on a category shows its rows; Right or OK goes to its first row
//! (OK on Sign out asks at once). Choices open a side panel that saves and closes on a pick;
//! switches flip on OK; Sign out and "Delete all local data" ask first; "Manage your account on
//! the web" shows the address and its QR code.
//!
//! What the panes show is gathered when what it comes from changes (the settings, who is
//! watching, the household, a save's answer), not each frame; the pane's rows and their heights
//! are made once per change, and an open panel's choices once.

use std::sync::Arc;

use pley_base::{Family, ImageRequest, Key, Point, Rect, TextOpts, TextStyle};
use pley_core::http::sentence;
use pley_core::model::{AppProfile, Me, TrackDefaults};
use pley_core::store::SubtitleFont;
use pley_core::{ApiError, Promise};
use pley_i18n::{Language, t};
use pley_kit::{Dialog, DialogButton, Heading, List, ListRow, ListState, ProfileTile, SidePanel, Trailing, is_inert};
use pley_shell::{AppCx, AppEvent, Chrome, Route, Screen, SettingsSection, avatar_color};
use pley_ui::theme;
use pley_ui::{Dirs, Edge, Group, Id, Ui};

use crate::notices;
use crate::pane::{self, End, Facts, Item, Panel, Picking, ProfileFacts, RowAction, Value};
use crate::web::WebDialog;

/// The categories' column.
const CATEGORIES_X: f32 = theme::GUTTER;
const CATEGORIES_W: f32 = 256.0;
/// Where the categories and the pane start, under the title.
const TOP: f32 = 81.0;
/// The pane: x 376 to 912.
const PANE_X: f32 = 376.0;
const PANE_W: f32 = theme::CANVAS_W - theme::SAFE_X - PANE_X;
/// Between rows, in the column and the pane.
const ROW_GAP: f32 = 2.0;
/// The profile header's avatar.
const AVATAR: f32 = 48.0;
/// The device facts' label column (`DiagnosticsViewModel.deviceFacts`' table).
const FACT_LABEL: f32 = 150.0;
/// A pane's paragraph (Audio's, Sign out's): 20 in, 8 above the rows (`TvSettingsScreen.kt`).
const INTRO_X: f32 = 20.0;
const INTRO_BELOW: f32 = 8.0;

/// A question Settings asks before doing something it can't undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Confirm {
    SignOut,
    DeleteData,
}

/// The pane as drawn: its items and their heights, for the category and the facts they were
/// made from.
struct PaneCache {
    section: SettingsSection,
    revision: u64,
    items: Vec<Item>,
    /// Each item's height, then the room under the last.
    heights: Vec<f32>,
}

/// The open panel as drawn: its choices and their heights at the panel's width.
struct PanelCache {
    picking: Picking,
    revision: u64,
    width: f32,
    panel: Panel,
    heights: Vec<f32>,
}

/// A change of the account's language the server is keeping.
struct SavingLanguage {
    answer: Promise<Result<Me, ApiError>>,
    /// The language chosen; none: Automatic.
    language: Option<Language>,
    /// The server it went to.
    server: Option<String>,
}

/// What doesn't change while Settings shows: the TV's facts and the app's version.
struct Fixed {
    device: Vec<(String, String)>,
    version: String,
}

/// The Settings page.
pub struct SettingsScreen {
    shown: SettingsSection,
    /// Each category's pane, so each keeps its place.
    panes: [ListState; 8],
    /// Focus goes into this category's first row this frame.
    into: Option<SettingsSection>,
    panel: Option<Picking>,
    /// What the panel shows while it slides out.
    panel_drawn: Option<Picking>,
    panel_list: ListState,
    panel_cache: Option<PanelCache>,
    confirm: Option<Confirm>,
    confirm_drawn: Confirm,
    /// The question of signing out closed without signing out: focus goes to its category.
    to_sign_out: bool,
    web: WebDialog,
    /// The change of the defaults the server is keeping.
    saving: Option<Promise<Result<Me, ApiError>>>,
    saving_language: Option<SavingLanguage>,
    /// A change made while another was being kept: sent once that one is (`MeViewModel.save`
    /// keeps one request at a time).
    queued: Option<TrackDefaults>,
    /// The defaults as they show while the server keeps them (shown at once, put back on failure).
    defaults: Option<TrackDefaults>,
    /// The server refused to change the defaults in this visit.
    refused: bool,
    /// The subtitle faces this TV draws differently from the default, measured once.
    fonts: Option<Vec<SubtitleFont>>,
    /// What the panes show, and how many times it has changed.
    facts: Arc<Facts>,
    revision: u64,
    /// What `facts` was gathered from: the sign-in, who is watching, the household as listed.
    generation: Option<u64>,
    me: Option<Me>,
    profiles: Option<Vec<AppProfile>>,
    /// The household as last listed, read from the store off the main thread.
    household_read: Option<Promise<Vec<AppProfile>>>,
    saved_household: Vec<AppProfile>,
    /// NOTICES.txt's parts, once read (an empty list when the app carries none).
    licences: Option<Vec<(String, String)>>,
    fixed: Option<Fixed>,
    /// Something of this page's own changed (a pick, a save's answer): gather the facts again.
    dirty: bool,
    pane: Option<PaneCache>,
}

impl SettingsScreen {
    /// Settings on `section` (Profile when none).
    pub fn new(section: Option<SettingsSection>) -> SettingsScreen {
        SettingsScreen {
            shown: section.unwrap_or(SettingsSection::Profile),
            panes: [ListState::default(); 8],
            into: None,
            panel: None,
            panel_drawn: None,
            panel_list: ListState::default(),
            panel_cache: None,
            confirm: None,
            confirm_drawn: Confirm::SignOut,
            to_sign_out: false,
            web: WebDialog::default(),
            saving: None,
            saving_language: None,
            queued: None,
            defaults: None,
            refused: false,
            fonts: None,
            facts: Arc::new(Facts::default()),
            revision: 0,
            generation: None,
            me: None,
            profiles: None,
            household_read: None,
            saved_household: Vec::new(),
            licences: None,
            fixed: None,
            dirty: false,
            pane: None,
        }
    }

    /// The category shown in the pane.
    pub fn shown(&self) -> SettingsSection {
        self.shown
    }

    fn slot(c: SettingsSection) -> usize {
        SettingsSection::ALL.iter().position(|x| *x == c).unwrap_or(0)
    }

    /// The id of category `c`'s row, under the page's `root`.
    pub fn category_id(root: Id, c: SettingsSection) -> Id {
        root.with("categories").with(c.as_str())
    }

    /// The id of row `index` of category `c`'s pane, under the page's `root`.
    pub fn row_id(root: Id, c: SettingsSection, index: usize) -> Id {
        SettingsScreen::pane_id(root, c).with(index as u64)
    }

    fn pane_id(root: Id, c: SettingsSection) -> Id {
        root.with("pane").with(c.as_str())
    }

    /// The id of choice `index` of the open panel, under the page's `root`.
    pub fn choice_id(root: Id, index: usize) -> Id {
        root.with("panel").with("choices").with(index as u64)
    }

    /// Gathers what the panes show again when what it comes from changed since the last frame.
    fn gather(&mut self, ui: &Ui, cx: &mut AppCx) {
        if self.fonts.is_none() {
            self.fonts = Some(fonts_here(ui));
        }
        let mut stale = std::mem::take(&mut self.dirty);
        let generation = cx.state.generation();
        if self.generation != Some(generation) {
            // First drawn, or another sign-in: what doesn't change while the page shows, and the
            // household as last listed, read off the main thread while the server is asked.
            self.generation = Some(generation);
            let info = cx.device().info();
            self.fixed = Some(Fixed {
                device: device_facts(&info, cx.device().tier()),
                version: pane::build_label(&info.app_version, pane::COMMIT),
            });
            self.saved_household.clear();
            self.household_read = match cx.state.server() {
                Some(server) if cx.state.profiles().is_none() => {
                    let (store, server) = (cx.store().clone(), server.to_string());
                    Some(cx.tasks().spawn(move || store.household(&server)))
                }
                _ => None,
            };
            stale = true;
        }
        if let Some(p) = self.household_read.as_mut()
            && (p.ready() || p.failed())
        {
            self.saved_household = p.take().unwrap_or_default();
            self.household_read = None;
            stale = true;
        }
        if self.shown == SettingsSection::About
            && self.licences.is_none()
            && let Some(read) = notices::shared(cx.tasks())
        {
            self.licences = Some(read.map(|n| n.list()).unwrap_or_default());
            stale = true;
        }
        let state = &*cx.state;
        if self.me.as_ref() != state.me() {
            self.me = state.me().cloned();
            // Once who is watching says the same as the change kept, what shows comes from there.
            if self.saving.is_none()
                && self.defaults.is_some()
                && self.me.as_ref().map(Me::track_defaults) == self.defaults
            {
                self.defaults = None;
            }
            stale = true;
        }
        if self.profiles.as_deref() != state.profiles() {
            self.profiles = state.profiles().map(<[AppProfile]>::to_vec);
            stale = true;
        }
        stale |= self.facts.settings != *state.settings();
        let on_device = !state.keeps_language();
        let automatic = if on_device { state.automatic_language() } else { state.account_automatic_language() };
        stale |= self.facts.automatic_language != automatic || self.facts.language_on_device != on_device;
        stale |= on_device && self.facts.language != state.language_choice();
        if !stale {
            return;
        }
        let me = self.me.as_ref();
        let scopes: Vec<String> = match state.tokens() {
            Some(t) => t.scopes(),
            None => me.map(|m| m.scopes().into_iter().map(str::to_string).collect()).unwrap_or_default(),
        };
        let household = self.profiles.as_deref().unwrap_or(&self.saved_household);
        let fixed = self.fixed.as_ref();
        self.facts = Arc::new(Facts {
            profile: me.map(|m| ProfileFacts {
                name: m.shown_name().to_string(),
                managed: m.managed,
                avatar_url: m.avatar_url.clone(),
                avatar_color: m.avatar_color.clone(),
            }),
            defaults: self.defaults.clone().or_else(|| me.map(Me::track_defaults)),
            can_switch: scopes.iter().any(|s| s == "profiles"),
            can_play: scopes.iter().any(|s| s == "play"),
            refused: self.refused,
            household: household.iter().map(|p| (p.id, p.name.clone())).collect(),
            always_start_as: state.always_start_as(),
            settings: state.settings().clone(),
            server: state.server().map(pley_core::addresses::host_port),
            device: fixed.map(|f| f.device.clone()).unwrap_or_default(),
            version: fixed.map(|f| f.version.clone()).unwrap_or_default(),
            licences: self.licences.clone(),
            language: match on_device {
                true => state.language_choice(),
                false => pley_i18n::supported(me.and_then(|m| m.language.as_deref())),
            },
            automatic_language: automatic,
            language_on_device: on_device,
        });
        self.revision += 1;
    }

    /// Collects the server's answer to a change of the account's language: the app shows it at
    /// once. A server without the route (404 or 405, a Pley from before languages) keeps none:
    /// the choice is this TV's from then on, as the globe picks it, and shows with no word.
    fn collect_language(&mut self, cx: &mut AppCx) {
        let Some(saving) = self.saving_language.as_mut() else { return };
        let answer = match saving.answer.take() {
            Some(a) => a,
            None if saving.answer.failed() => Err(ApiError::Unreadable("the request stopped".into())),
            None => return,
        };
        let Some(SavingLanguage { language, server, .. }) = self.saving_language.take() else { return };
        match answer {
            Ok(me) => {
                cx.state.account_changed(me);
                cx.toast(t!("settings.saved"));
            }
            Err(e) if e.is(404) || e.is(405) => {
                // Another server since: the answer was the one before's.
                if cx.state.server() == server.as_deref() {
                    cx.state.language_route_missing();
                    cx.state.choose_language_here(language);
                }
            }
            Err(e) if e.is(403) => cx.toast(t!("settings.readOnly")),
            Err(e) if !e.cancelled() => cx.toast(sentence(&e.to_string())),
            Err(_) => {}
        }
    }

    /// Collects the server's answer to a change of the defaults.
    fn collect(&mut self, cx: &mut AppCx) {
        let Some(p) = self.saving.as_mut() else { return };
        let answer = match p.take() {
            Some(a) => a,
            None if p.failed() => Err(ApiError::Unreadable("the request stopped".into())),
            None => return,
        };
        self.saving = None;
        self.dirty = true;
        match answer {
            Ok(me) => {
                if let Some(next) = self.queued.take() {
                    // A change made meanwhile goes now: it carries this one too.
                    self.save(next, cx);
                    return;
                }
                self.defaults = Some(me.track_defaults());
                // Titles kept start with the tracks these pick now; who is watching is asked
                // again, so every page has the new defaults.
                cx.state.tracks_changed(None);
                cx.state.load_account();
                cx.toast(t!("settings.saved"));
            }
            Err(e) => {
                self.defaults = None;
                self.queued = None;
                // Said as "Saved" is, at the top right for 3.5 s (`TvSettingsScreen.kt`'s message).
                if [403, 404, 405].iter().any(|s| e.is(*s)) {
                    self.refused = true;
                    cx.toast(t!("settings.readOnly"));
                } else if !e.cancelled() {
                    cx.toast(sentence(&e.to_string()));
                }
            }
        }
    }

    fn save(&mut self, after: TrackDefaults, cx: &mut AppCx) {
        self.saving = Some(cx.spawn(move |api| api.set_track_defaults(&after)));
    }

    fn pick(&mut self, picking: Picking, value: &Value, cx: &mut AppCx) {
        match value {
            Value::Profile(p) => cx.state.set_always_start_as(*p),
            Value::AppLanguage(language) if *language != self.facts.language => {
                if self.facts.language_on_device {
                    cx.state.choose_language_here(*language);
                } else if self.saving_language.is_none() {
                    let code = language.map(Language::code);
                    self.saving_language = Some(SavingLanguage {
                        answer: cx.spawn(move |api| api.set_language(code)),
                        language: *language,
                        server: cx.state.server().map(str::to_string),
                    });
                }
            }
            Value::AppLanguage(_) => {}
            Value::Language(_) | Value::Mode(_) => {
                // Nothing to change before who is watching is known.
                let Some(before) = self.facts.defaults.clone() else { return };
                if let Some(after) = pane::defaults_for(picking, &before, value) {
                    self.defaults = Some(after.clone());
                    self.dirty = true;
                    if self.saving.is_some() {
                        self.queued = Some(after);
                    } else {
                        self.save(after, cx);
                    }
                }
            }
            other => {
                let value = other.clone();
                cx.update_settings(|s| {
                    pane::apply_setting(s, &value);
                });
            }
        }
    }

    fn act(&mut self, action: &RowAction, cx: &mut AppCx) {
        match action {
            RowAction::None => {}
            RowAction::Choose(p) => {
                self.panel = Some(*p);
                self.panel_drawn = Some(*p);
                // The list opens on the choice in force: set when the panel first draws.
                self.panel_list = ListState::at(usize::MAX);
            }
            RowAction::Toggle(t) => {
                let t = *t;
                cx.update_settings(|s| t.flip(s));
            }
            RowAction::Open(route) => cx.push(route.clone()),
            RowAction::Restart(route) => cx.reset(route.clone()),
            RowAction::Web => {
                if let Some(server) = cx.state.server() {
                    self.web.open(format!("{}/account", server.trim_end_matches('/')));
                }
            }
            RowAction::SignOut => self.ask(Confirm::SignOut),
            RowAction::DeleteData => self.ask(Confirm::DeleteData),
            RowAction::Licence(i) => {
                notices::open_at(*i);
                cx.push(Route::Licences);
            }
        }
    }

    fn ask(&mut self, c: Confirm) {
        self.confirm = Some(c);
        self.confirm_drawn = c;
    }

    fn categories(&mut self, ui: &mut Ui, root: Id) -> Option<RowAction> {
        let group_id = root.with("categories");
        let mut action = None;
        let group =
            Group::column().edge(pley_ui::Dir::Up, Edge::Stop).edge(pley_ui::Dir::Down, Edge::Stop).takes(Dirs::RIGHT);
        ui.initial_focus(SettingsScreen::category_id(root, self.shown));
        let inside = ui.focused_in(group_id).is_some();
        if inside && ui.keys().iter().any(|k| k.key == Key::Right && k.pressed) {
            self.into = Some(self.shown);
        }
        ui.group(group_id, group, |ui| {
            let mut y = TOP;
            for c in SettingsSection::ALL {
                let (label, icon) = pane::category_look(c);
                let row = ListRow::setting(label)
                    .icon(icon)
                    .danger(c == SettingsSection::SignOut)
                    .trailing(if c == self.shown { Trailing::Chevron } else { Trailing::None });
                let h = row.height(ui, CATEGORIES_W);
                let r = row.show(ui, SettingsScreen::category_id(root, c), Point::new(CATEGORIES_X, y), CATEGORIES_W);
                if r.focused && self.shown != c {
                    self.shown = c;
                }
                if r.activated {
                    self.shown = c;
                    if c == SettingsSection::SignOut {
                        action = Some(RowAction::SignOut);
                    } else {
                        self.into = Some(c);
                    }
                }
                y += h + ROW_GAP;
            }
        });
        action
    }

    /// Makes the shown category's items again when the category or its facts changed. A row that
    /// came or went above the focused one leaves focus on the same row (Android keys its items).
    fn lay_out_pane(&mut self, ui: &mut Ui, root: Id) -> Option<Id> {
        if self.pane.as_ref().is_some_and(|p| p.section == self.shown && p.revision == self.revision) {
            return None;
        }
        let items = pane::items(self.shown, &self.facts);
        let mut heights: Vec<f32> = items.iter().map(|item| item_height(ui, item, PANE_W)).collect();
        heights.push(theme::SAFE_Y);
        let slot = SettingsScreen::slot(self.shown);
        let id = SettingsScreen::pane_id(root, self.shown);
        let mut refocus = None;
        if let Some(old) = self.pane.as_ref().filter(|p| p.section == self.shown)
            && ui.focused_in(id).is_some()
        {
            let at = self.panes[slot].index;
            if let Some(key) = old.items.get(at).and_then(Item::key)
                && let Some(now) = items.iter().position(|i| i.key() == Some(key))
                && now != at
            {
                self.panes[slot].index = now;
                refocus = Some(SettingsScreen::row_id(root, self.shown, now));
            }
        }
        self.pane = Some(PaneCache { section: self.shown, revision: self.revision, items, heights });
        refocus
    }

    fn pane(&mut self, ui: &mut Ui, root: Id, avatar: &[ImageRequest]) -> Option<RowAction> {
        let refocus = self.lay_out_pane(ui, root);
        let cache = self.pane.as_ref()?;
        let items = &cache.items;
        let heights = &cache.heights;
        let slot = SettingsScreen::slot(self.shown);
        let first = pane::first_row(items);
        let mut focus = refocus;
        if self.into.take() == Some(self.shown)
            && let Some(first) = first
        {
            // Into the category at its first row, whatever was focused in it before.
            self.panes[slot].index = first;
            self.panes[slot].scroll = 0.0;
            focus = Some(SettingsScreen::row_id(root, self.shown, first));
        }
        let view = Rect::new(PANE_X, TOP, PANE_W, theme::CANVAS_H - TOP);
        let list = List::new(view).gap(ROW_GAP).edges(Edge::Stop, Edge::Stop);
        let id = SettingsScreen::pane_id(root, self.shown);
        let facts = &*self.facts;
        let mut action = None;
        list.show(
            ui,
            id,
            &mut self.panes[slot],
            heights.len(),
            |_, i| heights[i],
            |ui, s| {
                let Some(item) = items.get(s.index) else { return };
                match item {
                    Item::Profile => profile_header(ui, s.rect, facts, avatar),
                    Item::Intro(text) => {
                        let opts = TextOpts::wrap(s.rect.w - INTRO_X, 0);
                        ui.text(Point::new(s.rect.x + INTRO_X, s.rect.y), text, &theme::BODY, &opts, theme::MUTED);
                    }
                    Item::Heading(text) => {
                        Heading::Settings.show(ui, text, s.rect.origin(), s.rect.w);
                    }
                    Item::Facts(list) => facts_table(ui, s.rect, list),
                    Item::Row(row) => {
                        if setting_row(row).show(ui, s.id, s.rect.origin(), s.rect.w).activated {
                            action = Some(row.action.clone());
                        }
                    }
                }
            },
        );
        if let Some(id) = focus {
            ui.request_focus(id);
        }
        action
    }

    fn panel(&mut self, ui: &mut Ui, root: Id, cx: &mut AppCx) {
        let open = self.panel.is_some();
        let drawn = self.panel_drawn;
        let (facts, revision) = (&*self.facts, self.revision);
        let fonts = self.fonts.as_deref().unwrap_or_default();
        let cache = &mut self.panel_cache;
        let list_state = &mut self.panel_list;
        let mut chosen = None;
        let r = SidePanel::new().back_closes(false).show(ui, root.with("panel"), open, |ui, area| {
            let Some(p) = drawn else { return };
            let title_w = area.w - 16.0;
            if cache.as_ref().is_none_or(|c| c.picking != p || c.revision != revision || c.width != area.w) {
                let panel = pane::panel(p, facts, fonts);
                // Measured as drawn: the check narrows the choice in force, whose words may then wrap.
                let current = panel.current;
                let heights =
                    panel.choices.iter().enumerate().map(|(i, c)| choice_row(c, i == current).height(ui, area.w));
                let heights = heights.collect();
                *cache = Some(PanelCache { picking: p, revision, width: area.w, panel, heights });
            }
            let Some(PanelCache { panel, heights, .. }) = cache.as_ref() else { return };
            let h = ui
                .text(
                    Point::new(area.x + 16.0, area.y + 8.0),
                    &panel.title,
                    &theme::TITLE,
                    &TextOpts::wrap(title_w, 2),
                    theme::FG,
                )
                .h;
            let top = area.y + 8.0 + h + 12.0;
            let view = Rect::new(area.x, top, area.w, area.bottom() - top);
            if list_state.index == usize::MAX {
                *list_state = ListState::at(panel.current);
            }
            let id = root.with("panel").with("choices");
            if !is_inert() {
                ui.initial_focus(id.with(panel.current as u64));
            }
            List::new(view).gap(ROW_GAP).edges(Edge::Stop, Edge::Stop).show(
                ui,
                id,
                list_state,
                heights.len(),
                |_, i| heights[i],
                |ui, s| {
                    let choice = &panel.choices[s.index];
                    if choice_row(choice, s.index == panel.current).show(ui, s.id, s.rect.origin(), s.rect.w).activated
                    {
                        chosen = Some((p, choice.value.clone()));
                    }
                },
            );
        });
        if r.closed {
            self.panel = None;
        }
        if let Some((p, value)) = chosen
            && self.panel.is_some()
        {
            self.panel = None;
            self.pick(p, &value, cx);
        }
    }

    fn confirm(&mut self, ui: &mut Ui, root: Id, cx: &mut AppCx) {
        let cancel = DialogButton::new(t!("settings.actions.cancel"));
        let (title, body, buttons) = match self.confirm_drawn {
            Confirm::SignOut => (
                t!("settings.signOut.title"),
                match self.facts.server.as_deref() {
                    Some(server) => t!("settings.signOut.body", server = server),
                    None => t!("settings.signOut.bodyNoServer").to_string(),
                },
                [cancel, DialogButton::danger(t!("settings.signOut.button"))],
            ),
            Confirm::DeleteData => (
                t!("settings.signOut.deleteTitle"),
                t!("settings.signOut.deleteBody").to_string(),
                [cancel, DialogButton::danger(t!("settings.signOut.delete"))],
            ),
        };
        let r = Dialog::new(title).body(&body).buttons(&buttons).initial(0).show(
            ui,
            root.with("confirm"),
            self.confirm.is_some(),
        );
        if (r.dismissed || r.chosen == Some(0)) && self.confirm.take().is_some() {
            self.to_sign_out = self.confirm_drawn == Confirm::SignOut;
        }
        if std::mem::take(&mut self.to_sign_out) {
            // Not signing out puts focus on the Sign out category, wherever the question came from
            // (`TvSettingsScreen.kt`'s `askedSignOut`).
            self.shown = SettingsSection::SignOut;
            ui.request_focus(SettingsScreen::category_id(root, SettingsSection::SignOut));
        }
        if r.chosen == Some(1) && self.confirm.take().is_some() {
            match self.confirm_drawn {
                Confirm::SignOut => {
                    let back_to = match cx.state.server() {
                        Some(server) => Route::Link { server: server.to_string() },
                        None => Route::Servers,
                    };
                    cx.state.sign_out();
                    cx.reset(back_to);
                }
                Confirm::DeleteData => {
                    cx.state.delete_local_data();
                    cx.reset(Route::Servers);
                }
            }
        }
    }
}

impl Screen for SettingsScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.collect(cx);
        self.collect_language(cx);
        self.gather(ui, cx);
        let root = ui.scope_id();
        let avatar: Vec<ImageRequest> = match (&self.facts.profile, self.shown) {
            (Some(p), SettingsSection::Profile) => {
                p.avatar_url.as_deref().and_then(|url| cx.avatar(url, ui.image_px(AVATAR))).into_iter().collect()
            }
            _ => Vec::new(),
        };
        ui.text(
            Point::new(theme::GUTTER, theme::SAFE_Y),
            t!("settings.title"),
            &theme::PAGE_TITLE,
            &TextOpts::line(CATEGORIES_W + PANE_W),
            theme::FG,
        );
        let mut action = self.categories(ui, root);
        if let Some(a) = self.pane(ui, root, &avatar) {
            action = Some(a);
        }
        if let Some(a) = action {
            self.act(&a, cx);
        }
        self.panel(ui, root, cx);
        self.confirm(ui, root, cx);
        self.web.show(ui, root.with("web"));
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if let Some(c) = self.confirm.take() {
            self.to_sign_out = c == Confirm::SignOut;
            return true;
        }
        if self.panel.take().is_some() {
            return true;
        }
        if self.web.is_open() {
            self.web.close();
            return true;
        }
        false
    }

    fn event(&mut self, event: &AppEvent, _cx: &mut AppCx) {
        if matches!(event, AppEvent::SignedOut) {
            self.panel = None;
            self.confirm = None;
            self.web.close();
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn modal(&self) -> bool {
        self.panel.is_some() || self.confirm.is_some() || self.web.is_open()
    }

    /// Every word comes from the facts and the panes made from them, so they are made again in
    /// the new language and the page keeps its place: a language picked in Settings leaves focus
    /// on the Language row, and a change being kept is still heard.
    fn relabel(&mut self, cx: &mut AppCx) -> bool {
        if let Some(fixed) = self.fixed.as_mut() {
            fixed.device = device_facts(&cx.device().info(), cx.device().tier());
        }
        self.dirty = true;
        self.panel_cache = None;
        true
    }
}

/// A pane row as the kit draws it.
fn setting_row(row: &pane::Row) -> ListRow<'_> {
    let mut r = ListRow::setting(&row.label).supporting(&row.value).danger(row.danger).trailing(match row.end {
        End::Nothing => Trailing::None,
        End::Chevron => Trailing::Chevron,
        End::Switch(on) => Trailing::Switch(on),
    });
    if let Some(icon) = row.icon {
        r = r.icon(icon);
    }
    r
}

/// A panel's choice as the kit draws it: its label, what it does under it, a check when in force.
fn choice_row(choice: &pane::Choice, checked: bool) -> ListRow<'_> {
    ListRow::new(&choice.label).supporting(choice.hint.unwrap_or("")).supporting_lines(3).checked(checked)
}

/// How tall a pane's item is at `width`.
fn item_height(ui: &Ui, item: &Item, width: f32) -> f32 {
    match item {
        Item::Profile => AVATAR + 12.0,
        Item::Intro(text) => ui.measure(text, &theme::BODY, &TextOpts::wrap(width - INTRO_X, 0)).h + INTRO_BELOW,
        Item::Heading(text) => Heading::Settings.height(ui, text, width),
        Item::Facts(list) => {
            let n = list.len() as f32;
            12.0 + n * theme::CAPTION.line_height + (n - 1.0).max(0.0) * 6.0
        }
        Item::Row(row) => setting_row(row).height(ui, width),
    }
}

/// The Profile category's header (`TvSettingsScreen.kt`): a 48 dp avatar, the name in `Tv.Title`,
/// and "Managed profile · host:port" or the address in `Tv.Caption` Muted, 20 in, 12 above the
/// rows.
fn profile_header(ui: &mut Ui, rect: Rect, f: &Facts, avatar: &[ImageRequest]) {
    let Some(p) = &f.profile else { return };
    let circle = Rect::new(rect.x + 20.0, rect.y, AVATAR, AVATAR);
    ProfileTile::new(&p.name).color(avatar_color(p.avatar_color.as_deref())).picture(avatar).avatar(ui, circle);
    let x = circle.right() + 16.0;
    let w = rect.right() - x;
    let line = [p.managed.then(|| t!("settings.profile.managed")), f.server.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let block = theme::TITLE.line_height + theme::CAPTION.line_height;
    let y = circle.y + (AVATAR - block) / 2.0;
    ui.text(Point::new(x, y), &p.name, &theme::TITLE, &TextOpts::line(w), theme::FG);
    ui.text(Point::new(x, y + theme::TITLE.line_height), &line, &theme::CAPTION, &TextOpts::line(w), theme::MUTED);
}

/// The device's facts under "Open diagnostics": `Tv.Caption`, a 150 dp label column in Muted,
/// lines 6 apart, 20 in and 12 below the row.
fn facts_table(ui: &mut Ui, rect: Rect, list: &[(String, String)]) {
    let mut y = rect.y + 12.0;
    let x = rect.x + 20.0;
    let value_w = rect.w - 40.0 - FACT_LABEL;
    for (label, value) in list {
        ui.text(Point::new(x, y), label, &theme::CAPTION, &TextOpts::line(FACT_LABEL - 8.0), theme::MUTED);
        ui.text(Point::new(x + FACT_LABEL, y), value, &theme::CAPTION, &TextOpts::line(value_w), theme::FG);
        y += theme::CAPTION.line_height + 6.0;
    }
}

/// The TV's facts, label and value, as Settings' Diagnostics and the Diagnostics page list them.
pub fn device_facts(info: &pley_core::DeviceInfo, tier: pley_base::Tier) -> Vec<(String, String)> {
    let tier = match tier {
        pley_base::Tier::Low => t!("diagnostics.tiers.low"),
        pley_base::Tier::Mid => t!("diagnostics.tiers.mid"),
        pley_base::Tier::High => t!("diagnostics.tiers.high"),
    };
    let or_dash = |s: &str| if s.is_empty() { "—".to_string() } else { s.to_string() };
    let mut out = vec![
        (t!("diagnostics.facts.model").to_string(), or_dash(&info.model)),
        (t!("diagnostics.facts.system").to_string(), or_dash(&info.os)),
        (t!("diagnostics.facts.firmware").to_string(), info.firmware.clone().unwrap_or_else(|| "—".into())),
        (t!("diagnostics.facts.tier").to_string(), tier.to_string()),
    ];
    if !info.data_durable {
        // The data folder is in /tmp: say so, since the sign-in goes with it.
        out.push((t!("diagnostics.facts.savedData").to_string(), t!("diagnostics.facts.lostOnRestart").to_string()));
    }
    out
}

/// The subtitle faces this TV draws differently from its default face: a sample measured in each
/// (the TV app's `fontsHere`); a face the TV lacks falls back to the default and measures the same.
fn fonts_here(ui: &Ui) -> Vec<SubtitleFont> {
    let sample = "Pley shows subtitles: WiWi mmm lll";
    let style = |family: Family| TextStyle { family, ..theme::BODY };
    let base = ui.measure(sample, &style(Family::Sans), &TextOpts::default()).w;
    [
        (SubtitleFont::Serif, Family::Named(Arc::from("serif"))),
        (SubtitleFont::Monospace, Family::Mono),
        (SubtitleFont::Casual, Family::Named(Arc::from("casual"))),
    ]
    .into_iter()
    .filter(|(_, family)| (ui.measure(sample, &style(family.clone()), &TextOpts::default()).w - base).abs() > 0.5)
    .map(|(font, _)| font)
    .collect()
}
