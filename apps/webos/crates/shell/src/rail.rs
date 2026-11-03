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

use pley_base::{Color, Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::model::{Library, LibraryKind, Me, RequestSummary};
use pley_core::requests::{has_requests, may_ask, requests_count, requests_title};
use pley_ui::theme::{self, focus};
use pley_ui::{Dir, Dirs, Easing, Edge, Group, Id, ImageOpts, Motion, Ui};

use crate::route::Route;

/// At most this many libraries have entries of their own (`MAX_LIBRARIES`).
pub const MAX_LIBRARIES: usize = 3;
/// Entries between the avatar and Settings, at most: their pitch fills the rail (`MAX_ITEMS`).
pub const MAX_ITEMS: usize = 7;

/// The rail's width while closed: 8 + 56 + 8.
pub const CLOSED_WIDTH: f32 = theme::RAIL_COLLAPSED;
/// The open panel's width: 8 + 256 + 8 (a 64 dp soft edge beyond it).
pub const OPEN_WIDTH: f32 = theme::RAIL_EXPANDED;

const X: f32 = 8.0;
const TOP: f32 = 15.0;
const BOTTOM: f32 = 27.0;
const AVATAR_ENTRY: f32 = 56.0;
const AVATAR_GAP: f32 = 4.0;
const ENTRY_H: f32 = 48.0;
const ENTRY_W: f32 = 56.0;
const ENTRY_W_OPEN: f32 = 256.0;
const PITCH: f32 = 56.0;
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
        RailEntry::new("discover", "Discover", Icon::Compass, Route::Discover)
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

    /// Whether the Libraries page lists Playlists after the libraries: the rail had no room for
    /// it, and the page is the way there.
    pub fn playlists_on_libraries_page(&self) -> bool {
        !self.has("playlists")
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
/// [`requests_entry`]), Live TV where the server has channels
/// for this profile, up to three libraries, then Libraries when there are more, Playlists, and
/// Collections when there are one to three libraries; Now Playing while music has a queue; USB
/// drive last while the app can read a drive plugged into the TV (the Samsung app's `Pages.cs`);
/// Settings at the foot. Seven fit: past that Collections goes first, then libraries from the
/// last, Libraries listing them. With Discover, Live TV, Now Playing and the drive all there, that
/// still leaves eight, and no library can make way: Playlists then leaves the rail for the
/// Libraries page, which lists it after the libraries
/// ([`RailEntries::playlists_on_libraries_page`]). By then a lone library has become Libraries
/// too, so the rail always reaches Playlists, one way or the other.
pub fn rail_entries(
    libraries: &[Library],
    now_playing: bool,
    live: bool,
    requests: Option<RailEntry>,
    drive: bool,
) -> RailEntries {
    let mut shown = libraries.len().min(MAX_LIBRARIES);
    let mut collections = !libraries.is_empty() && libraries.len() <= MAX_LIBRARIES;
    let build = |shown: usize, collections: bool| {
        let mut top = vec![
            RailEntry::new("search", "Search", Icon::Search, Route::Search { query: String::new() }),
            RailEntry::new("home", "Home", Icon::House, Route::Home),
        ];
        top.extend(requests.clone());
        if live {
            top.push(RailEntry::new("live-tv", "Live TV", Icon::TvMinimal, Route::LiveTv));
        }
        for l in &libraries[..shown] {
            let route = Route::Library { id: l.id, tab: None, sort: None };
            top.push(RailEntry::new(format!("library:{}", l.id), l.name.clone(), library_icon(l.kind), route));
        }
        if libraries.len() > shown {
            top.push(RailEntry::new("libraries", "Libraries", Icon::Layers, Route::Libraries { sort: false }));
        }
        top.push(RailEntry::new("playlists", "Playlists", Icon::ListMusic, Route::Playlists));
        if collections {
            let first = libraries.iter().find(|l| l.kind != LibraryKind::Music).unwrap_or(&libraries[0]);
            let route = Route::Collections { library_id: Some(first.id) };
            top.push(RailEntry::new("collections", "Collections", Icon::Layers, route));
        }
        if now_playing {
            top.push(RailEntry::new("now-playing", "Now Playing", Icon::AudioLines, Route::NowPlaying));
        }
        if drive {
            top.push(RailEntry::new("storage", "USB drive", Icon::Usb, Route::Storage { path: None }));
        }
        top
    };
    let mut top = build(shown, collections);
    // The first library to go makes no room, "Libraries" coming in for it; the loop goes on, so a
    // lone library has become "Libraries" by the time Playlists leaves below.
    while top.len() > MAX_ITEMS && (collections || shown > 0) {
        if collections {
            collections = false;
        } else {
            shown -= 1;
        }
        top = build(shown, collections);
    }
    if top.len() > MAX_ITEMS {
        // Only the drive brings it here, with no library left to make way: Playlists goes to the
        // Libraries page, which the rail's "Libraries" opens.
        top.retain(|e| e.id != "playlists");
    }
    RailEntries {
        top,
        settings: RailEntry::new("settings", "Settings", Icon::Settings, Route::Settings { section: None }),
    }
}

/// The entry of the page at the root of the back stack (`railCurrent`): the entry that opened it,
/// Libraries for a library or Playlists not in the rail, the avatar for Profiles, or none.
pub fn current_entry(root: Option<&Route>, entries: &RailEntries) -> Option<String> {
    let top = &entries.top;
    let has = |id: &str| entries.has(id).then(|| id.to_string());
    match root? {
        Route::Library { id, .. } => has(&format!("library:{id}")).or_else(|| has("libraries")),
        Route::Playlists => has("playlists").or_else(|| has("libraries")),
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

        let mut all: Vec<(&RailEntry, f32)> =
            view.entries.top.iter().enumerate().map(|(i, e)| (e, FIRST_Y + i as f32 * PITCH)).collect();
        all.push((&view.entries.settings, SETTINGS_Y));

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
                    chosen = Some(RailEntry::new("profile", "Profiles", Icon::Users, Route::Profiles));
                }
            }
            for &(entry, y) in &all {
                if let Some(r) = entry_ui(ui, entry, y, view.current == Some(entry.id.as_str()), open, t, g) {
                    if r.focused {
                        focus_now = Some(r.id);
                    }
                    if r.activated {
                        chosen = Some(entry.clone());
                    }
                }
            }
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
                && let Some(id) = entry_at(view, p)
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

const FIRST_Y: f32 = TOP + AVATAR_ENTRY + AVATAR_GAP;
const SETTINGS_Y: f32 = theme::CANVAS_H - BOTTOM - ENTRY_H;

/// The entry under the pointer at `p` while the rail is closed.
fn entry_at(view: &RailView, p: Point) -> Option<String> {
    if p.x >= CLOSED_WIDTH {
        return None;
    }
    if (TOP..TOP + AVATAR_ENTRY).contains(&p.y) {
        return Some("profile".into());
    }
    if (SETTINGS_Y..SETTINGS_Y + ENTRY_H).contains(&p.y) {
        return Some(view.entries.settings.id.clone());
    }
    let i = ((p.y - FIRST_Y) / PITCH).floor();
    let inside = p.y >= FIRST_Y && p.y - FIRST_Y - i * PITCH < ENTRY_H;
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
        Some((n, _)) => format!("{}, {n}", e.label),
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
        ui.icon(Rect::new(X + 18.0, y + 14.0, ICON, ICON), e.icon, content);
        if let Some((count, danger)) = e.badge {
            badge(ui, Point::new(X + 29.0, y + 6.0), count, danger, focused);
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
    ui.name(id, || match view.me {
        Some(m) => format!("{}, Switch profile", m.name),
        None => "Switch profile".to_string(),
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
        let circle = Rect::new(X + 12.0, TOP + 12.0, AVATAR, AVATAR);
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
            let mark = Rect::new(X + 34.0, TOP + 34.0, MARK, MARK);
            ui.fill(mark.expand(2.0), theme::BG, (MARK + 4.0) / 2.0);
            ui.fill(mark, theme::SURFACE3, MARK / 2.0);
            ui.icon(Rect::new(mark.x + 3.5, mark.y + 3.5, 9.0, 9.0), Icon::ServerOff, theme::FG);
        }
        if t > 0.0 {
            ui.opacity(t, |ui| {
                let x = X + LABEL_X + label_slide(open, t);
                ui.text(Point::new(x, TOP + 10.0), name, &theme::RAIL_PROFILE, &TextOpts::line(LABEL_W), content);
                let below = TOP + 10.0 + theme::RAIL_PROFILE.line_height;
                ui.opacity(0.8, |ui| {
                    ui.text(
                        Point::new(x, below),
                        "Switch profile",
                        &theme::RAIL_SWITCH,
                        &TextOpts::line(LABEL_W),
                        content,
                    )
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
    fn three_libraries_fit_with_collections() {
        let libs = [lib(1, LibraryKind::Movies), lib(2, LibraryKind::Shows), lib(3, LibraryKind::Music)];
        let e = rail_entries(&libs, false, false, None, false);
        assert_eq!(ids(&e), ["search", "home", "library:1", "library:2", "library:3", "playlists", "collections"]);
        assert_eq!(e.settings.id, "settings");
    }

    #[test]
    fn more_libraries_than_fit_go_under_libraries() {
        let libs: Vec<Library> = (1..=5).map(|i| lib(i, LibraryKind::Movies)).collect();
        let e = rail_entries(&libs, false, false, None, false);
        assert_eq!(ids(&e), ["search", "home", "library:1", "library:2", "library:3", "libraries", "playlists"]);
    }

    #[test]
    fn now_playing_takes_the_place_of_collections() {
        let libs = [lib(1, LibraryKind::Movies), lib(2, LibraryKind::Shows), lib(3, LibraryKind::Music)];
        let e = rail_entries(&libs, true, false, None, false);
        assert_eq!(ids(&e), ["search", "home", "library:1", "library:2", "library:3", "playlists", "now-playing"]);
    }

    #[test]
    fn live_tv_comes_after_home_and_makes_room() {
        let libs = [lib(1, LibraryKind::Movies), lib(2, LibraryKind::Shows), lib(3, LibraryKind::Music)];
        let e = rail_entries(&libs, false, true, None, false);
        assert_eq!(ids(&e), ["search", "home", "live-tv", "library:1", "library:2", "library:3", "playlists"]);
        // With music as well, the last two libraries give way to Libraries.
        let e = rail_entries(&libs, true, true, None, false);
        assert_eq!(ids(&e), ["search", "home", "live-tv", "library:1", "libraries", "playlists", "now-playing"]);
        assert!(e.top.len() <= MAX_ITEMS);
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
        let libs: Vec<Library> = (1..=5).map(|i| lib(i, LibraryKind::Movies)).collect();
        let e = rail_entries(&libs, false, false, None, false);
        let current = |r: Route| current_entry(Some(&r), &e);
        assert_eq!(current(Route::Home).as_deref(), Some("home"));
        assert_eq!(current(Route::Search { query: "x".into() }).as_deref(), Some("search"));
        assert_eq!(current(Route::Library { id: 2, tab: None, sort: None }).as_deref(), Some("library:2"));
        assert_eq!(current(Route::Library { id: 5, tab: None, sort: None }).as_deref(), Some("libraries"));
        assert_eq!(current(Route::Collections { library_id: Some(2) }).as_deref(), Some("library:2"));
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
        assert!(e.top.len() <= MAX_ITEMS);
    }

    fn discover() -> Option<RailEntry> {
        Some(RailEntry::new("discover", "Discover", Icon::Compass, Route::Discover))
    }

    #[test]
    fn with_no_library_left_to_make_way_playlists_leaves_for_the_libraries_page() {
        let libs = [lib(1, LibraryKind::Movies), lib(2, LibraryKind::Shows), lib(3, LibraryKind::Music)];
        let e = rail_entries(&libs, true, true, discover(), true);
        assert_eq!(ids(&e), ["search", "home", "discover", "live-tv", "libraries", "now-playing", "storage"]);
        assert!(e.playlists_on_libraries_page());
        // Playlists, opened from that page or left on top as the drive came in, is under Libraries.
        assert_eq!(current_entry(Some(&Route::Libraries { sort: false }), &e).as_deref(), Some("libraries"));
        assert_eq!(current_entry(Some(&Route::Playlists), &e).as_deref(), Some("libraries"));
        // Without the drive, Playlists keeps its place, and the page doesn't list it.
        let e = rail_entries(&libs, true, true, discover(), false);
        assert_eq!(ids(&e), ["search", "home", "discover", "live-tv", "libraries", "playlists", "now-playing"]);
        assert!(!e.playlists_on_libraries_page());
        assert_eq!(current_entry(Some(&Route::Playlists), &e).as_deref(), Some("playlists"));
        // Room for both: nothing gives way.
        let e = rail_entries(&libs[..1], false, false, None, true);
        assert_eq!(ids(&e), ["search", "home", "library:1", "playlists", "collections", "storage"]);
    }

    #[test]
    fn a_lone_library_becomes_libraries_when_playlists_leaves() {
        let libs = [lib(1, LibraryKind::Movies)];
        // Collections making way is enough: the library keeps its entry.
        let e = rail_entries(&libs, true, true, discover(), false);
        assert_eq!(ids(&e), ["search", "home", "discover", "live-tv", "library:1", "playlists", "now-playing"]);
        // With the drive too, Libraries, which lists the library and then Playlists, takes its place.
        let e = rail_entries(&libs, true, true, discover(), true);
        assert_eq!(ids(&e), ["search", "home", "discover", "live-tv", "libraries", "now-playing", "storage"]);
        let libraries = e.top.iter().find(|e| e.id == "libraries").unwrap();
        assert_eq!((libraries.label.as_str(), libraries.icon), ("Libraries", Icon::Layers));
        assert_eq!(libraries.route, Route::Libraries { sort: false });
        assert_eq!(
            current_entry(Some(&Route::Library { id: 1, tab: None, sort: None }), &e).as_deref(),
            Some("libraries")
        );
        assert_eq!(current_entry(Some(&Route::Playlists), &e).as_deref(), Some("libraries"));
        // Two libraries go the same way.
        let libs = [lib(1, LibraryKind::Movies), lib(2, LibraryKind::Music)];
        let e = rail_entries(&libs, true, true, discover(), true);
        assert_eq!(ids(&e), ["search", "home", "discover", "live-tv", "libraries", "now-playing", "storage"]);
    }

    #[test]
    fn playlists_is_always_in_reach_and_the_rail_never_holds_more_than_seven() {
        let kinds = [LibraryKind::Movies, LibraryKind::Shows, LibraryKind::Music];
        let requests = [None, discover(), Some(RailEntry::new("requests", "Requests", Icon::Inbox, Route::Requests))];
        for n in 0..=6 {
            let libs: Vec<Library> = (1..=n).map(|i| lib(i, kinds[i as usize % 3])).collect();
            for bits in 0..8 {
                let (queued, live, drive) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
                for req in &requests {
                    let e = rail_entries(&libs, queued, live, req.clone(), drive);
                    let case = format!("{n} libraries, {:?}", ids(&e));
                    assert!(e.top.len() <= MAX_ITEMS, "{case}");
                    let playlists = current_entry(Some(&Route::Playlists), &e);
                    if e.playlists_on_libraries_page() {
                        // In reach through Libraries, and only once nothing else could make way.
                        assert!(e.has("libraries") && !e.has("collections"), "{case}");
                        assert!(!e.top.iter().any(|e| e.id.starts_with("library:")), "{case}");
                        assert_eq!(playlists.as_deref(), Some("libraries"), "{case}");
                    } else {
                        assert_eq!(playlists.as_deref(), Some("playlists"), "{case}");
                    }
                    // Every library is in reach too: its own entry, else Libraries.
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
    fn the_geometry_is_androids() {
        assert_eq!(FIRST_Y, 75.0);
        assert_eq!(SETTINGS_Y, 465.0);
        assert_eq!(CLOSED_WIDTH, X + ENTRY_W + 8.0);
        assert_eq!(OPEN_WIDTH, X + ENTRY_W_OPEN + 8.0);
    }
}
