//! Lyrics of music tracks: from a file beside the track named after it,
//! `.lrc` (synced: LRC's `[mm:ss.xx]` times) or `.txt` (plain), which scans
//! read and keep in the database ([`changes`]); else from the track's own
//! tags as Pley's probe reads them (ID3's `USLT`, FLAC's `LYRICS` and
//! `UNSYNCEDLYRICS`, MP4's `©lyr`; Ogg files' comments are their stream's,
//! which the probe leaves out); else, for an MP3, from its ID3
//! `SYLT` frame (synced lyrics, which the probe doesn't read), read from the
//! file when asked for. Lyrics whose lines carry LRC times are synced,
//! wherever they come from.
//!
//! What is read is bounded: files and tags of [`MAX_TEXT`] bytes at most,
//! [`MAX_LINES`] lines of [`MAX_LINE_CHARS`] characters.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sqlx::SqliteConnection;

use crate::catalog::FileRow;
use crate::db::Db;
use crate::library::sidecar::Sidecars;
use crate::util::path_str;

/// The most of a lyrics file, or of lyrics in a tag, that is read.
pub const MAX_TEXT: usize = 256 * 1024;
/// The most lines kept.
pub const MAX_LINES: usize = 5_000;
/// The longest line kept, in characters (the rest is cut).
pub const MAX_LINE_CHARS: usize = 1_000;
/// The largest ID3 tag looked through for a `SYLT` frame (its pictures
/// skipped, not read).
const MAX_ID3_TAG: u64 = 64 * 1024 * 1024;
/// The most ID3 frames looked at.
const MAX_ID3_FRAMES: usize = 4_096;

/// Where lyrics come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// A file beside the track.
    File,
    /// The track's tags.
    Tags,
}

/// A line of lyrics, and when it is sung (ms of the track), when synced.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<i64>,
    pub text: String,
}

/// A track's lyrics.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lyrics {
    pub synced: bool,
    pub source: Source,
    pub lines: Vec<Line>,
    /// What an LRC file says of itself: `[ar:]`, `[ti:]`, `[al:]`, `[au:]`,
    /// `[by:]`, `[re:]`, `[ve:]`.
    #[serde(skip)]
    pub tags: BTreeMap<&'static str, String>,
    /// `[length:]`, in ms.
    #[serde(skip)]
    pub length_ms: Option<i64>,
}

/// A line of at most [`MAX_LINE_CHARS`] characters, without the line
/// break's ends and the controls of other systems.
fn clean(line: &str) -> String {
    line.trim_end_matches(['\r', '\n']).chars().filter(|c| !c.is_control() || *c == '\t').take(MAX_LINE_CHARS).collect()
}

/// `text` cut to [`MAX_TEXT`] bytes, at a character's start.
fn bounded(text: &str) -> &str {
    if text.len() <= MAX_TEXT {
        return text;
    }
    let mut end = MAX_TEXT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// An LRC time, `mm:ss`, `mm:ss.x`, `mm:ss.xx`, `mm:ss.xxx` or `h:mm:ss.xx`
/// (also with `:` before the fraction), in ms.
fn lrc_time(text: &str) -> Option<i64> {
    let text = text.trim();
    let (clock, fraction) = match text.rfind(['.', ',']) {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => match text.matches(':').count() {
            // `mm:ss:xx`, as some files write it.
            2 if text.len() <= 8 => text.rsplit_once(':').unwrap_or((text, "")),
            _ => (text, ""),
        },
    };
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 || parts.iter().any(|p| p.is_empty() || p.len() > 3) {
        return None;
    }
    let numbers: Vec<i64> = parts.iter().map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let (hours, minutes, seconds) = match numbers[..] {
        [m, s] => (0, m, s),
        [h, m, s] => (h, m, s),
        _ => return None,
    };
    if seconds >= 60 || (parts.len() == 3 && minutes >= 60) {
        return None;
    }
    let ms = match fraction {
        "" => 0,
        f if f.len() <= 3 && f.bytes().all(|b| b.is_ascii_digit()) => {
            f.parse::<i64>().ok()? * 10_i64.pow(3 - f.len() as u32)
        }
        _ => return None,
    };
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000 + ms)
}

/// The tags an LRC file says of itself in brackets (`[ar:Someone]`); other
/// words in brackets are its text.
const LRC_TAGS: &[&str] = &["ar", "ti", "al", "au", "by", "re", "ve", "length", "offset", "la", "id", "tool", "#"];

/// Enhanced LRC's word times (`<00:12.30>`) out of a line's text.
fn without_word_times(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let Some(close) = rest[open..].find('>').map(|c| open + c) else { break };
        out.push_str(&rest[..open]);
        if lrc_time(&rest[open + 1..close]).is_none() {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out.trim().to_owned()
}

/// Lyrics from `text`: LRC when lines carry its times (several to a line
/// for a chorus sung again; `[offset:]` applied), else plain lines. `None`
/// for text with no words.
pub fn parse(text: &str, source: Source) -> Option<Lyrics> {
    let text = bounded(text).trim_start_matches('\u{feff}');
    let mut tags = BTreeMap::new();
    let (mut length_ms, mut offset) = (None, 0_i64);
    let mut timed: Vec<(i64, usize, String)> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        let mut tagged = false;
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(close) = inner.find(']') else { break };
            let (label, after) = (&inner[..close], &inner[close + 1..]);
            if let Some(ms) = lrc_time(label) {
                times.push(ms);
            } else if let Some((key, value)) = label.split_once(':')
                && LRC_TAGS.contains(&key.trim().to_ascii_lowercase().as_str())
            {
                let value = value.trim();
                let key = match key.trim().to_ascii_lowercase().as_str() {
                    "ar" => Some("artist"),
                    "ti" => Some("title"),
                    "al" => Some("album"),
                    "au" => Some("author"),
                    "by" => Some("by"),
                    "re" => Some("creator"),
                    "ve" => Some("version"),
                    "length" => {
                        length_ms = lrc_time(value);
                        None
                    }
                    "offset" => {
                        offset = value.trim_start_matches('+').parse::<i64>().unwrap_or(0).clamp(-600_000, 600_000);
                        None
                    }
                    _ => None,
                };
                if let Some(key) = key.filter(|_| !value.is_empty()) {
                    tags.insert(key, clean(value));
                }
                tagged = true;
            } else {
                // Words in brackets (`[Chorus: …]`): the line's text.
                break;
            }
            rest = after.trim_start();
        }
        if !times.is_empty() {
            let line = clean(&without_word_times(rest));
            for ms in times {
                timed.push((ms, n, line.clone()));
            }
        } else if !tagged {
            plain.push(clean(raw.trim_end()));
        }
    }
    let synced = !timed.is_empty();
    let mut lines: Vec<Line> = if synced {
        // A positive offset shows the words sooner.
        timed.sort_by_key(|(ms, n, _)| (*ms, *n));
        timed.into_iter().map(|(ms, _, text)| Line { start_ms: Some((ms - offset).max(0)), text }).collect()
    } else {
        let first = plain.iter().position(|l| !l.trim().is_empty())?;
        let last = plain.iter().rposition(|l| !l.trim().is_empty())?;
        plain.drain(first..=last).map(|text| Line { start_ms: None, text }).collect()
    };
    if !lines.iter().any(|l| !l.text.trim().is_empty()) {
        return None;
    }
    lines.truncate(MAX_LINES);
    Some(Lyrics { synced, source, lines, tags, length_ms })
}

/// Text read from a file: UTF-8 (its byte order mark left out), UTF-16
/// with one, else Latin-1, as older LRC files are.
fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xff, 0xfe, rest @ ..] => utf16(rest, false),
        [0xfe, 0xff, rest @ ..] => utf16(rest, true),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => bytes.iter().map(|b| char::from(*b)).collect(),
        },
    }
}

fn utf16(bytes: &[u8], big_endian: bool) -> String {
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| if big_endian { u16::from_be_bytes(*p) } else { u16::from_le_bytes(*p) });
    char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

/// A lyrics file beside a track, read when it is a file (not a link to
/// another kind) of [`MAX_TEXT`] bytes at most.
pub fn read_file(path: &Path) -> Option<String> {
    let (file, meta) = crate::library::walk::open_of_its_kind(path)?;
    if meta.len() > MAX_TEXT as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_TEXT as u64).read_to_end(&mut bytes).ok()?;
    Some(decode(&bytes))
}

/// Tags that hold lyrics, as the probe names them in lowercase: `lyrics`
/// (MP4's `©lyr`, ID3's text frame), `lyrics-…` (ID3's `USLT`, with its
/// description and language), Vorbis comments' `LYRICS`, `UNSYNCEDLYRICS`
/// and `SYNCEDLYRICS`.
fn lyrics_tag(key: &str) -> bool {
    matches!(key, "lyrics" | "unsyncedlyrics" | "syncedlyrics" | "unsynced lyrics") || key.starts_with("lyrics-")
}

/// The lyrics of a track's tags: synced ones first, those of any tag
/// otherwise, in the tags' order.
pub fn from_tags(tags: &BTreeMap<String, String>) -> Option<Lyrics> {
    let found: Vec<Lyrics> =
        tags.iter().filter(|(k, _)| lyrics_tag(k)).filter_map(|(_, v)| parse(v, Source::Tags)).collect();
    let synced = found.iter().position(|l| l.synced);
    found.into_iter().nth(synced.unwrap_or(0))
}

/// Whether the track's tags hold lyrics, as [`from_tags`] reads them.
pub fn in_tags(file: &FileRow) -> bool {
    file.media_info().is_some_and(|info| from_tags(&info.tags).is_some())
}

// ------------------------------------------------------------------ ID3 SYLT

/// The synced lyrics of an MP3's ID3v2 tag (its first `SYLT` frame whose
/// times are in ms), read from `file`: its frames are looked through by
/// their sizes, their pictures skipped. Tags of ID3 2.3 and 2.4.
pub fn sylt<R: Read + Seek>(file: &mut R) -> Option<Lyrics> {
    let mut header = [0u8; 10];
    file.read_exact(&mut header).ok()?;
    let (major, flags) = (header[3], header[5]);
    if &header[..3] != b"ID3" || !(3..=4).contains(&major) || header[6..].iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    // A whole tag unsynchronised (2.3) has its frames' sizes changed too.
    if flags & 0x80 != 0 && major == 3 {
        return None;
    }
    let size = u64::from(syncsafe(&header[6..10]));
    let end = 10 + size.min(MAX_ID3_TAG);
    let mut at = 10u64;
    if flags & 0x40 != 0 {
        // An extended header: its size (2.4's counts itself, 2.3's doesn't).
        let mut bytes = [0u8; 4];
        file.read_exact(&mut bytes).ok()?;
        at += if major == 4 { u64::from(syncsafe(&bytes)) } else { 4 + u64::from(u32::from_be_bytes(bytes)) };
    }
    for _ in 0..MAX_ID3_FRAMES {
        if at + 10 > end {
            return None;
        }
        file.seek(SeekFrom::Start(at)).ok()?;
        let mut frame = [0u8; 10];
        file.read_exact(&mut frame).ok()?;
        if frame[0] == 0 {
            // Padding.
            return None;
        }
        let length = if major == 4 { syncsafe(&frame[4..8]) } else { u32::from_be_bytes(frame[4..8].try_into().ok()?) };
        let length = u64::from(length);
        at += 10 + length;
        if &frame[..4] != b"SYLT" || length > MAX_TEXT as u64 || at > end {
            continue;
        }
        let format = frame[9];
        // Compressed, encrypted or grouped frames aren't read.
        let unreadable = if major == 4 { format & 0x4c != 0 } else { format & 0xe0 != 0 };
        if unreadable {
            continue;
        }
        let mut body = vec![0u8; length as usize];
        file.read_exact(&mut body).ok()?;
        if major == 4 && format & 0x01 != 0 {
            // A data length indicator.
            body.drain(..body.len().min(4));
        }
        if (major == 4 && format & 0x02 != 0) || flags & 0x80 != 0 {
            body = resynchronised(&body);
        }
        if let Some(lyrics) = sylt_body(&body) {
            return Some(lyrics);
        }
    }
    None
}

fn syncsafe(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0, |n, b| (n << 7) | u32::from(b & 0x7f))
}

/// A frame's bytes without the zeros unsynchronisation put after `0xff`.
fn resynchronised(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut after_ff = false;
    for &b in bytes {
        if !(after_ff && b == 0) {
            out.push(b);
        }
        after_ff = b == 0xff;
    }
    out
}

/// A terminated string of the frame's `encoding` at the start of `bytes`,
/// and what follows it.
fn id3_text(bytes: &[u8], encoding: u8) -> Option<(String, &[u8])> {
    match encoding {
        0 | 3 => {
            let end = bytes.iter().position(|b| *b == 0)?;
            let text = if encoding == 3 {
                String::from_utf8_lossy(&bytes[..end]).into_owned()
            } else {
                bytes[..end].iter().map(|b| char::from(*b)).collect()
            };
            Some((text, &bytes[end + 1..]))
        }
        1 | 2 => {
            let end = bytes.as_chunks::<2>().0.iter().position(|p| *p == [0, 0])? * 2;
            let text = match &bytes[..end] {
                [0xff, 0xfe, rest @ ..] => utf16(rest, false),
                [0xfe, 0xff, rest @ ..] => utf16(rest, true),
                text => utf16(text, encoding == 2),
            };
            Some((text, &bytes[end + 2..]))
        }
        _ => None,
    }
}

/// A `SYLT` frame's lyrics: lines, or syllables (each new line starting
/// with a line break) joined into lines. Times in MPEG frames are left out.
fn sylt_body(body: &[u8]) -> Option<Lyrics> {
    let (&encoding, rest) = body.split_first()?;
    let rest = rest.get(3..)?;
    let (&format, rest) = rest.split_first()?;
    if format != 2 {
        return None;
    }
    let (_, rest) = rest.split_first()?;
    let (_, mut rest) = id3_text(rest, encoding)?;
    let mut items = Vec::new();
    while !rest.is_empty() && items.len() < MAX_LINES * 8 {
        let Some((text, after)) = id3_text(rest, encoding) else { break };
        let Some(time) = after.get(..4) else { break };
        items.push((i64::from(u32::from_be_bytes(time.try_into().ok()?)), text));
        rest = &after[4..];
    }
    let syllables = items.iter().any(|(_, t)| t.starts_with(['\n', '\r']));
    let mut lines: Vec<Line> = Vec::new();
    for (ms, text) in items {
        match lines.last_mut() {
            Some(line) if syllables && !text.starts_with(['\n', '\r']) => {
                line.text = clean(&format!("{}{text}", line.text));
            }
            _ => lines.push(Line { start_ms: Some(ms), text: clean(text.trim_start_matches(['\n', '\r'])) }),
        }
    }
    lines.sort_by_key(|l| l.start_ms);
    lines.truncate(MAX_LINES);
    lines.iter().any(|l| !l.text.trim().is_empty()).then(|| Lyrics {
        synced: true,
        source: Source::Tags,
        lines,
        tags: BTreeMap::new(),
        length_ms: None,
    })
}

/// An MP3's `SYLT` lyrics, read from the file the scan checked.
fn sylt_of(path: PathBuf) -> Option<Lyrics> {
    let (file, _) = crate::library::walk::open_of_its_kind(&path)?;
    sylt(&mut std::io::BufReader::new(file))
}

// ------------------------------------------------------------------ the database

/// A track file's lyrics, wherever they are: the file beside it, its tags,
/// its `SYLT` frame.
pub async fn of_file(db: &Db, file: &FileRow) -> anyhow::Result<Option<Lyrics>> {
    let stored: Option<String> = sqlx::query_scalar("SELECT text FROM track_lyrics WHERE file_id = ?")
        .bind(file.id)
        .fetch_optional(&db.read)
        .await?;
    if let Some(lyrics) = stored.and_then(|text| parse(&text, Source::File)) {
        return Ok(Some(lyrics));
    }
    if let Some(lyrics) = file.media_info().and_then(|info| from_tags(&info.tags)) {
        return Ok(Some(lyrics));
    }
    let path = PathBuf::from(&file.path);
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp3")) {
        return Ok(None);
    }
    crate::util::blocking(move || Ok(sylt_of(path))).await
}

/// Which of `files` have lyrics beside them or in their tags (not those
/// only an MP3's `SYLT` frame has, which are read when asked for).
pub async fn files_with(db: &Db, files: &[&FileRow]) -> sqlx::Result<HashSet<i64>> {
    let mut found: HashSet<i64> = files.iter().filter(|f| in_tags(f)).map(|f| f.id).collect();
    let ids: Vec<i64> = files.iter().map(|f| f.id).filter(|id| !found.contains(id)).collect();
    for chunk in ids.chunks(500) {
        let sql = format!("SELECT file_id FROM track_lyrics WHERE file_id IN ({})", vec!["?"; chunk.len()].join(","));
        let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()));
        for id in chunk {
            query = query.bind(id);
        }
        found.extend(query.fetch_all(&db.read).await?);
    }
    Ok(found)
}

// ------------------------------------------------------------------ scans

/// A music file of a library, with the lyrics file it had at the last scan.
#[derive(Debug, sqlx::FromRow)]
pub struct Row {
    pub file_id: i64,
    pub path: String,
    pub lyrics_path: Option<String>,
    pub lyrics_size: Option<i64>,
    pub lyrics_mtime: Option<i64>,
}

/// A library's music files, with the lyrics files they had.
pub const ROWS: &str = "SELECT f.id AS file_id, f.path, l.path AS lyrics_path, l.size AS lyrics_size,
        l.mtime AS lyrics_mtime
     FROM media_files f LEFT JOIN track_lyrics l ON l.file_id = f.id WHERE f.library_id = ?";

/// A track whose lyrics file appeared, changed or went.
#[derive(Debug, PartialEq)]
pub struct Change {
    pub file_id: i64,
    /// The file and its text now: path, size, modification time, text.
    pub after: Option<(String, i64, i64, String)>,
}

impl Change {
    pub async fn save(&self, conn: &mut SqliteConnection) -> sqlx::Result<()> {
        match &self.after {
            Some((path, size, mtime, text)) => {
                // Unless the file went meanwhile.
                sqlx::query(
                    "INSERT INTO track_lyrics (file_id, path, size, mtime, text)
                     SELECT ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM media_files WHERE id = ?)
                     ON CONFLICT(file_id) DO UPDATE SET path = excluded.path, size = excluded.size,
                        mtime = excluded.mtime, text = excluded.text",
                )
                .bind(self.file_id)
                .bind(path)
                .bind(size)
                .bind(mtime)
                .bind(text)
                .bind(self.file_id)
                .execute(&mut *conn)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM track_lyrics WHERE file_id = ?")
                    .bind(self.file_id)
                    .execute(&mut *conn)
                    .await?;
            }
        }
        Ok(())
    }
}

/// The lyrics files that appeared, changed or went for `rows` (a music
/// library's files), found with `sidecars`, the walk's index; `listed` has
/// the sizes and times the walk saw. Only those are read. Those below
/// `unavailable` (a folder that couldn't be read) are left as they were.
pub fn changes(
    rows: Vec<Row>,
    sidecars: &Sidecars,
    listed: &HashMap<PathBuf, (i64, i64)>,
    unavailable: &[PathBuf],
) -> Vec<Change> {
    let mut out = Vec::new();
    for row in rows {
        let track = Path::new(&row.path);
        let found = sidecars.lyrics(track).and_then(|path| {
            let (size, mtime) = match listed.get(&path) {
                Some(seen) => *seen,
                None => {
                    let meta = std::fs::metadata(&path).ok()?;
                    (meta.len() as i64, meta.modified().map(crate::util::system_time_secs).unwrap_or(0))
                }
            };
            Some((path_str(&path), size, mtime))
        });
        let before =
            row.lyrics_path.as_ref().map(|p| (p.clone(), row.lyrics_size.unwrap_or(0), row.lyrics_mtime.unwrap_or(0)));
        match (&found, &before) {
            (Some(now), Some(seen)) if now == seen => continue,
            (None, None) => continue,
            (None, Some(_)) if unavailable.iter().any(|p| track.starts_with(p)) => continue,
            _ => {}
        }
        // A file that can't be read (too big, a link elsewhere) is as good as none.
        let after = found.and_then(|(path, size, mtime)| {
            let text = read_file(Path::new(&path))?;
            Some((path, size, mtime, text))
        });
        if after.is_none() && before.is_none() {
            continue;
        }
        out.push(Change { file_id: row.file_id, after });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn times(lyrics: &Lyrics) -> Vec<(Option<i64>, &str)> {
        lyrics.lines.iter().map(|l| (l.start_ms, l.text.as_str())).collect()
    }

    #[test]
    fn lrc_lines_are_timed_sorted_and_offset() {
        let lrc = "\u{feff}[ar:Someone]\n[ti: A Song ]\n[length: 03:30]\n[offset:+500]\n[re:A tool]\n\
                   [00:12.00][00:45.50]Again <00:12.50>and <00:13.00>again\n[00:05.1]First\r\n\
                   [00:20]\n[1:00:00.000]An hour in\nno time: left out\n[xx:yy]Not a time\n";
        let lyrics = parse(lrc, Source::File).unwrap();
        assert!(lyrics.synced);
        assert_eq!(
            times(&lyrics),
            [
                (Some(4_600), "First"),
                (Some(11_500), "Again and again"),
                (Some(19_500), ""),
                (Some(45_000), "Again and again"),
                (Some(3_599_500), "An hour in"),
            ]
        );
        assert_eq!(lyrics.tags["artist"], "Someone");
        assert_eq!((lyrics.tags["title"].as_str(), lyrics.length_ms), ("A Song", Some(210_000)));
        assert_eq!(lyrics.tags["creator"], "A tool");
        assert_eq!(lrc_time("01:02:03"), Some(62_030), "mm:ss:xx");
        assert_eq!((lrc_time("00:61.00"), lrc_time("ab:cd"), lrc_time("00:01.5")), (None, None, Some(1_500)));
    }

    #[test]
    fn plain_text_is_its_lines() {
        let lyrics = parse("\n\nFirst line\n\nSecond line  \n\n", Source::Tags).unwrap();
        assert!(!lyrics.synced);
        assert_eq!(times(&lyrics), [(None, "First line"), (None, ""), (None, "Second line")]);
        let sections = parse("[Verse 1: Someone]\nWords\n[Chorus]\nMore", Source::File).unwrap();
        assert_eq!(sections.lines.len(), 4, "words in brackets are lines");
        assert_eq!(parse(" \n\t\n", Source::File), None);
        assert_eq!(parse("[ar:Someone]\n[ti:Nothing sung]\n", Source::File), None, "tags alone");
        assert_eq!(parse("[00:01.00]\n[00:02.00]  ", Source::File), None, "times alone");
    }

    #[test]
    fn what_is_read_is_bounded() {
        let many: String = (0..MAX_LINES + 100).map(|n| format!("[00:{:02}.00]line {n}\n", n % 60)).collect();
        assert_eq!(parse(&many, Source::File).unwrap().lines.len(), MAX_LINES);
        let long = "é".repeat(MAX_LINE_CHARS * 2);
        assert_eq!(parse(&long, Source::File).unwrap().lines[0].text.chars().count(), MAX_LINE_CHARS);
        let huge = "word ".repeat(MAX_TEXT);
        assert!(bounded(&huge).len() <= MAX_TEXT);
        assert_eq!(bounded(&"é".repeat(MAX_TEXT)).len(), MAX_TEXT, "cut at a character");
        assert_eq!(clean("a\u{7}b\tc\r\n"), "ab\tc");
    }

    #[test]
    fn files_are_read_in_their_encoding() {
        assert_eq!(decode(b"\xef\xbb\xbfcaf\xc3\xa9"), "café");
        assert_eq!(decode(&[0xff, 0xfe, b'h', 0, b'i', 0]), "hi");
        assert_eq!(decode(&[0xfe, 0xff, 0, b'h', 0, b'i']), "hi");
        assert_eq!(decode(b"caf\xe9"), "café", "Latin-1");
    }

    #[test]
    fn tags_give_their_lyrics_synced_first() {
        let tags = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
        };
        let plain = from_tags(&tags(&[("title", "Song"), ("lyrics-desc-eng", "Words\nMore words")])).unwrap();
        assert_eq!((plain.synced, plain.source, plain.lines.len()), (false, Source::Tags, 2));
        let both = tags(&[("unsyncedlyrics", "Plain"), ("lyrics", "[00:01.00]Timed")]);
        assert_eq!(times(&from_tags(&both).unwrap()), [(Some(1_000), "Timed")]);
        assert!(from_tags(&tags(&[("title", "Song"), ("comment", "[00:01.00]Not lyrics")])).is_none());
        assert!(from_tags(&tags(&[("lyrics", "  ")])).is_none());
    }

    fn syncsafe_bytes(n: usize) -> [u8; 4] {
        let n = n as u32;
        [(n >> 21) as u8 & 0x7f, (n >> 14) as u8 & 0x7f, (n >> 7) as u8 & 0x7f, n as u8 & 0x7f]
    }

    /// An ID3 tag of `major` version with `frames` (id, flags, body).
    fn id3(major: u8, frames: &[(&[u8; 4], u8, Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, flags, bytes) in frames {
            body.extend_from_slice(*id);
            let size = if major == 4 { syncsafe_bytes(bytes.len()) } else { (bytes.len() as u32).to_be_bytes() };
            body.extend_from_slice(&size);
            body.extend_from_slice(&[0, *flags]);
            body.extend_from_slice(bytes);
        }
        body.extend_from_slice(&[0; 32]);
        let mut tag = b"ID3".to_vec();
        tag.extend_from_slice(&[major, 0, 0]);
        tag.extend_from_slice(&syncsafe_bytes(body.len()));
        tag.extend(body);
        tag.extend_from_slice(b"\xff\xfb\x90\x00 the audio");
        tag
    }

    /// A `SYLT` body in UTF-8, its times in ms.
    fn sylt_utf8(items: &[(&str, u32)]) -> Vec<u8> {
        let mut body = vec![3];
        body.extend_from_slice(b"eng");
        body.extend_from_slice(&[2, 1]);
        body.extend_from_slice(b"desc\0");
        for (text, ms) in items {
            body.extend_from_slice(text.as_bytes());
            body.push(0);
            body.extend_from_slice(&ms.to_be_bytes());
        }
        body
    }

    fn read(tag: Vec<u8>) -> Option<Lyrics> {
        sylt(&mut std::io::Cursor::new(tag))
    }

    #[test]
    fn mp3s_synced_lyrics_are_read_from_their_sylt_frame() {
        let picture = vec![0u8; 300_000];
        let lines = sylt_utf8(&[("Second", 2_500), ("First", 1_000)]);
        let tag = id3(4, &[(b"TIT2", 0, b"\x03Title".to_vec()), (b"APIC", 0, picture), (b"SYLT", 0, lines.clone())]);
        let lyrics = read(tag).unwrap();
        assert_eq!((lyrics.synced, lyrics.source), (true, Source::Tags));
        assert_eq!(times(&lyrics), [(Some(1_000), "First"), (Some(2_500), "Second")]);

        // ID3 2.3, in UTF-16 with byte order marks; syllables joined into lines.
        let mut body = vec![1];
        body.extend_from_slice(b"eng\x02\x01");
        body.extend_from_slice(&[0xff, 0xfe, 0, 0]);
        for (text, ms) in [("\nHel", 1_000u32), ("lo", 1_400), ("\nAgain", 3_000)] {
            body.extend_from_slice(&[0xff, 0xfe]);
            body.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
            body.extend_from_slice(&[0, 0]);
            body.extend_from_slice(&ms.to_be_bytes());
        }
        let lyrics = read(id3(3, &[(b"SYLT", 0, body)])).unwrap();
        assert_eq!(times(&lyrics), [(Some(1_000), "Hello"), (Some(3_000), "Again")]);

        // A 2.4 frame unsynchronised: the zero put after its 0xff taken out
        // again (Latin-1's ÿ, then the text's end).
        let mut unsynced = b"\x00eng\x02\x01\x00\xff\x00\x00".to_vec();
        unsynced.extend_from_slice(&1_000u32.to_be_bytes());
        assert_eq!(times(&read(id3(4, &[(b"SYLT", 0x02, unsynced)])).unwrap()), [(Some(1_000), "ÿ")]);

        // Times in MPEG frames, compressed frames, no tag at all: none.
        let mut frames = lines.clone();
        frames[4] = 1;
        assert!(read(id3(4, &[(b"SYLT", 0, frames)])).is_none());
        assert!(read(id3(4, &[(b"SYLT", 0x08, lines)])).is_none());
        assert!(read(b"\xff\xfb\x90\x00 audio alone".to_vec()).is_none());
        assert!(read(b"ID3\x04\x00\x00\x7f".to_vec()).is_none(), "cut short");
    }

    #[test]
    fn scans_read_lyrics_files_that_appeared_or_changed() {
        let dir = tempfile::tempdir().unwrap();
        let album = dir.path().join("Album");
        std::fs::create_dir_all(&album).unwrap();
        std::fs::write(album.join("01 Song.lrc"), "[00:01.00]Hello").unwrap();
        let track = path_str(&album.join("01 Song.flac"));
        let other = path_str(&album.join("02 Other.flac"));
        let index = Sidecars::new(HashMap::from([(album.clone(), vec!["01 Song.lrc".to_owned()])]));
        let row = |file_id, path: &str, stored: Option<(&str, i64, i64)>| Row {
            file_id,
            path: path.to_owned(),
            lyrics_path: stored.map(|s| s.0.to_owned()),
            lyrics_size: stored.map(|s| s.1),
            lyrics_mtime: stored.map(|s| s.2),
        };
        let lrc = path_str(&album.join("01 Song.lrc"));
        let listed = HashMap::from([(album.join("01 Song.lrc"), (15, 100))]);

        let found = changes(vec![row(1, &track, None), row(2, &other, None)], &index, &listed, &[]);
        assert_eq!(found, [Change { file_id: 1, after: Some((lrc.clone(), 15, 100, "[00:01.00]Hello".into())) }]);
        // Unchanged: not read again. Changed: read again.
        assert!(changes(vec![row(1, &track, Some((&lrc, 15, 100)))], &index, &listed, &[]).is_empty());
        let changed = changes(vec![row(1, &track, Some((&lrc, 15, 90)))], &index, &listed, &[]);
        assert_eq!(changed.len(), 1);
        // Gone, unless its folder couldn't be read this time.
        let gone = Sidecars::new(HashMap::new());
        let stored = row(1, &track, Some((&lrc, 15, 100)));
        assert_eq!(changes(vec![stored], &gone, &listed, &[]), [Change { file_id: 1, after: None }]);
        let stored = row(1, &track, Some((&lrc, 15, 100)));
        assert!(changes(vec![stored], &gone, &listed, &[dir.path().to_path_buf()]).is_empty());
    }
}
