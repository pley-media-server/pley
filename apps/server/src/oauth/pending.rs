//! Requests waiting on the consent page: what an app asked for, checked,
//! until the person allows it or not, and then where the browser goes back
//! to. In memory only.
//!
//! A request belongs to the person and the browser session that opened the
//! page (the SHA-256 of its session token), and to the address pley was
//! reached at (the issuer, sent back as `iss`, RFC 9207). It lasts 10
//! minutes; each person has at most 10 (the oldest goes) and the server
//! 1,000. Once answered, the way back repeats for a minute: while the code
//! works and wasn't exchanged when allowed, and for 60 seconds after a
//! refusal or an error.
//!
//! The request of a TV signing in with a code ([`super::devices`]) waits
//! here too, while the person decides: its answer goes to the TV's request,
//! and nothing goes back through the browser.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use url::Url;

use super::AppSnapshot;
use crate::auth::new_token;
use crate::auth::scopes::Scopes;

/// How long a request waits for an answer.
pub const PENDING_LIFETIME: Duration = Duration::from_secs(600);
/// How long the way back repeats after a refusal or an error (and how long a
/// code works).
pub const ANSWER_LIFETIME: Duration = Duration::from_secs(60);
/// Requests per person; past it the oldest goes.
pub const MAX_PENDING_PER_USER: usize = 10;
/// Requests in all.
pub const MAX_PENDING: usize = 1_000;

/// A mistake in an app's request that goes back to it, but only when the
/// person clicks "Back to the app" (OAuth 2.1 -16 §4.1.2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnedError {
    /// The OAuth error code: `invalid_request`, `invalid_scope`, …
    pub error: &'static str,
    /// What went wrong, for the person and the app (ASCII).
    pub description: String,
}

/// A request an app made, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRequest {
    pub user_id: i64,
    /// SHA-256 of the session token of the browser that opened the page.
    pub session_hash: String,
    /// The address pley was reached at.
    pub issuer: String,
    pub app: AppSnapshot,
    /// Where the answer goes.
    pub kind: Kind,
    pub scopes: Scopes,
    /// Set when the request can't be allowed: it only goes back.
    pub error: Option<ReturnedError>,
}

/// Where the answer to a request goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Back to the app, through the browser.
    Redirect(Redirect),
    /// To a TV that shows a code and asks pley for the answer: the key of
    /// its request in [`super::devices::Devices`].
    Device(String),
}

/// How the browser goes back to the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    /// As the app sent it: answers go there, port included.
    pub redirect_uri: String,
    pub state: Option<String>,
    /// Empty when the request's `error` says it was missing or wrong.
    pub code_challenge: String,
}

/// Too many requests are waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

/// Why a decision wasn't taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecideError<E> {
    /// Not this person's or this session's, or over.
    NotFound,
    /// Decided the other way already, or a request that can only go back.
    Conflict,
    /// The code couldn't be made, or the TV's request couldn't be told.
    Code(E),
}

/// The way back to the app once a request is answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continue {
    /// The app's return address with the answer.
    pub location: String,
    /// The code in it, when the app was allowed: the way back repeats only
    /// while it works and wasn't exchanged.
    pub code: Option<String>,
}

struct Entry {
    request: PendingRequest,
    created: Instant,
    /// When it is forgotten.
    until: Instant,
    decision: Option<bool>,
    answer: Option<Continue>,
}

impl Entry {
    fn alive(&self, now: Instant) -> bool {
        now < self.until
    }

    fn belongs_to(&self, user_id: i64, session_hash: &str) -> bool {
        self.request.user_id == user_id && self.request.session_hash == session_hash
    }
}

/// The requests waiting, by id.
#[derive(Default)]
pub struct Pending {
    requests: Mutex<HashMap<String, Entry>>,
}

/// The return address of `back` with `params`, then `state` (when the app
/// sent one) and `iss` (the `issuer`) added to its query (its own query
/// kept).
pub fn answer_url(issuer: &str, back: &Redirect, params: &[(&str, &str)]) -> String {
    let Ok(mut url) = Url::parse(&back.redirect_uri) else { return back.redirect_uri.clone() };
    {
        let mut query = url.query_pairs_mut();
        for (name, value) in params {
            query.append_pair(name, value);
        }
        if let Some(state) = &back.state {
            query.append_pair("state", state);
        }
        query.append_pair("iss", issuer);
    }
    url.into()
}

impl Pending {
    /// Keeps a request; its id. The person's oldest goes past 10.
    pub fn create(&self, request: PendingRequest) -> Result<String, Full> {
        self.create_lasting(request, PENDING_LIFETIME)
    }

    /// [`Pending::create`], for a request that ends after `lifetime` (10
    /// minutes at most): a TV's request ends no later than the TV's own.
    pub fn create_lasting(&self, request: PendingRequest, lifetime: Duration) -> Result<String, Full> {
        let now = Instant::now();
        let mut requests = self.requests.lock();
        requests.retain(|_, entry| entry.alive(now));
        loop {
            let mine = requests.iter().filter(|(_, entry)| entry.request.user_id == request.user_id);
            let (count, oldest) = mine.fold((0, None::<(&String, Instant)>), |(count, oldest), (id, entry)| {
                let older = oldest.is_none_or(|(_, created)| entry.created < created);
                (count + 1, if older { Some((id, entry.created)) } else { oldest })
            });
            match oldest {
                Some((id, _)) if count >= MAX_PENDING_PER_USER => {
                    let id = id.clone();
                    requests.remove(&id);
                }
                _ => break,
            }
        }
        if requests.len() >= MAX_PENDING {
            return Err(Full);
        }
        let id = new_token();
        let until = now + lifetime.min(PENDING_LIFETIME);
        let entry = Entry { request, created: now, until, decision: None, answer: None };
        requests.insert(id.clone(), entry);
        Ok(id)
    }

    /// Forgets request `id`.
    pub fn remove(&self, id: &str) {
        self.requests.lock().remove(id);
    }

    /// Whose request `id` is, while it isn't over: the person, and the
    /// SHA-256 of their session's token.
    pub fn owner(&self, id: &str) -> Option<(i64, String)> {
        let now = Instant::now();
        let requests = self.requests.lock();
        let entry = requests.get(id).filter(|entry| entry.alive(now))?;
        Some((entry.request.user_id, entry.request.session_hash.clone()))
    }

    /// The request `id`, when it is this person's and this session's and
    /// isn't over.
    pub fn get(&self, id: &str, user_id: i64, session_hash: &str) -> Option<PendingRequest> {
        let now = Instant::now();
        let requests = self.requests.lock();
        let entry = requests.get(id).filter(|entry| entry.alive(now) && entry.belongs_to(user_id, session_hash))?;
        Some(entry.request.clone())
    }

    /// Allows the app (with the code `make_code` makes for the request) or
    /// not, for a request that goes back to the app. The same decision again
    /// changes nothing; the other one is a conflict.
    pub fn decide<E>(
        &self,
        id: &str,
        user_id: i64,
        session_hash: &str,
        allow: bool,
        make_code: impl FnOnce(&PendingRequest, &Redirect) -> Result<String, E>,
    ) -> Result<(), DecideError<E>> {
        self.answer(id, user_id, session_hash, allow, false, |request| {
            let Kind::Redirect(back) = &request.kind else { return Err(DecideError::NotFound) };
            let answer = if allow {
                let code = make_code(request, back).map_err(DecideError::Code)?;
                Continue { location: answer_url(&request.issuer, back, &[("code", &code)]), code: Some(code) }
            } else {
                Continue { location: answer_url(&request.issuer, back, &[("error", "access_denied")]), code: None }
            };
            Ok(Some(answer))
        })
    }

    /// Allows the app on a TV or not, for a TV's request ([`Kind::Device`]):
    /// `tell` passes the answer on to the TV's request (by its key), and when
    /// it can't, because that request is over, nothing is decided. The same
    /// decision again changes nothing; the other one is a conflict.
    pub fn decide_device<E>(
        &self,
        id: &str,
        user_id: i64,
        session_hash: &str,
        allow: bool,
        tell: impl FnOnce(&PendingRequest, &str) -> Result<(), E>,
    ) -> Result<(), DecideError<E>> {
        self.answer(id, user_id, session_hash, allow, true, |request| {
            let Kind::Device(key) = &request.kind else { return Err(DecideError::NotFound) };
            tell(request, key).map_err(DecideError::Code)?;
            Ok(None)
        })
    }

    /// Decides request `id` of the given kind (a TV's or not): the first
    /// time, with what `first` makes of the request, under the lock, so
    /// that when it fails nothing is decided.
    fn answer<E>(
        &self,
        id: &str,
        user_id: i64,
        session_hash: &str,
        allow: bool,
        device: bool,
        first: impl FnOnce(&PendingRequest) -> Result<Option<Continue>, DecideError<E>>,
    ) -> Result<(), DecideError<E>> {
        let now = Instant::now();
        let mut requests = self.requests.lock();
        let Some(entry) = requests.get_mut(id).filter(|entry| {
            entry.alive(now)
                && entry.belongs_to(user_id, session_hash)
                && matches!(entry.request.kind, Kind::Device(_)) == device
        }) else {
            return Err(DecideError::NotFound);
        };
        if entry.request.error.is_some() {
            return Err(DecideError::Conflict);
        }
        match entry.decision {
            Some(decided) if decided == allow => return Ok(()),
            Some(_) => return Err(DecideError::Conflict),
            None => {}
        }
        entry.answer = first(&entry.request)?;
        entry.decision = Some(allow);
        entry.until = now + ANSWER_LIFETIME;
        Ok(())
    }

    /// Where the browser goes back to, once the request is answered (or
    /// can only go back): for a minute after the answer, or after the first
    /// time back for a request with a mistake (the person may read the page
    /// for a while before clicking). Never for a TV's request.
    pub fn location(&self, id: &str, user_id: i64, session_hash: &str) -> Option<Continue> {
        let now = Instant::now();
        let mut requests = self.requests.lock();
        let entry = requests.get_mut(id).filter(|entry| entry.alive(now) && entry.belongs_to(user_id, session_hash))?;
        if entry.answer.is_none() {
            let error = entry.request.error.clone()?;
            let Kind::Redirect(back) = &entry.request.kind else { return None };
            let params = [("error", error.error), ("error_description", error.description.as_str())];
            let location = answer_url(&entry.request.issuer, back, &params);
            entry.answer = Some(Continue { location, code: None });
            entry.until = now + ANSWER_LIFETIME;
        }
        entry.answer.clone()
    }

    /// The sign-ins of `user_id` ended (a new password, signing out
    /// everywhere, disabling, deletion): their requests are over, answered
    /// or not.
    pub fn forget_user(&self, user_id: i64) {
        self.requests.lock().retain(|_, entry| entry.request.user_id != user_id);
    }

    /// Forgets requests that are over.
    pub fn sweep(&self) {
        let now = Instant::now();
        self.requests.lock().retain(|_, entry| entry.alive(now));
    }

    /// Makes request `id` end in `left` (tests).
    #[cfg(test)]
    fn ends_in(&self, id: &str, left: Duration) {
        if let Some(entry) = self.requests.lock().get_mut(id) {
            entry.until = Instant::now() + left;
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.requests.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(user_id: i64) -> PendingRequest {
        PendingRequest {
            user_id,
            session_hash: format!("session-of-{user_id}"),
            issuer: "http://pley.local:7539".into(),
            app: AppSnapshot {
                id: "https://pley-media-server.github.io/pley/oauth/desktop.json".into(),
                host: "pley-media-server.github.io".into(),
                name: Some("Pley for desktop".into()),
                first_party: true,
                refresh: true,
            },
            kind: Kind::Redirect(Redirect {
                redirect_uri: "http://127.0.0.1:53111/oauth/callback?keep=1".into(),
                state: Some("st&te".into()),
                code_challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".into(),
            }),
            scopes: Scopes::parse("library").unwrap(),
            error: None,
        }
    }

    /// A TV's request, shown to `user_id`.
    fn tv_request(user_id: i64) -> PendingRequest {
        PendingRequest { kind: Kind::Device("tv-key".into()), ..request(user_id) }
    }

    fn code(_: &PendingRequest, _: &Redirect) -> Result<String, ()> {
        Ok("c0de".into())
    }

    #[test]
    fn requests_belong_to_their_person_and_session() {
        let pending = Pending::default();
        let id = pending.create(request(1)).unwrap();
        assert_eq!(id.len(), 43);
        assert_eq!(pending.get(&id, 1, "session-of-1"), Some(request(1)));
        assert_eq!(pending.get(&id, 2, "session-of-1"), None);
        assert_eq!(pending.get(&id, 1, "another session"), None);
        assert_eq!(pending.decide(&id, 1, "another session", true, code), Err(DecideError::NotFound));
        assert_eq!(pending.location(&id, 1, "session-of-1"), None, "not answered yet");
        assert_eq!(pending.decide(&id, 1, "session-of-1", true, code), Ok(()));
        assert_eq!(pending.location(&id, 1, "another session"), None);
        let back = pending.location(&id, 1, "session-of-1").unwrap();
        assert_eq!(back.code.as_deref(), Some("c0de"));
        assert_eq!(
            back.location,
            "http://127.0.0.1:53111/oauth/callback?keep=1&code=c0de&state=st%26te&iss=http%3A%2F%2Fpley.local%3A7539"
        );
    }

    #[test]
    fn a_decision_stands() {
        let pending = Pending::default();
        let id = pending.create(request(1)).unwrap();
        assert_eq!(pending.decide(&id, 1, "session-of-1", false, code), Ok(()));
        // The same again changes nothing, and makes no code.
        let no_code = |_: &PendingRequest, _: &Redirect| -> Result<String, ()> { panic!("no code for a refusal") };
        assert_eq!(pending.decide(&id, 1, "session-of-1", false, no_code), Ok(()));
        assert_eq!(pending.decide(&id, 1, "session-of-1", true, code), Err(DecideError::Conflict));
        let back = pending.location(&id, 1, "session-of-1").unwrap();
        assert_eq!(back.code, None);
        assert!(back.location.contains("?keep=1&error=access_denied&state=st%26te&iss="), "{}", back.location);
        // A code that can't be made decides nothing.
        let id = pending.create(request(1)).unwrap();
        assert_eq!(pending.decide(&id, 1, "session-of-1", true, |_, _| Err("full")), Err(DecideError::Code("full")));
        assert_eq!(pending.decide(&id, 1, "session-of-1", false, code), Ok(()));
    }

    #[test]
    fn a_tvs_request_is_answered_to_the_tv() {
        let pending = Pending::default();
        let id = pending.create(tv_request(1)).unwrap();
        assert_eq!(pending.decide(&id, 1, "session-of-1", true, code), Err(DecideError::NotFound), "no code");
        // The TV's request is told once, and nothing goes back through the
        // browser.
        let mut told = Vec::new();
        let tell = |request: &PendingRequest, key: &str| -> Result<(), ()> {
            told.push((request.user_id, key.to_owned()));
            Ok(())
        };
        assert_eq!(pending.decide_device(&id, 1, "session-of-1", true, tell), Ok(()));
        assert_eq!(told, [(1, "tv-key".to_owned())]);
        let never = |_: &PendingRequest, _: &str| -> Result<(), ()> { panic!("told twice") };
        assert_eq!(pending.decide_device(&id, 1, "session-of-1", true, never), Ok(()));
        assert_eq!(pending.decide_device(&id, 1, "session-of-1", false, never), Err(DecideError::Conflict));
        assert_eq!(pending.location(&id, 1, "session-of-1"), None);

        // When the TV's request is over, nothing is decided.
        let id = pending.create(tv_request(1)).unwrap();
        assert_eq!(
            pending.decide_device(&id, 1, "session-of-1", true, |_, _| Err("over")),
            Err(DecideError::Code("over"))
        );
        assert_eq!(pending.decide_device(&id, 1, "session-of-1", false, |_, _| Ok::<_, ()>(())), Ok(()));
        // Only its person and session decide it, and only as a TV's.
        let id = pending.create(tv_request(2)).unwrap();
        assert_eq!(pending.decide_device(&id, 1, "session-of-1", true, never), Err(DecideError::NotFound));
        assert_eq!(pending.decide_device(&id, 2, "another session", true, never), Err(DecideError::NotFound));
        let redirect = pending.create(request(1)).unwrap();
        assert_eq!(pending.decide_device(&redirect, 1, "session-of-1", true, never), Err(DecideError::NotFound));
    }

    #[test]
    fn requests_last_as_long_as_asked_and_say_whose_they_are() {
        let pending = Pending::default();
        let id = pending.create_lasting(tv_request(1), Duration::from_secs(30)).unwrap();
        assert!(pending.requests.lock()[&id].until <= Instant::now() + Duration::from_secs(30));
        assert_eq!(pending.owner(&id), Some((1, "session-of-1".to_owned())));
        // Never longer than ten minutes.
        let long = pending.create_lasting(tv_request(1), Duration::from_secs(3600)).unwrap();
        assert!(pending.requests.lock()[&long].until <= Instant::now() + PENDING_LIFETIME);
        pending.remove(&id);
        assert_eq!(pending.owner(&id), None, "removed");
        pending.ends_in(&long, Duration::ZERO);
        assert_eq!(pending.owner(&long), None, "over");
    }

    #[test]
    fn mistakes_only_go_back() {
        let pending = Pending::default();
        let mut wrong = request(1);
        if let Kind::Redirect(back) = &mut wrong.kind {
            back.state = None;
        }
        wrong.error = Some(ReturnedError { error: "invalid_scope", description: "no such scope".into() });
        let id = pending.create(wrong).unwrap();
        assert_eq!(pending.decide(&id, 1, "session-of-1", true, code), Err(DecideError::Conflict));
        let back = pending.location(&id, 1, "session-of-1").unwrap();
        assert_eq!(
            back.location,
            "http://127.0.0.1:53111/oauth/callback?keep=1&error=invalid_scope&error_description=no+such+scope&iss=\
             http%3A%2F%2Fpley.local%3A7539"
        );
        // It repeats for a minute after the first time.
        assert_eq!(pending.location(&id, 1, "session-of-1"), Some(back));
        pending.ends_in(&id, Duration::ZERO);
        assert_eq!(pending.location(&id, 1, "session-of-1"), None);
    }

    #[test]
    fn requests_end() {
        let pending = Pending::default();
        let id = pending.create(request(1)).unwrap();
        pending.ends_in(&id, Duration::ZERO);
        assert_eq!(pending.get(&id, 1, "session-of-1"), None);
        assert_eq!(pending.decide(&id, 1, "session-of-1", true, code), Err(DecideError::NotFound));
        pending.sweep();
        assert_eq!(pending.len(), 0);

        // With the sign-ins of their person, answered or not.
        let waiting = pending.create(request(1)).unwrap();
        let answered = pending.create(request(1)).unwrap();
        assert_eq!(pending.decide(&answered, 1, "session-of-1", true, code), Ok(()));
        let tv = pending.create(tv_request(1)).unwrap();
        let someone_elses = pending.create(request(2)).unwrap();
        pending.forget_user(1);
        assert_eq!(pending.get(&waiting, 1, "session-of-1"), None);
        assert_eq!(pending.location(&answered, 1, "session-of-1"), None);
        assert_eq!(pending.owner(&tv), None);
        assert!(pending.get(&someone_elses, 2, "session-of-2").is_some());
    }

    #[test]
    fn requests_are_capped() {
        let pending = Pending::default();
        let ids: Vec<String> = (0..MAX_PENDING_PER_USER).map(|_| pending.create(request(1)).unwrap()).collect();
        // The 11th pushes the oldest out.
        let newest = pending.create(request(1)).unwrap();
        assert_eq!(pending.get(&ids[0], 1, "session-of-1"), None);
        assert!(ids[1..].iter().chain([&newest]).all(|id| pending.get(id, 1, "session-of-1").is_some()));
        assert_eq!(pending.len(), MAX_PENDING_PER_USER);
        // At most 1,000 in all.
        for user in 2..=(MAX_PENDING / MAX_PENDING_PER_USER) as i64 {
            for _ in 0..MAX_PENDING_PER_USER {
                pending.create(request(user)).unwrap();
            }
        }
        assert_eq!(pending.len(), MAX_PENDING);
        assert_eq!(pending.create(request(1000)), Err(Full));
        // Someone with requests still gets a new one, in place of their oldest.
        assert!(pending.create(request(1)).is_ok());
        // Room comes back as they end.
        pending.ends_in(&newest, Duration::ZERO);
        assert!(pending.create(request(1000)).is_ok());
    }
}
