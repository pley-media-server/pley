package app.pley.android.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PleyJsonTest {
    private val summary = """
        {"id":101,"library_id":1,"kind":"movie","title":"Arrival","sort_title":"Arrival","added_at":1761300000,
         "user_state":{"position_ms":0,"watched":false,"play_count":0}}
    """.trimIndent()

    @Test
    fun unknownKeysAreSkipped() {
        val text = summary.replace("\"title\"", "\"aspect_ratio\":2.39,\"new_thing\":{\"a\":[1,2]},\"title\"")
        val item = PleyJson.decodeFromString(ItemSummary.serializer(), text)
        assertEquals("Arrival", item.title)
    }

    @Test
    fun omittedOptionalsTakeTheirDefaults() {
        val item = PleyJson.decodeFromString(ItemSummary.serializer(), """{"id":1,"library_id":2,"kind":"show","title":"Severance"}""")
        assertNull(item.year)
        assertNull(item.posterUrl)
        assertNull(item.media)
        assertEquals("", item.sortTitle)
        assertEquals(UserState(), item.userState)
        assertEquals(0L, item.addedAt)

        val detail = PleyJson.decodeFromString(ItemDetail.serializer(), """{"id":1,"library_id":2,"kind":"show","title":"Severance"}""")
        assertTrue(detail.files.isEmpty() && detail.cast.isEmpty() && detail.lockedFields.isEmpty())
    }

    @Test
    fun explicitNullsReadAsNullOrTheDefault() {
        val text = """
            {"id":1,"library_id":2,"kind":"movie","title":"Arrival","sort_title":null,"year":null,"user_state":null,
             "genres":null,"files":null,"original_title":null,"media":null}
        """.trimIndent()
        val detail = PleyJson.decodeFromString(ItemDetail.serializer(), text)
        assertNull(detail.year)
        assertNull(detail.originalTitle)
        assertEquals("", detail.sortTitle)
        assertEquals(UserState(), detail.userState)
        assertTrue(detail.genres.isEmpty())
        assertTrue(detail.files.isEmpty())
    }

    @Test
    fun unknownEnumNamesReadAsUnknown() {
        val item = PleyJson.decodeFromString(
            ItemSummary.serializer(),
            summary.replace("\"movie\"", "\"podcast\"").replace("\"added_at\"", "\"extra_type\":\"bloopers\",\"added_at\""),
        )
        assertEquals(ItemKind.Unknown, item.kind)
        assertEquals(ExtraType.Unknown, item.extraType)

        val stream = PleyJson.decodeFromString(
            MediaStream.serializer(),
            """{"index":0,"kind":"data","codec":"bin_data","dynamic_range":"hdr_vivid"}""",
        )
        assertEquals(StreamKind.Unknown, stream.kind)
        assertEquals(DynamicRange.Unknown, stream.dynamicRange)
    }

    @Test
    fun theExtraKindIsKnown() {
        val item = PleyJson.decodeFromString(
            ItemSummary.serializer(),
            """{"id":102,"library_id":1,"kind":"extra","title":"Trailer","parent_id":101,"extra_type":"behind_the_scenes"}""",
        )
        assertEquals(ItemKind.Extra, item.kind)
        assertTrue(item.kind.playable)
        assertEquals(ExtraType.BehindTheScenes, item.extraType)
    }

    @Test
    fun everyEnumReadsBackWhatItWrites() {
        fun <E> check(serializer: KSerializer<E>, values: List<E>) where E : Enum<E>, E : WireEnum {
            for (value in values) {
                val text = PleyJson.encodeToString(serializer, value)
                assertEquals("\"${value.wire}\"", text)
                assertEquals(value, PleyJson.decodeFromString(serializer, text))
            }
        }
        check(ItemKind.Serializer, ItemKind.entries)
        check(ExtraType.Serializer, ExtraType.entries)
        check(DynamicRange.Serializer, DynamicRange.entries)
        check(LibraryKind.Serializer, LibraryKind.entries)
        check(StreamKind.Serializer, StreamKind.entries)
        check(MarkerKind.Serializer, MarkerKind.entries)
        check(MarkerSource.Serializer, MarkerSource.entries)
        check(CreditRole.Serializer, CreditRole.entries)
        check(CollectionSort.Serializer, CollectionSort.entries)
        check(PlaylistKind.Serializer, PlaylistKind.entries)
        check(PlaybackMode.Serializer, PlaybackMode.entries)
        check(PlaybackMethod.Serializer, PlaybackMethod.entries)
        check(JobState.Serializer, JobState.entries)
        check(StreamAction.Serializer, StreamAction.entries)
        check(ProgressState.Serializer, ProgressState.entries)
        check(SubtitleMode.Serializer, SubtitleMode.entries)
        check(ProfileNeeds.Serializer, ProfileNeeds.entries)
        check(ScanPhase.Serializer, ScanPhase.entries)
    }

    @Test
    fun namesAreTheServersSnakeCase() {
        val stream = MediaStream(
            index = 0, kind = StreamKind.Video, codec = "hevc", pixFmt = "yuv420p10le", dvBlCompat = 1,
            dvLevel = 6, colorTransfer = "smpte2084", bitDepth = 10, frameRate = 23.976,
        )
        val keys = PleyJson.encodeToJsonElement(MediaStream.serializer(), stream).jsonObject.keys
        for (key in listOf("pix_fmt", "dv_bl_compat", "dv_level", "color_transfer", "bit_depth", "frame_rate")) {
            assertTrue("$key in $keys", key in keys)
        }
    }

    @Test
    fun aRequestBodyReadsBackTheSame() {
        val request = PlaybackRequest(
            itemId = 203,
            fileId = 601,
            audioIndex = 1,
            maxBitrate = 8000,
            maxHeight = 1080,
            startMs = 754_000,
            mode = PlaybackMode.Remux,
            clientId = "android-3f2a",
            caps = app.pley.android.model.caps.ClientCaps(containers = listOf("mp4"), videoCodecs = listOf("h264")),
        )
        val text = PleyJson.encodeToString(PlaybackRequest.serializer(), request)
        assertEquals(request, PleyJson.decodeFromString(PlaybackRequest.serializer(), text))

        val body = PleyJson.parseToJsonElement(text).jsonObject
        assertEquals(
            setOf("item_id", "file_id", "audio_index", "max_bitrate", "max_height", "start_ms", "mode", "client_id", "caps"),
            body.keys,
        )
        assertEquals("\"remux\"", body["mode"].toString())
        // Asked for only beside a session playing: left out otherwise.
        val beside = PleyJson.parseToJsonElement(PleyJson.encodeToString(PlaybackRequest.serializer(), request.copy(keepPrevious = true))).jsonObject
        assertEquals("true", beside["keep_previous"].toString())
    }

    @Test
    fun nullsAreLeftOutOfBodiesAndDefaultsKept() {
        val choice = PleyJson.encodeToString(StreamChoice.serializer(), StreamChoice(audioIndex = 2, subtitleIndex = null))
        assertEquals("""{"audio_index":2}""", choice)
        val automatic = PleyJson.encodeToString(StreamChoice.serializer(), StreamChoice(fileId = 3, subtitleIndex = 4, automaticAudio = true))
        assertEquals("""{"file_id":3,"subtitle_index":4,"audio_index":null}""", automatic)
        assertEquals(StreamChoice(fileId = 3, subtitleIndex = 4, automaticAudio = true), PleyJson.decodeFromString(StreamChoice.serializer(), automatic))
        assertEquals(StreamChoice(audioIndex = 2), PleyJson.decodeFromString(StreamChoice.serializer(), choice))
        val progress = PleyJson.encodeToString(ProgressReport.serializer(), ProgressReport(positionMs = 5000, sessionId = "s1"))
        assertEquals("""{"position_ms":5000,"state":"playing","session_id":"s1"}""", progress)
        assertEquals("""{}""", PleyJson.encodeToString(RatingRequest.serializer(), RatingRequest(null)))
    }

    @Test
    fun meSplitsItsScopes() {
        val me = PleyJson.decodeFromString(Me.serializer(), """{"id":2,"username":"alex","name":"Alex","scope":"library play"}""")
        assertEquals(listOf("library", "play"), me.scopes)
        assertFalse(me.isAdmin)
        assertEquals(TrackDefaults(), me.trackDefaults)
        assertEquals(emptyList<String>(), me.copy(scope = null).scopes)
    }

    @Test
    fun theServerSaysWhetherItRequiresTwoStepSignIn() {
        fun required(field: String) =
            PleyJson.decodeFromString(ServerInfo.serializer(), """{"name":"Den","version":"1.0.0"$field}""").twoStepRequired
        assertTrue(required(""","two_step_required":true"""))
        assertFalse(required(""","two_step_required":false"""))
        // A server from before it says nothing of it.
        assertFalse(required(""))
        // Anything but true is no, and the rest of the answer still reads.
        for (odd in listOf("null", "\"yes\"", "\"true\"", "1", "{}", "[true]")) {
            assertFalse(odd, required(""","two_step_required":$odd"""))
        }
        assertEquals(
            ServerInfo("Den", "1.0.0"),
            PleyJson.decodeFromString(ServerInfo.serializer(), """{"name":"Den","version":"1.0.0","two_step_required":"yes"}"""),
        )
        // Written as the server writes it (the fixture server's answer).
        assertEquals(
            """{"name":"Den","version":"1.0.0","setup_required":false,"setup_code_required":false,"two_step_required":true}""",
            PleyJson.encodeToString(ServerInfo.serializer(), ServerInfo("Den", "1.0.0", twoStepRequired = true)),
        )
    }

    @Test
    fun errorBodiesRead() {
        val error = PleyJson.decodeFromString(ApiErrorBody.serializer(), """{"code":"not_found","message":"item 42 not found"}""")
        assertEquals("not_found", error.code)
        assertEquals(ApiErrorBody(), PleyJson.decodeFromString(ApiErrorBody.serializer(), "{}"))
    }

    @Test
    fun trickplayFindsThumbnails() {
        val info = TrickplayInfo(id = "a1", intervalMs = 10_000, width = 320, height = 180, columns = 10, rows = 10, count = 250, sheets = 3)
        assertEquals(0, info.thumbnailAt(-5))
        assertEquals(123, info.thumbnailAt(1_235_000))
        assertEquals(249, info.thumbnailAt(99_000_000))
        assertEquals(1, info.sheetOf(123))
        assertEquals(3 to 2, info.cellOf(123))
    }

    @Test
    fun detailsGiveTheirSummaries() {
        val detail = ItemDetail(id = 1, libraryId = 2, kind = ItemKind.Movie, title = "Arrival", year = 2016, genres = listOf("Drama"))
        assertEquals(ItemSummary(id = 1, libraryId = 2, kind = ItemKind.Movie, title = "Arrival", year = 2016), detail.toSummary())
        val asJson = PleyJson.encodeToJsonElement(ItemSummary.serializer(), detail.toSummary()) as JsonObject
        assertFalse("genres" in asJson)
    }
}
