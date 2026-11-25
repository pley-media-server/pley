//! How busy the machine is and how much of it pley takes: the processors'
//! time, the memory and the network, sampled every [`INTERVAL`] for the
//! dashboard's charts (Plex's "CPU" and "RAM"). pley here is this process
//! and every process it started, so a managed transcoder and its FFmpegs
//! count too; an external transcoder, perhaps on another machine, doesn't.
//! On the network, pley is what it sends its clients ([`crate::traffic`]),
//! and the machine is what its hardware network adapters carry (in a
//! container, which sees none, its own).
//!
//! The machine's processors and memory cost a few system calls to read.
//! Pley's own share and the network adapters cost more: a listing of every
//! process on the machine (Windows) or a read of each `/proc/<pid>/stat`
//! (Linux), and the adapters' table, a few milliseconds of one processor on
//! a desktop with hundreds of processes. Those are read with every sample
//! while someone looks at the samples (the dashboard asks every 2 s) and
//! while pley's processes come and go (FFmpegs starting and ending), and
//! otherwise every [`UNWATCHED_EVERY`] samples: the samples between them
//! wait for the next reading and take its averages, so the charts have a
//! sample every 2 s either way, and an idle server doesn't list the
//! machine's processes every 2 s for no one. A process's time counts only
//! while it is seen running, so one that ended between two listings would
//! be lost to pley's share and taken for another program's by the library
//! work scheduler ([`Monitor::others_cpu`]): that is why pley's processes
//! are listed with every sample while they come and go.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::traffic::Client;

/// How often the machine is sampled.
pub const INTERVAL: Duration = Duration::from_secs(2);
/// While no one looks, pley's processes and the network adapters are read
/// every this many samples (10 s).
const UNWATCHED_EVERY: u32 = 5;
/// How long someone looks at the samples after asking for them, and how
/// long pley's processes are listed with every sample after one began or
/// ended.
const WATCHED_FOR: Duration = Duration::from_secs(30);
/// The samples kept: five minutes of them.
const KEEP: usize = 150;
/// The samples a client's speed is averaged over, the newest among them:
/// players fetch a few seconds of media at a time, and a list of who sent
/// in the last two seconds alone would blink.
const CLIENT_SAMPLES: usize = 5;
/// The clients listed at most, the busiest first.
const MAX_CLIENTS: usize = 50;

/// The machine and pley at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Sample {
    /// When, in Unix milliseconds.
    pub at: i64,
    /// The share of all the processors' time used since the sample before,
    /// 0 to 100, by everything on the machine.
    pub cpu: f32,
    /// The same, by pley; between readings of pley's processes 10 s apart
    /// (no one looking, none of them coming or going), the share over those
    /// 10 s (see the module's documentation).
    pub pley_cpu: f32,
    /// Memory in use on the machine, in bytes (without what only caches
    /// files and could be given back at once).
    pub memory: u64,
    /// Memory pley holds: its processes' resident sets, in bytes, at the
    /// reading of its processes that came with this sample.
    pub pley_memory: u64,
    /// What the machine's network adapters received since the sample
    /// before, in bytes a second; like `pley_cpu`, over 10 s between
    /// readings that far apart.
    pub net_in: u64,
    /// What they sent.
    pub net_out: u64,
    /// What pley sent its clients, in bytes a second.
    pub pley_out: u64,
}

/// A client pley is sending to, and how fast.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClientRate {
    #[serde(flatten)]
    pub client: Client,
    /// Bytes a second, over the last few samples.
    pub sent: u64,
}

/// What `GET /api/system/resources` answers.
#[derive(Debug, Serialize)]
pub struct Resources {
    /// Whether this system can be sampled at all (Windows, Linux and macOS).
    pub available: bool,
    pub interval_ms: u64,
    /// Logical processors on the machine.
    pub cpus: u32,
    /// Its memory, in bytes.
    pub memory_total: u64,
    /// Oldest first.
    pub samples: Vec<Sample>,
    /// Who pley is sending to now, the busiest first.
    pub clients: Vec<ClientRate>,
}

/// The samples of the last few minutes, taken in the background.
pub struct Monitor {
    history: Mutex<History>,
    clients: Mutex<Vec<ClientRate>>,
    /// When the samples were last asked for.
    asked: Mutex<Option<Instant>>,
    cpus: u32,
    memory_total: u64,
}

#[derive(Default)]
struct History {
    /// Oldest first.
    samples: VecDeque<Sample>,
    /// The newest samples that wait for the costly readings ([`Costly`]):
    /// those taken since they were last read.
    waiting: usize,
}

impl History {
    /// The samples that don't wait any more, oldest first.
    fn done(&self) -> impl Iterator<Item = &Sample> {
        self.samples.iter().take(self.samples.len() - self.waiting)
    }
}

impl Monitor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            history: Mutex::new(History { samples: VecDeque::with_capacity(KEEP), waiting: 0 }),
            clients: Mutex::default(),
            asked: Mutex::default(),
            cpus: os::cpus().max(1),
            memory_total: os::memory().map(|m| m.total).unwrap_or(0),
        })
    }

    /// Sample every [`INTERVAL`] until `stopping`; pley's processes and the
    /// network with each sample while someone looks, and otherwise every
    /// [`UNWATCHED_EVERY`] samples (see the module's documentation).
    pub fn start(self: &Arc<Self>, stopping: CancellationToken) {
        let monitor = self.clone();
        if !os::SUPPORTED {
            return;
        }
        tokio::spawn(async move {
            let mut sampler = Sampler::new(monitor.cpus);
            let mut tick = tokio::time::interval(INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            // The costly readings begin with the first sample.
            let mut since_costly = UNWATCHED_EVERY;
            // Until when pley's processes are listed with every sample, one
            // of them having begun or ended.
            let mut busy_until = Instant::now();
            loop {
                tokio::select! {
                    _ = stopping.cancelled() => break,
                    _ = tick.tick() => {}
                }
                if let Some(sample) = sampler.sample() {
                    monitor.push(sample);
                }
                since_costly += 1;
                if since_costly >= UNWATCHED_EVERY || monitor.watched() || Instant::now() < busy_until {
                    monitor.fill(sampler.costly());
                    since_costly = 0;
                    if sampler.came_and_went {
                        busy_until = Instant::now() + WATCHED_FOR;
                    }
                }
                *monitor.clients.lock() = sampler.clients();
            }
        });
    }

    /// A sample whose costly readings are yet to come.
    fn push(&self, sample: Sample) {
        let mut history = self.history.lock();
        if history.samples.len() == KEEP {
            history.samples.pop_front();
        }
        history.samples.push_back(sample);
        history.waiting = (history.waiting + 1).min(history.samples.len());
    }

    /// What the costly readings found since they were last read goes to
    /// each sample that waited for them: the shares and rates over that time
    /// were those of each part of it, as far as anyone can tell. `None`
    /// (the first reading) leaves them as they are.
    fn fill(&self, costly: Option<Costly>) {
        let mut history = self.history.lock();
        let waiting = std::mem::take(&mut history.waiting);
        let Some(costly) = costly else { return };
        for sample in history.samples.iter_mut().rev().take(waiting) {
            sample.pley_cpu = costly.pley_cpu;
            sample.pley_memory = costly.pley_memory;
            sample.net_in = costly.net_in;
            sample.net_out = costly.net_out;
        }
    }

    /// Whether someone asked for the samples lately.
    fn watched(&self) -> bool {
        self.asked.lock().is_some_and(|at| at.elapsed() < WATCHED_FOR)
    }

    /// The share of all the processors' time other programs took over the
    /// last `window`, 0 to 100: everything on the machine but pley (an
    /// external transcoder's FFmpeg on this machine counts as another
    /// program). `None` before a sample, or where nothing is sampled.
    pub fn others_cpu(&self, window: Duration) -> Option<f32> {
        let since = crate::util::now_ms() - window.as_millis() as i64;
        let history = self.history.lock();
        // Pley's share of samples that waited is the average over them all,
        // while the machine's is each one's own: the averages over the
        // window are compared, not sample by sample, which would count a
        // burst of pley's as other programs' in the sample it fell in.
        let (mut cpu, mut pley, mut count) = (0.0, 0.0, 0);
        for sample in history.done().filter(|s| s.at >= since) {
            cpu += sample.cpu;
            pley += sample.pley_cpu;
            count += 1;
        }
        (count > 0).then(|| ((cpu - pley) / count as f32).max(0.0))
    }

    /// The samples taken after `since` (Unix milliseconds), or all of them;
    /// not those still waiting for the costly readings, which come with
    /// the next answer once read (asking makes them come with every sample
    /// for a while).
    pub fn resources(&self, since: Option<i64>) -> Resources {
        *self.asked.lock() = Some(Instant::now());
        let samples = self.history.lock().done().filter(|s| since.is_none_or(|t| s.at > t)).copied().collect();
        Resources {
            available: os::SUPPORTED,
            interval_ms: INTERVAL.as_millis() as u64,
            cpus: self.cpus,
            memory_total: self.memory_total,
            samples,
            clients: self.clients.lock().clone(),
        }
    }
}

/// Processor time so far, as the system counts it (any unit, the same each time).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CpuTimes {
    busy: u64,
    total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Memory {
    total: u64,
    used: u64,
}

/// Bytes the network adapters have carried so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Network {
    received: u64,
    sent: u64,
}

/// One of pley's processes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Process {
    /// Its id and when it started (in the system's own unit), so that an id
    /// used again by a new process isn't taken for the old one.
    key: (u32, u64),
    /// Processor time it has used, in nanoseconds.
    cpu_ns: u64,
    /// Its resident set, in bytes.
    memory: u64,
}

/// Turns the system's running totals into the shares between two samples.
struct Sampler {
    cpus: u32,
    last: Option<Reading>,
    /// When the costly readings were last read, and the adapters' totals then.
    last_costly: Option<(Instant, Option<Network>)>,
    /// Each process's processor time at the costly readings before.
    processes: HashMap<(u32, u64), u64>,
    /// Whether one of pley's processes began or ended between the last two
    /// costly readings.
    came_and_went: bool,
    /// What each client had been sent at the last few samples, newest last.
    clients: VecDeque<(Instant, HashMap<Client, u64>)>,
}

/// The running totals at one sample.
#[derive(Clone, Copy)]
struct Reading {
    at: Instant,
    cpu: CpuTimes,
    pley_sent: u64,
}

/// What the costly readings tell of the time since they were last read:
/// pley's share of the processors and the adapters' rates over it, and
/// pley's memory at its end.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Costly {
    pley_cpu: f32,
    pley_memory: u64,
    net_in: u64,
    net_out: u64,
}

impl Sampler {
    fn new(cpus: u32) -> Self {
        Self {
            cpus,
            last: None,
            last_costly: None,
            processes: HashMap::new(),
            came_and_went: false,
            clients: VecDeque::new(),
        }
    }

    /// A sample of what costs next to nothing to read, without pley's
    /// processes and the network ([`Self::costly`]). `None` the first time
    /// (a share needs two readings) or when the system can't be read.
    fn sample(&mut self) -> Option<Sample> {
        let now = Instant::now();
        let times = os::cpu_times()?;
        let memory = os::memory()?;
        let pley_sent = crate::traffic::traffic().sent();
        let last = self.last.replace(Reading { at: now, cpu: times, pley_sent });
        let Reading { at: then, cpu: before, pley_sent: pley_sent_before } = last?;
        let seconds = now.duration_since(then).as_secs_f64();
        Some(Sample {
            at: crate::util::now_ms(),
            cpu: share(times.busy.saturating_sub(before.busy) as f64, times.total.saturating_sub(before.total) as f64),
            memory: memory.used,
            pley_out: rate(pley_sent.saturating_sub(pley_sent_before), seconds),
            ..Sample::default()
        })
    }

    /// Pley's processes and the network adapters since they were last
    /// read; `None` the first time.
    fn costly(&mut self) -> Option<Costly> {
        let now = Instant::now();
        let network = os::network();
        let tree = os::processes(std::process::id());
        let pley_memory = tree.iter().map(|p| p.memory).sum();
        let previous = std::mem::replace(&mut self.processes, tree.iter().map(|p| (p.key, p.cpu_ns)).collect());
        self.came_and_went = came_and_went(&previous, &self.processes);
        let (then, network_before) = self.last_costly.replace((now, network))?;
        let seconds = now.duration_since(then).as_secs_f64();
        let elapsed = now.duration_since(then).as_nanos() as f64 * f64::from(self.cpus);
        // Counters only go up; one that went down (an adapter gone, a
        // 32-bit count past its end) reads as nothing for this reading.
        let (net_in, net_out) = match (network, network_before) {
            (Some(n), Some(b)) => {
                (rate(n.received.saturating_sub(b.received), seconds), rate(n.sent.saturating_sub(b.sent), seconds))
            }
            _ => (0, 0),
        };
        // A process seen for the first time started since the reading
        // before (or this is the second reading, and every process is new).
        let used: u64 = tree.iter().map(|p| p.cpu_ns.saturating_sub(previous.get(&p.key).copied().unwrap_or(0))).sum();
        Some(Costly { pley_cpu: share(used as f64, elapsed), pley_memory, net_in, net_out })
    }

    /// Who pley is sending to, and how fast over the last few samples.
    fn clients(&mut self) -> Vec<ClientRate> {
        // A client is forgotten once it has been sent nothing for as long as
        // the speeds are averaged over: one between two requests, or whose
        // player paused for a moment, stays listed.
        let before = self.clients.front().map(|(_, totals)| totals.clone()).unwrap_or_default();
        let now: HashMap<Client, u64> = crate::traffic::traffic().clients(&before).into_iter().collect();
        self.clients.push_back((Instant::now(), now));
        if self.clients.len() > CLIENT_SAMPLES + 1 {
            self.clients.pop_front();
        }
        client_rates(&self.clients)
    }
}

/// Each client's speed between the oldest and newest of `totals`, the
/// busiest first. An account's requests from one address are one client,
/// named after its busiest device: one device can go by two names, the
/// app's own on its API requests ("Pley for Roku") and the one it gave its
/// playback on its player's, or by none, a player fetching a stream itself.
/// A request whose sign-in nobody read (the web app's files) counts for
/// the busiest signed-in client at its address, if there is one.
fn client_rates(totals: &VecDeque<(Instant, HashMap<Client, u64>)>) -> Vec<ClientRate> {
    let (Some((first, oldest)), Some((last, newest))) = (totals.front(), totals.back()) else { return Vec::new() };
    let seconds = last.duration_since(*first).as_secs_f64();
    // A client missing from the oldest totals was first sent to since, and
    // one whose total went down was forgotten and counted again from 0.
    let mut sent: Vec<(&Client, u64)> = newest
        .iter()
        .map(|(client, &total)| {
            let before = oldest.get(client).copied().unwrap_or(0);
            (client, if total >= before { total - before } else { total })
        })
        .filter(|&(_, bytes)| bytes > 0)
        .collect();
    // The busiest first, so that each joins the busiest it can.
    sent.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let mut listed: Vec<(&Client, u64)> = Vec::new();
    for signed_in in [true, false] {
        for &(client, bytes) in sent.iter().filter(|(c, _)| c.user.is_some() == signed_in) {
            let joins = client.address.and_then(|address| {
                listed.iter().position(|(c, _)| {
                    c.user.is_some() && c.address == Some(address) && (!signed_in || c.user == client.user)
                })
            });
            match joins {
                Some(i) => {
                    listed[i].1 += bytes;
                    if signed_in && listed[i].0.device.is_none() {
                        listed[i].0 = client;
                    }
                }
                None => listed.push((client, bytes)),
            }
        }
    }
    let mut rates: Vec<ClientRate> = listed
        .into_iter()
        .map(|(client, bytes)| ClientRate { client: client.clone(), sent: rate(bytes, seconds) })
        .collect();
    rates.sort_by(|a, b| b.sent.cmp(&a.sent).then_with(|| a.client.cmp(&b.client)));
    rates.truncate(MAX_CLIENTS);
    rates
}

/// `bytes` over `seconds`, a second.
fn rate(bytes: u64, seconds: f64) -> u64 {
    if seconds <= 0.0 { 0 } else { (bytes as f64 / seconds).round() as u64 }
}

/// `part` of `whole` as a percentage, 0 to 100, to a tenth.
fn share(part: f64, whole: f64) -> f32 {
    if whole <= 0.0 {
        return 0.0;
    }
    ((part / whole * 100.0).clamp(0.0, 100.0) * 10.0).round() as f32 / 10.0
}

/// Whether a process is in one listing and not in the other: one began or
/// ended between them.
fn came_and_went(before: &HashMap<(u32, u64), u64>, now: &HashMap<(u32, u64), u64>) -> bool {
    before.len() != now.len() || now.keys().any(|key| !before.contains_key(key))
}

#[cfg(windows)]
mod os {
    use std::collections::HashMap;
    use std::mem::{size_of, zeroed};

    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GetIfTable2, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IF_ROW2, MIB_IF_TABLE2,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::{
        ALL_PROCESSOR_GROUPS, GetActiveProcessorCount, GetProcessTimes, GetSystemTimes, OpenProcess,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    use super::{CpuTimes, Memory, Network, Process};

    pub const SUPPORTED: bool = true;

    fn ticks(t: FILETIME) -> u64 {
        (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
    }

    pub fn cpus() -> u32 {
        unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) }
    }

    pub fn cpu_times() -> Option<CpuTimes> {
        let (mut idle, mut kernel, mut user) = unsafe { (zeroed(), zeroed(), zeroed()) };
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return None;
        }
        // Kernel time includes the idle time.
        let total = ticks(kernel) + ticks(user);
        Some(CpuTimes { busy: total.saturating_sub(ticks(idle)), total })
    }

    pub fn memory() -> Option<Memory> {
        let mut status: MEMORYSTATUSEX = unsafe { zeroed() };
        status.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
            return None;
        }
        Some(Memory { total: status.ullTotalPhys, used: status.ullTotalPhys.saturating_sub(status.ullAvailPhys) })
    }

    /// The adapters' counts. Windows lists each adapter again for every
    /// filter on it (and its virtual ones): only hardware is counted, so a
    /// byte is counted once.
    pub fn network() -> Option<Network> {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if unsafe { GetIfTable2(&mut table) } != 0 || table.is_null() {
            return None;
        }
        let mut network = Network::default();
        unsafe {
            // The rows run on past the one the struct declares: no reference
            // to that one-row array, which would cover the first alone.
            let first = (&raw const (*table).Table).cast::<MIB_IF_ROW2>();
            let rows = std::slice::from_raw_parts(first, (*table).NumEntries as usize);
            for row in rows {
                let hardware = row.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
                if hardware && row.Type != IF_TYPE_SOFTWARE_LOOPBACK {
                    network.received += row.InOctets;
                    network.sent += row.OutOctets;
                }
            }
            FreeMibTable(table.cast());
        }
        Some(network)
    }

    /// `root` and the processes it started, theirs and so on.
    pub fn processes(root: u32) -> Vec<Process> {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Vec::new();
            }
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                if entry.th32ProcessID != entry.th32ParentProcessID {
                    children.entry(entry.th32ParentProcessID).or_default().push(entry.th32ProcessID);
                }
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        let mut found = Vec::new();
        let mut queue = vec![(root, 0u64)];
        while let Some((pid, parent_started)) = queue.pop() {
            let Some(process) = process(pid) else { continue };
            // Windows keeps a parent's id after the parent ends, and may give
            // it to a newer process: a child is never older than its parent.
            if process.key.1 < parent_started {
                continue;
            }
            for &child in children.get(&pid).into_iter().flatten() {
                queue.push((child, process.key.1));
            }
            found.push(process);
        }
        found
    }

    fn process(pid: u32) -> Option<Process> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let (mut created, mut exited, mut kernel, mut user) = (zeroed(), zeroed(), zeroed(), zeroed());
            let times = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
            let mut counters: PROCESS_MEMORY_COUNTERS = zeroed();
            counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            let memory = K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) != 0;
            CloseHandle(handle);
            times.then(|| Process {
                key: (pid, ticks(created)),
                // In units of 100 ns.
                cpu_ns: (ticks(kernel) + ticks(user)) * 100,
                memory: if memory { counters.WorkingSetSize as u64 } else { 0 },
            })
        }
    }
}

#[cfg(target_os = "linux")]
mod os {
    use std::collections::HashMap;

    use super::{CpuTimes, Memory, Network, Process};

    pub const SUPPORTED: bool = true;

    pub fn cpus() -> u32 {
        let stat = std::fs::read_to_string("/proc/stat").unwrap_or_default();
        stat.lines().filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit)).count()
            as u32
    }

    pub fn cpu_times() -> Option<CpuTimes> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        parse_cpu_times(stat.lines().next()?)
    }

    /// The first line of `/proc/stat`: "cpu  user nice system idle iowait irq
    /// softirq steal guest guest_nice". Guest time is already in user time.
    pub(super) fn parse_cpu_times(line: &str) -> Option<CpuTimes> {
        let mut fields = line.split_ascii_whitespace();
        if fields.next()? != "cpu" {
            return None;
        }
        let values: Vec<u64> = fields.take(8).map(|f| f.parse().unwrap_or(0)).collect();
        if values.len() < 4 {
            return None;
        }
        let total: u64 = values.iter().sum();
        let idle = values[3] + values.get(4).copied().unwrap_or(0);
        Some(CpuTimes { busy: total.saturating_sub(idle), total })
    }

    pub fn memory() -> Option<Memory> {
        parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
    }

    pub fn network() -> Option<Network> {
        let text = std::fs::read_to_string("/proc/net/dev").ok()?;
        // A hardware adapter has a device behind it; bridges, bonds, VLANs,
        // tunnels and containers' links (/sys/devices/virtual) don't.
        Some(parse_net_dev(&text, |name| std::path::Path::new("/sys/class/net").join(name).join("device").exists()))
    }

    /// `/proc/net/dev`: two lines of headings, then "  eth0: received-bytes
    /// packets … (8 fields) sent-bytes …" for each adapter. Only `hardware`
    /// adapters count, as a byte through a bridge (Docker's, Unraid's br0), a
    /// container's link or a VPN goes through a hardware one too; where
    /// there is none, as in a container, which sees only its own link,
    /// every adapter but loopback counts.
    pub(super) fn parse_net_dev(text: &str, hardware: impl Fn(&str) -> bool) -> Network {
        let adapters: Vec<(&str, u64, u64)> = text
            .lines()
            .skip(2)
            .filter_map(|line| {
                let (name, counts) = line.split_once(':')?;
                let name = name.trim();
                if name == "lo" {
                    return None;
                }
                let counts: Vec<u64> = counts.split_ascii_whitespace().map(|c| c.parse().unwrap_or(0)).collect();
                Some((name, counts.first().copied().unwrap_or(0), counts.get(8).copied().unwrap_or(0)))
            })
            .collect();
        let any_hardware = adapters.iter().any(|&(name, ..)| hardware(name));
        let mut network = Network::default();
        for (name, received, sent) in adapters {
            if !any_hardware || hardware(name) {
                network.received += received;
                network.sent += sent;
            }
        }
        network
    }

    pub(super) fn parse_meminfo(text: &str) -> Option<Memory> {
        let field = |name: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
                .and_then(|v| v.split_ascii_whitespace().next()?.parse::<u64>().ok())
                .map(|kib| kib * 1024)
        };
        let total = field("MemTotal")?;
        // Kernels before 3.14 have no MemAvailable.
        let available = field("MemAvailable")
            .or_else(|| Some(field("MemFree")? + field("Buffers").unwrap_or(0) + field("Cached").unwrap_or(0)))?;
        Some(Memory { total, used: total.saturating_sub(available) })
    }

    /// One `/proc/<pid>/stat`: its parent and the process as counted here.
    pub(super) fn parse_stat(pid: u32, text: &str, tick_ns: u64, page: u64) -> Option<(u32, Process)> {
        // The name, in parentheses, may itself hold spaces and parentheses.
        let rest = &text[text.rfind(')')? + 1..];
        let fields: Vec<&str> = rest.split_ascii_whitespace().collect();
        // Field 3 (state) is fields[0]; ppid (4) fields[1]; utime (14)
        // fields[11]; stime (15) fields[12]; starttime (22) fields[19]; rss
        // (24) fields[21].
        let number = |i: usize| fields.get(i).and_then(|f| f.parse::<u64>().ok());
        let parent = number(1)? as u32;
        Some((
            parent,
            Process {
                key: (pid, number(19)?),
                cpu_ns: (number(11)? + number(12)?) * tick_ns,
                memory: number(21)? * page,
            },
        ))
    }

    /// `root` and the processes it started, theirs and so on.
    pub fn processes(root: u32) -> Vec<Process> {
        let (tick_ns, page) = unsafe { (libc::sysconf(libc::_SC_CLK_TCK), libc::sysconf(libc::_SC_PAGESIZE)) };
        let tick_ns = if tick_ns > 0 { 1_000_000_000 / tick_ns as u64 } else { 10_000_000 };
        let page = if page > 0 { page as u64 } else { 4096 };
        let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
        let mut all: HashMap<u32, (u32, Process)> = HashMap::new();
        for entry in dir.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
            // A process may end between the listing and the read.
            if let Ok(text) = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                && let Some(read) = parse_stat(pid, &text, tick_ns, page)
            {
                all.insert(pid, read);
            }
        }
        super::descendants(root, &all)
    }
}

#[cfg(target_os = "macos")]
mod os {
    #![allow(deprecated)] // mach_host_self and mach_timebase_info: libc points to the mach2 crate.

    use std::mem::{size_of, zeroed};
    use std::sync::OnceLock;

    use super::{CpuTimes, Memory, Network, Process};

    pub const SUPPORTED: bool = true;

    fn host() -> libc::mach_port_t {
        static HOST: OnceLock<libc::mach_port_t> = OnceLock::new();
        *HOST.get_or_init(|| unsafe { libc::mach_host_self() })
    }

    fn sysctl_u64(name: &std::ffi::CStr) -> Option<u64> {
        let mut value: u64 = 0;
        let mut len = size_of::<u64>();
        let ok =
            unsafe { libc::sysctlbyname(name.as_ptr(), (&raw mut value).cast(), &mut len, std::ptr::null_mut(), 0) };
        (ok == 0).then_some(value)
    }

    pub fn cpus() -> u32 {
        let mut value: i32 = 0;
        let mut len = size_of::<i32>();
        let ok = unsafe {
            libc::sysctlbyname(c"hw.logicalcpu".as_ptr(), (&raw mut value).cast(), &mut len, std::ptr::null_mut(), 0)
        };
        if ok == 0 { value.max(1) as u32 } else { 1 }
    }

    pub fn cpu_times() -> Option<CpuTimes> {
        let mut load: libc::host_cpu_load_info = unsafe { zeroed() };
        let mut count = libc::HOST_CPU_LOAD_INFO_COUNT;
        let ok = unsafe { libc::host_statistics(host(), libc::HOST_CPU_LOAD_INFO, (&raw mut load).cast(), &mut count) };
        if ok != libc::KERN_SUCCESS {
            return None;
        }
        let t = load.cpu_ticks.map(u64::from);
        let total = t[libc::CPU_STATE_USER as usize]
            + t[libc::CPU_STATE_SYSTEM as usize]
            + t[libc::CPU_STATE_IDLE as usize]
            + t[libc::CPU_STATE_NICE as usize];
        Some(CpuTimes { busy: total - t[libc::CPU_STATE_IDLE as usize], total })
    }

    pub fn memory() -> Option<Memory> {
        let total = sysctl_u64(c"hw.memsize")?;
        let mut vm: libc::vm_statistics64 = unsafe { zeroed() };
        let mut count =
            (size_of::<libc::vm_statistics64>() / size_of::<libc::integer_t>()) as libc::mach_msg_type_number_t;
        let ok = unsafe { libc::host_statistics64(host(), libc::HOST_VM_INFO64, (&raw mut vm).cast(), &mut count) };
        if ok != libc::KERN_SUCCESS {
            return None;
        }
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
        // What Activity Monitor calls Memory Used: app memory, wired and compressed.
        let pages = u64::from(vm.internal_page_count).saturating_sub(u64::from(vm.purgeable_count))
            + u64::from(vm.wire_count)
            + u64::from(vm.compressor_page_count);
        Some(Memory { total, used: (pages * page).min(total) })
    }

    /// Ethernet and Wi-Fi adapters' type (`IFT_ETHER`, net/if_types.h).
    const IFT_ETHER: u8 = 0x6;

    /// The Ethernet and Wi-Fi adapters' counts: a bridge, a VLAN or a VPN's
    /// `utun` carries bytes one of them carries too. They are 32-bit here,
    /// so one passing 4 GiB reads as nothing for that sample.
    pub fn network() -> Option<Network> {
        let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
        if unsafe { libc::getifaddrs(&mut list) } != 0 {
            return None;
        }
        let mut network = Network::default();
        let mut entry = list;
        while !entry.is_null() {
            let ifa = unsafe { &*entry };
            let link = !ifa.ifa_addr.is_null() && i32::from(unsafe { (*ifa.ifa_addr).sa_family }) == libc::AF_LINK;
            let loopback = ifa.ifa_flags & libc::IFF_LOOPBACK as u32 != 0;
            if link && !loopback && !ifa.ifa_data.is_null() {
                let data = unsafe { &*ifa.ifa_data.cast::<libc::if_data>() };
                if data.ifi_type == IFT_ETHER {
                    network.received += u64::from(data.ifi_ibytes);
                    network.sent += u64::from(data.ifi_obytes);
                }
            }
            entry = ifa.ifa_next;
        }
        unsafe { libc::freeifaddrs(list) };
        Some(network)
    }

    /// Mach time units per nanosecond, as numerator and denominator.
    fn timebase() -> (u64, u64) {
        static TIMEBASE: OnceLock<(u64, u64)> = OnceLock::new();
        *TIMEBASE.get_or_init(|| {
            let mut info = libc::mach_timebase_info { numer: 1, denom: 1 };
            unsafe { libc::mach_timebase_info(&mut info) };
            (u64::from(info.numer), u64::from(info.denom.max(1)))
        })
    }

    fn process(pid: i32) -> Option<Process> {
        let mut info: libc::proc_taskinfo = unsafe { zeroed() };
        let size = size_of::<libc::proc_taskinfo>() as i32;
        let read = unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size) };
        if read != size {
            return None;
        }
        let (numer, denom) = timebase();
        Some(Process {
            // Ids come back slowly here (they count up to 99999 first).
            key: (pid as u32, 0),
            cpu_ns: (info.pti_total_user + info.pti_total_system) * numer / denom,
            memory: info.pti_resident_size,
        })
    }

    /// `root` and the processes it started, theirs and so on.
    pub fn processes(root: u32) -> Vec<Process> {
        let mut found = Vec::new();
        let mut queue = vec![root as i32];
        while let Some(pid) = queue.pop() {
            let Some(process) = process(pid) else { continue };
            found.push(process);
            let mut children = [0i32; 256];
            let count =
                unsafe { libc::proc_listchildpids(pid, children.as_mut_ptr().cast(), size_of::<[i32; 256]>() as i32) };
            queue.extend(children.iter().take(count.max(0) as usize).filter(|&&c| c > 0));
        }
        found
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod os {
    use super::{CpuTimes, Memory, Network, Process};

    pub const SUPPORTED: bool = false;

    pub fn cpus() -> u32 {
        std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1)
    }

    pub fn cpu_times() -> Option<CpuTimes> {
        None
    }

    pub fn memory() -> Option<Memory> {
        None
    }

    pub fn network() -> Option<Network> {
        None
    }

    pub fn processes(_root: u32) -> Vec<Process> {
        Vec::new()
    }
}

/// `root` and its descendants among `all` (each process by id, with its parent's id).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn descendants(root: u32, all: &HashMap<u32, (u32, Process)>) -> Vec<Process> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&pid, &(parent, _)) in all {
        if pid != parent {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut found = Vec::new();
    let mut queue = vec![root];
    while let Some(pid) = queue.pop() {
        let Some(&(_, process)) = all.get(&pid) else { continue };
        found.push(process);
        queue.extend(children.get(&pid).into_iter().flatten());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, cpu_ns: u64) -> Process {
        Process { key: (pid, 1), cpu_ns, memory: u64::from(pid) }
    }

    #[test]
    fn descendants_are_the_root_its_children_and_theirs() {
        let all: HashMap<u32, (u32, Process)> = [
            (1, (0, proc(1, 0))),
            (10, (1, proc(10, 0))),
            (11, (10, proc(11, 0))),
            (12, (10, proc(12, 0))),
            (20, (1, proc(20, 0))),
        ]
        .into_iter()
        .collect();
        let mut pids: Vec<u32> = descendants(10, &all).iter().map(|p| p.key.0).collect();
        pids.sort_unstable();
        assert_eq!(pids, [10, 11, 12]);
        assert!(descendants(99, &all).is_empty());
    }

    #[test]
    fn shares_are_percentages_to_a_tenth() {
        assert_eq!(share(1.0, 3.0), 33.3);
        assert_eq!(share(5.0, 0.0), 0.0);
        assert_eq!(share(7.0, 5.0), 100.0);
    }

    #[test]
    fn only_samples_after_since_are_answered() {
        let monitor = Monitor::new();
        for at in [1_000, 3_000, 5_000] {
            monitor.push(Sample { at, cpu: 1.0, pley_cpu: 0.5, memory: 10, pley_memory: 5, ..Sample::default() });
        }
        monitor.fill(None);
        assert_eq!(monitor.resources(None).samples.len(), 3);
        assert_eq!(monitor.resources(Some(3_000)).samples.iter().map(|s| s.at).collect::<Vec<_>>(), [5_000]);
        for at in 0..KEEP as i64 + 10 {
            monitor.push(Sample { at: 10_000 + at, ..Sample::default() });
            monitor.fill(None);
        }
        assert_eq!(monitor.resources(None).samples.len(), KEEP);
    }

    #[test]
    fn samples_wait_for_the_costly_readings_and_share_them() {
        let monitor = Monitor::new();
        let now = crate::util::now_ms();
        monitor.push(Sample { at: now - 6_000, cpu: 50.0, ..Sample::default() });
        monitor.fill(Some(Costly { pley_cpu: 10.0, pley_memory: 100, net_in: 1, net_out: 2 }));
        for at in [now - 4_000, now - 2_000] {
            monitor.push(Sample { at, cpu: 80.0, memory: 7, pley_out: 3, ..Sample::default() });
        }
        assert!(!monitor.watched());
        let answered = monitor.resources(None).samples;
        assert_eq!(answered.iter().map(|s| s.at).collect::<Vec<_>>(), [now - 6_000], "the newest two wait");
        assert!(monitor.watched(), "someone asked");
        assert_eq!(monitor.others_cpu(Duration::from_secs(60)), Some(40.0), "only from what is known of pley");

        monitor.fill(Some(Costly { pley_cpu: 30.0, pley_memory: 200, net_in: 4, net_out: 5 }));
        let answered = monitor.resources(Some(now - 6_000)).samples;
        let waited = Sample {
            cpu: 80.0,
            pley_cpu: 30.0,
            memory: 7,
            pley_memory: 200,
            net_in: 4,
            net_out: 5,
            pley_out: 3,
            at: 0,
        };
        assert_eq!(answered, [Sample { at: now - 4_000, ..waited }, Sample { at: now - 2_000, ..waited }]);
        assert_eq!(monitor.others_cpu(Duration::from_secs(60)), Some((40.0 + 50.0 + 50.0) / 3.0));
    }

    #[test]
    fn a_burst_of_pleys_over_waiting_samples_is_not_taken_for_others() {
        let monitor = Monitor::new();
        let now = crate::util::now_ms();
        // Pley takes 80 % in one sample of five and other programs 5 % in
        // each; the reading over the five gives pley 16 % in each.
        for (at, cpu) in [(10_000, 85.0), (8_000, 5.0), (6_000, 5.0), (4_000, 5.0), (2_000, 5.0)] {
            monitor.push(Sample { at: now - at, cpu, ..Sample::default() });
        }
        monitor.fill(Some(Costly { pley_cpu: 16.0, pley_memory: 1, net_in: 0, net_out: 0 }));
        let others = monitor.others_cpu(Duration::from_secs(60)).unwrap();
        assert!((others - 5.0).abs() < 0.01, "{others}");
    }

    #[test]
    fn processes_beginning_or_ending_are_noticed() {
        let listing = |keys: &[(u32, u64)]| keys.iter().map(|&key| (key, 0)).collect::<HashMap<_, _>>();
        let both = listing(&[(1, 10), (2, 20)]);
        assert!(!came_and_went(&both, &listing(&[(2, 20), (1, 10)])));
        assert!(came_and_went(&both, &listing(&[(1, 10)])), "one ended");
        assert!(came_and_went(&both, &listing(&[(1, 10), (2, 20), (3, 30)])), "one began");
        assert!(came_and_went(&both, &listing(&[(1, 10), (2, 25)])), "an id used again");
        assert!(came_and_went(&HashMap::new(), &both), "the first listing");
    }

    #[test]
    fn this_machine_samples() {
        if !os::SUPPORTED {
            return;
        }
        let mut sampler = Sampler::new(os::cpus());
        assert!(sampler.cpus >= 1);
        assert_eq!(sampler.sample(), None, "a share needs two readings");
        assert_eq!(sampler.costly(), None);
        // Some work of our own, so pley's share is above nothing.
        let start = Instant::now();
        let mut x = 0u64;
        while start.elapsed() < Duration::from_millis(150) {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
        std::hint::black_box(x);
        let sample = sampler.sample().expect("a sample");
        let costly = sampler.costly().expect("the costly readings");
        let memory = os::memory().unwrap();
        assert!(sample.memory > 0 && sample.memory <= memory.total);
        assert!(costly.pley_memory > 0 && costly.pley_memory < memory.total);
        assert!(costly.pley_cpu > 0.0 && costly.pley_cpu <= 100.0, "{costly:?}");
        assert!(sample.cpu >= 0.0 && sample.cpu <= 100.0);
        assert!(os::network().is_some(), "the adapters are read");
    }

    #[test]
    fn clients_are_averaged_and_a_bare_address_joins_its_sign_in() {
        let client = |user: Option<&str>, device: Option<&str>, ip: &str| Client {
            user: user.map(Into::into),
            device: device.map(Into::into),
            address: Some(ip.parse().unwrap()),
        };
        let ana_tv = client(Some("ana"), Some("Pley for Roku"), "203.0.113.5");
        let ana_player = client(None, None, "203.0.113.5");
        let stranger = client(None, None, "198.51.100.9");
        let quiet = client(Some("bo"), None, "10.0.0.2");
        let start = Instant::now();
        let totals: VecDeque<(Instant, HashMap<Client, u64>)> = [
            (start, [(ana_tv.clone(), 1_000), (quiet.clone(), 500)].into()),
            (start + Duration::from_secs(1), [(ana_tv.clone(), 1_500)].into()),
            (
                start + Duration::from_secs(10),
                [(ana_tv.clone(), 3_000), (ana_player, 8_000_000), (stranger.clone(), 50_000), (quiet, 500)].into(),
            ),
        ]
        .into();
        assert_eq!(
            client_rates(&totals),
            [
                ClientRate { client: ana_tv.clone(), sent: 800_200 },
                ClientRate { client: stranger.clone(), sent: 5_000 }
            ],
            "the player's segments are ana's, and bo, who got nothing since, isn't listed",
        );
        assert!(client_rates(&VecDeque::new()).is_empty());

        // Ana's TV names its playback "Living Room": one client, named after
        // it. Bo, another account behind the same address, is another.
        let ana_living_room = client(Some("ana"), Some("Living Room"), "203.0.113.5");
        let bo_phone = client(Some("bo"), Some("Pley for Android"), "203.0.113.5");
        let two_names: VecDeque<(Instant, HashMap<Client, u64>)> = [
            (start, [(ana_tv.clone(), 1_000)].into()),
            (
                start + Duration::from_secs(10),
                [
                    (ana_tv.clone(), 3_000),
                    (ana_living_room.clone(), 4_000_000),
                    (client(None, None, "203.0.113.5"), 8_000_000),
                    (bo_phone.clone(), 1_000_000),
                ]
                .into(),
            ),
        ]
        .into();
        assert_eq!(
            client_rates(&two_names),
            [ClientRate { client: ana_living_room, sent: 1_200_200 }, ClientRate { client: bo_phone, sent: 100_000 }],
        );

        // A Jellyfin app's player fetches the stream without naming a
        // device: the line takes its API requests' name.
        let cy_stream = client(Some("cy"), None, "198.51.100.20");
        let cy_app = client(Some("cy"), Some("Pixel 8"), "198.51.100.20");
        let unnamed: VecDeque<(Instant, HashMap<Client, u64>)> = [
            (start, HashMap::new()),
            (start + Duration::from_secs(10), [(cy_stream, 6_000_000), (cy_app.clone(), 20_000)].into()),
        ]
        .into();
        assert_eq!(client_rates(&unnamed), [ClientRate { client: cy_app, sent: 602_000 }]);

        // One forgotten and counted again from 0 since the oldest totals
        // still shows what it was sent since.
        let again: VecDeque<(Instant, HashMap<Client, u64>)> = [
            (start, [(stranger.clone(), 9_000_000)].into()),
            (start + Duration::from_secs(10), [(stranger.clone(), 40_000)].into()),
        ]
        .into();
        assert_eq!(client_rates(&again), [ClientRate { client: stranger, sent: 4_000 }]);
    }

    #[test]
    fn processes_started_here_count_as_pley() {
        if !os::SUPPORTED {
            return;
        }
        #[cfg(windows)]
        let mut child = std::process::Command::new("cmd").args(["/C", "ping -n 3 127.0.0.1 > NUL"]).spawn().unwrap();
        #[cfg(not(windows))]
        let mut child = std::process::Command::new("sleep").arg("2").spawn().unwrap();
        let tree = os::processes(std::process::id());
        let _ = child.kill();
        let _ = child.wait();
        assert!(tree.iter().any(|p| p.key.0 == std::process::id()));
        assert!(tree.iter().any(|p| p.key.0 == child.id()), "the child is pley's too");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_files_are_read() {
        assert_eq!(os::parse_cpu_times("cpu  100 10 50 800 40 0 5 0 0 0"), Some(CpuTimes { busy: 165, total: 1005 }));
        let meminfo = "MemTotal:       16000000 kB\nMemFree:         1000000 kB\nMemAvailable:    6000000 kB\n";
        assert_eq!(os::parse_meminfo(meminfo), Some(Memory { total: 16_000_000 * 1024, used: 10_000_000 * 1024 }));
        let net_dev = "Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 9999999    100    0    0    0     0          0         0  9999999     100    0    0    0     0       0          0
  eth0: 1000    10    0    0    0     0          0         0     2000      20    0    0    0     0       0          0
 wlan0:500 5 0 0 0 0 0 0 700 7 0 0 0 0 0 0
docker0: 300 3 0 0 0 0 0 0 400 4 0 0 0 0 0 0
vethab12: 300 3 0 0 0 0 0 0 400 4 0 0 0 0 0 0
";
        let hardware = |name: &str| name == "eth0" || name == "wlan0";
        assert_eq!(os::parse_net_dev(net_dev, hardware), Network { received: 1500, sent: 2700 });
        // A container sees only its own link, which isn't hardware.
        assert_eq!(os::parse_net_dev(net_dev, |_| false), Network { received: 2100, sent: 3500 });
        let stat = "42 (a (weird) name) S 7 42 42 0 -1 4194560 100 0 0 0 30 20 0 0 20 0 4 0 12345 1000000 250 18446744073709551615";
        let (parent, process) = os::parse_stat(42, stat, 10_000_000, 4096).unwrap();
        assert_eq!(parent, 7);
        assert_eq!(process, Process { key: (42, 12345), cpu_ns: 500_000_000, memory: 250 * 4096 });
    }
}
