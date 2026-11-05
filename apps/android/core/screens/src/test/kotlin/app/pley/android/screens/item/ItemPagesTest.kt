package app.pley.android.screens.item

import app.pley.android.data.Loadable
import app.pley.android.model.Credit
import app.pley.android.model.CreditRole
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.model.UserState
import app.pley.android.screens.inEnglish
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Item pages as both interfaces show them; on Robolectric, to read their words in English. */
@RunWith(RobolectricTestRunner::class)
class ItemPagesTest {
    private fun page(detail: ItemDetail, input: PageInput.() -> PageInput = { this }) =
        itemPage(PageInput(detail = detail, origin = ORIGIN, locale = Locale.US).input())

    /** [meta] with its words as a screen shows them in English. */
    private fun shown(meta: List<MetaPart>): List<Any?> = meta.map { if (it is MetaPart.Text) it.text.inEnglish() else it }

    @Test
    fun `a movie not started plays, with no From start`() {
        val p = page(movie())
        assertEquals(PrimaryAction.Kind.Play, p.primary.kind)
        assertFalse(p.fromStart)
        assertNull(p.primary.episode)
    }

    @Test
    fun `a movie in progress resumes, with what is left and From start`() {
        val p = page(movie(positionMs = 3_480_000, durationMs = 9_060_000))
        assertEquals(PrimaryAction.Kind.Resume, p.primary.kind)
        assertTrue(p.fromStart)
        assertEquals("1h 33m left", p.primary.timeLeft)
        assertEquals(3_480_000f / 9_060_000f, p.primary.progress, 0.001f)
    }

    @Test
    fun `a watched movie plays again rather than resuming`() {
        val p = page(movie(positionMs = 3_480_000, watched = true))
        assertEquals(PrimaryAction.Kind.Play, p.primary.kind)
        assertTrue(p.watched)
    }

    @Test
    fun `a movie without files is unavailable`() {
        assertEquals(PrimaryAction.Kind.Unavailable, page(movie(files = emptyList())).primary.kind)
    }

    @Test
    fun `the metadata line carries the year, runtime, rating and short badges`() {
        val meta = page(movie()).meta
        assertEquals(
            listOf(
                MetaPart.Text("2021"),
                MetaPart.Text("2h 31m"),
                MetaPart.ContentRating("PG-13"),
                MetaPart.Rating("8.1"),
                MetaPart.Badges(listOf("4K", "7.1")),
            ),
            meta,
        )
    }

    @Test
    fun `tracks use the web's labels, and without a pick the audio is automatic in its language`() {
        val tracks = page(movie()).tracks!!
        assertEquals(listOf("English (TrueHD 7.1)", "French (AC3 5.1)"), tracks.audio.map { it.label })
        // Each device plays its best English track: none is named.
        assertTrue(tracks.automaticAudio)
        assertEquals("English", tracks.automaticLanguage)
        assertNull(tracks.audioIndex)
        assertNull(tracks.audioLabel)
        assertTrue(tracks.audio.none { it.selected })
        assertNull(tracks.subtitleIndex)
        assertEquals(listOf("English (SRT)"), tracks.subtitles.map { it.label })
        assertTrue(tracks.versions.isEmpty())
    }

    @Test
    fun `a picked track is named and ticked, and Automatic picked here names none`() {
        val streams = arrayOf(stream(1, StreamKind.Audio, "truehd", "eng", default = true, channels = 8), stream(2, StreamKind.Audio, "ac3", "fra", channels = 6))
        val picked = movie(files = listOf(file(11, *streams, selectedAudio = 2, audioPicked = true)))
        val tracks = page(picked).tracks!!
        assertFalse(tracks.automaticAudio)
        assertEquals(2, tracks.audioIndex)
        assertEquals("French (AC3 5.1)", tracks.audioLabel)
        val automatic = page(picked) { copy(pick = StreamPick(audioFile = 11, audio = null)) }.tracks!!
        assertTrue(automatic.automaticAudio)
        assertNull(automatic.audioIndex)
    }

    @Test
    fun `the profile's saved choice wins, and a pick for another version doesn't apply`() {
        val withChoice = movie(
            files = listOf(
                file(11, stream(1, StreamKind.Audio, "aac", "eng", default = true), stream(2, StreamKind.Audio, "ac3", "fra"), stream(3, StreamKind.Subtitle, "subrip", "eng"), selectedAudio = 2, selectedSubtitle = 3, audioPicked = true),
                file(12, stream(1, StreamKind.Audio, "aac", "eng", default = true), stream(2, StreamKind.Audio, "ac3", "deu")),
            ),
        )
        val first = page(withChoice).tracks!!
        assertEquals(2, first.audioIndex)
        assertEquals(3, first.subtitleIndex)
        assertEquals(2, first.versions.size)
        val other = page(withChoice) { copy(pick = StreamPick(fileId = 12, audioFile = 11, audio = 1)) }.tracks!!
        assertEquals(12L, other.versionId)
        // The second version has no saved choice: automatic, in its default track's language.
        assertNull(other.audioIndex)
        assertTrue(other.automaticAudio)
        assertEquals("English", other.automaticLanguage)
    }

    @Test
    fun `nothing to choose gives no selects`() {
        val single = movie(files = listOf(file(11, stream(0, StreamKind.Video, "h264"), stream(1, StreamKind.Audio, "aac", "eng", default = true))))
        assertNull(page(single).tracks)
    }

    @Test
    fun `a show plays its next episode, labelled with it`() {
        val show = ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Driftline", leafCount = 24, childCount = 3)
        val next = summary(21, ItemKind.Episode, parentId = 30, parentIndex = 1, index = 3)
        val p = page(show) { copy(onDeck = Loadable.Ready(next, true)) }
        assertEquals(PrimaryAction.Kind.Play, p.primary.kind)
        assertEquals("S1 · E3", p.primary.episode)
        val resuming = page(show) { copy(onDeck = Loadable.Ready(next.copy(userState = UserState(positionMs = 1_000)), true)) }
        assertEquals(PrimaryAction.Kind.Resume, resuming.primary.kind)
        assertTrue(resuming.fromStart)
    }

    @Test
    fun `a show without episodes is unavailable`() {
        val show = ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Empty", leafCount = 0)
        assertEquals(PrimaryAction.Kind.Unavailable, page(show) { copy(onDeck = Loadable.Ready(null, true)) }.primary.kind)
    }

    @Test
    fun `a show's meta line counts seasons and spans their years`() {
        val show = ItemDetail(id = 10, libraryId = 2, kind = ItemKind.Show, title = "Driftline", year = 2021, childCount = 3, contentRating = "TV-14")
        val seasons = listOf(2021, 2023, 2025).mapIndexed { i, y -> summary(30L + i, ItemKind.Season, index = i + 1).copy(year = y) }
        assertEquals(
            listOf("3 seasons", "2021-2025", MetaPart.ContentRating("TV-14")),
            shown(page(show) { copy(seasons = seasons) }.meta),
        )
    }

    @Test
    fun `episodes get Guest Stars, their show and season, and the season's row`() {
        val episode = ItemDetail(
            id = 21, libraryId = 2, kind = ItemKind.Episode, title = "Glasshouse", index = 5, parentId = 30, parentIndex = 2,
            grandparentId = 10, grandparentTitle = "Driftline",
        )
        val p = page(episode) { copy(selectedSeason = 30, episodes = listOf(summary(21, ItemKind.Episode, index = 5, parentIndex = 2))) }
        assertEquals(CastTitle.GuestStars, p.castTitle)
        assertEquals("Driftline", p.context?.showTitle)
        assertEquals("S2 · E5", p.context?.episodeLabel)
        assertEquals("Season 2 · Episode 5", p.context?.episodeLongLabel)
        assertEquals(21L, p.seasons?.focus)
        assertEquals("5. Item 21", p.seasons?.episodes?.single()?.title)
    }

    @Test
    fun `crew lines stop at four names, then say how many more`() {
        val writers = (1L..6L).map { Credit(it, "Writer $it", CreditRole.Writer) }
        val p = page(movie().copy(crew = writers + Credit(9, "Producer", CreditRole.Producer)))
        val line = p.details.crew.single()
        assertEquals(CreditRole.Writer, line.role)
        assertEquals(4, line.people.size)
        assertEquals(2, line.more)
        assertTrue(line.plural)
    }

    @Test
    fun `media info shows file names only`() {
        val f = file(11, MediaStream(0, StreamKind.Video, "hevc")).copy(path = "/srv/media/Movies/Saltwind (2021)/Saltwind.mkv")
        val info = page(movie(files = listOf(f))).mediaInfo.single()
        assertEquals(InfoRow(InfoLabel.File, "Saltwind.mkv"), info.rows.first())
        assertEquals("Saltwind.mkv", fileName("C:\\media\\Saltwind.mkv"))
    }

    @Test
    fun `the cast lists each person once`() {
        val cast = listOf(Credit(1, "A", CreditRole.Actor, "One"), Credit(1, "A", CreditRole.Actor, "Two"), Credit(2, "B", CreditRole.Actor))
        assertEquals(listOf(1L, 2L), page(movie().copy(cast = cast)).cast.map { it.id })
    }

    @Test
    fun `a show opens on its next episode's season, else the first that isn't Specials`() {
        val seasons = listOf(summary(30, ItemKind.Season, index = 0), summary(31, ItemKind.Season, index = 1), summary(32, ItemKind.Season, index = 2))
        assertEquals(32L, defaultSeason(seasons, summary(40, ItemKind.Episode, parentId = 32)))
        assertEquals(31L, defaultSeason(seasons, null))
    }

    @Test
    fun `extras say what they are and how long`() {
        val trailer = summary(5, ItemKind.Extra, durationMs = 120_000).copy(extraType = app.pley.android.model.ExtraType.Trailer)
        assertEquals("Trailer · 2m", extraSubtitle(trailer).inEnglish())
        assertEquals("Extra", extraSubtitle(summary(6, ItemKind.Extra, durationMs = null)).inEnglish())
    }
}
