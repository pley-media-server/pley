// SPDX-License-Identifier: AGPL-3.0-only
//! How transcodes that tone map with libplacebo share the GPU.
//!
//! Each process tone maps at the best quality the GPU keeps ahead of real
//! time with what the other jobs tone mapping on it need (see
//! [`crate::plan::tonemap_quality`]). Where even the fastest falls behind
//! and the GPU's video engine decodes as well as encodes (AMD's and
//! Intel's do both, NVIDIA's are apart), the video is decoded by the CPU
//! instead, while it has the cores to spare besides those other programs
//! keep busy: its frames come to memory for libplacebo either way. On a
//! Ryzen 5 5600G two 4K films went from 1.0× to 1.2× real time each
//! (decoding 4K HEVC and encoding 4K H.264 at once is more than its
//! Radeon's engine can do for two); made 1080p, which its Radeon keeps
//! ahead, they gained nothing, so a GPU that keeps up decodes, and the CPU
//! stays free.
//!
//! What the start-up measurement predicts is checked against what the
//! jobs make: one about to keep its player waiting teaches a lesson
//! ([`Lessons`]), that the GPU fell behind (a cheaper quality, then the
//! CPU decodes) or that the CPU had no time to spare (the GPU decodes),
//! which holds while the GPU has as much to do. Another program using the
//! GPU or the CPU, or an FFmpeg build that copies frames slower, is
//! measured that way. A job another one tone mapping alike (see [`others`])
//! leaves far behind waits for something of its own (its file's hard disk,
//! read by another program too): it teaches nothing.
//!
//! A running job moves at once when another starts using the GPU, or a
//! lesson calls for it, between two segments, and back to more work for
//! the GPU once it is left alone and well ahead of its player.

use std::time::{Duration, Instant};

use pley_transcoder_api::TonemapQuality;

/// Cores the CPU takes to decode 4K HEVC at the speed a 24 fps film is
/// transcoded at with two on the GPU, 1.2× real time: 3.6 to 4.8 measured
/// on a Ryzen 5 5600G, with the rest of FFmpeg's work.
const DECODE_CORES_4K: f64 = 4.0;
/// Cores left to the rest: audio, the server, the system.
const CORES_KEPT: f64 = 2.0;
/// A move that gives the GPU more to do (its job is left alone on it) waits
/// until the process has run this long, so that jobs coming and going don't
/// restart it each time.
pub const MORE_AFTER: Duration = Duration::from_secs(60);
/// ... and its job is this far ahead of the player (seconds of video), so
/// that the restart can't make it wait.
pub const MORE_LEAD_SECS: f64 = 30.0;
/// A process is judged by its speed once it has run this long: past its
/// start, with its speed measured over the last 20 s.
pub const JUDGE_AFTER: Duration = Duration::from_secs(30);
/// Under this speed (× real time), with less than [`LITTLE_LEAD_SECS`]
/// ready past what its player asked for, a job is about to keep the player
/// waiting.
const FALLS_BEHIND: f64 = 1.05;
const LITTLE_LEAD_SECS: f64 = 8.0;
/// The share of the CPU's time busy above which it has none to spare.
const CPU_SATURATED: f64 = 0.9;
/// Another job tone mapping alike ([`others`]) getting this many times as
/// much of its work done is held back by neither: jobs the GPU holds back
/// slow down together (on a Ryzen 5 5600G, two 4K films decoded on the GPU
/// beside a third made 0.70× real time each), while one whose file shares a
/// hard disk with another program's reads made 0.3× real time beside one
/// making 2.3×.
const STARVED: f64 = 2.0;
/// How long a lesson holds after it was last confirmed: then what was
/// measured at start-up is tried again.
const LESSON_FOR: Duration = Duration::from_secs(600);
/// A lesson holds while the GPU has this much of the load it was learned
/// with, or more.
const SAME_LOAD: f64 = 0.9;

/// What one process does on the GPU, besides its plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Share {
    pub quality: TonemapQuality,
    /// The video is decoded by the CPU rather than the GPU planned.
    pub cpu_decode: bool,
    /// Quick Sync's scaler tone maps rather than libplacebo
    /// ([`pley_transcoder_api::Tonemapper::QuickSync`]): where the GPU falls
    /// behind with jobs it decodes and scales.
    pub quick_sync: bool,
}

impl Share {
    /// Whether a move from this to `to` happens at once: it gives the GPU
    /// less to do (a cheaper quality, the decoding moved to the CPU, or the
    /// tone mapping to Quick Sync's scaler), or
    /// the CPU, where it was found with no time to spare (`cpu_busy`, see
    /// [`Lessons`]).
    pub fn moves_at_once(self, to: Share, cpu_busy: bool) -> bool {
        to.quality < self.quality
            || (to.cpu_decode && !self.cpu_decode)
            || (to.quick_sync && !self.quick_sync)
            || (cpu_busy && self.cpu_decode && !to.cpu_decode)
    }
}

/// The cores decoding `pixels` a frame at `fps` frames a second takes.
pub fn decode_cores(pixels: u64, fps: f64) -> f64 {
    DECODE_CORES_4K * (pixels as f64 / (3840.0 * 2160.0)) * (fps / 24.0)
}

/// The GPU a job would decode on: see [`cpu_decodes`].
#[derive(Debug, Clone, Copy)]
pub struct Gpu {
    /// It decodes on the video engine it encodes with.
    pub engine_shared: bool,
    /// It falls behind the jobs tone mapping on it, this one included, even
    /// at the fastest quality ([`crate::plan::tonemap_falls_behind`], or a
    /// lesson).
    pub behind: bool,
}

/// What the CPU is doing besides, and what it has: see [`cpu_decodes`].
#[derive(Debug, Clone, Copy)]
pub struct Cpu {
    pub cores: u32,
    /// What the other processes decoding on it take ([`decode_cores`]).
    pub decoding: f64,
    /// Software encodes running: x264 and x265 take what cores there are.
    pub software_encodes: u32,
    /// The cores other programs keep busy ([`crate::cpuload`]).
    pub foreign: f64,
}

/// Whether a job decodes on the CPU rather than on `gpu`, taking `cores`
/// ([`decode_cores`]).
pub fn cpu_decodes(gpu: Gpu, cpu: Cpu, cores: f64) -> bool {
    gpu.engine_shared
        && gpu.behind
        && cpu.software_encodes == 0
        && f64::from(cpu.cores) - CORES_KEPT - cpu.foreign >= cpu.decoding + cores
}

/// How a job tone mapping on the GPU goes, measured.
#[derive(Debug, Clone, Copy)]
pub struct Measured {
    /// Its process's speed (× real time) lately.
    pub speed: f64,
    /// Seconds of video ready past what its player asked for.
    pub lead_secs: f64,
    /// How busy the CPU was lately (0 to 1), where known.
    pub cpu_busy: Option<f64>,
    /// The most another job tone mapping alike gets done meanwhile, as a
    /// multiple of this one ([`others`]).
    pub others: f64,
}

/// The most the other jobs tone mapping alike with the `i`th of `jobs` get
/// done, as a multiple of what it does: each job its [`Share`] and its work,
/// its speed times what it needs of the GPU at real time
/// ([`crate::plan::Plan::tonemap_need`]). Alike is the same share: the same
/// quality, decoding on the same side. A job the CPU decodes for leaves the
/// GPU's video engine to the others' decoding, and gets more done than they
/// do for that alone (up to twice as much beside two 4K films on a Ryzen 5
/// 5600G).
pub fn others(jobs: &[(Share, f64)], i: usize) -> f64 {
    let (share, work) = jobs[i];
    let alike = jobs.iter().enumerate().filter(|&(k, &(s, _))| k != i && s == share);
    let most = alike.map(|(_, &(_, w))| w).fold(0.0, f64::max);
    // One making nothing beside one making some is infinitely far behind.
    if most > 0.0 { most / work } else { 0.0 }
}

/// What a job's measured speed says about the GPU and the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// It keeps ahead of its player.
    KeepsUp,
    /// It is about to keep its player waiting, while another job tone
    /// mapping alike gets far more done ([`STARVED`]): what holds it back
    /// is its own (its file's disk, a network share), which neither a
    /// cheaper quality nor the CPU decoding would change.
    Starved,
    /// It is about to keep its player waiting while the CPU has time to
    /// spare: the GPU holds it back.
    GpuBound,
    /// The same with the CPU busy all the time.
    CpuBound,
}

pub fn verdict(m: Measured) -> Verdict {
    if m.lead_secs >= LITTLE_LEAD_SECS || m.speed >= FALLS_BEHIND {
        Verdict::KeepsUp
    } else if m.others >= STARVED {
        Verdict::Starved
    } else if m.cpu_busy.is_some_and(|busy| busy >= CPU_SATURATED) {
        Verdict::CpuBound
    } else {
        Verdict::GpuBound
    }
}

/// When a lesson was learned, last confirmed, and with how much on the GPU.
#[derive(Debug, Clone, Copy)]
struct Learned {
    /// What the jobs tone mapping on the GPU needed in all
    /// ([`crate::plan::Plan::tonemap_need`]).
    load: f64,
    at: Instant,
}

impl Learned {
    fn holds(self, load: f64, now: Instant) -> bool {
        load >= self.load * SAME_LOAD && now.saturating_duration_since(self.at) < LESSON_FOR
    }
}

/// What jobs falling behind their players taught (see [`verdict`]): the
/// GPU fell behind at a quality, or the CPU had no time to spare. Each
/// holds while the GPU has about as much to tone map or more, for
/// [`LESSON_FOR`] after it was last confirmed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Lessons {
    gpu: Option<(Learned, TonemapQuality)>,
    cpu: Option<Learned>,
}

impl Lessons {
    /// Learns from a job at `share` with `load` on the GPU in all; whether
    /// it taught something new, rather than confirming what holds.
    pub fn learn(&mut self, verdict: Verdict, share: Share, load: f64, now: Instant) -> bool {
        let known = |l: Learned| l.holds(load, now).then_some(l);
        match verdict {
            Verdict::KeepsUp | Verdict::Starved => false,
            Verdict::GpuBound => {
                let held = self.gpu.and_then(|(l, q)| Some((known(l)?, q)));
                let quality = held.map_or(share.quality, |(_, q)| q.min(share.quality));
                let load = held.map_or(load, |(l, _)| l.load.min(load));
                self.gpu = Some((Learned { load, at: now }, quality));
                held.is_none_or(|(_, q)| quality < q)
            }
            Verdict::CpuBound => {
                let held = self.cpu.and_then(known);
                self.cpu = Some(Learned { load: held.map_or(load, |l| l.load.min(load)), at: now });
                held.is_none()
            }
        }
    }

    /// What holds with `load` on the GPU.
    pub fn holding(&self, load: f64, now: Instant) -> Held {
        Held {
            fell_behind_at: self.gpu.filter(|(l, _)| l.holds(load, now)).map(|(_, q)| q),
            cpu_busy: self.cpu.is_some_and(|l| l.holds(load, now)),
        }
    }
}

/// The lessons that hold for a decision (see [`Lessons::holding`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    /// The GPU fell behind at this quality.
    pub fell_behind_at: Option<TonemapQuality>,
    /// The CPU had no time to spare: it decodes no more.
    pub cpu_busy: bool,
}

impl Held {
    /// `quality`, or a cheaper one than the GPU fell behind at.
    pub fn cap(self, quality: TonemapQuality) -> TonemapQuality {
        match self.fell_behind_at {
            Some(TonemapQuality::High) => quality.min(TonemapQuality::Balanced),
            Some(_) => TonemapQuality::Fast,
            None => quality,
        }
    }

    /// Whether the GPU fell behind even at the fastest quality.
    pub fn gpu_behind(self) -> bool {
        self.fell_behind_at == Some(TonemapQuality::Fast)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{tonemap_falls_behind, tonemap_need};
    use pley_transcoder_api::TonemapSpeed;

    const UHD: u64 = 3840 * 2160;
    const HD: u64 = 1920 * 1080;

    fn cpu(cores: u32, decoding: f64) -> Cpu {
        Cpu { cores, decoding, software_encodes: 0, foreign: 0.0 }
    }

    /// AMD's or Intel's, falling behind.
    const BEHIND: Gpu = Gpu { engine_shared: true, behind: true };

    #[test]
    fn the_cpu_decodes_for_a_gpu_behind_while_it_has_the_cores() {
        let film = decode_cores(UHD, 23.976);
        // Two films on a 6-core, 12-thread Ryzen, not a third.
        assert!(cpu_decodes(BEHIND, cpu(12, 0.0), film));
        assert!(cpu_decodes(BEHIND, cpu(12, film), film));
        assert!(!cpu_decodes(BEHIND, cpu(12, 2.0 * film), film));
        // A 4-core, 8-thread processor takes one; a 4-thread one none.
        assert!(cpu_decodes(BEHIND, cpu(8, 0.0), film));
        assert!(!cpu_decodes(BEHIND, cpu(8, film), film));
        assert!(!cpu_decodes(BEHIND, cpu(4, 0.0), film));
        // 1080p takes a quarter: many fit, even on 4 threads.
        let hd = decode_cores(HD, 24.0);
        assert!(cpu_decodes(BEHIND, cpu(8, 3.0 * hd), hd));
        assert!(cpu_decodes(BEHIND, cpu(4, 0.0), hd));
        // Cores other programs keep busy aren't to spare.
        assert!(!cpu_decodes(BEHIND, Cpu { foreign: 3.0, ..cpu(12, film) }, film));
        assert!(cpu_decodes(BEHIND, Cpu { foreign: 1.5, ..cpu(12, film) }, film));
        // A GPU that keeps up decodes.
        assert!(!cpu_decodes(Gpu { behind: false, ..BEHIND }, cpu(12, 0.0), film));
        // NVIDIA decodes apart from its encoder; software encodes take the cores.
        assert!(!cpu_decodes(Gpu { engine_shared: false, ..BEHIND }, cpu(12, 0.0), film));
        assert!(!cpu_decodes(BEHIND, Cpu { software_encodes: 1, ..cpu(12, 0.0) }, film));
    }

    /// Whether the `n`th of `n` 4K HDR films made `out` pixels decodes on
    /// the CPU of `cores` threads, with libplacebo measured at `speed`, the
    /// others decoding where they would.
    fn nth_on_cpu(speed: &TonemapSpeed, engine_shared: bool, cores: u32, out: u64, n: u32) -> bool {
        let need = tonemap_need(UHD, out, 23.976);
        let gpu = Gpu { engine_shared, behind: tonemap_falls_behind(speed, need * f64::from(n)) };
        let film = decode_cores(UHD, 23.976);
        let (mut decoding, mut on_cpu) = (0.0, false);
        for _ in 0..n {
            on_cpu = cpu_decodes(gpu, cpu(cores, decoding), film);
            if on_cpu {
                decoding += film;
            }
        }
        on_cpu
    }

    #[test]
    fn each_computer_decodes_where_it_goes_fastest() {
        // An integrated Radeon (Vega 7 of a Ryzen 5 5600G): one 4K film, or
        // two made 1080p, keep ahead; two 4K films fall behind, and the CPU
        // decodes both, not a third.
        let apu = TonemapSpeed { high: 36.6, balanced: 55.9, fast: 84.0 };
        assert!(!nth_on_cpu(&apu, true, 12, UHD, 1));
        assert!(!nth_on_cpu(&apu, true, 12, HD, 2));
        assert!(nth_on_cpu(&apu, true, 12, UHD, 2));
        assert!(!nth_on_cpu(&apu, true, 12, UHD, 3));
        // The same Radeon beside 4 threads (a Ryzen 3): the GPU decodes.
        assert!(!nth_on_cpu(&apu, true, 4, UHD, 2));
        // A smaller integrated GPU falls behind with one: the CPU decodes it.
        let small = TonemapSpeed { high: 12.0, balanced: 20.0, fast: 30.0 };
        assert!(nth_on_cpu(&small, true, 8, UHD, 1));
        // A graphics card keeps three ahead: it decodes them all.
        let card = TonemapSpeed { high: 240.0, balanced: 300.0, fast: 320.0 };
        assert!(!nth_on_cpu(&card, true, 16, UHD, 3));
        // NVIDIA's decoder doesn't take from its encoder's time.
        assert!(!nth_on_cpu(&apu, false, 16, UHD, 2));
    }

    fn measured(speed: f64, lead_secs: f64, cpu_busy: Option<f64>) -> Measured {
        Measured { speed, lead_secs, cpu_busy, others: 1.0 }
    }

    #[test]
    fn a_job_about_to_keep_its_player_waiting_tells_what_holds_it_back() {
        assert_eq!(verdict(measured(1.2, 2.0, Some(0.5))), Verdict::KeepsUp);
        assert_eq!(verdict(measured(0.9, 2.0, Some(0.5))), Verdict::GpuBound);
        assert_eq!(verdict(measured(0.9, 2.0, None)), Verdict::GpuBound, "the CPU not measured");
        assert_eq!(verdict(measured(0.9, 2.0, Some(0.95))), Verdict::CpuBound);
        // Far enough ahead (its read rate holding it to real time): fine.
        assert_eq!(verdict(measured(1.0, 60.0, Some(0.95))), Verdict::KeepsUp);
        // Measured: two 4K films made 1080p, one waiting for its hard disk
        // (0.31× real time) beside one making 2.27×: neither the GPU nor
        // the CPU holds it back, and it teaches nothing.
        let starved = Measured { others: 2.27 / 0.31, ..measured(0.31, 0.0, Some(0.22)) };
        assert_eq!(verdict(starved), Verdict::Starved);
        let mut lessons = Lessons::default();
        let fast = Share { quality: TonemapQuality::Fast, cpu_decode: false, quick_sync: false };
        assert!(!lessons.learn(Verdict::Starved, fast, 67.0, Instant::now()));
        assert_eq!(lessons.holding(67.0, Instant::now()), Held::default());
        // Three 4K films the GPU holds back: 0.47× beside 0.85×, together.
        let together = Measured { others: 0.85 / 0.47, ..measured(0.47, 0.0, Some(0.4)) };
        assert_eq!(verdict(together), Verdict::GpuBound);
    }

    #[test]
    fn only_jobs_tone_mapping_alike_tell_that_one_waits_for_its_input() {
        let gpu = Share { quality: TonemapQuality::Fast, cpu_decode: false, quick_sync: false };
        let cpu = Share { cpu_decode: true, ..gpu };
        let balanced = Share { quality: TonemapQuality::Balanced, ..gpu };
        // Three 4K films as measured: two decoded on the GPU, at 0.56× and
        // about 0.7× real time, beside one the CPU decodes for, at twice the
        // first: the GPU holds back the two, and the third doesn't count.
        let three = [(gpu, 0.56), (gpu, 0.70), (cpu, 1.12)];
        assert!((others(&three, 0) - 0.70 / 0.56).abs() < 1e-9);
        assert_eq!(others(&three, 2), 0.0, "none alike");
        let behind = Measured { others: others(&three, 0), ..measured(0.56, 0.0, Some(0.75)) };
        assert_eq!(verdict(behind), Verdict::GpuBound);
        // One alike far ahead: what holds this one back is its own.
        let two = [(gpu, 0.31), (gpu, 2.27), (balanced, 0.1)];
        let starved = Measured { others: others(&two, 0), ..measured(0.31, 0.0, Some(0.22)) };
        assert_eq!(verdict(starved), Verdict::Starved);
        // Making nothing beside one alike making some; alone; or none making any.
        assert_eq!(others(&[(gpu, 0.0), (gpu, 0.5)], 0), f64::INFINITY);
        assert_eq!(others(&[(gpu, 0.5)], 0), 0.0);
        assert_eq!(others(&[(gpu, 0.0), (gpu, 0.0)], 0), 0.0);
    }

    #[test]
    fn a_gpu_that_fell_behind_takes_a_cheaper_quality_then_leaves_the_decoding_to_the_cpu() {
        let now = Instant::now();
        let mut lessons = Lessons::default();
        let balanced = Share { quality: TonemapQuality::Balanced, cpu_decode: false, quick_sync: false };
        assert!(!lessons.learn(Verdict::KeepsUp, balanced, 100.0, now));
        assert_eq!(lessons.holding(100.0, now), Held::default());

        assert!(lessons.learn(Verdict::GpuBound, balanced, 100.0, now));
        let held = lessons.holding(100.0, now);
        assert_eq!(held.cap(TonemapQuality::High), TonemapQuality::Fast);
        assert!(!held.gpu_behind(), "fast may do");
        // Confirmed: nothing new.
        assert!(!lessons.learn(Verdict::GpuBound, balanced, 100.0, now));
        // Fast too.
        let fast = Share { quality: TonemapQuality::Fast, ..balanced };
        assert!(lessons.learn(Verdict::GpuBound, fast, 100.0, now));
        assert!(lessons.holding(100.0, now).gpu_behind());

        // As much on the GPU, or more: it holds; a job less, or ten minutes on, not.
        assert!(lessons.holding(95.0, now).gpu_behind());
        assert!(lessons.holding(150.0, now).gpu_behind());
        assert_eq!(lessons.holding(50.0, now), Held::default());
        assert_eq!(lessons.holding(100.0, now + LESSON_FOR), Held::default());
        // Falling behind at High leaves Balanced.
        let mut lessons = Lessons::default();
        lessons.learn(Verdict::GpuBound, Share { quality: TonemapQuality::High, ..balanced }, 100.0, now);
        assert_eq!(lessons.holding(100.0, now).cap(TonemapQuality::High), TonemapQuality::Balanced);
        assert_eq!(lessons.holding(100.0, now).cap(TonemapQuality::Fast), TonemapQuality::Fast);
    }

    #[test]
    fn a_cpu_with_no_time_to_spare_decodes_no_more() {
        let now = Instant::now();
        let mut lessons = Lessons::default();
        let decoding = Share { quality: TonemapQuality::Fast, cpu_decode: true, quick_sync: false };
        assert!(lessons.learn(Verdict::CpuBound, decoding, 100.0, now));
        assert!(!lessons.learn(Verdict::CpuBound, decoding, 120.0, now + Duration::from_secs(5)));
        let held = lessons.holding(100.0, now + Duration::from_secs(5));
        assert!(held.cpu_busy);
        assert_eq!(held.fell_behind_at, None);
        // Confirmed lately: it holds ten minutes from then.
        assert!(lessons.holding(100.0, now + LESSON_FOR).cpu_busy);
        assert!(!lessons.holding(80.0, now).cpu_busy);
    }

    #[test]
    fn moves_relieving_the_gpu_or_a_busy_cpu_happen_at_once() {
        let alone = Share { quality: TonemapQuality::Balanced, cpu_decode: false, quick_sync: false };
        let shared = Share { quality: TonemapQuality::Fast, cpu_decode: true, quick_sync: false };
        assert!(alone.moves_at_once(shared, false));
        assert!(alone.moves_at_once(Share { quality: TonemapQuality::Fast, ..alone }, false));
        assert!(alone.moves_at_once(Share { cpu_decode: true, ..alone }, false));
        assert!(!shared.moves_at_once(alone, false));
        assert!(!shared.moves_at_once(shared, false));
        // Back to the GPU at once where the CPU had no time to spare.
        assert!(shared.moves_at_once(Share { cpu_decode: false, ..shared }, true));
        // To Quick Sync's tone mapping at once, back to libplacebo later.
        let fast = Share { quality: TonemapQuality::Fast, cpu_decode: false, quick_sync: false };
        let intel = Share { quick_sync: true, ..fast };
        assert!(fast.moves_at_once(intel, false));
        assert!(!intel.moves_at_once(fast, false));
    }
}
