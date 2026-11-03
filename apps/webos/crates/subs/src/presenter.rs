//! What subtitles show at a playback position: the server's WebVTT, merged
//! as partial answers come in, or its `.mks` of a picture track, read as it
//! grows and decoded when shown (the TV app's `subs/presenter.ts`, Android's
//! `ServerSubtitles.kt` and `PictureSubtitles.kt`). A video on a drive hands
//! its picture tracks over as they are read out of the file, with no `.mks`.
//!
//! The presenter fetches nothing: [`Presenter::next_fetch`] says what the
//! player should ask the server for and when, and the answers come back
//! through [`Presenter::text_arrived`], [`Presenter::pictures_arrived`] and
//! [`Presenter::fetch_failed`]. Each frame the player asks
//! [`Presenter::update`] what shows at its position; that costs a
//! comparison until what shows changes, and a binary search when it does or
//! the position jumps.

use std::sync::Arc;

use crate::picture::{MksReader, PictureCue, PictureSubtitles, PictureTrack};
use crate::sync::clamp_offset;
use crate::vtt::{CueIndex, TextCue, parse_vtt, reaches_further};

/// A partial answer (a track still being read out of its file) is asked for
/// again this long after it arrived.
pub const PARTIAL_RETRY_MS: u64 = 3_000;
/// The header that marks a partial answer (`X-Pley-Complete: 0`).
pub const COMPLETE_HEADER: &str = "X-Pley-Complete";
/// How long cue checks wait at most, and at least, while cues change; and
/// while nothing changes any more (Android's cue loop).
const MAX_WAIT_MS: u64 = 250;
const MIN_WAIT_MS: u64 = 20;
const IDLE_WAIT_MS: u64 = 500;

/// Whether an answer is whole: the header absent or anything but `0`.
pub fn is_complete(header: Option<&str>) -> bool {
    header.is_none_or(|v| v.trim() != "0")
}

/// The kind of track shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    /// The server's WebVTT (`.vtt`).
    Text,
    /// The server's Matroska of a picture track (`.mks`).
    Pictures,
}

impl TrackKind {
    /// The `Accept` header to send.
    pub fn accept(self) -> &'static str {
        match self {
            TrackKind::Text => "text/vtt",
            TrackKind::Pictures => "video/x-matroska",
        }
    }

    /// How long to wait for an answer: the server may read a whole video to
    /// make picture subtitles the first time, minutes on a network share.
    pub fn timeout_ms(self) -> u64 {
        match self {
            TrackKind::Text => 120_000,
            TrackKind::Pictures => 15 * 60_000,
        }
    }
}

/// A request the player makes for the track shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetch {
    /// Pass it back with the answer; answers to requests made for another
    /// choice are ignored.
    pub id: u64,
    /// The track's address, as given to [`Presenter::show_text`] or [`Presenter::show_pictures`].
    pub url: Arc<str>,
    pub kind: TrackKind,
    /// Ask for the file from this byte on (`Range: bytes=<n>-`); `None` for the whole file.
    pub range_from: Option<u64>,
    /// Send it this long from now, in ms: at once, or [`PARTIAL_RETRY_MS`] after a partial answer.
    pub after_ms: u64,
}

impl Fetch {
    /// The `Range` header's value, when it asks for part of the file.
    pub fn range_header(&self) -> Option<String> {
        self.range_from.map(|n| format!("bytes={n}-"))
    }
}

/// An answer for a picture track.
#[derive(Clone, Debug, PartialEq)]
pub enum PictureAnswer {
    /// The file from its start (200).
    Whole(Vec<u8>),
    /// The file from `start` (206, as its `Content-Range` says).
    Range { start: u64, bytes: Vec<u8> },
    /// The range asked for starts past what the server holds (416).
    RangeRefused,
    /// The track as read so far, handed over as it is (a video on a drive,
    /// read out of the file): every sample found yet, in place of the last.
    Track(PictureTrack),
}

/// Why a track can't show, for the player to tell the person or act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// "These subtitles couldn't be loaded." Choosing the track again retries.
    Failed,
    /// The server can't make picture subtitles (404 from an old server, 503
    /// from an old transcoder) or they are in a codec the app doesn't draw:
    /// the player burns the track in instead.
    PicturesUnavailable,
}

/// What shows now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shown {
    /// Text cues, earliest first.
    pub text: Vec<Arc<TextCue>>,
    /// Pictures, as their frame places them.
    pub pictures: Vec<PictureCue>,
    /// Changes whenever what shows changes, so the screen can keep what it
    /// laid out until then.
    pub key: u64,
}

impl Shown {
    /// Nothing shows.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.pictures.is_empty()
    }
}

enum Source {
    Text(CueIndex),
    Pictures { reader: Box<MksReader>, subs: Option<Box<PictureSubtitles>> },
}

enum Loading {
    /// The next request: from which byte, and how long from now.
    Due {
        range_from: Option<u64>,
        after_ms: u64,
    },
    InFlight(u64),
    Done,
    Failed,
}

struct Track {
    kind: TrackKind,
    url: Arc<str>,
    source: Source,
    loading: Loading,
}

/// The subtitles shown during a playback: one track at a time, what shows
/// at the position, and what to fetch next.
#[derive(Default)]
pub struct Presenter {
    track: Option<Track>,
    shown: Shown,
    /// The track time `shown` was found at and when it changes next: it
    /// holds in between.
    valid: Option<(i64, Option<i64>)>,
    /// The track time of the last update.
    at: i64,
    offset_ms: i32,
    next_id: u64,
    /// A partial picture track is asked for again after the first of these
    /// (ms), then twice as long each time up to the second; none: every
    /// [`PARTIAL_RETRY_MS`].
    picture_retry: Option<(u64, u64)>,
    /// The wait after the next partial picture answer; 0: the first.
    picture_wait: u64,
}

impl Presenter {
    /// A presenter showing no track.
    pub fn new() -> Presenter {
        Presenter::default()
    }

    /// Asks again for a partial picture track after `first_ms`, then twice as
    /// long each time, `most_ms` at most: a video on a drive (1 s up to 4 s),
    /// whose answer is the samples in memory, not the server's `.mks` so far.
    pub fn set_picture_retry(&mut self, first_ms: u64, most_ms: u64) {
        self.picture_retry = Some((first_ms, most_ms.max(first_ms)));
        self.picture_wait = 0;
    }

    /// The playback jumped: what shows is looked up again, and a track
    /// still being read is asked for again at once, from the first wait, so
    /// what was read meanwhile shows where it landed.
    pub fn sought(&mut self) {
        self.picture_wait = 0;
        self.valid = None;
        if let Some(Track { loading: Loading::Due { after_ms, .. }, .. }) = &mut self.track {
            *after_ms = 0;
        }
    }

    /// The wait after a partial picture answer, as things stand.
    fn picture_retry_ms(&self) -> u64 {
        match self.picture_retry {
            None => PARTIAL_RETRY_MS,
            Some((first, _)) if self.picture_wait == 0 => first,
            Some(_) => self.picture_wait,
        }
    }

    /// A partial picture answer came: the next waits twice as long, up to the most.
    fn picture_retried(&mut self) {
        if let Some((_, most)) = self.picture_retry {
            self.picture_wait = self.picture_retry_ms().saturating_mul(2).min(most);
        }
    }

    /// Shows the server's WebVTT at `url`. The same track again keeps what
    /// was fetched, and retries one that failed.
    pub fn show_text(&mut self, url: &str) {
        self.show(TrackKind::Text, url);
    }

    /// Shows the server's `.mks` of a picture track at `url`.
    pub fn show_pictures(&mut self, url: &str) {
        self.show(TrackKind::Pictures, url);
    }

    /// Shows no subtitles.
    pub fn hide(&mut self) {
        self.track = None;
        self.set_shown(Vec::new(), Vec::new());
        self.valid = None;
    }

    /// The track shown: its kind and address.
    pub fn showing(&self) -> Option<(TrackKind, &str)> {
        self.track.as_ref().map(|t| (t.kind, &*t.url))
    }

    /// What shows, as of the last [`Presenter::update`].
    pub fn shown(&self) -> &Shown {
        &self.shown
    }

    fn show(&mut self, kind: TrackKind, url: &str) {
        self.picture_wait = 0;
        if let Some(t) = &mut self.track
            && t.kind == kind
            && &*t.url == url
        {
            if matches!(t.loading, Loading::Failed) {
                t.loading = Loading::Due { range_from: None, after_ms: 0 };
            }
            self.valid = None;
            return;
        }
        let source = match kind {
            TrackKind::Text => Source::Text(CueIndex::default()),
            TrackKind::Pictures => Source::Pictures { reader: Box::default(), subs: None },
        };
        self.track =
            Some(Track { kind, url: url.into(), source, loading: Loading::Due { range_from: None, after_ms: 0 } });
        self.set_shown(Vec::new(), Vec::new());
        self.valid = None;
    }

    /// The request to make now for the track shown, if one is due; it is
    /// then in flight until its answer or failure comes back.
    pub fn next_fetch(&mut self) -> Option<Fetch> {
        let t = self.track.as_mut()?;
        let Loading::Due { range_from, after_ms } = t.loading else { return None };
        self.next_id += 1;
        t.loading = Loading::InFlight(self.next_id);
        Some(Fetch { id: self.next_id, url: t.url.clone(), kind: t.kind, range_from, after_ms })
    }

    /// The track in flight for request `id`, if that is still the one wanted.
    fn answered(&mut self, id: u64, kind: TrackKind) -> Option<&mut Track> {
        self.track.as_mut().filter(|t| t.kind == kind && matches!(t.loading, Loading::InFlight(i) if i == id))
    }

    /// The answer to text request `id`: the body, and whether it is whole
    /// ([`is_complete`]). A partial answer keeps whichever of it and what was
    /// had reaches further, and asks again in 3 s.
    pub fn text_arrived(&mut self, id: u64, body: &str, complete: bool) {
        let Some(t) = self.answered(id, TrackKind::Text) else { return };
        let fresh = parse_vtt(body);
        let mut changed = false;
        if let Source::Text(index) = &mut t.source
            && reaches_further(index.cues().iter().map(|c| &**c), &fresh)
        {
            *index = CueIndex::new(fresh);
            changed = true;
        }
        t.loading =
            if complete { Loading::Done } else { Loading::Due { range_from: None, after_ms: PARTIAL_RETRY_MS } };
        if changed {
            self.valid = None;
        }
    }

    /// The answer to picture request `id`, and whether it is whole. A
    /// partial answer asks for the rest in 3 s (or as
    /// [`Presenter::set_picture_retry`] says), from the bytes it lacks.
    pub fn pictures_arrived(&mut self, id: u64, answer: PictureAnswer, complete: bool) -> Result<(), Trouble> {
        let retry = self.picture_retry_ms();
        let Some(t) = self.answered(id, TrackKind::Pictures) else { return Ok(()) };
        let Source::Pictures { reader, subs } = &mut t.source else { return Ok(()) };
        let (fed, handed) = match answer {
            PictureAnswer::Whole(bytes) => (reader.feed_whole(bytes).map(|()| true), None),
            PictureAnswer::Range { start, bytes } => (reader.feed_range(start, bytes), None),
            PictureAnswer::RangeRefused => (Ok(false), None),
            // Handed over as it is: no `.mks` to read.
            PictureAnswer::Track(track) => (Ok(true), Some(track)),
        };
        let outcome = match fed {
            // The file changed on the server: all of it again.
            Ok(false) => Ok(Loading::Due { range_from: None, after_ms: 0 }),
            Ok(true) => match handed.map_or_else(|| reader.track(), |track| Ok(Some(track))) {
                Ok(None) if complete => Err(Trouble::Failed),
                Ok(None) => Ok(Loading::Due { range_from: reader.resume_from(), after_ms: retry }),
                Ok(Some(track)) => {
                    let ready = match subs {
                        Some(s)
                            if s.track().codec_id == track.codec_id
                                && s.track().codec_private == track.codec_private =>
                        {
                            s.replace_samples(track.samples);
                            true
                        }
                        _ => match PictureSubtitles::of(track) {
                            Some(s) => {
                                *subs = Some(Box::new(s));
                                true
                            }
                            None => false,
                        },
                    };
                    if !ready {
                        Err(Trouble::PicturesUnavailable)
                    } else if complete {
                        Ok(Loading::Done)
                    } else {
                        Ok(Loading::Due { range_from: reader.resume_from(), after_ms: retry })
                    }
                }
                Err(_) => Err(Trouble::Failed),
            },
            Err(_) => Err(Trouble::Failed),
        };
        if matches!(outcome, Ok(Loading::Due { after_ms, .. }) if after_ms > 0) {
            self.picture_retried();
        }
        let Some(t) = self.track.as_mut() else { return Ok(()) };
        let result = match outcome {
            Ok(loading) => {
                t.loading = loading;
                Ok(())
            }
            Err(trouble) => {
                t.loading = Loading::Failed;
                Err(trouble)
            }
        };
        self.valid = None;
        result
    }

    /// Request `id` failed: with the server's status when it answered one.
    /// Says what to tell the person, if anything.
    pub fn fetch_failed(&mut self, id: u64, status: Option<u16>) -> Option<Trouble> {
        let kind = self.track.as_ref()?.kind;
        let t = self.answered(id, kind)?;
        let trouble = match (kind, status) {
            (TrackKind::Pictures, Some(416)) => {
                t.loading = Loading::Due { range_from: None, after_ms: 0 };
                return None;
            }
            (TrackKind::Pictures, Some(404 | 503)) => Trouble::PicturesUnavailable,
            _ => Trouble::Failed,
        };
        t.loading = Loading::Failed;
        Some(trouble)
    }

    /// Finds what shows at `position_ms` with subtitles moved by `offset_ms`
    /// (positive is later; stepped and clamped as the sync row does). True
    /// when that changed since the last call.
    pub fn update(&mut self, position_ms: u64, offset_ms: i64) -> bool {
        let offset = clamp_offset(offset_ms);
        if offset != self.offset_ms {
            self.offset_ms = offset;
            self.valid = None;
        }
        let at = position_ms.min(i64::MAX as u64) as i64 - i64::from(offset);
        self.at = at;
        if let Some((from, until)) = self.valid
            && at >= from
            && until.is_none_or(|u| at < u)
        {
            return false;
        }
        let key = self.shown.key;
        let (text, pictures, next) = match self.track.as_mut().map(|t| &mut t.source) {
            Some(Source::Text(index)) => (index.at(at).cloned().collect(), Vec::new(), index.next_change(at)),
            Some(Source::Pictures { subs: Some(subs), .. }) => {
                let pictures = subs.cues_at(at).map(|(_, cues)| cues).unwrap_or_default();
                (Vec::new(), pictures, subs.next_change(at))
            }
            _ => (Vec::new(), Vec::new(), None),
        };
        self.set_shown(text, pictures);
        self.valid = Some((at, next));
        self.shown.key != key
    }

    /// The playback position at which what shows changes next, as of the
    /// last [`Presenter::update`]; `None` when nothing will (as far as the
    /// track is known).
    pub fn next_change_ms(&self) -> Option<u64> {
        let (_, until) = self.valid?;
        Some((until? + i64::from(self.offset_ms)).max(0) as u64)
    }

    /// How long until the player should look again, in ms of wall-clock
    /// time at `speed`: until the next change, within 20 to 250 ms, or 500 ms
    /// when nothing changes any more (Android's cue loop).
    pub fn wait_ms(&self, speed: f32) -> u64 {
        let Some((_, Some(until))) = self.valid else { return IDLE_WAIT_MS };
        let wait = (until - self.at) as f64 / f64::from(speed.max(0.01));
        (wait.ceil() as u64).clamp(MIN_WAIT_MS, MAX_WAIT_MS)
    }

    fn set_shown(&mut self, text: Vec<Arc<TextCue>>, pictures: Vec<PictureCue>) {
        let same_text =
            text.len() == self.shown.text.len() && text.iter().zip(&self.shown.text).all(|(a, b)| Arc::ptr_eq(a, b));
        let same_pictures = pictures.len() == self.shown.pictures.len()
            && pictures.iter().zip(&self.shown.pictures).all(|(a, b)| a.key == b.key && a.end_ms == b.end_ms);
        if same_text && same_pictures {
            return;
        }
        self.shown = Shown { text, pictures, key: self.shown.key + 1 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picture::PictureSample;
    use crate::picture::mks::tests::{Block, cluster, header, mks};
    use crate::picture::tests::{empty_set, white_set};

    const PART: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nOne\n\n00:00:02.000 --> 00:00:04.000\nTwo\n";
    const WHOLE: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nOne\n\n00:00:02.000 --> 00:00:04.000\nTwo\n\n\
                         00:00:10.000 --> 00:00:12.000\nThree\n";

    fn texts(p: &Presenter) -> Vec<String> {
        p.shown().text.iter().map(|c| c.plain_text()).collect()
    }

    #[test]
    fn fetches_text_again_until_whole_and_shows_the_cues_at_the_position() {
        let mut p = Presenter::new();
        assert_eq!(p.next_fetch(), None);
        p.show_text("/api/files/7/subtitles/2.vtt");
        let first = p.next_fetch().unwrap();
        assert_eq!((first.kind, first.range_from, first.after_ms), (TrackKind::Text, None, 0));
        assert_eq!((first.kind.accept(), &*first.url), ("text/vtt", "/api/files/7/subtitles/2.vtt"));
        assert_eq!(p.next_fetch(), None, "one request at a time");
        p.text_arrived(first.id, PART, is_complete(Some("0")));
        assert!(p.update(2500, 0));
        assert_eq!(texts(&p), ["One", "Two"]);
        assert_eq!(p.next_change_ms(), Some(3000));
        let again = p.next_fetch().unwrap();
        assert_eq!(again.after_ms, PARTIAL_RETRY_MS);
        // A shorter answer (an old one) never removes cues.
        p.text_arrived(again.id, "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nOne\n", false);
        assert!(!p.update(2600, 0));
        let last = p.next_fetch().unwrap();
        p.text_arrived(last.id, WHOLE, is_complete(None));
        assert_eq!(p.next_fetch(), None, "whole: nothing more to ask");
        assert!(p.update(10_500, 0));
        assert_eq!(texts(&p), ["Three"]);
        assert_eq!(p.next_change_ms(), Some(12_000));
        assert!(p.update(13_000, 0));
        assert!(p.shown().is_empty());
        assert_eq!(p.next_change_ms(), None);
    }

    #[test]
    fn moves_cues_by_the_offset_and_looks_again_when_they_change() {
        let mut p = Presenter::new();
        p.show_text("a.vtt");
        let f = p.next_fetch().unwrap();
        p.text_arrived(f.id, WHOLE, true);
        // +0.5 s: a cue made for 1 s shows at 1.5 s.
        assert!(!p.update(1400, 500));
        assert!(p.update(1500, 500));
        assert_eq!(texts(&p), ["One"]);
        assert_eq!(p.next_change_ms(), Some(2500));
        assert_eq!(p.wait_ms(1.0), 250);
        assert!(!p.update(2400, 500), "nothing changes before 2.5 s");
        assert_eq!(p.wait_ms(1.0), 100);
        assert_eq!(p.wait_ms(2.0), 50);
        assert!(p.update(2500, 500));
        assert_eq!(texts(&p), ["One", "Two"]);
        // Steps of 100 ms within 10 s: 520 is 500.
        assert!(!p.update(2500, 520));
        // A jump back is looked up again.
        assert!(p.update(0, 0));
        assert!(p.shown().is_empty());
        assert_eq!(p.wait_ms(1.0), 250);
        assert!(!p.update(20_000, 0), "nothing showed before either");
        assert_eq!(p.wait_ms(1.0), 500);
        // The same track again keeps its cues; another starts afresh; hiding clears.
        p.show_text("a.vtt");
        assert_eq!(p.next_fetch(), None);
        assert!(p.update(1000, 0));
        p.hide();
        assert!(p.shown().is_empty() && p.showing().is_none());
    }

    #[test]
    fn ignores_answers_for_another_choice_and_retries_a_failed_track_when_chosen_again() {
        let mut p = Presenter::new();
        p.show_text("a.vtt");
        let old = p.next_fetch().unwrap();
        p.show_text("b.vtt");
        let new = p.next_fetch().unwrap();
        p.text_arrived(old.id, WHOLE, true);
        assert!(!p.update(1500, 0));
        assert_eq!(p.fetch_failed(old.id, Some(500)), None);
        assert_eq!(p.fetch_failed(new.id, None), Some(Trouble::Failed));
        assert_eq!(p.next_fetch(), None);
        p.show_text("b.vtt");
        let retry = p.next_fetch().unwrap();
        assert_eq!(retry.range_from, None);
        p.text_arrived(retry.id, WHOLE, true);
        assert!(p.update(1500, 0));
    }

    #[test]
    fn reads_pictures_as_they_grow_asking_only_for_new_bytes() {
        let head = header("S_HDMV/PGS", None);
        let first = cluster(0, &[Block { time: 1000, data: white_set(), duration: None }]);
        let second = cluster(2000, &[Block { time: 3000, data: empty_set(), duration: None }]);
        let mut p = Presenter::new();
        p.show_pictures("/api/files/7/subtitles/3.mks");
        // The server's first answer: the track's header alone.
        let f = p.next_fetch().unwrap();
        assert_eq!((f.kind.accept(), f.kind.timeout_ms()), ("video/x-matroska", 900_000));
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Whole(head.clone()), false), Ok(()));
        let f = p.next_fetch().unwrap();
        assert_eq!((f.range_from, f.after_ms), (None, PARTIAL_RETRY_MS));
        let mut so_far = head.clone();
        so_far.extend(&first);
        p.pictures_arrived(f.id, PictureAnswer::Whole(so_far.clone()), false).unwrap();
        assert!(p.update(1500, 0));
        let cue = p.shown().pictures[0].clone();
        assert_eq!((cue.start_ms, cue.end_ms), (1000, None));
        // Then only what is new, with a few bytes it holds.
        let f = p.next_fetch().unwrap();
        let from = f.range_from.unwrap();
        assert_eq!(from, so_far.len() as u64 - 16);
        assert_eq!(f.range_header(), Some(format!("bytes={from}-")));
        let mut whole = so_far.clone();
        whole.extend(&second);
        let rest = whole[from as usize..].to_vec();
        p.pictures_arrived(f.id, PictureAnswer::Range { start: from, bytes: rest }, true).unwrap();
        assert_eq!(p.next_fetch(), None);
        // The picture now ends where the next sample (an empty one) starts.
        assert!(p.update(1600, 0));
        assert_eq!(p.shown().pictures[0].end_ms, Some(3000));
        assert_eq!(p.shown().pictures[0].key, cue.key, "decoded once");
        assert!(p.update(3000, 0));
        assert!(p.shown().is_empty());
    }

    #[test]
    fn asks_for_the_whole_file_when_the_rest_doesnt_follow() {
        let mut so_far = header("S_HDMV/PGS", None);
        so_far.extend(cluster(0, &[Block { time: 1000, data: white_set(), duration: None }]));
        let mut p = Presenter::new();
        p.show_pictures("x.mks");
        let f = p.next_fetch().unwrap();
        p.pictures_arrived(f.id, PictureAnswer::Whole(so_far.clone()), false).unwrap();
        let f = p.next_fetch().unwrap();
        let from = f.range_from.unwrap();
        p.pictures_arrived(f.id, PictureAnswer::Range { start: from, bytes: vec![0xEE; 40] }, false).unwrap();
        let f = p.next_fetch().unwrap();
        assert_eq!((f.range_from, f.after_ms), (None, 0));
        p.pictures_arrived(f.id, PictureAnswer::Whole(so_far.clone()), false).unwrap();
        let f = p.next_fetch().unwrap();
        assert!(f.range_from.is_some());
        assert_eq!(p.fetch_failed(f.id, Some(416)), None);
        assert_eq!(p.next_fetch().unwrap().range_from, None);
        assert!(p.update(1200, 0));
    }

    #[test]
    fn says_when_pictures_cant_be_had() {
        let mut p = Presenter::new();
        p.show_pictures("x.mks");
        let f = p.next_fetch().unwrap();
        assert_eq!(p.fetch_failed(f.id, Some(503)), Some(Trouble::PicturesUnavailable));
        p.show_pictures("y.mks");
        let f = p.next_fetch().unwrap();
        assert_eq!(p.fetch_failed(f.id, Some(500)), Some(Trouble::Failed));
        p.show_pictures("z.mks");
        let f = p.next_fetch().unwrap();
        let text = mks("S_TEXT/UTF8", &[Block { time: 0, data: b"hi".to_vec(), duration: None }], None);
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Whole(text), true), Err(Trouble::PicturesUnavailable));
        p.show_pictures("w.mks");
        let f = p.next_fetch().unwrap();
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Whole(vec![1, 2, 3]), true), Err(Trouble::Failed));
        assert_eq!(p.next_fetch(), None);
    }

    fn pgs(samples: Vec<PictureSample>) -> PictureAnswer {
        PictureAnswer::Track(PictureTrack { codec_id: "S_HDMV/PGS".into(), codec_private: None, samples })
    }

    #[test]
    fn a_drives_track_is_asked_for_again_after_1_2_4_4_s_and_at_once_after_a_seek() {
        let first = PictureSample::new(1000, None, white_set());
        let mut p = Presenter::new();
        p.set_picture_retry(1_000, 4_000);
        p.show_pictures("/api/files/7/subtitles/3.mks");
        let mut f = p.next_fetch().unwrap();
        assert_eq!(f.after_ms, 0);
        let mut waits = Vec::new();
        for _ in 0..4 {
            p.pictures_arrived(f.id, pgs(vec![first.clone()]), false).unwrap();
            f = p.next_fetch().unwrap();
            waits.push(f.after_ms);
        }
        assert_eq!(waits, [1_000, 2_000, 4_000, 4_000]);
        // A seek: the request waiting goes at once, and the waits start over.
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false).unwrap();
        p.sought();
        let f = p.next_fetch().unwrap();
        assert_eq!(f.after_ms, 0);
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false).unwrap();
        assert_eq!(p.next_fetch().unwrap().after_ms, 1_000);

        // A server's track is asked for every 3 s.
        let mut p = Presenter::new();
        p.show_pictures("x.mks");
        for _ in 0..3 {
            let f = p.next_fetch().unwrap();
            p.pictures_arrived(f.id, pgs(vec![first.clone()]), false).unwrap();
        }
        assert_eq!(p.next_fetch().unwrap().after_ms, PARTIAL_RETRY_MS);
    }

    #[test]
    fn a_handed_over_track_replaces_its_samples() {
        let first = PictureSample::new(1000, None, white_set());
        let mut p = Presenter::new();
        p.show_pictures("/api/files/7/subtitles/3.mks");
        let f = p.next_fetch().unwrap();
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false).unwrap();
        assert!(p.update(1500, 0));
        let cue = p.shown().pictures[0].clone();
        assert_eq!((cue.start_ms, cue.end_ms), (1000, None));
        let f = p.next_fetch().unwrap();
        p.pictures_arrived(f.id, pgs(vec![first, PictureSample::new(3000, None, empty_set())]), true).unwrap();
        assert_eq!(p.next_fetch(), None);
        assert!(p.update(1600, 0));
        assert_eq!(p.shown().pictures[0].end_ms, Some(3000));
        assert_eq!(p.shown().pictures[0].key, cue.key, "decoded once");
        assert!(p.update(3000, 0));
        assert!(p.shown().is_empty());
        // A codec the app doesn't draw.
        p.show_pictures("/api/files/7/subtitles/4.mks");
        let f = p.next_fetch().unwrap();
        let text = PictureTrack { codec_id: "S_TEXT/UTF8".into(), codec_private: None, samples: Vec::new() };
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Track(text), true), Err(Trouble::PicturesUnavailable));
    }
}
