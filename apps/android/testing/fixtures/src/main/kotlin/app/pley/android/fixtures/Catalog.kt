package app.pley.android.fixtures

import app.pley.android.model.Chapter
import app.pley.android.model.CollectionSort
import app.pley.android.model.Credit
import app.pley.android.model.CreditRole
import app.pley.android.model.DynamicRange
import app.pley.android.model.ExtraType
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind
import app.pley.android.model.MarkerSource
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import kotlin.random.Random

/** "Now" for the generated data, in unix seconds: added and released dates count back from it. */
internal const val BASE_TIME: Long = 1_790_000_000L

/** The two committed clips (scripts/make-android-fixtures.ps1) every file streams. */
internal enum class Clip(val resource: String, val container: String, val contentType: String, val durationMs: Long) {
    Sdr1080("sample-1080p.mp4", "mp4", "video/mp4", 20_000),
    Hdr2160("sample-2160p-hdr10.mkv", "mkv", "video/x-matroska", 20_020),
}

/** A file of an item, and the clip it streams. */
internal class FileEntry(val id: Long, val itemId: Long, val clip: Clip)

internal class PersonEntry(val id: Long, val name: String, val hasPhoto: Boolean) {
    val photoUrl: String? get() = if (hasPhoto) "/api/people/$id/photo?v=$BASE_TIME" else null
}

internal class CollectionEntry(
    val id: Long,
    val libraryId: Long,
    val title: String,
    val summary: String?,
    val automatic: Boolean,
    val sort: CollectionSort,
    val smartFilter: String?,
    val itemIds: List<Long>,
    val createdAt: Long,
)

/**
 * The FixtureServer's catalog, made the same on every run: three
 * libraries, their items with files, streams, chapters, markers, extras and
 * credits, the people credited, and the collections. What changes with use
 * (watched marks, positions, playlists) is in [Household], not here.
 */
internal class Catalog(private val sizes: Map<Clip, Long>) {
    val libraries: List<Library>
    val items = LinkedHashMap<Long, ItemDetail>()

    /** Seasons of a show, episodes of a season, albums of an artist, tracks of an album: by index. */
    val children = HashMap<Long, MutableList<Long>>()

    /** Movies, shows or artists of each library, in the order they were made. */
    val topLevel = HashMap<Long, MutableList<Long>>()
    val files = HashMap<Long, FileEntry>()
    val people = LinkedHashMap<Long, PersonEntry>()

    /** What each person is credited in (movies and shows). */
    val credits = HashMap<Long, MutableList<Long>>()
    val collections = LinkedHashMap<Long, CollectionEntry>()
    val itemCollections = HashMap<Long, MutableList<Long>>()

    private val random = Random(7539)

    init {
        makePeople()
        makeMovies()
        makeShows()
        makeMusic()
        makeCollections()
        libraries = listOf(
            library(FixtureIds.MOVIES, "Movies", LibraryKind.Movies),
            library(FixtureIds.SHOWS, "TV Shows", LibraryKind.Shows),
            library(FixtureIds.MUSIC, "Music", LibraryKind.Music),
        )
    }

    fun item(id: Long): ItemDetail? = items[id]

    fun file(id: Long): Pair<FileEntry, MediaFile>? {
        val entry = files[id] ?: return null
        val file = items[entry.itemId]?.files?.firstOrNull { it.id == id } ?: return null
        return entry to file
    }

    /** The playable items under [id], in playing order: itself when it plays. */
    fun leaves(id: Long): List<Long> {
        val item = items[id] ?: return emptyList()
        if (item.kind.playable) return listOf(id)
        return children[id].orEmpty().flatMap(::leaves)
    }

    private fun library(id: Long, name: String, kind: LibraryKind) = Library(
        id = id,
        name = name,
        kind = kind,
        itemCount = topLevel[id].orEmpty().size,
        createdAt = BASE_TIME - 400 * DAY,
        scannedAt = BASE_TIME - 3600,
    )

    // ---------------------------------------------------------------- people

    private fun makePeople() {
        var id = 1L
        for (last in Words.lastNames) {
            for (first in Words.firstNames) {
                if (people.size >= 400) return
                people[id] = PersonEntry(id, "$first $last", hasPhoto = id % 7 != 0L)
                id++
            }
        }
    }

    private fun cast(itemId: Long, count: Int, seed: Int): List<Credit> = (0 until count).map { n ->
        val person = people.getValue(((seed * 31L + n * 17L) % people.size) + 1)
        credits.getOrPut(person.id) { mutableListOf() }.add(itemId)
        Credit(person.id, person.name, CreditRole.Actor, character = "${Words.firstNames[(seed + n) % Words.firstNames.size]} ${Words.nouns[(seed + n * 3) % Words.nouns.size]}", photoUrl = person.photoUrl)
    }.distinctBy { it.id }

    private fun crew(itemId: Long, seed: Int, roles: List<CreditRole>): List<Credit> = roles.mapIndexed { n, role ->
        val person = people.getValue(((seed * 13L + n * 101L + 7) % people.size) + 1)
        credits.getOrPut(person.id) { mutableListOf() }.add(itemId)
        Credit(person.id, person.name, role, photoUrl = person.photoUrl)
    }

    // ---------------------------------------------------------------- movies

    private fun makeMovies() {
        val movies = topLevel.getOrPut(FixtureIds.MOVIES) { mutableListOf() }
        var extraId = 2_001L
        for (i in 0 until 1_500) {
            val id = i + 1L
            val seed = i * 7 + 3
            val sample = sampleMovie(id)
            val title = sample?.first ?: movieTitle(i)
            val year = if (sample != null) 2026 else 1950 + random.nextInt(77)
            val added = BASE_TIME - i * 5 * HOUR - random.nextLong(3 * HOUR)
            val duration = sample?.second?.durationMs ?: ((80 + random.nextInt(90)) * MINUTE)
            val genres = pick(Words.genres, 1 + random.nextInt(3), seed)
            val files = movieFiles(id, title, year, duration, i, sample?.second)
            val extras = if (i % 10 == 3) {
                val extra = extraItem(extraId++, id, title, added)
                listOf(extra)
            } else {
                emptyList()
            }
            val detail = ItemDetail(
                id = id,
                libraryId = FixtureIds.MOVIES,
                kind = ItemKind.Movie,
                title = title,
                sortTitle = sortTitle(title),
                year = year,
                durationMs = duration,
                rating = (40 + random.nextInt(56)) / 10.0,
                addedAt = added,
                posterUrl = image(id, "poster", added),
                backdropUrl = image(id, "backdrop", added),
                thumbUrl = image(id, "thumb", added),
                summary = Words.summary(title, seed),
                releaseDate = date(year, seed),
                tagline = "${Words.adjectives[seed % 30]} and ${Words.adjectives[(seed + 11) % 30].lowercase()}.",
                genres = genres,
                studio = Words.studios[seed % Words.studios.size],
                contentRating = Words.movieRatings[seed % Words.movieRatings.size],
                tmdbId = 500_000L + id,
                imdbId = "tt%07d".format(9_000_000 + id),
                files = files,
                markers = movieMarkers(files, id),
                extras = extras.map { it.toSummary() },
                cast = cast(id, 6 + seed % 5, seed),
                crew = crew(id, seed, listOf(CreditRole.Director, CreditRole.Writer, CreditRole.Producer)),
            )
            items[id] = detail
            extras.forEach { items[it.id] = it }
            movies += id
        }
    }

    /** The named samples: their title and the clip they play exactly. */
    private fun sampleMovie(id: Long): Pair<String, Clip>? = when (id) {
        FixtureIds.SAMPLE_1080P -> "Sample 1080p" to Clip.Sdr1080
        FixtureIds.SAMPLE_HDR10 -> "Sample 2160p HDR10" to Clip.Hdr2160
        FixtureIds.DOLBY_VISION_P5 -> "Sample Dolby Vision Profile 5" to Clip.Hdr2160
        FixtureIds.DOLBY_VISION_P7 -> "Sample Dolby Vision Profile 7" to Clip.Hdr2160
        FixtureIds.DOLBY_VISION_P81 -> "Sample Dolby Vision Profile 8.1" to Clip.Hdr2160
        FixtureIds.HDR10_PLUS -> "Sample HDR10+" to Clip.Hdr2160
        else -> null
    }

    private fun movieTitle(i: Int): String {
        val adjective = Words.adjectives[i % 30]
        val noun = Words.nouns[(i / 30) % 30]
        return when (i / 900) {
            0 -> "The $adjective $noun"
            else -> "$noun of the $adjective ${Words.nouns[(i / 30 + 7) % 30]}"
        }
    }

    private fun movieFiles(id: Long, title: String, year: Int, duration: Long, i: Int, sample: Clip?): List<MediaFile> {
        val name = "$title ($year)"
        val first = when {
            sample == Clip.Sdr1080 -> sdrFile(id + FILE_BASE, "$name.mp4", duration, exact = true, subtitles = false)
            sample != null -> hdrFile(id + FILE_BASE, "$name.mkv", duration, exact = true, range = sampleRange(id))
            i % 10 < 7 -> sdrFile(id + FILE_BASE, "$name.mp4", duration, exact = false, subtitles = i % 3 == 0)
            else -> hdrFile(id + FILE_BASE, "$name.mkv", duration, exact = false, range = Range.Hdr10)
        }
        register(first.id, id, if (first.container == "mp4") Clip.Sdr1080 else Clip.Hdr2160)
        // Every 50th movie has a second version.
        if (sample != null || i % 50 != 7) return listOf(first)
        val second = hdrFile(id + 2 * FILE_BASE, "$name {edition-Director's Cut}.mkv", duration + 9 * MINUTE, exact = false, range = Range.Hdr10)
            .copy(edition = "Director's Cut")
        register(second.id, id, Clip.Hdr2160)
        return listOf(first, second)
    }

    private fun sampleRange(id: Long): Range = when (id) {
        FixtureIds.DOLBY_VISION_P5 -> Range.DolbyVision5
        FixtureIds.DOLBY_VISION_P7 -> Range.DolbyVision7
        FixtureIds.DOLBY_VISION_P81 -> Range.DolbyVision81
        FixtureIds.HDR10_PLUS -> Range.Hdr10Plus
        else -> Range.Hdr10
    }

    private fun movieMarkers(files: List<MediaFile>, id: Long): List<Marker> = files.flatMap { file ->
        val duration = file.durationMs ?: return@flatMap emptyList()
        when {
            id == FixtureIds.SAMPLE_HDR10 -> listOf(
                Marker(file.id, MarkerKind.Intro, 0, 4_000, MarkerSource.Chapter),
                Marker(file.id, MarkerKind.Credits, 16_000, duration, MarkerSource.Chapter),
            )
            file.container == "mkv" && duration > 10 * MINUTE ->
                listOf(Marker(file.id, MarkerKind.Credits, duration - 6 * MINUTE, duration, MarkerSource.Frames))
            else -> emptyList()
        }
    }

    private fun extraItem(id: Long, movie: Long, title: String, added: Long): ItemDetail {
        val file = sdrFile(id + FILE_BASE, "$title-trailer.mp4", Clip.Sdr1080.durationMs, exact = true, subtitles = false)
        register(file.id, id, Clip.Sdr1080)
        return ItemDetail(
            id = id,
            libraryId = FixtureIds.MOVIES,
            kind = ItemKind.Extra,
            title = "Trailer",
            sortTitle = "Trailer",
            parentId = movie,
            parentTitle = title,
            durationMs = file.durationMs,
            addedAt = added,
            thumbUrl = image(id, "thumb", added),
            extraType = ExtraType.Trailer,
            files = listOf(file),
        )
    }

    // ---------------------------------------------------------------- shows

    private fun makeShows() {
        val shows = topLevel.getOrPut(FixtureIds.SHOWS) { mutableListOf() }
        var seasonId = FixtureIds.SHOW_SEASON_1
        var episodeId = FixtureIds.SHOW_S1E1
        for (i in 0 until 300) {
            val id = FixtureIds.SHOW + i
            val seed = i * 11 + 5
            val fixture = id == FixtureIds.SHOW
            val title = if (fixture) "Fixture Show" else "${Words.nouns[i % 30]} ${Words.showSuffixes[(i / 30) % 10]}"
            val year = if (fixture) 2025 else 1990 + random.nextInt(36)
            val added = BASE_TIME - i * 11 * HOUR - random.nextLong(5 * HOUR)
            val seasons = if (fixture) 2 else 1 + random.nextInt(4)
            val genres = pick(Words.genres, 1 + random.nextInt(2), seed)
            val seasonIds = mutableListOf<Long>()
            var leafCount = 0
            for (s in 1..seasons) {
                val sid = seasonId++
                seasonIds += sid
                val episodes = if (fixture) 3 else 4 + random.nextInt(7)
                val episodeIds = mutableListOf<Long>()
                for (e in 1..episodes) {
                    val eid = episodeId++
                    episodeIds += eid
                    items[eid] = episode(eid, sid, id, title, s, e, year, added, fixture, seed + e)
                }
                leafCount += episodes
                children[sid] = episodeIds
                items[sid] = ItemDetail(
                    id = sid,
                    libraryId = FixtureIds.SHOWS,
                    kind = ItemKind.Season,
                    title = "Season $s",
                    sortTitle = "Season $s",
                    year = year + s - 1,
                    index = s,
                    parentId = id,
                    parentTitle = title,
                    addedAt = added,
                    posterUrl = image(sid, "poster", added),
                    backdropUrl = image(id, "backdrop", added),
                    leafCount = episodes,
                    summary = "Season $s of $title.",
                    releaseDate = date(year + s - 1, seed + s),
                    genres = genres,
                    contentRating = Words.showRatings[seed % 4],
                )
            }
            children[id] = seasonIds
            items[id] = ItemDetail(
                id = id,
                libraryId = FixtureIds.SHOWS,
                kind = ItemKind.Show,
                title = title,
                sortTitle = sortTitle(title),
                year = year,
                rating = (50 + random.nextInt(46)) / 10.0,
                addedAt = added,
                posterUrl = image(id, "poster", added),
                backdropUrl = image(id, "backdrop", added),
                leafCount = leafCount,
                childCount = seasons,
                summary = Words.summary(title, seed),
                releaseDate = date(year, seed),
                genres = genres,
                studio = Words.studios[seed % Words.studios.size],
                contentRating = Words.showRatings[seed % 4],
                tmdbId = 800_000L + i,
                cast = cast(id, 5 + seed % 4, seed),
                crew = crew(id, seed, listOf(CreditRole.Creator)),
            )
            shows += id
        }
    }

    private fun episode(
        id: Long,
        season: Long,
        show: Long,
        showTitle: String,
        s: Int,
        e: Int,
        year: Int,
        added: Long,
        fixture: Boolean,
        seed: Int,
    ): ItemDetail {
        val duration = if (fixture) Clip.Sdr1080.durationMs else (22 + random.nextInt(37)) * MINUTE
        val file = sdrFile(id + FILE_BASE, "$showTitle - S%02dE%02d.mp4".format(s, e), duration, exact = fixture, subtitles = e % 2 == 0)
        register(file.id, id, Clip.Sdr1080)
        val markers = if (fixture) {
            listOf(
                Marker(file.id, MarkerKind.Intro, FixtureIds.INTRO_START_MS, FixtureIds.INTRO_END_MS, MarkerSource.Fingerprint),
                Marker(file.id, MarkerKind.Credits, FixtureIds.CREDITS_START_MS, duration, MarkerSource.Frames),
            )
        } else {
            listOf(
                Marker(file.id, MarkerKind.Intro, 60_000, 115_000, MarkerSource.Fingerprint),
                Marker(file.id, MarkerKind.Credits, duration - 90_000, duration, MarkerSource.Fingerprint),
            )
        }
        val episodeAdded = added - (100 - s * 10 - e) * HOUR
        return ItemDetail(
            id = id,
            libraryId = FixtureIds.SHOWS,
            kind = ItemKind.Episode,
            title = if (fixture) "Episode $e" else "The ${Words.adjectives[(seed + e) % 30]} ${Words.nouns[(seed * 3 + s) % 30]}",
            sortTitle = "Episode $e",
            year = year + s - 1,
            index = e,
            parentId = season,
            parentTitle = "Season $s",
            parentIndex = s,
            grandparentId = show,
            grandparentTitle = showTitle,
            durationMs = duration,
            rating = (60 + random.nextInt(35)) / 10.0,
            addedAt = episodeAdded,
            posterUrl = image(season, "poster", added),
            backdropUrl = image(show, "backdrop", added),
            thumbUrl = image(id, "thumb", episodeAdded),
            summary = Words.summary(showTitle, seed + e * 13),
            releaseDate = date(year + s - 1, seed + e),
            files = listOf(file),
            markers = markers,
            cast = if (e % 3 == 0) cast(id, 1, seed + 97) else emptyList(),
        )
    }

    // ---------------------------------------------------------------- music

    private fun makeMusic() {
        val artists = topLevel.getOrPut(FixtureIds.MUSIC) { mutableListOf() }
        var albumId = FixtureIds.ALBUM
        var trackId = FixtureIds.TRACK
        for (i in 0 until 50) {
            val id = FixtureIds.ARTIST + i
            val name = "${Words.adjectives[(i * 7) % 30]} ${Words.nouns[(i * 11 + 3) % 30]}s"
            val added = BASE_TIME - i * 19 * HOUR
            val genre = Words.musicGenres[i % Words.musicGenres.size]
            val albumIds = mutableListOf<Long>()
            var trackCount = 0
            for (a in 1..4) {
                val aid = albumId++
                albumIds += aid
                val year = 1975 + (i * 4 + a) % 50
                val albumTitle = "${Words.nouns[(i + a * 7) % 30]} ${listOf("Sessions", "Songs", "Lights", "Days")[a - 1]}"
                val exact = aid == FixtureIds.ALBUM
                val trackIds = mutableListOf<Long>()
                for (t in 1..10) {
                    val tid = trackId++
                    trackIds += tid
                    val duration = if (exact) Clip.Sdr1080.durationMs else (120 + random.nextInt(300)) * 1_000L
                    val file = audioFile(tid + FILE_BASE, "%02d - %s.m4a".format(t, Words.adjectives[(t + a) % 30]), duration)
                    register(file.id, tid, Clip.Sdr1080)
                    items[tid] = ItemDetail(
                        id = tid,
                        libraryId = FixtureIds.MUSIC,
                        kind = ItemKind.Track,
                        title = "${Words.adjectives[(t * 3 + a) % 30]} ${Words.nouns[(t * 5 + i) % 30]}",
                        sortTitle = "%02d".format(t),
                        year = year,
                        index = t,
                        parentId = aid,
                        parentTitle = albumTitle,
                        parentIndex = 1,
                        grandparentId = id,
                        grandparentTitle = name,
                        durationMs = duration,
                        addedAt = added,
                        posterUrl = image(aid, "poster", added),
                        files = listOf(file),
                    )
                }
                trackCount += trackIds.size
                children[aid] = trackIds
                items[aid] = ItemDetail(
                    id = aid,
                    libraryId = FixtureIds.MUSIC,
                    kind = ItemKind.Album,
                    title = albumTitle,
                    sortTitle = albumTitle,
                    year = year,
                    index = a,
                    parentId = id,
                    parentTitle = name,
                    addedAt = added - a * HOUR,
                    posterUrl = image(aid, "poster", added),
                    leafCount = trackIds.size,
                    releaseDate = date(year, a),
                    genres = listOf(genre),
                )
            }
            children[id] = albumIds
            items[id] = ItemDetail(
                id = id,
                libraryId = FixtureIds.MUSIC,
                kind = ItemKind.Artist,
                title = name,
                sortTitle = sortTitle(name),
                addedAt = added,
                posterUrl = image(id, "poster", added),
                backdropUrl = image(id, "backdrop", added),
                leafCount = trackCount,
                childCount = albumIds.size,
                summary = "$name play ${genre.lowercase()} that the FixtureServer made up.",
                genres = listOf(genre),
            )
            artists += id
        }
    }

    // ---------------------------------------------------------------- collections

    private fun makeCollections() {
        val movies = topLevel.getValue(FixtureIds.MOVIES)
        add(
            CollectionEntry(
                id = FixtureIds.SMART_COLLECTION,
                libraryId = FixtureIds.MOVIES,
                title = "Comedies of the 1990s",
                summary = "Every comedy of the library made in the 1990s.",
                automatic = false,
                sort = CollectionSort.Release,
                smartFilter = SMART_COLLECTION_FILTER,
                itemIds = emptyList(),
                createdAt = BASE_TIME - 30 * DAY,
            ),
        )
        for (c in 0 until 24) {
            val id = FixtureIds.COLLECTION + c
            val members = (0 until 3 + c % 4).map { n -> movies[(c * 53 + n * 211 + 6) % movies.size] }.distinct()
            add(
                CollectionEntry(
                    id = id,
                    libraryId = FixtureIds.MOVIES,
                    title = "The ${Words.nouns[(c * 7 + 2) % 30]} Collection",
                    summary = if (c % 2 == 0) "Films about a ${Words.nouns[c % 30].lowercase()}." else null,
                    automatic = c % 2 == 0,
                    sort = if (c % 3 == 0) CollectionSort.Custom else CollectionSort.Release,
                    smartFilter = null,
                    itemIds = members,
                    createdAt = BASE_TIME - (40 + c) * DAY,
                ),
            )
        }
        val shows = topLevel.getValue(FixtureIds.SHOWS)
        for (c in 0 until 4) {
            val id = FixtureIds.COLLECTION + 24 + c
            add(
                CollectionEntry(
                    id = id,
                    libraryId = FixtureIds.SHOWS,
                    title = "${Words.adjectives[c * 5]} Series",
                    summary = null,
                    automatic = false,
                    sort = CollectionSort.Title,
                    smartFilter = null,
                    itemIds = (0 until 5).map { n -> shows[(c * 31 + n * 17) % shows.size] }.distinct(),
                    createdAt = BASE_TIME - 20 * DAY,
                ),
            )
        }
    }

    private fun add(collection: CollectionEntry) {
        collections[collection.id] = collection
        collection.itemIds.forEach { itemCollections.getOrPut(it) { mutableListOf() }.add(collection.id) }
    }

    // ---------------------------------------------------------------- files

    /** How a generated video's picture is described. */
    enum class Range { Hdr10, Hdr10Plus, DolbyVision5, DolbyVision7, DolbyVision81 }

    private fun register(fileId: Long, itemId: Long, clip: Clip) {
        files[fileId] = FileEntry(fileId, itemId, clip)
    }

    private fun sdrFile(id: Long, name: String, duration: Long, exact: Boolean, subtitles: Boolean): MediaFile {
        val bitrate = if (exact) sizes.getValue(Clip.Sdr1080) * 8_000 / duration else 6_000_000L + (id % 7) * 500_000
        val streams = buildList {
            add(
                MediaStream(
                    index = 0, kind = StreamKind.Video, codec = "h264", default = true, profile = "High",
                    width = 1920, height = 1080, frameRate = 24.0, bitDepth = 8, pixFmt = "yuv420p", level = 40,
                    interlaced = false, hdr = false, dynamicRange = DynamicRange.Sdr,
                    colorTransfer = "bt709", colorPrimaries = "bt709", colorSpace = "bt709",
                    bitrate = bitrate - 96_000,
                ),
            )
            add(
                MediaStream(
                    index = 1, kind = StreamKind.Audio, codec = "aac", language = "eng", default = true, profile = "LC",
                    channels = 2, channelLayout = "stereo", sampleRate = 48_000, bitrate = 96_000,
                ),
            )
            if (subtitles) {
                add(MediaStream(index = 2, kind = StreamKind.Subtitle, codec = "mov_text", language = "eng", textBased = true))
            }
        }
        return MediaFile(
            id = id,
            path = name,
            size = if (exact) sizes.getValue(Clip.Sdr1080) else bitrate * duration / 8_000,
            container = "mp4",
            durationMs = duration,
            bitrate = bitrate,
            width = 1920,
            height = 1080,
            videoCodec = "h264",
            audioCodec = "aac",
            streams = streams,
        )
    }

    private fun hdrFile(id: Long, name: String, duration: Long, exact: Boolean, range: Range): MediaFile {
        val bitrate = if (exact) sizes.getValue(Clip.Hdr2160) * 8_000 / duration else 24_000_000L + (id % 5) * 2_000_000
        val dv = when (range) {
            Range.DolbyVision5 -> Triple(5, 0, null)
            Range.DolbyVision7 -> Triple(7, 6, "smpte2084")
            Range.DolbyVision81 -> Triple(8, 1, "smpte2084")
            else -> null
        }
        val dynamicRange = when (range) {
            Range.Hdr10 -> DynamicRange.Hdr10
            Range.Hdr10Plus -> DynamicRange.Hdr10Plus
            else -> DynamicRange.DolbyVision
        }
        val video = MediaStream(
            index = 0, kind = StreamKind.Video, codec = "hevc", default = true, profile = "Main 10",
            width = 3840, height = 2160, frameRate = 23.976, bitDepth = 10, pixFmt = "yuv420p10le", level = 150,
            interlaced = false, hdr = true, dynamicRange = dynamicRange,
            dvProfile = dv?.first, dvLevel = dv?.let { 6 }, dvBlCompat = dv?.second,
            colorTransfer = if (range == Range.DolbyVision5) null else "smpte2084",
            colorPrimaries = if (range == Range.DolbyVision5) null else "bt2020",
            colorSpace = if (range == Range.DolbyVision5) null else "bt2020nc",
            bitrate = bitrate - 640_000,
        )
        val streams = buildList {
            add(video)
            add(
                MediaStream(
                    index = 1, kind = StreamKind.Audio, codec = "ac3", language = "eng", default = true,
                    channels = 6, channelLayout = "5.1(side)", sampleRate = 48_000, bitrate = 256_000,
                ),
            )
            if (exact) {
                add(MediaStream(index = 2, kind = StreamKind.Subtitle, codec = "subrip", language = "eng", title = "English", textBased = true))
                add(MediaStream(index = 1000, kind = StreamKind.Subtitle, codec = "subrip", language = "fre", textBased = true, external = true))
            } else {
                add(
                    MediaStream(
                        index = 2, kind = StreamKind.Audio, codec = "eac3", language = "fre",
                        channels = 6, channelLayout = "5.1(side)", sampleRate = 48_000, bitrate = 384_000,
                    ),
                )
                add(MediaStream(index = 3, kind = StreamKind.Subtitle, codec = "subrip", language = "eng", textBased = true))
                add(MediaStream(index = 4, kind = StreamKind.Subtitle, codec = "hdmv_pgs_subtitle", language = "fre", textBased = false))
            }
        }
        val chapters = if (exact) {
            listOf(
                Chapter(0, 4_000, "Intro", chapterThumb(id, 0)),
                Chapter(4_000, 16_000, "Part 1", chapterThumb(id, 1)),
                Chapter(16_000, duration, "Credits", chapterThumb(id, 2)),
            )
        } else {
            val step = duration / 8
            (0 until 8).map { n -> Chapter(n * step, if (n == 7) duration else (n + 1) * step, "Chapter ${n + 1}", chapterThumb(id, n)) }
        }
        return MediaFile(
            id = id,
            path = name,
            size = if (exact) sizes.getValue(Clip.Hdr2160) else bitrate * duration / 8_000,
            container = "mkv",
            durationMs = duration,
            bitrate = bitrate,
            width = 3840,
            height = 2160,
            videoCodec = "hevc",
            audioCodec = "ac3",
            streams = streams,
            chapters = chapters,
        )
    }

    private fun audioFile(id: Long, name: String, duration: Long) = MediaFile(
        id = id,
        path = name,
        size = 256_000L * duration / 8_000,
        container = "mp4",
        durationMs = duration,
        bitrate = 256_000,
        audioCodec = "aac",
        streams = listOf(
            MediaStream(
                index = 0, kind = StreamKind.Audio, codec = "aac", default = true, profile = "LC",
                channels = 2, channelLayout = "stereo", sampleRate = 48_000, bitrate = 256_000,
            ),
        ),
    )

    private fun chapterThumb(fileId: Long, n: Int) = "/api/files/$fileId/chapters/$n.jpg?v=${trickplayId(fileId)}"

    // ---------------------------------------------------------------- helpers

    private fun <T> pick(from: List<T>, count: Int, seed: Int): List<T> =
        (0 until count).map { from[(seed + it * 5) % from.size] }.distinct()

    companion object {
        const val MINUTE: Long = 60_000
        const val HOUR: Long = 3_600
        const val DAY: Long = 86_400

        /** File ids: an item's first file is [FILE_BASE] + its id, a second version twice that. */
        const val FILE_BASE: Long = 1_000_000

        /** The smart collection's filter, in the Plex-compatible API's syntax. */
        const val SMART_COLLECTION_FILTER = "genre=Comedy&decade=1990"

        /** The seek previews' version of a file: the `v` of its sheets and chapter pictures. */
        fun trickplayId(fileId: Long): String = "tp$fileId"

        fun image(id: Long, kind: String, version: Long) = "/api/items/$id/images/$kind?v=$version"

        /** A title as it sorts: without "The " or "A ". */
        fun sortTitle(title: String): String = title.removePrefix("The ").removePrefix("A ")

        fun date(year: Int, seed: Int): String = "%04d-%02d-%02d".format(year, 1 + seed % 12, 1 + seed % 28)
    }
}
