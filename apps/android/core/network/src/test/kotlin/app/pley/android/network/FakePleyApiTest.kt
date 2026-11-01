package app.pley.android.network

import app.pley.android.model.ConversionState
import app.pley.android.model.Decline
import app.pley.android.model.DiscoverList
import app.pley.android.model.EntryIds
import app.pley.android.model.NewRequest
import app.pley.android.model.RequestQuery
import app.pley.android.model.RequestStatus
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.model.ImageKind
import app.pley.android.model.ItemIds
import app.pley.android.model.LibraryIds
import app.pley.android.model.LibrarySort
import app.pley.android.model.NewPlaylist
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PlaylistPatch
import app.pley.android.model.ProgressReport
import app.pley.android.model.RatingRequest
import app.pley.android.model.StreamChoice
import app.pley.android.testing.network.FakePleyApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FakePleyApiTest {
    private val api = FakePleyApi()

    @Test
    fun everyMethodAnswersFromTheFixtures() = runTest {
        assertEquals("ok", api.health().status)
        assertEquals("Living Room", api.server().name)
        assertEquals(2L, api.me().id)
        assertEquals(3, api.libraries().size)
        assertEquals("Movies", api.library(1).name)
        assertEquals(listOf(3L, 1L, 2L), api.sortLibraries(LibraryIds(listOf(3, 1, 2))).map { it.id })
        assertEquals(listOf(1L, 2L, 3L), api.forgetLibraryOrder().map { it.id })
        assertEquals(2, api.libraryItems(1, offset = 0, limit = 2).items.size)
        assertNotNull(api.libraryItems(1, sort = LibrarySort.Bitrate).items.first().media)
        api.libraryFilters(1)
        api.libraryHubs(1)
        api.libraryCollections(1)
        assertEquals(801L, api.collection(801).id)
        assertEquals(803L, api.collection(803).id)
        api.playlists()
        assertEquals(902L, api.playlist(902).id)
        assertEquals("Arrival", api.item(101).title)
        assertEquals("Severance", api.item(201).title)
        assertEquals(106L, api.item(106).id)
        assertEquals(555L, api.item(555).id)
        assertTrue(api.children(201).items.isNotEmpty())
        assertTrue(api.children(202).items.isNotEmpty())
        assertNotNull(api.onDeck(201).item)
        assertEquals(null, api.next(203).item)
        assertTrue(api.related(101, limit = 1).items.size <= 1)
        api.itemCollections(101)
        assertEquals(706L, api.person(706).id)
        api.history()
        api.homeHubs()
        api.search("arrival")
        assertEquals(PlaybackMethod.Direct, api.startPlayback(PlaybackRequest(itemId = 101)).method)
        val hls = api.startPlayback(PlaybackRequest(itemId = 101, mode = PlaybackMode.Transcode))
        assertEquals(PlaybackMethod.Transcode, hls.method)
        assertNotNull(api.playbackStatus(hls.sessionId).transcode)
        assertEquals("other", api.playbackStatus("other").sessionId)
        api.stopPlayback(hls.sessionId)
        assertTrue(api.playbackNetwork("android-test").local)
        api.reportNetwork(app.pley.android.model.NetworkReport.Ceiling("android-test", 8_000))
        assertTrue(api.speedTest(1_000_000, 2_000, 3_000).complete)
        assertTrue(api.searchSubtitles(101, null, listOf("en")).setup)
        assertTrue(api.addSubtitle(101, app.pley.android.model.AddSubtitle("opensubtitles:1")).added)
        assertEquals(ConversionState.Direct, api.convert(PlaybackRequest(itemId = 101, fileId = 601)).state)
        assertEquals(ConversionState.Done, api.conversion("c1").state)
        api.cancelConversion("c1")
        assertTrue(api.subtitles(601, 3).complete)
        assertNotNull(api.trickplay(601))
        api.reportProgress(101, ProgressReport(positionMs = 1_000))
        api.markWatched(101)
        api.markUnwatched(101)
        api.setStreams(101, StreamChoice(audioIndex = 1))
        api.setRating(101, RatingRequest(8.0))
        api.deleteHistoryEntry(1)
        assertEquals(listOf(201L, 104L), api.watchlist().items.map { it.id })
        api.addToWatchlist(101)
        api.removeFromWatchlist(201)
        api.hideFromContinueWatching(204)
        assertTrue(api.liveSummary().available)
        assertEquals(3, api.liveChannels().size)
        assertEquals(1761742800L, api.liveGuide(1761742800, 1761764400).from)
        assertEquals(setOf(2L), api.liveGuide(1761742800, 1761764400, listOf(2)).programs.keys)
        assertEquals(5, api.liveGroups().groups.size)
        assertEquals(listOf(1L, 5L), api.liveBrowse(app.pley.android.model.LiveCategory.Group("News")).channels.map { it.id })
        assertEquals(2, api.liveBrowse(app.pley.android.model.LiveCategory.All, limit = 2, around = 5).offset)
        assertEquals("Weather Now", api.liveChannel(5).name)
        api.setFavoriteChannel(1, true)
        val watching = api.watchChannel(1, app.pley.android.model.WatchRequest("android-test"))
        assertEquals("/api/live/sessions/live-1/index.m3u8", watching.url)
        assertTrue(api.livePlan(watching.sessionId)!!.direct)
        api.leaveChannel(watching.sessionId)
        assertEquals("New", api.createPlaylist(NewPlaylist(title = "New", itemIds = listOf(101))).title)
        assertEquals("Renamed", api.updatePlaylist(901, PlaylistPatch(title = "Renamed")).title)
        api.deletePlaylist(901)
        assertEquals(9, api.addToPlaylist(901, ItemIds(listOf(101))).added)
        api.removeFromPlaylist(901, 1)
        val detail = api.playlist(901)
        val last = detail.entries.last().entryId
        assertEquals(last, api.reorderPlaylist(901, EntryIds(listOf(last))).entries.first().entryId)
        assertTrue(api.requestSummary().enabled)
        assertEquals(2, api.discover(DiscoverList.Movies, page = 2).page)
        assertTrue(api.discoverSearch("a", missing = true).results.none { it.status == TitleStatus.Library })
        assertEquals("Breaking Bad", api.discoverTitle(TitleKind.Show, 1396).title)
        assertEquals("The Wild Robot", api.discoverTitle(TitleKind.Movie, 1184918).title)
        assertEquals(listOf(2), api.itemRequest(201).askable)
        assertEquals(6, api.requests().requests.size)
        assertEquals(2, api.requests(RequestQuery(attention = true, kind = TitleKind.Movie)).requests.size)
        assertEquals(44L, api.request(44).id)
        assertTrue(api.ask(NewRequest(TitleKind.Movie, 1184918)).created)
        api.deleteRequest(44)
        api.unwatchRequest(47)
        api.requestsSeen()
        assertEquals(RequestStatus.Approved, api.approveRequest(44).status)
        assertEquals("No", api.declineRequest(44, Decline("No")).reason)
        assertEquals("Added by hand", api.requestHandled(49).note)
        assertEquals(6L, api.requestOptions().sonarr?.qualityProfile)

        val suspending =PleyApi::class.java.methods.filter { m -> m.parameterTypes.lastOrNull()?.name == "kotlin.coroutines.Continuation" }
        val called = api.calls.map { it.method }.toSet()
        assertEquals(suspending.map { it.name }.toSet() - called, emptySet<String>())
    }

    @Test
    fun addressesAreTheRealOnes() {
        val real = HttpPleyApi("http://127.0.0.1:7539", { error("unused") })
        assertEquals(real.itemImageUrl(101, ImageKind.Poster, 300, "1"), api.itemImageUrl(101, ImageKind.Poster, 300, "1"))
        assertEquals(real.fileStreamUrl(601), api.fileStreamUrl(601))
        assertEquals(real.url("/api/items/1/images/poster?v=2", 100), api.url("/api/items/1/images/poster?v=2", 100))
    }

    @Test
    fun callsAreRecordedAndAnswersChanged() = runTest {
        api.markWatched(101)
        assertEquals(listOf(FakePleyApi.Call("markWatched", listOf(101L))), api.calls)

        val error = ApiException(403, "insufficient_scope", "no", scope = "play")
        api.fail("markWatched", error)
        try {
            api.markWatched(101)
            throw AssertionError("no failure")
        } catch (e: ApiException) {
            assertEquals(error, e)
        }

        api.answer("libraries", emptyList<Any>())
        assertEquals(emptyList<Any>(), api.libraries())

        api.reset()
        assertEquals(3, api.libraries().size)
        assertEquals(1, api.calls.size)
    }

    @Test
    fun heldCallsWaitUntilReleased() = runTest {
        api.hold()
        var done = false
        launch {
            api.homeHubs()
            done = true
        }
        testScheduler.runCurrent()
        assertEquals(false, done)
        api.release()
        testScheduler.runCurrent()
        assertEquals(true, done)
    }
}
