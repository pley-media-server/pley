//! The globe of the screens before signing in (the web app's LanguagePicker, the Samsung apps'
//! `LanguageGlobe`): a small plain globe and the language shown by its two letters ("FR"), with
//! no fill until it has focus, "FR" at the screen's right edge. At the bottom right of Choose your
//! server and the address entry, centred on the forget hint's line; on the sign-in code at the top
//! right, centred on the logo's line. Down or Up reaches it, Left and Right never do.
//!
//! OK opens the languages in a side panel, as Settings shows its choices: Automatic, with what
//! that is on this TV (its own language where Pley has it, else the server's, else English), then
//! English, Français and Español, each named in itself, the choice in force checked. A choice
//! shows at once and is kept on the TV; Automatic forgets it.

use pley_base::{Icon, Point, TextOpts};
use pley_i18n::{Language, t};
use pley_kit::{Button, List, ListRow, ListState, SidePanel, button::SMALL_PADDING, is_inert};
use pley_shell::{AppCx, AppState};
use pley_ui::theme;
use pley_ui::{Dir, Edge, Group, Id, Ui};

/// From the screen's right edge to what the globe shows ("FR").
const RIGHT: f32 = 48.0;
/// From the screen's bottom edge to the globe at the bottom right, which centres it on the forget
/// hint's line (`Caption`, 27 from the bottom).
const BOTTOM: f32 = 20.0;
/// The gap between the panel's rows, as in Settings.
const ROW_GAP: f32 = 2.0;

/// Where the globe goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// The bottom right: Choose your server, the address entry.
    BottomRight,
    /// The top right, centred on `y` (dp): the logo's line on the sign-in code.
    TopRight { centre: f32 },
}

/// A language the globe's panel lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// What it says: "Automatic (English)", "Français".
    pub label: String,
    /// The choice in force.
    pub checked: bool,
    /// None: Automatic.
    pub language: Option<Language>,
}

/// Automatic, with what that is on this TV, then each language named in itself.
pub fn choices(state: &AppState) -> Vec<Choice> {
    let picked = state.language_choice();
    let automatic = t!("settings.language.automatic", language = state.automatic_language().name());
    std::iter::once(Choice { label: automatic, checked: picked.is_none(), language: None })
        .chain(Language::ALL.into_iter().map(|l| Choice {
            label: l.name().to_string(),
            checked: picked == Some(l),
            language: Some(l),
        }))
        .collect()
}

/// The globe and its panel, as a screen holds them.
#[derive(Debug, Default)]
pub struct Globe {
    open: bool,
    list: ListState,
}

impl Globe {
    /// The globe's focus id under the screen's root.
    pub fn id() -> Id {
        Id::ROOT.with("globe").with("button")
    }

    /// Whether its panel is open: the screen keeps Back and focus away from itself meanwhile.
    pub fn open(&self) -> bool {
        self.open
    }

    /// Closes the panel (Back).
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Draws the globe at `place`, and its panel while open; a choice shows at once.
    pub fn show(&mut self, ui: &mut Ui, cx: &mut AppCx, place: Place) {
        let letters = cx.state.language().letters();
        let button = Button::small(letters).icon(Icon::Globe);
        let size = button.size(ui);
        let x = theme::CANVAS_W - RIGHT + SMALL_PADDING - size.w;
        let y = match place {
            Place::BottomRight => theme::CANVAS_H - BOTTOM - size.h,
            Place::TopRight { centre } => centre - size.h / 2.0,
        };
        let group = Group::row().edge(Dir::Left, Edge::Stop).edge(Dir::Right, Edge::Stop);
        let name = format!("{}, {}", t!("signin.language.choose"), cx.state.language().name());
        let r = ui.group(ui.id("globe"), group, |ui| {
            let id = ui.id("button");
            // Heard by what OK does and the language's name, not its two letters (the first name
            // given stands).
            ui.name(id, || name);
            button.show(ui, id, Point::new(x, y))
        });
        if r.activated && !self.open {
            self.open = true;
            self.list = ListState::at(choices(cx.state).iter().position(|c| c.checked).unwrap_or(0));
            ui.request_repaint();
        }
        self.panel(ui, cx);
    }

    fn panel(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let choices = choices(cx.state);
        let list = &mut self.list;
        let mut chosen = None;
        let root = Id::ROOT.with("globe").with("panel");
        let r = SidePanel::new().show(ui, root, self.open, |ui, area| {
            let title = t!("signin.language.choose");
            let h = ui.text(
                Point::new(area.x + 16.0, area.y + 8.0),
                title,
                &theme::TITLE,
                &TextOpts::wrap(area.w - 16.0, 2),
                theme::FG,
            );
            let top = area.y + 8.0 + h.h + 12.0;
            let view = pley_base::Rect::new(area.x, top, area.w, area.bottom() - top);
            let rows: Vec<ListRow> = choices.iter().map(|c| ListRow::new(&c.label).checked(c.checked)).collect();
            let heights: Vec<f32> = rows.iter().map(|r| r.height(ui, area.w)).collect();
            let id = root.with("choices");
            if !is_inert() {
                ui.initial_focus(id.with(list.index as u64));
            }
            List::new(view).gap(ROW_GAP).edges(Edge::Stop, Edge::Stop).show(
                ui,
                id,
                list,
                rows.len(),
                |_, i| heights[i],
                |ui, s| {
                    if rows[s.index].show(ui, s.id, s.rect.origin(), s.rect.w).activated {
                        chosen = Some(s.index);
                    }
                },
            );
        });
        if r.closed {
            self.open = false;
        }
        if let Some(i) = chosen
            && self.open
        {
            self.open = false;
            cx.state.choose_language_here(choices[i].language);
            ui.request_repaint();
        }
    }
}
