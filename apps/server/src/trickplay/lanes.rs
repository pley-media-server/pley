//! Which seek previews are made at once: rules over what is known now, with
//! no I/O of their own, for [`super::TrickplayService`] to follow.
//!
//! A title someone opened never waits for the libraries' previews: it has a
//! lane of its own, and the transcoder keeps a set's room for it. Its file
//! puts off a library file read from the same disk (one hard disk reading
//! two whole files at once makes each a fraction as fast), or every one
//! where the transcoder has few cores, so that what someone waits for goes
//! fast.
//!
//! The libraries' previews go one file at a time, as library work allows
//! ([`crate::schedule`]). More go at once only where they disturb nobody: a
//! second in library work's own hours once nothing has played for ten
//! minutes (someone who just stopped often starts something else, and a
//! playback that starts puts off every file being made), a third and a
//! fourth in the hours people rarely watch. Each needs a core the transcoder
//! leaves idle, at least what each file being made takes, and memory to
//! spare, and is started a while after the one before, so that the
//! transcoder's look at its processors has seen what that one takes. Files
//! go at once only from different storage: one file at a time is read from
//! a hard disk, an array of them, a ZFS pool or a network server, and up to
//! four from a solid-state disk or an array of nothing else ([`crate::disks`]
//! says how storage is told apart, and what goes alone where it can't be).
//!
//! Something that plays puts off the libraries' files within seconds, a
//! transcode all files but one (which the transcoder pauses), and a playback
//! that starts every file at once: they are made again later, in the order
//! they were in, and never count as failed. Only what the transcoder can't
//! make of the file itself does, until a restart.

use std::cmp::Reverse;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use pley_transcoder_api::Load;

use super::{Entry, Kind};
use crate::disks::Disk;
use crate::playback::QUIET_BEFORE_BACKGROUND_SECS;
use crate::schedule::Soon;
use crate::transcoder::TranscoderError;

/// The libraries' files made at once at most.
pub(super) const MOST_AT_ONCE: u32 = 4;
/// More than one of the libraries' files once nothing has played for this long.
const WIDE_AFTER_QUIET_SECS: i64 = 600;
/// Time between the starts of the libraries' files beyond the first: the
/// transcoder looks at its processors every 5 s, each look weighing as much
/// as those before it, so after four a file just started shows at 94 %.
pub(super) const SETTLE: Duration = Duration::from_secs(20);
/// After a title someone opened is made, its disk takes no library file
/// for this long: people open titles one after another.
pub(super) const OPENED_GRACE: Duration = Duration::from_secs(30);
/// A further file needs at least a core idle, so that it doesn't merely
/// slow the others (they run at a low priority: only idle time is free).
const MIN_IDLE_CORES: f64 = 1.0;
/// What a file being made is reckoned to take at least, so that one
/// waiting for its disk doesn't make the next look free.
const MIN_RUN_CORES: f64 = 0.5;
/// Memory a further file needs free: up to a gigabyte for its own (a 10-bit
/// 4K frame is 25 MB, and FFmpeg keeps a few dozen), and as much for a
/// playback that starts before the files are put off.
const MEMORY_PER_RUN: u64 = 2 << 30;
/// Below this many cores, a title someone opened is made alone: a second
/// FFmpeg would halve what someone waits for.
pub(super) const FEW_CORES: f64 = 4.0;

/// Why no more of the libraries' files are made at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Narrow {
    /// Not in these hours, or something played lately.
    Hours,
    /// The transcoder makes no more, or can't tell its load.
    Transcoder,
    /// The processors have no core to spare, or the last file started just now.
    Processors,
    Memory,
    /// The files left are on disks that are read already.
    Storage,
    /// As many as are ever made at once.
    Most,
}

/// Why a file being made was put off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Why {
    /// Something plays (a player came back without a new start).
    Playing,
    /// Something is transcoded.
    Transcode,
    /// A title someone opened reads the same disk.
    Opened,
}

impl Why {
    pub(super) fn says(self) -> &'static str {
        match self {
            Why::Playing => "something plays",
            Why::Transcode => "something is transcoded",
            Why::Opened => "a title someone opened goes first",
        }
    }
}

/// How making a file's previews ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    Made,
    /// What the transcoder can't make of this version of the file: not
    /// asked for again until a restart.
    Failed,
    /// Not made now (the file is gone, the transcoder couldn't reach it):
    /// out of the queue until it is asked for again (opened, or a scan).
    Dropped,
    /// The transcoder isn't there or says "not now": the file keeps its
    /// place, and nothing starts until the next look.
    Again,
    /// Put off: the file keeps its place and its turn.
    PutOff,
    /// Another file with these contents (the same file in two libraries) is
    /// being made: this one waits for it, then finds its previews.
    Twin(Arc<str>),
}

/// A file being made, as the rules see it.
#[derive(Debug, Clone)]
pub(super) struct RunView<'a> {
    pub id: i64,
    pub kind: Kind,
    pub disk: &'a Disk,
    /// Its order among the files started.
    pub seq: u64,
}

/// What [`widen`] goes by.
#[derive(Debug, Clone, Copy)]
pub(super) struct Facts {
    /// Sets the transcoder makes at once ([`crate::transcoder::Transcoder::trickplay_runs`]).
    pub capacity: u32,
    pub load: Option<Load>,
    /// The rest of the libraries may be done now ([`crate::schedule::Class::Later`]).
    pub later: bool,
    /// This hour and the next are rarely watched ([`crate::schedule::Scheduler::wide`]).
    pub wide: bool,
    /// Seconds since anything played.
    pub quiet_for: i64,
    /// Since the last of the libraries' files started.
    pub since_library_start: Option<Duration>,
    pub settle: Duration,
}

/// The libraries' files made at once at most with this transcoder: one set's
/// room is kept for a title someone opens, and each file gets a core.
fn most(capacity: u32, cores: f64) -> u32 {
    MOST_AT_ONCE.min(capacity.saturating_sub(1)).min(cores.max(0.0).floor() as u32).max(1)
}

/// Whether one more of the libraries' files may start beside the `library`
/// ones being made (`all` with a title someone opened); why not otherwise.
/// The first needs only library work's turn, as ever.
pub(super) fn widen(f: &Facts, library: u32, all: u32) -> Result<(), Narrow> {
    let Some(load) = f.load.filter(|_| f.capacity >= 3) else { return Err(Narrow::Transcoder) };
    let (Some(busy), Some(foreign), Some(memory)) = (load.busy, load.foreign, load.memory_available) else {
        return Err(Narrow::Transcoder);
    };
    if !f.later || f.quiet_for < WIDE_AFTER_QUIET_SECS || (library >= 2 && !f.wide) {
        return Err(Narrow::Hours);
    }
    if library + 1 > most(f.capacity, load.cores) {
        return Err(Narrow::Most);
    }
    if f.since_library_start.is_some_and(|since| since < f.settle) {
        return Err(Narrow::Processors);
    }
    // The next file is reckoned to take what each one going now takes.
    let idle = load.cores * (1.0 - busy);
    let ours = (load.cores * busy - foreign).max(0.0);
    let per_run = (ours / f64::from(all.max(1))).max(MIN_RUN_CORES);
    if idle < per_run.max(MIN_IDLE_CORES) {
        return Err(Narrow::Processors);
    }
    if memory < MEMORY_PER_RUN {
        return Err(Narrow::Memory);
    }
    Ok(())
}

/// Whether a file on `disk` may start beside the files being made: fewer
/// are read from its disk than it takes, it isn't the disk of a title just
/// opened (`grace`), and nothing is read from a storage that can't be told
/// apart (which goes alone).
pub(super) fn room(disk: &Disk, runs: &[RunView], grace: Option<&Disk>) -> bool {
    if grace.is_some_and(|g| g.width == 0 || g.key == disk.key) {
        return false;
    }
    if disk.width == 0 {
        return runs.is_empty();
    }
    if runs.iter().any(|r| r.disk.width == 0) {
        return false;
    }
    runs.iter().filter(|r| r.disk.key == disk.key).count() < usize::from(disk.width)
}

/// The title someone opened to make next: the first that `startable` lets.
pub(super) fn pick_opened(opened: &VecDeque<Entry>, startable: impl Fn(&Entry) -> bool) -> Option<i64> {
    opened.iter().find(|e| startable(e)).map(|e| e.id)
}

/// The library file to make next: what is likely to be watched soon first,
/// then (in the hours for the rest, `later`) the libraries in order, then
/// those whose previews an earlier version made. Whole queues are looked
/// through: a file on a disk that is free goes before one on a disk that is
/// read already.
pub(super) fn pick_library(
    library: &VecDeque<Entry>,
    refresh: &VecDeque<Entry>,
    soon: &Soon,
    later: bool,
    startable: impl Fn(&Entry) -> bool,
) -> Option<(Kind, i64)> {
    if let Some(e) = library.iter().find(|e| soon.files.contains(&e.id) && startable(e)) {
        return Some((Kind::Library, e.id));
    }
    if !later {
        return None;
    }
    let library = library.iter().find(|e| startable(e)).map(|e| (Kind::Library, e.id));
    library.or_else(|| refresh.iter().find(|e| startable(e)).map(|e| (Kind::Refresh, e.id)))
}

/// The library files a title someone opened on `disk` puts off: those read
/// from its disk beyond what the disk takes beside it (the newest first),
/// or all of them where the transcoder has few cores or a disk can't be
/// told apart.
pub(super) fn opened_puts_off(disk: &Disk, runs: &[RunView], few_cores: bool) -> Vec<i64> {
    let library = runs.iter().filter(|r| r.kind != Kind::Opened);
    if few_cores || disk.width == 0 || library.clone().any(|r| r.disk.width == 0) {
        return library.map(|r| r.id).collect();
    }
    let mut same: Vec<&RunView> = library.filter(|r| r.disk.key == disk.key).collect();
    same.sort_by_key(|r| Reverse(r.seq));
    let beside = usize::from(disk.width).saturating_sub(1);
    let over = same.len().saturating_sub(beside);
    same[..over].iter().map(|r| r.id).collect()
}

/// The files to put off now: the libraries' while something plays (`quiet_for`
/// under a minute), and all but one while something is transcoded (the
/// title someone opened, else the library file started first: the
/// transcoder pauses it, and one paused file keeps its memory, not five).
pub(super) fn put_offs(runs: &[RunView], quiet_for: i64, transcoding: bool) -> Vec<(i64, Why)> {
    let playing = quiet_for < QUIET_BEFORE_BACKGROUND_SECS;
    let mut off: Vec<(i64, Why)> =
        runs.iter().filter(|r| playing && r.kind != Kind::Opened).map(|r| (r.id, Why::Playing)).collect();
    if transcoding {
        let left: Vec<&RunView> = runs.iter().filter(|r| !off.iter().any(|(id, _)| *id == r.id)).collect();
        let kept = left.iter().find(|r| r.kind == Kind::Opened).or_else(|| left.iter().min_by_key(|r| r.seq));
        let kept = kept.map(|r| r.id);
        off.extend(left.iter().filter(|r| Some(r.id) != kept).map(|r| (r.id, Why::Transcode)));
    }
    off
}

/// What a request the transcoder answered with `err` means for the file.
pub(super) fn outcome(err: &TranscoderError) -> Outcome {
    match err {
        // No video, an unknown length, FFmpeg failing on it, or longer than
        // its file's size allows: the file version's own.
        TranscoderError::Api { status: 400, .. } => Outcome::Failed,
        TranscoderError::Api { status: 500, code, .. } if code == "ffmpeg_failed" => Outcome::Failed,
        TranscoderError::Api { status: 504, code, .. } if code == "timeout" => Outcome::Failed,
        // The transcoder's own "not now" (a proxy's page says nothing).
        TranscoderError::Api { status: 503, code, .. } if code != "http" => Outcome::Again,
        TranscoderError::Unavailable(_) => Outcome::Again,
        TranscoderError::Http(err) if err.is_connect() => Outcome::Again,
        // A share not mounted, a moment's trouble, a request that took too
        // long: the next scan or opening asks again.
        _ => Outcome::Dropped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn disk(key: &str, width: u8) -> Disk {
        Disk { key: Arc::from(key), width }
    }

    fn entry(id: i64) -> Entry {
        Entry { id, fingerprint: None }
    }

    fn run(id: i64, kind: Kind, disk: &Disk, seq: u64) -> RunView<'_> {
        RunView { id, kind, disk, seq }
    }

    /// Library files being made, started in the order given.
    fn library<'a>(runs: &[(i64, &'a Disk)]) -> Vec<RunView<'a>> {
        runs.iter().map(|&(id, disk)| run(id, Kind::Library, disk, id as u64)).collect()
    }

    fn load(cores: f64, busy: f64, foreign: f64, memory: u64) -> Load {
        Load { cores, busy: Some(busy), foreign: Some(foreign), memory_available: Some(memory) }
    }

    fn facts() -> Facts {
        Facts {
            capacity: 5,
            load: Some(load(8.0, 0.2, 0.4, 8 * GIB)),
            later: true,
            wide: true,
            quiet_for: 3600,
            since_library_start: Some(Duration::from_secs(60)),
            settle: SETTLE,
        }
    }

    #[test]
    fn a_second_library_file_needs_library_hours_quiet_minutes_processors_and_memory() {
        let second = |f: Facts| widen(&f, 1, 1);
        // 6.4 cores idle, each file taking 1.2.
        assert_eq!(second(facts()), Ok(()));
        assert_eq!(second(Facts { load: None, ..facts() }), Err(Narrow::Transcoder), "an older transcoder");
        let unmeasured = Load { busy: None, ..load(8.0, 0.2, 0.4, 8 * GIB) };
        assert_eq!(second(Facts { load: Some(unmeasured), ..facts() }), Err(Narrow::Transcoder), "macOS");
        assert_eq!(second(Facts { capacity: 2, ..facts() }), Err(Narrow::Transcoder), "one for opened titles");
        assert_eq!(second(Facts { later: false, ..facts() }), Err(Narrow::Hours));
        assert_eq!(second(Facts { quiet_for: 599, ..facts() }), Err(Narrow::Hours));
        let just = Facts { since_library_start: Some(Duration::from_secs(5)), ..facts() };
        assert_eq!(second(just), Err(Narrow::Processors), "settling");
        // 2.4 cores idle, but each file going takes 5.6.
        assert_eq!(second(Facts { load: Some(load(8.0, 0.7, 0.0, 8 * GIB)), ..facts() }), Err(Narrow::Processors));
        // 0.9 idle: not a whole core.
        assert_eq!(second(Facts { load: Some(load(2.0, 0.55, 0.0, 8 * GIB)), ..facts() }), Err(Narrow::Processors));
        let short = Facts { load: Some(load(8.0, 0.2, 0.4, 3 * GIB / 2)), ..facts() };
        assert_eq!(second(short), Err(Narrow::Memory));
    }

    #[test]
    fn more_than_two_need_hours_people_rarely_watch_and_room_on_the_transcoder() {
        assert_eq!(widen(&Facts { wide: false, ..facts() }, 2, 2), Err(Narrow::Hours));
        assert_eq!(widen(&facts(), 2, 2), Ok(()));
        assert_eq!(widen(&facts(), 3, 3), Ok(()));
        assert_eq!(widen(&facts(), 4, 4), Err(Narrow::Most), "four at most");
        assert_eq!(widen(&Facts { capacity: 4, ..facts() }, 3, 3), Err(Narrow::Most), "one kept for opened titles");
        let three_cores = Facts { load: Some(load(2.9, 0.1, 0.0, 8 * GIB)), ..facts() };
        assert_eq!(widen(&three_cores, 2, 2), Err(Narrow::Most), "a core each");
    }

    #[test]
    fn the_first_file_whose_disk_has_room_goes() {
        let (one, two) = (disk("disk:sda", 1), disk("disk:sdb", 1));
        let (ssd, unknown) = (disk("disk:nvme0n1", 4), disk("volume:x", 0));
        let none = VecDeque::new();
        // The head is on a disk read already: a file further down, on a free disk, goes.
        let queue: VecDeque<Entry> = (1..=501).map(entry).collect();
        let disk = |id: i64| if id == 501 { &two } else { &one };
        let reading = library(&[(1, &one)]);
        let startable = |e: &Entry| e.id != 1 && room(disk(e.id), &reading, None);
        assert_eq!(pick_library(&queue, &none, &Soon::default(), true, startable), Some((Kind::Library, 501)));
        // What is likely to be watched soon goes first, wherever it is.
        let free = |e: &Entry| room(disk(e.id), &[], None);
        let soon = Soon { files: [400, 501].into(), ..Default::default() };
        assert_eq!(pick_library(&queue, &none, &soon, false, free), Some((Kind::Library, 400)));
        // Out of the hours for the rest, only that.
        assert_eq!(pick_library(&queue, &none, &Soon::default(), false, free), None);
        // Previews made again come last, in the hours for the rest.
        let refresh: VecDeque<Entry> = [entry(900)].into();
        assert_eq!(pick_library(&none, &refresh, &Soon::default(), true, free), Some((Kind::Refresh, 900)));
        assert_eq!(pick_library(&none, &refresh, &Soon::default(), false, free), None);
        assert_eq!(pick_library(&queue, &refresh, &Soon::default(), true, |_| false), None, "nothing can start");

        // A solid-state disk takes four.
        assert!(room(&ssd, &library(&[(1, &ssd), (2, &ssd), (3, &ssd)]), None));
        assert!(!room(&ssd, &library(&[(1, &ssd), (2, &ssd), (3, &ssd), (4, &ssd)]), None));
        // A storage that can't be told apart goes alone, and nothing beside it.
        assert!(room(&unknown, &[], None));
        assert!(!room(&unknown, &library(&[(1, &two)]), None));
        assert!(!room(&two, &library(&[(1, &unknown)]), None));
        // The disk of a title just opened rests; one that can't be told apart, all.
        assert!(!room(&one, &[], Some(&one)));
        assert!(room(&two, &[], Some(&one)));
        assert!(!room(&two, &[], Some(&unknown)));
    }

    #[test]
    fn an_opened_title_puts_off_the_library_files_on_its_disk() {
        let (one, two) = (disk("disk:sda", 1), disk("disk:sdb", 1));
        let (ssd, unknown) = (disk("disk:nvme0n1", 4), disk("volume:x", 0));
        let runs = library(&[(1, &one), (2, &two)]);
        assert_eq!(opened_puts_off(&one, &runs, false), [1]);
        assert_eq!(opened_puts_off(&two, &runs, false), [2]);
        assert!(opened_puts_off(&ssd, &runs, false).is_empty(), "other disks keep theirs");
        assert_eq!(opened_puts_off(&ssd, &runs, true), [1, 2], "few cores: all of them");
        assert_eq!(opened_puts_off(&unknown, &runs, false), [1, 2]);
        assert_eq!(opened_puts_off(&one, &library(&[(3, &unknown)]), false), [3]);
        // Four read from a solid-state disk: the newest gives way.
        let runs = [
            run(1, Kind::Library, &ssd, 1),
            run(2, Kind::Library, &ssd, 4),
            run(3, Kind::Refresh, &ssd, 3),
            run(4, Kind::Library, &ssd, 2),
        ];
        assert_eq!(opened_puts_off(&ssd, &runs, false), [2]);
        // Another opened title isn't put off.
        assert!(opened_puts_off(&one, &[run(9, Kind::Opened, &one, 1)], true).is_empty());
    }

    #[test]
    fn what_gives_way_to_playback_and_transcodes() {
        let (a, b) = (disk("disk:sda", 1), disk("disk:sdb", 1));
        let runs = [run(1, Kind::Opened, &a, 1), run(2, Kind::Library, &b, 3), run(3, Kind::Refresh, &a, 2)];
        assert_eq!(put_offs(&runs, 30, false), [(2, Why::Playing), (3, Why::Playing)], "not the opened title");
        assert_eq!(put_offs(&runs, 3600, true), [(2, Why::Transcode), (3, Why::Transcode)], "the opened title stays");
        let runs = [run(2, Kind::Library, &a, 2), run(4, Kind::Library, &b, 5), run(5, Kind::Refresh, &a, 7)];
        assert_eq!(put_offs(&runs, 3600, true), [(4, Why::Transcode), (5, Why::Transcode)], "the oldest stays");
        assert_eq!(put_offs(&runs, 30, true), [(2, Why::Playing), (4, Why::Playing), (5, Why::Playing)]);
        assert!(put_offs(&runs, 3600, false).is_empty());
    }

    #[tokio::test]
    async fn only_what_the_transcoder_cannot_make_fails() {
        let api = |status: u16, code: &str| TranscoderError::Api { status, code: code.into(), message: String::new() };
        for err in [api(400, "bad_request"), api(500, "ffmpeg_failed"), api(504, "timeout")] {
            assert_eq!(outcome(&err), Outcome::Failed, "{err:?}");
        }
        // Nothing listens on a port just freed.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let refused = reqwest::get(format!("http://127.0.0.1:{port}/v1/trickplay")).await.unwrap_err();
        for err in [api(503, "busy"), TranscoderError::Unavailable(None), TranscoderError::Http(refused)] {
            assert_eq!(outcome(&err), Outcome::Again, "{err:?}");
        }
        // A transcoder that doesn't answer in time.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let client = reqwest::Client::builder().timeout(Duration::from_millis(100)).build().unwrap();
        let late = client.get(format!("http://{addr}/v1/trickplay")).send().await.unwrap_err();
        drop(listener);
        for err in [
            api(404, "not_found"),
            api(401, "unauthorized"),
            api(403, "forbidden"),
            api(500, "internal"),
            api(502, "http"),
            api(503, "http"),
            api(504, "http"),
            TranscoderError::Http(late),
        ] {
            assert_eq!(outcome(&err), Outcome::Dropped, "{err:?}");
        }
    }
}
