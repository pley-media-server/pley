//! A library title's place in requests (`useItemRequest` and `ItemRequestLine` in the TV app's
//! `request-kit.tsx`): asked once the page has drawn, for a movie or show where requests are on
//! ([`crate::page::wants_request`]); asked again when requests change for it (or for many), when
//! the page shows again, and every 10 s while its download moves. A failure says nothing: the
//! line and the button just don't show.
//!
//! The hero shows the request's stage as a small pill (a dot and its words in the stage's colour)
//! with the download's bar beside it.

use pley_base::{Point, Rect};
use pley_core::model::ItemRequest;
use pley_core::requests::{StageWords, TvTime, moving, title_key};
use pley_core::{ApiError, Promise};
use pley_requests::{TvClock, download_bar, stage_pill};
use pley_shell::AppCx;
use pley_ui::Ui;

/// How often a moving download is asked about.
pub const POLL: f64 = 10.0;

/// A library title's request, as the server says it.
pub struct TitleRequest {
    id: i64,
    answer: Option<ItemRequest>,
    asking: Option<Promise<Result<ItemRequest, ApiError>>>,
    /// When to ask next; `None`: not until something changes.
    due: Option<f64>,
    /// The sign-in and profile the answer is for.
    generation: Option<u64>,
}

impl TitleRequest {
    /// Item `id`'s request, asked once the page wants it.
    pub fn new(id: i64) -> TitleRequest {
        TitleRequest { id, answer: None, asking: None, due: Some(0.0), generation: None }
    }

    /// Starts what is needed and collects what came back: asked while `wanted` (the page drew a
    /// movie or show, and requests are on for it); whether what shows changed.
    pub fn update(&mut self, cx: &mut AppCx, wanted: bool) -> bool {
        let mut changed = false;
        let generation = cx.state.generation();
        if self.generation != Some(generation) {
            changed = self.answer.take().is_some();
            self.asking = None;
            self.due = Some(0.0);
            self.generation = Some(generation);
        }
        if !wanted {
            // Requests went off (or the title isn't a movie or show): nothing of them shows.
            self.asking = None;
            return changed | self.answer.take().is_some();
        }
        if let Some(p) = self.asking.as_mut() {
            let answer = match p.take() {
                Some(a) => Some(a),
                None if p.failed() => Some(Err(ApiError::Unreadable("the request stopped".into()))),
                None => None,
            };
            if let Some(answer) = answer {
                self.asking = None;
                match answer {
                    Ok(r) => {
                        changed |= self.answer.as_ref() != Some(&r);
                        self.answer = Some(r);
                    }
                    Err(e) if e.cancelled() => {}
                    // A server without requests (404) or out of reach: what showed stays.
                    Err(e) => log::info!("item {} request not read: {e}", self.id),
                }
                // Its download moves without events: asked again every 10 s while it does.
                if self.due.is_none()
                    && self
                        .answer
                        .as_ref()
                        .and_then(|a| a.request.as_ref())
                        .is_some_and(|r| moving(std::slice::from_ref(r)))
                {
                    self.due = Some(cx.now + POLL);
                }
            }
        }
        if self.asking.is_none() && self.due.is_some_and(|d| cx.now >= d) && cx.state.signed_in() {
            self.due = None;
            let id = self.id;
            self.asking = Some(cx.spawn(move |api| api.item_request(id)));
        }
        changed
    }

    /// Requests changed for the title `title` (`kind:tmdb_id`), or for many (`None`): asked again
    /// when it is this one's, or before the server named it.
    pub fn changed(&mut self, title: Option<&str>) {
        let mine = match (&self.answer, title) {
            (Some(a), Some(t)) => title_key(a.kind, a.tmdb_id) == t,
            _ => true,
        };
        if mine {
            self.again();
        }
    }

    /// Asked again at once (the page shows again, the library changed).
    pub fn again(&mut self) {
        self.due = Some(0.0);
    }

    /// When it is asked next, for the page to wake then.
    pub fn due(&self) -> Option<f64> {
        self.due.filter(|_| self.asking.is_none())
    }

    /// What the server said.
    pub fn answer(&self) -> Option<&ItemRequest> {
        self.answer.as_ref()
    }
}

/// The TV's clock, for the words of a stage ("Airs today at 21:05", "Trying again in 5 min").
pub fn tv_time(cx: &AppCx) -> TvTime {
    TvClock::default().time(cx)
}

/// The line's height (the small pill's) and the space above it.
pub const LINE_HEIGHT: f32 = 22.0;
pub const LINE_GAP: f32 = 10.0;
/// The download's bar: 12 after the pill, 120 × 4.
const BAR_GAP: f32 = 12.0;
const BAR_W: f32 = 120.0;
const BAR_H: f32 = 4.0;

/// The stage line at `at`, at most `width` wide: the small pill, and the download's bar beside it.
pub fn stage_line(ui: &mut Ui, at: Point, width: f32, words: &StageWords) {
    let bar = if words.progress.is_some() { BAR_GAP + BAR_W } else { 0.0 };
    let pill = stage_pill(ui, at, words, true, (width - bar).max(0.0));
    if let Some(p) = words.progress {
        let mid = at.y + LINE_HEIGHT / 2.0;
        download_bar(ui, Rect::new(at.x + pill.w + BAR_GAP, mid - BAR_H / 2.0, BAR_W, BAR_H), p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_to_another_title_asks_nothing() {
        use pley_core::model::TitleKind;
        let mut r = TitleRequest::new(12);
        r.due = None;
        r.answer = Some(ItemRequest { kind: TitleKind::Movie, tmdb_id: 603, ..ItemRequest::default() });
        r.changed(Some(&title_key(TitleKind::Show, 603)));
        assert_eq!(r.due(), None, "TMDB numbers movies and shows apart");
        r.changed(Some(&title_key(TitleKind::Movie, 603)));
        assert_eq!(r.due(), Some(0.0));
        r.due = None;
        r.changed(None);
        assert_eq!(r.due(), Some(0.0), "a change to many asks again");
    }
}
