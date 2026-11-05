//! A programme's details in the side panel (`ProgramPanel` in `TvLiveTvScreen.kt`): the channel,
//! the programme's picture, title, episode, day and times with its badges, how far it is, what the
//! guide says of it, then Watch and the favourite at the foot: a star beside Watch, filled for a
//! favourite, as in the live player, its words only read by Audio Guidance. As a pill, Remove from
//! favourites didn't fit beside Watch and went under it.

use pley_base::{Icon, Point, Rect, TextOpts};
use pley_core::model::{LiveChannel, LiveProgram};
use pley_kit::{Badge, BadgeStyle, Button, RoundButton, bar_group};
use pley_player::live::data::time_left;
use pley_shell::AppCx;
use pley_ui::theme::{self, radius};
use pley_ui::{Id, ImageOpts, Ui};

use crate::clock::Clock;
use crate::draw::{Well, channel_tile, progress, shortened};

/// The description stops this far above the buttons.
const BUTTONS_GAP: f32 = 12.0;
/// The description is cut at a word near this many characters, a little more than 6 lines hold.
const DESCRIPTION_CHARS: usize = 400;

/// What the details asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsAction {
    Watch,
    Favourite,
}

/// The id of the details' Watch button, which takes focus as they open.
pub fn watch_id(id: Id) -> Id {
    id.with("watch")
}

/// The details' content in the panel's `area`.
pub fn show(
    ui: &mut Ui,
    cx: &AppCx,
    id: Id,
    area: Rect,
    channel: &LiveChannel,
    program: Option<&LiveProgram>,
    clock: &Clock,
) -> Option<DetailsAction> {
    let now = clock.now;
    // Watch and the favourite side by side at the foot. Placed first, so the description stops
    // above them.
    let airing = program.is_none_or(|p| p.airing_at(now));
    let watch = Button::pill(if airing { "Watch now" } else { "Watch channel" }).icon(Icon::Play);
    let favourite = if channel.favorite {
        RoundButton::new(Icon::Star, "Remove from favourites")
    } else {
        RoundButton::new(Icon::StarOutline, "Add to favourites")
    }
    .on_art(false)
    .show_label(false);
    let ws = watch.size(ui);
    let buttons_top = area.bottom() - ws.h.max(favourite.size().h);

    let mut y = area.y;
    let tile = Rect::new(area.x, y, 64.0, 38.0);
    channel_tile(ui, cx, tile, channel, Well::PAGE);
    let title = [channel.number.as_deref().filter(|n| !n.is_empty()), Some(channel.name.as_str())]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("  ");
    let tx = tile.right() + 12.0;
    crate::draw::line_mid(ui, &title, &theme::LABEL, tx, y, tile.h, area.right() - tx, theme::MUTED);
    y = tile.bottom();
    if let Some(p) = program {
        if let Some(req) = p.image.as_deref().and_then(|path| cx.image(path, ui.image_px(area.w))) {
            y += 14.0;
            let rect = Rect::new(area.x, y, area.w, 160.0);
            ui.fill(rect, theme::SURFACE2, radius::TILE);
            ui.image(rect, &req, &ImageOpts::cover().rounded(radius::TILE));
            y = rect.bottom();
        }
        y += 14.0;
        let t = ui.layout(&p.title, &theme::TITLE, &TextOpts::wrap(area.w, 2));
        let th = t.size.h;
        ui.draw_text(Point::new(area.x, y), t, theme::FG);
        y += th;
        let episode = [p.episode_label(), p.subtitle.clone().filter(|s| !s.trim().is_empty())]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        if !episode.is_empty() {
            y += 2.0;
            ui.text(Point::new(area.x, y), &episode, &theme::BODY, &TextOpts::line(area.w), theme::MUTED);
            y += theme::BODY.line_height;
        }
        y += 8.0;
        y = facts(ui, area, y, p, clock);
        if p.airing_at(now) {
            y += 10.0;
            let left = time_left(p.stop, now);
            let lw = crate::draw::measure(ui, &left, &theme::CAPTION);
            let line_h = theme::CAPTION.line_height;
            let bar = Rect::new(area.x, y + (line_h - 3.0) / 2.0, (area.w - lw - 10.0).max(0.0), 3.0);
            progress(ui, bar, p, now, theme::SURFACE4, theme::ACCENT);
            ui.text(Point::new(area.right() - lw, y), &left, &theme::CAPTION, &TextOpts::default(), theme::SUBTLE);
            y += line_h;
        }
        y += 10.0;
        let described = p.description.as_deref().filter(|d| !d.trim().is_empty());
        let (text, color) = match described {
            Some(d) => (d, theme::FG),
            None => ("The guide says nothing more of it.", theme::SUBTLE),
        };
        // As many lines as the room above the buttons holds, 6 at most, at least 1
        // (`LiveKit.cs`), less the line saying it has ended.
        let ended = p.stop <= now;
        let room = buttons_top - BUTTONS_GAP - y - if ended { theme::CAPTION.line_height + 8.0 } else { 0.0 };
        let lines = ((room / theme::BODY.line_height).floor() as u16).clamp(1, 6);
        let text = shortened(text, DESCRIPTION_CHARS);
        let d = ui.layout(&text, &theme::BODY, &TextOpts::wrap(area.w, lines));
        let dh = d.size.h;
        ui.draw_text(Point::new(area.x, y), d, color);
        y += dh;
        if ended {
            y += 8.0;
            ui.text(Point::new(area.x, y), "It has ended.", &theme::CAPTION, &TextOpts::line(area.w), theme::SUBTLE);
        }
    }

    let mut action = None;
    let (wid, fid) = (watch_id(id), id.with("favourite"));
    ui.initial_focus(wid);
    let top = buttons_top;
    ui.group(id.with("buttons"), bar_group(), |ui| {
        if watch.show(ui, wid, Point::new(area.x, top)).activated {
            action = Some(DetailsAction::Watch);
        }
        if favourite.show(ui, fid, Point::new(area.x + ws.w + theme::BUTTON_GAP, top)).activated {
            action = Some(DetailsAction::Favourite);
        }
    });
    action
}

/// The day and times, then ON NOW, LIVE, NEW and the rating, wrapping 6 apart; where they end.
fn facts(ui: &mut Ui, area: Rect, y: f32, p: &LiveProgram, clock: &Clock) -> f32 {
    let when = format!("{} · {}", clock.day(p.start), clock.span(p));
    let mut items: Vec<Badge> = Vec::new();
    if p.airing_at(clock.now) {
        items.push(Badge::on_now());
    }
    if p.is_live {
        items.push(Badge::live());
    }
    if p.is_new {
        items.push(Badge::new_programme());
    }
    let rating = p.rating.clone().filter(|r| !r.trim().is_empty());
    let line_h = theme::CAPTION.line_height.max(crate::draw::BADGE_H);
    let ww = crate::draw::measure(ui, &when, &theme::CAPTION).min(area.w);
    ui.text(
        Point::new(area.x, y + (line_h - theme::CAPTION.line_height) / 2.0),
        &when,
        &theme::CAPTION,
        &TextOpts::line(area.w),
        theme::MUTED,
    );
    let (mut x, mut top) = (area.x + ww + 6.0, y);
    let rating_badge = rating.as_deref().map(|r| Badge::new(r, BadgeStyle::Outline));
    for b in items.iter().chain(rating_badge.iter()) {
        let size = b.size(ui);
        if x + size.w > area.right() {
            x = area.x;
            top += line_h + 6.0;
        }
        b.show(ui, Point::new(x, top + (line_h - size.h) / 2.0));
        x += size.w + 6.0;
    }
    top + line_h
}
