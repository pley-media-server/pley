//! The catalogue, made the same on every run (a port of the Android app's
//! fixture server's `Catalog`, with the same seed, so both have the same
//! titles): three libraries, their items with files, streams, chapters,
//! markers, extras and credits, the people credited, and the collections.
//! What changes with use (watched marks, positions, playlists) is in the
//! household, not here.

use std::collections::HashMap;

use crate::ids;
use crate::media::Clip;
use crate::model::{Chapter, Credit, ItemDetail, Kind, Library, Marker, MediaFile, MediaStream};
use crate::random::KotlinRandom;
use crate::words::{
    ADJECTIVES, FIRST_NAMES, GENRES, LAST_NAMES, MOVIE_RATINGS, MUSIC_GENRES, NOUNS, SHOW_RATINGS, SHOW_SUFFIXES,
    STUDIOS, summary,
};

/// "Now" for the generated data, in unix seconds: added and released dates
/// count back from it.
pub(crate) const BASE_TIME: i64 = 1_790_000_000;
pub(crate) const MINUTE_MS: i64 = 60_000;
pub(crate) const HOUR: i64 = 3_600;
pub(crate) const DAY: i64 = 86_400;
/// File ids: an item's first file is this plus its id, a second version twice
/// that.
pub(crate) const FILE_BASE: i64 = 1_000_000;
/// The smart collection's filter, in the compatibility API's syntax.
pub(crate) const SMART_COLLECTION_FILTER: &str = "genre=Comedy&decade=1990";
/// How many people are credited.
const PEOPLE: i64 = 400;

/// A file and the clip it streams.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FileEntry {
    pub id: i64,
    pub item_id: i64,
    pub clip: Clip,
}

/// A person credited in titles.
#[derive(Clone, Debug)]
pub(crate) struct Person {
    pub id: i64,
    pub name: String,
    pub has_photo: bool,
}

impl Person {
    pub fn photo_url(&self) -> Option<String> {
        self.has_photo.then(|| format!("/api/people/{}/photo?v={BASE_TIME}", self.id))
    }
}

/// A collection; a smart one finds its items with its filter.
#[derive(Clone, Debug)]
pub(crate) struct Collection {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    pub summary: Option<String>,
    pub automatic: bool,
    /// `release`, `title` or `custom`.
    pub sort: &'static str,
    pub smart_filter: Option<String>,
    pub item_ids: Vec<i64>,
    pub created_at: i64,
}

/// How a generated video's picture is described.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Range {
    Hdr10,
    Hdr10Plus,
    DolbyVision5,
    DolbyVision7,
    DolbyVision81,
}

/// The fixture server's catalogue.
pub(crate) struct Catalog {
    pub libraries: Vec<Library>,
    items: Vec<ItemDetail>,
    index: HashMap<i64, usize>,
    /// Seasons of a show, episodes of a season, albums of an artist, tracks
    /// of an album: by index.
    children: HashMap<i64, Vec<i64>>,
    /// Movies, shows or artists of each library, in the order they were made.
    top_level: HashMap<i64, Vec<i64>>,
    files: HashMap<i64, FileEntry>,
    people: Vec<Person>,
    /// What each person is credited in (movies, shows, episodes).
    pub credits: HashMap<i64, Vec<i64>>,
    collections: Vec<Collection>,
    item_collections: HashMap<i64, Vec<i64>>,
    random: KotlinRandom,
    sdr_size: i64,
    hdr_size: i64,
}

impl Catalog {
    /// The catalogue, with the clips' sizes in bytes.
    pub fn new(sdr_size: i64, hdr_size: i64) -> Catalog {
        let mut catalog = Catalog {
            libraries: Vec::new(),
            items: Vec::new(),
            index: HashMap::new(),
            children: HashMap::new(),
            top_level: HashMap::new(),
            files: HashMap::new(),
            people: Vec::new(),
            credits: HashMap::new(),
            collections: Vec::new(),
            item_collections: HashMap::new(),
            random: KotlinRandom::new(7539),
            sdr_size,
            hdr_size,
        };
        catalog.make_people();
        catalog.make_movies();
        catalog.make_shows();
        catalog.make_music();
        catalog.make_collections();
        catalog.libraries = vec![
            catalog.make_library(ids::MOVIES, "Movies", "movies"),
            catalog.make_library(ids::SHOWS, "TV Shows", "shows"),
            catalog.make_library(ids::MUSIC, "Music", "music"),
        ];
        catalog
    }

    pub fn item(&self, id: i64) -> Option<&ItemDetail> {
        self.index.get(&id).map(|&i| &self.items[i])
    }

    /// Every item, in the order they were made.
    pub fn items(&self) -> impl Iterator<Item = &ItemDetail> {
        self.items.iter()
    }

    pub fn library(&self, id: i64) -> Option<&Library> {
        self.libraries.iter().find(|l| l.id == id)
    }

    /// A library's movies, shows or artists.
    pub fn top(&self, library: i64) -> Vec<&ItemDetail> {
        self.top_level.get(&library).map(|ids| ids.iter().filter_map(|id| self.item(*id)).collect()).unwrap_or_default()
    }

    pub fn top_ids(&self, library: i64) -> &[i64] {
        self.top_level.get(&library).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn children(&self, id: i64) -> &[i64] {
        self.children.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// A file and the item it belongs to's description of it.
    pub fn file(&self, id: i64) -> Option<(FileEntry, &MediaFile)> {
        let entry = *self.files.get(&id)?;
        let file = self.item(entry.item_id)?.files.iter().find(|f| f.id == id)?;
        Some((entry, file))
    }

    /// The playable items under `id`, in playing order: itself when it plays.
    pub fn leaves(&self, id: i64) -> Vec<i64> {
        let Some(item) = self.item(id) else { return Vec::new() };
        if item.kind().playable() {
            return vec![id];
        }
        self.children(id).iter().flat_map(|child| self.leaves(*child)).collect()
    }

    pub fn person(&self, id: i64) -> Option<&Person> {
        usize::try_from(id - 1).ok().and_then(|i| self.people.get(i))
    }

    pub fn people(&self) -> &[Person] {
        &self.people
    }

    pub fn collection(&self, id: i64) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }

    pub fn collections(&self) -> &[Collection] {
        &self.collections
    }

    /// The collections an item is in.
    pub fn collections_of(&self, item: i64) -> Vec<&Collection> {
        self.item_collections
            .get(&item)
            .map(|ids| ids.iter().filter_map(|id| self.collection(*id)).collect())
            .unwrap_or_default()
    }

    fn insert(&mut self, item: ItemDetail) {
        self.index.insert(item.id(), self.items.len());
        self.items.push(item);
    }

    fn register(&mut self, file: i64, item: i64, clip: Clip) {
        self.files.insert(file, FileEntry { id: file, item_id: item, clip });
    }

    fn make_library(&self, id: i64, name: &str, kind: &'static str) -> Library {
        Library {
            id,
            name: name.to_string(),
            kind,
            paths: Vec::new(),
            item_count: self.top_ids(id).len() as i64,
            created_at: BASE_TIME - 400 * DAY,
            scanned_at: Some(BASE_TIME - 3_600),
            scanning: false,
        }
    }

    fn make_people(&mut self) {
        let mut id = 1;
        for last in LAST_NAMES {
            for first in FIRST_NAMES {
                if self.people.len() as i64 >= PEOPLE {
                    return;
                }
                self.people.push(Person { id, name: format!("{first} {last}"), has_photo: id % 7 != 0 });
                id += 1;
            }
        }
    }

    fn cast(&mut self, item: i64, count: usize, seed: usize) -> Vec<Credit> {
        let mut out: Vec<Credit> = Vec::new();
        for n in 0..count {
            let pid = (seed as i64 * 31 + n as i64 * 17) % PEOPLE + 1;
            self.credits.entry(pid).or_default().push(item);
            let person = &self.people[pid as usize - 1];
            if out.iter().any(|c| c.id == pid) {
                continue;
            }
            out.push(Credit {
                id: pid,
                name: person.name.clone(),
                role: "actor",
                character: Some(format!("{} {}", FIRST_NAMES[(seed + n) % 30], NOUNS[(seed + n * 3) % 30])),
                photo_url: person.photo_url(),
            });
        }
        out
    }

    fn crew(&mut self, item: i64, seed: usize, roles: &[&'static str]) -> Vec<Credit> {
        let mut out = Vec::new();
        for (n, role) in roles.iter().enumerate() {
            let pid = (seed as i64 * 13 + n as i64 * 101 + 7) % PEOPLE + 1;
            self.credits.entry(pid).or_default().push(item);
            let person = &self.people[pid as usize - 1];
            out.push(Credit {
                id: pid,
                name: person.name.clone(),
                role,
                character: None,
                photo_url: person.photo_url(),
            });
        }
        out
    }

    fn make_movies(&mut self) {
        let mut movies = Vec::new();
        let mut extra_id = 2_001;
        for i in 0..1_500usize {
            let id = i as i64 + 1;
            let seed = i * 7 + 3;
            let sample = sample_movie(id);
            let title = sample.map(|(t, _)| t.to_string()).unwrap_or_else(|| movie_title(i));
            let year = if sample.is_some() { 2026 } else { 1950 + self.random.below(77) as i64 };
            let added = BASE_TIME - i as i64 * 5 * HOUR - self.random.below_long(3 * HOUR);
            let duration = match sample {
                Some((_, clip)) => clip.duration_ms(),
                None => (80 + self.random.below(90) as i64) * MINUTE_MS,
            };
            let genre_count = 1 + self.random.below(3) as usize;
            let genres = pick(&GENRES, genre_count, seed);
            let files = self.movie_files(id, &title, year, duration, i, sample.map(|(_, c)| c));
            let extras = if i % 10 == 3 {
                let extra = self.extra_item(extra_id, id, &title, added);
                extra_id += 1;
                vec![extra]
            } else {
                Vec::new()
            };
            let rating = (40 + self.random.below(56)) as f64 / 10.0;
            let mut detail = ItemDetail::new(id, ids::MOVIES, Kind::Movie, &title, sort_title(&title), added);
            let b = &mut detail.base;
            b.year = Some(year);
            b.duration_ms = Some(duration);
            b.rating = Some(rating);
            b.poster_url = Some(image(id, "poster", added));
            b.backdrop_url = Some(image(id, "backdrop", added));
            b.thumb_url = Some(image(id, "thumb", added));
            b.summary = Some(summary(&title, seed));
            b.release_date = Some(date(year, seed));
            detail.tagline =
                Some(format!("{} and {}.", ADJECTIVES[seed % 30], ADJECTIVES[(seed + 11) % 30].to_lowercase()));
            detail.genres = genres;
            detail.studio = Some(STUDIOS[seed % STUDIOS.len()].to_string());
            detail.content_rating = Some(MOVIE_RATINGS[seed % MOVIE_RATINGS.len()].to_string());
            detail.tmdb_id = Some(500_000 + id);
            detail.imdb_id = Some(format!("tt{:07}", 9_000_000 + id));
            detail.markers = movie_markers(&files, id);
            detail.files = files;
            detail.extras = extras.iter().map(|e| e.base.clone()).collect();
            detail.cast = self.cast(id, 6 + seed % 5, seed);
            detail.crew = self.crew(id, seed, &["director", "writer", "producer"]);
            self.insert(detail);
            for extra in extras {
                self.insert(extra);
            }
            movies.push(id);
        }
        self.top_level.insert(ids::MOVIES, movies);
    }

    fn movie_files(
        &mut self,
        id: i64,
        title: &str,
        year: i64,
        duration: i64,
        i: usize,
        sample: Option<Clip>,
    ) -> Vec<MediaFile> {
        let name = format!("{title} ({year})");
        let first = match sample {
            Some(Clip::Sdr1080) => self.sdr_file(id + FILE_BASE, &format!("{name}.mp4"), duration, true, false),
            Some(Clip::Hdr2160) => {
                self.hdr_file(id + FILE_BASE, &format!("{name}.mkv"), duration, true, sample_range(id))
            }
            None if i % 10 < 7 => {
                self.sdr_file(id + FILE_BASE, &format!("{name}.mp4"), duration, false, i.is_multiple_of(3))
            }
            None => self.hdr_file(id + FILE_BASE, &format!("{name}.mkv"), duration, false, Range::Hdr10),
        };
        let clip = if first.container.as_deref() == Some("mp4") { Clip::Sdr1080 } else { Clip::Hdr2160 };
        self.register(first.id, id, clip);
        // Every 50th movie has a second version.
        if sample.is_some() || i % 50 != 7 {
            return vec![first];
        }
        let mut second = self.hdr_file(
            id + 2 * FILE_BASE,
            &format!("{name} {{edition-Director's Cut}}.mkv"),
            duration + 9 * MINUTE_MS,
            false,
            Range::Hdr10,
        );
        second.edition = Some("Director's Cut".to_string());
        self.register(second.id, id, Clip::Hdr2160);
        let mut files = vec![first, second];
        // The server lists the tallest first, then by id.
        files.sort_by_key(|f| (-f.height.unwrap_or(0), f.id));
        files
    }

    fn extra_item(&mut self, id: i64, movie: i64, title: &str, added: i64) -> ItemDetail {
        let file =
            self.sdr_file(id + FILE_BASE, &format!("{title}-trailer.mp4"), Clip::Sdr1080.duration_ms(), true, false);
        self.register(file.id, id, Clip::Sdr1080);
        let mut extra = ItemDetail::new(id, ids::MOVIES, Kind::Extra, "Trailer", "Trailer", added);
        extra.base.parent_id = Some(movie);
        extra.base.parent_title = Some(title.to_string());
        extra.base.duration_ms = file.duration_ms;
        extra.base.thumb_url = Some(image(id, "thumb", added));
        extra.base.extra_type = Some("trailer");
        extra.files = vec![file];
        extra
    }

    fn make_shows(&mut self) {
        let mut shows = Vec::new();
        let mut season_id = ids::SHOW_SEASON_1;
        let mut episode_id = ids::SHOW_S1E1;
        for i in 0..300usize {
            let id = ids::SHOW + i as i64;
            let seed = i * 11 + 5;
            let fixture = id == ids::SHOW;
            let title = if fixture {
                "Fixture Show".to_string()
            } else {
                format!("{} {}", NOUNS[i % 30], SHOW_SUFFIXES[(i / 30) % 10])
            };
            let year = if fixture { 2025 } else { 1990 + self.random.below(36) as i64 };
            let added = BASE_TIME - i as i64 * 11 * HOUR - self.random.below_long(5 * HOUR);
            let seasons = if fixture { 2 } else { 1 + self.random.below(4) as usize };
            let genre_count = 1 + self.random.below(2) as usize;
            let genres = pick(&GENRES, genre_count, seed);
            let content_rating = SHOW_RATINGS[seed % 4].to_string();
            let mut season_ids = Vec::new();
            let mut leaf_count = 0;
            for s in 1..=seasons {
                let sid = season_id;
                season_id += 1;
                season_ids.push(sid);
                let episodes = if fixture { 3 } else { 4 + self.random.below(7) as usize };
                let mut episode_ids = Vec::new();
                for e in 1..=episodes {
                    let eid = episode_id;
                    episode_id += 1;
                    episode_ids.push(eid);
                    let place = EpisodePlace { season: sid, show: id, s, e, year, added, fixture };
                    let episode = self.episode(eid, &place, &title, seed + e);
                    self.insert(episode);
                }
                leaf_count += episodes;
                self.children.insert(sid, episode_ids);
                let season_title = format!("Season {s}");
                let mut season = ItemDetail::new(sid, ids::SHOWS, Kind::Season, &season_title, &season_title, added);
                let b = &mut season.base;
                b.year = Some(year + s as i64 - 1);
                b.index = Some(s as i64);
                b.parent_id = Some(id);
                b.parent_title = Some(title.clone());
                b.poster_url = Some(image(sid, "poster", added));
                b.backdrop_url = Some(image(id, "backdrop", added));
                b.leaf_count = Some(episodes as i64);
                b.summary = Some(format!("Season {s} of {title}."));
                b.release_date = Some(date(year + s as i64 - 1, seed + s));
                season.genres = genres.clone();
                season.content_rating = Some(content_rating.clone());
                self.insert(season);
            }
            self.children.insert(id, season_ids);
            let rating = (50 + self.random.below(46)) as f64 / 10.0;
            let mut show = ItemDetail::new(id, ids::SHOWS, Kind::Show, &title, sort_title(&title), added);
            let b = &mut show.base;
            b.year = Some(year);
            b.rating = Some(rating);
            b.poster_url = Some(image(id, "poster", added));
            b.backdrop_url = Some(image(id, "backdrop", added));
            b.leaf_count = Some(leaf_count as i64);
            b.child_count = Some(seasons as i64);
            b.summary = Some(summary(&title, seed));
            b.release_date = Some(date(year, seed));
            show.genres = genres;
            show.studio = Some(STUDIOS[seed % STUDIOS.len()].to_string());
            show.content_rating = Some(content_rating);
            show.tmdb_id = Some(800_000 + i as i64);
            show.cast = self.cast(id, 5 + seed % 4, seed);
            show.crew = self.crew(id, seed, &["creator"]);
            self.insert(show);
            shows.push(id);
        }
        self.top_level.insert(ids::SHOWS, shows);
    }

    fn episode(&mut self, id: i64, place: &EpisodePlace, show_title: &str, seed: usize) -> ItemDetail {
        let EpisodePlace { season, show, s, e, year, added, fixture } = *place;
        let duration =
            if fixture { Clip::Sdr1080.duration_ms() } else { (22 + self.random.below(37) as i64) * MINUTE_MS };
        let file =
            self.sdr_file(id + FILE_BASE, &format!("{show_title} - S{s:02}E{e:02}.mp4"), duration, fixture, e % 2 == 0);
        self.register(file.id, id, Clip::Sdr1080);
        let markers = if fixture {
            vec![
                marker(file.id, "intro", ids::INTRO_START_MS, ids::INTRO_END_MS, "fingerprint"),
                marker(file.id, "credits", ids::CREDITS_START_MS, duration, "frames"),
            ]
        } else {
            vec![
                marker(file.id, "intro", 60_000, 115_000, "fingerprint"),
                marker(file.id, "credits", duration - 90_000, duration, "fingerprint"),
            ]
        };
        let episode_added = added - (100 - s as i64 * 10 - e as i64) * HOUR;
        let title = if fixture {
            format!("Episode {e}")
        } else {
            format!("The {} {}", ADJECTIVES[(seed + e) % 30], NOUNS[(seed * 3 + s) % 30])
        };
        let rating = (60 + self.random.below(35)) as f64 / 10.0;
        let mut item = ItemDetail::new(id, ids::SHOWS, Kind::Episode, &title, &format!("Episode {e}"), episode_added);
        let b = &mut item.base;
        b.year = Some(year + s as i64 - 1);
        b.index = Some(e as i64);
        b.parent_id = Some(season);
        b.parent_title = Some(format!("Season {s}"));
        b.parent_index = Some(s as i64);
        b.grandparent_id = Some(show);
        b.grandparent_title = Some(show_title.to_string());
        b.duration_ms = Some(duration);
        b.rating = Some(rating);
        b.poster_url = Some(image(season, "poster", added));
        b.backdrop_url = Some(image(show, "backdrop", added));
        b.thumb_url = Some(image(id, "thumb", episode_added));
        b.summary = Some(summary(show_title, seed + e * 13));
        b.release_date = Some(date(year + s as i64 - 1, seed + e));
        item.files = vec![file];
        item.markers = markers;
        item.cast = if e % 3 == 0 { self.cast(id, 1, seed + 97) } else { Vec::new() };
        item
    }

    fn make_music(&mut self) {
        let mut artists = Vec::new();
        let mut album_id = ids::ALBUM;
        let mut track_id = ids::TRACK;
        const ALBUM_WORDS: [&str; 4] = ["Sessions", "Songs", "Lights", "Days"];
        for i in 0..50usize {
            let id = ids::ARTIST + i as i64;
            let name = format!("{} {}s", ADJECTIVES[(i * 7) % 30], NOUNS[(i * 11 + 3) % 30]);
            let added = BASE_TIME - i as i64 * 19 * HOUR;
            let genre = MUSIC_GENRES[i % MUSIC_GENRES.len()];
            let mut album_ids = Vec::new();
            let mut track_count = 0;
            for a in 1..=4usize {
                let aid = album_id;
                album_id += 1;
                album_ids.push(aid);
                let year = 1975 + ((i * 4 + a) % 50) as i64;
                let album_title = format!("{} {}", NOUNS[(i + a * 7) % 30], ALBUM_WORDS[a - 1]);
                let exact = aid == ids::ALBUM;
                let mut track_ids = Vec::new();
                for t in 1..=10usize {
                    let tid = track_id;
                    track_id += 1;
                    track_ids.push(tid);
                    let duration =
                        if exact { Clip::Sdr1080.duration_ms() } else { (120 + self.random.below(300) as i64) * 1_000 };
                    let file =
                        audio_file(tid + FILE_BASE, &format!("{t:02} - {}.m4a", ADJECTIVES[(t + a) % 30]), duration);
                    self.register(file.id, tid, Clip::Sdr1080);
                    let title = format!("{} {}", ADJECTIVES[(t * 3 + a) % 30], NOUNS[(t * 5 + i) % 30]);
                    let mut track = ItemDetail::new(tid, ids::MUSIC, Kind::Track, &title, &format!("{t:02}"), added);
                    let b = &mut track.base;
                    b.year = Some(year);
                    b.index = Some(t as i64);
                    b.parent_id = Some(aid);
                    b.parent_title = Some(album_title.clone());
                    b.parent_index = Some(1);
                    b.grandparent_id = Some(id);
                    b.grandparent_title = Some(name.clone());
                    b.duration_ms = Some(duration);
                    b.poster_url = Some(image(aid, "poster", added));
                    track.files = vec![file];
                    self.insert(track);
                }
                track_count += track_ids.len();
                let leaves = track_ids.len() as i64;
                self.children.insert(aid, track_ids);
                let mut album =
                    ItemDetail::new(aid, ids::MUSIC, Kind::Album, &album_title, &album_title, added - a as i64 * HOUR);
                let b = &mut album.base;
                b.year = Some(year);
                b.index = Some(a as i64);
                b.parent_id = Some(id);
                b.parent_title = Some(name.clone());
                b.poster_url = Some(image(aid, "poster", added));
                b.leaf_count = Some(leaves);
                b.release_date = Some(date(year, a));
                album.genres = vec![genre.to_string()];
                self.insert(album);
            }
            self.children.insert(id, album_ids);
            let mut artist = ItemDetail::new(id, ids::MUSIC, Kind::Artist, &name, sort_title(&name), added);
            let b = &mut artist.base;
            b.poster_url = Some(image(id, "poster", added));
            b.backdrop_url = Some(image(id, "backdrop", added));
            b.leaf_count = Some(track_count as i64);
            b.child_count = Some(4);
            b.summary = Some(format!("{name} play {} that Pley's fixture server made up.", genre.to_lowercase()));
            artist.genres = vec![genre.to_string()];
            self.insert(artist);
            artists.push(id);
        }
        self.top_level.insert(ids::MUSIC, artists);
    }

    fn make_collections(&mut self) {
        let movies = self.top_ids(ids::MOVIES).to_vec();
        self.add_collection(Collection {
            id: ids::SMART_COLLECTION,
            library_id: ids::MOVIES,
            title: "Comedies of the 1990s".to_string(),
            summary: Some("Every comedy of the library made in the 1990s.".to_string()),
            automatic: false,
            sort: "release",
            smart_filter: Some(SMART_COLLECTION_FILTER.to_string()),
            item_ids: Vec::new(),
            created_at: BASE_TIME - 30 * DAY,
        });
        for c in 0..24usize {
            let mut members = Vec::new();
            for n in 0..3 + c % 4 {
                let member = movies[(c * 53 + n * 211 + 6) % movies.len()];
                if !members.contains(&member) {
                    members.push(member);
                }
            }
            self.add_collection(Collection {
                id: ids::COLLECTION + c as i64,
                library_id: ids::MOVIES,
                title: format!("The {} Collection", NOUNS[(c * 7 + 2) % 30]),
                summary: (c % 2 == 0).then(|| format!("Films about a {}.", NOUNS[c % 30].to_lowercase())),
                automatic: c % 2 == 0,
                sort: if c % 3 == 0 { "custom" } else { "release" },
                smart_filter: None,
                item_ids: members,
                created_at: BASE_TIME - (40 + c as i64) * DAY,
            });
        }
        let shows = self.top_ids(ids::SHOWS).to_vec();
        for c in 0..4usize {
            let mut members = Vec::new();
            for n in 0..5 {
                let member = shows[(c * 31 + n * 17) % shows.len()];
                if !members.contains(&member) {
                    members.push(member);
                }
            }
            self.add_collection(Collection {
                id: ids::COLLECTION + 24 + c as i64,
                library_id: ids::SHOWS,
                title: format!("{} Series", ADJECTIVES[c * 5]),
                summary: None,
                automatic: false,
                sort: "title",
                smart_filter: None,
                item_ids: members,
                created_at: BASE_TIME - 20 * DAY,
            });
        }
    }

    fn add_collection(&mut self, collection: Collection) {
        for item in &collection.item_ids {
            self.item_collections.entry(*item).or_default().push(collection.id);
        }
        self.collections.push(collection);
    }

    fn sdr_file(&self, id: i64, name: &str, duration: i64, exact: bool, subtitles: bool) -> MediaFile {
        let bitrate = if exact { self.sdr_size * 8_000 / duration } else { 6_000_000 + (id % 7) * 500_000 };
        let mut streams = vec![
            MediaStream {
                index: 0,
                kind: "video",
                codec: "h264".into(),
                default: true,
                profile: Some("High".into()),
                width: Some(1920),
                height: Some(1080),
                frame_rate: Some(24.0),
                bit_depth: Some(8),
                pix_fmt: Some("yuv420p".into()),
                level: Some(40),
                interlaced: Some(false),
                hdr: Some(false),
                dynamic_range: Some("sdr"),
                color_transfer: Some("bt709".into()),
                color_primaries: Some("bt709".into()),
                color_space: Some("bt709".into()),
                bitrate: Some(bitrate - 96_000),
                ..MediaStream::default()
            },
            audio_stream(1, "aac", Some("eng"), true, Some("LC"), 2, "stereo", 96_000),
        ];
        if subtitles {
            streams.push(subtitle_stream(2, "mov_text", Some("eng"), None, true, false));
        }
        MediaFile {
            id,
            path: name.to_string(),
            size: if exact { self.sdr_size } else { bitrate * duration / 8_000 },
            container: Some("mp4".into()),
            duration_ms: Some(duration),
            bitrate: Some(bitrate),
            width: Some(1920),
            height: Some(1080),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            streams,
            edition: None,
            selected_audio: None,
            audio_picked: false,
            selected_subtitle: None,
            chapters: Vec::new(),
        }
    }

    fn hdr_file(&self, id: i64, name: &str, duration: i64, exact: bool, range: Range) -> MediaFile {
        let bitrate = if exact { self.hdr_size * 8_000 / duration } else { 24_000_000 + (id % 5) * 2_000_000 };
        let dv = match range {
            Range::DolbyVision5 => Some((5, 0)),
            Range::DolbyVision7 => Some((7, 6)),
            Range::DolbyVision81 => Some((8, 1)),
            Range::Hdr10 | Range::Hdr10Plus => None,
        };
        let dynamic_range = match range {
            Range::Hdr10 => "hdr10",
            Range::Hdr10Plus => "hdr10plus",
            _ => "dolby_vision",
        };
        // Profile 5 has no HDR10 base layer, so no PQ or BT.2020 tags.
        let tagged = range != Range::DolbyVision5;
        let video = MediaStream {
            index: 0,
            kind: "video",
            codec: "hevc".into(),
            default: true,
            profile: Some("Main 10".into()),
            width: Some(3840),
            height: Some(2160),
            frame_rate: Some(23.976),
            bit_depth: Some(10),
            pix_fmt: Some("yuv420p10le".into()),
            level: Some(150),
            interlaced: Some(false),
            hdr: Some(true),
            dynamic_range: Some(dynamic_range),
            dv_profile: dv.map(|d| d.0),
            dv_level: dv.map(|_| 6),
            dv_bl_compat: dv.map(|d| d.1),
            hdr10plus: (range == Range::Hdr10Plus).then_some(true),
            color_transfer: tagged.then(|| "smpte2084".into()),
            color_primaries: tagged.then(|| "bt2020".into()),
            color_space: tagged.then(|| "bt2020nc".into()),
            bitrate: Some(bitrate - 640_000),
            ..MediaStream::default()
        };
        let mut streams = vec![video, audio_stream(1, "ac3", Some("eng"), true, None, 6, "5.1(side)", 256_000)];
        if exact {
            streams.push(subtitle_stream(2, "subrip", Some("eng"), Some("English"), true, false));
            streams.push(subtitle_stream(1000, "subrip", Some("fre"), None, true, true));
        } else {
            streams.push(audio_stream(2, "eac3", Some("fre"), false, None, 6, "5.1(side)", 384_000));
            streams.push(subtitle_stream(3, "subrip", Some("eng"), None, true, false));
            streams.push(subtitle_stream(4, "hdmv_pgs_subtitle", Some("fre"), None, false, false));
        }
        let chapters = if exact {
            vec![
                chapter(0, 4_000, "Intro", id, 0),
                chapter(4_000, 16_000, "Part 1", id, 1),
                chapter(16_000, duration, "Credits", id, 2),
            ]
        } else {
            let step = duration / 8;
            (0..8)
                .map(|n| {
                    let end = if n == 7 { duration } else { (n + 1) * step };
                    chapter(n * step, end, &format!("Chapter {}", n + 1), id, n as usize)
                })
                .collect()
        };
        MediaFile {
            id,
            path: name.to_string(),
            size: if exact { self.hdr_size } else { bitrate * duration / 8_000 },
            container: Some("mkv".into()),
            duration_ms: Some(duration),
            bitrate: Some(bitrate),
            width: Some(3840),
            height: Some(2160),
            video_codec: Some("hevc".into()),
            audio_codec: Some("ac3".into()),
            streams,
            edition: None,
            selected_audio: None,
            audio_picked: false,
            selected_subtitle: None,
            chapters,
        }
    }
}

/// Where an episode is, for making it.
#[derive(Clone, Copy)]
struct EpisodePlace {
    season: i64,
    show: i64,
    s: usize,
    e: usize,
    year: i64,
    added: i64,
    fixture: bool,
}

fn audio_file(id: i64, name: &str, duration: i64) -> MediaFile {
    MediaFile {
        id,
        path: name.to_string(),
        size: 256_000 * duration / 8_000,
        container: Some("mp4".into()),
        duration_ms: Some(duration),
        bitrate: Some(256_000),
        width: None,
        height: None,
        video_codec: None,
        audio_codec: Some("aac".into()),
        streams: vec![audio_stream(0, "aac", None, true, Some("LC"), 2, "stereo", 256_000)],
        edition: None,
        selected_audio: None,
        audio_picked: false,
        selected_subtitle: None,
        chapters: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn audio_stream(
    index: i64,
    codec: &str,
    language: Option<&str>,
    default: bool,
    profile: Option<&str>,
    channels: i64,
    layout: &str,
    bitrate: i64,
) -> MediaStream {
    MediaStream {
        index,
        kind: "audio",
        codec: codec.into(),
        language: language.map(Into::into),
        default,
        profile: profile.map(Into::into),
        channels: Some(channels),
        channel_layout: Some(layout.into()),
        sample_rate: Some(48_000),
        bitrate: Some(bitrate),
        ..MediaStream::default()
    }
}

fn subtitle_stream(
    index: i64,
    codec: &str,
    language: Option<&str>,
    title: Option<&str>,
    text_based: bool,
    external: bool,
) -> MediaStream {
    MediaStream {
        index,
        kind: "subtitle",
        codec: codec.into(),
        language: language.map(Into::into),
        title: title.map(Into::into),
        text_based: Some(text_based),
        external,
        ..MediaStream::default()
    }
}

fn chapter(start: i64, end: i64, title: &str, file: i64, n: usize) -> Chapter {
    Chapter {
        start_ms: start,
        end_ms: end,
        title: Some(title.to_string()),
        thumb_url: Some(format!("/api/files/{file}/chapters/{n}.jpg?v={}", trickplay_id(file))),
    }
}

fn marker(file: i64, kind: &'static str, start: i64, end: i64, source: &'static str) -> Marker {
    Marker { file_id: file, kind, start_ms: start, end_ms: end, source }
}

fn movie_markers(files: &[MediaFile], id: i64) -> Vec<Marker> {
    let mut out = Vec::new();
    for file in files {
        let Some(duration) = file.duration_ms else { continue };
        if id == ids::SAMPLE_HDR10 {
            out.push(marker(file.id, "intro", 0, 4_000, "chapter"));
            out.push(marker(file.id, "credits", 16_000, duration, "chapter"));
        } else if file.container.as_deref() == Some("mkv") && duration > 10 * MINUTE_MS {
            out.push(marker(file.id, "credits", duration - 6 * MINUTE_MS, duration, "frames"));
        }
    }
    out
}

/// The named samples: their title and the clip they play exactly.
fn sample_movie(id: i64) -> Option<(&'static str, Clip)> {
    Some(match id {
        ids::SAMPLE_1080P => ("Sample 1080p", Clip::Sdr1080),
        ids::SAMPLE_HDR10 => ("Sample 2160p HDR10", Clip::Hdr2160),
        ids::DOLBY_VISION_P5 => ("Sample Dolby Vision Profile 5", Clip::Hdr2160),
        ids::DOLBY_VISION_P7 => ("Sample Dolby Vision Profile 7", Clip::Hdr2160),
        ids::DOLBY_VISION_P81 => ("Sample Dolby Vision Profile 8.1", Clip::Hdr2160),
        ids::HDR10_PLUS => ("Sample HDR10+", Clip::Hdr2160),
        _ => return None,
    })
}

fn sample_range(id: i64) -> Range {
    match id {
        ids::DOLBY_VISION_P5 => Range::DolbyVision5,
        ids::DOLBY_VISION_P7 => Range::DolbyVision7,
        ids::DOLBY_VISION_P81 => Range::DolbyVision81,
        ids::HDR10_PLUS => Range::Hdr10Plus,
        _ => Range::Hdr10,
    }
}

fn movie_title(i: usize) -> String {
    let adjective = ADJECTIVES[i % 30];
    let noun = NOUNS[(i / 30) % 30];
    if i / 900 == 0 {
        format!("The {adjective} {noun}")
    } else {
        format!("{noun} of the {adjective} {}", NOUNS[(i / 30 + 7) % 30])
    }
}

fn pick(from: &[&str], count: usize, seed: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in 0..count {
        let value = from[(seed + n * 5) % from.len()];
        if !out.iter().any(|v| v == value) {
            out.push(value.to_string());
        }
    }
    out
}

/// The seek previews' version of a file: the `v` of its sheets and chapter
/// pictures.
pub(crate) fn trickplay_id(file: i64) -> String {
    format!("tp{file}")
}

pub(crate) fn image(id: i64, kind: &str, version: i64) -> String {
    format!("/api/items/{id}/images/{kind}?v={version}")
}

/// A title as it sorts: without "The " or "A ".
pub(crate) fn sort_title(title: &str) -> &str {
    let title = title.strip_prefix("The ").unwrap_or(title);
    title.strip_prefix("A ").unwrap_or(title)
}

fn date(year: i64, seed: usize) -> String {
    format!("{year:04}-{:02}-{:02}", 1 + seed % 12, 1 + seed % 28)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::new(1_379_196, 2_502_426)
    }

    #[test]
    fn libraries_hold_what_the_ids_say() {
        let c = catalog();
        assert_eq!(c.top_ids(ids::MOVIES).len(), 1_500);
        assert_eq!(c.top_ids(ids::SHOWS).len(), 300);
        assert_eq!(c.top_ids(ids::MUSIC).len(), 50);
        assert_eq!(c.library(ids::MUSIC).unwrap().item_count, 50);
        assert_eq!(c.people().len(), 400);
        assert_eq!(c.collections().len(), 29);
    }

    #[test]
    fn the_named_items_are_where_the_ids_say() {
        let c = catalog();
        assert_eq!(c.item(ids::SAMPLE_1080P).unwrap().base.title, "Sample 1080p");
        assert_eq!(c.item(ids::SAMPLE_1080P).unwrap().files[0].id, ids::SAMPLE_1080P_FILE);
        assert_eq!(c.item(ids::SAMPLE_HDR10).unwrap().files[0].id, ids::SAMPLE_HDR10_FILE);
        assert_eq!(c.item(ids::SHOW).unwrap().base.title, "Fixture Show");
        assert_eq!(c.children(ids::SHOW), &[ids::SHOW_SEASON_1, ids::SHOW_SEASON_2]);
        assert_eq!(c.children(ids::SHOW_SEASON_1), &[ids::SHOW_S1E1, ids::SHOW_S1E2, ids::SHOW_S1E3]);
        assert_eq!(c.leaves(ids::SHOW)[3], ids::SHOW_S2E1);
        let e1 = c.item(ids::SHOW_S1E1).unwrap();
        assert_eq!(e1.base.duration_ms, Some(20_000));
        assert_eq!(e1.markers[0].start_ms, ids::INTRO_START_MS);
        assert_eq!(c.item(ids::TRACK).unwrap().base.parent_id, Some(ids::ALBUM));
        assert_eq!(c.item(ids::ALBUM).unwrap().base.parent_id, Some(ids::ARTIST));
        assert_eq!(c.item(ids::TRACK).unwrap().base.duration_ms, Some(20_000));
        let pgs = c.item(ids::PGS_MOVIE).unwrap();
        assert_eq!(pgs.files[0].id, ids::PGS_MOVIE_FILE);
        let track = pgs.files[0].streams.iter().find(|s| s.index == ids::PGS_MOVIE_SUBTITLE).unwrap();
        assert_eq!(track.codec, "hdmv_pgs_subtitle");
        let (entry, file) = c.file(ids::SAMPLE_HDR10_FILE).unwrap();
        assert_eq!(entry.clip, Clip::Hdr2160);
        assert_eq!(file.chapters.len(), 3);
    }

    #[test]
    fn the_same_catalogue_every_time() {
        let a = catalog();
        let b = catalog();
        for id in [7, 500, 1_499, ids::SHOW + 17, ids::TRACK + 99] {
            let (x, y) = (a.item(id).unwrap(), b.item(id).unwrap());
            assert_eq!(x.base.title, y.base.title);
            assert_eq!(x.base.added_at, y.base.added_at);
            assert_eq!(x.base.duration_ms, y.base.duration_ms);
        }
    }
}
