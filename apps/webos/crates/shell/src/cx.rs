//! [`AppCx`]: everything a screen reaches besides its [`pley_ui::Ui`]: the app's state and the
//! current server's API, the TV, the task pool, saved data, navigation, playback and toasts.

use std::any::Any;
use std::sync::Arc;

use pley_base::ImageRequest;
use pley_core::model::{ImageKind, ServerEvent};
use pley_core::store::Snapshots;
use pley_core::{Api, ApiError, Device, Promise, Settings, Store, Tasks};

use pley_player::PlayerEnv;

use crate::route::{PlayRequest, Route};
use crate::state::AppState;

/// A navigation asked for while a frame draws; the shell carries it out after the frame.
#[derive(Clone, Debug, PartialEq)]
pub enum NavCommand {
    /// Opens a page over the current one.
    Push(Route),
    /// Swaps the page on top (an item that turns out to be an artist).
    Replace(Route),
    /// Closes the page on top.
    Pop,
    /// Starts the back stack over from a page (the rail, signing in, switching profile).
    Reset(Route),
    /// Sends focus to the rail's current entry, opening it.
    ToRail,
    /// Does what Back does past the page's own handling: pop, else the rail, else leave.
    Back,
    /// Leaves the app the way the TV expects.
    Exit,
}

/// A message shown at the top right for a few seconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastRequest {
    pub text: String,
    /// Shown in DangerText, for 6 s instead of 3.5.
    pub error: bool,
}

/// What a frame asked of the shell; carried out after it.
#[derive(Debug, Default)]
pub struct Requests {
    pub nav: Vec<NavCommand>,
    pub toasts: Vec<ToastRequest>,
}

/// A session that plays on while people browse: the music player. The music screens put theirs
/// in [`AppState::music`]; the shell runs it every frame, shows Now Playing in the rail while it
/// has a queue, and ends it before a profile switch, a sign-out, another server or leaving.
pub trait BackgroundSession: Any {
    /// Whether it has a queue: the rail shows Now Playing.
    fn queued(&self) -> bool;
    /// Runs it at `now` (its engine's news, its reports, its clock); when it next wants to run.
    fn tick(&mut self, now: f64) -> Option<f64>;
    /// The app went to the background: pause what plays.
    fn pause(&mut self);
    /// Ends the session on the server and stops playing.
    fn end(&mut self);
    /// A server event (a session stopped from elsewhere ends it).
    fn server_event(&mut self, _event: &ServerEvent) {}
    /// Itself, for the music screens to reach their own type.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// What a screen works with, alongside its [`pley_ui::Ui`], while it draws or hears an event.
pub struct AppCx<'a> {
    /// The app's state: servers, sign-in, who is watching, libraries, Home, Live TV's summary,
    /// offline, settings.
    pub state: &'a mut AppState,
    /// The frame's time ([`pley_base::time::now`]).
    pub now: f64,
    requests: &'a mut Requests,
}

impl<'a> AppCx<'a> {
    /// The context of a frame at `now`.
    pub fn new(state: &'a mut AppState, requests: &'a mut Requests, now: f64) -> AppCx<'a> {
        AppCx { state, now, requests }
    }

    /// The current server's API as the current sign-in; `None` before a server is chosen.
    pub fn api(&self) -> Option<&Api> {
        self.state.api()
    }

    /// Runs `f` with the current API on the task pool. Without one, the promise holds
    /// [`ApiError::SignedOut`].
    pub fn spawn<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Api) -> Result<T, ApiError> + Send + 'static,
    ) -> Promise<Result<T, ApiError>> {
        self.state.spawn(f)
    }

    /// The task pool.
    pub fn tasks(&self) -> &Tasks {
        self.state.tasks()
    }

    /// The TV.
    pub fn device(&self) -> &Arc<dyn Device> {
        self.state.device()
    }

    /// What the TV keeps.
    pub fn store(&self) -> &Store {
        self.state.store()
    }

    /// Saved answers per server and profile.
    pub fn snapshots(&self) -> &Arc<Snapshots> {
        self.state.snapshots()
    }

    /// This TV's settings.
    pub fn settings(&self) -> &Settings {
        self.state.settings()
    }

    /// Changes the settings and keeps them.
    pub fn update_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        self.state.update_settings(f);
    }

    /// What a player made now works with (pley-player's [`PlayerEnv`]): the current server and
    /// sign-in, the profile watching, the TV's capabilities, the settings, and the report queue,
    /// quality, Automatic's memory and subtitle sync every player shares. `None` without a server.
    pub fn player_env(&mut self) -> Option<PlayerEnv> {
        crate::players::player_env(self.state)
    }

    /// This TV's id for playbacks and Live TV (`webos-…`).
    pub fn install_id(&self) -> &str {
        self.state.install_id()
    }

    /// The `User-Agent` requests carry (`pleyLG/0.1.0 (webOS 23; OLED65C3PUA)`).
    pub fn user_agent(&self) -> &str {
        self.state.user_agent()
    }

    /// A picture the server gave (`/api/items/12/images/poster?v=…`) at the server's size for
    /// `width_px` device pixels.
    pub fn image(&self, path: &str, width_px: u32) -> Option<ImageRequest> {
        self.api().map(|api| api.image(path, width_px))
    }

    /// An item's picture by kind.
    pub fn item_image(&self, item: i64, kind: ImageKind, version: Option<&str>, width_px: u32) -> Option<ImageRequest> {
        self.api().map(|api| ImageRequest::new(api.item_image(item, kind, version, width_px), width_px))
    }

    /// A profile's picture (`/api/users/{id}/avatar?v=…`), which the server sends at 256 px.
    pub fn avatar(&self, avatar_url: &str, width_px: u32) -> Option<ImageRequest> {
        self.api().map(|api| ImageRequest::new(api.url(avatar_url), width_px))
    }

    // Navigation.

    /// Opens `route` over the current page.
    pub fn push(&mut self, route: Route) {
        self.requests.nav.push(NavCommand::Push(route));
    }

    /// Opens `route` in place of the current page.
    pub fn replace(&mut self, route: Route) {
        self.requests.nav.push(NavCommand::Replace(route));
    }

    /// Closes the current page.
    pub fn pop(&mut self) {
        self.requests.nav.push(NavCommand::Pop);
    }

    /// Starts the back stack over from `route`.
    pub fn reset(&mut self, route: Route) {
        self.requests.nav.push(NavCommand::Reset(route));
    }

    /// Sends focus to the rail's current entry (a page with nothing to focus).
    pub fn to_rail(&mut self) {
        self.requests.nav.push(NavCommand::ToRail);
    }

    /// Does what Back does once the page has nothing of its own to close.
    pub fn back(&mut self) {
        self.requests.nav.push(NavCommand::Back);
    }

    /// Leaves the app.
    pub fn exit(&mut self) {
        self.requests.nav.push(NavCommand::Exit);
    }

    /// What this frame asked to open, so far.
    pub fn pending(&self) -> &[NavCommand] {
        &self.requests.nav
    }

    // Playback.

    /// Plays a video: opens the player on `request`.
    pub fn play(&mut self, request: PlayRequest) {
        let request =
            PlayRequest { pressed_at: if request.pressed_at > 0.0 { request.pressed_at } else { self.now }, ..request };
        self.push(Route::Player(request));
    }

    /// The music session, as the music screens' own type.
    pub fn music<T: BackgroundSession>(&mut self) -> Option<&mut T> {
        self.state.music.as_mut().and_then(|m| m.as_any_mut().downcast_mut::<T>())
    }

    /// Starts `session` as the music playing in the background, ending the one before.
    pub fn set_music(&mut self, session: Box<dyn BackgroundSession>) {
        if let Some(mut old) = self.state.music.replace(session) {
            old.end();
        }
    }

    // Toasts.

    /// Shows `text` at the top right for 3.5 s.
    pub fn toast(&mut self, text: impl Into<String>) {
        self.requests.toasts.push(ToastRequest { text: text.into(), error: false });
    }

    /// Shows `text` as an error, in DangerText, for 6 s.
    pub fn toast_error(&mut self, text: impl Into<String>) {
        self.requests.toasts.push(ToastRequest { text: text.into(), error: true });
    }
}
