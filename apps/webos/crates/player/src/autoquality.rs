//! Automatic quality (docs/QUALITY.md; packages/tv/src/player/autoquality.ts):
//! the original, unless it doesn't play well; then the quality that does. The
//! player tells it what it sees (waits for data, how fast video arrives, how
//! fast the network loads, a decoder falling behind) and starts again at the
//! quality it names. Nothing here knows the player or the clock: times come
//! in, in ms.
//!
//! LG's pipeline goes black between streams (`blanks`, docs/QUALITY.md
//! "Players that go black between streams"): a converted stream's bitrate
//! changes as it is sent ([`LiveStream`]), and the original moves only once
//! its picture has stopped.

use std::collections::HashSet;

/// The original's id.
pub const ORIGINAL_ID: &str = "original";

/// A wait for data this long is a play issue; two shorter ones within a minute are one too.
pub const LONG_WAIT_MS: i64 = 2_000;
/// Two shorter waits within this are a play issue.
pub const SHORT_WAITS_WITHIN_MS: i64 = 60_000;
/// The original as the file is moves for a wait only where the network, over
/// the last 2 minutes, isn't shown to carry this many times its bitrate: on a
/// network that does, the server is why it waits (its disk, a remux catching
/// up), which a conversion only slows down.
pub const ORIGINAL_NETWORK: f64 = 1.5;
/// How far back the network is looked at for the original as the file is.
pub const ORIGINAL_NETWORK_MS: i64 = 2 * 60_000;
/// Where nothing measured the network, the original moves at the second such wait within this.
pub const ORIGINAL_WAITS_WITHIN_MS: i64 = 5 * 60_000;
/// Delivery is measured over this much time, once there is at least the second figure of it.
pub const DELIVERY_WINDOW_MS: i64 = 20_000;
/// Delivery says nothing before it spans this long.
pub const DELIVERY_MIN_SPAN_MS: i64 = 5_000;
/// Video arriving slower than 1.1 s a second, with under 10 s ahead and falling, for 15 s: a play issue before it waits.
pub const DELIVERY_LOW: f64 = 1.1;
/// Low delivery this long is an issue.
pub const DELIVERY_LOW_FOR_MS: i64 = 15_000;
/// Low delivery counts with less than this many seconds loaded ahead.
pub const DELIVERY_AHEAD_S: f64 = 10.0;
/// The new ceiling keeps a quarter in hand.
pub const SAFETY: f64 = 0.75;
/// No change comes sooner than this after the last.
pub const CHANGE_GAP_MS: i64 = 20_000;
/// A step back up needs this long without an issue, and this long since the last step down.
pub const CALM_MS: i64 = 3 * 60_000;
/// A step back up waits this long after the last step down.
pub const AFTER_DOWN_MS: i64 = 5 * 60_000;
/// The network must carry this many times the next quality up.
pub const HEADROOM: f64 = 1.6;
/// A step up a step down undoes this soon was one too far: it isn't tried again during the title.
pub const SETTLE_MS: i64 = 10 * 60_000;
/// Each step up undone doubles the calm the next one waits for, up to this.
pub const MAX_CALM_MS: i64 = 4 * CALM_MS;
/// On a player that goes black, frames the original drops move it in this much of a title's playing only.
pub const FIRST_MINUTE_MS: i64 = 60_000;
/// A slow network is remembered this long, per device and server.
pub const MEMORY_MS: i64 = 30 * 60_000;
/// Network figures older than this go.
const KEEP_MS: i64 = 6 * 60_000;
/// "Long ago": before anything happened.
const NEVER: i64 = -1_000_000_000_000_000;

/// A quality Automatic can play below the original: highest first, each lower than the one before.
#[derive(Clone, Debug, PartialEq)]
pub struct Rung {
    pub id: String,
    pub label: String,
    pub kbps: f64,
    pub height: u32,
}

/// Why Automatic went below the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lowered {
    Network,
    Device,
}

/// A change Automatic decided on: the player starts again at `to`, or its stream goes on at `bitrate`.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoStep {
    /// "original" or a rung's id.
    pub to: String,
    /// Why it went down; none for a step back up.
    pub why: Option<Lowered>,
    /// A stream whose bitrate changes as it is sent ([`LiveStream`]): it goes
    /// on at this many kbps, the same stream and size, and `to` is where it
    /// plays still. None for a new stream.
    pub bitrate: Option<f64>,
}

/// A converted stream whose bitrate changes as it is sent
/// (`PlaybackInfo.live_bitrate`, `PUT /api/playback/{id}/bitrate`): its own
/// bitrate, the most it goes back up to, and the least its size keeps
/// (8 Mbps for 4K, 3 for 1080p), kbps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveStream {
    pub top: f64,
    pub floor: f64,
}

/// What the network carried when it last stepped down (kbps), and when (ms since 1970).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Remembered {
    pub kbps: f64,
    pub at: i64,
}

/// Where the TV keeps its memory of a slow network (per server).
pub trait AutoMemory: Send {
    fn get(&self) -> Option<Remembered>;
    fn set(&mut self, value: Option<Remembered>);
}

/// A memory kept in this run only (tests, and a store that can't be written).
#[derive(Clone, Debug, Default)]
pub struct HeldMemory(pub Option<Remembered>);

impl AutoMemory for HeldMemory {
    fn get(&self) -> Option<Remembered> {
        self.0
    }

    fn set(&mut self, value: Option<Remembered>) {
        self.0 = value;
    }
}

/// How an [`AutoQuality`] is made.
pub struct AutoOptions {
    /// The qualities below the original it may play (see [`Rung`]).
    pub ladder: Vec<Rung>,
    /// What the original plays at, kbps: the file's bitrate, or the server's quality when it converts it; none when unknown.
    pub original_kbps: Option<f64>,
    pub memory: Option<Box<dyn AutoMemory>>,
    /// The time the memory is kept in (ms since 1970).
    pub wall: Box<dyn Fn() -> i64 + Send>,
    /// The remembered ceiling changed (kbps; none: forgotten), for the server
    /// to keep for this network too (docs/QUALITY.md).
    pub on_ceiling: Option<Box<dyn FnMut(Option<f64>) + Send>>,
    /// The player goes black between streams (LG's pipeline): it never moves
    /// to another stream while one plays well. A stream whose bitrate changes
    /// as it is sent steps that; any other moves only once its picture has
    /// stopped (a wait of 2 s or more, a decoder failing), and never back up.
    pub blanks: bool,
}

impl AutoOptions {
    /// Options for `ladder` and `original_kbps`, with no memory, the system's wall clock and a player that doesn't go black.
    pub fn new(ladder: Vec<Rung>, original_kbps: Option<f64>) -> AutoOptions {
        AutoOptions { ladder, original_kbps, memory: None, wall: Box::new(wall_ms), on_ceiling: None, blanks: false }
    }
}

/// Milliseconds since 1970 by the system's clock.
pub fn wall_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// One title's Automatic: where it plays now, and when to change. Made for
/// each title; [`AutoQuality::start`] says where it begins.
pub struct AutoQuality {
    /// "original" or a rung's id.
    pub playing: String,
    /// Why it plays below the original; none at the original.
    pub lowered: Option<Lowered>,
    /// The bitrate a stream whose bitrate changes as it is sent goes on at (kbps); none for any other stream.
    pub live_kbps: Option<f64>,

    ladder: Vec<Rung>,
    original_kbps: Option<f64>,
    /// The original plays converted ([`AutoQuality::set_original_kbps`]), not as the file is.
    converted: bool,
    memory: Option<Box<dyn AutoMemory>>,
    wall: Box<dyn Fn() -> i64 + Send>,
    on_ceiling: Option<Box<dyn FnMut(Option<f64>) + Send>>,
    blanks: bool,
    begun: bool,

    last_change: i64,
    last_down: i64,
    last_issue: i64,
    pending: Option<Lowered>,
    /// The last step up, until a step down or `SETTLE_MS` says whether it held.
    last_up: Option<(i64, String)>,
    /// Qualities a step down undid soon after a step up to them: not tried again this title.
    too_high: HashSet<String>,
    /// The calm a step up waits for: `CALM_MS`, doubled by each step up undone.
    calm_ms: i64,

    /// The stream playing, when its bitrate changes as it is sent; and the quality it was asked at.
    live: Option<LiveStream>,
    live_of: Option<String>,
    /// Where the last bitrate step started from, until the server says it took it.
    live_undo: Option<(f64, Option<Lowered>)>,
    /// Frames dropped where it doesn't move have lowered where the next title starts.
    struggle_kept: bool,

    wait_from: Option<i64>,
    /// The wait under way lasted 2 s (`long_wait`): what's left of it is no short wait.
    wait_long: bool,
    /// The wait under way came on a network nothing measured, and is among `original_waits`.
    wait_unmeasured: bool,
    /// When the original's long waits came on a network nothing measured.
    original_waits: Vec<i64>,
    short_waits: Vec<i64>,
    fills: Vec<(i64, f64)>,
    /// The last delivery measured, seconds of video a second; none while unknown.
    delivery: Option<f64>,
    low_from: Option<i64>,
    low_ahead: f64,
    loads: Vec<(i64, f64, f64)>,
    rates: Vec<(i64, f64)>,
    /// What the network carried at least, from the buffer filling faster than it plays.
    floors: Vec<(i64, f64)>,
}

impl AutoQuality {
    /// A title's Automatic.
    pub fn new(o: AutoOptions) -> AutoQuality {
        AutoQuality {
            playing: ORIGINAL_ID.into(),
            lowered: None,
            live_kbps: None,
            ladder: o.ladder,
            original_kbps: o.original_kbps.filter(|k| *k > 0.0),
            converted: false,
            memory: o.memory,
            wall: o.wall,
            on_ceiling: o.on_ceiling,
            blanks: o.blanks,
            begun: false,
            last_change: NEVER,
            last_down: NEVER,
            last_issue: NEVER,
            pending: None,
            last_up: None,
            too_high: HashSet::new(),
            calm_ms: CALM_MS,
            live: None,
            live_of: None,
            live_undo: None,
            struggle_kept: false,
            wait_from: None,
            wait_long: false,
            wait_unmeasured: false,
            original_waits: Vec::new(),
            short_waits: Vec::new(),
            fills: Vec::new(),
            delivery: None,
            low_from: None,
            low_ahead: 0.0,
            loads: Vec::new(),
            rates: Vec::new(),
            floors: Vec::new(),
        }
    }

    /// Whether [`AutoQuality::start`] has said where it begins.
    pub fn started(&self) -> bool {
        self.begun
    }

    /// The qualities below the original it may play.
    pub fn ladder(&self) -> &[Rung] {
        &self.ladder
    }

    /// Where it starts: the original, or below it when a network too slow for
    /// it was found in the last 30 minutes. `now` is the player's clock.
    /// `known`: what the server knows of this network instead (`Some(None)`:
    /// no limit), which keeps its own time; none when it can't say.
    pub fn start(&mut self, now: i64, known: Option<Option<Remembered>>) -> String {
        self.begun = true;
        self.last_issue = now;
        let from_server = known.is_some();
        let kept = match known {
            Some(k) => k,
            None => self.memory.as_ref().and_then(|m| m.get()),
        };
        let wall = (self.wall)();
        // A clock set back a little since doesn't make it older than now.
        let age = kept.map_or(0, |k| (wall - k.at).max(0));
        if let Some(k) = kept {
            if !from_server && (age >= MEMORY_MS || k.at - wall >= MEMORY_MS) {
                if let Some(m) = self.memory.as_mut() {
                    m.set(None);
                }
            } else if !self.ladder.is_empty() && self.original_kbps.is_none_or(|o| k.kbps < o) {
                self.playing = self.highest_at_most(Some(k.kbps), None).expect("the ladder isn't empty").id.clone();
                self.lowered = Some(Lowered::Network);
                self.last_down = now - age;
            }
        }
        self.playing.clone()
    }

    /// On a player that goes black, a stream whose bitrate can't change as it
    /// plays (the original, a remux, an older server's conversion): only a
    /// stopped picture moves it.
    pub fn patient(&self) -> bool {
        self.blanks && self.live.is_none()
    }

    /// On a player that goes black, the original as the file is (direct play or a remux).
    fn as_the_file(&self) -> bool {
        self.patient() && self.playing == ORIGINAL_ID && !self.converted
    }

    /// The stream playing goes on below its own bitrate.
    pub fn live_lowered(&self) -> bool {
        matches!((self.live, self.live_kbps), (Some(l), Some(k)) if k < l.top)
    }

    /// The own bitrate of the stream playing, when it changes as it is sent (kbps).
    pub fn live_top(&self) -> Option<f64> {
        self.live.map(|l| l.top)
    }

    /// The bitrate playing now (kbps); none for an original of unknown bitrate.
    pub fn playing_kbps(&self) -> Option<f64> {
        if let (Some(_), Some(k)) = (self.live, self.live_kbps) {
            return Some(k);
        }
        self.rung(&self.playing).map(|r| r.kbps).or(self.original_kbps)
    }

    /// The original plays converted (a fallback of the player's, or a file it can't play), at `kbps`.
    pub fn set_original_kbps(&mut self, kbps: Option<f64>) {
        self.converted = true;
        if let Some(k) = kbps.filter(|k| *k > 0.0) {
            self.original_kbps = Some(k);
        }
    }

    /// A new session began (a change, a recovery): what was measured of the
    /// one before goes, but the network's speed. `live`: its bitrate changes
    /// as it is sent. A new stream of the same quality (a recovery, a track)
    /// goes on at the bitrate the last one had come to: `live_kbps`, for the
    /// player to ask for when below its own.
    pub fn began(&mut self, live: Option<LiveStream>) {
        self.wait_from = None;
        self.short_waits.clear();
        self.pending = None;
        self.interrupted();
        let kept = match (live, self.live, self.live_kbps) {
            (Some(new), Some(old), Some(k)) if self.live_of.as_deref() == Some(&self.playing) && old.top == new.top => {
                Some(k)
            }
            _ => None,
        };
        self.live = live;
        self.live_of = live.map(|_| self.playing.clone());
        self.live_kbps = live.map(|l| match kept {
            None => l.top,
            Some(k) => l.floor.max(l.top.min(k)),
        });
        self.live_undo = None;
    }

    /// The server says what the stream goes on at (kbps) after the step that
    /// asked for `asked`. Less than asked: the stream's own bitrate is lower
    /// than its quality's, and that is as high as it goes.
    pub fn live_at(&mut self, kbps: f64, asked: f64) {
        self.live_undo = None;
        let Some(live) = self.live else { return };
        if kbps <= 0.0 || kbps.is_nan() {
            return;
        }
        if kbps < asked {
            self.live = Some(LiveStream { top: live.top.min(kbps), floor: live.floor.min(kbps) });
        }
        self.live_kbps = Some(kbps);
        if self.playing == ORIGINAL_ID && !self.live_lowered() {
            self.lowered = None;
        }
    }

    /// The server didn't change the bitrate: the stream goes on as it was.
    /// `final` (it can't, an older transcoder): from now on it is a stream
    /// whose bitrate can't change.
    pub fn live_refused(&mut self, last: bool) {
        if let Some((kbps, lowered)) = self.live_undo.take()
            && self.live.is_some()
        {
            self.live_kbps = Some(kbps);
            self.lowered = lowered;
        }
        if last {
            self.live = None;
            self.live_of = None;
            self.live_kbps = None;
        }
    }

    /// Paused, seeking, or out of sight: what it was measuring starts over, and a wait under way isn't one.
    pub fn interrupted(&mut self) {
        self.wait_from = None;
        self.wait_long = false;
        self.wait_unmeasured = false;
        self.fills.clear();
        self.delivery = None;
        self.low_from = None;
    }

    /// The picture stopped for want of data while meant to play (not after a seek, a pause or a new session).
    pub fn waiting(&mut self, now: i64) {
        if self.wait_from.is_some() {
            return;
        }
        self.wait_from = Some(now);
        self.wait_long = false;
        self.wait_unmeasured = false;
    }

    /// Whether the wait under way, 2 s long by `now`, is a play issue. At the
    /// original as the file is, only on a network not measured at
    /// `ORIGINAL_NETWORK` times its bitrate; where nothing measured the
    /// network, the second such wait within 5 minutes.
    fn long_wait(&mut self, now: i64) -> bool {
        self.wait_long = true;
        if !self.as_the_file() {
            return true;
        }
        let net = self.network(now - ORIGINAL_NETWORK_MS);
        if let (Some(net), Some(original)) = (net, self.original_kbps) {
            return net < ORIGINAL_NETWORK * original;
        }
        if !self.wait_unmeasured {
            self.wait_unmeasured = true;
            self.original_waits.retain(|t| now - t < ORIGINAL_WAITS_WITHIN_MS);
            self.original_waits.push(now);
        }
        self.original_waits.len() >= 2
    }

    /// It plays on after a wait.
    pub fn resumed(&mut self, now: i64) {
        let Some(from) = self.wait_from.take() else { return };
        if now - from >= LONG_WAIT_MS && self.long_wait(now) {
            self.pending = self.pending.or(Some(Lowered::Network));
        }
        let long = self.wait_long;
        self.wait_long = false;
        self.wait_unmeasured = false;
        // Short waits don't move a stream that can't change as it plays, on a player that goes black.
        if long || self.patient() {
            return;
        }
        let mut recent: Vec<i64> =
            self.short_waits.iter().copied().filter(|t| now - t < SHORT_WAITS_WITHIN_MS).collect();
        recent.push(now);
        if recent.len() >= 2 {
            self.pending = self.pending.or(Some(Lowered::Network));
            self.short_waits.clear();
        } else {
            self.short_waits = recent;
        }
    }

    /// Where the playhead is and where what's loaded ahead of it ends, in
    /// seconds, while it plays; `buffered_end` none when the player can't say.
    pub fn sample(&mut self, now: i64, position: f64, buffered_end: Option<f64>) {
        let Some(end) = buffered_end.filter(|e| e.is_finite()) else { return };
        if self.fills.last().is_some_and(|l| end < l.1 - 0.5) {
            self.fills.clear();
        }
        self.fills.push((now, end));
        while self.fills.len() > 2 && now - self.fills[1].0 >= DELIVERY_WINDOW_MS {
            self.fills.remove(0);
        }
        let first = self.fills[0];
        let span = now - first.0;
        if span < DELIVERY_MIN_SPAN_MS {
            return;
        }
        let delivery = ((end - first.1) / (span as f64 / 1000.0)).max(0.0);
        self.delivery = Some(delivery);
        let ahead = (end - position).max(0.0);
        // Filling faster than it plays: the network carries at least that much.
        if delivery > 1.0
            && let Some(kbps) = self.playing_kbps()
        {
            self.floor(now, delivery * kbps);
        }
        if delivery < DELIVERY_LOW && ahead < DELIVERY_AHEAD_S {
            match self.low_from {
                None => {
                    self.low_from = Some(now);
                    self.low_ahead = ahead;
                }
                Some(from) if now - from >= DELIVERY_LOW_FOR_MS => {
                    // Nor does slow delivery move a patient stream: the picture hasn't stopped.
                    if ahead < self.low_ahead && !self.patient() {
                        self.pending = self.pending.or(Some(Lowered::Network));
                    }
                    self.low_from = Some(now);
                    self.low_ahead = ahead;
                }
                Some(_) => {}
            }
        } else {
            self.low_from = None;
        }
    }

    /// A download of `bytes` took `ms` (an HLS segment).
    pub fn loaded(&mut self, now: i64, bytes: f64, ms: f64) {
        if bytes > 0.0 && ms > 0.0 {
            self.loads.push((now, bytes, ms));
        }
        self.trim(now);
    }

    /// The player measured its network at `kbps`.
    pub fn bandwidth(&mut self, now: i64, kbps: f64) {
        if kbps > 0.0 {
            self.rates.push((now, kbps));
        }
        self.trim(now);
    }

    /// The network carried at least `kbps` (the buffer filled that much
    /// faster than the video plays). A full buffer fills only as fast as it
    /// plays, so most of these are far below what the network carries: their
    /// highest says the most (LG's pipeline tells nothing else of its network).
    fn floor(&mut self, now: i64, kbps: f64) {
        self.floors.push((now, kbps));
        self.trim(now);
    }

    /// The decoder fell behind (frames dropped, a decoder failing) while it converts.
    pub fn decoding(&mut self) {
        self.pending = Some(Lowered::Device);
    }

    /// Frames dropped where Automatic doesn't move for them (a player that
    /// goes black, past the title's first minute): this title plays on, and
    /// the next starts a step lower, through the network's memory. Once a title.
    pub fn struggled(&mut self) {
        if self.struggle_kept {
            return;
        }
        let kbps = self.playing_kbps();
        let ceiling = kbps.map(|k| (k * SAFETY).round());
        let Some(target) = self.highest_at_most(ceiling, kbps).map(|r| r.kbps) else { return };
        let Some(ceiling) = ceiling else { return };
        self.struggle_kept = true;
        self.remember(Some(ceiling.max(target)));
    }

    /// The player itself turned the original into a conversion (it couldn't play it well): a change, as one of its own.
    pub fn fell_back(&mut self, now: i64) {
        self.last_change = now;
        self.last_issue = now;
        self.pending = None;
    }

    /// How fast the network carried data since `since`, kbps: its downloads,
    /// the player's own figures (their median), or at least the most a refill
    /// of the buffer showed; none when nothing says.
    pub fn network(&self, since: i64) -> Option<f64> {
        let (mut bytes, mut ms, mut count) = (0.0, 0.0, 0);
        for &(at, b, m) in &self.loads {
            if at < since {
                continue;
            }
            bytes += b;
            ms += m;
            count += 1;
        }
        let mut best = (count >= 2 && ms > 0.0).then(|| bytes * 8.0 / ms);
        let mut said: Vec<f64> = self.rates.iter().filter(|r| r.0 >= since).map(|r| r.1).collect();
        if !said.is_empty() {
            said.sort_by(f64::total_cmp);
            let median = said[said.len() / 2];
            if best.is_none_or(|b| median > b) {
                best = Some(median);
            }
        }
        for &(at, kbps) in &self.floors {
            if at >= since && best.is_none_or(|b| kbps > b) {
                best = Some(kbps);
            }
        }
        best
    }

    /// What to do now: a step down after a play issue, a step back up after a
    /// calm spell, or nothing. The step is taken here; the player starts again
    /// at `to`.
    pub fn check(&mut self, now: i64) -> Option<AutoStep> {
        if self.wait_from.is_some_and(|from| now - from >= LONG_WAIT_MS) && self.long_wait(now) {
            self.pending = self.pending.or(Some(Lowered::Network));
            // Waiting on: another 2 s is another issue.
            self.wait_from = Some(now);
        }
        if let Some(issue) = self.pending.take() {
            self.last_issue = now;
            if now - self.last_change < CHANGE_GAP_MS {
                return None;
            }
            return if self.live.is_some() { self.live_down(now, issue) } else { self.step_down(now, issue) };
        }
        if self.live.is_some() {
            return self.live_up(now);
        }
        // A player that goes black doesn't leave a stream that plays well for another.
        if self.blanks { None } else { self.step_up(now) }
    }

    fn delivery_for(&self, now: i64, why: Lowered, kbps: f64) -> f64 {
        if why != Lowered::Network {
            return 1.0;
        }
        let net = self.network(now - DELIVERY_WINDOW_MS);
        let delivery = match (self.delivery, net) {
            (Some(d), _) => d,
            (None, Some(n)) => n / kbps,
            (None, None) => 1.0,
        };
        delivery.min(1.0)
    }

    fn step_down(&mut self, now: i64, why: Lowered) -> Option<AutoStep> {
        let kbps = self.playing_kbps();
        let ceiling = match kbps {
            Some(k) => Some((k * self.delivery_for(now, why, k) * SAFETY).round()),
            // An original of unknown bitrate: what the network carries, when it says.
            None => self.network(now - DELIVERY_WINDOW_MS).map(|n| (n * SAFETY).round()),
        };
        let target = self.highest_at_most(ceiling, kbps)?.clone();
        if target.id == self.playing {
            return None;
        }
        // Down again from where it just stepped up to: that was one step too far.
        if self.last_up.as_ref().is_some_and(|(at, to)| *to == self.playing && now - at < SETTLE_MS) {
            self.too_high.insert(self.playing.clone());
            self.calm_ms = MAX_CALM_MS.min(self.calm_ms * 2);
        }
        self.last_up = None;
        self.playing = target.id.clone();
        self.lowered = Some(why);
        self.last_change = now;
        self.last_down = now;
        self.interrupted();
        if why == Lowered::Network {
            self.remember(Some(ceiling.map_or(target.kbps, |c| c.max(target.kbps))));
        }
        Some(AutoStep { to: target.id, why: Some(why), bitrate: None })
    }

    fn step_up(&mut self, now: i64) -> Option<AutoStep> {
        if self.playing == ORIGINAL_ID {
            return None;
        }
        let calm_from = self.last_issue.max(self.last_change);
        if now - calm_from < self.calm_ms || now - self.last_down < AFTER_DOWN_MS {
            return None;
        }
        let at = self.index_of(&self.playing);
        let next = (at > 0).then(|| self.ladder[(at - 1) as usize].clone());
        // Settled: the step above didn't hold this title.
        if self.too_high.contains(next.as_ref().map_or(ORIGINAL_ID, |n| n.id.as_str())) {
            return None;
        }
        let need = next.as_ref().map(|n| n.kbps).or(self.original_kbps)?;
        let net = self.network(calm_from)?;
        if net < HEADROOM * need {
            return None;
        }
        self.playing = next.as_ref().map_or(ORIGINAL_ID.to_string(), |n| n.id.clone());
        self.last_change = now;
        self.last_up = Some((now, self.playing.clone()));
        self.interrupted();
        match &next {
            None => {
                self.lowered = None;
                self.remember(None);
            }
            // The network carries this one now: the next title starts here rather than lower.
            Some(n) if self.lowered == Some(Lowered::Network) => self.remember(Some(n.kbps)),
            Some(_) => {}
        }
        Some(AutoStep { to: self.playing.clone(), why: None, bitrate: None })
    }

    /// A step down of a stream whose bitrate changes as it is sent: the same
    /// ceiling as a new stream's, in whole Mbps, within the least its size
    /// keeps and its own bitrate. At that least already, the network waits.
    fn live_down(&mut self, now: i64, why: Lowered) -> Option<AutoStep> {
        let live = self.live?;
        let kbps = self.live_kbps.unwrap_or(live.top);
        let ceiling = (kbps * self.delivery_for(now, why, kbps) * SAFETY).round();
        let to = live.floor.max(live.top.min((ceiling / 1000.0).floor() * 1000.0));
        if to >= kbps {
            return None;
        }
        // Down again from where it just stepped up to: that was one step too far.
        let place = self.place(self.live_kbps);
        if self.last_up.as_ref().is_some_and(|(at, to)| *to == place && now - at < SETTLE_MS) {
            self.too_high.insert(place);
            self.calm_ms = MAX_CALM_MS.min(self.calm_ms * 2);
        }
        self.last_up = None;
        self.live_undo = Some((kbps, self.lowered));
        self.live_kbps = Some(to);
        self.lowered = Some(why);
        self.last_change = now;
        self.last_down = now;
        self.interrupted();
        // What the network carries, though the stream keeps more for its size: the next title starts by it.
        if why == Lowered::Network {
            self.remember(Some(ceiling));
        }
        Some(AutoStep { to: self.playing.clone(), why: Some(why), bitrate: Some(to) })
    }

    /// A step back up of a stream whose bitrate changes as it is sent, on
    /// `step_up`'s rules: to the next of the file's qualities' bitrates above
    /// it, or to its own.
    fn live_up(&mut self, now: i64) -> Option<AutoStep> {
        let live = self.live?;
        let kbps = self.live_kbps.unwrap_or(live.top);
        if kbps >= live.top {
            return None;
        }
        let calm_from = self.last_issue.max(self.last_change);
        if now - calm_from < self.calm_ms || now - self.last_down < AFTER_DOWN_MS {
            return None;
        }
        let mut next = live.top;
        for r in &self.ladder {
            if r.kbps > kbps && r.kbps < next {
                next = r.kbps;
            }
        }
        // Settled: the step above didn't hold this title.
        if self.too_high.contains(&self.place(Some(next))) {
            return None;
        }
        let net = self.network(calm_from)?;
        if net < HEADROOM * next {
            return None;
        }
        self.live_undo = Some((kbps, self.lowered));
        self.live_kbps = Some(next);
        self.last_change = now;
        self.last_up = Some((now, self.place(self.live_kbps)));
        self.interrupted();
        if next >= live.top && self.playing == ORIGINAL_ID {
            // The original as this player gets it: a step back up to the original.
            self.lowered = None;
            self.remember(None);
        } else if self.lowered == Some(Lowered::Network) {
            self.remember(Some(next));
        }
        Some(AutoStep { to: self.playing.clone(), why: None, bitrate: Some(next) })
    }

    /// Where it plays, as steps up are told apart: the quality, and the bitrate of a stream that changes as it is sent.
    fn place(&self, kbps: Option<f64>) -> String {
        match (self.live, kbps) {
            (Some(_), Some(k)) => format!("{}@{}", self.playing, k),
            _ => self.playing.clone(),
        }
    }

    /// Remembers `kbps` as the network's ceiling (none: forgets it), here and on the server.
    fn remember(&mut self, kbps: Option<f64>) {
        let at = (self.wall)();
        if let Some(m) = self.memory.as_mut() {
            m.set(kbps.map(|kbps| Remembered { kbps, at }));
        }
        if let Some(f) = self.on_ceiling.as_mut() {
            f(kbps);
        }
    }

    /// The highest rung at or below `ceiling` kbps (any, when none) and below
    /// `under` kbps when given: at least one step down. The lowest rung when
    /// none is that low; none when there is no rung below `under`.
    fn highest_at_most(&self, ceiling: Option<f64>, under: Option<f64>) -> Option<&Rung> {
        let below: Vec<&Rung> = self.ladder.iter().filter(|r| under.is_none_or(|u| r.kbps < u)).collect();
        below.iter().find(|r| ceiling.is_none_or(|c| r.kbps <= c)).or(below.last()).copied()
    }

    fn rung(&self, id: &str) -> Option<&Rung> {
        self.ladder.iter().find(|r| r.id == id)
    }

    fn index_of(&self, id: &str) -> i64 {
        self.ladder.iter().position(|r| r.id == id).map_or(-1, |i| i as i64)
    }

    fn trim(&mut self, now: i64) {
        self.loads.retain(|l| now - l.0 <= KEEP_MS);
        self.rates.retain(|r| now - r.0 <= KEEP_MS);
        self.floors.retain(|f| now - f.0 <= KEEP_MS);
    }
}
