package app.pley.android.designsystem.gallery

import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.model.DynamicRange
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaSummary
import app.pley.android.model.UserState
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.formatTimeLeft

/** Made-up titles for the gallery, the design mockups' own. */
internal object GallerySamples {
    private fun poster(name: String) = ArtworkImage(listOf("gallery://poster/$name"))

    private fun still(name: String) = ArtworkImage(listOf("gallery://still/$name"))

    /** A movie through [ArtworkTileModel.of], as screens will build them. */
    private fun movie(id: Long, name: String, title: String, year: Int, watched: Boolean = false, media: MediaSummary? = null) =
        ArtworkTileModel.of(
            ItemSummary(id = id, libraryId = 1, kind = ItemKind.Movie, title = title, year = year, userState = UserState(watched = watched), media = media),
            app.pley.android.designsystem.tile.TileKind.Poster,
            origin = "",
            words = FormatWords.English,
            badges = media != null,
        ).copy(image = poster(name))

    val quietHarbor = movie(1, "quiet-harbor", "The Quiet Harbor", 2023)
    val glassMeridian = movie(2, "glass-meridian", "Glass Meridian", 2022)
    val lowOrbit = movie(3, "low-orbit", "Low Orbit", 2024)
    val halcyon = movie(4, "halcyon", "Halcyon", 2025)
    val northernLights = movie(5, "northern-lights", "Northern Lights", 2024, watched = true)
    val copperSky = movie(6, "copper-sky", "Copper Sky", 2020, media = MediaSummary(width = 3840, height = 2160, dynamicRange = DynamicRange.DolbyVision))
    val violetHour = movie(7, "violet-hour", "Violet Hour", 2021, media = MediaSummary(width = 1920, height = 1080, dynamicRange = DynamicRange.Hdr10Plus))

    /** No poster: the first address 404s and there is no other, so the title's gradient stays. */
    val noPoster = ArtworkTileModel(
        id = 8,
        title = "A Film With No Poster Yet",
        subtitle = "2018",
        image = ArtworkImage(listOf("gallery://missing/a-film")),
        description = "A Film With No Poster Yet, 2018",
    )

    val movies = listOf(lowOrbit, halcyon, northernLights, copperSky, violetHour, quietHarbor, glassMeridian, noPoster)

    private fun show(id: Long, name: String, title: String, seasons: Int, leaves: Int, viewed: Int) = ArtworkTileModel.of(
        ItemSummary(id = id, libraryId = 2, kind = ItemKind.Show, title = title, childCount = seasons, leafCount = leaves, viewedLeafCount = viewed),
        app.pley.android.designsystem.tile.TileKind.Poster,
        origin = "",
        words = FormatWords.English,
    ).copy(image = poster(name))

    val shows = listOf(
        show(20, "overgrowth", "Overgrowth", 2, 16, 10),
        show(21, "redline", "Redline", 1, 8, 8).copy(watched = true),
        show(22, "white-valley", "White Valley", 3, 30, 27),
        show(23, "driftline", "Driftline", 2, 18, 4),
        show(24, "lantern-house", "The Lantern House", 4, 40, 0),
        show(25, "parallel-coast", "Parallel Coast", 4, 36, 30),
    )

    private fun continueMovie(id: Long, name: String, title: String, positionMs: Long, durationMs: Long) = ArtworkTileModel(
        id = id,
        title = title,
        subtitle = formatTimeLeft(positionMs, durationMs, FormatWords.English),
        image = still(name),
        progress = positionMs.toFloat() / durationMs,
        description = title,
    )

    private fun continueEpisode(id: Long, name: String, show: String, season: Int, episode: Int, title: String, positionMs: Long, durationMs: Long) =
        ArtworkTileModel.of(
            ItemSummary(
                id = id, libraryId = 2, kind = ItemKind.Episode, title = title, index = episode, parentIndex = season,
                grandparentTitle = show, durationMs = durationMs, userState = UserState(positionMs = positionMs),
            ),
            app.pley.android.designsystem.tile.TileKind.Landscape,
            origin = "",
            words = FormatWords.English,
        ).copy(image = still(name))

    val continueWatching = listOf(
        continueMovie(30, "ashfall", "Ashfall", 3_000_000, 7_320_000),
        continueEpisode(31, "driftline", "Driftline", 2, 5, "Glasshouse", 1_920_000, 3_480_000),
        continueMovie(32, "saltwind", "Saltwind", 4_400_000, 6_680_000),
        continueEpisode(33, "parallel-coast", "Parallel Coast", 4, 2, "Undertow", 1_200_000, 3_060_000),
        continueEpisode(34, "lantern-house", "The Lantern House", 1, 3, "Glasshouse", 300_000, 2_900_000),
    )

    val episodes = (1..8).map { n ->
        val names = listOf("driftline", "lantern-house", "parallel-coast", "white-valley", "redline", "overgrowth", "saltwind", "ashfall")
        val titles = listOf("Arrival", "Undertow", "Static", "Glasshouse", "Low Tide", "Flare", "Signal", "Homecoming")
        ArtworkTileModel.of(
            ItemSummary(
                id = 100L + n, libraryId = 2, kind = ItemKind.Episode, title = titles[n - 1], index = n, parentIndex = 2,
                grandparentTitle = "Driftline", durationMs = 3_480_000,
                userState = when {
                    n < 4 -> UserState(watched = true)
                    n == 4 -> UserState(positionMs = 1_560_000)
                    else -> UserState()
                },
            ),
            app.pley.android.designsystem.tile.TileKind.Episode,
            origin = "",
            words = FormatWords.English,
        ).copy(image = still(names[n - 1]))
    }

    val albums = listOf("violet-hour", "copper-sky", "glass-meridian", "halcyon", "saltwind", "low-orbit").mapIndexed { i, name ->
        ArtworkTileModel(
            id = 200L + i,
            title = listOf("Night Swim", "Copper", "Meridian", "Halcyon Days", "Salt", "Orbit")[i],
            subtitle = listOf("The Glass Coast · 2021", "Dune Radio · 2019", "Meridian · 2022", "Halcyon · 2024", "Saltwind · 2018", "Low · 2023")[i],
            image = ArtworkImage(listOf("gallery://square/$name")),
        )
    }

    val people = listOf("Lena Okafor", "Idris Vale", "Maren Holt", "Tomas Reyes", "Ama Mensah", "Jun Park", "Sofia Lind").mapIndexed { i, name ->
        ArtworkTileModel(
            id = 300L + i,
            title = name,
            subtitle = listOf("Director", "Kai", "Ines", "Mateo", "Dr. Oduya", "Ren", "Astrid")[i],
            image = if (i % 3 == 2) null else ArtworkImage(listOf("gallery://square/${listOf("quiet-harbor", "violet-hour", "copper-sky", "halcyon", "low-orbit", "saltwind", "redline")[i]}")),
        )
    }
}
