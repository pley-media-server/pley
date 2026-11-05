package app.pley.android.data

import app.pley.android.model.LiveCategory
import app.pley.android.model.ServerEvent
import app.pley.android.model.WatchRequest
import app.pley.android.network.ApiException
import app.pley.android.network.LiveEvent
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** Live TV's groups, pages, guide by channel, favourites and watching. */
class LiveTvTest {
    @get:Rule
    val folder = TemporaryFolder()

    @Test
    fun `a category is asked for a page at a time, and the page around a channel`() = runTest {
        val data = signedIn(folder.newFolder())
        val news = collect(data.graph.liveTv.page(LiveCategory.Group("News"), "", 0))
        val around = collect(data.graph.liveTv.around(LiveCategory.All, 5))
        testScheduler.runCurrent()
        assertEquals(listOf(1L, 5L), news.last().valueOrNull!!.channels.map { it.id })
        assertEquals(listOf<Any?>(LiveCategory.Group("News"), "", 0, LiveTvRepository.PAGE, null), data.api.calls("liveBrowse").first().args)
        assertEquals(listOf<Any?>(LiveCategory.All, null, 0, LiveTvRepository.PAGE, 5L), data.api.calls("liveBrowse").last().args)
        assertEquals(3, around.last().valueOrNull!!.total)
    }

    @Test
    fun `a favourite shows at once on every page and count, and is put back when refused`() = runTest {
        val data = signedIn(folder.newFolder())
        val all = collect(data.graph.liveTv.page(LiveCategory.All, "", 0))
        val groups = collect(data.graph.liveTv.groups())
        val channel = collect(data.graph.liveTv.channel(1))
        testScheduler.runCurrent()
        assertFalse(channel.last().valueOrNull!!.favorite)

        data.api.hold()
        val change = launch { data.graph.liveTv.setFavorite(1, true) }
        testScheduler.runCurrent()
        assertTrue(all.last().valueOrNull!!.channels.first { it.id == 1L }.favorite)
        assertTrue(channel.last().valueOrNull!!.favorite)
        assertEquals(2, groups.last().valueOrNull!!.favorites)
        data.api.release()
        change.join()
        assertEquals(listOf<Any?>(1L, true), data.api.calls("setFavoriteChannel").single().args)

        val refused = ApiException(500, "internal", "the server failed")
        data.api.fail("setFavoriteChannel", refused)
        try {
            data.graph.liveTv.setFavorite(2, false)
            fail("refused")
        } catch (e: ApiException) {
            assertSame(refused, e)
        }
        assertTrue(all.last().valueOrNull!!.channels.first { it.id == 2L }.favorite)
    }

    @Test
    fun `the guide asks for the channels in view only`() = runTest {
        val data = signedIn(folder.newFolder())
        val guide = collect(data.graph.liveTv.guide(1761742800, listOf(2)))
        collect(data.graph.liveTv.guide(1761742800, listOf(2)))
        testScheduler.runCurrent()
        assertEquals(setOf(2L), guide.last().valueOrNull!!.programs.keys)
        assertEquals(listOf<Any?>(1761742800L, 1761742800L + LiveTvRepository.GUIDE_CHUNK_S, listOf(2L)), data.api.calls("liveGuide").single().args)
    }

    @Test
    fun `a number tunes the channel numbered so exactly`() = runTest {
        val data = signedIn(folder.newFolder())
        assertEquals(5L, data.graph.liveTv.numbered("105")?.id)
        assertNull(data.graph.liveTv.numbered("10"))
    }

    @Test
    fun `watching sends this device's id and answers an absolute address`() = runTest {
        val data = signedIn(folder.newFolder())
        val watching = data.graph.liveTv.watch(5, "android-1")
        assertEquals(listOf<Any?>(5L, WatchRequest("android-1")), data.api.calls("watchChannel").single().args)
        assertEquals("${data.issuer}/api/live/sessions/live-5/index.m3u8", watching.url)
    }

    @Test
    fun `a session's plan is asked for, and the channel's stream ts carries this device's id`() = runTest {
        val data = signedIn(folder.newFolder())
        assertEquals(true, data.graph.liveTv.plan("live-5")?.direct)
        assertEquals(listOf<Any?>("live-5"), data.api.calls("livePlan").single().args)
        assertEquals("${data.issuer}/api/live/channels/5/stream.ts?client_id=android%201%2F2", data.graph.liveTv.streamUrl(5, "android 1/2"))
    }

    @Test
    fun `leaving outlives whoever asked, and a failure to is no one's`() = runTest {
        val data = signedIn(folder.newFolder())
        data.graph.liveTv.leave("s1")
        testScheduler.runCurrent()
        assertEquals(listOf<Any?>("s1"), data.api.calls("leaveChannel").single().args)

        data.api.fail("leaveChannel", ApiException(500, "internal", "no"))
        data.graph.liveTv.leave("s2")
        testScheduler.runCurrent()
        assertEquals(2, data.api.calls("leaveChannel").size)
    }

    @Test
    fun `a channel watched before a drop is never left after it, as the next profile`() = runTest {
        val data = signedIn(folder.newFolder())
        val live = data.graph.liveTv
        val before = live.watch(5, "android-1").sessionId
        // Another asked for as the profile switches, answered after.
        data.api.hold()
        val asked = async { live.watch(6, "android-1").sessionId }
        testScheduler.runCurrent()

        // What played ended, and its window took longer than the wait: the switch goes ahead.
        live.drop()
        data.api.release()
        val during = asked.await()
        // The windows go at last, as the next profile.
        live.leave(before)
        live.leave(during)
        testScheduler.runCurrent()
        assertTrue(data.api.calls("leaveChannel").isEmpty())

        // The next profile's own channel is left as usual.
        val after = live.watch(7, "android-1").sessionId
        live.leave(after)
        testScheduler.runCurrent()
        assertEquals(listOf<Any?>(after), data.api.calls("leaveChannel").single().args)
    }

    @Test
    fun `the server's Live TV changes ask again for the groups, the pages and the guide`() = runTest {
        val data = signedIn(folder.newFolder())
        collect(data.graph.liveTv.groups())
        collect(data.graph.liveTv.page(LiveCategory.All, "", 0))
        collect(data.graph.liveTv.guide(0, listOf(1)))
        collect(data.graph.hubs.home())
        data.graph.liveUpdates.start()
        testScheduler.runCurrent()

        data.events.send(LiveEvent.Event(ServerEvent.LiveChannelsChanged))
        testScheduler.runCurrent()
        assertEquals(2, data.api.calls("liveGroups").size)
        assertEquals(2, data.api.calls("liveBrowse").size)
        assertEquals(2, data.api.calls("liveGuide").size)
        assertEquals(1, data.api.calls("homeHubs").size)

        // Who watches what: nothing a viewer shows.
        data.events.send(LiveEvent.Event(ServerEvent.LiveTunersChanged))
        testScheduler.runCurrent()
        assertEquals(2, data.api.calls("liveBrowse").size)
    }
}
