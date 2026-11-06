//! A title's page as data (Android's `ItemPages.kt`, `ItemViewModel.kt`; the TV app's `item.tsx`):
//! what Play plays and says, the metadata line, the track choices, the details and media info, the
//! seasons and episodes and the rows under them, from the server's answers and what was picked on
//! the page. Pure, so it is tested without a screen.

use pley_core::format::{FactBlock, media_facts};
use pley_core::format::{
    audio_format_label, codec_label, episode_label, format_bytes, format_runtime, format_time_left, is_in_progress,
    language_name, quality_badges, resolution_label, season_title, stream_label, stream_range, version_label,
};
use pley_core::model::{
    CollectionRef, CreditRole, ExtraType, ItemDetail, ItemKind, ItemRequest, ItemSummary, MediaFile, MediaStream,
    RequestSummary, SignInAgain, StreamKind,
};
use pley_core::requests::may_ask;
use pley_i18n::t;

use crate::words::{self, join, medium_date, year_range};

/// What was picked on the page: the version (not saved, as on the web) and tracks (saved for the
/// title, and kept here so Play uses them at once).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picks {
    pub file: Option<i64>,
    /// The file audio was picked for, and the track (`None`: Automatic).
    pub audio: Option<(i64, Option<i64>)>,
    /// The file subtitles were picked for, and the track (`None`: off).
    pub subtitle: Option<(i64, Option<i64>)>,
}

/// The version and tracks the picks come to on a title (`playing` in `ItemPages.kt`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Playing<'a> {
    /// The version chosen here, else the first (the tallest).
    pub file: Option<&'a MediaFile>,
    /// The audio picked here, else the details' choice.
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
    pub default_audio: Option<&'a MediaStream>,
    /// The audio picked on this page, the only one the player is told of: otherwise the server
    /// picks the track each device plays best.
    pub picked_audio: Option<i64>,
    /// No track is picked, here or before: the page names only the language.
    pub automatic_audio: bool,
}

/// What `picks` come to on `d`.
pub fn playing<'a>(d: &'a ItemDetail, picks: &Picks) -> Playing<'a> {
    let file = picks.file.and_then(|id| d.file(id)).or(d.files.first());
    let default_audio = file
        .and_then(|f| f.streams_of(StreamKind::Audio).find(|s| s.default))
        .or_else(|| file.and_then(|f| f.streams_of(StreamKind::Audio).next()));
    let picked_here = matches!((file, picks.audio), (Some(f), Some((at, _))) if at == f.id);
    let picked_audio = if picked_here { picks.audio.and_then(|a| a.1) } else { None };
    // A server that doesn't say whether the profile picked the audio reads as Automatic, as on Android
    // (`audioPicked` defaults to false there).
    let automatic_audio =
        if picked_here { picked_audio.is_none() } else { !file.is_some_and(|f| f.audio_picked == Some(true)) };
    let subtitle_index = match (file, picks.subtitle) {
        (Some(f), Some((at, pick))) if at == f.id => pick,
        (f, _) => f.and_then(|f| f.selected_subtitle),
    };
    Playing {
        file,
        audio_index: picked_audio.or(file.and_then(|f| f.selected_audio)),
        subtitle_index,
        default_audio,
        picked_audio,
        automatic_audio,
    }
}

/// What the Play pill does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryKind {
    Play,
    Resume,
    /// No files, or no episode to play: the pill is disabled.
    Unavailable,
}

/// The Play pill: what it says and what it plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Primary {
    pub kind: PrimaryKind,
    /// A show's or season's next episode: "S1 · E3".
    pub episode: Option<String>,
    /// "1h 33m left" while resuming.
    pub time_left: String,
    /// The item it plays (the title itself, or a show's next episode).
    pub target: Option<i64>,
}

impl Primary {
    /// The pill's label: "Resume", "Play S1 · E3", "Unavailable"…
    pub fn label(&self) -> String {
        match (self.kind, &self.episode) {
            (PrimaryKind::Unavailable, _) => t!("item.hero.unavailable").into(),
            (PrimaryKind::Resume, Some(e)) => t!("item.hero.resumeEpisode", episode = e),
            (PrimaryKind::Resume, None) => t!("item.hero.resume").into(),
            (PrimaryKind::Play, Some(e)) => t!("item.hero.playEpisode", episode = e),
            (PrimaryKind::Play, None) => t!("item.hero.play").into(),
        }
    }
}

/// The pill of `d`: a video plays itself; a show or season its next episode (`on_deck`), which
/// Play asks the server for when it isn't known yet. `own_episodes`: a season's episodes, once
/// shown. Nothing to play is a show without episodes, or a season whose own are known to be none
/// (`containerPrimary`, `ItemPages.kt`).
pub fn primary(d: &ItemDetail, on_deck: Option<&ItemSummary>, own_episodes: Option<&[ItemSummary]>) -> Primary {
    let video = matches!(d.kind, ItemKind::Movie | ItemKind::Episode | ItemKind::Extra);
    let state = &d.user_state;
    if video {
        let resumable = !state.watched && state.position_ms > 0;
        let kind = if d.files.is_empty() {
            PrimaryKind::Unavailable
        } else if resumable {
            PrimaryKind::Resume
        } else {
            PrimaryKind::Play
        };
        let time_left = if kind == PrimaryKind::Resume {
            format_time_left(state.position_ms, d.duration_ms)
        } else {
            String::new()
        };
        return Primary { kind, episode: None, time_left, target: Some(d.id) };
    }
    if let Some(next) = on_deck {
        let resuming = next.user_state.position_ms > 0 && !next.user_state.watched;
        let label = episode_label(next.parent_index, next.index);
        return Primary {
            kind: if resuming { PrimaryKind::Resume } else { PrimaryKind::Play },
            episode: (!label.is_empty()).then_some(label),
            time_left: if resuming {
                format_time_left(next.user_state.position_ms, next.duration_ms)
            } else {
                String::new()
            },
            target: Some(next.id),
        };
    }
    let nothing = match d.kind {
        ItemKind::Season => own_episodes.is_some_and(|e| e.is_empty()) || d.leaf_count == Some(0),
        _ => d.leaf_count.unwrap_or(0) == 0,
    };
    Primary {
        kind: if nothing { PrimaryKind::Unavailable } else { PrimaryKind::Play },
        episode: None,
        time_left: String::new(),
        target: None,
    }
}

/// One choice of a panel or the details: a version or a track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: i64,
    pub label: String,
    pub selected: bool,
}

/// The Audio and subtitles panel's choices (`trackChoices`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackChoices {
    /// With several versions.
    pub versions: Vec<Choice>,
    /// With two tracks or more.
    pub audio: Vec<Choice>,
    pub subtitles: Vec<Choice>,
    /// No track is picked: Automatic is ticked.
    pub automatic_audio: bool,
    /// "Automatic (English)", else "Automatic".
    pub automatic_label: String,
    /// Off is ticked.
    pub subtitles_off: bool,
    /// The file the tracks are of.
    pub file_id: i64,
}

/// The choices of `d`, or none when there is nothing to choose (one version, at most one audio
/// track, no subtitles): the button isn't shown then.
pub fn track_choices(d: &ItemDetail, playing: &Playing) -> Option<TrackChoices> {
    let file = playing.file?;
    let audio: Vec<&MediaStream> = file.streams_of(StreamKind::Audio).collect();
    let subtitles: Vec<&MediaStream> = file.streams_of(StreamKind::Subtitle).collect();
    if d.files.len() < 2 && audio.len() < 2 && subtitles.is_empty() {
        return None;
    }
    let audio_start = playing.audio_index.or(playing.default_audio.map(|s| s.index));
    let audio_shown = audio_start.filter(|_| !playing.automatic_audio);
    let language =
        audio.iter().find(|s| Some(s.index) == audio_start).and_then(|s| language_name(s.language.as_deref()));
    let subtitle_shown = playing.subtitle_index.filter(|i| subtitles.iter().any(|s| s.index == *i));
    Some(TrackChoices {
        versions: if d.files.len() > 1 {
            d.files
                .iter()
                .enumerate()
                .map(|(i, f)| Choice { id: f.id, label: version_label(f, i), selected: f.id == file.id })
                .collect()
        } else {
            Vec::new()
        },
        audio: if audio.len() > 1 {
            audio
                .iter()
                .map(|s| Choice { id: s.index, label: stream_label(s), selected: Some(s.index) == audio_shown })
                .collect()
        } else {
            Vec::new()
        },
        subtitles: subtitles
            .iter()
            .map(|s| Choice { id: s.index, label: stream_label(s), selected: Some(s.index) == subtitle_shown })
            .collect(),
        automatic_audio: playing.automatic_audio,
        automatic_label: match language {
            Some(l) => t!("item.panels.automaticLanguage", language = l),
            None => t!("item.panels.automatic").into(),
        },
        subtitles_off: subtitle_shown.is_none(),
        file_id: file.id,
    })
}

/// The metadata line's parts (`meta`): facts, the content rating, the critics' rating, the
/// quality badges of the version that plays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub facts: Vec<String>,
    pub content_rating: Option<String>,
    pub rating: Option<String>,
    pub badges: Vec<String>,
}

/// The metadata line of `d` playing `file`; a show's seasons give its count and years.
pub fn meta(d: &ItemDetail, file: Option<&MediaFile>, seasons: &[ItemSummary]) -> Meta {
    let edition = file.and_then(|f| f.edition.clone()).unwrap_or_default();
    let facts = match d.kind {
        ItemKind::Show => {
            let count =
                d.child_count.unwrap_or_else(|| seasons.iter().filter(|s| s.index.unwrap_or(1) > 0).count() as i64);
            vec![
                if count > 0 { t!("item.counts.seasons", count = count) } else { String::new() },
                year_range(d.year, seasons).unwrap_or_default(),
            ]
        }
        ItemKind::Season => vec![
            d.year.map(|y| y.to_string()).unwrap_or_default(),
            d.leaf_count.filter(|l| *l > 0).map(|l| t!("item.counts.episodes", count = l)).unwrap_or_default(),
        ],
        ItemKind::Episode => vec![medium_date(d.release_date.as_deref()), edition, format_runtime(d.duration_ms)],
        _ => vec![d.year.map(|y| y.to_string()).unwrap_or_default(), edition, format_runtime(d.duration_ms)],
    };
    Meta {
        facts: facts.into_iter().filter(|f| !f.trim().is_empty()).collect(),
        content_rating: d.content_rating.clone().filter(|r| !r.trim().is_empty()),
        rating: d.rating.filter(|r| *r > 0.0).map(|r| pley_i18n::decimal(r, 1)),
        badges: file.map(quality_badges).unwrap_or_default(),
    }
}

/// A line of who made a title: its role and up to four people (`CrewLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewLine {
    pub role: CreditRole,
    /// The people's ids and names.
    pub people: Vec<(i64, String)>,
    /// How many more there are.
    pub more: usize,
}

/// At most this many names a crew line shows, then "and N more" (`CREW_NAMES`).
pub const CREW_NAMES: usize = 4;

/// Who made `d`: its creators, directors and writers, each once.
pub fn crew_lines(d: &ItemDetail) -> Vec<CrewLine> {
    [CreditRole::Creator, CreditRole::Director, CreditRole::Writer]
        .into_iter()
        .filter_map(|role| {
            let mut people: Vec<(i64, String)> = Vec::new();
            for c in d.crew.iter().filter(|c| c.role == role) {
                if !people.iter().any(|p| p.0 == c.id) {
                    people.push((c.id, c.name.clone()));
                }
            }
            if people.is_empty() {
                return None;
            }
            let more = people.len().saturating_sub(CREW_NAMES);
            people.truncate(CREW_NAMES);
            Some(CrewLine { role, people, more })
        })
        .collect()
}

/// The hero's credits line: "Directed by A, B" (or "Created by") and "Starring X, Y, Z".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Credits {
    /// "Directed by" or "Created by", and up to two names.
    pub made: Option<(&'static str, String)>,
    /// Up to three of the cast.
    pub starring: Option<String>,
}

/// The credits of `d`, or none with neither makers nor cast.
pub fn credits(d: &ItemDetail, crew: &[CrewLine]) -> Option<Credits> {
    let makers = crew
        .iter()
        .find(|l| l.role == CreditRole::Director)
        .or_else(|| crew.iter().find(|l| l.role == CreditRole::Creator));
    let made = makers.map(|l| {
        let label =
            if l.role == CreditRole::Director { t!("item.credits.directedBy") } else { t!("item.credits.createdBy") };
        (label, l.people.iter().take(2).map(|p| p.1.as_str()).collect::<Vec<_>>().join(", "))
    });
    let mut stars: Vec<&str> = Vec::new();
    for c in &d.cast {
        if stars.len() == 3 {
            break;
        }
        if !stars.contains(&c.name.as_str()) {
            stars.push(&c.name);
        }
    }
    let starring = (!stars.is_empty()).then(|| stars.join(", "));
    (made.is_some() || starring.is_some()).then_some(Credits { made, starring })
}

/// The details card's facts, label and value (`TvDetails.kt`).
pub fn facts(d: &ItemDetail, crew: &[CrewLine]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in crew {
        let people = line.people.len() + line.more;
        let label = match line.role {
            CreditRole::Creator => t!("item.details.createdBy").to_string(),
            CreditRole::Director => t!("item.details.directors", count = people),
            _ => t!("item.details.writers", count = people),
        };
        let mut names = line.people.iter().map(|p| p.1.as_str()).collect::<Vec<_>>().join(", ");
        if line.more > 0 {
            names = format!("{names} {}", words::and_more(line.more));
        }
        out.push((label, names));
    }
    if let Some(t) = d.original_title.as_ref().filter(|t| !t.trim().is_empty() && **t != d.title) {
        out.push((t!("item.details.originalTitle").into(), t.clone()));
    }
    if d.kind != ItemKind::Episode {
        let released = medium_date(d.release_date.as_deref());
        if !released.is_empty() {
            out.push((t!("item.details.released").into(), released));
        }
    }
    if let Some(studio) = d.studio.as_ref().filter(|s| !s.trim().is_empty()) {
        let network = matches!(d.kind, ItemKind::Episode | ItemKind::Show);
        let label = if network { t!("item.details.network") } else { t!("item.details.studio") };
        out.push((label.into(), studio.clone()));
    }
    if d.user_state.play_count > 0 {
        out.push((t!("item.details.plays").into(), pley_i18n::number(d.user_state.play_count)));
    }
    out
}

/// A version in its long form for the details: "Final Cut · 4K · HEVC · Dolby Vision P8.1 · TrueHD
/// Atmos 7.1 · 62.0 GB".
pub fn version_long(file: &MediaFile) -> String {
    let video = file.streams_of(StreamKind::Video).next();
    let audio = pley_core::format::starting_audio(file);
    let parts = [
        file.edition.clone().unwrap_or_default(),
        resolution_label(file.width, file.height).unwrap_or("").to_string(),
        file.video_codec.as_deref().map(codec_label).unwrap_or_default(),
        video.and_then(stream_range).unwrap_or_default(),
        audio.map(audio_format_label).unwrap_or_default(),
        if file.size > 0 { format_bytes(Some(file.size as u64)) } else { String::new() },
    ];
    let line = parts.into_iter().filter(|p| !p.trim().is_empty()).collect::<Vec<_>>().join(" · ");
    if line.is_empty() { t!("item.details.version").into() } else { line }
}

/// One file's media info: its title ("Final Cut", "Version 2", or none with one file) and blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileInfo {
    pub title: Option<String>,
    pub blocks: Vec<FactBlock>,
}

/// Every file's media info.
pub fn media_info(files: &[MediaFile]) -> Vec<FileInfo> {
    files
        .iter()
        .enumerate()
        .map(|(i, f)| FileInfo {
            title: f
                .edition
                .clone()
                .filter(|e| !e.trim().is_empty())
                .or_else(|| (files.len() > 1).then(|| t!("item.details.versionNumber", number = i + 1))),
            blocks: media_facts(f),
        })
        .collect()
}

/// A person in the cast row: who, their part or job, their photo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonTile {
    pub id: i64,
    pub name: String,
    pub subtitle: String,
    pub photo: Option<String>,
}

/// The cast with their parts, then who made it with their jobs, each person once (`castAndCrew`).
pub fn cast_and_crew(d: &ItemDetail, crew: &[CrewLine]) -> Vec<PersonTile> {
    let mut out: Vec<PersonTile> = Vec::new();
    for c in &d.cast {
        if !out.iter().any(|p| p.id == c.id) {
            out.push(PersonTile {
                id: c.id,
                name: c.name.clone(),
                subtitle: c.character.clone().unwrap_or_default(),
                photo: c.photo_url.clone(),
            });
        }
    }
    for line in crew {
        for (id, name) in &line.people {
            if !out.iter().any(|p| p.id == *id) {
                let photo = d.crew.iter().find(|c| c.id == *id).and_then(|c| c.photo_url.clone());
                out.push(PersonTile {
                    id: *id,
                    name: name.clone(),
                    subtitle: words::crew_role(line.role).into(),
                    photo,
                });
            }
        }
    }
    out
}

/// The season a show page opens on: the next episode's, else the first that isn't Specials, else
/// the first (`defaultSeason`).
pub fn default_season(seasons: &[ItemSummary], on_deck: Option<&ItemSummary>) -> Option<i64> {
    on_deck
        .and_then(|e| e.parent_id)
        .filter(|id| seasons.iter().any(|s| s.id == *id))
        .or_else(|| seasons.iter().find(|s| s.index.unwrap_or(1) > 0).map(|s| s.id))
        .or_else(|| seasons.first().map(|s| s.id))
}

/// The hero's title: an episode's or season's show, else the title itself.
pub fn hero_title(d: &ItemDetail) -> String {
    let show = match d.kind {
        ItemKind::Episode => d.grandparent_title.clone(),
        ItemKind::Season => d.parent_title.clone(),
        _ => None,
    };
    show.filter(|t| !t.is_empty()).unwrap_or_else(|| own_title(d))
}

/// The title's own name: a season's "Season 2" when it has none.
pub fn own_title(d: &ItemDetail) -> String {
    if d.kind == ItemKind::Season { season_title(Some(&d.title), d.index) } else { d.title.clone() }
}

/// The line under the hero's title: an episode's "S2 · E5 · Glasshouse"; a season's own title
/// under its show's.
pub fn context_line(d: &ItemDetail) -> Option<String> {
    match d.kind {
        ItemKind::Episode => Some(join([episode_label(d.parent_index, d.index), d.title.clone()])),
        ItemKind::Season if d.parent_title.as_ref().is_some_and(|t| !t.is_empty()) => Some(own_title(d)),
        _ => None,
    }
    .filter(|l| !l.is_empty())
}

/// Genres, then "Part of <collection>" for each collection.
pub fn genres_line(d: &ItemDetail, collections: &[CollectionRef]) -> String {
    d.genres
        .iter()
        .cloned()
        .chain(collections.iter().map(|c| t!("item.credits.partOf", collection = c.title)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Whether a title counts as watched: a show or season when every episode is.
pub fn watched(item: &ItemSummary) -> bool {
    match item.kind {
        ItemKind::Show | ItemKind::Season => {
            let leaves = item.leaf_count.unwrap_or(0);
            leaves > 0 && item.viewed_leaf_count.unwrap_or(0) >= leaves
        }
        _ => item.user_state.watched,
    }
}

/// The trailer the Trailer button plays: the first extra that is one.
pub fn trailer(d: &ItemDetail) -> Option<&ItemSummary> {
    d.extras.iter().find(|e| e.extra_type == Some(ExtraType::Trailer))
}

/// The profile's rating in half stars (1–10), or none.
pub fn user_rating(d: &ItemDetail) -> Option<i64> {
    d.user_state.rating.map(|r| (r.round() as i64).clamp(0, 10)).filter(|r| *r > 0)
}

/// Whether the profile can rate it: movies, shows and episodes.
pub fn rateable(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Movie | ItemKind::Show | ItemKind::Episode)
}

/// The episode the row opens on: the page's own, or the next episode when it is in the season
/// shown.
pub fn episode_target(d: &ItemDetail, on_deck: Option<&ItemSummary>, season: Option<i64>) -> Option<i64> {
    match d.kind {
        ItemKind::Episode => Some(d.id),
        _ => on_deck.filter(|e| e.parent_id.is_some() && e.parent_id == season).map(|e| e.id),
    }
}

/// Whether an item is in progress (a tile's bar, Resume in its actions).
pub fn in_progress(item: &ItemSummary) -> bool {
    is_in_progress(item)
}

/// Whether a title's page asks where its request is (`useItemRequest` in the TV app's
/// `request-kit.tsx`): a movie or show, with requests on for this sign-in, for a profile that may
/// ask or asked before.
pub fn wants_request(kind: ItemKind, s: Option<&RequestSummary>) -> bool {
    matches!(kind, ItemKind::Movie | ItemKind::Show)
        && s.is_some_and(|s| {
            s.enabled && s.sign_in_again != Some(SignInAgain::Requests) && (may_ask(Some(s)) || s.own > 0)
        })
}

/// Whether the page offers Request more seasons: a show the server names seasons left to ask for,
/// for a profile that may ask.
pub fn asks_more(r: &ItemRequest, s: Option<&RequestSummary>) -> bool {
    r.askable.as_ref().is_some_and(|a| !a.is_empty()) && may_ask(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::model::{Credit, UserState};

    fn stream(index: i64, kind: StreamKind, language: &str) -> MediaStream {
        MediaStream {
            index,
            kind,
            codec: if kind == StreamKind::Audio { "eac3".into() } else { "subrip".into() },
            language: Some(language.into()),
            channels: Some(6),
            ..MediaStream::default()
        }
    }

    fn movie() -> ItemDetail {
        let file = MediaFile {
            id: 100,
            width: Some(3840),
            height: Some(2160),
            video_codec: Some("hevc".into()),
            size: 62_000_000_000,
            streams: vec![
                stream(1, StreamKind::Audio, "eng"),
                stream(2, StreamKind::Audio, "fra"),
                stream(3, StreamKind::Subtitle, "fra"),
            ],
            ..MediaFile::default()
        };
        let mut d = ItemDetail::default();
        d.summary.id = 12;
        d.summary.kind = ItemKind::Movie;
        d.summary.title = "Arrival".into();
        d.summary.year = Some(2016);
        d.summary.duration_ms = Some(6_960_000);
        d.summary.rating = Some(7.94);
        d.content_rating = Some("PG-13".into());
        d.files = vec![file, MediaFile { id: 101, edition: Some("Final Cut".into()), ..MediaFile::default() }];
        d
    }

    #[test]
    fn a_movie_plays_or_resumes_itself() {
        let mut d = movie();
        let p = primary(&d, None, None);
        assert_eq!((p.kind, p.label(), p.target), (PrimaryKind::Play, "Play".to_string(), Some(12)));
        d.summary.user_state = UserState { position_ms: 1_000_000, ..UserState::default() };
        let p = primary(&d, None, None);
        assert_eq!(p.label(), "Resume");
        assert_eq!(p.time_left, "1h 39m left");
        d.files.clear();
        assert_eq!(primary(&d, None, None).label(), "Unavailable");
    }

    #[test]
    fn a_show_plays_its_next_episode() {
        let mut show = ItemDetail::default();
        show.summary.kind = ItemKind::Show;
        show.summary.leaf_count = Some(6);
        let next = ItemSummary {
            id: 20_003,
            kind: ItemKind::Episode,
            parent_index: Some(1),
            index: Some(3),
            duration_ms: Some(1_200_000),
            user_state: UserState { position_ms: 300_000, ..UserState::default() },
            ..ItemSummary::default()
        };
        let p = primary(&show, Some(&next), None);
        assert_eq!((p.label(), p.target), ("Resume S1 · E3".to_string(), Some(20_003)));
        let fresh = ItemSummary { user_state: UserState::default(), ..next };
        assert_eq!(primary(&show, Some(&fresh), None).label(), "Play S1 · E3");
        // Before the server names the next episode the pill says Play, and plays what it names.
        let waiting = primary(&show, None, None);
        assert_eq!((waiting.kind, waiting.target), (PrimaryKind::Play, None));
        show.summary.leaf_count = Some(0);
        assert_eq!(primary(&show, None, None).kind, PrimaryKind::Unavailable);
    }

    #[test]
    fn a_season_without_a_next_episode_still_says_play_unless_it_has_none() {
        let mut season = ItemDetail::default();
        season.summary.kind = ItemKind::Season;
        let episode = ItemSummary { id: 31, kind: ItemKind::Episode, ..ItemSummary::default() };
        // Its first episode isn't played in place of the next one (Android says Nothing to play).
        let p = primary(&season, None, Some(std::slice::from_ref(&episode)));
        assert_eq!((p.kind, p.target), (PrimaryKind::Play, None));
        assert_eq!(primary(&season, None, Some(&[])).kind, PrimaryKind::Unavailable);
        season.summary.leaf_count = Some(0);
        assert_eq!(primary(&season, None, None).kind, PrimaryKind::Unavailable);
    }

    #[test]
    fn picks_choose_the_version_and_tracks() {
        let d = movie();
        let none = Picks::default();
        let p = playing(&d, &none);
        assert_eq!(p.file.map(|f| f.id), Some(100));
        let t = track_choices(&d, &p).unwrap();
        assert_eq!(t.versions.len(), 2);
        assert!(t.versions[0].selected && !t.versions[1].selected);
        assert_eq!(t.audio.len(), 2);
        assert!(t.automatic_audio);
        assert_eq!(t.automatic_label, "Automatic (English)");
        assert!(t.subtitles_off);

        let picks = Picks { file: Some(100), audio: Some((100, Some(2))), subtitle: Some((100, Some(3))) };
        let p = playing(&d, &picks);
        assert_eq!((p.picked_audio, p.subtitle_index), (Some(2), Some(3)));
        let t = track_choices(&d, &p).unwrap();
        assert!(!t.automatic_audio && t.audio[1].selected && t.subtitles[0].selected && !t.subtitles_off);

        // A pick for another version doesn't carry over.
        let other = Picks { file: Some(101), ..picks };
        assert_eq!(playing(&d, &other).picked_audio, None);
    }

    #[test]
    fn nothing_to_choose_hides_the_tracks_button() {
        let mut d = movie();
        d.files.truncate(1);
        d.files[0].streams.truncate(1);
        assert_eq!(track_choices(&d, &playing(&d, &Picks::default())), None);
    }

    #[test]
    fn the_metadata_line_follows_the_kind() {
        let d = movie();
        let m = meta(&d, d.files.first(), &[]);
        assert_eq!(m.facts, vec!["2016".to_string(), "1h 56m".to_string()]);
        assert_eq!(m.content_rating.as_deref(), Some("PG-13"));
        assert_eq!(m.rating.as_deref(), Some("7.9"));
        assert_eq!(m.badges.first().map(String::as_str), Some("4K"));

        let mut show = ItemDetail::default();
        show.summary.kind = ItemKind::Show;
        show.summary.year = Some(2021);
        let seasons = [
            ItemSummary { index: Some(0), year: Some(2021), ..ItemSummary::default() },
            ItemSummary { index: Some(1), year: Some(2021), ..ItemSummary::default() },
            ItemSummary { index: Some(2), year: Some(2024), ..ItemSummary::default() },
        ];
        assert_eq!(meta(&show, None, &seasons).facts, vec!["2 seasons".to_string(), "2021-2024".to_string()]);
    }

    #[test]
    fn credits_and_facts_name_who_made_it() {
        let mut d = movie();
        let person = |id, name: &str, role| Credit { id, name: name.into(), role, ..Credit::default() };
        d.crew = (1..=6).map(|i| person(i, &format!("Writer {i}"), CreditRole::Writer)).collect();
        d.crew.push(person(10, "Denis", CreditRole::Director));
        d.cast = vec![
            person(20, "Amy", CreditRole::Actor),
            person(21, "Jeremy", CreditRole::Actor),
            person(20, "Amy", CreditRole::Actor),
            person(22, "Forest", CreditRole::Actor),
            person(23, "Michael", CreditRole::Actor),
        ];
        let crew = crew_lines(&d);
        let c = credits(&d, &crew).unwrap();
        assert_eq!(c.made, Some(("Directed by", "Denis".to_string())));
        assert_eq!(c.starring.as_deref(), Some("Amy, Jeremy, Forest"));
        let f = facts(&d, &crew);
        assert_eq!(f[0], ("Director".to_string(), "Denis".to_string()));
        assert_eq!(f[1], ("Writers".to_string(), "Writer 1, Writer 2, Writer 3, Writer 4 and 2 more".to_string()));
        let tiles = cast_and_crew(&d, &crew);
        assert_eq!(tiles.len(), 4 + 1 + 4);
        assert_eq!(tiles[4].subtitle, "Director");
    }

    #[test]
    fn a_title_page_speaks_french() {
        use pley_i18n::{Language, with_language};
        with_language(Language::Fr, || {
            let resume = Primary {
                kind: PrimaryKind::Resume,
                episode: Some("S1 · E3".into()),
                time_left: String::new(),
                target: None,
            };
            assert_eq!(resume.label(), "Reprendre S1 · E3");
            let mut show = ItemDetail::default();
            show.summary.kind = ItemKind::Show;
            show.summary.child_count = Some(2);
            assert_eq!(meta(&show, None, &[]).facts, vec!["2 saisons".to_string()]);
            let mut d = movie();
            let person = |id, name: &str, role| Credit { id, name: name.into(), role, ..Credit::default() };
            d.crew = vec![person(1, "Denis", CreditRole::Director), person(2, "Eric", CreditRole::Writer)];
            d.summary.user_state.play_count = 1234;
            let f = facts(&d, &crew_lines(&d));
            assert_eq!(f[0], ("Réalisation".to_string(), "Denis".to_string()));
            assert_eq!(f[1].0, "Scénario");
            assert_eq!(f.last(), Some(&("Lectures".to_string(), "1\u{a0}234".to_string())));
            assert_eq!(meta(&d, d.files.first(), &[]).rating.as_deref(), Some("7,9"));
            let picked = playing(&d, &Picks::default());
            assert!(track_choices(&d, &picked).unwrap().automatic_label.starts_with("Automatique ("));
            assert_eq!(media_info(&d.files)[0].title.as_deref(), Some("Version 1"));
        });
    }

    #[test]
    fn versions_read_in_full() {
        let d = movie();
        assert_eq!(version_long(&d.files[0]), "4K · HEVC · EAC3 5.1 · 57.7 GB");
        assert_eq!(version_long(&d.files[1]), "Final Cut");
        let info = media_info(&d.files);
        assert_eq!(info[1].title.as_deref(), Some("Final Cut"));
        assert_eq!(info[0].title.as_deref(), Some("Version 1"));
    }

    #[test]
    fn a_show_opens_on_its_next_episodes_season() {
        let season = |id, index| ItemSummary { id, index: Some(index), ..ItemSummary::default() };
        let seasons = [season(1, 0), season(2, 1), season(3, 2)];
        let next = ItemSummary { parent_id: Some(3), ..ItemSummary::default() };
        assert_eq!(default_season(&seasons, Some(&next)), Some(3));
        assert_eq!(default_season(&seasons, None), Some(2));
        assert_eq!(default_season(&seasons[..1], None), Some(1));
        assert_eq!(default_season(&[], None), None);
    }

    #[test]
    fn movies_and_shows_ask_for_their_request_where_requests_are_on() {
        use pley_core::model::RequestAccess;
        let asks = RequestSummary { enabled: true, access: RequestAccess::Ask, ..RequestSummary::default() };
        assert!(wants_request(ItemKind::Movie, Some(&asks)));
        assert!(wants_request(ItemKind::Show, Some(&asks)));
        for kind in [ItemKind::Season, ItemKind::Episode, ItemKind::Album] {
            assert!(!wants_request(kind, Some(&asks)), "{kind:?}");
        }
        assert!(!wants_request(ItemKind::Movie, None), "a server without requests");
        let off = RequestSummary { enabled: false, ..asks.clone() };
        assert!(!wants_request(ItemKind::Movie, Some(&off)));
        let old_sign_in = RequestSummary { sign_in_again: Some(SignInAgain::Requests), ..asks.clone() };
        assert!(!wants_request(ItemKind::Movie, Some(&old_sign_in)));
        // A profile that may no longer ask still sees what it asked for.
        let closed = RequestSummary { access: RequestAccess::Off, ..asks.clone() };
        assert!(!wants_request(ItemKind::Movie, Some(&closed)));
        assert!(wants_request(ItemKind::Movie, Some(&RequestSummary { own: 2, ..closed })));
    }

    #[test]
    fn request_more_seasons_needs_seasons_left_and_leave_to_ask() {
        use pley_core::model::RequestAccess;
        let asks = RequestSummary { enabled: true, access: RequestAccess::Auto, ..RequestSummary::default() };
        let show = ItemRequest { askable: Some(vec![3, 4]), ..ItemRequest::default() };
        assert!(asks_more(&show, Some(&asks)));
        assert!(!asks_more(&ItemRequest { askable: Some(Vec::new()), ..show.clone() }, Some(&asks)));
        assert!(!asks_more(&ItemRequest { askable: None, ..show.clone() }, Some(&asks)), "no TMDB details yet");
        let closed = RequestSummary { access: RequestAccess::Off, own: 1, ..asks };
        assert!(!asks_more(&show, Some(&closed)));
    }

    #[test]
    fn an_episode_page_names_its_show_and_place() {
        let mut d = ItemDetail::default();
        d.summary.kind = ItemKind::Episode;
        d.summary.title = "Glasshouse".into();
        d.summary.grandparent_title = Some("Severance".into());
        d.summary.parent_index = Some(2);
        d.summary.index = Some(5);
        assert_eq!(hero_title(&d), "Severance");
        assert_eq!(context_line(&d).as_deref(), Some("S2 · E5 · Glasshouse"));
        let mut s = ItemDetail::default();
        s.summary.kind = ItemKind::Season;
        s.summary.index = Some(0);
        s.summary.parent_title = Some("Severance".into());
        assert_eq!((hero_title(&s), context_line(&s)), ("Severance".to_string(), Some("Specials".to_string())));
    }
}
