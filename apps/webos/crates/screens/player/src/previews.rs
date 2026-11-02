//! Seek previews cut out of the trickplay sheets (`TrickplayThumbnails.kt`). A sheet holds 10 × 10
//! previews: 3200 × 1800 pixels at the server's usual 320, about 23 MB once decoded. Uploaded whole
//! it would take a third of a Low-tier TV's picture memory and stall the frame it goes up in. So a
//! sheet is fetched and decoded on the task pool and kept in memory only while a scrub uses it;
//! each preview shown is cut out of it (its pixels copied, whole, in the film's shape) and
//! drawn as a small picture of its own. As on Android, the last 40 previews are kept (16 on
//! Low-tier TVs), and the bytes of two sheets: the one scrubbed and the next one the scrub heads
//! for, fetched ahead.

use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use pley_base::{Fetch, FetchError, RgbaImage, Tier};
use pley_core::{Promise, Tasks};
use pley_gfx::images::decode::{MAX_SIDE, decode};

/// Previews kept (`TrickplayThumbnails`: 40, and 16 on the weakest TVs).
const TILES: usize = 40;
/// See [`TILES`].
const TILES_LOW: usize = 16;
/// Sheets whose bytes are kept: the one scrubbed and the one ahead.
const SHEETS: usize = 2;
/// Sheets that couldn't be had, remembered so they aren't asked for every frame.
const FAILED: usize = 4;
/// The decoded sheet is let go this long after a preview of it last showed, so a run of presses
/// that each settle doesn't decode it again.
pub const KEEP_SHEET_S: f64 = 5.0;
/// A sheet that couldn't be had is asked for again after this long.
const RETRY_S: f64 = 10.0;

/// The preview wanted: its sheet's address, its place in the sheet's pixels, and the sheet's
/// columns of previews (to tell how much smaller the sheet came, if it did). The numbers are the
/// server's, and checked before they are used.
#[derive(Clone, Copy, Debug)]
pub struct Want<'a> {
    pub sheet: &'a str,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub columns: i64,
}

/// A preview ready to draw, in the film's shape, and the name the renderer keeps its
/// texture by.
#[derive(Clone, Debug)]
pub struct Tile {
    pub image: Arc<RgbaImage>,
    pub key: u64,
}

/// A sheet's pixels (RGBA), decoded on the task pool.
struct Sheet {
    url: Arc<str>,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

/// What a decode brings back: the bytes, kept to decode them again, and the pixels.
struct Decoded {
    bytes: Arc<[u8]>,
    sheet: Sheet,
}

/// A preview cut out, and where from.
struct Cut {
    sheet: Arc<str>,
    x: i64,
    y: i64,
    tile: Tile,
}

type Loading<T> = Option<(Arc<str>, Promise<Result<T, String>>)>;

/// The seek previews of one playback.
pub struct Previews {
    source: Option<(Arc<dyn Fetch>, Tasks)>,
    /// Previews cut lately, the latest first.
    tiles: VecDeque<Cut>,
    /// The bytes of the sheet scrubbed and of the one ahead, the latest first.
    bytes: VecDeque<(Arc<str>, Arc<[u8]>)>,
    /// The sheet scrubbed, decoded, and when a preview of it last showed.
    sheet: Option<Sheet>,
    used: f64,
    decoding: Loading<Decoded>,
    fetching: Loading<Arc<[u8]>>,
    failed: VecDeque<(Arc<str>, f64)>,
    /// The preview shown last in this scrub, shown on while the next one's sheet decodes.
    last: Option<Tile>,
}

impl fmt::Debug for Previews {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Previews")
            .field("tiles", &self.tiles.len())
            .field("sheets", &self.bytes.len())
            .field("decoded", &self.sheet.is_some())
            .finish()
    }
}

impl Default for Previews {
    fn default() -> Previews {
        Previews::none()
    }
}

impl Previews {
    /// Previews that never load (a stand-in playback with no server).
    pub fn none() -> Previews {
        Previews {
            source: None,
            tiles: VecDeque::new(),
            bytes: VecDeque::new(),
            sheet: None,
            used: 0.0,
            decoding: None,
            fetching: None,
            failed: VecDeque::new(),
            last: None,
        }
    }

    /// Previews whose sheets `fetch` brings (the pictures' fetcher, which follows the server in
    /// use), decoded on `tasks`.
    pub fn new(fetch: Arc<dyn Fetch>, tasks: Tasks) -> Previews {
        Previews { source: Some((fetch, tasks)), ..Previews::none() }
    }

    /// The preview `want` names when its sheet is in memory, else the one shown last while the
    /// sheet decodes. Starts what is missing, and fetches `ahead` (the next sheet the scrub heads
    /// for) once nothing else is on its way. `now` is the frame's time in seconds.
    pub fn tile(&mut self, want: &Want, ahead: Option<&str>, tier: Tier, now: f64) -> Option<Tile> {
        self.collect(now);
        self.used = now;
        let cap = if tier == Tier::Low { TILES_LOW } else { TILES };
        let found = self.cached(want).or_else(|| self.cut(want, cap));
        if found.is_none() {
            self.load(want.sheet, now);
        }
        if let Some(url) = ahead {
            self.fetch_ahead(url, now);
        }
        match found {
            Some(tile) => {
                self.last = Some(tile.clone());
                Some(tile)
            }
            None => self.last.clone(),
        }
    }

    /// No preview shows: the one shown last is forgotten, and the decoded sheet goes
    /// [`KEEP_SHEET_S`] after a preview of it last showed.
    pub fn idle(&mut self, now: f64) {
        self.collect(now);
        self.last = None;
        if self.sheet.is_some() && now - self.used >= KEEP_SHEET_S {
            self.sheet = None;
        }
    }

    /// When [`Previews::idle`] next has something to let go.
    pub fn next_due(&self) -> Option<f64> {
        self.sheet.as_ref().map(|_| self.used + KEEP_SHEET_S)
    }

    /// Whether a sheet is decoded in memory.
    pub fn decoded(&self) -> bool {
        self.sheet.is_some()
    }

    /// Whether a sheet is being fetched or decoded.
    pub fn loading(&self) -> bool {
        self.decoding.is_some() || self.fetching.is_some()
    }

    /// Previews kept.
    pub fn kept(&self) -> usize {
        self.tiles.len()
    }

    /// Takes what the task pool finished.
    fn collect(&mut self, now: f64) {
        if let Some((url, mut promise)) = self.decoding.take() {
            match promise.take() {
                Some(Ok(d)) => {
                    self.keep_bytes(url, d.bytes);
                    self.sheet = Some(d.sheet);
                }
                Some(Err(why)) => self.fail(url, &why, now),
                None if promise.failed() => self.fail(url, "the decode stopped", now),
                None => self.decoding = Some((url, promise)),
            }
        }
        if let Some((url, mut promise)) = self.fetching.take() {
            match promise.take() {
                Some(Ok(bytes)) => self.keep_bytes(url, bytes),
                Some(Err(why)) => self.fail(url, &why, now),
                None if promise.failed() => self.fail(url, "the fetch stopped", now),
                None => self.fetching = Some((url, promise)),
            }
        }
    }

    fn cached(&mut self, want: &Want) -> Option<Tile> {
        let i = self.tiles.iter().position(|c| *c.sheet == *want.sheet && c.x == want.x && c.y == want.y)?;
        let cut = self.tiles.remove(i)?;
        let tile = cut.tile.clone();
        self.tiles.push_front(cut);
        Some(tile)
    }

    fn cut(&mut self, want: &Want, cap: usize) -> Option<Tile> {
        let sheet = self.sheet.as_ref().filter(|s| *s.url == *want.sheet)?;
        let image = cut_tile(sheet, want)?;
        let tile = Tile { image: Arc::new(image), key: tile_key(want) };
        self.tiles.push_front(Cut { sheet: sheet.url.clone(), x: want.x, y: want.y, tile: tile.clone() });
        self.tiles.truncate(cap);
        Some(tile)
    }

    /// Decodes the sheet at `url` (fetching it unless its bytes are here); a decode of another
    /// sheet stops, as the scrub has left it.
    fn load(&mut self, url: &str, now: f64) {
        let Some((fetch, tasks)) = self.source.clone() else { return };
        let decoded = self.sheet.as_ref().is_some_and(|s| *s.url == *url);
        let on_its_way = self.decoding.as_ref().is_some_and(|(u, _)| **u == *url)
            || self.fetching.as_ref().is_some_and(|(u, _)| **u == *url);
        // A sheet being fetched ahead is decoded once its bytes are here.
        if decoded || on_its_way || self.failed_lately(url, now) {
            return;
        }
        let url: Arc<str> = url.into();
        let bytes = self.bytes.iter().find(|(u, _)| *u == url).map(|(_, b)| b.clone());
        let u = url.clone();
        let promise = tasks.spawn(move || decode_sheet(&*fetch, u, bytes));
        self.decoding = Some((url, promise));
    }

    /// Fetches the bytes of the sheet ahead of the scrub when nothing else is on its way.
    fn fetch_ahead(&mut self, url: &str, now: f64) {
        let Some((fetch, tasks)) = self.source.clone() else { return };
        let have = self.bytes.iter().any(|(u, _)| **u == *url) || self.sheet.as_ref().is_some_and(|s| *s.url == *url);
        if have || self.loading() || self.failed_lately(url, now) {
            return;
        }
        let url: Arc<str> = url.into();
        let u = url.clone();
        let promise = tasks.spawn(move || fetch_bytes(&*fetch, &u));
        self.fetching = Some((url, promise));
    }

    fn keep_bytes(&mut self, url: Arc<str>, bytes: Arc<[u8]>) {
        self.bytes.retain(|(u, _)| *u != url);
        self.bytes.push_front((url, bytes));
        self.bytes.truncate(SHEETS);
    }

    fn fail(&mut self, url: Arc<str>, why: &str, now: f64) {
        log::warn!("a sheet of seek previews couldn't be had: {why}");
        self.failed.retain(|(u, _)| *u != url);
        self.failed.push_front((url, now));
        self.failed.truncate(FAILED);
    }

    fn failed_lately(&self, url: &str, now: f64) -> bool {
        self.failed.iter().any(|(u, at)| **u == *url && now - at < RETRY_S)
    }
}

/// The sheet's bytes, said without the server's words (which may carry its address).
fn fetch_bytes(fetch: &dyn Fetch, url: &str) -> Result<Arc<[u8]>, String> {
    fetch.fetch(url).map(Arc::from).map_err(|e| match e {
        FetchError::Failed(_) => "the server refused it".to_string(),
        other => other.to_string(),
    })
}

/// On the task pool: the sheet's bytes (fetched unless given) and its pixels.
fn decode_sheet(fetch: &dyn Fetch, url: Arc<str>, bytes: Option<Arc<[u8]>>) -> Result<Decoded, String> {
    let bytes = match bytes {
        Some(b) => b,
        None => fetch_bytes(fetch, &url)?,
    };
    let picture = decode(&bytes, 0).map_err(|e| e.to_string())?;
    let sheet = Sheet { url, width: picture.width, height: picture.height, pixels: picture.pixels };
    Ok(Decoded { bytes, sheet })
}

/// The name of a preview's picture for the renderer: its sheet and place.
fn tile_key(want: &Want) -> u64 {
    let mut h = DefaultHasher::new();
    ("player.preview", want.sheet, want.x, want.y).hash(&mut h);
    h.finish()
}

/// The preview at `want` cut out of `sheet` (scaled to it when the sheet came smaller than its
/// columns of previews make), whole: the box letterboxes it, as the server's previews keep the
/// film's shape; none when the numbers fall outside the sheet.
fn cut_tile(sheet: &Sheet, want: &Want) -> Option<RgbaImage> {
    let side = i64::from(MAX_SIDE);
    let sized = (1..=side).contains(&want.width) && (1..=side).contains(&want.height) && want.columns <= side;
    if !sized || want.x < 0 || want.y < 0 || sheet.width == 0 || sheet.height == 0 {
        return None;
    }
    // In floating point from here: the server's numbers can't overflow it.
    let full = want.columns.max(1) as f64 * want.width as f64;
    let k = (f64::from(sheet.width) / full).min(1.0);
    let (x, y) = (want.x as f64 * k, want.y as f64 * k);
    let (w, h) = (want.width as f64 * k, want.height as f64 * k);
    let (x, y, w, h) = (x.round(), y.round(), w.round().max(1.0), h.round().max(1.0));
    if x + w > f64::from(sheet.width) || y + h > f64::from(sheet.height) {
        return None;
    }
    let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);
    let stride = sheet.width as usize * 4;
    let mut pixels = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        let start = row * stride + x * 4;
        pixels.extend_from_slice(sheet.pixels.get(start..start + w * 4)?);
    }
    Some(RgbaImage { width: w as u32, height: h as u32, pixels })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use pley_base::Waker;

    use super::*;

    /// A sheet whose every pixel says where it is: red the column, green the row.
    fn sheet(width: u32, height: u32) -> Sheet {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&[x as u8, y as u8, 0, 255]);
            }
        }
        Sheet { url: "s".into(), width, height, pixels }
    }

    fn want(x: i64, y: i64, width: i64, height: i64, columns: i64) -> Want<'static> {
        Want { sheet: "s", x, y, width, height, columns }
    }

    #[test]
    fn a_tile_is_cut_whole() {
        let s = sheet(64, 36);
        let t = cut_tile(&s, &want(16, 9, 16, 9, 4)).unwrap();
        assert_eq!((t.width, t.height), (16, 9));
        assert_eq!(&t.pixels[..4], &[16, 9, 0, 255]);
        let last = t.pixels.len() - 4;
        assert_eq!(&t.pixels[last..], &[31, 17, 0, 255]);
    }

    #[test]
    fn a_tile_of_another_shape_keeps_all_of_it() {
        // 4:3 previews (40 × 30) for a 16:9 box: the whole tile, which the box letterboxes.
        let s = sheet(80, 60);
        let t = cut_tile(&s, &want(40, 30, 40, 30, 2)).unwrap();
        assert_eq!((t.width, t.height), (40, 30));
        assert_eq!(&t.pixels[..4], &[40, 30, 0, 255]);
        // A wide film's previews keep their sides.
        let s = sheet(100, 20);
        let t = cut_tile(&s, &want(0, 0, 100, 20, 1)).unwrap();
        assert_eq!((t.width, t.height), (100, 20));
        assert_eq!(&t.pixels[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn a_smaller_sheet_scales_the_place_and_bad_numbers_cut_nothing() {
        // Four columns of 32 make 128, but the sheet came 64 wide: everything at half.
        let s = sheet(64, 36);
        let t = cut_tile(&s, &want(32, 18, 32, 18, 4)).unwrap();
        assert_eq!((t.width, t.height), (16, 9));
        assert_eq!(&t.pixels[..4], &[16, 9, 0, 255]);
        assert!(cut_tile(&s, &want(60, 0, 16, 9, 4)).is_none());
        assert!(cut_tile(&s, &want(-1, 0, 16, 9, 4)).is_none());
        assert!(cut_tile(&s, &want(0, 0, 0, 9, 4)).is_none());
        assert!(cut_tile(&s, &want(i64::MAX, i64::MAX, i64::MAX, i64::MAX, i64::MAX)).is_none());
    }

    /// Sheets of 4 × 4 previews of 32 × 18, as PNG, counting what is fetched.
    struct Server {
        fetched: Mutex<Vec<String>>,
    }

    impl Fetch for Server {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
            self.fetched.lock().unwrap().push(url.to_string());
            if url == "missing" {
                return Err(FetchError::Missing);
            }
            let mut image = RgbaImage::new(128, 72);
            for (i, px) in image.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                *px = [(i % 128) as u8, (i / 128) as u8, url.len() as u8, 255];
            }
            pley_gfx::screenshot::encode_png(&image).map_err(|e| FetchError::Failed(e.to_string()))
        }
    }

    fn previews() -> (Previews, Arc<Server>) {
        let server = Arc::new(Server { fetched: Mutex::new(Vec::new()) });
        (Previews::new(server.clone(), Tasks::with_threads(1, Waker::noop())), server)
    }

    fn at(sheet: &str, k: i64) -> Want<'_> {
        Want { sheet, x: k % 4 * 32, y: k / 4 * 18, width: 32, height: 18, columns: 4 }
    }

    /// Asks until the preview itself comes (not the one standing in), as frames would.
    fn wait(p: &mut Previews, want: &Want, ahead: Option<&str>) -> Tile {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(t) = p.tile(want, ahead, Tier::Mid, 1.0)
                && t.key == tile_key(want)
            {
                return t;
            }
            assert!(Instant::now() < until, "the preview never came");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn previews_come_from_a_sheet_decoded_once_and_the_next_is_fetched_ahead() {
        let (mut p, server) = previews();
        assert!(p.tile(&at("a", 5), Some("bb"), Tier::Mid, 1.0).is_none());
        assert!(p.loading());
        let t = wait(&mut p, &at("a", 5), Some("bb"));
        assert_eq!((t.image.width, t.image.height), (32, 18));
        assert_eq!(&t.image.pixels[..4], &[32, 18, 1, 255]);
        // Another preview of the same sheet is cut at once, with nothing fetched again.
        let other = p.tile(&at("a", 6), Some("bb"), Tier::Mid, 1.0).unwrap();
        assert_ne!(other.key, t.key);
        assert_eq!(p.kept(), 2);
        // The sheet ahead is fetched once nothing else is on its way, and decoded from its bytes.
        let until = Instant::now() + Duration::from_secs(5);
        while server.fetched.lock().unwrap().len() < 2 || p.loading() {
            p.tile(&at("a", 6), Some("bb"), Tier::Mid, 1.0);
            assert!(Instant::now() < until, "the sheet ahead was never fetched");
            std::thread::sleep(Duration::from_millis(2));
        }
        // While the next sheet decodes, the last preview stands in.
        let stand_in = p.tile(&at("bb", 0), None, Tier::Mid, 1.0).unwrap();
        assert_eq!(stand_in.key, other.key);
        let next = wait(&mut p, &at("bb", 0), None);
        assert_eq!(&next.image.pixels[..4], &[0, 0, 2, 255]);
        assert_eq!(*server.fetched.lock().unwrap(), vec!["a".to_string(), "bb".to_string()]);
    }

    #[test]
    fn previews_kept_are_bounded_and_the_sheet_goes_after_the_scrub() {
        let (mut p, _) = previews();
        wait(&mut p, &at("a", 0), None);
        for k in 0..16 {
            p.tile(&at("a", k), None, Tier::Low, 2.0).unwrap();
        }
        p.tile(&at("a", 0), None, Tier::Low, 2.0).unwrap();
        assert_eq!(p.kept(), TILES_LOW);
        assert_eq!(p.next_due(), Some(2.0 + KEEP_SHEET_S));
        p.idle(3.0);
        assert!(p.decoded());
        p.idle(2.0 + KEEP_SHEET_S);
        assert!(!p.decoded());
        assert_eq!(p.next_due(), None);
        // Previews already cut still show.
        assert!(p.tile(&at("a", 3), None, Tier::Low, 9.0).is_some());
    }

    #[test]
    fn a_sheet_that_cant_be_had_is_asked_again_only_later() {
        let (mut p, server) = previews();
        let gone = Want { sheet: "missing", ..at("a", 0) };
        let until = Instant::now() + Duration::from_secs(5);
        while server.fetched.lock().unwrap().is_empty() || p.loading() {
            assert!(p.tile(&gone, None, Tier::Mid, 1.0).is_none());
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        p.tile(&gone, None, Tier::Mid, 2.0);
        assert!(!p.loading());
        p.tile(&gone, None, Tier::Mid, 1.0 + RETRY_S);
        assert!(p.loading());
    }

    #[test]
    fn without_a_server_nothing_loads() {
        let mut p = Previews::none();
        assert!(p.tile(&at("a", 0), Some("b"), Tier::Mid, 1.0).is_none());
        assert!(!p.loading());
    }
}
