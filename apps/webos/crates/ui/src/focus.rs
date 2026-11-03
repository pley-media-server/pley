//! D-pad focus, ported from the TV app's `packages/tv/src/ui/focus.ts` with the Android TV app's
//! rules (`ui/tv/kit/Focus.kt`): focusable elements sit in groups that move in order (rows,
//! columns, grids) or by geometry (free); a move that would leave a group asks the group's edge
//! rule, then the group above; groups remember the element focused last in them; traps (panels,
//! dialogs, the open rail) keep focus until they close; and something is always focused.
//!
//! Each frame registers what it draws into a [`Tree`]. Moves are resolved against the previous
//! frame's tree, so the frame after a key press already draws the new focus.

use pley_base::{Key, Point, Rect};

use crate::id::{Id, IdMap};

/// A direction on the D-pad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    /// The four directions.
    pub const ALL: [Dir; 4] = [Dir::Left, Dir::Right, Dir::Up, Dir::Down];

    /// The direction a remote key moves in.
    pub fn from_key(key: Key) -> Option<Dir> {
        match key {
            Key::Left => Some(Dir::Left),
            Key::Right => Some(Dir::Right),
            Key::Up => Some(Dir::Up),
            Key::Down => Some(Dir::Down),
            _ => None,
        }
    }

    /// The key that moves this way.
    pub fn key(self) -> Key {
        match self {
            Dir::Left => Key::Left,
            Dir::Right => Key::Right,
            Dir::Up => Key::Up,
            Dir::Down => Key::Down,
        }
    }

    /// Whether it is Left or Right.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Dir::Left | Dir::Right)
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// A set of directions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Dirs(u8);

impl Dirs {
    /// No direction.
    pub const NONE: Dirs = Dirs(0);
    /// Left alone.
    pub const LEFT: Dirs = Dirs::of(Dir::Left);
    /// Right alone.
    pub const RIGHT: Dirs = Dirs::of(Dir::Right);
    /// Up alone.
    pub const UP: Dirs = Dirs::of(Dir::Up);
    /// Down alone.
    pub const DOWN: Dirs = Dirs::of(Dir::Down);
    /// Left and Right.
    pub const HORIZONTAL: Dirs = Dirs(Dirs::LEFT.0 | Dirs::RIGHT.0);
    /// Up and Down.
    pub const VERTICAL: Dirs = Dirs(Dirs::UP.0 | Dirs::DOWN.0);
    /// Every direction.
    pub const ALL: Dirs = Dirs(0b1111);

    /// The set of `dir` alone.
    pub const fn of(dir: Dir) -> Dirs {
        Dirs(1 << dir.index())
    }

    /// The directions of both sets.
    pub const fn union(self, other: Dirs) -> Dirs {
        Dirs(self.0 | other.0)
    }

    /// Whether `dir` is in the set.
    pub const fn contains(self, dir: Dir) -> bool {
        self.0 & (1 << dir.index()) != 0
    }
}

/// How focus moves among a group's items (its elements and the groups directly inside it, in the
/// order they are drawn).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupKind {
    /// Left and Right go to the previous and next item; Up and Down leave the group.
    Row,
    /// Up and Down go to the previous and next item; Left and Right leave.
    Column,
    /// Items fill rows of `columns`: Left and Right within a row (never wrapping), Up and Down a
    /// row; Down from above a shorter last row lands on its last item.
    Grid { columns: u16 },
    /// The nearest item in the direction, by geometry.
    Free,
}

/// What a move does when it would leave a group on one side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Edge {
    /// The group above decides (by its own order, or by geometry between groups).
    #[default]
    Pass,
    /// Focus stays where it is: Right at a row's end. [`crate::Ui::edge_hit`] tells the group.
    Stop,
    /// Focus goes into this element or group: a grid's Right edge to its jump control.
    To(Id),
    /// The move leaves the page: [`crate::Ui::failed_move`] tells the shell, which sends Left at
    /// a row's start to the rail.
    Out,
}

/// A focus group: how focus moves inside it, what it remembers, what it keeps and what happens
/// at its edges. Built from a [`GroupKind`] (`Group::row()`, `GroupKind::Row.into()`) and the
/// methods below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Group {
    pub kind: GroupKind,
    /// Coming back into the group focuses the element focused last in it (on by default, as the
    /// Android app's rows and grids restore their tile).
    pub memory: bool,
    /// Focus stays inside the group while it is drawn (a panel, a dialog, the open rail), and
    /// goes back to what had it before once the group is no longer drawn.
    pub trap: bool,
    /// The Magic Remote's wheel over the group scrolls it ([`crate::Ui::wheel`]) instead of
    /// moving focus.
    pub wheel: bool,
    /// Directions the group handles itself while focus is inside it: they reach
    /// [`crate::Ui::keys`] instead of moving focus.
    pub takes: Dirs,
    /// Where focus lands when it enters the group, before its memory (a panel's current choice).
    pub enter: Option<Id>,
    /// Sides a held key stops at, where a press goes on out: a library grid's Up to its chips and
    /// Right to its jump control (the Samsung app's `Exit` rules under `gate.Holding`).
    pub held_stops: Dirs,
    edges: [Edge; 4],
}

impl Group {
    /// A group of `kind` that remembers its last element and passes every edge on.
    pub const fn new(kind: GroupKind) -> Group {
        Group {
            kind,
            memory: true,
            trap: false,
            wheel: false,
            takes: Dirs::NONE,
            enter: None,
            held_stops: Dirs::NONE,
            edges: [Edge::Pass; 4],
        }
    }

    /// A row: Left and Right move along it.
    pub const fn row() -> Group {
        Group::new(GroupKind::Row)
    }

    /// A column: Up and Down move along it.
    pub const fn column() -> Group {
        Group::new(GroupKind::Column)
    }

    /// A grid of `columns` columns.
    pub const fn grid(columns: u16) -> Group {
        Group::new(GroupKind::Grid { columns })
    }

    /// A group moving by geometry alone.
    pub const fn free() -> Group {
        Group::new(GroupKind::Free)
    }

    /// What a move leaving the group toward `dir` does.
    pub const fn edge(mut self, dir: Dir, edge: Edge) -> Group {
        self.edges[dir.index()] = edge;
        self
    }

    /// The rule for leaving toward `dir`.
    pub const fn edge_for(&self, dir: Dir) -> Edge {
        self.edges[dir.index()]
    }

    /// A held key stops at these sides; a press of its own goes on.
    pub const fn held_stop(mut self, dirs: Dirs) -> Group {
        self.held_stops = dirs;
        self
    }

    /// Keeps focus inside while drawn.
    pub const fn trap(mut self) -> Group {
        self.trap = true;
        self
    }

    /// Enters by geometry every time, never returning to the last element.
    pub const fn no_memory(mut self) -> Group {
        self.memory = false;
        self
    }

    /// Takes the wheel turned over it.
    pub const fn wheel(mut self) -> Group {
        self.wheel = true;
        self
    }

    /// Handles these directions itself.
    pub const fn takes(mut self, dirs: Dirs) -> Group {
        self.takes = dirs;
        self
    }

    /// Lands on `id` when focus enters.
    pub const fn enter_at(mut self, id: Id) -> Group {
        self.enter = Some(id);
        self
    }
}

impl From<GroupKind> for Group {
    fn from(kind: GroupKind) -> Group {
        Group::new(kind)
    }
}

/// What a focusable element asks of the keys beyond focus, OK and Menu.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FocusOpts {
    /// OK held 500 ms is a long press here ([`crate::Response::long_pressed`]), and its release
    /// then does nothing. Without it, a held OK still selects on release.
    pub long_press: bool,
    /// Directions it handles itself while focused (a field's caret, the jump control's letters):
    /// they reach [`crate::Ui::keys`] instead of moving focus.
    pub takes: Dirs,
    /// OK while the pointer shows over it is a click where the pointer is (a scrubber seeks
    /// there), as the pointer's own click is.
    pub click_at: bool,
}

impl FocusOpts {
    /// Takes OK held 500 ms as a long press.
    pub const fn long_press(mut self) -> FocusOpts {
        self.long_press = true;
        self
    }

    /// Handles `dirs` itself while focused.
    pub const fn takes(mut self, dirs: Dirs) -> FocusOpts {
        self.takes = dirs;
        self
    }

    /// Takes OK, while the pointer shows over it, as a click there.
    pub const fn click_at(mut self) -> FocusOpts {
        self.click_at = true;
        self
    }
}

/// A page's focus, kept with it on the back stack: the element it had and each group's last
/// element. Coming back, the element takes focus again as soon as it is drawn, unless a key was
/// pressed first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageMemory {
    pub focused: Option<Id>,
    pub groups: Vec<(Id, Id)>,
}

pub(crate) const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
pub(crate) enum NodeKind {
    Element(FocusOpts),
    Group(Group),
    /// A rectangle only the pointer uses (a scrim): hovered and clicked, never focused.
    Area,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Node {
    pub id: Id,
    pub parent: u32,
    pub kind: NodeKind,
    /// On screen, in dp; a group's is the union of its items'.
    pub rect: Rect,
    /// The part of `rect` not clipped away, where the pointer finds it.
    pub hit: Rect,
    /// As registered, in the coordinates current then.
    pub local: Rect,
    first: u32,
    next: u32,
    /// Elements inside a group, at any depth.
    count: u32,
}

/// Everything focusable a frame drew, in the order it was drawn.
#[derive(Debug, Default)]
pub(crate) struct Tree {
    nodes: Vec<Node>,
    index: IdMap<u32>,
    /// Trap groups, in the order drawn: the last is where focus is kept.
    pub traps: Vec<u32>,
}

impl Tree {
    /// Empties the tree for a new frame, keeping its allocations, with the root group in place.
    pub fn reset(&mut self) {
        self.nodes.clear();
        self.index.clear();
        self.traps.clear();
        self.nodes.push(Node {
            id: Id::ROOT,
            parent: NONE,
            kind: NodeKind::Group(Group::free()),
            rect: Rect::ZERO,
            hit: Rect::ZERO,
            local: Rect::ZERO,
            first: NONE,
            next: NONE,
            count: 0,
        });
    }

    /// Adds a node under `parent`; `None` when `id` is drawn already this frame (the first wins).
    pub fn add(&mut self, id: Id, parent: u32, kind: NodeKind, local: Rect, rect: Rect, hit: Rect) -> Option<u32> {
        if self.nodes.is_empty() {
            self.reset();
        }
        let at = self.nodes.len() as u32;
        if id == Id::ROOT || self.index.contains_key(&id) {
            return None;
        }
        self.index.insert(id, at);
        self.nodes.push(Node { id, parent, kind, rect, hit, local, first: NONE, next: NONE, count: 0 });
        Some(at)
    }

    /// Links children to parents and works out groups' rectangles and counts, once a frame has
    /// drawn everything.
    pub fn close(&mut self) {
        if self.nodes.is_empty() {
            self.reset();
        }
        for n in &mut self.nodes {
            n.first = NONE;
            n.next = NONE;
            n.count = 0;
            if matches!(n.kind, NodeKind::Group(_)) {
                n.rect = Rect::ZERO;
            }
        }
        // Children come after their parents, so walking backwards finishes each group's
        // children before the group itself, and leaves each sibling list in drawing order.
        for i in (1..self.nodes.len()).rev() {
            let n = self.nodes[i];
            let p = n.parent as usize;
            self.nodes[i].next = self.nodes[p].first;
            self.nodes[p].first = i as u32;
            let (count, rect) = match n.kind {
                NodeKind::Element(_) => (1, n.rect),
                NodeKind::Group(_) if n.count > 0 => (n.count, n.rect),
                _ => continue,
            };
            let parent = &mut self.nodes[p];
            parent.count += count;
            parent.rect = parent.rect.union(&rect);
        }
    }

    pub fn get(&self, id: Id) -> Option<u32> {
        self.index.get(&id).copied().or((id == Id::ROOT && !self.nodes.is_empty()).then_some(0))
    }

    pub fn node(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }

    pub fn group_mut(&mut self, i: u32) -> Option<&mut Group> {
        match &mut self.nodes.get_mut(i as usize)?.kind {
            NodeKind::Group(g) => Some(g),
            _ => None,
        }
    }

    pub fn is_element(&self, i: u32) -> bool {
        matches!(self.node(i).kind, NodeKind::Element(_))
    }

    fn is_trap(&self, i: u32) -> bool {
        matches!(self.node(i).kind, NodeKind::Group(g) if g.trap)
    }

    /// Where focus is kept: the trap drawn last, else the root.
    pub fn scope(&self) -> u32 {
        self.traps.last().copied().unwrap_or(0)
    }

    /// Whether `i` is `ancestor` or inside it.
    pub fn within(&self, mut i: u32, ancestor: u32) -> bool {
        while i != NONE {
            if i == ancestor {
                return true;
            }
            i = self.node(i).parent;
        }
        false
    }

    /// A group's items: its elements and its groups that hold an element, in drawing order.
    pub fn items(&self, group: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut c = self.node(group).first;
        while c != NONE {
            let n = self.node(c);
            match n.kind {
                NodeKind::Element(_) => out.push(c),
                NodeKind::Group(_) if n.count > 0 => out.push(c),
                _ => {}
            }
            c = n.next;
        }
        out
    }

    /// The position of `child` among `group`'s items, without collecting them (containers ask
    /// every frame).
    pub fn item_index(&self, group: u32, child: u32) -> Option<usize> {
        let mut at = 0;
        let mut c = self.node(group).first;
        while c != NONE {
            let n = self.node(c);
            let item = match n.kind {
                NodeKind::Element(_) => true,
                NodeKind::Group(_) => n.count > 0,
                NodeKind::Area => false,
            };
            if c == child {
                return item.then_some(at);
            }
            at += item as usize;
            c = n.next;
        }
        None
    }

    /// The element or pointer area drawn last (so on top) where `p` is.
    pub fn hit_test(&self, p: Point) -> Option<u32> {
        let i = self
            .nodes
            .iter()
            .rposition(|n| matches!(n.kind, NodeKind::Element(_) | NodeKind::Area) && n.hit.contains(p))?;
        Some(i as u32)
    }

    /// The innermost group in scope that takes the wheel where `p` is.
    pub fn wheel_group_at(&self, p: Point) -> Option<u32> {
        let scope = self.scope();
        let i = self.nodes.iter().enumerate().rposition(|(i, n)| {
            matches!(n.kind, NodeKind::Group(g) if g.wheel) && n.rect.contains(p) && self.within(i as u32, scope)
        })?;
        Some(i as u32)
    }

    /// Whether the element at `i`, or a group around it, handles `dir` itself.
    pub fn takes(&self, mut i: u32, dir: Dir) -> bool {
        while i != NONE {
            let taken = match self.node(i).kind {
                NodeKind::Element(o) => o.takes,
                NodeKind::Group(g) => g.takes,
                NodeKind::Area => Dirs::NONE,
            };
            if taken.contains(dir) {
                return true;
            }
            i = self.node(i).parent;
        }
        false
    }
}

/// The element to focus entering `target` (moving toward `dir` from `from`, when a move enters
/// it): an element is itself; a group lands on its entry point, else its remembered element,
/// else the item nearest the source (along the group for rows and columns; in the direction for
/// free groups), else its first item (the TV app's `enter`).
pub(crate) fn enter(tree: &Tree, memory: &IdMap<Id>, target: u32, dir: Option<Dir>, from: Option<Rect>) -> Option<u32> {
    let n = tree.node(target);
    let g = match n.kind {
        NodeKind::Element(_) => return Some(target),
        NodeKind::Area => return None,
        NodeKind::Group(g) => g,
    };
    if n.count == 0 {
        return None;
    }
    if let Some(e) = g.enter.and_then(|id| tree.get(id)).filter(|&e| e != target && tree.within(e, target))
        && let Some(t) = enter(tree, memory, e, dir, from)
    {
        return Some(t);
    }
    if g.memory {
        let last = memory.get(&n.id).and_then(|&id| tree.get(id));
        if let Some(m) = last.filter(|&m| tree.is_element(m) && tree.within(m, target)) {
            return Some(m);
        }
    }
    let items = tree.items(target);
    let first = *items.first()?;
    let pick = match (from, g.kind) {
        (None, _) => first,
        (Some(r), GroupKind::Free) => {
            nearest(tree, r, &items, dir).or_else(|| nearest(tree, r, &items, None)).unwrap_or(first)
        }
        (Some(r), kind) => {
            let p = nearest(tree, r, &items, None).unwrap_or(first);
            match kind {
                GroupKind::Row => nearest_on_axis(tree, r, &items, true).unwrap_or(p),
                GroupKind::Column => nearest_on_axis(tree, r, &items, false).unwrap_or(p),
                _ => p,
            }
        }
    };
    enter(tree, memory, pick, dir, from)
}

/// Android's FocusFinder, simplified as the TV app has it: candidates must lie in the direction
/// (with 0.5 dp of tolerance, overlapping back at most half the smaller size); those in the
/// source's beam (overlapping it across the direction) win over any outside it; among equals
/// the lowest 13·major² + minor² wins, major being the gap along the direction and minor the
/// distance between centres across it. Without a direction, the nearest centre wins.
pub(crate) fn nearest(tree: &Tree, from: Rect, candidates: &[u32], dir: Option<Dir>) -> Option<u32> {
    let mut best = None;
    let mut best_score = f32::INFINITY;
    let mut best_in_beam = false;
    let c = from.center();
    for &cand in candidates {
        let r = tree.node(cand).rect;
        if r.w == 0.0 && r.h == 0.0 {
            continue;
        }
        let (major, minor, in_beam) = match dir {
            Some(d @ (Dir::Left | Dir::Right)) => {
                let right = d == Dir::Right;
                let behind = if right {
                    r.right() <= from.right() + 0.5 && r.x < from.right() - 0.5
                } else {
                    r.x >= from.x - 0.5 && r.right() > from.x + 0.5
                };
                let major = if right { r.x - from.right() } else { from.x - r.right() };
                if behind || major < -r.w.min(from.w) / 2.0 {
                    continue;
                }
                (major, (r.center().y - c.y).abs(), r.bottom() > from.y && r.y < from.bottom())
            }
            Some(d) => {
                let down = d == Dir::Down;
                let behind = if down {
                    r.bottom() <= from.bottom() + 0.5 && r.y < from.bottom() - 0.5
                } else {
                    r.y >= from.y - 0.5 && r.bottom() > from.y + 0.5
                };
                let major = if down { r.y - from.bottom() } else { from.y - r.bottom() };
                if behind || major < -r.h.min(from.h) / 2.0 {
                    continue;
                }
                (major, (r.center().x - c.x).abs(), r.right() > from.x && r.x < from.right())
            }
            None => ((r.center().x - c.x).hypot(r.center().y - c.y), 0.0, true),
        };
        let m = major.max(0.0);
        let score = 13.0 * m * m + minor * minor;
        if (in_beam && !best_in_beam) || (in_beam == best_in_beam && score < best_score) {
            best = Some(cand);
            best_score = score;
            best_in_beam = in_beam;
        }
    }
    best
}

/// The item whose span along the axis is nearest the source's centre on it: entering a row from
/// above lands on the tile under the one left.
fn nearest_on_axis(tree: &Tree, from: Rect, items: &[u32], horizontal: bool) -> Option<u32> {
    let c = if horizontal { from.center().x } else { from.center().y };
    let mut best = None;
    let mut best_d = f32::INFINITY;
    for &it in items {
        let r = tree.node(it).rect;
        let (lo, hi) = if horizontal { (r.x, r.right()) } else { (r.y, r.bottom()) };
        let d = if c < lo {
            lo - c
        } else if c > hi {
            c - hi
        } else {
            0.0
        };
        if d < best_d {
            best_d = d;
            best = Some(it);
        }
    }
    best
}

/// The item a move from `node` lands on inside its group, or `None` at the group's edge.
fn step(tree: &Tree, group: &Group, items: &[u32], node: u32, dir: Dir, from: Rect) -> Option<u32> {
    let at = items.iter().position(|&i| i == node)?;
    let get = |i: usize| items.get(i).copied();
    match group.kind {
        GroupKind::Row => match dir {
            Dir::Left => at.checked_sub(1).and_then(get),
            Dir::Right => get(at + 1),
            _ => None,
        },
        GroupKind::Column => match dir {
            Dir::Up => at.checked_sub(1).and_then(get),
            Dir::Down => get(at + 1),
            _ => None,
        },
        GroupKind::Grid { columns } => {
            let cols = columns.max(1) as usize;
            match dir {
                Dir::Left => (at % cols > 0).then(|| get(at - 1)).flatten(),
                Dir::Right => (at % cols < cols - 1).then(|| get(at + 1)).flatten(),
                Dir::Up => at.checked_sub(cols).and_then(get),
                Dir::Down => {
                    if at + cols < items.len() {
                        return get(at + cols);
                    }
                    // A last row shorter than the others: its last item.
                    let rem = items.len() % cols;
                    let last_row_start = items.len() - if rem == 0 { cols } else { rem };
                    (at < last_row_start).then(|| get(items.len() - 1)).flatten()
                }
            }
        }
        GroupKind::Free => {
            let others: Vec<u32> = items.iter().copied().filter(|&i| i != node).collect();
            nearest(tree, from, &others, Some(dir))
        }
    }
}

/// What a direction key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    /// Focus goes to this element.
    To(u32),
    /// Focus stays: the group's edge stopped it.
    Stay { edge: u32 },
    /// Nothing took the move; `edge` is the group whose edge sent it out, if one did.
    Fail { edge: Option<u32> },
}

/// One step of focus toward `dir` (the TV app's `FocusManager.move`): the focused element's group
/// picks the next item; at its edge, the group's rule applies, then the group above is asked, up
/// to the scope. With nothing focused in the scope, focus enters the scope. A `held` key stops at
/// a group's [`Group::held_stops`].
pub(crate) fn move_focus(tree: &Tree, memory: &IdMap<Id>, focused: Option<Id>, dir: Dir, held: bool) -> Move {
    let scope = tree.scope();
    let from = focused.and_then(|id| tree.get(id)).filter(|&i| tree.is_element(i) && tree.within(i, scope));
    let Some(from) = from else {
        return enter(tree, memory, scope, Some(dir), None).map_or(Move::Fail { edge: None }, Move::To);
    };
    let from_rect = tree.node(from).rect;
    let mut node = from;
    while node != scope {
        let group = tree.node(node).parent;
        let NodeKind::Group(g) = tree.node(group).kind else { break };
        let items = tree.items(group);
        if let Some(next) = step(tree, &g, &items, node, dir, from_rect)
            && let Some(t) = enter(tree, memory, next, Some(dir), Some(from_rect))
        {
            return Move::To(t);
        }
        if held && g.held_stops.contains(dir) {
            return Move::Stay { edge: group };
        }
        match g.edge_for(dir) {
            Edge::Pass => {}
            Edge::Stop => return Move::Stay { edge: group },
            Edge::Out => return Move::Fail { edge: Some(group) },
            Edge::To(id) => {
                let target = tree.get(id).filter(|&i| tree.within(i, scope));
                if let Some(t) = target.and_then(|i| enter(tree, memory, i, Some(dir), Some(from_rect))) {
                    return Move::To(t);
                }
            }
        }
        node = group;
    }
    Move::Fail { edge: None }
}

/// A trap's place on the stack: what had focus when it opened, and whether the pointer gave it.
#[derive(Clone, Copy, Debug)]
struct Trap {
    id: Id,
    opener: Option<Id>,
    by_pointer: bool,
}

/// Who has focus, and everything focus remembers between frames.
#[derive(Debug, Default)]
pub(crate) struct Focus {
    pub focused: Option<Id>,
    /// Whether the pointer gave the current focus: scrollers then nudge instead of re-centring.
    pub by_pointer: bool,
    /// Each group's last focused element.
    pub memory: IdMap<Id>,
    traps: Vec<Trap>,
    /// A page's remembered element, waiting to be drawn.
    pub wanted: Option<Id>,
    /// An element or group asked for, with the frames it may still wait to be drawn.
    pub request: Option<(Id, u8)>,
}

impl Focus {
    /// Focuses the element at `i`; every group above it remembers it. Whether focus changed.
    pub fn set(&mut self, tree: &Tree, i: u32, by_pointer: bool) -> bool {
        let id = tree.node(i).id;
        if self.focused == Some(id) {
            return false;
        }
        self.focused = Some(id);
        self.by_pointer = by_pointer;
        let mut p = tree.node(i).parent;
        while p != NONE {
            self.memory.insert(tree.node(p).id, id);
            p = tree.node(p).parent;
        }
        true
    }

    /// Settles focus once a frame has drawn `cur` (after `prev`): traps that closed give focus
    /// back to what opened them, new traps take it, a page's remembered element and a requested
    /// one take it when drawn, and an element that wasn't drawn hands focus to the nearest one
    /// left in its group, then in the groups around it, then to the scope's entry point (its
    /// initial element, else its first). Whether focus changed.
    pub fn settle(&mut self, prev: &Tree, cur: &Tree) -> bool {
        let before = self.focused;
        let mut restore = None;
        for k in (0..self.traps.len()).rev() {
            let t = self.traps[k];
            if !cur.get(t.id).is_some_and(|i| cur.is_trap(i)) {
                if k == self.traps.len() - 1 {
                    restore = Some(t);
                }
                self.traps.remove(k);
            }
        }
        let scope = cur.scope();
        let in_scope = |id: Id| cur.get(id).filter(|&i| cur.is_element(i) && cur.within(i, scope));
        if let Some(t) = restore
            && let Some(i) = t.opener.and_then(in_scope)
        {
            self.set(cur, i, t.by_pointer);
        }
        for &ti in &cur.traps {
            let id = cur.node(ti).id;
            if !self.traps.iter().any(|t| t.id == id) {
                self.traps.push(Trap { id, opener: self.focused, by_pointer: self.by_pointer });
            }
        }
        if let Some(i) = self.wanted.and_then(in_scope) {
            self.set(cur, i, false);
            self.wanted = None;
        }
        if let Some((id, waits)) = self.request.take() {
            match cur.get(id) {
                Some(i) if cur.within(i, scope) => {
                    if let Some(t) = enter(cur, &self.memory, i, None, None) {
                        self.set(cur, t, false);
                    }
                }
                None if waits > 0 => self.request = Some((id, waits - 1)),
                _ => {}
            }
        }
        let target = match self.focused {
            Some(f) if in_scope(f).is_some() => None,
            Some(f) if cur.get(f).is_some_and(|i| cur.is_element(i)) => enter(cur, &self.memory, scope, None, None),
            Some(f) => refocus(prev, cur, &self.memory, f),
            None => enter(cur, &self.memory, scope, None, None),
        };
        if let Some(t) = target {
            self.set(cur, t, false);
        }
        self.focused != before
    }

    /// The page's focus, to keep on the back stack.
    pub fn export_page(&self) -> PageMemory {
        PageMemory { focused: self.focused, groups: self.memory.iter().map(|(&g, &e)| (g, e)).collect() }
    }

    /// Starts a page's focus over from `mem`.
    pub fn import_page(&mut self, mem: PageMemory) {
        self.focused = None;
        self.by_pointer = false;
        self.wanted = mem.focused;
        self.request = None;
        self.traps.clear();
        self.memory.clear();
        self.memory.extend(mem.groups);
    }
}

/// Where focus goes when the element `lost` wasn't drawn: the item nearest where it was among
/// what is left of its group, then of each group around it inside the scope (the tile now in its
/// place, else the row now in its place), else the scope's entry point.
fn refocus(prev: &Tree, cur: &Tree, memory: &IdMap<Id>, lost: Id) -> Option<u32> {
    let scope = cur.scope();
    if let Some(old) = prev.get(lost) {
        let was = prev.node(old).rect;
        let mut g = prev.node(old).parent;
        // The root mixes the shell's elements with the page's: past the page's own groups, focus
        // goes to the scope's entry point rather than whatever lies nearest.
        while g != NONE && g != 0 {
            if let Some(cg) = cur.get(prev.node(g).id).filter(|&c| c != 0 && cur.within(c, scope)) {
                let items = cur.items(cg);
                if let Some(t) = nearest(cur, was, &items, None).and_then(|p| enter(cur, memory, p, None, Some(was))) {
                    return Some(t);
                }
                if cg == scope {
                    break;
                }
            }
            g = prev.node(g).parent;
        }
    }
    enter(cur, memory, scope, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el() -> NodeKind {
        NodeKind::Element(FocusOpts::default())
    }

    /// A tree from (id, parent id, kind, rect) rows, root first implied.
    fn tree(rows: &[(&str, &str, NodeKind, Rect)]) -> Tree {
        let mut t = Tree::default();
        t.reset();
        for &(id, parent, kind, r) in rows {
            let p = if parent.is_empty() { 0 } else { t.get(Id::new(parent)).unwrap() };
            t.add(Id::new(id), p, kind, r, r, r);
            if matches!(kind, NodeKind::Group(g) if g.trap) {
                let i = t.get(Id::new(id)).unwrap();
                t.traps.push(i);
            }
        }
        t.close();
        t
    }

    fn id_at(t: &Tree, i: u32) -> Id {
        t.node(i).id
    }

    #[test]
    fn nearest_prefers_the_beam_then_the_weighted_distance() {
        let t = tree(&[
            ("from", "", el(), Rect::new(100.0, 100.0, 50.0, 50.0)),
            // In the beam to the right but far.
            ("beam", "", el(), Rect::new(400.0, 110.0, 50.0, 50.0)),
            // Out of the beam but close.
            ("near", "", el(), Rect::new(160.0, 200.0, 50.0, 50.0)),
            // Behind.
            ("behind", "", el(), Rect::new(0.0, 100.0, 50.0, 50.0)),
        ]);
        let all: Vec<u32> = (2..=4).collect();
        let from = t.node(1).rect;
        assert_eq!(nearest(&t, from, &all, Some(Dir::Right)).map(|i| id_at(&t, i)), Some(Id::new("beam")));
        assert_eq!(nearest(&t, from, &all, Some(Dir::Left)).map(|i| id_at(&t, i)), Some(Id::new("behind")));
        assert_eq!(nearest(&t, from, &all, Some(Dir::Up)), None);
        assert_eq!(nearest(&t, from, &all, None).map(|i| id_at(&t, i)), Some(Id::new("behind")));
    }

    #[test]
    fn groups_count_and_bound_their_items() {
        let t = tree(&[
            ("row", "", NodeKind::Group(Group::row()), Rect::ZERO),
            ("a", "row", el(), Rect::new(0.0, 0.0, 10.0, 10.0)),
            ("empty", "row", NodeKind::Group(Group::row()), Rect::ZERO),
            ("b", "row", el(), Rect::new(20.0, 0.0, 10.0, 10.0)),
            ("scrim", "", NodeKind::Area, Rect::new(0.0, 0.0, 960.0, 540.0)),
        ]);
        let row = t.get(Id::new("row")).unwrap();
        assert_eq!(t.items(row).iter().map(|&i| id_at(&t, i)).collect::<Vec<_>>(), [Id::new("a"), Id::new("b")]);
        assert_eq!(t.node(row).rect, Rect::new(0.0, 0.0, 30.0, 10.0));
        assert_eq!(t.items(0), [row]);
        // The scrim is on top of everything.
        assert_eq!(t.hit_test(Point::new(5.0, 5.0)).map(|i| id_at(&t, i)), Some(Id::new("scrim")));
    }

    #[test]
    fn a_short_last_row_takes_down_on_its_last_cell() {
        let mut rows = vec![("grid", "", NodeKind::Group(Group::grid(3)), Rect::ZERO)];
        let names: Vec<String> = (0..5).map(|i| format!("c{i}")).collect();
        for (i, n) in names.iter().enumerate() {
            let r = Rect::new((i % 3) as f32 * 100.0, (i / 3) as f32 * 100.0, 90.0, 90.0);
            rows.push((n.as_str(), "grid", el(), r));
        }
        let t = tree(&rows);
        let memory = IdMap::default();
        let go = |from: &str, d| match move_focus(&t, &memory, Some(Id::new(from)), d, false) {
            Move::To(i) => Some(id_at(&t, i)),
            _ => None,
        };
        assert_eq!(go("c0", Dir::Right), Some(Id::new("c1")));
        assert_eq!(go("c2", Dir::Right), None);
        assert_eq!(go("c3", Dir::Left), None);
        assert_eq!(go("c1", Dir::Down), Some(Id::new("c4")));
        assert_eq!(go("c2", Dir::Down), Some(Id::new("c4")));
        assert_eq!(go("c4", Dir::Up), Some(Id::new("c1")));
        assert_eq!(go("c4", Dir::Down), None);
    }
}
