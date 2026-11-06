//! Moving subtitles in time, for files whose subtitles are early or late:
//! positive is later (a subtitle made for 10 s shows at 10.5 s with
//! +0.5 s). Steps of 100 ms, at most 10 s either way, remembered per profile
//! and file on this TV and never sent to the server (the Android app's
//! rules, `SubtitleSync.kt`; the TV app's `subs/sync.ts`).

/// One press of the sync row moves subtitles this much.
pub const SYNC_STEP_MS: i32 = 100;
/// Subtitles move at most this far either way.
pub const SYNC_MAX_MS: i32 = 10_000;

/// `ms` to the nearest step (halves round up, as the TV app's `Math.round`),
/// within ±10 s.
pub fn clamp_offset(ms: i64) -> i32 {
    let step = i64::from(SYNC_STEP_MS);
    let stepped = ms.saturating_add(step / 2).div_euclid(step).saturating_mul(step);
    stepped.clamp(-i64::from(SYNC_MAX_MS), i64::from(SYNC_MAX_MS)) as i32
}

/// The offset as the sync row shows it: `+0.5 s`, `−1.2 s` (a true minus
/// sign), `0.0 s`; `+0,5 s` in French and Spanish.
pub fn format_offset(ms: i64) -> String {
    let v = clamp_offset(ms);
    let sign = match v {
        1.. => "+",
        ..0 => "\u{2212}",
        0 => "",
    };
    // Whole steps of 100 ms: the tenths are exact.
    let tenths = v.unsigned_abs() / 100;
    format!("{sign}{} s", pley_i18n::decimal(f64::from(tenths) / 10.0, 1))
}

/// The offsets this TV remembers, by file and profile, oldest first, as the
/// player keeps them in its saved data (the TV app's `pley.subtitleOffsets`).
///
/// The key is the file's absolute address and the profile,
/// `<server>/api/files/<id>#u<profile>`: subtitles one person finds late may
/// suit another. An offset kept by file alone (`<server>/api/files/<id>`,
/// before offsets were per profile) goes to the first profile that plays the
/// file. Without a profile nothing is read or kept. The player saves
/// [`OffsetBook::entries`] whenever [`OffsetBook::take_changed`] says so.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OffsetBook {
    entries: Vec<(String, i32)>,
    changed: bool,
}

impl OffsetBook {
    /// How many files' offsets are kept: the oldest go first.
    pub const KEPT: usize = 300;

    /// The book as saved: `(key, ms)` pairs, oldest first.
    pub fn new(entries: Vec<(String, i32)>) -> OffsetBook {
        OffsetBook { entries, changed: false }
    }

    /// What to save, oldest first.
    pub fn entries(&self) -> &[(String, i32)] {
        &self.entries
    }

    /// Whether the entries changed since the last call: they need saving.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// The offset for `file_id` on `server` (its address without a trailing
    /// slash) for `profile`; 0 when none is kept.
    pub fn get(&mut self, server: &str, file_id: i64, profile: Option<i64>) -> i32 {
        let Some(profile) = profile else { return 0 };
        let file = format!("{server}/api/files/{file_id}");
        let key = format!("{file}#u{profile}");
        if let Some((_, v)) = self.entries.iter().find(|(k, _)| *k == key) {
            return clamp_offset(i64::from(*v));
        }
        let Some(before) = self.entries.iter().find(|(k, _)| *k == file).map(|(_, v)| *v) else { return 0 };
        // Kept before offsets were per profile: this profile's from now on.
        let moved = clamp_offset(i64::from(before));
        self.entries.retain(|(k, _)| *k != file);
        if moved != 0 {
            self.entries.push((key, moved));
        }
        self.keep_last();
        moved
    }

    /// Remembers `ms` for the file and the profile; 0 forgets it.
    pub fn set(&mut self, server: &str, file_id: i64, profile: Option<i64>, ms: i64) {
        let Some(profile) = profile else { return };
        let file = format!("{server}/api/files/{file_id}");
        let key = format!("{file}#u{profile}");
        self.entries.retain(|(k, _)| *k != key && *k != file);
        let value = clamp_offset(ms);
        if value != 0 {
            self.entries.push((key, value));
        }
        self.keep_last();
    }

    fn keep_last(&mut self) {
        let extra = self.entries.len().saturating_sub(Self::KEPT);
        self.entries.drain(..extra);
        self.changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAS: &str = "http://nas:7539";

    #[test]
    fn steps_by_100_ms_within_10_s() {
        assert_eq!(clamp_offset(149), 100);
        assert_eq!(clamp_offset(150), 200);
        assert_eq!(clamp_offset(-150), -100);
        assert_eq!(clamp_offset(-12_345), -10_000);
        assert_eq!(clamp_offset(10_049), 10_000);
        // The TypeScript test's NaN has no integer counterpart; -20 rounds to a plain 0.
        assert_eq!(clamp_offset(-20), 0);
        assert_eq!(clamp_offset(i64::MIN), -10_000);
    }

    #[test]
    fn writes_offsets_as_people_read_them() {
        assert_eq!(format_offset(500), "+0.5 s");
        assert_eq!(format_offset(-1200), "\u{2212}1.2 s");
        assert_eq!(format_offset(0), "0.0 s");
        assert_eq!(format_offset(-10_000), "\u{2212}10.0 s");
        pley_i18n::with_language(pley_i18n::Language::Fr, || {
            assert_eq!(format_offset(500), "+0,5 s");
            assert_eq!(format_offset(-10_000), "\u{2212}10,0 s");
        });
    }

    #[test]
    fn remembers_offsets_per_file_0_forgetting() {
        let mut book = OffsetBook::default();
        book.set(NAS, 7, Some(1), 1500);
        book.set(NAS, 8, Some(1), -300);
        assert_eq!(book.get(NAS, 7, Some(1)), 1500);
        assert_eq!(book.get("http://other:7539", 7, Some(1)), 0);
        book.set(NAS, 7, Some(1), 0);
        assert_eq!(book.get(NAS, 7, Some(1)), 0);
        assert_eq!(book.get(NAS, 8, Some(1)), -300);
        assert!(book.take_changed());
        assert!(!book.take_changed());
    }

    #[test]
    fn keeps_each_profiles_own_and_none_without_a_profile() {
        let mut book = OffsetBook::default();
        book.set(NAS, 7, Some(1), 2000);
        assert_eq!(book.get(NAS, 7, Some(2)), 0);
        book.set(NAS, 7, Some(2), -500);
        assert_eq!(book.get(NAS, 7, Some(2)), -500);
        assert_eq!(book.get(NAS, 7, Some(1)), 2000);
        assert_eq!(book.get(NAS, 7, None), 0);
        book.set(NAS, 8, None, 100);
        assert_eq!(book.entries().len(), 2);
    }

    #[test]
    fn gives_an_offset_kept_by_file_alone_to_the_first_profile_that_plays_the_file() {
        let mut book = OffsetBook::new(vec![(format!("{NAS}/api/files/7"), 1500), (format!("{NAS}/api/files/8"), 300)]);
        assert_eq!(book.get(NAS, 7, Some(2)), 1500);
        assert_eq!(book.get(NAS, 7, Some(1)), 0);
        assert_eq!(book.entries(), [(format!("{NAS}/api/files/8"), 300), (format!("{NAS}/api/files/7#u2"), 1500)]);
        assert!(book.take_changed());
    }

    #[test]
    fn keeps_the_last_300_files() {
        let mut book = OffsetBook::default();
        for id in 0..310 {
            book.set(NAS, id, Some(1), 100);
        }
        assert_eq!(book.entries().len(), OffsetBook::KEPT);
        assert_eq!(book.get(NAS, 9, Some(1)), 0);
        assert_eq!(book.get(NAS, 10, Some(1)), 100);
    }
}
