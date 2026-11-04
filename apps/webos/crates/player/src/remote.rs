//! The user's remote, on another of their devices (docs/API.md, "Remote control"): what each
//! player here obeys, and what it does with a command. The TV obeys and shows no remote of its
//! own. The rules are the other apps' players' (the Apple apps' `Remote.swift`), pinned by the
//! TV apps' shared fixture (packages/tv/test/fixtures/remote.json); a video's tracks, which the
//! fixture doesn't have, by the tests here.

use pley_core::model::{Control, ServerEvent};

/// "Previous" this far into a track starts it again, as the music's own button does.
pub const RESTART_AFTER_MS: i64 = 3_000;

const VIDEO_CONTROLS: [Control; 4] = [Control::Play, Control::Pause, Control::Seek, Control::Skip];
/// What the music obeys: everything its buttons do.
pub const MUSIC_CONTROLS: [Control; 6] =
    [Control::Play, Control::Pause, Control::Seek, Control::Skip, Control::Next, Control::Previous];
/// Live is live here: no pause or seek, only channel up and down.
const LIVE_CONTROLS: [Control; 2] = [Control::Next, Control::Previous];

/// What a video obeys: next only when something may come after it.
pub fn video_controls(has_next: bool) -> Vec<Control> {
    let mut controls = VIDEO_CONTROLS.to_vec();
    if has_next {
        controls.push(Control::Next);
    }
    controls
}

/// What a video obeys with its tracks: [`video_controls`], then `audio` and `subtitles` where its
/// file has tracks of that kind, which the remote picks among as the player's own menu does.
pub fn video_controls_with_tracks(has_next: bool, audio: bool, subtitles: bool) -> Vec<Control> {
    let mut controls = video_controls(has_next);
    if audio {
        controls.push(Control::Audio);
    }
    if subtitles {
        controls.push(Control::Subtitles);
    }
    controls
}

/// What a live channel obeys: channel up and down when the player has an order to go through,
/// else nothing but Stop.
pub fn live_controls(has_order: bool) -> Vec<Control> {
    if has_order { LIVE_CONTROLS.to_vec() } else { Vec::new() }
}

/// A command of the remote, as the server's `player_command` says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command<'a> {
    /// The session whose player it is for.
    pub session_id: &'a str,
    pub command: Control,
    /// Where to, for a seek.
    pub position_ms: Option<i64>,
    /// How far, for a skip (back when negative).
    pub offset_ms: Option<i64>,
    /// Which track, for another audio track or other subtitles (-1: none).
    pub stream_index: Option<i64>,
}

impl Command<'_> {
    /// The command `event` carries, if it is one.
    pub fn of(event: &ServerEvent) -> Option<Command<'_>> {
        match event {
            ServerEvent::PlayerCommand { session_id, command, position_ms, offset_ms, stream_index } => Some(Command {
                session_id,
                command: *command,
                position_ms: *position_ms,
                offset_ms: *offset_ms,
                stream_index: *stream_index,
            }),
            _ => None,
        }
    }
}

/// Which player a command finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteKind {
    Video,
    /// `index`: the track's place in the music's play order.
    Music {
        index: usize,
    },
    /// `has_order`: the player goes through channels, with channel up and down.
    Live {
        has_order: bool,
    },
}

/// What a player is doing, as much as a command needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemotePlayer<'a> {
    pub kind: RemoteKind,
    /// The session it plays now; none while it has none.
    pub session: Option<&'a str>,
    pub playing: bool,
    pub position_ms: i64,
    /// None while the length isn't known.
    pub duration_ms: Option<i64>,
    /// Something may come after it (an episode, the queue's next).
    pub has_next: bool,
}

impl<'a> RemotePlayer<'a> {
    /// A live player: only its session and whether it goes through channels count.
    pub fn live(session: Option<&'a str>, has_order: bool) -> RemotePlayer<'a> {
        RemotePlayer {
            kind: RemoteKind::Live { has_order },
            session,
            playing: true,
            position_ms: 0,
            duration_ms: None,
            has_next: false,
        }
    }
}

/// What the player does with a command. A play, pause or seek tells the server where it is at
/// once, so the remote sees it (a pause reports as any pause does); another track or channel
/// tells it with its own session. A play while playing, or a pause while paused, changes nothing
/// but says where it is again ([`RemoteAction::Report`]), so a remote a little behind settles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteAction {
    None,
    Report,
    Play,
    Pause,
    Seek(i64),
    Next,
    Previous,
    /// Channel up (1) or down (-1).
    Channel(i64),
    /// A video's audio track, by stream index, picked as its menu picks it.
    Audio(i64),
    /// A video's subtitles, by stream index; none turns them off.
    Subtitles(Option<i64>),
}

/// The rule every receiver follows: a command for another session is someone else's player's;
/// play while playing and pause while paused say again what it does; a seek stays within the
/// title and a skip counts from where it is; next with nothing next, or previous on a video, does
/// nothing; previous on music starts the track again past 3 s or on the first track, else plays
/// the one before; a live channel's next and previous are channel up and down; a video's audio
/// and subtitles are its tracks of that stream index (subtitles -1: off), which music and live
/// channels don't have. A command the player didn't declare does nothing.
pub fn remote_action(p: &RemotePlayer, c: &Command) -> RemoteAction {
    if p.session != Some(c.session_id) {
        return RemoteAction::None;
    }
    let index = match p.kind {
        RemoteKind::Live { has_order } => {
            return match c.command {
                Control::Next if has_order => RemoteAction::Channel(1),
                Control::Previous if has_order => RemoteAction::Channel(-1),
                _ => RemoteAction::None,
            };
        }
        RemoteKind::Video => None,
        RemoteKind::Music { index } => Some(index),
    };
    match c.command {
        Control::Play if p.playing => RemoteAction::Report,
        Control::Play => RemoteAction::Play,
        Control::Pause if p.playing => RemoteAction::Pause,
        Control::Pause => RemoteAction::Report,
        Control::Seek => c.position_ms.map_or(RemoteAction::None, |at| RemoteAction::Seek(within(at, p.duration_ms))),
        Control::Skip => c.offset_ms.map_or(RemoteAction::None, |by| {
            RemoteAction::Seek(within(p.position_ms.saturating_add(by), p.duration_ms))
        }),
        Control::Next if p.has_next => RemoteAction::Next,
        Control::Next => RemoteAction::None,
        Control::Previous => match index {
            None => RemoteAction::None,
            Some(i) if restarts_track(p.position_ms, i) => RemoteAction::Seek(0),
            Some(_) => RemoteAction::Previous,
        },
        // Music has no tracks to pick.
        Control::Audio | Control::Subtitles if index.is_some() => RemoteAction::None,
        Control::Audio => c.stream_index.map_or(RemoteAction::None, RemoteAction::Audio),
        Control::Subtitles => {
            c.stream_index.map_or(RemoteAction::None, |i| RemoteAction::Subtitles((i >= 0).then_some(i)))
        }
        // A command a newer server knows.
        Control::Unknown => RemoteAction::None,
    }
}

/// Previous starts the track again (rather than playing the one before) past 3 s, or on the
/// first track.
pub fn restarts_track(position_ms: i64, index: usize) -> bool {
    position_ms > RESTART_AFTER_MS || index == 0
}

/// From the start to the end; past the start only while the length isn't known.
fn within(ms: i64, duration_ms: Option<i64>) -> i64 {
    let from_start = ms.max(0);
    match duration_ms.filter(|d| *d > 0) {
        Some(d) => from_start.min(d),
        None => from_start,
    }
}

/// The place `by` from `at` in an order of `total` channels, round its ends; the first when the
/// channel isn't in it (tuned by its number, say); none when it is empty.
pub fn channel_step(at: Option<i64>, by: i64, total: i64) -> Option<i64> {
    if total <= 0 {
        return None;
    }
    Some(at.map_or(0, |at| (at + by).rem_euclid(total)))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    /// packages/tv/test/fixtures/remote.json, among the TV apps' shared fixtures: what each
    /// player obeys, and what it does with each command.
    const SHARED: &str = include_str!("../../../../../packages/tv/test/fixtures/remote.json");

    fn fixture(group: &str) -> Vec<Value> {
        let fixture: Value = serde_json::from_str(SHARED).unwrap();
        let cases = fixture[group].as_array().unwrap().clone();
        assert!(!cases.is_empty(), "{group}");
        cases
    }

    fn command(c: &Value) -> Command<'_> {
        Command {
            session_id: c["session_id"].as_str().unwrap(),
            command: Control::from_wire(c["command"].as_str().unwrap()),
            position_ms: c["position_ms"].as_i64(),
            offset_ms: c["offset_ms"].as_i64(),
            stream_index: c["stream_index"].as_i64(),
        }
    }

    /// An action as the fixture writes it: what the player does, and whether its session reports
    /// at once (a play, pause or seek does, and a play or pause that finds it so already); a
    /// channel as the one it tunes.
    fn written(a: RemoteAction, channel: Option<i64>) -> Value {
        match a {
            RemoteAction::None => json!({ "act": "none", "report": false }),
            RemoteAction::Report => json!({ "act": "none", "report": true }),
            RemoteAction::Play => json!({ "act": "play", "report": true }),
            RemoteAction::Pause => json!({ "act": "pause", "report": true }),
            RemoteAction::Seek(at) => json!({ "act": "seek", "position_ms": at, "report": true }),
            RemoteAction::Next => json!({ "act": "next", "report": false }),
            RemoteAction::Previous => json!({ "act": "previous", "report": false }),
            RemoteAction::Channel(_) => match channel {
                Some(c) => json!({ "act": "channel", "channel": c, "report": false }),
                None => json!({ "act": "none", "report": false }),
            },
            // The shared fixture has no tracks yet: the tests below have them.
            RemoteAction::Audio(i) => json!({ "act": "audio", "stream_index": i }),
            RemoteAction::Subtitles(i) => json!({ "act": "subtitles", "stream_index": i.unwrap_or(-1) }),
        }
    }

    fn played(kind: RemoteKind, input: &Value) -> Value {
        let p = &input["player"];
        let player = RemotePlayer {
            kind,
            session: p["session_id"].as_str(),
            playing: p["state"] == "playing",
            position_ms: p["position_ms"].as_i64().unwrap(),
            duration_ms: p["duration_ms"].as_i64(),
            has_next: p["has_next"].as_bool().unwrap(),
        };
        written(remote_action(&player, &command(&input["command"])), None)
    }

    #[test]
    fn what_each_player_obeys_follows_the_shared_fixture() {
        for case in fixture("controls") {
            let input = &case["in"];
            let declared = match input["kind"].as_str().unwrap() {
                "video" => video_controls(input["has_next"].as_bool().unwrap()),
                "music" => MUSIC_CONTROLS.to_vec(),
                _ => live_controls(input["has_order"].as_bool().unwrap()),
            };
            let declared: Vec<&str> = declared.iter().map(|c| c.as_str()).collect();
            assert_eq!(json!(declared), case["out"], "{input}");
        }
    }

    #[test]
    fn a_video_obeys_as_the_shared_fixture_says() {
        for case in fixture("video") {
            assert_eq!(played(RemoteKind::Video, &case["in"]), case["out"], "{}", case["in"]);
        }
    }

    #[test]
    fn music_obeys_as_the_shared_fixture_says() {
        for case in fixture("music") {
            let index = case["in"]["player"]["index"].as_u64().unwrap() as usize;
            assert_eq!(played(RemoteKind::Music { index }, &case["in"]), case["out"], "{}", case["in"]);
        }
    }

    /// Through [`channel_step`], which [`crate::live::ChannelPages`] steps by (its paging has
    /// tests of its own, in tests/live.rs), over the case's channel order; the player leaves alone
    /// a channel the step would tune again (one with no other in its category).
    #[test]
    fn a_live_channel_obeys_as_the_shared_fixture_says() {
        for case in fixture("live") {
            let p = &case["in"]["player"];
            let channels: Option<Vec<i64>> =
                p["channels"].as_array().map(|a| a.iter().map(|c| c.as_i64().unwrap()).collect());
            let current = p["channel"].as_i64().unwrap();
            let player = RemotePlayer::live(p["session_id"].as_str(), channels.is_some());
            let action = remote_action(&player, &command(&case["in"]["command"]));
            let to = match (action, &channels) {
                (RemoteAction::Channel(by), Some(order)) => {
                    let at = order.iter().position(|c| *c == current).map(|i| i as i64);
                    channel_step(at, by, order.len() as i64).map(|j| order[j as usize]).filter(|c| *c != current)
                }
                _ => None,
            };
            assert_eq!(written(action, to), case["out"], "{}", case["in"]);
        }
    }

    /// What a player of `kind` playing session `s1` does with `command` for `session`.
    fn pick_in(kind: RemoteKind, session: &str, command: Control, stream_index: Option<i64>) -> RemoteAction {
        let player = RemotePlayer {
            kind,
            session: Some("s1"),
            playing: true,
            position_ms: 60_000,
            duration_ms: Some(7_200_000),
            has_next: false,
        };
        let c = Command { session_id: session, command, position_ms: None, offset_ms: None, stream_index };
        remote_action(&player, &c)
    }

    fn pick(kind: RemoteKind, command: Control, stream_index: Option<i64>) -> RemoteAction {
        pick_in(kind, "s1", command, stream_index)
    }

    #[test]
    fn a_video_declares_its_tracks_after_the_rest_where_its_file_has_them() {
        let names = |c: Vec<Control>| c.iter().map(|c| c.as_str()).collect::<Vec<_>>();
        assert_eq!(names(video_controls_with_tracks(false, false, false)), ["play", "pause", "seek", "skip"]);
        assert_eq!(
            names(video_controls_with_tracks(true, true, true)),
            ["play", "pause", "seek", "skip", "next", "audio", "subtitles"]
        );
        assert_eq!(names(video_controls_with_tracks(false, true, false)), ["play", "pause", "seek", "skip", "audio"]);
        assert_eq!(
            names(video_controls_with_tracks(false, false, true)),
            ["play", "pause", "seek", "skip", "subtitles"]
        );
    }

    #[test]
    fn a_video_picks_the_track_named_and_minus_one_turns_subtitles_off() {
        assert_eq!(pick(RemoteKind::Video, Control::Audio, Some(2)), RemoteAction::Audio(2));
        assert_eq!(pick(RemoteKind::Video, Control::Subtitles, Some(4)), RemoteAction::Subtitles(Some(4)));
        assert_eq!(pick(RemoteKind::Video, Control::Subtitles, Some(-1)), RemoteAction::Subtitles(None));
        // Without a track there is nothing to pick.
        assert_eq!(pick(RemoteKind::Video, Control::Audio, None), RemoteAction::None);
        assert_eq!(pick(RemoteKind::Video, Control::Subtitles, None), RemoteAction::None);
    }

    #[test]
    fn music_and_live_channels_have_no_tracks_to_pick() {
        for kind in [RemoteKind::Music { index: 1 }, RemoteKind::Live { has_order: true }] {
            assert_eq!(pick(kind, Control::Audio, Some(2)), RemoteAction::None, "{kind:?}");
            assert_eq!(pick(kind, Control::Subtitles, Some(-1)), RemoteAction::None, "{kind:?}");
        }
        // Another session's track is someone else's player's.
        assert_eq!(pick_in(RemoteKind::Video, "s2", Control::Audio, Some(2)), RemoteAction::None);
    }

    #[test]
    fn a_channel_step_goes_round_the_ends_and_starts_at_the_first_from_outside() {
        assert_eq!(channel_step(Some(2), 1, 3), Some(0));
        assert_eq!(channel_step(Some(0), -1, 3), Some(2));
        assert_eq!(channel_step(None, -1, 3), Some(0));
        assert_eq!(channel_step(Some(0), 1, 0), None);
    }
}
