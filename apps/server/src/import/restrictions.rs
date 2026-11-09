//! A Plex Home user's restrictions as a Pley profile's limits: an age limit
//! with the rating system it's shown in, and labels always or never shown.
//!
//! Plex writes a rule per kind of library (`filterMovies`,
//! `filterTelevision`, `filterMusic`), URL-encoded: terms joined by `&`
//! (all must hold), each a choice of terms joined by `|` (one must), such as
//!
//! ```text
//! contentRating=03,04,05|label=Override: Kids&label!=Override: Adult
//! ```
//!
//! a rating on the list or the label `Override: Kids`, and never the label
//! `Override: Adult`. Pley has one age limit for every library, so the
//! lower of the rules' wins; "never" labels come from any rule, "always"
//! labels only from those every rule with ratings has, so no library opens
//! wider than Plex had it.

use std::collections::BTreeMap;

use crate::accounts::MAX_LABEL_RULES;
use crate::ratings::{self, Kind, SYSTEMS};

/// A profile's limits, worked out from a Home user's rules.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Limits {
    /// The age of the oldest rating allowed, `None` for no age limit.
    pub max_age: Option<i64>,
    /// "age", or the country (ISO 3166-1 alpha-2) of the ratings Plex's
    /// list mostly has; `None` with no age limit.
    pub system: Option<String>,
    /// Labels always shown, at most [`MAX_LABEL_RULES`].
    pub show: Vec<String>,
    /// Labels never shown, at most [`MAX_LABEL_RULES`].
    pub hide: Vec<String>,
    /// Labels past [`MAX_LABEL_RULES`] in either list, left out.
    pub left_over: Vec<String>,
    /// Parts of the rules Pley has no way to keep (only titles with a
    /// label, or a field other than ratings and labels), as Plex wrote them.
    pub not_kept: Vec<String>,
}

/// One kind of library's rule, read.
#[derive(Debug, Default)]
struct Rule {
    /// Ratings allowed: `None` when the rule doesn't limit them.
    ratings: Option<Vec<String>>,
    /// Ratings not allowed (`contentRating!=`): every other one is.
    denied: Vec<String>,
    /// Labels shown whatever the rating.
    always: Vec<String>,
    never: Vec<String>,
    not_kept: Vec<String>,
}

/// The limits of a user with these rules (any may be empty). `region` is
/// the server's country, for ratings that don't say theirs.
pub fn limits(movies: &str, television: &str, music: &str, region: &str) -> Limits {
    let rules = [(read(movies), Kind::Film), (read(television), Kind::Tv), (read(music), Kind::Film)];
    let mut out = Limits::default();

    let all: Vec<&str> =
        rules.iter().flat_map(|(r, _)| r.ratings.iter().flatten().chain(&r.denied)).map(String::as_str).collect();
    let system = system_of(&all, region);
    let country = system.as_deref().filter(|s| *s != "age").unwrap_or(region);
    let by_age = system.as_deref() == Some("age");
    out.max_age = rules
        .iter()
        .filter_map(|(rule, kind)| {
            // An empty list allows nothing rated, so the youngest age, 0.
            let allowed = rule
                .ratings
                .as_ref()
                .map(|list| list.iter().filter_map(|r| age_of(r, country, *kind)).max().unwrap_or(0));
            let below = below_denied(rule, country, by_age, *kind, &mut out.not_kept);
            match (allowed, below) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            }
        })
        .min();
    out.system = out.max_age.and(system);

    let mut hide = Vec::new();
    for (rule, _) in &rules {
        for label in &rule.never {
            push_unique(&mut hide, label);
        }
        out.not_kept.extend(rule.not_kept.iter().cloned());
    }
    let limited: Vec<&Rule> =
        rules.iter().map(|(r, _)| r).filter(|r| r.ratings.is_some() || !r.denied.is_empty()).collect();
    let mut show = Vec::new();
    if let Some((first, rest)) = limited.split_first() {
        for label in &first.always {
            let everywhere = rest.iter().all(|r| r.always.iter().any(|l| same_label(l, label)));
            if everywhere && !hide.iter().any(|h| same_label(h, label)) {
                push_unique(&mut show, label);
            }
        }
    }
    for (list, kept) in [(show, &mut out.show), (hide, &mut out.hide)] {
        let mut list = list.into_iter();
        kept.extend(list.by_ref().take(MAX_LABEL_RULES));
        out.left_over.extend(list);
    }
    out
}

/// The age a rule's denied ratings limit it to: the oldest one under the
/// youngest denied, which hides them all. Plex shows every rating it
/// doesn't deny, so when some it allows are older than that (it denies
/// PG-13 alone), Pley hides those too and says so: an age limit can't
/// have gaps. A denied rating Pley doesn't know limits nothing, and is said.
fn below_denied(rule: &Rule, country: &str, by_age: bool, kind: Kind, not_kept: &mut Vec<String>) -> Option<i64> {
    if rule.denied.is_empty() {
        return None;
    }
    let mut ages = Vec::new();
    for rating in &rule.denied {
        match age_of(rating, country, kind) {
            Some(age) => ages.push(age),
            None => push_unique(not_kept, &format!("contentRating!={rating}")),
        }
    }
    let youngest = ages.into_iter().min()?;
    if by_age {
        return Some((youngest - 1).max(0));
    }
    let Some(system) = ratings::system(country) else { return Some((youngest - 1).max(0)) };
    let of_kind = || system.ratings.iter().filter(|r| r.kind.is_none_or(|k| k == kind));
    let denied = |r: &ratings::Rating| r.names.iter().any(|n| rule.denied.iter().any(|d| same(without_country(d), n)));
    let denied_ages: Vec<i64> = of_kind().filter(|r| denied(r)).map(|r| r.age).collect();
    // A rating Plex shows over the limit, unless one of its age is denied
    // (X beside NC-17: an old name for the same).
    let gaps = of_kind().any(|r| r.age >= youngest && !denied(r) && !denied_ages.contains(&r.age));
    if gaps {
        push_unique(not_kept, &format!("contentRating!={}", rule.denied.join(",")));
    }
    Some(of_kind().map(|r| r.age).filter(|age| *age < youngest).max().unwrap_or(0))
}

/// `gb/12A` → `12A`: a rating without the country Plex wrote before it.
fn without_country(rating: &str) -> &str {
    match rating.split_once(['/', ':']) {
        Some((before, after)) if (2..=3).contains(&before.trim().len()) => after.trim(),
        _ => rating.trim(),
    }
}

/// The age of a rating on Plex's list. One written with its country is
/// read with that country's ratings only, and left out when Pley doesn't
/// know it there, rather than read by another country's names (Plex's
/// presets have Korea's `kr/A`, for all ages, which would read as India's
/// adults only): a rating left out only makes the limit stricter.
fn age_of(rating: &str, country: &str, kind: Kind) -> Option<i64> {
    let rating = rating.trim();
    if let Some((before, after)) = rating.split_once(['/', ':']) {
        let before = before.trim();
        if (2..=3).contains(&before.len()) && before.chars().all(|c| c.is_ascii_alphabetic()) {
            let written = match before.to_ascii_uppercase().as_str() {
                "UK" => "GB".to_owned(),
                "USA" => "US".to_owned(),
                other => other.to_owned(),
            };
            // Only that country's names: "A" is no age at all in Korea's.
            return ratings::system(&written)?.age(kind, after.trim());
        }
    }
    ratings::age(Some(country), kind, rating)
}

/// A rule as Plex writes it, read.
fn read(rule: &str) -> Rule {
    let mut out = Rule::default();
    for all in rule.split('&').filter(|t| !t.trim().is_empty()) {
        let mut ratings: Option<Vec<String>> = None;
        let (mut always, mut never, mut denied, mut other) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for term in all.split('|') {
            let (field, negated, values) = match term.split_once("!=") {
                Some((field, values)) => (field, true, values),
                None => match term.split_once('=') {
                    Some((field, values)) => (field, false, values),
                    None => (term, false, ""),
                },
            };
            let values: Vec<String> =
                decode(values).split(',').map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()).collect();
            match (decode(field).trim(), negated) {
                ("contentRating", false) => ratings.get_or_insert_default().extend(values),
                ("contentRating", true) => denied.extend(values),
                ("label", false) => always.extend(values),
                ("label", true) => never.extend(values),
                _ => other.push(decode(term)),
            }
        }
        out.never.extend(never);
        out.denied.extend(denied);
        out.not_kept.extend(other);
        match ratings {
            // Rated as listed, or labelled.
            Some(list) => {
                out.ratings = Some(match out.ratings.take() {
                    // Two lists that must both hold: the ratings on both.
                    Some(before) => before.into_iter().filter(|r| list.iter().any(|l| l == r)).collect(),
                    None => list,
                });
                out.always.extend(always);
            }
            // Only titles with these labels: Pley has no such limit, so the
            // strictest it has (rated for all, or labelled), never none.
            None if !always.is_empty() => {
                out.ratings = Some(Vec::new());
                out.always.extend(always);
                out.not_kept.push(decode(all));
            }
            None => {}
        }
    }
    out
}

/// The rating system most of `ratings` are in: "age" for plain numbers
/// (Kometa's Common Sense ages), else the country with most of them,
/// `region`'s when it's as good as any. `None` when there are none.
fn system_of(ratings: &[&str], region: &str) -> Option<String> {
    if ratings.is_empty() {
        return None;
    }
    let mut votes: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut ages = 0;
    for rating in ratings {
        let rating = rating.trim();
        if let Some((country, _)) = rating.split_once('/')
            && let Some(system) = ratings::system(country)
        {
            *votes.entry(system.country).or_default() += 1;
            continue;
        }
        // A bare number is an age, and a rating of the countries that have
        // it (the UK's 15); "CSM 5" only an age.
        let digits = rating.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == ' ');
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            ages += 1;
        }
        for system in SYSTEMS {
            if system.ratings.iter().any(|r| r.names.iter().any(|n| same(n, rating))) {
                *votes.entry(system.country).or_default() += 1;
            }
        }
    }
    let best = votes.values().copied().max().unwrap_or(0);
    if ages >= best {
        return (ages > 0).then(|| "age".to_owned());
    }
    let region = ratings::system(region).map(|s| s.country);
    let country = match region {
        Some(r) if votes.get(r) == Some(&best) => r,
        _ => votes.iter().find(|(_, n)| **n == best).map(|(c, _)| *c)?,
    };
    Some(country.to_owned())
}

fn decode(text: &str) -> String {
    url::form_urlencoded::parse(format!("v={text}").as_bytes()).next().map(|(_, v)| v.into_owned()).unwrap_or_default()
}

fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Labels compare as the database's `COLLATE NOCASE` does: "kids" is
/// "Kids", but "Été" isn't "été", so a rule finds the titles it names.
fn same_label(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn push_unique(list: &mut Vec<String>, label: &str) {
    if !list.iter().any(|l| same_label(l, label)) {
        list.push(label.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_with_ratings_and_labels() {
        let rule = "contentRating=G%2CPG%2CTV-PG|label=Override%3A%20Kids&label!=Override%3A%20Adult";
        let l = limits(rule, rule, "", "US");
        assert_eq!(l.max_age, Some(10));
        assert_eq!(l.system.as_deref(), Some("US"));
        assert_eq!(l.show, ["Override: Kids"]);
        assert_eq!(l.hide, ["Override: Adult"]);
        assert!(l.left_over.is_empty() && l.not_kept.is_empty());
    }

    #[test]
    fn the_lower_age_wins_and_always_needs_every_rated_rule() {
        let l = limits("contentRating=12A,15|label=Family", "contentRating=12|label=Cartoons", "label!=Loud", "GB");
        assert_eq!(l.max_age, Some(12));
        assert_eq!(l.system.as_deref(), Some("GB"));
        // Neither label is in both rated rules: either would open a library
        // wider than Plex had it.
        assert!(l.show.is_empty());
        assert_eq!(l.hide, ["Loud"]);
    }

    #[test]
    fn denied_ratings_limit_below_the_youngest() {
        // Plex's "disallow": R and NC-17 out, so up to PG-13.
        let l = limits("contentRating!=R,NC-17", "contentRating!=TV-MA", "", "US");
        assert_eq!((l.max_age, l.system.as_deref()), (Some(13), Some("US")));
        assert!(l.not_kept.is_empty(), "{:?}", l.not_kept);
    }

    #[test]
    fn denied_ratings_with_a_gap_are_kept_strictly_and_said() {
        // PG-13 alone denied: Pley hides R too, which Plex showed.
        let l = limits("contentRating!=PG-13", "", "", "US");
        assert_eq!(l.max_age, Some(10));
        assert_eq!(l.not_kept, ["contentRating!=PG-13"]);
    }

    #[test]
    fn denied_and_allowed_ratings_together_take_the_lower() {
        let l = limits("contentRating=G,PG,PG-13,R&contentRating!=R", "", "", "US");
        assert_eq!(l.max_age, Some(13));
    }

    #[test]
    fn denied_ages_and_unknown_ratings() {
        let l = limits("contentRating!=16,18", "", "", "GB");
        assert_eq!((l.max_age, l.system.as_deref()), (Some(15), Some("age")));
        let l = limits("contentRating!=Nonsense", "", "", "US");
        assert_eq!(l.max_age, None);
        assert_eq!(l.not_kept, ["contentRating!=Nonsense"]);
    }

    #[test]
    fn plain_numbers_are_ages() {
        let l = limits("contentRating=03,04,05,06,07,08", "contentRating=CSM 5,CSM 7", "", "GB");
        assert_eq!((l.max_age, l.system.as_deref()), (Some(7), Some("age")));
    }

    #[test]
    fn ratings_with_their_country() {
        let l = limits("contentRating=gb%2FU,gb%2FPG,gb%2F12A", "", "", "US");
        assert_eq!((l.max_age, l.system.as_deref()), (Some(12), Some("GB")));
    }

    #[test]
    fn no_rules_no_limits() {
        assert_eq!(limits("", "", "", "US"), Limits::default());
        let l = limits("label!=Scary", "", "", "US");
        assert_eq!((l.max_age, l.system, l.hide), (None, None, vec!["Scary".to_owned()]));
    }

    #[test]
    fn a_label_never_and_always_is_never() {
        let l = limits("contentRating=G|label=Mixed&label!=Mixed", "", "", "US");
        assert!(l.show.is_empty());
        assert_eq!(l.hide, ["Mixed"]);
    }

    #[test]
    fn labels_differing_only_in_case_are_one_as_the_database_says() {
        let l = limits("label!=Kids&label!=kids&label!=%C3%89t%C3%A9&label!=%C3%A9t%C3%A9", "", "", "US");
        assert_eq!(l.hide, ["Kids", "Été", "été"]);
    }

    #[test]
    fn what_pley_cant_keep_is_said() {
        let l = limits("label=Kids", "year=1990&contentRating=G", "", "US");
        assert_eq!(l.not_kept, ["label=Kids", "year=1990"]);
        assert_eq!(l.max_age, Some(0));
        // Only titles labelled Kids: as strict as Pley can be, never open.
        let l = limits("label=Kids", "", "", "US");
        assert_eq!((l.max_age, l.show.as_slice()), (Some(0), ["Kids".to_owned()].as_slice()));
    }

    #[test]
    fn labels_past_the_most_kept_are_left_over() {
        let never: Vec<String> = (0..MAX_LABEL_RULES + 2).map(|n| format!("label!=L{n}")).collect();
        let l = limits(&never.join("&"), "", "", "US");
        assert_eq!(l.hide.len(), MAX_LABEL_RULES);
        assert_eq!(l.left_over, [format!("L{MAX_LABEL_RULES}"), format!("L{}", MAX_LABEL_RULES + 1)]);
    }

    #[test]
    fn plex_presets_become_ages() {
        // The filters Plex writes for its Little kid, Older kid and Teen
        // presets: every country's ratings up to an age, and plain ages.
        const LITTLE: &str = "G%2CTV-G%2CTV-Y%2Cgb%2FUc%2Cgb%2FU%2CUK%3A+Uc%2CUK%3A+U%2Cde%2F0%2CFSK+0%2CAb+0+Jahren%2C\
            au%2FG%2Cca%2FG%2Cfr%2FTous+publics%2Cfr%2FU%2Cbr%2FL%2Cbr%2FLivre%2Cch%2F0%2Cnl%2FAL%2C\
            es%2FAPTA%2Cdk%2FA%2Ckr%2FA%2Cse%2FBtl%2CCSM+3%2CCSM+4%2CCSM+5%2C0%2C0%2B%2C2%2C3%2C4%2C5%2C\
            US%2FG%2CUS%2FTV-G%2CUS%2FTV-Y";
        const OLDER: &str = "%2CPG%2CTV-PG%2CTV-Y7%2CGP%2Cgb%2FPG%2CUK%3A+PG%2Cde%2F6%2CFSK+6%2CAb+6+Jahren%2Cau%2FPG%2C\
            ca%2FPG%2Cfr%2F10%2Cbr%2F10%2Cch%2F6%2Cch%2F7%2Cch%2F8%2Cch%2F10+%2Cnl%2F6%2Cnl%2F9%2C\
            es%2F7%2Cdk%2F7%2Cdk%2F11%2Cse%2F7%2Cse%2F11%2CCSM+6%2CCSM+7%2CCSM+8%2CCSM+9%2CCSM+10%2C\
            CSM+11%2C6%2C7%2C8%2C9%2C10%2C11%2CUS%2FPG%2CUS%2FTV-PG%2CUS%2FTV-Y7";
        const TEEN: &str = "%2CPG-13%2CTV-14%2Cgb%2F12A%2Cgb%2F12%2CUK%3A+12a%2CUK%3A+12%2Cde%2F12%2CFSK+12%2C\
            Ab+12+Jahren%2Cca%2F14A%2Cca%2F13%2B%2Cca%2F14%2Cca%2F14%2B%2Cfr%2F12+%2Cbr%2F12%2C\
            br%2F14%2Cnl%2F12%2Ces%2F12%2Ckr%2F12%2CCSM+12%2CCSM+13%2CCSM+14%2C12%2C12%2B%2C12A%2C13%2C\
            14%2C14A%2CUS%2FPG-13%2CUS%2FTV-14";
        let presets = [
            (format!("contentRating={LITTLE}"), 5),
            (format!("contentRating={LITTLE}{OLDER}"), 11),
            (format!("contentRating={LITTLE}{OLDER}{TEEN}"), 14),
        ];
        for region in ["US", "GB"] {
            for (rule, age) in &presets {
                let l = limits(rule, rule, "", region);
                assert_eq!((l.max_age, l.system.as_deref()), (Some(*age), Some("age")), "{region} {age}");
                assert!(l.not_kept.is_empty() && l.show.is_empty() && l.hide.is_empty(), "{region} {age}: {l:?}");
            }
        }
        // Korea's "A" (for all) isn't India's adults-only "A".
        assert_eq!(age_of("UK: 15", "US", Kind::Film), Some(15));
        assert_eq!(age_of("kr/A", "US", Kind::Film), None);
    }
}
