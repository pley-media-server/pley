//! Pley's authorization server as the LG app meets it (docs/API.md, "Apps
//! (OAuth 2.1)", with the device grant of RFC 8628): discovery with the
//! issuer from Host, the device grant (approved at once, or after a delay),
//! refresh rotation with reuse detection, the switch-profile grant with Sam's
//! PIN, revocation, the household's profiles, and every route's token check.
//! Only Pley for LG TV's client id is known ([`ids::CLIENT_ID`]).

use std::collections::HashMap;

use ring::hmac;
use serde_json::json;

use crate::http::{Request, Response};
use crate::ids;
use crate::model::AppProfile;
use crate::random::{base64url, from_base64url, token, unique_u64};
use crate::server::{
    ALL_SCOPES, Answer, Caller, Cx, LIBRARY, Need, PROFILES, Route, SCOPES, State, error, json, locked, no_content,
    scope_names,
};
use crate::{DeviceApproval, Options, Tokens};

pub(crate) const SWITCH_PROFILE: &str = "urn:pley:params:oauth:grant-type:switch-profile";
pub(crate) const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub(crate) const ACCESS_LIFETIME_S: i64 = 3_600;
const DEVICE_LIFETIME_S: i64 = 600;
/// RFC 8628 §6.1's consonants.
const USER_CODE_LETTERS: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
/// A grant keeps its newest access tokens only.
const LIVE_ACCESS_TOKENS: usize = 10;

/// One sign-in of an app: its tokens come and go, it stays.
struct Grant {
    profile: i64,
    scopes: u8,
    ended: bool,
    /// Its access tokens and when each ends (ms), oldest first.
    access: Vec<(String, i64)>,
    refresh: Option<String>,
}

struct RefreshToken {
    grant: u64,
    spent: bool,
}

/// A TV waiting for its code to be approved.
struct Device {
    scopes: u8,
    created_ms: i64,
    expires_ms: i64,
    interval_s: i64,
    last_poll_ms: Option<i64>,
}

/// The sign-ins and their tokens.
pub(crate) struct OAuth {
    grants: HashMap<u64, Grant>,
    next_grant: u64,
    access: HashMap<String, u64>,
    refresh: HashMap<String, RefreshToken>,
    devices: HashMap<String, Device>,
    /// The profile sign-in codes sign in as.
    pub sign_in_as: i64,
    pub approval: DeviceApproval,
    /// How long approving a code takes, in ms.
    pub delay_ms: i64,
    pub interval_s: i64,
    pub adopt_unknown_tokens: bool,
    pub refuse_access_tokens: bool,
}

impl OAuth {
    pub fn new(options: &Options) -> OAuth {
        OAuth {
            grants: HashMap::new(),
            next_grant: 1,
            access: HashMap::new(),
            refresh: HashMap::new(),
            devices: HashMap::new(),
            sign_in_as: ids::ALEX,
            approval: DeviceApproval::Allow,
            delay_ms: options.device_delay.as_millis() as i64,
            interval_s: options.device_interval_secs as i64,
            adopt_unknown_tokens: options.adopt_unknown_tokens,
            refuse_access_tokens: false,
        }
    }

    fn new_grant(&mut self, profile: i64, scopes: u8) -> u64 {
        let id = self.next_grant;
        self.next_grant += 1;
        self.grants.insert(id, Grant { profile, scopes, ended: false, access: Vec::new(), refresh: None });
        id
    }

    /// Who a request comes from, by its access token; or the refusal: a token
    /// outside the Authorization header, none, or one that doesn't work.
    pub fn authenticate(&self, request: &Request, now_ms: i64, require_auth: bool) -> Result<Caller, Response> {
        if request.query("token").is_some() || request.header("x-pley-token").is_some() {
            return Err(error(400, "bad_request", "an app's token goes in the Authorization header only")
                .with_header("WWW-Authenticate", "Bearer error=\"invalid_request\""));
        }
        let Some(header) = request.header("authorization") else {
            if !require_auth {
                return Ok(Caller { profile: self.sign_in_as, scopes: ALL_SCOPES, grant: None, expires_at_ms: None });
            }
            return Err(error(401, "unauthorized", "authentication required").with_header("WWW-Authenticate", "Bearer"));
        };
        let refused = || {
            error(401, "unauthorized", "this token doesn't work any more: refresh it, or sign in again")
                .with_header("WWW-Authenticate", "Bearer error=\"invalid_token\"")
        };
        let token = match header.split_once(' ') {
            Some((scheme, token)) if scheme.eq_ignore_ascii_case("bearer") => token.trim(),
            _ => return Err(refused()),
        };
        if self.refuse_access_tokens {
            return Err(refused());
        }
        let Some(&grant_id) = self.access.get(token) else {
            if self.adopt_unknown_tokens && token.starts_with("pley_at_") {
                return Ok(Caller {
                    profile: self.sign_in_as,
                    scopes: ALL_SCOPES,
                    grant: None,
                    expires_at_ms: Some(now_ms + ACCESS_LIFETIME_S * 1_000),
                });
            }
            return Err(refused());
        };
        let grant = &self.grants[&grant_id];
        let expires = grant.access.iter().find(|(t, _)| t == token).map(|(_, at)| *at);
        match expires {
            Some(at) if !grant.ended && at > now_ms => Ok(Caller {
                profile: grant.profile,
                scopes: grant.scopes,
                grant: Some(grant_id),
                expires_at_ms: Some(at),
            }),
            _ => Err(refused()),
        }
    }

    /// A grant for `profile` with `scopes`, as if the app had signed in.
    pub fn grant_for(&mut self, profile: i64, scopes: u8, now_ms: i64) -> Tokens {
        let grant = self.new_grant(profile, scopes);
        self.issue(grant, now_ms)
    }

    /// Ends every access token now.
    pub fn expire_access(&mut self, now_ms: i64) {
        for grant in self.grants.values_mut() {
            for (_, at) in &mut grant.access {
                *at = (*at).min(now_ms);
            }
        }
    }

    /// A new token pair for `grant`; the refresh token before it is spent.
    fn issue(&mut self, grant_id: u64, now_ms: i64) -> Tokens {
        let access = format!("pley_at_{}", token(32));
        let refresh = format!("pley_rt_{}", token(32));
        let grant = self.grants.get_mut(&grant_id).expect("grants are kept");
        grant.access.push((access.clone(), now_ms + ACCESS_LIFETIME_S * 1_000));
        while grant.access.len() > LIVE_ACCESS_TOKENS {
            let (oldest, _) = grant.access.remove(0);
            self.access.remove(&oldest);
        }
        if let Some(old) = grant.refresh.replace(refresh.clone())
            && let Some(old) = self.refresh.get_mut(&old)
        {
            old.spent = true;
        }
        let scope = scope_names(grant.scopes);
        self.access.insert(access.clone(), grant_id);
        self.refresh.insert(refresh.clone(), RefreshToken { grant: grant_id, spent: false });
        Tokens {
            access_token: access,
            refresh_token: refresh,
            expires_in: ACCESS_LIFETIME_S,
            scope,
            proof_id: grant_id.to_string(),
            proof_key: base64url(&grant_key(grant_id)),
        }
    }

    fn end(&mut self, grant_id: u64) {
        if let Some(grant) = self.grants.get_mut(&grant_id) {
            grant.ended = true;
            for (token, _) in grant.access.drain(..) {
                self.access.remove(&token);
            }
        }
    }

    /// The grant of a refresh token, which it spends; `None` when it doesn't
    /// work. One shown again ends its grant.
    fn spend(&mut self, refresh: Option<&str>) -> Option<u64> {
        let refresh = refresh?;
        if self.adopt_unknown_tokens && refresh.starts_with("pley_rt_") && !self.refresh.contains_key(refresh) {
            let grant = self.new_grant(self.sign_in_as, ALL_SCOPES);
            self.grants.get_mut(&grant).expect("just made").refresh = Some(refresh.to_string());
            self.refresh.insert(refresh.to_string(), RefreshToken { grant, spent: false });
        }
        let token = self.refresh.get_mut(refresh)?;
        let grant = token.grant;
        if token.spent {
            self.end(grant);
            return None;
        }
        if self.grants[&grant].ended {
            return None;
        }
        token.spent = true;
        Some(grant)
    }
}

pub(crate) fn routes() -> Vec<Route> {
    vec![
        locked("GET", "/.well-known/oauth-authorization-server", Need::Public, metadata),
        locked("GET", "/.well-known/*", Need::Public, |_, _| Err(crate::server::not_found("no such document"))),
        locked("POST", "/api/oauth/token", Need::Public, token_endpoint),
        locked("POST", "/api/oauth/revoke", Need::Public, revoke),
        locked("POST", "/api/oauth/device", Need::Public, device_authorization),
        locked("GET", "/api/oauth/profiles", Need::Scope(PROFILES), profiles),
        locked("POST", "/api/auth/logout", Need::Public, |cx, state| {
            if let Some(grant) = cx.maybe_caller().and_then(|c| c.grant) {
                state.oauth.end(grant);
            }
            Ok(no_content())
        }),
    ]
}

/// The issuer: the address the request came to (docs/API.md, "Discovery").
fn issuer(request: &Request) -> Option<String> {
    let host = request.header("host")?.to_ascii_lowercase();
    if host.is_empty() || host.contains(['/', '\\', '?', '#', '@']) {
        return None;
    }
    let https = request.header("x-forwarded-proto").and_then(|v| v.split(',').next()).map(str::trim) == Some("https");
    let host =
        if https { host.strip_suffix(":443").unwrap_or(&host) } else { host.strip_suffix(":80").unwrap_or(&host) };
    Some(format!("{}://{host}", if https { "https" } else { "http" }))
}

fn metadata(cx: &Cx<'_>, _: &mut State) -> Answer {
    let issuer = issuer(cx.req).ok_or_else(|| crate::server::bad_request("the Host header is missing or wrong"))?;
    let scopes: Vec<&str> = SCOPES.iter().map(|(n, _)| *n).collect();
    let body = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/api/oauth/token"),
        "revocation_endpoint": format!("{issuer}/api/oauth/revoke"),
        "device_authorization_endpoint": format!("{issuer}/api/oauth/device"),
        "response_types_supported": ["code"],
        "response_modes_supported": ["query"],
        "grant_types_supported": ["authorization_code", "refresh_token", SWITCH_PROFILE, DEVICE_GRANT],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "code_challenge_methods_supported": ["S256"],
        "scopes_supported": scopes,
        "authorization_response_iss_parameter_supported": true,
        "client_id_metadata_document_supported": false,
        "service_documentation": "https://github.com/pley-media-server/pley/blob/master/docs/API.md",
    });
    Ok(json(200, &body).with_header("Cache-Control", "no-store"))
}

/// An RFC 6749 §5.2 error, with Pley's `reason` for profile switches.
fn oauth_error(error: &str, description: &str, reason: Option<&str>) -> Response {
    let mut body = json!({ "error": error, "error_description": description });
    if let Some(reason) = reason {
        body["reason"] = reason.into();
    }
    no_store(json(400, &body))
}

fn no_store(response: Response) -> Response {
    response.with_header("Cache-Control", "no-store").with_header("Pragma", "no-cache")
}

/// The form's values, or the refusal of a body that isn't one form with each
/// parameter once.
fn form(request: &Request) -> Result<HashMap<String, String>, Response> {
    let is_form = request
        .header("content-type")
        .is_some_and(|t| t.to_ascii_lowercase().starts_with("application/x-www-form-urlencoded"));
    if !is_form {
        return Err(oauth_error(
            "invalid_request",
            "send the parameters as a form (application/x-www-form-urlencoded)",
            None,
        ));
    }
    let mut out = HashMap::new();
    for (name, value) in request.form() {
        if out.insert(name, value).is_some() {
            return Err(oauth_error("invalid_request", "each parameter may be given once", None));
        }
    }
    if out.contains_key("client_secret") || request.header("authorization").is_some_and(|a| a.starts_with("Basic ")) {
        let mut refusal = oauth_error("invalid_client", "Pley's apps have no secrets", None);
        refusal.status = 401;
        if request.header("authorization").is_some() {
            refusal.set_header("WWW-Authenticate", "Basic realm=\"pley\"");
        }
        return Err(refusal);
    }
    Ok(out)
}

/// Asked scopes; none is `library`. `None` when one isn't Pley's.
fn scopes(asked: Option<&str>) -> Option<u8> {
    let mut bits = 0;
    for name in asked.unwrap_or("").split(' ').filter(|n| !n.is_empty()) {
        bits |= SCOPES.iter().find(|(n, _)| *n == name)?.1;
    }
    Some(if bits == 0 { LIBRARY } else { bits })
}

fn tokens_answer(tokens: &Tokens) -> Response {
    no_store(json(
        200,
        &json!({
            "access_token": tokens.access_token,
            "token_type": "Bearer",
            "expires_in": tokens.expires_in,
            "scope": tokens.scope,
            "refresh_token": tokens.refresh_token,
            "server_id": ids::SERVER_ID,
            "proof_id": tokens.proof_id,
            "proof_key": tokens.proof_key,
        }),
    ))
}

/// What the server's proof keys are derived from: the real server's comes
/// from its `secret.key`; the fake one's is the same on every run, so a TV
/// that kept a sign-in from an earlier run still finds its server.
const PROOF_ROOT: &[u8] = b"pley fixture server proofs";

/// The proof key of the sign-in `grant`, the same at every refresh (the
/// server's `Identity::proof_key`).
fn grant_key(grant: u64) -> Vec<u8> {
    let root = hmac::Key::new(hmac::HMAC_SHA256, PROOF_ROOT);
    hmac::sign(&root, format!("grant {grant}").as_bytes()).as_ref().to_vec()
}

/// GET /api/server's `proof` and the address it covers, when the request
/// asks for one with `challenge` and `proof_id` (docs/API.md, "Telling
/// servers apart"): HMAC-SHA256 under the sign-in's key of
/// `pley-server-proof`, the server's id, the address it was asked at and the
/// challenge. Unlike the real server it doesn't look at where the request
/// comes from: it listens on 127.0.0.1 only, where every client is at home.
pub(crate) fn server_proof(request: &Request) -> Result<Option<(String, String)>, Response> {
    let (challenge, proof_id) = match (request.query("challenge"), request.query("proof_id")) {
        (None, None) => return Ok(None),
        (Some(challenge), Some(proof_id)) => (challenge, proof_id),
        _ => return Err(crate::server::bad_request("a proof takes both challenge and proof_id")),
    };
    let grant = proof_id
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && id.to_string() == proof_id)
        .ok_or_else(|| crate::server::bad_request("proof_id isn't a sign-in's id"))?;
    // A challenge decodes from one spelling only, so the proof covers what the app sent.
    let canonical =
        from_base64url(challenge).is_some_and(|b| (16..=64).contains(&b.len()) && base64url(&b) == challenge);
    if !canonical {
        return Err(crate::server::bad_request("challenge must be 16 to 64 bytes in base64url without padding"));
    }
    let address = issuer(request).ok_or_else(|| crate::server::bad_request("the Host header is missing or wrong"))?;
    let key = hmac::Key::new(hmac::HMAC_SHA256, &grant_key(grant));
    let text = format!("pley-server-proof\n{}\n{address}\n{challenge}", ids::SERVER_ID);
    Ok(Some((base64url(hmac::sign(&key, text.as_bytes()).as_ref()), address)))
}

/// POST /api/oauth/device: a TV starts signing in with a code.
fn device_authorization(cx: &Cx<'_>, state: &mut State) -> Answer {
    let form = form(cx.req)?;
    let client = form.get("client_id").ok_or_else(|| oauth_error("invalid_request", "client_id is missing", None))?;
    if client != ids::CLIENT_ID {
        let mut refusal = oauth_error("invalid_client", "only Pley's own apps can sign in with a code", None);
        refusal.status = 401;
        return Err(refusal);
    }
    let scopes = scopes(form.get("scope").map(String::as_str))
        .ok_or_else(|| oauth_error("invalid_scope", "Pley doesn't offer that scope", None))?;
    let issuer =
        issuer(cx.req).ok_or_else(|| oauth_error("invalid_request", "the Host header is missing or wrong", None))?;
    if let Some(resource) = form.get("resource")
        && resource.trim_end_matches('/') != issuer
    {
        return Err(oauth_error("invalid_target", "resource names another server", None));
    }
    let device_code = token(32);
    let letters: String =
        (0..8).map(|_| USER_CODE_LETTERS[(unique_u64() % USER_CODE_LETTERS.len() as u64) as usize] as char).collect();
    let now = cx.now_ms();
    let interval = state.oauth.interval_s;
    state.oauth.devices.insert(
        device_code.clone(),
        Device {
            scopes,
            created_ms: now,
            expires_ms: now + DEVICE_LIFETIME_S * 1_000,
            interval_s: interval,
            last_poll_ms: None,
        },
    );
    Ok(no_store(json(
        200,
        &json!({
            "device_code": device_code,
            "user_code": format!("{}-{}", &letters[..4], &letters[4..]),
            "verification_uri": format!("{issuer}/link"),
            "verification_uri_complete": format!("{issuer}/link?code={letters}"),
            "expires_in": DEVICE_LIFETIME_S,
            "interval": interval,
        }),
    )))
}

/// POST /api/oauth/token: the device, refresh and switch-profile grants.
fn token_endpoint(cx: &Cx<'_>, state: &mut State) -> Answer {
    let form = form(cx.req)?;
    let now = cx.now_ms();
    match form.get("grant_type").map(String::as_str) {
        None => Err(oauth_error("invalid_request", "grant_type is missing", None)),
        Some(DEVICE_GRANT) => device_grant(&form, state, now),
        Some("refresh_token") => refresh_grant(&form, state, now),
        Some(SWITCH_PROFILE) => switch_profile(&form, state, now),
        Some("authorization_code") => {
            Err(oauth_error("unauthorized_client", "this app signs in with a code shown on the TV", None))
        }
        Some(_) => Err(oauth_error("unsupported_grant_type", "Pley doesn't offer that grant", None)),
    }
}

fn device_grant(form: &HashMap<String, String>, state: &mut State, now: i64) -> Answer {
    let code = form.get("device_code").ok_or_else(|| oauth_error("invalid_request", "device_code is missing", None))?;
    let client = form.get("client_id").ok_or_else(|| oauth_error("invalid_request", "client_id is missing", None))?;
    let oauth = &mut state.oauth;
    let (approval, delay) = (oauth.approval, oauth.delay_ms);
    let device = match oauth.devices.get_mut(code) {
        Some(device) if client == ids::CLIENT_ID => device,
        _ => return Err(oauth_error("invalid_grant", "unknown device_code", None)),
    };
    if device.expires_ms <= now {
        oauth.devices.remove(code);
        return Err(oauth_error("expired_token", "the code expired: start again", None));
    }
    let too_soon = device.last_poll_ms.is_some_and(|last| now - last < device.interval_s * 1_000);
    device.last_poll_ms = Some(now);
    if too_soon {
        device.interval_s += 5;
        return Err(oauth_error("slow_down", "asked too soon: wait 5 seconds longer between questions", None));
    }
    if approval == DeviceApproval::Never || now < device.created_ms + delay {
        return Err(oauth_error("authorization_pending", "the person hasn't answered yet", None));
    }
    let scopes = device.scopes;
    oauth.devices.remove(code);
    if approval == DeviceApproval::Deny {
        return Err(oauth_error("access_denied", "the person didn't allow this app", None));
    }
    let profile = oauth.sign_in_as;
    let grant = oauth.new_grant(profile, scopes);
    Ok(tokens_answer(&oauth.issue(grant, now)))
}

fn spent() -> Response {
    oauth_error("invalid_grant", "refresh token used again or unknown: the app's access is ended", None)
}

fn refresh_grant(form: &HashMap<String, String>, state: &mut State, now: i64) -> Answer {
    if form.get("client_id").map(String::as_str) != Some(ids::CLIENT_ID) {
        return Err(oauth_error("invalid_grant", "unknown refresh token", None));
    }
    let oauth = &mut state.oauth;
    let grant = oauth.spend(form.get("refresh_token").map(String::as_str)).ok_or_else(spent)?;
    if let Some(asked) = form.get("scope") {
        let asked =
            scopes(Some(asked)).ok_or_else(|| oauth_error("invalid_scope", "Pley doesn't offer that scope", None))?;
        if asked & !oauth.grants[&grant].scopes != 0 {
            return Err(oauth_error("invalid_scope", "more than the app was allowed", None));
        }
    }
    Ok(tokens_answer(&oauth.issue(grant, now)))
}

/// The switch-profile grant: to another profile of the household (its PIN
/// when it has one), or to the app's own profile with its PIN, which checks
/// the PIN and rotates the tokens. A refusal spends nothing.
fn switch_profile(form: &HashMap<String, String>, state: &mut State, now: i64) -> Answer {
    if form.get("client_id").map(String::as_str) != Some(ids::CLIENT_ID) {
        return Err(oauth_error("unauthorized_client", "only Pley's own apps switch profiles", None));
    }
    let refresh = form.get("refresh_token").cloned().unwrap_or_default();
    let (grant_id, spent_token) = match state.oauth.refresh.get(&refresh) {
        Some(token) => (token.grant, token.spent),
        None => return Err(spent()),
    };
    if spent_token {
        state.oauth.end(grant_id);
        return Err(spent());
    }
    let (current, scopes) = {
        let grant = &state.oauth.grants[&grant_id];
        if grant.ended {
            return Err(spent());
        }
        (grant.profile, grant.scopes)
    };
    if scopes & PROFILES == 0 {
        return Err(oauth_error("unauthorized_client", "this app wasn't allowed to switch profiles", None));
    }
    let target = form
        .get("profile")
        .and_then(|p| p.parse::<i64>().ok())
        .ok_or_else(|| oauth_error("invalid_request", "profile is missing or not an id", None))?;
    let pin = form.get("pin");
    let now_s = now.div_euclid(1_000);
    let Some(profile) = state.household.profile_mut(target) else {
        return Err(oauth_error("invalid_grant", "not a profile of the household", Some("not_allowed")));
    };
    if target == current && (pin.is_none() || profile.pin.is_none()) {
        return Err(oauth_error(
            "invalid_request",
            "this server can't check PINs: the app uses this profile already",
            None,
        ));
    }
    if let Some(expected) = profile.pin {
        profile.wrong_pins.retain(|at| *at >= now_s - 15 * 60);
        if profile.wrong_pins.len() >= 5 {
            return Err(oauth_error("invalid_grant", "too many attempts, try again in 15 min", Some("throttled")));
        }
        match pin {
            None => return Err(oauth_error("invalid_grant", "this profile is protected by a PIN", Some("wrong_pin"))),
            Some(pin) if pin != expected => {
                profile.wrong_pins.push(now_s);
                return Err(oauth_error("invalid_grant", "wrong PIN", Some("wrong_pin")));
            }
            Some(_) => profile.wrong_pins.clear(),
        }
    } else if profile.needs() == "password" {
        return Err(oauth_error(
            "invalid_grant",
            "this profile needs its password: sign the app in again",
            Some("browser"),
        ));
    }
    let oauth = &mut state.oauth;
    if let Some(token) = oauth.refresh.get_mut(&refresh) {
        token.spent = true;
    }
    // The old tokens stop at once.
    let grant = oauth.grants.get_mut(&grant_id).expect("grants are kept");
    let old: Vec<String> = grant.access.drain(..).map(|(t, _)| t).collect();
    grant.profile = target;
    for token in old {
        oauth.access.remove(&token);
    }
    Ok(tokens_answer(&oauth.issue(grant_id, now)))
}

/// POST /api/oauth/revoke: any token of the app ends its whole sign-in.
fn revoke(cx: &Cx<'_>, state: &mut State) -> Answer {
    let form = form(cx.req)?;
    let token = form.get("token").ok_or_else(|| oauth_error("invalid_request", "token is missing", None))?;
    if !form.contains_key("client_id") {
        return Err(oauth_error("invalid_request", "client_id is missing", None));
    }
    let oauth = &mut state.oauth;
    let grant = oauth.access.get(token).copied().or_else(|| oauth.refresh.get(token).map(|r| r.grant));
    if let Some(grant) = grant {
        oauth.end(grant);
    }
    Ok(no_store(Response::new(200)))
}

/// GET /api/oauth/profiles: the household, and what switching to each takes.
fn profiles(cx: &Cx<'_>, state: &mut State) -> Answer {
    let me = cx.caller().profile;
    let list: Vec<AppProfile> = state
        .household
        .profiles
        .values()
        .map(|p| AppProfile {
            id: p.id,
            name: p.name.to_string(),
            avatar_url: p.avatar_url(),
            avatar_color: Some(p.avatar_color.to_string()),
            current: p.id == me,
            needs: if p.id == me { "none" } else { p.needs() },
        })
        .collect();
    Ok(json(200, &list))
}
