//! Sends playbacks' reports one after the other (packages/tv/src/player/reporter.ts):
//! a player that closes still gets its last position and its session's end to
//! the server, in the order they were made (progress, then `stopped`, then
//! DELETE). Best effort: a failure is dropped, as on the web and Android.
//!
//! One thread for the whole app, started with the first report, so reports
//! never wait behind the task pool's other work and outlive the screen that
//! made them.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

type Job = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    /// Reports sent so far, and those done (run, or failed).
    sent: u64,
    done: u64,
    running: bool,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
}

/// The report queue. Cheap to clone; every clone sends through the same thread.
#[derive(Clone, Default)]
pub struct Reporter {
    shared: Arc<Shared>,
}

fn lock(m: &Mutex<Queue>) -> MutexGuard<'_, Queue> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Reporter {
    /// A queue with nothing sent yet.
    pub fn new() -> Reporter {
        Reporter::default()
    }

    /// Queues `report` after everything sent before it.
    pub fn send(&self, report: impl FnOnce() + Send + 'static) {
        let mut q = lock(&self.shared.queue);
        q.jobs.push_back(Box::new(report));
        q.sent += 1;
        if !q.running {
            q.running = true;
            let shared = self.shared.clone();
            drop(q);
            let started = thread::Builder::new().name("pley-reports".into()).spawn(move || run(&shared));
            if started.is_err() {
                // No thread to be had: the reports go now, on this thread, rather than never.
                run(&self.shared);
            }
            return;
        }
        self.shared.changed.notify_all();
    }

    /// Waits until everything queued so far went out, or `timeout` passed:
    /// true when it all went. Blocks: call it on a task thread, never the main one.
    pub fn flush(&self, timeout: Duration) -> bool {
        let until = Instant::now() + timeout;
        let mut q = lock(&self.shared.queue);
        let want = q.sent;
        while q.done < want {
            let now = Instant::now();
            if now >= until {
                return false;
            }
            q = self.shared.changed.wait_timeout(q, until - now).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
        }
        true
    }

    /// Whether every report sent so far went out.
    pub fn idle(&self) -> bool {
        let q = lock(&self.shared.queue);
        q.done == q.sent
    }
}

/// The reports' thread: runs what is queued, in order, then waits a while for
/// more before it ends (the next report starts it again).
fn run(shared: &Shared) {
    loop {
        let mut q = lock(&shared.queue);
        let job = loop {
            if let Some(job) = q.jobs.pop_front() {
                break job;
            }
            let (g, timed_out) =
                shared.changed.wait_timeout(q, Duration::from_secs(30)).unwrap_or_else(|e| e.into_inner());
            q = g;
            if timed_out.timed_out() && q.jobs.is_empty() {
                q.running = false;
                return;
            }
        };
        drop(q);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
        lock(&shared.queue).done += 1;
        shared.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_go_in_order_and_a_flush_waits_for_them() {
        let r = Reporter::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        for n in 0..5 {
            let log = log.clone();
            r.send(move || {
                thread::sleep(Duration::from_millis(2));
                log.lock().unwrap().push(n);
            });
        }
        assert!(r.flush(Duration::from_secs(5)));
        assert!(r.idle());
        assert_eq!(*log.lock().unwrap(), [0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_failing_report_doesnt_stop_the_next_and_a_flush_gives_up_in_time() {
        let r = Reporter::new();
        r.send(|| panic!("the server refused"));
        let (tx, rx) = std::sync::mpsc::channel();
        r.send(move || tx.send(()).unwrap());
        assert!(rx.recv_timeout(Duration::from_secs(5)).is_ok());
        r.send(|| thread::sleep(Duration::from_millis(300)));
        assert!(!r.flush(Duration::from_millis(20)));
        assert!(r.flush(Duration::from_secs(5)));
    }
}
