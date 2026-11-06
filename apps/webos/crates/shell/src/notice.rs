//! Ready-to-watch news over the pages, one at a time (the TV app's `shell/notice.tsx`; TVs pop up
//! nothing else about requests): a card at the bottom right with Open and Close. It shows once
//! the remote has rested and nothing else holds focus: never over a playing video, a panel, a
//! dialog, the open rail, Your requests or a field that types. It gives focus back to where it was
//! when it closes: by Close, Back, Up or Down, or after 10 s untouched.

use pley_base::{Icon, ImageRequest, Key, Point, Rect, TextOpts};
use pley_i18n::t;
use pley_kit::Button;
use pley_ui::theme::{self, radius};
use pley_ui::{Dir, Edge, Group, Id, ImageOpts, Ui};

use crate::request_state::RequestNotice;
use crate::state::AppState;

/// Untouched this long, a notice goes by itself (seconds).
pub const NOTICE_SHOWN: f64 = 10.0;
/// A notice waits for the remote to rest this long, so it never takes a press meant for the page.
pub const NOTICE_IDLE: f64 = 1.5;
/// OK this soon after a notice took focus was meant for what had it before: it does nothing.
const OK_GUARD: f64 = 0.6;
/// The card's width and its distance from the screen's right and bottom edges.
const W: f32 = 360.0;
const RIGHT: f32 = 48.0;
const BOTTOM: f32 = 51.0;
const PAD: f32 = 16.0;

/// The notice on screen, and when the remote was last used.
#[derive(Debug, Default)]
pub struct Notices {
    shown: Option<RequestNotice>,
    shown_at: f64,
    touched_at: f64,
    last_key_at: f64,
    /// What had focus when the notice took it, to give it back.
    opener: Option<Id>,
}

impl Notices {
    /// Whether a notice is on screen (it holds focus meanwhile).
    pub fn showing(&self) -> bool {
        self.shown.is_some()
    }

    /// When the card next needs a frame: to show the waiting notice, or to go by itself.
    pub fn next_due(&self, waiting: bool) -> Option<f64> {
        match &self.shown {
            Some(_) => Some(self.touched_at + NOTICE_SHOWN),
            None if waiting => Some(self.last_key_at + NOTICE_IDLE),
            None => None,
        }
    }

    /// Draws the notice over the page; `free`: nothing else holds focus. Returns the notice
    /// opened (the shell opens its title, or Your requests).
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, free: bool) -> Option<RequestNotice> {
        let now = ui.time();
        if !ui.keys().is_empty() {
            self.last_key_at = now;
        }
        // Gone from the queue meanwhile (another profile): it goes.
        if let Some(n) = &self.shown
            && state.request_notice().is_none_or(|first| first.id != n.id)
        {
            self.shown = None;
            state.showing_request_notice(None);
        }
        if self.shown.is_none() {
            let rested = now - self.last_key_at >= NOTICE_IDLE;
            let next = state.request_notice().filter(|_| free && rested).cloned()?;
            state.showing_request_notice(Some(next.id));
            self.opener = ui.focused();
            self.shown_at = now;
            self.touched_at = now;
            ui.request_focus(ui.id("request-notice").with("open"));
            self.shown = Some(next);
        }
        let notice = self.shown.clone()?;
        if !ui.keys().is_empty() {
            self.touched_at = now;
        }

        let mut close = now - self.touched_at >= NOTICE_SHOWN;
        let mut open = false;
        for key in [Key::Back, Key::Up, Key::Down] {
            close |= ui.take_key(key);
        }
        let art = notice.poster.as_ref().and_then(|p| {
            let api = state.api()?;
            Some(ImageRequest::new(api.url(p), ui.image_px(40.0)))
        });
        let text_x = if art.is_some() { PAD + 40.0 + 14.0 } else { PAD };
        let text_w = W - text_x - PAD - theme::ICON_SIZE - 10.0;
        let opts = TextOpts::wrap(text_w, 3);
        let text_h = ui.measure(&notice.text, &theme::BODY, &opts).h;
        let head_h = text_h.max(if art.is_some() { 60.0 } else { 0.0 });
        let h = PAD + head_h + 14.0 + theme::BUTTON_HEIGHT + PAD;
        let card = Rect::new(theme::CANVAS_W - RIGHT - W, theme::CANVAS_H - BOTTOM - h, W, h);
        ui.fill(card, theme::alpha(theme::SURFACE2, 0.96), radius::CARD);
        ui.fill(Rect::new(card.x + radius::CARD, card.y, card.w - 2.0 * radius::CARD, 1.0), theme::SURFACE4, 0.0);
        if let Some(req) = &art {
            let rect = Rect::new(card.x + PAD, card.y + PAD, 40.0, 60.0);
            ui.fill(rect, theme::SURFACE3, 4.0);
            ui.image(rect, req, &ImageOpts { radius: 4.0, ..ImageOpts::default() });
        }
        let top = card.y + PAD + (head_h - text_h).max(0.0) / 2.0;
        let icon = Rect::new(card.x + text_x, top, theme::ICON_SIZE, theme::ICON_SIZE);
        ui.icon(icon, Icon::CircleCheck, theme::SUCCESS);
        let at = Point::new(icon.right() + 10.0, top);
        ui.text(at, &notice.text, &theme::BODY, &opts, theme::FG);

        let root = ui.id("request-notice");
        ui.announce_once(root.with("read"), || notice.text.clone());
        let buttons_y = card.y + PAD + head_h + 14.0;
        let group = Group::row().edge(Dir::Up, Edge::Stop).edge(Dir::Down, Edge::Stop).trap();
        ui.group(root, group, |ui| {
            let close_b = Button::text(t!("common.actions.close"));
            let open_b = Button::text(t!("common.actions.open"));
            let close_w = close_b.size(ui).w;
            let open_w = open_b.size(ui).w;
            let right = card.right() - PAD;
            let guarded = now - self.shown_at < OK_GUARD;
            let o = open_b.show(ui, ui.id("open"), Point::new(right - close_w - 10.0 - open_w, buttons_y));
            let c = close_b.show(ui, ui.id("close"), Point::new(right - close_w, buttons_y));
            if (o.focused || c.focused) && (o.activated || c.activated || o.pressed || c.pressed) {
                self.touched_at = now;
            }
            if !guarded {
                open |= o.activated;
                close |= c.activated;
            }
        });
        ui.repaint_after((self.touched_at + NOTICE_SHOWN - now).max(0.0));
        if !(open || close) {
            return None;
        }
        self.shown = None;
        state.drop_request_notice(notice.id);
        if close
            && !open
            && let Some(id) = self.opener.take()
        {
            ui.request_focus(id);
        }
        open.then_some(notice)
    }
}
