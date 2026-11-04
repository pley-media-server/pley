//! Live TV (docs/API.md, "Live TV"): a lineup of made-up channels in six
//! groups on two sources, a guide made the same for any time asked, watching
//! with live HLS (the sample stream's segments over and over, in a sliding
//! window), a channel's `stream.ts` (the same segments one after another, in
//! real time, for as long as it is read), keyed addresses, plans, leaving,
//! favourites and the channels watched last. The second source allows one connection at once, and
//! another of Alex's devices holds it at start ([`ids::LIVE_BUSY_CHANNEL`]),
//! so watching another of its channels answers `409` until that one is
//! taken over. Live TV isn't open to Sam.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};

use crate::household::Profile;
use crate::http::{Body, Response, lock};
use crate::media::{HLS_SEGMENT_MS, HLS_SEGMENTS};
use crate::pictures::{Shape, picture};
use crate::play::controls_of;
use crate::random::{hex_id, token};
use crate::server::{
    Answer, Cx, LIBRARY, Need, PLAY, Route, State, bad_request, forbidden, free, json, locked, no_content, not_found,
    session_stopped,
};
use crate::words::{ADJECTIVES, GENRES, NOUNS};
use crate::{ServerEvent, ids};

/// A guide block: programmes never cross one, so any span can be made alone.
const BLOCK_S: i64 = 3 * 3_600;
const SLOT_S: i64 = 30 * 60;
/// How a block is cut, in half hours.
const PATTERNS: [&[i64]; 6] = [&[2, 2, 2], &[1, 2, 3], &[3, 3], &[1, 1, 2, 2], &[2, 1, 3], &[4, 2]];
/// Segments in a live playlist's window.
const WINDOW: i64 = 5;
/// The client id of Alex's other device, which holds the limited source at start.
const OTHER_DEVICE: &str = "fixture-living-room";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Genre {
    News,
    Sport,
    Entertainment,
    Movies,
    Kids,
    Music,
}

struct Group {
    name: &'static str,
    region: Option<&'static str>,
    label: &'static str,
    count: usize,
    source: i64,
    genre: Genre,
}

/// The lineup's groups, in order: channel ids run through them from 1.
const GROUPS: [Group; 6] = [
    Group { name: "UK| News", region: Some("UK"), label: "News", count: 6, source: 1, genre: Genre::News },
    Group { name: "UK| Sport", region: Some("UK"), label: "Sport", count: 8, source: 1, genre: Genre::Sport },
    Group {
        name: "UK| Entertainment",
        region: Some("UK"),
        label: "Entertainment",
        count: 10,
        source: 1,
        genre: Genre::Entertainment,
    },
    Group { name: "FR| Cinéma", region: Some("FR"), label: "Cinéma", count: 6, source: 2, genre: Genre::Movies },
    Group { name: "Kids", region: None, label: "Kids", count: 6, source: 1, genre: Genre::Kids },
    Group { name: "Music", region: None, label: "Music", count: 6, source: 1, genre: Genre::Music },
];

/// The sources: id, name, and how many channels they carry at once.
const SOURCES: [(i64, &str, Option<usize>); 2] = [(1, "Fixture IPTV", None), (2, "Fixture IPTV Two", Some(1))];

struct Channel {
    id: i64,
    group: usize,
    name: String,
    number: Option<String>,
    logo: bool,
    has_guide: bool,
}

impl Channel {
    fn group(&self) -> &'static Group {
        &GROUPS[self.group]
    }
}

/// A programme, as apps get it.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Program {
    channel_id: i64,
    start: i64,
    stop: i64,
    title: String,
    subtitle: Option<String>,
    description: Option<String>,
    categories: Vec<String>,
    image: Option<String>,
    season: Option<i64>,
    episode: Option<i64>,
    is_new: bool,
    is_live: bool,
    rating: Option<String>,
}

/// A channel, as apps get it.
#[derive(Clone, Debug, Serialize)]
struct ChannelJson {
    id: i64,
    source_id: i64,
    source_uid: String,
    name: String,
    number: Option<String>,
    group: String,
    logo: Option<String>,
    favorite: bool,
    has_guide: bool,
    now: Option<Program>,
    next: Option<Program>,
}

/// Someone watching a channel.
struct Viewer {
    profile: i64,
    channel: i64,
    client_id: Option<String>,
    key: Option<String>,
    started_ms: i64,
    /// A stream.ts reader's: set when either end lets go (the reader, or the session ended
    /// here), which ends the other.
    stream: Option<Arc<AtomicBool>>,
    /// The remote's commands its player obeys (none for a stream.ts reader), and what it plays on.
    controls: Vec<String>,
    device_kind: Option<String>,
}

/// The device a client id is: players read a channel's stream as `<client_id>-stream`, beside
/// the session asked for as `<client_id>` (the server's `device_of`).
fn device_of(client: &str) -> &str {
    client.strip_suffix("-stream").unwrap_or(client)
}

/// Sets a stream reader's flag when its body goes, sent or not (a HEAD request).
struct StreamEnd(Arc<AtomicBool>);

impl Drop for StreamEnd {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// The lineup and who watches what.
pub(crate) struct Live {
    channels: Vec<Channel>,
    sessions: HashMap<String, Viewer>,
    /// Sessions that ended for a reason the viewer is told (`410`).
    ended: HashMap<String, (i64, Option<String>)>,
    keys: HashMap<String, String>,
}

/// A number from two, the same every run.
fn mix(a: i64, b: i64) -> u64 {
    let mut z = (a as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (b as u64).wrapping_add(0x632b_e59b_d9b4_e019);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Live {
    pub fn new(now_ms: i64) -> Live {
        let mut channels = Vec::new();
        let mut id = 1;
        for (g, group) in GROUPS.iter().enumerate() {
            for n in 0..group.count {
                let word = NOUNS[(id as usize * 7 + 3) % 30];
                let name = match group.genre {
                    Genre::News => format!("{word} News"),
                    Genre::Sport => format!("{word} Sport {}", n + 1),
                    Genre::Entertainment => format!("{} {word}", ADJECTIVES[(id as usize * 3) % 30]),
                    Genre::Movies => format!("Ciné {word}"),
                    Genre::Kids => format!("{} Kids", ADJECTIVES[(id as usize * 5) % 30]),
                    Genre::Music => format!("{word} FM"),
                };
                channels.push(Channel {
                    id,
                    group: g,
                    name,
                    number: (id != ids::LIVE_CHANNELS).then(|| id.to_string()),
                    logo: id % 7 != 0,
                    has_guide: id != ids::LIVE_NO_GUIDE_CHANNEL,
                });
                id += 1;
            }
        }
        let mut live = Live { channels, sessions: HashMap::new(), ended: HashMap::new(), keys: HashMap::new() };
        live.sessions.insert(
            hex_id(),
            Viewer {
                profile: ids::ALEX,
                channel: ids::LIVE_BUSY_CHANNEL,
                client_id: Some(OTHER_DEVICE.into()),
                key: None,
                started_ms: now_ms,
                stream: None,
                controls: Vec::new(),
                device_kind: None,
            },
        );
        live
    }

    fn channel(&self, id: i64) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    fn json(&self, channel: &Channel, profile: &Profile, now: i64) -> ChannelJson {
        let (on_now, next) = if channel.has_guide { now_and_next(channel, now) } else { (None, None) };
        let source = channel.group().source;
        ChannelJson {
            id: channel.id,
            source_id: source,
            source_uid: format!("fixture{source}"),
            name: channel.name.clone(),
            number: channel.number.clone(),
            group: channel.group().name.to_string(),
            logo: channel
                .logo
                .then(|| format!("/api/live/channels/{}/logo?v={:08x}", channel.id, mix(channel.id, 7) as u32)),
            favorite: profile.live_favorites.contains(&channel.id),
            has_guide: channel.has_guide,
            now: on_now,
            next,
        }
    }

    /// Channels by name, number or what's on now, for search.
    pub fn search(&self, q: &str, profile: &Profile, now: i64, limit: usize) -> Vec<Value> {
        self.channels
            .iter()
            .filter(|c| matches(c, q, now))
            .take(limit)
            .map(|c| serde_json::to_value(self.json(c, profile, now)).unwrap_or(Value::Null))
            .collect()
    }

    /// The channels a source carries now.
    fn tuned(&self, source: i64) -> Vec<i64> {
        let mut tuned: Vec<i64> = self
            .sessions
            .values()
            .filter(|v| self.channel(v.channel).is_some_and(|c| c.group().source == source))
            .map(|v| v.channel)
            .collect();
        tuned.sort_unstable();
        tuned.dedup();
        tuned
    }

    /// Takes a session away, ending its stream.ts if it has one.
    fn remove(&mut self, id: &str) -> Option<Viewer> {
        let viewer = self.sessions.remove(id)?;
        if let Some(key) = &viewer.key {
            self.keys.remove(key);
        }
        if let Some(stream) = &viewer.stream {
            stream.store(true, Ordering::SeqCst);
        }
        Some(viewer)
    }

    /// Forgets the stream.ts readers that let go, as the server drops a stream viewer whose
    /// response ended.
    fn forget_gone_streams(&mut self) {
        self.sessions.retain(|_, v| !v.stream.as_ref().is_some_and(|s| s.load(Ordering::SeqCst)));
    }

    /// Who watches as session `id`, and what its player obeys.
    pub(crate) fn viewer(&self, id: &str) -> Option<(i64, Vec<String>)> {
        self.sessions
            .get(id)
            .filter(|v| !v.stream.as_ref().is_some_and(|s| s.load(Ordering::SeqCst)))
            .map(|v| (v.profile, v.controls.clone()))
    }

    /// The viewers `me` sees in the fixture's own session list: their own, or everyone's for an
    /// administrator. Live TV has no tracks to pick.
    pub(crate) fn listed(&self, me: i64, admin: bool) -> Vec<Value> {
        let mut list: Vec<(&String, &Viewer)> =
            self.sessions.iter().filter(|(_, v)| admin || v.profile == me).collect();
        list.sort_by_key(|(id, v)| (v.started_ms, id.as_str()));
        list.into_iter()
            .map(|(id, v)| {
                json!({
                    "session_id": id, "kind": "live", "profile_id": v.profile, "channel_id": v.channel,
                    "controls": v.controls, "device_kind": v.device_kind,
                    "audio_index": null, "subtitle_index": null, "tracks": { "audio": [], "subtitles": [] },
                })
            })
            .collect()
    }

    pub(crate) fn end(&mut self, id: &str, message: Option<String>) {
        if let Some(viewer) = self.sessions.remove(id) {
            if let Some(key) = viewer.key {
                self.keys.insert(key, id.to_string());
            }
            if let Some(stream) = viewer.stream {
                stream.store(true, Ordering::SeqCst);
            }
            self.ended.insert(id.to_string(), (viewer.profile, message));
        }
    }
}

/// Whether a channel answers a search: its name, its number's start, or
/// what's on now.
fn matches(channel: &Channel, q: &str, now: i64) -> bool {
    let q = q.to_lowercase();
    channel.name.to_lowercase().contains(&q)
        || channel.number.as_deref().is_some_and(|n| n.starts_with(&q))
        || (channel.has_guide && now_and_next(channel, now).0.is_some_and(|p| p.title.to_lowercase().contains(&q)))
}

/// The programmes of a channel between `from` and `to` (unix seconds).
fn programs(channel: &Channel, from: i64, to: i64) -> Vec<Program> {
    let mut out = Vec::new();
    let mut block = from.div_euclid(BLOCK_S);
    while block * BLOCK_S < to {
        let pattern = PATTERNS[(mix(channel.id, block) % PATTERNS.len() as u64) as usize];
        let mut start = block * BLOCK_S;
        for slots in pattern {
            let stop = start + slots * SLOT_S;
            if stop > from && start < to {
                out.push(program(channel, start, stop));
            }
            start = stop;
        }
        block += 1;
    }
    out
}

fn now_and_next(channel: &Channel, now: i64) -> (Option<Program>, Option<Program>) {
    let mut around = programs(channel, now, now + 5 * 3_600).into_iter();
    (around.next(), around.next())
}

/// A programme's title, episode title, categories, and season and episode.
type Listing = (String, Option<String>, Vec<String>, Option<(i64, i64)>);

fn program(channel: &Channel, start: i64, stop: i64) -> Program {
    let h = mix(channel.id * 1_000_003, start) as usize;
    let adjective = ADJECTIVES[h % 30];
    let noun = NOUNS[(h / 30) % 30];
    let episode = |title: String, category: &str| -> Listing {
        let subtitle = format!("The {} {}", ADJECTIVES[(h / 7) % 30], NOUNS[(h / 11) % 30]);
        (title, Some(subtitle), vec![category.to_string()], Some((1 + (h % 6) as i64, 1 + (h / 6 % 12) as i64)))
    };
    let (title, subtitle, categories, numbers) = match channel.group().genre {
        Genre::News => (
            [format!("The {noun} Report"), format!("{noun} Tonight"), "Morning Briefing".to_string()][h % 3].clone(),
            None,
            vec!["News".to_string()],
            None,
        ),
        Genre::Sport => (
            [format!("{noun} Football"), format!("{noun} Cup Live"), format!("Match of the {noun}")][h % 3].clone(),
            None,
            vec!["Sport".to_string()],
            None,
        ),
        Genre::Entertainment => episode(format!("{adjective} {noun}"), "Drama"),
        Genre::Movies => (
            format!("The {adjective} {noun}"),
            None,
            vec!["Movie".to_string(), GENRES[h % GENRES.len()].to_string()],
            None,
        ),
        Genre::Kids => episode(format!("{adjective} Friends"), "Children"),
        Genre::Music => (format!("{noun} Hits"), None, vec!["Music".to_string()], None),
    };
    let genre = channel.group().genre;
    let pictured = matches!(genre, Genre::Movies | Genre::Entertainment | Genre::Kids);
    Program {
        channel_id: channel.id,
        start,
        stop,
        description: Some(format!("{title} on {}. Pley's fixture server made it up.", channel.name)),
        title,
        subtitle,
        categories,
        image: pictured.then(|| format!("/api/live/channels/{}/programs/{start}/image", channel.id)),
        season: numbers.map(|n| n.0),
        episode: numbers.map(|n| n.1),
        is_new: numbers.is_some() && h.is_multiple_of(4),
        is_live: matches!(genre, Genre::News | Genre::Sport) && h.is_multiple_of(2),
        rating: matches!(genre, Genre::Movies | Genre::Entertainment).then(|| ["PG", "TV-14", "12"][h % 3].to_string()),
    }
}

pub(crate) fn routes() -> Vec<Route> {
    let library = Need::Scope(LIBRARY);
    let play = Need::Scope(PLAY);
    vec![
        locked("GET", "/api/live", library, |cx, state| {
            let profile = state.profile(cx.caller().profile);
            let open = profile.live;
            let recent: Vec<i64> =
                if open { profile.live_recent.iter().take(12).copied().collect() } else { Vec::new() };
            Ok(json(
                200,
                &json!({
                    "available": open, "allowed": open,
                    "channels": if open { state.live.channels.len() } else { 0 },
                    "sources": 0, "recent": recent,
                }),
            ))
        }),
        locked("GET", "/api/live/channels", library, |cx, state| {
            let profile = open(cx, state)?;
            let now = cx.now();
            let list: Vec<ChannelJson> = state.live.channels.iter().map(|c| state.live.json(c, profile, now)).collect();
            Ok(json(200, &list))
        }),
        locked("GET", "/api/live/groups", library, groups),
        locked("GET", "/api/live/browse", library, browse),
        locked("GET", "/api/live/guide", library, guide),
        locked("GET", "/api/live/channels/*", library, |cx, state| {
            let profile = open(cx, state)?;
            let channel = channel(cx, state)?;
            Ok(json(200, &state.live.json(channel, profile, cx.now())))
        }),
        free("GET", "/api/live/channels/*/logo", library, |cx| {
            let id = cx.id(0)?;
            let has_logo = {
                let state = lock(&cx.shared.state);
                state.live.channel(id).map(|c| c.logo)
            };
            if has_logo != Some(true) {
                return Err(not_found("no logo"));
            }
            let width = cx.query("w").and_then(|w| w.parse().ok());
            Ok(picture(cx.shared.pictures.logo(id, width), "image/png", "private, max-age=86400"))
        }),
        free("GET", "/api/live/channels/*/programs/*/image", library, |cx| {
            let (id, start) = (cx.id(0)?, cx.id(1)?);
            let width = cx.query("w").and_then(|w| w.parse().ok());
            let seed = id * 1_000 + start.div_euclid(SLOT_S).rem_euclid(1_000);
            Ok(picture(cx.shared.pictures.artwork(seed, Shape::Wide, width), "image/jpeg", "private, max-age=86400"))
        }),
        locked("PUT", "/api/live/channels/*/favorite", play, |cx, state| favorite(cx, state, true)),
        locked("DELETE", "/api/live/channels/*/favorite", play, |cx, state| favorite(cx, state, false)),
        locked("POST", "/api/live/channels/*/watch", play, watch),
        locked("GET", "/api/live/sessions/*/plan", play, |cx, state| {
            open(cx, state)?;
            own_viewer(state, cx.value(0), cx.caller().profile)?;
            Ok(json(
                200,
                &json!({
                    "video_codec": "h264", "width": 1920, "height": 1080, "frame_rate": 24.0, "interlaced": false,
                    "audio_codec": "aac", "audio_channels": 2, "video": "copy", "audio": "copy",
                }),
            ))
        }),
        free("GET", "/api/live/channels/*/stream.ts", play, stream),
        free("GET", "/api/live/sessions/*/*", play, |cx| {
            let started = {
                let mut state = lock(&cx.shared.state);
                own_viewer(&mut state, cx.value(0), cx.caller().profile)?.started_ms
            };
            live_file(cx, cx.value(1), started)
        }),
        locked("DELETE", "/api/live/sessions/*", play, |cx, state| {
            let id = cx.value(0);
            // A bare 204 or 404, as the server answers it.
            match state.live.sessions.get(id) {
                Some(viewer) if viewer.profile == cx.caller().profile => {
                    state.live.remove(id);
                    cx.shared.events.emit(&ServerEvent::LiveTunersChanged);
                    Ok(no_content())
                }
                _ => Err(Response::new(404)),
            }
        }),
        free("GET", "/api/live/media/*/*", Need::Public, |cx| {
            let started = {
                let state = lock(&cx.shared.state);
                let id = state.live.keys.get(cx.value(0)).ok_or_else(|| not_found("no such stream"))?;
                if let Some((_, message)) = state.live.ended.get(id) {
                    return Err(session_stopped(message.as_deref()));
                }
                state.live.sessions.get(id).ok_or_else(|| not_found("no such stream"))?.started_ms
            };
            live_file(cx, cx.value(1), started)
        }),
    ]
}

/// The caller's profile when Live TV is open to it, else `403`.
fn open<'a>(cx: &Cx<'_>, state: &'a State) -> Result<&'a Profile, Response> {
    let profile = state.profile(cx.caller().profile);
    if !profile.live {
        return Err(forbidden("Live TV isn't open to this account"));
    }
    Ok(profile)
}

fn channel<'a>(cx: &Cx<'_>, state: &'a State) -> Result<&'a Channel, Response> {
    let id = cx.id(0)?;
    state.live.channel(id).ok_or_else(|| not_found("channel not found"))
}

/// The caller's own live session, or why not.
fn own_viewer<'a>(state: &'a mut State, id: &str, me: i64) -> Result<&'a Viewer, Response> {
    if let Some((profile, message)) = state.live.ended.get(id)
        && *profile == me
    {
        return Err(session_stopped(message.as_deref()));
    }
    match state.live.sessions.get(id) {
        Some(viewer) if viewer.profile == me => Ok(viewer),
        _ => Err(not_found("no such session")),
    }
}

fn groups(cx: &Cx<'_>, state: &mut State) -> Answer {
    let profile = open(cx, state)?;
    let list: Vec<Value> = GROUPS
        .iter()
        .map(|g| json!({ "name": g.name, "region": g.region, "label": g.label, "count": g.count }))
        .collect();
    Ok(json(
        200,
        &json!({
            "total": state.live.channels.len(),
            "favorites": profile.live_favorites.len(),
            "recent": profile.live_recent.len(),
            "groups": list,
        }),
    ))
}

/// GET /api/live/browse: a page of a category's channels.
fn browse(cx: &Cx<'_>, state: &mut State) -> Answer {
    let profile = open(cx, state)?;
    let live = &state.live;
    let now = cx.now();
    let mut list: Vec<&Channel> = match cx.query("category").unwrap_or("all") {
        "all" => live.channels.iter().collect(),
        "favorites" => live.channels.iter().filter(|c| profile.live_favorites.contains(&c.id)).collect(),
        "recent" => profile.live_recent.iter().take(24).filter_map(|id| live.channel(*id)).collect(),
        "group" => {
            let group = cx.query("group").ok_or_else(|| bad_request("category=group needs a group"))?;
            live.channels.iter().filter(|c| c.group().name == group).collect()
        }
        other => return Err(bad_request(&format!("unknown category '{other}'"))),
    };
    if let Some(q) = cx.query("q").map(str::trim).filter(|q| !q.is_empty()) {
        list.retain(|c| matches(c, q, now));
    }
    let number = |name: &str| -> Result<Option<i64>, Response> {
        cx.query(name)
            .map(|v| v.parse::<i64>().map_err(|_| bad_request(&format!("{name} must be a number"))))
            .transpose()
    };
    let limit = number("limit")?.unwrap_or(100).clamp(1, 500) as usize;
    let mut offset = number("offset")?.unwrap_or(0).max(0) as usize;
    if let Some(around) = number("around")?
        && let Some(at) = list.iter().position(|c| c.id == around)
    {
        offset = at / limit * limit;
    }
    let channels: Vec<ChannelJson> = list.iter().skip(offset).take(limit).map(|c| live.json(c, profile, now)).collect();
    Ok(json(200, &json!({ "total": list.len(), "offset": offset, "channels": channels })))
}

/// GET /api/live/guide?from=&to=&ids=: programmes by channel, from half an
/// hour ago for six hours unless asked otherwise, two days at most.
fn guide(cx: &Cx<'_>, state: &mut State) -> Answer {
    open(cx, state)?;
    let number = |name: &str| -> Result<Option<i64>, Response> {
        cx.query(name)
            .map(|v| v.parse::<i64>().map_err(|_| bad_request(&format!("{name} must be a number"))))
            .transpose()
    };
    let from = number("from")?.unwrap_or(cx.now() - 30 * 60);
    let to = number("to")?.unwrap_or(from + 6 * 3_600);
    if to <= from || to - from > 2 * 86_400 {
        return Err(bad_request("the guide spans up to two days, forwards"));
    }
    let wanted: Option<HashSet<i64>> = match cx.query("ids").filter(|v| !v.is_empty()) {
        Some(list) => Some(
            list.split(',')
                .map(|id| id.trim().parse::<i64>().map_err(|_| bad_request("ids are numbers")))
                .collect::<Result<_, _>>()?,
        ),
        None => None,
    };
    let mut by_channel: BTreeMap<i64, Vec<Program>> = BTreeMap::new();
    for channel in &state.live.channels {
        if wanted.as_ref().is_some_and(|w| !w.contains(&channel.id)) {
            continue;
        }
        let list = if channel.has_guide { programs(channel, from, to) } else { Vec::new() };
        by_channel.insert(channel.id, list);
    }
    Ok(json(200, &json!({ "from": from, "to": to, "programs": by_channel })))
}

fn favorite(cx: &Cx<'_>, state: &mut State, on: bool) -> Answer {
    open(cx, state)?;
    let id = channel(cx, state)?.id;
    let profile = state.profile_mut(cx.caller().profile);
    profile.live_favorites.retain(|c| *c != id);
    if on {
        profile.live_favorites.push(id);
    }
    Ok(no_content())
}

/// POST /api/live/channels/{id}/watch `{ client_id?, keyed_urls?, take_over?, controls?, device_kind? }`.
fn watch(cx: &Cx<'_>, state: &mut State) -> Answer {
    let body = cx.req.json().unwrap_or(Value::Null);
    let client = body["client_id"].as_str().filter(|c| (1..=128).contains(&c.len())).map(str::to_string);
    let keyed = body["keyed_urls"].as_bool().unwrap_or(false);
    let player = Player {
        controls: controls_of(&body["controls"]),
        device_kind: body["device_kind"].as_str().map(str::to_string),
        stream: None,
    };
    let (session, url, id) = start_watching(cx, state, client, keyed, body["take_over"].as_i64(), player)?;
    let profile = state.profile(cx.caller().profile);
    let channel = state.live.channel(id).expect("checked");
    Ok(json(
        200,
        &json!({ "session_id": session, "url": url, "kind": "hls", "channel": state.live.json(channel, profile, cx.now()) }),
    ))
}

/// GET /api/live/channels/{id}/stream.ts `?client_id=&take_over=&device_kind=`: the channel's MPEG-TS for as
/// long as it is read, a session of its own (the server's `livetv::stream`): here the sample
/// segments one after another, each when its time comes.
fn stream(cx: &Cx<'_>) -> Answer {
    let segments: Vec<Arc<[u8]>> = (0..HLS_SEGMENTS).filter_map(|n| cx.shared.media.segment(n)).collect();
    if segments.is_empty() {
        return Err(not_found("the sample media aren't here"));
    }
    let client = cx.query("client_id").filter(|c| (1..=128).contains(&c.len())).map(str::to_string);
    let take_over = cx.query("take_over").and_then(|t| t.parse().ok());
    let gone = Arc::new(AtomicBool::new(false));
    {
        let mut state = lock(&cx.shared.state);
        // A reader that watches by itself obeys no remote.
        let player = Player {
            controls: Vec::new(),
            device_kind: cx.query("device_kind").map(str::to_string),
            stream: Some(gone.clone()),
        };
        start_watching(cx, &mut state, client, false, take_over, player)?;
    }
    let end = StreamEnd(gone);
    let run = move |out: &mut crate::http::ChunkWriter<'_>| {
        let end = end;
        for segment in segments.iter().cycle() {
            if end.0.load(Ordering::SeqCst) {
                break;
            }
            out.send(segment)?;
            // A segment's time, waited in steps so a session ended here ends the stream soon.
            for _ in 0..HLS_SEGMENT_MS / 100 {
                if end.0.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        Ok(())
    };
    Ok(Response::new(200)
        .with_header("Content-Type", "video/mp2t")
        .with_header("Cache-Control", "no-store")
        .with_body(Body::Stream(Box::new(run))))
}

/// What a viewer's player said of itself.
struct Player {
    controls: Vec<String>,
    device_kind: Option<String>,
    /// A stream.ts reader's flag.
    stream: Option<Arc<AtomicBool>>,
}

/// Starts the caller watching the channel of the path, as `watch` and `stream.ts` both do: the
/// session, its playlist's address and the channel.
fn start_watching(
    cx: &Cx<'_>,
    state: &mut State,
    client: Option<String>,
    keyed: bool,
    take_over: Option<i64>,
    player: Player,
) -> Result<(String, String, i64), Response> {
    let Player { controls, device_kind, stream } = player;
    open(cx, state)?;
    let id = channel(cx, state)?.id;
    let me = cx.caller().profile;
    let live = &mut state.live;
    live.forget_gone_streams();
    // What this device watched before ends: the same client id, and its stream reader of
    // another channel (the server's rule).
    if let Some(client) = &client {
        let old: Vec<String> = live
            .sessions
            .iter()
            .filter(|(_, v)| {
                v.profile == me
                    && v.client_id.as_deref().is_some_and(|theirs| {
                        theirs == client || (device_of(theirs) == device_of(client) && v.channel != id)
                    })
            })
            .map(|(k, _)| k.clone())
            .collect();
        for old in old {
            live.remove(&old);
        }
    }
    let channel = live.channel(id).expect("checked");
    let name = channel.name.clone();
    let source = channel.group().source;
    let (source_name, limit) =
        SOURCES.iter().find(|s| s.0 == source).map(|s| (s.1, s.2)).expect("every source is known");
    let tuned = live.tuned(source);
    if let Some(limit) = limit
        && !tuned.contains(&id)
        && tuned.len() >= limit
    {
        let take = take_over.filter(|t| tuned.contains(t));
        let viewers_of = |live: &Live, channel: i64| -> Vec<(String, i64)> {
            live.sessions.iter().filter(|(_, v)| v.channel == channel).map(|(k, v)| (k.clone(), v.profile)).collect()
        };
        match take {
            Some(taken) => {
                let viewers = viewers_of(live, taken);
                if viewers.iter().any(|(_, p)| *p != me) {
                    return Err(forbidden(
                        "someone else watches that channel: only an administrator takes it over, from those who aren't",
                    ));
                }
                // As the server: each viewer hears why before its stream ends.
                let message = format!("Watching moved to {name} on another device.");
                for (session, profile) in viewers {
                    let event = ServerEvent::LiveTakenOver {
                        session_id: session.clone(),
                        channel_id: taken,
                        message: message.clone(),
                    };
                    cx.shared.events.emit_to(profile, &event);
                    live.end(&session, Some(message.clone()));
                }
            }
            None => {
                let connections: Vec<Value> = tuned
                    .iter()
                    .map(|channel| {
                        let viewers = viewers_of(live, *channel);
                        let people: HashSet<i64> = viewers.iter().map(|(_, p)| *p).collect();
                        let yours = people.len() == 1 && people.contains(&me);
                        json!({
                            "channel_id": channel,
                            "channel_name": live.channel(*channel).map(|c| c.name.clone()),
                            "watchers": [],
                            "people": people.len(),
                            "yours": yours,
                            "can_take_over": yours,
                        })
                    })
                    .collect();
                let watching = tuned.first().and_then(|c| live.channel(*c)).map(|c| c.name.clone()).unwrap_or_default();
                let message =
                    format!("{source_name} allows one connection at once, and it is in use: watching {watching}.");
                return Err(json(409, &json!({ "code": "conflict", "message": message, "connections": connections })));
            }
        }
    }
    let session = hex_id();
    let key = (keyed && stream.is_none()).then(|| format!("pley_mk_{}", token(32)));
    let url = match &key {
        Some(key) => format!("/api/live/media/{key}/index.m3u8"),
        None => format!("/api/live/sessions/{session}/index.m3u8"),
    };
    if let Some(key) = &key {
        live.keys.insert(key.clone(), session.clone());
    }
    live.sessions.insert(
        session.clone(),
        Viewer {
            profile: me,
            channel: id,
            client_id: client,
            key,
            started_ms: cx.now_ms(),
            stream,
            controls,
            device_kind,
        },
    );
    let profile = state.profile_mut(me);
    profile.live_recent.retain(|c| *c != id);
    profile.live_recent.insert(0, id);
    profile.live_recent.truncate(24);
    cx.shared.events.emit(&ServerEvent::LiveTunersChanged);
    Ok((session, url, id))
}

/// A live session's playlist or segment: a sliding window over the sample
/// segments, said again and again, each wrap a discontinuity.
fn live_file(cx: &Cx<'_>, name: &str, started_ms: i64) -> Answer {
    // Two segments exist when the session starts, as the server waits for
    // 2 s of media before the first playlist.
    let last = 2 + (cx.now_ms() - started_ms).max(0) / HLS_SEGMENT_MS;
    let first = (last - WINDOW + 1).max(0);
    let segments = HLS_SEGMENTS as i64;
    if name == "index.m3u8" {
        let mut text = format!(
            "#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:{}\n#EXT-X-MEDIA-SEQUENCE:{first}\n#EXT-X-DISCONTINUITY-SEQUENCE:{}\n",
            HLS_SEGMENT_MS / 1_000,
            first / segments,
        );
        for n in first..=last {
            if n > first && n % segments == 0 {
                text.push_str("#EXT-X-DISCONTINUITY\n");
            }
            text.push_str(&format!("#EXTINF:{:.3},\ns{n}.ts\n", HLS_SEGMENT_MS as f64 / 1_000.0));
        }
        return Ok(Response::bytes(200, "application/vnd.apple.mpegurl", text.into_bytes())
            .with_header("Cache-Control", "no-cache"));
    }
    let n: i64 = name
        .strip_prefix('s')
        .and_then(|n| n.strip_suffix(".ts"))
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| not_found(&format!("no file {name}")))?;
    if n > last || n < first - 60 {
        return Err(not_found(&format!("no file {name}")));
    }
    let data =
        cx.shared.media.segment((n % segments) as usize).ok_or_else(|| not_found("the sample media aren't here"))?;
    let len = data.len();
    Ok(Response::new(200)
        .with_header("Content-Type", "video/mp2t")
        .with_header("Cache-Control", "private, max-age=60")
        .with_body(Body::Shared(data, 0..len)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lineup_has_what_the_ids_say() {
        let live = Live::new(0);
        assert_eq!(live.channels.len() as i64, ids::LIVE_CHANNELS);
        assert_eq!(live.channel(ids::LIVE_BUSY_CHANNEL).unwrap().group().source, 2);
        assert_eq!(live.channel(ids::LIVE_CONFLICT_CHANNEL).unwrap().group().source, 2);
        assert!(!live.channel(ids::LIVE_NO_GUIDE_CHANNEL).unwrap().has_guide);
        assert_eq!(live.channel(ids::LIVE_CHANNEL).unwrap().group().genre as u8, Genre::News as u8);
        assert!(live.channel(ids::LIVE_CHANNELS).unwrap().number.is_none());
        assert_eq!(live.tuned(2), vec![ids::LIVE_BUSY_CHANNEL]);
    }

    #[test]
    fn the_guide_tiles_time_without_gaps() {
        let live = Live::new(0);
        let channel = live.channel(5).unwrap();
        let from = 1_793_400_017;
        let list = programs(channel, from, from + 86_400);
        assert!(list.first().unwrap().start <= from);
        assert!(list.last().unwrap().stop >= from + 86_400);
        for pair in list.windows(2) {
            assert_eq!(pair[0].stop, pair[1].start);
        }
        // The same span asked in two halves gives the same programmes.
        let halves: Vec<i64> = programs(channel, from, from + 43_200)
            .into_iter()
            .chain(programs(channel, from + 43_200, from + 86_400))
            .map(|p| p.start)
            .collect();
        let mut halves = halves;
        halves.dedup();
        assert_eq!(halves, list.iter().map(|p| p.start).collect::<Vec<_>>());
        let (now, next) = now_and_next(channel, from);
        assert!(now.as_ref().unwrap().start <= from && now.as_ref().unwrap().stop > from);
        assert_eq!(next.unwrap().start, now.unwrap().stop);
    }
}
