@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.session

import app.pley.android.caps.CapsSnapshot
import app.pley.android.caps.DefaultPlaybackCapsPolicy
import app.pley.android.data.LocalMedia
import app.pley.android.fixtures.FixtureIds
import app.pley.android.fixtures.FixtureServer
import app.pley.android.fixtures.Journal
import app.pley.android.model.AddSubtitle
import app.pley.android.model.ItemDetail
import app.pley.android.model.NetworkReport
import app.pley.android.model.MaybeItem
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.ServerEvent
import app.pley.android.model.PleyJson
import app.pley.android.model.ProgressReport
import app.pley.android.model.SessionStatus
import app.pley.android.model.StreamChoice
import app.pley.android.model.TrickplayInfo
import app.pley.android.network.ApiException
import app.pley.android.network.HttpPleyApi
import app.pley.android.network.OfflineException
import app.pley.android.network.PleyApi
import app.pley.android.network.SubtitleText
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlayerUiState
import app.pley.android.player.subtitles.ApiSubtitleFetcher
import app.pley.android.player.subtitles.PictureSource
import app.pley.android.player.subtitles.SubtitleOffsets
import app.pley.android.testing.caps.FakeDeviceCapabilities
import app.pley.android.testing.caps.TestDevices
import app.pley.android.testing.caps.capsOf
import app.pley.android.testing.player.session.FakeEngineLink
import java.net.ConnectException
import java.util.Collections
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestCoroutineScheduler
import okhttp3.OkHttpClient
import org.junit.Assert.fail
import org.robolectric.RuntimeEnvironment

/**
 * [PleyApi] counting the calls on their way: a call counts until the
 * coroutine that made it goes on, so [SessionHarness.settle] knows when the
 * FixtureServer has answered everything.
 */
class CountingApi(private val delegate: PleyApi) : PleyApi by delegate {
    val inFlight = AtomicInteger()

    /** The calls made, by name, in order. */
    val names: MutableList<String> = Collections.synchronizedList(mutableListOf())

    /** How many calls of [name] were made. */
    fun count(name: String): Int = synchronized(names) { names.count { it == name } }

    /** Calls that failed, with their route. */
    val failures = mutableListOf<Pair<String, Throwable>>()

    /** Calls (by name) that fail as if the server were away. */
    val unreachable: MutableSet<String> = Collections.synchronizedSet(mutableSetOf())

    /** Sessions asked for beside the one playing (keep_previous) are refused: no transcode is free for two at once (503). */
    @Volatile
    var noTranscodeFreeBeside = false

    /** Sessions asked for from now on are refused with this status (503: the server runs its most transcodes); `null`: none. */
    @Volatile
    var refusedWith: Int? = null

    /** Counts a call that isn't the API's (a warm-up's requests) until [block] returns. */
    suspend fun <T> along(block: suspend () -> T): T {
        inFlight.incrementAndGet()
        try {
            return block()
        } finally {
            inFlight.decrementAndGet()
        }
    }

    private suspend fun <T> counted(name: String, block: suspend () -> T): T {
        inFlight.incrementAndGet()
        names += name
        try {
            if (name in unreachable) throw OfflineException(ConnectException("$name: the server is away"))
            return block()
        } catch (e: Exception) {
            synchronized(failures) { failures += name to e }
            throw e
        } finally {
            inFlight.decrementAndGet()
        }
    }

    override suspend fun item(id: Long): ItemDetail = counted("item") { delegate.item(id) }

    override suspend fun next(id: Long): MaybeItem = counted("next") { delegate.next(id) }

    override suspend fun playlist(id: Long) = counted("playlist") { delegate.playlist(id) }

    override suspend fun collection(id: Long) = counted("collection") { delegate.collection(id) }

    override suspend fun startPlayback(request: PlaybackRequest): PlaybackInfo = counted("startPlayback") {
        if (request.keepPrevious == true && noTranscodeFreeBeside) throw ApiException(503, "transcode_limit", "No transcode is free")
        refusedWith?.let { throw ApiException(it, if (it == 503) "transcode_limit" else "internal", "Refused") }
        delegate.startPlayback(request)
    }

    override suspend fun playbackStatus(sessionId: String): SessionStatus = counted("playbackStatus") { delegate.playbackStatus(sessionId) }

    override suspend fun stopPlayback(sessionId: String) = counted("stopPlayback") { delegate.stopPlayback(sessionId) }

    override suspend fun subtitles(fileId: Long, index: Int): SubtitleText = counted("subtitles") { delegate.subtitles(fileId, index) }

    override suspend fun trickplay(fileId: Long): TrickplayInfo? = counted("trickplay") { delegate.trickplay(fileId) }

    override suspend fun reportProgress(itemId: Long, report: ProgressReport) = counted("reportProgress") { delegate.reportProgress(itemId, report) }

    override suspend fun setStreams(itemId: Long, choice: StreamChoice) = counted("setStreams") { delegate.setStreams(itemId, choice) }

    override suspend fun playbackNetwork(clientId: String) = counted("playbackNetwork") { delegate.playbackNetwork(clientId) }

    override suspend fun reportNetwork(report: NetworkReport) = counted("reportNetwork") { delegate.reportNetwork(report) }

    override suspend fun speedTest(bytes: Long, readMs: Long, timeoutMs: Long) = counted("speedTest") { delegate.speedTest(bytes, readMs, timeoutMs) }

    override suspend fun searchSubtitles(itemId: Long, fileId: Long?, languages: List<String>?) =
        counted("searchSubtitles") { delegate.searchSubtitles(itemId, fileId, languages) }

    override suspend fun addSubtitle(itemId: Long, add: AddSubtitle) = counted("addSubtitle") { delegate.addSubtitle(itemId, add) }
}

/**
 * A playback against a FixtureServer, with a fake engine and virtual time:
 * the server's clock is the test scheduler's, so the journal says when (in
 * virtual ms) each write came. Robolectric tests only (the texts are the
 * app's resources).
 */
class SessionHarness(
    val caps: CapsSnapshot = capsOf(TestDevices.sdrPhone),
    val autoplay: Boolean = true,
    /** The quality remembered on the device: the original unless a test says otherwise ("auto" for Automatic). */
    val quality: String? = QualityPreset.ORIGINAL_ID,
    /** Picture subtitles beside a stream; `null` burns them in. */
    val pictures: PictureSource? = null,
    /** The server's events, as the app's stream would bring them. */
    val events: Flow<ServerEvent> = emptyFlow(),
    /** Subtitle offsets, shared by the playbacks of this harness. */
    val offsets: SubtitleOffsets = SubtitleOffsets.memory(),
    /** Told whether a playback has a session on the server (LiveUpdates.playing). */
    val playing: (player: Any, playing: Boolean) -> Unit = { _, _ -> },
    /** Told which session a playback plays (RemoteRepository.here). */
    val here: (player: Any, sessionId: String?) -> Unit = { _, _ -> },
) : AutoCloseable {
    /** The profile watching the playbacks started from now on. */
    var profileId: Long? = null

    val scheduler = TestCoroutineScheduler()
    private val dispatcher = StandardTestDispatcher(scheduler)
    val server = FixtureServer(FixtureServer.Options(scanEveryMs = 0, clock = { scheduler.currentTime }))
    val engine = FakeEngineLink { scheduler.currentTime }
    private val reportScope = CoroutineScope(dispatcher + SupervisorJob())
    val api: CountingApi
    val remembered = mutableListOf<String>()

    /** Streams made ready beside the one playing: their address and where they were asked for from, in order. */
    val warmups: MutableList<Pair<String, Long>> = Collections.synchronizedList(mutableListOf())

    /** How long a warm-up takes before it asks the server, in virtual ms. */
    var warmupMs = 0L
    private val warmer: StreamWarmer
    private var controllers = mutableListOf<SessionPlaybackController>()

    /** Downloads, for playbacks started from now on. */
    var local: LocalMedia = LocalMedia.None

    init {
        server.start()
        val token = server.accessToken()
        val client = OkHttpClient.Builder()
            .readTimeout(10, TimeUnit.SECONDS)
            .addInterceptor { chain -> chain.proceed(chain.request().newBuilder().header("Authorization", "Bearer $token").build()) }
            .build()
        api = CountingApi(HttpPleyApi(server.url, { client }))
        warmer = OkHttpStreamWarmer({ client }, dispatcher)
    }

    val reporter = SessionReporter(reportScope)

    fun environment(): PlaybackEnvironment = PlaybackEnvironment(
        api = api,
        capabilities = FakeDeviceCapabilities(probe = caps),
        policy = DefaultPlaybackCapsPolicy,
        details = ItemDetails.of(api),
        engines = { engine },
        texts = ResourcePlayerTexts(RuntimeEnvironment.getApplication()),
        clientId = { CLIENT_ID },
        reporter = reporter,
        subtitles = ApiSubtitleFetcher(api),
        quality = object : QualityMemory {
            override suspend fun remembered(): String? = remembered.lastOrNull() ?: quality

            override suspend fun remember(id: String) {
                remembered += id
            }
        },
        autoplayNext = { autoplay },
        now = { scheduler.currentTime },
        work = dispatcher,
        pictures = pictures,
        events = events,
        playing = playing,
        here = here,
        local = local,
        offsets = offsets,
        profileId = profileId,
        tracksChosen = { itemId, choice -> synchronized(tracksChosen) { tracksChosen += itemId to choice } },
        warmer = StreamWarmer { url, startMs ->
            warmups += url to startMs
            delay(warmupMs)
            api.along { warmer.warm(url, startMs) }
        },
    )

    /** What the app was told of tracks the player saved (PlaybackEnvironment.tracksChosen). */
    val tracksChosen = mutableListOf<Pair<Long, StreamChoice>>()

    /** Starts playing [request] and lets the server answer. */
    fun play(request: PlayRequest): SessionPlaybackController {
        val controller = SessionPlaybackController(request, environment(), CoroutineScope(dispatcher + SupervisorJob()))
        controllers += controller
        controller.start()
        settle()
        return controller
    }

    /** Runs what is due now, waiting for the server's answers, until nothing is left to run. */
    fun settle() {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
        while (true) {
            scheduler.runCurrent()
            if (api.inFlight.get() == 0) {
                scheduler.runCurrent()
                if (api.inFlight.get() == 0) return
            }
            if (System.nanoTime() > deadline) fail("the FixtureServer didn't answer in 10 s")
            Thread.sleep(2)
        }
    }

    /** Lets [ms] of virtual time pass, the server answering along the way. */
    fun advance(ms: Long) {
        var left = ms
        while (left > 0) {
            val step = minOf(STEP_MS, left)
            scheduler.advanceTimeBy(step)
            scheduler.runCurrent()
            settle()
            left -= step
        }
    }

    // ---------------------------------------------------------------- what the server got

    val journal: Journal get() = server.journal

    /** The playback requests, in order. */
    fun playbackRequests(): List<PlaybackRequest> =
        journal.entries().filter { it.method == "POST" && it.path == "/api/playback" }.map { PleyJson.decodeFromString(PlaybackRequest.serializer(), it.body) }

    /** The progress reports of [itemId], in order. */
    fun progress(itemId: Long): List<Pair<Long, ProgressReport>> =
        journal.writes("POST", "/api/items/$itemId/progress").map { it.atMs to PleyJson.decodeFromString(ProgressReport.serializer(), it.body) }

    /** The DELETEs of sessions, in order: their ids. */
    fun deletes(): List<String> = journal.entries().filter { it.method == "DELETE" && it.path.startsWith("/api/playback/") }.map { it.path.removePrefix("/api/playback/") }

    /** What happened, as "POST /api/playback", "POST /api/items/1/progress stopped", "DELETE"…, for order checks. */
    fun timeline(): List<String> = journal.entries().map { e ->
        when {
            e.path.endsWith("/progress") -> "progress ${PleyJson.decodeFromString(ProgressReport.serializer(), e.body).state.wire}"
            e.method == "DELETE" && e.path.startsWith("/api/playback/") -> "delete"
            e.path == "/api/playback" -> "start"
            e.path.endsWith("/streams") -> "streams"
            else -> "${e.method} ${e.path}"
        }
    }

    fun state(controller: SessionPlaybackController): PlayerUiState = controller.state.value

    override fun close() {
        controllers.forEach { it.close() }
        settle()
        server.close()
    }

    companion object {
        const val CLIENT_ID = "android-test"
        private const val STEP_MS = 250L

        val S1E1 = FixtureIds.SHOW_S1E1
    }
}
