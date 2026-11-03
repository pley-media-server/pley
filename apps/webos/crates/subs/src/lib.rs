//! Subtitles for Pley for LG TV, everything but the drawing: the server's
//! WebVTT ([`vtt`]), moving subtitles in time ([`sync`]), what shows at a
//! position as the playback advances or jumps ([`presenter`]), how text
//! subtitles look and where they go on the screen ([`style`]), and picture
//! subtitles (PGS, VobSub, DVB) read from the server's `.mks` ([`picture`]).
//!
//! Every subtitle is drawn by the app, as the TV app and Android do: LG's
//! media pipeline shows none (docs/TV_PLATFORMS.md). Nothing here fetches:
//! functions take the bytes the player fetched and say what to ask for next.

pub mod picture;
pub mod presenter;
pub mod style;
pub mod sync;
pub mod vtt;

pub use picture::{
    MksError, MksReader, PictureCue, PictureFrame, PictureSample, PictureSubtitles, PictureTrack, PlacedBitmap,
    read_picture_track,
};
pub use presenter::{Fetch, PictureAnswer, Presenter, Shown, TrackKind, Trouble};
pub use style::{
    CueBox, CueGroup, Outline, SubtitleFont, SubtitlePosition, SubtitleSettings, SubtitleSize, TextLayer, cue_align,
    cue_group, fonts_here, picture_rect,
};
pub use sync::{OffsetBook, SYNC_MAX_MS, SYNC_STEP_MS, clamp_offset, format_offset};
pub use vtt::{CueAlign, CueIndex, CueLine, Span, TextCue, parse_vtt};
