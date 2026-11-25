@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.music

import app.pley.android.common.UiText
import app.pley.android.data.Loadable
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.model.Book
import app.pley.android.model.Chapter
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.MediaFile
import app.pley.android.model.UserState
import app.pley.android.model.format.FormatWords
import app.pley.android.player.NowPlaying
import app.pley.android.screens.R
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** An audiobook's page: what it says of the book, and what Play, a part and a chapter play. */
class BookViewModelTest {
    private val dispatcher = UnconfinedTestDispatcher()
    private val items = FakeItems()
    private val music = FakeMusic()
    private val watch = FakeWatchState()
    private val resolved = mutableListOf<ItemSummary>()

    /** Play's answer: the server's on-deck part, the second, 20 minutes in. */
    private val playItem = PlayItem { item, _ ->
        resolved += item
        PlayTarget.Music(PARTS.map { it.id }, 1, PARTS, startMs = 1_200_000)
    }

    @Before
    fun main() = Dispatchers.setMain(dispatcher)

    @After
    fun reset() = Dispatchers.resetMain()

    private fun TestScope.model() = BookViewModel(items, flowOf(ORIGIN), playItem, music, watch, 70, and = { "and" }, compute = dispatcher).also { vm ->
        backgroundScope.launch { vm.state.collect { } }
        backgroundScope.launch { vm.chapterNow.collect { } }
    }

    @Test
    fun aBookPageSaysWhoWroteAndReadsItItsSeriesAndHowFarItIsHeard() {
        val page = bookPage(BOOK, PARTS, next = PARTS[1], chapters = emptyList(), ORIGIN, "and", FormatWords.English)
        assertEquals(UiText.res(R.string.book_series_place, "The Long Road", "2"), page.kicker)
        assertEquals(UiText.res(R.string.book_by, "A Writer and Another"), page.byline)
        assertEquals(UiText.res(R.string.book_read_by, "A Voice"), page.readBy)
        assertEquals(listOf(UiText.raw("2021"), UiText.raw("3h"), UiText.plural(R.plurals.book_count_parts, 3)), page.metaTexts)
        // The first part heard, 20 minutes of the second: 80 of 180 minutes.
        val progress = page.progress!!
        assertEquals(80f / 180f, progress.fraction, 0.001f)
        assertEquals("1h 40m left", progress.left)
        assertEquals(UiText.res(R.string.book_up_to, "Part 2"), page.upTo)
        assertFalse(page.finished)
        val rows = page.rows.map { it as BookRow.Part }
        assertTrue(rows[0].heard)
        assertEquals(1_200_000f / 3_600_000f, rows[1].fraction, 0.001f)
        assertEquals("1:00:00", rows[2].time)

        // Every part heard: finished, no progress; a book of one file lists its chapters.
        val done = bookPage(BOOK, PARTS.map { it.copy(userState = UserState(watched = true)) }, null, emptyList(), ORIGIN, "and", FormatWords.English)
        assertTrue(done.finished)
        assertNull(done.progress)
        val chapters = listOf(Chapter(0, 60_000, "Opening"), Chapter(60_000, 120_000, null))
        val one = bookPage(BOOK, PARTS.take(1), null, chapters, ORIGIN, "and", FormatWords.English)
        assertEquals(listOf(UiText.raw("Opening"), UiText.res(R.string.book_chapter, 2)), one.rows.map { it.title })
        assertEquals("1:00", one.rows[1].time)
    }

    @Test
    fun namesAreJoinedAsPeopleSayThem() {
        assertEquals("", joinNames(emptyList(), "and"))
        assertEquals("A", joinNames(listOf("A"), "and"))
        assertEquals("A and B", joinNames(listOf("A", "B"), "and"))
        assertEquals("A, B and C", joinNames(listOf("A", "B", "C"), "and"))
    }

    @Test
    fun playGoesOnWhereTheServerSaysAndPlaysOrPausesTheBookInThePlayer() = runTest(dispatcher) {
        val vm = model()
        items.detailOf(70).value = Loadable.Ready(BOOK, fresh = true)
        items.kidsOf(70).value = Loadable.Ready(PARTS, fresh = true)
        val events = mutableListOf<MusicEvent>()
        backgroundScope.launch { vm.events.collect { events += it } }

        vm.play()
        assertEquals(ItemKind.Audiobook, resolved.single().kind)
        assertEquals(Triple(PARTS.map { it.id }, 1, false), music.played.last())
        assertEquals(1_200_000L, music.startedAt.last())
        assertEquals(listOf(MusicEvent.Played), events)

        // The book's second part plays: Play pauses it, its row too; another part plays from where it was left.
        music.now.value = NowPlaying(72, "Part 2", null, playing = true, book = true)
        val ready = vm.state.value as MusicPageState.Ready
        assertEquals(72L, ready.current)
        vm.play()
        vm.playPart(1)
        assertEquals(2, music.toggles)
        vm.playPart(2)
        assertEquals(2, music.played.last().second)
        assertEquals(0L, music.startedAt.last())
        // A part heard to its end starts from its start; From the start, the first part's.
        vm.playPart(0)
        assertEquals(0, music.played.last().second)
        assertEquals(0L, music.startedAt.last())
        vm.playPart(1)
        assertEquals(3, music.toggles)
        vm.fromStart()
        assertEquals(0, music.played.last().second)

        // Heard to the end of what the player had: Play asks the server again.
        music.now.value = NowPlaying(73, "Part 3", null, playing = false, book = true, ended = true)
        vm.play()
        assertEquals(2, resolved.size)
    }

    @Test
    fun aChapterPlaysFromItsStartAndTheOnePlayingIsMarked() = runTest(dispatcher) {
        val vm = model()
        val chapters = listOf(Chapter(0, 60_000, "Opening"), Chapter(60_000, 120_000, "Middle"))
        items.detailOf(70).value = Loadable.Ready(BOOK, fresh = true)
        items.kidsOf(70).value = Loadable.Ready(PARTS.take(1), fresh = true)
        items.detailOf(71).value = Loadable.Ready(ItemDetail(71, 5, ItemKind.Part, "Part 1", files = listOf(MediaFile(1, "book.m4b", chapters = chapters))), fresh = true)
        val page = (vm.state.value as MusicPageState.Ready).page
        assertTrue(page.rows.all { it is BookRow.Chapter })

        vm.playChapter(60_000)
        assertEquals(60_000L, music.startedAt.last())

        music.now.value = NowPlaying(71, "Part 1", null, playing = true, book = true, chapters = chapters, chapter = 1)
        assertEquals(1, vm.chapterNow.value)
        vm.playChapter(0)
        assertEquals(0L, music.seeks.last())
        assertEquals(1, music.played.size)
    }

    @Test
    fun aBookIsMarkedFinishedOrNot() = runTest(dispatcher) {
        val vm = model()
        vm.setFinished(true)
        vm.setFinished(false)
        assertEquals(listOf(70L to true, 70L to false), watch.watched)
    }

    private companion object {
        val BOOK = ItemDetail(
            id = 70,
            libraryId = 5,
            kind = ItemKind.Audiobook,
            title = "Long Book",
            year = 2021,
            durationMs = 10_800_000,
            book = Book(authors = listOf("A Writer", "Another"), narrators = listOf("A Voice"), series = "The Long Road", seriesIndex = 2.0),
        )

        val PARTS = listOf(
            part(71, index = 1, positionMs = 0, watched = true),
            part(72, index = 2, positionMs = 1_200_000),
            part(73, index = 3, positionMs = 0),
        )

        fun part(id: Long, index: Int, positionMs: Long, watched: Boolean = false) = ItemSummary(
            id = id,
            libraryId = 5,
            kind = ItemKind.Part,
            title = "Part $index",
            index = index,
            parentId = 70,
            parentTitle = "Long Book",
            durationMs = 3_600_000,
            userState = UserState(positionMs = positionMs, watched = watched),
        )
    }
}
