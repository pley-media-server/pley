//! Measures intro and credits detection on a real library's cached
//! fingerprints, to compare changes to the matching:
//!
//! ```text
//! PLEY_MARKER_EVAL=<dir> cargo test -p pley-server --release --lib markers::eval -- --ignored --nocapture
//! ```
//!
//! `<dir>` holds `episodes.tsv` (file id, season id, season and episode
//! numbers, duration in ms, the intro and credits fingerprint file names or
//! `-`, the intro and credits chapter times or `-1`, a title, the show id)
//! and `fingerprints/`, the server's fingerprint cache. Chapter-marked intros
//! and credits are the reference for accuracy; every season counts for how
//! many episodes get markers and how much their lengths agree. With
//! `PLEY_MARKER_EVAL_OUT=<file>` every result is written out, to compare two
//! versions season by season.
//!
//! `up_next` compares where players offer the next episode before and after
//! a change to how credits are settled, on a library's stored markers:
//!
//! ```text
//! PLEY_UP_NEXT_EVAL=<dir> cargo test -p pley-server --lib markers::eval::up_next -- --ignored --nocapture
//! ```
//!
//! `<dir>` holds `files.tsv` (file id, `episode` or `movie`, duration in ms,
//! the stored credits marker's source or `-`, its start and end in ms, the
//! file's chapters as JSON or `-`) and `frames/<file id>.json`, the key frames
//! ([`FrameStats`]) of the end of the files whose shared credits stop early.
//! Every file's result goes to `<dir>/up_next.tsv`.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::*;

/// Where Up Next comes in a file: when credits reaching the end begin, else
/// at the end of the file. Before this rule, any credits brought it.
fn up_next_at(credits: Option<(i64, i64)>, duration_ms: i64, any_credits: bool) -> i64 {
    match credits {
        Some((start, end)) if any_credits || duration_ms - end <= CREDITS_TAIL_MS => start,
        _ => duration_ms,
    }
}

#[test]
#[ignore = "needs a library's markers: PLEY_UP_NEXT_EVAL=<dir>"]
fn up_next() {
    let Ok(dir) = std::env::var("PLEY_UP_NEXT_EVAL") else { return };
    let dir = Path::new(&dir);
    let text = std::fs::read_to_string(dir.join("files.tsv")).expect("files.tsv");
    // (kind, what changed) -> (files, seconds Up Next moved)
    let mut tally: BTreeMap<(String, String), (usize, Vec<i64>)> = BTreeMap::new();
    let mut out = String::new();
    for line in text.lines() {
        let c: Vec<&str> = line.split('\t').collect();
        let num = |i: usize| c.get(i).and_then(|v| v.parse::<i64>().ok());
        let (Some(file_id), Some(duration)) = (num(0), num(2)) else { continue };
        let kind = c[1].to_owned();
        let old = Some((c[3], num(4).unwrap_or(-1), num(5).unwrap_or(-1))).filter(|(s, ..)| *s != "-");
        let chapters: Vec<Chapter> =
            c.get(6).filter(|j| **j != "-").and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default();
        let frames: Option<Vec<FrameStats>> = std::fs::read(dir.join("frames").join(format!("{file_id}.json")))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());

        // The credits as the server now settles them: chapters first, then
        // the stored fingerprint or key-frame match with today's rules.
        let mut measured = true;
        let new: Option<Found> =
            match chapter_markers(&chapters, duration).into_iter().find(|m| m.kind == MarkerKind::Credits) {
                Some(chapter) => Some(chapter),
                None => match old {
                    Some(("fingerprint", start, end)) => {
                        let shared = finalize(Some((start, end)), MarkerKind::Credits, duration);
                        match (shared, &frames) {
                            (Some(s), Some(f)) if s.end_ms < duration => settle_credits(s, f, duration),
                            (Some(s), None) if s.end_ms < duration => {
                                measured = false;
                                Some(s)
                            }
                            (s, _) => s,
                        }
                    }
                    // Found again from the key frames when they are at hand.
                    Some(("frames", start, end)) => match &frames {
                        Some(f) => credits_from_frames(f, duration),
                        None => {
                            measured = false;
                            Some(Found { kind: MarkerKind::Credits, start_ms: start, end_ms: end, source: "frames" })
                        }
                    },
                    _ => None,
                },
            };
        let before = up_next_at(old.map(|(_, s, e)| (s, e)), duration, true);
        let after = up_next_at(new.as_ref().map(|m| (m.start_ms, m.end_ms)), duration, false);
        let skip_before = old.map(|(_, s, e)| (s, e));
        let skip_after = new.as_ref().map(|m| (m.start_ms, m.end_ms));
        let what = if !measured {
            "credits stopping early, key frames not measured".to_owned()
        } else if after == before {
            if skip_after == skip_before { "unchanged" } else { "Up Next unchanged, Skip Credits moved" }.to_owned()
        } else if after == duration {
            match &new {
                Some(m) if m.source == "frames" && old.is_some_and(|(s, ..)| s == "fingerprint") => {
                    "Up Next at the end: credits later on, a scene after them".to_owned()
                }
                Some(_) => "Up Next at the end: credits stop short of the end".to_owned(),
                None => "Up Next at the end: no credits".to_owned(),
            }
        } else if after > before {
            "Up Next later: at credits found later on".to_owned()
        } else {
            "Up Next earlier".to_owned()
        };
        let entry = tally.entry((kind.clone(), what.clone())).or_default();
        entry.0 += 1;
        entry.1.push((after - before) / 1000);
        let (ns, ne, nsrc) = new.as_ref().map_or((-1, -1, "-"), |m| (m.start_ms, m.end_ms, m.source));
        let (os, oe) = skip_before.unwrap_or((-1, -1));
        out.push_str(&format!(
            "{file_id}\t{kind}\t{duration}\t{}\t{os}\t{oe}\t{nsrc}\t{ns}\t{ne}\t{before}\t{after}\t{what}\n",
            old.map_or("-", |(s, ..)| s)
        ));
    }
    for ((kind, what), (n, moved)) in &tally {
        let mut moved = moved.clone();
        moved.sort_unstable();
        let median = moved.get(moved.len() / 2).copied().unwrap_or(0);
        println!("{kind}: {what}: {n} (Up Next moves by {median} s, median)");
    }
    std::fs::write(dir.join("up_next.tsv"), out).expect("up_next.tsv");
}

struct Episode {
    file_id: i64,
    season: i64,
    show: i64,
    duration_ms: i64,
    fp: [Option<Fp>; 2],
    truth: [Option<(i64, i64)>; 2],
    title: String,
}

fn load(dir: &Path) -> Vec<Episode> {
    let text = std::fs::read_to_string(dir.join("episodes.tsv")).expect("episodes.tsv");
    let fp =
        |name: Option<&&str>| name.filter(|n| **n != "-").and_then(|n| read_cached(&dir.join("fingerprints").join(n)));
    text.lines()
        .filter_map(|line| {
            let c: Vec<&str> = line.split('\t').collect();
            let num = |i: usize| c.get(i).and_then(|v| v.parse::<i64>().ok());
            let truth = |i: usize| Some((num(i)?, num(i + 1)?)).filter(|(s, _)| *s >= 0);
            Some(Episode {
                file_id: num(0)?,
                season: num(1)?,
                show: num(12).unwrap_or(-1),
                duration_ms: num(4)?,
                fp: [fp(c.get(5)), fp(c.get(6))],
                truth: [truth(7), truth(9)],
                title: c.get(11).copied().unwrap_or("?").to_owned(),
            })
        })
        .collect()
}

/// A way of choosing what each episode is compared with.
struct Variant {
    name: &'static str,
    /// Episodes of the season compared with each one.
    neighbours: usize,
    /// Episodes of the show's other seasons compared with one its own season
    /// found nothing in.
    other_seasons: usize,
}

#[derive(Default)]
struct Score {
    episodes: usize,
    found: usize,
    truth: usize,
    truth_found: usize,
    start_errors: Vec<i64>,
    end_errors: Vec<i64>,
    season_spread: Vec<i64>,
    seasons: usize,
    empty_seasons: Vec<String>,
}

fn median(mut v: Vec<i64>) -> i64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

fn within(v: &[i64], ms: i64) -> String {
    if v.is_empty() {
        return "-".into();
    }
    format!("{:.0}%", 100.0 * v.iter().filter(|e| e.abs() <= ms).count() as f64 / v.len() as f64)
}

#[test]
#[ignore = "needs a library's fingerprints: PLEY_MARKER_EVAL=<dir>"]
fn evaluate() {
    let Ok(dir) = std::env::var("PLEY_MARKER_EVAL") else { return };
    let episodes = load(Path::new(&dir));
    let mut seasons: BTreeMap<i64, Vec<&Episode>> = BTreeMap::new();
    let mut shows: HashMap<i64, Vec<&Episode>> = HashMap::new();
    for e in &episodes {
        seasons.entry(e.season).or_default().push(e);
        if e.show >= 0 {
            shows.entry(e.show).or_default().push(e);
        }
    }
    let variants = [
        Variant { name: "4 neighbours", neighbours: 4, other_seasons: 0 },
        Variant { name: "8 neighbours", neighbours: 8, other_seasons: 0 },
        Variant { name: "8 neighbours, then 6 from other seasons", neighbours: 8, other_seasons: 6 },
    ];
    let mut results = String::new();
    for variant in &variants {
        println!("== {}", variant.name);
        for (k, kind) in [MarkerKind::Intro, MarkerKind::Credits].into_iter().enumerate() {
            let mut score = Score::default();
            for season in seasons.values() {
                if season.iter().filter(|e| e.fp[k].is_some()).count() < 2 {
                    continue;
                }
                score.seasons += 1;
                let mut lengths = Vec::new();
                for (pos, episode) in season.iter().enumerate() {
                    let Some(own) = &episode.fp[k] else { continue };
                    let others: Vec<&Fp> = nearest(pos, season.len(), variant.neighbours)
                        .into_iter()
                        .filter_map(|o| season[o].fp[k].as_ref())
                        .collect();
                    let mut found = detect_one(own, &others, kind, episode.duration_ms);
                    if found.is_none() && variant.other_seasons > 0 {
                        let elsewhere: Vec<&Fp> = shows
                            .get(&episode.show)
                            .into_iter()
                            .flatten()
                            .filter(|e| e.season != episode.season)
                            .filter_map(|e| e.fp[k].as_ref())
                            .take(variant.other_seasons)
                            .collect();
                        found = detect_one(own, &elsewhere, kind, episode.duration_ms);
                    }
                    score.episodes += 1;
                    if let Some(f) = &found {
                        score.found += 1;
                        lengths.push(f.end_ms - f.start_ms);
                    }
                    if let Some((s, e)) = episode.truth[k] {
                        score.truth += 1;
                        if let Some(f) = &found {
                            score.truth_found += 1;
                            score.start_errors.push(f.start_ms - s);
                            score.end_errors.push(f.end_ms - e);
                        }
                    }
                    let (s, e) = found.as_ref().map_or((-1, -1), |f| (f.start_ms, f.end_ms));
                    results.push_str(&format!(
                        "{}\t{}\t{}\t{s}\t{e}\t{}\n",
                        variant.name,
                        episode.file_id,
                        kind.as_str(),
                        episode.title
                    ));
                }
                if lengths.is_empty() {
                    score.empty_seasons.push(season[0].title.clone());
                } else {
                    let m = median(lengths.clone());
                    score.season_spread.extend(lengths.iter().map(|l| l - m));
                }
            }
            let abs = |v: &[i64]| v.iter().map(|e| e.abs()).collect::<Vec<_>>();
            println!(
                "{}: {}/{} episodes ({:.0}%) in {} seasons, {} seasons with none\n  vs chapters: {}/{} found, start error median {} ms (signed {}, {} within 2 s), end error median {} ms (signed {}, {} within 2 s)\n  length spread within seasons: median {} ms, {} within 5 s",
                kind.as_str(),
                score.found,
                score.episodes,
                100.0 * score.found as f64 / score.episodes.max(1) as f64,
                score.seasons,
                score.empty_seasons.len(),
                score.truth_found,
                score.truth,
                median(abs(&score.start_errors)),
                median(score.start_errors.clone()),
                within(&score.start_errors, 2_000),
                median(abs(&score.end_errors)),
                median(score.end_errors.clone()),
                within(&score.end_errors, 2_000),
                median(abs(&score.season_spread)),
                within(&score.season_spread, 5_000),
            );
            println!("  seasons with none: {}", score.empty_seasons.join(", "));
        }
    }
    if let Ok(out) = std::env::var("PLEY_MARKER_EVAL_OUT") {
        std::fs::write(out, results).expect("results file");
    }
}
