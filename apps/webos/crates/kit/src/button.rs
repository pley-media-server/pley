//! Buttons (`TvButtons.kt`, `TvLibraryControls.kt`, `TvEpisodes.kt`): the pill,
//! the 40 dp "round" button whose label fades in under it, text buttons and links, chips, tabs and
//! season tabs. All are 40 dp tall with 6 dp corners, take an Accent fill with OnAccent content and
//! a small scale when focused, and AccentBright while pressed; none glows. Disabled ones are drawn
//! at 40 % and can't take focus.

use pley_base::{Color, Icon, Point, Rect, Size};
use pley_ui::theme::{self, focus, radius};
use pley_ui::{Dir, Edge, FocusOpts, Group, Id, Motion, Response, Ui};

use crate::look::{Shade, free_line, inner_stroke, line_within, mix, register, shade, text_mid, unregistered};

/// Which button a [`Button`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ButtonKind {
    /// Play, Resume, Retry: at least 128 wide, padding 20, Fg 12 % with a 1 dp Control edge.
    Pill,
    /// Change server, Cancel, Clear filters: padding 16, Fg 8 %, no edge.
    Text,
    /// A text button with no fill until focused (an album's artist).
    Link,
    /// Filters and choices: Surface3, padding 16; selected ones lead with an Accent check.
    Chip,
    /// A library's tabs: transparent and Muted, the current one Surface3 and Fg; padding 16.
    Tab,
    /// A show's season tabs: as tabs, padding 20 (`TvEpisodes.kt`).
    SeasonTab,
}

/// A button of the [`ButtonKind`]s, built with the methods below and drawn with [`Button::show`].
#[derive(Clone, Copy, Debug)]
pub struct Button<'a> {
    kind: ButtonKind,
    label: &'a str,
    icon: Option<Icon>,
    enabled: bool,
    long_press: bool,
    selected: bool,
    max_label: Option<f32>,
    color: Option<Color>,
    tall: bool,
    min_width: Option<f32>,
}

/// A pill's least width (`PillMinWidth`).
pub const PILL_MIN_WIDTH: f32 = 128.0;
/// A text button's least width: tv-material's `ButtonDefaults.MinWidth`, which the TV app copied.
pub const TEXT_MIN_WIDTH: f32 = 58.0;
/// A tall text button's height (the TV app's `tall`).
pub const TALL_HEIGHT: f32 = 48.0;
/// The gap between an icon and its label.
const ICON_GAP: f32 = 8.0;

impl<'a> Button<'a> {
    fn new(kind: ButtonKind, label: &'a str) -> Button<'a> {
        Button {
            kind,
            label,
            icon: None,
            enabled: true,
            long_press: false,
            selected: false,
            max_label: None,
            color: None,
            tall: false,
            min_width: None,
        }
    }

    /// A pill button.
    pub fn pill(label: &'a str) -> Button<'a> {
        Button::new(ButtonKind::Pill, label)
    }

    /// A text button.
    pub fn text(label: &'a str) -> Button<'a> {
        Button::new(ButtonKind::Text, label)
    }

    /// A text button drawn as a link.
    pub fn link(label: &'a str) -> Button<'a> {
        Button::new(ButtonKind::Link, label)
    }

    /// A chip.
    pub fn chip(label: &'a str) -> Button<'a> {
        Button::new(ButtonKind::Chip, label)
    }

    /// A tab; `current` is the one shown.
    pub fn tab(label: &'a str, current: bool) -> Button<'a> {
        Button { selected: current, ..Button::new(ButtonKind::Tab, label) }
    }

    /// A season tab; `current` is the season shown.
    pub fn season_tab(label: &'a str, current: bool) -> Button<'a> {
        Button { selected: current, ..Button::new(ButtonKind::SeasonTab, label) }
    }

    /// With a 20 dp icon before the label.
    pub fn icon(self, icon: Icon) -> Button<'a> {
        Button { icon: Some(icon), ..self }
    }

    /// Drawn at 40 % and out of focus's way when `false`.
    pub fn enabled(self, enabled: bool) -> Button<'a> {
        Button { enabled, ..self }
    }

    /// OK held 500 ms is a long press (Play and Resume open the title's actions).
    pub fn long_press(self) -> Button<'a> {
        Button { long_press: true, ..self }
    }

    /// A chip that is on (it leads with a check), or the current tab.
    pub fn selected(self, selected: bool) -> Button<'a> {
        Button { selected, ..self }
    }

    /// Cuts the label with "…" at this width (Genre and Year chips: 220; recent searches: 320).
    pub fn max_label_width(self, width: f32) -> Button<'a> {
        Button { max_label: Some(width), ..self }
    }

    /// The label's colour at rest (a link's AccentBright, a destructive DangerText).
    pub fn color(self, color: Color) -> Button<'a> {
        Button { color: Some(color), ..self }
    }

    /// 48 dp tall instead of 40.
    pub fn tall(self) -> Button<'a> {
        Button { tall: true, ..self }
    }

    /// At least this wide.
    pub fn min_width(self, width: f32) -> Button<'a> {
        Button { min_width: Some(width), ..self }
    }

    /// The button's kind.
    pub fn kind(&self) -> ButtonKind {
        self.kind
    }

    fn padding(&self) -> f32 {
        match self.kind {
            ButtonKind::Pill | ButtonKind::SeasonTab => 20.0,
            _ => 16.0,
        }
    }

    fn look(&self) -> focus::ControlLook {
        let mut look = match self.kind {
            ButtonKind::Pill => focus::PILL,
            ButtonKind::Text => focus::TEXT_BUTTON,
            ButtonKind::Link => focus::LINK,
            ButtonKind::Chip => focus::CHIP,
            ButtonKind::Tab | ButtonKind::SeasonTab if self.selected => focus::TAB_CURRENT,
            ButtonKind::Tab | ButtonKind::SeasonTab => focus::TAB,
        };
        if let Some(c) = self.color {
            look.content = c;
        }
        look
    }

    /// The glyph before the label: a selected chip's check, else the icon.
    fn lead(&self) -> Option<Icon> {
        if self.kind == ButtonKind::Chip && self.selected { Some(Icon::Check) } else { self.icon }
    }

    fn height(&self) -> f32 {
        if self.tall { TALL_HEIGHT } else { theme::BUTTON_HEIGHT }
    }

    fn min(&self) -> f32 {
        let own = match self.kind {
            ButtonKind::Pill => PILL_MIN_WIDTH,
            ButtonKind::Text | ButtonKind::Link => TEXT_MIN_WIDTH,
            _ => 0.0,
        };
        own.max(self.min_width.unwrap_or(0.0))
    }

    /// The button's size: its label and icon with the padding either side, at least its least
    /// width, 40 tall.
    pub fn size(&self, ui: &Ui) -> Size {
        let label = ui.measure(self.label, &theme::LABEL, &line_within(self.max_label)).w;
        let icon = if self.lead().is_some() {
            theme::ICON_SIZE + if self.label.is_empty() { 0.0 } else { ICON_GAP }
        } else {
            0.0
        };
        Size::new((self.padding() * 2.0 + icon + label).max(self.min()), self.height())
    }

    /// Draws the button with its top-left corner at `at`, registered as `id`.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point) -> Response {
        let rect = Rect::from_origin_size(at, self.size(ui));
        let look = self.look();
        if !self.enabled {
            ui.opacity(theme::DISABLED_ALPHA, |ui| self.paint(ui, rect, &Shade::rest(&look)));
            return unregistered(id);
        }
        let opts = if self.long_press { FocusOpts::default().long_press() } else { FocusOpts::default() };
        ui.name(id, || self.label.to_string());
        let r = register(ui, id, rect, opts);
        let s = shade(ui, &look, &r);
        self.paint(ui, rect, &s);
        r
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, s: &Shade) {
        let check_color = mix(theme::ACCENT, theme::ON_ACCENT, s.progress);
        ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
            ui.fill(rect, s.fill, radius::CONTROL);
            if let Some(edge) = s.edge {
                inner_stroke(ui, rect, edge, radius::CONTROL, focus::REST_EDGE);
            }
            let layout = ui.layout(self.label, &theme::LABEL, &line_within(self.max_label));
            let lead = self.lead();
            let icon_w = if lead.is_some() { theme::ICON_SIZE } else { 0.0 };
            let gap = if lead.is_some() && !self.label.is_empty() { ICON_GAP } else { 0.0 };
            let content = icon_w + gap + layout.size.w;
            let mut x = rect.x + (rect.w - content) / 2.0;
            if let Some(icon) = lead {
                let tint = if self.kind == ButtonKind::Chip && self.selected { check_color } else { s.content };
                let r = Rect::new(x, rect.y + (rect.h - theme::ICON_SIZE) / 2.0, theme::ICON_SIZE, theme::ICON_SIZE);
                ui.icon(r, icon, tint);
                x += icon_w + gap;
            }
            text_mid(ui, x, rect.y, rect.h, layout, s.content);
        });
    }
}

/// The 40 dp "round" button (a rounded square, radius 6): From start, Mark as watched, More. Its
/// label (`Tv.Caption`, Fg, one line, as wide as it needs) fades in 6 dp below it while it has focus
/// (150 ms, kept with Reduce motion); rows of them keep [`theme::BUTTON_LABEL_ROOM`] under themselves
/// so nothing jumps (`TvButtons.kt` `TvRoundButton`). Without [`RoundButton::show_label`] (a star
/// that says it all) the label is only read by Audio Guidance.
#[derive(Clone, Copy, Debug)]
pub struct RoundButton<'a> {
    icon: Icon,
    label: &'a str,
    on_art: bool,
    enabled: bool,
    long_press: bool,
    show_label: bool,
}

/// A round button's side.
pub const ROUND_SIZE: f32 = 40.0;
/// Its label's top, from the button's top: 40 + 6.
pub const ROUND_LABEL_TOP: f32 = 46.0;

impl<'a> RoundButton<'a> {
    /// A round button over art (Bg at 80 %), the usual place for one.
    pub fn new(icon: Icon, label: &'a str) -> RoundButton<'a> {
        RoundButton { icon, label, on_art: true, enabled: true, long_press: false, show_label: true }
    }

    /// Off art it sits on Surface3 instead.
    pub fn on_art(self, on_art: bool) -> RoundButton<'a> {
        RoundButton { on_art, ..self }
    }

    /// Drawn at 40 % and out of focus's way when `false`.
    pub fn enabled(self, enabled: bool) -> RoundButton<'a> {
        RoundButton { enabled, ..self }
    }

    /// OK held 500 ms is a long press.
    pub fn long_press(self) -> RoundButton<'a> {
        RoundButton { long_press: true, ..self }
    }

    /// With `false`, no label shows under it; Audio Guidance still reads it.
    pub fn show_label(self, show_label: bool) -> RoundButton<'a> {
        RoundButton { show_label, ..self }
    }

    /// The square's size (the label hangs below it).
    pub fn size(&self) -> Size {
        Size::new(ROUND_SIZE, ROUND_SIZE)
    }

    /// Draws the button with its square's top-left corner at `at`.
    pub fn show(&self, ui: &mut Ui, id: Id, at: Point) -> Response {
        let rect = Rect::new(at.x, at.y, ROUND_SIZE, ROUND_SIZE);
        let look = if self.on_art { focus::ROUND_ON_ART } else { focus::ROUND };
        if !self.enabled {
            ui.opacity(theme::DISABLED_ALPHA, |ui| self.paint(ui, rect, &Shade::rest(&look)));
            return unregistered(id);
        }
        let opts = if self.long_press { FocusOpts::default().long_press() } else { FocusOpts::default() };
        ui.name(id, || self.label.to_string());
        let r = register(ui, id, rect, opts);
        let s = shade(ui, &look, &r);
        self.paint(ui, rect, &s);
        if self.show_label {
            let reveal = ui.animate(id.with("kit.label"), if r.focused { 1.0 } else { 0.0 }, Motion::REVEAL);
            if reveal > 0.0 && !self.label.is_empty() {
                let layout = ui.layout(self.label, &theme::CAPTION, &free_line());
                let x = rect.center().x - layout.size.w / 2.0;
                ui.opacity(reveal, |ui| ui.draw_text(Point::new(x, at.y + ROUND_LABEL_TOP), layout, theme::FG));
            }
        }
        r
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, s: &Shade) {
        ui.transform(Point::ZERO, s.scale, rect.center(), |ui| {
            ui.fill(rect, s.fill, radius::CONTROL);
            if let Some(edge) = s.edge {
                inner_stroke(ui, rect, edge, radius::CONTROL, focus::REST_EDGE);
            }
            let icon = rect.place(Size::new(theme::ICON_SIZE, theme::ICON_SIZE), 0.5, 0.5);
            ui.icon(icon, self.icon, s.content);
        });
    }
}

/// One control in a [`button_bar`].
#[derive(Clone, Copy, Debug)]
pub enum BarItem<'a> {
    Button(Button<'a>),
    Round(RoundButton<'a>),
}

impl BarItem<'_> {
    fn width(&self, ui: &Ui) -> f32 {
        match self {
            BarItem::Button(b) => b.size(ui).w,
            BarItem::Round(r) => r.size().w,
        }
    }
}

/// A row of buttons as heroes and dialogs have them: Left past the first leaves the page (the
/// shell sends it to the rail), Right past the last stops.
pub fn bar_group() -> Group {
    Group::row().edge(Dir::Left, Edge::Out).edge(Dir::Right, Edge::Stop)
}

/// Buttons side by side from `at`, [`theme::BUTTON_GAP`] apart, in one [`bar_group`] registered as
/// `id`. Each item comes with a key naming it (so focus stays on "Mark as watched" when it turns
/// into "Mark as unwatched"); the answers come back in order.
pub fn button_bar(ui: &mut Ui, id: Id, at: Point, items: &[(&str, BarItem)]) -> Vec<Response> {
    ui.group(id, bar_group(), |ui| {
        let mut x = at.x;
        let mut out = Vec::with_capacity(items.len());
        for (key, item) in items {
            let item_id = ui.id(key);
            let p = Point::new(x, at.y);
            out.push(match item {
                BarItem::Button(b) => b.show(ui, item_id, p),
                BarItem::Round(r) => r.show(ui, item_id, p),
            });
            x += item.width(ui) + theme::BUTTON_GAP;
        }
        out
    })
}
