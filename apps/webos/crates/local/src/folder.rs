//! What a drive's folders show, and the subtitle files beside a video.
//!
//! Names stay as the drive has them (`OsString`, `PathBuf`) for opening; they
//! are shown and sorted as text, with bytes that aren't UTF-8 replaced.

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use pley_core::model::{MediaStream, StreamKind};

/// The containers the TV's player opens (webos/src/caps.rs's, less sound
/// alone). LG's player lists no WebM.
const VIDEOS: [&str; 10] = ["mkv", "mp4", "m4v", "mov", "ts", "m2ts", "mts", "avi", "mpg", "mpeg"];

/// What Windows and macOS leave on a drive, never shown.
const SYSTEM: [&str; 6] =
    ["System Volume Information", "$RECYCLE.BIN", "RECYCLER", "lost+found", "Thumbs.db", "desktop.ini"];

/// The most entries a folder lists, and the most names looked through for
/// subtitle files: a folder of films has a few hundred.
pub const ENTRIES_MOST: usize = 10_000;
/// The most of a subtitle file read: a film's SubRip is well under 1 MB.
pub const SIDECAR_MOST: u64 = 10 * 1024 * 1024;
/// Subtitle files beside the media are numbered from here, as the server
/// numbers them.
pub const SIDECAR_FIRST_INDEX: i64 = 1000;

/// A folder or a video in a folder of a drive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    /// Its name as shown.
    pub name: String,
    pub folder: bool,
    /// Bytes; 0 for a folder.
    pub size: u64,
}

/// A subtitle file beside a video: `Film.en.forced.srt` for `Film.mkv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sidecar {
    pub path: PathBuf,
    /// `subrip` or `webvtt`.
    pub codec: &'static str,
    pub language: Option<String>,
    pub title: Option<String>,
    pub forced: bool,
}

impl Sidecar {
    /// The subtitle stream it is, numbered `index`.
    pub fn stream(&self, index: i64) -> MediaStream {
        MediaStream {
            index,
            kind: StreamKind::Subtitle,
            codec: self.codec.into(),
            language: self.language.clone(),
            title: self.title.clone(),
            forced: self.forced,
            external: true,
            text_based: Some(true),
            ..MediaStream::default()
        }
    }
}

/// The subtitle streams of `sidecars`, numbered from 1000 as the server
/// numbers files beside the media.
pub fn sidecar_streams(sidecars: &[Sidecar]) -> Vec<MediaStream> {
    (SIDECAR_FIRST_INDEX..).zip(sidecars).map(|(index, s)| s.stream(index)).collect()
}

/// A name's extension, after its last dot; none for a hidden file's only dot.
fn extension(name: &str) -> Option<&str> {
    Path::new(name).extension().and_then(OsStr::to_str)
}

/// Whether `name` is a video the TV's player opens.
pub fn is_video(name: &str) -> bool {
    extension(name).is_some_and(|ext| VIDEOS.iter().any(|v| v.eq_ignore_ascii_case(ext)))
}

/// Hidden ones (a dot, a dollar) and the systems' own aren't shown.
pub fn shown(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.starts_with('$')
        && !SYSTEM.iter().any(|s| s.eq_ignore_ascii_case(name))
}

/// `dir`'s folders, then its videos, each by name as a person sorts them
/// ("Part 2" before "Part 10"); other files aren't shown, nor a link that
/// leads off the drive at `root`. An error when the folder can't be read.
pub fn list(dir: &Path, root: &Path) -> io::Result<Vec<Entry>> {
    let root = fs::canonicalize(root)?;
    let mut folders = Vec::new();
    let mut videos = Vec::new();
    for entry in fs::read_dir(dir)? {
        if folders.len() + videos.len() >= ENTRIES_MOST {
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        if !shown(&name) {
            continue;
        }
        let path = entry.path();
        // Through a link, as a person browsing the drive sees it, while it stays on the drive.
        let link = entry.file_type().is_ok_and(|t| t.is_symlink());
        if link && !fs::canonicalize(&path).is_ok_and(|p| p.starts_with(&root)) {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else { continue };
        if meta.is_dir() {
            folders.push(Entry { path, name, folder: true, size: 0 });
        } else if meta.is_file() && is_video(&name) {
            videos.push(Entry { path, name, folder: false, size: meta.len() });
        }
    }
    folders.sort_by(|a, b| natural(&a.name, &b.name));
    videos.sort_by(|a, b| natural(&a.name, &b.name));
    folders.append(&mut videos);
    Ok(folders)
}

/// Whether `path` is on the drive at `root` once links and `..` are followed:
/// a link on a drive may lead anywhere on the TV the app can read. False when
/// either can't be found.
pub fn within(root: &Path, path: &Path) -> bool {
    match (fs::canonicalize(root), fs::canonicalize(path)) {
        (Ok(root), Ok(path)) => path.starts_with(root),
        _ => false,
    }
}

/// Compares names by their runs of digits as numbers (leading zeros
/// ignored), the rest ignoring case.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let (si, sj) = (i, j);
            while a.get(i).is_some_and(char::is_ascii_digit) {
                i += 1;
            }
            while b.get(j).is_some_and(char::is_ascii_digit) {
                j += 1;
            }
            let number = |run: &[char]| -> Vec<char> { run.iter().copied().skip_while(|c| *c == '0').collect() };
            let (n, m) = (number(&a[si..i]), number(&b[sj..j]));
            let order = n.len().cmp(&m.len()).then_with(|| n.cmp(&m));
            if order != Ordering::Equal {
                return order;
            }
            continue;
        }
        let order = x.to_uppercase().cmp(y.to_uppercase());
        if order != Ordering::Equal {
            return order;
        }
        i += 1;
        j += 1;
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// The title a video shows: its file name without the extension.
pub fn title(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The subtitle files beside `video` among `names` (its folder's files):
/// those named as it is, then a language (`en`, `eng`, `English`),
/// `forced`, `sdh`, in any order. SubRip and WebVTT, which the app draws;
/// sorted as a person sorts them.
pub fn sidecars<N: AsRef<OsStr>>(video: &Path, names: impl IntoIterator<Item = N>) -> Vec<Sidecar> {
    let dir = video.parent().unwrap_or(Path::new(""));
    let stem = title(video);
    let mut found = Vec::new();
    for name in names.into_iter().take(ENTRIES_MOST) {
        let name = name.as_ref();
        let text = name.to_string_lossy();
        let Some(ext) = extension(&text) else { continue };
        let codec = match ext.to_ascii_lowercase().as_str() {
            "srt" => "subrip",
            "vtt" => "webvtt",
            _ => continue,
        };
        let Some(after_stem) = prefix_ignoring_case(&text, &stem) else { continue };
        let Some(middle) = text.get(after_stem..text.len() - ext.len() - 1) else { continue };
        // "Film 2.srt" is Film 2's, not Film's.
        if !middle.is_empty() && !middle.starts_with(['.', '_']) {
            continue;
        }
        let (mut language, mut forced, mut titles) = (None, false, Vec::new());
        for part in middle.split(['.', '_', ' ']).filter(|p| !p.is_empty()) {
            let p = part.to_lowercase();
            if p == "forced" {
                forced = true;
            } else if matches!(p.as_str(), "sdh" | "cc" | "hi") {
                titles.push("SDH".to_string());
            } else if language.is_none()
                && let Some(code) = language_of(&p)
            {
                language = Some(code);
            } else {
                titles.push(part.to_string());
            }
        }
        found.push(Sidecar {
            path: dir.join(name),
            codec,
            language,
            title: (!titles.is_empty()).then(|| titles.join(" ")),
            forced,
        });
    }
    found.sort_by(|a, b| natural(&a.path.to_string_lossy(), &b.path.to_string_lossy()));
    found
}

/// The subtitle files beside `video`, from its folder; none when the folder
/// can't be read.
pub fn sidecars_beside(video: &Path) -> Vec<Sidecar> {
    let dir = video.parent().unwrap_or(Path::new("."));
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let names: Vec<OsString> = entries.filter_map(|e| e.ok()).map(|e| e.file_name()).take(ENTRIES_MOST).collect();
    sidecars(video, names)
}

/// Where `name` goes on after `prefix`, when it starts with it, ignoring case.
fn prefix_ignoring_case(name: &str, prefix: &str) -> Option<usize> {
    let mut rest = name.char_indices();
    for p in prefix.chars() {
        let (_, c) = rest.next()?;
        if !c.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
    }
    Some(rest.next().map_or(name.len(), |(at, _)| at))
}

const LANGUAGES: [(&str, &str); 15] = [
    ("english", "eng"),
    ("french", "fre"),
    ("spanish", "spa"),
    ("german", "ger"),
    ("italian", "ita"),
    ("portuguese", "por"),
    ("japanese", "jpn"),
    ("korean", "kor"),
    ("chinese", "chi"),
    ("arabic", "ara"),
    ("russian", "rus"),
    ("dutch", "dut"),
    ("swedish", "swe"),
    ("hindi", "hin"),
    ("turkish", "tur"),
];

/// A language code (`en`, `eng`, `pt-br`) or English name, as the code the
/// player's labels read; none for anything else.
fn language_of(p: &str) -> Option<String> {
    if let Some((_, code)) = LANGUAGES.iter().find(|(name, _)| *name == p) {
        return Some((*code).to_string());
    }
    let letters =
        |s: &str, n: std::ops::RangeInclusive<usize>| n.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase());
    let ok = match p.split_once('-') {
        Some((code, region)) => letters(code, 2..=3) && letters(region, 2..=2),
        None => letters(p, 2..=3),
    };
    ok.then(|| p.to_string())
}

/// A subtitle file beside a video, as WebVTT: read up to 10 MB (a larger
/// one is refused), decoded and converted.
pub fn read_sidecar(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(SIDECAR_MOST + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > SIDECAR_MOST {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "subtitle file too large"));
    }
    Ok(to_webvtt(&decode(&bytes)))
}

/// A SubRip file as WebVTT, as the server would serve it: the header, and
/// its times with a full stop. A WebVTT file is kept as it is.
pub fn to_webvtt(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if text.trim_start().starts_with("WEBVTT") {
        return text;
    }
    // "00:00:01,500" becomes "00:00:01.500": a comma after h:mm:ss and
    // before a digit.
    let mut bytes = text.into_bytes();
    let digit = |b: &[u8], i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    for i in 7..bytes.len() {
        let time = digit(&bytes, i - 7)
            && bytes.get(i - 6) == Some(&b':')
            && digit(&bytes, i - 5)
            && digit(&bytes, i - 4)
            && bytes.get(i - 3) == Some(&b':')
            && digit(&bytes, i - 2)
            && digit(&bytes, i - 1);
        if time
            && digit(&bytes, i + 1)
            && let Some(comma @ b',') = bytes.get_mut(i)
        {
            *comma = b'.';
        }
    }
    // Only an ASCII comma changed to an ASCII full stop: still UTF-8.
    "WEBVTT\n\n".to_string() + &String::from_utf8_lossy(&bytes)
}

/// A subtitle file's text. A byte-order mark says its encoding: UTF-16 or UTF-32, as Windows
/// tools save "Unicode" text (UTF-8's goes in [`to_webvtt`]). Without one, UTF-16 shows by its NUL
/// bytes, which text in UTF-8 or a code page never has; then UTF-8 when it is, else Windows-1252,
/// the Western code page older SubRip files are in.
pub fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xFF, 0xFE, 0, 0, rest @ ..] => utf32(rest, u32::from_le_bytes),
        [0, 0, 0xFE, 0xFF, rest @ ..] => utf32(rest, u32::from_be_bytes),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => match (utf16_order(bytes), std::str::from_utf8(bytes)) {
            (Some(word), _) => utf16(bytes, word),
            (None, Ok(text)) => text.to_string(),
            (None, Err(_)) => bytes.iter().map(|&b| windows_1252(b)).collect(),
        },
    }
}

/// The byte order of UTF-16 without its mark, from its first 4 KB: where it is Latin text, times
/// or numbers, every other byte is NUL, the high one.
fn utf16_order(bytes: &[u8]) -> Option<fn([u8; 2]) -> u16> {
    let (pairs, _) = bytes[..bytes.len().min(4096)].as_chunks::<2>();
    let (first, second) =
        pairs.iter().fold((0, 0), |(f, s), [a, b]| (f + usize::from(*a == 0), s + usize::from(*b == 0)));
    let many = |n: usize| pairs.len() >= 8 && n * 4 >= pairs.len();
    if many(second) && first * 8 <= second {
        Some(u16::from_le_bytes)
    } else if many(first) && second * 8 <= first {
        Some(u16::from_be_bytes)
    } else {
        None
    }
}

/// Windows-1252's byte `b`: Latin-1 but for 0x80–0x9F, where it has the euro, curly quotes,
/// dashes and the like (the five it leaves out stay as Latin-1 has them).
fn windows_1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2C6}',
        '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}',
        '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}',
        '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// UTF-32 text in `word`'s byte order; what isn't a character reads as U+FFFD, and a last word
/// cut short is dropped.
fn utf32(bytes: &[u8], word: fn([u8; 4]) -> u32) -> String {
    bytes.as_chunks::<4>().0.iter().map(|c| char::from_u32(word(*c)).unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

/// UTF-16 text in `word`'s byte order; a lone surrogate reads as U+FFFD and
/// an odd last byte is dropped.
fn utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|c| word(*c));
    char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}
