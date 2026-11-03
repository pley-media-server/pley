//! Extended M3U playlists of channels, as IPTV providers hand them out:
//!
//! ```text
//! #EXTM3U url-tvg="https://example.com/guide.xml.gz"
//! #EXTINF:-1 tvg-id="bbc1.uk" tvg-chno="101" tvg-logo="https://…/bbc1.png" group-title="UK",BBC One
//! #EXTVLCOPT:http-user-agent=VLC/3.0
//! http://example.com/live/user/pass/1234.ts
//! ```
//!
//! Films and series that Xtream-style playlists list with their channels
//! (`/movie/…`, `/series/…`) are left out: they are no channels.

use std::collections::HashSet;

/// A channel the playlist lists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    /// What it is known by in the playlist: its `tvg-id` with its URL when
    /// it has one (several entries may share a guide id), else its URL.
    pub key: String,
    pub name: String,
    pub number: Option<String>,
    pub group: String,
    pub logo: String,
    pub guide_id: String,
    pub url: String,
    /// `#EXTVLCOPT:http-user-agent`, which some providers require.
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Playlist {
    /// The guide the playlist names (`url-tvg`, `x-tvg-url`), the first
    /// web address when it names several.
    pub guide_url: Option<String>,
    pub entries: Vec<Entry>,
}

pub fn parse(text: &str) -> Playlist {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut playlist = Playlist::default();
    let mut pending: Option<Entry> = None;
    let mut keys = HashSet::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix("#EXTM3U") {
            let attrs = attributes(header);
            // A web address only: a path or a `file:` address would have the
            // server open a file the provider chose, and on Windows a share
            // on the provider's own computer, signing in to it.
            playlist.guide_url = ["url-tvg", "x-tvg-url", "tvg-url"]
                .iter()
                .filter_map(|k| attr(&attrs, k))
                .find_map(|urls| urls.split(',').map(str::trim).find(|u| is_web_url(u)).map(str::to_owned));
        } else if let Some(info) = line.strip_prefix("#EXTINF:") {
            pending = Some(extinf(info));
        } else if let Some(group) = line.strip_prefix("#EXTGRP:") {
            if let Some(entry) = pending.as_mut().filter(|e| e.group.is_empty()) {
                entry.group = group.trim().to_owned();
            }
        } else if let Some(opt) = line.strip_prefix("#EXTVLCOPT:") {
            if let (Some(entry), Some(agent)) = (pending.as_mut(), opt.trim().strip_prefix("http-user-agent=")) {
                entry.user_agent = Some(agent.trim().to_owned()).filter(|a| !a.is_empty());
            }
        } else if line.starts_with('#') {
            continue;
        } else if let Some(mut entry) = pending.take() {
            // `url|User-Agent=…`: Kodi's way of giving headers.
            let (url, headers) = line.split_once('|').unwrap_or((line, ""));
            entry.url = url.trim().to_owned();
            for header in headers.split('&') {
                if let Some((name, value)) = header.split_once('=')
                    && name.eq_ignore_ascii_case("user-agent")
                {
                    entry.user_agent = Some(value.to_owned());
                }
            }
            if !is_channel_url(&entry.url) {
                continue;
            }
            if entry.name.is_empty() {
                entry.name = entry.url.rsplit('/').next().unwrap_or_default().to_owned();
            }
            let base =
                if entry.guide_id.is_empty() { entry.url.clone() } else { format!("{}|{}", entry.guide_id, entry.url) };
            let mut key = base.clone();
            let mut n = 2;
            while !keys.insert(key.clone()) {
                key = format!("{base}#{n}");
                n += 1;
            }
            entry.key = key;
            playlist.entries.push(entry);
        }
    }
    playlist
}

/// Whether `url` is an http(s) address.
fn is_web_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

/// Whether `url` is a channel's stream: http(s), rtmp, rtsp or udp, and
/// not one of the films or series Xtream playlists list too.
fn is_channel_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let scheme_ok = ["http://", "https://"].iter().any(|s| lower.starts_with(s));
    if !scheme_ok {
        return false;
    }
    let path = lower.split(['?', '#']).next().unwrap_or_default();
    !(path.contains("/movie/") || path.contains("/series/"))
        && ![".mkv", ".mp4", ".avi", ".m4v", ".mov"].iter().any(|ext| path.ends_with(ext))
}

/// `-1 tvg-id="…" …,Name`.
fn extinf(info: &str) -> Entry {
    // The name follows the first comma outside quotes.
    let mut in_quotes = false;
    let mut split = None;
    for (i, c) in info.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let (head, name) = match split {
        Some(i) => (&info[..i], info[i + 1..].trim()),
        None => (info, ""),
    };
    let attrs = attributes(head);
    let get = |k: &str| attr(&attrs, k).unwrap_or_default();
    let name = if name.is_empty() { get("tvg-name") } else { name.to_owned() };
    Entry {
        name,
        number: attr(&attrs, "tvg-chno").or_else(|| attr(&attrs, "channel-number")).filter(|n| !n.is_empty()),
        group: get("group-title"),
        logo: attr(&attrs, "tvg-logo").or_else(|| attr(&attrs, "logo")).unwrap_or_default(),
        guide_id: get("tvg-id"),
        ..Entry::default()
    }
}

fn attr(attrs: &[(String, String)], key: &str) -> Option<String> {
    attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.trim().to_owned())
}

/// `key="value"` pairs (or `key=value`).
fn attributes(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].rsplit(|c: char| c.is_whitespace()).next().unwrap_or_default().to_owned();
        let after = &rest[eq + 1..];
        let (value, next) = if let Some(quoted) = after.strip_prefix('"') {
            match quoted.find('"') {
                Some(end) => (&quoted[..end], &quoted[end + 1..]),
                None => (quoted, ""),
            }
        } else {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            (&after[..end], &after[end..])
        };
        if !key.is_empty() {
            out.push((key, value.to_owned()));
        }
        rest = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_channels_and_their_attributes() {
        let text = "\u{feff}#EXTM3U url-tvg=\"https://e.com/a.xml.gz,https://e.com/b.xml\"\n\
            #EXTINF:-1 tvg-id=\"bbc1.uk\" tvg-chno=\"101\" tvg-logo=\"https://l/bbc1.png\" group-title=\"UK, Main\",BBC One HD\n\
            #EXTVLCOPT:http-user-agent=VLC/3.0\n\
            http://e.com/live/u/p/1.ts\n\
            #EXTINF:-1,Plain\n\
            #EXTGRP:News\n\
            https://e.com/plain.m3u8|User-Agent=Kodi\n\
            #EXTINF:-1 tvg-id=\"x\",A film\n\
            http://e.com/movie/u/p/9.mkv\n\
            #EXTINF:-1 tvg-name=\"Nameless\"\n\
            http://e.com/live/u/p/2.ts\n";
        let p = parse(text);
        assert_eq!(p.guide_url.as_deref(), Some("https://e.com/a.xml.gz"));
        assert_eq!(p.entries.len(), 3);
        let bbc = &p.entries[0];
        assert_eq!(
            (bbc.name.as_str(), bbc.number.as_deref(), bbc.group.as_str(), bbc.guide_id.as_str()),
            ("BBC One HD", Some("101"), "UK, Main", "bbc1.uk")
        );
        assert_eq!((bbc.logo.as_str(), bbc.user_agent.as_deref()), ("https://l/bbc1.png", Some("VLC/3.0")));
        assert_eq!(bbc.key, "bbc1.uk|http://e.com/live/u/p/1.ts");
        let plain = &p.entries[1];
        assert_eq!((plain.group.as_str(), plain.url.as_str()), ("News", "https://e.com/plain.m3u8"));
        assert_eq!(plain.user_agent.as_deref(), Some("Kodi"));
        assert_eq!(p.entries[2].name, "Nameless");
    }

    #[test]
    fn the_guide_a_playlist_names_is_a_web_address() {
        // Never a file of the provider's choosing, nor a share of its own.
        for named in [r"/etc/passwd", r"file:///etc/passwd", r"//evil.example/s/g.xml", r"\\evil.example\s\g.xml"] {
            assert_eq!(parse(&format!("#EXTM3U url-tvg=\"{named}\"\n")).guide_url, None, "{named}");
        }
        let p = parse("#EXTM3U x-tvg-url=\"file:///etc/passwd, https://e.com/g.xml\"\n");
        assert_eq!(p.guide_url.as_deref(), Some("https://e.com/g.xml"));
        let p = parse("#EXTM3U url-tvg=\"/etc/passwd\" x-tvg-url=\"https://e.com/g.xml\"\n");
        assert_eq!(p.guide_url.as_deref(), Some("https://e.com/g.xml"), "another key's web address");
    }

    #[test]
    fn the_same_entry_twice_gets_two_keys() {
        let text = "#EXTM3U\n#EXTINF:-1,A\nhttp://e.com/1.ts\n#EXTINF:-1,A again\nhttp://e.com/1.ts\n";
        let keys: Vec<String> = parse(text).entries.into_iter().map(|e| e.key).collect();
        assert_eq!(keys, ["http://e.com/1.ts", "http://e.com/1.ts#2"]);
    }
}
