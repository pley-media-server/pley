//! Casts from the user's remotes on their other devices (docs/API.md, "Casting"): the TV plays
//! what one sends it as its own Play on that page would, in place of what plays, and says where
//! it came from over the player for 4 s.
//!
//! The event stream names the TV a receiver ([`crate::AppState::listen`]); a `cast` for it is
//! taken while the app shows and is signed in, past the pages of getting in. What it names is
//! found on the task pool ([`resolve`]): a title's kind, a show's next episode, the tracks of an
//! album, an artist or a playlist. A newer cast replaces one still being found.
//!
//! A remote on another profile of the household casts with its profile and a ticket: the TV ends
//! what plays, lets its last reports go out as the profile it played as (5 s at most), switches
//! to the remote's profile with the ticket ([`AppState::switch_for_cast`]), then finds and plays
//! the cast as the new profile. The switch restarts the event stream, so the cast waits here, not
//! on the stream, and plays once; any other cast heard until the switch is done is dropped. A
//! switch the server refused plays nothing and says so; a cast from another profile without a
//! ticket is ignored.

use std::time::Duration;

use pley_core::auth::{AuthError, SwitchRefusal};
use pley_core::model::{ItemKind, ItemSummary, PlaylistKind, ServerEvent, SwitchTicket};
use pley_core::{Api, ApiError, Promise};

use crate::cx::NavCommand;
use crate::route::{PlayQueue, PlayRequest, Route};
use crate::state::AppState;

/// How long "Playing from …" shows, as long as a player's title banner.
pub const CAST_NOTICE_SECONDS: f64 = 4.0;

/// How long a switch for a cast waits for the last reports of what played to go out as the
/// profile it played as; what hasn't gone out by then is dropped.
pub const LAST_REPORTS_WAIT: Duration = Duration::from_secs(5);

/// What the TV says of a cast that names nothing to play: a show without episodes, an empty album
/// or playlist.
pub const NOTHING_TO_PLAY: &str = "Nothing to play";

/// What the TV says when it couldn't switch to the profile of the remote that cast, as every app
/// says it.
pub const CANT_SWITCH: &str = "Couldn't switch to the remote's profile";

/// What a remote cast to this TV.
#[derive(Clone, Debug, PartialEq)]
pub struct Cast {
    pub item_id: Option<i64>,
    pub playlist_id: Option<i64>,
    pub channel_id: Option<i64>,
    /// Where a title starts, in ms.
    pub start_ms: Option<i64>,
    pub shuffle: bool,
    /// The tracks picked on the remote's page for a movie or an episode, by stream index
    /// (subtitles -1: none), for this play only.
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
    /// The remote's device, as the server names it.
    pub from: Option<String>,
    /// From another profile's remote: the switch to its profile that comes first.
    pub switch: Option<CastSwitch>,
}

/// The profile of the remote that cast, which the TV switches to before it plays, and the cast's
/// ticket for it.
#[derive(Clone, Debug, PartialEq)]
pub struct CastSwitch {
    pub profile: i64,
    pub ticket: SwitchTicket,
}

impl Cast {
    /// The cast `event` carries when it is for receiver `receiver` (this TV's id), acting as
    /// profile `on`, and names something to play: the profile's streams all hear every cast, and
    /// one for another of its devices is that device's. One from another profile's remote
    /// (`profile_id` other than `on`) switches to it first with its ticket; without one it is
    /// ignored, as the TV never plays as a profile it isn't on.
    pub fn for_receiver(event: &ServerEvent, receiver: &str, on: Option<i64>) -> Option<Cast> {
        match event {
            ServerEvent::Cast {
                receiver_id,
                item_id,
                playlist_id,
                channel_id,
                start_ms,
                shuffle,
                audio_index,
                subtitle_index,
                from,
                profile_id,
                switch_ticket,
            } if receiver_id == receiver && (item_id.is_some() || playlist_id.is_some() || channel_id.is_some()) => {
                let switch = match (profile_id, switch_ticket) {
                    (Some(profile), _) if Some(*profile) == on => None,
                    (Some(profile), Some(ticket)) => Some(CastSwitch { profile: *profile, ticket: ticket.clone() }),
                    (Some(_), None) => {
                        log::info!("a cast from another profile came without a ticket to switch to it");
                        return None;
                    }
                    (None, _) => None,
                };
                Some(Cast {
                    item_id: *item_id,
                    playlist_id: *playlist_id,
                    channel_id: *channel_id,
                    start_ms: *start_ms,
                    shuffle: *shuffle,
                    audio_index: *audio_index,
                    subtitle_index: *subtitle_index,
                    from: from.clone(),
                    switch,
                })
            }
            _ => None,
        }
    }
}

/// What the TV says of a switch for a cast the server refused, if anything: signed out, the
/// shell shows signing in again.
pub fn switch_refused(e: &AuthError) -> Option<String> {
    match SwitchRefusal::of(e) {
        SwitchRefusal::SignedOut => None,
        _ => Some(CANT_SWITCH.into()),
    }
}

/// Tracks a remote cast, for the app's music player ([`crate::Registry::cast_music`]).
#[derive(Clone, Debug, PartialEq)]
pub struct CastTracks {
    /// In their order.
    pub tracks: Vec<ItemSummary>,
    /// The track to start at; shuffled, it plays first and the rest in a random order.
    pub at: usize,
    pub shuffle: bool,
    /// Where in that track, in ms.
    pub start_ms: i64,
}

/// What a cast plays, once found.
#[derive(Clone, Debug, PartialEq)]
pub enum CastPlay {
    /// A movie or an episode in the video player.
    Video(PlayRequest),
    /// Tracks in the music player.
    Music(CastTracks),
    /// A Live TV channel, live.
    Channel(i64),
    /// Nothing to play.
    Nothing,
}

/// "Playing from {from}", or from another device when the server couldn't name it.
pub fn notice(from: Option<&str>) -> String {
    match from.map(str::trim).filter(|f| !f.is_empty()) {
        Some(from) => format!("Playing from {from}"),
        None => "Playing from another device".into(),
    }
}

/// Finds what `cast` plays, as the app's own Play on that page does: a movie or an episode
/// itself, from `start_ms` when given, else where Play resumes, with the tracks sent (a track its
/// file doesn't have leaves the saved choices); a show or a season its next episode (On Deck)
/// where Play resumes it; a track its album from that track; an album, an artist
/// (album after album) or an audio playlist its tracks, shuffled from a random one with
/// `shuffle`; a video playlist its first entry (a random one shuffled), the playlist going on as
/// its queue; a channel live. `seed` picks the random start. Blocking: it runs on the task pool.
pub fn resolve(api: &Api, cast: &Cast, seed: f64) -> Result<CastPlay, ApiError> {
    let start_ms = cast.start_ms.map(|ms| ms.max(0));
    if let Some(channel) = cast.channel_id {
        return Ok(CastPlay::Channel(channel));
    }
    if let Some(id) = cast.playlist_id {
        let playlist = api.playlist(id)?;
        let entries: Vec<ItemSummary> = playlist.entries.into_iter().map(|e| e.item).collect();
        if playlist.playlist.kind == PlaylistKind::Audio {
            return Ok(queue(tracks_of(entries), cast.shuffle, seed));
        }
        if entries.is_empty() {
            return Ok(CastPlay::Nothing);
        }
        let at = if cast.shuffle { random_index(seed, entries.len()) } else { 0 };
        return Ok(CastPlay::Video(PlayRequest {
            queue: Some(PlayQueue::Playlist(id)),
            ..PlayRequest::item(entries[at].id)
        }));
    }
    let Some(id) = cast.item_id else { return Ok(CastPlay::Nothing) };
    let item = api.item(id)?;
    Ok(match item.kind {
        ItemKind::Movie | ItemKind::Episode | ItemKind::Extra => CastPlay::Video(PlayRequest {
            audio_index: cast.audio_index,
            subtitle_index: cast.subtitle_index,
            ..video(id, start_ms)
        }),
        // A place named with a show isn't its next episode's: that starts where Play resumes it.
        ItemKind::Show | ItemKind::Season => match api.on_deck(id)? {
            Some(episode) => CastPlay::Video(video(episode.id, None)),
            None => CastPlay::Nothing,
        },
        ItemKind::Track => {
            // Its album from it, as the album's line plays it; a track without one alone.
            let album = match item.parent_id {
                Some(album) => tracks_of(api.children(album)?.items),
                None => Vec::new(),
            };
            let (tracks, at) = match album.iter().position(|t| t.id == id) {
                Some(at) => (album, at),
                None => (vec![item.summary.clone()], 0),
            };
            CastPlay::Music(CastTracks { tracks, at, shuffle: false, start_ms: start_ms.unwrap_or(0) })
        }
        ItemKind::Album => queue(tracks_of(api.children(id)?.items), cast.shuffle, seed),
        ItemKind::Artist => {
            let albums = api.children(id)?.items;
            let mut tracks = Vec::new();
            for album in albums.iter().filter(|a| a.kind == ItemKind::Album) {
                tracks.extend(tracks_of(api.children(album.id)?.items));
            }
            queue(tracks, cast.shuffle, seed)
        }
        ItemKind::Unknown => CastPlay::Nothing,
    })
}

fn video(item_id: i64, start_ms: Option<i64>) -> PlayRequest {
    PlayRequest { start_ms: start_ms.map(|ms| ms as u64), ..PlayRequest::item(item_id) }
}

/// The tracks among `items`.
fn tracks_of(items: Vec<ItemSummary>) -> Vec<ItemSummary> {
    items.into_iter().filter(|t| t.kind == ItemKind::Track).collect()
}

/// `tracks` from the first, or shuffled from a random one as an album's Shuffle does.
fn queue(tracks: Vec<ItemSummary>, shuffle: bool, seed: f64) -> CastPlay {
    if tracks.is_empty() {
        return CastPlay::Nothing;
    }
    let at = if shuffle { random_index(seed, tracks.len()) } else { 0 };
    CastPlay::Music(CastTracks { tracks, at, shuffle, start_ms: 0 })
}

/// A place below `len` from `seed` (the clock): nothing here needs more than a well-mixed number.
fn random_index(seed: f64, len: usize) -> usize {
    let mut x = (seed * 1_000_003.0) as u64 ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x % len.max(1) as u64) as usize
}

/// How the page of what a cast plays opens over `top`, the page on top: in place of a player,
/// whose playback it replaces (a player kept under another would play on), else over the page;
/// Now Playing already on top stays.
pub fn opening(top: Option<&Route>, route: Route) -> Option<NavCommand> {
    match top {
        Some(Route::NowPlaying) if route == Route::NowPlaying => None,
        Some(Route::Player(_) | Route::LivePlayer { .. }) => Some(NavCommand::Replace(route)),
        _ => Some(NavCommand::Push(route)),
    }
}

/// The cast being switched for or found: one at a time, a newer one replacing it.
#[derive(Default)]
pub struct Casts {
    pending: Option<Pending>,
}

struct Pending {
    /// The server, sign-in and profile it was heard under.
    generation: u64,
    stage: Stage,
}

enum Stage {
    /// The TV switches to the profile of the remote that cast (its last reports out first); `cast`
    /// is found once it has, unless it was let go (`play` false).
    Switching { profile: i64, cast: Cast, play: bool, answer: Promise<Result<(), AuthError>> },
    /// What the cast plays is being found.
    Finding { from: Option<String>, answer: Promise<Result<CastPlay, ApiError>> },
}

/// What a cast being switched for or found came to.
#[derive(Debug)]
pub enum CastNews {
    /// The TV acts as `profile` on the server now, for a cast: the app follows, and then finds
    /// `cast`, unless it was let go.
    Switched { profile: i64, cast: Option<Cast> },
    /// The server didn't switch the TV: the cast plays nothing.
    Refused(AuthError),
    /// Where the cast came from, and what to play or why not.
    Found { from: Option<String>, found: Result<CastPlay, ApiError> },
}

impl Casts {
    /// Starts on `cast`: a switch to its remote's profile first, when it comes from another (the
    /// shell has ended what played), else finding what it plays, in place of a cast still being
    /// found (whose requests are let go). A channel needs nothing found: its player tunes it.
    ///
    /// A switch under way goes on whatever comes next, as the server may have made it already, and
    /// any cast heard until it is done is dropped, the new profile's own included.
    pub fn start(&mut self, mut cast: Cast, state: &AppState, now: f64) {
        if self.switching() {
            log::info!("a remote cast to this TV as it switched profiles for another cast: dropped");
            return;
        }
        let generation = state.generation();
        if let Some(switch) = cast.switch.take() {
            let answer = state.switch_for_cast(switch.profile, switch.ticket);
            let stage = Stage::Switching { profile: switch.profile, cast, play: true, answer };
            self.pending = Some(Pending { generation, stage });
            return;
        }
        let answer = match cast.channel_id {
            Some(channel) => Promise::resolved(Ok(CastPlay::Channel(channel))),
            None => {
                let asked = cast.clone();
                state.spawn(move |api| resolve(api, &asked, now))
            }
        };
        self.pending = Some(Pending { generation, stage: Stage::Finding { from: cast.from, answer } });
    }

    /// Lets go of the cast being found: the app was hidden. A switch under way still ends in
    /// [`CastNews::Switched`], without the cast.
    pub fn cancel(&mut self) {
        match &mut self.pending {
            Some(Pending { stage: Stage::Switching { play, .. }, .. }) => *play = false,
            _ => self.pending = None,
        }
    }

    /// Whether a cast is being switched for or found.
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether the TV is switching profiles for a cast.
    pub fn switching(&self) -> bool {
        matches!(self.pending, Some(Pending { stage: Stage::Switching { .. }, .. }))
    }

    /// What the cast being switched for or found came to, once it has. One heard under another
    /// server, sign-in or profile than `generation`'s is dropped.
    pub fn take(&mut self, generation: u64) -> Option<CastNews> {
        let Pending { generation: heard, stage } = self.pending.take()?;
        let news = match stage {
            Stage::Switching { profile, cast, play, mut answer } => match answer.take() {
                Some(Ok(())) => CastNews::Switched { profile, cast: play.then_some(cast) },
                Some(Err(e)) => CastNews::Refused(e),
                None if answer.failed() => CastNews::Refused(AuthError::Failed("the request stopped".into())),
                None => {
                    let stage = Stage::Switching { profile, cast, play, answer };
                    self.pending = Some(Pending { generation: heard, stage });
                    return None;
                }
            },
            Stage::Finding { from, mut answer } => match answer.take() {
                Some(found) => CastNews::Found { from, found },
                None if answer.failed() => {
                    CastNews::Found { from, found: Err(ApiError::Unreadable("the request stopped".into())) }
                }
                None => {
                    self.pending = Some(Pending { generation: heard, stage: Stage::Finding { from, answer } });
                    return None;
                }
            },
        };
        (heard == generation).then_some(news)
    }
}

#[cfg(test)]
mod tests {
    use pley_core::model::LiveCategory;

    use super::*;

    fn cast_event(receiver: &str) -> ServerEvent {
        cast_from(receiver, None, None)
    }

    /// A cast to `receiver` from the remote of `profile` (none: the server's before profiles),
    /// with `ticket`.
    fn cast_from(receiver: &str, profile: Option<i64>, ticket: Option<&str>) -> ServerEvent {
        ServerEvent::Cast {
            receiver_id: receiver.into(),
            item_id: Some(12),
            playlist_id: None,
            channel_id: None,
            start_ms: Some(60_000),
            shuffle: false,
            audio_index: Some(2),
            subtitle_index: Some(-1),
            from: Some("Chrome on Windows".into()),
            profile_id: profile,
            switch_ticket: ticket.map(|t| SwitchTicket(t.into())),
        }
    }

    #[test]
    fn a_cast_is_this_tvs_only_when_it_names_its_receiver() {
        let cast = Cast::for_receiver(&cast_event("webos-1"), "webos-1", Some(1)).expect("this TV's");
        assert_eq!(
            (cast.item_id, cast.start_ms, cast.audio_index, cast.subtitle_index, cast.from.as_deref()),
            (Some(12), Some(60_000), Some(2), Some(-1), Some("Chrome on Windows"))
        );
        assert_eq!(cast.switch, None, "the profile's own remote");
        assert_eq!(Cast::for_receiver(&cast_event("webos-2"), "webos-1", Some(1)), None, "another device's");
        assert_eq!(Cast::for_receiver(&ServerEvent::SessionsChanged, "webos-1", Some(1)), None);
        // One naming nothing, which the server never sends, has nothing to play.
        let nothing = ServerEvent::Cast {
            receiver_id: "webos-1".into(),
            item_id: None,
            playlist_id: None,
            channel_id: None,
            start_ms: None,
            shuffle: true,
            audio_index: None,
            subtitle_index: None,
            from: None,
            profile_id: None,
            switch_ticket: None,
        };
        assert_eq!(Cast::for_receiver(&nothing, "webos-1", Some(1)), None);
    }

    #[test]
    fn a_cast_from_another_profile_switches_to_it_with_its_ticket_or_is_ignored() {
        let cast = Cast::for_receiver(&cast_from("webos-1", Some(4), Some("t1")), "webos-1", Some(1)).unwrap();
        assert_eq!(cast.switch, Some(CastSwitch { profile: 4, ticket: SwitchTicket("t1".into()) }));
        assert_eq!(cast.item_id, Some(12));
        // Without a ticket the TV can't switch, and never plays as a profile it isn't on.
        assert_eq!(Cast::for_receiver(&cast_from("webos-1", Some(4), None), "webos-1", Some(1)), None);
        // On the remote's profile already, it plays as it is; while the TV doesn't know who it
        // is, the ticket says.
        let same = Cast::for_receiver(&cast_from("webos-1", Some(4), Some("t1")), "webos-1", Some(4)).unwrap();
        assert_eq!(same.switch, None);
        let unknown = Cast::for_receiver(&cast_from("webos-1", Some(4), Some("t1")), "webos-1", None).unwrap();
        assert_eq!(unknown.switch.map(|s| s.profile), Some(4));
        assert_eq!(Cast::for_receiver(&cast_from("webos-2", Some(4), Some("t1")), "webos-1", Some(1)), None);
    }

    #[test]
    fn a_refused_switch_says_why_unless_the_sign_in_ended() {
        use pley_core::auth::SignOutReason;
        use pley_core::model::OAuthError;

        let refused = |reason: &str| AuthError::Refused {
            status: 400,
            oauth: Some(OAuthError {
                error: "invalid_grant".into(),
                error_description: None,
                reason: Some(reason.into()),
            }),
        };
        assert_eq!(switch_refused(&refused("ticket_spent")).as_deref(), Some(CANT_SWITCH));
        assert_eq!(switch_refused(&refused("not_allowed")).as_deref(), Some(CANT_SWITCH));
        assert_eq!(switch_refused(&AuthError::SignedOut(SignOutReason::Ended)), None);
    }

    #[test]
    fn the_notice_says_where_the_cast_came_from() {
        assert_eq!(notice(Some("Chrome on Windows")), "Playing from Chrome on Windows");
        assert_eq!(notice(None), "Playing from another device");
        assert_eq!(notice(Some("  ")), "Playing from another device");
    }

    #[test]
    fn a_cast_replaces_a_player_on_top_and_opens_over_any_other_page() {
        let film = Route::Player(PlayRequest::item(12));
        let channel = Route::LivePlayer { channel_id: 4, category: LiveCategory::All, pressed_at: 0.0 };
        assert_eq!(opening(Some(&Route::Home), film.clone()), Some(NavCommand::Push(film.clone())));
        assert_eq!(opening(None, film.clone()), Some(NavCommand::Push(film.clone())));
        assert_eq!(opening(Some(&channel), film.clone()), Some(NavCommand::Replace(film.clone())));
        assert_eq!(opening(Some(&film), channel.clone()), Some(NavCommand::Replace(channel)));
        assert_eq!(opening(Some(&film), Route::NowPlaying), Some(NavCommand::Replace(Route::NowPlaying)));
        assert_eq!(opening(Some(&Route::Item { id: 3 }), Route::NowPlaying), Some(NavCommand::Push(Route::NowPlaying)));
        assert_eq!(opening(Some(&Route::NowPlaying), Route::NowPlaying), None, "Now Playing shows already");
    }

    #[test]
    fn a_random_start_stays_within_the_list() {
        for seed in [0.0, 1.5, 1_730_000_000.123, 99.99] {
            assert!(random_index(seed, 7) < 7);
        }
        assert_eq!(random_index(5.0, 1), 0);
        assert_eq!(random_index(5.0, 0), 0);
    }
}
