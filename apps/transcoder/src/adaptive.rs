// SPDX-License-Identifier: AGPL-3.0-only
//! The automatic preset for software encodes (`auto`): x264's and x265's
//! presets trade encoding speed for picture quality at the same bitrate.
//!
//! A job starts at the slowest preset the computer keeps faster than real
//! time for the transcodes running (more of them, fewer cores or bigger
//! pictures: faster), one step faster still at a start or a seek, so the
//! first segments come quickly. While it plays it moves a step faster when
//! the player is about to wait for it, and a step slower, for better
//! pictures, once it is well ahead with speed to spare. Each move restarts
//! ffmpeg where it was, between two segments, so playback never notices.

use std::time::Duration;

/// Presets from the fastest to the slowest used.
pub const LADDER: [&str; 6] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium"];

/// The preset setting that asks for this.
pub const AUTO: &str = "auto";

/// Below this much video ready past what the player asked for (seconds), a
/// job that isn't comfortably faster than real time moves to a faster preset.
const SHORT_LEAD_SECS: f64 = 8.0;
/// Above it, a job fast enough moves back toward its target.
const LONG_LEAD_SECS: f64 = 45.0;
/// ffmpeg's speed (× real time) under which a short lead means buffering soon.
const TOO_SLOW: f64 = 1.25;
/// The speed a job needs to move to a slower preset.
const FAST_ENOUGH: f64 = 1.6;
/// The least time between two moves of a job: its speed settles first.
const SPEED_UP_AFTER: Duration = Duration::from_secs(10);
const SLOW_DOWN_AFTER: Duration = Duration::from_secs(40);

/// What a job's preset depends on besides the job.
#[derive(Debug, Clone, Copy)]
pub struct Load {
    /// Software encodes running, this one included.
    pub software_jobs: u32,
    pub cores: u32,
    /// The output picture's pixels (1920 × 1080 for 1080p).
    pub pixels: u64,
    /// HEVC (x265) takes a few times x264's work.
    pub hevc: bool,
}

/// The slowest preset (index in [`LADDER`]) this load keeps faster than real time.
pub fn target(load: Load) -> usize {
    let size = (load.pixels as f64 / (1920.0 * 1080.0)).max(0.25);
    let work = size * if load.hevc { 3.0 } else { 1.0 };
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

/// The preset a process starts with: a step faster than the target when
/// nothing is ready ahead of the player (a start or a seek).
pub fn start(target: usize, lead_secs: f64) -> usize {
    if lead_secs <= 0.0 { target.saturating_sub(1) } else { target }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Faster,
    Slower,
    Stay,
}

/// What a running job should do now, at preset `current` with `target` the
/// load's, `lead_secs` of video ready past what the player asked for, at
/// `speed` × real time, `since_move` after its last start or move.
pub fn decide(current: usize, target: usize, lead_secs: f64, speed: Option<f64>, since_move: Duration) -> Move {
    let speed = speed.unwrap_or(1.0);
    if current > 0 && lead_secs < SHORT_LEAD_SECS && speed < TOO_SLOW && since_move >= SPEED_UP_AFTER {
        return Move::Faster;
    }
    if current < target && lead_secs > LONG_LEAD_SECS && speed >= FAST_ENOUGH && since_move >= SLOW_DOWN_AFTER {
        return Move::Slower;
    }
    Move::Stay
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: u64 = 1920 * 1080;

    fn load(software_jobs: u32, cores: u32, pixels: u64) -> Load {
        Load { software_jobs, cores, pixels, hevc: false }
    }

    #[test]
    fn the_target_follows_the_load() {
        assert_eq!(LADDER[target(load(1, 16, HD))], "medium");
        assert_eq!(LADDER[target(load(1, 8, HD))], "fast");
        assert_eq!(LADDER[target(load(2, 8, HD))], "veryfast");
        assert_eq!(LADDER[target(load(4, 8, HD))], "superfast");
        assert_eq!(LADDER[target(load(8, 8, HD))], "ultrafast");
        // Smaller pictures leave room; 4K and HEVC take more.
        assert_eq!(LADDER[target(load(2, 8, 1280 * 720))], "fast");
        assert_eq!(LADDER[target(load(1, 8, 3840 * 2160))], "superfast");
        assert_eq!(LADDER[target(Load { hevc: true, ..load(1, 8, HD) })], "superfast");
    }

    #[test]
    fn starts_and_seeks_go_a_step_faster() {
        assert_eq!(start(3, 0.0), 2);
        assert_eq!(start(0, 0.0), 0);
        assert_eq!(start(3, 20.0), 3);
    }

    #[test]
    fn a_job_about_to_keep_the_player_waiting_speeds_up() {
        let settled = Duration::from_secs(30);
        assert_eq!(decide(3, 3, 4.0, Some(0.9), settled), Move::Faster);
        assert_eq!(decide(3, 3, 4.0, Some(2.0), settled), Move::Stay, "fast enough to catch up");
        assert_eq!(decide(3, 3, 20.0, Some(0.9), settled), Move::Stay, "still ahead");
        assert_eq!(decide(3, 3, 4.0, Some(0.9), Duration::from_secs(3)), Move::Stay, "just moved");
        assert_eq!(decide(0, 3, 0.0, Some(0.5), settled), Move::Stay, "already the fastest");
    }

    #[test]
    fn a_job_well_ahead_goes_back_to_better_pictures() {
        let settled = Duration::from_secs(60);
        assert_eq!(decide(1, 3, 60.0, Some(2.0), settled), Move::Slower);
        assert_eq!(decide(3, 3, 60.0, Some(2.0), settled), Move::Stay, "at the target");
        assert_eq!(decide(1, 3, 60.0, Some(1.1), settled), Move::Stay, "no speed to spare");
        assert_eq!(decide(1, 3, 60.0, Some(2.0), Duration::from_secs(20)), Move::Stay, "too soon");
    }
}
