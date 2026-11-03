//! Live TV's rules that draw nothing (packages/tv/src/live/data.ts; docs/API.md,
//! "Live TV"): what's on, the categories, changing channels by number, and
//! the guide's chunks and cells. Times are unix seconds; the TV's offset
//! from UTC (seconds) turns them into its own clock and days.

use std::collections::{BTreeMap, HashMap};

use pley_core::api::LiveBrowse;
use pley_core::format::format_time_of_day;
use pley_core::model::{LiveCategory, LiveChannel, LiveGroup, LiveGuide, LiveProgram};

/// The guide's columns: half an hour each.
pub const SLOT: i64 = 1800;
/// The guide is asked for in chunks of six hours, as it comes into view.
pub const GUIDE_CHUNK: i64 = 6 * 3600;
/// How far ahead the guide goes from where it opens.
pub const GUIDE_SPAN: i64 = 48 * 3600;
/// A gap in a channel's guide shorter than this belongs to the programme before it.
const MIN_GAP: i64 = 60;
/// Channels asked for at once.
pub const CHANNEL_PAGE: i64 = 100;

/// Unix seconds now.
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// What's on a channel and what comes next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NowAndNext {
    pub on: Option<LiveProgram>,
    pub next: Option<LiveProgram>,
}

/// What's on a channel at `now`, and what comes next. The channel list is
/// asked for again every minute, so its `now` may have just ended: its
/// `next` is on then.
pub fn now_and_next(c: &LiveChannel, now: i64) -> NowAndNext {
    if let Some(on) = c.now.as_ref().filter(|p| p.airing_at(now)) {
        return NowAndNext { on: Some(on.clone()), next: c.next.clone() };
    }
    if let Some(on) = c.next.as_ref().filter(|p| p.airing_at(now)) {
        return NowAndNext { on: Some(on.clone()), next: None };
    }
    let next = [&c.now, &c.next].into_iter().flatten().find(|p| p.start > now).cloned();
    NowAndNext { on: None, next }
}

/// How far a programme is at `now`, 0–1.
pub fn program_progress(p: &LiveProgram, now: i64) -> f32 {
    p.progress_at(now)
}

/// "12 min left", "1 h 5 min left".
pub fn time_left(stop: i64, now: i64) -> String {
    let minutes = (((stop - now) as f64 / 60.0).round() as i64).max(1);
    if minutes < 60 {
        return format!("{minutes} min left");
    }
    let (h, m) = (minutes / 60, minutes % 60);
    if m > 0 { format!("{h} h {m} min left") } else { format!("{h} h left") }
}

/// "20:00" or "8:00 PM", as the TV writes the time; `utc_offset` is the TV's, in seconds.
pub fn format_time(ts: i64, hours24: bool, utc_offset: i64) -> String {
    let secs = (ts + utc_offset).rem_euclid(86_400);
    format_time_of_day((secs / 3600) as u32, ((secs % 3600) / 60) as u32, hours24)
}

/// "20:00 – 21:00".
pub fn format_span(p: &LiveProgram, hours24: bool, utc_offset: i64) -> String {
    format!("{} – {}", format_time(p.start, hours24, utc_offset), format_time(p.stop, hours24, utc_offset))
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Days since 1970-01-01 as (year, month 1–12, day 1–31) (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// "Today", "Tomorrow", "Yesterday", or "Fri 30 Oct", in the TV's own days.
pub fn format_guide_day(ts: i64, now: i64, utc_offset: i64) -> String {
    let day = |t: i64| (t + utc_offset).div_euclid(86_400);
    let (d, today) = (day(ts), day(now));
    if d == today {
        return "Today".into();
    }
    if d == today + 1 {
        return "Tomorrow".into();
    }
    if d == today - 1 {
        return "Yesterday".into();
    }
    let (_, month, date) = civil_from_days(d);
    // 1970-01-01 was a Thursday.
    let weekday = (d + 4).rem_euclid(7) as usize;
    format!("{} {date} {}", DAYS[weekday], MONTHS[month as usize - 1])
}

/// "S2 E14", when the guide numbers the episode.
pub fn episode_text(p: &LiveProgram) -> Option<String> {
    p.episode_label()
}

/// "101 Pley News HD", or the name alone.
pub fn channel_title(c: &LiveChannel) -> String {
    match c.number.as_deref().filter(|n| !n.is_empty()) {
        Some(n) => format!("{n} {}", c.name),
        None => c.name.clone(),
    }
}

// ---- categories

/// Which channels a list has: the marked ones, every one, a group's, or every
/// channel whose name, number or what's on has the words in it (the one
/// search, whatever category was open).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Category {
    #[default]
    All,
    Favorites,
    Recent,
    Group(String),
    Search(String),
}

impl Category {
    /// `all`, `favorites`, `recent`, `group:<name>` or `search:<words>`: kept on the TV and in a page's key.
    pub fn key(&self) -> String {
        match self {
            Category::All => "all".into(),
            Category::Favorites => "favorites".into(),
            Category::Recent => "recent".into(),
            Category::Group(g) => format!("group:{g}"),
            Category::Search(q) => format!("search:{q}"),
        }
    }

    /// The category kept as `key`; anything else is All.
    pub fn parse(key: Option<&str>) -> Category {
        match key {
            Some("favorites") => Category::Favorites,
            Some("recent") => Category::Recent,
            Some(k) if k.starts_with("group:") => Category::Group(k[6..].to_string()),
            Some(k) if k.starts_with("search:") && k.len() > 7 => Category::Search(k[7..].to_string()),
            _ => Category::All,
        }
    }

    /// What `GET /api/live/browse` is asked for it: a search among every channel.
    pub fn browse(&self, offset: i64, limit: i64, around: Option<i64>) -> LiveBrowse {
        let (category, q) = match self {
            Category::All | Category::Search(_) => (LiveCategory::All, None),
            Category::Favorites => (LiveCategory::Favorites, None),
            Category::Recent => (LiveCategory::Recent, None),
            Category::Group(g) => (LiveCategory::Group(g.clone()), None),
        };
        let q = match self {
            Category::Search(words) => Some(words.clone()),
            _ => q,
        };
        LiveBrowse { category, q, offset, limit, around }
    }
}

/// A region's groups, gathered.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub region: String,
    pub groups: Vec<LiveGroup>,
    pub count: i64,
}

/// The groups as the categories list them: a provider naming hundreds of
/// groups by region ("UK| Sport", "UK| News"…) has them gathered under each
/// region, the regions in alphabetical order, and the rest after, in its own
/// order; a region with one group is one of the rest. With fewer than two
/// regions, the groups as the provider lists them.
pub fn group_sections(groups: &[LiveGroup]) -> (Vec<Region>, Vec<LiveGroup>) {
    let mut by_region: HashMap<&str, Vec<LiveGroup>> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for g in groups {
        let Some(r) = g.region.as_deref().filter(|r| !r.is_empty()) else { continue };
        if !by_region.contains_key(r) {
            order.push(r);
        }
        by_region.entry(r).or_default().push(g.clone());
    }
    let mut regions: Vec<Region> = order
        .into_iter()
        .filter(|r| by_region[r].len() > 1)
        .map(|r| Region {
            region: r.to_string(),
            count: by_region[r].iter().map(|g| g.count).sum(),
            groups: by_region[r].clone(),
        })
        .collect();
    regions.sort_by(|a, b| a.region.to_lowercase().cmp(&b.region.to_lowercase()).then(a.region.cmp(&b.region)));
    if regions.len() < 2 {
        return (Vec::new(), groups.to_vec());
    }
    let others = groups
        .iter()
        .filter(|g| g.region.as_deref().is_none_or(|r| r.is_empty() || !regions.iter().any(|x| x.region == r)))
        .cloned()
        .collect();
    (regions, others)
}

/// The groups whose names have `q` in them, for a search's quick links.
pub fn groups_matching(groups: &[LiveGroup], q: &str, max: usize) -> Vec<LiveGroup> {
    let s = q.trim().to_lowercase();
    if s.is_empty() {
        return Vec::new();
    }
    groups.iter().filter(|g| g.name.to_lowercase().contains(&s)).take(max).cloned().collect()
}

/// What a category is called: "Favourites", "UK · Sport", "Results for “bbc”".
pub fn category_name(c: &Category, groups: &[LiveGroup]) -> String {
    match c {
        Category::All => "All channels".into(),
        Category::Favorites => "Favourites".into(),
        Category::Recent => "Recently watched".into(),
        Category::Search(q) => format!("Results for “{q}”"),
        Category::Group(name) => match groups.iter().find(|g| g.name == *name) {
            Some(g) => match g.region.as_deref().filter(|r| !r.is_empty()) {
                Some(r) => format!("{r} · {}", g.label),
                None => g.label.clone(),
            },
            None => name.clone(),
        },
    }
}

/// A name's region and the rest, when it begins with one the way providers
/// write them (`UK| Sport`, `[UK] Sport`, `UK: Sport`), as the server splits
/// group names.
pub fn split_region(name: &str) -> Option<(String, String)> {
    let n = name.trim().trim_start_matches('|').trim_start();
    let (region, rest) = bracketed(n).or_else(|| piped(n)).or_else(|| coloned(n))?;
    let rest = rest.trim_start_matches(['|', ':', '-', ' ', '\t']).trim();
    let region = region.trim();
    (!region.is_empty() && !rest.is_empty()).then(|| (region.to_string(), rest.to_string()))
}

/// `[UK] Sport`: up to 16 characters in brackets.
fn bracketed(n: &str) -> Option<(&str, &str)> {
    let inner = n.strip_prefix('[')?;
    let close = inner.find(']')?;
    let region = &inner[..close];
    let rest = inner[close + 1..].trim_start();
    (!region.is_empty() && region.chars().count() <= 16 && !rest.is_empty()).then_some((region, rest))
}

/// `UK| Sport`: up to 16 characters before a bar.
fn piped(n: &str) -> Option<(&str, &str)> {
    let bar = n.find('|')?;
    let region = &n[..bar];
    let rest = n[bar + 1..].trim_start();
    (!region.is_empty() && region.chars().count() <= 16 && !rest.is_empty()).then_some((region, rest))
}

/// `UK: Sport`: one to four capitals before a colon.
fn coloned(n: &str) -> Option<(&str, &str)> {
    let colon = n.find(':')?;
    let region = &n[..colon];
    let rest = n[colon + 1..].trim_start();
    let capitals = !region.is_empty() && region.len() <= 4 && region.chars().all(|c| c.is_ascii_uppercase());
    (capitals && !rest.is_empty()).then_some((region, rest))
}

/// A channel's name as a list shows it: in a group, without the region its
/// group already says ("Sky Sports 1" in UK · Sport, not "UK| Sky Sports 1").
pub fn channel_name(c: &LiveChannel, category: Option<&Category>) -> String {
    if !matches!(category, Some(Category::Group(_))) {
        return c.name.clone();
    }
    match (split_region(&c.name), split_region(&c.group)) {
        (Some(name), Some(group)) if name.0.to_lowercase() == group.0.to_lowercase() => name.1,
        _ => c.name.clone(),
    }
}

// ---- changing channels

/// The channel numbered `digits` ("7" finds "007" too); none when none is.
pub fn channel_by_number<'a>(list: &'a [LiveChannel], digits: &str) -> Option<&'a LiveChannel> {
    if let Some(exact) = list.iter().find(|c| c.number.as_deref() == Some(digits)) {
        return Some(exact);
    }
    let n: u64 = digits.parse().ok()?;
    list.iter().find(|c| {
        c.number.as_deref().is_some_and(|num| {
            !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) && num.parse::<u64>() == Ok(n)
        })
    })
}

// ---- the guide

/// The starts of the chunks covering `from`–`to`, counted from `origin` and kept within the guide's span.
pub fn chunk_starts(from: i64, to: i64, origin: i64) -> Vec<i64> {
    let first = (from - origin).div_euclid(GUIDE_CHUNK).max(0);
    let last = (GUIDE_SPAN / GUIDE_CHUNK - 1).min((from.max(to - 1) - origin).div_euclid(GUIDE_CHUNK));
    (first..=last).map(|i| origin + i * GUIDE_CHUNK).collect()
}

/// A chunk's programmes added to what was had, each once (by its start), in order.
pub fn merge_guide(into: &mut HashMap<i64, Vec<LiveProgram>>, guide: &LiveGuide) {
    for (id, programs) in &guide.programs {
        let known = into.entry(*id).or_default();
        let mut added: Vec<LiveProgram> =
            programs.iter().filter(|p| !known.iter().any(|k| k.start == p.start)).cloned().collect();
        if added.is_empty() {
            continue;
        }
        // A chunk can list the same programme twice: once each.
        added.sort_by_key(|p| p.start);
        added.dedup_by_key(|p| p.start);
        known.append(&mut added);
        known.sort_by_key(|p| p.start);
    }
}

/// The programmes of a guide answer by channel, as [`merge_guide`] takes them.
pub type Programs = BTreeMap<i64, Vec<LiveProgram>>;

/// One cell of a guide row: a programme, or a stretch the guide says nothing of (`program` none).
#[derive(Clone, Debug, PartialEq)]
pub struct GuideCell {
    pub key: String,
    pub start: i64,
    pub stop: i64,
    pub program: Option<LiveProgram>,
}

/// A channel's row between `from` and `to`: its programmes there, with what
/// the guide says nothing of between them as cells of their own, so the
/// remote moves through time without jumping over a hole. A channel without
/// a guide is one cell.
pub fn guide_cells(programs: Option<&[LiveProgram]>, from: i64, to: i64) -> Vec<GuideCell> {
    let mut out = Vec::new();
    let mut at = from;
    for p in programs.unwrap_or(&[]) {
        if p.stop <= from || p.stop <= at {
            continue;
        }
        if p.start >= to {
            break;
        }
        if p.start - at >= MIN_GAP {
            out.push(GuideCell { key: format!("gap/{at}"), start: at, stop: p.start, program: None });
        }
        out.push(GuideCell {
            key: p.start.to_string(),
            start: p.start.max(at),
            stop: p.stop,
            program: Some(p.clone()),
        });
        at = p.stop;
    }
    if to - at >= MIN_GAP || out.is_empty() {
        out.push(GuideCell { key: format!("gap/{at}"), start: at, stop: to.max(at + MIN_GAP), program: None });
    }
    out
}

/// The cell at time `t` (the last one when it is past them all).
pub fn cell_at(cells: &[GuideCell], t: i64) -> usize {
    cells.iter().position(|c| c.stop > t).unwrap_or(cells.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(channel: i64, start: i64, stop: i64) -> LiveProgram {
        LiveProgram { channel_id: channel, start, stop, title: format!("P{start}"), ..LiveProgram::default() }
    }

    fn channel(id: i64, now: Option<LiveProgram>, next: Option<LiveProgram>) -> LiveChannel {
        LiveChannel {
            id,
            source_id: 1,
            name: format!("Ch {id}"),
            number: Some((100 + id).to_string()),
            group: "News".into(),
            has_guide: true,
            now,
            next,
            ..LiveChannel::default()
        }
    }

    fn group(name: &str, region: Option<&str>, label: &str, count: i64) -> LiveGroup {
        LiveGroup { name: name.into(), region: region.map(str::to_string), label: label.into(), count }
    }

    #[test]
    fn whats_on_is_the_channels_now_then_its_next_once_the_list_is_a_minute_old() {
        let c = channel(1, Some(program(1, 0, 600)), Some(program(1, 600, 1200)));
        assert_eq!(now_and_next(&c, 300).on.map(|p| p.start), Some(0));
        assert_eq!(now_and_next(&c, 300).next.map(|p| p.start), Some(600));
        // The list was asked for before 600: its next is on now, and nothing is known after it.
        assert_eq!(now_and_next(&c, 700), NowAndNext { on: c.next.clone(), next: None });
        // Nothing on yet: what comes first is next.
        let later = channel(2, Some(program(2, 900, 1200)), None);
        assert_eq!(now_and_next(&later, 300), NowAndNext { on: None, next: Some(program(2, 900, 1200)) });
    }

    #[test]
    fn says_how_long_is_left_as_the_web_does() {
        assert_eq!(time_left(600, 0), "10 min left");
        assert_eq!(time_left(3900, 0), "1 h 5 min left");
        assert_eq!(time_left(7200, 0), "2 h left");
        assert_eq!(time_left(10, 0), "1 min left");
    }

    #[test]
    fn times_and_days_in_the_tvs_own_clock() {
        // 2026-10-30 20:00 UTC was a Friday.
        let t = 1_793_390_400;
        assert_eq!(format_time(t, true, 0), "20:00");
        assert_eq!(format_time(t, false, 3600), "9:00 PM");
        let p = LiveProgram { start: t, stop: t + 3600, ..LiveProgram::default() };
        assert_eq!(format_span(&p, true, 0), "20:00 – 21:00");
        assert_eq!(format_guide_day(t, t, 0), "Today");
        assert_eq!(format_guide_day(t + 86_400, t, 0), "Tomorrow");
        assert_eq!(format_guide_day(t - 86_400, t, 0), "Yesterday");
        assert_eq!(format_guide_day(t, t + 3 * 86_400, 0), "Fri 30 Oct");
        assert_eq!(channel_title(&channel(1, None, None)), "101 Ch 1");
    }

    #[test]
    fn categories_round_trip_through_their_keys_a_searchs_words_included() {
        for c in [
            Category::All,
            Category::Favorites,
            Category::Recent,
            Category::Group("UK| Sport".into()),
            Category::Search("bbc one".into()),
        ] {
            assert_eq!(Category::parse(Some(&c.key())), c);
        }
        assert_eq!(Category::parse(None), Category::All);
        assert_eq!(Category::parse(Some("search:")), Category::All);
    }

    #[test]
    fn categories_are_asked_for_as_browse_wants_them_a_search_among_every_channel() {
        let search = Category::Search("bbc".into()).browse(0, 100, None);
        assert_eq!((search.category, search.q.as_deref()), (LiveCategory::All, Some("bbc")));
        let news = Category::Group("News".into()).browse(0, 100, None);
        assert_eq!((news.category, news.q), (LiveCategory::Group("News".into()), None));
        assert_eq!(Category::Recent.browse(0, 100, None).category, LiveCategory::Recent);
    }

    #[test]
    fn regions_in_alphabetical_order_then_the_other_groups_in_the_providers_order() {
        let groups = vec![
            group("UK| Sport", Some("UK"), "Sport", 40),
            group("News", None, "News", 2),
            group("UK| News", Some("UK"), "News", 10),
            group("FR| Films", Some("FR"), "Films", 7),
        ];
        let mut more = groups.clone();
        more.push(group("FR| News", Some("FR"), "News", 3));
        more.push(group("DE| Sport", Some("DE"), "Sport", 4));
        let (regions, others) = group_sections(&more);
        let shown: Vec<String> =
            regions.iter().map(|r| format!("{} ({}, {})", r.region, r.groups.len(), r.count)).collect();
        assert_eq!(shown, ["FR (2, 10)", "UK (2, 50)"]);
        // A region of one group is one of the rest.
        assert_eq!(others.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["News", "DE| Sport"]);
        // Fewer than two regions: one plain list.
        assert_eq!(group_sections(&groups), (Vec::new(), groups.clone()));
        assert_eq!(
            groups_matching(&groups, "news", 5).iter().map(|g| g.name.as_str()).collect::<Vec<_>>(),
            ["News", "UK| News"]
        );
        assert!(groups_matching(&groups, " ", 5).is_empty());
        assert_eq!(category_name(&Category::Group("UK| Sport".into()), &groups), "UK · Sport");
        assert_eq!(category_name(&Category::Group("News".into()), &groups), "News");
        assert_eq!(category_name(&Category::Recent, &groups), "Recently watched");
        assert_eq!(category_name(&Category::Group("Gone".into()), &[]), "Gone");
    }

    #[test]
    fn a_channels_name_in_a_group_leaves_out_the_region_the_group_says() {
        let named =
            |name: &str, group: &str| LiveChannel { name: name.into(), group: group.into(), ..LiveChannel::default() };
        let sport = Category::Group("UK| Sport".into());
        assert_eq!(channel_name(&named("UK| Sky Sports 1", "UK| Sport"), Some(&sport)), "Sky Sports 1");
        assert_eq!(channel_name(&named("[UK] Sky Sports 1", "UK| Sport"), Some(&sport)), "Sky Sports 1");
        // Another region, or not in a group: the name as it is.
        assert_eq!(
            channel_name(&named("AR| Sport 01", "BEE| Sport"), Some(&Category::Group("BEE| Sport".into()))),
            "AR| Sport 01"
        );
        assert_eq!(channel_name(&named("UK| Sky Sports 1", "UK| Sport"), Some(&Category::All)), "UK| Sky Sports 1");
        assert_eq!(split_region("UK: Sport"), Some(("UK".into(), "Sport".into())));
        assert_eq!(split_region("Sport"), None);
    }

    #[test]
    fn finds_a_number_typed_with_or_without_its_leading_zeros() {
        let numbered = |id: i64, n: &str| LiveChannel { id, number: Some(n.into()), ..LiveChannel::default() };
        let list = [numbered(1, "007"), numbered(2, "101"), numbered(3, "1.1")];
        assert_eq!(channel_by_number(&list, "101").map(|c| c.id), Some(2));
        assert_eq!(channel_by_number(&list, "7").map(|c| c.id), Some(1));
        assert_eq!(channel_by_number(&list, "11"), None);
    }

    #[test]
    fn the_guide_asks_for_the_chunks_covering_what_shows_within_its_two_days() {
        assert_eq!(chunk_starts(0, 3600, 0), [0]);
        assert_eq!(chunk_starts(GUIDE_CHUNK - 60, GUIDE_CHUNK + 60, 0), [0, GUIDE_CHUNK]);
        assert_eq!(chunk_starts(-3600, 60, 0), [0]);
        assert_eq!(chunk_starts(47 * 3600, 60 * 3600, 0), [7 * GUIDE_CHUNK]);
    }

    #[test]
    fn merges_chunks_each_programme_once_in_order() {
        let mut into = HashMap::new();
        let guide = |from, to, programs: Vec<(i64, Vec<LiveProgram>)>| LiveGuide {
            from,
            to,
            programs: programs.into_iter().collect(),
        };
        merge_guide(&mut into, &guide(0, 100, vec![(1, vec![program(1, 60, 120), program(1, 0, 60)])]));
        merge_guide(
            &mut into,
            &guide(100, 200, vec![(1, vec![program(1, 60, 120), program(1, 120, 300)]), (2, vec![program(2, 0, 300)])]),
        );
        assert_eq!(into[&1].iter().map(|p| p.start).collect::<Vec<_>>(), [0, 60, 120]);
        assert_eq!(into[&2].iter().map(|p| p.start).collect::<Vec<_>>(), [0]);
    }

    #[test]
    fn fills_what_the_guide_says_nothing_of() {
        let programs = [program(1, 0, 1800), program(1, 3600, 5400), program(1, 5430, 7200)];
        let cells = guide_cells(Some(&programs), 900, 9000);
        let shown: Vec<(String, i64, i64)> = cells.iter().map(|c| (c.key.clone(), c.start, c.stop)).collect();
        let want = [
            ("0", 900, 1800),
            ("gap/1800", 1800, 3600),
            ("3600", 3600, 5400),
            ("5430", 5430, 7200),
            ("gap/7200", 7200, 9000),
        ];
        assert_eq!(shown, want.map(|(k, a, b)| (k.to_string(), a, b)));
        assert_eq!(cell_at(&cells, 1000), 0);
        assert_eq!(cell_at(&cells, 3600), 2);
        assert_eq!(cell_at(&cells, 99_999), 4);
        // A channel without a guide is one cell.
        assert_eq!(
            guide_cells(None, 0, 3600),
            [GuideCell { key: "gap/0".into(), start: 0, stop: 3600, program: None }]
        );
    }
}
