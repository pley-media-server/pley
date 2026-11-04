//! The music that plays on while people browse: pley-player's [`MusicSession`] held by the shell
//! as its [`BackgroundSession`], so the rail shows Now Playing while it has a queue and the shell
//! runs it every frame, makes it give way when the app goes to the background or a video or a
//! channel plays, and ends it before a profile switch, a sign-out or another server.
//!
//! The pages reach the music through [`MusicPlayer`], which keeps three things Android's player
//! does that the session alone doesn't: Pause pressed while a track starts opens it paused
//! (`playWhenReady`); once the music gave way, its place still shows, seeks move it and Play goes
//! on from it (Android only pauses its player); and an artist's Play starts as soon as their
//! tracks come, whatever page shows then, while the artist's page is on the stack
//! (`ArtistViewModel.play` in `viewModelScope`).

use std::any::Any;
use std::sync::{Arc, Weak};

use pley_base::Key;
use pley_core::http::sentence;
use pley_core::model::{Control, ItemKind, ItemList, ItemSummary, ServerEvent};
use pley_core::{Api, ApiError, PlayerEngine, Promise, Tasks};
use pley_player::remote::{Command, RemoteAction, restarts_track};
use pley_player::sleep::SleepState;
use pley_player::{MusicSession, NowPlaying, PlayerEnv};
use pley_shell::{AppCx, BackgroundSession};
use pley_ui::Ui;

use crate::pages::{self, SEEK_STEP_MS};

/// How often the engine is asked for news while a track opens, and for how long at most. The
/// TV's engine wakes the app itself when it has news; the simulator's doesn't, and a track's start
/// shouldn't wait for the next frame. Mid-track waits aren't polled.
const OPENING_POLL_MS: i64 = 250;
const OPENING_POLL_FOR_MS: i64 = 10_000;
/// A track's end is looked for this long after the clock says it comes.
const END_SLACK_MS: i64 = 20;

/// An answer from the server.
type Asked<T> = Promise<Result<T, ApiError>>;

/// How an artist's Play or Shuffle went ([`gather`]), for the page that asked.
#[derive(Clone, Debug, PartialEq)]
pub enum Gathered {
    /// The music started on their tracks.
    Played,
    /// They have no tracks.
    Nothing,
    /// Why the server couldn't give them, in a sentence.
    Failed(String),
    /// Nothing to tell: other music started first, the music ended, or the ask was cancelled.
    Dropped,
}

/// An artist's tracks on their way: their albums, then every album's tracks, asked for together
/// (Android's `CachedPlayItem`).
struct Gathering {
    ticket: u64,
    shuffle: bool,
    /// The page that asked: once it goes, so does its Play, as Android's view model's work does.
    owner: Weak<()>,
    api: Api,
    tasks: Tasks,
    step: Step,
}

enum Step {
    Albums(Asked<ItemList>),
    Tracks(Vec<Asked<ItemList>>, Vec<Option<Vec<ItemSummary>>>),
}

impl Gathering {
    /// Moves on with what came: every track, in the albums' order, once all are in; the first
    /// failure; or `None` while answers are still to come.
    fn poll(&mut self) -> Option<Result<Vec<ItemSummary>, ApiError>> {
        if let Step::Albums(task) = &mut self.step {
            let albums: Vec<i64> = match answer(task)? {
                Ok(list) => list.items.iter().filter(|a| a.kind == ItemKind::Album).map(|a| a.id).collect(),
                Err(e) => return Some(Err(e)),
            };
            let tasks: Vec<Asked<ItemList>> = albums
                .into_iter()
                .map(|id| {
                    let api = self.api.clone();
                    self.tasks.spawn(move || api.children(id))
                })
                .collect();
            let got = tasks.iter().map(|_| None).collect();
            self.step = Step::Tracks(tasks, got);
        }
        let Step::Tracks(tasks, got) = &mut self.step else { return None };
        for (task, slot) in tasks.iter_mut().zip(got.iter_mut()) {
            if slot.is_none() {
                match answer(task) {
                    Some(Ok(list)) => *slot = Some(pages::tracks_of(&list.items)),
                    Some(Err(e)) => return Some(Err(e)),
                    None => {}
                }
            }
        }
        if got.iter().any(Option::is_none) {
            return None;
        }
        Some(Ok(got.iter_mut().flat_map(|t| t.take().unwrap_or_default()).collect()))
    }
}

/// A task's answer once it has one; a task that ended without one reads as a failure.
fn answer<T>(task: &mut Asked<T>) -> Option<Result<T, ApiError>> {
    match task.take() {
        Some(a) => Some(a),
        None if task.failed() => Some(Err(ApiError::Unreadable("the request stopped".into()))),
        None => None,
    }
}

/// The app's music player as the shell holds it ([`AppCx::music`]).
pub struct MusicPlayer {
    session: MusicSession,
    /// Pause came while the track was starting: it pauses as soon as it plays. The session would
    /// start the track over (or let it play); Android's player opens it paused.
    hold: bool,
    /// The music gave way at this place. The TV's engine has let go of the track and says 0, so
    /// this is what shows, seeks move it (it stays paused) and Play starts there.
    held_at: Option<i64>,
    /// Play went on from `held_at`, which shows until the track plays.
    resuming: bool,
    /// The music gave way while a track was starting, which may still come and play.
    late_start: bool,
    /// Since when the track opening now has been (the engine is asked for news meanwhile).
    opening_since: Option<i64>,
    gathering: Option<Gathering>,
    gathered: Option<(u64, Gathered)>,
    tickets: u64,
    now_ms: i64,
}

impl MusicPlayer {
    /// A music player on `engine`, the TV's media pipeline.
    pub fn new(env: PlayerEnv, engine: Box<dyn PlayerEngine>) -> MusicPlayer {
        MusicPlayer {
            session: MusicSession::new(env, engine),
            hold: false,
            held_at: None,
            resuming: false,
            late_start: false,
            opening_since: None,
            gathering: None,
            gathered: None,
            tickets: 0,
            now_ms: 0,
        }
    }

    /// The session underneath.
    pub fn session(&self) -> &MusicSession {
        &self.session
    }

    /// The session underneath, to drive directly (tests), without this player's rules.
    pub fn session_mut(&mut self) -> &mut MusicSession {
        &mut self.session
    }

    /// What plays; none without a queue.
    pub fn now_playing(&self) -> Option<&NowPlaying> {
        self.session.now_playing()
    }

    /// The queue in play order.
    pub fn list(&self) -> &[ItemSummary] {
        self.session.list()
    }

    /// The sleep timer.
    pub fn sleep(&self) -> SleepState {
        self.session.sleep()
    }

    /// Changes with every change of what shows.
    pub fn version(&self) -> u64 {
        self.session.version()
    }

    /// Where the track is: the place it gave way at, while it waits there.
    pub fn position_ms(&self) -> i64 {
        self.held_at.unwrap_or_else(|| self.session.position_ms())
    }

    /// The track's length.
    pub fn duration_ms(&self) -> i64 {
        self.session.duration_ms()
    }

    /// Where what is loaded ends.
    pub fn buffered_ms(&self) -> i64 {
        self.session.buffered_ms()
    }

    /// Whether the track playing is marked as playing on the pages: it plays or is about to
    /// (Android's `playingTrack`: playing or loading).
    pub fn marks_playing(&self) -> bool {
        self.session.now_playing().is_some_and(|n| n.playing || (n.loading && n.error.is_none() && !self.given_way()))
    }

    /// Whether Play/Pause shows Pause: it plays or is about to, as Android's does (a Pause asked
    /// for during the start shows once the track is ready).
    pub fn shows_pause(&self) -> bool {
        !self.given_way() && self.session.now_playing().is_some_and(pages::shows_pause)
    }

    /// Now Playing's status line, and whether it reports a failure: no "Loading…" while it waits
    /// where it gave way.
    pub fn status(&self) -> (String, bool) {
        let n = self.session.now_playing();
        let loading = n.is_some_and(|n| n.loading) && !self.given_way();
        pages::status(n.and_then(|n| n.error), loading, &self.session.sleep())
    }

    /// Whether the track is starting: the engine hasn't played it yet.
    fn starting(&self) -> bool {
        self.session.now_playing().is_some_and(|n| n.loading && !n.playing && n.error.is_none() && !n.ended)
    }

    /// Whether the music gave way and waits at [`MusicPlayer::position_ms`].
    fn given_way(&self) -> bool {
        self.held_at.is_some() && !self.resuming
    }

    /// Plays `tracks` from `at` (shuffled with `at` first); an artist's Play still on its way gives
    /// way to it.
    pub fn play(&mut self, tracks: Vec<ItemSummary>, at: usize, shuffle: bool) {
        self.gathering = None;
        self.forget_place();
        self.session.play(tracks, at, shuffle);
    }

    /// Play/Pause: plays or pauses; during a track's start, the start opens paused (or plays
    /// again); after giving way, plays from where it was.
    pub fn toggle(&mut self) {
        if let Some(at) = self.held_at.filter(|_| !self.resuming) {
            self.resume(at);
        } else if self.starting() {
            self.hold = !self.hold;
        } else {
            self.session.toggle();
        }
    }

    /// The remote's Play: plays, never pauses.
    pub fn press_play(&mut self) {
        if let Some(at) = self.held_at.filter(|_| !self.resuming) {
            self.resume(at);
        } else if self.starting() {
            self.hold = false;
        } else if !self.session.now_playing().is_some_and(|n| n.playing) {
            self.session.toggle();
        }
    }

    /// The remote's Pause: pauses, or opens a starting track paused.
    pub fn press_pause(&mut self) {
        if self.given_way() {
            return;
        }
        if self.starting() {
            self.hold = true;
        } else {
            self.session.pause();
        }
    }

    /// The next track, playing.
    pub fn next(&mut self) {
        if self.session.now_playing().is_some_and(|n| n.index + 1 < n.count) {
            self.forget_place();
            self.session.next();
        }
    }

    /// The track before; this one from its start when past 3 s, or the first. Waiting where it
    /// gave way, it goes to the start and keeps waiting, as Android's paused player does.
    pub fn previous(&mut self) {
        let Some(index) = self.session.now_playing().map(|n| n.index) else { return };
        if let Some(at) = self.held_at {
            if !restarts_track(at, index) {
                self.forget_place();
                self.session.jump_to(index - 1);
            } else {
                self.held_at = Some(0);
                if self.resuming {
                    self.session.seek_to(0);
                }
            }
            return;
        }
        self.session.previous();
        if self.session.now_playing().is_none_or(|n| n.index != index) {
            self.hold = false;
        }
    }

    /// The track at `position` of the queue, playing.
    pub fn jump_to(&mut self, position: usize) {
        self.forget_place();
        self.session.jump_to(position);
    }

    /// Moves to `ms` of the track; where it gave way, it keeps waiting there.
    pub fn seek_to(&mut self, ms: i64) {
        let ms = ms.max(0);
        if let Some(held) = &mut self.held_at {
            *held = ms;
            if !self.resuming {
                return;
            }
        }
        self.session.seek_to(ms);
    }

    /// Shuffle on (the track playing first) or off.
    pub fn set_shuffle(&mut self, on: bool) {
        self.session.set_shuffle(on);
    }

    /// The sleep timer's next choice.
    pub fn step_sleep(&mut self) {
        self.session.step_sleep();
    }

    /// Plays on from `at` after giving way: a new session there (the session has none then).
    fn resume(&mut self, at: i64) {
        self.resuming = true;
        self.late_start = false;
        self.hold = false;
        self.session.seek_to(at);
    }

    /// Another track starts: what was kept for this one goes.
    fn forget_place(&mut self) {
        self.held_at = None;
        self.resuming = false;
        self.late_start = false;
        self.hold = false;
    }

    /// Carries out what was asked for once the engine plays: the pause asked for during the start,
    /// the place given up once playing again, and giving way again if a start from before comes.
    fn keep_wishes(&mut self) {
        let Some(n) = self.session.now_playing() else {
            self.forget_place();
            return;
        };
        let (playing, loading, failed) = (n.playing, n.loading, n.error.is_some());
        if playing && self.late_start && self.given_way() {
            let at = self.session.position_ms();
            self.session.yield_to_video();
            self.held_at = Some(at);
            self.late_start = false;
            return;
        }
        if self.hold && playing {
            self.hold = false;
            self.session.pause();
        } else if self.hold && (!loading || failed) {
            self.hold = false;
        }
        if self.resuming && playing {
            self.held_at = None;
            self.resuming = false;
        } else if self.resuming && failed {
            // Play tries again from the place.
            self.resuming = false;
        }
    }

    fn gather_on(&mut self) {
        let Some(g) = self.gathering.as_mut() else { return };
        if g.owner.strong_count() == 0 {
            self.gathering = None;
            return;
        }
        let Some(answer) = g.poll() else { return };
        let (ticket, shuffle) = (g.ticket, g.shuffle);
        self.gathering = None;
        let outcome = match answer {
            Ok(tracks) if tracks.is_empty() => Gathered::Nothing,
            Ok(tracks) => {
                let at = if shuffle { random_below(tracks.len()) } else { 0 };
                self.play(tracks, at, shuffle);
                self.session.tick(self.now_ms);
                Gathered::Played
            }
            Err(e) if e.cancelled() => return,
            Err(e) => Gathered::Failed(sentence(&e.to_string())),
        };
        self.gathered = Some((ticket, outcome));
    }

    /// When the session next needs to run, in ms of the app's clock: its own timers (reports,
    /// retries, the sleep timer), the engine while a track opens, and the end of the track playing.
    fn next_wake(&self, now_ms: i64) -> Option<i64> {
        let own = self.session.next_wake();
        let news = self.session.now_playing().and_then(|n| {
            if n.error.is_some() || n.ended || self.given_way() {
                None
            } else if n.playing {
                let duration = self.session.duration_ms();
                let left = duration - self.session.position_ms();
                (duration > 0).then(|| now_ms + left.max(0) + END_SLACK_MS)
            } else {
                self.opening_since
                    .filter(|since| now_ms - since < OPENING_POLL_FOR_MS)
                    .map(|_| now_ms + OPENING_POLL_MS)
            }
        });
        [own, news].into_iter().flatten().min().map(|t| t.max(now_ms + 1))
    }
}

impl BackgroundSession for MusicPlayer {
    fn queued(&self) -> bool {
        self.session.now_playing().is_some()
    }

    fn tick(&mut self, now: f64) -> Option<f64> {
        let now_ms = (now * 1000.0) as i64;
        self.now_ms = now_ms;
        self.session.tick(now_ms);
        self.gather_on();
        self.keep_wishes();
        self.opening_since = if self.starting() { Some(self.opening_since.unwrap_or(now_ms)) } else { None };
        self.next_wake(now_ms).map(|t| t as f64 / 1000.0)
    }

    /// The app went to the background, or a video or a live channel takes the TV's media
    /// pipeline (the TV app's `yieldToVideo`): the music pauses and lets go of its session and the
    /// pipeline; the queue and the place stay, and Play starts again there. Giving way twice
    /// keeps the first place: the engine knows none by then.
    fn pause(&mut self) {
        let Some(ended) = self.session.now_playing().map(|n| n.ended) else { return };
        if self.given_way() {
            return;
        }
        let late = self.starting();
        let at = self.held_at.unwrap_or_else(|| self.session.position_ms());
        self.session.yield_to_video();
        self.hold = false;
        self.resuming = false;
        self.late_start = late;
        // After the queue's end, Play starts the last track over: no place to keep.
        self.held_at = (!ended).then_some(at);
    }

    fn playing(&self) -> bool {
        self.marks_playing() && !self.hold
    }

    fn play_on(&mut self) {
        self.press_play();
    }

    /// The remote's media keys while music has a queue, whatever page shows but a player's: play,
    /// pause (as Stop does), play/pause, next, previous, ◀◀ and ▶▶ (10 s).
    fn media_keys(&mut self, ui: &mut Ui) -> bool {
        if self.now_playing().is_none() {
            return false;
        }
        let mut acted = false;
        if ui.take_key(Key::PlayPause) {
            self.toggle();
            acted = true;
        }
        if ui.take_key(Key::Play) {
            self.press_play();
            acted = true;
        }
        for key in [Key::Pause, Key::Stop] {
            if ui.take_key(key) {
                self.press_pause();
                acted = true;
            }
        }
        if ui.take_key(Key::Next) {
            self.next();
            acted = true;
        }
        if ui.take_key(Key::Previous) {
            self.previous();
            acted = true;
        }
        for (key, step) in [(Key::FastForward, SEEK_STEP_MS), (Key::Rewind, -SEEK_STEP_MS)] {
            if ui.take_key(key) {
                self.seek_to(pages::seek_target(self.position_ms(), step, self.duration_ms()));
                acted = true;
            }
        }
        acted
    }

    fn end(&mut self) {
        self.gathering = None;
        self.forget_place();
        self.session.stop();
    }

    /// A stop, or the user's remote on another device. The remote acts only while a session
    /// plays the track, never on a place held after the music gave way: a track or place it goes
    /// to is the one shown. While the track starts, its Pause opens it paused, as the Pause here
    /// does, and its Play wins over a Pause pressed here.
    fn server_event(&mut self, event: &ServerEvent) {
        let Some(command) = Command::of(event) else {
            self.session.server_event(event);
            return;
        };
        let starting = self.starting();
        match self.session.obey(&command) {
            RemoteAction::Seek(_) | RemoteAction::Next | RemoteAction::Previous => self.forget_place(),
            RemoteAction::Play => self.hold = false,
            // A start counts as playing: Play finds it so.
            RemoteAction::Report if command.command == Control::Play => self.hold = false,
            RemoteAction::Pause if starting => self.hold = true,
            _ => {}
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The app's music player, when music has a queue or played since the app started.
pub fn music<'a>(cx: &'a mut AppCx) -> Option<&'a mut MusicPlayer> {
    cx.music::<MusicPlayer>()
}

/// The app's music player, made the first time; none without a server to play from.
fn player<'a>(cx: &'a mut AppCx) -> Option<&'a mut MusicPlayer> {
    if cx.music::<MusicPlayer>().is_none() {
        let env = cx.player_env()?;
        let engine = cx.device().engine();
        cx.set_music(Box::new(MusicPlayer::new(env, engine)));
    }
    cx.music::<MusicPlayer>()
}

/// The track playing (or paused) and whether the pages mark it as playing.
pub fn current_track(cx: &mut AppCx) -> Option<(i64, bool)> {
    let m = music(cx)?;
    m.now_playing().map(|n| (n.track.id, m.marks_playing()))
}

/// Plays `tracks` from `at` (shuffled with `at` first) on the app's music player; false without a
/// server to play from.
pub fn play(cx: &mut AppCx, tracks: Vec<ItemSummary>, at: usize, shuffle: bool) -> bool {
    if tracks.is_empty() {
        return false;
    }
    let now_ms = (cx.now * 1000.0) as i64;
    let Some(m) = player(cx) else { return false };
    m.play(tracks, at, shuffle);
    m.session.tick(now_ms);
    true
}

/// Plays every track of artist `artist`, in their albums' order or shuffled, as soon as they come,
/// whatever page shows then, unless `owner` (held by the page that asked) is gone by then; the
/// ticket to ask [`gathered`] how it went. None without a server.
pub fn gather(cx: &mut AppCx, artist: i64, shuffle: bool, owner: &Arc<()>) -> Option<u64> {
    let api = cx.api()?.clone();
    let tasks = cx.tasks().clone();
    let albums = cx.spawn(move |api| api.children(artist));
    let m = player(cx)?;
    m.tickets += 1;
    let ticket = m.tickets;
    let owner = Arc::downgrade(owner);
    m.gathering = Some(Gathering { ticket, shuffle, owner, api, tasks, step: Step::Albums(albums) });
    Some(ticket)
}

/// How the artist's Play `ticket` went, once it did.
pub fn gathered(cx: &mut AppCx, ticket: u64) -> Option<Gathered> {
    let Some(m) = music(cx) else { return Some(Gathered::Dropped) };
    if m.gathered.as_ref().is_some_and(|(t, _)| *t == ticket) {
        return m.gathered.take().map(|(_, g)| g);
    }
    if m.gathering.as_ref().is_some_and(|g| g.ticket == ticket) { None } else { Some(Gathered::Dropped) }
}

/// A number below `n` for Shuffle's first track (xorshift on the clock: nothing here needs more).
pub fn random_below(n: usize) -> usize {
    let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0x9e37_79b9, |d| d.as_nanos())
        as u64
        | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    if n == 0 { 0 } else { (x % n as u64) as usize }
}
