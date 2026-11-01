//! What every player shares with the app (pley-player's [`PlayerEnv`]): one report queue, so a
//! playback's reports and its end go out in order even when the next title has started; the
//! quality kept in Settings, which the player's menu sets too; Automatic's memory of a slow
//! network for each server; and subtitle sync per file and profile.

use std::sync::{Arc, Mutex};

use pley_core::{CapsOverrides, Store, Tasks};
use pley_player::autoquality::AutoMemory;
use pley_player::env::{QualityPrefs, SharedMemory, StoreMemory, SubtitleOffsets};
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

/// What the players share, kept by [`AppState`].
pub(crate) struct Players {
    reporter: Reporter,
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

    /// Waits up to `timeout` for the reports queued so far to go out (on leaving the app).
    pub(crate) fn flush(&self, timeout: std::time::Duration) -> bool {
        self.reporter.flush(timeout)
    }
}

/// The environment of a player made now: the current server and sign-in, the profile watching,
/// the TV's capabilities (the probe's answer when there is one, else asked on the player's task)
/// and the settings. `None` without a server.
pub(crate) fn player_env(state: &mut AppState) -> Option<PlayerEnv> {
    let server: Arc<dyn PlayerServer> = Arc::new(state.api()?.clone());
    let address = server.server().to_string();
    let tasks = state.tasks().clone();
    let store = state.store().clone();
    let client_id = state.install_id().to_string();
    let device = state.device().clone();
    let overrides = CapsOverrides::from_settings(state.settings());
    let probed = state.caps().cloned();
    let caps = Arc::new(move || probed.clone().or_else(|| Some(device.caps(&overrides))));
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
