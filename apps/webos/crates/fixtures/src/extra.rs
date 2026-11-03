//! Automatic's network memory and its speed test, and subtitles found
//! online (docs/API.md): GET and PUT /api/playback/network as the server
//! keeps them (a ceiling for 24 h, a speed test for 7 days at 70 %), GET
//! /api/playback/speedtest, GET /api/items/{id}/subtitles/search (two results
//! per language, or `503` "not set up"), and adding and deleting what was
//! found, whose tracks the subtitle route then serves.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use serde_json::{Value, json};

use crate::http::{Body, Response};
use crate::model::Kind;
use crate::play::add_subtitle;
use crate::random::unique_u64;
use crate::server::{
    Answer, Cx, Need, PLAY, Route, State, bad_request, error, forbidden, free, json, locked, no_content, not_found,
};

const DAY_MS: i64 = 24 * 3_600_000;
const WEEK_MS: i64 = 7 * DAY_MS;
const DEFAULT_BYTES: u64 = 33_554_432;
const MIN_BYTES: u64 = 65_536;
const MAX_BYTES: u64 = 67_108_864;

/// What the server remembers of each client's network.
pub(crate) struct NetworkMemory {
    /// The clients play from another network than the server's: a speed test
    /// is due when nothing is known.
    pub remote: bool,
    /// The routes are there; off, they answer as an older server's do.
    pub available: bool,
    /// By profile and client: kbps and when.
    ceilings: HashMap<(i64, String), (i64, i64)>,
    /// By profile and client: kbps, whether it was complete, and when.
    probes: HashMap<(i64, String), (i64, bool, i64)>,
}

impl Default for NetworkMemory {
    fn default() -> NetworkMemory {
        NetworkMemory { remote: false, available: true, ceilings: HashMap::new(), probes: HashMap::new() }
    }
}

impl NetworkMemory {
    /// GET's answer: a ceiling of the last 24 h, else 70 % of a speed test of
    /// the last 7 days; on the server's own network, never a test.
    fn answer(&self, key: &(i64, String), now: i64) -> Value {
        let local = !self.remote;
        if let Some((kbps, at)) = self.ceilings.get(key).filter(|(_, at)| now - at < DAY_MS) {
            return network(local, Some(*kbps), Some("playback"), Some(*at), false);
        }
        if local {
            return network(true, None, None, None, false);
        }
        match self.probes.get(key).filter(|(_, _, at)| now - at < WEEK_MS) {
            Some((_, true, _)) => network(false, None, None, None, false),
            Some((kbps, false, at)) => network(false, Some(kbps * 7 / 10), Some("probe"), Some(*at), false),
            None => network(false, None, None, None, true),
        }
    }
}

fn network(local: bool, ceiling: Option<i64>, source: Option<&str>, since: Option<i64>, probe: bool) -> Value {
    json!({ "local": local, "ceiling_kbps": ceiling, "source": source, "since": since, "probe": probe })
}

pub(crate) fn routes() -> Vec<Route> {
    vec![
        locked("GET", "/api/playback/network", Need::Scope(PLAY), |cx, state| {
            if !state.network.available {
                return Err(not_found("playback session expired"));
            }
            let client = client(cx.query("client_id"))?;
            let key = (cx.caller().profile, client);
            Ok(json(200, &state.network.answer(&key, cx.now_ms())))
        }),
        locked("PUT", "/api/playback/network", Need::Scope(PLAY), put_network),
        free("GET", "/api/playback/speedtest", Need::Scope(PLAY), |cx| {
            let n = match cx.query("bytes") {
                Some(v) => v.parse::<u64>().map_err(|_| bad_request("bytes must be a number"))?,
                None => DEFAULT_BYTES,
            }
            .clamp(MIN_BYTES, MAX_BYTES);
            Ok(Response::new(200)
                .with_header("Content-Type", "application/octet-stream")
                .with_header("Cache-Control", "no-store, no-transform")
                .with_body(Body::Repeat(noise(), n)))
        }),
        locked("GET", "/api/items/*/subtitles/search", Need::Scope(PLAY), search),
        locked("POST", "/api/items/*/subtitles", Need::Scope(PLAY), add),
        locked("DELETE", "/api/items/*/subtitles/*", Need::Scope(PLAY), delete),
    ]
}

fn client(value: Option<&str>) -> Result<String, Response> {
    value.filter(|c| (1..=128).contains(&c.len())).map(str::to_string).ok_or_else(|| bad_request("client_id is needed"))
}

/// 64 KiB that don't compress, repeated for the speed test.
fn noise() -> Arc<[u8]> {
    static NOISE: OnceLock<Arc<[u8]>> = OnceLock::new();
    NOISE
        .get_or_init(|| {
            let mut bytes = Vec::with_capacity(65_536);
            while bytes.len() < 65_536 {
                bytes.extend_from_slice(&unique_u64().to_le_bytes());
            }
            bytes.into()
        })
        .clone()
}

/// PUT /api/playback/network: a speed test's result, a ceiling learnt while
/// playing, or `null` to forget it.
fn put_network(cx: &Cx<'_>, state: &mut State) -> Answer {
    if !state.network.available {
        return Ok(Response::new(405));
    }
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't JSON"))?;
    let key = (cx.caller().profile, client(body["client_id"].as_str())?);
    let now = cx.now_ms();
    let speed = |value: &Value| {
        value
            .as_f64()
            .filter(|k| (1.0..=10_000_000.0).contains(k))
            .map(|k| k.round() as i64)
            .ok_or_else(|| bad_request("a speed is 1 to 10,000,000 kbps"))
    };
    let memory = &mut state.network;
    if let Some(probe) = body.get("probe_kbps") {
        let kbps = speed(probe)?;
        let complete = body["probe_complete"].as_bool().unwrap_or(false);
        memory.probes.insert(key.clone(), (kbps, complete, now));
    } else if let Some(ceiling) = body.get("ceiling_kbps") {
        if ceiling.is_null() {
            memory.ceilings.remove(&key);
            memory.probes.insert(key.clone(), (0, true, now));
        } else {
            memory.ceilings.insert(key.clone(), (speed(ceiling)?, now));
        }
    } else {
        return Err(bad_request("send probe_kbps or ceiling_kbps"));
    }
    Ok(json(200, &memory.answer(&key, now)))
}

/// GET /api/items/{id}/subtitles/search: two results per language asked
/// (the profile's subtitle language, else its audio's, else English).
fn search(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let item = cx.catalog().item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    if !matches!(item.kind(), Kind::Movie | Kind::Episode | Kind::Extra) {
        return Err(bad_request("subtitles are searched for movies, episodes and extras"));
    }
    if !state.subtitle_search {
        return Err(json(
            503,
            &json!({ "code": "unavailable", "message": "Subtitle search isn't set up on this server", "setup": false }),
        ));
    }
    let defaults = &state.profile(cx.caller().profile).track_defaults;
    let fallback = defaults.subtitle_language.clone().or(defaults.audio_language.clone()).unwrap_or("en".into());
    let languages: Vec<String> = match cx.query("language") {
        Some(list) if !list.trim().is_empty() => {
            list.split(',').map(str::trim).filter(|l| !l.is_empty()).take(20).map(str::to_string).collect()
        }
        _ => vec![fallback],
    };
    let title = &item.base.title;
    let results: Vec<Value> = languages
        .iter()
        .flat_map(|language| {
            [
                json!({
                    "id": format!("opensubtitles:{}1:{language}:srt", id), "provider": "opensubtitles",
                    "language": language, "name": format!("{title}.1080p.BluRay.x264-GRP"), "format": "srt",
                    "hearing_impaired": false, "forced": false, "downloads": 1234, "matched": "hash",
                }),
                json!({
                    "id": format!("opensubtitles:{}2:{language}:hi:srt", id), "provider": "opensubtitles",
                    "language": language, "name": format!("{title}.720p.WEB"), "format": "srt",
                    "hearing_impaired": true, "forced": false, "downloads": 56, "matched": "title",
                }),
            ]
        })
        .collect();
    Ok(json(200, &json!({ "results": results })))
}

/// POST /api/items/{id}/subtitles `{ "result", "file_id"? }`: `201` with the
/// new track.
fn add(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let item = cx.catalog().item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    let body = cx.req.json().filter(Value::is_object).ok_or_else(|| bad_request("the body isn't a subtitle to add"))?;
    let result = body["result"].as_str().ok_or_else(|| bad_request("result is needed"))?;
    let mut parts = result.split(':');
    if parts.next() != Some("opensubtitles") {
        return Err(not_found("unknown subtitle"));
    }
    let language = parts.nth(1).unwrap_or("en").to_string();
    let file = match body["file_id"].as_i64() {
        Some(file) => item.files.iter().find(|f| f.id == file),
        None => item.files.first(),
    }
    .ok_or_else(|| not_found("file not found"))?;
    if !state.subtitle_search {
        return Err(error(503, "unavailable", "Subtitle search isn't set up on this server"));
    }
    let me = cx.caller().profile;
    let added = add_subtitle(state, file.id, me, &language);
    Ok(json(201, &added.track(me)))
}

/// DELETE /api/items/{id}/subtitles/{index}: only who added it.
fn delete(cx: &Cx<'_>, state: &mut State) -> Answer {
    let id = cx.id(0)?;
    let index = cx.id(1)?;
    let item = cx.catalog().item(id).ok_or_else(|| not_found(&format!("item {id} not found")))?;
    let me = cx.caller().profile;
    for file in &item.files {
        let Some(list) = state.added_subtitles.get_mut(&file.id) else { continue };
        let Some(at) = list.iter().position(|a| a.index == index) else { continue };
        if list[at].profile != me {
            return Err(forbidden("only who added a subtitle deletes it"));
        }
        list.remove(at);
        return Ok(no_content());
    }
    Err(not_found("subtitle not found"))
}
