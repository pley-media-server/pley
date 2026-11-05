@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.remote

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.Me
import app.pley.android.model.PlaylistKind
import app.pley.android.model.ServerEvent
import app.pley.android.network.LiveEvent
import app.pley.android.player.CastFrom
import app.pley.android.player.MusicControl
import app.pley.android.player.NowPlaying
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest
import app.pley.android.screens.PlayerLauncher
import app.pley.android.testing.Fixtures
import app.pley.android.testing.auth.FakeAuthGraph
import app.pley.android.testing.data.TestData
import kotlin.random.Random
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * What a remote casts here plays as the app's own Play would (docs/API.md,
 * "Casting"), over the real data side: only a cast to this device's
 * receiver while the stream names it, from where the remote says, and
 * saying where it came from. Robolectric for the data side's JSON.
 */
@RunWith(RobolectricTestRunner::class)
class CastReceiverTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val launcher = Launcher()
    private val music = Music()
    private var closed = 0

    /**
     * Signed in, in front, the stream connected naming this device: the
     * receiver listening, and this device's id. The token endpoint answers
     * through [auth]; what plays ends through [endPlaying].
     */
    private suspend fun TestScope.receiving(
        auth: FakeAuthGraph = FakeAuthGraph(TestData.offlineHttp()),
        endPlaying: suspend () -> Unit = {},
        asking: () -> Boolean = { false },
        prepare: TestData.() -> Unit = {},
    ): Pair<TestData, String> {
        val data = TestData(this, folder.newFolder(), auth = auth)
        data.prepare()
        data.signIn()
        data.useServer()
        data.graph.session
        testScheduler.runCurrent()
        ServerCastReceiver(
            data.graph,
            launcher,
            { music },
            backgroundScope,
            Random(7),
            closePlayers = { closed++ },
            io = data.dispatcher,
            endPlaying = endPlaying,
            asking = asking,
        )
            .also { receiver = it }
            .start()
        testScheduler.runCurrent()
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        return data to data.graph.servers.clientId()
    }

    private lateinit var receiver: ServerCastReceiver

    private fun TestScope.cast(data: TestData, event: ServerEvent.Cast) {
        data.events.send(LiveEvent.Event(event))
        testScheduler.runCurrent()
    }

    @Test
    fun `a movie cast here plays from where the remote says, and says where it came from`() = runTest {
        val (data, id) = receiving()
        cast(data, ServerEvent.Cast(id, itemId = 101, startMs = 600_000, from = "Kitchen phone"))
        assertEquals(listOf(PlayRequest(itemId = 101, startMs = 600_000, castFrom = CastFrom("Kitchen phone"))), launcher.played)

        // Without a place, where its Play resumes; a remote the server can't name is another device.
        cast(data, ServerEvent.Cast(id, itemId = 101))
        assertEquals(PlayRequest(itemId = 101, startMs = 2_412_000, castFrom = CastFrom(null)), launcher.played.last())
    }

    @Test
    fun `a movie cast with the tracks the remote picked plays with them, subtitles off as -1`() = runTest {
        val (data, id) = receiving()
        cast(data, ServerEvent.Cast(id, itemId = 101, audioIndex = 2, subtitleIndex = -1, from = "Kitchen phone"))
        assertEquals(
            PlayRequest(itemId = 101, startMs = 2_412_000, audioIndex = 2, subtitleIndex = -1, castFrom = CastFrom("Kitchen phone")),
            launcher.played.single(),
        )
    }

    @Test
    fun `a show plays its next episode as its Play does`() = runTest {
        val (data, id) = receiving()
        cast(data, ServerEvent.Cast(id, itemId = 201, startMs = 5_000, from = "Kitchen phone"))
        // The episode on deck, where it was left, with its own tracks: a place and tracks are a title's own.
        assertEquals(listOf(PlayRequest(itemId = 204, startMs = 1_185_000, castFrom = CastFrom("Kitchen phone"))), launcher.played)
        cast(data, ServerEvent.Cast(id, itemId = 201, audioIndex = 2, subtitleIndex = 3))
        assertEquals(PlayRequest(itemId = 204, startMs = 1_185_000, castFrom = CastFrom(null)), launcher.played.last())
    }

    @Test
    fun `a cast for another device does nothing, nor one heard while this one isn't a receiver`() = runTest {
        val (data, id) = receiving()
        cast(data, ServerEvent.Cast("someone-else", itemId = 101))
        assertTrue(launcher.played.isEmpty())

        // In the background the stream names no receiver: what comes for this one then isn't played.
        data.graph.liveUpdates.playing(this, true)
        data.foreground.value = false
        testScheduler.runCurrent()
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        cast(data, ServerEvent.Cast(id, itemId = 101))
        assertTrue(launcher.played.isEmpty())
        data.graph.liveUpdates.playing(this, false)
    }

    @Test
    fun `music plays in the music player, what plays here making way, and says where it came from`() = runTest {
        val heard = mutableListOf<CastMusic>()
        val (data, id) = receiving()
        backgroundScope.launch { receiver.music.collect { heard += it } }
        testScheduler.runCurrent()
        cast(data, ServerEvent.Cast(id, itemId = 303, startMs = 30_000, from = "Kitchen phone"))
        // A track plays its album from it (the fixtures' album has only it), from where the remote says.
        assertEquals(listOf(Triple(listOf(303L), 0, false)), music.played)
        assertEquals(listOf(30_000L), music.seeks)
        assertEquals(1, closed)
        assertEquals(listOf(CastMusic(CastFrom("Kitchen phone"))), heard)
        assertTrue(launcher.played.isEmpty())
    }

    @Test
    fun `an album plays its tracks in order, shuffled when the remote asks`() = runTest {
        val tracks = (1..4L).map { ItemSummary(500 + it, 3, ItemKind.Track, "Track $it", parentId = 500) }
        val (data, id) = receiving {
            api.answer("item", Fixtures.decode<app.pley.android.model.ItemDetail>("items/track_detail.json").copy(id = 500, kind = ItemKind.Album))
            api.answer("children", ItemList(tracks))
        }
        cast(data, ServerEvent.Cast(id, itemId = 500))
        assertEquals(Triple(tracks.map { it.id }, 0, false), music.played.last())
        cast(data, ServerEvent.Cast(id, itemId = 500, shuffle = true))
        assertEquals(true, music.played.last().third)
    }

    @Test
    fun `a playlist plays from its first entry, video as a queue in the player`() = runTest {
        val (data, id) = receiving()
        cast(data, ServerEvent.Cast(id, playlistId = 901, from = "Kitchen phone"))
        assertEquals(
            listOf(PlayRequest(itemId = 101, queue = PlayQueue.Playlist(901), castFrom = CastFrom("Kitchen phone"))),
            launcher.played,
        )
        // Shuffled: from one of its entries, the queue going on from there.
        cast(data, ServerEvent.Cast(id, playlistId = 901, shuffle = true))
        assertTrue(launcher.played.last().itemId in listOf(101L, 203L, 204L))
        assertEquals(PlayQueue.Playlist(901), launcher.played.last().queue)
    }

    @Test
    fun `a music playlist plays in the music player`() = runTest {
        val (data, id) = receiving {
            val video = Fixtures.decode<app.pley.android.model.PlaylistDetail>("playlists/detail.json")
            api.answer("playlist", video.copy(kind = PlaylistKind.Audio))
        }
        cast(data, ServerEvent.Cast(id, playlistId = 901))
        assertEquals(listOf(101L, 203L, 204L), music.played.single().first)
        assertTrue(launcher.played.isEmpty())
    }

    @Test
    fun `a channel is tuned in the live player`() = runTest {
        val (data, id) = receiving()
        cast(data, ServerEvent.Cast(id, channelId = 2, from = "Kitchen phone"))
        assertEquals(listOf(Triple(2L, "all", CastFrom("Kitchen phone"))), launcher.watched)
    }

    /**
     * The token endpoint of a server whose switch tickets are [good]: the
     * forms it was sent (also told to [heard]), a new pair for a good
     * ticket, `ticket_spent` for any other.
     */
    private class TicketServer(private val good: Set<String>, private val heard: (Map<String, String>) -> Unit = {}) {
        val forms: MutableList<Map<String, String>> = java.util.Collections.synchronizedList(mutableListOf())

        val http: OkHttpClient = OkHttpClient.Builder().addInterceptor { chain ->
            val buffer = okio.Buffer()
            chain.request().body?.writeTo(buffer)
            val form = buffer.readUtf8().split('&').filter { it.isNotEmpty() }.associate {
                java.net.URLDecoder.decode(it.substringBefore('='), "UTF-8") to java.net.URLDecoder.decode(it.substringAfter('='), "UTF-8")
            }
            forms += form
            heard(form)
            val (code, body) = if (form["ticket"] in good) {
                200 to """{"access_token":"access-2","token_type":"Bearer","expires_in":3600,"refresh_token":"refresh-2","scope":"library play playlists profiles"}"""
            } else {
                400 to """{"error":"invalid_grant","error_description":"this cast's switch has expired or was used: cast again","reason":"ticket_spent"}"""
            }
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(code).message("")
                .body(body.toResponseBody("application/json".toMediaType())).build()
        }.build()
    }

    /**
     * Runs what is due until [done], for 5 s at most: the sign-in module
     * sends a switch on its own thread, outside the test's scheduler.
     */
    private fun TestScope.settle(done: () -> Boolean) {
        val deadline = System.currentTimeMillis() + 5_000
        while (!done() && System.currentTimeMillis() < deadline) {
            testScheduler.runCurrent()
            Thread.sleep(5)
        }
        testScheduler.runCurrent()
    }

    /** Sam, the profile another remote of the household casts from. */
    private val sam: Me = TestData.me().copy(id = 3, username = "sam", name = "Sam", displayName = "Sam")

    @Test
    fun `a cast from another profile switches to it with its ticket, then plays once as it`() = runTest {
        val tickets = TicketServer(good = setOf("t1"))
        val (data, id) = receiving(FakeAuthGraph(tickets.http))
        assertEquals(2L, data.graph.session.session.value?.me?.id)
        // Who the app is once switched, as the server says.
        data.api.answer("me", sam)
        cast(data, ServerEvent.Cast(id, itemId = 101, startMs = 5_000, from = "Kitchen phone", profileId = 3, switchTicket = "t1"))
        settle { launcher.played.isNotEmpty() }
        // The switch-profile grant, the ticket in place of a PIN.
        val form = tickets.forms.single()
        assertEquals("urn:pley:params:oauth:grant-type:switch-profile" to "3", form["grant_type"] to form["profile"])
        assertEquals("t1" to null, form["ticket"] to form["pin"])
        assertEquals(3L, data.graph.session.session.value?.me?.id)
        assertEquals(listOf(PlayRequest(itemId = 101, startMs = 5_000, castFrom = CastFrom("Kitchen phone"))), launcher.played)

        // Another from Sam's remote, now the profile in use, once the stream is back as Sam's: no switch.
        data.events.send(LiveEvent.Reconnected)
        testScheduler.runCurrent()
        cast(data, ServerEvent.Cast(id, itemId = 101, profileId = 3, switchTicket = "t2"))
        assertEquals(1, tickets.forms.size)
        assertEquals(2, launcher.played.size)
    }

    @Test
    fun `no cast plays while Who's watching shows, nor switches the profile there`() = runTest {
        var asking = true
        val tickets = TicketServer(good = setOf("t1"))
        val (data, id) = receiving(FakeAuthGraph(tickets.http), asking = { asking })
        cast(data, ServerEvent.Cast(id, itemId = 101, from = "Kitchen phone"))
        cast(data, ServerEvent.Cast(id, itemId = 101, from = "Kitchen phone", profileId = 3, switchTicket = "t1"))
        assertTrue(launcher.played.isEmpty())
        assertTrue(tickets.forms.isEmpty())
        assertEquals(2L, data.graph.session.session.value?.me?.id)

        // A profile chosen, casts play again.
        asking = false
        cast(data, ServerEvent.Cast(id, itemId = 101, from = "Kitchen phone"))
        assertEquals(listOf(CastFrom("Kitchen phone")), launcher.played.map { it.castFrom })
    }

    @Test
    fun `what plays ends, its stop report sent, before the switch to the caster's profile`() = runTest {
        val order = java.util.Collections.synchronizedList(mutableListOf<String>())
        val tickets = TicketServer(good = setOf("t1")) { order += "switch to ${it["profile"]}" }
        // The player's last report takes a moment to go out.
        val sent = CompletableDeferred<Unit>()
        val (data, id) = receiving(
            FakeAuthGraph(tickets.http),
            endPlaying = {
                order += "stopping"
                sent.await()
                order += "stopped"
            },
        )
        data.api.answer("me", sam)
        cast(data, ServerEvent.Cast(id, itemId = 101, from = "Kitchen phone", profileId = 3, switchTicket = "t1"))
        // The switch waits for it, as the profile it played for.
        assertEquals(listOf("stopping"), order.toList())
        assertTrue(tickets.forms.isEmpty())
        assertEquals(2L, data.graph.session.session.value?.me?.id)

        // One for the profile it leaves, heard meanwhile, is dropped: the first plays, as Sam.
        cast(data, ServerEvent.Cast(id, itemId = 101, from = "Hall phone"))
        sent.complete(Unit)
        settle { launcher.played.isNotEmpty() }
        assertEquals(listOf("stopping", "stopped", "switch to 3"), order.toList())
        assertEquals(3L, data.graph.session.session.value?.me?.id)
        assertEquals(listOf(CastFrom("Kitchen phone")), launcher.played.map { it.castFrom })
    }

    @Test
    fun `a ticket refused plays nothing and says so, and a cast from another profile without one is ignored`() = runTest {
        val tickets = TicketServer(good = emptySet())
        val refused = mutableListOf<Unit>()
        val (data, id) = receiving(FakeAuthGraph(tickets.http))
        backgroundScope.launch { receiver.refused.collect { refused += it } }
        testScheduler.runCurrent()
        cast(data, ServerEvent.Cast(id, itemId = 101, from = "Kitchen phone", profileId = 3, switchTicket = "spent"))
        settle { refused.isNotEmpty() }
        assertEquals(1, refused.size)
        assertTrue(launcher.played.isEmpty())
        // Still the profile it was on, signed in as before.
        assertEquals(2L, data.graph.session.session.value?.me?.id)

        // An older server's, or a copy: nothing to switch with, nothing played, nothing said.
        cast(data, ServerEvent.Cast(id, itemId = 101, profileId = 3))
        assertEquals(1, tickets.forms.size)
        assertEquals(1, refused.size)
        assertTrue(launcher.played.isEmpty())

        // From the profile in use, said as such: it plays.
        cast(data, ServerEvent.Cast(id, itemId = 101, profileId = 2))
        assertEquals(1, launcher.played.size)
    }

    /** The players as the app launches them: what each was asked to play. */
    private class Launcher : PlayerLauncher {
        val played = mutableListOf<PlayRequest>()
        val watched = mutableListOf<Triple<Long, String, CastFrom?>>()

        override fun play(request: PlayRequest) {
            played += request
        }

        override fun watchLive(channelId: Long, category: String, castFrom: CastFrom?) {
            watched += Triple(channelId, category, castFrom)
        }
    }

    /** The music player: what it was asked to play (the tracks' ids, where from, shuffled) and where to go. */
    private class Music : MusicControl {
        override val nowPlaying = MutableStateFlow<NowPlaying?>(null)
        val played = mutableListOf<Triple<List<Long>, Int, Boolean>>()
        val seeks = mutableListOf<Long>()

        override fun play(tracks: List<ItemSummary>, startIndex: Int, shuffle: Boolean) {
            played += Triple(tracks.map { it.id }, startIndex, shuffle)
        }

        override fun seekTo(positionMs: Long) {
            seeks += positionMs
        }

        override fun pauseForVideo() = Unit
    }
}
