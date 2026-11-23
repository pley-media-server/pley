// SPDX-License-Identifier: AGPL-3.0-only
//! Arbitrary input for the code that reads what comes from outside (media
//! files, their containers and the metadata in their video): tests that it
//! never panics, never hangs and never holds memory without bound, whatever
//! it is given.
//!
//! The inputs come from a seeded generator, so every run on every computer
//! tries the same ones and a failure names the case that makes it again:
//! random bytes and text, and valid inputs changed a little (bytes changed,
//! cut out or repeated, sizes and counts set to their edges), which reach
//! further into a parser than noise does. `PLEY_FUZZ_CASES=<n>` runs `n`
//! cases of each check instead of its own few, and `PLEY_FUZZ_SEED=<n>`
//! other cases, to look harder on one's own computer.
//!
//! The server has the same file (`apps/server/src/fuzz.rs`): the two
//! programs share no code.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fmt;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// How long one case may take: a parser takes milliseconds on these inputs,
/// a debug build on a busy computer many times that, and one stuck in a loop
/// forever.
const CASE_TIME: Duration = Duration::from_secs(10);

/// The seed of every check, unless `PLEY_FUZZ_SEED` gives another.
const SEED: u64 = 0x504C_4559;

/// The most of an input a failure shows.
const SHOWN: usize = 4000;

/// A small, fast generator of test inputs (SplitMix64), never of secrets.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }

    /// True once in `n` times.
    pub fn one_in(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// A length up to `max`, as often below 10 as below 1,000: a parser's
    /// edges are at its input's start and end.
    pub fn len(&mut self, max: usize) -> usize {
        let bits = self.below((usize::BITS - max.leading_zeros()) as usize + 1);
        self.below(1usize.checked_shl(bits as u32).unwrap_or(usize::MAX).min(max.saturating_add(1)))
    }

    /// Up to `max` random bytes.
    pub fn bytes(&mut self, max: usize) -> Vec<u8> {
        let len = self.len(max);
        (0..len).map(|_| self.next() as u8).collect()
    }

    /// A character: printable ASCII most often, else one that trips text
    /// handling up (controls, combining marks, characters whose case
    /// changes their length, emoji), else any.
    pub fn char(&mut self) -> char {
        const ODD: [char; 16] = [
            '\0', '\t', '\n', '\r', '\u{7F}', '\u{85}', 'é', 'ß', 'İ', 'ﬁ', '\u{301}', '\u{200B}', '\u{FEFF}', '日',
            '😀', '\u{FFFD}',
        ];
        match self.below(8) {
            0 => char::from_u32(self.next() as u32 % 0x11_0000).unwrap_or('\u{FFFD}'),
            1 => *self.pick(&ODD),
            _ => char::from(b' ' + self.below(95) as u8),
        }
    }

    /// Text of up to `max` pieces, each one of `pieces` or a character.
    pub fn text(&mut self, pieces: &[&str], max: usize) -> String {
        let mut text = String::new();
        for _ in 0..self.len(max) {
            if pieces.is_empty() || self.one_in(6) {
                text.push(self.char());
            } else {
                text.push_str(self.pick(pieces));
            }
        }
        text
    }

    /// `valid` changed one to eight times: a byte set to anything or to an
    /// edge (0, 1, the top bit, all ones), a bit flipped, a run of bytes set
    /// to an edge (a size or a count at its smallest or largest), bytes cut
    /// out, bytes repeated elsewhere, or the end cut off. It grows to twice
    /// its length at most.
    pub fn mutate(&mut self, valid: &[u8]) -> Vec<u8> {
        const EDGES: [u8; 8] = [0x00, 0x01, 0x02, 0x7F, 0x80, 0x81, 0xFE, 0xFF];
        let most = valid.len() * 2 + 1024;
        let mut data = valid.to_vec();
        for _ in 0..1 + self.below(8) {
            if data.is_empty() {
                data = self.bytes(64);
                continue;
            }
            let at = self.below(data.len());
            let rest = data.len() - at;
            match self.below(7) {
                0 => data[at] = self.next() as u8,
                1 => data[at] = *self.pick(&EDGES),
                2 => data[at] ^= 1 << self.below(8),
                3 => {
                    let edge = *self.pick(&EDGES);
                    let end = at + 1 + self.below(8.min(rest));
                    data[at..end].fill(edge);
                }
                4 => {
                    let end = at + 1 + self.len(rest - 1);
                    data.drain(at..end);
                }
                5 => {
                    let copy = data[at..at + 1 + self.len(255.min(rest - 1))].to_vec();
                    let to = self.below(data.len() + 1);
                    data.splice(to..to, copy);
                }
                _ => data.truncate(at),
            }
            data.truncate(most);
        }
        data
    }
}

/// Bytes, shown as a byte string.
pub struct Bytes(pub Vec<u8>);

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} bytes: b\"{}\"", self.0.len(), self.0.escape_ascii())
    }
}

/// Runs `test` on `cases` inputs made by `input`, each from a seed of its
/// own, on a thread of its own, and fails at the first input that makes it
/// panic, take longer than [`CASE_TIME`] or hold more bytes at once (what it
/// allocated and hadn't freed yet) than `limit` gives for that input, saying
/// which input it was and how to make it again.
pub fn check<T: fmt::Debug + 'static>(
    name: &str,
    cases: usize,
    input: impl Fn(&mut Rng) -> T + Send + Sync + 'static,
    limit: impl Fn(&T) -> usize + Send + 'static,
    test: impl Fn(&T) + Send + 'static,
) {
    let number = |var: &str| std::env::var(var).ok().and_then(|v| v.trim().parse::<u64>().ok());
    let cases = number("PLEY_FUZZ_CASES").map_or(cases, |n| n as usize);
    let seed = number("PLEY_FUZZ_SEED").unwrap_or(SEED);
    let name_seed = name.bytes().fold(seed, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3));
    let case_seed = move |case: usize| Rng::new(name_seed ^ (case as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)).next();
    let input = Arc::new(input);
    let (started, cases_started) = mpsc::channel();
    let worker = {
        let input = input.clone();
        std::thread::Builder::new()
            .name(format!("fuzz {name}"))
            .spawn(move || {
                for case in 0..cases {
                    let _ = started.send(case);
                    let made = input(&mut Rng::new(case_seed(case)));
                    let limit = limit(&made);
                    let began = Instant::now();
                    Held::count();
                    test(&made);
                    let held = Held::most();
                    assert!(held <= limit, "it held {held} bytes at once, more than the {limit} it may");
                    let took = began.elapsed();
                    assert!(took <= CASE_TIME, "it took {took:?}, more than {CASE_TIME:?}");
                }
            })
            .expect("a thread for the cases")
    };
    let mut case = None;
    let failure = loop {
        match cases_started.recv_timeout(CASE_TIME) {
            Ok(next) => case = Some(next),
            Err(mpsc::RecvTimeoutError::Timeout) => break format!("it was still running after {CASE_TIME:?}"),
            Err(mpsc::RecvTimeoutError::Disconnected) => match worker.join() {
                Ok(()) => return,
                Err(panic) => {
                    let message = panic.downcast_ref::<String>().map(String::as_str);
                    break message.or(panic.downcast_ref::<&str>().copied()).unwrap_or("it panicked").to_owned();
                }
            },
        }
    };
    let case = case.unwrap_or_default();
    let mut shown = format!("{:?}", input(&mut Rng::new(case_seed(case))));
    if shown.len() > SHOWN {
        let end = (0..=SHOWN).rev().find(|&i| shown.is_char_boundary(i)).unwrap_or(0);
        shown.truncate(end);
        shown.push('…');
    }
    panic!(
        "{name}, case {case} of {cases} (PLEY_FUZZ_SEED={seed}): {failure}\ninput: {shown}\n\
         Fix the code it found, then add the input to the module's tests."
    );
}

/// What the thread running a case holds, counted by [`Counting`] from the
/// case's start.
struct Held;

thread_local! {
    /// Whether this thread counts, the bytes it holds since it began and the
    /// most it has held.
    static HELD: (Cell<bool>, Cell<isize>, Cell<isize>) = const { (Cell::new(false), Cell::new(0), Cell::new(0)) };
}

impl Held {
    /// Counts from now.
    fn count() {
        HELD.with(|(on, now, most)| {
            on.set(true);
            now.set(0);
            most.set(0);
        });
    }

    /// The most held since [`Held::count`].
    fn most() -> usize {
        HELD.with(|(on, _, most)| {
            on.set(false);
            most.get().max(0) as usize
        })
    }

    fn change(by: isize) {
        // Never fails for a value without a destructor.
        let _ = HELD.try_with(|(on, now, most)| {
            if on.get() {
                now.set(now.get() + by);
                most.set(most.get().max(now.get()));
            }
        });
    }
}

/// The system's allocator, telling [`Held`] what each thread allocates and
/// frees.
pub struct Counting;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

// SAFETY: every call goes to the system's allocator as it is; counting
// touches a thread-local value only, and never allocates.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's guarantees, passed on.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            Held::change(layout.size() as isize);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's guarantees, passed on.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            Held::change(layout.size() as isize);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's guarantees, passed on.
        unsafe { System.dealloc(ptr, layout) };
        Held::change(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller's guarantees, passed on.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            Held::change(new_size as isize - layout.size() as isize);
        }
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_makes_the_same_inputs() {
        let made = |seed| {
            let mut rng = Rng::new(seed);
            (rng.bytes(100), rng.text(&["a", "b"], 20), rng.mutate(b"0123456789"))
        };
        assert_eq!(made(1), made(1));
        assert_ne!(made(1), made(2));
    }

    #[test]
    fn bytes_show_as_a_byte_string() {
        assert_eq!(format!("{:?}", Bytes(b"a\0\xff".to_vec())), r#"3 bytes: b"a\x00\xff""#);
    }

    #[test]
    fn lengths_are_short_more_often() {
        let mut rng = Rng::new(7);
        let lens: Vec<usize> = (0..10_000).map(|_| rng.len(1 << 20)).collect();
        assert!(lens.iter().all(|&n| n <= 1 << 20));
        assert!(lens.iter().filter(|&&n| n < 16).count() > 1000, "short ones");
        assert!(lens.iter().filter(|&&n| n > 1 << 16).count() > 100, "long ones");
    }

    #[test]
    fn what_a_thread_holds_is_counted() {
        Held::count();
        let kept = std::hint::black_box(vec![0u8; 1 << 20]);
        drop(std::hint::black_box(vec![0u8; 2 << 20]));
        let most = Held::most();
        assert!((3 << 20..4 << 20).contains(&most), "{most}");
        drop(kept);
    }

    #[test]
    #[should_panic(expected = "panics, case 0 of")]
    fn a_panic_names_its_case() {
        check("panics", 3, |rng| rng.next(), |_| usize::MAX, |_| panic!("found"));
    }

    #[test]
    #[should_panic(expected = "more than the 1000 it may")]
    fn holding_too_much_fails() {
        check("holds", 3, |_| (), |_| 1000, |_| drop(std::hint::black_box(vec![0u8; 4000])));
    }
}
