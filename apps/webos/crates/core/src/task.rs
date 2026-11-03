//! Background work: a small pool of threads that run the screens' requests,
//! promises that hand their answers back to the main loop, and the
//! cancellation that stops a request whose screen went away.
//!
//! A task runs with its [`Cancel`] as the thread's current one, so every
//! request it makes through [`crate::http`] is aborted (its socket shut down)
//! as soon as its [`Promise`] is dropped.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pley_base::Waker;

/// How many threads run tasks: the design's three (requests and JSON
/// decoding; pictures have their own workers).
pub const TASK_THREADS: usize = 3;

type Listener = Box<dyn FnOnce() + Send>;

struct CancelState {
    cancelled: bool,
    next: u64,
    listeners: Vec<(u64, Listener)>,
}

struct CancelInner {
    flag: AtomicBool,
    state: Mutex<CancelState>,
    woken: Condvar,
}

/// Stops work: a screen that went away, a newer search, the event stream
/// told to stop. Cheap to clone; every clone cancels the same work.
#[derive(Clone)]
pub struct Cancel(Arc<CancelInner>);

thread_local! {
    static CURRENT: RefCell<Option<Cancel>> = const { RefCell::new(None) };
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic elsewhere never leaves these states half-written, so a poisoned lock is still good.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Default for Cancel {
    fn default() -> Cancel {
        Cancel::new()
    }
}

impl std::fmt::Debug for Cancel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cancel({})", if self.is_cancelled() { "cancelled" } else { "live" })
    }
}

impl Cancel {
    /// A new token, not cancelled.
    pub fn new() -> Cancel {
        Cancel(Arc::new(CancelInner {
            flag: AtomicBool::new(false),
            state: Mutex::new(CancelState { cancelled: false, next: 0, listeners: Vec::new() }),
            woken: Condvar::new(),
        }))
    }

    /// Cancels: runs every listener once, wakes every [`Cancel::sleep`].
    pub fn cancel(&self) {
        let listeners = {
            let mut s = lock(&self.0.state);
            if s.cancelled {
                return;
            }
            s.cancelled = true;
            self.0.flag.store(true, Ordering::SeqCst);
            std::mem::take(&mut s.listeners)
        };
        self.0.woken.notify_all();
        for (_, f) in listeners {
            f();
        }
    }

    /// Whether this was cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }

    /// Runs `f` when this is cancelled (at once when it already is). The
    /// listener is forgotten when the guard is dropped.
    pub fn on_cancel(&self, f: impl FnOnce() + Send + 'static) -> CancelGuard {
        let mut s = lock(&self.0.state);
        if s.cancelled {
            drop(s);
            f();
            return CancelGuard { cancel: None, id: 0 };
        }
        let id = s.next;
        s.next += 1;
        s.listeners.push((id, Box::new(f)));
        CancelGuard { cancel: Some(self.clone()), id }
    }

    /// Waits `d`, or less when cancelled meanwhile: true when it waited the
    /// whole time.
    pub fn sleep(&self, d: Duration) -> bool {
        let until = Instant::now() + d;
        let mut s = lock(&self.0.state);
        loop {
            if s.cancelled {
                return false;
            }
            let now = Instant::now();
            if now >= until {
                return true;
            }
            s = self.0.woken.wait_timeout(s, until - now).unwrap_or_else(|e| e.into_inner()).0;
        }
    }

    /// The token of the task running on this thread, if any.
    pub fn current() -> Option<Cancel> {
        CURRENT.with(|c| c.borrow().clone())
    }

    /// Runs `f` with this token as the thread's current one, as tasks do.
    pub fn scope<R>(&self, f: impl FnOnce() -> R) -> R {
        let before = CURRENT.with(|c| c.borrow_mut().replace(self.clone()));
        struct Restore(Option<Cancel>);
        impl Drop for Restore {
            fn drop(&mut self) {
                let before = self.0.take();
                CURRENT.with(|c| *c.borrow_mut() = before);
            }
        }
        let _restore = Restore(before);
        f()
    }
}

/// Keeps a [`Cancel::on_cancel`] listener; dropping it forgets the listener.
pub struct CancelGuard {
    cancel: Option<Cancel>,
    id: u64,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        if let Some(c) = &self.cancel {
            lock(&c.0.state).listeners.retain(|(id, _)| *id != self.id);
        }
    }
}

/// Time as sign-in and its polling see it. Tests replace it to run a
/// ten-minute code flow in a moment.
pub trait Clock: Send + Sync {
    /// Wall time in milliseconds since 1970: token expiries are kept in it.
    fn now_ms(&self) -> u64;
    /// Waits `d` unless `cancel` fires first: true when it waited the whole time.
    fn sleep(&self, d: Duration, cancel: &Cancel) -> bool;
}

/// The real clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
    }

    fn sleep(&self, d: Duration, cancel: &Cancel) -> bool {
        cancel.sleep(d)
    }
}

/// Where a promise's answer is kept.
struct Slot<T> {
    value: Mutex<SlotState<T>>,
}

enum SlotState<T> {
    Pending,
    Ready(T),
    Taken,
    /// The task panicked or was skipped: no answer will come.
    Failed,
}

/// The answer of a task, collected by the main loop once [`Promise::ready`].
/// Dropping it cancels the task (and the requests it is making).
pub struct Promise<T> {
    slot: Arc<Slot<T>>,
    cancel: Cancel,
}

impl<T> Promise<T> {
    /// A promise that already holds its answer (a cached one).
    pub fn resolved(value: T) -> Promise<T> {
        Promise { slot: Arc::new(Slot { value: Mutex::new(SlotState::Ready(value)) }), cancel: Cancel::new() }
    }

    /// Whether the answer is here to take.
    pub fn ready(&self) -> bool {
        matches!(*lock(&self.slot.value), SlotState::Ready(_))
    }

    /// The answer, once: `None` until it is ready, and after it was taken.
    pub fn take(&mut self) -> Option<T> {
        let mut v = lock(&self.slot.value);
        match std::mem::replace(&mut *v, SlotState::Taken) {
            SlotState::Ready(value) => Some(value),
            other => {
                *v = other;
                None
            }
        }
    }

    /// No answer will ever come: the task panicked, or was cancelled before it ran.
    pub fn failed(&self) -> bool {
        matches!(*lock(&self.slot.value), SlotState::Failed)
    }

    /// Stops the task: one not started yet never runs; one running has its requests aborted.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// The task's cancellation token.
    pub fn token(&self) -> &Cancel {
        &self.cancel
    }

    /// Blocks until the answer is here or `timeout` passes: for tests and for
    /// the last reports before the app closes, never for the main loop.
    pub fn wait(mut self, timeout: Duration) -> Option<T> {
        let until = Instant::now() + timeout;
        loop {
            if let Some(v) = self.take() {
                return Some(v);
            }
            if self.failed() || Instant::now() >= until {
                return None;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

impl<T> Drop for Promise<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl<T> std::fmt::Debug for Promise<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.ready() { "Promise(ready)" } else { "Promise(pending)" })
    }
}

type Job = Box<dyn FnOnce() + Send>;

struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    more: Condvar,
    closed: AtomicBool,
}

/// Ends the workers when the last [`Tasks`] handle goes.
struct Owner(Arc<Queue>);

impl Drop for Owner {
    fn drop(&mut self) {
        self.0.closed.store(true, Ordering::SeqCst);
        self.0.more.notify_all();
    }
}

/// The task pool: three threads running requests and decoding answers for
/// screens and the app's state. Cheap to clone.
#[derive(Clone)]
pub struct Tasks {
    queue: Arc<Queue>,
    _owner: Arc<Owner>,
    waker: Waker,
}

impl Tasks {
    /// The pool, waking the main loop with `waker` whenever a task finishes.
    pub fn new(waker: Waker) -> Tasks {
        Tasks::with_threads(TASK_THREADS, waker)
    }

    /// A pool of `threads` threads (at least one).
    pub fn with_threads(threads: usize, waker: Waker) -> Tasks {
        let queue = Arc::new(Queue { jobs: Mutex::new(VecDeque::new()), more: Condvar::new(), closed: false.into() });
        for n in 0..threads.max(1) {
            let q = queue.clone();
            thread::Builder::new()
                .name(format!("pley-task-{n}"))
                .spawn(move || work(&q))
                .expect("the task threads start");
        }
        Tasks { _owner: Arc::new(Owner(queue.clone())), queue, waker }
    }

    /// Runs `f` on a worker; its answer comes back through the promise, and
    /// the main loop is woken when it is there.
    pub fn spawn<T, F>(&self, f: F) -> Promise<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let slot = Arc::new(Slot { value: Mutex::new(SlotState::Pending) });
        let cancel = Cancel::new();
        let (s, c, waker) = (slot.clone(), cancel.clone(), self.waker.clone());
        self.push(Box::new(move || {
            if c.is_cancelled() {
                *lock(&s.value) = SlotState::Failed;
                return;
            }
            let answer = catch_unwind(AssertUnwindSafe(|| c.scope(f)));
            *lock(&s.value) = match answer {
                Ok(v) => SlotState::Ready(v),
                Err(_) => SlotState::Failed,
            };
            waker.wake();
        }));
        Promise { slot, cancel }
    }

    /// Runs `f` on a worker with nothing to hand back and nothing that cancels
    /// it: progress reports and the end of a session, which outlive the screen
    /// that made them.
    pub fn detach(&self, f: impl FnOnce() + Send + 'static) {
        self.push(Box::new(move || {
            let _ = catch_unwind(AssertUnwindSafe(f));
        }));
    }

    /// The waker tasks call when they finish.
    pub fn waker(&self) -> &Waker {
        &self.waker
    }

    fn push(&self, job: Job) {
        lock(&self.queue.jobs).push_back(job);
        self.queue.more.notify_one();
    }
}

fn work(q: &Queue) {
    loop {
        let job = {
            let mut jobs = lock(&q.jobs);
            loop {
                if let Some(job) = jobs.pop_front() {
                    break job;
                }
                if q.closed.load(Ordering::SeqCst) {
                    return;
                }
                jobs = q.more.wait(jobs).unwrap_or_else(|e| e.into_inner());
            }
        };
        job();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn a_promise_brings_the_answer_and_wakes_the_loop() {
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let tasks = Tasks::new(Waker::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        let p = tasks.spawn(|| 6 * 7);
        assert_eq!(p.wait(Duration::from_secs(5)), Some(42));
        assert_eq!(woken.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn take_gives_the_answer_once() {
        let mut p = Promise::resolved("hi");
        assert!(p.ready());
        assert_eq!(p.take(), Some("hi"));
        assert!(!p.ready());
        assert_eq!(p.take(), None);
    }

    #[test]
    fn dropping_a_promise_cancels_its_task() {
        let tasks = Tasks::with_threads(1, Waker::noop());
        let (seen_tx, seen_rx) = std::sync::mpsc::channel();
        let gate = Cancel::new();
        let g = gate.clone();
        // Hold the only worker so the next task waits in the queue.
        let blocker = tasks.spawn(move || {
            g.sleep(Duration::from_secs(5));
        });
        let p = tasks.spawn(move || {
            seen_tx.send(()).ok();
        });
        drop(p);
        gate.cancel();
        assert!(blocker.wait(Duration::from_secs(5)).is_some());
        assert!(seen_rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn a_running_task_sees_its_cancel_as_current() {
        let tasks = Tasks::with_threads(1, Waker::noop());
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let p = tasks.spawn(move || {
            let cancel = Cancel::current().expect("a task has a current token");
            started_tx.send(()).ok();
            // Returns early once the promise is dropped.
            !cancel.sleep(Duration::from_secs(10))
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let token = p.token().clone();
        drop(p);
        assert!(token.is_cancelled());
    }

    #[test]
    fn listeners_run_once_and_guards_forget_them() {
        let c = Cancel::new();
        let runs = Arc::new(AtomicUsize::new(0));
        let r = runs.clone();
        let _kept = c.on_cancel(move || {
            r.fetch_add(1, Ordering::SeqCst);
        });
        let r = runs.clone();
        drop(c.on_cancel(move || {
            r.fetch_add(10, Ordering::SeqCst);
        }));
        c.cancel();
        c.cancel();
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        // Already cancelled: runs at once.
        let r = runs.clone();
        let _late = c.on_cancel(move || {
            r.fetch_add(100, Ordering::SeqCst);
        });
        assert_eq!(runs.load(Ordering::SeqCst), 101);
    }

    #[test]
    fn a_panicking_task_fails_without_killing_its_thread() {
        let tasks = Tasks::with_threads(1, Waker::noop());
        let p = tasks.spawn(|| -> u32 { panic!("boom") });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !p.failed() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(p.failed());
        assert_eq!(tasks.spawn(|| 1).wait(Duration::from_secs(5)), Some(1));
    }
}
