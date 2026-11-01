// SPDX-License-Identifier: AGPL-3.0-only
//! Text subtitles as WebVTT, cue for cue as FFmpeg writes them with
//! `-c:s webvtt -f webvtt`: subtitles read straight out of a Matroska file
//! ([`crate::matroska::text_tracks`]) come out exactly as an extraction by
//! FFmpeg would have made them, quirks included, so it never matters which
//! way a file's subtitles were extracted.
//!
//! FFmpeg decodes each subtitle to an ASS event (the SubRip decoder turns
//! HTML-like markup into ASS override codes, the WebVTT decoder its tags),
//! and its WebVTT encoder writes the event's text back out, keeping only
//! bold, italics and underline (from the codes or the event's style) and line
//! breaks. Each step below follows FFmpeg 9's (`htmlsubtitles.c`,
//! `ass_split.c`, `webvttenc.c` and the decoders), as do the cue times of its
//! WebVTT muxer.

use std::collections::HashMap;

/// The most styles an ASS header may define here: real scripts define tens,
/// a few hundred at most. FFmpeg converts a track whose header has more.
const MAX_STYLES: usize = 10_000;
/// The most fields a `Format:` line is read for.
const MAX_FORMAT_FIELDS: usize = 64;

/// A text subtitle codec, by FFmpeg's name for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCodec {
    /// SubRip: plain lines with HTML-like markup (`<i>`, `<font>`).
    Subrip,
    /// (Advanced) SubStation Alpha events, styled by the script's header.
    Ass,
    /// Plain text.
    Text,
    /// WebVTT cue text.
    Webvtt,
}

/// One subtitle of a track, times in milliseconds on the output timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub start_ms: i64,
    pub duration_ms: i64,
    pub data: Vec<u8>,
}

/// A whole track as a WebVTT file. `header` is the track's codec private
/// data (the ASS script header that defines the styles), `events` are in the
/// order of the file. `go_on` is asked before each event and once done, with
/// the memory the WebVTT takes so far: an error from it stops the conversion.
pub fn to_webvtt(
    codec: TextCodec,
    header: &[u8],
    events: &[Event],
    mut go_on: impl FnMut(usize) -> Result<(), String>,
) -> Result<String, String> {
    let styles = match codec {
        TextCodec::Ass => Styles::parse(header)?,
        // The other decoders give FFmpeg's default header, whose only style
        // is neither bold, italic nor underlined.
        _ => Styles::default(),
    };
    let mut encoder = Encoder { styles, stack: Vec::new() };
    let mut out = String::from("WEBVTT\n");
    for event in events {
        go_on(out.capacity())?;
        let Some(dialog) = decode(codec, &event.data) else { continue };
        // FFmpeg drops what isn't UTF-8 once decoded.
        if !valid_utf8(&dialog) {
            continue;
        }
        let text = encoder.encode(&dialog);
        out.push('\n');
        push_time(&mut out, event.start_ms);
        out.push_str(" --> ");
        push_time(&mut out, event.start_ms.saturating_add(event.duration_ms));
        out.push('\n');
        out.push_str(&String::from_utf8_lossy(&text));
        out.push('\n');
    }
    go_on(out.capacity())?;
    Ok(out)
}

/// `MM:SS.mmm`, with hours in front only when there are some, as FFmpeg's
/// muxer writes cue times.
fn push_time(out: &mut String, ms: i64) {
    let (hours, minutes, seconds, millis) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    if hours > 0 {
        out.push_str(&format!("{hours:02}:"));
    }
    out.push_str(&format!("{minutes:02}:{seconds:02}.{millis:03}"));
}

/// The ASS event an FFmpeg decoder makes of a subtitle
/// (`ReadOrder,Layer,Style,Name,MarginL,MarginR,MarginV,Effect,Text`), or
/// `None` when it makes none.
fn decode(codec: TextCodec, data: &[u8]) -> Option<Vec<u8>> {
    // Packets are read as C strings: anything after a NUL is lost.
    let data = data.iter().position(|&b| b == 0).map_or(data, |end| &data[..end]);
    match codec {
        TextCodec::Ass => Some(data.to_vec()),
        TextCodec::Subrip => Some(dialog(&html_to_ass(data))),
        TextCodec::Text => (!data.is_empty()).then(|| dialog(&plain_to_ass(data))),
        TextCodec::Webvtt => (!data.is_empty()).then(|| dialog(&webvtt_to_ass(data))),
    }
}

/// An event in the default style, as FFmpeg's decoders add them.
fn dialog(text: &[u8]) -> Vec<u8> {
    [b"0,0,Default,,0,0,0,,".as_slice(), text].concat()
}

/// UTF-8 as FFmpeg checks decoded subtitles: a byte-order mark the wrong way
/// round (U+FFFE) doesn't pass either.
fn valid_utf8(text: &[u8]) -> bool {
    std::str::from_utf8(text).is_ok_and(|s| !s.contains('\u{FFFE}'))
}

// ------------------------------------------------------------------ decoders

fn rstrip_spaces(buf: &mut Vec<u8>) {
    while buf.last() == Some(&b' ') {
        buf.pop();
    }
}

#[derive(Clone, Default)]
struct FontTag {
    face: Vec<u8>,
}

/// SubRip's HTML-like markup as ASS (FFmpeg's `ff_htmlmarkup_to_ass`):
/// `<b>`, `<i>`, `<u>`, `<s>` and `<br>` become codes, other tags go,
/// lines lose their outer spaces, and a blank line ends the subtitle. ASS
/// codes in braces are dropped, except from the first `{\anN}` on.
/// Font colours and sizes are left out: the WebVTT encoder drops them all
/// the same. Font faces are kept, for the rare face that has braces in it.
fn html_to_ass(input: &[u8]) -> Vec<u8> {
    let at = |i: usize| input.get(i).copied().unwrap_or(0);
    let mut dst = Vec::with_capacity(input.len() + 16);
    let mut fonts = vec![FontTag::default()];
    let mut line_start = true;
    let mut an = 0;
    let mut closing_brace_missing = false;
    let mut i = 0;
    while i < input.len() {
        match input[i] {
            b'\r' => {}
            b'\n' => {
                if line_start {
                    break;
                }
                rstrip_spaces(&mut dst);
                dst.extend_from_slice(b"\\N");
                line_start = true;
            }
            b' ' => {
                if !line_start {
                    dst.push(b' ');
                }
            }
            b'{' => {
                if input[i..].starts_with(b"{\\an") && at(i + 4).is_ascii_digit() && at(i + 5) == b'}' {
                    an += 1;
                }
                let next = at(i + 1);
                let skip = !closing_brace_missing
                    && ((an != 1 && next == b'\\')
                        || (next != 0 && b"CcFfoPSsYy".contains(&next) && at(i + 2) == b':'));
                match skip.then(|| input[i + 2..].iter().position(|&b| b == b'}')) {
                    Some(Some(end)) => i += 2 + end,
                    Some(None) => {
                        closing_brace_missing = true;
                        dst.push(b'{');
                    }
                    None => dst.push(b'{'),
                }
            }
            b'<' => {
                let mut likely_a_tag = true;
                while at(i + 1) == b'<' {
                    dst.push(b'<');
                    likely_a_tag = false;
                    i += 1;
                }
                let tag_close = at(i + 1) == b'/';
                if tag_close {
                    likely_a_tag = true;
                }
                match scan_tag(input, i + 1 + tag_close as usize) {
                    Some(tag) => {
                        let skip = tag.len() + 1 + tag_close as usize;
                        let mut name = tag;
                        while name.first() == Some(&b' ') {
                            likely_a_tag = false;
                            name = &name[1..];
                        }
                        let (name, params) = match name.iter().position(|&b| b == b' ') {
                            Some(space) => (&name[..space], Some(&name[space + 1..])),
                            None => (name, None),
                        };
                        if !name.iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'/') {
                            likely_a_tag = false;
                        }
                        if name.eq_ignore_ascii_case(b"font") {
                            if tag_close && fonts.len() > 1 {
                                let closed = fonts.pop().unwrap_or_default();
                                let outer = fonts.last().cloned().unwrap_or_default();
                                if !closed.face.is_empty() {
                                    if outer.face.is_empty() {
                                        dst.extend_from_slice(b"{\\fn}");
                                    } else if outer.face != closed.face {
                                        dst.extend_from_slice(b"{\\fn");
                                        dst.extend_from_slice(&outer.face);
                                        dst.push(b'}');
                                    }
                                }
                            } else if !tag_close && fonts.len() < 16 {
                                let mut font = fonts.last().cloned().unwrap_or_default();
                                if let Some(params) = params {
                                    font_params(params, &mut font, &mut dst);
                                }
                                fonts.push(font);
                            }
                            i += skip;
                        } else if name.len() == 1 && b"bisu".contains(&name[0].to_ascii_lowercase()) {
                            dst.extend_from_slice(&[
                                b'{',
                                b'\\',
                                name[0].to_ascii_lowercase(),
                                if tag_close { b'0' } else { b'1' },
                            ]);
                            dst.push(b'}');
                            i += skip;
                        } else if name.len() >= 2
                            && name[..2].eq_ignore_ascii_case(b"br")
                            && (name.len() == 2 || &name[2..] == b"/")
                        {
                            dst.extend_from_slice(b"\\N");
                            i += skip;
                        } else if likely_a_tag {
                            i += skip;
                        } else {
                            dst.push(b'<');
                        }
                    }
                    None => dst.push(b'<'),
                }
            }
            c => dst.push(c),
        }
        if !matches!(at(i), b' ' | b'\r' | b'\n') {
            line_start = false;
        }
        i += 1;
    }
    while dst.ends_with(b"\\N") {
        dst.truncate(dst.len() - 2);
    }
    rstrip_spaces(&mut dst);
    dst
}

/// The text of a tag starting at `start`, up to its `>` (at most 127 bytes).
fn scan_tag(input: &[u8], start: usize) -> Option<&[u8]> {
    for len in 0..128 {
        match input.get(start + len).copied().unwrap_or(0) {
            0 | b'<' => return None,
            b'>' => return Some(&input[start..start + len]),
            _ => {}
        }
    }
    None
}

/// The parameters of a `<font>` tag: only its face is written out.
fn font_params(mut params: &[u8], font: &mut FontTag, dst: &mut Vec<u8>) {
    loop {
        if params.len() >= 5 && params[..5].eq_ignore_ascii_case(b"face=") {
            let quoted = params.get(5) == Some(&b'"');
            params = &params[5 + quoted as usize..];
            let len = params.iter().position(|&b| b == if quoted { b'"' } else { b' ' }).unwrap_or(params.len());
            font.face = params[..len.min(127)].to_vec();
            params = &params[len..];
            dst.extend_from_slice(b"{\\fn");
            dst.extend_from_slice(&font.face);
            dst.push(b'}');
        }
        match params.iter().position(|&b| b == b' ') {
            Some(space) => params = &params[space + 1..],
            None => break,
        }
    }
}

/// Plain text as ASS (FFmpeg's `ff_ass_bprint_text_event`): braces and
/// backslashes can't start codes, and line breaks become `\N` (not one at
/// the very end).
fn plain_to_ass(input: &[u8]) -> Vec<u8> {
    let mut dst = Vec::with_capacity(input.len() + 16);
    for (i, &c) in input.iter().enumerate() {
        match c {
            b'{' => dst.extend_from_slice(b"\\{{}"),
            b'\\' => {
                if input.len() - i <= 3 || !input[i + 1..].starts_with("\u{2060}".as_bytes()) {
                    dst.extend_from_slice("\\\u{2060}".as_bytes());
                } else {
                    dst.push(b'\\');
                }
            }
            b'\n' => {
                if i + 1 < input.len() {
                    dst.extend_from_slice(b"\\N");
                }
            }
            b'\r' if input.get(i + 1) == Some(&b'\n') => {}
            c => dst.push(c),
        }
    }
    dst
}

/// WebVTT cue text as ASS (FFmpeg's WebVTT decoder): `<i>`, `<b>` and `<u>`
/// (and tags starting like them) become codes, other tags go, a `<` without
/// its `>` ends the text, and a few entities are decoded.
fn webvtt_to_ass(input: &[u8]) -> Vec<u8> {
    const TAGS: [(&[u8], &[u8]); 6] = [
        (b"i", b"{\\i1}"),
        (b"/i", b"{\\i0}"),
        (b"b", b"{\\b1}"),
        (b"/b", b"{\\b0}"),
        (b"u", b"{\\u1}"),
        (b"/u", b"{\\u0}"),
    ];
    const REPLACE: [(&[u8], &str); 8] = [
        (b"{", "\\{{}"),
        (b"\\", "\\\u{2060}"),
        (b"&gt;", ">"),
        (b"&lt;", "<"),
        (b"&lrm;", "\u{200E}"),
        (b"&rlm;", "\u{200F}"),
        (b"&amp;", "&"),
        (b"&nbsp;", "\\h"),
    ];
    let mut dst = Vec::with_capacity(input.len() + 16);
    let mut p = 0;
    while p < input.len() {
        let mut again = false;
        if input[p] == b'<' {
            let Some(len) = input[p..].iter().position(|&b| b == b'>') else { break };
            let body = &input[p + 1..];
            if let Some((_, code)) = TAGS.iter().find(|(tag, _)| body.starts_with(tag)) {
                dst.extend_from_slice(code);
            }
            p += len + 1;
            again = true;
        }
        if let Some((from, to)) = REPLACE.iter().find(|(from, _)| input[p..].starts_with(from)) {
            dst.extend_from_slice(to.as_bytes());
            p += from.len();
            again = true;
        }
        if again || p >= input.len() {
            continue;
        }
        if input[p] == b'\n' && p + 1 < input.len() {
            dst.extend_from_slice(b"\\N");
        } else if input[p] != b'\r' {
            dst.push(input[p]);
        }
        p += 1;
    }
    dst
}

// ------------------------------------------------------------------ styles

/// The styles of an ASS script header, as FFmpeg reads them (only whether
/// each is bold, italic or underlined matters here).
#[derive(Debug, Default)]
struct Styles {
    styles: Vec<Style>,
    /// The first style of each name, which is the one FFmpeg uses.
    by_name: HashMap<Vec<u8>, usize>,
}

#[derive(Debug, Default, Clone)]
struct Style {
    name: Vec<u8>,
    bold: i64,
    italic: i64,
    underline: i64,
}

/// Where a style field goes.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Name,
    Bold,
    Italic,
    Underline,
    Other,
}

const V4_PLUS_FIELDS: [&str; 23] = [
    "Name",
    "Fontname",
    "Fontsize",
    "PrimaryColour",
    "SecondaryColour",
    "OutlineColour",
    "BackColour",
    "Bold",
    "Italic",
    "Underline",
    "StrikeOut",
    "ScaleX",
    "ScaleY",
    "Spacing",
    "Angle",
    "BorderStyle",
    "Outline",
    "Shadow",
    "Alignment",
    "MarginL",
    "MarginR",
    "MarginV",
    "Encoding",
];

const V4_FIELDS: [&str; 18] = [
    "Name",
    "Fontname",
    "Fontsize",
    "PrimaryColour",
    "SecondaryColour",
    "TertiaryColour",
    "BackColour",
    "Bold",
    "Italic",
    "BorderStyle",
    "Outline",
    "Shadow",
    "Alignment",
    "MarginL",
    "MarginR",
    "MarginV",
    "AlphaLevel",
    "Encoding",
];

fn field_kind(name: &str) -> Field {
    match name {
        "Name" => Field::Name,
        "Bold" => Field::Bold,
        "Italic" => Field::Italic,
        "Underline" => Field::Underline,
        _ => Field::Other,
    }
}

/// FFmpeg's sections of a script, by name; only the two style sections are
/// read, but a `Style:` or `Dialogue:` line elsewhere switches sections as
/// it does there.
#[derive(Clone, Copy, PartialEq)]
enum Section {
    ScriptInfo,
    V4Plus,
    V4,
    Events,
}

impl Section {
    fn named(name: &[u8]) -> Option<Self> {
        match name {
            b"Script Info" => Some(Section::ScriptInfo),
            b"V4+ Styles" => Some(Section::V4Plus),
            b"V4 Styles" => Some(Section::V4),
            b"Events" => Some(Section::Events),
            _ => None,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn fields(self) -> &'static [&'static str] {
        match self {
            Section::V4Plus => &V4_PLUS_FIELDS,
            Section::V4 => &V4_FIELDS,
            // Events have a format too. Their lines aren't kept, but how
            // many fields one has decides where FFmpeg reads on (see
            // `parse_entry`).
            Section::Events => {
                &["Layer", "Start", "End", "Style", "Name", "MarginL", "MarginR", "MarginV", "Effect", "Text"]
            }
            Section::ScriptInfo => &[],
        }
    }

    /// The line key that adds an entry to the section.
    fn entry_key(self) -> Option<&'static [u8]> {
        match self {
            Section::V4Plus | Section::V4 => Some(b"Style"),
            Section::Events => Some(b"Dialogue"),
            Section::ScriptInfo => None,
        }
    }
}

/// `strncmp(a, b, n) == 0` on byte strings (NUL-terminated in C).
fn strn_eq(a: &[u8], b: &[u8], n: usize) -> bool {
    for i in 0..n {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return false;
        }
        if x == 0 {
            return true;
        }
    }
    true
}

fn skip_spaces(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|&&b| b == b' ').count();
    &s[n..]
}

/// Bytes up to the first of `stop` (or the end).
fn span_until(s: &[u8], stop: &[u8]) -> usize {
    s.iter().position(|b| stop.contains(b)).unwrap_or(s.len())
}

/// `sscanf("%d")`: leading whitespace, a sign, digits.
fn scan_int(s: &[u8]) -> Option<i64> {
    let s = &s[s.iter().take_while(|b| b.is_ascii_whitespace()).count()..];
    let (negative, digits) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let len = digits.iter().take_while(|b| b.is_ascii_digit()).count();
    if len == 0 {
        return None;
    }
    // Only whether it's 0 matters here.
    let value = digits[..len].iter().fold(0i64, |v, d| v.saturating_mul(10).saturating_add((d - b'0') as i64));
    Some(if negative { -value } else { value })
}

impl Styles {
    /// FFmpeg's `ff_ass_split`, for the style sections; an error for a
    /// header with more styles than are read here.
    fn parse(header: &[u8]) -> Result<Self, String> {
        let header = header.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(header);
        let header = header.iter().position(|&b| b == 0).map_or(header, |end| &header[..end]);
        let mut styles = Styles::default();
        // Field order per section (index into its fields, `None` = ignored).
        let mut orders: [Option<Vec<Option<usize>>>; 4] = Default::default();
        let mut rest = header;
        while !rest.is_empty() {
            let line_end = span_until(rest, b"\n");
            let section = parse_section_name(rest);
            rest = &rest[(line_end + 1).min(rest.len())..];
            if let Some(section) = section {
                rest = styles.parse_section(section, rest, &mut orders)?;
            }
        }
        for (i, style) in styles.styles.iter().enumerate() {
            styles.by_name.entry(style.name.clone()).or_insert(i);
        }
        Ok(styles)
    }

    /// The lines of a section, up to the next `[`; returns what's left.
    fn parse_section<'a>(
        &mut self,
        mut section: Section,
        mut buf: &'a [u8],
        orders: &mut [Option<Vec<Option<usize>>>; 4],
    ) -> Result<&'a [u8], String> {
        while !buf.is_empty() {
            if buf[0] == b'[' {
                return Ok(buf);
            }
            let comment = buf[0] == b';' || buf.starts_with(b"!:");
            if !comment {
                let key_len = span_until(buf, b":\r\n");
                if buf.get(key_len) == Some(&b':') && section.entry_key().is_none_or(|key| !strn_eq(buf, key, key_len))
                {
                    for candidate in [Section::V4Plus, Section::Events] {
                        if candidate.entry_key().is_some_and(|key| strn_eq(buf, key, key_len)) {
                            section = candidate;
                            break;
                        }
                    }
                }
                let order = &mut orders[section.index()];
                let entry = section.entry_key().filter(|key| buf.starts_with(key) && buf.get(key.len()) == Some(&b':'));
                if section != Section::ScriptInfo && order.is_none() && buf.starts_with(b"Format:") {
                    *order = Some(parse_format(&buf[7..], section.fields()));
                } else if let Some(key) = entry {
                    let order = order.get_or_insert_with(|| (0..section.fields().len()).map(Some).collect());
                    let (style, rest) = parse_entry(&buf[key.len() + 1..], order, section.fields());
                    if matches!(section, Section::V4Plus | Section::V4) {
                        if self.styles.len() == MAX_STYLES {
                            return Err(format!("the ASS header has more than {MAX_STYLES} styles"));
                        }
                        self.styles.push(style);
                    }
                    // The next line is the one after where the fields ended.
                    buf = rest;
                }
            }
            let line_end = span_until(buf, b"\n");
            buf = &buf[(line_end + 1).min(buf.len())..];
        }
        Ok(buf)
    }

    fn get(&self, name: &[u8]) -> Option<&Style> {
        let name = if name.is_empty() { b"Default".as_slice() } else { name };
        self.by_name.get(name).map(|&i| &self.styles[i])
    }
}

/// `[Section Name]` followed by at least one more byte, as FFmpeg's
/// `sscanf(buf, "[%15[0-9A-Za-z+ ]]%c")` takes it.
fn parse_section_name(line: &[u8]) -> Option<Section> {
    let rest = line.strip_prefix(b"[")?;
    let len = rest.iter().take(15).take_while(|b| b.is_ascii_alphanumeric() || **b == b'+' || **b == b' ').count();
    if len == 0 || rest.get(len) != Some(&b']') || rest.len() <= len + 1 {
        return None;
    }
    Section::named(&rest[..len])
}

/// A `Format:` line: the position of each listed field among `fields`. Real
/// ones list about twenty: past [`MAX_FORMAT_FIELDS`] (a line of commas in a
/// crafted file), the rest is left out rather than kept in memory.
fn parse_format(mut buf: &[u8], fields: &[&str]) -> Vec<Option<usize>> {
    let mut order = Vec::new();
    while !buf.is_empty() && !matches!(buf[0], b'\r' | b'\n') && order.len() < MAX_FORMAT_FIELDS {
        buf = skip_spaces(buf);
        let len = span_until(buf, b", \r\n");
        order.push(fields.iter().position(|f| strn_eq(buf, f.as_bytes(), len)));
        let next = len + (buf.get(len) == Some(&b',')) as usize;
        buf = skip_spaces(&buf[next.min(buf.len())..]);
    }
    order
}

/// A `Style:` (or `Dialogue:`) line's values, in the order of the section's
/// format, and what's left after them. As in FFmpeg, a line with fewer values
/// than the format has fields doesn't end the entry when it ends with a
/// bare line feed: that's stepped over like a comma, and the next line is
/// read on as the entry's last values (a line ending with `\r\n` stops it).
fn parse_entry<'a>(mut buf: &'a [u8], order: &[Option<usize>], fields: &[&str]) -> (Style, &'a [u8]) {
    let mut style = Style::default();
    for (i, field) in order.iter().enumerate() {
        if buf.is_empty() || matches!(buf[0], b'\r' | b'\n') {
            break;
        }
        let last = i == order.len() - 1;
        buf = skip_spaces(buf);
        let len = span_until(buf, if last { b"\r\n".as_slice() } else { b",\r\n".as_slice() });
        let value = &buf[..len];
        match field.map(|f| field_kind(fields[f])) {
            Some(Field::Name) => style.name = value.to_vec(),
            Some(Field::Bold) => style.bold = scan_int(value).unwrap_or(style.bold),
            Some(Field::Italic) => style.italic = scan_int(value).unwrap_or(style.italic),
            Some(Field::Underline) => style.underline = scan_int(value).unwrap_or(style.underline),
            _ => {}
        }
        buf = &buf[len..];
        if !last && !buf.is_empty() {
            buf = &buf[1..];
        }
        buf = skip_spaces(buf);
    }
    (style, buf)
}

// ------------------------------------------------------------------ encoder

/// FFmpeg's WebVTT encoder. Its stack of open tags carries over from one
/// event to the next: an event whose codes it can't read stops halfway and
/// leaves its tags open for the next, as FFmpeg's does.
struct Encoder {
    styles: Styles,
    stack: Vec<u8>,
}

const STACK_SIZE: usize = 64;

impl Encoder {
    /// The text of one ASS event as WebVTT.
    fn encode(&mut self, dialog: &[u8]) -> Vec<u8> {
        let (style, text) = split_dialog(dialog);
        let mut out = Vec::new();
        self.apply_style(&mut out, style);
        self.override_codes(&mut out, text);
        out
    }

    fn push(&mut self, tag: u8) {
        if self.stack.len() < STACK_SIZE {
            self.stack.push(tag);
        }
    }

    /// Close the open tags down to the last `tag` (all of them for `None`).
    fn close(&mut self, out: &mut Vec<u8>, tag: Option<u8>) {
        let keep = match tag {
            None => 0,
            Some(tag) => match self.stack.iter().rposition(|&t| t == tag) {
                Some(at) => at,
                None => return,
            },
        };
        while self.stack.len() > keep {
            let tag = self.stack.pop().unwrap_or_default();
            out.extend_from_slice(&[b'<', b'/', tag, b'>']);
        }
    }

    fn apply_style(&mut self, out: &mut Vec<u8>, name: &[u8]) {
        let Some(style) = self.styles.get(name).cloned() else { return };
        for (on, tag) in [(style.bold, b'b'), (style.italic, b'i'), (style.underline, b'u')] {
            if on != 0 {
                out.extend_from_slice(&[b'<', tag, b'>']);
                self.push(tag);
            }
        }
    }

    /// FFmpeg's `ff_ass_split_override_codes` with the WebVTT encoder's
    /// callbacks: text as it is, `\N` and `\n` as line breaks, `\b`, `\i`
    /// and `\u` as tags, `\r` back to a style; every other code is dropped.
    fn override_codes(&mut self, out: &mut Vec<u8>, text: &[u8]) {
        let at = |i: usize| text.get(i).copied().unwrap_or(0);
        let mut i = 0;
        while i < text.len() {
            if text[i] == b'\\' && matches!(at(i + 1), b'n' | b'N') {
                out.push(b'\n');
                i += 2;
            } else if text[i] == b'{' && at(i + 1) == b'\\' {
                i += 1;
                while at(i) == b'\\' {
                    let code = at(i + 1);
                    let len = if b"bisu".contains(&code) && matches!(at(i + 2), b'0' | b'1' | b'\\' | b'}') {
                        let value = at(i + 2);
                        if code != b's' {
                            match value {
                                b'1' => {
                                    self.push(code);
                                    out.extend_from_slice(&[b'<', code, b'>']);
                                }
                                // `\b0`, or `\b` alone: back to normal.
                                _ => self.close(out, Some(code)),
                            }
                        }
                        3 + matches!(value, b'0' | b'1') as usize
                    } else if code == b'r' && reset_name(&text[i + 2..]).is_some() {
                        let name = reset_name(&text[i + 2..]).unwrap_or_default();
                        self.close(out, None);
                        self.apply_style(out, name);
                        3 + name.len()
                    } else {
                        span_until(&text[(i + 1).min(text.len())..], b"\\}") + 2
                    };
                    i += len - 1;
                }
                if at(i) != b'}' {
                    // Unreadable codes: FFmpeg gives up on the rest of the
                    // event, tags left open.
                    return;
                }
                i += 1;
            } else {
                out.push(text[i]);
                i += 1;
            }
        }
        self.close(out, None);
    }
}

/// The style named by a `\r` code (`\r}` or `\rName}`, up to 127 bytes),
/// when it ends as FFmpeg reads it.
fn reset_name(after_r: &[u8]) -> Option<&[u8]> {
    let len = after_r.iter().take(127).take_while(|b| !matches!(b, b'\\' | b'}' | 0)).count();
    matches!(after_r.get(len), Some(b'\\' | b'}')).then(|| &after_r[..len])
}

/// The style and text of an event (FFmpeg's `ff_ass_split_dialog`):
/// fields up to each comma, without their leading spaces; the text is the
/// rest.
fn split_dialog(dialog: &[u8]) -> (&[u8], &[u8]) {
    let mut rest = dialog;
    let mut style: &[u8] = b"";
    for field in 0..8 {
        rest = skip_spaces(rest);
        let len = span_until(rest, b",");
        if field == 2 {
            style = &rest[..len];
        }
        rest = &rest[(len + 1).min(rest.len())..];
    }
    (style, skip_spaces(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A track of `(start, end, data)` as WebVTT.
    fn convert(codec: TextCodec, header: &str, cues: &[(i64, i64, &str)]) -> String {
        let events: Vec<Event> = cues
            .iter()
            .map(|(start, end, data)| Event {
                start_ms: *start,
                duration_ms: end - start,
                data: data.as_bytes().to_vec(),
            })
            .collect();
        to_webvtt(codec, header.as_bytes(), &events, |_| Ok(())).unwrap()
    }

    /// The WebVTT file FFmpeg writes for cues of `(start, end, text)`.
    fn ffmpeg_file(cues: &[(i64, i64, &str)]) -> String {
        let time = |ms: i64| match ms / 3_600_000 {
            0 => format!("{:02}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000),
            h => format!("{h:02}:{:02}:{:02}.{:03}", ms / 60_000 % 60, ms / 1000 % 60, ms % 1000),
        };
        let cues: String =
            cues.iter().map(|(start, end, text)| format!("\n{} --> {}\n{text}\n", time(*start), time(*end))).collect();
        format!("WEBVTT\n{cues}")
    }

    /// Each case: start, end, the subtitle as the Matroska block holds it,
    /// and the cue text FFmpeg 9 wrote for it.
    type Case = (i64, i64, &'static str, &'static str);

    fn check(codec: TextCodec, header: &str, cases: &[Case]) {
        let input: Vec<(i64, i64, &str)> = cases.iter().map(|(s, e, data, _)| (*s, *e, *data)).collect();
        let expected: Vec<(i64, i64, &str)> = cases.iter().map(|(s, e, _, out)| (*s, *e, *out)).collect();
        assert_eq!(convert(codec, header, &input), ffmpeg_file(&expected));
    }

    #[test]
    fn cue_times_are_written_like_ffmpeg() {
        let out = convert(
            TextCodec::Subrip,
            "",
            &[(1_000, 2_000, "Hello"), (3_723_004, 3_725_000, "Later"), (59_999, 59_999, "x")],
        );
        assert_eq!(
            out,
            "WEBVTT\n\n00:01.000 --> 00:02.000\nHello\n\n01:02:03.004 --> 01:02:05.000\nLater\n\n00:59.999 --> 00:59.999\nx\n"
        );
        assert_eq!(convert(TextCodec::Subrip, "", &[]), "WEBVTT\n");
    }

    /// SubRip from SRT files muxed into Matroska by FFmpeg, then extracted
    /// by it with `-c:s webvtt`.
    #[test]
    fn subrip_matches_ffmpeg() {
        check(
            TextCodec::Subrip,
            "",
            &[
                (1_000, 2_000, "Hello there.", "Hello there."),
                (2_500, 3_000, "Line one\nLine two", "Line one\nLine two"),
                (3_500, 4_000, "<i>Italic</i> text", "<i>Italic</i> text"),
                (
                    4_500,
                    5_000,
                    "<b>Bold</b> and <u>under</u> and <s>strike</s>",
                    "<b>Bold</b> and <u>under</u> and strike",
                ),
                (5_500, 6_000, "<i><b>both</b></i> <I>Upper</I>", "<i><b>both</b></i> <i>Upper</i>"),
                (6_500, 7_000, "<i>Unclosed", "<i>Unclosed</i>"),
                (
                    7_500,
                    8_000,
                    "<font color=\"#ff0000\">Red</font> text <font face=\"Arial\" size=\"20\">face</font>",
                    "Red text face",
                ),
                (8_500, 9_000, "{\\an8}Top line", "Top line"),
                (9_500, 10_000, "{\\i1}braced{\\i0} {\\an2}second an {\\b1}x", "braced second an <b>x</b>"),
                (10_500, 11_000, "Tom &amp; Jerry &lt;3 &nbsp;", "Tom &amp; Jerry &lt;3 &nbsp;"),
                (11_500, 12_000, "a < b & c > d", "a <b> d</b>"),
                (12_500, 13_000, "<<quoted>> <span>span</span> <c.yellow>vtt</c>", "<<quoted>> span <c.yellow>vtt"),
                (13_500, 14_000, "  spaced   words  \n  second  ", "spaced   words\nsecond"),
                (14_500, 15_000, "Line\\Nbreak and \\h hard", "Line\nbreak and \\h hard"),
                (15_500, 16_000, "<i></i>", "<i></i>"),
                (16_500, 17_000, "{\\an8}", ""),
                (17_500, 18_000, "Ça va? — «oui» ♪", "Ça va? — «oui» ♪"),
                (18_500, 19_000, "one<br>two<br/>three", "one\ntwo\nthree"),
                (19_500, 20_000, "<i>multi\nline</i>", "<i>multi\nline</i>"),
                (20_000, 21_000, "overlap A", "overlap A"),
                (20_500, 21_500, "overlap B", "overlap B"),
                (22_000, 23_000, "- Dash one\n- Dash two", "- Dash one\n- Dash two"),
                (23_500, 24_000, "trailing<br>", "trailing"),
                (24_500, 25_000, "<font color=red><i>red italic</i></font>", "<i>red italic</i>"),
                (25_500, 26_000, "{Y:i}microdvd style", "microdvd style"),
                (26_500, 27_000, "tab\tinside", "tab\tinside"),
                (
                    27_500,
                    28_000,
                    "<b>open bold <i>open italic</b> closed bold",
                    "<b>open bold <i>open italic</i></b> closed bold",
                ),
                (3_723_004, 3_725_000, "Past an hour", "Past an hour"),
            ],
        );
        check(
            TextCodec::Subrip,
            "",
            &[
                // A font face is written as a code: one with a brace in it shows.
                (1_000, 2_000, "<font face=\"a}b\">x</font>", "b}x"),
                (2_500, 3_000, "<font face=x><font face=y>a</font>b</font>", "ab"),
                (3_500, 4_000, "{\\an8}<i>top italic</i>", "<i>top italic</i>"),
                (4_500, 5_000, "<b><i>x</b>y</i>", "<b><i>x</i></b>y"),
                (5_500, 6_000, "&lt;i&gt;not a tag&lt;/i&gt;", "&lt;i&gt;not a tag&lt;/i&gt;"),
                (6_500, 7_000, "<i>", "<i></i>"),
                (7_500, 8_000, "{\\an1}{\\an2}two an tags {\\b1}bold", "two an tags bold"),
                (8_500, 9_000, "<font size=\"40\" color=\"blue\" face=\"Comic Sans\">big</font> after", "big after"),
                (9_500, 10_000, "a {b c", "a {b c"),
                (10_500, 11_000, "<u>under</u> <S>strike</S> <B>BOLD</B>", "<u>under</u> strike <b>BOLD</b>"),
            ],
        );
    }

    #[test]
    fn subrip_ends_at_a_blank_line() {
        // As FFmpeg's decoder does (an SRT file can't hold one).
        check(TextCodec::Subrip, "", &[(0, 1, "first\n\nlost", "first"), (1, 2, "\nnothing", "")]);
    }

    /// An ASS script's styles: Default, Italics, Loud (bold, underlined)
    /// and Signs.
    const HEADER: &str = "[Script Info]\n; A test script\nTitle: Edge cases\nScriptType: v4.00+\nPlayResX: 1920\nPlayResY: 1080\n\n\
        [V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
        Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\
        Style: Italics,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,-1,0,0,100,100,0,0,1,2,2,2,10,10,10,1\n\
        Style: Loud,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,-1,0,-1,0,100,100,0,0,1,2,2,2,10,10,10,1\n\
        Style: Signs,Arial,40,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,8,10,10,10,1\n\n\
        [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n";

    /// ASS events from scripts muxed into Matroska by FFmpeg, in the order
    /// of the file, then extracted by it. Tags left open by an event whose
    /// codes FFmpeg can't read close in the next one.
    #[test]
    fn ass_matches_ffmpeg() {
        check(
            TextCodec::Ass,
            HEADER,
            &[
                (1_000, 2_000, "0,0,Default,,0,0,0,,Plain ass line", "Plain ass line"),
                (
                    2_500,
                    3_000,
                    "1,0,Default,Bob,0,0,0,,Line one\\NLine two\\nsoft break",
                    "Line one\nLine two\nsoft break",
                ),
                (3_500, 4_000, "2,0,Default,,0,0,0,,{\\i1}italic{\\i0} normal", "<i>italic</i> normal"),
                (
                    4_500,
                    5_000,
                    "3,0,Default,,0,0,0,,{\\b1}bold{\\b0} {\\u1}under{\\u0} {\\s1}strike{\\s0}",
                    "<b>bold</b> <u>under</u> strike",
                ),
                (5_500, 6_000, "4,0,Signs,,0,0,0,,{\\an8}Top sign", "Top sign"),
                (
                    6_500,
                    7_000,
                    "5,0,Default,,0,0,0,,{\\pos(100,200)\\c&H0000FF&\\fs40}Positioned red",
                    "Positioned red",
                ),
                (7_500, 8_000, "6,0,Italics,,0,0,0,,Styled italic", "<i>Styled italic</i>"),
                (8_500, 9_000, "7,0,Italics,,0,0,0,,Styled {\\i0}not italic", "<i>Styled </i>not italic"),
                (9_500, 10_000, "8,0,Default,,0,0,0,,{\\i1}a{\\r}b", "<i>a</i>b"),
                (10_500, 11_000, "9,0,Default,,0,0,0,,{\\p1}m 0 0 l 100 0 100 100{\\p0}", "m 0 0 l 100 0 100 100"),
                (11_500, 12_000, "10,0,Default,,0,0,0,,hard\\hspace", "hard\\hspace"),
                (12_500, 13_000, "11,0,Default,,0,0,0,,   leading spaces", "leading spaces"),
                (13_500, 14_000, "12,0,Default,,0,0,0,,", ""),
                (14_500, 15_000, "13,0,Loud,,0,0,0,,Loud style", "<b><u>Loud style</u></b>"),
                (15_500, 16_000, "14,0,Default,,0,0,0,,{\\b700}weight{\\b0}", "weight"),
                (
                    16_500,
                    17_000,
                    "15,0,Default,,0,0,0,,{\\i1\\b1}combined{\\i0\\b0} after",
                    "<i><b>combined</b></i> after",
                ),
                (17_500, 18_000, "16,0,Default,,0,0,0,,text {comment braces} in it", "text {comment braces} in it"),
                (18_500, 19_000, "17,0,Missing,,0,0,0,,Unknown style", "Unknown style"),
                (19_500, 20_000, "18,1,Default,,5,6,7,Banner;30,Layer and margins", "Layer and margins"),
                (20_500, 21_000, "19,0,Default,,0,0,0,,a < b & c > d <i>tag</i>", "a < b & c > d <i>tag</i>"),
                (21_500, 22_000, "20,0,Default,,0,0,0,,{\\i1}open at the end", "<i>open at the end</i>"),
                (
                    22_500,
                    23_000,
                    "21,0,Default,,0,0,0,,{\\b1}{\\i1}nested{\\b0}after b0",
                    "<b><i>nested</i></b>after b0",
                ),
                (23_500, 24_000, "22,0,Default,,0,0,0,,{\\i}reset i{\\i1}on{\\i}off", "reset i<i>on</i>off"),
                (24_500, 25_000, "23,0,Default,,0,0,0,,{\\fad(200,200)\\blur3}Fading", "Fading"),
                (25_500, 26_000, "24,0,Default,,0,0,0,,trailing break\\N", "trailing break\n"),
                (26_500, 27_000, "25,0,Default,,0,0,0,,{\\i1 unclosed", "<i>"),
                (27_500, 28_000, "27,0,Italics,,0,0,0,,{\\r}reset to style", "<i></i></i>reset to style"),
                (28_500, 29_000, "28,0,Default,,0,0,0,,{\\rItalics}reset to named", "<i>reset to named</i>"),
                (29_500, 30_000, "29,0,Default,,0,0,0,,{\\u1}{\\i1}u then i{\\u0}rest", "<u><i>u then i</i></u>rest"),
                (
                    30_500,
                    31_000,
                    "30,0,Default,,0,0,0,,{\\c&HFF00FF&\\1c&H00FF00&\\3c&H000000&\\alpha&H80&\\1a&H00&}colors",
                    "colors",
                ),
                (
                    31_500,
                    32_000,
                    "31,0,Default,,0,0,0,,{\\a6}legacy align{\\fnTimes New Roman}font{\\fs}reset",
                    "legacy alignfontreset",
                ),
                (32_500, 33_000, "32,0,Default,,0,0,0,,{\\move(1,2,3,4)\\org(5,6)}moved", "moved"),
                (33_500, 34_000, "33,0,Default,,0,0,0,,{\\k20}ka{\\k30}ra{\\kf40}oke", "karaoke"),
                (34_500, 35_000, "34,0,Default,,0,0,0,,{}empty braces", "{}empty braces"),
                (35_500, 36_000, "35,0,Default,,0,0,0,,back\\\\slash and {\\\\} weird", "back\\\\slash and  weird"),
                (3_723_040, 3_725_000, "26,0,Default,,0,0,0,,Past an hour, with, commas", "Past an hour, with, commas"),
            ],
        );
        check(
            TextCodec::Ass,
            HEADER,
            &[
                (1_000, 2_000, "0,0,Default,,0,0,0,,{\\t(\\i1)}animated", "<i>"),
                (2_500, 3_000, "1,0,Default,,0,0,0,,next", "next</i>"),
                (3_500, 4_000, "2,0,Default,,0,0,0,,{\\fnArial\\i1}x", "<i>x</i>"),
                (4_500, 5_000, "3,0,Default,,0,0,0,,{\\c&H00FF00&\\b1}green bold", "<b>green bold</b>"),
                (5_500, 6_000, "4,0,Default,,0,0,0,,{\\i1}a\\N{\\i0}b", "<i>a\n</i>b"),
                (6_500, 7_000, "5,0,Default,,0,0,0,,a {b c", "a {b c"),
                (7_500, 8_000, "6,0,Default,,0,0,0,,{\\i1}soft\\nbreak", "<i>soft\nbreak</i>"),
                (8_500, 9_000, "7,0,Default,,0,0,0,,{\\i10}ten", "<i>"),
                (9_500, 10_000, "8,0,Default,,0,0,0,,after ten", "after ten</i>"),
            ],
        );
    }

    #[test]
    fn ass_styles_follow_the_format_line() {
        // Fields in another order, V4 styles (no underline), a BOM, CRLF.
        let header = "\u{FEFF}[Script Info]\r\nScriptType: v4.00\r\n\r\n[V4 Styles]\r\n\
            Format: Name, Italic, Bold, Fontname\r\nStyle: Shout,0,-1,Arial\r\nStyle: Whisper, 1,0,Arial\r\n\r\n[Events]\r\n";
        check(
            TextCodec::Ass,
            header,
            &[(0, 1, "0,0,Shout,,0,0,0,,A", "<b>A</b>"), (1, 2, "1,0,Whisper,,0,0,0,,B", "<i>B</i>")],
        );
        // No header at all: no styles.
        check(TextCodec::Ass, "", &[(0, 1, "0,0,Default,,0,0,0,,{\\b1}x", "<b>x</b>")]);
    }

    /// Style lines (LF-terminated) with fewer values than the Format line,
    /// and a Dialogue line in the header: FFmpeg reads each on into the
    /// lines after it, which are lost as styles. What FFmpeg 9 made of these
    /// headers in Matroska files.
    #[test]
    fn short_header_lines_go_on_into_the_next_as_in_ffmpeg() {
        let styles = "[Script Info]\nScriptType: v4.00+\n\n[V4+ Styles]\n\
            Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline\n\
            Style: Default,Arial,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,0\n";
        let late = "Dialogue: 0,0:00:00.00\n\
            Style: Late,Arial,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,-1,0\n\
            Style: Later,Arial,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0\n";
        let header = format!(
            "{styles}Style: Short,Arial\n\
            Style: Slanted,Arial,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,-1,0\n\
            Style: Two,Arial\nStyle: Three,Arial\n\
            Style: Under,Arial,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,0,-1\n\
            Style: After,Arial,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,-1,0,0\n\
            Style: Crlf,Arial\r\n\
            Style: Kept,Arial,20,&H00FFFFFF,&H00FFFFFF,&H00000000,&H00000000,0,-1,0\n\n\
            [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n{late}"
        );
        let cases: [Case; 10] = [
            (0, 1, "0,0,Short,,0,0,0,,line", "line"),
            (1, 2, "1,0,Slanted,,0,0,0,,line", "line"),
            (2, 3, "2,0,Two,,0,0,0,,line", "line"),
            (3, 4, "3,0,Three,,0,0,0,,line", "line"),
            (4, 5, "4,0,Under,,0,0,0,,line", "line"),
            (5, 6, "5,0,After,,0,0,0,,line", "<b>line</b>"),
            (6, 7, "6,0,Crlf,,0,0,0,,line", "line"),
            (7, 8, "7,0,Kept,,0,0,0,,line", "<i>line</i>"),
            (8, 9, "8,0,Late,,0,0,0,,line", "line"),
            (9, 10, "9,0,Later,,0,0,0,,line", "<b>line</b>"),
        ];
        check(TextCodec::Ass, &header, &cases);
        // Without a Format line, a Dialogue line takes all ten fields of one.
        let header = format!("{styles}\n[Events]\n{late}");
        check(TextCodec::Ass, &header, &cases[8..]);
    }

    #[test]
    fn styles_are_found_by_name_however_many() {
        // Two of one name: FFmpeg takes the first.
        let header = "[V4+ Styles]\nFormat: Name, Bold, Italic\nStyle: Twice,0,-1\nStyle: Twice,-1,0\n";
        check(TextCodec::Ass, header, &[(0, 1, "0,0,Twice,,0,0,0,,x", "<i>x</i>")]);
        // As many as are read here, each found at once (every other one italic).
        let many: String = (0..MAX_STYLES).map(|i| format!("Style: S{i},0,{}\n", i % 2)).collect();
        let header = format!("[V4+ Styles]\nFormat: Name, Bold, Italic\n{many}");
        let events: Vec<Event> = (0..MAX_STYLES as i64)
            .map(|i| Event {
                start_ms: i,
                duration_ms: 1,
                data: format!("{i},0,S{i},,0,0,0,,{{\\rS{i}}}x").into_bytes(),
            })
            .collect();
        let out = to_webvtt(TextCodec::Ass, header.as_bytes(), &events, |_| Ok(())).unwrap();
        assert_eq!(out.matches("\n<i></i><i>x</i>\n").count(), MAX_STYLES / 2);
        // One more is more than is read here: FFmpeg converts such a track.
        let header = format!("{header}Style: Last,0,0\n");
        let err = to_webvtt(TextCodec::Ass, header.as_bytes(), &events[..1], |_| Ok(())).unwrap_err();
        assert!(err.contains("more than 10000 styles"), "{err}");
    }

    #[test]
    fn conversions_stop_when_told_to() {
        let events = vec![Event { start_ms: 0, duration_ms: 1, data: b"a".to_vec() }; 3];
        let mut asked = 0;
        let stopped = to_webvtt(TextCodec::Subrip, b"", &events, |_| {
            asked += 1;
            if asked < 2 { Ok(()) } else { Err("cancelled".to_owned()) }
        });
        assert_eq!((stopped, asked), (Err("cancelled".to_owned()), 2));
    }

    #[test]
    fn conversions_say_what_they_hold() {
        let events: Vec<Event> =
            (0..1_000).map(|i| Event { start_ms: i, duration_ms: 1, data: b"a line".to_vec() }).collect();
        let mut held = Vec::new();
        let out = to_webvtt(TextCodec::Subrip, b"", &events, |size| {
            held.push(size);
            Ok(())
        })
        .unwrap();
        // Before each event, and once done: all the WebVTT made.
        assert_eq!(held.len(), events.len() + 1);
        assert!(held.windows(2).all(|w| w[0] <= w[1]), "it only grows");
        assert_eq!(held.last(), Some(&out.capacity()));
        assert!(out.len() > 30_000);
    }

    /// WebVTT cues from a .vtt file muxed into Matroska (in WebM's layout)
    /// by FFmpeg, then extracted by it: the cue text as the block holds it.
    #[test]
    fn webvtt_matches_ffmpeg() {
        check(
            TextCodec::Webvtt,
            "",
            &[
                (1_000, 2_000, "<v Bob>Hi</v> &amp; <i>there</i>", "Hi & <i>there</i>"),
                (2_500, 3_000, "<c.yellow>colored</c> &lt;tag&gt;", "colored <tag>"),
                (3_500, 4_000, "line1\nline2", "line1\nline2"),
                (4_500, 5_000, "<b>bold <u>under</u></b>", "<b>bold <u>under</u></b>"),
                (5_500, 6_000, "timestamps <00:00:05.800>karaoke", "timestamps karaoke"),
                (6_500, 7_000, "back\\slash {braces} &nbsp;x", "back\\\u{2060}slash \\{{}braces} \\hx"),
                (7_500, 8_000, "unclosed <tag", "unclosed "),
                (8_500, 9_000, "<ruby>base<rt>ruby</rt></ruby> <lang en>text</lang>", "baseruby text"),
            ],
        );
    }

    #[test]
    fn plain_text_escapes_braces_and_backslashes() {
        // As FFmpeg's plain text decoder does (`ff_ass_bprint_text_event`);
        // an empty subtitle makes no cue.
        let out = convert(TextCodec::Text, "", &[(0, 1, "a {b} \\N c\r\nd\n"), (1, 2, "")]);
        assert_eq!(out, ffmpeg_file(&[(0, 1, "a \\{{}b} \\\u{2060}N c\nd")]));
    }

    #[test]
    fn what_ffmpeg_drops_is_dropped() {
        let events = [
            Event { start_ms: 0, duration_ms: 1, data: b"caf\xE9".to_vec() },
            Event { start_ms: 1, duration_ms: 1, data: "\u{FFFE}".as_bytes().to_vec() },
            Event { start_ms: 2, duration_ms: 1, data: b"kept\0after a NUL".to_vec() },
        ];
        assert_eq!(to_webvtt(TextCodec::Subrip, b"", &events, |_| Ok(())).unwrap(), ffmpeg_file(&[(2, 3, "kept")]));
    }
}
