//! Appearance (Settings → Appearance, docs/THEMES.md): this TV's theme and accent, laid out as on
//! every TV app. Under the title, "Theme" and its hint on one line, then the nine themes, five to a
//! line, each a card with a small Pley in its colors, its name and caption; then "Accent" and its
//! hint, and the accents, Default (the theme's own) first, each a swatch of the color it gives the
//! theme shown. The theme in force has an accent edge and a check on its corner; the accent in
//! force is filled with the accent, a check in place of its swatch. Focus starts on the pick in
//! force, and Up and Down between the cards and the accents land on it. A pick shows at once,
//! every color in the same frame, and is kept on the TV for the profile watching
//! ([`pley_shell::AppState::pick_theme`]); nothing goes to the server.

use pley_base::{Icon, Point, Rect, Size, TextOpts, TextStyle};
use pley_core::store::ThemePick;
use pley_i18n::t;
use pley_kit::look::{focus_progress, press_progress};
use pley_shell::{AppCx, Chrome, Screen};
use pley_ui::theme::{self, Accent, Palette, Theme, focus, radius};
use pley_ui::{Dir, Edge, Group, Id, Response, Ui};

/// The cards' columns, from the gutter to the safe area's right edge.
const COLUMNS: usize = 5;
const LEFT: f32 = theme::GUTTER;
const WIDTH: f32 = theme::CANVAS_W - theme::SAFE_X - theme::GUTTER;
const GAP: f32 = 16.0;
const CARD_W: f32 = (WIDTH - GAP * (COLUMNS as f32 - 1.0)) / COLUMNS as f32;
/// What a card holds sits this far in from its edge.
const CARD_PAD: f32 = 6.0;
/// A preview is the screen's shape.
const PREVIEW_W: f32 = CARD_W - 2.0 * CARD_PAD;
const PREVIEW_H: f32 = PREVIEW_W * 9.0 / 16.0;
/// The name 8 under the preview, then the caption on up to two lines.
const NAME_GAP: f32 = 8.0;
const CARD_H: f32 = CARD_PAD + PREVIEW_H + NAME_GAP + 18.0 + 2.0 + 2.0 * 17.0 + CARD_PAD;
const ROW_GAP: f32 = 14.0;
/// "Theme" and its hint under the title, then the cards.
const THEME_TITLE_Y: f32 = theme::SAFE_Y + 40.0;
const GRID_Y: f32 = THEME_TITLE_Y + 30.0;
/// "Accent" and its hint under the cards, then the accents.
const ACCENT_TITLE_Y: f32 = GRID_Y + 2.0 * CARD_H + ROW_GAP + 16.0;
const ACCENTS_Y: f32 = ACCENT_TITLE_Y + 30.0;
/// An accent's swatch, and the chip around it.
const SWATCH: f32 = 20.0;
const CHIP_PAD: f32 = 10.0;
const CHIP_GAP: f32 = 10.0;
/// The check on the pick in force.
const CHECK: f32 = 22.0;

// The cards and the accents fit between the title and the foot of the safe area.
const _: () = assert!(GRID_Y + 2.0 * CARD_H + ROW_GAP < ACCENT_TITLE_Y);
const _: () = assert!(ACCENTS_Y + theme::BUTTON_HEIGHT <= theme::CANVAS_H - theme::SAFE_Y);

/// A theme's name, in the language shown (the web's `account.theme.names`).
pub fn theme_name(theme: Theme) -> &'static str {
    match theme {
        Theme::Midnight => t!("settings.appearance.names.midnight"),
        Theme::Midday => t!("settings.appearance.names.midday"),
        Theme::Twilight => t!("settings.appearance.names.twilight"),
        Theme::Starlight => t!("settings.appearance.names.starlight"),
        Theme::Moonlight => t!("settings.appearance.names.moonlight"),
        Theme::Nightshade => t!("settings.appearance.names.nightshade"),
        Theme::Greenlight => t!("settings.appearance.names.greenlight"),
        Theme::Sunshine => t!("settings.appearance.names.sunshine"),
        Theme::Blacklight => t!("settings.appearance.names.blacklight"),
    }
}

/// A theme's caption (the web's `account.theme.about`).
pub fn theme_about(theme: Theme) -> &'static str {
    match theme {
        Theme::Midnight => t!("settings.appearance.about.midnight"),
        Theme::Midday => t!("settings.appearance.about.midday"),
        Theme::Twilight => t!("settings.appearance.about.twilight"),
        Theme::Starlight => t!("settings.appearance.about.starlight"),
        Theme::Moonlight => t!("settings.appearance.about.moonlight"),
        Theme::Nightshade => t!("settings.appearance.about.nightshade"),
        Theme::Greenlight => t!("settings.appearance.about.greenlight"),
        Theme::Sunshine => t!("settings.appearance.about.sunshine"),
        Theme::Blacklight => t!("settings.appearance.about.blacklight"),
    }
}

/// An accent's name; none is the theme's own, "Default" (the web's `account.theme.accent`).
pub fn accent_name(accent: Option<Accent>) -> &'static str {
    match accent {
        None => t!("settings.appearance.accent.own"),
        Some(Accent::Sky) => t!("settings.appearance.accent.names.sky"),
        Some(Accent::Violet) => t!("settings.appearance.accent.names.violet"),
        Some(Accent::Miami) => t!("settings.appearance.accent.names.miami"),
        Some(Accent::Gold) => t!("settings.appearance.accent.names.gold"),
        Some(Accent::Mint) => t!("settings.appearance.accent.names.mint"),
        Some(Accent::Amber) => t!("settings.appearance.accent.names.amber"),
    }
}

/// The profile's theme and accent on this TV.
fn picked(cx: &AppCx) -> (Theme, Option<Accent>) {
    let pick = cx.state.theme_pick();
    (Theme::from_id(Some(&pick.theme)), Accent::from_id(pick.accent.as_deref()))
}

/// The Appearance page.
pub struct AppearanceScreen {
    /// Focus starts on the accents rather than the themes.
    accents: bool,
    opened: bool,
}

impl AppearanceScreen {
    /// The page, focused on the pick in force among the themes, or the accents.
    pub fn new(accents: bool) -> AppearanceScreen {
        AppearanceScreen { accents, opened: false }
    }

    /// The id of theme `theme`'s card, under the page's `root`.
    pub fn theme_id(root: Id, theme: Theme) -> Id {
        root.with("themes").with(theme.id())
    }

    /// The id of an accent's chip (none: Default), under the page's `root`.
    pub fn accent_id(root: Id, accent: Option<Accent>) -> Id {
        root.with("accents").with(accent.map_or("own", Accent::id))
    }
}

impl Screen for AppearanceScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let root = ui.scope_id();
        let (shown, accent) = picked(cx);
        if !self.opened {
            self.opened = true;
            let first = match self.accents {
                true => AppearanceScreen::accent_id(root, accent),
                false => AppearanceScreen::theme_id(root, shown),
            };
            ui.initial_focus(first);
        }
        let width = TextOpts::line(WIDTH);
        ui.text(
            Point::new(LEFT, theme::SAFE_Y),
            t!("settings.categories.appearance"),
            &theme::PAGE_TITLE,
            &width,
            theme::fg(),
        );
        section_line(ui, THEME_TITLE_Y, t!("settings.appearance.theme"), t!("settings.appearance.hint"));

        // Down from the cards and Up from the accents land on the pick in force.
        let (theme_in_force, accent_in_force) =
            (AppearanceScreen::theme_id(root, shown), AppearanceScreen::accent_id(root, accent));
        let mut pick = None;
        let themes = Group::grid(COLUMNS as u16)
            .edge(Dir::Up, Edge::Stop)
            .edge(Dir::Right, Edge::Stop)
            .edge(Dir::Down, Edge::To(accent_in_force));
        ui.group(root.with("themes"), themes, |ui| {
            for (i, t) in Theme::ALL.into_iter().enumerate() {
                let (col, row) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
                let at = Point::new(LEFT + col * (CARD_W + GAP), GRID_Y + row * (CARD_H + ROW_GAP));
                if theme_card(ui, AppearanceScreen::theme_id(root, t), at, t, accent, t == shown).activated {
                    pick = Some((t, accent));
                }
            }
        });

        section_line(ui, ACCENT_TITLE_Y, t!("settings.appearance.accent.title"), t!("settings.appearance.accent.hint"));
        let accents = Group::row()
            .edge(Dir::Down, Edge::Stop)
            .edge(Dir::Right, Edge::Stop)
            .edge(Dir::Up, Edge::To(theme_in_force));
        ui.group(root.with("accents"), accents, |ui| {
            let mut x = LEFT;
            for a in std::iter::once(None).chain(Accent::ALL.map(Some)) {
                let id = AppearanceScreen::accent_id(root, a);
                let (r, w) = accent_chip(ui, id, Point::new(x, ACCENTS_Y), shown, a, a == accent);
                if r.activated {
                    pick = Some((shown, a));
                }
                x += w + CHIP_GAP;
            }
        });

        if let Some((t, a)) = pick
            && (t, a) != (shown, accent)
        {
            // Shown from the next frame on, which the shell starts in the new palette.
            cx.state.pick_theme(ThemePick { theme: t.id().to_string(), accent: a.map(|a| a.id().to_string()) });
            ui.request_repaint();
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn title(&self) -> Option<String> {
        Some(t!("settings.categories.appearance").into())
    }
}

/// A section's line: its label in `Tv.Title`, then its hint on the same line in Muted.
fn section_line(ui: &mut Ui, y: f32, label: &str, hint: &str) {
    let title = ui.text(Point::new(LEFT, y), label, &theme::TITLE, &TextOpts::line(WIDTH), theme::fg());
    let hint_x = LEFT + title.w + 12.0;
    let hint_y = y + (theme::TITLE.line_height - theme::BODY.line_height) / 2.0;
    ui.text(Point::new(hint_x, hint_y), hint, &theme::BODY, &TextOpts::line(LEFT + WIDTH - hint_x), theme::muted());
}

/// A theme's card at `at`: its edge, the preview, its name and caption under it. The theme in
/// force has an accent edge over a tint of the accent and the check on its preview's corner. The
/// card grows as focus comes, with the ring around it in place of its edge, so one outline shows,
/// as on a chip.
fn theme_card(ui: &mut Ui, id: Id, at: Point, t: Theme, accent: Option<Accent>, current: bool) -> Response {
    let card = Rect::new(at.x, at.y, CARD_W, CARD_H);
    let r = ui.focusable(id, card);
    let (name, about) = (theme_name(t), theme_about(t));
    ui.name(id, || pley_kit::spoken(&[name, about]));
    let p = focus_progress(ui, id, r.focused);
    let q = press_progress(ui, id, r.pressed);
    let scale = 1.0 + (focus::chip().focus_scale - 1.0) * p * (1.0 - q);
    let edge = 1.0 - p;
    ui.transform(Point::ZERO, scale, card.center(), |ui| {
        if current {
            ui.fill(card, theme::accent().alpha(0.06), radius::CARD);
            if edge > 0.0 {
                ui.stroke(card.expand(-1.0), theme::accent().alpha(edge), radius::CARD - 1.0, 2.0);
            }
        } else if edge > 0.0 {
            ui.stroke(card.expand(-0.5), theme::surface4().alpha(edge), radius::CARD, 1.0);
        }
        let preview = Rect::new(card.x + CARD_PAD, card.y + CARD_PAD, PREVIEW_W, PREVIEW_H);
        draw_preview(ui, preview, Palette::of(t, accent));
        if current {
            check(ui, Point::new(preview.right() - CHECK / 2.0 - 4.0, preview.y + CHECK / 2.0 + 4.0));
        }
        let (x, y, w) = (preview.x, preview.bottom() + NAME_GAP, PREVIEW_W);
        let style = if r.focused { theme::TITLE } else { theme::LABEL };
        let named = TextStyle { size: theme::LABEL.size, line_height: 18.0, ..style };
        ui.text(Point::new(x, y), name, &named, &TextOpts::line(w), theme::fg());
        ui.text(Point::new(x, y + 20.0), about, &theme::CAPTION, &TextOpts::wrap(w, 2), theme::muted());
    });
    ring(ui, card, scale, radius::CARD, p);
    r
}

/// The focus ring around `rect` grown to `scale`, at `progress`.
fn ring(ui: &mut Ui, rect: Rect, scale: f32, corner: f32, progress: f32) {
    if progress > 0.0 {
        let grown = rect.scale_about(rect.center(), scale).expand(focus::RING_OFFSET);
        let color = focus::ring_color().alpha(progress);
        ui.stroke(grown, color, corner * scale + focus::RING_OFFSET, focus::RING_STROKE);
    }
}

/// A small Pley in `p`'s colors, as the web's preview draws it: the sidebar with the current
/// entry in the accent, a heading with a button, a line of words and a row of posters, the first
/// one focused.
fn draw_preview(ui: &mut Ui, rect: Rect, p: &Palette) {
    ui.fill(rect, p.bg, radius::CARD);
    ui.clip(rect, |ui| {
        let unit = rect.w / 112.0;
        let pad = 8.0 * unit;
        let bar = 6.0 * unit;
        // The sidebar.
        let side = Rect::new(rect.x, rect.y, rect.w / 4.0, rect.h);
        ui.fill(side, p.surface, 0.0);
        ui.fill(Rect::new(side.right() - unit.max(1.0), rect.y, unit.max(1.0), rect.h), p.border, 0.0);
        let inner = side.w - 2.0 * pad;
        let mut y = rect.y + pad;
        for (w, color) in
            [(0.75, p.accent), (1.0, p.fg.alpha(0.15)), (0.67, p.fg.alpha(0.15)), (0.83, p.fg.alpha(0.15))]
        {
            ui.fill(Rect::new(side.x + pad, y, inner * w, bar), color, bar / 2.0);
            y += bar + bar;
        }
        // The page: a heading and its button, a line, then posters along the bottom.
        let page =
            Rect::new(side.right() + pad, rect.y + pad, rect.right() - side.right() - 2.0 * pad, rect.h - 2.0 * pad);
        let head = 8.0 * unit;
        ui.fill(Rect::new(page.x, page.y + unit, page.w * 0.5, head - 2.0 * unit), p.fg.alpha(0.8), head);
        ui.fill(Rect::new(page.right() - page.w * 0.25, page.y, page.w * 0.25, head + 2.0 * unit), p.accent, head);
        ui.fill(Rect::new(page.x, page.y + head + 2.0 * bar, page.w / 3.0, bar), p.muted.alpha(0.5), bar / 2.0);
        let gap = 4.0 * unit;
        let w = (page.w - 3.0 * gap) / 4.0;
        let h = (w * 1.5).min(page.h - head - 4.0 * bar);
        for i in 0..4 {
            let poster = Rect::new(page.x + i as f32 * (w + gap), page.bottom() - h, w, h);
            ui.fill(poster, p.surface3, 2.0 * unit);
            if i == 0 {
                ui.stroke(poster, p.accent, 2.0 * unit, unit.max(1.0));
            }
        }
    });
    ui.stroke(rect, theme::fg().alpha(0.1), radius::CARD, 1.0);
}

/// The check on the pick in force, centered on `center`: OnAccent on the accent, edged in the
/// page's color so it stands off whatever is under it.
fn check(ui: &mut Ui, center: Point) {
    let disk = Rect::new(center.x - CHECK / 2.0, center.y - CHECK / 2.0, CHECK, CHECK);
    ui.fill(disk.expand(2.0), theme::surface(), CHECK / 2.0 + 2.0);
    ui.fill(disk, theme::accent(), CHECK / 2.0);
    ui.icon(disk.place(Size::new(14.0, 14.0), 0.5, 0.5), Icon::Check, theme::on_accent());
}

/// An accent's chip at `at`: its swatch, the color it gives `shown`, then its name, in a 1 dp
/// edge. The accent in force is filled with the accent, its words in AccentFg and a check in
/// place of its swatch. The chip grows as focus comes, with the ring around it in place of its
/// edge. Its response and width.
fn accent_chip(ui: &mut Ui, id: Id, at: Point, shown: Theme, accent: Option<Accent>, current: bool) -> (Response, f32) {
    let name = accent_name(accent);
    let layout = ui.layout(name, &theme::LABEL, &TextOpts::default());
    let w = CHIP_PAD + SWATCH + 8.0 + layout.size.w + CHIP_PAD + 4.0;
    let rect = Rect::new(at.x, at.y, w, theme::BUTTON_HEIGHT);
    let r = ui.focusable(id, rect);
    ui.name(id, || name.to_string());
    let p = focus_progress(ui, id, r.focused);
    let q = press_progress(ui, id, r.pressed);
    let scale = 1.0 + (focus::chip().focus_scale - 1.0) * p * (1.0 - q);
    ui.transform(Point::ZERO, scale, rect.center(), |ui| {
        let swatch = Rect::new(rect.x + CHIP_PAD, rect.y + (rect.h - SWATCH) / 2.0, SWATCH, SWATCH);
        let content = if current {
            ui.fill(rect, theme::accent(), radius::CONTROL);
            let on = theme::on_accent();
            ui.stroke(swatch.expand(-0.75), on.alpha(0.6), SWATCH / 2.0, 1.5);
            ui.icon(swatch.place(Size::new(12.0, 12.0), 0.5, 0.5), Icon::Check, on);
            on
        } else {
            if p < 1.0 {
                ui.stroke(rect.expand(-0.5), theme::surface4().alpha(1.0 - p), radius::CONTROL, 1.0);
            }
            ui.fill(swatch.expand(1.0), theme::fg().alpha(0.2), SWATCH / 2.0 + 1.0);
            ui.fill(swatch, Palette::of(shown, accent).accent, SWATCH / 2.0);
            theme::fg()
        };
        let text_at = Point::new(swatch.right() + 8.0, rect.y + (rect.h - layout.size.h) / 2.0);
        ui.draw_text(text_at, layout, content);
    });
    ring(ui, rect, scale, radius::CONTROL, p);
    (r, w)
}
