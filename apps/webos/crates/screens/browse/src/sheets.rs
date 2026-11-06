//! New playlist, Save these filters and Add to playlist (`TvPlaylistSheets.kt`; the TV app's
//! `NewPlaylistScreen` and `AddToPlaylistScreen`): what the page is for at the gutter (a list-plus
//! glyph, then the words, x 96 from y 58, 464 wide) and a side panel to name the playlist (OK on
//! the title types it with the on-screen keyboard) and, for Add to playlist, the playlists that
//! take the titles. Back closes the page.

use pley_base::{Align, Color, Icon, Point, Rect, TextOpts};
use pley_core::http::sentence;
use pley_core::model::{Added, ItemDetail, ItemKind, NewPlaylist, PlaylistDetail, PlaylistKind, PlaylistList};
use pley_core::store::snapshot_keys;
use pley_core::{ApiError, Promise};
use pley_i18n::t;
use pley_kit::{
    Backdrop, BackdropArt, BackdropScrim, BackdropState, List, ListRow, ListState, SidePanel, TextField, backdrop_px,
    inert, skeleton_block,
};
use pley_shell::{AppCx, AppEvent, Chrome, Load, Resource, Route, Screen, SmartPlaylist};
use pley_ui::Ui;
use pley_ui::theme::{self, radius};

use crate::parts::{TITLE_MAX, Typing, answer};
use crate::playlists::{Card, cover};
use crate::tiles::{ArtKind, art_paths, requests};
use crate::view::describe_smart_filter;

/// The text column beside the panel: x 96 to 560, from y 58.
const LEFT_W: f32 = theme::TEXT_COLUMN_END - theme::GUTTER;
const LEFT_Y: f32 = 58.0;
/// How long "Added to …" shows before the page closes (`DONE_MS`).
const DONE_FOR: f64 = 1.4;
/// A playlist row's cover (`TvPlaylistArt` at 40 dp, its glyph 18), 16 in from the row's edge as
/// a panel row's leading picture is.
const ROW_ART: f32 = 40.0;
const ROW_ART_GLYPH: f32 = 18.0;
const ROW_PAD: f32 = 16.0;
/// The name field's line: the field and 6 above and below it.
const FIELD_H: f32 = 6.0 + 44.0 + 6.0;

/// What a playlist of items of `kind` plays: music for artists, albums and tracks, else video
/// (`Playlists.kindOf`).
pub fn kind_of(kind: ItemKind) -> PlaylistKind {
    match kind {
        ItemKind::Artist | ItemKind::Album | ItemKind::Track => PlaylistKind::Audio,
        _ => PlaylistKind::Video,
    }
}

/// What the panel says once titles are added (`addedMessage`): "Nothing to add", "Added to Road
/// trip", "Added 12 items to Road trip".
pub fn added_message(added: i64, playlist: &str) -> String {
    match added {
        0 => t!("playlists.add.nothing").to_string(),
        1 => t!("playlists.add.addedTo", playlist = playlist),
        n => t!("playlists.add.addedItems", count = n, playlist = playlist),
    }
}

/// Whether `title`, trimmed, will do as a playlist's title: 1 to 200 characters.
fn fits(title: &str) -> bool {
    let t = title.trim();
    !t.is_empty() && t.chars().count() <= TITLE_MAX
}

/// The words at the gutter: a 32 dp list-plus glyph in Muted, 16, then `draw`'s.
fn behind(ui: &mut Ui, draw: impl FnOnce(&mut Ui, f32)) {
    ui.icon(Rect::new(theme::GUTTER, LEFT_Y, 32.0, 32.0), Icon::ListPlus, theme::MUTED);
    draw(ui, LEFT_Y + 32.0 + 16.0);
}

/// A side panel's title in `Tv.Title` with what it is for under it (`TvPanelTitle`: 16 in, 8
/// above, 12 below); returns the height used.
fn panel_title(ui: &mut Ui, area: Rect, y: f32, title: &str, subtitle: &str) -> f32 {
    let w = area.w - 16.0;
    let x = area.x + 16.0;
    let mut h = 8.0;
    h += ui.text(Point::new(x, y + h), title, &theme::TITLE, &TextOpts::wrap(w, 2), theme::FG).h;
    if !subtitle.is_empty() {
        h += ui.text(Point::new(x, y + h), subtitle, &theme::CAPTION, &TextOpts::line(w), theme::MUTED).h;
    }
    h + 12.0
}

/// A heading inside a panel ("For", "Your playlists": `TvPanelHeading`, `Tv.Label` Muted, 16 in,
/// 16 above, 6 below); returns the height used.
fn panel_heading(ui: &mut Ui, area: Rect, y: f32, text: &str) -> f32 {
    let h = ui.text(Point::new(area.x + 16.0, y + 16.0), text, &theme::LABEL, &TextOpts::line(area.w), theme::MUTED).h;
    16.0 + h + 6.0
}

/// New playlist, or Save these filters: the title and, for an empty playlist, what it plays;
/// Create (or Save) makes it and opens it in this page's place.
pub struct NewPlaylistScreen {
    item_ids: Vec<i64>,
    smart: Option<SmartPlaylist>,
    kind: PlaylistKind,
    typing: Typing,
    making: Option<Promise<Result<PlaylistDetail, ApiError>>>,
    filters: Vec<String>,
}

impl NewPlaylistScreen {
    /// A new playlist of `item_ids` (none: an empty one, whose kind is asked), or with `smart`,
    /// a smart playlist of a library's listing.
    pub fn new(item_ids: Vec<i64>, smart: Option<SmartPlaylist>) -> NewPlaylistScreen {
        let filters = smart.as_ref().map(|s| describe_smart_filter(&s.filter)).unwrap_or_default();
        NewPlaylistScreen { item_ids, smart, kind: PlaylistKind::Video, typing: Typing::new(""), making: None, filters }
    }

    /// Whether the panel asks what the playlist plays: only an empty one's isn't known.
    fn asks_kind(&self) -> bool {
        self.smart.is_none() && self.item_ids.is_empty()
    }

    fn heading(&self) -> &'static str {
        if self.smart.is_some() { t!("playlists.form.smartTitle") } else { t!("playlists.form.newTitle") }
    }

    fn body(&self) -> String {
        match &self.smart {
            None => t!("playlists.form.newBody").to_string(),
            Some(_) if self.filters.is_empty() => t!("playlists.form.smartBody").to_string(),
            Some(_) => t!("playlists.form.smartBodyFilters", filters = self.filters.join(" · ")),
        }
    }

    fn create(&mut self, cx: &mut AppCx) {
        let Some(title) = self.typing.title() else { return };
        if self.making.is_some() {
            return;
        }
        let body = match &self.smart {
            Some(s) => NewPlaylist {
                title,
                smart_filter: Some(s.filter.clone()),
                library_id: Some(s.library_id),
                ..NewPlaylist::default()
            },
            None => NewPlaylist {
                title,
                kind: self.asks_kind().then_some(self.kind),
                item_ids: self.item_ids.clone(),
                ..NewPlaylist::default()
            },
        };
        self.typing.typing = false;
        self.making = Some(cx.spawn(move |api| api.create_playlist(&body)));
    }
}

impl Screen for NewPlaylistScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        if let Some(p) = &mut self.making
            && let Some(answer) = answer(p)
        {
            self.making = None;
            match answer {
                Ok(made) => cx.replace(Route::Playlist { id: made.id }),
                Err(e) if e.cancelled() => {}
                Err(e) => {
                    cx.state.report_failure(&e);
                    cx.toast_error(sentence(&e.to_string()));
                }
            }
        }
        let heading = self.heading();
        let body = self.body();
        behind(ui, |ui, y| {
            let x = theme::GUTTER;
            let h = ui.text(Point::new(x, y), heading, &theme::PAGE_TITLE, &TextOpts::wrap(LEFT_W, 2), theme::FG).h;
            let opts = TextOpts::wrap(LEFT_W, 0);
            ui.text(Point::new(x, y + h + 12.0), &body, &theme::BODY_LARGE, &opts, theme::MUTED);
        });
        let busy = self.making.is_some();
        let ok = fits(&self.typing.edit.text) && !busy;
        let asks = self.asks_kind();
        let smart = self.smart.is_some();
        let (field_id, video_id, music_id, create_id) =
            (ui.id("title"), ui.id("for-video"), ui.id("for-music"), ui.id("create"));
        let mut create = false;
        let mut kind = None;
        let typing = &mut self.typing;
        let current = self.kind;
        SidePanel::new().back_closes(false).show(ui, ui.id("panel"), true, |ui, area| {
            let mut y = area.y;
            y += panel_title(ui, area, y, heading, "");
            ui.initial_focus(field_id);
            typing.field(
                ui,
                field_id,
                Point::new(area.x + 4.0, y + 6.0),
                area.w - 8.0,
                t!("playlists.form.titleField"),
            );
            y += 6.0 + 44.0 + 6.0;
            let hint = t!("playlists.form.hint");
            let opts = TextOpts::wrap(area.w - 8.0, 2);
            y += 4.0 + ui.text(Point::new(area.x + 8.0, y + 4.0), hint, &theme::CAPTION, &opts, theme::MUTED).h;
            if asks {
                y += panel_heading(ui, area, y, t!("playlists.form.for"));
                let video = ListRow::new(t!("playlists.form.video"))
                    .icon(Icon::ListVideo)
                    .checked(current == PlaylistKind::Video);
                if video.show(ui, video_id, Point::new(area.x, y), area.w).activated {
                    kind = Some(PlaylistKind::Video);
                }
                y += video.height(ui, area.w);
                let music = ListRow::new(t!("playlists.form.music"))
                    .icon(Icon::ListMusic)
                    .checked(current == PlaylistKind::Audio);
                if music.show(ui, music_id, Point::new(area.x, y), area.w).activated {
                    kind = Some(PlaylistKind::Audio);
                }
                y += music.height(ui, area.w);
            }
            y += 16.0;
            let (label, icon) =
                if smart { (t!("playlists.form.save"), Icon::Save) } else { (t!("playlists.form.create"), Icon::Plus) };
            create =
                ListRow::new(label).icon(icon).enabled(ok).show(ui, create_id, Point::new(area.x, y), area.w).activated;
        });
        if let Some(k) = kind {
            self.kind = k;
            ui.request_repaint();
        }
        if self.typing.keyboard(ui, ui.id("keyboard"), t!("playlists.form.done")) && ok {
            create = true;
        }
        if create {
            self.create(cx);
            ui.request_repaint();
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        self.typing.back()
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn modal(&self) -> bool {
        true
    }
}

/// Add to playlist: a name for a new playlist, then the playlists of the same kind. It closes
/// itself 1.4 s after adding.
pub struct AddToPlaylistScreen {
    item_ids: Vec<i64>,
    title: String,
    /// What the titles are, from the first one: what playlists take them, the backdrop.
    kind: Option<PlaylistKind>,
    lead: Option<Promise<Result<ItemDetail, ApiError>>>,
    art: Option<BackdropArt>,
    backdrop: BackdropState,
    list: Resource<PlaylistList>,
    cards: Vec<Card>,
    typing: Typing,
    adding: Option<(String, Promise<Result<Added, ApiError>>)>,
    making: Option<Promise<Result<PlaylistDetail, ApiError>>>,
    /// What was added, and when.
    done: Option<(String, f64)>,
    /// Whether focus has been placed (on the first playlist, else the field).
    placed: bool,
    /// The panel's list: its title, the field, Create, "Your playlists", then the playlists.
    lines: ListState,
}

impl AddToPlaylistScreen {
    /// Adding `item_ids`, named `title` at the gutter.
    pub fn new(item_ids: Vec<i64>, title: String) -> AddToPlaylistScreen {
        AddToPlaylistScreen {
            item_ids,
            title,
            kind: None,
            lead: None,
            art: None,
            backdrop: BackdropState::default(),
            list: Resource::new(Some(snapshot_keys::PLAYLISTS.to_string()), |api| api.playlists()),
            cards: Vec::new(),
            typing: Typing::new(""),
            adding: None,
            making: None,
            done: None,
            placed: false,
            lines: ListState::default(),
        }
    }

    fn busy(&self) -> bool {
        self.adding.is_some() || self.making.is_some() || self.done.is_some()
    }

    /// The first title's details tell the kind and the backdrop: the ones kept, else the server's.
    fn learn(&mut self, ui: &Ui, cx: &mut AppCx) {
        if self.kind.is_some() {
            return;
        }
        let Some(&first) = self.item_ids.first() else {
            self.kind = Some(PlaylistKind::Video);
            return;
        };
        let detail = match cx.state.cached_item(first) {
            Some(d) => Some(Ok(d.clone())),
            None => {
                let p = self.lead.get_or_insert_with(|| cx.spawn(move |api| api.item(first)));
                answer(p)
            }
        };
        match detail {
            Some(Ok(d)) => {
                self.kind = Some(kind_of(d.kind));
                let art = cx.api().map(|api| requests(api, &art_paths(&d, ArtKind::Backdrop), backdrop_px(ui)));
                self.art = Some(BackdropArt::new(d.id as u64, art.unwrap_or_default(), d.title.clone()));
                self.lead = None;
            }
            Some(Err(_)) => {
                self.kind = Some(PlaylistKind::Video);
                self.lead = None;
            }
            None => {}
        }
    }

    fn collect(&mut self, cx: &mut AppCx) {
        if let Some((name, p)) = &mut self.adding
            && let Some(answer) = answer(p)
        {
            let name = std::mem::take(name);
            self.adding = None;
            match answer {
                Ok(added) => self.done = Some((added_message(added.added, &name), cx.now)),
                Err(e) => fail(cx, &e),
            }
        }
        if let Some(p) = &mut self.making
            && let Some(answer) = answer(p)
        {
            self.making = None;
            match answer {
                Ok(made) => self.done = Some((added_message(made.item_count.max(1), &made.title), cx.now)),
                Err(e) => fail(cx, &e),
            }
        }
    }

    fn create(&mut self, cx: &mut AppCx) {
        let (Some(title), Some(kind)) = (self.typing.title(), self.kind) else { return };
        if self.busy() {
            return;
        }
        let body = NewPlaylist { title, kind: Some(kind), item_ids: self.item_ids.clone(), ..NewPlaylist::default() };
        self.typing.typing = false;
        self.making = Some(cx.spawn(move |api| api.create_playlist(&body)));
    }
}

/// A refusal's words as an error toast.
fn fail(cx: &mut AppCx, e: &ApiError) {
    if !e.cancelled() {
        cx.state.report_failure(e);
        cx.toast_error(sentence(&e.to_string()));
    }
}

impl Screen for AddToPlaylistScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        self.learn(ui, cx);
        self.collect(cx);
        if self.list.update(cx) {
            let (px, quarter) = (ui.image_px(ROW_ART), ui.image_px(ROW_ART / 2.0));
            let playlists = self.list.value().map(|l| l.playlists.as_slice()).unwrap_or_default();
            self.cards = playlists.iter().map(|p| Card::new(cx, p, px, quarter)).collect();
        }
        if let Some((_, at)) = &self.done {
            let left = DONE_FOR - (cx.now - at);
            if left <= 0.0 {
                cx.pop();
                return;
            }
            ui.repaint_after(left);
        }
        Backdrop::new(BackdropScrim::Item).show(ui, ui.id("backdrop"), &mut self.backdrop, self.art.as_ref());
        let title = self.title.clone();
        behind(ui, |ui, y| {
            let x = theme::GUTTER;
            let h = ui
                .text(Point::new(x, y), t!("playlists.add.title"), &theme::LABEL, &TextOpts::line(LEFT_W), theme::MUTED)
                .h;
            ui.text(Point::new(x, y + h + 6.0), &title, &theme::PAGE_TITLE, &TextOpts::wrap(LEFT_W, 3), theme::FG);
        });

        let music = self.kind == Some(PlaylistKind::Audio);
        let kind = self.kind;
        let busy = self.busy();
        let list: Vec<&Card> = self
            .cards
            .iter()
            .filter(|c| !c.smart && kind.is_none_or(|k| c.video == (k != PlaylistKind::Audio)))
            .collect();
        let loading = matches!(self.list.state(), Load::Loading);
        let failed = match self.list.state() {
            Load::Failed { error, stale: None } => Some(sentence(&error.problem())),
            _ => None,
        };
        let known = self.list.value().is_some();
        // The panel is one list that scrolls with focus (`TvPlaylistSheets.kt`): its title, the
        // field, Create, "Your playlists", then what there is to say or the playlists.
        let mut lines = vec![AddLine::Title, AddLine::Field, AddLine::Create, AddLine::Heading];
        let none = if music { t!("playlists.add.noneMusic") } else { t!("playlists.add.noneVideo") };
        if loading {
            lines.push(AddLine::Loading);
        } else if let Some(problem) = &failed {
            lines.push(AddLine::Note(problem, theme::DANGER_TEXT));
        } else if known && list.is_empty() && kind.is_some() {
            lines.push(AddLine::Note(none, theme::MUTED));
        } else {
            lines.extend(list.iter().map(|c| AddLine::Playlist(c)));
        }
        let lines_id = ui.id("lines");
        let field_id = lines_id.with(1u64);
        // Focus goes to the first playlist, else the field, once both are known.
        if !self.placed && kind.is_some() && !loading {
            self.placed = true;
            let first = lines.iter().position(|l| matches!(l, AddLine::Playlist(_)));
            ui.request_focus(first.map_or(field_id, |i| lines_id.with(i as u64)));
        }
        let done = self.done.as_ref().map(|(m, _)| m.clone());
        let typing = &mut self.typing;
        let state = &mut self.lines;
        let mut add = None;
        let mut create = false;
        let subtitle = self.title.as_str();
        SidePanel::new().back_closes(false).show(ui, ui.id("panel"), true, |ui, area| {
            if let Some(message) = &done {
                let opts = TextOpts::wrap(area.w, 3).aligned(Align::Center);
                let text_h = ui.measure(message, &theme::TITLE, &opts).h;
                let top = area.y + (area.h - (48.0 + 16.0 + text_h)) / 2.0;
                let icon = Rect::new(area.x + (area.w - 48.0) / 2.0, top, 48.0, 48.0);
                ui.icon(icon, Icon::CircleCheck, theme::ACCENT);
                ui.text(Point::new(area.x, top + 64.0), message, &theme::TITLE, &opts, theme::FG);
                return;
            }
            let placeholder = if music { t!("playlists.add.newMusic") } else { t!("playlists.add.newVideo") };
            let ok = kind.is_some() && !busy && fits(&typing.edit.text);
            let w = area.w;
            let height = |ui: &Ui, i: usize| lines[i].height(ui, w, subtitle, !busy);
            List::new(area).gap(0.0).show(ui, lines_id, state, lines.len(), height, |ui, slot| {
                let (rect, at) = (slot.rect, slot.rect.origin());
                match lines[slot.index] {
                    AddLine::Title => {
                        panel_title(ui, rect, rect.y, t!("playlists.add.title"), subtitle);
                    }
                    AddLine::Field => {
                        let field_at = Point::new(rect.x + 4.0, rect.y + 6.0);
                        if kind.is_some() && !busy {
                            typing.field(ui, slot.id, field_at, rect.w - 8.0, placeholder);
                        } else {
                            let field = TextField::new(&typing.edit.text, rect.w - 8.0).placeholder(placeholder);
                            ui.opacity(theme::DISABLED_ALPHA, |ui| inert(ui, |ui| field.show(ui, slot.id, field_at)));
                        }
                    }
                    AddLine::Create => {
                        let row = ListRow::new(t!("playlists.add.create")).icon(Icon::Plus).enabled(ok);
                        create = row.show(ui, slot.id, at, rect.w).activated;
                    }
                    AddLine::Heading => {
                        panel_heading(ui, rect, rect.y, t!("playlists.add.yours"));
                    }
                    AddLine::Loading => {
                        for k in 0..2 {
                            let block =
                                Rect::new(rect.x + 16.0, rect.y + 8.0 + k as f32 * 56.0, (rect.w - 32.0) * 0.8, 40.0);
                            skeleton_block(ui, block, radius::CONTROL);
                        }
                    }
                    AddLine::Note(text, color) => {
                        let opts = TextOpts::wrap(rect.w - 32.0, 0);
                        ui.text(Point::new(rect.x + 16.0, rect.y + 8.0), text, &theme::BODY, &opts, color);
                    }
                    AddLine::Playlist(card) => {
                        let row = playlist_row(card, !busy);
                        if row.show(ui, slot.id, at, rect.w).activated {
                            add = Some((card.id, card.title.clone()));
                        }
                        // The row's own picture is left empty: the playlist's cover goes in its place.
                        let h = row.height(ui, rect.w);
                        let art = Rect::new(rect.x + ROW_PAD, rect.y + (h - ROW_ART) / 2.0, ROW_ART, ROW_ART);
                        let alpha = if busy { theme::DISABLED_ALPHA } else { 1.0 };
                        ui.opacity(alpha, |ui| cover(ui, art, card, radius::TILE, ROW_ART_GLYPH));
                    }
                }
            });
        });
        if self.typing.keyboard(ui, ui.id("keyboard"), t!("playlists.add.done")) {
            create = true;
        }
        if create {
            self.create(cx);
            ui.request_repaint();
        }
        if let Some((id, name)) = add
            && !self.busy()
        {
            let items = self.item_ids.clone();
            self.adding = Some((name, cx.spawn(move |api| api.add_to_playlist(id, &items))));
            ui.request_repaint();
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        self.typing.back()
    }

    fn event(&mut self, event: &AppEvent, _cx: &mut AppCx) {
        if matches!(event, AppEvent::Changed) {
            self.list.invalidate();
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }

    fn modal(&self) -> bool {
        true
    }
}

/// A line of Add to playlist's panel.
#[derive(Clone, Copy)]
enum AddLine<'a> {
    Title,
    Field,
    Create,
    Heading,
    /// Two still rows while the playlists come.
    Loading,
    /// Why there are none to show.
    Note(&'a str, Color),
    Playlist(&'a Card),
}

impl AddLine<'_> {
    /// Its height `width` wide; `subtitle` is what is added.
    fn height(&self, ui: &Ui, width: f32, subtitle: &str, enabled: bool) -> f32 {
        let w = width - 16.0;
        match self {
            AddLine::Title => {
                let title = ui.measure(t!("playlists.add.title"), &theme::TITLE, &TextOpts::wrap(w, 2)).h;
                let sub =
                    if subtitle.is_empty() { 0.0 } else { ui.measure(subtitle, &theme::CAPTION, &TextOpts::line(w)).h };
                8.0 + title + sub + 12.0
            }
            AddLine::Field => FIELD_H,
            AddLine::Create => ListRow::new(t!("playlists.add.create")).icon(Icon::Plus).height(ui, width),
            AddLine::Heading => {
                16.0 + ui.measure(t!("playlists.add.yours"), &theme::LABEL, &TextOpts::line(width)).h + 6.0
            }
            AddLine::Loading => 8.0 + 56.0 + 40.0 + 8.0,
            AddLine::Note(text, _) => 8.0 + ui.measure(text, &theme::BODY, &TextOpts::wrap(width - 32.0, 0)).h + 8.0,
            // The rows are 2 apart.
            AddLine::Playlist(card) => playlist_row(card, enabled).height(ui, width) + 2.0,
        }
    }
}

/// A playlist's row in Add to playlist: its title over its count and length, its cover leading.
fn playlist_row(card: &Card, enabled: bool) -> ListRow<'_> {
    ListRow::new(&card.title).supporting(&card.meta).art(&[], &card.title).enabled(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_says_what_it_did() {
        assert_eq!(added_message(0, "Road trip"), "Nothing to add");
        assert_eq!(added_message(1, "Road trip"), "Added to Road trip");
        assert_eq!(added_message(12, "Road trip"), "Added 12 items to Road trip");
    }

    #[test]
    fn adding_says_what_it_did_in_the_language_shown() {
        use pley_i18n::{Language, with_language};
        with_language(Language::Fr, || {
            assert_eq!(added_message(0, "Road trip"), "Rien à ajouter");
            assert_eq!(added_message(12, "Road trip"), "12 éléments ajoutés à Road trip");
        });
        with_language(Language::Es, || {
            assert_eq!(added_message(1, "Road trip"), "Añadido a Road trip");
            assert_eq!(added_message(1500, "Road trip"), "Se añadieron 1500 elementos a Road trip");
        });
        let smart = SmartPlaylist { library_id: 1, filter: "genre=Comedy&unwatched=1".into() };
        let (heading, body) = with_language(Language::Fr, || {
            let s = NewPlaylistScreen::new(Vec::new(), Some(smart));
            (s.heading(), s.body())
        });
        assert_eq!(heading, "Enregistrer ces filtres");
        assert_eq!(body, "Genre\u{a0}: Comedy · Non vu. Elle se met à jour toute seule avec la bibliothèque.");
    }

    #[test]
    fn music_goes_in_music_playlists() {
        assert_eq!(kind_of(ItemKind::Album), PlaylistKind::Audio);
        assert_eq!(kind_of(ItemKind::Track), PlaylistKind::Audio);
        assert_eq!(kind_of(ItemKind::Show), PlaylistKind::Video);
        assert_eq!(kind_of(ItemKind::Episode), PlaylistKind::Video);
    }

    #[test]
    fn titles_fit_between_one_and_two_hundred_characters() {
        assert!(!fits("   "));
        assert!(fits(" Road trip "));
        assert!(fits(&"é".repeat(200)));
        assert!(!fits(&"a".repeat(201)));
    }

    #[test]
    fn a_new_playlist_asks_its_kind_only_when_empty() {
        assert!(NewPlaylistScreen::new(Vec::new(), None).asks_kind());
        assert!(!NewPlaylistScreen::new(vec![3], None).asks_kind());
        let smart = SmartPlaylist { library_id: 1, filter: "genre=Comedy&sort=year:desc".into() };
        let s = NewPlaylistScreen::new(Vec::new(), Some(smart));
        assert!(!s.asks_kind());
        assert_eq!(s.heading(), "Save these filters");
        assert_eq!(s.body(), "Genre: Comedy · Newest first. It keeps up with the library by itself.");
    }
}
