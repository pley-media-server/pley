//! How fast the TV's network brings what it plays (the Samsung app's `Player/NetworkMeter.cs`),
//! which LG's pipeline doesn't say of its own downloads: from the bytes the TV's network
//! interfaces have received (Linux's `/proc/net/dev`, proc(5),
//! <https://man7.org/linux/man-pages/man5/proc_pid_net.5.html>), read at most once a second
//! while Info asks. A player with a full buffer takes only the video's own rate, so the fastest
//! second of the last ten is told: one in which the player was filling its buffer, as fast as the
//! network let it. What else the TV downloads at the time counts too, as it takes from the same
//! network. A TV whose jail hides the file says nothing.

use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The fastest second of this long is told.
pub const WINDOW_MS: u64 = 10_000;
/// Readings closer together than this are left out: a second's rate from less is noise.
pub const EVERY_MS: u64 = 900;
/// A reading this long after the last gives no rate: it would be the average of a time nothing
/// asked (Info closed), not a second's.
pub const GAP_MS: u64 = 3_000;
/// Where Linux lists what each network interface has received and sent.
pub const PROC_NET_DEV: &str = "/proc/net/dev";
/// The most of the file read: a TV's is well under 2 KB.
const FILE_MOST: u64 = 64 * 1024;
/// The most interfaces counted.
const INTERFACES_MOST: usize = 64;

/// The fastest second of the last [`WINDOW_MS`], from readings of the bytes received so far.
#[derive(Debug, Default)]
pub struct NetworkMeter {
    /// Each second's rate, in bits per second, by when it ended.
    rates: VecDeque<(u64, u64)>,
    /// The last reading: when, and the bytes received by then.
    last: Option<(u64, u64)>,
}

impl NetworkMeter {
    /// The bytes received so far, `received`, read at `at_ms`.
    pub fn add(&mut self, at_ms: u64, received: u64) {
        if let Some((then, before)) = self.last {
            if at_ms < then || at_ms - then < EVERY_MS {
                return;
            }
            // A counter that went back (an interface come again), or a reading after a gap, starts
            // over from here.
            if received >= before && at_ms - then <= GAP_MS {
                let bps = (received - before) as f64 * 8_000.0 / (at_ms - then) as f64;
                self.rates.push_back((at_ms, bps as u64));
            }
        }
        self.last = Some((at_ms, received));
        while self.rates.front().is_some_and(|(t, _)| t + WINDOW_MS < at_ms) {
            self.rates.pop_front();
        }
    }

    /// Bits per second, the fastest second of the last [`WINDOW_MS`]; none before two readings, or
    /// when nothing came.
    pub fn bps(&self) -> Option<u64> {
        self.rates.iter().map(|(_, bps)| *bps).max().filter(|b| *b > 0)
    }
}

/// The bytes every interface but loopback has received, from `/proc/net/dev`'s text; none when
/// it names none.
pub fn received(proc_net_dev: &str) -> Option<u64> {
    let mut total: Option<u64> = None;
    for line in proc_net_dev.lines().take(INTERFACES_MOST + 2) {
        // "  wlan0: 123456 789 0 0 ...": the name, then the bytes received first.
        let Some((name, fields)) = line.split_once(':') else { continue };
        let name = name.trim();
        if name.is_empty() || name == "lo" {
            continue;
        }
        let Some(bytes) = fields.split_whitespace().next() else { continue };
        if !bytes.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(bytes) = bytes.parse::<u64>() else { continue };
        total = Some(total.unwrap_or(0).saturating_add(bytes));
    }
    total
}

/// The TV's network speed, read from `file` ([`PROC_NET_DEV`] on the TV) as it is asked for, at
/// most once every [`EVERY_MS`] however often that is.
#[derive(Debug)]
pub struct TvNetwork {
    file: PathBuf,
    meter: NetworkMeter,
    /// When the file was last looked at, read or not.
    looked: Option<u64>,
}

impl TvNetwork {
    pub fn new(file: impl Into<PathBuf>) -> TvNetwork {
        TvNetwork { file: file.into(), meter: NetworkMeter::default(), looked: None }
    }

    /// Bits per second at `at_ms` (ms of a clock that only goes on); none while not known.
    pub fn bps(&mut self, at_ms: u64) -> Option<u64> {
        if self.looked.is_none_or(|t| at_ms.saturating_sub(t) >= EVERY_MS) {
            self.looked = Some(at_ms);
            if let Some(bytes) = read(&self.file).as_deref().and_then(received) {
                self.meter.add(at_ms, bytes);
            }
        }
        self.meter.bps()
    }
}

/// The file's text, at most [`FILE_MOST`]; none when it can't be read.
fn read(file: &Path) -> Option<String> {
    let mut text = String::new();
    fs::File::open(file).ok()?.take(FILE_MOST).read_to_string(&mut text).ok()?;
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: &str = "Inter-|   Receive                            |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets
    lo: 5000000    1000    0    0    0     0          0         0  5000000    1000
 wlan0: 125000000  90000    0    0    0     0          0         0  2000000    9000
  eth0: 0          0        0    0    0     0          0         0  0          0
";

    #[test]
    fn every_interface_but_loopback_counts() {
        assert_eq!(received(DEV), Some(125_000_000));
        assert_eq!(received("Inter-|   Receive\n face |bytes\n    lo: 5000 10 0 0\n"), None);
        assert_eq!(received(""), None);
        // What isn't a count is passed over; counts too large add up to the most there is.
        assert_eq!(received("a: -5 1\nb: x 1\nc: 18446744073709551615\nd: 7\n"), Some(u64::MAX));
    }

    #[test]
    fn the_fastest_second_of_the_last_ten_is_told() {
        let mut m = NetworkMeter::default();
        m.add(0, 0);
        assert_eq!(m.bps(), None);
        // 21 MB in a second while the player fills its buffer: 168 Mbit/s.
        m.add(1_000, 21_000_000);
        // Then only the video's own 12 MB a second (96 Mbit/s) once it's full.
        for s in 2..=10u64 {
            m.add(s * 1_000, 21_000_000 + (s - 1) * 12_000_000);
        }
        assert_eq!(m.bps(), Some(168_000_000));
        // Ten seconds on, the fast second is gone.
        m.add(11_500, 21_000_000 + 9 * 12_000_000 + 18_000_000);
        assert_eq!(m.bps(), Some(96_000_000));
    }

    #[test]
    fn readings_too_close_and_a_counter_that_went_back_are_left_out() {
        let mut m = NetworkMeter::default();
        m.add(0, 1_000_000);
        // A reading 0.2 s on is noise: left out, and the next second measured from the first.
        m.add(200, 1_500_000);
        m.add(1_000, 2_000_000);
        assert_eq!(m.bps(), Some(8_000_000));
        // The interface came again from 0: no rate from it, and the next one counts from there.
        m.add(2_000, 100);
        assert_eq!(m.bps(), Some(8_000_000));
        m.add(3_000, 4_000_100);
        assert_eq!(m.bps(), Some(32_000_000));
        // Info closed for a minute: the bytes since are no second's rate, and the next one counts.
        m.add(63_000, 64_000_100);
        assert_eq!(m.bps(), None);
        m.add(64_000, 66_000_100);
        assert_eq!(m.bps(), Some(16_000_000));
    }

    #[test]
    fn nothing_received_is_not_known() {
        let mut m = NetworkMeter::default();
        m.add(0, 500);
        m.add(1_000, 500);
        assert_eq!(m.bps(), None);
    }

    #[test]
    fn the_file_is_read_once_a_second_however_often_asked_and_unreadable_is_unknown() {
        let dir = pley_core::device::scratch_dir("net");
        let file = dir.join("dev");
        let mut tv = TvNetwork::new(&file);
        assert_eq!(tv.bps(0), None, "no file: not known");
        fs::write(&file, " eth0: 1000 1 0\n").unwrap();
        assert_eq!(tv.bps(500), None, "not looked at again so soon");
        assert_eq!(tv.bps(1_000), None, "one reading");
        fs::write(&file, " eth0: 1001000 1 0\n").unwrap();
        assert_eq!(tv.bps(1_500), None, "not read before a second has passed");
        assert_eq!(tv.bps(2_000), Some(8_000_000));
    }
}
