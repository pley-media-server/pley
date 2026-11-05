//! A title's one main action as buttons (`TitleActions` in `request-kit.tsx`): the stage and a
//! download's bar beside [Requested] or [Following], then what it says under them: the stage's
//! line, the allowance (for the title's kind) when nothing is left to ask for, why it can't be
//! asked for. Discover's hero has room for little: there the first of what it says goes above
//! the buttons, as it does wherever no button of the main action's shows.

use pley_base::{Icon, Point, Rect, TextOpts};
use pley_core::model::{
    DiscoverDetail, DiscoverTitle, MediaRequest, NewRequest, RequestAction, RequestSummary, TitleKind,
};
use pley_core::requests::{
    Allowance, MainAction, NO_ACCESS, StageWords, TvTime, allowance_lines, list_action, main_action, stage_words,
    title_request,
};
use pley_kit::{Button, RoundButton, bar_group};
use pley_ui::theme;
use pley_ui::{Id, Ui};

use crate::common::{keep_last_words, pill_size, stage_block};

/// What a title's buttons ask for.
#[derive(Clone, Debug, PartialEq)]
pub enum TitleIntent {
    Ask(NewRequest),
    /// Asking for a show: its seasons are picked first.
    Pick,
    Open(i64),
    Unwatch(Box<MediaRequest>),
    SignIn,
    /// One of the buttons after the main action's, by its key.
    Extra(&'static str),
}

/// A button of the row.
struct Btn {
    key: &'static str,
    label: &'static str,
    icon: Icon,
    enabled: bool,
    intent: Option<TitleIntent>,
    round: bool,
}

/// A line under the buttons.
pub struct Note {
    pub icon: Option<Icon>,
    pub text: String,
    pub danger: bool,
}

/// A button after the main action's: its key, icon and label.
#[derive(Clone, Copy, Debug)]
pub struct Extra {
    pub key: &'static str,
    pub icon: Icon,
    pub label: &'static str,
    /// Shown only when none of the main action's buttons takes focus first.
    pub unclaimed_only: bool,
}

/// The main action worked out: its buttons, the stage beside them, and what it says.
pub struct Actions {
    buttons: Vec<Btn>,
    stage: Option<StageWords>,
    after: Vec<Btn>,
    pub notes: Vec<Note>,
}

impl Actions {
    /// The main action of a title: from its details, else (while they load) from the list's title.
    pub fn of(d: Option<&DiscoverDetail>, t: &DiscoverTitle, s: Option<&RequestSummary>, time: TvTime) -> Actions {
        let action = match d {
            Some(d) => main_action(d, s, time),
            None => list_action(t, s),
        };
        let manage = s.is_some_and(|s| s.manage);
        let mut out = Actions { buttons: Vec::new(), stage: None, after: Vec::new(), notes: Vec::new() };
        let main = |label, icon, intent: Option<TitleIntent>, enabled| Btn {
            key: "main",
            label,
            icon,
            enabled,
            intent,
            round: false,
        };
        let note = |text: &str| Note { icon: None, text: text.to_string(), danger: false };
        let allowance = |out: &mut Actions| {
            let lines = allowance_lines(&Allowance::of(d, s, Some(t.kind)), time);
            out.notes.extend(lines.unwrap_or_default().iter().map(|l| note(l)));
        };
        let request = d.and_then(title_request);
        match &action {
            MainAction::SignIn => {
                out.buttons.push(main("Sign in", Icon::UserRound, Some(TitleIntent::SignIn), true));
            }
            MainAction::Restricted { text } => {
                out.notes.push(Note { icon: Some(Icon::Lock), text: text.clone(), danger: false })
            }
            MainAction::Open { item_id } => {
                out.buttons.push(main("Open", Icon::Play, item_id.map(TitleIntent::Open), item_id.is_some()))
            }
            MainAction::Declined { text } => {
                out.notes.push(Note { icon: Some(Icon::CircleX), text: text.clone(), danger: true })
            }
            // Inert: it says where things are; the stage beside it says how far.
            MainAction::Requested { .. } => out.buttons.push(main("Requested", Icon::Check, None, true)),
            MainAction::Following { request: r } => {
                out.buttons.push(main("Following", Icon::Bell, None, true));
                if let Some(r) = r.filter(|r| r.can(RequestAction::Unwatch)) {
                    out.after.push(Btn {
                        key: "unwatch",
                        label: "Stop following",
                        icon: Icon::BellOff,
                        enabled: true,
                        intent: Some(TitleIntent::Unwatch(Box::new((*r).clone()))),
                        round: true,
                    });
                }
            }
            MainAction::TellMeToo { seasons } => {
                let seasons = seasons.clone().filter(|s| !s.is_empty());
                let body = NewRequest { kind: t.kind, tmdb_id: t.tmdb_id, seasons };
                out.buttons.push(main("Tell me too", Icon::Bell, Some(TitleIntent::Ask(body)), true));
                out.notes.push(note("Someone asked for it already."));
            }
            MainAction::More { item_id, more, ask, disabled } => {
                if let Some(id) = item_id {
                    out.buttons.push(main("Open", Icon::Play, Some(TitleIntent::Open(*id)), true));
                }
                if *more && *ask {
                    out.buttons.push(Btn {
                        key: "more",
                        label: "Request more seasons",
                        icon: Icon::Plus,
                        enabled: !disabled,
                        intent: Some(TitleIntent::Pick),
                        round: false,
                    });
                    if *disabled {
                        allowance(&mut out);
                    }
                } else if !ask {
                    out.notes.push(note(NO_ACCESS));
                }
            }
            MainAction::Request { disabled } => {
                let intent = if t.kind == TitleKind::Movie {
                    TitleIntent::Ask(NewRequest { kind: TitleKind::Movie, tmdb_id: t.tmdb_id, seasons: None })
                } else {
                    TitleIntent::Pick
                };
                out.buttons.push(main("Request", Icon::Plus, Some(intent), !disabled));
                if *disabled {
                    allowance(&mut out);
                }
            }
            MainAction::NoAccess => out.notes.push(note(NO_ACCESS)),
        }
        if matches!(action, MainAction::Requested { .. } | MainAction::Following { .. }) {
            out.stage = request.map(|r| stage_words(r, manage, time));
        }
        // The stage's own line comes first: it says more of the pill beside the button.
        if let Some(n) = out.stage.as_ref().and_then(|s| s.note.clone()) {
            out.notes.insert(0, note(&n));
        }
        out
    }

    /// Whether one of the main action's buttons takes focus first.
    pub fn claims(&self) -> bool {
        self.buttons.iter().any(|b| b.enabled)
    }

    /// The notes drawn: the first only when `compact`.
    fn said(&self, compact: bool) -> &[Note] {
        if compact { &self.notes[..self.notes.len().min(1)] } else { &self.notes }
    }

    /// Whether what it says goes over the buttons.
    fn above(&self, compact: bool) -> bool {
        compact || self.buttons.is_empty()
    }

    /// The height drawn from the top, at most `width` wide, with a row of buttons.
    pub fn height(&self, ui: &Ui, compact: bool, width: f32) -> f32 {
        let notes = notes_height(ui, self.said(compact), width);
        let gap = if notes > 0.0 { if self.above(compact) { 14.0 } else { 12.0 } } else { 0.0 };
        notes + gap + theme::BUTTON_HEIGHT
    }

    /// Draws the buttons from `at` in a row group `id` (the main action's first takes focus first
    /// when `initial`), with `extras` after them, and what it says; what was pressed.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &self,
        ui: &mut Ui,
        id: Id,
        at: Point,
        width: f32,
        compact: bool,
        initial: bool,
        extras: &[Extra],
    ) -> Option<TitleIntent> {
        let said = self.said(compact);
        let notes_h = notes_height(ui, said, width);
        let above = self.above(compact);
        let mut y = at.y;
        if above && notes_h > 0.0 {
            draw_notes(ui, Point::new(at.x, y), said, width);
            y += notes_h + 14.0;
        }
        let claimed = initial && self.claims();
        let mut out = None;
        ui.group(id, bar_group(), |ui| {
            let mut x = at.x;
            let mut first = initial;
            let mut show = |ui: &mut Ui, b: &Btn, x: &mut f32, first: &mut bool| {
                let bid = ui.id(b.key);
                if *first && b.enabled {
                    ui.initial_focus(bid);
                    *first = false;
                }
                let r = if b.round {
                    let r = RoundButton::new(b.icon, b.label).show(ui, bid, Point::new(*x, y));
                    *x += RoundButton::new(b.icon, b.label).size().w;
                    r
                } else {
                    let button = Button::pill(b.label).icon(b.icon).enabled(b.enabled);
                    let r = button.show(ui, bid, Point::new(*x, y));
                    *x += button.size(ui).w;
                    r
                };
                *x += theme::BUTTON_GAP;
                if r.activated {
                    out = b.intent.clone();
                }
            };
            for b in &self.buttons {
                show(ui, b, &mut x, &mut first);
            }
            if let Some(stage) = &self.stage {
                // Beside a 40 dp button, the pill (and its bar) centred on it.
                let room = (at.x + width + 300.0 - x).max(120.0);
                let h = pill_size(ui, stage, false, room).h + if stage.progress.is_some() { 8.0 } else { 0.0 };
                let size = stage_block(ui, Point::new(x, y + (theme::BUTTON_HEIGHT - h) / 2.0), stage, room, false);
                x += size.w + theme::BUTTON_GAP;
            }
            for b in &self.after {
                show(ui, b, &mut x, &mut first);
            }
            let mut first_extra = initial && !claimed;
            for e in extras.iter().filter(|e| !e.unclaimed_only || !claimed) {
                let b = Btn {
                    key: e.key,
                    label: e.label,
                    icon: e.icon,
                    enabled: true,
                    intent: Some(TitleIntent::Extra(e.key)),
                    round: true,
                };
                show(ui, &b, &mut x, &mut first_extra);
            }
        });
        if !above && notes_h > 0.0 {
            draw_notes(ui, Point::new(at.x, y + theme::BUTTON_HEIGHT + 12.0), said, width);
        }
        out
    }
}

const NOTE_ICON: f32 = 16.0;

fn note_opts(note: &Note, width: f32) -> (TextOpts, f32) {
    let indent = if note.icon.is_some() { NOTE_ICON + 8.0 } else { 0.0 };
    (TextOpts::wrap((width - indent).max(1.0), 2), indent)
}

fn notes_height(ui: &Ui, notes: &[Note], width: f32) -> f32 {
    let mut h = 0.0;
    for (i, n) in notes.iter().enumerate() {
        let (opts, _) = note_opts(n, width);
        h += ui.measure(&keep_last_words(&n.text), &theme::BODY, &opts).h + if i > 0 { 4.0 } else { 0.0 };
    }
    h
}

/// What the main action says, muted and close together; a refusal's icon in the danger colour.
fn draw_notes(ui: &mut Ui, at: Point, notes: &[Note], width: f32) {
    let mut y = at.y;
    for n in notes {
        let (opts, indent) = note_opts(n, width);
        if let Some(icon) = n.icon {
            let tint = if n.danger { theme::DANGER_TEXT } else { theme::MUTED };
            ui.icon(Rect::new(at.x, y + 2.0, NOTE_ICON, NOTE_ICON), icon, tint);
        }
        let size = ui.text(Point::new(at.x + indent, y), &keep_last_words(&n.text), &theme::BODY, &opts, theme::MUTED);
        y += size.h + 4.0;
    }
}
