// SPDX-License-Identifier: AGPL-3.0-only
//! The automatic preset for software encodes (`auto`): x264's and x265's
//! presets trade encoding speed for picture quality at the same bitrate.
//!
//! The rule: the software encodes together take at most half the computer's
//! time. One encode alone uses the slowest preset that makes twice real
//! time; N of them, the slowest that makes 2 N times real time alone.
//!
//! What a preset makes is measured ([`Speeds`]): a process runs as fast as
//! it can until FFmpeg's read rate holds it, and while no other software
//! encode runs beside it, its speed is its preset's. The other presets are
//! estimated from the nearest one measured ([`STEP`]): the room twice real
//! time leaves covers what an estimate a few steps away can be off by.
//! Before anything is measured, a job starts by its computer's processors
//! ([`target`]).
//!
//! A job moves in four cases, each a restart of FFmpeg where it was,
//! between two segments:
//!
//! - Ahead of its player, to the slowest preset that is fast enough.
//! - About to keep its player waiting, to a faster preset, as many steps at
//!   once as it takes.
//! - Under real time in its first minute at a preset, to a faster one: the
//!   estimate was wrong.
//! - When encodes start or end, to the preset for the new count, once the
//!   count has stayed the same for a minute.
//!
//! A job alone that keeps ahead is otherwise left where it is: another
//! program taking the processor for a while doesn't restart it.
//!
//! At a start or a seek the player waits, so the process starts at a preset
//! fast enough for its first segment to come soon ([`Need::Quick`]), and
//! moves to the steady one as soon as its speed is known and a restart
//! keeps nobody waiting ([`hand_over`]).

use std::time::{Duration, Instant};

/// Presets from the fastest to the slowest used. `fast` and `medium` are
/// left out: encoded as the transcoder does at 2 to 8 Mbps, a 1080p film, the
/// same with grain added and a 1080p60 clip each gained 0.5 to 2 dB (PSNR) a
/// step up to `faster`, and between nothing and 0.6 dB past it (the most on
/// the 1080p60 clip, less than nothing on the grain), for 1.3 to 1.6 times
/// the processor's time. x265 gained 0.1 dB on the film.
pub const LADDER: [&str; 4] = ["ultrafast", "superfast", "veryfast", "faster"];

/// The preset setting that asks for this.
pub const AUTO: &str = "auto";

/// A preset's speed as a share of the one a step faster (the one before it
/// in [`LADDER`]), about the middle of what was measured: x264 on 2 to 24
/// threads of a 12-core Ryzen, 1080p at 24 and 60 frames a second and 720p,
/// made 0.43 to 0.64 for superfast, 0.62 to 0.88 for veryfast, 0.60 to 0.87
/// for faster. Estimated from `ultrafast`, `faster` made as little as 0.6
/// of what these say: a preset expected at twice real time ([`COMFORT`])
/// still makes more than [`TOO_SLOW`], and is measured as soon as a job is
/// on it.
const STEP: [f64; LADDER.len()] = [1.0, 0.55, 0.7, 0.7];

/// The speed (× real time) a preset must be expected to make to be moved
/// to: the encode then takes half the computer's time, which leaves room
/// for harder scenes, a seek, and a second video starting.
const COMFORT: f64 = 2.0;
/// The speed a job keeps its preset with when more encodes start: under
/// it, the job moves to a faster one. Lower than [`COMFORT`], so that a
/// preset just fast enough isn't left and taken again.
const STAY: f64 = 1.6;
/// Under this speed, as fast as it can go, a job doesn't keep ahead.
const TOO_SLOW: f64 = 1.15;
/// The speed the preset it then moves to must be expected to make.
const RECOVER: f64 = 1.5;
/// Under this speed nothing is told of the preset: the input stopped
/// coming (a disk waking, a share that hangs), which no preset helps.
const STALLED: f64 = 0.1;
/// How long a process's first segment may take to encode at a start or a
/// seek, where the player waits for it (seconds): about as long as FFmpeg
/// takes to start. The process hands over to the steady preset seconds
/// later ([`hand_over`]).
const FIRST_SEGMENT_SECS: f64 = 0.4;
/// How many presets faster than its steady one a job starts at before
/// anything was measured, where the player waits: the first segment needs
/// three to four times the steady speed ([`Need::Quick`]), and each step
/// makes 1.4 to 1.8 times the one after it.
const QUICK_STEPS: usize = 3;
/// The frames x264 reads before its first comes out, by preset
/// (`rc-lookahead`): a slower preset must go faster for its first segment
/// to come as soon.
const LOOKAHEAD_FRAMES: [f64; LADDER.len()] = [0.0, 0.0, 10.0, 20.0];
/// What the encoder needs past the segment's own length before it is
/// written, besides the frames it looks ahead (seconds of video): measured
/// with [`FIRST_SEGMENT_SECS`] on 24 threads, where a 2 s segment took 0.44
/// s from the job's creation at ultrafast (38 × real time) and 0.96 s at
/// medium (7.8 ×, 40 frames looked ahead), 0.4 s of it FFmpeg's start
/// whatever the preset.
const SEGMENT_TAIL_SECS: f64 = 0.5;

/// A measurement that says a preset makes more than was known counts in
/// full only once it has said so for this long: one easy stretch of film
/// isn't a faster computer. One that says less counts at once.
const RISES_OVER: Duration = Duration::from_secs(60);
/// Below this much video ready past what the player asked for (seconds), a
/// job that is too slow moves to a faster preset.
const SHORT_LEAD_SECS: f64 = 8.0;
/// Above it, a job may move when nothing presses: a restart takes under a
/// second, and a preset that proves too slow is left seconds later.
const LONG_LEAD_SECS: f64 = 10.0;
/// The least time between two moves of a job: its new process's speed is
/// measured first, over the 4 s it is measured over.
const MOVE_AFTER: Duration = Duration::from_secs(4);
/// How long the number of software encodes must have stayed the same
/// before jobs move for it, so that encodes coming and going don't restart
/// the others each time. Also how long a preset has to show it was a
/// mistake: later, a slow stretch is something else's doing.
const SETTLES: Duration = Duration::from_secs(60);
/// How long a measurement tells what a preset makes, once no process shows
/// it any more: the computer's other work changes.
const FRESH: Duration = Duration::from_secs(10 * 60);

/// How fast a preset must be expected to go to be taken.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Need {
    /// To keep ahead while it plays ([`COMFORT`]).
    Steady,
    /// For the first segment to come soon ([`FIRST_SEGMENT_SECS`]), by the
    /// segments' length and the video's frames a second: with 2 s segments
    /// at 24 frames a second, 6.3 × real time at ultrafast and 8.3 at faster.
    Quick { segment_secs: f64, fps: f64 },
    /// To get a job that falls behind ahead again ([`RECOVER`]).
    Recover,
}

impl Need {
    fn speed(self, preset: usize) -> f64 {
        match self {
            Need::Steady => COMFORT,
            Need::Quick { segment_secs, fps } => {
                let looked_ahead = LOOKAHEAD_FRAMES[preset] / fps.max(1.0);
                ((segment_secs + SEGMENT_TAIL_SECS + looked_ahead) / FIRST_SEGMENT_SECS).max(COMFORT)
            }
            Need::Recover => RECOVER,
        }
    }
}

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

/// What a preset was measured to make with no other software encode
/// running (× real time), and when.
#[derive(Debug, Clone, Copy)]
struct Seen {
    alone: f64,
    at: Instant,
    /// Since when measurements have said it makes more, and what was known
    /// before they did ([`RISES_OVER`]).
    rising: Option<(Instant, f64)>,
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
    /// A process at `preset` went at `measured` at `at`, with no other
    /// software encode running if it went as fast as it could. A process
    /// the read rate holds shows only that its preset still makes that
    /// much, which keeps what was known from growing old.
    pub fn learn(&mut self, preset: usize, measured: Measured, at: Instant) {
        let speed = measured.speed();
        let exact = matches!(measured, Measured::Exactly(_));
        let Some(slot) = self.seen.get_mut(preset) else { return };
        if !speed.is_finite() || speed < STALLED {
            return;
        }
        let measured = Seen { alone: speed, at, rising: None };
        *slot = match slot.filter(|old| old.fresh(at)) {
            // The same measurement again: a paused process keeps its last.
            Some(old) if at <= old.at => Some(old),
            // More than was known: counted in as long as it has said so,
            // from what was known before it did, and in full after that.
            Some(old) if speed > old.alone => {
                let (since, before) = old.rising.unwrap_or((old.at, old.alone));
                let said_for = at.saturating_duration_since(since).as_secs_f64() / RISES_OVER.as_secs_f64();
                let alone = (before + (speed - before) * said_for).max(old.alone);
                Some(if said_for >= 1.0 { measured } else { Seen { alone, at, rising: Some((since, before)) } })
            }
            Some(_) if exact => Some(measured),
            Some(old) => Some(Seen { at, ..old }),
            None if exact => Some(measured),
            None => None,
        };
    }

    /// Whether anything measured still tells.
    pub fn any(&self, now: Instant) -> bool {
        self.seen.iter().flatten().any(|s| s.fresh(now))
    }

    /// What `preset` is expected to make alone, from the nearest preset
    /// measured (of two as near, the later).
    pub fn expect(&self, preset: usize, now: Instant) -> Option<f64> {
        let (from, seen) = self
            .seen
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().filter(|s| s.fresh(now)).map(|s| (i, s)))
            .min_by_key(|(i, s)| (i.abs_diff(preset), std::cmp::Reverse(s.at)))?;
        let (faster, slower) = (from.min(preset), from.max(preset));
        let steps: f64 = STEP[faster + 1..=slower].iter().product();
        Some(if preset > from { seen.alone * steps } else { seen.alone / steps })
    }

    /// The slowest preset expected to go as fast as `need` asks with `jobs`
    /// software encodes running, each needing its share of the computer.
    /// `None` when none is, or nothing was measured.
    pub fn choose(&self, jobs: u32, need: Need, now: Instant) -> Option<usize> {
        let sharing = f64::from(jobs.max(1));
        (0..LADDER.len())
            .rev()
            .find(|&preset| self.expect(preset, now).is_some_and(|alone| alone / sharing >= need.speed(preset)))
    }

    /// The preset a running job moves to when it has to: the slowest that
    /// is fast enough for `need`; where none is, the slowest that still
    /// gets ahead ([`RECOVER`]), or the fastest. `None` when nothing was
    /// measured.
    pub fn settle_for(&self, jobs: u32, need: Need, now: Instant) -> Option<usize> {
        self.any(now)
            .then(|| self.choose(jobs, need, now).or_else(|| self.choose(jobs, Need::Recover, now)).unwrap_or(0))
    }
}

/// The software encodes running, as seen every few seconds: how long the
/// same number has been running, and when two last encoded at once.
#[derive(Debug, Clone, Copy, Default)]
pub struct Crowd {
    jobs: u32,
    changed: Option<Instant>,
    shared: Option<Instant>,
}

impl Crowd {
    /// `jobs` software encodes run now, `encoding` of them not paused.
    pub fn tick(&mut self, jobs: u32, encoding: u32, now: Instant) {
        // A job that starts or ends alone moves nobody, and once none is
        // left there is nobody to move.
        if jobs == 0 {
            self.changed = None;
        } else if jobs != self.jobs && jobs.max(self.jobs) >= 2 {
            self.changed = Some(now);
        }
        self.jobs = jobs;
        if encoding >= 2 {
            self.shared = Some(now);
        }
    }

    /// Whether the same number of encodes has run for long enough to move
    /// by ([`SETTLES`]).
    pub fn settled(&self, now: Instant) -> bool {
        self.changed.is_none_or(|at| now.saturating_duration_since(at) >= SETTLES)
    }

    /// Whether no two encoded at once in the `window` before `at`: a speed
    /// measured over it is what the preset makes alone. Encodes that are
    /// paused far ahead of their players take none of the computer's time.
    pub fn alone(&self, at: Instant, window: Duration) -> bool {
        self.shared.is_none_or(|shared| at.saturating_duration_since(shared) >= window)
    }
}

/// What a job's first preset depends on besides what was measured.
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
    /// The length of its segments (seconds).
    pub segment_secs: f64,
}

/// The preset (index in [`LADDER`]) a job starts from before anything was
/// measured: by the processors each encode has, counted for a slow one (an
/// Atom's, where each makes a third of what a desktop's does), since a
/// start that is too slow keeps the player waiting and one too fast costs a
/// few seconds of a lesser picture.
pub fn target(load: Load) -> usize {
    room(load).min(LADDER.len() - 1)
}

/// How much room the load leaves each encode, as steps up a ladder that
/// goes two past [`LADDER`]'s end: a load with room to spare starts at the
/// slowest preset even where a start is a step faster.
fn room(load: Load) -> usize {
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
/// the player asked for, for a job whose last process ran at `current`.
///
/// - Before anything was measured: the load's ([`target`]), [`QUICK_STEPS`]
///   faster when nothing is ready ahead (a start or a seek), where the
///   player waits.
/// - With video ready ahead: the slowest preset that keeps ahead with room
///   to spare ([`Need::Steady`]).
/// - With nothing ready ahead: the slowest whose first segment comes soon
///   ([`Need::Quick`]), or the fastest where none's does.
/// - Where no preset keeps ahead with room to spare (tone mapping takes
///   more than the encoder, or more encodes run than the computer has room
///   for), what was measured picks nothing, and only rules out: the job
///   keeps the preset it had, or takes the load's, unless that one is known
///   not to keep ahead ([`TOO_SLOW`]). Then it takes the slowest that gets
///   ahead ([`RECOVER`]), or the fastest.
pub fn start(speeds: &Speeds, load: Load, lead_secs: f64, current: Option<usize>, now: Instant) -> usize {
    let waiting = lead_secs <= 0.0;
    let by_load = if waiting { room(load).saturating_sub(QUICK_STEPS).min(LADDER.len() - 1) } else { target(load) };
    if !speeds.any(now) {
        return by_load;
    }
    let jobs = load.software_jobs;
    match speeds.choose(jobs, Need::Steady, now) {
        Some(steady) if !waiting => steady,
        Some(_) => {
            let quick = Need::Quick { segment_secs: load.segment_secs, fps: load.fps };
            speeds.choose(jobs, quick, now).unwrap_or(0)
        }
        None => {
            let keeps_ahead = |preset: &usize| {
                speeds.expect(*preset, now).is_some_and(|alone| alone / f64::from(jobs.max(1)) >= TOO_SLOW)
            };
            let kept = current.or(Some(by_load)).filter(keeps_ahead);
            kept.or_else(|| speeds.choose(jobs, Need::Recover, now)).unwrap_or(0)
        }
    }
}

/// How a job on the automatic preset goes: see [`decide`].
#[derive(Debug, Clone, Copy)]
pub struct Going {
    /// Its preset (index in [`LADDER`]).
    pub preset: usize,
    /// What its process makes now, over the last few seconds; `None` while
    /// it is paused, or too young to tell.
    pub speed: Option<Measured>,
    /// What it makes as fast as it goes, over the last 20 s or all it has
    /// run; `None` while the read rate holds it, or it is paused.
    pub made: Option<f64>,
    /// Seconds of video ready past what the player asked for.
    pub lead_secs: f64,
    /// Since its last start or move.
    pub since_move: Duration,
    /// Software encodes running, this one included.
    pub jobs: u32,
    /// Whether that many have been running for a while ([`Crowd::settled`]).
    pub settled: bool,
}

/// The preset a running job should move to now, if any, by what its kind
/// of job was measured to make (`speeds`).
pub fn decide(speeds: &Speeds, going: Going, now: Instant) -> Option<usize> {
    let Going { preset, speed, made, lead_secs, since_move, jobs, settled } = going;
    // Nothing before its process's speed is known.
    if since_move < MOVE_AFTER {
        return None;
    }
    // Too slow to keep ahead, with little ready or in its first minute
    // here: faster, by as many steps as it takes.
    let too_slow = |speed: f64| (STALLED..TOO_SLOW).contains(&speed);
    let about_to_wait =
        matches!(speed, Some(Measured::Exactly(lately)) if too_slow(lately)) && lead_secs < SHORT_LEAD_SECS;
    let a_mistake = made.is_some_and(too_slow) && since_move < SETTLES;
    if preset > 0 && (about_to_wait || a_mistake) {
        let fast_enough = speeds.settle_for(jobs, Need::Recover, now).unwrap_or(preset - 1);
        return Some(fast_enough.min(preset - 1));
    }
    if lead_secs <= LONG_LEAD_SECS || !settled {
        return None;
    }
    // Ahead: the slowest preset that is fast enough…
    if let Some(slower) = speeds.choose(jobs, Need::Steady, now).filter(|to| *to > preset) {
        return Some(slower);
    }
    // …or a faster one, where more encodes run than its preset leaves each
    // enough for. Alone, a job that keeps ahead stays.
    let alone = speeds.expect(preset, now)?;
    if jobs >= 2 && alone / f64::from(jobs) < STAY {
        return speeds.settle_for(jobs, Need::Steady, now).filter(|to| *to < preset);
    }
    None
}

/// Where a process that started quick for a waiting player ([`start`]) at
/// `current`, `since_start` ago, moves once its own speed is known, with
/// `ahead_secs` of video ready past where the player can be: the preset a
/// start with video ready takes, when that is slower. In the process's
/// first minute this doesn't wait for the other encodes to settle, as
/// [`decide`]'s moves do: the move was planned when the process started,
/// and moves this job alone. Later, only [`decide`] moves it.
pub fn hand_over(
    speeds: &Speeds,
    load: Load,
    current: usize,
    since_start: Duration,
    ahead_secs: f64,
    now: Instant,
) -> Option<usize> {
    if ahead_secs <= LONG_LEAD_SECS || since_start >= SETTLES {
        return None;
    }
    let steady = start(speeds, load, ahead_secs, Some(current), now);
    (steady > current).then_some(steady)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: u64 = 1920 * 1080;
    const UF: usize = 0;
    const SF: usize = 1;
    const VF: usize = 2;
    const FASTER: usize = 3;

    /// What each preset made flat out on a Ryzen 9 5900X held to 8, 4 and 2
    /// threads: a 1080p film at 24 frames a second, a 1080p video at 60,
    /// and 4K HDR10 tone mapped on the CPU, which the tone mapping holds
    /// back more than the encoder.
    const FILM_8: [f64; 4] = [15.58, 9.52, 7.29, 5.2];
    const FILM_4: [f64; 4] = [7.65, 4.07, 3.57, 2.13];
    const FILM_2: [f64; 4] = [3.75, 2.4, 1.88, 1.29];
    const P60_2: [f64; 4] = [1.9, 0.92, 0.7, 0.5];
    const HDR_8: [f64; 4] = [1.52, 1.31, 1.36, 1.2];
    const HDR_4: [f64; 4] = [0.81, 0.69, 0.7, 0.52];

    fn load(software_jobs: u32, cores: u32, pixels: u64) -> Load {
        Load { software_jobs, cores, pixels, fps: 24.0, hevc: false, segment_secs: 2.0 }
    }

    /// What these presets were measured to make by a job alone.
    fn knowing(presets: &[(usize, f64)], now: Instant) -> Speeds {
        let mut speeds = Speeds::default();
        for &(preset, speed) in presets {
            speeds.learn(preset, Measured::Exactly(speed), now);
        }
        speeds
    }

    fn all(machine: &[f64; 4], now: Instant) -> Speeds {
        knowing(&[0, 1, 2, 3].map(|preset| (preset, machine[preset])), now)
    }

    /// A job at `preset` going flat out at `speed`, two minutes after its
    /// last move, the encodes around it settled.
    fn going(preset: usize, speed: f64, lead_secs: f64, jobs: u32) -> Going {
        Going {
            preset,
            speed: Some(Measured::Exactly(speed)),
            made: Some(speed),
            lead_secs,
            since_move: Duration::from_secs(120),
            jobs,
            settled: true,
        }
    }

    /// A job alone on a computer that makes `machine` at each preset, from
    /// a cold start: the presets it goes through, each measured flat out
    /// and the job moved as [`decide`] says once it is a minute ahead.
    fn settles(machine: &[f64; 4], load: Load) -> Vec<usize> {
        let mut now = Instant::now();
        let mut speeds = Speeds::default();
        let mut preset = start(&speeds, load, 0.0, None, now);
        let mut through = vec![preset];
        for _ in 0..8 {
            now += Duration::from_secs(30);
            speeds.learn(preset, Measured::Exactly(machine[preset]), now);
            match decide(&speeds, going(preset, machine[preset], 60.0, 1), now) {
                Some(to) => preset = to,
                None => break,
            }
            through.push(preset);
        }
        through
    }

    #[test]
    fn before_anything_is_measured_the_start_follows_the_load() {
        assert_eq!(LADDER[target(load(1, 16, HD))], "faster");
        assert_eq!(LADDER[target(load(1, 6, HD))], "faster");
        assert_eq!(LADDER[target(load(2, 8, HD))], "veryfast");
        assert_eq!(LADDER[target(load(4, 8, HD))], "superfast");
        assert_eq!(LADDER[target(load(8, 8, HD))], "ultrafast");
        // Smaller pictures leave room; 4K and HEVC take more.
        assert_eq!(LADDER[target(load(2, 8, 1280 * 720))], "faster");
        assert_eq!(LADDER[target(load(1, 8, 3840 * 2160))], "superfast");
        assert_eq!(LADDER[target(Load { hevc: true, ..load(1, 8, HD) })], "superfast");
        // So do 60 frames a second, twice 30's work.
        assert_eq!(LADDER[target(Load { fps: 60.0, ..load(1, 8, HD) })], "veryfast");
        assert_eq!(LADDER[target(Load { fps: 30.0, ..load(1, 8, HD) })], "faster");
        // Three steps faster at a start or a seek, where the player waits;
        // fewer where the load has room to spare.
        let (cold, now) = (Speeds::default(), Instant::now());
        assert_eq!(start(&cold, load(1, 6, HD), 0.0, None, now), UF);
        assert_eq!(start(&cold, load(1, 6, HD), 20.0, None, now), FASTER);
        assert_eq!(start(&cold, load(1, 8, HD), 0.0, None, now), SF);
        assert_eq!(start(&cold, load(1, 16, HD), 0.0, None, now), VF);
        assert_eq!(start(&cold, load(8, 8, HD), 0.0, None, now), UF);
    }

    #[test]
    fn a_job_alone_ends_on_the_slowest_preset_that_makes_twice_real_time() {
        // A start, then straight to it once measured: 8 threads make 9.5 ×
        // at superfast, and faster is estimated at 4.7 (it makes 5.2).
        assert_eq!(settles(&FILM_8, load(1, 8, HD)), [SF, FASTER]);
        // 4 threads make 7.7 × at ultrafast: faster is estimated at 2.06.
        assert_eq!(settles(&FILM_4, load(1, 4, HD)), [UF, FASTER]);
        // 2 threads make 3.8 × at ultrafast: superfast is estimated at 2.06,
        // veryfast at 1.4.
        assert_eq!(settles(&FILM_2, load(1, 2, HD)), [UF, SF]);
    }

    #[test]
    fn a_job_never_moves_to_a_preset_estimated_under_twice_real_time() {
        // 1080p60 on 2 threads: ultrafast makes 1.9 ×, superfast under 1.
        let p60 = Load { fps: 60.0, ..load(1, 2, HD) };
        assert_eq!(settles(&P60_2, p60), [UF]);
        // Nor when ultrafast makes less beside another program.
        let now = Instant::now();
        let speeds = knowing(&[(UF, 1.7)], now);
        for lead in [9.0, 30.0, 46.0, 90.0] {
            assert_eq!(decide(&speeds, going(UF, 1.7, lead, 1), now), None, "{lead} s ahead");
        }
        // As many steps past what was measured as are expected to make it.
        let speeds = knowing(&[(UF, 7.0)], now);
        assert_eq!(speeds.choose(1, Need::Steady, now), Some(VF), "faster is estimated at 1.9");
    }

    #[test]
    fn a_job_alone_that_keeps_ahead_stays_where_it_is() {
        // 4K HDR tone mapped on the CPU makes no preset twice real time:
        // the job stays at the one it started with, whatever it is told.
        assert_eq!(settles(&HDR_8, load(1, 8, HD)), [SF]);
        let now = Instant::now();
        let speeds = all(&HDR_8, now);
        for preset in [SF, VF, FASTER] {
            for lead in [9.0, 50.0, 90.0] {
                assert_eq!(decide(&speeds, going(preset, HDR_8[preset], lead, 1), now), None);
            }
        }
        // A film whose preset another program slows to 1.3 × stays too.
        let mut speeds = all(&FILM_4, now);
        speeds.learn(FASTER, Measured::Exactly(1.3), now + Duration::from_secs(5));
        assert_eq!(decide(&speeds, going(FASTER, 1.3, 60.0, 1), now + Duration::from_secs(5)), None);
    }

    #[test]
    fn a_job_about_to_keep_its_player_waiting_moves_as_many_steps_as_it_takes() {
        let now = Instant::now();
        // 4K HDR on 4 threads at faster makes half of real time: only
        // ultrafast is estimated fast enough.
        let speeds = knowing(&[(FASTER, HDR_4[FASTER])], now);
        assert_eq!(decide(&speeds, going(FASTER, 0.52, 4.0, 1), now), Some(UF));
        // With enough ready ahead it waits, past its first minute.
        assert_eq!(decide(&speeds, going(FASTER, 0.52, 20.0, 1), now), None);
        // With nothing measured, a step.
        assert_eq!(decide(&Speeds::default(), going(FASTER, 0.9, 4.0, 1), now), Some(VF));
        // Fast enough to keep ahead, or the fastest already: it stays.
        assert_eq!(decide(&speeds, going(FASTER, 1.2, 4.0, 1), now), None);
        assert_eq!(decide(&speeds, going(UF, 0.5, 0.0, 1), now), None);
        // Just moved: its speed isn't known yet.
        let just = Going { since_move: Duration::from_secs(3), ..going(FASTER, 0.52, 4.0, 1) };
        assert_eq!(decide(&speeds, just, now), None);
        // The read rate holding it at real time isn't the preset's doing.
        let held = Going { speed: Some(Measured::AtLeast(1.0)), made: None, ..going(FASTER, 1.0, 4.0, 1) };
        assert_eq!(decide(&speeds, held, now), None);
        // Nor is input that stopped coming.
        assert_eq!(decide(&speeds, going(FASTER, 0.02, 4.0, 1), now), None);
    }

    #[test]
    fn a_preset_that_proves_too_slow_in_its_first_minute_is_left_whatever_is_ready() {
        let now = Instant::now();
        let speeds = knowing(&[(SF, 4.0), (VF, 0.9)], now);
        let moved = |secs| Going { since_move: Duration::from_secs(secs), ..going(VF, 0.9, 60.0, 1) };
        assert_eq!(decide(&speeds, moved(3), now), None, "too soon to tell");
        assert_eq!(decide(&speeds, moved(5), now), Some(SF));
        assert_eq!(decide(&speeds, moved(20), now), Some(SF));
        // Later, it is something else's doing, and the job rides it out.
        assert_eq!(decide(&speeds, moved(90), now), None);
    }

    #[test]
    fn encodes_running_together_each_take_a_preset_that_leaves_room_for_the_others() {
        let now = Instant::now();
        let speeds = all(&FILM_8, now);
        // Four need 8 × alone: superfast makes 9.5, veryfast 7.3.
        assert_eq!(speeds.choose(4, Need::Steady, now), Some(SF));
        assert_eq!(decide(&speeds, going(FASTER, 5.2, 60.0, 4), now), Some(SF));
        assert_eq!(decide(&speeds, going(UF, 15.0, 60.0, 4), now), Some(SF));
        assert_eq!(decide(&speeds, going(SF, 9.5, 60.0, 4), now), None);
        // Not before the same four have run for a while, nor one that
        // isn't ahead.
        let unsettled = Going { settled: false, ..going(FASTER, 5.2, 60.0, 4) };
        assert_eq!(decide(&speeds, unsettled, now), None);
        assert_eq!(decide(&speeds, going(FASTER, 5.2, 9.0, 4), now), None);
        // Three: faster makes 1.7 × for each, enough to stay at, where
        // veryfast would be taken anew. Two fit at faster.
        assert_eq!(decide(&speeds, going(FASTER, 5.2, 60.0, 3), now), None);
        assert_eq!(speeds.choose(3, Need::Steady, now), Some(VF));
        assert_eq!(speeds.choose(2, Need::Steady, now), Some(FASTER));
        // Alone again: back to faster.
        assert_eq!(decide(&speeds, going(SF, 9.5, 60.0, 1), now), Some(FASTER));
        // More than the computer has room for: the fastest.
        assert_eq!(decide(&speeds, going(SF, 9.5, 60.0, 12), now), Some(UF));
    }

    #[test]
    fn a_start_or_a_seek_takes_a_preset_whose_first_segment_comes_soon() {
        let now = Instant::now();
        let speeds = all(&FILM_8, now);
        let film = load(1, 8, HD);
        // 2 s segments: superfast makes 9.5 × where 6.3 is needed, faster
        // 5.2 where 8.3 is.
        assert_eq!(start(&speeds, film, 0.0, None, now), SF);
        // Two encodes: each has half. With video ready ahead, nobody
        // waits: the steady preset.
        assert_eq!(start(&speeds, Load { software_jobs: 2, ..film }, 0.0, None, now), UF);
        assert_eq!(start(&speeds, Load { software_jobs: 2, ..film }, 20.0, None, now), FASTER);
        // Where none makes the first segment that soon, the fastest: 6 s
        // segments take longer to fill.
        let long = Load { segment_secs: 6.0, ..film };
        assert_eq!(start(&speeds, long, 0.0, None, now), UF);
        assert_eq!(start(&speeds, long, 20.0, None, now), FASTER);
    }

    #[test]
    fn a_quick_start_hands_over_to_the_steady_preset_once_ahead() {
        let now = Instant::now();
        let soon = Duration::from_secs(3);
        // Measured at ultrafast on 4 threads: faster is estimated at 2.06.
        let speeds = knowing(&[(UF, FILM_4[UF])], now);
        assert_eq!(hand_over(&speeds, load(1, 4, HD), UF, soon, 9.0, now), None, "not far enough ahead");
        assert_eq!(hand_over(&speeds, load(1, 4, HD), UF, soon, 12.0, now), Some(FASTER));
        // Beside another encode, however lately it started: faster makes
        // 2.6 × for each of two on 8 threads.
        assert_eq!(hand_over(&all(&FILM_8, now), load(2, 8, HD), UF, soon, 12.0, now), Some(FASTER));
        // Encodes that started together, before anything was measured: the
        // load's.
        assert_eq!(hand_over(&Speeds::default(), load(2, 8, HD), UF, soon, 12.0, now), Some(VF));
        // Where the quick preset is the steady one too, it stays: 1080p60 on
        // 2 threads makes 1.9 × at ultrafast.
        let p60 = Load { fps: 60.0, ..load(1, 2, HD) };
        assert_eq!(hand_over(&knowing(&[(UF, P60_2[UF])], now), p60, UF, soon, 12.0, now), None);
        // Past its first minute, only `decide` moves it.
        let later = Duration::from_secs(90);
        assert_eq!(hand_over(&all(&FILM_8, now), load(2, 8, HD), UF, later, 12.0, now), None);
    }

    #[test]
    fn where_no_preset_is_fast_enough_a_job_keeps_the_one_it_had() {
        let now = Instant::now();
        let film = load(1, 8, HD);
        // 4K HDR tone mapped on 8 threads makes 1.2 to 1.5 × whatever the
        // preset: a seek leaves the job where it was…
        let speeds = all(&HDR_8, now);
        assert_eq!(start(&speeds, film, 0.0, Some(VF), now), VF);
        assert_eq!(start(&speeds, film, 0.0, Some(FASTER), now), FASTER);
        // …and a new job starts by its load, as the first one did.
        assert_eq!(start(&speeds, film, 0.0, None, now), SF);
        // Unless that preset is known not to keep ahead: then the slowest
        // that gets ahead, or the fastest.
        let speeds = knowing(&[(UF, 1.6), (FASTER, 1.0)], now);
        assert_eq!(start(&speeds, film, 0.0, Some(FASTER), now), UF);
        assert_eq!(start(&all(&HDR_4, now), film, 0.0, None, now), UF);
    }

    #[test]
    fn what_a_preset_makes_falls_at_once_and_rises_over_a_minute() {
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        let alone = |speeds: &Speeds, now| speeds.expect(VF, now).unwrap();
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        let mut speeds = knowing(&[(VF, 5.0)], start);
        speeds.learn(VF, Measured::Exactly(4.0), at(5));
        assert_eq!(alone(&speeds, at(5)), 4.0);
        // The same reading again changes nothing.
        speeds.learn(VF, Measured::Exactly(6.0), at(5));
        assert_eq!(alone(&speeds, at(5)), 4.0);
        // Read every second, more counts in as long as it has said so.
        for secs in 6..=20 {
            speeds.learn(VF, Measured::Exactly(6.0), at(secs));
        }
        assert!(close(alone(&speeds, at(20)), 4.5), "a quarter of the way after 15 s");
        for secs in 21..=65 {
            speeds.learn(VF, Measured::Exactly(6.0), at(secs));
        }
        assert_eq!(alone(&speeds, at(65)), 6.0, "in full after a minute");
        // A rise after that starts anew.
        speeds.learn(VF, Measured::Exactly(8.0), at(80));
        assert!(close(alone(&speeds, at(80)), 6.5), "{}", alone(&speeds, at(80)));
        // One reading under what is known counts at once, and ends the rise.
        speeds.learn(VF, Measured::Exactly(6.0), at(81));
        assert_eq!(alone(&speeds, at(81)), 6.0);
        // Held by the read rate, a process shows the least it makes: no
        // less than was known…
        speeds.learn(VF, Measured::AtLeast(2.0), at(85));
        assert_eq!(alone(&speeds, at(85)), 6.0);
        // …and nothing of a preset never measured.
        speeds.learn(UF, Measured::AtLeast(2.0), at(85));
        assert_eq!(speeds.expect(UF, at(85)).map(|alone| alone > 6.0), Some(true), "estimated from veryfast");
        // Input that stopped coming tells nothing.
        speeds.learn(VF, Measured::Exactly(0.01), at(90));
        assert_eq!(alone(&speeds, at(90)), 6.0);
    }

    #[test]
    fn what_was_measured_is_forgotten_after_ten_minutes_unless_a_process_still_shows_it() {
        let start = Instant::now();
        let at = |mins: u64| start + Duration::from_secs(mins * 60);
        let mut speeds = knowing(&[(VF, 3.2)], start);
        assert!(speeds.any(at(9)));
        assert!(!speeds.any(at(11)));
        assert_eq!(super::start(&speeds, load(1, 4, HD), 0.0, None, at(11)), UF, "by the processors again");
        // A process held at its preset keeps it known.
        speeds.learn(VF, Measured::AtLeast(2.0), at(9));
        assert!(speeds.any(at(18)));
        assert_eq!(speeds.expect(VF, at(18)), Some(3.2));
    }

    #[test]
    fn encodes_coming_and_going_move_the_others_once_they_have_settled() {
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        let mut crowd = Crowd::default();
        // One job alone is settled from its start.
        crowd.tick(1, 1, at(0));
        assert!(crowd.settled(at(0)));
        // A second one starts: a minute before anyone moves for it.
        crowd.tick(2, 2, at(30));
        assert!(!crowd.settled(at(60)));
        assert!(crowd.settled(at(90)));
        // It ends, and another starts 20 s later: counted from the last.
        crowd.tick(1, 1, at(100));
        crowd.tick(2, 2, at(120));
        assert!(!crowd.settled(at(170)));
        assert!(crowd.settled(at(180)));
        // Both end, then a new one starts alone: nobody to wait for.
        crowd.tick(0, 0, at(200));
        crowd.tick(1, 1, at(205));
        assert!(crowd.settled(at(205)));
    }

    #[test]
    fn a_speed_tells_of_its_preset_only_when_no_other_encode_ran_beside_it() {
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        let window = Duration::from_secs(20);
        let mut crowd = Crowd::default();
        crowd.tick(1, 1, at(0));
        assert!(crowd.alone(at(5), window));
        // Two encoding at once…
        crowd.tick(2, 2, at(10));
        assert!(!crowd.alone(at(10), window));
        // …then one paused far ahead: the other is alone on the processor,
        // which shows once its last 20 s are its own.
        crowd.tick(2, 1, at(15));
        assert!(!crowd.alone(at(25), window));
        assert!(crowd.alone(at(30), window));
        // A reading older than that isn't taken late: it told when it was
        // made.
        assert!(!crowd.alone(at(8), window));
    }
}
