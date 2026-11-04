@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.screens.remote

import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
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
import app.pley.android.testing.data.TestData
import kotlin.random.Random
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
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

    /** Signed in, in front, the stream connected naming this device: the receiver listening, and this device's id. */
    private suspend fun TestScope.receiving(prepare: TestData.() -> Unit = {}): Pair<TestData, String> {
        val data = TestData(this, folder.newFolder())
        data.prepare()
        data.signIn()
        data.useServer()
        data.graph.session
        testScheduler.runCurrent()
        ServerCastReceiver(data.graph, launcher, { music }, backgroundScope, Random(7), closePlayers = { closed++ }).also { receiver = it }.start()
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
    fun `a show plays its next episode as its Play does`() = runTest {
        val (data, id) = receiving()
        cast(data, ServerEvent.Cast(id, itemId = 201, startMs = 5_000, from = "Kitchen phone"))
        // The episode on deck, where it was left: a place is a title's own.
        assertEquals(listOf(PlayRequest(itemId = 204, startMs = 1_185_000, castFrom = CastFrom("Kitchen phone"))), launcher.played)
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
