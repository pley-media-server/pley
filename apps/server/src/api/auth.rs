use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::accounts::{self, UserDto};
use crate::auth::grants::APP_TOKEN_PREFIX;
use crate::auth::{
    AuthUser, ClientIp, CurrentToken, SourceIp, clear_cookie, dummy_password_hash, forwarded_https, hash_password,
    hash_token, session_cookie, token_from_parts, validate_password, validate_username, verify_password,
};
use crate::error::{ApiResult, AppError};
use crate::state::AppState;
use crate::util::now;

#[derive(Serialize)]
pub struct LoginResponse {
    user: UserDto,
    /// For clients that aren't browsers (see [`login_response`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
    /// Two-step sign-in was set up on the way in (see [`second_step`]):
    /// its recovery codes, shown once.
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_codes: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct Credentials {
    username: String,
    password: String,
    /// A code from the authenticator app, or a recovery code, for accounts
    /// with two-step sign-in (see [`crate::two_step`]).
    #[serde(default)]
    code: Option<String>,
    /// Setting up two-step sign-in, which the server requires: the `id` of
    /// the secret the code comes from (see [`second_step`]).
    #[serde(default)]
    setup_id: Option<String>,
}

pub async fn server_status(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&state.db.read).await?;
    if users > 0 {
        // Set up, by the setup page or elsewhere (`pley reset-password
        // --admin` beside the running server): the code is done with, and
        // the tray no longer opens the setup page.
        state.auth.setup.clear();
    }
    let settings = state.settings.get();
    Ok(Json(serde_json::json!({
        "name": settings.server_name,
        "version": crate::VERSION,
        "commit": crate::COMMIT,
        "source": crate::SOURCE,
        "setup_required": users == 0,
        // Setting up takes the code in the log (see crate::setup).
        "setup_code_required": users == 0,
        // Signing in may take setting it up first (see second_step).
        "two_step_required": state.auth.two_step_required(),
    })))
}

pub(crate) async fn login_response(
    state: &AppState,
    user_id: i64,
    headers: &HeaderMap,
    ip: Option<&str>,
    recovery_codes: Option<Vec<String>>,
) -> ApiResult<Response> {
    let agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok());
    let token = state.auth.create_session(user_id, agent, ip).await?;
    let user = accounts::user(&state.db, user_id).await?.ok_or(AppError::Unauthorized)?;
    let cookie = session_cookie(&token, forwarded_https(headers));
    // Browsers (whose requests carry an Origin, which page scripts can't
    // remove) keep the session in the HttpOnly cookie only: no script on the
    // page, an injected one included, ever sees it. Scripts and apps that
    // sign in get the token to send.
    let token = (!headers.contains_key(header::ORIGIN)).then_some(token);
    Ok(([(header::SET_COOKIE, cookie)], Json(LoginResponse { user, token, recovery_codes })).into_response())
}

/// The second step of signing in as `user_id`, whose password was right:
/// a code from the authenticator app or a recovery code when two-step
/// sign-in is on. When it is off and the server requires it, setting it up:
/// the error carries a secret to add to an authenticator app until `code`
/// is its first code, sent with the secret's `setup_id`, and then the new
/// recovery codes come back. The password comes with every try, so nothing
/// is signed in until then, nor while the server can't set it up
/// ([`can_set_up_two_step`]).
pub(crate) async fn second_step(
    state: &AppState,
    user_id: i64,
    username: &str,
    code: Option<&str>,
    setup_id: Option<&str>,
) -> ApiResult<Option<Vec<String>>> {
    if state.two_step.is_on(user_id).await? {
        state.two_step.check(&state.auth.throttle, user_id, code).await?;
        return Ok(None);
    }
    if !state.auth.two_step_required() {
        return Ok(None);
    }
    can_set_up_two_step(state)?;
    let server_name = state.settings.get().server_name.clone();
    let codes = state.two_step.set_up_signing_in(user_id, username, &server_name, setup_id, code).await?;
    tracing::info!("'{username}' set up two-step sign-in, which this server requires, as they signed in");
    Ok(Some(codes))
}

/// Refuses a way in for an account that has to set up two-step sign-in,
/// which the server requires, while the server can't set it up (it can't
/// use its key): requiring it holds, so the password alone lets nobody in.
pub(crate) fn can_set_up_two_step(state: &AppState) -> ApiResult<()> {
    if state.two_step.can_set_up() {
        return Ok(());
    }
    Err(AppError::Unavailable(format!(
        "this server requires two-step sign-in, but can't set it up until an administrator fixes its {} (its log \
         says why)",
        crate::two_step::KEY_FILE
    )))
}

#[derive(Deserialize)]
pub struct SetupRequest {
    username: String,
    password: String,
    /// The code in the server's log.
    #[serde(default)]
    setup_code: Option<String>,
}

/// Create the first account, an administrator (see [`crate::setup`]).
pub async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    SourceIp(source): SourceIp,
    ClientIp(ip): ClientIp,
    Json(body): Json<SetupRequest>,
) -> ApiResult<Response> {
    let set_up: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)").fetch_one(&state.db.read).await?;
    if set_up {
        // An account made elsewhere (`pley reset-password --admin`, say).
        state.auth.setup.clear();
        return Err(AppError::conflict("Pley is already set up"));
    }
    // 5 tries every 15 minutes from each address: someone guessing doesn't
    // lock the owner out (31^8 codes; a restart makes a new one).
    let attempt = state.auth.throttle.begin_setup_code(source)?;
    if !state.auth.setup.matches(body.setup_code.as_deref().unwrap_or_default()) {
        attempt.failed();
        return Err(AppError::forbidden(
            "enter the setup code from the server's log (opening Pley from its tray icon fills it in)",
        ));
    }
    attempt.succeeded();
    let username = body.username.trim();
    validate_username(username)?;
    validate_password(&body.password, username)?;
    let hash = hash_password(body.password).await?;
    // Insert only when there are no users yet (atomic with the write pool).
    let created: Option<i64> = sqlx::query_scalar(
        "INSERT INTO users (id, username, password_hash, is_admin, created_at)
         SELECT (SELECT last + 1 FROM user_ids), ?, ?, 1, ? WHERE NOT EXISTS (SELECT 1 FROM users)
         RETURNING id",
    )
    .bind(body.username.trim())
    .bind(hash)
    .bind(now())
    .fetch_optional(&state.db.write)
    .await?;
    let id = created.ok_or_else(|| AppError::conflict("Pley is already set up"))?;
    state.auth.setup.clear();
    tracing::info!("created administrator account '{}'", body.username.trim());
    login_response(&state, id, &headers, ip.as_deref(), None).await
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    ClientIp(ip): ClientIp,
    SourceIp(source): SourceIp,
    Json(body): Json<Credentials>,
) -> ApiResult<Response> {
    let username = body.username.trim().to_owned();
    // No account has a longer name (`validate_username`): nothing to check,
    // and nothing to count and keep.
    if username.chars().count() > 64 {
        return Err(AppError::Unauthorized);
    }
    let attempt = state.auth.throttle.begin_password(&username, source)?;
    let row: Option<(i64, String, bool)> =
        sqlx::query_as("SELECT id, password_hash, disabled FROM users WHERE username = ?")
            .bind(&username)
            .fetch_optional(&state.db.read)
            .await?;
    // An account without a password (a marker starting with '!' instead of a
    // hash) never signs in with one, like a username that doesn't exist. A
    // real check against a stand-in hash takes as long as one against a
    // user's, so neither can be told apart by timing.
    let Some((id, hash, disabled)) = row.filter(|(_, hash, _)| !hash.starts_with('!')) else {
        verify_password(body.password, dummy_password_hash().await).await?;
        attempt.failed();
        return Err(AppError::Unauthorized);
    };
    if !verify_password(body.password, hash).await? {
        attempt.failed();
        return Err(AppError::Unauthorized);
    }
    attempt.succeeded();
    if disabled {
        return Err(AppError::forbidden("this account is disabled"));
    }
    let recovery_codes = second_step(&state, id, &username, body.code.as_deref(), body.setup_id.as_deref()).await?;
    login_response(&state, id, &headers, ip.as_deref(), recovery_codes).await
}

/// Checks that the signed-in user may change how their account signs in:
/// not a managed profile, not an account without a password, and with its
/// password (a session alone isn't enough).
pub(crate) async fn confirm_password(state: &AppState, user: &AuthUser, password: Option<String>) -> ApiResult<()> {
    let (hash, managed): (String, bool) = sqlx::query_as("SELECT password_hash, managed FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&state.db.read)
        .await?;
    if managed {
        return Err(AppError::forbidden("managed profiles are signed in to through their household"));
    }
    if hash.starts_with('!') {
        return Err(AppError::forbidden("this account has no password: an administrator can give it one"));
    }
    // Counted apart from sign-ins, which anyone can fail on purpose.
    let attempt = state.auth.throttle.begin(&format!("confirm:{}", user.id))?;
    if !verify_password(password.unwrap_or_default(), hash).await? {
        attempt.failed();
        return Err(AppError::forbidden("the password is wrong"));
    }
    attempt.succeeded();
    Ok(())
}

pub async fn logout(State(state): State<AppState>, request: axum::extract::Request) -> Response {
    let (parts, _) = request.into_parts();
    match token_from_parts(&parts) {
        // An app signing out ends its grant, wherever it put its token.
        Some(token) if token.starts_with(APP_TOKEN_PREFIX) => {
            state.auth.revoke_app_token(&token).await;
        }
        Some(token) => state.auth.revoke(&token).await,
        None => {}
    }
    (StatusCode::NO_CONTENT, [(header::SET_COOKIE, clear_cookie())]).into_response()
}

#[derive(Serialize)]
pub struct Me {
    #[serde(flatten)]
    user: UserDto,
    /// What the app was allowed, for an app's token (space-separated).
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<String>,
}

pub async fn me(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Me>> {
    let mut dto = accounts::user(&state.db, user.id).await?.ok_or(AppError::Unauthorized)?;
    let scope = user.app.as_ref().map(|app| app.scopes.to_string());
    if user.app.is_some() {
        // What the app may do, not what the account's role allows.
        dto.is_admin = user.is_admin;
        dto.all_libraries = user.all_libraries;
        dto.max_age = user.max_age;
    }
    // What asking from here does (an administrator's ask from other
    // publishers' apps).
    dto.request_access = state.requests.caller(&user).await?.access;
    Ok(Json(Me { user: dto, scope }))
}

#[derive(Deserialize)]
pub struct PasswordChange {
    current_password: String,
    new_password: String,
}

/// Changes the signed-in user's password, and signs them out everywhere else
/// (whoever knew the old one).
pub async fn change_password(
    State(state): State<AppState>,
    user: AuthUser,
    CurrentToken(token): CurrentToken,
    Json(body): Json<PasswordChange>,
) -> ApiResult<StatusCode> {
    validate_password(&body.new_password, &user.username)?;
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&state.db.read)
        .await?;
    if hash == accounts::NO_PASSWORD {
        return Err(AppError::bad_request("managed profiles have no password"));
    }
    if hash.starts_with('!') {
        return Err(AppError::bad_request("this account has no password: an administrator can give it one"));
    }
    let attempt = state.auth.throttle.begin(&format!("confirm:{}", user.id))?;
    if !verify_password(body.current_password, hash).await? {
        attempt.failed();
        return Err(AppError::bad_request("the current password is wrong"));
    }
    attempt.succeeded();
    let new_hash = hash_password(body.new_password).await?;
    let keep = token.as_deref().map(hash_token).unwrap_or_default();
    let mut tx = state.db.write.begin().await?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(new_hash)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash != ?")
        .bind(user.id)
        .bind(&keep)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    // The apps acting as this account or allowed by it end too, with the
    // ones it was signing in: codes not exchanged yet, even this session's,
    // and TVs allowed that haven't asked for their answer yet.
    state.oauth.forget_user(user.id);
    state.auth.sign_out_everywhere(user.id, Some(keep.as_str())).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ home profiles

/// Profiles a home member can switch to ("Who's watching?").
pub async fn profiles(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<UserDto>>> {
    let all = accounts::users(&state.db).await?;
    let me_home = all.iter().any(|u| u.id == user.id && u.home);
    if !me_home {
        return Ok(Json(Vec::new()));
    }
    Ok(Json(all.into_iter().filter(|u| u.home && !u.disabled).collect()))
}

#[derive(Deserialize)]
pub struct Switch {
    user_id: i64,
    #[serde(default)]
    pin: Option<String>,
    /// Needed to switch into a freer profile without a PIN (see [`switch`]).
    #[serde(default)]
    password: Option<String>,
    /// With the password, for a profile with two-step sign-in.
    #[serde(default)]
    code: Option<String>,
    /// With the password and code, for a profile setting up two-step
    /// sign-in (see [`second_step`]).
    #[serde(default)]
    setup_id: Option<String>,
}

/// What switching to a profile takes, from the profile one is on (see
/// [`check_switch`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Needs {
    /// Nothing: a profile without a PIN that sees no more than this one.
    None,
    /// Its PIN.
    Pin,
    /// Its password (and its code, with two-step sign-in): a freer profile
    /// without a PIN, or one that has to set up two-step sign-in, which the
    /// server requires.
    Password,
    /// No way in from here: a freer profile with neither PIN nor password,
    /// or a PIN locked after too many wrong tries.
    Locked,
}

impl Needs {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Needs::None => "none",
            Needs::Pin => "pin",
            Needs::Password => "password",
            Needs::Locked => "locked",
        }
    }
}

/// A profile someone may switch to, as seen from the profile they are on.
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct SwitchTarget {
    pub id: i64,
    pub username: String,
    /// Both are home members (the household's profile switcher).
    pub both_home: bool,
    pub disabled: bool,
    pub is_admin: bool,
    pub pin_hash: Option<String>,
    pub pin_failures: i64,
    pub password_hash: String,
    pub all_libraries: bool,
    pub max_age: Option<i64>,
    /// It sees a library the current profile can't.
    pub more_libraries: bool,
    /// It uses two-step sign-in.
    pub two_step: bool,
}

impl SwitchTarget {
    /// It has to set up two-step sign-in, which the server requires
    /// (`required`), before anyone acts as it: it signs in with a password
    /// and doesn't use it yet.
    pub fn sets_up_two_step(&self, required: bool) -> bool {
        required && !self.two_step && !self.password_hash.starts_with('!')
    }
}

const SWITCH_TARGETS: &str = "SELECT t.id, t.username, t.home AND me.home AS both_home, t.disabled, t.is_admin,
        t.pin_hash, t.pin_failures, t.password_hash, t.all_libraries, t.max_age,
        EXISTS (SELECT library_id FROM user_libraries WHERE user_id = t.id
                EXCEPT SELECT library_id FROM user_libraries WHERE user_id = me.id) AS more_libraries,
        EXISTS (SELECT 1 FROM two_step w WHERE w.user_id = t.id) AS two_step
 FROM users t, users me WHERE me.id = ?";

/// Profile `target` as seen from `from`'s; `None` when there is no such
/// profile.
pub(crate) async fn switch_target(state: &AppState, from: i64, target: i64) -> ApiResult<Option<SwitchTarget>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!("{SWITCH_TARGETS} AND t.id = ?")))
        .bind(from)
        .bind(target)
        .fetch_optional(&state.db.read)
        .await?)
}

/// Every profile as seen from `from`'s.
pub(crate) async fn switch_targets(state: &AppState, from: i64) -> ApiResult<Vec<SwitchTarget>> {
    Ok(sqlx::query_as(SWITCH_TARGETS).bind(from).fetch_all(&state.db.read).await?)
}

/// Whether `target` is freer than the profile `from` acts as: an
/// administrator, a higher age limit, or libraries it can't see. From
/// `from`'s rights for this request: an administrator's own sign-in may go
/// anywhere, but an app's token, even an administrator's, has no such
/// exemption (its `is_admin` is false).
fn is_freer(from: &AuthUser, target: &SwitchTarget) -> bool {
    !from.is_admin
        && (target.is_admin
            || from.max_age.is_some_and(|mine| target.max_age.is_none_or(|theirs| theirs > mine))
            || (!from.all_libraries && (target.all_libraries || target.more_libraries)))
}

/// What switching from `from`'s profile to `target` takes, by the rules of
/// [`check_switch`] (`two_step_required`: the server requires two-step
/// sign-in); not whether it is allowed at all (both home members, the
/// target enabled).
pub(crate) fn switch_needs(from: &AuthUser, target: &SwitchTarget, two_step_required: bool) -> Needs {
    if target.id == from.id {
        return Needs::None;
    }
    if target.sets_up_two_step(two_step_required) {
        return Needs::Password;
    }
    match (&target.pin_hash, is_freer(from, target)) {
        (Some(_), _) if target.pin_failures >= accounts::MAX_PIN_FAILURES_IN_A_ROW => Needs::Locked,
        (Some(_), _) => Needs::Pin,
        (None, true) if target.password_hash.starts_with('!') => Needs::Locked,
        (None, true) => Needs::Password,
        (None, false) => Needs::None,
    }
}

/// Why a switch was refused (see [`check_switch`]).
#[derive(Debug)]
pub(crate) enum SwitchError {
    /// No such profile.
    NotFound,
    /// Not both home members, or the target is disabled.
    NotAllowed,
    /// Too many wrong PINs or passwords for the profile lately.
    Throttled(AppError),
    /// Its PIN is locked after too many wrong ones in a row.
    PinLocked,
    /// It has a PIN, and none was given.
    PinNeeded,
    WrongPin,
    /// A freer profile with neither PIN nor password.
    NoWayIn,
    /// A freer profile without a PIN, and no password was given.
    PasswordNeeded,
    /// A profile that has to set up two-step sign-in, which the server
    /// requires, and no password was given: setting it up takes it.
    SetUpFirst,
    WrongPassword,
    /// Its second step (two-step sign-in) failed, or is being set up.
    TwoStep(AppError),
    Internal(AppError),
}

impl From<AppError> for SwitchError {
    fn from(err: AppError) -> Self {
        SwitchError::Internal(err)
    }
}

impl From<sqlx::Error> for SwitchError {
    fn from(err: sqlx::Error) -> Self {
        SwitchError::Internal(err.into())
    }
}

/// The web app's answers.
impl From<SwitchError> for AppError {
    fn from(err: SwitchError) -> Self {
        match err {
            SwitchError::NotFound => AppError::not_found("profile not found"),
            SwitchError::NotAllowed => AppError::forbidden(NOT_ALLOWED),
            // Told apart by their codes, as the apps' switch tells them by
            // its reasons: a pad that asks again, or one that stops.
            SwitchError::PinLocked => AppError::ForbiddenAs("pin_locked", PIN_LOCKED.to_owned()),
            SwitchError::PinNeeded => AppError::ForbiddenAs("wrong_pin", PIN_NEEDED.to_owned()),
            SwitchError::WrongPin => AppError::ForbiddenAs("wrong_pin", "wrong PIN".to_owned()),
            SwitchError::NoWayIn => AppError::forbidden(NO_WAY_IN),
            SwitchError::PasswordNeeded => {
                AppError::forbidden("this profile can see more than yours: it needs its password")
            }
            SwitchError::SetUpFirst => AppError::forbidden(SET_UP_FIRST),
            SwitchError::WrongPassword => AppError::forbidden("wrong password"),
            SwitchError::Throttled(err) | SwitchError::TwoStep(err) | SwitchError::Internal(err) => err,
        }
    }
}

pub(crate) const NOT_ALLOWED: &str = "you can't switch to this profile";
pub(crate) const PIN_NEEDED: &str = "this profile is protected by a PIN";
pub(crate) const PIN_LOCKED: &str =
    "this profile's PIN is locked after too many wrong tries. Its owner or an administrator can reset it";
pub(crate) const NO_WAY_IN: &str =
    "this profile can see more than yours: an administrator can give it a PIN to switch to it";
pub(crate) const SET_UP_FIRST: &str =
    "this profile has to set up two-step sign-in, which this server requires: switching to it takes its password";

/// Checks that `from` may switch to profile `target`: both home members,
/// and the target enabled. Then, unless it is the same profile: the
/// target's PIN when it has one (until 100 wrong ones in a row lock it);
/// for a profile freer than the current one (see [`is_freer`]) without a
/// PIN, its password, and its code with two-step sign-in, so age limits and
/// library choices hold. One that has neither can only be switched to from
/// a profile at least as free, until it gets a PIN. Attempts are throttled
/// per profile (`profile:{id}`) and wrong PINs counted in a row
/// (`users.pin_failures`), for the web app and apps alike.
///
/// While the server requires two-step sign-in, a profile that has to set it
/// up takes its password whatever else it has, administrators' switches
/// too, and then sets it up as a sign-in does ([`second_step`]): its
/// recovery codes come back. A PIN can't set up how an account signs in.
pub(crate) async fn check_switch(
    state: &AppState,
    from: &AuthUser,
    target: i64,
    pin: Option<&str>,
    password: Option<&str>,
    code: Option<&str>,
    setup_id: Option<&str>,
) -> Result<Option<Vec<String>>, SwitchError> {
    let Some(profile) = switch_target(state, from.id, target).await? else {
        return Err(SwitchError::NotFound);
    };
    if !profile.both_home || profile.disabled {
        return Err(SwitchError::NotAllowed);
    }
    if target == from.id {
        return Ok(None);
    }
    let freer = is_freer(from, &profile);
    let attempt = state.auth.throttle.begin(&format!("profile:{target}")).map_err(SwitchError::Throttled)?;
    if profile.sets_up_two_step(state.auth.two_step_required()) {
        let Some(password) = password else { return Err(SwitchError::SetUpFirst) };
        if !verify_password(password.to_owned(), profile.password_hash.clone()).await? {
            attempt.failed();
            return Err(SwitchError::WrongPassword);
        }
        attempt.succeeded();
        return second_step(state, target, &profile.username, code, setup_id).await.map_err(SwitchError::TwoStep);
    }
    let ok = match (&profile.pin_hash, freer) {
        (Some(pin_hash), _) => match pin {
            Some(pin) => check_pin(state, &profile, pin_hash, pin).await?,
            None => return Err(SwitchError::PinNeeded),
        },
        (None, true) if profile.password_hash.starts_with('!') => return Err(SwitchError::NoWayIn),
        (None, true) => match password {
            Some(password) => verify_password(password.to_owned(), profile.password_hash.clone()).await?,
            None => return Err(SwitchError::PasswordNeeded),
        },
        (None, false) => true,
    };
    if !ok {
        attempt.failed();
        return Err(if profile.pin_hash.is_some() { SwitchError::WrongPin } else { SwitchError::WrongPassword });
    }
    if profile.pin_hash.is_none() && freer {
        // Its password signs in to it: so does its second step.
        state.two_step.check(&state.auth.throttle, target, code).await.map_err(SwitchError::TwoStep)?;
    }
    attempt.succeeded();
    Ok(None)
}

/// Whether `pin` is `profile`'s PIN (`pin_hash`), counting it in a row
/// (`users.pin_failures`): throttled per profile by the caller, and capped
/// in a row, so guessing a 4-digit PIN over weeks doesn't get through
/// either.
async fn check_pin(state: &AppState, profile: &SwitchTarget, pin_hash: &str, pin: &str) -> Result<bool, SwitchError> {
    if profile.pin_failures >= accounts::MAX_PIN_FAILURES_IN_A_ROW {
        return Err(SwitchError::PinLocked);
    }
    let right = verify_password(pin.to_owned(), pin_hash.to_owned()).await?;
    if !right || profile.pin_failures > 0 {
        sqlx::query("UPDATE users SET pin_failures = CASE WHEN ? THEN 0 ELSE pin_failures + 1 END WHERE id = ?")
            .bind(right)
            .bind(profile.id)
            .execute(&state.db.write)
            .await?;
    }
    Ok(right)
}

/// Checks the PIN of the profile `user` is on, for an app asking "Who's
/// watching?" as it starts (the switch-profile grant to its own profile):
/// by [`check_switch`]'s rules, throttled per profile and locked after 100
/// wrong ones in a row, counted with the switches to it. `Ok(false)`: the
/// profile has no PIN, so there is nothing to check.
pub(crate) async fn check_own_pin(state: &AppState, user: &AuthUser, pin: &str) -> Result<bool, SwitchError> {
    let Some(profile) = switch_target(state, user.id, user.id).await? else {
        return Err(SwitchError::NotFound);
    };
    let Some(pin_hash) = &profile.pin_hash else {
        return Ok(false);
    };
    let attempt = state.auth.throttle.begin(&format!("profile:{}", user.id)).map_err(SwitchError::Throttled)?;
    if !check_pin(state, &profile, pin_hash, pin).await? {
        attempt.failed();
        return Err(SwitchError::WrongPin);
    }
    attempt.succeeded();
    Ok(true)
}

/// Switch this device to another home profile, by the rules of
/// [`check_switch`]: the session it came from ends, and a new one starts.
pub async fn switch(
    State(state): State<AppState>,
    user: AuthUser,
    headers: HeaderMap,
    ClientIp(ip): ClientIp,
    CurrentToken(token): CurrentToken,
    Json(body): Json<Switch>,
) -> ApiResult<Response> {
    let recovery_codes = check_switch(
        &state,
        &user,
        body.user_id,
        body.pin.as_deref(),
        body.password.as_deref(),
        body.code.as_deref(),
        body.setup_id.as_deref(),
    )
    .await?;
    if let Some(token) = token {
        state.auth.revoke(&token).await;
    }
    login_response(&state, body.user_id, &headers, ip.as_deref(), recovery_codes).await
}
