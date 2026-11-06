//! A person's page (Android's `TvPersonScreen.kt`, `PersonViewModel.kt`; the TV app's
//! `person.tsx`): the photo as a 160 dp circle at (96, 58), the name and what of theirs is in the
//! libraries beside it, then a row of posters per part they had (Acting, Directing, Creating,
//! Writing, Producing) from y 240, captioned "2019 · as Neo". Focus starts on the first title.

use std::collections::HashSet;

use pley_base::{Icon, ImageRequest, Point, Rect, TextOpts};
use pley_core::model::{CreditRole, PersonDetail, PersonRole};
use pley_core::store::snapshot_keys;
use pley_i18n::t;
use pley_kit::{ColumnState, Row, RowColumn, RowState, Section, TileKind};
use pley_shell::{AppCx, AppEvent, Chrome, Load, Resource, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Id, ImageOpts, Ui};

use crate::common::{self, ArtKind, TileData};
use crate::words;

/// The photo's side (`PhotoSize`).
pub const PHOTO: f32 = 160.0;
/// The header's top.
const HEADER_TOP: f32 = 58.0;
/// The gap between the photo and the name.
const PHOTO_GAP: f32 = 32.0;

/// One row of the page: a part and the titles they had it in.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleRow {
    pub role: CreditRole,
    /// The titles, with their caption ("2019 · as Neo").
    pub credits: Vec<(pley_core::model::ItemSummary, String)>,
}

/// A person's page as data (`personPage`): their parts in order, each title once per part.
#[derive(Clone, Debug, PartialEq)]
pub struct PersonPage {
    pub name: String,
    pub photo: Option<String>,
    /// "3 movies · 2 shows".
    pub counts: String,
    pub rows: Vec<RoleRow>,
}

impl PersonPage {
    /// The page of `p`.
    pub fn of(p: &PersonDetail) -> PersonPage {
        let mut seen = HashSet::new();
        let credits: Vec<&pley_core::model::PersonCredit> =
            p.credits.iter().filter(|c| seen.insert(c.item.id)).collect();
        let rows =
            [CreditRole::Actor, CreditRole::Director, CreditRole::Creator, CreditRole::Writer, CreditRole::Producer]
                .into_iter()
                .filter_map(|role| {
                    let titles: Vec<_> = credits
                        .iter()
                        .filter(|c| c.roles.iter().any(|r| r.role == role))
                        .map(|c| {
                            let roles: Vec<PersonRole> = c.roles.iter().filter(|r| r.role == role).cloned().collect();
                            (c.item.clone(), words::credit_subtitle(&c.item, &roles))
                        })
                        .collect();
                    (!titles.is_empty()).then_some(RoleRow { role, credits: titles })
                })
                .collect();
        PersonPage {
            name: p.name.clone(),
            photo: p.photo_url.clone().filter(|u| !u.is_empty()),
            counts: words::person_counts(&credits),
            rows,
        }
    }
}

/// The page of a person.
pub struct PersonScreen {
    person: Resource<PersonDetail>,
    page: Option<PersonPage>,
    tiles: Vec<Vec<TileData>>,
    photo: Option<ImageRequest>,
    column: ColumnState,
    rows: Vec<RowState>,
    told: bool,
}

impl PersonScreen {
    /// The page of person `id`.
    pub fn new(id: i64) -> PersonScreen {
        PersonScreen {
            person: Resource::new(Some(snapshot_keys::person(id)), move |api| api.person(id)),
            page: None,
            tiles: Vec::new(),
            photo: None,
            column: ColumnState::default(),
            rows: Vec::new(),
            told: false,
        }
    }

    fn rebuild(&mut self, ui: &Ui, cx: &AppCx) {
        let Some(p) = self.person.value() else { return };
        let page = PersonPage::of(p);
        let width = TileKind::Poster.size().w;
        self.tiles = page
            .rows
            .iter()
            .map(|r| {
                r.credits
                    .iter()
                    .map(|(item, sub)| TileData::of(cx, ui, item, ArtKind::Poster, width, sub.clone()))
                    .collect()
            })
            .collect();
        self.rows.resize(page.rows.len(), RowState::default());
        self.photo = page.photo.as_deref().and_then(|u| cx.image(u, ui.image_px(PHOTO)));
        self.page = Some(page);
    }

    fn header(&self, ui: &mut Ui) {
        let photo = Rect::new(theme::GUTTER, HEADER_TOP, PHOTO, PHOTO);
        let r = radius::circle(PHOTO);
        ui.fill(photo, theme::SURFACE2, r);
        let x = photo.right() + PHOTO_GAP;
        let width = theme::CANVAS_W - theme::SAFE_X - x;
        let Some(page) = &self.page else {
            // Still blocks while loading, centred beside the photo.
            let y = photo.center().y - (38.0 + 8.0 + 20.0) / 2.0;
            common::block(ui, x, y, 280.0, 38.0);
            common::block(ui, x, y + 46.0, 200.0, 20.0);
            return;
        };
        // The photo's circle stays Surface2 under it (`TvPersonScreen.kt`, the Samsung app's
        // `PersonScreen.cs`). Initials only for someone without a photo: one still coming shows
        // the circle alone.
        let shown = self.photo.as_ref().map(|req| ui.image_state(req, pley_base::Priority::Visible));
        if self.photo.is_none() {
            let initials = words::initials(&page.name);
            let size = ui.measure(&initials, &theme::PAGE_TITLE, &TextOpts::default());
            let at = Point::new(photo.center().x - size.w / 2.0, photo.center().y - size.h / 2.0);
            ui.text(at, &initials, &theme::PAGE_TITLE, &TextOpts::default(), theme::FG.alpha(0.8));
        }
        if let Some(s) = shown {
            ui.draw_image(photo, &s, &ImageOpts::cover().rounded(r));
        }
        let name_opts = TextOpts::wrap(width, 2).balanced();
        let name_h = ui.measure(&page.name, &theme::PAGE_TITLE, &name_opts).h;
        let counts =
            if page.counts.is_empty() { String::new() } else { t!("item.person.inLibraries", counts = page.counts) };
        let counts_h = if counts.is_empty() { 0.0 } else { 8.0 + theme::BODY.line_height };
        let mut y = photo.center().y - (name_h + counts_h) / 2.0;
        ui.text(Point::new(x, y), &page.name, &theme::PAGE_TITLE, &name_opts, theme::FG);
        y += name_h + 8.0;
        if !counts.is_empty() {
            ui.text(Point::new(x, y), &counts, &theme::BODY, &TextOpts::line(width), theme::MUTED);
        }
    }

    fn rows(&mut self, ui: &mut Ui, cx: &mut AppCx, root: Id) {
        let Some(page) = &self.page else { return };
        let kind = TileKind::Poster;
        let sections: Vec<Section> = (0..page.rows.len())
            .map(|i| Section::new(i as u64, kind.row_height(true), Some(theme::HEADER_Y_PERSON)))
            .collect();
        let column =
            RowColumn::new(theme::HEADER_Y_PERSON).clip_above(Some(theme::HEADER_Y_PERSON - theme::ROWS_CLIP_ABOVE));
        let rows_id = root.with("rows");
        if let Some(first) = self.tiles.first().and_then(|t| t.first()) {
            ui.initial_focus(rows_id.with(0u64).with(first.id as u64));
        }
        let tiles = &self.tiles;
        let states = &mut self.rows;
        let mut open = None;
        let mut actions = None;
        column.show(ui, rows_id, &mut self.column, &sections, |ui, i, top| {
            let row_id = rows_id.with(i as u64);
            let row_tiles = &tiles[i];
            let r = Row::new(kind, top).header(words::role_heading(page.rows[i].role)).show(
                ui,
                row_id,
                &mut states[i],
                row_tiles.len(),
                |t| row_tiles[t].id as u64,
                |ui, slot| row_tiles[slot.index].tile(kind).paint(ui, slot),
            );
            if let Some(t) = r.activated {
                open = row_tiles[t].item.as_ref().map(common::page_of);
            }
            if let Some(t) = r.actions {
                actions = Some(row_tiles[t].id);
            }
        });
        if let Some(route) = open {
            cx.push(route);
        }
        if let Some(id) = actions {
            cx.tile_actions(id, false);
        }
    }
}

impl Screen for PersonScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let changed = self.person.update(cx);
        if changed || (self.page.is_none() && self.person.value().is_some()) {
            self.rebuild(ui, cx);
        }
        let root = ui.scope_id();
        match self.person.state() {
            Load::Failed { error, stale: None } => {
                let missing = error.is(404);
                let (icon, title, body, action) = if missing {
                    (
                        Icon::UserRound,
                        t!("item.person.notFound"),
                        t!("item.person.notFoundBody").to_string(),
                        t!("item.states.back"),
                    )
                } else {
                    (
                        Icon::CircleAlert,
                        common::failed_title(error, t!("item.person.failed")),
                        error.problem(),
                        t!("item.states.retry"),
                    )
                };
                if common::message(ui, root.with("message"), icon, title, &body, action).activated {
                    if missing {
                        cx.back();
                    } else {
                        self.person.invalidate();
                    }
                }
                return;
            }
            Load::Failed { error, stale: Some(_) } if !self.told && !error.unreachable() => {
                self.told = true;
                cx.toast_error(error.problem());
            }
            Load::Ready { .. } => self.told = false,
            _ => {}
        }
        self.header(ui);
        self.rows(ui, cx, root);
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Changed => self.person.invalidate(),
            AppEvent::Shown => self.person.revalidate(cx.now),
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::{ItemKind, ItemSummary, PersonCredit, PersonSummary};

    fn credit(id: i64, kind: ItemKind, roles: Vec<PersonRole>) -> PersonCredit {
        PersonCredit { item: ItemSummary { id, kind, year: Some(1999), ..ItemSummary::default() }, roles }
    }

    fn role(role: CreditRole, character: Option<&str>) -> PersonRole {
        PersonRole { role, character: character.map(String::from), episodes: None }
    }

    #[test]
    fn rows_follow_the_parts_in_androids_order() {
        let p = PersonDetail {
            person: PersonSummary { id: 1, name: "Lana Wachowski".into(), photo_url: None },
            credits: vec![
                credit(10, ItemKind::Movie, vec![role(CreditRole::Writer, None), role(CreditRole::Director, None)]),
                credit(11, ItemKind::Show, vec![role(CreditRole::Creator, None)]),
                credit(10, ItemKind::Movie, vec![role(CreditRole::Actor, Some("Neo"))]),
                credit(12, ItemKind::Movie, vec![role(CreditRole::Actor, Some("Trinity"))]),
            ],
        };
        let page = PersonPage::of(&p);
        let order: Vec<CreditRole> = page.rows.iter().map(|r| r.role).collect();
        assert_eq!(order, [CreditRole::Actor, CreditRole::Director, CreditRole::Creator, CreditRole::Writer]);
        assert_eq!(page.rows[0].credits.len(), 1);
        assert_eq!(page.rows[0].credits[0].1, "1999 · as Trinity");
        assert_eq!(page.rows[1].credits[0].1, "1999 · Director");
        assert_eq!(page.counts, "2 movies · 1 show");
        assert_eq!(page.photo, None);
    }
}
