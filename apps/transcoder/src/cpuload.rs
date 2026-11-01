// SPDX-License-Identifier: AGPL-3.0-only
//! How busy the processors the transcoder may use are, and how much of them
//! other programs take: the share of their time that went to any work, and
//! the cores other programs' work kept from this transcoder and its FFmpeg
//! processes, between two looks. Windows: `GetSystemTimes`, and the job
//! object the transcoder's processes share (see
//! [`crate::util::kill_children_on_exit`]). Linux: `/proc`, of the
//! processors the transcoder may run on (a container's `--cpuset-cpus`),
//! and the CPU limit of its control group or of one above it (a container's
//! `--cpus`, systemd's `CPUQuota=`), whichever leaves it less. Elsewhere
//! unknown.

#[cfg(any(target_os = "linux", test))]
use std::path::{Path, PathBuf};
use std::time::Instant;

use parking_lot::Mutex;

/// Processor time so far, in seconds.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct Times {
    /// The processors' time busy, and in all: of those the transcoder may
    /// run on (Linux), or of all of them (Windows, whose shares stand for
    /// those it may run on).
    busy: f64,
    total: f64,
    /// How many processors the transcoder may run on.
    cpus: f64,
    /// What this process and its children took; `None` where unknown.
    ours: Option<f64>,
    /// The CPU limit of its control group, where it has one.
    limit: Option<Limit>,
}

/// A control group's CPU limit: the cores' time its processes may take, and
/// the time they took so far, by `at`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct Limit {
    cores: f64,
    used: f64,
    at: Instant,
}

/// How the processors went between two looks.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Shares {
    /// The share of their time that went to work, 0 to 1.
    busy: f64,
    /// The cores other programs' work kept from the transcoder.
    foreign: Option<f64>,
}

/// Looks at the processors' time now and then (the reaper's turn, every few
/// seconds), and keeps how busy they were lately.
#[derive(Debug, Default)]
pub struct CpuMeter {
    /// The last look, and the shares so far.
    state: Mutex<(Option<Times>, Option<Shares>)>,
}

impl CpuMeter {
    pub fn sample(&self) {
        if let Some(now) = read() {
            self.record(now);
        }
    }

    /// Each look weighs as much as all those before it together, so that the
    /// shares follow the last two or three (10 s or so).
    fn record(&self, now: Times) {
        let mut state = self.state.lock();
        if let Some(new) = state.0.and_then(|last| shares_between(last, now)) {
            let mean = |old: Option<f64>, new: Option<f64>| match (old, new) {
                (Some(old), Some(new)) => Some((old + new) / 2.0),
                (old, new) => old.or(new),
            };
            state.1 = Some(state.1.map_or(new, |old| Shares {
                busy: (old.busy + new.busy) / 2.0,
                foreign: mean(old.foreign, new.foreign),
            }));
        }
        state.0 = Some(now);
    }

    /// The share of the processors' time that went to work lately, 0 to 1
    /// (under a CPU limit, of the limit where that is the more); `None`
    /// before two looks, or where it can't be measured.
    pub fn busy(&self) -> Option<f64> {
        self.state.lock().1.map(|s| s.busy)
    }

    /// The cores other programs' work kept from the transcoder lately: not
    /// its own work, nor its FFmpeg processes'.
    pub fn foreign(&self) -> Option<f64> {
        self.state.lock().1.and_then(|s| s.foreign)
    }

    /// The cores the transcoder may use, as of the last look: the processors
    /// it may run on, or its control group's CPU limit where that is lower.
    /// `None` before a look, and where it can't look (macOS).
    pub fn room(&self) -> Option<f64> {
        self.state.lock().0.map(|t| t.limit.map_or(t.cpus, |l| l.cores.min(t.cpus)))
    }

    /// Its control group's CPU limit in cores, as of the last look, where
    /// that leaves fewer than the processors it may run on: those FFmpeg
    /// counts, which don't see the limit. Linux only.
    pub fn quota(&self) -> Option<f64> {
        self.state.lock().0.and_then(|t| t.limit.filter(|l| l.cores < t.cpus).map(|l| l.cores))
    }
}

/// A bound on the processors' time the transcoder can have: the cores, the
/// share of them that went to work, and the cores other programs' work took.
#[derive(Debug, Clone, Copy)]
struct Bound {
    cores: f64,
    busy: f64,
    others: Option<f64>,
}

fn shares_between(a: Times, b: Times) -> Option<Shares> {
    let (busy, total) = (b.busy - a.busy, b.total - a.total);
    // A counter that went back (a wrap, a new boot), or other processors:
    // no shares.
    if total <= 0.0 || busy < 0.0 || a.cpus != b.cpus {
        return None;
    }
    let ours = a.ours.zip(b.ours).map(|(a, b)| b - a).filter(|&t| t >= 0.0);
    let cpus = Bound { cores: b.cpus, busy: busy / total, others: ours.map(|ours| (busy - ours) / total * b.cpus) };
    let limit = a.limit.zip(b.limit).and_then(|(x, y)| {
        let (used, wall) = (y.used - x.used, y.at.saturating_duration_since(x.at).as_secs_f64());
        (used >= 0.0 && wall > 0.0).then(|| Bound {
            cores: y.cores,
            busy: used / wall / y.cores,
            others: ours.map(|ours| (used - ours) / wall),
        })
    });
    // The cores the transcoder could have, and those the others leave it:
    // whichever bound leaves less.
    let bounds: Vec<Bound> = [Some(cpus), limit].into_iter().flatten().collect();
    let room = bounds.iter().map(|b| b.cores).fold(f64::INFINITY, f64::min);
    let left = bounds
        .iter()
        .map(|b| b.others.map(|others| b.cores - others.max(0.0)))
        .try_fold(f64::INFINITY, |least, left| left.map(|left| least.min(left)));
    Some(Shares {
        busy: bounds.iter().map(|b| b.busy).fold(0.0, f64::max).clamp(0.0, 1.0),
        foreign: left.map(|left| (room - left).clamp(0.0, room)),
    })
}

#[cfg(windows)]
fn read() -> Option<Times> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::JobObjects::{
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation, QueryInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetSystemTimes;
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut idle, mut kernel, mut user) = (zero, zero, zero);
    // SAFETY: plain Win32 call writing three FILETIMEs of our own.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    // 100 ns.
    let secs = |ticks: u64| ticks as f64 / 1e7;
    let ticks = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    // Kernel time counts the idle time too.
    let total = ticks(kernel) + ticks(user);
    // The job object this process is in, which its FFmpeg processes join
    // as they start: the time all of them took, those ended included, in
    // the same 100 ns.
    // SAFETY: plain Win32 call writing a struct of our own of the size given.
    let ours = unsafe {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
        let ok = QueryInformationJobObject(
            std::ptr::null_mut(),
            JobObjectBasicAccountingInformation,
            (&raw mut info).cast(),
            std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            std::ptr::null_mut(),
        ) != 0;
        ok.then(|| info.TotalUserTime.max(0) as u64 + info.TotalKernelTime.max(0) as u64)
    };
    Some(Times {
        busy: secs(total.saturating_sub(ticks(idle))),
        total: secs(total),
        cpus: f64::from(crate::util::processors()),
        ours: ours.map(secs),
        limit: None,
    })
}

#[cfg(target_os = "linux")]
fn read() -> Option<Times> {
    let allowed = std::fs::read_to_string("/proc/self/status").ok().and_then(|s| allowed_cpus(&s));
    let stat = parse_proc_stat(&std::fs::read_to_string("/proc/stat").ok()?, allowed.as_deref())?;
    // SAFETY: sysconf only reads a constant of the system's.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    let secs = |ticks: u64| ticks as f64 / if hz > 0 { hz as f64 } else { 100.0 };
    Some(Times {
        busy: secs(stat.busy),
        total: secs(stat.total),
        cpus: f64::from(stat.cpus),
        ours: ours().map(secs),
        limit: limit(),
    })
}

#[cfg(not(any(windows, target_os = "linux")))]
fn read() -> Option<Times> {
    None
}

/// The time this process and its children took, in clock ticks: theirs
/// while they run, and its children's once it waited for them.
#[cfg(target_os = "linux")]
fn ours() -> Option<u64> {
    let me = std::process::id();
    let mut ours = 0;
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        if !entry.file_name().to_str().is_some_and(|name| name.bytes().all(|b| b.is_ascii_digit())) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path().join("stat")) else { continue };
        if let Some(stat) = parse_pid_stat(&text).filter(|s| s.pid == me || s.parent == me) {
            ours += stat.time;
        }
    }
    Some(ours)
}

/// The processors `/proc/self/status` lets the process run on: its
/// `Cpus_allowed_list` (`0-3,8`).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn allowed_cpus(status: &str) -> Option<Vec<u32>> {
    let list = status.lines().find_map(|line| line.strip_prefix("Cpus_allowed_list:"))?.trim();
    let mut cpus = Vec::new();
    for part in list.split(',') {
        let (from, to) = part.split_once('-').unwrap_or((part, part));
        let (from, to): (u32, u32) = (from.parse().ok()?, to.parse().ok()?);
        cpus.extend(from..=to.min(from.saturating_add(4096)));
    }
    (!cpus.is_empty()).then_some(cpus)
}

/// Processors' time in `/proc/stat`, in clock ticks.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
struct ProcStat {
    busy: u64,
    total: u64,
    /// The processors counted.
    cpus: u32,
}

/// `/proc/stat`'s `cpuN user nice system idle iowait irq softirq steal …`
/// lines of the processors in `allowed`, where idle, and waiting for the
/// disk, aren't busy. All of them (its first line, `cpu  …`) where that
/// isn't known, or its lines number them otherwise (LXCFS numbers a
/// container's own from 0).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_proc_stat(text: &str, allowed: Option<&[u32]>) -> Option<ProcStat> {
    let ticks = |fields: &str| -> Option<(u64, u64)> {
        let ticks: Vec<u64> = fields.split_whitespace().take(8).map(|f| f.parse().ok()).collect::<Option<_>>()?;
        let total: u64 = ticks.iter().sum();
        let idle = ticks.get(3)? + ticks.get(4).copied().unwrap_or(0);
        Some((total.saturating_sub(idle), total))
    };
    let (mut all, mut cpus, mut mine) = (None, 0, ProcStat { busy: 0, total: 0, cpus: 0 });
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("cpu") else { continue };
        if rest.starts_with(' ') {
            all = ticks(rest);
            continue;
        }
        let Some((cpu, fields)) = rest.split_once(' ') else { continue };
        let Ok(cpu) = cpu.parse::<u32>() else { continue };
        cpus += 1;
        if allowed.is_some_and(|allowed| allowed.contains(&cpu))
            && let Some((busy, total)) = ticks(fields)
        {
            mine = ProcStat { busy: mine.busy + busy, total: mine.total + total, cpus: mine.cpus + 1 };
        }
    }
    match allowed {
        Some(allowed) if mine.cpus > 0 && mine.cpus as usize == allowed.len() => Some(mine),
        _ => all.map(|(busy, total)| ProcStat { busy, total, cpus: cpus.max(1) }),
    }
}

/// A process in `/proc/<pid>/stat`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
struct PidStat {
    pid: u32,
    parent: u32,
    /// Its time, and its children's it waited for, in clock ticks.
    time: u64,
}

/// `pid (name) state ppid … utime stime cutime cstime …`, whose name may
/// hold spaces and parentheses.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_pid_stat(text: &str) -> Option<PidStat> {
    let (pid, rest) = text.split_once(" (")?;
    let fields: Vec<&str> = rest.get(rest.rfind(')')? + 1..)?.split_whitespace().collect();
    let parent = fields.get(1)?.parse().ok()?;
    let time =
        fields.get(11..15)?.iter().map(|f| f.parse::<i64>().ok().map(|t| t.max(0) as u64)).sum::<Option<u64>>()?;
    Some(PidStat { pid: pid.trim().parse().ok()?, parent, time })
}

/// The tightest CPU limit of the control group the transcoder is in, or of
/// those above it that it sees: cgroup v1's `cpu` controller where the
/// process has one, else v2's, at their usual places below `/sys/fs/cgroup`.
#[cfg(target_os = "linux")]
fn limit() -> Option<Limit> {
    let groups = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let root = Path::new("/sys/fs/cgroup");
    let (cores, used) = match group_path(&groups, Some("cpu")) {
        Some(group) => {
            let both = root.join("cpu,cpuacct");
            let (cpu, acct) =
                if both.is_dir() { (both.clone(), both) } else { (root.join("cpu"), root.join("cpuacct")) };
            limit_v1(&cpu, group, &acct)?
        }
        None => limit_v2(root, group_path(&groups, None)?)?,
    };
    Some(Limit { cores, used, at: Instant::now() })
}

/// Where `/proc/self/cgroup` puts the process: its cgroup v1 group with
/// `controller`, or its v2 group with `None`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn group_path<'a>(text: &'a str, controller: Option<&str>) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let mut fields = line.splitn(3, ':');
        let (_, controllers, path) = (fields.next()?, fields.next()?, fields.next()?);
        let found = match controller {
            Some(controller) => controllers.split(',').any(|c| c == controller),
            None => controllers.is_empty(),
        };
        found.then_some(path)
    })
}

/// Of `group` (a path below `mount`) and the groups above it, the one whose
/// limit (`limit_of`: the cores, or the memory it leaves) is the tightest. A
/// container sees its own group at `mount`, whatever its path says.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn tightest<T: PartialOrd>(
    mount: &Path,
    group: &str,
    limit_of: impl Fn(&Path) -> Option<T>,
) -> Option<(PathBuf, T)> {
    let mut dir = mount.join(group.trim_start_matches('/'));
    if !dir.is_dir() {
        dir = mount.to_path_buf();
    }
    let mut found: Option<(PathBuf, T)> = None;
    loop {
        if let Some(limit) = limit_of(&dir)
            && found.as_ref().is_none_or(|(_, tightest)| limit < *tightest)
        {
            found = Some((dir.clone(), limit));
        }
        if !dir.pop() || !dir.starts_with(mount) {
            return found;
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn read_file(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// cgroup v2: `cpu.max` (`200000 100000`, or `max 100000` for none), and
/// the time used in that group's `cpu.stat`, in seconds.
#[cfg(any(target_os = "linux", test))]
fn limit_v2(mount: &Path, group: &str) -> Option<(f64, f64)> {
    let (dir, cores) = tightest(mount, group, |dir| {
        let max = read_file(dir.join("cpu.max"))?;
        let (quota, period) = max.trim().split_once(' ')?;
        let (quota, period): (f64, f64) = (quota.parse().ok()?, period.parse().ok()?);
        (quota > 0.0 && period > 0.0).then(|| quota / period)
    })?;
    let stat = read_file(dir.join("cpu.stat"))?;
    let usec: f64 = stat.lines().find_map(|line| line.strip_prefix("usage_usec "))?.trim().parse().ok()?;
    Some((cores, usec / 1e6))
}

/// cgroup v1: `cpu.cfs_quota_us` (-1 for none) over `cpu.cfs_period_us`,
/// and the time used in the same group's `cpuacct.usage` (below `acct`,
/// where cpuacct is mounted, with cpu or on its own), in seconds.
#[cfg(any(target_os = "linux", test))]
fn limit_v1(cpu: &Path, group: &str, acct: &Path) -> Option<(f64, f64)> {
    let (dir, cores) = tightest(cpu, group, |dir| {
        let quota: f64 = read_file(dir.join("cpu.cfs_quota_us"))?.trim().parse().ok()?;
        let period: f64 = read_file(dir.join("cpu.cfs_period_us"))?.trim().parse().ok()?;
        (quota > 0.0 && period > 0.0).then(|| quota / period)
    })?;
    let usage = read_file(acct.join(dir.strip_prefix(cpu).ok()?).join("cpuacct.usage"))?;
    let nsec: f64 = usage.trim().parse().ok()?;
    Some((cores, nsec / 1e9))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn proc_stat_counts_all_but_idle_and_waiting_for_the_disk() {
        let stat = "cpu  100 20 30 800 50 0 0 0 0 0\ncpu0 50 10 15 400 25 0 0 0 0 0\nintr 1\n";
        assert_eq!(parse_proc_stat(stat, None), Some(ProcStat { busy: 150, total: 1000, cpus: 1 }));
        assert_eq!(parse_proc_stat("intr 1\n", None), None);
    }

    #[test]
    fn proc_stat_counts_the_processors_allowed() {
        let stat = "cpu  60 0 0 340 0 0 0 0 0 0\n\
                    cpu0 10 0 0 90 0 0 0 0 0 0\ncpu1 20 0 0 80 0 0 0 0 0 0\n\
                    cpu2 30 0 0 70 0 0 0 0 0 0\ncpu3 0 0 0 100 0 0 0 0 0 0\nintr 1\n";
        assert_eq!(parse_proc_stat(stat, Some(&[1, 2])), Some(ProcStat { busy: 50, total: 200, cpus: 2 }));
        assert_eq!(parse_proc_stat(stat, None), Some(ProcStat { busy: 60, total: 400, cpus: 4 }));
        // Numbered otherwise (LXCFS shows a container's own from cpu0): all.
        assert_eq!(parse_proc_stat(stat, Some(&[3, 4])), Some(ProcStat { busy: 60, total: 400, cpus: 4 }));
    }

    #[test]
    fn the_processors_allowed_are_read_from_the_status() {
        let status = "Name:\tpley-transcoder\nCpus_allowed:\t0d0f\nCpus_allowed_list:\t0-3,8,10-11\nMems_allowed:\t1\n";
        assert_eq!(allowed_cpus(status), Some(vec![0, 1, 2, 3, 8, 10, 11]));
        assert_eq!(allowed_cpus("Cpus_allowed_list:\t0\n"), Some(vec![0]));
        assert_eq!(allowed_cpus("Cpus_allowed_list:\tx\n"), None);
        assert_eq!(allowed_cpus("Name:\tpley\n"), None);
    }

    #[test]
    fn a_process_counts_its_time_and_its_ended_childrens() {
        let stat = "4242 (ffmpeg (x) y) S 1000 4242 4242 0 -1 4194560 100 0 0 0 250 50 7 3 20 0 1 0 900 1 1\n";
        assert_eq!(parse_pid_stat(stat), Some(PidStat { pid: 4242, parent: 1000, time: 310 }));
        assert_eq!(parse_pid_stat("4242 (ffmpeg) S 1000"), None);
    }

    #[test]
    fn the_control_group_is_found_by_version() {
        let v2 = "0::/system.slice/pley.service\n";
        assert_eq!(group_path(v2, None), Some("/system.slice/pley.service"));
        assert_eq!(group_path(v2, Some("cpu")), None);
        let v1 = "5:cpuacct,cpu:/docker/abc\n4:memory:/docker/abc\n0::/\n";
        assert_eq!(group_path(v1, Some("cpu")), Some("/docker/abc"));
        assert_eq!(group_path(v1, None), Some("/"));
    }

    fn write(dir: &Path, files: &[(&str, &str)]) {
        std::fs::create_dir_all(dir).unwrap();
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
    }

    #[test]
    fn the_tightest_limit_of_the_groups_above_counts_with_its_time() {
        let mount = tempfile::tempdir().unwrap();
        let mount = mount.path();
        write(mount, &[("cpu.stat", "usage_usec 9000000\n")]);
        write(
            &mount.join("a"),
            &[("cpu.max", "300000 100000\n"), ("cpu.stat", "usage_usec 4500000\nuser_usec 4000000\n")],
        );
        write(&mount.join("a/b"), &[("cpu.max", "max 100000\n"), ("cpu.stat", "usage_usec 2000000\n")]);
        assert_eq!(limit_v2(mount, "/a/b"), Some((3.0, 4.5)));
        write(&mount.join("a/b"), &[("cpu.max", "250000 100000\n")]);
        assert_eq!(limit_v2(mount, "/a/b"), Some((2.5, 2.0)));
        // A container's own group is its root, whatever the path.
        write(mount, &[("cpu.max", "400000 100000\n")]);
        assert_eq!(limit_v2(mount, "/docker/abc"), Some((4.0, 9.0)));
        assert_eq!(limit_v2(&mount.join("a/b/none"), "/"), None);

        let v1 = tempfile::tempdir().unwrap();
        let (cpu, acct) = (v1.path().join("cpu"), v1.path().join("cpuacct"));
        write(&cpu.join("docker/abc"), &[("cpu.cfs_quota_us", "150000\n"), ("cpu.cfs_period_us", "100000\n")]);
        write(&cpu, &[("cpu.cfs_quota_us", "-1\n"), ("cpu.cfs_period_us", "100000\n")]);
        write(&acct.join("docker/abc"), &[("cpuacct.usage", "3000000000\n")]);
        assert_eq!(limit_v1(&cpu, "/docker/abc", &acct), Some((1.5, 3.0)));
        assert_eq!(limit_v1(&cpu, "/", &acct), None, "no limit");
    }

    fn times(busy: f64, total: f64, ours: Option<f64>) -> Times {
        Times { busy, total, cpus: 1.0, ours, limit: None }
    }

    #[test]
    fn the_shares_follow_the_last_looks() {
        let meter = CpuMeter::default();
        // Busy and foreign, as they are to a rounding.
        let is = |busy: f64, foreign: f64| {
            let near = |got: Option<f64>, want: f64| got.is_some_and(|got| (got - want).abs() < 1e-9);
            near(meter.busy(), busy) && near(meter.foreign(), foreign)
        };
        meter.record(times(0.0, 0.0, Some(0.0)));
        assert_eq!(meter.busy(), None, "one look says nothing yet");
        meter.record(times(80.0, 100.0, Some(60.0)));
        assert!(is(0.8, 0.2), "{meter:?}");
        meter.record(times(100.0, 200.0, Some(60.0)));
        assert!(is(0.5, 0.2), "(0.8 + 0.2) / 2, (0.2 + 0.2) / 2: {meter:?}");
        // A counter that went back (a wrap, a new boot) is skipped.
        meter.record(times(10.0, 20.0, Some(0.0)));
        assert!(is(0.5, 0.2), "{meter:?}");
        // Ours unknown: the others' share as it was.
        meter.record(times(30.0, 40.0, None));
        assert!(is(0.75, 0.2), "{meter:?}");
    }

    #[test]
    fn other_programs_keep_the_cores_the_processors_or_the_limit_leave_less_of() {
        let at = Instant::now();
        let look = |busy, total, ours, used, secs| Times {
            busy,
            total,
            cpus: 12.0,
            ours: Some(ours),
            limit: Some(Limit { cores: 4.0, used, at: at + Duration::from_secs(secs) }),
        };
        // In 10 s, 12 processors' 120 s: 40 s busy, 30 s of them the
        // transcoder's. Its group, limited to 4 cores, used 38 s: the limit
        // is 95 % busy, and the rest of the group took 0.8 of its cores.
        let shares = shares_between(look(0.0, 0.0, 0.0, 0.0, 0), look(40.0, 120.0, 30.0, 38.0, 10)).unwrap();
        assert!((shares.busy - 0.95).abs() < 1e-9, "{shares:?}");
        assert!((shares.foreign.unwrap() - 0.8).abs() < 1e-9, "{shares:?}");
        // Other programs on the processors leaving less than the limit does.
        let shares = shares_between(look(0.0, 0.0, 0.0, 0.0, 0), look(117.0, 120.0, 30.0, 30.0, 10)).unwrap();
        assert!((shares.busy - 0.975).abs() < 1e-9, "{shares:?}");
        assert!((shares.foreign.unwrap() - 0.7).abs() < 1e-9, "the processors leave 3.3 of the 4 cores: {shares:?}");
        // Without a limit, the processors' share of the cores.
        let free = |t: Times| Times { limit: None, ..t };
        let shares =
            shares_between(free(look(0.0, 0.0, 0.0, 0.0, 0)), free(look(40.0, 120.0, 30.0, 38.0, 10))).unwrap();
        assert!((shares.busy - 1.0 / 3.0).abs() < 1e-9 && (shares.foreign.unwrap() - 1.0).abs() < 1e-9, "{shares:?}");
    }

    #[test]
    fn room_and_quota_come_from_the_last_look() {
        let meter = CpuMeter::default();
        assert_eq!((meter.room(), meter.quota()), (None, None), "before any look");
        // 16 processors, and a control group's limit of `cores`.
        let look = |cores: Option<f64>| Times {
            busy: 0.0,
            total: 0.0,
            cpus: 16.0,
            ours: None,
            limit: cores.map(|cores| Limit { cores, used: 0.0, at: Instant::now() }),
        };
        meter.record(look(Some(2.0)));
        assert_eq!((meter.room(), meter.quota()), (Some(2.0), Some(2.0)), "a container's --cpus=2");
        meter.record(look(Some(32.0)));
        assert_eq!((meter.room(), meter.quota()), (Some(16.0), None), "a limit the processors are within");
        meter.record(look(None));
        assert_eq!((meter.room(), meter.quota()), (Some(16.0), None));
    }

    #[test]
    fn this_computer_can_be_measured() {
        if cfg!(any(windows, target_os = "linux")) {
            let meter = CpuMeter::default();
            meter.sample();
            std::thread::sleep(std::time::Duration::from_millis(50));
            meter.sample();
            assert!(meter.busy().is_some_and(|b| (0.0..=1.0).contains(&b)));
            assert!(meter.room().is_some_and(|room| room > 0.0), "{:?}", meter.room());
            let cores = f64::from(crate::util::processors());
            assert!(meter.foreign().is_none_or(|f| (0.0..=cores + 1.0).contains(&f)), "{:?}", meter.foreign());
        }
    }
}
