//! The TV's clock as Live TV writes it: times in its 12- or 24-hour format, days ("Today",
//! "Tomorrow", "Fri 30 Oct") in its own time zone. Times are unix seconds; the zone's offset comes
//! from the C library (`localtime_r` on the TV, whose `/etc/localtime` follows the TV's setting),
//! as the web app's `Date` took it from the browser. Each time is written with the offset it has, so a
//! change to or from summer time inside the guide's two days moves the times after it.

use std::cell::Cell;

use pley_core::model::LiveProgram;
use pley_player::live::data::{GUIDE_SPAN, format_guide_day, format_time, now_seconds};

/// How far back and ahead of now the zone's changes are looked for: the guide's start (an hour back at
/// most) and its two days.
const ZONE_BACK: i64 = 6 * 3600;
const ZONE_AHEAD: i64 = GUIDE_SPAN + 6 * 3600;

/// How Live TV reads and writes the time now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    /// Unix seconds now.
    pub now: i64,
    /// The TV's offset from UTC now, in seconds.
    pub offset: i64,
    /// "20:00" rather than "8:00 PM".
    pub hours24: bool,
    /// A change of the offset before now within the guide's reach: when, and the offset before it.
    before: Option<(i64, i64)>,
    /// A change of the offset after now within the guide's reach: when, and the offset from then on.
    after: Option<(i64, i64)>,
}

impl Clock {
    /// A clock at `now` in a zone `offset` seconds from UTC, all year.
    pub fn new(now: i64, offset: i64, hours24: bool) -> Clock {
        Clock { now, offset, hours24, before: None, after: None }
    }

    /// The clock now, in the TV's zone.
    pub fn read(hours24: bool) -> Clock {
        let now = now_seconds();
        let zone = Zone::around(now);
        Clock { now, offset: utc_offset(now), hours24, before: zone.before, after: zone.after }
    }

    /// The TV's offset from UTC at `t`.
    pub fn offset_at(&self, t: i64) -> i64 {
        match (self.before, self.after) {
            (Some((at, before)), _) if t < at => before,
            (_, Some((at, after))) if t >= at => after,
            _ => self.offset,
        }
    }

    /// "20:00" or "8:00 PM".
    pub fn time(&self, t: i64) -> String {
        format_time(t, self.hours24, self.offset_at(t))
    }

    /// "20:00 – 21:00".
    pub fn span(&self, p: &LiveProgram) -> String {
        format!("{} – {}", self.time(p.start), self.time(p.stop))
    }

    /// "Today", "Tomorrow", "Yesterday" or "Fri 30 Oct".
    pub fn day(&self, t: i64) -> String {
        format_guide_day(t + self.offset_at(t), self.now + self.offset, 0)
    }

    /// Whether `t` is midnight on the TV: the guide's ruler names the day there.
    pub fn midnight(&self, t: i64) -> bool {
        (t + self.offset_at(t)).rem_euclid(86_400) == 0
    }
}

/// The zone's changes of offset around a time, found again once a minute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Zone {
    minute: i64,
    before: Option<(i64, i64)>,
    after: Option<(i64, i64)>,
}

thread_local! {
    static ZONE: Cell<Option<Zone>> = const { Cell::new(None) };
}

impl Zone {
    /// The changes around `now`: the C library is asked a few times a minute at most, not per time
    /// written.
    fn around(now: i64) -> Zone {
        let minute = now.div_euclid(60);
        if let Some(z) = ZONE.get().filter(|z| z.minute == minute) {
            return z;
        }
        let offset = utc_offset(now);
        let (back, ahead) = (now - ZONE_BACK, now + ZONE_AHEAD);
        let early = utc_offset(back);
        let late = utc_offset(ahead);
        let zone = Zone {
            minute,
            before: (early != offset).then(|| (change_between(back, now), early)),
            after: (late != offset).then(|| (change_between(now, ahead), late)),
        };
        ZONE.set(Some(zone));
        zone
    }
}

/// The first second from which the offset is `utc_offset(to)`'s, `from` having another.
fn change_between(mut from: i64, mut to: i64) -> i64 {
    let last = utc_offset(to);
    while to - from > 1 {
        let mid = from + (to - from) / 2;
        if utc_offset(mid) == last {
            to = mid;
        } else {
            from = mid;
        }
    }
    to
}

/// The TV's offset from UTC at `t` (unix seconds), in seconds; 0 when the system can't say.
pub fn utc_offset(t: i64) -> i64 {
    sys::offset(t).filter(|o| o.abs() <= 18 * 3600).unwrap_or(0)
}

#[cfg(unix)]
mod sys {
    use std::ffi::{c_char, c_int, c_long};
    use std::mem::MaybeUninit;

    /// `struct tm` as glibc and the BSDs lay it out: nine ints, then the offset and the zone's name.
    #[repr(C)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
        gmtoff: c_long,
        zone: *const c_char,
    }

    unsafe extern "C" {
        // `time_t` is a `long` on the TV's glibc and on 64-bit hosts.
        fn localtime_r(t: *const c_long, out: *mut Tm) -> *mut Tm;
    }

    /// The zone's offset from UTC at `t`, in seconds.
    // A `long` is 32 bits on the TV and 64 on a 64-bit host, where the conversion does nothing.
    #[allow(clippy::useless_conversion)]
    pub fn offset(t: i64) -> Option<i64> {
        let t = c_long::try_from(t).ok()?;
        let mut tm = MaybeUninit::<Tm>::zeroed();
        // SAFETY: `localtime_r` reads `t` and fills the `tm` it is given, which outlives the call.
        let done = unsafe { localtime_r(&t, tm.as_mut_ptr()) };
        if done.is_null() {
            return None;
        }
        // SAFETY: it was zeroed (a valid `Tm`) and then filled.
        let tm = unsafe { tm.assume_init() };
        Some(i64::from(tm.gmtoff))
    }
}

#[cfg(windows)]
mod sys {
    use std::ffi::c_int;

    /// The C runtime's `struct tm`.
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        sec: c_int,
        min: c_int,
        hour: c_int,
        mday: c_int,
        mon: c_int,
        year: c_int,
        wday: c_int,
        yday: c_int,
        isdst: c_int,
    }

    unsafe extern "C" {
        fn _localtime64_s(out: *mut Tm, t: *const i64) -> c_int;
        fn _mkgmtime64(tm: *mut Tm) -> i64;
    }

    /// The zone's offset from UTC at `t`, in seconds.
    pub fn offset(t: i64) -> Option<i64> {
        let mut tm = Tm::default();
        // SAFETY: both read and write only the values they are given, which outlive the calls.
        if unsafe { _localtime64_s(&mut tm, &t) } != 0 {
            return None;
        }
        // The local time read back as if it were UTC is the offset ahead of `t`.
        // SAFETY: as above.
        let local = unsafe { _mkgmtime64(&mut tm) };
        (local != -1).then_some(local - t)
    }
}

#[cfg(not(any(unix, windows)))]
mod sys {
    /// The zone's offset from UTC at `t`, in seconds.
    pub fn offset(_: i64) -> Option<i64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_offset_is_a_whole_quarter_hour_within_a_day() {
        let now = now_seconds();
        let offset = utc_offset(now);
        assert_eq!(offset % 900, 0, "{offset}");
        assert!(offset.abs() <= 14 * 3600);
    }

    #[test]
    fn times_and_days_follow_the_offset() {
        // 2026-10-30 19:30 UTC, a Friday; the TV an hour ahead.
        let t = 1_793_388_600;
        let c = Clock::new(t, 3600, true);
        assert_eq!(c.time(t), "20:30");
        assert_eq!(c.day(t), "Today");
        assert_eq!(c.day(t + 86_400), "Tomorrow");
        assert_eq!(c.day(t + 2 * 86_400), "Sun 1 Nov");
        let twelve = Clock::new(t, 3600, false);
        assert_eq!(twelve.time(t), "8:30 PM");
        // Midnight on the TV is 23:00 UTC.
        assert!(c.midnight(1_793_401_200));
        assert!(!c.midnight(t));
    }

    #[test]
    fn a_change_of_summer_time_inside_the_guide_moves_the_times_after_it() {
        // Saturday 2026-10-31 22:00 UTC, the TV in New York: summer time (−4 h) ends on Sunday at
        // 06:00 UTC, 2:00 local, when the clock goes back to 1:00 (−5 h).
        let now = 1_793_484_000;
        let change = 1_793_512_800;
        let c = Clock { after: Some((change, -5 * 3600)), ..Clock::new(now, -4 * 3600, true) };
        assert_eq!(c.time(now), "18:00");
        assert_eq!(c.time(change - 1800), "01:30");
        assert_eq!(c.time(change), "01:00");
        assert_eq!(
            c.span(&LiveProgram { start: change - 3600, stop: change + 3600, ..LiveProgram::default() }),
            "01:00 – 02:00"
        );
        // Midnight on Monday is 05:00 UTC, no longer 04:00.
        assert!(c.midnight(change + 23 * 3600));
        assert!(!c.midnight(change + 22 * 3600));
        assert_eq!(c.day(change + 23 * 3600), "Mon 2 Nov");
        assert_eq!(c.day(change + 23 * 3600 - 1), "Tomorrow");
    }

    #[test]
    fn a_change_is_found_to_the_second() {
        // The offset where the system says so, wherever this test runs.
        let now = now_seconds();
        let zone = Zone::around(now);
        for (at, _) in zone.before.into_iter().chain(zone.after) {
            assert_ne!(utc_offset(at - 1), utc_offset(at));
        }
        let c = Clock::read(true);
        assert_eq!(c.offset_at(now), utc_offset(now));
        assert_eq!(c.offset_at(now + ZONE_AHEAD - 1), utc_offset(now + ZONE_AHEAD - 1));
    }
}
