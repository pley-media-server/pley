//! How a library is listed (its sort, order and filters, the "view"), and the words around it:
//! sort names, counts, smart filters in Plex's syntax and in words. Ported from the TV app's
//! `library-data.ts` and the Android app's `LibraryParams.kt` and `SmartFilter.kt`.

use pley_core::format::grouped;
use pley_core::model::{LibraryKind, LibraryQuery, LibrarySort, RangeFilter, ResolutionFilter, SortOrder};
use pley_shell::LibraryTab;

use crate::tiles::Quality;

/// Every sort, in the order the Sort panel lists them (`LibraryParams.kt:89-99`).
const ALL_SORTS: [LibrarySort; 9] = [
    LibrarySort::Title,
    LibrarySort::Year,
    LibrarySort::Added,
    LibrarySort::Rating,
    LibrarySort::UserRating,
    LibrarySort::Release,
    LibrarySort::Duration,
    LibrarySort::LastPlayed,
    LibrarySort::Bitrate,
];
const SHOW_SORTS: [LibrarySort; 8] = [
    LibrarySort::Title,
    LibrarySort::Year,
    LibrarySort::Added,
    LibrarySort::Rating,
    LibrarySort::UserRating,
    LibrarySort::Release,
    LibrarySort::LastPlayed,
    LibrarySort::Bitrate,
];
const MUSIC_SORTS: [LibrarySort; 4] =
    [LibrarySort::Title, LibrarySort::Added, LibrarySort::UserRating, LibrarySort::LastPlayed];

/// The sorts a library of `kind` offers: shows have no Duration, music only four.
pub fn sorts_for(kind: LibraryKind) -> &'static [LibrarySort] {
    match kind {
        LibraryKind::Music => &MUSIC_SORTS,
        LibraryKind::Shows => &SHOW_SORTS,
        _ => &ALL_SORTS,
    }
}

/// A sort's name on its chip and in the Sort panel.
pub fn sort_label(sort: LibrarySort) -> &'static str {
    match sort {
        LibrarySort::Title => "Title",
        LibrarySort::Year => "Year",
        LibrarySort::Added => "Date added",
        LibrarySort::Rating => "Rating",
        LibrarySort::UserRating => "Your rating",
        LibrarySort::Release => "Release date",
        LibrarySort::Duration => "Duration",
        LibrarySort::LastPlayed => "Last played",
        LibrarySort::Bitrate => "Bitrate",
    }
}

/// The order a sort takes when it is picked: titles A to Z, the rest newest or highest first.
pub fn default_order(sort: LibrarySort) -> SortOrder {
    if sort == LibrarySort::Title { SortOrder::Asc } else { SortOrder::Desc }
}

/// The order `query` lists in.
pub fn order_of(query: &LibraryQuery) -> SortOrder {
    query.order.unwrap_or(default_order(query.sort))
}

/// A view fit for a library of `kind`: a sort it doesn't offer becomes Title, the order is always
/// said (so the query string is the same every time, for the server's ETag), a resolution or
/// dynamic range this app doesn't know is dropped, and music has no filters.
pub fn fit(query: LibraryQuery, kind: LibraryKind) -> LibraryQuery {
    let sort = if sorts_for(kind).contains(&query.sort) { query.sort } else { LibrarySort::Title };
    let order = Some(query.order.filter(|_| sort == query.sort).unwrap_or(default_order(sort)));
    if kind == LibraryKind::Music {
        return LibraryQuery { sort, order, ..LibraryQuery::default() };
    }
    LibraryQuery {
        sort,
        order,
        watched: query.watched,
        genre: query.genre.filter(|g| !g.is_empty()),
        year: query.year,
        resolution: query.resolution.filter(|r| *r != ResolutionFilter::Unknown),
        dynamic_range: query.dynamic_range.filter(|r| *r != RangeFilter::Unknown),
    }
}

/// The view a library opens in by default: by title, A to Z, unfiltered.
pub fn default_view() -> LibraryQuery {
    LibraryQuery { order: Some(SortOrder::Asc), ..LibraryQuery::default() }
}

/// The view a row's See all opens: that row's order, unfiltered.
pub fn see_all_view(sort: LibrarySort, kind: LibraryKind) -> LibraryQuery {
    fit(LibraryQuery { sort, order: Some(default_order(sort)), ..LibraryQuery::default() }, kind)
}

/// Whether the view filters the library (watched, genre, year, resolution or dynamic range).
pub fn filtered(query: &LibraryQuery) -> bool {
    query.watched.is_some()
        || query.genre.is_some()
        || query.year.is_some()
        || query.resolution.is_some()
        || query.dynamic_range.is_some()
}

/// The view without its filters.
pub fn unfiltered(query: &LibraryQuery) -> LibraryQuery {
    LibraryQuery { watched: None, genre: None, year: None, resolution: None, dynamic_range: None, ..query.clone() }
}

/// A resolution as its chip and the Resolution panel name it.
pub fn resolution_label(resolution: ResolutionFilter) -> &'static str {
    match resolution {
        ResolutionFilter::Uhd => "4K",
        ResolutionFilter::Fhd => "1080p",
        ResolutionFilter::Hd => "720p",
        ResolutionFilter::Sd => "SD",
        ResolutionFilter::Unknown => "",
    }
}

/// A dynamic range as its chip and the Dynamic range panel name it.
pub fn range_label(range: RangeFilter) -> &'static str {
    match range {
        RangeFilter::Hdr => "HDR",
        RangeFilter::DolbyVision => "Dolby Vision",
        RangeFilter::Hdr10Plus => "HDR10+",
        RangeFilter::Hdr10 => "HDR10",
        RangeFilter::Hlg => "HLG",
        RangeFilter::Sdr => "SDR",
        RangeFilter::Unknown => "",
    }
}

/// What the views ask for of the grid's quality badges and bitrate line: the badges sorted by
/// bitrate or filtered by resolution or dynamic range, where they say why a title is there; the
/// bitrate line sorted by bitrate.
pub fn quality_of(query: &LibraryQuery) -> Quality {
    let bitrate = query.sort == LibrarySort::Bitrate;
    Quality { badges: bitrate || query.resolution.is_some() || query.dynamic_range.is_some(), bitrate }
}

/// The tabs a library of `kind` has: music has no Collections.
pub fn tabs_for(kind: LibraryKind) -> &'static [LibraryTab] {
    const ALL: [LibraryTab; 3] = [LibraryTab::Recommended, LibraryTab::Library, LibraryTab::Collections];
    if kind == LibraryKind::Music { &ALL[..2] } else { &ALL }
}

/// A tab's name.
pub fn tab_label(tab: LibraryTab) -> &'static str {
    match tab {
        LibraryTab::Recommended => "Recommended",
        LibraryTab::Library => "Library",
        LibraryTab::Collections => "Collections",
    }
}

/// The tab a library opens on: the one asked for (See all asks for Library), else the one it was
/// left on, else Library; one the library hasn't is Library.
pub fn opening_tab(tabs: &[LibraryTab], asked: Option<LibraryTab>, kept: Option<LibraryTab>) -> LibraryTab {
    let wanted = asked.or(kept).unwrap_or(LibraryTab::Library);
    if tabs.contains(&wanted) { wanted } else { LibraryTab::Library }
}

/// The sort a Recommended row's See all opens the Library tab in (the TV app's `seeAllSort`):
/// recently added, recently released and top rated rows have one.
pub fn see_all_sort(hub_key: &str) -> Option<LibrarySort> {
    if hub_key.starts_with("recently_added") {
        Some(LibrarySort::Added)
    } else if hub_key.starts_with("recently_released") {
        Some(LibrarySort::Release)
    } else if hub_key.starts_with("top_rated") {
        Some(LibrarySort::Rating)
    } else {
        None
    }
}

/// The count beside the chips: "1,500 movies", "1 show", "12 artists"; filtered, "23 of 1,500
/// movies" (the plurals of `strings_tv_library.xml`, which follow the larger number).
pub fn count_label(n: i64, kind: LibraryKind, of: Option<i64>) -> String {
    let m = of.unwrap_or(n);
    let noun = match (kind, m == 1) {
        (LibraryKind::Movies, true) => "movie",
        (LibraryKind::Movies, false) => "movies",
        (LibraryKind::Shows, true) => "show",
        (LibraryKind::Shows, false) => "shows",
        (_, true) => "artist",
        (_, false) => "artists",
    };
    match of {
        Some(of) => format!("{} of {} {noun}", grouped(n), grouped(of)),
        None => format!("{} {noun}", grouped(n)),
    }
}

/// "12 collections", "1 collection".
pub fn collections_label(n: usize) -> String {
    format!("{} {}", grouped(n as i64), if n == 1 { "collection" } else { "collections" })
}

/// A collection's size: "12 movies", or "3 shows" in a library of shows.
pub fn collection_count(count: i64, kind: Option<LibraryKind>) -> String {
    let noun = match (kind, count == 1) {
        (Some(LibraryKind::Shows), true) => "show",
        (Some(LibraryKind::Shows), false) => "shows",
        (_, true) => "movie",
        (_, false) => "movies",
    };
    format!("{} {noun}", grouped(count))
}

/// Plex's name of each sort, as smart filters write it (docs/COMPATIBILITY.md).
fn plex_sort(sort: LibrarySort) -> &'static str {
    match sort {
        LibrarySort::Title => "titleSort",
        LibrarySort::Year => "year",
        LibrarySort::Added => "addedAt",
        LibrarySort::Rating => "rating",
        LibrarySort::UserRating => "userRating",
        LibrarySort::Release => "originallyAvailableAt",
        LibrarySort::Duration => "duration",
        LibrarySort::LastPlayed => "lastViewedAt",
        LibrarySort::Bitrate => "mediaBitrate",
    }
}

/// The view's filters and order as a smart filter in Plex's syntax, each part form-encoded
/// (`genre=Comedy&resolution=4k&unwatched=1&sort=year%3Adesc`): what "Save as…" saves
/// (`SmartFilter.of`). Resolution is Plex's own field, `dynamicRange` the server's.
pub fn smart_filter_of(query: &LibraryQuery) -> String {
    let mut parts: Vec<(&str, String)> = Vec::new();
    if let Some(g) = &query.genre {
        parts.push(("genre", g.clone()));
    }
    if let Some(y) = query.year {
        parts.push(("year", y.to_string()));
    }
    if let Some(r) = query.resolution {
        parts.push(("resolution", r.as_str().to_string()));
    }
    if let Some(r) = query.dynamic_range {
        parts.push(("dynamicRange", r.as_str().to_string()));
    }
    if let Some(w) = query.watched {
        parts.push(("unwatched", if w { "0" } else { "1" }.to_string()));
    }
    let field = plex_sort(query.sort);
    let sort = if order_of(query) == SortOrder::Desc { format!("{field}:desc") } else { field.to_string() };
    parts.push(("sort", sort));
    parts.iter().map(|(k, v)| format!("{}={}", encode_form(k), encode_form(v))).collect::<Vec<_>>().join("&")
}

/// A form value as Java's `URLEncoder` writes it (the Android app's `LibraryParams.encode`):
/// letters, digits and `.-*_` as they are, spaces as `+`, the rest as `%XX` of its UTF-8 bytes.
pub fn encode_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'*' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A form value read back: `+` as a space, `%XX` as its byte; a broken escape is kept as written.
pub fn decode_form(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push(hi * 16 + lo);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// A smart filter's sort in words ("Newest first", "By title"), from its first field.
fn sort_note(value: &str) -> String {
    let first = value.split(',').next().unwrap_or("");
    let (field, direction) = match first.split_once(':') {
        Some((f, d)) => (f, Some(d)),
        None => (first, None),
    };
    // python-plexapi may name the kind first: `movie.year`.
    let name = field.split_once('.').map_or(field, |(_, n)| n);
    let notes = match name {
        "titleSort" => ("By title", "By title, Z to A"),
        "year" => ("Oldest first", "Newest first"),
        "originallyAvailableAt" => ("Earliest release first", "Latest release first"),
        "addedAt" => ("First added first", "Recently added first"),
        "rating" | "audienceRating" => ("Lowest rated first", "Best rated first"),
        "userRating" => ("Your lowest rated first", "Your best rated first"),
        "lastViewedAt" => ("Least recently played first", "Recently played first"),
        "duration" => ("Shortest first", "Longest first"),
        "viewCount" => ("Least played first", "Most played first"),
        _ => return format!("Sorted by {name}"),
    };
    if direction == Some("desc") { notes.1 } else { notes.0 }.to_string()
}

/// A smart filter's resolution as the Resolution panel names it, Plex's own values too (`2160`,
/// `1080p`, and `576` and `480`, which are SD).
fn resolution_words(value: &str) -> Option<&'static str> {
    let value = value.trim().to_ascii_lowercase();
    let resolution = match value.strip_suffix('p').unwrap_or(&value) {
        "2160" => ResolutionFilter::Uhd,
        "576" | "480" => ResolutionFilter::Sd,
        other => ResolutionFilter::from_wire(other),
    };
    Some(resolution_label(resolution)).filter(|l| !l.is_empty())
}

/// A smart filter's dynamic range as the Dynamic range panel names it.
fn range_words(value: &str) -> Option<&'static str> {
    Some(range_label(RangeFilter::from_wire(&value.trim().to_ascii_lowercase()))).filter(|l| !l.is_empty())
}

/// What a smart filter finds, in words: "Genre: Comedy", "Unwatched", "From 1999", "4K", "Dolby
/// Vision", "Newest first"… (`SmartFilter.describe`).
pub fn describe_smart_filter(filter: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in filter.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = match part.split_once('=') {
            Some((k, v)) => (decode_form(k), decode_form(v)),
            None => (decode_form(part), String::new()),
        };
        let off = value == "0" || value == "false";
        match key.as_str() {
            "genre" => out.push(format!("Genre: {value}")),
            "label" => out.push(format!("Label: {value}")),
            "studio" => out.push(format!("Studio: {value}")),
            "contentRating" => out.push(format!("Rated {value}")),
            "unwatched" => out.push(if off { "Watched" } else { "Unwatched" }.to_string()),
            "inProgress" => out.push(if off { "Not in progress" } else { "In progress" }.to_string()),
            "year" => out.push(format!("From {value}")),
            "year>>" => out.push(format!("After {value}")),
            "year<<" => out.push(format!("Before {value}")),
            "decade" => out.push(format!("The {value}s")),
            "resolution" => out.push(resolution_words(&value).map_or_else(|| format!("{key}: {value}"), String::from)),
            "dynamicRange" => out.push(range_words(&value).map_or_else(|| format!("{key}: {value}"), String::from)),
            "sort" => out.push(sort_note(&value)),
            "limit" => out.push(format!("At most {value}")),
            // Plumbing, not part of what is found.
            "type" | "includeGuids" => {}
            _ => out.push(format!("{key}: {value}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(sort: LibrarySort) -> LibraryQuery {
        LibraryQuery { sort, ..LibraryQuery::default() }
    }

    #[test]
    fn each_kind_offers_its_sorts() {
        assert_eq!(sorts_for(LibraryKind::Movies).len(), 9);
        assert!(!sorts_for(LibraryKind::Shows).contains(&LibrarySort::Duration));
        assert_eq!(
            sorts_for(LibraryKind::Music),
            &[LibrarySort::Title, LibrarySort::Added, LibrarySort::UserRating, LibrarySort::LastPlayed]
        );
        assert_eq!(sort_label(LibrarySort::UserRating), "Your rating");
    }

    #[test]
    fn a_kept_view_is_fit_for_its_library() {
        // A sort the kind hasn't becomes Title, A to Z; music keeps no filters.
        let kept =
            LibraryQuery { sort: LibrarySort::Duration, order: Some(SortOrder::Desc), ..view(LibrarySort::Title) };
        assert_eq!(fit(kept, LibraryKind::Shows), default_view());
        let music = LibraryQuery { watched: Some(false), genre: Some("Jazz".into()), ..view(LibrarySort::Added) };
        assert_eq!(
            fit(music, LibraryKind::Music),
            LibraryQuery { sort: LibrarySort::Added, order: Some(SortOrder::Desc), ..LibraryQuery::default() }
        );
        // The order is always said: Title ascending, the rest descending.
        assert_eq!(fit(view(LibrarySort::Year), LibraryKind::Movies).order, Some(SortOrder::Desc));
        assert_eq!(fit(view(LibrarySort::Title), LibraryKind::Movies).order, Some(SortOrder::Asc));
        let empty_genre = LibraryQuery { genre: Some(String::new()), ..view(LibrarySort::Title) };
        assert_eq!(fit(empty_genre, LibraryKind::Movies).genre, None);
        assert_eq!(see_all_view(LibrarySort::Added, LibraryKind::Movies).order, Some(SortOrder::Desc));
    }

    #[test]
    fn libraries_open_on_the_tab_asked_then_kept_then_library() {
        let movies = tabs_for(LibraryKind::Movies);
        let music = tabs_for(LibraryKind::Music);
        assert_eq!(opening_tab(movies, None, None), LibraryTab::Library);
        assert_eq!(opening_tab(movies, None, Some(LibraryTab::Collections)), LibraryTab::Collections);
        assert_eq!(opening_tab(movies, Some(LibraryTab::Library), Some(LibraryTab::Recommended)), LibraryTab::Library);
        assert_eq!(opening_tab(music, None, Some(LibraryTab::Collections)), LibraryTab::Library);
        assert_eq!(see_all_sort("recently_added:1"), Some(LibrarySort::Added));
        assert_eq!(see_all_sort("top_rated:1"), Some(LibrarySort::Rating));
        assert_eq!(see_all_sort("continue_watching:1"), None);
    }

    #[test]
    fn counts_read_as_on_android() {
        assert_eq!(count_label(1500, LibraryKind::Movies, None), "1,500 movies");
        assert_eq!(count_label(1, LibraryKind::Shows, None), "1 show");
        assert_eq!(count_label(23, LibraryKind::Movies, Some(1500)), "23 of 1,500 movies");
        assert_eq!(count_label(12, LibraryKind::Music, None), "12 artists");
        assert_eq!(collections_label(1), "1 collection");
        assert_eq!(collections_label(12), "12 collections");
        assert_eq!(collection_count(3, Some(LibraryKind::Shows)), "3 shows");
        assert_eq!(collection_count(1, Some(LibraryKind::Movies)), "1 movie");
    }

    #[test]
    fn save_as_writes_plex_syntax() {
        let q = LibraryQuery {
            sort: LibrarySort::Year,
            order: Some(SortOrder::Desc),
            watched: Some(false),
            genre: Some("Science Fiction".into()),
            year: Some(2010),
            ..LibraryQuery::default()
        };
        // Java's URLEncoder, as Android writes it, escapes the colon too.
        assert_eq!(smart_filter_of(&q), "genre=Science+Fiction&year=2010&unwatched=1&sort=year%3Adesc");
        // Resolution (Plex's field) and dynamic range (the server's) after the year.
        let q = LibraryQuery { resolution: Some(ResolutionFilter::Uhd), dynamic_range: Some(RangeFilter::Hdr), ..q };
        assert_eq!(
            smart_filter_of(&q),
            "genre=Science+Fiction&year=2010&resolution=4k&dynamicRange=hdr&unwatched=1&sort=year%3Adesc"
        );
        let range = LibraryQuery { dynamic_range: Some(RangeFilter::DolbyVision), ..default_view() };
        assert_eq!(smart_filter_of(&range), "dynamicRange=dolby_vision&sort=titleSort");
        assert_eq!(smart_filter_of(&default_view()), "sort=titleSort");
        let watched = LibraryQuery { watched: Some(true), ..default_view() };
        assert_eq!(smart_filter_of(&watched), "unwatched=0&sort=titleSort");
        assert_eq!(encode_form("Rock & Roll"), "Rock+%26+Roll");
        assert_eq!(decode_form("Rock+%26+Roll"), "Rock & Roll");
        assert_eq!(decode_form("100%"), "100%");
        assert_eq!(decode_form("Caf%C3%A9"), "Café");
    }

    #[test]
    fn smart_filters_read_in_words() {
        assert_eq!(
            describe_smart_filter("genre=Comedy&unwatched=1&sort=year:desc"),
            vec!["Genre: Comedy", "Unwatched", "Newest first"]
        );
        assert_eq!(describe_smart_filter("year=1999&type=1&sort=titleSort"), vec!["From 1999", "By title"]);
        assert_eq!(describe_smart_filter("sort=movie.rating:desc"), vec!["Best rated first"]);
        assert_eq!(describe_smart_filter("decade=1990&unwatched=0"), vec!["The 1990s", "Watched"]);
        assert_eq!(describe_smart_filter("sort=mediaBitrate"), vec!["Sorted by mediaBitrate"]);
        assert_eq!(describe_smart_filter("studio=A24&limit=10"), vec!["Studio: A24", "At most 10"]);
        assert!(describe_smart_filter("").is_empty());
        // Resolution and dynamic range by the labels of their panels, Plex's resolutions too.
        assert_eq!(
            describe_smart_filter("resolution=4k&dynamicRange=dolby_vision&unwatched=1"),
            vec!["4K", "Dolby Vision", "Unwatched"]
        );
        assert_eq!(describe_smart_filter("resolution=1080p&dynamicRange=hdr"), vec!["1080p", "HDR"]);
        assert_eq!(describe_smart_filter("resolution=480&dynamicRange=hdr10plus"), vec!["SD", "HDR10+"]);
        assert_eq!(describe_smart_filter("resolution=8k"), vec!["resolution: 8k"]);
    }

    #[test]
    fn resolution_and_dynamic_range_filter_like_genre_and_year() {
        let q = LibraryQuery { resolution: Some(ResolutionFilter::Hd), ..default_view() };
        assert!(filtered(&q));
        assert!(filtered(&LibraryQuery { dynamic_range: Some(RangeFilter::Sdr), ..default_view() }));
        let both = LibraryQuery { dynamic_range: Some(RangeFilter::Hlg), ..q.clone() };
        assert_eq!(unfiltered(&both), default_view());
        // Kept as they are; a value this app doesn't know is dropped; music keeps neither.
        assert_eq!(fit(both.clone(), LibraryKind::Shows), both);
        let odd = LibraryQuery {
            resolution: Some(ResolutionFilter::Unknown),
            dynamic_range: Some(RangeFilter::Unknown),
            ..default_view()
        };
        assert_eq!(fit(odd, LibraryKind::Movies), default_view());
        assert_eq!(fit(both, LibraryKind::Music), default_view());
        // Chosen values are named by their labels, never their tokens.
        let labels: Vec<&str> = ResolutionFilter::ALL.iter().map(|r| resolution_label(*r)).collect();
        assert_eq!(labels, ["4K", "1080p", "720p", "SD", ""]);
        let labels: Vec<&str> = RangeFilter::ALL.iter().map(|r| range_label(*r)).collect();
        assert_eq!(labels, ["HDR", "Dolby Vision", "HDR10+", "HDR10", "HLG", "SDR", ""]);
    }

    #[test]
    fn badges_show_where_they_say_why_a_title_is_there() {
        let by = |sort| LibraryQuery { sort, ..default_view() };
        assert_eq!(quality_of(&by(LibrarySort::Title)), Quality { badges: false, bitrate: false });
        assert_eq!(quality_of(&by(LibrarySort::Bitrate)), Quality { badges: true, bitrate: true });
        let res = LibraryQuery { resolution: Some(ResolutionFilter::Uhd), ..by(LibrarySort::Added) };
        assert_eq!(quality_of(&res), Quality { badges: true, bitrate: false });
        let range = LibraryQuery { dynamic_range: Some(RangeFilter::Hdr), ..by(LibrarySort::Title) };
        assert_eq!(quality_of(&range), Quality { badges: true, bitrate: false });
        // Other filters say nothing of quality.
        let other = LibraryQuery {
            genre: Some("Drama".into()),
            year: Some(1999),
            watched: Some(false),
            ..by(LibrarySort::Year)
        };
        assert_eq!(quality_of(&other), Quality::NONE);
    }
}
