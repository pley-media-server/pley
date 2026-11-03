//! Getting a new session's stream ready while the last one plays on
//! (packages/tv/src/player/prepare.ts): a quality, audio or Automatic change.
//! Its playlists and the segment it starts with are asked for, so the
//! transcoder makes them, and the TV's player moves to it once that segment
//! is there rather than waiting for it on a black screen.
//!
//! The TV's own player can't use what is fetched here (LG's pipeline has
//! HTTP of its own), so the segment is asked for with HEAD: the server
//! answers once it is made, and no picture crosses the network twice.

use std::time::{Duration, Instant};

use pley_core::http::Method;

use crate::server::PlayerServer;

/// How long a session prepared beside the one playing may take to get ready before the player moves to it anyway.
pub const PREPARE_MAX_MS: i64 = 20_000;
/// How far past where it starts a prepared session's first segment is asked for: where the player will be by then.
pub const PREPARE_LEAD_MS: i64 = 4_000;

/// `reference` against `base`, as a playlist names its playlists and segments.
pub fn resolve(reference: &str, base: &str) -> String {
    if reference.contains("://") {
        return reference.to_string();
    }
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let origin_end = base[scheme_end..].find('/').map_or(base.len(), |i| scheme_end + i);
    if let Some(rest) = reference.strip_prefix("//") {
        return format!("{}{rest}", &base[..scheme_end]);
    }
    let path = if reference.starts_with('/') {
        reference.to_string()
    } else {
        let base_path = &base[origin_end..];
        let base_path = base_path.split(['?', '#']).next().unwrap_or("");
        let dir = base_path.rfind('/').map_or("/", |i| &base_path[..=i]);
        format!("{dir}{reference}")
    };
    let (path, query) = match path.find(['?', '#']) {
        Some(i) => (path[..i].to_string(), path[i..].to_string()),
        None => (path, String::new()),
    };
    let mut parts: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path.split('/').collect();
    for (i, seg) in segments.iter().enumerate() {
        match *seg {
            "." => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            s if s.is_empty() && i != 0 && i != segments.len() - 1 => {}
            s => parts.push(s),
        }
    }
    format!("{}{}{query}", &base[..origin_end], parts.join("/"))
}

/// The sizes a multivariant playlist lists, in its order; none for a media playlist.
pub fn variants_of(master: &str, base: &str) -> Vec<String> {
    let lines: Vec<&str> = master.lines().map(str::trim).collect();
    let mut found = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with("#EXT-X-STREAM-INF") {
            continue;
        }
        if let Some(uri) = lines[i + 1..].iter().find(|l| !l.is_empty() && !l.starts_with('#')) {
            found.push(resolve(uri, base));
        }
    }
    found
}

/// The segment of a media playlist that holds `ms`: the last one for a time past its end; none for none.
pub fn segment_at(playlist: &str, base: &str, ms: i64) -> Option<String> {
    let mut at = 0.0;
    let mut duration: Option<f64> = None;
    let mut last = None;
    for line in playlist.lines().map(str::trim) {
        if let Some(d) = line.strip_prefix("#EXTINF:") {
            let number: String = d.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
            duration = number.parse::<f64>().ok().map(|s| s * 1000.0);
        } else if !line.is_empty() && !line.starts_with('#') {
            let Some(d) = duration.take() else { continue };
            let url = resolve(line, base);
            if (ms as f64) < at + d {
                return Some(url);
            }
            at += d;
            last = Some(url);
        }
    }
    last
}

/// Asks for the stream at `url` as the TV's player starting at `start_ms`
/// would: its playlist, the size it lists first of a stream in several
/// (where the TV's player starts), and the segment that holds `start_ms`.
/// Returns once that segment is made, after [`PREPARE_MAX_MS`] at most, or
/// when anything fails: the player moves anyway, and finds out for itself.
/// Run on a task, whose cancelling ends it at once.
pub fn warm_stream(server: &dyn PlayerServer, url: &str, start_ms: i64) {
    let until = Instant::now() + Duration::from_millis(PREPARE_MAX_MS as u64);
    let left = || until.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
    let run = || -> Option<()> {
        let mut playlist_url = url.to_string();
        let mut playlist = server.media(Method::Get, &playlist_url, left()).ok()?;
        if let Some(first) = variants_of(&playlist, &playlist_url).into_iter().next() {
            playlist_url = first;
            playlist = server.media(Method::Get, &playlist_url, left()).ok()?;
        }
        let segment = segment_at(&playlist, &playlist_url, start_ms)?;
        server.media(Method::Head, &segment, left()).ok().map(drop)
    };
    run();
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://nas:7539/api/media/pley_mk_KEY/master.m3u8";

    #[test]
    fn a_streams_playlists_list_their_sizes_in_order() {
        let master = [
            "#EXTM3U",
            "#EXT-X-STREAM-INF:BANDWIDTH=8000000,RESOLUTION=1920x1080",
            "v0/index.m3u8",
            "",
            "#EXT-X-STREAM-INF:BANDWIDTH=4000000",
            "v1/index.m3u8",
        ]
        .join("\r\n");
        assert_eq!(
            variants_of(&master, BASE),
            [
                "http://nas:7539/api/media/pley_mk_KEY/v0/index.m3u8",
                "http://nas:7539/api/media/pley_mk_KEY/v1/index.m3u8"
            ]
        );
        assert!(variants_of("#EXTM3U\n#EXTINF:4,\n0.ts", BASE).is_empty());
    }

    #[test]
    fn name_the_segment_holding_a_time_the_last_for_one_past_the_end() {
        let media =
            ["#EXTM3U", "#EXTINF:4.000,", "0.ts", "#EXTINF:4.000,", "1.ts", "#EXTINF:2.5,", "2.ts", "#EXT-X-ENDLIST"]
                .join("\n");
        let at = |ms| segment_at(&media, BASE, ms);
        assert_eq!(at(0).as_deref(), Some("http://nas:7539/api/media/pley_mk_KEY/0.ts"));
        assert_eq!(at(4_000).as_deref(), Some("http://nas:7539/api/media/pley_mk_KEY/1.ts"));
        assert_eq!(at(9_000).as_deref(), Some("http://nas:7539/api/media/pley_mk_KEY/2.ts"));
        assert_eq!(at(60_000).as_deref(), Some("http://nas:7539/api/media/pley_mk_KEY/2.ts"));
        assert_eq!(segment_at("#EXTM3U", BASE, 0), None);
    }

    #[test]
    fn resolves_like_a_browser() {
        assert_eq!(resolve("/a/b.ts", BASE), "http://nas:7539/a/b.ts");
        assert_eq!(resolve("../x/i.m3u8", BASE), "http://nas:7539/api/media/x/i.m3u8");
        assert_eq!(resolve("https://cdn/x.ts", BASE), "https://cdn/x.ts");
        assert_eq!(resolve("0.ts?k=1", "http://h/p/index.m3u8?v=2"), "http://h/p/0.ts?k=1");
    }
}
