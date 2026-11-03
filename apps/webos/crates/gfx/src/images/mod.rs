//! The picture cache: answers the interface at once with what it has, fetches
//! and decodes the rest on two worker threads, and hands decoded pictures to
//! the renderer to upload within a per-frame budget.
//!
//! Requests are served by [`Priority`], then the most recently asked first
//! (everything on screen is asked every frame, so what scrolled away sinks),
//! then in the order they came; a request not asked again within two seconds
//! leaves the queue unless a worker already took it. Textures live under a
//! byte budget by device tier (64, 128 or 192 MB), the least
//! recently drawn going first.

pub mod decode;
pub mod disk;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use pley_base::{Fetch, FetchError, ImageId, ImageProvider, ImageRequest, ImageState, Priority, Waker, time};

use self::decode::Picture;
use self::disk::DiskCache;

/// How the picture cache is sized.
#[derive(Clone, Debug)]
pub struct ImageCacheConfig {
    /// Bytes of texels the decoded pictures may hold on the GPU.
    pub gpu_budget: usize,
    /// Bytes of texels uploaded per frame (one picture always goes, however
    /// big: a backdrop takes its frame alone).
    pub upload_budget: usize,
    /// Where fetched bytes are kept between runs, and how many.
    pub disk: Option<DiskCacheConfig>,
    /// Fetch and decode threads.
    pub workers: usize,
    /// A queued request not asked again for this long is dropped.
    pub forget_after: Duration,
}

/// The disk cache's folder and byte budget.
#[derive(Clone, Debug)]
pub struct DiskCacheConfig {
    pub dir: PathBuf,
    pub budget: u64,
}

impl ImageCacheConfig {
    /// The GPU budget of a low-tier TV.
    pub const GPU_LOW: usize = 64 << 20;
    /// The GPU budget of a mid-tier TV, the default.
    pub const GPU_MID: usize = 128 << 20;
    /// The GPU budget of a high-tier TV.
    pub const GPU_HIGH: usize = 192 << 20;
    /// The disk cache's budget unless the device says otherwise.
    pub const DISK_DEFAULT: u64 = 128 << 20;
}

impl Default for ImageCacheConfig {
    fn default() -> ImageCacheConfig {
        ImageCacheConfig {
            gpu_budget: ImageCacheConfig::GPU_MID,
            upload_budget: 2 << 20,
            disk: None,
            workers: 2,
            forget_after: Duration::from_secs(2),
        }
    }
}

/// A queued request.
#[derive(Debug)]
struct Job {
    req: ImageRequest,
    priority: Priority,
    /// The frame it was last asked in, and when (for forgetting).
    asked_tick: u64,
    asked_at: f64,
    seq: u64,
}

#[derive(Default)]
struct Queue {
    jobs: Vec<Job>,
    running: HashSet<ImageRequest>,
    seq: u64,
    shutdown: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    work: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// What a worker made of a request.
#[derive(Debug)]
enum Outcome {
    Decoded(Picture),
    /// The server has no such picture, or its bytes aren't a picture.
    Gone,
    /// Worth asking again later: the server was out of reach or failed.
    Retry {
        transient: bool,
    },
}

struct Done {
    req: ImageRequest,
    priority: Priority,
    outcome: Outcome,
}

/// What the cache knows of a request.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    /// Queued or being fetched; when it was last asked for.
    Pending {
        asked_at: f64,
    },
    /// Decoded, waiting for its upload.
    Decoded,
    Ready {
        id: ImageId,
        width: u32,
        height: u32,
        since: f64,
    },
    /// Failed for now; asked again after `after`.
    Retry {
        after: f64,
        attempts: u8,
    },
    Failed,
}

/// A picture on the GPU.
#[derive(Debug)]
struct Resident {
    req: ImageRequest,
    texture: glow::Texture,
    bytes: usize,
    last_used: u64,
}

/// The cache. Lives on the main thread; [`crate::Renderer::prepare`] and
/// [`crate::Renderer::render`] upload what it decoded.
pub struct ImageCache {
    shared: Arc<Shared>,
    done: Receiver<Done>,
    disk: Option<Arc<DiskCache>>,
    entries: HashMap<ImageRequest, Entry>,
    uploads: Vec<(ImageRequest, Priority, Picture)>,
    resident: HashMap<ImageId, Resident>,
    gpu_bytes: usize,
    /// Textures evicted or forgotten, for the renderer to delete.
    dead: Vec<glow::Texture>,
    next_id: u32,
    tick: u64,
    config: ImageCacheConfig,
}

/// More entries than this and the ones that can't come back soon are pruned.
const MAX_ENTRIES: usize = 4096;

impl ImageCache {
    /// A cache fetching through `fetch`, calling `waker` whenever a picture is
    /// decoded or given up on.
    pub fn new(fetch: Arc<dyn Fetch>, waker: Waker, config: ImageCacheConfig) -> ImageCache {
        let shared = Arc::new(Shared { queue: Mutex::new(Queue::default()), work: Condvar::new() });
        let disk = config.disk.as_ref().map(|d| Arc::new(DiskCache::new(d.dir.clone(), d.budget)));
        let (tx, done) = channel();
        for n in 0..config.workers.max(1) {
            let worker = Worker {
                shared: shared.clone(),
                fetch: fetch.clone(),
                disk: disk.clone(),
                tx: tx.clone(),
                waker: waker.clone(),
                forget_after: config.forget_after.as_secs_f64(),
            };
            // A refused thread leaves fewer workers, not a crashed app.
            let _ = std::thread::Builder::new().name(format!("pley-image-{n}")).spawn(move || worker.run());
        }
        ImageCache {
            shared,
            done,
            disk,
            entries: HashMap::new(),
            uploads: Vec::new(),
            resident: HashMap::new(),
            gpu_bytes: 0,
            dead: Vec::new(),
            next_id: 1,
            tick: 0,
            config,
        }
    }

    /// The configuration it was made with.
    pub fn config(&self) -> &ImageCacheConfig {
        &self.config
    }

    /// Bytes of texels on the GPU now.
    pub fn gpu_bytes(&self) -> usize {
        self.gpu_bytes
    }

    /// Whether decoded pictures wait for an upload: the app should draw
    /// another frame.
    pub fn has_uploads(&self) -> bool {
        !self.uploads.is_empty()
    }

    /// Forgets every picture, queued, decoded and on the GPU, and the disk
    /// cache's files (signing out: the next server's pictures start afresh).
    pub fn clear(&mut self) {
        self.shared.lock().jobs.clear();
        self.entries.clear();
        self.uploads.clear();
        self.dead.extend(self.resident.drain().map(|(_, r)| r.texture));
        self.gpu_bytes = 0;
        if let Some(disk) = &self.disk {
            disk.clear();
        }
    }

    /// Takes in what the workers finished.
    pub(crate) fn pump(&mut self) {
        let now = time::now();
        while let Ok(done) = self.done.try_recv() {
            let entry = match done.outcome {
                Outcome::Decoded(picture) => {
                    self.uploads.push((done.req.clone(), done.priority, picture));
                    Entry::Decoded
                }
                Outcome::Gone => Entry::Failed,
                Outcome::Retry { transient } => {
                    let attempts = match self.entries.get(&done.req) {
                        Some(Entry::Retry { attempts, .. }) => attempts.saturating_add(1),
                        _ => 1,
                    };
                    // A failing server gets three tries; one out of reach is
                    // asked again for as long as the picture is wanted.
                    if !transient && attempts >= 3 {
                        Entry::Failed
                    } else {
                        Entry::Retry { after: now + retry_delay(attempts), attempts }
                    }
                }
            };
            self.entries.insert(done.req, entry);
        }
        if self.entries.len() > MAX_ENTRIES {
            self.prune(now);
        }
    }

    /// Drops what can be had again cheaply: failures and requests nobody
    /// asked for in a minute.
    fn prune(&mut self, now: f64) {
        self.entries.retain(|_, e| match e {
            Entry::Failed | Entry::Retry { .. } => false,
            Entry::Pending { asked_at } => now - *asked_at < 60.0,
            Entry::Decoded | Entry::Ready { .. } => true,
        });
    }

    /// The size in bytes of the next picture to upload, the most urgent first.
    pub(crate) fn next_upload_bytes(&mut self) -> Option<usize> {
        let i = self.next_upload_index()?;
        Some(self.uploads[i].2.bytes())
    }

    fn next_upload_index(&self) -> Option<usize> {
        self.uploads.iter().enumerate().min_by_key(|(i, u)| (u.1, *i)).map(|(i, _)| i)
    }

    /// The next picture to upload, the most urgent first.
    pub(crate) fn take_upload(&mut self) -> Option<(ImageRequest, Picture)> {
        let i = self.next_upload_index()?;
        let (req, _, picture) = self.uploads.remove(i);
        Some((req, picture))
    }

    /// Records that `req`'s picture is on the GPU as `texture`.
    pub(crate) fn install(&mut self, req: ImageRequest, texture: glow::Texture, width: u32, height: u32) -> ImageId {
        let id = ImageId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let bytes = width as usize * height as usize * 4;
        self.gpu_bytes += bytes;
        let since = time::now();
        self.entries.insert(req.clone(), Entry::Ready { id, width, height, since });
        self.resident.insert(id, Resident { req, texture, bytes, last_used: self.tick });
        id
    }

    /// A picture's texture and size, if it is on the GPU; records that it was
    /// drawn.
    pub(crate) fn texture(&mut self, id: ImageId) -> Option<(glow::Texture, u32, u32)> {
        let tick = self.tick;
        let r = self.resident.get_mut(&id)?;
        r.last_used = tick;
        match self.entries.get(&r.req) {
            Some(Entry::Ready { width, height, .. }) => Some((r.texture, *width, *height)),
            _ => None,
        }
    }

    /// Takes textures over the GPU budget out, least recently drawn first,
    /// never one drawn or asked for in this frame. Returns every texture the
    /// renderer must now delete.
    pub(crate) fn evict(&mut self) -> Vec<glow::Texture> {
        if self.gpu_bytes > self.config.gpu_budget {
            let mut by_age: Vec<(u64, ImageId)> = self
                .resident
                .iter()
                .filter(|(_, r)| r.last_used < self.tick)
                .map(|(id, r)| (r.last_used, *id))
                .collect();
            by_age.sort_unstable_by_key(|(used, id)| (*used, id.0));
            for (_, id) in by_age {
                if self.gpu_bytes <= self.config.gpu_budget {
                    break;
                }
                if let Some(r) = self.resident.remove(&id) {
                    self.gpu_bytes -= r.bytes;
                    self.entries.remove(&r.req);
                    self.dead.push(r.texture);
                }
            }
        }
        std::mem::take(&mut self.dead)
    }

    /// Ends a frame: what is asked or drawn from now on is the next frame's.
    pub(crate) fn end_frame(&mut self) {
        self.tick += 1;
    }

    fn enqueue(&mut self, req: &ImageRequest, priority: Priority, now: f64) {
        let tick = self.tick;
        let mut q = self.shared.lock();
        if let Some(job) = q.jobs.iter_mut().find(|j| j.req == *req) {
            // Within a frame the most urgent use wins; a new frame says
            // afresh how urgent it is.
            job.priority = if job.asked_tick == tick { job.priority.min(priority) } else { priority };
            job.asked_tick = tick;
            job.asked_at = now;
            return;
        }
        if q.running.contains(req) {
            return;
        }
        q.seq += 1;
        let seq = q.seq;
        q.jobs.push(Job { req: req.clone(), priority, asked_tick: tick, asked_at: now, seq });
        drop(q);
        self.shared.work.notify_one();
    }
}

impl ImageProvider for ImageCache {
    fn request(&mut self, req: &ImageRequest, priority: Priority) -> ImageState {
        self.pump();
        let now = time::now();
        match self.entries.get_mut(req) {
            Some(Entry::Ready { id, width, height, since }) => {
                let state = ImageState::Ready { id: *id, width: *width, height: *height, since: *since };
                if let Some(r) = self.resident.get_mut(id) {
                    r.last_used = self.tick;
                }
                state
            }
            Some(Entry::Failed) => ImageState::Failed,
            Some(Entry::Decoded) => ImageState::Loading,
            Some(Entry::Pending { asked_at }) => {
                *asked_at = now;
                self.enqueue(req, priority, now);
                ImageState::Loading
            }
            Some(Entry::Retry { after, .. }) => {
                if now >= *after {
                    self.entries.insert(req.clone(), Entry::Pending { asked_at: now });
                    self.enqueue(req, priority, now);
                }
                ImageState::Loading
            }
            None => {
                self.entries.insert(req.clone(), Entry::Pending { asked_at: now });
                self.enqueue(req, priority, now);
                ImageState::Loading
            }
        }
    }
}

impl Drop for ImageCache {
    fn drop(&mut self) {
        // Workers finish what they hold and stop; nobody waits for them.
        self.shared.lock().shutdown = true;
        self.shared.work.notify_all();
    }
}

/// Seconds before asking again after `attempts` failures: 1, 2, 4, 8, 16,
/// then 30.
fn retry_delay(attempts: u8) -> f64 {
    (1u32 << attempts.saturating_sub(1).min(5)).min(30) as f64
}

/// The most urgent job: by priority, then asked most recently, then first come.
fn best(jobs: &[Job]) -> Option<usize> {
    jobs.iter().enumerate().min_by_key(|(_, j)| (j.priority, std::cmp::Reverse(j.asked_tick), j.seq)).map(|(i, _)| i)
}

struct Worker {
    shared: Arc<Shared>,
    fetch: Arc<dyn Fetch>,
    disk: Option<Arc<DiskCache>>,
    tx: Sender<Done>,
    waker: Waker,
    forget_after: f64,
}

impl Worker {
    fn run(self) {
        while let Some(job) = self.next_job() {
            let outcome = load(&job.req, &*self.fetch, self.disk.as_deref());
            self.shared.lock().running.remove(&job.req);
            if self.tx.send(Done { req: job.req, priority: job.priority, outcome }).is_err() {
                return;
            }
            self.waker.wake();
        }
    }

    /// Waits for the most urgent job, dropping those nobody asked for lately;
    /// `None` when the cache is gone.
    fn next_job(&self) -> Option<Job> {
        let mut q = self.shared.lock();
        loop {
            if q.shutdown {
                return None;
            }
            let now = time::now();
            let forget = self.forget_after;
            q.jobs.retain(|j| now - j.asked_at <= forget);
            if let Some(i) = best(&q.jobs) {
                let job = q.jobs.swap_remove(i);
                q.running.insert(job.req.clone());
                return Some(job);
            }
            q = self.shared.work.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// Disk first, then the network; decoded at the width asked. Bytes that
/// decode are kept on disk; bytes that don't are forgotten.
fn load(req: &ImageRequest, fetch: &dyn Fetch, disk: Option<&DiskCache>) -> Outcome {
    if let Some(bytes) = disk.and_then(|d| d.get(&req.url)) {
        match decode::decode(&bytes, req.width_px) {
            Ok(picture) => return Outcome::Decoded(picture),
            Err(_) => disk.into_iter().for_each(|d| d.remove(&req.url)),
        }
    }
    let bytes = match fetch.fetch(&req.url) {
        Ok(bytes) => bytes,
        Err(FetchError::Missing) => return Outcome::Gone,
        Err(FetchError::Unreachable) => return Outcome::Retry { transient: true },
        Err(FetchError::Failed(_)) => return Outcome::Retry { transient: false },
    };
    match decode::decode(&bytes, req.width_px) {
        Ok(picture) => {
            if let Some(d) = disk {
                d.put(&req.url, &bytes);
            }
            Outcome::Decoded(picture)
        }
        Err(_) => Outcome::Gone,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use super::*;

    /// Serves tiny PNGs; "missing" addresses 404, "down" ones are out of reach.
    struct FakeServer {
        fetched: Mutex<Vec<String>>,
        gate: Mutex<bool>,
        opened: Condvar,
    }

    impl FakeServer {
        fn new(open: bool) -> Arc<FakeServer> {
            Arc::new(FakeServer { fetched: Mutex::new(Vec::new()), gate: Mutex::new(open), opened: Condvar::new() })
        }

        fn open(&self) {
            *self.gate.lock().unwrap() = true;
            self.opened.notify_all();
        }

        fn fetched(&self) -> Vec<String> {
            self.fetched.lock().unwrap().clone()
        }
    }

    impl Fetch for FakeServer {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            let mut open = self.gate.lock().unwrap();
            while !*open {
                open = self.opened.wait(open).unwrap();
            }
            drop(open);
            self.fetched.lock().unwrap().push(url.to_string());
            if url.contains("missing") {
                return Err(FetchError::Missing);
            }
            if url.contains("down") {
                return Err(FetchError::Unreachable);
            }
            if url.contains("junk") {
                return Ok(b"not a picture".to_vec());
            }
            Ok(png_bytes(64, 32))
        }
    }

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&vec![90; (w * h * 3) as usize]).unwrap();
        out
    }

    fn tex(n: u32) -> glow::Texture {
        glow::NativeTexture(NonZeroU32::new(n).unwrap())
    }

    fn cache(server: Arc<FakeServer>, wakes: Arc<AtomicUsize>, config: ImageCacheConfig) -> ImageCache {
        ImageCache::new(server, Waker::new(move || _ = wakes.fetch_add(1, Ordering::SeqCst)), config)
    }

    /// Pumps until `f` holds or two seconds pass.
    fn wait_until(c: &mut ImageCache, mut f: impl FnMut(&mut ImageCache) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            c.pump();
            if f(c) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        false
    }

    /// Plays the renderer: uploads everything decoded.
    fn upload_all(c: &mut ImageCache, next_tex: &mut u32) {
        while let Some((req, picture)) = c.take_upload() {
            *next_tex += 1;
            c.install(req, tex(*next_tex), picture.width, picture.height);
        }
    }

    #[test]
    fn a_request_loads_decodes_and_becomes_ready() {
        let server = FakeServer::new(true);
        let wakes = Arc::new(AtomicUsize::new(0));
        let mut c = cache(server.clone(), wakes.clone(), ImageCacheConfig::default());
        let req = ImageRequest::new("http://s/a.png", 32);
        assert_eq!(c.request(&req, Priority::Visible), ImageState::Loading);
        assert!(wait_until(&mut c, |c| c.has_uploads()));
        assert!(wakes.load(Ordering::SeqCst) >= 1);
        assert_eq!(c.request(&req, Priority::Visible), ImageState::Loading);
        let (got, picture) = c.take_upload().unwrap();
        assert_eq!(got, req);
        // Shrunk to the width asked.
        assert_eq!((picture.width, picture.height), (32, 16));
        let id = c.install(got, tex(1), 32, 16);
        match c.request(&req, Priority::Visible) {
            ImageState::Ready { id: got, width: 32, height: 16, since } => {
                assert_eq!(got, id);
                assert!(since <= time::now());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(c.texture(id), Some((tex(1), 32, 16)));
        assert_eq!(server.fetched(), vec!["http://s/a.png".to_string()]);
    }

    #[test]
    fn missing_and_undecodable_pictures_fail_for_good() {
        let server = FakeServer::new(true);
        let mut c = cache(server, Arc::default(), ImageCacheConfig::default());
        let missing = ImageRequest::new("http://s/missing.jpg", 100);
        let junk = ImageRequest::new("http://s/junk.jpg", 100);
        c.request(&missing, Priority::Visible);
        c.request(&junk, Priority::Visible);
        assert!(wait_until(&mut c, |c| {
            c.request(&missing, Priority::Visible) == ImageState::Failed
                && c.request(&junk, Priority::Visible) == ImageState::Failed
        }));
    }

    #[test]
    fn an_unreachable_server_is_asked_again_later() {
        let server = FakeServer::new(true);
        let mut c = cache(server.clone(), Arc::default(), ImageCacheConfig::default());
        let req = ImageRequest::new("http://down/a.jpg", 100);
        c.request(&req, Priority::Visible);
        assert!(wait_until(&mut c, |c| matches!(c.entries.get(&req), Some(Entry::Retry { .. }))));
        // Asked again before the delay: no new fetch.
        assert_eq!(c.request(&req, Priority::Visible), ImageState::Loading);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(server.fetched().len(), 1);
        assert_eq!(retry_delay(1), 1.0);
        assert_eq!(retry_delay(3), 4.0);
        assert_eq!(retry_delay(9), 30.0);
    }

    #[test]
    fn queue_serves_visible_first_then_most_recent_then_first_asked() {
        let job = |priority, tick, seq| Job {
            req: ImageRequest::new(format!("u{seq}"), 1),
            priority,
            asked_tick: tick,
            asked_at: 0.0,
            seq,
        };
        let jobs = vec![
            job(Priority::Prefetch, 9, 1),
            job(Priority::Visible, 3, 2),
            job(Priority::Visible, 5, 4),
            job(Priority::Visible, 5, 3),
            job(Priority::Near, 9, 5),
        ];
        assert_eq!(jobs[best(&jobs).unwrap()].seq, 3);
    }

    #[test]
    fn requests_nobody_asks_for_again_leave_the_queue() {
        // The server holds the only worker on the first request.
        let server = FakeServer::new(false);
        let config = ImageCacheConfig { workers: 1, forget_after: Duration::from_millis(30), ..Default::default() };
        let mut c = cache(server.clone(), Arc::default(), config);
        let first = ImageRequest::new("http://s/first.png", 8);
        let stale = ImageRequest::new("http://s/stale.png", 8);
        let kept = ImageRequest::new("http://s/kept.png", 8);
        c.request(&first, Priority::Visible);
        // Let the worker take the first request before queueing the others.
        assert!(wait_until(&mut c, |c| c.shared.lock().running.contains(&first)));
        c.request(&stale, Priority::Visible);
        c.request(&kept, Priority::Visible);
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(80) {
            c.request(&kept, Priority::Visible);
            std::thread::sleep(Duration::from_millis(5));
        }
        server.open();
        assert!(wait_until(&mut c, |c| c.uploads.len() == 2));
        let fetched = server.fetched();
        assert!(fetched.contains(&first.url.to_string()) && fetched.contains(&kept.url.to_string()));
        assert!(!fetched.contains(&stale.url.to_string()));
        // Asked again, the forgotten one is queued afresh.
        c.request(&stale, Priority::Visible);
        assert!(wait_until(&mut c, |c| c.uploads.len() == 3));
    }

    #[test]
    fn uploads_go_most_urgent_first() {
        let server = FakeServer::new(true);
        let mut c = cache(server, Arc::default(), ImageCacheConfig::default());
        let near = ImageRequest::new("http://s/near.png", 8);
        let visible = ImageRequest::new("http://s/visible.png", 8);
        c.request(&near, Priority::Near);
        assert!(wait_until(&mut c, |c| c.uploads.len() == 1));
        c.request(&visible, Priority::Visible);
        assert!(wait_until(&mut c, |c| c.uploads.len() == 2));
        assert_eq!(c.next_upload_bytes(), Some(8 * 4 * 4));
        assert_eq!(c.take_upload().unwrap().0, visible);
        assert_eq!(c.take_upload().unwrap().0, near);
    }

    #[test]
    fn gpu_budget_evicts_the_least_recently_drawn_never_this_frames() {
        let server = FakeServer::new(true);
        // Room for two 64 × 32 pictures.
        let config = ImageCacheConfig { gpu_budget: 2 * 64 * 32 * 4, ..Default::default() };
        let mut c = cache(server, Arc::default(), config);
        let reqs: Vec<ImageRequest> = (0..3).map(|n| ImageRequest::new(format!("http://s/{n}.png"), 0)).collect();
        for req in &reqs {
            c.request(req, Priority::Visible);
        }
        assert!(wait_until(&mut c, |c| c.uploads.len() == 3));
        upload_all(&mut c, &mut 0);
        let ids: Vec<ImageId> = reqs
            .iter()
            .map(|r| match c.request(r, Priority::Visible) {
                ImageState::Ready { id, .. } => id,
                other => panic!("{other:?}"),
            })
            .collect();
        let second = c.texture(ids[1]).unwrap().0;
        // Three pictures in a budget of two, but all drawn in this frame.
        assert!(c.evict().is_empty());
        assert_eq!(c.gpu_bytes(), 3 * 64 * 32 * 4);
        c.end_frame();
        // The next frame draws the first and the third: the second goes.
        assert!(c.texture(ids[0]).is_some());
        assert!(c.texture(ids[2]).is_some());
        assert_eq!(c.evict(), vec![second]);
        assert_eq!(c.gpu_bytes(), 2 * 64 * 32 * 4);
        assert!(c.texture(ids[1]).is_none());
        assert_eq!(c.request(&reqs[1], Priority::Visible), ImageState::Loading);
    }

    #[test]
    fn disk_cache_is_read_before_the_network() {
        let dir = std::env::temp_dir().join(format!("pley-gfx-images-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let disk = DiskCacheConfig { dir: dir.clone(), budget: 1 << 20 };
        let config = ImageCacheConfig { disk: Some(disk), ..Default::default() };
        let req = ImageRequest::new("http://s/cached.png", 0);
        {
            let server = FakeServer::new(true);
            let mut c = cache(server.clone(), Arc::default(), config.clone());
            c.request(&req, Priority::Visible);
            assert!(wait_until(&mut c, |c| c.has_uploads()));
            assert_eq!(server.fetched().len(), 1);
        }
        let server = FakeServer::new(true);
        let mut c = cache(server.clone(), Arc::default(), config);
        c.request(&req, Priority::Visible);
        assert!(wait_until(&mut c, |c| c.has_uploads()));
        assert!(server.fetched().is_empty());
        c.clear();
        assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
