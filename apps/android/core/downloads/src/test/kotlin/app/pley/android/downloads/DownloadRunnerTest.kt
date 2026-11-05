package app.pley.android.downloads

import app.pley.android.common.UiText
import app.pley.android.data.DownloadState
import app.pley.android.model.ConversionInfo
import app.pley.android.model.ConversionState
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.StreamKind
import app.pley.android.network.ApiException
import app.pley.android.network.HttpPleyApi
import app.pley.android.network.PleyApi
import app.pley.android.testing.network.FakePleyApi
import kotlin.random.Random
import kotlinx.coroutines.runBlocking
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okio.Buffer
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** A download's file, whole or picked up where it stopped, its failures, and waiting for a network. */
class DownloadRunnerTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val server = MockWebServer()
    private lateinit var api: FakePleyApi
    private lateinit var store: DownloadStore
    private val body = Random(7).nextBytes(300_000)

    @Before
    fun setUp() {
        server.start()
        api = FakePleyApi(server.url("/").toString().trimEnd('/'))
        api.answer("item", detail())
        store = DownloadStore(folder.newFolder("downloads"))
        store.load()
    }

    @After
    fun tearDown() {
        server.close()
    }

    @Test
    fun aFileComesWholeAndIsPlayable() = runBlocking {
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        val saved = add()

        assertFalse("nothing waits", runner().run())

        val done = store.get(saved.key)!!
        assertEquals(DownloadState.Done, done.state)
        assertTrue(done.extrasSaved)
        assertArrayEquals(body, store.mediaOf(done).readBytes())
        assertFalse("the partial file is gone", store.partOf(done).exists())
        assertEquals("/api/files/7/stream", server.takeRequest().url.encodedPath)
    }

    @Test
    fun aQueuedDownloadHasItsPictureBeforeItsTurn() = runBlocking {
        val picture = Random(3).nextBytes(2_000)
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(picture)).build())
        val saved = add()

        runner().savePicture(saved, detail().copy(thumbUrl = "/api/items/42/thumb"))

        val queued = store.get(saved.key)!!
        assertEquals(DownloadState.Queued, queued.state)
        assertTrue(queued.image)
        assertArrayEquals(picture, store.imageOf(queued).readBytes())
        assertEquals("/api/items/42/thumb", server.takeRequest().url.encodedPath)
    }

    @Test
    fun aFileCutShortGoesOnFromWhereItStopped() = runBlocking {
        val saved = add()
        store.partOf(saved).apply { parentFile!!.mkdirs() }.writeBytes(body.copyOfRange(0, 100_000))
        server.enqueue(
            MockResponse.Builder()
                .code(206)
                .setHeader("Content-Range", "bytes 100000-299999/300000")
                .body(Buffer().write(body.copyOfRange(100_000, body.size)))
                .build(),
        )

        runner().run()

        assertEquals("bytes=100000-", server.takeRequest().headers["Range"])
        val done = store.get(saved.key)!!
        assertEquals(DownloadState.Done, done.state)
        assertArrayEquals(body, store.mediaOf(done).readBytes())
    }

    @Test
    fun aServerThatIgnoresTheRangeSendsItAllAgain() = runBlocking {
        val saved = add()
        store.partOf(saved).apply { parentFile!!.mkdirs() }.writeBytes(ByteArray(50_000))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())

        runner().run()

        assertArrayEquals(body, store.mediaOf(store.get(saved.key)!!).readBytes())
    }

    @Test
    fun aFileTheServerNoLongerHasFails() = runBlocking {
        server.enqueue(MockResponse.Builder().code(404).build())
        val saved = add()

        runner().run()

        val failed = store.get(saved.key)!!
        assertEquals(DownloadState.Failed, failed.state)
        assertEquals(UiText.res(R.string.downloads_error_gone), failed.errorText())
    }

    @Test
    fun withoutANetworkItMayUseDownloadsWait() = runBlocking {
        val saved = add()

        assertTrue("some wait", runner(open = false).run(waitMs = 10))

        assertEquals(DownloadState.Waiting, store.get(saved.key)!!.state)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun aFileTheServerMakesForThisPhoneIsWaitedForThenDownloaded() = runBlocking {
        api.answer("convert", ConversionInfo(id = CONVERSION, state = ConversionState.Running, progress = 0.4, url = FILE))
        api.answer("conversion", ConversionInfo(id = CONVERSION, state = ConversionState.Done, progress = 1.0, size = body.size.toLong(), url = FILE))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        val saved = add(convert = true)

        assertFalse("nothing waits", runner().run())

        val done = store.get(saved.key)!!
        assertEquals(DownloadState.Done, done.state)
        assertArrayEquals(body, store.mediaOf(done).readBytes())
        assertEquals("the server's file", FILE, server.takeRequest().url.encodedPath)
        assertEquals((done.conversion to done.converting), CONVERSION to null)
        // Asked for once, then looked at until whole.
        assertEquals(1, api.calls("convert").size)
        assertEquals(listOf(CONVERSION), api.calls("conversion").map { it.args.single() })
    }

    @Test
    fun theOriginalComesWhenItPlaysHereAfterAll() = runBlocking {
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        val saved = add(convert = true)

        runner().run()

        assertEquals(DownloadState.Done, store.get(saved.key)!!.state)
        assertEquals("/api/files/7/stream", server.takeRequest().url.encodedPath)
    }

    @Test
    fun aTranscoderNotRunningIsWaitedForLikeAServerAway() = runBlocking {
        api.fail("convert", ApiException(503, "unavailable", "the transcoder isn't running"))
        val saved = add(convert = true)

        assertTrue("it waits", runner().run(waitMs = 0))

        val waiting = store.get(saved.key)!!
        assertEquals(DownloadState.Waiting to true, waiting.state to waiting.unreachable)
        assertEquals("nothing came", 0, server.requestCount)
    }

    @Test
    fun aServerThatCantConvertGivesTheOriginalUnlessASmallerFileWasAsked() = runBlocking {
        api.fail("convert", ApiException(501, "not_implemented", "the transcoder can't convert files"))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        val playable = add(convert = true)
        runner().run()
        assertEquals(DownloadState.Done, store.get(playable.key)!!.state)

        val smaller = add(convert = true, itemId = 43, maxHeight = 720)
        runner().run()
        val failed = store.get(smaller.key)!!
        assertEquals(DownloadState.Failed, failed.state)
        assertEquals(UiText.res(R.string.downloads_error_no_conversions), failed.errorText())
    }

    @Test
    fun aDeletedDownloadStopsTheFileTheServerIsMakingButNotOneMade() = runBlocking {
        val making = add(convert = true).copy(conversion = CONVERSION)
        runner().forget(making)
        assertEquals(listOf(CONVERSION), api.calls("cancelConversion").map { it.args.single() })

        runner().forget(making.copy(state = DownloadState.Done, mediaUrl = FILE))
        runner().forget(add(itemId = 43))
        assertEquals("a file made, or none asked for: nothing to stop", 1, api.calls("cancelConversion").size)
    }

    @Test
    fun aSubtitleTrackCutShortIsAskedForAgainOnceTheFileIsDown() = runBlocking {
        api.answer("item", detail(subtitles = true))
        server.enqueue(vtt(PARTIAL, whole = false))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        server.enqueue(vtt(WHOLE, whole = true))
        val saved = add()

        assertFalse(runner().run())

        assertEquals("/api/files/7/subtitles/2.vtt", server.takeRequest().url.encodedPath)
        assertEquals("the file doesn't wait for the track", "/api/files/7/stream", server.takeRequest().url.encodedPath)
        assertEquals("/api/files/7/subtitles/2.vtt", server.takeRequest().url.encodedPath)
        val done = store.get(saved.key)!!
        assertEquals(DownloadState.Done, done.state)
        assertEquals(listOf(2), done.subtitles)
        assertEquals(emptyList<Int>(), done.partialSubtitles)
        assertEquals(WHOLE, store.subtitleOf(done, 2).readText())
    }

    @Test
    fun subtitlesStillShortAreFinishedOnALaterRun() = runBlocking {
        api.answer("item", detail(subtitles = true))
        server.enqueue(vtt(PARTIAL, whole = false))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        server.enqueue(vtt(PARTIAL, whole = false))
        val saved = add()

        assertFalse(runner().run())
        val short = store.get(saved.key)!!
        assertEquals(DownloadState.Done, short.state)
        assertEquals("what came is kept meanwhile", PARTIAL, store.subtitleOf(short, 2).readText())
        assertEquals(listOf(2), short.partialSubtitles)

        server.enqueue(vtt(WHOLE, whole = true))
        assertFalse("nothing waits", runner().run())

        val whole = store.get(saved.key)!!
        assertEquals(emptyList<Int>(), whole.partialSubtitles)
        assertEquals(WHOLE, store.subtitleOf(whole, 2).readText())
        assertEquals("asked once after the file, once on the next run", 4, server.requestCount)
    }

    @Test
    fun aConvertedFileCutShortGoesOnWhenTheServerForgotItsConversion() = runBlocking {
        // The server restarted: it answers 404 for the file until asked for the conversion again.
        api.answer("conversion", ConversionInfo(id = CONVERSION, state = ConversionState.Done, progress = 1.0, size = body.size.toLong(), url = FILE))
        val begun = add(convert = true, extrasSaved = true)
        store.update(begun.key) { it.copy(conversion = CONVERSION, mediaUrl = FILE, sizeBytes = body.size.toLong()) }
        store.partOf(store.get(begun.key)!!).apply { parentFile!!.mkdirs() }.writeBytes(body.copyOfRange(0, 100_000))
        server.enqueue(MockResponse.Builder().code(404).build())
        server.enqueue(
            MockResponse.Builder()
                .code(206)
                .setHeader("Content-Range", "bytes 100000-299999/300000")
                .body(Buffer().write(body.copyOfRange(100_000, body.size)))
                .build(),
        )

        runner().run()

        assertEquals("bytes=100000-", server.takeRequest().headers["Range"])
        assertEquals("what was down is kept", "bytes=100000-", server.takeRequest().headers["Range"])
        val done = store.get(begun.key)!!
        assertEquals(DownloadState.Done, done.state)
        assertArrayEquals(body, store.mediaOf(done).readBytes())
    }

    @Test
    fun aFileChangedOnTheServerSinceItBeganStartsAgain() = runBlocking {
        val saved = add()
        store.partOf(saved).apply { parentFile!!.mkdirs() }.writeBytes(ByteArray(100_000) { 1 })
        // Replaced by a longer one: what is down belongs to the file before.
        server.enqueue(
            MockResponse.Builder()
                .code(206)
                .setHeader("Content-Range", "bytes 100000-399999/400000")
                .body(Buffer().write(ByteArray(300_000)))
                .build(),
        )
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())

        runner().run()

        assertEquals("bytes=100000-", server.takeRequest().headers["Range"])
        assertEquals("from the start", null, server.takeRequest().headers["Range"])
        assertArrayEquals(body, store.mediaOf(store.get(saved.key)!!).readBytes())
    }

    @Test
    fun onlyTheSignedInProfilesDownloadsRunWithItsToken() = runBlocking {
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        val mine = add()
        val theirs = add(itemId = 43, userId = 3)

        assertFalse(runner(profile = "http://nas:7539" to 2L).run())

        assertEquals(DownloadState.Done, store.get(mine.key)!!.state)
        assertEquals("until that profile signs in", DownloadState.Queued, store.get(theirs.key)!!.state)
        assertEquals(1, server.requestCount)
    }

    @Test
    fun nothingRunsWhileNobodyIsSignedIn() = runBlocking {
        val saved = add()

        assertFalse(runner(profile = null).run())

        assertEquals(DownloadState.Queued, store.get(saved.key)!!.state)
        assertEquals(0, server.requestCount)
    }

    @Test
    fun aDownloadWhoseServerIsAwayLetsTheOthersGo() = runBlocking {
        // The older one can't reach its server; the newer one comes meanwhile.
        api.fail("item", java.io.IOException("away"))
        server.enqueue(MockResponse.Builder().code(200).body(Buffer().write(body)).build())
        val away = add()
        val other = add(itemId = 43, fileId = 8, extrasSaved = true)

        assertTrue("one still waits", runner().run(waitMs = 0))

        assertEquals(DownloadState.Done, store.get(other.key)!!.state)
        assertEquals(DownloadState.Waiting, store.get(away.key)!!.state)
    }

    @Test
    fun theIndexIsReadBackAndACutDownloadWaitsItsTurn() {
        val saved = add()
        store.update(saved.key) { it.copy(state = DownloadState.Downloading) }
        store.partOf(saved).apply { parentFile!!.mkdirs() }.writeBytes(ByteArray(1234))

        val again = DownloadStore(store.dirOf(saved).parentFile!!.parentFile!!.parentFile!!).apply { load() }

        val back = again.get(saved.key)!!
        assertEquals(DownloadState.Queued, back.state)
        assertEquals(1234L, back.downloadedBytes)
    }

    private fun add(
        convert: Boolean = false,
        itemId: Long = 42,
        maxHeight: Int? = null,
        userId: Long = 2,
        fileId: Long = 7,
        extrasSaved: Boolean = false,
    ): Saved {
        val saved = Saved(
            server = "http://nas:7539",
            userId = userId,
            itemId = itemId,
            fileId = fileId,
            extrasSaved = extrasSaved,
            kind = ItemKind.Movie,
            title = "Alien",
            container = "mkv",
            sizeBytes = if (convert) 0 else body.size.toLong(),
            addedAt = 1,
            convert = convert,
            maxHeight = maxHeight,
            maxBitrateKbps = maxHeight?.let { 4_000 },
        )
        store.add(listOf(saved))
        return saved
    }

    private fun runner(open: Boolean = true, profile: Pair<String, Long>? = "http://nas:7539" to 2L) = DownloadRunner(
        store = store,
        sources = object : DownloadSources {
            override fun api(server: String, userId: Long): PleyApi = api

            override fun media(server: String) = OkHttpClient()

            override fun subtitles(server: String) = OkHttpClient()

            override val images = OkHttpClient()
        },
        gate = object : NetworkGate {
            override fun open(mobile: Boolean, server: String?) = open

            override suspend fun await(mobile: Boolean, timeoutMs: Long, server: String?) = open
        },
        mobileAllowed = { false },
        conversionPollMs = 10,
        backoffMs = 1,
        profile = { profile },
    )

    private companion object {
        const val CONVERSION = "0123456789abcdef0123456789abcdef"
        const val FILE = "/api/conversions/$CONVERSION/file"
        const val PARTIAL = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nFirst\n\n"
        const val WHOLE = PARTIAL + "01:50:00.000 --> 01:50:02.000\nLast\n\n"
    }

    /** The item, with an English SubRip track when [subtitles]. */
    private fun detail(subtitles: Boolean = false) = ItemDetail(
        id = 42,
        libraryId = 1,
        kind = ItemKind.Movie,
        title = "Alien",
        files = listOf(
            MediaFile(
                id = 7,
                path = "Alien (1979).mkv",
                size = body.size.toLong(),
                container = "mkv",
                streams = if (subtitles) listOf(MediaStream(2, StreamKind.Subtitle, "subrip", language = "eng", textBased = true)) else emptyList(),
            ),
        ),
    )

    /** The server's WebVTT of a track, said to be [whole] or still being read out of the file. */
    private fun vtt(text: String, whole: Boolean) = MockResponse.Builder()
        .code(200)
        .setHeader("Content-Type", "text/vtt; charset=utf-8")
        .apply { if (!whole) setHeader(HttpPleyApi.SUBTITLES_COMPLETE_HEADER, "0") }
        .body(text)
        .build()
}
