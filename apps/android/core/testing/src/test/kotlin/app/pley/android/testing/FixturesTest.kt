package app.pley.android.testing

import app.pley.android.model.AppProfile
import app.pley.android.model.ApiErrorBody
import app.pley.android.model.Added
import app.pley.android.model.ArrService
import app.pley.android.model.DiscoverDetail
import app.pley.android.model.DiscoverPage
import app.pley.android.model.DownloadState
import app.pley.android.model.ItemRequest
import app.pley.android.model.RequestAccess
import app.pley.android.model.RequestAction
import app.pley.android.model.RequestList
import app.pley.android.model.RequestOptions
import app.pley.android.model.RequestStage
import app.pley.android.model.RequestStatus
import app.pley.android.model.RequestSummary
import app.pley.android.model.SeasonState
import app.pley.android.model.TitleKind
import app.pley.android.model.TitleStatus
import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionList
import app.pley.android.model.CollectionSort
import app.pley.android.model.ConversionInfo
import app.pley.android.model.CreditRole
import app.pley.android.model.DynamicRange
import app.pley.android.model.DynamicRangeFilter
import app.pley.android.model.EntryIds
import app.pley.android.model.ExtraType
import app.pley.android.model.Health
import app.pley.android.model.HistoryPage
import app.pley.android.model.Hubs
import app.pley.android.model.ItemCollections
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemIds
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemsPage
import app.pley.android.model.JobState
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibraryKind
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveChannelPage
import app.pley.android.model.LivePlan
import app.pley.android.model.LiveGroups
import app.pley.android.model.LiveGuide
import app.pley.android.model.LiveSummary
import app.pley.android.model.LiveWatching
import app.pley.android.model.MarkerKind
import app.pley.android.model.MarkerSource
import app.pley.android.model.MaybeItem
import app.pley.android.model.Me
import app.pley.android.model.NewPlaylist
import app.pley.android.model.PersonDetail
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.PlayerCommandRequest
import app.pley.android.model.PlayingSession
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistList
import app.pley.android.model.PlaylistPatch
import app.pley.android.model.PleyJson
import app.pley.android.model.ProfileNeeds
import app.pley.android.model.ProgressReport
import app.pley.android.model.ProgressState
import app.pley.android.model.RatingRequest
import app.pley.android.model.ResolutionFilter
import app.pley.android.model.ScanPhase
import app.pley.android.model.SearchResults
import app.pley.android.model.ServerEvent
import app.pley.android.model.ServerInfo
import app.pley.android.model.SessionStatus
import app.pley.android.model.StreamChoice
import app.pley.android.model.StreamKind
import app.pley.android.model.SubtitleMode
import app.pley.android.model.TrackDefaults
import app.pley.android.model.TrickplayInfo
import app.pley.android.model.UserState
import app.pley.android.model.WatchRequest
import app.pley.android.model.caps.ClientCaps
import app.pley.android.model.caps.DeviceForm
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ListSerializer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

class FixturesTest {
    /** Every fixture and what it is read as. A new fixture goes here too ([everyFixtureIsListed]). */
    private val table: Map<String, DeserializationStrategy<*>> = mapOf(
        "server/info.json" to ServerInfo.serializer(),
        "server/health.json" to Health.serializer(),
        "errors/not_found.json" to ApiErrorBody.serializer(),
        "errors/insufficient_scope.json" to ApiErrorBody.serializer(),
        "me/app_token.json" to Me.serializer(),
        "me/managed_profile.json" to Me.serializer(),
        "me/track_defaults.json" to TrackDefaults.serializer(),
        "profiles/household.json" to ListSerializer(AppProfile.serializer()),
        "libraries/list.json" to ListSerializer(Library.serializer()),
        "libraries/library.json" to Library.serializer(),
        "libraries/items.json" to ItemsPage.serializer(),
        "libraries/items_by_bitrate.json" to ItemsPage.serializer(),
        "libraries/filters.json" to LibraryFilters.serializer(),
        "libraries/hubs.json" to Hubs.serializer(),
        "libraries/collections.json" to CollectionList.serializer(),
        "collections/detail.json" to CollectionDetail.serializer(),
        "collections/smart_detail.json" to CollectionDetail.serializer(),
        "items/movie_detail.json" to ItemDetail.serializer(),
        "items/episode_detail.json" to ItemDetail.serializer(),
        "items/show_detail.json" to ItemDetail.serializer(),
        "items/extra.json" to ItemDetail.serializer(),
        "items/track_detail.json" to ItemDetail.serializer(),
        "items/minimal_detail.json" to ItemDetail.serializer(),
        "items/explicit_nulls.json" to ItemDetail.serializer(),
        "items/unknown_fields.json" to ItemDetail.serializer(),
        "items/children_seasons.json" to ItemList.serializer(),
        "items/children_episodes.json" to ItemList.serializer(),
        "items/related.json" to ItemList.serializer(),
        "items/on_deck.json" to MaybeItem.serializer(),
        "items/next_none.json" to MaybeItem.serializer(),
        "items/collections.json" to ItemCollections.serializer(),
        "people/detail.json" to PersonDetail.serializer(),
        "history/page.json" to HistoryPage.serializer(),
        "hubs/home.json" to Hubs.serializer(),
        "watchlist/list.json" to ItemList.serializer(),
        "search/results.json" to SearchResults.serializer(),
        "playlists/list.json" to PlaylistList.serializer(),
        "playlists/detail.json" to PlaylistDetail.serializer(),
        "playlists/smart_detail.json" to PlaylistDetail.serializer(),
        "playlists/added.json" to Added.serializer(),
        "playlists/create_request.json" to NewPlaylist.serializer(),
        "playlists/create_smart_request.json" to NewPlaylist.serializer(),
        "playlists/patch_request.json" to PlaylistPatch.serializer(),
        "playlists/add_items_request.json" to ItemIds.serializer(),
        "playlists/order_request.json" to EntryIds.serializer(),
        "playback/request.json" to PlaybackRequest.serializer(),
        "playback/info_direct.json" to PlaybackInfo.serializer(),
        "playback/info_transcode.json" to PlaybackInfo.serializer(),
        "playback/status_transcode.json" to SessionStatus.serializer(),
        "playback/status_direct.json" to SessionStatus.serializer(),
        "playback/progress_request.json" to ProgressReport.serializer(),
        "playback/streams_request.json" to StreamChoice.serializer(),
        "playback/rating_request.json" to RatingRequest.serializer(),
        "sessions/list.json" to ListSerializer(PlayingSession.serializer()),
        "sessions/command_request.json" to PlayerCommandRequest.serializer(),
        "conversions/direct.json" to ConversionInfo.serializer(),
        "conversions/running.json" to ConversionInfo.serializer(),
        "trickplay/info.json" to TrickplayInfo.serializer(),
        "caps/v1.json" to ClientCaps.serializer(),
        "caps/v2.json" to ClientCaps.serializer(),
        "events/scan_started.json" to ServerEvent.Serializer,
        "events/scan_progress.json" to ServerEvent.Serializer,
        "events/scan_finished.json" to ServerEvent.Serializer,
        "events/library_changed.json" to ServerEvent.Serializer,
        "events/libraries_changed.json" to ServerEvent.Serializer,
        "events/sessions_changed.json" to ServerEvent.Serializer,
        "events/player_command.json" to ServerEvent.Serializer,
        "events/unknown_type.json" to ServerEvent.Serializer,
        "live/summary.json" to LiveSummary.serializer(),
        "live/channels.json" to ListSerializer(LiveChannel.serializer()),
        "live/guide.json" to LiveGuide.serializer(),
        "live/watching.json" to LiveWatching.serializer(),
        "live/watch_request.json" to WatchRequest.serializer(),
        "live/busy.json" to ApiErrorBody.serializer(),
        "live/groups.json" to LiveGroups.serializer(),
        "live/browse.json" to LiveChannelPage.serializer(),
        "live/plan.json" to LivePlan.serializer(),
        "events/requests_changed.json" to ServerEvent.Serializer,
        "events/request_available.json" to ServerEvent.Serializer,
        "events/request_decided.json" to ServerEvent.Serializer,
        "events/request_created.json" to ServerEvent.Serializer,
        "events/request_failed.json" to ServerEvent.Serializer,
        "events/requests_settings_changed.json" to ServerEvent.Serializer,
        "requests/summary.json" to RequestSummary.serializer(),
        "requests/summary_manager.json" to RequestSummary.serializer(),
        "requests/list.json" to RequestList.serializer(),
        "requests/attention.json" to RequestList.serializer(),
        "requests/options.json" to RequestOptions.serializer(),
        "requests/item_request.json" to ItemRequest.serializer(),
        "discover/trending.json" to DiscoverPage.serializer(),
        "discover/movie.json" to DiscoverDetail.serializer(),
        "discover/show.json" to DiscoverDetail.serializer(),
        "errors/tmdb_down.json" to ApiErrorBody.serializer(),
        "errors/declined.json" to ApiErrorBody.serializer(),
        "errors/conflict_request.json" to ApiErrorBody.serializer(),
        "errors/not_controllable.json" to ApiErrorBody.serializer(),
    )

    @Test
    fun everyFixtureDecodes() {
        val failures = table.mapNotNull { (path, deserializer) ->
            runCatching { Fixtures.decode(path, deserializer) }.exceptionOrNull()?.let { "$path: $it" }
        }
        assertTrue(failures.joinToString("\n"), failures.isEmpty())
    }

    @Test
    fun everyFixtureIsListed() {
        val files = Fixtures.all()
        assertTrue("no fixtures found on the classpath", files.isNotEmpty())
        assertEquals("fixtures missing from the table", emptyList<String>(), files - table.keys)
        assertEquals("table entries without a file", emptyList<String>(), (table.keys - files.toSet()).toList())
        assertEquals(96, files.size)
    }

    @Test
    fun aMissingFixtureSaysWhere() {
        try {
            Fixtures.text("items/nothing.json")
            fail("expected a missing fixture to fail")
        } catch (e: IllegalArgumentException) {
            assertTrue(e.message!!, "fixtures/items/nothing.json" in e.message!!)
        }
    }

    @Test
    fun theMovieHasWhatAppsGet() {
        val movie = Fixtures.decode<ItemDetail>("items/movie_detail.json")
        assertEquals(ItemKind.Movie, movie.kind)
        assertEquals(listOf("tagline"), movie.lockedFields)
        val file = movie.files.first()
        assertFalse("apps get file names only", '/' in file.path || '\\' in file.path)
        val video = file.streams.first { it.kind == StreamKind.Video }
        assertEquals(DynamicRange.DolbyVision, video.dynamicRange)
        assertEquals(8, video.dvProfile)
        assertEquals(6, video.dvLevel)
        assertEquals(1, video.dvBlCompat)
        assertEquals("yuv420p10le", video.pixFmt)
        assertEquals(153, video.level)
        assertEquals(false, video.interlaced)
        assertEquals("smpte2084", video.colorTransfer)
        assertEquals("bt2020", video.colorPrimaries)
        assertEquals("bt2020nc", video.colorSpace)
        assertTrue(file.streams.single { it.index == 1000 }.external)
        assertEquals(MarkerSource.Chapter, movie.markers.single().source)
        assertEquals(listOf(ExtraType.Trailer, ExtraType.Featurette), movie.extras.map { it.extraType })
        assertTrue(movie.extras.all { it.kind == ItemKind.Extra && it.parentId == movie.id })
        assertEquals(CreditRole.Director, movie.crew.first().role)
        assertEquals("Open Matte", movie.files[1].edition)
        assertEquals(movie.id, movie.toSummary().id)
    }

    @Test
    fun anExtraPlays() {
        val extra = Fixtures.decode<ItemDetail>("items/extra.json")
        assertEquals(ItemKind.Extra, extra.kind)
        assertTrue(extra.kind.playable)
        assertEquals(ExtraType.Trailer, extra.extraType)
        assertEquals(101L, extra.parentId)
    }

    @Test
    fun anEpisodeHasItsShowAndMarkers() {
        val episode = Fixtures.decode<ItemDetail>("items/episode_detail.json")
        assertEquals(201L, episode.grandparentId)
        assertEquals(1, episode.parentIndex)
        assertEquals(listOf(MarkerKind.Intro, MarkerKind.Credits), episode.markers.map { it.kind })
        assertEquals(listOf(MarkerSource.Fingerprint, MarkerSource.Frames), episode.markers.map { it.source })
    }

    @Test
    fun omittedFieldsTakeDefaults() {
        val item = Fixtures.decode<ItemDetail>("items/minimal_detail.json")
        assertEquals("", item.sortTitle)
        assertEquals(UserState(), item.userState)
        assertNull(item.year)
        assertTrue(item.files.isEmpty() && item.markers.isEmpty() && item.extras.isEmpty() && item.lockedFields.isEmpty())
    }

    @Test
    fun explicitNullsReadLikeOmittedOnes() {
        val nulls = Fixtures.decode<ItemDetail>("items/explicit_nulls.json")
        val omitted = Fixtures.decode<ItemDetail>("items/minimal_detail.json")
        assertEquals(omitted, nulls.copy(files = emptyList()))
        val file = nulls.files.single()
        assertEquals(0L, file.size)
        assertTrue(file.streams.isEmpty() && file.chapters.isEmpty())
    }

    @Test
    fun unknownFieldsAndNamesDontBreakAnything() {
        val item = Fixtures.decode<ItemDetail>("items/unknown_fields.json")
        assertEquals(ItemKind.Unknown, item.kind)
        assertFalse(item.kind.playable)
        assertEquals(ExtraType.Unknown, item.extraType)
        assertEquals(DynamicRange.Unknown, item.media?.dynamicRange)
        assertEquals(listOf(StreamKind.Video, StreamKind.Audio, StreamKind.Unknown), item.files.single().streams.map { it.kind })
        assertEquals(DynamicRange.Unknown, item.files.single().streams.first().dynamicRange)
        assertEquals(MarkerKind.Unknown, item.markers.single().kind)
        assertEquals(MarkerSource.Unknown, item.markers.single().source)
        assertEquals(CreditRole.Unknown, item.cast.single().role)
        assertNull(item.cast.single().photoUrl)

        assertEquals(ServerEvent.Unknown("transcoder_restarted"), Fixtures.decode<ServerEvent>("events/unknown_type.json"))
    }

    @Test
    fun appsSeeNoPathsAndNoAdmin() {
        assertTrue(Fixtures.decode<List<Library>>("libraries/list.json").all { it.paths.isEmpty() })
        assertEquals(listOf(LibraryKind.Movies, LibraryKind.Shows, LibraryKind.Music), Fixtures.decode<List<Library>>("libraries/list.json").map { it.kind })
        val me = Fixtures.decode<Me>("me/app_token.json")
        assertFalse(me.isAdmin)
        assertEquals(listOf("library", "play", "playlists", "profiles"), me.scopes)
        assertEquals(TrackDefaults("eng", null, SubtitleMode.Foreign), me.trackDefaults)
        val kids = Fixtures.decode<Me>("me/managed_profile.json")
        assertEquals(listOf(1L, 2L), kids.libraryIds)
        assertEquals(10, kids.maxAge)
        assertEquals(SubtitleMode.Always, Fixtures.decode<TrackDefaults>("me/track_defaults.json").subtitleMode)
    }

    @Test
    fun requestsReadAsTheServerSendsThem() {
        val show = Fixtures.decode<DiscoverDetail>("discover/show.json")
        assertEquals(TitleKind.Show, show.kind)
        assertEquals(TitleStatus.Partial, show.status)
        assertEquals(listOf(0, 1, 2, 3, 4, 5), show.seasons!!.map { it.number })
        assertEquals(SeasonState.Requested, show.seasons!![1].state)
        assertTrue(show.seasons!![4].declined)
        val request = show.requests.single()
        assertEquals(RequestStage.Downloading, request.stage)
        assertEquals(DownloadState.Downloading, request.download?.state)
        assertTrue("pictures are the server's", show.posterUrl!!.startsWith("/api/discover/images/poster/"))
        assertNull(Fixtures.decode<DiscoverDetail>("discover/movie.json").seasons)

        val list = Fixtures.decode<RequestList>("requests/list.json")
        assertEquals(1, list.count(RequestStatus.Pending))
        assertEquals(listOf(RequestAction.Unwatch), list.requests.single { it.watching }.actions)
        assertNull(list.requests.single { it.watching }.requestedBy)
        val attention = Fixtures.decode<RequestList>("requests/attention.json")
        assertEquals(listOf("Kit", "Sam"), attention.people.map { it.name })
        assertEquals(ArrService.Radarr, attention.requests.last().sentTo?.service)

        val manager = Fixtures.decode<RequestSummary>("requests/summary_manager.json")
        assertEquals(3, manager.badge)
        assertTrue(manager.badgeAlarm)
        assertEquals(RequestAccess.Auto, manager.access)
        assertEquals(1, Fixtures.decode<RequestSummary>("requests/summary.json").badge)

        val options = Fixtures.decode<RequestOptions>("requests/options.json")
        assertEquals("/anime", options.of(TitleKind.Show)?.animeRootFolder)
        assertEquals(4L, options.of(TitleKind.Movie)?.qualityProfile)
        assertEquals(listOf(2), Fixtures.decode<ItemRequest>("requests/item_request.json").askable)

        assertEquals(1763992000L, Fixtures.decode<ApiErrorBody>("errors/declined.json").retryAt)
        assertEquals(RequestStatus.Approved, Fixtures.decode<ApiErrorBody>("errors/conflict_request.json").request?.status)
        assertEquals(true, Fixtures.decode<ApiErrorBody>("errors/tmdb_down.json").setup)
        val available = Fixtures.decode<ServerEvent>("events/request_available.json") as ServerEvent.RequestAvailable
        assertEquals(listOf(1, 2), available.seasons)
        assertFalse(available.complete)
        assertEquals(ServerEvent.RequestsSettingsChanged, Fixtures.decode<ServerEvent>("events/requests_settings_changed.json"))
    }

    @Test
    fun theHouseholdSaysWhatSwitchingTakes() {
        val profiles = Fixtures.decode<List<AppProfile>>("profiles/household.json")
        assertEquals(2L, profiles.single { it.current }.id)
        assertEquals(
            listOf(ProfileNeeds.None, ProfileNeeds.Pin, ProfileNeeds.None, ProfileNeeds.Password, ProfileNeeds.Locked),
            profiles.map { it.needs },
        )
    }

    @Test
    fun listingsAndRows() {
        val listing = Fixtures.decode<ItemsPage>("libraries/items.json")
        assertEquals(412, listing.total)
        // Every listing carries what the files are like, whatever its sort.
        assertTrue(listing.items.all { it.media?.dynamicRange != null })
        val byBitrate = Fixtures.decode<ItemsPage>("libraries/items_by_bitrate.json")
        assertTrue(byBitrate.items.all { it.media?.bitrate != null })
        val filters = Fixtures.decode<LibraryFilters>("libraries/filters.json")
        assertEquals(listOf(ResolutionFilter.Uhd, ResolutionFilter.FullHd), filters.resolutionChoices)
        assertEquals(
            listOf(DynamicRangeFilter.Hdr10, DynamicRangeFilter.DolbyVision, DynamicRangeFilter.Sdr),
            filters.dynamicRangeChoices,
        )
        // An older server's answer has neither, and a token this app doesn't know is left out.
        assertEquals(LibraryFilters(listOf("Drama"), listOf(1999)), PleyJson.decodeFromString(LibraryFilters.serializer(), """{"genres":["Drama"],"years":[1999]}"""))
        assertEquals(
            listOf(ResolutionFilter.Sd),
            PleyJson.decodeFromString(LibraryFilters.serializer(), """{"resolutions":["8k","sd"]}""").resolutionChoices,
        )
        val home = Fixtures.decode<Hubs>("hubs/home.json").hubs
        assertNull(home.first().libraryId)
        assertEquals("continue_watching", home.first().key)
        assertNull(Fixtures.decode<MaybeItem>("items/next_none.json").item)
        assertEquals(204L, Fixtures.decode<MaybeItem>("items/on_deck.json").item?.id)
        assertEquals(listOf(CollectionSort.Release, CollectionSort.Title, CollectionSort.Custom), Fixtures.decode<CollectionList>("libraries/collections.json").collections.map { it.sort })
        val smart = Fixtures.decode<PlaylistDetail>("playlists/smart_detail.json")
        assertTrue("a smart playlist's entries are its items", smart.entries.all { it.entryId == it.item.id })
        assertEquals(6, Fixtures.decode<PersonDetail>("people/detail.json").credits.first().roles.first().episodes)
        assertEquals(3, Fixtures.decode<HistoryPage>("history/page.json").entries.size)
    }

    @Test
    fun playbackAnswers() {
        val direct = Fixtures.decode<PlaybackInfo>("playback/info_direct.json")
        assertEquals(PlaybackMethod.Direct, direct.method)
        assertFalse(direct.hls)
        val transcode = Fixtures.decode<PlaybackInfo>("playback/info_transcode.json")
        assertTrue(transcode.hls && transcode.burnSubtitles)
        assertNull(transcode.subtitles.first().url)
        val status = Fixtures.decode<SessionStatus>("playback/status_transcode.json").transcode!!
        assertEquals(JobState.Running, status.state)
        assertNull(status.preset)
        assertEquals(15_192_000L, status.bitrateOut)
        assertNull(Fixtures.decode<SessionStatus>("playback/status_direct.json").transcode)
        assertEquals(ProgressState.Buffering, Fixtures.decode<ProgressReport>("playback/progress_request.json").state)
        assertEquals(699, Fixtures.decode<TrickplayInfo>("trickplay/info.json").count)
    }

    @Test
    fun eventsRead() {
        assertEquals(ServerEvent.ScanProgress(1, ScanPhase.Probing, 118, 412), Fixtures.decode<ServerEvent>("events/scan_progress.json"))
        assertEquals(ServerEvent.LibrariesChanged, Fixtures.decode<ServerEvent>("events/libraries_changed.json"))
        assertEquals(ServerEvent.LibraryChanged(2), ServerEvent.parse(Fixtures.text("events/library_changed.json")))
    }

    @Test
    fun capsFixturesWriteBackAsTheyCame() {
        for (path in listOf("caps/v1.json", "caps/v2.json")) {
            val caps = Fixtures.decode<ClientCaps>(path)
            assertEquals(path, PleyJson.parseToJsonElement(Fixtures.text(path)), PleyJson.encodeToJsonElement(ClientCaps.serializer(), caps))
        }
        val v2 = Fixtures.decode<ClientCaps>("caps/v2.json")
        assertEquals(ClientCaps.VERSION_2, v2.capsVersion)
        assertEquals(DeviceForm.Tv, v2.device?.form)
        assertNull(Fixtures.decode<ClientCaps>("caps/v1.json").capsVersion)
    }

    @Test
    fun requestBodiesReadBackTheSame() {
        fun <T> check(path: String, serializer: KSerializer<T>) {
            val body = Fixtures.decode(path, serializer)
            assertEquals(path, body, PleyJson.decodeFromString(serializer, PleyJson.encodeToString(serializer, body)))
        }
        check("playback/request.json", PlaybackRequest.serializer())
        check("playback/progress_request.json", ProgressReport.serializer())
        check("playback/streams_request.json", StreamChoice.serializer())
        check("playback/rating_request.json", RatingRequest.serializer())
        check("playlists/create_request.json", NewPlaylist.serializer())
        check("playlists/create_smart_request.json", NewPlaylist.serializer())
        check("playlists/patch_request.json", PlaylistPatch.serializer())
        check("playlists/add_items_request.json", ItemIds.serializer())
        check("playlists/order_request.json", EntryIds.serializer())
        check("me/track_defaults.json", TrackDefaults.serializer())
        check("live/watch_request.json", WatchRequest.serializer())
    }

    @Test
    fun liveTvAnswers() {
        val channels = Fixtures.decode<List<LiveChannel>>("live/channels.json")
        assertEquals(listOf("101", "102", "105"), channels.map { it.number })
        val news = channels[0]
        assertEquals("S1 E7", news.now?.episodeLabel)
        // Halfway through the briefing, then into the next programme before the list is asked again.
        assertEquals("Morning Briefing", news.onAt(1761751800)?.title)
        assertEquals(0.5f, news.now!!.progressAt(1761751800), 0.001f)
        assertEquals("World at One", news.nextAt(1761751800)?.title)
        assertEquals("World at One", news.onAt(1761754000)?.title)
        assertNull(news.nextAt(1761754000))
        assertNull(channels[2].onAt(1761751800))
        val guide = Fixtures.decode<LiveGuide>("live/guide.json")
        assertEquals(setOf(1L, 2L), guide.programs.keys)
        assertTrue(guide.programs.getValue(2).single().isLive)
        assertEquals(listOf(2L, 1L), Fixtures.decode<LiveSummary>("live/summary.json").recent)
    }
}
