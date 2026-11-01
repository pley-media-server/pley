//! WebVTT as the server writes it (`/api/files/{id}/subtitles/{n}.vtt`,
//! docs/API.md): cues with their times, text in bold, italic and underline
//! with its line breaks, and the settings that place a cue (`line`,
//! `position`, `align`, `size`).
//!
//! The server reduces every text format to that markup, so nothing else is
//! kept: voice and class spans and timestamps inside cues are dropped, and
//! entities are decoded into the text. A file still being read out of its
//! video (`X-Pley-Complete: 0`) ends after its last whole cue; anything cut
//! short is skipped, never an error.

use std::sync::Arc;

/// A run of a cue's text in one style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Span {
    /// Plain text, no style.
    pub fn plain(text: impl Into<String>) -> Span {
        Span { text: text.into(), ..Span::default() }
    }
}

/// Where a cue asks to sit vertically (WebVTT's `line`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueLine {
    /// A line number (from the top when 0 or more) or a percentage of the height.
    pub value: f32,
    /// `value` counts lines (`line:2`) rather than percent (`line:20%`).
    pub snap_to_lines: bool,
}

/// How a cue's lines are aligned (WebVTT's `align`; `middle` reads as centre).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CueAlign {
    Start,
    #[default]
    Center,
    End,
    Left,
    Right,
}

/// One cue: when it shows, its lines of styled text, and where it asks to go.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCue {
    /// When it shows and stops showing, in ms of the track.
    pub start_ms: u64,
    pub end_ms: u64,
    /// The cue's lines as written (line breaks kept), each a run of spans.
    pub lines: Vec<Vec<Span>>,
    /// `None`: the last line, at the bottom.
    pub line: Option<CueLine>,
    /// Percentage of the width the cue's box is placed at; `None`: centred.
    pub position: Option<f32>,
    /// Percentage of the width the cue's box takes; `None`: 100.
    pub size: Option<f32>,
    pub align: CueAlign,
}

impl TextCue {
    /// The cue's text without its styles, lines joined with `\n`: for tests
    /// and the screen reader.
    pub fn plain_text(&self) -> String {
        let lines: Vec<String> = self.lines.iter().map(|l| l.iter().map(|s| s.text.as_str()).collect()).collect();
        lines.join("\n")
    }
}

/// Reads a WebVTT file, or as much of one as there is, into its cues sorted
/// by start, then end.
pub fn parse_vtt(text: &str) -> Vec<TextCue> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.split('\n').collect();
        let Some(at) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((start_ms, end_ms, settings)) = timing(lines[at]) else { continue };
        let body = lines[at + 1..].join("\n");
        let body = body.trim();
        if body.is_empty() || end_ms <= start_ms {
            continue;
        }
        let mut cue = TextCue {
            start_ms,
            end_ms,
            lines: cue_lines(body),
            line: None,
            position: None,
            size: None,
            align: CueAlign::Center,
        };
        apply_settings(&mut cue, settings);
        cues.push(cue);
    }
    cues.sort_by(|a, b| a.start_ms.cmp(&b.start_ms).then(a.end_ms.cmp(&b.end_ms)));
    cues
}

/// `"01:02:03.456"` (or `"02:03,456"`) in milliseconds; `None` when it isn't a
/// WebVTT time.
pub fn time_ms(time: &str) -> Option<u64> {
    timestamp(time.trim()).filter(|(_, rest)| rest.is_empty()).map(|(ms, _)| ms)
}

/// A timing line, `start --> end settings`: the times and the settings.
fn timing(line: &str) -> Option<(u64, u64, &str)> {
    let (start, rest) = timestamp(line.trim_start())?;
    let after = rest.trim_start();
    if after.len() == rest.len() {
        return None;
    }
    let rest = after.strip_prefix("-->")?;
    let after = rest.trim_start();
    if after.len() == rest.len() {
        return None;
    }
    let (end, settings) = timestamp(after)?;
    Some((start, end, settings))
}

/// A time at the start of `s` (`[h:]m:ss.f`, one to three digits of
/// fraction, a comma accepted) and what follows it.
fn timestamp(s: &str) -> Option<(u64, &str)> {
    let b = s.as_bytes();
    let mut runs: Vec<(u64, usize)> = Vec::with_capacity(3);
    let mut i = 0;
    loop {
        let start = i;
        let mut value: u64 = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            value = value.saturating_mul(10).saturating_add(u64::from(b[i] - b'0'));
            i += 1;
        }
        if i == start || runs.len() == 3 {
            return None;
        }
        runs.push((value, i - start));
        if i < b.len() && b[i] == b':' {
            i += 1;
            continue;
        }
        break;
    }
    if i >= b.len() || (b[i] != b'.' && b[i] != b',') {
        return None;
    }
    i += 1;
    let start = i;
    let mut fraction: u64 = 0;
    while i < b.len() && i - start < 3 && b[i].is_ascii_digit() {
        fraction = fraction * 10 + u64::from(b[i] - b'0');
        i += 1;
    }
    let digits = i - start;
    if digits == 0 {
        return None;
    }
    let fraction_ms = fraction * 10u64.pow(3 - digits as u32);
    let (h, m, sec) = match runs.as_slice() {
        [(m, ml), (sec, 2)] if *ml <= 2 => (0, *m, *sec),
        [(h, _), (m, ml), (sec, 2)] if *ml <= 2 => (*h, *m, *sec),
        _ => return None,
    };
    let seconds = h.saturating_mul(3600).saturating_add(m * 60).saturating_add(sec);
    Some((seconds.saturating_mul(1000).saturating_add(fraction_ms), &s[i..]))
}

fn apply_settings(cue: &mut TextCue, settings: &str) {
    for setting in settings.split_whitespace() {
        let mut parts = setting.split(':');
        let name = parts.next().unwrap_or("");
        let Some(raw) = parts.next().filter(|r| !r.is_empty()) else { continue };
        let value = raw.split(',').next().unwrap_or("");
        match name {
            "line" => {
                let (number, percent) = match value.strip_suffix('%') {
                    Some(n) => (n, true),
                    None => (value, false),
                };
                if let Some(v) = decimal(number, true) {
                    cue.line = Some(CueLine { value: v, snap_to_lines: !percent });
                }
            }
            "position" => {
                if let Some(v) = value.strip_suffix('%').and_then(|n| decimal(n, false)) {
                    cue.position = Some(v);
                }
            }
            "size" => {
                if let Some(v) = value.strip_suffix('%').and_then(|n| decimal(n, false)) {
                    cue.size = Some(v);
                }
            }
            "align" => match value {
                "start" => cue.align = CueAlign::Start,
                "center" | "middle" => cue.align = CueAlign::Center,
                "end" => cue.align = CueAlign::End,
                "left" => cue.align = CueAlign::Left,
                "right" => cue.align = CueAlign::Right,
                _ => {}
            },
            _ => {}
        }
    }
}

/// `-?\d+(\.\d+)?` (the sign only when `signed`) as a number.
fn decimal(s: &str, signed: bool) -> Option<f32> {
    let digits = if signed { s.strip_prefix('-').unwrap_or(s) } else { s };
    let (whole, fraction) = match digits.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (digits, None),
    };
    let all_digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(whole) || fraction.is_some_and(|f| !all_digits(f)) {
        return None;
    }
    s.parse().ok()
}

/// The entities WebVTT names, as the TV app decodes them.
fn named_entity(name: &str) -> Option<char> {
    Some(match name.to_ascii_lowercase().as_str() {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "nbsp" => '\u{a0}',
        "lrm" => '\u{200e}',
        "rlm" => '\u{200f}',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    })
}

/// A cue's text as lines of styled spans: `<b>`, `<i>` and `<u>` kept (closed
/// when the cue forgets to, overlaps repaired), every other tag dropped,
/// entities decoded, and spaces collapsed as the TV app's `pre-line` shows
/// them.
pub fn cue_lines(body: &str) -> Vec<Vec<Span>> {
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    let mut open: Vec<u8> = Vec::new();
    let b = body.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let style = (open.contains(&b'b'), open.contains(&b'i'), open.contains(&b'u'));
        match b[i] {
            b'\n' => {
                lines.push(Vec::new());
                i += 1;
            }
            b'<' => match tag(body, i) {
                Some((closing, name, end)) => {
                    let name = name.to_ascii_lowercase();
                    if let [t @ (b'b' | b'i' | b'u')] = name.as_bytes() {
                        if closing {
                            if let Some(p) = open.iter().rposition(|o| o == t) {
                                open.remove(p);
                            }
                        } else {
                            open.push(*t);
                        }
                    }
                    i = end;
                }
                None => {
                    push(&mut lines, "<", style);
                    i += 1;
                }
            },
            b'&' => match entity(body, i) {
                Some((Some(ch), end)) => {
                    push(&mut lines, ch.encode_utf8(&mut [0; 4]), style);
                    i = end;
                }
                Some((None, end)) => {
                    push(&mut lines, &body[i..end], style);
                    i = end;
                }
                None => {
                    push(&mut lines, "&", style);
                    i += 1;
                }
            },
            _ => {
                let end = body[i..].find(['<', '&', '\n']).map_or(body.len(), |n| i + n);
                push(&mut lines, &body[i..end], style);
                i = end;
            }
        }
    }
    for line in &mut lines {
        tidy(line);
    }
    lines
}

/// Adds text in a style to the last line, joining the last span when it has
/// the same style.
fn push(lines: &mut [Vec<Span>], text: &str, (bold, italic, underline): (bool, bool, bool)) {
    let Some(line) = lines.last_mut() else { return };
    if let Some(last) = line.last_mut()
        && (last.bold, last.italic, last.underline) == (bold, italic, underline)
    {
        last.text.push_str(text);
        return;
    }
    line.push(Span { text: text.to_owned(), bold, italic, underline });
}

/// A tag at `at` (`<b>`, `</i>`, `<v Joe>`, `<c.yellow>`, `<00:01.000>`):
/// whether it closes, its name, and where it ends.
fn tag(body: &str, at: usize) -> Option<(bool, &str, usize)> {
    let b = body.as_bytes();
    let mut i = at + 1;
    let closing = b.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let start = i;
    match b.get(i) {
        Some(c) if c.is_ascii_alphabetic() => {
            while i < b.len() && b[i].is_ascii_alphabetic() {
                i += 1;
            }
        }
        Some(c) if c.is_ascii_digit() => {
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b':' || b[i] == b'.') {
                i += 1;
            }
        }
        _ => return None,
    }
    let name = &body[start..i];
    let close = body[i..].find('>')?;
    Some((closing, name, i + close + 1))
}

/// An entity at `at` (`&amp;`, `&#9834;`, `&#x266a;`): the character it
/// stands for (`None` for one that stands for none, kept as written) and
/// where it ends.
fn entity(body: &str, at: usize) -> Option<(Option<char>, usize)> {
    let b = body.as_bytes();
    let mut i = at + 1;
    let ch = if b.get(i) == Some(&b'#') {
        i += 1;
        let hex = b.get(i) == Some(&b'x');
        if hex {
            i += 1;
        }
        let start = i;
        let mut code: u32 = 0;
        while i < b.len() && (if hex { b[i].is_ascii_hexdigit() } else { b[i].is_ascii_digit() }) {
            let d = (b[i] as char).to_digit(16).unwrap_or(0);
            code = code.saturating_mul(if hex { 16 } else { 10 }).saturating_add(d);
            i += 1;
        }
        if i == start {
            return None;
        }
        if code > 0 && code < 0x110000 { char::from_u32(code) } else { None }
    } else {
        let start = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        if i == start {
            return None;
        }
        named_entity(&body[start..i])
    };
    if b.get(i) != Some(&b';') {
        return None;
    }
    Some((ch, i + 1))
}

/// Collapses runs of spaces and tabs into one space and drops them at the
/// line's ends, as CSS `white-space: pre-line` does.
fn tidy(line: &mut Vec<Span>) {
    let mut after_space = true;
    for span in line.iter_mut() {
        if !span.text.contains([' ', '\t']) {
            if !span.text.is_empty() {
                after_space = false;
            }
            continue;
        }
        let mut out = String::with_capacity(span.text.len());
        for ch in span.text.chars() {
            if ch == ' ' || ch == '\t' {
                if !after_space {
                    out.push(' ');
                }
                after_space = true;
            } else {
                out.push(ch);
                after_space = false;
            }
        }
        span.text = out;
    }
    if let Some(last) = line.iter_mut().rev().find(|s| !s.text.is_empty())
        && last.text.ends_with(' ')
    {
        last.text.pop();
    }
    line.retain(|s| !s.text.is_empty());
}

/// How far a set of cues reaches: its last end and how many there are.
fn reach<'a>(cues: impl IntoIterator<Item = &'a TextCue>) -> (u64, usize) {
    cues.into_iter().fold((0, 0), |(end, n), c| (end.max(c.end_ms), n + 1))
}

/// Whether `fresh`, a later answer for the same track, replaces `old`: it
/// reaches further (a later last end, or as far with more cues). A partial
/// answer never removes cues an earlier one had.
pub fn reaches_further<'a, 'b>(
    old: impl IntoIterator<Item = &'a TextCue>,
    fresh: impl IntoIterator<Item = &'b TextCue>,
) -> bool {
    let ((old_end, old_n), (fresh_end, fresh_n)) = (reach(old), reach(fresh));
    fresh_end > old_end || (fresh_end == old_end && fresh_n > old_n)
}

/// A track's cues, for fast lookups while playing. Times are the track's
/// (the playback position less the sync offset), so they may be negative.
#[derive(Clone, Debug, Default)]
pub struct CueIndex {
    cues: Vec<Arc<TextCue>>,
    /// The longest cue: how far back a lookup has to look.
    longest: u64,
}

impl CueIndex {
    /// An index of `cues`, in any order.
    pub fn new(cues: Vec<TextCue>) -> CueIndex {
        let mut cues: Vec<Arc<TextCue>> = cues.into_iter().map(Arc::new).collect();
        cues.sort_by_key(|c| c.start_ms);
        let longest = cues.iter().map(|c| c.end_ms - c.start_ms).max().unwrap_or(0);
        CueIndex { cues, longest }
    }

    /// How many cues the track has.
    pub fn len(&self) -> usize {
        self.cues.len()
    }

    /// The track has no cue.
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// Every cue, sorted by start.
    pub fn cues(&self) -> &[Arc<TextCue>] {
        &self.cues
    }

    /// The cues showing at `ms`, earliest first.
    pub fn at(&self, ms: i64) -> impl Iterator<Item = &Arc<TextCue>> {
        self.cues[self.first_starting_at_or_after(ms.saturating_sub(self.longest as i64))..]
            .iter()
            .take_while(move |c| c.start_ms as i64 <= ms)
            .filter(move |c| ms < c.end_ms as i64)
    }

    /// When the cues showing change after `ms`: the next start or end; `None`
    /// when nothing changes any more.
    pub fn next_change(&self, ms: i64) -> Option<i64> {
        let mut next: Option<i64> = None;
        for c in &self.cues[self.first_starting_at_or_after(ms.saturating_sub(self.longest as i64))..] {
            let (start, end) = (c.start_ms as i64, c.end_ms as i64);
            if start > ms {
                next = Some(next.map_or(start, |n| n.min(start)));
                break;
            }
            if end > ms {
                next = Some(next.map_or(end, |n| n.min(end)));
            }
        }
        next
    }

    fn first_starting_at_or_after(&self, ms: i64) -> usize {
        self.cues.partition_point(|c| (c.start_ms as i64) < ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VTT: &str = "WEBVTT

1
00:00:01.000 --> 00:00:03.500
Hello <i>there</i>

2
00:00:02.000 --> 00:00:04.000 line:0 position:20% align:start size:50%
Sign & <b>top</b>

00:01:00,250 --> 00:01:02,000
Comma times
second line
";

    fn span(text: &str, bold: bool, italic: bool, underline: bool) -> Span {
        Span { text: text.into(), bold, italic, underline }
    }

    fn one_line(body: &str) -> Vec<Span> {
        let mut lines = cue_lines(body);
        assert_eq!(lines.len(), 1, "{lines:?}");
        lines.remove(0)
    }

    #[test]
    fn reads_times_text_and_settings() {
        let cues = parse_vtt(VTT);
        let times: Vec<(u64, u64)> = cues.iter().map(|c| (c.start_ms, c.end_ms)).collect();
        assert_eq!(times, [(1000, 3500), (2000, 4000), (60250, 62000)]);
        assert_eq!(cues[0].lines, [vec![Span::plain("Hello "), span("there", false, true, false)]]);
        let c = &cues[1];
        assert_eq!(c.line, Some(CueLine { value: 0.0, snap_to_lines: true }));
        assert_eq!((c.position, c.align, c.size), (Some(20.0), CueAlign::Start, Some(50.0)));
        assert_eq!(c.lines, [vec![Span::plain("Sign & "), span("top", true, false, false)]]);
        assert_eq!(cues[2].lines, [vec![Span::plain("Comma times")], vec![Span::plain("second line")]]);
        assert_eq!(cues[2].plain_text(), "Comma times\nsecond line");
        assert_eq!((cues[0].line, cues[0].align), (None, CueAlign::Center));
    }

    #[test]
    fn keeps_only_safe_markup() {
        assert_eq!(
            one_line("<script>alert(1)</script><v Joe>Hi</v> <c.yellow>x</c> &lt;b&gt;"),
            [Span::plain("alert(1)Hi x <b>")]
        );
        assert_eq!(one_line("<i>open <b>both"), [span("open ", false, true, false), span("both", true, true, false)]);
        assert_eq!(
            one_line("<i>a <b>b</i> c</b>"),
            [span("a ", false, true, false), span("b", true, true, false), span(" c", true, false, false)]
        );
        assert_eq!(one_line("&#9834; &amp;"), [Span::plain("♪ &")]);
    }

    #[test]
    fn underline_entities_and_odd_text() {
        assert_eq!(one_line("<u>under</u>line"), [span("under", false, false, true), Span::plain("line")]);
        assert_eq!(one_line("&#x266a; &nbsp;&quot;&APOS;"), [Span::plain("♪ \u{a0}\"'")]);
        // Unknown entities, unfinished tags and code points that aren't characters stay as written.
        assert_eq!(one_line("&bogus; a < b &#0; &#xD800; &amp"), [Span::plain("&bogus; a < b &#0; &#xD800; &amp")]);
        assert_eq!(one_line("  two   spaces\there  "), [Span::plain("two spaces here")]);
        assert_eq!(cue_lines("<i>a</i>\n<b></b>\nb").len(), 3);
    }

    #[test]
    fn parses_times_with_and_without_hours() {
        assert_eq!(time_ms("01:02:03.456"), Some(3_723_456));
        assert_eq!(time_ms("02:03,5"), Some(123_500));
        assert_eq!(time_ms("123:45:01.07"), Some(((123 * 3600 + 45 * 60 + 1) * 1000) + 70));
        for bad in ["1:2.5", "01:02", "1:2:3:04.5", "001:02.5", "ab:cd.e", ""] {
            assert_eq!(time_ms(bad), None, "{bad}");
        }
    }

    #[test]
    fn finds_the_cues_showing_and_when_that_changes() {
        let index = CueIndex::new(parse_vtt(VTT));
        let starts = |ms: i64| index.at(ms).map(|c| c.start_ms).collect::<Vec<_>>();
        assert!(starts(500).is_empty());
        assert_eq!(starts(2500), [1000, 2000]);
        assert_eq!(starts(3500), [2000]);
        assert!(starts(-4000).is_empty());
        assert_eq!(index.next_change(0), Some(1000));
        assert_eq!(index.next_change(1000), Some(2000));
        assert_eq!(index.next_change(2500), Some(3500));
        assert_eq!(index.next_change(5000), Some(60250));
        assert_eq!(index.next_change(70000), None);
        assert_eq!(index.next_change(-3000), Some(1000));
    }

    #[test]
    fn keeps_the_partial_answer_that_reaches_further() {
        let short = parse_vtt("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\na\n");
        let long = parse_vtt("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\na\n\n00:00:05.000 --> 00:00:06.000\nb\n");
        assert!(reaches_further(&[], &short));
        assert!(reaches_further(&short, &long));
        assert!(!reaches_further(&long, &short));
        assert!(!reaches_further(&long, &long));
    }

    #[test]
    fn reads_what_there_is_of_a_file_cut_short() {
        // A byte-order mark, CRLF, no header, a cue without text, one whose end
        // isn't after its start, and a last cue cut inside its timing line.
        let text = "\u{feff}00:00:01.000 --> 00:00:02.000\r\nOne\r\n\r\n00:00:03.000 --> 00:00:04.000\r\n\r\n\
                    00:00:05.000 --> 00:00:05.000\r\nNever\r\n\r\n00:00:06.000 --> 00:0";
        let cues = parse_vtt(text);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].plain_text(), "One");
        assert!(parse_vtt("").is_empty());
        assert!(parse_vtt("WEBVTT\n\n00:00:01.000 -->").is_empty());
    }

    #[test]
    fn reads_line_settings_in_lines_and_percent() {
        let cues = parse_vtt(
            "WEBVTT\n\n00:01.000 --> 00:02.000 line:-1 align:middle\na\n\n\
             00:01.000 --> 00:02.000 line:85.5%,end position:x% align:right\nb\n",
        );
        assert_eq!(cues[0].line, Some(CueLine { value: -1.0, snap_to_lines: true }));
        assert_eq!(cues[0].align, CueAlign::Center);
        assert_eq!(cues[1].line, Some(CueLine { value: 85.5, snap_to_lines: false }));
        assert_eq!((cues[1].position, cues[1].align), (None, CueAlign::Right));
    }
}
