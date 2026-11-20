// SPDX-License-Identifier: AGPL-3.0-only
//! The automatic preset for software encodes (`auto`): x264's and x265's
//! presets trade encoding speed for picture quality at the same bitrate.
//!
//! A job takes the slowest preset its computer makes at twice real time or
//! more, by what jobs alike were measured to make ([`Speeds`]): a process
//! goes as fast as it can for its first two minutes of video, before it is
//! held to the read rate, and that speed is the computer's at its preset.
//! Presets not tried yet are reckoned from the nearest one measured
//! ([`STEP`]), and software encodes running together share what one makes
//! alone. At a start or a seek, where the player waits, a job takes a
//! preset that makes its first segment in 0.7 s ([`Need::Quick`]), and
//! moves to the slower one once it is well ahead.
//!
//! While it plays, a job about to keep its player waiting moves to a preset
//! fast enough, as many steps at once as that takes. One well ahead moves
//! as encodes come and go: to a faster preset when its share gets too
//! small, back to a slower one when they leave. Video its encoder doesn't
//! hold back (4K HDR tone mapped on the CPU), which no preset makes fast
//! enough, takes the slowest that goes about as fast as the others. Each
//! move restarts ffmpeg where it was, between two segments, so playback
//! never notices. What a preset measured is remembered: a job doesn't go
//! back to one that was too slow, and the next process (a seek, another
//! film at the same size) starts from what the last one learned.
//!
//! Before anything was measured, a job starts by its computer's processors
//! ([`target`]).

use std::time::{Duration, Instant};

/// Presets from the fastest to the slowest used.
pub const LADDER: [&str; 6] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium"];

/// The preset setting that asks for this.
pub const AUTO: &str = "auto";

/// A preset's speed beside the one a step faster (the one before it in
/// [`LADDER`]), on the low side of what was measured: x264 on 2 to 24
/// threads of a 12-core Ryzen, 1080p at 24 and 60 frames a second and 720p,
/// made 0.43 to 0.64 for superfast (0.57 to 0.64 at 24 frames a second,
/// under 0.51 at 60), 0.62 to 0.88 for veryfast, 0.60 to 0.87 for faster,
/// 0.77 to 0.95 for fast, 0.66 to 0.92 for medium. x265's steps are less
/// even (veryfast and faster go alike), within the same bounds. A preset
/// reckoned too slow is reached a step at a time, as each one is measured;
/// one reckoned too fast restarts the job twice. Video held back by
/// something else than its encoder (4K HDR tone mapped on the CPU) loses
/// less at each step.
const STEP: [f64; 6] = [1.0, 0.6, 0.7, 0.7, 0.78, 0.8];

/// The speed (× real time) a preset must be expected to make to be taken:
/// half of the computer's time then goes to the encode, which leaves room
/// for scenes that take longer, a seek, and a second video starting.
const COMFORT: f64 = 2.0;
/// The speed a job stays at a preset with, once there: one it moved to
/// that makes less goes back, and so does one whose share gets smaller
/// when more encodes start.
const STAY: f64 = 1.6;
/// A preset a job moved to proves too slow within this long of the move:
/// later, another encode starting beside it takes of its speed for a
/// moment.
const SETTLES_WITHIN: Duration = Duration::from_secs(120);
/// Under this speed, as fast as it can go and with little ready ahead, a
/// job is about to keep its player waiting.
const TOO_SLOW: f64 = 1.15;
/// The speed the preset it moves to then must be expected to make.
const RECOVER: f64 = 1.5;
/// How long a process's first segment may take to make at a start or a
/// seek, where the player waits for it (seconds).
const FIRST_SEGMENT_SECS: f64 = 0.7;
/// The frames x264 takes in before its first comes out, by preset
/// (`rc-lookahead`): a slower preset must go faster for its first segment
/// to come as soon. From a job's creation to its first 2 s segment took
/// 0.44 s at ultrafast (38 × real time) and 0.96 s at medium (7.8 ×) on 24
/// threads: 0.4 s whatever the preset (the probe, FFmpeg's start), and the
/// time to make those 2 s, half a second more and the frames looked ahead.
const LOOKAHEAD_FRAMES: [f64; 6] = [0.0, 0.0, 10.0, 20.0, 30.0, 40.0];

/// How fast a preset must be expected to go to be taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// To keep ahead while it plays ([`COMFORT`]).
    Steady,
    /// For the first segment to come soon ([`FIRST_SEGMENT_SECS`]): 3.6 ×
    /// real time at ultrafast and superfast, 4.2 at veryfast, 6 at medium.
    Quick,
    /// To get a job that falls behind ahead again ([`RECOVER`]).
    Recover,
}

impl Need {
    fn speed(self, preset: usize) -> f64 {
        match self {
            Need::Steady => COMFORT,
            Need::Quick => (2.5 + LOOKAHEAD_FRAMES[preset] / 24.0) / FIRST_SEGMENT_SECS,
            Need::Recover => RECOVER,
        }
    }
}

/// How far what a preset is known to make moves toward a measurement that
/// says more: a stretch of film takes a third more or less than the next,
/// and one easy stretch isn't a faster computer. One that says less counts
/// whole, at once: a preset is chosen by what it makes when film is hard.
const TOWARD: f64 = 1.0 / 3.0;
/// What a process makes beside other encodes depends on what they do
/// meanwhile (paused far ahead, or starting as fast as they go): between
/// all of what its preset makes alone and its share of that, give or take
/// (two films starting together made 0.8 of half each). Under this much of
/// its share, something else holds it back (the computer's other work),
/// and the preset makes less than was known.
const ITS_SHARE: f64 = 0.7;
/// The least a process shows its preset makes replaces what the preset was
/// measured to make alone only where it is this much more: the computer was
/// busier then. Less of a difference is one stretch of film beside another.
const MORE: f64 = 1.3;
/// A slower preset measured to make this much more than a faster one was
/// measured at another time, while the computer had less to do.
const NO_FASTER: f64 = 1.1;
/// A preset reckoned from one measured two steps away or more must be
/// expected to make this much more for each step past the first: the
/// further the reckoning, the more room it needs.
const REACH: f64 = 1.2;
/// Where no preset is fast enough, those expected within this share of the
/// fastest go as fast as it, and the slowest of them is taken: the picture
/// isn't given up for a speed that barely changes.
const SAME_SPEED: f64 = 0.85;
/// The speed a slower preset that goes about as fast must still make to be
/// taken for the picture: a little more than keeps a job from falling
/// behind ([`TOO_SLOW`]).
const KEEPS_AHEAD: f64 = 1.25;
/// Below this much video ready past what the player asked for (seconds), a
/// job that is too slow moves to a faster preset…
const SHORT_LEAD_SECS: f64 = 8.0;
/// …or when what is ready would be played through within this many seconds
/// at the speed it makes (under real time).
const EMPTY_WITHIN_SECS: f64 = 20.0;
/// Above it, a job moves to another preset when nothing presses.
const LONG_LEAD_SECS: f64 = 45.0;
/// The least time between two moves of a job: its speed settles first.
/// Less for a faster preset, which a player may be waiting for.
const SPEED_UP_AFTER: Duration = Duration::from_secs(4);
const SLOW_DOWN_AFTER: Duration = Duration::from_secs(10);
/// …and before a job held to the read rate tries a step slower.
const TRY_SLOWER_AFTER: Duration = Duration::from_secs(40);
/// How long a measurement tells what a preset makes, once no process runs
/// at it: the computer's other work changes, and so does what is played.
const FRESH: Duration = Duration::from_secs(10 * 60);

/// How fast a process goes at its preset (× real time).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Measured {
    /// As fast as the computer makes it: nothing else holds it back.
    Exactly(f64),
    /// Held to this speed by the read rate: it could go faster.
    AtLeast(f64),
}

impl Measured {
    pub fn speed(self) -> f64 {
        match self {
            Measured::Exactly(speed) | Measured::AtLeast(speed) => speed,
        }
    }
}

/// What a preset was measured to make.
#[derive(Debug, Clone, Copy)]
struct Seen {
    /// What it makes with no other software encode running (× real time)…
    alone: f64,
    /// …or only the least it does: a process the read rate held, or one
    /// beside other encodes, showed that much.
    at_least: bool,
    at: Instant,
}

impl Seen {
    fn fresh(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) < FRESH
    }
}

/// What the presets make alone for one kind of job (the same codecs, sizes,
/// frame rate and filters), as measured, by index in [`LADDER`].
#[derive(Debug, Clone, Default)]
pub struct Speeds {
    seen: [Option<Seen>; LADDER.len()],
}

impl Speeds {
    /// A process at `preset` went at `measured` with `jobs` software encodes
    /// running, itself included.
    pub fn learn(&mut self, preset: usize, measured: Measured, jobs: u32, now: Instant) {
        let Some(slot) = self.seen.get_mut(preset) else { return };
        let known = slot.filter(|old| old.fresh(now));
        let sharing = f64::from(jobs.max(1));
        let seen = match (measured, known) {
            // Alone, as fast as it goes: what the preset makes.
            (Measured::Exactly(speed), old) if jobs <= 1 => match old.filter(|old| !old.at_least) {
                // The same measurement again: a paused process keeps its last.
                Some(old) if now <= old.at => old,
                Some(old) if speed < old.alone => Seen { alone: speed, at_least: false, at: now },
                Some(old) => Seen { alone: old.alone + (speed - old.alone) * TOWARD, at_least: false, at: now },
                None => Seen { alone: speed, at_least: false, at: now },
            },
            // Beside others, far less than its share of what was known:
            // what all of them make of it now, at most.
            (Measured::Exactly(speed), Some(old)) if speed < old.alone / sharing * ITS_SHARE => {
                Seen { alone: speed * sharing, at_least: false, at: now }
            }
            // Else the least it makes alone, which is news only where it is
            // more than what was known. What was known still tells.
            (Measured::Exactly(speed) | Measured::AtLeast(speed), Some(old)) => {
                if speed > old.alone * if old.at_least { 1.0 } else { MORE } {
                    Seen { alone: speed, at_least: true, at: now }
                } else {
                    Seen { at: old.at.max(now), ..old }
                }
            }
            (Measured::Exactly(speed) | Measured::AtLeast(speed), None) => {
                Seen { alone: speed, at_least: true, at: now }
            }
        };
        if seen.alone.is_finite() && seen.alone > 0.0 {
            *slot = Some(seen);
        }
    }

    /// Whether anything measured still tells.
    pub fn any(&self, now: Instant) -> bool {
        self.seen.iter().flatten().any(|s| s.fresh(now))
    }

    /// What is known of `preset` itself.
    fn seen(&self, preset: usize, now: Instant) -> Option<&Seen> {
        self.seen.get(preset)?.as_ref().filter(|seen| seen.fresh(now))
    }

    /// What `preset` was measured to make alone.
    fn alone(&self, preset: usize, now: Instant) -> Option<f64> {
        self.seen(preset, now).filter(|seen| !seen.at_least).map(|seen| seen.alone)
    }

    /// The speed expected of `preset` with `jobs` software encodes running,
    /// each making its share of what the preset makes alone, and how many
    /// steps slower it is than the measurement it is reckoned from (0: its
    /// own, or a slower preset's). The nearest one counts, and of two as
    /// near the later. The least a preset is known to make counts where it
    /// says more.
    pub fn expect(&self, preset: usize, jobs: u32, now: Instant) -> Option<(f64, usize)> {
        let reckoned = |at_least: bool| {
            let (from, seen) = self
                .seen
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.as_ref().filter(|s| s.fresh(now) && s.at_least == at_least).map(|s| (i, s)))
                .min_by_key(|(i, s)| (i.abs_diff(preset), std::cmp::Reverse(s.at)))?;
            let (faster, slower) = (from.min(preset), from.max(preset));
            let steps: f64 = STEP[faster + 1..=slower].iter().product();
            let alone = seen.alone * if preset > from { steps } else { 1.0 / steps };
            Some((alone / f64::from(jobs.max(1)), preset.saturating_sub(from)))
        };
        match (reckoned(false), reckoned(true)) {
            (Some(known), Some(least)) => Some(if least.0 > known.0 { least } else { known }),
            (known, least) => known.or(least),
        }
    }

    /// The slowest preset expected to go as fast as `need` asks with `jobs`
    /// software encodes running; where none is, the slowest of the fastest
    /// ([`SAME_SPEED`]). `None` when nothing was measured.
    pub fn choose(&self, jobs: u32, need: Need, now: Instant) -> Option<usize> {
        let expected: Vec<(f64, usize)> =
            (0..LADDER.len()).map(|preset| self.expect(preset, jobs, now)).collect::<Option<_>>()?;
        // What a slower preset loses is the less sure the further it is
        // reckoned ([`REACH`]); a faster one makes more, however much.
        let enough = |(preset, &(speed, slower)): (usize, &(f64, usize))| {
            speed >= need.speed(preset) * REACH.powi(slower.saturating_sub(1) as i32)
        };
        if let Some(preset) = expected.iter().enumerate().rposition(enough) {
            return Some(preset);
        }
        // None is: under real time the fastest, above it the slowest that
        // goes about as fast and isn't too slow.
        let fastest = expected.iter().map(|e| e.0).fold(0.0, f64::max);
        let near = (fastest * SAME_SPEED).max(fastest.min(TOO_SLOW));
        expected.iter().rposition(|e| e.0 >= near)
    }
}

/// What a job's first preset depends on besides the job, before anything
/// was measured.
#[derive(Debug, Clone, Copy)]
pub struct Load {
    /// Software encodes running, this one included.
    pub software_jobs: u32,
    pub cores: u32,
    /// The output picture's pixels (1920 × 1080 for 1080p).
    pub pixels: u64,
    /// Its frames a second.
    pub fps: f64,
    /// HEVC (x265) takes a few times x264's work.
    pub hevc: bool,
}

/// The preset (index in [`LADDER`]) a job starts from before anything was
/// measured: by the processors each encode has, counted for a slow one (an
/// Atom's, where each makes a third of what a desktop's does), since a
/// start that is too slow keeps the player waiting and one too fast costs a
/// few seconds of a lesser picture.
pub fn target(load: Load) -> usize {
    let size = (load.pixels as f64 / (1920.0 * 1080.0)).max(0.25);
    // Films and television are 24 to 30 frames a second; 50 or 60 take
    // twice the work.
    let rate = (load.fps / 30.0).clamp(1.0, 4.0);
    let work = size * rate * if load.hevc { 3.0 } else { 1.0 };
    // Cores for each 1080p x264-sized encode.
    let cores = f64::from(load.cores.max(1)) / (f64::from(load.software_jobs.max(1)) * work);
    match cores {
        c if c >= 12.0 => 5,
        c if c >= 8.0 => 4,
        c if c >= 5.0 => 3,
        c if c >= 3.0 => 2,
        c if c >= 1.5 => 1,
        _ => 0,
    }
}

/// The preset a process starts with, `lead_secs` of video ready past what
/// the player asked for: fast enough for the player to wait little when
/// nothing is ready ahead (a start or a seek), for the encode to keep ahead
/// otherwise. Before anything was measured, the load's ([`target`]), a step
/// faster when nothing is ready ahead.
pub fn start(speeds: &Speeds, load: Load, lead_secs: f64, now: Instant) -> usize {
    let waiting = lead_secs <= 0.0;
    speeds.choose(load.software_jobs, if waiting { Need::Quick } else { Need::Steady }, now).unwrap_or_else(|| {
        let target = target(load);
        if waiting { target.saturating_sub(1) } else { target }
    })
}

/// How a job on the automatic preset goes: see [`decide`].
#[derive(Debug, Clone, Copy)]
pub struct Going {
    /// Its preset (index in [`LADDER`]).
    pub preset: usize,
    /// What its process makes now; `None` while it is paused, or too young
    /// to tell.
    pub speed: Option<Measured>,
    /// What it makes as fast as it goes, over all it has of its burst's
    /// last 20 s; `None` while the read rate holds it, or it is paused.
    pub made: Option<f64>,
    /// Seconds of video ready past what the player asked for.
    pub lead_secs: f64,
    /// Since its last start or move.
    pub since_move: Duration,
    /// Software encodes running, this one included.
    pub jobs: u32,
}

/// The preset a running job should move to now, if any, by what its kind
/// of job was measured to make (`speeds`, its own process's speed
/// included).
pub fn decide(speeds: &Speeds, going: Going, now: Instant) -> Option<usize> {
    let Going { preset, speed, made, lead_secs, since_move, jobs } = going;
    // About to keep its player waiting: faster, by as many steps as it takes.
    if let Some(Measured::Exactly(speed)) = speed {
        let short = SHORT_LEAD_SECS.max(EMPTY_WITHIN_SECS * (1.0 - speed));
        if speed < TOO_SLOW && lead_secs < short && preset > 0 && since_move >= SPEED_UP_AFTER {
            let fast_enough = speeds.choose(jobs, Need::Recover, now).unwrap_or(preset - 1);
            return Some(fast_enough.min(preset - 1));
        }
    }
    if lead_secs <= LONG_LEAD_SECS || since_move < SLOW_DOWN_AFTER {
        return None;
    }
    // Well ahead: the slowest preset that is fast enough…
    let steady = speeds.choose(jobs, Need::Steady, now);
    if let Some(slower) = steady.filter(|to| *to > preset) {
        return Some(slower);
    }
    // …a faster one where more encodes share the computer than its preset
    // leaves each enough of: each makes room for the others…
    if let Some(alone) = speeds.alone(preset, now)
        && alone >= STAY
        && alone / f64::from(jobs.max(1)) < STAY
    {
        return steady.filter(|to| *to < preset);
    }
    // …and back from one it moved to that proves too slow to stay at, to
    // the one it came from (the nearest measured), unless the faster
    // presets go no faster.
    let sharing = f64::from(jobs.max(1));
    let faster = || (0..preset).rev().filter_map(|faster| Some((faster, speeds.seen(faster, now)?.alone / sharing)));
    let fastest = faster()
        .map(|(_, speed)| speed)
        .fold(None, |most: Option<f64>, speed| Some(most.map_or(speed, |m| m.max(speed))));
    if since_move < SETTLES_WITHIN
        && let (Some((came_from, _)), Some(fastest)) = (faster().next(), fastest)
        && made.is_some_and(|made| made < STAY && made < fastest * SAME_SPEED)
    {
        return Some(came_from);
    }
    // Video its encoder doesn't hold back (4K HDR tone mapped on the CPU)
    // makes no preset fast enough, and loses little at a slower one: where
    // the job's preset goes about as fast as the fastest one measured, a
    // slower one measured to go as fast is taken for its picture
    // ([`NO_FASTER`]: not one measured to go faster), and the next one is
    // tried. It goes back if that one makes much less.
    let share = |preset: usize| speeds.alone(preset, now).map(|alone| alone / sharing);
    if let Some(own) = share(preset).filter(|own| *own < COMFORT) {
        let as_fast = (fastest.unwrap_or(own).max(own) * SAME_SPEED).max(KEEPS_AHEAD);
        let slower = (preset + 1..LADDER.len())
            .rev()
            .find(|slower| share(*slower).is_some_and(|it| it >= as_fast && it <= own * NO_FASTER));
        if slower.is_some() {
            return slower;
        }
        // With nothing faster measured to tell by, only a preset that keeps
        // the job well ahead is worth a try.
        let next = preset + 1;
        let worth = own >= as_fast && (fastest.is_some() || own >= STAY);
        if worth && next < LADDER.len() && speeds.seen(next, now).is_none() && since_move >= TRY_SLOWER_AFTER {
            return Some(next);
        }
    }
    // Held to the read rate, it can't show what it could make. Where its
    // preset was never measured alone and as fast as it went (a burst too
    // short to tell) and nothing is known of the next one, it tries that:
    // its first minutes, made as fast as it goes, tell.
    let held = matches!(speed, Some(Measured::AtLeast(_)));
    let next = preset + 1;
    let unknown = speeds.alone(preset, now).is_none() && next < LADDER.len() && speeds.seen(next, now).is_none();
    (held && since_move >= TRY_SLOWER_AFTER && unknown).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: u64 = 1920 * 1080;
    const UF: usize = 0;
    const SF: usize = 1;
    const VF: usize = 2;
    const FASTER: usize = 3;
    const FAST: usize = 4;
    const MEDIUM: usize = 5;

    fn load(software_jobs: u32, cores: u32, pixels: u64) -> Load {
        Load { software_jobs, cores, pixels, fps: 24.0, hevc: false }
    }

    /// A 1080p film on x264, as measured on 8, 4 and 2 threads of a Ryzen 9
    /// 5900X, and 4K HDR10 tone mapped on 8 and 4 of them, which the tone
    /// mapping holds back rather than the encoder.
    const FILM_8: [f64; 6] = [15.58, 9.52, 7.29, 5.2, 4.1, 3.22];
    const FILM_4: [f64; 6] = [7.65, 4.07, 3.57, 2.13, 1.99, 1.63];
    const FILM_2: [f64; 6] = [3.75, 2.4, 1.88, 1.29, 1.04, 0.69];
    const HDR_8: [f64; 6] = [1.52, 1.31, 1.36, 1.2, 1.17, 1.07];
    const HDR_4: [f64; 6] = [0.81, 0.69, 0.7, 0.52, 0.54, 0.47];

    /// What these presets were measured to make by a job alone.
    fn knowing(presets: &[(usize, f64)], now: Instant) -> Speeds {
        let mut speeds = Speeds::default();
        for &(preset, speed) in presets {
            speeds.learn(preset, Measured::Exactly(speed), 1, now);
        }
        speeds
    }

    /// A job at `preset` whose process goes at `speed`, a minute after its
    /// last move.
    fn going(preset: usize, speed: Measured, lead_secs: f64, jobs: u32) -> Going {
        let made = match speed {
            Measured::Exactly(speed) => Some(speed),
            Measured::AtLeast(_) => None,
        };
        Going { preset, speed: Some(speed), made, lead_secs, since_move: Duration::from_secs(60), jobs }
    }

    /// A job alone on a computer that makes `machine` at each preset, from
    /// a cold start: the presets it goes through, each process measured as
    /// fast as it goes, then moved as [`decide`] says once it is `lead`
    /// seconds ahead.
    fn settles(machine: &[f64; 6], cores: u32, lead: f64) -> Vec<usize> {
        let now = Instant::now();
        let mut speeds = Speeds::default();
        let mut preset = start(&speeds, load(1, cores, HD), 0.0, now);
        let mut through = vec![preset];
        for _ in 0..8 {
            let speed = Measured::Exactly(machine[preset]);
            speeds.learn(preset, speed, 1, now);
            match decide(&speeds, going(preset, speed, lead, 1), now) {
                Some(to) => preset = to,
                None => break,
            }
            through.push(preset);
        }
        through
    }

    #[test]
    fn before_anything_is_measured_the_start_follows_the_load() {
        assert_eq!(LADDER[target(load(1, 16, HD))], "medium");
        assert_eq!(LADDER[target(load(1, 8, HD))], "fast");
        assert_eq!(LADDER[target(load(2, 8, HD))], "veryfast");
        assert_eq!(LADDER[target(load(4, 8, HD))], "superfast");
        assert_eq!(LADDER[target(load(8, 8, HD))], "ultrafast");
        // Smaller pictures leave room; 4K and HEVC take more.
        assert_eq!(LADDER[target(load(2, 8, 1280 * 720))], "fast");
        assert_eq!(LADDER[target(load(1, 8, 3840 * 2160))], "superfast");
        assert_eq!(LADDER[target(Load { hevc: true, ..load(1, 8, HD) })], "superfast");
        // So do 60 frames a second, twice 30's work.
        assert_eq!(LADDER[target(Load { fps: 60.0, ..load(1, 8, HD) })], "veryfast");
        assert_eq!(LADDER[target(Load { fps: 30.0, ..load(1, 8, HD) })], "fast");
        // A step faster at a start or a seek, where the player waits.
        let (cold, now) = (Speeds::default(), Instant::now());
        assert_eq!(start(&cold, load(1, 8, HD), 0.0, now), FASTER);
        assert_eq!(start(&cold, load(1, 8, HD), 20.0, now), FAST);
        assert_eq!(start(&cold, load(8, 8, HD), 0.0, now), UF);
    }

    #[test]
    fn a_job_settles_at_the_slowest_preset_its_computer_makes_twice_real_time() {
        // 8 threads: medium makes 3.2 times real time, in one move.
        assert_eq!(settles(&FILM_8, 8, 60.0), [FASTER, MEDIUM]);
        // 4 threads: faster makes 2.1, and fast is expected to make 1.7.
        // A step at a time where each measurement leaves it close.
        assert_eq!(settles(&FILM_4, 4, 60.0), [SF, VF, FASTER]);
        // 2 threads: superfast makes 2.4, veryfast would make 1.9.
        assert_eq!(settles(&FILM_2, 2, 60.0), [UF, SF]);
        // Not while little is ready ahead: a restart could make the player wait.
        assert_eq!(settles(&FILM_8, 8, 20.0), [FASTER]);
    }

    #[test]
    fn unmeasured_presets_are_reckoned_from_the_nearest_measured() {
        let now = Instant::now();
        let speeds = knowing(&[(FASTER, 5.2)], now);
        assert_eq!(speeds.expect(FASTER, 1, now), Some((5.2, 0)));
        let (fast, away) = speeds.expect(FAST, 1, now).unwrap();
        assert!((fast - 5.2 * 0.78).abs() < 1e-9 && away == 1);
        let (medium, away) = speeds.expect(MEDIUM, 1, now).unwrap();
        assert!((medium - 5.2 * 0.78 * 0.8).abs() < 1e-9 && away == 2);
        let (ultrafast, slower) = speeds.expect(UF, 1, now).unwrap();
        assert!((ultrafast - 5.2 / (0.6 * 0.7 * 0.7)).abs() < 1e-9 && slower == 0);
        // The further the reckoning, the more room it needs: 2.3 expected of
        // medium two steps away isn't taken for the 2 it must make, 2.1 a
        // step away is.
        let speeds = knowing(&[(FASTER, 3.7)], now);
        assert_eq!(speeds.choose(1, Need::Steady, now), Some(FAST));
        let speeds = knowing(&[(FAST, 2.6)], now);
        assert_eq!(speeds.choose(1, Need::Steady, now), Some(MEDIUM));
        // Nothing measured, nothing chosen; nor from what was measured too long ago.
        assert_eq!(Speeds::default().choose(1, Need::Steady, now), None);
        assert_eq!(speeds.choose(1, Need::Steady, now + FRESH), None);
        assert!(speeds.any(now) && !speeds.any(now + FRESH));
    }

    #[test]
    fn one_easy_stretch_of_film_isnt_a_faster_computer() {
        let now = Instant::now();
        // Faster makes 2.3 on 4 threads: fast is expected to make 1.8.
        let mut speeds = knowing(&[(FASTER, 2.3)], now);
        assert_eq!(speeds.choose(1, Need::Steady, now), Some(FASTER));
        // A minute on, an easy stretch goes at 2.9: a third of the way, and
        // fast is still expected to make under 2.
        let later = now + Duration::from_secs(60);
        let known = |speeds: &Speeds| speeds.expect(FASTER, 1, later).unwrap().0;
        speeds.learn(FASTER, Measured::Exactly(2.9), 1, later);
        assert!((known(&speeds) - 2.5).abs() < 1e-9);
        assert_eq!(speeds.choose(1, Need::Steady, later), Some(FASTER));
        // The same measurement again (its process is paused) changes nothing.
        speeds.learn(FASTER, Measured::Exactly(2.9), 1, later);
        assert!((known(&speeds) - 2.5).abs() < 1e-9);
        // Stretch after stretch at 2.9, it is what the preset makes.
        for stretch in 1..=8 {
            speeds.learn(FASTER, Measured::Exactly(2.9), 1, later + Duration::from_secs(60 * stretch));
        }
        let later = later + Duration::from_secs(480);
        assert_eq!(speeds.choose(1, Need::Steady, later), Some(FAST));
        // A hard stretch counts at once: the preset must keep ahead through it.
        speeds.learn(FASTER, Measured::Exactly(2.1), 1, later + Duration::from_secs(60));
        assert_eq!(speeds.expect(FASTER, 1, later + Duration::from_secs(60)), Some((2.1, 0)));
    }

    #[test]
    fn a_start_or_a_seek_takes_a_preset_whose_first_segment_comes_soon() {
        let now = Instant::now();
        // The film on 8 threads, once medium was measured. A seek, or the
        // next film at the same size, starts at faster, whose first segment
        // comes in 0.7 s (medium's would take 1.3), and what goes on from
        // segments made before starts at medium.
        let speeds = knowing(&[(FASTER, 5.2), (MEDIUM, 3.22)], now);
        assert_eq!(start(&speeds, load(1, 8, HD), 0.0, now), FASTER);
        assert_eq!(start(&speeds, load(1, 8, HD), 30.0, now), MEDIUM);
        // Once ahead, the process that started at faster moves to medium,
        // even paused.
        let seeked = Going { since_move: Duration::from_secs(12), ..going(FASTER, Measured::Exactly(5.2), 60.0, 1) };
        assert_eq!(decide(&speeds, seeked, now), Some(MEDIUM));
        assert_eq!(decide(&speeds, Going { since_move: Duration::from_secs(5), ..seeked }, now), None, "just started");
        assert_eq!(decide(&speeds, Going { speed: None, made: None, lead_secs: 90.0, ..seeked }, now), Some(MEDIUM));
        // On 24 threads medium makes 7.8: its first segment comes as soon.
        let speeds = knowing(&[(MEDIUM, 7.75)], now);
        assert_eq!(start(&speeds, load(1, 24, HD), 0.0, now), MEDIUM);
        // On 4 threads faster makes 2.1: a seek starts at superfast.
        let speeds = knowing(&[(VF, 3.57), (FASTER, 2.13), (FAST, 1.99)], now);
        assert_eq!(start(&speeds, load(1, 4, HD), 0.0, now), SF);
        assert_eq!(start(&speeds, load(1, 4, HD), 30.0, now), FASTER);
        // A second film beside one starts at superfast (5.2 each), a fourth
        // at ultrafast.
        let speeds = knowing(&[(MEDIUM, 3.22), (VF, 7.29)], now);
        assert_eq!(start(&speeds, load(2, 8, HD), 0.0, now), SF);
        assert_eq!(start(&speeds, load(4, 8, HD), 0.0, now), UF);
    }

    #[test]
    fn encodes_share_the_computer_as_they_come_and_go() {
        let now = Instant::now();
        let ahead = |preset, jobs| going(preset, Measured::AtLeast(2.0), 70.0, jobs);
        // A film alone on 8 threads settled at medium.
        let mut speeds = knowing(&[(FASTER, 5.2), (MEDIUM, 3.22)], now);
        assert_eq!(speeds.expect(MEDIUM, 2, now), Some((1.61, 0)));
        assert_eq!(decide(&speeds, ahead(MEDIUM, 1), now), None);
        // A second starts: medium makes 1.6 for each, which keeps both
        // ahead. The new one settles at fast, expected to make 2.
        assert_eq!(decide(&speeds, ahead(MEDIUM, 2), now), None);
        assert_eq!(speeds.choose(2, Need::Steady, now), Some(FAST));
        // A third: 1.1 each at medium. The first moves to veryfast,
        // expected to make 2.5, while it is well ahead, rather than wait to
        // fall behind.
        assert_eq!(decide(&speeds, ahead(MEDIUM, 3), now), Some(VF));
        assert_eq!(decide(&speeds, ahead(MEDIUM, 3).with_lead(30.0), now), None, "not now");
        // What its process makes beside the others, 0.9 while two start and
        // 3 while they are paused, is about its share or more: medium's 3.2
        // alone still tells.
        speeds.learn(MEDIUM, Measured::Exactly(0.9), 3, now);
        speeds.learn(MEDIUM, Measured::Exactly(3.0), 3, now);
        assert_eq!(speeds.expect(MEDIUM, 1, now), Some((3.22, 0)));
        // The others leave: back to medium, with nothing to measure again.
        assert_eq!(decide(&speeds, ahead(VF, 1), now), Some(MEDIUM));
        assert_eq!(decide(&speeds, ahead(VF, 3), now), None);
        // Far less than its share is something else holding it back (the
        // computer's other work): 0.6 each of three is 1.8 alone at most.
        speeds.learn(MEDIUM, Measured::Exactly(0.6), 3, now);
        let (alone, _) = speeds.expect(MEDIUM, 1, now).unwrap();
        assert!((alone - 1.8).abs() < 1e-9, "{alone}");
    }

    #[test]
    fn what_a_process_makes_beside_others_is_the_least_its_preset_makes_alone() {
        let now = Instant::now();
        // A film that started beside three others made 5.5 at veryfast:
        // alone it makes that or more, and of four each has a quarter.
        let mut speeds = Speeds::default();
        speeds.learn(VF, Measured::Exactly(5.5), 4, now);
        assert_eq!(speeds.expect(VF, 1, now), Some((5.5, 0)));
        assert_eq!(speeds.expect(VF, 4, now), Some((1.375, 0)));
        assert_eq!(speeds.choose(4, Need::Steady, now), Some(UF), "superfast is expected to make just under 2");
        // More says more; less, its share or better, says nothing.
        speeds.learn(VF, Measured::Exactly(6.0), 4, now);
        speeds.learn(VF, Measured::Exactly(4.0), 4, now);
        assert_eq!(speeds.expect(VF, 1, now), Some((6.0, 0)));
        // The others left, and the read rate holds it at real time: fast is
        // expected to make 3.3 alone. (Medium's 2.6, three steps on, isn't
        // taken for the 2.9 that far a reckoning needs.)
        assert_eq!(decide(&speeds, going(VF, Measured::AtLeast(1.0), 70.0, 1), now), Some(FAST));
        // Measured alone, it is what the preset makes.
        speeds.learn(VF, Measured::Exactly(7.29), 1, now);
        assert_eq!(speeds.expect(VF, 1, now), Some((7.29, 0)));
        // A process held to the read rate shows the least its preset makes
        // too. Of one measured to make more, only that it still does…
        let mut speeds = knowing(&[(FAST, 4.1)], now);
        let later = now + FRESH * 2 / 3;
        speeds.learn(FAST, Measured::AtLeast(2.0), 1, later);
        assert_eq!(speeds.expect(FAST, 1, later + FRESH * 2 / 3), Some((4.1, 0)));
        // …and of one measured to make clearly less (the computer was busy
        // then), that it makes more now.
        let mut speeds = knowing(&[(SF, 1.3)], now);
        speeds.learn(SF, Measured::AtLeast(2.0), 1, now);
        assert_eq!(speeds.expect(SF, 1, now), Some((2.0, 0)));
        let mut speeds = knowing(&[(SF, 1.95)], now);
        speeds.learn(SF, Measured::AtLeast(2.0), 1, now);
        assert_eq!(speeds.expect(SF, 1, now), Some((1.95, 0)), "as much, within what film varies by");
    }

    #[test]
    fn a_job_about_to_keep_the_player_waiting_moves_as_far_as_it_takes() {
        let now = Instant::now();
        let at = |preset, speed, lead_secs| Going {
            since_move: Duration::from_secs(30),
            ..going(preset, Measured::Exactly(speed), lead_secs, 3)
        };
        // Three films on 8 threads where one was at medium, which makes 2.7
        // alone: 0.9 each.
        let mut speeds = knowing(&[(MEDIUM, 2.7)], now);
        // Veryfast is expected to make 2.1: three steps at once.
        assert_eq!(decide(&speeds, at(MEDIUM, 0.9, 4.0), now), Some(VF));
        assert_eq!(decide(&speeds, at(MEDIUM, 0.9, 20.0), now), None, "still ahead");
        assert_eq!(decide(&speeds, at(MEDIUM, 1.3, 4.0), now), None, "fast enough to keep up");
        let just_moved = Going { since_move: Duration::from_secs(3), ..at(MEDIUM, 0.9, 4.0) };
        assert_eq!(decide(&speeds, just_moved, now), None);
        // Far under real time (the computer's other work), it moves before
        // what is ready runs out, to ultrafast: 0.4 each at medium leaves
        // superfast 1.3.
        speeds.learn(MEDIUM, Measured::Exactly(0.4), 3, now);
        assert_eq!(decide(&speeds, at(MEDIUM, 0.4, 11.0), now), Some(UF));
        assert_eq!(decide(&speeds, at(MEDIUM, 0.4, 13.0), now), None);
        // Already the fastest: nowhere to go.
        assert_eq!(decide(&speeds, at(UF, 0.5, 0.0), now), None);
        // Held to the read rate with little ahead (a seek into what is
        // ready): that isn't the computer falling behind.
        assert_eq!(decide(&speeds, going(MEDIUM, Measured::AtLeast(1.0), 2.0, 3), now), None);
    }

    #[test]
    fn a_preset_that_proves_too_slow_is_left_and_not_tried_again() {
        let now = Instant::now();
        // Faster makes 2.7, and fast is expected to make 2.1: taken.
        let speeds = knowing(&[(FASTER, 2.7)], now);
        assert_eq!(decide(&speeds, going(FASTER, Measured::Exactly(2.7), 60.0, 1), now), Some(FAST));
        // Its burst there makes 1.4: back, and it stays at faster however
        // far ahead, and however long the read rate holds it.
        let tried = |made: f64, since_move: u64| {
            let mut speeds = speeds.clone();
            speeds.learn(FAST, Measured::Exactly(made), 1, now);
            let at_fast =
                Going { since_move: Duration::from_secs(since_move), ..going(FAST, Measured::Exactly(made), 60.0, 1) };
            let back = [Measured::Exactly(2.7), Measured::AtLeast(2.0)].map(|speed| going(FASTER, speed, 80.0, 1));
            (decide(&speeds, at_fast, now), back.map(|going| decide(&speeds, going, now)))
        };
        assert_eq!(tried(1.4, 20), (Some(FASTER), [None, None]));
        // Had it made 1.7, it would stay.
        assert_eq!(tried(1.7, 20).0, None);
        // Minutes after the move, a slow moment is another encode starting.
        assert_eq!(tried(1.4, 300).0, None);
    }

    #[test]
    fn a_job_held_to_the_read_rate_tries_a_step_slower_where_nothing_was_measured() {
        let now = Instant::now();
        let held = |preset| going(preset, Measured::AtLeast(2.0), 80.0, 1);
        // A burst too short to measure: all the process shows is the rate
        // it catches up at, twice real time. Enough for a step, whose own
        // burst then measures it.
        let mut speeds = Speeds::default();
        speeds.learn(VF, Measured::AtLeast(2.0), 1, now);
        assert_eq!(decide(&speeds, held(VF), now), Some(FASTER));
        assert_eq!(decide(&speeds, Going { since_move: Duration::from_secs(20), ..held(VF) }, now), None, "too soon");
        assert_eq!(decide(&speeds, held(VF).with_lead(30.0), now), None, "not far enough ahead");
        // There, its burst makes 1.4: back to veryfast, and not again.
        speeds.learn(FASTER, Measured::Exactly(1.4), 1, now);
        let at_faster = Going { since_move: Duration::from_secs(20), ..going(FASTER, Measured::Exactly(1.4), 80.0, 1) };
        assert_eq!(decide(&speeds, at_faster, now), Some(VF));
        assert_eq!(decide(&speeds, held(VF), now), None);
        // At the slowest preset there is nothing to try.
        assert_eq!(decide(&Speeds::default(), held(MEDIUM), now), None);
        // Nor where its own preset was measured: fast made 2.1 on 4 threads,
        // and medium is expected to make 1.7. Trying would restart it twice.
        let speeds = knowing(&[(FAST, 2.07)], now);
        assert_eq!(decide(&speeds, held(FAST), now), None);
    }

    #[test]
    fn a_start_that_keeps_ahead_stays_though_slow() {
        let now = Instant::now();
        // 1080p at 60 frames a second on 4 threads: superfast makes 1.4,
        // and nothing says ultrafast, a far worse picture, is needed.
        let speeds = knowing(&[(SF, 1.37)], now);
        let at_start = Going { since_move: Duration::from_secs(30), ..going(SF, Measured::Exactly(1.37), 60.0, 1) };
        assert_eq!(decide(&speeds, at_start, now), None);
        // With little ready ahead it still keeps up.
        assert_eq!(decide(&speeds, at_start.with_lead(2.0), now), None);
        // Under 1.15, it wouldn't for long.
        let slow = Going { speed: Some(Measured::Exactly(1.1)), ..at_start.with_lead(2.0) };
        assert_eq!(decide(&speeds, slow, now), Some(UF));
    }

    #[test]
    fn a_slower_preset_that_goes_as_fast_is_taken_for_its_picture() {
        let now = Instant::now();
        let at = |preset, made: f64, since_move| Going {
            since_move: Duration::from_secs(since_move),
            ..going(preset, Measured::Exactly(made), 60.0, 1)
        };
        // 4K HDR10 tone mapped on 8 threads started at faster, which made
        // 1.3, fell behind in a hard stretch and moved to veryfast: 1.42.
        // Faster goes about as fast: back to it once well ahead.
        let mut speeds = knowing(&[(VF, 1.42), (FASTER, 1.3)], now);
        assert_eq!(decide(&speeds, at(VF, 1.42, 60), now), Some(FASTER));
        // That step cost little: fast is tried, 40 s on…
        assert_eq!(decide(&speeds, at(FASTER, 1.3, 20), now), None);
        assert_eq!(decide(&speeds, at(FASTER, 1.3, 60), now), Some(FAST));
        // …and makes 1.45: it stays, and medium is tried. That makes 1.0,
        // far less: back to fast, for good.
        speeds.learn(FAST, Measured::Exactly(1.45), 1, now);
        assert_eq!(decide(&speeds, at(FAST, 1.45, 20), now), None);
        assert_eq!(decide(&speeds, at(FAST, 1.45, 60), now), Some(MEDIUM));
        speeds.learn(MEDIUM, Measured::Exactly(1.0), 1, now);
        assert_eq!(decide(&speeds, at(MEDIUM, 1.0, 20), now), Some(FAST));
        assert_eq!(decide(&speeds, at(FAST, 1.45, 60), now), None);
        // A slower preset that barely keeps ahead isn't worth its picture.
        let speeds = knowing(&[(VF, 1.36), (FASTER, 1.2)], now);
        assert_eq!(decide(&speeds, at(VF, 1.36, 60), now), None);
        // From a start at faster that makes 1.7, with nothing measured
        // beside it: well ahead, fast is tried; at 1.4 it isn't.
        let speeds = knowing(&[(FASTER, 1.7)], now);
        assert_eq!(decide(&speeds, at(FASTER, 1.7, 60), now), Some(FAST));
        let speeds = knowing(&[(FASTER, 1.4)], now);
        assert_eq!(decide(&speeds, at(FASTER, 1.4, 60), now), None);
        // One measured to go faster than this one was measured at another
        // time: veryfast made 1.6 as another program started taking the
        // processor, and superfast 1.4 once it had. Not that it goes as fast.
        let speeds = knowing(&[(SF, 1.36), (VF, 1.6)], now);
        assert_eq!(decide(&speeds, at(SF, 1.36, 60), now), None);
        // Nor is any of this for video its encoder holds back, which a
        // slower preset fast enough is chosen for: a film on 4 threads
        // stays at faster (2.1), though fast was measured to make 2.0. On
        // slower ones, where faster makes 1.8 and superfast made 2.9, it
        // stays too: each step of x264's costs little, and all of them much.
        let speeds = knowing(&[(VF, 3.57), (FASTER, 2.13), (FAST, 1.99)], now);
        assert_eq!(decide(&speeds, at(FASTER, 2.13, 60), now), None);
        let speeds = knowing(&[(SF, 2.9), (VF, 2.6), (FASTER, 1.8)], now);
        assert_eq!(decide(&speeds, at(FASTER, 1.8, 60), now), None);
        let speeds = knowing(&[(SF, 2.9), (VF, 2.6), (FASTER, 1.8), (FAST, 1.6)], now);
        assert_eq!(decide(&speeds, at(FASTER, 1.8, 60), now), None);
    }

    #[test]
    fn video_its_encoder_doesnt_hold_back_keeps_its_picture() {
        let now = Instant::now();
        // 4K HDR10 tone mapped on 8 threads: 1.07 to 1.52 times real time
        // whatever the preset. No preset makes twice real time: veryfast,
        // within 15 % of ultrafast, is as fast as it gets.
        let all: Vec<(usize, f64)> = HDR_8.iter().copied().enumerate().collect();
        assert_eq!(knowing(&all, now).choose(1, Need::Steady, now), Some(VF));
        // From a cold start at faster (1.2): fast enough to keep up, so it
        // stays rather than restart for a few percent.
        assert_eq!(settles(&HDR_8, 8, 2.0), [FASTER]);
        // On 4 threads nothing makes real time: the fastest, at once.
        let all: Vec<(usize, f64)> = HDR_4.iter().copied().enumerate().collect();
        assert_eq!(knowing(&all, now).choose(1, Need::Steady, now), Some(UF));
        let through = settles(&HDR_4, 4, 0.0);
        assert_eq!((through[0], *through.last().unwrap()), (SF, UF));
    }

    impl Going {
        fn with_lead(self, lead_secs: f64) -> Going {
            Going { lead_secs, ..self }
        }
    }
}
