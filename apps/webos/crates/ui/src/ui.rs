//! The [`Ui`]: what every screen draws and interacts through, built once a frame over the display
//! list being recorded, and the [`UiState`] it keeps between frames.
//!
//! A frame goes: [`Ui::new`] applies the frame's input against what the previous frame drew
//! (moving focus, pressing, clicking); the screen then draws itself, registering focusable
//! rectangles and groups and reading their [`Response`]s; [`Ui::finish`] settles focus against
//! what this frame drew and says when the next frame is due.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use pley_base::{
    Cmd, Color, DisplayList, GradientStop, Icon, ImageProvider, ImageRequest, ImageState, Key, KeyEvent, Point,
    PointerEvent, Priority, Rect, RgbaImage, Size, TextLayout, TextOpts, TextShaper, TextStyle,
};

use crate::focus::{self, Dir, Focus, FocusOpts, Group, Move, NodeKind, PageMemory, Tree};
use crate::id::Id;
use crate::input::{
    FrameInput, InputEvent, KeyGate, LONG_PRESS, OK_HELD_MAX, POINTER_SHOWN, TWIN, WHEEL_RUN, WHEEL_STEP,
};
use crate::motion::{Lanes, Motion, Tweens};
use crate::theme::{self, Ramp, Tier, TierExt};

/// What an element learns from the frame: whether it has focus and what was done to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Response {
    pub id: Id,
    pub focused: bool,
    /// Focus arrived this frame.
    pub gained: bool,
    /// OK was released on it (not after a long press), or it was clicked.
    pub activated: bool,
    /// The activation was the pointer's click (or OK standing for one); [`Response::pointer`]
    /// says where.
    pub clicked: bool,
    /// OK has been held on it for 500 ms (it asked with [`FocusOpts::long_press`]).
    pub long_pressed: bool,
    /// The menu key was released while it had focus.
    pub menu: bool,
    /// OK or the pointer's button is held down on it.
    pub pressed: bool,
    /// The pointer shows over it, on top of everything else there.
    pub hovered: bool,
    /// Where the pointer is while it hovers, in the element's own coordinates.
    pub pointer: Option<Point>,
}

impl Response {
    fn none(id: Id) -> Response {
        Response {
            id,
            focused: false,
            gained: false,
            activated: false,
            clicked: false,
            long_pressed: false,
            menu: false,
            pressed: false,
            hovered: false,
            pointer: None,
        }
    }
}

/// What a frame tells the main loop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameOutput {
    /// When the next frame is due if no input comes: at once (a time at or before this frame's)
    /// while something animates, at a timer (a long press, rest), or `None`: only input brings
    /// one.
    pub wake_at: Option<f64>,
    /// Focus changed after the screen drew (what had it wasn't drawn, a page's element appeared):
    /// draw once more before presenting, so no frame shows without focus.
    pub redraw: bool,
    /// Whether the pointer should show: keys hide it until it moves again.
    pub pointer_visible: bool,
    /// A text field wants typed text: the platform shows the TV's keyboard.
    pub text_input: bool,
    pub focused: Option<Id>,
}

/// How a picture fills its rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Fit {
    /// Fills it, cropped toward the bias point.
    #[default]
    Cover,
    /// Fits inside it whole, placed by the bias point.
    Contain,
    /// Stretched to it.
    Stretch,
}

/// How a picture is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageOpts {
    pub fit: Fit,
    /// Where the crop (or the placement) leans, as Android's `BiasAlignment`: −1 the left or top
    /// edge, 0 the centre, 1 the right or bottom ([`theme::BACKDROP_BIAS`] for backdrops).
    pub bias: (f32, f32),
    pub radius: f32,
    pub priority: Priority,
}

impl Default for ImageOpts {
    fn default() -> ImageOpts {
        ImageOpts { fit: Fit::Cover, bias: (0.0, 0.0), radius: 0.0, priority: Priority::Visible }
    }
}

impl ImageOpts {
    /// Filling the rectangle, cropped about the centre.
    pub fn cover() -> ImageOpts {
        ImageOpts::default()
    }

    /// Whole inside the rectangle, centred.
    pub fn contain() -> ImageOpts {
        ImageOpts { fit: Fit::Contain, ..ImageOpts::default() }
    }

    /// Leaning toward (`h`, `v`), each from −1 to 1.
    pub fn biased(self, h: f32, v: f32) -> ImageOpts {
        ImageOpts { bias: (h, v), ..self }
    }

    /// With its corners rounded by `radius`.
    pub fn rounded(self, radius: f32) -> ImageOpts {
        ImageOpts { radius, ..self }
    }

    /// Asked for at `priority`.
    pub fn with_priority(self, priority: Priority) -> ImageOpts {
        ImageOpts { priority, ..self }
    }
}

/// A picture as this frame can show it: its state in the cache and its fade-in so far (0 until
/// it is ready; 1 once shown fully, at once when it was already in memory).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub state: ImageState,
    pub opacity: f32,
}

impl Shown {
    /// Whether it hides what is under it: the placeholder needn't be drawn.
    pub fn opaque(&self) -> bool {
        self.opacity >= 1.0
    }
}

/// Where a `width`×`height` picture goes in `rect`, and which part of it shows: the destination
/// and the source rectangle (in the picture's pixels). `None` for an empty picture.
pub fn fit_rects(rect: Rect, width: u32, height: u32, fit: Fit, bias: (f32, f32)) -> Option<(Rect, Rect)> {
    if width == 0 || height == 0 || rect.is_empty() {
        return None;
    }
    let (w, h) = (width as f32, height as f32);
    let full = Rect::new(0.0, 0.0, w, h);
    let lean = |room: f32, b: f32| room * (1.0 + b.clamp(-1.0, 1.0)) / 2.0;
    Some(match fit {
        Fit::Stretch => (rect, full),
        Fit::Cover => {
            let s = (rect.w / w).max(rect.h / h);
            let (sw, sh) = (rect.w / s, rect.h / s);
            (rect, Rect::new(lean(w - sw, bias.0), lean(h - sh, bias.1), sw, sh))
        }
        Fit::Contain => {
            let s = (rect.w / w).min(rect.h / h);
            let (dw, dh) = (w * s, h * s);
            (Rect::new(rect.x + lean(rect.w - dw, bias.0), rect.y + lean(rect.h - dh, bias.1), dw, dh), full)
        }
    })
}

/// OK's press in progress, and what tells its key from its click.
#[derive(Debug)]
struct OkPress {
    held: bool,
    press_at: f64,
    release_at: f64,
    swallowed: bool,
    target: Option<Id>,
    down_at: Option<f64>,
    long: bool,
    long_due: Option<f64>,
    /// OK went down while the pointer showed over a pointer area, or over the focused element
    /// that wants the click's place: its release clicks there.
    click: Option<Id>,
}

impl Default for OkPress {
    fn default() -> OkPress {
        OkPress {
            held: false,
            press_at: f64::NEG_INFINITY,
            release_at: f64::NEG_INFINITY,
            swallowed: false,
            target: None,
            down_at: None,
            long: false,
            long_due: None,
            click: None,
        }
    }
}

/// What this frame's input did, for the elements to read as they are drawn.
#[derive(Debug, Default)]
struct FrameEvents {
    keys: Vec<KeyEvent>,
    typed: String,
    /// Elements activated, and whether by a click.
    activated: Vec<(Id, bool)>,
    long_pressed: Option<Id>,
    menu: Option<Id>,
    failed_move: Option<Dir>,
    /// The failed move came from a held key (or a run of the wheel).
    failed_held: bool,
    edges: Vec<(Id, Dir)>,
    wheel: Vec<(Id, i32)>,
    pointer_outside: Option<Id>,
    hover: Option<Id>,
}

impl FrameEvents {
    fn clear(&mut self) {
        self.keys.clear();
        self.typed.clear();
        self.activated.clear();
        self.long_pressed = None;
        self.menu = None;
        self.failed_move = None;
        self.failed_held = false;
        self.edges.clear();
        self.wheel.clear();
        self.pointer_outside = None;
        self.hover = None;
    }
}

/// Everything the interface keeps from one frame to the next: focus and its memories, keys and
/// the pointer in progress, animated values, and the display settings that change motion. One
/// for the whole app, owned by the main loop.
pub struct UiState {
    focus: Focus,
    prev: Tree,
    cur: Tree,
    gate: KeyGate,
    tweens: Tweens,
    frame: u64,
    time: f64,
    gained_frame: u64,
    reduced_motion: bool,
    tier: Tier,
    ok: OkPress,
    menu_down: bool,
    pressed: Option<Id>,
    pointer: Option<Point>,
    pointer_visible: bool,
    /// When the pointer last moved, for what OK does; keys clear it.
    pointer_at: Option<f64>,
    pointer_press: Option<Id>,
    wheel_at: f64,
    click_at: f64,
    typing: bool,
    ev: FrameEvents,
    speech: Speech,
}

/// What Audio Guidance reads: the names controls gave this frame, what focus last read, and what
/// waits to be read.
#[derive(Debug, Default)]
struct Speech {
    on: bool,
    /// The names of this frame's controls, by id (collected only while on).
    names: HashMap<Id, String>,
    /// The focus whose name was read last, and what it read.
    read: Option<(Id, String)>,
    /// A page opened: its name, read before what has focus.
    title: Option<String>,
    /// What opened this frame (a dialog, a notice), read before what has focus in it.
    opened: Vec<String>,
    /// Announced this frame or before, not read yet.
    announced: Vec<String>,
    /// Ready to read.
    out: Vec<String>,
    /// What [`Ui::announce_once`] was given last frame and this one, by id.
    once_prev: HashSet<Id>,
    once_cur: HashSet<Id>,
}

impl Speech {
    /// After focus settles: the page's name, the newly focused control's name, then what was
    /// announced. Focus settles on what this frame drew, so its name, if it has one, is here.
    fn settle(&mut self, focused: Option<Id>) {
        self.once_prev = std::mem::take(&mut self.once_cur);
        if !self.on {
            self.names.clear();
            self.opened.clear();
            self.announced.clear();
            self.title = None;
            return;
        }
        self.out.extend(self.title.take());
        self.out.append(&mut self.opened);
        if let Some(id) = focused
            && let Some(name) = self.names.get(&id)
            && self.read.as_ref().is_none_or(|(read, _)| *read != id)
        {
            self.out.push(name.clone());
            self.read = Some((id, name.clone()));
        }
        self.out.append(&mut self.announced);
        self.names.clear();
    }
}

impl Default for UiState {
    fn default() -> UiState {
        UiState::new()
    }
}

impl UiState {
    /// Nothing focused, nothing held, nothing animating.
    pub fn new() -> UiState {
        let mut prev = Tree::default();
        prev.reset();
        prev.close();
        let mut cur = Tree::default();
        cur.reset();
        UiState {
            focus: Focus::default(),
            prev,
            cur,
            gate: KeyGate::new(),
            tweens: Tweens::default(),
            frame: 0,
            time: 0.0,
            gained_frame: 0,
            reduced_motion: false,
            tier: Tier::default(),
            ok: OkPress::default(),
            menu_down: false,
            pressed: None,
            pointer: None,
            pointer_visible: false,
            pointer_at: None,
            pointer_press: None,
            wheel_at: f64::NEG_INFINITY,
            click_at: f64::NEG_INFINITY,
            typing: false,
            ev: FrameEvents::default(),
            speech: Speech::default(),
        }
    }

    /// Reduce motion (Settings → Display, or the TV's own setting): scales, slides and fades
    /// snap; focus rings, fills and reveal fades stay.
    pub fn set_reduced_motion(&mut self, on: bool) {
        self.reduced_motion = on;
    }

    /// Whether Reduce motion is on.
    pub fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    /// The TV's tier, which decides what animates ([`Tier::rules`]).
    pub fn set_tier(&mut self, tier: Tier) {
        self.tier = tier;
    }

    /// The TV's tier.
    pub fn tier(&self) -> Tier {
        self.tier
    }

    /// The focused element.
    pub fn focused(&self) -> Option<Id> {
        self.focus.focused
    }

    /// Whether Audio Guidance is on: controls name themselves ([`Ui::name`]) and frames say what
    /// to read ([`UiState::take_speech`]) only then.
    pub fn set_speaking(&mut self, on: bool) {
        if on && !self.speech.on {
            // Turned on: what has focus is read at once.
            self.speech.read = None;
        }
        self.speech.on = on;
    }

    /// What Audio Guidance should read after the last frame: the name of what just gained focus,
    /// then what was announced ([`Ui::name`], [`Ui::announce`]); `None` when there is nothing new.
    pub fn take_speech(&mut self) -> Option<String> {
        let parts = std::mem::take(&mut self.speech.out);
        let mut words = String::new();
        for part in parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
            if !words.is_empty() {
                // A pause between parts: a full stop, unless the part before ended a sentence.
                words.push_str(if words.ends_with(['.', '!', '?']) { " " } else { ". " });
            }
            words.push_str(part);
        }
        (!words.is_empty()).then_some(words)
    }

    /// Whether the pointer gave the current focus.
    pub fn pointer_mode(&self) -> bool {
        self.focus.by_pointer
    }

    /// Focus goes to `id` (an element, or a group, entered by its rules) at the end of the next
    /// frame that draws it, waiting at most one frame more; outside the trap that keeps focus,
    /// the request is dropped.
    pub fn request_focus(&mut self, id: Id) {
        self.focus.request = Some((id, 1));
    }

    /// The page's focus, to keep on the back stack with it.
    pub fn export_page(&self) -> PageMemory {
        self.focus.export_page()
    }

    /// Starts the page's focus over from `mem` (an empty one for a new page): the element it had
    /// takes focus when drawn, unless a key is pressed first; until then focus goes to what the
    /// page declares initial, else its first element. Memories of other pages' groups go.
    pub fn import_page(&mut self, mem: PageMemory) {
        self.focus.import_page(mem);
    }

    fn begin(&mut self, input: &FrameInput) {
        self.frame += 1;
        self.time = input.time.max(self.time);
        self.ev.clear();
        self.cur.reset();
        self.fire_long_press();
        for e in &input.events {
            match e {
                InputEvent::Key(k) => self.key(*k),
                InputEvent::Pointer(p) => self.pointer_event(*p),
                InputEvent::Text(s) => self.ev.typed.push_str(s),
            }
        }
        // A held direction moves on the app's clock, as a repeat of its key.
        if let Some(key) = self.gate.tick(self.time)
            && let Some(dir) = Dir::from_key(key)
        {
            self.step(dir, KeyEvent { key, pressed: true, repeat: true, code: 0 });
        }
        self.ev.hover = match (self.pointer_visible, self.pointer) {
            (true, Some(p)) => self.prev.hit_test(p).map(|i| self.prev.node(i).id),
            _ => None,
        };
    }

    /// One key event, in the TV app's order: OK's twin rules, then the repeat gate for
    /// directions, then what the focused element takes, then focus moves, OK and Menu.
    fn key(&mut self, e: KeyEvent) {
        if e.pressed {
            // A key since the page showed: focus stays where people put it.
            self.focus.wanted = None;
            if e.key != Key::Ok {
                // webOS hides the pointer when any other key is pressed.
                self.pointer_at = None;
                self.pointer_visible = false;
            }
        }
        if e.key == Key::Ok {
            if !self.ok_twin(e.pressed, e.repeat) {
                self.ok_key(e);
            }
            return;
        }
        if let Some(dir) = Dir::from_key(e.key) {
            self.direction(dir, e);
            return;
        }
        if e.key == Key::Menu {
            // The menu acts once, on key-up, however long it is held.
            if e.pressed {
                self.menu_down |= !e.repeat;
            } else if std::mem::take(&mut self.menu_down) {
                self.ev.menu = self.focus.focused;
            }
            return;
        }
        self.ev.keys.push(e);
    }

    fn direction(&mut self, dir: Dir, e: KeyEvent) {
        if self.gate.admit(e.key, e.pressed, self.time) {
            self.step(dir, e);
        }
    }

    /// A direction key's event let through: to the focused element when it takes the direction
    /// (and every key-up), else a move of focus.
    fn step(&mut self, dir: Dir, e: KeyEvent) {
        let focused = self.focus.focused.and_then(|f| self.prev.get(f));
        if !e.pressed || focused.is_some_and(|i| self.prev.takes(i, dir)) {
            self.ev.keys.push(e);
            return;
        }
        match focus::move_focus(&self.prev, &self.focus.memory, self.focus.focused, dir, e.repeat) {
            Move::To(i) => {
                if self.focus.set(&self.prev, i, false) {
                    self.gained_frame = self.frame;
                }
            }
            Move::Stay { edge } => self.ev.edges.push((self.prev.node(edge).id, dir)),
            Move::Fail { edge } => {
                if let Some(g) = edge {
                    self.ev.edges.push((self.prev.node(g).id, dir));
                }
                self.ev.failed_move = Some(dir);
                self.ev.failed_held = e.repeat;
            }
        }
    }

    /// Whether an OK key event belongs to the pointer and does nothing more as a key (the TV
    /// app's `okTwin`): with the Magic Remote's pointer showing, webOS may send one press both as
    /// the key and as a click, in either order. The key half of a press whose click came first is
    /// dropped; OK while the pointer shows over a pointer area (a scrim) or over the focused
    /// element that wants the click's place is a click there on release.
    fn ok_twin(&mut self, pressed: bool, repeat: bool) -> bool {
        let now = self.time;
        if !pressed {
            let swallowed = std::mem::take(&mut self.ok.swallowed);
            if self.ok.held {
                self.ok.held = false;
                self.ok.release_at = now;
            }
            if let Some(id) = self.ok.click.take() {
                if self.pressed == Some(id) {
                    self.pressed = None;
                }
                self.ev.activated.push((id, true));
            }
            return swallowed;
        }
        if repeat {
            return self.ok.swallowed;
        }
        if now - self.click_at < TWIN {
            self.click_at = f64::NEG_INFINITY;
            self.ok.swallowed = true;
            return true;
        }
        self.ok.held = true;
        self.ok.press_at = now;
        self.ok.swallowed = false;
        if !self.pointer_at.is_some_and(|t| now - t < POINTER_SHOWN) {
            return false;
        }
        let Some(i) = self.pointer.and_then(|p| self.prev.hit_test(p)) else { return false };
        let n = self.prev.node(i);
        let clicks = match n.kind {
            NodeKind::Area => true,
            NodeKind::Element(o) => o.click_at && self.focus.focused == Some(n.id),
            NodeKind::Group(_) => false,
        };
        if clicks {
            self.ok.swallowed = true;
            self.ok.click = Some(n.id);
            self.pressed = Some(n.id);
        }
        self.ok.swallowed
    }

    /// OK as a key: down presses the focused element (and starts its long press), up selects it
    /// unless the long press fired or focus moved meanwhile.
    fn ok_key(&mut self, e: KeyEvent) {
        let Some(focused) = self.focus.focused else {
            self.ev.keys.push(e);
            return;
        };
        if e.pressed {
            if e.repeat {
                return;
            }
            self.ok.down_at = Some(self.time);
            self.ok.long = false;
            self.ok.target = Some(focused);
            self.pressed = Some(focused);
            let long = self
                .prev
                .get(focused)
                .is_some_and(|i| matches!(self.prev.node(i).kind, NodeKind::Element(o) if o.long_press));
            self.ok.long_due = long.then_some(self.time + LONG_PRESS);
            return;
        }
        self.ok.long_due = None;
        let target = self.ok.target.take();
        if target.is_some() && self.pressed == target {
            self.pressed = None;
        }
        if self.ok.long || target != Some(focused) || self.ok.down_at.take().is_none() {
            return;
        }
        self.ev.activated.push((focused, false));
    }

    fn fire_long_press(&mut self) {
        let Some(due) = self.ok.long_due else { return };
        if self.time < due - 1e-6 {
            return;
        }
        self.ok.long_due = None;
        self.ok.long = true;
        if let Some(t) = self.ok.target {
            self.ev.long_pressed = Some(t);
            if self.pressed == Some(t) {
                self.pressed = None;
            }
        }
    }

    fn pointer_event(&mut self, e: PointerEvent) {
        let now = self.time;
        match e {
            PointerEvent::Moved(p) => {
                // What slides under a still pointer never takes focus from the keys.
                let moved = self.pointer != Some(p);
                self.pointer = Some(p);
                self.pointer_visible = true;
                self.pointer_at = Some(now);
                let Some(i) = self.prev.hit_test(p).filter(|_| moved) else { return };
                let n = *self.prev.node(i);
                if !matches!(n.kind, NodeKind::Element(_)) || self.focus.focused == Some(n.id) || self.typing {
                    return;
                }
                if self.prev.within(i, self.prev.scope()) {
                    self.focus.wanted = None;
                    if self.focus.set(&self.prev, i, true) {
                        self.gained_frame = self.frame;
                        // Focus following the pointer moves like a key's: what waits for focus
                        // to rest (backdrops, info blocks) waits for the pointer to settle.
                        self.gate.moved(now);
                    }
                } else {
                    self.ev.pointer_outside = Some(n.id);
                }
            }
            PointerEvent::Button { pressed: true } => {
                self.pointer_press = self.pointer_target();
                if self.pointer_press.is_some() {
                    self.pressed = self.pointer_press;
                }
            }
            PointerEvent::Button { pressed: false } => {
                let press = self.pointer_press.take();
                if press.is_some() && self.pressed == press {
                    self.pressed = None;
                }
                if let Some(id) = press.filter(|&id| self.pointer_target() == Some(id)) {
                    self.click(id);
                }
            }
            PointerEvent::Wheel(d) => self.wheel(d),
            PointerEvent::Hidden => {
                self.pointer_visible = false;
                self.pointer_at = None;
            }
        }
    }

    /// What the pointer would click: a pointer area, or an element where focus may go.
    fn pointer_target(&self) -> Option<Id> {
        let i = self.prev.hit_test(self.pointer?)?;
        let n = self.prev.node(i);
        match n.kind {
            NodeKind::Area => Some(n.id),
            NodeKind::Element(_) if self.prev.within(i, self.prev.scope()) => Some(n.id),
            _ => None,
        }
    }

    /// A click focuses and selects, unless it is the click half of an OK press the key made or
    /// is making (the key's select may already have opened a page whose button is now under the
    /// pointer).
    fn click(&mut self, id: Id) {
        let now = self.time;
        if (self.ok.held && now - self.ok.press_at < OK_HELD_MAX) || now - self.ok.release_at < TWIN {
            self.ok.release_at = f64::NEG_INFINITY;
            return;
        }
        self.click_at = now;
        if let Some(i) = self.prev.get(id).filter(|&i| self.prev.is_element(i)) {
            self.focus.wanted = None;
            if self.focus.set(&self.prev, i, true) {
                self.gained_frame = self.frame;
                self.gate.moved(now);
            }
        }
        self.ev.activated.push((id, true));
    }

    /// The Magic Remote's wheel: over a group that scrolls by it, a notch for that group;
    /// elsewhere a step of focus up or down as the keys take, at most one per 100 ms, notches in a
    /// quick run counting as a held key.
    fn wheel(&mut self, d: f32) {
        if d == 0.0 || self.typing {
            return;
        }
        // Turned away from the viewer it goes up, toward the start, as scrolling up does.
        let notch = if d > 0.0 { -1 } else { 1 };
        let over = self.pointer.filter(|_| self.pointer_visible).and_then(|p| self.prev.wheel_group_at(p));
        if let Some(g) = over {
            let id = self.prev.node(g).id;
            match self.ev.wheel.iter_mut().find(|w| w.0 == id) {
                Some(w) => w.1 += notch,
                None => self.ev.wheel.push((id, notch)),
            }
            return;
        }
        let now = self.time;
        if now - self.wheel_at < WHEEL_STEP {
            return;
        }
        let repeat = now - self.wheel_at < WHEEL_RUN;
        self.wheel_at = now;
        self.focus.wanted = None;
        self.pointer_at = None;
        // A notch has no key-up: it steps once, past the gate, which only learns of the run.
        self.gate.wheeled(repeat, now);
        let dir = if notch < 0 { Dir::Up } else { Dir::Down };
        self.step(dir, KeyEvent { key: dir.key(), pressed: true, repeat, code: 0 });
    }
}

/// Local coordinates to the screen's: scaled by `s`, then moved by `t`.
#[derive(Clone, Copy, Debug)]
struct Xf {
    s: f32,
    tx: f32,
    ty: f32,
}

impl Xf {
    const IDENTITY: Xf = Xf { s: 1.0, tx: 0.0, ty: 0.0 };

    fn rect(&self, r: Rect) -> Rect {
        Rect::new(r.x * self.s + self.tx, r.y * self.s + self.ty, r.w * self.s, r.h * self.s)
    }

    fn to_local(self, p: Point) -> Point {
        if self.s == 0.0 {
            return p;
        }
        Point::new((p.x - self.tx) / self.s, (p.y - self.ty) / self.s)
    }

    /// This transform followed by [`Cmd::PushTransform`]'s: scaled by `scale` about `pivot`,
    /// then moved by `offset`.
    fn then(&self, offset: Point, scale: f32, pivot: Point) -> Xf {
        let ix = pivot.x * (1.0 - scale) + offset.x;
        let iy = pivot.y * (1.0 - scale) + offset.y;
        Xf { s: self.s * scale, tx: self.s * ix + self.tx, ty: self.s * iy + self.ty }
    }
}

/// A frame being drawn: the context every screen and control draws and interacts through.
///
/// Drawing calls record [`Cmd`]s in dp, in the coordinates of the enclosing transforms;
/// focusable elements and groups are registered as they are drawn, in drawing order, which is
/// also the order focus moves through a group and the order the pointer finds things (the last
/// drawn is on top).
pub struct Ui<'a> {
    st: &'a mut UiState,
    list: &'a mut DisplayList,
    shaper: &'a dyn TextShaper,
    images: &'a mut dyn ImageProvider,
    scale: f32,
    ids: Vec<Id>,
    groups: Vec<u32>,
    xf: Vec<Xf>,
    clips: Vec<Rect>,
    wake_now: bool,
    wake_at: Option<f64>,
    want_gate: bool,
    text_input: bool,
}

impl<'a> Ui<'a> {
    /// Starts a frame: `list` is cleared to record it (with `Bg` as its clear colour), `scale` is
    /// device pixels per dp, and `input` is applied against what the previous frame drew.
    pub fn new(
        state: &'a mut UiState,
        list: &'a mut DisplayList,
        shaper: &'a dyn TextShaper,
        images: &'a mut dyn ImageProvider,
        scale: f32,
        input: &FrameInput,
    ) -> Ui<'a> {
        list.cmds.clear();
        list.clear = theme::BG;
        state.begin(input);
        Ui {
            st: state,
            list,
            shaper,
            images,
            scale,
            ids: vec![Id::ROOT],
            groups: vec![0],
            xf: vec![Xf::IDENTITY],
            clips: Vec::new(),
            wake_now: false,
            wake_at: None,
            want_gate: false,
            text_input: false,
        }
    }

    /// Ends the frame: settles focus against what was drawn, drops animated values nobody asked
    /// for, and says when the next frame is due.
    pub fn finish(self) -> FrameOutput {
        let Ui { st, wake_now, wake_at, want_gate, text_input, .. } = self;
        st.cur.close();
        let changed = st.focus.settle(&st.prev, &st.cur);
        if changed {
            st.gained_frame = st.frame + 1;
        }
        st.speech.settle(st.focus.focused);
        std::mem::swap(&mut st.prev, &mut st.cur);
        st.tweens.sweep(st.frame);
        st.typing = text_input;
        let now = st.time;
        let mut wake = if wake_now { Some(now) } else { wake_at };
        let mut sooner = |t: f64| wake = Some(wake.map_or(t, |w| w.min(t)));
        if let Some(due) = st.ok.long_due {
            sooner(due);
        }
        // While a direction key is down, frames keep coming for its held moves.
        if (want_gate || st.gate.down())
            && let Some(t) = st.gate.next_change(now)
        {
            sooner(t);
        }
        if st.focus.request.is_some() {
            sooner(now);
        }
        FrameOutput {
            wake_at: wake,
            redraw: changed,
            pointer_visible: st.pointer_visible,
            text_input,
            focused: st.focus.focused,
        }
    }

    /// Whether Audio Guidance is on ([`UiState::set_speaking`]).
    pub fn speaking(&self) -> bool {
        self.st.speech.on
    }

    /// Names `id` for Audio Guidance: what it reads when `id` gains focus ("Play", "Arrival, 2016",
    /// "Season 2, tab"). `text` is called only while Audio Guidance is on; the first name given an
    /// id in a frame is kept, so a page can name a control before the kit draws it.
    pub fn name(&mut self, id: Id, text: impl FnOnce() -> String) {
        if self.st.speech.on {
            self.st.speech.names.entry(id).or_insert_with(text);
        }
    }

    /// Reads `text` (a toast, a status line that changed) after the name of what has focus.
    pub fn announce(&mut self, text: impl Into<String>) {
        if self.st.speech.on {
            self.st.speech.announced.push(text.into());
        }
    }

    /// Reads `text` once as `id` appears (a dialog opening, a toast), before what has focus, and
    /// not again while it is drawn frame after frame.
    pub fn announce_once(&mut self, id: Id, text: impl FnOnce() -> String) {
        if !self.st.speech.on {
            return;
        }
        if !self.st.speech.once_prev.contains(&id) && !self.st.speech.once_cur.contains(&id) {
            let text = text();
            self.st.speech.opened.push(text);
        }
        self.st.speech.once_cur.insert(id);
    }

    /// A page opened: Audio Guidance reads its name before what has focus.
    pub fn announce_page(&mut self, title: impl Into<String>) {
        if self.st.speech.on {
            self.st.speech.title = Some(title.into());
            // Focus on the new page is read even where an id repeats from the page before.
            self.st.speech.read = None;
        }
    }

    // Frame facts.

    /// The frame's time, in seconds.
    pub fn time(&self) -> f64 {
        self.st.time
    }

    /// Device pixels per dp (2 on the TV's 1920×1080 interface plane).
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The width in device pixels to ask a picture drawn `width` dp wide for.
    pub fn image_px(&self, width: f32) -> u32 {
        (width * self.scale).ceil().max(1.0) as u32
    }

    /// Whether Reduce motion is on.
    pub fn reduced_motion(&self) -> bool {
        self.st.reduced_motion
    }

    /// The TV's tier.
    pub fn tier(&self) -> Tier {
        self.st.tier
    }

    /// The colour the frame is cleared to: Bg for pages, transparent while video plays under the
    /// interface.
    pub fn set_clear(&mut self, color: Color) {
        self.list.clear = color;
    }

    /// Draw another frame as soon as possible.
    pub fn request_repaint(&mut self) {
        self.wake_now = true;
    }

    /// Draw another frame within `seconds` (a clock, a toast's end).
    pub fn repaint_after(&mut self, seconds: f64) {
        let t = self.st.time + seconds.max(0.0);
        self.wake_at = Some(self.wake_at.map_or(t, |w| w.min(t)));
    }

    // Ids.

    /// The id of `key` in the current scope (the innermost group or [`Ui::push_id`]).
    pub fn id(&self, key: impl std::hash::Hash) -> Id {
        self.scope_id().with(key)
    }

    /// The current scope's id.
    pub fn scope_id(&self) -> Id {
        *self.ids.last().unwrap_or(&Id::ROOT)
    }

    /// Runs `f` with ids scoped under `key`.
    pub fn push_id<R>(&mut self, key: impl std::hash::Hash, f: impl FnOnce(&mut Self) -> R) -> R {
        let id = self.id(key);
        self.ids.push(id);
        let r = f(self);
        self.ids.pop();
        r
    }

    // Focus.

    /// A focusable element at `rect` (in current coordinates), with what happened to it.
    pub fn focusable(&mut self, id: Id, rect: Rect) -> Response {
        self.focusable_with(id, rect, FocusOpts::default())
    }

    /// A focusable element that asks more of the keys (a long press, directions of its own, the
    /// click's place). An id drawn twice in a frame counts once: the first.
    pub fn focusable_with(&mut self, id: Id, rect: Rect, opts: FocusOpts) -> Response {
        if self.register(id, NodeKind::Element(opts), rect).is_none() {
            return Response::none(id);
        }
        self.response(id, true)
    }

    /// A rectangle only the pointer uses, never focused: a scrim whose click closes a panel, the
    /// hero's hidden area that brings it back. OK while the pointer shows over it is its click.
    pub fn clickable(&mut self, id: Id, rect: Rect) -> Response {
        if self.register(id, NodeKind::Area, rect).is_none() {
            return Response::none(id);
        }
        self.response(id, false)
    }

    /// A focus group around what `f` draws; its id is the scope of the ids made inside it.
    pub fn group<R>(&mut self, id: Id, group: impl Into<Group>, f: impl FnOnce(&mut Self) -> R) -> R {
        let g = group.into();
        let at = self.register(id, NodeKind::Group(g), Rect::ZERO);
        if let Some(at) = at {
            if g.trap {
                self.st.cur.traps.push(at);
            }
            self.groups.push(at);
        }
        self.ids.push(id);
        let r = f(self);
        self.ids.pop();
        if at.is_some() {
            self.groups.pop();
        }
        r
    }

    /// Where focus lands when the page (or the trap being drawn) has none: its initial element.
    /// The first call in a frame wins; without one, the first element drawn.
    pub fn initial_focus(&mut self, id: Id) {
        let tree = &mut self.st.cur;
        let holder = self
            .groups
            .iter()
            .rev()
            .copied()
            .find(|&g| matches!(tree.node(g).kind, NodeKind::Group(g) if g.trap))
            .unwrap_or(0);
        if let Some(g) = tree.group_mut(holder) {
            g.enter = g.enter.or(Some(id));
        }
    }

    /// Focus goes to `id` at the end of this frame, if it is drawn (see
    /// [`UiState::request_focus`]).
    pub fn request_focus(&mut self, id: Id) {
        self.st.request_focus(id);
        self.wake_now = true;
    }

    /// The focused element.
    pub fn focused(&self) -> Option<Id> {
        self.st.focus.focused
    }

    /// Whether `id` has focus.
    pub fn is_focused(&self, id: Id) -> bool {
        self.st.focus.focused == Some(id)
    }

    /// The focused element, if it is inside `group`: rows and grids scroll to keep it in view.
    pub fn focused_in(&self, group: Id) -> Option<Id> {
        self.path_in(group).and(self.st.focus.focused)
    }

    /// The index among `group`'s items (its elements and the groups directly in it with an
    /// element, in drawing order, as drawn last frame) of the one holding focus.
    pub fn focused_index(&self, group: Id) -> Option<usize> {
        let (g, child) = self.path_in(group)?;
        self.st.prev.item_index(g, child)
    }

    /// The focused element's rectangle, as it was registered last frame (in the coordinates
    /// current then), if it is inside `group`.
    pub fn focused_rect(&self, group: Id) -> Option<Rect> {
        self.path_in(group)?;
        let t = &self.st.prev;
        Some(t.node(t.get(self.st.focus.focused?)?).local)
    }

    /// The element focused last inside `group`.
    pub fn remembered(&self, group: Id) -> Option<Id> {
        self.st.focus.memory.get(&group).copied()
    }

    /// `group` and its item on the way down to the focused element.
    fn path_in(&self, group: Id) -> Option<(u32, u32)> {
        let t = &self.st.prev;
        let g = t.get(group)?;
        let mut i = t.get(self.st.focus.focused?)?;
        loop {
            let p = t.node(i).parent;
            if p == g {
                return Some((g, i));
            }
            if p == focus::NONE {
                return None;
            }
            i = p;
        }
    }

    /// Whether the pointer gave the current focus: scrollers then move only as far as brings the
    /// element in whole ([`crate::nudge`]), never re-centring.
    pub fn pointer_mode(&self) -> bool {
        self.st.focus.by_pointer
    }

    /// A direction this frame that nothing took (Left past a page's first column): the shell
    /// sends it to the rail.
    pub fn failed_move(&self) -> Option<Dir> {
        self.st.ev.failed_move
    }

    /// Whether this frame's failed move came from a held key rather than a press of its own: a
    /// held Left stops at a row's start instead of opening the rail.
    pub fn failed_move_held(&self) -> bool {
        self.st.ev.failed_held
    }

    /// A move this frame that met `group`'s [`crate::Edge::Stop`] or [`crate::Edge::Out`].
    pub fn edge_hit(&self, group: Id) -> Option<Dir> {
        self.st.ev.edges.iter().find(|e| e.0 == group).map(|e| e.1)
    }

    /// The element the pointer moved onto this frame outside the trap that keeps focus (the
    /// page under an open panel or rail).
    pub fn pointer_outside(&self) -> Option<Id> {
        self.st.ev.pointer_outside
    }

    /// The page's focus, to keep on the back stack ([`UiState::export_page`]).
    pub fn export_page(&self) -> PageMemory {
        self.st.export_page()
    }

    /// Starts the page's focus over from `mem` ([`UiState::import_page`]).
    pub fn import_page(&mut self, mem: PageMemory) {
        self.st.import_page(mem);
        self.wake_now = true;
    }

    fn register(&mut self, id: Id, kind: NodeKind, local: Rect) -> Option<u32> {
        let rect = self.xf().rect(local);
        let hit = match self.clips.last() {
            Some(c) => rect.intersect(c).unwrap_or(Rect::ZERO),
            None => rect,
        };
        let parent = *self.groups.last().unwrap_or(&0);
        self.st.cur.add(id, parent, kind, local, rect, hit)
    }

    fn response(&self, id: Id, focusable: bool) -> Response {
        let st = &*self.st;
        let focused = focusable && st.focus.focused == Some(id);
        let activation = st.ev.activated.iter().find(|a| a.0 == id);
        let hovered = st.ev.hover == Some(id);
        Response {
            id,
            focused,
            gained: focused && st.gained_frame == st.frame,
            activated: activation.is_some(),
            clicked: activation.is_some_and(|a| a.1),
            long_pressed: st.ev.long_pressed == Some(id),
            menu: st.ev.menu == Some(id),
            pressed: st.pressed == Some(id),
            hovered,
            pointer: st.pointer.filter(|_| hovered).map(|p| self.xf().to_local(p)),
        }
    }

    // Keys and the pointer.

    /// This frame's key events that focus didn't use: Back, digits, the player's and colour keys,
    /// key-ups, and directions the focused element or its groups take. A held direction comes as
    /// the repeat gate's repeats, on the app's clock, not the TV's.
    pub fn keys(&self) -> &[KeyEvent] {
        &self.st.ev.keys
    }

    /// Whether `key` was pressed this frame (a first press, not a repeat), still unclaimed.
    pub fn key_down(&self, key: Key) -> bool {
        self.st.ev.keys.iter().any(|e| e.key == key && e.pressed && !e.repeat)
    }

    /// Claims a press of `key` this frame, so whatever reads the keys after doesn't act on it too
    /// (a panel closing on Back before the shell sees it). Whether there was one.
    pub fn take_key(&mut self, key: Key) -> bool {
        let keys = &mut self.st.ev.keys;
        match keys.iter().position(|e| e.key == key && e.pressed && !e.repeat) {
            Some(at) => {
                keys.remove(at);
                true
            }
            None => false,
        }
    }

    /// Text typed this frame.
    pub fn typed(&self) -> &str {
        &self.st.ev.typed
    }

    /// Says a text field is typing this frame: the platform shows the TV's keyboard, and the
    /// pointer and wheel leave focus on the field.
    pub fn text_input(&mut self, on: bool) {
        self.text_input |= on;
    }

    /// Whether a direction key is held: animations snap, except that scrolls ([`Ui::scroll`]) and
    /// a tile's focus glide evenly from move to move ([`Motion::GLIDE`]).
    pub fn holding(&mut self) -> bool {
        let h = self.st.gate.holding(self.st.time);
        self.want_gate |= h;
        h
    }

    /// Whether focus has rested (no key held, nothing moved for 400 ms): backdrops and info
    /// blocks follow focus only then. A frame is drawn when it becomes true.
    pub fn rested(&mut self) -> bool {
        let r = self.st.gate.rested(self.st.time);
        self.want_gate |= !r;
        r
    }

    /// Where the pointer is on screen while it shows.
    pub fn pointer(&self) -> Option<Point> {
        self.st.pointer.filter(|_| self.st.pointer_visible)
    }

    /// What the pointer shows over: the element or pointer area on top there.
    pub fn hovered(&self) -> Option<Id> {
        self.st.ev.hover
    }

    /// Wheel notches turned over `group` this frame (it takes the wheel, [`Group::wheel`]): one
    /// per notch toward the viewer (down the content), negative away.
    pub fn wheel(&self, group: Id) -> i32 {
        self.st.ev.wheel.iter().filter(|w| w.0 == group).map(|w| w.1).sum()
    }

    // Animation.

    /// The value of `id` heading for `target` with `motion`: a new value starts at its target,
    /// and a changed target starts a tween from where the value is. Snaps while a key is held
    /// (unless the motion is a held key's glide), with Reduce motion (unless the motion is kept)
    /// and on Low-tier TVs for motions that snap there. A running tween asks for the next frame.
    pub fn animate(&mut self, id: Id, target: f32, motion: Motion) -> f32 {
        self.tween(id, None, [target, 0.0, 0.0, 0.0], motion)[0]
    }

    /// A scroll following focus (a row's offset, a page's shift, a grid's scroll): as
    /// [`Ui::animate`] with `motion`, but while a direction key is held each move glides evenly
    /// into the next ([`Motion::GLIDE`]) rather than jumping.
    pub fn scroll(&mut self, id: Id, target: f32, motion: Motion) -> f32 {
        let motion = if self.holding() { Motion::GLIDE } else { motion };
        self.animate(id, target, motion)
    }

    /// As [`Ui::animate`], but a new value starts at `from`: a panel opening, a toast showing.
    pub fn animate_from(&mut self, id: Id, from: f32, target: f32, motion: Motion) -> f32 {
        self.tween(id, Some([from, 0.0, 0.0, 0.0]), [target, 0.0, 0.0, 0.0], motion)[0]
    }

    /// A colour heading for `target`, channel by channel.
    pub fn animate_color(&mut self, id: Id, target: Color, motion: Motion) -> Color {
        let lanes = [target.r as f32, target.g as f32, target.b as f32, target.a as f32];
        let v = self.tween(id, None, lanes, motion);
        let ch = |x: f32| x.round().clamp(0.0, 255.0) as u8;
        Color { r: ch(v[0]), g: ch(v[1]), b: ch(v[2]), a: ch(v[3]) }
    }

    /// A rectangle heading for `target`.
    pub fn animate_rect(&mut self, id: Id, target: Rect, motion: Motion) -> Rect {
        let v = self.tween(id, None, [target.x, target.y, target.w, target.h], motion);
        Rect::new(v[0], v[1], v[2], v[3])
    }

    /// Whether `motion` snaps this frame instead of animating.
    pub fn snaps(&mut self, motion: Motion) -> bool {
        (self.st.gate.holding(self.st.time) && !motion.held)
            || (self.st.reduced_motion && !motion.reduced)
            || (self.st.tier == Tier::Low && motion.low_snaps)
    }

    fn tween(&mut self, id: Id, from: Option<Lanes>, to: Lanes, motion: Motion) -> Lanes {
        let snap = self.snaps(motion);
        let (v, running) = self.st.tweens.animate(id, from, to, motion, self.st.time, self.st.frame, snap);
        self.wake_now |= running;
        v
    }

    // Drawing.

    /// Records a command as it is.
    pub fn cmd(&mut self, cmd: Cmd) {
        self.list.push(cmd);
    }

    /// A filled rectangle, its corners rounded by `radius`.
    pub fn fill(&mut self, rect: Rect, color: Color, radius: f32) {
        if !color.is_transparent() && !rect.is_empty() {
            self.list.push(Cmd::Fill { rect, color, radius });
        }
    }

    /// An outline `width` thick centred on the rectangle's edge.
    pub fn stroke(&mut self, rect: Rect, color: Color, radius: f32, width: f32) {
        if !color.is_transparent() && width > 0.0 {
            self.list.push(Cmd::Stroke { rect, color, radius, width });
        }
    }

    /// A gradient along x (`vertical` false) or y through `stops` (0 to 1 across the rectangle).
    pub fn gradient(&mut self, rect: Rect, vertical: bool, stops: Vec<GradientStop>) {
        if !rect.is_empty() && !stops.is_empty() {
            self.list.push(Cmd::LinearGradient { rect, vertical, stops });
        }
    }

    /// A design ramp ([`theme::Ramp`]) across `rect`, its dp positions read along the rectangle's
    /// own x (or y) in the current coordinates.
    pub fn ramp(&mut self, rect: Rect, vertical: bool, ramp: &Ramp) {
        let stops = if vertical { ramp.stops(rect.y, rect.h) } else { ramp.stops(rect.x, rect.w) };
        self.gradient(rect, vertical, stops);
    }

    /// A radial glow: `inner` at `center`, fading to `outer` at `radius`.
    pub fn radial(&mut self, rect: Rect, center: Point, radius: f32, inner: Color, outer: Color) {
        self.list.push(Cmd::RadialGradient { rect, center, radius, inner, outer });
    }

    /// An icon in `rect` (square), tinted.
    pub fn icon(&mut self, rect: Rect, icon: Icon, color: Color) {
        if !color.is_transparent() {
            self.list.push(Cmd::Icon { rect, icon, color });
        }
    }

    /// Clears `rect` to transparent so the video plane shows through.
    pub fn punch(&mut self, rect: Rect) {
        self.list.push(Cmd::Punch { rect });
    }

    /// A picture the app made (a subtitle, a QR code); `key` names its content for the renderer's
    /// texture.
    pub fn raw(&mut self, rect: Rect, image: Arc<RgbaImage>, key: u64) {
        self.list.push(Cmd::Raw { rect, image, key, radius: 0.0, opacity: 1.0 });
    }

    // Pictures.

    /// Asks for a picture and says how it can show this frame, without drawing it: a picture
    /// that wasn't in memory fades in over 150 ms from when it became ready (at once with Reduce
    /// motion or on Low-tier TVs). While it fades, frames keep coming.
    pub fn image_state(&mut self, req: &ImageRequest, priority: Priority) -> Shown {
        let state = self.images.request(req, priority);
        let opacity = match state {
            ImageState::Ready { since, .. } => {
                if self.st.reduced_motion || !self.st.tier.rules().image_fade {
                    1.0
                } else {
                    let p = Motion::IMAGE_FADE.progress(self.st.time - since);
                    self.wake_now |= p < 1.0;
                    p
                }
            }
            _ => 0.0,
        };
        Shown { state, opacity }
    }

    /// Draws a picture [`Ui::image_state`] gave, if it is ready.
    pub fn draw_image(&mut self, rect: Rect, shown: &Shown, opts: &ImageOpts) {
        let ImageState::Ready { id, width, height, .. } = shown.state else { return };
        if shown.opacity <= 0.0 {
            return;
        }
        if let Some((dst, src)) = fit_rects(rect, width, height, opts.fit, opts.bias) {
            self.list.push(Cmd::Image { rect: dst, image: id, src, radius: opts.radius, opacity: shown.opacity });
        }
    }

    /// Asks for a picture and draws it in `rect` when ready; draw the placeholder first when the
    /// result isn't [`Shown::opaque`] (or ask with [`Ui::image_state`] before drawing anything).
    pub fn image(&mut self, rect: Rect, req: &ImageRequest, opts: &ImageOpts) -> Shown {
        let shown = self.image_state(req, opts.priority);
        self.draw_image(rect, &shown, opts);
        shown
    }

    // Text.

    /// `text` laid out (shaped once and cached by the shaper), for measuring or drawing.
    pub fn layout(&self, text: &str, style: &TextStyle, opts: &TextOpts) -> Arc<TextLayout> {
        self.shaper.layout(text, style, opts)
    }

    /// The size `text` takes, without drawing it.
    pub fn measure(&self, text: &str, style: &TextStyle, opts: &TextOpts) -> Size {
        self.layout(text, style, opts).size
    }

    /// Draws `text` with its layout's top-left corner at `origin`; returns its laid-out size.
    pub fn text(&mut self, origin: Point, text: &str, style: &TextStyle, opts: &TextOpts, color: Color) -> Size {
        let layout = self.layout(text, style, opts);
        let size = layout.size;
        self.draw_text(origin, layout, color);
        size
    }

    /// Draws a layout from [`Ui::layout`].
    pub fn draw_text(&mut self, origin: Point, layout: Arc<TextLayout>, color: Color) {
        if !layout.text.is_empty() && !color.is_transparent() {
            self.list.push(Cmd::Text { origin, layout, color });
        }
    }

    // Scopes.

    fn xf(&self) -> Xf {
        *self.xf.last().unwrap_or(&Xf::IDENTITY)
    }

    /// Draws `f` clipped to `rect` (within any clip around it); the pointer finds nothing clipped
    /// away.
    pub fn clip<R>(&mut self, rect: Rect, f: impl FnOnce(&mut Self) -> R) -> R {
        let screen = self.xf().rect(rect);
        let c = match self.clips.last() {
            Some(outer) => outer.intersect(&screen).unwrap_or(Rect::ZERO),
            None => screen,
        };
        self.clips.push(c);
        self.list.push(Cmd::PushClip { rect });
        let r = f(self);
        self.list.push(Cmd::PopClip);
        self.clips.pop();
        r
    }

    /// Draws `f` scaled by `scale` about `pivot`, then moved by `offset`: a focused tile growing,
    /// a row scrolling. Elements registered inside are found where they show.
    pub fn transform<R>(&mut self, offset: Point, scale: f32, pivot: Point, f: impl FnOnce(&mut Self) -> R) -> R {
        if offset == Point::ZERO && scale == 1.0 {
            return f(self);
        }
        let x = self.xf().then(offset, scale, pivot);
        self.xf.push(x);
        self.list.push(Cmd::PushTransform { offset, scale, pivot });
        let r = f(self);
        self.list.push(Cmd::PopTransform);
        self.xf.pop();
        r
    }

    /// Draws `f` moved by `offset`.
    pub fn translate<R>(&mut self, offset: Point, f: impl FnOnce(&mut Self) -> R) -> R {
        self.transform(offset, 1.0, Point::ZERO, f)
    }

    /// Draws `f` at `alpha` opacity (with any around it).
    pub fn opacity<R>(&mut self, alpha: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        if alpha >= 1.0 {
            return f(self);
        }
        self.list.push(Cmd::PushOpacity(alpha.max(0.0)));
        let r = f(self);
        self.list.push(Cmd::PopOpacity);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_crops_toward_the_bias_point() {
        // A 1280×720 backdrop in a 960×720 box: 320 px of width to crop.
        let (dst, src) = fit_rects(Rect::new(0.0, 0.0, 960.0, 720.0), 1280, 720, Fit::Cover, (0.0, 0.0)).unwrap();
        assert_eq!(dst, Rect::new(0.0, 0.0, 960.0, 720.0));
        assert_eq!(src, Rect::new(160.0, 0.0, 960.0, 720.0));
        let (_, src) =
            fit_rects(Rect::new(0.0, 0.0, 960.0, 720.0), 1280, 720, Fit::Cover, theme::BACKDROP_BIAS).unwrap();
        assert!((src.x - 320.0 * 0.7).abs() < 1e-3);
        // A tall picture in a wide box crops rows, leaning up with −0.6.
        let (_, src) = fit_rects(Rect::new(0.0, 0.0, 100.0, 50.0), 100, 100, Fit::Cover, (0.4, -0.6)).unwrap();
        assert_eq!((src.w, src.h), (100.0, 50.0));
        assert!((src.y - 50.0 * 0.2).abs() < 1e-4);
    }

    #[test]
    fn contain_fits_whole_and_leans_by_the_bias() {
        let (dst, src) = fit_rects(Rect::new(10.0, 10.0, 200.0, 100.0), 100, 100, Fit::Contain, (0.0, 0.0)).unwrap();
        assert_eq!(dst, Rect::new(60.0, 10.0, 100.0, 100.0));
        assert_eq!(src, Rect::new(0.0, 0.0, 100.0, 100.0));
        let (dst, _) = fit_rects(Rect::new(10.0, 10.0, 200.0, 100.0), 100, 100, Fit::Contain, (1.0, 0.0)).unwrap();
        assert_eq!(dst.x, 110.0);
        assert_eq!(fit_rects(Rect::new(0.0, 0.0, 10.0, 10.0), 0, 10, Fit::Cover, (0.0, 0.0)), None);
    }

    #[test]
    fn transforms_compose_like_the_renderer() {
        let outer = Xf::IDENTITY.then(Point::new(-100.0, 0.0), 1.0, Point::ZERO);
        let inner = outer.then(Point::ZERO, 2.0, Point::new(50.0, 50.0));
        // (60, 60) scaled about (50, 50) is (70, 70), then moved left 100.
        assert_eq!(inner.rect(Rect::new(60.0, 60.0, 10.0, 10.0)), Rect::new(-30.0, 70.0, 20.0, 20.0));
        assert_eq!(inner.to_local(Point::new(-30.0, 70.0)), Point::new(60.0, 60.0));
    }
}
