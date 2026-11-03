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
//!
//! The server's picture track is asked for from where the player is at every
//! ask (`?at=`, 0 at the start too; the Samsung app's `SubtitlePresenter`):
//! while the server still reads the track out of its file, it answers with
//! the pictures around there, and each answer replaces the last. It is asked
//! again 3 s after the ask, then twice as long each time up to 15 s, never
//! sooner than 3 s after the answer, which may have waited for a read at a
//! seek. An answer over 4 MB is a copy of the whole track so far (a file
//! whose index doesn't list it): it is asked for again from its answer, up
//! to 24 s on, as no read at a seek is to be spared for it. A seek asks at
//! once from where the player went and drops an answer still on its way. A
//! video on a drive is asked again 1 s after its answer, then twice as long
//! up to 4 s, and at once after a seek. Nothing asks for part of a file
//! (`Range`): an answer is a file of its own.

use std::sync::Arc;

use crate::picture::{MksReader, PictureCue, PictureSubtitles, PictureTrack};
use crate::sync::clamp_offset;
use crate::vtt::{CueIndex, TextCue, parse_vtt, reaches_further};

/// A partial answer (a track still being read out of its file) is asked for
/// again this long after it arrived; the server's pictures this long after
/// they were asked for, and never sooner after the answer.
pub const PARTIAL_RETRY_MS: u64 = 3_000;
/// The server's partial picture track is asked for again [`PARTIAL_RETRY_MS`]
/// after the ask, then twice as long each time up to this. Its answer is
/// mostly the pictures around where it was asked from, which are small; the
/// server reads those up to 40 s past where a player asked from and answers
/// from that read for 20 s on, so an ask this long after the last is
/// answered from the read the last one started, not a read of its own.
pub const PICTURES_RETRY_MOST_MS: u64 = 15_000;
/// An answer past [`COPY_MIN_BYTES`] is the whole track so far, tens of MB
/// for a film: it is asked for again this long after it came at most.
pub const COPY_RETRY_MOST_MS: u64 = 24_000;
/// More than the pictures around where a player is come to (two and a half
/// minutes of a Blu-ray's).
pub const COPY_MIN_BYTES: usize = 4 << 20;
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
    /// The server's picture track asked for from here, in ms of the file (a
    /// seek's target); none: from where the player is as the request goes.
    pub at_ms: Option<u64>,
    /// Send it this long from now, in ms: at once, or after a partial answer.
    pub after_ms: u64,
}

/// An answer for a picture track.
#[derive(Clone, Debug, PartialEq)]
pub enum PictureAnswer {
    /// The server's `.mks`: the whole track, or while it isn't whole the
    /// pictures around where it was asked from (or all of it so far).
    Whole(Vec<u8>),
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
    /// The next request: from where (a seek's target), and how long from now.
    Due {
        at_ms: Option<u64>,
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
    /// A video on a drive: its partial picture track is asked for again
    /// after the first of these (ms) from the answer, then twice as long each
    /// time up to the second. None: the server's.
    drive_retry: Option<(u64, u64)>,
    /// The wait for the next partial picture answer; 0: the first.
    picture_wait: u64,
}

impl Presenter {
    /// A presenter showing no track.
    pub fn new() -> Presenter {
        Presenter::default()
    }

    /// Shows a video on a drive's tracks, whose answer is the samples read
    /// so far, not the server's `.mks`: a partial picture track is asked for
    /// again `first_ms` after its answer, then twice as long each time,
    /// `most_ms` at most (1 s up to 4 s).
    pub fn set_drive_retry(&mut self, first_ms: u64, most_ms: u64) {
        self.drive_retry = Some((first_ms, most_ms.max(first_ms)));
        self.picture_wait = 0;
    }

    /// The playback jumped to `at_ms` of the file: what shows is looked up
    /// again. The server's picture track that isn't whole is asked for at
    /// once from there, the answer on its way for the old place dropped and
    /// the waits starting over; its text waits for its next ask. A drive's
    /// track still being read is asked for again at once, so what was read
    /// meanwhile shows where it landed.
    pub fn sought(&mut self, at_ms: u64) {
        self.valid = None;
        let Some(t) = &mut self.track else { return };
        if self.drive_retry.is_some() {
            self.picture_wait = 0;
            if let Loading::Due { after_ms, .. } = &mut t.loading {
                *after_ms = 0;
            }
        } else if t.kind == TrackKind::Pictures && matches!(t.loading, Loading::Due { .. } | Loading::InFlight(_)) {
            self.picture_wait = 0;
            t.loading = Loading::Due { at_ms: Some(at_ms), after_ms: 0 };
        }
    }

    /// How long after a partial picture answer of `bytes` to ask again, its
    /// ask `waited_ms` before; the wait after the next one doubles, up to the
    /// most for its kind.
    fn picture_retry_ms(&mut self, bytes: usize, waited_ms: u64) -> u64 {
        let wait = match (self.picture_wait, self.drive_retry) {
            (0, Some((first, _))) => first,
            (0, None) => PARTIAL_RETRY_MS,
            (wait, _) => wait,
        };
        let (after, most) = match self.drive_retry {
            Some((_, most)) => (wait, most),
            // A copy of the whole track so far: from its answer, and less often.
            None if bytes > COPY_MIN_BYTES => (wait, COPY_RETRY_MOST_MS),
            // The pictures around the player: counted from the ask.
            None => (wait.saturating_sub(waited_ms).max(PARTIAL_RETRY_MS), PICTURES_RETRY_MOST_MS),
        };
        self.picture_wait = wait.saturating_mul(2).min(most);
        after
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
                t.loading = Loading::Due { at_ms: None, after_ms: 0 };
            }
            self.valid = None;
            return;
        }
        let source = match kind {
            TrackKind::Text => Source::Text(CueIndex::default()),
            TrackKind::Pictures => Source::Pictures { reader: Box::default(), subs: None },
        };
        self.track = Some(Track { kind, url: url.into(), source, loading: Loading::Due { at_ms: None, after_ms: 0 } });
        self.set_shown(Vec::new(), Vec::new());
        self.valid = None;
    }

    /// The request to make now for the track shown, if one is due; it is
    /// then in flight until its answer or failure comes back.
    pub fn next_fetch(&mut self) -> Option<Fetch> {
        let t = self.track.as_mut()?;
        let Loading::Due { at_ms, after_ms } = t.loading else { return None };
        self.next_id += 1;
        t.loading = Loading::InFlight(self.next_id);
        Some(Fetch { id: self.next_id, url: t.url.clone(), kind: t.kind, at_ms, after_ms })
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
        t.loading = if complete { Loading::Done } else { Loading::Due { at_ms: None, after_ms: PARTIAL_RETRY_MS } };
        if changed {
            self.valid = None;
        }
    }

    /// The answer to picture request `id`, whether it is whole, and how long
    /// it took since it was asked for. A partial answer replaces what was
    /// had and asks again as the module says.
    pub fn pictures_arrived(
        &mut self,
        id: u64,
        answer: PictureAnswer,
        complete: bool,
        waited_ms: u64,
    ) -> Result<(), Trouble> {
        let Some(t) = self.answered(id, TrackKind::Pictures) else { return Ok(()) };
        let Source::Pictures { reader, subs } = &mut t.source else { return Ok(()) };
        let (bytes, read) = match answer {
            PictureAnswer::Whole(bytes) => {
                let n = bytes.len();
                (n, reader.feed_whole(bytes).and_then(|()| reader.track()).map_err(|_| Trouble::Failed))
            }
            // Handed over as it is: no `.mks` to read.
            PictureAnswer::Track(track) => (0, Ok(Some(track))),
        };
        let outcome = match read {
            Ok(None) if complete => Err(Trouble::Failed),
            Ok(None) => Ok(false),
            Ok(Some(track)) => {
                let ready = match subs {
                    Some(s)
                        if s.track().codec_id == track.codec_id && s.track().codec_private == track.codec_private =>
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
                if ready { Ok(complete) } else { Err(Trouble::PicturesUnavailable) }
            }
            Err(trouble) => Err(trouble),
        };
        self.valid = None;
        let loading = match outcome {
            Ok(true) => Loading::Done,
            Ok(false) => Loading::Due { at_ms: None, after_ms: self.picture_retry_ms(bytes, waited_ms) },
            Err(_) => Loading::Failed,
        };
        if let Some(t) = self.track.as_mut() {
            t.loading = loading;
        }
        outcome.map(|_| ())
    }

    /// Request `id` failed: with the server's status when it answered one.
    /// Says what to tell the person, if anything.
    pub fn fetch_failed(&mut self, id: u64, status: Option<u16>) -> Option<Trouble> {
        let kind = self.track.as_ref()?.kind;
        let t = self.answered(id, kind)?;
        let trouble = match (kind, status) {
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
        assert_eq!((first.kind, first.at_ms, first.after_ms), (TrackKind::Text, None, 0));
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
        assert_eq!((retry.at_ms, retry.after_ms), (None, 0));
        p.text_arrived(retry.id, WHOLE, true);
        assert!(p.update(1500, 0));
    }

    #[test]
    fn the_servers_pictures_come_from_where_the_player_is_each_answer_replacing_the_last() {
        let head = header("S_HDMV/PGS", None);
        let around = |time: u32| {
            let mut file = head.clone();
            file.extend(cluster(time, &[Block { time, data: white_set(), duration: None }]));
            PictureAnswer::Whole(file)
        };
        let mut p = Presenter::new();
        p.show_pictures("/api/files/7/subtitles/3.mks");
        let f = p.next_fetch().unwrap();
        assert_eq!((f.kind.accept(), f.kind.timeout_ms()), ("video/x-matroska", 900_000));
        assert_eq!((f.at_ms, f.after_ms), (None, 0), "from where the player is");
        // The header alone: asked for again, counting from the ask.
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Whole(head.clone()), false, 500), Ok(()));
        let f = p.next_fetch().unwrap();
        assert_eq!((f.at_ms, f.after_ms), (None, PARTIAL_RETRY_MS), "never sooner than 3 s after the answer");
        p.pictures_arrived(f.id, around(1000), false, 1_000).unwrap();
        assert!(p.update(1500, 0));
        assert_eq!(p.shown().pictures[0].start_ms, 1000);
        // 6 s from the ask, which waited 1 s; then 12 s, which waited 11 s: 3 s after the answer.
        let f = p.next_fetch().unwrap();
        assert_eq!(f.after_ms, 5_000);
        p.pictures_arrived(f.id, around(60_000), false, 11_000).unwrap();
        assert!(p.update(1600, 0), "the pictures around 1 s are gone with the answer that had them");
        assert!(p.shown().is_empty());
        assert!(p.update(60_500, 0));
        assert_eq!(p.next_fetch().unwrap().after_ms, PARTIAL_RETRY_MS);
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
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Whole(text), true, 0), Err(Trouble::PicturesUnavailable));
        p.show_pictures("w.mks");
        let f = p.next_fetch().unwrap();
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Whole(vec![1, 2, 3]), true, 0), Err(Trouble::Failed));
        assert_eq!(p.next_fetch(), None);
    }

    fn pgs(samples: Vec<PictureSample>) -> PictureAnswer {
        PictureAnswer::Track(PictureTrack { codec_id: "S_HDMV/PGS".into(), codec_private: None, samples })
    }

    #[test]
    fn a_drives_track_is_asked_for_again_after_1_2_4_4_s_and_at_once_after_a_seek() {
        let first = PictureSample::new(1000, None, white_set());
        let mut p = Presenter::new();
        p.set_drive_retry(1_000, 4_000);
        p.show_pictures("/api/files/7/subtitles/3.mks");
        let mut f = p.next_fetch().unwrap();
        assert_eq!(f.after_ms, 0);
        let mut waits = Vec::new();
        for _ in 0..4 {
            // However long the read took: from the answer.
            p.pictures_arrived(f.id, pgs(vec![first.clone()]), false, 900).unwrap();
            f = p.next_fetch().unwrap();
            waits.push(f.after_ms);
        }
        assert_eq!(waits, [1_000, 2_000, 4_000, 4_000]);
        assert_eq!(f.at_ms, None, "a drive's track is read where the film goes");
        // A seek: the request waiting goes at once, and the waits start over.
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false, 0).unwrap();
        p.sought(90_000);
        let f = p.next_fetch().unwrap();
        assert_eq!((f.at_ms, f.after_ms), (None, 0));
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false, 0).unwrap();
        assert_eq!(p.next_fetch().unwrap().after_ms, 1_000);
    }

    #[test]
    fn a_seek_asks_the_server_at_once_from_where_it_went_dropping_the_answer_on_its_way() {
        let first = PictureSample::new(1000, None, white_set());
        let mut p = Presenter::new();
        p.show_pictures("x.mks");
        let on_its_way = p.next_fetch().unwrap();
        p.sought(90_000);
        let f = p.next_fetch().unwrap();
        assert_eq!((f.at_ms, f.after_ms), (Some(90_000), 0));
        p.pictures_arrived(on_its_way.id, pgs(vec![first.clone()]), false, 0).unwrap();
        assert!(!p.update(1500, 0), "the answer for the old place is dropped");
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false, 0).unwrap();
        // The waits start over, from where the player is at each ask.
        let f = p.next_fetch().unwrap();
        assert_eq!((f.at_ms, f.after_ms), (None, PARTIAL_RETRY_MS));
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false, 0).unwrap();
        p.sought(10_000);
        assert_eq!(p.next_fetch().map(|f| (f.at_ms, f.after_ms)), Some((Some(10_000), 0)));
        // A whole track isn't asked for again; nor is the server's text at a seek.
        p.pictures_arrived(p.next_id, pgs(vec![first.clone()]), true, 0).unwrap();
        p.sought(20_000);
        assert_eq!(p.next_fetch(), None);
        p.show_text("a.vtt");
        let f = p.next_fetch().unwrap();
        p.text_arrived(f.id, PART, false);
        p.sought(20_000);
        assert_eq!(p.next_fetch().map(|f| (f.at_ms, f.after_ms)), Some((None, PARTIAL_RETRY_MS)));
    }

    #[test]
    fn the_servers_waits_double_to_15_s_and_a_whole_copy_waits_to_24_s_from_its_answer() {
        let first = PictureSample::new(1000, None, white_set());
        let mut p = Presenter::new();
        p.show_pictures("x.mks");
        let mut waits = Vec::new();
        let mut f = p.next_fetch().unwrap();
        for _ in 0..5 {
            p.pictures_arrived(f.id, pgs(vec![first.clone()]), false, 0).unwrap();
            f = p.next_fetch().unwrap();
            waits.push(f.after_ms);
        }
        assert_eq!(waits, [3_000, 6_000, 12_000, 15_000, 15_000]);

        // Past 4 MB: all of the track so far, asked for again from its answer however long it took.
        let mut big = header("S_HDMV/PGS", None);
        big.extend(cluster(0, &[Block { time: 1000, data: vec![0; COPY_MIN_BYTES], duration: None }]));
        let mut p = Presenter::new();
        p.show_pictures("y.mks");
        let mut waits = Vec::new();
        let mut f = p.next_fetch().unwrap();
        for _ in 0..5 {
            p.pictures_arrived(f.id, PictureAnswer::Whole(big.clone()), false, 2_000).unwrap();
            f = p.next_fetch().unwrap();
            waits.push(f.after_ms);
        }
        assert_eq!(waits, [3_000, 6_000, 12_000, 24_000, 24_000]);
    }

    #[test]
    fn a_handed_over_track_replaces_its_samples() {
        let first = PictureSample::new(1000, None, white_set());
        let mut p = Presenter::new();
        p.show_pictures("/api/files/7/subtitles/3.mks");
        let f = p.next_fetch().unwrap();
        p.pictures_arrived(f.id, pgs(vec![first.clone()]), false, 0).unwrap();
        assert!(p.update(1500, 0));
        let cue = p.shown().pictures[0].clone();
        assert_eq!((cue.start_ms, cue.end_ms), (1000, None));
        let f = p.next_fetch().unwrap();
        p.pictures_arrived(f.id, pgs(vec![first, PictureSample::new(3000, None, empty_set())]), true, 0).unwrap();
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
        assert_eq!(p.pictures_arrived(f.id, PictureAnswer::Track(text), true, 0), Err(Trouble::PicturesUnavailable));
    }
}
