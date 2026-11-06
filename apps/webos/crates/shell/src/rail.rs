//! The rail at the left edge (Android's `TvRail.kt` and
//! `RailItems.kt`, the TV app's `ui/shell/rail.tsx`): icons alone while the page has focus, and
//! labels on a panel while it has focus, which is exactly when it is open. Opening, the panel
//! widens out of the closed rail as it fades in, and the labels slide in 24 dp as they fade.
//!
//! Focus enters it only when sent (Left where nothing is further left, Back on a first page, a
//! page with nothing to focus), always on the current entry. Up and Down move along it, Left does
//! nothing, Right goes back to the element the page had. Choosing the page already shown returns
//! to it; any other entry starts the back stack over with its page, the rail keeping focus until
//! that page has something to focus.
//!
//! The avatar stays at the top and Settings at the foot; the entries between them scroll when
//! more than nine are listed, keeping the focused one in view (the current one while closed) with
//! the next one in the direction of travel, and are cut off at the avatar and Settings.

use pley_base::{Color, Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::model::{Library, LibraryKind, Me, RequestSummary};
use pley_core::requests::{has_requests, may_ask, requests_count, requests_title};
use pley_i18n::t;
use pley_ui::theme::{self, focus};
use pley_ui::{Dir, Dirs, Easing, Edge, Group, Id, ImageOpts, Motion, Ui, nudge};

use crate::route::Route;

/// At most this many libraries have entries of their own (`MAX_LIBRARIES`); past that, Libraries
/// lists them all.
pub const MAX_LIBRARIES: usize = 6;

/// The rail's width while closed: 8 + 56 + 8.
pub const CLOSED_WIDTH: f32 = theme::RAIL_COLLAPSED;
/// The open panel's width: 8 + 256 + 8 (a 64 dp soft edge beyond it).
pub const OPEN_WIDTH: f32 = theme::RAIL_EXPANDED;

const X: f32 = 8.0;
const TOP: f32 = 15.0;
const BOTTOM: f32 = 27.0;
/// The avatar's entry: 48 high, its 32 dp picture in the middle.
const AVATAR_ENTRY: f32 = 48.0;
const AVATAR_GAP: f32 = 4.0;
/// An entry: as high as the library's chips, 4 apart.
const ENTRY_H: f32 = 40.0;
const ENTRY_W: f32 = 56.0;
const ENTRY_W_OPEN: f32 = 256.0;
const GAP: f32 = 4.0;
const PITCH: f32 = ENTRY_H + GAP;
const ICON: f32 = 20.0;
const LABEL_X: f32 = 56.0;
const LABEL_W: f32 = 188.0;
const AVATAR: f32 = 32.0;
const MARK: f32 = 16.0;

/// The pill's growth as the rail opens: 250 ms of cubic-bezier(0.2, 0, 0, 1), the web port's.
const GROW: Motion = Motion::new(0.250, Easing::cubic(0.2, 0.0, 0.0, 1.0));

/// One entry of the rail.
#[derive(Clone, Debug, PartialEq)]
pub struct RailEntry {
    /// `search`, `home`, `live-tv`, `library:<id>`, `libraries`, `playlists`, `collections`,
    /// `now-playing`, `storage`, `settings`, `profile`.
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub route: Route,
    /// A number beside the icon (what waits in requests), in the danger colour with `.1`.
    pub badge: Option<(i64, bool)>,
}

impl RailEntry {
    fn new(id: impl Into<String>, label: impl Into<String>, icon: Icon, route: Route) -> RailEntry {
        RailEntry { id: id.into(), label: label.into(), icon, route, badge: None }
    }
}

/// The rail's way to requests, after Home: Discover where the profile may ask, else its requests
/// ("Requests" for managers) where it has any to see; none without requests. It carries the
/// requests' one number.
pub fn requests_entry(s: Option<&RequestSummary>) -> Option<RailEntry> {
    let (count, danger) = requests_count(s);
    let badge = (count > 0).then_some((count, danger));
    let entry = if may_ask(s) {
        RailEntry::new("discover", t!("shell.rail.discover"), Icon::Compass, Route::Discover)
    } else if has_requests(s) {
        RailEntry::new("requests", requests_title(s), Icon::Inbox, Route::Requests)
    } else {
        return None;
    };
    Some(RailEntry { badge, ..entry })
}

/// The rail's entries: those between the avatar and Settings, and Settings at the foot.
#[derive(Clone, Debug, PartialEq)]
pub struct RailEntries {
    pub top: Vec<RailEntry>,
    pub settings: RailEntry,
}

impl RailEntries {
    /// Whether the rail has the entry `id` between the avatar and Settings.
    pub fn has(&self, id: &str) -> bool {
        self.top.iter().any(|e| e.id == id)
    }
}

fn library_icon(kind: LibraryKind) -> Icon {
    match kind {
        LibraryKind::Movies => Icon::Clapperboard,
        LibraryKind::Shows => Icon::Tv,
        LibraryKind::Music => Icon::Music,
        _ => Icon::Film,
    }
}

/// The rail's entries (Android's `RailItems`): Search, Home, Discover (or the requests,
/// [`requests_entry`]), Live TV where the server has channels for this profile, up to six
/// libraries, then Libraries when there are more, Playlists, and Collections when there are one to
/// six libraries; Now Playing while music has a queue; USB drive last while the app can read a
/// drive plugged into the TV (the Samsung app's `Pages.cs`); Settings at the foot. Nothing makes
/// room for anything: the entries scroll when they don't fit.
pub fn rail_entries(
    libraries: &[Library],
    now_playing: bool,
    live: bool,
    requests: Option<RailEntry>,
    drive: bool,
) -> RailEntries {
    let shown = libraries.len().min(MAX_LIBRARIES);
    let mut top = vec![
        RailEntry::new("search", t!("shell.rail.search"), Icon::Search, Route::Search { query: String::new() }),
        RailEntry::new("home", t!("shell.rail.home"), Icon::House, Route::Home),
    ];
    top.extend(requests);
    if live {
        top.push(RailEntry::new("live-tv", t!("shell.rail.liveTv"), Icon::TvMinimal, Route::LiveTv));
    }
    for l in &libraries[..shown] {
        let route = Route::Library { id: l.id, tab: None, sort: None };
        top.push(RailEntry::new(format!("library:{}", l.id), l.name.clone(), library_icon(l.kind), route));
    }
    if libraries.len() > shown {
        top.push(RailEntry::new(
            "libraries",
            t!("shell.rail.libraries"),
            Icon::Layers,
            Route::Libraries { sort: false },
        ));
    }
    top.push(RailEntry::new("playlists", t!("shell.rail.playlists"), Icon::ListMusic, Route::Playlists));
    // Collections while every library has an entry: the first one's that isn't music.
    if libraries.len() == shown
        && let Some(first) = libraries.iter().find(|l| l.kind != LibraryKind::Music).or(libraries.first())
    {
        let route = Route::Collections { library_id: Some(first.id) };
        top.push(RailEntry::new("collections", t!("shell.rail.collections"), Icon::Layers, route));
    }
    if now_playing {
        top.push(RailEntry::new("now-playing", t!("shell.rail.nowPlaying"), Icon::AudioLines, Route::NowPlaying));
    }
    if drive {
        top.push(RailEntry::new("storage", t!("shell.rail.storage"), Icon::Usb, Route::Storage { path: None }));
    }
    RailEntries {
        top,
        settings: RailEntry::new(
            "settings",
            t!("shell.rail.settings"),
            Icon::Settings,
            Route::Settings { section: None },
        ),
    }
}

/// The entry of the page at the root of the back stack (`railCurrent`): the entry that opened it,
/// Libraries for a library not in the rail, the avatar for Profiles, or none.
pub fn current_entry(root: Option<&Route>, entries: &RailEntries) -> Option<String> {
    let top = &entries.top;
    let has = |id: &str| entries.has(id).then(|| id.to_string());
    match root? {
        Route::Library { id, .. } => has(&format!("library:{id}")).or_else(|| has("libraries")),
        Route::Collections { library_id } => has("collections").or_else(|| {
            let id = (*library_id)?;
            current_entry(Some(&Route::Library { id, tab: None, sort: None }), entries)
        }),
        Route::Settings { .. } => Some("settings".into()),
        Route::Profiles => Some("profile".into()),
        // Discover's pages, and the requests reached from it, under whichever of the two the rail
        // has.
        Route::Discover
        | Route::DiscoverList { .. }
        | Route::DiscoverSearch { .. }
        | Route::Title { .. }
        | Route::Requests => {
            let own = if matches!(root?, Route::Requests) { "requests" } else { "discover" };
            has(own).or_else(|| has("discover")).or_else(|| has("requests"))
        }
        root => {
            top.iter().find(|e| std::mem::discriminant(&e.route) == std::mem::discriminant(root)).map(|e| e.id.clone())
        }
    }
}

/// A profile's own colour (`#e07a5f` only), else none.
pub fn avatar_color(color: Option<&str>) -> Option<Color> {
    let hex = color?.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().map(Color::rgb)
}

/// The initial on an avatar without a picture.
pub fn initial(name: &str) -> String {
    name.trim().chars().next().map_or_else(|| "?".to_string(), |c| c.to_uppercase().collect())
}

/// What the person did with the rail this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RailOutcome {
    /// An entry was chosen.
    pub chosen: Option<RailEntry>,
}

/// What the rail shows besides its entries.
pub struct RailView<'a> {
    pub entries: &'a RailEntries,
    /// The current entry's id.
    pub current: Option<&'a str>,
    pub me: Option<&'a Me>,
    /// The avatar's picture, when the profile has one.
    pub avatar: Option<ImageRequest>,
    /// The server is out of reach and the notice has had its 6 s: the avatar wears a mark.
    pub offline_mark: bool,
    /// The page has a panel or dialog open: the pointer passing over the rail leaves it closed.
    pub page_modal: bool,
}

/// The rail's state between frames.
#[derive(Debug, Default)]
pub struct Rail {
    open: bool,
    /// Opened by choosing a page here: it hands focus to the page once the page has something to
    /// focus, unless the person moves along it meanwhile.
    handing: bool,
    /// The pointer has been over the open rail: moving it out over the page closes it (a rail
    /// opened by keys stays).
    pointed_in: bool,
    last_pointer: Option<Point>,
    last_focus: Option<Id>,
    /// The entry chosen, to keep focus once the page it opened replaces the stack.
    chosen: Option<String>,
    /// How far the entries between the avatar and Settings are scrolled: where it is heading.
    scroll: f32,
}

/// The rail's focus group (also its scope while closed, so its animated values carry over).
pub fn rail_id() -> Id {
    Id::new(RAIL_KEY)
}

const RAIL_KEY: &str = "pley.rail";

impl Rail {
    /// Whether it is open (it has focus).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether it waits to hand focus to a page chosen in it.
    pub fn handing(&self) -> bool {
        self.open && self.handing
    }

    /// The focus id of the entry `entry_id`.
    pub fn entry_focus_id(entry_id: &str) -> Id {
        rail_id().with(entry_id)
    }

    /// Opens it with focus on `entry` (the current one, Home's when there is none).
    pub fn open(&mut self, ui: &mut Ui, entry: &str) {
        self.open = true;
        self.handing = false;
        self.pointed_in = false;
        ui.request_focus(Rail::entry_focus_id(entry));
    }

    /// Opens it for focus to move to with the next frame (from outside a frame: Back handled
    /// after drawing).
    pub fn open_later(&mut self, ui: &mut pley_ui::UiState, entry: &str) {
        self.open = true;
        self.handing = false;
        self.pointed_in = false;
        ui.request_focus(Rail::entry_focus_id(entry));
    }

    /// Closes it; focus goes back to what the page had (or the page's first element).
    pub fn close(&mut self) {
        self.open = false;
        self.handing = false;
        self.pointed_in = false;
    }

    /// The page of `entry` was chosen here: keep focus on the entry until the page can take it.
    pub fn hand_over_later(&mut self, entry: &str) {
        self.handing = true;
        self.chosen = Some(entry.to_string());
    }

    /// The entry to focus again once the page chosen here starts the stack over (its focus
    /// starts over with it).
    pub fn take_chosen(&mut self) -> Option<String> {
        self.chosen.take()
    }

    /// Draws the rail over the page: the fade at the page's left edge, then the rail closed or
    /// open, and handles its keys and the pointer.
    pub fn ui(&mut self, ui: &mut Ui, view: &RailView) -> RailOutcome {
        let mut outcome = RailOutcome::default();
        if self.open && ui.take_key(pley_base::Key::Right) {
            self.close();
        }
        // Left in the open rail does nothing.
        if self.open {
            ui.take_key(pley_base::Key::Left);
        }
        let t = ui.animate(rail_id().with("open"), if self.open { 1.0 } else { 0.0 }, Motion::OVERLAY);
        let g = ui.animate(rail_id().with("grow"), if self.open { 1.0 } else { 0.0 }, GROW);

        if !self.open {
            ui.ramp(Rect::new(0.0, 0.0, CLOSED_WIDTH, theme::CANVAS_H), false, &theme::RAIL_FADE);
        }
        if t > 0.0 {
            // The panel grows out of the closed rail, its soft edge riding on its right (the
            // Samsung app's RailView).
            let width = panel_width(g);
            ui.opacity(t, |ui| {
                ui.fill(Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H), theme::RAIL_SCRIM, 0.0);
                ui.fill(Rect::new(0.0, 0.0, width, theme::CANVAS_H), theme::RAIL_PANEL, 0.0);
                let edge = theme::RAIL_EDGE_END - OPEN_WIDTH;
                let stops = theme::RAIL_PANEL_EDGE.stops(OPEN_WIDTH, edge);
                ui.gradient(Rect::new(width, 0.0, edge, theme::CANVAS_H), false, stops);
            });
        }

        let pointer = ui.pointer();
        let moved = pointer.is_some() && pointer != self.last_pointer;
        self.last_pointer = pointer;
        if self.open {
            // Clicking the page, or moving out over it after being over the rail, closes it.
            let scrim =
                ui.clickable(rail_id().with("scrim"), Rect::new(OPEN_WIDTH, 0.0, theme::CANVAS_W, theme::CANVAS_H));
            if scrim.clicked || (scrim.hovered && moved && self.pointed_in) {
                self.close();
            }
        }

        let scroll = self.follow(ui, view);
        let open = self.open;
        let group =
            Group::column().edge(Dir::Up, Edge::Stop).edge(Dir::Down, Edge::Stop).takes(Dirs::HORIZONTAL).trap();
        let mut chosen = None;
        let mut focus_now = None;
        let mut draw = |ui: &mut Ui| {
            if let Some(r) = profile_entry(ui, view, open, t, g) {
                if r.focused {
                    focus_now = Some(r.id);
                }
                if r.activated {
                    chosen = Some(RailEntry::new("profile", t!("shell.rail.profiles"), Icon::Users, Route::Profiles));
                }
            }
            let mut entry = |ui: &mut Ui, entry: &RailEntry, y: f32| {
                if let Some(r) = entry_ui(ui, entry, y, view.current == Some(entry.id.as_str()), open, t, g) {
                    if r.focused {
                        focus_now = Some(r.id);
                    }
                    if r.activated {
                        chosen = Some(entry.clone());
                    }
                }
            };
            // Every entry is drawn, for focus to move along them in order; those scrolled under
            // the avatar or over Settings are cut off there.
            ui.clip(Rect::new(0.0, CLIP_TOP, theme::CANVAS_W, SETTINGS_Y - CLIP_TOP), |ui| {
                for (i, e) in view.entries.top.iter().enumerate() {
                    entry(ui, e, FIRST_Y + i as f32 * PITCH - scroll);
                }
            });
            entry(ui, &view.entries.settings, SETTINGS_Y);
        };
        if open {
            ui.group(rail_id(), group, &mut draw);
        } else {
            ui.push_id(RAIL_KEY, &mut draw);
        }

        if open {
            // Moving along the rail: the person is choosing again, and a page that loads keeps
            // its hands off.
            if focus_now.is_some() && self.last_focus.is_some() && focus_now != self.last_focus {
                self.handing = false;
            }
            if ui.pointer_mode() && focus_now.is_some() && moved {
                self.pointed_in = true;
            }
        } else if let Some(p) = pointer.filter(|_| moved && !view.page_modal) {
            // The pointer moving onto an entry opens the rail there.
            let strip = ui.clickable(rail_id().with("strip"), Rect::new(0.0, 0.0, CLOSED_WIDTH, theme::CANVAS_H));
            if strip.hovered
                && let Some(id) = entry_at(view, p, scroll)
            {
                self.open(ui, &id);
                self.pointed_in = true;
            }
        } else {
            ui.clickable(rail_id().with("strip"), Rect::new(0.0, 0.0, CLOSED_WIDTH, theme::CANVAS_H));
        }
        self.last_focus = focus_now;
        outcome.chosen = chosen;
        outcome
    }

    /// Where the entries between the avatar and Settings show scrolled to this frame, by the rule
    /// the TV apps share ([`rail_scroll`]): placed for the focused entry, or, while the rail is
    /// closed, for the current one, so opening it on that entry moves nothing. The pointer keeps
    /// its own way: the entry it is on comes in only as far as shows it whole, since entries
    /// moving under a still pointer would move focus.
    fn follow(&mut self, ui: &mut Ui, view: &RailView) -> f32 {
        let top = &view.entries.top;
        let at = if self.open {
            ui.focused().and_then(|id| place(view.entries, id))
        } else {
            view.current.and_then(|c| top.iter().position(|e| e.id == c)).map(|i| i as isize)
        };
        let at = at.unwrap_or(-1);
        self.scroll = match usize::try_from(at) {
            Ok(i) if self.open && ui.pointer_mode() && i < top.len() => {
                let y = i as f32 * PITCH;
                nudge(self.scroll, y, y + ENTRY_H, 0.0, VIEW_H, 0.0, scroll_end(top.len()))
            }
            _ => rail_scroll(self.scroll, at, top.len()),
        };
        ui.scroll(rail_id().with("scroll"), self.scroll, SCROLL)
    }
}

/// Where the rail's element `id` is along it (packages/tv's `placeOf`): -1 for the avatar, an
/// entry's index, the entries' count for Settings.
fn place(entries: &RailEntries, id: Id) -> Option<isize> {
    if id == Rail::entry_focus_id("profile") {
        return Some(-1);
    }
    if id == Rail::entry_focus_id(&entries.settings.id) {
        return Some(entries.top.len() as isize);
    }
    entries.top.iter().position(|e| Rail::entry_focus_id(&e.id) == id).map(|i| i as isize)
}

/// How far the entries are scrolled while entry `focused` of `count` has focus (packages/tv's
/// `railScroll`, which packages/tv/test/fixtures/rail.json pins for every TV app): by whole
/// entries, so none is ever cut in half at either end. Going down, focus moves until it is the
/// fourth entry shown, then stays there while the entries move under it an entry a press, until
/// the last ones are all in view and focus goes on to the last; going up is the same backwards.
/// Where an entry shows depends on it alone, never on the way focus came or on `scroll`, as the
/// app's lists keep their focused row in one place. With no entry focused (the avatar,
/// Settings, -1 or `count`) `scroll` is kept, within the ends.
fn rail_scroll(scroll: f32, focused: isize, count: usize) -> f32 {
    let end = scroll_end(count);
    match usize::try_from(focused) {
        Ok(i) if i < count => ((i as f32 - PIVOT) * PITCH).clamp(0.0, end),
        _ => scroll.clamp(0.0, end),
    }
}

/// The furthest the entries scroll: by whole entries, until the last nine show.
fn scroll_end(count: usize) -> f32 {
    count.saturating_sub(FITS) as f32 * PITCH
}

/// How far a label slides in from as the rail opens: a little, since an entry doesn't clip it and
/// more would cross the icon (the Samsung app's `LabelSlide`).
const LABEL_SLIDE: f32 = 24.0;

/// The panel's width as it grows (`g` from 0, closed, to 1, open).
fn panel_width(g: f32) -> f32 {
    CLOSED_WIDTH + (OPEN_WIDTH - CLOSED_WIDTH) * g
}

/// How far right of their place the labels are: sliding in as the rail opens (`t` from 0 to 1),
/// fading where they are as it closes.
fn label_slide(open: bool, t: f32) -> f32 {
    if open { LABEL_SLIDE * (1.0 - t) } else { 0.0 }
}

/// The first entry's top, unscrolled: 67.
const FIRST_Y: f32 = TOP + AVATAR_ENTRY + AVATAR_GAP;
/// Settings' top: 473, 27 from the bottom.
const SETTINGS_Y: f32 = theme::CANVAS_H - BOTTOM - ENTRY_H;
/// The view the entries scroll in, from the first entry's top to a gap above Settings: nine fit.
const VIEW_H: f32 = SETTINGS_Y - GAP - FIRST_Y;
/// Whole entries in the view, the last one's gap aside: nine.
const FITS: usize = ((VIEW_H + GAP) / PITCH) as usize;
/// The entries shown above the focused one while they scroll under it ([`rail_scroll`]).
const PIVOT: f32 = 3.0;
/// What shows of the entries, from the avatar's foot to Settings, so a focused entry growing at
/// either end of the view isn't cut.
const CLIP_TOP: f32 = TOP + AVATAR_ENTRY;
/// The scroll's slide (the kit's lists').
const SCROLL: Motion = pley_kit::SCROLL;

/// The entry under the pointer at `p` while the rail is closed, its entries scrolled by `scroll`.
fn entry_at(view: &RailView, p: Point, scroll: f32) -> Option<String> {
    if p.x >= CLOSED_WIDTH {
        return None;
    }
    if (TOP..TOP + AVATAR_ENTRY).contains(&p.y) {
        return Some("profile".into());
    }
    if (SETTINGS_Y..SETTINGS_Y + ENTRY_H).contains(&p.y) {
        return Some(view.entries.settings.id.clone());
    }
    if !(CLIP_TOP..SETTINGS_Y).contains(&p.y) {
        return None;
    }
    let y = p.y - FIRST_Y + scroll;
    let i = (y / PITCH).floor();
    let inside = y >= 0.0 && y - i * PITCH < ENTRY_H;
    inside.then(|| view.entries.top.get(i as usize).map(|e| e.id.clone())).flatten()
}

/// How an entry looks this frame: its fill and content colour.
fn entry_look(ui: &mut Ui, id: Id, focused: bool, pressed: bool, current: bool, t: f32) -> (Color, Color) {
    let look = if current { focus::RAIL_ENTRY_CURRENT } else { focus::RAIL_ENTRY };
    let rest = if current { theme::alpha(theme::SURFACE3, t) } else { Color::TRANSPARENT };
    let fill = if pressed {
        look.pressed_fill
    } else if focused {
        look.focus_fill
    } else {
        rest
    };
    let fill = ui.animate_color(id.with("fill"), fill, Motion::FOCUS);
    let content =
        ui.animate_color(id.with("content"), if focused { look.focus_content } else { look.content }, Motion::FOCUS);
    (fill, content)
}

/// One entry: focusable while open, an icon (and the current bar) while closed.
fn entry_ui(
    ui: &mut Ui,
    e: &RailEntry,
    y: f32,
    current: bool,
    open: bool,
    t: f32,
    g: f32,
) -> Option<pley_ui::Response> {
    let id = ui.id(e.id.as_str());
    ui.name(id, || match e.badge {
        Some((n, _)) => format!("{}, {}", e.label, pley_i18n::number(n)),
        None => e.label.clone(),
    });
    let hit = Rect::new(X, y, ENTRY_W_OPEN, ENTRY_H);
    let r = open.then(|| ui.focusable(id, hit));
    let focused = r.is_some_and(|r| r.focused);
    let pressed = r.is_some_and(|r| r.pressed);
    let (fill, content) = entry_look(ui, id, focused, pressed, current, t);
    let pill = Rect::new(X, y, ENTRY_W + (ENTRY_W_OPEN - ENTRY_W) * g, ENTRY_H);
    let scale = ui.animate(id.with("scale"), if focused { focus::RAIL_ENTRY.focus_scale } else { 1.0 }, Motion::FOCUS);
    ui.transform(Point::ZERO, scale, pill.center(), |ui| {
        if fill.a > 0 {
            ui.fill(pill, fill, theme::radius::CONTROL);
        }
        if current && t < 1.0 {
            let bar = Rect::new(X, y + (ENTRY_H - 20.0) / 2.0, 4.0, 20.0);
            ui.fill(bar, theme::alpha(theme::ACCENT, 1.0 - t), theme::radius::BADGE);
        }
        let icon = Rect::new(X + 18.0, y + (ENTRY_H - ICON) / 2.0, ICON, ICON);
        ui.icon(icon, e.icon, content);
        if let Some((count, danger)) = e.badge {
            badge(ui, Point::new(icon.x + 11.0, icon.y - 8.0), count, danger, focused);
        }
        if t > 0.0 {
            let at = Point::new(X + LABEL_X + label_slide(open, t), y + (ENTRY_H - theme::LABEL.line_height) / 2.0);
            ui.opacity(t, |ui| ui.text(at, &e.label, &theme::LABEL, &TextOpts::line(LABEL_W), content));
        }
    });
    r
}

/// The number on an entry's icon: Accent with OnAccent (OnAccent with AccentBright on the focused
/// entry), Danger while something failed; 18 high, 99 at most.
fn badge(ui: &mut Ui, at: Point, count: i64, danger: bool, focused: bool) {
    let text = if count > 99 { "99+".to_string() } else { count.to_string() };
    let w = (ui.measure(&text, &theme::BADGE, &TextOpts::line(f32::MAX)).w + 10.0).max(18.0);
    let rect = Rect::new(at.x, at.y, w, 18.0);
    let (fill, ink) = match (danger, focused) {
        (true, _) => (theme::DANGER, theme::BG),
        (false, true) => (theme::ON_ACCENT, theme::ACCENT_BRIGHT),
        (false, false) => (theme::ACCENT, theme::ON_ACCENT),
    };
    // A ring of the background around it, as on the web.
    ui.fill(Rect::new(rect.x - 2.0, rect.y - 2.0, rect.w + 4.0, rect.h + 4.0), theme::BG, 11.0);
    ui.fill(rect, fill, 9.0);
    let line = (rect.h - theme::BADGE.line_height) / 2.0;
    let opts = TextOpts::line(rect.w).aligned(pley_base::Align::Center);
    ui.text(Point::new(rect.x, rect.y + line), &text, &theme::BADGE, &opts, ink);
}

/// The avatar entry: the profile's picture or initial, the offline mark, and while open its name
/// over "Switch profile".
fn profile_entry(ui: &mut Ui, view: &RailView, open: bool, t: f32, g: f32) -> Option<pley_ui::Response> {
    let id = ui.id("profile");
    let switch = t!("shell.rail.switchProfile");
    ui.name(id, || match view.me {
        Some(m) => format!("{}, {switch}", m.name),
        None => switch.to_string(),
    });
    let r = open.then(|| ui.focusable(id, Rect::new(X, TOP, ENTRY_W_OPEN, AVATAR_ENTRY)));
    let focused = r.is_some_and(|r| r.focused);
    let pressed = r.is_some_and(|r| r.pressed);
    let current = view.current == Some("profile");
    let (fill, content) = entry_look(ui, id, focused, pressed, current, t);
    let pill = Rect::new(X, TOP, ENTRY_W + (ENTRY_W_OPEN - ENTRY_W) * g, AVATAR_ENTRY);
    let scale = ui.animate(id.with("scale"), if focused { focus::RAIL_ENTRY.focus_scale } else { 1.0 }, Motion::FOCUS);
    let name = view.me.map_or("", |m| m.name.as_str());
    ui.transform(Point::ZERO, scale, pill.center(), |ui| {
        if fill.a > 0 {
            ui.fill(pill, fill, theme::radius::CONTROL);
        }
        let circle = Rect::new(X + (ENTRY_W - AVATAR) / 2.0, TOP + (AVATAR_ENTRY - AVATAR) / 2.0, AVATAR, AVATAR);
        let color = avatar_color(view.me.and_then(|m| m.avatar_color.as_deref())).unwrap_or(theme::SURFACE3);
        let shown = view.avatar.as_ref().map(|req| ui.image_state(req, pley_base::Priority::Visible));
        if !shown.is_some_and(|s| s.opaque()) {
            ui.fill(circle, color, AVATAR / 2.0);
            let lum = (0.2126 * color.r as f32 + 0.7152 * color.g as f32 + 0.0722 * color.b as f32) / 255.0;
            let ink = if lum > 0.45 { theme::ON_ACCENT } else { theme::FG };
            let style = theme::LABEL.with_weight(pley_base::Weight::Bold);
            let letter = initial(name);
            let size = ui.measure(&letter, &style, &TextOpts::default());
            let at = Point::new(circle.center().x - size.w / 2.0, circle.center().y - size.h / 2.0);
            ui.text(at, &letter, &style, &TextOpts::default(), ink);
        }
        if let Some(s) = shown {
            ui.draw_image(circle, &s, &ImageOpts::cover().rounded(AVATAR / 2.0));
        }
        if view.offline_mark {
            let mark = Rect::new(circle.x + 22.0, circle.y + 22.0, MARK, MARK);
            ui.fill(mark.expand(2.0), theme::BG, (MARK + 4.0) / 2.0);
            ui.fill(mark, theme::SURFACE3, MARK / 2.0);
            ui.icon(Rect::new(mark.x + 3.5, mark.y + 3.5, 9.0, 9.0), Icon::ServerOff, theme::FG);
        }
        if t > 0.0 {
            ui.opacity(t, |ui| {
                let x = X + LABEL_X + label_slide(open, t);
                let block = theme::RAIL_PROFILE.line_height + theme::RAIL_SWITCH.line_height;
                let y = TOP + (AVATAR_ENTRY - block) / 2.0;
                ui.text(Point::new(x, y), name, &theme::RAIL_PROFILE, &TextOpts::line(LABEL_W), content);
                let below = y + theme::RAIL_PROFILE.line_height;
                ui.opacity(0.8, |ui| {
                    ui.text(Point::new(x, below), switch, &theme::RAIL_SWITCH, &TextOpts::line(LABEL_W), content)
                });
            });
        }
    });
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(id: i64, kind: LibraryKind) -> Library {
        Library { id, name: format!("Library {id}"), kind, ..Library::default() }
    }

    fn ids(e: &RailEntries) -> Vec<&str> {
        e.top.iter().map(|e| e.id.as_str()).collect()
    }

    // apps/android/ui/tv/src/test/.../shell/ShellRulesTest.kt
    #[test]
    fn up_to_six_libraries_have_entries_with_collections() {
        let libs: Vec<Library> = (1..=6).map(|i| lib(i, LibraryKind::Movies)).collect();
        for n in 4..=6 {
            let e = rail_entries(&libs[..n], false, false, None, false);
            let mut want = vec!["search".to_string(), "home".to_string()];
            want.extend((1..=n).map(|i| format!("library:{i}")));
            want.extend(["playlists".to_string(), "collections".to_string()]);
            assert_eq!(ids(&e), want, "{n} libraries");
        }
        let e = rail_entries(&libs[..3], false, false, None, false);
        assert_eq!(ids(&e), ["search", "home", "library:1", "library:2", "library:3", "playlists", "collections"]);
        assert_eq!(e.settings.id, "settings");
    }

    #[test]
    fn more_libraries_than_six_go_under_libraries() {
        let libs: Vec<Library> = (1..=7).map(|i| lib(i, LibraryKind::Movies)).collect();
        let e = rail_entries(&libs, false, false, None, false);
        assert_eq!(
            ids(&e),
            [
                "search",
                "home",
                "library:1",
                "library:2",
                "library:3",
                "library:4",
                "library:5",
                "library:6",
                "libraries",
                "playlists"
            ]
        );
    }

    #[test]
    fn nothing_makes_room_for_live_tv_now_playing_or_the_drive() {
        let libs = [lib(1, LibraryKind::Movies), lib(2, LibraryKind::Shows), lib(3, LibraryKind::Music)];
        let e = rail_entries(&libs, true, false, None, false);
        assert_eq!(
            ids(&e),
            ["search", "home", "library:1", "library:2", "library:3", "playlists", "collections", "now-playing"]
        );
        let e = rail_entries(&libs, true, true, discover(), true);
        assert_eq!(
            ids(&e),
            [
                "search",
                "home",
                "discover",
                "live-tv",
                "library:1",
                "library:2",
                "library:3",
                "playlists",
                "collections",
                "now-playing",
                "storage"
            ]
        );
        assert_eq!(current_entry(Some(&Route::Playlists), &e).as_deref(), Some("playlists"));
        assert_eq!(
            current_entry(Some(&Route::Library { id: 3, tab: None, sort: None }), &e).as_deref(),
            Some("library:3")
        );
    }

    #[test]
    fn collections_open_on_the_first_library_that_isnt_music() {
        let libs = [lib(7, LibraryKind::Music), lib(8, LibraryKind::Shows)];
        let e = rail_entries(&libs, false, false, None, false);
        let c = e.top.iter().find(|e| e.id == "collections").unwrap();
        assert_eq!(c.route, Route::Collections { library_id: Some(8) });
        assert_eq!(e.top.iter().find(|e| e.id == "library:7").unwrap().icon, Icon::Music);
        // No libraries: no Collections either.
        assert_eq!(ids(&rail_entries(&[], false, false, None, false)), ["search", "home", "playlists"]);
    }

    #[test]
    fn the_current_entry_follows_the_first_page() {
        let libs: Vec<Library> = (1..=8).map(|i| lib(i, LibraryKind::Movies)).collect();
        let e = rail_entries(&libs, false, false, None, false);
        let current = |r: Route| current_entry(Some(&r), &e);
        assert_eq!(current(Route::Home).as_deref(), Some("home"));
        assert_eq!(current(Route::Search { query: "x".into() }).as_deref(), Some("search"));
        assert_eq!(current(Route::Library { id: 6, tab: None, sort: None }).as_deref(), Some("library:6"));
        assert_eq!(current(Route::Library { id: 7, tab: None, sort: None }).as_deref(), Some("libraries"));
        assert_eq!(current(Route::Collections { library_id: Some(2) }).as_deref(), Some("library:2"));
        assert_eq!(current(Route::Playlists).as_deref(), Some("playlists"));
        assert_eq!(current(Route::Settings { section: None }).as_deref(), Some("settings"));
        assert_eq!(current(Route::Profiles).as_deref(), Some("profile"));
        assert_eq!(current(Route::Item { id: 3 }), None);
        assert_eq!(current_entry(None, &e), None);
    }

    #[test]
    fn the_drive_shows_in_the_rail_while_one_is_plugged_in() {
        let libs = [lib(1, LibraryKind::Movies), lib(2, LibraryKind::Shows)];
        assert!(!ids(&rail_entries(&libs, true, false, None, false)).contains(&"storage"));
        let e = rail_entries(&libs, true, false, None, true);
        assert_eq!(ids(&e)[e.top.len() - 2..], ["now-playing", "storage"]);
        let drive = e.top.last().unwrap();
        assert_eq!((drive.label.as_str(), drive.icon), ("USB drive", Icon::Usb));
        assert_eq!(drive.route, Route::Storage { path: None });
        // Every folder of the drive is under it.
        let folder = Route::Storage { path: Some("/tmp/usb/sda/sda1/Films".into()) };
        assert_eq!(current_entry(Some(&folder), &e).as_deref(), Some("storage"));
    }

    fn discover() -> Option<RailEntry> {
        Some(RailEntry::new("discover", "Discover", Icon::Compass, Route::Discover))
    }

    #[test]
    fn every_entry_is_listed_in_its_order_whatever_else_is_there() {
        let kinds = [LibraryKind::Movies, LibraryKind::Shows, LibraryKind::Music];
        let requests = [None, discover(), Some(RailEntry::new("requests", "Requests", Icon::Inbox, Route::Requests))];
        for n in 0..=8 {
            let libs: Vec<Library> = (1..=n).map(|i| lib(i, kinds[i as usize % 3])).collect();
            for bits in 0..8 {
                let (queued, live, drive) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
                for req in &requests {
                    let e = rail_entries(&libs, queued, live, req.clone(), drive);
                    let case = format!("{n} libraries, {:?}", ids(&e));
                    let mut want: Vec<String> = vec!["search".into(), "home".into()];
                    want.extend(req.iter().map(|r| r.id.clone()));
                    want.extend(live.then(|| "live-tv".to_string()));
                    want.extend(libs.iter().take(MAX_LIBRARIES).map(|l| format!("library:{}", l.id)));
                    want.extend((libs.len() > MAX_LIBRARIES).then(|| "libraries".to_string()));
                    want.push("playlists".into());
                    want.extend((1..=MAX_LIBRARIES).contains(&libs.len()).then(|| "collections".to_string()));
                    want.extend(queued.then(|| "now-playing".to_string()));
                    want.extend(drive.then(|| "storage".to_string()));
                    assert_eq!(ids(&e), want, "{case}");
                    // Every library is in reach: its own entry, else Libraries.
                    for l in &libs {
                        let root = Route::Library { id: l.id, tab: None, sort: None };
                        assert!(current_entry(Some(&root), &e).is_some(), "{case}");
                    }
                }
            }
        }
    }

    #[test]
    fn avatars_take_six_digit_colours_only() {
        assert_eq!(avatar_color(Some("#e07a5f")), Some(Color::rgb(0xe07a5f)));
        assert_eq!(avatar_color(Some("e07a5f")), None);
        assert_eq!(avatar_color(Some("#fff")), None);
        assert_eq!(avatar_color(None), None);
        assert_eq!(initial(" alex"), "A");
        assert_eq!(initial(""), "?");
    }

    /// The open panel's width in a frame, where it is drawn, and where its edge starts.
    fn panel(list: &pley_base::DisplayList) -> Option<(f32, f32)> {
        use pley_base::Cmd;
        let fill = list.cmds.iter().find_map(|c| match c {
            Cmd::Fill { rect, color, .. } if *color == theme::RAIL_PANEL && rect.h == theme::CANVAS_H => Some(rect.w),
            _ => None,
        })?;
        let edge = list.cmds.iter().find_map(|c| match c {
            Cmd::LinearGradient { rect, .. } if rect.x > 0.0 && rect.h == theme::CANVAS_H => Some(rect.x),
            _ => None,
        })?;
        Some((fill, edge))
    }

    /// Where the label `text` is drawn.
    fn label_x(list: &pley_base::DisplayList, text: &str) -> Option<f32> {
        list.cmds.iter().find_map(|c| match c {
            pley_base::Cmd::Text { origin, layout, .. } if &*layout.text == text => Some(origin.x),
            _ => None,
        })
    }

    #[test]
    fn the_panel_widens_out_of_the_closed_rail_and_the_labels_slide_in() {
        use pley_ui::testing::{Harness, Step};
        use std::cell::{Cell, RefCell};
        let entries = rail_entries(&[], false, false, None, false);
        let rail = RefCell::new(Rail::default());
        let toggle = Cell::new(false);
        let mut screen = |ui: &mut Ui| {
            let mut r = rail.borrow_mut();
            if toggle.take() {
                if r.is_open() { r.close() } else { r.open(ui, "home") }
            }
            let view = RailView {
                entries: &entries,
                current: Some("home"),
                me: None,
                avatar: None,
                offline_mark: false,
                page_modal: false,
            };
            r.ui(ui, &view);
        };
        let mut h = Harness::new();
        h.idle(&mut screen);
        toggle.set(true);
        h.idle(&mut screen);
        let frames = h.run(&[Step::Wait(0.5)], &mut screen);
        let panels: Vec<(f32, f32)> = frames.iter().filter_map(|f| panel(&f.list)).collect();
        // From the closed rail's width to the open one's, the edge on the panel's right.
        assert!(panels.len() > 5);
        assert!(panels.windows(2).all(|w| w[1].0 >= w[0].0));
        assert!(panels.iter().all(|&(w, edge)| w == edge && w > CLOSED_WIDTH));
        assert!(panels[0].0 < (CLOSED_WIDTH + OPEN_WIDTH) / 2.0, "{panels:?}");
        assert_eq!(panels.last().unwrap().0, OPEN_WIDTH);
        // The labels slide in 24 dp as they fade in.
        let xs: Vec<f32> = frames.iter().filter_map(|f| label_x(&f.list, "Search")).collect();
        assert!(xs[0] > X + LABEL_X + 4.0 && xs[0] <= X + LABEL_X + LABEL_SLIDE, "{xs:?}");
        assert!(xs.windows(2).all(|w| w[1] <= w[0]));
        assert_eq!(*xs.last().unwrap(), X + LABEL_X);
        // Closing, they fade where they are.
        toggle.set(true);
        h.idle(&mut screen);
        let frames = h.run(&[Step::Wait(0.5)], &mut screen);
        let xs: Vec<f32> = frames.iter().filter_map(|f| label_x(&f.list, "Search")).collect();
        assert!(!xs.is_empty() && xs.iter().all(|&x| x == X + LABEL_X), "{xs:?}");
    }

    #[test]
    fn nine_entries_fit_between_the_avatar_and_settings() {
        assert_eq!((ENTRY_H, PITCH, AVATAR_ENTRY), (40.0, 44.0, 48.0));
        assert_eq!(FIRST_Y, 67.0);
        assert_eq!(SETTINGS_Y, 473.0);
        assert_eq!(theme::CANVAS_H - SETTINGS_Y - ENTRY_H, BOTTOM);
        // Whole entries in the view, the last one's gap aside.
        assert_eq!(((VIEW_H + GAP) / PITCH).floor(), 9.0);
        assert_eq!(FITS, 9);
        assert_eq!(CLOSED_WIDTH, X + ENTRY_W + 8.0);
        assert_eq!(OPEN_WIDTH, X + ENTRY_W_OPEN + 8.0);
    }

    /// A rail of fourteen entries: Search, Home, Discover, Live TV, six libraries, Libraries,
    /// Playlists, Now Playing and the drive.
    fn long_rail() -> RailEntries {
        let libs: Vec<Library> = (1..=8).map(|i| lib(i, LibraryKind::Movies)).collect();
        rail_entries(&libs, true, true, discover(), true)
    }

    /// Where each entry's label is drawn, as the entry's top.
    fn tops(list: &pley_base::DisplayList, entries: &RailEntries) -> Vec<f32> {
        let lift = (ENTRY_H - theme::LABEL.line_height) / 2.0;
        entries.top.iter().map(|e| label_y(list, &e.label).expect("every entry is drawn") - lift).collect()
    }

    fn label_y(list: &pley_base::DisplayList, text: &str) -> Option<f32> {
        list.cmds.iter().find_map(|c| match c {
            pley_base::Cmd::Text { origin, layout, .. } if &*layout.text == text => Some(origin.y),
            _ => None,
        })
    }

    /// Whether the entry at `top` shows whole between the avatar and Settings.
    fn whole(top: f32) -> bool {
        top >= FIRST_Y - 0.01 && top + ENTRY_H <= FIRST_Y + VIEW_H + 0.01
    }

    /// Draws a rail of `entries` with `current`, open on `open` when given.
    fn rail_screen<'a>(
        entries: &'a RailEntries,
        rail: &'a std::cell::RefCell<Rail>,
        open: &'a std::cell::Cell<Option<&'static str>>,
        current: &'a str,
    ) -> impl FnMut(&mut Ui) + 'a {
        move |ui: &mut Ui| {
            let mut r = rail.borrow_mut();
            if let Some(at) = open.take() {
                r.open(ui, at);
            }
            let view = RailView {
                entries,
                current: Some(current),
                me: None,
                avatar: None,
                offline_mark: false,
                page_modal: false,
            };
            r.ui(ui, &view);
        }
    }

    /// Where entry `i` of `count` starts while it has focus: down to the fourth place, then there
    /// while the entries scroll by whole entries, until the last nine show.
    fn resting_top(i: usize, count: usize) -> f32 {
        FIRST_Y + i as f32 * PITCH - ((i as f32 - 3.0) * PITCH).clamp(0.0, (count - 9) as f32 * PITCH)
    }

    #[test]
    fn the_focused_entry_stays_fourth_from_the_top_as_the_entries_scroll_the_same_both_ways() {
        use pley_base::Key;
        use pley_ui::testing::{Harness, Step};
        use std::cell::{Cell, RefCell};
        let entries = long_rail();
        let n = entries.top.len();
        assert_eq!(n, 14);
        let rail = RefCell::new(Rail::default());
        let open = Cell::new(Some("search"));
        let mut screen = rail_screen(&entries, &rail, &open, "home");
        let mut h = Harness::new();
        h.idle(&mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(tops(&f.list, &entries)[0], FIRST_Y, "the first entry starts at the top");
        // Down along the rail: focus goes down to the fourth entry shown, stays there while the
        // entries move under it an entry a press, then goes on to the last. They move by whole
        // entries, so the first one shown starts at the top.
        for i in 1..n {
            h.step(Step::Press(Key::Down), &mut screen);
            let f = h.step(Step::Wait(0.4), &mut screen);
            assert_eq!(f.focused, Some(Rail::entry_focus_id(&entries.top[i].id)));
            let tops = tops(&f.list, &entries);
            assert_eq!(tops[i], resting_top(i, n), "entry {i}");
            assert!(tops.iter().all(|&t| (t - FIRST_Y).rem_euclid(PITCH) == 0.0), "{tops:?}");
        }
        // At the end the last one ends at 459, and Down goes on to Settings, moving nothing.
        assert_eq!(resting_top(n - 1, n) + ENTRY_H, FIRST_Y + 9.0 * PITCH - GAP);
        let f = h.step(Step::Press(Key::Down), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("settings")));
        assert_eq!(label_y(&f.list, "Settings"), Some(SETTINGS_Y + (ENTRY_H - theme::LABEL.line_height) / 2.0));
        let f = h.step(Step::Wait(0.4), &mut screen);
        assert_eq!(tops(&f.list, &entries)[n - 1], resting_top(n - 1, n));
        // Back up: each entry where it was on the way down.
        for i in (0..n).rev() {
            h.step(Step::Press(Key::Up), &mut screen);
            let f = h.step(Step::Wait(0.4), &mut screen);
            assert_eq!(f.focused, Some(Rail::entry_focus_id(&entries.top[i].id)));
            assert_eq!(tops(&f.list, &entries)[i], resting_top(i, n), "entry {i}");
        }
        assert_eq!(tops(&h.idle(&mut screen).list, &entries)[0], FIRST_Y);
    }

    #[test]
    fn the_rail_opens_with_its_current_entry_in_view_and_cuts_off_the_rest() {
        use pley_base::Cmd;
        use pley_ui::testing::{Harness, Step};
        use std::cell::{Cell, RefCell};
        let entries = long_rail();
        let rail = RefCell::new(Rail::default());
        let open = Cell::new(None);
        // Closed on the drive's page: its icon shows, scrolled up to it.
        let mut screen = rail_screen(&entries, &rail, &open, "storage");
        let mut h = Harness::new();
        h.idle(&mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        let usb = f.list.cmds.iter().find_map(|c| match c {
            Cmd::Icon { rect, icon: Icon::Usb, .. } => Some(rect.y),
            _ => None,
        });
        let usb = usb.expect("the drive's icon is drawn");
        assert_eq!(usb - (ENTRY_H - ICON) / 2.0, resting_top(13, 14), "the drive's icon at {usb}");
        assert_eq!(rail.borrow().scroll, 5.0 * PITCH);
        // Opened there, nothing moves: the view is at its end, the last nine whole.
        open.set(Some("storage"));
        h.idle(&mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        let tops = tops(&f.list, &entries);
        assert_eq!(rail.borrow().scroll, 5.0 * PITCH);
        assert!(tops[5..].iter().all(|&t| whole(t)), "{tops:?}");
        assert!(!whole(tops[0]), "the first entries are scrolled away");
        // The entries are drawn inside a clip from the avatar's foot to Settings, which is outside.
        let at = |pred: &dyn Fn(&Cmd) -> bool| f.list.cmds.iter().position(pred).unwrap();
        let clip = at(&|c| matches!(c, Cmd::PushClip { rect } if rect.y == CLIP_TOP && rect.bottom() == SETTINGS_Y));
        let search = at(&|c| matches!(c, Cmd::Text { layout, .. } if &*layout.text == "Search"));
        let unclip = clip + f.list.cmds[clip..].iter().position(|c| matches!(c, Cmd::PopClip)).unwrap();
        let settings = at(&|c| matches!(c, Cmd::Text { layout, .. } if &*layout.text == "Settings"));
        assert!(clip < search && search < unclip && unclip < settings);
        assert_eq!(CLIP_TOP, TOP + AVATAR_ENTRY);
    }

    /// packages/tv/test/fixtures/rail.json, which the web TV app's and the native Samsung app's
    /// tests read too: the rail's entries and how they scroll.
    const SHARED: &str = include_str!("../../../../../packages/tv/test/fixtures/rail.json");

    #[test]
    fn the_entries_and_their_scroll_follow_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(SHARED).unwrap();
        let cases = fixture["entries"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["in"];
            let libs: Vec<Library> = (input["libraries"].as_array().unwrap().iter().enumerate())
                .map(|(i, kind)| lib(i as i64 + 1, serde_json::from_value(kind.clone()).unwrap()))
                .collect();
            let (now_playing, live) = (input["nowPlaying"].as_bool().unwrap(), input["live"].as_bool().unwrap());
            let e = rail_entries(&libs, now_playing, live, None, false);
            // The web TV app calls Live TV's entry "live"; this app, as Android's, "live-tv".
            let got: Vec<&str> = ids(&e).into_iter().map(|id| if id == "live-tv" { "live" } else { id }).collect();
            let want: Vec<&str> = case["out"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(got, want, "{input}");
        }
        let cases = fixture["scroll"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["in"];
            let focused = input["focused"].as_i64().unwrap() as isize;
            let count = input["count"].as_u64().unwrap() as usize;
            // With an entry focused, where it was scrolled before makes no difference.
            let from = match input["scroll"].as_f64() {
                Some(scroll) => vec![scroll as f32],
                None => vec![0.0, 37.0, 1000.0],
            };
            for scroll in from {
                assert_eq!(
                    rail_scroll(scroll, focused, count),
                    case["out"].as_f64().unwrap() as f32,
                    "{input} from {scroll}"
                );
            }
        }
    }

    /// Where the first entry's top is drawn: `FIRST_Y` less the scroll.
    /// Where the first entry starts, by its icon: a closed rail draws no labels.
    fn first_top(list: &pley_base::DisplayList, entries: &RailEntries) -> f32 {
        let icon = entries.top[0].icon;
        let y = list.cmds.iter().find_map(|c| match c {
            pley_base::Cmd::Icon { rect, icon: drawn, .. } if *drawn == icon => Some(rect.y),
            _ => None,
        });
        y.expect("the first entry's icon is drawn") - (ENTRY_H - ICON) / 2.0
    }

    #[test]
    fn the_closed_rail_places_its_entry_by_the_rule_and_the_pointer_only_brings_one_in_whole() {
        use pley_base::Key;
        use pley_ui::testing::{Harness, Step};
        use std::cell::{Cell, RefCell};
        // Twelve entries: Search, Home, Live TV, six libraries, Playlists, Collections, Now
        // Playing. Closed on the sixth library's page (entry 8), placed as focus on it would be:
        // the entries at their end, the last nine whole.
        let libs: Vec<Library> = (1..=6).map(|i| lib(i, LibraryKind::Movies)).collect();
        let entries = rail_entries(&libs, true, true, None, false);
        assert_eq!((entries.top.len(), entries.top[8].id.as_str()), (12, "library:6"));
        let rail = RefCell::new(Rail::default());
        let open = Cell::new(None);
        let mut screen = rail_screen(&entries, &rail, &open, "library:6");
        let mut h = Harness::new();
        h.idle(&mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(rail.borrow().scroll, 132.0);
        assert_eq!(first_top(&f.list, &entries), FIRST_Y - 132.0);
        // Opened there with keys, nothing moves.
        open.set(Some("library:6"));
        h.idle(&mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("library:6")));
        assert_eq!(first_top(&f.list, &entries), FIRST_Y - 132.0);
        // Down and back up at the end: focus moves, the entries don't.
        h.step(Step::Press(Key::Down), &mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("playlists")));
        assert_eq!(first_top(&f.list, &entries), FIRST_Y - 132.0);
        h.step(Step::Press(Key::Up), &mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(first_top(&f.list, &entries), FIRST_Y - 132.0);
        // Up to entry 5, fourth from the top, the entries moving under it.
        for _ in 0..3 {
            h.step(Step::Press(Key::Up), &mut screen);
        }
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("library:3")));
        assert_eq!(first_top(&f.list, &entries), FIRST_Y - 88.0);
        // Up to entry 1 brings Search back to the top.
        for _ in 0..4 {
            h.step(Step::Press(Key::Up), &mut screen);
        }
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("home")));
        assert_eq!(first_top(&f.list, &entries), FIRST_Y);
        // The pointer on entry 9, cut off above Settings, brings it in whole and no further.
        h.step(Step::Point(Point::new(30.0, FIRST_Y + 9.0 * PITCH + 2.0)), &mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("playlists")));
        let pointed = 9.0 * PITCH + ENTRY_H - VIEW_H;
        assert_eq!(first_top(&f.list, &entries), FIRST_Y - pointed);
        // On entry 4, in view, nothing moves under it.
        h.step(Step::Point(Point::new(30.0, FIRST_Y + 4.0 * PITCH - pointed + 2.0)), &mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id(&entries.top[4].id)));
        assert_eq!(first_top(&f.list, &entries), FIRST_Y - pointed);
    }

    #[test]
    fn entries_made_again_keep_focus_and_place_the_focused_one_by_the_rule() {
        use pley_base::Key;
        use pley_ui::testing::{Harness, Step};
        use std::cell::{Cell, RefCell};
        // Eleven entries, Playlists last (entry 10); then Live TV comes before it and Now
        // Playing after it, so it is entry 11 of 13.
        let libs: Vec<Library> = (1..=8).map(|i| lib(i, LibraryKind::Movies)).collect();
        let before = rail_entries(&libs, false, false, discover(), false);
        let after = rail_entries(&libs, true, true, discover(), false);
        assert_eq!((before.top.len(), before.top[10].id.as_str()), (11, "playlists"));
        assert_eq!((after.top.len(), after.top[11].id.as_str()), (13, "playlists"));
        let entries = RefCell::new(before);
        let rail = RefCell::new(Rail::default());
        let open = Cell::new(Some("playlists"));
        let mut screen = |ui: &mut Ui| {
            let mut r = rail.borrow_mut();
            if let Some(at) = open.take() {
                r.open(ui, at);
            }
            let entries = entries.borrow();
            let view = RailView {
                entries: &entries,
                current: Some("playlists"),
                me: None,
                avatar: None,
                offline_mark: false,
                page_modal: false,
            };
            r.ui(ui, &view);
        };
        let mut h = Harness::new();
        h.idle(&mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("playlists")));
        assert_eq!(tops(&f.list, &entries.borrow())[10], resting_top(10, 11), "Playlists last of the nine shown");
        // The entries change under it: focus stays on Playlists, placed again by the rule for
        // its new place, which shows Now Playing after it whole.
        *entries.borrow_mut() = after;
        h.idle(&mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("playlists")));
        assert_eq!(rail.borrow().scroll, 4.0 * PITCH);
        let tops_now = tops(&f.list, &entries.borrow());
        assert_eq!(tops_now[11], resting_top(11, 13));
        assert!(whole(tops_now[12]), "Now Playing in view");
        // Down to Now Playing, the last: nothing moves.
        h.step(Step::Press(Key::Down), &mut screen);
        let f = h.step(Step::Wait(0.5), &mut screen);
        assert_eq!(f.focused, Some(Rail::entry_focus_id("now-playing")));
        assert_eq!(rail.borrow().scroll, 4.0 * PITCH);
        assert_eq!(tops(&f.list, &entries.borrow())[12] + ENTRY_H, FIRST_Y + 9.0 * PITCH - GAP);
    }

    #[test]
    fn the_pointer_finds_the_entry_shown_under_it() {
        let entries = long_rail();
        let view = RailView {
            entries: &entries,
            current: None,
            me: None,
            avatar: None,
            offline_mark: false,
            page_modal: false,
        };
        let at = |y: f32, scroll: f32| entry_at(&view, Point::new(30.0, y), scroll);
        assert_eq!(at(FIRST_Y + 5.0, 0.0).as_deref(), Some("search"));
        assert_eq!(at(FIRST_Y + PITCH + 5.0, 0.0).as_deref(), Some("home"));
        assert_eq!(at(FIRST_Y + ENTRY_H + 1.0, 0.0), None, "the gap between two");
        // Scrolled by three entries, the fourth is at the top.
        assert_eq!(at(FIRST_Y + 5.0, 3.0 * PITCH).as_deref(), Some(entries.top[3].id.as_str()));
        assert_eq!(at(TOP + 5.0, 3.0 * PITCH).as_deref(), Some("profile"));
        assert_eq!(at(SETTINGS_Y + 5.0, 3.0 * PITCH).as_deref(), Some("settings"));
        assert_eq!(at(FIRST_Y - 2.0, 0.0), None, "under the avatar, before the first entry");
    }
}
