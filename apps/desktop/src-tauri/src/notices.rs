//! The system's notifications for requests, while the window isn't in
//! front: a movie or show someone asked for is ready to watch, or was
//! approved or declined; for administrators, someone asked for one, or a
//! request couldn't be sent. The app hears of them on the event stream of
//! the server the window shows (`/api/events`), with the window's session,
//! and decides itself what to show: the server's pages get nothing from it.
//! While the window is in front, the page tells of them itself. After a
//! dropped stream (the computer slept, the network went), the news of the
//! account's requests that came meanwhile (ready to watch, all of it or
//! more of a show's seasons, or declined) is told of in one notice. Notices
//! are in the language the server's pages are in: the account's, else the
//! computer's, else the server's (`words`), looked at each time the stream
//! opens.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::watch;
use url::Url;

use crate::words::Language;

/// How long the server has to start answering.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);
/// The server sends something every 15 s, a comment at least: a stream
/// silent for this long has dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(45);
/// After a dropped stream, the app tries again after this, then twice as
/// long each time up to [`RETRY_MAX`].
const RETRY_FIRST: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(60);
/// An event longer than this isn't one the app knows: skipped.
const MAX_EVENT: usize = 64 * 1024;
/// The computer's languages kept, at most, and their tags' length.
const MAX_LANGUAGES: usize = 16;
const MAX_TAG: usize = 35;

/// The server the window shows, which the notices follow, and the
/// computer's languages, as the picker's web view has them.
pub struct Notices {
    shown: watch::Sender<Option<Url>>,
    languages: Mutex<Vec<String>>,
}

impl Default for Notices {
    fn default() -> Self {
        Self { shown: watch::channel(None).0, languages: Mutex::default() }
    }
}

impl Notices {
    /// The window shows `server` now (`None`: the picker).
    pub fn follow(&self, server: Option<Url>) {
        self.shown.send_if_modified(|shown| {
            let changed = *shown != server;
            *shown = server;
            changed
        });
    }

    /// The computer's languages, preferred first (`navigator.languages`).
    pub fn languages_are(&self, tags: Vec<String>) {
        let tags = tags.into_iter().filter(|tag| tag.len() <= MAX_TAG).take(MAX_LANGUAGES).collect();
        *self.languages.lock().unwrap() = tags;
    }

    fn languages(&self) -> Vec<String> {
        self.languages.lock().unwrap().clone()
    }
}

/// Who asked, as requests name people.
#[derive(Debug, PartialEq, Deserialize)]
struct Person {
    name: String,
}

/// The events notices are made of (apps/server/src/events.rs); the others
/// are of no use here.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Heard {
    /// Something this account asked for, or follows, is ready to watch:
    /// all of it, or (`complete` false) the seasons of a show in `seasons`.
    RequestAvailable {
        request_id: i64,
        title: String,
        #[serde(default)]
        seasons: Option<Vec<i64>>,
        #[serde(default = "whole")]
        complete: bool,
    },
    /// An administrator approved or declined it (no reason for followers).
    RequestDecided {
        request_id: i64,
        title: String,
        status: String,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Administrators: someone asked for a title.
    RequestCreated {
        title: String,
        #[serde(default)]
        seasons: Option<Vec<i64>>,
        requested_by: Person,
        /// "pending", or "approved" as it was made (nothing to decide then).
        #[serde(default)]
        status: Option<String>,
    },
    /// Administrators: a request couldn't be sent to Radarr or Sonarr.
    RequestFailed {
        title: String,
        kind: String,
        #[serde(default)]
        reason: Option<String>,
        /// When the server tries again by itself: no notice until it gives up.
        #[serde(default)]
        retry_at: Option<i64>,
    },
    #[serde(other)]
    Other,
}

fn whole() -> bool {
    true
}

fn heard(data: &str) -> Heard {
    serde_json::from_str(data).unwrap_or(Heard::Other)
}

/// `reason` as notices give it: none when it's blank.
fn given(reason: Option<&str>) -> Option<&str> {
    reason.map(str::trim).filter(|r| !r.is_empty())
}

/// A title, or a reason, as running text quotes it: “Heat” (« Heat » in French).
fn quoted(language: Language, text: &str) -> String {
    language.say("quoted", &[("title", text)])
}

/// `text` ending as a sentence does.
fn sentence(text: &str) -> String {
    if text.ends_with(['.', '!', '?']) { text.to_owned() } else { format!("{text}.") }
}

/// "“Heat” is ready to watch.", or for a show's first seasons, "Season 2 of
/// “The Expanse” is ready to watch." (`readyText` in apps/web/src/lib/requests.ts).
fn ready_words(language: Language, title: &str, seasons: Option<&[i64]>) -> String {
    let title = quoted(language, title);
    let Some(seasons) = seasons.filter(|s| !s.is_empty()) else {
        return language.say("ready", &[("title", &title)]);
    };
    // Specials alone have words of their own: they agree as a plural in French and Spanish.
    if seasons.iter().all(|&n| n == 0) {
        return language.say("specialsReady", &[("title", &title)]);
    }
    let label = seasons_label(language, Some(seasons)).unwrap_or_default();
    language.say_counted("seasonsReady", seasons.len(), &[("seasons", &label), ("title", &title)])
}

fn declined_words(language: Language, title: &str, reason: Option<&str>) -> String {
    let title = quoted(language, title);
    match given(reason) {
        Some(reason) => language.say("declinedBecause", &[("title", &title), ("reason", &quoted(language, reason))]),
        None => language.say("declined", &[("title", &title)]),
    }
}

/// What the notice for `heard` says, if there is one (the words every
/// client uses, docs/API.md "Requests").
fn notice(language: Language, heard: &Heard) -> Option<String> {
    Some(match heard {
        Heard::RequestAvailable { title, seasons, complete, .. } => {
            ready_words(language, title, if *complete { None } else { seasons.as_deref() })
        }
        Heard::RequestDecided { title, status, reason, .. } => match status.as_str() {
            "approved" => language.say("approved", &[("title", &quoted(language, title))]),
            "declined" => declined_words(language, title, reason.as_deref()),
            _ => return None,
        },
        Heard::RequestCreated { title, seasons, requested_by, status } => {
            if status.as_deref().is_some_and(|s| s != "pending") {
                return None;
            }
            let (name, title) = (requested_by.name.as_str(), quoted(language, title));
            match seasons_label(language, seasons.as_deref()) {
                Some(seasons) => {
                    language.say("createdSeasons", &[("name", name), ("title", &title), ("seasons", &seasons)])
                }
                None => language.say("created", &[("name", name), ("title", &title)]),
            }
        }
        Heard::RequestFailed { title, kind, reason, retry_at } => {
            if retry_at.is_some() {
                return None;
            }
            let title = quoted(language, title);
            let service = if kind == "show" { "Sonarr" } else { "Radarr" };
            match given(reason.as_deref()) {
                Some(reason) => language
                    .say("notSentBecause", &[("title", &title), ("service", service), ("reason", &sentence(reason))]),
                None => language.say("notSent", &[("title", &title), ("service", service)]),
            }
        }
        Heard::Other => return None,
    })
}

/// A show's seasons as the web says them (`seasonsLabel` in
/// apps/web/src/lib/requests.ts): "Season 2", "Seasons 1–3, 5",
/// "Seasons 1–2 and specials", "Specials"; `None` for none (a movie).
fn seasons_label(language: Language, seasons: Option<&[i64]>) -> Option<String> {
    let seasons = seasons.filter(|s| !s.is_empty())?;
    let mut numbers: Vec<i64> = seasons.iter().copied().filter(|&n| n > 0).collect();
    numbers.sort_unstable();
    numbers.dedup();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < numbers.len() {
        let start = numbers[i];
        while i + 1 < numbers.len() && numbers[i + 1] == numbers[i] + 1 {
            i += 1;
        }
        let end = numbers[i];
        ranges.push(if start == end { start.to_string() } else { format!("{start}–{end}") });
        i += 1;
    }
    if ranges.is_empty() {
        return Some(language.say("specials", &[]));
    }
    let label = language.say_counted("seasonList", numbers.len(), &[("list", &ranges.join(", "))]);
    Some(if seasons.contains(&0) { language.say("andSpecials", &[("seasons", &label)]) } else { label })
}

/// Splits a `text/event-stream` into the data of its events.
#[derive(Debug, Default)]
struct EventStream {
    line: Vec<u8>,
    data: Option<String>,
    /// The event being read is too long: dropped at its end.
    too_long: bool,
}

impl EventStream {
    /// Reads `chunk`: the data of each event it ends.
    fn read(&mut self, chunk: &[u8]) -> Vec<String> {
        let mut events = Vec::new();
        for &byte in chunk {
            if byte != b'\n' {
                if !self.too_long {
                    self.line.push(byte);
                    self.too_long = self.line.len() + self.data.as_ref().map_or(0, String::len) > MAX_EVENT;
                }
                continue;
            }
            let mut line = std::mem::take(&mut self.line);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                // A blank line ends an event.
                let data = self.data.take();
                if !std::mem::take(&mut self.too_long) {
                    events.extend(data);
                }
                continue;
            }
            if self.too_long {
                continue;
            }
            // Only the data matters here: comments (":") keep the stream
            // alive, and events' names, ids and retry times aren't used.
            let line = String::from_utf8_lossy(&line);
            if let Some(value) = line.strip_prefix("data:") {
                let value = value.strip_prefix(' ').unwrap_or(value);
                match &mut self.data {
                    Some(data) => {
                        data.push('\n');
                        data.push_str(value);
                    }
                    None => self.data = Some(value.to_owned()),
                }
            }
        }
        events
    }
}

/// A request as the server lists it: with news for the account (ready to
/// watch, partly, or declined since it last looked), or not.
#[derive(Debug, Deserialize)]
struct Listed {
    id: i64,
    title: String,
    /// The decision: "pending", "approved", "declined", "failed" or "available".
    status: String,
    stage: String,
    #[serde(default)]
    available_seasons: Vec<i64>,
    #[serde(default)]
    reason: Option<String>,
    updated_at: i64,
    /// Older servers have no such thing: then nothing is news.
    #[serde(default)]
    unseen: bool,
}

impl Listed {
    /// Its news, as a notice tells it: all of it ready to watch, some of a
    /// show's seasons (while others still download, too), or declined.
    fn news(&self, language: Language) -> Option<String> {
        match self.status.as_str() {
            "available" if self.stage != "gone" => Some(ready_words(language, &self.title, None)),
            "approved" if !self.available_seasons.is_empty() => {
                Some(ready_words(language, &self.title, Some(&self.available_seasons)))
            }
            "declined" => Some(declined_words(language, &self.title, self.reason.as_deref())),
            _ => None,
        }
    }

    fn has_news(&self) -> bool {
        self.news(Language::En).is_some()
    }
}

#[derive(Deserialize)]
struct Requests {
    requests: Vec<Listed>,
}

/// How a request stood when last told of, or listed: its status, and the
/// seasons of a show ready then.
#[derive(Debug, Default, PartialEq)]
struct Known {
    status: String,
    seasons: Vec<i64>,
}

impl Known {
    fn of(request: &Listed) -> Self {
        Self { status: request.status.clone(), seasons: request.available_seasons.clone() }
    }

    /// `request`'s news is told already: its status is the same, and, while
    /// it is partly ready, no season of it is ready that wasn't then.
    fn covers(&self, request: &Listed) -> bool {
        self.status == request.status
            && (request.status != "approved" || request.available_seasons.iter().all(|n| self.seasons.contains(n)))
    }
}

/// What the notices of one server have told of.
#[derive(Debug, Default)]
struct Told {
    /// The latest change among the requests with news, as listed when the
    /// stream opened; `None` until it first opens, which tells of nothing
    /// old (the page does, as it opens).
    newest: Option<i64>,
    /// The requests told of as they changed, or listed when a stream
    /// opened, as they stood then: not told of twice, but told of again
    /// once they move on, or more of a show's seasons are ready.
    known: HashMap<i64, Known>,
}

impl Told {
    /// What `listed` (the account's requests with news, as listed when a
    /// stream opens) has that wasn't told of: nothing the first time.
    fn catch_up<'a>(&mut self, listed: &'a [Listed]) -> Vec<&'a Listed> {
        let listed: Vec<&Listed> = listed.iter().filter(|r| r.unseen).collect();
        let latest = listed.iter().map(|r| r.updated_at).max();
        let news: Vec<&Listed> = match self.newest {
            None => Vec::new(),
            Some(newest) => listed
                .iter()
                .copied()
                .filter(|r| r.updated_at > newest && r.has_news())
                .filter(|r| !self.known.get(&r.id).is_some_and(|known| known.covers(r)))
                .collect(),
        };
        let newest = self.newest.unwrap_or(0);
        self.newest = Some(latest.map_or(newest, |latest| latest.max(newest)));
        self.known.extend(listed.iter().map(|r| (r.id, Known::of(r))));
        news
    }

    /// A request heard of as it changed: ready (all of it, or the seasons
    /// of a show in `seasons`), or decided (`status`).
    fn heard(&mut self, id: i64, status: &str, seasons: &[i64]) {
        let known = self.known.entry(id).or_default();
        status.clone_into(&mut known.status);
        for &n in seasons {
            if !known.seasons.contains(&n) {
                known.seasons.push(n);
            }
        }
    }
}

/// The notice for what came while the stream was down: the request's own
/// words when there is one, else how many have news.
fn caught_up(language: Language, news: &[&Listed]) -> Option<String> {
    match news {
        [] => None,
        [one] => one.news(language),
        several => Some(language.say_counted("news", several.len(), &[])),
    }
}

/// A notice's words as the system's notifications take them. On Linux and
/// the BSDs they go over D-Bus as they are, and notification servers read
/// the body as markup (`body-markup`): a reason or a name with `<` or `&`
/// in it is escaped there, to show as written. Windows' toasts escape it
/// themselves, and macOS takes plain text.
fn body_text(body: &str) -> String {
    if cfg!(all(unix, not(target_os = "macos"))) { escape_markup(body) } else { body.to_owned() }
}

fn escape_markup(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// What the notices need of the app: kept apart so that the tests run
/// without a window.
trait Shell: Send + Sync {
    /// Whether the window is in front: its page tells of what happens then.
    fn in_front(&self) -> bool;
    fn show(&self, title: &str, body: &str);
    /// The window's cookies for `server`: its session there.
    fn session(&self, server: &Url) -> Option<String>;
    fn proofs(&self) -> &crate::proofs::Proofs;
    /// Another device answers at `server`'s address now.
    fn not_yours(&self, server: &Url);
    /// The computer's languages, preferred first.
    fn languages(&self) -> Vec<String>;
}

struct App<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Shell for App<R> {
    fn in_front(&self) -> bool {
        let Some(window) = self.0.get_webview_window("main") else { return false };
        window.is_visible().unwrap_or(false)
            && !window.is_minimized().unwrap_or(false)
            && window.is_focused().unwrap_or(false)
    }

    fn show(&self, title: &str, body: &str) {
        if let Err(err) = self.0.notification().builder().title(title).body(body_text(body)).show() {
            eprintln!("cannot show a notification: {err}");
        }
    }

    fn session(&self, server: &Url) -> Option<String> {
        crate::downloads::cookies(&self.0, server)
    }

    fn proofs(&self) -> &crate::proofs::Proofs {
        self.0.state::<crate::proofs::Proofs>().inner()
    }

    fn not_yours(&self, server: &Url) {
        crate::not_yours(&self.0, server);
    }

    fn languages(&self) -> Vec<String> {
        self.0.state::<Notices>().languages()
    }
}

/// How a stream ended.
#[derive(Debug, PartialEq)]
enum Ended {
    /// The server can't be reached, or dropped the stream.
    Dropped,
    /// The window isn't signed in there (or the server has no events):
    /// tried again in a while, with the window's session then.
    Refused,
}

fn get(client: &reqwest::Client, url: Url, session: Option<&str>) -> reqwest::RequestBuilder {
    let request = client.get(url);
    match session {
        Some(cookie) => request.header(reqwest::header::COOKIE, cookie),
        None => request,
    }
}

/// `path` on `server`, read as JSON (`None`: no answer, or a refusal).
async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    server: &Url,
    path: &str,
    session: Option<&str>,
) -> Option<T> {
    let url = server.join(path).ok()?;
    let response = tokio::time::timeout(ANSWER_TIMEOUT, get(client, url, session).send()).await.ok()?.ok()?;
    if !response.status().is_success() {
        return None;
    }
    tokio::time::timeout(ANSWER_TIMEOUT, response.json()).await.ok()?.ok()
}

/// The account's requests with news, latest first (`None`: the server
/// doesn't list them for it, requests being off for instance).
async fn listed_news(client: &reqwest::Client, server: &Url, session: Option<&str>) -> Option<Vec<Listed>> {
    let list: Requests = get_json(client, server, "/api/requests?unseen=1&limit=20", session).await?;
    Some(list.requests)
}

/// An account's or a server's language (`User.language`, `/api/server`'s
/// `language`): none for Automatic.
#[derive(Deserialize)]
struct LanguageOf {
    #[serde(default)]
    language: Option<String>,
}

/// The language the account sees `server`'s pages in: its own, else the
/// computer's (`device`), else the server's, which is asked for only then.
async fn language_of(client: &reqwest::Client, server: &Url, session: Option<&str>, device: &[String]) -> Language {
    let account = get_json::<LanguageOf>(client, server, "/api/auth/me", session).await.and_then(|me| me.language);
    if account.as_deref().and_then(Language::of).is_some() || Language::of_device(device).is_some() {
        return Language::pick(account.as_deref(), device, None);
    }
    let theirs = get_json::<LanguageOf>(client, server, "/api/server", None).await.and_then(|s| s.language);
    Language::pick(None, device, theirs.as_deref())
}

/// Listens to `server`'s events once, telling of them, until the stream ends.
async fn listen(client: &reqwest::Client, server: &Url, shell: &dyn Shell, name: &str, told: &mut Told) -> Ended {
    let Ok(url) = server.join("/api/events") else { return Ended::Refused };
    let session = match crate::proofs::session(server, shell.proofs(), shell.session(server)).await {
        Ok(session) => session,
        Err(crate::proofs::Refused::Unreachable(_)) => return Ended::Dropped,
        Err(crate::proofs::Refused::NotYours | crate::proofs::Refused::Misnamed(_)) => {
            shell.not_yours(server);
            return Ended::Refused;
        }
    };
    let language = language_of(client, server, session.as_deref(), &shell.languages()).await;
    let request = get(client, url, session.as_deref())
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .header(reqwest::header::ACCEPT_LANGUAGE, language.code());
    let mut response = match tokio::time::timeout(ANSWER_TIMEOUT, request.send()).await {
        Ok(Ok(response)) => response,
        _ => return Ended::Dropped,
    };
    match response.status().as_u16() {
        200 => {}
        401 | 403 | 404 => return Ended::Refused,
        _ => return Ended::Dropped,
    }
    // What came while the stream was down (nothing, the first time).
    let news = match listed_news(client, server, session.as_deref()).await {
        Some(listed) => caught_up(language, &told.catch_up(&listed)),
        None => None,
    };
    if let Some(body) = news.filter(|_| !shell.in_front()) {
        shell.show(name, &body);
    }
    let mut stream = EventStream::default();
    loop {
        let chunk = match tokio::time::timeout(READ_TIMEOUT, response.chunk()).await {
            Ok(Ok(Some(chunk))) => chunk,
            _ => return Ended::Dropped,
        };
        for data in stream.read(&chunk) {
            let heard = heard(&data);
            match notice(language, &heard) {
                Some(body) if !shell.in_front() => shell.show(name, &body),
                _ => {}
            }
            match &heard {
                Heard::RequestAvailable { request_id, complete: true, .. } => told.heard(*request_id, "available", &[]),
                Heard::RequestAvailable { request_id, seasons, complete: false, .. } => {
                    told.heard(*request_id, "approved", seasons.as_deref().unwrap_or_default());
                    // The event names the seasons newly ready: those ready before them come with the
                    // request, so that a catch-up tells only of seasons ready after these.
                    let path = format!("/api/requests/{request_id}");
                    if let Some(now) = get_json::<Listed>(client, server, &path, session.as_deref()).await {
                        told.known.insert(*request_id, Known::of(&now));
                    }
                }
                Heard::RequestDecided { request_id, status, .. } => told.heard(*request_id, status, &[]),
                _ => {}
            }
        }
    }
}

/// Follows `server`'s events for as long as it is shown (the caller drops
/// this when the window moves on), and tries again after a dropped stream.
async fn follow(client: &reqwest::Client, server: &Url, shell: &dyn Shell, name: &str) {
    let mut told = Told::default();
    let mut retry = RETRY_FIRST;
    loop {
        let opened = std::time::Instant::now();
        let ended = listen(client, server, shell, name, &mut told).await;
        if ended == Ended::Dropped {
            // The computer may be on another network now.
            shell.proofs().doubt(server);
        }
        // A stream that stayed open a while starts the pace again.
        if ended == Ended::Dropped && opened.elapsed() > RETRY_MAX {
            retry = RETRY_FIRST;
        }
        tokio::time::sleep(retry).await;
        retry = (retry * 2).min(RETRY_MAX);
    }
}

/// What notices from `server` are titled: the name it was saved with, else
/// its address.
fn title_for(saved: &[crate::servers::SavedServer], server: &Url) -> String {
    let address = server.as_str().trim_end_matches('/');
    saved.iter().find(|s| s.url == address).map(|s| s.name.clone()).unwrap_or_else(|| match server.port() {
        Some(port) => format!("{}:{port}", server.host_str().unwrap_or(address)),
        None => server.host_str().unwrap_or(address).to_owned(),
    })
}

/// Shows the notices of the server the window shows, for as long as the app runs.
pub async fn run(app: AppHandle) {
    let client = match reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("notifications are off: {err}");
            return;
        }
    };
    let mut shown = app.state::<Notices>().shown.subscribe();
    let shell = App(app.clone());
    loop {
        let server = shown.borrow_and_update().clone();
        if let Some(server) = server {
            let name = title_for(&crate::servers::load(&app), &server);
            tokio::select! {
                changed = shown.changed() => if changed.is_err() { return },
                () = follow(&client, &server, &shell, &name) => {}
            }
        } else if shown.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn events_are_read_across_chunks_and_line_endings() {
        let mut stream = EventStream::default();
        assert!(stream.read(b": keep-alive\n\nda").is_empty());
        assert!(stream.read(b"ta: {\"type\":\"request_av").is_empty());
        assert_eq!(
            stream.read(b"ailable\"}\r\n\r\ndata: one\ndata: two\n\n"),
            [r#"{"type":"request_available"}"#, "one\ntwo"]
        );
        assert_eq!(stream.read(b"event: x\nid: 4\ndata:no space\n\n\n\n"), ["no space"]);
        // An event too long is dropped whole; the next is read.
        let mut long = b"data: ".to_vec();
        long.extend(std::iter::repeat_n(b'x', MAX_EVENT + 10));
        long.extend_from_slice(b"\ndata: more\n\ndata: next\n\n");
        assert_eq!(stream.read(&long), ["next"]);
    }

    #[test]
    fn the_events_notices_come_from_are_read_as_the_server_sends_them() {
        assert_eq!(
            heard(
                r#"{"type":"request_available","request_id":2,"title":"Heat","kind":"movie","tmdb_id":949,"item_id":9,"poster_url":null,"seasons":null,"complete":true}"#
            ),
            Heard::RequestAvailable { request_id: 2, title: "Heat".into(), seasons: None, complete: true }
        );
        assert_eq!(
            heard(r#"{"type":"request_available","request_id":2,"title":"Heat","item_id":9}"#),
            Heard::RequestAvailable { request_id: 2, title: "Heat".into(), seasons: None, complete: true },
            "a server that sends less"
        );
        assert_eq!(
            heard(
                r##"{"type":"request_created","request_id":3,"title":"The Expanse","kind":"show","tmdb_id":63639,"seasons":[1,2,3],"requested_by":{"id":4,"name":"Sam","avatar_url":null,"avatar_color":"#5b8def"},"status":"pending"}"##
            ),
            Heard::RequestCreated {
                title: "The Expanse".into(),
                seasons: Some(vec![1, 2, 3]),
                requested_by: Person { name: "Sam".into() },
                status: Some("pending".into())
            }
        );
        assert_eq!(heard(r#"{"type":"requests_changed","request_id":null}"#), Heard::Other);
        assert_eq!(heard(r#"{"type":"library_scan","library_id":1}"#), Heard::Other);
        assert_eq!(heard("not json"), Heard::Other);
        assert_eq!(heard(r#"{"type":"request_available"}"#), Heard::Other, "missing fields");
    }

    #[test]
    fn notices_use_the_words_every_client_uses() {
        let say = |json: &str| notice(Language::En, &heard(json));
        assert_eq!(
            say(r#"{"type":"request_available","request_id":2,"title":"Heat","item_id":null}"#).as_deref(),
            Some("“Heat” is ready to watch.")
        );
        assert_eq!(
            say(r#"{"type":"request_available","request_id":2,"title":"The Expanse","seasons":[2],"complete":false}"#)
                .as_deref(),
            Some("Season 2 of “The Expanse” is ready to watch.")
        );
        assert_eq!(
            say(
                r#"{"type":"request_available","request_id":2,"title":"The Expanse","seasons":[1,2],"complete":false}"#
            )
            .as_deref(),
            Some("Seasons 1–2 of “The Expanse” are ready to watch.")
        );
        assert_eq!(
            say(
                r#"{"type":"request_available","request_id":2,"title":"The Expanse","seasons":[1,2,3],"complete":true}"#
            )
            .as_deref(),
            Some("“The Expanse” is ready to watch."),
            "complete: the whole request"
        );
        assert_eq!(
            say(r#"{"type":"request_decided","request_id":2,"title":"Heat","status":"approved","reason":null}"#)
                .as_deref(),
            Some("“Heat” was approved.")
        );
        assert_eq!(
            say(r#"{"type":"request_decided","request_id":2,"title":"Heat","status":"declined","reason":" Not in English "}"#)
                .as_deref(),
            Some("“Heat” was declined: “Not in English”.")
        );
        assert_eq!(
            say(r#"{"type":"request_decided","request_id":2,"title":"Heat","status":"declined","reason":""}"#)
                .as_deref(),
            Some("“Heat” was declined.")
        );
        assert_eq!(say(r#"{"type":"request_decided","request_id":2,"title":"Heat","status":"weird"}"#), None);
        let sam = r#""requested_by":{"id":4,"name":"Sam","avatar_url":null,"avatar_color":null}"#;
        assert_eq!(
            say(&format!(
                r#"{{"type":"request_created","request_id":3,"title":"Heat","seasons":null,{sam},"status":"pending"}}"#
            ))
            .as_deref(),
            Some("Sam asked for “Heat”.")
        );
        assert_eq!(
            say(&format!(
                r#"{{"type":"request_created","request_id":3,"title":"The Expanse","seasons":[5,1,2,3,0],{sam}}}"#
            ))
            .as_deref(),
            Some("Sam asked for “The Expanse” (Seasons 1–3, 5 and specials).")
        );
        assert_eq!(
            say(&format!(
                r#"{{"type":"request_created","request_id":3,"title":"Heat","seasons":null,{sam},"status":"approved"}}"#
            )),
            None,
            "approved as made: nothing to decide"
        );
        assert_eq!(
            say(r#"{"type":"request_failed","request_id":3,"title":"Heat","kind":"movie","reason":"HTTP 500","retry_at":null}"#)
                .as_deref(),
            Some("“Heat” couldn't be sent to Radarr: HTTP 500.")
        );
        assert_eq!(
            say(r#"{"type":"request_failed","request_id":3,"title":"Ran","kind":"show","reason":null}"#).as_deref(),
            Some("“Ran” couldn't be sent to Sonarr.")
        );
        assert_eq!(
            say(
                r#"{"type":"request_failed","request_id":3,"title":"Heat","kind":"movie","reason":"HTTP 503","retry_at":1798000000}"#
            ),
            None,
            "the server tries again by itself"
        );
        assert_eq!(say(r#"{"type":"requests_changed","request_id":3,"kind":"movie","tmdb_id":949}"#), None);
        assert_eq!(
            say(r#"{"type":"request_available","request_id":2,"title":"Dark","seasons":[0],"complete":false}"#)
                .as_deref(),
            Some("Specials of “Dark” are ready to watch.")
        );
    }

    #[test]
    fn notices_are_in_french_and_spanish_too() {
        let fr = |json: &str| notice(Language::Fr, &heard(json));
        let es = |json: &str| notice(Language::Es, &heard(json));
        let dark = r#"{"type":"request_available","request_id":2,"title":"Dark","seasons":[1,2],"complete":false}"#;
        // French puts no-break spaces inside its quotes and before a colon.
        assert_eq!(fr(dark).as_deref(), Some("Saisons 1–2 de «\u{a0}Dark\u{a0}» sont prêtes à regarder."));
        assert_eq!(es(dark).as_deref(), Some("Temporadas 1–2 de «Dark» ya están listas para ver."));
        let one = r#"{"type":"request_available","request_id":2,"title":"Dark","seasons":[3],"complete":false}"#;
        assert_eq!(fr(one).as_deref(), Some("Saison 3 de «\u{a0}Dark\u{a0}» est prête à regarder."));
        let declined =
            r#"{"type":"request_decided","request_id":2,"title":"Heat","status":"declined","reason":"Déjà là"}"#;
        assert_eq!(fr(declined).as_deref(), Some("«\u{a0}Heat\u{a0}» a été refusé\u{a0}: «\u{a0}Déjà là\u{a0}»."));
        let sam = r#""requested_by":{"id":4,"name":"Sam"}"#;
        assert_eq!(
            es(&format!(r#"{{"type":"request_created","request_id":3,"title":"Dark","seasons":[0,1],{sam}}}"#))
                .as_deref(),
            Some("Sam solicitó «Dark» (Temporada 1 y especiales).")
        );
        let failed = r#"{"type":"request_failed","request_id":3,"title":"Ran","kind":"show","reason":null}"#;
        assert_eq!(es(failed).as_deref(), Some("No se pudo enviar «Ran» a Sonarr."));
    }

    #[test]
    fn words_show_as_written_where_notification_servers_read_markup() {
        assert_eq!(escape_markup("“R&D” <b>now</b> > later"), "“R&amp;D” &lt;b&gt;now&lt;/b&gt; &gt; later");
        let body = "“Heat” was declined: “<a href=\"x\">Ask Sam & Jo</a>”.";
        if cfg!(all(unix, not(target_os = "macos"))) {
            assert_eq!(body_text(body), "“Heat” was declined: “&lt;a href=\"x\"&gt;Ask Sam &amp; Jo&lt;/a&gt;”.");
        } else {
            assert_eq!(body_text(body), body, "Windows' toasts escape it themselves, macOS takes plain text");
        }
    }

    #[test]
    fn seasons_read_as_on_the_web() {
        let label = |s: &[i64]| seasons_label(Language::En, Some(s));
        assert_eq!(label(&[2]).as_deref(), Some("Season 2"));
        assert_eq!(label(&[3, 1, 2, 5]).as_deref(), Some("Seasons 1–3, 5"));
        assert_eq!(label(&[0, 1, 2]).as_deref(), Some("Seasons 1–2 and specials"));
        assert_eq!(label(&[0]).as_deref(), Some("Specials"));
        assert_eq!(label(&[4, 4]).as_deref(), Some("Season 4"));
        assert_eq!(label(&[]), None);
        assert_eq!(seasons_label(Language::En, None), None);
        assert_eq!(seasons_label(Language::Fr, Some(&[0])).as_deref(), Some("Épisodes spéciaux"));
        assert_eq!(seasons_label(Language::Fr, Some(&[0, 2, 3])).as_deref(), Some("Saisons 2–3 et épisodes spéciaux"));
    }

    /// A request with news, as listed: its status, and its stage the same.
    fn news(id: i64, title: &str, status: &str, updated_at: i64) -> Listed {
        Listed {
            id,
            title: title.into(),
            status: status.into(),
            stage: status.into(),
            available_seasons: Vec::new(),
            reason: None,
            updated_at,
            unseen: true,
        }
    }

    /// A show with `seasons` ready, as listed.
    fn partly(id: i64, title: &str, seasons: &[i64], updated_at: i64) -> Listed {
        Listed {
            stage: "partial".into(),
            available_seasons: seasons.to_vec(),
            ..news(id, title, "approved", updated_at)
        }
    }

    fn titles(news: &[&Listed]) -> Vec<String> {
        news.iter().map(|r| r.title.clone()).collect()
    }

    #[test]
    fn catching_up_tells_only_of_what_came_since() {
        let mut told = Told::default();
        // The first time, nothing old is told of: the page does, as it opens.
        assert!(told.catch_up(&[news(1, "Heat", "available", 100), news(2, "Ran", "declined", 50)]).is_empty());
        assert!(told.catch_up(&[news(1, "Heat", "available", 100)]).is_empty());
        // One heard of as it changed isn't told of again; one seen isn't news.
        told.heard(4, "available", &[]);
        // One heard of before it was ready (approved) is told of once it is.
        told.heard(5, "approved", &[]);
        let seen = Listed { unseen: false, ..news(6, "Big", "available", 180) };
        let listed = [
            news(5, "Up", "available", 170),
            news(4, "Ronin", "available", 160),
            news(3, "Alien", "declined", 150),
            seen,
            news(1, "Heat", "available", 100),
        ];
        assert_eq!(titles(&told.catch_up(&listed)), ["Up", "Alien"]);
        assert!(told.catch_up(&listed).is_empty());
        let mut empty = Told::default();
        assert!(empty.catch_up(&[]).is_empty());
        assert_eq!(titles(&empty.catch_up(&[partly(9, "Big", &[1], 1)])), ["Big"]);
    }

    #[test]
    fn a_show_still_partly_ready_is_told_of_again_as_more_of_it_is() {
        let mut told = Told::default();
        assert!(told.catch_up(&[]).is_empty());
        // Season 2 heard of as it came, read with season 1, ready before it.
        told.heard(7, "approved", &[2]);
        told.known.insert(7, Known { status: "approved".into(), seasons: vec![1, 2] });
        assert!(told.catch_up(&[partly(7, "Dark", &[1, 2], 200)]).is_empty());
        // Season 3 came while the stream was down.
        assert_eq!(titles(&told.catch_up(&[partly(7, "Dark", &[1, 2, 3], 300)])), ["Dark"]);
        assert!(told.catch_up(&[partly(7, "Dark", &[1, 2, 3], 300)]).is_empty());
        // And season 4, while season 5 downloads.
        let downloading = Listed { stage: "downloading".into(), ..partly(7, "Dark", &[1, 2, 3, 4], 400) };
        assert_eq!(titles(&told.catch_up(&[downloading])), ["Dark"]);
        // Then all of it.
        assert_eq!(titles(&told.catch_up(&[news(7, "Dark", "available", 500)])), ["Dark"]);
    }

    #[test]
    fn what_came_meanwhile_is_one_notice() {
        let heat = news(1, "Heat", "available", 100);
        let caught_up = |news: &[&Listed]| caught_up(Language::En, news);
        assert_eq!(caught_up(&[&heat]).as_deref(), Some("“Heat” is ready to watch."));
        let expanse = partly(2, "The Expanse", &[1, 2], 100);
        assert_eq!(caught_up(&[&expanse]).as_deref(), Some("Seasons 1–2 of “The Expanse” are ready to watch."));
        let downloading = Listed { stage: "downloading".into(), ..partly(2, "The Expanse", &[1], 100) };
        assert_eq!(caught_up(&[&downloading]).as_deref(), Some("Season 1 of “The Expanse” is ready to watch."));
        let ran = Listed { reason: Some("Not in English".into()), ..news(3, "Ran", "declined", 100) };
        assert_eq!(caught_up(&[&ran]).as_deref(), Some("“Ran” was declined: “Not in English”."));
        assert_eq!(caught_up(&[&heat, &expanse, &ran]).as_deref(), Some("3 of your requests have news."));
        assert_eq!(caught_up(&[]), None);
        let searching = Listed { stage: "searching".into(), ..news(4, "Up", "approved", 100) };
        assert_eq!(caught_up(&[&searching]), None);
        let gone = Listed { stage: "gone".into(), ..news(5, "Big", "available", 100) };
        assert_eq!(caught_up(&[&gone]), None);
        assert_eq!(
            super::caught_up(Language::Es, &[&heat, &ran]).as_deref(),
            Some("2 de tus solicitudes tienen novedades.")
        );
    }

    #[test]
    fn notices_are_titled_with_the_servers_name() {
        let saved = [crate::servers::SavedServer {
            url: "http://192.168.1.20:7539".into(),
            name: "Den".into(),
            id: None,
            last_used: 0,
        }];
        let at = |s: &str| title_for(&saved, &Url::parse(s).unwrap());
        assert_eq!(at("http://192.168.1.20:7539/"), "Den");
        assert_eq!(at("http://192.168.1.21:7539/"), "192.168.1.21:7539");
        assert_eq!(at("https://pley.example.com/"), "pley.example.com");
    }

    /// The window, as the tests see it.
    struct Window {
        in_front: Mutex<bool>,
        shown: Mutex<Vec<(String, String)>>,
        proofs: crate::proofs::Proofs,
        not_yours: Mutex<Vec<Url>>,
        languages: Vec<String>,
    }

    impl Window {
        /// On `server`, which proved itself a moment ago (or not), on a
        /// computer in American English.
        fn at(server: &Url, proved: bool) -> Self {
            Self {
                in_front: Mutex::default(),
                shown: Mutex::default(),
                proofs: crate::proofs::Proofs::for_tests(server, proved),
                not_yours: Mutex::default(),
                languages: vec!["en-US".to_owned()],
            }
        }
    }

    impl Shell for Window {
        fn in_front(&self) -> bool {
            *self.in_front.lock().unwrap()
        }

        fn show(&self, title: &str, body: &str) {
            self.shown.lock().unwrap().push((title.to_owned(), body.to_owned()));
        }

        fn session(&self, _server: &Url) -> Option<String> {
            Some("pley_token=abc".to_owned())
        }

        fn proofs(&self) -> &crate::proofs::Proofs {
            &self.proofs
        }

        fn not_yours(&self, server: &Url) {
            self.not_yours.lock().unwrap().push(server.clone());
        }

        fn languages(&self) -> Vec<String> {
            self.languages.clone()
        }
    }

    /// A server that answers each connection with the next of `answers`
    /// (written in pieces, a stream's events one by one), and tells what
    /// each request asked.
    async fn serve(answers: Vec<Vec<Vec<u8>>>) -> (Url, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            for pieces in answers {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buf).await.unwrap();
                    request.extend_from_slice(&buf[..n]);
                }
                let _ = tx.send(String::from_utf8_lossy(&request).into_owned());
                for piece in pieces {
                    let _ = socket.write_all(&piece).await;
                    let _ = socket.flush().await;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                let _ = socket.shutdown().await;
            }
        });
        (url, rx)
    }

    fn whole(status: &str, body: &str) -> Vec<Vec<u8>> {
        vec![
            format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .into_bytes(),
        ]
    }

    fn events(events: &[&str]) -> Vec<Vec<u8>> {
        let mut pieces =
            vec![b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n".to_vec()];
        pieces.extend(events.iter().map(|e| e.as_bytes().to_vec()));
        pieces
    }

    /// The account, in `language` (`None`: Automatic).
    fn me(language: Option<&str>) -> Vec<Vec<u8>> {
        whole("200 OK", &serde_json::json!({ "id": 1, "username": "sam", "language": language }).to_string())
    }

    #[tokio::test]
    async fn the_stream_is_heard_with_the_windows_session_and_heard_again_after_it_drops() {
        let list =
            |items: &str| whole("200 OK", &format!(r#"{{"total":0,"counts":{{}},"requests":[{items}],"people":[]}}"#));
        let ready = |id: i64, title: &str, updated_at: i64| {
            format!(
                r#"{{"id":{id},"title":"{title}","status":"available","stage":"available","available_seasons":[],"updated_at":{updated_at},"unseen":true}}"#
            )
        };
        let (ran, heat, alien) = (ready(1, "Ran", 100), ready(2, "Heat", 140), ready(3, "Alien", 150));
        let (server, mut asked) = serve(vec![
            me(None),
            events(&[
                ": hello\n\n",
                "data: {\"type\":\"request_available\",\"request_id\":2,\"title\":\"Heat\",\"item_id\":9}\n\n",
            ]),
            list(&ran),
            // Dropped: the second stream catches up.
            me(None),
            events(&["data: {\"type\":\"request_created\",\"request_id\":5,\"title\":\"Up\",\"seasons\":null,\
                 \"requested_by\":{\"id\":4,\"name\":\"Sam\"},\"status\":\"pending\"}\n\n"]),
            list(&[alien, heat, ran].join(",")),
        ])
        .await;
        let window = Window::at(&server, true);
        let client = reqwest::Client::new();
        let mut told = Told::default();
        assert_eq!(listen(&client, &server, &window, "Den", &mut told).await, Ended::Dropped);
        assert_eq!(listen(&client, &server, &window, "Den", &mut told).await, Ended::Dropped);
        let account = asked.recv().await.unwrap().to_ascii_lowercase();
        assert!(account.starts_with("get /api/auth/me "), "{account}");
        assert!(account.contains("cookie: pley_token=abc\r\n"), "{account}");
        let first = asked.recv().await.unwrap().to_ascii_lowercase();
        assert!(first.starts_with("get /api/events "), "{first}");
        assert!(first.contains("cookie: pley_token=abc\r\n"), "{first}");
        assert!(first.contains("accept: text/event-stream\r\n"), "{first}");
        assert!(first.contains("accept-language: en\r\n"), "{first}");
        let listing = asked.recv().await.unwrap().to_ascii_lowercase();
        assert!(listing.starts_with("get /api/requests?unseen=1&limit=20 "), "{listing}");
        assert!(listing.contains("cookie: pley_token=abc\r\n"), "{listing}");
        let shown: Vec<String> =
            window.shown.lock().unwrap().iter().map(|(title, body)| format!("{title}: {body}")).collect();
        assert_eq!(
            shown,
            [
                "Den: “Heat” is ready to watch.",
                // The second stream first catches up: Heat was heard of as it
                // came, so only Alien is news.
                "Den: “Alien” is ready to watch.",
                "Den: Sam asked for “Up”.",
            ]
        );
    }

    #[tokio::test]
    async fn a_shows_seasons_ready_while_the_stream_was_down_are_told_of_and_those_heard_are_not() {
        let list =
            |items: &str| whole("200 OK", &format!(r#"{{"total":0,"counts":{{}},"requests":[{items}],"people":[]}}"#));
        let dark = |seasons: &str, updated_at: i64| {
            format!(
                r#"{{"id":7,"title":"Dark","status":"approved","stage":"partial","available_seasons":[{seasons}],"updated_at":{updated_at},"unseen":true}}"#
            )
        };
        let (server, mut asked) = serve(vec![
            me(None),
            events(&["data: {\"type\":\"request_available\",\"request_id\":7,\"title\":\"Dark\",\"kind\":\"show\",\
                 \"tmdb_id\":70523,\"item_id\":9,\"poster_url\":null,\"seasons\":[2],\"complete\":false}\n\n"]),
            list(""),
            // Season 1 was ready before: the request as it is now says so.
            whole("200 OK", &dark("1,2", 200)),
            // Dropped, with nothing new meanwhile: Season 2 was told of.
            me(None),
            events(&[]),
            list(&dark("1,2", 200)),
            // Dropped again, and Season 3 came meanwhile.
            me(None),
            events(&[]),
            list(&dark("1,2,3", 300)),
        ])
        .await;
        let window = Window::at(&server, true);
        let client = reqwest::Client::new();
        let mut told = Told::default();
        for _ in 0..3 {
            assert_eq!(listen(&client, &server, &window, "Den", &mut told).await, Ended::Dropped);
        }
        let paths: Vec<String> = std::iter::from_fn(|| asked.try_recv().ok())
            .map(|request| request.split(' ').nth(1).unwrap_or_default().to_owned())
            .collect();
        assert_eq!(paths[3], "/api/requests/7");
        let shown: Vec<String> = window.shown.lock().unwrap().iter().map(|(_, body)| body.clone()).collect();
        assert_eq!(shown, ["Season 2 of “Dark” is ready to watch.", "Seasons 1–3 of “Dark” are ready to watch."]);
    }

    #[tokio::test]
    async fn nothing_is_shown_while_the_window_is_in_front_and_a_refusal_waits() {
        let signed_out = || whole("401 Unauthorized", r#"{"code":"unauthorized","message":"sign in"}"#);
        let (server, _asked) = serve(vec![
            me(None),
            events(&["data: {\"type\":\"request_available\",\"request_id\":2,\"title\":\"Heat\",\"item_id\":9}\n\n"]),
            whole("503 Service Unavailable", r#"{"code":"unavailable","message":"requests are off"}"#),
            signed_out(),
            signed_out(),
        ])
        .await;
        let window = Window::at(&server, true);
        *window.in_front.lock().unwrap() = true;
        let client = reqwest::Client::new();
        let mut told = Told::default();
        assert_eq!(listen(&client, &server, &window, "Den", &mut told).await, Ended::Dropped);
        assert!(window.shown.lock().unwrap().is_empty());
        assert_eq!(listen(&client, &server, &window, "Den", &mut told).await, Ended::Refused);
    }

    #[tokio::test]
    async fn notices_are_in_the_accounts_language_else_the_computers_else_the_servers() {
        let heat =
            "data: {\"type\":\"request_decided\",\"request_id\":2,\"title\":\"Heat\",\"status\":\"approved\"}\n\n";
        let (server, mut asked) = serve(vec![
            // The account chose French, on a computer in Spanish.
            me(Some("fr")),
            events(&[heat]),
            whole("200 OK", r#"{"requests":[]}"#),
            // Automatic, on a computer in German: the server's language.
            me(None),
            whole("200 OK", r#"{"id":"x","name":"Den","language":"es","version":"0.1.10","setup_required":false}"#),
            events(&[heat]),
            whole("200 OK", r#"{"requests":[]}"#),
        ])
        .await;
        let mut window = Window::at(&server, true);
        window.languages = vec!["es-MX".to_owned()];
        let client = reqwest::Client::new();
        let mut told = Told::default();
        assert_eq!(listen(&client, &server, &window, "Den", &mut told).await, Ended::Dropped);
        window.languages = vec!["de-DE".to_owned()];
        assert_eq!(listen(&client, &server, &window, "Den", &mut told).await, Ended::Dropped);
        let shown: Vec<String> = window.shown.lock().unwrap().iter().map(|(_, body)| body.clone()).collect();
        assert_eq!(shown, ["«\u{a0}Heat\u{a0}» a été approuvé.", "«Heat» se aprobó."]);
        let asked: Vec<String> = std::iter::from_fn(|| asked.try_recv().ok()).collect();
        assert!(asked[1].to_ascii_lowercase().contains("accept-language: fr\r\n"), "{}", asked[1]);
        assert!(asked[4].starts_with("GET /api/server "), "{}", asked[4]);
        assert!(!asked[4].contains("pley_token"), "{}", asked[4]);
        assert!(asked[5].to_ascii_lowercase().contains("accept-language: es\r\n"), "{}", asked[5]);
    }

    #[tokio::test]
    async fn another_device_at_the_address_hears_nothing_and_the_window_leaves() {
        let (server, mut asked) =
            serve(vec![whole("200 OK", r#"{"id":"another","name":"Den","version":"0.1.2","setup_required":false}"#)])
                .await;
        let window = Window::at(&server, false);
        let client = reqwest::Client::new();
        assert_eq!(listen(&client, &server, &window, "Den", &mut Told::default()).await, Ended::Refused);
        let request = asked.recv().await.unwrap();
        assert!(request.starts_with("GET /api/server?challenge="), "{request}");
        assert!(!request.contains("pley_token"), "{request}");
        assert_eq!(*window.not_yours.lock().unwrap(), [server]);
    }
}
