//! Picture subtitles: the server's `.mks` of a PGS, VobSub or DVB track
//! (`/api/files/{id}/subtitles/{n}.mks`, docs/API.md), read as it arrives,
//! its samples timed, and each decoded into RGBA bitmaps placed on the plane
//! the stream declares (1920×1080 for a Blu-ray's PGS, 720×576 for a DVD's
//! VobSub), only when it shows. A port of `packages/picture-subtitles`.

pub(crate) mod dvb;
pub(crate) mod mks;
pub(crate) mod pgs;
pub(crate) mod vobsub;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use pley_base::{Rect, RgbaImage, Size};

pub use dvb::DvbDecoder;
pub use mks::{MksError, MksReader, PictureSample, PictureTrack, read_picture_track};
pub use pgs::{PgsDecoder, decode_rle};
pub use vobsub::{Spu, VobSubDecoder, VobSubHeader, parse_idx, parse_spu};

/// The Matroska codec ids of the picture subtitles the app draws, with the
/// server's (ffprobe's) names for them.
pub const PICTURE_CODECS: [(&str, &str); 3] =
    [("S_HDMV/PGS", "hdmv_pgs_subtitle"), ("S_VOBSUB", "dvd_subtitle"), ("S_DVBSUB", "dvb_subtitle")];

/// A bitmap placed on the subtitle plane, in the stream's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedBitmap {
    pub x: u32,
    pub y: u32,
    /// Straight (not premultiplied) RGBA.
    pub image: Arc<RgbaImage>,
    /// Names the bitmap's content for the renderer's texture (`Cmd::Raw`'s key): unique to this decode.
    pub key: u64,
}

/// What a picture sample shows once decoded.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureFrame {
    /// The plane the bitmaps are placed on: the display size the stream declares.
    pub plane_width: u32,
    pub plane_height: u32,
    /// Empty: nothing shows, the screen clears.
    pub bitmaps: Vec<PlacedBitmap>,
}

/// One picture showing: what the player screen draws over the video.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureCue {
    pub start_ms: u64,
    /// When it stops showing; `None` while no later picture is known (the
    /// track's last so far).
    pub end_ms: Option<u64>,
    pub image: Arc<RgbaImage>,
    /// Where the picture goes on [`PictureCue::plane`], in the stream's pixels.
    pub rect: Rect,
    /// The plane the stream declares; the screen maps it onto the video's rectangle.
    pub plane: Size,
    /// The bitmap's [`PlacedBitmap::key`].
    pub key: u64,
}

/// A decoder for one track's samples, which may need earlier samples to
/// decode a later one.
pub trait PictureDecoder: Send {
    /// The frame sample `index` shows; `None` when it can't be decoded.
    fn decode(&mut self, samples: &[PictureSample], index: usize) -> Option<Arc<PictureFrame>>;
    /// How long sample `index` shows when the sample itself says (VobSub's
    /// stop command, DVB's page time-out); `None`: until the next sample.
    fn own_duration_ms(&mut self, samples: &[PictureSample], index: usize) -> Option<u64>;
    /// The samples from `index` on changed (a partial track read further):
    /// forget what was made of them.
    fn forget_from(&mut self, index: usize);
}

/// The decoder for a track's codec; `None` for a codec the app doesn't draw.
pub fn decoder_for(track: &PictureTrack) -> Option<Box<dyn PictureDecoder>> {
    match track.codec_id.as_str() {
        "S_HDMV/PGS" => Some(Box::new(PgsDecoder::new())),
        "S_VOBSUB" => Some(Box::new(VobSubDecoder::new(track.codec_private.as_deref()))),
        "S_DVBSUB" => Some(Box::new(DvbDecoder::new())),
        _ => None,
    }
}

/// Limited-range YCbCr to RGB: BT.709 for HD planes, BT.601 for SD, as
/// FFmpeg's decoders do.
pub fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8, hd: bool) -> [u8; 3] {
    let yy = 1.164383 * (f64::from(y) - 16.0);
    let u = f64::from(cb) - 128.0;
    let v = f64::from(cr) - 128.0;
    let (r, g, b) = if hd {
        (yy + 1.792741 * v, yy - 0.213249 * u - 0.532909 * v, yy + 2.112402 * u)
    } else {
        (yy + 1.596027 * v, yy - 0.391762 * u - 0.812968 * v, yy + 2.017232 * u)
    };
    let c = |x: f64| x.clamp(0.0, 255.0).round() as u8;
    [c(r), c(g), c(b)]
}

/// The most pixels a bitmap may have: a 4K picture's. A stream asking for
/// more is broken, and a TV can't spare the memory.
const MAX_PIXELS: usize = 3840 * 2160;

/// Whether a bitmap of `width`×`height` may be made.
fn fits(width: usize, height: usize) -> bool {
    width.checked_mul(height).is_some_and(|n| n <= MAX_PIXELS)
}

/// A fresh key for a bitmap just decoded.
fn next_key() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A bitmap of `width`×`height` straight RGBA pixels at (`x`, `y`).
fn placed(x: u32, y: u32, width: u32, height: u32, pixels: Vec<u8>) -> PlacedBitmap {
    PlacedBitmap { x, y, image: Arc::new(RgbaImage { width, height, pixels }), key: next_key() }
}

/// The last frames decoded: a few around the playhead are enough, seeking
/// decodes again (the TV app keeps 25).
#[derive(Default)]
struct FrameCache {
    entries: VecDeque<(usize, Option<Arc<PictureFrame>>)>,
}

impl FrameCache {
    const KEPT: usize = 25;

    fn get(&self, index: usize) -> Option<Option<Arc<PictureFrame>>> {
        self.entries.iter().find(|(i, _)| *i == index).map(|(_, f)| f.clone())
    }

    fn put(&mut self, index: usize, frame: Option<Arc<PictureFrame>>) {
        if self.entries.len() >= Self::KEPT {
            self.entries.pop_front();
        }
        self.entries.push_back((index, frame));
    }

    fn forget_from(&mut self, index: usize) {
        self.entries.retain(|(i, _)| *i < index);
    }

    /// The frame for `index`, from the cache or made now.
    fn frame(&mut self, index: usize, make: impl FnOnce() -> Option<PictureFrame>) -> Option<Arc<PictureFrame>> {
        if let Some(frame) = self.get(index) {
            return frame;
        }
        let frame = make().map(Arc::new);
        self.put(index, frame.clone());
        frame
    }
}

/// A picture track ready to show: a sample shows from its time until the
/// next sample, or for its own duration when it carries one (a Matroska
/// block duration, VobSub's stop command, DVB's page time-out); a sample
/// without bitmaps clears. Decoded only when shown. Times are the track's
/// (the playback position less the sync offset), so they may be negative.
pub struct PictureSubtitles {
    track: PictureTrack,
    decoder: Box<dyn PictureDecoder>,
}

impl PictureSubtitles {
    /// The track with its codec's decoder; `None` for a codec the app doesn't draw.
    pub fn of(track: PictureTrack) -> Option<PictureSubtitles> {
        let decoder = decoder_for(&track)?;
        Some(PictureSubtitles { track, decoder })
    }

    /// The track as it stands.
    pub fn track(&self) -> &PictureTrack {
        &self.track
    }

    /// The track read further (a partial answer grown): its samples now.
    /// What was decoded from samples that didn't change is kept.
    pub fn replace_samples(&mut self, samples: Vec<PictureSample>) {
        let old = &self.track.samples;
        let same = old.iter().zip(&samples).take_while(|(a, b)| a.same(b)).count();
        if same < old.len() || same < samples.len() {
            self.decoder.forget_from(same);
        }
        self.track.samples = samples;
    }

    /// The sample showing at `ms`.
    pub fn sample_at(&mut self, ms: i64) -> Option<usize> {
        let i = self.starting_at_or_before(ms)?;
        let end = self.end_of(i);
        end.is_none_or(|e| ms < e).then_some(i)
    }

    /// When sample `i` stops showing; `None`: not before a later sample is known.
    pub fn end_of(&mut self, i: usize) -> Option<i64> {
        let samples = &self.track.samples;
        let start = samples[i].time_ms as i64;
        let next = samples.get(i + 1).map(|s| s.time_ms as i64);
        let own = match samples[i].duration_ms {
            Some(d) => Some(d),
            None => self.decoder.own_duration_ms(samples, i),
        };
        match own.filter(|d| *d > 0) {
            Some(d) => {
                let end = start.saturating_add(d.min(i64::MAX as u64) as i64);
                Some(next.map_or(end, |n| n.min(end)))
            }
            None => next,
        }
    }

    /// What shows at `ms`: the sample and its frame, when it has bitmaps.
    pub fn frame_at(&mut self, ms: i64) -> Option<(usize, Arc<PictureFrame>)> {
        let i = self.sample_at(ms)?;
        let frame = self.decoder.decode(&self.track.samples, i)?;
        (!frame.bitmaps.is_empty()).then_some((i, frame))
    }

    /// The pictures showing at `ms`, as the screen draws them.
    pub fn cues_at(&mut self, ms: i64) -> Option<(usize, Vec<PictureCue>)> {
        let (i, frame) = self.frame_at(ms)?;
        let start_ms = self.track.samples[i].time_ms;
        let end_ms = self.end_of(i).map(|e| e.max(0) as u64);
        let plane = Size::new(frame.plane_width as f32, frame.plane_height as f32);
        let cues = frame
            .bitmaps
            .iter()
            .map(|b| PictureCue {
                start_ms,
                end_ms,
                image: b.image.clone(),
                rect: Rect::new(b.x as f32, b.y as f32, b.image.width as f32, b.image.height as f32),
                plane,
                key: b.key,
            })
            .collect();
        Some((i, cues))
    }

    /// When what shows changes after `ms`; `None` when nothing does any more
    /// (as far as the track is known).
    pub fn next_change(&mut self, ms: i64) -> Option<i64> {
        if let Some(i) = self.sample_at(ms) {
            return self.end_of(i);
        }
        let next = self.track.samples.partition_point(|s| s.time_ms as i64 <= ms);
        self.track.samples.get(next).map(|s| s.time_ms as i64)
    }

    /// The last sample starting at or before `ms`.
    fn starting_at_or_before(&self, ms: i64) -> Option<usize> {
        self.track.samples.partition_point(|s| s.time_ms as i64 <= ms).checked_sub(1)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 16-bit big-endian value, as the streams write them.
    pub fn u16b(v: u32) -> [u8; 2] {
        [(v >> 8) as u8, v as u8]
    }

    pub fn pgs_segment(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend(u16b(body.len() as u32));
        out.extend(body);
        out
    }

    /// A PGS display set placing a `w`×`h` object at (`x`, `y`) on a
    /// 1920×1080 plane, with palette entry 1 white and 2 half-transparent black.
    pub fn pgs_display_set(state: u8, objects: bool, x: u32, y: u32, rle: Option<&[u8]>, w: u32, h: u32) -> Vec<u8> {
        let mut pcs = Vec::new();
        pcs.extend(u16b(1920));
        pcs.extend(u16b(1080));
        pcs.push(0x10);
        pcs.extend(u16b(1));
        pcs.extend([state, 0x00, 0x00, u8::from(objects)]);
        if objects {
            pcs.extend(u16b(0));
            pcs.extend([0, 0x00]);
            pcs.extend(u16b(x));
            pcs.extend(u16b(y));
        }
        let mut out = pgs_segment(0x16, &pcs);
        if let Some(rle) = rle {
            let mut wds = vec![1, 0];
            for v in [x, y, w, h] {
                wds.extend(u16b(v));
            }
            out.extend(pgs_segment(0x17, &wds));
            out.extend(pgs_segment(0x14, &[0, 0, 1, 235, 128, 128, 255, 2, 16, 128, 128, 128]));
            let mut data = Vec::new();
            data.extend(u16b(w));
            data.extend(u16b(h));
            data.extend(rle);
            let n = data.len();
            let mut ods = vec![0, 0, 0, 0xc0, (n >> 16) as u8, (n >> 8) as u8, n as u8];
            ods.extend(data);
            out.extend(pgs_segment(0x15, &ods));
        }
        out.extend(pgs_segment(0x80, &[]));
        out
    }

    /// Display sets every test of timing uses: two lines of white.
    pub fn white_set() -> Vec<u8> {
        pgs_display_set(0x80, true, 100, 900, Some(&[1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0]), 4, 2)
    }

    pub fn empty_set() -> Vec<u8> {
        pgs_display_set(0, false, 0, 0, None, 0, 0)
    }

    fn track(samples: Vec<PictureSample>) -> PictureTrack {
        PictureTrack { codec_id: "S_HDMV/PGS".into(), codec_private: None, samples }
    }

    #[test]
    fn times_samples_until_the_next_or_their_own_end() {
        let mut subs = PictureSubtitles::of(track(vec![
            PictureSample::new(1000, None, white_set()),
            PictureSample::new(3000, None, empty_set()),
            PictureSample::new(5000, Some(1500), white_set()),
        ]))
        .unwrap();
        assert!(subs.frame_at(500).is_none());
        assert_eq!(subs.frame_at(1500).map(|f| f.0), Some(0));
        assert!(subs.frame_at(3200).is_none());
        assert_eq!(subs.frame_at(6000).map(|f| f.0), Some(2));
        assert!(subs.frame_at(6600).is_none());
        assert_eq!(subs.next_change(-2000), Some(1000));
        assert_eq!(subs.next_change(1500), Some(3000));
        assert_eq!(subs.next_change(4000), Some(5000));
        assert_eq!(subs.next_change(5100), Some(6500));
        assert_eq!(subs.next_change(7000), None);
    }

    #[test]
    fn places_cues_on_the_plane_and_keeps_decodes_as_the_track_grows() {
        let mut subs = PictureSubtitles::of(track(vec![PictureSample::new(1000, None, white_set())])).unwrap();
        let (i, cues) = subs.cues_at(1200).unwrap();
        assert_eq!(i, 0);
        assert_eq!(cues.len(), 1);
        let cue = &cues[0];
        assert_eq!((cue.start_ms, cue.end_ms), (1000, None));
        assert_eq!(cue.rect, Rect::new(100.0, 900.0, 4.0, 2.0));
        assert_eq!(cue.plane, Size::new(1920.0, 1080.0));
        // More of the track arrives: the first sample now ends, and its decode is kept.
        let mut grown = subs.track().samples.clone();
        grown.push(PictureSample::new(4000, None, empty_set()));
        subs.replace_samples(grown);
        let (_, again) = subs.cues_at(1200).unwrap();
        assert_eq!(again[0].key, cue.key);
        assert_eq!(again[0].end_ms, Some(4000));
        assert!(subs.cues_at(4100).is_none());
    }

    #[test]
    fn refuses_codecs_it_doesnt_draw() {
        let t = PictureTrack { codec_id: "S_TEXT/ASS".into(), codec_private: None, samples: Vec::new() };
        assert!(PictureSubtitles::of(t).is_none());
        assert!(PICTURE_CODECS.iter().all(|(id, _)| decoder_for(&track_of(id)).is_some()));
    }

    fn track_of(id: &str) -> PictureTrack {
        PictureTrack { codec_id: id.into(), codec_private: None, samples: Vec::new() }
    }

    #[test]
    fn converts_limited_range_colours() {
        assert_eq!(ycbcr_to_rgb(235, 128, 128, true), [255, 255, 255]);
        assert_eq!(ycbcr_to_rgb(16, 128, 128, false), [0, 0, 0]);
        // Red in BT.601 and BT.709.
        assert_eq!(ycbcr_to_rgb(81, 90, 240, false), [254, 0, 0]);
        assert_eq!(ycbcr_to_rgb(63, 102, 240, true), [255, 1, 0]);
    }
}
