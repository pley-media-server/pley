//! The time of day at the top of the player and in "Ends at": the C library's local time, which
//! follows the TV's time zone (glibc reads it from `TZ` or `/etc/localtime`). Nothing else in the
//! app knows the time zone yet, so the player asks the C library itself.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since 1970, now, with their fraction (for the next minute's change).
pub fn unix_now_f64() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// The hour and minute of `unix_secs` in the TV's time zone (UTC when it can't be read).
pub fn local_hm(unix_secs: i64) -> (u32, u32) {
    local(unix_secs).unwrap_or_else(|| {
        let s = unix_secs.rem_euclid(86_400);
        ((s / 3600) as u32, ((s % 3600) / 60) as u32)
    })
}

/// "21:05" or "9:05 PM" for `unix_secs`, as the TV is set to write times.
pub fn time_of_day(unix_secs: i64, hours24: bool) -> String {
    let (h, m) = local_hm(unix_secs);
    pley_core::format::format_time_of_day(h, m, hours24)
}

/// Seconds until the next minute starts, for the clock's next change.
pub fn to_next_minute(unix_secs_f: f64) -> f64 {
    60.0 - unix_secs_f.rem_euclid(60.0)
}

#[cfg(unix)]
fn local(unix_secs: i64) -> Option<(u32, u32)> {
    use std::os::raw::{c_char, c_int, c_long};

    // glibc's (and macOS's) `struct tm`: nine ints, then the offset and the zone's name.
    #[repr(C)]
    struct Tm {
        fields: [c_int; 9],
        gmtoff: c_long,
        zone: *const c_char,
    }
    unsafe extern "C" {
        fn localtime_r(time: *const c_long, out: *mut Tm) -> *mut Tm;
    }
    let t = c_long::try_from(unix_secs).ok()?;
    let mut tm = Tm { fields: [0; 9], gmtoff: 0, zone: std::ptr::null() };
    // SAFETY: both pointers are to live values of the C types the function takes; it writes
    // only into `tm` and is the thread-safe variant.
    let ok = unsafe { !localtime_r(&t, &mut tm).is_null() };
    ok.then(|| (tm.fields[2].clamp(0, 23) as u32, tm.fields[1].clamp(0, 59) as u32))
}

#[cfg(windows)]
fn local(unix_secs: i64) -> Option<(u32, u32)> {
    use std::os::raw::c_int;

    #[repr(C)]
    struct Tm {
        fields: [c_int; 9],
    }
    unsafe extern "C" {
        fn _localtime64_s(out: *mut Tm, time: *const i64) -> c_int;
    }
    let mut tm = Tm { fields: [0; 9] };
    // SAFETY: both pointers are to live values of the C types the function takes; it writes only
    // into `tm`.
    let ok = unsafe { _localtime64_s(&mut tm, &unix_secs) } == 0;
    ok.then(|| (tm.fields[2].clamp(0, 23) as u32, tm.fields[1].clamp(0, 59) as u32))
}

#[cfg(not(any(unix, windows)))]
fn local(_unix_secs: i64) -> Option<(u32, u32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_time_of_day() {
        let (h, m) = local_hm(unix_now_f64() as i64);
        assert!(h < 24 && m < 60);
        // Whatever the zone, minutes past the hour agree with UTC's for whole-hour zones, and a
        // minute later is a minute later.
        let (_, m1) = local_hm(1_700_000_000);
        let (_, m2) = local_hm(1_700_000_060);
        assert_eq!((m1 + 1) % 60, m2);
        assert!(time_of_day(1_700_000_000, true).contains(':'));
        assert!((to_next_minute(120.25) - 59.75).abs() < 1e-9);
    }
}
