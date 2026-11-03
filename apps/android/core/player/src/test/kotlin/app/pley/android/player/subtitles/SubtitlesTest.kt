@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.subtitles

import androidx.media3.common.text.Cue
import app.pley.android.fixtures.PgsMks
import app.pley.android.network.SubtitleText
import java.io.File
import java.io.IOException
import java.util.Collections
import java.util.concurrent.TimeUnit
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestCoroutineScheduler
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * WebVTT through Media3's parser, the cue index, partial tracks asked for
 * again and merged, and picture tracks dropped on their way let go of.
 */
@RunWith(RobolectricTestRunner::class)
class SubtitlesTest {
    private val vtt = """
        WEBVTT

        00:00:01.000 --> 00:00:04.000
        The fixture server plays this.

        00:00:03.000 --> 00:00:05.500 line:0
        <i>A sign</i> at the top

        00:01:06.000 --> 00:01:09.000
        Two
        lines
    """.trimIndent()

    private fun texts(cues: List<Cue>) = cues.map { it.text.toString() }

    @Test
    fun `cues without a line sit on the area's last line, so the screen pads the area`() {
        // SubtitleView draws a cue on line -1 flush with its area's bottom, not 8 % above it:
        // a player that doesn't pad the view puts the server's subtitles on the screen's edge.
        val cue = Media3VttParser.parse(vtt).first().cues.single()
        assertEquals(Cue.LINE_TYPE_NUMBER, cue.lineType)
        assertEquals(-1f, cue.line)
    }

    @Test
    fun `the settings' size and background, and pley's style once either was chosen`() {
        assertEquals(CaptionLook(0.048f, box = false, custom = false), CaptionLook.of(app.pley.android.data.Settings()))
        val big = app.pley.android.data.Settings(subtitleSize = 0.075f, subtitleBackground = app.pley.android.data.SubtitleBackground.DarkBox)
        assertEquals(CaptionLook(0.076f, box = true, custom = true), CaptionLook.of(big))
        // A font is pley's style too; the position applies either way, never counted as one.
        val serif = app.pley.android.data.Settings(subtitleFont = app.pley.android.data.SubtitleFont.Serif)
        assertEquals(CaptionLook(0.048f, box = false, custom = true, font = app.pley.android.data.SubtitleFont.Serif), CaptionLook.of(serif))
        val raised = app.pley.android.data.Settings(subtitleRaise = 0.11f)
        assertEquals(CaptionLook(0.048f, box = false, custom = false, raise = 0.1f), CaptionLook.of(raised))
    }

    @Test
    fun `Media3's parser reads times, text, styling and lines`() {
        val index = SubtitleIndex(Media3VttParser.parse(vtt))
        assertEquals(69_000L, index.endMs)
        assertEquals(listOf("The fixture server plays this."), texts(index.at(1_000)))
        assertEquals(listOf("A sign at the top", "The fixture server plays this."), texts(index.at(3_500)).sorted())
        assertEquals(listOf("A sign at the top"), texts(index.at(4_000)))
        assertEquals(listOf("Two\nlines"), texts(index.at(67_000)))
        assertEquals(emptyList<String>(), texts(index.at(10_000)))
        // Placed at the top by its line setting.
        assertEquals(0f, index.at(4_500).single().line)
    }

    @Test
    fun `the next change is the next start or end`() {
        val index = SubtitleIndex(Media3VttParser.parse(vtt))
        assertEquals(1_000L, index.nextChangeAfter(0))
        assertEquals(3_000L, index.nextChangeAfter(1_000))
        assertEquals(4_000L, index.nextChangeAfter(3_000))
        assertEquals(66_000L, index.nextChangeAfter(5_500))
        assertNull(index.nextChangeAfter(69_000))
    }

    @Test
    fun `merging keeps what was there and adds what is new, once`() {
        val first = SubtitleIndex(Media3VttParser.parse(vtt.substringBefore("00:01:06")))
        val whole = SubtitleIndex(Media3VttParser.parse(vtt))
        val merged = first.merge(whole)
        assertEquals(whole.size, merged.size)
        assertSame(merged, merged.merge(first))
        assertEquals(listOf("Two\nlines"), texts(merged.at(67_000)))
    }

    @Test
    fun `a partial track is asked for every 3 s until whole, and a failure is said once`() {
        val scheduler = TestCoroutineScheduler()
        val scope = CoroutineScope(StandardTestDispatcher(scheduler) + SupervisorJob())
        val answers = ArrayDeque(
            listOf(
                SubtitleText(vtt.substringBefore("00:00:03"), complete = false),
                SubtitleText(vtt.substringBefore("00:01:06"), complete = false),
                SubtitleText(vtt, complete = true),
            ),
        )
        val asked = mutableListOf<String>()
        val errors = mutableListOf<Throwable>()
        val subtitles = ServerSubtitles(
            scope,
            { fileId, index, url ->
                asked += "$fileId:$index:$url"
                answers.removeFirstOrNull() ?: throw IOException("gone")
            },
            parse = StandardTestDispatcher(scheduler),
            onError = { errors += it },
        )
        subtitles.show(7, 2, "http://pley/api/files/7/subtitles/2.vtt")
        scheduler.runCurrent()
        assertEquals(4_000L, subtitles.index.value?.endMs)
        assertEquals(1, subtitles.index.value!!.at(3_500).size)
        scheduler.advanceTimeBy(3_001)
        assertEquals(5_500L, subtitles.index.value?.endMs)
        assertEquals(2, subtitles.index.value!!.at(3_500).size)
        scheduler.advanceTimeBy(3_000)
        assertEquals(69_000L, subtitles.index.value?.endMs)
        scheduler.advanceTimeBy(30_000)
        assertEquals(3, asked.size)
        assertEquals("7:2:http://pley/api/files/7/subtitles/2.vtt", asked.first())

        // The same track again changes nothing; another one starts over, and its failure is reported.
        subtitles.show(7, 2, "x")
        subtitles.show(7, 1000, "http://pley/api/files/7/subtitles/1000.vtt")
        scheduler.runCurrent()
        assertNull(subtitles.index.value)
        assertEquals(1, errors.size)
        subtitles.hide()
        assertNull(subtitles.showing)
    }

    @Test
    fun `a picture track read after its ask was dropped lets go of its samples`() {
        val dir = File(System.getProperty("java.io.tmpdir"), "pley-pictures-${System.nanoTime()}")
        val server = MockWebServer()
        server.enqueue(MockResponse.Builder().body(Buffer().write(PgsMks.build(listOf(Triple(500L, 4_500L, "PGS 1")), 10_000, "eng"))).build())
        server.start()
        val client = OkHttpClient()
        val scheduler = TestCoroutineScheduler()
        val scope = CoroutineScope(StandardTestDispatcher(scheduler) + SupervisorJob())
        // What goes to the reading thread waits here until the test runs it.
        val reads = Collections.synchronizedList(mutableListOf<Runnable>())
        val io = object : CoroutineDispatcher() {
            override fun dispatch(context: CoroutineContext, block: Runnable) {
                reads += block
            }
        }
        val source = OkHttpPictureSource({ client }, dir, io, Dispatchers.Unconfined)
        try {
            val job = scope.launch { source.load(7, 2, server.url("/api/files/7/subtitles/2.mks?at=0").toString()) }
            // The client first, then the answer comes and waits to be read.
            scheduler.runCurrent()
            reads.removeAt(0).run()
            val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
            while (reads.isEmpty()) {
                if (System.nanoTime() > deadline) fail("the .mks didn't come in 10 s")
                Thread.sleep(2)
                scheduler.runCurrent()
            }
            // Dropped (a seek) while it is read: it is read to the end, and closed.
            job.cancel()
            reads.removeAt(0).run()
            scheduler.runCurrent()
            assertTrue(job.isCompleted)
            assertEquals(emptyList<String>(), dir.list().orEmpty().filter { it.endsWith(".samples") })
        } finally {
            scope.cancel()
            server.close()
            client.dispatcher.executorService.shutdown()
            dir.deleteRecursively()
        }
    }
}
