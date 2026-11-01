// SPDX-License-Identifier: AGPL-3.0-only
//! Process-wide counters, exposed as JSON (`/v1/status`) and in the Prometheus
//! text format (`/metrics`).

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use pley_transcoder_api::{Counters, JobState, TranscoderStatus};

#[derive(Default)]
pub struct Metrics {
    pub jobs_created: AtomicU64,
    pub jobs_failed: AtomicU64,
    pub processes_started: AtomicU64,
    pub process_failures: AtomicU64,
    pub segments_produced: AtomicU64,
    pub segments_served: AtomicU64,
    pub bytes_served: AtomicU64,
    pub probes: AtomicU64,
    pub probe_failures: AtomicU64,
    pub probe_ms_total: AtomicU64,
    pub thumbnails: AtomicU64,
    pub subtitle_extractions: AtomicU64,
    pub keyframe_scans: AtomicU64,
}

impl Metrics {
    pub fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Relaxed);
    }

    pub fn snapshot(&self) -> Counters {
        Counters {
            jobs_created: self.jobs_created.load(Relaxed),
            jobs_failed: self.jobs_failed.load(Relaxed),
            processes_started: self.processes_started.load(Relaxed),
            process_failures: self.process_failures.load(Relaxed),
            segments_produced: self.segments_produced.load(Relaxed),
            segments_served: self.segments_served.load(Relaxed),
            bytes_served: self.bytes_served.load(Relaxed),
            probes: self.probes.load(Relaxed),
            probe_failures: self.probe_failures.load(Relaxed),
            probe_ms_total: self.probe_ms_total.load(Relaxed),
            thumbnails: self.thumbnails.load(Relaxed),
            subtitle_extractions: self.subtitle_extractions.load(Relaxed),
            keyframe_scans: self.keyframe_scans.load(Relaxed),
        }
    }
}

/// Render a status snapshot in the Prometheus exposition format.
pub fn render_prometheus(status: &TranscoderStatus) -> String {
    let mut out = String::with_capacity(4096);
    let c = &status.counters;
    let counters: [(&str, &str, u64); 13] = [
        ("jobs_created_total", "Transcode jobs created", c.jobs_created),
        ("jobs_failed_total", "Transcode jobs whose ffmpeg process failed", c.jobs_failed),
        ("processes_started_total", "ffmpeg transcode processes started", c.processes_started),
        ("process_failures_total", "ffmpeg transcode processes that exited with an error", c.process_failures),
        ("segments_produced_total", "HLS segments produced", c.segments_produced),
        ("segments_served_total", "HLS segments served", c.segments_served),
        ("bytes_served_total", "Bytes of HLS segments served", c.bytes_served),
        ("probes_total", "Media probes", c.probes),
        ("probe_failures_total", "Failed media probes", c.probe_failures),
        ("probe_milliseconds_total", "Time spent probing media", c.probe_ms_total),
        ("thumbnails_total", "Thumbnails extracted", c.thumbnails),
        ("subtitle_extractions_total", "Subtitle extractions", c.subtitle_extractions),
        ("keyframe_scans_total", "Keyframe index scans", c.keyframe_scans),
    ];
    for (name, help, value) in counters {
        let _ = writeln!(out, "# HELP pley_transcoder_{name} {help}");
        let _ = writeln!(out, "# TYPE pley_transcoder_{name} counter");
        let _ = writeln!(out, "pley_transcoder_{name} {value}");
    }
    let gauges: [(&str, &str, f64); 7] = [
        ("uptime_seconds", "Seconds since the transcoder started", status.uptime_secs as f64),
        ("running_processes", "ffmpeg transcode processes currently running", status.running_processes as f64),
        ("hardware_processes", "Running processes using a hardware encoder", status.hw_processes as f64),
        ("max_jobs", "Maximum concurrent transcode processes", status.max_jobs as f64),
        ("jobs", "Transcode jobs (sessions) currently known", status.jobs.len() as f64),
        ("work_dir_bytes", "Disk space used by segments and caches", status.work_dir_bytes as f64),
        ("background_paused", "Background ffmpeg runs paused for playback", status.background_paused as f64),
    ];
    for (name, help, value) in gauges {
        let _ = writeln!(out, "# HELP pley_transcoder_{name} {help}");
        let _ = writeln!(out, "# TYPE pley_transcoder_{name} gauge");
        let _ = writeln!(out, "pley_transcoder_{name} {value}");
    }

    let _ = writeln!(out, "# HELP pley_transcoder_job_speed Realtime multiplier of a running job");
    let _ = writeln!(out, "# TYPE pley_transcoder_job_speed gauge");
    for job in &status.jobs {
        if let Some(speed) = job.process.as_ref().and_then(|p| p.speed) {
            let _ = writeln!(out, "pley_transcoder_job_speed{{job=\"{}\"}} {speed}", job.id);
        }
    }
    let _ = writeln!(out, "# HELP pley_transcoder_job_fps Frames per second of a running job");
    let _ = writeln!(out, "# TYPE pley_transcoder_job_fps gauge");
    for job in &status.jobs {
        if let Some(fps) = job.process.as_ref().and_then(|p| p.fps) {
            let _ = writeln!(out, "pley_transcoder_job_fps{{job=\"{}\"}} {fps}", job.id);
        }
    }
    let _ = writeln!(out, "# HELP pley_transcoder_jobs_by_state Jobs per state");
    let _ = writeln!(out, "# TYPE pley_transcoder_jobs_by_state gauge");
    for state in [JobState::Starting, JobState::Running, JobState::Idle, JobState::Finished, JobState::Failed] {
        let count = status.jobs.iter().filter(|j| j.state == state).count();
        let label = serde_json_state(state);
        let _ = writeln!(out, "pley_transcoder_jobs_by_state{{state=\"{label}\"}} {count}");
    }
    out
}

fn serde_json_state(state: JobState) -> &'static str {
    match state {
        JobState::Starting => "starting",
        JobState::Running => "running",
        JobState::Idle => "idle",
        JobState::Finished => "finished",
        JobState::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prometheus_output_contains_counters() {
        let status = TranscoderStatus { counters: Counters { probes: 7, ..Default::default() }, ..Default::default() };
        let text = render_prometheus(&status);
        assert!(text.contains("pley_transcoder_probes_total 7"));
        assert!(text.contains("# TYPE pley_transcoder_running_processes gauge"));
    }
}
