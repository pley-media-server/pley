package app.pley.android.fixtures

import app.pley.android.model.Added
import app.pley.android.model.ApiErrorBody
import app.pley.android.model.AppProfile
import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionList
import app.pley.android.model.DynamicRange
import app.pley.android.model.Health
import app.pley.android.model.HistoryPage
import app.pley.android.model.Hubs
import app.pley.android.model.ItemCollections
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemsPage
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibrarySort
import app.pley.android.model.MarkerKind
import app.pley.android.model.MaybeItem
import app.pley.android.model.Me
import app.pley.android.model.PersonDetail
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistList
import app.pley.android.model.SearchResults
import app.pley.android.model.ServerInfo
import app.pley.android.model.SessionStatus
import app.pley.android.model.StreamKind
import app.pley.android.model.TrickplayInfo
import kotlinx.serialization.builtins.ListSerializer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Every route's answer decodes with :core:model, and holds what the catalogue promises. */
class EndpointsTest {
    @Test
    fun theServerAndTheProfile() = withServer { server ->
        val app = Client(server, server.accessToken())
        assertEquals("ok", app.read("/api/health", Health.serializer()).status)
        assertEquals("Fixture Server", app.read("/api/server", ServerInfo.serializer()).name)
        val me = app.read("/api/auth/me", Me.serializer())
        assertEquals(FixtureIds.ALEX, me.id)
        assertFalse(me.isAdmin)
        assertEquals(listOf("library", "play", "playlists", "profiles"), me.scopes)
        assertEquals(3, app.read("/api/oauth/profiles", ListSerializer(AppProfile.serializer())).size)
    }

    @Test
    fun librariesAndTheirListings() = withServer { server ->
        val app = Client(server, server.accessToken())
        val libraries = app.read("/api/libraries", ListSerializer(Library.serializer()))
        assertEquals(listOf(1_500, 300, 50), libraries.map { it.itemCount })
        assertEquals(libraries[0], app.read("/api/libraries/1", Library.serializer()))
        for (library in libraries) {
            for (sort in LibrarySort.entries) {
                val page = app.read("/api/libraries/${library.id}/items?sort=${sort.wire}&offset=10&limit=50", ItemsPage.serializer())
                assertEquals(library.itemCount, page.total)
                assertEquals(minOf(50, library.itemCount - 10), page.items.size)
                assertTrue(page.items.all { it.backdropUrl == null && it.thumbUrl == null })
                // What the files are like, whatever the sort.
                assertTrue(page.items.all { it.media != null })
            }
            app.read("/api/libraries/${library.id}/filters", LibraryFilters.serializer())
            assertTrue(app.read("/api/libraries/${library.id}/hubs", Hubs.serializer()).hubs.isNotEmpty())
            app.read("/api/libraries/${library.id}/collections", CollectionList.serializer())
        }
        val all = app.read("/api/libraries/1/items", ItemsPage.serializer())
        assertEquals(1_500, all.items.size)
        assertEquals(all.items.map { it.sortTitle.lowercase() }, all.items.map { it.sortTitle.lowercase() }.sorted())
        val comedies = app.read("/api/libraries/1/items?genre=Comedy&year=1999&watched=false", ItemsPage.serializer())
        assertTrue(comedies.items.all { it.year == 1999 && !it.userState.watched })
        assertEquals(400, app.get("/api/libraries/1/items?sort=colour").code)
    }

    @Test
    fun listingsFilterByResolutionAndDynamicRange() = withServer { server ->
        val app = Client(server, server.accessToken())
        val filters = app.read("/api/libraries/1/filters", LibraryFilters.serializer())
        assertEquals(listOf("4k", "1080"), filters.resolutions)
        assertEquals(listOf("hdr", "dolby_vision", "hdr10plus", "hdr10", "sdr"), filters.dynamicRanges)
        fun count(query: String) = app.read("/api/libraries/1/items?$query&limit=0", ItemsPage.serializer()).total
        // Each title is of one resolution and one range.
        assertEquals(1_500, filters.resolutions.sumOf { count("resolution=$it") })
        assertEquals(1_500, count("dynamic_range=hdr") + count("dynamic_range=sdr"))
        val uhd = app.read("/api/libraries/1/items?resolution=4k&dynamic_range=dolby_vision", ItemsPage.serializer())
        assertTrue(uhd.items.isNotEmpty())
        assertTrue(uhd.items.all { it.media?.height == 2160 && it.media?.dynamicRange == DynamicRange.DolbyVision })
        assertTrue(count("dynamic_range=hdr") > count("dynamic_range=hdr10"))
        // A value the server doesn't know is refused; none is no filter.
        assertEquals(400, app.get("/api/libraries/1/items?resolution=8k").code)
        assertEquals(400, app.get("/api/libraries/1/items?dynamic_range=hdr12").code)
        assertEquals(1_500, count("resolution=&dynamic_range="))
        // Music has neither.
        val music = app.read("/api/libraries/3/filters", LibraryFilters.serializer())
        assertTrue(music.resolutions.isEmpty() && music.dynamicRanges.isEmpty())
    }

    @Test
    fun listingsAnswer304WhileNothingChanged() = withServer { server ->
        val app = Client(server, server.accessToken())
        val first = app.get("/api/libraries/1/items?sort=added&limit=200")
        val tag = first.header("ETag")
        assertNotNull(tag)
        assertEquals("private, no-cache", first.header("Cache-Control"))
        assertEquals(304, app.get("/api/libraries/1/items?sort=added&limit=200", mapOf("If-None-Match" to tag!!)).code)
        // Another query, or a change to the profile's state, is another tag.
        assertEquals(200, app.get("/api/libraries/1/items?sort=added&limit=100", mapOf("If-None-Match" to tag)).code)
        assertEquals(204, app.send("POST", "/api/items/100/watched").code)
        assertEquals(200, app.get("/api/libraries/1/items?sort=added&limit=200", mapOf("If-None-Match" to tag)).code)
    }

    @Test
    fun itemsOfEveryKind() = withServer { server ->
        val app = Client(server, server.accessToken())
        val kinds = mapOf(
            FixtureIds.SAMPLE_HDR10 to ItemKind.Movie,
            FixtureIds.SHOW to ItemKind.Show,
            FixtureIds.SHOW_SEASON_1 to ItemKind.Season,
            FixtureIds.SHOW_S1E1 to ItemKind.Episode,
            FixtureIds.ARTIST to ItemKind.Artist,
            FixtureIds.ALBUM to ItemKind.Album,
            FixtureIds.TRACK to ItemKind.Track,
            2_001L to ItemKind.Extra,
        )
        for ((id, kind) in kinds) {
            val item = app.read("/api/items/$id", ItemDetail.serializer())
            assertEquals(kind, item.kind)
            app.read("/api/items/$id/children", ItemList.serializer())
            app.read("/api/items/$id/on-deck", MaybeItem.serializer())
            app.read("/api/items/$id/next", MaybeItem.serializer())
            app.read("/api/items/$id/related", ItemList.serializer())
            app.read("/api/items/$id/collections", ItemCollections.serializer())
        }
        assertEquals(2, app.read("/api/items/${FixtureIds.SHOW}/children", ItemList.serializer()).items.size)
        assertEquals(10, app.read("/api/items/${FixtureIds.ALBUM}/children", ItemList.serializer()).items.size)
        // Alex watched S1E1 and is in the middle of S1E2.
        assertEquals(FixtureIds.SHOW_S1E2, app.read("/api/items/${FixtureIds.SHOW}/on-deck", MaybeItem.serializer()).item?.id)
        assertEquals(FixtureIds.SHOW_S2E1, app.read("/api/items/${FixtureIds.SHOW_S1E3}/next", MaybeItem.serializer()).item?.id)
        assertNull(app.read("/api/items/${FixtureIds.SAMPLE_1080P}/on-deck", MaybeItem.serializer()).item)
        assertEquals(20, app.read("/api/items/${FixtureIds.SAMPLE_1080P}/related", ItemList.serializer()).items.size)
        assertEquals(60, app.read("/api/items/${FixtureIds.SAMPLE_1080P}/related?limit=100", ItemList.serializer()).items.size)

        val show = app.read("/api/items/${FixtureIds.SHOW}", ItemDetail.serializer())
        assertEquals(6, show.leafCount)
        assertEquals(1, show.viewedLeafCount)
        val episode = app.read("/api/items/${FixtureIds.SHOW_S1E1}", ItemDetail.serializer())
        assertEquals(listOf(MarkerKind.Intro, MarkerKind.Credits), episode.markers.map { it.kind })
        assertEquals(20_000L, episode.durationMs)
        assertEquals(404, app.get("/api/items/999999").code)
    }

    @Test
    fun theSamplesForPlaybackPolicies() = withServer { server ->
        val app = Client(server, server.accessToken())
        val hdr = app.read("/api/items/${FixtureIds.SAMPLE_HDR10}", ItemDetail.serializer())
        val file = hdr.files.single()
        assertEquals(FixtureIds.SAMPLE_HDR10_FILE, file.id)
        assertEquals(listOf("Intro", "Part 1", "Credits"), file.chapters.map { it.title })
        assertEquals(DynamicRange.Hdr10, file.streams.first { it.kind == StreamKind.Video }.dynamicRange)
        assertEquals("ac3", file.audioCodec)
        assertEquals(setOf(2, 1000), file.streams.filter { it.kind == StreamKind.Subtitle }.map { it.index }.toSet())

        fun video(id: Long) = app.read("/api/items/$id", ItemDetail.serializer()).files.single().streams.first { it.kind == StreamKind.Video }
        assertEquals(5, video(FixtureIds.DOLBY_VISION_P5).dvProfile)
        assertEquals(0, video(FixtureIds.DOLBY_VISION_P5).dvBlCompat)
        assertEquals(7, video(FixtureIds.DOLBY_VISION_P7).dvProfile)
        assertEquals(8, video(FixtureIds.DOLBY_VISION_P81).dvProfile)
        assertEquals(1, video(FixtureIds.DOLBY_VISION_P81).dvBlCompat)
        assertEquals(DynamicRange.Hdr10Plus, video(FixtureIds.HDR10_PLUS).dynamicRange)
    }

    @Test
    fun peopleCollectionsSearchHistoryAndHubs() = withServer { server ->
        val app = Client(server, server.accessToken())
        val movie = app.read("/api/items/${FixtureIds.SAMPLE_1080P}", ItemDetail.serializer())
        val actor = movie.cast.first()
        val person = app.read("/api/people/${actor.id}", PersonDetail.serializer())
        assertTrue(person.credits.any { it.item.id == movie.id })

        val smart = app.read("/api/collections/${FixtureIds.SMART_COLLECTION}", CollectionDetail.serializer())
        assertNotNull(smart.smartFilter)
        assertTrue(smart.items.isNotEmpty())
        assertTrue(smart.items.all { (it.year ?: 0) in 1990..1999 })
        assertTrue(app.read("/api/collections/${FixtureIds.COLLECTION}", CollectionDetail.serializer()).items.isNotEmpty())

        val results = app.read("/api/search?q=silent+har&limit=200", SearchResults.serializer())
        assertTrue(results.items.isNotEmpty())
        assertTrue(results.items.all { "silent" in it.title.lowercase() })
        assertTrue(app.read("/api/search?q=ada", SearchResults.serializer()).people.isNotEmpty())

        val history = app.read("/api/history?limit=5", HistoryPage.serializer())
        assertEquals(5, history.entries.size)
        assertTrue(history.total > 5)
        assertEquals(history.entries.sortedByDescending { it.viewedAt }, history.entries)
        val home = app.read("/api/hubs/home", Hubs.serializer()).hubs
        assertEquals(listOf("continue_watching", "my_list", "recently_added:1", "recently_added:2", "recently_added:3"), home.map { it.key })
    }

    @Test
    fun playlistsAndTheirChanges() = withServer { server ->
        val app = Client(server, server.accessToken())
        val lists = app.read("/api/playlists", PlaylistList.serializer()).playlists
        assertEquals(3, lists.size)
        val smart = app.read("/api/playlists/${FixtureIds.PLAYLIST_SMART}", PlaylistDetail.serializer())
        assertTrue(smart.entries.isNotEmpty())
        assertEquals(400, app.send("POST", "/api/playlists/${FixtureIds.PLAYLIST_SMART}/items", """{"item_ids":[1]}""").code)

        val made = app.send("POST", "/api/playlists", """{"title":"New","item_ids":[${FixtureIds.SHOW}]}""")
        assertEquals(201, made.code)
        val playlist = made.decode(PlaylistDetail.serializer())
        assertEquals(6, playlist.entries.size)
        val added = app.send("POST", "/api/playlists/${playlist.id}/items", """{"item_ids":[1,2]}""")
        assertEquals(2, added.decode(Added.serializer()).added)
        assertEquals(400, app.send("POST", "/api/playlists/${playlist.id}/items", """{"item_ids":[${FixtureIds.TRACK}]}""").code)
        val last = playlist.entries.last().entryId
        val reordered = app.send("PUT", "/api/playlists/${playlist.id}/order", """{"entry_ids":[$last]}""").decode(PlaylistDetail.serializer())
        assertEquals(last, reordered.entries.first().entryId)
        val renamed = app.send("PATCH", "/api/playlists/${playlist.id}", """{"title":"Renamed"}""").decode(PlaylistDetail.serializer())
        assertEquals("Renamed", renamed.title)
        assertEquals(204, app.send("DELETE", "/api/playlists/${playlist.id}/items/$last").code)
        assertEquals(204, app.send("DELETE", "/api/playlists/${playlist.id}").code)
        assertEquals(404, app.get("/api/playlists/${playlist.id}").code)
        // Another profile's playlists aren't there.
        assertEquals(404, Client(server, server.accessToken(FixtureIds.SAM)).get("/api/playlists/${FixtureIds.PLAYLIST_VIDEO}").code)
    }

    @Test
    fun playbackDirectAndHls() = withServer { server ->
        val app = Client(server, server.accessToken())
        val caps = """{"containers":["mp4"],"video_codecs":["h264"],"audio_codecs":["aac"]}"""
        val direct = app.send("POST", "/api/playback", """{"item_id":${FixtureIds.SAMPLE_1080P},"caps":$caps}""").decode(PlaybackInfo.serializer())
        assertEquals(PlaybackMethod.Direct, direct.method)
        assertEquals("/api/files/${FixtureIds.SAMPLE_1080P_FILE}/stream", direct.url)
        assertEquals(SessionStatus(direct.sessionId, PlaybackMethod.Direct), app.read("/api/playback/${direct.sessionId}", SessionStatus.serializer()))

        val hls = app.send("POST", "/api/playback", """{"item_id":${FixtureIds.SAMPLE_HDR10},"caps":$caps}""").decode(PlaybackInfo.serializer())
        assertEquals(PlaybackMethod.Transcode, hls.method)
        assertTrue(hls.hls)
        assertEquals("/api/playback/${hls.sessionId}/index.m3u8", hls.url)
        assertTrue(hls.reasons.isNotEmpty())
        assertEquals("/api/files/${FixtureIds.SAMPLE_HDR10_FILE}/subtitles/2.vtt", hls.subtitles.first { it.index == 2 }.url)
        assertNotNull(app.read("/api/playback/${hls.sessionId}", SessionStatus.serializer()).transcode)

        val hevc = """{"containers":["mkv"],"video_codecs":["hevc"],"audio_codecs":["aac"],"hdr":true}"""
        val remux = app.send("POST", "/api/playback", """{"item_id":${FixtureIds.SAMPLE_HDR10},"caps":$hevc}""").decode(PlaybackInfo.serializer())
        assertEquals(PlaybackMethod.Remux, remux.method)
        assertTrue(remux.audioConverted)
        val p5 = """{"containers":["mkv"],"video_codecs":["hevc"],"audio_codecs":["ac3"],"hdr":true}"""
        assertEquals(PlaybackMethod.Transcode, app.send("POST", "/api/playback", """{"item_id":${FixtureIds.DOLBY_VISION_P5},"caps":$p5}""").decode(PlaybackInfo.serializer()).method)
        assertEquals(PlaybackMethod.Direct, app.send("POST", "/api/playback", """{"item_id":${FixtureIds.DOLBY_VISION_P81},"caps":$p5}""").decode(PlaybackInfo.serializer()).method)

        assertEquals(204, app.send("DELETE", "/api/playback/${hls.sessionId}").code)
        assertEquals(404, app.get("/api/playback/${hls.sessionId}").code)
        assertEquals(404, app.get("/api/playback/${hls.sessionId}/0.ts").code)

        val trickplay = app.read("/api/files/${FixtureIds.SAMPLE_HDR10_FILE}/trickplay", TrickplayInfo.serializer())
        assertEquals(20, trickplay.count)
        assertEquals(1, trickplay.sheets)
    }

    @Test
    fun refusalsAreTheServersErrorBodies() = withServer { server ->
        val app = Client(server, server.accessToken())
        val missing = app.get("/api/items/0")
        assertEquals(404, missing.code)
        assertEquals("not_found", missing.decode(ApiErrorBody.serializer()).code)
        assertEquals(404, app.get("/api/nothing/here").code)
    }
}
