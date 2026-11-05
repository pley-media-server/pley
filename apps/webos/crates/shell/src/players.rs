//! What every player shares with the app (pley-player's [`PlayerEnv`]): one report queue, so a
//! playback's reports and its end go out in order even when the next title has started; the
//! quality kept in Settings, which the player's menu sets too; Automatic's memory of a slow
//! network for each server; and subtitle sync per file and profile. A video on the USB drive
//! plays with the same reports and subtitle sync, and no server ([`local_player_env`]).
//!
//! A player's requests carry the sign-in as the profile it started under, and never another's:
//! when a remote's cast switches the TV to its profile, the players made before are fenced off
//! ([`ProfileFence`]) once their last reports have gone out, so what is left of them is dropped
//! rather than sent with the new profile's tokens.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use pley_core::{Api, ApiError, CapsOverrides, NetError, Store, Tasks, TokenSource, Tokens};
use pley_local::LocalServer;
use pley_player::autoquality::AutoMemory;
use pley_player::env::{CapsProbe, HeldQuality, QualityPrefs, SharedMemory, StoreMemory, SubtitleOffsets};
use pley_player::network::ServerNetwork;
use pley_player::{PlayerEnv, PlayerServer, Reporter};

use crate::state::AppState;

/// The quality a player starts at: Settings' choice, which choosing one in a player's menu also
/// changes. The player reads and sets it; the app saves it with the settings.
#[derive(Default)]
pub(crate) struct KeptQuality {
    /// The quality's id, and whether a player changed it since the app last saved it.
    value: Mutex<(String, bool)>,
}

impl KeptQuality {
    fn lock(&self) -> std::sync::MutexGuard<'_, (String, bool)> {
        self.value.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Settings changed it.
    pub(crate) fn set(&self, id: &str) {
        let mut value = self.lock();
        if value.0 != id {
            *value = (id.to_string(), false);
        }
    }

    /// The quality a player chose since this was last asked, for the settings.
    pub(crate) fn take_chosen(&self) -> Option<String> {
        let mut value = self.lock();
        std::mem::take(&mut value.1).then(|| value.0.clone())
    }
}

impl QualityPrefs for KeptQuality {
    fn remembered(&self) -> Option<String> {
        Some(self.lock().0.clone()).filter(|id| !id.is_empty())
    }

    fn remember(&self, id: &str) {
        let mut value = self.lock();
        if value.0 != id {
            *value = (id.to_string(), true);
        }
    }
}

/// Which profile's players may still use the sign-in: those made since it was last closed.
#[derive(Default)]
pub(crate) struct ProfileFence {
    /// Bumped as it closes. Held for reading while a player's request takes a token, so closing
    /// waits for one being taken, and none is taken after.
    closed: RwLock<u64>,
}

impl ProfileFence {
    fn now(&self) -> u64 {
        *self.closed.read().unwrap_or_else(|e| e.into_inner())
    }

    /// The players made so far send nothing more: the sign-in is about to act as another profile.
    /// Blocks while one of them takes a token.
    pub(crate) fn close(&self) {
        *self.closed.write().unwrap_or_else(|e| e.into_inner()) += 1;
    }
}

/// The sign-in as a player made under the fence's `at` sees it: the tokens until the fence
/// closes, then none, the request let go as cancelled (never shown, nothing doubted).
struct Fenced {
    tokens: Arc<Tokens>,
    fence: Arc<ProfileFence>,
    at: u64,
}

impl Fenced {
    fn open<T>(&self, f: impl FnOnce(&Tokens) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let closed = self.fence.closed.read().unwrap_or_else(|e| e.into_inner());
        if *closed != self.at {
            return Err(ApiError::Net(NetError::Cancelled));
        }
        f(&self.tokens)
    }
}

impl TokenSource for Fenced {
    fn access_token(&self) -> Result<String, ApiError> {
        self.open(TokenSource::access_token)
    }

    fn refresh(&self, stale: &str) -> Result<String, ApiError> {
        self.open(|t| TokenSource::refresh(t, stale))
    }

    fn refused_after_refresh(&self, sent: &str) -> bool {
        self.open(|t| Ok(TokenSource::refused_after_refresh(t, sent))).unwrap_or(false)
    }

    fn doubt_server(&self) {
        TokenSource::doubt_server(&*self.tokens)
    }
}

/// What the players share, kept by [`AppState`].
pub(crate) struct Players {
    reporter: Reporter,
    /// Closed as a cast switches the TV's profile.
    pub(crate) fence: Arc<ProfileFence>,
    pub(crate) quality: Arc<KeptQuality>,
    offsets: SubtitleOffsets,
    /// Automatic's memory, for the server it belongs to.
    memory: Option<(String, SharedMemory)>,
}

impl Players {
    pub(crate) fn new(store: &Store, quality: &str) -> Players {
        let kept = KeptQuality::default();
        kept.set(quality);
        Players {
            reporter: Reporter::new(),
            fence: Arc::default(),
            quality: Arc::new(kept),
            offsets: SubtitleOffsets::load(store.clone()),
            memory: None,
        }
    }

    /// Automatic's memory of `server`, read from the store the first time.
    fn memory(&mut self, server: &str, store: &Store, tasks: &Tasks) -> SharedMemory {
        match &self.memory {
            Some((s, memory)) if s == server => memory.clone(),
            _ => {
                let memory: Arc<Mutex<dyn AutoMemory>> =
                    Arc::new(Mutex::new(StoreMemory::new(store.clone(), server, Some(tasks.clone()))));
                self.memory = Some((server.to_string(), memory.clone()));
                memory
            }
        }
    }

    /// The report queue, for what reads the reports back.
    pub(crate) fn reporter(&self) -> Reporter {
        self.reporter.clone()
    }

    /// Waits up to `timeout` for the reports queued so far to go out (on leaving the app).
    pub(crate) fn flush(&self, timeout: std::time::Duration) -> bool {
        self.reporter.flush(timeout)
    }
}

/// The environment of a player made now: the current server and sign-in (fenced, for the
/// profile watching only), the profile watching, the TV's capabilities (the probe's answer when
/// there is one, else asked on the player's task) and the settings. `None` without a server.
pub(crate) fn player_env(state: &mut AppState) -> Option<PlayerEnv> {
    let fence = state.players_mut().fence.clone();
    let api = state.api()?;
    let tokens: Arc<dyn TokenSource> = match state.tokens() {
        Some(tokens) => Arc::new(Fenced { tokens: tokens.clone(), at: fence.now(), fence }),
        None => api.tokens().clone(),
    };
    let server: Arc<dyn PlayerServer> = Arc::new(Api::new(api.http().clone(), api.server(), tokens));
    let address = server.server().to_string();
    let tasks = state.tasks().clone();
    let store = state.store().clone();
    let client_id = state.install_id().to_string();
    let caps = caps_probe(state);
    let autoplay_next = state.settings().autoplay_next;
    let profile = state.profile();
    let players = state.players_mut();
    let mut env = PlayerEnv::new(server.clone(), tasks.clone(), players.reporter.clone(), caps, client_id.clone());
    env.profile = profile;
    env.offsets = players.offsets.clone();
    env.quality = players.quality.clone();
    env.auto_memory = Some(players.memory(&address, &store, &tasks));
    env.network = Some(Arc::new(ServerNetwork::new(server, client_id)));
    env.autoplay_next = autoplay_next;
    Some(env)
}

/// The TV's capabilities for a player: the probe's answer when there is one, else asked on the
/// player's task.
fn caps_probe(state: &AppState) -> CapsProbe {
    let device = state.device().clone();
    let overrides = CapsOverrides::from_settings(state.settings());
    let probed = state.caps().cloned();
    Arc::new(move || probed.clone().or_else(|| Some(device.caps(&overrides))))
}

/// The environment of a player for `file` on the drive mounted at `root`, with no server behind
/// it: pley-local's [`LocalServer`] answers as a server would for direct play. The file plays as
/// it is (Original, no Automatic and no network memory), its address is a plain `file://` one,
/// and nothing plays next by itself.
pub(crate) fn local_player_env(state: &mut AppState, file: PathBuf, root: PathBuf) -> PlayerEnv {
    let caps = caps_probe(state);
    let server = Arc::new(LocalServer::new(file, root, state.store().clone(), caps.clone()));
    let tasks = state.tasks().clone();
    let client_id = state.install_id().to_string();
    let profile = state.profile();
    let players = state.players_mut();
    let mut env = PlayerEnv::new(server, tasks, players.reporter.clone(), caps, client_id);
    env.profile = profile;
    env.offsets = players.offsets.clone();
    env.keyed_urls = false;
    env.quality = Arc::new(HeldQuality::with("original"));
    env.autoplay_next = false;
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quality_chosen_in_the_player_goes_back_to_the_settings_once() {
        let kept = KeptQuality::default();
        kept.set("auto");
        assert_eq!(kept.remembered().as_deref(), Some("auto"));
        assert_eq!(kept.take_chosen(), None);
        kept.remember("8m");
        assert_eq!(kept.remembered().as_deref(), Some("8m"));
        assert_eq!(kept.take_chosen().as_deref(), Some("8m"));
        assert_eq!(kept.take_chosen(), None);
        // Settings choosing one isn't sent back to them.
        kept.set("original");
        assert_eq!(kept.take_chosen(), None);
        assert_eq!(kept.remembered().as_deref(), Some("original"));
    }
}
