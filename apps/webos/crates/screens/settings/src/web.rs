//! "Manage your account on the web" (`TvWebDialog.kt`): a TV opens no browser, so a dialog gives
//! the address to type on a phone or computer and its QR code to scan. Back, Close or a click
//! outside it closes it; focus starts on Close and goes back to the row that opened it.

use pley_base::{Color, Key, Point, Rect, TextOpts};
use pley_i18n::t;
use pley_kit::{Button, Qr, bar_group, inert, is_inert, scrim_fill};
use pley_ui::theme::{self, radius};
use pley_ui::{Id, Motion, Ui};

/// The dialog's padding.
const PAD: f32 = 28.0;
/// Its text column's width.
const COLUMN: f32 = 300.0;
/// The QR code's side, in a white box 4 dp wider each way.
const QR: f32 = 220.0;
/// Between the column's lines.
const GAP: f32 = 10.0;

/// The dialog's state: the address and its code, kept while it shows.
#[derive(Debug, Default)]
pub struct WebDialog {
    url: String,
    qr: Option<Qr>,
    open: bool,
}

impl WebDialog {
    /// Opens it on `url` (`http://192.168.1.20:7539/account`).
    pub fn open(&mut self, url: String) {
        if self.url != url || self.qr.is_none() {
            self.qr = Qr::new(&url).ok();
            self.url = url;
        }
        self.open = true;
    }

    /// Closes it (it fades out).
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether it is open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The address it shows, without its scheme: `192.168.1.20:7539/account`.
    pub fn shown_address(url: &str) -> &str {
        let rest = url.split_once("://").map_or(url, |(_, r)| r);
        rest.trim_end_matches('/')
    }

    /// Draws it while open or fading out; closes it on Close, Back or a click outside.
    pub fn show(&mut self, ui: &mut Ui, id: Id) {
        let p = ui.animate_from(id.with("open"), 0.0, if self.open { 1.0 } else { 0.0 }, Motion::OVERLAY);
        if p <= 0.0 && !self.open {
            return;
        }
        scrim_fill(ui, p);
        let live = self.open && !is_inert();
        let mut closed = false;
        if live {
            let full = Rect::new(0.0, 0.0, theme::CANVAS_W, theme::CANVAS_H);
            closed |= ui.clickable(id.with("outside"), full).activated;
        }
        let title = t!("settings.web.title");
        let body = t!("settings.web.body");
        let address = WebDialog::shown_address(&self.url).to_string();
        let wrap = TextOpts::wrap(COLUMN, 0);
        let title_l = ui.layout(title, &theme::TITLE, &wrap);
        let body_l = ui.layout(body, &theme::BODY, &wrap);
        let address_l = ui.layout(&address, &theme::TITLE, &wrap);
        let close = Button::pill(t!("settings.actions.close"));
        // Title, body and address 10 apart, then the button 28 below (10, a spacer of 8, 10).
        let column_h =
            title_l.size.h + GAP + body_l.size.h + GAP + address_l.size.h + GAP + 8.0 + GAP + close.size(ui).h;
        let qr_box = QR + 8.0;
        let inner_h = column_h.max(qr_box);
        let w = PAD + COLUMN + PAD + qr_box + PAD;
        let h = PAD + inner_h + PAD;
        let card = Rect::new((theme::CANVAS_W - w) / 2.0, (theme::CANVAS_H - h) / 2.0, w, h);
        let qr = self.qr.as_mut();
        ui.opacity(p, |ui| {
            ui.fill(card, theme::SURFACE2, radius::DIALOG);
            if live {
                ui.clickable(id.with("card"), card);
            }
            let x = card.x + PAD;
            let mut y = card.y + PAD + (inner_h - column_h) / 2.0;
            let (th, bh, ah) = (title_l.size.h, body_l.size.h, address_l.size.h);
            ui.draw_text(Point::new(x, y), title_l, theme::FG);
            y += th + GAP;
            ui.draw_text(Point::new(x, y), body_l, theme::MUTED);
            y += bh + GAP;
            ui.draw_text(Point::new(x, y), address_l, theme::ACCENT_BRIGHT);
            y += ah + GAP + 8.0 + GAP;
            let at = Point::new(x, y);
            let button = |ui: &mut Ui| {
                let group = if is_inert() { pley_ui::Group::free() } else { bar_group().trap() };
                ui.group(id.with("buttons"), group, |ui| {
                    let bid = ui.id("close");
                    ui.initial_focus(bid);
                    close.show(ui, bid, at).activated
                })
            };
            closed |= if live { button(ui) } else { inert(ui, button) };
            let qx = card.x + PAD + COLUMN + PAD;
            let qy = card.y + PAD + (inner_h - qr_box) / 2.0;
            ui.fill(Rect::new(qx, qy, qr_box, qr_box), Color::WHITE, radius::DIALOG);
            if let Some(code) = qr {
                code.show(ui, Rect::new(qx + 4.0, qy + 4.0, QR, QR), 0.0, 1.0);
            }
        });
        if live && ui.take_key(Key::Back) {
            closed = true;
        }
        if closed {
            self.open = false;
            ui.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_shows_without_its_scheme() {
        assert_eq!(WebDialog::shown_address("http://192.168.1.20:7539/account"), "192.168.1.20:7539/account");
        assert_eq!(WebDialog::shown_address("https://pley.example/account/"), "pley.example/account");
        assert_eq!(WebDialog::shown_address("pley.local:7539/account"), "pley.local:7539/account");
    }

    #[test]
    fn it_keeps_its_code_while_open() {
        let mut d = WebDialog::default();
        assert!(!d.is_open());
        d.open("http://192.168.1.20:7539/account".into());
        assert!(d.is_open() && d.qr.is_some());
        d.close();
        assert!(!d.is_open());
    }
}
