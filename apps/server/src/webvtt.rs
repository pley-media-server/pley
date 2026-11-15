//! WebVTT cues moved in time, for `GET /api/files/{id}/subtitles/{index}.vtt?offset_ms=`:
//! players that draw subtitles themselves move them as they draw (Android,
//! the TVs' web app), but Roku's Video node draws a sideloaded track as it
//! comes, so its subtitle sync asks the server for the track moved.

/// The most a track is moved, either way: ten minutes.
pub const MAX_OFFSET_MS: i64 = 600_000;

/// `text` with every cue `offset_ms` later (earlier when negative). A time
/// that would come before the start is the start. Everything but the cues'
/// timing lines stays as it was, their settings too.
pub fn shift(text: &str, offset_ms: i64) -> String {
    if offset_ms == 0 {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    for line in text.split_inclusive('\n') {
        match moved_timing(line, offset_ms) {
            Some(moved) => out.push_str(&moved),
            None => out.push_str(line),
        }
    }
    out
}

/// `text` with the line HLS players read to match its cues with a stream's
/// timestamps: the stream's time `mpegts` (90 kHz) is the track's start.
/// It goes in the header, after the first line.
pub fn with_timestamp_map(text: &str, mpegts: u64) -> String {
    let map = format!("X-TIMESTAMP-MAP=MPEGTS:{mpegts},LOCAL:00:00:00.000\n");
    match text.find('\n') {
        Some(end) => format!("{}{map}{}", &text[..=end], &text[end + 1..]),
        None => format!("{text}\n{map}"),
    }
}

/// The cues of `text` shown at some time from `from_ms` to before `to_ms`,
/// after its header (the first line and any blocks before the first cue):
/// a segment of a stream's subtitles. A cue on both sides of a boundary is
/// in both segments, as HLS players expect.
pub fn between(text: &str, from_ms: i64, to_ms: i64) -> String {
    let mut header = Vec::new();
    let mut cues = Vec::new();
    for block in blocks(text) {
        match timing(block) {
            Some((start, end)) => {
                if start < to_ms && end > from_ms {
                    cues.push(block);
                }
            }
            None if cues.is_empty() && !header.is_empty() && block.starts_with("NOTE") => {}
            None if cues.is_empty() => header.push(block),
            None => {}
        }
    }
    let mut out = header.into_iter().chain(cues).collect::<Vec<_>>().join("\n\n");
    out.push('\n');
    out
}

/// When the last cue of `text` starts: how far a track being read out of a
/// file has got.
pub fn last_start_ms(text: &str) -> Option<i64> {
    blocks(text).filter_map(timing).map(|(start, _)| start).max()
}

/// The blocks of a WebVTT text, between blank lines.
fn blocks(text: &str) -> impl Iterator<Item = &str> {
    text.split("\n\n").flat_map(|b| b.split("\r\n\r\n")).map(|b| b.trim_matches(['\r', '\n'])).filter(|b| !b.is_empty())
}

/// A cue block's start and end.
fn timing(block: &str) -> Option<(i64, i64)> {
    let line = block.lines().find(|l| l.contains("-->"))?;
    let (start, rest) = line.split_once("-->")?;
    let end = rest.split_whitespace().next()?;
    Some((time_ms(start.trim())?, time_ms(end)?))
}

/// A cue's timing line ("00:01.000 --> 00:04.000 line:90%") moved, or
/// `None` for any other line. WebVTT allows `-->` nowhere else.
fn moved_timing(line: &str, offset_ms: i64) -> Option<String> {
    let (start, rest) = line.split_once("-->")?;
    let start = time_ms(start.trim())?;
    let rest = rest.trim_start();
    let end_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let (end, after) = rest.split_at(end_len);
    let end = time_ms(end)?;
    let moved = |ms: i64| clock((ms + offset_ms).max(0));
    Some(format!("{} --> {}{}", moved(start), moved(end), after))
}

/// "01:02:03.456" or "02:03.456" in milliseconds.
fn time_ms(text: &str) -> Option<i64> {
    let (clock, millis) = text.split_once('.')?;
    let number = |part: &str| -> Option<i64> {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    if millis.len() != 3 {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (0, number(minutes)?, number(seconds)?),
        [hours, minutes, seconds] => (number(hours)?, number(minutes)?, number(seconds)?),
        _ => return None,
    };
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + number(millis)?)
}

/// Milliseconds as "01:02:03.456".
fn clock(ms: i64) -> String {
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACK: &str = "WEBVTT\n\nNOTE made by pley\n\n1\n00:01.000 --> 00:04.000 line:90% align:center\nHello\n\n01:00:02.500 --> 01:00:03.000\r\n<i>there</i>\r\n";

    #[test]
    fn a_segment_has_the_header_and_the_cues_shown_in_its_time() {
        let track = "WEBVTT\n\nSTYLE\n::cue { color: yellow }\n\n1\n00:10.000 --> 00:12.000\nOne\n\n\
                     00:29.000 --> 00:31.000\nAcross\n\nNOTE between\n\n00:45.000 --> 00:46.000\nThree\n";
        assert_eq!(
            between(track, 0, 30_000),
            "WEBVTT\n\nSTYLE\n::cue { color: yellow }\n\n1\n00:10.000 --> 00:12.000\nOne\n\n00:29.000 --> 00:31.000\nAcross\n"
        );
        assert_eq!(
            between(track, 30_000, 60_000),
            "WEBVTT\n\nSTYLE\n::cue { color: yellow }\n\n00:29.000 --> 00:31.000\nAcross\n\n00:45.000 --> 00:46.000\nThree\n"
        );
        assert_eq!(between(track, 60_000, 90_000), "WEBVTT\n\nSTYLE\n::cue { color: yellow }\n");
        assert_eq!(last_start_ms(track), Some(45_000));
        assert_eq!(last_start_ms("WEBVTT\n"), None);
        assert_eq!(
            between("WEBVTT\r\n\r\n00:01.000 --> 00:02.000\r\nHi\r\n", 0, 30_000),
            "WEBVTT\n\n00:01.000 --> 00:02.000\r\nHi\n"
        );
    }

    #[test]
    fn the_timestamp_map_goes_in_the_header() {
        let mapped = with_timestamp_map(TRACK, 126_000);
        assert!(mapped.starts_with("WEBVTT\nX-TIMESTAMP-MAP=MPEGTS:126000,LOCAL:00:00:00.000\n\nNOTE made by pley\n"));
        assert_eq!(with_timestamp_map("WEBVTT", 0), "WEBVTT\nX-TIMESTAMP-MAP=MPEGTS:0,LOCAL:00:00:00.000\n");
    }

    #[test]
    fn moves_every_cue_and_nothing_else() {
        assert_eq!(
            shift(TRACK, 300),
            "WEBVTT\n\nNOTE made by pley\n\n1\n00:00:01.300 --> 00:00:04.300 line:90% align:center\nHello\n\n01:00:02.800 --> 01:00:03.300\r\n<i>there</i>\r\n"
        );
        assert_eq!(shift(TRACK, 0), TRACK);
    }

    #[test]
    fn earlier_stops_at_the_start() {
        let moved = shift("WEBVTT\n\n00:00.500 --> 00:02.000\nHi\n", -1_000);
        assert_eq!(moved, "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHi\n");
    }

    #[test]
    fn leaves_what_isnt_a_timing_line() {
        // Not times: left as they are.
        assert_eq!(shift("a --> b\n", 300), "a --> b\n");
        assert_eq!(shift("00:01 --> 00:02.000\n", 300), "00:01 --> 00:02.000\n");
        assert_eq!(time_ms("1:02:03.456"), Some(3_723_456));
        assert_eq!(clock(3_723_456), "01:02:03.456");
    }
}
