//! The categories column (`CategoryColumn` in `TvLiveTvScreen.kt`, `LiveTv.groupSections`): beside
//! the guide, and in the player's channel list when another group is chosen. In order: the search
//! field (on the page), the groups a search names, Favourites, Recently watched and All channels with
//! their counts, then the regions (one open at a time, OK opening or closing it) and the other
//! groups. The category shown is marked; focused rows take the accent; OK on a category shows it.

use pley_base::{Icon, Point, Rect, TextOpts};
use pley_core::format::grouped;
use pley_core::model::{LiveGroup, LiveGroups};
use pley_kit::{ListState, TextField};
use pley_player::live::Category;
use pley_player::live::data::{Region, category_name, group_sections};
use pley_ui::theme::{self, radius};
use pley_ui::{Dirs, Edge, Id, Response, Ui};

use crate::draw::line_mid;

/// A row's height.
pub const ROW_H: f32 = 40.0;
/// Between rows.
pub const GAP: f32 = 2.0;
/// How far the rows' fill reaches left of the page's gutter, so their text lines up with it.
pub const INSET: f32 = 12.0;
/// The column's width.
pub const WIDTH: f32 = 210.0;
/// A group's name starts where the categories' names do, after their icons.
pub const GROUP_INSET: f32 = INSET + 26.0;
/// The search field and the room under it.
const SEARCH_H: f32 = 44.0 + 8.0;
/// A heading: 14 above, a line of `Tv.Badge`, 6 under.
const HEADING_H: f32 = 14.0 + 13.0 + 6.0;
/// The most groups a search lists.
pub const FOUND_MAX: usize = 12;

/// One line of the column.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Search,
    Heading(&'static str),
    /// A group whose name has the searched words in it.
    Found(LiveGroup),
    Favorites,
    Recent,
    All,
    /// A region gathering its groups; `open` shows them under it.
    Region {
        region: String,
        count: i64,
        open: bool,
    },
    /// A group, its name `indent` in.
    Group {
        group: LiveGroup,
        indent: f32,
    },
}

impl Line {
    /// Its height in the column.
    pub fn height(&self) -> f32 {
        match self {
            Line::Search => SEARCH_H,
            Line::Heading(_) => HEADING_H,
            _ => ROW_H,
        }
    }

    /// The category it shows, if it is one.
    pub fn category(&self) -> Option<Category> {
        match self {
            Line::Favorites => Some(Category::Favorites),
            Line::Recent => Some(Category::Recent),
            Line::All => Some(Category::All),
            Line::Found(g) | Line::Group { group: g, .. } => Some(Category::Group(g.name.clone())),
            _ => None,
        }
    }
}

/// The groups as the column lists them (`LiveTv.groupSections`): by region, and the others.
#[derive(Clone, Debug, Default)]
struct Sections {
    /// The [`ColumnView::rev`] they were made for.
    rev: u64,
    regions: Vec<Region>,
    others: Vec<LiveGroup>,
}

impl Sections {
    fn of(groups: Option<&LiveGroups>, rev: u64) -> Sections {
        let (regions, others) = group_sections(groups.map(|g| g.groups.as_slice()).unwrap_or(&[]));
        Sections { rev, regions, others }
    }

    /// The region holding group `name`, when the groups are gathered by region.
    fn region_of(&self, name: &str) -> Option<String> {
        self.regions.iter().find(|r| r.groups.iter().any(|g| g.name == name)).map(|r| r.region.clone())
    }

    /// The column's lines: with `search` its field first, the groups a search `found`, the three
    /// marked categories, then the regions (`open`'s groups under it) and the other groups.
    fn lines(&self, found: &[LiveGroup], open: Option<&str>, search: bool) -> Vec<Line> {
        let mut out = Vec::new();
        if search {
            out.push(Line::Search);
        }
        if !found.is_empty() {
            out.push(Line::Heading("Groups found"));
            out.extend(found.iter().cloned().map(Line::Found));
        }
        out.extend([Line::Favorites, Line::Recent, Line::All]);
        let has_regions = !self.regions.is_empty();
        if has_regions {
            out.push(Line::Heading("Regions"));
            for r in &self.regions {
                let expanded = open == Some(r.region.as_str());
                out.push(Line::Region { region: r.region.clone(), count: r.count, open: expanded });
                if expanded {
                    let indent = GROUP_INSET + 16.0;
                    out.extend(r.groups.iter().map(|g| Line::Group { group: g.clone(), indent }));
                }
            }
        }
        // A lone group without regions is All channels again: it isn't listed.
        let others = &self.others;
        if others.len() > 1 || (others.len() == 1 && has_regions) {
            out.push(Line::Heading(if has_regions { "More groups" } else { "Groups" }));
            out.extend(others.iter().map(|g| Line::Group { group: g.clone(), indent: GROUP_INSET }));
        }
        out
    }
}

/// The column's lines for `groups` (see [`Sections::lines`]).
#[cfg(test)]
fn lines(groups: Option<&LiveGroups>, found: &[LiveGroup], open: Option<&str>, search: bool) -> Vec<Line> {
    Sections::of(groups, 0).lines(found, open, search)
}

/// Where `selected` shows in `lines`: its row, or its `region`'s while that is closed (the search's
/// found groups never count).
fn shown_line(lines: &[Line], selected: &Category, region: Option<&str>) -> Option<usize> {
    let own = lines.iter().position(|l| !matches!(l, Line::Found(_)) && l.category().as_ref() == Some(selected));
    own.or_else(|| {
        let region = region?;
        lines.iter().position(|l| matches!(l, Line::Region { region: r, .. } if r == region))
    })
}

/// The search field at the top of the column.
#[derive(Clone, Copy, Debug)]
pub struct Search<'a> {
    pub text: &'a str,
    pub placeholder: &'a str,
    pub typing: bool,
}

/// What the column shows and marks.
#[derive(Clone, Copy, Debug)]
pub struct ColumnView<'a> {
    pub groups: Option<&'a LiveGroups>,
    /// Changes whenever `groups` or `found` do: the lines are made again only then, not every frame
    /// (a provider can name thousands of groups).
    pub rev: u64,
    /// The category shown, marked.
    pub selected: Option<&'a Category>,
    pub found: &'a [LiveGroup],
    pub search: Option<Search<'a>>,
}

/// What was chosen in the column this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Picked {
    Category(Category),
    /// OK on the search field: typing starts.
    Search,
}

/// What the column did this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColumnResponse {
    pub picked: Option<Picked>,
    /// Focus is in the column.
    pub focused: bool,
    /// The menu key was released on one of its lines.
    pub menu: bool,
}

/// What the column keeps between frames.
#[derive(Clone, Debug, Default)]
pub struct Column {
    pub list: ListState,
    /// The region open.
    open: Option<String>,
    /// The region of the category shown when `open` was last set from it.
    opened_for: Option<Option<String>>,
    sections: Option<Sections>,
    /// The region of the category shown, for the rev and the category it was found for.
    shown_region: Option<(u64, Category, Option<String>)>,
    /// The lines as last made, and what from: the rev, the region open and the search field.
    lines: Vec<Line>,
    lines_for: Option<(u64, Option<String>, bool)>,
}

impl Column {
    /// Makes the lines again when what they show changed: the region of the category shown open
    /// unless closed since.
    fn refresh(&mut self, v: &ColumnView) {
        if self.sections.as_ref().is_none_or(|s| s.rev != v.rev) {
            self.sections = Some(Sections::of(v.groups, v.rev));
        }
        let sections = self.sections.as_ref().expect("made above");
        match v.selected {
            Some(c @ Category::Group(name)) => {
                if self.shown_region.as_ref().is_none_or(|(rev, s, _)| *rev != v.rev || s != c) {
                    self.shown_region = Some((v.rev, c.clone(), sections.region_of(name)));
                }
            }
            _ => self.shown_region = None,
        }
        let shown = self.shown_region.as_ref().and_then(|s| s.2.as_deref());
        if self.opened_for.as_ref().map(Option::as_deref) != Some(shown) {
            self.open = shown.map(str::to_string);
            self.opened_for = Some(self.open.clone());
        }
        let search = v.search.is_some();
        let made = self.lines_for.as_ref().is_some_and(|(rev, open, with_search)| {
            *rev == v.rev && open.as_deref() == self.open.as_deref() && *with_search == search
        });
        if !made {
            self.lines = sections.lines(v.found, self.open.as_deref(), search);
            self.lines_for = Some((v.rev, self.open.clone(), search));
        }
    }

    /// The line of the category shown: its row, or its region's while that is closed; else the first.
    pub fn shown_at(&mut self, v: &ColumnView) -> usize {
        self.refresh(v);
        let region = self.shown_region.as_ref().and_then(|s| s.2.as_deref());
        v.selected.and_then(|s| shown_line(&self.lines, s, region)).unwrap_or(0)
    }

    /// Sends focus to the category shown, brought into view first (Left from the channels).
    pub fn focus_shown(&mut self, ui: &mut Ui, id: Id, v: &ColumnView) {
        let at = self.shown_at(v);
        self.list.index = at;
        ui.request_focus(id.with(at as u64));
    }

    /// Draws the column in `view`; `up` is what Up past its first line does, and a held key stops
    /// at the sides `held_stops` names.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        id: Id,
        view: Rect,
        v: &ColumnView,
        up: Edge,
        held_stops: Dirs,
    ) -> ColumnResponse {
        self.refresh(v);
        let lines = &self.lines;
        let mut out = ColumnResponse::default();
        let mut toggled: Option<String> = None;
        let list = pley_kit::List::new(view).gap(GAP).edges(up, Edge::Pass).held_stop(held_stops);
        let r = list.show(
            ui,
            id,
            &mut self.list,
            lines.len(),
            |_, i| lines[i].height(),
            |ui, slot| {
                let line = &lines[slot.index];
                match line {
                    Line::Search => {
                        let s = v.search.expect("a search line has its field");
                        let field = TextField::new(s.text, slot.rect.w - (INSET - 10.0))
                            .placeholder(s.placeholder)
                            .icon(Icon::Search)
                            .typing(s.typing);
                        let at = Point::new(slot.rect.x + INSET - 10.0, slot.rect.y);
                        let r = field.show(ui, slot.id, at);
                        if r.activated {
                            out.picked = Some(Picked::Search);
                        }
                        out.menu |= r.menu;
                    }
                    Line::Heading(text) => {
                        let at = Point::new(slot.rect.x + INSET, slot.rect.y + 14.0);
                        let opts = TextOpts::line(slot.rect.w - INSET);
                        ui.text(at, &text.to_uppercase(), &theme::BADGE, &opts, theme::SUBTLE);
                    }
                    _ => {
                        let selected = v
                            .selected
                            .is_some_and(|s| !matches!(line, Line::Found(_)) && line.category().as_ref() == Some(s));
                        let r = row(ui, slot.id, slot.rect, line, v, selected);
                        out.menu |= r.menu;
                        if r.activated {
                            match line {
                                Line::Region { region, .. } => toggled = Some(region.clone()),
                                l => out.picked = l.category().map(Picked::Category),
                            }
                        }
                    }
                }
            },
        );
        out.focused = r.focused.is_some();
        if let Some(region) = toggled {
            self.open = if self.open.as_deref() == Some(region.as_str()) { None } else { Some(region) };
            ui.request_repaint();
        }
        out
    }
}

/// One category's row: its icon, name and count; the one shown on Surface3 with an accent icon;
/// focused, the accent with OnAccent (`CategoryRow`).
fn row(ui: &mut Ui, id: Id, rect: Rect, line: &Line, v: &ColumnView, selected: bool) -> Response {
    let r = ui.focusable(id, rect);
    if r.activated {
        ui.request_repaint();
    }
    let lit = r.focused || r.pressed;
    let fill = if r.pressed {
        theme::ACCENT_BRIGHT
    } else if r.focused {
        theme::ACCENT
    } else if selected {
        theme::SURFACE3
    } else {
        pley_base::Color::TRANSPARENT
    };
    if fill.a > 0 {
        ui.fill(rect, fill, radius::CONTROL);
    }
    let content = if lit {
        theme::ON_ACCENT
    } else if selected {
        theme::FG
    } else {
        theme::MUTED
    };
    let groups = v.groups;
    let (icon, name, count, indent) = match line {
        Line::Favorites => (Some(Icon::Star), "Favourites".to_string(), groups.map(|g| g.favorites), INSET),
        Line::Recent => (Some(Icon::History), "Recently watched".to_string(), groups.map(|g| g.recent), INSET),
        Line::All => (Some(Icon::Layers), "All channels".to_string(), groups.map(|g| g.total), INSET),
        Line::Region { region, count, open } => {
            let icon = if *open { Icon::ChevronDown } else { Icon::ChevronRight };
            (Some(icon), region.clone(), Some(*count), INSET)
        }
        Line::Found(g) => {
            let name = category_name(&Category::Group(g.name.clone()), std::slice::from_ref(g));
            (None, name, Some(g.count), GROUP_INSET)
        }
        Line::Group { group, indent } => {
            let name = if *indent > GROUP_INSET {
                group.shown_label().to_string()
            } else if group.name.is_empty() {
                "No group".to_string()
            } else {
                group.name.clone()
            };
            (None, name, Some(group.count), *indent)
        }
        Line::Search | Line::Heading(_) => return r,
    };
    let mut x = rect.x + indent;
    if let Some(icon) = icon {
        let tint = if selected && !lit { theme::ACCENT } else { content };
        ui.icon(Rect::new(x, rect.y + (rect.h - 16.0) / 2.0, 16.0, 16.0), icon, tint);
        x += 16.0 + 10.0;
    }
    let right = rect.right() - 10.0;
    let mut name_end = right;
    if let Some(n) = count {
        let text = grouped(n);
        let layout = ui.layout(&text, &theme::CAPTION, &TextOpts::default());
        let w = layout.size.w;
        crate::draw::text_mid(ui, right - w, rect.y, rect.h, layout, content.alpha(0.7));
        name_end = right - w - 8.0;
    }
    line_mid(ui, &name, &theme::LABEL, x, rect.y, rect.h, name_end - x, content);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(name: &str, region: Option<&str>, label: &str, count: i64) -> LiveGroup {
        LiveGroup { name: name.into(), region: region.map(str::to_string), label: label.into(), count }
    }

    fn lineup() -> LiveGroups {
        LiveGroups {
            total: 40,
            favorites: 2,
            recent: 3,
            groups: vec![
                group("UK| News", Some("UK"), "News", 6),
                group("UK| Sport", Some("UK"), "Sport", 8),
                group("FR| Cinéma", Some("FR"), "Cinéma", 6),
                group("FR| Sport", Some("FR"), "Sport", 4),
                group("Kids", None, "Kids", 6),
                group("Music", None, "Music", 6),
            ],
        }
    }

    fn names(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| match l {
                Line::Search => "search".into(),
                Line::Heading(h) => format!("# {h}"),
                Line::Found(g) => format!("found {}", g.name),
                Line::Favorites => "favourites".into(),
                Line::Recent => "recent".into(),
                Line::All => "all".into(),
                Line::Region { region, open, .. } => format!("{region}{}", if *open { " v" } else { " >" }),
                Line::Group { group, indent } => {
                    format!("{}{}", if *indent > GROUP_INSET { "  " } else { "" }, group.name)
                }
            })
            .collect()
    }

    #[test]
    fn regions_gather_their_groups_one_open_at_a_time() {
        let g = lineup();
        let l = lines(Some(&g), &[], Some("UK"), true);
        assert_eq!(
            names(&l),
            [
                "search",
                "favourites",
                "recent",
                "all",
                "# Regions",
                "FR >",
                "UK v",
                "  UK| News",
                "  UK| Sport",
                "# More groups",
                "Kids",
                "Music"
            ]
        );
        // A group in a closed region shows on its region's row.
        let closed = lines(Some(&g), &[], None, true);
        assert_eq!(shown_line(&closed, &Category::Group("FR| Sport".into()), Some("FR")), Some(5));
        assert_eq!(shown_line(&l, &Category::Group("UK| Sport".into()), Some("UK")), Some(8));
        assert_eq!(shown_line(&l, &Category::All, None), Some(3));
    }

    #[test]
    fn with_one_region_every_group_is_listed_flat() {
        let mut g = lineup();
        g.groups.retain(|g| !g.name.starts_with("FR"));
        let l = lines(Some(&g), &[], None, false);
        assert_eq!(names(&l), ["favourites", "recent", "all", "# Groups", "UK| News", "UK| Sport", "Kids", "Music"]);
    }

    #[test]
    fn a_search_lists_the_groups_it_names_first() {
        let g = lineup();
        let found = vec![group("UK| Sport", Some("UK"), "Sport", 8)];
        let l = lines(Some(&g), &found, None, true);
        assert_eq!(names(&l)[..4], ["search", "# Groups found", "found UK| Sport", "favourites"]);
        // The found row isn't the category's own row.
        assert_eq!(shown_line(&l, &Category::Favorites, None), Some(3));
    }

    #[test]
    fn the_shown_region_opens_by_itself_until_closed() {
        let g = lineup();
        let shown = Category::Group("FR| Sport".into());
        let v = ColumnView { groups: Some(&g), rev: 1, selected: Some(&shown), found: &[], search: None };
        let mut c = Column::default();
        let shown_lines = |c: &mut Column, v: &ColumnView| {
            c.refresh(v);
            names(&c.lines)
        };
        assert!(shown_lines(&mut c, &v).contains(&"FR v".to_string()));
        assert_eq!(c.shown_at(&v), 6, "the group's own row, its region open");
        c.open = None;
        assert!(shown_lines(&mut c, &v).contains(&"FR >".to_string()), "closing it holds");
        assert_eq!(c.shown_at(&v), 4, "its region's row while closed");
        let other = Category::Group("UK| News".into());
        let v = ColumnView { selected: Some(&other), ..v };
        assert!(shown_lines(&mut c, &v).contains(&"UK v".to_string()), "another region's category opens its region");
    }

    #[test]
    fn the_lines_are_made_again_only_when_what_they_show_changes() {
        let mut g = lineup();
        let all = Category::All;
        let mut c = Column::default();
        let v = ColumnView { groups: Some(&g), rev: 1, selected: Some(&all), found: &[], search: None };
        c.refresh(&v);
        let before = c.lines.len();
        g.groups.push(group("News", None, "News", 2));
        // The same rev: what was made stands.
        let v = ColumnView { groups: Some(&g), rev: 1, selected: Some(&all), found: &[], search: None };
        c.refresh(&v);
        assert_eq!(c.lines.len(), before);
        let v = ColumnView { rev: 2, ..v };
        c.refresh(&v);
        assert_eq!(c.lines.len(), before + 1);
    }
}
