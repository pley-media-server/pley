//! Accounts: profile data, the users list as the API returns it, PIN and
//! password attempt throttling, avatars, device names and invitation codes.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use image::codecs::jpeg::JpegEncoder;
use serde::Serialize;

use crate::db::Db;
use crate::error::AppError;
use crate::requests::RequestAccess;
use crate::streams::SubtitleMode;

/// Stored instead of a password hash for managed profiles: never verifies.
pub const NO_PASSWORD: &str = "!managed";
const AVATAR_SIZE: u32 = 256;

#[derive(Debug, Clone, sqlx::FromRow)]
struct UserRow {
    id: i64,
    username: String,
    display_name: Option<String>,
    is_admin: bool,
    managed: bool,
    home: bool,
    has_pin: bool,
    avatar: Option<String>,
    avatar_color: Option<String>,
    all_libraries: bool,
    max_age: Option<i64>,
    disabled: bool,
    created_at: i64,
    audio_language: Option<String>,
    subtitle_language: Option<String>,
    subtitle_mode: String,
    has_password: bool,
    two_step: bool,
    request_access: String,
}

/// A user as the API shows it.
#[derive(Debug, Clone, Serialize)]
pub struct UserDto {
    pub id: i64,
    pub username: String,
    /// What to show: the display name, or the username.
    pub name: String,
    pub display_name: Option<String>,
    pub is_admin: bool,
    /// A profile without a password, reached by switching from a home member.
    pub managed: bool,
    /// Listed in the profile switcher of the household.
    pub home: bool,
    pub has_pin: bool,
    pub avatar_url: Option<String>,
    pub avatar_color: Option<String>,
    pub all_libraries: bool,
    pub library_ids: Vec<i64>,
    pub max_age: Option<i64>,
    pub disabled: bool,
    pub created_at: i64,
    /// Default tracks for titles without a choice (see `crate::streams`).
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
    pub subtitle_mode: SubtitleMode,
    /// Signs in with a password: false for a managed profile (reached by
    /// switching), and for an account without one until an administrator
    /// gives it one.
    pub has_password: bool,
    /// Signs in with a code from an authenticator app after the password.
    pub two_step: bool,
    /// Whether it may ask for movies and shows (`crate::requests`):
    /// administrators always are approved as they ask.
    pub request_access: RequestAccess,
}

const USER_COLUMNS: &str = "id, username, display_name, is_admin, managed, home, pin_hash IS NOT NULL AS has_pin,
    avatar, avatar_color, all_libraries, max_age, disabled, created_at, audio_language, subtitle_language,
    subtitle_mode, password_hash NOT LIKE '!%' AS has_password,
    EXISTS (SELECT 1 FROM two_step t WHERE t.user_id = users.id) AS two_step, request_access";

impl UserRow {
    fn into_dto(self, library_ids: Vec<i64>) -> UserDto {
        UserDto {
            name: self.display_name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| self.username.clone()),
            avatar_url: avatar_url(self.id, self.avatar.as_deref()),
            id: self.id,
            username: self.username,
            display_name: self.display_name,
            is_admin: self.is_admin,
            managed: self.managed,
            home: self.home,
            has_pin: self.has_pin,
            avatar_color: self.avatar_color,
            all_libraries: self.all_libraries || self.is_admin,
            library_ids,
            max_age: if self.is_admin { None } else { self.max_age },
            disabled: self.disabled,
            created_at: self.created_at,
            audio_language: self.audio_language,
            subtitle_language: self.subtitle_language,
            subtitle_mode: SubtitleMode::parse(&self.subtitle_mode),
            has_password: self.has_password,
            two_step: self.two_step,
            request_access: if self.is_admin {
                RequestAccess::Auto
            } else {
                RequestAccess::parse(&self.request_access)
            },
        }
    }
}

/// `"7-1734567890.jpg"` → `"1734567890"`, for cache-busting avatar URLs.
fn version_of(file: &str) -> &str {
    file.rsplit_once('-').map(|(_, v)| v.trim_end_matches(".jpg")).unwrap_or(file)
}

/// `/api/users/{id}/avatar?v=…` for a user with an avatar file.
pub fn avatar_url(user_id: i64, avatar: Option<&str>) -> Option<String> {
    avatar.map(|a| format!("/api/users/{user_id}/avatar?v={}", version_of(a)))
}

pub async fn user(db: &Db, id: i64) -> sqlx::Result<Option<UserDto>> {
    let row: Option<UserRow> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?")))
            .bind(id)
            .fetch_optional(&db.read)
            .await?;
    let Some(row) = row else { return Ok(None) };
    let ids = sqlx::query_scalar("SELECT library_id FROM user_libraries WHERE user_id = ? ORDER BY library_id")
        .bind(id)
        .fetch_all(&db.read)
        .await?;
    Ok(Some(row.into_dto(ids)))
}

pub async fn users(db: &Db) -> sqlx::Result<Vec<UserDto>> {
    let rows: Vec<UserRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} FROM users ORDER BY managed, COALESCE(display_name, username) COLLATE NOCASE"
    )))
    .fetch_all(&db.read)
    .await?;
    let pairs: Vec<(i64, i64)> = sqlx::query_as("SELECT user_id, library_id FROM user_libraries ORDER BY library_id")
        .fetch_all(&db.read)
        .await?;
    let mut by_user: HashMap<i64, Vec<i64>> = HashMap::new();
    for (user, library) in pairs {
        by_user.entry(user).or_default().push(library);
    }
    Ok(rows
        .into_iter()
        .map(|r| {
            let libraries = by_user.remove(&r.id).unwrap_or_default();
            r.into_dto(libraries)
        })
        .collect())
}

/// Replace a user's library list.
pub async fn set_libraries(db: &Db, user_id: i64, library_ids: &[i64]) -> sqlx::Result<()> {
    let mut tx = db.write.begin().await?;
    sqlx::query("DELETE FROM user_libraries WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
    for id in library_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO user_libraries (user_id, library_id) SELECT ?, id FROM libraries WHERE id = ?",
        )
        .bind(user_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Wrong PINs in a row after which a profile's PIN stops working until it is
/// set again (NIST SP 800-63B, 5.2.2: no more than 100).
pub const MAX_PIN_FAILURES_IN_A_ROW: i64 = 100;

pub fn validate_pin(pin: &str) -> Result<(), AppError> {
    if (4..=8).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err(AppError::bad_request("the PIN must be 4 to 8 digits"))
    }
}

pub fn validate_color(color: &str) -> Result<(), AppError> {
    let hex = color.strip_prefix('#').unwrap_or_default();
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::bad_request("the color must look like #38bdf8"))
    }
}

// ------------------------------------------------------------------ throttling

/// Password and PIN attempts per key (username, profile id). After
/// `MAX_FAILURES` failures within the window, attempts are refused until it
/// ends, which makes guessing a 4-digit PIN impractical.
///
/// An attempt is reserved before the secret is checked ([`Throttle::begin`])
/// and attempts still being checked count towards the limit, so firing many
/// at once doesn't get more guesses than firing them one by one.
#[derive(Default)]
pub struct Throttle {
    attempts: DashMap<String, Attempts>,
}

#[derive(Debug, Clone, Copy)]
struct Attempts {
    failures: u32,
    in_flight: u32,
    /// When the current window started (its first attempt).
    since: Instant,
}

const MAX_FAILURES: u32 = 5;
/// Failed sign-ins to one account from every address together: someone
/// guessing from elsewhere (5 a window from each address) doesn't lock its
/// owner out, and many addresses still don't get many guesses.
const MAX_ACCOUNT_FAILURES: u32 = 50;
/// Wrong setup codes from every address together (31^8 codes).
const MAX_SETUP_FAILURES: u32 = 1000;
const WINDOW: Duration = Duration::from_secs(15 * 60);
/// Keys counted at most. Each failure is an Argon2 check, so filling it
/// takes a while; then new keys wait until old windows end.
const MAX_KEYS: usize = 100_000;

impl Throttle {
    /// Reserve an attempt for `key`, or `Err` with the remaining wait while
    /// it is locked out (or the attempts left are all in flight).
    pub fn begin(&self, key: &str) -> Result<Attempt<'_>, AppError> {
        self.begin_with(key, MAX_FAILURES)
    }

    fn begin_with(&self, key: &str, max_failures: u32) -> Result<Attempt<'_>, AppError> {
        if self.attempts.len() >= MAX_KEYS && !self.attempts.contains_key(key) {
            self.purge();
            if self.attempts.len() >= MAX_KEYS {
                // Full of counts that lock nobody out yet (one address trying
                // that many usernames, say): they go and the lockouts stay,
                // rather than everyone waiting for the window to end.
                self.attempts.retain(|_, a| a.in_flight > 0 || a.failures >= MAX_FAILURES);
            }
            if self.attempts.len() >= MAX_KEYS {
                return Err(AppError::TooManyRequests("too many attempts, try again later".into()));
            }
        }
        let mut entry = self.attempts.entry(key.to_owned()).or_insert(Attempts {
            failures: 0,
            in_flight: 0,
            since: Instant::now(),
        });
        if entry.since.elapsed() >= WINDOW {
            entry.failures = 0;
            entry.since = Instant::now();
        }
        if entry.failures + entry.in_flight >= max_failures {
            let minutes = WINDOW.saturating_sub(entry.since.elapsed()).as_secs().div_ceil(60).max(1);
            return Err(AppError::TooManyRequests(format!("too many attempts, try again in {minutes} min")));
        }
        entry.in_flight += 1;
        Ok(Attempt { throttle: self, key: key.to_owned(), settled: false })
    }

    /// Reserve a password attempt for the account `username` from `source`:
    /// 5 failures a window from each address, [`MAX_ACCOUNT_FAILURES`] from
    /// all of them.
    pub fn begin_password(&self, username: &str, source: Option<IpAddr>) -> Result<CountedAttempt<'_>, AppError> {
        let account = username.to_lowercase();
        self.begin_counted(&format!("login:{account}"), source, MAX_ACCOUNT_FAILURES)
    }

    /// Reserve a guess of the setup code from `source`: 5 failures a window
    /// from each address, [`MAX_SETUP_FAILURES`] from all of them. Behind a
    /// proxy each client counts on its own, and someone on the local network
    /// naming other addresses still gets few guesses.
    pub fn begin_setup_code(&self, source: Option<IpAddr>) -> Result<CountedAttempt<'_>, AppError> {
        self.begin_counted("setup-code", source, MAX_SETUP_FAILURES)
    }

    fn begin_counted(
        &self,
        key: &str,
        source: Option<IpAddr>,
        max_overall: u32,
    ) -> Result<CountedAttempt<'_>, AppError> {
        let source = source.map(|ip| ip.to_canonical().to_string()).unwrap_or_default();
        let here = self.begin(&format!("{key}@{source}"))?;
        let overall = self.begin_with(key, max_overall)?;
        Ok(CountedAttempt { here, overall })
    }

    /// Forget windows that ended: keys nobody uses again would stay forever.
    pub fn purge(&self) {
        self.attempts.retain(|_, a| a.in_flight > 0 || a.since.elapsed() < WINDOW);
    }

    fn settle(&self, key: &str, outcome: Outcome) {
        let Some(mut entry) = self.attempts.get_mut(key) else { return };
        entry.in_flight = entry.in_flight.saturating_sub(1);
        match outcome {
            Outcome::Failed => entry.failures += 1,
            Outcome::Succeeded => entry.failures = 0,
            Outcome::Abandoned => {}
        }
        let idle = entry.in_flight == 0 && entry.failures == 0;
        drop(entry);
        if idle {
            self.attempts.remove_if(key, |_, a| a.in_flight == 0 && a.failures == 0);
        }
    }
}

enum Outcome {
    Failed,
    Succeeded,
    Abandoned,
}

/// A reserved attempt: [`Attempt::failed`] counts it, [`Attempt::succeeded`]
/// clears the failures. Dropping it unsettled (a request refused before any
/// secret was checked) just gives the reservation back.
pub struct Attempt<'a> {
    throttle: &'a Throttle,
    key: String,
    settled: bool,
}

impl Attempt<'_> {
    pub fn failed(mut self) {
        self.settled = true;
        self.throttle.settle(&self.key, Outcome::Failed);
    }

    pub fn succeeded(mut self) {
        self.settled = true;
        self.throttle.settle(&self.key, Outcome::Succeeded);
    }
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.throttle.settle(&self.key, Outcome::Abandoned);
        }
    }
}

/// An attempt counted for its address and for all addresses together
/// ([`Throttle::begin_password`], [`Throttle::begin_setup_code`]).
pub struct CountedAttempt<'a> {
    here: Attempt<'a>,
    overall: Attempt<'a>,
}

impl CountedAttempt<'_> {
    pub fn failed(self) {
        self.here.failed();
        self.overall.failed();
    }

    pub fn succeeded(self) {
        self.here.succeeded();
        self.overall.succeeded();
    }
}

// ------------------------------------------------------------------ devices

/// "Chrome on Windows", "Safari on iPhone", or the raw agent for other clients.
pub fn device_name(user_agent: Option<&str>) -> String {
    let Some(ua) = user_agent.filter(|u| !u.is_empty()) else { return "Unknown device".into() };
    // The Android app, its web view and its player all say pleyAndroid/.
    if ua.contains("pleyAndroid/") {
        return "Pley for Android".into();
    }
    // The Apple apps say "pleyApple/0.1.0 (iPadOS 16.7; iPad)": one app
    // for iPhone, iPad and Mac and one for Apple TV, told apart by the
    // system they give first.
    if ua.contains("pleyApple/") {
        let system = bracketed(ua).and_then(|inside| inside.split([';', ' ']).next());
        return match system {
            Some("iOS") => "Pley for iPhone",
            Some("iPadOS") => "Pley for iPad",
            Some("tvOS") => "Pley for Apple TV",
            Some("macOS") => "Pley for Mac",
            _ => "Pley for Apple",
        }
        .into();
    }
    if ua.contains("pleyKodi/") {
        return "Pley for Kodi".into();
    }
    if ua.contains("pleyRoku/") {
        return "Pley for Roku".into();
    }
    if ua.contains("pleyLG/") {
        return "Pley for LG TV".into();
    }
    // Samsung's TV app, and LG's before it was a native app, are web apps,
    // which can't set their own agent: the TV's web engine names the TV.
    if ua.contains("Tizen") && ua.contains("TV") {
        return "Samsung TV".into();
    }
    if ua.contains("Web0S") || ua.contains("webOS") {
        return "LG TV".into();
    }
    let browser = if ua.contains("Edg/") {
        Some("Edge")
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        Some("Opera")
    } else if ua.contains("Firefox/") {
        Some("Firefox")
    } else if ua.contains("Chrome/") || ua.contains("CriOS/") {
        Some("Chrome")
    } else if ua.contains("Safari/") {
        Some("Safari")
    } else {
        None
    };
    let os = if ua.contains("iPhone") {
        Some("iPhone")
    } else if ua.contains("iPad") {
        Some("iPad")
    } else if ua.contains("Android") {
        Some("Android")
    } else if ua.contains("Windows") {
        Some("Windows")
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        Some("macOS")
    } else if ua.contains("CrOS") {
        Some("ChromeOS")
    } else if ua.contains("Linux") {
        Some("Linux")
    } else {
        None
    };
    match (browser, os) {
        (Some(b), Some(o)) => format!("{b} on {o}"),
        (Some(b), None) => b.to_owned(),
        _ => ua.split_whitespace().next().unwrap_or(ua).chars().take(60).collect(),
    }
}

/// What a User-Agent gives in its first brackets, trimmed: Pley's apps put
/// their platform there, "pleyKodi/0.1.0 (Kodi 21.2; Windows)".
pub(crate) fn bracketed(user_agent: &str) -> Option<&str> {
    user_agent
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inside, _)| inside.trim())
        .filter(|inside| !inside.is_empty())
}

// ------------------------------------------------------------------ avatars

pub fn avatars_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("avatars")
}

/// Center-crop and scale an uploaded picture to a square JPEG.
pub fn process_avatar(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let img = crate::images::decode_avatar(bytes)
        .map_err(|_| AppError::bad_request("that file is not a supported picture, or is over 4096 pixels"))?;
    let side = img.width().min(img.height());
    let (x, y) = ((img.width() - side) / 2, (img.height() - side) / 2);
    // Scaled from a view of the square, not a copy of it.
    let square = image::imageops::crop_imm(&img, x, y, side, side);
    let resized = image::imageops::resize(&*square, AVATAR_SIZE, AVATAR_SIZE, image::imageops::FilterType::Lanczos3);
    let resized = image::DynamicImage::ImageRgba8(resized).to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 88)
        .encode_image(&resized)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("encoding avatar: {e}")))?;
    Ok(out)
}

// ------------------------------------------------------------------ invites

/// Short, unambiguous invitation code (no 0/O, 1/l/I).
pub fn invite_code() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 12];
    getrandom::fill(&mut bytes).expect("the system's random numbers");
    bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_names() {
        let chrome_win = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36";
        assert_eq!(device_name(Some(chrome_win)), "Chrome on Windows");
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/140.0 Safari/537.36 Edg/140.0";
        assert_eq!(device_name(Some(edge)), "Edge on Windows");
        let iphone = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1";
        assert_eq!(device_name(Some(iphone)), "Safari on iPhone");
        assert_eq!(device_name(Some("python-requests/2.32")), "python-requests/2.32");
        assert_eq!(device_name(None), "Unknown device");
        let web_view = "Mozilla/5.0 (Linux; Android 16; Pixel 9; wv) AppleWebKit/537.36 Chrome/140.0 Mobile Safari/537.36 pleyAndroid/0.1.0";
        assert_eq!(device_name(Some(web_view)), "Pley for Android");
        assert_eq!(device_name(Some("pleyAndroid/0.1.0 (Android 16)")), "Pley for Android");
        assert_eq!(device_name(Some("pleyKodi/0.1.0 (Kodi 21.2; Windows)")), "Pley for Kodi");
        assert_eq!(device_name(Some("pleyRoku/0.1.0 (Roku OS 14.5; Roku Ultra)")), "Pley for Roku");
        assert_eq!(device_name(Some("pleyApple/0.1.0 (iOS 18.1; iPhone)")), "Pley for iPhone");
        assert_eq!(device_name(Some("pleyApple/0.1.0 (iPadOS 16.7; iPad)")), "Pley for iPad");
        assert_eq!(device_name(Some("pleyApple/0.1.0 (tvOS 18.2; Apple TV)")), "Pley for Apple TV");
        assert_eq!(device_name(Some("pleyApple/0.1.0 (macOS 15.2; Mac)")), "Pley for Mac");
        assert_eq!(device_name(Some("pleyApple/0.1.0")), "Pley for Apple");
        assert_eq!(device_name(Some("pleyApple/0.1.0 (visionOS 2.0; Apple Vision Pro)")), "Pley for Apple");
        // AVPlayer's own requests are not the app's.
        let core_media = "AppleCoreMedia/1.0.0.22A (iPhone; U; CPU OS 18_0 like Mac OS X; en_us)";
        assert_eq!(device_name(Some(core_media)), "AppleCoreMedia/1.0.0.22A");
        assert_eq!(device_name(Some("pleyLG/0.1.0 (webOS 23; OLED65C3PUA)")), "Pley for LG TV");
        let tizen = "Mozilla/5.0 (SMART-TV; LINUX; Tizen 6.0) AppleWebKit/537.36 (KHTML, like Gecko) 76.0.3809.146/6.0 TV Safari/537.36";
        assert_eq!(device_name(Some(tizen)), "Samsung TV");
        let webos = "Mozilla/5.0 (Web0S; Linux/SmartTV) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/94.0.4606.128 Safari/537.36 WebAppManager";
        assert_eq!(device_name(Some(webos)), "LG TV");
    }

    #[test]
    fn a_full_table_keeps_its_lockouts_and_lets_others_in() {
        let t = Throttle::default();
        for _ in 0..MAX_FAILURES {
            t.begin("login:owner").expect("allowed").failed();
        }
        // One address tries as many usernames as the table holds, once each.
        for i in 1..MAX_KEYS {
            t.begin(&format!("login:name{i}")).expect("allowed").failed();
        }
        assert_eq!(t.attempts.len(), MAX_KEYS);
        // Someone else still signs in, and the lockout stays.
        assert!(t.begin("login:someone").is_ok(), "counts that lock nobody out made room");
        assert!(t.begin("login:owner").is_err(), "the lockout stays");
    }

    #[test]
    fn throttle_locks_after_repeated_failures() {
        let t = Throttle::default();
        for _ in 0..MAX_FAILURES {
            t.begin("pin:7").expect("allowed").failed();
        }
        assert!(t.begin("pin:7").is_err());
        assert!(t.begin("pin:8").is_ok());
        // Failures within the limit are forgotten after a success.
        let t = Throttle::default();
        for _ in 1..MAX_FAILURES {
            t.begin("pin:7").expect("allowed").failed();
        }
        t.begin("pin:7").expect("allowed").succeeded();
        assert!(t.attempts.is_empty(), "nothing left to track");
        assert!(t.begin("pin:7").is_ok());
    }

    #[test]
    fn concurrent_attempts_count_against_the_limit() {
        let t = Throttle::default();
        // Attempts being checked at the same time hold their places...
        let in_flight: Vec<_> = (0..MAX_FAILURES).map(|_| t.begin("pin:9").expect("allowed")).collect();
        assert!(matches!(t.begin("pin:9").map(|_| ()), Err(AppError::TooManyRequests(_))));
        // ...and once they all fail, the profile is locked out.
        for attempt in in_flight {
            attempt.failed();
        }
        assert!(t.begin("pin:9").is_err());
    }

    #[test]
    fn unsettled_attempts_give_their_place_back() {
        let t = Throttle::default();
        for _ in 0..MAX_FAILURES * 2 {
            drop(t.begin("login:amy").expect("allowed"));
        }
        assert!(t.begin("login:amy").is_ok());
        assert!(t.attempts.get("login:amy").is_none_or(|a| a.failures == 0));
    }

    #[test]
    fn someone_guessing_elsewhere_does_not_lock_the_owner_out() {
        let t = Throttle::default();
        let guesser = Some(IpAddr::from([203, 0, 113, 7]));
        let owner = Some(IpAddr::from([192, 168, 1, 20]));
        for _ in 0..MAX_FAILURES {
            t.begin_password("Amy", guesser).expect("allowed").failed();
        }
        assert!(t.begin_password("amy", guesser).is_err(), "locked out from that address");
        t.begin_password("amy", owner).expect("the owner still signs in").succeeded();
        // Many addresses together still get few guesses.
        let t = Throttle::default();
        for n in 0..MAX_ACCOUNT_FAILURES {
            let source = Some(IpAddr::from([10, 0, (n / 250) as u8, (n % 250) as u8]));
            t.begin_password("amy", source).expect("allowed").failed();
        }
        assert!(t.begin_password("amy", owner).is_err());
    }

    #[test]
    fn ended_windows_are_forgotten() {
        let t = Throttle::default();
        t.begin("login:amy").expect("allowed").failed();
        t.attempts.get_mut("login:amy").expect("counted").since = Instant::now() - WINDOW;
        t.begin("login:bob").expect("allowed").failed();
        t.purge();
        assert!(t.attempts.get("login:amy").is_none());
        assert!(t.attempts.get("login:bob").is_some());
    }

    #[test]
    fn pins_colors_and_codes() {
        assert!(validate_pin("1234").is_ok());
        assert!(validate_pin("12a4").is_err());
        assert!(validate_pin("123").is_err());
        assert!(validate_color("#38bdf8").is_ok());
        assert!(validate_color("red").is_err());
        let code = invite_code();
        assert_eq!(code.len(), 12);
        assert!(!code.contains(['0', 'o', '1', 'l', 'i']));
    }

    #[test]
    fn avatars_are_square_jpegs() {
        let img = image::RgbImage::from_pixel(400, 200, image::Rgb([56, 189, 248]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let jpeg = process_avatar(&png).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (AVATAR_SIZE, AVATAR_SIZE));
        assert!(process_avatar(b"not an image").is_err());
    }

    #[test]
    fn more_device_names() {
        for (agent, name) in [
            ("Mozilla/5.0 (X11; Linux x86_64; rv:130.0) Gecko/20100101 Firefox/130.0", "Firefox on Linux"),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/120.0 Safari/537.36 OPR/106.0",
                "Opera on Windows",
            ),
            (
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/120.0 Mobile Safari/537.36",
                "Chrome on Android",
            ),
            (
                "Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 Version/17.5 Mobile/15E148 Safari/604.1",
                "Safari on iPad",
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 CriOS/126.0 Mobile/15E148 Safari/604.1",
                "Chrome on iPhone",
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 Version/17.5 Safari/605.1.15",
                "Safari on macOS",
            ),
            (
                "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 Chrome/120.0 Safari/537.36",
                "Chrome on ChromeOS",
            ),
            ("Something Firefox/1.0", "Firefox"),
            // A known system but no known browser: the client's own name.
            ("Kodi/20.2 (Windows NT 10.0; Win64; x64) App_Bitness/64 Version/20.2", "Kodi/20.2"),
            ("", "Unknown device"),
        ] {
            assert_eq!(device_name(Some(agent)), name, "{agent}");
        }
        assert_eq!(device_name(Some("x".repeat(100).as_str())).len(), 60, "raw agents are cut short");
    }

    #[test]
    fn pin_and_color_rules() {
        for pin in ["1234", "12345678", "0000"] {
            assert!(validate_pin(pin).is_ok(), "{pin}");
        }
        for pin in ["", "123", "123456789", " 1234", "12 34", "１２３４", "-123"] {
            assert!(validate_pin(pin).is_err(), "{pin:?}");
        }
        for color in ["#abc", "#ABCDEF", "#38bdf8"] {
            assert!(validate_color(color).is_ok(), "{color}");
        }
        for color in ["38bdf8", "#38bdf", "#ggg", "#", "", "#38bdf8ff"] {
            assert!(validate_color(color).is_err(), "{color:?}");
        }
    }

    #[test]
    fn throttle_forgets_failures_once_the_window_has_passed() {
        // `Instant` can't go back further than the machine's uptime.
        let Some(long_ago) = Instant::now().checked_sub(WINDOW + Duration::from_secs(1)) else { return };
        let t = Throttle::default();
        t.attempts.insert("login:bob".into(), Attempts { failures: MAX_FAILURES, in_flight: 0, since: long_ago });
        // The lockout is over, and the next failure starts a new count
        // instead of adding to the old one.
        t.begin("login:bob").expect("the lockout is over").failed();
        for _ in 1..MAX_FAILURES {
            t.begin("login:bob").expect("allowed").failed();
        }
        match t.begin("login:bob").map(|_| ()) {
            Err(AppError::TooManyRequests(message)) => assert!(message.contains("15 min"), "{message}"),
            other => panic!("expected a lockout, got {other:?}"),
        }
    }

    fn user_row(is_admin: bool, display_name: Option<&str>) -> UserRow {
        UserRow {
            id: 7,
            username: "kid".into(),
            display_name: display_name.map(str::to_owned),
            is_admin,
            managed: false,
            home: true,
            has_pin: false,
            avatar: Some("7-1734567890.jpg".into()),
            avatar_color: None,
            all_libraries: false,
            max_age: Some(12),
            disabled: false,
            created_at: 0,
            audio_language: None,
            subtitle_language: Some("fre".into()),
            subtitle_mode: "foreign".into(),
            has_password: true,
            two_step: false,
            request_access: "off".into(),
        }
    }

    #[test]
    fn users_as_the_api_shows_them() {
        let dto = user_row(false, Some("  ")).into_dto(vec![3]);
        assert_eq!(dto.name, "kid", "a blank display name shows the username");
        assert_eq!((dto.all_libraries, dto.max_age, dto.library_ids), (false, Some(12), vec![3]));
        assert_eq!(dto.avatar_url.as_deref(), Some("/api/users/7/avatar?v=1734567890"));
        assert_eq!(user_row(false, Some("Kiddo")).into_dto(vec![]).name, "Kiddo");
        assert_eq!(dto.request_access, RequestAccess::Off);
        // Administrators are never restricted, whatever is stored.
        let admin = user_row(true, None).into_dto(vec![]);
        assert_eq!((admin.all_libraries, admin.max_age), (true, None));
        assert_eq!(admin.request_access, RequestAccess::Auto, "administrators' requests are approved as made");
        assert_eq!(avatar_url(1, None), None);
    }
}
