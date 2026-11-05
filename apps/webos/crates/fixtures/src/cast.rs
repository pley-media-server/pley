//! Casting (docs/API.md, "Casting"): an event stream that names its device is a receiver while
//! it is open, several streams of one id counting once; `GET /api/remote/devices` lists a
//! profile's, one per device, and the household's other profiles' with the profile each is on;
//! `POST /api/remote/devices/{id}/cast` checks what a remote sends one, in the server's order, and
//! sends `cast` to the streams of the profile the receiver is on. To another profile's receiver,
//! after the caster's PIN, the cast carries the caster's profile and a switch ticket, which the
//! switch-profile grant takes in place of a PIN ([`redeem`]).

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::events::ServerEvent;
use crate::http::Response;
use crate::model::Kind;
use crate::play::{device_name, has_track};
use crate::random::token;
use crate::server::{
    Answer, Cx, Need, PLAY, Route, Shared, State, bad_request, error, forbidden, json, locked, not_found,
};

/// The most receivers one profile has at once: a stream past them still gets its events.
pub(crate) const MAX_RECEIVERS: usize = 32;
/// Casts one receiver takes each second at most.
const CASTS_PER_SECOND: usize = 10;
/// The latest a title starts: a day in.
const MAX_START_MS: i64 = 24 * 3_600_000;
/// The kinds of device a stream may say it is.
const KINDS: [&str; 6] = ["tv", "phone", "tablet", "computer", "browser", "other"];
/// How long a cast's switch ticket lasts, in ms.
const TICKET_MS: i64 = 60_000;
/// The most tickets waiting at once; past them, the oldest go.
const MAX_TICKETS: usize = 256;

/// What a remote casts, as `POST /api/remote/devices/{id}/cast` takes it: one of a title, a
/// playlist or a Live TV channel, where a title starts, whether to shuffle, and a movie's or an
/// episode's tracks (subtitles -1: none). A receiver of another of the household's profiles takes
/// that profile (`profile_id`), and the caster's PIN when their profile has one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CastBody {
    pub item_id: Option<i64>,
    pub playlist_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub start_ms: Option<i64>,
    pub shuffle: bool,
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
    pub profile_id: Option<i64>,
    pub pin: Option<String>,
}

/// What a cast's switch ticket lets its receiver do: device `device_id`, on profile `from`,
/// switches to profile `to`, once, within a minute of `issued_ms`.
pub(crate) struct Ticket {
    device_id: String,
    from: i64,
    to: i64,
    issued_ms: i64,
}

/// A ticket for device `device_id`, on profile `from`, to switch to profile `to` once within a
/// minute.
fn issue_ticket(state: &mut State, device_id: &str, from: i64, to: i64, now_ms: i64) -> String {
    let tickets = &mut state.cast_tickets;
    tickets.retain(|_, t| now_ms - t.issued_ms < TICKET_MS);
    if tickets.len() >= MAX_TICKETS
        && let Some(oldest) = tickets.iter().min_by_key(|(_, t)| t.issued_ms).map(|(k, _)| k.clone())
    {
        tickets.remove(&oldest);
    }
    let ticket = token(32);
    tickets.insert(ticket.clone(), Ticket { device_id: device_id.to_string(), from, to, issued_ms: now_ms });
    ticket
}

/// Spends `ticket`: whether it lets device `device_id`, on profile `from`, switch to profile `to`
/// now. A ticket shown for anything else is spent too.
pub(crate) fn redeem(state: &mut State, ticket: &str, device_id: &str, from: i64, to: i64, now_ms: i64) -> bool {
    let Some(t) = state.cast_tickets.remove(ticket) else { return false };
    now_ms - t.issued_ms < TICKET_MS && t.device_id == device_id && t.from == from && t.to == to
}

impl CastBody {
    /// A title: a movie, an episode, a show, a season, a track, an album or an artist.
    pub fn item(id: i64) -> CastBody {
        CastBody { item_id: Some(id), ..CastBody::default() }
    }

    pub fn playlist(id: i64) -> CastBody {
        CastBody { playlist_id: Some(id), ..CastBody::default() }
    }

    pub fn channel(id: i64) -> CastBody {
        CastBody { channel_id: Some(id), ..CastBody::default() }
    }

    /// The body's JSON.
    pub fn to_json(&self) -> Value {
        json!({
            "item_id": self.item_id, "playlist_id": self.playlist_id, "channel_id": self.channel_id,
            "start_ms": self.start_ms, "shuffle": self.shuffle,
            "audio_index": self.audio_index, "subtitle_index": self.subtitle_index,
            "profile_id": self.profile_id, "pin": self.pin,
        })
    }
}

/// A device that takes casts, as its event stream says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Receiver {
    /// The app's own id for the device: where casts go.
    pub id: String,
    /// Which device it is, as its sessions say.
    pub device_id: String,
    pub name: Option<String>,
    pub kind: String,
    /// What it plays when cast to: `item`, `live`.
    pub casts: Vec<&'static str>,
}

impl Receiver {
    /// The receiver an event stream's query names, when it does: `receiver` (1 to 128 letters,
    /// digits, `.`, `_`, `:` and `-`, else the stream isn't one), `device_name` and `device_kind`
    /// (else told from the app, which here is always a TV, and its User-Agent), and `casts`.
    pub fn of(cx: &Cx<'_>) -> Option<Receiver> {
        let id = cx.query("receiver").filter(|id| receiver_id(id))?;
        let name = cx.query("device_name").map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
        Some(Receiver {
            id: id.to_string(),
            device_id: cx.caller().grant.map_or_else(|| format!("receiver-{id}"), |grant| format!("app-{grant}")),
            name: name.or_else(|| device_name(cx.req.header("user-agent"))),
            kind: cx.query("device_kind").filter(|k| KINDS.contains(k)).unwrap_or("tv").to_string(),
            casts: cx.query("casts").map(cast_kinds).unwrap_or_default(),
        })
    }

    /// As `GET /api/remote/devices` lists it.
    pub fn to_json(&self) -> Value {
        json!({
            "receiver_id": self.id, "device_id": self.device_id, "name": self.name,
            "kind": self.kind, "casts": self.casts,
        })
    }
}

fn receiver_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

/// The kinds a stream's `casts` names that the server knows, each once.
fn cast_kinds(text: &str) -> Vec<&'static str> {
    let named: Vec<&str> = text.split(',').map(str::trim).collect();
    ["item", "live"].into_iter().filter(|k| named.contains(k)).collect()
}

pub(crate) fn routes() -> Vec<Route> {
    let play = Need::Scope(PLAY);
    vec![
        locked("GET", "/api/remote/devices", play, |cx, state| {
            Ok(json(200, &listed(cx.shared, state, cx.caller().profile)))
        }),
        locked("POST", "/api/remote/devices/*/cast", play, |cx, state| {
            let body = cx.req.json().filter(Value::is_object).unwrap_or(Value::Null);
            send(cx.shared, state, cx.caller().profile, cx.value(0), &body, cx.req.header("user-agent"))
        }),
    ]
}

/// `GET /api/remote/devices` for `profile`: its receivers, and the household's other profiles'
/// with the profile each is on (`{ id, name }`), one per device (the caller's own while one is on
/// two, switching), by name.
fn listed(shared: &Shared, state: &State, profile: i64) -> Vec<Value> {
    let mut out: Vec<(Receiver, Option<Value>)> =
        shared.events.receivers(profile).into_iter().map(|r| (r, None)).collect();
    for other in state.household.profiles.values().filter(|p| p.id != profile) {
        let on = json!({ "id": other.id, "name": other.name });
        out.extend(shared.events.receivers(other.id).into_iter().map(|r| (r, Some(on.clone()))));
    }
    let mut seen = HashSet::new();
    out.retain(|(r, _)| seen.insert(r.device_id.clone()));
    out.sort_by_cached_key(|(r, _)| (r.name.is_none(), r.name.as_deref().map(str::to_lowercase), r.id.clone()));
    out.into_iter()
        .map(|(r, on)| {
            let mut listed = r.to_json();
            if let Some(on) = on {
                listed["profile"] = on;
            }
            listed
        })
        .collect()
}

/// The caster's PIN, when their profile has one, before a cast to another profile's receiver
/// makes it act as theirs: counted and throttled with the switches to it.
fn check_casters_pin(state: &mut State, caster: i64, pin: Option<&str>, now_ms: i64) -> Result<(), Response> {
    let now_s = now_ms.div_euclid(1_000);
    let profile = state.profile_mut(caster);
    let Some(expected) = profile.pin else { return Ok(()) };
    profile.wrong_pins.retain(|at| *at >= now_s - 15 * 60);
    if profile.wrong_pins.len() >= 5 {
        return Err(error(429, "too_many_requests", "too many attempts, try again in 15 min"));
    }
    match pin {
        None => Err(error(403, "pin_needed", "this profile is protected by a PIN")),
        Some(pin) if pin != expected => {
            profile.wrong_pins.push(now_s);
            Err(error(403, "wrong_pin", "wrong PIN"))
        }
        Some(_) => {
            profile.wrong_pins.clear();
            Ok(())
        }
    }
}

/// What `POST /api/remote/devices/{id}/cast` does for `profile`, the remote's device named by
/// `agent` (its User-Agent): the receiver (of `profile_id`, the household's profile it is on, when
/// another), the one thing named and what it is, what the receiver takes, where a title starts and
/// its tracks, the caster's PIN for another profile's receiver, and how often, in that order, then
/// `cast` to the streams of the profile the receiver is on, with a switch ticket when it isn't
/// `profile`.
pub(crate) fn send(
    shared: &Shared,
    state: &mut State,
    profile: i64,
    receiver: &str,
    body: &Value,
    agent: Option<&str>,
) -> Answer {
    // A receiver outside the household, or someone else's not named by its profile, is as unknown
    // as one that isn't there.
    let owner = body["profile_id"].as_i64().unwrap_or(profile);
    if state.household.profile(owner).is_none() {
        return Err(not_found("no such device"));
    }
    let takes = shared.events.receiver(owner, receiver).ok_or_else(|| not_found("no such device"))?.casts;
    let (item_id, playlist_id, channel_id) =
        (body["item_id"].as_i64(), body["playlist_id"].as_i64(), body["channel_id"].as_i64());
    let kind = match (item_id, playlist_id, channel_id) {
        (Some(id), None, None) => {
            let item = shared.catalog.item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
            if item.kind() == Kind::Extra {
                return Err(bad_request(
                    "only movies, episodes, shows, seasons, tracks, albums and artists can be cast",
                ));
            }
            "item"
        }
        (None, Some(id), None) => {
            if !state.household.playlists.get(&id).is_some_and(|p| p.owner == profile) {
                return Err(not_found(&format!("playlist {id} not found")));
            }
            "item"
        }
        (None, None, Some(id)) => {
            if !state.profile(profile).live {
                return Err(forbidden("Live TV isn't open to this account"));
            }
            if !state.live.has_channel(id) {
                return Err(not_found("channel not found"));
            }
            "live"
        }
        _ => return Err(bad_request("name one of item_id, playlist_id and channel_id")),
    };
    if !takes.contains(&kind) {
        let words = if kind == "live" { "Live TV" } else { "titles" };
        return Err(error(409, "not_castable", &format!("this device doesn't play {words} from a remote")));
    }
    let start_ms = body["start_ms"].as_i64();
    if start_ms.is_some_and(|s| !(0..=MAX_START_MS).contains(&s)) {
        return Err(bad_request(&format!("start_ms must be between 0 and {MAX_START_MS}")));
    }
    // The tracks picked on the remote's page: of one of a movie's or an episode's files.
    let (audio_index, subtitle_index) = (body["audio_index"].as_i64(), body["subtitle_index"].as_i64());
    if audio_index.is_some() || subtitle_index.is_some() {
        let item =
            item_id.and_then(|id| shared.catalog.item(id)).filter(|i| matches!(i.kind(), Kind::Movie | Kind::Episode));
        let item = item.ok_or_else(|| bad_request("only a movie or an episode is cast with tracks"))?;
        let one_of =
            |kind: &str, index: i64| item.files.iter().any(|f| has_track(&shared.catalog, state, f.id, kind, index));
        if audio_index.is_some_and(|i| !one_of("audio", i))
            || subtitle_index.is_some_and(|i| i != -1 && !one_of("subtitle", i))
        {
            return Err(bad_request("a track must be one of the title's"));
        }
    }
    let now = shared.now_ms();
    // The receiver will act as the caster's profile: their PIN, as a switch to it would take.
    if owner != profile {
        check_casters_pin(state, profile, body["pin"].as_str(), now)?;
    }
    let sent = state.casts_sent.entry((owner, receiver.to_string())).or_default();
    sent.retain(|at| now - at < 1_000);
    if sent.len() >= CASTS_PER_SECOND {
        return Err(error(429, "too_many_requests", "10 casts a second at most: wait a moment"));
    }
    sent.push(now);
    let switch_ticket = if owner == profile {
        None
    } else {
        let device = shared.events.receiver(owner, receiver).ok_or_else(|| not_found("no such device"))?.device_id;
        Some(issue_ticket(state, &device, owner, profile, now))
    };
    shared.events.emit_to(
        owner,
        &ServerEvent::Cast {
            receiver_id: receiver.to_string(),
            item_id,
            playlist_id,
            channel_id,
            start_ms,
            shuffle: body["shuffle"].as_bool().unwrap_or(false),
            audio_index,
            subtitle_index,
            from: device_name(agent),
            profile_id: switch_ticket.is_some().then_some(profile),
            switch_ticket,
        },
    );
    Ok(Response::new(202))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_receiver_is_short_and_plain_and_takes_what_the_server_knows() {
        for id in ["webos-0123456789abcdef", "a", "den.tv_2:x", &"a".repeat(128)] {
            assert!(receiver_id(id), "{id}");
        }
        for id in ["", "den tv", "tv/1", "tv?", &"a".repeat(129)] {
            assert!(!receiver_id(id), "{id:?}");
        }
        assert_eq!(cast_kinds("item,live"), ["item", "live"]);
        assert_eq!(cast_kinds("live, item,item"), ["item", "live"]);
        assert_eq!(cast_kinds("item,radio"), ["item"]);
        assert!(cast_kinds("").is_empty());
    }
}
