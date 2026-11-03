//! The words the generated titles, names and summaries are made of: made up,
//! so no generated title is a real film's, and fixed, so every run has the
//! same catalogue (the Android app's fixture server uses the same ones).

pub(crate) const ADJECTIVES: [&str; 30] = [
    "Silent", "Golden", "Broken", "Hidden", "Last", "Crimson", "Distant", "Electric", "Frozen", "Wild", "Quiet",
    "Burning", "Hollow", "Velvet", "Iron", "Paper", "Midnight", "Northern", "Secret", "Lucky", "Little", "Endless",
    "Bright", "Dark", "Lost", "Final", "Savage", "Gentle", "Rapid", "Lonely",
];

pub(crate) const NOUNS: [&str; 30] = [
    "Harbor",
    "Signal",
    "Garden",
    "River",
    "Empire",
    "Mirror",
    "Station",
    "Orchard",
    "Frontier",
    "Lantern",
    "Voyage",
    "Machine",
    "Island",
    "Kingdom",
    "Echo",
    "Horizon",
    "Archive",
    "Canyon",
    "Circus",
    "Meridian",
    "Summit",
    "Tide",
    "Engine",
    "Atlas",
    "Comet",
    "Valley",
    "Fortress",
    "Carnival",
    "Harvest",
    "Labyrinth",
];

pub(crate) const SHOW_SUFFIXES: [&str; 10] =
    ["Chronicles", "Files", "Diaries", "Stories", "Club", "Street", "Academy", "Report", "Nights", "Line"];

pub(crate) const FIRST_NAMES: [&str; 30] = [
    "Ada", "Bruno", "Clara", "Dmitri", "Elena", "Felix", "Grace", "Hugo", "Iris", "Jonas", "Kaito", "Lena", "Marco",
    "Nadia", "Oscar", "Priya", "Quentin", "Rosa", "Samir", "Tess", "Umar", "Vera", "Wes", "Ximena", "Yusuf", "Zoe",
    "Amara", "Bastien", "Chiara", "Dario",
];

pub(crate) const LAST_NAMES: [&str; 20] = [
    "Abbott",
    "Brandt",
    "Castillo",
    "Duval",
    "Eriksen",
    "Fontaine",
    "Gallo",
    "Hart",
    "Ito",
    "Janssen",
    "Kowalski",
    "Lindqvist",
    "Moreau",
    "Novak",
    "Okafor",
    "Petrov",
    "Quinn",
    "Rossi",
    "Sato",
    "Tanaka",
];

pub(crate) const GENRES: [&str; 17] = [
    "Action",
    "Adventure",
    "Animation",
    "Comedy",
    "Crime",
    "Documentary",
    "Drama",
    "Family",
    "Fantasy",
    "History",
    "Horror",
    "Mystery",
    "Romance",
    "Science Fiction",
    "Thriller",
    "War",
    "Western",
];

pub(crate) const MUSIC_GENRES: [&str; 7] = ["Ambient", "Electronic", "Folk", "Jazz", "Pop", "Rock", "Soul"];

pub(crate) const STUDIOS: [&str; 6] =
    ["Northlight Pictures", "Paper Crane Films", "Meridian Studios", "Blue Hour", "Ironwood", "Tidewater"];

pub(crate) const MOVIE_RATINGS: [&str; 4] = ["G", "PG", "PG-13", "R"];
pub(crate) const SHOW_RATINGS: [&str; 4] = ["TV-Y7", "TV-PG", "TV-14", "TV-MA"];

/// Two sentences about `title`, the same every time.
pub(crate) fn summary(title: &str, seed: usize) -> String {
    let a = ADJECTIVES[seed % 30].to_lowercase();
    let n = NOUNS[(seed / 3) % 30].to_lowercase();
    let m = NOUNS[(seed / 7 + 5) % 30].to_lowercase();
    format!(
        "In {title}, a {a} {n} changes everything for the people of the {m}. \
         Nothing here is a real film: Pley's fixture server made it up."
    )
}
