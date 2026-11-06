//! Pley for LG TV's video player: the controls, the timeline and its previews, the
//! panels, Skip and Up Next, and the subtitles over the picture.
//!
//! The screen ([`PlayerScreen`]) owns one [`pley_player::PlaybackController`] on the TV's engine
//! and draws over a transparent frame, so the picture the TV's media pipeline shows under the
//! interface is seen everywhere the player draws nothing. What the screen shows and how it
//! answers keys lives in [`PlayerView`], which drives the playback through the [`Playback`]
//! trait so it can be tested against a stand-in.
//!
//! A video on the USB drive ([`PlayRequest::file`]) plays the same way, with no server behind
//! it: pley-local answers for it, and finding subtitles isn't offered.

mod chapters;
mod clock;
mod controls;
mod overlays;
mod panels;
mod playback;
mod previews;
mod subtitles;
mod text;
mod view;

use std::sync::Arc;

use pley_core::Device;
use pley_i18n::t;
use pley_player::{PlaybackController, SavedPlayback};
use pley_shell::{AppCx, AppEvent, Chrome, PlayQueue, PlayRequest, Route, Screen};
use pley_ui::Ui;

pub use controls::{Btn, RowOptions};
pub use panels::{Panel, Pick};
pub use playback::Playback;
pub use previews::{KEEP_SHEET_S, Previews, Tile, Want};
pub use view::{CONFIRM_MS, FrameEnv, HIDE_AFTER_MS, Out, PlayerView, STOPPED_LEAVE_MS, Style, Target, keeps_awake};

/// What the player says when a drive's video is opened and the drive is gone.
pub fn no_drive() -> &'static str {
    t!("player.errors.noDrive")
}

/// The screen of `route` when it is one of this crate's pages: the video player.
pub fn screen(route: &Route, _cx: &mut AppCx) -> Option<Box<dyn Screen>> {
    match route {
        Route::Player(request) => Some(Box::new(PlayerScreen::new(request.clone()))),
        _ => None,
    }
}

/// The player's request in pley-player's terms.
pub fn player_request(r: &PlayRequest) -> pley_player::PlayRequest {
    pley_player::PlayRequest {
        item_id: r.item_id,
        file_id: r.file_id,
        start_ms: r.start_ms.map(|ms| ms.min(i64::MAX as u64) as i64),
        from_start: r.from_start,
        audio_index: r.audio_index,
        subtitle_index: r.subtitle_index,
        queue: r.queue.map(|q| match q {
            PlayQueue::Playlist(id) => pley_player::Queue::Playlist(id),
            PlayQueue::Collection(id) => pley_player::Queue::Collection(id),
        }),
    }
}

/// The video player page (`TvPlayerScreen.kt`; the TV app's `ui/player/player.tsx`): one
/// playback, made on the first frame (a factory may be asked for a screen it then drops), let go
/// while the TV hides the app and made again where it was when it shows.
pub struct PlayerScreen {
    request: PlayRequest,
    playback: Option<PlaybackController>,
    view: PlayerView,
    /// Where it was when the TV hid the app.
    saved: Option<SavedPlayback>,
    started: bool,
    device: Option<Arc<dyn Device>>,
    /// The TV's screen saver is kept away (while the video plays).
    awake: bool,
}

impl PlayerScreen {
    /// The player for `request`; nothing plays before its first frame.
    pub fn new(request: PlayRequest) -> PlayerScreen {
        PlayerScreen {
            request,
            playback: None,
            view: PlayerView::new(),
            saved: None,
            started: false,
            device: None,
            awake: false,
        }
    }

    /// Makes the playback: the music gives way (one thing plays at a time), and the controller
    /// starts on the TV's engine, its seek previews fetched with the pictures' fetcher. A video on
    /// the drive plays from the file, the drive's own; anything else through the server. False
    /// without a server, or for a file whose drive isn't plugged in.
    fn begin(&mut self, request: pley_player::PlayRequest, pressed_at: f64, cx: &mut AppCx) -> bool {
        let env = match &self.request.file {
            Some(file) => {
                let drive = cx.device().drive().filter(|d| d.readable() && file.starts_with(&d.root));
                let Some(drive) = drive else { return false };
                self.view.set_can_find(false);
                cx.local_player_env(file.clone(), drive.root)
            }
            None => match cx.player_env() {
                Some(env) => env,
                None => return false,
            },
        };
        if let Some(music) = cx.state.music.as_mut() {
            music.pause();
        }
        self.view.set_previews(Previews::new(cx.state.picture_fetch(), cx.tasks().clone()));
        let device = cx.device().clone();
        let now = (pressed_at * 1000.0) as i64;
        let mut p = PlaybackController::new(request, env, device.engine(), now);
        p.start();
        self.device = Some(device);
        self.playback = Some(p);
        true
    }

    /// Ends the playback (its session on the server, the TV's engine) and lets the screen sleep.
    fn end(&mut self) {
        if let Some(mut p) = self.playback.take() {
            p.close();
        }
        self.keep_awake(false);
    }

    /// Keeps the TV's screen saver away, or lets it come, when that changes.
    fn keep_awake(&mut self, on: bool) {
        if on != self.awake
            && let Some(d) = &self.device
        {
            d.keep_awake(on);
            self.awake = on;
        }
    }

    fn leave(&mut self, cx: &mut AppCx) {
        self.end();
        cx.pop();
    }

    /// The playback couldn't be made: says why and leaves, rather than showing an empty page.
    fn cant_begin(&mut self, cx: &mut AppCx) {
        if self.request.file.is_some() {
            log::warn!("the player opened on a drive that isn't there");
            cx.toast_error(no_drive());
        } else {
            log::warn!("the player opened without a server");
            cx.toast_error(t!("player.errors.noServer"));
        }
        self.leave(cx);
    }
}

impl Drop for PlayerScreen {
    fn drop(&mut self) {
        self.end();
    }
}

impl Screen for PlayerScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        ui.set_clear(pley_base::Color::TRANSPARENT);
        if !self.started {
            self.started = true;
            let pressed = if self.request.pressed_at > 0.0 { self.request.pressed_at } else { cx.now };
            if !self.begin(player_request(&self.request), pressed, cx) {
                self.cant_begin(cx);
                return;
            }
        }
        let Some(p) = self.playback.as_mut() else { return };
        let hours24 = cx.device().clock_24h();
        let out = {
            let env = FrameEnv { settings: cx.settings(), hours24, unix_now: clock::unix_now_f64() };
            self.view.frame(ui, p, &env)
        };
        let awake = keeps_awake(&p.state().phase, p.wants_to_play());
        self.keep_awake(awake);
        if let Some(style) = out.style {
            cx.update_settings(|s| style.apply(s));
        }
        if out.leave {
            self.leave(cx);
        }
    }

    fn back(&mut self, _cx: &mut AppCx) -> bool {
        if let Some(p) = self.playback.as_mut()
            && self.view.back(p)
        {
            return true;
        }
        // Leaving: the session ends now rather than when the shell drops the page.
        self.end();
        false
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Server(e) => {
                if let Some(p) = self.playback.as_mut() {
                    p.server_event(e);
                }
            }
            // The TV hid the app (Home, another app, standby): the session ends and the engine
            // lets go, remembering the place (`player.tsx`'s `saveState` then `close`).
            AppEvent::Visible(false) => {
                if let Some(p) = self.playback.as_ref() {
                    let saved = p.save_state();
                    log::info!("the player lets go while hidden, at {:?} ms", saved.request.start_ms);
                    self.saved = Some(saved);
                }
                self.end();
            }
            // Shown again: a new playback where it was, paused, as the Samsung and Android apps
            // leave a video the TV's Home interrupted, with the controls showing as a pause does.
            // A drive's video starts again from the same file; the drive pulled out meanwhile,
            // the player says so and leaves, as it does when it opens.
            AppEvent::Visible(true) => {
                if let Some(saved) = self.saved.take() {
                    if !self.begin(saved.request, cx.now, cx) {
                        self.cant_begin(cx);
                        return;
                    }
                    if let Some(p) = self.playback.as_mut() {
                        p.pause();
                    }
                }
                self.view.reopened();
            }
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Video
    }

    fn modal(&self) -> bool {
        self.view.modal()
    }
}
