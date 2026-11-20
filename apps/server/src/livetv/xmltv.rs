//! XMLTV guides, read as they stream (they run to hundreds of megabytes):
//! the channels, then only the programs of the channels wanted, within a
//! window of time.
//!
//! ```xml
//! <tv>
//!   <channel id="bbc1.uk"><display-name>BBC One</display-name><icon src="…"/></channel>
//!   <programme start="20261029200000 +0000" stop="20261029210000 +0000" channel="bbc1.uk">
//!     <title>EastEnders</title><sub-title>…</sub-title><desc>…</desc>
//!     <category>Drama</category><episode-num system="xmltv_ns">39.180.</episode-num><new/>
//!   </programme>
//! </tv>
//! ```

use std::collections::HashSet;
use std::io::BufRead;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GuideChannel {
    pub id: String,
    pub names: Vec<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub channel: String,
    pub start: i64,
    pub stop: i64,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub categories: Vec<String>,
    pub image: Option<String>,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub is_new: bool,
    pub is_live: bool,
    pub rating: Option<String>,
}

#[derive(Debug, Default)]
pub struct Guide {
    pub channels: Vec<GuideChannel>,
    pub programs: Vec<Program>,
}

/// The guide in `input`. Once its channels are read (they come first),
/// `wanted` says which of them to keep programs of; programs ending
/// before `from` or starting after `to` (unix seconds) are left out.
pub fn read<R: BufRead>(
    input: R,
    from: i64,
    to: i64,
    wanted: impl FnOnce(&[GuideChannel]) -> HashSet<String>,
) -> Result<Guide, quick_xml::Error> {
    // Text is trimmed once its element ends: `&amp;` and the like come as
    // events of their own, so the spaces around them are part of it.
    let mut reader = Reader::from_reader(input);
    let mut guide = Guide::default();
    let mut wanted_fn = Some(wanted);
    let mut keep: HashSet<String> = HashSet::new();
    let mut buf = Vec::new();
    // Where we are: the element whose text is read, and what is being built.
    let mut channel: Option<GuideChannel> = None;
    let mut program: Option<Program> = None;
    let mut text_of: Option<Vec<u8>> = None;
    let mut episode_system = String::new();
    let mut text = String::new();
    loop {
        let event = reader.read_event_into(&mut buf)?;
        match event {
            Event::Eof => break,
            Event::Start(e) if e.name().as_ref() == b"channel" => {
                channel = Some(GuideChannel { id: attribute(&e, b"id").unwrap_or_default(), ..Default::default() });
            }
            // A channel saying nothing of itself.
            Event::Empty(e) if e.name().as_ref() == b"channel" => {}
            Event::Start(e) if e.name().as_ref() == b"programme" => {
                if let Some(wanted) = wanted_fn.take() {
                    keep = wanted(&guide.channels);
                }
                let id = attribute(&e, b"channel").unwrap_or_default();
                let start = attribute(&e, b"start").as_deref().and_then(parse_time);
                let stop = attribute(&e, b"stop").as_deref().and_then(parse_time);
                program = match (start, stop) {
                    (Some(start), Some(stop)) if keep.contains(&id) && stop > from && start < to && stop > start => {
                        Some(Program { channel: id, start, stop, ..Default::default() })
                    }
                    // A program without its end lasts until the next one: an hour said.
                    (Some(start), None) if keep.contains(&id) && start + 3600 > from && start < to => {
                        Some(Program { channel: id, start, stop: start + 3600, ..Default::default() })
                    }
                    _ => {
                        reader.read_to_end_into(e.name(), &mut Vec::new())?;
                        None
                    }
                };
            }
            Event::Empty(e) => {
                let name = e.name().as_ref().to_vec();
                if let Some(c) = channel.as_mut()
                    && name == b"icon"
                {
                    c.icon = c.icon.take().or_else(|| attribute(&e, b"src"));
                }
                if let Some(p) = program.as_mut() {
                    match name.as_slice() {
                        b"icon" | b"image" => p.image = p.image.take().or_else(|| attribute(&e, b"src")),
                        b"new" => p.is_new = true,
                        b"live" => p.is_live = true,
                        _ => {}
                    }
                }
            }
            Event::Start(e) => {
                let name = e.name().as_ref().to_vec();
                if program.is_some() || channel.is_some() {
                    if name == b"episode-num" {
                        episode_system = attribute(&e, b"system").unwrap_or_default();
                    }
                    if let Some(p) = program.as_mut() {
                        match name.as_slice() {
                            b"icon" => p.image = p.image.take().or_else(|| attribute(&e, b"src")),
                            b"new" => p.is_new = true,
                            b"live" => p.is_live = true,
                            _ => {}
                        }
                    }
                    if let Some(c) = channel.as_mut()
                        && name == b"icon"
                    {
                        c.icon = c.icon.take().or_else(|| attribute(&e, b"src"));
                    }
                    text.clear();
                    text_of = Some(name);
                }
            }
            Event::Text(t) => {
                if text_of.is_some() {
                    text.push_str(&t.xml10_content().unwrap_or_default());
                }
            }
            // `&amp;`, `&#233;`: XML's own; others (HTML's `&nbsp;`) are left out.
            Event::GeneralRef(r) => {
                if text_of.is_some() {
                    if let Ok(Some(c)) = r.resolve_char_ref() {
                        text.push(c);
                    } else if let Some(s) = r.decode().ok().as_deref().and_then(resolve_predefined_entity) {
                        text.push_str(s);
                    }
                }
            }
            Event::CData(t) => {
                if text_of.is_some() {
                    text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Event::End(e) => {
                let name = e.name().as_ref().to_vec();
                if name == b"channel" {
                    if let Some(c) = channel.take().filter(|c| !c.id.is_empty()) {
                        guide.channels.push(c);
                    }
                } else if name == b"programme" {
                    if let Some(p) = program.take().filter(|p| !p.title.is_empty()) {
                        guide.programs.push(p);
                    }
                } else if text_of.as_deref() == Some(name.as_slice()) {
                    let value = std::mem::take(&mut text).trim().to_owned();
                    text_of = None;
                    if value.is_empty() {
                        // The value element of `<rating>` ends here, or an empty one.
                    } else if let Some(c) = channel.as_mut() {
                        if name == b"display-name" {
                            c.names.push(value);
                        }
                    } else if let Some(p) = program.as_mut() {
                        match name.as_slice() {
                            b"title" if p.title.is_empty() => p.title = value,
                            b"sub-title" if p.subtitle.is_none() => p.subtitle = Some(value),
                            b"desc" if p.description.is_none() => p.description = Some(value),
                            b"category" if !p.categories.contains(&value) => p.categories.push(value),
                            b"value" if p.rating.is_none() => p.rating = Some(value),
                            b"episode-num" => {
                                let (season, episode) = match episode_system.as_str() {
                                    "xmltv_ns" => xmltv_ns(&value),
                                    "onscreen" | "" => onscreen(&value),
                                    _ => (None, None),
                                };
                                p.season = p.season.or(season);
                                p.episode = p.episode.or(episode);
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }
    if let Some(wanted) = wanted_fn.take() {
        let _ = wanted(&guide.channels);
    }
    Ok(guide)
}

fn attribute(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok().map(|v| v.trim().to_owned()))
        .filter(|v| !v.is_empty())
}

/// `20261029200000 +0100` (seconds and the offset may be missing; no
/// offset is UTC) as unix seconds.
pub fn parse_time(text: &str) -> Option<i64> {
    let text = text.trim();
    let (stamp, offset) = match text.split_once(' ') {
        Some((stamp, offset)) => (stamp, Some(offset.trim())),
        None => match text.find(['+', '-']) {
            Some(i) if i >= 12 => (&text[..i], Some(&text[i..])),
            _ => (text, None),
        },
    };
    let digits = |range: std::ops::Range<usize>| stamp.get(range).and_then(|s| s.parse::<i64>().ok());
    let (year, month, day) = (digits(0..4)?, digits(4..6)?, digits(6..8)?);
    let (hour, minute) = (digits(8..10).unwrap_or(0), digits(10..12).unwrap_or(0));
    let second = digits(12..14).unwrap_or(0);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second;
    if let Some(offset) = offset.filter(|o| o.len() >= 5) {
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let h: i64 = offset.get(1..3)?.parse().ok()?;
        let m: i64 = offset.get(3..5)?.parse().ok()?;
        secs -= sign * (h * 3600 + m * 60);
    }
    Some(secs)
}

/// Days since 1970-01-01 of a date in the proleptic Gregorian calendar.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `xmltv_ns`: `season.episode.part`, counted from 0, each maybe `n/total`.
fn xmltv_ns(text: &str) -> (Option<i64>, Option<i64>) {
    let mut parts = text.split('.');
    let number = |part: Option<&str>| {
        part.and_then(|p| p.split('/').next()).and_then(|n| n.trim().parse::<i64>().ok()).map(|n| n + 1)
    };
    (number(parts.next()), number(parts.next()))
}

/// `S01E05`, `S1 E5`, `1x05`.
fn onscreen(text: &str) -> (Option<i64>, Option<i64>) {
    let upper = text.to_ascii_uppercase();
    let number_after = |s: &str, tag: char| -> Option<i64> {
        let i = s.find(tag)?;
        let digits: String = s[i + 1..].trim_start().chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    if upper.contains('S') && upper.contains('E') {
        return (number_after(&upper, 'S'), number_after(&upper, 'E'));
    }
    if let Some((s, e)) = upper.split_once('X') {
        return (s.trim().parse().ok(), e.trim().parse().ok());
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUIDE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE tv SYSTEM "xmltv.dtd">
<tv generator-info-name="test">
  <channel id="bbc1.uk"><display-name lang="en">BBC One</display-name><display-name>BBC1</display-name><icon src="https://l/bbc1.png"/></channel>
  <channel id="itv.uk"><display-name>ITV</display-name></channel>
  <programme start="20261029200000 +0100" stop="20261029210000 +0100" channel="bbc1.uk">
    <title lang="en">EastEnders &amp; Friends</title>
    <sub-title>Episode 7</sub-title>
    <desc><![CDATA[Drama <in> the square.]]></desc>
    <category>Drama</category><category>Soap</category>
    <episode-num system="xmltv_ns">39.6.</episode-num>
    <icon src="https://i/ee.jpg" />
    <rating system="UK"><value>12</value></rating>
    <new />
  </programme>
  <programme start="20261029210000 +0100" channel="bbc1.uk"><title>News</title><live/>
    <episode-num system="onscreen">S2 E14</episode-num></programme>
  <programme start="20261029200000 +0000" stop="20261029210000 +0000" channel="itv.uk"><title>Not wanted</title></programme>
  <programme start="20200101000000 +0000" stop="20200101010000 +0000" channel="bbc1.uk"><title>Long gone</title></programme>
</tv>"#;

    #[test]
    fn reads_the_programmes_of_the_channels_wanted() {
        let from = parse_time("20261029000000").unwrap();
        let to = from + 14 * 86_400;
        let mut seen = Vec::new();
        let guide = read(GUIDE.as_bytes(), from, to, |channels| {
            seen = channels.to_vec();
            ["bbc1.uk".to_owned()].into()
        })
        .unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].names, ["BBC One", "BBC1"]);
        assert_eq!(seen[0].icon.as_deref(), Some("https://l/bbc1.png"));
        assert_eq!(guide.programs.len(), 2);
        let ee = &guide.programs[0];
        assert_eq!(ee.title, "EastEnders & Friends");
        assert_eq!(ee.start, parse_time("20261029190000 +0000").unwrap());
        assert_eq!(ee.stop - ee.start, 3600);
        assert_eq!(ee.description.as_deref(), Some("Drama <in> the square."));
        assert_eq!(ee.categories, ["Drama", "Soap"]);
        assert_eq!((ee.season, ee.episode), (Some(40), Some(7)));
        assert_eq!(
            (ee.image.as_deref(), ee.rating.as_deref(), ee.is_new),
            (Some("https://i/ee.jpg"), Some("12"), true)
        );
        let news = &guide.programs[1];
        assert_eq!((news.is_live, news.season, news.episode, news.stop - news.start), (true, Some(2), Some(14), 3600));
    }

    #[test]
    fn references_in_text_and_attributes_are_resolved() {
        let guide = r#"<tv><channel id="a&amp;b"><display-name>Caf&#233; &lt;TV&gt;</display-name></channel>
  <programme start="20261029200000" stop="20261029210000" channel="a&amp;b">
    <title> Tom &amp; Jerry&nbsp;Show </title><desc>Line one
line two</desc></programme></tv>"#;
        let from = parse_time("20261029000000").unwrap();
        let mut names = Vec::new();
        let guide = read(guide.as_bytes(), from, from + 86_400, |channels| {
            names = channels.iter().flat_map(|c| c.names.clone()).collect();
            channels.iter().map(|c| c.id.clone()).collect()
        })
        .unwrap();
        assert_eq!(names, ["Café <TV>"]);
        assert_eq!(guide.programs[0].channel, "a&b");
        // HTML's entities aren't XML's: left out, the rest kept.
        assert_eq!(guide.programs[0].title, "Tom & JerryShow");
        assert_eq!(guide.programs[0].description.as_deref(), Some("Line one\nline two"));
    }

    #[test]
    fn times_are_read_with_their_offsets() {
        assert_eq!(parse_time("19700101000000 +0000"), Some(0));
        assert_eq!(parse_time("19700101010000 +0100"), Some(0));
        assert_eq!(parse_time("19700101000000 -0130"), Some(5400));
        assert_eq!(parse_time("197001010100+0100"), Some(0));
        assert_eq!(parse_time("20261029200000"), Some(1_793_304_000));
        assert_eq!(parse_time("2026"), None);
        assert_eq!(onscreen("1x05"), (Some(1), Some(5)));
    }
}
