//! The words of a title's page, its actions and a person's page, as the Android TV app writes them
//! (`strings_tv_item.xml`, `ItemPages.kt`, `PersonViewModel.kt`; the TV app's `item.tsx`).

use pley_core::format::{episode_label, format_duration, format_runtime, format_time_left, is_in_progress, pluralize};
use pley_core::model::{CreditRole, ExtraType, ItemKind, ItemSummary, PersonCredit, PersonRole};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The year, month (1–12) and day of a server date (`2024-03-12`, or a timestamp starting so).
fn ymd(date: &str) -> Option<(i64, usize, u32)> {
    let mut parts = date.get(..10)?.split('-');
    let y = parts.next()?.parse::<i64>().ok()?;
    let m = parts.next()?.parse::<usize>().ok().filter(|m| (1..=12).contains(m))?;
    let d = parts.next()?.parse::<u32>().ok().filter(|d| (1..=31).contains(d))?;
    Some((y, m, d))
}

/// "Apr 1, 2023" (Android's medium date in English); empty when there is none.
pub fn medium_date(date: Option<&str>) -> String {
    match date.and_then(ymd) {
        Some((y, m, d)) => format!("{} {d}, {y}", &MONTHS[m - 1][..3]),
        None => String::new(),
    }
}

/// "12 March 2024", as the info block says when an episode aired; empty when there is none.
pub fn long_date(date: Option<&str>) -> String {
    match date.and_then(ymd) {
        Some((y, m, d)) => format!("{d} {} {y}", MONTHS[m - 1]),
        None => String::new(),
    }
}

/// "Season 2 · Episode 5", "Episode 5", or empty.
pub fn episode_long_label(season: Option<i64>, episode: Option<i64>) -> String {
    [season.map(|s| format!("Season {s}")), episode.map(|e| format!("Episode {e}"))]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What an extra is ("Trailer", "Behind the Scenes"…), else "Extra" (`EXTRA_LABELS`).
pub fn extra_label(kind: Option<ExtraType>) -> &'static str {
    match kind {
        Some(ExtraType::Trailer) => "Trailer",
        Some(ExtraType::BehindTheScenes) => "Behind the Scenes",
        Some(ExtraType::DeletedScene) => "Deleted Scene",
        Some(ExtraType::Featurette) => "Featurette",
        Some(ExtraType::Interview) => "Interview",
        Some(ExtraType::Scene) => "Scene",
        Some(ExtraType::Short) => "Short",
        _ => "Extra",
    }
}

/// An extra's caption line: "Trailer · 2m" (`extraSubtitle`).
pub fn extra_subtitle(item: &ItemSummary) -> String {
    join([extra_label(item.extra_type).to_string(), format_duration(item.duration_ms)])
}

/// An episode tile's title: "5. Glasshouse".
pub fn episode_tile_title(item: &ItemSummary) -> String {
    match item.index {
        Some(i) => format!("{i}. {}", item.title),
        None => item.title.clone(),
    }
}

/// An episode tile's second line: its runtime, or the time left while it is in progress.
pub fn episode_length(item: &ItemSummary) -> String {
    if is_in_progress(item) {
        format_time_left(item.user_state.position_ms, item.duration_ms)
    } else {
        format_runtime(item.duration_ms)
    }
}

/// The info block's line for an episode focus rests on: "Season 2 · Episode 5 · 58 min · Aired 12
/// March 2024".
pub fn episode_info_line(item: &ItemSummary) -> String {
    let aired = long_date(item.release_date.as_deref());
    join([
        episode_long_label(item.parent_index, item.index),
        format_runtime(item.duration_ms),
        if aired.is_empty() { aired } else { format!("Aired {aired}") },
    ])
}

/// A show's years: "2021-2025" from its seasons, or its own year (`yearRange`).
pub fn year_range(year: Option<i64>, seasons: &[ItemSummary]) -> Option<String> {
    let years: Vec<i64> = seasons.iter().filter_map(|s| s.year).chain(year).collect();
    let first = *years.iter().min()?;
    let last = *years.iter().max()?;
    Some(if last > first { format!("{first}-{last}") } else { first.to_string() })
}

/// "1 star", "3 stars".
pub fn stars_label(stars: u32) -> String {
    if stars == 1 { "1 star".into() } else { format!("{stars} stars") }
}

/// "and 3 more", after the names a details line shows.
pub fn and_more(more: usize) -> String {
    format!("and {more} more")
}

/// The cast row's header (`TvItemScreen.kt`): "Cast", "Crew", "Cast & Crew", and for an episode
/// "Guest Stars" or "Guest Stars & Crew".
pub fn cast_title(episode: bool, cast: bool, crew: bool) -> &'static str {
    match (cast, crew, episode) {
        (_, false, true) => "Guest Stars",
        (_, false, false) => "Cast",
        (false, true, _) => "Crew",
        (true, true, true) => "Guest Stars & Crew",
        (true, true, false) => "Cast & Crew",
    }
}

/// A crew member's tile caption in the cast row.
pub fn crew_role(role: CreditRole) -> &'static str {
    match role {
        CreditRole::Director => "Director",
        CreditRole::Writer => "Writer",
        CreditRole::Creator => "Creator",
        CreditRole::Producer => "Producer",
        _ => "",
    }
}

/// The line under a title on its actions page: "S2 · E5 · Office Olympics" for an episode (its
/// title only when the show's name is the page's title, `ItemActions.kt`), else the card's own
/// ("2019", "3 seasons").
pub fn actions_subtitle(item: &ItemSummary) -> String {
    if item.kind == ItemKind::Episode {
        let named_by_show = item.grandparent_title.as_deref().is_some_and(|t| !t.is_empty());
        let title = if named_by_show { item.title.clone() } else { String::new() };
        return join([episode_label(item.parent_index, item.index), title]);
    }
    pley_core::format::card_subtitle(item)
}

/// A title as its actions name it: an episode by its show.
pub fn actions_title(item: &ItemSummary) -> String {
    match item.kind {
        ItemKind::Episode => {
            item.grandparent_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| item.title.clone())
        }
        _ => item.title.clone(),
    }
}

/// A person's parts in a title, as the web says them: "as Neo", "Director", "Guest star (3
/// episodes)" (`roleText`).
pub fn role_text(roles: &[PersonRole]) -> String {
    roles
        .iter()
        .map(|r| {
            let episodes = r.episodes.unwrap_or(0);
            let base = if r.role == CreditRole::Actor {
                match r.character.as_deref().filter(|c| !c.is_empty()) {
                    Some(c) => format!("as {c}"),
                    None if episodes > 0 => "Guest star".into(),
                    None => "Actor".into(),
                }
            } else {
                match crew_role(r.role) {
                    "" => "Crew".into(),
                    name => name.into(),
                }
            };
            if episodes > 0 { format!("{base} ({})", pluralize(episodes, "episode", None)) } else { base }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A title's caption on a person's page: "2019 · as Neo".
pub fn credit_subtitle(item: &ItemSummary, roles: &[PersonRole]) -> String {
    join([item.year.map(|y| y.to_string()).unwrap_or_default(), role_text(roles)])
}

/// "3 movies · 2 shows", for "… in your libraries"; empty with none.
pub fn person_counts(credits: &[&PersonCredit]) -> String {
    let count = |kind| credits.iter().filter(|c| c.item.kind == kind).count() as i64;
    let (movies, shows) = (count(ItemKind::Movie), count(ItemKind::Show));
    join([
        if movies > 0 { pluralize(movies, "movie", None) } else { String::new() },
        if shows > 0 { pluralize(shows, "show", None) } else { String::new() },
    ])
}

/// A row's header on a person's page.
pub fn role_heading(role: CreditRole) -> &'static str {
    match role {
        CreditRole::Director => "Directing",
        CreditRole::Creator => "Creating",
        CreditRole::Writer => "Writing",
        CreditRole::Producer => "Producing",
        _ => "Acting",
    }
}

/// "Idris Vale" → "IV".
pub fn initials(name: &str) -> String {
    name.split_whitespace().take(2).filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect()
}

/// The parts that say something, joined with " · ".
pub fn join<const N: usize>(parts: [String; N]) -> String {
    parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::UserState;

    fn episode(index: i64, season: i64) -> ItemSummary {
        ItemSummary {
            id: 7,
            kind: ItemKind::Episode,
            title: "Glasshouse".into(),
            index: Some(index),
            parent_index: Some(season),
            duration_ms: Some(58 * 60_000),
            release_date: Some("2024-03-12".into()),
            grandparent_title: Some("Severance".into()),
            ..ItemSummary::default()
        }
    }

    #[test]
    fn dates_read_as_android_writes_them() {
        assert_eq!(medium_date(Some("2023-04-01")), "Apr 1, 2023");
        assert_eq!(medium_date(Some("2024-09-05T00:00:00Z")), "Sep 5, 2024");
        assert_eq!(long_date(Some("2024-03-12")), "12 March 2024");
        assert_eq!(medium_date(Some("2024-13-01")), "");
        assert_eq!(medium_date(None), "");
    }

    #[test]
    fn episodes_are_labelled_long_and_short() {
        let e = episode(5, 2);
        assert_eq!(episode_long_label(Some(2), Some(5)), "Season 2 · Episode 5");
        assert_eq!(episode_tile_title(&e), "5. Glasshouse");
        assert_eq!(episode_length(&e), "58 min");
        assert_eq!(episode_info_line(&e), "Season 2 · Episode 5 · 58 min · Aired 12 March 2024");
        assert_eq!(actions_subtitle(&e), "S2 · E5 · Glasshouse");
        assert_eq!(actions_title(&e), "Severance");
        // Without its show's name the episode's title heads the page, and isn't said twice.
        let nameless = ItemSummary { grandparent_title: None, ..e.clone() };
        assert_eq!(
            (actions_title(&nameless).as_str(), actions_subtitle(&nameless).as_str()),
            ("Glasshouse", "S2 · E5")
        );
        let started = ItemSummary { user_state: UserState { position_ms: 32 * 60_000, ..UserState::default() }, ..e };
        assert_eq!(episode_length(&started), "26 min left");
    }

    #[test]
    fn extras_say_what_they_are() {
        let trailer = ItemSummary {
            kind: ItemKind::Extra,
            extra_type: Some(ExtraType::Trailer),
            duration_ms: Some(125_000),
            ..ItemSummary::default()
        };
        assert_eq!(extra_subtitle(&trailer), "Trailer · 2m");
        let other = ItemSummary { kind: ItemKind::Extra, ..ItemSummary::default() };
        assert_eq!(extra_subtitle(&other), "Extra");
        assert_eq!(extra_label(Some(ExtraType::BehindTheScenes)), "Behind the Scenes");
    }

    #[test]
    fn a_shows_years_span_its_seasons() {
        let season = |y| ItemSummary { year: Some(y), ..ItemSummary::default() };
        assert_eq!(year_range(Some(2021), &[season(2021), season(2025)]).as_deref(), Some("2021-2025"));
        assert_eq!(year_range(Some(2019), &[]).as_deref(), Some("2019"));
        assert_eq!(year_range(None, &[]), None);
    }

    #[test]
    fn cast_rows_are_titled_by_what_they_hold() {
        assert_eq!(cast_title(false, true, false), "Cast");
        assert_eq!(cast_title(true, true, false), "Guest Stars");
        assert_eq!(cast_title(false, false, true), "Crew");
        assert_eq!(cast_title(false, true, true), "Cast & Crew");
        assert_eq!(cast_title(true, true, true), "Guest Stars & Crew");
    }

    #[test]
    fn a_persons_parts_read_as_on_the_web() {
        let actor = |character: Option<&str>, episodes| PersonRole {
            role: CreditRole::Actor,
            character: character.map(String::from),
            episodes,
        };
        assert_eq!(role_text(&[actor(Some("Neo"), None)]), "as Neo");
        assert_eq!(role_text(&[actor(None, Some(3))]), "Guest star (3 episodes)");
        assert_eq!(role_text(&[actor(None, None)]), "Actor");
        let director = PersonRole { role: CreditRole::Director, character: None, episodes: None };
        assert_eq!(role_text(std::slice::from_ref(&director)), "Director");
        let item = ItemSummary { year: Some(1999), ..ItemSummary::default() };
        assert_eq!(credit_subtitle(&item, &[actor(Some("Neo"), None)]), "1999 · as Neo");
        assert_eq!(credit_subtitle(&ItemSummary::default(), &[director]), "Director");
    }

    #[test]
    fn a_person_is_counted_and_named_by_initials() {
        let credit = |kind| PersonCredit { item: ItemSummary { kind, ..ItemSummary::default() }, roles: Vec::new() };
        let (a, b, c) = (credit(ItemKind::Movie), credit(ItemKind::Movie), credit(ItemKind::Show));
        assert_eq!(person_counts(&[&a, &b, &c]), "2 movies · 1 show");
        assert_eq!(person_counts(&[]), "");
        assert_eq!(initials("Idris Vale"), "IV");
        assert_eq!(initials("cher"), "C");
    }

    #[test]
    fn ratings_count_stars() {
        assert_eq!(stars_label(1), "1 star");
        assert_eq!(stars_label(5), "5 stars");
        assert_eq!(and_more(3), "and 3 more");
    }
}
