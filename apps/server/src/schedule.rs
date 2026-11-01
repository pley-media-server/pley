//! When library work runs: intro and credits detection, and seek previews
//! of whole libraries. What plays, titles someone opens and analyses an
//! administrator asks for don't wait for it.
//!
//! Library work waits while anything plays, and a minute after
//! ([`QUIET_BEFORE_BACKGROUND_SECS`]), and while other programs keep the
//! processors busy ([`Settings::library_work_max_cpu`]). Then, by default
//! ([`LibraryWork::Auto`]), what someone is likely to watch soon goes
//! first, at any hour: shows someone watched lately, titles on a watchlist
//! or begun, new episodes of shows someone watches, the newest movies. The
//! rest of the libraries waits for the hours people rarely watch, learned
//! from when something played: each hour of the week, the last weeks
//! counting most (each week 0.8 of the one after it). An hour is quiet
//! where something played in less than a tenth of its minutes, and each
//! day's four quietest hours in a row always are, so that a server watched
//! all day still gets its libraries done. Work begins where this hour and
//! the next are quiet, so that it doesn't run into a busy one. Until about
//! a week is known (play history counts), any hour is. Where the rest has
//! waited for quiet hours through a day of the server being up (a computer
//! on only in the evening), it goes on whenever nothing plays. Where this
//! hour and the next are rarely watched, not only quiet by that rule,
//! seek previews go several files at once ([`Scheduler::wide`]).

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering::Relaxed};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::playback::{QUIET_BEFORE_BACKGROUND_SECS, Sessions};
use crate::resources::Monitor;
use crate::settings::{LibraryWork, Settings, SettingsStore};
use crate::util::now;

pub const HOURS: usize = 7 * 24;
const WEEK_SECS: i64 = 7 * 24 * 3600;
/// Each week counts this much of the one after it.
const DECAY: f64 = 0.8;
/// Under this share of its minutes with something playing, an hour is quiet.
const QUIET_SHARE: f64 = 0.1;
/// Each day's quietest hours in a row are quiet, however busy.
const QUIET_PER_DAY: usize = 4;
/// The minutes of an hour of the week seen (as they count now) that tell
/// how busy it is.
const MIN_SEEN: f64 = 30.0;
/// The minutes seen in all (as they count now) after which the hours are
/// known: about a week.
const LEARNED: f64 = 0.75 * (WEEK_SECS / 60) as f64;
/// How far back play history tells what wasn't seen.
const HISTORY_SECS: i64 = 8 * WEEK_SECS;
/// The minutes the server was up while the rest of the libraries waited for
/// quiet hours after which it goes on whenever nothing plays: the hours
/// learned may be ones the server is never up at (a computer on only in the
/// evening, its play history saying nothing of when it is off).
const OVERDUE_MINUTES: u32 = 24 * 60;
/// Other programs' use of the processors is averaged over this.
const CPU_WINDOW: Duration = Duration::from_secs(60);
/// What someone is likely to watch soon is looked up again after this.
const SOON_FOR: Duration = Duration::from_secs(600);
/// A look again while library work waits for the hours or the processors
/// (sooner when a session starts or ends).
pub const LOOK: Duration = Duration::from_secs(60);
/// Where the hours learned are kept, in the `settings` table.
const KEY: &str = "watching_hours";

/// Library work, by how soon its result is likely to be wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// For what someone is likely to watch soon.
    Soon,
    /// The rest of the libraries.
    Later,
}

/// Why library work waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Wait {
    /// Something plays, or did less than a minute ago.
    Playing,
    /// Other programs keep the processors busy.
    Busy,
    /// These hours aren't library work's: people watch then, or they are
    /// outside the hours set.
    Hours,
}

/// What [`decide`] goes by, from the settings.
#[derive(Debug, Clone, Copy)]
pub struct Rules {
    pub mode: LibraryWork,
    pub from: u8,
    pub to: u8,
    pub max_cpu: u8,
}

impl From<&Settings> for Rules {
    fn from(s: &Settings) -> Self {
        Rules {
            mode: s.library_work,
            from: s.library_work_from,
            to: s.library_work_to,
            max_cpu: s.library_work_max_cpu,
        }
    }
}

/// What [`decide`] looks at.
#[derive(Debug, Clone, Copy)]
pub struct Looks {
    /// Seconds since anything played.
    pub quiet_for: i64,
    /// Other programs' share of the processors lately, in percent.
    pub others_cpu: Option<f32>,
    /// The hour of the week ([`hour_of_week`]).
    pub hour: usize,
    /// The rest of the libraries waited [`OVERDUE_MINUTES`] for quiet hours.
    pub overdue: bool,
}

/// Whether library work of `class` may begin now, with the hours learned
/// quiet (`None` until known); why it waits otherwise.
pub fn decide(class: Class, rules: Rules, quiet: Option<&[bool; HOURS]>, looks: Looks) -> Result<(), Wait> {
    if looks.quiet_for < QUIET_BEFORE_BACKGROUND_SECS {
        return Err(Wait::Playing);
    }
    if rules.max_cpu < 100 && looks.others_cpu.is_some_and(|cpu| cpu > f32::from(rules.max_cpu)) {
        return Err(Wait::Busy);
    }
    let open = match rules.mode {
        LibraryWork::Idle => true,
        LibraryWork::Hours => in_hours(looks.hour % 24, rules.from, rules.to),
        LibraryWork::Auto => class == Class::Soon || looks.overdue || quiet_at(quiet, looks.hour),
    };
    if open { Ok(()) } else { Err(Wait::Hours) }
}

/// Whether the hours learned let the rest of the libraries begin at `hour`
/// of the week: it and the next are quiet, or none are known yet.
fn quiet_at(quiet: Option<&[bool; HOURS]>, hour: usize) -> bool {
    quiet.is_none_or(|q| q[hour % HOURS] && q[(hour + 1) % HOURS])
}

/// Whether `hour` of the day is from `from` to `to`: past midnight where
/// `to` comes first, all day where they are the same.
fn in_hours(hour: usize, from: u8, to: u8) -> bool {
    let (from, to) = (usize::from(from), usize::from(to));
    if from < to { (from..to).contains(&hour) } else { from == to || hour >= from || hour < to }
}

/// The hour of the week at `unix` seconds, `offset` seconds from UTC: 0 is
/// Sunday from midnight.
pub fn hour_of_week(unix: i64, offset: i64) -> usize {
    let local = unix + offset;
    // 1 January 1970 was a Thursday.
    let weekday = (local.div_euclid(86_400) + 4).rem_euclid(7);
    (weekday * 24 + local.rem_euclid(86_400) / 3600) as usize
}

fn week_of(unix: i64) -> i64 {
    unix.div_euclid(WEEK_SECS)
}

/// One hour of the week as seen: minutes, each week counting [`DECAY`] of
/// the one after it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
struct Hour {
    /// The minutes the server saw, and of them those something played in.
    seen: f64,
    playing: f64,
    /// The week they count as of ([`week_of`]).
    week: i64,
}

impl Hour {
    /// As it counts in `week`.
    fn at(self, week: i64) -> Hour {
        let keep = DECAY.powi((week - self.week).clamp(0, 520) as i32);
        Hour { seen: self.seen * keep, playing: self.playing * keep, week: week.max(self.week) }
    }
}

/// When people watch: each hour of the week, learned.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    hours: Vec<Hour>,
    /// The minutes the server was up since the hours last let the rest of
    /// the libraries begin ([`OVERDUE_MINUTES`]).
    #[serde(default)]
    waited: u32,
}

impl Default for Profile {
    fn default() -> Self {
        Self { hours: vec![Hour::default(); HOURS], waited: 0 }
    }
}

impl Profile {
    /// `seen` minutes of `hour` of the week in `week`, something playing in
    /// `playing` of them.
    fn add(&mut self, hour: usize, week: i64, seen: f64, playing: f64) {
        let slot = &mut self.hours[hour % HOURS];
        let mut now = slot.at(week);
        now.seen += seen;
        now.playing += playing.min(seen);
        *slot = now;
    }

    /// A minute of `hour` of the week in `week` seen, something `playing`
    /// in it or not; and whether the hours let the rest of the libraries
    /// begin then.
    fn minute(&mut self, hour: usize, week: i64, playing: bool) {
        self.add(hour, week, 1.0, if playing { 1.0 } else { 0.0 });
        let open = quiet_at(self.quiet(week).as_ref(), hour);
        self.waited = if open { 0 } else { self.waited.saturating_add(1) };
    }

    /// The share of `hour`'s minutes something played in, as it counts in
    /// `week`, where seen enough to tell.
    fn share(&self, hour: usize, week: i64) -> Option<f64> {
        let h = self.hours[hour % HOURS].at(week);
        (h.seen >= MIN_SEEN).then(|| h.playing / h.seen)
    }

    /// Whether about a week is known as of `week`.
    fn learned(&self, week: i64) -> bool {
        self.hours.iter().map(|h| h.at(week).seen).sum::<f64>() >= LEARNED
    }

    /// The quiet hours of the week (see the module's documentation), as
    /// they count in `week`; `None` before about a week is known.
    fn quiet(&self, week: i64) -> Option<[bool; HOURS]> {
        if !self.learned(week) {
            return None;
        }
        let shares: Vec<Option<f64>> = (0..HOURS).map(|h| self.share(h, week)).collect();
        // Hours not seen enough (the server was off) are quiet as far as is known.
        let mut quiet = shares.iter().map(|share| share.is_none_or(|s| s < QUIET_SHARE)).collect::<Vec<_>>();
        // Each day's least watched hours in a row, from one that day on, among
        // those the server was up for: library work can't run in the others.
        let run = |start: usize| (start..start + QUIET_PER_DAY).map(|h| shares[h % HOURS]).sum::<Option<f64>>();
        for day in 0..7 {
            let runs = (day * 24..day * 24 + 24).filter_map(|start| Some((start, run(start)?)));
            if let Some((start, _)) = runs.min_by(|a, b| a.1.total_cmp(&b.1)) {
                for h in start..start + QUIET_PER_DAY {
                    quiet[h % HOURS] = true;
                }
            }
        }
        quiet.try_into().ok()
    }

    /// Whether `hour` of the week and the next had something playing in
    /// less than a tenth of their minutes, as they count in `week` (an hour
    /// not seen enough counts as such), or the hours aren't known yet. Each
    /// day's quietest hours, library work's even where people watch then,
    /// don't count: a playback that starts puts off every file being made.
    fn wide_at(&self, hour: usize, week: i64) -> bool {
        let rare = |hour: usize| self.share(hour, week).is_none_or(|share| share < QUIET_SHARE);
        !self.learned(week) || (rare(hour) && rare(hour + 1))
    }
}

/// What play history (`(viewed_at, length)` in seconds) tells of the
/// [`HISTORY_SECS`] before `now`: every hour from the first play on seen,
/// with the minutes something played in. A play is recorded as it reaches
/// 90 %: it began 0.9 of its length before.
fn from_history(plays: &[(i64, i64)], now: i64, offset: i64) -> Profile {
    let mut profile = Profile::default();
    let span = |&(at, length): &(i64, i64)| (at - length.max(0) * 9 / 10, at + length.max(0) / 10);
    let since = now - HISTORY_SECS;
    let Some(first) = plays.iter().map(span).map(|(begin, _)| begin.max(since)).min() else { return profile };
    let start = first.div_euclid(3600) * 3600;
    let hours = ((now - start) / 3600).max(0) as usize;
    let mut minutes = vec![0.0f64; hours];
    for (begin, end) in plays.iter().map(span) {
        let (begin, end) = (begin.max(start), end.min(start + hours as i64 * 3600));
        let mut t = begin;
        while t < end {
            let hour_end = (t.div_euclid(3600) + 1) * 3600;
            minutes[((t - start) / 3600) as usize] += (hour_end.min(end) - t) as f64 / 60.0;
            t = hour_end;
        }
    }
    for (i, playing) in minutes.into_iter().enumerate() {
        let t = start + i as i64 * 3600;
        profile.add(hour_of_week(t, offset), week_of(t), 60.0, playing.min(60.0));
    }
    profile
}

/// What someone is likely to watch soon: shows (seasons, for intros and
/// credits) and titles (files, for seek previews).
#[derive(Debug, Default)]
pub struct Soon {
    pub seasons: HashSet<i64>,
    pub files: HashSet<i64>,
}

/// Shows and movies likely to be watched soon (`?1`: played after, `?2`:
/// added after, in Unix seconds): shows someone played an episode of,
/// movies someone began and didn't finish, titles on a watchlist, shows
/// with new episodes that someone watched before, and the 20 newest movies.
macro_rules! soon_titles {
    () => {
        "WITH soon(id) AS (
            SELECT e.grandparent_id FROM user_item_state u JOIN items e ON e.id = u.item_id
             WHERE e.kind = 'episode' AND u.last_played_at > ?1
            UNION
            SELECT u.item_id FROM user_item_state u JOIN items m ON m.id = u.item_id
             WHERE m.kind = 'movie' AND u.position_ms > 0 AND u.watched = 0 AND u.last_played_at > ?1
            UNION
            SELECT item_id FROM watchlist
            UNION
            SELECT e.grandparent_id FROM items e
             WHERE e.kind = 'episode' AND e.added_at > ?2 AND EXISTS (
                 SELECT 1 FROM items w JOIN user_item_state u ON u.item_id = w.id
                  WHERE w.grandparent_id = e.grandparent_id AND (u.watched = 1 OR u.position_ms > 0))
            UNION
            SELECT id FROM (SELECT id FROM items WHERE kind = 'movie' AND added_at > ?2 ORDER BY added_at DESC LIMIT 20)
        ) "
    };
}

/// Library work's state, for the activity view (see [`Scheduler::status`]).
#[derive(Debug, Clone, Serialize)]
pub struct LibraryWorkStatus {
    pub mode: LibraryWork,
    /// Why the work on the rest of the libraries waits now, if it does.
    pub waiting: Option<Wait>,
    /// When the hours it waits for begin, in Unix seconds.
    pub resumes_at: Option<i64>,
    /// Whether the hours people watch are known yet.
    pub learned: bool,
}

/// One hour of the week, for the settings page.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct HourView {
    /// The share of its minutes something played in, 0 to 1; `None` where
    /// it wasn't seen enough to tell.
    pub playing: Option<f64>,
    /// Work on the rest of the libraries may begin then.
    pub open: bool,
}

/// The week as learned, from Sunday midnight in the server's time.
#[derive(Debug, Clone, Serialize)]
pub struct WatchingHours {
    #[serde(flatten)]
    pub status: LibraryWorkStatus,
    /// The hour of the week now.
    pub now: usize,
    pub hours: Vec<HourView>,
}

pub struct Scheduler {
    db: Db,
    settings: Arc<SettingsStore>,
    sessions: Arc<Sessions>,
    monitor: Arc<Monitor>,
    profile: Mutex<Profile>,
    /// The server's time's offset from UTC, in seconds (SQLite's
    /// `localtime`, which follows the system's time zone and `TZ`).
    offset: AtomicI64,
    /// What someone is likely to watch soon, and when it was looked up.
    soon: tokio::sync::Mutex<Option<(Instant, Arc<Soon>)>>,
}

impl Scheduler {
    pub fn new(db: Db, settings: Arc<SettingsStore>, sessions: Arc<Sessions>, monitor: Arc<Monitor>) -> Arc<Self> {
        Arc::new(Self {
            db,
            settings,
            sessions,
            monitor,
            profile: Mutex::default(),
            offset: AtomicI64::new(0),
            soon: tokio::sync::Mutex::new(None),
        })
    }

    /// Learns until `stopping`: each minute, whether something plays; the
    /// hours learned are saved every hour, and as the server stops.
    pub fn start(self: &Arc<Self>, stopping: CancellationToken) {
        let this = self.clone();
        tokio::spawn(async move {
            this.find_offset().await;
            this.load().await;
            let minute = Duration::from_secs(60);
            let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + minute, minute);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            for minute in 1u64.. {
                tokio::select! {
                    () = stopping.cancelled() => {
                        this.save().await;
                        break;
                    }
                    _ = tick.tick() => {}
                }
                let now = now();
                let playing = !this.sessions.active().is_empty();
                this.profile.lock().minute(hour_of_week(now, this.offset.load(Relaxed)), week_of(now), playing);
                if minute % 10 == 0 {
                    // Daylight saving time.
                    this.find_offset().await;
                }
                if minute % 60 == 0 {
                    this.save().await;
                }
            }
        });
    }

    async fn find_offset(&self) {
        let offset: sqlx::Result<i64> = sqlx::query_scalar(
            "SELECT CAST(strftime('%s', 'now', 'localtime') AS INTEGER) - CAST(strftime('%s', 'now') AS INTEGER)",
        )
        .fetch_one(&self.db.read)
        .await;
        if let Ok(offset) = offset.map(|o| o.clamp(-14 * 3600, 14 * 3600)) {
            self.offset.store(offset, Relaxed);
        }
    }

    /// The hours saved, or else what play history tells.
    async fn load(&self) {
        let saved: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(KEY)
            .fetch_optional(&self.db.read)
            .await
            .ok()
            .flatten();
        let saved =
            saved.and_then(|json| serde_json::from_str::<Profile>(&json).ok()).filter(|p| p.hours.len() == HOURS);
        let profile = match saved {
            Some(profile) => profile,
            None => {
                let now = now();
                let plays: Vec<(i64, i64)> = sqlx::query_as(
                    "SELECT h.viewed_at, COALESCE(i.duration_ms, 0) / 1000 FROM play_history h
                     JOIN items i ON i.id = h.item_id WHERE h.viewed_at > ?",
                )
                .bind(now - HISTORY_SECS)
                .fetch_all(&self.db.read)
                .await
                .unwrap_or_default();
                if !plays.is_empty() {
                    tracing::info!("library work: when people watch, from {} plays of the last 8 weeks", plays.len());
                }
                from_history(&plays, now, self.offset.load(Relaxed))
            }
        };
        *self.profile.lock() = profile;
    }

    async fn save(&self) {
        let Ok(json) = serde_json::to_string(&*self.profile.lock()) else { return };
        let saved = sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(KEY)
        .bind(json)
        .execute(&self.db.write)
        .await;
        if let Err(err) = saved {
            tracing::warn!("saving when people watch: {err}");
        }
    }

    /// The quiet hours as they count at `now`, and what [`decide`] looks at.
    fn looks(&self, now: i64) -> (Option<[bool; HOURS]>, Looks) {
        let (quiet, waited) = {
            let profile = self.profile.lock();
            (profile.quiet(week_of(now)), profile.waited)
        };
        let looks = Looks {
            quiet_for: self.sessions.quiet_for(),
            others_cpu: self.monitor.others_cpu(CPU_WINDOW),
            hour: hour_of_week(now, self.offset.load(Relaxed)),
            overdue: waited >= OVERDUE_MINUTES,
        };
        (quiet, looks)
    }

    /// Whether library work of `class` may begin now (see [`decide`]).
    pub fn turn(&self, class: Class) -> Result<(), Wait> {
        let (quiet, looks) = self.looks(now());
        decide(class, Rules::from(&*self.settings.get()), quiet.as_ref(), looks)
    }

    /// Whether this hour and the next are rarely watched (see
    /// [`Profile::wide_at`]): seek previews of the libraries may go several
    /// files at once.
    pub fn wide(&self) -> bool {
        let now = now();
        self.profile.lock().wide_at(hour_of_week(now, self.offset.load(Relaxed)), week_of(now))
    }

    /// What someone is likely to watch soon, looked up at most every
    /// [`SOON_FOR`].
    pub async fn soon(&self) -> Arc<Soon> {
        let mut cached = self.soon.lock().await;
        if let Some((at, soon)) = cached.as_ref()
            && at.elapsed() < SOON_FOR
        {
            return soon.clone();
        }
        let soon = Arc::new(self.look_up_soon().await.unwrap_or_else(|err| {
            tracing::warn!("library work: what may be watched soon: {err}");
            Soon::default()
        }));
        *cached = Some((Instant::now(), soon.clone()));
        soon
    }

    async fn look_up_soon(&self) -> sqlx::Result<Soon> {
        let now = now();
        let (played, added) = (now - 30 * 24 * 3600, now - 14 * 24 * 3600);
        let seasons: Vec<i64> = sqlx::query_scalar(concat!(
            soon_titles!(),
            "SELECT id FROM items WHERE kind = 'season' AND parent_id IN soon"
        ))
        .bind(played)
        .bind(added)
        .fetch_all(&self.db.read)
        .await?;
        let files: Vec<i64> = sqlx::query_scalar(concat!(
            soon_titles!(),
            "SELECT id FROM media_files WHERE item_id IN soon
             UNION
             SELECT f.id FROM items e JOIN media_files f ON f.item_id = e.id WHERE e.grandparent_id IN soon"
        ))
        .bind(played)
        .bind(added)
        .fetch_all(&self.db.read)
        .await?;
        Ok(Soon { seasons: seasons.into_iter().collect(), files: files.into_iter().collect() })
    }

    /// Where library work stands: whether the rest of the libraries waits,
    /// and for which hours.
    pub fn status(&self) -> LibraryWorkStatus {
        let now = now();
        let settings = self.settings.get();
        let rules = Rules::from(&*settings);
        let (quiet, looks) = self.looks(now);
        let waiting = decide(Class::Later, rules, quiet.as_ref(), looks).err();
        let offset = self.offset.load(Relaxed);
        // The next hour whose work may begin, were nothing playing.
        let resumes_at = (waiting == Some(Wait::Hours))
            .then(|| {
                (1..=HOURS as i64).map(|k| (now.div_euclid(3600) + k) * 3600).find(|&t| {
                    let looks =
                        Looks { quiet_for: i64::MAX, others_cpu: None, hour: hour_of_week(t, offset), overdue: false };
                    decide(Class::Later, rules, quiet.as_ref(), looks).is_ok()
                })
            })
            .flatten();
        LibraryWorkStatus { mode: settings.library_work, waiting, resumes_at, learned: quiet.is_some() }
    }

    /// The week as learned, and the hours library work may begin in.
    pub fn watching_hours(&self) -> WatchingHours {
        let now = now();
        let week = week_of(now);
        let rules = Rules::from(&*self.settings.get());
        let (playing, quiet) = {
            let profile = self.profile.lock();
            ((0..HOURS).map(|h| profile.share(h, week)).collect::<Vec<_>>(), profile.quiet(week))
        };
        let hours = playing
            .into_iter()
            .enumerate()
            .map(|(hour, playing)| {
                let looks = Looks { quiet_for: i64::MAX, others_cpu: None, hour, overdue: false };
                HourView { playing, open: decide(Class::Later, rules, quiet.as_ref(), looks).is_ok() }
            })
            .collect();
        WatchingHours { status: self.status(), now: hour_of_week(now, self.offset.load(Relaxed)), hours }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTO: Rules = Rules { mode: LibraryWork::Auto, from: 1, to: 7, max_cpu: 50 };

    fn looks(hour: usize) -> Looks {
        Looks { quiet_for: 3600, others_cpu: Some(5.0), hour, overdue: false }
    }

    #[test]
    fn the_hour_of_the_week_is_the_servers() {
        // Thursday 1 January 1970, midnight UTC.
        assert_eq!(hour_of_week(0, 0), 4 * 24);
        // Sunday 4 January 1970 at 21:30 UTC is Monday 0:30 three hours east.
        let sunday = 3 * 86_400 + 21 * 3600 + 1800;
        assert_eq!(hour_of_week(sunday, 0), 21);
        assert_eq!(hour_of_week(sunday, 3 * 3600), 24);
        // Saturday's last hour, from Sunday 0:30 one hour west.
        assert_eq!(hour_of_week(sunday - 21 * 3600, -3600), HOURS - 1);
    }

    #[test]
    fn set_hours_may_go_past_midnight() {
        assert!(in_hours(1, 1, 7) && in_hours(6, 1, 7) && !in_hours(7, 1, 7) && !in_hours(0, 1, 7));
        assert!(in_hours(23, 22, 6) && in_hours(2, 22, 6) && !in_hours(12, 22, 6));
        assert!((0..24).all(|h| in_hours(h, 3, 3)), "all day");
    }

    #[test]
    fn library_work_waits_for_playback_a_busy_computer_and_its_hours() {
        let mut quiet = [false; HOURS];
        quiet[2..6].fill(true);
        let quiet = Some(&quiet);
        // Something played less than a minute ago: none.
        let playing = Looks { quiet_for: 10, ..looks(3) };
        assert_eq!(decide(Class::Soon, AUTO, quiet, playing), Err(Wait::Playing));
        // Other programs keep the processors busy: none, unless allowed.
        let busy = Looks { others_cpu: Some(80.0), ..looks(3) };
        assert_eq!(decide(Class::Soon, AUTO, quiet, busy), Err(Wait::Busy));
        assert_eq!(decide(Class::Soon, Rules { max_cpu: 100, ..AUTO }, quiet, busy), Ok(()));
        // What may be watched soon at any hour; the rest where this hour and the next are quiet.
        assert_eq!(decide(Class::Soon, AUTO, quiet, looks(20)), Ok(()));
        assert_eq!(decide(Class::Later, AUTO, quiet, looks(20)), Err(Wait::Hours));
        assert_eq!(decide(Class::Later, AUTO, quiet, looks(3)), Ok(()));
        assert_eq!(decide(Class::Later, AUTO, quiet, looks(5)), Err(Wait::Hours), "the next hour is busy");
        // Before the hours are known: any.
        assert_eq!(decide(Class::Later, AUTO, None, looks(20)), Ok(()));
        // Set hours, for both; whenever nothing plays.
        let hours = Rules { mode: LibraryWork::Hours, from: 22, to: 6, ..AUTO };
        assert_eq!(decide(Class::Soon, hours, quiet, looks(20)), Err(Wait::Hours));
        assert_eq!(decide(Class::Later, hours, quiet, looks(23)), Ok(()));
        let idle = Rules { mode: LibraryWork::Idle, ..AUTO };
        assert_eq!(decide(Class::Later, idle, quiet, looks(20)), Ok(()));
        assert_eq!(decide(Class::Later, idle, quiet, playing), Err(Wait::Playing));
        // Waited a day for quiet hours: whenever nothing plays, but set hours are kept.
        let overdue = Looks { overdue: true, ..looks(20) };
        assert_eq!(decide(Class::Later, AUTO, quiet, overdue), Ok(()));
        assert_eq!(decide(Class::Later, AUTO, quiet, Looks { quiet_for: 10, ..overdue }), Err(Wait::Playing));
        assert_eq!(decide(Class::Later, hours, quiet, overdue), Err(Wait::Hours));
    }

    #[test]
    fn a_computer_on_only_in_the_evening_gets_its_libraries_done_then() {
        // On from 18:00 to midnight for five weeks, something playing half the time.
        let mut profile = Profile::default();
        for week in 0..5 {
            for hour in (0..HOURS).filter(|h| h % 24 >= 18) {
                profile.add(hour, week, 60.0, 30.0);
            }
        }
        let quiet = profile.quiet(4).unwrap();
        for day in 0..7 {
            let today: Vec<usize> = (18..24).filter(|h| quiet[day * 24 + h]).collect();
            assert_eq!(today, vec![18, 19, 20, 21], "day {day}: its least watched hours on");
        }
        assert!(quiet[3], "an hour it is off at says nothing against it");
    }

    #[test]
    fn library_work_that_waited_a_day_for_quiet_hours_goes_on_when_nothing_plays() {
        let mut profile = evenings(3);
        let evening = 3 * 24 + 21;
        for _ in 0..OVERDUE_MINUTES {
            profile.minute(evening, 2, true);
        }
        assert_eq!(profile.waited, OVERDUE_MINUTES);
        // A quiet hour comes: the hours it waits for again.
        profile.minute(3 * 24 + 3, 2, false);
        assert_eq!(profile.waited, 0);
        // Kept with the hours.
        let saved: Profile = serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
        assert_eq!(saved, profile);
        let older: Profile = serde_json::from_str(r#"{"hours":[]}"#).unwrap();
        assert_eq!(older.waited, 0);
    }

    /// `weeks` of evenings watched, 20:00 to 23:00 every day.
    fn evenings(weeks: i64) -> Profile {
        let mut profile = Profile::default();
        for week in 0..weeks {
            for hour in 0..HOURS {
                let watched = (20..23).contains(&(hour % 24));
                profile.add(hour, week, 60.0, if watched { 50.0 } else { 0.0 });
            }
        }
        profile
    }

    #[test]
    fn the_hours_people_rarely_watch_are_quiet() {
        let profile = evenings(3);
        assert!((profile.share(20, 2).unwrap() - 50.0 / 60.0).abs() < 1e-9);
        let quiet = profile.quiet(2).unwrap();
        assert!(!quiet[20] && !quiet[22] && quiet[23] && quiet[3] && quiet[13]);
        // A week less than half seen isn't known yet.
        let mut young = Profile::default();
        (0..HOURS / 3).for_each(|h| young.add(h, 0, 60.0, 0.0));
        assert_eq!(young.quiet(0), None);
        // Not seen for years: unknown again.
        assert_eq!(profile.quiet(200), None);
    }

    #[test]
    fn wide_hours_are_this_and_the_next_rarely_watched() {
        let profile = evenings(3);
        assert!(profile.wide_at(3, 2) && profile.wide_at(13, 2) && profile.wide_at(23, 2));
        assert!(!profile.wide_at(19, 2), "the next hour is watched");
        assert!(!profile.wide_at(20, 2) && !profile.wide_at(22, 2));
        assert!(Profile::default().wide_at(20, 0), "before the hours are known, any");
    }

    #[test]
    fn a_server_watched_all_day_keeps_four_quiet_hours_in_a_row_each_day() {
        let mut profile = Profile::default();
        for week in 0..3 {
            for hour in 0..HOURS {
                // Busy at every hour, least from 3:00 to 7:00.
                let playing = if (3..7).contains(&(hour % 24)) { 15.0 } else { 40.0 };
                profile.add(hour, week, 60.0, playing);
            }
        }
        let quiet = profile.quiet(2).unwrap();
        for day in 0..7 {
            let today: Vec<usize> = (0..24).filter(|h| quiet[day * 24 + h]).collect();
            assert_eq!(today, vec![3, 4, 5, 6], "day {day}");
        }
        // Library work's, but watched a quarter of the time: one file at a time.
        assert!((0..HOURS).all(|hour| !profile.wide_at(hour, 2)));
    }

    #[test]
    fn recent_weeks_count_most() {
        // Evenings for three weeks, then afternoons for three.
        let mut profile = evenings(3);
        for week in 3..6 {
            for hour in 0..HOURS {
                let watched = (14..17).contains(&(hour % 24));
                profile.add(hour, week, 60.0, if watched { 50.0 } else { 0.0 });
            }
        }
        // As much watched, but three weeks later: 1 + 0.8 + 0.64 against 0.8³ + 0.8⁴ + 0.8⁵.
        let (evening, afternoon) = (profile.share(21, 5).unwrap(), profile.share(15, 5).unwrap());
        assert!(afternoon > 1.9 * evening, "{afternoon} vs {evening}");
        let quiet = profile.quiet(5).unwrap();
        assert!(!quiet[15] && !quiet[21], "both still watched more than a tenth of the time");
    }

    #[test]
    fn play_history_tells_when_people_watched() {
        // A two-hour film recorded at 90 % on Wednesday 7 January 1970,
        // 21:48 UTC: it played from 20:00 to 22:00.
        let at = 6 * 86_400 + 21 * 3600 + 48 * 60;
        let film = (at, 2 * 3600);
        let now = at + 3 * 3600;
        let profile = from_history(&[film], now, 0);
        let wednesday = 3 * 24;
        let minutes = |hour: usize| profile.hours[wednesday + hour].playing;
        assert_eq!((minutes(19), minutes(20), minutes(21), minutes(22)), (0.0, 60.0, 60.0, 0.0));
        assert_eq!(profile.hours[wednesday + 23].seen, 60.0, "seen since the first play");
        assert_eq!(profile.hours[wednesday + 19].seen, 0.0, "not before it");
        assert_eq!(from_history(&[], now, 0), Profile::default());
        // Plays before the last 8 weeks don't count.
        let old = from_history(&[film], now + 9 * WEEK_SECS, 0);
        assert!(old.hours.iter().all(|h| h.playing == 0.0));
    }
}
