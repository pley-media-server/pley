package app.pley.android.fixtures

/**
 * The words the generated titles, names and summaries are made of. Made up,
 * so no generated title is a real film's, and fixed, so every run of the
 * FixtureServer has the same catalog.
 */
internal object Words {
    val adjectives = listOf(
        "Silent", "Golden", "Broken", "Hidden", "Last", "Crimson", "Distant", "Electric", "Frozen", "Wild",
        "Quiet", "Burning", "Hollow", "Velvet", "Iron", "Paper", "Midnight", "Northern", "Secret", "Lucky",
        "Little", "Endless", "Bright", "Dark", "Lost", "Final", "Savage", "Gentle", "Rapid", "Lonely",
    )

    val nouns = listOf(
        "Harbor", "Signal", "Garden", "River", "Empire", "Mirror", "Station", "Orchard", "Frontier", "Lantern",
        "Voyage", "Machine", "Island", "Kingdom", "Echo", "Horizon", "Archive", "Canyon", "Circus", "Meridian",
        "Summit", "Tide", "Engine", "Atlas", "Comet", "Valley", "Fortress", "Carnival", "Harvest", "Labyrinth",
    )

    val showSuffixes = listOf(
        "Chronicles", "Files", "Diaries", "Stories", "Club", "Street", "Academy", "Report", "Nights", "Line",
    )

    val firstNames = listOf(
        "Ada", "Bruno", "Clara", "Dmitri", "Elena", "Felix", "Grace", "Hugo", "Iris", "Jonas",
        "Kaito", "Lena", "Marco", "Nadia", "Oscar", "Priya", "Quentin", "Rosa", "Samir", "Tess",
        "Umar", "Vera", "Wes", "Ximena", "Yusuf", "Zoe", "Amara", "Bastien", "Chiara", "Dario",
    )

    val lastNames = listOf(
        "Abbott", "Brandt", "Castillo", "Duval", "Eriksen", "Fontaine", "Gallo", "Hart", "Ito", "Janssen",
        "Kowalski", "Lindqvist", "Moreau", "Novak", "Okafor", "Petrov", "Quinn", "Rossi", "Sato", "Tanaka",
    )

    val genres = listOf(
        "Action", "Adventure", "Animation", "Comedy", "Crime", "Documentary", "Drama", "Family", "Fantasy",
        "History", "Horror", "Mystery", "Romance", "Science Fiction", "Thriller", "War", "Western",
    )

    val musicGenres = listOf("Ambient", "Electronic", "Folk", "Jazz", "Pop", "Rock", "Soul")

    val studios = listOf(
        "Northlight Pictures", "Paper Crane Films", "Meridian Studios", "Blue Hour", "Ironwood", "Tidewater",
    )

    val movieRatings = listOf("G", "PG", "PG-13", "R")
    val showRatings = listOf("TV-Y7", "TV-PG", "TV-14", "TV-MA")

    /** Two or three sentences about [title], the same every time. */
    fun summary(title: String, seed: Int): String {
        val a = adjectives[seed % adjectives.size].lowercase()
        val n = nouns[(seed / 3) % nouns.size].lowercase()
        val m = nouns[(seed / 7 + 5) % nouns.size].lowercase()
        return "In $title, a $a $n changes everything for the people of the $m. " +
            "Nothing here is a real film: the FixtureServer made it up."
    }
}
